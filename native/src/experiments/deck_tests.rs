//! Star Deck glue tests: the command path the UI uses, the event timeline the view animates, and the stage layout.
use super::deck::*;
use super::deck_core::*;
use super::deck_run::*;
use crate::characters::Character;

fn game(seed: u64, c: Character) -> DeckGame {
    DeckGame::new(c, 1, seed)
}

/// Run the whole game through the same `Cmd` path the UI uses, checking that the event stream the view
/// animates always reconstructs the core's hand and pile counts.
#[test]
fn commands_drive_a_whole_run_and_events_stay_consistent() {
    for (seed, chr) in [(3u64, Character::Mario), (4, Character::Link), (5, Character::YoungLink), (6, Character::Banjo)] {
        let mut g = game(seed, chr);
        let mut steps = 0;
        let mut hand: std::collections::HashSet<u32> = Default::default();
        let (mut draw_n, mut discard_n) = (0i64, 0i64);
        let mut in_combat = false;
        while !g.run.ended() && steps < 3000 {
            steps += 1;
            if g.run.screen != Screen::Combat {
                in_combat = false;
            } else if !in_combat {
                in_combat = true;
                hand.clear();
                draw_n = g.disp.draw_n as i64;
                discard_n = 0;
            }
            // Fold the queued events into a model of the view's piles.
            for (_, ev) in g.queue.drain(..) {
                match ev {
                    Ev::Draw { uid } => {
                        assert!(hand.insert(uid), "uid {uid} drawn twice");
                        draw_n -= 1;
                    }
                    Ev::Reshuffle => {
                        draw_n += discard_n;
                        discard_n = 0;
                    }
                    Ev::Play { uid, .. } => assert!(hand.remove(&uid), "played a card that was not in hand"),
                    Ev::Discard { uid } => {
                        hand.remove(&uid);
                        discard_n += 1;
                    }
                    Ev::Exhaust { uid } => {
                        hand.remove(&uid);
                    }
                    Ev::Junk { count } => discard_n += count as i64,
                    _ => {}
                }
            }
            g.t = g.q_end + 1.0;
            if let Some(c) = g.combat() {
                if !c.over() {
                    let mut want: Vec<u32> = c.hand.iter().map(|i| i.uid).collect();
                    let mut have: Vec<u32> = hand.iter().copied().collect();
                    want.sort();
                    have.sort();
                    assert_eq!(want, have, "view hand matches the core (seed {seed}, step {steps})");
                    assert_eq!((c.draw.len() as i64, c.discard.len() as i64), (draw_n, discard_n), "pile counts (seed {seed}, step {steps})");
                }
            }
            if let Some(cmd) = autoplay_step(&mut g) {
                g.execute(cmd);
            } else if g.run.screen == Screen::Combat {
                // Fight over: close it the way the flow system does.
                g.run.finish_combat();
                g.combat_open = false;
                g.rev += 1;
            }
        }
        assert!(g.run.ended(), "seed {seed} finished in {steps} steps");
        assert!(g.run.score() > 0);
    }
}

#[test]
fn script_tokens_parse() {
    assert_eq!(parse_cmd("play.2.1"), Some(Cmd::Play(2, Some(1))));
    assert_eq!(parse_cmd("play.0"), Some(Cmd::Play(0, None)));
    assert_eq!(parse_cmd("end"), Some(Cmd::EndTurn));
    assert_eq!(parse_cmd("node.4"), Some(Cmd::Node(4)));
    assert_eq!(parse_cmd("open.draw"), Some(Cmd::Open(Modal::Pile(Pile::Draw))));
    assert_eq!(parse_cmd("nonsense"), None);
}

#[test]
fn busy_blocks_plays_but_not_menus() {
    let mut g = game(9, Character::Mario);
    let n = g.run.map.rows[0][0];
    g.execute(Cmd::Node(n));
    assert_eq!(g.run.screen, Screen::Combat);
    g.start_combat_view();
    assert!(g.busy(), "the opening draw is still animating");
    let before = g.combat().unwrap().cards_played;
    g.execute(Cmd::Play(0, Some(0)));
    assert_eq!(g.combat().unwrap().cards_played, before, "input is locked while events play");
    g.execute(Cmd::Open(Modal::Deck));
    assert_eq!(g.modal, Modal::Deck);
    g.execute(Cmd::Close);
    g.t = g.q_end + 1.0;
    g.queue.clear();
    g.execute(Cmd::Play(0, Some(0)));
    assert_eq!(g.combat().unwrap().cards_played, before + 1);
}

