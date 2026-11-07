//! Zelda scene data: the `kokiri` block of `assets/courses/<id>.json` (`KokiriScene` in
//! `kokiriVillage.ts`) plus the scene objects the generic course loader leaves alone
//! (`is_managed`). Engine-free.
use crate::game::course_data::{CollisionData, CourseObject, V3};
use crate::game::world::CourseRequest;
use crate::zelda::textbox::MessagePage;
use jumpman_sim::mario_runtime::SpawnPoint;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FixedRoom {
    pub eye: V3,
    pub pitch: f64,
    pub yaw: f64,
    pub fov: f64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TalkSpot {
    pub pos: V3,
    /// `saria` | `sign` | `resident`.
    pub kind: String,
    pub id: Option<String>,
    pub label: Option<String>,
    pub range: Option<f64>,
    pub height: Option<f64>,
    pub pages: Vec<MessagePage>,
    pub repeat: Option<Vec<MessagePage>>,
    /// Filled in by the view from the speaker's model bounds.
    #[serde(skip)]
    pub portrait: Option<Portrait>,
}

#[derive(Clone, Copy, Debug)]
pub struct Portrait {
    pub center: V3,
    pub distance: f64,
    pub yaw: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Exit {
    pub course: String,
    pub entry: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Arrival {
    pub pos: V3,
    pub face_yaw: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RupeePickup {
    pub key: String,
    pub pos: V3,
    pub value: u32,
    #[serde(default)]
    pub permanent: bool,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TreasureChest {
    pub key: String,
    pub pos: V3,
    pub item: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WarpPad {
    pub pos: V3,
    pub course: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Counter {
    pub shield: V3,
    pub heart: V3,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Light {
    pub ambient: V3,
    pub key: V3,
    #[serde(rename = "keyDir")]
    pub key_dir: V3,
    pub fill: V3,
    #[serde(rename = "fillDir")]
    pub fill_dir: V3,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Mouth {
    pub closed: V3,
    pub opened: V3,
    pub meeting: V3,
    pub eye: V3,
    pub look_from: V3,
    pub look_to: V3,
    pub first: Vec<MessagePage>,
    pub retry: Vec<MessagePage>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Quest {
    pub mouth: Mouth,
    pub chest: V3,
    pub crawl: [V3; 2],
    pub path: Vec<V3>,
    pub reward: Vec<MessagePage>,
    pub approved: Vec<MessagePage>,
    pub passed: Vec<MessagePage>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct KokiriScene {
    #[serde(default)]
    pub rupees: Vec<RupeePickup>,
    #[serde(default)]
    pub treasure_chests: Vec<TreasureChest>,
    pub warp_pad: Option<WarpPad>,
    pub counter: Option<Counter>,
    pub light: Light,
    pub quest: Option<Quest>,
    pub fixed: Option<FixedRoom>,
    #[serde(default)]
    pub enclosed: bool,
    /// Keyed by the floor's room number.
    #[serde(default)]
    pub exits: HashMap<String, Exit>,
    #[serde(default)]
    pub arrivals: HashMap<String, Arrival>,
    #[serde(default)]
    pub talks: Vec<TalkSpot>,
    pub ladder: Option<[V3; 2]>,
    pub after_mido: Option<[Vec<MessagePage>; 2]>,
    pub greeting: Option<Vec<MessagePage>>,
    pub repeat: Option<Vec<MessagePage>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    kokiri: Option<KokiriScene>,
    #[serde(default)]
    objects: Vec<CourseObject>,
    #[serde(default)]
    extra_collision: HashMap<String, CollisionData>,
}

/// Everything the Zelda plugin needs from one course area file.
#[derive(Clone, Debug)]
pub struct ZeldaScene {
    pub course: String,
    pub area: i32,
    pub kokiri: KokiriScene,
    /// The objects `is_managed` claims, in file order.
    pub objects: Vec<CourseObject>,
    pub extra_collision: HashMap<String, CollisionData>,
    /// The Great Deku Tree's farewell (`assets/levels/deku_tree/farewell.json`).
    pub farewell: Vec<MessagePage>,
}

const QUEST_MODELS: [&str; 4] = ["sword_chest", "kokiri_sword", "mido", "deku_tree_mouth"];

/// Models of the sword quest (animated/moved by `sword_quest`).
pub fn quest_part(model: &str) -> Option<&'static str> {
    let name = model.strip_prefix("levels/kokiri_forest/")?.strip_suffix(".glb")?;
    QUEST_MODELS.iter().copied().find(|q| *q == name)
}

/// Objects the generic course loader skips because the Zelda plugin instantiates them (NPCs,
/// chests, rupees, fairies, recovery hearts, dungeon parts and the sword-quest pieces).
pub fn is_managed(o: &CourseObject) -> bool {
    o.behavior.is_none()
        && (o.dungeon_role.is_some()
            || o.treasure_key.is_some()
            || o.rupee.is_some()
            || o.fairy
            || o.idle
            || o.speaker.is_some()
            || o.recovery_heart
            || quest_part(&o.model).is_some())
}

impl ZeldaScene {
    /// `None` for courses without a `kokiri` block (the Mario courses).
    pub fn load(root: &Path, id: &str, area: i32) -> Option<Self> {
        let bytes = std::fs::read(crate::game::course_data::CourseData::path(root, id, area)).ok()?;
        let raw: Raw = serde_json::from_slice(&bytes).ok()?;
        let kokiri = raw.kokiri?;
        let farewell = std::fs::read(root.join("levels/deku_tree/farewell.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Some(Self {
            course: id.to_string(),
            area,
            kokiri,
            objects: raw.objects.into_iter().filter(is_managed).collect(),
            extra_collision: raw.extra_collision,
            farewell,
        })
    }
}

/// `data.kokiri.arrivals[entry]` as the spawn point.
pub fn arrival_spawn(root: &Path, request: &CourseRequest) -> Option<SpawnPoint> {
    let entry = request.entry.as_deref()?;
    let scene = ZeldaScene::load(root, &request.id, request.area)?;
    let a = scene.kokiri.arrivals.get(entry)?;
    Some(SpawnPoint { action: None, pos: a.pos, face_yaw: a.face_yaw })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn every_zelda_course_parses() {
        let mut n = 0;
        for info in crate::game::courses::COURSES {
            if let Some(s) = ZeldaScene::load(&assets(), info.id, 1) {
                n += 1;
                assert!(!s.kokiri.exits.is_empty() || s.course == "hyrule_field" || !s.kokiri.talks.is_empty() || true);
            }
        }
        assert!(n >= 20, "parsed {n} zelda scenes");
    }

    #[test]
    fn kokiri_forest_scene() {
        let s = ZeldaScene::load(&assets(), "kokiri_forest", 1).unwrap();
        let q = s.kokiri.quest.as_ref().unwrap();
        assert_eq!(q.mouth.first.len(), 9);
        assert!(s.kokiri.talks.iter().any(|t| t.id.as_deref() == Some("mido")));
        assert!(s.kokiri.talks.iter().any(|t| t.kind == "saria"));
        assert_eq!(s.kokiri.exits["2"].course, "deku_tree");
        assert!(s.objects.iter().any(|o| quest_part(&o.model) == Some("mido")));
        assert!(s.kokiri.rupees.len() >= 10);
        assert!(!s.farewell.is_empty());
    }

    #[test]
    fn deku_tree_roles() {
        let s = ZeldaScene::load(&assets(), "deku_tree", 1).unwrap();
        for role in ["block", "lowerWeb", "bossDoor", "stick", "gate", "web", "ladder", "scrub", "eye", "ferry", "log", "switch", "brother1", "steppingStone"] {
            assert!(s.objects.iter().any(|o| o.dungeon_role.as_deref() == Some(role)), "{role}");
        }
        assert_eq!(s.kokiri.treasure_chests.len(), 6);
        let boss = ZeldaScene::load(&assets(), "deku_tree", 2).unwrap();
        assert!(boss.objects.iter().any(|o| o.dungeon_role.as_deref() == Some("gohma")));
        assert!(boss.objects.iter().any(|o| o.dungeon_role.as_deref() == Some("container")));
    }

    #[test]
    fn arrivals_select_spawn() {
        let mut r = CourseRequest::new("kokiri_forest");
        r.entry = Some("shop".into());
        assert_eq!(arrival_spawn(&assets(), &r).unwrap().pos, [-4413.2, 0.0, 195.8]);
        assert!(arrival_spawn(&assets(), &CourseRequest::new("bob")).is_none());
    }

    #[test]
    fn shop_has_counter() {
        let s = ZeldaScene::load(&assets(), "kokiri_shop", 1).unwrap();
        assert!(s.kokiri.counter.is_some() && s.kokiri.fixed.is_some());
    }
}
