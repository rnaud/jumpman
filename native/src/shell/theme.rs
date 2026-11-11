//! The v3 (N64-era) front-end look on `bevy_ui`: the shared building blocks every shell screen and mode UI is made of.
//! Navy framed panels with a light inner bevel, flat slabs with a hard outline (gold for the chosen/focused row, red BACK,
//! green ENTER) and the title's chunky 5x7 pixel font (`retro::font`, generated at start-up) for every `Text`.
//! Palette and slab colours are the ones of `retro::skin`, so a bevy_ui screen and a framebuffer page match.
use crate::button_marker::{Button, BorderRadius};
use bevy::asset::RenderAssetUsages;
use bevy::color::Mix;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::widget::NodeImageMode;

pub const fn hex(rgb: u32) -> Color {
    Color::srgb(((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0)
}

pub const NAVY: Color = hex(0x050a22);
pub const NAVY_2: Color = hex(0x141c58);
pub const BLUE: Color = hex(0x2f6fe6);
pub const GOLD: Color = hex(0xffcc33);
pub const GOLD_HI: Color = hex(0xfff1a0);
pub const GOLD_LO: Color = hex(0xe08a10);
pub const BROWN: Color = hex(0x3a1a04);
pub const RED: Color = hex(0xe5302a);
pub const RED_HI: Color = hex(0xff7a62);
pub const GREEN: Color = hex(0x2a8f37);
pub const CREAM: Color = hex(0xfff6dc);

/// Loaded fonts, icons and art (`UiAssets`), created at startup.
#[derive(Resource, Clone)]
pub struct UiAssets {
    pub display: Handle<Font>,
    pub body: Handle<Font>,
    pub star: Handle<Image>,
    pub coin: Handle<Image>,
    pub quilt: Handle<Image>,
    /// The scrolling checkerboard of the menu backdrop (procedural, [`checker_pixels`]).
    pub checker: Handle<Image>,
    pub backdrop: Handle<Image>,
    /// Mode card thumbnails (`ui/modes/<key>.jpg`); a mode without a file simply has no entry.
    pub modes: std::collections::HashMap<&'static str, Handle<Image>>,
    /// Header art of the LEVELS tab (`ui/modes/levels.jpg`).
    pub levels: Option<Handle<Image>>,
    /// The gold chevron that points at the focused row (the title's cursor).
    pub chevron: Handle<Image>,
    /// True on the set returned by [`UiAssets::hud`]: the pre-v3 look (rounded glossy pills, the old rounded face) that the
    /// in-game heads-up displays keep. Menus and screens use the v3 set itself.
    pub classic: bool,
    /// The classic set (only present on the v3 one).
    pub(crate) classic_set: Option<Box<UiAssets>>,
}

impl UiAssets {
    /// The assets for in-game heads-up elements built from the theme widgets: the same widgets in their pre-v3 look.
    pub fn hud(&self) -> &UiAssets {
        self.classic_set.as_deref().unwrap_or(self)
    }
}

/// The generated pixel face, for menu `Text` that has no `UiAssets` at hand (set once by [`load_ui_assets`]). It is NOT
/// Bevy's default font: in-game HUD text that names no font keeps the original default.
static PIXEL_FONT: std::sync::OnceLock<Handle<Font>> = std::sync::OnceLock::new();

/// `TextFont` in the menu pixel face (falls back to the default font before the assets are loaded).
pub fn pixel_font(size: f32) -> TextFont {
    match PIXEL_FONT.get() {
        Some(h) => font(h, size),
        None => TextFont { font_size: FontSize::Px(size), ..default() },
    }
}

fn load_font(fonts: &mut Assets<Font>, candidates: &[String]) -> Option<Handle<Font>> {
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = Ok::<Font, ()>(Font::from_bytes(bytes)) {
                return Some(fonts.add(font));
            }
        }
    }
    None
}

