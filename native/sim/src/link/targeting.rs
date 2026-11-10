//! What Link can Z-target, strike with the blade's reach and shoot on an SM64 course.
//!
//! The SM64 objects only say how they interact with Mario (`interact_type`), which mixes real
//! opponents with scenery that merely hurts (flames, rolling balls, crushers). One classification
//! is shared by the lock-on (`camera/link_focus.rs` in the app), the sword reach
//! (`object_manager::detect_mario_interactions`) and the projectiles (`LinkExt::collect_targets`).
use crate::object::{ObjectBehaviorKind, SimObject, INTERACT_DAMAGE, INTERACT_KOOPA, INTERACT_MR_BLIZZARD, INTERACT_TEXT};

const INTERACT_BOUNCE_TOP: u32 = crate::behaviors::g1::INTERACT_BOUNCE_TOP;
const INTERACT_HIT_FROM_BELOW: u32 = crate::behaviors::g1::INTERACT_HIT_FROM_BELOW;
const INTERACT_CLAM_OR_BUBBA: u32 = 1 << 26;

/// Interaction types that mark something that fights back.
pub const OPPONENT_INTERACT: u32 = INTERACT_DAMAGE | INTERACT_KOOPA | INTERACT_MR_BLIZZARD | INTERACT_BOUNCE_TOP | INTERACT_HIT_FROM_BELOW | INTERACT_CLAM_OR_BUBBA;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetClass {
    /// An opponent: yellow marker, battle camera, the sword reaches it.
    Enemy,
    /// Somebody to talk to: targetable from close by, never struck by the reach extension.
    Friendly,
}

/// Hurts on touch but is not somebody to fight: fire, projectiles, rolling balls, crushers, mines.
fn hazard(kind: ObjectBehaviorKind) -> bool {
    use ObjectBehaviorKind as K;
    matches!(
        kind,
        K::Flame
            | K::VolcanoFlames
            | K::LllFireBars
            | K::LllHexFlame
            | K::Flamethrower
            | K::FlamethrowerFlame
            | K::BouncingFireball
            | K::BouncingFireFlame
            | K::SmallPiranhaFlame
            | K::BowserFlame
            | K::SmBowserFlame
            | K::SmBowserShockWave
            | K::SmFlamethrower
            | K::SmFlamethrowerFlame
            | K::FireSpitter
            | K::PitBowlingBall
            | K::MountainBall
            | K::BigBoulder
            | K::WaterBomb
            | K::SnufitBall
            | K::MontyMoleRock
            | K::MrIParticle
            | K::SkeeterWave
            | K::PoisonGas
            | K::BowserMine
            | K::SmBowserBomb
            | K::ToxBox
            | K::Grindel
            | K::HorizontalGrindel
            | K::Spindel
            | K::Thwomp
            | K::Thwomp2
            | K::RollingLogLll
            | K::RollingLogTtm
            | K::CirclingAmp
            | K::Tweester
            | K::Whirlpool
            | K::StrongWindParticle
            | K::SlSnowmanWind
            | K::ChainPart
            | K::UnagiPart
            | K::WigglerBody
            | K::MrIBody
            | K::KoopaShell
            | K::KoopaShellUnderwater
    )
}

fn friendly(kind: ObjectBehaviorKind) -> bool {
    use ObjectBehaviorKind as K;
    matches!(
        kind,
        K::BobombBuddy | K::BobombBuddyAdvice | K::ToadMessage | K::KoopaTheQuick | K::Penguin | K::TuxiesMother | K::RacingPenguin | K::BabyPenguin | K::SlWalkingPenguin | K::Mips | K::Yoshi | K::Hoot | K::Dorrie | K::Ukiki
    )
}

/// Opponents whose interaction type alone does not say so (grabbable or pushing enemies).
fn opponent(kind: ObjectBehaviorKind) -> bool {
    use ObjectBehaviorKind as K;
    matches!(kind, K::Bobomb | K::KingBobomb | K::Chuckya | K::HeaveHo | K::Bully | K::BigBully | K::BigChillBully | K::Bowser | K::SmBowser | K::Moneybag | K::MontyMole | K::Klepto | K::FlyGuy)
}

