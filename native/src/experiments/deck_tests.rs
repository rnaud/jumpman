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

#[test]
fn heroes_map_to_their_decks() {
    for (c, h) in [(Character::Mario, Hero::Mario), (Character::Link, Hero::Link), (Character::YoungLink, Hero::YoungLink), (Character::Banjo, Hero::Banjo)] {
        let g = game(1, c);
        assert_eq!(g.run.hero, h);
        assert_eq!(g.run.deck, h.starter_deck());
    }
}
