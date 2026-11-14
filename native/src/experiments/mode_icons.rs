//! Mode icons: one bold 16x16 pixel picture per mode, drawn in code (original project art, no image files), so the
//! hub doors, the mode cards and the in-game mode list tell the modes apart at a glance.
//!
//! Each icon is a rounded tile in the mode's own colour with a 12x12 glyph on it; the glyph gets a dark outline where it
//! meets the tile, which keeps the silhouette readable at 1x on the low-res hub and when scaled up (nearest-sampled) on the
//! cards. Same look as the Star Swarm upgrade icons (`swarm_icons.rs`).
use super::ExperimentId;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub const SIZE: usize = 16;
const GLYPH: usize = 12;

/// The colours a glyph row may use: '.' tile, '#' the glyph's main colour, '+' main lightened, 'o' main darkened,
/// 'w' white, 'k' near black (never outlined, for thin lines), 'g' gold, 'r' red, 'b' brown, 'c' cream, 'u' sky blue,
/// 'y' yellow.
pub const PALETTE: &[u8] = b".#+owkgrbcuy";

/// The mode's colour (tile, door sign and card accent): every listed mode has its own hue.
pub fn color(id: ExperimentId) -> u32 {
    use ExperimentId::*;
    match id {
        Brawl => 0xd0302a,
        Swarm => 0x7a3fd0,
        Gate => 0x6e7c90,
        Deck => 0x139c94,
        LastStar => 0x2f9a3a,
        Race => 0x3a9ae0,
        Runner => 0xe0a81c,
        Party => 0xd8459a,
        Relic => 0x8a5a2b,
        Kart => 0x2a48c8,
        God => 0x4a3a98,
        Dimensions => 0x18b4c8,
        Hundred => 0x98243a,
        Shrink => 0x78b828,
        Halls => 0xd8782a,
        Diorama => 0xb08a4a,
        Planet => 0x1a5a6a,
    }
}

