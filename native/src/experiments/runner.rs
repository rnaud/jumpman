//! Endless Run (`runner.ts`): Mario auto-runs down a generated track using his real SM64
//! physics. You steer, jump, long-jump and dive; one fall ends the run.
//!
//! Daily runs record your best attempt and replay it as a blue ghost Mario (a second skinned
//! Mario fed from recorded frames). Blocks use the real SM64 textures, per theme, with distance
//! fog that follows the theme; decor = trees on islands / towers / spinning cubes.
//! Not ported: the dome's cloud layer (palette blends per theme) and the coin glb.
use super::crowd::{init_clips, spawn_body, CrowdLook, CrowdMario};
use super::dock::DockItem;
use super::geometry::{add_box_collision, add_face};
use super::textures::{textured, tiled_box, tiled_top};
use super::runner_gen::*;
use super::{store, CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::to_bevy;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::collision::{Surface, SurfaceOptions};
use jumpman_sim::state::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Far from every course object, so nothing in the hidden course reaches Mario.
const ORIGIN_X: f64 = 120000.0;
const ORIGIN_TOP: f64 = 1000.0;
const AHEAD: f64 = 9000.0;
const BEHIND: f64 = 2500.0;
/// Ticks of "READY..." before the first run of a visit; a retry only gets a short beat (death to running again in under
/// 2 s, `MINIGAMES_AUDIT_3.md`).
const COUNTDOWN: u32 = 45;
const RETRY_COUNTDOWN: u32 = 12;
/// A run is over once Mario is this far below the last floor he stood on (block bottoms are 260 below their tops, so this
/// is clearly under the track; it was 1300, which spent most of a second watching the fall).
const FALL_OUT: f64 = 800.0;
/// Ticks after the fall before A starts the next run (a jump press still held from the fall must not skip the result).
const RETRY_LOCK: u32 = 8;

/// Mario's body for the boulders: a 37-unit cylinder 160 tall (SM64's hitbox).
const MARIO_R: f64 = 37.0;
const MARIO_H: f64 = 160.0;

/// The horizontal move (x, z) that puts Mario (feet at `m`) outside a boulder (a sphere centred at `c`, radius `r`), or
/// `None` when they do not touch. He leaves sideways, away from the ball's centre (straight back down the track when dead
/// centre), also when he jumps into its upper half (the ball has no collision surfaces: he would sink into it). The body
/// is [`super::body::SQUEEZE`] of the contact reach, so the knock-back touch still lands first.
pub fn boulder_push(m: [f64; 3], c: [f64; 3], r: f64) -> Option<[f64; 2]> {
    // Closest point of Mario's vertical axis to the centre.
    let y = c[1].clamp(m[1], m[1] + MARIO_H);
    let dy = c[1] - y;
    let reach = (r + MARIO_R) * super::body::SQUEEZE as f64;
    if dy.abs() >= reach {
        return None;
    }
    let want = (reach * reach - dy * dy).sqrt();
    let (dx, dz) = (m[0] - c[0], m[2] - c[2]);
    let d = dx.hypot(dz);
    if d >= want {
        return None;
    }
    let (ux, uz) = if d > 0.01 { (dx / d, dz / d) } else { (-1.0, 0.0) };
    Some([c[0] + ux * want - m[0], c[2] + uz * want - m[2]])
}

/// Countdown ticks for the `runs`-th run of this visit (1-based).
pub fn countdown_for(runs: u32) -> u32 {
    if runs <= 1 { COUNTDOWN } else { RETRY_COUNTDOWN }
}

/// `SKIES` (top, mid, bottom) of the TS runner.
const SKY_STOPS: [(u32, u32, u32); 3] = [(0x2f7fd6, 0x8fcdf5, 0xe9f6ff), (0x3a3f8f, 0xf29a5c, 0xffe0b0), (0x3b1d73, 0xd48cff, 0xffd0ec)];
const SKY: [[f32; 3]; 3] = [[0.56, 0.80, 0.96], [0.95, 0.60, 0.36], [0.83, 0.55, 1.0]];

/// Today's shared seed: everyone runs the same track on the same day (UTC).
pub fn daily_seed_from_days(days_since_epoch: i64) -> f64 {
    // Civil-from-days (Howard Hinnant).
    let z = days_since_epoch + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y * 10000 + m * 100 + d) as f64
}

/// Score of a finished run: whole metres, and whether it beats `best`.
pub fn score_run(distance: f64, best: f64) -> (f64, bool) {
    let score = distance.floor();
    (score, score > best)
}

pub fn daily_seed() -> f64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    daily_seed_from_days(secs.div_euclid(86400))
}

fn surface_id(s: &Arc<Surface>) -> usize {
    Arc::as_ptr(s) as usize
}

struct Built {
    segment: Segment,
    entities: Vec<Entity>,
    surfaces: HashSet<usize>,
    coins: Vec<CoinEntry>,
    hazards: Vec<(Hazard, Entity, f64)>,
}

