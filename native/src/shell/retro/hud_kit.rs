//! The in-game HUD kit: the v3 menu look (framed navy panels with a light inner edge, the chunky 5x7 bitmap font with a dark
//! outline and drop shadow, gold highlights, red/green/blue slabs, pixel icons) as small `bevy_ui` pieces, so every heads-up
//! display in the game shares one set of frames, counters, bars and buttons.
//!
//! * [`PixelText`]: a string rasterised with the title's font (self-sizing image node, re-rendered when it changes).
//! * [`HudKit`]: a system parameter that hands out nine-sliced plates ([`Plate`]), icons ([`Icon`]) and slab faces.
//! * [`BarFill`] / [`bar`]: a framed meter whose fill follows a 0..1 fraction.
//!
//! Everything is drawn at the framebuffer's pixel size and scaled up by the integer [`HudScale`] (2 on a phone, 3 from about
//! 600 layout px of the short side), so one HUD pixel is `k` layout pixels like the menus. All art is procedural (no ROM data).
use super::fb::{clean, mix, rgb, Fb, IRect, Rgba};
use super::skin::{upload, GOLD, PALE, WHITE};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::ui::widget::NodeImageMode;
use std::collections::HashMap;

pub const INK: Rgba = rgb(0x040720);
pub const NAVY: Rgba = rgb(0x1a2468);
pub const RED: Rgba = rgb(0xf0604c);
pub const GREEN: Rgba = rgb(0x5cdc6c);
pub const AQUA: Rgba = rgb(0x7ad8ff);

/// Layout px per HUD pixel.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct HudScale(pub u32);

impl Default for HudScale {
    fn default() -> Self {
        Self(2)
    }
}

/// HUD pixel size for a window of `w` x `h` layout px: 2 on a phone, 3 once the short side reaches 600.
pub fn scale_for(w: f32, h: f32) -> u32 {
    if w.min(h) >= 600.0 {
        3
    } else {
        2
    }
}

/// Frame styles of a plate (chip, panel or button face).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Plate {
    /// The dialog frame of the menus: blue gradient, light inner edge, dark outline.
    Navy,
    /// A darker, slightly see-through chip for small readouts over busy scenes.
    Dark,
    Red,
    Green,
    Blue,
    /// The chosen/pressed slab (gold).
    Gold,
    /// Disabled / cooling down.
    Grey,
}

/// (top, bottom, light edge, dark edge, outline, alpha)
fn plate_colors(p: Plate) -> [Rgba; 5] {
    match p {
        Plate::Navy => [rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), INK],
        Plate::Dark => [rgb(0x1a2260), rgb(0x0a1038), rgb(0x4a5cb8), rgb(0x050a22), INK],
        Plate::Red => [rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603)],
        Plate::Green => [rgb(0x4ad060), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a)],
        Plate::Blue => [rgb(0x4a7dff), rgb(0x1f48d0), rgb(0xb8ccff), rgb(0x0f2a8a), INK],
        Plate::Gold => [rgb(0xffd23a), rgb(0xe08a18), rgb(0xfff1a8), rgb(0x8a4a08), rgb(0x3a1a04)],
        Plate::Grey => [rgb(0x6a6a78), rgb(0x3a3a46), rgb(0x9a9aa8), rgb(0x20202a), rgb(0x0a0a10)],
    }
}

/// Side of a plate tile and its fixed border (HUD px).
const TILE: i32 = 12;
const EDGE: i32 = 3;

fn plate_tile(p: Plate) -> Fb {
    let [top, bot, light, dark, outline] = plate_colors(p);
    let mut f = Fb::new(TILE, TILE);
    f.slab(IRect::new(0, 0, TILE, TILE), top, bot, light, dark, outline);
    if p == Plate::Dark {
        for px in f.data.chunks_exact_mut(4) {
            px[3] = if px[3] == 255 && !(px[0] == INK[0] && px[1] == INK[1] && px[2] == INK[2]) { 226 } else { px[3] };
        }
    }
    // Round the four corners off by one pixel (the menus' slabs have notched corners).
    for (x, y) in [(0, 0), (TILE - 1, 0), (0, TILE - 1), (TILE - 1, TILE - 1)] {
        let i = ((y * TILE + x) * 4) as usize;
        f.data[i + 3] = 0;
    }
    f
}

