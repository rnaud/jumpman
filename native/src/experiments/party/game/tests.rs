use super::*;

fn cpu_cfg(board: usize, turns: u32, seed: u64) -> Config {
    Config { board, turns, seed, players: [Some(Level::Normal); 4] }
}

fn play(board: usize, turns: u32, seed: u64) -> Party {
    let mut p = Party::new(cpu_cfg(board, turns, seed));
    p.autoplay();
    p
}

fn signature(p: &Party) -> Vec<i64> {
    let mut v = vec![p.round as i64, p.minis_played as i64];
    for pl in &p.players {
        v.extend([pl.coins as i64, pl.sun as i64, pl.space as i64, pl.wins as i64, pl.earned as i64]);
    }
    v
}

#[test]
fn every_board_reaches_the_end_under_autoplay_for_every_length() {
    for board in 0..BOARD_COUNT {
        for turns in [5, 6, 8] {
            for seed in 0..25u64 {
                let p = play(board, turns, seed * 31 + 7);
                assert!(p.over(), "board {board} turns {turns} seed {seed}");
                assert_eq!(p.round, turns + 1);
                assert_eq!(p.minis_played, turns + p.events_duels());
                assert_eq!(p.awards.len(), 3);
                for pl in &p.players {
                    assert!(pl.coins >= 0);
                    assert!(pl.items.len() <= MAX_ITEMS);
                }
            }
        }
    }
}

impl Party {
    /// Duels played (test helper): total mini-games minus one per round.
    fn events_duels(&self) -> u32 {
        self.minis_played - self.round.saturating_sub(1).min(self.minis_played)
    }
}

#[test]
fn games_are_deterministic_by_seed_and_differ_between_seeds() {
    for board in 0..BOARD_COUNT {
        let a = signature(&play(board, 12, 77));
        let b = signature(&play(board, 12, 77));
        assert_eq!(a, b, "board {board}");
        let c = signature(&play(board, 12, 78));
        assert_ne!(a, c, "board {board}");
    }
}

#[test]
fn sunstones_are_conserved_between_purchases_and_bonuses() {
    for board in 0..BOARD_COUNT {
        let mut bought = 0;
        let mut p = Party::new(cpu_cfg(board, 12, 5));
        while p.auto_step() {
            for e in p.drain_events() {
                if let Ev::Sunstone { .. } = e {
                    bought += 1;
                }
            }
        }
        for e in p.drain_events() {
            if let Ev::Sunstone { .. } = e {
                bought += 1;
            }
        }
        let bonuses: u32 = p.players.iter().map(|x| x.bonus).sum();
        let total: u32 = p.players.iter().map(|x| x.sun).sum();
        assert_eq!(total, bought + bonuses);
        assert!(bought > 0, "nobody bought a Sunstone on board {board}");
    }
}

fn at_keeper(p: &mut Party, who: usize, coins: i32) {
    p.cur = who;
    p.players[who].space = p.keeper;
    p.players[who].coins = coins;
    p.phase = Phase::KeeperOffer;
    p.moves_left = 0;
    p.rolled = true;
}

#[test]
fn the_sunstone_costs_fifteen_and_the_keeper_moves_after_a_sale() {
    let mut p = Party::new(Config::default());
    let before = p.keeper;
    at_keeper(&mut p, 0, SUNSTONE_COST - 1);
    assert!(p.apply(Cmd::Keeper(true)).is_err());
    p.players[0].coins = SUNSTONE_COST + 5;
    p.apply(Cmd::Keeper(true)).unwrap();
    assert_eq!(p.players[0].coins, 5);
    assert_eq!(p.players[0].sun, 1);
    assert_ne!(p.keeper, before);
    assert!(p.board.sites.contains(&p.keeper));
    // The turn moved on to the next seat.
    assert_eq!(p.cur, 1);
    assert!(matches!(p.phase, Phase::TurnStart));
}

#[test]
fn declining_the_keeper_keeps_the_coins() {
    let mut p = Party::new(Config::default());
    let k = p.keeper;
    at_keeper(&mut p, 0, 30);
    p.apply(Cmd::Keeper(false)).unwrap();
    assert_eq!(p.players[0].coins, 30);
    assert_eq!(p.players[0].sun, 0);
    assert_eq!(p.keeper, k);
}