pub fn load_ui_assets(mut commands: Commands, assets: Res<AssetServer>, root: Res<crate::render::AssetRoot>, mut fonts: ResMut<Assets<Font>>, mut images: ResMut<Assets<Image>>) {
    // Menus: the title's bitmap font, named explicitly (Bevy's default font is left alone so in-game HUD text is unchanged).
    let pixel = fonts.add(super::retro::font::pixel_font());
    let _ = PIXEL_FONT.set(pixel.clone());
    let (display, body) = (pixel.clone(), pixel);
    // Heads-up displays: the pre-v3 faces (Lilita One / Nunito from the asset folder, else a rounded system face, else Bevy's built-in one).
    let font_dir = root.0.join("ui/fonts");
    let in_dir = |name: &str| font_dir.join(name).to_string_lossy().into_owned();
    let rounded = ["/System/Library/Fonts/Supplemental/Arial Rounded Bold.ttf".to_string(), "C:/Windows/Fonts/ARLRDBD.TTF".to_string()];
    let old_display = load_font(&mut fonts, &[vec![in_dir("LilitaOne-Regular.ttf")], rounded.to_vec()].concat()).unwrap_or_default();
    let old_body = load_font(&mut fonts, &[vec![in_dir("Nunito-ExtraBold.ttf")], rounded.to_vec()].concat()).unwrap_or_default();
    let mut icon = |data| {
        let mut image = Image::new(Extent3d { width: ICON, height: ICON, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        image.sampler = ImageSampler::linear();
        images.add(image)
    };
    let star = icon(star_pixels());
    let coin = icon(coin_pixels());
    let mut checker = Image::new(Extent3d { width: CHECKER_PX, height: CHECKER_PX, depth_or_array_layers: 1 }, TextureDimension::D2, checker_pixels(), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
    checker.sampler = ImageSampler::nearest();
    let checker = images.add(checker);
    let mut modes = std::collections::HashMap::new();
    for id in crate::experiments::EXPERIMENT_IDS {
        let path = format!("ui/modes/{}.jpg", id.key());
        if root.exists(&path) {
            modes.insert(id.key(), assets.load(path));
        }
    }
    let chevron = super::retro::skin::upload(&mut images, &super::retro::skin::draw_chevron(), 2);
    let levels = root.exists("ui/modes/levels.jpg").then(|| assets.load("ui/modes/levels.jpg"));
    let classic = UiAssets { display: old_display, body: old_body, star: star.clone(), coin: coin.clone(), quilt: assets.load("ui/quilt-tile.png"), checker: checker.clone(), backdrop: assets.load("ui/menu-backdrop.png"), modes: modes.clone(), levels: levels.clone(), chevron: chevron.clone(), classic: true, classic_set: None };
    commands.insert_resource(UiAssets { display, body, star, coin, quilt: assets.load("ui/quilt-tile.png"), checker, backdrop: assets.load("ui/menu-backdrop.png"), modes, levels, chevron, classic: false, classic_set: Some(Box::new(classic)) });
}

const ICON: u32 = 64;

fn inside(poly: &[Vec2], p: Vec2) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// Rasterise `shape(p) -> Option<layer>` (0 = outline, 1 = fill) with 3x3 supersampling.
fn raster(shape: impl Fn(Vec2) -> Option<u8>, fill: impl Fn(Vec2) -> [f32; 3], outline: [f32; 3]) -> Vec<u8> {
    let mut data = vec![0u8; (ICON * ICON * 4) as usize];
    for y in 0..ICON {
        for x in 0..ICON {
            let (mut a, mut rgb) = (0.0f32, [0.0f32; 3]);
            for sy in 0..3 {
                for sx in 0..3 {
                    let p = Vec2::new(x as f32 + (sx as f32 + 0.5) / 3.0, y as f32 + (sy as f32 + 0.5) / 3.0) / ICON as f32 * 64.0;
                    if let Some(layer) = shape(p) {
                        let c = if layer == 0 { outline } else { fill(p) };
                        a += 1.0 / 9.0;
                        for k in 0..3 {
                            rgb[k] += c[k] / 9.0;
                        }
                    }
                }
            }
            let i = ((y * ICON + x) * 4) as usize;
            if a > 0.0 {
                for k in 0..3 {
                    data[i + k] = ((rgb[k] / a).clamp(0.0, 1.0) * 255.0) as u8;
                }
                data[i + 3] = (a * 255.0) as u8;
            }
        }
    }
    data
}

fn gold_gradient(p: Vec2) -> [f32; 3] {
    let t = (p.y / 64.0).clamp(0.0, 1.0);
    let (top, mid, bot) = ([1.0, 0.95, 0.63], [1.0, 0.82, 0.23], [0.91, 0.54, 0.07]);
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    if t < 0.45 { lerp(top, mid, t / 0.45) } else { lerp(mid, bot, (t - 0.45) / 0.55) }
}

fn star_pixels() -> Vec<u8> {
    let star = |scale: f32| -> Vec<Vec2> {
        (0..10)
            .map(|i| {
                let r = if i % 2 == 0 { 29.0 } else { 13.0 } * scale;
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                Vec2::new(32.0 + r * a.cos(), 35.0 + r * a.sin())
            })
            .collect()
    };
    let (outer, inner) = (star(1.0), star(0.74));
    raster(|p| if inside(&inner, p) { Some(1) } else if inside(&outer, p) { Some(0) } else { None }, gold_gradient, [0.35, 0.16, 0.0])
}

fn coin_pixels() -> Vec<u8> {
    let ell = |p: Vec2, rx: f32, ry: f32| ((p.x - 32.0) / rx).powi(2) + ((p.y - 32.0) / ry).powi(2) <= 1.0;
    raster(
        |p| {
            if ell(p, 19.0, 25.0) { Some(1) } else if ell(p, 23.0, 29.0) { Some(0) } else { None }
        },
        |p| if (p.x - 32.0).abs() < 3.5 && (p.y - 32.0).abs() < 13.0 { [1.0, 0.95, 0.66] } else { gold_gradient(p) },
        [0.35, 0.16, 0.0],
    )
}

// --------------------------------------------------------------------------------------------
// Widgets
//
// The N64 front-end look: glossy plastic slabs (a vertical gradient face, a light bevel strip
// along the top, a shade strip along the bottom, a dark rim and a hard drop shadow that makes the
// slab stand off the screen), chunky outlined display type, a bouncing star cursor on whatever has
// focus and a scrolling checkerboard behind everything.
// --------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    /// START PLAYING / RESUME: gold with a brown edge.
    Gold,
    /// Character pills, menu actions: blue with a navy edge.
    Blue,
    /// Experiment tiles: green.
    Green,
    /// Dense list entries (courses): deep blue.
    Chip,
    Red,
    /// GAMES mode cards: a bright blue slab.
    Card,
    /// EXPERIMENTS mode cards: a violet slab, so the lab never reads as the games list.
    LabCard,
}

impl ButtonKind {
    /// (face, hover face, selected face, edge, text). Faces are the mid-tone of the skin's slab gradients.
    pub fn palette(self) -> (Color, Color, Color, Color, Color) {
        match self {
            ButtonKind::Gold => (hex(0xf0ac28), hex(0xffc43e), hex(0xffd23a), BROWN, BROWN),
            ButtonKind::Blue => (hex(0x2a3a82), hex(0x3562e8), hex(0xf0ac28), hex(0x040720), hex(0xdfe6ff)),
            ButtonKind::Green => (hex(0x32ac48), hex(0x5cd868), hex(0x8aff8a), hex(0x03220a), Color::WHITE),
            ButtonKind::Chip => (hex(0x1a2466), hex(0x33449a), hex(0xf0ac28), hex(0x040720), hex(0xdfe6ff)),
            ButtonKind::Red => (hex(0xae3226), hex(0xd84a36), hex(0xf06a52), hex(0x2a0603), Color::WHITE),
            ButtonKind::Card => (hex(0x22307a), hex(0x33449a), hex(0x33449a), hex(0x040720), Color::WHITE),
            ButtonKind::LabCard => (hex(0x3a2a82), hex(0x5a42b8), hex(0x7a5ad8), hex(0x0c0630), Color::WHITE),
        }
    }

    /// (light edge, dark edge) of the slab's 1-pixel bevel.
    pub fn bevel_edges(self) -> (Color, Color) {
        match self {
            ButtonKind::Gold => (hex(0xfff1a8), hex(0x8a4a08)),
            ButtonKind::Blue | ButtonKind::Chip => (hex(0x5b70c8), hex(0x0a1040)),
            ButtonKind::Green => (hex(0xc8ffc8), hex(0x0a4a18)),
            ButtonKind::Red => (hex(0xff9a8a), hex(0x500a06)),
            ButtonKind::Card => (hex(0x6f86e0), hex(0x0a0f35)),
            ButtonKind::LabCard => (hex(0x9b7ce0), hex(0x1e0a44)),
        }
    }

    /// Slabs are square: this is a pixel-art front end.
    fn radius(self) -> f32 {
        0.0
    }

    /// The pre-v3 palette (glossy rounded pills), kept for in-game heads-up buttons.
    pub fn classic_palette(self) -> (Color, Color, Color, Color, Color) {
        match self {
            ButtonKind::Gold => (GOLD, hex(0xffe27a), GOLD_HI, CLASSIC_BROWN, CLASSIC_BROWN),
            ButtonKind::Blue => (BLUE, hex(0x5b8ff0), GOLD, CLASSIC_NAVY, Color::WHITE),
            ButtonKind::Green => (GREEN, hex(0x3aa84a), hex(0x66d470), CLASSIC_NAVY, Color::WHITE),
            ButtonKind::Chip => (hex(0x1a3496), hex(0x2f55c8), GOLD, CLASSIC_NAVY, Color::WHITE),
            ButtonKind::Red => (hex(0xc02a24), RED, RED_HI, hex(0x4a0606), Color::WHITE),
            ButtonKind::Card => (hex(0x2349c4), hex(0x3462e0), hex(0x3462e0), CLASSIC_NAVY, Color::WHITE),
            ButtonKind::LabCard => (hex(0x5a2aa8), hex(0x7340c8), hex(0xa06cf2), hex(0x1e0a44), Color::WHITE),
        }
    }

    fn classic_radius(self) -> f32 {
        match self {
            ButtonKind::Chip => 9.0,
            ButtonKind::Card | ButtonKind::LabCard => 12.0,
            _ => 14.0,
        }
    }

    fn palette_in(self, classic: bool) -> (Color, Color, Color, Color, Color) {
        if classic { self.classic_palette() } else { self.palette() }
    }
}

/// The pre-v3 navy / brown inks (the v3 palette darkened [`NAVY`] and [`BROWN`]).
pub const CLASSIC_NAVY: Color = hex(0x0a1640);
pub const CLASSIC_NAVY_2: Color = hex(0x14286e);
pub const CLASSIC_BROWN: Color = hex(0x5a2a00);

/// Mix towards white (`k > 0`) or black (`k < 0`) in sRGB, keeping alpha.
pub fn shade(c: Color, k: f32) -> Color {
    let s = c.to_srgba();
    let (t, f) = if k >= 0.0 { (1.0, k.min(1.0)) } else { (0.0, (-k).min(1.0)) };
    Color::srgba(s.red + (t - s.red) * f, s.green + (t - s.green) * f, s.blue + (t - s.blue) * f, s.alpha)
}

/// The glossy face of a slab: lighter at the top, the base colour through the middle, darker below.
pub fn face_gradient(c: Color) -> BackgroundGradient {
    BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(shade(c, 0.14), 0.0), ColorStop::percent(c, 50.0), ColorStop::percent(shade(c, -0.22), 100.0)]))
}

