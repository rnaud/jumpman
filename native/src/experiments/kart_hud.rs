//! The MK64 look of the kart HUD: the item window (the real ROM item icons and the rolling roulette), the big
//! ordinal (`mk64-hud-2nd`), `LAP 2/3`, the yellow race clock in MK64's digit font, a racer strip and a minimap with
//! the racers' faces / dots, Lakitu with the start lights, the lap and finish signs, and the results card with racer
//! faces. Every picture is a ROM texture exported by `scripts/export-mk64.py` into `public/mk64`.
//!
//! The pure decisions (which picture for which state) are plain functions with tests; the bevy_ui systems at the
//! bottom only apply them. The race writes a [`KartHud`] resource; taps come back through atomics in
//! `kart_race_view.rs`.
use super::kart_race::LAPS;
use super::XpEntity;
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use super::kart_race_view::ModeOff;
use crate::touch::{ModeButtons, TouchControls, MARGIN_X, MARGIN_Y, PAUSE_SIZE, PAUSE_TOP};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// The racers in grid order: the player is Mario, the CPUs are Luigi, Peach and Toad (MK64 portraits, minimap
/// icons and kart atlases all follow this order).
pub const RACER_NAMES: [&str; 4] = ["Mario", "Luigi", "Peach", "Toad"];
const RACER_FILES: [&str; 4] = ["mario", "luigi", "peach", "toad"];

/// Face of racer `i` (`hud-portrait-<name>.png`, 32 x 32 with its coloured frame).
pub fn portrait_file(i: usize) -> String {
    format!("hud-portrait-{}.png", RACER_FILES[i.min(3)])
}
/// The racer's coloured minimap marker (8 x 8).
pub fn minimap_icon_file(i: usize) -> String {
    format!("hud-minimap-kart-{}.png", RACER_FILES[i.min(3)])
}
/// The directional kart atlas of racer `i`.
pub fn kart_atlas_file(i: usize) -> String {
    if i == 0 { "mario-kart.png".into() } else { format!("{}-kart.png", RACER_FILES[i.min(3)]) }
}
/// MK64's big `1st` ... `8th` (`rank` is 0-based).
pub fn rank_file(rank: usize) -> String {
    format!("mk64-hud-{}.png", ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th"][rank.min(7)])
}
/// Tint of the big ordinal: gold for the win, then silver, bronze, and a muted red for last.
pub fn rank_tint(rank: usize) -> Color {
    match rank {
        0 => Color::srgb(1., 0.86, 0.2),
        1 => Color::srgb(0.88, 0.92, 1.),
        2 => Color::srgb(1., 0.62, 0.3),
        _ => Color::srgb(1., 0.55, 0.5),
    }
}
/// The tiny rank digit beside a racer in the strip (`rank` is 0-based).
pub fn rank_digit_file(rank: usize) -> String {
    format!("hud-rank-rank-tiny-font-{}.png", rank + 1)
}
/// The purple `2/3` sign of the lap counter (`lap` is 1-based).
pub fn lap_file(lap: u32, laps: u32) -> String {
    debug_assert_eq!(laps, 3, "the ROM only has 1/3, 2/3 and 3/3");
    format!("mk64-hud-lap-{}-on-3.png", lap.clamp(1, laps))
}
/// The 40 x 32 item window picture for the held item (`none` is the empty window).
pub fn item_file(icon: &str) -> String {
    let name = match icon {
        "mushroom" => "mushroom",
        "banana" => "banana",
        "shell" => "green-shell",
        _ => "none",
    };
    format!("hud-item-{name}.png")
}
/// What the window shows while the roulette spins: it flicks through the items at 15 Hz, then settles.
pub fn roulette_icon(clock: u32) -> &'static str {
    ["mushroom", "banana", "shell"][(clock / 2) as usize % 3]
}
/// Glyph indices into `mk64-hud-normal-digit.png` (8 x 16 each: 0-9, `'`, `"`) for `m'ss"cc`.
pub fn time_glyphs(ticks: u32) -> [usize; 7] {
    let cs = (ticks as f64 * 100. / 30.).round() as u32;
    let (m, s, c) = ((cs / 6000).min(9) as usize, (cs / 100 % 60) as usize, (cs % 100) as usize);
    [m, 10, s / 10, s % 10, 11, c / 10, c % 10]
}

