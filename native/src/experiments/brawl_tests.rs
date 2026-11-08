use super::*;

fn idle() -> [Input; MAXF] {
    [Input::default(); MAXF]
}

fn duel_cfg(arena: ArenaKind) -> Config {
    Config { seed: 5, mode: Mode::Versus, arena, humans: 2, bots: 0, ..Default::default() }
}

/// Two human slots, past the countdown, standing 90 units apart facing each other.
fn ready(arena: ArenaKind) -> Brawl {
    let mut b = ready_with(arena);
    b.arena.bumpers.clear();
    b
}

fn ready_with(arena: ArenaKind) -> Brawl {
    let mut b = Brawl::new(duel_cfg(arena));
    for _ in 0..INTRO_TICKS + 2 {
        b.tick(&idle());
    }
    assert_eq!(b.phase, Phase::Fight);
    b.items = [Item::NONE; MAXI];
    b.item_timer = 100_000;
    for g in b.gems.iter_mut() {
        g.state = GemState::Spent;
    }
    place(&mut b, 0, [0.0, 0.0]);
    place(&mut b, 1, [95.0, 0.0]);
    b.f[0].yaw = yaw_of([1.0, 0.0]);
    b.f[1].yaw = yaw_of([-1.0, 0.0]);
    b
}

fn place(b: &mut Brawl, i: usize, xz: V2) {
    let top = b.arena.support(xz[0], xz[1], 1000.0).map_or(0.0, |t| t.0);
    let f = &mut b.f[i];
    f.pos = [xz[0], top, xz[1]];
    f.vel = [0.0; 3];
    f.grounded = true;
    f.act = Act::Idle;
    f.inv = 0;
}

fn press_attack(i: usize) -> [Input; MAXF] {
    let mut inp = idle();
    inp[i].attack = true;
    inp
}

fn run(b: &mut Brawl, n: usize) {
    for _ in 0..n {
        b.tick(&idle());
    }
}

#[test]
fn punch_damages_and_pushes_back() {
    let mut b = ready(ArenaKind::Bumpers);
    b.tick(&press_attack(0));
    run(&mut b, 10);
    assert!(b.f[1].hp < b.f[1].max_hp, "hp {}", b.f[1].hp);
    assert!((b.f[1].hp - 94.0).abs() < 0.01, "a jab does 6: {}", b.f[1].hp);
    assert!(b.f[1].pos[0] > 95.0, "knocked away from the attacker");
    assert_eq!(b.f[0].hp, 100.0);
}

#[test]
fn punch_punch_kick_chain_ends_in_a_knockdown() {
    let mut b = ready(ArenaKind::Bumpers);
    b.f[1].inv = 0;
    // Press attack three times with the right rhythm; keep the victim in reach.
    let mut downs = 0;
    for t in 0..40 {
        let x0 = b.f[0].pos[0];
        place(&mut b, 1, [x0 + 85.0, 0.0]);
        b.f[1].inv = 0;
        let mut inp = idle();
        if t == 0 || t == 5 || t == 14 {
            inp[0].attack = true;
        }
        b.tick(&inp);
        if b.f[1].act == Act::Down {
            downs += 1;
        }
    }
    assert!(downs > 0, "the kick launches the victim");
    assert!(b.f[1].hp < 100.0 - 6.0 - 6.0 - 9.0, "three hits landed: {}", b.f[1].hp);
}

#[test]
fn invulnerability_frames_stop_double_hits() {
    let mut b = ready(ArenaKind::Bumpers);
    assert!(b.hit(1, 0, 10.0, [1.0, 0.0], 100.0, 0.0));
    assert!(!b.hit(1, 0, 10.0, [1.0, 0.0], 100.0, 0.0), "the victim is briefly protected");
    assert_eq!(b.f[1].hp, 90.0);
}

#[test]
fn knockback_grows_as_health_falls() {
    let mut b = ready(ArenaKind::Bumpers);
    b.hit(1, 0, 1.0, [1.0, 0.0], 400.0, 0.0);
    let healthy = b.f[1].vel[0];
    let mut c = ready(ArenaKind::Bumpers);
    c.f[1].hp = 20.0;
    c.hit(1, 0, 1.0, [1.0, 0.0], 400.0, 0.0);
    assert!(c.f[1].vel[0] > healthy * 1.2, "{} vs {}", c.f[1].vel[0], healthy);
}

