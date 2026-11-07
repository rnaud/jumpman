//! Port of `gohma.ts` (Queen Gohma, the Deku Tree boss) as an engine-free state machine. The
//! caller owns the models: it reads `boss_pos`/`boss_rot_*`/`heart_*`/`portal_scale`/`clip` and
//! feeds the eye joint's world position (`eye`) back in for the seed target.
use super::save::ZeldaSave;
use super::slingshot::SeedTarget;
use jumpman_sim::math::Vec3;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Crawl,
    Drop,
    Stunned,
    Attack,
    Rise,
    Dying,
    Cleared,
}

#[derive(Default, Debug)]
pub struct GohmaOutput {
    pub prompt: Option<&'static str>,
    pub notify: Vec<&'static str>,
    pub return_to_forest: bool,
}

pub struct Gohma {
    pub phase: Phase,
    pub ticks: u32,
    last_hit: u32,
    pub health: i32,
    pub vulnerable: bool,
    pub boss_home: Vec3,
    pub heart_home: Vec3,
    pub floor: f64,
    pub boss_pos: Vec3,
    pub boss_rot_y: f64,
    pub boss_rot_z: f64,
    pub boss_visible: bool,
    pub heart_pos: Vec3,
    pub heart_rot_y: f64,
    pub heart_visible: bool,
    pub portal_pos: Vec3,
    pub portal_scale: f64,
    pub portal_visible: bool,
    pub glint_visible: bool,
    /// World position of the eye joint (set by the view each frame).
    pub eye: Vec3,
    /// Animation clip to (re)start, taken by the view.
    pub clip: Option<&'static str>,
}

const PI: f64 = std::f64::consts::PI;

impl Gohma {
    pub fn new(boss_home: Vec3, heart_home: Vec3, floor: f64) -> Self {
        Self {
            phase: Phase::Crawl,
            ticks: 0,
            last_hit: 0,
            health: 3,
            vulnerable: false,
            boss_home,
            heart_home,
            floor,
            boss_pos: boss_home,
            boss_rot_y: 0.0,
            boss_rot_z: 0.0,
            boss_visible: true,
            heart_pos: heart_home,
            heart_rot_y: 0.0,
            heart_visible: false,
            portal_pos: [heart_home[0] + 480.0, floor + 65.0, heart_home[2]],
            portal_scale: 1.0,
            portal_visible: false,
            glint_visible: false,
            eye: boss_home,
            clip: None,
        }
    }

    fn transition(&mut self, phase: Phase, clip: &'static str) {
        self.phase = phase;
        self.ticks = 0;
        self.clip = Some(clip);
    }

    pub fn restore(&mut self, save: &ZeldaSave) {
        self.health = 3;
        self.last_hit = 0;
        self.vulnerable = false;
        let won = save.deku_cleared("gohma");
        self.transition(if won { Phase::Cleared } else { Phase::Crawl }, "Idle");
        self.boss_visible = !won;
        self.glint_visible = false;
        self.heart_visible = won && !save.deku_cleared("heart");
        self.portal_visible = won;
        self.boss_pos = self.boss_home;
    }

    /// The red eye is a seed target only while exposed (`id` 0).
    pub fn targets(&self) -> Vec<SeedTarget> {
        if self.vulnerable { vec![SeedTarget { center: self.eye, radius: 100.0, id: 0 }] } else { vec![] }
    }

    /// A seed reached the eye.
    pub fn seed_hit(&mut self) {
        if !self.vulnerable {
            return;
        }
        self.vulnerable = false;
        self.glint_visible = false;
        self.transition(Phase::Drop, "Stunned");
    }

