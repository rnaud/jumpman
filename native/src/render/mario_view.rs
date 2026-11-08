//! Mario's model (`Sm64MarioView.ts`): the exported glb carries one clip per SM64 animation id;
//! the sim-side [`Sm64AnimationPlayer`](crate::game::sm64_animation::Sm64AnimationPlayer) picks
//! the clip + source frame each 30 Hz tick and this view seeks to it. Position and yaw are
//! interpolated between ticks.
//!
//! TODO(mario-variants): hand/cap/eye `GEO_SWITCH_CASE` swaps (alternate display lists live in
//! `actors/mario/variants/*`), metal/vanish materials, teleport fade and 0.06 s clip blending.
use super::{glb_animation_count, scene, to_bevy, AssetRoot};
use crate::game::runner::{GameSim, MARIO_MODEL};
use crate::game::sm64_animation::Pose;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::*;

pub struct MarioViewPlugin;

impl Plugin for MarioViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_mario.in_set(crate::progress::PersistentSpawn)).add_systems(Update, (place_mario, animate_mario, toggle_wings).chain().after(crate::CameraSet));
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

fn spawn_mario(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut graphs: ResMut<Assets<AnimationGraph>>, sim: Res<GameSim>) {
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
    trigger: On<WorldInstanceReady>,
    mut commands: Commands,
    clips: Res<MarioClips>,
    children: Query<&Children>,
    parts: Query<Option<&bevy::gltf::GltfExtras>>,
    players: Query<&AnimationPlayer>,
) {
    for child in children.iter_descendants(trigger.event_target()) {
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

fn place_mario(sim: Res<GameSim>, fixed: Res<Time<Fixed>>, mut mario: Single<(&mut Transform, &mut Visibility), With<MarioRoot>>) {
    let (mut mario, mut vis) = mario.into_inner();
    let t = fixed.overstep_fraction() as f64;
    let m = &sim.mario;
    let (pos, yaw) = sim.mario_at(t);
    let (rot_yaw, mut pitch, mut roll) = sim.visual;
    if sim.visual_continuous {
        pitch = sim.prev_visual.1 + (sim.visual.1 - sim.prev_visual.1) * t;
        roll = sim.prev_visual.2 + (sim.visual.2 - sim.prev_visual.2) * t;
    }
    let mut p = to_bevy(pos);
    if m.riding_shell.is_some() {
        p.y += if m.action == ACT_RIDING_SHELL_GROUND { 0.45 } else { 0.42 };
    }
    mario.translation = p;
    mario.rotation = Quat::from_euler(EulerRot::YXZ, angle_to_radians(yaw) as f32 + rot_yaw as f32, pitch as f32, roll as f32);
    if m.action == ACT_RIDING_SHELL_GROUND {
        mario.rotation *= Quat::from_rotation_z(angle_to_radians(m.shell_roll) as f32);
    }
    mario.scale = if m.cut.scale != [1.0; 3] {
        // ACT_SQUISHED / ACT_BBH_ENTER_SPIN scale the model (`gfx.scale`).
        Vec3::new(m.cut.scale[0] as f32, m.cut.scale[1] as f32, m.cut.scale[2] as f32)
    } else if m.squish_timer > 0.0 {
        Vec3::new(1.2, 0.65, 1.2)
    } else {
        Vec3::ONE
    };
    // Actions that clear GRAPH_RENDER_ACTIVE (disappeared, intro, pipe emerge, Bubba, BBH).
    *vis = if m.cut.mario_visible { Visibility::Inherited } else { Visibility::Hidden };
}

/// Clip change cross-fade (`Sm64AnimationPlayer.update`: `fadeOut/fadeIn(0.06)`; idle->idle swaps cut).
#[derive(Default)]
struct ClipFade {
    clip: Option<u32>,
    last_seconds: f32,
    last_action: u32,
    out: Option<(AnimationNodeIndex, f32)>,
    t: f32,
}

const CROSSFADE: f32 = 0.06;

fn animate_mario(
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    clips: Option<Res<MarioClips>>,
    mut players: Query<&mut AnimationPlayer, With<MarioAnimator>>,
    mut fade: Local<ClipFade>,
) {
    let Some(clips) = clips else { return };
    let Some(&node) = clips.nodes.get(sim.pose.clip as usize) else { return };
    // Sample between keyframes at the render rate (the sim only advances the clock at 30 Hz).
    let seconds = Pose::seconds_at(&sim.prev_pose, &sim.pose, fixed.overstep_fraction() as f64);
    if fade.clip.is_some_and(|c| c != sim.pose.clip) {
        let idle_cut = sim.mario.action == ACT_IDLE && fade.last_action == ACT_IDLE;
        let old = fade.clip.and_then(|c| clips.nodes.get(c as usize).copied());
        fade.out = if idle_cut { None } else { old.map(|n| (n, fade.last_seconds)) };
        fade.t = 0.0;
    }
    fade.clip = Some(sim.pose.clip);
    fade.last_seconds = seconds;
    fade.last_action = sim.mario.action;
    fade.t += time.delta_secs();
    let k = (fade.t / CROSSFADE).min(1.0);
    if k >= 1.0 {
        fade.out = None;
    }
    for mut player in &mut players {
        if !player.is_playing_animation(node) {
            player.stop_all();
            player.play(node).pause();
            if let Some((old, at)) = fade.out {
                player.play(old).pause().seek_to(at);
            }
        }
        if let Some(active) = player.animation_mut(node) {
            active.seek_to(seconds);
            active.set_weight(if fade.out.is_some() { k } else { 1.0 });
        }
        match fade.out {
            Some((old, _)) => {
                if let Some(active) = player.animation_mut(old) {
                    active.set_weight(1.0 - k);
                }
            }
            None => {
                // Fade finished: drop the outgoing clip.
                for n in &clips.nodes {
                    if *n != node && player.is_playing_animation(*n) {
                        player.stop(*n);
                    }
                }
            }
        }
    }
}

fn toggle_wings(sim: Res<GameSim>, mut wings: Query<&mut Visibility, With<MarioWings>>) {
    let on = sim.mario.flags & MARIO_WING_CAP != 0;
    for mut v in &mut wings {
        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    }
}
