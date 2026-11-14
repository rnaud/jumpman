//! Gem Clash things stand on the ground (`brawl_hazard.rs`, `brawl_hazard_view.rs`, `brawl_view.rs`): on a tilted course the
//! Chain Chomp's feet follow the floor through its bob, rear-up, lunge and return, its ball (lifted by its radius, more on a
//! slope) never dips into the hill and its chain never runs under it; the centre-origin item models sit on their base; and
//! items pushed uphill by the wind ride up the floor. A child of `brawl_region_tests.rs`.
use super::*;
use bevy::prelude::Vec3;
use crate::experiments::brawl::hazard_view::{chain_link, look_model, LINK_R};
use crate::experiments::brawl_view::item_look;
use crate::experiments::ground_fit::{self, tests::slope_world, CHOMP_FOOT};

/// The tilt: 0.35 up per unit of x (about 19 degrees), arena-local y = 0 at x = 0.
const SLOPE: f32 = 0.35;

fn slope_arena(defs: Vec<HazardDef>, seed: u64) -> Brawl {
    let region = Region { hazards: Box::leak(defs.into_boxed_slice()), half: [700.0, 700.0], ..fixture_region() };
    let cfg = Config { seed, humans: 2, bots: 0, arena: ArenaKind::Field, ..Default::default() };
    let mut b = Brawl::with_arena(cfg, Some(region.build(slope_world(1000.0, SLOPE as f64, 3000.0))));
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
    b.round_t = ROUND_SECS - HAZARD_GRACE - 1.0;
    b
}

fn floor_at(b: &Brawl, x: f32, z: f32, near: f32) -> f32 {
    b.arena.support(x, z, near + 300.0).expect("floor").0
}

#[test]
fn the_chain_chomp_rests_on_a_slope_through_its_whole_attack() {
    let mut b = slope_arena(vec![HazardDef::new(Look::ChainChomp, [100.0, 0.0])], 7);
    b.f[1].inv = 60000;
    put(&mut b, 1, -600.0, 600.0);
    let r = CHOMP_SCALE * CHOMP_FOOT;
    let (_, scale, lift) = look_model(Look::ChainChomp);
    assert_eq!((scale, lift), (CHOMP_SCALE, r));
    let mut seen = [false; 4];
    for t in 0..30 * 40 {
        // Fighter 0 circles the post, uphill and downhill of it, inside its reach: it lunges both ways.
        let a = t as f32 * 0.02;
        let goal = [100.0 + a.cos() * 380.0, a.sin() * 380.0];
        let mut inp = idle();
        let p = b.f[0].pos;
        let d = [goal[0] - p[0], goal[1] - p[2]];
        let l = d[0].hypot(d[1]).max(1.0);
        inp[0].mv = if l > 30.0 { [d[0] / l, d[1] / l] } else { [0.0, 0.0] };
        b.f[0].hp = b.f[0].max_hp;
        if !b.f[0].alive {
            break;
        }
        b.tick(&inp);
        let h = b.arena.hazards[0];
        seen[h.st as usize] = true;
        let ground = floor_at(&b, h.pos[0], h.pos[2], h.pos[1]);
        // Feet: on the ground under it (a rear-up and the lunge hop it off a little, never into it).
        let above = h.pos[1] - ground;
        match h.st {
            HzState::Idle | HzState::Rest => assert!(above.abs() < 0.5, "tick {t} {:?}: feet {above:.1} off the floor", h.st),
            _ => assert!((-0.5..=41.0).contains(&above), "tick {t} {:?}: feet {above:.1} off the floor", h.st),
        }
        // The ball: its centre at least its radius from the tilted plane (y = SLOPE * x), and resting on it (not floating)
        // when its feet are down.
        let c = b.chomp_centre(h.pos);
        let gap = (c - SLOPE * h.pos[0]) / (1.0 + SLOPE * SLOPE).sqrt() - r;
        assert!(gap > -0.5, "tick {t} {:?}: ball {gap:.1} into the hill", h.st);
        if matches!(h.st, HzState::Idle | HzState::Rest) {
            assert!(gap < 1.5, "tick {t}: ball floats {gap:.1} over the hill");
        }
        // The chain from the post to the ball never runs under the hill.
        let anchor = Vec3::from(h.home) + Vec3::Y * 110.0;
        let head = Vec3::new(h.pos[0], c, h.pos[2]);
        let floor = |x: f32, z: f32| b.arena.support(x, z, anchor.y.max(head.y)).map(|s| s.0);
        for i in 0..6 {
            let q = chain_link(anchor, head, i, 6, h.spec().reach, floor);
            assert!(q.y - LINK_R >= SLOPE * q.x - 0.5, "tick {t}: link {i} at {q:?} under the hill");
        }
    }
    assert!(seen.iter().all(|s| *s), "the chomp went through every state: {seen:?}");
    // Its post stands on the ground too.
    let h = b.arena.hazards[0];
    assert!((h.home[1] - floor_at(&b, h.home[0], h.home[2], h.home[1])).abs() < 0.5);
}

