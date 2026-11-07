//! Serde mirror of `assets/courses/*.json` (the `CourseData` interface in `src/game/sm64World.ts`).
//! Unknown fields are ignored so the exporter can grow without breaking the app.
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type V3 = [f64; 3];

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CollisionData {
    #[serde(default)]
    pub rooms: Vec<i32>,
    pub vertices: Vec<V3>,
    pub triangles: Vec<[i32; 5]>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Spawn {
    pub pos: V3,
    pub face_yaw: f64,
    #[serde(default)]
    pub action: Option<u32>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CourseObject {
    #[serde(default)]
    pub model: String,
    pub pos: V3,
    #[serde(default)]
    pub yaw: f64,
    #[serde(default)]
    pub param: Option<f64>,
    #[serde(default)]
    pub behavior: Option<String>,
    #[serde(default)]
    pub acts: Option<Vec<i32>>,
    #[serde(default)]
    pub warp_to: Option<String>,
    #[serde(default)]
    pub warp_entry: Option<String>,
    #[serde(default)]
    pub warp_area: Option<i32>,
    #[serde(default)]
    pub collision: Option<String>,
    #[serde(default)]
    pub actor_scale: Option<f64>,
    #[serde(default)]
    pub rotation: Option<V3>,
    #[serde(default)]
    pub billboard: bool,
    /// Texture scroll speed (v units per second, lava / water planes).
    #[serde(default)]
    pub scroll_t: Option<f64>,
    /// Spin about local Z (radians per second).
    #[serde(default)]
    pub spin_z: Option<f64>,
    #[serde(default)]
    pub grounded_prop: bool,
    /// Signpost text pages.
    #[serde(default)]
    pub pages: Vec<String>,
    /// Ropeway lift waypoints.
    #[serde(default)]
    pub track: Option<Vec<V3>>,
    // Zelda scene objects (`zelda::scene` interprets them; the generic loader only needs to know
    // that these are managed by the Zelda plugin).
    #[serde(default)]
    pub dungeon_role: Option<String>,
    #[serde(default)]
    pub treasure_key: Option<String>,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub rupee: Option<String>,
    #[serde(default)]
    pub fairy: bool,
    #[serde(default)]
    pub idle: bool,
    #[serde(default)]
    pub recovery_heart: bool,
    /// Fairies follow their owner NPC (`"mido"`).
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CannonPlacement {
    pub pos: V3,
    #[serde(default)]
    pub yaw: f64,
    #[serde(default)]
    pub param: f64,
    #[serde(default)]
    pub acts: Option<Vec<i32>>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Door {
    pub pos: V3,
    pub yaw: f64,
    pub node: String,
    #[serde(default)]
    pub dest_node: String,
    #[serde(default)]
    pub dest_area: i32,
    #[serde(default)]
    pub destination: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WarpSpawn {
    pub pos: V3,
    pub face_yaw: f64,
    pub behavior: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WarpTarget {
    pub course: String,
    #[serde(default = "one")]
    pub area: i32,
    pub entry: String,
}
fn one() -> i32 {
    1
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FadingWarp {
    pub pos: V3,
    pub target: V3,
    pub face_yaw: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Warp {
    pub pos: V3,
    #[serde(default)]
    pub param: f64,
    pub warp_to: String,
    pub warp_entry: String,
    #[serde(default)]
    pub warp_area: Option<i32>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BallSpawner {
    pub pos: V3,
    pub path: usize,
    pub period: f64,
    #[serde(default)]
    pub acts: Vec<i32>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WaterBombSpawner {
    pub pos: V3,
    #[serde(default)]
    pub param: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlacedStar {
    pub pos: V3,
    pub act: i32,
    #[serde(default)]
    pub acts: Vec<i32>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CoinFormation {
    pub shape: String,
    pub pos: V3,
    /// s16 angle.
    #[serde(default)]
    pub yaw: f64,
    #[serde(default)]
    pub flying: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Goomba {
    pub pos: V3,
    #[serde(default)]
    pub triplet: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PenguinRace {
    pub path: Vec<[f64; 4]>,
    pub finish: V3,
    pub shortcut: V3,
}

/// One course area (`assets/courses/<id>[.<area>].json`).
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CourseData {
    #[serde(flatten)]
    pub collision: CollisionData,
    pub model: String,
    pub spawn: Spawn,
    #[serde(default)]
    pub objects: Vec<CourseObject>,
    #[serde(default)]
    pub water_boxes: Vec<[f64; 6]>,
    #[serde(default)]
    pub horizontal_boundary: Option<f64>,
    #[serde(default)]
    pub slide_terrain: bool,
    #[serde(default)]
    pub snow_terrain: bool,
    #[serde(default)]
    pub room_models: Vec<String>,
    #[serde(default)]
    pub doors: Vec<Door>,
    #[serde(default)]
    pub warps: Vec<Warp>,
    #[serde(default)]
    pub warp_spawns: HashMap<String, WarpSpawn>,
    #[serde(default)]
    pub death_warp: Option<WarpTarget>,
    #[serde(default)]
    pub fall_warp: Option<WarpTarget>,
    #[serde(default)]
    pub success_entry: Option<String>,
    #[serde(default)]
    pub success_area: Option<i32>,
    #[serde(default)]
    pub fading_warps: Vec<FadingWarp>,
    #[serde(default)]
    pub coins: Vec<V3>,
    #[serde(default)]
    pub coin_formations: Vec<CoinFormation>,
    #[serde(default)]
    pub red_coins: Vec<V3>,
    #[serde(default)]
    pub goombas: Vec<Goomba>,
    #[serde(default)]
    pub mountain_ball_paths: Vec<Vec<V3>>,
    #[serde(default)]
    pub mountain_ball_spawners: Vec<BallSpawner>,
    #[serde(default)]
    pub water_bomb_spawners: Vec<WaterBombSpawner>,
    #[serde(default)]
    pub placed_stars: Vec<PlacedStar>,
    #[serde(default)]
    pub snowman_path: Option<Vec<V3>>,
    #[serde(default)]
    pub snowman_star: Option<SnowmanStar>,
    #[serde(default)]
    pub penguin_race: Option<PenguinRace>,
    #[serde(default)]
    pub cannons: Vec<CannonPlacement>,
    #[serde(default)]
    pub extra_collision: HashMap<String, CollisionData>,
    #[serde(default)]
    pub ship_collision: Vec<ShipPart>,
}

/// A rotated static collision part (the Jolly Roger Bay sunken ship): `shipCollision`.
#[derive(Deserialize, Clone, Debug)]
pub struct ShipPart {
    pub name: String,
    pub acts: Vec<i32>,
    pub pos: V3,
    /// s16 (pitch, yaw, roll).
    pub rotation: V3,
}

/// `snowmanStar` in the course json: the act whose star completes the snowman.
#[derive(Clone, Debug, Deserialize)]
pub struct SnowmanStar {
    pub act: i32,
}

impl CourseData {
    pub fn path(root: &Path, id: &str, area: i32) -> PathBuf {
        let name = if area > 1 { format!("{id}.{area}.json") } else { format!("{id}.json") };
        root.join("courses").join(name)
    }

    pub fn load(root: &Path, id: &str, area: i32) -> Result<Self, String> {
        let path = Self::path(root, id, area);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}
