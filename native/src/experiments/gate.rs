//! Hold the Gate: an original tower-defence mode where the hero fights beside the towers.
//!
//! The boards lie on the real castle grounds (Peach's lawn, see [`SITES`]): the road, the build grid and the gate
//! are drawn over the course's own ground, and foes and towers follow its height.
//!
//! Mario keeps his real physics: he runs the lawn, and his punch, dive and ground-pound hurt foes,
//! he scoops up the coins they drop, and standing near a tower rallies it (a ground-pound there
//! triggers an overdrive and clears jams). Everything else is a tower-defence ruleset:
//! six tower kinds with three levels each, 15 waves with a boss every fifth, two maps, a gate with
//! 20 lives.
//!
//! * The rules live in `gate_core.rs` (deterministic, seeded, unit-tested); this file glues them
//!   to the host: it lays the board on the course, feeds the core Mario's frame every 30 Hz tick, runs the
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
use super::foes::Pose;
use super::foes_view::FoeItem;
use super::gate_core::*;
use super::gate_view::*;
use super::swarm_view::RItem;
use super::textures::{textured, tiled_box};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::{scene, to_bevy};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::*;
use std::sync::Arc;

const SUMMARY_LOCKOUT: f64 = 1.2;
/// Space around the playfield inside the outer wall.
const MARGIN: f32 = 2.0 * TILE;
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
        // Ported units: sized from their radius by `foes_view`; flyers hover.
        EKind::Bat | EKind::Peahat => (1.0, 110.0),
        _ => (1.0, 0.0),
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

/// The real course the boards lie on: Peach's castle grounds (the gate you hold is the castle's).
pub const COURSE: &str = "castle_grounds";
/// Where each map's board lies on the course: course (x, z) of the board centre and quarter turns. Chosen by
/// [`board_fit`] (every road tile and every build tile on level, open, walkable lawn) and checked again by a test.
pub const SITES: [([f64; 2], u8); 2] = [([-2935.0, 4165.0], 2), ([-4935.0, 2165.0], 2)];

/// The ground under a board on the real course, sampled once per map on a grid (bilinear lookups after that).
pub struct BoardGround {
    pub frame: BoardFrame,
    x0: f32,
    z0: f32,
    step: f32,
    nx: usize,
    nz: usize,
    h: Vec<f32>,
    /// Grass tiles that cannot hold a tower here (a tree, a slope, water): they become rock in the rules.
    pub blocked: Vec<(i32, i32)>,
}

impl BoardGround {
    /// A flat stand-in (no game data, unit tests).
    pub fn flat(frame: BoardFrame) -> Self {
        Self { frame, x0: 0.0, z0: 0.0, step: 1.0, nx: 1, nz: 1, h: vec![frame.origin[1] as f32], blocked: vec![] }
    }

    pub fn sample(world: &CollisionWorld, obstacles: &[[f64; 2]], map: &Map, site: ([f64; 2], u8)) -> Self {
        let (centre, turn) = site;
        let base = board_fit(world, obstacles, map, centre, turn).map_or_else(|| world.find_floor(centre[0], 30000.0, centre[1]).height, |f| f.1);
        let frame = BoardFrame { origin: [centre[0], base, centre[1]], turn };
        let [hx, hz] = map.half();
        let (x0, z0, step) = (-hx - MARGIN, -hz - MARGIN, 55.0f32);
        let (nx, nz) = (((hx + MARGIN) * 2.0 / step).ceil() as usize + 1, ((hz + MARGIN) * 2.0 / step).ceil() as usize + 1);
        let mut h = Vec::with_capacity(nx * nz);
        for j in 0..nz {
            for i in 0..nx {
                let w = frame.to_world([x0 + i as f32 * step, z0 + j as f32 * step]);
                let f = world.find_floor(w[0], base + 700.0, w[1]);
                let y = if f.surface.is_some() && (f.height - base).abs() < 500.0 { f.height } else { base };
                h.push(y as f32);
            }
        }
        let mut blocked = vec![];
        for y in 0..map.h {
            for x in 0..map.w {
                if map.tile(x, y) == Some(Tile::Grass) && !tile_ground(world, obstacles, map, (x, y), centre, turn, base + 600.0).is_some_and(|t| (t - base).abs() < 260.0) {
                    blocked.push((x, y));
                }
            }
        }
        Self { frame, x0, z0, step, nx, nz, h, blocked }
    }

    /// Course height of the ground under a board-local point.
    pub fn height(&self, p: [f32; 2]) -> f64 {
        if self.nx < 2 {
            return self.h[0] as f64;
        }
        let fx = ((p[0] - self.x0) / self.step).clamp(0.0, (self.nx - 1) as f32 - 0.001);
        let fz = ((p[1] - self.z0) / self.step).clamp(0.0, (self.nz - 1) as f32 - 0.001);
        let (i, j) = (fx as usize, fz as usize);
        let (tx, tz) = (fx - i as f32, fz - j as f32);
        let at = |i: usize, j: usize| self.h[j * self.nx + i];
        let a = at(i, j) + (at(i + 1, j) - at(i, j)) * tx;
        let b = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * tx;
        (a + (b - a) * tz) as f64
    }

    /// Course position `height` above the ground under a board-local point.
    pub fn world(&self, p: [f32; 2], height: f64) -> [f64; 3] {
        let w = self.frame.to_world(p);
        [w[0], self.height(p) + height, w[1]]
    }