/// [`face_gradient`] in the pre-v3 (glossier) look.
pub fn face_gradient_in(c: Color, classic: bool) -> BackgroundGradient {
    if !classic {
        return face_gradient(c);
    }
    BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(shade(c, 0.2), 0.0), ColorStop::percent(c, 50.0), ColorStop::percent(shade(c, -0.2), 100.0)]))
}

/// The hard, unblurred drop shadow under slabs, plus the 1-pixel dark outline ring of the skin's slabs.
pub fn slab_shadow(depth: f32) -> BoxShadow {
    BoxShadow(vec![
        ShadowStyle { color: hex(0x040720), x_offset: Val::Px(0.0), y_offset: Val::Px(0.0), spread_radius: Val::Px(1.5), blur_radius: Val::Px(0.0) },
        ShadowStyle { color: Color::srgba(0.01, 0.02, 0.12, 0.85), x_offset: Val::Px(0.0), y_offset: Val::Px(depth), spread_radius: Val::Px(0.0), blur_radius: Val::Px(0.0) },
    ])
}

/// The pre-v3 hard drop shadow (no outline ring).
pub fn classic_slab_shadow(depth: f32) -> BoxShadow {
    BoxShadow::new(Color::srgba(0.01, 0.02, 0.12, 0.85), Val::Px(0.0), Val::Px(depth), Val::Px(0.0), Val::Px(0.0))
}

/// Light bevel along the top edge and a shade along the bottom edge (children of a classic slab).
pub fn classic_bevel(p: &mut ChildSpawnerCommands, inset: f32, light: f32, shade: f32) {
    p.spawn((
        Node { position_type: PositionType::Absolute, top: Val::Px(1.0), left: Val::Px(inset), right: Val::Px(inset), height: Val::Px(3.0), ..default() },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, light)),
        BorderRadius::all(Val::Px(2.0)),
        bevy::ui::FocusPolicy::Pass,
    ));
    p.spawn((
        Node { position_type: PositionType::Absolute, bottom: Val::Px(1.0), left: Val::Px(inset), right: Val::Px(inset), height: Val::Px(3.0), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.1, shade)),
        BorderRadius::all(Val::Px(2.0)),
        bevy::ui::FocusPolicy::Pass,
    ));
}

