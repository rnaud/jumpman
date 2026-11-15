//! Super Smash Bros. stages as walkable courses (`assets/courses/ssb64_<stage>.json` + `levels/ssb64_<stage>/stage.glb`,
//! exported from the user's ROM by `scripts/export-ssb64-stages.py` / `byor-rs/src/ssb64_stages.rs`, see
//! `native/SSB64.md` "Stages").
//!
//! The SSB64 stage is a 2D plane (x right, y up); the export extrudes its collision lines over a depth band
//! (`z` in `[-depth, depth]`) closed by two invisible walls, so any character walks the stage's real floors, walls,
//! ceilings and pass-through platforms in 3D. Falling below the bottom blast zone kills (the course's `deathHeight`), and
//! so does leaving it sideways (`horizontalBoundary` = the nearer side blast line).
//!
//! API for other modes (Gem Clash arenas, the Smash knockback layer): [`STAGES`] / [`stage_ids`] list them,
//! [`stage`] reads one stage's [`Stage`] block (blast zones, camera bounds, spawn points, BGM, pass-through
//! triangles), [`load`] loads one as a [`LoadedCourse`] by stage id (`"pupupu"`) or course id (`"ssb64_pupupu"`).
use super::world::{CourseRequest, LoadedCourse};
use serde::Deserialize;
use std::path::Path;

/// Course-id prefix of every exported Smash stage (`ssb64_pupupu`).
pub const PREFIX: &str = "ssb64_";

/// What a stage is in SSB64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The nine VS stages.
    Battle,
    /// Final Destination, Meta Crystal, Duel Zone, Race to the Finish, How to Play, the beta / 1P variants.
    Extra,
    /// Break the Targets (one per fighter).
    Targets,
    /// Board the Platforms (one per fighter).
    Platforms,
}

/// (stage id, label, kind), in the game's GRKind order (`src/gr/grdef.h`); the course id is `ssb64_<stage id>`.
pub const STAGES: &[(&str, &str, Kind)] = &[
    ("castle", "Peach's Castle", Kind::Battle),
    ("sector", "Sector Z", Kind::Battle),
    ("jungle", "Congo Jungle", Kind::Battle),
    ("zebes", "Planet Zebes", Kind::Battle),
    ("hyrule", "Hyrule Castle", Kind::Battle),
    ("yoster", "Yoshi's Island", Kind::Battle),
    ("pupupu", "Dream Land", Kind::Battle),
    ("yamabuki", "Saffron City", Kind::Battle),
    ("inishie", "Mushroom Kingdom", Kind::Battle),
    ("pupupu_small", "Dream Land (beta)", Kind::Extra),
    ("pupupu_test", "Test Stage", Kind::Extra),
    ("explain", "How to Play", Kind::Extra),
    ("yoster_small", "Mini Yoshi's Island", Kind::Extra),
    ("metal", "Meta Crystal", Kind::Extra),
    ("zako", "Duel Zone", Kind::Extra),
    ("bonus3", "Race to the Finish", Kind::Extra),
    ("last", "Final Destination", Kind::Extra),
    ("bonus1_mario", "Break the Targets: Mario", Kind::Targets),
    ("bonus1_fox", "Break the Targets: Fox", Kind::Targets),
    ("bonus1_donkey", "Break the Targets: Donkey Kong", Kind::Targets),
    ("bonus1_samus", "Break the Targets: Samus", Kind::Targets),
    ("bonus1_luigi", "Break the Targets: Luigi", Kind::Targets),
    ("bonus1_link", "Break the Targets: Link", Kind::Targets),
    ("bonus1_yoshi", "Break the Targets: Yoshi", Kind::Targets),
    ("bonus1_captain", "Break the Targets: Captain Falcon", Kind::Targets),
    ("bonus1_kirby", "Break the Targets: Kirby", Kind::Targets),
    ("bonus1_pikachu", "Break the Targets: Pikachu", Kind::Targets),
    ("bonus1_purin", "Break the Targets: Jigglypuff", Kind::Targets),
    ("bonus1_ness", "Break the Targets: Ness", Kind::Targets),
    ("bonus2_mario", "Board the Platforms: Mario", Kind::Platforms),
    ("bonus2_fox", "Board the Platforms: Fox", Kind::Platforms),
    ("bonus2_donkey", "Board the Platforms: Donkey Kong", Kind::Platforms),
    ("bonus2_samus", "Board the Platforms: Samus", Kind::Platforms),
    ("bonus2_luigi", "Board the Platforms: Luigi", Kind::Platforms),
    ("bonus2_link", "Board the Platforms: Link", Kind::Platforms),
    ("bonus2_yoshi", "Board the Platforms: Yoshi", Kind::Platforms),
    ("bonus2_captain", "Board the Platforms: Captain Falcon", Kind::Platforms),
    ("bonus2_kirby", "Board the Platforms: Kirby", Kind::Platforms),
    ("bonus2_pikachu", "Board the Platforms: Pikachu", Kind::Platforms),
    ("bonus2_purin", "Board the Platforms: Jigglypuff", Kind::Platforms),
    ("bonus2_ness", "Board the Platforms: Ness", Kind::Platforms),
];

