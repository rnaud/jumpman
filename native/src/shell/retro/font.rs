//! The v3 bitmap font as a real TrueType face, so every `bevy_ui` `Text` can use the same chunky 5x7 capitals as the title's
//! framebuffer (`fb::GLYPHS`). The font file is generated at start-up from the glyph table: each lit pixel is a square
//! contour (merged along each row), so there is no font asset to ship and nothing ROM-derived.
//!
//! Metrics: 1000 units per em, one font pixel is [`PIXEL`] units, glyphs are 5 pixels wide on a 6 pixel advance and
//! 7 pixels tall (cap height 700). Lower case maps to the capitals, exactly like [`super::fb::clean`].
use super::fb::glyph;
use bevy::prelude::*;

/// Units per font pixel (a 5x7 glyph is 500 x 700 units on a 1000 unit em: the width of a rounded UI face).
pub const PIXEL: i16 = 100;
const ADVANCE: u16 = 6 * PIXEL as u16;
const ASCENT: i16 = 900;
const DESCENT: i16 = -250;

/// Other code points the game's text uses, drawn as the ASCII character they stand for.
const ALIASES: &[(u16, char)] = &[(0xa0, ' '), (0xb7, '-'), (0xd7, 'x'), (0x2013, '-'), (0x2014, '-'), (0x2018, '\''), (0x2019, '\''), (0x201c, '"'), (0x201d, '"'), (0x2022, '-'), (0x2190, '<'), (0x2192, '>')];

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}
fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}
fn puti16(v: &mut Vec<u8>, x: i16) {
    v.extend_from_slice(&x.to_be_bytes());
}

/// One glyph's `glyf` entry (empty for a blank glyph).
fn outline(bits: [u8; 7]) -> (Vec<u8>, usize, usize) {
    let mut rects: Vec<(i16, i16, i16, i16)> = Vec::new(); // x0, y0, x1, y1 (y up, baseline 0)
    for (row, b) in bits.iter().enumerate() {
        let (y1, y0) = ((7 - row as i16) * PIXEL, (6 - row as i16) * PIXEL);
        let mut col = 0;
        while col < 5 {
            if b & (0b10000 >> col) != 0 {
                let start = col;
                while col < 5 && b & (0b10000 >> col) != 0 {
                    col += 1;
                }
                rects.push((start as i16 * PIXEL, y0, col as i16 * PIXEL, y1));
            } else {
                col += 1;
            }
        }
    }
    if rects.is_empty() {
        return (Vec::new(), 0, 0);
    }
    let mut g = Vec::new();
    puti16(&mut g, rects.len() as i16);
    puti16(&mut g, 0);
    puti16(&mut g, 0);
    puti16(&mut g, 5 * PIXEL);
    puti16(&mut g, 7 * PIXEL);
    for i in 0..rects.len() {
        put16(&mut g, (i * 4 + 3) as u16);
    }
    put16(&mut g, 0); // no instructions
    let pts: Vec<(i16, i16)> = rects.iter().flat_map(|&(x0, y0, x1, y1)| [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]).collect();
    g.extend(std::iter::repeat(1u8).take(pts.len())); // on-curve, 16 bit deltas
    let mut prev = 0;
    for &(x, _) in &pts {
        puti16(&mut g, x - prev);
        prev = x;
    }
    prev = 0;
    for &(_, y) in &pts {
        puti16(&mut g, y - prev);
        prev = y;
    }
    while g.len() % 4 != 0 {
        g.push(0);
    }
    (g, pts.len(), rects.len())
}

fn table_sum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    for chunk in data.chunks(4) {
        let mut w = [0u8; 4];
        w[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(w));
    }
    sum
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
}

