//! Bevy views of the Zelda scene: NPCs / chests / rupees / hearts / fairies / dungeon parts /
//! sword-quest pieces / Gohma as scene entities driven from [`ZeldaWorld`], procedural bits
//! (nut, flame, seed, portal, glint) and the slingshot model, plus the camera overrides
//! (`frameConversation`, fixed rooms, mouth opening, first-person slingshot).
use super::glb;
use super::save::ZeldaSave;
use super::world::ZeldaWorld;
use crate::camera::{CameraRig, MainCamera};
use crate::game::runner::GameSim;
use crate::progress::CourseScoped;
use crate::render::{scene, to_bevy, AssetRoot};
use bevy::prelude::*;
use bevy::render::view::NoFrustumCulling;
use bevy::scene::SceneInstanceReady;
use jumpman_sim::math::Vec3 as V3;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum ActorKind {
    Static,
    /// Dungeon part by role.
    Part(String),
    Quest(&'static str),
    Rupee(String),
    Heart(usize),
    Fairy { home: V3, owner: Option<String> },
    Chest(String),
    Npc(Option<String>),
    Boss,
    Container,
}

#[derive(Component)]
pub struct ZActor {
    pub kind: ActorKind,
    pub model: String,
    pub base_scale: f32,
}

/// Clip bookkeeping of an actor (filled when its scene is instantiated).
#[derive(Component)]
pub struct Clips {
    pub names: Vec<String>,
    pub nodes: Vec<AnimationNodeIndex>,
    pub handles: Vec<Handle<AnimationClip>>,
    pub graph: Handle<AnimationGraph>,
}

/// The AnimationPlayer entity inside an actor's scene.
#[derive(Component)]
pub struct ActorPlayer {
    actor: Entity,
    current: Option<usize>,
    chest_applied: Option<bool>,
}

#[derive(Component)]
pub struct GohmaEye;

#[derive(Component)]
pub enum ZMesh {
    Nut,
    Flame,
    Torch,
    Seed,
    Portal,
    Halo,
    Glint,
}

#[derive(Component)]
pub struct SlingshotModel;

fn yaw(deg: f64) -> Quat {
    Quat::from_rotation_y(deg.to_radians() as f32)
}

pub fn spawn_scene(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    world: Option<Res<ZeldaWorld>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut photo_materials: ResMut<Assets<super::photo::PhotoMaterial>>,
) {
    let Some(world) = world else { return };
    let mut hearts = 0;
    let boss_pos = world.boss_parts;
    for o in &world.scene.objects {
        if !root.exists(&o.model) {
            warn!("zelda model missing: {}", o.model);
            continue;
        }
        let kind = if let Some(role) = &o.dungeon_role {
            match (role.as_str(), boss_pos.is_some()) {
                ("gohma", true) => ActorKind::Boss,
                ("container", true) => ActorKind::Container,
                _ => ActorKind::Part(role.clone()),
            }
        } else if let Some(key) = &o.treasure_key {
            ActorKind::Chest(key.clone())
        } else if let Some(key) = &o.rupee {
            ActorKind::Rupee(key.clone())
        } else if o.fairy {
            ActorKind::Fairy { home: o.pos, owner: None }
        } else if o.recovery_heart {
            hearts += 1;
            ActorKind::Heart(hearts - 1)
        } else if let Some(q) = super::scene::quest_part(&o.model) {
            ActorKind::Quest(q)
        } else if o.speaker.is_some() || o.idle {
            ActorKind::Npc(o.speaker.clone())
        } else {
            ActorKind::Static
        };
        let kind = match kind {
            ActorKind::Fairy { home, .. } => ActorKind::Fairy { home, owner: o.owner.clone() },
            k => k,
        };
        let abs = root.0.join(&o.model);
        let base_scale = glb::bevy_scale(&abs) * o.actor_scale.unwrap_or(1.0) as f32;
        let names = glb::animation_names(&abs);
        let mut e = commands.spawn((
            scene(&assets, &o.model),
            Transform::from_translation(to_bevy(o.pos)).with_rotation(yaw(o.yaw)).with_scale(Vec3::splat(base_scale)),
            ZActor { kind, model: o.model.clone(), base_scale },
            CourseScoped,
        ));
        if !names.is_empty() {
            let handles: Vec<Handle<AnimationClip>> = (0..names.len()).map(|i| assets.load(GltfAssetLabel::Animation(i).from_asset(o.model.clone()))).collect();
            let (graph, nodes) = AnimationGraph::from_clips(handles.clone());
            e.insert(Clips { names, nodes, handles, graph: graphs.add(graph) });
            e.observe(attach_players);
        }
    }
    // Fairies of Mido follow him.
    // (owner is resolved from the object list: the fairy object listing `owner: "mido"`.)
    let mat = |c: Color, unlit: bool, materials: &mut Assets<StandardMaterial>| materials.add(StandardMaterial { base_color: c, unlit, ..default() });
    let sphere = |r: f32, meshes: &mut Assets<Mesh>| meshes.add(Sphere::new(r * 0.01).mesh().ico(2).unwrap());
    commands.spawn((CourseScoped, ZMesh::Nut, Mesh3d(sphere(18.0, &mut meshes)), MeshMaterial3d(mat(Color::srgb_u8(0x9c, 0x53, 0x26), false, &mut materials)), Visibility::Hidden, Transform::default()));
    commands.spawn((CourseScoped, ZMesh::Flame, Mesh3d(sphere(13.0, &mut meshes)), MeshMaterial3d(mat(Color::srgb_u8(0xff, 0xa1, 0x28), true, &mut materials)), Visibility::Hidden, Transform::default()));
    commands.spawn((CourseScoped, ZMesh::Torch, Mesh3d(sphere(13.0, &mut meshes)), MeshMaterial3d(mat(Color::srgb_u8(0xff, 0xa1, 0x28), true, &mut materials)), Visibility::Hidden, Transform::default()));
    commands.spawn((CourseScoped, ZMesh::Seed, Mesh3d(sphere(9.0, &mut meshes)), MeshMaterial3d(mat(Color::srgb_u8(0xca, 0x99, 0x49), false, &mut materials)), Visibility::Hidden, Transform::default()));
    if boss_pos.is_some() {
        let portal = meshes.add(Cylinder::new(1.5, 1.3).mesh().resolution(48).build());
        let blue = materials.add(StandardMaterial {
            base_color: Color::srgba(0.35, 0.74, 1.0, 0.55),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((CourseScoped, ZMesh::Portal, Mesh3d(portal), MeshMaterial3d(blue), Visibility::Hidden, Transform::default()));
        let halo = meshes.add(Annulus::new(0.45, 1.8));
        let halo_mat = materials.add(StandardMaterial { base_color: Color::srgba(0.58, 0.92, 1.0, 0.75), unlit: true, alpha_mode: AlphaMode::Blend, double_sided: true, cull_mode: None, ..default() });
        commands.spawn((CourseScoped, ZMesh::Halo, Mesh3d(halo), MeshMaterial3d(halo_mat), Visibility::Hidden, Transform::default()));
        commands.spawn((CourseScoped, ZMesh::Glint, Mesh3d(sphere(26.0, &mut meshes)), MeshMaterial3d(mat(Color::srgb_u8(0xff, 0x44, 0x33), true, &mut materials)), Visibility::Hidden, Transform::default(), NoFrustumCulling));
    }
    // Fixed-camera rooms: the room meshes keep writing depth and show the photograph through
    // `photo::PhotoMaterial` (`prepareKokiriBackground`).
    commands.remove_resource::<super::photo::PhotoRoom>();
    if let Some(room) = &world.scene.kokiri.fixed {
        let background = format!("levels/{}/background.jpg", world.scene.course);
        if root.exists(&background) {
            // The photograph also fills the view behind the room (outside the mesh).
            let (yaw, pitch) = (room.yaw * std::f64::consts::PI / 32768.0, room.pitch * std::f64::consts::PI / 32768.0);
            let dir = Vec3::new((pitch.cos() * yaw.sin()) as f32, -pitch.sin() as f32, (pitch.cos() * yaw.cos()) as f32);
            let depth = 200.0_f32;
            let plate_h = 2.0 * depth * ((room.fov as f32).to_radians() * 0.5).tan();
            let plate = materials.add(StandardMaterial { base_color_texture: Some(assets.load(background.clone())), unlit: true, fog_enabled: false, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() });
            commands.spawn((
                CourseScoped,
                Mesh3d(meshes.add(Rectangle::new(plate_h * 4.0 / 3.0, plate_h))),
                MeshMaterial3d(plate),
                Transform::from_translation(to_bevy(room.eye) + dir * depth).looking_to(dir, Vec3::Y),
                NoFrustumCulling,
            ));
            let handle = photo_materials.add(super::photo::PhotoMaterial { photo: assets.load(background), aspect: Vec4::new(1.0, 0.0, 0.0, 0.0) });
            commands.insert_resource(super::photo::PhotoRoom(handle));
        }
    }
    let sling = "levels/deku_tree/slingshot.glb";
    if root.exists(sling) {
        commands.spawn((CourseScoped, SlingshotModel, scene(&assets, sling), Transform::from_scale(Vec3::splat(glb::bevy_scale(&root.0.join(sling)))), Visibility::Hidden));
    }
}

/// When an actor's scene is ready, give its AnimationPlayer the clip graph.
fn attach_players(trigger: Trigger<SceneInstanceReady>, mut commands: Commands, clips: Query<&Clips>, children: Query<&Children>, players: Query<(), With<AnimationPlayer>>, names: Query<&Name>) {
    let actor = trigger.target();
    let Ok(clips) = clips.get(actor) else { return };
    for child in children.iter_descendants(actor) {
        if players.contains(child) {
            commands.entity(child).insert((AnimationGraphHandle(clips.graph.clone()), AnimationTransitions::new(), ActorPlayer { actor, current: None, chest_applied: None }));
        }
        if names.get(child).is_ok_and(|n| n.as_str() == "joint_4") {
            commands.entity(child).insert(GohmaEye);
        }
    }
}

fn clip_index(names: &[String], want: &str) -> usize {
    names.iter().position(|n| n == want).unwrap_or(0)
}

/// Pick each actor's clip: speakers gesture, Mido walks, Gohma follows its phase, chests open.
pub fn drive_animation(
    world: Option<ResMut<ZeldaWorld>>,
    save: Res<ZeldaSave>,
    clips_assets: Res<Assets<AnimationClip>>,
    actors: Query<(&ZActor, &Clips)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &mut ActorPlayer)>,
    mut gohma_clip: Local<Option<String>>,
) {
    let Some(mut world) = world else { return };
    let gesture = world.visit.gesture();
    let speaker = world.visit.speaker_id().map(str::to_string);
    let walking = world.quest.as_ref().is_some_and(|q| q.walking);
    if let Some(g) = world.gohma.as_mut() {
        if let Some(c) = g.clip.take() {
            *gohma_clip = Some(c.to_string());
        }
    }
    let lids: Vec<String> = std::mem::take(&mut world.lid_requests);
    for (mut player, mut transitions, mut ap) in &mut players {
        let Ok((actor, clips)) = actors.get(ap.actor) else { continue };
        let desired = match &actor.kind {
            ActorKind::Chest(key) => {
                let handle = clips.handles.get(clip_index(&clips.names, "Open"));
                let node = clips.nodes[clip_index(&clips.names, "Open")];
                let opened = save.chest_opened(key);
                let req = lids.iter().any(|k| k == key || (k == "sword_chest" && false));
                if req || ap.chest_applied != Some(opened) {
                    if req {
                        transitions.play(&mut player, node, Duration::ZERO);
                        if let Some(a) = player.animation_mut(node) {
                            a.set_speed(1.5);
                        }
                        ap.chest_applied = Some(true);
                    } else if let Some(duration) = handle.and_then(|h| clips_assets.get(h)).map(|c| c.duration()) {
                        transitions.play(&mut player, node, Duration::ZERO);
                        if let Some(a) = player.animation_mut(node) {
                            a.set_speed(0.0);
                            a.seek_to(if opened { duration } else { 0.0 });
                        }
                        ap.chest_applied = Some(opened);
                    }
                }
                continue;
            }
            ActorKind::Quest("sword_chest") => {
                let i = clip_index(&clips.names, "Open");
                let node = clips.nodes[i];
                let opened = save.sword;
                let req = lids.iter().any(|k| k == "sword_chest");
                if req {
                    transitions.play(&mut player, node, Duration::ZERO);
                    if let Some(a) = player.animation_mut(node) {
                        a.set_speed(1.5);
                    }
                    ap.chest_applied = Some(true);
                } else if ap.chest_applied != Some(opened) {
                    if let Some(duration) = clips_assets.get(&clips.handles[i]).map(|c| c.duration()) {
                        transitions.play(&mut player, node, Duration::ZERO);
                        if let Some(a) = player.animation_mut(node) {
                            a.set_speed(0.0);
                            a.seek_to(if opened { duration } else { 0.0 });
                        }
                        ap.chest_applied = Some(opened);
                    }
                }
                continue;
            }
            ActorKind::Quest("mido") => {
                let talk = speaker.as_deref() == Some("mido");
                clip_index(&clips.names, if walking { "Walk" } else if talk { gesture } else { "Idle" })
            }
            ActorKind::Npc(id) => {
                let talk = id.is_some() && id.as_deref() == speaker.as_deref();
                clip_index(&clips.names, if talk { gesture } else { "Idle" })
            }
            ActorKind::Boss => clip_index(&clips.names, gohma_clip.as_deref().unwrap_or("Idle")),
            ActorKind::Quest(_) | ActorKind::Static => continue,
            _ => 0,
        };
        let respond = matches!(actor.kind, ActorKind::Boss) && gohma_clip.is_some();
        if ap.current != Some(desired) || respond {
            ap.current = Some(desired);
            let node = clips.nodes[desired];
            transitions.play(&mut player, node, Duration::from_millis(120)).repeat();
        }
    }
    *gohma_clip = None;
}

/// Pose every managed actor from the Zelda state.
#[allow(clippy::type_complexity)]
pub fn sync_actors(
    time: Res<Time>,
    world: Option<Res<ZeldaWorld>>,
    sim: Res<GameSim>,
    save: Res<ZeldaSave>,
    mut actors: Query<(&ZActor, &mut Transform, &mut Visibility)>,
) {
    let Some(world) = world else { return };
    let t = time.elapsed_secs_f64();
    let mido = world.quest.as_ref().map(|q| q.mido_pos);
    let _ = &sim;
    for (actor, mut tf, mut vis) in &mut actors {
        let mut visible = true;
        match &actor.kind {
            ActorKind::Part(id) => {
                if let Some(p) = world.dungeon.as_ref().and_then(|d| d.parts.get(id)) {
                    tf.translation = to_bevy(p.pos);
                    visible = p.visible;
                    tf.rotation = Quat::from_euler(EulerRot::YXZ, p.yaw.to_radians() as f32, p.rot_x as f32, 0.0);
                    tf.scale = Vec3::splat(actor.base_scale);
                }
            }
            ActorKind::Quest(name) => {
                if let Some(q) = &world.quest {
                    match *name {
                        "mido" => {
                            tf.translation = to_bevy(q.mido_pos);
                            tf.rotation = Quat::from_rotation_y(q.mido_rot_y as f32);
                        }
                        "kokiri_sword" => {
                            visible = q.sword_visible;
                            tf.translation = to_bevy(q.sword_pos);
                            tf.rotation = Quat::from_rotation_y(q.sword_rot_y as f32);
                        }
                        "deku_tree_mouth" => tf.translation = to_bevy(q.mouth_position()),
                        _ => {}
                    }
                }
            }
            ActorKind::Rupee(key) => {
                visible = !world.taken.contains(key);
                tf.rotation = Quat::from_rotation_y((t * 3.0) as f32);
            }
            ActorKind::Heart(i) => {
                visible = world.hearts.get(*i).is_some_and(|h| h.visible);
                tf.rotation = Quat::from_rotation_y((t * 2.0) as f32);
            }
            ActorKind::Fairy { home, owner } => {
                let h = if owner.is_some() { mido.unwrap_or(*home) } else { *home };
                tf.translation = to_bevy([h[0] + (t * 1.9).sin() * 67.0, h[1] + 204.0 + (t * 3.4).sin() * 19.0, h[2] + (t * 1.9).cos() * 58.0]);
                tf.rotation = Quat::from_rotation_y((t * 1.9) as f32);
            }
            ActorKind::Chest(_) | ActorKind::Npc(_) | ActorKind::Static => {}
            ActorKind::Boss => {
                if let Some(g) = &world.gohma {
                    visible = g.boss_visible;
                    tf.translation = to_bevy(g.boss_pos);
                    tf.rotation = Quat::from_euler(EulerRot::YZX, g.boss_rot_y as f32, g.boss_rot_z as f32, 0.0);
                }
            }
            ActorKind::Container => {
                if let Some(g) = &world.gohma {
                    visible = g.heart_visible;
                    tf.translation = to_bevy(g.heart_pos);
                    tf.rotation = Quat::from_rotation_y(g.heart_rot_y as f32);
                }
            }
        }
        let _ = &save;
        let v = if visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != v {
            *vis = v;
        }
    }
}

/// Fairies owned by Mido follow him: resolved by name once (the scene lists `owner`).
pub fn sync_effects(
    world: Option<ResMut<ZeldaWorld>>,
    eye: Query<&GlobalTransform, With<GohmaEye>>,
    mut meshes: Query<(&ZMesh, &mut Transform, &mut Visibility), Without<SlingshotModel>>,
    mut sling: Query<(&mut Transform, &mut Visibility), With<SlingshotModel>>,
) {
    let Some(mut world) = world else { return };
    if let (Some(g), Ok(e)) = (world.gohma.as_mut(), eye.single()) {
        let p = e.translation() * 100.0;
        g.eye = [p.x as f64, p.y as f64, p.z as f64];
    }
    let w = &*world;
    for (kind, mut tf, mut vis) in &mut meshes {
        let (visible, pos) = match kind {
            ZMesh::Nut => (w.dungeon.as_ref().is_some_and(|d| d.nut_visible), w.dungeon.as_ref().map(|d| d.nut_pos)),
            ZMesh::Flame => (w.dungeon.as_ref().is_some_and(|d| d.flame_visible), w.dungeon.as_ref().map(|d| d.flame_pos)),
            ZMesh::Torch => (w.dungeon.is_some(), w.dungeon.as_ref().map(|d| d.torch_pos)),
            ZMesh::Seed => (w.slingshot.seed_visible, Some(w.slingshot.seed_pos)),
            ZMesh::Portal => (w.gohma.as_ref().is_some_and(|g| g.portal_visible), w.gohma.as_ref().map(|g| g.portal_pos)),
            ZMesh::Halo => (w.gohma.as_ref().is_some_and(|g| g.portal_visible), w.gohma.as_ref().map(|g| [g.portal_pos[0], g.portal_pos[1] - 59.0, g.portal_pos[2]])),
            ZMesh::Glint => (w.gohma.as_ref().is_some_and(|g| g.glint_visible), w.gohma.as_ref().map(|g| g.eye)),
        };
        if let Some(p) = pos {
            tf.translation = to_bevy(p);
        }
        match kind {
            ZMesh::Portal => tf.scale = Vec3::new(w.gohma.as_ref().map_or(1.0, |g| g.portal_scale as f32), 1.0, w.gohma.as_ref().map_or(1.0, |g| g.portal_scale as f32)) * 100.0 * 0.01,
            ZMesh::Halo => tf.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            _ => {}
        }
        let v = if visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != v {
            *vis = v;
        }
    }
    for (mut tf, mut vis) in &mut sling {
        let s = &w.slingshot;
        *vis = if s.aiming { Visibility::Inherited } else { Visibility::Hidden };
        if s.aiming {
            tf.translation = to_bevy(s.model_position());
            tf.rotation = Quat::from_euler(EulerRot::YXZ, s.yaw as f32, -s.pitch as f32, 0.0);
        }
    }
}

/// Camera takeovers, run after every Update system and before transform propagation.
#[allow(clippy::too_many_arguments)]
pub fn zelda_camera(
    world: Option<Res<ZeldaWorld>>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    mut rig: ResMut<CameraRig>,
    mut camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    mut base_fov: Local<Option<f32>>,
) {
    let (tf, projection) = &mut *camera;
    let Projection::Perspective(p) = &mut **projection else { return };
    let base = *base_fov.get_or_insert(p.fov);
    let Some(world) = world else { return };
    let look = |tf: &mut Transform, eye: V3, at: V3| *tf = Transform::from_translation(to_bevy(eye)).looking_at(to_bevy(at), Vec3::Y);
    let mut fov = base;
    let t = fixed.overstep_fraction() as f64;
    let (mario, _) = sim.mario_at(t);
    if world.slingshot.aiming {
        let s = &world.slingshot;
        look(tf, s.origin, [s.origin[0] + s.direction[0], s.origin[1] + s.direction[1], s.origin[2] + s.direction[2]]);
    } else if let Some(q) = world.quest.as_ref().filter(|q| q.mouth_opening()) {
        let (eye, at) = q.opening_camera();
        look(tf, eye, at);
    } else if let Some((eye, at)) = world.visit.frame_conversation(mario) {
        look(tf, eye, at);
    } else if let Some(room) = &world.scene.kokiri.fixed {
        // `frameKokiriRoom`: s16 yaw/pitch, the rig's orbit yaw follows (+pi).
        let yaw = room.yaw * std::f64::consts::PI / 32768.0;
        let pitch = room.pitch * std::f64::consts::PI / 32768.0;
        let eye = room.eye;
        let at = [eye[0] + pitch.cos() * yaw.sin(), eye[1] - pitch.sin(), eye[2] + pitch.cos() * yaw.cos()];
        let dir = Vec3::new(at[0] as f32 - eye[0] as f32, at[1] as f32 - eye[1] as f32, at[2] as f32 - eye[2] as f32);
        **tf = Transform::from_translation(to_bevy(eye)).looking_to(dir, Vec3::Y);
        fov = (room.fov as f32).to_radians();
        rig.yaw = (yaw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU);
    }
    p.fov = fov;
}
