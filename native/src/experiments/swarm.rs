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
use super::foes_view::FoeItem;
use super::swarm_core::*;
use super::swarm_view::{CardView, RItem, Summary};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use bevy::math::DVec3;
use crate::render::to_bevy;
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::*;
use jumpman_sim::math::Vec3 as V3;

/// The arena on Bob-omb Battlefield: the open meadow below the castle bridge (SM64 units, x/z of the
/// centre) and its half extent. Chosen from the course's collision: the ground there is level (the
/// mound at its west edge is walkable) and nothing but a few cliffs and a pond interrupts it.
pub const ARENA_CENTER: [f64; 2] = [-500.0, 4750.0];
pub const ARENA_SIZE: f32 = 2250.0;
/// Cell size of the terrain sampling grid.
const CELL: f32 = 100.0;
/// Pause between a card appearing and it accepting input (stray thumbs).
const CARD_LOCKOUT: f64 = 0.5;
const SUMMARY_LOCKOUT: f64 = 0.9;
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
        // The ported species are drawn by `foes_view`.
        _ => (1.0, 0.0),
    }
}

/// The course's ground under the arena, sampled once on entering: heights for snapping everything
/// to the terrain and a walkable mask for the enemies.
pub struct Terrain {
    center: [f64; 2],
    half: f32,
    base: f32,
    /// (n + 1)^2 corner heights, row-major over z then x.
    heights: Vec<f32>,
    n: usize,
}

impl Terrain {
    pub fn sample(world: &CollisionWorld, center: [f64; 2], half: f32, base_hint: f64) -> Self {
        let n = (half * 2.0 / CELL) as usize;
        let mut heights = Vec::with_capacity((n + 1) * (n + 1));
        let mut raw = |x: f64, z: f64| {
            let hit = world.find_floor(x, base_hint + 2500.0, z);
            match hit.surface {
                Some(s) if s.normal[1] > 0.7 && !matches!(s.kind, 0x01 | 0x0a | 0x12 | 0x13) => Some(hit.height as f32),
                _ => None,
            }
        };
        let mut found = vec![];
        for iz in 0..=n {
            for ix in 0..=n {
                found.push(raw(center[0] + (-half + ix as f32 * CELL) as f64, center[1] + (-half + iz as f32 * CELL) as f64));
            }
        }
        // The level of the meadow: the median of what was found.
        let mut sorted: Vec<f32> = found.iter().flatten().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let base = sorted.get(sorted.len() / 2).copied().unwrap_or(base_hint as f32);
        for h in &found {
            heights.push(h.unwrap_or(f32::NAN));
        }
        Self { center, half, base, heights, n }
    }

    pub fn center(&self) -> [f64; 2] {
        self.center
    }

    fn corner(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz.min(self.n) * (self.n + 1) + ix.min(self.n)]
    }

    /// Ground height at an arena-local point (bilinear; blocked corners count as the meadow level).
    pub fn height(&self, p: [f32; 2]) -> f32 {
        let fx = ((p[0] + self.half) / CELL).clamp(0.0, self.n as f32);
        let fz = ((p[1] + self.half) / CELL).clamp(0.0, self.n as f32);
        let (ix, iz) = (fx.floor() as usize, fz.floor() as usize);
        let (tx, tz) = (fx - ix as f32, fz - iz as f32);
        let c = |x, z| {
            let h = self.corner(x, z);
            if h.is_nan() { self.base } else { h }
        };
        let a = c(ix, iz) + (c(ix + 1, iz) - c(ix, iz)) * tx;
        let b = c(ix, iz + 1) + (c(ix + 1, iz + 1) - c(ix, iz + 1)) * tx;
        a + (b - a) * tz
    }

    /// Can something walk on cell (ix, iz)? Needs ground at all four corners, near the meadow's
    /// level, without a cliff step between neighbours.
    fn cell_ok(&self, ix: usize, iz: usize) -> bool {
        let hs = [self.corner(ix, iz), self.corner(ix + 1, iz), self.corner(ix, iz + 1), self.corner(ix + 1, iz + 1)];
        if hs.iter().any(|h| h.is_nan() || (h - self.base).abs() > 320.0) {
            return false;
        }
        let (lo, hi) = hs.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
        hi - lo < 110.0
    }

    pub fn mask(&self) -> WalkMask {
        WalkMask::build(self.half, CELL, |x, z| {
            let (ix, iz) = (((x + self.half) / CELL) as usize, ((z + self.half) / CELL) as usize);
            self.cell_ok(ix, iz)
        })
    }
}

