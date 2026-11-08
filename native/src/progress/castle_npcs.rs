//! The save-file side of the castle NPCs (`bhv_toad_message_init`, `bhv_mips_init`): the original
//! behaviors read `save_file_get_total_star_count` and the `SAVE_FLAG_COLLECTED_TOAD_STAR_n` /
//! `COLLECTED_MIPS_STAR_n` flags when they are created. The sim has no save file, so the flow publishes
//! those values as vars on the freshly spawned objects (`behaviors/part_b/toad.rs`,
//! `behaviors/g1/mips.rs`); the objects apply them once.
use super::save::{SaveFile, FLAG_MOAT_DRAINED};
use crate::game::runner::GameSim;
use jumpman_sim::behaviors::part_b::toad::star_index;
use jumpman_sim::object::ObjectBehaviorKind as K;

/// The castle stars are saved as `castle_inside` acts 1-3 (Toad) and 4-5 (Mips).
pub const CASTLE_COURSE: &str = "castle_inside";

/// Publish the star count / flags to every Toad and Mips that has not applied them yet.
pub fn apply(sim: &mut GameSim, save: &SaveFile) {
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
            K::WaterLevelPillar | K::MoatGrill if !o.has_var("drained") => {
                o.set_var("drained", if save.has_flag(FLAG_MOAT_DRAINED) { 1.0 } else { 0.0 });
            }
            _ => {}
        }
    }
}

/// Per tick: a basement pillar finished the drain (`save_file_set_flags(SAVE_FLAG_MOAT_DRAINED)`).
pub fn step(sim: &mut GameSim, save: &mut SaveFile) {
    for o in sim.mario.objects.objects.iter_mut().filter(|o| o.behavior == K::WaterLevelPillar) {
        if o.var("drainedNow") != 0.0 {
            o.set_var("drainedNow", 0.0);
            save.set_flag(FLAG_MOAT_DRAINED);
        }
    }
}
