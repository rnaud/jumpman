//! Star Swarm upgrade icons: one small pixel picture per weapon, tome, technique and evolution, drawn in code (original
//! project art: 12x12 glyphs in a 16x16 frame, no image files), so a card or a build-list row is recognised at a glance.
//!
//! The frame colour says the category: red for a weapon (takes a slot), blue for a tome (passive), green for a technique
//! (a move of the hero's own), gold for an evolution. An evolution reuses the picture of the weapon it grows from, drawn
//! brighter, with a gold frame and a white sparkle in the corner. Images are nearest-sampled, so they stay crisp at any size.
use super::swarm_core::{Upgrade, ALL_UPGRADES, UPGRADE_COUNT};
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub const SIZE: usize = 16;
const GLYPH: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Weapon,
    Tome,
    Technique,
    Evolution,
}

pub fn category(u: Upgrade) -> Category {
    if u.is_evolution() {
        Category::Evolution
    } else if u.is_weapon() {
        Category::Weapon
    } else if matches!(u, Upgrade::Quake | Upgrade::Dive) {
        Category::Technique
    } else {
        Category::Tome
    }
}

/// The frame colour of a category (also used for the level pips on the cards).
pub fn category_rgb(c: Category) -> u32 {
    match c {
        Category::Weapon => 0xe24a3a,
        Category::Tome => 0x3a8ae8,
        Category::Technique => 0x3cc65e,
        Category::Evolution => 0xffb13a,
    }
}

pub fn category_label(c: Category) -> &'static str {
    match c {
        Category::Weapon => "WEAPON",
        Category::Tome => "TOME",
        Category::Technique => "TECHNIQUE",
        Category::Evolution => "EVOLUTION",
    }
}

/// The upgrade a card or build-list name belongs to.
pub fn upgrade_named(name: &str) -> Option<Upgrade> {
    ALL_UPGRADES.iter().copied().find(|u| u.info().name == name)
}

/// The weapon (or technique) an evolution grows from: its picture is reused.
fn base_of(u: Upgrade) -> Upgrade {
    match u {
        Upgrade::StarStorm => Upgrade::Stars,
        Upgrade::Supernova => Upgrade::Nova,
        Upgrade::Cyclone => Upgrade::Whirl,
        Upgrade::ThunderGod => Upgrade::Lightning,
        Upgrade::Comet => Upgrade::Boomerang,
        Upgrade::Earthshaker => Upgrade::Quake,
        Upgrade::MasterSword => Upgrade::Sword,
        Upgrade::Inferno => Upgrade::Fireball,
        Upgrade::Singularity => Upgrade::Gravity,
        other => other,
    }
}

