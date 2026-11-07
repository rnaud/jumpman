//! Port of `src/game/goomba.ts` plus the Goomba block of `Game.stepWorld`: walking goombas that
//! live outside the object manager, and their stomp / strike / hurt rules against Mario.
use bevy::math::DVec3;
use jumpman_sim::characters::sword_reach;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::js_math::js_cos;
use jumpman_sim::js_math::js_sin;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::math::*;
use jumpman_sim::object::mario_hitbox;
use jumpman_sim::state::*;
use jumpman_sim::surfaces::SURFACE_DEATH_PLANE;

use super::course_data::Goomba as GoombaSpawn;

pub const GOOMBA_MODEL: &str = "actors/goomba/goomba_geo.glb";
pub const GOOMBA_SCALE: f64 = 1.5;
pub const RADIUS: f64 = 72.0 * 1.5;

pub struct WalkingGoomba {
    pub pos: DVec3,
    pub home: DVec3,
    /// s16 yaw (the model's rotation.y is `angle_to_radians` of this).
    pub yaw: f64,
    pub visible: bool,
    pub dead: bool,
    /// Model scale multiplier on top of [`GOOMBA_SCALE`] (x, y, z).
    pub squash: DVec3,
    squash_frame: i32,
    vy: f64,
    target: f64,
    speed: f64,
    relative_speed: f64,
    walk_timer: f64,
    airborne: bool,
    awake: bool,
    spawner: Option<[f64; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GoombaEvent {
    /// Loot coin released at a position (stomp finished or struck down).
    Coin([f64; 3]),
    Sparks([f64; 3]),
    Dust([f64; 3]),
    Stomp,
    Hurt,
}

impl WalkingGoomba {
    pub fn new(pos: [f64; 3], spawner: Option<[f64; 3]>) -> Self {
        let mut g = Self {
            pos: DVec3::from(pos),
            home: DVec3::from(pos),
            yaw: 0.0,
            visible: true,
            dead: false,
            squash: DVec3::ONE,
            squash_frame: -1,
            vy: 0.0,
            target: 0.0,
            speed: 0.0,
            relative_speed: 0.0,
            walk_timer: 0.0,
            airborne: false,
            awake: false,
            spawner,
        };
        g.reset();
        g
    }

    pub fn top(&self) -> f64 {
        self.pos.y + 50.0 * 1.5
    }

    pub fn tangible(&self) -> bool {
        self.visible && !self.dead
    }

    pub fn reset(&mut self) {
        self.dead = false;
        self.vy = 0.0;
        self.squash_frame = -1;
        self.squash = DVec3::ONE;
        self.visible = true;
        self.pos = self.home;
        self.awake = self.spawner.is_none();
        self.yaw = 0.0;
        self.target = 0.0;
        self.speed = 0.0;
        self.relative_speed = 0.0;
        self.walk_timer = 0.0;
        self.airborne = false;
    }

    pub fn defeat(&mut self, stomp: bool) {
        self.dead = true;
        if stomp {
            self.squash_frame = 0;
        } else {
            self.visible = false;
        }
    }

    pub fn alert(&mut self, x: f64, z: f64) {
        self.target = atan2s(z - self.pos.z, x - self.pos.x) as f64;
        self.airborne = true;
        self.speed = 0.0;
        self.vy = 25.0;
    }

    /// Returns true on the single tick that finishes a stomp and releases its coin.
    pub fn step(&mut self, mario: &[f64; 3], world: &CollisionWorld) -> bool {
        if !self.dead {
            self.walk(mario, world);
        }
        if self.squash_frame < 0 {
            return false;
        }
        let frame = self.squash_frame;
        self.squash_frame += 1;
        let ground = world.find_floor(self.pos.x, self.pos.y, self.pos.z).height;
        self.vy = (-78.0f64).max(self.vy - 4.0);
        self.pos.y = ground.max(self.pos.y + self.vy);
        if self.pos.y == ground {
            self.vy = 0.0;
        }
        let height = (0.3f64).max(1.0 - (frame + 1) as f64 * 0.14);
        let width = if height == 0.3 { 1.7 } else { 1.0 };
        self.squash = DVec3::new(width, height, width);
        if frame <= 15 {
            return false;
        }
        self.visible = false;
        self.squash_frame = -1;
        true
    }

    fn walk(&mut self, mario: &[f64; 3], world: &CollisionWorld) {
        if let Some(sp) = self.spawner {
            let range = js_hypot3(mario[0] - sp[0], mario[1] - sp[1], mario[2] - sp[2]);
            if range > 4000.0 && self.awake {
                self.reset();
            }
            if range < 3000.0 {
                self.awake = true;
            }
            if !self.awake {
                return;
            }
        }
        let p = self.pos;
        let homing = js_hypot(p.x - self.home.x, p.z - self.home.z) > 1000.0;
        let nearby = !homing && js_hypot3(p.x - mario[0], p.y - mario[1], p.z - mario[2]) < 500.0;
        if !self.airborne {
            self.speed = approach_f32(self.speed, self.relative_speed * 1.5, 0.4, 0.4);
            if nearby {
                if self.relative_speed <= 2.0 {
                    self.alert(mario[0], mario[2]);
                }
                self.target = atan2s(mario[2] - p.z, mario[0] - p.x) as f64;
                self.relative_speed = 20.0;
            } else {
                self.relative_speed = 4.0 / 3.0;
                if homing {
                    self.target = atan2s(self.home.z - p.z, self.home.x - p.x) as f64;
                } else {
                    self.walk_timer -= 1.0;
                    if self.walk_timer <= 0.0 {
                        self.walk_timer = 100.0 + (random() * 100.0).floor();
                        let hop = random() < 0.25;
                        if hop {
                            self.alert(p.x, p.z);
                        }
                        let sign = if random() < 0.5 { 1.0 } else { -1.0 };
                        self.target = to_s16(self.yaw + sign * if hop { 0x6000 as f64 } else { 0x2000 as f64 }) as f64;
                    }
                }
            }
        }
        let turn = if self.airborne { 0x800 as f64 } else { 0x200 as f64 };
        self.yaw = to_s16(self.yaw + clamp(to_s16(self.target - self.yaw) as f64, -turn, turn)) as f64;
        let p = self.pos;
        let mut next = [p.x + sins(self.yaw) * self.speed, p.y, p.z + coss(self.yaw) * self.speed];
        let wall = world.resolve_and_return_wall_collisions(&mut next, 30.0, 40.0);
        let floor = world.find_floor(next[0], p.y + 30.0, next[2]);
        let blocked = wall.is_some()
            || match &floor.surface {
                None => true,
                Some(s) => s.kind == SURFACE_DEATH_PLANE || floor.height < p.y - 120.0 || s.normal[1] < (78.0f64 * std::f64::consts::PI / 180.0).cos(),
            };
        if blocked {
            self.target = to_s16(self.yaw + 0x8000 as f64) as f64;
            self.speed = 0.0;
        } else {
            self.pos.x = next[0];
            self.pos.z = next[2];
        }
        let ground = world.find_floor(self.pos.x, self.pos.y + 30.0, self.pos.z).height;
        self.vy = (-78.0f64).max(self.vy - 4.0);
        self.pos.y = ground.max(self.pos.y + self.vy);
        if self.pos.y <= ground {
            self.vy = 0.0;
            self.airborne = false;
        }
    }
}

/// `touchesGoombaHurtbox`: regular Goombas have separate attack and damage cylinders.
pub fn touches_hurtbox(feet: &[f64; 3], mario_height: f64, enemy: &WalkingGoomba) -> bool {
    let p = enemy.pos;
    js_hypot(feet[0] - p.x, feet[2] - p.z) < 42.0 * 1.5 && feet[1] <= p.y + 40.0 * 1.5 && feet[1] + mario_height >= p.y
}

/// `crossesGoomba`: test the feet at the instant they cross the enemy's head during a physics tick.
pub fn crosses(previous: &[f64; 3], current: &[f64; 3], enemy: &WalkingGoomba) -> bool {
    let top = enemy.top();
    let drop = previous[1] - current[1];
    if drop <= 0.0 || previous[1] < top || current[1] > top {
        return false;
    }
    let t = (previous[1] - top) / drop;
    let x = previous[0] + t * (current[0] - previous[0]);
    let z = previous[2] + t * (current[2] - previous[2]);
    js_hypot(x - enemy.pos.x, z - enemy.pos.z) < RADIUS + 37.0
}

/// Spawn positions for a course's goomba spots (`prepareArea`): triplets form a ring of three.
pub fn spawn_goombas(spots: &[GoombaSpawn], world: &CollisionWorld) -> Vec<WalkingGoomba> {
    let mut out = vec![];
    for spawn in spots {
        for i in 0..if spawn.triplet { 3 } else { 1 } {
            let angle = i as f64 * 0x5555 as f64 * std::f64::consts::PI / 32768.0;
            let x = spawn.pos[0] + if spawn.triplet { (500.0 * js_cos(angle)).trunc() } else { 0.0 };
            let z = spawn.pos[2] + if spawn.triplet { (500.0 * js_sin(angle)).trunc() } else { 0.0 };
            let floor = world.find_floor(x, spawn.pos[1] + 200.0, z);
            if floor.surface.is_none() {
                continue;
            }
            out.push(WalkingGoomba::new([x, floor.height, z], spawn.triplet.then_some(spawn.pos)));
        }
    }
    out
}

/// The post-step Goomba block of `Game.stepWorld`: stomps, strikes and hurts. At most one
/// stomp or hurt per tick (the TS `break`s).
pub fn interact(goombas: &mut [WalkingGoomba], m: &mut MarioRuntime, prev_pos: &[f64; 3]) -> Vec<GoombaEvent> {
    let mut events = vec![];
    if m.events.iter().any(|e| matches!(e, MarioEvent::Respawn)) {
        return events;
    }
    let height = mario_hitbox(m).height;
    for enemy in goombas.iter_mut() {
        if !enemy.tangible() {
            continue;
        }
        let descending = m.action & ACT_FLAG_AIR != 0
            && m.vel[1] < 0.0
            && m.pos[1] > enemy.pos.y
            && m.pos[1] <= enemy.top()
            && js_hypot(m.pos[0] - enemy.pos.x, m.pos[2] - enemy.pos.z) < RADIUS + 37.0;
        if !descending && !crosses(prev_pos, &m.pos, enemy) {
            let [x, y, z] = m.pos;
            let foot = enemy.pos;
            if y <= enemy.top() && y + height >= foot.y && js_hypot(x - foot.x, z - foot.z) < RADIUS + 37.0 + sword_reach(m) {
                let angle = atan2s(foot.z - z, foot.x - x) as f64;
                let difference = (to_s16(angle - m.face_angle[1]) as f64).abs();
                let strike = m.flags & (MARIO_PUNCHING | MARIO_KICKING | MARIO_TRIPPING) != 0;
                if (strike && difference <= 0x2aaa as f64) || (m.action & ACT_FLAG_ATTACKING != 0 && m.forward_vel.abs() >= 26.0) {
                    events.push(GoombaEvent::Coin(foot.to_array()));
                    enemy.defeat(false);
                    events.push(GoombaEvent::Sparks(foot.to_array()));
                    continue;
                }
                if m.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE) != 0 || m.invinc_timer != 0.0 || !touches_hurtbox(&m.pos, height, enemy) {
                    continue;
                }
                let hurt = m.take_damage(1.0, Some(foot.to_array()));
                enemy.alert(x, z);
                if hurt {
                    events.push(GoombaEvent::Hurt);
                    break;
                }
            }
            continue;
        }
        enemy.defeat(true);
        if m.action != ACT_GROUND_POUND && m.action != ACT_GROUND_POUND_LAND {
            m.pos[1] = enemy.top();
            if m.action & ACT_FLAG_AIR == 0 {
                m.set_action(ACT_FREEFALL, 0.0);
            }
            m.vel[1] = 30.0;
            m.flags &= !MARIO_UNKNOWN_08;
        }
        events.push(GoombaEvent::Stomp);
        events.push(GoombaEvent::Dust(enemy.pos.to_array()));
        break;
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;
    use jumpman_sim::mario_runtime::SpawnPoint;
    use std::sync::Arc;

    fn flat_world() -> CollisionWorld {
        let mut w = CollisionWorld::default();
        w.add_triangle([-3000.0, 0.0, -3000.0], [3000.0, 0.0, 3000.0], [3000.0, 0.0, -3000.0], &SurfaceOptions::default());
        w.add_triangle([-3000.0, 0.0, -3000.0], [-3000.0, 0.0, 3000.0], [3000.0, 0.0, 3000.0], &SurfaceOptions::default());
        w
    }

    #[test]
    fn spawns_one_goomba_or_a_triplet_on_the_floor() {
        let w = flat_world();
        let spots = [GoombaSpawn { pos: [0.0, 100.0, 0.0], triplet: false }, GoombaSpawn { pos: [1000.0, 100.0, 0.0], triplet: true }];
        let g = spawn_goombas(&spots, &w);
        assert_eq!(g.len(), 4);
        assert!(g.iter().all(|g| g.pos.y == 0.0));
        assert!(g[1].spawner.is_some());
    }

    #[test]
    fn goomba_chases_nearby_mario_and_stomp_releases_coin() {
        let w = flat_world();
        let mut g = WalkingGoomba::new([0.0, 0.0, 0.0], None);
        for _ in 0..30 {
            g.step(&[300.0, 0.0, 0.0], &w);
        }
        assert!(g.pos.x > 5.0, "goomba moved toward mario: {:?}", g.pos);
        g.defeat(true);
        let mut coin = false;
        for _ in 0..20 {
            coin |= g.step(&[300.0, 0.0, 0.0], &w);
        }
        assert!(coin);
        assert!(!g.visible);
    }

    #[test]
    fn mario_landing_on_goomba_stomps_it_and_bounces() {
        let w = Arc::new(flat_world());
        let mut m = MarioRuntime::new(w, SpawnPoint { action: None, pos: [0.0, 60.0, 0.0], face_yaw: 0.0 }, -9000.0);
        let mut goombas = vec![WalkingGoomba::new([0.0, 0.0, 0.0], None)];
        m.pos = [0.0, 60.0, 0.0];
        m.vel = [0.0, -20.0, 0.0];
        m.set_action(ACT_FREEFALL, 0.0);
        let prev = [0.0, 120.0, 0.0];
        let events = interact(&mut goombas, &mut m, &prev);
        assert!(events.contains(&GoombaEvent::Stomp));
        assert!(goombas[0].dead);
        assert_eq!(m.vel[1], 30.0);
        assert_eq!(m.pos[1], goombas[0].top());
    }
}
