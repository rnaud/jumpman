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
use super::kart_physics::{Ground, Input as KartInput, Kart};
use super::kart_race::{format_ticks, load_bests, ordinal, save_bests, Bests, Event, Item, KartRace, Phase, Results, BOX_RESPAWN, CPU_COUNT, LAPS};
use super::{Host, XpEntity};
use crate::render::{to_bevy, SCALE};
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::touch::{ModeButtons, TouchControls};
use bevy::math::{Affine2, DVec3};
use bevy::prelude::*;
use jumpman_sim::math::Vec3 as V3;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};

const SEED: u32 = 20240607;
/// Edge of the item box cube, world units (10 course units).
const BOX_SIZE: f32 = 80.;
/// MK64 draws the kart as an 18 x 18 course-unit quad: 144 world units.
pub const SPRITE_SIZE: f32 = 144.;
/// Tint, name and results colour of the CPU drivers.
pub const CPU_STYLE: [(&str, [f32; 3]); CPU_COUNT] = [("Luigi", [0.52, 1.0, 0.52]), ("Peach", [1.0, 0.65, 0.83]), ("Toad", [0.62, 0.77, 1.0])];

static ITEM_TAP: AtomicBool = AtomicBool::new(false);
static RETRY_TAP: AtomicBool = AtomicBool::new(false);
static LEAVE_TAP: AtomicBool = AtomicBool::new(false);
/// Item button tapped on the touch overlay since last asked.
pub fn take_item_press() -> bool {
    ITEM_TAP.swap(false, Ordering::Relaxed)
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResultsCard {
    pub title: String,
    pub rows: Vec<(String, String)>,
    pub standings: Vec<(String, Color)>,
}

/// Item icons (scripts/make-kart-items.py) for the HUD, the item box and the world items.
#[derive(Resource, Clone)]
pub struct KartIcons {
    pub mushroom: Handle<Image>,
    pub banana: Handle<Image>,
    pub shell: Handle<Image>,
    pub question: Handle<Image>,
}
impl KartIcons {
    fn get(&self, name: &str) -> Handle<Image> {
        match name {
            "mushroom" => self.mushroom.clone(),
            "banana" => self.banana.clone(),
            "shell" => self.shell.clone(),
            _ => self.question.clone(),
        }
    }
}

/// Everything the HUD shows; `active` (the race row) is false in free roam, `live` (the item window) is true in both.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct KartHud {
    pub live: bool,
    pub active: bool,
    /// Icon of the held / rolling item: mushroom, banana, shell or question.
    pub icon: &'static str,
    pub place: String,
    pub lap: String,
    pub time: String,
    pub best: String,
    pub item: String,
    pub item_color: Option<Color>,
    pub banner: String,
    pub results: Option<ResultsCard>,
}

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
    /// A coloured gem floating over each CPU so the three are easy to tell apart.
    markers: Vec<Entity>,
    boxes: Option<Pooled>,
    /// The '?' inside each box.
    marks: Option<Pooled>,
    bananas: Option<Pooled>,
    shells: Option<Pooled>,
    /// Events drained by the last tick (for sound cues) and the world positions of the boxes the player smashed.
    pub last_events: Vec<Event>,
    pub smashed: Vec<[f64; 3]>,
    prev_cpu: [V3; CPU_COUNT],
    item_requested: bool,
    rescue_requested: bool,
    toasts: Vec<String>,
    last_hud: Option<KartHud>,
    /// The experiment's title card, hidden while racing (it covers a phone screen).
    saved_card: Option<String>,
}

