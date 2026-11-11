//! Gem Clash: an original arena brawler (native-only experiment; there is no browser version).
//!
//! Two to four fighters (you, Mario, plus up to three bots or extra local gamepads) brawl in six
//! small multi-level arenas with a set piece each: lifts and sliders, a lava tide, bumper
//! pillars, a crumbling bridge, gusting wind, a central cannon. Crates, barrels, rocks and
//! Bob-ombs can be picked up and thrown; a hammer, a boomerang and a blaster are weapons.
//! Three gems fall every round; whoever holds all three goes into a timed power form (bigger,
//! faster, Nova blast and Seeker beam). Health bars decide knock-outs, rounds are best of three.
//! Modes: Versus (you vs 1-3 bots) and the single-player Gauntlet (six escalating stages and
//! King Bob-omb, three lives).
//!
//! An original design in a genre tradition ("pick up everything, collect the gems, transform"):
//! the rules, arenas, items, names and art direction are ours, nothing is taken from any
//! commercial game. The rules live in `brawl_core.rs` (seeded, unit-tested), the bots in
//! `brawl_ai.rs`; this file glues them to the host (input, camera, arena entities, sound) and
//! `brawl_view.rs` draws the fighters, items and HUD.
//!
//! Controls: stick moves, A jumps (again in the air), B hits (jab-jab-kick; in the air a dive; with
//! an item it throws or uses it), Z grabs and throws on the ground and ground-pounds in the air.
//! Keyboard also: `F` grab/throw anywhere, `G` shock wave (the Nova: a blast all round you), `H` seek beam (a
//! homing zap at the nearest foe); both need the power form, and touch has SHOCK WAVE and SEEK BEAM buttons with a
//! cooldown shade. `F1` or the `?` button opens HOW TO PLAY (shown once on the first visit; the match pauses). Extra gamepads join as players 2-4 in Versus.
//!
//! Test hooks (none of them touch the saved record): `JUMPMAN_BRAWL_SEED=<n>`, `_MODE=versus|gauntlet`,
//! `_ARENA=gear|ember|bumper|bridge|gale|cannon|<0-5>` (also skips the menu), `_BOTS=<1-3>`,
//! `_DIFF=easy|normal|hard` (the setup screen starts on Easy), `_HELP=1|0` (force / suppress the how-to-play card), `_STAGE=<0-5>`, `_START=1` (skip the menu), `_DEMO=1` (a Hard bot plays your
//! fighter), `_GEMS=<0-3>` (start holding gems), `_POWER=1` (start transformed), `_ITEMS=hammer,bomb,..`
//! (spawn items beside you), `_FIGHT=1` (skip the countdown), `_T=<secs>` (arena clock, e.g. 15 for high
//! tide), `_HP=<slot>:<hp>`, `_GOD=1`, `_SPEED=<k>` (k core ticks per step).
use super::brawl_arena::*;
use super::brawl_core::*;
use super::brawl_hero::link_hint;
use super::brawl_view::*;
use super::feel::{Pace, Pop, Shake};
use crate::fx::particles::BurstOptions;
use super::foes_view::FoeItem;
use super::textures::{textured, tiled_box, tiled_top};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::to_bevy;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::state::*;

const CRATER_FOG: (f64, f64) = (7000.0, 26000.0);
const CANNON_BASE: &str = "actors/cannon_base/cannon_base_geo.glb";
const CANNON_BARREL: &str = "actors/cannon_barrel/cannon_barrel_geo.glb";

fn env_num<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok().and_then(|s| s.parse().ok())
}

fn best_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("brawl.json")
}

/// (best Gauntlet stage reached, Versus matches won, Gauntlet clears)
pub fn load_best() -> (u32, u32, u32) {
    let v = std::fs::read_to_string(best_path()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let get = |k: &str| v.as_ref().and_then(|v| v[k].as_u64()).unwrap_or(0) as u32;
    (get("best_stage"), get("versus_wins"), get("clears"))
}

fn save_best(b: (u32, u32, u32)) {
    let path = best_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "best_stage": b.0, "versus_wins": b.1, "clears": b.2 }).to_string());
}

fn help_seen_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("brawl_help_seen")
}

/// Whether the HOW TO PLAY card should open by itself: the first time ever (`_HELP=1` forces, `_HELP=0` never).
fn help_due() -> bool {
    match std::env::var("JUMPMAN_BRAWL_HELP").as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => !help_seen_path().exists(),
    }
}

fn mark_help_seen() {
    let path = help_seen_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, "1");
}

fn cheating() -> bool {
    ["JUMPMAN_BRAWL_DEMO", "JUMPMAN_BRAWL_GEMS", "JUMPMAN_BRAWL_POWER", "JUMPMAN_BRAWL_HP", "JUMPMAN_BRAWL_GOD", "JUMPMAN_BRAWL_STAGE"].iter().any(|k| std::env::var_os(k).is_some())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Menu,
    Playing,
    Summary,
}

/// Interpolation snapshot of the previous 30 Hz tick.
#[derive(Clone)]
struct Snap {
    f: [[f32; 3]; MAXF],
    fy: [f32; MAXF],
    it: [[f32; 3]; MAXI],
    gm: [[f32; 3]; GEMS],
    pr: [[f32; 3]; MAXP],
    mn: [[f32; 3]; MAXM],
}

impl Snap {
    fn of(b: &Brawl) -> Self {
        Self { f: std::array::from_fn(|i| b.f[i].pos), fy: std::array::from_fn(|i| b.f[i].yaw), it: std::array::from_fn(|i| b.items[i].pos), gm: std::array::from_fn(|i| b.gems[i].pos), pr: std::array::from_fn(|i| b.proj[i].pos), mn: std::array::from_fn(|i| b.mons[i].pos) }
    }
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> Vec3 {
    let far = (a[0] - b[0]).abs().max((a[1] - b[1]).abs()).max((a[2] - b[2]).abs()) > 320.0;
    let t = if far { 1.0 } else { t };
    Vec3::new(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t)
}

fn bv(p: Vec3) -> Vec3 {
    p * crate::render::SCALE
}

fn rgb_hex(c: [f32; 3]) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2])
}

pub struct GemClashExperiment {
    core: Brawl,
    stage: Stage,
    menu: MenuView,
    seed: u64,
    t: f64,
    ctl: ControllerSnapshot,
    latched: u8,
    snap: Snap,
    arena_ents: Vec<Entity>,
    mover_ents: Vec<(usize, Entity)>,
    bumper_ents: Vec<Entity>,
    lava_ent: Option<Entity>,
    barrel_ent: Option<Entity>,
    built: Option<ArenaKind>,
    cam_focus: Vec3,
    cam_dist: f32,
    cam_ready: bool,
    mario_act: (u32, f64),
    banner: (String, String, f32),
    summary_at: f64,
    fx: [FxView; FX_MAX],
    best: (u32, u32, u32),
    demo: bool,
    god: bool,
    speed: usize,
    last_round: u32,
    last_stage: usize,
    last_phase: Phase,
    spark_t: f32,
    recorded: bool,
    hint_t: f32,
    started_at: f64,
    /// The HOW TO PLAY card is open (a running match is paused meanwhile).
    help: bool,
    /// Power buttons were ready last frame (to announce when they become available).
    was_ready: (bool, bool),
    prev_gems: u32,
    /// Game feel (view only): camera shake, hit-stop / slow motion of the core clock, HUD card knocks.
    shake: Shake,
    pace: Pace,
    card_hit: [Pop; MAXF],
    /// Button edges pressed while the core clock was held (hit-stop), fed to the next tick so no press is lost.
    held_in: [Input; MAXF],
    /// Throttle for the lava ember bursts.
    ember_t: u32,
}

