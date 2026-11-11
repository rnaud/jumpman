//! Ocarina of Time Link as a full playable controller: this plugin wires the sim-side faithful
//! controller (`jumpman_sim::link`) into the app. It
//!
//! * turns the controller on for Link / Young Link and off for everybody else (`sync_enabled`),
//! * reads the C buttons (keyboard 1-2-3 or U-I-O, gamepad D-pad, touch buttons from `ui.rs`) and
//!   hands them to the sim in the free `ControllerSnapshot` bits, with the lock-on toggle,
//! * steps the projectiles after the main tick and applies hits to goombas, coins and the
//!   object arena, turns sim events into sounds, toasts, warps and effects,
//! * draws the projectiles / spells / reticle (`view.rs`) and the hearts, magic, C buttons, item
//!   screen and ocarina staff (`ui.rs`),
//! * provides the Z-target camera inputs (`zcam.rs`).
//!
//! Controls: C-left / C-down / C-right = `1 2 3` or `U I O` or D-pad left / down / right; C-up =
//! `7` or `Y` (opens the ocarina); items screen = `Tab` / `N` / D-pad up / the ITEMS touch button.
use crate::characters::Character;
use crate::experiments::{ExperimentSlot, StepControl};
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::objects::goombas::Goombas;
use crate::sim_plugin::{ModalActive, PostTickSet, PreTickSet};
use bevy::prelude::*;
use jumpman_sim::link::items::Age;
use jumpman_sim::link::moves::{LinkEvent, LinkExt, C_DOWN, C_LEFT, C_RIGHT, C_UP, EXTRA_TARGET_BASE};
use jumpman_sim::link::ocarina::SongEffect;
use jumpman_sim::link::projectiles::{Target, TargetKind};

pub mod aimcam;
pub mod combat_fx;
pub mod demo;
pub mod epona_hud;
pub mod epona_view;
pub mod items3d;
pub mod prop_world;
#[cfg(test)]
mod epona_scenarios;
#[cfg(test)]
mod epona_view_tests;
#[cfg(test)]
mod mario_worlds;
#[cfg(test)]
mod parity_tests;
#[cfg(test)]
mod sword_tests;
pub mod icons;
pub mod ui;
pub mod view;
pub mod zcam;

pub const PICKUP_TARGET_BASE: u32 = EXTRA_TARGET_BASE + 500_000;

pub struct LinkPlugin;

impl Plugin for LinkPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LinkSettings::load())
            .init_resource::<LinkInput>()
            .init_resource::<LockOn>()
            .init_resource::<WorldFx>()
            .add_plugins((prop_world::PropsPlugin, demo::DemoPlugin, aimcam::AimCamPlugin, combat_fx::CombatFxPlugin, items3d::ItemModelsPlugin, ui::LinkUiPlugin, view::LinkViewPlugin, zcam::ZTargetPlugin, epona_view::EponaViewPlugin, epona_hud::EponaHudPlugin))
            .add_systems(PreUpdate, read_link_keys.after(crate::input::read_input))
            .add_systems(RunFixedMainLoop, lock_toggle.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop))
            .add_systems(Update, sync_enabled.after(crate::characters::apply_character).before(crate::CameraSet))
            .add_systems(Update, epona_presence.after(sync_enabled).before(crate::CameraSet))
            .add_systems(FixedUpdate, pre_tick.in_set(PreTickSet).before(crate::zelda::zelda_step))
            .add_systems(FixedUpdate, post_tick.in_set(PostTickSet));
    }
}

/// Persistent Link options (the item screen's SETTINGS tab).
#[derive(Resource, Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LinkSettings {
    /// Z-targeting is a toggle (press to lock on, press again to release); false = hold L.
    pub toggle_lock: bool,
    /// A ridable Epona waits near the start of every course (off by default: she comes when called with her song).
    pub epona_follows: bool,
    /// Invert the vertical aim of the bow, slingshot and hookshot (stick and drag).
    pub invert_aim_y: bool,
}

impl Default for LinkSettings {
    fn default() -> Self {
        Self { toggle_lock: true, epona_follows: false, invert_aim_y: false }
    }
}