/// The glyph rows and its main colour.
fn glyph(id: ExperimentId) -> ([&'static str; GLYPH], u32) {
    use ExperimentId::*;
    match id {
        // A faceted gem.
        Brawl => (["............", "............", "...######...", "..#ww#+##o..", ".#w+##+###o.", "############", ".o########o.", "..o######o..", "...o####o...", "....o##o....", ".....oo.....", "............"], 0xffc020),
        // A star in a swarm of sparks.
        Swarm => (["w....##....w", ".....##.....", "....####....", "############", ".##########.", "..########..", "...######...", "..###..###..", "w.##....##.w", ".##......##.", "............", "............"], 0xffe040),
        // A gatehouse with battlements.
        Gate => (["#.#.#..#.#.#", "############", "############", ".##########.", ".####kk####.", ".###kkkk###.", ".###kkkk###.", ".###kkkk###.", "####kkkk####", "............", "............", "............"], 0xeceef4),
        // Two cards, the front one with a gold diamond.
        Deck => (["oooooo......", "o####o......", "o####o......", "o##kkkkkkkk.", "o##kwwwwwwk.", "o##kwwwgwwk.", "o##kwwgggwk.", "o##kwwwgwwk.", "oookwwwwwwk.", "...kwwwwwwk.", "...kkkkkkkk.", "............"], 0xe04848),
        // A parachute dropping in.
        LastStar => (["...wwwwww...", ".wwrrwwrrww.", "wwrrrwwrrrww", "k..........k", ".k........k.", "..k......k..", "...k.##.k...", "....k##k....", ".....##.....", "....####....", "....#..#....", "............"], 0x2850d8),
        // A mountain with a flag on the summit.
        Race => ([".....krrr...", ".....krrrr..", ".....krr....", ".....k......", "....www.....", "...wwwww....", "...##w###...", "..#######o..", ".########oo.", "#########ooo", "............", "............"], 0x5f8a44),
        // A running shoe with speed lines.
        Runner => (["............", ".....####...", ".....#ww#...", ".....#ww##..", "www..######.", "....########", "wwww########", "...#########", "ww.wwwwwwwww", "............", "............", "............"], 0xe0382a),
        // A die.
        Party => ([".wwwwwwwwww.", "wwwwwwwwwwww", "wkkwwwwwwkkw", "wkkwwwwwwkkw", "wwwwwwwwwwww", "wwwwwkkwwwww", "wwwwwkkwwwww", "wwwwwwwwwwww", "wkkwwwwwwkkw", "wkkwwwwwwkkw", "wwwwwwwwwwww", ".wwwwwwwwww."], 0xffffff),
        // A skull.
        Relic => (["...######...", ".##########.", "############", "############", "##kkk##kkk##", "##kkk##kkk##", "###k####k###", ".#####k####.", "..########..", "..#k#k#k##..", "..########..", "............"], 0xf4ecd8),
        // A kart from the side.
        Kart => (["............", ".....www....", ".....wwwk...", "....####....", "..#########.", ".###########", "#####ww#####", "############", ".kkk....kkk.", "kkwkk..kkwkk", ".kkk....kkk.", "............"], 0xe83a2a),
        // A storm cloud and its bolt.
        God => (["...www......", "..wwwww.ww..", ".wwwwwwwwww.", "wwwwwwwwwwww", "wwwwwwwwwwww", ".wwwwwwwwww.", "....yy......", "...yy.......", "..yyyyy.....", "....yy......", "...yy.......", "..y........."], 0xffffff),
        // A cube in three shades.
        Dimensions => ([".....++.....", "...++++++...", ".++++++++++.", "##++++++++oo", "####++++oooo", "######oooooo", "######oooooo", "######oooooo", "######oooooo", ".#####ooooo.", "...###ooo...", ".....#o....."], 0xbfe8f4),
        // "100".
        Hundred => (["............", "............", ".ww.www.www.", "www.w.w.w.w.", ".ww.w.w.w.w.", ".ww.w.w.w.w.", ".ww.w.w.w.w.", ".ww.w.w.w.w.", "www.www.www.", "............", "............", "............"], 0xffffff),
        // A magnifying glass.
        Shrink => (["..wwww......", ".w++++w.....", "w++++++w....", "w++++++w....", "w++++++w....", "w++++++w....", ".w++++w.....", "..wwwwbb....", "......bbb...", ".......bbb..", "........bbb.", ".........bb."], 0x6ad0f0),
        // A framed landscape painting.
        Halls => (["gggggggggggg", "gbbbbbbbbbbg", "gbuuuuuuuubg", "gbuuuwwuuubg", "gbuuu##uuubg", "gbuu####uubg", "gbu######ubg", "gb########bg", "gbbbbbbbbbbg", "gggggggggggg", "............", "............"], 0x4a9a3a),
        // A little world under a glass dome.
        Diorama => (["...uuuuuu...", ".uuuuuuuuuu.", "uuuuuuwuuuuu", "uuuu##uuuuuu", "uuu####uuuuu", "uu########uu", ".##########.", "bbbbbbbbbbbb", "obbbbbbbbbbo", "............", "............", "............"], 0x4aa83a),
        // A ringed planet.
        Planet => (["............", ".....####...", "...#++####..", "..#++######y", "..#+######yy", "yyyyyyyyyyy.", "yy########..", "..#######o..", "...#####o...", ".....##o....", "............", "............"], 0x58c8a0),
    }
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 0xff) as f32 / 255.0, ((c >> 8) & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0]
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

const OUTLINE: [f32; 3] = [0.06, 0.06, 0.12];

/// The icon as 16x16 RGBA bytes (sRGB), row by row from the top. The four corner pixels are clear.
pub fn pixels(id: ExperimentId) -> Vec<u8> {
    let tile = rgb(color(id));
    let (rows, main) = glyph(id);
    let main = rgb(main);
    let mut px = vec![[0.0f32; 4]; SIZE * SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            let corner = (x == 0 || x == SIZE - 1) && (y == 0 || y == SIZE - 1);
            px[y * SIZE + x] = if corner {
                [0.0; 4]
            } else if edge {
                let c = scale(tile, 0.35);
                [c[0], c[1], c[2], 1.0]
            } else {
                // A soft top-to-bottom shade, with a lit top row.
                let k = if y == 1 { 1.35 } else { 1.18 - 0.36 * (y as f32 - 1.0) / (SIZE as f32 - 3.0) };
                let c = scale(tile, k);
                [c[0], c[1], c[2], 1.0]
            };
        }
    }
    // Centre the glyph's drawn pixels on the tile.
    let cells: Vec<(usize, usize, u8)> = rows.iter().enumerate().flat_map(|(gy, r)| r.bytes().enumerate().filter(|(_, c)| *c != b'.').map(move |(gx, c)| (gx, gy, c))).collect();
    let (x0, x1) = (cells.iter().map(|c| c.0).min().unwrap_or(0), cells.iter().map(|c| c.0).max().unwrap_or(0));
    let (y0, y1) = (cells.iter().map(|c| c.1).min().unwrap_or(0), cells.iter().map(|c| c.1).max().unwrap_or(0));
    let ox = (SIZE as i32 - (x1 - x0 + 1) as i32) / 2 - x0 as i32;
    let oy = (SIZE as i32 - (y1 - y0 + 1) as i32) / 2 - y0 as i32;
    let at = |gx: usize, gy: usize| ((gx as i32 + ox) as usize, (gy as i32 + oy) as usize);
    // Outline first (every tile pixel next to a glyph pixel other than a thin black line), then the glyph over it.
    for &(gx, gy, c) in &cells {
        if c == b'k' {
            continue;
        }
        let (x, y) = at(gx, gy);
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if (1..SIZE as i32 - 1).contains(&nx) && (1..SIZE as i32 - 1).contains(&ny) {
                px[ny as usize * SIZE + nx as usize] = [OUTLINE[0], OUTLINE[1], OUTLINE[2], 1.0];
            }
        }
    }
    for &(gx, gy, ch) in &cells {
        let c = match ch {
            b'#' => main,
            b'+' => mix(main, [1.0, 1.0, 1.0], 0.55),
            b'o' => scale(main, 0.55),
            b'w' => [0.97, 0.97, 1.0],
            b'k' => OUTLINE,
            b'g' => rgb(0xf0c040),
            b'r' => rgb(0xe03828),
            b'b' => rgb(0x8a5a2b),
            b'c' => rgb(0xf0c8a0),
            b'u' => rgb(0x8ad0ff),
            b'y' => rgb(0xffe040),
            _ => continue,
        };
        let (x, y) = at(gx, gy);
        px[y * SIZE + x] = [c[0], c[1], c[2], 1.0];
    }
    px.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

