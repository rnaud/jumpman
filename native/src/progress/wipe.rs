//! Screen transitions (`StarWarp.ts`): the screen_transition.c star / Mario / Bowser wipes, plus
//! the circular iris used for door and floor warps. Pure pixel generation; the Bevy overlay lives
//! in `flow.rs`.
//!
//! The overlay is a 240-row RGBA image: black everywhere except a square window (`radius`
//! around the centre) filled with the mask texture (mirrored halves for the star/circle/Bowser
//! masks, whole for Mario). Alpha 0 is see-through.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WipeKind {
    /// Mario's face closes in (star collected, 32 frames).
    Mario,
    /// Bowser's face (death warp, 48 frames).
    Bowser,
    /// Plain circle (fall warp, 20 frames).
    Circle,
    /// Percentage iris of the door / floor warps (`clip-path: circle(r%)`).
    Iris,
}

impl WipeKind {
    pub fn duration(self) -> u32 {
        match self {
            WipeKind::Bowser => 48,
            WipeKind::Circle => 20,
            WipeKind::Mario => 32,
            WipeKind::Iris => 20,
        }
    }
}

/// An ia8 mask texture as RGBA8 (R = intensity, A = coverage).
#[derive(Clone, Debug)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Mask {
    pub fn solid(width: usize, height: usize, alpha: u8) -> Self {
        Self { width, height, rgba: [0, 0, 0, alpha].repeat(width * height) }
    }

    fn sample(&self, u: f64, v: f64) -> [u8; 4] {
        let x = ((u * self.width as f64) as usize).min(self.width - 1);
        let y = ((v * self.height as f64) as usize).min(self.height - 1);
        let i = (y * self.width + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Masks {
    pub star: Option<Mask>,
    pub mario: Option<Mask>,
    pub bowser: Option<Mask>,
    pub circle: Option<Mask>,
}

/// Overlay width in pixels for a viewport (`240 * aspect`).
pub fn overlay_width(width: f32, height: f32) -> usize {
    ((240.0 * width / height.max(1.0)).round() as usize).max(1)
}

/// Radius of the transparent window in overlay pixels.
pub fn radius(opening: bool, frame: u32, w: usize, kind: WipeKind) -> i32 {
    let full = w.max(240) as f64;
    let duration = f64::from(kind.duration());
    let end = if kind == WipeKind::Circle { 0.0 } else { 16.0 };
    let frame = f64::from(frame);
    (if opening { full * frame / 15.0 } else { full + (end - full) * frame / (duration - 1.0) }).round() as i32
}

/// Fill `buf` (w x 240 RGBA8) for one frame.
pub fn render(buf: &mut Vec<u8>, w: usize, opening: bool, frame: u32, kind: WipeKind, masks: &Masks) {
    buf.clear();
    buf.extend([0u8, 0, 0, 255].repeat(w * 240));
    if kind == WipeKind::Iris {
        // `circle(r% at center)`: r is a share of sqrt(w^2+h^2)/sqrt2.
        let r = ((20 - frame.min(20)) as f64 * 5.0 / 100.0) * ((w * w + 240 * 240) as f64 / 2.0).sqrt();
        let (cx, cy) = (w as f64 / 2.0, 120.0);
        for y in 0..240 {
            for x in 0..w {
                if ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt() < r {
                    buf[(y * w + x) * 4 + 3] = 0;
                }
            }
        }
        return;
    }
    let r = radius(opening, frame, w, kind);
    if r <= 0 {
        return;
    }
    let mask = if opening {
        masks.star.as_ref()
    } else {
        match kind {
            WipeKind::Mario => masks.mario.as_ref(),
            WipeKind::Bowser => masks.bowser.as_ref(),
            _ => masks.circle.as_ref(),
        }
    };
    let (x0, y0) = (((w as f64) / 2.0).round() as i32 - r, 120 - r);
    let size = (r * 2) as f64;
    let whole = !opening && kind == WipeKind::Mario;
    for y in y0.max(0)..(y0 + r * 2).min(240) {
        for x in x0.max(0)..(x0 + r * 2).min(w as i32) {
            let i = (y as usize * w + x as usize) * 4;
            let u = (f64::from(x - x0) + 0.5) / size;
            let v = (f64::from(y - y0) + 0.5) / size;
            // The window is cleared first; the mask is drawn on top (halves are mirrored).
            let px = match mask {
                None => [0, 0, 0, 0],
                Some(m) if whole => m.sample(u, v),
                Some(m) => m.sample(if u < 0.5 { (0.5 - u) * 2.0 } else { (u - 0.5) * 2.0 }, v),
            };
            buf[i..i + 4].copy_from_slice(&px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_follows_screen_transition_c() {
        assert_eq!(radius(true, 0, 320, WipeKind::Mario), 0);
        assert_eq!(radius(true, 15, 320, WipeKind::Mario), 320);
        assert_eq!(radius(false, 0, 320, WipeKind::Mario), 320);
        assert_eq!(radius(false, 31, 320, WipeKind::Mario), 16);
        assert_eq!(radius(false, 19, 320, WipeKind::Circle), 0);
        assert_eq!(radius(false, 47, 320, WipeKind::Bowser), 16);
        // Narrow viewports still use the 240 high projection.
        assert_eq!(radius(true, 15, 180, WipeKind::Mario), 240);
    }

    #[test]
    fn closing_wipe_ends_almost_black_and_opening_starts_black() {
        let masks = Masks { circle: Some(Mask::solid(32, 64, 0)), mario: Some(Mask::solid(64, 64, 0)), star: Some(Mask::solid(32, 64, 0)), bowser: None };
        let mut buf = vec![];
        render(&mut buf, 320, true, 0, WipeKind::Mario, &masks);
        assert!(buf.chunks(4).all(|p| p[3] == 255), "opening frame 0 is fully black");
        render(&mut buf, 320, true, 15, WipeKind::Mario, &masks);
        assert!(buf.chunks(4).all(|p| p[3] == 0), "opening ends clear");
        render(&mut buf, 320, false, 0, WipeKind::Circle, &masks);
        assert!(buf.chunks(4).all(|p| p[3] == 0), "closing starts clear");
        render(&mut buf, 320, false, 19, WipeKind::Circle, &masks);
        assert!(buf.chunks(4).all(|p| p[3] == 255), "circle closes completely");
        render(&mut buf, 320, false, 31, WipeKind::Mario, &masks);
        let clear = buf.chunks(4).filter(|p| p[3] == 0).count();
        assert!(clear > 0 && clear < 32 * 32 + 8, "a 16px window remains: {clear}");
    }

    #[test]
    fn iris_shrinks() {
        let mut buf = vec![];
        let count = |buf: &Vec<u8>| buf.chunks(4).filter(|p| p[3] == 0).count();
        render(&mut buf, 320, false, 0, WipeKind::Iris, &Masks::default());
        let wide = count(&buf);
        render(&mut buf, 320, false, 10, WipeKind::Iris, &Masks::default());
        let mid = count(&buf);
        render(&mut buf, 320, false, 20, WipeKind::Iris, &Masks::default());
        assert!(wide > mid && mid > 0 && count(&buf) == 0);
    }
}
