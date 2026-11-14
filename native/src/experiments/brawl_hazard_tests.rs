//! Course hazards (`brawl_hazard.rs`) on the fixture ground: every look telegraphs before it strikes, stepping out of the
//! telegraph (or over a roller) avoids it, drifters behave, credit for a set-up knock-out, quicksand, and every shipped region's
//! hazards stand on its real course. A child of `brawl_region_tests.rs`.
use super::*;

fn region_with(defs: Vec<HazardDef>) -> Region {
    Region { hazards: Box::leak(defs.into_boxed_slice()), ..fixture_region() }
}

/// A duel on the fixture with the hazards live (past the calm opening).
fn hz_duel(defs: Vec<HazardDef>, seed: u64) -> Brawl {
    let mut b = duel_on(region_with(defs), seed);
    b.round_t = ROUND_SECS - HAZARD_GRACE - 1.0;
    b
}

/// Fighter 1 out of the way and untouchable (killing it would end the round and freeze the hazards).
fn park(b: &mut Brawl) {
    put(b, 1, -350.0, 650.0);
    b.f[1].inv = 60000;
    for h in b.arena.hazards.iter_mut() {
        h.target = -1;
    }
}

fn total_hp(b: &Brawl) -> f32 {
    (0..b.n).map(|i| b.f[i].hp).sum()
}

/// Run until the first hazard damage; (tick its telegraph began, tick of the damage).
fn first_strike(b: &mut Brawl, max: usize, mut each: impl FnMut(&mut Brawl)) -> (Option<usize>, Option<usize>) {
    let (mut warn, mut hurt) = (None, None);
    for t in 0..max {
        each(b);
        let hp = total_hp(b);
        b.tick(&idle());
        for e in &b.ev {
            if let Ev::Hazard { strike: false, .. } = e {
                warn.get_or_insert(t);
            }
        }
        if total_hp(b) < hp - 0.01 {
            hurt = Some(t);
            break;
        }
        b.round_t = b.round_t.max(30.0);
    }
    (warn, hurt)
}

#[test]
fn every_attacking_hazard_telegraphs_before_it_strikes() {
    for look in [Look::ChainChomp, Look::DekuBaba, Look::Whomp, Look::WhompKing, Look::Thwomp, Look::IronBall, Look::Boulder, Look::Snowball, Look::Horse, Look::Goomba, Look::Wave] {
        let mut b = hz_duel(vec![HazardDef::new(look, [0.0, 0.0])], 5);
        put(&mut b, 0, -220.0, 0.0);
        put(&mut b, 1, 350.0, 650.0);
        let (warn, hurt) = first_strike(&mut b, 30 * 25, |b| {
            // Keep the fighters standing where they are (a hit moves them; the test only wants the first one).
            for i in 0..2 {
                b.f[i].hp = b.f[i].max_hp;
            }
        });
        let (w, h) = (warn.unwrap_or_else(|| panic!("{look:?} never telegraphed")), hurt.unwrap_or_else(|| panic!("{look:?} never struck")));
        let sp = look.spec();
        assert!(h >= w + sp.warn as usize - 1, "{look:?}: struck {} ticks after its warning (needs {})", h - w, sp.warn);
        assert!(sp.warn >= 20, "{look:?}: at least two thirds of a second of warning");
    }
}

#[test]
fn stepping_out_of_the_ring_dodges_the_chomp() {
    let mut b = hz_duel(vec![HazardDef::new(Look::ChainChomp, [0.0, 0.0])], 5);
    put(&mut b, 0, -220.0, 0.0);
    put(&mut b, 1, 350.0, 650.0);
    let mut moved = false;
    for _ in 0..30 * 10 {
        b.tick(&idle());
        let h = b.arena.hazards[0];
        if h.st == HzState::Warn && !moved {
            // The ring is where the target stood; walk out of it (and out of the chain's reach).
            assert!(h.zone().unwrap().contains([b.f[0].pos[0], b.f[0].pos[2]], 0.0), "the ring is on the target");
            put(&mut b, 0, -220.0, -680.0);
            moved = true;
        }
        if moved && h.st == HzState::Rest {
            break;
        }
    }
    assert!(moved);
    assert_eq!(b.f[0].hp, b.f[0].max_hp, "out of the ring, untouched");
}