impl LinkSettings {
    fn path() -> std::path::PathBuf {
        crate::byor::data_dir().join("link.json")
    }
    pub fn load() -> Self {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let p = Self::path();
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, b);
        }
    }
}

/// C-button state gathered from keyboard / pad / touch for the next sim step.
#[derive(Resource, Default)]
pub struct LinkInput {
    /// Edges latched until the fixed step consumes them (N64 `CONT_C*` bits).
    pub c_pressed: u32,
    pub c_down: u32,
    pub menu_toggle: bool,
    /// C bits held by the on-screen buttons (written by `ui.rs`).
    pub touch_bits: u32,
    /// First-person aim: pixels dragged since the last tick and the right stick (stick units), see `aimcam.rs`.
    pub look_px: Vec2,
    pub look_stick: Vec2,
}

/// Z-target lock state (toggle or hold).
#[derive(Resource, Default)]
pub struct LockOn {
    pub locked: bool,
    raw_prev: bool,
    had_target: bool,
    /// Seconds the toggle lock has been on without ever finding a target.
    empty: f32,
    /// One press of the lock button from a script (`JUMPMAN_LINK_DEMO`'s `lock`).
    pub pulse: bool,
}

/// With nothing to lock, a toggled Z only holds while the button is down (and at least this long, so a
/// tap still swings the camera behind Link). Otherwise Link would stay in the strafing stance with
/// A turned into hops and backflips on courses with no opponent nearby.
pub const EMPTY_LOCK_GRACE: f32 = 0.35;

impl LockOn {
    /// One frame of the lock button. `raw` is the button, `has_target` whether the sim holds a target.
    /// Returns true when Z was pressed again on a target: the caller moves the lock to the next one
    /// and calls [`LockOn::release`] when there is none.
    pub fn step(&mut self, raw: bool, toggle: bool, has_target: bool, dt: f32) -> bool {
        let pressed = raw && !self.raw_prev;
        self.raw_prev = raw;
        if !toggle {
            self.locked = raw;
            self.had_target = false;
            self.empty = 0.0;
            return false;
        }
        let mut switch = false;
        if pressed {
            if !self.locked {
                self.locked = true;
                self.had_target = false;
                self.empty = 0.0;
            } else if has_target {
                switch = true;
            } else {
                self.release();
            }
        }
        if self.locked {
            if has_target {
                self.had_target = true;
            } else if self.had_target {
                // The locked target is gone (defeated / out of range).
                self.release();
            } else {
                self.empty += dt;
                if !raw && self.empty >= EMPTY_LOCK_GRACE {
                    self.release();
                }
            }
        }
        switch
    }

    pub fn release(&mut self) {
        self.locked = false;
        self.had_target = false;
        self.empty = 0.0;
    }
}

/// Link is the active character (not inside an experiment that takes the controls).
pub fn link_active(character: &Character, slot: &ExperimentSlot) -> bool {
    character.is_link() && slot.id.is_none()
}

pub fn age_of(c: Character) -> Age {
    if c == Character::YoungLink {
        Age::Child
    } else {
        Age::Adult
    }
}

fn read_link_keys(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut li: ResMut<LinkInput>) {
    let pad = pads.iter().next();
    let key = |ks: &[KeyCode]| ks.iter().any(|k| keys.pressed(*k));
    let key_p = |ks: &[KeyCode]| ks.iter().any(|k| keys.just_pressed(*k));
    let pad_d = |b: GamepadButton| pad.is_some_and(|p| p.pressed(b));
    let pad_p = |b: GamepadButton| pad.is_some_and(|p| p.just_pressed(b));
    let table = [
        (C_LEFT, &[KeyCode::Digit1, KeyCode::KeyU][..], GamepadButton::DPadLeft),
        (C_DOWN, &[KeyCode::Digit2, KeyCode::KeyI][..], GamepadButton::DPadDown),
        (C_RIGHT, &[KeyCode::Digit3, KeyCode::KeyO][..], GamepadButton::DPadRight),
        (C_UP, &[KeyCode::Digit7, KeyCode::KeyY][..], GamepadButton::RightThumb),
    ];
    let mut down = 0;
    for (bit, ks, pb) in table {
        if key(ks) || pad_d(pb) {
            down |= bit;
        }
        if key_p(ks) || pad_p(pb) {
            li.c_pressed |= bit;
        }
    }
    li.c_down = down | li.touch_bits;
    if key_p(&[KeyCode::Tab, KeyCode::KeyN]) || pad_p(GamepadButton::DPadUp) {
        li.menu_toggle = true;
    }
}

