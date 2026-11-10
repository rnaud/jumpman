//! Z-targeting target selection for Link (grown out of the port of `LinkFocus.ts`). Candidates are
//! the walking goombas (they live outside the object manager, see `objects::goombas`; the OoT scene
//! enemies ride in the same list) and every sim object `jumpman_sim::link::targeting` classifies as
//! an opponent or as somebody to talk to. Hazards that merely hurt (flames, rolling balls) are not
//! targets. Goombas are considered first, like the TS.
#![allow(dead_code)]
use jumpman_sim::link::targeting::{target_class, TargetClass};
use jumpman_sim::math::{atan2s, coss, sins, Vec3};
use jumpman_sim::object::{ObjId, SimObject};
use jumpman_sim::object_manager::ObjectManager;

/// A walking goomba as a focus candidate: (feet position, head height), `None` while it is not
/// tangible (`GoombaEnemy.tangible`). Indexed like `objects::goombas::Goombas`.
pub type GoombaTarget = Option<(Vec3, f64)>;

/// Opponents lock from this far; a held lock survives to [`KEEP_RANGE`].
pub const ENEMY_RANGE: f64 = 1500.0;
pub const KEEP_RANGE: f64 = 1800.0;
/// People are only targeted from close by, and opponents win over them.
pub const FRIENDLY_RANGE: f64 = 700.0;
pub const FRIENDLY_KEEP_RANGE: f64 = 1000.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusTarget {
    Object(ObjId),
    Goomba(usize),
}

#[derive(Default)]
pub struct LinkFocus {
    pub target: Option<FocusTarget>,
}

/// Score of a candidate at `p` seen from `pos` facing `yaw` (lower is better), or `None` when it is out of
/// range or behind Link and not right next to him. A retained target only has to stay in the wider range.
fn score(p: Vec3, pos: Vec3, yaw: f64, class: TargetClass, retained: bool) -> Option<f64> {
    let (dx, dz) = (p[0] - pos[0], p[2] - pos[2]);
    let distance = (dx * dx + (p[1] - pos[1]).powi(2) + dz * dz).sqrt();
    let range = match (class, retained) {
        (TargetClass::Enemy, false) => ENEMY_RANGE,
        (TargetClass::Enemy, true) => KEEP_RANGE,
        (TargetClass::Friendly, false) => FRIENDLY_RANGE,
        (TargetClass::Friendly, true) => FRIENDLY_KEEP_RANGE,
    };
    if distance > range {
        return None;
    }
    if retained {
        return Some(-1.0);
    }
    let ahead = (dx * sins(yaw) + dz * coss(yaw)) / 1.0f64.max(jumpman_sim::math::js_hypot(dx, dz));
    if ahead < -0.35 && distance > 250.0 {
        return None;
    }
    let base = distance * (1.5 - 0.5 * ahead);
    Some(if class == TargetClass::Friendly { base + ENEMY_RANGE * 2.0 } else { base })
}

impl LinkFocus {
    pub fn clear(&mut self) {
        self.target = None;
    }

    /// Everything Link could lock on right now with its score, best first. `skip` leaves one target out
    /// (switching) and stops the current one from being favoured.
    fn ranked(&self, objects: &ObjectManager, goombas: &[GoombaTarget], pos: Vec3, yaw: f64, skip: Option<FocusTarget>) -> Vec<(f64, FocusTarget)> {
        let mut out: Vec<(f64, FocusTarget)> = vec![];
        let mut consider = |target: FocusTarget, p: Vec3, class: TargetClass| {
            if skip == Some(target) {
                return;
            }
            let retained = skip.is_none() && self.target == Some(target);
            if let Some(s) = score(p, pos, yaw, class, retained) {
                out.push((s, target));
            }
        };
        for (i, g) in goombas.iter().enumerate() {
            if let Some((feet, _)) = g {
                consider(FocusTarget::Goomba(i), *feet, TargetClass::Enemy);
            }
        }
        for id in objects.ids() {
            let obj = &objects[id];
            if let Some(class) = target_class(obj) {
                consider(FocusTarget::Object(id), obj.pos, class);
            }
        }
        // Stable: equal scores keep the goombas-first order of the TS.
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out
    }

    /// Prefer opponents ahead of Link. Keep the chosen opponent until it leaves range.
    /// Returns the focus yaw (s16) to feed `ControllerSnapshot::focus_yaw`.
    pub fn acquire(&mut self, objects: &ObjectManager, goombas: &[GoombaTarget], pos: Vec3, yaw: f64) -> f64 {
        self.target = self.ranked(objects, goombas, pos, yaw, None).first().map(|c| c.1);
        match self.point(objects, goombas) {
            Some(p) => atan2s(p[2] - pos[2], p[0] - pos[0]) as f64,
            None => yaw,
        }
    }

