//! Hold the Gate: an original tower-defence mode where the hero fights beside the towers.
//!
//! Mario keeps his real physics: he runs the meadow, and his punch, dive and ground-pound hurt foes,
//! he scoops up the coins they drop, and standing near a tower rallies it (a ground-pound there
//! triggers an overdrive and clears jams). Everything else is a tower-defence ruleset:
//! six tower kinds with three levels each, 15 waves with a boss every fifth, two maps, a gate with
//! 20 lives.
//!
//! * The rules live in `gate_core.rs` (deterministic, seeded, unit-tested); this file glues them
//!   to the host: it builds the meadow, feeds the core Mario's frame every 30 Hz tick, runs the
//!   build menu / tower card state machine and publishes render lists and HUD numbers through
//!   [`GateBridge`].
//! * `gate_view.rs` draws everything and owns the HUD, the build bar, the tower card, the wave
//!   preview, the map picker and the end-of-run card.
//!
//! Controls: stick/A/B/Z as usual. Tap (or click) a tile to build or inspect; `B` / the BUILD
//! touch button / pad `X` acts on the tile in front of Mario. `1`-`6` pick a tower (again to
//! build), `U` upgrade, `V` sell, `T` target mode, `N` call the next wave, `F` speed, `P` pause.
//!
//! Test hooks (none of them touch the saved record): `JUMPMAN_GATE_SEED=<n>`, `_MAP=0|1`
//! (skips the picker), `_WAVE=<n>` (start with wave n), `_COINS=<n>`, `_GOD=1`, `_BOT=1` (an
//! automatic builder), `_TOWERS="kind:level@x,y;..."` (pre-built towers), `_MENU=build@x,y|card@x,y`,
//! `_SPEED=2`, `_END=won|lost` (jump to the summary), `_LIVES=<n>`.
//! The best wave per map is kept in `gate.json` next to the save files.
use super::gate_core::*;
use super::gate_view::*;
use super::geometry::add_box_collision;
use super::swarm_view::RItem;
use super::textures::{textured, tiled_box, tiled_top};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::{scene, to_bevy};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::state::*;
use std::sync::Arc;

const SUMMARY_LOCKOUT: f64 = 1.2;
/// Space around the playfield inside the outer wall.
const MARGIN: f32 = 2.0 * TILE;
const TREE: &str = "actors/tree/bubbly_tree_geo.glb";
const PIPE: &str = "actors/warp_pipe/warp_pipe_geo.glb";
const STAR: &str = "actors/star/star_geo.glb";

/// Per-foe display: model scale and hover height (SM64 units).
fn look(k: EKind) -> (f32, f32) {
    match k {
        EKind::Goomba => (2.2, 0.0),
        EKind::Bomb => (2.0, 0.0),
        EKind::Bully => (1.8, 0.0),
        EKind::Spiny => (2.3, 0.0),
        EKind::Boo => (1.7, 130.0),
        EKind::Lakitu => (2.2, 160.0),
        EKind::Koopa => (2.2, 0.0),
        EKind::KingBomb => (1.1, 0.0),
        EKind::Chomp => (1.5, 0.0),
        EKind::Whomp => (0.85, 0.0),
    }
}

fn best_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("gate.json")
}

pub fn load_best() -> [u32; 2] {
    let v = std::fs::read_to_string(best_path()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let get = |i: usize| v.as_ref().and_then(|v| v["best_wave"][i].as_u64()).unwrap_or(0) as u32;
    [get(0), get(1)]
}

fn save_best(best: [u32; 2]) {
    let path = best_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "best_wave": best }).to_string());
}

fn env_num<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

fn run_seed() -> u64 {
    env_num("JUMPMAN_GATE_SEED").unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64))
}