/// The glossy strips of the old look are gone (the skin's slabs are flat with a 1-pixel bevel drawn as the border), so this
/// draws nothing; the signature stays for callers.
pub fn bevel(_p: &mut ChildSpawnerCommands, _inset: f32, _light: f32, _shade: f32) {}

/// A focusable, clickable thing; `UiAction` says what activating it does.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct StyledButton {
    pub kind: ButtonKind,
    /// Pre-v3 look (in-game heads-up buttons), see [`UiAssets::hud`].
    pub classic: bool,
}

/// A selected/"pressed in" state (current character, current course...).
#[derive(Component)]
pub struct Selected;

/// Marks a button that cannot be activated.
#[derive(Component)]
pub struct Disabled;

/// The face colour currently painted on a slab (so its gradient is only rebuilt on change).
#[derive(Component, Clone, Copy, PartialEq)]
pub struct Painted(pub Color);

pub fn font(handle: &Handle<Font>, size: f32) -> TextFont {
    TextFont { font: FontSource::Handle(handle.clone()), font_size: FontSize::Px(size), ..default() }
}

/// Plain label with a navy drop shadow (default for body text).
pub fn label<'a>(p: &'a mut ChildSpawnerCommands, ui: &UiAssets, text: impl Into<String>, size: f32, color: Color) -> EntityCommands<'a> {
    if ui.classic {
        return p.spawn((Text::new(clean(text.into())), font(&ui.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: CLASSIC_NAVY }));
    }
    p.spawn((Text::new(clean(text.into())), font(&ui.body, size), TextColor(color), TextShadow { offset: Vec2::splat(pixel_of(size)), color: SHADOW_INK }))
}

/// Colour of the hard drop shadow under pixel text (the skin's `SHADOW`).
pub const SHADOW_INK: Color = hex(0x050a22);

/// One pixel of the bitmap font at `size`, in layout px.
pub fn pixel_of(size: f32) -> f32 {
    (size * super::retro::font::PIXEL as f32 / 1000.0).max(1.0)
}

/// Title-style lettering: the face with a dark outline all round and a two-step extruded drop, like the logo's letters.
pub fn outlined(p: &mut ChildSpawnerCommands, ui: &UiAssets, text: &str, size: f32, color: Color) -> Entity {
    outlined_with(p, ui, text, size, color, ())
}

/// [`outlined`] with extra components on the wrapper (animation markers).
pub fn outlined_with(p: &mut ChildSpawnerCommands, ui: &UiAssets, text: &str, size: f32, color: Color, extra: impl Bundle) -> Entity {
    let text = clean(text.to_string());
    if ui.classic {
        let t = (size / 14.0).clamp(1.5, 4.5);
        let deep = hex(0x050b2a);
        return p
            .spawn((Node::default(), extra))
            .with_children(|c| {
                for k in 1..=3 {
                    let dy = t + k as f32 * t * 0.55;
                    for dx in [-t, 0.0, t] {
                        c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(deep), Node { position_type: PositionType::Absolute, left: Val::Px(dx), top: Val::Px(dy), ..default() }));
                    }
                }
                for (dx, dy) in [(t, 0.0), (-t, 0.0), (0.0, t), (0.0, -t), (t, t), (-t, t), (t, -t), (-t, -t)] {
                    c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(CLASSIC_NAVY), Node { position_type: PositionType::Absolute, left: Val::Px(dx), top: Val::Px(dy), ..default() }));
                }
                c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(color)));
            })
            .id();
    }
    let t = (pixel_of(size) * 0.5).round().max(1.0);
    let deep = hex(0x050a22);
    p.spawn((Node::default(), extra))
        .with_children(|c| {
            // Extrusion: dark copies stepping down, then the outline ring, then the face.
            for k in 1..=2 {
                let dy = t + k as f32 * t;
                for dx in [-t, 0.0, t] {
                    c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(deep), Node { position_type: PositionType::Absolute, left: Val::Px(dx), top: Val::Px(dy), ..default() }));
                }
            }
            for (dx, dy) in [(t, 0.0), (-t, 0.0), (0.0, t), (0.0, -t), (t, t), (-t, t), (t, -t), (-t, -t)] {
                c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(deep), Node { position_type: PositionType::Absolute, left: Val::Px(dx), top: Val::Px(dy), ..default() }));
            }
            c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(color)));
        })
        .id()
}

/// The built-in font only covers basic Latin: swap typographic punctuation for ASCII.
pub fn clean(s: String) -> String {
    s.replace(['\u{2019}', '\u{2018}'], "'").replace(['\u{201c}', '\u{201d}'], "\"").replace('\u{2022}', "-").replace(['\u{2013}', '\u{2014}'], "-")
}

pub fn icon(p: &mut ChildSpawnerCommands, image: &Handle<Image>, size: f32) {
    p.spawn((ImageNode::new(image.clone()), Node { width: Val::Px(size), height: Val::Px(size), flex_shrink: 0.0, ..default() }));
}

pub struct ButtonSpec {
    pub kind: ButtonKind,
    pub width: Val,
    pub height: Val,
    pub font_size: f32,
    pub display: bool,
}

impl ButtonSpec {
    pub fn new(kind: ButtonKind, width: Val, height: Val, font_size: f32) -> Self {
        Self { kind, width, height, font_size, display: true }
    }
}

/// The slab components every styled button carries (face, rim, shadow, focus outline).
pub fn slab(kind: ButtonKind, node: Node) -> impl Bundle {
    slab_in(false, kind, node)
}

