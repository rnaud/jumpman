//! The OoT hookshot surface marks (`SurfaceType_CanHookshot`) parsed by `scripts/export-oot-courses.py`.
use super::course_data::CourseData;
use super::native_collision::add_native_collision;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::surfaces::SURFACE_FLAG_HOOKABLE;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// Per-course counts of marked triangles (bit 17 of surface type word 1), all inside the triangle list and carried onto the world's surfaces.
#[test]
fn exported_oot_courses_carry_hookshot_marks_that_reach_the_collision_world() {
    let expected = [("deku_tree", 61), ("kokiri_forest", 110), ("hyrule_field", 4), ("goron_city", 156), ("zoras_fountain", 101), ("lon_lon_ranch", 0), ("links_house", 0)];
    let mut total = 0;
    for (id, count) in expected {
        let course = CourseData::load(&root(), id, 1).unwrap();
        let marks = course.collision.hookable.clone().unwrap_or_else(|| panic!("{id}: no hookable list"));
        assert_eq!(marks.len(), count, "{id}");
        assert!(marks.windows(2).all(|w| w[0] < w[1]) && marks.iter().all(|&i| (i as usize) < course.collision.triangles.len()), "{id}");
        let mut world = CollisionWorld::default();
        add_native_collision(&mut world, &course.collision, [0.0; 3], 0.0);
        assert!(world.hook_marked, "{id}");
        let flagged = world.surfaces().iter().filter(|s| s.flags & SURFACE_FLAG_HOOKABLE != 0).count();
        assert!(flagged <= count && (count == 0 || flagged > 0), "{id}: {flagged} of {count} reached the world");
        total += count;
    }
    assert!(total > 400);
}

#[test]
fn sm64_courses_have_no_marks_so_the_old_rule_applies() {
    let course = CourseData::load(&root(), "bob", 1).unwrap();
    assert!(course.collision.hookable.is_none());
    let mut world = CollisionWorld::default();
    add_native_collision(&mut world, &course.collision, [0.0; 3], 0.0);
    assert!(!world.hook_marked);
}