#[test]
fn passing_the_keeper_with_enough_coins_pauses_the_walk() {
    let mut p = Party::new(Config::default());
    // Find the space two before the keeper along the ring and walk 5.
    let k = p.keeper;
    let mut prev = k;
    for _ in 0..2 {
        prev = (0..p.board.spaces.len()).find(|&s| p.board.edges[s].iter().any(|e| e.to == prev && e.rule == Rule::Always)).unwrap();
    }
    p.players[0].space = prev;
    p.players[0].coins = SUNSTONE_COST + 5;
    p.apply(Cmd::Roll(Some(5))).unwrap();
    assert!(matches!(p.phase, Phase::KeeperOffer), "{:?}", p.phase);
    assert_eq!(p.players[0].space, k);
    assert_eq!(p.moves_left, 3);
    p.apply(Cmd::Keeper(false)).unwrap();
    // The walk resumed and finished, then the turn passed on (or a decision awaits at the new space).
    assert_eq!(p.moves_left, 0);
}

#[test]
fn items_work_and_the_hand_is_limited() {
    let mut p = Party::new(Config::default());
    p.players[0].items = vec![Item::Twin];
    p.apply(Cmd::Item(0, None)).unwrap();
    assert_eq!(p.die, Die::Twin);
    assert!(p.apply(Cmd::Item(0, None)).is_err());
    p.players[0].coins = 40;
    let start = p.players[0].space;
    p.apply(Cmd::Roll(Some(12))).unwrap();
    assert_ne!(p.players[0].space, start);

    // Pickpocket takes at most 6.
    let mut p = Party::new(Config::default());
    p.players[0].items = vec![Item::Pickpocket];
    p.players[2].coins = 20;
    let me = p.players[0].coins;
    p.apply(Cmd::Item(0, Some(2))).unwrap();
    assert_eq!(p.players[2].coins, 14);
    assert_eq!(p.players[0].coins, me + 6);

    // Whistle warps to the keeper.
    let mut p = Party::new(Config::default());
    p.players[0].items = vec![Item::Whistle];
    p.apply(Cmd::Item(0, None)).unwrap();
    assert_eq!(p.players[0].space, p.keeper);
    assert!(matches!(p.phase, Phase::TurnStart));
    // With coins it offers the Sunstone first, then lets you roll.
    let mut p = Party::new(Config::default());
    p.players[0].items = vec![Item::Whistle];
    p.players[0].coins = SUNSTONE_COST + 3;
    p.apply(Cmd::Item(0, None)).unwrap();
    assert!(matches!(p.phase, Phase::KeeperOffer));
    p.apply(Cmd::Keeper(true)).unwrap();
    assert!(matches!(p.phase, Phase::TurnStart));
    assert_eq!(p.cur, 0);
    assert!(!p.rolled);
}

#[test]
fn shop_sells_to_the_budget_and_to_two_slots() {
    let mut p = Party::new(Config::default());
    p.cur = 0;
    p.players[0].coins = 30;
    p.phase = Phase::ShopMenu;
    p.apply(Cmd::ShopBuy(Item::Whistle)).unwrap();
    assert_eq!(p.players[0].coins, 20);
    p.apply(Cmd::ShopBuy(Item::Dash)).unwrap();
    assert_eq!(p.apply(Cmd::ShopBuy(Item::Twin)), Err("hands full"));
    p.players[0].items.clear();
    p.players[0].coins = 3;
    assert_eq!(p.apply(Cmd::ShopBuy(Item::Twin)), Err("not enough coins"));
    p.apply(Cmd::ShopLeave).unwrap();
    assert_eq!(p.cur, 1);
}

#[test]
fn the_die_obeys_its_range_and_slow_boots_cap_the_roll() {
    let mut p = Party::new(Config::default());
    p.players[0].slow = true;
    p.apply(Cmd::Roll(Some(9))).unwrap();
    let rolled = p.events.iter().find_map(|e| if let Ev::Rolled { value, .. } = e { Some(*value) } else { None }).unwrap();
    assert_eq!(rolled, 3);
    let mut p = Party::new(Config::default());
    p.apply(Cmd::Roll(Some(200))).unwrap();
    let rolled = p.events.iter().find_map(|e| if let Ev::Rolled { value, .. } = e { Some(*value) } else { None }).unwrap();
    assert_eq!(rolled, 10);
}

#[test]
fn mini_formats_follow_the_space_tints() {
    for (tints, format) in [([true; 4], Format::Ffa), ([false; 4], Format::Ffa), ([true, true, false, false], Format::TwoVsTwo), ([true, false, false, false], Format::OneVsThree), ([true, true, true, false], Format::OneVsThree)] {
        let mut p = Party::new(Config::default());
        for i in 0..4 {
            p.players[i].tint = tints[i];
        }
        p.phase = Phase::ShopMenu;
        p.cur = 3;
        p.apply(Cmd::ShopLeave).unwrap();
        let Phase::Mini(ctx) = p.phase.clone() else { panic!("expected a mini-game, got {:?}", p.phase) };
        assert_eq!(ctx.format, format, "{tints:?}");
        if format == Format::OneVsThree {
            assert_eq!((0..4).filter(|&i| ctx.teams[i] == 0).count(), 1, "the lone player is team 0");
        }
    }
}

