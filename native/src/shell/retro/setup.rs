//! MODE SETUP: the start options of the chosen mode (Gem Clash's arena and opponents, Star Swarm's battlefield and
//! difficulty, Hold the Gate's map, Sunstone Shindig's board...), one row each with left / right arrows, and NEXT on to
//! the character select. Shown before anything is loaded: READY on the select then loads the one course these choices
//! need and the mode starts straight away (`experiments::start_setup`). Drawn into the CPU framebuffer over the title
//! backdrop, like the grid.
use super::fb::{clean, mix, rgb, Fb, IRect, Rgba};
use super::model::{Ctx, Hit, Menu};
use super::{Hits, Insets};

const GOLD: Rgba = rgb(0xffd23a);
const WHITE: Rgba = rgb(0xf6f6ee);

/// Where everything of the setup page sits.
#[derive(Clone, Debug, PartialEq)]
pub struct SetupLayout {
    pub back: IRect,
    pub plate: IRect,
    pub rows: Vec<IRect>,
    pub next: IRect,
    pub hint: IRect,
}

pub fn layout(w: i32, h: i32, sa: Insets, n: usize) -> SetupLayout {
    let back = IRect::new(8 + sa.l, 6 + sa.t, 38, 15);
    let pw = (w - 2 * (back.x + back.w + 6)).clamp(120, 260);
    let plate = IRect::new((w - pw) / 2, back.y + if w < 300 { back.h + 4 } else { 0 }, pw, 27);
    let hint = IRect::new(8 + sa.l, h - 17 - sa.b, w - 16 - sa.l - sa.r, 13);
    let rw = (w - 16 - sa.l - sa.r).min(300);
    let x = sa.l + (w - sa.l - sa.r - rw) / 2;
    let next_h = 18;
    // Rows between the plate and NEXT, as tall as fits (17..22 px).
    let top = plate.y + plate.h + 8;
    let bottom = hint.y - next_h - 10;
    let step = ((bottom - top) / n.max(1) as i32).clamp(19, 26);
    let rows = (0..n).map(|i| IRect::new(x, top + i as i32 * step, rw, step - 3)).collect::<Vec<_>>();
    let ny = rows.last().map_or(top, |r| r.y + r.h + 8).min(bottom + 4);
    SetupLayout { back, plate, rows, next: IRect::new(x + rw / 2 - 80.min(rw / 2), ny, 160.min(rw), next_h), hint }
}

/// A small triangle pointing left (`dir` -1) or right (+1).
fn arrow(f: &mut Fb, cx: f32, cy: f32, dir: f32, c: Rgba) {
    f.poly(&[(cx + 3.0 * dir + 1.0, cy + 1.0), (cx - 3.0 * dir + 1.0, cy - 5.0 + 1.0), (cx - 3.0 * dir + 1.0, cy + 5.0 + 1.0)], rgb(0x050a22));
    f.poly(&[(cx + 3.0 * dir, cy), (cx - 3.0 * dir, cy - 5.0), (cx - 3.0 * dir, cy + 5.0)], c);
}

/// Draw the setup page and register its hit zones.
pub fn draw_setup(f: &mut Fb, menu: &Menu, ctx: &Ctx, hits: &mut Hits, t: f32, sa: Insets, hint: &str) {
    let (w, h) = (f.w, f.h);
    let lay = layout(w, h, sa, ctx.setup.len());
    // BACK.
    f.slab(lay.back, rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603));
    f.text_centered(lay.back.x + lay.back.w / 2, lay.back.y + 4, "BACK", 1, WHITE);
    hits.0.push((lay.back.inflate(3), Hit::Back));
    // Plate: the mode.
    let p = lay.plate;
    f.slab(p, rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
    f.text_centered(w / 2, p.y + 4, "MODE SETUP", 1, rgb(0x9fe0ff));
    let name = clean(&ctx.target.name);
    let ns = if Fb::text_width(&name, 2) <= p.w - 8 { 2 } else { 1 };
    f.text_centered(w / 2, p.y + 13 + (2 - ns) * 3, &name, ns, GOLD);

    // Rows: label on the left, the value between two arrows on the right.
    for (i, (r, row)) in lay.rows.iter().zip(&ctx.setup).enumerate() {
        let on = menu.opt == i;
        if !row.enabled {
            f.slab(*r, rgb(0x3a3a4a), rgb(0x22222e), rgb(0x5a5a6a), rgb(0x101018), rgb(0x06060a));
        } else if on {
            f.slab(*r, mix(rgb(0x4a7dff), WHITE, ((t * 6.0).sin() * 0.5 + 0.5) * 0.15), rgb(0x1f48d0), rgb(0xb8ccff), rgb(0x0f2a8a), rgb(0x040720));
        } else {
            f.slab(*r, rgb(0x33449a), rgb(0x1a2468), rgb(0x5b70c8), rgb(0x0a1040), rgb(0x040720));
        }
        let ty = r.y + (r.h - 7) / 2;
        let label = clean(&row.label);
        f.text(r.x + 6, ty, &label, 1, if row.enabled { rgb(0x9fe0ff) } else { rgb(0x8a8a98) });
        let lw = 62.min(r.w / 3);
        let (vx0, vx1) = (r.x + lw + 4, r.x + r.w - 4);
        let mid = (vx0 + vx1) / 2;
        let value = row.values.get(row.value).map(|v| clean(v)).unwrap_or_default();
        let max_chars = ((vx1 - vx0 - 28) / 6).max(4) as usize;
        let shown: String = if value.chars().count() > max_chars { value.chars().take(max_chars - 1).chain(std::iter::once('.')).collect() } else { value };
        let locked = row.locked.get(row.value).copied().unwrap_or(false);
        let col = if !row.enabled { rgb(0x8a8a98) } else if locked { rgb(0xffa090) } else if on { GOLD } else { WHITE };
        f.text_centered(mid, ty, &shown, 1, col);
        if row.enabled && row.values.len() > 1 {
            let ac = if on { GOLD } else { rgb(0xb8c8ff) };
            let cy = (r.y + r.h / 2) as f32;
            arrow(f, (vx0 + 6) as f32, cy, -1.0, ac);
            arrow(f, (vx1 - 6) as f32, cy, 1.0, ac);
        }
        hits.0.push((*r, Hit::Opt(i)));
        if row.enabled {
            hits.0.push((IRect::new(vx0 - 2, r.y - 1, 18, r.h + 2), Hit::OptStep(i, -1)));
            hits.0.push((IRect::new(vx1 - 16, r.y - 1, 18, r.h + 2), Hit::OptStep(i, 1)));
        }
    }
    // NEXT.
    let n = lay.next;
    let on = menu.opt >= ctx.setup.len();
    let pulse = (t * 5.0).sin() * 0.5 + 0.5;
    f.slab(n, mix(rgb(0x4ad060), rgb(0x8aff8a), if on { pulse * 0.6 } else { 0.0 }), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a));
    f.text_centered(n.x + n.w / 2, n.y + (n.h - 7) / 2, "NEXT: CHOOSE HERO", 1, rgb(0x04300c));
    if on {
        f.frame(n.inflate(2), GOLD);
    }
    hits.0.push((n.inflate(2), Hit::Ready));
    // Hint bar.
    let b = lay.hint;
    f.slab(b, rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
    let hint = if Fb::text_width(hint, 1) > b.w - 8 { "TAP AN ARROW TO CHANGE" } else { hint };
    f.text(b.x + 5, b.y + 3, hint, 1, WHITE);
}

