//! `geometry.ts` (collision half): helpers that add planar faces / boxes / ramps to a
//! [`CollisionWorld`] with the requested outward normal, whatever the vertex order.
//! Texturing is a render concern and lives with each experiment's meshes.
use jumpman_sim::collision::{CollisionWorld, Surface, SurfaceOptions};
use jumpman_sim::math::Vec3;
use std::sync::Arc;

/// Add a planar convex polygon as collision triangles whose normal faces `outward`.
pub fn add_face(world: &mut CollisionWorld, points: &[Vec3], outward: [f64; 3], into: &mut Vec<Arc<Surface>>, options: &SurfaceOptions) {
    if points.len() < 3 {
        return;
    }
    let (a, b, c) = (points[0], points[1], points[2]);
    // Same convention as CollisionWorld::add_triangle: n = (v2 - v1) x (v3 - v2).
    let nx = (b[1] - a[1]) * (c[2] - b[2]) - (b[2] - a[2]) * (c[1] - b[1]);
    let ny = (b[2] - a[2]) * (c[0] - b[0]) - (b[0] - a[0]) * (c[2] - b[2]);
    let nz = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
    let flip = nx * outward[0] + ny * outward[1] + nz * outward[2] < 0.0;
    let mut ordered = points.to_vec();
    if flip {
        ordered.reverse();
    }
    for i in 1..ordered.len() - 1 {
        if let Some(s) = world.add_triangle(ordered[0], ordered[i], ordered[i + 1], options) {
            into.push(s);
        }
    }
}

/// Collision for a box rotated by `yaw` (radians) about Y.
pub fn add_box_collision(world: &mut CollisionWorld, center: Vec3, size: Vec3, yaw: f64, into: &mut Vec<Arc<Surface>>, options: &SurfaceOptions) {
    let (c, s) = (yaw.cos(), yaw.sin());
    let (hx, hy, hz) = (size[0] / 2.0, size[1] / 2.0, size[2] / 2.0);
    let corner = |x: f64, y: f64, z: f64| -> Vec3 { [center[0] + x * c + z * s, center[1] + y, center[2] - x * s + z * c] };
    let top = [corner(-hx, hy, -hz), corner(hx, hy, -hz), corner(hx, hy, hz), corner(-hx, hy, hz)];
    let bottom = [corner(-hx, -hy, -hz), corner(hx, -hy, -hz), corner(hx, -hy, hz), corner(-hx, -hy, hz)];
    add_face(world, &top, [0., 1., 0.], into, options);
    add_face(world, &bottom, [0., -1., 0.], into, options);
    let sides: [(usize, usize, [f64; 3]); 4] = [(0, 1, [-s, 0., -c]), (1, 2, [c, 0., -s]), (2, 3, [s, 0., c]), (3, 0, [-c, 0., s])];
    for (i, j, n) in sides {
        add_face(world, &[top[i], top[j], bottom[j], bottom[i]], n, into, options);
    }
}

/// Collision for a wedge rising toward local -Z, rotated by `yaw`.
pub fn add_ramp_collision(world: &mut CollisionWorld, center: Vec3, size: Vec3, yaw: f64, into: &mut Vec<Arc<Surface>>) {
    let (c, s) = (yaw.cos(), yaw.sin());
    let (hx, hy, hz) = (size[0] / 2.0, size[1] / 2.0, size[2] / 2.0);
    let corner = |x: f64, y: f64, z: f64| -> Vec3 { [center[0] + x * c + z * s, center[1] + y, center[2] - x * s + z * c] };
    let (front_l, front_r) = (corner(-hx, -hy, hz), corner(hx, -hy, hz));
    let (back_l, back_r) = (corner(-hx, -hy, -hz), corner(hx, -hy, -hz));
    let (top_l, top_r) = (corner(-hx, hy, -hz), corner(hx, hy, -hz));
    let len = (size[2] * size[2] + size[1] * size[1]).sqrt();
    let (slope_y, slope_z) = (size[2] / len, size[1] / len);
    let n = [slope_z * s, slope_y, slope_z * c];
    let o = SurfaceOptions::default();
    add_face(world, &[front_l, front_r, top_r, top_l], n, into, &o);
    add_face(world, &[front_l, front_r, back_r, back_l], [0., -1., 0.], into, &o);
    add_face(world, &[back_l, back_r, top_r, top_l], [-s, 0., -c], into, &o);
    add_face(world, &[front_l, back_l, top_l], [-c, 0., s], into, &o);
    add_face(world, &[front_r, back_r, top_r], [c, 0., -s], into, &o);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_top_is_standable_and_sides_are_walls() {
        let mut w = CollisionWorld::default();
        let mut into = vec![];
        add_box_collision(&mut w, [0., 100., 0.], [400., 200., 400.], 0.5, &mut into, &SurfaceOptions::default());
        assert_eq!(into.len(), 12);
        let f = w.find_floor(0., 500., 0.);
        assert!((f.height - 200.0).abs() < 1e-6);
        assert_eq!(into.iter().filter(|s| s.normal[1] > 0.99).count(), 2);
    }

    #[test]
    fn ramp_rises_toward_negative_z() {
        let mut w = CollisionWorld::default();
        let mut into = vec![];
        add_ramp_collision(&mut w, [0., 100., 0.], [400., 200., 600.], 0.0, &mut into);
        let low = w.find_floor(0., 600., 250.).height;
        let high = w.find_floor(0., 600., -250.).height;
        assert!(high > low + 100.0, "{low} {high}");
    }
}