    /// Bevy position `height` above the ground under a board-local point.
    pub fn at(&self, p: [f32; 2], height: f32) -> Vec3 {
        to_bevy(self.world(p, height as f64))
    }

    /// Board-local point of a course position.
    pub fn local(&self, pos: [f64; 3]) -> [f32; 2] {
        self.frame.to_local(pos[0], pos[2])
    }
}

/// Course (x, z) of the loaded course's trees (objects, not static collision: they would stand in the road).
fn trees(host: &Host) -> Vec<[f64; 2]> {
    host.sim.mario.objects.objects.iter().filter(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::Tree).map(|o| [o.pos[0], o.pos[2]]).collect()
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
    /// The board on the real course (ground heights, frame, tiles the lawn cannot hold).
    ground: Arc<BoardGround>,
    sfx_tick: u32,
    /// Camera shake for leaks, slams and bosses.
    shake: super::feel::Shake,
    /// Last tick each tower kind made its shot sound (so a row of cannons is not a machine gun).
    shot_tick: [u32; 6],
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
            ground: Arc::new(BoardGround::flat(BoardFrame { origin: ORIGIN, turn: 0 })),
            sfx_tick: 0,
            shake: Default::default(),
            shot_tick: [0; 6],
            bot: false,
            first_run: true,
            ui_gen: 0,
            yaws: vec![0.0; ENEMY_SLOTS],
            script: vec![],
            script_next: 0,
            run_started: 0.0,
        }
    }

    /// A mesh of one quad per cell, draped over the real ground `lift` units up (course coordinates, Bevy scale).
    fn cells_mesh(map: &Map, ground: &BoardGround, cells: &[(i32, i32)], lift: f64) -> Mesh {
        let h = TILE / 2.0 * 0.985;
        let (mut pos, mut uv, mut idx) = (vec![], vec![], vec![]);
        for &(cx, cy) in cells {
            let c = map.center(cx, cy);
            let base = pos.len() as u32;
            // 3 x 3 vertices per tile so the quad follows gentle bumps in the lawn.
            for j in 0..3 {
                for i in 0..3 {
                    let (sx, sz) = (i as f32 - 1.0, j as f32 - 1.0);
                    let v = ground.at([c[0] + sx * h, c[1] + sz * h], lift as f32);
                    pos.push([v.x, v.y, v.z]);
                    uv.push([i as f32 * 0.5, j as f32 * 0.5]);
                }
            }
            for j in 0..2u32 {
                for i in 0..2u32 {
                    let k = base + j * 3 + i;
                    // Wound to face up whichever way the board is turned (the turn is a rotation, never a mirror).
                    idx.extend([k, k + 3, k + 1, k + 1, k + 3, k + 4]);
                }
            }
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

    /// A fresh rules state for the current map, with the tiles the real lawn cannot hold turned to rock.
    fn fresh_core(&self, seed: u64) -> Gate {
        let mut g = Gate::new(seed, self.map);
        let w = g.map.w;
        for &(x, y) in &self.ground.blocked {
            g.map.tiles[(y * w + x) as usize] = Tile::Rock;
        }
        g
    }

    /// Lay `map`'s board on the castle grounds: sample the ground under it, mark the tiles the lawn cannot hold,
    /// and draw the road, a faint build grid, the spawn pipes and the gate over the real course.
    fn build_board(&mut self, host: &mut Host, map: usize) {
        for e in self.map_ents.drain(..) {
            host.commands.entity(e).despawn();
        }
        let m = Map::new(map);
        let world = host.course_world();
        self.ground = Arc::new(if world.surfaces().is_empty() { BoardGround::flat(BoardFrame { origin: ORIGIN, turn: 0 }) } else { BoardGround::sample(&world, &trees(host), &m, SITES[map % SITES.len()]) });
        host.gate.frame = self.ground.frame;
        let g = self.ground.clone();
        let (mut road, mut light, mut dark, mut rocks) = (vec![], vec![], vec![], vec![]);
        for y in 0..m.h {
            for x in 0..m.w {
                match m.tile(x, y) {
                    Some(Tile::Path) => road.push((x, y)),
                    Some(Tile::Rock) => rocks.push((x, y)),
                    Some(Tile::Grass) if !g.blocked.contains(&(x, y)) => {
                        if (x + y) % 2 == 0 {
                            light.push((x, y))
                        } else {
                            dark.push((x, y))
                        }
                    }
                    _ => {}
                }
            }
        }
        // The road is a packed-earth path; build tiles are a light checker tint over the real lawn.
        let road_mat = textured(host, "sandstone", Color::srgb(0.95, 0.85, 0.65));
        let mesh = host.meshes.add(Self::cells_mesh(&m, &g, &road, 3.0));
        self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(road_mat), Transform::IDENTITY));
        // The map's rock tiles are flagstone plazas (nothing can be built on them).
        let stone = textured(host, "stone", Color::srgb(0.78, 0.76, 0.84));
        let mesh = host.meshes.add(Self::cells_mesh(&m, &g, &rocks, 2.5));
        self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(stone), Transform::IDENTITY));
        for (cells, rgba) in [(&light, [1.0f32, 1.0, 0.85, 0.16]), (&dark, [0.75, 1.0, 0.6, 0.10])] {
            let mat = host.materials.add(StandardMaterial { base_color: Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
            let mesh = host.meshes.add(Self::cells_mesh(&m, &g, cells, 2.0));
            self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY));
        }
        // The gate: two pillars and a lintel across the end of the road, a star above (meshes only: the course keeps
        // its own collision).
        let gate = m.gate;
        let out = g.frame.dir_to_world([1.0, 0.0]);
        let side = g.frame.dir_to_world([0.0, 1.0]);
        let gp = g.world([gate[0] + TILE * 0.62, gate[1]], 0.0);
        let yaw = (side[0]).atan2(side[1]) as f32;
        let brick = textured(host, "brick", Color::srgb(1.0, 0.9, 0.9));
        for sgn in [-1.0f64, 1.0] {
            let size = [150.0, 330.0, 150.0];
            let c = [gp[0] + side[0] * sgn * TILE as f64 * 0.62, gp[1] + 165.0, gp[2] + side[1] * sgn * TILE as f64 * 0.62];
            let mesh = host.meshes.add(tiled_box(size, 110.0));
            self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(brick.clone()), Transform::from_translation(to_bevy(c))));
        }
        let lintel = [150.0, 80.0, TILE as f64 * 1.62 + 150.0];
        let mesh = host.meshes.add(tiled_box(lintel, 110.0));
        self.spawn_ent(host, (Mesh3d(mesh), MeshMaterial3d(brick.clone()), Transform::from_translation(to_bevy([gp[0], gp[1] + 370.0, gp[2]])).with_rotation(Quat::from_rotation_y(yaw))));
        let _ = out;
        if host.asset_root.exists(STAR) {
            let e = crate::render::object_view::spawn_model(host.commands, host.assets, host.asset_root, STAR, true).insert((super::XpEntity, Transform::from_translation(to_bevy([gp[0], gp[1] + 520.0, gp[2]])).with_scale(Vec3::splat(2.2)), Visibility::Inherited)).id();
            self.map_ents.push(e);
        }
        // Spawn pipes, one per road.
        if host.asset_root.exists(PIPE) {
            for lane in &m.lanes {
                let p = lane.pts[0];
                let pos = g.at([p[0] - 60.0, p[1]], 0.0);
                let e = scene(host.assets, PIPE);
                let id = host.spawn((e, Transform::from_translation(pos).with_scale(Vec3::splat(1.3)), Visibility::Inherited));
                self.map_ents.push(id);
            }
        }
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
        self.build_board(host, self.map);
        self.core = self.fresh_core(self.seed);
        let start = self.mario_start();
        host.place_mario(self.ground.world(start, 0.0), Some(self.face_yaw()));
        // Look at the board from its near (+z) side, the way the old meadow was framed.
        let d = self.ground.frame.dir_to_world([0.0, 1.0]);
        host.rig.yaw = d[0].atan2(d[1]);
    }

    /// Mario's face yaw for looking up the board (toward board-local -z, the far side, as on the old meadow).
    fn face_yaw(&self) -> f64 {
        let d = self.ground.frame.dir_to_world([0.0, -1.0]);
        jumpman_sim::math::atan2s(d[1], d[0]) as f64
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        self.core = self.fresh_core(seed);
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
        host.place_mario(self.ground.world(start, 0.0), Some(self.face_yaw()));
        host.set_player_visible(true);
        host.gate.reset();
        host.gate.frame = self.ground.frame;
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
        let ahead = [m.pos[0] + face.sin() * 130.0, m.pos[1], m.pos[2] + face.cos() * 130.0];
        self.core.map.cell_at(self.ground.local(ahead))
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
        Self::cue(host, "move_menu");
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
                host.fx("coin", self.ground.world(p, 60.0));
                self.pin = Some(cell);
                self.set_menu(host, Menu::Card { tower: i });
            }
            Err(e) => {
                Self::cue(host, "bonk");
                host.toast(e.text(), 1.2);
            }
        }
    }

    /// A non-positional UI sound.
    fn cue(host: &mut Host, kind: &'static str) {
        host.shell.cues.push(crate::audio::SfxCue::Effect { kind, pos: None });
    }

    fn apply(&mut self, host: &mut Host, cmd: Cmd) {
        match cmd {
            Cmd::PickMap(m) if self.stage == Stage::MapSelect => {
                if m != self.map {
                    self.switch_map(host, m);
                }
                Self::cue(host, "select_mission");
                self.start_run(host);
            }
            Cmd::Retry | Cmd::OtherMap if self.stage == Stage::Over && host.gate.summary_ready => {
                if cmd == Cmd::OtherMap {
                    let next = (self.map + 1) % MAPS.len();
                    self.switch_map(host, next);
                }
                Self::cue(host, "select_mission");
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
                        Self::cue(host, "move_menu");
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
                            host.fx("star_sound", self.ground.world(p, 60.0));
                        }
                        Err(BuildError::NotBuildable) => {
                            Self::cue(host, "bonk");
                            host.toast("Already at max level", 1.0)
                        }
                        Err(e) => {
                            Self::cue(host, "bonk");
                            host.toast(e.text(), 1.2)
                        }
                    }
                }
            }
            Cmd::Sell => {
                if let Menu::Card { tower } = self.menu {
                    let v = self.core.sell(tower);
                    Self::cue(host, "coin_bounce");
                    host.toast(&format!("Sold for {v} coins"), 1.2);
                    self.pin = None;
                    self.set_menu(host, Menu::None);
                }
            }
            Cmd::CycleMode => {
                if let Menu::Card { tower } = self.menu {
                    self.core.cycle_mode(tower);
                    Self::cue(host, "move_menu");
                }
            }
            Cmd::Speed => {
                self.core.speed = if self.core.speed >= 2 { 1 } else { 2 };
                Self::cue(host, "cap_switch");
            }
            Cmd::Pause => {
                self.core.paused = !self.core.paused;
                Self::cue(host, "cap_switch");
            }
            Cmd::NextWave => {
                if !self.core.call_wave() {
                    Self::cue(host, "bonk");
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
        let g = self.ground.clone();
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
        b.foes.clear();
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
            if let Some(sp) = e.kind.species() {
                let hop = if e.kind == EKind::Tektite && e.dash_t > 0 { (e.dash_t as f32 / 14.0 * std::f32::consts::PI).sin() * 150.0 } else { 0.0 };
                let pose = if e.dash_t > 0 || e.stun_t > 0 { Pose::Attack } else { Pose::Move };
                b.foes.push(FoeItem { id: i as u32, species: sp, pos: g.at([p[0] + shake, p[1]], lift + bob + hop), yaw: self.yaws[i], radius: e.radius * 0.95, pose, pulse: 1.0 + 0.1 * e.flash as f32 / 4.0, age: 0.0 });
                // The entry stays (kind and health) so the health bar is drawn; the model comes from `foes_view`.
                *item = RItem { kind: e.kind as u8, pos: g.at(p, lift + bob + e.radius * 1.4), yaw: 0.0, scale: e.radius * 0.03, aux: (e.hp / e.max_hp).clamp(0.0, 1.0) };
                continue;
            }
            *item = RItem { kind: e.kind as u8, pos: g.at([p[0] + shake, p[1]], lift + bob), yaw: self.yaws[i], scale: scale * (1.0 + 0.1 * e.flash as f32 / 4.0), aux: (e.hp / e.max_hp).clamp(0.0, 1.0) };
        }
        for (i, tw) in core.towers.iter().enumerate() {
            let item = &mut b.towers[i];
            if !tw.alive {
                item.kind = 0;
                continue;
            }
            *item = TowerItem { kind: tw.kind as u8 + 1, level: tw.level, pos: g.at(tw.pos, 0.0), yaw: tw.aim, kick: tw.fired as f32 / 10.0, jam: tw.jam > 0, rally: tw.rally || tw.od_t > 0, selected: sel_tower == Some(i) };
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
            *item = RItem { kind, pos: g.at(q, h), yaw: 0.0, scale: 1.0, aux: 0.0 };
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
            *item = RItem { kind: 1, pos: g.at(p.pos, 45.0 + (t * 3.0 + i as f32).sin() * 10.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
        }
        for (i, bm) in core.beams.iter().enumerate() {
            let item = &mut b.beams[i];
            *item = if bm.alive { BeamItem { on: true, a: g.at(bm.from, 110.0), b: g.at(bm.to, 110.0), fade: bm.ttl as f32 / 6.0 } } else { BeamItem::default() };
        }
        for (i, f) in core.fx.iter().enumerate() {
            let item = &mut b.fx[i];
            if !f.alive {
                item.kind = 0;
                continue;
            }
            let fade = f.ttl as f32 / f.max.max(1) as f32;
            let r = f.radius * (0.35 + 0.65 * (1.0 - fade));
            *item = RItem { kind: f.kind, pos: g.at(f.pos, 8.0), yaw: 0.0, scale: (r * crate::render::SCALE).max(0.01), aux: fade };
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
                CursorView { show: true, pos: g.at(c, 1.0), ok: buildable || tower.is_some(), range, tint }
            }
            None => CursorView::default(),
        };
        b.summary_ready = b.summary.is_some() && self.t - self.over_at >= SUMMARY_LOCKOUT;
        b.hint = core.wave == 0 && self.stage == Stage::Playing;
    }

    fn camera(&mut self, host: &mut Host) {
        let hero = DVec3::from(host.render_pos());
        let g = self.ground.clone();
        let w = |p: [f32; 2], h: f64| DVec3::from(g.world(p, h));
        let centre = w([0.0, 0.0], 40.0);
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
                focus = w(c, 40.0);
            }
        }
        let (yaw, pitch, mut dist) = (host.rig.yaw, 0.98f64, (1.28 * fit + 300.0) * zoom);
        // Keep the gate and the road leading to it clear of the touch buttons, the preview panel and the safe-area insets.
        if zoom == 1.0 && host.gate.view[0] > 0.0 {
            let m = &self.core.map;
            let mut pts = vec![];
            let lane = &m.lanes[0];
            let mut seg = 0u16;
            for back in [0.0f32, 1.0, 2.0, 3.0] {
                let (p, _) = lane.sample(&mut seg, (lane.len - back * TILE).max(0.0), 0.0);
                pts.push(w(p, 0.0));
            }
            // The castle: pillars out to 0.62 tile past the gate point and 0.62 tile + 75 either side, a star about 700 high.
            let (cx, cz) = (TILE * 0.62 + 75.0, TILE * 0.62 + 75.0);
            let gate = m.gate;
            for (dx, dy, dz) in [(cx, 0.0, cz), (cx, 0.0, -cz), (cx, 700.0, 0.0), (cx, 420.0, cz), (cx, 420.0, -cz)] {
                pts.push(w([gate[0] + dx, gate[1] + dz], dy));
            }
            let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].map(|(a, b)| w([a * hx, b * hz], 0.0));
            (focus, dist) = frame_gate(&pts, &corners, focus, yaw, pitch, dist, 55.0, host.gate.view, host.gate.touch);
        }
        let eye = focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
        let mut pose = CamPose { eye, look: focus, up: DVec3::Y, fov: 55.0, near: 30.0, far: 40000.0 };
        self.shake.apply(&mut pose, dist * 0.012);
        host.cam.pose = Some(pose);
    }
}