/// Lock-on: Z toggles (default) or is held; pressed again on a target it moves to the next one.
/// The sim only ever sees `focus_held`.
fn lock_toggle(
    settings: Res<LinkSettings>,
    character: Res<Character>,
    slot: Res<ExperimentSlot>,
    time: Res<Time>,
    mut lock: ResMut<LockOn>,
    mut input: ResMut<PlayerInput>,
    mut sim: ResMut<GameSim>,
) {
    let raw = input.focus_held || std::mem::take(&mut lock.pulse);
    if !link_active(&character, &slot) {
        lock.raw_prev = raw;
        lock.release();
        return;
    }
    if lock.step(raw, settings.toggle_lock, sim.link_opponent.is_some(), time.delta_secs()) {
        let sim = &mut *sim;
        if !sim.link_focus.switch(&sim.mario.objects, &sim.focus_goombas, sim.mario.pos, sim.mario.face_angle[1]) {
            lock.release();
        }
    }
    if settings.toggle_lock {
        input.focus_held = lock.locked;
    }
}

/// Enable / disable the faithful controller as the character changes.
fn sync_enabled(character: Res<Character>, slot: Res<ExperimentSlot>, mut sim: ResMut<GameSim>, mut minimal: ResMut<crate::hud::HudMinimal>, mut we_hid: Local<bool>) {
    let want = link_active(&character, &slot);
    let age = age_of(*character);
    let link = &mut sim.mario.link;
    if want && (!link.enabled || link.age != age) {
        link.enable(age);
    } else if !want && link.enabled {
        link.disable();
    }
    if want && !minimal.0 {
        minimal.0 = true;
        *we_hid = true;
    } else if !want && *we_hid {
        minimal.0 = false;
        *we_hid = false;
    }
}

/// A fence `height` tall (sim units) and 3000 wide across the heading `yaw` at `at` (a debugging / test hook:
/// `JUMPMAN_EPONA_FENCE=<height in OoT units>` puts one 2400 units in front of Link): front face, top, back face.
pub fn with_fence(world: &jumpman_sim::collision::CollisionWorld, at: [f64; 3], yaw: f64, height: f64) -> jumpman_sim::collision::CollisionWorld {
    use jumpman_sim::math::{coss, sins};
    let mut w = world.clone();
    let base = world.find_floor_height(at[0], at[1] + 300.0, at[2]);
    let (sx, sz) = (sins(yaw + 16384.0) * 1500.0, coss(yaw + 16384.0) * 1500.0);
    let (nx, nz) = (sins(yaw) * 60.0, coss(yaw) * 60.0);
    let o = jumpman_sim::collision::SurfaceOptions::default();
    let pt = |dx: f64, dz: f64, y: f64| [at[0] + dx, base + y, at[2] + dz];
    let h = height;
    w.add_quad(pt(sx, sz, 0.0), pt(sx, sz, h), pt(-sx, -sz, h), pt(-sx, -sz, 0.0), &o);
    w.add_quad(pt(sx, sz, h), pt(sx + nx, sz + nz, h), pt(-sx + nx, -sz + nz, h), pt(-sx, -sz, h), &o);
    w.add_quad(pt(sx + nx, sz + nz, 0.0), pt(-sx + nx, -sz + nz, 0.0), pt(-sx + nx, -sz + nz, h), pt(sx + nx, sz + nz, h), &o);
    w
}

