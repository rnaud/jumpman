//! Port of `fairySlingshot.ts`: first-person aiming and the seed's flight. Scene glue (the
//! slingshot model, the seed mesh, first-person camera) reads the public fields; the ray against
//! scenery is supplied by the caller (`terrain`).
use super::save::ZeldaSave;
use jumpman_sim::math::Vec3;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;

#[derive(Clone, Debug)]
pub struct SeedTarget {
    pub center: Vec3,
    pub radius: f64,
    /// Handed back from [`FairySlingshot::step`] when the seed lands.
    pub id: usize,
}

#[derive(Default)]
pub struct FairySlingshot {
    pub aiming: bool,
    pub yaw: f64,
    pub pitch: f64,
    pub origin: Vec3,
    pub direction: Vec3,
    pub seed_pos: Vec3,
    pub seed_visible: bool,
    velocity: Vec3,
    remaining: f64,
    impact: Option<usize>,
}

#[derive(Debug, Default, PartialEq)]
pub struct SlingshotStep {
    /// Aiming took over the controls this tick.
    pub handled: bool,
    /// A seed arrived at a target.
    pub hit: Option<usize>,
}

/// `THREE.Ray.intersectSphere`: nearest forward hit, or the far one when starting inside.
pub fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, radius: f64) -> Option<f64> {
    let oc = [center[0] - origin[0], center[1] - origin[1], center[2] - origin[2]];
    let tca = oc[0] * dir[0] + oc[1] * dir[1] + oc[2] * dir[2];
    let d2 = oc.iter().map(|v| v * v).sum::<f64>() - tca * tca;
    let r2 = radius * radius;
    if d2 > r2 {
        return None;
    }
    let thc = (r2 - d2).sqrt();
    let (t0, t1) = (tca - thc, tca + thc);
    if t1 < 0.0 {
        return None;
    }
    Some(if t0 < 0.0 { t1 } else { t0 })
}