struct CoinEntry {
    entity: Entity,
    pos: [f64; 3],
    taken: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunState {
    Ready,
    Running,
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Daily,
    Random,
}

pub struct EndlessRunnerExperiment {
    track: Option<RunnerTrack>,
    seed: f64,
    mode: Mode,
    /// Pillar/hurdle collision faces (by surface identity), for glancing off instead of stopping.
    pillar_faces: HashMap<usize, (BlockKind, f64)>,
    run_ticks: u32,
    hits: u32,
    blocked: u32,
    built: HashMap<usize, Built>,
    materials: HashMap<(usize, u8), Handle<StandardMaterial>>,
    state: RunState,
    ticks: u32,
    coins: u32,
    best: f64,
    distance: f64,
    theme: i32,
    runs: u32,
    last_top: f64,
    cam_pos: DVec3,
    cam_look: DVec3,
    sky: [f32; 3],
    coin_mesh: Option<Handle<Mesh>>,
    coin_mat: Option<Handle<StandardMaterial>>,
    boulder: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    t: f64,
    tex: HashMap<(&'static str, u32), Handle<StandardMaterial>>,
    recording: Vec<Frame>,
    ghost_frames: Vec<Frame>,
    ghost: Option<CrowdMario>,
    spinners: Vec<(Entity, [f64; 3], Vec3, f32)>,
    tree_ok: bool,
    /// Ticks since the run ended (the result screen ignores the jump that was still held when Mario fell).
    over_ticks: u32,
    /// Hundreds of metres already announced this run, and whether the run has passed the stored best.
    milestone: u32,
    passed_best: bool,
}

/// One recorded tick: x, y, z, yaw, action, arg, state, timer, speed, vy.
type Frame = [f64; 10];

const GHOST_KEY: &str = "jumpman.runner.ghost";
const PASTELS: [u32; 5] = [0xffb3d9, 0xb3e5ff, 0xd4b3ff, 0xfff0a3, 0xb3ffd1];
const FOG: [([f32; 3], f32); 3] = [([0.81, 0.91, 0.98], 7000.0), ([0.97, 0.81, 0.62], 7000.0), ([0.92, 0.75, 0.95], 7000.0)];
const TREE: &str = "actors/tree/bubbly_tree_geo.glb";

/// (top, side) texture per theme and block kind (`LOOK`).
fn look(theme: usize, kind: BlockKind) -> (&'static str, &'static str) {
    match (theme, kind) {
        (0, BlockKind::Floor) => ("grassTop", "grassEdge"),
        (0, BlockKind::Bridge | BlockKind::Hurdle) => ("planks", "planks"),
        (0, BlockKind::Pillar) => ("stone", "stone"),
        (1, BlockKind::Floor) => ("flagstone", "brick"),
        (1, BlockKind::Bridge) => ("planks", "planks"),
        (1, BlockKind::Hurdle) => ("brick", "brick"),
        (1, BlockKind::Pillar) => ("sandstone", "sandstone"),
        (_, BlockKind::Floor) => ("checker", "whiteBrick"),
        (_, BlockKind::Bridge) => ("rainbow", "rainbow"),
        (_, BlockKind::Hurdle) => ("whiteBrick", "whiteBrick"),
        (_, BlockKind::Pillar) => ("gold", "gold"),
    }
}

fn hex(c: u32) -> Color {
    Color::srgb(((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0)
}

fn record_frame(m: &jumpman_sim::mario_runtime::MarioRuntime) -> Frame {
    [m.pos[0].round(), m.pos[1].round(), m.pos[2].round(), m.face_angle[1], m.action as f64, m.action_arg, m.action_state, m.action_timer, m.forward_vel.round(), m.vel[1].round()]
}

impl EndlessRunnerExperiment {
    pub fn new(seed: Option<f64>) -> Self {
        Self {
            track: None,
            seed: seed.unwrap_or_else(daily_seed),
            mode: if seed.is_some() { Mode::Random } else { Mode::Daily },
            pillar_faces: HashMap::new(),
            run_ticks: 0,
            hits: 0,
            blocked: 0,
            built: HashMap::new(),
            materials: HashMap::new(),
            state: RunState::Ready,
            ticks: 0,
            coins: 0,
            best: 0.0,
            distance: 0.0,
            theme: -1,
            runs: 0,
            last_top: ORIGIN_TOP,
            cam_pos: DVec3::ZERO,
            cam_look: DVec3::ZERO,
            sky: SKY[0],
            coin_mesh: None,
            coin_mat: None,
            boulder: None,
            t: 0.0,
            tex: HashMap::new(),
            recording: vec![],
            ghost_frames: vec![],
            ghost: None,
            spinners: vec![],
            tree_ok: false,
            over_ticks: 0,
            milestone: 0,
            passed_best: false,
        }
    }

    fn best_key(&self) -> String {
        if self.mode == Mode::Daily {
            format!("jumpman.runner.best.daily.{}", self.seed)
        } else {
            "jumpman.runner.best".into()
        }
    }

    /// Textured material for a block face (`tint` is the theme-2 pastel or white).
    fn material(&mut self, host: &mut Host, texture: &'static str, tint: u32) -> Handle<StandardMaterial> {
        if let Some(m) = self.tex.get(&(texture, tint)) {
            return m.clone();
        }
        let m = textured(host, texture, hex(tint));
        self.tex.insert((texture, tint), m.clone());
        m
    }

    fn stream(&mut self, host: &mut Host, x: f64) {
        // Build every segment that starts within reach ahead; retire those far behind.
        let mut i = 0;
        loop {
            let seg = self.track.as_mut().unwrap().segment(i).clone();
            if seg.start > x + AHEAD {
                break;
            }
            if seg.end < x - BEHIND {
                self.retire(host, i);
            } else if !self.built.contains_key(&i) {
                self.build(host, seg);
            }
            i += 1;
        }
    }

    fn build(&mut self, host: &mut Host, segment: Segment) {
        let mut built = Built { segment: segment.clone(), entities: vec![], surfaces: HashSet::new(), coins: vec![], hazards: vec![] };
        let mut lowest = f64::INFINITY;
        for (n, block) in segment.blocks.iter().enumerate() {
            let center = [(block.x0 + block.x1) / 2.0, block.top - block.height / 2.0, block.z];
            let size = [block.x1 - block.x0, block.height, block.width];
            let (top_tex, side_tex) = look(block.theme, block.kind);
            let tint = if block.theme == 2 && block.kind != BlockKind::Bridge { PASTELS[(segment.index + n) % PASTELS.len()] } else { 0xffffff };
            let side = self.material(host, side_tex, if block.kind == BlockKind::Floor && block.theme == 2 { 0xffffff } else { tint });
            let tile = if block.kind == BlockKind::Bridge { 140.0 } else { 240.0 };
            let mesh = host.meshes.add(tiled_box(size, tile));
            let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(side), Transform::from_translation(to_bevy(center))));
            built.entities.push(e);
            // The top face gets its own texture (grass on top, dirt on the sides).
            let top_mat = self.material(host, top_tex, tint);
            let top_mesh = host.meshes.add(tiled_top(size[0], size[2], tile));
            let skin = host.spawn((Mesh3d(top_mesh), MeshMaterial3d(top_mat), Transform::from_translation(to_bevy([center[0], block.top + 0.6, center[2]]))));
            built.entities.push(skin);
            let mut faces = vec![];
            add_box_collision(host.world_mut(), center, size, 0.0, &mut faces, &SurfaceOptions::default());
            for face in &faces {
                built.surfaces.insert(surface_id(face));
                if matches!(block.kind, BlockKind::Pillar | BlockKind::Hurdle) {
                    self.pillar_faces.insert(surface_id(face), (block.kind, block.z));
                }
            }
            lowest = lowest.min(block.top);
        }
        let (coin_mesh, coin_mat) = match (&self.coin_mesh, &self.coin_mat) {
            (Some(a), Some(b)) => (a.clone(), b.clone()),
            _ => {
                let mesh = host.meshes.add(Cylinder::new(45.0 * crate::render::SCALE, 12.0 * crate::render::SCALE));
                // Lit gold that glows a little: the spin catches the light instead of reading as a flat yellow blob.
                let mat = host.materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.12), emissive: LinearRgba::rgb(0.5, 0.33, 0.02), metallic: 0.25, perceptual_roughness: 0.35, ..default() });
                self.coin_mesh = Some(mesh.clone());
                self.coin_mat = Some(mat.clone());
                (mesh, mat)
            }
        };
        for p in &segment.coins {
            let e = host.spawn((Mesh3d(coin_mesh.clone()), MeshMaterial3d(coin_mat.clone()), Transform::from_translation(to_bevy(*p)).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))));
            built.coins.push(CoinEntry { entity: e, pos: *p, taken: false });
        }
        let (bm, bmat) = match &self.boulder {
            Some(b) => b.clone(),
            None => {
                let b = (host.meshes.add(Sphere::new(150.0 * crate::render::SCALE).mesh().ico(2).unwrap()), host.lit(Color::srgb(0.69, 0.63, 0.56)));
                self.boulder = Some(b.clone());
                b
            }
        };
        for h in &segment.hazards {
            let e = host.spawn((Mesh3d(bm.clone()), MeshMaterial3d(bmat.clone()), Transform::from_translation(to_bevy([h.x, h.top + h.radius, 0.0]))));
            built.hazards.push((h.clone(), e, 0.0));
        }
        self.decorate(host, &mut built);
        // A catch floor far below keeps Mario's floor query valid while he falls.
        let y = lowest - 3500.0;
        let mut catch = vec![];
        add_face(
            host.world_mut(),
            &[[segment.start - 3000.0, y, -6000.0], [segment.end + 3000.0, y, -6000.0], [segment.end + 3000.0, y, 6000.0], [segment.start - 3000.0, y, 6000.0]],
            [0.0, 1.0, 0.0],
            &mut catch,
            &SurfaceOptions::default(),
        );
        built.surfaces.extend(catch.iter().map(surface_id));
        self.built.insert(segment.index, built);
    }

    fn retire(&mut self, host: &mut Host, index: usize) {
        let Some(built) = self.built.remove(&index) else { return };
        host.world_mut().remove_surfaces(&|s| built.surfaces.contains(&(s as *const Surface as usize)));
        for id in &built.surfaces {
            self.pillar_faces.remove(id);
        }
        for e in built.entities.iter().chain(built.coins.iter().filter(|c| !c.taken).map(|c| &c.entity)).chain(built.hazards.iter().map(|h| &h.1)) {
            host.commands.entity(*e).despawn();
        }
    }

    /// Trees on islands beside the track, towers, spinning pastel cubes (`decorate`).
    fn decorate(&mut self, host: &mut Host, built: &mut Built) {
        let segment = built.segment.clone();
        let mut state = segment.index as f64 * 7919.0 + self.seed;
        let mut random = move || {
            state = (state * 16807.0) % 2147483647.0;
            state / 2147483647.0
        };
        let mut x = segment.start + 400.0;
        while x < segment.end {
            let side = if random() < 0.5 { -1.0 } else { 1.0 };
            match segment.theme {
                0 => {
                    let z = side * (1700.0 + random() * 2500.0);
                    let y = segment.exit_top - 300.0 - random() * 900.0;
                    let edge = self.material(host, "grassEdge", 0xffffff);
                    let mesh = host.meshes.add(tiled_box([700.0, 200.0, 700.0], 240.0));
                    built.entities.push(host.spawn((Mesh3d(mesh), MeshMaterial3d(edge), Transform::from_translation(to_bevy([x, y - 100.0, z])))));
                    let grass = self.material(host, "grassTop", 0xffffff);
                    let top = host.meshes.add(tiled_top(700.0, 700.0, 240.0));
                    built.entities.push(host.spawn((Mesh3d(top), MeshMaterial3d(grass), Transform::from_translation(to_bevy([x, y + 0.6, z])))));
                    let height = 380.0 + random() * 220.0;
                    if host.asset_root.exists(TREE) {
                        self.tree_ok = true;
                        // The glb tree is roughly 400 units tall at scale 1.
                        let k = (height / 400.0) as f32;
                        built.entities.push(host.spawn((crate::render::scene(host.assets, TREE), Transform::from_translation(to_bevy([x, y, z])).with_scale(Vec3::splat(k)), Visibility::Inherited)));
                    }
                }
                1 => {
                    let height = 1800.0 + random() * 2400.0;
                    let z = side * (2000.0 + random() * 3000.0);
                    let brick = self.material(host, "brick", 0xffffff);
                    let tower = host.meshes.add(Cylinder::new(280.0 * crate::render::SCALE, height as f32 * crate::render::SCALE));
                    let cy = segment.exit_top - 1800.0 + height / 2.0;
                    built.entities.push(host.spawn((Mesh3d(tower), MeshMaterial3d(brick), Transform::from_translation(to_bevy([x, cy, z])))));
                    let roof = host.meshes.add(Cone::new(340.0 * crate::render::SCALE, 460.0 * crate::render::SCALE));
                    let red = host.lit(Color::srgb(0.75, 0.22, 0.17));
                    built.entities.push(host.spawn((Mesh3d(roof), MeshMaterial3d(red), Transform::from_translation(to_bevy([x, cy + height / 2.0 + 230.0, z])))));
                }
                _ => {
                    let size = 300.0 + random() * 600.0;
                    let tint = PASTELS[(random() * PASTELS.len() as f64).floor() as usize % PASTELS.len()];
                    let mat = self.material(host, "checker", tint);
                    let mesh = host.meshes.add(tiled_box([size; 3], 200.0));
                    let y = segment.exit_top + (random() - 0.3) * 2400.0;
                    let z = side * (1500.0 + random() * 3500.0);
                    let rot = Vec3::new(random() as f32 * 3.0, random() as f32 * 3.0, 0.0);
                    let spin = 0.2 + random() as f32 * 0.5;
                    let e = host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy([x, y, z])).with_rotation(Quat::from_euler(EulerRot::XYZ, rot.x, rot.y, 0.0))));
                    built.entities.push(e);
                    self.spinners.push((e, [x, y, z], rot, spin));
                }
            }
            x += 900.0 + random() * 900.0;
        }
    }

    fn load_ghost(&self) -> Vec<Frame> {
        if self.mode != Mode::Daily {
            return vec![];
        }
        let Some(serde_json::Value::Array(rows)) = store::get(&format!("{GHOST_KEY}.{}", self.seed)) else { return vec![] };
        rows.iter()
            .filter_map(|r| {
                let a = r.as_array()?;
                let mut f = [0.0; 10];
                for (i, v) in a.iter().take(10).enumerate() {
                    f[i] = v.as_f64()?;
                }
                Some(f)
            })
            .collect()
    }

    /// Show frame `t` of the recorded ghost (fractional ticks interpolate position).
    fn show_ghost(&mut self, host: &mut Host, dt: f64) {
        let t = self.run_ticks as f64;
        let showing = self.state == RunState::Running && !self.ghost_frames.is_empty() && t < self.ghost_frames.len() as f64 - 1.0;
        let Some(ghost) = self.ghost.as_mut() else { return };
        let Some(entity) = ghost.entity else { return };
        if !showing {
            host.commands.entity(entity).insert(Visibility::Hidden);
            return;
        }
        let (i, k) = (t.floor() as usize, 0.0);
        let (a, b) = (self.ghost_frames[i], self.ghost_frames[(i + 1).min(self.ghost_frames.len() - 1)]);
        let r = &mut ghost.runtime;
        r.pos = [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
        r.face_angle[1] = a[3];
        r.action = a[4] as u32;
        r.action_arg = a[5];
        r.action_state = a[6];
        r.action_timer = a[7];
        r.forward_vel = a[8];
        r.intended_mag = a[8].abs();
        r.vel = [0.0, a[9], 0.0];
        ghost.sync_previous();
        ghost.update_pose();
        host.commands.entity(entity).insert(Visibility::Inherited);
        let eye = host.rig.eye;
        ghost.render(host.commands, 1.0, dt, (DVec3::from(ghost.runtime.pos) - DVec3::new(eye.x, eye.y, eye.z)).length());
    }

    fn dock(&self, host: &mut Host) {
        let mut daily = DockItem::new("Daily", KeyCode::KeyM, 0);
        daily.selected = self.mode == Mode::Daily;
        let mut random = DockItem::new("Random", KeyCode::KeyM, 0);
        random.selected = self.mode == Mode::Random;
        host.set_dock(vec![daily, random, DockItem::new("Run again", KeyCode::Space, 1).essential().primary()]);
    }

    fn start_run(&mut self, host: &mut Host) {
        for index in self.built.keys().copied().collect::<Vec<_>>() {
            self.retire(host, index);
        }
        self.pillar_faces.clear();
        self.spinners.clear();
        self.ghost_frames = self.load_ghost();
        self.recording.clear();
        self.dock(host);
        self.best = store::get_f64(&self.best_key()).unwrap_or(0.0);
        self.run_ticks = 0;
        self.hits = 0;
        self.track = Some(RunnerTrack::new(self.seed, ORIGIN_X, ORIGIN_TOP));
        let world = jumpman_sim::collision::CollisionWorld::new(1024.0, 1e9);
        let face_yaw = 0x4000 as f64; // +X
        host.set_mario_world(Arc::new(world), jumpman_sim::mario_runtime::SpawnPoint { action: None, pos: [ORIGIN_X, ORIGIN_TOP + 300.0, 0.0], face_yaw }, -1e9);
        self.stream(host, ORIGIN_X);
        host.place_mario([ORIGIN_X, ORIGIN_TOP, 0.0], Some(face_yaw));
        self.state = RunState::Ready;
        self.ticks = 0;
        self.coins = 0;
        self.distance = 0.0;
        self.milestone = 0;
        self.passed_best = false;
        self.last_top = ORIGIN_TOP;
        self.theme = -1;
        self.runs += 1;
        self.cam_pos = DVec3::ZERO;
        let text = if self.mode == Mode::Daily { format!("DAILY RUN - {}{}", self.seed, if self.ghost_frames.is_empty() { "" } else { " - BEAT YOUR GHOST" }) } else { "READY...".into() };
        host.toast(&text, 1.4);
        host.status("");
    }

    /// Clipping a pillar nudges Mario around it instead of stopping the run dead.
    fn glance_off_pillars(&mut self, host: &mut Host) {
        let m = &mut host.sim.mario;
        let hit = m.wall.as_ref().and_then(|w| self.pillar_faces.get(&surface_id(w))).copied();
        let Some((kind, z)) = hit else {
            self.blocked = 0;
            return;
        };
        if m.forward_vel > 20.0 {
            self.blocked = 0;
            return;
        }
        if kind == BlockKind::Hurdle {
            // Stuck at a hurdle: scramble over it (slower than jumping it cleanly).
            self.blocked += 1;
            if self.blocked >= 10 && m.action & ACT_FLAG_AIR == 0 {
                self.blocked = 0;
                m.set_action(ACT_JUMP, 0.0);
                m.vel[1] = 46.0;
                m.forward_vel = 18.0;
            }
            return;
        }
        let side = if m.pos[2] >= z { 1.0 } else { -1.0 };
        m.pos[2] += side * 26.0;
        m.forward_vel = m.forward_vel.max(22.0);
    }

    /// Boulders sweep across the track; touching one knocks Mario back.
    fn step_hazards(&mut self, host: &mut Host) {
        let ticks = self.ticks as f64;
        let mut hit = false;
        for built in self.built.values_mut() {
            for (hazard, entity, z_last) in &mut built.hazards {
                let angle = (ticks / hazard.period + hazard.phase) * std::f64::consts::TAU;
                let z = angle.sin() * hazard.sweep;
                let y = hazard.top + hazard.radius;
                *z_last = z;
                let tf = Transform::from_translation(to_bevy([hazard.x, y, z])).with_rotation(Quat::from_rotation_x((z / hazard.radius) as f32));
                host.commands.entity(*entity).insert(tf);
                let m = &mut host.sim.mario;
                let (dx, dy, dz) = (m.pos[0] - hazard.x, m.pos[1] + 80.0 - y, m.pos[2] - z);
                if (dx * dx + dy * dy + dz * dz).sqrt() < hazard.radius + 60.0 && m.take_damage(1.0, Some([hazard.x - 200.0, y, z])) {
                    m.hurt_counter = 0.0;
                    hit = true;
                }
                // The boulder is solid: hurt or not (the invincibility after a hit used to let Mario run through it), he
                // ends up outside it.
                if let Some(d) = boulder_push(host.sim.mario.pos, [hazard.x, y, z], hazard.radius) {
                    host.shift_mario([d[0], 0.0, d[1]]);
                }
            }
        }
        if hit {
            self.hits += 1;
        }
    }

    /// Feedback while running: every 100 m, and the moment the run passes the best one (the ghost's distance).
    fn announce_progress(&mut self, host: &mut Host) {
        let pos = host.sim.mario.pos;
        let hundreds = (self.distance / 100.0).floor() as u32;
        let passing = self.best > 0.0 && !self.passed_best && self.distance > self.best;
        if hundreds > self.milestone {
            self.milestone = hundreds;
            if !passing {
                host.toast(&format!("{} M", hundreds * 100), 1.1);
            }
            host.fx("coin", [pos[0], pos[1] + 160.0, pos[2]]);
        }
        if passing {
            self.passed_best = true;
            host.toast(if self.ghost_frames.is_empty() { "PAST YOUR BEST" } else { "GHOST BEATEN - PAST YOUR BEST" }, 2.0);
            host.fx("star_sound", pos);
        }
    }

    fn game_over(&mut self, host: &mut Host) {
        self.state = RunState::Over;
        let (score, record) = score_run(self.distance, self.best);
        self.over_ticks = 0;
        if record {
            self.best = score;
            store::set(&self.best_key(), serde_json::json!(score));
            // The best daily run becomes the ghost for today's track.
            if self.mode == Mode::Daily {
                store::set(&format!("{GHOST_KEY}.{}", self.seed), serde_json::json!(self.recording));
            }
        }
        let head = if record { "NEW RECORD!" } else { "YOU FELL" };
        let again = host.prompt("SPACE to run again", "tap A or RUN AGAIN", "A to run again");
        host.status(format!("{head}  {score} m   {} coins   best {} m   -   {again}", self.coins, self.best));
        host.toast(head, 2.0);
    }
}

