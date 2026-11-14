//! Standing things on the ground, shared by the special modes: how far below its origin each centre-origin model reaches
//! (a mode lifts it by that times its scale, so the ball / ghost / heart sits on the floor instead of half in it), and the
//! floor queries the views use to put a foe's feet or a pickup's base on the real terrain at its own (x, z), slopes and
//! bumps included. The models that stand on their origin (Goombas, Bob-ombs, Koopas, Thwomps, coins, chests ...) need
//! nothing here.
use jumpman_sim::collision::CollisionWorld;

/// Chain Chomp: its two jaw halves are hemispheres of 145 about the origin (the centre of the ball).
pub const CHOMP_FOOT: f32 = 145.0;
/// Boo: the ghost reaches 55 below its centre.
pub const BOO_FOOT: f32 = 55.0;
/// Recovery heart (a sprite centred on its origin).
pub const HEART_FOOT: f32 = 50.0;
/// Power Star (centred on its origin).
pub const STAR_FOOT: f32 = 50.0;
/// 1-up mushroom (a sprite centred on its origin).
pub const MUSHROOM_FOOT: f32 = 31.0;
/// Iron ball (a sprite centred on its origin).
pub const IRON_BALL_FOOT: f32 = 127.0;
/// Amp (a small ball centred on its origin).
pub const AMP_FOOT: f32 = 18.0;
/// Fire (a sprite centred on its origin).
pub const FLAME_FOOT: f32 = 25.0;
/// Small key.
pub const KEY_FOOT: f32 = 27.0;

/// Model units from a model's origin down to its lowest point at scale 1 (SM64 units) for the models whose origin is not at
/// their base; 0 for the ones standing on their origin.
pub fn foot(path: &str) -> f32 {
    match path.rsplit('/').next().unwrap_or(path) {
        "chain_chomp_geo.glb" => CHOMP_FOOT,
        "boo_geo.glb" => BOO_FOOT,
        "heart_geo.glb" => HEART_FOOT,
        "star_geo.glb" => STAR_FOOT,
        "mushroom_1up_geo.glb" => MUSHROOM_FOOT,
        "bowling_ball_geo.glb" => IRON_BALL_FOOT,
        "dAmpGeo.glb" => AMP_FOOT,
        "red_flame_geo.glb" => FLAME_FOOT,
        "small_key_geo.glb" => KEY_FOOT,
        _ => 0.0,
    }
}

/// How far a model drawn at `scale` must be lifted above the point it stands on so its lowest point touches it.
pub fn rest_lift(path: &str, scale: f32) -> f32 {
    foot(path) * scale
}

/// Highest a real floor may be above the expected ground and still count as the ground there (a bump, a step, a rise the
/// coarse sampling missed); anything higher is a roof, a ledge or a tree top.
pub const SNAP_UP: f64 = 160.0;
/// Lowest a real floor may be below the expected ground and still count (a dip the coarse sampling missed); lower than that
/// it is a pit or the level under a bridge, and the expected ground stands.
pub const SNAP_DOWN: f64 = 220.0;

/// The real floor under (x, z) near an expected ground height `near` (a mode's coarse grid, a tile's height, the last
/// height): its height and the y of its normal, or None when nothing is within [`SNAP_UP`] above / [`SNAP_DOWN`] below.
pub fn floor_hit(world: &CollisionWorld, x: f64, z: f64, near: f64) -> Option<(f64, f64)> {
    let top = near + SNAP_UP;
    let mut q = top;
    for _ in 0..4 {
        let f = world.find_floor(x, q, z);
        let s = f.surface.as_ref()?;
        if f.height <= top + 0.01 {
            return (f.height >= near - SNAP_DOWN).then_some((f.height, s.normal[1].clamp(0.05, 1.0)));
        }
        // `find_floor` takes floors up to 78 above the query: look under the roof it found.
        q = f.height - 79.0;
    }
    None
}

/// [`floor_hit`]'s height, else `near`.
pub fn floor_near(world: &CollisionWorld, x: f64, z: f64, near: f64) -> f64 {
    floor_hit(world, x, z, near).map_or(near, |h| h.0)
}

