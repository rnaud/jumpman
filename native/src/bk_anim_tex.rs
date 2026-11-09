//! Animated textures of the BK levels (water, fire, flags...): the exporter (`byor-rs/src/bk/levels.rs`) names the glb material of an
//! animated texture `bkanim<n>` and lists its other frames (PNGs next to the glb) and rate in `anim.json`
//! (`BKAnimTextureList` of the model header; `AnimTextureListCache_update` advances `frame_rate x dt` frames, wrapping at `frame_cnt`).
//! The glb's own image is frame 0. This module binds those materials when the level scene spawns and swaps their base texture over time.
use crate::game::runner::GameSim;
use crate::render::AssetRoot;
use bevy::asset::AssetServer;
use bevy::gltf::GltfMaterialName;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
pub struct Def {
    pub material: String,
    pub rate: f64,
    pub wrap: [usize; 2],
    pub frames: Vec<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct AnimFile {
    pub textures: Vec<Def>,
}

pub fn parse(text: &str) -> Result<AnimFile, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Frame shown `t` seconds after the level started: frame 0 is the glb's image, `count` frames in all.
pub fn frame_index(t: f64, rate: f64, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    ((t * rate).max(0.0) as usize) % count
}

struct Bound {
    material: Handle<StandardMaterial>,
    def: usize,
    first: Option<Handle<Image>>,
}

#[derive(Resource, Default)]
pub struct LevelAnims {
    course: String,
    defs: Vec<Def>,
    frames: Vec<Vec<Handle<Image>>>,
    by_name: HashMap<String, usize>,
    bound: Vec<Bound>,
    clock: f64,
}

pub struct BkAnimTexturePlugin;

impl Plugin for BkAnimTexturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelAnims>().add_systems(Update, (load_level_anims, bind_materials, animate).chain());
    }
}

fn wrap_mode(i: usize) -> ImageAddressMode {
    match i {
        0 => ImageAddressMode::Repeat,
        2 | 3 => ImageAddressMode::ClampToEdge,
        _ => ImageAddressMode::MirrorRepeat,
    }
}

/// The directory (below the assets root) of the glb a course uses, from its course file's `model`.
fn model_dir(root: &std::path::Path, course: &str) -> Option<String> {
    let text = std::fs::read_to_string(root.join("courses").join(format!("{course}.json"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let model = v.get("model")?.as_str()?;
    Some(std::path::Path::new(model).parent()?.to_string_lossy().into_owned())
}

fn load_level_anims(sim: Option<Res<GameSim>>, root: Res<AssetRoot>, server: Option<Res<AssetServer>>, mut anims: ResMut<LevelAnims>) {
    let (Some(sim), Some(server)) = (sim, server) else { return };
    let id = sim.course.request.id.as_str();
    if anims.course == id {
        return;
    }
    *anims = LevelAnims { course: id.to_string(), ..Default::default() };
    if !id.starts_with("bk_") {
        return;
    }
    let Some(dir) = model_dir(&root.0, id) else { return };
    let Ok(text) = std::fs::read_to_string(root.0.join(&dir).join("anim.json")) else { return };
    let Ok(file) = parse(&text) else { return };
    for def in file.textures {
        let wrap = def.wrap;
        let frames: Vec<Handle<Image>> = def
            .frames
            .iter()
            .map(|f| {
                server
                    .load_builder()
                    .with_settings(move |s: &mut ImageLoaderSettings| {
                        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: wrap_mode(wrap[0]), address_mode_v: wrap_mode(wrap[1]), ..default() });
                    })
                    .load::<Image>(format!("{dir}/{f}"))
            })
            .collect();
        let n = anims.defs.len();
        let a = &mut *anims;
        a.by_name.insert(def.material.clone(), n);
        a.defs.push(def);
        a.frames.push(frames);
    }
}

fn bind_materials(new: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>), Added<GltfMaterialName>>, materials: Res<Assets<StandardMaterial>>, mut anims: ResMut<LevelAnims>) {
    for (name, mat) in &new {
        let Some(&def) = anims.by_name.get(&name.0) else { continue };
        let first = materials.get(&mat.0).and_then(|m| m.base_color_texture.clone());
        anims.bound.push(Bound { material: mat.0.clone(), def, first });
    }
}

