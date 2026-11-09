//! Dungeon doors of the exported OoT dungeons: the `Door_Shutter` / `En_Door` actors of the placement file's `mechanisms`
//! become collision boxes while shut (`jumpman_sim::oot_doors` decides when they open: clear rooms, switch flags, small
//! keys spent on the first try, the boss key). A shut door is a wall; opening removes it from the collision world.
use crate::game::course_data::CollisionData;
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use bevy::prelude::*;
use jumpman_sim::oot_doors::{door_from_actor, DoorEvent, DoorKind, Doors, U};

#[derive(Resource, Default)]
pub struct DungeonDoors {
    pub doors: Doors,
    colliders: Vec<super::dungeon::Collider>,
    /// `course:area` the doors were built for.
    key: String,
    /// Doors whose collider is currently in the world.
    placed: Vec<bool>,
}

/// A closed door's collision: a box 110 wide, 160 high, 24 deep (OoT units, scaled).
pub fn door_box() -> CollisionData {
    let (hw, h, ht) = (55.0 * U, 160.0 * U, 12.0 * U);
    let vertices = vec![[-hw, 0.0, -ht], [hw, 0.0, -ht], [hw, h, -ht], [-hw, h, -ht], [-hw, 0.0, ht], [hw, 0.0, ht], [hw, h, ht], [-hw, h, ht]];
    let t = |a: i32, b: i32, c: i32| [a, b, c, 0, 0];
    let triangles = vec![t(4, 5, 6), t(4, 6, 7), t(1, 0, 3), t(1, 3, 2), t(1, 6, 5), t(1, 2, 6), t(0, 4, 7), t(0, 7, 3), t(3, 7, 6), t(3, 6, 2)];
    CollisionData { rooms: vec![], vertices, triangles, hookable: None }
}

impl DungeonDoors {
    pub fn build(mechanisms: &[crate::zelda_enemies::Mechanism]) -> Self {
        let list: Vec<_> = mechanisms.iter().filter_map(|m| door_from_actor(&m.actor, m.param, m.pos, m.rot[1] * 180.0 / 32768.0)).filter(|d| d.kind != DoorKind::Free).collect();
        let n = list.len();
        Self { doors: Doors::new(list), colliders: (0..n).map(|_| super::dungeon::Collider::new(door_box(), 0.0)).collect(), key: String::new(), placed: vec![false; n] }
    }

    /// Put every closed door's box in the world and take the open ones out.
    pub fn sync(&mut self, world: &mut jumpman_sim::collision::CollisionWorld) {
        for (i, d) in self.doors.doors.iter().enumerate() {
            let want = !d.open;
            if self.placed[i] != want {
                self.colliders[i].set_yaw(d.yaw);
                self.colliders[i].move_to(world, want.then_some(d.pos));
                self.placed[i] = want;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_doors(
    mut doors: ResMut<DungeonDoors>,
    mut sim: ResMut<GameSim>,
    root: Res<AssetRoot>,
    en: Option<Res<crate::zelda_enemies::OotEnemies>>,
    props: Option<Res<crate::link::prop_world::PropWorld>>,
    mut toast: ResMut<crate::hud::Toast>,
) {
    let req = &sim.course.request;
    let key = format!("{}:{}", req.id, req.area);
    if doors.key != key {
        let mech = crate::zelda_enemies::load_placements(&root.0, &req.id, req.area).map(|p| p.mechanisms).unwrap_or_default();
        *doors = DungeonDoors::build(&mech);
        doors.key = key;
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        doors.sync(world);
    }
    if doors.doors.doors.is_empty() {
        return;
    }
    let player = sim.mario.pos;
    let link = sim.mario.link.enabled;
    let mut keys = sim.mario.link.inv.small_keys;
    let boss = sim.mario.link.inv.boss_key;
    let switch = |f: u8| props.as_ref().is_some_and(|p| p.props.flag(f as i32));
    let enemy_near = |pos: [f64; 3], r: f64| en.as_ref().is_some_and(|e| e.field.enemies.iter().any(|x| x.alive() && !x.asleep && (x.pos[0] - pos[0]).hypot(x.pos[2] - pos[2]) < r));
    let events = doors.doors.step(player, &mut keys, boss, &switch, &enemy_near);
    if link {
        sim.mario.link.inv.small_keys = keys;
    }
    for e in &events {
        match e {
            DoorEvent::Unlocked(_) => {
                toast.show("UNLOCKED", 1.2);
                crate::audio::oot_sfx::play("EV_SLIDE_DOOR_OPEN");
            }
            DoorEvent::Locked(_) => toast.show("IT'S LOCKED", 1.0),
            DoorEvent::Opened(_) => {
                crate::audio::oot_sfx::play("EV_SLIDE_DOOR_OPEN");
            }
        }
    }
    if !events.is_empty() {
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        doors.sync(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::CollisionWorld;

    #[test]
    fn a_shut_door_is_a_wall_and_an_open_one_is_not() {
        let mech = vec![crate::zelda_enemies::Mechanism { actor: "ACTOR_DOOR_SHUTTER".into(), param: 11 << 6, pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0] }];
        let mut d = DungeonDoors::build(&mech);
        assert_eq!(d.doors.doors.len(), 1);
        let mut world = CollisionWorld::default();
        d.sync(&mut world);
        // Walk at the door from +z: the wall stops Link inside the box.
        let mut p = [0.0, 50.0, 40.0];
        let hit = world.resolve_and_return_wall_collisions(&mut p, 30.0, 50.0);
        assert!(hit.is_some(), "the closed door pushes back");
        assert!(p[2] > 0.0, "pushed out the +z side: {p:?}");
        let mut keys = 1;
        let ev = d.doors.step([0.0, 0.0, 100.0], &mut keys, false, &|_| false, &|_, _| false);
        assert_eq!(ev, vec![DoorEvent::Unlocked(0)]);
        d.sync(&mut world);
        let mut p = [0.0, 50.0, 40.0];
        assert!(world.resolve_and_return_wall_collisions(&mut p, 30.0, 50.0).is_none(), "the open door lets Link through");
    }

    #[test]
    fn dungeon_chests_hold_keys() {
        use crate::zelda::save::treasure_reward;
        assert_eq!(treasure_reward(0x42).unwrap().key, 1);
        assert_eq!(treasure_reward(0x3f).unwrap().key, 2);
        assert_eq!(treasure_reward(0x4c).unwrap().key, 0);
    }

    #[test]
    fn every_chest_of_the_three_dungeons_can_be_opened() {
        use crate::zelda::save::treasure_reward;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/courses");
        for course in ["dodongos_cavern", "jabu_jabu", "forest_temple"] {
            let Ok(text) = std::fs::read_to_string(root.join(format!("{course}.json"))) else { continue };
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            for c in v["kokiri"]["treasureChests"].as_array().unwrap() {
                let item = c["item"].as_u64().unwrap() as u32;
                assert!(treasure_reward(item).is_some(), "{course}: chest item {item:#x} has no reward");
            }
        }
        let mut inv = jumpman_sim::link::items::Inventory::default();
        (treasure_reward(0x32).unwrap().grant.unwrap())(&mut inv);
        assert_eq!(inv.upgrades.cap(jumpman_sim::link::items::Ammo::Bomb), 20);
        (treasure_reward(0x04).unwrap().grant.unwrap())(&mut inv);
        assert!(inv.has(jumpman_sim::link::items::Item::Bow));
    }
}