/// The font file's bytes.
pub fn pixel_font_bytes() -> Vec<u8> {
    // Glyph 0 is .notdef (a question mark); glyph 1.. are U+0020..U+007E.
    let mut shapes: Vec<[u8; 7]> = vec![glyph('?')];
    shapes.extend((0x20u8..=0x7e).map(|c| glyph(c as char)));
    let n = shapes.len();
    let (mut glyf, mut loca, mut max_pts, mut max_cont) = (Vec::new(), Vec::new(), 0, 0);
    for s in &shapes {
        put32(&mut loca, glyf.len() as u32);
        let (g, pts, conts) = outline(*s);
        max_pts = max_pts.max(pts);
        max_cont = max_cont.max(conts);
        glyf.extend(g);
    }
    put32(&mut loca, glyf.len() as u32);

    let mut head = Vec::new();
    put32(&mut head, 0x0001_0000);
    put32(&mut head, 0x0001_0000);
    put32(&mut head, 0); // checkSumAdjustment, patched below
    put32(&mut head, 0x5f0f_3cf5);
    put16(&mut head, 3);
    put16(&mut head, 1000);
    head.extend([0u8; 16]); // created, modified
    puti16(&mut head, 0);
    puti16(&mut head, 0);
    puti16(&mut head, 5 * PIXEL);
    puti16(&mut head, 7 * PIXEL);
    put16(&mut head, 0);
    put16(&mut head, 7);
    puti16(&mut head, 2);
    puti16(&mut head, 1); // long loca
    puti16(&mut head, 0);

    let mut hhea = Vec::new();
    put32(&mut hhea, 0x0001_0000);
    puti16(&mut hhea, ASCENT);
    puti16(&mut hhea, DESCENT);
    puti16(&mut hhea, 0);
    put16(&mut hhea, ADVANCE);
    puti16(&mut hhea, 0);
    puti16(&mut hhea, PIXEL);
    puti16(&mut hhea, 5 * PIXEL);
    puti16(&mut hhea, 1);
    puti16(&mut hhea, 0);
    puti16(&mut hhea, 0);
    hhea.extend([0u8; 8]);
    puti16(&mut hhea, 0);
    put16(&mut hhea, n as u16);

    let mut maxp = Vec::new();
    put32(&mut maxp, 0x0001_0000);
    put16(&mut maxp, n as u16);
    put16(&mut maxp, max_pts as u16);
    put16(&mut maxp, max_cont as u16);
    put16(&mut maxp, 0);
    put16(&mut maxp, 0);
    put16(&mut maxp, 1);
    maxp.extend([0u8; 16]);

    let mut hmtx = Vec::new();
    for _ in 0..n {
        put16(&mut hmtx, ADVANCE);
        puti16(&mut hmtx, 0);
    }

    // cmap format 4: U+0020..U+007E map straight onto glyphs 1..95 (delta), the aliases onto their ASCII glyph.
    let gid = |c: char| (c as u16) - 0x1f;
    let mut segs: Vec<(u16, u16, u16)> = vec![(0x20, 0x7e, 1u16.wrapping_sub(0x20))];
    for &(cp, to) in ALIASES {
        segs.push((cp, cp, gid(to).wrapping_sub(cp)));
    }
    segs.push((0xffff, 0xffff, 1));
    let sc = segs.len() as u16;
    let mut sub = Vec::new();
    put16(&mut sub, 4);
    put16(&mut sub, (16 + sc * 8) as u16);
    put16(&mut sub, 0);
    put16(&mut sub, sc * 2);
    let mut sel = 0u16;
    while (1u32 << (sel + 1)) <= sc as u32 {
        sel += 1;
    }
    let search = 2u16 << sel;
    put16(&mut sub, search);
    put16(&mut sub, sel);
    put16(&mut sub, sc * 2 - search);
    for s in &segs {
        put16(&mut sub, s.1);
    }
    put16(&mut sub, 0);
    for s in &segs {
        put16(&mut sub, s.0);
    }
    for s in &segs {
        put16(&mut sub, s.2);
    }
    for _ in &segs {
        put16(&mut sub, 0);
    }
    let mut cmap = Vec::new();
    put16(&mut cmap, 0);
    put16(&mut cmap, 1);
    put16(&mut cmap, 3);
    put16(&mut cmap, 1);
    put32(&mut cmap, 12);
    cmap.extend(sub);

    let strings = [(1u16, "Jumpman Pixel"), (2, "Regular"), (4, "Jumpman Pixel Regular"), (6, "JumpmanPixel-Regular")];
    let mut name = Vec::new();
    put16(&mut name, 0);
    put16(&mut name, strings.len() as u16);
    put16(&mut name, (6 + 12 * strings.len()) as u16);
    let mut pool = Vec::new();
    for (id, s) in &strings {
        let u = utf16(s);
        put16(&mut name, 3);
        put16(&mut name, 1);
        put16(&mut name, 0x409);
        put16(&mut name, *id);
        put16(&mut name, u.len() as u16);
        put16(&mut name, pool.len() as u16);
        pool.extend(u);
    }
    name.extend(pool);

    let mut post = Vec::new();
    put32(&mut post, 0x0003_0000);
    put32(&mut post, 0);
    puti16(&mut post, -100);
    puti16(&mut post, 50);
    put32(&mut post, 1);
    post.extend([0u8; 16]);

    let mut os2 = Vec::new();
    put16(&mut os2, 4);
    puti16(&mut os2, ADVANCE as i16);
    put16(&mut os2, 400);
    put16(&mut os2, 5);
    put16(&mut os2, 0);
    for v in [650, 600, 0, 75, 650, 600, 0, 350, 50, 300] {
        puti16(&mut os2, v);
    }
    puti16(&mut os2, 0);
    os2.extend([2, 0, 0, 9, 0, 0, 0, 0, 0, 0]);
    for v in [1u32, 0, 0, 0] {
        put32(&mut os2, v);
    }
    os2.extend(*b"JMPN");
    put16(&mut os2, 0x40);
    put16(&mut os2, 0x20);
    put16(&mut os2, 0x7e);
    puti16(&mut os2, ASCENT);
    puti16(&mut os2, DESCENT);
    puti16(&mut os2, 0);
    put16(&mut os2, ASCENT as u16);
    put16(&mut os2, (-DESCENT) as u16);
    put32(&mut os2, 1);
    put32(&mut os2, 0);
    puti16(&mut os2, 5 * PIXEL);
    puti16(&mut os2, 7 * PIXEL);
    put16(&mut os2, 0);
    put16(&mut os2, 0x20);
    put16(&mut os2, 1);

    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"OS/2", os2), (b"cmap", cmap), (b"glyf", glyf), (b"head", head), (b"hhea", hhea), (b"hmtx", hmtx), (b"loca", loca), (b"maxp", maxp), (b"name", name), (b"post", post)];
    let count = tables.len() as u16;
    let mut sel = 0u16;
    while (2u32 << sel) <= count as u32 {
        sel += 1;
    }
    let search = (1u16 << sel) * 16;
    let mut out = Vec::new();
    put32(&mut out, 0x0001_0000);
    put16(&mut out, count);
    put16(&mut out, search);
    put16(&mut out, sel);
    put16(&mut out, count * 16 - search);
    let offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    let mut head_at = 0;
    for (tag, data) in &tables {
        out.extend_from_slice(*tag);
        put32(&mut out, table_sum(data));
        put32(&mut out, (offset + body.len()) as u32);
        put32(&mut out, data.len() as u32);
        if *tag == b"head" {
            head_at = offset + body.len();
        }
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    out.extend(body);
    let adjust = 0xb1b0_afbau32.wrapping_sub(table_sum(&out));
    out[head_at + 8..head_at + 12].copy_from_slice(&adjust.to_be_bytes());
    out
}

