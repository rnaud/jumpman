//! Original per-mode vignettes, drawn procedurally into 48x30 pixel buffers: they stand in for the
//! mode-card thumbnails (shown in the hub's dialog box and on the classic cards).
use super::fb::*;
use crate::shell::ExperimentId;

pub const ICON_W: i32 = 48;
pub const ICON_H: i32 = 30;

/// Sign / accent colour of a mode: the colour of its pixel icon (`experiments::mode_icons`).
pub fn accent(id: ExperimentId) -> Rgba {
    rgb(id.theirs().map_or(0x6a6a78, crate::experiments::mode_icons::color))
}

/// Side of a mode's pixel icon.
pub const BADGE: i32 = crate::experiments::mode_icons::SIZE as i32;

/// The mode's own 16x16 pixel icon (on the hub's door signs and in its dialog box).
pub fn badge(id: ExperimentId) -> Fb {
    let mut f = Fb::new(BADGE, BADGE);
    if let Some(x) = id.theirs() {
        f.data = crate::experiments::mode_icons::pixels(x);
    }
    f
}

fn shade(c: Rgba, f: f32) -> Rgba {
    [(c[0] as f32 * f).min(255.0) as u8, (c[1] as f32 * f).min(255.0) as u8, (c[2] as f32 * f).min(255.0) as u8, c[3]]
}

