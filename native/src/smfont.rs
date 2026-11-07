//! The SM64 bitmap fonts as `bevy_ui` text (`SmText`).
//!
//! Two fonts live in segment 2 of the US ROM (`reference/sm64/bin/segment2.c`):
//! * **menu font** (`main_font_lut`, `render_generic_char`): IA4 16x8 textures
//!   (`font_graphics.<offset>.ia4.png`) that are really 8x16 glyphs stored transposed (drawn with
//!   `gSPTextureRectangleFlip`), see `decode`. The LUT is indexed by the
//!   SM64 dialog code (`0-9`, `A-Z`, `a-z`, `'`, `.` then sparse specials such as `,`=0x6F,
//!   `-`=0x9F, `!`=0xF2, `?`=0xF4, `$`(coin)=0xF9, star=0xFA, multiply=0xFB). Per-glyph pixel
//!   advances are `gDialogCharWidths` (`ingame_menu.c`).
//! * **HUD font** (`main_hud_lut`, `print.c`): 16x16 RGBA16 glyphs (`segment2.<idx*0x200>.rgba16.png`)
//!   with `*` = multiply, `+` = coin, `,` = Mario head, `-` = star; 12 px advance.
//!
//! `SmText` rasterises the string into one small RGBA image (nearest sampled, integer scaled).
use crate::render::AssetRoot;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::HashMap;

pub struct SmFontPlugin;

impl Plugin for SmFontPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GlyphCache>().add_systems(Update, update_sm_text);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SmFont {
    Menu,
    Hud,
}

/// Text drawn with an SM64 bitmap font. Spawn with [`sm_text`].
#[derive(Component, Clone, PartialEq, Debug)]
pub struct SmText {
    pub text: String,
    pub font: SmFont,
    /// Menu font tint (the HUD font keeps its own colours).
    pub color: Color,
    /// Integer pixel scale.
    pub scale: f32,
    pub shadow: bool,
}

/// A menu-font label.
pub fn sm_text(text: impl Into<String>, color: Color, scale: f32) -> impl Bundle {
    (SmText { text: text.into(), font: SmFont::Menu, color, scale, shadow: false }, ImageNode::default(), Node::default())
}

/// A HUD-font label (`print_text`).
pub fn hud_text(text: impl Into<String>, scale: f32) -> impl Bundle {
    (SmText { text: text.into(), font: SmFont::Hud, color: Color::WHITE, scale, shadow: false }, ImageNode::default(), Node::default())
}

impl SmText {
    /// Set the text, touching change detection only when it differs.
    pub fn set(this: &mut Mut<SmText>, text: impl Into<String>) {
        let text = text.into();
        if this.text != text {
            this.text = text;
        }
    }
}

/// SM64 dialog code of a character in `main_font_lut` (US).
fn menu_code(c: char) -> Option<u32> {
    Some(match c {
        '0'..='9' => c as u32 - '0' as u32,
        'A'..='Z' => 10 + c as u32 - 'A' as u32,
        'a'..='z' => 36 + c as u32 - 'a' as u32,
        '\'' | '\u{2019}' | '\u{2018}' => 0x3e,
        '.' => 0x3f,
        ',' => 0x6f,
        '-' | '/' | '\u{2013}' | '\u{2014}' => 0x9f,
        '(' => 0xe1,
        ')' => 0xe3,
        '\u{2194}' => 0xe4,
        '&' => 0xe5,
        '\u{2026}' => 0xe6,
        '!' => 0xf2,
        '%' => 0xf3,
        '?' => 0xf4,
        '"' | '\u{201c}' => 0xf5,
        '\u{201d}' => 0xf6,
        '~' => 0xf7,
        '$' => 0xf9,
        '\u{2605}' => 0xfa,
        '\u{d7}' => 0xfb,
        '\u{b7}' | '\u{2022}' => 0xfc,
        '\u{2606}' => 0xfd,
        _ => return None,
    })
}

