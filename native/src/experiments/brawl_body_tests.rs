//! Solid bodies in Gem Clash (`brawl_body.rs`): course set pieces, roaming monsters and fighters never stand inside each
//! other, whichever walks into whichever, and the blows still land at body contact. A child of `brawl_region_tests.rs`
//! (the fixture ground).
use super::*;
use crate::experiments::brawl_arena::ARENAS;

/// Penetration allowed beyond the solid reach (units): walls pushing back at a body can leave a sliver.
const TOL: f32 = 2.0;
/// Bots piling up: a fighter boxed in by bodies (seen: a Chain Chomp's head, a boulder rolling past, a Redead and another
/// fighter around one fighter) may keep a sliver while the pile sorts itself out.
const TOL_JAM: f32 = 3.0;

fn region_with(defs: Vec<HazardDef>) -> Region {
    Region { hazards: Box::leak(defs.into_boxed_slice()), ..fixture_region() }
}

fn toward(from: [f32; 3], to: [f32; 2]) -> V2 {
    let d = [to[0] - from[0], to[1] - from[2]];
    let l = d[0].hypot(d[1]);
    if l > 1.0 { [d[0] / l, d[1] / l] } else { [0.0, 0.0] }
}

#[test]
fn ghosts_wind_and_water_stay_intangible_everything_else_is_solid() {
    for (look, solid) in [
        (Look::ChainChomp, true),
        (Look::DekuBaba, true),
        (Look::Whomp, true),
        (Look::WhompKing, true),
        (Look::Thwomp, true),
        (Look::Redead, true),
        (Look::Boo, false),
        (Look::Tornado, false),
    ] {
        let mut b = duel_on(region_with(vec![HazardDef::new(look, [0.0, 0.0])]), 3);
        b.round_t = ROUND_SECS - HAZARD_GRACE - 1.0;
        let (_, n) = b.solids();
        assert_eq!(n > 0, solid, "{look:?}");
    }
}

/// Fighter 0 walks into a course set piece for 20 s (and the set piece comes at it): never inside it. Run once untouchable
/// (pure body contact: it is held at the surface, never through) and once vulnerable (the blows still land).
#[test]
fn every_solid_course_hazard_blocks_a_fighter_both_ways_and_still_hits() {
    for look in [Look::ChainChomp, Look::DekuBaba, Look::Whomp, Look::WhompKing, Look::Thwomp, Look::IronBall, Look::Boulder, Look::Snowball, Look::Horse, Look::Goomba, Look::Redead] {
        for vulnerable in [false, true] {
            let mut b = duel_on(region_with(vec![HazardDef::lane(look, [0.0, 0.0], [1.0, 0.0])]), 5);
            b.round_t = ROUND_SECS - HAZARD_GRACE - 1.0;
            put(&mut b, 0, -260.0, 0.0);
            put(&mut b, 1, -350.0, 650.0);
            b.f[1].inv = 60000;
            let (mut worst, mut contact, mut hurt) = (0.0f32, 0u32, 0.0f32);
            for _ in 0..30 * 20 {
                if !vulnerable {
                    b.f[0].inv = 60000;
                }
                let hp = b.f[0].hp;
                let h = b.arena.hazards[0];
                let mut inp = idle();
                inp[0].mv = toward(b.f[0].pos, [h.pos[0], h.pos[2]]);
                b.tick(&inp);
                hurt += (hp - b.f[0].hp).max(0.0);
                b.f[0].hp = b.f[0].max_hp;
                b.f[1].hp = b.f[1].max_hp;
                b.round_t = b.round_t.max(30.0);
                let (o, what) = b.body_overlap();
                if o > worst {
                    worst = o;
                    assert!(o <= TOL, "{look:?} (vulnerable {vulnerable}): {what} overlap {o:.1}");
                }
                let (solids, n) = b.solids();
                for s in solids[..n].iter().flatten() {
                    if (s.c[0] - b.f[0].pos[0]).hypot(s.c[1] - b.f[0].pos[2]) < (s.radius + b.f[0].radius()) * 0.9 {
                        contact += 1;
                    }
                }
            }
            if vulnerable {
                assert!(hurt > 0.0, "{look:?} still hits a fighter who walks into it");
            } else if !matches!(look, Look::IronBall | Look::Boulder | Look::Snowball | Look::Horse | Look::Goomba) {
                // Anchored / walking set pieces: the fighter was pressed against it (rollers come and go).
                assert!(contact > 30, "{look:?}: the fighter reached body contact ({contact} ticks)");
            }
            eprintln!("body {look:?} vulnerable={vulnerable}: worst overlap {worst:.2}, {contact} contact ticks, {hurt:.0} hp lost");
        }
    }
}

