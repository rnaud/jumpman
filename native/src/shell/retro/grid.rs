//! The mode grid: the GAMES and LAB tabs as a 2D grid of tiles (the mode's 16x16 pixel icon and its name), a top bar
//! (BACK, the two tabs, the position) and an info box for the highlighted mode (its icon, name, tagline, why it is locked
//! and PLAY). Everything is drawn into the CPU framebuffer; the layout is pure and adapts to the canvas (landscape phone,
//! desktop, portrait tablet) and the safe-area insets. Navigation lives in [`super::model`].
use super::fb::{clean, mix, rgb, Fb, IRect, Rgba};
use super::icons;
use super::model::{sign_lines, sign_name, Ctx, Hit, Menu};
use super::{wrap_text, Anim, Hits, Insets};

const GOLD: Rgba = rgb(0xffd23a);
const WHITE: Rgba = rgb(0xf6f6ee);
/// Smallest tile before the grid scrolls instead of shrinking further.
const MIN_TILE_H: i32 = 44;
const MAX_TILE_W: i32 = 100;
const MAX_TILE_H: i32 = 84;
const GAP: i32 = 5;

/// Where everything of the grid page sits for a framebuffer of `w` x `h` and a tab of `n` tiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLayout {
    pub portrait: bool,
    pub back: IRect,
    pub tabs: [IRect; 2],
    /// The rectangle the tiles are fitted into.
    pub area: IRect,
    pub cols: usize,
    /// Rows that fit on screen (the grid scrolls by rows when the tab has more).
    pub rows_shown: usize,
    pub rows: usize,
    pub tile_w: i32,
    pub tile_h: i32,
    /// Top-left corner of the first shown tile.
    pub origin: (i32, i32),
    pub info: IRect,
}

pub fn layout(w: i32, h: i32, sa: Insets, n: usize) -> GridLayout {
    let portrait = w < h;
    let top = 6 + sa.t;
    let back = IRect::new(8 + sa.l, top, 38, 15);
    let tabs = [IRect::new(w / 2 - 50, top, 48, 15), IRect::new(w / 2 + 2, top, 48, 15)];
    let info_h = if portrait { 84 } else { 46 };
    let info = IRect::new(8 + sa.l, h - info_h - 6 - sa.b, w - 16 - sa.l - sa.r, info_h);
    // Room for the input hint over the info box on landscape canvases.
    let area = IRect::new(8 + sa.l, top + 15 + 7, w - 16 - sa.l - sa.r, (info.y - if portrait { 6 } else { 12 }) - (top + 15 + 7));
    let n = n.max(1);
    // Pick the column count that gives the biggest tiles (a little wider than tall reads best); scrolling is a last resort.
    let mut best: Option<(i32, usize, usize, i32, i32)> = None;
    for cols in 1..=n.min(6) {
        let rows = n.div_ceil(cols);
        let tw = (area.w - GAP * (cols as i32 - 1)) / cols as i32;
        let th_all = (area.h - GAP * (rows as i32 - 1)) / rows as i32;
        let (shown, th) = if th_all >= MIN_TILE_H { (rows, th_all) } else { let s = ((area.h + GAP) / (MIN_TILE_H + GAP)).max(1) as usize; (s.min(rows), MIN_TILE_H) };
        let mut score = tw.min(MAX_TILE_W).min((th as f32 * 1.25) as i32);
        if shown < rows {
            score -= 1000;
        }
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, cols, shown, tw, th));
        }
    }
    let (_, cols, rows_shown, tw, th) = best.unwrap_or((0, 1, 1, area.w, MIN_TILE_H));
    let tile_w = tw.min(MAX_TILE_W).max(24);
    let tile_h = th.min(MAX_TILE_H).min((tile_w as f32 * 0.95) as i32).max(MIN_TILE_H.min(th));
    let total_w = cols as i32 * tile_w + GAP * (cols as i32 - 1);
    let total_h = rows_shown as i32 * tile_h + GAP * (rows_shown as i32 - 1);
    let origin = (area.x + (area.w - total_w) / 2, area.y + ((area.h - total_h) / 2).max(0));
    GridLayout { portrait, back, tabs, area, cols, rows_shown, rows: n.div_ceil(cols), tile_w, tile_h, origin, info }
}

