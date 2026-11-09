//! SM64 "last stars" round: the behaviors the last six audit-blocked stars needed.
//!
//! * [`ukiki`]: `ukiki.inc.c` + `ukiki_cage.inc.c` (TTM act 2 "Mystery of the Monkey Cage": the cage Ukiki asks
//!   to be taken to his cage with a Yes / No box, jumps on it and breaks it open; the cap Ukiki steals Mario's
//!   cap when grabbed and gives it back the next time).
//! * [`boulder`]: `boulder.inc.c` (HMC act 6 "Watch for Rolling Rocks": `bhvBigBoulderGenerator` rolls
//!   `bhvBigBoulder`s down the rock slope).
//!
//! The Mario side of the cap theft (`mario_lose_cap_to_enemy`, `mario_retrieve_cap`,
//! `does_mario_have_normal_cap_on_head`, interaction.c) lives here too. Conventions are those of
//! `behaviors/g2/obj.rs` (decomp `oTimer` / `oAction` / `oSubAction`, the `o*` fields without a native
//! member in `vars`) with the animation state of `behaviors/g1/anim.rs`. Placements come from
//! `scripts/export-sm64-bosses.py` (objects flagged `g2`, so only the app loads them).
use crate::behaviors::g1::anim::AnimSpec;
use crate::cutscene_ext::{SAVE_FLAG_CAP_ON_KLEPTO, SAVE_FLAG_CAP_ON_UKIKI};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;
use crate::object_manager::BehaviorTable;
use crate::state::*;

pub mod bbh;
pub mod bub;
pub mod bubba;
pub mod chuckya;
pub mod moneybag;
pub mod sl_wind;
pub mod courtyard_boo;
pub mod heave_ho;
pub mod clam;
pub mod fire_spitter;
pub mod fly_guy;
pub mod coffin;
pub mod boulder;
pub mod skeeter;
pub mod tumbling;
pub mod ukiki;

pub fn register(t: &mut BehaviorTable) {
    t.register(K::Ukiki, ukiki::init, ukiki::update);
    t.register(K::UkikiCage, ukiki::init_cage, ukiki::cage);
    t.register(K::UkikiCageStar, ukiki::init_cage_star, ukiki::cage_star);
    t.register(K::BigBoulder, boulder::init, boulder::boulder);
    t.register(K::BigBoulderGenerator, boulder::init_generator, boulder::generator);
    t.register(K::ChirpChirp, bub::init_spawner, bub::spawner);
    t.register(K::Bub, bub::init_bub, bub::bub);
    t.register(K::FireSpitter, fire_spitter::init, fire_spitter::update);
    t.register(K::FlyGuy, fly_guy::init, fly_guy::update);
    t.set_interact(K::FlyGuy, crate::behaviors::g2::interact::interact);
    t.register(K::ClamShell, clam::init, clam::update);
    t.set_interact(K::ClamShell, crate::behaviors::r2a::interact::clam_or_bubba);
    t.register(K::CourtyardBooTriplet, courtyard_boo::init, courtyard_boo::update);
    t.register(K::Moneybag, moneybag::init_bag, moneybag::bag);
    t.set_interact(K::Moneybag, crate::behaviors::g2::interact::interact);
    t.register(K::MoneybagHidden, moneybag::init_hidden, moneybag::hidden);
    t.set_interact(K::MoneybagHidden, crate::behaviors::g2::interact::interact);
    t.register(K::SlSnowmanWind, sl_wind::init_wind, sl_wind::wind);
    t.register(K::StrongWindParticle, sl_wind::init_particle, sl_wind::particle);
    t.set_interact(K::StrongWindParticle, crate::mario_r2a::interact_strong_wind);
    t.register(K::SlWalkingPenguin, sl_wind::init_penguin, sl_wind::penguin);
    t.register(K::Chuckya, chuckya::init, chuckya::update);
    t.register(K::HeaveHo, heave_ho::init, heave_ho::update);
    t.register(K::CoffinSpawner, coffin::init_spawner, coffin::spawner);
    t.register(K::Coffin, coffin::init_coffin, coffin::coffin);
    t.register(K::Bubba, bubba::init, bubba::update);
    t.set_interact(K::Bubba, crate::behaviors::r2a::interact::clam_or_bubba);
    t.register(K::TumblingBridge, tumbling::init_bridge, tumbling::bridge);
    t.register(K::TumblingBridgePlatform, tumbling::init_platform, tumbling::platform);
    t.register(K::Skeeter, skeeter::init, skeeter::update);
    t.set_interact(K::Skeeter, crate::behaviors::g2::interact::interact);
    t.register(K::SkeeterWave, skeeter::init_wave, skeeter::wave);
    t.register(K::MadPiano, bbh::init_piano, bbh::piano);
    t.register(K::HauntedChair, bbh::init_chair, bbh::chair);
    t.register(K::FlyingBookend, bbh::init_bookend, bbh::bookend);
    t.register(K::BookendSpawn, bbh::init_spawner, bbh::spawner);
}

/// Kinds drawn like the round-2 actors (face angles in `facePitch` / `faceRoll`, glb clips by decomp name).
pub fn is_r3(kind: K) -> bool {
    matches!(kind, K::Ukiki | K::UkikiCage | K::UkikiCageStar | K::BigBoulder | K::MadPiano | K::HauntedChair | K::FlyingBookend | K::Skeeter | K::Bubba | K::Bub | K::FlyGuy | K::ClamShell | K::Chuckya | K::HeaveHo | K::Moneybag | K::SlWalkingPenguin)
}

