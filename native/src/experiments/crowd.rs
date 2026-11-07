//! `crowd.ts`: one autonomous Mario = a full [`MarioRuntime`] fed by synthetic controller input,
//! rendered with the real SM64 rig and its native animation clips.
//!
//! The TS batches the crowd into instanced meshes (`CrowdBatcher`); Bevy already batches
//! identical meshes/materials, so every Mario is one skinned `SceneRoot`. Cap/overalls are
//! tinted by cloning the two Mario display-list materials per colour (cached), and distant
//! Marios animate at a reduced rate (the animation clock still advances every tick).
use crate::game::runner::MARIO_MODEL;
use crate::game::sm64_animation::{read_clip_meta, visual_rotation, AnimInput, ClipMeta, Pose, Sm64AnimationPlayer};
use crate::render::{glb_animation_count, scene, to_bevy, AssetRoot};
use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::math::{angle_to_radians, coss, sins, to_s16, Vec3 as V3};
use jumpman_sim::state::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Mario's red (cap/shirt) and blue (overalls) display-list materials in the exported rig.
const CAP_MATERIAL: &str = "[1.0, 0.0, 0.0]";
const OVERALLS_MATERIAL: &str = "[0.0, 0.0, 1.0]";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrowdLook {
    pub cap: u32,
    pub overalls: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct DriveInput {
    /// World yaw (SM64 s16) to run toward, or None to let go of the stick.
    pub yaw: Option<f64>,
    /// Stick magnitude 0..64 (default 64).
    pub mag: Option<f64>,
    /// Press A this frame.
    pub jump: bool,
    /// Keep A held (higher jumps).
    pub hold_a: bool,
    pub punch: bool,
    pub crouch: bool,
    /// Keep Z held (crouch, backflip set-up).
    pub hold_z: bool,
}

/// One autonomous Mario.
pub struct CrowdMario {
    pub runtime: MarioRuntime,
    pub entity: Option<Entity>,
    anim: Option<Sm64AnimationPlayer>,
    pub pose: Option<Pose>,
    pub prev_pos: V3,
    prev_yaw: f64,
    skip: u32,
    debt: f64,
    was_air: bool,
    /// Frames since the agent last touched the ground after being airborne.
    pub landed_ticks: u32,
}

impl CrowdMario {
    pub fn new(world: Arc<CollisionWorld>, spawn: SpawnPoint, clip_meta: Option<&[ClipMeta]>) -> Self {
        let runtime = MarioRuntime::new(world, spawn, -1600.0);
        let mut me = Self {
            prev_pos: runtime.pos,
            prev_yaw: runtime.face_angle[1],
            runtime,
            entity: None,
            anim: clip_meta.map(|m| Sm64AnimationPlayer::new(m.to_vec())),
            pose: None,
            skip: 0,
            debt: 0.0,
            was_air: false,
            landed_ticks: 99,
        };
        me.update_pose();
        me
    }

    pub fn pos(&self) -> V3 {
        self.runtime.pos
    }

    pub fn update_pose(&mut self) {
        if let Some(a) = self.anim.as_mut() {
            self.pose = Some(a.update(&AnimInput::from_mario(&self.runtime), 1.0 / 30.0));
        }
    }

    pub fn sync_previous(&mut self) {
        self.prev_pos = self.runtime.pos;
        self.prev_yaw = self.runtime.face_angle[1];
    }

    pub fn teleport(&mut self, pos: V3, face_yaw: Option<f64>) {
        let yaw = face_yaw.unwrap_or(self.runtime.face_angle[1]);
        self.runtime.spawn = SpawnPoint { action: None, pos, face_yaw: yaw };
        self.runtime.reset_to_spawn();
        self.sync_previous();
        self.update_pose();
    }

    /// Advance one 30 Hz frame with synthetic stick and buttons.
    pub fn drive(&mut self, input: &DriveInput) {
        let mag = if input.yaw.is_none() { 0.0 } else { input.mag.unwrap_or(64.0).clamp(0.0, 64.0) };
        let yaw = input.yaw.unwrap_or(0.0);
        let (mut down, mut pressed) = (0, 0);
        if input.jump {
            pressed |= A_BUTTON;
            down |= A_BUTTON;
        }
        if input.hold_a {
            down |= A_BUTTON;
        }
        if input.punch {
            pressed |= B_BUTTON;
            down |= B_BUTTON;
        }
        if input.crouch {
            pressed |= Z_TRIG;
            down |= Z_TRIG;
        }
        if input.hold_z {
            down |= Z_TRIG;
        }
        let controller = ControllerSnapshot {
            // With a zero camera yaw, intended_yaw = atan2s(-stick_y, stick_x) = yaw.
            stick_x: mag * sins(yaw),
            stick_y: -mag * coss(yaw),
            stick_mag: mag,
            button_down: down,
            button_pressed: pressed,
            ..Default::default()
        };
        self.prev_pos = self.runtime.pos;
        self.prev_yaw = self.runtime.face_angle[1];
        self.runtime.step(&controller, 0.0, false, false);
        let air = self.runtime.action & ACT_FLAG_AIR != 0;
        if self.was_air && !air {
            self.landed_ticks = 0;
        } else if self.landed_ticks < 99 {
            self.landed_ticks += 1;
        }
        self.was_air = air;
        // Crowd Marios never die; knockbacks still play out in full.
        self.runtime.hurt_counter = 0.0;
        self.runtime.health = 0x880 as f64;
        self.update_pose();
    }

    /// Interpolated position and yaw between the last two ticks.
    pub fn interpolated(&self, alpha: f64) -> (V3, f64) {
        let p = self.runtime.pos;
        let pos = [0, 1, 2].map(|i| self.prev_pos[i] + (p[i] - self.prev_pos[i]) * alpha);
        let yaw = to_s16(self.prev_yaw + to_s16(self.runtime.face_angle[1] - self.prev_yaw) as f64 * alpha) as f64;
        (pos, yaw)
    }

    /// Push the interpolated transform and clip to the entity. Distant Marios animate at a
    /// reduced rate to keep a hundred rigs affordable.
    pub fn render(&mut self, commands: &mut Commands, alpha: f64, dt: f64, camera_distance: f64) {
        let Some(entity) = self.entity else { return };
        let (pos, yaw) = self.interpolated(alpha);
        self.debt += dt;
        let interval = if camera_distance > 9000.0 { 4 } else if camera_distance > 4500.0 { 2 } else { 1 };
        self.skip += 1;
        if self.skip < interval {
            commands.entity(entity).insert(Transform::from_translation(to_bevy(pos)).with_rotation(Quat::from_rotation_y(angle_to_radians(yaw) as f32)).with_scale(self.scale()));
            return;
        }
        self.skip = 0;
        self.debt = 0.0;
        let input = AnimInput::from_mario(&self.runtime);
        let (rot_yaw, pitch, roll) = visual_rotation(&input);
        let rotation = Quat::from_euler(EulerRot::YXZ, angle_to_radians(yaw) as f32 + rot_yaw as f32, pitch as f32, roll as f32);
        let mut e = commands.entity(entity);
        e.insert(Transform::from_translation(to_bevy(pos)).with_rotation(rotation).with_scale(self.scale()));
        if let Some(p) = self.pose {
            e.insert(CrowdClip { clip: p.clip, seconds: p.seconds });
        }
    }

    fn scale(&self) -> Vec3 {
        if self.runtime.squish_timer > 0.0 {
            Vec3::new(1.2, 0.65, 1.2)
        } else {
            Vec3::ONE
        }
    }
}

// ---------------------------------------------------------------- Bevy side

#[derive(Component)]
pub struct CrowdBody;
/// Cap/overalls colours for a crowd Mario's root entity (`0xRRGGBB`).
#[derive(Component, Clone, Copy)]
pub struct CrowdColors(pub CrowdLook);
/// Re-apply [`CrowdColors`] once the scene has loaded.
#[derive(Component)]
pub struct NeedsTint;
#[derive(Component)]
pub struct CrowdClip {
    pub clip: u32,
    pub seconds: f32,
}
#[derive(Component)]
pub struct CrowdAnimator(Entity);

/// Shared animation graph over all of Mario's clips (animation index == SM64 animation id).
#[derive(Resource)]
pub struct CrowdClips {
    graph: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
    pub meta: Vec<ClipMeta>,
}

impl CrowdClips {
    pub fn load(root: &AssetRoot, assets: &AssetServer, graphs: &mut Assets<AnimationGraph>) -> Option<Self> {
        let glb = std::fs::read(root.0.join(MARIO_MODEL)).ok()?;
        let count = glb_animation_count(&root.0.join(MARIO_MODEL)).max(1);
        let (graph, nodes) = AnimationGraph::from_clips((0..count).map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(MARIO_MODEL.to_owned()))));
        Some(Self { graph: graphs.add(graph), nodes, meta: read_clip_meta(&glb) })
    }
}

