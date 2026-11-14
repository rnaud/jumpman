//! Relic Descent: an original action-RPG "looter dungeon crawler" on Mario's real physics.
//!
//! You (Mario, Link, Young Link or Banjo) descend three floors of linked rooms (stretches of Ocarina
//! of Time's real dungeons when the player has imported them, `relic_real.rs`; generated ones
//! otherwise), wake packs of SM64 critters, dodge with the dive (its i-frames are the dodge), slam the
//! ground with the ground pound, fight a three-phase Bomb King on the last floor and fill your
//! pack with randomly rolled gear. The rules live in `relic_core.rs` (deterministic, seeded, unit
//! tested), loot in `relic_loot.rs`, floors in `relic_dungeon.rs`; this file glues them to the
//! host: it builds each floor's collision and meshes, maps Mario's actions to combat events
//! (punch/slash/claw = swing, dive = dodge, ground-pound landing = Quake Slam), feeds the core
//! the player's position every 30 Hz tick and publishes render lists and HUD numbers through
//! [`RelicBridge`]. `relic_view.rs` / `relic_ui.rs` draw them.
//!
//! Controls: stick moves, A jumps, B attacks (run + B dives = dodge), Z in the air pounds, the
//! touch ATK / DODGE / skill buttons, keys `Enter` (hold) attack, `B` dodge, `1`-`4` skills,
//! `H` potion, `I` pack, `T` skill tree, `Y` loot filter; gamepad d-pad skills / potion, RB attack,
//! Select pack, L-stick click tree.
//!
//! Test hooks (env): `JUMPMAN_RELIC_SEED`, `_FLOOR`, `_LEVEL`, `_GOD=1`, `_GIVE=legendary:weapon,...`,
//! `_BAG=<n>`, `_SCREEN=bag|tree`, `_TELEPORT=boss|stairs|treasure|combat|<room>`, `_BOT=1`
//! (walks to and fights the nearest foe), `_END=win|lose`, `_PROCEDURAL=1` (generated floors),
//! `_SITES=deku_hollow,forest_cellar,phantom_lair` (real sites per floor).
use super::geometry::{add_box_collision, add_face};
use super::relic_core::*;
use super::relic_dungeon::*;
use super::relic_loot::*;
use super::relic_view::*;
use super::foes_view::FoeItem;
use super::swarm_core::Rng;
use super::swarm_view::RItem;
use super::textures::textured;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::{scene, to_bevy, SCALE};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::math::{angle_to_radians, radians_to_angle, to_s16};
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::state::*;
use std::sync::Arc;

/// Bevy position of a floor-plane point `lift` above the floor there.
fn at(d: &Dungeon, p: [f32; 2], lift: f32) -> Vec3 {
    to_bevy(d.world(p, lift))
}

/// Course position of a floor-plane point on the floor.
fn world_pos(d: &Dungeon, p: [f32; 2]) -> [f64; 3] {
    d.world(p, 0.0)
}

/// Minimap frame of a floor: the tile offset and scale that fit its rooms into the generated grid's `W` x `H` (the
/// minimap's own size); generated floors map one to one.
fn mini_frame(d: &Dungeon) -> ([f32; 2], f32) {
    if d.real.is_none() {
        return ([0.0; 2], 1.0);
    }
    let (x0, y0) = (d.rooms.iter().map(|r| r.x).min().unwrap_or(0), d.rooms.iter().map(|r| r.y).min().unwrap_or(0));
    let (x1, y1) = (d.rooms.iter().map(|r| r.x + r.w).max().unwrap_or(1), d.rooms.iter().map(|r| r.y + r.h).max().unwrap_or(1));
    let k = ((W as f32 - 2.0) / (x1 - x0).max(1) as f32).min((H as f32 - 2.0) / (y1 - y0).max(1) as f32);
    ([x0 as f32 - 1.0 / k, y0 as f32 - 1.0 / k], k)
}

const WALL_H: f32 = 330.0;
const SUMMARY_LOCKOUT: f64 = 1.1;
const FLAME: &str = "actors/flame/red_flame_geo.glb";
/// Drops and labels beyond this distance from the player are not drawn.
const DRAW_RANGE: f32 = 3600.0;

// ---------------------------------------------------------------------------------------------
// Persistent meta progression
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meta {
    pub stash: u32,
    pub best_floor: u32,
    pub victories: u32,
    pub filter_commons: bool,
}

impl Default for Meta {
    fn default() -> Self {
        Self { stash: 0, best_floor: 0, victories: 0, filter_commons: true }
    }
}

fn meta_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("relic.json")
}

pub fn load_meta() -> Meta {
    let Some(v) = std::fs::read_to_string(meta_path()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else { return Meta::default() };
    Meta { stash: v["stash"].as_u64().unwrap_or(0) as u32, best_floor: v["best_floor"].as_u64().unwrap_or(0) as u32, victories: v["victories"].as_u64().unwrap_or(0) as u32, filter_commons: v["filter_commons"].as_bool().unwrap_or(true) }
}

pub fn save_meta(m: &Meta) {
    let path = meta_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "stash": m.stash, "best_floor": m.best_floor, "victories": m.victories, "filter_commons": m.filter_commons }).to_string());
}

/// What the first run teaches, one lesson at a time, when the situation first calls for it (a contextual
/// tutorial instead of a wall of text before the first room).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LessonCue {
    pub foes_awake: bool,
    pub hurt: bool,
    pub got_item: bool,
    pub level_up: bool,
    pub low_hp: bool,
    pub potions: u32,
}

pub const LESSON_FIGHT: u8 = 1;
pub const LESSON_DODGE: u8 = 2;
pub const LESSON_PACK: u8 = 4;
pub const LESSON_SKILLS: u8 = 8;
pub const LESSON_POTION: u8 = 16;

/// The lesson (bit) to show now, if any: each is shown once, the most urgent first.
pub fn next_lesson(taught: u8, c: LessonCue) -> Option<u8> {
    let want = [(c.low_hp && c.potions > 0, LESSON_POTION), (c.hurt, LESSON_DODGE), (c.foes_awake, LESSON_FIGHT), (c.level_up, LESSON_SKILLS), (c.got_item, LESSON_PACK)];
    want.iter().find(|(on, bit)| *on && taught & bit == 0).map(|(_, bit)| *bit)
}

/// Gold banked at the end of a run: all of it for a win, 60% for a death.
pub fn banked_gold(run_gold: u32, victory: bool) -> u32 {
    if victory { run_gold } else { run_gold * 3 / 5 }
}

