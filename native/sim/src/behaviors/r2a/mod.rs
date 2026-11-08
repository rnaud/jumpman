//! SM64 round 2, part A: bosses and big enemies (Eyerok, Wiggler, Mr. I, Merry-Go-Round Boo, Dorrie,
//! Unagi, Manta Ray and the jet stream rings, Tweester, ...). Each file cites the decomp function
//! it ports (`src/game/behaviors/*.inc.c`); placements come from `scripts/export-sm64-bosses.py`
//! (objects flagged `g2`, so the loader treats them like the group-2 actors).
use crate::behaviors::g1::anim::AnimSpec;
use crate::object::ObjectBehaviorKind as K;
use crate::object_manager::BehaviorTable;

pub mod eyerok;
pub mod helpers;

pub fn register(t: &mut BehaviorTable) {
    t.register(K::EyerokBoss, eyerok::init_boss, eyerok::boss);
    t.register(K::EyerokHand, eyerok::init_hand, eyerok::hand);
    t.set_interact(K::EyerokHand, crate::behaviors::g2::interact::interact);
}

/// Kinds drawn with their face angles (`facePitch` / `faceRoll` vars) and scaled by `flipX`.
pub fn is_r2a(kind: K) -> bool {
    matches!(kind, K::EyerokHand)
}

/// The decomp animation names of an actor, in table order (the app picks the glb clip by name).
pub fn anim_table(kind: K) -> &'static [&'static str] {
    match kind {
        K::EyerokHand => &eyerok::ANIM_NAMES,
        _ => &[],
    }
}

/// The header specs of an actor's animations (tests compare them with the decomp).
pub fn anim_specs(kind: K) -> &'static [AnimSpec] {
    match kind {
        K::EyerokHand => &eyerok::ANIMS,
        _ => &[],
    }
}
