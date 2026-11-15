//! Super Smash Bros. stages: plays the moving scenery of a stage (`levels/ssb64_<stage>/stage.glb`, one looping clip
//! per animated DObj: Sector Z's passing ships, Yoshi's Island's flowers, Final Destination's backdrop, ...). The moving
//! platforms that carry collision are surface objects instead (`jumpman_sim::solids::ssb64`, drawn by
//! `objects::solids` at the sim's position).
use super::course_view::RoomModel;
use super::AssetRoot;
use crate::game::runner::GameSim;
use bevy::prelude::*;

pub struct Ssb64StagePlugin;

impl Plugin for Ssb64StagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, play_stage_clips);
    }
}

/// The stage scene's animation player got its graph and is playing every clip.
#[derive(Component)]
struct StageClips;

fn play_stage_clips(
    mut commands: Commands,
    sim: Res<GameSim>,
    root: Res<AssetRoot>,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<(Entity, &mut AnimationPlayer), Without<StageClips>>,
    parents: Query<&ChildOf>,
    rooms: Query<(), With<RoomModel>>,
) {
    let id = sim.course.request.id.as_str();
    if !crate::game::ssb64::is_stage(id) {
        return;
    }
    let model = format!("levels/{id}/stage.glb");
    for (entity, mut player) in &mut players {
        // only the stage model's own player
        let mut e = entity;
        let mut stage = false;
        while let Ok(p) = parents.get(e) {
            e = p.parent();
            if rooms.contains(e) {
                stage = true;
                break;
            }
        }
        if !stage {
            continue;
        }
        let count = crate::zelda::glb::animation_names(&root.0.join(&model)).len();
        let (graph, nodes) = AnimationGraph::from_clips(super::clips::clip_handles(&assets, &root, &model, count));
        commands.entity(entity).insert((AnimationGraphHandle(graphs.add(graph)), StageClips));
        for node in nodes {
            player.play(node).repeat();
        }
    }
}
