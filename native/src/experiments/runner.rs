//! Endless Run (`runner.ts`): Mario auto-runs down a generated track using his real SM64
//! physics. You steer, jump, long-jump and dive; one fall ends the run.
//!
//! Not ported: the ghost replay (needs a second Mario view; best distance is persisted),
//! SM64 textures (flat theme colours instead), sky dome / fog, decor (trees, towers, cubes)
//! and the coin glb (spinning discs).
use super::geometry::{add_box_collision, add_face};
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
const COUNTDOWN: u32 = 45;

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
        }
    }

    fn best_key(&self) -> String {
        if self.mode == Mode::Daily {
            format!("jumpman.runner.best.daily.{}", self.seed)
        } else {
            "jumpman.runner.best".into()
        }
    }

    fn material(&mut self, host: &mut Host, theme: usize, kind: BlockKind, top: bool) -> Handle<StandardMaterial> {
        let key = (theme, (kind as u8) * 2 + top as u8);
        if let Some(m) = self.materials.get(&key) {
            return m.clone();
        }
        let c = match (theme, kind, top) {
            (0, BlockKind::Floor, true) => [0.36, 0.66, 0.25],
            (0, BlockKind::Floor, false) => [0.45, 0.32, 0.2],
            (0, BlockKind::Bridge | BlockKind::Hurdle, _) => [0.62, 0.43, 0.26],
            (0, BlockKind::Pillar, _) => [0.6, 0.6, 0.62],
            (1, BlockKind::Floor, true) => [0.62, 0.55, 0.45],
            (1, BlockKind::Floor, false) => [0.55, 0.32, 0.26],
            (1, BlockKind::Bridge, _) => [0.5, 0.36, 0.22],
            (1, BlockKind::Hurdle, _) => [0.62, 0.3, 0.24],
            (1, BlockKind::Pillar, _) => [0.82, 0.7, 0.45],
            (_, BlockKind::Floor, true) => [0.85, 0.7, 0.95],
            (_, BlockKind::Floor, false) => [0.95, 0.95, 1.0],
            (_, BlockKind::Bridge, _) => [1.0, 0.55, 0.75],
            (_, BlockKind::Hurdle, _) => [0.7, 0.9, 1.0],
            (_, BlockKind::Pillar, _) => [1.0, 0.85, 0.3],
        };
        let handle = host.lit(Color::srgb(c[0], c[1], c[2]));
        self.materials.insert(key, handle.clone());
        handle
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
        for block in &segment.blocks {
            let center = [(block.x0 + block.x1) / 2.0, block.top - block.height / 2.0, block.z];
            let size = [block.x1 - block.x0, block.height, block.width];
            let side = self.material(host, block.theme, block.kind, false);
            let e = host.spawn_box(center, size, 0.0, side);
            built.entities.push(e);
            // A thin top skin in the top colour so floors read as grass/stone.
            let top_mat = self.material(host, block.theme, block.kind, true);
            let skin = host.spawn_box([center[0], block.top - 4.0, center[2]], [size[0] + 2.0, 10.0, size[2] + 2.0], 0.0, top_mat);
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
                let mat = host.unlit(Color::srgb(1.0, 0.85, 0.1));
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

    fn start_run(&mut self, host: &mut Host) {
        for index in self.built.keys().copied().collect::<Vec<_>>() {
            self.retire(host, index);
        }
        self.pillar_faces.clear();
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
        self.last_top = ORIGIN_TOP;
        self.theme = -1;
        self.runs += 1;
        self.cam_pos = DVec3::ZERO;
        let text = if self.mode == Mode::Daily { format!("DAILY RUN - {}", self.seed) } else { "READY...".into() };
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
            }
        }
        if hit {
            self.hits += 1;
        }
    }

    fn game_over(&mut self, host: &mut Host) {
        self.state = RunState::Over;
        let score = self.distance.floor();
        let record = score > self.best;
        if record {
            self.best = score;
            store::set(&self.best_key(), serde_json::json!(score));
        }
        let head = if record { "NEW RECORD!" } else { "YOU FELL" };
        host.status(format!("{head}  {score} m   {} coins   best {} m   -   SPACE to run again", self.coins, self.best));
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
            &[("A / D", "steer"), ("Space", "jump (hold for height)"), ("Ctrl/K + Space", "long jump"), ("Space after a fall", "run again"), ("M", "daily / random track"), ("X", "leave")],
        );
        host.set_clear(Color::srgb(SKY[0][0], SKY[0][1], SKY[0][2]));
        self.start_run(host);
    }

    fn exit(&mut self, host: &mut Host) {
        for index in self.built.keys().copied().collect::<Vec<_>>() {
            self.retire(host, index);
        }
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.cam.pose = None;
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

    fn before_step(&mut self, _host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
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
        if self.state == RunState::Ready && self.ticks >= COUNTDOWN {
            self.state = RunState::Running;
            host.toast("GO!", 0.9);
        }
        if self.state == RunState::Over {
            return;
        }
        if self.state == RunState::Running {
            self.run_ticks += 1;
            self.glance_off_pillars(host);
            self.step_hazards(host);
        }
        let m = &host.sim.mario;
        if m.action & ACT_FLAG_AIR == 0 && m.floor.is_some() && m.floor_height >= self.last_top - 2500.0 {
            self.last_top = m.floor_height;
        }
        if m.pos[1] < self.last_top - 1300.0 {
            self.game_over(host);
            return;
        }
        let pos = host.sim.mario.pos;
        self.distance = self.distance.max((pos[0] - ORIGIN_X) / 100.0);
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
        if self.cam_pos == DVec3::ZERO || self.cam_pos.distance(desired) > 3000.0 {
            self.cam_pos = desired;
            self.cam_look = DVec3::new(render.x + 700.0, render.y + 80.0, render.z * 0.8);
        }
        let k = 1.0 - (-dt * 6.0).exp();
        self.cam_pos.x = desired.x;
        self.cam_pos.y += (desired.y - self.cam_pos.y) * if self.state == RunState::Over { 0.0 } else { k * 0.6 };
        self.cam_pos.z += (desired.z - self.cam_pos.z) * k;
        let look_y = if self.state == RunState::Over { self.last_top } else { render.y } + 80.0;
        self.cam_look = self.cam_look.lerp(DVec3::new(render.x + 700.0, look_y, render.z * 0.8), k);
        host.cam.pose = Some(CamPose { eye: self.cam_pos, look: self.cam_look, up: DVec3::Y, fov: 62.0, near: 5.0, far: 80000.0 });
        let t = self.theme.max(0) as usize;
        let target = SKY[t];
        let f = (dt * 0.8).min(1.0) as f32;
        for i in 0..3 {
            self.sky[i] += (target[i] - self.sky[i]) * f;
        }
        host.set_clear(Color::srgb(self.sky[0], self.sky[1], self.sky[2]));
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