/// Frame-rate independent smoothing of the point the camera looks at. The player is already
/// interpolated to the render time; this takes the last bit of unevenness out (landings, hits,
/// the running bob) without trailing noticeably: at 12 /s the lag behind a full-speed run is a
/// tenth of the screen's height at most.
#[derive(Clone, Copy, Debug)]
pub struct Follow {
    pub focus: DVec3,
    primed: bool,
}

impl Follow {
    pub const RATE_XZ: f64 = 14.0;
    pub const RATE_Y: f64 = 5.0;

    pub fn new() -> Self {
        Self { focus: DVec3::ZERO, primed: false }
    }
    /// Jump straight to `target` (a restart, a teleport).
    pub fn snap(&mut self) {
        self.primed = false;
    }
    pub fn update(&mut self, target: DVec3, dt: f64) -> DVec3 {
        if !self.primed || self.focus.distance(target) > 2500.0 {
            self.focus = target;
            self.primed = true;
        } else {
            let kxz = 1.0 - (-Self::RATE_XZ * dt).exp();
            let ky = 1.0 - (-Self::RATE_Y * dt).exp();
            self.focus.x += (target.x - self.focus.x) * kxz;
            self.focus.z += (target.z - self.focus.z) * kxz;
            self.focus.y += (target.y - self.focus.y) * ky;
        }
        self.focus
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
    /// Where the arena sits on the course (centre x/z, floor level) and its sampled ground.
    center: [f64; 3],
    terrain: Option<Terrain>,
    follow: Follow,
    /// Collision of the invisible walls around the arena (removed on exit).
    walls: Vec<usize>,
}

impl StarSwarmExperiment {
    pub fn new() -> Self {
        let seed = run_seed();
        Self { core: Swarm::new(seed), seed, runs: 0, t: 0.0, last_action: 0, was_air: false, best: load_best(), new_best: false, card_gen_shown: 0, cards_at: 0.0, over_at: 0.0, sfx_tick: 0, yaws: vec![0.0; ENEMY_SLOTS], first_run: true, center: [ARENA_CENTER[0], 0.0, ARENA_CENTER[1]], terrain: None, follow: Follow::new(), walls: vec![] }
    }

    /// Invisible walls on the square's edge, so running into it stops the player dead (no pushing back).
    fn build_walls(&mut self, host: &mut Host) {
        let o = jumpman_sim::collision::SurfaceOptions::default();
        let (cx, cz, h) = (self.center[0], self.center[2], ARENA_SIZE as f64 - 60.0);
        let (y0, y1) = (-2000.0, 6000.0);
        let mut faces = vec![];
        let w = host.world_mut();
        let quad = |a: [f64; 2], b: [f64; 2]| [[a[0], y0, a[1]], [b[0], y0, b[1]], [b[0], y1, b[1]], [a[0], y1, a[1]]];
        use super::geometry::add_face;
        add_face(w, &quad([cx - h, cz - h], [cx - h, cz + h]), [1.0, 0.0, 0.0], &mut faces, &o);
        add_face(w, &quad([cx + h, cz - h], [cx + h, cz + h]), [-1.0, 0.0, 0.0], &mut faces, &o);
        add_face(w, &quad([cx - h, cz - h], [cx + h, cz - h]), [0.0, 0.0, 1.0], &mut faces, &o);
        add_face(w, &quad([cx - h, cz + h], [cx + h, cz + h]), [0.0, 0.0, -1.0], &mut faces, &o);
        self.walls = faces.iter().map(|f| std::sync::Arc::as_ptr(f) as usize).collect();
    }

    /// A ring of glowing posts marking the edge of the arena, standing on the terrain.
    fn build_fence(&mut self, host: &mut Host) {
        let Some(t) = self.terrain.as_ref() else { return };
        let post = host.unlit(Color::srgb(1.0, 0.82, 0.25));
        let rail = host.unlit(Color::srgba(1.0, 0.9, 0.5, 0.55));
        let edge = ARENA_SIZE - 40.0;
        let per_side = (edge * 2.0 / 300.0) as usize;
        let mut spots = vec![];
        for i in 0..per_side {
            let k = -edge + i as f32 * (edge * 2.0 / per_side as f32);
            spots.extend([[k, -edge], [edge, k], [-k, edge], [-edge, -k]]);
        }
        let boxes: Vec<(V3, V3)> = spots
            .iter()
            .map(|p| {
                let y = t.height(*p) as f64;
                ([self.center[0] + p[0] as f64, y + 90.0, self.center[2] + p[1] as f64], [28.0, 180.0, 28.0])
            })
            .collect();
        for (c, size) in boxes {
            host.spawn_box(c, size, 0.0, post.clone());
            host.spawn_box([c[0], c[1] + 60.0, c[2]], [size[0] * 0.6, 24.0, size[2] * 0.6], 0.0, rail.clone());
        }
    }