/// Call Epona (the song, the EPONA item, or the song menu): she gallops in from the edge of view. Returns the toast.
pub fn summon_epona(sim: &mut GameSim) -> String {
    if sim.mario.link.epona.is_mounted() {
        return "YOU ARE ALREADY ON EPONA".into();
    }
    let world = sim.mario.world.clone();
    let (pos, away) = (sim.mario.pos, sim.mario.camera_yaw + 32768.0); // beyond Link, in view
    if sim.mario.link.epona.summon(&world, pos, away) {
        "EPONA'S SONG - EPONA IS COMING".into()
    } else {
        "EPONA'S SONG - THERE IS NO ROOM FOR EPONA HERE".into()
    }
}

/// Epona at the start of a course: `JUMPMAN_EPONA=1` (mounted), `near` (standing beside Link) and the 'Epona follows'
/// setting (standing near the start). Runs once per course world.
fn epona_presence(settings: Res<LinkSettings>, mut sim: ResMut<GameSim>, mut seen: Local<(usize, u64, bool)>) {
    if !sim.mario.link.enabled || sim.mario.link.age != Age::Adult {
        return;
    }
    // A new course is a new world (allocation addresses can repeat) and a tick counter that starts over.
    let id = std::sync::Arc::as_ptr(&sim.mario.world) as usize;
    let fresh = !seen.2 || seen.0 != id || sim.ticks < seen.1;
    *seen = (id, sim.ticks, true);
    if !fresh {
        return;
    }
    let hook = std::env::var("JUMPMAN_EPONA").unwrap_or_default();
    if !(settings.epona_follows || !hook.is_empty()) {
        return;
    }
    let world = sim.mario.world.clone();
    let (pos, yaw) = (sim.mario.pos, sim.mario.face_angle[1]);
    let riding = hook == "1" || hook == "ride";
    // Mounted: under Link; otherwise standing beside him.
    let d = if riding { 0.0 } else { 330.0 };
    let (side, side_z) = (jumpman_sim::math::sins(yaw + 16384.0) * d, jumpman_sim::math::coss(yaw + 16384.0) * d);
    sim.mario.link.epona.place(&world, [pos[0] + side, pos[1], pos[2] + side_z], yaw);
    if riding {
        sim.mario.link.epona.start_mounted();
    }
    if let Some(h) = std::env::var("JUMPMAN_EPONA_FENCE").ok().and_then(|v| v.parse::<f64>().ok()) {
        let (fx, fz) = (pos[0] + jumpman_sim::math::sins(yaw) * 2400.0, pos[2] + jumpman_sim::math::coss(yaw) * 2400.0);
        let fenced = with_fence(&sim.mario.world, [fx, pos[1], fz], yaw, h * jumpman_sim::link::combat::OOT_SCALE);
        sim.mario.world = std::sync::Arc::new(fenced);
        seen.0 = std::sync::Arc::as_ptr(&sim.mario.world) as usize;
    }
}

fn pre_tick(mut sim: ResMut<GameSim>, mut input: ResMut<PlayerInput>, mut li: ResMut<LinkInput>, mut control: ResMut<StepControl>, modal: Res<ModalActive>, settings: Res<LinkSettings>) {
    if !sim.mario.link.enabled || modal.0 || control.controller.is_some() || control.skip {
        return;
    }
    sim.mario.link.target_point = sim.link_opponent;
    // First-person aim: the right stick and the drag since the last tick (consumed here).
    sim.mario.link.look_stick = [li.look_stick.x as f64, li.look_stick.y as f64];
    let drag = std::mem::take(&mut li.look_px);
    sim.mario.link.look_drag[0] += drag.x as f64;
    sim.mario.link.look_drag[1] += drag.y as f64;
    sim.mario.link.invert_aim_y = settings.invert_aim_y;
    let mut c = input.consume_step();
    c.button_pressed |= std::mem::take(&mut li.c_pressed);
    c.button_down |= li.c_down | c.button_pressed;
    control.controller = Some(c);
}

