//! Real-world arenas: the `Ground` seam on a small hand-made `CollisionWorld` (flat plane, a wall,
//! a ramp, a lava patch, a pit) and the validity of every shipped [`Region`] on its real course
//! (skipped, not failed, when the imported course data is not on this machine).
use super::brawl_arena::*;
use super::brawl_core::*;
use super::brawl_region::tests::course_world;
use super::brawl_region::*;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use std::sync::Arc;

fn idle() -> [Input; MAXF] {
    [Input::default(); MAXF]
}

/// 4000 x 4000 floor at y = 1000 with: a wall across x = 500 (z -1500..1500, 300 high, facing -x), a ramp rising
/// from y 1000 to 1200 over x -1400..-1000 (z -300..300, cut out of the plane), a lava patch at (0, 900) and a pit
/// at (0, -900).
fn fixture() -> Arc<CollisionWorld> {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let h = 1000.0;
    let quad = |w: &mut CollisionWorld, x0: f64, z0: f64, x1: f64, z1: f64, o: &SurfaceOptions| {
        w.add_triangle([x0, h, z0], [x1, h, z1], [x1, h, z0], o);
        w.add_triangle([x0, h, z0], [x0, h, z1], [x1, h, z1], o);
    };
    // The plane in pieces so the pit and the ramp are holes.
    quad(&mut w, -2000.0, -2000.0, 2000.0, -1000.0, &o);
    quad(&mut w, -2000.0, -1000.0, -200.0, -800.0, &o);
    quad(&mut w, 200.0, -1000.0, 2000.0, -800.0, &o);
    quad(&mut w, -2000.0, -800.0, 2000.0, -300.0, &o);
    quad(&mut w, -2000.0, -300.0, -1400.0, 300.0, &o);
    quad(&mut w, -1000.0, -300.0, 2000.0, 300.0, &o);
    quad(&mut w, -2000.0, 300.0, 2000.0, 800.0, &o);
    quad(&mut w, -2000.0, 800.0, -200.0, 1000.0, &o);
    quad(&mut w, 200.0, 800.0, 2000.0, 1000.0, &o);
    quad(&mut w, -2000.0, 1000.0, 2000.0, 2000.0, &o);
    let lava = SurfaceOptions { kind: jumpman_sim::surfaces::SURFACE_BURNING, ..Default::default() };
    quad(&mut w, -200.0, 800.0, 200.0, 1000.0, &lava);
    // Ramp: 200 up over 400 (about 27 degrees).
    w.add_triangle([-1400.0, h, -300.0], [-1000.0, h + 200.0, 300.0], [-1000.0, h + 200.0, -300.0], &o);
    w.add_triangle([-1400.0, h, -300.0], [-1400.0, h, 300.0], [-1000.0, h + 200.0, 300.0], &o);
    // Wall facing -x.
    w.add_triangle([500.0, h, -1500.0], [500.0, h + 300.0, 1500.0], [500.0, h + 300.0, -1500.0], &o);
    w.add_triangle([500.0, h, -1500.0], [500.0, h, 1500.0], [500.0, h + 300.0, 1500.0], &o);
    Arc::new(w)
}

fn ground() -> WorldGround {
    WorldGround::new(fixture(), [0.0, 3000.0, 0.0])
}

/// A region on the fixture (arena-local y = 0 is the fixture floor).
fn fixture_region() -> Region {
    Region {
        key: "fixture",
        name: "Fixture",
        blurb: "",
        course: "fixture",
        centre: [0.0, 3000.0, 0.0],
        floor: Some(1000.0),
        half: [450.0, 700.0],
        void_y: -300.0,
        pits: &[],
        spawns: [[-300.0, -500.0], [300.0, 500.0], [300.0, -500.0], [-300.0, 500.0]],
        gems: &[[-300.0, 0.0], [300.0, 0.0], [0.0, 300.0], [0.0, -300.0], [-100.0, 100.0], [100.0, -100.0]],
        items: &[[-350.0, -100.0], [350.0, 100.0], [0.0, 400.0], [0.0, -400.0], [-250.0, 250.0], [250.0, -250.0]],
    }
}

fn arena() -> Arena {
    fixture_region().build(fixture())
}

#[test]
fn floor_reads_the_plane_and_ignores_surfaces_above_the_limit() {
    let mut g = ground();
    g.origin[1] = 1000.0;
    assert_eq!(g.floor(0.0, 0.0, 100.0), Some(0.0));
    assert_eq!(g.floor(0.0, 0.0, 0.0), Some(0.0));
    assert_eq!(g.floor(0.0, 0.0, -1.0), None, "a floor above the limit does not count");
    assert_eq!(g.floor(0.0, 900.0 - 5000.0, 100.0), None, "off the edge of the world there is nothing");
}

