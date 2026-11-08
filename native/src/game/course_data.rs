//! Serde mirror of `assets/courses/*.json` (the `CourseData` interface in `src/game/sm64World.ts`).
//! Unknown fields are ignored so the exporter can grow without breaking the app.
use serde::Deserialize;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

thread_local! {
    /// Whether `<course>.ext.json` extension files are merged on load. The golden fixtures that replay the original
    /// TypeScript sim bit-for-bit (`game_tests`, `longrun_tests`, `warps_tests`, ...) run the course exports as they were
    /// before the extensions, so tests start with them off (per test thread); the app always has them on.
    static EXTENSIONS: Cell<bool> = const { Cell::new(!cfg!(test)) };
}

/// `scripts/sm64_world.py` extensions (more warps, doors, cap boxes ...) on or off for courses loaded by this thread.
pub fn set_extensions(on: bool) {
    EXTENSIONS.with(|e| e.set(on));
}

pub fn extensions_enabled() -> bool {
    EXTENSIONS.with(Cell::get)
}

/// Merge an extension document into a course document: arrays append, objects merge key-wise, scalars replace.
pub fn merge_extension(base: &mut serde_json::Value, ext: serde_json::Value) {
    use serde_json::Value;
    let (Value::Object(base), Value::Object(ext)) = (base, ext) else { return };
    for (k, v) in ext {
        match (base.get_mut(&k), v) {
            (Some(Value::Array(a)), Value::Array(b)) => {
                for x in b {
                    if !a.contains(&x) {
                        a.push(x);
                    }
                }
            }
            (Some(Value::Object(a)), Value::Object(b)) => a.extend(b),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// The course document (`assets/courses/<id>[.<area>].json`) with its `.ext.json` merged (see [`set_extensions`]).
pub fn read_course_value(root: &Path, id: &str, area: i32) -> Result<serde_json::Value, String> {
    let path = CourseData::path(root, id, area);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if extensions_enabled() {
        let ext = path.with_extension("ext.json");
        if let Ok(bytes) = std::fs::read(&ext) {
            let ext = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", ext.display()))?;
            merge_extension(&mut value, ext);
        }
    }
    Ok(value)
}

pub type V3 = [f64; 3];

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CollisionData {
    #[serde(default)]
    pub rooms: Vec<i32>,
    pub vertices: Vec<V3>,
    pub triangles: Vec<[i32; 5]>,
    /// Indices of the triangles an OoT hookshot latches onto (`SurfaceType_CanHookshot`); `None` for courses without marks.
    #[serde(default)]
    pub hookable: Option<Vec<u32>>,
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
    /// Added by `scripts/export-sm64-g1-actors.py` (SM64 group 1 actors, `behaviors/g1`).
    #[serde(default)]
    pub g1: bool,
    /// Added by `scripts/export-sm64-castle.py` (SM64 round 2 part B, `behaviors/part_b`); gated like `g2`.
    #[serde(default)]
    pub pb: bool,
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
    /// SM64 group 2 placement (`scripts/export_sm64_g2.py`): only loaded with `CourseRequest::g2`.
    #[serde(default)]
    pub g2: bool,
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
    #[serde(default)]
    pub g2: bool,
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
    #[serde(default)]
    pub g2: bool,
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

/// Per-course sky dome stops and fog (`fx::sky`), sRGB 0..1, fog distances in SM64 units.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Sky {
    pub top: [f32; 3],
    pub mid: [f32; 3],
    pub bottom: [f32; 3],
    pub fog: [f32; 3],
    pub fog_start: f32,
    pub fog_end: f32,
}

/// An interactive OoT prop of a course (`props` in the json): `k` is the kind (`jumpman_sim::link::props::PropKind::parse`).
#[derive(Deserialize, Clone, Debug, Default)]
pub struct PropSpec {
    pub k: String,
    pub pos: V3,
    #[serde(default)]
    pub yaw: f64,
    /// Switch flag it sets (grass and pots have none).
    #[serde(default)]
    pub flag: Option<i32>,
    #[serde(default)]
    pub table: Option<u8>,
    #[serde(default)]
    pub item: Option<u8>,
    /// Torch burn-time parameter.
    #[serde(default)]
    pub count: i32,
    #[serde(default)]
    pub timed: i32,
    #[serde(default)]
    pub lit: i32,
    /// Switch sub type (0 once, 1 toggle, 2 hold).
    #[serde(default)]
    pub sub: i32,
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
    /// Interactive props placed from the OoT room actor lists (grass, pots, webs, torches, switches...).
    #[serde(default)]
    pub props: Vec<PropSpec>,
    #[serde(default)]
    pub water_boxes: Vec<[f64; 6]>,
    /// `WHIRLPOOL` level-script commands of the area (`gCurrentArea->whirlpools`).
    #[serde(default)]
    pub whirlpools: Vec<WhirlpoolData>,
    #[serde(default)]
    pub horizontal_boundary: Option<f64>,
    /// Sky gradient and distance fog of worlds that are not the default daylight (Banjo-Kazooie).
    #[serde(default)]
    pub sky: Option<Sky>,
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
    /// Course the star exit leads to when it is not the castle (`vcutm` -> the grounds, `bbh` -> the courtyard).
    #[serde(default)]
    pub success_course: Option<String>,
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
    /// SM64 surface objects (`scripts/export-sm64-solids.py`): TTC, Rainbow Ride, Bowser stages.
    #[serde(default)]
    pub solids: Vec<jumpman_sim::solids::SolidSpec>,
    /// Interactive objects the app spawns after loading (flame jets), kept out of `objects`.
    #[serde(default)]
    pub sm64_objects: Vec<CourseObject>,
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
        let value = read_course_value(root, id, area)?;
        serde_json::from_value(value).map_err(|e| format!("{}: {e}", Self::path(root, id, area).display()))
    }
}

/// One `WHIRLPOOL(index, condition, x, y, z, strength)` command (`level_cmd_create_whirlpool`).
#[derive(Clone, Debug, Deserialize)]
pub struct WhirlpoolData {
    pub index: usize,
    /// 0 always, 1 before Bowser 2 is beaten, 2 after, 3 from act 2 on.
    pub cond: u8,
    pub pos: [f64; 3],
    pub strength: f64,
}
