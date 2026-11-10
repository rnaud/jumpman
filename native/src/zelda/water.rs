//! The Water Temple's water (`jumpman_sim::oot_water`): the main level that the Triforce crests move and the three pools that
//! follow a switch, applied to the water boxes of the collision world (`fx::water` draws whatever heights they have).
use crate::game::runner::GameSim;
use crate::link::prop_world::PropWorld;
use crate::render::AssetRoot;
use bevy::prelude::*;
use jumpman_sim::oot_water::{Out, WaterTemple};
use std::collections::HashMap;

const U: f64 = 4.4;

#[derive(Resource, Default)]
pub struct WaterWorld {
    pub temple: WaterTemple,
    key: String,
    clock: u32,
    last_sfx: HashMap<&'static str, u32>,
}

/// `(params, height)` of every `Bg_Mizu_Water` of a course.
pub fn placements(root: &std::path::Path, id: &str, area: i32) -> Vec<(i32, f64)> {
    crate::zelda_enemies::load_placements(root, id, area).map(|p| p.mechanisms).unwrap_or_default().iter().filter(|m| m.actor == "ACTOR_BG_MIZU_WATER").map(|m| (m.param & 0xffff, m.pos[1] / U)).collect()
}

fn apply(outs: &[Out], props: &mut Option<ResMut<PropWorld>>, w: &mut WaterWorld) {
    for o in outs {
        match *o {
            Out::Set(f) => {
                if let Some(p) = props.as_mut() {
                    p.props.set_switch(f, true);
                }
            }
            Out::Unset(f) => {
                if let Some(p) = props.as_mut() {
                    p.props.set_switch(f, false);
                }
            }
            Out::Sfx(name) => {
                // the actor replays its looping sound every frame
                if w.last_sfx.get(name).is_none_or(|t| w.clock.wrapping_sub(*t) >= 15) {
                    w.last_sfx.insert(name, w.clock);
                    crate::audio::oot_sfx::play(name);
                }
            }
        }
    }
}

pub fn step_water(mut w: ResMut<WaterWorld>, mut sim: ResMut<GameSim>, root: Res<AssetRoot>, props: Option<ResMut<PropWorld>>) {
    let req = sim.course.request.clone();
    let key = format!("{}:{}", req.id, req.area);
    let mut props = props;
    if w.key != key {
        let list = placements(&root.0, &req.id, req.area);
        let flag = |f: i32| props.as_ref().is_some_and(|p| p.props.flag(f));
        let (temple, outs) = WaterTemple::new(&list, &flag);
        w.temple = temple;
        w.key = key;
        apply(&outs, &mut props, &mut w);
    } else if w.temple.actors.is_empty() {
        return;
    } else {
        let outs = {
            let flag = |f: i32| props.as_ref().is_some_and(|p| p.props.flag(f));
            w.temple.tick(&flag)
        };
        w.clock = w.clock.wrapping_add(1);
        apply(&outs, &mut props, &mut w);
    }
    // the surfaces
    let boxes = w.temple.boxes();
    if boxes.iter().any(|(i, y)| sim.mario.world.water_boxes.get(*i).is_some_and(|b| (b.5 - y * U).abs() > 1e-9)) {
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        for (i, y) in boxes {
            if let Some(b) = world.water_boxes.get_mut(i) {
                b.5 = y * U;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exported_water_temple_has_one_main_level_three_pools_and_the_boxes_they_move() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let list = placements(&root, "water_temple", 1);
        if list.is_empty() {
            return eprintln!("no OoT data imported; skipped");
        }
        assert_eq!(list.len(), 11);
        let (t, _) = WaterTemple::new(&list, &|_| false);
        assert_eq!(t.actors.len(), 4, "the main level once and the pools of types 2, 3 and 4");
        // the main level starts at the top: 765 units = the exported boxes' 3366
        assert!((t.level().unwrap() * U - 3366.0).abs() < 1.0, "{:?}", t.level());
        let course: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("courses/water_temple.json")).unwrap()).unwrap();
        let boxes = course["waterBoxes"].as_array().unwrap();
        for (i, y) in t.boxes() {
            assert!(i < boxes.len(), "box {i} exists");
            let _ = y;
        }
        assert!((boxes[2][5].as_f64().unwrap() - 3366.0).abs() < 1.0, "box 2 is the main surface");
    }
}
