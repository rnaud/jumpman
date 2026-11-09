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

/// The drawn slab of a closed door.
#[derive(Component)]
pub struct DoorView(pub usize);

/// Slab colour by kind: gold small-key doors, red boss door, grey slide doors (clear / switch).
fn door_color(kind: DoorKind) -> Color {
    match kind {
        DoorKind::SmallKey => Color::srgb(0.78, 0.62, 0.18),
        DoorKind::BossKey => Color::srgb(0.65, 0.12, 0.10),
        _ => Color::srgb(0.45, 0.46, 0.50),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_doors(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut views: Query<(Entity, &DoorView, &mut Visibility)>,
    mut doors: ResMut<DungeonDoors>,
    mut heat: Local<jumpman_sim::oot_heat::Heat>,
    fixed: Res<Time<Fixed>>,
    mut sim: ResMut<GameSim>,
    root: Res<AssetRoot>,
    en: Option<Res<crate::zelda_enemies::OotEnemies>>,
    props: Option<Res<crate::link::prop_world::PropWorld>>,
    mut toast: ResMut<crate::hud::Toast>,
) {
    let course_id = sim.course.request.id.clone();
    let req = &sim.course.request;
    let key = format!("{}:{}", req.id, req.area);
    if doors.key != key {
        let mech = crate::zelda_enemies::load_placements(&root.0, &req.id, req.area).map(|p| p.mechanisms).unwrap_or_default();
        *doors = DungeonDoors::build(&mech);
        doors.key = key;
        for (e, _, _) in &views {
            commands.entity(e).despawn();
        }
        for (i, d) in doors.doors.doors.iter().enumerate() {
            let (w, h, t) = (110.0 * U as f32 * crate::render::SCALE, 160.0 * U as f32 * crate::render::SCALE, 24.0 * U as f32 * crate::render::SCALE);
            let mut tf = Transform::from_translation(crate::render::to_bevy([d.pos[0], d.pos[1] + 80.0 * U, d.pos[2]]));
            tf.rotation = Quat::from_rotation_y(d.yaw.to_radians() as f32);
            commands.spawn((DoorView(i), Mesh3d(meshes.add(Cuboid::new(w, h, t))), MeshMaterial3d(materials.add(StandardMaterial { base_color: door_color(d.kind), perceptual_roughness: 0.8, ..default() })), tf, Visibility::Visible));
        }
        let world = std::sync::Arc::make_mut(&mut sim.mario.world);
        doors.sync(world);
    }
    for (_, v, mut vis) in &mut views {
        *vis = if doors.doors.doors.get(v.0).is_some_and(|d| !d.open) { Visibility::Visible } else { Visibility::Hidden };
    }
    // The Fire Temple's heat: a quarter heart every 6 s without the Goron Tunic.
    if sim.mario.link.enabled {
        let units = heat.step(fixed.delta_secs_f64(), jumpman_sim::oot_heat::is_hot_course(&course_id), sim.mario.link.inv.tunic);
        if units > 0 {
            let at = sim.mario.pos;
            jumpman_sim::oot_enemy::player::hurt_player(&mut sim.mario, units, at);
            toast.show("IT'S HOT IN HERE - THE GORON TUNIC PROTECTS FROM HEAT", 2.0);
        }
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