/// Test hooks that make a run ineligible for the record.
fn cheating() -> bool {
    ["JUMPMAN_GATE_WAVE", "JUMPMAN_GATE_COINS", "JUMPMAN_GATE_GOD", "JUMPMAN_GATE_BOT", "JUMPMAN_GATE_TOWERS", "JUMPMAN_GATE_END", "JUMPMAN_GATE_LIVES"].iter().any(|k| std::env::var_os(k).is_some())
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Menu {
    None,
    Build { cell: (i32, i32), sel: Option<TKind> },
    Card { tower: usize },
}

pub struct HoldTheGateExperiment {
    core: Gate,
    seed: u64,
    runs: u32,
    map: usize,
    stage: Stage,
    /// Seconds since entering (render clock, keeps running while paused).
    t: f64,
    last_action: u32,
    best: [u32; 2],
    new_best: bool,
    menu: Menu,
    /// A tile picked by tapping; the tile in front of Mario is used otherwise.
    pin: Option<(i32, i32)>,
    over_at: f64,
    map_ents: Vec<Entity>,
    sfx_tick: u32,
    bot: bool,
    first_run: bool,
    ui_gen: u32,
    yaws: Vec<f32>,
    /// `JUMPMAN_GATE_SCRIPT` commands: (seconds since the run began, command).
    script: Vec<(f64, Cmd)>,
    script_next: usize,
    run_started: f64,
}

impl HoldTheGateExperiment {
    pub fn new() -> Self {
        Self {
            core: Gate::new(1, 0),
            seed: run_seed(),
            runs: 0,
            map: 0,
            stage: Stage::MapSelect,
            t: 0.0,
            last_action: 0,
            best: load_best(),
            new_best: false,
            menu: Menu::None,
            pin: None,
            over_at: 0.0,
            map_ents: vec![],
            sfx_tick: 0,
            bot: false,
            first_run: true,
            ui_gen: 0,
            yaws: vec![0.0; ENEMY_SLOTS],
            script: vec![],
            script_next: 0,
            run_started: 0.0,
        }
    }

    /// World position (Bevy units) of an arena-plane point at `height`.
    fn at(p: [f32; 2], height: f32) -> Vec3 {
        to_bevy([ORIGIN[0] + p[0] as f64, ORIGIN[1] + height as f64, ORIGIN[2] + p[1] as f64])
    }

    fn cells_mesh(map: &Map, cells: &[(i32, i32)]) -> Mesh {
        let h = TILE / 2.0 * 0.985;
        let (mut pos, mut uv, mut idx) = (vec![], vec![], vec![]);
        for &(cx, cy) in cells {
            let c = map.center(cx, cy);
            let base = pos.len() as u32;
            for (sx, sz) in [(-1.0f32, 1.0f32), (1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                pos.push([(c[0] + sx * h) * crate::render::SCALE, 0.0, (c[1] + sz * h) * crate::render::SCALE]);
                uv.push([(sx * 0.5 + 0.5) * 1.0, (1.0 - (sz * 0.5 + 0.5)) * 1.0]);
            }
            idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let n = pos.len();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_indices(bevy::render::mesh::Indices::U32(idx));
        mesh
    }

    fn spawn_ent(&mut self, host: &mut Host, bundle: impl Bundle) {
        let e = host.spawn(bundle);
        self.map_ents.push(e);
    }

    /// The meadow for `map`: collision (floor, border walls, rocks) and the matching meshes.
    fn build_arena(&mut self, host: &mut Host, map: usize) -> CollisionWorld {
        for e in self.map_ents.drain(..) {
            host.commands.entity(e).despawn();
        }
        let m = Map::new(map);
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let o = SurfaceOptions::default();
        let mut faces = vec![];
        let [hx, hz] = m.half();
        let (ax, az) = ((hx + MARGIN) as f64, (hz + MARGIN) as f64);
        let floor_top = ORIGIN[1];
        add_box_collision(&mut world, [ORIGIN[0], floor_top - 200.0, ORIGIN[2]], [ax * 2.0 + 1200.0, 400.0, az * 2.0 + 1200.0], 0.0, &mut faces, &o);
        let wall_h = 300.0;
        let wall_t = 240.0;
        let stone = textured(host, "stone", Color::srgb(0.85, 0.82, 0.95));
        let walls = [
            ([0.0, wall_h / 2.0, az + wall_t / 2.0], [ax * 2.0 + wall_t * 2.0, wall_h, wall_t]),
            ([0.0, wall_h / 2.0, -az - wall_t / 2.0], [ax * 2.0 + wall_t * 2.0, wall_h, wall_t]),
            ([ax + wall_t / 2.0, wall_h / 2.0, 0.0], [wall_t, wall_h, az * 2.0]),
            ([-ax - wall_t / 2.0, wall_h / 2.0, 0.0], [wall_t, wall_h, az * 2.0]),
        ];
        for (c, size) in walls {
            let center = [ORIGIN[0] + c[0], floor_top + c[1], ORIGIN[2] + c[2]];
            add_box_collision(&mut world, center, size, 0.0, &mut faces, &o);
            let mesh = host.meshes.add(tiled_box(size, 240.0));
            self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(stone.clone()), Transform::from_translation(to_bevy(center))));
        }
        // Ground: grass beyond the walls, checkered grass cells, the road, rocks.
        let outer = textured(host, "grassTop", Color::srgb(0.38, 0.55, 0.38));
        let mesh = host.meshes.add(tiled_top(60000.0, 60000.0, 500.0));
        self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(outer), Transform::from_translation(to_bevy([ORIGIN[0], floor_top - 4.0, ORIGIN[2]]))));
        let apron = textured(host, "grassTop", Color::srgb(0.55, 0.72, 0.5));
        let mesh = host.meshes.add(tiled_top((ax * 2.0) as f64, (az * 2.0) as f64, 440.0));
        self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(apron), Transform::from_translation(to_bevy([ORIGIN[0], floor_top - 1.0, ORIGIN[2]]))));
        let mut light = vec![];
        let mut dark = vec![];
        let mut road = vec![];
        let mut rocks = vec![];
        for y in 0..m.h {
            for x in 0..m.w {
                match m.tile(x, y) {
                    Some(Tile::Path) => road.push((x, y)),
                    Some(Tile::Rock) => rocks.push((x, y)),
                    _ => {
                        if (x + y) % 2 == 0 {
                            light.push((x, y))
                        } else {
                            dark.push((x, y))
                        }
                    }
                }
            }
        }
        // Rock cells also look like grass underneath.
        for &c in &rocks {
            dark.push(c);
        }
        let origin = to_bevy([ORIGIN[0], floor_top + 1.2, ORIGIN[2]]);
        for (cells, tex, tint) in [(&light, "grassTop", Color::srgb(0.72, 0.95, 0.62)), (&dark, "grassTop", Color::srgb(0.58, 0.82, 0.5)), (&road, "sandstone", Color::srgb(0.95, 0.85, 0.65))] {
            let mat = textured(host, tex, tint);
            let mesh = host.meshes.add(Self::cells_mesh(&m, cells));
            self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(origin)));
        }
        let rock_mat = textured(host, "stone", Color::srgb(0.7, 0.68, 0.78));
        for (x, y) in rocks {
            let c = m.center(x, y);
            let hgt = 120.0 + ((x * 7 + y * 13) % 5) as f64 * 22.0;
            let size = [TILE as f64 * 0.9, hgt, TILE as f64 * 0.9];
            let center = [ORIGIN[0] + c[0] as f64, floor_top + hgt / 2.0, ORIGIN[2] + c[1] as f64];
            add_box_collision(&mut world, center, size, 0.0, &mut faces, &o);
            let mesh = host.meshes.add(tiled_box(size, 120.0));
            self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(rock_mat.clone()), Transform::from_translation(to_bevy(center))));
        }
        // The gate: two pillars and a lintel across the end of the road, a star above.
        let g = m.gate;
        let gx = ORIGIN[0] + g[0] as f64 + TILE as f64 * 0.62;
        let gz = ORIGIN[2] + g[1] as f64;
        let brick = textured(host, "brick", Color::srgb(1.0, 0.9, 0.9));
        for s in [-1.0f64, 1.0] {
            let size = [150.0, 330.0, 150.0];
            let center = [gx, floor_top + 165.0, gz + s * TILE as f64 * 0.62];
            add_box_collision(&mut world, center, size, 0.0, &mut faces, &o);
            let mesh = host.meshes.add(tiled_box(size, 110.0));
            self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(brick.clone()), Transform::from_translation(to_bevy(center))));
        }
        let lintel = [150.0, 80.0, TILE as f64 * 1.62 + 150.0];
        let mesh = host.meshes.add(tiled_box(lintel, 110.0));
        self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(brick.clone()), Transform::from_translation(to_bevy([gx, floor_top + 370.0, gz]))));
        if host.asset_root.exists(STAR) {
            let e = crate::render::object_view::spawn_model(host.commands, host.assets, host.asset_root, STAR, true).insert((super::XpEntity, Transform::from_translation(to_bevy([gx, floor_top + 520.0, gz])).with_scale(Vec3::splat(2.2)), Visibility::Inherited)).id();
            self.map_ents.push(e);
        }
        // Spawn pipes, one per road.
        if host.asset_root.exists(PIPE) {
            for lane in &m.lanes {
                let p = lane.pts[0];
                let pos = to_bevy([ORIGIN[0] + p[0] as f64 - 60.0, floor_top, ORIGIN[2] + p[1] as f64]);
                let e = scene(host.assets, PIPE);
                let id = host.spawn((e, Transform::from_translation(pos).with_scale(Vec3::splat(1.3)), Visibility::Inherited));
                self.map_ents.push(id);
            }
        }
        // Trees beyond the wall for a horizon.
        if host.asset_root.exists(TREE) {
            for i in 0..26 {
                let a = i as f64 / 26.0 * std::f64::consts::TAU;
                let r = ax.max(az) * 1.35 + (i % 3) as f64 * 500.0;
                let k = 3.0 + (i % 4) as f32 * 0.6;
                let id = host.spawn((scene(host.assets, TREE), Transform::from_translation(to_bevy([ORIGIN[0] + a.cos() * r * 1.4, floor_top - 4.0, ORIGIN[2] + a.sin() * r])).with_scale(Vec3::splat(k)), Visibility::Inherited));
                self.map_ents.push(id);
            }
        }
        world.finalize();
        world
    }

    fn mario_start(&self) -> [f32; 2] {
        let m = &self.core.map;
        let (cx, cy) = (m.w / 2, m.h / 2);
        let mut best = (f32::MAX, m.center(cx, cy));
        for y in 0..m.h {
            for x in 0..m.w {
                if m.tile(x, y) == Some(Tile::Grass) {
                    let d = (x - cx).pow(2) as f32 + (y - cy - 1).pow(2) as f32 * 1.4;
                    if d < best.0 {
                        best = (d, m.center(x, y));
                    }
                }
            }
        }
        best.1
    }

    fn switch_map(&mut self, host: &mut Host, map: usize) {
        self.map = map % MAPS.len();
        self.core = Gate::new(self.seed, self.map);
        let world = self.build_arena(host, self.map);
        let spawn = SpawnPoint { action: None, pos: [ORIGIN[0], ORIGIN[1] + 100.0, ORIGIN[2]], face_yaw: 0.0 };
        host.set_mario_world(Arc::new(world), spawn, -1e9);
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        self.core = Gate::new(seed, self.map);
        self.stage = Stage::Playing;
        if let Some(w) = env_num::<u32>("JUMPMAN_GATE_WAVE") {
            self.core.wave = w.clamp(1, WAVES) - 1;
            self.core.timer = 30;
        }
        if let Some(c) = env_num::<u32>("JUMPMAN_GATE_COINS") {
            self.core.coins = c;
        }
        if let Some(l) = env_num::<i32>("JUMPMAN_GATE_LIVES") {
            self.core.lives = l;
            self.core.max_lives = l.max(self.core.max_lives);
        }
        self.core.god = std::env::var_os("JUMPMAN_GATE_GOD").is_some();
        self.core.speed = env_num::<u8>("JUMPMAN_GATE_SPEED").unwrap_or(1).clamp(1, 2);
        self.bot = std::env::var_os("JUMPMAN_GATE_BOT").is_some();
        // `JUMPMAN_GATE_TOWERS="0:2@5,3;2:1@6,3"` pre-builds towers (kind:level@x,y).
        if let Ok(list) = std::env::var("JUMPMAN_GATE_TOWERS") {
            let had = self.core.coins;
            self.core.coins = 1_000_000;
            for item in list.split(';') {
                let parsed = (|| {
                    let (kl, xy) = item.split_once('@')?;
                    let (k, l) = kl.split_once(':')?;
                    let (x, y) = xy.split_once(',')?;
                    Some((TKind::from_u8(k.trim().parse().ok()?)?, l.trim().parse::<u8>().ok()?, (x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?)))
                })();
                if let Some((kind, level, cell)) = parsed {
                    if let Ok(i) = self.core.build(kind, cell) {
                        for _ in 0..level.min(2) {
                            let _ = self.core.upgrade(i);
                        }
                    }
                }
            }
            self.core.coins = env_num::<u32>("JUMPMAN_GATE_COINS").unwrap_or(had);
        }
        // `JUMPMAN_GATE_FOES="1:1000;5:1400"` places foes (kind:offset along the road) and
        // `JUMPMAN_GATE_FREEZE=1` stops the clock: for screenshots of models and bars.
        if let Ok(list) = std::env::var("JUMPMAN_GATE_FOES") {
            for item in list.split(';') {
                if let Some((k, off)) = item.split_once(':') {
                    if let (Some(kind), Ok(off)) = (k.trim().parse::<u8>().ok().and_then(EKind::from_u8), off.trim().parse::<f32>()) {
                        if let Some(i) = self.core.spawn_enemy(kind, 0, off, 0.0) {
                            self.core.enemies[i].hp *= 0.6;
                        }
                    }
                }
            }
        }
        self.core.paused = std::env::var_os("JUMPMAN_GATE_FREEZE").is_some();
        self.new_best = false;
        self.last_action = 0;
        self.pin = None;
        self.menu = Menu::None;
        self.over_at = 0.0;
        self.yaws.iter_mut().for_each(|y| *y = 0.0);
        let start = self.mario_start();
        host.place_mario([ORIGIN[0] + start[0] as f64, ORIGIN[1], ORIGIN[2] + start[1] as f64], Some(32768.0));
        host.set_player_visible(true);
        host.gate.reset();
        host.gate.active = true;
        host.gate.stage = Stage::Playing;
        host.gate.best = self.best;
        host.gate.hint = true;
        if let Ok(spec) = std::env::var("JUMPMAN_GATE_MENU") {
            if let Some((kind, xy)) = spec.split_once('@') {
                if let Some((x, y)) = xy.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?))) {
                    self.select_cell(host, (x, y));
                    if kind == "build" {
                        if let Menu::Build { cell, .. } = self.menu {
                            let sel = env_num::<u8>("JUMPMAN_GATE_SEL").and_then(TKind::from_u8);
                            self.menu = Menu::Build { cell, sel };
                        }
                    }
                }
            }
        }
        if let Ok(end) = std::env::var("JUMPMAN_GATE_END") {
            self.core.waves_cleared = if end == "won" { WAVES } else { 7 };
            self.core.wave = if end == "won" { WAVES } else { 8 };
            self.core.kills = 317;
            self.core.earned = 2480;
            self.core.phase = if end == "won" { Phase::Won } else { Phase::Lost };
            self.finish(host);
        }
        self.load_script();
        let msg = if self.first_run { "HOLD THE GATE!" } else { "AGAIN!" };
        host.toast(msg, 1.6);
        self.first_run = false;
    }

    /// `JUMPMAN_GATE_SCRIPT="2:tap@5,3;3:pick2;4:pick2;6:upgrade"`: UI commands at run-relative seconds, for
    /// end-to-end checks of the build flow without a pointer. Commands: tap@x,y (cell), pick<k>, confirm, upgrade,
    /// sell, mode, close, speed, pause, wave, interact, map0, map1, retry, othermap.
    fn load_script(&mut self) {
        self.script.clear();
        self.script_next = 0;
        self.run_started = self.t;
        let Ok(list) = std::env::var("JUMPMAN_GATE_SCRIPT") else { return };
        for item in list.split(';') {
            let Some((at, what)) = item.split_once(':') else { continue };
            let Ok(at) = at.trim().parse::<f64>() else { continue };
            let what = what.trim();
            let cmd = if let Some(xy) = what.strip_prefix("tap@") {
                let Some((x, y)) = xy.split_once(',').and_then(|(x, y)| Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))) else { continue };
                Cmd::Tap(self.core.map.center(x, y))
            } else if let Some(k) = what.strip_prefix("pick") {
                let Ok(k) = k.parse::<u8>() else { continue };
                Cmd::PickType(k)
            } else {
                match what {
                    "map0" => Cmd::PickMap(0),
                    "map1" => Cmd::PickMap(1),
                    "retry" => Cmd::Retry,
                    "othermap" => Cmd::OtherMap,
                    "confirm" => Cmd::Confirm,
                    "upgrade" => Cmd::Upgrade,
                    "sell" => Cmd::Sell,
                    "mode" => Cmd::CycleMode,
                    "close" => Cmd::Close,
                    "speed" => Cmd::Speed,
                    "pause" => Cmd::Pause,
                    "wave" => Cmd::NextWave,
                    "interact" => Cmd::Interact,
                    _ => continue,
                }
            };
            self.script.push((at, cmd));
        }
    }

    fn finish(&mut self, host: &mut Host) {
        let cleared = self.core.waves_cleared;
        let m = self.map;
        self.new_best = !cheating() && cleared > self.best[m];
        if self.new_best {
            self.best[m] = cleared;
            save_best(self.best);
        }
        self.over_at = self.t;
        self.stage = Stage::Over;
        self.menu = Menu::None;
        host.gate.stage = Stage::Over;
        host.gate.summary = Some(Summary { won: self.core.phase == Phase::Won, map: m, waves_cleared: cleared, kills: self.core.kills, coins: self.core.earned, best: self.best[m], new_best: self.new_best });
        host.gate.summary_ready = false;
        host.gate.best = self.best;
    }

    // ----- menu state machine ---------------------------------------------------------------

    fn auto_cell(&self, host: &Host) -> Option<(i32, i32)> {
        let m = &host.sim.mario;
        let face = angle_to_radians(m.face_angle[1]);
        let p = [(m.pos[0] - ORIGIN[0]) as f32 + face.sin() as f32 * 130.0, (m.pos[2] - ORIGIN[2]) as f32 + face.cos() as f32 * 130.0];
        self.core.map.cell_at(p)
    }

    fn cursor_cell(&self, host: &Host) -> Option<(i32, i32)> {
        match self.menu {
            Menu::Build { cell, .. } => Some(cell),
            Menu::Card { tower } => Some(self.core.towers[tower].cell),
            Menu::None => self.pin.or_else(|| self.auto_cell(host)),
        }
    }

    fn set_menu(&mut self, host: &mut Host, menu: Menu) {
        if self.menu != menu {
            let structural = std::mem::discriminant(&self.menu) != std::mem::discriminant(&menu)
                || match (self.menu, menu) {
                    (Menu::Build { cell: a, .. }, Menu::Build { cell: b, .. }) => a != b,
                    (Menu::Card { tower: a }, Menu::Card { tower: b }) => a != b,
                    _ => false,
                };
            if structural {
                self.ui_gen = self.ui_gen.wrapping_add(1);
                host.gate.ui_gen = self.ui_gen;
                host.gate.focus = 0;
            }
            self.menu = menu;
        }
    }

    fn select_cell(&mut self, host: &mut Host, cell: (i32, i32)) {
        if let Some(i) = self.core.tower_at(cell) {
            self.pin = Some(cell);
            self.set_menu(host, Menu::Card { tower: i });
            return;
        }
        match self.core.map.tile(cell.0, cell.1) {
            Some(Tile::Grass) => {
                self.pin = Some(cell);
                let sel = if let Menu::Build { sel, .. } = self.menu { sel } else { None };
                self.set_menu(host, Menu::Build { cell, sel });
            }
            Some(Tile::Path) => {
                host.toast("The road: towers go on grass", 1.2);
                self.set_menu(host, Menu::None);
            }
            _ => {
                host.toast("Can't build on rock", 1.2);
                self.set_menu(host, Menu::None);
            }
        }
    }

    fn try_build(&mut self, host: &mut Host, kind: TKind, cell: (i32, i32)) {
        match self.core.build(kind, cell) {
            Ok(i) => {
                let p = self.core.towers[i].pos;
                host.fx("coin", [ORIGIN[0] + p[0] as f64, ORIGIN[1] + 60.0, ORIGIN[2] + p[1] as f64]);
                self.pin = Some(cell);
                self.set_menu(host, Menu::Card { tower: i });
            }
            Err(e) => host.toast(e.text(), 1.2),
        }
    }

    fn apply(&mut self, host: &mut Host, cmd: Cmd) {
        match cmd {
            Cmd::PickMap(m) if self.stage == Stage::MapSelect => {
                if m != self.map {
                    self.switch_map(host, m);
                }
                self.start_run(host);
            }
            Cmd::Retry | Cmd::OtherMap if self.stage == Stage::Over && host.gate.summary_ready => {
                if cmd == Cmd::OtherMap {
                    let next = (self.map + 1) % MAPS.len();
                    self.switch_map(host, next);
                }
                self.start_run(host);
            }
            _ if self.stage != Stage::Playing => {}
            Cmd::Tap(p) => match self.core.map.cell_at(p) {
                Some(cell) => self.select_cell(host, cell),
                None => {
                    self.pin = None;
                    self.set_menu(host, Menu::None);
                }
            },
            Cmd::Interact => {
                if self.menu != Menu::None {
                    self.set_menu(host, Menu::None);
                    self.pin = None;
                } else if let Some(cell) = self.cursor_cell(host) {
                    self.select_cell(host, cell);
                }
            }
            Cmd::Close => {
                self.pin = None;
                self.set_menu(host, Menu::None);
            }
            Cmd::PickType(k) => {
                let Some(kind) = TKind::from_u8(k) else { return };
                // From the keyboard with no build bar open: open it on the tile in front of Mario.
                if !matches!(self.menu, Menu::Build { .. }) {
                    self.pin = None;
                    self.menu = Menu::None;
                    if let Some(cell) = self.auto_cell(host) {
                        self.select_cell(host, cell);
                    }
                }
                if let Menu::Build { cell, sel } = self.menu {
                    if sel == Some(kind) {
                        self.try_build(host, kind, cell);
                    } else {
                        self.menu = Menu::Build { cell, sel: Some(kind) };
                    }
                }
            }
            Cmd::Confirm => {
                if let Menu::Build { cell, sel: Some(kind) } = self.menu {
                    self.try_build(host, kind, cell);
                }
            }
            Cmd::Upgrade => {
                if let Menu::Card { tower } = self.menu {
                    match self.core.upgrade(tower) {
                        Ok(()) => {
                            let p = self.core.towers[tower].pos;
                            host.fx("star_sound", [ORIGIN[0] + p[0] as f64, ORIGIN[1] + 60.0, ORIGIN[2] + p[1] as f64]);
                        }
                        Err(BuildError::NotBuildable) => host.toast("Already at max level", 1.0),
                        Err(e) => host.toast(e.text(), 1.2),
                    }
                }
            }
            Cmd::Sell => {
                if let Menu::Card { tower } = self.menu {
                    let v = self.core.sell(tower);
                    host.toast(&format!("Sold for {v} coins"), 1.2);
                    self.pin = None;
                    self.set_menu(host, Menu::None);
                }
            }
            Cmd::CycleMode => {
                if let Menu::Card { tower } = self.menu {
                    self.core.cycle_mode(tower);
                }
            }
            Cmd::Speed => self.core.speed = if self.core.speed >= 2 { 1 } else { 2 },
            Cmd::Pause => self.core.paused = !self.core.paused,
            Cmd::NextWave => {
                if !self.core.call_wave() {
                    host.toast(if self.core.wave >= WAVES { "That was the last wave" } else { "A wave is still arriving" }, 1.0);
                }
            }
            _ => {}
        }
    }

    // ----- per-frame publishing -------------------------------------------------------------

    fn card_view(&self, tower: usize) -> Option<CardView> {
        let t = self.core.towers.get(tower).filter(|t| t.alive)?;
        let s = t.kind.spec(t.level);
        let up = self.core.upgrade_cost(tower);
        Some(CardView {
            kind: t.kind as u8 + 1,
            level: t.level,
            mode: t.mode as u8,
            up_cost: up,
            afford: up.is_some_and(|c| self.core.coins >= c),
            sell: self.core.sell_value(tower),
            jam: t.jam > 0,
            rally: t.rally,
            overdrive: t.od_t > 0,
            kills: t.kills,
            dealt: if t.kind == TKind::Spout { 0 } else { t.dealt as u32 },
            range: s.range,
            damage: s.damage,
            per_sec: TICKS_PER_SEC as f32 / s.interval.max(1.0),
            effect: if t.kind == TKind::Bombard { s.radius } else { s.effect },
        })
    }

    fn publish(&mut self, host: &mut Host) {
        let a = host.alpha.clamp(0.0, 1.0) as f32;
        let t = self.t as f32;
        let core = &self.core;
        let lerp = |p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a];
        // Selected / pinned cell and menu.
        let cursor_cell = self.cursor_cell(host);
        let menu_cell = match self.menu {
            Menu::Card { tower } => Some(core.towers[tower].cell),
            _ => None,
        };
        let sel_tower = match self.menu {
            Menu::Card { tower } => Some(tower),
            _ => None,
        };
        let _ = menu_cell;
        // A card for a tower that was sold (or never existed) closes.
        if let Menu::Card { tower } = self.menu {
            if !core.towers[tower].alive {
                self.menu = Menu::None;
                self.pin = None;
                self.ui_gen = self.ui_gen.wrapping_add(1);
                host.gate.ui_gen = self.ui_gen;
            }
        }
        let core = &self.core;
        let b = &mut *host.gate;
        b.time = t;
        b.stage = self.stage;
        for (i, e) in core.enemies.iter().enumerate() {
            let item = &mut b.enemies[i];
            if !e.alive {
                item.kind = 0;
                continue;
            }
            let (scale, lift) = look(e.kind);
            let p = lerp(e.prev, e.pos);
            if e.dir[0].abs() + e.dir[1].abs() > 0.01 {
                self.yaws[i] = e.dir[0].atan2(e.dir[1]);
            }
            let bob = if e.kind.flying() { (t * 2.2 + i as f32).sin() * 14.0 } else { 0.0 };
            let shake = if e.stun_t > 0 { (t * 60.0).sin() * 8.0 } else { 0.0 };
            *item = RItem { kind: e.kind as u8, pos: Self::at([p[0] + shake, p[1]], lift + bob), yaw: self.yaws[i], scale: scale * (1.0 + 0.1 * e.flash as f32 / 4.0), aux: (e.hp / e.max_hp).clamp(0.0, 1.0) };
        }
        for (i, tw) in core.towers.iter().enumerate() {
            let item = &mut b.towers[i];
            if !tw.alive {
                item.kind = 0;
                continue;
            }
            *item = TowerItem { kind: tw.kind as u8 + 1, level: tw.level, pos: Self::at(tw.pos, 0.0), yaw: tw.aim, kick: tw.fired as f32 / 10.0, jam: tw.jam > 0, rally: tw.rally || tw.od_t > 0, selected: sel_tower == Some(i) };
        }
        for (i, p) in core.projs.iter().enumerate() {
            let item = &mut b.projs[i];
            if !p.alive {
                item.kind = 0;
                continue;
            }
            let q = lerp(p.prev, p.pos);
            let (h, kind) = if p.kind == 1 {
                let k = 1.0 - p.t as f32 / p.total.max(1) as f32;
                (70.0 + (k * std::f32::consts::PI).sin() * 240.0, 1)
            } else {
                (110.0, 2)
            };
            *item = RItem { kind, pos: Self::at(q, h), yaw: 0.0, scale: 1.0, aux: 0.0 };
        }
        for (i, p) in core.pickups.iter().enumerate() {
            let item = &mut b.pickups[i];
            if !p.alive {
                item.kind = 0;
                continue;
            }
            // Blink out in the last second.
            let blink = p.ttl < 30 && (p.ttl / 3) % 2 == 0;
            if blink {
                item.kind = 0;
                continue;
            }
            *item = RItem { kind: 1, pos: Self::at(p.pos, 45.0 + (t * 3.0 + i as f32).sin() * 10.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
        }
        for (i, bm) in core.beams.iter().enumerate() {
            let item = &mut b.beams[i];
            *item = if bm.alive { BeamItem { on: true, a: Self::at(bm.from, 110.0), b: Self::at(bm.to, 110.0), fade: bm.ttl as f32 / 6.0 } } else { BeamItem::default() };
        }
        for (i, f) in core.fx.iter().enumerate() {
            let item = &mut b.fx[i];
            if !f.alive {
                item.kind = 0;
                continue;
            }
            let fade = f.ttl as f32 / f.max.max(1) as f32;
            let r = f.radius * (0.35 + 0.65 * (1.0 - fade));
            *item = RItem { kind: f.kind, pos: Self::at(f.pos, 8.0), yaw: 0.0, scale: (r * crate::render::SCALE).max(0.01), aux: fade };
        }
        // HUD.
        b.hud = Hud {
            coins: core.coins,
            lives: core.lives,
            max_lives: core.max_lives,
            wave: core.wave,
            waves: WAVES,
            timer: (core.timer as f32 / TICKS_PER_SEC as f32).ceil() as u32,
            spawning: core.spawning || core.alive_enemies > 0 && core.timer == 0,
            alive: core.alive_enemies as u32,
            speed: core.speed,
            paused: core.paused,
            can_call: !core.spawning && core.wave < WAVES && !core.over(),
            preview_current: core.wave > 0 && (core.spawning || core.wave >= WAVES),
            preview_wave: if core.wave > 0 && (core.spawning || core.wave >= WAVES) { core.wave } else { core.next_wave() },
        };
        b.hud.spawning = core.spawning || (core.wave >= WAVES && core.alive_enemies > 0);
        b.preview = preview(b.hud.preview_wave);
        // Menu and cursor.
        let blocked_reason = |cell: (i32, i32)| -> Option<&'static str> {
            match core.map.tile(cell.0, cell.1) {
                Some(Tile::Grass) if core.tower_at(cell).is_none() => {
                    if core.tower_count() >= MAX_TOWERS {
                        Some("Tower limit reached")
                    } else {
                        None
                    }
                }
                _ => Some("Can't build there"),
            }
        };
        b.menu = match self.menu {
            Menu::None => MenuView::None,
            Menu::Build { cell, sel } => MenuView::Build(BuildView { cell, sel: sel.map(|k| k as u8), afford: std::array::from_fn(|i| core.coins >= ALL_TKINDS[i].spec(0).cost), blocked: blocked_reason(cell) }),
            Menu::Card { tower } => match self.card_view(tower) {
                Some(c) => MenuView::Card(c),
                None => MenuView::None,
            },
        };
        b.cursor = match cursor_cell.filter(|_| self.stage == Stage::Playing) {
            Some(cell) => {
                let c = core.map.center(cell.0, cell.1);
                let tower = core.tower_at(cell);
                let buildable = blocked_reason(cell).is_none();
                let (range, tint) = match (self.menu, tower) {
                    (Menu::Build { sel: Some(k), .. }, _) => (k.spec(0).range, TOWER_TINTS[k as usize]),
                    (_, Some(i)) => {
                        let t = &core.towers[i];
                        (t.kind.spec(t.level).range, TOWER_TINTS[t.kind as usize])
                    }
                    _ => (0.0, [1.0; 3]),
                };
                CursorView { show: true, pos: Self::at(c, 1.0), ok: buildable || tower.is_some(), range, tint }
            }
            None => CursorView::default(),
        };
        b.summary_ready = b.summary.is_some() && self.t - self.over_at >= SUMMARY_LOCKOUT;
        b.hint = core.wave == 0 && self.stage == Stage::Playing;
    }

    fn camera(&mut self, host: &mut Host) {
        let hero = DVec3::from(host.render_pos());
        let centre = DVec3::new(ORIGIN[0], ORIGIN[1] + 40.0, ORIGIN[2]);
        let [hx, hz] = self.core.map.half();
        // Mostly the whole meadow; drift a little toward the hero.
        let rel = (hero - centre) * 0.22;
        let focus = centre + DVec3::new(rel.x.clamp(-300.0, 300.0), 0.0, rel.z.clamp(-200.0, 200.0)) + DVec3::new(0.0, 0.0, 0.0);
        let fit = (hx.max(hz * 1.5) as f64 + MARGIN as f64 * 0.3).max(2200.0);
        let mut focus = focus;
        let mut zoom = 1.0;
        // Test hooks: `JUMPMAN_GATE_ZOOM=0.4` pulls the camera in, `JUMPMAN_GATE_FOCUS=x,y` (cells) aims it.
        if let Some(z) = env_num::<f64>("JUMPMAN_GATE_ZOOM") {
            zoom = z;
            if let Some((x, y)) = std::env::var("JUMPMAN_GATE_FOCUS").ok().and_then(|v| v.split_once(',').and_then(|(a, b)| Some((a.parse::<i32>().ok()?, b.parse::<i32>().ok()?)))) {
                let c = self.core.map.center(x, y);
                focus = DVec3::new(ORIGIN[0] + c[0] as f64, ORIGIN[1] + 40.0, ORIGIN[2] + c[1] as f64);
            }
        }
        // On a phone the touch buttons cover the right edge: slide the meadow left so the gate stays visible.
        if host.gate.touch && zoom == 1.0 {
            focus.x += fit * 0.22;
        }
        let (yaw, pitch, dist) = (host.rig.yaw, 0.98f64, (1.28 * fit + 300.0) * zoom);
        let eye = focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
        host.cam.pose = Some(CamPose { eye, look: focus, up: DVec3::Y, fov: 55.0, near: 30.0, far: 40000.0 });
    }
}

