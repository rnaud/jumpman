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
    // Shared: the one who has stood in it longest keeps the crown and scores at half rate; the newcomer nothing.
    b[1] = body([c[0] + 50.0, c[1], c[2]]);
    let before = g.scores();
    for _ in 0..30 {
        g.step(DT, &b, &[Drive::default(); 4]);
    }
    assert!(g.contested);
    assert!((g.scores()[0] - before[0] - summit_crown::CONTESTED_RATE).abs() < 0.05, "half rate while contested: {:?}", g.scores());
    assert_eq!(g.scores()[1], before[1]);
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
fn the_roaming_crown_moves_its_circle_round_the_summit() {
    let f = flat(Mini::SummitCrown);
    let seed = (0..64u64).map(|k| k << 7 | 1).find(|s| (s >> 7) & 1 == 1).unwrap();
    let mut g = summit_crown::SummitCrown::new(seed, ALL4, f.clone());
    assert!(g.roam && g.twist().is_some());
    let b = [body(f.center); 4];
    let mut spots = vec![g.zone];
    for _ in 0..(30.0 * 30.0) as usize {
        g.step(DT, &b, &[Drive::default(); 4]);
        if hdist(*spots.last().unwrap(), g.zone) > 1.0 {
            spots.push(g.zone);
        }
    }
    assert!(spots.len() >= 4, "the circle moved {} times", spots.len() - 1);
    assert!(spots.iter().all(|z| hdist(*z, f.center) < 300.0 && f.on(*z)));
    assert!(summit_crown::SummitCrown::new(0, ALL4, f).twist().is_none(), "half the games keep the plain crown");
}