/// `font_graphics` file of a menu code (the first 62 are laid out sequentially; the rest are from the LUT).
fn menu_file(code: u32) -> Option<String> {
    let off = match code {
        0..=61 => 0x5900 + code * 0x40,
        0x3e => 0x6b80,
        0x3f => 0x6a80,
        0x6f => 0x6b40,
        0x9f => 0x6d40,
        0xe1 => 0x6980,
        0xe2 => 0x69c0,
        0xe3 => 0x6a00,
        0xe4 => 0x6880,
        0xe5 => 0x6d80,
        0xe6 => 0x6d00,
        0xf2 => 0x68c0,
        0xf3 => 0x6ac0,
        0xf4 => 0x6bc0,
        0xf5 => 0x6c80,
        0xf6 => 0x6cc0,
        0xf7 => 0x6a40,
        0xf9 => 0x6900,
        0xfa => 0x6c00,
        0xfb => 0x6940,
        0xfc => 0x6b00,
        0xfd => 0x6c40,
        _ => return None,
    };
    Some(format!("font_graphics.{off:05X}.ia4.png"))
}

/// `gDialogCharWidths` (US) for the codes we use.
fn menu_width(code: u32) -> i32 {
    const LOW: [u8; 64] = [
        7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 6, 6, 6, 6, 6, 6, 6, 6, 5, 6, 6, 5, 8, 8, 6, 6, 6, 6, 6, 5, 6, 6, 8, 7, 6, 6, 6, 5, 5, 6, 5, 5, 6, 5, 4, 5, 5, 3, 7, 5, 5, 5, 6, 5, 5, 5, 5, 5, 7, 7, 5, 5, 4, 4,
    ];
    match code {
        0..=63 => LOW[code as usize] as i32,
        0x6f => 4,
        0x9f => 6,
        0xe1 | 0xe3 => 5,
        0xe2 => 10,
        0xe4 => 9,
        0xe5 => 8,
        0xe6 => 4,
        0xf2 => 5,
        0xf3 | 0xf4 => 7,
        0xf5 | 0xf6 => 6,
        0xf7 | 0xf9 => 8,
        0xfa | 0xfd => 10,
        0xfb => 6,
        0xfc => 4,
        _ => 6,
    }
}

const MENU_SPACE: i32 = 5;

/// `(file, advance)` of a HUD-font character (`char_to_glyph_index` in `print.c`, US glyph set).
fn hud_glyph(c: char) -> Option<(String, i32)> {
    let (idx, adv) = match c {
        '0'..='9' => (c as u32 - '0' as u32, 12),
        'a'..='z' | 'A'..='Z' => {
            let u = c.to_ascii_uppercase();
            if "JQVXZ".contains(u) {
                return None;
            }
            (10 + u as u32 - 'A' as u32, 12)
        }
        '\'' | '\u{2019}' => (36, 12),
        '"' => (37, 12),
        '*' | '\u{d7}' => (43, 14),
        '+' => (44, 16),
        ',' => (45, 16),
        '-' => (46, 16),
        _ => return None,
    };
    Some((format!("segment2.{:05X}.rgba16.png", idx * 0x200), adv))
}

/// A decoded glyph: menu glyphs are squeezed to 8x8, HUD glyphs are 16x16.
struct Glyph {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

#[derive(Resource, Default)]
struct GlyphCache(HashMap<String, Option<Glyph>>);

fn decode(root: &AssetRoot, file: &str, menu: bool) -> Option<Glyph> {
    let bytes = std::fs::read(root.0.join("extracted/textures/segment2").join(file)).ok()?;
    let img = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, false, ImageSampler::nearest(), RenderAssetUsages::default()).ok()?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    let data = img.data?;
    if data.len() != w * h * 4 {
        return None;
    }
    if !menu {
        return Some(Glyph { w, h, rgba: data });
    }
    // The menu glyph is stored transposed (`render_generic_char` draws it with
    // `gSPTextureRectangleFlip`): image (x, y) is glyph column 7 - y, row 15 - x. The glyph is 8 wide and
    // 16 tall with the letters in rows 2..14; keep those.
    const TOP: usize = 2;
    const ROWS: usize = 12;
    let mut rgba = vec![0u8; 8 * ROWS * 4];
    for row in 0..ROWS {
        for col in 0..8 {
            let (sx, sy) = (15 - (row + TOP), 7 - col);
            let s = (sy * w + sx) * 4;
            rgba[(row * 8 + col) * 4..(row * 8 + col) * 4 + 4].copy_from_slice(&data[s..s + 4]);
        }
    }
    Some(Glyph { w: 8, h: ROWS, rgba })
}

