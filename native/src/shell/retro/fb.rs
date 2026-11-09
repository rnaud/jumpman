//! A tiny CPU framebuffer plus an original 5x7 pixel font, used to draw the v3 menu at a low
//! internal resolution (about 320x240). The buffer is uploaded to an image that the shell
//! shows full-screen with bilinear filtering, which gives the soft, chunky look of a period console.

/// RGBA, straight alpha.
pub type Rgba = [u8; 4];

pub const fn rgb(c: u32) -> Rgba {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]
}

pub fn with_alpha(c: Rgba, a: u8) -> Rgba {
    [c[0], c[1], c[2], a]
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), f(a[3], b[3])]
}

/// Integer rectangle (x, y, w, h) in framebuffer px.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct IRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl IRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32 && py >= self.y as f32 && px < (self.x + self.w) as f32 && py < (self.y + self.h) as f32
    }
    pub fn inflate(&self, d: i32) -> IRect {
        IRect::new(self.x - d, self.y - d, self.w + 2 * d, self.h + 2 * d)
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

pub struct Fb {
    pub w: i32,
    pub h: i32,
    pub data: Vec<u8>,
}

/// 4x4 ordered-dither matrix (Bayer), values 0..16.
const BAYER: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

impl Fb {
    pub fn new(w: i32, h: i32) -> Self {
        Self { w, h, data: vec![0; (w * h * 4) as usize] }
    }

    pub fn clear(&mut self, c: Rgba) {
        for p in self.data.chunks_exact_mut(4) {
            p.copy_from_slice(&c);
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return [0; 4];
        }
        let i = ((y * self.w + x) * 4) as usize;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }

    /// Alpha-blend one pixel over the buffer.
    pub fn px(&mut self, x: i32, y: i32, c: Rgba) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || c[3] == 0 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        if c[3] == 255 {
            self.data[i..i + 4].copy_from_slice(&c);
            return;
        }
        let a = c[3] as f32 / 255.0;
        let da = self.data[i + 3] as f32 / 255.0;
        let oa = a + da * (1.0 - a);
        for k in 0..3 {
            let d = self.data[i + k] as f32;
            let v = if oa > 0.0 { (c[k] as f32 * a + d * da * (1.0 - a)) / oa } else { 0.0 };
            self.data[i + k] = v.round() as u8;
        }
        self.data[i + 3] = (oa * 255.0).round() as u8;
    }

    pub fn rect(&mut self, r: IRect, c: Rgba) {
        for y in r.y.max(0)..(r.y + r.h).min(self.h) {
            for x in r.x.max(0)..(r.x + r.w).min(self.w) {
                self.px(x, y, c);
            }
        }
    }

    /// Vertical gradient with ordered dithering (the banding-free look of a 16-bit framebuffer).
    pub fn vgrad(&mut self, r: IRect, top: Rgba, bottom: Rgba) {
        for y in r.y.max(0)..(r.y + r.h).min(self.h) {
            let t = (y - r.y) as f32 / (r.h - 1).max(1) as f32;
            for x in r.x.max(0)..(r.x + r.w).min(self.w) {
                let mut c = mix(top, bottom, t);
                let d = BAYER[(y & 3) as usize][(x & 3) as usize] as f32 / 16.0 - 0.5;
                for ch in c.iter_mut().take(3) {
                    *ch = (*ch as f32 + d * 7.0).clamp(0.0, 255.0) as u8;
                }
                self.px(x, y, c);
            }
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: Rgba) {
        self.rect(IRect::new(x, y, w, 1), c);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, c: Rgba) {
        self.rect(IRect::new(x, y, 1, h), c);
    }

    /// 1 px frame.
    pub fn frame(&mut self, r: IRect, c: Rgba) {
        self.hline(r.x, r.y, r.w, c);
        self.hline(r.x, r.y + r.h - 1, r.w, c);
        self.vline(r.x, r.y, r.h, c);
        self.vline(r.x + r.w - 1, r.y, r.h, c);
    }

    /// A raised slab: gradient face with a light top-left and dark bottom-right edge and an outline.
    pub fn slab(&mut self, r: IRect, top: Rgba, bottom: Rgba, light: Rgba, dark: Rgba, outline: Rgba) {
        self.rect(r, outline);
        let inner = IRect::new(r.x + 1, r.y + 1, r.w - 2, r.h - 2);
        self.vgrad(inner, top, bottom);
        self.hline(inner.x, inner.y, inner.w, light);
        self.vline(inner.x, inner.y, inner.h, light);
        self.hline(inner.x, inner.y + inner.h - 1, inner.w, dark);
        self.vline(inner.x + inner.w - 1, inner.y, inner.h, dark);
    }

    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        for y in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
            for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= r * r {
                    self.px(x, y, c);
                }
            }
        }
    }

    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: Rgba) {
        for y in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
            for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt();
                if d <= r && d >= r - thick {
                    self.px(x, y, c);
                }
            }
        }
    }

    /// Filled polygon (even-odd, pixel-centre sampling).
    pub fn poly(&mut self, pts: &[(f32, f32)], c: Rgba) {
        if pts.len() < 3 {
            return;
        }
        let (miny, maxy) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        for y in miny.floor() as i32..=maxy.ceil() as i32 {
            let sy = y as f32 + 0.5;
            let mut xs: Vec<f32> = Vec::new();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= sy && b.1 > sy) || (b.1 <= sy && a.1 > sy) {
                    xs.push(a.0 + (sy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks_exact(2) {
                for x in (pair[0] - 0.5).ceil() as i32..(pair[1] - 0.5).ceil() as i32 {
                    self.px(x, y, c);
                }
            }
        }
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Rgba) {
        let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as i32).max(1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.px((a.0 + (b.0 - a.0) * t).floor() as i32, (a.1 + (b.1 - a.1) * t).floor() as i32, c);
        }
    }

    /// Copy `src` (straight alpha) into this buffer at an integer offset, integer-scaled.
    pub fn blit(&mut self, src: &Fb, x: i32, y: i32, scale: i32) {
        for sy in 0..src.h {
            for sx in 0..src.w {
                let c = src.get(sx, sy);
                if c[3] == 0 {
                    continue;
                }
                self.rect(IRect::new(x + sx * scale, y + sy * scale, scale, scale), c);
            }
        }
    }

    // ---- text ----

    pub fn text_width(s: &str, scale: i32) -> i32 {
        let n = s.chars().count() as i32;
        if n == 0 { 0 } else { (n * 6 - 1) * scale }
    }

    /// Raw glyphs, no effects. `fill(row)` picks the colour per glyph row (0..7), for gradient lettering.
    pub fn glyphs(&mut self, x: i32, y: i32, s: &str, scale: i32, fill: &dyn Fn(i32) -> Rgba) {
        let mut cx = x;
        for ch in s.chars() {
            let g = glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..5 {
                    if bits & (0b10000 >> col) != 0 {
                        self.rect(IRect::new(cx + col * scale, y + row as i32 * scale, scale, scale), fill(row as i32));
                    }
                }
            }
            cx += 6 * scale;
        }
    }

    /// Text with a hard drop shadow.
    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgba) {
        self.glyphs(x + scale, y + scale, s, scale, &|_| rgb(0x050a22));
        self.glyphs(x, y, s, scale, &|_| c);
    }

    /// Text with a full outline (8 neighbours), then the fill. `fill` per row gives gradient lettering.
    pub fn text_outlined(&mut self, x: i32, y: i32, s: &str, scale: i32, outline: Rgba, fill: &dyn Fn(i32) -> Rgba) {
        let o = scale.min(2);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx != 0 || dy != 0 {
                    self.glyphs(x + dx * o, y + dy * o, s, scale, &|_| outline);
                }
            }
        }
        self.glyphs(x, y, s, scale, fill);
    }

    pub fn text_centered(&mut self, cx: i32, y: i32, s: &str, scale: i32, c: Rgba) {
        self.text(cx - Self::text_width(s, scale) / 2, y, s, scale, c);
    }
}

