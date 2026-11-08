//! Draws the MK64 kart sprite and its ground shadow (`render_kart`, `render_player_shadow`); the frame choice and the
//! numbers live in the engine-free [`super::kart_sprite`]. Used for the player and every CPU.
use super::kart_physics::Ground;
use super::kart_sprite::{self as sprite, Pose};
use super::Host;
use crate::render::{to_bevy, SCALE};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::{Affine2, DVec3};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use std::f64::consts::PI;

/// Everything the sprite needs to know about a kart this frame (positions in sim units, `pos` the kart's feet).
#[derive(Clone, Copy, Debug)]
pub struct SpriteIn {
    pub pos: DVec3,
    pub eye: DVec3,
    /// Browser-convention heading (`-rotation[1]`), without the slip angle.
    pub yaw: f64,
    /// `unk_0C0` (s16 units).
    pub slip: i32,
    pub slope_accel: i32,
    /// `Kart::view_tilt`.
    pub tilt: (f64, f64),
    /// Extra heading while spun out by an item (radians).
    pub spin: f64,
    pub spinning: bool,
    pub midair: bool,
}

impl SpriteIn {
    /// Camera-relative view angle: `unk_048 + rotation[1] + unk_0C0` in radians.
    pub fn relative(&self) -> f64 {
        let heading = (self.pos.x - self.eye.x).atan2(self.pos.z - self.eye.z);
        let r = heading - self.yaw + self.spin + self.slip as f64 * PI / 32768.;
        r.sin().atan2(r.cos())
    }
}

fn linear(host: &Host, file: &str) -> Handle<Image> {
    host.assets.load_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: ImageAddressMode::ClampToEdge, address_mode_v: ImageAddressMode::ClampToEdge, ..ImageSamplerDescriptor::linear() });
    })
}

/// The shared quad of a kart sprite.
pub fn sprite_mesh(host: &mut Host) -> Handle<Mesh> {
    host.meshes.add(Rectangle::new(sprite::SPRITE_UNITS as f32 * SCALE, sprite::SPRITE_UNITS as f32 * SCALE))
}

/// A material showing the frame atlas of `character` (index into [`sprite::CHARACTERS`]).
pub fn sprite_material(host: &mut Host, character: usize) -> Handle<StandardMaterial> {
    let texture = linear(host, &sprite::frames_file(character));
    host.materials.add(StandardMaterial { base_color_texture: Some(texture), unlit: true, cull_mode: None, alpha_mode: AlphaMode::Mask(0.5), ..default() })
}

/// Pick the frame, mirror it and stand the quad up: a vertical billboard turned to the camera (`unk_048`), sitting
/// 4.5 course units towards the camera, rolled by the road's slope.
pub fn place_sprite(host: &mut Host, entity: Entity, material: &Handle<StandardMaterial>, i: &SpriteIn) {
    let relative = i.relative();
    let angle = sprite::view_angle(relative);
    let (frame, mirrored) = sprite::pick(angle, Pose { slope_accel: i.slope_accel, spinning: i.spinning });
    if let Some(m) = host.materials.get_mut(material) {
        let (s, o) = sprite::uv(frame, mirrored);
        m.uv_transform = Affine2::from_scale_angle_translation(Vec2::new(s[0], s[1]), 0., Vec2::new(o[0], o[1]));
    }
    let to_eye = DVec3::new(i.eye.x - i.pos.x, 0., i.eye.z - i.pos.z).normalize_or_zero();
    let base = i.pos + to_eye * sprite::TOWARDS_CAMERA + DVec3::Y * (sprite::SPRITE_UNITS / 2. - sprite::BASE_DROP);
    let at = to_bevy(base.into());
    let eye = to_bevy(i.eye.into());
    // On the ground the quad stays vertical; in the air it tilts to face the camera (`cameras[].rot[0] - 90 deg`).
    let look = if i.midair { at - eye } else { Vec3::new(at.x - eye.x, 0., at.z - eye.z) };
    let mut t = Transform::from_translation(at).looking_to(look, Vec3::Y);
    t.rotate_local_z(-sprite::screen_roll(i.tilt.0, i.tilt.1, angle) as f32);
    host.commands.entity(entity).insert((t, Visibility::Inherited));
}

/// A 1 x 1 quad lying in the XZ plane; texture row 0 is at +z (the front of the kart), column 0 at -x.
fn shadow_quad() -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-0.5, 0., 0.5], [0.5, 0., 0.5], [0.5, 0., -0.5], [-0.5, 0., -0.5]]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 1., 0.]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

/// The pooled shadow quads (`gTextureKartShadow`: black, the intensity is the alpha), hidden until placed.
pub fn spawn_shadows(host: &mut Host, count: usize) -> Vec<(Entity, Handle<StandardMaterial>)> {
    let mesh = host.meshes.add(shadow_quad());
    let texture = linear(host, "mk64-kart-shadow.png");
    (0..count)
        .map(|_| {
            let material = host.materials.add(StandardMaterial {
                base_color: Color::BLACK,
                base_color_texture: Some(texture.clone()),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                depth_bias: 4.,
                ..default()
            });
            let e = host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden, bevy::pbr::NotShadowCaster, bevy::pbr::NotShadowReceiver));
            (e, material)
        })
        .collect()
}

/// `render_player_shadow`: a 13.5 course unit square under the kart, turned with the displayed heading, lying on the
/// floor, 1 unit up and 2 units towards the camera; smaller as the kart rises and gone beyond 50 units.
pub fn place_shadow(host: &mut Host, entity: Entity, i: &SpriteIn, ground: &dyn Ground) {
    let floor = ground.floor(i.pos.x / 8., i.pos.y / 8. + 6., i.pos.z / 8.);
    let Some(floor) = floor else {
        host.commands.entity(entity).insert(Visibility::Hidden);
        return;
    };
    let height = i.pos.y - floor.height * 8.;
    let Some(scale) = sprite::shadow_scale(height, i.midair) else {
        host.commands.entity(entity).insert(Visibility::Hidden);
        return;
    };
    let to_eye = DVec3::new(i.eye.x - i.pos.x, 0., i.eye.z - i.pos.z).normalize_or_zero();
    let centre = DVec3::new(i.pos.x, floor.height * 8. + sprite::SHADOW_LIFT, i.pos.z) + to_eye * sprite::SHADOW_SHIFT;
    let normal = Vec3::new(floor.normal[0] as f32, floor.normal[1] as f32, floor.normal[2] as f32).normalize_or_zero();
    let normal = if normal == Vec3::ZERO { Vec3::Y } else { normal };
    let heading = i.yaw - i.slip as f64 * PI / 32768.;
    let rotation = Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(heading as f32);
    let size = (sprite::SPRITE_UNITS * scale) as f32 * SCALE;
    host.commands.entity(entity).insert((
        Transform::from_translation(to_bevy(centre.into())).with_rotation(rotation).with_scale(Vec3::new(size, 1., size)),
        Visibility::Inherited,
    ));
}