impl Experiment for EndlessRunnerExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Runner
    }
    fn replaces_course(&self) -> bool {
        true
    }

    fn enter(&mut self, host: &mut Host) {
        host.card(
            "03",
            "Endless Run",
            "Mario never stops running. Steer, jump the gaps and hurdles, dodge the pillars. One fall ends the run.",
            &[("A / D", "steer"), ("Space / A", "jump (hold for height)"), ("Ctrl/K + Space", "long jump"), ("Space / A after a fall", "run again"), ("M", "daily / random track"), ("X", "leave")],
        );
        host.set_clear(Color::srgb(SKY[0][0], SKY[0][1], SKY[0][2]));
        host.cam.fog = Some((Color::srgb(FOG[0].0[0], FOG[0].0[1], FOG[0].0[2]), FOG[0].1 as f64, 26000.0));
        if let Some(meta) = init_clips(host) {
            let spawn = host.sim.course.spawn.clone();
            let mut ghost = CrowdMario::new(host.course_world(), spawn, Some(meta.as_slice()));
            ghost.entity = Some(spawn_body(host.commands, host.assets, CrowdLook { cap: 0x4f9bff, overalls: Some(0xe6f1ff) }));
            host.commands.entity(ghost.entity.unwrap()).insert(Visibility::Hidden);
            self.ghost = Some(ghost);
        }
        self.start_run(host);
    }

    fn exit(&mut self, host: &mut Host) {
        for index in self.built.keys().copied().collect::<Vec<_>>() {
            self.retire(host, index);
        }
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.cam.pose = None;
        host.cam.fog = None;
        self.ghost = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if key == KeyCode::KeyM {
            self.mode = if self.mode == Mode::Daily { Mode::Random } else { Mode::Daily };
            self.seed = if self.mode == Mode::Daily { daily_seed() } else { 1.0 + (jumpman_sim::math::random() * 99999.0).floor() };
            self.start_run(host);
            return true;
        }
        if self.state == RunState::Over && matches!(key, KeyCode::Space | KeyCode::Enter | KeyCode::KeyR) {
            if self.mode == Mode::Random {
                self.seed = 1.0 + (jumpman_sim::math::random() * 99999.0).floor();
            }
            self.start_run(host);
            return true;
        }
        false
    }

    fn before_step(&mut self, host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
        // Result screen: the A button (touch / gamepad have no Space key) starts the next run.
        if self.state == RunState::Over {
            self.over_ticks += 1;
            if self.over_ticks > RETRY_LOCK && controller.button_pressed & A_BUTTON != 0 {
                if self.mode == Mode::Random {
                    self.seed = 1.0 + (jumpman_sim::math::random() * 99999.0).floor();
                }
                self.start_run(host);
            }
        }
        if self.track.is_none() || self.state != RunState::Running {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: Some(0xc000) };
        }
        // Always full speed ahead (+X); the player only steers and uses buttons.
        let steer = (controller.stick_x / 64.0).clamp(-1.0, 1.0) * 0.55;
        let x = steer * 64.0;
        let y = (64.0f64 * 64.0 - x * x).max(0.0).sqrt();
        // Camera yaw 0xC000: stick-up runs toward +X, stick-right toward +Z.
        let mut c = controller.clone();
        c.stick_x = x;
        c.stick_y = y;
        c.stick_mag = 64.0;
        StepOverride { controller: Some(c), camera_yaw: Some(0xc000) }
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.track.is_none() {
            return;
        }
        self.ticks += 1;
        if self.state == RunState::Ready && self.ticks >= countdown_for(self.runs) {
            self.state = RunState::Running;
            host.toast("GO!", 0.9);
        }
        if self.state == RunState::Over {
            return;
        }
        if self.state == RunState::Running {
            self.run_ticks += 1;
            self.recording.push(record_frame(&host.sim.mario));
            self.glance_off_pillars(host);
            self.step_hazards(host);
        }
        let m = &host.sim.mario;
        if m.action & ACT_FLAG_AIR == 0 && m.floor.is_some() && m.floor_height >= self.last_top - 2500.0 {
            self.last_top = m.floor_height;
        }
        if m.pos[1] < self.last_top - FALL_OUT {
            self.game_over(host);
            return;
        }
        let pos = host.sim.mario.pos;
        self.distance = self.distance.max((pos[0] - ORIGIN_X) / 100.0);
        self.announce_progress(host);
        self.stream(host, pos[0]);
        let mut taken = 0;
        for built in self.built.values_mut() {
            for coin in &mut built.coins {
                if coin.taken || (coin.pos[0] - pos[0]).abs() > 200.0 {
                    continue;
                }
                let d = ((coin.pos[0] - pos[0]).powi(2) + (coin.pos[1] - (pos[1] + 70.0)).powi(2) + (coin.pos[2] - pos[2]).powi(2)).sqrt();
                if d < 130.0 {
                    coin.taken = true;
                    host.commands.entity(coin.entity).despawn();
                    host.fx("coin", coin.pos);
                    taken += 1;
                }
            }
        }
        if taken > 0 {
            self.coins += taken;
            host.add_coins(taken);
        }
        let theme = self.built.values().find(|b| pos[0] >= b.segment.start && pos[0] < b.segment.end).map_or(0, |b| b.segment.theme) as i32;
        if theme != self.theme {
            if self.theme >= 0 {
                host.toast(&THEMES[theme as usize].0.to_uppercase(), 2.2);
            }
            self.theme = theme;
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        if self.track.is_none() {
            return;
        }
        self.t += dt;
        let render = DVec3::from(host.render_pos());
        // A low chase camera that leans into the run and lags a little on height.
        let desired = DVec3::new(render.x - 980.0, render.y + 430.0, render.z * 0.55);
        if self.cam_pos == DVec3::ZERO || (self.state != RunState::Over && self.cam_pos.distance(desired) > 3000.0) {
            self.cam_pos = desired;
            self.cam_look = DVec3::new(render.x + 700.0, render.y + 80.0, render.z * 0.8);
        }
        let k = 1.0 - (-dt * 6.0).exp();
        if self.state == RunState::Over {
            // The run is over: stop chasing the falling Mario and drift back up over the track he fell from
            // (following him down ended with the camera under the platforms, staring at their undersides).
            let rise = 1.0 - (-dt * 2.5).exp();
            self.cam_pos.y += (self.last_top + 520.0 - self.cam_pos.y) * rise;
            let look = DVec3::new(self.cam_pos.x + 1680.0, self.last_top + 80.0, self.cam_look.z);
            self.cam_look = self.cam_look.lerp(look, rise);
        } else {
            self.cam_pos.x = desired.x;
            self.cam_pos.y += (desired.y - self.cam_pos.y) * k * 0.6;
            self.cam_pos.z += (desired.z - self.cam_pos.z) * k;
            self.cam_look = self.cam_look.lerp(DVec3::new(render.x + 700.0, render.y + 80.0, render.z * 0.8), k);
        }
        host.cam.pose = Some(CamPose { eye: self.cam_pos, look: self.cam_look, up: DVec3::Y, fov: 62.0, near: 5.0, far: 80000.0 });
        let t = self.theme.max(0) as usize;
        let target = SKY[t];
        let f = (dt * 0.8).min(1.0) as f32;
        for i in 0..3 {
            self.sky[i] += (target[i] - self.sky[i]) * f;
        }
        host.set_clear(Color::srgb(self.sky[0], self.sky[1], self.sky[2]));
        let (a, b, c) = SKY_STOPS[t];
        let mut palette = *host.palette;
        palette.lerp_to(&crate::fx::sky::SkyPalette::from_hex(a, b, c).with_clouds(0.6), f);
        host.set_sky(palette);
        if let Some((c, _, _)) = host.cam.fog.as_mut() {
            let target = FOG[t].0;
            let cur = c.to_srgba();
            *c = Color::srgb(cur.red + (target[0] - cur.red) * f, cur.green + (target[1] - cur.green) * f, cur.blue + (target[2] - cur.blue) * f);
        }
        for (e, pos, rot, rate) in &self.spinners {
            if (pos[0] - render.x).abs() < AHEAD + 2000.0 {
                let q = Quat::from_euler(EulerRot::XYZ, rot.x, rot.y + self.t as f32 * rate, 0.0);
                host.commands.entity(*e).insert(Transform::from_translation(to_bevy(*pos)).with_rotation(q));
            }
        }
        self.show_ghost(host, dt);
        let spin = Quat::from_rotation_y(self.t as f32 * 4.0) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let ahead = render.x - 1500.0..render.x + AHEAD;
        for built in self.built.values() {
            for coin in built.coins.iter().filter(|c| !c.taken && ahead.contains(&c.pos[0])) {
                host.commands.entity(coin.entity).insert(Transform::from_translation(to_bevy(coin.pos)).with_rotation(spin));
            }
        }
        if self.state != RunState::Over {
            host.status(format!(
                "{} m   best {} m   {} coins   {}",
                self.distance.floor(),
                self.best.max(self.distance).floor(),
                self.coins,
                THEMES[t].0
            ));
        }
    }

    fn debug(&self) -> String {
        let mut kinds: Vec<_> = self.built.iter().map(|(i, b)| (*i, b.segment.kind)).collect();
        kinds.sort();
        format!("seed={} state={:?} distance={:.0} coins={} runs={} hits={} built={:?} theme={}", self.seed, self.state, self.distance, self.coins, self.runs, self.hits, kinds, self.theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_seed_is_the_calendar_date() {
        assert_eq!(daily_seed_from_days(0), 19700101.0);
        assert_eq!(daily_seed_from_days(19723), 20240101.0);
        assert_eq!(daily_seed_from_days(20058), 20241201.0);
        // Leap day.
        assert_eq!(daily_seed_from_days(19782), 20240229.0);
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use crate::experiments::god::test_bot::Bot;
    use jumpman_sim::collision::CollisionWorld;
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};

    #[test]
    fn scoring_counts_whole_metres_and_records() {
        assert_eq!(score_run(412.9, 400.0), (412.0, true));
        assert_eq!(score_run(412.9, 412.0), (412.0, false));
        assert_eq!(score_run(0.0, 0.0), (0.0, false));
    }

    /// Death to running again in under 2 s (`MINIGAMES_RESEARCH.md` section 7, checklist 1): the fall is called 800 units
    /// under the track (about 0.55 s of SM64 free fall from a running jump's top), A works after 8 ticks and a retry's
    /// countdown is 12 ticks, so fall + lock + countdown stays under 1.4 s; and the first obstacle comes within 3 s of GO.
    #[test]
    fn a_retry_is_quick_and_the_first_obstacle_comes_soon() {
        // Free fall at SM64's gravity (4 units/tick², capped at 75 units/tick) from a jump's top down FALL_OUT units.
        let (mut y, mut v, mut ticks) = (0.0f64, 0.0f64, 0u32);
        while y > -FALL_OUT {
            v = (v - 4.0).max(-75.0);
            y += v;
            ticks += 1;
        }
        let total = ticks + RETRY_LOCK + countdown_for(2);
        assert!(total < 2 * 30, "death to running again takes {total} ticks");
        assert_eq!(countdown_for(1), COUNTDOWN, "the first run of a visit keeps its READY beat");
        // First obstacle: the start floor runs 2600 units past the origin, then segment 1 may already be an obstacle.
        let mut firsts = vec![];
        for seed in 1..=60 {
            let mut track = RunnerTrack::new(seed as f64, ORIGIN_X, ORIGIN_TOP);
            let first = (1..10).map(|i| track.segment(i).clone()).find(|s| s.kind != "run").map_or(1e9, |s| s.start - ORIGIN_X);
            firsts.push(first);
        }
        firsts.sort_by(f64::total_cmp);
        // Mario's running speed on the track is about 32 units a tick (960 a second).
        let median_secs = firsts[firsts.len() / 2] / 960.0;
        eprintln!("first obstacle after GO: median {median_secs:.1}s, worst {:.1}s", firsts[firsts.len() - 1] / 960.0);
        assert!(median_secs <= 3.0, "the first obstacle comes {median_secs:.1}s after GO");
    }

    /// A real Mario with a scripted driver (steer to the safe lane, jump at gap and hurdle edges)
    /// runs the first segments of several generated tracks without falling: the run a player is
    /// asked to make is physically possible, and its distance is what the score counts.
    #[test]
    fn a_scripted_player_runs_generated_tracks() {
        for seed in [1.0, 7.0, 20240101.0] {
            let mut track = RunnerTrack::new(seed, ORIGIN_X, ORIGIN_TOP);
            let segments: Vec<Segment> = (0..14).map(|i| track.segment(i).clone()).collect();
            let mut world = CollisionWorld::new(1024.0, 1e9);
            let mut faces = vec![];
            for seg in &segments {
                for b in &seg.blocks {
                    let center = [(b.x0 + b.x1) / 2.0, b.top - b.height / 2.0, b.z];
                    add_box_collision(&mut world, center, [b.x1 - b.x0, b.height, b.width], 0.0, &mut faces, &SurfaceOptions::default());
                }
                // The catch floor `build` lays far below each segment, so a fall is a fall and not "out of bounds".
                let y = seg.blocks.iter().map(|b| b.top).fold(f64::INFINITY, f64::min) - 3500.0;
                add_face(&mut world, &[[seg.start - 3000.0, y, -6000.0], [seg.end + 3000.0, y, -6000.0], [seg.end + 3000.0, y, 6000.0], [seg.start - 3000.0, y, 6000.0]], [0.0, 1.0, 0.0], &mut faces, &SurfaceOptions::default());
            }
            world.finalize();
            let world = Arc::new(world);
            let mut m = MarioRuntime::new(world.clone(), jumpman_sim::mario_runtime::SpawnPoint { action: None, pos: [ORIGIN_X, ORIGIN_TOP, 0.0], face_yaw: 0x4000 as f64 }, -1e9);
            let mut bot = Bot::default();
            let end = segments.last().unwrap().start;
            let mut last_top = ORIGIN_TOP;
            let mut result = Err("ran out of time");
            for tick in 0..30 * 400 {
                let (x, y, z) = (m.pos[0], m.pos[1], m.pos[2]);
                if m.action & ACT_FLAG_AIR == 0 && m.floor.is_some() {
                    last_top = m.floor_height;
                }
                if y < last_top - FALL_OUT {
                    result = Err("fell");
                    break;
                }
                if x > end {
                    result = Ok(tick);
                    break;
                }
                // Lane: centre of the track unless a pillar/hurdle block blocks the way ahead.
                let mut lane = 0.0;
                // Split bridges leave the middle empty: take the first one.
                if let Some(b) = segments.iter().flat_map(|s| s.blocks.iter()).find(|b| matches!(b.kind, BlockKind::Bridge) && b.x0 - 1500.0 < x && x < b.x1 && b.z < 0.0) {
                    lane = b.z;
                }
                for seg in &segments {
                    for b in seg.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Pillar) && b.x0 - 900.0 < x && x < b.x1 + 100.0) {
                        lane = if b.z >= 0.0 { b.z - b.width / 2.0 - 170.0 } else { b.z + b.width / 2.0 + 170.0 };
                    }
                }
                let heading = Bot::heading_to(&m, [x + 800.0, 0.0, lane]);
                // Jump a little earlier at a gap than at a step (a late take-off off an island edge lands short of the next one).
                let ahead = (m.forward_vel * 2.0).max(60.0);
                let probe = m.world.find_floor(x + jumpman_sim::math::sins(heading) * ahead, y + 700.0, z + jumpman_sim::math::coss(heading) * ahead);
                let edge = probe.surface.is_none() || probe.height < y - 300.0;
                let jump = m.action & ACT_FLAG_AIR == 0 && m.forward_vel > 1.0 && (edge || Bot::edge_ahead(&m, heading));
                // A short island after the gap: a short hop (a full running jump, 841 units, sails over it into the next gap).
                if jump {
                    let next = segments.iter().flat_map(|s| s.blocks.iter()).filter(|b| matches!(b.kind, BlockKind::Floor | BlockKind::Bridge) && b.x0 > x + 20.0).min_by(|a, b| a.x0.total_cmp(&b.x0));
                    bot.max_hold = if next.is_some_and(|b| b.x1 - x < 950.0) { 3 } else { 100 };
                }
                bot.step(&mut m, heading, jump);
            }
            let near: Vec<String> = segments.iter().flat_map(|s| s.blocks.iter()).filter(|b| (b.x0 - m.pos[0]).abs() < 1500.0 || (b.x1 - m.pos[0]).abs() < 1500.0).map(|b| format!("{:?} x {:.0}..{:.0} z {:.0}±{:.0} top {:.0}", b.kind, b.x0, b.x1, b.z, b.width / 2.0, b.top)).collect();
            let tick = result.unwrap_or_else(|e| panic!("seed {seed}: {e}; at {:?} act {:x} v {}\n{}", m.pos, m.action, m.forward_vel, near.join("\n")));
            eprintln!("seed {seed}: {:.0} m in {:.1}s", (end - ORIGIN_X) / 100.0, tick as f64 / 30.0);
        }
    }

    /// The boulders are solid (`boulder_push`): Mario running at a boulder on real SM64 physics, and a boulder sweeping
    /// across the track into a Mario standing on it, never overlap him (also while the hit's invincibility runs, which is
    /// when he used to pass through), and the touch still knocks him back.
    #[test]
    fn boulders_are_solid_both_ways() {
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let mut faces = vec![];
        add_box_collision(&mut world, [ORIGIN_X + 2000.0, ORIGIN_TOP - 130.0, 0.0], [8000.0, 260.0, 3000.0], 0.0, &mut faces, &SurfaceOptions::default());
        world.finalize();
        let world = Arc::new(world);
        let (r, top) = (150.0, ORIGIN_TOP);
        let overlap = |m: &MarioRuntime, c: [f64; 3]| boulder_push(m.pos, c, r).map_or(0.0, |d| d[0].hypot(d[1]));
        // Mario runs into a boulder parked on the track.
        let mut m = MarioRuntime::new(world.clone(), SpawnPoint { action: None, pos: [ORIGIN_X, top, 0.0], face_yaw: 0x4000 as f64 }, -1e9);
        let mut bot = Bot::default();
        let c = [ORIGIN_X + 900.0, top + r, 0.0];
        let mut hits = 0;
        for t in 0..240 {
            let heading = Bot::heading_to(&m, c);
            bot.step(&mut m, heading, false);
            let (dx, dy, dz) = (m.pos[0] - c[0], m.pos[1] + 80.0 - c[1], m.pos[2] - c[2]);
            if (dx * dx + dy * dy + dz * dz).sqrt() < r + 60.0 && m.take_damage(1.0, Some([c[0] - 200.0, c[1], c[2]])) {
                m.hurt_counter = 0.0;
                hits += 1;
            }
            if let Some(d) = boulder_push(m.pos, c, r) {
                m.pos[0] += d[0];
                m.pos[2] += d[1];
            }
            assert!(overlap(&m, c) < 0.5, "tick {t}: Mario inside the parked boulder");
        }
        assert!(hits >= 1, "the touch still knocks Mario back");
        assert!(m.pos[0] < c[0], "Mario never got past the boulder by running through it");
        // A boulder sweeps across the track through Mario standing still.
        let mut m = MarioRuntime::new(world.clone(), SpawnPoint { action: None, pos: [ORIGIN_X + 300.0, top, 0.0], face_yaw: 0x4000 as f64 }, -1e9);
        let (sweep, period) = (390.0, 90.0);
        for t in 0..360 {
            m.step(&ControllerSnapshot::default(), 0.0, false, false);
            let z = (t as f64 / period * std::f64::consts::TAU).sin() * sweep;
            let c = [ORIGIN_X + 300.0, top + r, z];
            if let Some(d) = boulder_push(m.pos, c, r) {
                m.pos[0] += d[0];
                m.pos[2] += d[1];
            }
            assert!(overlap(&m, c) < 0.5, "tick {t}: the sweeping boulder overlaps Mario");
        }
    }
}