/// The slope's normal y under (x, z) from a height function (central differences `d` apart): 1 on the flat. For modes whose
/// ground is a function rather than a collision world.
pub fn normal_y(height: impl Fn(f32, f32) -> Option<f32>, x: f32, z: f32, d: f32) -> f32 {
    let (Some(xa), Some(xb), Some(za), Some(zb)) = (height(x - d, z), height(x + d, z), height(x, z - d), height(x, z + d)) else { return 1.0 };
    let (gx, gz) = ((xb - xa) / (2.0 * d), (zb - za) / (2.0 * d));
    1.0 / (1.0 + gx * gx + gz * gz).sqrt()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use jumpman_sim::collision::SurfaceOptions;
    use std::sync::Arc;

    /// A test world: a plane tilted along x (`y = base + slope * x`) over ±`half`. Shared by the per-mode ground tests.
    pub fn slope_world(base: f64, slope: f64, half: f64) -> Arc<CollisionWorld> {
        Arc::new(slope_world_raw(base, slope, half, false))
    }

    /// [`slope_world`] with a 70-unit plateau (a flat-topped mound) over x, z in 300..700, so the ground is uneven too.
    pub fn bumpy_world(base: f64, slope: f64, half: f64) -> Arc<CollisionWorld> {
        Arc::new(slope_world_raw(base, slope, half, true))
    }

    /// The ground of [`bumpy_world`] (or [`slope_world`] with `bump` false) under (x, z).
    pub fn bumpy_y(base: f64, slope: f64, x: f64, z: f64, bump: bool) -> f64 {
        let on = bump && (300.0..=700.0).contains(&x) && (300.0..=700.0).contains(&z);
        if on { base + slope * 700.0 + 70.0 } else { base + slope * x }
    }

    fn slope_world_raw(base: f64, slope: f64, half: f64, bump: bool) -> CollisionWorld {
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        let y = |x: f64| base + slope * x;
        let n = 8;
        let step = 2.0 * half / n as f64;
        for i in 0..n {
            for j in 0..n {
                let (x0, z0) = (-half + i as f64 * step, -half + j as f64 * step);
                let (x1, z1) = (x0 + step, z0 + step);
                w.add_triangle([x0, y(x0), z0], [x1, y(x1), z1], [x1, y(x1), z0], &o);
                w.add_triangle([x0, y(x0), z0], [x0, y(x0), z1], [x1, y(x1), z1], &o);
            }
        }
        if bump {
            let top = base + slope * 700.0 + 70.0;
            w.add_triangle([300.0, top, 300.0], [700.0, top, 700.0], [700.0, top, 300.0], &o);
            w.add_triangle([300.0, top, 300.0], [300.0, top, 700.0], [700.0, top, 700.0], &o);
        }
        w
    }

    /// The height of [`slope_world`]'s plane.
    pub fn slope_y(base: f64, slope: f64, x: f64) -> f64 {
        base + slope * x
    }

    #[test]
    fn floor_queries_follow_a_slope_and_ignore_roofs_and_pits() {
        let w = slope_world(1000.0, 0.4, 3000.0);
        for x in [-2000.0, -500.0, 0.0, 700.0, 2100.0] {
            let want = slope_y(1000.0, 0.4, x);
            // The expected ground may be off by a coarse cell's error either way.
            for err in [-120.0, 0.0, 140.0] {
                assert!((floor_near(&w, x, 50.0, want + err) - want).abs() < 0.5, "x {x} err {err}");
            }
            // Way off: not this floor.
            assert_eq!(floor_near(&w, x, 50.0, want - 900.0), want - 900.0);
        }
        // The slope's normal, from the world and from a height function: a ball of radius r resting on it has its centre
        // r / ny over the plane under it, exactly r from the plane.
        let ny = 1.0 / (1.0f64 + 0.16).sqrt();
        let (h, wny) = floor_hit(&w, 0.0, 0.0, 1000.0).unwrap();
        assert!((h - 1000.0).abs() < 0.5 && (wny - ny).abs() < 1e-3);
        let r = 150.0;
        let c = h + r / wny;
        let d = (0.4 * 0.0 - c + 1000.0).abs() / (0.16f64 + 1.0).sqrt();
        assert!((d - r).abs() < 0.5, "{d}");
        // A roof 100 over the expected ground (a bridge, a tree top) is looked under, not stood on.
        let mut roofed = slope_world_raw(1000.0, 0.0, 3000.0, false);
        let o = SurfaceOptions::default();
        roofed.add_triangle([-200.0, 1100.0, -200.0], [200.0, 1100.0, 200.0], [200.0, 1100.0, -200.0], &o);
        roofed.add_triangle([-200.0, 1100.0, -200.0], [-200.0, 1100.0, 200.0], [200.0, 1100.0, 200.0], &o);
        assert!((floor_near(&roofed, 0.0, 0.0, 900.0) - 1000.0).abs() < 0.5, "the ground under the roof");
        let ny2 = normal_y(|x, _| Some(1000.0 + 0.4 * x), 0.0, 0.0, 50.0);
        assert!((ny2 as f64 - ny).abs() < 1e-3);
    }

    /// The table matches the exported models where they are on this machine: a centre-origin model reaches about its foot
    /// below its origin (static parts; the animated ones are checked by their own modes).
    #[test]
    fn foot_table_matches_the_models() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        for (path, want) in [("actors/boo/boo_geo.glb", BOO_FOOT), ("actors/heart/heart_geo.glb", HEART_FOOT), ("actors/star/star_geo.glb", STAR_FOOT), ("actors/mushroom_1up/mushroom_1up_geo.glb", MUSHROOM_FOOT), ("actors/bowling_ball/bowling_ball_geo.glb", IRON_BALL_FOOT), ("actors/chain_chomp/chain_chomp_geo.glb", CHOMP_FOOT), ("actors/coin/yellow_coin_geo.glb", 0.0), ("actors/thwomp/thwomp_geo.glb", 0.0)] {
            let p = root.join(path);
            if !p.is_file() {
                eprintln!("skipped: {} not on this machine", p.display());
                continue;
            }
            let (lo, _) = crate::zelda::glb::bounds(&p).expect("bounds");
            assert!((-lo[1] as f32 - want).abs() < 6.0, "{path}: reaches {} below its origin, table says {want}", -lo[1]);
            assert_eq!(foot(path), want);
        }
    }
}
