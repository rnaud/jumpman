//! Port of `kokiriSwordQuest.ts`: the Kokiri Sword chest reward, Mido blocking and then
//! releasing the path to the Deku Tree, and the Deku Tree's mouth opening. Engine-free; the
//! caller applies `mido_pos`/`mido_rot_y`, the sword fields and `passage_blocked` to the scene
//! and collision (`passage_quads` gives the two wall windings).
use super::save::ZeldaSave;
use jumpman_sim::math::Vec3;

pub struct QuestData {
    /// Mido's route; first = home, last = destination.
    pub path: Vec<Vec3>,
    pub chest: Vec3,
    pub mouth_closed: Vec3,
    pub mouth_opened: Vec3,
    pub mouth_eye: Vec3,
    pub mouth_look_from: Vec3,
    pub mouth_look_to: Vec3,
}

pub struct KokiriSwordQuest {
    data: QuestData,
    opened: bool,
    permitted: bool,
    reward_time: i32,
    pub walking: bool,
    pub passage_blocked: bool,
    pub mido_pos: Vec3,
    pub mido_rot_y: f64,
    pub sword_visible: bool,
    pub sword_pos: Vec3,
    pub sword_rot_y: f64,
    /// 0 closed .. 1 open; negative until first positioned.
    pub mouth_phase: f64,
    /// Chest-opening animation: `Some(speed)` when it should start this tick.
    pub start_opening: bool,
    /// Where the player can talk to Mido (`talk` out-parameter in the TS).
    pub talk: Vec3,
}

impl KokiriSwordQuest {
    pub fn new(data: QuestData) -> Self {
        let home = data.path[0];
        Self {
            sword_pos: data.chest,
            mido_pos: home,
            talk: home,
            data,
            opened: false,
            permitted: false,
            reward_time: 0,
            walking: false,
            passage_blocked: false,
            mido_rot_y: 0.0,
            sword_visible: false,
            sword_rot_y: 0.0,
            mouth_phase: -1.0,
            start_opening: false,
        }
    }

    fn home(&self) -> Vec3 {
        self.data.path[0]
    }
    fn destination(&self) -> Vec3 {
        *self.data.path.last().unwrap()
    }

    pub fn mouth_open(&self) -> bool {
        self.mouth_phase == 1.0
    }
    pub fn mouth_opening(&self) -> bool {
        self.mouth_phase > 0.0 && self.mouth_phase < 1.0
    }

    /// Current mouth position (the view moves the model and rebuilds its collision when this changes).
    pub fn mouth_position(&self) -> Vec3 {
        let (c, o) = (self.data.mouth_closed, self.data.mouth_opened);
        [0, 1, 2].map(|i| c[i] + (o[i] - c[i]) * self.mouth_phase.max(0.0))
    }

    /// `frameOpening`: (eye, look-at) while the mouth opens.
    pub fn opening_camera(&self) -> (Vec3, Vec3) {
        let (a, b) = (self.data.mouth_look_from, self.data.mouth_look_to);
        let t = self.mouth_phase;
        (self.data.mouth_eye, [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t))
    }

    /// The neck between the two rock walls at Mido's home; both windings are needed by the
    /// one-sided wall solver.
    pub fn passage_quads(&self) -> [[Vec3; 4]; 2] {
        let [x, y, z] = self.home();
        let hw = 440.0;
        let (a, b) = ([x, y - 190.0, z - hw], [x, y - 190.0, z + hw]);
        let (c, d) = ([x, y + 1350.0, z + hw], [x, y + 1350.0, z - hw]);
        [[a, b, c, d], [b, a, d, c]]
    }

    pub fn restore(&mut self, save: &ZeldaSave) {
        self.mouth_phase = if save.deku_path_open && save.has_spoken_to("dekuTreeAccepted") { 1.0 } else { 0.0 };
        self.opened = save.sword;
        self.permitted = save.deku_path_open;
        self.walking = false;
        self.reward_time = 0;
        self.sword_visible = false;
        self.mido_pos = if self.permitted { self.destination() } else { self.home() };
        self.talk = self.mido_pos;
        self.passage_blocked = !self.permitted;
    }

