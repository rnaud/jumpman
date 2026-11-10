//! Mario vs. God (`god.ts`): play normally while pieces reshape the world around him. Every
//! piece is real collision, rebuilt the moment it changes, with undo/redo.
//!
//! Mouse-driven like the TS: 1 Grab (drag anything; Shift lifts; wheel resizes; right-click
//! deletes), 2 Bridge (drag to paint planks), 3 Block, 4 Ramp, 5 Spring, 6 Bob-omb, 7 Coin,
//! 8 Portal (two linked portals that keep speed). Every dock button is also a key.
//! `Z` undo  `Y` redo  `G` gravity  `T` slow motion  `F9` reset
use super::geometry::{add_box_collision, add_ramp_collision};
use super::dock::DockItem;
use super::{Experiment, ExperimentId, Host};
use bevy::math::DVec3;
use jumpman_sim::object::{ObjId, ObjectBehaviorKind, ObjectSpec};
use crate::render::{to_bevy, SCALE};
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::collision::{Surface, SurfaceOptions};
use jumpman_sim::math::{angle_to_radians, Vec3 as V3};
use jumpman_sim::state::*;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceKind {
    Plank,
    Block,
    Ramp,
    Spring,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieceSpec {
    pub kind: PieceKind,
    pub center: V3,
    pub size: V3,
    pub yaw: f64,
}

struct Piece {
    id: u32,
    spec: PieceSpec,
    entity: Entity,
    surfaces: HashSet<usize>,
    pop: f64,
}

const GRAVITY: [f64; 4] = [0.35, 0.7, 1.0, 1.8];

/// Undo/redo history of piece layouts.
#[derive(Default)]
pub struct History {
    undo: Vec<Vec<PieceSpec>>,
    redo: Vec<Vec<PieceSpec>>,
}

impl History {
    pub fn record(&mut self, current: Vec<PieceSpec>) {
        self.undo.push(current);
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, current: Vec<PieceSpec>) -> Option<Vec<PieceSpec>> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }
    pub fn redo(&mut self, current: Vec<PieceSpec>) -> Option<Vec<PieceSpec>> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}

pub struct GodModeExperiment {
    pieces: Vec<Piece>,
    next_id: u32,
    history: History,
    slow_mo: bool,
    gravity_index: usize,
    bounces: u32,
    materials: Vec<(PieceKind, Handle<StandardMaterial>)>,
    tool: usize,
    drag: Option<Drag>,
    stroke: Option<Stroke>,
    hovered: Option<Target>,
    highlight: Option<Entity>,
    portals: [Option<Portal>; 2],
    next_portal: usize,
    portal_cooldown: u32,
    warps: u32,

    spawned_pickups: Vec<usize>,
    spawned_objects: Vec<ObjId>,
    /// Goal: the star on the highest block of the floating staircase.
    goal: Option<(V3, Entity)>,
    ticks: u32,
    /// Ticks and piece count when the star was collected.
    won: Option<(u32, usize)>,
}

pub const TOOLS: [(&str, &str); 8] = [
    ("Grab", "DRAG ANYTHING - SHIFT LIFTS - WHEEL RESIZES - RIGHT-CLICK DELETES"),
    ("Bridge", "DRAG TO PAINT PLANKS AT MARIO'S HEIGHT"),
    ("Block", "CLICK TO DROP A STONE BLOCK"),
    ("Ramp", "CLICK TO PLACE A RAMP FACING AWAY FROM MARIO"),
    ("Spring", "CLICK TO PLACE A SPRING PAD"),
    ("Bomb", "CLICK TO SPAWN A BOB-OMB"),
    ("Coin", "CLICK TO SPAWN A COIN"),
    ("Portal", "CLICK ANY WALL OR FLOOR: TWO LINKED PORTALS THAT KEEP MARIO'S SPEED"),
];
const TOOL_KEYS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Piece(u32),
    Pickup(usize),
    Object(usize),
}

struct Drag {
    target: Target,
    vertical: bool,
    offset: DVec3,
    last: DVec3,
    /// Plane normal and a point on it, SM64 units.
    plane: (DVec3, DVec3),
}

struct Stroke {
    height: f64,
    last: Option<DVec3>,
    count: u32,
}

struct Portal {
    center: DVec3,
    normal: DVec3,
    entity: Entity,
}

