//! Shrink Ray (`shrink.ts`): Mario keeps SM64's exact physics while the whole course is
//! rescaled around him, so a blade of grass becomes a platform (Tiny) or the mountain becomes a
//! step (Giant). Collision is rebuilt for every size; the framework scales the course's display
//! entities (`Shell::course_xform`) and hides its objects, which stay put far away.
use super::{Experiment, ExperimentId, Host};
use bevy::prelude::*;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions, WaterBox};
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::math::Vec3 as V3;
use std::collections::HashMap;
use std::sync::Arc;

/// World scale factors: the world grows (Mario shrinks) or the world shrinks (Mario grows).
pub const SIZES: [(&str, &str, f64); 4] = [("tiny", "Tiny", 8.0), ("small", "Small", 3.0), ("normal", "Normal", 1.0), ("giant", "Giant", 0.25)];

/// Far from the original course, so its (hidden) objects never meet Mario.
pub const OFFSET: V3 = [400000.0, 0.0, 0.0];
const TRANSITION: f64 = 0.9;

/// Map a point from the original course into a world scaled by `k`.
pub fn to_scaled(p: V3, k: f64) -> V3 {
    [OFFSET[0] + p[0] * k, OFFSET[1] + p[1] * k, OFFSET[2] + p[2] * k]
}
pub fn from_scaled(p: V3, k: f64) -> V3 {
    [(p[0] - OFFSET[0]) / k, (p[1] - OFFSET[1]) / k, (p[2] - OFFSET[2]) / k]
}

/// The collision world of `source` rescaled by `k` around [`OFFSET`].
pub fn scaled_world(source: &CollisionWorld, k: f64) -> CollisionWorld {
    let mut world = CollisionWorld::new(1024.0, (source.horizontal_boundary * k).max(1024.0) + OFFSET[0] + 4096.0);
    for s in source.surfaces() {
        let options = SurfaceOptions { room: s.room, kind: s.kind, force: s.force, flags: s.flags, object: None };
        world.add_triangle(to_scaled(s.v1, k), to_scaled(s.v2, k), to_scaled(s.v3, k), &options);
    }
    world.water_boxes = source.water_boxes.iter().map(|WaterBox(id, x1, z1, x2, z2, y)| WaterBox(*id, OFFSET[0] + x1 * k, OFFSET[2] + z1 * k, OFFSET[0] + x2 * k, OFFSET[2] + z2 * k, OFFSET[1] + y * k)).collect();
    world.ceiling_limit = (20000.0f64).max(20000.0 * k + OFFSET[1]);
    world.slide_terrain = source.slide_terrain;
    world.snow_terrain = source.snow_terrain;
    world.finalize();
    world
}

struct Transition {
    from: f64,
    to: f64,
    t: f64,
    anchor: V3,
}

pub struct ShrinkRayExperiment {
    worlds: HashMap<u64, Arc<CollisionWorld>>,
    scale: f64,
    size: &'static str,
    transition: Option<Transition>,
    resizes: u32,
}

impl ShrinkRayExperiment {
    pub fn new() -> Self {
        Self { worlds: HashMap::new(), scale: 1.0, size: "normal", transition: None, resizes: 0 }
    }

    fn world_for(&mut self, host: &Host, k: f64) -> Arc<CollisionWorld> {
        self.worlds.entry(k.to_bits()).or_insert_with(|| Arc::new(scaled_world(&host.sim.course.world, k))).clone()
    }

    /// Put Mario (at original-course point `anchor`) into the world scaled by `k`.
    fn apply_world(&mut self, host: &mut Host, k: f64, anchor: V3) {
        let world = self.world_for(host, k);
        let mut spawn: SpawnPoint = host.sim.course.spawn.clone();
        spawn.pos = to_scaled(spawn.pos, k);
        let death = OFFSET[1] + host.sim.course.death_height * k;
        host.set_mario_world(world, spawn, death);
        let yaw = host.sim.mario.face_angle[1];
        host.place_mario(to_scaled(anchor, k), Some(yaw));
        host.shell.course_xform = Some((k as f32, Vec3::new(OFFSET[0] as f32, OFFSET[1] as f32, OFFSET[2] as f32) * crate::render::SCALE));
        host.cam.far_scale = k.max(1.0);
        self.scale = k;
    }

    fn resize(&mut self, host: &mut Host, size: &'static str) {
        let target = SIZES.iter().find(|s| s.0 == size).unwrap().2;
        if target == self.scale || self.transition.is_some() {
            return;
        }
        self.size = size;
        self.transition = Some(Transition { from: self.scale, to: target, t: 0.0, anchor: from_scaled(host.sim.mario.pos, self.scale) });
        self.resizes += 1;
        host.toast(if target > 1.0 { "SHRINKING..." } else if target < 1.0 { "GROWING..." } else { "BACK TO NORMAL" }, 1.0);
    }

