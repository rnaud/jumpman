//! Playable characters beyond Mario (`Game.ts` `switchCharacter`, `CharacterView.ts`,
//! `OotLinkView.ts`, `BanjoView.ts`): adult/young Link and Banjo-Kazooie. The sim already
//! contains their controllers (`jumpman_sim::characters`); this plugin picks the active
//! character (V cycles, env `JUMPMAN_CHARACTER=link|young-link|banjo|sm64`), sets
//! `GameSim::{as_link, as_banjo}`, draws the CPU-skinned models and the Z-target marker.
//!
//! The N64 models are not glTF: they are the pre-extracted JSON assets of the web build, skinned
//! on the CPU each frame (a few thousand vertices) into plain Bevy meshes.
pub mod assets;
pub mod banjo_pose;
pub mod banjo_pose_ext;
pub mod gltf_view;
pub mod link_pose;

use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::render::mario_view::MarioRoot;
use crate::render::{to_bevy, AssetRoot, SCALE};
use assets::{BanjoAsset, LinkAsset, TextureSpec};
use banjo_pose::{BanjoAnimator, Part};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::camera::visibility::NoFrustumCulling;
use jumpman_sim::math::to_s16;
use jumpman_sim::state::*;
use link_pose::{LinkAnimator, ANGLE_TO_RAD};
use std::collections::HashMap;
use std::sync::Arc;

/// The character the player controls.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Character {
    #[default]
    Mario,
    Link,
    YoungLink,
    Banjo,
}

impl Character {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "sm64" | "mario" | "procedural" => Some(Self::Mario),
            "link" => Some(Self::Link),
            "young-link" => Some(Self::YoungLink),
            "banjo" => Some(Self::Banjo),
            _ => None,
        }
    }

    /// The id used by `assets/audio/character-sounds.json` (same ids as the web `data-character`).
    pub fn sound_id(self) -> &'static str {
        match self {
            Self::Mario => "sm64",
            Self::Link => "link",
            Self::YoungLink => "young-link",
            Self::Banjo => "banjo",
        }
    }

    /// The currency this character collects and counts in every world (coins, rupees or notes).
    pub fn theme(self) -> jumpman_sim::collectible_theme::Theme {
        use jumpman_sim::collectible_theme::Theme;
        match self {
            Self::Mario => Theme::Coin,
            Self::Link | Self::YoungLink => Theme::Rupee,
            Self::Banjo => Theme::Note,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mario => "MARIO",
            Self::Link => "ADULT LINK",
            Self::YoungLink => "YOUNG LINK",
            Self::Banjo => "BANJO-KAZOOIE",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Mario => Self::Link,
            Self::Link => Self::YoungLink,
            Self::YoungLink => Self::Banjo,
            Self::Banjo => Self::Mario,
        }
    }

    pub fn is_link(self) -> bool {
        matches!(self, Self::Link | Self::YoungLink)
    }

    /// Hint shown after starting (Game.ts `start`).
    /// The same hint worded for the on-screen buttons (a phone has no J, 1 2 3 or Tab).
    pub fn controls_hint_touch(self) -> &'static str {
        match self {
            Self::Link | Self::YoungLink => "L LOCK-ON - B SWORD - C BUTTONS AND ITEMS FOR THE REST",
            other => other.controls_hint(),
        }
    }

    /// The same hint for a gamepad (Z = R1, L = L2, R = R2; the D-pad holds Link's C items).
    pub fn controls_hint_pad(self) -> &'static str {
        match self {
            Self::Link | Self::YoungLink => "L2 LOCK-ON - B SWORD - R1 SHIELD - R2 AIM - D-PAD ITEMS",
            Self::Banjo => "B CLAW - R1 CROUCH - R2 R - D-PAD C BUTTONS",
            Self::Mario => "A = JUMP - HOLD FOR HEIGHT",
        }
    }

    pub fn controls_hint(self) -> &'static str {
        match self {
            Self::Link | Self::YoungLink => "L LOCK-ON - J SWORD - 1 2 3 ITEMS - TAB MENU",
            Self::Banjo => "J CLAW - K+SPACE FLAP FLIP - K+J BEAK BARGE",
            Self::Mario => "A = JUMP - HOLD FOR HEIGHT",
        }
    }
}

