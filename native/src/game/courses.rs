//! Port of `src/game/courses.ts`: the course catalogue and mission names.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Series {
    Mario,
    Zelda,
    /// Banjo-Kazooie worlds exported by `scripts/export-bk-levels.py`.
    Banjo,
}

#[derive(Clone, Copy, Debug)]
pub struct CourseInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub series: Series,
}

include!("courses_table.rs");

pub fn info(id: &str) -> Option<&'static CourseInfo> {
    COURSES.iter().find(|c| c.id == id)
}

/// `courseId`: unknown or missing ids fall back to the castle grounds.
pub fn course_id(value: Option<&str>) -> &'static str {
    value.and_then(info).map_or("castle_grounds", |c| c.id)
}

#[derive(Clone, Copy, Debug)]
pub struct Mission {
    pub act: i32,
    pub name: &'static str,
}

/// `missionsFor`: the course's acts, or a single act named after the course.
pub fn missions_for(id: &str) -> Vec<Mission> {
    if let Some((_, list)) = MISSIONS.iter().find(|(c, _)| *c == id) {
        return list.iter().map(|&(act, name)| Mission { act, name }).collect();
    }
    vec![Mission { act: 1, name: info(id).map_or("", |c| c.name) }]
}

/// Courses that count toward the score table (`SCORE_COURSES`).
pub const SCORE_COURSES: &[&str] = &[
    "bob", "wf", "jrb", "ccm", "bbh", "hmc", "lll", "ssl", "ddd", "sl", "wdw", "ttm", "thi", "ttc", "rr",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_resolves() {
        assert_eq!(course_id(Some("bob")), "bob");
        assert_eq!(course_id(Some("nope")), "castle_grounds");
        assert_eq!(missions_for("bob").len(), 6);
        assert_eq!(missions_for("castle_grounds")[0].name, "Castle Grounds");
        assert_eq!(info("kokiri_forest").unwrap().series, Series::Zelda);
    }
}