/// Slab test against a yawed box; returns the entry distance.
fn ray_box(o: DVec3, d: DVec3, center: V3, size: V3, yaw: f64) -> Option<f64> {
    let (c, s) = (yaw.cos(), yaw.sin());
    // Rotate into the box frame (inverse of add_box_collision's corner mapping).
    let rel = o - DVec3::from(center);
    let to_local = |v: DVec3| DVec3::new(v.x * c - v.z * s, v.y, v.x * s + v.z * c);
    let (lo, ld) = (to_local(rel), to_local(d));
    let half = DVec3::from(size) / 2.0;
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for (oo, dd, h) in [(lo.x, ld.x, half.x), (lo.y, ld.y, half.y), (lo.z, ld.z, half.z)] {
        if dd.abs() < 1e-9 {
            if oo.abs() > h {
                return None;
            }
        } else {
            let (a, b) = ((-h - oo) / dd, (h - oo) / dd);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    (t0 <= t1).then_some(t0)
}

fn ray_sphere(o: DVec3, d: DVec3, c: DVec3, r: f64) -> Option<f64> {
    let oc = o - c;
    let b = oc.dot(d);
    let disc = b * b - (oc.dot(oc) - r * r);
    (disc >= 0.0).then(|| (-b - disc.sqrt()).max(0.0))
}

/// Nearest collision triangle along the ray: distance and its normal.
fn ray_world(world: &jumpman_sim::collision::CollisionWorld, o: DVec3, d: DVec3, max: f64) -> Option<(f64, DVec3)> {
    let mut best: Option<(f64, DVec3)> = None;
    for s in world.surfaces() {
        if s.kind == jumpman_sim::surfaces::SURFACE_DEATH_PLANE {
            continue;
        }
        let a = DVec3::from(s.v1);
        let (e1, e2) = (DVec3::from(s.v2) - a, DVec3::from(s.v3) - a);
        let p = d.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-9 {
            continue;
        }
        let inv = 1.0 / det;
        let tv = o - a;
        let u = tv.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = tv.cross(e1);
        let v = d.dot(q) * inv;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = e2.dot(q) * inv;
        if t >= 0.0 && t <= max && best.map_or(true, |b| t < b.0) {
            best = Some((t, DVec3::from(s.normal)));
        }
    }
    best
}

fn sid(s: &Arc<Surface>) -> usize {
    Arc::as_ptr(s) as usize
}

/// Wedge mesh matching `add_ramp_collision` (rises toward local -Z), in Bevy units.
pub fn ramp_mesh(size: V3) -> Mesh {
    let (hx, hy, hz) = ((size[0] / 2.0) as f32 * SCALE, (size[1] / 2.0) as f32 * SCALE, (size[2] / 2.0) as f32 * SCALE);
    let (fl, fr, bl, br, tl, tr) = ([-hx, -hy, hz], [hx, -hy, hz], [-hx, -hy, -hz], [hx, -hy, -hz], [-hx, hy, -hz], [hx, hy, -hz]);
    let tris: [[[f32; 3]; 3]; 8] = [
        [fl, fr, tr],
        [fl, tr, tl],
        [bl, br, fr],
        [bl, fr, fl],
        [br, bl, tl],
        [br, tl, tr],
        [fl, tl, bl],
        [fr, br, tr],
    ];
    let positions: Vec<[f32; 3]> = tris.iter().flatten().copied().collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.compute_flat_normals();
    mesh
}

/// The floating staircase ahead of Mario that makes the toy obvious from the first frame:
/// four blocks climbing 160 units each. Mario at `pos` on ground `base`, facing `yaw` (radians).
pub fn staircase(pos: V3, base: f64, yaw: f64) -> Vec<PieceSpec> {
    let (fx, fz) = (yaw.sin(), yaw.cos());
    [(520.0, 110.0, 300.0, -90.0), (920.0, 220.0, 300.0, 90.0), (1320.0, 330.0, 300.0, -90.0), (1760.0, 440.0, 420.0, 0.0)]
        .into_iter()
        .map(|(ahead, height, width, side)| PieceSpec { kind: PieceKind::Block, center: [pos[0] + fx * ahead + fz * side, base + height, pos[2] + fz * ahead - fx * side], size: [width, 60.0, width], yaw })
        .collect()
}

/// Where the goal star floats: just above the top of the last block.
pub fn goal_star(stairs: &[PieceSpec]) -> V3 {
    let last = stairs.last().expect("staircase has blocks");
    [last.center[0], last.center[1] + last.size[1] / 2.0 + 90.0, last.center[2]]
}

/// Mario (feet at `pos`) is touching the star at `star`.
pub fn star_reached(pos: V3, star: V3) -> bool {
    (pos[0] - star[0]).hypot(pos[2] - star[2]) < 130.0 && (pos[1] + 60.0 - star[1]).abs() < 130.0
}

impl GodModeExperiment {
    pub fn new() -> Self {
        Self {
            pieces: vec![],
            next_id: 1,
            history: History::default(),
            slow_mo: false,
            gravity_index: 2,
            bounces: 0,
            materials: vec![],
            tool: 0,
            drag: None,
            stroke: None,
            hovered: None,
            highlight: None,
            portals: [None, None],
            next_portal: 0,
            portal_cooldown: 0,
            warps: 0,

            spawned_pickups: vec![],
            spawned_objects: vec![],
            goal: None,
            ticks: 0,
            won: None,
        }
    }

    fn material(&mut self, host: &mut Host, kind: PieceKind) -> Handle<StandardMaterial> {
        if let Some((_, m)) = self.materials.iter().find(|(k, _)| *k == kind) {
            return m.clone();
        }
        let m = match kind {
            PieceKind::Plank => host.lit(Color::srgb(0.64, 0.45, 0.27)),
            PieceKind::Block => host.lit(Color::srgb(0.7, 0.68, 0.62)),
            PieceKind::Ramp => host.materials.add(StandardMaterial { base_color: Color::srgb(0.85, 0.75, 0.55), perceptual_roughness: 0.9, cull_mode: None, ..default() }),
            PieceKind::Spring => host.materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.42, 0.16), emissive: LinearRgba::rgb(0.35, 0.1, 0.0), ..default() }),
        };
        self.materials.push((kind, m.clone()));
        m
    }

    fn snapshot(&self) -> Vec<PieceSpec> {
        self.pieces.iter().map(|p| p.spec).collect()
    }

    fn create_piece(&mut self, host: &mut Host, spec: PieceSpec, pop: bool) {
        let material = self.material(host, spec.kind);
        let mesh = if spec.kind == PieceKind::Ramp { host.meshes.add(ramp_mesh(spec.size)) } else { host.meshes.add(Cuboid::new(spec.size[0] as f32 * SCALE, spec.size[1] as f32 * SCALE, spec.size[2] as f32 * SCALE)) };
        let entity = host.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(to_bevy(spec.center)).with_rotation(Quat::from_rotation_y(spec.yaw as f32))));
        let mut surfaces = vec![];
        let world = host.world_mut();
        if spec.kind == PieceKind::Ramp {
            add_ramp_collision(world, spec.center, spec.size, spec.yaw, &mut surfaces);
        } else {
            add_box_collision(world, spec.center, spec.size, spec.yaw, &mut surfaces, &SurfaceOptions::default());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.pieces.push(Piece { id, spec, entity, surfaces: surfaces.iter().map(sid).collect(), pop: if pop { 0.0 } else { 1.0 } });
    }

    fn remove_piece(&mut self, host: &mut Host, index: usize) {
        let piece = self.pieces.remove(index);
        host.world_mut().remove_surfaces(&|s| piece.surfaces.contains(&(s as *const Surface as usize)));
        host.commands.entity(piece.entity).despawn();
    }

    fn restore(&mut self, host: &mut Host, state: Vec<PieceSpec>) {
        while !self.pieces.is_empty() {
            self.remove_piece(host, 0);
        }
        for spec in state {
            self.create_piece(host, spec, false);
        }
    }

    fn set_gravity(&mut self, host: &mut Host, index: usize) {
        self.gravity_index = index;
        host.sim.mario.gravity_scale = GRAVITY[index];
        self.dock(host);
        host.toast(["MOON", "FLOATY", "NORMAL GRAVITY", "HEAVY"][index], 1.0);
    }

    /// The point on the floor ahead of Mario where a placed piece lands, and his facing.
    fn target(&self, host: &Host) -> (V3, f64) {
        let m = &host.sim.mario;
        let facing = angle_to_radians(m.face_angle[1]);
        let (x, z) = (m.pos[0] + facing.sin() * 450.0, m.pos[2] + facing.cos() * 450.0);
        let floor = m.world.find_floor(x, m.pos[1] + 300.0, z);
        ([x, if floor.surface.is_some() { floor.height } else { m.pos[1] }, z], facing)
    }

    fn dock(&self, host: &mut Host) {
        let touch = host.touch;
        let mut items: Vec<DockItem> = TOOLS
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                // Phones: short chips in two rows (the dock sits between the stick and the A/B grid).
                let group = if touch { (i / 6) as u8 } else { 0 };
                let mut it = DockItem::new(&if touch { (*name).to_string() } else { format!("{} {}", i + 1, name) }, TOOL_KEYS[i], group);
                it.selected = i == self.tool;
                it
            })
            .collect();
        if touch {
            // Second row: the rest of the tools, Place, Undo and Reset (G / T / Y stay on the keyboard).
            items.push(DockItem::new("Place", KeyCode::Enter, 1).essential().primary());
            items.push(DockItem::new("Undo", KeyCode::KeyZ, 1).essential());
            items.push(DockItem::new("Reset", KeyCode::F9, 1));
            host.set_dock(items);
            return;
        }
        items.push(DockItem::new(&format!("Grav x{}", GRAVITY[self.gravity_index]), KeyCode::KeyG, 1));
        let mut slow = DockItem::new("Slow", KeyCode::KeyT, 1);
        slow.selected = self.slow_mo;
        items.push(slow);
        items.push(DockItem::new("Place", KeyCode::Enter, 3).primary());
        items.push(DockItem::new("Undo", KeyCode::KeyZ, 2));
        items.push(DockItem::new("Redo", KeyCode::KeyY, 2));
        items.push(DockItem::new("Reset", KeyCode::F9, 2));
        host.set_dock(items);
    }

    fn set_tool(&mut self, host: &mut Host, tool: usize) {
        self.tool = tool;
        host.toast(TOOLS[tool].1, 1.6);
        self.dock(host);
    }

    fn piece_index(&self, id: u32) -> Option<usize> {
        self.pieces.iter().position(|p| p.id == id)
    }

    fn target_pos(&self, host: &Host, t: Target) -> Option<DVec3> {
        Some(match t {
            Target::Piece(id) => DVec3::from(self.pieces[self.piece_index(id)?].spec.center),
            Target::Pickup(i) => DVec3::from(host.sim.pickups.get(i)?.pos),
            Target::Object(i) => DVec3::from(host.sim.mario.objects.objects.get(i)?.pos),
        })
    }

    /// Nearest grabbable under the ray.
    fn pick_target(&self, host: &Host, o: DVec3, d: DVec3) -> Option<Target> {
        let mut best: Option<(f64, Target)> = None;
        let mut consider = |t: f64, target: Target| {
            if best.map_or(true, |b| t < b.0) {
                best = Some((t, target));
            }
        };
        for piece in &self.pieces {
            if let Some(t) = ray_box(o, d, piece.spec.center, piece.spec.size, piece.spec.yaw) {
                consider(t, Target::Piece(piece.id));
            }
        }
        for (i, pk) in host.sim.pickups.iter().enumerate() {
            if pk.collected {
                continue;
            }
            if let Some(t) = ray_sphere(o, d, DVec3::from(pk.pos), 45.0) {
                consider(t, Target::Pickup(i));
            }
        }
        for (i, obj) in host.sim.mario.objects.objects.iter().enumerate() {
            if !obj.active {
                continue;
            }
            if let Some(t) = ray_sphere(o, d, DVec3::from(obj.pos) + DVec3::Y * 45.0, 75.0) {
                consider(t, Target::Object(i));
            }
        }
        best.map(|b| b.1)
    }

    fn move_target(&mut self, host: &mut Host, t: Target, to: DVec3) {
        match t {
            Target::Piece(id) => {
                let Some(i) = self.piece_index(id) else { return };
                let delta = to - DVec3::from(self.pieces[i].spec.center);
                if delta.length_squared() < 0.01 {
                    return;
                }
                let m = &host.sim.mario;
                // A platform carries Mario when it moves under his feet.
                let riding = m.floor.as_ref().is_some_and(|f| self.pieces[i].surfaces.contains(&sid(f))) && m.action & ACT_FLAG_AIR == 0 && m.pos[1] <= m.floor_height + 4.0;
                let mut spec = self.pieces[i].spec;
                spec.center = to.into();
                self.replace_piece(host, i, spec);
                if riding {
                    host.shift_mario(delta.into());
                }
            }
            Target::Pickup(i) => {
                if let Some(p) = host.sim.pickups.get_mut(i) {
                    p.pos = to.into();
                }
            }
            Target::Object(i) => {
                if let Some(o) = host.sim.mario.objects.objects.get_mut(i) {
                    o.pos = to.into();
                    o.vel = [0.0; 3];
                }
            }
        }
    }

    /// Rebuild a piece's mesh + collision from a new spec, keeping its id and slot.
    fn replace_piece(&mut self, host: &mut Host, index: usize, spec: PieceSpec) {
        let id = self.pieces[index].id;
        let pop = self.pieces[index].pop;
        self.remove_piece(host, index);
        self.create_piece(host, spec, false);
        let last = self.pieces.pop().unwrap();
        self.pieces.insert(index, Piece { id, pop, ..last });
    }

    fn resize_target(&mut self, host: &mut Host, t: Target, factor: f64) {
        // Coins and actors keep their sim size.
        let Target::Piece(id) = t else { return };
        let Some(i) = self.piece_index(id) else { return };
        let mut spec = self.pieces[i].spec;
        let mut next = [spec.size[0] * factor, spec.size[1] * factor, spec.size[2] * factor];
        if next[0].max(next[2]) > 2400.0 || next[0].min(next[2]) < 60.0 {
            return;
        }
        if matches!(spec.kind, PieceKind::Plank | PieceKind::Spring) {
            next[1] = spec.size[1];
        }
        self.history.record(self.snapshot());
        spec.size = next;
        self.replace_piece(host, i, spec);
    }

    fn delete_target(&mut self, host: &mut Host, t: Target) {
        match t {
            Target::Piece(id) => {
                if let Some(i) = self.piece_index(id) {
                    self.history.record(self.snapshot());
                    host.fx("break_box", self.pieces[i].spec.center);
                    self.remove_piece(host, i);
                }
            }
            Target::Pickup(i) => {
                if let Some(p) = host.sim.pickups.get_mut(i) {
                    p.collected = true;
                }
            }
            Target::Object(i) => {
                if let Some(o) = host.sim.mario.objects.objects.get_mut(i) {
                    o.active = false;
                }
            }
        }
        if self.hovered == Some(t) {
            self.hovered = None;
        }
    }

    /// Intersection of the ray with the plane (normal n through p).
    fn ray_plane(o: DVec3, d: DVec3, n: DVec3, p: DVec3) -> Option<DVec3> {
        let denom = n.dot(d);
        if denom.abs() < 1e-6 {
            return None;
        }
        let t = (p - o).dot(n) / denom;
        (t > 0.0).then(|| o + d * t)
    }

    fn pointer(&mut self, host: &mut Host) {
        let mouse = host.mouse;
        let ray = if mouse.over_ui { None } else { mouse.ray() };
        let shift = host.keys.pressed(KeyCode::ShiftLeft) || host.keys.pressed(KeyCode::ShiftRight);
        if self.tool == 0 && self.drag.is_none() && self.stroke.is_none() {
            self.hovered = ray.and_then(|(o, d)| self.pick_target(host, o, d));
        } else if self.drag.is_none() {
            self.hovered = None;
        }
        if mouse.right_just_pressed {
            if let Some((o, d)) = ray {
                if let Some(t) = self.pick_target(host, o, d) {
                    self.delete_target(host, t);
                }
            }
        }
        if mouse.scroll != 0.0 {
            if let Some(t) = self.hovered {
                self.resize_target(host, t, if mouse.scroll > 0.0 { 1.12 } else { 1.0 / 1.12 });
            }
        }
        if mouse.left_just_pressed {
            if let Some((o, d)) = ray {
                self.pointer_down(host, o, d, shift);
            }
        }
        if mouse.left {
            if let Some((o, d)) = mouse.ray() {
                self.pointer_move(host, o, d);
            }
        }
        if mouse.left_just_released || !mouse.left {
            if self.stroke.as_ref().is_some_and(|s| s.count == 0) {
                self.history.undo.pop();
            }
            self.drag = None;
            self.stroke = None;
        }
    }

    fn pointer_down(&mut self, host: &mut Host, o: DVec3, d: DVec3, shift: bool) {
        match self.tool {
            0 => {
                let Some(target) = self.pick_target(host, o, d) else { return };
                let Some(pos) = self.target_pos(host, target) else { return };
                if matches!(target, Target::Piece(_)) {
                    self.history.record(self.snapshot());
                }
                let plane = if shift {
                    let mut n = DVec3::from(host.rig.eye) - pos;
                    n.y = 0.0;
                    (n.normalize_or(DVec3::Z), pos)
                } else {
                    (DVec3::Y, pos)
                };
                let offset = Self::ray_plane(o, d, plane.0, plane.1).map_or(DVec3::ZERO, |hit| pos - hit);
                self.drag = Some(Drag { target, vertical: shift, offset, last: pos, plane });
            }
            1 => {
                self.history.record(self.snapshot());
                let m = &host.sim.mario;
                // Plank tops sit flush with his feet (or just under him while airborne).
                let height = if m.action & ACT_FLAG_AIR != 0 { m.pos[1] - 40.0 } else { m.floor_height };
                self.stroke = Some(Stroke { height, last: None, count: 0 });
                self.extend_stroke(host, o, d);
            }
            _ => self.place_tool(host, o, d),
        }
    }

    fn pointer_move(&mut self, host: &mut Host, o: DVec3, d: DVec3) {
        if let Some(drag) = self.drag.as_ref() {
            let Some(hit) = Self::ray_plane(o, d, drag.plane.0, drag.plane.1) else { return };
            let mut next = hit + drag.offset;
            if drag.vertical {
                next = DVec3::new(drag.last.x, next.y, drag.last.z);
            } else {
                next.y = drag.last.y;
            }
            let target = drag.target;
            self.drag.as_mut().unwrap().last = next;
            self.move_target(host, target, next);
        } else if self.stroke.is_some() {
            self.extend_stroke(host, o, d);
        }
    }

    fn extend_stroke(&mut self, host: &mut Host, o: DVec3, d: DVec3) {
        let Some(stroke) = self.stroke.as_mut() else { return };
        let Some(point) = Self::ray_plane(o, d, DVec3::Y, DVec3::new(0.0, stroke.height, 0.0)) else { return };
        let Some(mut last) = stroke.last else {
            stroke.last = Some(point);
            return;
        };
        let step = 90.0;
        let mut specs = vec![];
        let mut distance = point.distance(last);
        while distance >= step {
            let dir = (point - last).normalize();
            let center = last + dir * (step / 2.0);
            specs.push(PieceSpec { kind: PieceKind::Plank, center: [center.x, center.y - 15.0, center.z], size: [280.0, 30.0, 104.0], yaw: dir.x.atan2(dir.z) });
            last += dir * step;
            distance -= step;
        }
        stroke.last = Some(last);
        stroke.count += specs.len() as u32;
        if stroke.count % 4 == 1 {
            host.fx("footstep", [point.x, point.y, point.z]);
        }
        for spec in specs {
            self.create_piece(host, spec, true);
        }
    }

    /// Terrain point + normal under the ray (collision includes the placed pieces).
    fn terrain_hit(&self, host: &Host, o: DVec3, d: DVec3) -> Option<(DVec3, DVec3)> {
        let (t, mut n) = ray_world(&host.sim.mario.world, o, d, 60000.0)?;
        if n.dot(d) > 0.0 {
            n = -n;
        }
        Some((o + d * t, n))
    }

    fn place_tool(&mut self, host: &mut Host, o: DVec3, d: DVec3) {
        let Some((point, normal)) = self.terrain_hit(host, o, d) else {
            host.toast("CLICK THE GROUND OR A PLATFORM", 1.0);
            return;
        };
        let m = host.sim.mario.pos;
        let facing = (point.x - m[0]).atan2(point.z - m[2]);
        let p = [point.x, point.y, point.z];
        match self.tool {
            2 => {
                self.history.record(self.snapshot());
                self.create_piece(host, PieceSpec { kind: PieceKind::Block, center: [p[0], p[1] + 120.0, p[2]], size: [240.0; 3], yaw: facing }, true);
            }
            3 => {
                self.history.record(self.snapshot());
                self.create_piece(host, PieceSpec { kind: PieceKind::Ramp, center: [p[0], p[1] + 82.0, p[2]], size: [320.0, 160.0, 520.0], yaw: facing + std::f64::consts::PI }, true);
            }
            4 => {
                self.history.record(self.snapshot());
                self.create_piece(host, PieceSpec { kind: PieceKind::Spring, center: [p[0], p[1] + 22.0, p[2]], size: [170.0, 44.0, 170.0], yaw: 0.0 }, true);
            }
            5 => {
                let mut spec = ObjectSpec::new(ObjectBehaviorKind::Bobomb, [p[0], p[1] + 60.0, p[2]], 0.0);
                spec.model = Some("actors/bobomb/black_bobomb_geo.glb".into());
                let id = host.sim.mario.objects.spawn(&spec);
                self.spawned_objects.push(id);
            }
            6 => {
                host.sim.pickups.push(crate::game::runner::Pickup { kind: crate::game::runner::PickupKind::Coin, pos: [p[0], p[1] + 80.0, p[2]], value: 1, collected: false, loot: None });
                self.spawned_pickups.push(host.sim.pickups.len() - 1);
            }
            7 => self.place_portal(host, point, normal),
            _ => {}
        }
    }

    /// Place a portal flush on the clicked surface, facing out of it.
    fn place_portal(&mut self, host: &mut Host, point: DVec3, normal: DVec3) {
        let slot = self.next_portal;
        self.next_portal = 1 - slot;
        if let Some(old) = self.portals[slot].take() {
            host.commands.entity(old.entity).despawn();
        }
        let color = if slot == 0 { Color::srgb(0.23, 0.66, 1.0) } else { Color::srgb(1.0, 0.54, 0.12) };
        let ring = host.meshes.add(Torus::new(105.0 * SCALE, 135.0 * SCALE));
        let disc = host.meshes.add(Circle::new(112.0 * SCALE));
        let ring_mat = host.unlit(color);
        let disc_mat = host.materials.add(StandardMaterial { base_color: color.with_alpha(0.45), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
        let center = point + normal * 4.0;
        let n = Vec3::new(normal.x as f32, normal.y as f32, normal.z as f32);
        let entity = host.spawn((Transform::from_translation(to_bevy(center.into())).with_rotation(Quat::from_rotation_arc(Vec3::Z, n)), Visibility::Inherited));
        host.commands.entity(entity).with_children(|c| {
            c.spawn((Mesh3d(ring), MeshMaterial3d(ring_mat), Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))));
            c.spawn((Mesh3d(disc), MeshMaterial3d(disc_mat)));
        });
        self.portals[slot] = Some(Portal { center, normal, entity });
        host.toast(if slot == 0 { "BLUE PORTAL" } else { "ORANGE PORTAL" }, 0.9);
    }

    /// Enter one portal, leave the other at the same speed, flung out along its face.
    fn step_portals(&mut self, host: &mut Host) {
        if self.portal_cooldown > 0 {
            self.portal_cooldown -= 1;
            return;
        }
        let (Some(a), Some(b)) = (&self.portals[0], &self.portals[1]) else { return };
        let m = &host.sim.mario;
        let body = DVec3::new(m.pos[0], m.pos[1] + 80.0, m.pos[2]);
        for (from, to) in [(a, b), (b, a)] {
            let mut offset = body - from.center;
            let depth = offset.dot(from.normal);
            offset -= from.normal * depth;
            if offset.length() > 115.0 || depth > 110.0 || depth < -60.0 {
                continue;
            }
            let yaw = angle_to_radians(m.face_angle[1]);
            let velocity = DVec3::new(yaw.sin() * m.forward_vel, m.vel[1], yaw.cos() * m.forward_vel);
            let speed = velocity.length().max(28.0);
            if velocity.dot(from.normal) > 4.0 && depth > 40.0 {
                continue;
            }
            let out = to.normal;
            let exit = to.center + out * 140.0 - DVec3::Y * 80.0;
            let delta = [exit.x - m.pos[0], exit.y - m.pos[1], exit.z - m.pos[2]];
            host.shift_mario(delta);
            let m = &mut host.sim.mario;
            m.set_action(ACT_FREEFALL, 0.0);
            let horizontal = out.x.hypot(out.z);
            if horizontal > 0.3 {
                m.face_angle[1] = jumpman_sim::math::atan2s(out.z, out.x) as f64;
            }
            m.forward_vel = speed * horizontal;
            m.vel[1] = out.y * speed + if horizontal > 0.3 { 12.0 } else { 0.0 };
            self.portal_cooldown = 18;
            self.warps += 1;
            host.fx("teleport", [exit.x, exit.y, exit.z]);
            return;
        }
    }

    fn update_highlight(&mut self, host: &mut Host) {
        let target = self.drag.as_ref().map(|d| d.target).or(self.hovered);
        let (center, size) = match target {
            Some(Target::Piece(id)) => match self.piece_index(id) {
                Some(i) => (DVec3::from(self.pieces[i].spec.center), DVec3::from(self.pieces[i].spec.size) + DVec3::splat(14.0)),
                None => (DVec3::ZERO, DVec3::ZERO),
            },
            Some(t) => match self.target_pos(host, t) {
                Some(p) => (p + DVec3::Y * 45.0, DVec3::splat(130.0)),
                None => (DVec3::ZERO, DVec3::ZERO),
            },
            None => (DVec3::ZERO, DVec3::ZERO),
        };
        let visible = target.is_some() && size != DVec3::ZERO;
        let yaw = match target {
            Some(Target::Piece(id)) => self.piece_index(id).map_or(0.0, |i| self.pieces[i].spec.yaw),
            _ => 0.0,
        };
        let e = match self.highlight {
            Some(e) => e,
            None => {
                let mesh = host.meshes.add(Cuboid::new(1.0, 1.0, 1.0));
                let mat = host.materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.82, 0.29, 0.35), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden));
                self.highlight = Some(e);
                e
            }
        };
        let mut ec = host.commands.entity(e);
        if visible {
            ec.insert((Transform::from_translation(to_bevy(center.into())).with_rotation(Quat::from_rotation_y(yaw as f32)).with_scale(Vec3::new(size.x as f32, size.y as f32, size.z as f32) * SCALE), Visibility::Inherited));
        } else {
            ec.insert(Visibility::Hidden);
        }
    }

    /// The floating staircase ahead of Mario that makes the toy obvious from the first frame.
    fn layout(&mut self, host: &mut Host) {
        let m = &host.sim.mario;
        let stairs = staircase(m.pos, m.floor_height, angle_to_radians(m.face_angle[1]));
        let star = goal_star(&stairs);
        for spec in stairs {
            self.create_piece(host, spec, false);
        }
        if let Some((_, old)) = self.goal.take() {
            host.commands.entity(old).despawn();
        }
        let mesh = host.meshes.add(Sphere::new(48.0 * SCALE).mesh().ico(2).unwrap());
        let mat = host.materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.85, 0.1), emissive: LinearRgba::rgb(1.0, 0.7, 0.0), unlit: true, ..default() });
        let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy(star))));
        self.goal = Some((star, e));
        self.won = None;
        self.ticks = 0;
    }

    /// Touch / keyboard placement without a pointer: the selected tool drops its piece in front of Mario.
    fn place_ahead(&mut self, host: &mut Host) {
        let (p, facing) = self.target(host);
        match self.tool {
            0 => host.toast("PICK A TOOL, THEN PRESS PLACE", 1.4),
            1 => self.place(host, PieceKind::Plank),
            2 => self.place(host, PieceKind::Block),
            3 => self.place(host, PieceKind::Ramp),
            4 => self.place(host, PieceKind::Spring),
            5 => {
                let mut spec = ObjectSpec::new(ObjectBehaviorKind::Bobomb, [p[0], p[1] + 60.0, p[2]], 0.0);
                spec.model = Some("actors/bobomb/black_bobomb_geo.glb".into());
                let id = host.sim.mario.objects.spawn(&spec);
                self.spawned_objects.push(id);
            }
            6 => {
                host.sim.pickups.push(crate::game::runner::Pickup { kind: crate::game::runner::PickupKind::Coin, pos: [p[0], p[1] + 80.0, p[2]], value: 1, collected: false, loot: None });
                self.spawned_pickups.push(host.sim.pickups.len() - 1);
            }
            _ => {
                // Portal: onto whatever Mario is looking at, chest height.
                let m = host.sim.mario.pos;
                let o = DVec3::new(m[0], m[1] + 90.0, m[2]);
                let d = DVec3::new(facing.sin(), 0.0, facing.cos());
                match ray_world(&host.sim.mario.world, o, d, 2500.0) {
                    Some((t, mut n)) => {
                        if n.dot(d) > 0.0 {
                            n = -n;
                        }
                        self.place_portal(host, o + d * t, n);
                    }
                    None => host.toast("FACE A WALL OR THE GROUND", 1.0),
                }
            }
        }
    }

    fn place(&mut self, host: &mut Host, kind: PieceKind) {
        let (p, facing) = self.target(host);
        self.history.record(self.snapshot());
        match kind {
            PieceKind::Plank => {
                // A bridge of planks at Mario's height, five in a row away from him.
                let (fx, fz) = (facing.sin(), facing.cos());
                let y = host.sim.mario.pos[1] - 20.0;
                let m = host.sim.mario.pos;
                for i in 0..5 {
                    let d = 300.0 + i as f64 * 220.0;
                    let center = [m[0] + fx * d, y - 15.0, m[2] + fz * d];
                    self.create_piece(host, PieceSpec { kind, center, size: [260.0, 30.0, 230.0], yaw: facing }, true);
                }
            }
            PieceKind::Block => self.create_piece(host, PieceSpec { kind, center: [p[0], p[1] + 120.0, p[2]], size: [240.0, 240.0, 240.0], yaw: facing }, true),
            // Rises away from Mario (local -Z points away); lifted 2 units so its low edge outranks the floor it sits on (SM64 orders floors by first-vertex height).
            PieceKind::Ramp => self.create_piece(host, PieceSpec { kind, center: [p[0], p[1] + 82.0, p[2]], size: [320.0, 160.0, 520.0], yaw: facing + std::f64::consts::PI }, true),
            PieceKind::Spring => self.create_piece(host, PieceSpec { kind, center: [p[0], p[1] + 22.0, p[2]], size: [170.0, 44.0, 170.0], yaw: 0.0 }, true),
        }
    }
}

