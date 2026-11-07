//! Diorama (`diorama.ts`): the real course shrunk (by distance and a narrow lens, not by scaling
//! physics) into a plastic compact case. Real Marios live on it; you fly around and poke them.
//!
//! Mouse: drag looks around, wheel zooms, click pokes a Mario (knockback), a Goomba (it charges
//! the nearest Mario) or drops a coin every Mario races for. The course's Goombas and objects
//! run their real behaviours around the Mario you poked last. `T` toggles the miniature lens
//! (bevy depth-of-field standing in for the TS tilt-shift band). `W`/`S` tilt, `A`/`D` turn,
//! `Q`/`E` zoom, `Space` shakes the case, `R` resets the view. Not ported: the rounded case.
use super::crowd::init_clips;
use super::mario_crowd::*;
use super::dock::DockItem;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::game::goomba::{crosses, touches_hurtbox};
use bevy::math::DVec3 as V;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::{ACT_FLAG_AIR, ACT_FREEFALL};
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
    lens: bool,
    drag: Option<(Vec2, f32)>,
    goombas: Vec<GoombaState>,
}

#[derive(Default, Clone, Copy)]
struct GoombaState {
    chase: u32,
    target: Option<u32>,
    respawn: u32,
}

/// Camera position for an orbit around `center`.
pub fn orbit_eye(center: DVec3, yaw: f64, pitch: f64, distance: f64) -> DVec3 {
    center + DVec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()) * distance
}

impl DioramaExperiment {
    pub fn new() -> Self {
        Self { crowd: None, yaw: 0.6, pitch: -0.55, distance: 32000.0, center: DVec3::new(0.0, 400.0, 0.0), pokes: 0, lens: true, drag: None, goombas: vec![] }
    }

    fn nearest_mario(&self, p: V) -> Option<(u32, [f64; 3])> {
        let crowd = self.crowd.as_ref()?;
        crowd
            .agents
            .iter()
            .map(|a| (a.id, a.body.pos()))
            .min_by(|x, y| (x.1[0] - p.x).hypot(x.1[2] - p.z).partial_cmp(&(y.1[0] - p.x).hypot(y.1[2] - p.z)).unwrap())
    }

    /// Every Mario hops at once, like the toy was bumped.
    fn shake(&mut self, host: &mut Host) {
        if let Some(crowd) = self.crowd.as_mut() {
            for a in &mut crowd.agents {
                let m = &mut a.body.runtime;
                if m.action & ACT_FLAG_AIR == 0 {
                    m.set_action(ACT_FREEFALL, 0.0);
                    m.vel[1] = 58.0;
                }
            }
        }
        for g in host.goombas.0.iter_mut().filter(|g| g.tangible()) {
            let p = g.pos;
            g.alert(p.x, p.z);
        }
        host.toast("SHAKE!", 0.8);
    }

    /// Click: poke a Mario, wake a Goomba, or drop a coin on the ground.
    fn poke(&mut self, host: &mut Host, cursor: Vec2) {
        let mouse = host.mouse;
        let alpha = host.alpha;
        let Some(crowd) = self.crowd.as_mut() else { return };
        if let Some(id) = crowd.pick(cursor, 40.0, alpha, |p| mouse.project(p)) {
            if let Some(a) = crowd.agents.iter_mut().find(|a| a.id == id) {
                let pos = a.body.pos();
                a.body.runtime.take_damage(2.0, Some([pos[0] - 80.0, pos[1], pos[2]]));
                host.toast(&format!("{} - {}", a.personality.info().label.to_uppercase(), a.label.to_uppercase()), 1.2);
            }
            crowd.selected = Some(id);
            self.pokes += 1;
            return;
        }
        let Some((o, d)) = mouse.ray() else { return };
        let mut best: Option<(f64, usize)> = None;
        for (i, g) in host.goombas.0.iter().enumerate().filter(|(_, g)| g.tangible()) {
            let oc = o - (g.pos + V::Y * 50.0);
            let b = oc.dot(d);
            let disc = b * b - (oc.dot(oc) - 110.0 * 110.0);
            if disc >= 0.0 {
                let t = -b - disc.sqrt();
                if best.map_or(true, |x| t < x.0) {
                    best = Some((t, i));
                }
            }
        }
        if let Some((_, i)) = best {
            let p = host.goombas.0[i].pos;
            let target = self.nearest_mario(p).map(|n| n.0);
            self.goombas[i].chase = 30 * 9;
            self.goombas[i].target = target;
            host.goombas.0[i].alert(p.x, p.z);
            self.pokes += 1;
            host.toast("GOOMBA - CHASE!", 1.2);
            return;
        }
        if let Some(t) = crate::camera::rig::camera_ray(crowd.world(), o, d, 400000.0) {
            let hit = o + d * t;
            if crowd.drop_coin(host, [hit.x, hit.y, hit.z]) {
                self.pokes += 1;
                host.toast("A COIN FALLS FROM THE SKY", 1.2);
            }
        }
    }