#[test]
fn jump_and_double_jump_then_land() {
    let mut b = ready(ArenaKind::Bumpers);
    let mut inp = idle();
    inp[0].jump = true;
    inp[0].jump_held = true;
    b.tick(&inp);
    assert!(!b.f[0].grounded && b.f[0].vel[1] > 800.0);
    let mut held = idle();
    held[0].jump_held = true;
    for _ in 0..8 {
        b.tick(&held);
    }
    let mut again = idle();
    again[0].jump = true;
    again[0].jump_held = true;
    let before = b.f[0].vel[1];
    b.tick(&again);
    assert!(b.f[0].vel[1] > before, "a second jump in the air");
    let mut peak = 0.0f32;
    for _ in 0..80 {
        b.tick(&idle());
        peak = peak.max(b.f[0].pos[1]);
    }
    assert!(peak > 250.0, "double jump goes higher than one: {peak}");
    assert!(b.f[0].grounded && b.f[0].pos[1] == 0.0);
}

#[test]
fn ground_pound_shockwave_hits_nearby_not_far() {
    let mut b = ready(ArenaKind::Bumpers);
    place(&mut b, 1, [200.0, 0.0]);
    b.f[0].pos[1] = 300.0;
    b.f[0].grounded = false;
    b.f[0].act = Act::Air;
    let mut inp = idle();
    inp[0].z = true;
    b.tick(&inp);
    assert_eq!(b.f[0].act, Act::PoundWind);
    run(&mut b, 40);
    assert!(b.f[1].hp < 100.0, "inside the ring: {}", b.f[1].hp);
    let mut c = ready(ArenaKind::Bumpers);
    place(&mut c, 1, [700.0, 0.0]);
    c.f[0].pos[1] = 300.0;
    c.f[0].grounded = false;
    c.f[0].act = Act::Air;
    c.tick(&inp);
    run(&mut c, 40);
    assert_eq!(c.f[1].hp, 100.0);
}

#[test]
fn dive_attack_connects() {
    let mut b = ready(ArenaKind::Bumpers);
    place(&mut b, 1, [220.0, 0.0]);
    b.f[0].pos[1] = 150.0;
    b.f[0].grounded = false;
    b.f[0].act = Act::Air;
    b.tick(&press_attack(0));
    assert_eq!(b.f[0].act, Act::Dive);
    run(&mut b, 14);
    assert!(b.f[1].hp < 100.0, "dive hit {}", b.f[1].hp);
}

#[test]
fn grab_and_throw_a_crate_hurts_and_breaks() {
    let mut b = ready(ArenaKind::Bumpers);
    b.spawn_item(ItemKind::Crate, [40.0, 0.0, 0.0], false);
    run(&mut b, 4);
    let mut inp = idle();
    inp[0].z = true;
    b.tick(&inp);
    assert!(b.f[0].held >= 0, "picked up");
    b.f[0].cd_grab = 0;
    b.tick(&press_attack(0));
    assert!(b.f[0].held < 0);
    run(&mut b, 20);
    assert!(b.f[1].hp < 100.0, "crate hit: {}", b.f[1].hp);
    assert!(!b.items.iter().any(|i| i.alive), "the crate broke on impact");
}

#[test]
fn bomb_fuse_explodes_in_hand() {
    let mut b = ready(ArenaKind::Bumpers);
    place(&mut b, 1, [400.0, 0.0]);
    b.spawn_item(ItemKind::Bomb, [30.0, 0.0, 0.0], false);
    run(&mut b, 3);
    let mut inp = idle();
    inp[0].z = true;
    b.tick(&inp);
    assert!(b.f[0].held >= 0);
    run(&mut b, 90);
    assert!(b.f[0].hp < 100.0, "holding a lit bomb too long hurts: {}", b.f[0].hp);
}

