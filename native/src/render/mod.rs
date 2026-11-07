//! Rendering: everything that turns [`GameSim`] state into Bevy entities. The sim is in SM64
//! units; the exported glb models are baked at 1 unit = 0.01 m, so world positions are scaled
//! by [`SCALE`] and models are placed at scale 1.
pub mod billboard;
pub mod course_view;
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
            mario_view::MarioViewPlugin,
            object_view::ObjectViewPlugin,
            pickup_view::PickupViewPlugin,
            billboard::BillboardPlugin,
        ));
    }
}
