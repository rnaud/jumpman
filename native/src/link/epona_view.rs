//! Epona on screen: the skinned `object_horse` model (CPU-skinned like Link: each vertex is the weighted sum of up to
//! three limb matrices, `Skin_ApplyLimbModifications`), her nine animations, the blink, and Link sitting on the saddle.
//!
//! The rider is placed the way `EnHorse_PostDraw` does it: limb 30 (the saddle) transformed by `(600, -1670, 0)`.
use crate::characters::assets::{self, EponaAsset};
use crate::characters::link_pose::pose_limbs;
use crate::characters::{empty_mesh, make_image, material, write_mesh, Character, CharacterRoot};
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::prelude::*;
use bevy::render::view::NoFrustumCulling;
use jumpman_sim::link::epona::Epona;
use jumpman_sim::math::to_s16;
use std::collections::HashMap;
use std::sync::Arc;

pub struct EponaViewPlugin;

impl Plugin for EponaViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EponaRig>()
            .add_systems(Update, (spawn_epona, update_epona).chain().after(crate::characters::update_rigs).before(crate::CameraSet));
    }
}

/// Model units -> sim units (the actor scale 0.01 times the 4.4 course scale, as for Link).
pub const MODEL_SCALE: f32 = 0.044;
/// Limb that carries the saddle and the point where the rider's origin sits (`EnHorse_PostDraw`).
pub const SADDLE_LIMB: usize = 30;
/// OoT units from the saddle point down to Link's origin.
pub const RIDER_DROP: f32 = 66.0;
pub const RIDER_OFFSET: Vec3 = Vec3::new(600.0, -1670.0, 0.0);
const EYE_OPEN: &str = "gEponaEyeOpenTex";
const BLINK: [usize; 4] = [0, 1, 2, 1];

#[derive(Resource, Default)]
pub struct EponaRig {
    root: Option<Entity>,
    failed: bool,
    batches: Vec<(Entity, Handle<Mesh>)>,
    asset: Option<Arc<EponaAsset>>,
    eye: Option<(Handle<StandardMaterial>, [Handle<Image>; 3])>,
    /// World-space saddle (rider) point of the last frame, for the camera and the rider.
    pub saddle: Option<Vec3>,
}

#[allow(clippy::too_many_arguments)]
fn spawn_epona(
    mut commands: Commands,
    mut rig: ResMut<EponaRig>,
    mut sim: ResMut<GameSim>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if rig.root.is_some() || rig.failed || !sim.mario.link.epona.present {
        return;
    }
    let asset = match assets::load_epona(&root.0) {
        Ok(a) => Arc::new(a),
        Err(e) => {
            warn!("could not load Epona: {e}");
            rig.failed = true;
            return;
        }
    };
    // The jump animations' root heights drive her body in the air (`EnHorse_LowJump` / `HighJump`).
    let ys = |name: &str| asset.clips[name].iter().map(|f| f[1] * 0.01).collect::<Vec<f64>>();
    sim.mario.link.epona.set_jump_profiles(ys("jump"), ys("jumpHigh"));
    let root_entity = commands.spawn((Transform::default(), Visibility::Hidden)).id();
    let mut textures: HashMap<String, Handle<Image>> = HashMap::new();
    let mut batches = Vec::new();
    let mut eye = None;
    for b in &asset.batches {
        let tex = b.texture.as_ref().and_then(|name| {
            let spec = asset.textures.get(name)?;
            Some(textures.entry(format!("{name}/{:?}", b.wrap)).or_insert_with(|| images.add(make_image(spec, b.wrap, true))).clone())
        });
        let (w, h) = b.texture.as_ref().and_then(|n| asset.textures.get(n)).map_or((1.0, 1.0), |s| (s.width as f64, s.height as f64));
        let uvs = b.vertices.iter().map(|v| [((v[0] / 32.0 + 0.5) / w) as f32, ((v[1] / 32.0 + 0.5) / h) as f32]).collect();
        let mesh = meshes.add(empty_mesh(uvs, None));
        let color = Color::srgb_u8((b.color >> 16) as u8, (b.color >> 8) as u8, b.color as u8);
        let mat = materials.add(material(color, tex.clone()));
        if b.texture.as_deref() == Some(EYE_OPEN) {
            let mut eye_image = |name: &str| images.add(make_image(&asset.textures[name], b.wrap, true));
            eye = Some((mat.clone(), [tex.clone().unwrap_or_default(), eye_image("gEponaEyeHalfTex"), eye_image("gEponaEyeClosedTex")]));
        }
        let e = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), NoFrustumCulling, Transform::default(), Visibility::Inherited)).id();
        commands.entity(root_entity).add_child(e);
        batches.push((e, mesh));
    }
    rig.root = Some(root_entity);
    rig.batches = batches;
    rig.asset = Some(asset);
    rig.eye = eye;
}