impl Experiment for HoldTheGateExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Gate
    }
    /// The boards lie on the real castle grounds (the lawn west of the castle).
    fn course(&self) -> Option<&'static str> {
        Some(COURSE)
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
        self.load_script();
        host.rig.distance = 1300.0;
        host.rig.pitch = 0.72;
        host.gate.reset();
        self.switch_map(host, self.map);
        host.gate.active = true;
        host.gate.best = self.best;
        host.gate.stage = Stage::MapSelect;
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
            let me = self.ground.local(m.pos);
            for t in self.core.towers.iter().filter(|t| t.alive) {
                let d = [me[0] - t.pos[0], me[1] - t.pos[1]];
                let dist = d[0].hypot(d[1]);
                if dist < 100.0 && (m.pos[1] - self.ground.height(t.pos)) < 160.0 {
                    let k = (100.0 - dist) / dist.max(1.0);
                    push[0] += (d[0] * k) as f64;
                    push[1] += (d[1] * k) as f64;
                }
            }
        }
        if push != [0.0; 2] {
            let w = self.ground.frame.dir_to_world([push[0] as f32, push[1] as f32]);
            host.shift_mario([w[0], 0.0, w[1]]);
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
        let me = self.ground.local(m.pos);
        // The rules' facing is board-local: turn the course yaw back by the board's quarter turns.
        let face = face + self.ground.frame.turn as f32 * std::f32::consts::FRAC_PI_2;
        let frame = HeroFrame { pos: me, height: (m.pos[1] - self.ground.height(me)).max(0.0) as f32, face, punch_started, dive_started, pound_land };
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
        // Foes are solid: after they moved, shove the hero out of their bodies (`Gate::hero_body_push`).
        if self.stage == Stage::Playing && !self.core.over() {
            let m = &host.sim.mario;
            let me = self.ground.local(m.pos);
            let height = (m.pos[1] - self.ground.height(me)).max(0.0) as f32;
            let to = self.core.hero_body_push(me, height);
            if to != me {
                let w = self.ground.frame.dir_to_world([to[0] - me[0], to[1] - me[1]]);
                host.shift_mario([w[0], 0.0, w[1]]);
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
        self.shake.update(dt as f32);
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
        let g = self.ground.clone();
        let world = |p: [f32; 2], h: f64| g.world(p, h);
        // One distinct sound per tower kind (rate-limited per kind): thump, whoosh, zap, slam, coin.
        for (k, kind) in [(TKind::Bombard, "cannon_fire"), (TKind::Bloom, "dive"), (TKind::Prism, "teleport"), (TKind::Slab, "ground_pound_drop")] {
            let i = k as usize;
            if ev.fired_kinds & (1 << i) != 0 && tick >= self.shot_tick[i] + 6 {
                self.shot_tick[i] = tick;
                host.fx(kind, hero);
            }
        }
        if ev.income && tick >= self.shot_tick[TKind::Spout as usize] + 20 {
            self.shot_tick[TKind::Spout as usize] = tick;
            host.fx("coin_bounce", hero);
        }
        // Particles and shake (a few particles a tick at most).
        let up = |h: f64| [hero[0], hero[1] + h, hero[2]];
        if ev.kills > 0 {
            let n = (3 + ev.kills as usize * 2).min(10);
            host.particles.burst(world(ev.last_kill, 60.0), crate::fx::particles::BurstOptions { color: 0xfff1b8, count: n, speed: 240.0, life: 0.4, gravity: -500.0, size: 50.0, upward: 170.0, spread: 1.0, ..Default::default() });
        }
        if ev.splash {
            host.particles.burst(world(ev.at, 40.0), crate::fx::particles::BurstOptions { color: 0xff9a3c, count: 8, speed: 380.0, life: 0.4, gravity: -400.0, size: 60.0, upward: 200.0, spread: 1.0, ..Default::default() });
        }
        if ev.slam {
            self.shake.add(0.25);
            host.particles.burst(world(ev.at, 10.0), crate::fx::particles::BurstOptions { color: 0xd8c39a, count: 10, speed: 420.0, life: 0.45, gravity: -300.0, size: 64.0, upward: 80.0, spread: 1.0, ..Default::default() });
        }
        if ev.stomp {
            self.shake.add(0.2);
            host.particles.burst(world(ev.at, 30.0), crate::fx::particles::BurstOptions { color: 0xffffff, count: 8, speed: 360.0, life: 0.35, gravity: -400.0, size: 50.0, upward: 140.0, spread: 1.0, ..Default::default() });
        }
        if ev.pickup {
            host.particles.burst(up(90.0), crate::fx::particles::BurstOptions { color: 0xffd84a, count: 6, speed: 160.0, life: 0.4, gravity: 100.0, size: 36.0, upward: 260.0, spread: 1.0, ..Default::default() });
        }
        if ev.hero_hit {
            self.shake.add(0.3);
            host.particles.burst(up(80.0), crate::fx::particles::BurstOptions { color: 0xff5a4a, count: 8, speed: 300.0, life: 0.4, gravity: -600.0, size: 46.0, upward: 200.0, spread: 1.0, ..Default::default() });
        }
        if ev.overdrive {
            host.particles.burst(up(60.0), crate::fx::particles::BurstOptions { color: 0xffe066, count: 18, speed: 460.0, life: 0.6, gravity: 0.0, size: 50.0, upward: 160.0, spread: 1.0, ..Default::default() });
        }
        if ev.leaked > 0 {
            self.shake.add(0.6);
            host.particles.burst(world(self.core.map.gate, 80.0), crate::fx::particles::BurstOptions { color: 0xff4a3a, count: 14, speed: 420.0, life: 0.6, gravity: -300.0, size: 70.0, upward: 260.0, spread: 1.0, ..Default::default() });
        }
        if ev.boss_spawned {
            self.shake.add(0.6);
        }
        if ev.boss_killed {
            self.shake.add(1.0);
            for (color, speed) in [(0xffd24au32, 700.0f32), (0xffffffu32, 420.0)] {
                host.particles.burst(world(ev.last_kill, 120.0), crate::fx::particles::BurstOptions { color: color, count: 20, speed: speed, life: 0.8, gravity: -420.0, size: 90.0, upward: 320.0, spread: 1.0, ..Default::default() });
            }
        }
        if ev.jammed {
            host.fx("bonk", hero);
        }
        if ev.boss_spawned {
            host.fx("explosion", world(self.core.map.gate, 80.0));
        }
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
        if ev.splash && tick >= self.sfx_tick + 2 {
            self.sfx_tick = tick;
            host.fx("explosion", world(ev.at, 40.0));
        }
        if ev.leaked > 0 {
            host.fx("hurt", hero);
            host.toast(&format!("THEY GOT THROUGH!  {} lives left", self.core.lives.max(0)), 1.4);
        }
        if ev.wave_started {
            host.fx("select_mission", hero);
            let w = self.core.wave;
            let boss = preview(w).boss;
            host.toast(&if boss { format!("WAVE {w}  -  BOSS!") } else { format!("WAVE {w}") }, 1.8);
        }
        if ev.boss_killed {
            host.toast("BOSS DEFEATED!", 2.0);
            host.fx("star", hero);
        }
        if ev.wave_cleared {
            host.fx("star_sound", hero);
            host.toast(&format!("WAVE {} CLEARED", self.core.wave), 1.4);
        }
    }
}

/// Ground under one tile of a board laid at `origin` (course x, z) with `turn`: the height at its centre, or None
/// when the tile cannot hold a tower or a road (no floor, steep, water, a wall or a low ceiling in it, too bumpy).
pub fn tile_ground(world: &CollisionWorld, obstacles: &[[f64; 2]], map: &Map, cell: (i32, i32), origin: [f64; 2], turn: u8, probe_y: f64) -> Option<f64> {
    let c = map.center(cell.0, cell.1);
    // Course objects that stand in the way (the castle grounds' trees): not in the static collision.
    let [cx, cz] = BoardFrame { origin: [origin[0], 0.0, origin[1]], turn }.to_world(c);
    if obstacles.iter().any(|o| (o[0] - cx).hypot(o[1] - cz) < TILE as f64 * 0.75) {
        return None;
    }
    let mut hs = [0.0f64; 5];
    for (k, (dx, dz)) in [(0.0, 0.0), (-80.0, -80.0), (80.0, -80.0), (-80.0, 80.0), (80.0, 80.0)].iter().enumerate() {
        let [x, z] = BoardFrame { origin: [origin[0], 0.0, origin[1]], turn }.to_world([c[0] + dx, c[1] + dz]);
        let f = world.find_floor(x, probe_y, z);
        let s = f.surface.as_ref()?;
        if s.normal[1] < 0.9 || s.kind == jumpman_sim::surfaces::SURFACE_DEATH_PLANE || world.water_level(x, z) > f.height - 30.0 {
            return None;
        }
        hs[k] = f.height;
    }
    let (lo, hi) = hs.iter().fold((f64::MAX, f64::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
    if hi - lo > 110.0 {
        return None;
    }
    let [x, z] = BoardFrame { origin: [origin[0], 0.0, origin[1]], turn }.to_world(c);
    let mut at = [x, hs[0] + 60.0, z];
    if world.f32_find_wall_collision(&mut at, 0.0, 100.0) > 0 || world.find_ceil(at[0], hs[0] + 20.0, at[2]).height < hs[0] + 420.0 {
        return None;
    }
    Some(hs[0])
}

/// How well `map`'s board fits at `origin` / `turn`: (good grass tiles, base height, grass tiles lost), or None when
/// a road tile (or a spawn) has no good ground or the board is not roughly level.
pub fn board_fit(world: &CollisionWorld, obstacles: &[[f64; 2]], map: &Map, origin: [f64; 2], turn: u8) -> Option<(i32, f64, i32)> {
    let probe = 30000.0;
    let mut road = vec![];
    for y in 0..map.h {
        for x in 0..map.w {
            if map.tile(x, y) == Some(Tile::Path) {
                road.push(tile_ground(world, obstacles, map, (x, y), origin, turn, probe)?);
            }
        }
    }
    road.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let base = road[road.len() / 2];
    if road.iter().any(|h| (h - base).abs() > 260.0) {
        return None;
    }
    let (mut good, mut bad) = (0, 0);
    for y in 0..map.h {
        for x in 0..map.w {
            if map.tile(x, y) == Some(Tile::Grass) {
                match tile_ground(world, obstacles, map, (x, y), origin, turn, base + 600.0) {
                    Some(h) if (h - base).abs() < 260.0 => good += 1,
                    _ => bad += 1,
                }
            }
        }
    }
    Some((good, base, bad))
}

fn camera_basis(eye: DVec3, look: DVec3) -> (DVec3, DVec3, DVec3) {
    let f = (look - eye).normalize();
    let r = f.cross(DVec3::Y).normalize();
    (f, r, r.cross(f))
}

/// Logical-pixel position of `p` for a vertical-fov camera, or `None` behind it.
fn project(p: DVec3, eye: DVec3, look: DVec3, fov_deg: f64, w: f64, h: f64) -> Option<(f64, f64)> {
    let (f, r, u) = camera_basis(eye, look);
    let v = p - eye;
    let z = v.dot(f);
    if z < 1.0 {
        return None;
    }
    let t = (fov_deg.to_radians() / 2.0).tan();
    Some(((v.dot(r) / (z * t * w / h) * 0.5 + 0.5) * w, (0.5 - v.dot(u) / (z * t) * 0.5) * h))
}

/// The free screen rectangle `(left, top, right, bottom)` for the thing being defended: clear of the
/// safe-area insets, the HUD strip on top and, with the touch overlay, the BUILD/WAVE/x1 row and the
/// A/B grid on the right.
pub fn free_rect(view: [f32; 6], touch: bool) -> (f64, f64, f64, f64) {
    let [w, h, il, ir, it, ib] = view.map(|v| v as f64);
    let right_ui = if touch {
        let margin = (crate::touch::MARGIN_X as f64).max(ir);
        margin + 3.0 * crate::touch::MODE_SIZE as f64 + 2.0 * crate::touch::BUTTON_GAP as f64 + 14.0
    } else {
        ir.max(10.0) + 14.0
    };
    (il.max(10.0) + 6.0, it.max(8.0) + 52.0, w - right_ui, h - ib.max(8.0) - 6.0)
}

/// Pick a camera focus and distance so every point of `must_see` lands in [`free_rect`] while the
/// playfield corners stay on the left of the screen; zooms out a little only when shifting is not enough.
#[allow(clippy::too_many_arguments)]
pub fn frame_gate(must_see: &[DVec3], corners: &[DVec3], focus: DVec3, yaw: f64, pitch: f64, dist: f64, fov: f64, view: [f32; 6], touch: bool) -> (DVec3, f64) {
    let (w, h) = (view[0] as f64, view[1] as f64);
    let (l, t, r, b) = free_rect(view, touch);
    let eye_for = |focus: DVec3, dist: f64| focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
    let mut best = (focus, dist);
    for step in 0..=14 {
        let d = dist * (1.0 + 0.04 * step as f64);
        let mut f = focus;
        for _ in 0..8 {
            let eye = eye_for(f, d);
            let (fw, right, _) = camera_basis(eye, f);
            let fh = DVec3::new(fw.x, 0.0, fw.z).normalize();
            let (mut dx, mut dy) = (0.0f64, 0.0f64);
            for p in must_see {
                if let Some((sx, sy)) = project(*p, eye, f, fov, w, h) {
                    let cx = if sx > r { sx - r } else if sx < l { sx - l } else { 0.0 };
                    let cy = if sy > b { sy - b } else if sy < t { sy - t } else { 0.0 };
                    if cx.abs() > dx.abs() {
                        dx = cx;
                    }
                    if cy.abs() > dy.abs() {
                        dy = cy;
                    }
                }
            }
            if dx.abs() < 0.5 && dy.abs() < 0.5 {
                break;
            }
            // Pixels to world units at the focus depth.
            let per_px = 2.0 * (fov.to_radians() / 2.0).tan() * d / h;
            f += right * dx * per_px - fh * dy * per_px / pitch.sin().max(0.3);
        }
        best = (f, d);
        let eye = eye_for(f, d);
        let fits = must_see.iter().all(|p| project(*p, eye, f, fov, w, h).is_some_and(|(sx, sy)| sx >= l - 1.0 && sx <= r + 1.0 && sy >= t - 1.0 && sy <= b + 1.0));
        let keeps_left = corners.iter().all(|c| project(*c, eye, f, fov, w, h).is_none_or(|(sx, _)| sx >= l - 40.0));
        if fits && keeps_left {
            return best;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate, the road leading to it and the castle top must land in the free rectangle for the phone sizes and both maps.
    #[test]
    fn camera_keeps_the_gate_clear_of_touch_buttons_on_phones() {
        for (w, h, inset) in [(874.0f32, 402.0f32, [0.0f32, 0.0, 0.0, 0.0]), (844.0, 390.0, [59.0, 59.0, 21.0, 0.0]), (932.0, 430.0, [59.0, 59.0, 21.0, 0.0])] {
            for map in 0..MAPS.len() {
                let m = Map::new(map);
                let [hx, hz] = m.half();
                let mut pts = vec![];
                let mut seg = 0u16;
                for back in [0.0f32, 1.0, 2.0, 3.0] {
                    let (p, _) = m.lanes[0].sample(&mut seg, (m.lanes[0].len - back * TILE).max(0.0), 0.0);
                    pts.push(DVec3::new(ORIGIN[0] + p[0] as f64, ORIGIN[1], ORIGIN[2] + p[1] as f64));
                }
                let g = pts[0];
                let c = TILE as f64 * 0.62 + 75.0;
                pts.push(g + DVec3::new(c, 700.0, 0.0));
                pts.push(g + DVec3::new(c, 0.0, c));
                let corners = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].map(|(a, b)| DVec3::new(ORIGIN[0] + a * hx as f64, ORIGIN[1], ORIGIN[2] + b * hz as f64));
                let fit = (hx.max(hz * 1.5) as f64 + MARGIN as f64 * 0.3).max(2200.0);
                let focus = DVec3::new(ORIGIN[0], ORIGIN[1] + 40.0, ORIGIN[2]);
                let view = [w, h, inset[0], inset[1], inset[2], inset[3]];
                let (yaw, pitch) = (0.0f64, 0.98f64);
                let (f, d) = frame_gate(&pts, &corners, focus, yaw, pitch, 1.28 * fit + 300.0, 55.0, view, true);
                let eye = f + DVec3::new(yaw.sin() * pitch.cos() * d, pitch.sin() * d, yaw.cos() * pitch.cos() * d);
                let (l, t, r, b) = free_rect(view, true);
                for p in &pts {
                    let (sx, sy) = project(*p, eye, f, 55.0, w as f64, h as f64).expect("in front of the camera");
                    assert!(sx >= l - 2.0 && sx <= r + 2.0 && sy >= t - 2.0 && sy <= b + 2.0, "{w}x{h} map {map}: point {p:?} at ({sx:.0},{sy:.0}) outside ({l:.0},{t:.0})-({r:.0},{b:.0})");
                }
            }
        }
    }

    #[test]
    fn free_rect_reserves_the_button_column_only_with_touch() {
        let v = [874.0, 402.0, 0.0, 0.0, 0.0, 0.0];
        assert!(free_rect(v, true).2 < free_rect(v, false).2 - 100.0);
    }

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
        let g = BoardGround::flat(BoardFrame { origin: ORIGIN, turn: 1 });
        let mesh = HoldTheGateExperiment::cells_mesh(&m, &g, &[(0, 0), (1, 0), (5, 5)], 2.0);
        assert_eq!(mesh.count_vertices(), 27, "a draped 3 x 3 grid per cell");
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

    /// Both boards lie on open, level castle lawn: every road tile is good ground with no tree on it, and nearly
    /// every build tile is usable (the sites were picked by `scan_real_courses_for_boards`).
    #[test]
    fn the_boards_fit_the_real_castle_grounds() {
        use crate::game::world::{CourseRequest, LoadedCourse};
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !assets.join(format!("courses/{COURSE}.json")).exists() {
            return;
        }
        let course = LoadedCourse::load(&assets, CourseRequest::new(COURSE)).expect("course loads");
        let trees: Vec<[f64; 2]> = course.objects.objects.iter().filter(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::Tree).map(|o| [o.pos[0], o.pos[2]]).collect();
        assert!(!trees.is_empty(), "the castle grounds have trees to avoid");
        for (map, site) in SITES.iter().enumerate() {
            let m = Map::new(map);
            let (good, base, lost) = board_fit(&course.world, &trees, &m, site.0, site.1).unwrap_or_else(|| panic!("map {map}: a road tile has no good ground"));
            eprintln!("map {map}: {good} build tiles, {lost} lost, base {base:.0}");
            assert!(lost <= 3, "map {map} loses {lost} build tiles");
            let g = BoardGround::sample(&course.world, &trees, &m, *site);
            assert_eq!(g.blocked.len() as i32, lost);
            // The ground under the road is the course's own floor, within a step of the base.
            let lane = &m.lanes[0];
            let mut seg = 0u16;
            let mut d = 0.0;
            while d < lane.len {
                let (p, _) = lane.sample(&mut seg, d, 0.0);
                assert!((g.height(p) - base).abs() < 260.0, "map {map}: road at {p:?} is {:.0}", g.height(p));
                d += 100.0;
            }
        }
    }

    /// Search real courses for spots where a map's board fits (GATE_SCAN=course,course...): the tool that picked [`SITES`].
    #[test]
    #[ignore]
    fn scan_real_courses_for_boards() {
        use crate::game::world::{CourseRequest, LoadedCourse};
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let courses = std::env::var("GATE_SCAN").unwrap_or_else(|_| "castle_grounds,lon_lon_ranch,hyrule_field".into());
        for key in courses.split(',') {
            let Ok(course) = LoadedCourse::load(&assets, CourseRequest::new(key)) else { continue };
            let w = &course.world;
            let trees: Vec<[f64; 2]> = course.objects.objects.iter().filter(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::Tree).map(|o| [o.pos[0], o.pos[2]]).collect();
            let (mut minx, mut maxx, mut minz, mut maxz) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for s in w.surfaces() {
                if s.normal[1] > 0.9 {
                    for v in [s.v1, s.v2, s.v3] {
                        minx = minx.min(v[0]);
                        maxx = maxx.max(v[0]);
                        minz = minz.min(v[2]);
                        maxz = maxz.max(v[2]);
                    }
                }
            }
            for map in 0..MAPS.len() {
                let m = Map::new(map);
                let mut best: Vec<(i32, f64, f64, f64, u8, i32)> = vec![];
                let mut x = minx;
                while x < maxx {
                    let mut z = minz;
                    while z < maxz {
                        for q in 0..4u8 {
                            if let Some((score, base, bad)) = board_fit(w, &trees, &m, [x, z], q) {
                                best.push((score, x, z, base, q, bad));
                            }
                        }
                        z += 250.0;
                    }
                    x += 250.0;
                }
                best.sort_by(|a, b| b.0.cmp(&a.0));
                let mut picked: Vec<(i32, f64, f64, f64, u8, i32)> = vec![];
                for b in best {
                    if picked.iter().all(|p| (p.1 - b.1).hypot(p.2 - b.2) > 2500.0) {
                        picked.push(b);
                    }
                }
                eprintln!("{key} map {map}: {:?}", &picked[..picked.len().min(6)]);
            }
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