/// The loaded font asset.
pub fn pixel_font() -> Font {
    Font::from_bytes(pixel_font_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], i: usize) -> usize {
        u16::from_be_bytes([b[i], b[i + 1]]) as usize
    }

    #[test]
    fn the_generated_font_has_a_consistent_table_directory() {
        let b = pixel_font_bytes();
        assert_eq!(&b[..4], &[0, 1, 0, 0]);
        let tables = u16_at(&b, 4);
        assert_eq!(tables, 10);
        let mut seen = Vec::new();
        for i in 0..tables {
            let r = 12 + i * 16;
            let tag = String::from_utf8_lossy(&b[r..r + 4]).into_owned();
            let off = u32::from_be_bytes([b[r + 8], b[r + 9], b[r + 10], b[r + 11]]) as usize;
            let len = u32::from_be_bytes([b[r + 12], b[r + 13], b[r + 14], b[r + 15]]) as usize;
            assert!(off + len <= b.len(), "{tag} runs past the end");
            assert_eq!(off % 4, 0);
            seen.push(tag);
        }
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "tags sorted: {seen:?}");
        // The whole file sums to the magic once the adjustment is in.
        assert_eq!(table_sum(&b), 0xb1b0_afba);
    }

    #[test]
    fn letters_have_outlines_and_space_is_empty() {
        let (a, pts, conts) = outline(glyph('A'));
        assert!(!a.is_empty() && pts == conts * 4);
        assert!(outline(glyph(' ')).0.is_empty());
    }
}
