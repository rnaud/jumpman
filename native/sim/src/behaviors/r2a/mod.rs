//! SM64 round 2, part A: bosses and big enemies (Eyerok, Wiggler, Mr. I, Merry-Go-Round Boo, Dorrie,
//! Unagi, Manta Ray and the jet stream rings, Tweester, ...). Each file cites the decomp function
//! it ports (`src/game/behaviors/*.inc.c`); placements come from `scripts/export-sm64-bosses.py`
//! (objects flagged `g2`, so the loader treats them like the group-2 actors).
use crate::behaviors::g1::anim::AnimSpec;
use crate::object::ObjectBehaviorKind as K;
use crate::object_manager::BehaviorTable;

pub mod eyerok;
pub mod dorrie;
pub mod helpers;
pub mod interact;
pub mod merry_boo;
pub mod mr_i;
pub mod tweester_hoot;
pub mod unagi;
pub mod water_rings;
pub mod wiggler;

pub fn register(t: &mut BehaviorTable) {
    t.register(K::EyerokBoss, eyerok::init_boss, eyerok::boss);
    t.register(K::EyerokHand, eyerok::init_hand, eyerok::hand);
    t.set_interact(K::EyerokHand, crate::behaviors::g2::interact::interact);
    t.register(K::WigglerHead, wiggler::init_head, wiggler::head);
    t.register(K::WigglerBody, wiggler::init_body, wiggler::body);
    t.register(K::MrI, mr_i::init, mr_i::mr_i);
    t.register(K::MrIBody, mr_i::init_body, mr_i::body);
    t.register(K::MrIParticle, mr_i::init_particle, mr_i::particle);
    t.register(K::MerryGoRoundBooManager, merry_boo::init_manager, merry_boo::manager);
    t.register(K::MerryGoRoundBoo, merry_boo::init_boo, merry_boo::boo);
    t.register(K::MerryGoRoundBigBoo, merry_boo::init_big_boo, merry_boo::big_boo);
    t.register(K::Dorrie, dorrie::init, dorrie::update);
    t.register(K::Unagi, unagi::init, unagi::unagi);
    t.register(K::UnagiPart, unagi::init_part, unagi::part);
    t.set_interact(K::UnagiPart, interact::clam_or_bubba);
    t.register(K::MantaRay, water_rings::init_manta, water_rings::manta);
    t.set_interact(K::MantaRay, interact::clam_or_bubba);
    t.register(K::JetStreamRingSpawner, water_rings::init_spawner, water_rings::jet_stream_spawner);
    t.register(K::WaterRing, water_rings::init_ring, water_rings::ring);
    t.set_interact(K::WaterRing, interact::water_ring);
    t.register(K::Tweester, tweester_hoot::init_tweester, tweester_hoot::tweester);
    t.set_interact(K::Tweester, crate::mario_r2a::interact_tornado);
    t.register(K::Hoot, tweester_hoot::init_hoot, tweester_hoot::hoot);
    t.set_interact(K::Hoot, crate::mario_r2a::interact_hoot);
}

/// Kinds drawn with their face angles (`facePitch` / `faceRoll` vars) and scaled by `flipX`.
pub fn is_r2a(kind: K) -> bool {
    matches!(kind, K::EyerokHand | K::WigglerHead | K::WigglerBody | K::MrIBody | K::MrIParticle | K::MerryGoRoundBoo | K::MerryGoRoundBigBoo | K::Unagi | K::MantaRay | K::WaterRing | K::Tweester | K::Hoot)
}

/// The decomp animation names of an actor, in table order (the app picks the glb clip by name).
pub fn anim_table(kind: K) -> &'static [&'static str] {
    match kind {
        K::EyerokHand => &eyerok::ANIM_NAMES,
        K::WigglerHead => &wiggler::ANIM_NAMES_HEAD,
        K::WigglerBody => &wiggler::ANIM_NAMES_BODY,
        K::Dorrie => &dorrie::ANIM_NAMES,
        K::Unagi => &unagi::ANIM_NAMES,
        K::MantaRay => &water_rings::MANTA_ANIM_NAMES,
        K::WaterRing => &water_rings::RING_ANIM_NAMES,
        K::Hoot => &tweester_hoot::HOOT_ANIM_NAMES,
        _ => &[],
    }
}

/// The header specs of an actor's animations (tests compare them with the decomp).
pub fn anim_specs(kind: K) -> &'static [AnimSpec] {
    match kind {
        K::EyerokHand => &eyerok::ANIMS,
        K::WigglerHead | K::WigglerBody => &wiggler::ANIMS,
        K::Dorrie => &dorrie::ANIMS,
        K::Unagi => &unagi::ANIMS,
        K::MantaRay => &water_rings::MANTA_ANIMS,
        K::WaterRing => &water_rings::RING_ANIMS,
        K::Hoot => &tweester_hoot::HOOT_ANIMS,
        _ => &[],
    }
}