/// Pixel icons of the HUD (original art, small enough to sit beside a number).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Heart,
    HeartHalf,
    HeartEmpty,
    Coin,
    Star,
    Life,
    Key,
    Rupee,
    Gem,
    Note,
    Comb,
    Jiggy,
    Egg,
    Feather,
    FeatherRed,
    FeatherGold,
    CombEmpty,
    Clock,
    Flag,
}

/// (rows, base colour, light, shade)
fn icon_art(i: Icon) -> (&'static [&'static str], Rgba, Rgba, Rgba) {
    match i {
        Icon::Heart => (&[".OOO.OOO.", "OLLRORRRO", "OLRRRRRDO", "ORRRRRRDO", ".ORRRRDO.", "..ORRDO..", "...ODO...", "....O...."], rgb(0xe8403a), rgb(0xffb0a0), rgb(0x9a1818)),
        Icon::HeartHalf => (&[".OOO.OOO.", "OLLRONNNO", "OLRRONNNO", "ORRRONNNO", ".ORRRNNO.", "..ORRNO..", "...ORO...", "....O...."], rgb(0xe8403a), rgb(0xffb0a0), rgb(0x9a1818)),
        Icon::HeartEmpty => (&[".OOO.OOO.", "ONNNONNNO", "ONNNNNNNO", "ONNNNNNNO", ".ONNNNNO.", "..ONNNO..", "...ONO...", "....O...."], rgb(0x1a2468), rgb(0x3a4eb0), rgb(0x0b1240)),
        Icon::Coin => (&["..OOOO..", ".OLYYYO.", "OLYYDYYO", "OYYYDYYO", "OYYYDYYO", "OYYYDYDO", ".OYYYDO.", "..OOOO.."], rgb(0xffcc2a), rgb(0xfff2a0), rgb(0xc07a08)),
        Icon::Star => (&["....O....", "...OYO...", "...OYO...", "OOOOLYOOO", "OYYYYYYYO", ".OYYYYYO.", "..OYYDO..", "..OYODO..", ".OOO.OOO."], rgb(0xffd23a), rgb(0xfff6b0), rgb(0xd08a10)),
        Icon::Life => (&["..OOOO...", ".ORRRRO..", "ORRRLRRO.", "ORRRRRRO.", "OOOOOOOOO", "OSSSSSSSO", "OSSESESSO", ".OSSSSSO.", "..OOOOO.."], rgb(0xe8403a), rgb(0xffb0a0), rgb(0x9a1818)),
        Icon::Key => (&[".OOO.......", "OYLYO......", "OY.YOOOOOOO", "OYYYYYYYYYO", ".OOO.OYOYO.", ".....OOOOO."], rgb(0xffd23a), rgb(0xfff6b0), rgb(0xc07a08)),
        Icon::Rupee => (&["...O...", "..OLO..", ".OLGGO.", "OLGGGGO", "OGGLGDO", "OGGLGDO", "OGGGGDO", ".OGGDO.", "..OGO..", "...O..."], rgb(0x3adc6a), rgb(0xc8ffd8), rgb(0x0a8a30)),
        Icon::Gem => (&["..OOOO..", ".OLBBBO.", "OLBBBBBO", "OBBBBBDO", ".OBBBDO.", "..OBDO..", "...OO..."], rgb(0x4aa8ff), rgb(0xcce8ff), rgb(0x1a58c8)),
        Icon::Note => (&["...OOOO", "...OWWO", "...OWOO", "...OWO.", "...OWO.", ".OOOWO.", "OWWOWO.", "OWWWO..", ".OOO..."], rgb(0xfff0c8), rgb(0xffffff), rgb(0xc8a860)),
        Icon::Comb => (&["..OOOOO..", ".OLYYYYO.", "OLYYYYYYO", "OYYYYYYYO", "OYYYYYYDO", "OYYYYYYDO", ".OYYYYDO.", "..OOOOO.."], rgb(0xffb81c), rgb(0xffe890), rgb(0xc07008)),
        Icon::Jiggy => (&["..OOO....", ".OYLYO...", ".OYYYO...", "OOYYYOOO.", "OYYYYYYYO", "OYYYYYYDO", "OOYYYYYOO", ".OYYYYDO.", "..OOOOO.."], rgb(0xf8c828), rgb(0xfff0a0), rgb(0xd07810)),
        Icon::Egg => (&["..OOO..", ".OWWWO.", "OWWLWWO", "OWWWWDO", "OWWWWDO", "OWWWDDO", ".OWDDO.", "..OOO.."], rgb(0xf4ecd8), rgb(0xffffff), rgb(0xc0b090)),
        Icon::Feather => (&[".....OOO", "...OOBBO", "..OBBLBO", ".OBBBBO.", "OBBBBBO.", "OBBBBO..", ".OBBO...", ".OOO...."], rgb(0x4ab8ff), rgb(0xc8ecff), rgb(0x1a68c8)),
        Icon::FeatherRed => (&[".....OOO", "...OOBBO", "..OBBLBO", ".OBBBBO.", "OBBBBBO.", "OBBBBO..", ".OBBO...", ".OOO...."], rgb(0xe8403a), rgb(0xffb0a0), rgb(0x9a1818)),
        Icon::FeatherGold => (&[".....OOO", "...OOBBO", "..OBBLBO", ".OBBBBO.", "OBBBBBO.", "OBBBBO..", ".OBBO...", ".OOO...."], rgb(0xffcc2a), rgb(0xfff2a0), rgb(0xc07a08)),
        Icon::CombEmpty => (&["..OOOOO..", ".ONNNNNO.", "ONNNNNNNO", "ONNNNNNNO", "ONNNNNNNO", "ONNNNNNNO", ".ONNNNNO.", "..OOOOO.."], rgb(0x1a2468), rgb(0x3a4eb0), rgb(0x0b1240)),
        Icon::Clock => (&["..OOOOO..", ".OWWWWWO.", "OWWWDWWWO", "OWWWDWWWO", "OWWWDDDWO", "OWWWWWWWO", ".OWWWWWO.", "..OOOOO.."], rgb(0xf6f6ee), rgb(0xffffff), rgb(0x8a8aa0)),
        Icon::Flag => (&["OO.......", "OROOO....", "ORRRROO..", "ORRRRRRO.", "ORRRROO..", "OROOO....", "OO.......", "O........", "O........"], rgb(0xe8403a), rgb(0xffb0a0), rgb(0x9a1818)),
    }
}

