//! The app-layer steps `Game.stepWorld` interleaves with the sim tick (see
//! [`crate::game::runner::TickHooks`]): object colliders follow their objects right after the
//! object pass and again after Mario's step (`CourseObjects.syncViews`), and between the object pass
//! and Mario's step the Bob-omb buddy check and `cannons.step` run. The Bevy `tick` system and the
//! differential test (`game/game_tests.rs`) both use this, so they cannot drift apart.
use super::colliders::{sync, ObjectColliders};
use crate::game::cannons::{CannonOperator, Cannons};
use crate::game::runner::{GameSim, TickHooks};
use jumpman_sim::math::to_s16;
use jumpman_sim::object::{mario_overlaps_object, ObjId, ObjectBehaviorKind as K};
use jumpman_sim::state::*;

pub struct StageHooks<'a> {
    pub colliders: Option<&'a mut ObjectColliders>,
    pub cannons: Option<&'a mut Cannons>,
    pub wing_unlocked: bool,
    pub course_id: &'a str,
    /// `save.cannonOpen(course)`.
    pub cannon_unlocked: bool,
    /// Filled when the buddy check started a cannon operator this tick (Mario is not stepped).
    pub operator: Option<CannonOperator>,
}

impl TickHooks for StageHooks<'_> {
    fn after_objects(&mut self, sim: &mut GameSim) {
        if let Some(colliders) = self.colliders.as_deref_mut() {
            sync(sim, colliders, self.wing_unlocked);
        }
    }

    fn before_mario(&mut self, sim: &mut GameSim, controller: &ControllerSnapshot) -> bool {
        let Some(cannons) = self.cannons.as_deref_mut() else { return false };
        if controller.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
            let m = &sim.mario;
            if m.action & ACT_FLAG_IDLE != 0 || m.action == ACT_WALKING {
                let buddy = m.objects.objects.iter().position(|o| {
                    o.active
                        && matches!(o.behavior, K::BobombBuddy | K::BobombBuddyAdvice)
                        && mario_overlaps_object(m, o)
                        && (to_s16(o.angle_to_mario + 0x8000 as f64 - m.face_angle[1]) as f64).abs() <= 0x4000 as f64
                });
                if let Some(buddy) = buddy {
                    let unlocked = self.cannon_unlocked || cannons.open;
                    self.operator = Some(CannonOperator::new(m, ObjId(buddy as u32), self.course_id == "bob", unlocked));
                    return true;
                }
            }
        }
        cannons.step(&mut sim.mario);
        false
    }

    fn after_mario(&mut self, sim: &mut GameSim) {
        if let Some(colliders) = self.colliders.as_deref_mut() {
            sync(sim, colliders, self.wing_unlocked);
        }
    }
}
