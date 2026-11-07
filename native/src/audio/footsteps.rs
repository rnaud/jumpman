//! Port of `footsteps.ts`: the two `play_step_sound` frames of the walking clips.
use crate::game::sm64_animation::{choose_clip, AnimInput, Playback};
use jumpman_sim::state::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Step,
    Tiptoe,
}

/// (clip id, start frame, loop end, step frame 1, step frame 2)
const CLIPS: [(u32, f64, f64, f64, f64); 7] = [
    (0xCA, 25.0, 30.0, 7.0, 22.0),
    (0x92, 27.0, 116.0, 14.0, 72.0),
    (0x48, 0.0, 77.0, 10.0, 49.0),
    (0x72, 0.0, 72.0, 9.0, 45.0),
    (0x18, 42.0, 100.0, 12.0, 62.0),
    (0x16, 0.0, 100.0, 12.0, 62.0),
    (0x17, 0.0, 76.0, 10.0, 49.0),
];

fn clip(id: u32) -> Option<&'static (u32, f64, f64, f64, f64)> {
    CLIPS.iter().find(|c| c.0 == id)
}

#[derive(Default)]
pub struct Footsteps {
    clip: Option<u32>,
    position: f64,
}

impl Footsteps {
    pub fn reset(&mut self) {
        self.clip = None;
    }

    pub fn step(&mut self, m: &AnimInput) -> Option<Step> {
        if ![ACT_WALKING, ACT_DECELERATING, ACT_HOLD_WALKING, ACT_HOLD_DECELERATING].contains(&m.action) {
            self.reset();
            return None;
        }
        let previous = self.clip.and_then(clip).map(|p| Playback {
            clip: self.clip,
            frame: (self.position % p.2).floor(),
            cycles: (self.position / p.2).floor(),
            finished: false,
        });
        let choice = choose_clip(m, previous.as_ref());
        let Some(&(id, start_frame, end, f1, f2)) = clip(choice.id) else {
            self.reset();
            return None;
        };
        if self.clip != Some(id) {
            self.clip = Some(id);
            self.position = start_frame;
            return None;
        }
        let start = self.position;
        self.position += choice.speed;
        let pos = self.position;
        let crossed = |frame: f64| ((start - frame) / end).floor() < ((pos - frame) / end).floor();
        if crossed(f1) || crossed(f2) {
            return Some(if id == 0x92 { Step::Tiptoe } else { Step::Step });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_emits_steps_at_the_two_frames() {
        let mut f = Footsteps::default();
        let m = AnimInput { action: ACT_WALKING, forward_vel: 12.0, intended_mag: 32.0, ..Default::default() };
        let mut steps = 0;
        for _ in 0..200 {
            if f.step(&m).is_some() {
                steps += 1;
            }
        }
        // 200 ticks at speed 3 = 600 frames of a ~100 frame loop with 2 steps per loop.
        assert!((8..=60).contains(&steps), "{steps}");
        assert_eq!(f.step(&AnimInput { action: ACT_IDLE, ..Default::default() }), None);
    }
}