/// Draw an icon into its own buffer with the dark outline already part of the art.
pub fn icon_fb(i: Icon) -> Fb {
    let (rows, base, light, shade) = icon_art(i);
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(1) as i32;
    let mut f = Fb::new(w, rows.len() as i32);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let c = match ch {
                'O' => INK,
                'L' => light,
                'D' => shade,
                'N' => rgb(0x1a2468),
                'E' => rgb(0x2a1608),
                'S' => rgb(0xf2c498),
                'W' | 'R' | 'Y' | 'G' | 'B' => base,
                _ => continue,
            };
            f.px(x as i32, y as i32, c);
        }
    }
    f
}

/// Word-wrap `s` (newlines kept) to lines of at most `max` characters; `max == 0` only splits at newlines.
pub fn wrap_lines(s: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in s.split('\n') {
        if max == 0 {
            out.push(para.to_string());
            continue;
        }
        let mut line = String::new();
        for word in para.split(' ') {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

/// Render text as a self-contained image: the title's font with a dark outline and drop shadow, `scale` HUD px per glyph px.
/// Several lines (`\n`) stack with a one-pixel gap.
pub fn text_fb(s: &str, scale: i32, color: Rgba) -> Fb {
    text_fb_lines(&s.split('\n').map(clean).collect::<Vec<_>>(), scale, color)
}

fn text_fb_lines(lines: &[String], scale: i32, color: Rgba) -> Fb {
    let tw = lines.iter().map(|l| Fb::text_width(l, scale)).max().unwrap_or(0);
    let n = lines.len().max(1) as i32;
    let mut f = Fb::new((tw + 3 * scale).max(1), (9 * (n - 1) + 10) * scale);
    let shade = mix(color, rgb(0x101840), 0.28);
    // The outline ring plus a two-pixel shadow towards the bottom right.
    let ring = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1), (2, 0), (0, 2), (2, 1), (1, 2), (2, 2)];
    for (i, line) in lines.iter().enumerate() {
        let y = scale + i as i32 * 9 * scale;
        for (dx, dy) in ring {
            f.glyphs(scale + dx * scale, y + dy * scale, line, scale, &|_| INK);
        }
        f.glyphs(scale, y, line, scale, &|row| mix(color, shade, row as f32 / 6.0));
    }
    f
}

/// A string drawn in the HUD font. Spawn it with [`HudKit::text`]; change `text` / `color` and the node follows.
#[derive(Component, Clone, PartialEq, Debug)]
pub struct PixelText {
    pub text: String,
    pub color: Rgba,
    /// HUD px per glyph px (1 = 5x7 glyphs, 2 = big banner lettering).
    pub scale: i32,
    /// Wrap to this many characters per line (0: only at `\n`).
    pub wrap: usize,
    /// Shrink the drawn size (never enlarge) so the node is at most this many layout px wide (0: no limit). Small buttons
    /// use it to keep a label like SHOCK inside; the glyphs end up a little under 2 layout px per font pixel.
    pub max_w: f32,
}

impl PixelText {
    pub fn new(text: impl Into<String>, color: Rgba) -> Self {
        Self { text: text.into(), color, scale: 1, wrap: 0, max_w: 0.0 }
    }

    pub fn big(mut self, scale: i32) -> Self {
        self.scale = scale;
        self
    }

    pub fn wrapped(mut self, chars: usize) -> Self {
        self.wrap = chars;
        self
    }

    pub fn fit_width(mut self, px: f32) -> Self {
        self.max_w = px;
        self
    }

    pub fn lines(&self) -> Vec<String> {
        wrap_lines(&self.text, self.wrap).iter().map(|l| clean(l)).collect()
    }

    /// Assign a new string only when it differs (keeps change detection quiet).
    pub fn set(this: &mut Mut<PixelText>, text: impl Into<String>) {
        let text = text.into();
        if this.text != text {
            this.text = text;
        }
    }

    pub fn set_color(this: &mut Mut<PixelText>, color: Rgba) {
        if this.color != color {
            this.color = color;
        }
    }

    /// Layout size of the rendered node.
    pub fn size_px(&self, k: u32) -> Vec2 {
        let (w, h) = self.fb_size();
        let full = Vec2::new((w as u32 * k) as f32, (h as u32 * k) as f32);
        if self.max_w > 0.0 && full.x > self.max_w {
            (full * (self.max_w / full.x)).round()
        } else {
            full
        }
    }

    fn fb_size(&self) -> (i32, i32) {
        let lines = self.lines();
        let tw = lines.iter().map(|l| Fb::text_width(l, self.scale)).max().unwrap_or(0);
        ((tw + 3 * self.scale).max(1), (9 * (lines.len().max(1) as i32 - 1) + 10) * self.scale)
    }
}

/// The convenience colour for the common cases.
pub fn text_color(c: Color) -> Rgba {
    let s = c.to_srgba();
    [(s.red * 255.0) as u8, (s.green * 255.0) as u8, (s.blue * 255.0) as u8, 255]
}

/// Cached image handles of the kit, per integer scale.
#[derive(Resource, Default)]
pub struct HudArt {
    plates: HashMap<(Plate, u32), Handle<Image>>,
    icons: HashMap<(Icon, u32), Handle<Image>>,
}

#[derive(SystemParam)]
pub struct HudKit<'w> {
    pub art: ResMut<'w, HudArt>,
    pub images: ResMut<'w, Assets<Image>>,
    pub scale: Res<'w, HudScale>,
}

