//! Mario's model (`Sm64MarioView.ts`): the exported glb carries one clip per SM64 animation id;
//! the sim-side [`Sm64AnimationPlayer`](crate::game::sm64_animation::Sm64AnimationPlayer) picks
//! the clip + source frame each 30 Hz tick and this view seeks to it. Position and yaw are
//! interpolated between ticks.
//!
//! TODO(mario-variants): hand/cap/eye `GEO_SWITCH_CASE` swaps (alternate display lists live in
//! `actors/mario/variants/*`), metal/vanish materials, teleport fade and 0.06 s clip blending.
use super::{glb_animation_count, scene, to_bevy, AssetRoot};
use crate::game::runner::{GameSim, MARIO_MODEL};
use crate::game::sm64_animation::{visual_rotation, AnimInput};
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::*;

pub struct MarioViewPlugin;

impl Plugin for MarioViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_mario).add_systems(Update, (place_mario, animate_mario, toggle_wings).chain().after(crate::CameraSet));
    }
}

#[derive(Component)]
pub struct MarioRoot;
#[derive(Component)]
struct MarioAnimator;
#[derive(Component)]
struct MarioWings;

#[derive(Resource)]
struct MarioClips {
    graph: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
}

pub(crate) fn spawn_mario(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut graphs: ResMut<Assets<AnimationGraph>>, sim: Res<GameSim>) {
    // Animation index == SM64 animation id.
    let count = glb_animation_count(&root.0.join(MARIO_MODEL)).max(1);
    let (graph, nodes) = AnimationGraph::from_clips((0..count).map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(MARIO_MODEL.to_owned()))));
    commands.insert_resource(MarioClips { graph: graphs.add(graph), nodes });
    commands
        .spawn((MarioRoot, scene(&assets, MARIO_MODEL), to_transform(&sim)))
        .observe(prepare_mario);
}

fn to_transform(sim: &GameSim) -> Transform {
    Transform::from_translation(to_bevy(sim.mario.pos))
}

fn prepare_mario(
    trigger: Trigger<SceneInstanceReady>,
    mut commands: Commands,
    clips: Res<MarioClips>,
    children: Query<&Children>,
    parts: Query<Option<&bevy::gltf::GltfExtras>>,
    players: Query<&AnimationPlayer>,
) {
    for child in children.iter_descendants(trigger.target()) {
        if players.contains(child) {
            commands.entity(child).insert((MarioAnimator, AnimationGraphHandle(clips.graph.clone())));
        }
        if let Ok(Some(extras)) = parts.get(child) {
            if extras.value.contains(r#""mario_cap_wings""#) {
                commands.entity(child).insert((MarioWings, Visibility::Hidden));
            }
        }
    }
}

fn place_mario(sim: Res<GameSim>, fixed: Res<Time<Fixed>>, mut mario: Single<&mut Transform, With<MarioRoot>>) {
    let t = fixed.overstep_fraction() as f64;
    let m = &sim.mario;
    let (pos, yaw) = sim.mario_at(t);
    let input = AnimInput::from_mario(m);
    let (rot_yaw, pitch, roll) = visual_rotation(&input);
    let mut p = to_bevy(pos);
    if m.riding_shell.is_some() {
        p.y += if m.action == ACT_RIDING_SHELL_GROUND { 0.45 } else { 0.42 };
    }
    mario.translation = p;
    mario.rotation = Quat::from_euler(EulerRot::YXZ, angle_to_radians(yaw) as f32 + rot_yaw as f32, pitch as f32, roll as f32);
    if m.action == ACT_RIDING_SHELL_GROUND {
        mario.rotation *= Quat::from_rotation_z(angle_to_radians(m.shell_roll) as f32);
    }
    mario.scale = if m.squish_timer > 0.0 { Vec3::new(1.2, 0.65, 1.2) } else { Vec3::ONE };
}

fn animate_mario(sim: Res<GameSim>, clips: Option<Res<MarioClips>>, mut players: Query<&mut AnimationPlayer, With<MarioAnimator>>) {
    let Some(clips) = clips else { return };
    let Some(&node) = clips.nodes.get(sim.pose.clip as usize) else { return };
    for mut player in &mut players {
        if !player.is_playing_animation(node) {
            player.stop_all();
            player.play(node).pause();
        }
        if let Some(active) = player.animation_mut(node) {
            active.seek_to(sim.pose.seconds);
        }
    }
}

fn toggle_wings(sim: Res<GameSim>, mut wings: Query<&mut Visibility, With<MarioWings>>) {
    let on = sim.mario.flags & MARIO_WING_CAP != 0;
    for mut v in &mut wings {
        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    }
}
