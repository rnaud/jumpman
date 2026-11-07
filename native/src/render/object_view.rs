//! Sim object presentation (`CourseObjects.syncViews`): one scene per [`SimObject`] with a
//! model, positioned from the interpolated sim snapshot, with the first glb clip looped.
//!
//! TODO(object-views): behavior-specific rigs from `courseObjects.ts` - chain chomp jaw frames,
//! koopa shell/bare swap and animation index tables, snowman pitch, racing-penguin floor
//! alignment, chest lid, wing/star box outlines and scale, cannon barrel pitch, collider
//! (DoorCollider) syncing, shadows.
use super::billboard::Billboard;
use super::{glb_animation_count, scene, to_bevy, AssetRoot};
use crate::game::runner::GameSim;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use jumpman_sim::math::angle_to_radians;
use std::collections::HashSet;

pub struct ObjectViewPlugin;

impl Plugin for ObjectViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectViews>().add_systems(Update, (spawn_object_views, sync_object_views).chain().before(crate::CameraSet));
    }
}

#[derive(Component)]
pub struct ObjectView(pub usize);

/// Loop the first clip of the glb once its scene exists.
#[derive(Component)]
struct LoopFirstClip(Handle<AnimationClip>);

#[derive(Resource, Default)]
struct ObjectViews {
    /// Entity per object arena index (None = no model).
    entities: Vec<Option<Entity>>,
    missing: HashSet<String>,
}

fn spawn_object_views(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, sim: Res<GameSim>, mut views: ResMut<ObjectViews>) {
    let objects = &sim.mario.objects.objects;
    // The arena shrinks when the manager resets; drop views of vanished dynamic objects.
    while views.entities.len() > objects.len() {
        if let Some(Some(e)) = views.entities.pop() {
            commands.entity(e).despawn();
        }
    }
    for idx in views.entities.len()..objects.len() {
        let mut entity = None;
        if let Some(model) = objects[idx].model.as_deref().filter(|m| !m.is_empty()) {
            if root.exists(model) {
                let mut e = commands.spawn((scene(&assets, model), Transform::default(), Visibility::Hidden, ObjectView(idx)));
                if glb_animation_count(&root.0.join(model)) > 0 {
                    e.insert(LoopFirstClip(assets.load(GltfAssetLabel::Animation(0).from_asset(model.to_owned()))));
                    e.observe(start_clip);
                }
                if objects[idx].billboard {
                    e.insert(Billboard::Full);
                }
                entity = Some(e.id());
            } else if views.missing.insert(model.to_owned()) {
                warn!("object model missing: {model}");
            }
        }
        views.entities.push(entity);
    }
}

fn start_clip(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    clip: Query<&LoopFirstClip>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
) {
    let Ok(clip) = clip.get(trigger.target()) else { return };
    let (graph, node) = AnimationGraph::from_clip(clip.0.clone());
    let graph = graphs.add(graph);
    for child in children.iter_descendants(trigger.target()) {
        if players.contains(child) {
            let mut player = AnimationPlayer::default();
            player.play(node).repeat();
            commands.entity(child).insert((player, AnimationGraphHandle(graph.clone())));
        }
    }
}

fn sync_object_views(sim: Res<GameSim>, fixed: Res<Time<Fixed>>, mut views: Query<(&ObjectView, &mut Transform, &mut Visibility, Has<Billboard>)>) {
    let t = fixed.overstep_fraction() as f64;
    let objects = &sim.mario.objects.objects;
    for (view, mut tf, mut vis, billboard) in &mut views {
        let Some(o) = objects.get(view.0) else { continue };
        let cur = sim.curr.objects.get(view.0).copied().unwrap_or_default();
        let prev = sim.prev.objects.get(view.0).copied().unwrap_or(cur);
        // Objects teleport (respawn, held, warps): only interpolate short moves.
        let near = (0..3).all(|i| (cur.pos[i] - prev.pos[i]).abs() < 300.0);
        let pos = if near { [0, 1, 2].map(|i| prev.pos[i] + (cur.pos[i] - prev.pos[i]) * t) } else { cur.pos };
        let mut p = pos;
        p[1] += o.graph_y_offset;
        tf.translation = to_bevy(p);
        if !billboard {
            tf.rotation = Quat::from_rotation_y(angle_to_radians(o.move_yaw) as f32);
        }
        tf.scale = Vec3::splat(o.scale as f32);
        *vis = if o.active && o.visible { Visibility::Inherited } else { Visibility::Hidden };
    }
}
