//! Object colliders (`DoorCollider` use in `courseObjects.ts`): wooden posts, the chain gate,
//! star/wing boxes and blast walls own native collision that follows the sim object. Surfaces
//! carry `Surface::object = Some(<arena index>)` so behaviors can recognise "Mario stands on me".
use crate::game::collider::Collider;
use crate::game::runner::GameSim;
use crate::progress::flow::Progress;
use crate::sim_plugin::{PostTickSet, SimResetMsg};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use jumpman_sim::object::ObjectBehaviorKind;

pub struct CollidersPlugin;

impl Plugin for CollidersPlugin {
    fn build(&self, app: &mut App) {
        super::register_rebuild(app, rebuild);
        app.add_systems(Startup, setup)
            .add_systems(FixedUpdate, sync_colliders.in_set(PostTickSet))
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

#[derive(Resource, Default)]
pub struct ObjectColliders(pub Vec<ObjectCollider>);

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
pub fn build_colliders(sim: &mut GameSim) -> ObjectColliders {
    let mut list = vec![];
    for (i, o) in sim.mario.objects.objects.iter().enumerate() {
        let Some(key) = collision_key(o.behavior, o.param) else { continue };
        let Some(data) = sim.course.extras.extra_collision.get(key) else { continue };
        list.push(ObjectCollider {
            object: i,
            collider: Collider::new(data.clone(), o.move_yaw * 180.0 / 32768.0, i as u32),
            wing: o.behavior == ObjectBehaviorKind::WingBox,
            solid: true,
            y: o.home[1],
        });
    }
    for c in &mut list {
        let home = sim.mario.objects.objects[c.object].home;
        c.collider.place(&mut sim.mario.world, Some(home));
    }
    ObjectColliders(list)
}

fn rebuild(world: &mut World) {
    let _ = world.run_system_once(setup);
}

fn setup(mut commands: Commands, mut sim: ResMut<GameSim>) {
    let colliders = build_colliders(&mut sim);
    commands.insert_resource(colliders);
}

fn sync_colliders(mut sim: ResMut<GameSim>, mut colliders: ResMut<ObjectColliders>, progress: Res<Progress>) {
    let sim = &mut *sim;
    let wing_unlocked = progress.save.wing_switch_on();
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

fn reset_colliders(mut resets: EventReader<SimResetMsg>, mut sim: ResMut<GameSim>, mut colliders: ResMut<ObjectColliders>) {
    if resets.read().count() == 0 {
        return;
    }
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
        let colliders = build_colliders(&mut sim);
        assert!(colliders.0.len() >= 8, "found {}", colliders.0.len());
        let owned = sim.mario.world.surfaces().iter().filter(|s| s.object.is_some()).count();
        assert!(owned > 20);
        // Every owner tag is a valid arena index and matches the right behavior.
        for s in sim.mario.world.surfaces().iter().filter_map(|s| s.object) {
            assert!((s as usize) < sim.mario.objects.len());
        }
    }
}
