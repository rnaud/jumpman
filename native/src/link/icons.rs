//! Item icons for Link's item screen, the C buttons and the "you got" card.
//!
//! When the OoT import produced the game's own 2D item textures (`assets/oot-icons/`, see `scripts/export-oot-icons.py` and
//! `byor-rs/src/oot/icons.rs`) [`IconCache`] shows those ([`RealIcons`], [`real_id`] picks the file per icon). Otherwise, and for
//! any single icon the import lacks, every icon is composed here from a handful of anti-aliased vector shapes (no ROM-derived art):
//! [`render`] rasterises an [`Icon`] into a 64x64 RGBA buffer and [`IconCache`] turns buffers into Bevy images on demand. Colours follow the game's items (green/blue/red/purple/gold rupees, red
//! hearts, gold keys...) so each reads at a glance even at 40 px.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use jumpman_sim::link::items::{Bottle, Boots, Item, Shield, Sword, Tunic};
use jumpman_sim::link::ocarina::Song;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SIZE: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum RupeeColor {
    Green,
    Blue,
    Red,
    Purple,
    Gold,
}

/// Everything that can be drawn as an item picture.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Icon {
    Item(Item),
    Bottle(Bottle),
    Sword(Sword),
    Shield(Shield),
    Tunic(Tunic),
    Boots(Boots),
    Song(Song),
    Rupee(RupeeColor),
    Heart,
    HeartPiece,
    SmallKey,
    BossKey,
    Compass,
    DungeonMap,
    Mushroom,
    Gauntlets,
    Arrows,
    BombBag,
    Magic,
    Questionmark,
}

/// The picture of a treasure chest reward (`treasure_reward` item ids of `zelda/save.rs`).
pub fn chest_icon(item: u32) -> Icon {
    match item {
        0x04 => Icon::Item(Item::Bow),
        0x05 => Icon::Item(Item::Slingshot),
        0x06 => Icon::Item(Item::Boomerang),
        0x1f => Icon::Mushroom,
        0x29 => Icon::Shield(Shield::Deku),
        0x2b => Icon::Shield(Shield::Mirror),
        0x32 => Icon::BombBag,
        0x35 => Icon::Gauntlets,
        0x3e => Icon::HeartPiece,
        0x3f => Icon::BossKey,
        0x40 => Icon::Compass,
        0x41 => Icon::DungeonMap,
        0x42 => Icon::SmallKey,
        0x48 => Icon::Heart,
        0x49..=0x4b => Icon::Arrows,
        0x4c => Icon::Rupee(RupeeColor::Green),
        0x4d => Icon::Rupee(RupeeColor::Blue),
        0x4e => Icon::Rupee(RupeeColor::Red),
        0x55 => Icon::Rupee(RupeeColor::Purple),
        0x56 => Icon::Rupee(RupeeColor::Gold),
        _ => Icon::Questionmark,
    }
}

/// The rupee colour for a wallet value (a rupee pickup of 1 / 5 / 20 / 50 / 200).
pub fn rupee_color(value: u32) -> RupeeColor {
    match value {
        0..=1 => RupeeColor::Green,
        2..=5 => RupeeColor::Blue,
        6..=20 => RupeeColor::Red,
        21..=50 => RupeeColor::Purple,
        _ => RupeeColor::Gold,
    }
}

// ---------------------------------------------------------------------------------------------
// The game's own icons (extracted at import time, loaded from the data directory)
// ---------------------------------------------------------------------------------------------
/// Which extracted icon (`assets/oot-icons/<id>.png`) shows `icon`, or `None` when only the procedural picture exists (songs, rupees
/// (the game draws them as 3D models) and the question mark).
pub fn real_id(icon: Icon) -> Option<&'static str> {
    Some(match icon {
        Icon::Item(i) => match i {
            Item::DekuStick => "deku_stick",
            Item::DekuNut => "deku_nut",
            Item::Bomb => "bomb",
            Item::Bow => "bow",
            Item::FireArrow => "arrow_fire",
            Item::DinsFire => "dins_fire",
            Item::Slingshot => "slingshot",
            Item::Ocarina => "ocarina_time",
            Item::Bombchu => "bombchu",
            Item::Hookshot => "hookshot",
            Item::IceArrow => "arrow_ice",
            Item::FaroresWind => "farores_wind",
            Item::Boomerang => "boomerang",
            Item::LensOfTruth => "lens_of_truth",
            Item::MagicBean => "magic_bean",
            Item::MegatonHammer => "hammer",
            Item::LightArrow => "arrow_light",
            Item::NayrusLove => "nayrus_love",
        },
        Icon::Bottle(b) => match b {
            Bottle::Empty => "bottle_empty",
            Bottle::RedPotion => "bottle_potion_red",
            Bottle::GreenPotion => "bottle_potion_green",
            Bottle::BluePotion => "bottle_potion_blue",
            Bottle::Fairy => "bottle_fairy",
            Bottle::Fish => "bottle_fish",
            Bottle::Milk => "bottle_milk_full",
            Bottle::HalfMilk => "bottle_milk_half",
            Bottle::RutoLetter => "bottle_rutos_letter",
            Bottle::BlueFire => "bottle_blue_fire",
            Bottle::Bug => "bottle_bug",
            Bottle::BigPoe => "bottle_big_poe",
            Bottle::Poe => "bottle_poe",
        },
        Icon::Sword(s) => match s {
            Sword::Kokiri => "sword_kokiri",
            Sword::Master => "sword_master",
            Sword::Biggoron => "sword_biggoron",
        },
        Icon::Shield(s) => match s {
            Shield::Deku => "shield_deku",
            Shield::Hylian => "shield_hylian",
            Shield::Mirror => "shield_mirror",
        },
        Icon::Tunic(t) => match t {
            Tunic::Kokiri => "tunic_kokiri",
            Tunic::Goron => "tunic_goron",
            Tunic::Zora => "tunic_zora",
        },
        Icon::Boots(b) => match b {
            Boots::Kokiri => "boots_kokiri",
            Boots::Iron => "boots_iron",
            Boots::Hover => "boots_hover",
        },
        // The recovery heart pickup is a 3D model in the game; the quest-status heart is the closest 2D picture.
        Icon::Heart => "heart_container",
        Icon::HeartPiece => "heart_piece",
        Icon::SmallKey => "small_key",
        Icon::BossKey => "boss_key",
        Icon::Compass => "compass",
        Icon::DungeonMap => "dungeon_map",
        Icon::Mushroom => "odd_mushroom",
        Icon::Gauntlets => "silver_gauntlets",
        // Arrow pickups: the quiver icon is a bundle of arrows.
        Icon::Arrows => "quiver_30",
        Icon::BombBag => "bomb_bag_20",
        Icon::Magic => "magic_jar_big",
        Icon::Song(_) | Icon::Rupee(_) | Icon::Questionmark => return None,
    })
}

/// One line of `assets/oot-icons/index.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealEntry {
    pub file: String,
    pub w: u32,
    pub h: u32,
}