impl KartRaceView {
    pub fn new() -> Self {
        Self {
            race: None,
            path: vec![],
            bests: load_bests(),
            sprites: vec![],
            markers: vec![],
            boxes: None,
            marks: None,
            bananas: None,
            shells: None,
            last_events: vec![],
            smashed: vec![],
            prev_cpu: [[0.; 3]; CPU_COUNT],
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
    pub fn build(&mut self, host: &mut Host, atlas: Handle<Image>, path: &[V3]) {
        self.path = path.to_vec();
        let icons = KartIcons {
            mushroom: super::kart::KartExperiment::texture(host, "item-mushroom.png"),
            banana: super::kart::KartExperiment::texture(host, "item-banana.png"),
            shell: super::kart::KartExperiment::texture(host, "item-shell.png"),
            question: super::kart::KartExperiment::texture(host, "item-question.png"),
        };
        host.commands.insert_resource(icons.clone());
        let quad = host.meshes.add(Rectangle::new(SPRITE_SIZE * SCALE, SPRITE_SIZE * SCALE));
        for (_, tint) in CPU_STYLE {
            let material = host.materials.add(StandardMaterial {
                base_color: Color::srgb(tint[0], tint[1], tint[2]),
                base_color_texture: Some(atlas.clone()),
                unlit: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Mask(0.5),
                ..default()
            });
            let e = host.spawn((Mesh3d(quad.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden));
            self.sprites.push((e, material));
            let gem = host.meshes.add(Cuboid::new(24. * SCALE, 24. * SCALE, 24. * SCALE));
            let gem_material = host.unlit(Color::srgb(tint[0], tint[1], tint[2]));
            self.markers.push(host.spawn((Mesh3d(gem), MeshMaterial3d(gem_material), Transform::default(), Visibility::Hidden)));
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
        let cube_material = host.materials.add(StandardMaterial { base_color: Color::srgba(1., 1., 1., 0.62), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
        let mark_mesh = host.meshes.add(Rectangle::new(BOX_SIZE * 0.66 * SCALE, BOX_SIZE * 0.66 * SCALE));
        let mark_material = host.materials.add(StandardMaterial { base_color_texture: Some(icons.question.clone()), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() });
        let banana_mesh = host.meshes.add(Rectangle::new(84. * SCALE, 84. * SCALE));
        let banana_material = host.materials.add(StandardMaterial { base_color_texture: Some(icons.banana.clone()), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() });
        let shell_mesh = host.meshes.add(Rectangle::new(96. * SCALE, 96. * SCALE));
        let shell_material = host.materials.add(StandardMaterial { base_color_texture: Some(icons.shell.clone()), unlit: true, alpha_mode: AlphaMode::Mask(0.4), cull_mode: None, ..default() });
        let pool = |host: &mut Host, n: usize, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>| {
            Pooled::new((0..n).map(|_| host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden))).collect())
        };
        let mut boxes = pool(host, 24, &cube, &cube_material);
        let mut marks = pool(host, 24, &mark_mesh, &mark_material);
        let mut bananas = pool(host, 12, &banana_mesh, &banana_material);
        let mut shells = pool(host, 8, &shell_mesh, &shell_material);
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
        for e in self.sprites.iter().map(|(e, _)| e).chain(&self.markers) {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
        for pool in [self.boxes.as_mut(), self.marks.as_mut(), self.bananas.as_mut(), self.shells.as_mut()].into_iter().flatten() {
            for i in 0..pool.entities.len() {
                pool.show(host, i, false, None);
            }
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

    pub fn spin_angle(&self) -> f64 {
        self.race.as_ref().map_or(0., |r| if r.racers.is_empty() { 0. } else { r.spin_angle(0) })
    }

    pub fn dock(&self, free: Vec<DockItem>, touch: bool) -> Vec<DockItem> {
        let key = |label: &str, key: &str| if touch { label.to_string() } else { format!("{label} · {key}") };
        if self.active() {
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
            let it = Item::ALL[(race.clock / 3) as usize % 3];
            let (n, c) = item_style(it);
            (n.to_string(), Some(c), icon_name(it))
        } else if let Some(i) = me.item {
            let (n, c) = item_style(i);
            (n.to_string(), Some(c), icon_name(i))
        } else {
            ("ITEM".into(), None, "question")
        };
        let banner = match race.banner() {
            Some(0) => "GO!".to_string(),
            Some(n) if race.phase == Phase::Countdown => n.to_string(),
            _ => String::new(),
        };
        KartHud {
            live: true,
            active: racing,
            icon,
            place: if racing { ordinal(race.rank(0)).into() } else { String::new() },
            lap: if racing { format!("LAP {}/{LAPS}", me.lap()) } else { String::new() },
            time: if racing { format_ticks(shown) } else { String::new() },
            best: if racing { race.best_lap.map(|b| format!("BEST {}", format_ticks(b))).unwrap_or_default() } else { String::new() },
            item,
            item_color,
            banner,
            results: race.results.as_ref().map(card),
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
    pub fn frame(&mut self, host: &mut Host, eye: DVec3, alpha: f64) {
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
                host.commands.entity(self.markers[c]).insert(Visibility::Hidden);
                continue;
            };
            let k = &cpu.kart;
            let (a, b) = (DVec3::from(self.prev_cpu[c]), DVec3::from(k.pos));
            let p = a.lerp(b, alpha.clamp(0., 1.)) * 8.;
            let angle = (eye.x - p.x).atan2(eye.z - p.z) - k.yaw() - PI + race.spin_angle(c + 1);
            let relative = angle.sin().atan2(angle.cos());
            let degrees = relative.abs().to_degrees();
            let frame = (if degrees < 45. { degrees / 2.86 } else { degrees / 9. + 15. }).round().min(34.) as f32;
            let flip = relative < 0.;
            let (entity, material) = &self.sprites[c];
            if let Some(m) = host.materials.get_mut(material) {
                m.uv_transform = Affine2::from_scale_angle_translation(
                    Vec2::new(if flip { -1. / 35. } else { 1. / 35. }, 1.),
                    0.,
                    Vec2::new((frame + if flip { 1. } else { 0. }) / 35., 0.),
                );
            }
            let position = to_bevy((p + DVec3::Y * (SPRITE_SIZE as f64 / 2.)).into());
            host.commands.entity(*entity).insert((Transform::from_translation(position).looking_to(position - eye_bevy, Vec3::Y), Visibility::Inherited));
            let gem = to_bevy((p + DVec3::Y * (SPRITE_SIZE as f64 + 24.)).into());
            host.commands.entity(self.markers[c]).insert((
                Transform::from_translation(gem).with_rotation(Quat::from_euler(EulerRot::YXZ, (time * 2.) as f32, 0.6, 0.6)),
                Visibility::Inherited,
            ));
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
        // Bananas and shells are camera-facing icons standing on the track.
        if let Some(pool) = self.bananas.as_mut() {
            for i in 0..pool.entities.len() {
                let b = race.bananas.get(i);
                let t = b.map(|b| {
                    let at = to_bevy([b.pos[0] * 8., b.y * 8. + 4. + 42., b.pos[1] * 8.]);
                    Transform::from_translation(at).looking_to(at - eye_bevy, Vec3::Y)
                });
                pool.show(host, i, b.is_some(), t);
            }
        }
        if let Some(pool) = self.shells.as_mut() {
            for i in 0..pool.entities.len() {
                let s = race.shells.get(i);
                let t = s.map(|s| {
                    let at = to_bevy([s.pos[0] * 8., s.y * 8. + 6. + 48. + ((time * 14. + i as f64).sin().abs() * 10.), s.pos[1] * 8.]);
                    Transform::from_translation(at).looking_to(at - eye_bevy, Vec3::Y)
                });
                pool.show(host, i, s.is_some(), t);
            }
        }
    }
}

impl Default for KartRaceView {
    fn default() -> Self {
        Self::new()
    }
}

fn card(res: &Results) -> ResultsCard {
    let place = res.place();
    let mut rows = vec![("Total".to_string(), format!("{}{}", format_ticks(res.total), if res.new_best_total { "  NEW BEST" } else { "" }))];
    for (i, l) in res.laps.iter().enumerate() {
        rows.push((format!("Lap {}", i + 1), format!("{}{}", format_ticks(*l), if *l == res.best_lap { "  best" } else { "" })));
    }
    if res.new_best_lap {
        rows.push(("Best lap".into(), format!("{}  NEW BEST", format_ticks(res.best_lap))));
    }
    ResultsCard {
        title: if place == 1 { "1st place!".into() } else { format!("{} place", ordinal(place - 1)) },
        rows,
        standings: res
            .order
            .iter()
            .map(|i| if *i == 0 { ("YOU".to_string(), Color::WHITE) } else { let (n, c) = CPU_STYLE[i - 1]; (n.to_string(), Color::srgb(c[0], c[1], c[2])) })
            .collect(),
    }
}

/// Marker resource inserted when race mode ends so the touch overlay drops its item button.
#[derive(Resource)]
pub struct ModeOff;

pub struct KartRaceUiPlugin;

impl Plugin for KartRaceUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KartHud>().add_systems(Update, (drive_touch, build_ui, buttons).chain());
    }
}

#[derive(Component)]
struct HudRoot;
#[derive(Component)]
enum Act {
    Retry,
    Leave,
}
#[derive(Component)]
struct PlaceText;
#[derive(Component)]
struct LapText;
#[derive(Component)]
struct TimeText;
#[derive(Component)]
struct BestText;
#[derive(Component)]
struct ItemChip;
#[derive(Component)]
struct ItemIcon;
#[derive(Component)]
struct RaceRow;
#[derive(Component)]
struct ItemText;
#[derive(Component)]
struct BannerBox;
#[derive(Component)]
struct ResultsBox;

/// Mirrors the item slot onto the touch overlay's first extra button and forwards its taps.
fn drive_touch(hud: Res<KartHud>, mut mode: ResMut<ModeButtons>, mut tc: ResMut<TouchControls>, off: Option<Res<ModeOff>>, mut commands: Commands) {
    if off.is_some() {
        mode.active = false;
        mode.tint[0] = None;
        commands.remove_resource::<ModeOff>();
    }
    if !hud.live {
        return;
    }
    mode.active = true;
    let label = match hud.item.as_str() {
        "MUSHROOM" => "MUSH",
        "BANANA" => "BANANA",
        "SHELL" => "SHELL",
        _ => "ITEM",
    };
    if mode.labels[0] != label {
        mode.labels[0] = label.into();
    }
    mode.tint[0] = Some(hud.item_color.unwrap_or(Color::srgba(0.09, 0.14, 0.23, 0.74)));
    if tc.mode_pressed & 1 != 0 {
        ITEM_TAP.store(true, Ordering::Relaxed);
    }
    tc.mode_pressed = 0;
}

fn buttons(q: Query<(&Interaction, &Act), Changed<Interaction>>) {
    for (i, a) in &q {
        if *i == Interaction::Pressed {
            match a {
                Act::Retry => RETRY_TAP.store(true, Ordering::Relaxed),
                Act::Leave => LEAVE_TAP.store(true, Ordering::Relaxed),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_ui(
    hud: Res<KartHud>,
    ui: Option<Res<UiAssets>>,
    area: Res<SafeArea>,
    mut commands: Commands,
    root: Query<Entity, With<HudRoot>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<PlaceText>>,
        Query<&mut Text, With<LapText>>,
        Query<&mut Text, With<TimeText>>,
        Query<&mut Text, With<BestText>>,
        Query<&mut Text, With<ItemText>>,
    )>,
    mut chip: Query<&mut BorderColor, With<ItemChip>>,
    mut icon: Query<&mut ImageNode, With<ItemIcon>>,
    mut race_row: Query<&mut Node, With<RaceRow>>,
    icons: Option<Res<KartIcons>>,
    banner: Query<Entity, With<BannerBox>>,
    results: Query<Entity, With<ResultsBox>>,
    mut last: Local<(String, Option<ResultsCard>)>,
) {
    let Some(ui) = ui else { return };
    if !hud.live {
        for e in &root {
            commands.entity(e).despawn();
        }
        *last = Default::default();
        return;
    }
    if root.is_empty() {
        spawn_root(&mut commands, &ui, &area, icons.as_deref());
        *last = Default::default();
        return; // The entities exist next frame.
    }
    for mut node in &mut race_row {
        let want = if hud.active { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    if let Some(icons) = icons.as_deref() {
        for mut image in &mut icon {
            let want = icons.get(hud.icon);
            if image.image != want {
                image.image = want;
            }
        }
    }
    for mut t in &mut texts.p0() {
        if t.0 != hud.place {
            t.0 = hud.place.clone();
        }
    }
    for mut t in &mut texts.p1() {
        if t.0 != hud.lap {
            t.0 = hud.lap.clone();
        }
    }
    for mut t in &mut texts.p2() {
        if t.0 != hud.time {
            t.0 = hud.time.clone();
        }
    }
    for mut t in &mut texts.p3() {
        if t.0 != hud.best {
            t.0 = hud.best.clone();
        }
    }
    for mut t in &mut texts.p4() {
        if t.0 != hud.item {
            t.0 = hud.item.clone();
        }
    }
    for mut b in &mut chip {
        b.0 = hud.item_color.unwrap_or(GOLD);
    }
    if last.0 != hud.banner {
        last.0 = hud.banner.clone();
        for e in &banner {
            commands.entity(e).despawn_related::<Children>();
            if !hud.banner.is_empty() {
                let text = hud.banner.clone();
                commands.entity(e).with_children(|c| {
                    outlined(c, &ui, &text, 96., GOLD);
                });
            }
        }
    }
    if last.1 != hud.results {
        last.1 = hud.results.clone();
        for e in &results {
            commands.entity(e).despawn_related::<Children>();
            match &hud.results {
                Some(card) => {
                    commands.entity(e).insert(Node { display: Display::Flex, ..results_node() }).with_children(|c| fill_results(c, &ui, card));
                }
                None => {
                    commands.entity(e).insert(Node { display: Display::None, ..results_node() });
                }
            }
        }
    }
}

fn results_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::Percent(50.),
        left: Val::Percent(50.),
        margin: UiRect { left: Val::Px(-190.), top: Val::Px(-130.), ..default() },
        width: Val::Px(380.),
        padding: UiRect::axes(Val::Px(18.), Val::Px(12.)),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.),
        border: UiRect::all(Val::Px(3.)),
        ..default()
    }
}

fn spawn_root(commands: &mut Commands, ui: &UiAssets, area: &SafeArea, icons: Option<&KartIcons>) {
    let pill = Node { padding: UiRect::axes(Val::Px(12.), Val::Px(5.)), border: UiRect::all(Val::Px(2.)), align_items: AlignItems::Center, column_gap: Val::Px(10.), ..default() };
    let root = commands
        .spawn((
            XpEntity,
            HudRoot,
            GlobalZIndex(14),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(100.),
                padding: UiRect { left: Val::Px(area.left), right: Val::Px(area.right), top: Val::Px(area.top + 26.), bottom: Val::Px(area.bottom) },
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .id();
    let question = icons.map(|i| i.question.clone());
    commands.entity(root).with_children(|r| {
        // Place, lap and time: only in a race.
        r.spawn((RaceRow, Node { display: Display::None, column_gap: Val::Px(8.), align_items: AlignItems::Center, ..default() })).with_children(|row| {
            row.spawn((pill.clone(), BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.8)), BorderColor(GOLD), BorderRadius::all(Val::Px(12.)))).with_children(|p| {
                p.spawn((PlaceText, Text::new(""), font(&ui.display, 26.), TextColor(GOLD)));
                p.spawn((LapText, Text::new(""), font(&ui.body, 14.), TextColor(Color::WHITE)));
                p.spawn((TimeText, Text::new(""), font(&ui.body, 14.), TextColor(Color::WHITE)));
                p.spawn((BestText, Text::new(""), font(&ui.body, 10.), TextColor(Color::srgba(1., 0.96, 0.86, 0.6))));
            });
        });
        // The item window: always there in free roam and in a race, held item or the roulette.
        let top = area.top + if area.top > 0. { 112. } else { 126. };
        r.spawn((
            ItemChip,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(top),
                right: Val::Px(area.right + 12.),
                min_width: Val::Px(70.),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.),
                padding: UiRect::axes(Val::Px(8.), Val::Px(5.)),
                border: UiRect::all(Val::Px(3.)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.8)),
            BorderColor(GOLD),
            BorderRadius::all(Val::Px(14.)),
        ))
        .with_children(|p| {
            if let Some(q) = question {
                p.spawn((ItemIcon, ImageNode::new(q), Node { width: Val::Px(46.), height: Val::Px(46.), ..default() }));
            }
            p.spawn((ItemText, Text::new("ITEM"), font(&ui.body, 10.), TextColor(Color::WHITE)));
        });
        r.spawn((BannerBox, Node { position_type: PositionType::Absolute, top: Val::Percent(34.), justify_content: JustifyContent::Center, ..default() }));
        r.spawn((ResultsBox, results_node_hidden(), BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.94)), BorderColor(GOLD), BorderRadius::all(Val::Px(18.))));
    });
}

fn results_node_hidden() -> Node {
    Node { display: Display::None, ..results_node() }
}

fn fill_results(c: &mut ChildSpawnerCommands, ui: &UiAssets, card: &ResultsCard) {
    c.spawn(Node { justify_content: JustifyContent::Center, ..default() }).with_children(|t| {
        outlined(t, ui, &card.title, 30., GOLD);
    });
    for (label, value) in &card.rows {
        c.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
            r.spawn((Text::new(clean(label.clone())), font(&ui.body, 14.), TextColor(if label == "Best lap" { GOLD } else { CREAM })));
            r.spawn((Text::new(clean(value.clone())), font(&ui.body, 14.), TextColor(Color::WHITE)));
        });
    }
    c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(14.), margin: UiRect::top(Val::Px(6.)), justify_content: JustifyContent::Center, ..default() }).with_children(|s| {
        for (i, (name, color)) in card.standings.iter().enumerate() {
            s.spawn((Text::new(format!("{}. {}", i + 1, name)), font(&ui.body, 13.), TextColor(*color)));
        }
    });
    c.spawn(Node { column_gap: Val::Px(10.), margin: UiRect::top(Val::Px(8.)), justify_content: JustifyContent::Center, ..default() }).with_children(|b| {
        let spec = |w: f32| ButtonSpec::new(ButtonKind::Blue, Val::Px(w), Val::Px(36.), 14.);
        button(b, ui, spec(90.), "Retry").insert(Act::Retry);
        button(b, ui, spec(170.), "Back to free roam").insert(Act::Leave);
    });
}

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
    if race.clock == 15 {
        race.racers[0].item = match mode.as_str() {
            "mushroom" => Some(Item::Mushroom),
            "banana" => Some(Item::Banana),
            "shell" => Some(Item::Shell),
            _ => race.racers[0].item,
        };
    }
}