#[test]
fn hammer_swings_hard_and_wears_out() {
    let mut b = ready(ArenaKind::Bumpers);
    let k = b.spawn_item(ItemKind::Hammer, [30.0, 0.0, 0.0], false).unwrap();
    run(&mut b, 3);
    let mut inp = idle();
    inp[0].z = true;
    b.tick(&inp);
    assert_eq!(b.f[0].held, k as i8);
    b.tick(&press_attack(0));
    run(&mut b, 20);
    assert!(b.f[1].hp <= 80.0, "hammer does 22: {}", b.f[1].hp);
    for _ in 0..4 {
        place(&mut b, 1, [95.0, 0.0]);
        b.f[1].inv = 0;
        b.f[0].yaw = yaw_of([1.0, 0.0]);
        b.tick(&press_attack(0));
        run(&mut b, 22);
    }
    assert!(!b.items[k].alive, "four swings break it");
}

#[test]
fn blaster_fires_bullets_with_ammo() {
    let mut b = ready(ArenaKind::Bumpers);
    place(&mut b, 1, [500.0, 0.0]);
    let k = b.spawn_item(ItemKind::Blaster, [30.0, 0.0, 0.0], false).unwrap();
    run(&mut b, 3);
    let mut inp = idle();
    inp[0].z = true;
    b.tick(&inp);
    assert_eq!(b.f[0].held, k as i8);
    b.tick(&press_attack(0));
    run(&mut b, 14);
    assert!(b.f[1].hp < 100.0, "bullet hit {}", b.f[1].hp);
    assert_eq!(b.items[k].uses, 11);
}

#[test]
fn boomerang_returns_to_its_thrower() {
    let mut b = ready(ArenaKind::Bumpers);
    place(&mut b, 1, [600.0, 400.0]);
    let k = b.spawn_item(ItemKind::Boomerang, [30.0, 0.0, 0.0], false).unwrap();
    run(&mut b, 3);
    let mut inp = idle();
    inp[0].z = true;
    b.tick(&inp);
    b.tick(&press_attack(0));
    assert!(b.f[0].boom_out);
    run(&mut b, 120);
    assert!(!b.f[0].boom_out, "caught or expired");
    assert!(b.items[k].alive && b.items[k].uses == 5);
}

#[test]
fn heart_heals_on_touch() {
    let mut b = ready(ArenaKind::Bumpers);
    b.f[0].hp = 40.0;
    b.spawn_item(ItemKind::Heal, [20.0, 0.0, 0.0], false);
    run(&mut b, 5);
    assert!(b.f[0].hp > 70.0);
}

fn give_gem(b: &mut Brawl, g: usize, to: usize) {
    b.gems[g] = Gem { state: GemState::Free, pos: b.f[to].pos, vel: [0.0; 3], spot: 0, immune: -1, immune_t: 0 };
    run(b, 2);
}

#[test]
fn three_gems_transform_and_power_form_burns_down() {
    let mut b = ready(ArenaKind::Bumpers);
    for g in 0..GEMS {
        give_gem(&mut b, g, 0);
        if g < 2 {
            assert!(!b.f[0].powered());
        }
    }
    assert!(b.f[0].powered(), "all three gems make the power form");
    assert_eq!(b.f[0].gems, 0, "the gems are used up");
    assert!(b.gems.iter().all(|g| g.state == GemState::Spent));
    assert!(b.f[0].transforms == 1);
    run(&mut b, 60);
    assert!(b.f[0].size > 1.2, "grows: {}", b.f[0].size);
    let mut ended = false;
    for _ in 0..(POWER_SECS * 30.0) as usize + 10 {
        b.tick(&idle());
        ended |= b.ev.iter().any(|e| matches!(e, Ev::PowerEnd(0)));
    }
    assert!(ended && !b.f[0].powered());
    run(&mut b, 60);
    assert!(b.f[0].size < 1.2, "shrinks back");
    run(&mut b, 200);
    assert!(b.gems.iter().any(|g| matches!(g.state, GemState::Free | GemState::Pending(_))), "a new batch of gems arrives");
}

#[test]
fn power_form_hits_harder_and_takes_less() {
    let mut b = ready(ArenaKind::Bumpers);
    b.f[0].power = 10.0;
    b.hit(1, 0, 10.0, [1.0, 0.0], 0.0, 0.0);
    assert!((b.f[1].hp - 85.0).abs() < 0.01, "x1.5 dealt: {}", b.f[1].hp);
    let mut c = ready(ArenaKind::Bumpers);
    c.f[1].power = 10.0;
    c.hit(1, 0, 10.0, [1.0, 0.0], 0.0, 0.0);
    assert!((c.f[1].hp - 93.0).abs() < 0.01, "x0.7 taken: {}", c.f[1].hp);
}