pub struct CharactersPlugin;

impl Plugin for CharactersPlugin {
    fn build(&self, app: &mut App) {
        let initial = std::env::var("JUMPMAN_CHARACTER").ok().and_then(|n| Character::from_name(&n)).unwrap_or_default();
        app.insert_resource(initial)
            .init_resource::<Rigs>()
            .init_resource::<crate::experiments::characters_support::CharacterFallback>()
            .add_systems(Startup, spawn_marker.in_set(crate::progress::PersistentSpawn))
            .add_plugins(gltf_view::GltfRigPlugin)
            .add_systems(Update, (crate::experiments::characters_support::enforce, cycle_character, apply_character, update_rigs, update_marker).chain().before(crate::CameraSet));
    }
}

fn cycle_character(slot: Res<crate::experiments::ExperimentSlot>, keys: Res<ButtonInput<KeyCode>>, mut current: ResMut<Character>, mut toast: ResMut<Toast>) {
    // Modes that cannot use the selection (Kart, Party, Halls, ...) are handled by `experiments::characters_support::enforce`.
    if slot.id.is_some_and(|id| !crate::experiments::characters_support::is_supported(id, Character::Link)) {
        return;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        *current = current.next();
        toast.show(format!("NOW PLAYING - {}", current.label()), 1.6);
    }
}

/// Mirror the selection into the sim and the Mario model's visibility (`switchCharacter`).
pub fn apply_character(
    current: Res<Character>,
    mut sim: ResMut<GameSim>,
    mut mario_root: Query<&mut Visibility, With<MarioRoot>>,
    mut applied: Local<Option<Character>>,
    mut toast: ResMut<Toast>,
    last: Option<Res<crate::last_input::LastInput>>,
) {
    // The sim is replaced by every course load (a fresh `GameSim` plays as Mario), so the selection is also re-applied
    // whenever the sim no longer matches it, not only when the selection changes.
    let stale = sim.as_link != current.is_link() || sim.as_banjo != (*current == Character::Banjo);
    if *applied == Some(*current) && !stale {
        return;
    }
    let changed = *applied != Some(*current);
    let first = applied.is_none();
    *applied = Some(*current);
    sim.as_link = current.is_link();
    sim.as_banjo = *current == Character::Banjo;
    sim.link_focus.clear();
    sim.link_opponent = None;
    if first && *current != Character::Mario {
        let hint = if last.as_ref().is_some_and(|l| l.touch()) {
            current.controls_hint_touch()
        } else if last.is_some_and(|l| l.gamepad()) {
            current.controls_hint_pad()
        } else {
            current.controls_hint()
        };
        toast.show(hint, 2.6);
    }
    if current.is_link() && !first && changed && [ACT_IDLE, ACT_WALKING, ACT_PUNCHING].contains(&sim.mario.action) {
        sim.mario.set_action(ACT_IDLE, 0.0);
    }
    for mut v in &mut mario_root {
        *v = if *current == Character::Mario { Visibility::Inherited } else { Visibility::Hidden };
    }
}

// ---------------------------------------------------------------------------------------------
// Skinned rigs
// ---------------------------------------------------------------------------------------------

pub(crate) enum RigKind {
    Link { asset: Arc<LinkAsset>, anim: LinkAnimator, young: bool, modes: Vec<String> },
    Banjo { asset: Arc<BanjoAsset>, anim: Box<BanjoAnimator>, parts: Vec<String> },
}

pub(crate) struct Rig {
    pub(crate) root: Entity,
    pub(crate) batches: Vec<(Entity, Handle<Mesh>)>,
    pub(crate) kind: RigKind,
}

#[derive(Resource, Default)]
pub struct Rigs {
    rigs: HashMap<Character, Rig>,
    failed: Vec<Character>,
}