impl FairySlingshot {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `terrain(origin, dir, far)` returns the distance to the nearest scenery hit.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        m: &mut MarioRuntime,
        c: &ControllerSnapshot,
        aim: bool,
        save: &mut ZeldaSave,
        targets: &[SeedTarget],
        terrain: &dyn Fn(Vec3, Vec3, f64) -> Option<f64>,
        toast: &mut Vec<String>,
    ) -> SlingshotStep {
        let mut out = SlingshotStep::default();
        if self.remaining > 0.0 {
            let travel = self.remaining.min(180.0);
            for i in 0..3 {
                self.seed_pos[i] += self.velocity[i] * travel;
            }
            self.remaining -= travel;
            if self.remaining == 0.0 {
                self.seed_visible = false;
                out.hit = self.impact.take();
            }
        }
        if !aim || !save.slingshot_equipped() || m.floor.is_none() || (m.pos[1] - m.floor_height).abs() > 35.0 {
            self.aiming = false;
            return out;
        }
        if !self.aiming {
            self.yaw = m.face_angle[1] * std::f64::consts::PI / 32768.0;
            self.pitch = 0.0;
        }
        self.aiming = true;
        self.yaw -= c.stick_x * 0.0007;
        self.pitch = (self.pitch + c.stick_y * 0.00055).clamp(-1.3, 1.35);
        self.direction = [self.yaw.sin() * self.pitch.cos(), self.pitch.sin(), self.yaw.cos() * self.pitch.cos()];
        self.origin = [m.pos[0], m.pos[1] + 130.0, m.pos[2]];
        m.face_angle[1] = (self.yaw * 32768.0 / std::f64::consts::PI).round();
        m.forward_vel = 0.0;
        m.vel = [0.0; 3];
        if m.action != ACT_IDLE {
            m.set_action(ACT_IDLE, 0.0);
        }
        out.handled = true;
        if self.remaining == 0.0 && c.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
            if !save.use_seed() {
                toast.push("Out of seeds. Refill at the slingshot chest.".into());
                return out;
            }
            let far = 7200.0;
            let mut distance = terrain(self.origin, self.direction, far).unwrap_or(far);
            self.impact = None;
            for t in targets {
                if let Some(reach) = ray_sphere(self.origin, self.direction, t.center, t.radius) {
                    if reach <= distance {
                        distance = reach;
                        self.impact = Some(t.id);
                    }
                }
            }
            self.remaining = distance.max(1.0);
            self.velocity = self.direction;
            self.seed_pos = self.origin;
            self.seed_visible = true;
        }
        out
    }

    /// World position of the slingshot model while aiming (`model.position`).
    pub fn model_position(&self) -> Vec3 {
        let mut p = [0.0; 3];
        for i in 0..3 {
            p[i] = self.origin[i] + self.direction[i] * 85.0;
        }
        p[0] -= self.yaw.cos() * 28.0;
        p[2] += self.yaw.sin() * 28.0;
        p[1] -= 25.0;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::test_support::mario;

    fn equipped() -> ZeldaSave {
        let mut s = ZeldaSave::default();
        s.open_treasure_chest("deku_tree:1", 0x05);
        s.set_slingshot_equipped(true);
        s
    }

    #[test]
    fn ray_sphere_cases() {
        assert!((ray_sphere([0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, 10.0], 2.0).unwrap() - 8.0).abs() < 1e-9);
        assert!(ray_sphere([0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, -10.0], 2.0).is_none());
        assert!(ray_sphere([0.0; 3], [0.0, 0.0, 1.0], [5.0, 0.0, 10.0], 2.0).is_none());
        // inside: far hit
        assert!((ray_sphere([0.0; 3], [0.0, 0.0, 1.0], [0.0; 3], 3.0).unwrap() - 3.0).abs() < 1e-9);
    }

    /// Grounded Mario (a floor-bound sim needs a world; fake it by poking the fields).
    fn grounded() -> MarioRuntime {
        let mut m = mario();
        let w = jumpman_sim::collision::CollisionWorld::default();
        let _ = w;
        m.floor = Some(std::sync::Arc::new(jumpman_sim::collision::Surface {
            object: None, room: None, kind: 0, force: 0, flags: 0, lower_y: 0.0, upper_y: 0.0,
            v1: [0.0; 3], v2: [0.0; 3], v3: [0.0; 3], normal: [0.0, 1.0, 0.0], origin_offset: 0.0,
        }));
        m
    }

    #[test]
    fn seed_flies_and_hits_target() {
        let mut m = grounded();
        let mut save = equipped();
        let mut sling = FairySlingshot::default();
        let mut toast = vec![];
        let targets = [SeedTarget { center: [0.0, 130.0, 900.0], radius: 50.0, id: 7 }];
        let fire = ControllerSnapshot { button_pressed: A_BUTTON, ..Default::default() };
        let terrain = |_: Vec3, _: Vec3, _: f64| None;
        let r = sling.step(&mut m, &fire, true, &mut save, &targets, &terrain, &mut toast);
        assert!(r.handled && sling.seed_visible);
        assert_eq!(save.deku_seeds(), 29);
        let idle = ControllerSnapshot::default();
        let mut hit = None;
        for _ in 0..10 {
            hit = sling.step(&mut m, &idle, true, &mut save, &targets, &terrain, &mut toast).hit.or(hit);
        }
        assert_eq!(hit, Some(7));
        assert!(!sling.seed_visible);
    }

    #[test]
    fn terrain_blocks_the_seed_and_empty_pouch_toasts() {
        let mut m = grounded();
        let mut save = equipped();
        let mut sling = FairySlingshot::default();
        let mut toast = vec![];
        let targets = [SeedTarget { center: [0.0, 130.0, 900.0], radius: 50.0, id: 1 }];
        let fire = ControllerSnapshot { button_pressed: B_BUTTON, ..Default::default() };
        let wall = |_: Vec3, _: Vec3, _: f64| Some(300.0);
        sling.step(&mut m, &fire, true, &mut save, &targets, &wall, &mut toast);
        let idle = ControllerSnapshot::default();
        let mut hit = None;
        for _ in 0..5 {
            hit = sling.step(&mut m, &idle, true, &mut save, &targets, &wall, &mut toast).hit.or(hit);
        }
        assert_eq!(hit, None);
        for _ in 0..40 {
            sling.remaining = 0.0;
            sling.step(&mut m, &fire, true, &mut save, &[], &wall, &mut toast);
        }
        assert!(toast.iter().any(|t| t.contains("Out of seeds")));
    }
}