#[test]
fn a_roller_is_jumped_and_a_giant_goomba_is_stomped() {
    // Hold fighter 0 in the air above the ball for the whole crossing: never touched.
    let mut b = hz_duel(vec![HazardDef::lane(Look::IronBall, [0.0, 0.0], [1.0, 0.0])], 6);
    put(&mut b, 0, 0.0, 0.0);
    put(&mut b, 1, 0.0, 600.0);
    park(&mut b);
    let mut crossed = false;
    for _ in 0..30 * 15 {
        let f = &mut b.f[0];
        f.pos[1] = 230.0;
        f.vel = [0.0; 3];
        f.grounded = false;
        f.act = Act::Air;
        b.tick(&idle());
        let h = b.arena.hazards[0];
        crossed |= h.st == HzState::Act && h.pos[0] > 0.0;
        if crossed && h.st == HzState::Idle {
            break;
        }
    }
    assert!(crossed, "the ball rolled through");
    assert_eq!(b.f[0].hp, b.f[0].max_hp, "jumped over");
    // A goomba: fall on it from above and it is flattened (the crossing ends at once).
    let mut b = hz_duel(vec![HazardDef::lane(Look::Goomba, [0.0, 0.0], [1.0, 0.0])], 6);
    put(&mut b, 0, 0.0, 0.0);
    put(&mut b, 1, 0.0, 600.0);
    park(&mut b);
    let mut flat = false;
    for _ in 0..30 * 15 {
        let h = b.arena.hazards[0];
        if h.st == HzState::Act && h.pos[0].abs() < 250.0 {
            // Drop onto the goomba's head.
            let f = &mut b.f[0];
            f.pos = [h.pos[0], h.pos[1] + Look::Goomba.spec().height + 20.0, h.pos[2]];
            f.vel = [0.0, -400.0, 0.0];
            f.grounded = false;
            f.act = Act::Air;
        }
        b.tick(&idle());
        if b.ev.iter().any(|e| matches!(e, Ev::MonKo { .. })) {
            flat = true;
            assert!(b.f[0].vel[1] > 500.0, "bounced off");
            break;
        }
    }
    assert!(flat, "stomped");
}

#[test]
fn drifters_tornado_flings_boo_waits_while_watched_redead_freezes() {
    // Tweester: walks up to a still fighter and throws them in the air.
    let mut b = hz_duel(vec![HazardDef::new(Look::Tornado, [0.0, 0.0])], 7);
    put(&mut b, 0, -250.0, 0.0);
    put(&mut b, 1, 350.0, 650.0);
    let mut flung = false;
    for _ in 0..30 * 10 {
        b.tick(&idle());
        if b.f[0].vel[1] > 1000.0 || b.f[1].vel[1] > 1000.0 {
            flung = true;
            break;
        }
    }
    assert!(flung, "the Tweester launches whoever it touches");
    // Boo: frozen while its target faces it, moving in when the back is turned.
    let mut b = hz_duel(vec![HazardDef::new(Look::Boo, [300.0, 0.0])], 7);
    put(&mut b, 0, -300.0, 0.0);
    put(&mut b, 1, 400.0, 650.0);
    park(&mut b);
    b.f[0].yaw = yaw_of([1.0, 0.0]);
    let start = b.arena.hazards[0].pos;
    b.arena.hazards[0].target = -1;
    for _ in 0..40 {
        b.f[0].yaw = yaw_of([1.0, 0.0]);
        b.tick(&idle());
    }
    let watched = b.arena.hazards[0].pos;
    assert!((watched[0] - start[0]).abs() < 1.0, "a watched Boo holds still: {start:?} -> {watched:?}");
    for _ in 0..40 {
        b.f[0].yaw = yaw_of([-1.0, 0.0]);
        b.tick(&idle());
    }
    let h = b.arena.hazards[0];
    assert!(h.pos[0] < watched[0] - 100.0, "and creeps up once the back is turned: {:?} {:?} t{} tgt {} f0 {:?}", h.pos, h.st, h.t, h.target, b.f[0].pos);
    // Redead: screams (telegraph) then freezes whoever is near.
    let mut b = hz_duel(vec![HazardDef::new(Look::Redead, [0.0, 0.0])], 7);
    put(&mut b, 0, -150.0, 0.0);
    put(&mut b, 1, 400.0, 650.0);
    park(&mut b);
    let (warn, hurt) = first_strike(&mut b, 30 * 5, |_| {});
    let (w, h) = (warn.expect("scream telegraph"), hurt.expect("scream"));
    assert!(h >= w + Look::Redead.spec().warn as usize - 1);
    assert!(b.f[0].act == Act::Hurt && b.f[0].stun >= 20, "frozen");
}

