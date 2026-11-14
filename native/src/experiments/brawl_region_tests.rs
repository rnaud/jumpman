//! Real-world arenas: the `Ground` seam on a small hand-made `CollisionWorld` (flat plane, a wall,
//! a ramp, a lava patch, a pit) and the validity of every shipped [`Region`] on its real course
//! (skipped, not failed, when the imported course data is not on this machine).
use super::brawl_arena::*;
use super::brawl_core::*;
use super::brawl_region::tests::course_world;
use super::brawl_region::*;
use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
use std::sync::Arc;

#[path = "brawl_hazard_tests.rs"]
mod hazard_tests;

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
        barrage: 0,
        hazards: &[],
        gusts: false,
        view_lift: 0.0,
        view_yaw: 0.0,
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
    let mut bad: Vec<String> = vec![];
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
            if !seen[idx(qx, qz)] {
                bad.push(format!("{}: {what} at {spot:?} cannot be walked to from spawn 0", r.key));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
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

// ---------------------------------------------------------------------------------------------
// Feel and hazards on real ground (the fixture has a wall at x = 500 and a barrage option).
// ---------------------------------------------------------------------------------------------

/// A wider fixture region so the wall at x = 500 is inside the rim.
fn wide_region(barrage: i32) -> Region {
    Region { half: [900.0, 700.0], barrage, ..fixture_region() }
}

fn duel_on(region: Region, seed: u64) -> Brawl {
    let cfg = Config { seed, humans: 2, bots: 0, arena: ArenaKind::Field, ..Default::default() };
    let mut b = Brawl::with_arena(cfg, Some(region.build(fixture())));
    for _ in 0..INTRO_TICKS + 2 {
        b.tick(&idle());
    }
    for it in b.items.iter_mut() {
        it.alive = false;
    }
    b.item_timer = 100_000;
    for g in b.gems.iter_mut() {
        g.state = GemState::Spent;
    }
    b
}

fn put(b: &mut Brawl, i: usize, x: f32, z: f32) {
    let top = b.arena.support(x, z, 600.0).map_or(0.0, |t| t.0);
    let f = &mut b.f[i];
    f.pos = [x, top, z];
    f.vel = [0.0; 3];
    f.grounded = true;
    f.act = Act::Idle;
    f.inv = 0;
}

#[test]
fn a_solid_blow_freezes_both_fighters_for_a_beat() {
    let mut b = duel_on(wide_region(0), 3);
    put(&mut b, 0, -200.0, 0.0);
    put(&mut b, 1, -100.0, 0.0);
    assert!(b.hit(1, 0, 9.0, [1.0, 0.0], 300.0, 100.0));
    assert!(b.f[1].hitstop >= 2 && b.f[0].hitstop >= 1, "victim {} attacker {}", b.f[1].hitstop, b.f[0].hitstop);
    let (p0, p1) = (b.f[0].pos, b.f[1].pos);
    b.tick(&idle());
    assert_eq!((b.f[0].pos, b.f[1].pos), (p0, p1), "frozen fighters do not move");
    for _ in 0..8 {
        b.tick(&idle());
    }
    assert!(b.f[1].pos[0] > p1[0] + 5.0, "then the knockback plays out");
}

#[test]
fn blows_in_a_row_build_a_combo_that_ends_with_the_window() {
    let mut b = duel_on(wide_region(0), 3);
    put(&mut b, 0, -200.0, 0.0);
    put(&mut b, 1, -100.0, 0.0);
    let mut dealt = vec![];
    for k in 0..5 {
        b.f[1].inv = 0;
        let before = b.f[1].hp;
        b.hit(1, 0, 6.0, [1.0, 0.0], 100.0, 0.0);
        dealt.push(before - b.f[1].hp);
        assert_eq!(b.f[0].combo, k + 1);
    }
    assert!(dealt.windows(2).all(|w| w[1] > w[0] - 1e-4) && dealt[4] > dealt[0] * 1.3, "{dealt:?}");
    assert!(b.ev.iter().any(|e| matches!(e, Ev::Combo { n, .. } if *n >= 2)));
    for _ in 0..COMBO_WINDOW + 2 {
        b.tick(&idle());
    }
    assert_eq!(b.f[0].combo, 0, "the chain is over");
}

#[test]
fn a_launched_fighter_splats_against_a_wall() {
    let mut b = duel_on(wide_region(0), 3);
    put(&mut b, 0, 100.0, 0.0);
    put(&mut b, 1, 400.0, 0.0);
    let hp = b.f[1].hp;
    assert!(b.hit(1, 0, 6.0, [1.0, 0.0], 900.0, 120.0));
    let mut splat = false;
    for _ in 0..40 {
        b.tick(&idle());
        splat |= b.ev.iter().any(|e| matches!(e, Ev::Splat { id: 1, .. }));
    }
    assert!(splat, "slammed into the wall at x = 500");
    assert!(hp - b.f[1].hp > 6.0 + 5.0, "the splat adds damage: {}", hp - b.f[1].hp);
    assert!(b.f[1].pos[0] < 500.0, "and the wall holds");
}

#[test]
fn sliding_along_open_ground_never_splats() {
    let mut b = duel_on(wide_region(0), 3);
    put(&mut b, 0, -600.0, 0.0);
    put(&mut b, 1, -500.0, 0.0);
    b.hit(1, 0, 6.0, [1.0, 0.0], 900.0, 120.0);
    for _ in 0..40 {
        b.tick(&idle());
        assert!(!b.ev.iter().any(|e| matches!(e, Ev::Splat { .. })));
    }
}

#[test]
fn boots_and_bubbles_are_picked_up_by_touch() {
    let mut b = duel_on(wide_region(0), 3);
    put(&mut b, 0, -200.0, 0.0);
    put(&mut b, 1, 300.0, 0.0);
    b.spawn_item(ItemKind::Boots, [-200.0, 0.0, 0.0], false);
    b.spawn_item(ItemKind::Shield, [-200.0, 0.0, 30.0], false);
    for _ in 0..3 {
        b.tick(&idle());
    }
    assert!(b.f[0].haste > 0 && b.f[0].shield > 0, "haste {} shield {}", b.f[0].haste, b.f[0].shield);
    let plain = {
        let mut c = duel_on(wide_region(0), 3);
        put(&mut c, 0, -200.0, 0.0);
        put(&mut c, 1, 300.0, 0.0);
        let hp = c.f[0].hp;
        c.hit(0, 1, 10.0, [1.0, 0.0], 0.0, 0.0);
        hp - c.f[0].hp
    };
    let hp = b.f[0].hp;
    b.hit(0, 1, 10.0, [1.0, 0.0], 0.0, 0.0);
    assert!(((hp - b.f[0].hp) / plain - 0.4).abs() < 0.02, "the bubble cuts the damage to 40 %");
    // Boots really are faster.
    let run = |haste: bool| {
        let mut c = duel_on(wide_region(0), 3);
        put(&mut c, 0, -800.0, -600.0);
        put(&mut c, 1, 800.0, 600.0);
        c.f[0].haste = if haste { 200 } else { 0 };
        let mut inp = idle();
        inp[0].mv = [1.0, 0.0];
        for _ in 0..30 {
            c.tick(&inp);
        }
        c.f[0].pos[0]
    };
    assert!(run(true) > run(false) + 80.0);
}

#[test]
fn three_gems_blast_nearby_foes_away() {
    let mut b = duel_on(wide_region(0), 3);
    put(&mut b, 0, -200.0, 0.0);
    put(&mut b, 1, -100.0, 0.0);
    let hp = b.f[1].hp;
    b.give_gems(0, 3);
    assert!(b.f[0].powered());
    assert!(hp - b.f[1].hp >= 8.0 && b.f[1].vel[0] > 300.0, "hp lost {}, vx {}", hp - b.f[1].hp, b.f[1].vel[0]);
    assert!(b.ev.iter().any(|e| matches!(e, Ev::Nova { .. })));
}

#[test]
fn shells_rain_after_the_opening_seconds_and_the_rim_closes_at_the_bell() {
    let mut b = duel_on(wide_region(45), 8);
    let mut first = None;
    let mut booms = 0;
    for t in 0..30 * 60 {
        b.tick(&idle());
        for e in &b.ev {
            match e {
                Ev::Barrage(_) => {
                    first.get_or_insert(t);
                }
                Ev::Boom { .. } => booms += 1,
                _ => {}
            }
        }
        for i in 0..2 {
            b.f[i].hp = b.f[i].max_hp;
        }
    }
    let first = first.expect("shells fall");
    assert!(first as f32 / 30.0 > 8.0, "not in the opening seconds: tick {first}");
    assert!(booms >= 5, "and they explode: {booms}");
    // The last half minute: the rim closes in on the real ground, down to 55 %.
    let before = b.arena.bounds.unwrap();
    b.round_t = RIM_CLOSE_SECS;
    for _ in 0..30 * 28 {
        b.tick(&idle());
        for i in 0..2 {
            b.f[i].hp = b.f[i].max_hp;
        }
    }
    let after = b.arena.bounds.unwrap();
    assert!(after[0] < before[0] - 100.0 && after[0] >= 900.0 * 0.55 - 1.0, "{before:?} -> {after:?}");
}

#[test]
fn a_calm_region_has_no_shells() {
    let mut b = duel_on(wide_region(0), 8);
    for _ in 0..30 * 40 {
        b.tick(&idle());
        assert!(!b.ev.iter().any(|e| matches!(e, Ev::Barrage(_))));
        for i in 0..2 {
            b.f[i].hp = b.f[i].max_hp;
        }
    }
}

#[test]
fn a_gauntlet_walks_through_the_regions_of_its_course() {
    let other = Region { centre: [300.0, 3000.0, 200.0], ..wide_region(0) };
    let (a, c) = (wide_region(0).build(fixture()), other.build(fixture()));
    assert_ne!(a.origin(), c.origin());
    let cfg = Config { seed: 4, mode: Mode::Gauntlet, arena: ArenaKind::Field, humans: 1, bots: 0, ..Default::default() };
    let mut b = Brawl::with_arenas(cfg, Some(a.clone()), vec![a.clone(), c.clone()]);
    assert_eq!(b.arena.origin(), a.origin());
    let mut origins = vec![b.arena.origin()];
    for _ in 0..30 * 60 * 5 {
        b.tick(&idle());
        if b.phase == Phase::Fight {
            for j in 1..b.n {
                b.f[j].inv = 0;
                b.hit(j, 0, 1e6, [1.0, 0.0], 0.0, 0.0);
            }
        }
        if origins.last() != Some(&b.arena.origin()) {
            origins.push(b.arena.origin());
        }
        if origins.len() >= 3 || b.phase == Phase::MatchEnd {
            break;
        }
    }
    assert_eq!(origins.len(), 3, "stage 1 plays on the second region and stage 2 loops back: {origins:?}");
    assert_eq!(origins[0], origins[2]);
    assert!(b.arena.ground.is_some());
}

#[test]
fn no_menu_entry_is_a_box_arena() {
    assert!(arena_count() >= 14, "{} regions", arena_count());
    for k in 0..arena_count() as u8 {
        let r = region_of(k).expect("every menu entry is a region of a real course");
        assert!(!r.course.is_empty() && !r.name.is_empty() && !r.blurb.is_empty());
        let list = course_regions(k);
        assert_eq!(list[0].key, r.key, "the picked region leads its Gauntlet");
        assert!(list.iter().all(|x| x.course == r.course));
    }
    let keys: std::collections::HashSet<_> = REGIONS.iter().map(|r| r.key).collect();
    assert_eq!(keys.len(), REGIONS.len(), "region keys are unique");
    assert!(REGIONS.iter().any(|r| r.course == "wf") && REGIONS.iter().any(|r| r.course == "lon_lon_ranch"));
}

/// Cost of one 30 Hz core tick (the real bob summit: four bots, many monsters, shells, items, gems). Release builds only:
/// `cargo test --release bench_core_tick -- --ignored --nocapture`. An iPhone is several times slower than this Mac per core, so
/// the budget (a tick must stay under 1.5 ms here, well inside the 33 ms a 30 Hz tick may take) keeps a wide margin.
#[test]
#[ignore]
fn bench_core_tick() {
    let Some(world) = course_world("bob") else { return };
    let region = &REGIONS[Region::parse("bob").unwrap()];
    let mut worst = 0.0f64;
    let mut total = 0.0f64;
    let mut ticks = 0u32;
    for seed in 0..6u64 {
        let cfg = Config { seed, humans: 0, bots: 3, diff: Diff::Hard, arena: ArenaKind::Field, monsters: Monsters::Many, rounds_to_win: 3, ..Default::default() };
        let mut b = Brawl::with_arena(cfg, Some(region.build(world.clone())));
        for _ in 0..30 * 90 {
            let t = std::time::Instant::now();
            b.tick(&idle());
            let us = t.elapsed().as_secs_f64() * 1e6;
            total += us;
            worst = worst.max(us);
            ticks += 1;
            assert!(b.ev.len() <= 128, "event list stays small: {}", b.ev.len());
            if b.phase == Phase::MatchEnd {
                break;
            }
        }
    }
    println!("CORE TICK over {ticks} ticks: mean {:.0} us, worst {worst:.0} us", total / ticks as f64);
    assert!(total / (ticks as f64) < 1500.0, "mean tick {:.0} us", total / ticks as f64);
}

/// What `MINIGAMES_RESEARCH.md` asks of Gem Clash, measured over bots-only matches on one region (see `MINIGAMES_AUDIT_3.md`).
#[derive(Default, Debug, Clone)]
struct PaceLog {
    rounds: u32,
    /// Ticks from the start of each round's fight to its first landed blow on a fighter.
    first_contact: Vec<u32>,
    /// Per fighter-tick in a fight: distance to the nearest living foe, in seconds of running at the fighter's speed.
    foe_secs: Vec<f32>,
    /// Fighter-ticks in the first 30 s of a round with a throwable / weapon within one second of running, and all of them.
    item_near: u32,
    item_ticks: u32,
    /// Rounds in which a gem went from one fighter's hands to another's.
    gem_swaps: u32,
    /// Power forms: ticks each one lasted, and whether its owner won that round.
    power_ticks: Vec<u32>,
    power_won: u32,
    power_rounds: u32,
    /// Ticks of each round's fight, and of each whole match.
    round_ticks: Vec<u32>,
    match_ticks: Vec<u32>,
    hits: u32,
    /// Knockouts of fighters, and those landed by a fighter in the power form.
    kos: u32,
    power_kos: u32,
}

impl PaceLog {
    fn add(&mut self, l: &PaceLog) {
        self.rounds += l.rounds;
        self.first_contact.extend(&l.first_contact);
        self.foe_secs.extend(&l.foe_secs);
        self.item_near += l.item_near;
        self.item_ticks += l.item_ticks;
        self.gem_swaps += l.gem_swaps;
        self.power_ticks.extend(&l.power_ticks);
        self.power_won += l.power_won;
        self.power_rounds += l.power_rounds;
        self.round_ticks.extend(&l.round_ticks);
        self.match_ticks.extend(&l.match_ticks);
        self.hits += l.hits;
        self.kos += l.kos;
        self.power_kos += l.power_kos;
    }
    fn secs(v: &[u32]) -> Vec<f32> {
        v.iter().map(|t| *t as f32 / 30.0).collect()
    }
}

fn pct(v: &[f32], q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[((s.len() - 1) as f32 * q).round() as usize]
}

fn measure_pace(a: Arena, seed: u64, bots: usize, log: &mut PaceLog) {
    let cfg = Config { seed, humans: 0, bots, diff: Diff::Normal, arena: ArenaKind::Field, rounds_to_win: 2, monsters: Monsters::Few, ..Default::default() };
    let mut b = Brawl::with_arena(cfg, Some(a));
    let mut fight_t = 0u32;
    let mut contact = false;
    let mut holder = [None::<u8>; GEMS];
    let mut swapped = false;
    let mut power_at = [None::<u32>; MAXF];
    let mut powered_round = [false; MAXF];
    for _ in 0..30 * 60 * 8 {
        b.tick(&idle());
        for k in 0..b.ev.len() {
            match b.ev[k] {
                Ev::Hit { victim, .. } if b.phase == Phase::Fight && victim < b.n => {
                    log.hits += 1;
                    if !contact {
                        contact = true;
                        log.first_contact.push(fight_t);
                    }
                }
                Ev::Ko { id, .. } if id < b.n => {
                    log.kos += 1;
                    let by = b.f[id].last_hit_by;
                    log.power_kos += (by >= 0 && b.f[by as usize].powered()) as u32;
                }
                Ev::Transform { id, .. } => {
                    power_at[id] = Some(b.tick);
                    powered_round[id] = true;
                }
                Ev::PowerEnd(id) => {
                    if let Some(t) = power_at[id].take() {
                        log.power_ticks.push(b.tick - t);
                    }
                }
                Ev::RoundEnd { winner } => {
                    log.rounds += 1;
                    log.round_ticks.push(fight_t);
                    log.gem_swaps += swapped as u32;
                    for (i, p) in powered_round.iter().enumerate() {
                        if *p {
                            log.power_rounds += 1;
                            log.power_won += (winner == i as i8) as u32;
                        }
                    }
                    fight_t = 0;
                    contact = false;
                    swapped = false;
                    holder = [None; GEMS];
                    powered_round = [false; MAXF];
                    power_at = [None; MAXF];
                }
                _ => {}
            }
        }
        if b.phase == Phase::Fight {
            for (g, gem) in b.gems.iter().enumerate() {
                match gem.state {
                    GemState::Held(h) => {
                        if holder[g].is_some_and(|o| o != h) {
                            swapped = true;
                        }
                        holder[g] = Some(h);
                    }
                    GemState::Spent | GemState::Pending(_) => holder[g] = None,
                    GemState::Free => {}
                }
            }
            for i in 0..b.n {
                let f = &b.f[i];
                if !f.alive {
                    continue;
                }
                let speed = RUN * f.stats().speed;
                if let Some((_, d)) = b.nearest_foe(i) {
                    log.foe_secs.push(d / speed);
                }
                if fight_t < 30 * 30 {
                    log.item_ticks += 1;
                    let near = b.items.iter().any(|it| it.alive && !it.kind.pickup() && it.state == IState::Rest && (it.pos[0] - f.pos[0]).hypot(it.pos[2] - f.pos[2]) < speed);
                    log.item_near += near as u32;
                }
            }
            fight_t += 1;
        }
        if b.phase == Phase::MatchEnd {
            log.match_ticks.push(b.total_ticks);
            break;
        }
    }
}

fn pace_line(name: &str, log: &PaceLog) -> String {
    let fc = PaceLog::secs(&log.first_contact);
    let rt = PaceLog::secs(&log.round_ticks);
    let mt = PaceLog::secs(&log.match_ticks);
    let pw = PaceLog::secs(&log.power_ticks);
    format!(
        "PACE {name:>12}: rounds {:3} | first contact p50 {:.1}s p90 {:.1}s max {:.1}s | nearest foe p50 {:.2}s p95 {:.2}s max {:.2}s | prop within 1 s {:.0}% | gem-swap rounds {:.0}% | power forms {} p50 {:.1}s, owner won {}/{}, KOs by a powered fighter {}/{} | round p50 {:.0}s p90 {:.0}s | match p50 {:.0}s max {:.0}s | hits/round {:.1}",
        log.rounds,
        pct(&fc, 0.5),
        pct(&fc, 0.9),
        pct(&fc, 1.0),
        pct(&log.foe_secs, 0.5),
        pct(&log.foe_secs, 0.95),
        pct(&log.foe_secs, 1.0),
        100.0 * log.item_near as f32 / log.item_ticks.max(1) as f32,
        100.0 * log.gem_swaps as f32 / log.rounds.max(1) as f32,
        pw.len(),
        pct(&pw, 0.5),
        log.power_won,
        log.power_rounds,
        log.power_kos,
        log.kos,
        pct(&rt, 0.5),
        pct(&rt, 0.9),
        pct(&mt, 0.5),
        pct(&mt, 1.0),
        log.hits as f32 / log.rounds.max(1) as f32,
    )
}

/// The research checklist for Gem Clash, measured on every shipped region with Normal bots (1v1 and four-way):
/// `cargo test --release gem_clash_pace_report -- --nocapture` prints the table `MINIGAMES_AUDIT_3.md` quotes.
#[test]
fn gem_clash_pace_report() {
    let mut all1 = PaceLog::default();
    let mut all4 = PaceLog::default();
    for r in REGIONS {
        let Some(world) = course_world(r.course) else { continue };
        let mut l1 = PaceLog::default();
        let mut l4 = PaceLog::default();
        for seed in 0..4u64 {
            measure_pace(r.build(world.clone()), seed, 2, &mut l1);
            measure_pace(r.build(world.clone()), 50 + seed, 4, &mut l4);
        }
        println!("{}", pace_line(&format!("{} 1v1", r.key), &l1));
        println!("{}", pace_line(&format!("{} 4way", r.key), &l4));
        all1.add(&l1);
        all4.add(&l4);
    }
    if all1.rounds == 0 {
        return;
    }
    println!("{}", pace_line("ALL 1v1", &all1));
    println!("{}", pace_line("ALL 4way", &all4));
    // Pinned (MINIGAMES_AUDIT_3.md): dense fights, a prop at hand, gems that change hands, a power form near 8-15 s that
    // is a comeback rather than an automatic knockout, and matches well under five minutes.
    for (name, l) in [("1v1", &all1), ("4way", &all4)] {
        let fc = PaceLog::secs(&l.first_contact);
        assert!(pct(&fc, 0.5) <= 3.0, "{name}: first contact p50 {:.1}s", pct(&fc, 0.5));
        assert!(pct(&l.foe_secs, 0.95) <= 2.0, "{name}: nearest foe p95 {:.2}s", pct(&l.foe_secs, 0.95));
        assert!(l.item_near * 100 >= l.item_ticks * 75, "{name}: a prop within a second only {}/{}", l.item_near, l.item_ticks);
        assert!(l.gem_swaps * 100 >= l.rounds * 60, "{name}: gems changed hands in {}/{} rounds", l.gem_swaps, l.rounds);
        assert!(pct(&PaceLog::secs(&l.power_ticks), 0.5) >= 7.0, "{name}: the power form is too short");
        assert!(l.power_kos * 100 <= l.kos * 55, "{name}: the power form lands {}/{} knockouts", l.power_kos, l.kos);
        assert!(pct(&PaceLog::secs(&l.match_ticks), 1.0) <= 300.0, "{name}: a match ran over five minutes");
    }
}
