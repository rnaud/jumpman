//! Presentation of the SM64 group 1 actors (`jumpman_sim::behaviors::g1`): the glb clip of an
//! object is chosen by its decomp animation name (the sim keeps the decomp table index in the
//! `animation` var and the frame in `animFrame`), and the clip is held at that frame.
use super::object_view::AnimRig;
use jumpman_sim::behaviors::g1;
use jumpman_sim::object::{ObjectBehaviorKind as K, SimObject};
use std::path::Path;

/// Names of a glb's animations, in file order (empty when the file has none).
pub fn glb_animation_names(path: &Path) -> Vec<String> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return vec![] };
    let mut head = [0u8; 20];
    if file.read_exact(&mut head).is_err() || &head[0..4] != b"glTF" {
        return vec![];
    }
    let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let mut json = vec![0u8; len];
    if file.read_exact(&mut json).is_err() {
        return vec![];
    }
    serde_json::from_slice::<serde_json::Value>(&json)
        .ok()
        .and_then(|v| {
            v["animations"].as_array().map(|a| a.iter().map(|c| c["name"].as_str().unwrap_or("").to_string()).collect())
        })
        .unwrap_or_default()
}

/// The decomp animation table (names in table order) of a group 1 actor.
pub fn anim_table(kind: K) -> &'static [&'static str] {
    match kind {
        K::PiranhaPlant => &g1::piranha_plant::ANIM_NAMES,
        K::Swoop => &g1::swoop::ANIM_NAMES,
        K::Scuttlebug => &g1::scuttlebug::ANIM_NAMES,
        K::Spindrift => &g1::spindrift::ANIM_NAMES,
        K::Mips => &g1::mips::ANIM_NAMES,
        K::MontyMole => &g1::monty_mole::ANIM_NAMES,
        K::Butterfly => &g1::butterfly::BUTTERFLY_ANIM_NAMES,
        K::Bird => &g1::butterfly::BIRD_ANIM_NAMES,
        _ => &[],
    }
}

/// Select the clip for the object's current decomp animation and hold it at the sim's frame
/// (`animFrame`, 30 fps clips).
pub fn drive_rig(rig: &mut AnimRig, o: &SimObject) {
    let table = anim_table(o.behavior);
    if let Some(name) = table.get(o.var("animation") as usize) {
        if let Some(i) = rig.names.iter().position(|n| n == name) {
            rig.want = i;
        }
    }
    if !table.is_empty() {
        rig.seek = Some((o.var("animFrame").max(0.0) / 30.0) as f32);
        rig.looped = false;
    }
}