    pub fn step(&mut self, save: &ZeldaSave, player: Vec3, talking: bool) {
        self.start_opening = false;
        if save.deku_path_open && save.has_spoken_to("dekuTreeAccepted") && !self.mouth_open() {
            // OoT advances 0.01 per 20 Hz gameplay frame; this simulation runs at 30 Hz.
            self.mouth_phase = (self.mouth_phase.max(0.0) + 1.0 / 150.0).min(1.0);
        }
        if save.sword && !self.opened {
            self.opened = true;
            self.start_opening = true;
            self.reward_time = 90;
        }
        if self.reward_time > 0 {
            let elapsed = 90 - self.reward_time;
            self.reward_time -= 1;
            self.sword_visible = elapsed >= 24 && self.reward_time > 0;
            self.sword_pos[1] = self.data.chest[1] + 245.0f64.min(95.0 + elapsed as f64 * 2.6);
            self.sword_rot_y = elapsed as f64 * 0.065;
        }
        if save.deku_path_open && !self.permitted {
            self.permitted = true;
            self.walking = true;
            self.passage_blocked = false;
        }
        let home = self.home();
        if self.walking {
            let dest = self.destination();
            let delta = [0, 1, 2].map(|i| dest[i] - self.mido_pos[i]);
            let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            self.mido_rot_y = delta[0].atan2(delta[2]);
            if distance <= 6.6 {
                self.mido_pos = dest;
                self.walking = false;
            } else {
                for i in 0..3 {
                    self.mido_pos[i] += delta[i] * 6.6 / distance;
                }
            }
        } else if !self.permitted && !talking {
            // EnMd_BlockPath intercepts Link on a 60 OoT-unit radius.
            let angle = (player[0] - home[0]).atan2(player[2] - home[2]);
            self.mido_pos = [home[0] + 264.0 * angle.sin(), home[1], home[2] + 264.0 * angle.cos()];
        }
        if !self.walking {
            self.mido_rot_y = (player[0] - self.mido_pos[0]).atan2(player[2] - self.mido_pos[2]);
        }
        self.talk = self.mido_pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quest() -> KokiriSwordQuest {
        KokiriSwordQuest::new(QuestData {
            path: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 66.0]],
            chest: [10.0, 20.0, 30.0],
            mouth_closed: [0.0, 0.0, 0.0],
            mouth_opened: [0.0, 100.0, 0.0],
            mouth_eye: [1.0, 2.0, 3.0],
            mouth_look_from: [0.0, 0.0, 0.0],
            mouth_look_to: [10.0, 0.0, 0.0],
        })
    }

    #[test]
    fn mido_blocks_then_walks_away_when_the_path_opens() {
        let mut q = quest();
        let mut save = ZeldaSave::default();
        q.restore(&save);
        assert!(q.passage_blocked);
        q.step(&save, [500.0, 0.0, 0.0], false);
        // intercepts on the 264-unit circle toward Link
        assert!((q.mido_pos[0] - 264.0).abs() < 1e-9);
        save.sword = true;
        save.shield = true;
        save.open_deku_path();
        q.step(&save, [500.0, 0.0, 0.0], false);
        assert!(q.walking && !q.passage_blocked && q.start_opening);
        for _ in 0..100 {
            q.step(&save, [500.0, 0.0, 0.0], false);
        }
        assert!(!q.walking);
        assert_eq!(q.mido_pos, [0.0, 0.0, 66.0]);
    }

    #[test]
    fn reward_sword_rises_then_vanishes() {
        let mut q = quest();
        let mut save = ZeldaSave::default();
        q.restore(&save);
        save.sword = true;
        let mut shown = 0;
        for _ in 0..95 {
            q.step(&save, [500.0, 0.0, 0.0], false);
            shown += q.sword_visible as i32;
        }
        assert!(shown > 50 && !q.sword_visible);
        assert!(q.sword_pos[1] <= 20.0 + 245.0);
    }

    #[test]
    fn mouth_opens_over_150_ticks() {
        let mut q = quest();
        let mut save = ZeldaSave::default();
        save.sword = true;
        save.deku_path_open = true;
        save.finish_conversation("dekuTreeAccepted");
        q.restore(&save);
        assert!(q.mouth_open(), "restored open");
        let mut fresh = quest();
        save.spoken.clear();
        fresh.restore(&save);
        assert_eq!(fresh.mouth_phase, 0.0);
        save.finish_conversation("dekuTreeAccepted");
        for _ in 0..149 {
            fresh.step(&save, [500.0, 0.0, 0.0], false);
        }
        assert!(fresh.mouth_opening());
        fresh.step(&save, [500.0, 0.0, 0.0], false);
        assert!(fresh.mouth_open());
        assert_eq!(fresh.mouth_position(), [0.0, 100.0, 0.0]);
    }
}