#[test]
fn foe_slots_do_not_overlap() {
    for n in 1..=3 {
        let slots: Vec<_> = (0..n + 2).map(|i| foe_slot(i, n)).collect();
        for i in 0..slots.len() {
            for j in i + 1..slots.len() {
                let d = (slots[i][0] - slots[j][0]).hypot(slots[i][2] - slots[j][2]);
                assert!(d > 150.0 || (n == 2 && d > 120.0), "foes {i} and {j} of {n} stand {d} apart");
            }
        }
    }
}

/// A card-balance sanity check over whole runs: a simple bot should get somewhere but not coast through the act.
#[test]
fn bot_runs_are_neither_trivial_nor_hopeless() {
    let (mut wins, mut depth, n) = (0u32, 0u32, 80u32);
    for seed in 0..n as u64 {
        let mut run = Run::new(1000 + seed, Hero::ALL[seed as usize % 4]);
        let mut steps = 0;
        while !run.ended() && steps < 400 {
            run.bot_act();
            steps += 1;
        }
        assert!(run.ended());
        wins += run.won() as u32;
        depth += run.stats.nodes_cleared;
    }
    let avg = depth as f32 / n as f32;
    eprintln!("bot balance: {wins}/{n} wins, average {avg:.2} stops cleared");
    assert!(avg > 2.0 && avg < 7.0, "average depth {avg}");
    assert!(wins < n * 9 / 10, "a bot should not win nine runs in ten ({wins}/{n})");
}

#[test]
fn heroes_map_to_their_decks() {
    for (c, h) in [(Character::Mario, Hero::Mario), (Character::Link, Hero::Link), (Character::YoungLink, Hero::YoungLink), (Character::Banjo, Hero::Banjo)] {
        let g = game(1, c);
        assert_eq!(g.run.hero, h);
        assert_eq!(g.run.deck, h.starter_deck());
    }
}

#[test]
fn cycle_foe_wraps_in_both_directions_and_recovers_from_a_dead_aim() {
    let order = [2usize, 0, 1];
    assert_eq!(cycle_foe(&order, Some(2), 1), Some(0));
    assert_eq!(cycle_foe(&order, Some(1), 1), Some(2), "wraps to the first");
    assert_eq!(cycle_foe(&order, Some(2), -1), Some(1), "wraps to the last");
    assert_eq!(cycle_foe(&order, None, 1), Some(2));
    assert_eq!(cycle_foe(&order, None, -1), Some(1));
    assert_eq!(cycle_foe(&order, Some(9), 1), Some(2), "an aim that is no longer alive restarts");
    assert_eq!(cycle_foe(&[], Some(0), 1), None);
}

fn three_foe_game() -> DeckGame {
    let mut g = game(21, Character::Mario);
    g.execute(Cmd::Node(g.run.map.rows[0][0]));
    let foes = [ENEMY_KINDS[0], ENEMY_KINDS[1], ENEMY_KINDS[2]];
    let r = &g.run;
    g.run.combat = Some(Combat::new(5, r.hero, r.hp, r.max_hp, &r.deck.clone(), &r.relics.clone(), r.potions, &foes));
    g.start_combat_view();
    g.t = g.q_end + 1.0;
    g.queue.clear();
    let c = g.run.combat.as_mut().unwrap();
    c.hand.clear();
    c.hand.push(Inst { card: Card::new(card_id("punch")), uid: 900 });
    c.momentum = 5;
    g
}

