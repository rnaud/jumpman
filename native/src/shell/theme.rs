//! The late-90s console look of `src/ui/theme.css` on `bevy_ui`: palette, fonts, procedural
//! star/coin icons, chunky bevelled buttons/panels and outlined display type.
//!
//! Fonts: `assets/ui/fonts/LilitaOne-Regular.ttf` / `Nunito-ExtraBold.ttf` are used when present
//! (the web build loads them from Google Fonts), otherwise a rounded system face, otherwise Bevy's
//! built-in font. Bevy text has no stroke, so display type fakes the navy outline with offset copies.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::widget::NodeImageMode;

pub const fn hex(rgb: u32) -> Color {
    Color::srgb(((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0)
}

pub const NAVY: Color = hex(0x0a1640);
pub const NAVY_2: Color = hex(0x14286e);
pub const BLUE: Color = hex(0x2f6fe6);
pub const GOLD: Color = hex(0xffcc33);
pub const GOLD_HI: Color = hex(0xfff1a0);
pub const GOLD_LO: Color = hex(0xe08a10);
pub const BROWN: Color = hex(0x5a2a00);
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
    pub backdrop: Handle<Image>,
    /// Mode card thumbnails (`ui/modes/<key>.jpg`); a mode without a file simply has no entry.
    pub modes: std::collections::HashMap<&'static str, Handle<Image>>,
    /// Header art of the LEVELS tab (`ui/modes/levels.jpg`).
    pub levels: Option<Handle<Image>>,
}

fn load_font(fonts: &mut Assets<Font>, candidates: &[String]) -> Option<Handle<Font>> {
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = Font::try_from_bytes(bytes) {
                return Some(fonts.add(font));
            }
        }
    }
    None
}

pub fn load_ui_assets(mut commands: Commands, assets: Res<AssetServer>, root: Res<crate::render::AssetRoot>, mut fonts: ResMut<Assets<Font>>, mut images: ResMut<Assets<Image>>) {
    let font_dir = root.0.join("ui/fonts");
    let in_dir = |name: &str| font_dir.join(name).to_string_lossy().into_owned();
    let rounded = ["/System/Library/Fonts/Supplemental/Arial Rounded Bold.ttf".to_string(), "C:/Windows/Fonts/ARLRDBD.TTF".to_string()];
    let display = load_font(&mut fonts, &[vec![in_dir("LilitaOne-Regular.ttf")], rounded.to_vec()].concat()).unwrap_or_default();
    let body = load_font(&mut fonts, &[vec![in_dir("Nunito-ExtraBold.ttf")], rounded.to_vec()].concat()).unwrap_or_default();
    let mut icon = |data| {
        let mut image = Image::new(Extent3d { width: ICON, height: ICON, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        image.sampler = ImageSampler::linear();
        images.add(image)
    };
    let star = icon(star_pixels());
    let coin = icon(coin_pixels());
    let mut modes = std::collections::HashMap::new();
    for id in crate::experiments::EXPERIMENT_IDS {
        let path = format!("ui/modes/{}.jpg", id.key());
        if root.exists(&path) {
            modes.insert(id.key(), assets.load(path));
        }
    }
    let levels = root.exists("ui/modes/levels.jpg").then(|| assets.load("ui/modes/levels.jpg"));
    commands.insert_resource(UiAssets { display, body, star, coin, quilt: assets.load("ui/quilt-tile.png"), backdrop: assets.load("ui/menu-backdrop.png"), modes, levels });
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
// --------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    /// START PLAYING / RESUME: gold with a brown edge.
    Gold,
    /// Character pills, menu actions: blue with a navy edge.
    Blue,
    /// Experiment tiles: green.
    Green,
    /// Dense list entries (courses): translucent navy.
    Chip,
    Red,
}

impl ButtonKind {
    /// (face, hover face, selected face, edge, text)
    pub fn palette(self) -> (Color, Color, Color, Color, Color) {
        match self {
            ButtonKind::Gold => (GOLD, hex(0xffe27a), GOLD_HI, BROWN, BROWN),
            ButtonKind::Blue => (BLUE, hex(0x5b8ff0), GOLD, NAVY, Color::WHITE),
            ButtonKind::Green => (GREEN, hex(0x3aa84a), hex(0x66d470), NAVY, Color::WHITE),
            ButtonKind::Chip => (hex(0x0c1b50), hex(0x1b3a9a), GOLD, NAVY, Color::WHITE),
            ButtonKind::Red => (hex(0xc02a24), RED, RED_HI, hex(0x4a0606), Color::WHITE),
        }
    }
}

/// A focusable, clickable thing; `UiAction` says what activating it does.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct StyledButton {
    pub kind: ButtonKind,
}

/// A selected/"pressed in" state (current character, current course...).
#[derive(Component)]
pub struct Selected;

/// Marks a button that cannot be activated.
#[derive(Component)]
pub struct Disabled;

pub fn font(handle: &Handle<Font>, size: f32) -> TextFont {
    TextFont { font: handle.clone(), font_size: size, ..default() }
}

/// Plain label with a navy drop shadow (default for body text).
pub fn label<'a>(p: &'a mut ChildSpawnerCommands, ui: &UiAssets, text: impl Into<String>, size: f32, color: Color) -> EntityCommands<'a> {
    p.spawn((Text::new(clean(text.into())), font(&ui.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }))
}

