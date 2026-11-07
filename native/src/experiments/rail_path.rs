//! `railPath.ts`: a horizontal rail for 2.5D play. Mario's x/z are projected onto it every step
//! while his height stays free. Corners from the route planner are rounded with Catmull-Rom.
use jumpman_sim::math::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct RailHit {
    /// Distance along the rail.
    pub s: f64,
    /// Closest rail point (horizontal); y is the rail's authored height there.
    pub point: Vec3,
    /// Horizontal unit tangent toward increasing s (x, z).
    pub tangent: [f64; 2],
    pub index: usize,
}

pub struct RailPath {
    pub points: Vec<Vec3>,
    pub lengths: Vec<f64>,
    pub total: f64,
}

fn hyp(a: f64, b: f64) -> f64 {
    a.hypot(b)
}

impl RailPath {
    pub fn new(route: &[Vec3]) -> Self {
        Self::with_spacing(route, 60.0)
    }

    pub fn with_spacing(route: &[Vec3], spacing: f64) -> Self {
        // Unfold before and after smoothing: the spline itself can loop where route points crowd.
        let points = resample(&unfold(&resample(&smooth(&unfold(route, -0.55)), spacing), -0.2), spacing);
        let mut lengths = vec![0.0];
        for i in 1..points.len() {
            let (a, b) = (points[i - 1], points[i]);
            lengths.push(lengths[i - 1] + hyp(b[0] - a[0], b[2] - a[2]));
        }
        let total = *lengths.last().unwrap();
        Self { points, lengths, total }
    }

    /// Nearest point to `pos`, searching around `hint` (the last index) when given.
    pub fn nearest(&self, pos: &Vec3, hint: Option<usize>, window: usize) -> RailHit {
        let last = self.points.len() - 2;
        let (from, to) = match hint {
            None => (0, last),
            Some(h) => (h.saturating_sub(window), (h + window).min(last)),
        };
        let mut best: Option<RailHit> = None;
        let mut best_d = f64::INFINITY;
        for i in from..=to {
            let (a, b) = (self.points[i], self.points[i + 1]);
            let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
            let len2 = if dx * dx + dz * dz == 0.0 { 1.0 } else { dx * dx + dz * dz };
            let t = (((pos[0] - a[0]) * dx + (pos[2] - a[2]) * dz) / len2).clamp(0.0, 1.0);
            let (x, z) = (a[0] + dx * t, a[2] + dz * t);
            // Prefer the layer at Mario's height where the rail passes over itself.
            let y = a[1] + (b[1] - a[1]) * t;
            let d = hyp(pos[0] - x, pos[2] - z) + ((pos[1] - y).abs() - 400.0).max(0.0) * 2.0;
            if d < best_d {
                best_d = d;
                let len = len2.sqrt();
                best = Some(RailHit { s: self.lengths[i] + len * t, point: [x, y, z], tangent: [dx / len, dz / len], index: i });
            }
        }
        best.unwrap()
    }