/// [`slab`] in the v3 look (`classic == false`) or the pre-v3 glossy rounded look the in-game heads-up buttons keep.
pub fn slab_in(classic: bool, kind: ButtonKind, node: Node) -> impl Bundle {
    let (face, _, _, edge, _) = kind.palette_in(classic);
    let (light, dark) = if classic { (edge, edge) } else { kind.bevel_edges() };
    (
        Button,
        StyledButton { kind, classic },
        Node { border: UiRect::all(Val::Px(if classic { 3.0 } else { 2.0 })), flex_shrink: 0.0, ..node },
        BackgroundColor(face),
        face_gradient_in(face, classic),
        Painted(face),
        BorderColor { top: light, left: light, right: dark, bottom: dark },
        BorderRadius::all(Val::Px(if classic { kind.classic_radius() } else { kind.radius() })),
        if classic { classic_slab_shadow(if kind == ButtonKind::Chip { 3.0 } else { 4.0 }) } else { slab_shadow(if kind == ButtonKind::Chip { 2.0 } else { 3.0 }) },
        Outline { width: Val::Px(0.0), offset: Val::Px(2.0), color: Color::WHITE },
    )
}

/// Spawn a chunky bevelled button with a centred label. Returns its entity commands so callers can
/// attach the action / `Selected`.
pub fn button<'a>(p: &'a mut ChildSpawnerCommands, ui: &UiAssets, spec: ButtonSpec, text: &str) -> EntityCommands<'a> {
    let classic = ui.classic;
    let (_, _, _, _, ink) = spec.kind.palette_in(classic);
    let mut e = p.spawn(slab_in(
        classic,
        spec.kind,
        Node { width: spec.width, height: spec.height, padding: UiRect::horizontal(Val::Px(10.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
    ));
    e.with_children(|b| {
        let chip = spec.kind == ButtonKind::Chip;
        if classic {
            classic_bevel(b, if chip { 5.0 } else { 7.0 }, if chip { 0.22 } else { 0.5 }, 0.22);
        } else {
            bevel(b, if chip { 5.0 } else { 7.0 }, if chip { 0.22 } else { 0.5 }, 0.22);
        }
        let mut t = b.spawn((Text::new(clean(text.to_string())), font(if spec.display { &ui.display } else { &ui.body }, spec.font_size), TextColor(ink)));
        if classic {
            if ink != CLASSIC_BROWN {
                t.insert(TextShadow { offset: Vec2::new(0.0, 2.0), color: CLASSIC_NAVY });
            }
        } else if ink != BROWN {
            t.insert(TextShadow { offset: Vec2::splat(pixel_of(spec.font_size)), color: SHADOW_INK });
        }
    });
    e
}

/// The dialog frame of the v3 menus (`#settings`, `.overlay-card`): a navy gradient with a light inner bevel (light top and
/// left, dark bottom and right), a dark outline ring and a hard drop shadow. Square corners, no texture.
pub fn panel<'a>(p: &'a mut ChildSpawnerCommands, ui: &UiAssets, node: Node) -> EntityCommands<'a> {
    if ui.classic {
        // The pre-v3 bright bevelled plastic panel (in-game heads-up panels keep it).
        let mut e = p.spawn((
            Node { border: UiRect::all(Val::Px(4.0)), overflow: Overflow::clip(), overflow_clip_margin: OverflowClipMargin::padding_box(), flex_direction: FlexDirection::Column, ..node },
            BackgroundColor(hex(0x2447c0)),
            BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(hex(0x3d70f0), 0.0), ColorStop::percent(hex(0x2447c0), 55.0), ColorStop::percent(hex(0x172f94), 100.0)])),
            BorderColor { top: hex(0xa9c6ff), left: hex(0x7fa6fa), right: hex(0x0c1a66), bottom: hex(0x0a1450) },
            BorderRadius::all(Val::Px(18.0)),
            BoxShadow(vec![
                ShadowStyle { color: Color::srgba(0.01, 0.02, 0.12, 0.9), x_offset: Val::Px(0.0), y_offset: Val::Px(7.0), spread_radius: Val::Px(0.0), blur_radius: Val::Px(0.0) },
                ShadowStyle { color: Color::srgba(0.0, 0.0, 0.0, 0.35), x_offset: Val::Px(0.0), y_offset: Val::Px(14.0), spread_radius: Val::Px(2.0), blur_radius: Val::Px(18.0) },
            ]),
        ));
        let quilt = ui.quilt.clone();
        e.with_children(|c| {
            c.spawn((
                ImageNode { image: quilt, color: Color::srgba(1.0, 1.0, 1.0, 0.1), image_mode: NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 0.6 }, ..default() },
                Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
            ));
        });
        return e;
    }
    p.spawn((
        Node { border: UiRect::all(Val::Px(3.0)), overflow: Overflow::clip(), overflow_clip_margin: OverflowClipMargin::padding_box(), flex_direction: FlexDirection::Column, ..node },
        BackgroundColor(hex(0x1c2a70)),
        BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(hex(0x2a3a8a), 0.0), ColorStop::percent(hex(0x141c58), 100.0)])),
        BorderColor { top: hex(0x6f86e0), left: hex(0x6f86e0), right: hex(0x0a0f35), bottom: hex(0x0a0f35) },
        BorderRadius::all(Val::Px(0.0)),
        BoxShadow(vec![
            ShadowStyle { color: hex(0x040720), x_offset: Val::Px(0.0), y_offset: Val::Px(0.0), spread_radius: Val::Px(2.0), blur_radius: Val::Px(0.0) },
            ShadowStyle { color: Color::srgba(0.0, 0.0, 0.0, 0.5), x_offset: Val::Px(0.0), y_offset: Val::Px(6.0), spread_radius: Val::Px(0.0), blur_radius: Val::Px(0.0) },
        ]),
    ))
}

/// A recessed tray inside a panel or over the backdrop (the hub's content area): darker than the
/// slabs on it, with the bevel inverted (dark rim on top, light rim below).
pub fn tray(tint: Color) -> impl Bundle {
    (
        BackgroundColor(Color::NONE),
        BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(shade(tint, -0.5).with_alpha(0.95), 0.0), ColorStop::percent(shade(tint, -0.25).with_alpha(0.95), 100.0)])),
        BorderColor { top: hex(0x0a0f35), left: hex(0x0a0f35), right: hex(0x5b70c8), bottom: hex(0x5b70c8) },
        BorderRadius::all(Val::Px(0.0)),
    )
}