fn post_tick(
    mut sim: ResMut<GameSim>,
    mut goombas: ResMut<Goombas>,
    mut toast: ResMut<Toast>,
    mut cues: MessageWriter<crate::audio::SfxCue>,
    mut start: MessageWriter<crate::shell::StartCourse>,
    mut fx: ResMut<WorldFx>,
    mut oot_enemies: Option<ResMut<crate::zelda_enemies::OotEnemies>>,
    mut props: Option<ResMut<prop_world::PropWorld>>,
) {
    if !sim.mario.link.enabled {
        return;
    }
    // Targets that live outside the object arena: walking goombas and coins.
    let mut extra: Vec<Target> = goombas
        .0
        .iter()
        .enumerate()
        .filter(|(_, g)| g.tangible())
        .map(|(i, g)| Target { id: EXTRA_TARGET_BASE + i as u32, pos: g.pos.to_array(), radius: 55.0, height: 75.0, kind: TargetKind::Enemy })
        .collect();
    extra.extend(sim.pickups.iter().enumerate().filter(|(_, p)| !p.collected).map(|(i, p)| Target { id: PICKUP_TARGET_BASE + i as u32, pos: [p.pos[0], p.pos[1] - 30.0, p.pos[2]], radius: 40.0, height: 70.0, kind: TargetKind::Pickup }));
    // OoT enemies (zelda_enemies) are targets too; their hits are queued for their own tick.
    if let Some(en) = oot_enemies.as_ref() {
        extra.extend(en.link_targets());
    }
    // Props: grass, pots, eye / crystal switches, torches and webs take projectile hits like enemies do.
    if let Some(p) = props.as_ref() {
        extra.extend(p.props.targets());
    }
    LinkExt::step_projectiles(&mut sim.mario, &extra);
    let events = sim.mario.link.take_events();
    let log = std::env::var_os("JUMPMAN_LINK_LOG").is_some();
    if std::env::var("JUMPMAN_LINK_LOG").is_ok_and(|v| v == "2") {
        let m = &sim.mario;
        eprintln!("LINKACT tick {} act {:#x}/{} mv {:?} health {} pressed {:#x} focus {:?} hurt {} invinc {}", sim.ticks, m.action, m.action_arg, m.link.mv, m.health, m.controller.button_pressed, m.controller.focus_yaw.is_some(), m.hurt_counter, m.invinc_timer);
    }
    if log && sim.ticks % 30 == 0 {
        eprintln!("LINKPOS tick {} link {:?} yaw {:.0} goombas {:?}", sim.ticks, sim.mario.pos.map(|v| v.round()), sim.mario.face_angle[1], goombas.0.iter().filter(|g| g.tangible()).map(|g| g.pos.to_array().map(|v| v.round())).collect::<Vec<_>>());
    }
    if log {
        for e in &events {
            eprintln!("LINKEV tick {} {:?}", sim.ticks, e);
        }
    }
    for e in events {
        // `JUMPMAN_LINK_LOG=1` prints the combat events (headless checks of the touch / key path).
        if log && matches!(e, LinkEvent::Swing(_) | LinkEvent::HitObject { .. } | LinkEvent::HitExtra { .. }) {
            eprintln!("LINKLOG tick {} {:?}", sim.ticks, e);
        }
        if let Some(p) = props.as_mut() {
            if p.props.feed(&e) {
                continue;
            }
        }
        if let (LinkEvent::HitExtra { id, dmg, pos }, Some(en)) = (&e, oot_enemies.as_mut()) {
            if en.hit_extra(*id, *dmg, *pos) {
                continue;
            }
        }
        handle_event(e, &mut sim, &mut goombas, &mut toast, &mut cues, &mut start, &mut fx);
    }
}

/// Things the view layer should show for a moment.
#[derive(Resource, Default)]
pub struct WorldFx {
    pub flashes: Vec<(Vec3, f32, f32)>,
    /// Combat effects queued for `combat_fx.rs`.
    pub queue: Vec<combat_fx::FxEvent>,
    pub night: bool,
    /// Sun's Song was played: the OoT clock (`zelda::kankyo`) races to the other half of the day.
    pub sun_song: bool,
    pub storm: f32,
    pub lightning: f32,
    /// Songs Link played since the tags (`zelda::tags`) last looked.
    pub songs: Vec<jumpman_sim::link::ocarina::Song>,
}