    /// Point and tangent at distance s (clamped).
    pub fn at(&self, s: f64) -> RailHit {
        let clamped = s.clamp(0.0, self.total);
        let (mut lo, mut hi) = (0usize, self.lengths.len() - 1);
        while hi - lo > 1 {
            let mid = (lo + hi) >> 1;
            if self.lengths[mid] <= clamped {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (a, b) = (self.points[lo], self.points[hi]);
        let seg = self.lengths[hi] - self.lengths[lo];
        let seg = if seg == 0.0 { 1.0 } else { seg };
        let t = (clamped - self.lengths[lo]) / seg;
        let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
        let len = hyp(dx, dz);
        let len = if len == 0.0 { 1.0 } else { len };
        RailHit { s: clamped, point: [a[0] + dx * t, a[1] + (b[1] - a[1]) * t, a[2] + dz * t], tangent: [dx / len, dz / len], index: lo }
    }
}

/// Remove dead-end spikes: a route that runs into a corner and doubles back folds the rail onto
/// itself, which a 2.5D projection cannot follow.
pub fn unfold(route: &[Vec3], threshold: f64) -> Vec<Vec3> {
    let mut out = route.to_vec();
    let mut changed = true;
    while changed && out.len() > 2 {
        changed = false;
        for i in 1..out.len() - 1 {
            let (a, b, c) = (out[i - 1], out[i], out[i + 1]);
            let (ux, uz, vx, vz) = (b[0] - a[0], b[2] - a[2], c[0] - b[0], c[2] - b[2]);
            let (lu, lv) = (hyp(ux, uz), hyp(vx, vz));
            if lu < 1.0 || lv < 1.0 || (ux * vx + uz * vz) / (lu * lv) < threshold {
                out.remove(i);
                changed = true;
                break;
            }
        }
    }
    out
}

/// Centripetal-ish Catmull-Rom through the route, 6 samples per segment.
fn smooth(route: &[Vec3]) -> Vec<Vec3> {
    if route.len() < 3 {
        return route.to_vec();
    }
    let mut out = vec![];
    for i in 0..route.len() - 1 {
        let p0 = route[i.saturating_sub(1)];
        let (p1, p2) = (route[i], route[i + 1]);
        let p3 = route[(i + 2).min(route.len() - 1)];
        for k in 0..6 {
            let t = k as f64 / 6.0;
            let (t2, t3) = (t * t, t * t * t);
            let c = |a: f64, b: f64, c2: f64, d: f64| 0.5 * (2.0 * b + (-a + c2) * t + (2.0 * a - 5.0 * b + 4.0 * c2 - d) * t2 + (-a + 3.0 * b - 3.0 * c2 + d) * t3);
            out.push([c(p0[0], p1[0], p2[0], p3[0]), c(p0[1], p1[1], p2[1], p3[1]), c(p0[2], p1[2], p2[2], p3[2])]);
        }
    }
    out.push(*route.last().unwrap());
    out
}

fn resample(points: &[Vec3], spacing: f64) -> Vec<Vec3> {
    let mut out = vec![points[0]];
    let mut carry = 0.0;
    for i in 1..points.len() {
        let (a, b) = (points[i - 1], points[i]);
        let len = hyp(b[0] - a[0], b[2] - a[2]);
        let mut d = spacing - carry;
        while d <= len {
            let t = d / len;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
            d += spacing;
        }
        carry = len - (d - spacing);
    }
    let last = *points.last().unwrap();
    let tail = out.last().unwrap();
    if hyp(last[0] - tail[0], last[2] - tail[2]) > 1.0 {
        out.push(last);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_dead_end_spikes_that_fold_the_rail_back_on_itself() {
        let route = [[0., 0., 0.], [1000., 0., 0.], [1600., 0., 0.], [1200., 0., 40.], [2400., 0., 600.]];
        let cleaned = unfold(&route, -0.55);
        for i in 1..cleaned.len() - 1 {
            let (a, b, c) = (cleaned[i - 1], cleaned[i], cleaned[i + 1]);
            let dot = (b[0] - a[0]) * (c[0] - b[0]) + (b[2] - a[2]) * (c[2] - b[2]);
            assert!(dot > 0.0);
        }
    }

    #[test]
    fn projects_onto_the_nearest_point_and_reports_a_forward_tangent() {
        let rail = RailPath::new(&[[0., 0., 0.], [2000., 0., 0.], [2000., 0., 2000.]]);
        let hit = rail.nearest(&[300., 0., 400.], None, 40);
        let mut s = 0.0;
        while s <= rail.total {
            let p = rail.at(s).point;
            assert!(hyp(p[0] - 300., p[2] - 400.) >= hyp(hit.point[0] - 300., hit.point[2] - 400.) - 1.0);
            s += 50.0;
        }
        assert!(hit.tangent[0] > 0.9);
        assert!((rail.at(rail.total).point[2] - 2000.0).abs() < 0.5);
    }
}
