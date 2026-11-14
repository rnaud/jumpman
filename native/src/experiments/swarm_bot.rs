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
    /// Ticks spent on the ground well below the next waypoint (fell off a ramp or a ledge).
    below: u32,
    /// Set when the route no longer leads anywhere from here: the caller plans a fresh one from the player's position.
    pub lost: bool,
    /// The caller sets this when the player hangs from a ceiling (a jump under a hangable underside): the walker lets go.
    pub hanging: bool,
}

impl Nav {
    pub fn new(route: Vec<[f32; 3]>) -> Self {
        Self { route, idx: 0, last: [f64::NAN; 3], still: 0, ticks: 0, below: 0, lost: false, hanging: false }
    }

    /// Start over on a fresh route (after [`Nav::lost`]).
    pub fn replan(&mut self, route: Vec<[f32; 3]>) {
        *self = Self::new(route);
    }

    pub fn done(&self) -> bool {
        self.idx + 1 >= self.route.len()
    }

    /// The controller and camera yaw for this step, given the player's position and whether he is airborne.
    pub fn step(&mut self, pos: [f64; 3], air: bool) -> (ControllerSnapshot, i32) {
        self.ticks += 1;
        if self.hanging {
            // Let go (A released, Z pressed) and plan again from wherever the drop lands.
            self.lost = true;
            let c = ControllerSnapshot { button_pressed: Z_TRIG, button_down: Z_TRIG, ..Default::default() };
            return (c, 0);
        }
        let hd = |p: &[f32; 3]| (p[0] as f64 - pos[0]).hypot(p[2] as f64 - pos[2]);
        while self.idx + 1 < self.route.len() {
            let p = self.route[self.idx];
            let q = self.route[self.idx + 1];
            // A waypoint counts as passed close up only (a grid cell is 100 wide): passing it 100 units off cuts the corner
            // of whatever the route bends round (the Graveyard's fence end stopped the walker dead).
            if (hd(&p) < 60.0 && (p[1] as f64 - pos[1]).abs() < 160.0) || (hd(&q) < 45.0 && (q[1] as f64 - pos[1]).abs() < 160.0) {
                self.idx += 1;
            } else {
                break;
            }
        }
        // A zigzag in the grid route can leave the next waypoint behind: skip ahead to a later one that is already closer.
        let n = self.route.len();
        if self.idx + 1 < n {
            let next = hd(&self.route[self.idx + 1]);
            if let Some(k) = (self.idx + 2..(self.idx + 6).min(n)).filter(|&k| hd(&self.route[k]) + 40.0 < next && (self.route[k][1] as f64 - pos[1]).abs() < 120.0).last() {
                self.idx = k - 1;
            }
        }
        let tgt = self.route[(self.idx + 1).min(n - 1)];
        let aim = self.route[(self.idx + 2).min(n - 1)];
        let dir = if hd(&tgt) < 40.0 { (aim[0] as f64 - pos[0], aim[2] as f64 - pos[2]) } else { (tgt[0] as f64 - pos[0], tgt[2] as f64 - pos[2]) };
        let rig_yaw = (-dir.0).atan2(-dir.1);
        let cam = ((rig_yaw / std::f64::consts::TAU) * 65536.0).round() as i64 as i16 as i32;
        let moved = if self.last[0].is_nan() { 100.0 } else { (pos[0] - self.last[0]).hypot(pos[2] - self.last[2]) };
        self.last = pos;
        self.still = if moved < 4.0 { self.still + 1 } else { 0 };
        // Fell off the route (standing far under the next waypoint) or stuck: ask for a new route from here.
        self.below = if !air && tgt[1] as f64 - pos[1] > 220.0 { self.below + 1 } else { 0 };
        if self.below > 45 || self.still > 120 {
            self.lost = true;
            self.below = 0;
            self.still = 0;
        }
        // Jump for a ledge (a steep rise), not on a ramp: a jump on a narrow ramp throws the walker off it (or up into a
        // hangable ceiling under it).
        let steep = |p: &[f32; 3], over: f64| { let dy = p[1] as f64 - pos[1]; dy > over && dy > 0.6 * hd(p) };
        let climb = steep(&tgt, 45.0) || steep(&aim, 90.0) || self.still > 20;
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
