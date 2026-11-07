//! Port of `LinkFocus.ts`: Z-targeting target selection for Link. Candidates are the walking
//! goombas (they live outside the object manager, see `objects::goombas`) and every sim object
//! that damages, is a Koopa or Mr. Blizzard, or is a Bob-omb; goombas are considered first, like the TS.
#![allow(dead_code)]
use jumpman_sim::math::{atan2s, coss, sins, Vec3};
use jumpman_sim::object::{ObjId, ObjectBehaviorKind, SimObject, INTERACT_DAMAGE, INTERACT_KOOPA, INTERACT_MR_BLIZZARD};
use jumpman_sim::object_manager::ObjectManager;

/// A walking goomba as a focus candidate: (feet position, head height), `None` while it is not
/// tangible (`GoombaEnemy.tangible`). Indexed like `objects::goombas::Goombas`.
pub type GoombaTarget = Option<(Vec3, f64)>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusTarget {
    Object(ObjId),
    Goomba(usize),
}

#[derive(Default)]
pub struct LinkFocus {
    pub target: Option<FocusTarget>,
}

impl LinkFocus {
    pub fn clear(&mut self) {
        self.target = None;
    }

    /// Prefer opponents ahead of Link. Keep the chosen opponent until it leaves range.
    /// Returns the focus yaw (s16) to feed `ControllerSnapshot::focus_yaw`.
    pub fn acquire(&mut self, objects: &ObjectManager, goombas: &[GoombaTarget], pos: Vec3, yaw: f64) -> f64 {
        let mut best: Option<FocusTarget> = None;
        let mut best_score = f64::INFINITY;
        let mut consider = |this: &Self, enemy: FocusTarget, p: Vec3| {
            let (dx, dz) = (p[0] - pos[0], p[2] - pos[2]);
            let distance = (dx * dx + (p[1] - pos[1]).powi(2) + dz * dz).sqrt();
            let retained = this.target == Some(enemy);
            if distance > if retained { 1800.0 } else { 1500.0 } {
                return;
            }
            let ahead = (dx * sins(yaw) + dz * coss(yaw)) / 1.0f64.max(dx.hypot(dz));
            if !retained && ahead < -0.35 && distance > 250.0 {
                return;
            }
            let score = if retained { -1.0 } else { distance * (1.5 - 0.5 * ahead) };
            if score < best_score {
                best_score = score;
                best = Some(enemy);
            }
        };
        for (i, g) in goombas.iter().enumerate() {
            if let Some((feet, _)) = g {
                consider(self, FocusTarget::Goomba(i), *feet);
            }
        }
        for id in objects.ids() {
            let obj = &objects[id];
            let targetable = obj.active
                && obj.held_state == 0
                && (obj.behavior == ObjectBehaviorKind::Bobomb
                    || obj.hitbox.interact_type & (INTERACT_DAMAGE | INTERACT_KOOPA | INTERACT_MR_BLIZZARD) != 0);
            if targetable {
                consider(self, FocusTarget::Object(id), obj.pos);
            }
        }
        self.target = best;
        match self.point(objects, goombas) {
            Some(p) => atan2s(p[2] - pos[2], p[0] - pos[0]) as f64,
            None => yaw,
        }
    }

    /// World position of the marker above the target, if it is still valid.
    pub fn point(&mut self, objects: &ObjectManager, goombas: &[GoombaTarget]) -> Option<Vec3> {
        match self.target? {
            FocusTarget::Object(id) => {
                let o: &SimObject = &objects[id];
                if !o.active {
                    self.target = None;
                    return None;
                }
                Some([o.pos[0], o.pos[1] + o.hitbox.height - o.hitbox.down_offset, o.pos[2]])
            }
            FocusTarget::Goomba(i) => match goombas.get(i).copied().flatten() {
                Some((feet, top)) => Some([feet[0], top, feet[2]]),
                None => {
                    self.target = None;
                    None
                }
            },
        }
    }
}
