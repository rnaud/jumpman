//! Swaps the faithful SM64 ports into a freshly loaded course without touching the exported
//! `objects` list (the TypeScript-generated goldens compare that list; the Bevy layer calls this from
//! `objects::solids`): the Bowser arenas use
//! `sm64Bowser` / `sm64BowserBomb` instead of the older simplified `bowser` / `bowserMine`, and the
//! `sm64Objects` of the course JSON (flame jets) are spawned.
use super::course_data::CourseObject;
use super::course_extras::CourseExtras;
use jumpman_sim::math::deg_to_angle;
use jumpman_sim::object::{ObjectBehaviorKind as K, ObjectSpec};
use jumpman_sim::object_manager::ObjectManager;

/// Apply to the object manager of a just-loaded course (`course_id` is the exported id).
pub fn apply(objects: &mut ObjectManager, course_id: &str, extras: &CourseExtras) {
    let arena = course_id.strip_prefix("bowser_").and_then(|n| n.parse::<i32>().ok());
    if let Some(n) = arena {
        objects.retarget(&|spec| match spec.behavior {
            // `param` of the exported Bowser is the arena number (1..3); the port wants 0..2
            K::Bowser => Some((K::SmBowser, Some((n - 1) as f64))),
            K::BowserMine => Some((K::SmBowserBomb, None)),
            _ => None,
        });
    }
    let specs: Vec<ObjectSpec> = extras.sm64_objects.iter().filter_map(spec_of).collect();
    // idempotent: the course may be rebuilt (`CourseRebuild`) without being reloaded
    if !specs.is_empty() && !objects.objects.iter().any(|o| o.behavior == K::SmFlamethrower) {
        objects.add_specs(specs);
    }
}

fn spec_of(o: &CourseObject) -> Option<ObjectSpec> {
    let kind = K::from_name(o.behavior.as_deref()?)?;
    let mut s = ObjectSpec::new(kind, o.pos, deg_to_angle(o.yaw) as f64);
    s.param = Some(o.param.unwrap_or(0.0));
    s.model = if o.model.is_empty() { None } else { Some(o.model.clone()) };
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::{CourseRequest, LoadedCourse};
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn bowser_arenas_get_the_faithful_kinds_and_flame_jets_appear() {
        for (id, arena) in [("bowser_1", 0.0), ("bowser_2", 1.0), ("bowser_3", 2.0)] {
            let mut c = LoadedCourse::load(&assets(), CourseRequest::new(id)).unwrap();
            // the exported list is untouched
            assert!(c.objects.objects.iter().any(|o| o.behavior == K::Bowser), "{id}: exported kind");
            apply(&mut c.objects, id, &c.extras);
            assert!(!c.objects.objects.iter().any(|o| matches!(o.behavior, K::Bowser | K::BowserMine)), "{id}: old kinds remain");
            let bowser = c.objects.objects.iter().find(|o| o.behavior == K::SmBowser).unwrap();
            assert_eq!(bowser.param, arena);
            assert!(c.objects.objects.iter().any(|o| o.behavior == K::SmBowserBomb));
        }
        let mut c = LoadedCourse::load(&assets(), CourseRequest::new("bitfs")).unwrap();
        let before = c.objects.len();
        apply(&mut c.objects, "bitfs", &c.extras);
        assert_eq!(c.objects.len(), before + 3, "three flame jets in BitFS");
        assert_eq!(c.objects.objects.iter().filter(|o| o.behavior == K::SmFlamethrower).count(), 3);
    }
}