/// Where the race stands for Lakitu.
#[derive(Clone, Copy, Debug, Default)]
pub struct LakituIn {
    /// Ticks left of the start countdown (90..1).
    pub countdown: Option<u32>,
    pub race_tick: u32,
    /// Ticks since the player crossed the line.
    pub finished_for: Option<u32>,
    pub lap: u32,
    pub lap_start: u32,
}

/// Ticks of the countdown the first and second red light take (the third second is the pause before green).
const LIGHT_TICKS: u32 = 35;
/// Ticks the lap sign stays up.
const SIGN_TICKS: u32 = 90;

/// Lakitu's picture for the moment, if he is on screen: the red lights (one per 35 ticks), the green light at GO, him
/// flying off, the `LAP 2` / `FINAL LAP` signs and the chequered flag at the end.
pub fn lakitu_file(l: &LakituIn) -> Option<String> {
    if let Some(c) = l.countdown {
        let n = 90u32.saturating_sub(c);
        let frame = if n < LIGHT_TICKS { 1 + n * 8 / LIGHT_TICKS } else if n < 2 * LIGHT_TICKS { 9 + (n - LIGHT_TICKS) * 8 / LIGHT_TICKS } else { 16 };
        return Some(format!("lakitu-red-lights{:02}.png", frame.min(16)));
    }
    if let Some(age) = l.finished_for {
        return Some(format!("lakitu-flag-flag{:02}.png", 1 + (age / 2) % 32));
    }
    match l.race_tick {
        0..=29 => return Some(format!("lakitu-blue-light{}.png", 1 + l.race_tick * 8 / 30)),
        30..=61 => return Some(format!("lakitu-none-lights{}.png", 1 + (l.race_tick - 30) * 8 / 32)),
        _ => {}
    }
    let age = l.race_tick.saturating_sub(l.lap_start);
    if l.lap >= 2 && age < SIGN_TICKS {
        let frame = 1 + (age * 16 / SIGN_TICKS).min(15);
        return Some(if l.lap >= LAPS { format!("lakitu-final-lap{frame:02}.png") } else { format!("lakitu-second-lap{frame:02}.png") });
    }
    None
}

/// Minimap placement: course units to the pixels of `hud-minimap-luigi-raceway.png` (64 x 96).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapFit {
    cx: f64,
    cz: f64,
    scale: f64,
}
impl MapFit {
    /// The outline's opaque box is 25 x 79 pixels centred at (31.5, 46.5); the course path is fitted into it.
    pub fn from_path(path: &[[f64; 3]]) -> Self {
        let pts: Vec<_> = path.iter().filter(|p| p[0] > -30000. && p[2] > -30000.).collect();
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in &pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[2])];
            hi = [hi[0].max(p[0]), hi[1].max(p[2])];
        }
        if pts.is_empty() {
            return Self { cx: 0., cz: 0., scale: 1. };
        }
        let scale = (25. / (hi[0] - lo[0]).max(1.)).min(79. / (hi[1] - lo[1]).max(1.));
        Self { cx: (lo[0] + hi[0]) / 2., cz: (lo[1] + hi[1]) / 2., scale }
    }
    /// Position as fractions (0..1) of the outline picture.
    pub fn uv(&self, x: f64, z: f64) -> (f32, f32) {
        (((31.5 + (x - self.cx) * self.scale) / 64.) as f32, ((46.5 + (z - self.cz) * self.scale) / 96.) as f32)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Standing {
    pub racer: usize,
    pub time: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResultsCard {
    pub place: usize,
    pub standings: Vec<Standing>,
    /// Label and value rows (laps, bests).
    pub rows: Vec<(String, String)>,
}

/// Everything the HUD shows; `active` (the race widgets) is false in free roam, `live` (the item window) is true in
/// both.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct KartHud {
    pub live: bool,
    pub active: bool,
    /// Held item (or the roulette frame): `none`, `mushroom`, `banana` or `shell`.
    pub icon: &'static str,
    /// 0-based position of the player.
    pub rank: usize,
    pub lap: u32,
    /// Race clock in ticks (30 Hz).
    pub ticks: u32,
    /// Racers by position, first place first.
    pub order: Vec<usize>,
    /// Minimap fractions per racer index.
    pub dots: Vec<(f32, f32)>,
    pub start: (f32, f32),
    pub light: Option<String>,
    pub item: String,
    pub item_color: Option<Color>,
    pub banner: String,
    pub results: Option<ResultsCard>,
}

#[derive(Component)]
struct HudRoot;
#[derive(Component)]
enum Act {
    Retry,
    Leave,
}
/// Something in the race-only part of the HUD.
#[derive(Component)]
struct RaceRow;
#[derive(Component)]
struct StripRow(usize);
#[derive(Component)]
struct BannerBox;
#[derive(Component)]
struct ResultsBox;
#[derive(Component)]
struct ItemChip;
/// What a HUD picture shows; `update_hud` points each at the right ROM texture.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
enum Pic {
    Item,
    Rank,
    RankShadow,
    Lap,
    Digit(usize),
    StripFace(usize),
    StripRank(usize),
    MapDot(usize),
    Lakitu,
}

const DIGIT_W: f32 = 8.;
const DIGIT_H: f32 = 16.;

pub struct KartHudPlugin;

impl Plugin for KartHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KartHud>().add_systems(Update, (drive_touch, build_ui, apply_taps).chain());
    }
}

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
        super::kart_race_view::press_item();
    }
    tc.mode_pressed = 0;
}

