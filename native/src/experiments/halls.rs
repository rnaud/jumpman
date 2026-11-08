//! The Painted Halls: an original metroidvania on Mario's real physics.
//!
//! One connected manor of sixteen rooms in six regions. Mario's own moves are the keys: a long
//! jump over wide gaps, wall kicks up tall shafts, the ground pound through cracked floors, and
//! the three power caps (wing: flutter up high ledges, metal: walk through the ink, vanish:
//! slip through iron bars). The data lives in `halls_world.rs` (with the solver that proves the
//! game completable), the rules in `halls_core.rs`; this file glues them to the host: it builds
//! each room's collision and meshes (only the current room exists), swaps the player's world on
//! every door, enforces which moves are unlocked, maps Mario's actions to combat events and
//! publishes render lists and HUD numbers through [`HallsBridge`] (`halls_view.rs`,
//! `halls_ui.rs`).
//!
//! Controls: stick and A / B / Z as ever; `Tab` (touch MAP, pad Select) opens the map, `F` (touch CAP, pad RB)
//! cycles the worn cap; Esc pauses. Test hooks (env): `JUMPMAN_HALLS_NEW=1`, `_ROOM=<index|name>`,
//! `_ABILITIES=all|LJ,WK,GP,WG,MT,VN`, `_CAP=wing|metal|vanish`, `_GOD=1`, `_POS=x,y,z`,
//! `_FRAGMENTS=<n>`, `_SCREEN=map|prompt|dead|win`, `_DISCOVER=all`, `_KEY=1`, `_BOSSES=<mask>`,
//! `_SHOWCASE=1`. Runs using any of them never touch the save file.
use super::geometry::add_box_collision;
use super::halls_core::*;
use super::halls_view::{self, FoeView, HallsBridge, HazardView, Screen, UiCmd};
use super::halls_world::Rect;
use super::halls_world::*;
use super::textures::textured;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::object_view::spawn_model;
use crate::render::{scene, to_bevy, SCALE};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::math::{angle_to_radians, radians_to_angle};
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::state::*;
use std::sync::Arc;

/// Far from every course object and from the other experiments' arenas.
pub const ORIGIN: [f64; 3] = [-240000.0, 2000.0, -240000.0];
const FLAME: &str = "actors/flame/red_flame_geo.glb";
const CHEST_BASE: &str = "actors/treasure_chest/treasure_chest_base_geo.glb";
const CHEST_LID: &str = "actors/treasure_chest/treasure_chest_lid_geo.glb";
const FLAP_VEL: f64 = 56.0;
/// Top of the catch-all floor under every pit (below `kill_y`).
const PIT_FLOOR: f32 = -800.0;
const MAX_FLAPS: u8 = 3;

fn at(p: [f32; 3]) -> Vec3 {
    to_bevy([ORIGIN[0] + p[0] as f64, ORIGIN[1] + p[1] as f64, ORIGIN[2] + p[2] as f64])
}

fn world_pos(p: [f32; 3]) -> [f64; 3] {
    [ORIGIN[0] + p[0] as f64, ORIGIN[1] + p[1] as f64, ORIGIN[2] + p[2] as f64]
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// Collision for a room: every box, the outer walls, and the bars unless they are open.
pub fn build_collision(geom: &Geom, origin: [f64; 3], bars_open: bool) -> CollisionWorld {
    let mut world = CollisionWorld::new(1024.0, 1e9);
    let o = SurfaceOptions::default();
    let mut faces = vec![];
    let mut add = |r: &Rect, y0: f32, y1: f32| {
        add_box_collision(&mut world, [origin[0] + r.cx as f64, origin[1] + (y0 + y1) as f64 / 2.0, origin[2] + r.cz as f64], [r.w as f64, (y1 - y0) as f64, r.d as f64], 0.0, &mut faces, &o);
    };
    for b in geom.boxes.iter().chain(geom.walls.iter()) {
        add(&b.r, b.y0, b.y1);
    }
    // A floor far below every pit: SM64 treats stepping into a column with no floor at all as hitting a wall,
    // so a jump across a hole would bonk off thin air without it. Falling Mario is caught by `kill_y` first.
    add(&Rect::new(0.0, 0.0, geom.w + 2.0 * WALL_THICK, geom.d + 2.0 * WALL_THICK), PIT_FLOOR - 100.0, PIT_FLOOR);
    if !bars_open {
        for (r, h) in &geom.bars {
            add(r, 0.0, *h);
        }
    }
    world.finalize();
    world
}

// ---------------------------------------------------------------------------------------------
// The moves that have to be found first (pure functions over the controller, shared with the tests)
// ---------------------------------------------------------------------------------------------

/// Strip the inputs of moves the player does not have yet: the crouch for the long jump on the
/// ground, Z in the air for the ground pound, A against a wall for the wall kick.
pub fn gate_controller(c: &mut ControllerSnapshot, action: u32, has: &dyn Fn(Ability) -> bool) {
    let air = action & ACT_FLAG_AIR != 0;
    if (!air && !has(Ability::LongJump)) || (air && !has(Ability::GroundPound) && action != ACT_GROUND_POUND) {
        c.button_down &= !Z_TRIG;
        c.button_pressed &= !Z_TRIG;
    }
    if action == ACT_AIR_HIT_WALL && !has(Ability::WallKick) {
        c.button_down &= !A_BUTTON;
        c.button_pressed &= !A_BUTTON;
    }
}

/// A wall kick before the ability is found is a stumble.
pub fn undo_wall_kick(m: &mut MarioRuntime, has: &dyn Fn(Ability) -> bool) {
    if m.action == ACT_WALL_KICK_AIR && !has(Ability::WallKick) {
        m.set_action(ACT_FREEFALL, 0.0);
        m.vel[1] = 0.0;
        m.forward_vel = 0.0;
    }
}

/// The wing cap: a flutter with A in the air (three per jump), a glide while A is held.
/// The flap is a scripted ballistic arc (`boost` is its remaining upward speed) because the
/// controller cuts a jump short as soon as A is released. Returns true when a flap happened.
pub fn wing_flutter(m: &mut MarioRuntime, c: &ControllerSnapshot, flaps: &mut u8, boost: &mut f64) -> bool {
    let ineligible = matches!(m.action, ACT_FLYING | ACT_FLYING_TRIPLE_JUMP | ACT_AIR_HIT_WALL | ACT_GROUND_POUND | ACT_DIVE | ACT_WALL_KICK_AIR) || m.action & ACT_FLAG_AIR == 0;
    if ineligible {
        *boost = 0.0;
        return false;
    }
    let mut flapped = false;
    if c.button_pressed & A_BUTTON != 0 && *flaps < MAX_FLAPS {
        *flaps += 1;
        *boost = FLAP_VEL;
        flapped = true;
    }
    if *boost > 0.0 {
        m.vel[1] = m.vel[1].max(*boost);
        *boost -= 4.0;
    } else if c.button_down & A_BUTTON != 0 && m.vel[1] < -14.0 {
        m.vel[1] = -14.0;
    }
    flapped
}


// ---------------------------------------------------------------------------------------------
// Demo autopilot (`JUMPMAN_HALLS_DEMO=wallkick|flutter|longjump|pound|north`): drives Mario through a
// move with scripted controller input so a screenshot can catch each ability in use.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DemoKind {
    WallKick,
    Flutter,
    LongJump,
    Pound,
    North,
}

struct Demo {
    kind: DemoKind,
    t: u32,
    dir: f64,
    last_flap: u32,
    jumped: bool,
}

const DEMO_NORTH: f64 = std::f64::consts::PI;
const DEMO_EAST: f64 = std::f64::consts::FRAC_PI_2;
const DEMO_WEST: f64 = -std::f64::consts::FRAC_PI_2;

/// Stick for a desired world yaw with camera yaw 0 (SM64's `atan2s(y, x)` measures from +y).
fn stick_for(yaw: f64) -> (f64, f64) {
    let theta = angle_to_radians(radians_to_angle(yaw) as f64);
    (theta.sin() * 64.0, -theta.cos() * 64.0)
}

impl Demo {
    fn parse(s: &str) -> Option<Self> {
        let kind = match s {
            "wallkick" => DemoKind::WallKick,
            "flutter" => DemoKind::Flutter,
            "longjump" => DemoKind::LongJump,
            "pound" => DemoKind::Pound,
            "north" => DemoKind::North,
            _ => return None,
        };
        Some(Self { kind, t: 0, dir: DEMO_EAST, last_flap: 0, jumped: false })
    }

    fn controller(&mut self, m: &MarioRuntime) -> ControllerSnapshot {
        self.t += 1;
        let p = [m.pos[0] - ORIGIN[0], m.pos[1] - ORIGIN[1], m.pos[2] - ORIGIN[2]];
        let air = m.action & ACT_FLAG_AIR != 0;
        let (mut yaw, mut a, mut z, mut zp, mut ap) = (None, false, false, false, false);
        match self.kind {
            DemoKind::North => yaw = Some(DEMO_NORTH),
            DemoKind::WallKick => {
                let wall_x = if self.dir == DEMO_EAST { 170.0 } else { -170.0 };
                let sign = if self.dir == DEMO_EAST { 1.0 } else { -1.0 };
                yaw = Some(self.dir);
                if m.action == ACT_AIR_HIT_WALL {
                    self.dir = if self.dir == DEMO_EAST { DEMO_WEST } else { DEMO_EAST };
                    yaw = Some(self.dir);
                    ap = true;
                    a = true;
                } else if !air && (wall_x - sign * 50.0 - p[0]).abs() <= 130.0 {
                    ap = true;
                    a = true;
                }
            }
            DemoKind::LongJump => {
                yaw = Some(DEMO_NORTH);
                if !self.jumped && p[2] < -560.0 && !air {
                    z = true;
                    zp = self.t % 2 == 0;
                    if self.t > 3 && self.t % 2 == 1 {
                        ap = true;
                        a = true;
                        self.jumped = true;
                    }
                }
                if self.jumped {
                    z = true;
                }
            }
            DemoKind::Pound => {
                // Hop in place, then slam.
                match self.t {
                    8 => {
                        ap = true;
                        a = true;
                    }
                    26 => {
                        zp = true;
                        z = true;
                    }
                    27..=60 => z = true,
                    _ => {}
                }
            }
            DemoKind::Flutter => {
                // Hold beside the foyer ledge flapping up, then slide onto it.
                let press = if !air { self.t < 4 } else { m.vel[1] < 6.0 && self.t - self.last_flap >= 6 };
                if press {
                    self.last_flap = self.t;
                    ap = true;
                    a = true;
                }
                let target = if p[1] < 1010.0 && p[0] < 450.0 { [320.0, 300.0] } else { [700.0, 300.0] };
                let (dx, dz) = (target[0] - p[0], target[1] - p[2]);
                if dx.hypot(dz) > 40.0 {
                    yaw = Some(dx.atan2(dz));
                }
            }
        }
        let (sx, sy, mag) = match yaw {
            Some(y) => {
                let (x, yy) = stick_for(y);
                (x, yy, 64.0)
            }
            None => (0.0, 0.0, 0.0),
        };
        ControllerSnapshot { focus_yaw: None, stick_x: sx, stick_y: sy, stick_mag: mag, button_down: if a { A_BUTTON } else { 0 } | if z { Z_TRIG } else { 0 }, button_pressed: if ap { A_BUTTON } else { 0 } | if zp { Z_TRIG } else { 0 } }
    }
}