/// Play an Ocarina of Time clip (`NA_SE_<name>`) from the extracted sound set.
fn oot(name: &str) {
    crate::audio::oot_sfx::play(name);
}

/// The sound of a projectile leaving Link (`NA_SE_IT_*`).
pub fn shot_sound(k: jumpman_sim::link::projectiles::Kind) -> Option<&'static str> {
    use jumpman_sim::link::projectiles::Kind;
    match k {
        Kind::Arrow => Some("IT_ARROW_SHOT"),
        Kind::FireArrow | Kind::IceArrow | Kind::LightArrow => Some("IT_MAGIC_ARROW_SHOT"),
        Kind::Seed => Some("IT_SLING_SHOT"),
        Kind::Boomerang => Some("IT_BOOMERANG_THROW"),
        Kind::Hookshot | Kind::Longshot => Some("IT_HOOKSHOT_CHAIN"),
        Kind::Bomb => Some("IT_BOMB_IGNIT"),
        Kind::Bombchu => Some("IT_BOMBCHU_MOVE"),
        Kind::Nut | Kind::DinsFire => None,
    }
}

/// The sound of a sword / hammer swing (`NA_SE_IT_*_SWING`).
pub fn swing_sound(s: jumpman_sim::link::combat::Swing) -> &'static str {
    use jumpman_sim::link::combat::Swing;
    match s {
        Swing::HammerForward | Swing::HammerSide => "IT_HAMMER_SWING",
        Swing::Spin | Swing::BigSpin => "IT_ROLLING_CUT",
        Swing::Combo(_) | Swing::FlipSlash | Swing::JumpSlash => "IT_SWORD_SWING_HARD",
        _ => "IT_SWORD_SWING",
    }
}

fn effect(cues: &mut MessageWriter<crate::audio::SfxCue>, kind: &'static str, pos: Option<[f64; 3]>) {
    cues.write(crate::audio::SfxCue::Effect { kind, pos });
}

