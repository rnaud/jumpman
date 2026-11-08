//! Port of `src/game/nativeCollision.ts` (+ `coinFormation.ts`): exported SM64 collision data
//! placed into a [`CollisionWorld`].
use super::course_data::{CoinFormation, CollisionData, V3};
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use jumpman_sim::math::{coss, sins};
use jumpman_sim::surfaces::{SURFACE_FLAG_HOOKABLE, SURFACE_FLAG_NO_CAM_COLLISION};

/// `addNativeCollision`: rotate by `yaw_degrees` about Y, translate by `pos`, add every triangle.
pub fn add_native_collision(world: &mut CollisionWorld, data: &CollisionData, pos: V3, yaw_degrees: f64) {
    let angle = yaw_degrees * std::f64::consts::PI / 180.0;
    let (c, s) = (angle.cos(), angle.sin());
    let vertices: Vec<V3> = data
        .vertices
        .iter()
        .map(|&[x, y, z]| [c * x + s * z + pos[0], y + pos[1], -s * x + c * z + pos[2]])
        .collect();
    let hookable: std::collections::HashSet<u32> = data.hookable.iter().flatten().copied().collect();
    world.hook_marked |= data.hookable.is_some();
    for (i, &[a, b, cc, kind, force]) in data.triangles.iter().enumerate() {
        let (Some(&va), Some(&vb), Some(&vc)) = (
            vertices.get(a as usize),
            vertices.get(b as usize),
            vertices.get(cc as usize),
        ) else {
            continue;
        };
        let opts = SurfaceOptions {
            room: data.rooms.get(i).copied(),
            kind,
            force,
            flags: (if (0x76..=0x79).contains(&kind) { SURFACE_FLAG_NO_CAM_COLLISION } else { 0 }) | if hookable.contains(&(i as u32)) { SURFACE_FLAG_HOOKABLE } else { 0 },
            object: None,
        };
        world.add_triangle(va, vb, vc, &opts);
    }
}

const ARROW: [[f64; 2]; 8] = [[0., -150.], [0., -50.], [0., 50.], [0., 150.], [-50., 100.], [-100., 50.], [50., 100.], [100., 50.]];

/// `formationCoins` (spawn_coin_in_formation).
pub fn formation_coins(f: &CoinFormation, world: &CollisionWorld) -> Vec<V3> {
    let mut out = vec![];
    let count = if f.shape.starts_with("line_") { 5 } else { 8 };
    for i in 0..count {
        let (mut x, mut y, mut z) = (0.0f64, 0.0f64, 0.0f64);
        let ang = (i << 13) as f64;
        match f.shape.as_str() {
            "line_horizontal" => z = 160.0 * (i as f64 - 2.0),
            "line_vertical" => y = 128.0 * i as f64,
            "ring_horizontal" => {
                x = (sins(ang) * 300.0).trunc();
                z = (coss(ang) * 300.0).trunc();
            }
            "ring_vertical" => {
                x = (coss(ang) * 200.0).trunc();
                y = (sins(ang) * 200.0 + 200.0).trunc();
            }
            "arrow" => {
                x = ARROW[i][0];
                z = ARROW[i][1];
            }
            _ => {}
        }
        let px = f.pos[0] + coss(f.yaw) * x + sins(f.yaw) * z;
        let pz = f.pos[2] - sins(f.yaw) * x + coss(f.yaw) * z;
        let mut py = f.pos[1] + y;
        if !f.flying && !f.shape.ends_with("_vertical") {
            let floor = world.find_floor(px, py + 300.0, pz);
            if floor.surface.is_none() || floor.height > py + 300.0 || floor.height < -10000.0 {
                continue;
            }
            py = floor.height;
        }
        out.push([px, py, pz]);
    }
    out
}
