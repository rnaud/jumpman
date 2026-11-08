//! Presentation for the MK64 items and race mode: CPU kart sprites (the occupied-kart atlas, tinted), pooled item
//! boxes (a translucent rainbow cube tumbling around a spinning '?') / bananas / shells, and the bevy_ui HUD (the
//! item window; in a race also position, lap, time, countdown banner and the results card). Item boxes and items
//! also exist in free roam (the `Practice` phase). The rules are in `kart_race.rs`; `kart.rs` only calls the small
//! hooks here.
//!
//! The HUD reaches bevy_ui through the [`KartHud`] resource (written with `commands.insert_resource`
//! from the experiment), and the UI hands taps back through atomics ([`take_item_press`] and friends), so
//! neither `Host` nor `XpParams` grew a new field.
use super::dock::DockItem;
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
const SHELL_SIZE: f32 = 78.;
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

fn item_style(item: Item) -> (&'static str, Color) {
    match item {
        Item::Mushroom => ("MUSHROOM", Color::srgb(0.9, 0.28, 0.24)),
        Item::Banana => ("BANANA", Color::srgb(0.95, 0.8, 0.17)),
        Item::Shell => ("SHELL", Color::srgb(0.27, 0.71, 0.29)),
    }
}

struct Pooled {
    entities: Vec<Entity>,
    shown: Vec<bool>,
}
fn icon_name(item: Item) -> &'static str {
    match item {
        Item::Mushroom => "mushroom",
        Item::Banana => "banana",
        Item::Shell => "shell",
    }
}
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
    auto: Option<super::kart_race::Driver>,
    auto_ticks: u32,
    shell_materials: Vec<Handle<StandardMaterial>>,
    boxes: Option<Pooled>,
    /// The '?' inside each box.
    marks: Option<Pooled>,
    bananas: Option<Pooled>,
    shells: Option<Pooled>,
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
            auto_ticks: 0,
            shell_materials: vec![],
            boxes: None,
            marks: None,
            bananas: None,
            shells: None,
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
        // Green shell: MK64's 8-frame spinning sprite strip, one material per shell so they spin out of step.
        let shell_mesh = host.meshes.add(Rectangle::new(SHELL_SIZE * SCALE, SHELL_SIZE * SCALE));
        let shell_strip = super::kart::KartExperiment::texture(host, "mk64-green-shell.png");
        self.shell_materials = (0..8)
            .map(|_| host.materials.add(StandardMaterial { base_color_texture: Some(shell_strip.clone()), uv_transform: Affine2::from_scale(Vec2::new(1. / 8., 1.)), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() }))
            .collect();
        let pool = |host: &mut Host, n: usize, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>| {
            Pooled::new((0..n).map(|_| host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden))).collect())
        };
        let mut boxes = pool(host, 24, &cube, &cube_material);
        let mut marks = pool(host, 24, &mark_mesh, &mark_material);
        let mut bananas = pool(host, 12, &banana_mesh, &banana_material);
        let mut shells = Pooled::new(self.shell_materials.iter().map(|m| host.spawn((Mesh3d(shell_mesh.clone()), MeshMaterial3d(m.clone()), Transform::default(), Visibility::Hidden))).collect());
        // The pools start hidden; `shown` mirrors that.
        for p in [&mut boxes, &mut marks, &mut bananas, &mut shells] {
            p.shown.iter_mut().for_each(|s| *s = false);
        }
        self.boxes = Some(boxes);
        self.marks = Some(marks);
        self.bananas = Some(bananas);
        self.shells = Some(shells);
    }

    /// Free roam with item boxes: no countdown, laps or CPUs. The kart stays where it is.
    pub fn begin_practice(&mut self, host: &mut Host, phys: &Kart, ground: &dyn Ground) {
        let mut race = KartRace::new(&self.path, SEED);
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
    }

    /// Start or restart; the player's kart is replaced on the grid.
    pub fn begin(&mut self, host: &mut Host, phys: &mut Kart, ground: &dyn Ground) {
        if self.race.as_ref().is_none_or(|r| r.phase == Phase::Practice) {
            let mut r = KartRace::new(&self.path, SEED);
            r.best_lap = self.bests.best_lap;
            r.best_total = self.bests.best_total;
            self.race = Some(r);
        }
        let race = self.race.as_mut().unwrap();
        race.start(phys, ground);
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
        for e in self.sprites.iter().map(|(e, _)| e).chain(self.shadows.iter().map(|(e, _)| e)) {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
        for pool in [self.boxes.as_mut(), self.marks.as_mut(), self.bananas.as_mut(), self.shells.as_mut()].into_iter().flatten() {
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

    /// One 30 Hz tick of the race. `item_pad` is the gamepad / Shift B edge.
    pub fn step(&mut self, pad: &KartInput, item_pad: bool, phys: &mut Kart, ground: &dyn Ground) {
        let Some(race) = self.race.as_mut() else { return };
        // Test hook: `JUMPMAN_KART_AUTO=1` lets the CPU driver steer the player; `=drift` also hops and holds a drift
        // through every bend (screenshots of the drift sparks and the mini-turbo).
        let auto_pad;
        let pad = if let (Ok(mode), true) = (std::env::var("JUMPMAN_KART_AUTO"), race.phase == Phase::Racing || race.phase == Phase::Practice) {
            let driver = self.auto.get_or_insert_with(|| super::kart_race::Driver::new(&mut super::kart_race::Rng(7), 0));
            let (mut input, _) = driver.input(&race.track, phys, 0., true);
            if mode == "drift" {
                // Drive 2 s, then hop into a right-hand drift and hold it.
                self.auto_ticks += 1;
                if self.auto_ticks >= 60 {
                    input.raw_x = 70;
                    input.hop_down = true;
                    input.hop_pressed = self.auto_ticks == 60;
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
        let use_item = std::mem::take(&mut self.item_requested) || item_pad || touch;
        race.tick(pad, use_item, phys, ground);
        let events = race.drain_events();
        self.smashed = if events.contains(&Event::Pickup(0)) {
            race.boxes.iter().filter(|b| b.respawn == BOX_RESPAWN).map(|b| [b.pos[0] * 8., b.y * 8. + 70., b.pos[1] * 8.]).collect()
        } else {
            vec![]
        };
        self.last_events = events.clone();
        self.hit_fx.clear();
        for e in &events {
            if let Event::Smash(i, item) = e {
                let at = if *i == 0 { phys.pos } else { race.cpus.get(i - 1).map_or(phys.pos, |c| c.kart.pos) };
                self.hit_fx.push(([at[0] * 8., at[1] * 8., at[2] * 8.], if *item == Item::Banana { 0xffe03a } else { 0x4cd964 }));
            }
        }
        for e in events {
            match e {
                Event::Lap(0, n) if n < LAPS => self.toasts.push(if n == LAPS - 1 { "Final lap!".into() } else { format!("Lap {} of {LAPS}", n + 1) }),
                Event::Hit(0) => self.toasts.push("Spun out!".into()),
                Event::Rescue(0) => self.toasts.push("Back on track".into()),
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
                let r = &race.racers[c + 1];
                let pos = [k.pos[0] * 8., k.pos[1] * 8., k.pos[2] * 8.];
                let spun = r.spin > 0;
                let puff = super::kart_puffs::PuffIn {
                    pos,
                    yaw: k.yaw(),
                    slip: k.slip_raw(),
                    current_speed: k.current_speed,
                    kmh: k.kmh(),
                    character: CPU_CHARACTER[c.min(CPU_COUNT - 1)],
                    driving: true,
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
                super::kart_fx::FxFrame { pos, yaw: k.yaw(), driving: true, midair: k.midair, spun, puff }
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
            vec![DockItem::new(&key("Item", "E"), KeyCode::KeyE, 0), DockItem::new(&key("Retry", "T"), KeyCode::KeyT, 0), DockItem::new(&key("Free roam", "N"), KeyCode::KeyN, 0)]
        } else {
            let mut items = free;
            items.push(DockItem::new(&key("Item", "E"), KeyCode::KeyE, 0));
            items.push(DockItem::new(&if touch { "RACE · 3 laps".to_string() } else { "Race · T".to_string() }, KeyCode::KeyT, 0));
            items
        }
    }

    pub fn hud(&self) -> KartHud {
        let Some(race) = self.race.as_ref().filter(|r| !r.racers.is_empty()) else { return KartHud::default() };
        let me = &race.racers[0];
        let racing = race.phase != Phase::Practice;
        let shown = if race.phase == Phase::Countdown { 0 } else { me.finish.unwrap_or(race.race_tick) };
        let (item, item_color, icon) = if me.roulette > 0 {
            let (n, c) = item_style(Item::Mushroom);
            let _ = (n, c);
            ("ROLLING".to_string(), Some(Color::srgb(1., 0.85, 0.3)), kart_hud::roulette_icon(race.clock))
        } else if let Some(i) = me.item {
            let (n, c) = item_style(i);
            (n.to_string(), Some(c), icon_name(i))
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
            results: race.results.as_ref().map(|r| card(r, race)),
        }
    }

    pub fn debug(&self) -> String {
        match self.race.as_ref().filter(|r| !r.racers.is_empty()) {
            Some(r) if r.phase == Phase::Practice => format!(
                " items item={:?} roulette={} boost={} spin={} bananas={} shells={} boxes={}",
                r.racers[0].item,
                r.racers[0].roulette,
                r.racers[0].boost,
                r.racers[0].spin,
                r.bananas.len(),
                r.shells.len(),
                r.boxes.iter().filter(|b| b.respawn == 0).count()
            ),
            Some(r) => format!(
                " race={:?} tick={} rank={} gates={} item={:?} spin={} boost={} bananas={} shells={}",
                r.phase,
                r.race_tick,
                r.rank(0) + 1,
                r.racers[0].passed,
                r.racers[0].item,
                r.racers[0].spin,
                r.racers[0].boost,
                r.bananas.len(),
                r.shells.len()
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
            let Some(cpu) = race.cpus.get(c) else {
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
            };
            let (entity, material) = &self.sprites[c];
            super::kart_sprite_view::place_sprite(host, *entity, material, &input);
            self.shadow(host, c + 1, &input, ground);
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
        if let Some(pool) = self.bananas.as_mut() {
            for i in 0..pool.entities.len() {
                let b = race.bananas.get(i);
                let t = b.map(|b| upright(to_bevy([b.pos[0] * 8., b.y * 8. + 3. + BANANA_SIZE as f64 / 2., b.pos[1] * 8.])));
                pool.show(host, i, b.is_some(), t);
            }
        }
        if let Some(pool) = self.shells.as_mut() {
            for i in 0..pool.entities.len() {
                let s = race.shells.get(i);
                let t = s.map(|s| upright(to_bevy([s.pos[0] * 8., s.y * 8. + 4. + SHELL_SIZE as f64 / 2. + ((time * 14. + i as f64).sin().abs() * 8.), s.pos[1] * 8.])));
                pool.show(host, i, s.is_some(), t);
                if let (Some(_), Some(m)) = (s, self.shell_materials.get(i).and_then(|m| host.materials.get_mut(m))) {
                    // Eight spin frames, 20 per second, each shell a little out of step.
                    let frame = ((time * 20.) as usize + i * 3) % 8;
                    m.uv_transform = Affine2::from_scale_angle_translation(Vec2::new(1. / 8., 1.), 0., Vec2::new(frame as f32 / 8., 0.));
                }
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
        race.racers[0].item = match mode.as_str() {
            "item" => Some(Item::Mushroom),
            "banana" => Some(Item::Banana),
            "shell" => Some(Item::Shell),
            _ => race.racers[0].item,
        };
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
    let Ok(mode) = std::env::var("JUMPMAN_KART_ITEM") else { return };
    if mode == "props" && race.clock == 15 {
        // Screenshot prop: bananas and a shell lying on the track ahead of the player.
        let hint = race.racers[0].hint;
        for (i, d) in [330., 400., 470.].into_iter().enumerate() {
            let n = race.track.ahead(hint, d);
            let p = race.track.nodes[n];
            race.bananas.push(super::kart_race::Banana { pos: [p[0] + (i as f64 - 1.) * 70., p[2]], y: p[1], owner: 1, age: 0 });
        }
        let n = race.track.ahead(hint, 600.);
        let p = race.track.nodes[n];
        race.shells.push(super::kart_race::Shell { pos: [p[0], p[2]], y: p[1], dir: [0., 0.], bounces: 0, life: 9999, owner: 1 });
    }
    if race.clock == 15 {
        race.racers[0].item = match mode.as_str() {
            "mushroom" => Some(Item::Mushroom),
            "banana" => Some(Item::Banana),
            "shell" => Some(Item::Shell),
            _ => race.racers[0].item,
        };
    }
}