impl HudKit<'_> {
    pub fn k(&self) -> u32 {
        self.scale.0
    }

    /// Layout px of `n` HUD px.
    pub fn px(&self, n: i32) -> Val {
        Val::Px((n * self.scale.0 as i32) as f32)
    }

    /// The nine-sliced frame image of a plate. Put it on a node whose padding keeps the content off the 3 px edge.
    pub fn plate(&mut self, p: Plate) -> ImageNode {
        let k = self.scale.0;
        let h = match self.art.plates.get(&(p, k)) {
            Some(h) => h.clone(),
            None => {
                let h = upload(&mut self.images, &plate_tile(p), k);
                self.art.plates.insert((p, k), h.clone());
                h
            }
        };
        let b = (EDGE as u32 * k) as f32;
        ImageNode {
            image: h,
            image_mode: NodeImageMode::Sliced(TextureSlicer { border: BorderRect::all(b), center_scale_mode: SliceScaleMode::Stretch, sides_scale_mode: SliceScaleMode::Stretch, max_corner_scale: 1.0 }),
            ..default()
        }
    }

    /// A row-flowing chip: plate frame plus the padding and gap the HUD uses. (An `ImageNode` is drawn inside its padding, so the
    /// frame is a separate child that [`apply_frames`] adds behind the content.)
    pub fn chip(&mut self, p: Plate) -> (Node, Framed) {
        let node = Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: self.px(3), padding: UiRect::axes(self.px(5), self.px(4)), flex_shrink: 0.0, ..default() };
        (node, Framed(p))
    }

    pub fn icon(&mut self, i: Icon) -> (ImageNode, Node) {
        let k = self.scale.0;
        let fb = icon_fb(i);
        let h = match self.art.icons.get(&(i, k)) {
            Some(h) => h.clone(),
            None => {
                let h = upload(&mut self.images, &fb, k);
                self.art.icons.insert((i, k), h.clone());
                h
            }
        };
        (ImageNode::new(h), Node { width: self.px(fb.w), height: self.px(fb.h), flex_shrink: 0.0, ..default() })
    }

    /// A self-rendering HUD string.
    pub fn text(&self, s: impl Into<String>, color: Rgba) -> (PixelText, ImageNode, Node) {
        self.text_big(s, color, 1)
    }

    pub fn text_big(&self, s: impl Into<String>, color: Rgba, scale: i32) -> (PixelText, ImageNode, Node) {
        let t = PixelText::new(s, color).big(scale);
        let size = t.size_px(self.scale.0);
        (t, ImageNode::default(), Node { width: Val::Px(size.x), height: Val::Px(size.y), flex_shrink: 0.0, ..default() })
    }
}