fn run_seed() -> u64 {
    std::env::var("JUMPMAN_RELIC_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64))
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// Per-kind display: model scale and hover height.
fn look(kind: EnemyKind) -> (f32, f32) {
    match kind {
        EnemyKind::Scamp => (2.0, 0.0),
        EnemyKind::Bruiser => (1.4, 0.0),
        EnemyKind::Lancer => (3.0, 0.0),
        EnemyKind::Wraith => (1.0, 60.0),
        EnemyKind::Sapper => (3.4, 0.0),
        EnemyKind::Seer => (1.1, 90.0),
        EnemyKind::King => (1.2, 0.0),
        // The ported cast is drawn by `foes_view` (sized from its own radius).
        _ => (1.0, 0.0),
    }
}

// ---------------------------------------------------------------------------------------------
// Mesh building: one wall mesh and a few floor meshes per floor
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct MeshBuf {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl MeshBuf {
    /// A box (Bevy units), UVs tiling every `tile` units along each face.
    fn push_box(&mut self, c: Vec3, size: Vec3, tile: f32) {
        let h = size / 2.0;
        let faces: [(Vec3, Vec3); 6] = [(Vec3::X, Vec3::Y), (Vec3::NEG_X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::NEG_Y, Vec3::Z), (Vec3::Z, Vec3::Y), (Vec3::NEG_Z, Vec3::Y)];
        for (n, u) in faces {
            let r = u.cross(n);
            let (hn, hr, hu) = ((n * h).length(), (r * h).length(), (u * h).length());
            let base = self.pos.len() as u32;
            for (sr, su) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = c + n * hn + r * (sr * hr) + u * (su * hu);
                self.pos.push(p.to_array());
                self.nor.push(n.to_array());
                self.uv.push([(sr * hr + hr) / tile, (hu - su * hu) / tile]);
            }
            self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// A horizontal quad facing up, UVs continuous in world units.
    fn push_floor(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, tile: f32) {
        let base = self.pos.len() as u32;
        for (x, z) in [(x0, z1), (x1, z1), (x1, z0), (x0, z0)] {
            self.pos.push([x, y, z]);
            self.nor.push([0.0, 1.0, 0.0]);
            self.uv.push([x / tile, z / tile]);
        }
        self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        mesh.insert_indices(Indices::U32(self.idx));
        mesh
    }

    fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }
}

pub struct RelicDescentExperiment {
    core: Relic,
    seed: u64,
    runs: u32,
    /// Seconds since entering (render clock, keeps running while paused).
    t: f64,
    meta: Meta,
    floor_ents: Vec<Entity>,
    light: Option<Entity>,
    built_floor: u32,
    intent: Intent,
    pending: u16,
    last_action: u32,
    leaping: bool,
    spin_t: u8,
    over_at: f64,
    summary_sent: bool,
    sfx_tick: u32,
    yaws: Vec<f32>,
    first_run: bool,
    shown_gen: u32,
    bag_full_at: f64,
    /// Game feel: camera shake, HUD punches, and what each foe looked like last tick (alive, flash) so
    /// hits and deaths can throw sparks without the core knowing.
    shake: super::feel::Shake,
    hurt_pop: super::feel::Pop,
    gain_pop: super::feel::Pop,
    seen: Vec<(bool, u8)>,
    /// Test hooks read once at entry.
    god: bool,
    bot: bool,
    /// Lessons already shown ([`next_lesson`]); a veteran (a cleared floor 2 or a win on record) starts with all.
    taught: u8,
    lesson_at: f64,
    /// Why this run fell back to generated floors (shown once at the start).
    no_real: Option<String>,
    /// Last spot Mario stood on the floor plan (a fall off a real floor puts him back there).
    safe: [f64; 3],
    /// Arrival messages shown one after another (a toast replaces the one before it).
    queued: std::collections::VecDeque<(String, f32)>,
    queue_at: f64,
    /// Real floors: the camera's eased (pitch, distance).
    cam: (f64, f64),
    /// Hitstop: real seconds the world stays frozen after a hit, a kill or a hurt (game feel).
    hitstop: f64,
    /// The scripted player (`JUMPMAN_RELIC_BOT`): where it stood and for how many steps it has not moved.
    bot_last: [f32; 2],
    bot_stuck: u32,
}

/// Hitstop for one tick's events, in seconds (the checklist: 2-6 frames at 30 Hz; a hit lands with a beat, a kill or a
/// crit with two, a hurt with three, the boss with more).
pub fn hitstop_for(ev: &Events) -> f64 {
    let tick = 1.0 / TICKS_PER_SEC as f64;
    if ev.boss_killed {
        0.3
    } else if ev.boss_phase.is_some() {
        0.18
    } else if ev.hurt {
        3.0 * tick
    } else if ev.kills > 0 || ev.crit {
        2.0 * tick
    } else if ev.hit > 0 {
        tick
    } else {
        0.0
    }
}

/// A toast may show at once only when no queued message (a floor's arrival lines) is still on screen; otherwise it
/// waits its turn so the arrival is never cut short.
pub fn toast_slot_free(queued: usize, t: f64, queue_at: f64) -> bool {
    queued == 0 && t >= queue_at
}

fn rarity_hex(r: Rarity) -> u32 {
    let c = RARITIES[(r as usize).min(4)].color();
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

impl RelicDescentExperiment {
    pub fn new() -> Self {
        let seed = run_seed();
        Self {
            core: Relic::new(seed, Hero::Mario, 1, 0),
            seed,
            runs: 0,
            t: 0.0,
            meta: load_meta(),
            floor_ents: vec![],
            light: None,
            built_floor: 0,
            intent: Intent::default(),
            pending: 0,
            last_action: 0,
            leaping: false,
            spin_t: 0,
            over_at: 0.0,
            summary_sent: false,
            sfx_tick: 0,
            yaws: vec![0.0; ENEMY_SLOTS],
            first_run: true,
            shown_gen: 0,
            bag_full_at: -10.0,
            shake: Default::default(),
            hurt_pop: Default::default(),
            gain_pop: Default::default(),
            seen: vec![(false, 0); ENEMY_SLOTS],
            god: env_flag("JUMPMAN_RELIC_GOD"),
            bot: env_flag("JUMPMAN_RELIC_BOT"),
            taught: 0,
            lesson_at: -10.0,
            no_real: None,
            safe: [0.0; 3],
            queued: Default::default(),
            queue_at: 0.0,
            cam: (0.95, 1500.0),
            hitstop: 0.0,
            bot_last: [0.0; 2],
            bot_stuck: 0,
        }
    }

    /// Collision and display for the current floor. Returns the collision world.
    fn build_floor(&mut self, host: &mut Host) -> CollisionWorld {
        for e in self.floor_ents.drain(..) {
            host.commands.entity(e).despawn();
        }
        let world = if self.core.dungeon.real.is_some() { self.build_real(host) } else { self.build_generated(host) };
        let dg = &self.core.dungeon;
        // Stairs: a gold ring with a column of light.
        if let Some(s) = self.core.stairs_pos() {
            let ring = host.meshes.add(Annulus::new(150.0 * SCALE, 185.0 * SCALE));
            let disc = host.meshes.add(Circle::new(150.0 * SCALE));
            let col = host.meshes.add(Cylinder::new(120.0 * SCALE, 900.0 * SCALE));
            let gold = host.unlit(Color::srgb(1.0, 0.82, 0.25));
            let soft = host.materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.9, 0.5, 0.22), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
            let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            for (mesh, mat, y, rot) in [(ring, gold.clone(), 4.0, flat), (disc, soft.clone(), 3.0, flat), (col, soft, 450.0, Quat::IDENTITY)] {
                let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(at(dg, s, y)).with_rotation(rot)));
                self.floor_ents.push(e);
            }
        }
        // Boss arena: a ring of dark light on the floor at the centre.
        if dg.boss_floor {
            let c = dg.rooms[dg.exit].center();
            let ring = host.meshes.add(Annulus::new(520.0 * SCALE, 560.0 * SCALE));
            let mat = host.unlit(Color::srgb(0.9, 0.2, 0.25));
            let e = host.spawn((Mesh3d(ring), MeshMaterial3d(mat), Transform::from_translation(at(dg, c, 4.0)).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))));
            self.floor_ents.push(e);
        }
        self.built_floor = self.core.floor;
        world
    }

    /// A real floor: the scene's own model and collision, fenced along the edge of the floor plan (`relic_real::fence`)
    /// so Mario stays where the foes can follow; a torch burns in every room's open middle.
    fn build_real(&mut self, host: &mut Host) -> CollisionWorld {
        let dg = &self.core.dungeon;
        let src = dg.real.clone().expect("a real floor");
        let root = host.asset_root.0.clone();
        let (mut world, model) = match super::relic_real::scene_world(&root, &src.course, src.area) {
            Ok(x) => x,
            Err(e) => {
                // The plan was built from this very file a moment ago; if it vanished, give Mario a floor to stand on.
                warn!("relic: scene {} failed to load: {e}", src.course);
                let mut w = CollisionWorld::new(1024.0, 1e9);
                let o = dg.origin;
                add_box_collision(&mut w, [o[0] + dg.w as f64 * dg.tile as f64 / 2.0, o[1] - 200.0, o[2] + dg.h as f64 * dg.tile as f64 / 2.0], [dg.w as f64 * dg.tile as f64, 400.0, dg.h as f64 * dg.tile as f64], 0.0, &mut vec![], &SurfaceOptions::default());
                (w, String::new())
            }
        };
        let fence_opts = SurfaceOptions { flags: jumpman_sim::surfaces::SURFACE_FLAG_NO_CAM_COLLISION, ..Default::default() };
        let mut faces = vec![];
        for (q, n) in super::relic_real::fence(dg) {
            add_face(&mut world, &q, n, &mut faces, &fence_opts);
        }
        world.finalize();
        if !model.is_empty() && host.asset_root.exists(&model) {
            let scale = crate::zelda::glb::bevy_scale(&root.join(&model));
            let e = host.spawn((scene(host.assets, &model), Transform::from_scale(Vec3::splat(scale)), Visibility::Inherited));
            self.floor_ents.push(e);
        }
        if host.asset_root.exists(FLAME) {
            for (i, r) in dg.rooms.iter().enumerate() {
                if i == dg.start || r.kind == RoomKind::Boss {
                    continue;
                }
                let e = host.spawn((scene(host.assets, FLAME), Transform::from_translation(at(dg, r.center(), 40.0)).with_scale(Vec3::splat(2.2)), Visibility::Inherited));
                self.floor_ents.push(e);
            }
        }
        world
    }

    /// A generated floor: brick walls round the rooms and corridors, pillars and torches.
    fn build_generated(&mut self, host: &mut Host) -> CollisionWorld {
        let d = &self.core.dungeon;
        let dg = d;
        let org = d.origin;
        let tile = d.tile as f64;
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let o = SurfaceOptions::default();
        let mut faces = vec![];
        let total = [d.w as f64 * tile, d.h as f64 * tile];
        // One big floor slab (walls fence the void), and a box per merged wall run.
        add_box_collision(&mut world, [org[0] + total[0] / 2.0, org[1] - 200.0, org[2] + total[1] / 2.0], [total[0] + 3000.0, 400.0, total[1] + 3000.0], 0.0, &mut faces, &o);
        let mut walls = MeshBuf::default();
        for b in d.wall_boxes() {
            let (ox, oy) = box_origin(&b);
            let (w, h) = (b.w as f64 * tile, b.h as f64 * tile);
            let (cx, cz) = (ox as f64 * tile + w / 2.0, oy as f64 * tile + h / 2.0);
            add_box_collision(&mut world, [org[0] + cx, org[1] + 350.0, org[2] + cz], [w, 700.0, h], 0.0, &mut faces, &o);
            walls.push_box(to_bevy([org[0] + cx, org[1] + WALL_H as f64 / 2.0, org[2] + cz]), Vec3::new(w as f32, WALL_H, h as f32) * SCALE, 2.6);
        }
        // Pillars at room corners; torches on two of them.
        let mut flames = vec![];
        for (ri, r) in d.rooms.iter().enumerate() {
            let (c, hf) = (r.center(), r.half());
            let inset = 190.0;
            for (k, (sx, sz)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].into_iter().enumerate() {
                let p = [c[0] + sx * (hf[0] - inset), c[1] + sz * (hf[1] - inset)];
                add_box_collision(&mut world, [org[0] + p[0] as f64, org[1] + 150.0, org[2] + p[1] as f64], [70.0, 300.0, 70.0], 0.0, &mut faces, &o);
                walls.push_box(at(dg, p, 150.0), Vec3::new(70.0, 300.0, 70.0) * SCALE, 2.6);
                if (k + ri) % 2 == 0 {
                    flames.push(p);
                }
            }
        }
        world.finalize();
        // Display: floors by kind, merged along rows.
        let mut rooms_f = MeshBuf::default();
        let mut corr_f = MeshBuf::default();
        let mut boss_f = MeshBuf::default();
        for y in 0..d.h as i32 {
            let mut x = 0;
            while x < d.w as i32 {
                let kind = d.tile(x, y);
                if kind == Tile::Void {
                    x += 1;
                    continue;
                }
                let x0 = x;
                while x < d.w as i32 && d.tile(x, y) == kind {
                    x += 1;
                }
                let (bx0, bx1) = (to_bevy([org[0] + x0 as f64 * tile, 0.0, 0.0]).x, to_bevy([org[0] + x as f64 * tile, 0.0, 0.0]).x);
                let (bz0, bz1) = (to_bevy([0.0, 0.0, org[2] + y as f64 * tile]).z, to_bevy([0.0, 0.0, org[2] + (y + 1) as f64 * tile]).z);
                let buf = match kind {
                    Tile::Room => &mut rooms_f,
                    Tile::Corridor => &mut corr_f,
                    _ => &mut boss_f,
                };
                buf.push_floor(bx0, bz0, bx1, bz1, (org[1] as f32 + 0.6) * SCALE, 3.6);
            }
        }
        let wall_mat = textured(host, "brick", Color::srgb(0.42, 0.34, 0.52));
        let floor_mat = textured(host, "flagstone", Color::srgb(0.55, 0.52, 0.72));
        let corr_mat = textured(host, "stone", Color::srgb(0.5, 0.48, 0.62));
        let boss_mat = textured(host, "checker", Color::srgb(0.55, 0.32, 0.5));
        for (buf, mat) in [(walls, wall_mat), (rooms_f, floor_mat), (corr_f, corr_mat), (boss_f, boss_mat)] {
            if buf.is_empty() {
                continue;
            }
            let mesh = host.meshes.add(buf.into_mesh());
            let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default()));
            self.floor_ents.push(e);
        }
        // Torches.
        if host.asset_root.exists(FLAME) {
            for p in flames {
                let e = host.spawn((scene(host.assets, FLAME), Transform::from_translation(at(dg, p, 300.0)).with_scale(Vec3::splat(2.2)), Visibility::Inherited));
                self.floor_ents.push(e);
            }
        }
        world
    }

    fn mood(&self, host: &mut Host) {
        let boss = self.core.dungeon.boss_floor;
        host.set_clear(if boss { Color::srgb(0.12, 0.03, 0.06) } else { Color::srgb(0.05, 0.05, 0.12) });
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(if boss { 0x1a0610 } else { 0x0a0c20 }, if boss { 0x40101e } else { 0x1a2050 }, if boss { 0x70283a } else { 0x30386a }).with_clouds(0.1));
        host.cam.fog = Some((if boss { Color::srgb(0.16, 0.04, 0.08) } else { Color::srgb(0.06, 0.07, 0.16) }, 2600.0, 7800.0));
    }

    fn apply_test_hooks(&mut self, host: &mut Host) {
        let floor = self.core.floor;
        if let Some(lv) = std::env::var("JUMPMAN_RELIC_LEVEL").ok().and_then(|v| v.parse::<u32>().ok()) {
            while self.core.player.level < lv.min(MAX_LEVEL) {
                self.core.add_xp(xp_to_next(self.core.player.level));
            }
            self.core.player.hp = self.core.stats.max_hp;
        }
        if let Ok(list) = std::env::var("JUMPMAN_RELIC_GIVE") {
            let mut rng = Rng::new(self.seed ^ 0x61F7);
            for part in list.split(',') {
                let Some((r, s)) = part.split_once(':') else { continue };
                let rarity = RARITIES.iter().copied().find(|x| x.name().eq_ignore_ascii_case(r));
                let slot = [Slot::Weapon, Slot::Armor, Slot::Boots, Slot::Charm].into_iter().find(|x| x.name().eq_ignore_ascii_case(s));
                if let (Some(rarity), Some(slot)) = (rarity, slot) {
                    let id = self.core.new_item_id();
                    let item = roll_item(&mut rng, id, slot, rarity, item_level(floor, self.core.player.level));
                    self.core.give_item(item);
                }
            }
        }
        if let Some(n) = std::env::var("JUMPMAN_RELIC_BAG").ok().and_then(|v| v.parse::<usize>().ok()) {
            let mut rng = Rng::new(self.seed ^ 0xBA6);
            for _ in 0..n.min(BAG_CAP) {
                let id = self.core.new_item_id();
                let item = roll_drop(&mut rng, id, DropSource::Chest, floor, self.core.player.level);
                self.core.give_item(item);
            }
        }
        if let Some(n) = std::env::var("JUMPMAN_RELIC_DROPS").ok().and_then(|v| v.parse::<usize>().ok()) {
            // Loot on the floor around the start, one of every rarity first.
            self.core.filter_commons = false;
            let mut rng = Rng::new(self.seed ^ 0xD0D);
            let c = self.core.player.pos;
            for k in 0..n {
                let a = k as f32 * 6.283 / n.max(1) as f32;
                let id = self.core.new_item_id();
                let rarity = RARITIES[k % 5];
                let slot = [Slot::Weapon, Slot::Armor, Slot::Boots, Slot::Charm][k % 4];
                let item = roll_item(&mut rng, id, slot, rarity, item_level(floor, 4));
                self.core.place_item([c[0] + a.sin() * 520.0, c[1] + a.cos() * 520.0], item);
                self.core.drops.iter_mut().for_each(|d| d.age = 20);
            }
            self.core.stats.pickup = 0.0;
        }
        if let Some(n) = std::env::var("JUMPMAN_RELIC_HORDE").ok().and_then(|v| v.parse::<usize>().ok()) {
            // A crowd around the start room's centre (performance checks); the cap still applies.
            let c = self.core.player.pos;
            let kinds = [EnemyKind::Scamp, EnemyKind::Lancer, EnemyKind::Bruiser, EnemyKind::Wraith, EnemyKind::Sapper];
            for k in 0..n {
                let a = k as f32 * 2.399;
                let r = 350.0 + (k % 7) as f32 * 120.0;
                let p = [c[0] + a.sin() * r, c[1] + a.cos() * r];
                if self.core.dungeon.circle_free(p, 90.0) {
                    self.core.spawn_enemy(kinds[k % kinds.len()], p, 0, if k % 9 == 0 { MOD_STOUT } else { 0 });
                }
            }
        }
        if env_flag("JUMPMAN_RELIC_SHOWCASE") {
            // One of each foe in a row ahead of the player, standing still, for scale checks.
            let c = self.core.player.pos;
            for (i, k) in [EnemyKind::Scamp, EnemyKind::Bruiser, EnemyKind::Lancer, EnemyKind::Wraith, EnemyKind::Sapper, EnemyKind::Seer, EnemyKind::King].into_iter().enumerate() {
                let (x, z) = if i < 4 { (-600.0 + i as f32 * 400.0, 480.0) } else { (-400.0 + (i - 4) as f32 * 400.0, 980.0) };
                self.core.spawn_enemy(k, [c[0] + x, c[1] + z], 0, if i == 2 { MOD_SWIFT } else { 0 });
            }
            self.core.hold_foes = true;
        }
        if let Some(p) = std::env::var("JUMPMAN_RELIC_POINTS").ok().and_then(|v| v.parse::<u32>().ok()) {
            self.core.tree.points = p;
        }
        if let Ok(w) = std::env::var("JUMPMAN_RELIC_TREE") {
            // `0:2,2:1` = node index : ranks, applied ignoring points.
            for (n, k) in w.split(',').filter_map(|p| p.split_once(':')).filter_map(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<u8>().ok()?))) {
                if let Some(node) = NODES.get(n) {
                    for _ in 0..k {
                        self.core.tree.points += 1;
                        self.core.rank_up(*node);
                    }
                }
            }
        }
        if let Ok(target) = std::env::var("JUMPMAN_RELIC_TELEPORT") {
            let d = &self.core.dungeon;
            let idx = match target.as_str() {
                "boss" | "stairs" => Some(d.exit),
                "treasure" => d.rooms.iter().position(|r| r.kind == RoomKind::Treasure),
                "combat" => d.rooms.iter().position(|r| r.kind == RoomKind::Combat),
                n => n.parse::<usize>().ok().filter(|i| *i < d.rooms.len()),
            };
            if let Some(i) = idx {
                let c = d.rooms[i].center();
                // A real room's centre is its most open spot; a box room is entered near its edge.
                let off = if target == "stairs" || d.real.is_some() { [0.0, 0.0] } else { [0.0, -d.rooms[i].half()[1] + 330.0] };
                let p = [c[0] + off[0], c[1] + off[1]];
                let w = world_pos(d, p);
                self.core.player.pos = p;
                self.core.player.prev = p;
                host.place_mario(w, Some(0.0));
            }
        }
        if let Ok(list) = std::env::var("JUMPMAN_RELIC_FOES") {
            // The ported OoT / BK cast standing in rows ahead of the player (`all`, or a floor 1-3 for that theme).
            for r in self.core.rooms.iter_mut() {
                r.active = true;
                r.cleared = true;
            }
            let c = self.core.player.pos;
            let kinds: Vec<EnemyKind> = match list.parse::<u32>() {
                Ok(f) => relic_theme_kinds(f),
                Err(_) => relic_foe_kinds().to_vec(),
            };
            for (i, k) in kinds.into_iter().enumerate() {
                let (row, col) = (i / 6, i % 6);
                let p = [c[0] - 950.0 + col as f32 * 380.0, c[1] + 500.0 + row as f32 * 420.0];
                if let Some(n) = self.core.spawn_enemy(k, p, 0, if k == EnemyKind::Bigbutt { MOD_STOUT } else { 0 }) {
                    if self.core.enemies[n].state == EState::Hidden {
                        self.core.enemies[n].state = EState::Chase;
                    }
                }
            }
            self.core.hold_foes = true;
        }
        if let Ok(screen) = std::env::var("JUMPMAN_RELIC_SCREEN") {
            host.relic.screen = match screen.as_str() {
                "bag" => Screen::Bag,
                "tree" => Screen::Tree,
                _ => Screen::None,
            };
        }
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        let hero = hero_of(host.relic.character);
        let start_floor = std::env::var("JUMPMAN_RELIC_FLOOR").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        let plan = self.real_plan(host, seed);
        self.core = Relic::with_plan(seed, hero, start_floor, self.meta.stash, plan);
        self.core.filter_commons = self.meta.filter_commons;
        self.enter_world(host);
        self.mood(host);
        self.last_action = 0;
        self.leaping = false;
        self.spin_t = 0;
        self.summary_sent = false;
        self.pending = 0;
        self.shown_gen = 0;
        self.yaws.iter_mut().for_each(|y| *y = 0.0);
        host.relic.reset();
        host.relic.active = true;
        host.relic.hud.hint = true;
        host.set_player_visible(true);
        self.apply_test_hooks(host);
        match std::env::var("JUMPMAN_RELIC_END").as_deref() {
            Ok("win") => {
                self.core.won = true;
                self.core.over = true;
                self.core.kills = 42;
                self.core.run_gold = 380;
                self.core.floor = FLOORS;
                self.core.items_found = 9;
                self.core.best_rarity = 4;
                self.core.player.level = 9;
                self.finish(host);
            }
            Ok("lose") => {
                self.core.over = true;
                self.core.kills = 17;
                self.core.run_gold = 160;
                self.core.floor = 2;
                self.core.items_found = 4;
                self.core.best_rarity = 2;
                self.core.player.level = 5;
                self.finish(host);
            }
            _ => {}
        }
        self.queued.clear();
        self.queue_at = self.t;
        self.queue(if self.first_run { "DESCEND, FIND THE RELIC" } else { "ONCE MORE INTO THE DEEP" }, 1.8);
        if let Some(why) = self.no_real.take() {
            self.queue(&why, 3.0);
        }
        self.announce();
        self.first_run = false;
    }

    fn queue(&mut self, text: &str, secs: f32) {
        self.queued.push_back((text.to_string(), secs));
    }

    /// An event message: at once when the screen is free, else after the queued ones (at most four wait).
    fn say(&mut self, host: &mut Host, text: &str, secs: f32) {
        if toast_slot_free(self.queued.len(), self.t, self.queue_at) {
            host.toast(text, secs);
        } else if self.queued.len() < 4 {
            self.queue(text, secs);
        }
    }

    /// The next queued message once the one before has had its time.
    fn pump_toasts(&mut self, host: &mut Host) {
        if self.t >= self.queue_at {
            if let Some((text, secs)) = self.queued.pop_front() {
                host.toast(&text, secs);
                self.queue_at = self.t + secs as f64;
            }
        }
    }

    /// The floors of a new run: real dungeons when the Ocarina of Time scenes are imported (`relic_real.rs`), else the
    /// generated ones, with the reason kept for a toast. `JUMPMAN_RELIC_PROCEDURAL=1` asks for generated floors,
    /// `JUMPMAN_RELIC_SITES=deku_hollow,forest_court,phantom_lair` picks the sites floor by floor.
    fn real_plan(&mut self, host: &Host, seed: u64) -> Option<Arc<Vec<Dungeon>>> {
        if env_flag("JUMPMAN_RELIC_PROCEDURAL") {
            return None;
        }
        let force = std::env::var("JUMPMAN_RELIC_SITES").unwrap_or_default();
        let force: Vec<&str> = force.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
        match super::relic_real::plan_run(&host.asset_root.0, seed, &force) {
            Ok(plan) => {
                info!("relic: real floors {:?}", plan.iter().filter_map(|d| d.real.as_ref().map(|r| r.key.clone())).collect::<Vec<_>>());
                Some(Arc::new(plan))
            }
            Err(e) => {
                warn!("relic: generated floors ({e})");
                self.no_real = Some("NO OCARINA OF TIME DUNGEONS IMPORTED - PROCEDURAL FLOORS".into());
                None
            }
        }
    }

    /// Build the floor and put the player at its entrance, facing the way in.
    fn enter_world(&mut self, host: &mut Host) {
        let world = self.build_floor(host);
        let dg = &self.core.dungeon;
        let start = self.core.start_pos();
        let ahead = dg.rooms.iter().enumerate().filter(|(i, _)| *i != dg.start).map(|(_, r)| r.center()).min_by(|a, b| dist(*a, start).total_cmp(&dist(*b, start)));
        let yaw = match (ahead, dg.real.is_some()) {
            (Some(a), true) => radians_to_angle(((a[0] - start[0]) as f64).atan2((a[1] - start[1]) as f64)) as f64,
            _ => 0.0,
        };
        let w = world_pos(dg, start);
        let spawn = SpawnPoint { action: None, pos: [w[0], w[1] + 100.0, w[2]], face_yaw: yaw };
        host.set_mario_world(Arc::new(world), spawn, -1e9);
        host.place_mario(w, Some(yaw));
        self.safe = w;
    }

    /// The floor's name, and on a real floor what to look for.
    fn announce(&mut self) {
        let Some(src) = self.core.dungeon.real.clone() else { return };
        let goal = if self.core.dungeon.boss_floor { "THE BOMB KING HOLDS THE RELIC" } else { "FIND THE GOLDEN LIGHT TO GO DEEPER" };
        self.queue(&format!("FLOOR {}: {}", self.core.floor, src.title), 2.4);
        self.queue(goal, 2.2);
    }

    /// A new floor after the stairs: fresh collision, meshes and mood.
    fn enter_floor(&mut self, host: &mut Host) {
        self.enter_world(host);
        self.mood(host);
        self.leaping = false;
        host.relic.names.clear();
        host.relic.floor_gen = host.relic.floor_gen.wrapping_add(1);
        self.queued.clear();
        self.queue_at = self.t;
        if self.core.dungeon.real.is_some() {
            self.announce();
        } else {
            self.queue(&format!("FLOOR {}", self.core.floor), 1.6);
        }
        if self.core.dungeon.boss_floor {
            self.queue("THE BOMB KING WAITS BELOW...", 2.4);
        }
        self.pump_toasts(host);
    }

    /// `JUMPMAN_RELIC_BOT=1`: a scripted player on the real physics (headless soak runs and screenshots). It walks the
    /// floor's tile path with the stick, fights whatever is awake (swinging, diving through telegraphed attacks, a
    /// potion when low), spends points and equips upgrades, sweeps every room, then walks into the exit room, clears it
    /// and stands in the golden light until the floor changes.
    fn bot_step(&mut self, host: &mut Host, mut c: ControllerSnapshot) -> StepOverride {
        let core = &mut self.core;
        while core.tree.points > 0 {
            let Some(n) = NODES.iter().copied().find(|n| core.tree.can_rank(*n)) else { break };
            core.rank_up(n);
        }
        if let Some(id) = core.bag.iter().find(|it| it.power() > core.equipment.slots[core.equipment.target(it.slot)].map_or(0.0, |e| e.power()) + 0.5).map(|it| it.id) {
            core.equip_from_bag(id);
        }
        let me = core.player.pos;
        let dg = &core.dungeon;
        // The nearest foe of a woken room, wherever it hangs back (the way down waits for the exit room's guards).
        let foe = (0..ENEMY_SLOTS).filter(|i| core.enemies[*i].alive && core.rooms.get(core.enemies[*i].room as usize).is_some_and(|x| x.active)).min_by(|a, b| dist(core.enemies[*a].pos, me).total_cmp(&dist(core.enemies[*b].pos, me)));
        let mut goal = None;
        let mut stop = 60.0;
        if let Some(i) = foe {
            let e = core.enemies[i];
            goal = Some(e.pos);
            stop = e.radius + PLAYER_RADIUS + 40.0;
            self.intent.attack = true;
            if core.enemies.iter().any(|x| x.alive && x.winding_up() && dist(x.pos, me) < 380.0) && core.player.dodge_cd == 0 {
                self.intent.dodge = true;
            }
            for s in SKILLS {
                if dist(e.pos, me) < 320.0 && core.skill_ready(s) && (s != Skill::Aegis || core.player.hp < 0.6 * core.stats.max_hp) {
                    self.intent.skill[s as usize] = true;
                }
            }
        } else if let Some(d) = core.drops.iter().filter(|d| d.alive && !matches!(d.kind, DropKind::Potion) && dist(d.pos, me) < 700.0 && !dg.path(me, d.pos).is_empty()).min_by(|a, b| dist(a.pos, me).total_cmp(&dist(b.pos, me))) {
            goal = Some(d.pos);
            stop = 20.0;
        } else {
            // The nearest room not yet walked (the exit last), then the exit itself.
            let next = (0..dg.rooms.len()).filter(|r| *r != dg.exit && !core.rooms[*r].visited).min_by(|a, b| dist(dg.rooms[*a].center(), me).total_cmp(&dist(dg.rooms[*b].center(), me)));
            goal = Some(dg.rooms[next.unwrap_or(dg.exit)].center());
            stop = 30.0;
        }
        self.intent.potion = core.player.hp < 0.4 * core.stats.max_hp && core.player.potions > 0;
        let mut camera_yaw = None;
        if let Some(g) = goal {
            let path = dg.path(me, g);
            let wp = path.iter().find(|w| dist(**w, me) > 45.0).copied().unwrap_or(g);
            let face_to = if foe.is_some() && dist(g, me) < 500.0 { g } else { wp };
            let heading = radians_to_angle((face_to[0] - me[0]).atan2(face_to[1] - me[1]) as f64) as f64;
            let walk = dist(g, me) > stop;
            // Pressed against a lip or a door frame: hop, and veer a little to either side.
            if walk && dist(me, self.bot_last) < 2.0 {
                self.bot_stuck += 1;
            } else {
                self.bot_stuck = 0;
            }
            self.bot_last = me;
            let veer = match (self.bot_stuck / 20) % 4 {
                0 => 0.0,
                1 => 0.6,
                2 => 0.0,
                _ => -0.6,
            };
            if self.bot_stuck % 20 == 10 {
                c.button_pressed |= A_BUTTON;
                c.button_down |= A_BUTTON;
            }
            let toward = radians_to_angle((wp[0] - me[0]).atan2(wp[1] - me[1]) as f64 + veer) as f64;
            camera_yaw = Some(to_s16(toward + 0x8000 as f64) as i32);
            if walk {
                c.stick_x = 0.0;
                c.stick_y = 64.0;
                c.stick_mag = 64.0;
            } else {
                c.stick_x = 0.0;
                c.stick_y = 0.0;
                c.stick_mag = 0.0;
                host.sim.mario.face_angle[1] = heading;
            }
        }
        let mario = &host.sim.mario;
        let grounded = mario.action & ACT_FLAG_AIR == 0;
        if self.intent.attack && self.core.player.atk_cd == 0 && grounded && mario.forward_vel < 24.0 {
            c.button_pressed |= B_BUTTON;
            c.button_down |= B_BUTTON;
        }
        if std::env::var_os("RELIC_BOT_DEBUG").is_some() && self.core.tick % 60 == 0 {
            eprintln!("relic bot: tick {} me {:?} goal {:?} stick {} cam {:?} action {:x} vel {:.1}", self.core.tick, self.core.player.pos, goal, c.stick_mag, camera_yaw, mario.action, mario.forward_vel);
        }
        StepOverride { controller: Some(c), camera_yaw }
    }

    /// One contextual lesson at a time, at least four seconds apart, on the first runs only.
    fn teach(&mut self, host: &mut Host) {
        if self.meta.victories > 0 || self.meta.best_floor >= 2 || self.t - self.lesson_at < 4.0 || self.core.over {
            return;
        }
        // Never over a floor's arrival lines: the lesson waits until they have been read.
        if !toast_slot_free(self.queued.len(), self.t, self.queue_at) {
            return;
        }
        let ev = self.core.ev;
        let cue = LessonCue {
            foes_awake: self.core.alive_enemies > 0,
            hurt: ev.hurt,
            got_item: ev.pickup_item.is_some(),
            level_up: ev.level_up || self.core.tree.points > 0,
            low_hp: self.core.player.hp < self.core.stats.max_hp * 0.45,
            potions: self.core.player.potions,
        };
        let Some(bit) = next_lesson(self.taught, cue) else { return };
        self.taught |= bit;
        self.lesson_at = self.t;
        let text = match bit {
            LESSON_FIGHT => host.prompt("HOLD ENTER TO ATTACK - B DIVES THROUGH HITS", "ATK TO ATTACK - DODGE DIVES THROUGH HITS", "RB ATTACKS - RUN + B DIVES THROUGH HITS"),
            LESSON_DODGE => "DIVE TO DODGE: YOU CAN'T BE HIT MID-DIVE",
            LESSON_PACK => host.prompt("NEW GEAR! I OPENS YOUR PACK", "NEW GEAR! TAP PACK TO COMPARE", "NEW GEAR! SELECT OPENS YOUR PACK"),
            LESSON_SKILLS => host.prompt("SKILL POINT! T OPENS THE SKILL TREE", "SKILL POINT! TAP TREE TO SPEND IT", "SKILL POINT! CLICK L-STICK FOR THE TREE"),
            _ => host.prompt("LOW HEALTH: H DRINKS A POTION", "LOW HEALTH: TAP POTION", "LOW HEALTH: D-PAD FOR A POTION"),
        };
        host.toast(text, 2.6);
    }

    fn finish(&mut self, host: &mut Host) {
        let s = self.core.summary();
        let banked = banked_gold(s.gold, s.victory);
        let cheated = ["JUMPMAN_RELIC_GOD", "JUMPMAN_RELIC_LEVEL", "JUMPMAN_RELIC_GIVE", "JUMPMAN_RELIC_END", "JUMPMAN_RELIC_FLOOR", "JUMPMAN_RELIC_TELEPORT"].iter().any(|k| env_flag(k));
        let new_best = !cheated && s.floor > self.meta.best_floor;
        if !cheated {
            self.meta.stash += banked;
            if new_best {
                self.meta.best_floor = s.floor;
            }
            if s.victory {
                self.meta.victories += 1;
            }
            save_meta(&self.meta);
        }
        self.over_at = self.t;
        host.relic.summary = Some(SummaryView { victory: s.victory, floor: s.floor, level: s.level, kills: s.kills, gold: s.gold, items: s.items, best_rarity: s.best_rarity, secs: s.secs, banked, stash: self.meta.stash, best_floor: self.meta.best_floor.max(s.floor), new_best_floor: new_best, legacy_pct: (legacy_bonus(self.meta.stash) * 100.0).round() as u32 });
        host.relic.summary_ready = false;
        host.relic.screen = Screen::None;
        self.summary_sent = true;
        host.set_player_visible(true);
    }

    /// A chase camera from above and behind that ignores the (low) walls of a generated floor. Under a real dungeon's
    /// ceilings it drops lower and closer until it sees the hero (rays against the scene's collision), easing back out.
    fn camera(&mut self, host: &mut Host, dt: f64) {
        let focus = DVec3::from(host.render_pos()) + DVec3::new(0.0, 80.0, 0.0);
        let yaw = host.rig.yaw;
        let (mut pitch, mut dist) = (0.95f64, 1500.0f64);
        if self.core.dungeon.real.is_some() {
            let world = host.sim.mario.world.clone();
            let mut best = (0.95f64, 0.0f64);
            // Higher first (looking down past the side walls of a corridor), then lower under a low ceiling.
            let pitches: &[f64] = if env_flag("JUMPMAN_RELIC_CAM_OLD") { &[0.95, 0.8, 0.66, 0.52, 0.4] } else { &[0.95, 1.12, 1.28, 0.8, 0.66, 0.52, 0.4, 1.45] };
            for &p in pitches {
                let dir = DVec3::new(yaw.sin() * p.cos(), p.sin(), yaw.cos() * p.cos());
                let t = crate::camera::rig::camera_ray(&world, focus, dir, 1500.0).map_or(1500.0, |t| (t - 60.0).max(if env_flag("JUMPMAN_RELIC_CAM_OLD") { 220.0 } else { 90.0 }));
                if t > best.1 + 1.0 {
                    best = (p, t);
                }
                if t >= 1150.0 {
                    break;
                }
            }
            // Pull in at once (never look through a wall), ease back out.
            let k = if best.1 < self.cam.1 { 1.0 } else { 1.0 - (-2.5 * dt).exp() };
            let kp = 1.0 - (-5.0 * dt).exp();
            self.cam = (self.cam.0 + (best.0 - self.cam.0) * kp, self.cam.1 + (best.1 - self.cam.1) * k);
            (pitch, dist) = self.cam;
        }
        let eye = focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
        let mut pose = CamPose { eye, look: focus, up: DVec3::Y, fov: 55.0, near: 20.0, far: 30000.0 };
        self.shake.apply(&mut pose, 22.0);
        host.cam.pose = Some(pose);
        if let Some(l) = self.light {
            let p = focus + DVec3::new(0.0, 260.0, 0.0);
            host.commands.entity(l).insert(Transform::from_translation(to_bevy(p.into())));
        }
    }

    /// Sparks, puffs and shake for what just happened this tick. Bounded: a handful of bursts per tick at most.
    fn feel(&mut self, host: &mut Host, mario_pos: [f64; 3]) {
        use crate::fx::particles::BurstOptions;
        let ev = self.core.ev;
        let dg = &self.core.dungeon;
        let lift = |p: [f32; 2], h: f64| {
            let w = world_pos(dg, p);
            [w[0], w[1] + h, w[2]]
        };
        let (mut puffs, mut sparks) = (0, 0);
        for (i, e) in self.core.enemies.iter().enumerate() {
            let (was_alive, was_flash) = self.seen[i];
            self.seen[i] = (e.alive, e.flash);
            let h = e.radius as f64 * 1.1 + 20.0;
            if was_alive && !e.alive && puffs < 6 {
                // Defeated: a puff of smoke and a spray in the foe's own flavour.
                puffs += 1;
                let big = e.is_elite() || e.kind == EnemyKind::King;
                let p = lift(e.pos, h);
                host.particles.burst(p, BurstOptions { color: 0xe8e2d4, count: if big { 14 } else { 8 }, speed: 150.0 + e.radius, life: 0.5, gravity: -40.0, size: 70.0 + e.radius * 0.6, upward: 60.0, ..Default::default() });
                host.particles.burst(p, BurstOptions { color: if big { 0xffd24a } else { 0xb48cff }, count: if big { 12 } else { 6 }, speed: 260.0, life: 0.45, gravity: 420.0, size: 35.2, upward: 170.0, ..Default::default() });
                self.shake.add(if big { 0.4 } else { 0.14 });
            } else if e.alive && e.flash > was_flash && sparks < 5 {
                // Struck this tick.
                sparks += 1;
                host.particles.burst(lift(e.pos, h), BurstOptions { color: if ev.crit { 0xffd24a } else { 0xfff6dc }, count: if ev.crit { 7 } else { 4 }, speed: 230.0, life: 0.22, gravity: 300.0, size: if ev.crit { 40.0 } else { 28.0 }, upward: 90.0, ..Default::default() });
            }
        }
        let at_hero = [mario_pos[0], mario_pos[1] + 70.0, mario_pos[2]];
        if ev.hit > 0 {
            self.shake.add(if ev.crit { 0.22 } else { 0.07 });
        }
        if ev.hurt {
            self.shake.add(0.4);
            self.hurt_pop.hit();
            host.particles.burst(at_hero, BurstOptions { color: 0xff5a4a, count: 9, speed: 240.0, life: 0.35, gravity: 380.0, size: 37.4, upward: 130.0, ..Default::default() });
        }
        if ev.dodged {
            host.particles.burst(at_hero, BurstOptions { color: 0x9fe8ff, count: 5, speed: 150.0, life: 0.3, gravity: 0.0, size: 33.0, upward: 40.0, ..Default::default() });
        }
        if ev.slam || ev.blast {
            self.shake.add(0.45);
            host.particles.burst(mario_pos, BurstOptions { color: 0xd8d2c4, count: 12, speed: 260.0, life: 0.5, gravity: 60.0, size: 52.0, upward: 50.0, ..Default::default() });
        }
        if ev.whirl || ev.aegis || ev.volley {
            host.particles.burst(at_hero, BurstOptions { color: 0x8fb4ff, count: 10, speed: 280.0, life: 0.35, gravity: 0.0, size: 35.2, upward: 30.0, ..Default::default() });
        }
        if ev.pickup_gold > 0 {
            self.gain_pop.hit();
            host.particles.burst(at_hero, BurstOptions { color: 0xffd84a, count: (3 + ev.pickup_gold / 12).min(9) as usize, speed: 170.0, life: 0.4, gravity: 520.0, size: 28.6, upward: 240.0, ..Default::default() });
        }
        if let Some(r) = ev.pickup_item {
            self.gain_pop.hit();
            let n = 8 + 3 * r as usize;
            host.particles.burst(at_hero, BurstOptions { color: rarity_hex(r), count: n, speed: 220.0 + 30.0 * r as u8 as f32, life: 0.6, gravity: 160.0, size: 37.4, upward: 220.0, ..Default::default() });
            host.particles.burst(at_hero, BurstOptions { color: 0xffffff, count: 5, speed: 120.0, life: 0.35, gravity: 0.0, size: 26.4, upward: 160.0, ..Default::default() });
        }
        if ev.pickup_potion {
            host.particles.burst(at_hero, BurstOptions { color: 0xff6f8f, count: 7, speed: 160.0, life: 0.4, gravity: 300.0, size: 30.8, upward: 200.0, ..Default::default() });
        }
        if ev.potion_used {
            host.particles.burst(at_hero, BurstOptions { color: 0x6dff8a, count: 12, speed: 120.0, life: 0.7, gravity: -160.0, size: 33.0, upward: 120.0, ..Default::default() });
        }
        if ev.level_up {
            host.particles.burst(at_hero, BurstOptions { color: 0xffe066, count: 18, speed: 300.0, life: 0.8, gravity: 120.0, size: 39.6, upward: 320.0, ..Default::default() });
            host.particles.burst(at_hero, BurstOptions { color: 0xd6a8ff, count: 10, speed: 200.0, life: 0.6, gravity: -80.0, size: 30.8, upward: 200.0, ..Default::default() });
        }
        if ev.chest || ev.token {
            host.particles.burst(at_hero, BurstOptions { color: 0xffd84a, count: 16, speed: 280.0, life: 0.7, gravity: 380.0, size: 33.0, upward: 360.0, ..Default::default() });
        }
        if ev.reflected {
            host.particles.sparks(at_hero, 6);
        }
        if ev.boss_spawned || ev.boss_phase.is_some() {
            self.shake.add(0.7);
        }
        if ev.boss_killed {
            self.shake.add(1.0);
            let p = lift(ev.last_kill, 160.0);
            host.particles.burst(p, BurstOptions { color: 0xff9a3c, count: 24, speed: 420.0, life: 0.9, gravity: 260.0, size: 57.2, upward: 300.0, ..Default::default() });
            host.particles.burst(p, BurstOptions { color: 0xfff1b8, count: 16, speed: 260.0, life: 0.7, gravity: 0.0, size: 44.0, upward: 120.0, ..Default::default() });
        }
        if ev.lastlight {
            self.shake.add(0.5);
            host.particles.burst(at_hero, BurstOptions { color: 0xfff6c8, count: 20, speed: 340.0, life: 0.7, gravity: 0.0, size: 44.0, upward: 80.0, ..Default::default() });
        }
        if ev.descended {
            self.seen.iter_mut().for_each(|s| *s = (false, 0));
        }
    }

    fn apply_cmds(&mut self, host: &mut Host) {
        let cmds: Vec<UiCmd> = std::mem::take(&mut host.relic.cmds);
        for c in cmds {
            match c {
                UiCmd::Equip(id) => {
                    self.core.equip_from_bag(id);
                    host.fx("star_sound", host.sim.mario.pos);
                }
                UiCmd::Sell(id) => {
                    self.core.sell_from_bag(id);
                    host.fx("coin", host.sim.mario.pos);
                }
                UiCmd::Unequip(s) => {
                    self.core.unequip(s);
                }
                UiCmd::SellUpTo(r) => {
                    if self.core.sell_up_to(r) > 0 {
                        host.fx("coin", host.sim.mario.pos);
                    }
                }
                UiCmd::Rank(n) => {
                    if self.core.rank_up(n) {
                        host.fx("star_sound", host.sim.mario.pos);
                    }
                }
                UiCmd::ToggleFilter => {
                    self.core.filter_commons = !self.core.filter_commons;
                    self.meta.filter_commons = self.core.filter_commons;
                    save_meta(&self.meta);
                    host.toast(if self.core.filter_commons { "HIDING COMMON LOOT (SOLD FOR GOLD)" } else { "SHOWING ALL LOOT" }, 1.4);
                }
                UiCmd::Open(s) => host.relic.screen = s,
                UiCmd::Close => host.relic.screen = Screen::None,
                UiCmd::Retry => {
                    if self.core.over && host.relic.summary_ready {
                        self.start_run(host);
                    }
                }
            }
            host.relic.snap_gen = host.relic.snap_gen.wrapping_add(1);
        }
    }

    fn refresh_snap(&mut self, host: &mut Host) {
        let s = &mut host.relic.snap;
        s.gold = self.core.player.gold;
        s.bag.clear();
        s.bag.extend_from_slice(&self.core.bag);
        s.equip = self.core.equipment.slots;
        s.stats = self.core.stats;
        s.tree = self.core.tree;
        s.level = self.core.player.level;
        s.hero = self.core.hero.label();
        s.attack = self.core.hero.attack().name;
        s.potions = self.core.player.potions;
        s.filter = self.core.filter_commons;
        s.hp = self.core.player.hp;
    }

    fn publish(&mut self, host: &mut Host) {
        let a = host.alpha.clamp(0.0, 1.0) as f32;
        let t = self.t as f32;
        let core = &self.core;
        let dg = &core.dungeon;
        let me = core.player.pos;
        let lerp =|p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a];
        let b = &mut *host.relic;
        b.time = t;
        let mut nbars = 0;
        b.bars.iter_mut().for_each(|x| x.kind = 0);
        b.foes.clear();
        for (i, e) in core.enemies.iter().enumerate() {
            let item = &mut b.enemies[i];
            if !e.alive {
                item.kind = 0;
                continue;
            }
            let (scale, lift) = look(e.kind);
            let p = lerp(e.prev, e.pos);
            if let Some(sp) = e.kind.species() {
                // Ported OoT / BK foe: face the way it moves (or the hero), pose from its state.
                let moving = dist(e.prev, e.pos) > 1.5;
                if moving {
                    self.yaws[i] = (e.pos[0] - e.prev[0]).atan2(e.pos[1] - e.prev[1]);
                }
                if e.state == EState::Dash || e.state == EState::Windup || !moving {
                    self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
                }
                let hidden = e.state == EState::Hidden && matches!(sp, crate::experiments::foes::Species::MadScrub);
                let shake = if e.winding_up() { (t * 70.0).sin() * 8.0 } else { 0.0 };
                if !hidden {
                    b.foes.push(FoeItem {
                        id: i as u32,
                        species: sp,
                        pos: at(dg, [p[0] + shake, p[1]], foe_lift(e) + (t * 3.0 + i as f32).sin().abs() * if sp.flies() { 12.0 } else { 0.0 }),
                        yaw: self.yaws[i],
                        radius: e.radius,
                        pose: foe_pose(e, moving),
                        pulse: (1.0 + 0.12 * e.flash as f32 / 4.0) * if e.state == EState::Fuse { 1.0 + 0.2 * (t * 30.0).sin().abs() } else { 1.0 },
                        age: if e.kind == EnemyKind::Limbo { (60u16.saturating_sub(e.timer)) as f32 / 30.0 } else { 0.0 },
                    });
                }
                // The elite ring and health bar still come from the common path.
                item.kind = if hidden || (e.state == EState::Hidden) { 0 } else { e.kind as u8 };
                item.pos = at(dg, p, 0.0);
                item.yaw = 0.0;
                item.scale = e.radius / 27.0 / if e.mods != 0 { 1.2 } else { 1.0 };
                item.aux = e.mods as f32;
                if nbars < b.bars.len() && (e.hp < e.max_hp || e.is_elite()) && e.state != EState::Hidden {
                    let d2 = (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
                    if d2 < 2600.0 * 2600.0 {
                        b.bars[nbars] = BarView { pos: at(dg, p, e.radius * 2.0 + foe_lift(e)), frac: (e.hp / e.max_hp.max(1.0)).max(0.0), kind: if e.is_elite() { 2 } else { 1 } };
                        nbars += 1;
                    }
                }
                continue;
            }
            if e.state == EState::Dash || e.vel[0].abs() + e.vel[1].abs() > 3.0 {
                let v = if e.state == EState::Dash { e.dir } else { e.vel };
                self.yaws[i] = v[0].atan2(v[1]);
            } else {
                self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
            }
            let hop = if matches!(e.kind, EnemyKind::Scamp | EnemyKind::Sapper) { (t * 11.0 + i as f32).sin().abs() * 14.0 } else { 0.0 };
            let bob = if matches!(e.kind, EnemyKind::Wraith | EnemyKind::Seer) { (t * 2.5 + i as f32).sin() * 16.0 } else { 0.0 };
            let shake = if e.winding_up() { (t * 70.0).sin() * 12.0 } else { 0.0 };
            let pulse = if e.state == EState::Fuse { 1.0 + 0.2 * (t * 30.0).sin().abs() } else { 1.0 };
            item.kind = e.kind as u8;
            item.pos = at(dg, [p[0] + shake, p[1]], lift + hop + bob);
            item.yaw = self.yaws[i];
            item.scale = scale * pulse * (1.0 + 0.12 * e.flash as f32 / 4.0) * if e.mods != 0 { 1.2 } else { 1.0 } * (e.radius / enemy_stats(e.kind).radius / if e.mods != 0 { 1.2 } else { 1.0 });
            item.aux = e.mods as f32;
            // Health bar for hurt, elite and boss foes within view range.
            if nbars < b.bars.len() && (e.hp < e.max_hp || e.is_elite() || e.kind == EnemyKind::King) && e.kind != EnemyKind::King {
                let d2 = (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
                if d2 < 2600.0 * 2600.0 {
                    b.bars[nbars] = BarView { pos: at(dg, p, e.radius * 2.0 + lift), frac: (e.hp / e.max_hp.max(1.0)).max(0.0), kind: if e.is_elite() { 2 } else { 1 } };
                    nbars += 1;
                }
            }
        }
        for (i, s) in core.shots.iter().enumerate() {
            let item = &mut b.shots[i];
            if s.alive {
                let p = lerp(s.prev, s.pos);
                *item = RItem { kind: if s.wave { 3 } else if s.from_player { 1 } else { 2 }, pos: at(dg, p, 85.0), yaw: 0.0, scale: s.radius / 36.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, h) in core.hazards.iter().enumerate() {
            let item = &mut b.hazards[i];
            if h.alive {
                *item = RItem { kind: 1, pos: at(dg, h.pos, 6.0), yaw: 0.0, scale: h.radius * SCALE, aux: 1.0 - h.delay as f32 / h.total as f32 };
            } else {
                item.kind = 0;
            }
        }
        for (i, r) in core.rings.iter().enumerate() {
            let rv = &mut b.rings[i];
            if r.alive {
                let kind = r.kind as u8;
                let scale = if r.kind == RingKind::Slash { r.radius * SCALE } else { r.radius * r.grow() * SCALE };
                rv.item = RItem { kind, pos: at(dg, r.center, 14.0), yaw: r.yaw, scale, aux: r.fade() };
                rv.half = r.half;
            } else {
                rv.item.kind = 0;
            }
        }
        let mut nlabels = 0;
        b.labels.iter_mut().for_each(|l| l.on = false);
        for (i, d) in core.drops.iter().enumerate() {
            let item = &mut b.drops[i];
            let d2 = (d.pos[0] - me[0]).powi(2) + (d.pos[1] - me[1]).powi(2);
            if !d.alive || d2 > DRAW_RANGE * DRAW_RANGE {
                item.kind = 0;
                continue;
            }
            let bob = 40.0 + (t * 3.0 + i as f32).sin() * 8.0;
            let pop = (1.0 - d.age as f32 / 10.0).max(0.0) * 80.0;
            match d.kind {
                DropKind::Gold => *item = RItem { kind: 1, pos: at(dg, d.pos, 35.0 + pop), yaw: 0.0, scale: 1.0, aux: 0.0 },
                DropKind::Potion => *item = RItem { kind: 2, pos: at(dg, d.pos, bob + pop), yaw: 0.0, scale: 1.0, aux: 0.0 },
                DropKind::Item => *item = RItem { kind: 3, pos: at(dg, d.pos, bob + pop), yaw: 0.0, scale: 1.0 + 0.1 * d.item.rarity as u8 as f32, aux: d.item.rarity as u8 as f32 },
            }
            let worth_label = match d.kind {
                DropKind::Gold => d.amount >= 40,
                _ => true,
            };
            if worth_label && d2 < 1500.0 * 1500.0 && nlabels < b.labels.len() {
                let (kind, id, rarity) = match d.kind {
                    DropKind::Gold => (0, d.amount, 0),
                    DropKind::Potion => (1, 0, 0),
                    DropKind::Item => (2, d.item.id, d.item.rarity as u8),
                };
                if kind == 2 && !b.names.contains_key(&id) {
                    b.names.insert(id, d.item.name());
                }
                b.labels[nlabels] = LabelView { on: true, pos: at(dg, d.pos, bob + pop), id, kind, rarity };
                nlabels += 1;
            }
        }
        for (i, c) in core.chests.iter().enumerate() {
            let item = &mut b.chests[i];
            match c {
                Some(ch) => *item = RItem { kind: 1, pos: at(dg, ch.pos, 0.0), yaw: 0.0, scale: 1.6, aux: if ch.opened { 1.0 } else { 0.0 } },
                None => item.kind = 0,
            }
        }
        for (i, f) in core.floaters.iter().enumerate().take(24) {
            b.floaters[i] = FloaterView { on: f.alive, pos: at(dg, f.pos, f.height + 80.0), value: f.value, kind: f.kind, age: f.age as f32 / 34.0 };
        }
        if let Some(e) = core.boss_enemy() {
            b.hud.boss = Some(BossHud { frac: (e.hp / e.max_hp).max(0.0), phase: e.phase });
        } else {
            b.hud.boss = None;
        }
        let p = &core.player;
        let h = &mut b.hud;
        h.hp = p.hp;
        h.max_hp = core.stats.max_hp;
        h.shield = p.shield;
        h.shield_max = p.shield_max;
        h.spark = p.spark;
        h.max_spark = core.stats.max_spark;
        h.xp = p.xp;
        h.xp_need = xp_to_next(p.level);
        h.level = p.level;
        h.floor = core.floor;
        h.gold = p.gold;
        h.potions = p.potions;
        h.kills = core.kills;
        h.points = core.tree.points;
        h.secs = core.secs();
        h.filter = core.filter_commons;
        h.hint = core.secs() < 14.0;
        h.low_hp = p.hp < core.stats.max_hp * 0.25 && !core.over;
        h.hurt = self.hurt_pop.value();
        h.gain = self.gain_pop.value();
        for s in SKILLS {
            let i = s as usize;
            h.skill_unlocked[i] = core.tree.unlocked(s);
            h.skill_cd[i] = core.skill_cd_frac(s);
            h.skill_ready[i] = core.skill_ready(s);
        }
        h.dodge_cd = p.dodge_cd as f32 / (DODGE_COOLDOWN + core.stats.dodge_ticks / 2) as f32;
        h.stairs = (core.stairs_t as f32 / 24.0).min(1.0);
        // Minimap: the rooms' boxes in the generated grid's frame (a real floor is fitted into it).
        let (off, k) = mini_frame(dg);
        b.mini.clear();
        for (i, r) in dg.rooms.iter().enumerate() {
            let rs = core.rooms[i];
            b.mini.push(MiniRoom { x: (r.x as f32 - off[0]) * k, y: (r.y as f32 - off[1]) * k, w: r.w as f32 * k, h: r.h as f32 * k, kind: match r.kind { RoomKind::Combat => 0, RoomKind::Start => 1, RoomKind::Treasure => 2, RoomKind::Stairs => 3, RoomKind::Boss => 4 }, seen: rs.visited, cleared: rs.cleared });
        }
        b.mini_player = [(me[0] / dg.tile - off[0]) * k, (me[1] / dg.tile - off[1]) * k];
        if b.screen != Screen::None {
            self.refresh_snap(host);
        }
    }
}

impl Experiment for RelicDescentExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Relic
    }
    fn replaces_course(&self) -> bool {
        true
    }
    /// The world stands still while the pack or skill page is open.
    fn time_scale(&self) -> f64 {
        if self.core.freeze || self.hitstop > 0.0 { 0.0 } else { 1.0 }
    }

    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        host.rig.distance = 1300.0;
        host.rig.pitch = 0.72;
        self.meta = load_meta();
        self.light = Some(host.spawn((PointLight { intensity: 4_000_000.0, range: 28.0, color: Color::srgb(1.0, 0.86, 0.66), shadow_maps_enabled: false, ..default() }, Transform::default())));
        self.start_run(host);
    }

    fn exit(&mut self, host: &mut Host) {
        host.relic.active = false;
        host.relic.summary = None;
        host.relic.screen = Screen::None;
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.set_player_visible(true);
        host.cam.fog = None;
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if self.core.over {
            if matches!(key, KeyCode::Space | KeyCode::Enter | KeyCode::KeyR) && host.relic.summary_ready {
                host.relic.cmds.push(UiCmd::Retry);
                return true;
            }
            return false;
        }
        let bit: Option<u16> = match key {
            KeyCode::Digit1 => Some(0),
            KeyCode::Digit2 => Some(1),
            KeyCode::Digit3 => Some(2),
            KeyCode::Digit4 => Some(3),
            KeyCode::KeyB => Some(4),
            KeyCode::KeyH => Some(6),
            KeyCode::KeyI => Some(7),
            KeyCode::KeyT => Some(8),
            _ => None,
        };
        if let Some(bit) = bit {
            self.pending |= 1 << bit;
            return true;
        }
        if key == KeyCode::KeyY {
            host.relic.cmds.push(UiCmd::ToggleFilter);
            return true;
        }
        false
    }

    fn before_step(&mut self, host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
        if self.core.over {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        // Frozen on a hit: keep the buttons pressed meanwhile for the first step after it.
        if self.hitstop > 0.0 {
            return StepOverride::default();
        }
        let bits = self.pending | std::mem::take(&mut host.relic.edges);
        self.pending = 0;
        // Bag / tree / potion buttons are handled here as well (edges arrive from touch and pad).
        let mut open = None;
        if bits & (1 << 7) != 0 {
            open = Some(if host.relic.screen == Screen::Bag { Screen::None } else { Screen::Bag });
        }
        if bits & (1 << 8) != 0 {
            open = Some(if host.relic.screen == Screen::Tree { Screen::None } else { Screen::Tree });
        }
        if let Some(s) = open {
            host.relic.screen = s;
            host.relic.snap_gen = host.relic.snap_gen.wrapping_add(1);
            self.refresh_snap(host);
        }
        self.core.freeze = host.relic.screen != Screen::None;
        let mut intent = Intent { attack: host.relic.atk_held || bits & (1 << 5) != 0, dodge: bits & (1 << 4) != 0, potion: bits & (1 << 6) != 0, skill: [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0] };
        if self.core.freeze {
            intent = Intent::default();
        }
        self.intent = intent;
        let mut c = controller.clone();
        let mario = &host.sim.mario;
        let grounded = mario.action & ACT_FLAG_AIR == 0;
        // The dedicated attack button punches / slashes through the real action when standing or walking.
        if intent.attack && self.core.player.atk_cd == 0 && grounded && mario.forward_vel < 24.0 {
            c.button_pressed |= B_BUTTON;
            c.button_down |= B_BUTTON;
        }
        if self.bot && !self.core.freeze {
            return self.bot_step(host, c);
        }
        StepOverride { controller: Some(c), camera_yaw: None }
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.core.paused_for_ui() || self.core.over || self.hitstop > 0.0 {
            return;
        }
        if self.god {
            self.core.player.hp = self.core.stats.max_hp;
        }
        let stats = self.core.stats;
        let mario = &mut host.sim.mario;
        let air = mario.action & ACT_FLAG_AIR != 0;
        let action = mario.action;
        let started = |a: u32| action == a && self.last_action != a;
        let punch_started = started(ACT_PUNCHING) || started(ACT_MOVE_PUNCHING) || started(ACT_JUMP_KICK);
        let dive_started = started(ACT_DIVE);
        let pound_land = started(ACT_GROUND_POUND_LAND);
        self.last_action = action;
        // Leap-and-slam: after the launch the pound starts at the top of the arc.
        if self.leaping && (mario.vel[1] <= 6.0 || !air) {
            self.leaping = false;
            if air {
                mario.set_action(ACT_GROUND_POUND, 0.0);
            }
        }
        // Boots: extra distance on top of the physics, never into a wall.
        if stats.move_bonus > 0.0 {
            let (vx, vz) = if air {
                (mario.vel[0], mario.vel[2])
            } else {
                let yaw = angle_to_radians(mario.face_angle[1]);
                (yaw.sin() * mario.forward_vel.max(0.0), yaw.cos() * mario.forward_vel.max(0.0))
            };
            let k = stats.move_bonus as f64;
            let me = self.core.dungeon.plane(mario.pos);
            let slid = self.core.dungeon.slide(me, [(vx * k) as f32, (vz * k) as f32], PLAYER_RADIUS + 15.0);
            host.shift_mario([(slid[0] - me[0]) as f64, 0.0, (slid[1] - me[1]) as f64]);
        }
        if self.spin_t > 0 {
            self.spin_t -= 1;
            host.sim.mario.face_angle[1] = to_s16(host.sim.mario.face_angle[1] + 6400.0) as f64;
        }
        // A real floor: remember the last spot on the plan, and put Mario back there should he ever drop through it.
        let (on_plan, floor_y) = {
            let d = &self.core.dungeon;
            let p = d.plane(host.sim.mario.pos);
            (d.walkable(p), d.world(p, 0.0)[1])
        };
        if self.core.dungeon.real.is_some() {
            let pos = host.sim.mario.pos;
            if on_plan && !air && (pos[1] - floor_y).abs() < 120.0 {
                self.safe = pos;
            } else if pos[1] < floor_y - 1200.0 {
                host.place_mario(self.safe, None);
            }
        }
        let m = &host.sim.mario;
        let face = angle_to_radians(m.face_angle[1]) as f32;
        let frame = PlayerFrame { pos: self.core.dungeon.plane(m.pos), height: (m.pos[1] - floor_y).max(0.0) as f32, face, pound_land, dive_started, punch_started, grounded: !air };
        let mario_pos = m.pos;
        let intent = self.intent;
        self.core.tick(&frame, &intent);
        let ev = self.core.ev;
        let tick = self.core.tick;
        self.feel(host, mario_pos);
        if !env_flag("JUMPMAN_RELIC_NO_HITSTOP") {
            self.hitstop = self.hitstop.max(hitstop_for(&ev));
        }
        // Physics requests from the core.
        if ev.want_dive {
            let m = &mut host.sim.mario;
            let yaw = if m.controller.stick_mag > 8.0 { m.intended_yaw } else { m.face_angle[1] };
            m.face_angle[1] = yaw;
            m.vel[1] = 14.0;
            m.forward_vel = 44.0;
            m.set_action(ACT_DIVE, 0.0);
        }
        if ev.want_leap {
            let m = &mut host.sim.mario;
            m.set_action(ACT_JUMP, 0.0);
            m.vel[1] = 64.0;
            self.leaping = true;
        }
        if ev.want_pound {
            host.sim.mario.set_action(ACT_GROUND_POUND, 0.0);
        }
        if ev.whirl {
            self.spin_t = 10;
        }
        // Sound, particles, toasts.
        if ev.swing {
            host.fx("sparks", mario_pos);
        }
        if ev.kills > 0 && tick >= self.sfx_tick + 3 {
            self.sfx_tick = tick;
            host.fx("break_box", world_pos(&self.core.dungeon, ev.last_kill));
        }
        if ev.hit > 0 && tick >= self.sfx_tick + 2 {
            host.fx("bonk", mario_pos);
        }
        if ev.hurt {
            host.fx("bonk", mario_pos);
        }
        if ev.slam || ev.whirl || ev.aegis || ev.volley {
            host.fx("teleport", mario_pos);
        }
        if ev.blast {
            host.fx("bonk", mario_pos);
        }
        if ev.pickup_gold > 0 {
            host.fx("coin", mario_pos);
        }
        if let Some(r) = ev.pickup_item {
            host.fx("star_sound", mario_pos);
            if r >= Rarity::Epic {
                self.say(host, &format!("{} FOUND!", r.name()), 1.6);
            }
        }
        if ev.pickup_potion {
            host.fx("coin", mario_pos);
        }
        if ev.potion_used {
            host.fx("star_sound", mario_pos);
        }
        if ev.level_up {
            host.fx("star_sound", mario_pos);
            let lv = self.core.player.level;
            self.say(host, &format!("LEVEL {lv}  -  SKILL POINT!"), 1.8);
        }
        if ev.auto_equipped {
            self.say(host, "EQUIPPED A NEW ITEM", 1.4);
        }
        if ev.chest {
            host.fx("star_sound", mario_pos);
            self.say(host, "CHEST OPENED!", 1.6);
        }
        if ev.room_cleared {
            let exit = self.core.dungeon.exit;
            let opened = self.core.dungeon.real.is_some() && !self.core.dungeon.boss_floor && self.core.rooms[exit].cleared && self.core.rooms[exit].active;
            let text = if opened && self.core.room_at_player() == Some(exit) { "THE WAY DOWN IS OPEN" } else { "ROOM CLEARED" };
            self.say(host, text, 1.4);
        }
        if ev.stairs_locked {
            self.say(host, "BEAT THIS ROOM'S GUARDS TO OPEN THE WAY", 1.8);
        }
        if ev.token {
            host.fx("star_sound", mario_pos);
            self.say(host, "GOLD SKULLTULA TOKEN!", 1.6);
        }
        if ev.reflected {
            host.fx("bonk", mario_pos);
        }
        if ev.boss_spawned {
            host.toast("THE BOMB KING AWAKENS!", 2.4);
        }
        if let Some(p) = ev.boss_phase {
            host.toast(if p == 1 { "HE CALLS FOR HELP!" } else { "HE IS ENRAGED!" }, 2.0);
        }
        if ev.boss_killed {
            host.toast("THE BOMB KING FALLS!", 2.4);
        }
        if ev.lastlight {
            self.say(host, "SECOND CHANCE!", 2.0);
        }
        if ev.bag_full && self.t - self.bag_full_at > 3.0 {
            self.bag_full_at = self.t;
            self.say(host, "PACK FULL - SELL SOMETHING", 1.6);
        }
        if ev.descended {
            self.enter_floor(host);
        }
        if (ev.died || ev.won) && !self.summary_sent {
            self.finish(host);
        }
        self.teach(host);
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        self.hitstop = (self.hitstop - dt).max(0.0);
        self.pump_toasts(host);
        self.shake.update(dt as f32);
        self.hurt_pop.update(dt as f32, 3.2);
        self.gain_pop.update(dt as f32, 5.0);
        self.apply_cmds(host);
        // Bag / tree toggles from the frame loop too, so they work with the game stopped.
        if self.core.freeze {
            let bits = self.pending | std::mem::take(&mut host.relic.edges);
            self.pending = 0;
            let mut open = None;
            if bits & (1 << 7) != 0 {
                open = Some(if host.relic.screen == Screen::Bag { Screen::None } else { Screen::Bag });
            }
            if bits & (1 << 8) != 0 {
                open = Some(if host.relic.screen == Screen::Tree { Screen::None } else { Screen::Tree });
            }
            if let Some(s) = open {
                host.relic.screen = s;
                host.relic.snap_gen = host.relic.snap_gen.wrapping_add(1);
            }
            if host.relic.screen == Screen::None {
                self.core.freeze = false;
            }
        } else if host.relic.screen != Screen::None {
            self.core.freeze = true;
        }
        host.relic.summary_ready = host.relic.summary.is_some() && self.t - self.over_at >= SUMMARY_LOCKOUT;
        // Brief flicker while invulnerable from a hit (the dive's own i-frames show as the dive).
        let hit_blink = self.core.player.iframes > 0 && self.core.player.dodge_t == 0 && !self.core.over && (self.t * 18.0) as i64 % 2 == 0;
        host.set_player_visible(!hit_blink);
        self.publish(host);
        self.camera(host, dt);
    }

    fn debug(&self) -> String {
        format!("floor={} level={} hp={:.0} live={} kills={} gold={}", self.core.floor, self.core.player.level, self.core.player.hp, self.core.alive_enemies, self.core.kills, self.core.player.gold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lessons_come_once_each_most_urgent_first() {
        let mut taught = 0u8;
        let quiet = LessonCue::default();
        assert_eq!(next_lesson(taught, quiet), None);
        let fight = LessonCue { foes_awake: true, ..quiet };
        assert_eq!(next_lesson(taught, fight), Some(LESSON_FIGHT));
        taught |= LESSON_FIGHT;
        assert_eq!(next_lesson(taught, fight), None, "shown once");
        // Low health with a potion in the pack beats everything else; without a potion there is nothing to teach.
        let crisis = LessonCue { foes_awake: true, hurt: true, low_hp: true, potions: 1, ..quiet };
        assert_eq!(next_lesson(taught, crisis), Some(LESSON_POTION));
        assert_eq!(next_lesson(taught, LessonCue { potions: 0, ..crisis }), Some(LESSON_DODGE));
        for bit in [LESSON_POTION, LESSON_DODGE] {
            taught |= bit;
        }
        let late = LessonCue { level_up: true, got_item: true, ..quiet };
        assert_eq!(next_lesson(taught, late), Some(LESSON_SKILLS));
        assert_eq!(next_lesson(taught | LESSON_SKILLS, late), Some(LESSON_PACK));
        assert_eq!(next_lesson(0xff, crisis), None);
    }

    /// Hitstop (checklist 4): a hit freezes the world for a tick, a kill or crit for two, a hurt for three, the boss
    /// for longer; nothing at all on a quiet tick.
    #[test]
    fn hits_freeze_the_world_for_a_beat_and_bigger_events_for_longer() {
        let mut ev = Events::default();
        assert_eq!(hitstop_for(&ev), 0.0);
        ev.hit = 1;
        let h = hitstop_for(&ev);
        assert!(h > 0.0 && h <= 0.07, "{h}");
        ev.kills = 1;
        let k = hitstop_for(&ev);
        assert!(k > h && k <= 0.2, "{k}");
        ev.hurt = true;
        assert!(hitstop_for(&ev) >= k && hitstop_for(&ev) <= 0.2);
        ev.boss_killed = true;
        assert!(hitstop_for(&ev) >= 0.25);
    }

    /// The arrival lines of a floor are never cut short: a lesson or an event message waits while one is queued or
    /// still on screen (the skill-point lesson used to replace the floor's name).
    #[test]
    fn messages_wait_for_the_arrival_lines() {
        assert!(toast_slot_free(0, 5.0, 4.0));
        assert!(!toast_slot_free(1, 5.0, 4.0), "an arrival line is still queued");
        assert!(!toast_slot_free(0, 3.0, 4.0), "the last arrival line is still on screen");
    }

    #[test]
    fn banking_rule_pays_most_on_death_and_all_on_victory() {
        assert_eq!(banked_gold(100, true), 100);
        assert_eq!(banked_gold(100, false), 60);
        assert_eq!(banked_gold(0, false), 0);
    }

    #[test]
    fn meta_defaults_and_legacy_steps() {
        let m = Meta::default();
        assert!(m.filter_commons && m.stash == 0);
        assert_eq!(legacy_bonus(0), 0.0);
        assert!(legacy_bonus(500) > 0.0);
        assert!((legacy_bonus(1_000_000) - 0.2).abs() < 1e-6, "capped at +20%");
    }

    #[test]
    fn mesh_buffers_build_valid_meshes() {
        let mut b = MeshBuf::default();
        b.push_box(Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0), 1.0);
        b.push_floor(0.0, 0.0, 2.0, 2.0, 0.0, 1.0);
        assert_eq!(b.pos.len(), 28);
        assert_eq!(b.idx.len(), 42);
        let mesh = b.into_mesh();
        assert_eq!(mesh.count_vertices(), 28);
    }

    #[test]
    fn every_model_exists_for_every_enemy_kind() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !root.exists() {
            return;
        }
        for k in 1..=7u8 {
            assert!(EnemyKind::from_u8(k).is_some());
            assert!(root.join(MODELS[k as usize]).exists(), "{}", MODELS[k as usize]);
        }
        assert!(root.join(CHEST_BASE).exists() && root.join(CHEST_LID).exists() && root.join(FLAME).exists());
    }
}