/// Map arbitrary text onto the font: upper-case, unknown characters become `?`.
pub fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            let u = c.to_ascii_uppercase();
            if GLYPHS.iter().any(|(g, _)| *g == u) {
                u
            } else if c == '\u{b7}' {
                '-'
            } else {
                '?'
            }
        })
        .collect()
}

pub fn glyph(c: char) -> [u8; 7] {
    let u = c.to_ascii_uppercase();
    GLYPHS.iter().find(|(g, _)| *g == u).map_or(QUESTION, |(_, bits)| *bits)
}

const QUESTION: [u8; 7] = [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100];

/// Hand-drawn 5x7 capitals, digits and punctuation (original pixel art).
pub const GLYPHS: &[(char, [u8; 7])] = &[
    (' ', [0; 7]),
    ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
    ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('I', [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ('N', [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001]),
    ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001]),
    ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ('Y', [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
    ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('2', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111]),
    ('3', [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    ('.', [0, 0, 0, 0, 0, 0b01100, 0b01100]),
    (',', [0, 0, 0, 0, 0b01100, 0b00100, 0b01000]),
    (':', [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0]),
    ('!', [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100]),
    ('?', QUESTION),
    ('-', [0, 0, 0, 0b11111, 0, 0, 0]),
    ('+', [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0]),
    ('/', [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000]),
    ('\'', [0b00100, 0b00100, 0b01000, 0, 0, 0, 0]),
    ('(', [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010]),
    (')', [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000]),
    ('%', [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011]),
    ('#', [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010]),
    ('*', [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0]),
    ('<', [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010]),
    ('>', [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000]),
    ('=', [0, 0, 0b11111, 0, 0b11111, 0, 0]),
    ('_', [0, 0, 0, 0, 0, 0, 0b11111]),
    ('&', [0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_table_has_unique_nonblank_letters() {
        for (i, (c, bits)) in GLYPHS.iter().enumerate() {
            assert!(GLYPHS[i + 1..].iter().all(|(d, _)| d != c), "duplicate glyph {c}");
            if *c != ' ' {
                assert!(bits.iter().any(|b| *b != 0), "blank glyph {c}");
            }
            assert!(bits.iter().all(|b| *b < 32));
        }
        for c in 'A'..='Z' {
            assert!(GLYPHS.iter().any(|(g, _)| *g == c));
        }
    }

    #[test]
    fn clean_maps_unknown_characters() {
        assert_eq!(clean("Gem Clash"), "GEM CLASH");
        assert_eq!(clean("a\u{e9}b"), "A?B");
    }

    #[test]
    fn text_draws_pixels_and_clips() {
        let mut fb = Fb::new(16, 8);
        fb.text(-3, 0, "AB", 1, rgb(0xffffff));
        assert!(fb.data.chunks(4).any(|p| p[3] == 255));
        assert_eq!(Fb::text_width("AB", 2), 22);
    }

    #[test]
    fn blending_over_transparent_keeps_colour() {
        let mut fb = Fb::new(2, 2);
        fb.px(0, 0, [200, 100, 50, 128]);
        let p = fb.get(0, 0);
        assert_eq!(&p[..3], &[200, 100, 50]);
        assert_eq!(p[3], 128);
    }
}
