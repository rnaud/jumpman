//! The progression-related slice of `assets/courses/*.json` that `game::course_data` does not
//! model (castle doors' keys, room doors, painting tables, star placement, wing switch ...).
//! Loaded independently from the same file so the shared loader stays untouched.
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub type V3 = [f64; 3];

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CastleDoor {
    pub pos: V3,
    pub yaw: f64,
    pub node: String,
    #[serde(default)]
    pub dest_node: String,
    #[serde(default = "one")]
    pub dest_area: i32,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub required_key: Option<i32>,
}

fn one() -> i32 {
    1
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoomDoorSpec {
    pub pos: V3,
    pub yaw: f64,
    #[serde(default)]
    pub stars: u32,
    #[serde(default)]
    pub sliding: bool,
    #[serde(default)]
    pub model: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct DoorMotion {
    #[serde(default)]
    pub push: Vec<[f64; 2]>,
    #[serde(default)]
    pub pull: Vec<[f64; 2]>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InstantWarp {
    pub area: i32,
    pub delta: V3,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SecretStar {
    pub act: i32,
    pub pos: V3,
    #[serde(default)]
    pub triggers: Vec<V3>,
    #[serde(default)]
    pub origin: Option<V3>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ActStar {
    pub act: i32,
    pub pos: V3,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlacedStar {
    pub act: i32,
    pub pos: V3,
    #[serde(default)]
    pub acts: Vec<i32>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FloorWarp {
    pub area: i32,
    pub entry: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProgressData {
    #[serde(default)]
    pub doors: Vec<CastleDoor>,
    #[serde(default)]
    pub room_doors: Vec<RoomDoorSpec>,
    #[serde(default)]
    pub door_motion: Option<DoorMotion>,
    /// Floor-type index (type - 0xd3) -> course id.
    #[serde(default)]
    pub paintings: HashMap<String, String>,
    #[serde(default)]
    pub instant_warps: HashMap<String, InstantWarp>,
    #[serde(default)]
    pub floor_warp: Option<FloorWarp>,
    #[serde(default)]
    pub wing_switch: Option<V3>,
    /// Which cap the switch at `wing_switch` unlocks: 0 wing (Tower of the Wing Cap), 1 metal (Cavern of the Metal
    /// Cap), 2 vanish (Vanish Cap under the Moat).
    #[serde(default)]
    pub cap_switch_kind: i32,
    /// Destination course -> stars Mario needs before a warp into it works (the courtyard's BBH cage: 12).
    #[serde(default)]
    pub warp_gates: HashMap<String, u32>,
    #[serde(default)]
    pub secret_star: Option<SecretStar>,
    #[serde(default)]
    pub red_coin_star: Option<V3>,
    #[serde(default)]
    pub red_coin_act: Option<i32>,
    #[serde(default)]
    pub snowman_star: Option<ActStar>,
    #[serde(default)]
    pub penguin_star: Option<ActStar>,
    #[serde(default)]
    pub chest_star: Option<ActStar>,
    #[serde(default)]
    pub placed_stars: Vec<PlacedStar>,
    #[serde(default)]
    pub exploration_only: bool,
    /// Area of the course this data belongs to (set by [`ProgressData::load`]; 0 for hand-built data).
    #[serde(skip)]
    pub area: i32,
    /// Painting layout indices visible in each room.
    #[serde(default)]
    pub room_paintings: Vec<Vec<i32>>,
    #[serde(default)]
    pub extra_collision: HashMap<String, crate::game::course_data::CollisionData>,
}

impl ProgressData {
    pub fn load(root: &Path, course: &str, area: i32) -> Self {
        let mut data: Self = crate::game::course_data::read_course_value(root, course, area).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        data.area = area;
        data
    }

    /// `paintings` keyed by integer floor index.
    pub fn painting_table(&self) -> HashMap<i32, String> {
        self.paintings.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.clone()))).collect()
    }
}