#[test]
fn ramp_height_follows_the_slope_and_ledge_limit_is_respected() {
    let mut g = ground();
    g.origin[1] = 1000.0;
    // x = -1200 is halfway up the ramp.
    let mid = g.floor(-1200.0, 0.0, 500.0).unwrap();
    assert!((mid - 100.0).abs() < 2.0, "{mid}");
    // The plane next to the ramp top is at 0; the ramp top at 200 only counts when the limit allows it.
    assert!(g.floor(-1010.0, 0.0, 500.0).unwrap() > 180.0);
    assert!(g.standable(-1200.0, mid, 0.0), "27 degrees is walkable");
}

#[test]
fn walls_push_a_standing_circle_back_and_a_jumper_over_them_is_free() {
    let mut g = ground();
    g.origin[1] = 1000.0;
    let mut p = [480.0, 0.0, 0.0];
    g.push(&mut p, 40.0);
    assert!(p[0] <= 461.0 && p[0] > 440.0, "pushed back to the near face: {p:?}");
    let mut high = [490.0, 330.0, 0.0];
    g.push(&mut high, 40.0);
    assert_eq!(high[0], 490.0, "above the wall nothing blocks");
    let mut away = [100.0, 0.0, 0.0];
    g.push(&mut away, 40.0);
    assert_eq!(away, [100.0, 0.0, 0.0]);
}

#[test]
fn lava_and_pits_are_not_standable_ground() {
    let a = arena();
    let g = a.ground.clone().unwrap();
    assert!(g.floor(0.0, 900.0, 500.0).is_some(), "lava is a floor to fall onto");
    assert!(!g.standable(0.0, 0.0, 900.0), "but nothing may rest on it");
    assert!(g.floor(0.0, -900.0, 500.0).is_none(), "the pit has no floor");
    assert!(!a.valid_spot([0.0, 0.0, -900.0]));
    assert!(a.valid_spot([0.0, 0.0, 0.0]));
    // Lava and pit are out of the rim anyway (bounds z 700): a spot just inside the rim.
    assert!(!a.valid_spot([0.0, 0.0, 800.0]), "outside the rim");
}

#[test]
fn arena_support_push_out_and_bounds_go_through_the_ground() {
    let a = arena();
    assert!(a.ground.is_some() && a.solids.is_empty());
    assert_eq!(a.support(0.0, 0.0, 34.0).map(|s| s.0), Some(0.0));
    let mut p = [430.0, 0.0, 0.0];
    a.push_out(&mut p, 30.0);
    assert!(p[0] < 421.0, "wall at 500 minus radius and the 450 rim: {p:?}");
    let mut q = [0.0, 0.0, 690.0];
    a.push_out(&mut q, 30.0);
    assert!(q[2] <= 670.0, "rim at 700: {q:?}");
    assert!(a.over_floor(0.0, 0.0) && !a.over_floor(0.0, 900.0), "beyond the rim is outside");
}

#[test]
fn nearest_floor_point_and_respawn_land_on_standable_ground() {
    let a = arena();
    // Over the pit.
    let q = a.nearest_floor_point(0.0, -600.0, 30.0);
    assert!(a.support(q[0], q[1], 500.0).is_some_and(|(top, _)| a.valid_spot([q[0], top, q[1]])), "{q:?}");
    let r = a.respawn_spot([0.0, 0.0, -690.0]);
    assert!(a.valid_spot(r), "{r:?}");
    let mut rng = Rng::new(3);
    for _ in 0..50 {
        let s = a.random_drop_spot(&mut rng);
        assert!(a.valid_spot(s), "{s:?}");
    }
}

#[test]
fn region_spots_all_have_floor_and_are_spread() {
    let a = arena();
    for s in a.spawns {
        let (top, _) = a.support(s[0], s[1], 500.0).expect("spawn floor");
        assert!(a.valid_spot([s[0], top, s[1]]), "{s:?}");
    }
    for (k, g) in a.gem_spots.iter().chain(a.item_spots.iter()).enumerate() {
        assert!(a.valid_spot(*g), "spot {k} {g:?}");
    }
    for i in 0..4 {
        for j in i + 1..4 {
            assert!(dist2(a.spawns[i], a.spawns[j]) > 300.0);
        }
    }
}