impl GridLayout {
    /// The first row on screen when tile `sel` is highlighted (the grid scrolls just enough to show it).
    pub fn first_row(&self, sel: usize) -> usize {
        let r = sel / self.cols.max(1);
        (r + 1).saturating_sub(self.rows_shown).min(self.rows.saturating_sub(self.rows_shown))
    }

    /// Screen rectangle of tile `i`, `None` when it is scrolled off.
    pub fn tile_rect(&self, i: usize, first_row: usize) -> Option<IRect> {
        let (r, c) = (i / self.cols.max(1), i % self.cols.max(1));
        if r < first_row || r >= first_row + self.rows_shown {
            return None;
        }
        let (rr, c) = ((r - first_row) as i32, c as i32);
        Some(IRect::new(self.origin.0 + c * (self.tile_w + GAP), self.origin.1 + rr * (self.tile_h + GAP), self.tile_w, self.tile_h))
    }
}

fn dialog(f: &mut Fb, r: IRect) {
    f.slab(r, rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
}

fn tab(f: &mut Fb, r: IRect, label: &str, sel: bool) {
    if sel {
        f.slab(r, rgb(0xffd23a), rgb(0xe08a18), rgb(0xfff1a8), rgb(0x8a4a08), rgb(0x3a1a04));
        f.text_centered(r.x + r.w / 2, r.y + 4, label, 1, rgb(0x3a1a04));
    } else {
        f.slab(r, rgb(0x33449a), rgb(0x1a2468), rgb(0x5b70c8), rgb(0x0a1040), rgb(0x040720));
        f.text_centered(r.x + r.w / 2, r.y + 4, label, 1, rgb(0xdfe6ff));
    }
}

fn padlock(f: &mut Fb, x: i32, y: i32) {
    f.rect(IRect::new(x, y + 4, 9, 7), rgb(0xb0b0c0));
    f.frame(IRect::new(x, y + 4, 9, 7), rgb(0x20202c));
    f.rect(IRect::new(x + 2, y, 1, 5), rgb(0xb0b0c0));
    f.rect(IRect::new(x + 6, y, 1, 5), rgb(0xb0b0c0));
    f.rect(IRect::new(x + 2, y, 5, 1), rgb(0xb0b0c0));
    f.rect(IRect::new(x + 4, y + 6, 1, 3), rgb(0x20202c));
}

/// Draw the grid page and register its hit zones. `hint` is the input hint line (empty to leave it out).
#[allow(clippy::too_many_arguments)]
pub fn draw_grid(f: &mut Fb, menu: &Menu, ctx: &Ctx, hits: &mut Hits, anim: &Anim, t: f32, sa: Insets, hint: &str, mario_only: impl Fn(crate::shell::ExperimentId) -> bool) {
    let (w, h) = (f.w, f.h);
    let tiles = menu.tiles(ctx);
    let n = tiles.len();
    let lay = layout(w, h, sa, n);
    let sel = menu.cur_tile().min(n.saturating_sub(1));
    let first = lay.first_row(sel);
    let pressed = menu.pending.is_some();

    // Tiles.
    for (i, tile) in tiles.iter().enumerate() {
        let Some(r) = lay.tile_rect(i, first) else { continue };
        let on = i == sel;
        let accent = icons::accent(tile.id);
        let (a, b) = if !tile.enabled {
            (mix(accent, rgb(0x303040), 0.7), mix(accent, rgb(0x0a0a12), 0.85))
        } else if on {
            (mix(accent, WHITE, 0.3), mix(accent, rgb(0), 0.15))
        } else {
            (mix(accent, rgb(0), 0.15), mix(accent, rgb(0), 0.6))
        };
        f.rect(IRect::new(r.x + 2, r.y + 2, r.w, r.h), rgb(0x03040e));
        if on {
            // A blinking gold frame (white-hot while the tile is pressed).
            let glow = if pressed { mix(GOLD, WHITE, menu.enter_progress()) } else { mix(GOLD, WHITE, ((t * 6.0).sin() * 0.5 + 0.5) * 0.5) };
            f.rect(r.inflate(3), rgb(0x3a1a04));
            f.rect(r.inflate(2), glow);
        }
        f.vgrad(r, a, b);
        f.frame(r, rgb(0x0a0a14));
        f.hline(r.x + 1, r.y + 1, r.w - 2, mix(a, WHITE, 0.4));
        // Icon (2x when the tile is tall enough), then the name under it.
        let name = clean(&sign_name(tile.id));
        let lines = sign_lines(&name, ((r.w - 4) / 6).max(4) as usize);
        let text_h = lines.len() as i32 * 9 - 2;
        // One icon size for the whole grid: 2x when a two-line name still fits under it.
        let scale = if lay.tile_h >= icons::BADGE * 2 + 2 + 16 + 2 { 2 } else { 1 };
        let icon = icons::BADGE * scale;
        let block = icon + 4 + text_h;
        let y0 = r.y + (r.h - block) / 2;
        f.blit(&icons::badge(tile.id), r.x + (r.w - icon) / 2, y0, scale);
        let col = if tile.enabled { WHITE } else { rgb(0xa0a0ac) };
        for (k, l) in lines.iter().enumerate() {
            f.text_centered(r.x + r.w / 2, y0 + icon + 4 + k as i32 * 9, l, 1, col);
        }
        if !tile.enabled {
            padlock(f, r.x + r.w - 12, r.y + 3);
        }
        hits.0.push((r, Hit::Tile(i)));
    }
    // Scroll arrows when the tab has more rows than fit.
    if first > 0 || first + lay.rows_shown < lay.rows {
        let x = (lay.area.x + lay.area.w - 4) as f32;
        let pulse = (t * 5.0).sin() * 1.0;
        if first > 0 {
            let y = lay.origin.1 as f32 - 4.0 - pulse;
            f.poly(&[(x - 4.0, y + 3.0), (x + 4.0, y + 3.0), (x, y - 2.0)], GOLD);
        }
        if first + lay.rows_shown < lay.rows {
            let y = (lay.origin.1 + lay.rows_shown as i32 * (lay.tile_h + GAP)) as f32 + pulse;
            f.poly(&[(x - 4.0, y - 3.0), (x + 4.0, y - 3.0), (x, y + 2.0)], GOLD);
        }
    }

    // Top bar: BACK, the tabs, the position.
    f.slab(lay.back, rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603));
    f.text_centered(lay.back.x + lay.back.w / 2, lay.back.y + 4, "BACK", 1, WHITE);
    hits.0.push((lay.back.inflate(3), Hit::Back));
    for (k, wing) in [super::model::Wing::Games, super::model::Wing::Lab].into_iter().enumerate() {
        tab(f, lay.tabs[k], wing.label(), menu.wing == wing);
        hits.0.push((lay.tabs[k].inflate(3), Hit::Wing(wing)));
    }
    let count = format!("{}/{}", sel + 1, n);
    let cx = (w - 10 - sa.r - Fb::text_width(&count, 1)).max(lay.tabs[1].x + lay.tabs[1].w + 4);
    f.text(cx, lay.back.y + 4, &count, 1, WHITE);

    // Info box: the highlighted mode.
    let b = lay.info;
    dialog(f, b);
    if let Some(d) = tiles.get(sel) {
        f.rect(IRect::new(b.x + 5, b.y + 6, 52, 34), rgb(0x040720));
        f.vgrad(IRect::new(b.x + 6, b.y + 7, 50, 32), mix(icons::accent(d.id), rgb(0), 0.35), mix(icons::accent(d.id), rgb(0), 0.75));
        f.blit(&icons::badge(d.id), b.x + 15, b.y + 7, 2);
        let tx = b.x + 64;
        // PLAY: right of the text on landscape canvases, along the bottom on portrait ones.
        let play = if lay.portrait { IRect::new(b.x + 6, b.y + b.h - 22, b.w - 12, 17) } else { IRect::new(b.x + b.w - 52, b.y + 8, 46, 16) };
        let text_w = if lay.portrait { b.x + b.w - 6 - tx } else { play.x - 6 - tx };
        let name = clean(&sign_name(d.id));
        let ns = if Fb::text_width(&name, 2) <= text_w { 2 } else { 1 };
        f.text(tx, b.y + 6, &name, ns, GOLD);
        let max_chars = (text_w / 6).max(8) as usize;
        let typed: usize = ((t - anim.since).max(0.0) * 60.0) as usize;
        let tag: String = clean(d.id.tagline()).chars().take(typed).collect();
        let max_lines = if lay.portrait { 4 } else { 2 };
        let ty = b.y + 7 + 7 * ns + 4;
        let mut lines = wrap_text(&tag, max_chars);
        lines.truncate(max_lines);
        for (k, l) in lines.iter().enumerate() {
            f.text(tx, ty + k as i32 * 9, l, 1, WHITE);
        }
        if d.enabled {
            let pulse = (t * 5.0).sin() * 0.5 + 0.5;
            f.slab(play, mix(rgb(0x4ad060), rgb(0x8aff8a), pulse * 0.5), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a));
            f.text_centered(play.x + play.w / 2, play.y + (play.h - 7) / 2, "PLAY", 1, rgb(0x04300c));
        } else {
            f.slab(play, rgb(0x6a6a78), rgb(0x3a3a46), rgb(0x9a9aa8), rgb(0x20202a), rgb(0x0a0a10));
            f.text_centered(play.x + play.w / 2, play.y + (play.h - 7) / 2, "LOCKED", 1, rgb(0xc0c0cc));
        }
        hits.0.push((play.inflate(2), Hit::Enter));
        // A note under PLAY (landscape) or over it (portrait): why it is locked, or that it takes Mario only.
        let note = if let Some(why) = d.lock.filter(|_| !d.enabled) { Some((why, rgb(0xffa090))) } else if mario_only(d.id) { Some(("MARIO ONLY", rgb(0x9fe0ff))) } else { None };
        if let Some((s, c)) = note {
            if lay.portrait {
                f.text(tx.min(b.x + 6), play.y - 10, s, 1, c);
            } else {
                let nx = (b.x + b.w - 6 - Fb::text_width(s, 1)).max(tx);
                f.text(nx, play.y + play.h + 6, s, 1, c);
            }
        }
    }
    if !hint.is_empty() && !lay.portrait && Fb::text_width(hint, 1) < w - 16 - sa.l - sa.r {
        f.text(w - 8 - sa.r - Fb::text_width(hint, 1), b.y - 9, hint, 1, rgb(0xe8e8f0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::retro::model::{Page, Wing};
    use crate::shell::Character;

    fn overlap(a: IRect, b: IRect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    /// Desktop 4:3 and wide windows, a landscape phone with a notch, a portrait tablet.
    const SIZES: [(i32, i32, Insets); 5] = [
        (320, 240, Insets { l: 0, r: 0, t: 0, b: 0 }),
        (380, 240, Insets { l: 0, r: 0, t: 0, b: 0 }),
        (528, 240, Insets { l: 16, r: 16, t: 0, b: 6 }),
        (240, 320, Insets { l: 0, r: 0, t: 6, b: 6 }),
        (240, 520, Insets { l: 0, r: 0, t: 14, b: 10 }),
    ];

    #[test]
    fn tiles_fit_inside_the_safe_area_and_never_overlap() {
        for (w, h, sa) in SIZES {
            for n in [4usize, 10, 13] {
                let l = layout(w, h, sa, n);
                let first = 0;
                let rects: Vec<IRect> = (0..n).filter_map(|i| l.tile_rect(i, first)).collect();
                assert!(!rects.is_empty());
                for (i, r) in rects.iter().enumerate() {
                    assert!(r.x >= sa.l && r.x + r.w <= w - sa.r && r.y >= sa.t + 20 && r.y + r.h <= l.info.y, "{w}x{h} n={n} tile {i} {r:?} info {:?}", l.info);
                    assert!(r.w >= 40 && r.h >= MIN_TILE_H.min(r.h), "{w}x{h} tile too small {r:?}");
                    for o in &rects[i + 1..] {
                        assert!(!overlap(*r, *o), "{w}x{h} n={n}: {r:?} {o:?}");
                    }
                    assert!(!overlap(*r, l.back) && !overlap(*r, l.tabs[0]) && !overlap(*r, l.tabs[1]));
                }
                assert!(l.info.y + l.info.h <= h - sa.b && l.info.x >= sa.l && l.info.x + l.info.w <= w - sa.r);
            }
        }
    }

    #[test]
    fn the_games_tab_shows_every_tile_without_scrolling_on_common_screens() {
        for (w, h, sa) in SIZES {
            let l = layout(w, h, sa, 10);
            assert_eq!(l.rows_shown, l.rows, "{w}x{h}: {l:?}");
            assert_eq!(l.first_row(9), 0);
        }
        // Landscape spreads out, portrait stacks up.
        assert!(layout(528, 240, Insets::default(), 10).cols >= 4);
        assert!(layout(240, 520, Insets::default(), 10).cols <= 3);
    }

    #[test]
    fn scrolls_to_keep_the_highlight_on_screen() {
        // A tiny canvas with many tiles must scroll.
        let l = layout(240, 240, Insets::default(), 24);
        assert!(l.rows_shown < l.rows, "{l:?}");
        assert_eq!(l.first_row(0), 0);
        let last = 23;
        let fr = l.first_row(last);
        assert!(l.tile_rect(last, fr).is_some(), "the last tile is on screen once highlighted");
        assert!(l.tile_rect(0, fr).is_none());
    }

    #[test]
    fn touch_targets_are_big_enough_on_a_phone() {
        // A landscape phone (874x402 pt) shows a 528x240 canvas: 1 px = 1.66 pt. Apple asks for 44 pt targets.
        let l = layout(528, 240, Insets { l: 16, r: 16, t: 0, b: 6 }, 10);
        let k = 874.0 / 528.0;
        assert!(l.tile_w as f32 * k >= 44.0 && l.tile_h as f32 * k >= 44.0, "{l:?}");
    }

    fn draw(w: i32, h: i32, wing: Wing, tile: usize, c: &Ctx) -> (Fb, Hits) {
        let mut m = Menu { page: Page::Grid, wing, ..Default::default() };
        m.tile[wing.idx()] = tile;
        let mut f = Fb::new(w, h);
        let mut hits = Hits::default();
        draw_grid(&mut f, &m, c, &mut hits, &Anim::default(), 1.0, Insets::default(), "ARROWS MOVE", |id| id == crate::shell::ExperimentId::Halls);
        (f, hits)
    }

    #[test]
    fn every_control_and_tile_has_a_hit_zone_in_both_tabs() {
        let c = Ctx::from_registry(Character::Mario, [true; 4], |_| true);
        for (w, h, _) in SIZES {
            for wing in [Wing::Games, Wing::Lab] {
                let (f, hits) = draw(w, h, wing, 0, &c);
                for want in [Hit::Back, Hit::Enter, Hit::Wing(Wing::Games), Hit::Wing(Wing::Lab)] {
                    assert!(hits.0.iter().any(|(_, x)| *x == want), "{w}x{h} missing {want:?}");
                }
                for i in 0..c.tiles[wing.idx()].len() {
                    assert!(hits.0.iter().any(|(_, x)| *x == Hit::Tile(i)), "{w}x{h} {wing:?} tile {i}");
                }
                assert!(f.data.chunks(4).filter(|p| p[3] > 0).count() > 4000);
            }
        }
    }

    #[test]
    fn a_tap_in_a_tile_centre_hits_that_tile() {
        let c = Ctx::from_registry(Character::Mario, [true; 4], |_| true);
        let (_, hits) = draw(528, 240, Wing::Games, 2, &c);
        for i in 0..c.tiles[0].len() {
            let (r, _) = hits.0.iter().find(|(_, x)| *x == Hit::Tile(i)).unwrap();
            let (cx, cy) = r.center();
            let top = hits.0.iter().rev().find(|(rect, _)| rect.contains(cx as f32, cy as f32)).map(|(_, h)| *h);
            assert_eq!(top, Some(Hit::Tile(i)));
        }
    }

    #[test]
    fn locked_tiles_draw_too() {
        let mut c = Ctx::from_registry(Character::Mario, [true; 4], |_| true);
        let kart = c.tiles[0].iter().position(|t| t.id == crate::shell::ExperimentId::Kart).unwrap();
        c.tiles[0][kart].enabled = false;
        c.tiles[0][kart].lock = Some("IMPORT MARIO KART 64");
        let (_, hits) = draw(528, 240, Wing::Games, kart, &c);
        assert!(hits.0.iter().any(|(_, x)| *x == Hit::Tile(kart)));
        let (_, hits) = draw(240, 520, Wing::Games, kart, &c);
        assert!(hits.0.iter().any(|(_, x)| *x == Hit::Enter));
    }
}
