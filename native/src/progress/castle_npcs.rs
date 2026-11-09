//! The save-file side of the castle NPCs (`bhv_toad_message_init`, `bhv_mips_init`): the original
//! behaviors read `save_file_get_total_star_count` and the `SAVE_FLAG_COLLECTED_TOAD_STAR_n` /
//! `COLLECTED_MIPS_STAR_n` flags when they are created. The sim has no save file, so the flow publishes
//! those values as vars on the freshly spawned objects (`behaviors/part_b/toad.rs`,
//! `behaviors/g1/mips.rs`); the objects apply them once.
use super::save::{SaveFile, FLAG_CAP_ON_KLEPTO, FLAG_CAP_ON_UKIKI, FLAG_MOAT_DRAINED};
use crate::game::runner::GameSim;
use jumpman_sim::behaviors::part_b::toad::star_index;
use jumpman_sim::object::ObjectBehaviorKind as K;

/// The castle stars are saved as `castle_inside` acts 1-3 (Toad) and 4-5 (Mips).
pub const CASTLE_COURSE: &str = "castle_inside";

/// Publish the star count / flags to every Toad and Mips that has not applied them yet.
pub fn apply(sim: &mut GameSim, save: &SaveFile) {
    apply_cap(sim, save);
    let stars = f64::from(save.stars());
    let (m1, m2) = (save.has_star(CASTLE_COURSE, 4), save.has_star(CASTLE_COURSE, 5));
    for o in sim.mario.objects.objects.iter_mut() {
        match o.behavior {
            K::ToadMessage if !o.has_var("gated") => {
                o.set_var("stars", stars);
                let collected = star_index(o.param as u32).is_some_and(|act| save.has_star(CASTLE_COURSE, act));
                o.set_var("collected", if collected { 1.0 } else { 0.0 });
            }
            K::Mips if !o.has_var("gated") => {
                o.set_var("stars", stars);
                o.set_var("collected1", if m1 { 1.0 } else { 0.0 });
                o.set_var("collected2", if m2 { 1.0 } else { 0.0 });
            }
            K::CourtyardBooTriplet if !o.has_var("stars") => o.set_var("stars", stars),
            K::Yoshi if !o.has_var("stars") => o.set_var("stars", stars),
            K::WaterLevelPillar | K::MoatGrill if !o.has_var("drained") => {
                o.set_var("drained", if save.has_flag(FLAG_MOAT_DRAINED) { 1.0 } else { 0.0 });
            }
            _ => {}
        }
    }
}

fn cap_flags(save: &SaveFile) -> u32 {
    [FLAG_CAP_ON_KLEPTO, FLAG_CAP_ON_UKIKI].into_iter().filter(|&f| save.has_flag(f)).fold(0, |a, f| a | f)
}

static INIT_CAP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The game turns on `init_mario`'s cap flags (`main`); off by default so the TypeScript golden harnesses, whose
/// Mario never had cap flags, keep their hashes.
pub fn set_init_cap(on: bool) {
    INIT_CAP.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// [`init_cap`] when the game enabled it ([`set_init_cap`]).
pub fn apply_cap(sim: &mut GameSim, save: &SaveFile) {
    if INIT_CAP.load(std::sync::atomic::Ordering::Relaxed) {
        init_cap(sim, save);
    }
}

/// `init_mario`: Mario wears his cap (`MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD`) unless the save says an enemy has
/// it (`SAVE_FLAG_CAP_ON_KLEPTO` / `SAVE_FLAG_CAP_ON_UKIKI`). Applied once per Mario runtime; respawns redo it
/// in `MarioRuntime::reset_to_spawn` (`CutExt::spawn_with_cap`).
pub fn init_cap(sim: &mut GameSim, save: &SaveFile) {
    use jumpman_sim::state::*;
    let m = &mut sim.mario;
    if m.cut.spawn_with_cap {
        return;
    }
    m.cut.spawn_with_cap = true;
    m.cut.cap_save_flags = cap_flags(save);
    let caps = MARIO_NORMAL_CAP | MARIO_VANISH_CAP | MARIO_METAL_CAP | MARIO_WING_CAP | MARIO_CAP_ON_HEAD | MARIO_CAP_IN_HAND;
    if m.cut.cap_save_flags == 0 && m.state.flags & caps == 0 {
        m.state.flags |= MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD;
    }
}

/// Per tick: a basement pillar finished the drain (`save_file_set_flags(SAVE_FLAG_MOAT_DRAINED)`); Ukiki took or
/// gave back the cap (`mario_lose_cap_to_enemy` / `mario_retrieve_cap` change `CutExt::cap_save_flags`).
pub fn step(sim: &mut GameSim, save: &mut SaveFile) {
    if sim.mario.cut.spawn_with_cap {
        let cap = sim.mario.cut.cap_save_flags & (FLAG_CAP_ON_KLEPTO | FLAG_CAP_ON_UKIKI);
        let saved = cap_flags(save);
        if cap != saved {
            save.clear_flag(saved & !cap);
            save.set_flag(cap & !saved);
        }
    }
    for o in sim.mario.objects.objects.iter_mut().filter(|o| o.behavior == K::WaterLevelPillar) {
        if o.var("drainedNow") != 0.0 {
            o.set_var("drainedNow", 0.0);
            save.set_flag(FLAG_MOAT_DRAINED);
        }
    }
}