fn run_bots(a: Arena, seed: u64, bots: usize, max_ticks: usize) -> (Brawl, MatchLog) {
    let cfg = Config { seed, humans: 0, bots, diff: Diff::Normal, arena: ArenaKind::Field, rounds_to_win: 2, monsters: Monsters::Few, ..Default::default() };
    let mut b = Brawl::with_arena(cfg, Some(a));
    let mut log = MatchLog::default();
    let mut last = [[0.0f32; 2]; MAXF];
    let mut still = [0usize; MAXF];
    for t in 0..max_ticks {
        b.tick(&idle());
        for e in &b.ev {
            match e {
                Ev::Gem { .. } => log.gems += 1,
                Ev::Grab(_) => log.grabs += 1,
                Ev::Fell(_) => log.fell += 1,
                _ => {}
            }
        }
        for i in 0..b.n {
            let f = &b.f[i];
            assert!(f.pos.iter().all(|v| v.is_finite()), "tick {t}: fighter {i} {:?}", f.pos);
            assert!(!f.alive || f.pos[1] > b.arena.void_y - 400.0, "tick {t}: fighter {i} fell out of the world: {:?}", f.pos);
            if b.phase == Phase::Fight && f.alive && f.grounded {
                let p = [f.pos[0], f.pos[2]];
                if dist2(p, last[i]) < 1.0 {
                    still[i] += 1;
                    if still[i] > log.longest_still {
                        log.longest_still = still[i];
                        log.stuck_at = [f.pos[0], f.pos[1], f.pos[2]];
                        log.stuck_goal = b.brains[i].map(|x| format!("{:?}", x.goal)).unwrap_or_default();
                    }
                } else {
                    still[i] = 0;
                }
                last[i] = p;
            }
        }
        if b.phase == Phase::MatchEnd {
            log.ended = true;
            break;
        }
    }
    log.ticks = b.total_ticks as usize;
    (b, log)
}

#[derive(Default, Debug)]
struct MatchLog {
    gems: u32,
    grabs: u32,
    fell: u32,
    longest_still: usize,
    stuck_at: [f32; 3],
    stuck_goal: String,
    ended: bool,
    ticks: usize,
}

#[test]
fn bots_play_a_whole_match_on_the_fixture_without_falling_through_or_freezing() {
    let (b, log) = run_bots(arena(), 11, 3, 30 * 60 * 6);
    println!("{log:?} winner {}", b.match_winner);
    assert!(log.ended, "match must end: {log:?}");
    assert!(log.gems > 0 && log.grabs > 0, "gems and pickups get collected: {log:?}");
    assert!(log.longest_still < 30 * 12, "nobody stands frozen for long: {log:?}");
}

#[test]
fn a_region_match_is_deterministic() {
    let d = |seed| {
        let mut b = Brawl::with_arena(Config { seed, humans: 0, bots: 3, arena: ArenaKind::Field, ..Default::default() }, Some(arena()));
        for _ in 0..900 {
            b.tick(&idle());
        }
        b.digest()
    };
    assert_eq!(d(4), d(4));
    assert_ne!(d(4), d(5));
}

#[test]
fn rounds_restart_on_the_same_ground() {
    let (b, _) = run_bots(arena(), 7, 2, 30 * 100 * 3);
    assert!(b.arena.ground.is_some(), "later rounds keep the real ground");
    assert_eq!(b.arena.kind, ArenaKind::Field);
}

// ---------------------------------------------------------------------------------------------
// The shipped regions, on the real courses.
// ---------------------------------------------------------------------------------------------

#[test]
fn every_region_has_floor_for_spawns_gems_and_items() {
    for r in REGIONS {
        let Some(world) = course_world(r.course) else { continue };
        let a = r.build(world);
        for s in a.spawns {
            let (top, _) = a.support(s[0], s[1], 600.0).unwrap_or_else(|| panic!("{}: spawn {s:?} has no floor", r.key));
            assert!(a.valid_spot([s[0], top, s[1]]), "{}: spawn {s:?} is not standable ground", r.key);
        }
        for (k, g) in a.gem_spots.iter().enumerate() {
            assert!(a.valid_spot(*g), "{}: gem spot {k} {g:?}", r.key);
        }
        for (k, g) in a.item_spots.iter().enumerate() {
            assert!(a.valid_spot(*g), "{}: item spot {k} {g:?}", r.key);
        }
        for i in 0..4 {
            for j in i + 1..4 {
                assert!(dist2(a.spawns[i], a.spawns[j]) > 500.0, "{}: spawns {i} and {j} too close", r.key);
            }
        }
    }
}