    fn start_run(&mut self, host: &mut Host) {
        self.runs += 1;
        let seed = self.seed.wrapping_add(self.runs as u64 * 7919);
        self.core = Swarm::new(seed);
        if let Some(t) = &self.terrain {
            self.core.half = ARENA_SIZE;
            self.core.mask = Some(t.mask());
        }
        self.follow.snap();
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
        // Start on the meadow, facing the castle.
        let start_y = self.ground([0.0, 0.0]) as f64;
        host.place_mario([self.center[0], start_y, self.center[2]], Some(face));
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

    /// Ground height under an arena-local point.
    fn ground(&self, p: [f32; 2]) -> f32 {
        self.terrain.as_ref().map_or(0.0, |t| t.height(p))
    }

    /// World position (SM64 units, Bevy-mapped) of an arena-plane point at `height` above the ground.
    fn at(&self, p: [f32; 2], height: f32) -> Vec3 {
        to_bevy([self.center[0] + p[0] as f64, (self.ground(p) + height) as f64, self.center[2] + p[1] as f64])
    }

    /// A chase camera above the field that ignores the walls and the swarm; the follow rig still
    /// supplies the zoom and the yaw the player drags (its automatic turn-behind-you is off, see
    /// `enter`), and the look-at point is eased by [`Follow`].
    fn camera(&mut self, host: &mut Host, dt: f64) {
        let target = DVec3::from(host.render_pos()) + DVec3::new(0.0, 92.0, 0.0);
        let focus = self.follow.update(target, dt);
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
                if e.vel[0].abs() + e.vel[1].abs() > 0.5 {
                    self.yaws[i] = e.vel[0].atan2(e.vel[1]);
                } else {
                    self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
                }
                let shake = if e.winding_up() { (t * 70.0).sin() * 14.0 } else { 0.0 };
                let flies = if sp.flies() { 130.0 + (t * 3.0 + i as f32).sin() * 14.0 } else { 0.0 };
                let hop = e.hop_height();
                b.foes.push(FoeItem { id: i as u32, species: sp, pos: self.at([p[0] + shake, p[1]], flies + hop), yaw: self.yaws[i], radius: e.radius * 0.95, pose: e.pose(), pulse: 1.0 + 0.12 * e.flash as f32 / 4.0, age: 0.0 });
                item.kind = 0;
                continue;
            }
            if e.vel[0].abs() + e.vel[1].abs() > 0.5 {
                self.yaws[i] = e.vel[0].atan2(e.vel[1]);
            } else {
                self.yaws[i] = (me[0] - p[0]).atan2(me[1] - p[1]);
            }
            let hop = if matches!(e.kind, EnemyKind::Skitter | EnemyKind::Dasher) { (t * 11.0 + i as f32).sin().abs() * 14.0 } else { 0.0 };
            let bob = if e.kind == EnemyKind::Wisp { (t * 2.5 + i as f32).sin() * 16.0 } else { 0.0 };
            let shake = if e.winding_up() { (t * 70.0).sin() * 14.0 } else { 0.0 };
            item.kind = e.kind as u8;
            item.pos = self.at([p[0] + shake, p[1]], lift + hop + bob);
            item.yaw = self.yaws[i];
            item.scale = scale * (1.0 + 0.12 * e.flash as f32 / 4.0);
        }
        for (i, bolt) in core.bolts.iter().enumerate() {
            let item = &mut b.bolts[i];
            if bolt.alive {
                let p = lerp(bolt.prev, bolt.pos);
                *item = RItem { kind: 1, pos: self.at(p, 85.0), yaw: 0.0, scale: bolt.radius / 42.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, o) in core.orbs.iter().enumerate() {
            let item = &mut b.orbs[i];
            if o.alive {
                *item = RItem { kind: 1, pos: self.at(o.pos, 75.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
            } else {
                item.kind = 0;
            }
        }
        for (i, g) in core.gems.iter().enumerate() {
            let item = &mut b.gems[i];
            let d2 = (g.pos[0] - me[0]).powi(2) + (g.pos[1] - me[1]).powi(2);
            if g.alive && d2 < DRAW_RANGE * DRAW_RANGE {
                let (tier, scale) = if g.kind == GemKind::Heart { (3.0, 1.0) } else { (g.tier() as f32, 0.8 + 0.3 * g.tier() as f32) };
                *item = RItem { kind: 1, pos: self.at(g.pos, 45.0 + (t * 3.0 + i as f32).sin() * 8.0), yaw: 0.0, scale, aux: tier };
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
                *item = RItem { kind, pos: self.at(r.center, 10.0), yaw: 0.0, scale: (r.radius() * crate::render::SCALE).max(0.01), aux: r.fade() };
            } else {
                item.kind = 0;
            }
        }
        for i in 0..ORBIT_MAX {
            let item = &mut b.orbit[i];
            if i < core.orbit_count {
                *item = RItem { kind: 1, pos: self.at(core.orbit_pos[i], 95.0), yaw: 0.0, scale: 1.0, aux: 0.0 };
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
    /// Bob-omb Battlefield, kept visible: the swarm fights on its meadow.
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }
    /// The game stands still while you choose an upgrade.
    fn time_scale(&self) -> f64 {
        if self.core.choices.is_some() { 0.0 } else { 1.0 }
    }

    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        // A top-down camera must not swing round to follow the player's heading.
        host.rig.auto_follow_yaw = false;
        let world = host.course_world();
        let terrain = Terrain::sample(&world, ARENA_CENTER, ARENA_SIZE, 0.0);
        self.center = [ARENA_CENTER[0], 0.0, ARENA_CENTER[1]];
        info!("swarm arena: base {:.0}, ground at centre {:.0}, walkable {:.0}%", terrain.base, terrain.height([0.0, 0.0]), terrain.mask().fraction() * 100.0);
        self.terrain = Some(terrain);
        self.build_walls(host);
        self.build_fence(host);
        host.rig.distance = 1300.0;
        host.rig.pitch = 0.72;
        self.best = load_best();
        self.start_run(host);
    }

    fn exit(&mut self, host: &mut Host) {
        host.bridge.active = false;
        host.bridge.summary = None;
        host.bridge.cards.clear();
        host.rig.auto_follow_yaw = true;
        let walls = std::mem::take(&mut self.walls);
        if !walls.is_empty() {
            host.world_mut().remove_surfaces(&|s| walls.contains(&(s as *const jumpman_sim::collision::Surface as usize)));
        }
        host.sim.mario.reset_to_spawn();
        host.set_player_visible(true);
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
            let lim = ARENA_SIZE as f64 - 90.0;
            let (nx, nz) = ((mario.pos[0] + vx * k).clamp(self.center[0] - lim, self.center[0] + lim), (mario.pos[2] + vz * k).clamp(self.center[2] - lim, self.center[2] + lim));
            let d = [nx - mario.pos[0], 0.0, nz - mario.pos[2]];
            host.shift_mario(d);
        }
        // The edge of the field is a wall to the player; course creatures cannot hurt him (the swarm does).
        let lim = ARENA_SIZE as f64 - 90.0;
        {
            let m = &host.sim.mario;
            let d = [(m.pos[0].clamp(self.center[0] - lim, self.center[0] + lim)) - m.pos[0], 0.0, (m.pos[2].clamp(self.center[2] - lim, self.center[2] + lim)) - m.pos[2]];
            if d[0] != 0.0 || d[2] != 0.0 {
                host.shift_mario(d);
            }
        }
        host.sim.mario.health = 0x880 as f64;
        host.sim.mario.hurt_counter = 0.0;
        let m = &host.sim.mario;
        let local = [(m.pos[0] - self.center[0]) as f32, (m.pos[2] - self.center[2]) as f32];
        let frame = PlayerFrame { pos: local, height: (m.pos[1] as f32 - self.ground(local)).max(0.0), pound_land };
        let mario_pos = m.pos;
        self.core.tick(&frame);
        let ev = self.core.ev;
        let tick = self.core.tick;
        if ev.kills > 0 && tick >= self.sfx_tick + 3 {
            self.sfx_tick = tick;
            host.fx("break_box", [self.center[0] + ev.last_kill[0] as f64, self.ground(ev.last_kill) as f64 + 60.0, self.center[2] + ev.last_kill[1] as f64]);
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
        self.camera(host, dt);
    }

    fn debug(&self) -> String {
        format!("t={} level={} hp={:.0} live={} kills={} gems={}", self.core.secs(), self.core.player.level, self.core.player.hp, self.core.alive_enemies, self.core.kills, self.core.gems.iter().filter(|g| g.alive).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::course_data::CourseData;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn bob_terrain() -> Option<(GameSim, Terrain)> {
        if !CourseData::path(&assets(), "bob", 1).is_file() {
            return None;
        }
        let sim = GameSim::load(&assets(), CourseRequest::new("bob")).ok()?;
        let t = Terrain::sample(&sim.mario.world, ARENA_CENTER, ARENA_SIZE, 0.0);
        Some((sim, t))
    }

    #[test]
    fn arena_sits_on_real_walkable_bob_omb_ground() {
        let Some((sim, t)) = bob_terrain() else { return };
        let mask = t.mask();
        assert!(mask.fraction() > 0.6, "only {:.0}% of the arena is walkable", mask.fraction() * 100.0);
        // The start point and its surroundings are open ground, and the sampled height is the real floor.
        for (x, z) in [(0.0f32, 0.0f32), (300.0, 0.0), (-300.0, 0.0), (0.0, 300.0), (0.0, -300.0)] {
            assert!(mask.at([x, z]), "({x},{z}) is blocked");
            let real = sim.mario.world.find_floor(ARENA_CENTER[0] + x as f64, 2500.0, ARENA_CENTER[1] + z as f64).height as f32;
            assert!((t.height([x, z]) - real).abs() < 60.0, "ground {} vs floor {real}", t.height([x, z]));
        }
    }

    #[test]
    fn enemies_spawn_and_walk_only_on_walkable_ground_of_the_course() {
        let Some((_, t)) = bob_terrain() else { return };
        for seed in 1..=4u64 {
            let mut s = Swarm::with_cap(seed, 160);
            s.half = ARENA_SIZE;
            s.mask = Some(t.mask());
            let mut worst = 0usize;
            for tick in 0..(150 * TICKS_PER_SEC) {
                // The player strolls a small circle in the open middle.
                let a = tick as f32 * 0.01;
                s.player.hp = s.player.max_hp;
                s.tick(&PlayerFrame { pos: [a.cos() * 500.0, a.sin() * 500.0], height: 0.0, pound_land: false });
                while s.choices.is_some() {
                    s.pick(0);
                }
                for e in s.enemies.iter().filter(|e| e.alive) {
                    // Pushing apart may nudge a foe a hair over a cell edge; never into the next cell over.
                    let near = [[0.0, 0.0], [40.0, 0.0], [-40.0, 0.0], [0.0, 40.0], [0.0, -40.0]].iter().any(|d| s.walkable([e.pos[0] + d[0], e.pos[1] + d[1]]));
                    assert!(near, "seed {seed} tick {tick}: {:?} at {:?} is off the ground", e.kind, e.pos);
                }
                worst = worst.max(s.alive_enemies);
                if s.over {
                    break;
                }
            }
            assert!(worst > 20, "the swarm should fill the arena (peak {worst})");
        }
    }

    /// The camera focus filter: Mario runs a straight line at a steady speed; frames arrive unevenly
    /// (a 120 Hz display with hitches) and his height bobs. The eased focus must move at an even speed.
    #[test]
    fn camera_follow_is_smooth_through_frame_jitter_and_bobbing() {
        let mut f = Follow::new();
        let mut t = 0.0f64;
        let mut prev: Option<DVec3> = None;
        let mut speeds = vec![];
        let mut raw_speeds = vec![];
        let mut prev_raw: Option<DVec3> = None;
        for n in 0..1200 {
            let dt = if n % 7 == 0 { 1.0 / 40.0 } else { 1.0 / 120.0 };
            t += dt;
            let bob = if n % 3 == 0 { 9.0 } else { 0.0 };
            let target = DVec3::new(960.0 * t, 92.0 + bob, 0.0);
            let out = f.update(target, dt);
            if n > 200 {
                if let Some(p) = prev {
                    speeds.push(p.distance(out) / dt);
                }
                if let Some(p) = prev_raw {
                    raw_speeds.push(p.distance(target) / dt);
                }
            }
            prev = Some(out);
            prev_raw = Some(target);
        }
        let cv = |v: &[f64]| {
            let m = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt() / m
        };
        assert!((cv(&speeds)) < 0.08, "filtered cv {}", cv(&speeds));
        assert!(cv(&speeds) < cv(&raw_speeds) * 0.5, "filter should calm the bob: {} vs {}", cv(&speeds), cv(&raw_speeds));
    }
}
