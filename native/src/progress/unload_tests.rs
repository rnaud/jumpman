//! The unload that precedes every course build (see `despawn_course_entities`).
use super::*;

/// No course entity (children included) is left after the unload, and everything else stays, resources (entities in
/// Bevy 0.20) among them: at most one course lives in the world.
#[test]
fn unloading_a_course_leaves_no_course_entity_and_spares_the_rest() {
    let mut world = World::new();
    world.insert_resource(ObjectViews::default());
    let hud = world.spawn_empty().id();
    let course_a = world.spawn(CourseScoped).id();
    let child = world.spawn((CourseScoped, ChildOf(course_a))).id();
    let plain_child = world.spawn(ChildOf(course_a)).id();
    let count = |w: &mut World| w.query_filtered::<(), (With<CourseScoped>, Without<bevy::ecs::resource::IsResource>)>().iter(w).count();
    assert_eq!(count(&mut world), 2);
    despawn_course_entities(&mut world);
    assert_eq!(count(&mut world), 0);
    assert!(world.get_entity(hud).is_ok(), "non-course entities stay");
    assert!(world.get_entity(course_a).is_err() && world.get_entity(child).is_err());
    assert!(world.get_entity(plain_child).is_err(), "children go with their parent");
    assert!(world.get_resource::<ObjectViews>().is_some(), "resource entities are never despawned");
}
