//! Star Swarm: an original arena-survivor ("horde survivor") mode on Mario's real physics.
//!
//! You run, jump, dive and ground-pound through the sim as usual; your stars fire on their own,
//! waves of SM64 critters pour in from the edge of a walled plaza, kills drop gems, gems level you
//! up, and each level offers three upgrades. Survive until the dawn (8:00) to win; the run also
//! ends when your health does.
//!
//! * The rules live in `swarm_core.rs` (deterministic, seeded, unit-tested); this file glues them
//!   to the host: it builds the plaza, feeds the core Mario's position every 30 Hz tick, applies
//!   the movement upgrades to the sim (extra run speed, higher jumps, ground-pound quake) and
//!   publishes render lists and HUD numbers through [`SwarmBridge`].
//! * `swarm_view.rs` draws everything and owns the HUD, the level-up cards and the run summary.
//!
//! `JUMPMAN_SWARM_SEED=<n>` fixes the run seed (otherwise it comes from the clock). The best
//! survival time is kept in `swarm.json` next to the save files. Test hooks for screenshots:
//! `JUMPMAN_SWARM_START=<secs>` begins with the waves of that time, `JUMPMAN_SWARM_XP=<n>` grants
//! XP up front, `JUMPMAN_SWARM_GOD=1` keeps health full.
use super::geometry::add_box_collision;
use super::swarm_core::*;
use super::swarm_view::{CardView, RItem, Summary};
use super::textures::{textured, tiled_box, tiled_top};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use bevy::math::DVec3;
use crate::render::{scene, to_bevy};
use bevy::prelude::*;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::state::*;
use std::sync::Arc;

/// Far from every course object, so nothing in the hidden course reaches Mario.
const ORIGIN: [f64; 3] = [150000.0, 1000.0, 150000.0];
/// Pause between a card appearing and it accepting input (stray thumbs).
const CARD_LOCKOUT: f64 = 0.5;
const SUMMARY_LOCKOUT: f64 = 0.9;
const TREE: &str = "actors/tree/bubbly_tree_geo.glb";
/// Gems and shots beyond this distance from the player are not drawn.
const DRAW_RANGE: f32 = 4200.0;

/// Per-kind display: model scale and hover height.
fn look(kind: EnemyKind) -> (f32, f32) {
    match kind {
        EnemyKind::Skitter => (2.0, 0.0),
        EnemyKind::Brute => (0.9, 0.0),
        EnemyKind::Dasher => (3.0, 0.0),
        EnemyKind::Wisp => (0.9, 50.0),
        EnemyKind::King => (2.4, 0.0),
    }
}

fn best_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("swarm.json")
}

pub fn load_best() -> f32 {
    std::fs::read_to_string(best_path()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).and_then(|v| v["best_secs"].as_f64()).unwrap_or(0.0) as f32
}

fn save_best(secs: f32) {
    let path = best_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "best_secs": secs }).to_string());
}

