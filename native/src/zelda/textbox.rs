//! Port of the layout/typewriter logic of `ootTextbox.ts`: OoT message pages with control codes
//! (`\x05` colour, `\x06` advance, `\x08`/`\x09` instant text, `\x1b`/`\x1c` choice block),
//! per-glyph reveal at ~47 chars/s, the box pop-in scale, and choice cycling. The renderer draws
//! the returned [`Layout`] using the glyph atlas in `assets/oot-dialogue` (16x16 cells, drawn at
//! 12x12, 16 per row, advance `floor(width*0.75)`).
use std::path::Path;

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct MessagePage {
    pub text: String,
    pub style: u32,
    pub branches: Option<Vec<Vec<MessagePage>>>,
}

pub const CONTROLS: [&str; 12] = ["A", "B", "C", "L", "R", "Z", "C-Up", "C-Down", "C-Left", "C-Right", "target", "Control Stick"];
pub const COLORS: [&str; 8] = ["#fff", "#ff3c3c", "#46ff50", "#505aff", "#64b4ff", "#ff96b4", "#e1ff32", "#000"];
pub const WOOD_COLORS: [&str; 8] = ["#fff", "#ff7800", "#46ff50", "#506eff", "#5ab4ff", "#d264ff", "#ffff1e", "#000"];

/// Glyph advance widths (`widths.json`).
pub fn load_widths(assets: &Path) -> Result<Vec<f64>, String> {
    let p = assets.join("oot-dialogue/widths.json");
    serde_json::from_slice(&std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| e.to_string())
}

/// Plain text for accessibility/logging: strips colour/advance/speed codes and names controls.
pub fn accessible_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i] as u32;
        match c {
            5 | 6 => i += 1,
            8 | 9 | 0x1b | 0x1c => {}
            0x9f..=0xaa => {
                out.push('[');
                out.push_str(CONTROLS[(c - 0x9f) as usize]);
                out.push(']');
            }
            _ => out.push(chars[i]),
        }
        i += 1;
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub index: usize,
    pub x: f64,
    pub y: f64,
    /// Colour slot (0..8) into [`COLORS`] / [`WOOD_COLORS`].
    pub color: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon {
    Arrow { x: f64, y: f64 },
    Next { x: f64, y: f64 },
    End { x: f64, y: f64 },
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub glyphs: Vec<Glyph>,
    pub icon: Option<Icon>,
    pub wood: bool,
    /// Pop-in scale of the box (1 when settled).
    pub grow: f64,
    /// Top of the box in 320x240 screen units.
    pub top: f64,
}

#[derive(Default)]
pub struct OotTextbox {
    pub page: Option<MessagePage>,
    pub speaker: String,
    page_started: f64,
    opened: f64,
    visible_count: f64,
    has_next: bool,
    pub choice: usize,
    stick_held: bool,
}

impl OotTextbox {
    pub fn shown(&self) -> bool {
        self.page.is_some()
    }

    /// `now` is a monotonic clock in milliseconds.
    pub fn show(&mut self, speaker: &str, page: MessagePage, has_next: bool, immediate: bool, now: f64) {
        let was_hidden = self.page.is_none();
        self.page = Some(page);
        self.has_next = has_next;
        self.choice = 0;
        self.stick_held = false;
        self.page_started = now;
        if was_hidden {
            self.opened = now;
        }
        self.speaker = speaker.to_string();
        self.visible_count = if immediate { f64::INFINITY } else { 0.0 };
    }

    /// Hide; the next `show` replays the pop-in.
    pub fn hide(&mut self) {
        self.page = None;
        self.speaker.clear();
    }

    /// A first tap completes an animating page; the next follows the original continuation.
    pub fn finish_reveal(&mut self) -> bool {
        let Some(page) = &self.page else { return false };
        if self.visible_count >= page.text.chars().count() as f64 {
            return false;
        }
        self.visible_count = f64::INFINITY;
        true
    }

    pub fn choose(&mut self, stick_y: f64) {
        let count = self.page.as_ref().and_then(|p| p.branches.as_ref()).map_or(0, Vec::len);
        if count == 0 || stick_y.abs() < 30.0 {
            self.stick_held = false;
            return;
        }
        if !self.stick_held {
            self.choice = (self.choice + count + if stick_y > 0.0 { count - 1 } else { 1 }) % count;
        }
        self.stick_held = true;
    }