impl Rigs {
    /// One line on Link's model data for the in-game diagnostics (`diag.rs`).
    pub fn link_diag(&self, root: &std::path::Path, who: Character) -> String {
        if !who.is_link() {
            return "not needed".into();
        }
        let file = assets::find_path(root, if who == Character::YoungLink { "oot/young-link.json" } else { "oot/link.json" });
        if !file.is_file() {
            return format!("MISSING {}", file.display());
        }
        match self.rigs.get(&who).map(|r| &r.kind) {
            Some(RigKind::Link { asset, .. }) => {
                let missing = assets::NEW_CLIPS.iter().filter(|n| !asset.clips.contains_key(**n)).count();
                format!("loaded, {} clips, {} of {} newer clips missing (re-import OoT)", asset.clips.len(), missing, assets::NEW_CLIPS.len())
            }
            _ if self.failed.contains(&who) => "FAILED TO LOAD".into(),
            _ => "loading".into(),
        }
    }
}

pub(crate) fn sampler(wrap: [usize; 2], nearest: bool) -> ImageSampler {
    let mode = |i: usize| match i {
        0 => ImageAddressMode::Repeat,
        2 => ImageAddressMode::ClampToEdge,
        _ => ImageAddressMode::MirrorRepeat,
    };
    let filter = if nearest { bevy::image::ImageFilterMode::Nearest } else { bevy::image::ImageFilterMode::Linear };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode(wrap[0]),
        address_mode_v: mode(wrap[1]),
        mag_filter: filter,
        min_filter: filter,
        ..default()
    })
}

pub(crate) fn make_image(spec: &TextureSpec, wrap: [usize; 2], nearest: bool) -> Image {
    let mut image = Image::new(
        Extent3d { width: spec.width, height: spec.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        spec.bytes(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler(wrap, nearest);
    image
}

pub(crate) fn empty_mesh(uvs: Vec<[f32; 2]>, colors: Option<Vec<[f32; 4]>>) -> Mesh {
    let n = uvs.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    if let Some(c) = colors {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, c);
    }
    mesh
}

pub(crate) fn material(color: Color, texture: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        base_color_texture: texture,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        double_sided: true,
        cull_mode: None,
        alpha_mode: AlphaMode::Mask(0.3),
        ..default()
    }
}

pub(crate) fn spawn_rig(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    root: &AssetRoot,
    which: Character,
) -> Result<Rig, String> {
    let mut batches = Vec::new();
    let mut textures: HashMap<String, Handle<Image>> = HashMap::new();
    let kind;
    let root_entity = commands.spawn((Transform::default(), Visibility::Hidden, CharacterRoot(which))).id();
    match which {
        Character::Banjo => {
            let asset = Arc::new(assets::load_banjo(&root.0)?);
            for b in &asset.batches {
                let tex = b.texture.as_ref().map(|name| {
                    textures
                        .entry(format!("{name}/{:?}", b.wrap))
                        .or_insert_with(|| images.add(make_image(&asset.textures[name], b.wrap, false)))
                        .clone()
                });
                let to_linear = |c: f64| (c / 255.0).powf(2.2) as f32;
                let uvs = b.vertices.iter().map(|v| [v[3] as f32, v[4] as f32]).collect();
                let colors = b.vertices.iter().map(|v| [to_linear(v[5]), to_linear(v[6]), to_linear(v[7]), 1.0]).collect();
                let mesh = meshes.add(empty_mesh(uvs, Some(colors)));
                let mat = materials.add(material(Color::WHITE, tex));
                let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), NoFrustumCulling, Transform::default(), Visibility::Inherited, crate::rt_scene::RtSolid)).id();
                commands.entity(root_entity).add_child(e);
                batches.push((e, mesh));
            }
            let parts = asset.batches.iter().map(|b| b.part.clone()).collect();
            kind = RigKind::Banjo { anim: Box::new(BanjoAnimator::new(&asset)), asset, parts };
        }
        link => {
            let young = link == Character::YoungLink;
            let mut link_asset = assets::load_link(&root.0, if young { "young-link" } else { "link" })?;
            // Link's riding clips (`gPlayerAnim_link_uma_*`) ship in epona.json; the sim names them `uma_<key>`. The
            // child shares the adult's 21-limb skeleton (the pose code scales the root translation), so he gets them
            // too: he never rides Epona, but the summoned kart seats him with `uma_wait1`.
            if let Ok(e) = assets::load_epona(&root.0) {
                link_asset.clips.extend(e.rider_clips.into_iter().map(|(k, v)| (format!("uma_{k}"), v)));
            }
            let asset = Arc::new(link_asset);
            for b in &asset.batches {
                let tex = b.texture.as_ref().and_then(|name| {
                    let spec = asset.textures.get(name)?;
                    Some(textures.entry(format!("{name}/{:?}", b.wrap)).or_insert_with(|| images.add(make_image(spec, b.wrap, true))).clone())
                });
                let (w, h) = b.texture.as_ref().and_then(|n| asset.textures.get(n)).map_or((1.0, 1.0), |s| (s.width as f64, s.height as f64));
                let uvs = b.vertices.iter().map(|v| [((v[3] / 32.0 + 0.5) / w) as f32, ((v[4] / 32.0 + 0.5) / h) as f32]).collect();
                let mesh = meshes.add(empty_mesh(uvs, None));
                let color = Color::srgb_u8((b.color >> 16) as u8, (b.color >> 8) as u8, b.color as u8);
                let mat = materials.add(material(color, tex));
                let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), NoFrustumCulling, Transform::default(), Visibility::Inherited, crate::rt_scene::RtSolid)).id();
                commands.entity(root_entity).add_child(e);
                batches.push((e, mesh));
            }
            let modes = asset.batches.iter().map(|b| b.mode.clone()).collect();
            kind = RigKind::Link { anim: LinkAnimator::default(), asset, young, modes };
        }
    }
    Ok(Rig { root: root_entity, batches, kind })
}