fn glyph<'a>(cache: &'a mut GlyphCache, root: &AssetRoot, file: &str, menu: bool) -> Option<&'a Glyph> {
    cache.0.entry(file.to_string()).or_insert_with(|| decode(root, file, menu)).as_ref()
}

/// Rasterise `t` into `(width, height, rgba)`.
fn rasterize(cache: &mut GlyphCache, root: &AssetRoot, t: &SmText) -> Option<(usize, usize, Vec<u8>)> {
    let menu = t.font == SmFont::Menu;
    let (gh, line_step) = if menu { (12usize, 14usize) } else { (16, 20) };
    let shadow = usize::from(t.shadow);
    // Layout pass: (x, y, glyph file) per drawn glyph.
    let mut placed: Vec<(i32, usize, String)> = Vec::new();
    let (mut x, mut line, mut width) = (0i32, 0usize, 0i32);
    for c in t.text.chars() {
        if c == '\n' {
            width = width.max(x);
            x = 0;
            line += 1;
            continue;
        }
        if menu {
            match menu_code(c).and_then(|code| menu_file(code).map(|f| (f, menu_width(code)))) {
                Some((file, adv)) => {
                    placed.push((x, line, file));
                    x += adv;
                }
                None => x += MENU_SPACE,
            }
        } else {
            match hud_glyph(c) {
                Some((file, adv)) => {
                    placed.push((x, line, file));
                    x += adv;
                }
                None => x += 12,
            }
        }
    }
    width = width.max(x);
    if width <= 0 {
        return None;
    }
    let (w, h) = (width as usize + shadow, line * line_step + gh + shadow);
    let mut out = vec![0u8; w * h * 4];
    let tint = t.color.to_srgba();
    let tint = [(tint.red * 255.0) as u32, (tint.green * 255.0) as u32, (tint.blue * 255.0) as u32, (tint.alpha * 255.0) as u32];
    for pass in 0..=shadow {
        // Shadow pass first (offset by one pixel), then the glyphs on top.
        let off = if shadow == 1 && pass == 0 { 1 } else { 0 };
        let xoff = off;
        for (gx, gline, file) in &placed {
            let Some(g) = glyph(cache, root, file, menu) else { continue };
            for y in 0..g.h {
                for px in 0..g.w {
                    let s = (y * g.w + px) * 4;
                    let (dx, dy) = (*gx as usize + px + xoff, gline * line_step + y + off);
                    if dx >= w || dy >= h {
                        continue;
                    }
                    let d = (dy * w + dx) * 4;
                    let a = g.rgba[s + 3] as u32;
                    if a == 0 {
                        continue;
                    }
                    let (rgb, alpha) = if shadow == 1 && pass == 0 {
                        ([0, 0, 0], a * 160 / 255)
                    } else if menu {
                        let i = g.rgba[s] as u32;
                        ([tint[0] * i / 255, tint[1] * i / 255, tint[2] * i / 255], a * tint[3] / 255)
                    } else {
                        ([g.rgba[s] as u32, g.rgba[s + 1] as u32, g.rgba[s + 2] as u32], a)
                    };
                    // Source-over onto what's there (shadow below, glyph above).
                    let da = out[d + 3] as u32;
                    let oa = alpha + da * (255 - alpha) / 255;
                    if oa == 0 {
                        continue;
                    }
                    for k in 0..3 {
                        out[d + k] = ((rgb[k] * alpha + out[d + k] as u32 * da * (255 - alpha) / 255) / oa).min(255) as u8;
                    }
                    out[d + 3] = oa as u8;
                }
            }
        }
    }
    Some((w, h, out))
}

