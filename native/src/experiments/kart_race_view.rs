//! Presentation for the MK64 items and race mode: CPU kart sprites (the occupied-kart atlas, tinted), pooled item
//! boxes (a translucent rainbow cube tumbling around a spinning '?') / bananas / shells, and the bevy_ui HUD (the
//! item window; in a race also position, lap, time, countdown banner and the results card). Item boxes and items
//! also exist in free roam (the `Practice` phase). The rules are in `kart_race.rs`; `kart.rs` only calls the small
//! hooks here.
//!
//! The HUD reaches bevy_ui through the [`KartHud`] resource (written with `commands.insert_resource`
//! from the experiment), and the UI hands taps back through atomics ([`take_item_press`] and friends), so
//! neither `Host` nor `XpParams` grew a new field.
use crate::button_marker::LoadWithSettings;
use super::dock::DockItem;
use super::kart_cpu::{Bot, CpuAi, RaceSetup};
use super::kart_hud::{self, lakitu_file, LakituIn, MapFit, Standing};
use super::kart_physics::{Ground, Input as KartInput, Kart};
use super::kart_race::{format_ticks, load_bests, save_bests, Bests, Event, Item, KartRace, Phase, Results, BOX_RESPAWN, CPU_COUNT, GO_TICKS, LAPS};
use super::Host;
use crate::render::{to_bevy, SCALE};
use bevy::math::{Affine2, DVec3};
use bevy::prelude::*;
use jumpman_sim::math::Vec3 as V3;
use std::sync::atomic::{AtomicBool, Ordering};

const SEED: u32 = 20240607;
/// Edge of the item box cube, world units (10 course units).
const BOX_SIZE: f32 = 80.;
/// World size of the banana peel and shell sprites.
const BANANA_SIZE: f32 = 84.;
/// A shell is 8 course units across (`boundingBoxSize` 4).
const SHELL_SIZE: f32 = 64.;
/// World size of the item sprites that share the 18 x 18 course unit quad (144 world units).
pub const SPRITE_SIZE: f32 = 144.;
/// `characterId` of each CPU racer (Luigi, Peach, Toad) for the kart frame atlases.
const CPU_CHARACTER: [usize; CPU_COUNT] = [1, 6, 3];
/// Tint, name and results colour of the CPU drivers.
pub const CPU_STYLE: [(&str, [f32; 3]); CPU_COUNT] = [("Luigi", [0.52, 1.0, 0.52]), ("Peach", [1.0, 0.65, 0.83]), ("Toad", [0.62, 0.77, 1.0])];

static ITEM_TAP: AtomicBool = AtomicBool::new(false);
static RETRY_TAP: AtomicBool = AtomicBool::new(false);
static LEAVE_TAP: AtomicBool = AtomicBool::new(false);
/// Item button tapped on the touch overlay since last asked.
pub fn take_item_press() -> bool {
    ITEM_TAP.swap(false, Ordering::Relaxed)
}
pub fn press_item() {
    ITEM_TAP.store(true, Ordering::Relaxed);
}
pub fn press_retry() {
    RETRY_TAP.store(true, Ordering::Relaxed);
}
pub fn press_leave() {
    LEAVE_TAP.store(true, Ordering::Relaxed);
}
pub fn take_retry_press() -> bool {
    RETRY_TAP.swap(false, Ordering::Relaxed)
}
pub fn take_leave_press() -> bool {
    LEAVE_TAP.swap(false, Ordering::Relaxed)
}

/// What a key press in race mode asks `kart.rs` to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceCmd {
    Start,
    Leave,
    /// Handled here (item, rescue).
    Handled,
}

pub use super::kart_hud::{KartHud, ResultsCard};

fn item_style(item: Item) -> (String, Color) {
    let c = item.colour();
    (item.name().to_uppercase(), Color::srgb(c[0], c[1], c[2]))
}

struct Pooled {
    entities: Vec<Entity>,
    shown: Vec<bool>,
}
fn icon_name(item: Item) -> &'static str {
    item.icon()
}
/// Item actor sprites drawn at once (bananas, shells, fake boxes).
const ACTOR_SLOTS: usize = 24;
const FAKE_SLOTS: usize = 8;
impl Pooled {
    fn new(entities: Vec<Entity>) -> Self {
        let n = entities.len();
        Self { entities, shown: vec![true; n] }
    }
    fn show(&mut self, host: &mut Host, i: usize, on: bool, transform: Option<Transform>) {
        let Some(&e) = self.entities.get(i) else { return };
        if self.shown[i] != on {
            self.shown[i] = on;
            host.commands.entity(e).insert(if on { Visibility::Inherited } else { Visibility::Hidden });
        }
        if let (true, Some(t)) = (on, transform) {
            host.commands.entity(e).insert(t);
        }
    }
}