    fn status(&self, host: &mut Host) {
        let list: Vec<String> =
            SIZES.iter().enumerate().map(|(i, (id, label, w))| format!("{}{}:{} x{}", if *id == self.size { ">" } else { "" }, i + 1, label, if *w >= 1.0 { format!("{w}") } else { "1/4".into() })).collect();
        host.status(list.join("   "));
    }
}

impl ShrinkRayExperiment {
    fn dock(&self, host: &mut Host) {
        let keys = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
        let items = SIZES
            .iter()
            .enumerate()
            .map(|(i, (id, label, _))| {
                let mut it = super::dock::DockItem::new(&format!("{} {}", i + 1, label), keys[i], 0);
                it.selected = *id == self.size;
                it
            })
            .collect();
        host.set_dock(items);
    }
}

impl Experiment for ShrinkRayExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Shrink
    }
    fn time_scale(&self) -> f64 {
        if self.transition.is_some() {
            0.0
        } else {
            1.0
        }
    }

    fn enter(&mut self, host: &mut Host) {
        host.card(
            "07",
            "Shrink Ray",
            "Same Mario, same physics - a different world size. Tiny makes grass a jungle; Giant makes the mountain a step.",
            &[("1 2 3 4", "tiny / small / normal / giant"), ("WASD Space", "run / jump"), ("X", "leave")],
        );
        host.shell.plain_camera = true;
        let anchor = host.sim.mario.pos;
        self.apply_world(host, 1.0, anchor);
        self.dock(host);
        self.status(host);
        host.toast("PICK A SIZE - 1 TINY, 4 GIANT", 2.6);
    }

    fn exit(&mut self, host: &mut Host) {
        host.shell.course_xform = None;
        host.cam.far_scale = 1.0;
        // Return to the original course at the matching spot.
        let anchor = from_scaled(host.sim.mario.pos, self.scale);
        host.restore_mario_world();
        let yaw = host.sim.mario.face_angle[1];
        host.place_mario(anchor, Some(yaw));
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
        let Some(i) = digits.iter().position(|k| *k == key) else { return false };
        self.resize(host, SIZES[i].0);
        self.dock(host);
        self.status(host);
        true
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        let Some(tr) = self.transition.as_mut() else { return };
        tr.t = (tr.t + dt / TRANSITION).min(1.0);
        let e = tr.t * tr.t * (3.0 - 2.0 * tr.t);
        // Scale the course about Mario, so he stays put while the world swells or shrinks.
        let k = (tr.from.ln() + (tr.to.ln() - tr.from.ln()) * e).exp();
        let mario = host.sim.mario.pos;
        let offset = [mario[0] - tr.anchor[0] * k, mario[1] - tr.anchor[1] * k, mario[2] - tr.anchor[2] * k];
        host.shell.course_xform = Some((k as f32, Vec3::new(offset[0] as f32, offset[1] as f32, offset[2] as f32) * crate::render::SCALE));
        if tr.t >= 1.0 {
            let (to, anchor) = (tr.to, tr.anchor);
            self.transition = None;
            self.apply_world(host, to, anchor);
            self.status(host);
        }
    }

    fn debug(&self) -> String {
        format!("size={} scale={} transitioning={} resizes={} worlds={}", self.size, self.scale, self.transition.is_some(), self.resizes, self.worlds.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> CollisionWorld {
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        w.add_quad([-1000., 100., -1000.], [-1000., 100., 1000.], [1000., 100., 1000.], [1000., 100., -1000.], &o);
        w.finalize();
        w
    }

    #[test]
    fn scaling_round_trips() {
        let p = [123.0, 456.0, -789.0];
        for (_, _, k) in SIZES {
            let q = from_scaled(to_scaled(p, k), k);
            for i in 0..3 {
                assert!((q[i] - p[i]).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn rebuilt_collision_scales_floor_heights() {
        let source = world();
        for (_, _, k) in SIZES {
            let scaled = scaled_world(&source, k);
            let at = to_scaled([10.0, 500.0, 20.0], k);
            let floor = scaled.find_floor(at[0], at[1] * 1.0 + 100.0 * k, at[2]);
            assert!(floor.surface.is_some(), "k={k}");
            assert!((floor.height - 100.0 * k).abs() < 1e-6, "k={k} h={}", floor.height);
        }
    }

    #[test]
    fn mario_stands_on_the_scaled_world() {
        use jumpman_sim::mario_runtime::MarioRuntime;
        use jumpman_sim::state::ControllerSnapshot;
        let source = world();
        let k = 8.0;
        let w = Arc::new(scaled_world(&source, k));
        let start = to_scaled([0.0, 100.0, 0.0], k);
        let mut m = MarioRuntime::new(w, SpawnPoint { action: None, pos: start, face_yaw: 0.0 }, -1600.0 * k);
        for _ in 0..30 {
            m.step(&ControllerSnapshot::default(), 0.0, false, false);
        }
        assert!((m.pos[1] - 100.0 * k).abs() < 1.0, "y={}", m.pos[1]);
    }
}
