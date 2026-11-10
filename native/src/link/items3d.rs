//! The static OoT item models (`scripts/export-oot-items.py`): bomb cap and body (the body is the billboarded quad of
//! `gBombBodyDL`), bombchu, boomerang, arrow, hookshot tip and chain, Deku nut and stick. Meshes are baked once at start-up
//! at the Link scale (1 model unit = 0.044 sim units, actor scale 0.01) and instanced with a transform every frame.
use crate::characters::assets::{self, LinkAsset};
use crate::characters::{empty_mesh, make_image, material, write_mesh};
use crate::render::{AssetRoot, SCALE};
use bevy::prelude::*;
use std::collections::HashMap;

pub const NAMES: [&str; 9] = ["bomb_body", "bomb_cap", "bombchu", "boomerang", "arrow", "hookshot_tip", "hookshot_chain", "deku_nut", "deku_stick"];
/// Model units to bevy units at the stock actor scale 0.01 (x 4.4 sim units per OoT unit).
pub const MODEL_TO_BEVY: f32 = 0.044 * SCALE;
/// `EnBom` flash colour `(flashIntensity, 0, 40)` is quantised to this many steps (0..=140).
pub const FLASH_STEPS: usize = 8;

pub struct Part {
    pub mesh: Handle<Mesh>,
    pub mat: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct ItemModels {
    pub parts: HashMap<&'static str, Vec<Part>>,
    /// The bomb body quad tinted for each flash step (`gDPSetPrimColor(flashIntensity, 0, 40)`).
    pub bomb_flash: Vec<Handle<StandardMaterial>>,
}

/// OoT was imported while the game runs: load the item models that were missing at launch (the primitives stood in for them).
pub fn reload(world: &mut World) {
    if world.get_resource::<ItemModels>().is_some_and(|m| m.parts.is_empty()) {
        let _ = world.run_system_cached(load_models);
    }
}

pub struct ItemModelsPlugin;

impl Plugin for ItemModelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemModels>().add_systems(Startup, load_models);
    }
}

fn load_models(root: Res<AssetRoot>, mut models: ResMut<ItemModels>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    for name in NAMES {
        let asset = match assets::load_item(&root.0, name) {
            Ok(a) => a,
            Err(e) => {
                warn!("OoT item model {name} is missing ({e}); the primitive stand-in is drawn");
                continue;
            }
        };
        let parts = build_parts(&asset, &mut meshes, &mut materials, &mut images);
        if name == "bomb_body" && !parts.is_empty() {
            // Unlit tinted copies of the body's material, one per flash step.
            let tex = materials.get(&parts[0].mat).and_then(|m| m.base_color_texture.clone());
            models.bomb_flash = (0..FLASH_STEPS)
                .map(|i| {
                    let f = bomb_flash_at(i);
                    let mut m = material(Color::srgb(f / 255.0 + 0.05, 0.03, 40.0 / 255.0 + 0.07), tex.clone());
                    m.unlit = true;
                    materials.add(m)
                })
                .collect();
        }
        models.parts.insert(name, parts);
    }
}

/// The `flashIntensity` (0..=140) of flash step `i`.
pub fn bomb_flash_at(i: usize) -> f32 {
    140.0 * i as f32 / (FLASH_STEPS - 1) as f32
}

fn build_parts(asset: &LinkAsset, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, images: &mut Assets<Image>) -> Vec<Part> {
    let mut textures: HashMap<String, Handle<Image>> = HashMap::new();
    let mut out = vec![];
    for b in &asset.batches {
        let tex = b.texture.as_ref().and_then(|name| {
            let spec = asset.textures.get(name)?;
            Some(textures.entry(format!("{name}/{:?}", b.wrap)).or_insert_with(|| images.add(make_image(spec, b.wrap, true))).clone())
        });
        let (w, h) = b.texture.as_ref().and_then(|n| asset.textures.get(n)).map_or((1.0, 1.0), |s| (s.width as f64, s.height as f64));
        let uvs = b.vertices.iter().map(|v| [((v[3] / 32.0 + 0.5) / w) as f32, ((v[4] / 32.0 + 0.5) / h) as f32]).collect();
        let mut mesh = empty_mesh(uvs, None);
        let positions: Vec<[f32; 3]> = b.vertices.iter().map(|v| [v[0] as f32 * MODEL_TO_BEVY, v[1] as f32 * MODEL_TO_BEVY, v[2] as f32 * MODEL_TO_BEVY]).collect();
        let normals: Vec<[f32; 3]> = b.vertices.iter().map(|v| Vec3::new(v[5] as f32, v[6] as f32, v[7] as f32).normalize_or_zero().to_array()).collect();
        write_mesh(&mut mesh, positions, normals);
        let color = Color::srgb_u8((b.color >> 16) as u8, (b.color >> 8) as u8, b.color as u8);
        out.push(Part { mesh: meshes.add(mesh), mat: materials.add(material(color, tex)) });
    }
    out
}