#[test]
fn mini_rewards_by_format() {
    let ctx = |format, teams| MiniCtx { kind: Mini::CoinScramble, format, active: [true; 4], teams, round_end: true, seed: 1 };
    // Free-for-all: 8 / 4 / 0 / 0.
    let mut p = Party::new(Config::default());
    p.finish_mini(ctx(Format::Ffa, [0, 1, 2, 3]), [3.0, 9.0, 5.0, 1.0]);
    let coins: Vec<i32> = (0..4).map(|i| p.players[i].coins - START_COINS).collect();
    assert_eq!(coins, vec![0, 8, 4, 0]);
    assert_eq!(p.players[1].wins, 1);
    // 2v2: winners +8 each.
    let mut p = Party::new(Config::default());
    p.finish_mini(ctx(Format::TwoVsTwo, [0, 0, 1, 1]), [9.0, 8.0, 1.0, 2.0]);
    let coins: Vec<i32> = (0..4).map(|i| p.players[i].coins - START_COINS).collect();
    assert_eq!(coins, vec![6, 6, 0, 0]);
    // 1v3: the solo player wins +12, the trio +6 each.
    let mut p = Party::new(Config::default());
    p.finish_mini(ctx(Format::OneVsThree, [0, 1, 1, 1]), [9.0, 2.0, 2.0, 2.0]);
    assert_eq!(p.players[0].coins - START_COINS, 10);
    let mut p = Party::new(Config::default());
    p.finish_mini(ctx(Format::OneVsThree, [0, 1, 1, 1]), [1.0, 5.0, 5.0, 5.0]);
    assert_eq!((p.players[0].coins - START_COINS, p.players[1].coins - START_COINS), (0, 5));
    // Everyone tied: a small consolation, no winner.
    let mut p = Party::new(Config::default());
    p.finish_mini(ctx(Format::Ffa, [0, 1, 2, 3]), [4.0; 4]);
    assert!(p.players.iter().all(|x| x.coins == START_COINS + 4 && x.wins == 0));
    // Duel: the winner takes up to 5 from the loser.
    let mut p = Party::new(Config::default());
    p.players[2].coins = 3;
    let mut c = ctx(Format::Duel, [0, 1, 2, 3]);
    c.active = [true, false, true, false];
    c.round_end = false;
    p.finish_mini(c, [9.0, 0.0, 1.0, 0.0]);
    assert_eq!((p.players[0].coins, p.players[2].coins), (START_COINS + 3, 0));
}

#[test]
fn a_duel_space_asks_for_a_rival_and_plays_a_two_player_game() {
    let mut p = Party::new(Config::default());
    p.phase = Phase::DuelPick;
    assert!(p.apply(Cmd::DuelPick(0)).is_err());
    p.apply(Cmd::DuelPick(2)).unwrap();
    let Phase::Mini(ctx) = p.phase.clone() else { panic!() };
    assert_eq!(ctx.format, Format::Duel);
    assert_eq!(ctx.active, [true, false, true, false]);
    assert!(!ctx.round_end);
    p.apply(Cmd::MiniDone([1.0, 0.0, 5.0, 0.0])).unwrap();
    assert!(matches!(p.phase, Phase::MiniResult(_)));
    p.apply(Cmd::Ack).unwrap();
    assert_eq!(p.cur, 1, "play passes to the next seat after the duel");
}

#[test]
fn the_drawbridge_flips_every_round_and_changes_the_fork() {
    let mut p = Party::new(Config { board: 0, ..Config::default() });
    let flips: Vec<bool> = {
        let mut v = vec![p.cond.flip];
        for _ in 0..4 {
            p.phase = Phase::MiniResult(MiniReport { kind: Mini::StarTag, format: Format::Ffa, active: [true; 4], teams: [0, 1, 2, 3], scores: [0.0; 4], ranks: [1; 4], won: [false; 4], coins: [0; 4], round_end: true });
            p.apply(Cmd::Ack).unwrap();
            v.push(p.cond.flip);
        }
        v
    };
    assert_eq!(flips, vec![false, true, false, true, false]);
}

