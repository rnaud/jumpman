//! Port of `src/game/wingSwitch.ts`: the Wing Cap switch (a squashable button with its own
//! collision). Engine-free.
use super::collider::Collider;
use super::course_data::CollisionData;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::mario_runtime::MarioRuntime;
use std::sync::Arc;

pub const SWITCH_TOP: &str = "actors/capswitch/cap_switch_geo.glb";
pub const SWITCH_BASE: &str = "actors/capswitch/cap_switch_base.glb";
const BASE_TAG: u32 = 0x6000_0000;
const TOP_TAG: u32 = 0x6000_0001;

fn scaled(shape: &CollisionData) -> CollisionData {
    CollisionData { vertices: shape.vertices.iter().map(|v| [v[0] / 2.0, v[1] / 2.0, v[2] / 2.0]).collect(), ..shape.clone() }
}

pub struct WingSwitch {
    /// Base position; the top sits 71 units above it.
    pub base_pos: [f64; 3],
    pub top_pos: [f64; 3],
    pub height: f64,
    collider: Collider,
    base: Collider,
    original_y: Vec<f64>,
    timer: i32,
}

impl WingSwitch {
    pub fn new(pos: [f64; 3], top: &CollisionData, base: &CollisionData) -> Self {
        let top = scaled(top);
        let original_y = top.vertices.iter().map(|v| v[1]).collect();
        Self {
            base_pos: pos,
            top_pos: [pos[0], pos[1] + 71.0, pos[2]],
            height: -1.0,
            collider: Collider::new(top, 0.0, TOP_TAG),
            base: Collider::new(scaled(base), 0.0, BASE_TAG),
            original_y,
            timer: -1,
        }
    }

    pub fn install(&mut self, world: &mut Arc<CollisionWorld>, pressed: bool) {
        let pos = self.base_pos;
        self.base.placed = false;
        self.base.place(world, Some(pos));
        self.reset(world, pressed);
    }

    fn resize(&mut self, world: &mut Arc<CollisionWorld>, value: f64) {
        if self.height == value {
            return;
        }
        self.height = value;
        for (v, y) in self.collider.data.vertices.iter_mut().zip(&self.original_y) {
            v[1] = y * value;
        }
        let pos = self.top_pos;
        self.collider.place(world, Some(pos));
    }

    pub fn reset(&mut self, world: &mut Arc<CollisionWorld>, pressed: bool) {
        self.timer = if pressed { 5 } else { -1 };
        self.resize(world, if pressed { 0.2 } else { 1.0 });
    }

    /// Returns true on the tick Mario presses the switch.
    pub fn step(&mut self, m: &mut MarioRuntime) -> bool {
        let pressed = self.timer < 0 && m.floor.as_ref().map_or(false, |f| f.object == Some(TOP_TAG)) && m.pos[1] <= m.floor_height + 1.0;
        if pressed {
            self.timer = 0;
        }
        if self.timer >= 0 && self.timer < 5 {
            let value = 1.0 - 0.2 * self.timer as f64;
            self.timer += 1;
            self.resize(&mut m.world, value);
        }
        pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;
    use jumpman_sim::mario_runtime::SpawnPoint;
    use jumpman_sim::state::ControllerSnapshot;

    fn shape(top: f64) -> CollisionData {
        CollisionData {
            rooms: vec![],
            vertices: vec![[-200.0, top, -200.0], [200.0, top, -200.0], [200.0, top, 200.0], [-200.0, top, 200.0]],
            triangles: vec![[0, 2, 1, 0, 0], [0, 3, 2, 0, 0]],
        }
    }

    #[test]
    fn standing_on_the_switch_presses_and_squashes_it() {
        let mut ground = CollisionWorld::default();
        ground.add_triangle([-5000.0, -500.0, -5000.0], [5000.0, -500.0, 5000.0], [5000.0, -500.0, -5000.0], &SurfaceOptions::default());
        ground.add_triangle([-5000.0, -500.0, -5000.0], [-5000.0, -500.0, 5000.0], [5000.0, -500.0, 5000.0], &SurfaceOptions::default());
        let mut m = MarioRuntime::new(Arc::new(ground), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
        let mut sw = WingSwitch::new([0.0, 0.0, 0.0], &shape(100.0), &shape(40.0));
        sw.install(&mut m.world, false);
        // Top surface is at 71 + 100/2 * 1.0.
        let top = m.world.find_floor(0.0, 500.0, 0.0).height;
        assert!((top - 121.0).abs() < 1e-9, "{top}");
        m.pos = [0.0, top, 0.0];
        let idle = ControllerSnapshot::default();
        let mut pressed = false;
        for _ in 0..10 {
            m.step(&idle, 0.0, false, false);
            pressed |= sw.step(&mut m);
        }
        assert!(pressed);
        assert!((sw.height - 0.2).abs() < 1e-9);
    }
}