/// Top / bottom / left / right lines in SM64 units (x and y; the stage plane is z = 0).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

impl Bounds {
    /// Whether a point (SM64 units) is inside (x and y only).
    pub fn contains(&self, p: [f64; 3]) -> bool {
        p[0] >= self.left && p[0] <= self.right && p[1] >= self.bottom && p[1] <= self.top
    }
}

/// One collision group (yakumono) of the stage: `animated` ones move with a layer-1 DObj in the game (exported at
/// their frame-0 place).
#[derive(Deserialize, Clone, Debug)]
pub struct Yakumono {
    pub id: u32,
    pub animated: bool,
    pub offset: [f64; 2],
}

/// The `ssb64` block of a stage's course json.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    /// Stage id (`pupupu`).
    pub stage: String,
    pub name: String,
    /// `battle`, `extra`, `bonus1`, `bonus2`.
    pub kind: String,
    /// SM64 units per SSB64 unit.
    pub unit: f64,
    /// Half depth of the collision band (SM64 units).
    pub depth: f64,
    /// BGM id = sequence index of `audio/music/ssb64` (`ssb64_music`).
    pub bgm: u32,
    /// The SSB64 camera's limits.
    pub camera: Bounds,
    /// Blast zones (`map_bound_*`): leaving them is a KO in SSB64.
    pub blast: Bounds,
    /// Height of the low-altitude warning whistle.
    pub warning: f64,
    /// Fog / background colour (sRGB bytes).
    pub fog: [u8; 3],
    /// Player starts (`nMPMapObjKindBattlePlayer1..4`), item spawns, the respawn point (`Rebirth`), SM64 units.
    pub players: Vec<[f64; 3]>,
    pub items: Vec<[f64; 3]>,
    pub rebirth: Vec<[f64; 3]>,
    /// Course triangles that are pass-through floors (`MAP_VERTEX_COLL_PASS`) and ledges (`MAP_VERTEX_COLL_CLIFF`).
    pub pass_triangles: Vec<u32>,
    pub ledge_triangles: Vec<u32>,
    pub yakumono: Vec<Yakumono>,
}

#[derive(Deserialize)]
struct Header {
    ssb64: Option<Stage>,
}

pub fn is_stage(course: &str) -> bool {
    course.starts_with(PREFIX)
}

/// `pupupu` or `ssb64_pupupu` -> `ssb64_pupupu`.
pub fn course_id(stage: &str) -> String {
    if is_stage(stage) {
        stage.to_string()
    } else {
        format!("{PREFIX}{stage}")
    }
}

/// Every stage id, in the game's order.
pub fn stage_ids() -> impl Iterator<Item = &'static str> {
    STAGES.iter().map(|s| s.0)
}

/// The nine VS stages (the Gem Clash arena candidates).
pub fn battle_stages() -> impl Iterator<Item = &'static str> {
    STAGES.iter().filter(|s| s.2 == Kind::Battle).map(|s| s.0)
}