#[test]
fn nova_blast_hurts_everyone_around_and_costs_power() {
    let mut b = ready(ArenaKind::Bumpers);
    b.f[0].power = 10.0;
    b.f[0].size = POWER_SIZE;
    let mut inp = idle();
    inp[0].s1 = true;
    b.tick(&inp);
    assert_eq!(b.f[0].act, Act::Nova);
    run(&mut b, 25);
    assert!(b.f[1].hp < 80.0, "nova does big damage: {}", b.f[1].hp);
    assert!(b.f[0].power < 10.0 - NOVA_COST + 0.1 + 25.0 / 30.0 * 0.0 + 1.0);
}

#[test]
fn seeker_beam_finds_a_far_target() {
    let mut b = ready(ArenaKind::Bumpers);
    place(&mut b, 1, [-500.0, 400.0]);
    b.f[0].power = 10.0;
    let mut inp = idle();
    inp[0].s2 = true;
    b.tick(&inp);
    let mut saw = false;
    for _ in 0..20 {
        b.tick(&idle());
        saw |= b.ev.iter().any(|e| matches!(e, Ev::Beam { .. }));
    }
    assert!(saw);
    assert!(b.f[1].hp < 90.0, "{}", b.f[1].hp);
}

#[test]
fn a_hard_hit_drops_gems_which_can_be_taken() {
    let mut b = ready(ArenaKind::Bumpers);
    give_gem(&mut b, 0, 0);
    give_gem(&mut b, 1, 0);
    assert_eq!(b.f[0].gem_count(), 2);
    b.hit(0, 1, 12.0, [-1.0, 0.0], 100.0, 0.0);
    assert_eq!(b.f[0].gem_count(), 1, "one gem pops out of a solid hit");
    b.f[0].inv = 0;
    b.hit(0, 1, 5.0, [-1.0, 0.0], 800.0, 400.0);
    assert_eq!(b.f[0].gem_count(), 0, "a knock-down shakes them all loose");
    assert!(b.gems.iter().filter(|g| g.state == GemState::Free).count() >= 2);
    // The dropper cannot snap them straight back up, someone else can.
    let gp = [b.gems[0].pos[0], b.gems[0].pos[2]];
    place(&mut b, 1, gp);
    for _ in 0..40 {
        b.tick(&idle());
    }
    assert!(b.f[1].gem_count() > 0);
}

#[test]
fn ko_ends_the_round_and_best_of_three_ends_the_match() {
    let mut b = ready(ArenaKind::Bumpers);
    b.f[1].hp = 3.0;
    b.hit(1, 0, 6.0, [1.0, 0.0], 100.0, 0.0);
    assert!(!b.f[1].alive);
    b.tick(&idle());
    assert_eq!(b.phase, Phase::RoundEnd);
    assert_eq!(b.winner, 0);
    assert_eq!(b.f[0].wins, 1);
    run(&mut b, END_TICKS as usize + 2);
    assert_eq!(b.round, 2);
    assert_eq!(b.phase, Phase::Intro);
    assert!(b.f[1].alive && b.f[1].hp == 100.0 && b.f[0].wins == 1, "round reset keeps wins");
    run(&mut b, INTRO_TICKS as usize + 2);
    b.f[1].hp = 3.0;
    b.hit(1, 0, 6.0, [1.0, 0.0], 100.0, 0.0);
    run(&mut b, END_TICKS as usize + 3);
    assert_eq!(b.phase, Phase::MatchEnd);
    assert_eq!(b.match_winner, 0);
    assert_eq!(b.f[0].wins, 2);
}

#[test]
fn time_out_goes_to_the_healthier_fighter_and_ties_are_replayed() {
    let mut b = ready(ArenaKind::Bumpers);
    b.f[1].hp = 60.0;
    b.round_t = 0.01;
    run(&mut b, 2);
    assert_eq!(b.winner, 0);
    let mut c = ready(ArenaKind::Bumpers);
    c.round_t = 0.01;
    run(&mut c, 2);
    assert_eq!(c.winner, -1, "a draw awards nothing");
    run(&mut c, END_TICKS as usize + 2);
    assert_eq!(c.round, 2);
}