fn update_sm_text(
    root: Res<AssetRoot>,
    mut cache: ResMut<GlyphCache>,
    mut images: ResMut<Assets<Image>>,
    mut labels: Query<(&SmText, &mut ImageNode, &mut Node), Changed<SmText>>,
) {
    for (t, mut node, mut ui) in &mut labels {
        match rasterize(&mut cache, &root, t) {
            Some((w, h, data)) => {
                let mut img = Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
                img.sampler = ImageSampler::nearest();
                node.image = images.add(img);
                ui.width = Val::Px(w as f32 * t.scale);
                ui.height = Val::Px(h as f32 * t.scale);
            }
            None => {
                node.image = Handle::default();
                ui.width = Val::Px(0.0);
                ui.height = Val::Px(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> AssetRoot {
        AssetRoot(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets"))
    }

    #[test]
    fn menu_table_matches_the_lut() {
        // `main_font_lut`: A=10, a=36, apostrophe=0x3e, period=0x3f, comma=0x6f, hyphen=0x9f.
        assert_eq!(menu_file(menu_code('A').unwrap()).unwrap(), "font_graphics.05B80.ia4.png");
        assert_eq!(menu_file(menu_code('a').unwrap()).unwrap(), "font_graphics.06200.ia4.png");
        assert_eq!(menu_file(menu_code('z').unwrap()).unwrap(), "font_graphics.06840.ia4.png");
        assert_eq!(menu_file(menu_code('.').unwrap()).unwrap(), "font_graphics.06A80.ia4.png");
        assert_eq!(menu_width(menu_code('M').unwrap()), 8);
        assert_eq!(menu_width(menu_code('I').unwrap()), 5);
        assert_eq!(menu_width(0xfa), 10);
    }

    #[test]
    fn hud_table_matches_the_lut() {
        assert_eq!(hud_glyph('0').unwrap().0, "segment2.00000.rgba16.png");
        assert_eq!(hud_glyph('A').unwrap().0, "segment2.01400.rgba16.png");
        assert_eq!(hud_glyph('+').unwrap().0, "segment2.05800.rgba16.png");
        assert_eq!(hud_glyph(',').unwrap().0, "segment2.05A00.rgba16.png");
        assert_eq!(hud_glyph('-').unwrap().0, "segment2.05C00.rgba16.png");
        assert!(hud_glyph('J').is_none());
    }

    #[test]
    fn every_mapped_glyph_exists_and_renders() {
        let root = root();
        if !root.0.join("extracted/textures/segment2").is_dir() {
            return;
        }
        let mut cache = GlyphCache::default();
        let all = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz'.,-()&!%?\"~$\u{2605}\u{d7}\u{b7}\u{2606}";
        for c in all.chars() {
            let file = menu_file(menu_code(c).unwrap()).unwrap();
            assert!(glyph(&mut cache, &root, &file, true).is_some(), "menu glyph {c:?} ({file})");
        }
        for c in "0123456789ABCDEFGHILKMNOPRSTUWY'*+,-".chars() {
            let (file, _) = hud_glyph(c).unwrap();
            assert!(glyph(&mut cache, &root, &file, false).is_some(), "hud glyph {c:?} ({file})");
        }
        let t = SmText { text: "Bob-omb Battlefield".into(), font: SmFont::Menu, color: Color::WHITE, scale: 1.0, shadow: true };
        let (w, h, data) = rasterize(&mut cache, &root, &t).unwrap();
        assert_eq!(data.len(), w * h * 4);
        assert!(data.chunks_exact(4).any(|p| p[3] > 0));
    }
}
