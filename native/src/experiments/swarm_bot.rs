//! A scripted walker for Star Swarm's chests (a test and screenshot tool, not part of the game): it follows a route of
//! course positions (from `swarm_world::route`) by pressing the stick toward the next waypoint, jumping up ledges and when
//! it stops making progress. Used by the headless chest-walking test (which steps the real simulation) and by
//! `JUMPMAN_SWARM_BOT=1` runs.
use jumpman_sim::state::*;

pub struct Nav {
    pub route: Vec<[f32; 3]>,
    pub idx: usize,
    last: [f64; 3],
    still: u32,
    ticks: u32,
}

impl Nav {
    pub fn new(route: Vec<[f32; 3]>) -> Self {
        Self { route, idx: 0, last: [f64::NAN; 3], still: 0, ticks: 0 }
    }

    pub fn done(&self) -> bool {
        self.idx + 1 >= self.route.len()
    }

    /// The controller and camera yaw for this step, given the player's position and whether he is airborne.
    pub fn step(&mut self, pos: [f64; 3], air: bool) -> (ControllerSnapshot, i32) {
        self.ticks += 1;
        let hd = |p: &[f32; 3]| (p[0] as f64 - pos[0]).hypot(p[2] as f64 - pos[2]);
        while self.idx + 1 < self.route.len() {
            let p = self.route[self.idx];
            let q = self.route[self.idx + 1];
            if (hd(&p) < 110.0 && (p[1] as f64 - pos[1]).abs() < 160.0) || (hd(&q) < 100.0 && (q[1] as f64 - pos[1]).abs() < 160.0) {
                self.idx += 1;
            } else {
                break;
            }
        }
        let n = self.route.len();
        let tgt = self.route[(self.idx + 1).min(n - 1)];
        let aim = self.route[(self.idx + 2).min(n - 1)];
        let dir = if hd(&tgt) < 60.0 { (aim[0] as f64 - pos[0], aim[2] as f64 - pos[2]) } else { (tgt[0] as f64 - pos[0], tgt[2] as f64 - pos[2]) };
        let rig_yaw = (-dir.0).atan2(-dir.1);
        let cam = ((rig_yaw / std::f64::consts::TAU) * 65536.0).round() as i64 as i16 as i32;
        let moved = if self.last[0].is_nan() { 100.0 } else { (pos[0] - self.last[0]).hypot(pos[2] - self.last[2]) };
        self.last = pos;
        self.still = if moved < 4.0 { self.still + 1 } else { 0 };
        if self.still > 150 && self.idx + 1 < n {
            self.idx += 1;
            self.still = 0;
        }
        let climb = tgt[1] as f64 - pos[1] > 45.0 || aim[1] as f64 - pos[1] > 90.0 || self.still > 20;
        let mut c = ControllerSnapshot { stick_x: 0.0, stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
        if climb && (!air || self.ticks % 2 == 0) {
            c.button_down |= A_BUTTON;
        }
        if climb && !air && self.ticks % 6 == 0 {
            c.button_pressed |= A_BUTTON;
        }
        (c, cam)
    }
}