#[derive(Component)]
#[allow(dead_code)]
pub struct CharacterRoot(pub Character);

pub(crate) fn write_mesh(mesh: &mut Mesh, positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>) {
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
}

#[allow(clippy::too_many_arguments)]
pub fn update_rigs(
    mut commands: Commands,
    current: Res<Character>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    root: Res<AssetRoot>,
    mut rigs: ResMut<Rigs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut visibility: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform>,
    mut toast: ResMut<Toast>,
    mut link_bones: Option<ResMut<crate::link::combat_fx::LinkBones>>,
) {
    let active = *current;
    if active != Character::Mario && !rigs.rigs.contains_key(&active) && !rigs.failed.contains(&active) {
        match spawn_rig(&mut commands, &mut meshes, &mut materials, &mut images, &root, active) {
            Ok(rig) => {
                rigs.rigs.insert(active, rig);
            }
            Err(e) => {
                warn!("could not load character {active:?}: {e}");
                toast.show("CHARACTER FAILED TO LOAD", 3.2);
                rigs.failed.push(active);
            }
        }
    }
    // Commands are deferred: the entities exist next frame.
    let m = &sim.mario;
    let t = fixed.overstep_fraction() as f64;
    let (pos, yaw) = sim.mario_at(t);
    let mut render_yaw = yaw;
    if matches!(m.action, ACT_SPECIAL_EXIT_AIRBORNE | ACT_EXIT_AIRBORNE | ACT_FALLING_EXIT_AIRBORNE | ACT_EXIT_LAND_SAVE_DIALOG) {
        render_yaw = to_s16(render_yaw + 32768.0) as f64;
    }
    let dt = time.delta_secs_f64().min(0.1);
    for (which, rig) in rigs.rigs.iter_mut() {
        // a Mumbo transformation draws its own model (`bk_enemies::forms`)
        let show = *which == active && !(sim.as_banjo && sim.mario.banjo.form != jumpman_sim::banjo_ext::Form::Banjo);
        if let Ok(mut v) = visibility.get_mut(rig.root) {
            let want = if show { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
        if !show {
            continue;
        }
        let Ok(mut tf) = transforms.get_mut(rig.root) else { continue };
        tf.translation = to_bevy(pos);
        tf.rotation = Quat::from_rotation_y((render_yaw * ANGLE_TO_RAD) as f32);
        match &mut rig.kind {
            RigKind::Link { asset, anim, young, modes } => {
                tf.scale = Vec3::splat(0.044 * SCALE);
                let (_, bones) = anim.update(asset, *young, m, dt, true);
                if let Some(lb) = link_bones.as_mut() {
                    lb.set(asset, &bones, *tf);
                }
                // Invincibility after a hit: the model blinks (`Player_Draw` skips the draw on alternate frames).
                let blink = m.link.enabled && link_pose::blinking(m.invinc_timer, m.link.nayru > 0.0);
                let drawn = link_pose::sword_drawn(m);
                let (held, first_person) = (m.link.enabled.then(|| m.link.held()).flatten(), m.link.enabled && m.link.first_person());
                for (i, ((entity, handle), batch)) in rig.batches.iter().zip(&asset.batches).enumerate() {
                    if let Ok(mut v) = visibility.get_mut(*entity) {
                        let limb = batch.vertices.first().map_or(0, |v| v[8] as usize);
                        *v = if !blink && link_pose::batch_visible(modes[i].as_str(), limb, drawn, held, first_person) { Visibility::Inherited } else { Visibility::Hidden };
                    }
                    let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                    let mut positions = Vec::with_capacity(batch.vertices.len());
                    let mut normals = Vec::with_capacity(batch.vertices.len());
                    for v in &batch.vertices {
                        let bone = &bones[(v[8] as usize).min(bones.len() - 1)];
                        // First-person aim: head, hat and collar vertices collapse to a point (batches mix limbs, e.g. skirt and hat).
                        let local = if first_person && (10..=12).contains(&(v[8] as usize)) { Vec3::ZERO } else { Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) };
                        positions.push(bone.transform_point3(local).to_array());
                        let n = bone.transform_vector3(Vec3::new(v[5] as f32, v[6] as f32, v[7] as f32) / 127.0);
                        normals.push(n.normalize_or_zero().to_array());
                    }
                    write_mesh(&mut mesh, positions, normals);
                }
            }
            RigKind::Banjo { asset, anim, parts } => {
                tf.scale = Vec3::new(1.25, if m.squish_timer > 0.0 { 0.8 } else { 1.25 }, 1.25) * SCALE;
                tf.rotation *= Quat::from_rotation_x(banjo_pose_ext::flight_pitch(m));
                anim.update(asset, m, dt, true);
                let mats = anim.matrices(asset);
                for (i, ((entity, handle), batch)) in rig.batches.iter().zip(&asset.batches).enumerate() {
                    if let Some(part) = Part::from_name(&parts[i]) {
                        if let Ok(mut v) = visibility.get_mut(*entity) {
                            *v = if anim.shown_parts.contains(&part) { Visibility::Inherited } else { Visibility::Hidden };
                        }
                    }
                    let Some(mut mesh) = meshes.get_mut(handle) else { continue };
                    let positions: Vec<[f32; 3]> = batch
                        .vertices
                        .iter()
                        .map(|v| mats[(v[9].max(0.0) as usize).min(mats.len() - 1)].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).to_array())
                        .collect();
                    let normals = flat_normals(&positions);
                    write_mesh(&mut mesh, positions, normals);
                }
            }
        }
    }
}

