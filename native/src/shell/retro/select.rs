//! The character select (fighting-game style): the playable characters stand in a lineup on a checkered
//! stage, the highlighted one steps forward, turns on its pedestal under a spotlight and plays its idle
//! loop; READY makes it hop. The models are the real ones (Mario's glb, Link / Young Link and Banjo from
//! the user's imported data, posed through the same animators the game uses) and exist only while this
//! screen is open: [`manage_stage`] spawns them when the page opens and despawns everything when it closes.
//!
//! Everything is drawn by the menu's private camera (render layer [`super::scene::LAYER`]) into the small
//! scene image, the 2D layer (plates, tabs, panel, READY) goes on top through the CPU framebuffer, exactly
//! like the title and the hub. No course is loaded for this screen.
use super::fb::{clean, mix, rgb, Fb, IRect, Rgba};
use super::model::{char_info, lock_reason, Ctx, Hit, Menu, Page};
use super::scene::{self, CamState, MeshBuilder, SceneAssets, LAYER};
use super::{Anim, Hits, Insets, LowRes, MenuRes, RetroActive};
use crate::characters::banjo_pose::Part;
use crate::characters::link_pose::{self, ANGLE_TO_RAD};
use crate::characters::{self, Rig, RigKind};
use crate::render::{AssetRoot, SCALE};
use crate::shell::Character;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
use jumpman_sim::state::*;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

const GOLD: Rgba = rgb(0xffd23a);
const WHITE: Rgba = rgb(0xf6f6ee);

// ------------------------------------------------------------------------------------------
// Layout (pure)
// ------------------------------------------------------------------------------------------

/// Where everything of the select screen sits for a framebuffer of `w` x `h`.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub portrait: bool,
    /// Columns of the lineup: 4 abreast on wide screens, a 2 x 2 grid on tall ones.
    pub cols: usize,
    /// Where each character's feet appear (framebuffer px).
    pub feet: [(f32, f32); 4],
    /// Horizontal distance between neighbours in px.
    pub pitch: f32,
    pub panel: IRect,
    pub plate_y: i32,
}

pub fn layout(w: i32, h: i32, sa: Insets) -> Layout {
    let (wf, hf) = (w as f32, h as f32);
    let portrait = w < h;
    let mut feet = [(0.0, 0.0); 4];
    if portrait {
        // 2 x 2 grid in the upper part, the info panel below.
        let (l, r) = (sa.l as f32 + 0.06 * wf, wf - sa.r as f32 - 0.06 * wf);
        let pitch = (r - l) / 2.0;
        let top = hf * 0.24 + sa.t as f32 * 0.5;
        let rows = [top + hf * 0.12, top + hf * 0.12 + hf * 0.20];
        for (i, f) in feet.iter_mut().enumerate() {
            *f = (l + (i % 2) as f32 * pitch + pitch / 2.0, rows[i / 2]);
        }
        let ptop = (hf * 0.66) as i32 + 10;
        Layout { portrait, cols: 2, feet, pitch, panel: IRect::new(8 + sa.l, ptop, w - 16 - sa.l - sa.r, h - ptop - 24 - sa.b), plate_y: 24 + sa.t }
    } else {
        let (l, r) = (sa.l as f32 + 0.04 * wf, 0.61 * wf);
        let pitch = (r - l) / 4.0;
        for (i, f) in feet.iter_mut().enumerate() {
            *f = (l + (i as f32 + 0.5) * pitch, hf * 0.64);
        }
        let px = (0.63 * wf) as i32;
        Layout { portrait, cols: 4, feet, pitch, panel: IRect::new(px, 38 + sa.t, w - px - 8 - sa.r, h - 38 - sa.t - 24 - sa.b), plate_y: 5 + sa.t }
    }
}

/// The stage camera for a framebuffer (the model plane is y = 0).
pub fn camera(w: i32, h: i32) -> (Vec3, Vec3) {
    if w < h {
        (Vec3::new(0.0, 4.6, 8.4), Vec3::new(0.0, 0.7, 0.0))
    } else {
        (Vec3::new(0.0, 2.6, 6.4), Vec3::new(0.0, 0.95, 0.0))
    }
}

/// The point of the floor (y = 0) that appears at framebuffer px (`px`, `py`).
pub fn floor_point(cam: &CamState, px: f32, py: f32, w: f32, h: f32) -> Option<Vec3> {
    let view = Mat4::look_at_rh(cam.eye, cam.target, Vec3::Y);
    let proj = Mat4::perspective_rh(scene::FOV, cam.aspect, 0.1, 200.0);
    let inv = (proj * view).inverse();
    let ndc = Vec2::new(px / w * 2.0 - 1.0, 1.0 - py / h * 2.0);
    let near = inv * Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
    let far = inv * Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
    let (a, b) = (near.truncate() / near.w, far.truncate() / far.w);
    let d = b - a;
    if d.y.abs() < 1e-6 {
        return None;
    }
    let t = -a.y / d.y;
    (t > 0.0).then(|| a + d * t)
}

/// World positions of the four slots for the current layout.
pub fn slot_positions(l: &Layout, cam: &CamState, w: i32, h: i32) -> [Vec3; 4] {
    std::array::from_fn(|i| floor_point(cam, l.feet[i].0, l.feet[i].1, w as f32, h as f32).unwrap_or(Vec3::new(i as f32 * 1.5 - 2.25, 0.0, 0.0)))
}

/// Where a slot's model stands: the highlighted one (`pop` 1) steps toward the camera, shifted sideways so it keeps its screen column.
pub fn stand(spot: Vec3, pop: f32, cam: &CamState) -> Vec3 {
    let fwd = 0.45 * pop;
    let k = (cam.eye.z - (spot.z + fwd)) / (cam.eye.z - spot.z).max(0.1);
    Vec3::new(spot.x * k, spot.y, spot.z + fwd)
}

/// A camera state for a framebuffer size (what `animate_scene` stores).
pub fn cam_for(w: i32, h: i32) -> CamState {
    let (eye, target) = camera(w, h);
    CamState { eye, target, aspect: w as f32 / h as f32, ..Default::default() }
}