/// The small navy pill with gold caps (`.overlay-kicker`).
pub fn kicker(p: &mut ChildSpawnerCommands, ui: &UiAssets, text: &str) {
    if ui.classic {
        p.spawn((
            Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() },
            BackgroundColor(CLASSIC_NAVY_2),
            BorderColor::all(CLASSIC_NAVY),
            BorderRadius::all(Val::Px(99.0)),
            classic_slab_shadow(3.0),
        ))
        .with_children(|k| {
            icon(k, &ui.star, 16.0);
            k.spawn((Text::new(clean(text.to_string())), font(&ui.body, 12.0), TextColor(GOLD)));
        });
        return;
    }
    p.spawn((
        Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() },
        BackgroundColor(NAVY_2),
        BorderColor { top: hex(0x6f86e0), left: hex(0x6f86e0), right: hex(0x0a0f35), bottom: hex(0x0a0f35) },
        BorderRadius::all(Val::Px(0.0)),
        slab_shadow(2.0),
    ))
    .with_children(|k| {
        icon(k, &ui.star, 16.0);
        k.spawn((Text::new(clean(text.to_string())), font(&ui.body, 12.0), TextColor(GOLD)));
    });
}

/// Recolour buttons from hover / pressed / selected / focus state; the focused one pulses (its
/// outline flashes white/gold and it swells a little), like a console cursor.
#[allow(clippy::type_complexity)]
pub fn style_buttons(
    focus: Res<super::nav::Focus>,
    time: Res<Time<Real>>,
    mut buttons: Query<(Entity, &StyledButton, &Interaction, &mut BackgroundColor, Option<&mut Painted>, &mut Outline, &mut UiTransform, Has<Selected>, Has<Disabled>, &Children), With<Button>>,
    mut inks: Query<&mut TextColor>,
    mut commands: Commands,
) {
    let t = time.elapsed_secs();
    let pulse = 0.5 + 0.5 * (t * 7.0).sin();
    for (entity, style, interaction, mut bg, painted, mut outline, mut xf, selected, disabled, children) in &mut buttons {
        let classic = style.classic;
        let (face, hover, chosen, _, ink) = style.kind.palette_in(classic);
        // Selected blue/chip buttons turn gold, so their text flips to dark ink.
        let ink = if selected && matches!(style.kind, ButtonKind::Blue | ButtonKind::Chip) { if classic { CLASSIC_BROWN } else { BROWN } } else { ink };
        for child in children.iter() {
            if let Ok(mut c) = inks.get_mut(child) {
                if c.0 != ink {
                    c.0 = ink;
                }
            }
        }
        let focused = focus.0 == Some(entity);
        let color = if disabled {
            face.with_alpha(0.35)
        } else if selected {
            chosen
        } else if matches!(interaction, Interaction::Hovered | Interaction::Pressed) || focused {
            hover
        } else {
            face
        };
        if bg.0 != color {
            bg.0 = color;
        }
        match painted {
            Some(mut p) if p.0 != color => {
                p.0 = color;
                commands.entity(entity).insert(face_gradient_in(color, classic));
            }
            Some(_) => {}
            None => {
                commands.entity(entity).insert((Painted(color), face_gradient_in(color, classic)));
            }
        }
        let live = focused && !disabled;
        let width = Val::Px(if live { if classic { 3.0 } else { 2.0 } } else { 0.0 });
        if outline.width != width {
            outline.width = width;
        }
        if live {
            outline.color = if classic { Color::WHITE.mix(&GOLD_HI, pulse) } else { GOLD.mix(&GOLD_HI, pulse) };
        }
        let pressed = *interaction == Interaction::Pressed && !disabled;
        let big = matches!(style.kind, ButtonKind::Card | ButtonKind::LabCard);
        let s = if classic {
            if pressed { 0.96 } else if live { 1.0 + if big { 0.018 } else { 0.04 } * pulse } else { 1.0 }
        } else if pressed {
            0.97
        } else {
            1.0
        };
        if (xf.scale.x - s).abs() > 1e-4 {
            xf.scale = Vec2::splat(s);
        }
    }
}

// --------------------------------------------------------------------------------------------
// Motion: the scrolling checkerboard, the bouncing title, pop-in transitions and the star cursor.
// --------------------------------------------------------------------------------------------

/// Side of the checker tile image in px, and how much it is scaled on screen.
pub const CHECKER_PX: u32 = 64;
pub const CHECKER_STRETCH: f32 = 1.25;
/// Size of one repeat of the checkerboard in layout px.
pub const CHECKER_TILE: f32 = CHECKER_PX as f32 * CHECKER_STRETCH;

/// The checkerboard tile: two navy blues, like the hub room's floor.
pub fn checker_pixels() -> Vec<u8> {
    let n = CHECKER_PX as i32;
    let half = n / 2;
    let mut data = vec![0u8; (n * n * 4) as usize];
    let (a, b) = ([0x24, 0x34, 0x86], [0x16, 0x20, 0x5e]);
    for y in 0..n {
        for x in 0..n {
            let light = (x < half) == (y < half);
            let c = if light { a } else { b };
            let i = ((y * n + x) * 4) as usize;
            data[i..i + 3].copy_from_slice(&c);
            data[i + 3] = 255;
        }
    }
    data
}

/// A node that scrolls its tiled image forever (the menu backdrop).
#[derive(Component)]
pub struct ScrollingTiles {
    /// Layout px per second.
    pub speed: Vec2,
}

/// Bobs up and down (title letters): `y = sin(t * rate + phase) * amp`.
#[derive(Component)]
pub struct Wave {
    pub phase: f32,
    pub amp: f32,
}

/// Slowly spins and twinkles (backdrop stars).
#[derive(Component)]
pub struct Twinkle {
    pub phase: f32,
}

/// Springs in from `from` (offset in layout px) and `scale` when a screen or tab opens.
#[derive(Component)]
pub struct PopIn {
    pub t: f32,
    pub from: Vec2,
    pub scale: f32,
}

impl PopIn {
    pub const SECONDS: f32 = 0.2;