// ---------------------------------------------------------------------------------------------
// Mesh building
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Buf {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Buf {
    /// A quad given in SM64 room units; wound to face `n`.
    fn quad(&mut self, p: [Vec3; 4], n: Vec3, uv: [[f32; 2]; 4], color: [f32; 4]) {
        let b = |v: Vec3| at(v.to_array());
        let q = [b(p[0]), b(p[1]), b(p[2]), b(p[3])];
        let cross = (q[1] - q[0]).cross(q[2] - q[0]);
        let order: [usize; 4] = if cross.dot(n) >= 0.0 { [0, 1, 2, 3] } else { [0, 3, 2, 1] };
        let base = self.pos.len() as u32;
        for &i in &order {
            self.pos.push(q[i].to_array());
            self.nor.push(n.to_array());
            self.uv.push(uv[i]);
            self.col.push(color);
        }
        self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn push_box(&mut self, r: &Rect, y0: f32, y1: f32, tile: f32, tops_only: bool, color: [f32; 4]) {
        self.push_box_in(r, y0, y1, tile, tops_only, color, None);
    }

    /// Like `push_box`, but sides lying on the room's outer boundary `(half w, half d)` are skipped.
    #[allow(clippy::too_many_arguments)]
    fn push_box_in(&mut self, r: &Rect, y0: f32, y1: f32, tile: f32, tops_only: bool, color: [f32; 4], outer: Option<(f32, f32)>) {
        let (x0, x1, z0, z1) = (r.cx - r.w / 2.0, r.cx + r.w / 2.0, r.cz - r.d / 2.0, r.cz + r.d / 2.0);
        let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        // Top.
        self.quad([v(x0, y1, z1), v(x1, y1, z1), v(x1, y1, z0), v(x0, y1, z0)], Vec3::Y, [[x0 / tile, z1 / tile], [x1 / tile, z1 / tile], [x1 / tile, z0 / tile], [x0 / tile, z0 / tile]], color);
        if tops_only {
            return;
        }
        let side = |s: &mut Self, a: Vec3, b: Vec3, n: Vec3, along: (f32, f32)| {
            s.quad([v(a.x, y0, a.z), v(b.x, y0, b.z), v(b.x, y1, b.z), v(a.x, y1, a.z)], n, [[along.0 / tile, -y0 / tile], [along.1 / tile, -y0 / tile], [along.1 / tile, -y1 / tile], [along.0 / tile, -y1 / tile]], color);
        };
        let on_edge = |c: f32, lim: f32| outer.is_some() && (c - lim).abs() < 1.0;
        let (hw, hd) = outer.unwrap_or((0.0, 0.0));
        if !on_edge(z1, hd) {
            side(self, v(x0, 0.0, z1), v(x1, 0.0, z1), Vec3::Z, (x0, x1));
        }
        if !on_edge(z0, -hd) {
            side(self, v(x1, 0.0, z0), v(x0, 0.0, z0), Vec3::NEG_Z, (-x1, -x0));
        }
        if !on_edge(x1, hw) {
            side(self, v(x1, 0.0, z1), v(x1, 0.0, z0), Vec3::X, (z1, z0));
        }
        if !on_edge(x0, -hw) {
            side(self, v(x0, 0.0, z0), v(x0, 0.0, z1), Vec3::NEG_X, (z0, z1));
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        mesh.insert_indices(Indices::U32(self.idx));
        mesh
    }

    fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }
}

/// One framed painting: gradient sky, hills and a sun as vertex-coloured quads.
fn painting_mesh(hue: f32) -> Mesh {
    let hsv = |h: f32, s: f32, v: f32| -> [f32; 4] {
        let c = Color::hsv((h.rem_euclid(1.0)) * 360.0, s, v).to_srgba();
        [c.red, c.green, c.blue, 1.0]
    };
    let mut b = Buf::default();
    // Local frame: x across (±100), y up (0..150), drawn in a plane facing +z at the origin; the caller rotates it.
    let q = |b: &mut Buf, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 4]| {
        let base = b.pos.len() as u32;
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
            b.pos.push([x * SCALE, y * SCALE, 0.0]);
            b.nor.push([0.0, 0.0, 1.0]);
            b.uv.push([0.0, 0.0]);
            b.col.push(c);
        }
        b.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    // Frame.
    q(&mut b, -112.0, -8.0, 112.0, 166.0, [0.75, 0.55, 0.16, 1.0]);
    q(&mut b, -100.0, 4.0, 100.0, 154.0, [0.1, 0.07, 0.04, 1.0]);
    // Sky bands.
    for i in 0..4 {
        let y0 = 60.0 + i as f32 * 24.0;
        q(&mut b, -96.0, y0, 96.0, y0 + 24.0 + 0.5, hsv(hue + 0.5 + i as f32 * 0.015, 0.45 - i as f32 * 0.06, 0.92 - i as f32 * 0.05));
    }
    // Ground and a hill.
    q(&mut b, -96.0, 8.0, 96.0, 60.5, hsv(hue + 0.25, 0.55, 0.45));
    q(&mut b, -60.0, 60.0, 30.0, 82.0, hsv(hue + 0.2, 0.5, 0.35));
    // Sun.
    q(&mut b, 40.0, 112.0, 70.0, 140.0, hsv(hue + 0.1, 0.85, 1.0));
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, b.nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.col);
    mesh.insert_indices(Indices::U32(b.idx));
    mesh
}

// ---------------------------------------------------------------------------------------------
// The experiment
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Then {
    Door { room: usize, spawn: [f32; 4], from: Option<usize> },
    Respawn,
}

#[derive(Clone, Debug)]
enum Fade {
    Idle,
    Out { t: f32, then: Then },
    In { t: f32 },
}

struct PickEnt {
    idx: usize,
    ent: Entity,
    lid: Option<Entity>,
    base: Vec3,
    chest: bool,
}

pub struct PaintedHallsExperiment {
    core: Halls,
    t: f64,
    ephemeral: bool,
    started: bool,
    fade: Fade,
    room_ents: Vec<Entity>,
    picks: Vec<PickEnt>,
    glyphs: Vec<(Ability, Entity, Vec3)>,
    /// Wall decor (doors, paintings): entity, wall point and inward normal in room units.
    decor: Vec<(Entity, [f32; 2], [f32; 2])>,
    eye: [f32; 3],
    bars_solid: Vec<Entity>,
    bars_ghost: Vec<Entity>,
    light: Option<Entity>,
    world_solid: Arc<CollisionWorld>,
    world_open: Arc<CollisionWorld>,
    has_bars: bool,
    bars_open: bool,
    last_action: u32,
    flaps: u8,
    boost: f64,
    save_dirty: bool,
    dead_at: f64,
    hurt_flash: f32,
    banner_age: f32,
    hint_age: f32,
    prev_foes: Vec<[f32; 3]>,
    prev_boss: [f32; 3],
    map_gen: u32,
    god: bool,
    seq: u32,
    hint_ink_at: f64,
    summary_sent: bool,
    demo: Option<Demo>,
}

impl PaintedHallsExperiment {
    pub fn new() -> Self {
        let empty = Arc::new(CollisionWorld::new(1024.0, 1e9));
        Self {
            core: Halls::new(SaveData::default(), 1),
            t: 0.0,
            ephemeral: false,
            started: false,
            fade: Fade::Idle,
            room_ents: vec![],
            picks: vec![],
            glyphs: vec![],
            decor: vec![],
            eye: [0.0, 1000.0, 3000.0],
            bars_solid: vec![],
            bars_ghost: vec![],
            light: None,
            world_solid: empty.clone(),
            world_open: empty,
            has_bars: false,
            bars_open: false,
            last_action: 0,
            flaps: 0,
            boost: 0.0,
            save_dirty: false,
            dead_at: 0.0,
            hurt_flash: 0.0,
            banner_age: 99.0,
            hint_age: 99.0,
            prev_foes: vec![[0.0; 3]; FOE_SLOTS],
            prev_boss: [0.0; 3],
            map_gen: 1,
            god: false,
            seq: 0,
            hint_ink_at: -10.0,
            summary_sent: false,
            demo: None,
        }
    }

    // -----------------------------------------------------------------------------------------
    // Rooms
    // -----------------------------------------------------------------------------------------

    fn refresh_world(&mut self) {
        let geom = &self.core.geom;
        self.has_bars = !geom.bars.is_empty();
        self.world_solid = Arc::new(build_collision(geom, ORIGIN, false));
        self.world_open = if self.has_bars { Arc::new(build_collision(geom, ORIGIN, true)) } else { self.world_solid.clone() };
    }

    fn current_world(&self) -> Arc<CollisionWorld> {
        if self.bars_open { self.world_open.clone() } else { self.world_solid.clone() }
    }

    /// Put the player into `room` at `spawn` (local x, y, z, yaw): rules, collision, entities, mood.
    fn go_to(&mut self, host: &mut Host, room: usize, spawn: [f32; 4], from: Option<usize>) {
        let region_before = self.core.def().region;
        self.core.enter_room(room, spawn, from);
        self.core.player.iframes = self.core.player.iframes.max(40);
        self.refresh_world();
        self.bars_open = self.core.save.cap == Some(Ability::VanishCap);
        let sp = SpawnPoint { action: None, pos: world_pos([spawn[0], spawn[1] + 60.0, spawn[2]]), face_yaw: 0.0 };
        host.set_mario_world(self.current_world(), sp, -1e9);
        host.place_mario(world_pos([spawn[0], spawn[1] + 8.0, spawn[2]]), Some(radians_to_angle(spawn[3] as f64) as f64));
        self.build_room(host);
        self.mood(host);
        self.flaps = 0;
        self.last_action = 0;
        self.prev_foes.iter_mut().for_each(|p| *p = [0.0; 3]);
        self.map_gen += 1;
        self.publish_map(host);
        let def = self.core.def();
        let small = if def.region != region_before || from.is_none() { def.region.name() } else { "" };
        host.halls.banner = Some((format!("{}|{}", small, def.name.to_uppercase()), 0.0));
        self.banner_age = 0.0;
        self.save_dirty = true;
        if let Some(b) = self.core.boss {
            host.toast(&format!("{} WAKES", b.kind.name()), 2.0);
        }
    }