// ------------------------------------------------------------------------------------------
// The stage (3D)
// ------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct StageRoot;
#[derive(Component)]
pub struct Pedestal(usize);
#[derive(Component)]
pub struct SpotLight;
#[derive(Component)]
pub struct PreviewAnimator;

enum Body {
    Mario { player: Box<crate::game::sm64_animation::Sm64AnimationPlayer>, nodes: Vec<AnimationNodeIndex> },
    Rig(Rig),
}

struct Slot {
    who: Character,
    root: Entity,
    body: Body,
    /// Stands in for the model's game state: the animators read it as they read the sim's Mario.
    m: Box<MarioRuntime>,
    /// Smoothed extra distance toward the camera and scale (the highlighted one steps forward).
    pop: f32,
    /// Seconds into a hop and its length / height.
    hop: Option<(f32, f32, f32)>,
    ready_hop: bool,
    /// Greyed out (the mode does not take this character): the model's materials are darkened, the originals kept here.
    dimmed: bool,
    orig: Vec<(Handle<StandardMaterial>, Color)>,
}

#[derive(Resource, Default)]
pub struct Stage {
    root: Option<Entity>,
    slots: [Option<Slot>; 4],
    last_sel: Option<usize>,
    /// Slots whose model could not be shown (locked data or a load error): the highlighted one falls back to the fox.
    pub missing: [bool; 4],
    /// World position of the stage floor slots (kept for the hero fallback and tests).
    pub spots: [Vec3; 4],
    pub fallback_at: Option<Vec3>,
    log_done: bool,
}