impl Experiment for GodModeExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::God
    }
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }
    fn time_scale(&self) -> f64 {
        if self.slow_mo {
            0.35
        } else {
            1.0
        }
    }

    fn enter(&mut self, host: &mut Host) {
        // Work on a private copy of the course collision so pieces can come and go.
        let copy = Arc::new((*host.course_world()).clone());
        let (spawn, death) = (host.sim.course.spawn.clone(), host.sim.course.death_height);
        host.set_mario_world(copy, spawn, death);
        host.card(
            "02",
            "Mario vs. God",
            "Keep playing Mario while you reshape the world around him: real collision, rebuilt the moment it changes.",
            &[("Goal", "get the star on the top block"), ("1-8", "pick a tool"), ("Enter / Place", "drop the tool's piece in front of Mario (touch)"), ("Drag", "move - Shift lifts"), ("Wheel", "resize what you hover"), ("Right-click", "delete"), ("T", "slow motion"), ("Z / Y", "undo / redo"), ("X", "leave")],
        );
        self.layout(host);
        self.dock(host);
        host.toast("GET THE STAR ON THE TOP BLOCK - BUILD YOUR WAY UP", 3.6);
    }

    fn exit(&mut self, host: &mut Host) {
        for &i in &self.spawned_pickups {
            if let Some(p) = host.sim.pickups.get_mut(i) {
                p.collected = true;
            }
        }
        for id in &self.spawned_objects {
            if let Some(o) = host.sim.mario.objects.objects.get_mut(id.idx()) {
                o.active = false;
            }
        }
        host.sim.mario.gravity_scale = 1.0;
        host.restore_mario_world();
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if let Some(i) = TOOL_KEYS.iter().position(|k| *k == key) {
            self.set_tool(host, i);
            return true;
        }
        match key {
            KeyCode::KeyZ => match self.history.undo(self.snapshot()) {
                Some(state) => self.restore(host, state),
                None => host.toast("NOTHING TO UNDO", 0.9),
            },
            KeyCode::KeyY => {
                if let Some(state) = self.history.redo(self.snapshot()) {
                    self.restore(host, state);
                }
            }
            KeyCode::F9 => {
                self.history.record(self.snapshot());
                while !self.pieces.is_empty() {
                    self.remove_piece(host, 0);
                }
                self.layout(host);
                self.set_gravity(host, 2);
                host.toast("WORLD RESET", 1.0);
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some(t) = self.hovered {
                    self.delete_target(host, t);
                }
            }
            KeyCode::Enter => self.place_ahead(host),
            KeyCode::KeyG => {
                let next = (self.gravity_index + 1) % GRAVITY.len();
                self.set_gravity(host, next);
            }
            KeyCode::KeyT => {
                self.slow_mo = !self.slow_mo;
                host.toast(if self.slow_mo { "SLOW MOTION" } else { "NORMAL SPEED" }, 1.0);
            }
            _ => return false,
        }
        self.dock(host);
        true
    }

    fn after_step(&mut self, host: &mut Host) {
        self.step_portals(host);
        if self.won.is_none() {
            self.ticks += 1;
            if let Some((star, entity)) = self.goal {
                if star_reached(host.sim.mario.pos, star) {
                    self.won = Some((self.ticks, self.pieces.len()));
                    host.commands.entity(entity).insert(Visibility::Hidden);
                    host.fx("star_sound", star);
                    host.toast("STAR GET!", 2.5);
                }
            }
        }
        let m = &mut host.sim.mario;
        if m.vel[1] > 0.0 || m.floor.is_none() || m.pos[1] > m.floor_height + 4.0 {
            return;
        }
        let floor = m.floor.as_ref().map(sid).unwrap();
        let mut bounce_at = None;
        for piece in self.pieces.iter_mut().filter(|p| p.spec.kind == PieceKind::Spring) {
            if !piece.surfaces.contains(&floor) {
                continue;
            }
            m.set_action(ACT_TRIPLE_JUMP, 0.0);
            m.vel[1] = 96.0;
            piece.pop = 0.4;
            self.bounces += 1;
            bounce_at = Some(m.pos);
            break;
        }
        if let Some(p) = bounce_at {
            host.fx("triple_jump", p);
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.pointer(host);
        self.update_highlight(host);
        if let (Some((star, entity)), None) = (self.goal, self.won) {
            let (clock, bob) = (host.time(), (host.time() * 3.0).sin() * 12.0);
            host.commands.entity(entity).insert(Transform::from_translation(to_bevy([star[0], star[1] + bob, star[2]])).with_rotation(Quat::from_rotation_y(clock as f32 * 2.0)));
        }
        for piece in self.pieces.iter_mut().filter(|p| p.pop < 1.0) {
            piece.pop = (piece.pop + dt * 5.0).min(1.0);
            let s = if piece.pop >= 1.0 { 1.0 } else { 1.0 + (piece.pop * std::f64::consts::PI).sin() * 0.18 };
            let sy = if piece.spec.kind == PieceKind::Spring { 2.0 - s } else { s };
            host.commands.entity(piece.entity).insert(Transform::from_translation(to_bevy(piece.spec.center)).with_rotation(Quat::from_rotation_y(piece.spec.yaw as f32)).with_scale(Vec3::new(s as f32, sy as f32, s as f32)));
        }
        let goal = match self.won {
            Some((t, n)) => format!("\nSTAR GET in {} with {} pieces - F9 / RESET to build it again", super::race::format_time(t as f64 / 30.0), n),
            None => "\nGOAL: get the star on the top block".to_string(),
        };
        host.status(format!("{}   {} pieces   gravity x{}   {}   bounces {}   warps {}", TOOLS[self.tool].0, self.pieces.len(), GRAVITY[self.gravity_index], if self.slow_mo { "SLOW-MO" } else { "" }, self.bounces, self.warps) + &goal);
    }

    fn debug(&self) -> String {
        format!("pieces={:?} gravity={} slow_mo={} bounces={} undo={}", self.pieces.iter().map(|p| (p.id, p.spec.kind)).collect::<Vec<_>>(), GRAVITY[self.gravity_index], self.slow_mo, self.bounces, self.history.undo.len())
    }
}