    fn mood(&self, host: &mut Host) {
        let look = self.core.def().region.look();
        let c = look.fog;
        host.set_clear(Color::srgb(c[0], c[1], c[2]));
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(look.sky_top, look.sky_bottom, look.sky_bottom).with_clouds(0.0));
        host.cam.fog = Some((Color::srgb(c[0], c[1], c[2]), look.fog_range[0] as f64, look.fog_range[1] as f64));
    }

    fn build_room(&mut self, host: &mut Host) {
        for e in self.room_ents.drain(..) {
            host.commands.entity(e).despawn();
        }
        self.picks.clear();
        self.glyphs.clear();
        self.decor.clear();
        self.bars_solid.clear();
        self.bars_ghost.clear();
        let room = self.core.room;
        let def = self.core.def().clone();
        let look = def.region.look();
        let (hw, hd) = (def.w / 2.0, def.d / 2.0);
        let flags = self.core.save.flags.clone();
        let rgba = |t: [f32; 3]| [t[0], t[1], t[2], 1.0];
        // Terrain meshes by style.
        let (mut floor, mut plat, mut wallb, mut crate_b, mut hedge, mut cracked, mut ink) = (Buf::default(), Buf::default(), Buf::default(), Buf::default(), Buf::default(), Buf::default(), Buf::default());
        for b in &self.core.geom.boxes {
            match b.style {
                Style::Floor => {
                    floor.push_box_in(&b.r, b.y0, b.y1, 400.0, false, rgba(look.floor_tint), Some((hw, hd)));
                }
                Style::Plat => plat.push_box(&b.r, b.y0, b.y1, 300.0, false, rgba(look.plat_tint)),
                Style::Wall => wallb.push_box(&b.r, b.y0, b.y1, 300.0, false, rgba(look.wall_tint)),
                Style::Crate => crate_b.push_box(&b.r, b.y0, b.y1, 200.0, false, [1.0, 0.85, 0.65, 1.0]),
                Style::Hedge => hedge.push_box(&b.r, b.y0, b.y1, 200.0, false, [0.5, 0.85, 0.4, 1.0]),
                Style::Cracked => cracked.push_box(&b.r, b.y0, b.y1, 300.0, false, [0.55, 0.5, 0.5, 1.0]),
                Style::Ink => ink.push_box(&b.r, b.y0, b.y1, 300.0, true, [0.05, 0.03, 0.1, 1.0]),
            }
        }
        // The dollhouse: inward-facing wall quads (invisible from outside, so the camera sees in).
        let mut walls = Buf::default();
        let (y0, y1) = (-120.0, def.h);
        let v = Vec3::new;
        let tile = 300.0;
        let wc = rgba(look.wall_tint);
        for (a, b, n) in [(v(-hw, 0.0, -hd), v(hw, 0.0, -hd), Vec3::Z), (v(hw, 0.0, hd), v(-hw, 0.0, hd), Vec3::NEG_Z), (v(-hw, 0.0, hd), v(-hw, 0.0, -hd), Vec3::X), (v(hw, 0.0, -hd), v(hw, 0.0, hd), Vec3::NEG_X)] {
            let len = (b - a).length();
            walls.quad([v(a.x, y0, a.z), v(b.x, y0, b.z), v(b.x, y1, b.z), v(a.x, y1, a.z)], n, [[0.0, -y0 / tile], [len / tile, -y0 / tile], [len / tile, -y1 / tile], [0.0, -y1 / tile]], wc);
        }
        // Skirting along the base so the room reads as a room.
        let floor_mat = textured(host, look.floor, Color::WHITE);
        let wall_mat = textured(host, look.wall, Color::WHITE);
        let plat_mat = textured(host, look.plat, Color::WHITE);
        let crate_mat = textured(host, "planks", Color::WHITE);
        let hedge_mat = textured(host, "grassTop", Color::srgb(0.5, 0.8, 0.45));
        let crack_mat = textured(host, "stone", Color::srgb(0.7, 0.55, 0.5));
        let ink_mat = host.materials.add(StandardMaterial { base_color: Color::srgb(0.05, 0.03, 0.1), perceptual_roughness: 0.6, ..default() });
        for (buf, mat) in [(floor, floor_mat), (plat, plat_mat), (wallb, wall_mat.clone()), (walls, wall_mat), (crate_b, crate_mat), (hedge, hedge_mat), (cracked, crack_mat), (ink, ink_mat)] {
            if buf.is_empty() {
                continue;
            }
            let mesh = host.meshes.add(buf.into_mesh());
            let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default()));
            self.room_ents.push(e);
        }
        // Features.
        let boss_reward = self.core.boss_reward();
        for (i, f) in def.feats.iter().enumerate() {
            let fl = flag(room, i);
            match *f {
                Feat::Door(d) => {
                    let p = d.wall_pos(def.w, def.d);
                    let n = d.inward();
                    let model = match d.lock {
                        Lock::None => "actors/door/wooden_door_geo.glb",
                        Lock::Key(_) => "actors/door/key_door_geo.glb",
                        Lock::Stars(_) => "actors/door/castle_door_3_stars_geo.glb",
                        Lock::Boss(_) | Lock::OneWay => "actors/door/metal_door_geo.glb",
                    };
                    if host.asset_root.exists(model) {
                        let mut e = spawn_model(host.commands, host.assets, host.asset_root, model, false);
                        let id = e.insert((Transform::from_translation(at([p[0] + n[0] * 8.0, d.y, p[1] + n[1] * 8.0])).with_rotation(Quat::from_rotation_y(n[0].atan2(n[1])))  .with_scale(Vec3::splat(0.9)), Visibility::Inherited)).id();
                        host.commands.entity(id).insert(super::XpEntity);
                        self.room_ents.push(id);
                        self.decor.push((id, p, n));
                    }
                }
                Feat::Save { x, z } => {
                    let base = host.lit(Color::srgb(0.75, 0.72, 0.85));
                    let m = host.meshes.add(Cylinder::new(80.0 * SCALE, 50.0 * SCALE));
                    let e = host.spawn((Mesh3d(m), MeshMaterial3d(base), Transform::from_translation(at([x, 25.0, z]))));
                    self.room_ents.push(e);
                    let glow = host.materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.9, 0.5, 0.25), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
                    let col = host.meshes.add(Cylinder::new(70.0 * SCALE, 520.0 * SCALE));
                    let e = host.spawn((Mesh3d(col), MeshMaterial3d(glow), Transform::from_translation(at([x, 290.0, z]))));
                    self.room_ents.push(e);
                    if host.asset_root.exists("actors/star/star_geo.glb") {
                        let mut s = spawn_model(host.commands, host.assets, host.asset_root, "actors/star/star_geo.glb", true);
                        let id = s.insert((Transform::from_translation(at([x, 150.0, z])).with_scale(Vec3::splat(1.1)), Visibility::Inherited)).id();
                        host.commands.entity(id).insert(super::XpEntity);
                        self.room_ents.push(id);
                    }
                }
                Feat::Pick { x, y, z, kind, .. } => {
                    let e = self.spawn_pick(host, kind, [x, y, z]);
                    self.picks.push(PickEnt { idx: i, ent: e, lid: None, base: at([x, y, z]), chest: false });
                }
                Feat::Chest { x, z, y, kind, yaw, .. } => {
                    let _ = kind;
                    let base = host.commands.spawn((super::XpEntity, scene(host.assets, CHEST_BASE), Transform::from_translation(at([x, y, z])).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(1.3)), Visibility::Inherited)).id();
                    let lid = host.commands.spawn((super::XpEntity, scene(host.assets, CHEST_LID), Transform::from_translation(at([x, y, z])).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(1.3)), Visibility::Inherited)).id();
                    self.room_ents.push(base);
                    self.picks.push(PickEnt { idx: i, ent: base, lid: Some(lid), base: at([x, y, z]), chest: true });
                    self.room_ents.push(lid);
                }
                Feat::Crack { rect, .. } => {
                    if !flags.contains(&fl) {
                        self.glyph(host, Ability::GroundPound, [rect.cx, 160.0, rect.cz]);
                        // Cracks across the plate.
                        let mat = host.unlit(Color::srgb(0.12, 0.07, 0.07));
                        for k in 0..5 {
                            let t = (k as f32 - 2.0) / 2.5;
                            let (len, ang) = (rect.w.min(rect.d) * (0.55 - 0.08 * k as f32), 0.7 * t + k as f32);
                            let m = host.meshes.add(Cuboid::new(len * SCALE, 2.0 * SCALE, 14.0 * SCALE));
                            let e = host.spawn((Mesh3d(m), MeshMaterial3d(mat.clone()), Transform::from_translation(at([rect.cx + t * rect.w * 0.3, 2.0, rect.cz + (t * 0.7).sin() * rect.d * 0.3])).with_rotation(Quat::from_rotation_y(ang))));
                            self.room_ents.push(e);
                        }
                        let mat = host.materials.add(StandardMaterial { base_color: Color::srgba(0.7, 0.2, 0.9, 0.2), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                        let m = host.meshes.add(Cuboid::new(rect.w * SCALE, 3.0 * SCALE, rect.d * SCALE));
                        let e = host.spawn((Mesh3d(m), MeshMaterial3d(mat), Transform::from_translation(at([rect.cx, 3.0, rect.cz]))));
                        self.room_ents.push(e);
                    }
                }
                Feat::Ink { rect, .. } => {
                    self.glyph(host, Ability::MetalCap, [rect.cx, 220.0, rect.cz]);
                    let mat = host.materials.add(StandardMaterial { base_color: Color::srgba(0.18, 0.05, 0.4, 0.8), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                    let m = host.meshes.add(Cuboid::new(rect.w * SCALE, 2.0 * SCALE, rect.d * SCALE));
                    let e = host.spawn((Mesh3d(m), MeshMaterial3d(mat), Transform::from_translation(at([rect.cx, -6.0, rect.cz]))));
                    self.room_ents.push(e);
                    // Lighter swirls so the pool reads as liquid.
                    let swirl = host.materials.add(StandardMaterial { base_color: Color::srgba(0.55, 0.3, 0.9, 0.35), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                    for k in 0..14 {
                        let a = k as f32 * 2.4;
                        let (px, pz) = (rect.cx + (a * 1.7).sin() * rect.w * 0.4, rect.cz + (a * 2.3).cos() * rect.d * 0.4);
                        let m = host.meshes.add(Cuboid::new(220.0 * SCALE, 1.5 * SCALE, 40.0 * SCALE));
                        let e = host.spawn((Mesh3d(m), MeshMaterial3d(swirl.clone()), Transform::from_translation(at([px, -4.5, pz])).with_rotation(Quat::from_rotation_y(a))));
                        self.room_ents.push(e);
                    }
                }
                Feat::Bars { rect, h, .. } => {
                    self.glyph(host, Ability::VanishCap, [rect.cx, h + 160.0, rect.cz]);
                    let solid = host.materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.34, 0.4), metallic: 0.6, perceptual_roughness: 0.4, ..default() });
                    let ghost = host.materials.add(StandardMaterial { base_color: Color::srgba(0.4, 0.55, 1.0, 0.22), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                    let n = (rect.w / 110.0) as i32;
                    for k in 0..=n {
                        let x = rect.cx - rect.w / 2.0 + k as f32 * rect.w / n as f32;
                        for (mat, store) in [(solid.clone(), true), (ghost.clone(), false)] {
                            let m = host.meshes.add(Cuboid::new(22.0 * SCALE, h * SCALE, 22.0 * SCALE));
                            let e = host.spawn((Mesh3d(m), MeshMaterial3d(mat), Transform::from_translation(at([x, h / 2.0, rect.cz]))));
                            self.room_ents.push(e);
                            if store { self.bars_solid.push(e) } else { self.bars_ghost.push(e) }
                        }
                    }
                    for y in [h * 0.12, h * 0.55, h - 14.0] {
                        for (mat, store) in [(solid.clone(), true), (ghost.clone(), false)] {
                            let m = host.meshes.add(Cuboid::new(rect.w * SCALE, 24.0 * SCALE, 30.0 * SCALE));
                            let e = host.spawn((Mesh3d(m), MeshMaterial3d(mat), Transform::from_translation(at([rect.cx, y, rect.cz]))));
                            self.room_ents.push(e);
                            if store { self.bars_solid.push(e) } else { self.bars_ghost.push(e) }
                        }
                    }
                }
                Feat::Gate { hint, req, .. } => self.glyph(host, req, [hint[0], hint[1] + 120.0, hint[2]]),
                Feat::Boss { .. } => {
                    if let Some((p, kind)) = boss_reward {
                        let e = self.spawn_pick(host, kind, p);
                        self.picks.push(PickEnt { idx: i, ent: e, lid: None, base: at(p), chest: false });
                    }
                }
                Feat::Painting { side, off, y, hue } => {
                    let (p, n): ([f32; 2], [f32; 2]) = match side {
                        Side::N => ([off, -hd], [0.0, 1.0]),
                        Side::S => ([off, hd], [0.0, -1.0]),
                        Side::E => ([hw, off], [-1.0, 0.0]),
                        Side::W => ([-hw, off], [1.0, 0.0]),
                    };
                    let mesh = host.meshes.add(painting_mesh(hue));
                    let mat = host.materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.9, ..default() });
                    let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(at([p[0] + n[0] * 6.0, y - 70.0, p[1] + n[1] * 6.0])).with_rotation(Quat::from_rotation_y(n[0].atan2(n[1]))).with_scale(Vec3::splat(1.0))));
                    self.room_ents.push(e);
                    self.decor.push((e, p, n));
                }
            }
        }
        // Torches on pillars.
        if host.asset_root.exists(FLAME) && def.region != Region::Garden {
            for b in def.boxes.iter().filter(|b| b.style == Style::Wall && b.y1 >= 280.0 && b.y1 < 800.0).take(4) {
                let mut e = spawn_model(host.commands, host.assets, host.asset_root, FLAME, true);
                let id = e.insert((Transform::from_translation(at([b.r.cx, b.y1 + 10.0, b.r.cz])).with_scale(Vec3::splat(2.0)), Visibility::Inherited)).id();
                host.commands.entity(id).insert(super::XpEntity);
                self.room_ents.push(id);
            }
        }
        // Light that follows the player.
        if self.light.is_none() {
            self.light = Some(host.spawn((PointLight { intensity: 5_000_000.0, range: 30.0, color: Color::srgb(1.0, 0.9, 0.75), shadow_maps_enabled: false, ..default() }, Transform::default())));
        }
    }

    /// A floating glyph in an ability's colour marking a gate that needs it (hidden once you own it).
    fn glyph(&mut self, host: &mut Host, req: Ability, pos: [f32; 3]) {
        let c = req.color();
        let col = Color::srgba(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 0.75);
        let mat = host.materials.add(StandardMaterial { base_color: col, unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
        let m = host.meshes.add(Sphere::new(26.0 * SCALE).mesh().ico(0).unwrap());
        let ring = host.meshes.add(Torus::new(30.0 * SCALE, 38.0 * SCALE));
        let root = host.spawn((Transform::from_translation(at(pos)), Visibility::Inherited));
        let g = host.commands.spawn((Mesh3d(m), MeshMaterial3d(mat.clone()), Transform::default())).id();
        let r = host.commands.spawn((Mesh3d(ring), MeshMaterial3d(mat), Transform::from_rotation(Quat::from_rotation_x(1.2)))).id();
        host.commands.entity(root).add_children(&[g, r]);
        self.room_ents.push(root);
        self.glyphs.push((req, root, at(pos)));
    }

    fn spawn_pick(&mut self, host: &mut Host, kind: PickKind, p: [f32; 3]) -> Entity {
        let root = host.spawn((Transform::from_translation(at(p)), Visibility::Inherited));
        self.room_ents.push(root);
        let mut kids: Vec<Entity> = vec![];
        let mut model = |host: &mut Host, path: &str, scale: f32, lift: f32| {
            if host.asset_root.exists(path) {
                let mut e = spawn_model(host.commands, host.assets, host.asset_root, path, true);
                let id = e.insert((Transform::from_translation(Vec3::new(0.0, lift, 0.0)).with_scale(Vec3::splat(scale)), Visibility::Inherited)).id();
                Some(id)
            } else {
                None
            }
        };
        match kind {
            PickKind::Fragment => kids.extend(model(host, "actors/star/star_geo.glb", 0.95, 0.0)),
            PickKind::Key(_) => kids.extend(model(host, "actors/small_key/small_key_geo.glb", 2.4, 0.0)),
            PickKind::HeartUp => kids.extend(model(host, "actors/heart/heart_geo.glb", 2.6, 0.0)),
            PickKind::HeartOfHalls => {
                kids.extend(model(host, "actors/star/star_geo.glb", 3.0, 0.0));
            }
            PickKind::Skill(a) => {
                let c = a.color();
                let col = Color::srgba(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 0.9);
                let mat = host.materials.add(StandardMaterial { base_color: col, unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                let gem = host.meshes.add(Sphere::new(34.0 * SCALE).mesh().ico(0).unwrap());
                let ring = host.meshes.add(Torus::new(46.0 * SCALE, 58.0 * SCALE));
                let g = host.commands.spawn((Mesh3d(gem), MeshMaterial3d(mat.clone()), Transform::from_translation(Vec3::new(0.0, 30.0 * SCALE, 0.0)))).id();
                let r = host.commands.spawn((Mesh3d(ring), MeshMaterial3d(mat), Transform::from_translation(Vec3::new(0.0, 30.0 * SCALE, 0.0)).with_rotation(Quat::from_rotation_x(1.1)))).id();
                kids.extend([g, r]);
                match a {
                    Ability::WingCap => kids.extend(model(host, "actors/mario_cap/marios_wing_cap_geo.glb", 2.6, 0.9)),
                    Ability::MetalCap => kids.extend(model(host, "actors/mario_cap/marios_metal_cap_geo.glb", 2.6, 0.9)),
                    Ability::VanishCap => kids.extend(model(host, "actors/mario_cap/marios_cap_geo.glb", 2.6, 0.9)),
                    _ => {}
                }
            }
        }
        host.commands.entity(root).add_children(&kids);
        root
    }

    // -----------------------------------------------------------------------------------------
    // Game flow
    // -----------------------------------------------------------------------------------------

    fn fresh_save() -> SaveData {
        SaveData::default()
    }

    fn apply_hooks(&mut self, host: &mut Host) {
        let mut s = self.core.save.clone();
        let mut used = false;
        if let Ok(a) = std::env::var("JUMPMAN_HALLS_ABILITIES") {
            used = true;
            for part in a.split(',') {
                let part = part.trim();
                if part.eq_ignore_ascii_case("all") {
                    for ab in ABILITIES {
                        s.abilities |= ab.bit();
                    }
                }
                for ab in ABILITIES {
                    if part.eq_ignore_ascii_case(ab.short()) {
                        s.abilities |= ab.bit();
                    }
                }
            }
        }
        if let Ok(c) = std::env::var("JUMPMAN_HALLS_CAP") {
            used = true;
            s.cap = match c.as_str() {
                "wing" => Some(Ability::WingCap),
                "metal" => Some(Ability::MetalCap),
                "vanish" => Some(Ability::VanishCap),
                _ => None,
            };
            if let Some(cap) = s.cap {
                s.abilities |= cap.bit();
            }
        }
        if let Some(n) = std::env::var("JUMPMAN_HALLS_FRAGMENTS").ok().and_then(|v| v.parse::<u16>().ok()) {
            used = true;
            for i in 0..n {
                s.fragments.insert(1000 + i);
            }
        }
        if env_flag("JUMPMAN_HALLS_KEY") {
            used = true;
            s.keys |= 1;
        }
        if let Some(m) = std::env::var("JUMPMAN_HALLS_BOSSES").ok().and_then(|v| v.parse::<u8>().ok()) {
            used = true;
            s.bosses = m;
        }
        if std::env::var("JUMPMAN_HALLS_DISCOVER").as_deref() == Ok("all") {
            used = true;
            s.discovered = (1u32 << ROOM_COUNT) - 1;
            for (ri, r) in self.core.rooms.iter().enumerate() {
                for i in 0..r.feats.len() {
                    s.seen_gates.insert(flag(ri, i));
                }
            }
        }
        if env_flag("JUMPMAN_HALLS_GOD") {
            used = true;
            self.god = true;
        }
        if let Some(d) = std::env::var("JUMPMAN_HALLS_DEMO").ok().and_then(|v| Demo::parse(&v)) {
            used = true;
            self.demo = Some(d);
        }
        let mut target: Option<usize> = None;
        if let Ok(r) = std::env::var("JUMPMAN_HALLS_ROOM") {
            used = true;
            target = r.parse::<usize>().ok().filter(|i| *i < ROOM_COUNT).or_else(|| self.core.rooms.iter().position(|d| d.name.to_lowercase().contains(&r.to_lowercase())));
        }
        if used || env_flag("JUMPMAN_HALLS_NEW") || env_flag("JUMPMAN_HALLS_POS") || env_flag("JUMPMAN_HALLS_SCREEN") || env_flag("JUMPMAN_HALLS_SHOWCASE") {
            self.ephemeral = true;
        }
        self.core.save = s;
        self.core.god = self.god;
        if let Some(r) = target {
            let spawn = self.core.rooms[r].door_spawn(0);
            self.go_to(host, r, spawn, Some(0));
        }
        if let Ok(p) = std::env::var("JUMPMAN_HALLS_POS") {
            let v: Vec<f32> = p.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() == 3 {
                self.core.player.pos = v.clone().try_into().unwrap();
                host.place_mario(world_pos([v[0], v[1], v[2]]), None);
            }
        }
        if env_flag("JUMPMAN_HALLS_SHOWCASE") {
            // One of each foe in a row in front of the start.
            let kinds = [FoeKind::Goomba, FoeKind::Bobomb, FoeKind::Spiny, FoeKind::Boo, FoeKind::Bully, FoeKind::Piranha];
            self.core.foes.clear();
            for (i, k) in kinds.iter().enumerate() {
                let x = -700.0 + i as f32 * 280.0;
                let mut f = super::halls_core::Foe::new(*k, [x, 0.0, -200.0], i as u16);
                f.state = FState::Rest;
                self.core.foes.push(f);
            }
        }
    }

    /// Begin (or continue) a game: set up the core and enter the saved room.
    fn start(&mut self, host: &mut Host, fresh: bool) {
        let save = if fresh { Self::fresh_save() } else { load_save().unwrap_or_default() };
        self.core = Halls::new(save, self.seq as u64 + 7);
        self.seq += 1;
        self.core.god = self.god;
        self.started = true;
        self.summary_sent = false;
        let room = self.core.room;
        let spawn = self.core.entry;
        self.go_to(host, room, spawn, None);
        self.apply_hooks(host);
        if fresh {
            // The very first room banner tells where you are.
            host.toast("THE PAINTED HALLS", 2.0);
        }
        self.save_dirty = true;
        host.halls.screen = Screen::None;
    }

    fn write_save(&mut self) {
        if !self.ephemeral && self.save_dirty {
            write_save(&self.core.save);
        }
        self.save_dirty = false;
    }

    fn prompt_card(&self) -> halls_view::PromptCard {
        let s = load_save();
        match s {
            Some(s) => halls_view::PromptCard { fragments: s.fragments.len() as u32, rooms: s.discovered.count_ones(), abilities: s.abilities.count_ones(), secs: s.ticks / 30, has_save: !s.is_new() },
            None => halls_view::PromptCard::default(),
        }
    }

    fn apply_cap(&self, m: &mut MarioRuntime) {
        let cap = self.core.save.cap;
        let far = 1.0e9;
        if cap == Some(Ability::WingCap) { m.wing_cap_timer = far; m.flags |= MARIO_WING_CAP } else { m.wing_cap_timer = 0.0 }
        if cap == Some(Ability::MetalCap) { m.metal_cap_timer = far; m.flags |= MARIO_METAL_CAP } else { m.metal_cap_timer = 0.0 }
        if cap == Some(Ability::VanishCap) { m.vanish_cap_timer = far; m.flags |= MARIO_VANISH_CAP } else { m.vanish_cap_timer = 0.0 }
    }

    /// Swap the collision between "bars solid" and "bars open" with the worn cap.
    fn sync_bars(&mut self, host: &mut Host) {
        if !self.has_bars {
            return;
        }
        let want = self.core.save.cap == Some(Ability::VanishCap);
        if want == self.bars_open {
            return;
        }
        if !want {
            // Never close the bars around the player.
            let p = host.sim.mario.pos;
            let me = [(p[0] - ORIGIN[0]) as f32, (p[2] - ORIGIN[2]) as f32];
            if self.core.geom.bars.iter().any(|(r, _)| Rect::new(r.cx, r.cz, r.w + 120.0, r.d + 140.0).contains(me[0], me[1])) {
                return;
            }
        }
        self.bars_open = want;
        host.sim.mario.world = self.current_world();
        // The bars slide away (or back): a gate cue, only when the player is near them.
        let mp = host.sim.mario.pos;
        host.fx(if want { "secret" } else { "cap_switch" }, mp);
    }

    fn knock(&mut self, host: &mut Host, dir: [f32; 2]) {
        let m = &mut host.sim.mario;
        let (dx, dz) = if dir == [0.0, 0.0] {
            // Ink: shoved toward the last solid ground.
            let s = self.core.safe_spot();
            let me = [(m.pos[0] - ORIGIN[0]) as f32, (m.pos[2] - ORIGIN[2]) as f32];
            let d = [s[0] - me[0], s[2] - me[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1.0);
            (d[0] / l, d[1] / l)
        } else {
            (dir[0], dir[1])
        };
        // Face the source: the knock-back actions throw him backwards.
        m.face_angle[1] = radians_to_angle((-dx).atan2(-dz) as f64) as f64;
        m.intended_yaw = m.face_angle[1];
        m.set_action(ACT_BACKWARD_AIR_KB, 0.0);
        m.forward_vel = -20.0;
        m.vel[1] = 32.0;
    }

    fn current_frame(&self, host: &Host) -> PlayerFrame {
        let m = &host.sim.mario;
        let air = m.action & ACT_FLAG_AIR != 0;
        let action = m.action;
        let started = |a: u32| action == a && self.last_action != a;
        PlayerFrame {
            pos: [(m.pos[0] - ORIGIN[0]) as f32, (m.pos[1] - ORIGIN[1]) as f32, (m.pos[2] - ORIGIN[2]) as f32],
            vel_y: m.vel[1] as f32,
            face: angle_to_radians(m.face_angle[1]) as f32,
            grounded: !air,
            punch: started(ACT_PUNCHING) || started(ACT_MOVE_PUNCHING) || started(ACT_JUMP_KICK),
            dive: started(ACT_DIVE),
            pound_land: started(ACT_GROUND_POUND_LAND),
            cap: self.core.save.cap,
        }
    }

    fn hint(&mut self, host: &mut Host, text: impl Into<String>) {
        host.halls.hint = Some((text.into(), 0.0));
        self.hint_age = 0.0;
    }

    fn handle_events(&mut self, host: &mut Host) {
        let ev = self.core.ev.clone();
        let mp = host.sim.mario.pos;
        if ev.hurt {
            self.hurt_flash = 1.0;
            host.fx("bonk", mp);
        }
        if let Some(k) = ev.knock {
            self.knock(host, k);
        }
        if ev.bounce {
            let m = &mut host.sim.mario;
            m.set_action(ACT_JUMP, 0.0);
            m.vel[1] = 46.0;
            host.fx("coin_bounce", mp);
        }
        if ev.boss_hit {
            host.fx("explosion", mp);
            host.fx("bonk", mp);
        } else if ev.hit {
            host.fx("bonk", mp);
        }
        if let Some(p) = ev.kill {
            host.fx("break_box", world_pos(p));
        }
        if let Some(p) = ev.blast {
            host.fx("break_box", world_pos(p));
            host.fx("bonk", world_pos(p));
        }
        if ev.shock {
            host.fx("bonk", mp);
        }
        if ev.heal {
            host.fx("coin", mp);
        }
        if let Some(kind) = ev.pickup {
            // Each kind of pickup has its own cue: fragments climb in pitch, hearts get the 1-up, keys and moves the jingles.
            match kind {
                PickKind::Fragment => host.shell.cues.push(crate::audio::SfxCue::RedCoin(self.core.fragments().min(8))),
                PickKind::HeartUp => host.fx("one_up", mp),
                PickKind::Key(_) => host.fx("secret", mp),
                _ => host.fx("star_sound", mp),
            }
            self.save_dirty = true;
            self.map_gen += 1;
            match kind {
                PickKind::Skill(a) => {
                    host.toast(&format!("{}!", a.name()), 2.4);
                    self.hint(host, a.blurb());
                    if a.is_cap() && self.core.save.cap == Some(a) {
                        self.bars_open = false;
                    }
                }
                PickKind::Key(_) => host.toast("THE GARDEN KEY", 2.2),
                PickKind::Fragment => host.toast(&format!("STAR FRAGMENT  {}/{}", self.core.fragments(), total_fragments()), 1.8),
                PickKind::HeartUp => host.toast("HEART CONTAINER!  MAX HEALTH UP", 2.2),
                PickKind::HeartOfHalls => host.toast("THE HEART OF THE HALLS", 2.4),
            }
        }
        if ev.chest {
            host.fx("teleport", mp);
        }
        if ev.saved {
            host.toast("PEDESTAL LIT  -  HEALTH RESTORED", 1.8);
            host.fx("teleport", mp);
            self.save_dirty = true;
        }
        if let Some(rect) = ev.cracked {
            host.fx("break_box", world_pos([rect.cx, 0.0, rect.cz]));
            host.fx("bonk", mp);
            host.toast("THE FLOOR GIVES WAY", 1.6);
            self.save_dirty = true;
            self.map_gen += 1;
        }
        if let Some(msg) = ev.blocked.clone() {
            if self.t - self.hint_ink_at > 2.0 {
                self.hint_ink_at = self.t;
                self.hint(host, msg);
            }
        }
        if ev.unlocked {
            host.shell.cues.push(crate::audio::SfxCue::CannonDoor { pos: mp });
            host.fx("secret", mp);
            host.toast("UNLOCKED", 1.4);
            self.save_dirty = true;
        }
        if ev.ink_launch && self.core.save.has(Ability::MetalCap) && self.t - self.hint_ink_at > 3.0 {
            self.hint_ink_at = self.t;
            self.hint(host, "THE INK BURNS. WEAR THE METAL CAP (F).");
        } else if ev.ink_launch && self.t - self.hint_ink_at > 3.0 {
            self.hint_ink_at = self.t;
            self.hint(host, "THE INK BURNS. SOMETHING IRON-CLAD COULD WADE IN.");
        }
        if ev.pit {
            let s = self.core.safe_spot();
            host.place_mario(world_pos([s[0], s[1] + 8.0, s[2]]), None);
            self.hint(host, "YOU FELL.");
        }
        if let Some(k) = ev.boss_start {
            host.toast(&format!("{} WAKES", k.name()), 2.0);
            host.fx("cannon_fire", mp);
        }
        if let Some(p) = ev.boss_phase {
            host.fx("cannon_fire", mp);
            host.toast(if p == 1 { "HE GROWS ANGRY!" } else { "HE IS FURIOUS!" }, 1.8);
        }
        if let Some(k) = ev.boss_down {
            host.toast(&format!("{} FALLS!", k.name()), 2.4);
            host.fx("explosion", mp);
            host.fx("star", mp);
            self.save_dirty = true;
            self.map_gen += 1;
            self.build_room(host);
        }
        if ev.drop_to.is_some() {
            host.fx("dive", mp);
        } else if ev.door.is_some() {
            host.shell.cues.push(crate::audio::SfxCue::CannonDoor { pos: mp });
        }
        if let Some((room, _zone, land)) = ev.drop_to {
            let fy = self.core.rooms[room as usize].build_geom(room as usize, &|f| self.core.save.flags.contains(&f)).floor_y(land[0], land[1], 4000.0).unwrap_or(0.0);
            self.fade = Fade::Out { t: 0.0, then: Then::Door { room: room as usize, spawn: [land[0], fy + 600.0, land[1], 0.0], from: None } };
        } else if let Some(di) = ev.door {
            let d = *self.core.def().door(di).unwrap();
            let target = self.core.rooms[d.to_room as usize].door_spawn(d.to_door as usize);
            self.fade = Fade::Out { t: 0.0, then: Then::Door { room: d.to_room as usize, spawn: target, from: Some(d.to_door as usize) } };
        }
        if ev.died {
            self.dead_at = self.t;
            host.halls.screen = Screen::Dead;
            self.save_dirty = true;
        }
        if ev.won && !self.summary_sent {
            self.summary_sent = true;
            let s = &self.core.save;
            host.halls.win = halls_view::WinCard { fragments: s.fragments.len() as u32, total: total_fragments(), secs: s.ticks / 30, deaths: s.deaths, abilities: s.abilities.count_ones(), rooms: s.discovered.count_ones() };
            host.halls.screen = Screen::Win;
            self.save_dirty = true;
        }
    }

    fn apply_cmds(&mut self, host: &mut Host) {
        let cmds: Vec<UiCmd> = std::mem::take(&mut host.halls.cmds);
        for c in cmds {
            match c {
                UiCmd::NewGame => {
                    if host.halls.screen == Screen::Prompt {
                        self.start(host, true);
                        self.fade = Fade::In { t: 0.0 };
                        host.halls.fade = 1.0;
                    }
                }
                UiCmd::Continue => {
                    if host.halls.screen == Screen::Prompt {
                        let has = load_save().is_some_and(|s| !s.is_new());
                        self.start(host, !has);
                        self.fade = Fade::In { t: 0.0 };
                        host.halls.fade = 1.0;
                    }
                }
                UiCmd::OpenMap => self.open_map(host),
                UiCmd::CloseMap => {
                    if host.halls.screen == Screen::Map {
                        host.halls.screen = Screen::None;
                    }
                }
                UiCmd::CycleCap => self.cycle_cap(host),
                UiCmd::KeepPlaying => {
                    if host.halls.screen == Screen::Win {
                        host.halls.screen = Screen::None;
                    }
                }
            }
        }
    }

    fn open_map(&mut self, host: &mut Host) {
        if !self.started || !matches!(host.halls.screen, Screen::None | Screen::Map) || !matches!(self.fade, Fade::Idle) || self.core.player.dead {
            return;
        }
        host.halls.screen = if host.halls.screen == Screen::Map { Screen::None } else { Screen::Map };
        self.publish_map(host);
    }

    fn cycle_cap(&mut self, host: &mut Host) {
        if !self.started || host.halls.screen != Screen::None || self.core.player.dead {
            return;
        }
        match self.core.cap_cycle() {
            Some(c) => host.toast(&format!("WEARING THE {}", c.name()), 1.2),
            None => {
                if self.core.save.caps().is_empty() {
                    self.hint(host, "NO CAPS YET.");
                } else {
                    host.toast("CAP OFF", 1.0);
                }
            }
        }
        host.fx("coin", host.sim.mario.pos);
        self.save_dirty = true;
    }

    fn publish_map(&mut self, host: &mut Host) {
        let b = &mut *host.halls;
        b.map = self.core.map();
        b.map_gen = self.map_gen;
        b.here = self.core.room;
    }

    fn publish(&mut self, host: &mut Host, dt: f64) {
        let a = host.alpha.clamp(0.0, 1.0) as f32;
        let t = self.t as f32;
        let me = self.core.player.pos;
        let lerp3 = |p: [f32; 3], q: [f32; 3]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a, p[2] + (q[2] - p[2]) * a];
        let core = &self.core;
        let b = &mut *host.halls;
        b.time = t;
        if b.foes.len() != FOE_SLOTS {
            b.reset();
        }
        for (i, slot) in b.foes.iter_mut().enumerate() {
            let Some(f) = core.foes.get(i).filter(|f| f.alive) else {
                slot.kind = 0;
                continue;
            };
            let prev = if self.prev_foes[i] == [0.0; 3] { f.pos } else { self.prev_foes[i] };
            let p = lerp3(prev, f.pos);
            let (scale, lift) = match f.kind {
                FoeKind::Goomba => (1.6, 0.0),
                FoeKind::Bobomb => (1.2, 0.0),
                FoeKind::Spiny => (2.0, 0.0),
                FoeKind::Boo => (1.5, 0.0),
                FoeKind::Bully => (1.2, 0.0),
                FoeKind::Piranha => (2.0, 0.0),
            };
            let hop = if matches!(f.kind, FoeKind::Goomba | FoeKind::Bobomb) && f.state == FState::Chase { (t * 12.0 + i as f32).sin().abs() * 10.0 } else { 0.0 };
            let yaw = if f.kind == FoeKind::Boo || f.dir == [0.0, 0.0] { (me[0] - p[0]).atan2(me[2] - p[2]) } else { f.dir[0].atan2(f.dir[1]) };
            let pulse = if f.state == FState::Fuse { 1.0 + 0.18 * (t * 30.0).sin().abs() } else { 1.0 };
            let squash = 1.0 + 0.15 * f.flash as f32 / 6.0;
            let shy = if f.kind == FoeKind::Boo && !f.solid { 0.8 } else { 1.0 };
            let rise = if f.kind == FoeKind::Piranha { match f.state { FState::Hide => 0.35, FState::Rise => 0.35 + 0.65 * (f.t as f32 / 18.0).min(1.0), _ => 1.0 } } else { 1.0 };
            *slot = FoeView { kind: (f.kind as u8) + 1, pos: at([p[0], p[1] + lift + hop, p[2]]), yaw, scale: scale * pulse * squash * shy * if f.kind == FoeKind::Piranha { rise } else { 1.0 }, tilt: 0.0 };
        }
        // Boss.
        b.boss = match core.boss {
            Some(bo) => {
                let prev = if self.prev_boss == [0.0; 3] { bo.pos } else { self.prev_boss };
                let p = lerp3(prev, bo.pos);
                let (kind, scale, base_tilt) = match bo.kind {
                    BossKind::Whomp => (7u8, 0.8f32, 0.0f32),
                    BossKind::KingBobomb => (8u8, 0.75, 0.0),
                };
                let tt = bo.t as f32;
                let tilt = match (bo.kind, bo.state) {
                    (BossKind::Whomp, BState::Telegraph) => -0.5 * (tt / 28.0).min(1.0),
                    (BossKind::Whomp, BState::Slam) => -0.5 + (tt / 8.0).min(1.0) * 2.07,
                    (BossKind::Whomp, BState::Prone) | (BossKind::Whomp, BState::Hurt) => 1.57,
                    (BossKind::Whomp, BState::Dying) => 1.57,
                    (BossKind::KingBobomb, BState::Charge) => 0.35,
                    (BossKind::KingBobomb, BState::Stunned) => 0.25 * (t * 9.0).sin(),
                    _ => base_tilt,
                };
                let flash = 1.0 + 0.1 * (bo.flash as f32 / 10.0);
                let sink = if bo.state == BState::Dying { 1.0 - (tt / 60.0).min(1.0) } else { 1.0 };
                let lift = if bo.state == BState::Intro { 0.0 } else { 0.0 };
                FoeView { kind, pos: at([p[0], p[1] + lift, p[2]]), yaw: bo.dir[0].atan2(bo.dir[1]), scale: scale * flash * sink, tilt }
            }
            None => FoeView::default(),
        };
        for (i, slot) in b.hazards.iter_mut().enumerate() {
            match core.hazards.get(i) {
                Some(h) => *slot = HazardView { on: true, pos: at([h.pos[0], 4.0, h.pos[2]]), radius: h.radius, frac: 1.0 - h.delay as f32 / h.total as f32, bomb: h.kind == HazardKind::Bomb },
                None => slot.on = false,
            }
        }
        for (i, slot) in b.drops.iter_mut().enumerate() {
            match core.drops.get(i) {
                Some(d) => *slot = FoeView { kind: 1, pos: at([d.pos[0], d.pos[1] + 30.0 + (t * 3.0 + i as f32).sin() * 8.0, d.pos[2]]), yaw: 0.0, scale: 1.7, tilt: 0.0 },
                None => slot.kind = 0,
            }
        }
        // HUD.
        let s = &core.save;
        let h = &mut b.hud;
        h.hp = core.player.hp;
        h.max_hp = s.max_hp;
        h.fragments = s.fragments.len() as u32;
        h.total_fragments = total_fragments();
        h.abilities = s.abilities;
        h.cap = s.cap;
        h.keys = s.keys;
        h.region = core.def().region.name();
        h.room = core.def().name;
        h.flaps = self.flaps;
        h.has_wing = s.cap == Some(Ability::WingCap);
        h.low_hp = core.player.hp <= 2 && !core.player.dead;
        h.hurt_flash = self.hurt_flash;
        h.god = self.god;
        h.boss = core.boss.map(|bo| halls_view::BossHud { frac: (bo.hp as f32 / bo.max_hp as f32).max(0.0), phase: bo.phase, name: bo.kind.name() });
        if b.here != core.room {
            b.here = core.room;
        }
        // Timers for the banner and hint.
        self.banner_age += dt as f32;
        self.hint_age += dt as f32;
        if let Some((_, age)) = &mut b.banner {
            *age = self.banner_age;
        }
        if let Some((_, age)) = &mut b.hint {
            *age = self.hint_age;
        }
        // Fade.
        b.fade = match &self.fade {
            Fade::Idle => 0.0,
            Fade::Out { t, .. } => (t / 0.22).clamp(0.0, 1.0),
            Fade::In { t } => 1.0 - (t / 0.3).clamp(0.0, 1.0),
        };
        if matches!(self.fade, Fade::Idle) && core.player.dead {
            b.fade = ((self.t - self.dead_at) as f32 - 1.0).clamp(0.0, 1.0);
        }
    }

    fn camera(&mut self, host: &mut Host) {
        let def = self.core.def();
        let tall = def.h > 1800.0;
        let focus = DVec3::from(host.render_pos()) + DVec3::new(0.0, 110.0, 0.0);
        let (yaw, pitch, dist) = (host.rig.yaw, if tall { 0.42f64 } else { 0.6f64 }, if tall { 1750.0f64 } else { 1250.0 });
        let eye = focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
        host.cam.pose = Some(CamPose { eye, look: focus, up: DVec3::Y, fov: 55.0, near: 20.0, far: 40000.0 });
        self.eye = [(eye.x - ORIGIN[0]) as f32, (eye.y - ORIGIN[1]) as f32, (eye.z - ORIGIN[2]) as f32];
        if let Some(l) = self.light {
            let p = focus + DVec3::new(0.0, 280.0, 0.0);
            host.commands.entity(l).insert(Transform::from_translation(to_bevy(p.into())));
        }
    }

    fn animate_room(&mut self, host: &mut Host) {
        let t = self.t as f32;
        let room = self.core.room;
        for (k, p) in self.picks.iter().enumerate() {
            let taken = self.core.save.flags.contains(&flag(room, p.idx));
            let vis = if taken && !p.chest { Visibility::Hidden } else { Visibility::Inherited };
            if p.chest {
                if let Some(l) = p.lid {
                    host.commands.entity(l).insert(if taken { Visibility::Hidden } else { Visibility::Inherited });
                }
                continue;
            }
            let bob = (t * 2.4 + k as f32).sin() * 12.0 * SCALE;
            host.commands.entity(p.ent).insert((vis, Transform::from_translation(p.base + Vec3::Y * (bob + 30.0 * SCALE)).with_rotation(Quat::from_rotation_y(t * 1.8 + k as f32))));
        }
        for (i, (ab, e, base)) in self.glyphs.iter().enumerate() {
            let owned = self.core.save.has(*ab);
            let bob = (t * 2.0 + i as f32).sin() * 14.0 * SCALE;
            host.commands.entity(*e).insert((if owned { Visibility::Hidden } else { Visibility::Inherited }, Transform::from_translation(*base + Vec3::Y * bob).with_rotation(Quat::from_rotation_y(t * 1.4))));
        }
        for (e, p, n) in &self.decor {
            // Wall decor is hidden when the camera is outside its wall (it would hang in front of the lens).
            let outside = (self.eye[0] - p[0]) * n[0] + (self.eye[2] - p[1]) * n[1] < 30.0;
            host.commands.entity(*e).insert(if outside { Visibility::Hidden } else { Visibility::Inherited });
        }
        for e in &self.bars_solid {
            host.commands.entity(*e).insert(if self.bars_open { Visibility::Hidden } else { Visibility::Inherited });
        }
        for e in &self.bars_ghost {
            host.commands.entity(*e).insert(if self.bars_open { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
}

impl Experiment for PaintedHallsExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Halls
    }
    fn replaces_course(&self) -> bool {
        true
    }
    /// The world stands still under a menu, a map and the fades.
    fn time_scale(&self) -> f64 {
        if !self.started || self.core.freeze { 0.0 } else { 1.0 }
    }

    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        host.rig.distance = 1250.0;
        host.rig.pitch = 0.6;
        host.halls.reset();
        host.halls.active = true;
        self.god = env_flag("JUMPMAN_HALLS_GOD");
        let hooked = ["JUMPMAN_HALLS_NEW", "JUMPMAN_HALLS_ROOM", "JUMPMAN_HALLS_ABILITIES", "JUMPMAN_HALLS_POS", "JUMPMAN_HALLS_SCREEN"].iter().any(|k| env_flag(k));
        let saved = load_save().is_some_and(|s| !s.is_new());
        if hooked {
            self.ephemeral = true;
            self.start(host, true);
        } else {
            // Show the title / continue prompt over a quiet foyer.
            self.core = Halls::new(if saved { load_save().unwrap_or_default() } else { Self::fresh_save() }, 3);
            let room = self.core.room;
            let spawn = self.core.entry;
            self.go_to(host, room, spawn, None);
            host.halls.banner = None;
            host.halls.prompt = self.prompt_card();
            host.halls.screen = Screen::Prompt;
            self.core.freeze = true;
            self.started = false;
        }
        if let Ok(s) = std::env::var("JUMPMAN_HALLS_SCREEN") {
            self.ephemeral = true;
            self.started = true;
            self.core.freeze = false;
            match s.as_str() {
                "map" => {
                    self.publish_map(host);
                    host.halls.screen = Screen::Map;
                }
                "prompt" => {
                    host.halls.prompt = halls_view::PromptCard { fragments: 5, rooms: 7, abilities: 3, secs: 3725, has_save: true };
                    host.halls.screen = Screen::Prompt;
                }
                "dead" => {
                    self.dead_at = self.t;
                    host.halls.screen = Screen::Dead;
                }
                "win" => {
                    host.halls.win = halls_view::WinCard { fragments: 12, total: total_fragments(), secs: 5231, deaths: 4, abilities: 6, rooms: 16 };
                    host.halls.screen = Screen::Win;
                }
                _ => {}
            }
        }
        if let Fade::Idle = self.fade {
            if self.started {
                self.fade = Fade::In { t: 0.0 };
            }
        }
    }

    fn exit(&mut self, host: &mut Host) {
        self.write_save();
        host.halls.active = false;
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.sim.mario.wing_cap_timer = 0.0;
        host.sim.mario.metal_cap_timer = 0.0;
        host.sim.mario.vanish_cap_timer = 0.0;
        host.set_player_visible(true);
        host.cam.fog = None;
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        match key {
            KeyCode::Tab => {
                host.halls.cmds.push(UiCmd::OpenMap);
                true
            }
            KeyCode::KeyF => {
                host.halls.cmds.push(UiCmd::CycleCap);
                true
            }
            _ => false,
        }
    }

    fn before_step(&mut self, host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
        if !self.started || self.core.freeze || self.core.player.dead {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        let mut c = controller.clone();
        let mut cam = None;
        if let Some(d) = &mut self.demo {
            c = d.controller(&host.sim.mario);
            cam = Some(0);
        }
        let abilities = self.core.save.abilities;
        let has = move |a: Ability| abilities & a.bit() != 0;
        gate_controller(&mut c, host.sim.mario.action, &has);
        // The worn cap.
        self.apply_cap(&mut host.sim.mario);
        if self.core.save.cap == Some(Ability::WingCap) && wing_flutter(&mut host.sim.mario, &c, &mut self.flaps, &mut self.boost) && c.button_pressed & A_BUTTON != 0 {
            let pos = host.sim.mario.pos;
            host.shell.cues.push(crate::audio::SfxCue::Effect { kind: "triple_jump", pos: Some(pos) });
        }
        StepOverride { controller: Some(c), camera_yaw: cam }
    }

    fn after_step(&mut self, host: &mut Host) {
        if !self.started {
            return;
        }
        {
            let m = &mut host.sim.mario;
            m.health = m.max_health;
            m.invinc_timer = 0.0;
            let abilities = self.core.save.abilities;
            undo_wall_kick(m, &move |a: Ability| abilities & a.bit() != 0);
            if m.action & ACT_FLAG_AIR == 0 || m.action == ACT_WALL_KICK_AIR {
                self.flaps = 0;
            }
        }
        if self.core.freeze || !matches!(self.fade, Fade::Idle) {
            return;
        }
        // Remember where the foes were for smooth rendering.
        for (i, f) in self.core.foes.iter().enumerate().take(FOE_SLOTS) {
            self.prev_foes[i] = f.pos;
        }
        if let Some(b) = self.core.boss {
            self.prev_boss = b.pos;
        }
        self.sync_bars(host);
        let frame = self.current_frame(host);
        self.last_action = host.sim.mario.action;
        self.core.tick(&frame);
        self.core.take_boss_reward(&frame);
        if self.core.take_dirty() {
            self.refresh_world();
            host.sim.mario.world = self.current_world();
            self.build_room(host);
        }
        self.handle_events(host);
        self.write_save();
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        self.hurt_flash = (self.hurt_flash - dt as f32 * 2.0).max(0.0);
        self.apply_cmds(host);
        // Edge buttons from the touch layer / pad.
        let edges = std::mem::take(&mut host.halls.edges);
        if edges & 1 != 0 {
            self.open_map(host);
        }
        if edges & 2 != 0 {
            self.cycle_cap(host);
        }
        // Fades and the death card advance on the render clock.
        match self.fade.clone() {
            Fade::Out { t, then } => {
                let t = t + dt as f32;
                if t >= 0.22 {
                    match then {
                        Then::Door { room, spawn, from } => {
                            self.go_to(host, room, spawn, from);
                            self.fade = Fade::In { t: 0.0 };
                        }
                        Then::Respawn => {
                            self.core.respawn();
                            let (room, spawn) = (self.core.room, self.core.entry);
                            self.go_to(host, room, spawn, None);
                            host.halls.screen = Screen::None;
                            self.fade = Fade::In { t: 0.0 };
                        }
                    }
                } else {
                    self.fade = Fade::Out { t, then };
                }
            }
            Fade::In { t } => {
                let t = t + dt as f32;
                self.fade = if t >= 0.3 { Fade::Idle } else { Fade::In { t } };
            }
            Fade::Idle => {}
        }
        if self.core.player.dead && matches!(self.fade, Fade::Idle) && self.t - self.dead_at > 2.0 {
            self.fade = Fade::Out { t: 0.0, then: Then::Respawn };
        }
        self.core.freeze = !self.started || host.halls.screen != Screen::None && host.halls.screen != Screen::Dead || !matches!(self.fade, Fade::Idle);
        if self.map_gen != host.halls.map_gen {
            self.publish_map(host);
        }
        // Brief flicker while invulnerable from a hit.
        let blink = self.core.player.iframes > 0 && !self.core.player.dead && (self.t * 18.0) as i64 % 2 == 0;
        host.set_player_visible(!blink && !self.core.player.dead);
        self.animate_room(host);
        self.publish(host, dt);
        self.camera(host);
        if let Some((_, age)) = &host.halls.hint {
            if *age > 3.4 {
                host.halls.hint = None;
            }
        }
    }

    fn debug(&self) -> String {
        format!("room={} hp={} frags={} abil={:06b} cap={:?}", self.core.def().name, self.core.player.hp, self.core.fragments(), self.core.save.abilities, self.core.save.cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_floor_and_walls_match_the_geometry() {
        let rs = rooms();
        for (ri, r) in rs.iter().enumerate() {
            let g = r.build_geom(ri, &|_| false);
            let w = build_collision(&g, [0.0, 0.0, 0.0], false);
            let s = r.door_spawn(0);
            let f = w.find_floor(s[0] as f64, s[1] as f64 + 200.0, s[2] as f64);
            assert!((f.height - s[1] as f64).abs() < 1.0, "{}: floor under the first door", r.name);
            // The outer wall stops a walker.
            let mut p = [r.w as f64 / 2.0 + 10.0, 60.0, 0.0];
            let hits = w.f32_find_wall_collision(&mut p, 60.0, 37.0);
            assert!(hits > 0 || p[0] < r.w as f64 / 2.0 + 10.0, "{}: east wall solid", r.name);
        }
    }

    #[test]
    fn bars_are_solid_until_opened() {
        let rs = rooms();
        let r = &rs[ROOM_GALLERY];
        let g = r.build_geom(ROOM_GALLERY, &|_| false);
        let solid = build_collision(&g, [0.0; 3], false);
        let open = build_collision(&g, [0.0; 3], true);
        assert!(solid.surfaces().len() > open.surfaces().len());
    }

    #[test]
    fn mesh_buffers_build_valid_meshes() {
        let mut b = Buf::default();
        b.push_box(&Rect::new(0.0, 0.0, 100.0, 200.0), 0.0, 50.0, 100.0, false, [1.0; 4]);
        assert_eq!(b.pos.len(), 20);
        assert_eq!(b.idx.len(), 30);
        let mesh = b.into_mesh();
        assert_eq!(mesh.count_vertices(), 20);
        assert!(painting_mesh(0.3).count_vertices() > 20);
    }

    #[test]
    fn every_model_the_halls_use_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !root.exists() {
            return;
        }
        for m in halls_view::MODELS.iter().skip(1) {
            assert!(root.join(m).exists(), "{m}");
        }
        for m in [halls_view::HEART_MODEL, FLAME, CHEST_BASE, CHEST_LID, "actors/door/wooden_door_geo.glb", "actors/door/key_door_geo.glb", "actors/door/castle_door_3_stars_geo.glb", "actors/door/metal_door_geo.glb", "actors/star/star_geo.glb", "actors/small_key/small_key_geo.glb", "actors/mario_cap/marios_wing_cap_geo.glb", "actors/mario_cap/marios_metal_cap_geo.glb", "actors/mario_cap/marios_cap_geo.glb"] {
            assert!(root.join(m).exists(), "{m}");
        }
        for r in rooms() {
            let l = r.region.look();
            for t in [l.floor, l.wall, l.plat] {
                assert!(super::super::textures::texture_path(t).is_some(), "texture {t}");
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Physical proof that the gates are what the data says: the real Mario controller on each room's collision.
    // -----------------------------------------------------------------------------------------

    struct Bot {
        m: MarioRuntime,
        abilities: u8,
        flaps: u8,
        boost: f64,
        frame: u32,
        log: Vec<String>,
    }

    /// Stick for a desired world yaw (radians) with camera yaw 0.
    fn stick(yaw: f64) -> (f64, f64) {
        let theta = angle_to_radians(radians_to_angle(yaw) as f64);
        // SM64's atan2s(y, x) measures from +y: intended yaw = atan2(stick_x, -stick_y) + camera yaw.
        (theta.sin() * 64.0, -theta.cos() * 64.0)
    }

    impl Bot {
        fn new(room: usize, abilities: u8, at: [f64; 3]) -> Self {
            let rs = rooms();
            let g = rs[room].build_geom(room, &|_| false);
            let world = Arc::new(build_collision(&g, [0.0; 3], false));
            let spawn = SpawnPoint { action: None, pos: at, face_yaw: 0.0 };
            let mut m = MarioRuntime::new(world, spawn, -1e9);
            for _ in 0..10 {
                m.step(&ControllerSnapshot::default(), 0.0, false, false);
            }
            Self { m, abilities, flaps: 0, boost: 0.0, frame: 0, log: vec![] }
        }
        fn has(&self) -> impl Fn(Ability) -> bool {
            let a = self.abilities;
            move |x: Ability| a & x.bit() != 0
        }
        /// One frame: steer toward `yaw` (None = no stick), buttons as pressed this frame / held.
        fn step(&mut self, yaw: Option<f64>, a_press: bool, a_down: bool, z_press: bool, z_down: bool) {
            let (sx, sy, mag) = match yaw {
                Some(y) => {
                    let (x, y) = stick(y);
                    (x, y, 64.0)
                }
                None => (0.0, 0.0, 0.0),
            };
            let mut c = ControllerSnapshot { focus_yaw: None, stick_x: sx, stick_y: sy, stick_mag: mag, button_down: if a_down { A_BUTTON } else { 0 } | if z_down { Z_TRIG } else { 0 }, button_pressed: if a_press { A_BUTTON } else { 0 } | if z_press { Z_TRIG } else { 0 } };
            let has = self.has();
            gate_controller(&mut c, self.m.action, &has);
            if self.abilities & Ability::WingCap.bit() != 0 {
                self.m.wing_cap_timer = 1e9;
                self.m.flags |= MARIO_WING_CAP;
                wing_flutter(&mut self.m, &c, &mut self.flaps, &mut self.boost);
            }
            self.m.step(&c, 0.0, false, false);
            undo_wall_kick(&mut self.m, &has);
            if self.m.action & ACT_FLAG_AIR == 0 || self.m.action == ACT_WALL_KICK_AIR {
                self.flaps = 0;
            }
            self.frame += 1;
            self.log.push(format!("f{} act={:x} pos=({:.0},{:.0},{:.0}) fv={:.1} vy={:.1}", self.frame, self.m.action, self.m.pos[0], self.m.pos[1], self.m.pos[2], self.m.forward_vel, self.m.vel[1]));
        }
        fn tail(&self, n: usize) -> String {
            self.log.iter().rev().take(n).rev().cloned().collect::<Vec<_>>().join("\n")
        }
        fn airborne(&self) -> bool {
            self.m.action & ACT_FLAG_AIR != 0
        }
    }

    const NORTH: f64 = std::f64::consts::PI;
    const EAST: f64 = std::f64::consts::FRAC_PI_2;
    const WEST: f64 = -std::f64::consts::FRAC_PI_2;

    /// Run north from `start_z` and jump once past `jump_z`; `long` crouches for the long jump.
    fn run_and_jump(room: usize, start_z: f64, jump_z: f64, long: bool, has_lj: bool) -> ([f64; 3], bool, String) {
        let ab = if has_lj { Ability::LongJump.bit() } else { 0 };
        let mut b = Bot::new(room, ab, [0.0, 0.0, start_z]);
        let mut jumped = false;
        let mut landed_after = false;
        for _ in 0..400 {
            if !jumped {
                if b.m.pos[2] > jump_z {
                    b.step(Some(NORTH), false, false, false, false);
                    continue;
                }
                if long {
                    b.step(Some(NORTH), false, false, true, true);
                    b.step(Some(NORTH), true, true, false, true);
                } else {
                    b.step(Some(NORTH), true, true, false, false);
                }
                jumped = true;
                continue;
            }
            b.step(Some(NORTH), false, false, false, long);
            if !b.airborne() && b.frame > 30 {
                landed_after = true;
                break;
            }
        }
        (b.m.pos, landed_after, b.tail(40))
    }

    fn hedge_crossing(long: bool, has_lj: bool) -> ([f64; 3], bool, String) {
        run_and_jump(ROOM_HEDGE, -60.0, -560.0, long, has_lj)
    }

    #[test]
    fn the_gear_foyer_gap_needs_the_long_jump_too() {
        // The gears are open from z = 700 to z = -200; the far side holds the door north.
        let (pos, landed, tail) = run_and_jump(ROOM_GEARS, 1450.0, 850.0, true, true);
        assert!(landed && pos[2] < -200.0 && pos[1] > -10.0, "long jump clears the gears: {pos:?}\n{tail}");
        let (pos, _, _) = run_and_jump(ROOM_GEARS, 1450.0, 850.0, false, true);
        assert!(pos[2] > -200.0, "a plain jump falls in: {pos:?}");
    }

    /// Climb with flaps from `start` holding near `hold` (x, z) until above `clear_y`, then move to `dest`; best (height, ended on top).
    fn flutter_to(room: usize, wing: bool, start: [f64; 3], hold: [f64; 2], dest: [f64; 2], clear_y: f64, goal_y: f64) -> (f64, String) {
        let mut b = Bot::new(room, if wing { Ability::WingCap.bit() } else { 0 }, start);
        let mut best = 0.0f64;
        let mut last = 0u32;
        for i in 0..500u32 {
            let press = if !b.airborne() { i < 3 } else { b.m.vel[1] < 6.0 && i - last >= 6 };
            if press {
                last = i;
            }
            let high = b.m.pos[1] > clear_y;
            let target = if high { dest } else { hold };
            let (dx, dz) = (target[0] - b.m.pos[0], target[1] - b.m.pos[2]);
            let yaw = if dx.hypot(dz) < 40.0 { None } else { Some(dx.atan2(dz)) };
            b.step(yaw, press, press, false, false);
            best = best.max(b.m.pos[1]);
            if b.m.pos[1] > goal_y - 10.0 && !b.airborne() {
                break;
            }
        }
        (best, b.tail(40))
    }

    #[test]
    fn the_clock_shaft_climb_needs_wall_kicks_then_the_wing() {
        let (best, kicks, tail) = climb_shaft(ROOM_SHAFT, [-110.0, 0.0, 100.0], true, 1100.0);
        assert!(best >= 1095.0, "wall kicks reach the mid ledge (best {best}, {kicks} kicks)\n{tail}");
        let (best, _, _) = climb_shaft(ROOM_SHAFT, [-110.0, 0.0, 100.0], false, 1100.0);
        assert!(best < 700.0, "not without the ability (best {best})");
        // From the mid ledge: only the wing reaches the high slab beside the north door.
        let (with, tail) = flutter_to(ROOM_SHAFT, true, [400.0, 1100.0, -200.0], [300.0, -200.0], [0.0, -550.0], 2000.0, 1950.0);
        assert!(with >= 1940.0, "flutter lands on the 1950 slab (best {with})\n{tail}");
        let (without, _) = flutter_to(ROOM_SHAFT, false, [400.0, 1100.0, -200.0], [300.0, -200.0], [0.0, -550.0], 2000.0, 1950.0);
        assert!(without < 1500.0, "a plain jump from the ledge cannot (best {without})");
    }

    #[test]
    fn the_hedge_run_gap_needs_the_long_jump() {
        // With the ability: lands on the north island (z beyond -1700, on the floor).
        let (pos, landed, tail) = hedge_crossing(true, true);
        assert!(landed && pos[2] < -1700.0 && pos[1] > -10.0, "long jump should clear the gap: landed {landed} at {pos:?}\n{tail}");
        // The same run and a plain jump falls into the hole.
        let (pos, _, _) = hedge_crossing(false, true);
        assert!(pos[2] > -1700.0, "a plain jump cannot clear it: {pos:?}");
        // Without the ability the crouch input is stripped, so even the long-jump inputs fall short.
        let (pos, _, _) = hedge_crossing(true, false);
        assert!(pos[2] > -1700.0, "no long jump without the sigil: {pos:?}");
    }

    fn dir_sign(d: f64) -> f64 {
        if d == EAST { 1.0 } else { -1.0 }
    }

    /// The wall-kick climb: run at one column, jump, kick off it, cross to the other, repeat.
    fn climb_shaft(room: usize, start: [f64; 3], has_wk: bool, goal_y: f64) -> (f64, u32, String) {
        let ab = if has_wk { Ability::WallKick.bit() } else { 0 };
        let mut b = Bot::new(room, ab, start);
        let mut dir = EAST;
        let mut best = 0.0f64;
        let mut kicks = 0;
        for _ in 0..1500 {
            let air = b.airborne();
            let wall_x = if dir == EAST { 170.0 } else { -170.0 };
            if b.m.action == ACT_AIR_HIT_WALL {
                // Kick off, then head for the opposite column.
                dir = if dir == EAST { WEST } else { EAST };
                b.step(Some(dir), true, true, false, false);
                if b.m.action == ACT_WALL_KICK_AIR {
                    kicks += 1;
                }
            } else if !air {
                // Run up (a wall kick needs some speed), then jump at the wall.
                if (wall_x - dir_sign(dir) * 50.0 - b.m.pos[0]).abs() > 130.0 {
                    b.step(Some(dir), false, false, false, false);
                } else {
                    b.step(Some(dir), true, true, false, false);
                }
            } else {
                b.step(Some(dir), false, false, false, false);
            }
            best = best.max(b.m.pos[1]);
            if b.m.pos[1] >= goal_y - 5.0 && !b.airborne() {
                break;
            }
        }
        (best, kicks, b.tail(30))
    }

    #[test]
    fn the_greenhouse_shaft_needs_the_wall_kick() {
        let (best, kicks, tail) = climb_shaft(ROOM_GREENHOUSE, [-110.0, 0.0, 0.0], true, 1000.0);
        assert!(best >= 995.0, "wall kicks should reach the column tops (best {best}, {kicks} kicks)\n{tail}");
        let (best, _, _) = climb_shaft(ROOM_GREENHOUSE, [-110.0, 0.0, 0.0], false, 1000.0);
        assert!(best < 700.0, "without the ability the shaft is out of reach (best {best})");
    }

    #[test]
    fn the_foyer_ledge_needs_the_wing_flutter() {
        // From beside the ledge: jump, then flutter toward it.
        let attempt = |wing: bool| -> (f64, String) {
            let mut b = Bot::new(ROOM_FOYER, if wing { Ability::WingCap.bit() } else { 0 }, [250.0, 0.0, 300.0]);
            let mut best = 0.0f64;
            let mut last = 0u32;
            for i in 0..400u32 {
                // Jump; in the air flutter whenever the rise has run out.
                let press = if !b.airborne() { i < 3 } else { b.m.vel[1] < 6.0 && i - last >= 6 };
                if press {
                    last = i;
                }
                // Hold position beside the ledge while climbing, then slide onto it.
                let target = if b.m.pos[1] < 1010.0 && b.m.pos[0] < 450.0 { 320.0 } else { 700.0 };
                b.step(Some(if b.m.pos[0] < target { EAST } else { WEST }), press, press, false, false);
                best = best.max(b.m.pos[1]);
                if b.m.pos[1] > 940.0 && !b.airborne() {
                    break;
                }
            }
            (best, b.log.iter().take(70).cloned().collect::<Vec<_>>().join("\n"))
        };
        let (with, tail) = attempt(true);
        let (without, _) = attempt(false);
        assert!(with >= 940.0, "three flutters carry Mario onto the 950 ledge (best {with})\n{tail}");
        assert!(without < 400.0, "without the cap a jump cannot (best {without})");
    }

    #[test]
    fn a_triple_jump_cannot_replace_the_wing() {
        // The best the plain controller manages: a running triple jump; it must stay under the ledge heights.
        let mut b = Bot::new(ROOM_FOYER, 0, [-900.0, 0.0, 0.0]);
        for _ in 0..30 {
            b.step(Some(EAST), false, false, false, false);
        }
        let mut best = 0.0f64;
        let mut jumps = 0;
        for _ in 0..200 {
            let press = !b.airborne() && b.m.forward_vel > 18.0 && jumps < 3;
            if press {
                jumps += 1;
            }
            b.step(Some(EAST), press, press, false, false);
            best = best.max(b.m.pos[1]);
        }
        assert!(best > 400.0, "the triple jump really is high (best {best})");
        assert!(best < 800.0, "but short of the 890 underside of the wing ledge (best {best})");
    }

    #[test]
    fn the_ground_pound_input_exists_only_with_the_ability() {
        let pounded = |has: bool| -> bool {
            let mut b = Bot::new(ROOM_CRYPT, if has { Ability::GroundPound.bit() } else { 0 }, [-200.0, 0.0, 800.0]);
            b.step(None, true, true, false, false);
            for _ in 0..6 {
                b.step(None, false, false, false, false);
            }
            b.step(None, false, false, true, true);
            let mut landed = false;
            for _ in 0..40 {
                b.step(None, false, false, false, false);
                landed |= b.m.action == ACT_GROUND_POUND_LAND;
            }
            landed
        };
        assert!(pounded(true));
        assert!(!pounded(false));
    }

    #[test]
    fn iron_bars_stop_mario_unless_the_vanish_cap_opens_them() {
        let rs = rooms();
        let g = rs[ROOM_GALLERY].build_geom(ROOM_GALLERY, &|_| false);
        let walk = |open: bool| -> f64 {
            let world = Arc::new(build_collision(&g, [0.0; 3], open));
            let mut m = MarioRuntime::new(world, SpawnPoint { action: None, pos: [0.0, 0.0, -100.0], face_yaw: 0.0 }, -1e9);
            for _ in 0..10 {
                m.step(&ControllerSnapshot::default(), 0.0, false, false);
            }
            for _ in 0..90 {
                let (sx, sy) = stick(NORTH);
                m.step(&ControllerSnapshot { focus_yaw: None, stick_x: sx, stick_y: sy, stick_mag: 64.0, button_down: 0, button_pressed: 0 }, 0.0, false, false);
            }
            m.pos[2]
        };
        assert!(walk(false) > -300.0, "solid bars block");
        assert!(walk(true) < -600.0, "open bars let him walk through");
    }
}
