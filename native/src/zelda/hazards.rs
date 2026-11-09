//! Icicles (`Bg_Ice_Turara`) and fire walls (`Bg_Hidan_Firewall`) of the exported dungeons: placed from the placement
//! file's `mechanisms`, simulated by `jumpman_sim::oot_hazards`, drawn as plain boxes (ice blue / flame orange).
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::prelude::*;
use jumpman_sim::oot_hazards::{FireCurtain, FireWall, HazardEvent, Hazards, Icicle, IcicleState, Trap, TrapKind, U};

#[derive(Resource, Default)]
pub struct HazardWorld {
    pub hazards: Hazards,
    key: String,
}

#[derive(Component)]
pub enum HazardView {
    Icicle(usize),
    Wall(usize),
    Trap(usize),
    Curtain(usize),
}

impl HazardWorld {
    pub fn build(mechanisms: &[crate::zelda_enemies::Mechanism]) -> Self {
        let mut h = Hazards::default();
        for m in mechanisms {
            match m.actor.as_str() {
                "ACTOR_BG_ICE_TURARA" => h.icicles.push(Icicle::new(m.pos)),
                "ACTOR_BG_HAKA_TRAP" => {
                    if let Some(k) = TrapKind::from_params(m.param) {
                        h.traps.push(Trap::new(m.pos, k, 0).with_yaw(m.rot[1] * 180.0 / 32768.0));
                    }
                }
                "ACTOR_BG_HIDAN_CURTAIN" => {
                    if let Some(c) = FireCurtain::new(m.pos, m.param) {
                        h.curtains.push(c);
                    }
                }
                "ACTOR_BG_HIDAN_FIREWALL" => h.walls.push(FireWall::new(m.pos, m.rot[1] * 180.0 / 32768.0)),
                _ => {}
            }
        }
        Self { hazards: h, key: String::new() }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_hazards(
    mut commands: Commands,
    mut world: ResMut<HazardWorld>,
    mut sim: ResMut<GameSim>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    props: Option<Res<crate::link::prop_world::PropWorld>>,
    en: Option<Res<crate::zelda_enemies::OotEnemies>>,
    rooms: Res<super::rooms::SceneRooms>,
    mut views: Query<(Entity, &HazardView, &mut Transform, &mut Visibility)>,
) {
    let req = sim.course.request.clone();
    let key = format!("{}:{}", req.id, req.area);
    if world.key != key {
        for (e, ..) in &views {
            commands.entity(e).despawn();
        }
        let mech = crate::zelda_enemies::load_placements(&root.0, &req.id, req.area).map(|p| p.mechanisms).unwrap_or_default();
        *world = HazardWorld::build(&mech);
        world.key = key;
        let ice = materials.add(StandardMaterial { base_color: Color::srgba(0.65, 0.85, 1.0, 0.85), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.2, ..default() });
        let fire = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.45, 0.1, 0.7), alpha_mode: AlphaMode::Blend, emissive: LinearRgba::new(3.0, 1.0, 0.2, 1.0), ..default() });
        let (iw, ih) = (14.0 * U as f32 * SCALE, 60.0 * U as f32 * SCALE);
        let icicle_mesh = meshes.add(Cuboid::new(iw, ih, iw));
        for (i, ic) in world.hazards.icicles.iter().enumerate() {
            let tf = Transform::from_translation(to_bevy([ic.pos[0], ic.pos[1] - 30.0 * U, ic.pos[2]]));
            commands.spawn((HazardView::Icicle(i), Mesh3d(icicle_mesh.clone()), MeshMaterial3d(ice.clone()), tf, Visibility::Visible));
        }
        let trap = materials.add(StandardMaterial { base_color: Color::srgb(0.35, 0.12, 0.12), perceptual_roughness: 0.6, ..default() });
        let trap_mesh = meshes.add(Cuboid::new(60.0 * U as f32 * SCALE, 100.0 * U as f32 * SCALE, 60.0 * U as f32 * SCALE));
        for (i, t) in world.hazards.traps.iter().enumerate() {
            let tf = Transform::from_translation(to_bevy([t.pos[0], t.pos[1] + 50.0 * U, t.pos[2]]));
            commands.spawn((HazardView::Trap(i), Mesh3d(trap_mesh.clone()), MeshMaterial3d(trap.clone()), tf, Visibility::Visible));
        }
        let cur_mesh = meshes.add(Cuboid::new(162.0 * U as f32 * SCALE, 144.0 * U as f32 * SCALE, 162.0 * U as f32 * SCALE));
        for (i, c) in world.hazards.curtains.iter().enumerate() {
            let tf = Transform::from_translation(to_bevy([c.pos[0], c.pos[1] + 72.0 * U, c.pos[2]]));
            commands.spawn((HazardView::Curtain(i), Mesh3d(cur_mesh.clone()), MeshMaterial3d(fire.clone()), tf, Visibility::Visible));
        }
        let wall_mesh = meshes.add(Cuboid::new(200.0 * U as f32 * SCALE, 100.0 * U as f32 * SCALE, 240.0 * U as f32 * SCALE));
        for (i, w) in world.hazards.walls.iter().enumerate() {
            let mut tf = Transform::from_translation(to_bevy([w.pos[0], w.pos[1] + 50.0 * U, w.pos[2]]));
            tf.rotation = Quat::from_rotation_y(w.yaw.to_radians() as f32);
            commands.spawn((HazardView::Wall(i), Mesh3d(wall_mesh.clone()), MeshMaterial3d(fire.clone()), tf, Visibility::Visible));
        }
        return;
    }
    if world.hazards.icicles.is_empty() && world.hazards.walls.is_empty() && world.hazards.traps.is_empty() && world.hazards.curtains.is_empty() {
        return;
    }
    let player = sim.mario.pos;
    // A sword blow (or any swing in reach) knocks a hanging icicle loose.
    if sim.mario.link.enabled && sim.mario.link.striking(&sim.mario) {
        let reach = sim.mario.link.weapon().reach() + 60.0 * U;
        let hit: Vec<usize> = world
            .hazards
            .icicles
            .iter()
            .enumerate()
            .filter(|(_, ic)| ic.state == IcicleState::Hanging && (ic.pos[0] - player[0]).hypot(ic.pos[2] - player[2]) < reach && ic.pos[1] - player[1] < 260.0 * U && ic.pos[1] > player[1])
            .map(|(i, _)| i)
            .collect();
        for i in hit {
            world.hazards.strike_icicle(i);
        }
    }
    let immune = sim.mario.fire_immune();
    let floor_world = sim.mario.world.clone();
    let floor = |p: [f64; 3]| {
        let hit = floor_world.find_floor(p[0], p[1] + 20.0, p[2]);
        hit.surface.is_some().then_some(hit.height)
    };
    let switch = |f: u8| props.as_ref().is_some_and(|p| p.props.flag(f as i32));
    // `Flags_GetClear(room)`: the enemies of Link's current room are all dead (the whole scene when the room is unknown)
    let cleared = if rooms.current.is_some() { rooms.cleared } else { en.as_ref().is_none_or(|e| !e.field.enemies.iter().any(|x| x.alive() && !x.asleep)) };
    let mut events = world.hazards.step(player, immune, &floor);
    events.extend(world.hazards.step_curtains(player, immune, &switch, cleared));
    for e in &events {
        match e {
            HazardEvent::Hurt { units } => {
                jumpman_sim::oot_enemy::player::hurt_player(&mut sim.mario, *units, player);
            }
            HazardEvent::IcicleShattered(_) => {
                crate::audio::oot_sfx::play("EV_ICE_BROKEN");
            }
            HazardEvent::IcicleDropped(_) => {}
        }
    }
    for (_, v, mut tf, mut vis) in &mut views {
        match v {
            HazardView::Icicle(i) => {
                if let Some(ic) = world.hazards.icicles.get(*i) {
                    tf.translation = to_bevy([ic.pos[0], ic.pos[1] - 30.0 * U, ic.pos[2]]);
                    *vis = if ic.state == IcicleState::Broken { Visibility::Hidden } else { Visibility::Visible };
                }
            }
            HazardView::Wall(_) => {}
            HazardView::Curtain(i) => {
                if let Some(c) = world.hazards.curtains.get(*i) {
                    tf.translation = to_bevy([c.pos[0], c.pos[1] - c.dims().2 * U + c.rise + c.dims().1 * U / 2.0, c.pos[2]]);
                    *vis = if c.gone() || c.rise <= 0.0 { Visibility::Hidden } else { Visibility::Visible };
                }
            }
            HazardView::Trap(i) => {
                if let Some(t) = world.hazards.traps.get(*i) {
                    *vis = if t.deadly(world.hazards.clock) { Visibility::Visible } else { Visibility::Hidden };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hazards_come_from_the_placed_actors() {
        let m = |a: &str| crate::zelda_enemies::Mechanism { actor: a.into(), param: 0, pos: [0.0; 3], rot: [0.0, 16384.0, 0.0] };
        let w = HazardWorld::build(&[m("ACTOR_BG_ICE_TURARA"), m("ACTOR_BG_ICE_TURARA"), m("ACTOR_BG_HIDAN_FIREWALL"), m("ACTOR_OBJ_TSUBO")]);
        assert_eq!((w.hazards.icicles.len(), w.hazards.walls.len()), (2, 1));
        assert!((w.hazards.walls[0].yaw - 90.0).abs() < 1e-9);
    }

    #[test]
    fn the_fire_temples_hammerable_totems_are_hammer_props_with_switch_flags() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/courses/fire_temple.json");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let totems: Vec<_> = v["props"].as_array().unwrap().iter().filter(|p| p["k"] == "hammer_rock").collect();
        assert!(totems.len() >= 5, "{} totems", totems.len());
        assert!(totems.iter().all(|p| p["flag"].as_i64().is_some_and(|f| (0..64).contains(&f))));
    }
}