#[test]
fn keyboard_aim_walks_living_foes_and_plays_at_the_aimed_one() {
    let mut g = three_foe_game();
    assert_eq!(g.disp.foes.len(), 3);
    g.selected = Some(0);
    g.execute(Cmd::Aim(0));
    assert_eq!(g.aim, Some(0), "starts on the leftmost foe");
    g.execute(Cmd::Aim(1));
    g.execute(Cmd::Aim(1));
    assert_eq!(g.aim, Some(2));
    g.execute(Cmd::Aim(1));
    assert_eq!(g.aim, Some(0), "wraps");
    g.execute(Cmd::Aim(-1));
    assert_eq!(g.aim, Some(2));
    // A dead foe is skipped.
    g.disp.foes[1].alive = false;
    g.execute(Cmd::Aim(-1));
    assert_eq!(g.aim, Some(0));
    g.disp.foes[1].alive = true;
    g.execute(Cmd::Aim(1));
    assert_eq!(g.aim, Some(1));
    let hp: Vec<i32> = g.combat().unwrap().foes.iter().map(|f| f.hp).collect();
    g.execute(Cmd::Aim(9));
    let queued = std::mem::take(&mut g.cmds);
    assert_eq!(queued, vec![Cmd::Play(0, Some(1))]);
    g.execute(queued[0]);
    let after: Vec<i32> = g.combat().unwrap().foes.iter().map(|f| f.hp).collect();
    assert!(after[1] < hp[1] && after[0] == hp[0] && after[2] == hp[2], "only the aimed foe was hit: {hp:?} -> {after:?}");
    assert_eq!(g.aim, None, "playing clears the aim");
    assert_eq!(g.selected, None);
}

#[test]
fn aiming_needs_several_living_foes() {
    let mut g = three_foe_game();
    g.disp.foes[0].alive = false;
    g.disp.foes[1].alive = false;
    g.execute(Cmd::Aim(0));
    assert_eq!(g.aim, None, "a single foe needs no aiming");
}

#[test]
fn daily_seed_is_shared_per_day_and_the_toggle_rerolls_a_fresh_map() {
    assert_eq!(daily_seed_for_day(20000), daily_seed_for_day(20000));
    assert_ne!(daily_seed_for_day(20000), daily_seed_for_day(20001));
    let mut a = game(5, Character::Mario);
    let mut b = game(77, Character::Banjo);
    for g in [&mut a, &mut b] {
        g.cheated = true; // keep the test off the real save file
        g.daily = false;
        assert!(g.fresh_start());
        g.toggle_daily(daily_seed_for_day(20000), 99);
        assert!(g.daily && g.saved.daily);
        assert_eq!(g.seed, daily_seed_for_day(20000));
    }
    assert_eq!(a.run.map.rows, b.run.map.rows, "the same daily seed gives the same map for every hero");
    a.toggle_daily(daily_seed_for_day(20000), 99);
    assert!(!a.daily);
    assert_eq!(a.seed, 99, "toggling off re-rolls a random seed");
    // Mid-run the toggle only changes the setting: the map stays.
    let first = b.run.map.rows[0][0];
    b.execute(Cmd::Node(first));
    assert!(!b.fresh_start());
    let rows = b.run.map.rows.clone();
    b.toggle_daily(5, 6);
    assert!(!b.daily);
    assert_eq!(b.run.map.rows, rows);
}

#[test]
fn script_tokens_cover_aim_and_daily() {
    assert_eq!(parse_cmd("aim"), Some(Cmd::Aim(0)));
    assert_eq!(parse_cmd("aim.r"), Some(Cmd::Aim(1)));
    assert_eq!(parse_cmd("aim.l"), Some(Cmd::Aim(-1)));
    assert_eq!(parse_cmd("aim.go"), Some(Cmd::Aim(9)));
    assert_eq!(parse_cmd("daily"), Some(Cmd::ToggleDaily));
}

/// The stage is a level patch of the real castle grounds: the hero's marks and every foe slot stand within a small
/// step of the stage height, and no tree stands on the fighting line.
#[test]
fn the_stage_is_level_real_ground() {
    use crate::game::world::{CourseRequest, LoadedCourse};
    let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    if !assets.join(format!("courses/{STAGE_COURSE}.json")).exists() {
        return;
    }
    let course = LoadedCourse::load(&assets, CourseRequest::new(STAGE_COURSE)).expect("course loads");
    let stage = stage_on(&course.world);
    let mut spots = vec![hero_home_at(stage, true), hero_home_at(stage, false)];
    for n in 1..=3 {
        for i in 0..n + 3 {
            spots.push(foe_slot_at(stage, i, n));
        }
    }
    for p in spots {
        let f = course.world.find_floor(p[0], p[1] + 300.0, p[2]);
        assert!(f.surface.is_some(), "no ground at {p:?}");
        assert!((f.height - stage[1]).abs() < 25.0, "ground at {p:?} is {:.0}, the stage is {:.0}", f.height, stage[1]);
    }
    for t in course.objects.objects.iter().filter(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::Tree) {
        assert!((t.pos[0] - stage[0]).abs() > 1300.0 || (t.pos[2] - stage[2]).abs() > 500.0, "a tree stands on the stage at {:?}", t.pos);
    }
}