#[test]
fn a_hazard_knock_out_credits_whoever_set_it_up() {
    let mut b = hz_duel(vec![], 3);
    put(&mut b, 0, -200.0, 0.0);
    put(&mut b, 1, 0.0, 0.0);
    b.hit(1, 0, 5.0, [1.0, 0.0], 100.0, 0.0);
    b.f[1].inv = 0;
    b.f[1].hp = 2.0;
    b.hit(1, -1, 10.0, [1.0, 0.0], 100.0, 0.0);
    assert!(!b.f[1].alive && b.f[0].kos == 1, "a fresh set-up counts");
    // An old blow does not.
    let mut b = hz_duel(vec![], 3);
    put(&mut b, 0, -200.0, 0.0);
    put(&mut b, 1, 0.0, 0.0);
    b.hit(1, 0, 5.0, [1.0, 0.0], 100.0, 0.0);
    b.tick += CREDIT_TICKS + 10;
    b.f[1].inv = 0;
    b.f[1].hp = 2.0;
    b.hit(1, -1, 10.0, [1.0, 0.0], 100.0, 0.0);
    assert!(!b.f[1].alive && b.f[0].kos == 0);
}

/// 3000 x 3000 of instant quicksand at y = 0.
fn sand_world() -> Arc<CollisionWorld> {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions { kind: jumpman_sim::surfaces::SURFACE_INSTANT_QUICKSAND, ..Default::default() };
    w.add_triangle([-1500.0, 0.0, -1500.0], [1500.0, 0.0, 1500.0], [1500.0, 0.0, -1500.0], &o);
    w.add_triangle([-1500.0, 0.0, -1500.0], [-1500.0, 0.0, 1500.0], [1500.0, 0.0, 1500.0], &o);
    Arc::new(w)
}

#[test]
fn quicksand_swallows_who_stands_still_and_not_who_keeps_running() {
    let region = Region { centre: [0.0, 600.0, 0.0], floor: Some(0.0), ..fixture_region() };
    let run = |moving: bool| {
        let cfg = Config { seed: 2, humans: 2, bots: 0, arena: ArenaKind::Field, ..Default::default() };
        let mut b = Brawl::with_arena(cfg, Some(region.build(sand_world())));
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
        let mut spat = 0;
        for t in 0..30 * 6 {
            let mut inp = idle();
            if moving {
                inp[0].mv = if (t / 25) % 2 == 0 { [1.0, 0.0] } else { [-1.0, 0.0] };
            }
            b.tick(&inp);
            spat += b.ev.iter().filter(|e| matches!(e, Ev::Hit { victim: 0, .. })).count();
        }
        (spat, b.f[0].hp < b.f[0].max_hp)
    };
    let (still, hurt) = run(false);
    assert!(still >= 2 && hurt, "standing still: spat out {still} times");
    let (moving, _) = run(true);
    assert_eq!(moving, 0, "running keeps you on top");
}

#[test]
fn hazards_are_deterministic_and_round_reset() {
    let d = |seed| {
        let defs = vec![HazardDef::new(Look::ChainChomp, [0.0, 0.0]), HazardDef::lane(Look::Horse, [0.0, 0.0], [1.0, 0.0]), HazardDef::new(Look::Tornado, [200.0, 300.0])];
        let cfg = Config { seed, humans: 0, bots: 3, arena: ArenaKind::Field, ..Default::default() };
        let mut b = Brawl::with_arena(cfg, Some(region_with(defs).build(fixture())));
        for _ in 0..30 * 40 {
            b.tick(&idle());
        }
        b.digest()
    };
    assert_eq!(d(4), d(4));
    assert_ne!(d(4), d(5));
}