/// `computeVertexNormals` on a non-indexed triangle list: one face normal per triangle.
pub fn flat_normals(positions: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity(positions.len());
    for tri in positions.chunks(3) {
        if tri.len() < 3 {
            out.extend(tri.iter().map(|_| [0.0, 1.0, 0.0]));
            continue;
        }
        let (a, b, c) = (Vec3::from(tri[0]), Vec3::from(tri[1]), Vec3::from(tri[2]));
        let n = (c - b).cross(a - b).normalize_or_zero().to_array();
        out.extend([n, n, n]);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Z-target marker (`LinkFocus.marker`: yellow downward cone over the opponent)
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct FocusMarker;

fn spawn_marker(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Cone::new(14.0 * SCALE, 24.0 * SCALE).mesh().resolution(4));
    let mat = materials.add(StandardMaterial { base_color: Color::srgb_u8(0xff, 0xdc, 0x49), unlit: true, ..default() });
    commands.spawn((FocusMarker, Mesh3d(mesh), MeshMaterial3d(mat), Transform::default().with_rotation(Quat::from_rotation_z(std::f32::consts::PI)), Visibility::Hidden));
}

fn update_marker(sim: Res<GameSim>, mut marker: Single<(&mut Transform, &mut Visibility), With<FocusMarker>>) {
    let (tf, vis) = &mut *marker;
    match sim.link_opponent {
        Some(p) => {
            let mut at = to_bevy(p);
            at.y += 32.0 * SCALE;
            tf.translation = at;
            **vis = Visibility::Inherited;
        }
        None => **vis = Visibility::Hidden,
    }
}

#[cfg(test)]
mod audit_tests;

#[cfg(test)]
pub mod test_support {
    use jumpman_sim::collision::CollisionWorld;
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
    use std::sync::Arc;

    pub fn mario() -> MarioRuntime {
        MarioRuntime::new(Arc::new(CollisionWorld::default()), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_names_and_cycle() {
        assert_eq!(Character::from_name("young-link"), Some(Character::YoungLink));
        assert_eq!(Character::from_name("sm64"), Some(Character::Mario));
        assert_eq!(Character::from_name("nope"), None);
        let mut c = Character::Mario;
        for _ in 0..4 {
            c = c.next();
        }
        assert_eq!(c, Character::Mario);
        assert!(Character::Link.is_link() && !Character::Banjo.is_link());
    }

    /// The sim is replaced by every course load; the chosen character must be applied to the new one too (the iOS "sword does
    /// nothing on Bob-omb Battlefield" bug: `apply_character` only reacted to the selection changing).
    #[test]
    fn apply_character_reapplies_to_a_replaced_sim() {
        use crate::game::runner::GameSim;
        use crate::game::world::CourseRequest;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let mut app = App::new();
        app.insert_resource(Character::Link)
            .insert_resource(GameSim::load(&root, CourseRequest::new("castle_grounds")).unwrap())
            .init_resource::<Toast>()
            .add_systems(Update, apply_character);
        app.update();
        assert!(app.world().resource::<GameSim>().as_link);
        // A course load: a fresh sim, same selection.
        *app.world_mut().resource_mut::<GameSim>() = GameSim::load(&root, CourseRequest::new("bob")).unwrap();
        assert!(!app.world().resource::<GameSim>().as_link, "a fresh sim starts as Mario");
        app.update();
        assert!(app.world().resource::<GameSim>().as_link, "Link plays the new course");
        *app.world_mut().resource_mut::<Character>() = Character::Banjo;
        *app.world_mut().resource_mut::<GameSim>() = GameSim::load(&root, CourseRequest::new("ccm")).unwrap();
        app.update();
        let sim = app.world().resource::<GameSim>();
        assert!(sim.as_banjo && !sim.as_link);
    }

    #[test]
    fn flat_normal_points_up_for_ccw_triangle_in_xz() {
        // (a - b) x (c - b) style: triangle in the XZ plane.
        let n = flat_normals(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!((n[0][1].abs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn link_and_banjo_tick_in_a_real_course_with_focus() {
        use crate::game::runner::GameSim;
        use crate::game::world::CourseRequest;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        for (link, banjo) in [(true, false), (false, true)] {
            let mut sim = GameSim::load(&root, CourseRequest::new("bob")).unwrap();
            sim.as_link = link;
            sim.as_banjo = banjo;
            sim.focus_held = true;
            let walk = ControllerSnapshot { stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
            for i in 0..120 {
                let mut c = walk.clone();
                if i % 40 == 5 {
                    c.button_pressed = A_BUTTON;
                    c.button_down = A_BUTTON;
                }
                sim.tick(&c);
            }
            assert!(sim.mario.pos.iter().all(|v| v.is_finite()));
            // Focus yaw only exists for Link.
            assert_eq!(sim.mario.controller.focus_yaw.is_some(), link);
        }
    }

    #[test]
    fn character_labels() {
        assert_eq!(Character::Banjo.label(), "BANJO-KAZOOIE");
        assert_eq!(Character::Mario.controls_hint(), "A = JUMP - HOLD FOR HEIGHT");
        // On a phone Link's hint names the on-screen buttons, not keys that do not exist there.
        assert!(Character::Link.controls_hint_touch().contains("B SWORD") && !Character::Link.controls_hint_touch().contains("TAB"));
        assert!(Character::YoungLink.controls_hint().contains("J SWORD"));
        assert!(Character::Link.controls_hint_pad().contains("R1 SHIELD") && Character::Link.controls_hint_pad().contains("L2 LOCK-ON"));
    }
}