#[test]
fn wind_shortcuts_open_only_with_the_wind() {
    let b = board(1);
    let fork = (0..b.spaces.len()).find(|&s| b.edges[s].iter().any(|e| e.rule == Rule::Wind)).unwrap();
    assert_eq!(b.out(fork, &Conditions { wind: false, ..Default::default() }).len(), 1);
    assert_eq!(b.out(fork, &Conditions { wind: true, ..Default::default() }).len(), 2);
}

#[test]
fn the_tide_floods_the_lowest_ring_from_round_four() {
    let mut p = Party::new(Config { board: 2, ..Config::default() });
    let low = (0..p.board.spaces.len()).find(|&s| p.board.spaces[s].tier == 0 && p.board.spaces[s].kind == Kind::Coin).unwrap();
    assert_eq!(p.space_kind(low), Kind::Coin);
    for _ in 0..3 {
        p.phase = Phase::MiniResult(MiniReport { kind: Mini::StarTag, format: Format::Ffa, active: [true; 4], teams: [0, 1, 2, 3], scores: [0.0; 4], ranks: [1; 4], won: [false; 4], coins: [0; 4], round_end: true });
        p.apply(Cmd::Ack).unwrap();
    }
    assert_eq!(p.round, 4);
    assert_eq!(p.cond.tide, 1);
    assert_eq!(p.space_kind(low), Kind::Pit, "a flooded coin space burns instead");
    // Landing there costs 4 coins.
    p.cur = 0;
    p.players[0].space = low;
    p.players[0].coins = 10;
    p.moves_left = 0;
    p.land();
    assert_eq!(p.players[0].coins, 6);
}

#[test]
fn bonus_awards_give_a_sunstone_to_the_leaders_and_share_ties() {
    let mut p = Party::new(Config::default());
    p.players[0].coins = 40;
    p.players[1].wins = 3;
    p.players[2].wins = 3;
    p.players[1].lands[Kind::Coin.idx()] = 5;
    p.bonus_pick = 0;
    p.round = p.cfg.turns; // the last round's mini result ends the game
    p.phase = Phase::MiniResult(MiniReport { kind: Mini::StarTag, format: Format::Ffa, active: [true; 4], teams: [0, 1, 2, 3], scores: [0.0; 4], ranks: [1; 4], won: [false; 4], coins: [0; 4], round_end: true });
    p.apply(Cmd::Ack).unwrap();
    assert!(p.over());
    assert_eq!(p.awards[0].winners, vec![0]);
    assert_eq!(p.awards[1].winners, vec![1, 2]);
    assert_eq!(p.awards[2].winners, vec![1]);
    assert_eq!(p.players[0].sun, 1);
    assert_eq!(p.players[1].sun, 2);
    assert_eq!(p.players[2].sun, 1);
    assert_eq!(p.standings()[0], 1);
}

#[test]
fn bots_make_sensible_choices() {
    // A Hard bot with a Sunstone's worth of coins heads for the Keeper at a fork when one branch leads there.
    let mut hits = 0;
    let mut total = 0;
    for seed in 0..30 {
        let mut p = Party::new(Config { board: 0, turns: 12, seed, players: [Some(Level::Hard); 4] });
        let fork = p.board.edges.iter().position(|e| e.len() > 1).unwrap();
        p.cond.flip = false;
        p.keeper = p.board.sites[3];
        p.players[0].coins = SUNSTONE_COST + 5;
        p.players[0].space = fork;
        p.moves_left = 6;
        p.phase = Phase::Branch(p.board.out(fork, &p.cond));
        let Some(Cmd::Choose(c)) = p.bot_cmd() else { panic!() };
        let best = p.board.out(fork, &p.cond).into_iter().min_by_key(|&o| p.keeper_distance(o)).unwrap();
        total += 1;
        if c == best {
            hits += 1;
        }
    }
    assert!(hits * 10 >= total * 8, "{hits}/{total}");
}

#[test]
fn human_decisions_wait_for_the_player() {
    let mut p = Party::new(Config::default());
    assert!(p.waiting_for_human());
    assert!(p.bot_cmd().is_none());
    p.cur = 2;
    assert!(!p.waiting_for_human());
    assert!(p.bot_cmd().is_some());
}

