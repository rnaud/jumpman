//! Presentation for the MK64 race mode: CPU kart sprites (the occupied-kart atlas, tinted), pooled item
//! boxes / bananas / shells, and the bevy_ui HUD (position, lap, time, item chip, countdown banner and
//! the results card). The rules are in `kart_race.rs`; `kart.rs` only calls the small hooks here.
//!
//! The HUD reaches bevy_ui through the [`KartHud`] resource (written with `commands.insert_resource`
//! from the experiment), and the UI hands taps back through atomics ([`take_item_press`] and friends), so
//! neither `Host` nor `XpParams` grew a new field.
use super::dock::DockItem;
use super::kart_physics::{Ground, Input as KartInput, Kart};
use super::kart_race::{format_ticks, load_bests, ordinal, save_bests, Bests, Event, Item, KartRace, Phase, Results, CPU_COUNT, LAPS};
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

/// Everything the HUD shows; `active` is false in free roam.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct KartHud {
    pub active: bool,
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
    bananas: Option<Pooled>,
    shells: Option<Pooled>,
    box_material: Option<Handle<StandardMaterial>>,
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
            bananas: None,
            shells: None,
            box_material: None,
            prev_cpu: [[0.; 3]; CPU_COUNT],
            item_requested: false,
            rescue_requested: false,
            toasts: vec![],
            last_hud: None,
            saved_card: None,
        }
    }
    pub fn active(&self) -> bool {
        self.race.is_some()
    }

    /// Spawn the pooled sprites and item meshes. `atlas` is the kart atlas the player already uses.
    pub fn build(&mut self, host: &mut Host, atlas: Handle<Image>, path: &[V3]) {
        self.path = path.to_vec();
        let quad = host.meshes.add(Rectangle::new(240. * SCALE, 240. * SCALE));
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
        let box_mesh = host.meshes.add(Cuboid::new(64. * SCALE, 64. * SCALE, 64. * SCALE));
        let box_material = host.materials.add(StandardMaterial { base_color: Color::srgba(1., 0.82, 0.29, 0.85), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
        let banana_mesh = host.meshes.add(Capsule3d::new(10. * SCALE, 34. * SCALE));
        let banana_material = host.unlit(Color::srgb(0.95, 0.8, 0.17));
        let shell_mesh = host.meshes.add(Sphere::new(30. * SCALE));
        let shell_material = host.unlit(Color::srgb(0.25, 0.75, 0.27));
        let mut pool = |host: &mut Host, n: usize, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>| {
            Pooled::new((0..n).map(|_| host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden))).collect())
        };
        let mut boxes = pool(host, 12, &box_mesh, &box_material);
        let mut bananas = pool(host, 12, &banana_mesh, &banana_material);
        let mut shells = pool(host, 8, &shell_mesh, &shell_material);
        // The pools start hidden; `shown` mirrors that.
        for p in [&mut boxes, &mut bananas, &mut shells] {
            p.shown.iter_mut().for_each(|s| *s = false);
        }
        self.boxes = Some(boxes);
        self.bananas = Some(bananas);
        self.shells = Some(shells);
        self.box_material = Some(box_material);
    }

    /// Start or restart; the player's kart is replaced on the grid.
    pub fn begin(&mut self, host: &mut Host, phys: &mut Kart, ground: &dyn Ground) {
        let race = self.race.get_or_insert_with(|| {
            let mut r = KartRace::new(&self.path, SEED);
            r.best_lap = self.bests.best_lap;
            r.best_total = self.bests.best_total;
            r
        });
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
        for pool in [self.boxes.as_mut(), self.bananas.as_mut(), self.shells.as_mut()].into_iter().flatten() {
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
            KeyCode::KeyE | KeyCode::ShiftLeft | KeyCode::ShiftRight if self.active() => {
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
        test_hook(race);
        let touch = take_item_press();
        let use_item = std::mem::take(&mut self.item_requested) || item_pad || touch;
        race.tick(pad, use_item, phys, ground);
        for e in race.drain_events() {
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
            items.push(DockItem::new(&key("Race", "T"), KeyCode::KeyT, 0));
            items
        }
    }

    pub fn hud(&self) -> KartHud {
        let Some(race) = self.race.as_ref().filter(|r| !r.racers.is_empty()) else { return KartHud::default() };
        let me = &race.racers[0];
        let shown = if race.phase == Phase::Countdown { 0 } else { me.finish.unwrap_or(race.race_tick) };
        let (item, item_color) = if me.roulette > 0 {
            let (n, c) = item_style(Item::ALL[(race.clock / 3) as usize % 3]);
            (n.to_string(), Some(c))
        } else if let Some(i) = me.item {
            let (n, c) = item_style(i);
            (n.to_string(), Some(c))
        } else {
            ("ITEM".into(), None)
        };
        let banner = match race.banner() {
            Some(0) => "GO!".to_string(),
            Some(n) if race.phase == Phase::Countdown => n.to_string(),
            _ => String::new(),
        };
        KartHud {
            active: true,
            place: ordinal(race.rank(0)).into(),
            lap: format!("LAP {}/{LAPS}", me.lap()),
            time: format_ticks(shown),
            best: race.best_lap.map(|b| format!("BEST {}", format_ticks(b))).unwrap_or_default(),
            item,
            item_color,
            banner,
            results: race.results.as_ref().map(card),
        }
    }

    pub fn debug(&self) -> String {
        match self.race.as_ref().filter(|r| !r.racers.is_empty()) {
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
            let k = &race.cpus[c].kart;
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
            let position = to_bevy((p + DVec3::Y * 100.).into());
            host.commands.entity(*entity).insert((Transform::from_translation(position).looking_to(position - eye_bevy, Vec3::Y), Visibility::Inherited));
            let gem = to_bevy((p + DVec3::Y * 252.).into());
            host.commands.entity(self.markers[c]).insert((
                Transform::from_translation(gem).with_rotation(Quat::from_euler(EulerRot::YXZ, (time * 2.) as f32, 0.6, 0.6)),
                Visibility::Inherited,
            ));
        }
        if let Some(h) = self.box_material.as_ref().and_then(|m| host.materials.get_mut(m)) {
            h.base_color = Color::hsla(((time * 0.4) % 1.) as f32 * 360., 0.9, 0.6, 0.85);
        }
        if let Some(pool) = self.boxes.as_mut() {
            for (i, b) in race.boxes.iter().enumerate() {
                let on = b.respawn == 0;
                let pos = to_bevy([b.pos[0] * 8., b.y * 8. + 70. + (time * 3. + i as f64).sin() * 8., b.pos[1] * 8.]);
                let t = Transform::from_translation(pos).with_rotation(Quat::from_euler(EulerRot::YXZ, (time * 1.9) as f32, (time * 1.3) as f32, 0.));
                pool.show(host, i, on, Some(t));
            }
        }
        if let Some(pool) = self.bananas.as_mut() {
            for i in 0..pool.entities.len() {
                let b = race.bananas.get(i);
                let t = b.map(|b| Transform::from_translation(to_bevy([b.pos[0] * 8., b.y * 8. + 22., b.pos[1] * 8.])).with_rotation(Quat::from_rotation_y((time + i as f64) as f32) * Quat::from_rotation_z(1.2)));
                pool.show(host, i, b.is_some(), t);
            }
        }
        if let Some(pool) = self.shells.as_mut() {
            for i in 0..pool.entities.len() {
                let s = race.shells.get(i);
                let t = s.map(|s| Transform::from_translation(to_bevy([s.pos[0] * 8., s.y * 8. + 36., s.pos[1] * 8.])).with_rotation(Quat::from_rotation_y((time * 12.) as f32)));
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
    if !hud.active {
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
    banner: Query<Entity, With<BannerBox>>,
    results: Query<Entity, With<ResultsBox>>,
    mut last: Local<(String, Option<ResultsCard>)>,
) {
    let Some(ui) = ui else { return };
    if !hud.active {
        for e in &root {
            commands.entity(e).despawn();
        }
        *last = Default::default();
        return;
    }
    if root.is_empty() {
        spawn_root(&mut commands, &ui, &area);
        *last = Default::default();
        return; // The entities exist next frame.
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

fn spawn_root(commands: &mut Commands, ui: &UiAssets, area: &SafeArea) {
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
    commands.entity(root).with_children(|r| {
        r.spawn(Node { column_gap: Val::Px(8.), align_items: AlignItems::Center, ..default() }).with_children(|row| {
            row.spawn((pill.clone(), BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.8)), BorderColor(GOLD), BorderRadius::all(Val::Px(12.)))).with_children(|p| {
                p.spawn((PlaceText, Text::new(""), font(&ui.display, 26.), TextColor(GOLD)));
                p.spawn((LapText, Text::new(""), font(&ui.body, 14.), TextColor(Color::WHITE)));
                p.spawn((TimeText, Text::new(""), font(&ui.body, 14.), TextColor(Color::WHITE)));
                p.spawn((BestText, Text::new(""), font(&ui.body, 10.), TextColor(Color::srgba(1., 0.96, 0.86, 0.6))));
            });
            row.spawn((ItemChip, Node { min_width: Val::Px(96.), justify_content: JustifyContent::Center, ..pill.clone() }, BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.8)), BorderColor(GOLD), BorderRadius::all(Val::Px(12.))))
                .with_children(|p| {
                    p.spawn((ItemText, Text::new("ITEM"), font(&ui.body, 12.), TextColor(Color::WHITE)));
                });
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
