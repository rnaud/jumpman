//! `DoorCollider` (`roomDoors.ts`): a movable native collision mesh whose surfaces carry an owner
//! tag in `Surface::object`, so it can be removed and re-placed in the world each time it moves.
use super::course_data::CollisionData;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::surfaces::SURFACE_FLAG_NO_CAM_COLLISION;
use std::sync::Arc;

/// `addNativeCollision` with an owner tag on every surface.
pub fn add_owned_collision(world: &mut CollisionWorld, data: &CollisionData, pos: [f64; 3], yaw_degrees: f64, owner: Option<u32>) {
    let angle = yaw_degrees * std::f64::consts::PI / 180.0;
    let (c, s) = (angle.cos(), angle.sin());
    let vertices: Vec<[f64; 3]> = data.vertices.iter().map(|&[x, y, z]| [c * x + s * z + pos[0], y + pos[1], -s * x + c * z + pos[2]]).collect();
    for (i, &[a, b, cc, kind, force]) in data.triangles.iter().enumerate() {
        let (Some(&va), Some(&vb), Some(&vc)) = (vertices.get(a as usize), vertices.get(b as usize), vertices.get(cc as usize)) else { continue };
        let opts = SurfaceOptions {
            room: data.rooms.get(i).copied(),
            kind,
            force,
            flags: if (0x76..=0x79).contains(&kind) { SURFACE_FLAG_NO_CAM_COLLISION } else { 0 },
            object: owner,
        };
        world.add_triangle(va, vb, vc, &opts);
    }
}

/// A movable collision mesh: `place(Some(pos))` rebuilds it there, `None` removes it.
#[derive(Clone)]
pub struct Collider {
    pub data: CollisionData,
    pub yaw_degrees: f64,
    pub owner: u32,
    pub placed: bool,
}

impl Collider {
    pub fn new(data: CollisionData, yaw_degrees: f64, owner: u32) -> Self {
        Self { data, yaw_degrees, owner, placed: false }
    }

    pub fn place(&mut self, world: &mut Arc<CollisionWorld>, pos: Option<[f64; 3]>) {
        let world = Arc::make_mut(world);
        let owner = self.owner;
        if self.placed {
            world.remove_surfaces(&|s| s.object == Some(owner));
        }
        self.placed = pos.is_some();
        if let Some(pos) = pos {
            add_owned_collision(world, &self.data, pos, self.yaw_degrees, Some(owner));
        }
    }
}