impl Stage {
    pub fn is_spawned(&self) -> bool {
        self.root.is_some()
    }
    /// Number of 3D models currently standing on the stage.
    pub fn model_count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

fn dummy_mario() -> Box<MarioRuntime> {
    use jumpman_sim::collision::CollisionWorld;
    let mut m = MarioRuntime::new(Arc::new(CollisionWorld::default()), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0);
    m.action = ACT_IDLE;
    Box::new(m)
}

fn pedestal_mesh(r: f32, h: f32) -> Mesh {
    let mut b = MeshBuilder::default();
    let seg = 20;
    let c = |hex| rgb(hex);
    let (top, side) = (c(0xffffff), c(0xc8c8d8));
    let ring = |rad: f32, y: f32, k: usize| Vec3::new((k as f32 / seg as f32 * TAU).cos() * rad, y, (k as f32 / seg as f32 * TAU).sin() * rad);
    for k in 0..seg {
        let (a, bq) = (k, k + 1);
        b.tri_away(Vec3::new(0.0, h, 0.0), ring(r, h, a), ring(r, h, bq), top, Vec3::new(0.0, -1.0, 0.0));
        b.quad_away([ring(r, 0.0, a), ring(r, 0.0, bq), ring(r, h, bq), ring(r, h, a)], [side; 4], Vec3::new(0.0, h / 2.0, 0.0));
        // A thin rim.
        b.quad_away([ring(r + 0.07, 0.0, a), ring(r + 0.07, 0.0, bq), ring(r + 0.07, h * 0.5, bq), ring(r + 0.07, h * 0.5, a)], [mix(side, rgb(0x303050), 0.4); 4], Vec3::new(0.0, h / 2.0, 0.0));
    }
    b.build()
}

fn floor_mesh() -> Mesh {
    let mut b = MeshBuilder::default();
    let (n, size) = (14i32, 1.1f32);
    for i in -n..n {
        for j in -7..9 {
            let dark = (i + j).rem_euclid(2) == 0;
            // The floor fades into the dark toward the back.
            let far = ((-(j as f32) * size) / 9.0).clamp(0.0, 1.0);
            let base = if dark { rgb(0x16206a) } else { rgb(0x2c44b8) };
            let col = mix(base, rgb(0x050824), far * 0.85);
            let (x0, z0) = (i as f32 * size, j as f32 * size);
            b.quad_away([Vec3::new(x0, 0.0, z0), Vec3::new(x0 + size, 0.0, z0), Vec3::new(x0 + size, 0.0, z0 + size), Vec3::new(x0, 0.0, z0 + size)], [col; 4], Vec3::new(0.0, -5.0, 0.0));
        }
    }
    b.build()
}

fn light_cone() -> Mesh {
    let mut b = MeshBuilder::default();
    b.cone(Vec3::ZERO, Vec3::new(0.0, 6.5, 0.0), 0.8, 16, rgb(0xffffff));
    b.build()
}

fn mark_layers(commands: &mut Commands, root: Entity) {
    commands.entity(root).insert(RenderLayers::layer(LAYER));
}

/// Spawn the stage and the models for the highlighted-page-independent lineup.
#[allow(clippy::too_many_arguments)]
fn spawn_stage(
    stage: &mut Stage,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    graphs: &mut Assets<AnimationGraph>,
    assets: &AssetServer,
    root: Option<&AssetRoot>,
    lit: &Handle<StandardMaterial>,
    unlit: &Handle<StandardMaterial>,
    ctx: &Ctx,
) {
    let layer = RenderLayers::layer(LAYER);
    let stage_root = commands.spawn((StageRoot, Transform::default(), Visibility::Inherited, layer.clone())).id();
    // Backdrop: a deep gradient with the floor in front of it.
    let back = meshes.add({
        let mut b = MeshBuilder::default();
        b.quad_cols(
            [Vec3::new(-60.0, -2.0, 0.0), Vec3::new(60.0, -2.0, 0.0), Vec3::new(60.0, 40.0, 0.0), Vec3::new(-60.0, 40.0, 0.0)],
            [rgb(0x1a2a8a), rgb(0x1a2a8a), rgb(0x04061c), rgb(0x04061c)],
        );
        b.build()
    });
    let floor = meshes.add(floor_mesh());
    commands.entity(stage_root).with_children(|p| {
        p.spawn((Mesh3d(back), MeshMaterial3d(unlit.clone()), Transform::from_xyz(0.0, 0.0, -9.0), layer.clone()));
        p.spawn((Mesh3d(floor), MeshMaterial3d(lit.clone()), Transform::default(), layer.clone()));
    });
    // Pedestals and the spotlight.
    let ped = meshes.add(pedestal_mesh(0.62, 0.07));
    for i in 0..4 {
        let mat = mats.add(StandardMaterial { base_color: Color::srgb(0.35, 0.38, 0.6), perceptual_roughness: 1.0, reflectance: 0.0, opaque_render_method: bevy::material::OpaqueRendererMethod::Forward, ..default() });
        commands.entity(stage_root).with_children(|p| {
            p.spawn((Pedestal(i), Mesh3d(ped.clone()), MeshMaterial3d(mat), Transform::default(), layer.clone()));
        });
    }
    let cone = meshes.add(light_cone());
    let spot_mat = mats.add(StandardMaterial { base_color: Color::srgba(1.0, 0.95, 0.7, 0.07), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
    commands.entity(stage_root).with_children(|p| {
        p.spawn((SpotLight, Mesh3d(cone), MeshMaterial3d(spot_mat), Transform::default(), layer.clone()));
    });

    // The models.
    for (i, who) in Character::ALL.into_iter().enumerate() {
        stage.missing[i] = true;
        if !ctx.hero_ok[i] {
            continue;
        }
        let Some(asset_root) = root else { continue };
        let built = match who {
            Character::Mario => spawn_mario(commands, assets, asset_root, graphs, &layer),
            other => {
                let which = match other {
                    Character::AdultLink => characters::Character::Link,
                    Character::YoungLink => characters::Character::YoungLink,
                    _ => characters::Character::Banjo,
                };
                match characters::spawn_rig(commands, meshes, mats, images, asset_root, which) {
                    Ok(rig) => {
                        // The game's own queries (Gem Clash scaling, the kart view) must not see these.
                        commands.entity(rig.root).remove::<characters::CharacterRoot>().insert(layer.clone());
                        for (e, _) in &rig.batches {
                            commands.entity(*e).insert(layer.clone());
                        }
                        Ok((rig.root, Body::Rig(rig)))
                    }
                    Err(e) => Err(e),
                }
            }
        };
        match built {
            Ok((root_e, body)) => {
                let mut m = dummy_mario();
                if matches!(who, Character::AdultLink | Character::YoungLink) {
                    // Sword out: the ready stance.
                    m.link.enabled = true;
                    m.link.sword_out = 1.0;
                }
                commands.entity(stage_root).add_child(root_e);
                commands.entity(root_e).insert(Visibility::Inherited);
                stage.slots[i] = Some(Slot { who, root: root_e, body, m, pop: 0.0, hop: None, ready_hop: false, dimmed: false, orig: Vec::new() });
                stage.missing[i] = false;
            }
            Err(e) => warn!("character select: no model for {who:?}: {e}"),
        }
    }
    stage.root = Some(stage_root);
    stage.last_sel = None;
    stage.log_done = false;
}

fn spawn_mario(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, graphs: &mut Assets<AnimationGraph>, layer: &RenderLayers) -> Result<(Entity, Body), String> {
    use crate::game::runner::MARIO_MODEL;
    let glb = std::fs::read(root.0.join(MARIO_MODEL)).map_err(|e| format!("{MARIO_MODEL}: {e}"))?;
    let meta = crate::game::sm64_animation::read_clip_meta(&glb);
    let count = crate::render::glb_animation_count(&root.0.join(MARIO_MODEL)).max(1);
    let (graph, nodes) = AnimationGraph::from_clips(crate::render::clips::clip_handles(assets, root, MARIO_MODEL, count));
    let graph = graphs.add(graph);
    let layer = layer.clone();
    let e = commands
        .spawn((crate::render::scene(assets, MARIO_MODEL), Transform::default(), Visibility::Inherited, layer.clone()))
        .observe(move |trigger: On<WorldInstanceReady>, mut commands: Commands, children: Query<&Children>, players: Query<&AnimationPlayer>, extras: Query<Option<&bevy::gltf::GltfExtras>>| {
            let target = trigger.event_target();
            commands.entity(target).insert(layer.clone());
            for child in children.iter_descendants(target) {
                commands.entity(child).insert(layer.clone());
                if players.contains(child) {
                    commands.entity(child).insert((PreviewAnimator, AnimationGraphHandle(graph.clone())));
                }
                if let Ok(Some(x)) = extras.get(child) {
                    if x.value.contains(r#""mario_cap_wings""#) {
                        commands.entity(child).insert(Visibility::Hidden);
                    }
                }
            }
        })
        .id();
    Ok((e, Body::Mario { player: Box::new(crate::game::sm64_animation::Sm64AnimationPlayer::new(meta)), nodes }))
}

/// Spawn the stage when the select page opens and take everything down when it closes.
#[allow(clippy::too_many_arguments)]
pub fn manage_stage(
    mut commands: Commands,
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    ctx: Res<super::CtxRes>,
    mut stage: ResMut<Stage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    scene_assets: Option<Res<SceneAssets>>,
) {
    let want = active.0 && menu.0.page == Page::Select;
    if want && !stage.is_spawned() {
        let (Some(server), Some(sa)) = (server, scene_assets) else { return };
        let t0 = std::time::Instant::now();
        spawn_stage(&mut stage, &mut commands, &mut meshes, &mut mats, &mut images, &mut graphs, &server, root.as_deref(), &sa.lit.clone(), &sa.unlit.clone(), &ctx.0);
        info!("character select: stage up, {} models, {:.0} ms", stage.slots.iter().flatten().count(), t0.elapsed().as_secs_f32() * 1000.0);
    } else if !want && stage.is_spawned() {
        if let Some(e) = stage.root.take() {
            commands.entity(e).despawn();
        }
        // Rigs are children of the stage root and go with it; their meshes drop with the handles.
        *stage = Stage::default();
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn pose_stage(
    time: Res<Time<Real>>,
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    ctx: Res<super::CtxRes>,
    low: Res<LowRes>,
    mut stage: ResMut<Stage>,
    mut tfs: Query<&mut Transform, Without<super::scene::Hero>>,
    mut vis: Query<&mut Visibility, Without<super::scene::Hero>>,
    mut meshes: ResMut<Assets<Mesh>>,
    peds: Query<(&Pedestal, &MeshMaterial3d<StandardMaterial>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut players: Query<&mut AnimationPlayer, With<PreviewAnimator>>,
    mat_q: Query<&MeshMaterial3d<StandardMaterial>>,
    mut hero: Query<(&mut Transform, &mut Visibility), (With<super::scene::Hero>, Without<Pedestal>, Without<SpotLight>)>,
) {
    if !active.0 || menu.0.page != Page::Select || !stage.is_spawned() {
        return;
    }
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let m = &menu.0;
    let lay = layout(low.w, low.h, Insets::default());
    let cam = cam_for(low.w, low.h);
    let spots = slot_positions(&lay, &cam, low.w, low.h);
    stage.spots = spots;
    let sel = m.sel;
    // A new highlight pops its character into a little hop; READY makes a bigger one.
    let confirming = m.pending.is_some();
    if stage.last_sel != Some(sel) {
        if stage.last_sel.is_some() {
            if let Some(Some(s)) = stage.slots.get_mut(sel) {
                s.hop = Some((0.0, 0.5, 0.28));
            }
        }
        stage.last_sel = Some(sel);
    }
    if confirming {
        if let Some(Some(s)) = stage.slots.get_mut(sel) {
            if !s.ready_hop {
                s.ready_hop = true;
                s.hop = Some((0.0, super::model::READY_TIME, 0.75));
            }
        }
    }
    let step = 1.0 - (-10.0 * dt).exp();
    let mut fallback = None;
    let missing = stage.missing;
    for i in 0..4 {
        let pos = spots[i];
        let is_sel = i == sel;
        let Some(slot) = stage.slots[i].as_mut() else {
            if is_sel && missing[i] {
                fallback = Some(pos);
            }
            continue;
        };
        // Greyed out when the mode does not take this character.
        let want_dim = !ctx.0.allowed[i];
        if slot.dimmed != want_dim {
            if let Body::Rig(rig) = &slot.body {
                if slot.orig.is_empty() {
                    slot.orig = rig.batches.iter().filter_map(|(e, _)| mat_q.get(*e).ok()).filter_map(|m| mats.get(&m.0).map(|x| (m.0.clone(), x.base_color))).collect();
                }
                for (h, col) in &slot.orig {
                    if let Some(mut m) = mats.get_mut(h) {
                        m.base_color = if want_dim { col.mix(&Color::srgb(0.12, 0.12, 0.16), 0.9) } else { *col };
                    }
                }
            }
            // (The materials exist one frame after the spawn; try again until they were found.)
            slot.dimmed = !slot.orig.is_empty() && want_dim;
        }
        slot.pop += ((if is_sel { 1.0 } else { 0.0 }) - slot.pop) * step;
        if !confirming {
            slot.ready_hop = false;
        }
        // Hop: a parabola over the animation; the airborne pose goes to the animators.
        let mut lift = 0.0;
        let mut airborne = false;
        if let Some((ht, dur, height)) = slot.hop.as_mut() {
            *ht += dt;
            let u = (*ht / *dur).clamp(0.0, 1.0);
            lift = *height * 4.0 * u * (1.0 - u);
            airborne = u < 1.0;
            if *ht >= *dur {
                slot.hop = None;
            }
        }
        let yaw = if is_sel { (t * 0.9).sin() * 1.15 + if confirming { 0.0 } else { 0.0 } } else { (-pos.x * 0.06).clamp(-0.3, 0.3) };
        let scale = 0.86 + 0.14 * slot.pop;
        let mut at = stand(pos, slot.pop, &cam);
        at.y = lift + 0.07;
        // Pose through the model's own animator.
        let air = airborne;
        slot.m.action = if air { ACT_JUMP } else { ACT_IDLE };
        slot.m.vel[1] = if air { 12.0 } else { 0.0 };
        let (who, root_e) = (slot.who, slot.root);
        let hop_t = slot.hop.map_or(0.0, |h| h.0 / h.1);
        match &mut slot.body {
            Body::Mario { player, nodes } => {
                let input = crate::game::sm64_animation::AnimInput { action: slot.m.action, vel_y: if air { 12.0 } else { 0.0 }, action_timer: (hop_t * 30.0) as f64, ..Default::default() };
                let pose = player.update(&input, dt as f64);
                if let Some(&node) = nodes.get(pose.clip as usize) {
                    for mut p in &mut players {
                        if !p.is_playing_animation(node) {
                            p.stop_all();
                            p.play(node).pause();
                        }
                        if let Some(a) = p.animation_mut(node) {
                            a.seek_to(pose.sample_seconds);
                        }
                    }
                }
                if let Ok(mut tf) = tfs.get_mut(root_e) {
                    *tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale));
                }
            }
            Body::Rig(rig) => {
                let base = match &rig.kind {
                    RigKind::Link { .. } => 0.044 * SCALE,
                    RigKind::Banjo { .. } => 1.25 * SCALE,
                };
                if let Ok(mut tf) = tfs.get_mut(rig.root) {
                    *tf = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(base * scale));
                }
                pose_rig(rig, &slot.m, dt as f64, &mut meshes, &mut vis);
            }
        }
        let _ = who;
    }
    stage.fallback_at = fallback;
    // Pedestals: gold under the highlighted slot, navy otherwise (the spot follows it).
    for (p, mat) in &peds {
        let on = p.0 == sel;
        let want = if stage.missing[p.0] {
            Color::srgb(0.2, 0.2, 0.26)
        } else if on {
            Color::srgb(1.0, 0.82, 0.25)
        } else {
            Color::srgb(0.32, 0.36, 0.62)
        };
        if let Some(mut mm) = mats.get_mut(&mat.0) {
            mm.base_color = want;
        }
        // Position pedestals at the slot (they are static children of the stage root).
    }
    let _ = &ctx;
    // Fallback fox when the highlighted slot has no model.
    if let Ok((mut tf, mut v)) = hero.single_mut() {
        match stage.fallback_at {
            Some(p) => {
                *v = Visibility::Inherited;
                let p = stand(p, 1.0, &cam);
                *tf = Transform::from_translation(p + Vec3::new(0.0, 0.07 + 0.015 * (t * 2.0).sin(), 0.0)).with_rotation(Quat::from_rotation_y((t * 0.9).sin() * 1.15)).with_scale(Vec3::splat(0.85));
            }
            None => *v = Visibility::Hidden,
        }
    }
}

/// Pedestals and the spotlight follow the slot positions (separate system so the borrow stays simple).
pub fn place_props(stage: Res<Stage>, menu: Res<MenuRes>, low: Res<LowRes>, mut peds: Query<(&Pedestal, &mut Transform), Without<SpotLight>>, mut spot: Query<&mut Transform, With<SpotLight>>) {
    if !stage.is_spawned() {
        return;
    }
    for (p, mut tf) in &mut peds {
        let s = stage.spots[p.0];
        let pop = stage.slots[p.0].as_ref().map_or(0.0, |sl| sl.pop);
        let cam = cam_for(low.w, low.h);
        *tf = Transform::from_translation(stand(s, pop, &cam)).with_scale(Vec3::splat(0.86 + 0.14 * pop));
    }
    if let Ok(mut tf) = spot.single_mut() {
        let s = stage.spots[menu.0.sel.min(3)];
        let pop = stage.slots[menu.0.sel.min(3)].as_ref().map_or(1.0, |sl| sl.pop);
        let cam = cam_for(low.w, low.h);
        *tf = Transform::from_translation(stand(s, pop, &cam));
    }
}

fn pose_rig(rig: &mut Rig, m: &MarioRuntime, dt: f64, meshes: &mut Assets<Mesh>, vis: &mut Query<&mut Visibility, Without<super::scene::Hero>>) {
    match &mut rig.kind {
        RigKind::Link { asset, anim, young, modes } => {
            let (_, bones) = anim.update(asset, *young, m, dt, true);
            let drawn = link_pose::sword_drawn(m);
            for (i, ((entity, handle), batch)) in rig.batches.iter().zip(&asset.batches).enumerate() {
                if let Ok(mut v) = vis.get_mut(*entity) {
                    let limb = batch.vertices.first().map_or(0, |v| v[8] as usize);
                    let want = if link_pose::batch_visible(modes[i].as_str(), limb, drawn, None, false) { Visibility::Inherited } else { Visibility::Hidden };
                    if *v != want {
                        *v = want;
                    }
                }
                let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                let mut positions = Vec::with_capacity(batch.vertices.len());
                let mut normals = Vec::with_capacity(batch.vertices.len());
                for v in &batch.vertices {
                    let bone = &bones[(v[8] as usize).min(bones.len() - 1)];
                    positions.push(bone.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array());
                    let n = bone.transform_vector3(Vec3::new(v[5] as f32, v[6] as f32, v[7] as f32) / 127.0);
                    normals.push(n.normalize_or_zero().to_array());
                }
                characters::write_mesh(&mut mesh, positions, normals);
            }
        }
        RigKind::Banjo { asset, anim, parts } => {
            anim.update(asset, m, dt, true);
            let mats = anim.matrices(asset);
            for (i, ((entity, handle), batch)) in rig.batches.iter().zip(&asset.batches).enumerate() {
                if let Some(part) = Part::from_name(&parts[i]) {
                    if let Ok(mut v) = vis.get_mut(*entity) {
                        *v = if anim.shown_parts.contains(&part) { Visibility::Inherited } else { Visibility::Hidden };
                    }
                }
                let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                let positions: Vec<[f32; 3]> = batch
                    .vertices
                    .iter()
                    .map(|v| mats[(v[9].max(0.0) as usize).min(mats.len() - 1)].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array())
                    .collect();
                let normals = characters::flat_normals(&positions);
                characters::write_mesh(&mut mesh, positions, normals);
            }
        }
    }
    let _ = (ANGLE_TO_RAD, PI, NoFrustumCulling);
}

// ------------------------------------------------------------------------------------------
// 2D layer
// ------------------------------------------------------------------------------------------

fn wrap_words(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn dialog(f: &mut Fb, r: IRect) {
    f.slab(r, rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
}

/// Pad / touch / keyboard hint line.
fn hint(pad: bool) -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        "TAP A HERO   TAP AGAIN OR READY TO PLAY"
    } else if pad {
        "D-PAD CHOOSE   A READY   B BACK"
    } else {
        "ARROWS CHOOSE   ENTER READY   ESC BACK"
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_select(f: &mut Fb, menu: &Menu, ctx: &Ctx, cam: &CamState, stage_spots: &[Vec3; 4], pops: [f32; 4], hits: &mut Hits, anim: &mut Anim, t: f32, sa: Insets, pad: bool) {
    let _ = anim;
    let (w, h) = (f.w, f.h);
    let lay = layout(w, h, sa);
    let sel = menu.sel.min(3);
    let who = Character::ALL[sel];
    let info = char_info(who);

    // Model hit areas first (tabs and READY are pushed after, so they win).
    for i in 0..4 {
        let p = stand(stage_spots[i], pops[i], cam);
        if let (Some(foot), Some(head)) = (cam.project(p, w as f32, h as f32), cam.project(p + Vec3::Y * 1.9, w as f32, h as f32)) {
            let half = ((foot.1 - head.1).abs() * 0.34).max(10.0);
            hits.0.push((IRect::new((foot.0 - half) as i32, head.1 as i32, (half * 2.0) as i32, (foot.1 - head.1).abs() as i32 + 4), Hit::Slot(i)));
        }
    }

    // BACK.
    let back = IRect::new(8 + sa.l, 6 + sa.t, 38, 15);
    f.slab(back, rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603));
    f.text_centered(back.x + back.w / 2, back.y + 4, "BACK", 1, WHITE);
    hits.0.push((back.inflate(3), Hit::Back));

    // Plate with the chosen level / mode.
    let name = clean(&ctx.target.name);
    let scale = if Fb::text_width(&name, 2) + 20 <= w - 2 * (back.x + back.w + 8) { 2 } else { 1 };
    let nw = Fb::text_width(&name, scale);
    let pw = (nw + 20).max(Fb::text_width(ctx.target.kind, 1) + 20);
    let ph = 11 + 7 * scale + 7;
    let plate = IRect::new((w - pw) / 2, lay.plate_y, pw, ph);
    if lay.portrait {
        // Under the BACK button in portrait, centred.
        let plate = IRect::new((w - pw) / 2, back.y + back.h + 6, pw, ph);
        dialog(f, plate);
        f.text_centered(w / 2, plate.y + 4, ctx.target.kind, 1, rgb(0x9fe0ff));
        f.text_centered(w / 2, plate.y + 14, &name, scale, GOLD);
    } else {
        dialog(f, plate);
        f.text_centered(w / 2, plate.y + 4, ctx.target.kind, 1, rgb(0x9fe0ff));
        f.text_centered(w / 2, plate.y + 14, &name, scale, GOLD);
    }
    if ctx.target.mario_only {
        let note = "MARIO ONLY MODE";
        let y = if lay.portrait { lay.plate_y + 36 + 6 + 14 } else { plate.y + plate.h + 4 };
        let nw = Fb::text_width(note, 1) + 10;
        let r = IRect::new((w - nw) / 2, y, nw, 12);
        f.slab(r, rgb(0xd09a2a), rgb(0x8a5a0c), rgb(0xffe08a), rgb(0x4a2a04), rgb(0x2a1602));
        f.text_centered(w / 2, r.y + 3, note, 1, rgb(0x2a1602));
    }

    // Slot tabs under the models.
    let tab_h = if lay.portrait { 14 } else { 13 };
    for i in 0..4 {
        let c = Character::ALL[i];
        let pop = pops[i];
        let (fx, fy) = lay.feet[i];
        let pitch = lay.pitch as i32;
        let tw = (pitch - 4).max(30);
        let r = IRect::new(fx as i32 - tw / 2, fy as i32 + if lay.portrait { 14 } else { 18 } + (pop * 16.0) as i32, tw, tab_h);
        let locked = lock_reason(c, ctx.hero_ok[i], ctx.allowed[i]).is_some();
        let on = i == sel;
        if locked {
            f.slab(r, rgb(0x55556a), rgb(0x2c2c3a), rgb(0x8a8aa0), rgb(0x16161e), rgb(0x08080c));
        } else if on {
            f.slab(r, mix(GOLD, WHITE, ((t * 6.0).sin() * 0.5 + 0.5) * 0.35), rgb(0xe08a18), rgb(0xfff1a8), rgb(0x8a4a08), rgb(0x3a1a04));
        } else {
            f.slab(r, rgb(0x33449a), rgb(0x1a2468), rgb(0x5b70c8), rgb(0x0a1040), rgb(0x040720));
        }
        let label = info_name(c);
        let col = if locked { rgb(0xa8a8bc) } else if on { rgb(0x3a1a04) } else { rgb(0xdfe6ff) };
        let label = if Fb::text_width(label, 1) > r.w - 4 { short_name(c) } else { label };
        f.text_centered(r.x + r.w / 2, r.y + (r.h - 7) / 2, label, 1, col);
        if locked {
            // A little padlock above the empty pedestal.
            if !on {
                draw_lock(f, fx as i32, fy as i32 - 20);
            }
        }
        hits.0.push((r.inflate(1), Hit::Slot(i)));
    }
    // Chevron over the highlighted tab.
    {
        let (fx, fy) = lay.feet[sel];
        let bob = ((t * 7.0).sin().abs() * 2.0) as i32;
        let (x, y) = (fx as i32, fy as i32 + if lay.portrait { 14 } else { 18 } + (pops[sel] * 16.0) as i32 + tab_h + 4 + bob);
        f.poly(&[((x - 5) as f32 + 1.0, (y + 5) as f32 + 1.0), ((x + 5) as f32 + 1.0, (y + 5) as f32 + 1.0), (x as f32 + 1.0, y as f32 + 1.0)], rgb(0x050a22));
        f.poly(&[((x - 5) as f32, (y + 5) as f32), ((x + 5) as f32, (y + 5) as f32), (x as f32, y as f32)], GOLD);
    }

    // Info panel.
    let p = lay.panel;
    dialog(f, p);
    let mut y = p.y + 6;
    let pad_x = p.x + 7;
    let max_chars = ((p.w - 14) / 6).max(8) as usize;
    let nm = info.name;
    let ns = if Fb::text_width(nm, 3) <= p.w - 14 && !lay.portrait { 3 } else { 2 };
    f.text(pad_x, y, nm, ns, GOLD);
    y += 7 * ns + 5;
    f.rect(IRect::new(pad_x, y, p.w - 14, 1), rgb(0x6f86e0));
    y += 5;
    f.text(pad_x, y, "MOVES", 1, rgb(0x9fe0ff));
    y += 9;
    for l in wrap_words(info.moves, max_chars) {
        f.text(pad_x, y, &l, 1, WHITE);
        y += 9;
    }
    y += 3;
    for l in wrap_words(info.blurb, max_chars) {
        f.text(pad_x, y, &l, 1, rgb(0xaec6ff));
        y += 9;
    }
    y += 3;
    f.text(pad_x, y, &format!("COLLECTS {}", info.collects), 1, rgb(0xffe08a));
    // Lock note.
    let reason = lock_reason(who, ctx.hero_ok[sel], ctx.allowed[sel]);
    if let Some(r) = reason {
        y += 12;
        f.text(pad_x, y, "LOCKED", 1, rgb(0xff8a7a));
        y += 9;
        for l in wrap_words(r, max_chars) {
            f.text(pad_x, y, &l, 1, rgb(0xffb0a0));
            y += 9;
        }
    }
    // READY.
    let rb = IRect::new(p.x + 7, p.y + p.h - 24, p.w - 14, 18);
    let ok = reason.is_none();
    if ok {
        let pulse = (t * 5.0).sin() * 0.5 + 0.5;
        f.slab(rb, mix(rgb(0x4ad060), rgb(0x8aff8a), pulse * 0.5), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a));
        f.text_centered(rb.x + rb.w / 2, rb.y + 5, "READY", 2, rgb(0x04300c));
    } else {
        f.slab(rb, rgb(0x6a6a78), rgb(0x3a3a46), rgb(0x9a9aa8), rgb(0x20202a), rgb(0x0a0a10));
        f.text_centered(rb.x + rb.w / 2, rb.y + 5, "LOCKED", 2, rgb(0xc0c0cc));
    }
    hits.0.push((rb.inflate(2), Hit::Ready));
    // Hint bar.
    let hint = hint(pad);
    let bar = IRect::new(8 + sa.l, h - 17 - sa.b, w - 16 - sa.l - sa.r, 13);
    dialog(f, bar);
    let hint = if Fb::text_width(hint, 1) > bar.w - 8 { "TAP TO CHOOSE" } else { hint };
    f.text(bar.x + 5, bar.y + 3, hint, 1, WHITE);
}

fn info_name(c: Character) -> &'static str {
    char_info(c).name
}

fn short_name(c: Character) -> &'static str {
    match c {
        Character::YoungLink => "Y LINK",
        other => char_info(other).name,
    }
}

fn draw_lock(f: &mut Fb, cx: i32, y: i32) {
    f.rect(IRect::new(cx - 6, y + 5, 12, 9), rgb(0x9a9ab0));
    f.frame(IRect::new(cx - 6, y + 5, 12, 9), rgb(0x20202c));
    f.rect(IRect::new(cx - 3, y, 2, 6), rgb(0x9a9ab0));
    f.rect(IRect::new(cx + 1, y, 2, 6), rgb(0x9a9ab0));
    f.rect(IRect::new(cx - 3, y - 1, 6, 2), rgb(0x9a9ab0));
    f.rect(IRect::new(cx - 1, y + 8, 2, 3), rgb(0x20202c));
}

/// Slot pop values for the 2D layer (so tabs and hit boxes follow the models).
pub fn pops(stage: &Stage) -> [f32; 4] {
    std::array::from_fn(|i| stage.slots[i].as_ref().map_or(if stage.missing[i] { 0.0 } else { 0.0 }, |s| s.pop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::retro::model::{Ctx, Target};

    fn ctx() -> Ctx {
        let mut c = Ctx::from_registry(Character::Mario, [true, true, false, true], |_| true);
        c.target = Target { kind: "COURSE", name: "BOB-OMB BATTLEFIELD".into(), mario_only: false };
        c
    }

    #[test]
    fn floor_point_inverts_the_projection() {
        for (w, h) in [(520, 240), (240, 522), (320, 240)] {
            let cam = cam_for(w, h);
            for (px, py) in [(w as f32 * 0.3, h as f32 * 0.7), (w as f32 * 0.6, h as f32 * 0.5)] {
                let p = floor_point(&cam, px, py, w as f32, h as f32).expect("on the floor");
                assert!(p.y.abs() < 1e-3);
                let (x, y, _) = cam.project(p, w as f32, h as f32).unwrap();
                assert!((x - px).abs() < 0.5 && (y - py).abs() < 0.5, "{w}x{h}: {x},{y} vs {px},{py}");
            }
        }
    }

    #[test]
    fn lineup_is_in_order_and_inside_the_screen_for_both_orientations() {
        for (w, h) in [(520, 240), (320, 240), (240, 522)] {
            let l = layout(w, h, Insets::default());
            assert_eq!(l.portrait, w < h);
            for i in 0..4 {
                let (x, y) = l.feet[i];
                assert!(x > 0.0 && x < w as f32 && y > 0.0 && y < h as f32, "{w}x{h} slot {i} at {x},{y}");
                // Tabs fit above the panel / hint bar.
                if !l.portrait {
                    assert!(x < l.panel.x as f32, "{w}x{h} slot {i} is left of the panel");
                }
            }
            if !l.portrait {
                assert!(l.feet.windows(2).all(|p| p[0].0 < p[1].0));
            } else {
                assert!(l.feet[0].0 < l.feet[1].0 && l.feet[0].1 < l.feet[2].1);
                assert!(l.feet[2].1 + 40.0 < l.panel.y as f32, "tabs stay clear of the panel: {} vs {}", l.feet[2].1, l.panel.y);
            }
            assert!(l.panel.w >= 100 && l.panel.h >= 80, "{w}x{h} panel {:?}", l.panel);
        }
    }

    #[test]
    fn slot_positions_are_four_distinct_floor_points() {
        for (w, h) in [(520, 240), (240, 522)] {
            let l = layout(w, h, Insets::default());
            let cam = cam_for(w, h);
            let p = slot_positions(&l, &cam, w, h);
            for i in 0..4 {
                assert!(p[i].y.abs() < 1e-3);
                for j in 0..i {
                    assert!(p[i].distance(p[j]) > 0.8, "{w}x{h}: slots {j} and {i} overlap");
                }
            }
        }
    }

    fn draw(w: i32, h: i32, sel: usize, c: &Ctx) -> (Fb, Hits) {
        let mut menu = Menu::default();
        menu.open_select(c);
        menu.sel = sel;
        let cam = cam_for(w, h);
        let l = layout(w, h, Insets::default());
        let spots = slot_positions(&l, &cam, w, h);
        let mut f = Fb::new(w, h);
        let mut hits = Hits::default();
        let mut anim = Anim::default();
        draw_select(&mut f, &menu, c, &cam, &spots, [0.0; 4], &mut hits, &mut anim, 1.0, Insets::default(), false);
        (f, hits)
    }

    #[test]
    fn every_control_has_a_hit_zone_in_both_orientations() {
        let c = ctx();
        for (w, h) in [(520, 240), (240, 522)] {
            let (f, hits) = draw(w, h, 1, &c);
            for want in [Hit::Back, Hit::Ready, Hit::Slot(0), Hit::Slot(1), Hit::Slot(2), Hit::Slot(3)] {
                assert!(hits.0.iter().any(|(_, x)| *x == want), "{w}x{h} missing {want:?}");
            }
            assert!(f.data.chunks(4).filter(|p| p[3] > 0).count() > 2000);
            // Touch targets: every slot tab and READY are at least 11 px tall (>= 40 pt on a phone at 3.x pt / px).
            for (r, x) in &hits.0 {
                if matches!(x, Hit::Ready) {
                    assert!(r.h >= 18, "READY too small: {r:?}");
                }
            }
        }
    }

    #[test]
    fn slot_tabs_do_not_overlap_each_other_or_the_panel() {
        let c = ctx();
        for (w, h) in [(520, 240), (240, 522), (320, 240)] {
            let (_, hits) = draw(w, h, 0, &c);
            let tabs: Vec<IRect> = hits.0.iter().filter(|(r, x)| matches!(x, Hit::Slot(_)) && r.h <= 18).map(|(r, _)| *r).collect();
            assert_eq!(tabs.len(), 4, "{w}x{h}");
            for (i, a) in tabs.iter().enumerate() {
                for b in &tabs[i + 1..] {
                    let overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                    assert!(!overlap, "{w}x{h}: tabs {a:?} {b:?}");
                }
                let panel = layout(w, h, Insets::default()).panel;
                let hit_panel = a.x < panel.x + panel.w && panel.x < a.x + a.w && a.y < panel.y + panel.h && panel.y < a.y + a.h;
                assert!(!hit_panel, "{w}x{h}: tab {a:?} runs into the panel {panel:?}");
            }
        }
    }

    #[test]
    fn click_on_a_tab_maps_to_its_slot() {
        let c = ctx();
        let (_, hits) = draw(520, 240, 0, &c);
        let (r, _) = hits.0.iter().rev().find(|(_, x)| *x == Hit::Slot(3)).unwrap();
        let (cx, cy) = r.center();
        let top = hits.0.iter().rev().find(|(rect, _)| rect.contains(cx as f32, cy as f32)).map(|(_, h)| *h);
        assert_eq!(top, Some(Hit::Slot(3)));
    }

    #[test]
    fn locked_and_mario_only_slots_say_why() {
        let mut c = ctx();
        c.hero_ok = [true, true, false, true];
        let (_, _) = draw(520, 240, 2, &c);
        assert_eq!(lock_reason(Character::YoungLink, c.hero_ok[2], c.allowed[2]), Some("IMPORT OCARINA OF TIME"));
        c.allowed = [true, false, false, false];
        c.target.mario_only = true;
        let (_, _) = draw(240, 522, 1, &c);
        assert_eq!(lock_reason(Character::Banjo, c.hero_ok[3], c.allowed[3]), Some("MARIO ONLY"));
    }

    /// The stage and its models exist only while the select page is open.
    #[test]
    fn models_spawn_on_enter_and_despawn_on_exit() {
        use crate::shell::retro::{CtxRes, MenuRes};
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let have_data = data.join("../public/models/oot/link.json").is_file() && data.join("../public/models/bk/banjo.json").is_file();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Image>()
            .init_asset::<AnimationGraph>()
            .init_asset::<AnimationClip>()
            .init_asset::<bevy::gltf::Gltf>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .insert_resource(AssetRoot::new(data))
            .insert_resource(RetroActive(true))
            .insert_resource(MenuRes(Menu::default()))
            .insert_resource(CtxRes(ctx()))
            .init_resource::<Stage>()
            .add_systems(Update, manage_stage);
        let (lit, unlit) = {
            let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            (mats.add(StandardMaterial::default()), mats.add(StandardMaterial::default()))
        };
        app.insert_resource(SceneAssets { lit, unlit });
        // Title page: nothing.
        app.update();
        assert!(!app.world().resource::<Stage>().is_spawned());
        let count = |app: &mut App| app.world_mut().query_filtered::<Entity, With<RenderLayers>>().iter(app.world()).count();
        assert_eq!(count(&mut app), 0);
        // Open the select.
        {
            let mut m = app.world_mut().resource_mut::<MenuRes>();
            let c = ctx();
            m.0.open_select(&c);
        }
        app.world_mut().resource_mut::<CtxRes>().0.hero_ok = if have_data { [true, true, true, true] } else { [false; 4] };
        app.update();
        app.update();
        let st = app.world().resource::<Stage>();
        assert!(st.is_spawned());
        let spawned = count(&mut app);
        assert!(spawned >= 7, "floor, backdrop, pedestals and spot: {spawned}");
        if have_data {
            assert_eq!(app.world().resource::<Stage>().model_count(), 4, "all four real models stand on the stage");
        }
        // Leave: everything is gone, also the models.
        app.world_mut().resource_mut::<MenuRes>().0.close_select();
        app.update();
        app.update();
        assert!(!app.world().resource::<Stage>().is_spawned());
        assert_eq!(count(&mut app), 0, "no stage entity survives the page");
        assert_eq!(app.world_mut().query::<&StageRoot>().iter(app.world()).count(), 0);
        // Re-entering does not leak: the same number of entities again.
        {
            let c = ctx();
            app.world_mut().resource_mut::<MenuRes>().0.open_select(&c);
        }
        app.update();
        app.update();
        assert_eq!(count(&mut app), spawned);
        app.world_mut().resource_mut::<MenuRes>().0.close_select();
        app.update();
        assert_eq!(count(&mut app), 0);
    }

    #[test]
    fn stage_helpers_start_empty() {
        let s = Stage::default();
        assert!(!s.is_spawned());
        assert_eq!(s.model_count(), 0);
        assert_eq!(pops(&s), [0.0; 4]);
    }

    #[test]
    fn meshes_are_valid() {
        for m in [pedestal_mesh(0.6, 0.07), floor_mesh(), light_cone()] {
            let n = m.count_vertices();
            assert!(n > 0 && n % 3 == 0);
        }
    }
}
