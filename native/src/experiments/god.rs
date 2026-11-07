//! Mario vs. God (`god.ts`): play normally while pieces reshape the world around him. Every
//! piece is real collision, rebuilt the moment it changes, with undo/redo.
//!
//! The TS drives this with the mouse (drag-to-move, paint a bridge, click to place, portals).
//! This port is keyboard-driven: pieces are placed in front of Mario. Not ported: the Grab tool,
//! Bob-omb/coin spawning, portals and the hover highlight.
//!
//! `2` bridge  `3` block  `4` ramp  `5` spring   `Z` undo  `Y` redo  `Delete` remove last
//! `G` gravity  `T` slow motion
use super::geometry::{add_box_collision, add_ramp_collision};
use super::{Experiment, ExperimentId, Host};
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

impl GodModeExperiment {
    pub fn new() -> Self {
        Self { pieces: vec![], next_id: 1, history: History::default(), slow_mo: false, gravity_index: 2, bounces: 0, materials: vec![] }
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
            &[("2", "bridge of planks"), ("3", "block"), ("4", "ramp"), ("5", "spring pad"), ("Z / Y", "undo / redo"), ("Delete", "remove last"), ("G", "gravity"), ("T", "slow motion"), ("X", "leave")],
        );
        host.status("2 bridge  3 block  4 ramp  5 spring  Z undo  G gravity  T slow-mo");
    }

    fn exit(&mut self, host: &mut Host) {
        host.sim.mario.gravity_scale = 1.0;
        host.restore_mario_world();
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        match key {
            KeyCode::Digit2 => self.place(host, PieceKind::Plank),
            KeyCode::Digit3 => self.place(host, PieceKind::Block),
            KeyCode::Digit4 => self.place(host, PieceKind::Ramp),
            KeyCode::Digit5 => self.place(host, PieceKind::Spring),
            KeyCode::KeyZ => {
                if let Some(state) = self.history.undo(self.snapshot()) {
                    self.restore(host, state);
                }
            }
            KeyCode::KeyY => {
                if let Some(state) = self.history.redo(self.snapshot()) {
                    self.restore(host, state);
                }
            }
            KeyCode::Delete | KeyCode::Backspace => {
                if !self.pieces.is_empty() {
                    self.history.record(self.snapshot());
                    let last = self.pieces.len() - 1;
                    self.remove_piece(host, last);
                }
            }
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
        true
    }

    fn after_step(&mut self, host: &mut Host) {
        let m = &mut host.sim.mario;
        if m.vel[1] > 0.0 || m.floor.is_none() || m.pos[1] > m.floor_height + 4.0 {
            return;
        }
        let floor = m.floor.as_ref().map(sid).unwrap();
        for piece in self.pieces.iter_mut().filter(|p| p.spec.kind == PieceKind::Spring) {
            if !piece.surfaces.contains(&floor) {
                continue;
            }
            m.set_action(ACT_TRIPLE_JUMP, 0.0);
            m.vel[1] = 96.0;
            piece.pop = 0.4;
            self.bounces += 1;
            break;
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        for piece in self.pieces.iter_mut().filter(|p| p.pop < 1.0) {
            piece.pop = (piece.pop + dt * 5.0).min(1.0);
            let s = if piece.pop >= 1.0 { 1.0 } else { 1.0 + (piece.pop * std::f64::consts::PI).sin() * 0.18 };
            let sy = if piece.spec.kind == PieceKind::Spring { 2.0 - s } else { s };
            host.commands.entity(piece.entity).insert(Transform::from_translation(to_bevy(piece.spec.center)).with_rotation(Quat::from_rotation_y(piece.spec.yaw as f32)).with_scale(Vec3::new(s as f32, sy as f32, s as f32)));
        }
        host.status(format!("{} pieces   gravity x{}   {}   bounces {}", self.pieces.len(), GRAVITY[self.gravity_index], if self.slow_mo { "SLOW-MO" } else { "" }, self.bounces));
    }

    fn debug(&self) -> String {
        format!("pieces={:?} gravity={} slow_mo={} bounces={} undo={}", self.pieces.iter().map(|p| (p.id, p.spec.kind)).collect::<Vec<_>>(), GRAVITY[self.gravity_index], self.slow_mo, self.bounces, self.history.undo.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::CollisionWorld;

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