    pub fn step(&mut self, m: &mut MarioRuntime, c: &ControllerSnapshot, save: &mut ZeldaSave, aiming: bool, link: bool) -> GohmaOutput {
        self.ticks += 1;
        let mut out = GohmaOutput::default();
        let dist = |a: Vec3, b: Vec3| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        let t = self.ticks;
        if self.phase == Phase::Cleared {
            self.portal_scale = 1.0 + 0.07 * (t as f64 * 0.08).sin();
            self.heart_rot_y += 0.045;
            self.heart_pos[1] = self.floor + 92.0 + 16.0 * (t as f64 * 0.08).sin();
            if self.heart_visible && dist(m.pos, self.heart_pos) < 240.0 {
                save.clear_deku_challenge("heart");
                self.heart_visible = false;
                m.max_health = 0x980 as f64;
                m.health = m.max_health;
                out.notify.push("Heart Container! Your maximum health increased.");
            }
            if dist(m.pos, self.portal_pos) < 290.0 {
                out.prompt = Some("SPACE / A - ENTER THE BLUE WARP");
                if c.button_pressed & A_BUTTON != 0 {
                    out.return_to_forest = true;
                }
            }
            return out;
        }
        match self.phase {
            Phase::Crawl => {
                self.boss_rot_z = PI;
                self.boss_pos[0] = self.boss_home[0] + 360.0 * (t as f64 * 0.012).sin();
                self.boss_pos[2] = self.boss_home[2] + 180.0 * (t as f64 * 0.012).cos();
                self.boss_rot_y = (m.pos[0] - self.boss_pos[0]).atan2(m.pos[2] - self.boss_pos[2]);
                self.vulnerable = t >= 45;
                self.glint_visible = self.vulnerable;
                out.prompt = Some(if self.vulnerable { "QUEEN GOHMA - SHOOT THE RED EYE" } else { "QUEEN GOHMA - WATCH THE CEILING" });
                if save.deku_seeds() == 0 && !aiming {
                    out.prompt = Some("SPACE / A - GATHER DEKU SEEDS");
                    if c.button_pressed & A_BUTTON != 0 {
                        save.refill_seeds();
                    }
                }
                if t >= 240 {
                    self.glint_visible = false;
                    self.vulnerable = false;
                    self.transition(Phase::Attack, "Attack");
                }
            }
            Phase::Drop | Phase::Attack => {
                self.boss_pos[1] = self.floor.max(self.boss_pos[1] - 66.0);
                self.boss_rot_z = (self.boss_rot_z - 0.14).max(0.0);
                if self.phase == Phase::Attack {
                    let (dx, dz) = (m.pos[0] - self.boss_pos[0], m.pos[2] - self.boss_pos[2]);
                    let len = dx.hypot(dz);
                    if len > 0.0 {
                        self.boss_pos[0] += dx / len * 12.0;
                        self.boss_pos[2] += dz / len * 12.0;
                    }
                    if t == 45 && dist(m.pos, self.boss_pos) < 550.0 {
                        m.take_damage(2.0, Some(self.boss_pos));
                    }
                    if t >= 65 {
                        self.transition(Phase::Rise, "Idle");
                    }
                } else if self.boss_pos[1] == self.floor {
                    self.last_hit = 0;
                    self.transition(Phase::Stunned, "Stunned");
                }
            }
            Phase::Stunned => {
                out.prompt = Some(if link { "GOHMA IS STUNNED - J / B TO SLASH" } else { "GOHMA IS STUNNED - J / B TO PUNCH" });
                let heading = (self.boss_pos[0] - m.pos[0]).atan2(self.boss_pos[2] - m.pos[2]);
                let facing = (heading - m.face_angle[1] * PI / 32768.0).cos() > 0.35;
                if !aiming && facing && c.button_pressed & B_BUTTON != 0 && dist(m.pos, self.boss_pos) < 460.0 && t - self.last_hit >= 12 {
                    self.last_hit = t;
                    self.health -= 1;
                    if self.health == 0 {
                        self.transition(Phase::Dying, "Death");
                        out.notify.push("Queen Gohma defeated!");
                    }
                }
                if self.phase == Phase::Stunned && t > 180 {
                    self.transition(Phase::Rise, "Idle");
                }
            }
            Phase::Rise => {
                for i in 0..3 {
                    self.boss_pos[i] += (self.boss_home[i] - self.boss_pos[i]) * 0.08;
                }
                self.boss_rot_z = (self.boss_rot_z + 0.09).min(PI);
                if t >= 45 {
                    self.transition(Phase::Crawl, "Idle");
                }
            }
            Phase::Dying => {
                if t >= 70 {
                    save.clear_deku_challenge("gohma");
                    self.restore(save);
                }
            }
            Phase::Cleared => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::test_support::mario;

    fn boss() -> Gohma {
        Gohma::new([0.0, 600.0, 0.0], [100.0, 0.0, 0.0], 0.0)
    }

    #[test]
    fn crawl_exposes_the_eye_after_45_ticks_and_seed_stuns() {
        let mut g = boss();
        let mut m = mario();
        let mut save = ZeldaSave::default();
        save.open_treasure_chest("deku_tree:1", 0x05);
        let c = ControllerSnapshot::default();
        for _ in 0..44 {
            g.step(&mut m, &c, &mut save, false, false);
        }
        assert!(g.targets().is_empty());
        let out = g.step(&mut m, &c, &mut save, false, false);
        assert!(g.vulnerable && out.prompt.unwrap().contains("RED EYE"));
        g.seed_hit();
        assert_eq!(g.phase, Phase::Drop);
        // drops 66 per tick to the floor, then stunned
        for _ in 0..12 {
            g.step(&mut m, &c, &mut save, false, false);
        }
        assert_eq!(g.phase, Phase::Stunned);
        assert_eq!(g.boss_pos[1], 0.0);
    }

    #[test]
    fn three_punches_defeat_gohma_and_open_the_portal() {
        let mut g = boss();
        let mut m = mario();
        let mut save = ZeldaSave::default();
        g.phase = Phase::Stunned;
        g.boss_pos = [0.0, 0.0, 100.0];
        // Mario at origin faces +z (yaw 0), boss is ahead.
        let punch = ControllerSnapshot { button_pressed: B_BUTTON, ..Default::default() };
        let idle = ControllerSnapshot::default();
        let mut notes = vec![];
        for tick in 0..60 {
            let c = if tick % 13 == 0 { &punch } else { &idle };
            notes.extend(g.step(&mut m, c, &mut save, false, true).notify);
            if g.phase == Phase::Dying {
                break;
            }
        }
        assert_eq!(g.phase, Phase::Dying);
        assert_eq!(notes, vec!["Queen Gohma defeated!"]);
        for _ in 0..70 {
            g.step(&mut m, &idle, &mut save, false, true);
        }
        assert!(save.deku_cleared("gohma"));
        assert_eq!(g.phase, Phase::Cleared);
        assert!(g.heart_visible && g.portal_visible);
    }

    #[test]
    fn heart_container_and_blue_warp() {
        let mut g = boss();
        let mut save = ZeldaSave::default();
        save.clear_deku_challenge("gohma");
        g.restore(&save);
        assert_eq!(g.phase, Phase::Cleared);
        let mut m = mario();
        m.pos = [100.0, 92.0, 0.0];
        let a = ControllerSnapshot { button_pressed: A_BUTTON, ..Default::default() };
        let out = g.step(&mut m, &a, &mut save, false, false);
        assert!(save.deku_cleared("heart") && !g.heart_visible);
        assert_eq!(m.max_health, 0x980 as f64);
        assert_eq!(out.notify.len(), 1);
        m.pos = [580.0, 65.0, 0.0];
        assert!(g.step(&mut m, &a, &mut save, false, false).return_to_forest);
    }
}