/// Epona's interpolated pose for drawing: position, yaw (s16), pitch and roll.
pub fn pose_at(e: &Epona, t: f64) -> (Vec3, f32, f32, f32) {
    let jump = (0..3).map(|i| (e.pos[i] - e.prev_pos[i]).abs()).fold(0.0, f64::max);
    let pos = if jump > 600.0 { e.pos } else { [0, 1, 2].map(|i| e.prev_pos[i] + (e.pos[i] - e.prev_pos[i]) * t) };
    let yaw = to_s16(e.prev_yaw + to_s16(e.yaw - e.prev_yaw) as f64 * t) as f64;
    (to_bevy(pos), (yaw * std::f64::consts::PI / 32768.0) as f32, e.pitch as f32, e.roll as f32)
}

#[allow(clippy::too_many_arguments)]
fn update_epona(
    mut rig: ResMut<EponaRig>,
    sim: Res<GameSim>,
    character: Res<Character>,
    fixed: Res<Time<Fixed>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut visibility: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform, Without<CharacterRoot>>,
    mut link_roots: Query<(&CharacterRoot, &mut Transform)>,
) {
    let Some(root) = rig.root else { return };
    let Some(asset) = rig.asset.clone() else { return };
    let e = &sim.mario.link.epona;
    let show = e.present && character.is_link();
    if let Ok(mut v) = visibility.get_mut(root) {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if !show {
        rig.saddle = None;
        return;
    }
    let t = fixed.overstep_fraction() as f64;
    let (pos, yaw, pitch, roll) = pose_at(e, t);
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll);
    let scale = MODEL_SCALE * SCALE;
    let root_tf = Transform { translation: pos, rotation, scale: Vec3::splat(scale) };
    if let Ok(mut tf) = transforms.get_mut(root) {
        *tf = root_tf;
    }
    let frames = &asset.clips[e.anim.clip()];
    // The body is drawn without the animation's root translation while jumping (`EnHorse_Draw`, setTranslation).
    let bones = pose_limbs(&asset.limbs, frames, e.frame_at(t), 1.0, false, true, !e.jumping);
    for ((_, handle), batch) in rig.batches.iter().zip(&asset.batches) {
        let Some(mesh) = meshes.get_mut(handle) else { continue };
        let mut positions = Vec::with_capacity(batch.vertices.len());
        let mut normals = Vec::with_capacity(batch.vertices.len());
        for v in &batch.vertices {
            let n = v[6] as usize;
            let mut p = Vec3::ZERO;
            for k in 0..n {
                let o = 7 + 5 * k;
                let bone = &bones[(v[o] as usize).min(bones.len() - 1)];
                p += bone.transform_point3(Vec3::new(v[o + 1] as f32, v[o + 2] as f32, v[o + 3] as f32)) * (v[o + 4] as f32 / 100.0);
            }
            positions.push(p.to_array());
            let nb = &bones[(v[5] as usize).min(bones.len() - 1)];
            normals.push(nb.transform_vector3(Vec3::new(v[2] as f32, v[3] as f32, v[4] as f32) / 127.0).normalize_or_zero().to_array());
        }
        write_mesh(mesh, positions, normals);
    }
    if let Some((mat, images)) = &rig.eye {
        if let Some(m) = materials.get_mut(mat) {
            let want = &images[BLINK[(e.blink as usize).min(3)]];
            if m.base_color_texture.as_ref() != Some(want) {
                m.base_color_texture = Some(want.clone());
            }
        }
    }
    // Link on the saddle.
    let saddle = root_tf.transform_point(bones[SADDLE_LIMB].transform_point3(RIDER_OFFSET));
    rig.saddle = Some(saddle);
    if std::env::var("JUMPMAN_EPONA_DEBUG").is_ok() && sim.ticks % 15 == 0 {
        eprintln!("stick {:?}/{:?} mag {:?} ", sim.mario.controller.stick_x, sim.mario.controller.stick_y, sim.mario.controller.stick_mag);
        eprintln!("epona t={} act={:?} anim={:?} speed={:.2} pos={:?} yaw={:.0} boosts={} jumping={}", sim.ticks, e.act, e.anim, e.speed, e.pos, e.yaw, e.boosts, e.jumping);
    }
    if e.is_mounted() {
        for (who, mut tf) in &mut link_roots {
            if who.0 == Character::Link {
                // The seated clips put Link's hip 71.5 above his origin, so the origin hangs about 66 below the saddle point
                // (the original uses `riderPos.y - 27`; with this model's saddle point that left him floating, checked on screenshots).
                tf.translation = saddle - Vec3::Y * (RIDER_DROP * 4.4 * SCALE);
                tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
            }
        }
    }
}