#[test]
fn trees_push_bodies_out_and_bots_run_round_them() {
    let solid = Solid { pos: [0.0, 0.0, 0.0], r: 80.0, h: 500.0 };
    // Inside the trunk: pushed to its surface; clear of it: left alone; well above it: left alone.
    let q = push_out(&[solid], [30.0, 0.0, 0.0]).unwrap();
    assert!((q[0] - (80.0 + BODY_R)).abs() < 1e-6 && q[2].abs() < 1e-6);
    assert!(push_out(&[solid], [200.0, 0.0, 0.0]).is_none());
    assert!(push_out(&[solid], [30.0, 900.0, 0.0]).is_none());
    // Running straight at it from 250 away: the direction bends off to one side; past it or well wide: unchanged.
    let d = steer_around(&[solid], [-250.0, 0.0, 10.0], [1.0, 0.0]);
    assert!(d[1].abs() > 0.3, "steered round: {d:?}");
    assert!(d[1] > 0.0, "round the side it is already on");
    assert_eq!(steer_around(&[solid], [250.0, 0.0, 0.0], [1.0, 0.0]), [1.0, 0.0]);
    assert_eq!(steer_around(&[solid], [-250.0, 0.0, 400.0], [1.0, 0.0]), [1.0, 0.0]);
    // Every game with trees on the field: no body is ever left inside a trunk, and the starts and coins stay clear of them.
    for kind in ALL_MINIS {
        let base = Field::flat([0.0; 3], kind.radius());
        let trees: Vec<Solid> = [[0.0, 0.0], [300.0, 200.0], [-350.0, -150.0], [100.0, -400.0]].iter().map(|p| Solid { pos: [p[0], 0.0, p[1]], r: 80.0, h: 500.0 }).collect();
        let f = Arc::new(base.with_solids(&trees));
        assert!(f.points().iter().all(|p| trees.iter().all(|t| hdist(*p, t.pos) > t.r + BODY_R)));
        let mut s = Session::new(kind, 11, ALL4, [false; 4], [Level::Normal; 4], f.clone());
        while !s.done() && s.steps < 30 * 90 {
            s.step_kin(&[Drive::default(); 4]);
            for b in s.kin.bodies() {
                if b.pos[1] > -200.0 {
                    assert!(trees.iter().all(|t| hdist(b.pos, t.pos) >= t.r + BODY_R - 1e-6), "{kind:?}: a body inside a tree at {:?}", b.pos);
                }
            }
        }
    }
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

/// Bodies never pass through each other: in every game the bots' bodies (they crowd the same coin, the crown, the star) stay
/// two body radii apart, on the kinematic stand-in as on the real physics (`physics_tests`).
#[test]
fn marios_do_not_pass_through_each_other() {
    for kind in ALL_MINIS {
        let f = flat(kind);
        for seed in 0..4u64 {
            let mut s = Session::new(kind, seed + 3, ALL4, [false; 4], [Level::Hard; 4], f.clone());
            let mut worst = 0.0f64;
            while !s.done() && s.steps < 30 * 90 {
                s.step_kin(&[Drive::default(); 4]);
                let b = s.kin.bodies();
                for x in 0..4 {
                    for y in x + 1..4 {
                        // Bodies over the field's floor (one going over the edge is falling, not standing).
                        let over = |p: [f64; 3]| s.kin.field.height(p[0], p[2]).is_some();
                        if (b[x].pos[1] - b[y].pos[1]).abs() < 100.0 && over(b[x].pos) && over(b[y].pos) {
                            let o = 2.0 * BODY_R - hdist(b[x].pos, b[y].pos);
                            if o > 2.0 && std::env::var_os("PARTY_OVERLAP").is_some() {
                                eprintln!("{kind:?} step {} bodies {x} {y}: {:?} {:?} heights {:?} {:?}", s.steps, b[x].pos, b[y].pos, s.kin.field.height(b[x].pos[0], b[x].pos[2]), s.kin.field.height(b[y].pos[0], b[y].pos[2]));
                            }
                            worst = worst.max(o);
                        }
                    }
                }
            }
            // A push that would put a body off the field's floor or into a trunk is not made, so a little overlap can stay at
            // the summit's edge.
            assert!(worst < 10.0, "{kind:?} seed {seed}: two bodies overlapped by {worst:.1}");
        }
    }
}

#[test]
fn separating_bodies_pushes_overlaps_apart_half_each() {
    let pos = [[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [500.0, 0.0, 0.0], [20.0, 400.0, 0.0]];
    let m = separate_bodies_fixed(&pos, &[true; 4], &[true, false, false, false]);
    assert!(m[0] == [0.0, 0.0] && (m[1][0] - 54.0).abs() < 1e-6, "the free body takes the whole push: {m:?}");
    let m = separate_bodies(&pos, &[true; 4]);
    assert!((m[0][0] + 27.0).abs() < 1e-6 && (m[1][0] - 27.0).abs() < 1e-6, "{m:?}");
    assert_eq!(m[2], [0.0, 0.0]);
    assert_eq!(m[3], [0.0, 0.0], "a body far above is not pushed");
    let m = separate_bodies(&pos, &[true, false, true, true]);
    assert_eq!(m[0], [0.0, 0.0], "a seat out of the game is left alone");
}

/// Goombas are solid: one never walks through a Mario standing on the ground, even in the second after a bump when it cannot
/// hurt him again.
#[test]
fn goombas_do_not_walk_through_marios() {
    let f = flat(Mini::GoombaStomp);
    let mut g = goomba_stomp::GoombaStomp::new(4, ALL4, f.clone());
    let c = f.center;
    // Four Marios standing still in the Goombas' way.
    let b: [Body; 4] = std::array::from_fn(|p| body([c[0] + (p as f64 - 1.5) * 300.0, c[1], c[2]]));
    let mut worst: f64 = 0.0;
    for _ in 0..30 * 20 {
        g.step(DT, &b, &[Drive::default(); 4]);
        let _ = g.events();
        for m in g.goombas.iter().filter(|m| m.dead.is_none()) {
            for x in &b {
                worst = worst.max(goomba_stomp::GOOMBA_BODY - hdist(m.pos, x.pos));
            }
        }
    }
    assert!(worst < 1e-6, "a Goomba stood {worst:.1} inside a Mario");
}