#[test]
fn every_region_is_connected_from_every_spawn_to_every_gem() {
    for r in REGIONS {
        let Some(world) = course_world(r.course) else { continue };
        let a = r.build(world);
        // Flood fill over standable cells (jumps clear up to 150): all spots must be one walkable region.
        let cell = 40.0;
        let (nx, nz) = ((a.half[0] * 2.0 / cell) as i32 + 1, (a.half[1] * 2.0 / cell) as i32 + 1);
        let at = |ix: i32, iz: i32| [-a.half[0] + ix as f32 * cell, -a.half[1] + iz as f32 * cell];
        let idx = |ix: i32, iz: i32| (iz * nx + ix) as usize;
        let mut height = vec![f32::NAN; (nx * nz) as usize];
        for iz in 0..nz {
            for ix in 0..nx {
                let p = at(ix, iz);
                if let Some((top, _)) = a.support(p[0], p[1], 900.0) {
                    if a.valid_spot([p[0], top, p[1]]) {
                        height[idx(ix, iz)] = top;
                    }
                }
            }
        }
        let cell_of = |x: f32, z: f32| (((x + a.half[0]) / cell).round() as i32, ((z + a.half[1]) / cell).round() as i32);
        let s0 = a.spawns[0];
        let (sx, sz) = cell_of(s0[0], s0[1]);
        let mut seen = vec![false; height.len()];
        let mut stack = vec![(sx, sz)];
        seen[idx(sx, sz)] = true;
        while let Some((x, z)) = stack.pop() {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                let (qx, qz) = (x + dx, z + dz);
                if qx < 0 || qz < 0 || qx >= nx || qz >= nz || seen[idx(qx, qz)] {
                    continue;
                }
                let (h0, h1) = (height[idx(x, z)], height[idx(qx, qz)]);
                if h1.is_nan() || (h1 - h0).abs() > 120.0 {
                    continue;
                }
                // The step must not run into a wall taller than a jump.
                let mut pp = [at(x, z)[0], h0, at(x, z)[1]];
                let target = at(qx, qz);
                pp[0] = target[0];
                pp[2] = target[1];
                let mut moved = pp;
                a.push_out(&mut moved, 30.0);
                if (moved[0] - pp[0]).hypot(moved[2] - pp[2]) > 12.0 && h1 - h0 > 34.0 {
                    continue;
                }
                seen[idx(qx, qz)] = true;
                stack.push((qx, qz));
            }
        }
        for (what, spot) in a.spawns.iter().map(|s| ("spawn", [s[0], 0.0, s[1]])).chain(a.gem_spots.iter().map(|g| ("gem", *g))).chain(a.item_spots.iter().map(|g| ("item", *g))) {
            let (qx, qz) = cell_of(spot[0], spot[2]);
            assert!(seen[idx(qx, qz)], "{}: {what} at {spot:?} cannot be walked to from spawn 0", r.key);
        }
    }
}

#[test]
fn every_region_plays_bots_only_matches_to_the_end() {
    for r in REGIONS {
        let Some(world) = course_world(r.course) else { continue };
        let mut grabs = 0;
        for seed in [1u64, 2, 3] {
            let (b, log) = run_bots(r.build(world.clone()), seed, 3, 30 * 100 * 5);
            println!("{} seed {seed}: {log:?} winner {} ({} ticks)", r.key, b.match_winner, log.ticks);
            assert!(log.ended, "{} seed {seed}: the match must finish: {log:?}", r.key);
            assert!(log.gems > 0, "{} seed {seed}: gems must be collected: {log:?}", r.key);
            grabs += log.grabs;
            assert!(log.longest_still < 30 * 15, "{} seed {seed}: a bot froze for {} ticks: {log:?}", r.key, log.longest_still);
        }
        assert!(grabs > 0, "{}: pickups must be collected over three matches", r.key);
    }
}

#[test]
#[ignore]
fn debug_items() {
    let Some(world) = course_world("bob") else { return };
    let cfg = Config { seed: 1, humans: 0, bots: 3, diff: Diff::Hard, arena: ArenaKind::Field, rounds_to_win: 1, ..Default::default() };
    let mut b = Brawl::with_arena(cfg, Some(REGIONS[0].build(world)));
    for t in 0..400 {
        b.tick(&idle());
        if t % 100 == 99 {
            for it in b.items.iter().filter(|i| i.alive) {
                println!("t{t} item {:?} {:?} state {:?} grounded {}", it.kind, it.pos, it.state, it.grounded);
            }
            for i in 0..b.n {
                println!("t{t} f{i} {:?} goal {:?} hp {}", b.f[i].pos, b.brains[i].map(|x| x.goal), b.f[i].hp);
            }
        }
    }
}

#[test]
fn every_region_digest_is_stable() {
    for r in REGIONS {
        let Some(world) = course_world(r.course) else { continue };
        let d = |w: Arc<CollisionWorld>| {
            let mut b = Brawl::with_arena(Config { seed: 9, humans: 0, bots: 3, arena: ArenaKind::Field, monsters: Monsters::Few, ..Default::default() }, Some(r.build(w)));
            for _ in 0..1500 {
                b.tick(&idle());
            }
            b.digest()
        };
        assert_eq!(d(world.clone()), d(world), "{}", r.key);
    }
}
