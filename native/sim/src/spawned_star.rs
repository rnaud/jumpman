//! Port of `src/sim/spawnedStar.ts`: bhv_spawned_star_loop and bhv_grand_star_loop at 30 Hz.
use crate::math::*;

/// Vertical arcs and horizontal travel of bhv_spawned_star_loop at 30 Hz.
#[derive(Clone, Debug)]
pub struct SpawnedStar {
    pub pos: Vec3,
    pub home_y: f64,
    velocity: f64,
    gravity: f64,
    dx: f64,
    dz: f64,
    phase: i32,
    spin: f64,
    pub yaw: f64,
    return_ticks: f64,
}

impl SpawnedStar {
    pub fn new(origin: Vec3, mario: Option<Vec3>) -> Self {
        // bhvSpawnedStar sets parameter 2 to 1; only the 100-coin star flies to Mario.
        let home_y = match mario { Some(m) => m[1] + 250.0, None => origin[1] };
        let pos = [origin[0], home_y, origin[2]];
        let (x, z) = match mario { Some(m) => (m[0] - origin[0], m[2] - origin[2]), None => (0.0, 0.0) };
        let speed = js_hypot(x, z) / 23.0;
        let yaw = atan2s(z, x) as f64;
        Self {
            pos, home_y, velocity: 50.0, gravity: -4.0,
            dx: sins(yaw) * speed, dz: coss(yaw) * speed,
            phase: 0, spin: 0x800 as f64, yaw: 0.0, return_ticks: 0.0,
        }
    }

    pub fn tangible(&self) -> bool { self.phase == 3 }
    pub fn returning(&self) -> bool { self.phase == 2 }
    pub fn bouncing(&self) -> bool { self.phase == 1 }

    pub fn step(&mut self) {
        if self.phase == 0 && self.velocity < 0.0 && self.pos[1] < self.home_y {
            self.dx = 0.0;
            self.dz = 0.0;
            self.velocity = 20.0;
            self.gravity = -1.0;
            self.phase += 1;
        } else if self.phase == 1 {
            self.velocity = (-4.0f64).max(self.velocity);
            if self.velocity < 0.0 && self.pos[1] < self.home_y {
                self.velocity = 0.0;
                self.gravity = 0.0;
                self.phase += 1;
            }
        } else if self.phase == 2 {
            self.return_ticks += 1.0;
            if self.return_ticks >= 15.0 { self.phase += 1; }
        } else if self.tangible() && self.spin > 0x400 as f64 {
            self.spin -= 0x40 as f64;
        }
        self.velocity += self.gravity;
        self.pos[0] += self.dx;
        self.pos[1] += self.velocity;
        self.pos[2] += self.dz;
        self.yaw = ((self.yaw + self.spin) as i64 & 0xffff) as f64;
    }
}

/// bhv_grand_star_loop waits, arcs to the center, then bounces before becoming tangible.
#[derive(Clone, Debug)]
pub struct GrandStar {
    pub pos: Vec3,
    ticks: f64,
    velocity: f64,
    bounced: bool,
    dx: f64,
    dz: f64,
    home_y: f64,
    pub tangible: bool,
}

impl GrandStar {
    pub fn new(origin: Vec3) -> Self {
        let speed = js_hypot(origin[0], origin[2]) / 79.0;
        let yaw = atan2s(-origin[2], -origin[0]) as f64;
        Self {
            pos: origin, ticks: 0.0, velocity: 80.0, bounced: false,
            dx: sins(yaw) * speed, dz: coss(yaw) * speed, home_y: origin[1], tangible: false,
        }
    }

    pub fn step(&mut self) {
        if self.tangible { return; }
        let t = self.ticks;
        self.ticks += 1.0;
        if t < 72.0 { return; }
        self.velocity -= 2.0;
        self.pos[1] += self.velocity;
        self.pos[0] += self.dx;
        self.pos[2] += self.dz;
        if !self.bounced && self.pos[1] < self.home_y {
            self.bounced = true;
            self.pos[1] = self.home_y;
            self.velocity = 60.0;
            self.dx = 0.0;
            self.dz = 0.0;
        } else if self.bounced && self.velocity < 0.0 && self.pos[1] < self.home_y + 200.0 {
            self.pos[1] = self.home_y + 200.0;
            self.tangible = true;
        }
    }
}