    fn step_goombas(&mut self, host: &mut Host) {
        let Some(crowd) = self.crowd.as_mut() else { return };
        let world = crowd.world().clone();
        for i in 0..host.goombas.0.len().min(self.goombas.len()) {
            let st = &mut self.goombas[i];
            let g = &mut host.goombas.0[i];
            let p = g.pos;
            if !g.tangible() {
                g.step(&[p.x, p.y, p.z], &world);
                st.respawn += 1;
                if st.respawn > 30 * 12 {
                    st.respawn = 0;
                    g.reset();
                }
                continue;
            }
            let nearest = crowd.agents.iter().map(|a| (a.id, a.body.pos())).min_by(|x, y| (x.1[0] - p.x).hypot(x.1[2] - p.z).partial_cmp(&(y.1[0] - p.x).hypot(y.1[2] - p.z)).unwrap());
            let target_pos = st.target.and_then(|id| crowd.agents.iter().find(|a| a.id == id)).map(|a| a.body.pos());
            match (st.chase > 0, target_pos) {
                (true, Some(t)) => {
                    st.chase -= 1;
                    // Charging: a straight run at the chosen Mario across the whole board.
                    let yaw = jumpman_sim::math::atan2s(t[2] - p.z, t[0] - p.x) as f64;
                    let r = angle_to_radians(yaw);
                    let next = [p.x + r.sin() * 13.0, p.y, p.z + r.cos() * 13.0];
                    let floor = world.find_floor(next[0], p.y + 120.0, next[2]);
                    if floor.surface.is_some() && floor.height > p.y - 400.0 {
                        g.pos = V::new(next[0], floor.height, next[2]);
                    } else {
                        st.chase = 0;
                    }
                    g.yaw = yaw;
                    if st.chase == 0 {
                        g.home = g.pos;
                    }
                }
                _ => {
                    st.chase = 0;
                    let aim = nearest.map_or([p.x, p.y + 5000.0, p.z], |n| n.1);
                    g.step(&aim, &world);
                }
            }
            for a in crowd.agents.iter_mut() {
                let prev = a.body.prev_pos;
                let m = &mut a.body.runtime;
                if m.vel[1] < 0.0 && crosses(&prev, &m.pos, g) {
                    g.defeat(true);
                    m.set_action(ACT_FREEFALL, 0.0);
                    m.vel[1] = 42.0;
                    if st.target == Some(a.id) {
                        st.chase = 0;
                    }
                    break;
                }
                if touches_hurtbox(&m.pos, 160.0, g) && m.take_damage(1.0, Some([g.pos.x, g.pos.y, g.pos.z])) && st.target == Some(a.id) {
                    st.chase = st.chase.min(20);
                }
            }
        }
    }

    /// The course's own objects (Chain Chomp, Bob-ombs, Koopas...) run their real behaviours
    /// around the Mario you poked last, as if he were the player.
    fn step_course_objects(&mut self, host: &mut Host) {
        let Some(crowd) = self.crowd.as_mut() else { return };
        let focus = crowd.selected.and_then(|id| crowd.agents.iter().position(|a| a.id == id)).unwrap_or(0);
        let Some(agent) = crowd.agents.get_mut(focus) else { return };
        let m = &mut agent.body.runtime;
        std::mem::swap(&mut m.objects, &mut host.sim.mario.objects);
        jumpman_sim::object_manager::step(m, None, false);
        jumpman_sim::object_manager::after_mario_step(m);
        m.objects.events.clear();
        std::mem::swap(&mut m.objects, &mut host.sim.mario.objects);
        // Crowd Marios never die, whatever the objects do to them.
        m.health = 0x880 as f64;
        m.hurt_counter = 0.0;
        // Feed the object views (the player's own tick is skipped here).
        let sim = &mut *host.sim;
        sim.prev = std::mem::take(&mut sim.curr);
        sim.curr = sim.snapshot();
        if sim.prev.objects.len() < sim.curr.objects.len() {
            let have = sim.prev.objects.len();
            let fresh = sim.curr.objects[have..].to_vec();
            sim.prev.objects.extend(fresh);
        }
    }