fn apply_taps(q: Query<(&Interaction, &Act), Changed<Interaction>>) {
    for (i, a) in &q {
        if *i == Interaction::Pressed {
            match a {
                Act::Retry => super::kart_race_view::press_retry(),
                Act::Leave => super::kart_race_view::press_leave(),
            }
        }
    }
}

/// A HUD picture, nearest-filtered like the N64's texture sampling for the sprites.
fn pic(server: &AssetServer, file: &str) -> Handle<Image> {
    server.load_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Nearest,
            min_filter: ImageFilterMode::Nearest,
            ..default()
        });
    })
}

/// Layout numbers for a window: `t` logical pixels per texel of the ROM art.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Layout {
    t: f32,
    w: f32,
    h: f32,
    area: [f32; 4],
}
impl Layout {
    fn new(w: f32, h: f32, area: &SafeArea) -> Self {
        Self { t: (h / 402. * 2.0).clamp(1.5, 3.4), w, h, area: [area.left, area.right, area.bottom, area.top] }
    }
    fn left(&self) -> f32 {
        MARGIN_X.max(self.area[0])
    }
    fn top(&self) -> f32 {
        self.area[3] + 8.
    }
    /// Right edge of the clear space left of the pause button.
    fn clear_right(&self) -> f32 {
        MARGIN_X.max(self.area[1]) + PAUSE_SIZE + 10.
    }
    fn below_pause(&self) -> f32 {
        MARGIN_Y.max(self.area[3]) + PAUSE_TOP + PAUSE_SIZE + 6.
    }
}

fn abs() -> Node {
    Node { position_type: PositionType::Absolute, ..default() }
}

fn image(server: &AssetServer, file: &str, w: f32, h: f32) -> (ImageNode, Node) {
    (ImageNode::new(pic(server, file)), Node { width: Val::Px(w), height: Val::Px(h), flex_shrink: 0., ..default() })
}

