//! Port of `wingSwitch.ts`: the Tower of the Wing Cap switch. A stepped-on switch squashes in 5
//! frames (the collider shrinks with it) and unlocks the wing boxes for good.
use super::data::V3;
use super::doors::DoorCollider;
use super::save::SaveFile;
use crate::game::course_data::CollisionData;
use jumpman_sim::collision::{CollisionWorld, Surface};
use std::collections::HashMap;
use std::sync::Arc;

pub const SWITCH_TOP: &str = "actors/capswitch/cap_switch_geo.glb";
pub const SWITCH_BASE: &str = "actors/capswitch/cap_switch_base.glb";

pub struct WingSwitch {
    /// Base position; the top sits 71 units above it.
    pub base_pos: V3,
    pub top_pos: V3,
    top: DoorCollider,
    original_y: Vec<f64>,
    /// Current height scale of the button (1 = unpressed, 0.2 = pressed).
    pub height: f64,
    /// -1 until pressed, then frames since.
    pub timer: i32,
    /// 0 wing (red), 1 metal (green), 2 vanish (blue): which cap boxes the switch opens.
    pub kind: i32,
}

fn halved(shape: &CollisionData) -> CollisionData {
    CollisionData { vertices: shape.vertices.iter().map(|v| [v[0] / 2.0, v[1] / 2.0, v[2] / 2.0]).collect(), ..shape.clone() }
}

impl WingSwitch {
    pub fn new(pos: V3, shapes: &HashMap<String, CollisionData>, world: &mut CollisionWorld, save: &SaveFile) -> Self {
        let top_pos = [pos[0], pos[1] + 71.0, pos[2]];
        let mut base = DoorCollider::new(halved(&shapes["switchBase"]), 0.0);
        base.move_to(world, Some(pos));
        let data = halved(&shapes["switchTop"]);
        let original_y = data.vertices.iter().map(|v| v[1]).collect();
        let mut s = Self { base_pos: pos, top_pos, top: DoorCollider::new(data, 0.0), original_y, height: -1.0, timer: -1, kind: 0 };
        s.reset(world, save.wing_switch_on());
        s
    }

    fn resize(&mut self, world: &mut CollisionWorld, value: f64) {
        if self.height == value {
            return;
        }
        self.height = value;
        for (v, y) in self.top.data.vertices.iter_mut().zip(&self.original_y) {
            v[1] = y * value;
        }
        let pos = self.top_pos;
        self.top.move_to(world, Some(pos));
    }

    pub fn reset(&mut self, world: &mut CollisionWorld, pressed: bool) {
        self.timer = if pressed { 5 } else { -1 };
        self.resize(world, if pressed { 0.2 } else { 1.0 });
    }

    /// Returns true on the frame the switch is first pressed.
    pub fn step(&mut self, floor: &Option<Arc<Surface>>, mario_y: f64, floor_height: f64, world: &mut CollisionWorld) -> bool {
        let pressed = self.timer < 0 && self.top.owns(floor) && mario_y <= floor_height + 1.0;
        if pressed {
            self.timer = 0;
        }
        if self.timer >= 0 && self.timer < 5 {
            let value = 1.0 - 0.2 * f64::from(self.timer);
            self.timer += 1;
            self.resize(world, value);
        }
        pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use crate::progress::data::ProgressData;
    use std::path::PathBuf;

    #[test]
    fn standing_on_the_switch_presses_it_and_shrinks_the_collider() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let mut sim = GameSim::load(&assets, CourseRequest::new("totwc")).unwrap();
        let data = ProgressData::load(&assets, "totwc", 1);
        let save = SaveFile::in_memory(1);
        let world = Arc::make_mut(&mut sim.mario.world);
        let mut sw = WingSwitch::new(data.wing_switch.unwrap(), &data.extra_collision, world, &save);
        assert_eq!(sw.height, 1.0);
        let top = sw.top_pos;
        let hit = world.find_floor(top[0], top[1] + 200.0, top[2]);
        assert!(sw.top.owns(&hit.surface), "the switch top is standable");
        let (floor, y, fh) = (hit.surface.clone(), hit.height, hit.height);
        assert!(sw.step(&floor, y, fh, world));
        for _ in 0..6 {
            let again = sw.step(&floor, y, fh, world);
            assert!(!again, "only reports once");
        }
        assert!((sw.height - 0.2).abs() < 1e-9, "squashed to 20%: {}", sw.height);
    }
}