/// Spawn model `name` at `tf` carrying `tag` (the caller's per-frame cleanup component); children are the batches.
/// `mat` replaces every part's material (the bomb flash tint). Returns false when the model did not load.
pub fn spawn_model<T: Component + Clone>(commands: &mut Commands, models: &ItemModels, name: &str, tf: Transform, tag: T, mat: Option<&Handle<StandardMaterial>>) -> bool {
    let Some(parts) = models.parts.get(name) else { return false };
    if parts.is_empty() {
        return false;
    }
    commands.spawn((tag, tf, Visibility::Inherited)).with_children(|p| {
        for part in parts {
            p.spawn((Mesh3d(part.mesh.clone()), MeshMaterial3d(mat.unwrap_or(&part.mat).clone()), Visibility::Inherited));
        }
    });
    true
}

/// EnBom `flashIntensity` for a bomb with `frames` OoT frames of fuse left (`z_en_bom.c`, `EnBom_Update`): the flash speed
/// scale starts at 7 and halves at 40, 20 and 3 frames; the intensity ramps toward 140 while `timer & (scale + 1)` is set
/// and back to 0 otherwise. `intensity` and `scale` carry between frames.
pub fn bomb_flash_step(frames: i32, scale: i32, intensity: f32) -> (f32, i32) {
    let scale = if matches!(frames, 3 | 20 | 40) { scale >> 1 } else { scale };
    let rate = 140.0 / scale.max(1) as f32;
    let target = if frames < 100 && (frames & (scale + 1)) != 0 { 140.0 } else { 0.0 };
    let step = (target - intensity).clamp(-rate, rate);
    (intensity + step, scale)
}

/// The flash intensity of a bomb with `frames` of fuse left, replayed from the start of the fuse (stateless for the view).
pub fn bomb_flash_at_frames(frames: i32) -> f32 {
    let (mut i, mut s) = (0.0, 7);
    let mut f = 70;
    while f >= frames.max(0) {
        (i, s) = bomb_flash_step(f, s, i);
        f -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bomb_flash_speeds_up_as_the_fuse_burns() {
        let edges = |from: i32, to: i32| {
            let (mut i, mut s) = (0.0, 7);
            let mut n = 0;
            let mut on = false;
            for f in (0..=70).rev() {
                (i, s) = bomb_flash_step(f, s, i);
                if f <= from && f >= to {
                    if (i > 70.0) != on {
                        n += 1;
                    }
                }
                on = i > 70.0;
            }
            n as f32 / (from - to + 1) as f32
        };
        assert!(edges(19, 3) > edges(69, 40), "late {} early {}", edges(19, 3), edges(69, 40));
    }

    #[test]
    fn bomb_flash_scale_halves_at_the_marks_and_intensity_stays_in_range() {
        let (mut i, mut s) = (0.0, 7);
        let mut seen = vec![];
        for f in (0..70).rev() {
            (i, s) = bomb_flash_step(f, s, i);
            assert!((0.0..=140.0).contains(&i));
            if matches!(f, 40 | 20 | 3) {
                seen.push(s);
            }
        }
        assert_eq!(seen, vec![3, 1, 0]);
    }

    #[test]
    fn flash_steps_cover_the_whole_range() {
        assert_eq!(bomb_flash_at(0), 0.0);
        assert_eq!(bomb_flash_at(FLASH_STEPS - 1), 140.0);
        assert!(bomb_flash_at_frames(30) >= 0.0 && bomb_flash_at_frames(30) <= 140.0);
    }
}