#[allow(clippy::too_many_arguments)]
fn build_ui(
    hud: Res<KartHud>,
    ui: Option<Res<UiAssets>>,
    area: Res<SafeArea>,
    server: Res<AssetServer>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    root: Query<Entity, With<HudRoot>>,
    mut pics: Query<(&Pic, &mut ImageNode, &mut Node)>,
    mut rows: Query<&mut Node, (With<RaceRow>, Without<Pic>)>,
    mut chips: Query<&mut BorderColor, With<ItemChip>>,
    mut strip: Query<(&StripRow, &mut BackgroundColor)>,
    banner: Query<Entity, With<BannerBox>>,
    results: Query<Entity, With<ResultsBox>>,
    mut state: Local<(String, Option<ResultsCard>, Option<Layout>)>,
) {
    let Some(ui) = ui else { return };
    if !hud.live {
        for e in &root {
            commands.entity(e).despawn();
        }
        *state = Default::default();
        return;
    }
    let size = window.single().map(|w| (w.width(), w.height())).unwrap_or((874., 402.));
    let layout = Layout::new(size.0, size.1, &area);
    if root.is_empty() || state.2 != Some(layout) {
        for e in &root {
            commands.entity(e).despawn();
        }
        spawn_root(&mut commands, &ui, &server, layout);
        *state = (String::new(), None, Some(layout));
        return; // The entities exist next frame.
    }
    for mut node in &mut rows {
        let want = if hud.active && hud.results.is_none() { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    let t = layout.t;
    for (p, mut image, mut node) in &mut pics {
        let want = match *p {
            Pic::Item => item_file(hud.icon),
            Pic::Rank | Pic::RankShadow => rank_file(hud.rank),
            Pic::Lap => lap_file(hud.lap.max(1), LAPS),
            Pic::Digit(i) => {
                let glyph = time_glyphs(hud.ticks)[i];
                image.rect = Some(Rect::new(glyph as f32 * DIGIT_W, 0., (glyph + 1) as f32 * DIGIT_W, DIGIT_H));
                "mk64-hud-normal-digit.png".to_string()
            }
            Pic::StripFace(pos) => portrait_file(hud.order.get(pos).copied().unwrap_or(0)),
            Pic::StripRank(pos) => rank_digit_file(pos),
            Pic::MapDot(i) => {
                if i == 99 {
                    node.left = Val::Px(hud.start.0 * 64. * t * MAP_SCALE - 4. * t * 0.8);
                    node.top = Val::Px(hud.start.1 * 96. * t * MAP_SCALE - 4. * t * 0.8);
                    node.display = Display::Flex;
                    "hud-minimap-finish-line.png".to_string()
                } else if let Some((u, v)) = hud.dots.get(i) {
                    node.left = Val::Px(u * 64. * t * MAP_SCALE - 4. * t * 0.8);
                    node.top = Val::Px(v * 96. * t * MAP_SCALE - 4. * t * 0.8);
                    node.display = Display::Flex;
                    minimap_icon_file(i)
                } else {
                    node.display = Display::None;
                    continue;
                }
            }
            Pic::Lakitu => match &hud.light {
                Some(f) => {
                    node.display = Display::Flex;
                    f.clone()
                }
                None => {
                    node.display = Display::None;
                    continue;
                }
            },
        };
        if matches!(*p, Pic::Rank) {
            image.color = rank_tint(hud.rank);
        } else if matches!(*p, Pic::RankShadow) {
            image.color = Color::srgba(0.05, 0.02, 0.15, 0.85);
        }
        let handle = pic(&server, &want);
        if image.image != handle {
            image.image = handle;
        }
    }
    for (row, mut bg) in &mut strip {
        let want = if hud.order.get(row.0) == Some(&0) { Color::srgba(1., 0.8, 0.2, 0.4) } else { Color::NONE };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for mut b in &mut chips {
        b.0 = hud.item_color.unwrap_or(Color::srgb(0.85, 0.85, 0.9));
    }
    if state.0 != hud.banner {
        state.0 = hud.banner.clone();
        for e in &banner {
            commands.entity(e).despawn_related::<Children>();
            if !hud.banner.is_empty() {
                let text = hud.banner.clone();
                commands.entity(e).with_children(|c| {
                    outlined(c, &ui, &text, 78. * t / 2., GOLD);
                });
            }
        }
    }
    if state.1 != hud.results {
        state.1 = hud.results.clone();
        for e in &results {
            commands.entity(e).despawn_related::<Children>();
            match &hud.results {
                Some(card) => {
                    commands.entity(e).insert(Node { display: Display::Flex, ..results_node() }).with_children(|c| fill_results(c, &ui, &server, card));
                }
                None => {
                    commands.entity(e).insert(Node { display: Display::None, ..results_node() });
                }
            }
        }
    }
}

/// The minimap is drawn at this fraction of the HUD texel size.
const MAP_SCALE: f32 = 0.8;

fn results_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::Percent(50.),
        left: Val::Percent(50.),
        margin: UiRect { left: Val::Px(-215.), top: Val::Px(-150.), ..default() },
        width: Val::Px(430.),
        padding: UiRect::axes(Val::Px(18.), Val::Px(10.)),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.),
        border: UiRect::all(Val::Px(3.)),
        ..default()
    }
}

