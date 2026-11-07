//! `geometry.ts` render half: the real SM64 course textures used by generated geometry, as
//! nearest-filtered world-tiling materials, and boxes whose UVs tile per world unit.
use super::Host;
use crate::render::SCALE;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use jumpman_sim::math::Vec3 as V3;

/// `TEXTURES` of the TS: name -> path under the asset root.
pub fn texture_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "grassTop" => "extracted/textures/generic/bob_textures.05800.rgba16.png",
        "grassEdge" => "extracted/textures/grass/wf_textures.05000.rgba16.png",
        "dirt" => "extracted/textures/grass/wf_textures.04800.rgba16.png",
        "stone" => "extracted/textures/generic/bob_textures.05000.rgba16.png",
        "flagstone" => "extracted/textures/grass/wf_textures.08000.rgba16.png",
        "sandstone" => "extracted/textures/grass/wf_textures.07000.rgba16.png",
        "brick" => "extracted/textures/grass/wf_textures.09000.rgba16.png",
        "planks" => "extracted/textures/generic/bob_textures.06000.rgba16.png",
        "gold" => "extracted/textures/generic/bob_textures.0A800.rgba16.png",
        "checker" => "extracted/textures/sky/rr_textures.08000.rgba16.png",
        "tiles" => "extracted/textures/sky/rr_textures.03000.rgba16.png",
        "whiteBrick" => "extracted/textures/sky/rr_textures.01000.rgba16.png",
        "rainbow" => "extracted/textures/sky/rr_textures.05800.rgba16.png",
        _ => return None,
    })
}

/// A repeating, nearest-filtered texture (`sm64Texture`); `None` when the file is missing.
pub fn texture(host: &Host, name: &str) -> Option<Handle<Image>> {
    let path = texture_path(name)?;
    if !host.asset_root.exists(path) {
        return None;
    }
    Some(host.assets.load_with_settings(path.to_string(), |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Nearest,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
    }))
}

/// A lit material with the named texture (flat `tint` colour when the texture is unavailable).
pub fn textured(host: &mut Host, name: &str, tint: Color) -> Handle<StandardMaterial> {
    let map = texture(host, name);
    let flat = map.is_none();
    host.materials.add(StandardMaterial { base_color: if flat { fallback(name, tint) } else { tint }, base_color_texture: map, perceptual_roughness: 0.85, ..default() })
}

fn fallback(name: &str, tint: Color) -> Color {
    let c = match name {
        "grassTop" => [0.36, 0.66, 0.25],
        "grassEdge" | "dirt" => [0.45, 0.32, 0.2],
        "stone" => [0.6, 0.6, 0.62],
        "flagstone" => [0.62, 0.55, 0.45],
        "sandstone" => [0.82, 0.7, 0.45],
        "brick" => [0.62, 0.3, 0.24],
        "planks" => [0.62, 0.43, 0.26],
        "gold" => [1.0, 0.85, 0.3],
        "checker" => [0.85, 0.7, 0.95],
        "rainbow" => [1.0, 0.55, 0.75],
        _ => [0.95, 0.95, 1.0],
    };
    let t = tint.to_srgba();
    Color::srgb(c[0] * t.red, c[1] * t.green, c[2] * t.blue)
}

/// Centred box in Bevy units from an SM64-unit size; UVs repeat every `tile` SM64 units.
pub fn tiled_box(size: V3, tile: f64) -> Mesh {
    let h = Vec3::new(size[0] as f32, size[1] as f32, size[2] as f32) * SCALE / 2.0;
    let t = tile as f32 * SCALE;
    let faces: [(Vec3, Vec3); 6] = [(Vec3::X, Vec3::Y), (Vec3::NEG_X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::NEG_Y, Vec3::Z), (Vec3::Z, Vec3::Y), (Vec3::NEG_Z, Vec3::Y)];
    let (mut pos, mut nor, mut uv, mut idx) = (vec![], vec![], vec![], vec![]);
    for (n, u) in faces {
        let r = u.cross(n);
        let (hn, hr, hu) = ((n * h).length(), (r * h).length(), (u * h).length());
        let base = pos.len() as u32;
        for (sr, su) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = n * hn + r * (sr * hr) + u * (su * hu);
            pos.push(p.to_array());
            nor.push(n.to_array());
            uv.push([(sr * hr + hr) * 2.0 / 2.0 / t * 1.0, (hu - su * hu) / t]);
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(idx));
    mesh
}

/// Horizontal quad (facing up) centred at the origin, UVs tiling every `tile` SM64 units.
pub fn tiled_top(size_x: f64, size_z: f64, tile: f64) -> Mesh {
    let (hx, hz) = (size_x as f32 * SCALE / 2.0, size_z as f32 * SCALE / 2.0);
    let t = tile as f32 * SCALE;
    let pos = vec![[-hx, 0.0, hz], [hx, 0.0, hz], [hx, 0.0, -hz], [-hx, 0.0, -hz]];
    let uv = vec![[0.0, 2.0 * hz / t], [2.0 * hx / t, 2.0 * hz / t], [2.0 * hx / t, 0.0], [0.0, 0.0]];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiled_box_has_six_faces_with_outward_normals() {
        let mesh = tiled_box([200.0, 100.0, 400.0], 100.0);
        assert_eq!(mesh.count_vertices(), 24);
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        for (n, p) in n.iter().zip(p) {
            assert!(Vec3::from(*n).dot(Vec3::from(*p)) > 0.0, "normal points away from the centre");
        }
    }
}