#[test]
fn falling_into_the_void_costs_health_and_respawns() {
    let mut b = ready(ArenaKind::Gearworks);
    b.f[0].pos = [0.0, -600.0, 2000.0];
    b.f[0].grounded = false;
    b.tick(&idle());
    assert!(b.f[0].hp < 100.0 && b.f[0].alive);
    assert!(b.f[0].pos[1] > -100.0, "put back on a spawn");
    assert!(b.f[0].inv > 30);
}

#[test]
fn lava_burns_fighters_standing_low_at_high_tide() {
    let mut b = ready(ArenaKind::Ember);
    place(&mut b, 0, [-600.0, 0.0]);
    b.arena.t = 17.0;
    let hp = b.f[0].hp;
    run(&mut b, 20);
    assert!(b.f[0].hp < hp, "burned: {}", b.f[0].hp);
    // The rocks stay safe.
    let mut c = ready(ArenaKind::Ember);
    place(&mut c, 0, [470.0, 470.0]);
    c.arena.t = 17.0;
    run(&mut c, 20);
    assert_eq!(c.f[0].hp, 100.0);
}

#[test]
fn bumper_pillars_fling_fighters() {
    let mut b = ready_with(ArenaKind::Bumpers);
    place(&mut b, 0, [150.0, 0.0]);
    let mut inp = idle();
    inp[0].mv = [-1.0, 0.0];
    for _ in 0..12 {
        b.tick(&inp);
    }
    assert!(b.f[0].hp < 100.0 || b.f[0].pos[0] > 120.0, "bounced off the centre pillar");
    assert!(dist2([b.f[0].pos[0], b.f[0].pos[2]], [0.0, 0.0]) > 100.0 + RADIUS - 1.0);
}

#[test]
fn cannon_lobs_shells_that_explode() {
    let mut b = ready(ArenaKind::Cannon);
    place(&mut b, 0, [-450.0, 450.0]);
    place(&mut b, 1, [450.0, -450.0]);
    b.arena.cannon_cd = 2;
    let mut fired = false;
    let mut boom = false;
    for _ in 0..90 {
        b.tick(&idle());
        fired |= b.ev.iter().any(|e| matches!(e, Ev::Cannon));
        boom |= b.ev.iter().any(|e| matches!(e, Ev::Boom { .. }));
    }
    assert!(fired && boom);
}

#[test]
fn wind_pushes_fighters_downwind() {
    let mut b = ready(ArenaKind::Gale);
    place(&mut b, 0, [-500.0, 500.0]);
    b.arena.t = 8.0;
    let x0 = b.f[0].pos[0];
    run(&mut b, 30);
    let dir = b.arena.wind_dir;
    assert!(b.arena.wind > 0.5);
    let moved = [(b.f[0].pos[0] - x0), 0.0];
    assert!(moved[0] * dir[0] >= -1.0, "pushed along the wind or not at all on a z gust");
}

#[test]
fn plank_bridge_drops_a_standing_fighter() {
    let mut b = ready(ArenaKind::Bridge);
    place(&mut b, 0, [0.0, 0.0]);
    place(&mut b, 1, [-700.0, 300.0]);
    let mut fell = false;
    for _ in 0..120 {
        b.f[0].inv = 99;
        b.tick(&idle());
        fell |= b.ev.iter().any(|e| matches!(e, Ev::Fell(0)));
    }
    assert!(fell, "the plank crumbled and dropped the fighter");
}

#[test]
fn riding_a_lift_carries_the_fighter() {
    let mut b = ready(ArenaKind::Gearworks);
    let i = b.arena.solids.iter().position(|s| matches!(s.mover, Mover::Lift { .. })).unwrap();
    let (x, z) = (b.arena.solids[i].x, b.arena.solids[i].z);
    place(&mut b, 0, [x, z]);
    b.f[0].inv = 0;
    let mut peak = 0.0f32;
    for _ in 0..105 {
        b.tick(&idle());
        peak = peak.max(b.f[0].pos[1]);
    }
    assert!(peak > 150.0, "rode up with the lift: {peak}");
}