#[test]
fn centre_origin_models_are_lifted_onto_their_base() {
    // Hazards: the iron ball and the Boo are centred on their origin (the Boo floats its sim height above the ground).
    for (look, path) in [(Look::IronBall, "bowling_ball_geo.glb"), (Look::Boo, "boo_geo.glb"), (Look::ChainChomp, "chain_chomp_geo.glb")] {
        let (_, scale, lift) = look_model(look);
        assert!((lift - ground_fit::rest_lift(path, scale)).abs() < 0.01, "{look:?}: lift {lift}");
    }
    // Items: through their pulse, the lowest point of the 1-up mushroom (a centred sprite) and the bubble (a unit sphere)
    // stays on the floor, never under it and never more than a hair above it.
    for k in 0..300 {
        let t = k as f32 * 0.05;
        let (s, lift) = item_look(7, false, t);
        let low = lift - ground_fit::MUSHROOM_FOOT * crate::render::SCALE * s.y;
        assert!(low.abs() < 1e-4, "mushroom bottom {low} at {t}");
        let (s, lift) = item_look(9, false, t);
        assert!((lift - s.y).abs() < 1e-4, "bubble bottom {} at {t}", lift - s.y);
        let (s, lift) = item_look(2, false, t);
        assert!(lift - s.y >= -1e-4, "rock bottom {} at {t}", lift - s.y);
    }
}

#[test]
fn items_pushed_uphill_by_the_wind_ride_up_the_floor() {
    let mut b = slope_arena(vec![], 3);
    for i in 0..2 {
        b.f[i].inv = 60000;
    }
    put(&mut b, 0, -600.0, -600.0);
    put(&mut b, 1, -600.0, 600.0);
    let kinds = [ItemKind::Crate, ItemKind::Barrel, ItemKind::Rock];
    let mut slots = vec![];
    for (k, kind) in kinds.iter().enumerate() {
        let z = -200.0 + 200.0 * k as f32;
        let y = floor_at(&b, -300.0, z, 0.0);
        slots.push(b.spawn_item(*kind, [-300.0, y, z], false).unwrap());
    }
    let start: Vec<f32> = slots.iter().map(|&s| b.items[s].pos[0]).collect();
    for t in 0..30 * 6 {
        // Wind straight up the hill (+x).
        b.arena.wind = 1.0;
        b.arena.wind_dir = [1.0, 0.0];
        b.tick(&idle());
        for &s in &slots {
            let it = b.items[s];
            if !it.alive || it.state != IState::Rest {
                continue;
            }
            let g = floor_at(&b, it.pos[0], it.pos[2], it.pos[1]);
            assert!(it.pos[1] >= g - 0.5, "tick {t}: {:?} {:.1} into the hill at {:?}", it.kind, g - it.pos[1], it.pos);
        }
    }
    for (k, &s) in slots.iter().enumerate() {
        let it = b.items[s];
        assert!(it.pos[0] > start[k] + 40.0, "{:?} was pushed uphill ({} -> {})", it.kind, start[k], it.pos[0]);
        let g = floor_at(&b, it.pos[0], it.pos[2], it.pos[1]);
        assert!((it.pos[1] - g).abs() < 1.0, "{:?} rests on the floor: {:.1} off", it.kind, it.pos[1] - g);
    }
}