/// The icon as a nearest-sampled image.
pub fn image(id: ExperimentId) -> Image {
    let mut img = Image::new(Extent3d { width: SIZE as u32, height: SIZE as u32, depth_or_array_layers: 1 }, TextureDimension::D2, pixels(id), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD);
    img.sampler = ImageSampler::nearest();
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::EXPERIMENT_IDS;

    #[test]
    fn every_mode_has_a_well_formed_glyph() {
        for id in EXPERIMENT_IDS {
            let (rows, _) = glyph(id);
            for (i, r) in rows.iter().enumerate() {
                assert_eq!(r.len(), GLYPH, "{id:?} row {i} is {} wide", r.len());
                assert!(r.bytes().all(|c| PALETTE.contains(&c)), "{id:?} row {i}: unknown colour");
            }
            assert_eq!(pixels(id).len(), SIZE * SIZE * 4);
        }
    }

    #[test]
    fn every_icon_and_every_colour_is_distinct() {
        let all: Vec<Vec<u8>> = EXPERIMENT_IDS.iter().map(|id| pixels(*id)).collect();
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert_ne!(all[i], all[j], "{:?} and {:?} look the same", EXPERIMENT_IDS[i], EXPERIMENT_IDS[j]);
                // The glyphs differ too, not only the tile colour.
                assert_ne!(glyph(EXPERIMENT_IDS[i]).0, glyph(EXPERIMENT_IDS[j]).0, "{:?} and {:?} share a glyph", EXPERIMENT_IDS[i], EXPERIMENT_IDS[j]);
                // Colours far enough apart to tell at a glance (sum of channel differences).
                let (a, b) = (color(EXPERIMENT_IDS[i]), color(EXPERIMENT_IDS[j]));
                let d: i32 = [16, 8, 0].iter().map(|s| (((a >> s) & 0xff) as i32 - ((b >> s) & 0xff) as i32).abs()).sum();
                assert!(d >= 60, "{:?} and {:?} colours are too close ({d})", EXPERIMENT_IDS[i], EXPERIMENT_IDS[j]);
            }
        }
    }

    #[test]
    fn the_glyph_fills_the_middle_and_the_corners_are_clear() {
        for id in EXPERIMENT_IDS {
            let p = pixels(id);
            for (x, y) in [(0, 0), (SIZE - 1, 0), (0, SIZE - 1), (SIZE - 1, SIZE - 1)] {
                assert_eq!(p[(y * SIZE + x) * 4 + 3], 0, "{id:?} corner");
            }
            // Some outline pixels: a bold silhouette.
            let dark = p.chunks(4).filter(|c| c[0] < 30 && c[1] < 30 && c[2] < 50 && c[3] == 255).count();
            assert!(dark >= 12, "{id:?} has almost no outline ({dark})");
        }
    }

    /// `ICON_SHEET=/path/sheet.ppm cargo test ... dump_mode_icon_sheet -- --ignored`: every mode icon at 6x (a look check).
    #[test]
    #[ignore]
    fn dump_mode_icon_sheet() {
        let Ok(path) = std::env::var("ICON_SHEET") else { return };
        let (k, cols) = (6usize, 6usize);
        let rows = EXPERIMENT_IDS.len().div_ceil(cols);
        let (w, h) = (cols * (SIZE + 2) * k, rows * (SIZE + 2) * k);
        let mut out = vec![40u8; w * h * 3];
        for (n, id) in EXPERIMENT_IDS.iter().enumerate() {
            let p = pixels(*id);
            let (ox, oy) = ((n % cols) * (SIZE + 2) * k, (n / cols) * (SIZE + 2) * k);
            for y in 0..SIZE * k {
                for x in 0..SIZE * k {
                    let i = ((y / k) * SIZE + x / k) * 4;
                    let a = p[i + 3] as u32;
                    let o = ((oy + y) * w + ox + x) * 3;
                    for c in 0..3 {
                        out[o + c] = ((p[i + c] as u32 * a + 40 * (255 - a)) / 255) as u8;
                    }
                }
            }
        }
        let mut f = format!("P6\n{w} {h}\n255\n").into_bytes();
        f.extend(out);
        std::fs::write(path, f).unwrap();
    }
}