#[test]
fn bot_walks_to_a_foe_and_fights() {
    let mut b = Brawl::new(Config { seed: 3, humans: 0, bots: 2, diff: Diff::Normal, arena: ArenaKind::Bumpers, ..Default::default() });
    let mut dealt = 0.0;
    for _ in 0..30 * 40 {
        b.tick(&idle());
        if b.phase != Phase::Fight && b.phase != Phase::Intro {
            break;
        }
    }
    for f in &b.f[..b.n] {
        dealt += f.dmg_dealt;
    }
    assert!(dealt > 20.0, "two bots must trade blows: {dealt}");
}

#[test]
fn bots_collect_gems_and_use_the_power_form_across_matches() {
    let mut transforms = 0;
    let mut gems = 0;
    for seed in 0..6 {
        let mut b = Brawl::new(Config { seed, humans: 0, bots: 2, diff: Diff::Hard, arena: ArenaKind::Gale, ..Default::default() });
        for _ in 0..30 * 90 {
            b.tick(&idle());
            if b.phase == Phase::MatchEnd {
                break;
            }
        }
        transforms += b.f[..b.n].iter().map(|f| f.transforms as u32).sum::<u32>();
        gems += b.f[..b.n].iter().map(|f| f.gems_got as u32).sum::<u32>();
    }
    assert!(gems >= 6, "bots go for gems: {gems}");
    assert!(transforms >= 1, "and sometimes complete the set: {transforms}");
}

#[test]
fn same_seed_same_match_different_seed_different_match() {
    let run_one = |seed: u64| {
        let mut b = Brawl::new(Config { seed, humans: 0, bots: 3, arena: ArenaKind::Cannon, ..Default::default() });
        for _ in 0..900 {
            b.tick(&idle());
        }
        b.digest()
    };
    assert_eq!(run_one(11), run_one(11));
    assert_ne!(run_one(11), run_one(12));
}

#[test]
fn bot_soak_every_arena_always_terminates_in_bounds() {
    for (k, arena) in ARENAS.into_iter().enumerate() {
        for bots in [2usize, 4] {
            let mut b = Brawl::new(Config { seed: 100 + k as u64, humans: 0, bots, diff: Diff::Normal, arena, ..Default::default() });
            let mut ticks = 0;
            while b.phase != Phase::MatchEnd {
                b.tick(&idle());
                ticks += 1;
                assert!(ticks < 30 * 60 * 14, "{arena:?} x{bots} never finished (round {}, wins {:?})", b.round, b.f.iter().map(|f| f.wins).collect::<Vec<_>>());
                for f in b.f[..b.n].iter().filter(|f| f.alive) {
                    assert!(f.pos.iter().all(|v| v.is_finite()), "{arena:?} nan");
                    assert!(f.pos[1] > b.arena.void_y - 3000.0 && f.pos[0].abs() < 4000.0 && f.pos[2].abs() < 4000.0, "{arena:?} fighter escaped {:?}", f.pos);
                }
            }
            assert!(b.match_winner >= 0);
        }
    }
}

#[test]
fn gauntlet_runs_start_to_finish_with_a_bot_in_the_hero_seat() {
    // Give the hero seat a Hard brain: the Gauntlet must reach a conclusion either way.
    let mut cleared = 0;
    for seed in 0..3u64 {
        let mut b = Brawl::new(Config { seed, mode: Mode::Gauntlet, humans: 1, bots: 0, ..Default::default() });
        b.brains[0] = Some(Brain::new(Diff::Hard, false, seed + 99));
        b.f[0].human = false;
        let mut ticks = 0;
        while b.phase != Phase::MatchEnd {
            // The bot hero must keep its brain across stages.
            if b.brains[0].is_none() {
                b.brains[0] = Some(Brain::new(Diff::Hard, false, seed + 5));
                b.f[0].human = false;
            }
            b.tick(&idle());
            ticks += 1;
            assert!(ticks < 30 * 60 * 40, "gauntlet never ended at stage {}", b.stage);
        }
        cleared += b.stage_clears;
        assert!(b.stage_clears <= GAUNTLET_STAGES as u32);
    }
    assert!(cleared >= 3, "a Hard bot should clear several stages in total: {cleared}");
}

