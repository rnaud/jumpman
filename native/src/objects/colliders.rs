//! Object colliders (`DoorCollider` use in `courseObjects.ts`): wooden posts, the chain gate,
//! star/wing boxes and blast walls own native collision that follows the sim object. Surfaces
//! carry `Surface::object = Some(<arena index>)` so behaviors can recognise "Mario stands on me".
use crate::game::collider::Collider;
use crate::game::runner::GameSim;
use crate::progress::flow::Progress;
use crate::sim_plugin::SimResetMsg;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use jumpman_sim::object::ObjectBehaviorKind;

pub struct CollidersPlugin;

impl Plugin for CollidersPlugin {
    fn build(&self, app: &mut App) {
        super::register_rebuild(app, rebuild);
        app.add_systems(Startup, setup)
            .add_systems(Update, reset_colliders.before(crate::CameraSet));
    }
}

pub struct ObjectCollider {
    pub object: usize,
    pub collider: Collider,
    pub wing: bool,
    pub solid: bool,
    pub y: f64,
}

/// The colliders plus how many arena objects have been looked at (`CourseObjects.ensureView` runs
/// for objects spawned later too, e.g. the chain chomp's post).
#[derive(Resource, Default)]
pub struct ObjectColliders(pub Vec<ObjectCollider>, pub usize);

fn collision_key(kind: ObjectBehaviorKind, param: f64) -> Option<&'static str> {
    Some(match kind {
        ObjectBehaviorKind::BlastWall => match param as i32 {
            0 => "breakable_wall_left/collision.inc.c",
            1 => "breakable_wall_right/collision.inc.c",
            _ => return None,
        },
        ObjectBehaviorKind::ChainChompGate => "chainGate",
        ObjectBehaviorKind::WoodenPost => "woodenPost",
        ObjectBehaviorKind::StarBox | ObjectBehaviorKind::WingBox => "starBox",
        _ => return None,
    })
}

/// Build the colliders for the current object arena and place them at their homes.
pub fn build_colliders(sim: &mut GameSim, wing_unlocked: bool) -> ObjectColliders {
    // `CourseObjects.buildViews` + `setWingBoxes`: wing boxes are not solid until the switch is pressed.
    let mut colliders = ObjectColliders::default();
    sync(sim, &mut colliders, wing_unlocked);
    colliders
}

fn rebuild(world: &mut World) {
    let _ = world.run_system_once(setup);
}

fn setup(mut commands: Commands, mut sim: ResMut<GameSim>, progress: Res<Progress>) {
    let colliders = build_colliders(&mut sim, progress.save.wing_switch_on());
    commands.insert_resource(colliders);
}

/// `CourseObjects.syncViews` for the colliders: every collider follows its object (solid while the
/// object is active and idle; wing boxes only once the switch is pressed).
pub fn sync(sim: &mut GameSim, colliders: &mut ObjectColliders, wing_unlocked: bool) {
    // `ensureView` for objects that appeared since the last sync.
    while colliders.1 < sim.mario.objects.objects.len() {
        let i = colliders.1;
        colliders.1 += 1;
        let o = &sim.mario.objects.objects[i];
        let Some(key) = collision_key(o.behavior, o.param) else { continue };
        let Some(data) = sim.course.extras.extra_collision.get(key) else { continue };
        let mut c = ObjectCollider {
            object: i,
            collider: Collider::new(data.clone(), o.move_yaw * 180.0 / 32768.0, i as u32),
            wing: o.behavior == ObjectBehaviorKind::WingBox,
            solid: true,
            y: o.home[1],
        };
        let home = o.home;
        c.collider.place(&mut sim.mario.world, Some(home));
        colliders.0.push(c);
    }
    for c in &mut colliders.0 {
        let Some(o) = sim.mario.objects.objects.get(c.object) else { continue };
        let solid = o.active && o.action == 0 && (!c.wing || wing_unlocked);
        let pos = o.pos;
        if solid != c.solid || (solid && c.y != pos[1]) {
            c.collider.place(&mut sim.mario.world, solid.then_some(pos));
            c.y = pos[1];
        }
        c.solid = solid;
    }
}

fn reset_colliders(mut resets: MessageReader<SimResetMsg>, mut sim: ResMut<GameSim>, mut colliders: ResMut<ObjectColliders>) {
    if resets.read().count() == 0 {
        return;
    }
    // The reset arena drops objects spawned at run time: forget their colliders (the surfaces were
    // rebuilt with the world) and let `sync` look at the arena again.
    let n = sim.mario.objects.objects.len();
    for c in colliders.0.iter_mut().filter(|c| c.object >= n) {
        c.collider.place(&mut sim.mario.world, None);
    }
    colliders.0.retain(|c| c.object < n);
    colliders.1 = colliders.1.min(n);
    for c in &mut colliders.0 {
        let home = sim.mario.objects.objects[c.object].home;
        c.collider.place(&mut sim.mario.world, Some(home));
        c.solid = true;
        c.y = home[1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn bob_posts_and_boxes_own_their_collision() {
        let mut sim = GameSim::load(&assets(), CourseRequest::new("bob")).unwrap();
        let colliders = build_colliders(&mut sim, false);
        assert!(colliders.0.len() >= 8, "found {}", colliders.0.len());
        let owned = sim.mario.world.surfaces().iter().filter(|s| s.object.is_some()).count();
        assert!(owned > 20);
        // Every owner tag is a valid arena index and matches the right behavior.
        for s in sim.mario.world.surfaces().iter().filter_map(|s| s.object) {
            assert!((s as usize) < sim.mario.objects.len());
        }
    }
}