/// Draws a plate behind the node's content (see [`apply_frames`]); change the value to restyle it.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Framed(pub Plate);

#[derive(Component)]
struct FrameImage;

fn apply_frames(mut commands: Commands, mut kit: HudKit, framed: Query<(Entity, Ref<Framed>, Option<&Children>)>, mut images: Query<&mut ImageNode, With<FrameImage>>, marks: Query<(), With<FrameImage>>) {
    for (e, f, children) in &framed {
        if !f.is_changed() {
            continue;
        }
        let existing = children.and_then(|c| c.iter().find(|ch| marks.contains(*ch)));
        let want = kit.plate(f.0);
        match existing {
            Some(ch) => {
                if let Ok(mut img) = images.get_mut(ch) {
                    if img.image != want.image {
                        img.image = want.image;
                    }
                }
            }
            None => {
                let frame = commands.spawn((FrameImage, want, Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), ..default() }, bevy::ui::FocusPolicy::Pass)).id();
                commands.entity(e).insert_children(0, &[frame]);
            }
        }
    }
}

/// A 1x1 transparent fallback is never needed: the render system fills the image before the first frame is drawn.
fn render_texts(scale: Res<HudScale>, mut images: ResMut<Assets<Image>>, mut q: Query<(Ref<PixelText>, &mut ImageNode, &mut Node)>) {
    let k = scale.0;
    for (t, mut img, mut node) in &mut q {
        if !(t.is_changed() || scale.is_changed()) {
            continue;
        }
        let fb = text_fb_lines(&t.lines(), t.scale, t.color);
        let mut data = vec![0u8; (fb.w as u32 * k * fb.h as u32 * k * 4) as usize];
        let (w, h) = (fb.w as u32 * k, fb.h as u32 * k);
        for y in 0..h {
            for x in 0..w {
                let s = (((y / k) as i32 * fb.w + (x / k) as i32) * 4) as usize;
                let d = ((y * w + x) * 4) as usize;
                data[d..d + 4].copy_from_slice(&fb.data[s..s + 4]);
            }
        }
        let mut image = Image::new(Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
        image.sampler = ImageSampler::linear();
        img.image = images.add(image);
        let size = t.size_px(k);
        node.width = Val::Px(size.x);
        node.height = Val::Px(size.y);
    }
}

/// Runs once before the HUDs are spawned: their fixed-size parts are built at this scale (a window resize across the 600 px
/// boundary keeps the first choice).
fn track_window(mut scale: ResMut<HudScale>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>) {
    if let Ok(w) = windows.single() {
        let k = scale_for(w.width(), w.height());
        if scale.0 != k {
            scale.0 = k;
        }
    }
}

/// Wrap a [`PixelText`] to this fraction of the window width (portrait phones, long hint lines).
#[derive(Component, Clone, Copy, Debug)]
pub struct WrapFrac(pub f32);

/// Characters per line that fit `px` layout px for text at `scale` HUD px per glyph px and `k` layout px per HUD px.
pub fn chars_for(px: f32, scale: i32, k: u32) -> usize {
    let per = (6 * scale as u32 * k) as f32;
    (((px - 3.0 * (scale as u32 * k) as f32) / per).floor() as i64).clamp(6, 200) as usize
}

fn wrap_to_window(scale: Res<HudScale>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut q: Query<(&WrapFrac, &mut PixelText)>) {
    let Ok(w) = windows.single() else { return };
    for (f, mut t) in &mut q {
        let chars = chars_for(w.width() * f.0, t.scale, scale.0);
        if t.wrap != chars {
            t.wrap = chars;
        }
    }
}

/// A framed meter: the fill's width follows `frac` (0..1).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct BarFill {
    pub frac: f32,
}