impl Experiment for HoldTheGateExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Gate
    }
    fn replaces_course(&self) -> bool {
        true
    }
    /// The map picker and the soft pause stand the hero still.
    fn time_scale(&self) -> f64 {
        if self.stage == Stage::MapSelect || self.core.paused { 0.0 } else { 1.0 }
    }

    fn enter(&mut self, host: &mut Host) {
        host.set_clear(Color::srgb(0.45, 0.68, 0.95));
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(0x3a78d0, 0x8cc4ff, 0xfff0c8).with_clouds(0.6));
        host.cam.fog = Some((Color::srgb(0.7, 0.85, 1.0), 9000.0, 30000.0));
        host.shell.plain_camera = true;
        self.best = load_best();
        let map = env_num::<usize>("JUMPMAN_GATE_MAP");
        self.map = map.unwrap_or(0) % MAPS.len();
        self.core = Gate::new(self.seed, self.map);
        self.load_script();
        let world = self.build_arena(host, self.map);
        let spawn = SpawnPoint { action: None, pos: [ORIGIN[0], ORIGIN[1] + 100.0, ORIGIN[2]], face_yaw: 0.0 };
        host.set_mario_world(Arc::new(world), spawn, -1e9);
        host.rig.distance = 1300.0;
        host.rig.pitch = 0.72;
        host.gate.reset();
        host.gate.active = true;
        host.gate.best = self.best;
        host.gate.stage = Stage::MapSelect;
        let start = self.mario_start();
        host.place_mario([ORIGIN[0] + start[0] as f64, ORIGIN[1], ORIGIN[2] + start[1] as f64], Some(32768.0));
        if map.is_some() {
            self.start_run(host);
        }
    }

    fn exit(&mut self, host: &mut Host) {
        host.gate.active = false;
        host.gate.summary = None;
        host.gate.menu = MenuView::None;
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.set_player_visible(true);
        host.cam.fog = None;
        host.cam.pose = None;
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        if self.stage == Stage::MapSelect {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.stage != Stage::Playing {
            return;
        }
        // Towers are solid: nudge the hero out of them.
        let mut push = [0.0f64; 2];
        {
            let m = &host.sim.mario;
            for t in self.core.towers.iter().filter(|t| t.alive) {
                let d = [(m.pos[0] - ORIGIN[0]) as f32 - t.pos[0], (m.pos[2] - ORIGIN[2]) as f32 - t.pos[1]];
                let dist = d[0].hypot(d[1]);
                if dist < 100.0 && (m.pos[1] - ORIGIN[1]) < 160.0 {
                    let k = (100.0 - dist) / dist.max(1.0);
                    push[0] += (d[0] * k) as f64;
                    push[1] += (d[1] * k) as f64;
                }
            }
        }
        if push != [0.0; 2] {
            host.shift_mario([push[0], 0.0, push[1]]);
        }
        let m = &host.sim.mario;
        let action = m.action;
        let started = |a: u32| action == a && self.last_action != a;
        let punch_started = started(ACT_PUNCHING) || started(ACT_MOVE_PUNCHING) || started(ACT_JUMP_KICK);
        let dive_started = started(ACT_DIVE);
        let pound_land = started(ACT_GROUND_POUND_LAND);
        self.last_action = action;
        let face = angle_to_radians(m.face_angle[1]) as f32;
        let mario_pos = m.pos;
        let frame = HeroFrame { pos: [(m.pos[0] - ORIGIN[0]) as f32, (m.pos[2] - ORIGIN[2]) as f32], height: (m.pos[1] - ORIGIN[1]).max(0.0) as f32, face, punch_started, dive_started, pound_land };
        if self.core.paused {
            return;
        }
        let steps = self.core.speed.clamp(1, 2);
        for s in 0..steps {
            if self.bot && self.core.tick % 20 == 0 {
                self.core.bot_step();
            }
            // One-tick edges (the hero's punch, dive, landing) only count once.
            let f = if s == 0 { frame } else { HeroFrame { punch_started: false, dive_started: false, pound_land: false, ..frame } };
            self.core.tick(&f);
            self.react(host, mario_pos);
            if self.core.over() {
                self.finish(host);
                break;
            }
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        while let Some(&(at, cmd)) = self.script.get(self.script_next) {
            if self.t - self.run_started < at {
                break;
            }
            self.script_next += 1;
            host.gate.cmds.push(cmd);
        }
        let mut cmds = std::mem::take(&mut host.gate.cmds);
        for c in cmds.drain(..) {
            self.apply(host, c);
        }
        // Hand the (empty) buffer back so its capacity is reused.
        cmds.append(&mut host.gate.cmds);
        host.gate.cmds = cmds;
        host.set_player_visible(true);
        self.publish(host);
        self.camera(host);
    }

    fn debug(&self) -> String {
        format!("wave={} lives={} coins={} towers={} live={} kills={}", self.core.wave, self.core.lives, self.core.coins, self.core.tower_count(), self.core.alive_enemies, self.core.kills)
    }
}

impl HoldTheGateExperiment {
    /// Sound, toasts and particles for the events of the tick that just ran.
    fn react(&mut self, host: &mut Host, hero: [f64; 3]) {
        let ev = self.core.ev;
        let tick = self.core.tick;
        let world = |p: [f32; 2], h: f64| [ORIGIN[0] + p[0] as f64, ORIGIN[1] + h, ORIGIN[2] + p[1] as f64];
        if ev.kills > 0 && tick >= self.sfx_tick + 3 {
            self.sfx_tick = tick;
            host.fx("break_box", world(ev.last_kill, 60.0));
        }
        if ev.pickup {
            host.fx("coin", hero);
        }
        if ev.hero_hit && tick >= self.sfx_tick + 2 {
            host.fx("bonk", hero);
        }
        if ev.overdrive {
            host.fx("star_sound", hero);
            host.toast("OVERDRIVE!", 1.2);
        }
        if ev.stomp {
            host.fx("bonk", world(ev.at, 60.0));
        }
        if (ev.slam || ev.splash) && tick >= self.sfx_tick + 2 {
            self.sfx_tick = tick;
            host.fx("bonk", world(ev.at, 40.0));
        }
        if ev.leaked > 0 {
            host.fx("bonk", world(self.core.map.gate, 80.0));
            host.toast(&format!("THEY GOT THROUGH!  {} lives left", self.core.lives.max(0)), 1.4);
        }
        if ev.wave_started {
            let w = self.core.wave;
            let boss = preview(w).boss;
            host.toast(&if boss { format!("WAVE {w}  -  BOSS!") } else { format!("WAVE {w}") }, 1.8);
        }
        if ev.boss_killed {
            host.toast("BOSS DEFEATED!", 2.0);
            host.fx("star_sound", hero);
        }
        if ev.wave_cleared {
            host.toast(&format!("WAVE {} CLEARED", self.core.wave), 1.4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_covers_every_foe() {
        for k in ALL_EKINDS {
            let (s, lift) = look(k);
            assert!(s > 0.0 && lift >= 0.0);
            assert_eq!(k.flying(), lift > 0.0, "{}", k.name());
        }
    }

    #[test]
    fn cells_mesh_builds_one_quad_per_cell() {
        let m = Map::new(0);
        let mesh = HoldTheGateExperiment::cells_mesh(&m, &[(0, 0), (1, 0), (5, 5)]);
        assert_eq!(mesh.count_vertices(), 12);
    }

    #[test]
    fn mario_starts_on_grass_near_the_middle() {
        for map in 0..MAPS.len() {
            let mut x = HoldTheGateExperiment::new();
            x.core = Gate::new(1, map);
            let p = x.mario_start();
            let (cx, cy) = x.core.map.cell_at(p).unwrap();
            assert_eq!(x.core.map.tile(cx, cy), Some(Tile::Grass));
            assert!(p[0].abs() < 2.0 * TILE && p[1].abs() < 3.0 * TILE);
        }
    }

    #[test]
    fn best_wave_survives_a_round_trip_per_map() {
        let dir = std::env::temp_dir().join(format!("gate-best-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("gate.json");
        std::fs::write(&path, serde_json::json!({ "best_wave": [7, 12] }).to_string()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!((v["best_wave"][0].as_u64(), v["best_wave"][1].as_u64()), (Some(7), Some(12)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