/// Chains feel different from loose cards (`MINIGAMES_RESEARCH.md` section 4, checklist 3): every further link rings a
/// higher chime, throws more sparks and shakes the stage harder; and a card that lands holds the fight for a beat of
/// hit-stop that grows with the damage (checklist 10).
#[test]
fn chains_escalate_and_landed_cards_hit_stop() {
    let links: Vec<_> = (1..=4).map(chain_feedback).collect();
    let sounds: Vec<&str> = links.iter().map(|l| l.0).collect();
    assert_eq!(sounds, ["gem1", "gem2", "gem3", "star"], "the chime climbs along the chain");
    assert!(links.windows(2).all(|w| w[1].1 > w[0].1 && w[1].2 > w[0].2), "sparks and shake grow: {links:?}");
    assert!((0.05..=0.06).contains(&card_hitstop(1)) && card_hitstop(30) <= 0.16 && card_hitstop(12) > card_hitstop(4));
}

/// Pace of the first fight (checklist 2 and 4): the policy bot's turns and cards per turn in the first battle of a run,
/// over 200 seeds. `-- --nocapture` prints what `MINIGAMES_AUDIT_3.md` quotes.
#[test]
fn star_deck_first_fight_pace_report() {
    let (mut turns, mut cards, mut fights) = (0u32, 0u32, 0u32);
    let mut longest = 0;
    for seed in 0..200u64 {
        let mut run = Run::new(seed, Hero::ALL[seed as usize % 4]);
        let first = run.map.rows[0][0];
        run.enter_node(first).unwrap();
        let c = run.combat.as_mut().unwrap();
        c.bot_fight(60);
        turns += c.turn;
        longest = longest.max(c.turn);
        run.finish_combat();
        cards += run.stats.cards_played;
        fights += 1;
        assert_eq!(run.reward.as_ref().map(|r| r.cards.len()), Some(3), "a reward offers three cards (plus skip)");
    }
    eprintln!("DECK first fight: {:.1} turns on average (longest {longest}), {:.1} cards a turn", turns as f32 / fights as f32, cards as f32 / turns.max(1) as f32);
}

/// Star Deck has no free-roaming foes: they stand on fixed marks and the moves are choreographed lunges. Body collision
/// there means the choreography never puts the hero inside a foe: every attack lunge (at a target or at the group, and
/// the ground pound that lands closer) ends clear of every live foe's body for every line-up and kind, and a foe's own
/// lunge stops short of the hero on his mark.
#[test]
fn choreographed_lunges_never_put_the_hero_inside_a_foe() {
    let stage = [0.0, 0.0, 0.0];
    let kinds = ENEMY_KINDS;
    let home = hero_home_at(stage, true);
    let mut worst = f64::INFINITY;
    for n in 1..=6usize {
        for k0 in 0..kinds.len() {
            let line: Vec<([f64; 3], EnemyKind)> = (0..n).map(|i| (foe_slot_at(stage, i, n), kinds[(k0 + i * 5) % kinds.len()])).collect();
            for target in (0..n).map(Some).chain([None]) {
                for pound in [false, true] {
                    let dest = hero_lunge_dest(stage, &line, target.map(|i| line[i]), pound);
                    for &(p, kind) in &line {
                        let gap = (dest[0] - p[0]).hypot(dest[2] - p[2]) - foe_body_radius(kind) - HERO_BODY;
                        worst = worst.min(gap);
                        assert!(gap >= 0.0, "{n} foes, target {target:?}, pound {pound}: the lunge ends {gap:.0} inside a {kind:?}");
                    }
                    if let Some(i) = target {
                        let d = (dest[0] - line[i].0[0]).hypot(dest[2] - line[i].0[2]);
                        assert!(d < 450.0, "the lunge still reaches its target ({d:.0})");
                    }
                }
            }
            for &(p, kind) in &line {
                let tip = [p[0] - FOE_LUNGE, p[2]];
                let gap = (tip[0] - home[0]).hypot(tip[1] - home[2]) - foe_body_radius(kind) - HERO_BODY;
                assert!(gap >= 0.0, "a {kind:?}'s lunge from {p:?} reaches {gap:.0} into the hero");
            }
        }
    }
    eprintln!("body deck: closest lunge end {worst:.0} units clear of a foe body");
}