/// The input hint of the setup page.
pub fn hint(pad: bool) -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        "TAP THE ARROWS TO CHANGE   NEXT TO GO ON"
    } else if pad {
        "D-PAD CHOOSE   A NEXT   B BACK"
    } else {
        "ARROWS CHOOSE   ENTER NEXT   ESC BACK"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::retro::model::{Page, SetupRow, Target};
    use crate::shell::Character;

    fn ctx(n: usize) -> Ctx {
        let mut c = Ctx::from_registry(Character::Mario, [true; 4], |_| true);
        c.target = Target { kind: "MODE", name: "SUNSTONE PARTY".into(), mario_only: false };
        c.setup = (0..n).map(|i| SetupRow { label: "OPPONENTS".into(), values: vec!["BOB-OMB BATTLEFIELD".into(), "2".into()], value: i % 2, locked: vec![false, true], enabled: i != 2 }).collect();
        c
    }

    fn overlap(a: IRect, b: IRect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn rows_next_and_bars_fit_without_overlap() {
        for (w, h, sa) in [(320, 240, Insets::default()), (528, 240, Insets { l: 16, r: 16, t: 0, b: 6 }), (240, 320, Insets::default()), (240, 520, Insets { t: 14, b: 10, ..Default::default() })] {
            for n in 1..=5 {
                let l = layout(w, h, sa, n);
                let mut all = vec![l.back, l.plate, l.next, l.hint];
                all.extend(l.rows.iter().copied());
                for (i, a) in all.iter().enumerate() {
                    assert!(a.x >= 0 && a.x + a.w <= w && a.y >= 0 && a.y + a.h <= h, "{w}x{h} n={n} {a:?}");
                    for b in &all[i + 1..] {
                        if !(i == 0 && *b == l.plate) {
                            assert!(!overlap(*a, *b), "{w}x{h} n={n}: {a:?} {b:?}");
                        }
                    }
                }
                assert!(l.rows.iter().all(|r| r.h >= 16), "touch rows");
            }
        }
    }

    #[test]
    fn every_row_and_arrow_has_a_hit_zone() {
        let c = ctx(5);
        let mut m = Menu { page: Page::Setup, ..Default::default() };
        m.opt = 1;
        let mut f = Fb::new(528, 240);
        let mut hits = Hits::default();
        draw_setup(&mut f, &m, &c, &mut hits, 1.0, Insets::default(), hint(false));
        for i in 0..5 {
            assert!(hits.0.iter().any(|(_, x)| *x == Hit::Opt(i)));
            assert_eq!(hits.0.iter().any(|(_, x)| *x == Hit::OptStep(i, 1)), i != 2, "disabled rows have no arrows");
        }
        for want in [Hit::Back, Hit::Ready] {
            assert!(hits.0.iter().any(|(_, x)| *x == want), "{want:?}");
        }
        // The arrows win over their row.
        let (r, _) = hits.0.iter().find(|(_, x)| *x == Hit::OptStep(0, 1)).unwrap();
        let (cx, cy) = r.center();
        assert_eq!(hits.0.iter().rev().find(|(rect, _)| rect.contains(cx as f32, cy as f32)).map(|(_, h)| *h), Some(Hit::OptStep(0, 1)));
        assert!(f.data.chunks(4).filter(|p| p[3] > 0).count() > 3000);
    }
}