    /// Lay out the page at time `now` (ms). `actor_y` is the NPC's screen height (0..1).
    pub fn layout(&mut self, widths: &[f64], actor_y: f64, now: f64) -> Layout {
        let Some(page) = self.page.clone() else { return Layout::default() };
        let position = page.style & 15;
        let top = match position {
            1 => 38.0,
            2 => 90.0,
            3 => 142.0,
            _ => {
                if actor_y > 0.59 { 38.0 } else { 142.0 }
            }
        };
        let frame = ((now - self.opened) * 0.02).floor();
        let grow = if (0.0..8.0).contains(&frame) { [0.6, 0.75, 0.9, 1.0, 1.05, 1.1, 1.05, 1.0][frame as usize] } else { 1.0 };
        let wood = page.style >> 4 == 1;
        let chars: Vec<u32> = page.text.chars().map(|c| c as u32).collect();
        let elapsed = now - self.page_started;
        self.visible_count = self.visible_count.max((elapsed / 21.0).floor() + 1.0);
        let mut limit = self.visible_count;
        let lines = page.text.split('\n').count().min(4);
        let mut x = 31.0;
        let mut y = match lines {
            1 => 26.0,
            2 => 20.0,
            3 => 16.0,
            _ => 8.0,
        };
        let (mut color, mut quick, mut choice_y, mut choice_block) = (0usize, false, 0.0, false);
        let mut glyphs = vec![];
        let mut i = 0usize;
        while i < chars.len() {
            let code = chars[i];
            if code == 8 {
                if i as f64 >= limit {
                    break;
                }
                quick = true;
                i += 1;
                continue;
            }
            if code == 9 {
                quick = false;
                limit = limit.max(i as f64 + 1.0);
                i += 1;
                continue;
            }
            if i as f64 >= limit && !quick {
                break;
            }
            if code == 5 {
                i += 1;
                color = (chars.get(i).copied().unwrap_or(64) as i64 - 64) as usize & 7;
                i += 1;
                continue;
            }
            if code == 6 {
                i += 1;
                x += chars.get(i).copied().unwrap_or(0) as f64;
                i += 1;
                continue;
            }
            if code == 27 || code == 28 {
                choice_block = true;
                choice_y = y;
                x += 16.0;
                i += 1;
                continue;
            }
            if code == 10 {
                x = if choice_block { 47.0 } else { 31.0 };
                y += 12.0;
                i += 1;
                continue;
            }
            let idx = code as i64 - 32;
            i += 1;
            if idx < 0 || idx as usize >= widths.len() {
                continue;
            }
            glyphs.push(Glyph { index: idx as usize, x, y, color });
            x += (widths[idx as usize] * 0.75).floor();
        }
        if quick || limit >= chars.len() as f64 {
            self.visible_count = f64::INFINITY;
        }
        let icon = (self.visible_count >= chars.len() as f64).then(|| {
            if choice_block {
                Icon::Arrow { x: 31.0, y: choice_y + self.choice as f64 * 12.0 }
            } else if self.has_next {
                Icon::Next { x: 121.0, y: 49.0 }
            } else {
                Icon::End { x: 121.0, y: 49.0 }
            }
        });
        Layout { glyphs, icon, wood, grow, top }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths() -> Vec<f64> {
        load_widths(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")).unwrap()
    }

    fn page(text: &str) -> MessagePage {
        MessagePage { text: text.into(), style: 0, branches: None }
    }

    #[test]
    fn typewriter_reveals_one_glyph_per_21ms() {
        let w = widths();
        let mut t = OotTextbox::default();
        t.show("Navi", page("Hello"), false, false, 0.0);
        assert_eq!(t.layout(&w, 0.0, 0.0).glyphs.len(), 1);
        assert_eq!(t.layout(&w, 0.0, 45.0).glyphs.len(), 3);
        let done = t.layout(&w, 0.0, 500.0);
        assert_eq!(done.glyphs.len(), 5);
        assert!(matches!(done.icon, Some(Icon::End { .. })));
        // first glyph 'H' at x = 31, advance floor(width * .75)
        assert_eq!(done.glyphs[0], Glyph { index: ('H' as usize) - 32, x: 31.0, y: 26.0, color: 0 });
        assert_eq!(done.glyphs[1].x, 31.0 + (w[('H' as usize) - 32] * 0.75).floor());
    }

    #[test]
    fn finish_reveal_then_next_icon() {
        let w = widths();
        let mut t = OotTextbox::default();
        t.show("Mido", page("A long line of text"), true, false, 0.0);
        t.layout(&w, 0.0, 10.0);
        assert!(t.finish_reveal());
        assert!(!t.finish_reveal());
        let l = t.layout(&w, 0.0, 20.0);
        assert!(matches!(l.icon, Some(Icon::Next { .. })));
        assert_eq!(l.glyphs.len(), 19);
    }

    #[test]
    fn colour_and_control_codes() {
        let w = widths();
        let mut t = OotTextbox::default();
        t.show("x", page("\u{5}Aab\u{6}\u{a}c"), false, true, 0.0);
        let l = t.layout(&w, 0.0, 0.0);
        // \x05 'A' => colour (65-64)&7 = 1; glyphs a, b, then advance 10, then 'c'
        assert_eq!(l.glyphs.len(), 3);
        assert!(l.glyphs.iter().all(|g| g.color == 1));
        let gap = l.glyphs[2].x - l.glyphs[1].x;
        assert!(gap >= 10.0);
        assert_eq!(accessible_text("\u{5}Aab\u{9f}"), "ab[A]");
    }

    #[test]
    fn choices_cycle_with_stick_edges() {
        let mut t = OotTextbox::default();
        let mut p = page("\u{1b}Yes\nNo");
        p.branches = Some(vec![vec![], vec![], vec![]]);
        t.show("x", p, false, true, 0.0);
        t.choose(-60.0);
        assert_eq!(t.choice, 1);
        t.choose(-60.0);
        assert_eq!(t.choice, 1, "held stick does not repeat");
        t.choose(0.0);
        t.choose(60.0);
        assert_eq!(t.choice, 0);
        t.choose(0.0);
        t.choose(60.0);
        assert_eq!(t.choice, 2, "wraps");
    }

    #[test]
    fn box_position_and_popin() {
        let w = widths();
        let mut t = OotTextbox::default();
        let mut p = page("x");
        p.style = 0x12;
        t.show("x", p, false, true, 1000.0);
        let l = t.layout(&w, 0.0, 1000.0);
        assert_eq!((l.top, l.grow, l.wood), (90.0, 0.6, true));
        assert_eq!(t.layout(&w, 0.0, 1400.0).grow, 1.0);
        t.show("x", page("y"), false, true, 2000.0);
        assert_eq!(t.layout(&w, 0.7, 2000.0).top, 38.0, "NPC low on screen: box on top... when actor_y > .59");
    }
}