/// Glyph rows ('.' empty, '#' main, '+' light, 'o' dark, 'w' white, 'k' black, 'g' gold, 'r' red, 'b' brown) and the main colour.
fn glyph(u: Upgrade) -> ([&'static str; GLYPH], u32) {
    match u {
        Upgrade::Stars => (["............", ".....++.....", ".....##.....", "....####....", "++########++", ".##########.", "...######...", "...######...", "..###..###..", "..##....##..", ".##......##.", "............"], 0xf8d030),
        Upgrade::Sword => (["..........w+", ".........w+o", "........w+o.", ".......w+o..", "......w+o...", "..g..w+o....", "...gw+o.....", "....gg......", "...bgg......", "..bb..g.....", ".bb.........", "kb.........."], 0xc8d4e0),
        Upgrade::Boomerang => (["............", ".##.........", ".+##........", "..+##.......", "...+##......", "....+####...", ".....######.", "....####....", "...##o......", "..##o.......", ".##o........", "............"], 0xff9a30),
        Upgrade::Lightning => (["......####..", ".....####...", "....####....", "...####.....", "..########..", ".....####...", "....####....", "...###......", "..###.......", "..##........", ".##.........", ".#.........."], 0xffe860),
        Upgrade::Whirl => (["...######...", "..##....##..", ".##......##.", ".#...###..#.", ".#..#...#.#.", ".#..#.#.#.#.", ".#..#.##..#.", ".#..#.....#.", ".##..#...##.", "..##..###...", "...##.......", "....####...."], 0x60e0ff),
        Upgrade::Nova => (["....####....", "..##....##..", ".#........#.", ".#..####..#.", "#..#.++.#..#", "#..#+ww+#..#", "#..#+ww+#..#", "#..#.++.#..#", ".#..####..#.", ".#........#.", "..##....##..", "....####...."], 0xff80e0),
        Upgrade::Orbit => ([".....##.....", "....####....", ".....##.....", "............", ".##..oo..##.", "####oooo####", ".##..oo..##.", "............", ".....##.....", "....####....", ".....##.....", "............"], 0xa8b8c8),
        Upgrade::Flame => ([".....#......", "....##......", "....###..#..", "...####.##..", "..######.#..", "..#######...", ".##++#####..", ".#+++++###..", ".#+wwww+##..", ".##+ww+###..", "..#######...", "...#####...."], 0xff7020),
        Upgrade::Fireball => (["........+...", ".......++...", "..oo..++....", ".o####+.....", "o######.....", "##++####....", "#+ww+###....", "#+ww+###....", "##++###.....", ".######.....", "..####......", "............"], 0xe84020),
        Upgrade::Frost => ([".....##.....", "...#.##.#...", "....####....", ".#..####..#.", "..#..##..#..", "############", "############", "..#..##..#..", ".#..####..#.", "....####....", "...#.##.#...", ".....##....."], 0xa8e4ff),
        Upgrade::Poison => (["....kkkk....", ".....oo.....", ".....oo.....", ".....oo.....", "....o##o....", "...o####o...", "..o##+###o..", ".o###++###o.", ".o##+######.", ".o########o.", "..o######o..", "...oooooo..."], 0x60d040),
        Upgrade::Gravity => (["....####....", "..##++++##..", ".#+......+#.", ".#..kkkk..#.", "#+.kkkkkk.+#", "#+.kkkkkk.+#", "#+.kkkkkk.+#", "#+.kkkkkk.+#", ".#..kkkk..#.", ".#+......+#.", "..##++++##..", "....####...."], 0xa050ff),
        Upgrade::Missiles => (["..........rr", ".........rr.", "........##..", ".......##o..", "......##o...", ".....##o....", "....##o.....", "..gg#o......", ".g.go.......", ".g.g........", "..g.........", "............"], 0xb8c0c8),
        Upgrade::Damage => ([".....##.....", "....####....", "...######...", "..###..###..", ".###....###.", "###..##..###", "....####....", "...######...", "..###..###..", ".###....###.", "###......###", "............"], 0xe84848),
        Upgrade::FireRate => ([".kkkkkkkkkk.", "..########..", "..#++++++#..", "...#++++#...", "....#++#....", ".....##.....", ".....##.....", "....#..#....", "...#....#...", "..#.++++.#..", "..##++++##..", ".kkkkkkkkkk."], 0xf0c040),
        Upgrade::Bolts => (["............", "..##....##..", ".####..####.", ".####..####.", "..##....##..", "............", ".....##.....", "....####....", "....####....", ".....##.....", "............", "............"], 0x80f0ff),
        Upgrade::Area => ([".....##.....", "....####....", ".....##.....", "...######...", ".#.#....#.#.", "###......###", "###......###", ".#.#....#.#.", "...######...", ".....##.....", "....####....", ".....##....."], 0x60e8a0),
        Upgrade::Pierce => (["............", "....oo......", "....oo...#..", "....oo...##.", "###########.", "############", "###########.", "....oo...##.", "....oo...#..", "....oo......", "............", "............"], 0xd0d8e0),
        Upgrade::Speed => (["............", ".+.....###..", "++....####..", "+++...####..", ".++...####..", "......####..", "......#####.", ".....######.", "...########.", "..#########.", "..kkkkkkkkk.", "............"], 0xd05030),
        Upgrade::Jump => (["...######...", ".....##.....", "...######...", "..#......#..", "...######...", "..#......#..", "...######...", "..#......#..", "...######...", ".....##.....", ".##########.", ".kkkkkkkkkk."], 0x70e050),
        Upgrade::Quake => ([".....##.....", ".....##.....", "....####....", ".....##.....", ".#........#.", "..#......#..", "............", "############", "#o##o###o###", "##o###o##o##", "#o###o###o##", "############"], 0xc08048),
        Upgrade::Vitality => (["............", ".###....###.", "#####..#####", "##+#########", "#++#########", "############", ".##########.", "..########..", "...######...", "....####....", ".....##.....", "............"], 0xf04060),
        Upgrade::Regen => (["............", ".###....###.", "#####..#####", "#####ww#####", "###wwwwww###", "###wwwwww###", ".####ww####.", "..###ww###..", "...######...", "....####....", ".....##.....", "............"], 0xff70a0),
        Upgrade::Magnet => (["............", ".www....www.", ".www....www.", ".##......##.", ".##......##.", ".##......##.", ".##......##.", ".###....###.", "..###..###..", "...######...", "....####....", "............"], 0xe83838),
        Upgrade::Crit => ([".wwwwwwwwww.", "wwwwwwwwwwww", "wwkkwwwwkkww", "wwkkwwwwkkww", "wwwwwwwwwwww", "wwwww##wwwww", "wwwww##wwwww", "wwwwwwwwwwww", "wwkkwwwwkkww", "wwkkwwwwkkww", "wwwwwwwwwwww", ".wwwwwwwwww."], 0x40c040),
        Upgrade::Armor => ([".##########.", ".#++######o.", ".#+#######o.", ".##########.", ".####oo####.", ".###oooo###.", ".####oo####.", "..########..", "..########..", "...######...", "....####....", ".....##....."], 0x9aa8b8),
        Upgrade::Growth => ([".....++.....", "....++++....", "...++++++...", ".....++.....", ".##########.", ".#wwww#wwww#", ".#wwww#wwww#", ".#wwww#wwww#", ".#wwww#wwww#", ".##########.", "............", "............"], 0xa070ff),
        Upgrade::Dive => (["#...........", ".#..........", "..#.......#.", "...##.....#.", "....##....#.", ".....###..#.", "......####..", ".......####.", "..#########.", "...########.", "............", "............"], 0x50a0ff),
        Upgrade::SecondChance => (["....####....", "...#....#...", "...#....#...", "....####....", ".....##.....", ".##########.", ".##########.", ".....##.....", ".....##.....", ".....##.....", ".....##.....", "....####...."], 0xffd860),
        // Evolutions draw their base weapon (`base_of`).
        _ => glyph(base_of(u)),
    }
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)]
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Shrine and pot glyphs (`ShrineKind as u8`, then 5 for a pot) and their main colour; the frame is the shrine's colour.
fn shrine_glyph(kind: u8) -> ([&'static str; GLYPH], u32) {
    match kind {
        // Charge: a lightning bolt inside a circle.
        0 => (["....####....", "..##....##..", ".#.....##.#.", ".#....##..#.", "#....##....#", "#...#####..#", "#..#####...#", "#....##....#", ".#..##....#.", ".#.##.....#.", "..##....##..", "....####...."], 0x60d8ff),
        // Challenge: crossed swords.
        1 => (["w..........w", ".w........w.", "..w......w..", "...w....w...", "....w..w....", ".....ww.....", ".....ww.....", "....w..w....", "..gw....wg..", "..g......g..", ".bb......bb.", "kb........bk"], 0xff6048),
        // Greed: a horned skull over gold.
        2 => (["#..........#", "##..####..##", ".##########.", "..########..", "..#kk##kk#..", "..#kk##kk#..", "...######...", "...#o##o#...", "....####....", "..gg.gg.gg..", ".gggggggggg.", "gggggggggggg"], 0xc070ff),
        // Boss altar: a crown on a block.
        3 => (["............", ".g...g...g..", ".gg.ggg.gg..", ".ggggggggg..", ".grgggggrg..", ".ggggggggg..", "............", ".##########.", ".#oooooooo#.", ".#o######o#.", ".#oooooooo#.", ".##########."], 0xff9030),
        // Duplicator: two overlapping cards.
        4 => (["#######.....", "#+++++#.....", "#+wwww######", "#+wwww#++++#", "#+wwww#+ww+#", "#+wwww#+ww+#", "#+++++#+ww+#", "#######+ww+#", "......#+ww+#", "......#++++#", "......######", "............"], 0x58e070),
        // Pot.
        _ => (["....kkkk....", "....oooo....", ".....##.....", "...######...", "..########..", ".####++####.", ".###++#####.", ".##########.", ".##########.", "..########..", "...######...", "....oooo...."], 0xc86a3a),
    }
}

/// A shrine's colour (`ShrineKind as u8`): its icon frame, beam, circle and compass dot.
pub fn shrine_rgb(kind: u8) -> [f32; 3] {
    rgb(match kind {
        0 => 0x40c8ff,
        1 => 0xff4a3a,
        2 => 0xb050ff,
        3 => 0xff9a2a,
        4 => 0x40e060,
        _ => 0xc86a3a,
    })
}

/// A shrine's (or a pot's, `kind` 5) icon as 16x16 RGBA bytes, framed in its colour like the upgrade icons.
pub fn shrine_pixels(kind: u8) -> Vec<u8> {
    let (rows, main) = shrine_glyph(kind);
    let frame = shrine_rgb(kind);
    draw(&rows, rgb(main), frame, false)
}

/// The icon as 16x16 RGBA bytes (sRGB), row by row from the top.
pub fn pixels(u: Upgrade) -> Vec<u8> {
    let cat = category(u);
    let frame = rgb(category_rgb(cat));
    let (rows, main) = glyph(u);
    let evo = cat == Category::Evolution;
    let main = if evo { mix(rgb(main), [1.0, 1.0, 1.0], 0.25) } else { rgb(main) };
    draw(&rows, main, frame, evo)
}

fn draw(rows: &[&'static str; GLYPH], main: [f32; 3], frame: [f32; 3], evo: bool) -> Vec<u8> {
    let back = mix([0.05, 0.06, 0.16], frame, 0.18);
    let mut px = vec![[0.0f32; 4]; SIZE * SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            let ring = x == 1 || y == 1 || x == SIZE - 2 || y == SIZE - 2;
            // Rounded corners: the four outermost corner pixels stay clear.
            let corner = (x == 0 || x == SIZE - 1) && (y == 0 || y == SIZE - 1);
            px[y * SIZE + x] = if corner {
                [0.0; 4]
            } else if edge {
                [0.02, 0.02, 0.06, 1.0]
            } else if ring {
                let lit = if x == 1 || y == 1 { 1.25 } else { 0.75 };
                let c = scale(frame, lit);
                [c[0], c[1], c[2], 1.0]
            } else {
                [back[0], back[1], back[2], 1.0]
            };
        }
    }
    for (gy, row) in rows.iter().enumerate() {
        for (gx, ch) in row.bytes().enumerate() {
            let c = match ch {
                b'#' => main,
                b'+' => mix(main, [1.0, 1.0, 1.0], 0.55),
                b'o' => scale(main, 0.5),
                b'w' => [0.97, 0.97, 1.0],
                b'k' => [0.05, 0.05, 0.08],
                b'g' => rgb(0xe8b840),
                b'r' => rgb(0xe03828),
                b'b' => rgb(0x8a5a2b),
                _ => continue,
            };
            px[(gy + 2) * SIZE + gx + 2] = [c[0], c[1], c[2], 1.0];
        }
    }
    if evo {
        // A white four-point sparkle in the top-right corner.
        for (x, y) in [(13usize, 2usize), (12, 3), (13, 3), (14, 3), (13, 4)] {
            px[y * SIZE + x] = [1.0, 1.0, 1.0, 1.0];
        }
    }
    px.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

/// The icon images, made once (nearest-sampled so the pixels stay square).
#[derive(Resource, Default)]
pub struct SwarmIcons(pub Vec<Handle<Image>>);

impl SwarmIcons {
    pub fn ensure(&mut self, images: &mut Assets<Image>) {
        if self.0.len() == UPGRADE_COUNT {
            return;
        }
        self.0 = ALL_UPGRADES
            .iter()
            .map(|u| {
                let mut img = Image::new(Extent3d { width: SIZE as u32, height: SIZE as u32, depth_or_array_layers: 1 }, TextureDimension::D2, pixels(*u), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
                img.sampler = ImageSampler::nearest();
                images.add(img)
            })
            .collect();
    }
    pub fn get(&self, u: Upgrade) -> Option<Handle<Image>> {
        self.0.get(u.index()).cloned()
    }
    pub fn named(&self, name: &str) -> Option<Handle<Image>> {
        upgrade_named(name).and_then(|u| self.get(u))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_twelve_by_twelve_and_every_upgrade_has_one() {
        for u in ALL_UPGRADES {
            let (rows, _) = glyph(u);
            for (i, r) in rows.iter().enumerate() {
                assert_eq!(r.len(), GLYPH, "{u:?} row {i} is {} wide", r.len());
                assert!(r.bytes().all(|c| b".#+owkgrb".contains(&c)), "{u:?} row {i}: unknown colour");
            }
            assert_eq!(pixels(u).len(), SIZE * SIZE * 4);
            assert_eq!(upgrade_named(u.info().name), Some(u));
        }
    }

    /// `ICON_SHEET=/path/sheet.ppm cargo test ... dump_icon_sheet -- --ignored`: every icon, 4x, eight to a row (a look check).
    #[test]
    #[ignore]
    fn dump_icon_sheet() {
        let Ok(path) = std::env::var("ICON_SHEET") else { return };
        let (k, cols) = (4usize, 8usize);
        let rows = UPGRADE_COUNT.div_ceil(cols);
        let (w, h) = (cols * (SIZE + 2) * k, rows * (SIZE + 2) * k);
        let mut out = vec![40u8; w * h * 3];
        for (n, u) in ALL_UPGRADES.iter().enumerate() {
            let p = pixels(*u);
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

    #[test]
    fn shrine_and_pot_icons_are_well_formed_and_distinct() {
        let all: Vec<Vec<u8>> = (0..6u8).map(shrine_pixels).collect();
        for k in 0..6u8 {
            let (rows, _) = shrine_glyph(k);
            assert!(rows.iter().all(|r| r.len() == GLYPH && r.bytes().all(|c| b".#+owkgrb".contains(&c))), "shrine {k}");
            assert_eq!(all[k as usize].len(), SIZE * SIZE * 4);
            for j in k as usize + 1..all.len() {
                assert_ne!(all[k as usize], all[j]);
            }
        }
    }

    #[test]
    fn every_icon_is_distinct_and_framed_by_its_category() {
        let all: Vec<Vec<u8>> = ALL_UPGRADES.iter().map(|u| pixels(*u)).collect();
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert_ne!(all[i], all[j], "{:?} and {:?} look the same", ALL_UPGRADES[i], ALL_UPGRADES[j]);
            }
        }
        // The frame pixel (row 1, middle) carries the category colour.
        let frame = |u: Upgrade| {
            let p = pixels(u);
            let i = (SIZE + SIZE / 2) * 4;
            [p[i], p[i + 1], p[i + 2]]
        };
        assert_ne!(frame(Upgrade::Sword), frame(Upgrade::Damage));
        assert_ne!(frame(Upgrade::Damage), frame(Upgrade::Dive));
        assert_ne!(frame(Upgrade::Sword), frame(Upgrade::MasterSword));
        assert_eq!(frame(Upgrade::Stars), frame(Upgrade::Sword));
        assert_eq!(category(Upgrade::Quake), Category::Technique);
        assert_eq!(category(Upgrade::Inferno), Category::Evolution);
        assert_eq!(category(Upgrade::Vitality), Category::Tome);
    }
}