/// `cargo test --release party_balance_report -- --ignored --nocapture`
#[test]
#[ignore]
fn party_balance_report() {
    for board in 0..BOARD_COUNT {
        let n = 400;
        let mut seat_wins = [0.0f32; 4];
        let (mut coins, mut suns, mut bought, mut duels, mut events) = (0.0, 0.0, 0.0, 0.0, 0u64);
        let mut stones = [0usize; 5];
        for seed in 0..n as u64 {
            let mut p = Party::new(cpu_cfg(board, 6, seed + 1));
            let mut buys = 0;
            while p.auto_step() {
                for e in p.drain_events() {
                    events += 1;
                    if matches!(e, Ev::Sunstone { .. }) {
                        buys += 1;
                    }
                }
            }
            let w = p.winners();
            for &x in &w {
                seat_wins[x] += 1.0 / w.len() as f32;
            }
            coins += p.players.iter().map(|x| x.coins as f32).sum::<f32>() / 4.0;
            suns += p.players.iter().map(|x| x.sun as f32).sum::<f32>() / 4.0;
            bought += buys as f32;
            duels += p.events_duels() as f32;
            stones[buys.min(4)] += 1;
        }
        println!(
            "{:<15} seat win%: {:>4.1} {:>4.1} {:>4.1} {:>4.1} | final coins {:>5.1} | sunstones/player {:>4.2} | bought/game {:>4.2} | duels/game {:>4.2} | events/game {:>5.0} | bought 0/1/2/3/4+: {:?}",
            BOARD_NAMES[board],
            seat_wins[0] / n as f32 * 100.0,
            seat_wins[1] / n as f32 * 100.0,
            seat_wins[2] / n as f32 * 100.0,
            seat_wins[3] / n as f32 * 100.0,
            coins / n as f32,
            suns / n as f32,
            bought / n as f32,
            duels / n as f32,
            events as f32 / n as f32,
            stones
        );
    }
    // Skill matters: a Hard CPU against three Easy ones.
    let mut hard_wins = 0.0;
    let n = 400;
    for seed in 0..n as u64 {
        let mut p = Party::new(Config { board: (seed % 3) as usize, turns: 12, seed: seed + 9, players: [Some(Level::Hard), Some(Level::Easy), Some(Level::Easy), Some(Level::Easy)] });
        p.autoplay();
        let w = p.winners();
        if w.contains(&0) {
            hard_wins += 1.0 / w.len() as f32;
        }
    }
    println!("Hard seat vs three Easy: wins {:.1}% (fair share is 25%)", hard_wins / n as f32 * 100.0);
}


/// Variety: the mini-games come out of a shuffle bag, so every one is played before any is played again, and never the
/// same one twice in a row.
#[test]
fn every_mini_game_comes_up_before_any_repeats() {
    for seed in 0..40u64 {
        let mut p = Party::new(cpu_cfg((seed % 3) as usize, 8, seed));
        let mut seen: Vec<(u32, Mini)> = vec![];
        let mut guard = 0;
        while !p.over() && guard < 200_000 {
            if let Phase::Mini(ctx) = &p.phase {
                if seen.last() != Some(&(p.minis_played, ctx.kind)) {
                    seen.push((p.minis_played, ctx.kind));
                }
            }
            p.auto_step();
            guard += 1;
        }
        let kinds: Vec<Mini> = seen.iter().map(|s| s.1).collect();
        assert!(kinds.len() >= 5, "seed {seed}: only {} games", kinds.len());
        for w in kinds.windows(2) {
            assert_ne!(w[0], w[1], "seed {seed}: the same game twice in a row");
        }
        for (k, chunk) in kinds.chunks(5).enumerate().filter(|(_, c)| c.len() == 5) {
            let mut c = chunk.to_vec();
            c.sort();
            c.dedup();
            assert_eq!(c.len(), 5, "seed {seed}: bag {k} repeats a game before the others: {chunk:?}");
        }
        let (lo, hi) = (p.mini_plays.iter().min().unwrap(), p.mini_plays.iter().max().unwrap());
        assert!(hi - lo <= 1, "seed {seed}: plays {:?}", p.mini_plays);
    }
}

/// Comeback (research checklist, Sunstone Shindig 5): how often the seat in last place after round 3 (fewest Sunstones, then
/// coins) wins a 6-round CPU game. `cargo test --release -p jumpman --lib comeback_report -- --ignored --nocapture`.
#[test]
#[ignore]
fn comeback_report() {
    let n = 600u64;
    let mut wins = 0.0f32;
    for seed in 0..n {
        let mut p = Party::new(cpu_cfg((seed % 3) as usize, 6, seed + 1));
        let mut last = None;
        while p.auto_step() {
            if last.is_none() && p.round >= 4 {
                let key = |q: usize| (p.players[q].sun, p.players[q].coins);
                last = (0..4).min_by_key(|&q| key(q));
            }
        }
        let w = p.winners();
        if let Some(l) = last {
            if w.contains(&l) {
                wins += 1.0 / w.len() as f32;
            }
        }
    }
    println!("COMEBACK: last after round 3 wins {:.1}% of {n} games (target 10-25%)", wins / n as f32 * 100.0);
}