fn handle_event(
    e: LinkEvent,
    sim: &mut GameSim,
    goombas: &mut Goombas,
    toast: &mut Toast,
    cues: &mut MessageWriter<crate::audio::SfxCue>,
    start: &mut MessageWriter<crate::shell::StartCourse>,
    fx: &mut WorldFx,
) {
    use jumpman_sim::link::combat::Dmg;
    match e {
        // The blade (or a projectile) struck an SM64 object: the same impact the OoT enemies get.
        LinkEvent::HitObject { dmg, pos, .. } => {
            fx.queue.push(combat_fx::FxEvent::Hit { pos, look: combat_fx::hit_look(dmg) });
            oot(if dmg == Dmg::HammerSwing { "IT_HAMMER_HIT" } else if dmg.is_sword() { "IT_SWORD_STRIKE" } else { "IT_SWORD_IMPACT" });
        }
        LinkEvent::HitExtra { id, dmg, pos } => {
            fx.queue.push(combat_fx::FxEvent::Hit { pos, look: combat_fx::hit_look(dmg) });
            oot(if dmg == Dmg::HammerSwing { "IT_HAMMER_HIT" } else if dmg.is_sword() { "IT_SWORD_STRIKE" } else { "IT_SWORD_IMPACT" });
            if (EXTRA_TARGET_BASE..PICKUP_TARGET_BASE).contains(&id) {
                let i = (id - EXTRA_TARGET_BASE) as usize;
                if dmg.typical() > 0 || dmg == Dmg::Nut {
                    if let Some(g) = goombas.0.get_mut(i) {
                        if g.tangible() && dmg.typical() > 0 {
                            g.defeat(false);
                            sim.coins += 1;
                            effect(cues, "stomp", Some(pos));
                        }
                    }
                }
            }
        }
        LinkEvent::Fetched { id } => {
            if id >= PICKUP_TARGET_BASE {
                let i = (id - PICKUP_TARGET_BASE) as usize;
                if let Some(p) = sim.pickups.get_mut(i) {
                    if !p.collected {
                        p.collected = true;
                        let v = p.value;
                        sim.coins += v as u32;
                        oot("SY_GET_RUPY");
                    }
                }
            }
        }
        LinkEvent::Explosion { pos, radius } => {
            fx.queue.push(combat_fx::FxEvent::Explosion { pos, radius });
            oot("IT_BOMB_EXPLOSION");
            fx.flashes.push((crate::render::to_bevy(pos), 1.0, 0.0));
        }
        LinkEvent::HammerSlam(pos) => {
            fx.queue.push(combat_fx::FxEvent::Slam { pos });
            oot("IT_HAMMER_HIT");
            fx.flashes.push((crate::render::to_bevy(pos), 0.5, 0.0));
        }
        LinkEvent::Flash { pos, .. } => {
            fx.queue.push(combat_fx::FxEvent::NutFlash { pos });
            oot("IT_DEKU");
            fx.flashes.push((crate::render::to_bevy(pos), 1.2, 0.0));
        }
        LinkEvent::Shot(k) => {
            if let Some(n) = shot_sound(k) {
                oot(n);
            }
        }
        LinkEvent::Boomerang => oot("IT_BOOMERANG_THROW"),
        LinkEvent::Swing(s) => oot(swing_sound(s)),
        LinkEvent::ItemUsed(item) => {
            use jumpman_sim::link::items::Item;
            match item {
                Item::DinsFire => oot("PL_MAGIC_FIRE"),
                Item::FaroresWind => oot("PL_MAGIC_WIND_NORMAL"),
                Item::NayrusLove => oot("PL_MAGIC_SOUL_NORMAL"),
                Item::DekuStick | Item::Bow | Item::Slingshot | Item::Hookshot | Item::Boomerang | Item::FireArrow | Item::IceArrow | Item::LightArrow => oot("PL_CHANGE_ARMS"),
                _ => {}
            }
        }
        LinkEvent::Ignite { pos, .. } => {
            oot("EV_FLAME_IGNITION");
            fx.queue.push(combat_fx::FxEvent::Ignite { pos })
        }
        LinkEvent::Freeze { pos, .. } => {
            oot("PL_FREEZE");
            fx.queue.push(combat_fx::FxEvent::Freeze { pos })
        }
        LinkEvent::Impact { kind, pos } => {
            use jumpman_sim::link::projectiles::Kind;
            oot(match kind {
                Kind::Seed => "IT_SLING_REFLECT",
                Kind::Hookshot | Kind::Longshot => "IT_HOOKSHOT_REFLECT",
                Kind::Boomerang => "IT_WALL_HIT_HARD",
                _ => "IT_ARROW_STICK_OBJ",
            });
            fx.queue.push(combat_fx::FxEvent::Stuck { pos, kind })
        }
        LinkEvent::HookAttached => oot("IT_HOOKSHOT_STICK_OBJ"),
        LinkEvent::BombLifted => oot("PL_PULL_UP_POT"),
        LinkEvent::BombThrown => oot("PL_THROW"),
        LinkEvent::StickLit => oot("EV_FLAME_IGNITION"),
        LinkEvent::NoAmmo(item) => {
            oot("SY_ERROR");
            toast.show(format!("NO {}", item.label()), 1.2)
        }
        LinkEvent::NoMagic(_) => {
            oot("SY_ERROR");
            toast.show("NOT ENOUGH MAGIC", 1.2)
        }
        LinkEvent::WrongAge(item) => toast.show(format!("{} CAN'T BE USED HERE", item.label()), 1.4),
        LinkEvent::NayruOn => toast.show("NAYRU'S LOVE", 1.5),
        LinkEvent::FaroreSet(_) => toast.show("FARORE'S WIND: POINT SET", 1.6),
        LinkEvent::FaroreWarp(_) => toast.show("FARORE'S WIND: RETURNED", 1.6),
        LinkEvent::LensToggled(on) => {
            oot(if on { "SY_GLASSMODE_ON" } else { "SY_GLASSMODE_OFF" });
            toast.show(if on { "LENS OF TRUTH ON" } else { "LENS OF TRUTH OFF" }, 1.0)
        }
        LinkEvent::BeanPlanted(_) => toast.show("MAGIC BEAN PLANTED", 1.5),
        LinkEvent::BottleUsed(b) => {
            toast.show(b.label(), 1.2);
            oot("SY_HP_RECOVER");
        }
        LinkEvent::StickBroke => {
            oot("IT_WOODSTICK_BROKEN");
            toast.show("THE DEKU STICK BROKE", 1.5)
        }
        LinkEvent::SpinCharged => oot("IT_ROLLING_CUT_LV1"),
        LinkEvent::Note(n) => crate::audio::play_tone(n.frequency(), 0.55),
        LinkEvent::UnknownSong => toast.show("YOU DON'T KNOW THAT SONG", 1.2),
        LinkEvent::Song(song) => {
            fx.songs.push(song);
            crate::zelda::music::queue_jingle(crate::zelda::music::song_jingle(song));
            toast.show(song.label(), 2.0);
            match song.effect() {
                SongEffect::Warp(id) => {
                    if crate::game::courses::info(id).is_some() {
                        start.write(crate::shell::StartCourse(crate::game::world::CourseRequest::new(id)));
                    } else {
                        toast.show(format!("{} - NO DESTINATION", song.label()), 2.0);
                    }
                }
                SongEffect::WarpTempleOfTime => {
                    start.write(crate::shell::StartCourse(crate::game::world::CourseRequest::new("temple_of_time")));
                }
                SongEffect::SunsSong => {
                    fx.sun_song = true;
                    toast.show("SUN'S SONG", 2.5);
                }
                SongEffect::Storm => {
                    fx.storm = 25.0;
                    fx.lightning = 1.0;
                    toast.show("SONG OF STORMS", 2.0);
                }
                SongEffect::TimeBlocks => toast.show("SONG OF TIME - NOTHING TO OPEN HERE", 2.5),
                SongEffect::RoyalCrest => toast.show("ZELDA'S LULLABY - NO CREST HERE", 2.5),
                SongEffect::CallEpona => {
                    // The sim already placed her when the song was recognised.
                    let e = &sim.mario.link.epona;
                    toast.show(
                        if e.is_mounted() {
                            "EPONA'S SONG - YOU ARE ALREADY ON EPONA"
                        } else if e.present && e.act == jumpman_sim::link::epona::Act::Follow {
                            "EPONA'S SONG - EPONA IS COMING"
                        } else {
                            "EPONA'S SONG - THERE IS NO ROOM FOR EPONA HERE"
                        },
                        2.5,
                    )
                }
                SongEffect::SariasCall => {
                    sim.mario.link.inv.heal(jumpman_sim::link::items::HEART);
                    toast.show("SARIA'S SONG - SARIA CHEERS YOU UP (+1 HEART)", 2.5);
                }
            }
        }
        LinkEvent::Epona(e) => epona_sound(e),
        _ => {}
    }
}

/// `NA_SE_EV_HORSE_*` and the carrot chime (`NA_SE_SY_CARROT_RECOVER`).
fn epona_sound(e: jumpman_sim::link::epona::EponaEvent) {
    use jumpman_sim::link::epona::EponaEvent as E;
    let clip = match e {
        E::Neigh | E::Summoned => "voices/epona/NEIGH",
        E::Groan => "voices/epona/GROAN",
        E::Walk => "voices/epona/WALK",
        E::Run => "voices/epona/RUN",
        E::Jump => "voices/epona/JUMP",
        E::Land => "voices/epona/LAND",
        E::Land2 => "voices/epona/LAND2",
        E::Slip | E::SandDust => "voices/epona/SLIP",
        E::Boost => return, // the carrot is silent in the original (`EnHorse_CheckBoost`); only the empty-carrot neigh sounds
        E::CarrotRecover => {
            // Two quick sine notes: C4 then C5 (the channel script's two notes).
            crate::audio::play_tone(261.6, 0.12);
            crate::audio::play_tone(523.2, 0.16);
            return;
        }
        E::Mounted | E::Dismounted => return,
    };
    crate::audio::play_clip(clip, 1.0);
}