pub fn anim_table(kind: K) -> &'static [&'static str] {
    match kind {
        K::Ukiki => &ukiki::ANIM_NAMES,
        K::Skeeter => &skeeter::ANIM_NAMES,
        K::Bub => &bub::ANIM_NAMES,
        K::SlWalkingPenguin => &sl_wind::ANIM_NAMES,
        K::Moneybag => &moneybag::ANIM_NAMES,
        K::Chuckya => &chuckya::ANIM_NAMES,
        K::HeaveHo => &heave_ho::ANIM_NAMES,
        K::ClamShell => &clam::ANIM_NAMES,
        K::FlyGuy => &fly_guy::ANIM_NAMES,
        K::MadPiano => &bbh::PIANO_ANIM_NAMES,
        K::FlyingBookend => &bbh::BOOKEND_ANIM_NAMES,
        _ => &[],
    }
}

pub fn anim_specs(kind: K) -> &'static [AnimSpec] {
    match kind {
        K::Ukiki => &ukiki::ANIMS,
        K::Skeeter => &skeeter::ANIMS,
        K::Bub => &bub::ANIMS,
        K::SlWalkingPenguin => &sl_wind::ANIMS,
        K::Moneybag => &moneybag::ANIMS,
        K::Chuckya => &chuckya::ANIMS,
        K::HeaveHo => &heave_ho::ANIMS,
        K::ClamShell => &clam::ANIMS,
        K::FlyGuy => &fly_guy::ANIMS,
        K::MadPiano => &bbh::PIANO_ANIMS,
        K::FlyingBookend => &bbh::BOOKEND_ANIMS,
        _ => &[],
    }
}

// ---------------------------------------------------------------------------------------------
// interaction.c: Mario's cap and the enemies that steal it
// ---------------------------------------------------------------------------------------------

/// `MARIO_CAPS`.
pub const MARIO_CAPS: u32 = MARIO_NORMAL_CAP | MARIO_VANISH_CAP | MARIO_METAL_CAP | MARIO_WING_CAP;

/// interaction.c `does_mario_have_normal_cap_on_head`.
pub fn does_mario_have_normal_cap_on_head(m: &MarioRuntime) -> bool {
    m.flags & (MARIO_CAPS | MARIO_CAP_ON_HEAD) == MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD
}

/// interaction.c `mario_lose_cap_to_enemy(arg)`: 1 = Klepto, 2 = Ukiki. The save flag lives on
/// `CutExt::cap_save_flags` (the host persists it).
pub fn mario_lose_cap_to_enemy(m: &mut MarioRuntime, arg: u32) -> bool {
    if does_mario_have_normal_cap_on_head(m) {
        m.cut.cap_save_flags |= if arg == 1 { SAVE_FLAG_CAP_ON_KLEPTO } else { SAVE_FLAG_CAP_ON_UKIKI };
        m.state.flags &= !(MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD);
        return true;
    }
    false
}

/// interaction.c `mario_retrieve_cap`: drop what Mario holds, clear the save flags, cap in hand
/// (`act_reading_npc_dialog` then puts it on with `ACT_PUTTING_ON_CAP`).
pub fn mario_retrieve_cap(m: &mut MarioRuntime) {
    m.drop_held_object();
    m.cut.cap_save_flags &= !(SAVE_FLAG_CAP_ON_KLEPTO | SAVE_FLAG_CAP_ON_UKIKI);
    m.state.flags &= !MARIO_CAP_ON_HEAD;
    m.state.flags |= MARIO_NORMAL_CAP | MARIO_CAP_IN_HAND;
}

// ---------------------------------------------------------------------------------------------
// object_helpers.c / obj_behaviors.c helpers the ports share
// ---------------------------------------------------------------------------------------------

/// obj_behaviors.c `check_if_moving_over_floor(maxDist, offset)`.
pub fn check_if_moving_over_floor(o: &SimObject, world: &crate::collision::CollisionWorld, max_dist: f64, offset: f64) -> bool {
    let x = o.pos[0] + sins(o.move_yaw) * offset;
    let z = o.pos[2] + coss(o.move_yaw) * offset;
    let h = world.find_floor(x, o.pos[1], z).height;
    (h - o.pos[1]).abs() < max_dist
}

/// object_helpers.c `cur_obj_can_mario_activate_textbox(radius, height, _)`.
pub fn can_mario_activate_textbox(m: &MarioRuntime, o: &SimObject, radius: f64, height: f64) -> bool {
    if o.distance_to_mario < 1500.0 {
        let lat = ((o.pos[0] - m.pos[0]).powi(2) + (o.pos[2] - m.pos[2]).powi(2)).sqrt();
        if lat < radius
            && o.pos[1] < m.pos[1] + 160.0
            && m.pos[1] < o.pos[1] + height
            && m.action & ACT_FLAG_AIR == 0
            && crate::behaviors::r2a::helpers::mario_ready_to_speak(m)
        {
            return true;
        }
    }
    false
}

/// object_helpers.c `cur_obj_mario_far_away`.
pub fn mario_far_away(m: &MarioRuntime, o: &SimObject) -> bool {
    let d = dist_3d(&o.home, &m.pos);
    o.distance_to_mario > 2000.0 && d > 2000.0
}

/// object_helpers.c `is_mario_moving_fast_or_in_air(speedThreshold)`.
pub fn is_mario_moving_fast_or_in_air(m: &MarioRuntime, threshold: f64) -> bool {
    m.forward_vel > threshold || m.action & ACT_FLAG_AIR != 0
}