#[test]
fn gauntlet_stages_escalate_and_end_with_the_boss() {
    let mut last_hp = 0.0;
    for s in 0..GAUNTLET_STAGES {
        let (_, bots, boss) = stage_def(s);
        let total: f32 = bots.iter().map(|(k, _)| k.stats().hp).sum();
        if s == GAUNTLET_STAGES - 1 {
            assert!(boss && bots[0].0 == Kind::KingBomb);
        } else {
            assert!(!boss);
        }
        assert!(total >= last_hp - 0.1 || s == GAUNTLET_STAGES - 1);
        last_hp = total;
    }
    let b = Brawl::new(Config { mode: Mode::Gauntlet, stage: GAUNTLET_STAGES - 1, humans: 1, bots: 0, ..Default::default() });
    assert!(b.f[1].max_hp > 300.0 && b.f[1].kind == Kind::KingBomb);
    assert!(b.f[1].team != b.f[0].team);
}

#[test]
fn allies_do_not_hurt_each_other_in_the_gauntlet() {
    let mut b = Brawl::new(Config { mode: Mode::Gauntlet, stage: 1, humans: 1, bots: 0, ..Default::default() });
    for _ in 0..INTRO_TICKS + 2 {
        b.tick(&idle());
    }
    b.f[1].inv = 0;
    assert!(!b.hit(2, 1, 10.0, [1.0, 0.0], 100.0, 0.0));
    assert!(b.hit(0, 1, 10.0, [1.0, 0.0], 100.0, 0.0));
}

#[test]
fn drops_arrive_from_the_sky_and_are_capped() {
    let mut b = ready(ArenaKind::Bumpers);
    b.item_timer = 1;
    run(&mut b, 3000);
    let live = b.items.iter().filter(|i| i.alive).count();
    assert!(live > 0 && live <= 10 + b.n, "{live}");
}

/// Balance report: Hard vs Normal vs Easy in 1v1 over every arena. `cargo test balance -- --nocapture`.
#[test]
fn balance_report_difficulty_ladder_and_mirror() {
    fn duel(a: Diff, c: Diff, seeds: u64) -> (u32, u32, u32) {
        let (mut aw, mut cw, mut draws) = (0, 0, 0);
        for seed in 0..seeds {
            for (k, arena) in ARENAS.into_iter().enumerate() {
                // Swap seats every other run so seat bias cancels out.
                let swap = (seed + k as u64) % 2 == 1;
                let (d0, d1) = if swap { (c, a) } else { (a, c) };
                let mut cfg = Config { seed: seed * 31 + k as u64, humans: 0, bots: 2, arena, same_kind: Some(Kind::Goomba), rounds_to_win: 1, ..Default::default() };
                cfg.diffs = [Some(d0), Some(d1), None, None];
                let mut b = Brawl::new(cfg);
                let mut ticks = 0;
                while b.phase != Phase::MatchEnd && ticks < 30 * 60 * 8 {
                    b.tick(&idle());
                    ticks += 1;
                }
                let w = b.match_winner;
                if w < 0 {
                    draws += 1;
                } else {
                    let a_won = (w == 0) != swap;
                    if a_won { aw += 1 } else { cw += 1 }
                }
            }
        }
        (aw, cw, draws)
    }
    let hn = duel(Diff::Hard, Diff::Normal, 10);
    let ne = duel(Diff::Normal, Diff::Easy, 10);
    let he = duel(Diff::Hard, Diff::Easy, 10);
    let nn = duel(Diff::Normal, Diff::Normal, 10);
    println!("BALANCE (60 duels each, 6 arenas x 10 seeds, one round to win, same fighter, seats swapped)");
    println!("  Hard   vs Normal: {}-{} ({} draws)", hn.0, hn.1, hn.2);
    println!("  Normal vs Easy  : {}-{} ({} draws)", ne.0, ne.1, ne.2);
    println!("  Hard   vs Easy  : {}-{} ({} draws)", he.0, he.1, he.2);
    println!("  Normal mirror   : {}-{} ({} draws) (seat A vs seat B)", nn.0, nn.1, nn.2);
    assert!(hn.0 > hn.1, "Hard must beat Normal");
    assert!(ne.0 > ne.1, "Normal must beat Easy");
    assert!(he.0 > he.1 * 2, "Hard must crush Easy");
}
