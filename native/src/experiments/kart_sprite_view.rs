//! Draws the MK64 kart sprite and its ground shadow (`render_kart`, `render_player_shadow`); the frame choice and the
//! numbers live in the engine-free [`super::kart_sprite`]. Used for the player and every CPU.
use crate::button_marker::LoadWithSettings;
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
    /// Item effects on the sprite (`kart_effects::Look`): lightning shrink, squish, star tint, boo alpha.
    pub look: SpriteLook,
}

/// How an effect changes a kart sprite (`change_player_color_effect_rgb`, `player->size`, `player->alpha`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteLook {
    pub size: f32,
    pub squash: f32,
    pub tint: Option<[f32; 3]>,
    pub alpha: f32,
}
impl Default for SpriteLook {
    fn default() -> Self {
        Self { size: 1., squash: 1., tint: None, alpha: 1. }
    }
}
impl SpriteLook {
    pub fn from(l: &super::kart_effects::Look) -> Self {
        Self { size: l.size as f32, squash: l.squash as f32, tint: l.tint, alpha: (l.alpha as f32 / 255.).clamp(0., 1.) }
    }
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
    host.assets.load_builder_with_settings(format!("mk64://{file}"), |s: &mut ImageLoaderSettings| {
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

/// Point an existing sprite material at another racer's kart atlas (kart setup: the player's pick, the CPUs' field).
pub fn set_character(host: &mut Host, material: &Handle<StandardMaterial>, character: usize) {
    let texture = linear(host, &sprite::frames_file(character));
    if let Some(mut m) = host.materials.get_mut(material) {
        m.base_color_texture = Some(texture);
    }
}

/// Pick the frame, mirror it and stand the quad up: a vertical billboard turned to the camera (`unk_048`), sitting
/// 4.5 course units towards the camera, rolled by the road's slope.
pub fn place_sprite(host: &mut Host, entity: Entity, material: &Handle<StandardMaterial>, i: &SpriteIn) {
    let relative = i.relative();
    let angle = sprite::view_angle(relative);
    let (frame, mirrored) = sprite::pick(angle, Pose { slope_accel: i.slope_accel, spinning: i.spinning });
    if let Some(mut m) = host.materials.get_mut(material) {
        let (s, o) = sprite::uv(frame, sprite::drawn_flipped(mirrored));
        m.uv_transform = Affine2::from_scale_angle_translation(Vec2::new(s[0], s[1]), 0., Vec2::new(o[0], o[1]));
        // Star / lightning flashes add a colour; a boo fades (blended instead of alpha-tested).
        let tint = i.look.tint.unwrap_or([0., 0., 0.]);
        let base = Color::srgba(1. + tint[0] * 2., 1. + tint[1] * 2., 1. + tint[2] * 2., i.look.alpha);
        if m.base_color != base {
            m.base_color = base;
        }
        let mode = if i.look.alpha < 0.99 { AlphaMode::Blend } else { AlphaMode::Mask(0.5) };
        if m.alpha_mode != mode {
            m.alpha_mode = mode;
        }
    }
    let to_eye = DVec3::new(i.eye.x - i.pos.x, 0., i.eye.z - i.pos.z).normalize_or_zero();
    // A shrunk or squashed kart keeps its wheels on the road.
    let high = i.look.size as f64 * i.look.squash as f64;
    let base = i.pos + to_eye * sprite::TOWARDS_CAMERA + DVec3::Y * ((sprite::SPRITE_UNITS / 2.) * high - sprite::BASE_DROP);
    let at = to_bevy(base.into());
    let eye = to_bevy(i.eye.into());
    // On the ground the quad stays vertical; in the air it tilts to face the camera (`cameras[].rot[0] - 90 deg`).
    let look = if i.midair { at - eye } else { Vec3::new(at.x - eye.x, 0., at.z - eye.z) };
    let mut t = Transform::from_translation(at).looking_to(look, Vec3::Y);
    if !i.midair {
        t.rotate_local_x(sprite::sprite_lean(i.tilt.1, angle) as f32);
    }
    t.rotate_local_z(sprite::sprite_roll(i.tilt.0, i.tilt.1, angle) as f32);
    t.scale = Vec3::new(i.look.size, i.look.size * i.look.squash, 1.);
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
            let e = host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(yaw: f64, slip: i32) -> SpriteIn {
        // The camera looks along +z (heading 0); a kart turned left has a larger yaw (browser convention).
        SpriteIn { pos: DVec3::new(0., 0., 800.), eye: DVec3::ZERO, yaw, slip, slope_accel: 0, tilt: (0., 0.), spin: 0., spinning: false, midair: false, look: SpriteLook::default() }
    }

    /// Which way the drawn kart faces on screen (-1 left, +1 right). The art's unflipped frames face screen-left, so
    /// a frame drawn with a positive horizontal UV scale faces left.
    fn facing(i: &SpriteIn) -> i32 {
        let (frame, mirrored) = sprite::pick(sprite::view_angle(i.relative()), Pose::default());
        let (scale, _) = sprite::uv(frame, sprite::drawn_flipped(mirrored));
        if scale[0] > 0. { -1 } else { 1 }
    }

    #[test]
    fn the_drawn_kart_faces_into_the_turn() {
        for t in [0.15, 0.5, 1.0, 1.5] {
            assert_eq!(facing(&input(t, 0)), -1, "turned {t} rad left must face screen-left");
            assert_eq!(facing(&input(-t, 0)), 1, "turned {t} rad right must face screen-right");
        }
    }

    #[test]
    fn the_slip_angle_turns_the_drawn_kart_further_into_the_bend() {
        // Steering left gives a negative slip (kart_physics test): kart turned further left of its heading.
        assert_eq!(facing(&input(0., -3000)), -1);
        assert_eq!(facing(&input(0., 3000)), 1);
    }
}