pub fn kind(course: &str) -> Option<Kind> {
    let id = course.strip_prefix(PREFIX)?;
    STAGES.iter().find(|s| s.0 == id).map(|s| s.2)
}

/// Whether a stage was imported (its course json is in the asset root).
pub fn installed(assets: &Path, stage: &str) -> bool {
    assets.join("courses").join(format!("{}.json", course_id(stage))).is_file()
}

/// The stage block of `courses/ssb64_<stage>.json` (`stage` = `pupupu` or `ssb64_pupupu`).
pub fn stage(assets: &Path, stage: &str) -> Option<Stage> {
    let id = course_id(stage);
    let bytes = std::fs::read(assets.join("courses").join(format!("{id}.json"))).ok()?;
    serde_json::from_slice::<Header>(&bytes).ok()?.ssb64
}

/// Load a stage as a course (`stage` = `pupupu` or `ssb64_pupupu`): collision, spawn, death height, model.
pub fn load(assets: &Path, stage: &str) -> Result<LoadedCourse, String> {
    LoadedCourse::load(assets, CourseRequest::new(&course_id(stage)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::course_data::CourseData;
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
    use jumpman_sim::state::*;
    use std::path::PathBuf;

    /// Stage data: `JUMPMAN_SMASH_TEST_ASSETS` (an import's asset folder) or the repo's own (git-ignored) assets.
    fn assets() -> PathBuf {
        std::env::var_os("JUMPMAN_SMASH_TEST_ASSETS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"))
    }

    #[test]
    fn catalogue_matches_the_course_list() {
        assert_eq!(STAGES.len(), 41);
        assert_eq!(battle_stages().count(), 9);
        for (id, name, _) in STAGES {
            let info = crate::game::courses::info(&course_id(id)).unwrap_or_else(|| panic!("{id} not in COURSES"));
            assert_eq!(info.series, crate::game::courses::Series::Smash);
            assert_eq!(info.name, *name);
        }
        assert_eq!(course_id("pupupu"), "ssb64_pupupu");
        assert_eq!(course_id("ssb64_pupupu"), "ssb64_pupupu");
        assert_eq!(kind("ssb64_bonus1_ness"), Some(Kind::Targets));
        assert_eq!(kind("bob"), None);
    }

    /// Every imported stage: the block parses, the spawn stands on a floor inside the blast zones, and a character dropped
    /// there lands, stands, and can walk left and right (the stage plane) without leaving the depth band.
    #[test]
    fn characters_land_and_walk_on_every_stage() {
        let root = assets();
        let mut checked = 0;
        let mut report = String::new();
        for id in stage_ids() {
            let course = course_id(id);
            if !CourseData::path(&root, &course, 1).is_file() {
                continue;
            }
            checked += 1;
            let info = stage(&root, id).unwrap_or_else(|| panic!("{id}: no ssb64 block"));
            assert!(info.blast.left < info.blast.right && info.blast.bottom < info.blast.top, "{id}: blast {:?}", info.blast);
            let c = load(&root, id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(c.death_height, info.blast.bottom, "{id}");
            let spawn = c.spawn.pos;
            assert!(info.blast.contains(spawn), "{id}: spawn {spawn:?} outside {:?}", info.blast);
            let floor = c.world.find_floor(spawn[0], spawn[1] + 100., spawn[2]);
            assert!(floor.surface.is_some(), "{id}: no floor under the spawn {spawn:?}");
            let idle = ControllerSnapshot::default();
            let mut m = MarioRuntime::new(c.world.clone(), SpawnPoint { pos: [spawn[0], spawn[1] + 100., spawn[2]], face_yaw: 0., action: Some(ACT_FREEFALL) }, c.death_height);
            for _ in 0..90 {
                m.step(&idle, 0., false, false);
            }
            assert!(m.floor.is_some(), "{id}: did not land");
            assert!((m.pos[1] - floor.height).abs() < 60., "{id}: stands at {} not {}", m.pos[1], floor.height);
            let walk = ControllerSnapshot { stick_y: 64., stick_mag: 64., ..Default::default() };
            let mut line = format!("{id:<16} floor {:>6.0}", floor.height);
            let mut along = 0f64;
            for yaw in [0x4000 as f64, 0xC000 as f64, 0.0, 0x8000 as f64] {
                let mut w = MarioRuntime::new(c.world.clone(), SpawnPoint { pos: m.pos, face_yaw: yaw, action: None }, c.death_height);
                let mut best = 0f64;
                for _ in 0..30 {
                    w.step(&walk, yaw, false, false);
                    best = best.max((w.pos[0] - m.pos[0]).abs());
                    assert!(w.pos.iter().all(|v| v.is_finite()), "{id}: NaN");
                    assert!(w.pos[2].abs() <= info.depth + 1.0, "{id}: left the depth band at {:?}", w.pos);
                }
                line.push_str(&format!(" {best:>5.0}"));
                if yaw == 0x4000 as f64 || yaw == 0xC000 as f64 {
                    along = along.max(best);
                }
            }
            assert!(along > 100., "{id}: cannot walk along the stage from the spawn ({along})");
            report.push_str(&line);
            report.push('\n');
        }
        eprintln!("{report}");
        assert!(checked == 0 || checked == STAGES.len(), "checked {checked} stages: a partial import");
    }

    /// The moving platforms (layer-1 yakumono with an animation) are surface objects: they follow their track, their
    /// collision moves with them, and a character standing on one is carried along.
    #[test]
    fn moving_platforms_carry_a_rider() {
        use jumpman_sim::solids::{SolidMesh, Solids};
        use std::collections::HashMap;
        let root = assets();
        let mut checked = 0;
        for id in ["jungle", "yamabuki", "castle", "zebes"] {
            if !installed(&root, id) {
                continue;
            }
            let c = load(&root, id).unwrap();
            let specs = c.extras.solids.clone();
            assert!(!specs.is_empty(), "{id}: no moving platform");
            let meshes: HashMap<String, SolidMesh> = c.extras.extra_collision.iter().map(|(k, v)| (k.clone(), SolidMesh { vertices: v.vertices.clone(), triangles: v.triangles.clone() })).collect();
            let mut solids = Solids::new(&specs, &meshes, 0);
            assert_eq!(solids.unported(), 0, "{id}");
            // stand on the middle of the first platform's top, at its frame-0 place
            let track = &specs[0].track;
            let mesh = &meshes[specs[0].collision.as_ref().unwrap()];
            let (lo, hi, top_y) = mesh.vertices.iter().fold((f64::MAX, f64::MIN, f64::MIN), |(lo, hi, t), v| (lo.min(v[0]), hi.max(v[0]), t.max(v[1])));
            let start = [track[0][1] + (lo + hi) / 2., track[0][2], 0.0];
            let mut m = MarioRuntime::new(c.world.clone(), SpawnPoint { pos: [start[0], start[1] + top_y + 10., 0.], face_yaw: 0., action: None }, c.death_height);
            solids.install(&mut m);
            let top = m.world.find_floor(start[0], start[1] + top_y + 10., 0.);
            assert!(top.surface.as_ref().is_some_and(|s| s.object == Some(solids.list[0].tag)), "{id}: no platform floor at {start:?} + {top_y}");
            m.pos = [start[0], top.height, 0.];
            let idle = ControllerSnapshot::default();
            let mut moved = 0f64;
            let mut carried = 0;
            for _ in 0..150 {
                let before = solids.list[0].o.pos;
                solids.step(&mut m);
                m.step(&idle, 0., false, false);
                let after = solids.list[0].o.pos;
                moved = moved.max(((after[0] - start[0]).powi(2) + (after[1] - start[1]).powi(2)).sqrt());
                let on = m.floor.as_ref().is_some_and(|f| f.object == Some(solids.list[0].tag));
                if on && (after[0] - before[0]).abs() + (after[1] - before[1]).abs() > 0.5 {
                    carried += 1;
                }
            }
            assert!(moved > 50., "{id}: the platform did not move ({moved})");
            assert!(carried > 30, "{id}: Mario rode the moving platform for only {carried} ticks");
            checked += 1;
        }
        eprintln!("moving platforms checked on {checked} stages");
    }
}
