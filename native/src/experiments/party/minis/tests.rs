use super::*;

const ALL4: [bool; 4] = [true; 4];

fn flat(m: Mini) -> Arc<Field> {
    Arc::new(Field::flat([1000.0, 500.0, -2000.0], m.radius()))
}

fn body(pos: [f64; 3]) -> Body {
    Body { pos, grounded: true, ..Default::default() }
}

#[test]
fn catalogue_has_unique_keys_and_every_game_says_where_it_is_played() {
    let mut keys: Vec<_> = ALL_MINIS.iter().map(|m| m.info().key).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), ALL_MINIS.len());
    for m in ALL_MINIS {
        let i = m.info();
        assert!(!i.blurb.is_empty() && !i.place.is_empty());
        assert_eq!(Mini::from_key(i.key), Some(m));
        assert_eq!(ALL_MINIS[m.idx()], m);
    }
    assert_eq!(Mini::from_key("Summit Crown"), Some(Mini::SummitCrown));
}

#[test]
fn every_game_terminates_under_bot_vs_bot_soak_in_every_format() {
    let formats: [[bool; 4]; 3] = [ALL4, [true, true, false, false], [false, true, false, true]];
    for m in ALL_MINIS {
        let f = flat(m);
        for seed in 0..6u64 {
            for lv in Level::ALL {
                for a in formats {
                    let (s, secs) = run_bots(m, seed * 7 + 1, a, [lv; 4], &f);
                    assert!(secs <= 40.0, "{m:?} ran {secs}s");
                    for p in 0..4 {
                        if a[p] {
                            assert!(s[p].is_finite() && s[p] >= 0.0, "{m:?} score for active {p}: {}", s[p]);
                        } else {
                            assert!(s[p] == f32::NEG_INFINITY, "{m:?} inactive {p} must not score");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn games_are_deterministic_by_seed() {
    for m in ALL_MINIS {
        let f = flat(m);
        let a = run_bots(m, 99, ALL4, [Level::Normal; 4], &f);
        let b = run_bots(m, 99, ALL4, [Level::Normal; 4], &f);
        assert_eq!(a, b, "{m:?}");
    }
}

#[test]
fn bots_actually_play_every_game() {
    // Normal bots score in every game (they collect, hold, survive, steal and stomp), and not all the same.
    for m in ALL_MINIS {
        let f = flat(m);
        let mut total = 0.0;
        let mut spread = 0.0;
        for seed in 0..8u64 {
            let (s, _) = run_bots(m, seed + 40, ALL4, [Level::Normal; 4], &f);
            total += s.iter().sum::<f32>();
            spread += s.iter().cloned().fold(f32::MIN, f32::max) - s.iter().cloned().fold(f32::MAX, f32::min);
        }
        assert!(total > 8.0, "{m:?}: bots scored {total}");
        assert!(spread > 1.0, "{m:?}: every game ends in a dead heat ({spread})");
    }
}

#[test]
fn idle_people_lose_to_bots() {
    // A player who never touches the controls must not beat three Normal bots on average.
    for m in ALL_MINIS {
        let f = flat(m);
        let mut idle_wins = 0;
        let n = 12;
        for seed in 0..n {
            let mut s = Session::new(m, seed + 3, ALL4, [true, false, false, false], [Level::Normal; 4], f.clone());
            let none = [Drive::default(); 4];
            while !s.done() && s.steps < 30 * 60 {
                s.step_kin(&none);
            }
            if s.ranks()[0] == 1 {
                idle_wins += 1;
            }
        }
        assert!(idle_wins <= n / 3, "{m:?}: an idle player wins {idle_wins}/{n}");
    }
}

#[test]
fn harder_bots_beat_easier_bots() {
    // Seat 0 plays Hard against three Easy bots: clearly more than a fair quarter of the wins, in every game.
    for m in ALL_MINIS {
        let f = flat(m);
        let mut wins = 0.0;
        let n = 16;
        for seed in 0..n as u64 {
            let (s, _) = run_bots(m, seed + 11, ALL4, [Level::Hard, Level::Easy, Level::Easy, Level::Easy], &f);
            let r = ranks(&s, &ALL4);
            if r[0] == 1 {
                wins += 1.0 / (0..4).filter(|&q| r[q] == 1).count() as f32;
            }
        }
        assert!(wins / n as f32 > 0.3, "{m:?}: the Hard seat wins only {wins}/{n}");
    }
}

#[test]
fn fields_keep_the_floor_connected_to_the_middle() {
    // A plateau at 1000 with a pit (no floor) ring and an outer ledge at 0: the ledge is not part of the arena.
    let f = Field::from_heights([0.0, 0.0], 900.0, 100.0, |x, z| {
        let r = x.hypot(z);
        if r < 500.0 {
            Some(1000.0)
        } else if r < 650.0 {
            None
        } else {
            Some(0.0)
        }
    });
    assert_eq!(f.center[1], 1000.0);
    assert!(f.height(0.0, 300.0).is_some() && f.height(0.0, 800.0).is_none());
    assert!(f.points().iter().all(|p| p[1] == 1000.0));
    assert!(f.fallen([0.0, 200.0, 800.0]) && !f.fallen([0.0, 1000.0, 0.0]));
    // Kinematic bodies walk off the edge and fall.
    let mut k = Kin::new(Arc::new(f), [[0.0, 1000.0, 400.0]; 4]);
    for _ in 0..40 {
        k.step(&[Drive { dir: Some([0.0, 1.0]), mag: 1.0, ..Default::default() }; 4]);
    }
    assert!(k.bodies()[0].pos[1] < 600.0);
}

#[test]
fn coins_go_to_whoever_touches_them_and_gold_is_three() {
    let f = flat(Mini::CoinScramble);
    let mut g = coin_scramble::CoinScramble::new(1, ALL4, f.clone());
    let c = g.coins[0].clone();
    let mut bodies = [body([f.center[0] + 2000.0, 0.0, 0.0]); 4];
    bodies[2] = body(c.pos);
    g.step(DT, &bodies, &[Drive::default(); 4]);
    assert_eq!(g.scores()[2], if c.gold { 3.0 } else { 1.0 });
    assert_eq!(g.coins.len(), 9, "a taken coin is replaced");
    assert!(g.events().iter().any(|e| matches!(e, MiniEv::Cue { .. })));
}

#[test]
fn the_crown_scores_only_alone_and_punches_knock_rivals_out_of_it() {
    let f = flat(Mini::SummitCrown);
    let mut g = summit_crown::SummitCrown::new(1, ALL4, f.clone());
    let c = f.center;
    let far = |k: f64| body([c[0] + 500.0, c[1], c[2] + k]);
    let mut b = [body(c), far(-100.0), far(0.0), far(100.0)];
    for _ in 0..30 {
        g.step(DT, &b, &[Drive::default(); 4]);
    }
    assert!((g.scores()[0] - 1.0).abs() < 0.05, "alone in the circle for a second: {}", g.scores()[0]);
    // Shared: nobody scores.
    b[1] = body([c[0] + 50.0, c[1], c[2]]);
    let before = g.scores();
    for _ in 0..30 {
        g.step(DT, &b, &[Drive::default(); 4]);
    }
    assert_eq!(g.scores()[..2], before[..2]);
    // Seat 1 punches seat 0 (facing +x toward... seat 0 is at -x of seat 1: face -x).
    b[1].attack = true;
    b[1].yaw = -16384.0;
    g.step(DT, &b, &[Drive::default(); 4]);
    assert!(g.events().iter().any(|e| matches!(e, MiniEv::Knock { p: 0, .. })));
    // A fall off the summit is carried back up.
    b[3].pos[1] = f.low - 100.0;
    let mut placed = false;
    for _ in 0..60 {
        g.step(DT, &b, &[Drive::default(); 4]);
        placed |= g.events().iter().any(|e| matches!(e, MiniEv::Place { p: 3, .. }));
        if placed {
            break;
        }
    }
    assert!(placed && g.falls[3] == 1);
}

#[test]
fn a_blast_costs_a_heart_and_three_put_you_out() {
    let f = flat(Mini::BombRain);
    let mut g = bomb_rain::BombRain::new(3, [true, true, false, false], f.clone());
    let c = f.center;
    let b = [body(c), body([c[0] + 700.0, c[1], c[2]]), body(c), body(c)];
    let mut hearts_lost = 0;
    for _ in 0..3 {
        g.bombs.clear();
        g.bombs.push(bomb_rain::Bomb { pos: c, age: 1.3, fuse: 1.35 });
        for _ in 0..60 {
            g.bombs.retain(|bb| hdist(bb.pos, c) < 10.0);
            g.step(DT, &b, &[Drive::default(); 4]);
        }
        hearts_lost += 1;
        assert_eq!(g.hearts[0], bomb_rain::HEARTS - hearts_lost);
    }
    assert!(g.out(0) && g.done(), "seat 0 is out, seat 1 is the last one standing");
    let s = g.scores();
    assert!(s[1] > s[0]);
}

#[test]
fn punching_the_star_holder_steals_the_star() {
    let f = flat(Mini::StarTag);
    let mut g = star_tag::StarTag::new(1, ALL4, f.clone());
    let c = g.loose;
    let mut b = [body(c), body([c[0] + 125.0, c[1], c[2]]), body([c[0] - 600.0, c[1], c[2]]), body([c[0], c[1], c[2] + 600.0])];
    g.step(DT, &b, &[Drive::default(); 4]);
    assert_eq!(g.holder, Some(0), "the nearest Mario takes the star");
    assert!(g.speed(0) < 1.0 && g.speed(1) == 1.0);
    // Too soon: the holder is guarded.
    b[1].attack = true;
    b[1].yaw = -16384.0;
    g.step(DT, &b, &[Drive::default(); 4]);
    assert_eq!(g.holder, Some(0));
    for _ in 0..40 {
        g.step(DT, &b, &[Drive::default(); 4]);
    }
    assert_eq!(g.holder, Some(1));
    assert_eq!(g.steals[1], 1);
    assert!(g.scores()[0] > 1.0);
}

#[test]
fn stomps_score_and_chain_and_walking_into_a_goomba_hurts() {
    let f = flat(Mini::GoombaStomp);
    let mut g = goomba_stomp::GoombaStomp::new(1, ALL4, f.clone());
    for m in &mut g.goombas {
        m.age = 1.0;
    }
    let gp = g.goombas[0].pos;
    let mut b = [body([gp[0], gp[1] + 80.0, gp[2]]), body([f.center[0] + 2000.0, 0.0, 0.0]), body([f.center[0] + 2000.0, 0.0, 0.0]), body([f.center[0] + 2000.0, 0.0, 0.0])];
    b[0].grounded = false;
    b[0].vy = -20.0;
    g.step(DT, &b, &[Drive::default(); 4]);
    assert_eq!(g.scores()[0], 1.0);
    assert!(g.events().contains(&MiniEv::Bounce { p: 0 }));
    // Still in the air, onto a second one: a chained stomp is worth two.
    let second = g.goombas.iter().find(|m| m.dead.is_none()).unwrap().pos;
    b[0].pos = [second[0], second[1] + 80.0, second[2]];
    g.step(DT, &b, &[Drive::default(); 4]);
    assert_eq!(g.scores()[0], 3.0);
    let _ = g.events();
    // Walking into another one: a knock and a point lost.
    let other = g.goombas.iter().find(|m| m.dead.is_none()).unwrap().pos;
    b[0] = body(other);
    g.step(DT, &b, &[Drive::default(); 4]);
    assert_eq!(g.scores()[0], 2.0);
    assert!(g.events().iter().any(|e| matches!(e, MiniEv::Knock { p: 0, .. })));
}

/// `cargo test --release balance_report -- --ignored --nocapture`
#[test]
#[ignore]
fn balance_report() {
    println!("{:<14} {:>6} {:>6} {:>6} {:>6} | {:>6}  | Hard seat0 vs Easy", "game", "seat0", "seat1", "seat2", "seat3", "len");
    for m in ALL_MINIS {
        let f = flat(m);
        let mut wins = [0.0f32; 4];
        let mut len = 0.0;
        let n = 80;
        for seed in 0..n as u64 {
            let (s, secs) = run_bots(m, seed * 13 + 5, ALL4, [Level::Normal; 4], &f);
            let r = ranks(&s, &ALL4);
            let k = (0..4).filter(|&q| r[q] == 1).count() as f32;
            for p in 0..4 {
                if r[p] == 1 {
                    wins[p] += 1.0 / k;
                }
            }
            len += secs;
        }
        let mut hard = 0.0;
        for seed in 0..n as u64 {
            let (s, _) = run_bots(m, seed * 17 + 2, ALL4, [Level::Hard, Level::Easy, Level::Easy, Level::Easy], &f);
            let r = ranks(&s, &ALL4);
            if r[0] == 1 {
                hard += 1.0 / (0..4).filter(|&q| r[q] == 1).count() as f32;
            }
        }
        println!("{:<14} {:>5.0}% {:>5.0}% {:>5.0}% {:>5.0}% | {:>5.1}s | {:>4.0}%", m.info().name, wins[0] / n as f32 * 100.0, wins[1] / n as f32 * 100.0, wins[2] / n as f32 * 100.0, wins[3] / n as f32 * 100.0, len / n as f32, hard / n as f32 * 100.0);
    }
}