    pub fn new(from: Vec2, scale: f32) -> Self {
        Self { t: 0.0, from, scale }
    }

    /// Ease-out with a small overshoot (0 -> 1, peaking just above 1).
    pub fn ease(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0) - 1.0;
        let k = 1.9;
        1.0 + (k + 1.0) * t * t * t + k * t * t
    }
}

/// The checkerboard backdrop with a vertical shading gradient over it. `alpha` < 1 lets the game
/// show through (pause menu).
pub fn backdrop(p: &mut ChildSpawnerCommands, ui: &UiAssets, alpha: f32) {
    let c = CHECKER_TILE;
    p.spawn((
        ImageNode { image: ui.checker.clone(), color: Color::srgba(1.0, 1.0, 1.0, alpha), image_mode: NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: CHECKER_STRETCH }, ..default() },
        Node { position_type: PositionType::Absolute, left: Val::Px(-c), right: Val::Px(-c), top: Val::Px(-c), bottom: Val::Px(-c), ..default() },
        ScrollingTiles { speed: Vec2::new(22.0, 14.0) },
        bevy::ui::FocusPolicy::Pass,
    ));
    p.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
        BackgroundGradient::from(LinearGradient::to_bottom(vec![
            ColorStop::percent(Color::srgba(0.16, 0.04, 0.42, 0.62 * alpha.max(0.6)), 0.0),
            ColorStop::percent(Color::srgba(0.05, 0.1, 0.45, 0.0), 42.0),
            ColorStop::percent(Color::srgba(0.02, 0.04, 0.2, 0.7 * alpha.max(0.6)), 100.0),
        ])),
        bevy::ui::FocusPolicy::Pass,
    ));
}

/// A few big stars drifting over the backdrop (title screen).
pub fn backdrop_stars(p: &mut ChildSpawnerCommands, ui: &UiAssets) {
    for (i, (x, y, size)) in [(6.0, 70.0, 34.0), (88.0, 18.0, 26.0), (94.0, 78.0, 40.0), (48.0, 92.0, 22.0), (30.0, 8.0, 18.0), (70.0, 60.0, 20.0)].into_iter().enumerate() {
        p.spawn((
            ImageNode { image: ui.star.clone(), color: Color::srgba(1.0, 1.0, 1.0, 0.55), ..default() },
            Node { position_type: PositionType::Absolute, left: Val::Percent(x), top: Val::Percent(y), width: Val::Px(size), height: Val::Px(size), ..default() },
            Twinkle { phase: i as f32 * 1.7 },
            bevy::ui::FocusPolicy::Pass,
        ));
    }
}

pub fn animate_menu(
    time: Res<Time<Real>>,
    mut tiles: Query<(&ScrollingTiles, &mut Node)>,
    mut waves: Query<(&Wave, &mut UiTransform), (Without<PopIn>, Without<Twinkle>)>,
    mut stars: Query<(&Twinkle, &mut UiTransform, &mut ImageNode), Without<PopIn>>,
    mut pops: Query<(Entity, &mut PopIn, &mut UiTransform), (Without<Wave>, Without<Twinkle>)>,
    mut commands: Commands,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs().min(0.05);
    for (s, mut node) in &mut tiles {
        let c = CHECKER_TILE;
        let o = Vec2::new((t * s.speed.x).rem_euclid(c), (t * s.speed.y).rem_euclid(c));
        node.left = Val::Px(-c + o.x);
        node.right = Val::Px(-c - o.x);
        node.top = Val::Px(-c + o.y);
        node.bottom = Val::Px(-c - o.y);
    }
    for (w, mut xf) in &mut waves {
        xf.translation = Val2::px(0.0, (t * 3.4 + w.phase).sin() * w.amp);
    }
    for (s, mut xf, mut image) in &mut stars {
        let k = 0.5 + 0.5 * (t * 1.6 + s.phase).sin();
        xf.rotation = Rot2::radians((t * 0.6 + s.phase).sin() * 0.5);
        xf.scale = Vec2::splat(0.8 + 0.3 * k);
        xf.translation = Val2::px((t * 0.4 + s.phase).sin() * 10.0, (t * 0.7 + s.phase).cos() * 8.0);
        image.color = Color::srgba(1.0, 1.0, 1.0, 0.3 + 0.45 * k);
    }
    for (e, mut pop, mut xf) in &mut pops {
        pop.t += dt / PopIn::SECONDS;
        let k = PopIn::ease(pop.t);
        xf.translation = Val2::px(pop.from.x * (1.0 - k), pop.from.y * (1.0 - k));
        xf.scale = Vec2::splat(pop.scale + (1.0 - pop.scale) * k);
        if pop.t >= 1.0 {
            *xf = UiTransform::IDENTITY;
            commands.entity(e).remove::<PopIn>();
        }
    }
}

/// The bouncing star that points at the focused button.
#[derive(Component)]
pub struct MenuCursor;

pub const CURSOR_SIZE: f32 = 28.0;

pub fn spawn_cursor(mut commands: Commands, ui: Res<UiAssets>) {
    commands.spawn((
        MenuCursor,
        super::screens::ShellRoot,
        ImageNode::new(ui.chevron.clone()),
        Node { position_type: PositionType::Absolute, width: Val::Px(CURSOR_SIZE), height: Val::Px(CURSOR_SIZE), ..default() },
        GlobalZIndex(60),
        Visibility::Hidden,
        bevy::ui::FocusPolicy::Pass,
    ));
}

/// Where the cursor star sits for a focused node: just left of it, vertically centred, bobbing
/// sideways; clamped on screen. `rect` is (left, top, width, height) in layout px.
pub fn cursor_spot(rect: Vec4, t: f32) -> Vec2 {
    let bob = (t * 9.0).sin().abs() * 6.0;
    let x = (rect.x - CURSOR_SIZE * 0.62 - bob).max(0.0);
    Vec2::new(x, rect.y + rect.w * 0.5 - CURSOR_SIZE * 0.5)
}