/// Load Mario's clips into the shared [`CrowdClips`] resource; returns the clip metadata the
/// crowd's animation players need.
pub fn init_clips(host: &mut super::Host) -> Option<Arc<Vec<ClipMeta>>> {
    let clips = CrowdClips::load(host.asset_root, host.assets, host.graphs)?;
    let meta = Arc::new(clips.meta.clone());
    host.commands.insert_resource(clips);
    Some(meta)
}

pub fn spawn_body(commands: &mut Commands, assets: &AssetServer, look: CrowdLook) -> Entity {
    commands.spawn((super::XpEntity, CrowdBody, CrowdColors(look), NeedsTint, scene(assets, MARIO_MODEL), Transform::default(), Visibility::Inherited)).id()
}

pub struct CrowdPlugin;

impl Plugin for CrowdPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (tint_crowd, link_animators, animate_crowd).chain());
    }
}

fn hex(c: u32) -> Color {
    Color::srgb(((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0)
}

#[allow(clippy::type_complexity)]
fn tint_crowd(
    mut commands: Commands,
    roots: Query<(Entity, &CrowdColors), With<NeedsTint>>,
    children: Query<&Children>,
    mut parts: Query<(&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<HashMap<(AssetId<StandardMaterial>, u32), Handle<StandardMaterial>>>,
    mut originals: Local<HashMap<Entity, Handle<StandardMaterial>>>,
    extras: Query<&bevy::gltf::GltfExtras>,
) {
    for (root, colors) in &roots {
        let mut found = false;
        for child in children.iter_descendants(root) {
            if extras.get(child).map_or(false, |e| e.value.contains("mario_cap_wings")) {
                commands.entity(child).insert(Visibility::Hidden);
            }
            let Ok((name, mut handle)) = parts.get_mut(child) else { continue };
            let color = match name.0.as_str() {
                CAP_MATERIAL => Some(colors.0.cap),
                OVERALLS_MATERIAL => colors.0.overalls,
                _ => continue,
            };
            found = true;
            // Remember the shared original so a later re-tint starts from it.
            let base = originals.entry(child).or_insert_with(|| handle.0.clone()).clone();
            let Some(color) = color else {
                handle.0 = base;
                continue;
            };
            let tinted = cache
                .entry((base.id(), color))
                .or_insert_with(|| {
                    let mut m = materials.get(&base).cloned().unwrap_or_default();
                    m.base_color = hex(color);
                    materials.add(m)
                })
                .clone();
            handle.0 = tinted;
        }
        if found {
            commands.entity(root).remove::<NeedsTint>();
        }
    }
}

fn link_animators(
    mut commands: Commands,
    clips: Option<Res<CrowdClips>>,
    players: Query<Entity, (With<AnimationPlayer>, Without<CrowdAnimator>, Without<crate::render::mario_view::MarioRoot>)>,
    parents: Query<&ChildOf>,
    bodies: Query<(), With<CrowdBody>>,
) {
    let Some(clips) = clips else { return };
    for player in &players {
        let mut e = player;
        while let Ok(p) = parents.get(e) {
            e = p.parent();
            if bodies.contains(e) {
                commands.entity(player).insert((CrowdAnimator(e), AnimationGraphHandle(clips.graph.clone())));
                break;
            }
        }
    }
}

fn animate_crowd(clips: Option<Res<CrowdClips>>, mut players: Query<(&mut AnimationPlayer, &CrowdAnimator)>, roots: Query<&CrowdClip>) {
    let Some(clips) = clips else { return };
    for (mut player, animator) in &mut players {
        let Ok(clip) = roots.get(animator.0) else { continue };
        let Some(&node) = clips.nodes.get(clip.clip as usize) else { continue };
        if !player.is_playing_animation(node) {
            player.stop_all();
            player.play(node).pause();
        }
        if let Some(active) = player.animation_mut(node) {
            active.seek_to(clip.seconds);
        }
    }
}
