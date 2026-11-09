//! Icicles (`Bg_Ice_Turara`) and fire walls (`Bg_Hidan_Firewall`) of the exported dungeons: placed from the placement
//! file's `mechanisms`, simulated by `jumpman_sim::oot_hazards`, drawn as plain boxes (ice blue / flame orange).
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::prelude::*;
use jumpman_sim::oot_hazards::{FireWall, HazardEvent, Hazards, Icicle, IcicleState, U};

#[derive(Resource, Default)]
pub struct HazardWorld {
    pub hazards: Hazards,
    key: String,
}

#[derive(Component)]
pub enum HazardView {
    Icicle(usize),
    Wall(usize),
}

impl HazardWorld {
    pub fn build(mechanisms: &[crate::zelda_enemies::Mechanism]) -> Self {
        let mut h = Hazards::default();
        for m in mechanisms {
            match m.actor.as_str() {
                "ACTOR_BG_ICE_TURARA" => h.icicles.push(Icicle::new(m.pos)),
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
        let wall_mesh = meshes.add(Cuboid::new(200.0 * U as f32 * SCALE, 100.0 * U as f32 * SCALE, 240.0 * U as f32 * SCALE));
        for (i, w) in world.hazards.walls.iter().enumerate() {
            let mut tf = Transform::from_translation(to_bevy([w.pos[0], w.pos[1] + 50.0 * U, w.pos[2]]));
            tf.rotation = Quat::from_rotation_y(w.yaw.to_radians() as f32);
            commands.spawn((HazardView::Wall(i), Mesh3d(wall_mesh.clone()), MeshMaterial3d(fire.clone()), tf, Visibility::Visible));
        }
        return;
    }
    if world.hazards.icicles.is_empty() && world.hazards.walls.is_empty() {
        return;
    }
    let player = sim.mario.pos;
    let immune = sim.mario.fire_immune();
    let floor_world = sim.mario.world.clone();
    let floor = |p: [f64; 3]| {
        let hit = floor_world.find_floor(p[0], p[1] + 20.0, p[2]);
        hit.surface.is_some().then_some(hit.height)
    };
    let events = world.hazards.step(player, immune, &floor);
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
}