#[allow(clippy::type_complexity)]
pub fn place_cursor(
    focus: Res<super::nav::Focus>,
    state: Res<super::AppState>,
    time: Res<Time<Real>>,
    targets: Query<(&bevy::ui::ComputedNode, &bevy::ui::UiGlobalTransform, &InheritedVisibility), (With<super::UiAction>, Without<MenuCursor>)>,
    mut cursor: Query<(&mut Node, &mut Visibility, &mut UiTransform), With<MenuCursor>>,
    retro: Query<(), With<super::retro::skin::RetroScreen>>,
) {
    let Ok((mut node, mut vis, mut xf)) = cursor.single_mut() else { return };
    // The v3 pages have their own chevron (`retro::skin::place_cursor`).
    if !retro.is_empty() {
        return;
    }
    let target = focus.0.filter(|_| state.in_menu()).and_then(|e| targets.get(e).ok()).filter(|(c, _, v)| v.get() && c.size().x > 0.0);
    let Some((computed, gt, _)) = target else {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        return;
    };
    let inv = computed.inverse_scale_factor();
    let size = computed.size() * inv;
    let centre = gt.translation * inv;
    let t = time.elapsed_secs();
    let spot = cursor_spot(Vec4::new(centre.x - size.x * 0.5, centre.y - size.y * 0.5, size.x, size.y), t);
    let (left, top) = (Val::Px(spot.x), Val::Px(spot.y));
    if node.left != left || node.top != top {
        node.left = left;
        node.top = top;
    }
    xf.rotation = Rot2::IDENTITY;
    if *vis != Visibility::Inherited {
        *vis = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_have_opaque_centres_and_clear_corners() {
        for data in [star_pixels(), coin_pixels()] {
            let px = |x: u32, y: u32| data[((y * ICON + x) * 4 + 3) as usize];
            assert_eq!(px(0, 0), 0);
            assert_eq!(px(63, 63), 0);
            assert_eq!(px(32, 36), 255);
        }
    }

    #[test]
    fn checker_tile_is_opaque_and_two_toned() {
        let data = checker_pixels();
        assert_eq!(data.len(), (CHECKER_PX * CHECKER_PX * 4) as usize);
        assert!(data.chunks(4).all(|p| p[3] == 255));
        let px = |x: u32, y: u32| &data[((y * CHECKER_PX + x) * 4) as usize..((y * CHECKER_PX + x) * 4 + 3) as usize];
        // Opposite quadrants match, neighbours differ: it tiles as a checkerboard.
        assert_eq!(px(1, 1), px(CHECKER_PX - 2, CHECKER_PX - 2));
        assert_ne!(px(1, 1), px(CHECKER_PX - 2, 1));
    }

    #[test]
    fn pop_in_settles_with_a_small_overshoot() {
        assert!(PopIn::ease(0.0).abs() < 1e-5);
        assert!((PopIn::ease(1.0) - 1.0).abs() < 1e-5);
        let peak = (0..=100).map(|i| PopIn::ease(i as f32 / 100.0)).fold(0.0f32, f32::max);
        assert!(peak > 1.0 && peak < 1.15, "{peak}");
    }

    #[test]
    fn cursor_sits_left_of_the_target_and_stays_on_screen() {
        let spot = cursor_spot(Vec4::new(200.0, 100.0, 120.0, 44.0), 0.0);
        assert!(spot.x < 200.0 && spot.x > 200.0 - 2.0 * CURSOR_SIZE);
        assert!((spot.y + CURSOR_SIZE * 0.5 - 122.0).abs() < 1e-3, "vertically centred");
        assert!(cursor_spot(Vec4::new(2.0, 0.0, 50.0, 44.0), 0.3).x >= 0.0);
    }

    #[test]
    fn shade_mixes_towards_white_or_black() {
        let c = hex(0x204080);
        assert_eq!(shade(c, 0.0), c);
        let near = |a: Color, r: f32, g: f32, b: f32| {
            let s = a.to_srgba();
            (s.red - r).abs() < 1e-5 && (s.green - g).abs() < 1e-5 && (s.blue - b).abs() < 1e-5 && (s.alpha - 1.0).abs() < 1e-5
        };
        assert!(near(shade(c, 1.0), 1.0, 1.0, 1.0));
        assert!(near(shade(c, -1.0), 0.0, 0.0, 0.0));
        let half = shade(c, -0.5).to_srgba();
        assert!((half.blue - c.to_srgba().blue * 0.5).abs() < 1e-5);
    }

    #[test]
    fn hex_and_clean() {
        assert_eq!(hex(0xff0000), Color::srgb(1.0, 0.0, 0.0));
        assert_eq!(clean("Whomp\u{2019}s".into()), "Whomp's");
    }
}

#[cfg(test)]
mod hud_look_tests {
    use super::*;

    fn assets(classic: bool, set: Option<Box<UiAssets>>) -> UiAssets {
        UiAssets { display: default(), body: default(), star: default(), coin: default(), quilt: default(), checker: default(), backdrop: default(), modes: Default::default(), levels: None, chevron: default(), classic, classic_set: set }
    }

    #[test]
    fn heads_up_elements_get_the_classic_set_and_menus_the_v3_one() {
        let v3 = assets(false, Some(Box::new(assets(true, None))));
        assert!(!v3.classic, "the resource itself is the v3 menu set");
        assert!(v3.hud().classic, "hud() is the pre-v3 set");
        assert!(v3.hud().hud().classic, "asking the classic set again stays classic");
        assert!(!assets(false, None).hud().classic, "a set without a classic twin falls back to itself");
    }

    #[test]
    fn classic_buttons_keep_the_round_glossy_palette() {
        assert_eq!(ButtonKind::Chip.classic_radius(), 9.0);
        assert_eq!(ButtonKind::Gold.classic_radius(), 14.0);
        assert_ne!(ButtonKind::Blue.classic_palette().0, ButtonKind::Blue.palette().0);
        assert_eq!(ButtonKind::Gold.radius(), 0.0, "v3 slabs are square");
    }
}