/// Inner fill width in layout px for a bar `inner_w` wide: whole HUD pixels only, so the edge stays crisp.
pub fn fill_px(frac: f32, inner_w: f32, k: f32) -> f32 {
    let f = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
    ((f * inner_w / k).round() * k).min(inner_w)
}

fn sync_bars(scale: Res<HudScale>, mut fills: Query<(Ref<BarFill>, &mut Node, &ChildOf)>, parents: Query<&ComputedNode>) {
    for (f, mut node, parent) in &mut fills {
        let inner = parents.get(parent.parent()).map(|c| c.size().x * c.inverse_scale_factor()).unwrap_or(0.0);
        let w = if inner > 0.0 { Val::Px(fill_px(f.frac, inner, scale.0 as f32)) } else { Val::Percent(f.frac.clamp(0.0, 1.0) * 100.0) };
        if node.width != w {
            node.width = w;
        }
    }
}

/// Spawn a framed bar `w` x `h` HUD px with a coloured fill (returns the fill entity so callers can tag it with their marker).
pub fn bar(p: &mut ChildSpawnerCommands, kit: &HudKit, w: i32, h: i32, color: Rgba, frac: f32) -> Entity {
    let c = |c: Rgba| Color::srgb_u8(c[0], c[1], c[2]);
    let mut fill = Entity::PLACEHOLDER;
    p.spawn((
        Node { width: kit.px(w), height: kit.px(h), border: UiRect::all(kit.px(1)), flex_shrink: 0.0, overflow: Overflow::clip(), ..default() },
        BorderColor::all(c(INK)),
        BackgroundColor(c(rgb(0x0a1038))),
    ))
    .with_children(|b| {
        fill = b
            .spawn((Node { height: Val::Percent(100.0), width: Val::Percent(frac * 100.0), ..default() }, BackgroundColor(c(color)), BarFill { frac }))
            .with_children(|f| {
                f.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), height: Val::Percent(38.0), ..default() }, BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.32))));
            })
            .id();
    });
    fill
}