/// How Link sees `o`, or `None` for scenery, pickups, hazards and anything inactive or carried.
pub fn target_class(o: &SimObject) -> Option<TargetClass> {
    if !o.active || o.held_state != 0 || hazard(o.behavior) {
        return None;
    }
    if friendly(o.behavior) {
        return Some(TargetClass::Friendly);
    }
    let t = o.hitbox.interact_type;
    if opponent(o.behavior) || t & OPPONENT_INTERACT != 0 {
        return Some(TargetClass::Enemy);
    }
    if t & INTERACT_TEXT != 0 {
        return Some(TargetClass::Friendly);
    }
    None
}

/// The blade's reach extension applies to `o` (opponents, plus the breakables / grabbables it always reached).
pub fn sword_reaches(o: &SimObject) -> bool {
    use crate::object::{INTERACT_BREAKABLE, INTERACT_GRABBABLE};
    if hazard(o.behavior) || friendly(o.behavior) {
        return false;
    }
    o.hitbox.interact_type & (OPPONENT_INTERACT | INTERACT_BREAKABLE | INTERACT_GRABBABLE) != 0 || opponent(o.behavior)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ObjectHitbox;

    fn obj(kind: ObjectBehaviorKind, interact: u32) -> SimObject {
        let mut o = SimObject::new(&crate::object::ObjectSpec::new(kind, [0.0; 3], 0.0));
        o.active = true;
        o.hitbox = ObjectHitbox { interact_type: interact, ..Default::default() };
        o
    }

    #[test]
    fn enemies_that_only_bounce_or_hit_from_below_are_opponents() {
        assert_eq!(target_class(&obj(ObjectBehaviorKind::Spindrift, INTERACT_BOUNCE_TOP)), Some(TargetClass::Enemy));
        assert_eq!(target_class(&obj(ObjectBehaviorKind::Scuttlebug, INTERACT_BOUNCE_TOP)), Some(TargetClass::Enemy));
        assert_eq!(target_class(&obj(ObjectBehaviorKind::Swoop, INTERACT_HIT_FROM_BELOW)), Some(TargetClass::Enemy));
        assert_eq!(target_class(&obj(ObjectBehaviorKind::Bobomb, crate::object::INTERACT_GRABBABLE)), Some(TargetClass::Enemy));
        assert_eq!(target_class(&obj(ObjectBehaviorKind::ChainChomp, INTERACT_MR_BLIZZARD)), Some(TargetClass::Enemy));
    }

    #[test]
    fn hazards_scenery_and_carried_things_are_not_targets() {
        assert_eq!(target_class(&obj(ObjectBehaviorKind::Flame, INTERACT_DAMAGE)), None);
        assert_eq!(target_class(&obj(ObjectBehaviorKind::PitBowlingBall, INTERACT_DAMAGE)), None);
        assert_eq!(target_class(&obj(ObjectBehaviorKind::Tree, crate::object::INTERACT_POLE)), None);
        assert_eq!(target_class(&obj(ObjectBehaviorKind::BreakableBox, crate::object::INTERACT_BREAKABLE)), None);
        let mut held = obj(ObjectBehaviorKind::Bobomb, crate::object::INTERACT_GRABBABLE);
        held.held_state = 1;
        assert_eq!(target_class(&held), None);
        let mut gone = obj(ObjectBehaviorKind::Boo, INTERACT_DAMAGE);
        gone.active = false;
        assert_eq!(target_class(&gone), None);
        assert!(!sword_reaches(&obj(ObjectBehaviorKind::Flame, INTERACT_DAMAGE)));
    }

    #[test]
    fn people_are_friendly_and_out_of_the_blades_reach() {
        let buddy = obj(ObjectBehaviorKind::BobombBuddy, INTERACT_TEXT);
        assert_eq!(target_class(&buddy), Some(TargetClass::Friendly));
        assert!(!sword_reaches(&buddy));
        assert!(sword_reaches(&obj(ObjectBehaviorKind::Spindrift, INTERACT_BOUNCE_TOP)));
        assert!(sword_reaches(&obj(ObjectBehaviorKind::BreakableBox, crate::object::INTERACT_BREAKABLE)));
    }
}
