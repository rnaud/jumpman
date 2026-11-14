//! Solid bodies for the minigames: the cheap circle push-out every mode uses so the player and the monsters / enemies /
//! other fighters never stand inside each other (Star Swarm's `body_block` is the swept, crowd-sized version of the same
//! idea). Engine-free and deterministic: a coincident pair always separates along the caller's fallback direction, so
//! the result depends only on the inputs (no random tie-breaks, no allocation).
//!
//! Every body is a vertical cylinder on the ground plane, `(x, z)` centre and a radius; the modes decide who is solid
//! (ghosts stay intangible), who is heavy (cannot be shoved) and which share of an overlap each side takes. A body's
//! solid radius is a little under its contact / hit reach ([`SQUEEZE`]) so standing at body contact still touches,
//! still bites and still lands every melee move.

/// Solid bodies are this fraction of the sum of the two contact radii: touching still registers as contact.
pub const SQUEEZE: f32 = 0.85;

/// Distance on the ground plane.
#[inline]
pub fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Unit direction from `from` to `to`, or `fallback` when they (almost) coincide.
#[inline]
pub fn away(from: [f32; 2], to: [f32; 2], fallback: [f32; 2]) -> [f32; 2] {
    let d = [to[0] - from[0], to[1] - from[1]];
    let l = d[0].hypot(d[1]);
    if l > 0.01 { [d[0] / l, d[1] / l] } else { fallback }
}

/// `p` pushed out of a solid circle at `c` of radius `reach` (unchanged when outside).
#[inline]
pub fn push_out(p: [f32; 2], c: [f32; 2], reach: f32, fallback: [f32; 2]) -> [f32; 2] {
    let d = dist(p, c);
    if d >= reach {
        return p;
    }
    let u = away(c, p, fallback);
    [c[0] + u[0] * reach, c[1] + u[1] * reach]
}

/// How far two bodies overlap (0 when apart).
#[inline]
pub fn overlap(a: [f32; 2], b: [f32; 2], reach: f32) -> f32 {
    (reach - dist(a, b)).max(0.0)
}

/// Separation of an overlapping pair: the moves `(da, db)` that put `a` and `b` exactly `reach` apart, `a` taking
/// `share_a` (0..1) of the correction. `None` when they do not overlap.
#[inline]
pub fn separate(a: [f32; 2], b: [f32; 2], reach: f32, share_a: f32, fallback: [f32; 2]) -> Option<([f32; 2], [f32; 2])> {
    let o = overlap(a, b, reach);
    if o <= 0.0 {
        return None;
    }
    let u = away(b, a, fallback);
    let (ka, kb) = (o * share_a, o * (1.0 - share_a));
    Some(([u[0] * ka, u[1] * ka], [-u[0] * kb, -u[1] * kb]))
}

/// The hero pushed out of a set of immovable bodies `(centre, reach)`: a few passes, since stepping out of one can press
/// into the next. If the bodies wedge the hero with no room (two closer together than the hero is wide), he stays where
/// he would have been pushed by the last pass. Returns the corrected point.
pub fn push_out_all(mut p: [f32; 2], bodies: &[([f32; 2], f32)], fallback: [f32; 2]) -> [f32; 2] {
    for _ in 0..4 {
        let before = p;
        for &(c, r) in bodies {
            p = push_out(p, c, r, fallback);
        }
        if dist(before, p) < 0.01 {
            break;
        }
    }
    p
}

/// Do the vertical extents `[a, a + ha]` and `[b, b + hb]` overlap, ignoring the top `clear` fraction of `b` (a jump that
/// gets the feet over most of a body clears it, as for contact damage)?
#[inline]
pub fn stacked(a: f32, ha: f32, b: f32, hb: f32, clear: f32) -> bool {
    a < b + hb * (1.0 - clear) && b < a + ha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_out_puts_a_point_on_the_rim_and_leaves_outside_points_alone() {
        let p = push_out([10.0, 0.0], [0.0, 0.0], 50.0, [1.0, 0.0]);
        assert!((dist(p, [0.0, 0.0]) - 50.0).abs() < 1e-3 && p[0] > 0.0);
        assert_eq!(push_out([60.0, 0.0], [0.0, 0.0], 50.0, [1.0, 0.0]), [60.0, 0.0]);
        // Coincident: the fallback decides, deterministically.
        assert_eq!(push_out([0.0, 0.0], [0.0, 0.0], 50.0, [0.0, 1.0]), [0.0, 50.0]);
    }

    #[test]
    fn separate_splits_the_overlap_by_share() {
        let (da, db) = separate([0.0, 0.0], [60.0, 0.0], 100.0, 0.25, [1.0, 0.0]).unwrap();
        assert!((da[0] + 10.0).abs() < 1e-3 && (db[0] - 30.0).abs() < 1e-3);
        assert!(separate([0.0, 0.0], [120.0, 0.0], 100.0, 0.5, [1.0, 0.0]).is_none());
    }

    #[test]
    fn push_out_all_escapes_a_cluster() {
        let bodies = [([0.0, 0.0], 50.0), ([140.0, 0.0], 50.0), ([70.0, 90.0], 50.0)];
        let p = push_out_all([60.0, 20.0], &bodies, [0.0, -1.0]);
        for (c, r) in bodies {
            assert!(dist(p, c) >= r - 0.5, "{p:?} inside {c:?}");
        }
    }
}