/// A counter: icon, "x" and the number, in one chip-less row (the caller supplies the frame). Returns nothing; the text entity
/// carries `marker`.
pub fn counter(p: &mut ChildSpawnerCommands, kit: &mut HudKit, icon: Icon, text: &str, color: Rgba, marker: impl Bundle) -> Entity {
    let mut id = Entity::PLACEHOLDER;
    let (img, node) = kit.icon(icon);
    let t = kit.text(text, color);
    p.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: kit.px(3), ..default() }).with_children(|r| {
        r.spawn((img, node));
        id = r.spawn((t, marker)).id();
    });
    id
}

/// Add the kit's plugin once, whichever HUD plugin builds first.
pub fn ensure(app: &mut App) {
    if !app.is_plugin_added::<HudKitPlugin>() {
        app.add_plugins(HudKitPlugin);
    }
}

/// Just the resources the kit's parameters need, for tests that build a minimal app.
#[cfg(test)]
pub fn test_resources(app: &mut App) {
    if !app.world().contains_resource::<Assets<Image>>() {
        app.init_resource::<Assets<Image>>();
    }
    app.init_resource::<HudScale>().init_resource::<HudArt>();
}

pub struct HudKitPlugin;

impl Plugin for HudKitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudScale>().init_resource::<HudArt>().add_systems(PreStartup, track_window).add_systems(PostUpdate, (apply_frames, wrap_to_window, render_texts, sync_bars).chain().before(bevy::ui::UiSystems::Prepare));
    }
}