#[test]
fn every_region_hazard_stands_on_its_course_and_bots_survive_them() {
    for r in REGIONS {
        let Some(world) = course_world(r.course) else { continue };
        let a = r.build(world);
        assert!(a.hazards.len() <= MAXH);
        for h in &a.hazards {
            let sp = h.spec();
            // Posts, homes and starting spots of the anchored / walking ones are standable ground in the arena.
            if sp.kind != HzKind::Roller {
                assert!(a.over_floor(h.home[0], h.home[2]), "{}: {:?} home {:?} is off the floor", r.key, h.look, h.home);
                assert!(a.support(h.home[0], h.home[2], h.home[1] + 50.0).is_some_and(|s| (s.0 - h.home[1]).abs() < 30.0), "{}: {:?} home {:?}", r.key, h.look, h.home);
            }
        }
        assert!(!a.hazards.is_empty() || a.gusts || a.barrage > 0, "{}: every region has something of its course going on", r.key);
    }
}

// ---------------------------------------------------------------------------------------------
// Bots: telegraphs, quicksand, ring-outs.
// ---------------------------------------------------------------------------------------------

/// You (an idle human, slot 0) against one bot (slot 1) of `diff` on `region`, past the calm opening.
fn vs_bot(region: Region, diff: Diff, seed: u64) -> Brawl {
    let cfg = Config { seed, humans: 1, bots: 1, diff, arena: ArenaKind::Field, ..Default::default() };
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
    b.round_t = ROUND_SECS - HAZARD_GRACE - 1.0;
    b
}

#[test]
fn bots_walk_out_of_a_telegraph() {
    let mut hits = [0u32; 2];
    for (k, diff) in [Diff::Normal, Diff::Hard].into_iter().enumerate() {
        for seed in 0..6u64 {
            let mut b = vs_bot(region_with(vec![HazardDef::new(Look::ChainChomp, [0.0, 0.0])]), diff, seed);
            put(&mut b, 0, 380.0, 650.0);
            b.f[0].inv = 60000;
            put(&mut b, 1, -260.0, 40.0);
            // Hold the bot still until the Chomp picks it, then let it think.
            let mut started = false;
            for _ in 0..30 * 12 {
                let h = b.arena.hazards[0];
                if !started {
                    put(&mut b, 1, -260.0, 40.0);
                    if h.st == HzState::Warn {
                        started = true;
                    }
                }
                let hp = b.f[1].hp;
                b.tick(&idle());
                if started && b.f[1].hp < hp {
                    hits[k] += 1;
                    break;
                }
                if started && b.arena.hazards[0].st == HzState::Rest {
                    break;
                }
            }
            assert!(started);
        }
    }
    println!("chomp hits on a bot over 6 lunges: normal {}, hard {}", hits[0], hits[1]);
    assert!(hits[1] <= 1 && hits[0] <= 3, "bots read the ring: {hits:?}");
}

#[test]
fn a_hard_bot_knocks_a_foe_standing_by_a_drop_over_it() {
    // A taller fixture region: the pit at z -1000..-800 (x -200..200) is inside the rim.
    let region = Region { half: [450.0, 1100.0], spawns: [[-300.0, -300.0], [300.0, 500.0], [300.0, -300.0], [-300.0, 500.0]], ..fixture_region() };
    let b0 = vs_bot(region, Diff::Hard, 1);
    let d = b0.knock_target([0.0, -720.0], 0.0).expect("the pit is seen");
    assert!(d[1] < -0.6, "toward the pit: {d:?}");
    assert!(b0.knock_target([-300.0, 300.0], 0.0).is_none(), "open ground has nothing to knock into");
    let mut falls = 0;
    for seed in 0..5u64 {
        let mut b = vs_bot(region, Diff::Hard, seed);
        put(&mut b, 0, 0.0, -730.0);
        put(&mut b, 1, -350.0, -900.0);
        for _ in 0..30 * 15 {
            b.tick(&idle());
            if b.ev.iter().any(|e| matches!(e, Ev::Fell(0))) {
                falls += 1;
                break;
            }
            if b.phase != Phase::Fight {
                break;
            }
            // You stand your ground by the edge (back on your spot when knocked along it but not over).
            if b.f[0].grounded && b.f[0].act == Act::Idle && b.tick % 45 == 0 {
                put(&mut b, 0, 0.0, -730.0);
            }
        }
    }
    println!("ring-outs: {falls}/5");
    assert!(falls >= 3, "the bot sets up the ring-out: {falls}/5");
}