pub fn icon(id: ExperimentId) -> Fb {
    use ExperimentId::*;
    let mut f = Fb::new(ICON_W, ICON_H);
    let a = accent(id);
    f.vgrad(IRect::new(0, 0, ICON_W, ICON_H), shade(a, 1.25), shade(a, 0.55));
    let white = rgb(0xf4f0e0);
    let dark = rgb(0x10122a);
    let gold = rgb(0xffc933);
    match id {
        Brawl => {
            // Two clashing fists around a faceted gem.
            f.rect(IRect::new(3, 12, 12, 9), rgb(0xe8b88a));
            f.rect(IRect::new(33, 12, 12, 9), rgb(0xe8b88a));
            f.rect(IRect::new(2, 12, 3, 9), dark);
            f.rect(IRect::new(43, 12, 3, 9), dark);
            f.poly(&[(24.0, 4.0), (30.0, 14.0), (24.0, 26.0), (18.0, 14.0)], rgb(0xff9a1e));
            f.poly(&[(24.0, 4.0), (30.0, 14.0), (24.0, 14.0)], rgb(0xffd36a));
            f.poly(&[(24.0, 14.0), (30.0, 14.0), (24.0, 26.0)], rgb(0xc2560a));
            for (x, y) in [(16.0, 5.0), (32.0, 6.0), (24.0, 1.0)] {
                f.line((x - 2.0, y), (x + 2.0, y), white);
                f.line((x, y - 2.0), (x, y + 2.0), white);
            }
        }
        Relic => {
            // Stairs going down toward a glowing relic.
            for i in 0..6 {
                f.rect(IRect::new(i * 6, 8 + i * 3, 48 - i * 6, 22), shade(rgb(0x9a7a50), 1.2 - i as f32 * 0.14));
            }
            f.poly(&[(36.0, 18.0), (40.0, 22.0), (36.0, 28.0), (32.0, 22.0)], gold);
            f.disc(36.0, 22.0, 6.0, with_alpha(gold, 60));
        }
        LastStar => {
            // The closing ring with survivors dotted inside it.
            f.ring(24.0, 15.0, 14.0, 2.0, rgb(0xff5a4a));
            f.ring(24.0, 15.0, 8.0, 1.0, with_alpha(white, 140));
            for (x, y) in [(20.0, 12.0), (28.0, 17.0), (23.0, 20.0), (27.0, 10.0)] {
                f.rect(IRect::new(x as i32, y as i32, 2, 2), white);
            }
            f.rect(IRect::new(23, 14, 3, 3), gold);
        }
        Kart => {
            f.poly(&[(0.0, 30.0), (14.0, 12.0), (34.0, 12.0), (48.0, 30.0)], rgb(0x4a4a58));
            for i in 0..4 {
                f.rect(IRect::new(23, 13 + i * 5, 2, 3), white);
            }
            f.rect(IRect::new(15, 17, 18, 6), rgb(0xe83a2a));
            f.rect(IRect::new(19, 14, 10, 4), rgb(0xf0e6d0));
            f.rect(IRect::new(13, 21, 5, 5), dark);
            f.rect(IRect::new(30, 21, 5, 5), dark);
        }
        Swarm => {
            let mut s = 7u32;
            for _ in 0..34 {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let (x, y) = ((s >> 8) % 46, (s >> 18) % 28);
                f.rect(IRect::new(x as i32, y as i32, 2, 2), rgb(0xd6a8ff));
            }
            f.disc(24.0, 15.0, 5.0, rgb(0x2a1a4a));
            f.disc(24.0, 15.0, 3.0, gold);
        }
        Deck => {
            for (i, (dx, col)) in [(-9.0, 0xe8e0d0), (0.0, 0xf4eee0), (9.0, 0xffffff)].iter().enumerate() {
                let x = 24.0 + dx;
                let y = 15.0 + (i as f32 - 1.0).abs() * 2.0;
                f.poly(&[(x - 7.0, y - 10.0 + (i as f32) * 0.0), (x + 7.0 + dx * 0.2, y - 10.0 + dx * 0.15), (x + 7.0 + dx * 0.2, y + 10.0 + dx * 0.15), (x - 7.0, y + 10.0)], rgb(*col));
                f.disc(x + dx * 0.1, y, 3.0, rgb(0x1f8f8a));
            }
        }
        Halls => {
            f.rect(IRect::new(10, 6, 28, 24), rgb(0x5a3a20));
            f.poly(&[(14.0, 30.0), (14.0, 14.0), (18.0, 9.0), (30.0, 9.0), (34.0, 14.0), (34.0, 30.0)], dark);
            f.rect(IRect::new(22, 20, 4, 4), gold);
            f.rect(IRect::new(23, 24, 2, 4), gold);
        }
        Gate => {
            f.rect(IRect::new(0, 20, 48, 10), rgb(0x7a8698));
            for i in 0..6 {
                f.rect(IRect::new(i * 8 + 1, 16, 5, 5), rgb(0x7a8698));
            }
            f.rect(IRect::new(18, 8, 12, 22), rgb(0x9aa6b8));
            f.rect(IRect::new(21, 18, 6, 12), dark);
            f.poly(&[(18.0, 8.0), (24.0, 1.0), (30.0, 8.0)], rgb(0xc04a3a));
        }
        Party => {
            for (i, (x, y)) in [(6.0, 22.0), (15.0, 15.0), (25.0, 20.0), (35.0, 12.0), (43.0, 20.0)].iter().enumerate() {
                f.disc(*x, *y, 3.5, [rgb(0xffd23a), rgb(0x5ad0ff), rgb(0xff6a8a), rgb(0x8aff7a), rgb(0xffd23a)][i]);
            }
            f.rect(IRect::new(18, 3, 11, 11), white);
            for (x, y) in [(20, 5), (26, 5), (23, 8), (20, 11), (26, 11)] {
                f.rect(IRect::new(x, y, 2, 2), dark);
            }
        }
        Race => {
            f.poly(&[(0.0, 30.0), (20.0, 6.0), (34.0, 20.0), (40.0, 14.0), (48.0, 30.0)], rgb(0x6a7a5a));
            f.poly(&[(14.0, 13.0), (20.0, 6.0), (26.0, 13.0), (20.0, 11.0)], white);
            f.vline(20, 0, 7, dark);
            f.poly(&[(21.0, 0.0), (27.0, 2.0), (21.0, 4.0)], rgb(0xe83a2a));
        }
        Runner => {
            for i in 0..4 {
                f.rect(IRect::new(2, 6 + i * 6, 14 + (i % 2) * 6, 1), with_alpha(white, 200));
            }
            f.rect(IRect::new(26, 8, 6, 6), rgb(0xe8b88a));
            f.rect(IRect::new(25, 14, 8, 9), rgb(0x2a7a8a));
            f.line((27.0, 23.0), (22.0, 28.0), rgb(0x2a7a8a));
            f.line((31.0, 23.0), (36.0, 26.0), rgb(0x2a7a8a));
            f.rect(IRect::new(40, 20, 3, 10), white);
        }
        God => {
            f.rect(IRect::new(4, 20, 14, 4), rgb(0xb0a8d8));
            f.rect(IRect::new(30, 12, 14, 4), rgb(0xb0a8d8));
            f.poly(&[(18.0, 22.0), (24.0, 14.0), (28.0, 28.0)], rgb(0x3a2f60));
            f.line((20.0, 0.0), (23.0, 10.0), gold);
            f.line((23.0, 10.0), (21.0, 12.0), gold);
        }
        Diorama => {
            f.poly(&[(24.0, 6.0), (42.0, 14.0), (24.0, 22.0), (6.0, 14.0)], rgb(0xd8b878));
            f.poly(&[(6.0, 14.0), (24.0, 22.0), (24.0, 30.0), (6.0, 22.0)], rgb(0x9a7a48));
            f.poly(&[(42.0, 14.0), (24.0, 22.0), (24.0, 30.0), (42.0, 22.0)], rgb(0x7a5c34));
            f.rect(IRect::new(22, 8, 4, 7), rgb(0xe8442a));
        }
        Dimensions => {
            f.rect(IRect::new(5, 6, 16, 16), rgb(0x9ad8f0));
            f.frame(IRect::new(5, 6, 16, 16), dark);
            f.poly(&[(30.0, 10.0), (40.0, 6.0), (44.0, 10.0), (44.0, 22.0), (34.0, 26.0), (30.0, 22.0)], rgb(0x4aa0c8));
            f.line((22.0, 14.0), (28.0, 14.0), white);
            f.line((26.0, 12.0), (28.0, 14.0), white);
            f.line((26.0, 16.0), (28.0, 14.0), white);
        }
        Shrink => {
            f.rect(IRect::new(4, 4, 22, 24), rgb(0x8ac84a));
            f.frame(IRect::new(4, 4, 22, 24), dark);
            f.rect(IRect::new(34, 20, 6, 8), rgb(0x8ac84a));
            f.frame(IRect::new(34, 20, 6, 8), dark);
            f.line((28.0, 8.0), (38.0, 17.0), white);
            f.line((38.0, 17.0), (38.0, 13.0), white);
            f.line((38.0, 17.0), (34.0, 17.0), white);
        }
        Planet => {
            f.disc(24.0, 36.0, 24.0, rgb(0x3aa07a));
            f.ring(24.0, 36.0, 24.0, 2.0, rgb(0x7ad8a8));
            f.rect(IRect::new(22, 5, 4, 7), rgb(0xe8442a));
            f.rect(IRect::new(22, 2, 4, 3), rgb(0xe8b88a));
            f.disc(8.0, 6.0, 1.5, white);
            f.disc(40.0, 9.0, 1.0, white);
        }
        Hundred => {
            for y in 0..4 {
                for x in 0..8 {
                    let c = [rgb(0xe8442a), rgb(0x2a7a8a), rgb(0xffc933), rgb(0x8aff7a)][(x + y * 3) as usize % 4];
                    f.rect(IRect::new(3 + x * 5, 4 + y * 6, 3, 3), c);
                    f.rect(IRect::new(3 + x * 5, 7 + y * 6, 3, 2), shade(c, 0.6));
                }
            }
        }
    }
    // Thin dark frame so every vignette reads as a picture.
    f.frame(IRect::new(0, 0, ICON_W, ICON_H), with_alpha(dark, 200));
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_a_distinct_filled_icon() {
        let mut seen: Vec<Vec<u8>> = Vec::new();
        for id in ExperimentId::ALL {
            let f = icon(id);
            assert_eq!((f.w, f.h), (ICON_W, ICON_H));
            assert!(f.data.chunks(4).all(|p| p[3] == 255), "{id:?} icon must be opaque");
            assert!(!seen.contains(&f.data), "{id:?} duplicates another icon");
            seen.push(f.data);
        }
    }
}