fn run_seed() -> u64 {
    std::env::var("JUMPMAN_SWARM_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64))
}

pub struct StarSwarmExperiment {
    core: Swarm,
    seed: u64,
    runs: u32,
    /// Seconds since entering (render clock, keeps running while paused).
    t: f64,
    last_action: u32,
    was_air: bool,
    best: f32,
    new_best: bool,
    card_gen_shown: u32,
    cards_at: f64,
    over_at: f64,
    sfx_tick: u32,
    yaws: Vec<f32>,
    first_run: bool,
}

impl StarSwarmExperiment {
    pub fn new() -> Self {
        let seed = run_seed();
        Self { core: Swarm::new(seed), seed, runs: 0, t: 0.0, last_action: 0, was_air: false, best: load_best(), new_best: false, card_gen_shown: 0, cards_at: 0.0, over_at: 0.0, sfx_tick: 0, yaws: vec![0.0; ENEMY_SLOTS], first_run: true }
    }

    fn build_arena(&mut self, host: &mut Host) -> CollisionWorld {
        let mut world = CollisionWorld::new(1024.0, 1e9);
        let o = SurfaceOptions::default();
        let mut faces = vec![];
        let half = ARENA_HALF as f64;
        let floor_top = ORIGIN[1];
        // Floor slab and four walls (collision + matching display boxes).
        add_box_collision(&mut world, [ORIGIN[0], floor_top - 200.0, ORIGIN[2]], [half * 2.0 + 1200.0, 400.0, half * 2.0 + 1200.0], 0.0, &mut faces, &o);
        let wall_h = 520.0;
        let wall_t = 300.0;
        let walls = [
            ([0.0, wall_h / 2.0, half + wall_t / 2.0], [half * 2.0 + wall_t * 2.0, wall_h, wall_t]),
            ([0.0, wall_h / 2.0, -half - wall_t / 2.0], [half * 2.0 + wall_t * 2.0, wall_h, wall_t]),
            ([half + wall_t / 2.0, wall_h / 2.0, 0.0], [wall_t, wall_h, half * 2.0]),
            ([-half - wall_t / 2.0, wall_h / 2.0, 0.0], [wall_t, wall_h, half * 2.0]),
        ];
        let stone = textured(host, "stone", Color::srgb(0.85, 0.78, 0.95));
        for (c, size) in walls {
            let center = [ORIGIN[0] + c[0], floor_top + c[1], ORIGIN[2] + c[2]];
            add_box_collision(&mut world, center, size, 0.0, &mut faces, &o);
            let mesh = host.meshes.add(tiled_box(size, 260.0));
            host.spawn((Mesh3d(mesh), MeshMaterial3d(stone.clone()), Transform::from_translation(to_bevy(center))));
        }
        // Plaza floor: flagstones inside the walls, grass beyond them.
        let tiles = textured(host, "flagstone", Color::srgb(0.8, 0.74, 1.0));
        let mesh = host.meshes.add(tiled_top(half * 2.0, half * 2.0, 360.0));
        host.spawn((Mesh3d(mesh), MeshMaterial3d(tiles), Transform::from_translation(to_bevy([ORIGIN[0], floor_top + 0.6, ORIGIN[2]]))));
        let grass = textured(host, "grassTop", Color::srgb(0.42, 0.55, 0.62));
        let outer = host.meshes.add(tiled_top(60000.0, 60000.0, 500.0));
        host.spawn((Mesh3d(outer), MeshMaterial3d(grass), Transform::from_translation(to_bevy([ORIGIN[0], floor_top - 4.0, ORIGIN[2]]))));
        // Trees around the outside for a horizon.
        if host.asset_root.exists(TREE) {
            for i in 0..28 {
                let a = i as f64 / 28.0 * std::f64::consts::TAU;
                let r = half * 1.45 + (i % 3) as f64 * 700.0;
                let k = 3.0 + (i % 4) as f32 * 0.6;
                host.spawn((scene(host.assets, TREE), Transform::from_translation(to_bevy([ORIGIN[0] + a.cos() * r, floor_top - 4.0, ORIGIN[2] + a.sin() * r])).with_scale(Vec3::splat(k)), Visibility::Inherited));
            }
        }
        world.finalize();
        world
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        self.core = Swarm::new(seed);
        if let Some(secs) = std::env::var("JUMPMAN_SWARM_START").ok().and_then(|v| v.parse::<f32>().ok()) {
            self.core.tick = (secs * TICKS_PER_SEC as f32) as u32;
            self.core.bosses = (secs / BOSS_PERIOD_SECS) as u32;
        }
        // Test hook: `JUMPMAN_SWARM_UPGRADES=12:3,13:2` preloads upgrade levels by index.
        if let Ok(list) = std::env::var("JUMPMAN_SWARM_UPGRADES") {
            for (i, n) in list.split(',').filter_map(|p| p.split_once(':')).filter_map(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<u8>().ok()?))) {
                if let Some(u) = ALL_UPGRADES.get(i) {
                    for _ in 0..n {
                        self.core.up.take(*u);
                    }
                }
            }
            self.core.stats = derive_stats(&self.core.up);
            self.core.player.max_hp = self.core.stats.max_hp;
            self.core.player.hp = self.core.stats.max_hp;
        }
        if let Some(xp) = std::env::var("JUMPMAN_SWARM_XP").ok().and_then(|v| v.parse::<u32>().ok()) {
            self.core.add_xp(xp);
        }
        self.new_best = false;
        self.last_action = 0;
        self.was_air = false;
        self.cards_at = 0.0;
        self.card_gen_shown = 0;
        self.yaws.iter_mut().for_each(|y| *y = 0.0);
        if std::env::var_os("JUMPMAN_SWARM_SHOWCASE").is_some() {
            // Test hook: one of each foe in a row ahead of the player, for scale checks.
            for (i, k) in [EnemyKind::Skitter, EnemyKind::Brute, EnemyKind::Dasher, EnemyKind::Wisp, EnemyKind::King].into_iter().enumerate() {
                self.core.spawn_enemy(k, [600.0 - i as f32 * 300.0, 900.0], 0.0);
            }
            self.core.freeze = true;
        }
        let face = 0.0;
        host.place_mario([ORIGIN[0], ORIGIN[1], ORIGIN[2]], Some(face));
        host.set_player_visible(true);
        host.bridge.reset();
        host.bridge.active = true;
        host.bridge.hud.hint = true;
        host.bridge.hud.max_hp = self.core.player.max_hp;
        host.toast(if self.first_run { "SURVIVE THE SWARM!" } else { "AGAIN!" }, 1.6);
        self.first_run = false;
    }

    fn finish(&mut self, host: &mut Host) {
        let secs = self.core.secs();
        // Test hooks (a skipped clock, free XP, god mode) never touch the record.
        let cheated = ["JUMPMAN_SWARM_START", "JUMPMAN_SWARM_XP", "JUMPMAN_SWARM_GOD", "JUMPMAN_SWARM_UPGRADES"].iter().any(|k| std::env::var_os(k).is_some());
        self.new_best = !cheated && secs > self.best + 0.05;
        if self.new_best {
            self.best = secs;
            save_best(secs);
        }
        self.over_at = self.t;
        host.bridge.summary = Some(Summary { secs, level: self.core.player.level, kills: self.core.kills, best: self.best, new_best: self.new_best, won: self.core.won });
        host.bridge.summary_ready = false;
        host.set_player_visible(true);
    }

    /// World position (SM64 units, Bevy-mapped) of an arena-plane point at `height`.
    fn at(p: [f32; 2], height: f32) -> Vec3 {
        to_bevy([ORIGIN[0] + p[0] as f64, ORIGIN[1] + height as f64, ORIGIN[2] + p[1] as f64])
    }

    /// A chase camera that ignores the walls (it sits above them) and the swarm; the follow rig
    /// still supplies the yaw/pitch/zoom the player drags.
    fn camera(&mut self, host: &mut Host) {
        let focus = DVec3::from(host.render_pos()) + DVec3::new(0.0, 92.0, 0.0);
        let (yaw, pitch, dist) = (host.rig.yaw, 0.85f64, 2000.0f64);
        let eye = focus + DVec3::new(yaw.sin() * pitch.cos() * dist, pitch.sin() * dist, yaw.cos() * pitch.cos() * dist);
        host.cam.pose = Some(CamPose { eye, look: focus, up: DVec3::Y, fov: 55.0, near: 20.0, far: 40000.0 });
    }

    fn publish(&mut self, host: &mut Host) {
        let a = host.alpha.clamp(0.0, 1.0) as f32;
        let t = self.t as f32;
        let core = &self.core;
        let me = core.player.pos;
        let lerp = |p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * a, p[1] + (q[1] - p[1]) * a];
        let b = &mut *host.bridge;
        b.time = t;
        for (i, e) in core.enemies.iter().enumerate() {
            let item = &mut b.enemies[i];
            if !e.alive {
                item.kind = 0;
                continue;
            }
            let (scale, lift) = look(e.kind);
            let p = lerp(e.prev, e.pos);
            if e.vel[0].abs() + e.vel[1].abs() > 0.5 {
                self.yaws[i] = e.vel[0].atan2(e.vel[1]);
            } else {
                self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
            }
            let hop = if matches!(e.kind, EnemyKind::Skitter | EnemyKind::Dasher) { (t * 11.0 + i as f32).sin().abs() * 14.0 } else { 0.0 };
            let bob = if e.kind == EnemyKind::Wisp { (t * 2.5 + i as f32).sin() * 16.0 } else { 0.0 };
            let shake = if e.winding_up() { (t * 70.0).sin() * 14.0 } else { 0.0 };
            item.kind = e.kind as u8;
            item.pos = Self::at([p[0] + shake, p[1]], lift + hop + bob);
            item.yaw = self.yaws[i];
            item.scale = scale * (1.0 + 0.12 * e.flash as f32 / 4.0);
        }
        for (i, bolt) in core.bolts.iter().enumerate() {
            let item = &mut b.bolts[i];
            if bolt.alive {
                let p = lerp(bolt.prev, bolt.pos);
                *item = RItem { kind: 1, pos: Self::at(p, 85.0), yaw: 0.0, scale: bolt.radius / 42.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, o) in core.orbs.iter().enumerate() {
            let item = &mut b.orbs[i];
            if o.alive {
                *item = RItem { kind: 1, pos: Self::at(o.pos, 75.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, g) in core.gems.iter().enumerate() {
            let item = &mut b.gems[i];
            let d2 = (g.pos[0] - me[0]).powi(2) + (g.pos[1] - me[1]).powi(2);
            if g.alive && d2 < DRAW_RANGE * DRAW_RANGE {
                let (tier, scale) = if g.kind == GemKind::Heart { (3.0, 1.0) } else { (g.tier() as f32, 0.8 + 0.3 * g.tier() as f32) };
                *item = RItem { kind: 1, pos: Self::at(g.pos, 45.0 + (t * 3.0 + i as f32).sin() * 8.0), yaw: 0.0, scale, aux: tier };
            } else {
                item.kind = 0;
            }
        }
        for (i, r) in core.rings.iter().enumerate() {
            let item = &mut b.rings[i];
            if r.alive {
                let kind = match r.kind {
                    RingKind::Nova => 1,
                    RingKind::Quake => 2,
                    RingKind::Revive => 3,
                };
                *item = RItem { kind, pos: Self::at(r.center, 10.0), yaw: 0.0, scale: (r.radius() * crate::render::SCALE).max(0.01), aux: r.fade() };
            } else {
                item.kind = 0;
            }
        }
        for i in 0..ORBIT_MAX {
            let item = &mut b.orbit[i];
            if i < core.orbit_count {
                *item = RItem { kind: 1, pos: Self::at(core.orbit_pos[i], 95.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        b.hud.hp = core.player.hp;
        b.hud.max_hp = core.player.max_hp;
        b.hud.xp = core.player.xp;
        b.hud.xp_need = xp_to_next(core.player.level);
        b.hud.level = core.player.level;
        b.hud.secs = core.secs();
        b.hud.kills = core.kills;
        b.hud.hint = core.secs() < 9.0 && core.choices.is_none();
        // Cards follow the core's offer.
        let want = core.choices;
        let shown = !b.cards.is_empty();
        if want.is_some() != shown || (want.is_some() && self.card_gen_shown == 0) {
            b.cards.clear();
            if let Some(c) = want {
                for u in c.as_slice() {
                    let info = u.info();
                    let lv = core.up.level(*u);
                    b.cards.push(CardView { name: info.name, desc: info.desc, from: lv, to: lv + 1, rare: info.rare });
                }
                b.focus = 0;
                self.cards_at = self.t;
                self.card_gen_shown = 1;
            } else {
                self.card_gen_shown = 0;
            }
            b.card_gen = b.card_gen.wrapping_add(1);
        }
        b.cards_ready = want.is_some() && self.t - self.cards_at >= CARD_LOCKOUT;
        b.summary_ready = b.summary.is_some() && self.t - self.over_at >= SUMMARY_LOCKOUT;
    }
}

impl Experiment for StarSwarmExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Swarm
    }
    fn replaces_course(&self) -> bool {
        true
    }
    /// The game stands still while you choose an upgrade.
    fn time_scale(&self) -> f64 {
        if self.core.choices.is_some() { 0.0 } else { 1.0 }
    }

    fn enter(&mut self, host: &mut Host) {
        host.set_clear(Color::srgb(0.30, 0.22, 0.52));
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(0x1b1458, 0x7a4cc0, 0xffc2e6).with_clouds(0.5));
        host.cam.fog = Some((Color::srgb(0.62, 0.45, 0.78), 5500.0, 22000.0));
        host.shell.plain_camera = true;
        let world = self.build_arena(host);
        let spawn = SpawnPoint { action: None, pos: [ORIGIN[0], ORIGIN[1] + 100.0, ORIGIN[2]], face_yaw: 0.0 };
        host.set_mario_world(Arc::new(world), spawn, -1e9);
        host.rig.distance = 1300.0;
        host.rig.pitch = 0.72;
        self.best = load_best();
        self.start_run(host);
    }

    fn exit(&mut self, host: &mut Host) {
        host.bridge.active = false;
        host.bridge.summary = None;
        host.bridge.cards.clear();
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.set_player_visible(true);
        host.cam.fog = None;
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if self.core.over {
            if matches!(key, KeyCode::Space | KeyCode::Enter | KeyCode::KeyR) && host.bridge.summary_ready {
                host.bridge.retry = true;
                return true;
            }
            return false;
        }
        let slot = match key {
            KeyCode::Digit1 => 0,
            KeyCode::Digit2 => 1,
            KeyCode::Digit3 => 2,
            _ => return false,
        };
        if self.core.choices.is_some() {
            host.bridge.pick = Some(slot);
            return true;
        }
        false
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        if self.core.over {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.core.paused() {
            return;
        }
        if std::env::var_os("JUMPMAN_SWARM_GOD").is_some() {
            self.core.player.hp = self.core.player.max_hp;
        }
        let stats = self.core.stats;
        let mario = &mut host.sim.mario;
        let air = mario.action & ACT_FLAG_AIR != 0;
        // Spring Legs: a bigger launch on takeoff.
        if air && !self.was_air && mario.vel[1] > 25.0 && stats.jump_bonus > 0.0 {
            mario.vel[1] *= 1.0 + stats.jump_bonus as f64;
        }
        self.was_air = air;
        let pound_land = mario.action == ACT_GROUND_POUND_LAND && self.last_action != ACT_GROUND_POUND_LAND;
        self.last_action = mario.action;
        // Swift Boots: extra distance on top of whatever the physics produced this step.
        if stats.move_bonus > 0.0 {
            let (vx, vz) = if air {
                (mario.vel[0], mario.vel[2])
            } else {
                let yaw = angle_to_radians(mario.face_angle[1]);
                (yaw.sin() * mario.forward_vel.max(0.0), yaw.cos() * mario.forward_vel.max(0.0))
            };
            let k = stats.move_bonus as f64;
            let lim = ARENA_HALF as f64 - 90.0;
            let (nx, nz) = ((mario.pos[0] + vx * k).clamp(ORIGIN[0] - lim, ORIGIN[0] + lim), (mario.pos[2] + vz * k).clamp(ORIGIN[2] - lim, ORIGIN[2] + lim));
            let d = [nx - mario.pos[0], 0.0, nz - mario.pos[2]];
            host.shift_mario(d);
        }
        let m = &host.sim.mario;
        let frame = PlayerFrame { pos: [(m.pos[0] - ORIGIN[0]) as f32, (m.pos[2] - ORIGIN[2]) as f32], height: (m.pos[1] - ORIGIN[1]).max(0.0) as f32, pound_land };
        let mario_pos = m.pos;
        self.core.tick(&frame);
        let ev = self.core.ev;
        let tick = self.core.tick;
        if ev.kills > 0 && tick >= self.sfx_tick + 3 {
            self.sfx_tick = tick;
            host.fx("break_box", [ORIGIN[0] + ev.last_kill[0] as f64, ORIGIN[1] + 60.0, ORIGIN[2] + ev.last_kill[1] as f64]);
        }
        if ev.gems > 0 {
            if tick >= self.sfx_tick + 2 {
                host.fx("coin", mario_pos);
            }
        }
        if ev.hit {
            host.fx("bonk", mario_pos);
        }
        if ev.nova {
            host.fx("teleport", mario_pos);
        }
        if ev.quake {
            host.fx("bonk", mario_pos);
        }
        if ev.level_up {
            host.fx("star_sound", mario_pos);
        }
        if ev.surge {
            host.toast("THE SWARM CLOSES IN!", 1.8);
        }
        if ev.boss_spawned {
            host.toast("A KING APPROACHES!", 2.2);
        }
        if ev.boss_killed {
            host.toast("KING DEFEATED!", 2.0);
        }
        if ev.revived {
            host.toast("SECOND CHANCE!", 2.0);
        }
        if ev.died || ev.won {
            self.finish(host);
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        if host.bridge.retry {
            host.bridge.retry = false;
            if self.core.over {
                self.start_run(host);
            }
        }
        if let Some(i) = host.bridge.pick.take() {
            if self.core.choices.is_some() && self.t - self.cards_at >= CARD_LOCKOUT && self.core.pick(i) {
                host.fx("star_sound", host.sim.mario.pos);
                // The next queued level-up (if any) shows a fresh set.
                self.card_gen_shown = 0;
            }
        }
        // Brief flicker while invulnerable.
        let blink = self.core.player.iframes > 0 && !self.core.over && (self.t * 18.0) as i64 % 2 == 0;
        host.set_player_visible(!blink);
        self.publish(host);
        self.camera(host);
    }

    fn debug(&self) -> String {
        format!("t={} level={} hp={:.0} live={} kills={} gems={}", self.core.secs(), self.core.player.level, self.core.player.hp, self.core.alive_enemies, self.core.kills, self.core.gems.iter().filter(|g| g.alive).count())
    }
}
