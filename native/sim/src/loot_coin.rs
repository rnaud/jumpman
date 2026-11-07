//! Port of `src/sim/lootCoin.ts`: bhvSingleCoinGetsSpawned, used by wooden posts, large boxes
//! and ordinary Goombas.
use crate::collision::CollisionWorld;
use crate::math::*;
use crate::surfaces::{SURFACE_BURNING, SURFACE_DEATH_PLANE};

fn apply_drag(speed: f64) -> f64 {
    let slowed = speed.abs() - speed * speed * 0.001;
    let sign = if speed > 0.0 { 1.0 } else if speed < 0.0 { -1.0 } else { speed };
    if slowed < 0.001 { 0.0 } else { sign * slowed }
}

#[derive(Clone, Debug)]
pub struct LootCoin {
    yaw: f64,
    speed: f64,
    vertical_speed: f64,
    bounce: f64,
    grounded: bool,
    age: f64,
    pub tangible: bool,
    pub visible: bool,
}

impl Default for LootCoin {
    fn default() -> Self { Self::new() }
}

impl LootCoin {
    /// Field initializers run in declaration order (yaw, speed, vertical speed).
    pub fn new() -> Self {
        let yaw = (random() * 65536.0).floor();
        let speed = random() * 10.0;
        let vertical_speed = 50.0 + random() * 10.0;
        Self { yaw, speed, vertical_speed, bounce: 0.7, grounded: false, age: 0.0, tangible: false, visible: true }
    }

    /// Returns false once the coin is gone.
    pub fn step(&mut self, pos: &mut Vec3, world: &CollisionWorld) -> bool {
        if let Some(wall) = world.resolve_and_return_wall_collisions(pos, 10.0, 30.0) {
            self.yaw = atan2s(wall.normal[2], wall.normal[0]) as f64;
        }
        let floor = world.find_floor(pos[0], pos[1], pos[2]);
        let vx = apply_drag(self.speed * sins(self.yaw));
        let vz = apply_drag(self.speed * coss(self.yaw));
        self.speed = js_hypot(vx, vz);
        let next = world.find_floor(pos[0] + vx, pos[1], pos[2] + vz);
        if let Some(ns) = &next.surface {
            if !(self.grounded && next.height < floor.height - 50.0)
                && (ns.normal[1] > 0.46947 || (next.height >= floor.height + 5.0 && pos[1] > next.height)) {
                pos[0] += vx;
                pos[2] += vz;
            }
        }
        self.vertical_speed = (-78.0f64).max(self.vertical_speed - 4.0);
        pos[1] += self.vertical_speed;
        if self.vertical_speed < 0.0 { self.tangible = true; }
        let on_floor = floor.surface.is_some() && pos[1] <= floor.height;
        if on_floor {
            let surface = floor.surface.as_ref().unwrap();
            if surface.kind == SURFACE_BURNING || surface.kind == SURFACE_DEATH_PLANE { return false; }
            pos[1] = floor.height;
            if self.vertical_speed < 0.0 { self.vertical_speed *= -self.bounce; }
            if self.grounded { self.bounce = 0.0; }
            let normal = surface.normal;
            if normal[1] < 0.9 {
                self.yaw = to_s16(self.yaw + clamp(to_s16(atan2s(normal[2], normal[0]) as f64 - self.yaw) as f64, -0x400 as f64, 0x400 as f64) as f64) as f64;
            }
        }
        self.grounded = on_floor;
        self.visible = self.age < 400.0 || self.age % 2.0 == 0.0;
        let alive = self.age < 443.0 && pos[1] > -11000.0;
        self.age += 1.0;
        alive
    }
}