fn spawn_root(commands: &mut Commands, _ui: &UiAssets, server: &AssetServer, l: Layout) {
    let t = l.t;
    let root = commands.spawn((XpEntity, HudRoot, GlobalZIndex(14), Node { position_type: PositionType::Absolute, width: Val::Percent(100.), height: Val::Percent(100.), ..default() })).id();
    commands.entity(root).with_children(|r| {
        // Item window, top left: the real MK64 frame and icons.
        r.spawn((ItemChip, Node { left: Val::Px(l.left()), top: Val::Px(l.top()), border: UiRect::all(Val::Px(2.)), ..abs() }, BorderColor(Color::srgb(0.85, 0.85, 0.9)), BorderRadius::all(Val::Px(3.))))
            .with_children(|p| {
                p.spawn((Pic::Item, image(server, &item_file("none"), 40. * t, 32. * t)));
            });
        // Racer strip and minimap under it (race only).
        let strip_top = l.top() + 32. * t + 12.;
        let face = 32. * t * 0.52;
        r.spawn((RaceRow, Node { left: Val::Px(l.left()), top: Val::Px(strip_top), flex_direction: FlexDirection::Column, row_gap: Val::Px(2.), display: Display::None, ..abs() })).with_children(|col| {
            for pos in 0..4 {
                col.spawn((StripRow(pos), BackgroundColor(Color::NONE), Node { align_items: AlignItems::Center, column_gap: Val::Px(3.), padding: UiRect::right(Val::Px(2.)), ..default() })).with_children(|row| {
                    row.spawn((Pic::StripRank(pos), image(server, &rank_digit_file(pos), 8. * t * 0.8, 8. * t * 0.8)));
                    row.spawn((Pic::StripFace(pos), image(server, &portrait_file(pos), face, face)));
                });
            }
        });
        let map_left = l.left() + 8. * t * 0.8 + face + 3. + 12.;
        r.spawn((RaceRow, Node { left: Val::Px(map_left), top: Val::Px(strip_top), width: Val::Px(64. * t * MAP_SCALE), height: Val::Px(96. * t * MAP_SCALE), display: Display::None, ..abs() })).with_children(|m| {
            m.spawn((ImageNode::new(pic(server, "hud-minimap-luigi-raceway.png")).with_color(Color::srgba(1., 1., 1., 0.85)), Node { width: Val::Percent(100.), height: Val::Percent(100.), ..abs() }));
            m.spawn((Pic::MapDot(99), ImageNode::new(pic(server, "hud-minimap-finish-line.png")), Node { width: Val::Px(8. * t * 0.8), height: Val::Px(8. * t * 0.8), ..abs() }));
            // Back to front: the CPUs, the player on top.
            for i in [3, 2, 1, 0] {
                m.spawn((Pic::MapDot(i), ImageNode::new(pic(server, &minimap_icon_file(i))), Node { width: Val::Px(8. * t * 0.8), height: Val::Px(8. * t * 0.8), ..abs() }));
            }
        });
        // Clock and lap, top right (left of the pause button).
        r.spawn((RaceRow, Node { right: Val::Px(l.clear_right()), top: Val::Px(l.top()), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: Val::Px(2.), display: Display::None, ..abs() })).with_children(|c| {
            c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.), ..default() }).with_children(|row| {
                row.spawn(image(server, "mk64-hud-lap-time.png", 32. * t * 0.8, 16. * t * 0.8));
                row.spawn((Pic::Lap, image(server, &lap_file(1, LAPS), 32. * t * 0.8, 16. * t * 0.8)));
            });
            c.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.), ..default() }).with_children(|row| {
                row.spawn(image(server, "mk64-hud-time.png", 32. * t * 0.7, 16. * t * 0.7));
                row.spawn(Node { align_items: AlignItems::Center, ..default() }).with_children(|digits| {
                    for i in 0..7 {
                        digits.spawn((Pic::Digit(i), image(server, "mk64-hud-normal-digit.png", DIGIT_W * t * 0.9, DIGIT_H * t * 0.9)));
                    }
                });
            });
        });
        // The big ordinal, right side under the pause button.
        r.spawn((RaceRow, Node { right: Val::Px(MARGIN_X.max(l.area[1])), top: Val::Px(l.below_pause()), display: Display::None, ..abs() })).with_children(|c| {
            let (rw, rh) = (128. * t * 0.5, 64. * t * 0.5);
            c.spawn((Pic::RankShadow, ImageNode::new(pic(server, &rank_file(0))), Node { left: Val::Px(t * 1.6), top: Val::Px(t * 1.6), width: Val::Px(rw), height: Val::Px(rh), ..abs() }));
            c.spawn((Pic::Rank, image(server, &rank_file(0), rw, rh)));
        });
        // Lakitu, top centre.
        let (lw, lh) = (56. * t * 0.95, 72. * t * 0.95);
        r.spawn((Pic::Lakitu, ImageNode::new(pic(server, "lakitu-red-lights01.png")), Node { width: Val::Px(lw), height: Val::Px(lh), left: Val::Percent(50.), margin: UiRect::left(Val::Px(-lw / 2.)), top: Val::Px(l.top() - 4.), display: Display::None, ..abs() }));
        r.spawn((BannerBox, Node { top: Val::Percent(40.), width: Val::Percent(100.), justify_content: JustifyContent::Center, ..abs() }));
        r.spawn((ResultsBox, Node { display: Display::None, ..results_node() }, BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.94)), BorderColor(GOLD), BorderRadius::all(Val::Px(18.))));
    });
}

