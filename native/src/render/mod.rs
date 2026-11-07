//! Rendering: everything that turns [`GameSim`] state into Bevy entities. The sim is in SM64
//! units; the exported glb models are baked at 1 unit = 0.01 m, so world positions are scaled
//! by [`SCALE`] and models are placed at scale 1.
pub mod billboard;
pub mod course_view;
pub mod decal;
pub mod mario_view;
pub mod object_view;
pub mod pickup_view;

use bevy::prelude::*;
use std::path::{Path, PathBuf};

pub const SCALE: f32 = 0.01;

/// SM64 position -> Bevy world position.
pub fn to_bevy(p: [f64; 3]) -> Vec3 {
    Vec3::new(p[0] as f32 * SCALE, p[1] as f32 * SCALE, p[2] as f32 * SCALE)
}

/// Asset root directory (bundled `Resources/assets` or the repo's `assets/`).
#[derive(Resource, Clone)]
pub struct AssetRoot(pub PathBuf);

impl AssetRoot {
    pub fn exists(&self, rel: &str) -> bool {
        self.0.join(rel).is_file()
    }
}

/// Number of animations in a glb (reads only the JSON chunk).
pub fn glb_animation_count(path: &Path) -> usize {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return 0 };
    let mut head = [0u8; 20];
    if file.read_exact(&mut head).is_err() || &head[0..4] != b"glTF" {
        return 0;
    }
    let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let mut json = vec![0u8; len];
    if file.read_exact(&mut json).is_err() {
        return 0;
    }
    serde_json::from_slice::<serde_json::Value>(&json)
        .ok()
        .and_then(|v| v["animations"].as_array().map(|a| a.len()))
        .unwrap_or(0)
}

pub fn scene(assets: &AssetServer, path: &str) -> SceneRoot {
    SceneRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(path.to_owned())))
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            course_view::CourseViewPlugin,
            decal::DecalPlugin,
            mario_view::MarioViewPlugin,
            object_view::ObjectViewPlugin,
            pickup_view::PickupViewPlugin,
            billboard::BillboardPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn glb_json(path: &Path) -> serde_json::Value {
        let bytes = std::fs::read(path).unwrap();
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&bytes[20..20 + len]).unwrap()
    }

    /// SM64 adds a door clip's root translation to the animated part's own translation, and a
    /// door always has its closed clip running. The exported rest pose must therefore already be
    /// the closed pose (part x = -300 geo + -300 clip = -6.0 m before the 0.25 scale node), or
    /// hinged room doors that never start a clip sit 75 units off-centre and leave a see-through
    /// gap beside the frame (castle lobby wooden / star / key doors).
    #[test]
    fn door_models_rest_in_the_closed_pose() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/actors/door");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("glb") {
                continue;
            }
            let json = glb_json(&path);
            let root = json["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["name"].as_str().is_some_and(|s| s.starts_with("GEO_ANIMATED_PART")))
                .unwrap_or_else(|| panic!("{}: no animated part", path.display()));
            let x = root["translation"][0].as_f64().unwrap();
            assert!((x + 6.0).abs() < 1e-4, "{}: door rest-pose hinge x = {x}, want -6.0", path.display());
            assert!(json["animations"].as_array().is_some_and(|a| !a.is_empty()), "{}: no clips", path.display());
            checked += 1;
        }
        assert!(checked >= 10, "only {checked} door models found");
    }
}