/// Colours shared by the HUDs.
pub const COL_GOLD: Rgba = GOLD;
pub const COL_WHITE: Rgba = WHITE;
pub const COL_PALE: Rgba = PALE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_follows_the_short_side() {
        assert_eq!(scale_for(874.0, 402.0), 2);
        assert_eq!(scale_for(402.0, 874.0), 2);
        assert_eq!(scale_for(1280.0, 720.0), 3);
    }

    #[test]
    fn bar_fill_is_whole_hud_pixels_and_clamped() {
        assert_eq!(fill_px(0.0, 60.0, 2.0), 0.0);
        assert_eq!(fill_px(1.0, 60.0, 2.0), 60.0);
        assert_eq!(fill_px(0.5, 60.0, 2.0), 30.0);
        assert_eq!(fill_px(0.51, 60.0, 3.0) % 3.0, 0.0);
        assert_eq!(fill_px(2.0, 60.0, 2.0), 60.0);
        assert_eq!(fill_px(-1.0, 60.0, 2.0), 0.0);
        assert_eq!(fill_px(f32::NAN, 60.0, 2.0), 0.0);
        let mut last = 0.0;
        for i in 0..=100 {
            let w = fill_px(i as f32 / 100.0, 61.0, 2.0);
            assert!(w >= last && w <= 61.0, "monotonic and inside the frame");
            last = w;
        }
    }

    #[test]
    fn every_icon_is_a_rectangle_of_drawn_pixels_inside_an_ink_outline() {
        for i in [Icon::Heart, Icon::HeartHalf, Icon::HeartEmpty, Icon::Coin, Icon::Star, Icon::Life, Icon::Key, Icon::Rupee, Icon::Gem, Icon::Note, Icon::Comb, Icon::Jiggy, Icon::Egg, Icon::Feather, Icon::FeatherRed, Icon::FeatherGold, Icon::CombEmpty, Icon::Clock, Icon::Flag] {
            let (rows, ..) = icon_art(i);
            let f = icon_fb(i);
            assert!(f.w >= 7 && f.w <= 12 && f.h >= 6 && f.h <= 12, "{i:?} {}x{}", f.w, f.h);
            assert!(f.data.chunks(4).filter(|p| p[3] > 0).count() > (f.w * f.h / 3) as usize, "{i:?} is mostly filled");
            assert!(f.data.chunks(4).any(|p| p[..3] == INK[..3] && p[3] == 255), "{i:?} has an outline");
        }
    }

    #[test]
    fn text_is_outlined_inside_its_buffer_and_sizes_match() {
        let t = PixelText::new("x 154", GOLD);
        for k in [2, 3] {
            let size = t.size_px(k);
            let fb = text_fb("x 154", 1, GOLD);
            assert_eq!(size, Vec2::new((fb.w as u32 * k) as f32, (fb.h as u32 * k) as f32));
        }
        let fb = text_fb("HI", 1, GOLD);
        assert!(fb.data.chunks(4).any(|p| p[..3] == GOLD[..3] || p[3] > 0));
        let big = text_fb("HI", 2, GOLD);
        assert_eq!((big.w, big.h), (Fb::text_width("HI", 2) + 6, 20));
    }

    #[test]
    fn wrapping_to_the_window_keeps_lines_inside_it() {
        for (w, k, scale) in [(402.0f32, 2u32, 1i32), (874.0, 2, 1), (1280.0, 3, 1), (402.0, 2, 2)] {
            let chars = chars_for(w * 0.9, scale, k);
            let text = "word ".repeat(60);
            let t = PixelText::new(text, GOLD).big(scale).wrapped(chars);
            assert!(t.size_px(k).x <= w * 0.9 + 0.5, "{w} {k} {scale}: {} > {}", t.size_px(k).x, w * 0.9);
            assert!(t.lines().len() > 1);
        }
    }

    #[test]
    fn labels_shrink_to_fit_but_never_grow() {
        let long = PixelText::new("SHOCK", GOLD).fit_width(44.0);
        assert!(long.size_px(2).x <= 44.0 && long.size_px(2).x > 38.0);
        let short = PixelText::new("A", GOLD).fit_width(44.0);
        assert_eq!(short.size_px(2), PixelText::new("A", GOLD).size_px(2));
        let two = PixelText::new("SHOCK WAVE", GOLD).wrapped(5);
        assert_eq!(two.lines(), vec!["SHOCK".to_string(), "WAVE".to_string()]);
        assert_eq!(two.size_px(2).y, 19.0 * 2.0);
        assert_eq!(wrap_lines("a\nb c", 0), vec!["a".to_string(), "b c".to_string()]);
    }

    #[test]
    fn plate_tiles_have_notched_corners_and_an_opaque_core() {
        for p in [Plate::Navy, Plate::Dark, Plate::Red, Plate::Green, Plate::Blue, Plate::Gold, Plate::Grey] {
            let f = plate_tile(p);
            assert_eq!(f.get(0, 0)[3], 0, "{p:?} corner");
            assert!(f.get(6, 6)[3] > 200, "{p:?} core");
            assert!(f.get(5, 0)[3] > 200, "{p:?} edge");
        }
    }

    #[test]
    fn the_plugin_renders_text_into_sized_nodes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Assets<Image>>().init_resource::<HudScale>().add_systems(Update, render_texts);
        let t = PixelText::new("HELLO", GOLD);
        let e = app.world_mut().spawn((t.clone(), ImageNode::default(), Node::default())).id();
        app.update();
        let n = app.world().get::<Node>(e).unwrap();
        assert_eq!((n.width, n.height), (Val::Px(t.size_px(2).x), Val::Px(t.size_px(2).y)));
        assert_ne!(app.world().get::<ImageNode>(e).unwrap().image, Handle::default());
    }
}
