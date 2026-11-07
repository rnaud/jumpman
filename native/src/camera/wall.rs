//! Port of `cameraWall.ts` (obstructing-wall branch of `rotate_camera_around_walls`) plus the
//! camera-flavoured wall push (`f32FindWallCollision(pos, offsetY, radius, true)`).
use bevy::math::DVec3;
use jumpman_sim::collision::{CollisionWorld, Surface, WallData};
use jumpman_sim::math::{atan2s, clamp, coss, sins, to_s16};

/// Push a point out of walls using the camera collision set.
pub fn wall_push(world: &CollisionWorld, pos: DVec3, offset_y: f64, radius: f64) -> DVec3 {
    let mut d = WallData { x: pos.x, y: pos.y, z: pos.z, offset_y, radius, walls: vec![] };
    world.find_wall_collisions(&mut d, true);
    DVec3::new(d.x, d.y, d.z)
}

fn in_front(x: f64, y: f64, z: f64, wall: &Surface) -> bool {
    (x - wall.v1[0]) * wall.normal[0] + (y - wall.v1[1]) * wall.normal[1] + (z - wall.v1[2]) * wall.normal[2] > 0.0
}

/// Obstructing-wall branch of rotate_camera_around_walls. Radial mode ignores near-only hits.
pub fn radial_wall_yaw(mario: DVec3, eye: DVec3, world: &CollisionWorld) -> Option<f64> {
    let (dx, dy, dz) = (eye.x - mario.x, eye.y - mario.y, eye.z - mario.z);
    let yaw = atan2s(dz, dx) as f64;
    let flat = jumpman_sim::math::js_hypot(dx, dz);
    let distance = jumpman_sim::math::js_hypot(flat, dy);
    let pitch = atan2s(flat, dy) as f64;
    let reach = distance * coss(pitch);
    let y = mario.y + distance * sins(pitch);
    let range = 0x400 as f64;
    let mut data = WallData { x: 0.0, y: 0.0, z: 0.0, offset_y: 100.0, radius: 150.0, walls: vec![] };
    let mut fine_radius = 100.0;
    for step in 0..8 {
        let t = step as f64 / 8.0;
        let (x, z) = (mario.x + dx * t, mario.z + dz * t);
        data.x = x;
        data.y = mario.y + dy * t;
        data.z = z;
        data.radius = (150.0 + 30.0 * step as f64).min(250.0);
        if world.find_wall_collisions(&mut data, true) == 0 {
            continue;
        }
        data.x = x;
        data.z = z;
        data.radius = fine_radius;
        fine_radius = (fine_radius + 20.0).min(200.0);
        if world.find_wall_collisions(&mut data, true) == 0 {
            continue;
        }
        let Some(wall) = data.walls.last() else { continue };
        if wall.kind == 0x28 || !in_front(mario.x, mario.y, mario.z, wall) {
            continue;
        }
        let heights = [wall.v1[1], wall.v2[1], wall.v3[1]];
        let hi = heights.iter().cloned().fold(f64::MIN, f64::max);
        let lo = heights.iter().cloned().fold(f64::MAX, f64::min);
        if hi - lo < 150.0 {
            continue;
        }
        if in_front(mario.x + reach * sins(yaw + range), y, mario.z + reach * coss(yaw + range), wall)
            && in_front(mario.x + reach * sins(yaw - range), y, mario.z + reach * coss(yaw - range), wall)
        {
            continue;
        }
        let normal = atan2s(wall.normal[2], wall.normal[0]) as f64;
        let tangent = normal + 0x4000 as f64;
        let avoid = to_s16(tangent + if to_s16(tangent - yaw + 0x4000 as f64) < 0 { 0x8000 as f64 } else { 0.0 }) as f64;
        return Some(to_s16(avoid + clamp(to_s16(normal - avoid) as f64, -range, range)) as f64);
    }
    None
}