/// A scripted player for the "can this mode be completed" tests: runs at a target on a
/// real `MarioRuntime`, jumping on request (shared by the Planet and Endless Run tests).
#[cfg(test)]
pub mod test_bot {
    use jumpman_sim::math::{atan2s, to_s16};
    use jumpman_sim::mario_runtime::MarioRuntime;
    use jumpman_sim::state::*;

    pub struct Bot {
        pub a_held: bool,
        pub held: u32,
        /// Stick deflection 0..64 (lower = slower, shorter jumps).
        pub speed: f64,
        /// Ticks A stays down after take-off (a tap vs a full-height jump).
        pub max_hold: u32,
    }

    impl Default for Bot {
        fn default() -> Self {
            Self { a_held: false, held: 0, speed: 64.0, max_hold: 100 }
        }
    }

    impl Bot {
        /// One 30 Hz step: stick toward `heading`, A held while rising from a jump.
        pub fn step(&mut self, m: &mut MarioRuntime, heading: f64, jump: bool) {
            let camera = to_s16(heading + 0x8000 as f64) as f64;
            let airborne = m.action & ACT_FLAG_AIR != 0;
            let want_a = if airborne { self.a_held && m.vel[1] > 0.0 && self.held < self.max_hold } else { jump };
            self.held = if want_a { self.held + 1 } else { 0 };
            let c = ControllerSnapshot {
                focus_yaw: None,
                stick_x: 0.0,
                stick_y: self.speed,
                stick_mag: self.speed,
                button_down: if want_a { A_BUTTON } else { 0 },
                button_pressed: if want_a && !self.a_held { A_BUTTON } else { 0 },
            };
            self.a_held = want_a;
            m.step(&c, camera, false, false);
        }
        /// Ground ahead along `heading` is missing, a step up, or a drop: time to jump.
        pub fn edge_ahead(m: &MarioRuntime, heading: f64) -> bool {
            if m.action & ACT_FLAG_AIR != 0 {
                return false;
            }
            let look = (m.forward_vel * 1.6).max(50.0);
            let (px, pz) = (m.pos[0] + jumpman_sim::math::sins(heading) * look, m.pos[2] + jumpman_sim::math::coss(heading) * look);
            let f = m.world.find_floor(px, m.pos[1] + 700.0, pz);
            f.surface.is_none() || f.height > m.pos[1] + 60.0 || f.height < m.pos[1] - 100.0
        }
        pub fn heading_to(m: &MarioRuntime, target: [f64; 3]) -> f64 {
            atan2s(target[2] - m.pos[2], target[0] - m.pos[0]) as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::CollisionWorld;

    /// Walks and jumps a real Mario up the starter staircase to the star: the goal can be reached
    /// with no tools at all, so "Mario vs. God" can be won as shipped (building only makes it easier).
    #[test]
    fn mario_can_climb_the_starter_staircase_to_the_star() {
        use jumpman_sim::mario_runtime::MarioRuntime;
        let mut world = CollisionWorld::default();
        world.add_quad([-6000., 0., -6000.], [-6000., 0., 6000.], [6000., 0., 6000.], [6000., 0., -6000.], &SurfaceOptions::default());
        let stairs = staircase([0.0, 0.0, 0.0], 0.0, 0.0);
        let mut faces = vec![];
        for s in &stairs {
            add_box_collision(&mut world, s.center, s.size, s.yaw, &mut faces, &SurfaceOptions::default());
        }
        world.finalize();
        let star = goal_star(&stairs);
        let mut m = MarioRuntime::new(Arc::new(world), jumpman_sim::mario_runtime::SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0);
        let mut bot = test_bot::Bot { speed: 44.0, max_hold: 8, ..Default::default() };
        let mut next = 0;
        let mut reached = None;
        for tick in 0..30 * 90 {
            let grounded = m.action & ACT_FLAG_AIR == 0;
            if grounded {
                // The next block to reach is the lowest one still above Mario's feet.
                next = stairs.iter().position(|s| s.center[1] + s.size[1] / 2.0 > m.floor_height + 10.0).unwrap_or(stairs.len());
            }
            let block = &stairs[next.min(stairs.len() - 1)];
            let target = [block.center[0], block.center[1] + block.size[1] / 2.0, block.center[2]];
            let aim = if next >= stairs.len() { [star[0], star[1], star[2]] } else { target };
            let heading = test_bot::Bot::heading_to(&m, aim);
            let jump = grounded && next < stairs.len() && m.forward_vel > 3.0 && test_bot::Bot::edge_ahead(&m, heading);
            bot.step(&mut m, heading, jump);
            if star_reached(m.pos, star) {
                reached = Some(tick);
                break;
            }
        }
        assert!(reached.is_some(), "bot never touched the star; stopped at {:?} next block {next}", m.pos);
    }

    #[test]
    fn star_reach_test_is_local() {
        let star = [100.0, 800.0, 100.0];
        assert!(star_reached([100.0, 760.0, 100.0], star));
        assert!(!star_reached([100.0, 0.0, 100.0], star));
        assert!(!star_reached([900.0, 760.0, 100.0], star));
    }

    #[test]
    fn history_undoes_and_redoes_layouts() {
        let a = PieceSpec { kind: PieceKind::Block, center: [0.0; 3], size: [100.0; 3], yaw: 0.0 };
        let b = PieceSpec { kind: PieceKind::Spring, center: [1.0; 3], size: [50.0; 3], yaw: 0.0 };
        let mut h = History::default();
        h.record(vec![]);
        // Now the layout is [a]; undo returns the recorded empty layout and remembers [a] for redo.
        assert_eq!(h.undo(vec![a]), Some(vec![]));
        assert_eq!(h.redo(vec![]), Some(vec![a]));
        h.record(vec![a]);
        assert!(h.redo(vec![a, b]).is_none(), "recording clears redo");
        assert_eq!(h.undo(vec![a, b]), Some(vec![a]));
    }

    #[test]
    fn placed_pieces_are_real_collision() {
        let mut world = CollisionWorld::default();
        world.add_quad([-3000., 0., -3000.], [-3000., 0., 3000.], [3000., 0., 3000.], [3000., 0., -3000.], &SurfaceOptions::default());
        let mut faces = vec![];
        add_box_collision(&mut world, [0., 120., 0.], [240., 240., 240.], 0.3, &mut faces, &SurfaceOptions::default());
        assert!((world.find_floor(0., 600., 0.).height - 240.0).abs() < 1e-6);
        let ids: HashSet<usize> = faces.iter().map(sid).collect();
        world.remove_surfaces(&|s| ids.contains(&(s as *const Surface as usize)));
        assert!(world.find_floor(0., 600., 0.).height.abs() < 1e-6, "removing a piece restores the ground");
        let mut ramp = vec![];
        add_ramp_collision(&mut world, [0., 82., 1000.], [320., 160., 520.], std::f64::consts::PI, &mut ramp);
        // Rises away from the origin (toward +Z after a half turn).
        let (lo, hi) = (world.find_floor(0., 600., 800.).height, world.find_floor(0., 600., 1200.).height);
        assert!(hi > lo + 100.0, "lo {lo} hi {hi}");
    }
}