fn animate(time: Res<Time>, mut anims: ResMut<LevelAnims>, mut materials: ResMut<Assets<StandardMaterial>>) {
    anims.clock += time.delta_secs_f64();
    let t = anims.clock;
    let a = &*anims;
    for b in &a.bound {
        let def = &a.defs[b.def];
        let frames = &a.frames[b.def];
        let i = frame_index(t, def.rate, frames.len() + 1);
        let want = if i == 0 { b.first.clone() } else { frames.get(i - 1).cloned() };
        if let Some(mut m) = materials.get_mut(&b.material) {
            if m.base_color_texture != want {
                m.base_color_texture = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_index_follows_the_rate_and_wraps_like_the_decomp() {
        // 10 frames per second over 20 frames: one lap every 2 s
        assert_eq!(frame_index(0.0, 10.0, 20), 0);
        assert_eq!(frame_index(0.35, 10.0, 20), 3);
        assert_eq!(frame_index(2.0, 10.0, 20), 0);
        assert_eq!(frame_index(2.55, 10.0, 20), 5);
        assert_eq!(frame_index(1.0, 5.0, 2), 1);
        assert_eq!(frame_index(5.0, 0.0, 2), 0);
        assert_eq!(frame_index(1.0, 5.0, 0), 0);
    }

    #[test]
    fn the_anim_json_format_parses() {
        let f = parse(r#"{"textures": [{"material": "bkanim0", "rate": 4.0, "wrap": [0, 0], "frames": ["anim/a0_1.png"]}]}"#).unwrap();
        assert_eq!((f.textures[0].material.as_str(), f.textures[0].frames.len()), ("bkanim0", 1));
        assert!(parse("{").is_err());
    }

    fn assets() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// The exported levels (needs the importer's output): Gobi's Valley has a 21-frame texture at 10 fps whose frames differ and whose
    /// files decode; the lair's furnace room has a 2-frame one.
    #[test]
    fn the_exported_levels_carry_their_animated_textures() {
        let dir = assets().join("levels/bk_gobis_valley");
        let Ok(text) = std::fs::read_to_string(dir.join("anim.json")) else {
            eprintln!("skipped: no exported BK levels");
            return;
        };
        let f = parse(&text).unwrap();
        let t = &f.textures[0];
        assert_eq!((t.frames.len(), t.rate), (20, 10.0), "21 frames: the glb image plus 20 files");
        let mut seen = std::collections::HashSet::new();
        for file in &t.frames {
            let bytes = std::fs::read(dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            seen.insert(bytes);
        }
        assert!(seen.len() >= 16, "frames differ: {} distinct", seen.len());
        // the glb names its material so the renderer can find it
        let glb = std::fs::read(dir.join("terrain.glb")).unwrap();
        assert!(String::from_utf8_lossy(&glb).contains("bkanim0"));
        let furnace = parse(&std::fs::read_to_string(assets().join("levels/bk_gl_furnace_fun/anim.json")).unwrap()).unwrap();
        assert_eq!((furnace.textures[0].frames.len(), furnace.textures[0].rate), (2, 4.0));
    }

    #[test]
    fn the_animator_swaps_a_bound_materials_texture_over_time() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).init_asset::<Image>().init_asset::<StandardMaterial>().init_resource::<LevelAnims>();
        let (first, f1, f2) = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            (images.add(Image::default()), images.add(Image::default()), images.add(Image::default()))
        };
        let mat = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial { base_color_texture: Some(first.clone()), ..default() });
        app.world_mut().spawn((GltfMaterialName("bkanim0".into()), MeshMaterial3d(mat.clone())));
        {
            let mut a = app.world_mut().resource_mut::<LevelAnims>();
            a.defs.push(Def { material: "bkanim0".into(), rate: 1000.0, wrap: [0, 0], frames: vec![] });
            a.frames.push(vec![f1.clone(), f2.clone()]);
            a.by_name.insert("bkanim0".into(), 0);
        }
        app.add_systems(Update, (bind_materials, animate).chain());
        let mut seen = std::collections::HashSet::new();
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            app.update();
            let m = app.world().resource::<Assets<StandardMaterial>>().get(&mat).unwrap();
            seen.insert(m.base_color_texture.clone().map(|h| h.id()));
        }
        assert_eq!(app.world().resource::<LevelAnims>().bound.len(), 1);
        assert!(seen.len() >= 3, "the material showed {} different textures", seen.len());
        assert!(seen.contains(&Some(first.id())) && seen.contains(&Some(f1.id())) && seen.contains(&Some(f2.id())));
    }
}
