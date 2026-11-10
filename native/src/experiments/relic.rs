//! Relic Descent: an original action-RPG "looter dungeon crawler" on Mario's real physics.
//!
//! You (Mario, Link, Young Link or Banjo) descend three procedurally built floors of linked
//! rooms, wake packs of SM64 critters, dodge with the dive (its i-frames are the dodge), slam the
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
//! (walks to and fights the nearest foe), `_END=win|lose`.
use super::geometry::add_box_collision;
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

/// Far from every course object and from Star Swarm's arena.
const ORIGIN: [f64; 3] = [220000.0, 1000.0, 220000.0];
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
        }
    }

    /// World position (SM64 units, Bevy-mapped) of a floor-plane point at `height`.
    fn at(p: [f32; 2], height: f32) -> Vec3 {
        to_bevy([ORIGIN[0] + p[0] as f64, ORIGIN[1] + height as f64, ORIGIN[2] + p[1] as f64])
    }

    fn world_pos(p: [f32; 2]) -> [f64; 3] {
        [ORIGIN[0] + p[0] as f64, ORIGIN[1], ORIGIN[2] + p[1] as f64]
    }

    /// Collision and display for the current floor. Returns the collision world.
    fn build_floor(&mut self, host: &mut Host) -> CollisionWorld {
        for e in self.floor_ents.drain(..) {
            host.commands.entity(e).despawn();
        }
        let d = &self.core.dungeon;
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let o = SurfaceOptions::default();
        let mut faces = vec![];
        let total = [W as f64 * TILE as f64, H as f64 * TILE as f64];
        // One big floor slab (walls fence the void), and a box per merged wall run.
        add_box_collision(&mut world, [ORIGIN[0] + total[0] / 2.0, ORIGIN[1] - 200.0, ORIGIN[2] + total[1] / 2.0], [total[0] + 3000.0, 400.0, total[1] + 3000.0], 0.0, &mut faces, &o);
        let mut walls = MeshBuf::default();
        for b in d.wall_boxes() {
            let (ox, oy) = box_origin(&b);
            let (w, h) = (b.w as f64 * TILE as f64, b.h as f64 * TILE as f64);
            let (cx, cz) = (ox as f64 * TILE as f64 + w / 2.0, oy as f64 * TILE as f64 + h / 2.0);
            add_box_collision(&mut world, [ORIGIN[0] + cx, ORIGIN[1] + 350.0, ORIGIN[2] + cz], [w, 700.0, h], 0.0, &mut faces, &o);
            walls.push_box(to_bevy([ORIGIN[0] + cx, ORIGIN[1] + WALL_H as f64 / 2.0, ORIGIN[2] + cz]), Vec3::new(w as f32, WALL_H, h as f32) * SCALE, 2.6);
        }
        // Pillars at room corners; torches on two of them.
        let mut flames = vec![];
        for (ri, r) in d.rooms.iter().enumerate() {
            let (c, hf) = (r.center(), r.half());
            let inset = 190.0;
            for (k, (sx, sz)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].into_iter().enumerate() {
                let p = [c[0] + sx * (hf[0] - inset), c[1] + sz * (hf[1] - inset)];
                add_box_collision(&mut world, [ORIGIN[0] + p[0] as f64, ORIGIN[1] + 150.0, ORIGIN[2] + p[1] as f64], [70.0, 300.0, 70.0], 0.0, &mut faces, &o);
                walls.push_box(Self::at(p, 150.0), Vec3::new(70.0, 300.0, 70.0) * SCALE, 2.6);
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
        let t = TILE;
        for y in 0..H as i32 {
            let mut x = 0;
            while x < W as i32 {
                let kind = d.tile(x, y);
                if kind == Tile::Void {
                    x += 1;
                    continue;
                }
                let x0 = x;
                while x < W as i32 && d.tile(x, y) == kind {
                    x += 1;
                }
                let (bx0, bx1) = (to_bevy([ORIGIN[0] + x0 as f64 * t as f64, 0.0, 0.0]).x, to_bevy([ORIGIN[0] + x as f64 * t as f64, 0.0, 0.0]).x);
                let (bz0, bz1) = (to_bevy([0.0, 0.0, ORIGIN[2] + y as f64 * t as f64]).z, to_bevy([0.0, 0.0, ORIGIN[2] + (y + 1) as f64 * t as f64]).z);
                let buf = match kind {
                    Tile::Room => &mut rooms_f,
                    Tile::Corridor => &mut corr_f,
                    _ => &mut boss_f,
                };
                buf.push_floor(bx0, bz0, bx1, bz1, (ORIGIN[1] as f32 + 0.6) * SCALE, 3.6);
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
                let e = host.spawn((scene(host.assets, FLAME), Transform::from_translation(Self::at(p, 300.0)).with_scale(Vec3::splat(2.2)), Visibility::Inherited));
                self.floor_ents.push(e);
            }
        }
        // Stairs: a gold ring with a column of light.
        if let Some(s) = self.core.stairs_pos() {
            let ring = host.meshes.add(Annulus::new(150.0 * SCALE, 185.0 * SCALE));
            let disc = host.meshes.add(Circle::new(150.0 * SCALE));
            let col = host.meshes.add(Cylinder::new(120.0 * SCALE, 900.0 * SCALE));
            let gold = host.unlit(Color::srgb(1.0, 0.82, 0.25));
            let soft = host.materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.9, 0.5, 0.22), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
            let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            for (mesh, mat, y, rot) in [(ring, gold.clone(), 4.0, flat), (disc, soft.clone(), 3.0, flat), (col, soft, 450.0, Quat::IDENTITY)] {
                let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(Self::at(s, y)).with_rotation(rot)));
                self.floor_ents.push(e);
            }
        }
        // Boss arena: a ring of dark light on the floor at the centre.
        if d.boss_floor {
            let c = d.rooms[d.exit].center();
            let ring = host.meshes.add(Annulus::new(520.0 * SCALE, 560.0 * SCALE));
            let mat = host.unlit(Color::srgb(0.9, 0.2, 0.25));
            let e = host.spawn((Mesh3d(ring), MeshMaterial3d(mat), Transform::from_translation(Self::at(c, 4.0)).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))));
            self.floor_ents.push(e);
        }
        self.built_floor = self.core.floor;
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
                let off = if target == "stairs" { [0.0, 0.0] } else { [0.0, -d.rooms[i].half()[1] + 330.0] };
                let p = [c[0] + off[0], c[1] + off[1]];
                self.core.player.pos = p;
                self.core.player.prev = p;
                host.place_mario(Self::world_pos(p), Some(0.0));
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
        self.core = Relic::new(seed, hero, start_floor, self.meta.stash);
        self.core.filter_commons = self.meta.filter_commons;
        let world = self.build_floor(host);
        let start = self.core.start_pos();
        let spawn = SpawnPoint { action: None, pos: [Self::world_pos(start)[0], ORIGIN[1] + 100.0, Self::world_pos(start)[2]], face_yaw: 0.0 };
        host.set_mario_world(Arc::new(world), spawn, -1e9);
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
        host.place_mario(Self::world_pos(start), Some(0.0));
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
        host.toast(if self.first_run { "DESCEND, FIND THE RELIC" } else { "ONCE MORE INTO THE DEEP" }, 1.8);
        self.first_run = false;
    }

    /// A new floor after the stairs: fresh collision, meshes and mood.
    fn enter_floor(&mut self, host: &mut Host) {
        let world = self.build_floor(host);
        let start = self.core.start_pos();
        let spawn = SpawnPoint { action: None, pos: [Self::world_pos(start)[0], ORIGIN[1] + 100.0, Self::world_pos(start)[2]], face_yaw: 0.0 };
        host.set_mario_world(Arc::new(world), spawn, -1e9);
        host.place_mario(Self::world_pos(start), Some(0.0));
        self.mood(host);
        self.leaping = false;
        host.relic.names.clear();
        host.relic.floor_gen = host.relic.floor_gen.wrapping_add(1);
        host.toast(&format!("FLOOR {}", self.core.floor), 1.6);
        if self.core.dungeon.boss_floor {
            host.toast("THE BOMB KING WAITS BELOW...", 2.4);
        }
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

    /// A chase camera from above and behind that ignores the (low) walls.
    fn camera(&mut self, host: &mut Host) {
        let focus = DVec3::from(host.render_pos()) + DVec3::new(0.0, 80.0, 0.0);
        let (yaw, pitch, dist) = (host.rig.yaw, 0.95f64, 1500.0f64);
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
        let lift = |p: [f32; 2], h: f64| {
            let w = Self::world_pos(p);
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
        let me = core.player.pos;
        let lerp = |p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a];
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
                        pos: Self::at([p[0] + shake, p[1]], foe_lift(e) + (t * 3.0 + i as f32).sin().abs() * if sp.flies() { 12.0 } else { 0.0 }),
                        yaw: self.yaws[i],
                        radius: e.radius,
                        pose: foe_pose(e, moving),
                        pulse: (1.0 + 0.12 * e.flash as f32 / 4.0) * if e.state == EState::Fuse { 1.0 + 0.2 * (t * 30.0).sin().abs() } else { 1.0 },
                        age: if e.kind == EnemyKind::Limbo { (60u16.saturating_sub(e.timer)) as f32 / 30.0 } else { 0.0 },
                    });
                }
                // The elite ring and health bar still come from the common path.
                item.kind = if hidden || (e.state == EState::Hidden) { 0 } else { e.kind as u8 };
                item.pos = Self::at(p, 0.0);
                item.yaw = 0.0;
                item.scale = e.radius / 27.0 / if e.mods != 0 { 1.2 } else { 1.0 };
                item.aux = e.mods as f32;
                if nbars < b.bars.len() && (e.hp < e.max_hp || e.is_elite()) && e.state != EState::Hidden {
                    let d2 = (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
                    if d2 < 2600.0 * 2600.0 {
                        b.bars[nbars] = BarView { pos: Self::at(p, e.radius * 2.0 + foe_lift(e)), frac: (e.hp / e.max_hp.max(1.0)).max(0.0), kind: if e.is_elite() { 2 } else { 1 } };
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
            item.pos = Self::at([p[0] + shake, p[1]], lift + hop + bob);
            item.yaw = self.yaws[i];
            item.scale = scale * pulse * (1.0 + 0.12 * e.flash as f32 / 4.0) * if e.mods != 0 { 1.2 } else { 1.0 } * (e.radius / enemy_stats(e.kind).radius / if e.mods != 0 { 1.2 } else { 1.0 });
            item.aux = e.mods as f32;
            // Health bar for hurt, elite and boss foes within view range.
            if nbars < b.bars.len() && (e.hp < e.max_hp || e.is_elite() || e.kind == EnemyKind::King) && e.kind != EnemyKind::King {
                let d2 = (p[0] - me[0]).powi(2) + (p[1] - me[1]).powi(2);
                if d2 < 2600.0 * 2600.0 {
                    b.bars[nbars] = BarView { pos: Self::at(p, e.radius * 2.0 + lift), frac: (e.hp / e.max_hp.max(1.0)).max(0.0), kind: if e.is_elite() { 2 } else { 1 } };
                    nbars += 1;
                }
            }
        }
        for (i, s) in core.shots.iter().enumerate() {
            let item = &mut b.shots[i];
            if s.alive {
                let p = lerp(s.prev, s.pos);
                *item = RItem { kind: if s.wave { 3 } else if s.from_player { 1 } else { 2 }, pos: Self::at(p, 85.0), yaw: 0.0, scale: s.radius / 36.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, h) in core.hazards.iter().enumerate() {
            let item = &mut b.hazards[i];
            if h.alive {
                *item = RItem { kind: 1, pos: Self::at(h.pos, 6.0), yaw: 0.0, scale: h.radius * SCALE, aux: 1.0 - h.delay as f32 / h.total as f32 };
            } else {
                item.kind = 0;
            }
        }
        for (i, r) in core.rings.iter().enumerate() {
            let rv = &mut b.rings[i];
            if r.alive {
                let kind = r.kind as u8;
                let scale = if r.kind == RingKind::Slash { r.radius * SCALE } else { r.radius * r.grow() * SCALE };
                rv.item = RItem { kind, pos: Self::at(r.center, 14.0), yaw: r.yaw, scale, aux: r.fade() };
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
                DropKind::Gold => *item = RItem { kind: 1, pos: Self::at(d.pos, 35.0 + pop), yaw: 0.0, scale: 1.0, aux: 0.0 },
                DropKind::Potion => *item = RItem { kind: 2, pos: Self::at(d.pos, bob + pop), yaw: 0.0, scale: 1.0, aux: 0.0 },
                DropKind::Item => *item = RItem { kind: 3, pos: Self::at(d.pos, bob + pop), yaw: 0.0, scale: 1.0 + 0.1 * d.item.rarity as u8 as f32, aux: d.item.rarity as u8 as f32 },
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
                b.labels[nlabels] = LabelView { on: true, pos: Self::at(d.pos, bob + pop), id, kind, rarity };
                nlabels += 1;
            }
        }
        for (i, c) in core.chests.iter().enumerate() {
            let item = &mut b.chests[i];
            match c {
                Some(ch) => *item = RItem { kind: 1, pos: Self::at(ch.pos, 0.0), yaw: 0.0, scale: 1.6, aux: if ch.opened { 1.0 } else { 0.0 } },
                None => item.kind = 0,
            }
        }
        for (i, f) in core.floaters.iter().enumerate().take(24) {
            b.floaters[i] = FloaterView { on: f.alive, pos: Self::at(f.pos, f.height + 80.0), value: f.value, kind: f.kind, age: f.age as f32 / 34.0 };
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
        // Minimap.
        b.mini.clear();
        for (i, r) in core.dungeon.rooms.iter().enumerate() {
            let rs = core.rooms[i];
            b.mini.push(MiniRoom { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, kind: match r.kind { RoomKind::Combat => 0, RoomKind::Start => 1, RoomKind::Treasure => 2, RoomKind::Stairs => 3, RoomKind::Boss => 4 }, seen: rs.visited, cleared: rs.cleared });
        }
        b.mini_player = [me[0] / TILE, me[1] / TILE];
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
        if self.core.freeze { 0.0 } else { 1.0 }
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
            // Walk at the nearest foe and keep swinging (screenshots and soak tests).
            let me = self.core.player.pos;
            let target = (0..ENEMY_SLOTS).filter(|i| self.core.enemies[*i].alive).min_by(|a, b| dist(self.core.enemies[*a].pos, me).partial_cmp(&dist(self.core.enemies[*b].pos, me)).unwrap()).map(|i| self.core.enemies[i].pos);
            if let Some(t) = target {
                let d = norm([t[0] - me[0], t[1] - me[1]]);
                let step = if dist(t, me) > 230.0 { 22.0 } else { 0.0 };
                host.shift_mario([d[0] as f64 * step, 0.0, d[1] as f64 * step]);
                host.sim.mario.face_angle[1] = radians_to_angle(d[0].atan2(d[1]) as f64) as f64;
                self.intent.attack = true;
            }
        }
        StepOverride { controller: Some(c), camera_yaw: None }
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.core.paused_for_ui() || self.core.over {
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
            let me = [(mario.pos[0] - ORIGIN[0]) as f32, (mario.pos[2] - ORIGIN[2]) as f32];
            let slid = self.core.dungeon.slide(me, [(vx * k) as f32, (vz * k) as f32], PLAYER_RADIUS + 15.0);
            host.shift_mario([(slid[0] - me[0]) as f64, 0.0, (slid[1] - me[1]) as f64]);
        }
        if self.spin_t > 0 {
            self.spin_t -= 1;
            host.sim.mario.face_angle[1] = to_s16(host.sim.mario.face_angle[1] + 6400.0) as f64;
        }
        let m = &host.sim.mario;
        let face = angle_to_radians(m.face_angle[1]) as f32;
        let frame = PlayerFrame { pos: [(m.pos[0] - ORIGIN[0]) as f32, (m.pos[2] - ORIGIN[2]) as f32], height: (m.pos[1] - ORIGIN[1]).max(0.0) as f32, face, pound_land, dive_started, punch_started, grounded: !air };
        let mario_pos = m.pos;
        let intent = self.intent;
        self.core.tick(&frame, &intent);
        let ev = self.core.ev;
        let tick = self.core.tick;
        self.feel(host, mario_pos);
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
            host.fx("break_box", Self::world_pos(ev.last_kill));
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
                host.toast(&format!("{} FOUND!", r.name()), 1.6);
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
            host.toast(&format!("LEVEL {}  -  SKILL POINT!", self.core.player.level), 1.8);
        }
        if ev.auto_equipped {
            host.toast("EQUIPPED A NEW ITEM", 1.4);
        }
        if ev.chest {
            host.fx("star_sound", mario_pos);
            host.toast("CHEST OPENED!", 1.6);
        }
        if ev.room_cleared {
            host.toast("ROOM CLEARED", 1.2);
        }
        if ev.token {
            host.fx("star_sound", mario_pos);
            host.toast("GOLD SKULLTULA TOKEN!", 1.6);
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
            host.toast("SECOND CHANCE!", 2.0);
        }
        if ev.bag_full && self.t - self.bag_full_at > 3.0 {
            self.bag_full_at = self.t;
            host.toast("PACK FULL - SELL SOMETHING", 1.6);
        }
        if ev.descended {
            self.enter_floor(host);
        }
        if (ev.died || ev.won) && !self.summary_sent {
            self.finish(host);
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
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
        self.camera(host);
    }

    fn debug(&self) -> String {
        format!("floor={} level={} hp={:.0} live={} kills={} gold={}", self.core.floor, self.core.player.level, self.core.player.hp, self.core.alive_enemies, self.core.kills, self.core.player.gold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
