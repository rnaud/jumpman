//! The areas of the SM64 courses that are exported (`assets/courses/<id>.<area>.json`). Areas beyond the original export
//! (`scripts/sm64_world.py`) only count while the course extensions are on (`course_data::extensions_enabled`), so the
//! golden fixtures that replay the TypeScript sim keep their single-area behaviour.
use super::course_data::extensions_enabled;

/// Courses with a second or third area before the extensions: (course, areas).
const BASE: &[(&str, &[i32])] = &[("ddd", &[1, 2]), ("ccm", &[1, 2]), ("jrb", &[1, 2]), ("castle_inside", &[1, 2, 3])];
/// Areas added by `sm64_world.NEW_AREAS`.
const EXTENDED: &[(&str, &[i32])] = &[
    ("lll", &[1, 2]),
    ("ssl", &[1, 2, 3]),
    ("sl", &[1, 2]),
    ("thi", &[1, 2, 3]),
    ("ttm", &[1, 2, 3, 4]),
    ("wdw", &[1, 2]),
];

/// Every exported area of `course` (area 1 for single-area courses).
pub fn areas(course: &str) -> Vec<i32> {
    let ext = extensions_enabled();
    BASE.iter().chain(EXTENDED.iter().filter(|_| ext)).find(|(c, _)| *c == course).map_or(vec![1], |(_, a)| a.to_vec())
}

pub fn has_area(course: &str, area: i32) -> bool {
    areas(course).contains(&area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_areas_follow_the_extension_switch() {
        super::super::course_data::set_extensions(false);
        assert_eq!(areas("thi"), vec![1]);
        assert_eq!(areas("ddd"), vec![1, 2]);
        super::super::course_data::set_extensions(true);
        assert_eq!(areas("thi"), vec![1, 2, 3]);
        assert!(has_area("ttm", 4) && !has_area("ttm", 5) && !has_area("bob", 2));
        super::super::course_data::set_extensions(false);
    }
}