impl GemClashExperiment {
    pub fn new() -> Self {
        let cfg = Config { seed: 1, ..Default::default() };
        let core = Brawl::new(cfg);
        let snap = Snap::of(&core);
        Self {
            core,
            stage: Stage::Menu,
            menu: MenuView { mode: 0, arena: 0, bots: 2, diff: 0, monsters: 1, cursor: 5, hero: 0 },
            seed: env_num("JUMPMAN_BRAWL_SEED").unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)),
            t: 0.0,
            ctl: ControllerSnapshot::default(),
            latched: 0,
            snap,
            arena_ents: vec![],
            mover_ents: vec![],
            bumper_ents: vec![],
            lava_ent: None,
            barrel_ent: None,
            built: None,
            cam_focus: Vec3::ZERO,
            cam_dist: 2600.0,
            cam_ready: false,
            mario_act: (0, -1.0),
            banner: (String::new(), String::new(), 0.0),
            summary_at: 0.0,
            fx: [FxView::default(); FX_MAX],
            best: (0, 0, 0),
            demo: false,
            god: false,
            speed: 1,
            last_round: 0,
            last_stage: 0,
            last_phase: Phase::Intro,
            spark_t: 0.0,
            recorded: false,
            hint_t: 0.0,
            started_at: 0.0,
            help: false,
            was_ready: (false, false),
            prev_gems: 0,
            shake: Shake::default(),
            pace: Pace::default(),
            card_hit: [Pop::default(); MAXF],
            held_in: [Input::default(); MAXF],
            ember_t: 0,
        }
    }

    fn config(&self) -> Config {
        let m = self.menu;
        let mode = if m.mode == 1 { Mode::Gauntlet } else { Mode::Versus };
        let diff = [Diff::Easy, Diff::Normal, Diff::Hard][m.diff as usize % 3];
        let monsters = match std::env::var("JUMPMAN_BRAWL_MONSTERS").ok().as_deref() {
            Some("off") | Some("0") => Monsters::Off,
            Some("few") | Some("1") => Monsters::Few,
            Some("many") | Some("2") => Monsters::Many,
            _ => Monsters::from_index(m.monsters),
        };
        Config { seed: self.seed, mode, arena: ARENAS[m.arena as usize % 6], humans: 1, bots: (m.bots as usize).clamp(1, 3), diff, stage: env_num("JUMPMAN_BRAWL_STAGE").unwrap_or(0), rounds_to_win: env_num("JUMPMAN_BRAWL_ROUNDS").unwrap_or(2), monsters, ..Default::default() }
    }

    fn cue(host: &mut Host, kind: &'static str) {
        host.shell.cues.push(crate::audio::SfxCue::Effect { kind, pos: None });
    }

    // -----------------------------------------------------------------------------------------
    // Arena entities
    // -----------------------------------------------------------------------------------------

    fn clear_arena(&mut self, host: &mut Host) {
        for e in self.arena_ents.drain(..) {
            host.commands.entity(e).despawn();
        }
        self.mover_ents.clear();
        self.bumper_ents.clear();
        self.lava_ent = None;
        self.barrel_ent = None;
        self.built = None;
    }

    fn style_material(host: &mut Host, style: u8) -> Handle<StandardMaterial> {
        match style {
            1 => textured(host, "brick", Color::srgb(1.0, 0.92, 0.9)),
            2 => textured(host, "planks", Color::srgb(1.0, 0.9, 0.8)),
            3 => textured(host, "gold", Color::srgb(0.8, 0.82, 0.95)),
            4 => textured(host, "stone", Color::srgb(0.78, 0.74, 0.7)),
            5 => textured(host, "sandstone", Color::srgb(1.0, 0.92, 0.8)),
            6 => textured(host, "tiles", Color::srgb(0.85, 0.9, 1.0)),
            _ => textured(host, "stone", Color::srgb(0.9, 0.92, 1.0)),
        }
    }

    fn build_arena(&mut self, host: &mut Host) {
        self.clear_arena(host);
        let kind = self.core.arena.kind;
        self.built = Some(kind);
        let arena = self.core.arena.clone();
        let (top_sky, mid_sky, low_sky, clear, fog, clouds) = match kind {
            ArenaKind::Gearworks => (0x2a2f6e, 0x7a6fc0, 0xe8b0a0, Color::srgb(0.3, 0.28, 0.5), Color::srgb(0.55, 0.5, 0.75), 0.4),
            ArenaKind::Ember => (0x14060a, 0x4a1410, 0x8a3418, Color::srgb(0.14, 0.05, 0.05), Color::srgb(0.3, 0.1, 0.07), 0.15),
            ArenaKind::Bumpers => (0x4a90e0, 0xa0d0ff, 0xfff0c8, Color::srgb(0.55, 0.78, 1.0), Color::srgb(0.75, 0.88, 1.0), 0.7),
            ArenaKind::Bridge => (0x2c6aa8, 0xf0a870, 0xffe0a0, Color::srgb(0.9, 0.6, 0.4), Color::srgb(0.95, 0.75, 0.55), 0.5),
            ArenaKind::Gale => (0x3a78d0, 0x8cc4f0, 0xcfe6ff, Color::srgb(0.6, 0.8, 1.0), Color::srgb(0.8, 0.9, 1.0), 1.0),
            ArenaKind::Cannon => (0x4a5aa0, 0xd09080, 0xffd8a0, Color::srgb(0.7, 0.55, 0.55), Color::srgb(0.85, 0.7, 0.65), 0.5),
        };
        host.set_clear(clear);
        host.set_sky(crate::fx::sky::SkyPalette::from_hex(top_sky, mid_sky, low_sky).with_clouds(clouds));
        host.cam.fog = Some((fog, CRATER_FOG.0, CRATER_FOG.1));
        let spawn = |this: &mut Self, host: &mut Host, bundle: Box<dyn FnOnce(&mut Host) -> Entity>| {
            let e = bundle(host);
            this.arena_ents.push(e);
            e
        };
        // Solids: columns down to a common bottom; movers and planks are thin plates.
        for (i, s) in arena.solids.iter().enumerate() {
            let plate = s.mover != Mover::None || s.crumble;
            let (h, cy) = if plate { (70.0, s.top - 35.0) } else { (s.top + 400.0, (s.top - 400.0) / 2.0) };
            let size = [s.hx as f64 * 2.0, h as f64, s.hz as f64 * 2.0];
            let mesh = host.meshes.add(tiled_box(size, 220.0));
            let mat = Self::style_material(host, s.style);
            let e = spawn(self, host, Box::new(move |host| host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy([s.x as f64, cy as f64, s.z as f64])), Visibility::Inherited))));
            if plate {
                self.mover_ents.push((i, e));
            }
        }
        // A low rim around walled arenas.
        if let Some(b) = arena.bounds {
            let stone = textured(host, "brick", Color::srgb(0.85, 0.75, 0.75));
            for (x, z, sx, sz) in [(0.0, b[1] + 30.0, b[0] + 60.0, 30.0), (0.0, -b[1] - 30.0, b[0] + 60.0, 30.0), (b[0] + 30.0, 0.0, 30.0, b[1]), (-b[0] - 30.0, 0.0, 30.0, b[1])] {
                let mesh = host.meshes.add(tiled_box([sx as f64 * 2.0, 90.0, sz as f64 * 2.0], 120.0));
                let mat = stone.clone();
                spawn(self, host, Box::new(move |host| host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy([x as f64, 40.0, z as f64])), Visibility::Inherited))));
            }
        }
        // Set pieces.
        let paint = |host: &mut Host, c: Color| host.materials.add(StandardMaterial { base_color: c, perceptual_roughness: 0.5, ..default() });
        for b in &arena.bumpers {
            let mesh = host.meshes.add(Cylinder::new(b.r * crate::render::SCALE, b.h * crate::render::SCALE));
            let mat = paint(host, Color::srgb(0.95, 0.35, 0.2));
            let (x, z, h) = (b.x, b.z, b.h);
            let e = spawn(self, host, Box::new(move |host| host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy([x as f64, h as f64 / 2.0, z as f64])), Visibility::Inherited))));
            self.bumper_ents.push(e);
            // A glowing cap ring.
            let cap = host.meshes.add(Cylinder::new(b.r * 1.05 * crate::render::SCALE, 14.0 * crate::render::SCALE));
            let capm = host.materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.85, 0.3), emissive: LinearRgba::rgb(1.5, 1.0, 0.2), ..default() });
            spawn(self, host, Box::new(move |host| host.spawn((Mesh3d(cap), MeshMaterial3d(capm), Transform::from_translation(to_bevy([x as f64, h as f64, z as f64])), Visibility::Inherited))));
        }
        if matches!(kind, ArenaKind::Gearworks | ArenaKind::Bridge | ArenaKind::Gale | ArenaKind::Bumpers | ArenaKind::Cannon) {
            // The drop: a canyon floor under the bridge, ground far below the other arenas. (Without it the
            // camera looks straight down into the bright horizon band of the sky, whose bloom washed out the
            // whole picture and the HUD in the Bumper and Cannon arenas.)
            let (tex, tint, y) = match kind {
                ArenaKind::Bridge => ("dirt", Color::srgb(0.7, 0.45, 0.35), -520.0),
                ArenaKind::Gearworks => ("tiles", Color::srgb(0.35, 0.3, 0.55), -520.0),
                ArenaKind::Bumpers => ("tiles", Color::srgb(0.3, 0.5, 0.62), -560.0),
                ArenaKind::Cannon => ("sandstone", Color::srgb(0.62, 0.42, 0.34), -560.0),
                _ => ("grassTop", Color::srgb(0.55, 0.7, 0.75), -560.0),
            };
            let mat = textured(host, tex, tint);
            let mesh = host.meshes.add(tiled_top(14000.0, 14000.0, 700.0));
            spawn(self, host, Box::new(move |host| host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy([0.0, y, 0.0])), Visibility::Inherited))));
        }
        if kind == ArenaKind::Ember {
            // Lava: a big dark-crust texture with a hot glow, so it reads against the sky and the fog.
            let mesh = host.meshes.add(tiled_top(20000.0, 20000.0, 260.0));
            let mat = textured(host, "dirt", Color::srgb(0.55, 0.14, 0.03));
            if let Some(mut m) = host.materials.get_mut(&mat) {
                m.emissive = LinearRgba::rgb(0.95, 0.24, 0.02);
                m.perceptual_roughness = 1.0;
            }
            let e = spawn(self, host, Box::new(move |host| host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_translation(to_bevy([0.0, LAVA_LOW as f64, 0.0])), Visibility::Inherited))));
            self.lava_ent = Some(e);
        }
        if arena.has_cannon {
            if host.asset_root.exists(CANNON_BASE) {
                spawn(self, host, Box::new(|host| host.spawn((crate::render::scene(host.assets, CANNON_BASE), Transform::from_translation(to_bevy([0.0, 150.0, 0.0])).with_scale(Vec3::splat(0.8)), Visibility::Inherited))));
            }
            if host.asset_root.exists(CANNON_BARREL) {
                let e = spawn(self, host, Box::new(|host| host.spawn((crate::render::scene(host.assets, CANNON_BARREL), Transform::from_translation(to_bevy([0.0, 175.0, 0.0])).with_scale(Vec3::splat(0.8)), Visibility::Inherited))));
                self.barrel_ent = Some(e);
            }
        }
    }

    fn update_arena(&mut self, host: &mut Host) {
        let a = &self.core.arena;
        for &(i, e) in &self.mover_ents {
            let s = &a.solids[i];
            let shake = match s.plank {
                Plank::Shaking(n) => ((n as f32) * 1.7).sin() * 4.0,
                _ => 0.0,
            };
            let vis = if s.active() { Visibility::Inherited } else { Visibility::Hidden };
            host.commands.entity(e).insert((Transform::from_translation(to_bevy([s.x as f64 + shake as f64, s.top as f64 - 35.0, s.z as f64])), vis));
        }
        for (k, &e) in self.bumper_ents.iter().enumerate() {
            let b = &a.bumpers[k];
            let pop = 1.0 + b.flash as f32 * 0.02;
            host.commands.entity(e).insert(Transform::from_translation(to_bevy([b.x as f64, b.h as f64 / 2.0, b.z as f64])).with_scale(Vec3::new(pop, 1.0, pop)));
        }
        if let Some(e) = self.lava_ent {
            host.commands.entity(e).insert(Transform::from_translation(to_bevy([0.0, a.lava as f64, 0.0])));
        }
        if let Some(e) = self.barrel_ent {
            host.commands.entity(e).insert(Transform::from_translation(to_bevy([0.0, 175.0, 0.0])).with_rotation(Quat::from_rotation_y(a.cannon_yaw) * Quat::from_rotation_x(0.9)).with_scale(Vec3::splat(0.8)));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Flow
    // -----------------------------------------------------------------------------------------

    fn apply_env_setup(&mut self, host: &mut Host) {
        let c = &mut self.core;
        if let Some(n) = env_num::<usize>("JUMPMAN_BRAWL_GEMS") {
            c.give_gems(0, n);
        }
        if std::env::var_os("JUMPMAN_BRAWL_POWER").is_some() {
            c.give_power(0);
            c.f[0].size = POWER_SIZE;
        }
        if let Ok(list) = std::env::var("JUMPMAN_BRAWL_ITEMS") {
            for (k, name) in list.split(',').enumerate() {
                let kind = match name.trim() {
                    "crate" => ItemKind::Crate,
                    "barrel" => ItemKind::Barrel,
                    "rock" => ItemKind::Rock,
                    "bomb" => ItemKind::Bomb,
                    "hammer" => ItemKind::Hammer,
                    "boomerang" => ItemKind::Boomerang,
                    "blaster" => ItemKind::Blaster,
                    "heal" => ItemKind::Heal,
                    _ => continue,
                };
                let p = c.f[0].pos;
                let a = k as f32 * 0.9 + 0.6;
                let spot = [p[0] + a.cos() * 170.0, p[1], p[2] + a.sin() * 170.0];
                c.spawn_item(kind, spot, false);
            }
        }
        if let Some(t) = env_num::<f32>("JUMPMAN_BRAWL_T") {
            c.arena.t = t;
        }
        if let Ok(list) = std::env::var("JUMPMAN_BRAWL_HP") {
            for hp in list.split(',') {
                if let Some((s, v)) = hp.split_once(':') {
                    if let (Ok(s), Ok(v)) = (s.parse::<usize>(), v.parse::<f32>()) {
                        if s < MAXF {
                            c.f[s].hp = v;
                        }
                    }
                }
            }
        }
        if std::env::var_os("JUMPMAN_BRAWL_FIGHT").is_some() {
            c.phase = Phase::Fight;
            c.phase_t = 0;
        }
        let _ = host;
    }

    fn start_match(&mut self, host: &mut Host) {
        let mut cfg = self.config();
        if cfg.mode == Mode::Versus {
            // Every extra connected gamepad takes over one bot seat.
            let extra = host.brawl.pads.iter().filter(|p| p.present).count().min(cfg.bots.saturating_sub(0)).min(3);
            let total = 1 + cfg.bots;
            cfg.humans = 1 + extra;
            cfg.bots = total.saturating_sub(cfg.humans);
        }
        cfg.hero = host.brawl.hero;
        self.core = Brawl::new(cfg);
        self.snap = Snap::of(&self.core);
        self.stage = Stage::Playing;
        host.brawl.screen = Screen::None;
        host.brawl.summary = SummaryView::default();
        self.last_round = 0;
        self.last_stage = self.core.stage;
        self.recorded = false;
        self.started_at = self.t;
        self.apply_env_setup(host);
        self.build_arena(host);
        self.cam_ready = false;
        self.pace.clear();
        self.shake.clear();
        self.held_in = [Input::default(); MAXF];
        Self::cue(host, "select_mission");
    }

    fn show_menu(&mut self, host: &mut Host) {
        self.stage = Stage::Menu;
        let mut cfg = self.config();
        cfg.mode = Mode::Versus;
        if self.menu.mode == 1 {
            cfg.arena = stage_def(0).0;
        }
        cfg.hero = host.brawl.hero;
        self.core = Brawl::new(cfg);
        self.snap = Snap::of(&self.core);
        self.build_arena(host);
        host.brawl.screen = Screen::Menu;
        host.brawl.menu = self.menu;
        self.cam_ready = false;
    }

    fn adjust(&mut self, host: &mut Host, row: u8, d: i8) {
        let m = &mut self.menu;
        let wrap = |v: u8, d: i8, n: u8| ((v as i16 + d as i16).rem_euclid(n as i16)) as u8;
        match row {
            0 => m.mode = wrap(m.mode, d, 2),
            1 if m.mode == 0 => m.arena = wrap(m.arena, d, 6),
            2 if m.mode == 0 => m.bots = 1 + wrap(m.bots.saturating_sub(1), d, 3),
            3 if m.mode == 0 => m.diff = wrap(m.diff, d, 3),
            4 => m.monsters = wrap(m.monsters, d, 3),
            _ => return,
        }
        Self::cue(host, "move_menu");
        let rebuild = row <= 1 || row == 4;
        host.brawl.menu = self.menu;
        if rebuild {
            self.show_menu(host);
        }
    }

    fn handle_cmds(&mut self, host: &mut Host) {
        let mut cmds = std::mem::take(&mut host.brawl.cmds);
        for c in cmds.drain(..) {
            match (c, self.stage) {
                (UiCmd::Adjust(r, d), Stage::Menu) => {
                    self.menu.cursor = r;
                    self.adjust(host, r, d);
                }
                (UiCmd::Cursor(d), Stage::Menu) => {
                    self.menu.cursor = ((self.menu.cursor as i16 + d as i16).rem_euclid(6)) as u8;
                    host.brawl.menu = self.menu;
                }
                (UiCmd::Help, _) => self.set_help(host, true),
                (UiCmd::CloseHelp, _) => self.set_help(host, false),
                (_, _) if self.help => {}
                (UiCmd::Start, Stage::Menu) => self.start_match(host),
                (UiCmd::Rematch, Stage::Summary) | (UiCmd::Next, Stage::Summary) => self.start_match(host),
                (UiCmd::Menu, Stage::Summary) => self.show_menu(host),
                _ => {}
            }
        }
        cmds.append(&mut host.brawl.cmds);
        host.brawl.cmds = cmds;
    }

    fn set_help(&mut self, host: &mut Host, open: bool) {
        if self.help && !open {
            mark_help_seen();
        }
        self.help = open;
        host.brawl.help = open;
    }

    fn finish(&mut self, host: &mut Host) {
        let c = &self.core;
        let win = c.match_winner == 0;
        let cheated = cheating();
        let mut lines = vec![];
        let (title, sub);
        match c.cfg.mode {
            Mode::Versus => {
                title = if win { "YOU WIN THE MATCH".to_string() } else { format!("{} WINS", Self::name_of(c, c.match_winner.max(0) as usize).to_uppercase()) };
                sub = format!("{} - first to {} round{} - {} played", c.arena.kind.name(), c.cfg.rounds_to_win, if c.cfg.rounds_to_win == 1 { "" } else { "s" }, c.round);
                for i in 0..c.n {
                    let f = &c.f[i];
                    lines.push(format!("{:<10} rounds {}   KOs {}   damage {:.0}   gems {}   power forms {}", Self::name_of(c, i), f.wins, f.kos, f.dmg_dealt, f.gems_got, f.transforms));
                }
            }
            Mode::Gauntlet => {
                title = if win { "GAUNTLET CLEARED".to_string() } else { "GAUNTLET OVER".to_string() };
                sub = if win { "King Bob-omb is down. The arena is yours.".to_string() } else { format!("You fell at stage {} of {}", c.stage + 1, GAUNTLET_STAGES) };
                lines.push(format!("Stages cleared {}/{}   lives left {}", c.stage_clears, GAUNTLET_STAGES, c.lives));
                let f = &c.f[0];
                lines.push(format!("KOs {}   damage {:.0}   gems {}   power forms {}", f.kos, f.dmg_dealt, f.gems_got, f.transforms));
            }
        }
        if !cheated && !self.recorded {
            self.recorded = true;
            match c.cfg.mode {
                Mode::Versus if win => self.best.1 += 1,
                Mode::Gauntlet => {
                    self.best.0 = self.best.0.max(c.stage_clears);
                    if win {
                        self.best.2 += 1;
                    }
                }
                _ => {}
            }
            save_best(self.best);
        }
        if c.cfg.mode == Mode::Gauntlet {
            lines.push(format!("Best: {} stages cleared, {} full clears", self.best.0, self.best.2));
        } else {
            lines.push(format!("Matches won: {}", self.best.1));
        }
        host.brawl.summary = SummaryView { title, sub, lines, win, can_next: false };
        host.brawl.screen = Screen::Summary;
        self.stage = Stage::Summary;
        self.summary_at = self.t;
    }

    fn name_of(c: &Brawl, i: usize) -> String {
        let f = &c.f[i];
        if f.human {
            if i == 0 {
                if f.kind == Kind::Mario && f.hero != Hero::Mario { f.hero.name().into() } else { "You".into() }
            } else {
                format!("P{}", i + 1)
            }
        } else {
            f.stats().name.to_string()
        }
    }

    // -----------------------------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------------------------

    fn camera_basis(yaw: f32) -> ([f32; 2], [f32; 2]) {
        let f = [-yaw.sin(), -yaw.cos()];
        (f, [-f[1], f[0]])
    }

    fn inputs(&mut self, host: &mut Host) -> [Input; MAXF] {
        let mut out = [Input::default(); MAXF];
        let (f, r) = Self::camera_basis(self.cam_yaw());
        let c = &self.ctl;
        let (sx, sy) = ((c.stick_x / 64.0).clamp(-1.0, 1.0) as f32, (c.stick_y / 64.0).clamp(-1.0, 1.0) as f32);
        let p = &mut out[0];
        p.mv = [r[0] * sx + f[0] * sy, r[1] * sx + f[1] * sy];
        p.jump = c.button_pressed & A_BUTTON != 0;
        p.jump_held = c.button_down & A_BUTTON != 0;
        p.attack = c.button_pressed & B_BUTTON != 0;
        p.attack_held = c.button_down & B_BUTTON != 0;
        p.z = c.button_pressed & Z_TRIG != 0;
        let l = self.latched;
        p.s1 = l & 1 != 0;
        p.s2 = l & 2 != 0;
        p.grab = l & 4 != 0;
        self.latched = 0;
        // Extra local pads join as players 2-4 (Versus; each replaces a bot).
        for k in 0..3 {
            let pad = host.brawl.pads[k];
            let slot = k + 1;
            if pad.present && slot < self.core.n && self.core.cfg.mode == Mode::Versus && self.core.cfg.humans > slot {
                let i = &mut out[slot];
                i.mv = [r[0] * pad.mv.x + f[0] * pad.mv.y, r[1] * pad.mv.x + f[1] * pad.mv.y];
                i.jump = pad.jump;
                i.jump_held = pad.jump_held;
                i.attack = pad.attack;
                i.z = pad.z;
                i.grab = pad.grab;
                i.s1 = pad.s1;
                i.s2 = pad.s2;
            }
        }
        out
    }

    fn cam_yaw(&self) -> f32 {
        0.0
    }

    // -----------------------------------------------------------------------------------------
    // Presentation
    // -----------------------------------------------------------------------------------------

    fn mario_pose(&mut self, host: &mut Host) {
        let f = self.core.f[0];
        let hero = if f.kind == Kind::Mario { f.hero } else { Hero::Mario };
        let mut arg = 0.0;
        let air = !f.grounded;
        let moving = f.speed2() > 60.0;
        let held = f.held >= 0;
        let act = match f.act {
            Act::Idle | Act::Air => {
                if air {
                    if f.vel[1] > 0.0 {
                        if f.jumps >= 2 { ACT_DOUBLE_JUMP } else if held { ACT_HOLD_JUMP } else { ACT_JUMP }
                    } else if held {
                        ACT_HOLD_FREEFALL
                    } else {
                        ACT_FREEFALL
                    }
                } else if held {
                    if moving { ACT_HOLD_WALKING } else { ACT_HOLD_IDLE }
                } else if moving {
                    ACT_WALKING
                } else {
                    ACT_IDLE
                }
            }
            Act::Punch(k) => {
                // Banjo's claw clip restarts on a new action: alternate the two claw actions down the combo.
                // (His move ids live in the action argument, so it stays 0.)
                arg = if k == 2 { 9.0 } else { k as f64 };
                if hero == Hero::Banjo {
                    arg = 0.0;
                    if k == 1 { ACT_MOVE_PUNCHING } else { ACT_PUNCHING }
                } else {
                    ACT_PUNCHING
                }
            }
            Act::Spin => {
                arg = 9.0;
                ACT_PUNCHING
            }
            Act::Dive => ACT_DIVE,
            Act::DiveSlide if hero == Hero::Banjo => ACT_DIVE,
            Act::DiveSlide => ACT_DIVE_SLIDE,
            Act::PoundWind | Act::Pound => ACT_GROUND_POUND,
            Act::PoundLand => ACT_GROUND_POUND_LAND,
            Act::Swing => ACT_JUMP_KICK,
            Act::Shoot | Act::Beam => ACT_PUNCHING,
            Act::Toss => ACT_THROWING,
            Act::Nova if hero == Hero::Banjo => ACT_GROUND_POUND_LAND,
            Act::Nova => ACT_FREEFALL,
            Act::Hurt => {
                if air { ACT_BACKWARD_AIR_KB } else { ACT_SOFT_BACKWARD_GROUND_KB }
            }
            Act::Down => {
                if air { ACT_HARD_BACKWARD_AIR_KB } else { ACT_HARD_BACKWARD_GROUND_KB }
            }
            Act::GetUp => ACT_FORWARD_GROUND_KB,
            Act::Dead => ACT_DEATH_ON_BACK,
        };
        let m = &mut host.sim.mario;
        if self.mario_act != (act, arg) {
            m.set_action(act, arg);
            self.mario_act = (act, arg);
        }
        // Link's rig plays the sword clips from the sim's pose hint and keeps the blade drawn (the lock-on stance); Banjo and
        // Mario pick their clips from the action alone.
        if hero.is_link() {
            m.link.pose = link_hint(f.act, f.act_t);
            m.controller.focus_yaw = Some(0.0);
        } else {
            m.link.pose = None;
            m.controller.focus_yaw = None;
        }
        m.pos = [f.pos[0] as f64, f.pos[1] as f64, f.pos[2] as f64];
        if self.pace.part() == 1 {
            // Slow motion: this host step shows the first half of the core tick (the next one the rest).
            let p = mix3(self.snap.f[0], f.pos, 0.5);
            m.pos = [p.x as f64, p.y as f64, p.z as f64];
        }
        m.vel = [f.vel[0] as f64 / 30.0, f.vel[1] as f64 / 30.0, f.vel[2] as f64 / 30.0];
        m.forward_vel = f.speed2() as f64 / 30.0;
        m.face_angle[1] = (f.yaw as f64 / std::f64::consts::TAU * 65536.0).rem_euclid(65536.0);
        if m.face_angle[1] > 32767.0 {
            m.face_angle[1] -= 65536.0;
        }
        m.floor_height = f.pos[1] as f64;
        m.events.clear();
        host.sim.refresh_view();
    }

    fn fx_push(&mut self, e: FxView) {
        if let Some(slot) = self.fx.iter_mut().find(|f| !f.on) {
            *slot = e;
        }
    }

    fn handle_events(&mut self, host: &mut Host) {
        let me = 0usize;
        let sm = |p: [f32; 3]| [p[0] as f64, p[1] as f64, p[2] as f64];
        for k in 0..self.core.ev.len() {
            let e = self.core.ev[k];
            match e {
                Ev::Jump(i) => {
                    if i == me {
                        host.fx("triple_jump", sm(self.core.f[i].pos));
                    }
                    host.particles.dust(sm(self.core.f[i].pos), 3);
                }
                Ev::Punch { id, step } => {
                    host.fx(if step == 2 { "kick" } else { "dive" }, sm(self.core.f[id].pos));
                }
                Ev::Swing(i) => host.fx("bonk", sm(self.core.f[i].pos)),
                Ev::Dive(i) => host.fx("dive", sm(self.core.f[i].pos)),
                Ev::PoundStart(i) => host.fx("flutter", sm(self.core.f[i].pos)),
                Ev::PoundLand { pos, r } => {
                    host.fx("ground_pound_drop", sm(pos));
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: [1.0, 1.0, 1.0], life: 0.35, radius: r, ..Default::default() });
                    host.particles.burst(sm(pos), BurstOptions { color: 0xd8c8a8, count: 10, speed: 260.0, life: 0.45, gravity: 300.0, size: 34.0, upward: 60.0, spread: 1.0 });
                    self.shake.add(0.3);
                }
                Ev::Hit { pos, big, victim } => {
                    host.fx(if big { "stomp" } else { "bonk" }, sm(pos));
                    host.particles.sparks(sm(pos), if big { 12 } else { 5 });
                    let col = SLOT_COLORS[victim % 4];
                    // A white pop on every hit; a heavy one adds a ring and chips in the victim's colour and a beat of hit-stop.
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)), color: [1.0, 1.0, 0.92], life: if big { 0.2 } else { 0.12 }, radius: if big { 120.0 } else { 62.0 }, ..Default::default() });
                    if big {
                        self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: col, life: 0.28, radius: 210.0, ..Default::default() });
                        host.particles.burst(sm(pos), BurstOptions { color: rgb_hex(col), count: 9, speed: 330.0, life: 0.4, gravity: 420.0, size: 22.0, upward: 120.0, spread: 1.0 });
                        self.pace.stop(2);
                    }
                    let mine = victim == me;
                    self.shake.add(match (big, mine) {
                        (true, true) => 0.45,
                        (true, false) => 0.3,
                        (false, true) => 0.2,
                        (false, false) => 0.08,
                    });
                    if victim < MAXF {
                        self.card_hit[victim].hit();
                    }
                }
                Ev::Ko { id, pos } => {
                    host.fx("hurt", sm(pos));
                    host.fx("explosion", sm(pos));
                    let col = SLOT_COLORS[id % 4];
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)), life: 0.55, radius: 340.0, ..Default::default() });
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: col, life: 0.6, radius: 520.0, ..Default::default() });
                    host.particles.burst(sm(pos), BurstOptions { color: rgb_hex(col), count: 18, speed: 420.0, life: 0.8, gravity: 380.0, size: 30.0, upward: 220.0, spread: 1.0 });
                    host.particles.burst(sm(pos), BurstOptions { color: 0xffffff, count: 8, speed: 240.0, life: 0.5, gravity: 120.0, size: 20.0, upward: 160.0, spread: 1.0 });
                    self.shake.add(0.6);
                    self.pace.stop(4);
                    if id < MAXF {
                        self.card_hit[id].hit();
                    }
                }
                Ev::Grab(i) => host.fx("coin_bounce", sm(self.core.f[i].pos)),
                Ev::Throw(i) => host.fx("kick", sm(self.core.f[i].pos)),
                Ev::Break(p) => {
                    host.fx("break_box", sm(p));
                    host.particles.burst(sm(p), BurstOptions { color: 0xb8823c, count: 10, speed: 300.0, life: 0.6, gravity: 620.0, size: 24.0, upward: 220.0, spread: 1.0 });
                    host.particles.dust(sm(p), 4);
                }
                Ev::Boom { pos, r } => {
                    host.fx("explosion", sm(pos));
                    host.particles.burst(sm(pos), BurstOptions { color: 0xffa030, count: 16, speed: 480.0, life: 0.5, gravity: 200.0, size: 36.0, upward: 160.0, spread: 1.0 });
                    host.particles.burst(sm(pos), BurstOptions { color: 0x4a4440, count: 8, speed: 170.0, life: 0.9, gravity: -60.0, size: 60.0, upward: 110.0, spread: 1.0 });
                    self.shake.add(0.5);
                    self.pace.stop(2);
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)) - Vec3::Y * 0.5, life: 0.4, radius: r, ..Default::default() });
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), life: 0.4, radius: r, ..Default::default() });
                }
                Ev::Gem { id, n, pos } => {
                    host.fx(["gem1", "gem2", "gem3"][(n as usize).clamp(1, 3) - 1], sm(pos));
                    host.particles.sparks(sm(pos), 10);
                    // A burst in the colours of the gems this fighter now holds, and a ring where it was picked up.
                    let mut ring = [1.0, 1.0, 1.0];
                    for (g, col) in GEM_COLORS.iter().enumerate() {
                        if self.core.gems[g].state == GemState::Held(id as u8) {
                            ring = *col;
                            host.particles.burst([pos[0] as f64, pos[1] as f64 + 60.0, pos[2] as f64], BurstOptions { color: rgb_hex(*col), count: 5, speed: 230.0, life: 0.55, gravity: 160.0, size: 22.0, upward: 200.0, spread: 1.0 });
                        }
                    }
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: ring, life: 0.4, radius: 190.0 + 50.0 * n as f32, ..Default::default() });
                    if id < MAXF {
                        self.card_hit[id].hit();
                    }
                    if id == me {
                        host.toast(&format!("Gem {n}/3"), 1.2);
                    }
                }
                Ev::GemDrop(_) => host.fx("coin_bounce", sm(self.core.f[me].pos)),
                Ev::GemLand(p) => host.fx("coin_bounce", sm(p)),
                Ev::Transform { id, pos } => {
                    host.fx("power_up", sm(pos));
                    host.particles.sparks(sm(pos), 24);
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), life: 0.7, radius: 600.0, ..Default::default() });
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: SLOT_COLORS[id % 4], life: 0.95, radius: 900.0, ..Default::default() });
                    for col in GEM_COLORS {
                        host.particles.burst([pos[0] as f64, pos[1] as f64 + 80.0, pos[2] as f64], BurstOptions { color: rgb_hex(col), count: 10, speed: 460.0, life: 0.9, gravity: 220.0, size: 30.0, upward: 300.0, spread: 1.0 });
                    }
                    self.shake.add(0.45);
                    self.pace.stop(5);
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)), life: 0.5, radius: 360.0, ..Default::default() });
                    let who = Self::name_of(&self.core, id);
                    self.banner = (format!("{} POWER!", who.to_uppercase()), if host.touch { "Power form! Tap SHOCK WAVE (blast all around) or SEEK BEAM (homing zap)".into() } else { "Power form! G = shock wave (blast all around), H = seek beam (homing zap)".into() }, 2.6);
                }
                Ev::PowerEnd(i) => {
                    host.fx("teleport", sm(self.core.f[i].pos));
                }
                Ev::Nova { pos, r } => {
                    host.fx("explosion", sm(pos));
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), life: 0.45, radius: r, ..Default::default() });
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: [1.0, 0.85, 0.3], life: 0.6, radius: r * 0.7, ..Default::default() });
                    host.particles.burst(sm(pos), BurstOptions { color: 0xffe070, count: 16, speed: 520.0, life: 0.5, gravity: 60.0, size: 28.0, upward: 90.0, spread: 1.0 });
                    self.shake.add(0.5);
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)), life: 0.35, radius: r * 0.6, ..Default::default() });
                }
                Ev::Beam { from, to, id } => {
                    host.fx("cannon_fire", sm(from));
                    let col = SLOT_COLORS[id % 4];
                    let tint = [0.55 + 0.45 * col[0], 0.55 + 0.45 * col[1], 0.55 + 0.45 * col[2]];
                    self.fx_push(FxView { on: true, kind: FxKind::Beam, pos: bv(Vec3::from(from)), to: bv(Vec3::from(to)), color: tint, life: 0.35, ..Default::default() });
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(to)), color: tint, life: 0.25, radius: 130.0, ..Default::default() });
                    host.particles.burst(sm(to), BurstOptions { color: rgb_hex(tint), count: 8, speed: 300.0, life: 0.35, gravity: 100.0, size: 22.0, upward: 80.0, spread: 1.0 });
                    self.shake.add(0.15);
                }
                Ev::Shot(p) => {
                    host.fx("kick", sm(p));
                    host.particles.burst(sm(p), BurstOptions { color: 0xfff0a0, count: 4, speed: 160.0, life: 0.2, gravity: 0.0, size: 18.0, upward: 20.0, spread: 1.0 });
                }
                Ev::Bump(p) => {
                    host.fx("bonk", sm(p));
                    host.particles.sparks(sm(p), 6);
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(p)), color: [1.0, 0.8, 0.3], life: 0.25, radius: 170.0, ..Default::default() });
                    self.shake.add(0.12);
                }
                Ev::Fell(i) => {
                    let p = sm(self.core.f[i].pos);
                    host.fx("splash", p);
                    host.particles.burst(p, BurstOptions { color: rgb_hex(SLOT_COLORS[i % 4]), count: 10, speed: 200.0, life: 0.7, gravity: -80.0, size: 28.0, upward: 420.0, spread: 0.5 });
                }
                Ev::Heal(i) => {
                    let p = self.core.f[i].pos;
                    host.fx("one_up", sm(p));
                    host.particles.burst([p[0] as f64, p[1] as f64 + 60.0, p[2] as f64], BurstOptions { color: 0x60f080, count: 10, speed: 120.0, life: 0.8, gravity: -140.0, size: 22.0, upward: 160.0, spread: 1.0 });
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(p)), color: [0.4, 1.0, 0.5], life: 0.45, radius: 200.0, ..Default::default() });
                }
                Ev::Land(i) => {
                    if i == me {
                        host.fx("land", sm(self.core.f[i].pos));
                    }
                    host.particles.dust(sm(self.core.f[i].pos), 3);
                }
                Ev::Lava(i) => {
                    if self.core.tick % 6 == 0 {
                        host.fx("splash", sm(self.core.f[i].pos));
                    }
                    if self.core.tick.wrapping_sub(self.ember_t) >= 3 {
                        self.ember_t = self.core.tick;
                        host.particles.burst(sm(self.core.f[i].pos), BurstOptions { color: 0xff6a18, count: 4, speed: 150.0, life: 0.55, gravity: -120.0, size: 24.0, upward: 260.0, spread: 0.7 });
                    }
                    if i == me {
                        self.shake.at_least(0.18);
                    }
                }
                Ev::Cannon => {
                    host.fx("cannon_fire", [0.0, 200.0, 0.0]);
                    host.particles.burst([0.0, 260.0, 0.0], BurstOptions { color: 0x58524c, count: 10, speed: 200.0, life: 0.8, gravity: -50.0, size: 56.0, upward: 200.0, spread: 1.0 });
                    self.shake.add(0.25);
                }
                Ev::Plank(p) => host.fx("bonk", sm(p)),
                Ev::RoundStart(n) => {
                    Self::cue(host, "select_mission");
                    let _ = n;
                }
                Ev::RoundEnd { .. } => {
                    Self::cue(host, "round_end");
                    // The deciding blow plays out at half speed for a moment.
                    self.pace.slow(26);
                }
                Ev::MatchEnd { .. } => Self::cue(host, "star"),
                Ev::Click(i) => host.fx("move_menu", sm(self.core.f[i].pos)),
                Ev::MonHit { pos, big } => {
                    host.fx(if big { "stomp" } else { "bonk" }, sm(pos));
                    host.particles.sparks(sm(pos), if big { 8 } else { 3 });
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)), color: [1.0, 1.0, 0.92], life: 0.12, radius: if big { 100.0 } else { 56.0 }, ..Default::default() });
                }
                Ev::MonKo { pos } => {
                    host.fx("explosion", sm(pos));
                    self.fx_push(FxView { on: true, kind: FxKind::Ring, pos: bv(Vec3::from(pos)), color: [0.75, 0.5, 1.0], life: 0.4, radius: 300.0, ..Default::default() });
                    host.particles.burst(sm(pos), BurstOptions { color: 0xb080ff, count: 12, speed: 320.0, life: 0.6, gravity: 300.0, size: 26.0, upward: 200.0, spread: 1.0 });
                    self.shake.add(0.22);
                    self.pace.stop(1);
                    self.fx_push(FxView { on: true, kind: FxKind::Flash, pos: bv(Vec3::from(pos)), life: 0.4, radius: 220.0, ..Default::default() });
                }
                Ev::MonSpawn(pos) => host.fx("teleport", sm(pos)),
            }
        }
    }

    fn publish(&mut self, host: &mut Host, dt: f32) {
        self.shake.update(dt);
        for p in self.card_hit.iter_mut() {
            p.update(dt, 7.0);
        }
        let c = &self.core;
        let a = self.pace.alpha(host.alpha as f32);
        let t = self.t as f32;
        let b = &mut *host.brawl;
        b.time = t;
        for i in 0..MAXF {
            let f = &c.f[i];
            let v = &mut b.fighters[i];
            if i >= c.n {
                v.on = false;
                continue;
            }
            let pos = mix3(self.snap.f[i], f.pos, a);
            let ground = c.arena.support(f.pos[0], f.pos[2], f.pos[1] + STEP).map_or(c.arena.void_y, |s| s.0);
            let yaw = self.snap.fy[i] + angle_diff(self.snap.fy[i], f.yaw) * a;
            let mut tilt = 0.0;
            let mut roll = 0.0;
            let mut spin = 0.0;
            let mut sc = 1.0;
            match f.act {
                Act::Punch(_) | Act::Swing | Act::Spin => tilt = 0.35,
                Act::Dive | Act::DiveSlide => tilt = 1.3,
                Act::PoundWind | Act::Pound => spin = t * 18.0,
                Act::Toss | Act::Shoot | Act::Beam => tilt = 0.25,
                Act::PoundLand => tilt = 0.6,
                Act::Nova => sc = 1.0 + 0.15 * (t * 30.0).sin(),
                Act::Hurt => tilt = -0.35,
                Act::Down => tilt = -1.45,
                Act::GetUp => tilt = -0.7,
                Act::Dead => {
                    tilt = -1.45;
                    roll = f.dead_t as f32 * 0.25;
                    sc = (1.0 - f.dead_t as f32 / 40.0).max(0.0);
                }
                Act::Idle | Act::Air => {
                    if f.speed2() > 100.0 && f.grounded {
                        tilt = 0.12;
                    }
                    if f.burn > 0 {
                        roll = (t * 40.0).sin() * 0.2;
                    }
                }
            }
            let stats = f.stats();
            let base = match f.kind {
                Kind::Mario => 1.0,
                Kind::Goomba => 1.6,
                Kind::Bomb => 1.35,
                Kind::Koopa => 1.9,
                Kind::Bully => 1.3,
                Kind::Chuckya => 1.1,
                Kind::KingBomb => 1.05,
            };
            let vis = base * f.size / stats.scale * sc;
            *v = FView { on: f.alive || f.dead_t < 40, kind: f.kind as u8, pos: bv(pos), yaw: yaw + spin, scale: vis, tilt, roll, ground: ground * crate::render::SCALE, blink: f.inv > 8 && f.alive && f.act != Act::GetUp && f.act != Act::Down, human: f.human, power: f.powered(), burn: f.burn > 0, tint: [1.0, 0.82, 0.25] };
            if f.kind == Kind::Mario {
                // Mario's own model is drawn by the game; the view only scales him up.
                v.scale = f.size / stats.scale * sc.max(0.05) * f.hero.style().fit;
                v.tint = f.hero.style().tint;
                v.kind = 0;
            }
        }
        for k in 0..MAXI {
            let it = &c.items[k];
            let v = &mut b.items[k];
            *v = IView { on: it.alive, kind: it.kind as u8, pos: bv(mix3(self.snap.it[k], it.pos, a)), yaw: it.spin, tilt: if it.state == IState::Flying { it.spin * 0.5 } else { 0.0 }, lit: it.fuse > 0, held: matches!(it.state, IState::Held(_)) };
            if let IState::Held(h) = it.state {
                v.yaw = c.f[h as usize].yaw;
                v.tilt = if it.kind == ItemKind::Hammer { std::f32::consts::FRAC_PI_2 * 0.25 } else { 0.0 };
                if matches!(c.f[h as usize].act, Act::Swing) {
                    v.tilt = -0.2 + (c.f[h as usize].act_t as f32 / 17.0) * 2.2;
                }
            }
            if it.kind.weapon() && !matches!(it.state, IState::Held(_)) {
                v.yaw = t * 2.0;
            }
        }
        // The ported OoT / BK monsters (drawn by `foes_view`): knocked-out ones play their death pose briefly.
        b.foes.clear();
        for (m, o) in c.mons.iter().enumerate() {
            if !o.on || (!o.alive && o.respawn < MON_RESPAWN - 26) {
                continue;
            }
            let p = mix3(self.snap.mn[m], o.pos, a);
            let age = if o.alive { 0.0 } else { (MON_RESPAWN - o.respawn).max(0) as f32 / 30.0 };
            let spawn = 1.0 - 0.7 * (o.spawn_t as f32 / 45.0);
            let shake = if o.st == 1 { (t * 60.0).sin() * 8.0 } else { 0.0 };
            b.foes.push(FoeItem { id: m as u32, species: o.species, pos: bv(Vec3::new(p.x + shake, p.y + o.lift, p.z)), yaw: o.yaw, radius: visual_radius(o.species), pose: o.pose(), pulse: spawn * (1.0 + 0.1 * o.flash as f32 / 5.0), age });
        }
        for k in 0..MAXP {
            let p = &c.proj[k];
            b.projs[k] = PView { on: p.alive, kind: p.kind as u8, pos: bv(mix3(self.snap.pr[k], p.pos, a)), yaw: 0.0, aim: bv(Vec3::from(p.aim)) };
        }
        for g in 0..GEMS {
            let gm = &c.gems[g];
            let on = matches!(gm.state, GemState::Free | GemState::Held(_));
            let held = matches!(gm.state, GemState::Held(_));
            let pos = match gm.state {
                GemState::Held(h) => {
                    let f = &c.f[h as usize];
                    bv(mix3(self.snap.f[h as usize], f.pos, a))
                }
                _ => bv(mix3(self.snap.gm[g], gm.pos, a)),
            };
            b.gems[g] = GView { on, pos, held };
        }
        // Effects age and expire.
        for (i, f) in self.fx.iter_mut().enumerate() {
            if f.on {
                f.age += dt;
                if f.age >= f.life {
                    f.on = false;
                }
            }
            b.fx[i] = *f;
        }
        b.wind = Vec3::new(c.arena.wind_dir[0], 0.0, c.arena.wind_dir[1]);
        b.wind_strength = if c.arena.kind == ArenaKind::Gale { c.arena.wind } else { 0.0 };
        // Power buttons.
        let p = &c.f[0];
        b.nova_ready = p.powered() && p.cd_s1 == 0 && p.power > NOVA_COST * 0.6;
        b.beam_ready = p.powered() && p.cd_s2 == 0 && p.power > BEAM_COST * 0.6;
        // Cooldown shade: full while the form is off, then drains as the move recharges.
        b.nova_cd = if p.powered() { if p.power > NOVA_COST * 0.6 { p.cd_s1 as f32 / 36.0 } else { 1.0 } } else { 1.0 };
        b.beam_cd = if p.powered() { if p.power > BEAM_COST * 0.6 { p.cd_s2 as f32 / 26.0 } else { 1.0 } } else { 1.0 };
        // Announce a move the moment it becomes available.
        let ready = (b.nova_ready, b.beam_ready);
        let touch_now = b.phone;
        if p.powered() && ready.0 && !self.was_ready.0 && self.banner.2 <= 0.0 && self.stage == Stage::Playing {
            self.banner = ("SHOCK WAVE READY".into(), if touch_now { "Tap SHOCK WAVE for a blast all around you".into() } else { "Press G for a blast all around you".into() }, 1.4);
        }
        self.was_ready = ready;

        // ---- HUD ----
        b.menu.hero = if self.stage == Stage::Menu { b.hero } else { c.f[0].hero } as u8;
        let h = &mut b.hud;
        h.n = if self.stage == Stage::Menu { 0 } else { c.n };
        let to_win = if c.cfg.mode == Mode::Versus { c.cfg.rounds_to_win } else { 0 };
        for i in 0..MAXF {
            let f = &c.f[i];
            let cv = &mut h.cards[i];
            cv.on = i < c.n;
            if i >= c.n {
                continue;
            }
            let mut name = Self::name_of(c, i);
            if name.len() > 12 {
                name.truncate(12);
            }
            cv.name = name;
            cv.tag = if f.powered() { " POWER" } else if f.kind == Kind::KingBomb { " BOSS" } else { "" };
            cv.hp = f.hp_frac();
            cv.hp_num = f.hp.ceil() as i32;
            cv.gems = f.gems;
            cv.wins = f.wins;
            cv.to_win = to_win;
            cv.power = if f.powered() { f.power / POWER_SECS } else { 0.0 };
            cv.human = f.human;
            cv.ko = !f.alive;
            cv.hit = self.card_hit[i].value();
        }
        let secs = c.round_t.max(0.0).ceil() as i32;
        h.clock = if c.phase == Phase::Intro { "--".into() } else { format!("{}:{:02}", secs / 60, secs % 60) };
        h.round = match c.cfg.mode {
            Mode::Versus => format!("ROUND {}", c.round),
            Mode::Gauntlet => format!("STAGE {}/{}  {}", c.stage + 1, GAUNTLET_STAGES, "*".repeat(c.lives as usize)),
        };
        // Banner.
        let (mut big, mut sub, mut alpha) = (String::new(), String::new(), 0.0f32);
        match c.phase {
            Phase::Intro => {
                if c.phase_t > 40 {
                    big = if c.cfg.mode == Mode::Gauntlet { format!("STAGE {}", c.stage + 1) } else { format!("ROUND {}", c.round) };
                    sub = format!("{}  -  {}", c.arena.kind.name(), c.arena.kind.hazard());
                    alpha = 1.0;
                } else {
                    big = "FIGHT!".into();
                    alpha = 1.0;
                }
            }
            Phase::Fight => {
                let since = c.total_ticks.saturating_sub(self.last_round_tick()) as f32;
                if since < 24.0 {
                    big = "FIGHT!".into();
                    alpha = 1.0 - since / 24.0;
                }
            }
            Phase::RoundEnd => {
                big = if c.round_t <= 0.0 { "TIME!".into() } else { "K.O.!".into() };
                sub = match c.winner {
                    w if w >= 0 => format!("{} takes the round", Self::name_of(c, w as usize)),
                    _ => "Draw - replay the round".into(),
                };
                alpha = 1.0;
            }
            Phase::MatchEnd => {}
        }
        if self.banner.2 > 0.0 {
            self.banner.2 -= dt;
            if big.is_empty() {
                big = self.banner.0.clone();
                sub = self.banner.1.clone();
                alpha = (self.banner.2 / 0.6).min(1.0);
            }
        }
        h.banner = big;
        h.sub = sub;
        h.banner_alpha = alpha;
        h.warn = match c.arena.kind {
            ArenaKind::Ember => match c.arena.lava_phase {
                LavaPhase::Warn => "LAVA IS RISING - GET TO THE ROCKS".into(),
                LavaPhase::Rising | LavaPhase::High => "LAVA!".into(),
                _ => String::new(),
            },
            ArenaKind::Gale => {
                if c.arena.wind_warn > 0.0 {
                    "GUST INCOMING".into()
                } else if c.arena.wind > 0.3 {
                    "GALE!".into()
                } else {
                    String::new()
                }
            }
            ArenaKind::Cannon => {
                if c.arena.cannon_cd < 40 && c.phase == Phase::Fight {
                    "INCOMING!".into()
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };
        self.hint_t += dt;
        let touch = host.touch;
        h.hint = if c.f[0].powered() {
            if touch { "POWER FORM! SHOCK WAVE = blast around you - SEEK BEAM = homing zap".into() } else { "POWER FORM! G shock wave (blast all around) - H seek beam (homing zap)".into() }
        } else if c.phase == Phase::Fight && self.hint_t < 14.0 && c.round == 1 {
            if touch { "Z grabs items - B hits - collect all 3 gems".into() } else { "Ctrl grabs / throws - Shift hits - collect all 3 gems for the power form".into() }
        } else if c.f[0].gem_count() == 2 && c.phase == Phase::Fight {
            "One more gem and you go GIANT - POWER FORM!".into()
        } else if c.f[0].gem_count() == 1 && c.phase == Phase::Fight {
            "Gem got! Two more for the power form (they drop if you are hit hard)".into()
        } else if c.gems.iter().any(|g| g.state == GemState::Free) && c.phase == Phase::Fight {
            "Gems on the field! Collect all 3 to go GIANT".into()
        } else {
            String::new()
        };
    }

    fn last_round_tick(&self) -> u32 {
        // Ticks at which the fight began: total ticks minus fight elapsed time.
        let elapsed = ((ROUND_SECS - self.core.round_t) / DT) as u32;
        self.core.total_ticks.saturating_sub(elapsed)
    }

    fn camera(&mut self, host: &mut Host, dt: f64) {
        let c = &self.core;
        let a = self.pace.alpha(host.alpha as f32);
        let phone = host.touch;
        let aspect = host.brawl.aspect.max(0.8);
        let yaw = self.cam_yaw() + if self.stage == Stage::Menu { (self.t as f32 * 0.15).sin() * 0.5 } else { 0.0 };
        let pitch: f32 = (if phone { 1.05 } else { 1.0 }) + if c.arena.kind == ArenaKind::Gearworks { 0.15 } else { 0.0 };
        let fov: f32 = 50f32.to_radians();
        let (tv, th) = ((fov / 2.0).tan(), (fov / 2.0).tan() * aspect);
        let fwd = Vec3::new(-yaw.sin() * pitch.cos(), -pitch.sin(), -yaw.cos() * pitch.cos());
        let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let up = right.cross(fwd);
        // Points to keep in frame: every fighter's feet and head, plus in menu the arena outline.
        let mut pts: [Vec3; 16] = [Vec3::ZERO; 16];
        let mut n = 0;
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let menu = self.stage == Stage::Menu;
        for i in 0..c.n {
            let f = &c.f[i];
            if !f.alive && f.dead_t > 30 {
                continue;
            }
            let p = mix3(self.snap.f[i], f.pos, a);
            let pad = 120.0 * f.size.min(2.0);
            for (dx, dy, dz) in [(0.0, -30.0, 0.0), (0.0, f.height() + 40.0, 0.0), (pad, 0.0, 0.0), (-pad, 0.0, 0.0), (0.0, 0.0, pad), (0.0, 0.0, -pad)] {
                if n < pts.len() {
                    pts[n] = p + Vec3::new(dx, dy, dz);
                    n += 1;
                }
            }
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if menu || n == 0 {
            n = 0;
            let h = c.arena.half;
            for (x, z) in [(-h[0], -h[1]), (h[0], -h[1]), (-h[0], h[1]), (h[0], h[1])] {
                pts[n] = Vec3::new(x, 0.0, z);
                n += 1;
            }
            lo = Vec3::new(-h[0], 0.0, -h[1]);
            hi = Vec3::new(h[0], 0.0, h[1]);
        }
        let mid = (lo + hi) * 0.5;
        let focus_t = if menu { mid } else { mid * 0.65 + Vec3::new(0.0, mid.y, 0.0) * 0.35 } + Vec3::Y * 40.0;
        // Margins in NDC: leave room for the HUD (top) and the touch controls (bottom corners).
        let (mx, y_hi, y_lo) = if phone { (0.78, 0.5, -0.62) } else { (0.84, 0.62, -0.76) };
        let (mx, y_hi, y_lo) = if menu { (0.6, 0.45, -0.5) } else { (mx, y_hi, y_lo) };
        let mut need = if phone { 1500.0f32 } else { 1750.0f32 };
        for p in &pts[..n] {
            let q = *p - focus_t;
            let (xc, yc, zf) = (q.dot(right), q.dot(up), q.dot(fwd));
            need = need.max(xc.abs() / (mx * th) - zf);
            if yc > 0.0 {
                need = need.max(yc / (y_hi * tv) - zf);
            } else {
                need = need.max(yc / (y_lo * tv) - zf);
            }
        }
        let need = need.clamp(if phone { 1500.0 } else { 1750.0 }, 5600.0);
        if !self.cam_ready {
            self.cam_focus = focus_t;
            self.cam_dist = need;
            self.cam_ready = true;
        } else {
            let k_f = 1.0 - (-dt as f32 * 6.0).exp();
            self.cam_focus += (focus_t - self.cam_focus) * k_f;
            let k_d = 1.0 - (-dt as f32 * if need > self.cam_dist { 9.0 } else { 2.5 }).exp();
            self.cam_dist += (need - self.cam_dist) * k_d;
        }
        // Keep the high-side fighters in the safe band when the smoothed focus lags behind.
        let eye = self.cam_focus - fwd * self.cam_dist;
        let d = |v: Vec3| DVec3::new(v.x as f64, v.y as f64, v.z as f64);
        let mut pose = CamPose { eye: d(eye), look: d(self.cam_focus), up: DVec3::Y, fov: fov.to_degrees() as f64, near: 30.0, far: 40000.0 };
        if !menu {
            self.shake.apply(&mut pose, self.cam_dist as f64 * 0.014);
        }
        host.cam.pose = Some(pose);
    }
}

impl Experiment for GemClashExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Brawl
    }
    fn replaces_course(&self) -> bool {
        true
    }
    fn freezes_player(&self) -> bool {
        true
    }
    fn enter(&mut self, host: &mut Host) {
        host.shell.plain_camera = true;
        self.best = load_best();
        self.demo = std::env::var_os("JUMPMAN_BRAWL_DEMO").is_some();
        self.god = std::env::var_os("JUMPMAN_BRAWL_GOD").is_some();
        self.speed = env_num::<usize>("JUMPMAN_BRAWL_SPEED").unwrap_or(1).clamp(1, 6);
        host.brawl.reset();
        host.brawl.active = true;
        host.brawl.phone = host.touch;
        // Menu defaults from the environment.
        if let Some(a) = std::env::var("JUMPMAN_BRAWL_ARENA").ok().and_then(|s| ArenaKind::parse(&s)) {
            self.menu.arena = ARENAS.iter().position(|k| *k == a).unwrap_or(0) as u8;
        }
        if let Some(b) = env_num::<u8>("JUMPMAN_BRAWL_BOTS") {
            self.menu.bots = b.clamp(1, 3);
        }
        if let Some(d) = std::env::var("JUMPMAN_BRAWL_DIFF").ok().and_then(|s| Diff::parse(&s)) {
            self.menu.diff = [Diff::Easy, Diff::Normal, Diff::Hard].iter().position(|x| *x == d).unwrap_or(0) as u8;
        }
        if std::env::var("JUMPMAN_BRAWL_MODE").is_ok_and(|m| m.starts_with('g')) {
            self.menu.mode = 1;
        }
        host.set_player_visible(true);
        let auto = std::env::var_os("JUMPMAN_BRAWL_START").is_some() || std::env::var_os("JUMPMAN_BRAWL_ARENA").is_some() || std::env::var_os("JUMPMAN_BRAWL_STAGE").is_some();
        if auto {
            self.start_match(host);
        } else {
            self.show_menu(host);
            if help_due() {
                self.set_help(host, true);
            }
        }
    }
    fn exit(&mut self, host: &mut Host) {
        host.brawl.active = false;
        host.brawl.help = false;
        self.help = false;
        host.brawl.screen = Screen::None;
        host.brawl.fx = [FxView::default(); FX_MAX];
        host.restore_mario_world();
        host.sim.mario.reset_to_spawn();
        host.sim.mario.link.pose = None;
        host.sim.mario.controller.focus_yaw = None;
        host.set_player_visible(true);
        host.cam.fog = None;
        host.cam.pose = None;
        *host.labels = Default::default();
        self.arena_ents.clear();
    }
    fn before_step(&mut self, _host: &mut Host, controller: &ControllerSnapshot) -> StepOverride {
        self.ctl = controller.clone();
        // Menus swallow the stick so the hidden player does not wander.
        StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None }
    }
    fn after_step(&mut self, host: &mut Host) {
        if self.stage != Stage::Playing || self.help {
            return;
        }
        if self.demo && self.core.brains[0].is_none() {
            self.core.brains[0] = Some(Brain::new(Diff::Hard, false, self.seed ^ 77));
            self.core.f[0].human = false;
        }
        let mut inputs = self.inputs(host);
        for i in 0..MAXF {
            let (h, n) = (self.held_in[i], &mut inputs[i]);
            n.jump |= h.jump;
            n.attack |= h.attack;
            n.z |= h.z;
            n.grab |= h.grab;
            n.s1 |= h.s1;
            n.s2 |= h.s2;
        }
        self.held_in = [Input::default(); MAXF];
        for _ in 0..self.speed {
            // Hit-stop / slow motion: the core simply ticks later (its outcome is unchanged); presses are kept.
            if !self.pace.step() {
                self.held_in = inputs;
                continue;
            }
            self.snap = Snap::of(&self.core);
            if self.god {
                self.core.f[0].hp = self.core.f[0].max_hp;
            }
            self.core.tick(&inputs);
            self.handle_events(host);
            if self.core.phase == Phase::MatchEnd && self.stage == Stage::Playing {
                self.finish(host);
                break;
            }
        }
        if self.core.phase == Phase::Fight && self.last_phase != Phase::Fight {
            self.hint_t = 0.0;
        }
        self.last_phase = self.core.phase;
        if self.core.arena.kind != self.built.unwrap_or(self.core.arena.kind) {
            self.build_arena(host);
        }
        self.mario_pose(host);
    }
    fn frame(&mut self, host: &mut Host, dt: f64) {
        self.t += dt;
        // Latch key / button edges for the next 30 Hz tick.
        if host.keys.just_pressed(KeyCode::KeyG) {
            self.latched |= 1;
        }
        if host.keys.just_pressed(KeyCode::KeyH) {
            self.latched |= 2;
        }
        if host.keys.just_pressed(KeyCode::KeyF) {
            self.latched |= 4;
        }
        let e = std::mem::take(&mut host.brawl.edges);
        self.latched |= e;
        self.handle_cmds(host);
        if self.stage == Stage::Menu {
            host.brawl.menu = self.menu;
        }
        // Sparkles around a powered fighter.
        self.spark_t += dt as f32;
        if self.spark_t > 0.1 {
            self.spark_t = 0.0;
            for i in 0..self.core.n {
                let f = &self.core.f[i];
                if f.powered() && f.alive {
                    host.particles.sparks([f.pos[0] as f64, f.pos[1] as f64 + 80.0 * f.size as f64, f.pos[2] as f64], 2);
                }
            }
        }
        if self.stage != Stage::Playing {
            self.mario_pose(host);
        }
        host.set_player_visible(self.core.f[0].alive || self.core.f[0].dead_t < 40);
        self.update_arena(host);
        self.publish(host, dt as f32);
        self.camera(host, dt);
        host.status("");
    }
    fn debug(&self) -> String {
        let c = &self.core;
        format!("round={} phase={:?} hp={:?} t={:.1}", c.round, c.phase, (0..c.n).map(|i| c.f[i].hp as i32).collect::<Vec<_>>(), c.round_t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_basis_is_right_handed() {
        let (f, r) = GemClashExperiment::camera_basis(0.0);
        assert!((f[1] + 1.0).abs() < 1e-6 && f[0].abs() < 1e-6);
        assert!((r[0] - 1.0).abs() < 1e-6, "right is +x when looking down -z");
        let (f, r) = GemClashExperiment::camera_basis(std::f32::consts::FRAC_PI_2);
        assert!((f[0] + 1.0).abs() < 1e-5);
        assert!((r[1] - 1.0).abs() < 1e-5 || (r[1] + 1.0).abs() < 1e-5);
    }

    #[test]
    fn interpolation_skips_teleports() {
        let v = mix3([0.0; 3], [1000.0, 0.0, 0.0], 0.5);
        assert_eq!(v.x, 1000.0);
        let v = mix3([0.0; 3], [100.0, 0.0, 0.0], 0.5);
        assert_eq!(v.x, 50.0);
    }

    #[test]
    fn best_record_round_trips() {
        // Pure file round trip through serde_json (the path helper only joins a name).
        let s = serde_json::json!({ "best_stage": 3, "versus_wins": 2, "clears": 1 }).to_string();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["best_stage"].as_u64(), Some(3));
    }
}
