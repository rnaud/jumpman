//! Mario Kart 64 courses as regular walkable courses (`assets/courses/mk64_<dir>.json`, exported by
//! `scripts/export-mk64.py`): the sky / fog the exporter picked, and the kart-mode course list.
use serde::Deserialize;
use std::path::Path;

/// Course-id prefix of every exported Mario Kart 64 course (`mk64_luigi_raceway`).
pub const PREFIX: &str = "mk64_";

/// The `mk64` block of a course json.
#[derive(Deserialize, Clone, Debug)]
pub struct Look {
    /// Course directory name in the decomp (`luigi_raceway`).
    pub course: String,
    pub name: String,
    /// `race` or `battle`.
    pub kind: String,
    /// World units per MK64 course unit.
    pub unit: f64,
    /// Sky dome top / middle / bottom, 0xRRGGBB.
    pub sky: [u32; 3],
    /// Fog colour 0xRRGGBB, near, far (world units).
    pub fog: (u32, f32, f32),
}

#[derive(Deserialize)]
struct Header {
    mk64: Option<Look>,
}

pub fn is_mk64(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// The look block of `courses/<id>.json`, if it is an exported Mario Kart course.
pub fn look(assets: &Path, id: &str) -> Option<Look> {
    if !is_mk64(id) {
        return None;
    }
    let bytes = std::fs::read(assets.join("courses").join(format!("{id}.json"))).ok()?;
    serde_json::from_slice::<Header>(&bytes).ok()?.mk64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::course_data::CourseData;
    use crate::game::world::{CourseRequest, LoadedCourse};
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
    use jumpman_sim::state::*;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn ids() -> Vec<String> {
        crate::game::courses::COURSES.iter().filter(|c| is_mk64(c.id)).map(|c| c.id.to_string()).collect()
    }

    #[test]
    fn all_twenty_courses_are_in_the_picker() {
        assert_eq!(ids().len(), 20);
        for id in ids() {
            let info = crate::game::courses::info(&id).unwrap();
            assert_eq!(info.series, crate::game::courses::Series::MarioKart);
        }
    }

    /// Mario dropped above the start line lands on a floor, stands, and walks every compass direction: the only
    /// things that may stop him inside 400 units (50 course units) are the course's own walls and edges.
    #[test]
    fn mario_stands_and_walks_on_every_course() {
        let mut report = String::new();
        let mut checked = 0;
        for id in ids() {
            if !CourseData::path(&assets(), &id, 1).is_file() {
                continue;
            }
            checked += 1;
            let course = LoadedCourse::load(&assets(), CourseRequest::new(&id)).unwrap_or_else(|e| panic!("{id}: {e}"));
            let spawn = course.spawn.pos;
            let floor = course.world.find_floor(spawn[0], spawn[1] + 300., spawn[2]);
            assert!(floor.surface.is_some(), "{id}: no floor under the spawn {spawn:?}");
            assert!((floor.height - spawn[1]).abs() < 200., "{id}: spawn {spawn:?} is not on the floor {}", floor.height);
            let idle = ControllerSnapshot::default();
            // Drop from 600 units up.
            let mut mario = MarioRuntime::new(
                course.world.clone(),
                SpawnPoint { pos: [spawn[0], spawn[1] + 600., spawn[2]], face_yaw: course.spawn.face_yaw, action: Some(ACT_FREEFALL) },
                course.death_height,
            );
            for _ in 0..90 {
                mario.step(&idle, 0., false, false);
            }
            assert!(mario.floor.is_some(), "{id}: did not land");
            assert!((mario.pos[1] - floor.height).abs() < 120., "{id}: stands at {} not {}", mario.pos[1], floor.height);
            assert!(mario.pos.iter().all(|v| v.is_finite()));
            let (stand_x, stand_y, stand_z) = (mario.pos[0], mario.pos[1], mario.pos[2]);
            let walk = ControllerSnapshot { stick_y: 64., stick_mag: 64., ..Default::default() };
            let mut free = 0;
            let mut line = String::new();
            for k in 0..8 {
                let mut m = MarioRuntime::new(
                    course.world.clone(),
                    SpawnPoint { pos: [stand_x, stand_y, stand_z], face_yaw: 0., action: None },
                    course.death_height,
                );
                let yaw = (k as f64 * 8192.).rem_euclid(65536.);
                let mut best = 0f64;
                let mut fell = false;
                for _ in 0..40 {
                    m.step(&walk, yaw, false, false);
                    best = best.max(((m.pos[0] - stand_x).powi(2) + (m.pos[2] - stand_z).powi(2)).sqrt());
                    if m.pos[1] < stand_y - 400. {
                        fell = true;
                    }
                    assert!(m.pos.iter().all(|v| v.is_finite()), "{id}: NaN walking");
                }
                // A real edge: he walked off the course.
                if best >= 400. || fell {
                    free += 1;
                }
                line.push_str(&format!(" {:>5.0}{}", best, if fell { "v" } else { " " }));
            }
            report.push_str(&format!("{id:<28} floor {:>7.0} free {free}/8 |{line}\n", floor.height));
            assert!(free >= 5, "{id}: only {free}/8 directions walkable from the start:\n{report}");
        }
        eprintln!("{report}");
        assert!(checked == 0 || checked == 20, "checked {checked} courses");
    }

    #[test]
    fn look_blocks_parse() {
        for id in ids() {
            if !CourseData::path(&assets(), &id, 1).is_file() {
                continue;
            }
            let l = look(&assets(), &id).unwrap_or_else(|| panic!("{id}: no mk64 block"));
            assert_eq!(l.unit, 8.);
            assert!(l.fog.1 < l.fog.2);
        }
    }
}