    fn dock(&self, host: &mut Host) {
        let mut lens = DockItem::new("Miniature lens", KeyCode::KeyT, 0);
        lens.selected = self.lens;
        host.set_dock(vec![DockItem::new("Reset view", KeyCode::KeyR, 0), lens, DockItem::new("Shake the case", KeyCode::Space, 0)]);
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
            &[("Drag", "look around"), ("Wheel", "zoom"), ("Click", "poke a Mario or Goomba, or drop a coin"), ("T", "miniature lens"), ("Space", "shake the case"), ("X", "leave")],
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
        host.cam.far_scale = 40.0;
        host.shell.goombas_driven = true;
        self.goombas = vec![GoombaState::default(); host.goombas.0.len()];
        for g in &mut host.goombas.0 {
            g.reset();
        }
        self.dock(host);
    }

    fn exit(&mut self, host: &mut Host) {
        if let Some(mut c) = self.crowd.take() {
            c.dispose(host);
        }
        host.cam.pose = None;
        host.cam.far_scale = 1.0;
        host.cam.dof = None;
        host.shell.goombas_driven = false;
        for g in &mut host.goombas.0 {
            g.reset();
        }
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        match key {
            KeyCode::Space => self.shake(host),
            KeyCode::KeyT => {
                self.lens = !self.lens;
                host.toast(if self.lens { "MINIATURE LENS ON" } else { "MINIATURE LENS OFF" }, 1.2);
            }
            KeyCode::KeyR => {
                self.yaw = 0.6;
                self.pitch = -0.55;
                self.distance = 32000.0;
            }
            _ => return false,
        }
        self.dock(host);
        true
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        // The player is frozen (skip), but swallow the input so jumps do not queue up.
        StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None }
    }

    fn after_step(&mut self, host: &mut Host) {
        if let Some(c) = self.crowd.as_mut() {
            c.update(None);
            let fx = c.take_fx();
            host.play_fx(fx);
        }
        self.step_goombas(host);
        self.step_course_objects(host);
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        let mouse = host.mouse;
        if mouse.left_just_pressed && !mouse.over_ui {
            self.drag = mouse.cursor.map(|c| (c, 0.0));
        }
        if let (Some((last, moved)), Some(cursor)) = (self.drag, mouse.cursor) {
            let delta = cursor - last;
            let moved = moved + delta.x.abs() + delta.y.abs();
            if moved > 4.0 {
                self.yaw -= delta.x as f64 * 0.0042;
                self.pitch = (self.pitch + delta.y as f64 * 0.0036).clamp(-1.45, -0.08);
            }
            self.drag = Some((cursor, moved));
            if mouse.left_just_released {
                self.drag = None;
                if moved <= 4.0 {
                    self.poke(host, cursor);
                }
            }
        }
        if !mouse.left {
            self.drag = None;
        }
        if mouse.scroll != 0.0 {
            self.distance = (self.distance * (1.0 - mouse.scroll as f64 * 0.08)).clamp(3000.0, 70000.0);
        }
        let held = |k: KeyCode| host.keys.pressed(k) as i32 as f64;
        self.yaw += (held(KeyCode::KeyD) - held(KeyCode::KeyA)) * dt * 1.2;
        self.pitch = (self.pitch + (held(KeyCode::KeyS) - held(KeyCode::KeyW)) * dt * 0.9).clamp(-1.45, -0.08);
        self.distance = (self.distance * (1.0 + (held(KeyCode::KeyE) - held(KeyCode::KeyQ)) * dt * 1.2)).clamp(3000.0, 70000.0);
        // A narrow lens from far away flattens perspective into a miniature.
        let eye = orbit_eye(self.center, self.yaw, self.pitch, self.distance);
        host.cam.pose = Some(CamPose { eye, look: self.center, up: DVec3::Y, fov: 28.0, near: 200.0, far: 120000.0 });
        host.cam.dof = self.lens.then(|| (self.distance, ((self.distance - 1500.0) / 14000.0).clamp(0.0, 1.0)));
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
