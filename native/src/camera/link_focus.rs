//! Port of `LinkFocus.ts`: Z-targeting target selection. Only sim objects are considered
//! (the TS also lists legacy goomba enemies, which live in the object system here).
//! Used only when playing as Link (`as_link` in `MarioRuntime::step`), which this app does
//! not enable yet (see PORTING.md `camera::link_focus`).
#![allow(dead_code)]
use jumpman_sim::math::{atan2s, coss, sins, Vec3};
use jumpman_sim::object::{ObjId, ObjectBehaviorKind, SimObject, INTERACT_DAMAGE, INTERACT_KOOPA, INTERACT_MR_BLIZZARD};
use jumpman_sim::object_manager::ObjectManager;

#[derive(Default)]
pub struct LinkFocus {
    pub target: Option<ObjId>,
}

impl LinkFocus {
    pub fn clear(&mut self) {
        self.target = None;
    }

    /// Prefer opponents ahead of Link. Keep the chosen opponent until it leaves range.
    /// Returns the focus yaw (s16) to feed `ControllerSnapshot::focus_yaw`.
    pub fn acquire(&mut self, objects: &ObjectManager, pos: Vec3, yaw: f64) -> f64 {
        let mut best: Option<ObjId> = None;
        let mut best_score = f64::INFINITY;
        for id in objects.ids() {
            let obj = &objects[id];
            let targetable = obj.active
                && obj.held_state == 0
                && (obj.behavior == ObjectBehaviorKind::Bobomb
                    || obj.hitbox.interact_type & (INTERACT_DAMAGE | INTERACT_KOOPA | INTERACT_MR_BLIZZARD) != 0);
            if !targetable {
                continue;
            }
            let (dx, dz) = (obj.pos[0] - pos[0], obj.pos[2] - pos[2]);
            let distance = (dx * dx + (obj.pos[1] - pos[1]).powi(2) + dz * dz).sqrt();
            let retained = self.target == Some(id);
            if distance > if retained { 1800.0 } else { 1500.0 } {
                continue;
            }
            let ahead = (dx * sins(yaw) + dz * coss(yaw)) / 1.0f64.max(dx.hypot(dz));
            if !retained && ahead < -0.35 && distance > 250.0 {
                continue;
            }
            let score = if retained { -1.0 } else { distance * (1.5 - 0.5 * ahead) };
            if score < best_score {
                best_score = score;
                best = Some(id);
            }
        }
        self.target = best;
        match self.point(objects) {
            Some(p) => atan2s(p[2] - pos[2], p[0] - pos[0]) as f64,
            None => yaw,
        }
    }

    /// World position of the marker above the target, if it is still valid.
    pub fn point(&mut self, objects: &ObjectManager) -> Option<Vec3> {
        let id = self.target?;
        let o: &SimObject = &objects[id];
        if !o.active {
            self.target = None;
            return None;
        }
        Some([o.pos[0], o.pos[1] + o.hitbox.height - o.hitbox.down_offset, o.pos[2]])
    }
}