/// Parses `index.json` (`{"version":1,"icons":{"bomb":{"file":"bomb.png","w":32,"h":32},...}}`). Entries that are malformed or whose
/// file name could leave the directory are skipped; a document that is not an index at all gives an empty map.
pub fn parse_index(text: &str) -> HashMap<String, RealEntry> {
    let mut out = HashMap::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return out };
    if v["version"].as_u64() != Some(1) {
        return out;
    }
    for (id, e) in v["icons"].as_object().into_iter().flatten() {
        let (Some(file), Some(w), Some(h)) = (e["file"].as_str(), e["w"].as_u64(), e["h"].as_u64()) else { continue };
        if file.is_empty() || file.contains(['/', '\\']) || file.starts_with('.') || w == 0 || h == 0 || w > 256 || h > 256 {
            continue;
        }
        out.insert(id.clone(), RealEntry { file: file.to_string(), w: w as u32, h: h as u32 });
    }
    out
}

/// Nearest-neighbour `k`x enlargement of an RGBA8 picture. The UI then samples it bilinearly, so the pixel art stays crisp at any
/// size the layout picks instead of shimmering at non-integer scales.
pub fn upscale_nearest(rgba: &[u8], w: usize, h: usize, k: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * k * k * 4];
    for y in 0..h * k {
        for x in 0..w * k {
            let (s, d) = (((y / k) * w + x / k) * 4, (y * w * k + x) * 4);
            out[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    out
}

/// The extracted icon set: the directory and its index. Pictures are only read when an icon is asked for.
#[derive(Clone, Debug, Default)]
pub struct RealIcons {
    dir: PathBuf,
    index: HashMap<String, RealEntry>,
}

impl RealIcons {
    /// `<assets>/oot-icons`; `None` when there is no readable `index.json` (a build without ROM data, or an import that predates icons).
    pub fn load(assets: &Path) -> Option<Self> {
        let dir = assets.join("oot-icons");
        let index = parse_index(&std::fs::read_to_string(dir.join("index.json")).ok()?);
        (!index.is_empty()).then_some(Self { dir, index })
    }

    pub fn has(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The decoded, enlarged RGBA8 picture of `id`; `None` when it is not in the index or its file does not decode as the indexed size.
    pub fn pixels(&self, id: &str) -> Option<(u32, u32, Vec<u8>)> {
        let e = self.index.get(id)?;
        let bytes = std::fs::read(self.dir.join(&e.file)).ok()?;
        let img = Image::from_buffer(&bytes, bevy::image::ImageType::Extension("png"), bevy::image::CompressedImageFormats::NONE, true, ImageSampler::nearest(), RenderAssetUsages::default()).ok()?;
        let (w, h) = (img.width(), img.height());
        let data = img.data?;
        if (w, h) != (e.w, e.h) || data.len() != (w * h * 4) as usize {
            return None;
        }
        let k = (128 / w.max(h)).max(1) as usize;
        Some((w * k as u32, h * k as u32, upscale_nearest(&data, w as usize, h as usize, k)))
    }
}

// ---------------------------------------------------------------------------------------------
// A tiny signed-distance painter
// ---------------------------------------------------------------------------------------------
type P = (f32, f32);
type Col = [f32; 4];

fn rgb(hex: u32) -> Col {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}

fn mix(a: Col, b: Col, t: f32) -> Col {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

fn lighten(c: Col, t: f32) -> Col {
    mix(c, [1.0, 1.0, 1.0, c[3]], t)
}

fn darken(c: Col, t: f32) -> Col {
    mix(c, [0.0, 0.0, 0.0, c[3]], t)
}

#[derive(Clone)]
enum Shape {
    Circle(P, f32),
    Ellipse(P, f32, f32),
    /// Centre, half extents, corner radius.
    Rect(P, f32, f32, f32),
    Capsule(P, P, f32),
    Poly(Vec<P>),
    Ring(P, f32, f32),
}

fn seg_dist(p: P, a: P, b: P) -> f32 {
    let (ab, ap) = ((b.0 - a.0, b.1 - a.1), (p.0 - a.0, p.1 - a.1));
    let len2 = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if len2 > 0.0 { ((ap.0 * ab.0 + ap.1 * ab.1) / len2).clamp(0.0, 1.0) } else { 0.0 };
    ((ap.0 - ab.0 * t).powi(2) + (ap.1 - ab.1 * t).powi(2)).sqrt()
}

impl Shape {
    /// Signed distance in pixels (negative inside).
    fn sdf(&self, p: P) -> f32 {
        match self {
            Shape::Circle(c, r) => ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r,
            Shape::Ellipse(c, rx, ry) => {
                let k = (((p.0 - c.0) / rx).powi(2) + ((p.1 - c.1) / ry).powi(2)).sqrt();
                (k - 1.0) * rx.min(*ry)
            }
            Shape::Rect(c, hw, hh, r) => {
                let (qx, qy) = ((p.0 - c.0).abs() - (hw - r), (p.1 - c.1).abs() - (hh - r));
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
            }
            Shape::Capsule(a, b, r) => seg_dist(p, *a, *b) - r,
            Shape::Ring(c, r, w) => (((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r).abs() - w * 0.5,
            Shape::Poly(v) => {
                let mut d = f32::MAX;
                let mut inside = false;
                let n = v.len();
                for i in 0..n {
                    let (a, b) = (v[i], v[(i + 1) % n]);
                    d = d.min(seg_dist(p, a, b));
                    if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
                        inside = !inside;
                    }
                }
                if inside { -d } else { d }
            }
        }
    }
}

struct Canvas {
    px: Vec<Col>,
    outline: Col,
}

impl Canvas {
    fn new() -> Self {
        Self { px: vec![[0.0; 4]; SIZE * SIZE], outline: [0.04, 0.03, 0.06, 1.0] }
    }

    fn blend(&mut self, shape: &Shape, grow: f32, col: Col) {
        // Bounding scan over the whole 64x64 is cheap enough (icons are rendered once).
        for y in 0..SIZE {
            for x in 0..SIZE {
                let d = shape.sdf((x as f32 + 0.5, y as f32 + 0.5)) - grow;
                let cov = (0.5 - d).clamp(0.0, 1.0) * col[3];
                if cov <= 0.0 {
                    continue;
                }
                let dst = &mut self.px[y * SIZE + x];
                let out_a = cov + dst[3] * (1.0 - cov);
                for i in 0..3 {
                    dst[i] = if out_a > 0.0 { (col[i] * cov + dst[i] * dst[3] * (1.0 - cov)) / out_a } else { 0.0 };
                }
                dst[3] = out_a;
            }
        }
    }

    /// A filled shape with the dark cartoon outline.
    fn o(&mut self, shape: Shape, col: Col) {
        let line = self.outline;
        self.blend(&shape, 1.7, line);
        self.blend(&shape, 0.0, col);
    }

    /// A filled shape without outline (highlights, details).
    fn f(&mut self, shape: Shape, col: Col) {
        self.blend(&shape, 0.0, col);
    }

    fn line(&mut self, a: P, b: P, r: f32, col: Col) {
        self.f(Shape::Capsule(a, b, r), col);
    }

    fn bytes(&self) -> Vec<u8> {
        self.px.iter().flat_map(|c| [c[0], c[1], c[2], c[3]].map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)).collect()
    }
}

fn poly(v: &[P]) -> Shape {
    Shape::Poly(v.to_vec())
}

// ---------------------------------------------------------------------------------------------
// The pictures
// ---------------------------------------------------------------------------------------------
const GOLD: u32 = 0xf2c230;
const WOOD: u32 = 0x9a6234;
const STEEL: u32 = 0xc9d3dc;

fn arrow(c: &mut Canvas, a: P, b: P, tip: Col) {
    c.line(a, b, 1.6, rgb(0x8a5a2c));
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = (dx * dx + dy * dy).sqrt().max(1.0);
    let (ux, uy) = (dx / l, dy / l);
    let (nx, ny) = (-uy, ux);
    c.o(poly(&[(b.0 + ux * 7.0, b.1 + uy * 7.0), (b.0 + nx * 4.5 - ux * 1.0, b.1 + ny * 4.5 - uy * 1.0), (b.0 - nx * 4.5 - ux * 1.0, b.1 - ny * 4.5 - uy * 1.0)]), tip);
    for k in [-1.0f32, 1.0] {
        c.f(poly(&[a, (a.0 + ux * 6.0 + nx * 4.0 * k, a.1 + uy * 6.0 + ny * 4.0 * k), (a.0 + ux * 9.0, a.1 + uy * 9.0)]), rgb(0xf0f0e8));
    }
}

fn gem(c: &mut Canvas, centre: P, size: f32, col: Col) {
    let (x, y) = centre;
    let s = size;
    c.o(poly(&[(x, y - s), (x + s * 0.62, y - s * 0.35), (x + s * 0.62, y + s * 0.35), (x, y + s), (x - s * 0.62, y + s * 0.35), (x - s * 0.62, y - s * 0.35)]), col);
    c.f(poly(&[(x, y - s), (x + s * 0.62, y - s * 0.35), (x, y), (x - s * 0.62, y - s * 0.35)]), lighten(col, 0.4));
    c.f(poly(&[(x, y), (x + s * 0.62, y - s * 0.35), (x + s * 0.62, y + s * 0.35), (x, y + s)]), darken(col, 0.25));
    c.f(poly(&[(x - s * 0.3, y - s * 0.55), (x - s * 0.05, y - s * 0.72), (x - s * 0.18, y - s * 0.3)]), [1.0, 1.0, 1.0, 0.8]);
}

fn heart(c: &mut Canvas, centre: P, s: f32, col: Col) {
    let (x, y) = centre;
    let lobes = |c: &mut Canvas, grow: f32, col: Col| {
        c.blend(&Shape::Circle((x - s * 0.5, y - s * 0.3), s * 0.55), grow, col);
        c.blend(&Shape::Circle((x + s * 0.5, y - s * 0.3), s * 0.55), grow, col);
        c.blend(&poly(&[(x - s * 1.02, y - s * 0.1), (x + s * 1.02, y - s * 0.1), (x, y + s * 1.0)]), grow, col);
    };
    let line = c.outline;
    lobes(c, 1.7, line);
    lobes(c, 0.0, col);
    c.f(Shape::Ellipse((x - s * 0.58, y - s * 0.55), s * 0.2, s * 0.14), [1.0, 1.0, 1.0, 0.75]);
}

fn key(c: &mut Canvas, centre: P, big: bool) {
    let (x, y) = centre;
    let gold = rgb(GOLD);
    c.o(Shape::Rect((x, y + 4.0), 2.6, 15.0, 1.0), gold);
    let bow = if big { Shape::Rect((x, y - 15.0), 10.0, 7.0, 3.0) } else { Shape::Ring((x, y - 15.0), 7.0, 5.0) };
    if big {
        c.o(bow, rgb(0xb02828));
        c.f(Shape::Circle((x, y - 15.0), 3.0), gold);
        c.f(Shape::Rect((x, y - 15.0), 7.5, 4.2, 2.0), [0.0, 0.0, 0.0, 0.0]);
        c.f(Shape::Circle((x, y - 15.0), 3.2), rgb(0xffe27a));
    } else {
        c.o(bow, gold);
    }
    c.o(Shape::Rect((x + 5.5, y + 12.0), 5.0, 2.2, 0.8), gold);
    c.o(Shape::Rect((x + 4.5, y + 4.0), 4.0, 2.2, 0.8), gold);
    c.f(Shape::Rect((x - 0.8, y + 4.0), 0.9, 12.0, 0.4), lighten(gold, 0.6));
}

fn rupee_color_rgb(r: RupeeColor) -> Col {
    rgb(match r {
        RupeeColor::Green => 0x30c050,
        RupeeColor::Blue => 0x3a6df0,
        RupeeColor::Red => 0xe03030,
        RupeeColor::Purple => 0xa040d8,
        RupeeColor::Gold => 0xf0b820,
    })
}

fn rupee(c: &mut Canvas, col: Col) {
    let (x, y) = (32.0, 32.0);
    c.o(poly(&[(x, y - 24.0), (x + 14.0, y - 10.0), (x + 14.0, y + 10.0), (x, y + 24.0), (x - 14.0, y + 10.0), (x - 14.0, y - 10.0)]), col);
    c.f(poly(&[(x, y - 24.0), (x + 14.0, y - 10.0), (x, y - 3.0), (x - 14.0, y - 10.0)]), lighten(col, 0.38));
    c.f(poly(&[(x, y - 3.0), (x + 14.0, y - 10.0), (x + 14.0, y + 10.0), (x, y + 24.0)]), darken(col, 0.22));
    c.f(poly(&[(x - 9.0, y - 11.0), (x - 4.0, y - 16.0), (x - 6.0, y - 3.0)]), [1.0, 1.0, 1.0, 0.8]);
}

fn bottle(c: &mut Canvas, kind: Bottle) {
    let glass = [0.82, 0.92, 1.0, 0.55];
    let liquid = match kind {
        Bottle::Empty => None,
        Bottle::RedPotion => Some(rgb(0xe03232)),
        Bottle::GreenPotion => Some(rgb(0x3ccc50)),
        Bottle::BluePotion => Some(rgb(0x3a6df0)),
        Bottle::Milk | Bottle::HalfMilk => Some(rgb(0xfff4e8)),
        Bottle::BlueFire => Some(rgb(0x58b8ff)),
        Bottle::Fairy => Some(rgb(0xffb8f0)),
        Bottle::Fish => Some(rgb(0x58a0d8)),
        Bottle::RutoLetter => Some(rgb(0xf0e6b8)),
        Bottle::Bug => Some(rgb(0x8a6a3a)),
        Bottle::BigPoe | Bottle::Poe => Some(rgb(0xb8f0a0)),
    };
    c.o(Shape::Rect((32.0, 17.0), 5.0, 6.0, 1.5), glass);
    c.o(Shape::Ellipse((32.0, 41.0), 17.0, 19.0), glass);
    if let Some(l) = liquid {
        let level = if kind == Bottle::HalfMilk { 6.0 } else { 0.0 };
        c.f(Shape::Ellipse((32.0, 44.0 + level / 2.0), 14.5, 14.5 - level / 2.0), l);
    }
    c.f(Shape::Rect((32.0, 12.0), 6.0, 3.5, 1.2), rgb(0xb5733a));
    c.f(Shape::Ellipse((25.0, 39.0), 2.6, 7.0), [1.0, 1.0, 1.0, 0.55]);
    match kind {
        Bottle::Fairy => {
            c.f(Shape::Circle((32.0, 42.0), 3.0), rgb(0xffffff));
            c.f(Shape::Ellipse((27.0, 39.0), 4.0, 2.4), [1.0, 1.0, 1.0, 0.7]);
            c.f(Shape::Ellipse((37.0, 39.0), 4.0, 2.4), [1.0, 1.0, 1.0, 0.7]);
        }
        Bottle::Fish => {
            c.f(Shape::Ellipse((32.0, 43.0), 8.0, 4.0), rgb(0xf08838));
            c.f(poly(&[(39.0, 43.0), (45.0, 38.0), (45.0, 48.0)]), rgb(0xf08838));
        }
        Bottle::RutoLetter => c.f(Shape::Rect((32.0, 42.0), 8.0, 6.0, 1.0), rgb(0xfffbe8)),
        Bottle::Bug => {
            c.f(Shape::Ellipse((32.0, 44.0), 4.5, 3.0), rgb(0x3a2a18));
            c.line((26.0, 44.0), (38.0, 44.0), 0.5, rgb(0x3a2a18));
        }
        Bottle::Poe => c.f(Shape::Circle((32.0, 43.0), 5.0), rgb(0xffe78a)),
        Bottle::BigPoe => {
            c.f(Shape::Circle((32.0, 43.0), 8.0), rgb(0xffe78a));
            c.f(Shape::Circle((29.0, 41.0), 1.6), rgb(0x303040));
            c.f(Shape::Circle((35.0, 41.0), 1.6), rgb(0x303040));
        }
        _ => {}
    }
}

fn sword(c: &mut Canvas, kind: Sword) {
    let (blade, guard, grip) = match kind {
        Sword::Kokiri => (rgb(STEEL), rgb(0x3a8a4a), rgb(0x6a4424)),
        Sword::Master => (rgb(0xe8f0ff), rgb(0x3050a0), rgb(0x2a3a8a)),
        Sword::Biggoron => (rgb(0xf4eec8), rgb(0xb0802a), rgb(0x6a4424)),
    };
    let long = if kind == Sword::Biggoron { 1.0 } else { 0.0 };
    c.o(poly(&[(32.0, 4.0 - long), (38.0, 12.0), (36.0, 42.0), (28.0, 42.0), (26.0, 12.0)]), blade);
    c.f(Shape::Rect((32.0, 25.0), 0.8, 16.0, 0.4), darken(blade, 0.25));
    c.o(Shape::Rect((32.0, 44.0), 14.0, 3.0, 1.2), guard);
    c.o(Shape::Rect((32.0, 51.0), 3.4, 5.0, 1.2), grip);
    c.o(Shape::Circle((32.0, 57.0), 3.2), rgb(GOLD));
}

fn shield(c: &mut Canvas, kind: Shield) {
    let outline = [(32.0, 6.0), (51.0, 12.0), (51.0, 32.0), (32.0, 58.0), (13.0, 32.0), (13.0, 12.0)];
    match kind {
        Shield::Deku => {
            c.o(poly(&outline), rgb(0xb07a3e));
            for x in [22.0, 32.0, 42.0] {
                c.f(Shape::Rect((x, 30.0), 0.7, 22.0, 0.3), rgb(0x7a4a22));
            }
            c.f(Shape::Ellipse((32.0, 26.0), 7.0, 6.0), rgb(0xd24a3a));
            c.f(Shape::Ellipse((32.0, 26.0), 3.0, 2.4), rgb(0xfff0d0));
        }
        Shield::Hylian => {
            c.o(poly(&outline), rgb(0x4a60b8));
            c.f(poly(&[(32.0, 14.0), (45.0, 18.0), (45.0, 31.0), (32.0, 48.0), (19.0, 31.0), (19.0, 18.0)]), rgb(0x6a7cd0));
            c.f(poly(&[(32.0, 18.0), (37.0, 30.0), (32.0, 28.0), (27.0, 30.0)]), rgb(0xf4e8b8));
            c.f(Shape::Circle((32.0, 24.0), 3.0), rgb(0xd24a3a));
        }
        Shield::Mirror => {
            c.o(poly(&outline), rgb(0xd8e0e8));
            c.f(poly(&[(32.0, 12.0), (46.0, 16.0), (46.0, 31.0), (32.0, 50.0), (18.0, 31.0), (18.0, 16.0)]), rgb(0xf4f8ff));
            c.f(poly(&[(32.0, 18.0), (38.0, 36.0), (26.0, 36.0)]), rgb(0xd8a028));
            c.f(Shape::Circle((32.0, 29.0), 3.2), rgb(0xc03030));
        }
    }
}

fn tunic(c: &mut Canvas, kind: Tunic) {
    let col = rgb(match kind {
        Tunic::Kokiri => 0x3aa850,
        Tunic::Goron => 0xd23a32,
        Tunic::Zora => 0x3a78e0,
    });
    c.o(poly(&[(22.0, 10.0), (42.0, 10.0), (58.0, 20.0), (51.0, 31.0), (45.0, 27.0), (45.0, 56.0), (19.0, 56.0), (19.0, 27.0), (13.0, 31.0), (6.0, 20.0)]), col);
    c.f(poly(&[(25.0, 10.0), (39.0, 10.0), (32.0, 21.0)]), darken(col, 0.45));
    c.f(Shape::Rect((32.0, 46.0), 13.0, 2.4, 0.5), rgb(0x6a4424));
    c.f(Shape::Rect((32.0, 46.0), 2.6, 3.0, 0.5), rgb(GOLD));
}

fn boots(c: &mut Canvas, kind: Boots) {
    let col = rgb(match kind {
        Boots::Kokiri => 0x8a5a2e,
        Boots::Iron => 0x5a6068,
        Boots::Hover => 0xc8b878,
    });
    c.o(poly(&[(20.0, 8.0), (38.0, 8.0), (38.0, 34.0), (56.0, 42.0), (56.0, 54.0), (14.0, 54.0), (14.0, 40.0), (20.0, 36.0)]), col);
    c.f(Shape::Rect((35.0, 50.0), 21.0, 3.0, 0.8), darken(col, 0.5));
    c.f(Shape::Rect((29.0, 12.0), 9.0, 3.0, 1.0), lighten(col, 0.35));
    if kind == Boots::Hover {
        c.f(Shape::Ellipse((32.0, 58.0), 18.0, 3.0), [0.55, 0.8, 1.0, 0.85]);
    }
    if kind == Boots::Iron {
        c.f(Shape::Circle((24.0, 20.0), 2.0), rgb(0xd8dde0));
        c.f(Shape::Circle((24.0, 30.0), 2.0), rgb(0xd8dde0));
    }
}

fn magic_gem(c: &mut Canvas, col: Col) {
    c.f(Shape::Circle((32.0, 32.0), 27.0), [col[0], col[1], col[2], 0.25]);
    c.f(Shape::Circle((32.0, 32.0), 21.0), [col[0], col[1], col[2], 0.28]);
    gem(c, (32.0, 32.0), 19.0, col);
}

fn note(c: &mut Canvas, col: Col) {
    c.o(Shape::Ellipse((24.0, 46.0), 9.0, 6.5), col);
    c.o(Shape::Rect((32.0, 28.0), 2.2, 18.0, 0.5), col);
    c.o(poly(&[(33.0, 10.0), (50.0, 18.0), (50.0, 28.0), (33.0, 21.0)]), col);
}

/// Rasterise `icon` into 64x64 straight-alpha RGBA.
pub fn render(icon: Icon) -> Vec<u8> {
    let mut c = Canvas::new();
    match icon {
        Icon::Item(item) => item_icon(&mut c, item),
        Icon::Bottle(b) => bottle(&mut c, b),
        Icon::Sword(s) => sword(&mut c, s),
        Icon::Shield(s) => shield(&mut c, s),
        Icon::Tunic(t) => tunic(&mut c, t),
        Icon::Boots(b) => boots(&mut c, b),
        Icon::Song(s) => note(&mut c, rgb(match s {
            Song::Minuet => 0x58c858,
            Song::Bolero => 0xe84a3a,
            Song::Serenade => 0x4a78f0,
            Song::Requiem => 0xe8a838,
            Song::Nocturne => 0xa858d8,
            Song::Prelude => 0xf0e060,
            Song::Sarias => 0x58d8a0,
            Song::Eponas => 0xd8884a,
            Song::Lullaby => 0xf08ab8,
            Song::Suns => 0xffc84a,
            Song::Time => 0x6ab0f0,
            Song::Storms => 0x9aa4b4,
        })),
        Icon::Rupee(r) => rupee(&mut c, rupee_color_rgb(r)),
        Icon::Heart => heart(&mut c, (32.0, 28.0), 22.0, rgb(0xe83040)),
        Icon::HeartPiece => {
            heart(&mut c, (32.0, 28.0), 22.0, rgb(0x6a2030));
            // A quarter of the heart in bright red: the piece.
            c.f(poly(&[(32.0, 30.0), (9.0, 22.0), (11.0, 11.0), (20.0, 7.0), (28.0, 11.0), (32.0, 17.0)]), rgb(0xff4a58));
            c.f(Shape::Ellipse((19.0, 14.0), 4.0, 2.6), [1.0, 1.0, 1.0, 0.7]);
        }
        Icon::SmallKey => key(&mut c, (32.0, 34.0), false),
        Icon::BossKey => key(&mut c, (32.0, 34.0), true),
        Icon::Compass => {
            c.o(Shape::Circle((32.0, 32.0), 21.0), rgb(0xd8b048));
            c.f(Shape::Circle((32.0, 32.0), 16.0), rgb(0xf8f0d0));
            c.f(poly(&[(32.0, 18.0), (36.0, 32.0), (28.0, 32.0)]), rgb(0xd83a32));
            c.f(poly(&[(32.0, 46.0), (36.0, 32.0), (28.0, 32.0)]), rgb(0x40507a));
        }
        Icon::DungeonMap => {
            c.o(poly(&[(8.0, 16.0), (22.0, 10.0), (42.0, 16.0), (56.0, 10.0), (56.0, 48.0), (42.0, 54.0), (22.0, 48.0), (8.0, 54.0)]), rgb(0xf0dc9c));
            c.line((22.0, 10.0), (22.0, 48.0), 0.8, rgb(0xb89850));
            c.line((42.0, 16.0), (42.0, 54.0), 0.8, rgb(0xb89850));
            c.line((14.0, 36.0), (30.0, 30.0), 1.2, rgb(0xd83a32));
            c.line((30.0, 30.0), (38.0, 40.0), 1.2, rgb(0xd83a32));
            c.f(Shape::Circle((48.0, 26.0), 2.6), rgb(0x3a8a4a));
        }
        Icon::Mushroom => {
            c.o(Shape::Rect((32.0, 44.0), 8.0, 12.0, 4.0), rgb(0xf0e6c8));
            c.o(Shape::Ellipse((32.0, 28.0), 22.0, 16.0), rgb(0xc8683a));
            for p in [(22.0, 24.0), (34.0, 17.0), (42.0, 28.0), (28.0, 34.0)] {
                c.f(Shape::Circle(p, 3.2), rgb(0xf8e0c0));
            }
        }
        Icon::Gauntlets => {
            c.o(Shape::Rect((32.0, 46.0), 14.0, 10.0, 3.0), rgb(0xb8c0c8));
            c.o(Shape::Rect((32.0, 28.0), 16.0, 14.0, 4.0), rgb(0xd8dee4));
            for x in [20.0, 27.0, 34.0, 41.0] {
                c.o(Shape::Rect((x, 15.0), 2.8, 8.0, 1.4), rgb(0xd8dee4));
            }
            c.f(Shape::Circle((32.0, 30.0), 4.0), rgb(0xd8a028));
        }
        Icon::Arrows => {
            arrow(&mut c, (14.0, 52.0), (44.0, 22.0), rgb(STEEL));
            arrow(&mut c, (24.0, 56.0), (54.0, 26.0), rgb(STEEL));
        }
        Icon::BombBag => {
            c.o(poly(&[(22.0, 18.0), (42.0, 18.0), (54.0, 54.0), (10.0, 54.0)]), rgb(0x8a5a2e));
            c.o(Shape::Rect((32.0, 17.0), 13.0, 4.0, 2.0), rgb(0x6a4424));
            c.o(Shape::Circle((32.0, 40.0), 9.0), rgb(0x30323c));
            c.f(Shape::Circle((29.0, 37.0), 2.2), [1.0, 1.0, 1.0, 0.6]);
            c.f(Shape::Circle((38.0, 28.0), 2.4), rgb(0xf0a030));
        }
        Icon::Magic => magic_gem(&mut c, rgb(0x40c860)),
        Icon::Questionmark => {
            c.o(Shape::Circle((32.0, 32.0), 22.0), rgb(0x7a88a8));
            c.f(Shape::Ring((32.0, 25.0), 7.0, 4.5), rgb(0xffffff));
            c.f(Shape::Rect((32.0, 38.0), 2.2, 5.0, 0.5), rgb(0xffffff));
            c.f(Shape::Circle((32.0, 48.0), 3.0), rgb(0xffffff));
        }
    }
    c.bytes()
}

fn item_icon(c: &mut Canvas, item: Item) {
    match item {
        Item::DekuStick => {
            c.o(Shape::Capsule((18.0, 52.0), (44.0, 14.0), 5.0), rgb(0xb4753a));
            c.line((16.0, 51.0), (40.0, 16.0), 1.0, rgb(0xd49a5a));
            c.o(Shape::Ellipse((46.0, 11.0), 5.0, 4.0), rgb(0x6a4220));
        }
        Item::DekuNut => {
            c.o(Shape::Circle((32.0, 36.0), 17.0), rgb(0xb8792e));
            c.f(Shape::Ellipse((26.0, 29.0), 5.0, 3.0), [1.0, 1.0, 1.0, 0.45]);
            c.o(Shape::Ellipse((32.0, 22.0), 11.0, 5.5), rgb(0x5a8a2a));
            c.o(Shape::Rect((32.0, 15.0), 1.8, 4.0, 0.5), rgb(0x5a8a2a));
        }
        Item::Bomb => {
            c.o(Shape::Circle((30.0, 38.0), 19.0), rgb(0x32343e));
            c.f(Shape::Ellipse((23.0, 31.0), 5.5, 3.5), [1.0, 1.0, 1.0, 0.45]);
            c.o(Shape::Rect((38.0, 20.0), 6.0, 5.0, 1.5), rgb(0x8a8e98));
            c.line((40.0, 16.0), (46.0, 9.0), 1.4, rgb(0xd8c890));
            c.f(Shape::Circle((47.0, 8.0), 3.6), rgb(0xffc230));
            c.f(Shape::Circle((47.0, 8.0), 1.6), rgb(0xffffff));
        }
        Item::Bow => {
            let pts: Vec<P> = (0..=12).map(|i| {
                let a = (-65.0 + 130.0 * i as f32 / 12.0).to_radians();
                (16.0 + 24.0 * a.cos(), 32.0 + 24.0 * a.sin())
            }).collect();
            for w in pts.windows(2) {
                c.blend(&Shape::Capsule(w[0], w[1], 4.4), 1.7, [0.04, 0.03, 0.06, 1.0]);
            }
            for w in pts.windows(2) {
                c.f(Shape::Capsule(w[0], w[1], 4.4), rgb(0x9a6234));
            }
            c.line(pts[0], pts[12], 0.7, rgb(0xf0f0e0));
            arrow(c, (16.0, 32.0), (52.0, 32.0), rgb(STEEL));
        }
        Item::FireArrow => {
            c.f(Shape::Circle((46.0, 22.0), 13.0), [1.0, 0.5, 0.1, 0.35]);
            arrow(c, (12.0, 54.0), (46.0, 20.0), rgb(0xff7a20));
        }
        Item::IceArrow => {
            c.f(Shape::Circle((46.0, 22.0), 13.0), [0.4, 0.8, 1.0, 0.35]);
            arrow(c, (12.0, 54.0), (46.0, 20.0), rgb(0x78d8ff));
        }
        Item::LightArrow => {
            c.f(Shape::Circle((46.0, 22.0), 13.0), [1.0, 0.95, 0.4, 0.4]);
            arrow(c, (12.0, 54.0), (46.0, 20.0), rgb(0xfff08a));
        }
        Item::DinsFire => magic_gem(c, rgb(0xe83a2a)),
        Item::FaroresWind => magic_gem(c, rgb(0x3ccc58)),
        Item::NayrusLove => magic_gem(c, rgb(0x4a78f0)),
        Item::Slingshot => {
            c.o(poly(&[(30.0, 58.0), (30.0, 38.0), (14.0, 14.0), (21.0, 10.0), (32.0, 28.0), (43.0, 10.0), (50.0, 14.0), (34.0, 38.0), (34.0, 58.0)]), rgb(0x9a6234));
            c.line((17.0, 14.0), (32.0, 46.0), 0.9, rgb(0xe8d8a0));
            c.line((47.0, 14.0), (32.0, 46.0), 0.9, rgb(0xe8d8a0));
            c.o(Shape::Circle((32.0, 46.0), 4.0), rgb(0x58a838));
        }
        Item::Ocarina => {
            c.o(Shape::Ellipse((30.0, 34.0), 22.0, 15.0), rgb(0x3f78d8));
            c.o(Shape::Capsule((46.0, 22.0), (54.0, 12.0), 4.0), rgb(0x3f78d8));
            for p in [(20.0, 33.0), (28.0, 31.0), (36.0, 33.0), (28.0, 40.0)] {
                c.f(Shape::Circle(p, 2.5), rgb(0x10285a));
            }
            c.f(Shape::Ellipse((22.0, 26.0), 7.0, 2.6), [1.0, 1.0, 1.0, 0.45]);
        }
        Item::Bombchu => {
            c.o(Shape::Ellipse((32.0, 38.0), 17.0, 14.0), rgb(0xe85a6a));
            c.o(Shape::Circle((19.0, 22.0), 6.0), rgb(0xf2a0a8));
            c.o(Shape::Circle((45.0, 22.0), 6.0), rgb(0xf2a0a8));
            c.f(Shape::Circle((26.0, 33.0), 2.4), rgb(0xffffff));
            c.f(Shape::Circle((38.0, 33.0), 2.4), rgb(0xffffff));
            c.f(Shape::Circle((26.0, 33.5), 1.2), rgb(0x101020));
            c.f(Shape::Circle((38.0, 33.5), 1.2), rgb(0x101020));
            c.f(Shape::Circle((32.0, 40.0), 2.2), rgb(0x40202a));
            c.line((32.0, 50.0), (44.0, 57.0), 1.1, rgb(0xd8a0a8));
        }
        Item::Hookshot => {
            c.o(Shape::Rect((22.0, 44.0), 5.0, 12.0, 2.0), rgb(0x6a4424));
            c.o(Shape::Capsule((22.0, 34.0), (46.0, 18.0), 3.0), rgb(0x9aa4ae));
            c.o(poly(&[(44.0, 24.0), (54.0, 6.0), (56.0, 16.0), (52.0, 22.0)]), rgb(STEEL));
            c.o(poly(&[(40.0, 16.0), (52.0, 6.0), (46.0, 22.0)]), rgb(STEEL));
        }
        Item::Boomerang => {
            c.o(poly(&[(10.0, 20.0), (26.0, 14.0), (50.0, 36.0), (46.0, 56.0), (36.0, 56.0), (36.0, 40.0)]), rgb(0xd8963a));
            c.f(poly(&[(14.0, 20.0), (25.0, 17.0), (42.0, 36.0)]), rgb(0xf4c46a));
        }
        Item::LensOfTruth => {
            c.o(Shape::Capsule((42.0, 42.0), (54.0, 54.0), 4.0), rgb(0x6a4424));
            c.o(Shape::Circle((28.0, 28.0), 19.0), rgb(0xd8b048));
            c.f(Shape::Circle((28.0, 28.0), 14.5), rgb(0xbfe8ff));
            c.f(Shape::Ellipse((28.0, 28.0), 9.0, 5.0), rgb(0xffffff));
            c.f(Shape::Circle((28.0, 28.0), 3.6), rgb(0x2a4aa0));
        }
        Item::MagicBean => {
            c.o(Shape::Ellipse((32.0, 34.0), 11.0, 17.0), rgb(0x58b838));
            c.f(Shape::Ellipse((28.0, 26.0), 3.0, 6.0), [1.0, 1.0, 1.0, 0.45]);
            c.line((32.0, 20.0), (32.0, 48.0), 0.8, rgb(0x3a8a24));
        }
        Item::MegatonHammer => {
            c.o(Shape::Capsule((22.0, 56.0), (40.0, 22.0), 3.2), rgb(0x9a6234));
            c.o(poly(&[(24.0, 20.0), (44.0, 6.0), (58.0, 24.0), (38.0, 38.0)]), rgb(0x8a929c));
            c.f(poly(&[(26.0, 20.0), (44.0, 8.0), (50.0, 16.0), (32.0, 28.0)]), rgb(0xc4ccd4));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Bevy glue
// ---------------------------------------------------------------------------------------------
/// Icons built so far (the game's own picture when the import has it, else rasterised on first use; the buffers are 16 KB each).
#[derive(Resource, Default)]
pub struct IconCache {
    map: HashMap<Icon, Handle<Image>>,
    real: Option<RealIcons>,
}

impl IconCache {
    /// The cache for an app: extracted icons from the app's `AssetRoot` when it has them, procedural ones otherwise.
    pub fn for_world(world: &World) -> Self {
        let real = world.get_resource::<crate::render::AssetRoot>().and_then(|r| RealIcons::load(&r.0));
        match &real {
            Some(r) => info!("item icons: {} extracted OoT icons", r.len()),
            None => info!("item icons: no extracted icons, using the procedural ones"),
        }
        Self { real, ..default() }
    }

    /// A cache that draws the extracted icons of `<assets>/oot-icons` where it has them.
    pub fn with_assets(assets: &Path) -> Self {
        Self { real: RealIcons::load(assets), ..default() }
    }

    /// Whether `icon` is drawn from the extracted set (false: procedural).
    pub fn is_real(&self, icon: Icon) -> bool {
        self.real.as_ref().zip(real_id(icon)).is_some_and(|(r, id)| r.has(id))
    }

    pub fn get(&mut self, icon: Icon, images: &mut Assets<Image>) -> Handle<Image> {
        let real = &self.real;
        self.map
            .entry(icon)
            .or_insert_with(|| {
                let from_rom = real.as_ref().zip(real_id(icon)).and_then(|(r, id)| r.pixels(id));
                let (w, h, data) = from_rom.unwrap_or_else(|| (SIZE as u32, SIZE as u32, render(icon)));
                let mut image = Image::new(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, data, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
                image.sampler = ImageSampler::linear();
                images.add(image)
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(icon: Icon) -> f32 {
        render(icon).chunks(4).map(|p| p[3] as f32 / 255.0).sum::<f32>() / (SIZE * SIZE) as f32
    }

    fn all_icons() -> Vec<Icon> {
        let mut v: Vec<Icon> = Item::ALL.iter().map(|i| Icon::Item(*i)).collect();
        v.extend([Bottle::Empty, Bottle::RedPotion, Bottle::GreenPotion, Bottle::BluePotion, Bottle::Fairy, Bottle::Fish, Bottle::Milk, Bottle::HalfMilk, Bottle::RutoLetter, Bottle::BlueFire, Bottle::Bug, Bottle::BigPoe, Bottle::Poe].map(Icon::Bottle));
        v.extend([Sword::Kokiri, Sword::Master, Sword::Biggoron].map(Icon::Sword));
        v.extend([Shield::Deku, Shield::Hylian, Shield::Mirror].map(Icon::Shield));
        v.extend([Tunic::Kokiri, Tunic::Goron, Tunic::Zora].map(Icon::Tunic));
        v.extend([Boots::Kokiri, Boots::Iron, Boots::Hover].map(Icon::Boots));
        v.extend(Song::ALL.map(Icon::Song));
        v.extend([RupeeColor::Green, RupeeColor::Blue, RupeeColor::Red, RupeeColor::Purple, RupeeColor::Gold].map(Icon::Rupee));
        v.extend([Icon::Heart, Icon::HeartPiece, Icon::SmallKey, Icon::BossKey, Icon::Compass, Icon::DungeonMap, Icon::Mushroom, Icon::Gauntlets, Icon::Arrows, Icon::BombBag, Icon::Magic, Icon::Questionmark]);
        v
    }

    #[test]
    fn every_icon_draws_something_visible_inside_the_frame() {
        for icon in all_icons() {
            let c = coverage(icon);
            assert!((0.08..0.85).contains(&c), "{icon:?} covers {c}");
            // Nothing is painted on the outer 1 px border (the picture is centred with a margin).
            let px = render(icon);
            for i in 0..SIZE {
                for (x, y) in [(i, 0), (i, SIZE - 1), (0, i), (SIZE - 1, i)] {
                    assert!(px[(y * SIZE + x) * 4 + 3] < 40, "{icon:?} touches the frame at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn icons_are_distinct_and_rupee_colours_read() {
        let all = all_icons();
        let hashes: Vec<Vec<u8>> = all.iter().map(|i| render(*i)).collect();
        for i in 0..hashes.len() {
            for j in i + 1..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "{:?} == {:?}", all[i], all[j]);
            }
        }
        // The centre pixel of each rupee is that rupee's hue.
        let centre = |r| {
            let px = render(Icon::Rupee(r));
            let o = (28 * SIZE + 36) * 4;
            [px[o], px[o + 1], px[o + 2]]
        };
        assert!(centre(RupeeColor::Green)[1] > centre(RupeeColor::Green)[0]);
        assert!(centre(RupeeColor::Blue)[2] > centre(RupeeColor::Blue)[0]);
        assert!(centre(RupeeColor::Red)[0] > centre(RupeeColor::Red)[2]);
    }

    #[test]
    fn chest_rewards_and_rupee_values_pick_icons() {
        assert_eq!(chest_icon(0x4e), Icon::Rupee(RupeeColor::Red));
        assert_eq!(chest_icon(0x3e), Icon::HeartPiece);
        assert_eq!(chest_icon(0x05), Icon::Item(Item::Slingshot));
        assert_eq!(chest_icon(0xff), Icon::Questionmark);
        assert_eq!(rupee_color(1), RupeeColor::Green);
        assert_eq!(rupee_color(50), RupeeColor::Purple);
        assert_eq!(rupee_color(200), RupeeColor::Gold);
    }

    // ---- the extracted (real) icons ----
    fn known_ids() -> std::collections::HashSet<&'static str> {
        byor_rs::oot::icons::ITEM_IDS.split_whitespace().chain(byor_rs::oot::icons::QUEST_IDS.split_whitespace()).collect()
    }

    /// A scratch directory with `oot-icons/index.json` and 32x32 PNGs for `ids` (a flat colour per id).
    fn icon_dir(tag: &str, ids: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("jumpman-icons-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("oot-icons")).unwrap();
        let mut entries = Vec::new();
        for (n, id) in ids.iter().enumerate() {
            let px: Vec<u8> = (0..32 * 32).flat_map(|_| [10 + n as u8, 100, 200, 255]).collect();
            std::fs::write(root.join(format!("oot-icons/{id}.png")), byor_rs::oot::model::png(32, 32, &px)).unwrap();
            entries.push(format!("\"{id}\":{{\"file\":\"{id}.png\",\"w\":32,\"h\":32}}"));
        }
        std::fs::write(root.join("oot-icons/index.json"), format!("{{\"version\":1,\"icons\":{{{}}}}}", entries.join(","))).unwrap();
        root
    }

    #[test]
    fn index_parsing_keeps_good_entries_and_drops_bad_ones() {
        let ok = parse_index(r#"{"version":1,"icons":{"bomb":{"file":"bomb.png","w":32,"h":32},"heart_piece":{"file":"heart_piece.png","w":24,"h":24}}}"#);
        assert_eq!(ok.len(), 2);
        assert_eq!(ok["bomb"], RealEntry { file: "bomb.png".into(), w: 32, h: 32 });
        assert_eq!(ok["heart_piece"].w, 24);
        // Path escapes, zero sizes, missing fields and a wrong version are rejected; garbage gives an empty set.
        let bad = parse_index(r#"{"version":1,"icons":{"a":{"file":"../x.png","w":32,"h":32},"b":{"file":"b.png","w":0,"h":32},"c":{"file":"c.png"},"d":{"file":"/etc/d.png","w":1,"h":1},"e":{"file":"e.png","w":32,"h":32}}}"#);
        assert_eq!(bad.keys().collect::<Vec<_>>(), vec!["e"]);
        assert!(parse_index(r#"{"version":2,"icons":{"bomb":{"file":"bomb.png","w":32,"h":32}}}"#).is_empty());
        assert!(parse_index("not json").is_empty());
        assert!(parse_index("").is_empty());
    }

    #[test]
    fn every_icon_with_a_picture_maps_to_an_extracted_id() {
        let known = known_ids();
        let mut seen = std::collections::HashMap::new();
        for icon in all_icons() {
            match real_id(icon) {
                Some(id) => {
                    assert!(known.contains(id), "{icon:?} -> {id} is not exported");
                    seen.entry(id).or_insert_with(Vec::new).push(icon);
                }
                // Only these have no extracted picture.
                None => assert!(matches!(icon, Icon::Song(_) | Icon::Rupee(_) | Icon::Questionmark), "{icon:?} has no extracted icon"),
            }
        }
        // Distinct items get distinct pictures.
        for (id, icons) in &seen {
            assert_eq!(icons.len(), 1, "{id} is shared by {icons:?}");
        }
        // Spot-check the order-sensitive ones.
        assert_eq!(real_id(Icon::Item(Item::Bomb)), Some("bomb"));
        assert_eq!(real_id(Icon::Item(Item::Hookshot)), Some("hookshot"));
        assert_eq!(real_id(Icon::Bottle(Bottle::BluePotion)), Some("bottle_potion_blue"));
        assert_eq!(real_id(Icon::Sword(Sword::Master)), Some("sword_master"));
    }

    #[test]
    fn chest_cards_map_to_real_icons_except_rupees() {
        for item in 0..=0xffu32 {
            if crate::zelda::save::treasure_reward(item).is_none() {
                continue;
            }
            let icon = chest_icon(item);
            assert_ne!(icon, Icon::Questionmark, "chest item {item:#x} has no icon");
            assert!(real_id(icon).is_some() || matches!(icon, Icon::Rupee(_)), "chest item {item:#x} -> {icon:?}");
        }
    }

    #[test]
    fn no_extracted_directory_falls_back_to_procedural() {
        let empty = std::env::temp_dir().join(format!("jumpman-icons-none-{}", std::process::id()));
        assert!(RealIcons::load(&empty).is_none());
        let mut cache = IconCache::with_assets(&empty);
        let mut images = Assets::<Image>::default();
        let h = cache.get(Icon::Item(Item::Bomb), &mut images);
        assert!(!cache.is_real(Icon::Item(Item::Bomb)));
        let img = images.get(&h).unwrap();
        assert_eq!((img.width(), img.height()), (SIZE as u32, SIZE as u32));
        assert_eq!(img.data.as_deref(), Some(render(Icon::Item(Item::Bomb)).as_slice()));
    }

    #[test]
    fn a_missing_icon_falls_back_on_its_own_and_present_ones_are_enlarged() {
        let root = icon_dir("partial", &["bomb"]);
        let mut cache = IconCache::with_assets(&root);
        let mut images = Assets::<Image>::default();
        assert!(cache.is_real(Icon::Item(Item::Bomb)));
        assert!(!cache.is_real(Icon::Item(Item::Bow)), "bow is not in the index");
        let handle = cache.get(Icon::Item(Item::Bomb), &mut images);
        let bomb = images.get(&handle).unwrap();
        assert_eq!((bomb.width(), bomb.height()), (128, 128));
        assert_eq!(&bomb.data.as_ref().unwrap()[..4], &[10, 100, 200, 255]);
        let bow = cache.get(Icon::Item(Item::Bow), &mut images);
        assert_eq!(images.get(&bow).unwrap().data.as_deref(), Some(render(Icon::Item(Item::Bow)).as_slice()));
        // Songs never have an extracted picture, even when the directory is there.
        assert!(!cache.is_real(Icon::Song(Song::Eponas)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_broken_file_or_wrong_size_falls_back() {
        let root = icon_dir("broken", &["bomb", "bow"]);
        std::fs::write(root.join("oot-icons/bomb.png"), b"not a png").unwrap();
        // The index claims 24x24 for a 32x32 picture.
        std::fs::write(root.join("oot-icons/index.json"), r#"{"version":1,"icons":{"bomb":{"file":"bomb.png","w":32,"h":32},"bow":{"file":"bow.png","w":24,"h":24}}}"#).unwrap();
        let real = RealIcons::load(&root).unwrap();
        assert!(real.pixels("bomb").is_none());
        assert!(real.pixels("bow").is_none());
        assert!(real.pixels("nope").is_none());
        let mut cache = IconCache::with_assets(&root);
        let mut images = Assets::<Image>::default();
        let h = cache.get(Icon::Item(Item::Bomb), &mut images);
        assert_eq!(images.get(&h).unwrap().width(), SIZE as u32);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upscaling_repeats_pixels() {
        let src = [1, 2, 3, 4, 5, 6, 7, 8];
        let up = upscale_nearest(&src, 2, 1, 2);
        assert_eq!(up.len(), 4 * 2 * 4);
        assert_eq!(&up[0..4], &[1, 2, 3, 4]);
        assert_eq!(&up[4..8], &[1, 2, 3, 4]);
        assert_eq!(&up[8..12], &[5, 6, 7, 8]);
        assert_eq!(&up[16..20], &[1, 2, 3, 4], "second row repeats the first");
    }
}