pub struct KartRaceView {
    pub race: Option<KartRace>,
    path: Vec<V3>,
    bests: Bests,
    sprites: Vec<(Entity, Handle<StandardMaterial>)>,
    fit: Option<MapFit>,
    auto: Option<Bot>,
    /// The CPU driver tables of `public/mk64/cpu-ai.json` and the course they were picked for.
    cpu_ai: Option<CpuAi>,
    course_dir: String,
    auto_ticks: u32,
    shell_materials: Vec<Handle<StandardMaterial>>,
    boxes: Option<Pooled>,
    /// The '?' inside each box.
    marks: Option<Pooled>,
    bananas: Option<Pooled>,
    shells: Option<Pooled>,
    /// Fake item boxes (a darker cube with the '?' upside down) and their marks.
    fakes: Option<Pooled>,
    fake_marks: Option<Pooled>,
    /// The green, red and blue shell strips (`mk64-*-shell.png`).
    shell_strips: Vec<Handle<Image>>,
    /// The item button is held (B) / was tapped (key, touch: released on the next tick).
    item_down: bool,
    tap_release: bool,
    /// Events drained by the last tick (for sound cues) and the world positions of the boxes the player smashed.
    pub last_events: Vec<Event>,
    pub smashed: Vec<[f64; 3]>,
    /// World position and colour of every racer hit this tick (stars and a puff).
    pub hit_fx: Vec<([f64; 3], u32)>,
    prev_cpu: [V3; CPU_COUNT],
    item_requested: bool,
    rescue_requested: bool,
    toasts: Vec<String>,
    last_hud: Option<KartHud>,
    /// The experiment's title card, hidden while racing (it covers a phone screen).
    saved_card: Option<String>,
    /// Round SM64 shadows on the track: slot 0 the player, 1.. the CPUs.
    shadows: Vec<(Entity, Handle<StandardMaterial>)>,
    /// `public/mk64` (the mode saves and arenas are read from here).
    root: std::path::PathBuf,
    /// A finished race / battle waiting to be shown: (panel, ticks left). The finish banner plays first.
    ended: Option<(super::kart_flow::Ended, u32)>,
    /// The Time Trial ghost: a translucent kart sprite.
    ghost_sprite: Option<(Entity, Handle<StandardMaterial>)>,
}

impl KartRaceView {
    pub fn new() -> Self {
        Self {
            race: None,
            path: vec![],
            bests: load_bests(),
            sprites: vec![],
            fit: None,
            auto: None,
            cpu_ai: None,
            course_dir: String::new(),
            auto_ticks: 0,
            shell_materials: vec![],
            boxes: None,
            marks: None,
            bananas: None,
            shells: None,
            fakes: None,
            fake_marks: None,
            shell_strips: vec![],
            item_down: false,
            tap_release: false,
            last_events: vec![],
            smashed: vec![],
            hit_fx: vec![],
            prev_cpu: [[0.; 3]; CPU_COUNT],
            shadows: vec![],
            item_requested: false,
            rescue_requested: false,
            toasts: vec![],
            last_hud: None,
            saved_card: None,
            root: std::path::PathBuf::new(),
            ended: None,
            ghost_sprite: None,
        }
    }
    /// In a race (as opposed to free roam with items).
    pub fn active(&self) -> bool {
        self.race.as_ref().is_some_and(|r| r.phase != Phase::Practice)
    }
    /// Items are live (free roam or race).
    pub fn live(&self) -> bool {
        self.race.is_some()
    }
    /// The player's racer.
    pub fn me(&self) -> Option<&super::kart_race::Racer> {
        self.race.as_ref().and_then(|r| r.racers.first())
    }
    pub fn clock(&self) -> u32 {
        self.race.as_ref().map_or(0, |r| r.clock)
    }

    /// The track whose CPU tables the next race uses; `root` is the folder of the MK64 exports.
    pub fn set_course(&mut self, root: &std::path::Path, dir: &str) {
        if self.cpu_ai.is_none() {
            self.cpu_ai = CpuAi::load(root);
        }
        self.course_dir = dir.to_string();
        self.root = root.to_path_buf();
    }

    /// The session (cup in progress, career, ghosts).
    pub fn flow<R>(&self, f: impl FnOnce(&mut super::kart_flow::Flow) -> R) -> R {
        super::kart_flow::with(&self.root, f)
    }

    /// The finished race's panel once its finish banner is over (`step` counts it down).
    pub fn take_ended(&mut self) -> Option<super::kart_flow::Ended> {
        match self.ended.take() {
            Some((e, 0)) => Some(e),
            other => {
                self.ended = other;
                None
            }
        }
    }

    /// The kart setup menu's choices with this track's CPU data.
    pub fn setup(&self) -> RaceSetup {
        let s = super::kart_setup::get();
        match &self.cpu_ai {
            Some(ai) => RaceSetup { cc: s.cc, character: s.character, course: ai.course(&self.course_dir), tables: Some(ai.kart.clone()) },
            None => RaceSetup { cc: s.cc, character: s.character, ..RaceSetup::default() },
        }
    }

    /// Stats of the player's kart under the current setup.
    pub fn player_stats(&self) -> super::kart_stats::KartStats {
        let setup = self.setup();
        setup.stats(setup.character)
    }