    /// Z pressed again while locked: move the lock to the best other candidate. `yaw` is the direction Link
    /// looks along (towards the current target). Returns false, leaving the lock alone, when there is no other.
    pub fn switch(&mut self, objects: &ObjectManager, goombas: &[GoombaTarget], pos: Vec3, yaw: f64) -> bool {
        let Some(current) = self.target else { return false };
        match self.ranked(objects, goombas, pos, yaw, Some(current)).first() {
            Some(&(_, next)) => {
                self.target = Some(next);
                true
            }
            None => false,
        }
    }

    /// The locked target is somebody to talk to rather than an opponent.
    pub fn friendly(&self, objects: &ObjectManager) -> bool {
        match self.target {
            Some(FocusTarget::Object(id)) => target_class(&objects[id]) == Some(TargetClass::Friendly),
            _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::object::{ObjectBehaviorKind, ObjectSpec, INTERACT_DAMAGE, INTERACT_TEXT};

    fn world(specs: &[(ObjectBehaviorKind, u32, Vec3)]) -> ObjectManager {
        let mut objects = ObjectManager::default();
        for (kind, interact, pos) in specs {
            let id = objects.spawn(&ObjectSpec::new(*kind, *pos, 0.0));
            objects[id].hitbox.interact_type = *interact;
        }
        objects
    }

    const BOUNCE_TOP: u32 = 1 << 15;

    #[test]
    fn spindrifts_lock_and_flames_do_not() {
        let objects = world(&[(ObjectBehaviorKind::Flame, INTERACT_DAMAGE, [0.0, 0.0, 300.0]), (ObjectBehaviorKind::Spindrift, BOUNCE_TOP, [0.0, 0.0, 900.0])]);
        let mut focus = LinkFocus::default();
        let yaw = focus.acquire(&objects, &[], [0.0; 3], 0.0);
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(1))), "the flame in front is a hazard, the spindrift behind it the opponent");
        assert_eq!(yaw, 0.0);
        assert!(focus.point(&objects, &[]).is_some());
    }

    #[test]
    fn nothing_to_lock_returns_links_own_yaw() {
        let objects = world(&[(ObjectBehaviorKind::Tree, jumpman_sim::object::INTERACT_POLE, [0.0, 0.0, 300.0])]);
        let mut focus = LinkFocus::default();
        assert_eq!(focus.acquire(&objects, &[], [0.0; 3], 1234.0), 1234.0);
        assert_eq!(focus.target, None);
        assert!(!focus.switch(&objects, &[], [0.0; 3], 0.0));
    }

    #[test]
    fn z_again_switches_to_the_other_opponent_and_back() {
        let objects = world(&[(ObjectBehaviorKind::Boo, INTERACT_DAMAGE, [0.0, 0.0, 400.0]), (ObjectBehaviorKind::Boo, INTERACT_DAMAGE, [300.0, 0.0, 700.0])]);
        let goombas = [Some(([-2000.0, 0.0, 9000.0], 60.0))]; // far out of range
        let mut focus = LinkFocus::default();
        focus.acquire(&objects, &goombas, [0.0; 3], 0.0);
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(0))));
        assert!(focus.switch(&objects, &goombas, [0.0; 3], 0.0));
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(1))));
        // The new target is kept by the following ticks...
        focus.acquire(&objects, &goombas, [0.0; 3], 0.0);
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(1))));
        // ...and another press goes back.
        assert!(focus.switch(&objects, &goombas, [0.0; 3], 0.0));
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(0))));
    }

    #[test]
    fn a_lone_target_cannot_be_switched_away_from() {
        let objects = world(&[(ObjectBehaviorKind::Boo, INTERACT_DAMAGE, [0.0, 0.0, 400.0])]);
        let mut focus = LinkFocus::default();
        focus.acquire(&objects, &[], [0.0; 3], 0.0);
        assert!(!focus.switch(&objects, &[], [0.0; 3], 0.0));
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(0))));
    }

    #[test]
    fn people_are_targeted_from_close_by_and_opponents_win() {
        let buddy = (ObjectBehaviorKind::BobombBuddy, INTERACT_TEXT, [0.0, 0.0, 300.0]);
        let mut focus = LinkFocus::default();
        let objects = world(&[buddy]);
        focus.acquire(&objects, &[], [0.0; 3], 0.0);
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(0))));
        assert!(focus.friendly(&objects));
        // From 1200 away he is not a target, an opponent would be.
        focus.clear();
        focus.acquire(&objects, &[], [0.0, 0.0, -900.0], 0.0);
        assert_eq!(focus.target, None);
        // An opponent further away than the buddy still takes the lock.
        let objects = world(&[buddy, (ObjectBehaviorKind::Boo, INTERACT_DAMAGE, [0.0, 0.0, 1200.0])]);
        focus.clear();
        focus.acquire(&objects, &[], [0.0; 3], 0.0);
        assert_eq!(focus.target, Some(FocusTarget::Object(ObjId(1))));
        assert!(!focus.friendly(&objects));
    }
}