/// Display type with the navy outline + drop (the `--ui-outline-drop` shadow stack).
pub fn outlined(p: &mut ChildSpawnerCommands, ui: &UiAssets, text: &str, size: f32, color: Color) -> Entity {
    let text = clean(text.to_string());
    let t = (size / 14.0).clamp(1.5, 4.0);
    p.spawn(Node::default())
        .with_children(|c| {
            for (dx, dy) in [(t, 0.0), (-t, 0.0), (0.0, t), (0.0, -t), (t, t), (-t, t), (t, -t), (-t, -t), (0.0, t * 1.9)] {
                c.spawn((Text::new(text.clone()), font(&ui.display, size), TextColor(NAVY), Node { position_type: PositionType::Absolute, left: Val::Px(dx), top: Val::Px(dy), ..default() }));
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

/// Spawn a chunky bevelled button with a centred label. Returns its entity commands so callers can
/// attach the action / `Selected`.
pub fn button<'a>(p: &'a mut ChildSpawnerCommands, ui: &UiAssets, spec: ButtonSpec, text: &str) -> EntityCommands<'a> {
    let (face, _, _, edge, ink) = spec.kind.palette();
    let mut e = p.spawn((
        Button,
        StyledButton { kind: spec.kind },
        Node {
            width: spec.width,
            height: spec.height,
            border: UiRect::all(Val::Px(3.0)),
            padding: UiRect::horizontal(Val::Px(10.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(face),
        BorderColor(edge),
        BorderRadius::all(Val::Px(if spec.kind == ButtonKind::Chip { 9.0 } else { 14.0 })),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.55), Val::Px(0.0), Val::Px(4.0), Val::Px(0.0), Val::Px(1.0)),
        Outline { width: Val::Px(0.0), offset: Val::Px(3.0), color: Color::WHITE },
    ));
    e.with_children(|b| {
        // Bevel highlight along the top edge.
        b.spawn((
            Node { position_type: PositionType::Absolute, top: Val::Px(0.0), left: Val::Px(6.0), right: Val::Px(6.0), height: Val::Px(3.0), ..default() },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, if spec.kind == ButtonKind::Chip { 0.12 } else { 0.4 })),
            BorderRadius::all(Val::Px(2.0)),
        ));
        let mut t = b.spawn((Text::new(clean(text.to_string())), font(if spec.display { &ui.display } else { &ui.body }, spec.font_size), TextColor(ink)));
        if matches!(spec.kind, ButtonKind::Blue | ButtonKind::Green | ButtonKind::Red) {
            t.insert(TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY });
        }
    });
    e
}

/// Quilted navy panel with the gold frame (`#settings`, `.overlay-card`).
pub fn panel<'a>(p: &'a mut ChildSpawnerCommands, ui: &UiAssets, node: Node) -> EntityCommands<'a> {
    let mut e = p.spawn((
        Node { border: UiRect::all(Val::Px(4.0)), overflow: Overflow::clip(), overflow_clip_margin: OverflowClipMargin::padding_box(), flex_direction: FlexDirection::Column, ..node },
        BackgroundColor(NAVY_2),
        BorderColor(GOLD),
        BorderRadius::all(Val::Px(22.0)),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), Val::Px(0.0), Val::Px(12.0), Val::Px(4.0), Val::Px(24.0)),
    ));
    let quilt = ui.quilt.clone();
    e.with_children(|c| {
        c.spawn((
            ImageNode { image: quilt, color: Color::srgba(1.0, 1.0, 1.0, 0.35), image_mode: NodeImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 0.6 }, ..default() },
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
        ));
        c.spawn((
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() },
            BackgroundColor(Color::srgba(0.04, 0.09, 0.25, 0.5)),
        ));
    });
    e
}

/// The small navy pill with gold caps (`.overlay-kicker`).
pub fn kicker(p: &mut ChildSpawnerCommands, ui: &UiAssets, text: &str) {
    p.spawn((
        Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(4.0)), border: UiRect::all(Val::Px(2.0)), align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() },
        BackgroundColor(NAVY_2),
        BorderColor(NAVY),
        BorderRadius::all(Val::Px(99.0)),
    ))
    .with_children(|k| {
        icon(k, &ui.star, 16.0);
        k.spawn((Text::new(clean(text.to_string())), font(&ui.body, 12.0), TextColor(GOLD)));
    });
}

/// Recolour buttons from hover / pressed / selected / focus state.
pub fn style_buttons(
    focus: Res<super::nav::Focus>,
    mut buttons: Query<(Entity, &StyledButton, &Interaction, &mut BackgroundColor, &mut Outline, Has<Selected>, Has<Disabled>, &Children), With<Button>>,
    mut inks: Query<&mut TextColor>,
) {
    for (entity, style, interaction, mut bg, mut outline, selected, disabled, children) in &mut buttons {
        let (face, hover, chosen, _, ink) = style.kind.palette();
        // Selected blue/chip buttons turn gold, so their text flips to dark ink.
        let ink = if selected && matches!(style.kind, ButtonKind::Blue | ButtonKind::Chip) { BROWN } else { ink };
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
        let width = Val::Px(if focused && !disabled { 3.0 } else { 0.0 });
        if outline.width != width {
            outline.width = width;
        }
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
    fn hex_and_clean() {
        assert_eq!(hex(0xff0000), Color::srgb(1.0, 0.0, 0.0));
        assert_eq!(clean("Whomp\u{2019}s".into()), "Whomp's");
    }
}