    /// Spawn the pooled sprites and item meshes. `atlas` is the kart atlas the player already uses.
    pub fn build(&mut self, host: &mut Host, _atlas: Handle<Image>, path: &[V3]) {
        self.path = path.to_vec();
        self.fit = Some(MapFit::from_path(&path.iter().map(|p| [p[0], p[1], p[2]]).collect::<Vec<_>>()));
        // MK64's own kart shadow texture, one quad per kart (`render_player_shadow`).
        self.shadows = super::kart_sprite_view::spawn_shadows(host, CPU_COUNT + 1);
        let quad = super::kart_sprite_view::sprite_mesh(host);
        for c in 0..CPU_COUNT {
            // Each CPU has its own MK64 frame atlas (Luigi, Peach, Toad), no tint.
            let material = super::kart_sprite_view::sprite_material(host, CPU_CHARACTER[c]);
            let e = host.spawn((Mesh3d(quad.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden));
            self.sprites.push((e, material));
        }
        let ghost_material = super::kart_sprite_view::sprite_material(host, 0);
        let ghost = host.spawn((Mesh3d(quad.clone()), MeshMaterial3d(ghost_material.clone()), Transform::default(), Visibility::Hidden));
        self.ghost_sprite = Some((ghost, ghost_material));
        // Item box: a translucent cube with a rainbow gradient across its corners, a '?' spinning inside.
        let mut cube = Mesh::from(Cuboid::new(BOX_SIZE * SCALE, BOX_SIZE * SCALE, BOX_SIZE * SCALE));
        let colors: Vec<[f32; 4]> = match cube.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::render::mesh::VertexAttributeValues::Float32x3(p)) => p
                .iter()
                .map(|v| {
                    let (x, y, z) = (v[0] / (BOX_SIZE * SCALE), v[1] / (BOX_SIZE * SCALE), v[2] / (BOX_SIZE * SCALE));
                    let h = ((x + y * 1.3 - z * 0.7) + 1.5) / 3.;
                    Color::hsl(h.rem_euclid(1.) * 360., 0.95, 0.58).to_linear().to_f32_array()
                })
                .collect(),
            _ => vec![],
        };
        cube.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        let cube = host.meshes.add(cube);
        let cube_material = host.materials.add(StandardMaterial { base_color: Color::srgba(1., 1., 1., 0.78), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
        // The question mark is the ROM texture (32 x 64), turning inside the cube.
        let mark_mesh = host.meshes.add(Rectangle::new(BOX_SIZE * 0.5 * SCALE, BOX_SIZE * 1.0 * SCALE));
        let mark_material = host.materials.add(StandardMaterial { base_color_texture: Some(super::kart::KartExperiment::texture(host, "mk64-item-box-question-mark.png")), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() });
        // Banana peel: MK64's own upright sprite, standing on the track.
        let banana_mesh = host.meshes.add(Rectangle::new(BANANA_SIZE * SCALE, BANANA_SIZE * SCALE));
        let banana_material = host.materials.add(StandardMaterial { base_color_texture: Some(super::kart::KartExperiment::texture(host, "mk64-banana.png")), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() });
        // Shells: MK64's 8-frame spinning sprite strips (green from the ROM, red with the palette of
        // `init_red_shell_texture`, the spiny blue shell), one material per slot so they spin out of step.
        let shell_mesh = host.meshes.add(Rectangle::new(SHELL_SIZE * SCALE, SHELL_SIZE * SCALE));
        self.shell_strips = ["mk64-green-shell.png", "mk64-red-shell.png", "mk64-blue-shell.png"].iter().map(|f| super::kart::KartExperiment::texture(host, f)).collect();
        let shell_strip = self.shell_strips[0].clone();
        self.shell_materials = (0..ACTOR_SLOTS)
            .map(|_| host.materials.add(StandardMaterial { base_color_texture: Some(shell_strip.clone()), uv_transform: Affine2::from_scale(Vec2::new(1. / 8., 1.)), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() }))
            .collect();
        // Fake item box: the same cube, darker and redder, the '?' upside down (`common_model_fake_itembox`).
        let fake_material = host.materials.add(StandardMaterial { base_color: Color::srgba(1., 0.55, 0.45, 0.85), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
        let pool = |host: &mut Host, n: usize, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>| {
            Pooled::new((0..n).map(|_| host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden))).collect())
        };
        let mut boxes = pool(host, 24, &cube, &cube_material);
        let mut marks = pool(host, 24, &mark_mesh, &mark_material);
        let mut bananas = pool(host, ACTOR_SLOTS, &banana_mesh, &banana_material);
        let mut shells = Pooled::new(self.shell_materials.iter().map(|m| host.spawn((Mesh3d(shell_mesh.clone()), MeshMaterial3d(m.clone()), Transform::default(), Visibility::Hidden))).collect());
        let mut fakes = pool(host, FAKE_SLOTS, &cube, &fake_material);
        let mut fake_marks = pool(host, FAKE_SLOTS, &mark_mesh, &mark_material);
        // The pools start hidden; `shown` mirrors that.
        for p in [&mut boxes, &mut marks, &mut bananas, &mut shells, &mut fakes, &mut fake_marks] {
            p.shown.iter_mut().for_each(|s| *s = false);
        }
        self.boxes = Some(boxes);
        self.marks = Some(marks);
        self.bananas = Some(bananas);
        self.shells = Some(shells);
        self.fakes = Some(fakes);
        self.fake_marks = Some(fake_marks);
    }

    /// Free roam with item boxes: no countdown, laps or CPUs. The kart stays where it is.
    pub fn begin_practice(&mut self, host: &mut Host, phys: &Kart, ground: &dyn Ground) {
        let mut race = KartRace::new(&self.path, SEED).with_course(super::kart::selected_track().dir);
        race.configure(self.setup(), self.course_dir == "toads_turnpike");
        race.best_lap = self.bests.best_lap;
        race.best_total = self.bests.best_total;
        race.start_practice(phys, ground);
        self.race = Some(race);
        self.item_requested = false;
        self.rescue_requested = false;
        take_item_press();
        self.last_events.clear();
        self.smashed.clear();
        self.toasts.clear();
        self.last_hud = None;
        self.hide_all(host);
        // A race may have been running: bring the title card back.
        if let Some(card) = self.saved_card.take() {
            host.ui.card = card;
        }
        super::kart_music::free_roam();
    }

    /// Start or restart; the player's kart is replaced on the grid.
    pub fn begin(&mut self, host: &mut Host, phys: &mut Kart, ground: &dyn Ground) {
        if self.race.as_ref().is_none_or(|r| r.phase == Phase::Practice) {
            let mut r = KartRace::new(&self.path, SEED).with_course(super::kart::selected_track().dir);
            r.best_lap = self.bests.best_lap;
            r.best_total = self.bests.best_total;
            self.race = Some(r);
        }
        let setup = self.setup();
        let race = self.race.as_mut().unwrap();
        race.configure(setup, self.course_dir == "toads_turnpike");
        race.start(phys, ground);
        self.ended = None;
        super::kart_flow::with(&self.root, |f| f.begin(race, phys, ground, SEED));
        for (c, (_, material)) in self.sprites.iter().enumerate() {
            super::kart_sprite_view::set_character(host, material, race.setup.cpu_character(c));
        }
        if let Some((_, material)) = &self.ghost_sprite {
            super::kart_sprite_view::set_character(host, material, race.setup.character);
        }
        self.item_requested = false;
        self.rescue_requested = false;
        take_item_press();
        for (i, c) in race.cpus.iter().enumerate() {
            self.prev_cpu[i] = c.kart.pos;
        }
        self.toasts.clear();
        self.last_hud = None;
        if self.saved_card.is_none() {
            self.saved_card = Some(std::mem::take(&mut host.ui.card));
        }
        super::kart_music::race_start();
        let benched = self.race.as_ref().map_or(0, |r| r.benched);
        super::kart_sfx::set_benched(benched);
    }

    pub fn end(&mut self, host: &mut Host) {
        self.race = None;
        self.hide_all(host);
        if let Some(card) = self.saved_card.take() {
            host.ui.card = card;
        }
        host.commands.insert_resource(KartHud::default());
        host.commands.insert_resource(ModeOff);
        self.last_hud = None;
    }

    fn hide_all(&mut self, host: &mut Host) {
        for e in self.sprites.iter().map(|(e, _)| e).chain(self.shadows.iter().map(|(e, _)| e)).chain(self.ghost_sprite.iter().map(|(e, _)| e)) {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
        for pool in [self.boxes.as_mut(), self.marks.as_mut(), self.bananas.as_mut(), self.shells.as_mut(), self.fakes.as_mut(), self.fake_marks.as_mut()].into_iter().flatten() {
            for i in 0..pool.entities.len() {
                pool.show(host, i, false, None);
            }
        }
    }

    /// MK64 shadow of the kart in `slot` (0 = the player): see [`super::kart_sprite_view::place_shadow`].
    pub fn shadow(&self, host: &mut Host, slot: usize, input: &super::kart_sprite_view::SpriteIn, ground: &dyn Ground) {
        if let Some((entity, _)) = self.shadows.get(slot) {
            super::kart_sprite_view::place_shadow(host, *entity, input, ground);
        }
    }

    fn drop_shadow(shadows: &[(Entity, Handle<StandardMaterial>)], host: &mut Host, slot: usize) {
        if let Some((e, _)) = shadows.get(slot) {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
    }

    /// Key presses (also dock clicks). `None` when the key is not a race key.
    pub fn key(&mut self, key: KeyCode) -> Option<RaceCmd> {
        match key {
            KeyCode::KeyT => Some(RaceCmd::Start),
            KeyCode::KeyN if self.active() => Some(RaceCmd::Leave),
            KeyCode::KeyE | KeyCode::ShiftLeft | KeyCode::ShiftRight if self.live() => {
                self.item_requested = true;
                Some(RaceCmd::Handled)
            }
            KeyCode::KeyR if self.active() => {
                self.rescue_requested = true;
                Some(RaceCmd::Handled)
            }
            _ => None,
        }
    }

    /// The item button's held state this frame (B on the pad): letting go releases a trailing item.
    pub fn item_held(&mut self, down: bool) {
        self.item_down = down;
    }
    /// The player's kart under item effects (lightning shrink, star tint, boo fade, squish).
    pub fn player_look(&self) -> super::kart_sprite_view::SpriteLook {
        self.race.as_ref().and_then(|r| r.racers.first()).map_or_else(Default::default, |m| super::kart_sprite_view::SpriteLook::from(&m.fx.look()))
    }

    /// One 30 Hz tick of the race. `item_pad` is the gamepad / Shift B edge.
    pub fn step(&mut self, pad: &KartInput, item_pad: bool, phys: &mut Kart, ground: &dyn Ground) {
        let stats = self.player_stats();
        if let Some((_, left)) = self.ended.as_mut() {
            *left = left.saturating_sub(1);
        }
        let Some(race) = self.race.as_mut() else { return };
        if race.phase == Phase::Practice && phys.stats != stats {
            phys.set_stats(stats);
        }
        // Test hook: `JUMPMAN_KART_AUTO=1` lets the CPU driver steer the player; `=drift` also hops and holds a drift
        // through every bend (screenshots of the drift sparks and the mini-turbo).
        let auto_pad;
        let pad = if let (Ok(mode), true) = (std::env::var("JUMPMAN_KART_AUTO"), race.phase == Phase::Racing || race.phase == Phase::Practice) {
            let bot = self.auto.get_or_insert_with(|| Bot::new(0));
            let mut input = bot.input(&race.setup.course, &race.path, phys, true);
            if mode == "drift" {
                // Drive 2 s, then hop into a right-hand drift and hold it.
                self.auto_ticks += 1;
                if self.auto_ticks >= 60 {
                    input.raw_x = 70;
                    input.hop_down = true;
                    input.hop_pressed = self.auto_ticks == 60;
                }
            }
            // `=left` / `=right`: drive 2 s, then hold the stick hard over (screenshots of the sprite turning).
            if mode == "left" || mode == "right" {
                if self.auto_ticks >= 60 {
                    input.raw_x = if mode == "left" { -80 } else { 80 };
                }
            }
            // The auto driver uses a held item 1.5 s after the start (`JUMPMAN_KART_AUTO_ITEM` = ticks to wait).
            self.auto_ticks += u32::from(mode != "drift");
            if race.racers[0].item.is_some() && std::env::var("JUMPMAN_KART_AUTO_ITEM").ok().and_then(|v| v.parse::<u32>().ok()).is_some_and(|t| t == self.auto_ticks) {
                self.item_requested = true;
            }
            auto_pad = input;
            &auto_pad
        } else {
            pad
        };
        for (i, c) in race.cpus.iter().enumerate() {
            self.prev_cpu[i] = c.kart.pos;
        }
        if std::mem::take(&mut self.rescue_requested) {
            race.rescue_player(phys, ground);
        }
        if race.phase != Phase::Practice {
            test_hook(race);
        } else {
            practice_hook(race);
        }
        if race.phase == Phase::Practice && race.clock == 45 && std::env::var_os("JUMPMAN_KART_ITEM_USE").is_some() {
            self.item_requested = true;
        }
        let touch = take_item_press();
        let tapped = std::mem::take(&mut self.item_requested) || touch;
        let use_item = tapped || item_pad;
        // Z in MK64: the press uses the item, the release drops / throws what trails behind (stick up throws a
        // banana forward, stick down sends a green shell backwards). A key or touch tap releases on the next tick.
        let was_down = race.item_pad.stick_x == i32::MIN;
        let release = (was_down && !self.item_down) || std::mem::take(&mut self.tap_release);
        race.item_pad = super::kart_items::Pad { press: false, release, stick_x: pad.raw_x, stick_y: pad.raw_y };
        if tapped && !self.item_down && !race.items.is_holding_actor(0) {
            self.tap_release = true;
        } else if tapped && race.items.is_holding_actor(0) {
            // A tap while something trails behind lets it go.
            race.item_pad.release = true;
        }
        let root = self.root.clone();
        super::kart_flow::with(&root, |f| f.before_tick(race, phys));
        race.tick(pad, use_item, phys, ground);
        // Remember the button for the release edge (stick_x is rewritten every tick, so it carries the flag).
        if self.item_down {
            race.item_pad.stick_x = i32::MIN;
        }
        let events = race.drain_events();
        if race.phase != Phase::Practice && self.ended.is_none() {
            if let Some(e) = super::kart_flow::with(&root, |f| f.after_tick(race, phys, &events)) {
                // The finish banner plays for 3 s (battles end at once) before the panel.
                let delay = if e.0 == super::kart_menu::Screen::BattleResult { 45 } else { 90 };
                self.ended = Some((e, delay));
            }
        }
        super::kart_sfx::set_benched(race.benched);
        self.smashed = if events.contains(&Event::Pickup(0)) {
            race.boxes.iter().filter(|b| b.respawn == BOX_RESPAWN).map(|b| [b.pos[0] * 8., b.y * 8. + 70., b.pos[1] * 8.]).collect()
        } else {
            vec![]
        };
        self.last_events = events.clone();
        super::kart_music::on_events(&events, race);
        self.hit_fx.clear();
        for e in &events {
            if let Event::Smash(i, item) = e {
                let at = if *i == 0 { phys.pos } else { race.cpus.get(i - 1).map_or(phys.pos, |c| c.kart.pos) };
                let c = item.colour();
                let colour = ((c[0] * 255.) as u32) << 16 | ((c[1] * 255.) as u32) << 8 | (c[2] * 255.) as u32;
                self.hit_fx.push(([at[0] * 8., at[1] * 8., at[2] * 8.], colour));
            }
        }
        for e in events {
            match e {
                Event::Lap(0, n) if n < LAPS => self.toasts.push(if n == LAPS - 1 { "Final lap!".into() } else { format!("Lap {} of {LAPS}", n + 1) }),
                Event::Smash(0, item) => self.toasts.push(format!("Hit by a {}!", item.name().to_lowercase())),
                Event::Rescue(0) => self.toasts.push("Back on track".into()),
                Event::StartBoost(0) => self.toasts.push("Rocket start!".into()),
                _ => {}
            }
        }
        for e in std::mem::take(&mut race.item_events) {
            match e {
                super::kart_items::ItemEvent::Thunder(i) => self.toasts.push(if i == 0 { "Lightning! Everyone else shrinks".into() } else { "Struck by lightning!".into() }),
                super::kart_items::ItemEvent::Use(0, Item::Star) => self.toasts.push("Star power!".into()),
                super::kart_items::ItemEvent::Steal(0, Some(item)) => self.toasts.push(format!("Boo brought a {}", item.name().to_lowercase())),
                super::kart_items::ItemEvent::Steal(0, None) => self.toasts.push("Boo found nothing".into()),
                _ => {}
            }
        }
        if race.best_dirty {
            race.best_dirty = false;
            self.bests = Bests { best_lap: race.best_lap, best_total: race.best_total };
            save_bests(&self.bests);
        }
    }

    /// Effect inputs for the CPU karts (the player's comes from `kart.rs`).
    pub fn cpu_fx_frames(&self) -> Vec<super::kart_fx::FxFrame> {
        let Some(race) = self.race.as_ref().filter(|r| r.phase != Phase::Practice && r.phase != Phase::Countdown) else { return vec![] };
        race.cpus
            .iter()
            .enumerate()
            .map(|(c, cpu)| {
                let k = &cpu.kart;
                let live = !race.is_benched(c + 1);
                let r = &race.racers[c + 1];
                let pos = [k.pos[0] * 8., k.pos[1] * 8., k.pos[2] * 8.];
                let spun = r.spin > 0;
                let puff = super::kart_puffs::PuffIn {
                    pos,
                    yaw: k.yaw(),
                    slip: k.slip_raw(),
                    current_speed: k.current_speed,
                    kmh: k.kmh(),
                    character: race.setup.cpu_character(c.min(CPU_COUNT - 1)),
                    driving: live,
                    throttle: k.current_speed > 20.,
                    mushroom: r.boost > 0,
                    human: false,
                    drifting: k.drifting,
                    hop: k.hop,
                    midair: k.midair,
                    disabled: spun,
                    drift_state: k.drift_state,
                    drift_duration: k.drift_duration(),
                    tyres: k.back_tyres().map(|t| [t[0] * 8., t[1] * 8., t[2] * 8.]),
                    tyre_grass: k.back_grass(),
                };
                super::kart_fx::FxFrame { pos, yaw: k.yaw(), driving: live, midair: k.midair, spun, puff }
            })
            .collect()
    }

    pub fn spin_angle(&self) -> f64 {
        self.race.as_ref().map_or(0., |r| if r.racers.is_empty() { 0. } else { r.spin_angle(0) })
    }

    pub fn dock(&self, free: Vec<DockItem>, touch: bool) -> Vec<DockItem> {
        let key = |label: &str, key: &str| if touch { label.to_string() } else { format!("{label} · {key}") };
        if self.active() && touch {
            // The touch overlay has the item button; Retry and Free roam are on the results card.
            vec![]
        } else if self.active() {
            vec![DockItem::new(&key("Item", "E"), KeyCode::KeyE, 0), DockItem::new(&key("Retry", "T"), KeyCode::KeyT, 0), DockItem::new(&key("Free roam", "N"), KeyCode::KeyN, 0), DockItem::new(&key("Menu", "M"), KeyCode::KeyM, 0)]
        } else {
            let mut items = free;
            items.push(DockItem::new(&key("Item", "E"), KeyCode::KeyE, 0).essential());
            items.push(DockItem::new(&if touch { "RACE · 3 laps".to_string() } else { "Race · T".to_string() }, KeyCode::KeyT, 0).essential());
            items
        }
    }

    pub fn hud(&self) -> KartHud {
        let Some(race) = self.race.as_ref().filter(|r| !r.racers.is_empty()) else { return KartHud::default() };
        let me = &race.racers[0];
        let racing = race.phase != Phase::Practice;
        let shown = if race.phase == Phase::Countdown { 0 } else { me.finish.unwrap_or(race.race_tick) };
        let (item, item_color, icon) = if me.roulette > 0 {
            ("ROLLING".to_string(), Some(Color::srgb(1., 0.85, 0.3)), kart_hud::roulette_icon(race.clock))
        } else if let Some(i) = me.item {
            let (n, c) = item_style(i);
            (n, Some(c), icon_name(i))
        } else {
            ("ITEM".into(), None, "none")
        };
        let finished_for = me.finish.map(|f| race.race_tick.saturating_sub(f)).filter(|a| *a < 150);
        let banner = match (race.phase, finished_for) {
            (_, Some(a)) if a < 90 => "FINISH!".to_string(),
            (Phase::Racing, _) if race.race_tick < GO_TICKS => "GO!".to_string(),
            _ => String::new(),
        };
        let light = racing.then(|| {
            lakitu_file(&LakituIn {
                countdown: (race.phase == Phase::Countdown).then_some(race.countdown),
                race_tick: race.race_tick,
                finished_for,
                lap: me.lap(),
                lap_start: me.lap_start,
            })
        });
        let fit = self.fit.unwrap_or_else(|| MapFit::from_path(&[]));
        let dots = if racing {
            (0..race.racers.len())
                .map(|i| {
                    if race.is_benched(i) {
                        // Out of the game (a solo Time Trial's CPUs, a battle's eliminated karts): off the map.
                        return (-10., -10.);
                    }
                    let p = if i == 0 { me.last } else { race.cpus.get(i - 1).map_or(me.last, |c| [c.kart.pos[0], c.kart.pos[2]]) };
                    fit.uv(p[0], p[1])
                })
                .collect()
        } else {
            vec![]
        };
        let start = self.path.first().map_or((0., 0.), |p| fit.uv(p[0], p[2]));
        KartHud {
            live: true,
            active: racing,
            icon,
            rank: if racing { race.rank(0) } else { 0 },
            lap: me.lap(),
            ticks: shown,
            order: if racing { race.order() } else { vec![] },
            dots,
            start,
            light: light.flatten(),
            item,
            item_color,
            banner,
            // The modes show their own result panel (standings, records, table) instead of the plain card.
            results: race.results.as_ref().filter(|_| self.flow(|f| f.setup.mode) == super::kart_modes::Mode::FreeRoam).map(|r| card(r, race)),
        }
    }

    pub fn debug(&self) -> String {
        match self.race.as_ref().filter(|r| !r.racers.is_empty()) {
            Some(r) if r.phase == Phase::Practice => format!(
                " items item={:?} roulette={} boost={} spin={} effects={:#x} actors={} boxes={}",
                r.racers[0].item,
                r.racers[0].roulette,
                r.racers[0].boost,
                r.racers[0].spin,
                r.racers[0].fx.flags,
                r.items.actors.iter().filter(|a| a.visible()).count(),
                r.boxes.iter().filter(|b| b.respawn == 0).count()
            ),
            Some(r) => format!(
                " race={:?} tick={} rank={} gates={} item={:?} spin={} boost={} effects={:#x} actors={}",
                r.phase,
                r.race_tick,
                r.rank(0) + 1,
                r.racers[0].passed,
                r.racers[0].item,
                r.racers[0].spin,
                r.racers[0].boost,
                r.racers[0].fx.flags,
                r.items.actors.iter().filter(|a| a.visible()).count()
            ),
            None => String::new(),
        }
    }

    /// Per rendered frame. `eye` is the camera position (sim units); `alpha` interpolates the 30 Hz tick.
    pub fn frame(&mut self, host: &mut Host, eye: DVec3, alpha: f64, ground: &dyn Ground) {
        for t in std::mem::take(&mut self.toasts) {
            host.toast(&t, 1.6);
        }
        let hud = self.hud();
        if self.last_hud.as_ref() != Some(&hud) {
            host.commands.insert_resource(hud.clone());
            self.last_hud = Some(hud);
        }
        let Some(race) = self.race.as_ref().filter(|r| !r.racers.is_empty()) else { return };
        let time = host.time();
        let eye_bevy = to_bevy(eye.into());
        for c in 0..CPU_COUNT {
            let Some(cpu) = race.cpus.get(c).filter(|_| !race.is_benched(c + 1)) else {
                host.commands.entity(self.sprites[c].0).insert(Visibility::Hidden);
                Self::drop_shadow(&self.shadows, host, c + 1);
                continue;
            };
            let k = &cpu.kart;
            let (a, b) = (DVec3::from(self.prev_cpu[c]), DVec3::from(k.pos));
            let p = a.lerp(b, alpha.clamp(0., 1.)) * 8.;
            let input = super::kart_sprite_view::SpriteIn {
                pos: p,
                eye,
                yaw: k.yaw(),
                slip: k.slip_raw(),
                slope_accel: k.slope_accel,
                tilt: k.view_tilt(),
                spin: race.spin_angle(c + 1),
                spinning: race.racers[c + 1].spin > 0,
                midair: k.midair,
                look: super::kart_sprite_view::SpriteLook::from(&race.racers[c + 1].fx.look()),
            };
            let (entity, material) = &self.sprites[c];
            super::kart_sprite_view::place_sprite(host, *entity, material, &input);
            self.shadow(host, c + 1, &input, ground);
        }
        if let Some((entity, material)) = &self.ghost_sprite {
            let pose = (race.phase == Phase::Racing || race.phase == Phase::Finished)
                .then(|| super::kart_flow::with(&self.root, |f| f.ghost_pose(race.race_tick as f64 + alpha)))
                .flatten();
            match pose {
                Some(pose) => {
                    let input = super::kart_sprite_view::SpriteIn {
                        pos: DVec3::from(pose.pos) * 8.,
                        eye,
                        yaw: pose.yaw,
                        slip: pose.slip,
                        slope_accel: 0,
                        tilt: (0., 0.),
                        spin: 0.,
                        spinning: pose.flags & super::kart_ghost::FLAG_SPIN != 0,
                        midair: pose.flags & super::kart_ghost::FLAG_MIDAIR != 0,
                        look: super::kart_sprite_view::SpriteLook { alpha: 0.45, ..Default::default() },
                    };
                    super::kart_sprite_view::place_sprite(host, *entity, material, &input);
                }
                None => {
                    host.commands.entity(*entity).insert(Visibility::Hidden);
                }
            }
        }
        if let Some(pool) = self.boxes.as_mut() {
            for (i, b) in race.boxes.iter().enumerate() {
                let on = b.respawn == 0;
                // MK64: the cube tumbles (x +1, y -2, z +1 degrees per 60 Hz frame).
                let pos = to_bevy([b.pos[0] * 8., b.y * 8. + 72. + (time * 2.4 + i as f64).sin() * 6., b.pos[1] * 8.]);
                let t = Transform::from_translation(pos).with_rotation(Quat::from_euler(EulerRot::XYZ, (time * 1.05) as f32, (-time * 2.1) as f32, (time * 1.05) as f32));
                pool.show(host, i, on, Some(t));
            }
            for i in race.boxes.len()..pool.entities.len() {
                pool.show(host, i, false, None);
            }
        }
        if let Some(pool) = self.marks.as_mut() {
            for (i, b) in race.boxes.iter().enumerate() {
                let on = b.respawn == 0;
                // The '?' turns twice as fast about the vertical axis.
                let pos = to_bevy([b.pos[0] * 8., b.y * 8. + 72. + (time * 2.4 + i as f64).sin() * 6., b.pos[1] * 8.]);
                let t = Transform::from_translation(pos).with_rotation(Quat::from_rotation_y((-time * 4.2) as f32));
                pool.show(host, i, on, Some(t));
            }
            for i in race.boxes.len()..pool.entities.len() {
                pool.show(host, i, false, None);
            }
        }
        // Bananas and shells are upright sprites that turn about the vertical axis to face the camera, with their
        // feet on the track (no pitch, so they never sink into the road).
        let upright = |at: Vec3| {
            let d = Vec3::new(eye_bevy.x - at.x, 0., eye_bevy.z - at.z);
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(d.x.atan2(d.z)))
        };
        // Item actors (`kart_items.rs`): positions are the decomp's (the centre, course units), drawn feet-down.
        use super::kart_items::Kind;
        let visible: Vec<&super::kart_items::Actor> = race.items.actors.iter().filter(|a| a.visible()).collect();
        let bananas: Vec<_> = visible.iter().filter(|a| a.kind == Kind::Banana).collect();
        let shells: Vec<_> = visible.iter().filter(|a| a.is_shell()).collect();
        let fakes: Vec<_> = visible.iter().filter(|a| a.kind == Kind::Fake).collect();
        if let Some(pool) = self.bananas.as_mut() {
            for i in 0..pool.entities.len() {
                let b = bananas.get(i);
                let t = b.map(|b| {
                    let mut t = upright(to_bevy([b.pos[0] * 8., (b.pos[1] - b.radius - 1.) * 8. + 3. + BANANA_SIZE as f64 / 2., b.pos[2] * 8.]));
                    t.rotate_local_z(b.spin as f32);
                    t
                });
                pool.show(host, i, b.is_some(), t);
            }
        }
        if let Some(pool) = self.shells.as_mut() {
            for i in 0..pool.entities.len() {
                let s = shells.get(i);
                let t = s.map(|s| upright(to_bevy([s.pos[0] * 8., (s.pos[1] - s.radius) * 8. + 4. + SHELL_SIZE as f64 / 2., s.pos[2] * 8.])));
                pool.show(host, i, s.is_some(), t);
                if let (Some(s), Some(mut m)) = (s, self.shell_materials.get(i).and_then(|m| host.materials.get_mut(m))) {
                    let strip = match s.kind {
                        Kind::Red => 1,
                        Kind::Blue => 2,
                        _ => 0,
                    };
                    if let Some(tex) = self.shell_strips.get(strip) {
                        if m.base_color_texture.as_ref() != Some(tex) {
                            m.base_color_texture = Some(tex.clone());
                        }
                    }
                    // `render_actor_shell`: the frame follows `rotVelocity` (+10 degrees a frame, 24 per frame step).
                    let frame = ((s.rot_vel as u16 as usize) / (24 * 182)) % 8;
                    m.uv_transform = Affine2::from_scale_angle_translation(Vec2::new(1. / 8., 1.), 0., Vec2::new(frame as f32 / 8., 0.));
                }
            }
        }
        if let (Some(pool), Some(marks)) = (self.fakes.as_mut(), self.fake_marks.as_mut()) {
            for i in 0..pool.entities.len() {
                let f = fakes.get(i);
                let t = f.map(|f| {
                    let size = f.size as f32 * (if f.state == super::kart_items::fake::DESTROYED { 1. + f.timer as f32 * 0.05 } else { 1. });
                    let pos = to_bevy([f.pos[0] * 8., f.pos[1] * 8., f.pos[2] * 8.]);
                    let a = f.rot_angle as f32 * std::f32::consts::PI / 32768.;
                    let b = f.rot_vel as f32 * std::f32::consts::PI / 32768.;
                    (Transform::from_translation(pos).with_rotation(Quat::from_euler(EulerRot::XYZ, a, b, a)).with_scale(Vec3::splat(size)), Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(-b * 2.) * Quat::from_rotation_z(std::f32::consts::PI)).with_scale(Vec3::splat(size)))
                });
                pool.show(host, i, f.is_some(), t.map(|t| t.0));
                marks.show(host, i, f.is_some(), t.map(|t| t.1));
            }
        }
    }
}

impl Default for KartRaceView {
    fn default() -> Self {
        Self::new()
    }
}

fn card(res: &Results, race: &KartRace) -> ResultsCard {
    let mut rows = vec![("Total".to_string(), format!("{}{}", format_ticks(res.total), if res.new_best_total { "  NEW BEST" } else { "" }))];
    for (i, l) in res.laps.iter().enumerate() {
        rows.push((format!("Lap {}", i + 1), format!("{}{}", format_ticks(*l), if *l == res.best_lap { "  best" } else { "" })));
    }
    if res.new_best_lap {
        rows.push(("Best lap".into(), format!("{}  NEW BEST", format_ticks(res.best_lap))));
    }
    ResultsCard {
        place: res.place(),
        rows,
        standings: res.order.iter().map(|&i| Standing { racer: i, time: if i == 0 { Some(res.total) } else { race.racers.get(i).and_then(|r| r.finish) } }).collect(),
    }
}

/// Marker resource inserted when race mode ends so the touch overlay drops its item button.
#[derive(Resource)]
pub struct ModeOff;

pub use super::kart_hud::KartHudPlugin as KartRaceUiPlugin;

/// `JUMPMAN_KART_RACE` test hooks for screenshots: `item` / `banana` / `shell` hand the player that item
/// 0.5 s after GO, `finish` runs the player through all three laps 1.5 s after GO (results card).
fn test_hook(race: &mut KartRace) {
    let Ok(mode) = std::env::var("JUMPMAN_KART_RACE") else { return };
    if race.phase == Phase::Racing && race.race_tick == 15 {
        let item = match mode.as_str() {
            "item" => Some(Item::Mushroom),
            "shell" => Some(Item::GreenShell),
            m => Item::ALL.into_iter().find(|i| i.icon() == m),
        };
        if let Some(i) = item {
            race.give_item(0, i);
        }
    }
    if mode == "finish" && race.phase == Phase::Racing && race.race_tick == 45 {
        for _ in 0..=LAPS {
            for g in 0..super::kart_race::GATES {
                race.race_tick += 12;
                let (c, t) = (race.track.gates[g].c, race.track.gates[g].t);
                race.racers[0].last = [c[0] - t[0] * 10., c[1] - t[1] * 10.];
                race.progress_step(0, [c[0] + t[0] * 10., c[1] + t[1] * 10.]);
            }
        }
    }
}

/// `JUMPMAN_KART_ITEM=mushroom|banana|shell` hands the free-roam player that item 0.5 s in (screenshots);
/// With `JUMPMAN_KART_ITEM_USE=1` the item button is pressed 1.5 s in (shows the thrown banana, shell or the boost).
fn practice_hook(race: &mut KartRace) {
    use super::kart_items::{banana, fake, shell, Kind};
    let Ok(mode) = std::env::var("JUMPMAN_KART_ITEM") else { return };
    if mode == "props" && race.clock == 15 {
        // Screenshot prop: bananas, the three shells and a fake box on the track ahead of the player.
        let hint = race.racers[0].hint;
        let at = |race: &KartRace, d: f64, side: f64, up: f64| {
            let n = race.track.ahead(hint, d);
            let p = race.track.nodes[n];
            let s = race.track.side(n);
            [p[0] + s[0] * side, p[1] + up, p[2] + s[1] * side]
        };
        for (i, d) in [330., 400., 470.].into_iter().enumerate() {
            let p = at(race, d, (i as f64 - 1.) * 70., 3.);
            race.items.place(Kind::Banana, 0, p, banana::ON_GROUND);
        }
        for (i, kind) in [Kind::Green, Kind::Red, Kind::Blue].into_iter().enumerate() {
            let p = at(race, 560., (i as f64 - 1.) * 40., 4.);
            race.items.place(kind, 0, p, 200);
        }
        let p = at(race, 640., 0., 8.66);
        race.items.place(Kind::Fake, 0, p, fake::ON_GROUND);
        let _ = shell::HELD;
    }
    if race.clock == 15 {
        let item = match mode.as_str() {
            "shell" => Some(Item::GreenShell),
            m => Item::ALL.into_iter().find(|i| i.icon() == m),
        };
        if let Some(i) = item {
            race.give_item(0, i);
        }
    }
}