/// On every box arena with the monsters at Many, fighter 0 keeps walking at the nearest monster: never inside one, and
/// the monsters still bite.
#[test]
fn roaming_monsters_are_solid_and_still_bite() {
    for arena in ARENAS {
        let cfg = Config { seed: 11, humans: 2, bots: 0, arena, monsters: Monsters::Many, ..Default::default() };
        let mut b = Brawl::new(cfg);
        for _ in 0..INTRO_TICKS + 2 {
            b.tick(&idle());
        }
        b.f[1].inv = 60000;
        let (mut worst, mut hurt) = (0.0f32, 0.0f32);
        for _ in 0..30 * 25 {
            let hp = b.f[0].hp;
            let me = b.f[0].pos;
            let near = b.mons.iter().filter(|m| m.on && m.alive).min_by(|a, c| {
                let da = (a.pos[0] - me[0]).hypot(a.pos[2] - me[2]);
                let dc = (c.pos[0] - me[0]).hypot(c.pos[2] - me[2]);
                da.total_cmp(&dc)
            });
            let mut inp = idle();
            if let Some(m) = near {
                inp[0].mv = toward(me, [m.pos[0], m.pos[2]]);
            }
            b.tick(&inp);
            hurt += (hp - b.f[0].hp).max(0.0);
            b.f[0].hp = b.f[0].max_hp;
            b.f[1].hp = b.f[1].max_hp;
            b.round_t = b.round_t.max(30.0);
            let (o, what) = b.body_overlap();
            worst = worst.max(o);
            assert!(o <= TOL, "{arena:?}: {what} overlap {o:.1}");
        }
        assert!(hurt > 0.0, "{arena:?}: the monsters still hurt a fighter at body contact");
        eprintln!("body monsters {arena:?}: worst overlap {worst:.2}, {hurt:.0} hp lost");
    }
}

/// A fighter running into another (standing, then both running at each other) stops at body contact and does not pass
/// through; a punch at body contact still connects.
#[test]
fn fighters_do_not_walk_through_each_other() {
    let mut b = duel_on(fixture_region(), 7);
    put(&mut b, 0, -300.0, 0.0);
    put(&mut b, 1, 0.0, 0.0);
    for t in 0..60 {
        let mut inp = idle();
        inp[0].mv = [1.0, 0.0];
        if t >= 30 {
            inp[1].mv = [-1.0, 0.0];
        }
        b.tick(&inp);
        let (o, what) = b.body_overlap();
        assert!(o <= TOL, "tick {t}: {what} overlap {o:.1}");
        assert!(b.f[0].pos[0] < b.f[1].pos[0], "tick {t}: fighter 0 passed through fighter 1");
    }
    let hp = b.f[1].hp;
    let mut inp = idle();
    inp[0].attack = true;
    b.f[0].yaw = yaw_of([1.0, 0.0]);
    for _ in 0..12 {
        b.tick(&inp);
        inp[0].attack = false;
    }
    assert!(b.f[1].hp < hp, "a punch at body contact connects");
}

/// Bots fighting on a region with monsters and set pieces: nobody ends up inside anybody over a whole match.
#[test]
fn bot_matches_with_monsters_and_hazards_keep_bodies_apart() {
    let defs = vec![HazardDef::new(Look::ChainChomp, [-200.0, 300.0]), HazardDef::new(Look::Whomp, [200.0, -300.0]), HazardDef::lane(Look::Boulder, [0.0, 0.0], [1.0, 0.0]), HazardDef::new(Look::Redead, [0.0, 0.0])];
    for seed in [1u64, 2, 3, 4, 5, 6] {
        let cfg = Config { seed, humans: 0, bots: 4, arena: ArenaKind::Field, monsters: Monsters::Many, ..Default::default() };
        let mut b = Brawl::with_arena(cfg, Some(region_with(defs.clone()).build(fixture())));
        let mut worst = 0.0f32;
        for _ in 0..30 * 60 {
            b.tick(&idle());
            let (o, what) = b.body_overlap();
            worst = worst.max(o);
            if o > TOL_JAM {
                for i in 0..4 {
                    let f = &b.f[i];
                    eprintln!("f{i} pos {:?} gr {} act {:?} alive {} r {} sup {:?}", f.pos, f.grounded, f.act, f.alive, f.radius(), b.arena.support(f.pos[0], f.pos[2], f.pos[1] + 34.0));
                }
                let (s, n) = b.solids();
                for s in s[..n].iter().flatten() {
                    eprintln!("solid {s:?}");
                }
            }
            assert!(o <= TOL_JAM, "seed {seed}: {what} overlap {o:.1}");
        }
        eprintln!("body bots seed {seed}: worst overlap {worst:.2}, round {}", b.round);
    }
}

/// What the body pass costs in Gem Clash's busiest scene: four bots, monsters at Many and four set pieces on a region.
/// The same match with the pass on and off; the difference per tick is the cost (it also changes the match, so this is
/// an estimate: both runs do the same amount of other work on average).
#[test]
fn body_pass_cost_in_the_busiest_scene() {
    let defs = vec![HazardDef::new(Look::ChainChomp, [-200.0, 300.0]), HazardDef::new(Look::Whomp, [200.0, -300.0]), HazardDef::lane(Look::Boulder, [0.0, 0.0], [1.0, 0.0]), HazardDef::new(Look::Redead, [0.0, 0.0])];
    let run = |off: bool| {
        let mut total = std::time::Duration::ZERO;
        let mut ticks = 0u32;
        for seed in 1..=4u64 {
            let cfg = Config { seed, humans: 0, bots: 4, arena: ArenaKind::Field, monsters: Monsters::Many, ..Default::default() };
            let mut b = Brawl::with_arena(cfg, Some(region_with(defs.clone()).build(fixture())));
            b.bodies_off = off;
            let t0 = std::time::Instant::now();
            for _ in 0..30 * 60 {
                b.tick(&idle());
                ticks += 1;
            }
            total += t0.elapsed();
        }
        total.as_secs_f64() * 1e6 / ticks as f64
    };
    let (on, off) = (run(false), run(true));
    eprintln!("body cost Gem Clash: {on:.2} us/tick with bodies, {off:.2} without ({:+.2} us)", on - off);
    assert!(on - off < 40.0, "the body pass costs {:.1} us a tick", on - off);
}
