//! Diorama (`diorama.ts`): the real course shrunk (by distance and a narrow lens, not by scaling
//! physics) into a plastic compact case. Real Marios live on it; you fly around and poke them.
//!
//! Not ported: the tilt-shift post-process, Goombas (`goomba.ts` has no Rust owner yet), the
//! rounded/star-trimmed case and mouse look/poke. `W`/`S` tilt, `A`/`D` turn, `Q`/`E` zoom,
//! `Space` pokes the Mario nearest the middle of the case, `T` cycles a trick.
use super::crowd::init_clips;
use super::mario_crowd::*;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::state::ControllerSnapshot;

const CASE_PINK: [f32; 3] = [1.0, 0.525, 0.706];
const CASE_TRIM: [f32; 3] = [0.54, 0.85, 0.91];
const HALF: f64 = 8400.0;

pub struct DioramaExperiment {
    crowd: Option<MarioCrowd>,
    yaw: f64,
    pitch: f64,
    distance: f64,
    center: DVec3,
    pokes: u32,
}

/// Camera position for an orbit around `center`.
pub fn orbit_eye(center: DVec3, yaw: f64, pitch: f64, distance: f64) -> DVec3 {
    center + DVec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()) * distance
}

impl DioramaExperiment {
    pub fn new() -> Self {
        Self { crowd: None, yaw: 0.6, pitch: -0.55, distance: 32000.0, center: DVec3::new(0.0, 400.0, 0.0), pokes: 0 }
    }

    /// Knock back the Mario nearest the middle of the case.
    fn poke(&mut self, host: &mut Host) {
        let Some(crowd) = self.crowd.as_mut() else { return };
        let c = self.center;
        let nearest = crowd.agents.iter().enumerate().min_by(|(_, a), (_, b)| {
            let d = |x: &CrowdAgent| (x.body.pos()[0] - c.x).hypot(x.body.pos()[2] - c.z);
            d(a).partial_cmp(&d(b)).unwrap()
        });
        let Some((i, _)) = nearest else { return };
        let pos = crowd.agents[i].body.pos();
        if crowd.agents[i].body.runtime.take_damage(2.0, Some([pos[0] - 80.0, pos[1], pos[2]])) {
            self.pokes += 1;
            host.toast("POKE!", 0.8);
        }
    }
}

impl Experiment for DioramaExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Diorama
    }
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }
    fn freezes_player(&self) -> bool {
        true
    }

    fn enter(&mut self, host: &mut Host) {
        host.card(
            "05",
            "Diorama",
            "The whole battlefield as a toy in a plastic case. Real Marios live on it; fly around and poke them.",
            &[("A D", "turn"), ("W S", "tilt"), ("Q E", "zoom"), ("Space", "poke a Mario"), ("X", "leave")],
        );
        // A pink base plate and rim around the course.
        let pink = host.lit(Color::srgb(CASE_PINK[0], CASE_PINK[1], CASE_PINK[2]));
        let trim = host.lit(Color::srgb(CASE_TRIM[0], CASE_TRIM[1], CASE_TRIM[2]));
        host.spawn_box([0.0, -450.0, 0.0], [HALF * 2.0 + 800.0, 300.0, HALF * 2.0 + 800.0], 0.0, pink.clone());
        for (x, z, w, d) in [(0.0, HALF + 200.0, HALF * 2.0 + 800.0, 400.0), (0.0, -HALF - 200.0, HALF * 2.0 + 800.0, 400.0), (HALF + 200.0, 0.0, 400.0, HALF * 2.0), (-HALF - 200.0, 0.0, 400.0, HALF * 2.0)] {
            host.spawn_box([x, -100.0, z], [w, 500.0, d], 0.0, trim.clone());
        }
        let meta = init_clips(host);
        let opts = MarioCrowdOptions {
            world: host.course_world(),
            route: Some(jumpman_sim::behaviors::koopa_the_quick::KOOPA_RACE_ROUTE.to_vec()),
            has_leader: false,
            coin_count: 30,
            center: [0.0; 3],
            radius: 6800.0,
            seed: 0xd10a,
            single_lap: false,
        };
        let mut crowd = MarioCrowd::new(opts, meta, Some(&mut *host));
        crowd.populate(60, [0.0, 0.0, 0.0], 3000.0, Some(&mut *host));
        self.crowd = Some(crowd);
        host.set_clear(Color::srgb(0.93, 0.85, 0.95));
        host.cam.far_scale = 4.0;
    }

    fn exit(&mut self, host: &mut Host) {
        if let Some(mut c) = self.crowd.take() {
            c.dispose(host);
        }
        host.cam.pose = None;
        host.cam.far_scale = 1.0;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if key == KeyCode::Space {
            self.poke(host);
            return true;
        }
        false
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        // The player is frozen (skip), but swallow the input so jumps do not queue up.
        StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None }
    }

    fn after_step(&mut self, host: &mut Host) {
        if let Some(c) = self.crowd.as_mut() {
            c.update(None);
        }
        let _ = host;
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        let held = |k: KeyCode| host.keys.pressed(k) as i32 as f64;
        self.yaw += (held(KeyCode::KeyD) - held(KeyCode::KeyA)) * dt * 1.2;
        self.pitch = (self.pitch + (held(KeyCode::KeyS) - held(KeyCode::KeyW)) * dt * 0.9).clamp(-1.45, -0.08);
        self.distance = (self.distance * (1.0 + (held(KeyCode::KeyE) - held(KeyCode::KeyQ)) * dt * 1.2)).clamp(3000.0, 70000.0);
        // A narrow lens from far away flattens perspective into a miniature.
        let eye = orbit_eye(self.center, self.yaw, self.pitch, self.distance);
        host.cam.pose = Some(CamPose { eye, look: self.center, up: DVec3::Y, fov: 28.0, near: 200.0, far: 120000.0 });
        let alpha = host.alpha;
        if let Some(crowd) = self.crowd.as_mut() {
            crowd.render(host, alpha, dt, [eye.x, eye.y, eye.z]);
            let s = crowd.stats;
            host.status(format!("{} Marios   coins {}   knockbacks {}   pokes {}", crowd.agents.len(), s.coins, s.hits, self.pokes));
        }
    }

    fn debug(&self) -> String {
        format!("agents={} pokes={} yaw={:.2} pitch={:.2} distance={:.0}", self.crowd.as_ref().map_or(0, |c| c.agents.len()), self.pokes, self.yaw, self.pitch, self.distance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_keeps_its_distance_and_looks_down_from_above() {
        let c = DVec3::new(10.0, 400.0, -20.0);
        for (yaw, pitch) in [(0.0, -0.5), (1.3, -1.0), (-2.0, -0.2)] {
            let eye = orbit_eye(c, yaw, pitch, 17000.0);
            assert!((eye.distance(c) - 17000.0).abs() < 1e-6);
            assert!(eye.y > c.y, "negative pitch means a high camera");
        }
    }
}
