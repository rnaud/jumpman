//! Port of `src/game/signpost.ts` (reading a signpost) and `kingDialog.ts` (the King Bob-omb
//! dialog pages). The dialog box itself is plain data ([`DialogBox`]) that `objects::dialog`
//! draws with bevy_ui. Engine-free.
use super::course_extras::Signpost;
use bevy::math::DVec3;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::math::*;
use jumpman_sim::object::mario_hitbox;
use jumpman_sim::state::*;

/// The on-screen dialog box (`sm64-sign-dialog` / `HUD.showDialogPage`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DialogBox {
    /// Text to show (lines separated by `\n`), `None` hides the box.
    pub text: Option<String>,
    /// Box scale for the open/close animation.
    pub scale: f64,
    /// Scroll offset in lines (sign page scroll).
    pub scroll_lines: f64,
    /// Fixed height in lines; 0 = fit the text.
    pub height_lines: f64,
}

impl DialogBox {
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = Some(text.into());
        self.scale = 1.0;
        self.scroll_lines = 0.0;
        self.height_lines = 0.0;
    }

    pub fn hide(&mut self) {
        *self = Self::default();
    }
}

pub fn lines(text: &str) -> usize {
    text.split('\n').count()
}

/// `readableSign`: the sign Mario is facing, standing idle (or walking in place) in front of.
/// `clip` is the glTF clip index of Mario's current animation (anim_80/7F/6C are the SM64
/// slow-walk clips that still count as idle).
pub fn readable_sign<'a>(signs: &'a [Signpost], m: &MarioRuntime, clip: Option<u32>) -> Option<&'a Signpost> {
    let walking_in_place = m.action == ACT_WALKING && matches!(clip, Some(0x80) | Some(0x7f) | Some(0x6c));
    if m.action & ACT_FLAG_IDLE == 0 && !walking_in_place {
        return None;
    }
    let hitbox = mario_hitbox(m);
    signs.iter().find(|sign| {
        let [x, y, z] = sign.pos;
        let (dx, dz) = (m.pos[0] - x, m.pos[2] - z);
        js_hypot(dx, dz) < 150.0 + hitbox.radius
            && m.pos[1] <= y + 80.0
            && m.pos[1] + hitbox.height >= y
            && (to_s16(atan2s(dz, dx) as f64 - sign.yaw) as f64).abs() <= 0x38e3 as f64
            && (to_s16(sign.yaw + 0x8000 as f64 - m.face_angle[1]) as f64).abs() <= 0x38e3 as f64
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignSound {
    Read,
    Open,
    Page,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Align,
    Open,
    Read,
    Scroll,
    Close,
    Return,
}

pub struct SignReading {
    sign: Signpost,
    saved_eye: DVec3,
    saved_focus: DVec3,
    eye: DVec3,
    focus: DVec3,
    pitch: f64,
    yaw: f64,
    page: usize,
    clock: i32,
    phase: Phase,
    /// Camera follows Mario once the alignment is over.
    scripted: bool,
}

impl SignReading {
    /// `new SignReading(sign, mario, camera, sound)`: starts the reading action. Returns the
    /// sound cues to play.
    pub fn new(sign: &Signpost, m: &mut MarioRuntime, eye: DVec3, focus: DVec3) -> (Self, Vec<SignSound>) {
        let [x, _, z] = sign.pos;
        m.sign_motion = [
            (x + 105.0 * sins(sign.yaw) - m.pos[0]) / 11.0,
            (to_s16(sign.yaw + 0x8000 as f64 - m.face_angle[1]) as f64 / 11.0).trunc(),
            (z + 105.0 * coss(sign.yaw) - m.pos[2]) / 11.0,
        ];
        m.set_action(ACT_READING_SIGN, 0.0);
        let me = Self { sign: sign.clone(), saved_eye: eye, saved_focus: focus, eye, focus, pitch: -(0x830 as f64), yaw: 0.0, page: 0, clock: 0, phase: Phase::Align, scripted: false };
        (me, vec![SignSound::Read])
    }

    /// (phase, page, clock) for the differential tests.
    pub fn debug_state(&self) -> (&'static str, usize, i32) {
        let phase = match self.phase {
            Phase::Align => "align",
            Phase::Open => "open",
            Phase::Read => "read",
            Phase::Scroll => "scroll",
            Phase::Close => "close",
            Phase::Return => "return",
        };
        (phase, self.page, self.clock)
    }

    pub fn camera(&self) -> (DVec3, DVec3) {
        (self.eye, self.focus)
    }

    /// One 30 Hz step (after Mario's own step). Returns (finished, sounds).
    pub fn step(&mut self, m: &mut MarioRuntime, controls: &ControllerSnapshot, ui: &mut DialogBox) -> (bool, Vec<SignSound>) {
        let mut sounds = vec![];
        if self.phase == Phase::Align {
            if m.action_state != 2.0 {
                return (false, sounds);
            }
            self.phase = Phase::Open;
            sounds.push(SignSound::Open);
            self.scripted = true;
            ui.show(self.sign.pages[0].clone());
            ui.height_lines = lines(&self.sign.pages[0]) as f64 * 1.4 + 1.0;
        }
        self.clock += 1;
        if self.phase == Phase::Return {
            let factor = 1.0 / (16 - self.clock) as f64;
            self.eye = self.eye.lerp(self.saved_eye, factor);
            self.focus = self.focus.lerp(self.saved_focus, factor);
            if self.clock == 15 {
                m.set_action(ACT_IDLE, 0.0);
                ui.hide();
                return (true, sounds);
            }
            return (false, sounds);
        }

        self.pitch = clamp(self.pitch + (controls.stick_y * 10.0).trunc(), -(0x2000 as f64), 0x38e3 as f64);
        self.yaw = clamp(self.yaw - (controls.stick_x * 10.0).trunc(), -(0x5555 as f64), 0x5555 as f64);
        let yaw = m.face_angle[1] + self.yaw + 0x8000 as f64;
        let [x, y, z] = m.pos;
        let target = DVec3::new(x + 250.0 * coss(self.pitch) * sins(yaw), y + 125.0 + 250.0 * sins(self.pitch), z + 250.0 * coss(self.pitch) * coss(yaw));
        self.eye = self.eye.lerp(target, 1.0 / (1.0f64).max(30.0 - m.action_timer));
        self.focus = DVec3::new(x, y + 125.0, z);
        let pressed = controls.button_pressed & (A_BUTTON | B_BUTTON) != 0;
        if self.phase == Phase::Open {
            ui.scale = 1.0 / (1.0f64).max(19.0 - 2.0 * self.clock as f64);
            if self.clock == 9 {
                self.phase = Phase::Read;
            }
        } else if self.phase == Phase::Read && pressed {
            self.clock = 0;
            self.phase = if self.page + 1 == self.sign.pages.len() { Phase::Close } else { Phase::Scroll };
            if self.phase == Phase::Scroll {
                sounds.push(SignSound::Page);
                let next = format!("{}\n{}", ui.text.clone().unwrap_or_default(), self.sign.pages[self.page + 1]);
                ui.text = Some(next);
            }
        } else if self.phase == Phase::Scroll {
            ui.scroll_lines = 1.4 * lines(&self.sign.pages[self.page]) as f64 * self.clock as f64 / 8.0;
            if self.clock == 8 {
                ui.scroll_lines = 0.0;
                self.page += 1;
                ui.text = Some(self.sign.pages[self.page].clone());
                self.phase = Phase::Read;
            }
        } else if self.phase == Phase::Close {
            if self.clock == 3 {
                sounds.push(SignSound::Close);
            }
            ui.scale = 1.0 / (1.0 + 2.0 * self.clock as f64);
            if self.clock == 9 {
                ui.hide();
                self.clock = 0;
                self.phase = Phase::Return;
            }
        }
        (false, sounds)
    }

    /// Whether the camera should be overridden (after alignment).
    pub fn camera_active(&self) -> bool {
        self.scripted
    }
}

// US dialogs 017, 116 and 128, in their original pages.
pub fn king_dialog_pages(id: i32) -> &'static [&'static str] {
    match id {
        17 => &[
            "I'm the Big Bob-omb, lord of all blasting matter, king of ka-booms the world over!",
            "How dare you scale my mountain? By what right do you set foot on my imperial mountaintop?",
            "You may have eluded my guards, but you'll never escape my grasp...",
            "...and you'll never take away my Power Star. I hereby challenge you, Mario!",
            "If you want the Star I hold, you must prove yourself in battle.",
            "Can you pick me up from the back and hurl me to this royal turf? I think that you cannot!",
        ],
        116 => &[
            "Whaaa....Whaaat? Can it be that a pipsqueak like you has defused the Bob-omb king????",
            "You might be fast enough to ground me, but you'll have to pick up the pace if you want to take King Bowser by the tail.",
            "Methinks my troops could learn a lesson from you! Here is your Star, as I promised, Mario.",
            "If you want to see me again, select this Star from the menu. For now, farewell.",
        ],
        128 => &["You must fight with honor! It is against the royal rules to throw the king out of the ring!"],
        _ => sm64_dialog_pages(id),
    }
}

/// The text boxes of any other SM64 dialog (`sm64_dialogs`, the NPC dialog modal of the round-2 bosses: Eyerok,
/// Wiggler, ...): at least four lines per page, the lines joined by spaces, empty lines dropped.
fn sm64_dialog_pages(id: i32) -> &'static [&'static str] {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<i32, &'static [&'static str]>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(p) = cache.get(&id) {
        return p;
    }
    let pages: Vec<&'static str> = match jumpman_sim::sm64_dialogs::dialog(id.max(0) as u32) {
        Some(d) => d
            .text
            .split('\n')
            .collect::<Vec<_>>()
            .chunks(d.lines_per_box.max(4) as usize)
            .map(|c| c.iter().filter(|l| !l.trim().is_empty()).copied().collect::<Vec<_>>().join(" "))
            .filter(|p| !p.is_empty())
            .map(|p| &*Box::leak(p.into_boxed_str()))
            .collect(),
        None => vec![],
    };
    let pages: &'static [&'static str] = Box::leak(pages.into_boxed_slice());
    cache.insert(id, pages);
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
    use jumpman_sim::mario_runtime::SpawnPoint;
    use std::sync::Arc;

    fn mario_at(pos: [f64; 3], face: f64) -> MarioRuntime {
        let mut w = CollisionWorld::default();
        w.add_triangle([-3000.0, 0.0, -3000.0], [3000.0, 0.0, 3000.0], [3000.0, 0.0, -3000.0], &SurfaceOptions::default());
        w.add_triangle([-3000.0, 0.0, -3000.0], [-3000.0, 0.0, 3000.0], [3000.0, 0.0, 3000.0], &SurfaceOptions::default());
        MarioRuntime::new(Arc::new(w), SpawnPoint { action: None, pos, face_yaw: face }, -9000.0)
    }

    fn sign() -> Signpost {
        // Faces +Z (yaw 0); readers stand in front (+Z) looking back toward -Z (yaw 0x8000).
        Signpost { pos: [0.0, 0.0, 0.0], yaw: 0.0, pages: vec!["Page one\nline two".into(), "Page two".into()] }
    }

    #[test]
    fn sign_is_readable_only_from_the_front_and_facing_it() {
        let signs = [sign()];
        let m = mario_at([0.0, 0.0, 100.0], 0x8000 as f64);
        assert!(readable_sign(&signs, &m, None).is_some());
        // Behind the sign.
        let behind = mario_at([0.0, 0.0, -100.0], 0.0);
        assert!(readable_sign(&signs, &behind, None).is_none());
        // In front but looking away.
        let away = mario_at([0.0, 0.0, 100.0], 0.0);
        assert!(readable_sign(&signs, &away, None).is_none());
        // Too far.
        let far = mario_at([0.0, 0.0, 400.0], 0x8000 as f64);
        assert!(readable_sign(&signs, &far, None).is_none());
    }

    #[test]
    fn reading_walks_through_pages_and_returns() {
        let s = sign();
        let mut m = mario_at([0.0, 0.0, 100.0], 0x8000 as f64);
        let mut ui = DialogBox::default();
        let (mut reading, sounds) = SignReading::new(&s, &mut m, DVec3::new(0.0, 300.0, 600.0), DVec3::ZERO);
        assert_eq!(sounds, vec![SignSound::Read]);
        assert_eq!(m.action, ACT_READING_SIGN);
        let idle = ControllerSnapshot::default();
        let press = ControllerSnapshot { button_pressed: A_BUTTON, button_down: A_BUTTON, ..Default::default() };
        let mut done = false;
        let mut opened = false;
        for tick in 0..200 {
            m.step(&idle, 0.0, false, false);
            let c = if opened && ui.scale == 1.0 && tick % 12 == 0 { &press } else { &idle };
            let (d, _) = reading.step(&mut m, c, &mut ui);
            opened |= ui.text.is_some();
            if d {
                done = true;
                break;
            }
        }
        assert!(opened, "box opened");
        assert!(done, "reading finished");
        assert_eq!(ui.text, None);
        assert_eq!(m.action, ACT_IDLE);
    }

    #[test]
    fn king_pages_match_ids() {
        assert_eq!(king_dialog_pages(17).len(), 6);
        assert_eq!(king_dialog_pages(116).len(), 4);
        assert_eq!(king_dialog_pages(128).len(), 1);
    }
}