fn fill_results(c: &mut ChildSpawnerCommands, ui: &UiAssets, server: &AssetServer, card: &ResultsCard) {
    c.spawn(Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, column_gap: Val::Px(10.), ..default() }).with_children(|t| {
        t.spawn((ImageNode::new(pic(server, &rank_file(card.place.saturating_sub(1)))).with_color(rank_tint(card.place.saturating_sub(1))), Node { width: Val::Px(96.), height: Val::Px(48.), ..default() }));
        outlined(t, ui, if card.place == 1 { "You win!" } else { "Finished" }, 24., GOLD);
    });
    c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(18.), ..default() }).with_children(|body| {
        // Standings: place, face, name, time.
        body.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.), flex_grow: 1., ..default() }).with_children(|col| {
            for (i, s) in card.standings.iter().enumerate() {
                col.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(7.), padding: UiRect::axes(Val::Px(4.), Val::Px(1.)), ..default() }).insert(BackgroundColor(if s.racer == 0 { Color::srgba(1., 0.8, 0.2, 0.22) } else { Color::NONE })).with_children(|row| {
                    row.spawn((ImageNode::new(pic(server, &rank_digit_file(i))), Node { width: Val::Px(18.), height: Val::Px(18.), ..default() }));
                    row.spawn((ImageNode::new(pic(server, &portrait_file(s.racer))), Node { width: Val::Px(30.), height: Val::Px(30.), ..default() }));
                    row.spawn(Node { width: Val::Px(58.), ..default() }).with_children(|n| {
                        n.spawn((Text::new(clean(if s.racer == 0 { "YOU".to_string() } else { RACER_NAMES[s.racer.min(3)].to_uppercase() })), font(&ui.body, 13.), TextColor(if s.racer == 0 { GOLD } else { CREAM })));
                    });
                    let time = s.time.map_or("--:--.--".to_string(), super::kart_race::format_ticks);
                    row.spawn((Text::new(time), font(&ui.body, 13.), TextColor(Color::WHITE)));
                });
            }
        });
        body.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(3.), min_width: Val::Px(150.), ..default() }).with_children(|col| {
            for (label, value) in &card.rows {
                col.spawn(Node { justify_content: JustifyContent::SpaceBetween, column_gap: Val::Px(8.), ..default() }).with_children(|r| {
                    r.spawn((Text::new(clean(label.clone())), font(&ui.body, 12.), TextColor(if label.starts_with("Best") { GOLD } else { CREAM })));
                    r.spawn((Text::new(clean(value.clone())), font(&ui.body, 12.), TextColor(Color::WHITE)));
                });
            }
        });
    });
    c.spawn(Node { column_gap: Val::Px(10.), margin: UiRect::top(Val::Px(6.)), justify_content: JustifyContent::Center, ..default() }).with_children(|b| {
        let spec = |w: f32| ButtonSpec::new(ButtonKind::Blue, Val::Px(w), Val::Px(34.), 14.);
        button(b, ui, spec(90.), "Retry").insert(Act::Retry);
        button(b, ui, spec(170.), "Back to free roam").insert(Act::Leave);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_use_the_rom_pictures() {
        assert_eq!(rank_file(0), "mk64-hud-1st.png");
        assert_eq!(rank_file(1), "mk64-hud-2nd.png");
        assert_eq!(rank_file(3), "mk64-hud-4th.png");
        assert_eq!(rank_digit_file(2), "hud-rank-rank-tiny-font-3.png");
    }

    #[test]
    fn lap_text_follows_the_lap() {
        assert_eq!(lap_file(1, 3), "mk64-hud-lap-1-on-3.png");
        assert_eq!(lap_file(3, 3), "mk64-hud-lap-3-on-3.png");
        assert_eq!(lap_file(9, 3), "mk64-hud-lap-3-on-3.png", "never past the last lap");
    }

    #[test]
    fn item_icons_map_to_the_window_pictures() {
        assert_eq!(item_file("mushroom"), "hud-item-mushroom.png");
        assert_eq!(item_file("banana"), "hud-item-banana.png");
        assert_eq!(item_file("shell"), "hud-item-green-shell.png");
        assert_eq!(item_file("none"), "hud-item-none.png");
        assert_eq!(item_file("?"), "hud-item-none.png");
    }

    #[test]
    fn the_roulette_cycles_through_every_item() {
        let seen: std::collections::BTreeSet<_> = (0..12).map(roulette_icon).collect();
        assert_eq!(seen.len(), 3);
        assert_ne!(roulette_icon(0), roulette_icon(2));
    }

    #[test]
    fn racers_have_distinct_faces_markers_and_karts() {
        let faces: std::collections::BTreeSet<_> = (0..4).map(portrait_file).collect();
        let dots: std::collections::BTreeSet<_> = (0..4).map(minimap_icon_file).collect();
        let karts: std::collections::BTreeSet<_> = (0..4).map(kart_atlas_file).collect();
        assert_eq!((faces.len(), dots.len(), karts.len()), (4, 4, 4));
        assert_eq!(kart_atlas_file(0), "mario-kart.png");
        assert_eq!(portrait_file(0), "hud-portrait-mario.png");
    }

    #[test]
    fn the_clock_is_minutes_seconds_hundredths_in_digit_glyphs() {
        // 30 Hz: 30 ticks = 1 s; 2475 ticks = 82.5 s = 1'22"50.
        assert_eq!(time_glyphs(2475), [1, 10, 2, 2, 11, 5, 0]);
        assert_eq!(time_glyphs(0), [0, 10, 0, 0, 11, 0, 0]);
        assert_eq!(time_glyphs(15), [0, 10, 0, 0, 11, 5, 0]);
    }

    #[test]
    fn lakitu_shows_two_red_lights_then_green_then_leaves() {
        let at = |c| lakitu_file(&LakituIn { countdown: Some(c), ..Default::default() }).unwrap();
        assert_eq!(at(90), "lakitu-red-lights01.png");
        assert_eq!(at(56), "lakitu-red-lights08.png");
        assert_eq!(at(55), "lakitu-red-lights09.png");
        assert_eq!(at(20), "lakitu-red-lights16.png");
        let go = |t| lakitu_file(&LakituIn { race_tick: t, ..Default::default() });
        assert_eq!(go(0).unwrap(), "lakitu-blue-light1.png");
        assert_eq!(go(29).unwrap(), "lakitu-blue-light8.png");
        assert_eq!(go(30).unwrap(), "lakitu-none-lights1.png");
        assert_eq!(go(61).unwrap(), "lakitu-none-lights8.png");
        assert_eq!(go(62), None);
    }

    #[test]
    fn lakitu_announces_the_second_and_final_lap_and_waves_the_flag() {
        let lap = |lap, now, start| lakitu_file(&LakituIn { race_tick: now, lap, lap_start: start, ..Default::default() });
        assert_eq!(lap(2, 1000, 950).unwrap(), "lakitu-second-lap09.png");
        assert!(lap(3, 2000, 1950).unwrap().starts_with("lakitu-final-lap"));
        assert_eq!(lap(3, 2100, 1950), None, "the sign leaves after three seconds");
        assert_eq!(lap(1, 300, 0), None);
        let flag = lakitu_file(&LakituIn { race_tick: 3000, finished_for: Some(5), ..Default::default() }).unwrap();
        assert_eq!(flag, "lakitu-flag-flag03.png");
    }

    #[test]
    fn the_minimap_puts_the_course_inside_the_outline_box() {
        let path = [[-1677., 0., -3316.], [-135., 0., 1699.], [-900., 0., 0.], [-32768., 0., -32768.]];
        let fit = MapFit::from_path(&path);
        let (u0, v0) = fit.uv(-1677., -3316.);
        let (u1, v1) = fit.uv(-135., 1699.);
        // The opaque box of the ROM outline is x 19..44, y 7..86 of 64 x 96.
        assert!((u0 * 64. - 19.).abs() < 1.2 && (u1 * 64. - 44.).abs() < 1.2, "{u0} {u1}");
        assert!((v0 * 96. - 7.).abs() < 1.2 && (v1 * 96. - 86.).abs() < 1.2, "{v0} {v1}");
    }
}
