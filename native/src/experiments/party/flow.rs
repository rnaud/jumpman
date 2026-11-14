//! The per-frame flow of Sunstone Shindig: UI commands, the animation queue that plays the core's events, CPU turns, and the
//! mini-game runner (human input in, fixed 30 Hz steps, results out).
use super::app::*;
use super::board::*;
use super::common::*;
use super::game::*;
use super::minis::*;
use crate::audio::SfxCue;
use crate::experiments::{ExperimentSlot, Shell};
use jumpman_sim::state::{A_BUTTON, B_BUTTON};
use crate::input::PlayerInput;
use crate::shell::AppState;
use bevy::prelude::*;

pub fn cue(shell: &mut Shell, kind: &'static str) {
    shell.cues.push(SfxCue::Effect { kind, pos: None });
}

/// The face the spinning die shows at time `t` (a quick, slightly uneven cycle so it cannot be stopped by counting).
pub fn dice_face(d: &Dice, t: f64) -> u8 {
    if !d.rolling {
        return d.value;
    }
    let (lo, hi) = d.die.range();
    let n = (hi - lo + 1) as u64;
    let tick = ((t - d.t0) * 17.0) as u64;
    lo + ((tick * 7 + (tick / 5) * 3) % n) as u8
}

fn rival_name(g: &Party, p: usize) -> &'static str {
    g.players[p].name
}

/// What a finished Rolled / event animation does when it starts: sound, banner, piece commands.
fn start_anim(app: &mut PartyApp, shell: &mut Shell, anim: &Anim) {
    let t = app.t;
    let fast = app.fast;
    match &anim.ev {
        Ev::RoundStart(n) => {
            let total = app.game.as_ref().map_or(12, |g| g.cfg.turns);
            if *n == total {
                app.say("Final round!", 0xffcc33);
            } else if !fast {
                app.say(format!("Round {n} of {total}"), 0xffffff);
            }
        }
        Ev::TurnStart(p) => {
            if let Some(g) = app.game.as_ref() {
                if !g.is_cpu(*p) && !fast {
                    app.toast("Your turn: roll the die!");
                }
            }
        }
        Ev::Rolled { p, value, die } => {
            cue(shell, "land");
            settle_call(app, shell, *p, *value, *die);
            let human = app.game.as_ref().is_some_and(|g| !g.is_cpu(g.cur));
            match &mut app.dice {
                Some(d) if human => {
                    d.rolling = false;
                    d.value = *value;
                    d.die = *die;
                }
                _ => app.dice = Some(Dice { die: *die, rolling: false, t0: t, value: *value }),
            }
        }
        Ev::Step { p, to, .. } => {
            let p = *p;
            if let Some(piece) = app.pieces.get_mut(p) {
                piece.at = Some(*to);
                piece.jump_next = true;
            }
            cue(shell, "footstep");
        }
        Ev::Landed { space, .. } => {
            app.pulse = Some((*space, t));
        }
        Ev::Coins { p, delta } => {
            let (text, color) = if *delta > 0 { (format!("+{delta}"), 0xffe14a) } else { (format!("{delta}"), 0xff6a5a) };
            app.floaters.push(Floater { p: *p, text, color, t0: t });
            cue(shell, if *delta > 0 { "coin" } else { "bonk" });
        }
        Ev::Payday { .. } => app.toast("Payday! +3 coins for passing Home."),
        Ev::Sunstone { p } => {
            app.say("SUNSTONE!", 0xffe14a);
            let name = app.game.as_ref().map_or("", |g| rival_name(g, *p));
            app.toast(format!("{name} bought a Sunstone!"));
            cue(shell, "star");
            app.confetti = t;
        }
        Ev::KeeperMoved { .. } => {
            app.toast("The Keeper moves to a new spot.");
            cue(shell, "teleport");
        }
        Ev::Warped { p, to, .. } => {
            if let Some(piece) = app.pieces.get_mut(*p) {
                piece.at = Some(*to);
                piece.snap = true;
            }
            cue(shell, "teleport");
        }
        Ev::Gift { p, item } => {
            let name = app.game.as_ref().map_or("", |g| rival_name(g, *p));
            app.toast(format!("{name}: {}!", item.label()));
            cue(shell, "one_up");
        }
        Ev::ItemUsed { p, item } => {
            let name = app.game.as_ref().map_or("", |g| rival_name(g, *p));
            app.toast(format!("{name} uses {}.", item.label()));
            cue(shell, "move_menu");
        }
        Ev::Stolen { from, to, amount } => {
            let (a, b) = app.game.as_ref().map_or(("", ""), |g| (rival_name(g, *to), rival_name(g, *from)));
            app.toast(format!("{a} steals {amount} coins from {b}!"));
            cue(shell, "hurt");
        }
        Ev::Swapped { a, b } => {
            let (sa, sb) = app.game.as_ref().map_or((0, 0), |g| (g.players[*a].space, g.players[*b].space));
            if let Some(p) = app.pieces.get_mut(*a) {
                p.at = Some(sa);
                p.snap = true;
            }
            if let Some(p) = app.pieces.get_mut(*b) {
                p.at = Some(sb);
                p.snap = true;
            }
            cue(shell, "teleport");
        }
        Ev::Text(s) => {
            app.toast(s.clone());
        }
        Ev::Flip(up) => {
            app.say(if *up { "East bridge lowers!" } else { "West bridge lowers!" }, 0xffd9a0);
            cue(shell, "cap_switch");
        }
        Ev::Wind(on) => {
            app.say(if *on { "Updrafts blow!" } else { "The wind drops." }, 0xbfe6ff);
            cue(shell, "flutter");
        }
        Ev::Tide(n) => {
            app.say(if *n == 1 { "The Bob-omb barrage moves in!" } else { "The barrage moves in again!" }, 0xff8a4a);
            cue(shell, "explosion");
            app.shake = 0.6;
        }
        Ev::MiniStart | Ev::MiniEnd | Ev::GameOver | Ev::Bonus { .. } => {}
    }
}

/// A rolled die settles the person's call on it: a right call pays a coin.
fn settle_call(app: &mut PartyApp, shell: &mut Shell, p: usize, value: u8, die: Die) {
    let Some(c) = app.call.clone().filter(|c| c.seat == p && c.rolled) else { return };
    app.call = None;
    let Some(high) = c.high else { return };
    let Some(me) = app.caller() else { return };
    app.calls.1 += 1;
    if (value >= die.call_line()) == high {
        app.calls.0 += 1;
        if let Some(g) = app.game.as_mut() {
            g.tip(me, CALL_PRIZE);
            let evs = g.drain_events();
            app.queue.extend(evs.into_iter().map(|ev| Anim { dur: anim_len(&ev, false) * 0.6, ev }));
        }
        app.toast(format!("Good call! +{CALL_PRIZE} coin ({} of {} right)", app.calls.0, app.calls.1));
        cue(shell, "coin");
    } else {
        app.toast(format!("Wrong call: {value}. ({} of {} right)", app.calls.0, app.calls.1));
    }
}

fn anim_done(app: &PartyApp, a: &Anim, t0: f64) -> bool {
    match a.ev {
        Ev::Step { p, .. } => {
            let elapsed = app.t - t0;
            let watched = app.game.as_ref().is_some_and(|g| g.is_cpu(p)) && app.cpu_watched();
            let limit = if app.fast { 0.12 } else if watched { 0.9 * app.cpu_pace() } else { 0.9 };
            if elapsed > limit {
                return true;
            }
            app.pieces.get(p).is_none_or(|pc| pc.arrived())
        }
        _ => app.t - t0 >= a.dur,
    }
}

pub fn handle_ui(app: &mut PartyApp, shell: &mut Shell, slot: &mut ExperimentSlot, cmd: UiCmd) {
    match cmd {
        UiCmd::Start => app.start_game(),
        UiCmd::PickBoard(n) => {
            app.cfg.board = n % BOARD_COUNT;
            cue(shell, "move_menu");
            app.bump();
        }
        UiCmd::PickTurns(n) => {
            app.cfg.turns = n.clamp(10, 15);
            cue(shell, "move_menu");
            app.bump();
        }
        UiCmd::PickLevel(l) => {
            app.level = l;
            cue(shell, "move_menu");
            app.bump();
        }
        UiCmd::PickHumans(n) => {
            app.humans = n.clamp(1, 4);
            cue(shell, "move_menu");
            app.bump();
        }
        UiCmd::OpenMinis => {
            app.screen = Screen::Minis;
            app.bump();
        }
        UiCmd::Practice(m) => app.start_practice(m),
        UiCmd::Back => match app.screen {
            Screen::Title => slot.exit = true,
            Screen::Minis => {
                app.screen = Screen::Title;
                app.bump();
            }
            Screen::Mini if app.mini.as_ref().is_some_and(|m| m.practice) => {
                app.mini = None;
                app.screen = Screen::Minis;
                app.place = true;
                app.bump();
            }
            _ => {
                app.game = None;
                app.mini = None;
                app.queue.clear();
                app.cur = None;
                app.screen = Screen::Title;
                app.place = true;
                app.bump();
            }
        },
        UiCmd::Game(c) => {
            if app.screen == Screen::Board && !app.busy() {
                app.dice = None;
                app.pending_item = None;
                app.send(c);
            }
        }
        UiCmd::UseItem(slot_n) => {
            if app.screen != Screen::Board || app.busy() {
                return;
            }
            let item = app.game.as_ref().and_then(|g| g.players[g.cur].items.get(slot_n).copied());
            if item == Some(Item::Pickpocket) {
                app.pending_item = Some(slot_n);
                app.bump();
            } else {
                app.send(Cmd::Item(slot_n, None));
            }
        }
        UiCmd::ItemTarget(slot_n, q) => {
            app.pending_item = None;
            if !app.busy() {
                app.send(Cmd::Item(slot_n, Some(q)));
            }
        }
        UiCmd::CancelItem => {
            app.pending_item = None;
            app.bump();
        }
        UiCmd::RollStart => {
            let ok = app.game.as_ref().is_some_and(|g| matches!(g.phase, Phase::TurnStart) && !g.is_cpu(g.cur)) && !app.busy() && app.dice.is_none();
            if ok {
                let die = app.game.as_ref().unwrap().die;
                app.dice = Some(Dice { die, rolling: true, t0: app.t, value: 1 });
                cue(shell, "move_menu");
                app.bump();
            }
        }
        UiCmd::RollStop => {
            if let Some(d) = app.dice.clone() {
                if d.rolling && !app.busy() {
                    let v = dice_face(&d, app.t);
                    app.send(Cmd::Roll(Some(v)));
                }
            }
        }
        UiCmd::Overview => {
            app.overview = !app.overview;
            app.bump();
        }
        UiCmd::MiniGo => {
            if let Some(m) = app.mini.as_mut() {
                if m.stage == MStage::Intro {
                    m.stage = MStage::Count;
                    m.t = 0.0;
                    cue(shell, "select_mission");
                    app.bump();
                }
            }
        }
        UiCmd::MiniAgain => {
            if let Some(m) = app.mini.as_ref() {
                if m.practice {
                    let k = m.kind;
                    app.cfg.seed = app.cfg.seed.wrapping_add(17);
                    app.start_practice(k);
                }
            }
        }
        UiCmd::MiniFinish => {
            let (practice, done) = app.mini.as_ref().map_or((false, false), |m| (m.practice, m.stage == MStage::Done));
            if done {
                if practice {
                    app.mini = None;
                    app.screen = Screen::Minis;
                    app.place = true;
                    app.bump();
                } else {
                    app.finish_mini_board();
                }
            }
        }
        UiCmd::Ack => {
            let ok = app.game.as_ref().is_some_and(|g| matches!(g.phase, Phase::MiniResult(_)));
            if ok && !app.busy() {
                app.send(Cmd::Ack);
            }
        }
        UiCmd::Next => {
            app.results_t0 -= 1.6;
        }
        UiCmd::Menu => {
            app.game = None;
            app.mini = None;
            app.screen = Screen::Title;
            app.place = true;
            app.bump();
        }
        UiCmd::Tile(k) => {
            if let Some(m) = app.mini.as_mut() {
                m.inp[0].pick = Some(k);
            }
        }
        UiCmd::Tap => {
            if let Some(m) = app.mini.as_mut() {
                m.inp[0].act = true;
                m.inp[0].held = true;
            }
        }
        UiCmd::Call(high) => {
            if app.call_open() {
                if let Some(c) = app.call.as_mut() {
                    c.high = Some(high);
                    c.rolled = true;
                }
                app.unanswered = 0;
                cue(shell, "move_menu");
                app.send(Cmd::Roll(None));
            }
        }
        UiCmd::PickBranch(i) => {
            let opt = app.game.as_ref().and_then(|g| match &g.phase {
                Phase::Branch(o) => o.get(i).copied(),
                _ => None,
            });
            if let Some(o) = opt {
                handle_ui(app, shell, slot, UiCmd::Game(Cmd::Choose(o)));
            }
        }
        UiCmd::CpuFast => {
            app.cpu_fast = !app.cpu_fast;
            app.toast(if app.cpu_fast { "CPU turns: fast-forward" } else { "CPU turns: normal speed" });
            cue(shell, "move_menu");
            app.bump();
        }
    }
}

/// Gather the live device state for the human seats of a running mini-game.
fn gather(run: &mut MiniRun, input: &PlayerInput, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, prev_a: &mut [bool; 8], tiles: usize) {
    let st = input.stick / 64.0;
    let mut x = st.x.clamp(-1.0, 1.0);
    let mut y = (-st.y).clamp(-1.0, 1.0);
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        x = -1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        x = 1.0;
    }
    if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
        y = -1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
        y = 1.0;
    }
    let a_down = input.down & A_BUTTON != 0 || keys.pressed(KeyCode::Space) || keys.pressed(KeyCode::Enter);
    let b_down = input.down & B_BUTTON != 0;
    let mut seats: Vec<(usize, f32, f32, bool, i32, bool)> = vec![(0, x, y, a_down, 0, b_down)];
    // Seats 1.. are driven by gamepads in order (the first pad also drives seat 0 through the touch/keyboard merge).
    let mut seat = 1;
    for pad in pads.iter() {
        if seat >= 4 {
            break;
        }
        let ls = pad.left_stick();
        let dir = if pad.just_pressed(GamepadButton::DPadRight) {
            1
        } else if pad.just_pressed(GamepadButton::DPadLeft) {
            -1
        } else {
            0
        };
        seats.push((seat, ls.x, -ls.y, pad.pressed(GamepadButton::South), dir, pad.pressed(GamepadButton::West) || pad.pressed(GamepadButton::East)));
        seat += 1;
    }
    for (s, sx, sy, a, dir, b) in seats {
        if !run.human[s] {
            continue;
        }
        if b && !prev_a[4 + s] {
            run.inp[s].b = true;
        }
        prev_a[4 + s] = b;
        let dz = |v: f32| if v.abs() < 0.12 { 0.0 } else { v };
        run.inp[s].x = dz(sx);
        run.inp[s].y = dz(sy);
        run.inp[s].held = a;
        if a && !prev_a[s] {
            run.inp[s].act = true;
            // A tile cursor for pads / keys: A picks the focused tile.
            if tiles > 0 && s == 0 {
                run.inp[s].pick.get_or_insert(run.tile_focus % tiles);
            } else if tiles > 0 {
                run.inp[s].pick.get_or_insert(0);
            }
        }
        prev_a[s] = a;
        if dir != 0 && tiles > 0 && s == 0 {
            run.tile_focus = (run.tile_focus as i32 + dir).rem_euclid(tiles as i32) as usize;
        }
    }
    if tiles > 0 {
        // The first gamepad moves seat 0's tile cursor with the d-pad (its A button picks, above).
        if let Some(pad) = pads.iter().next() {
            if pad.just_pressed(GamepadButton::DPadRight) || pad.just_pressed(GamepadButton::DPadDown) {
                run.tile_focus = (run.tile_focus + 1) % tiles;
            }
            if pad.just_pressed(GamepadButton::DPadLeft) || pad.just_pressed(GamepadButton::DPadUp) {
                run.tile_focus = (run.tile_focus + tiles - 1) % tiles;
            }
        }
        if keys.just_pressed(KeyCode::ArrowRight) {
            run.tile_focus = (run.tile_focus + 1) % tiles;
        }
        if keys.just_pressed(KeyCode::ArrowLeft) {
            run.tile_focus = (run.tile_focus + tiles - 1) % tiles;
        }
        for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9].iter().enumerate() {
            if keys.just_pressed(*k) && run.human[0] {
                run.inp[0].pick = Some(i);
            }
        }
    }
}

fn mini_flow(app: &mut PartyApp, shell: &mut Shell, dt: f64, gather_in: &mut dyn FnMut(&mut MiniRun, usize)) {
    let (fast, autoplay) = (app.fast, app.autoplay);
    let physics = app.pieces.len() >= 4;
    let t_now = app.t;
    let mut bump = false;
    let mut auto_finish = false;
    {
        let Some(run) = app.mini.as_mut() else { return };
        let any_human = (0..4).any(|p| run.human[p] && run.session.active[p]);
        match run.stage {
            MStage::Intro => {
                run.t += dt as f32;
                let mut s = Stage::default();
                run.session.game.scene(&mut s);
                run.scene = s;
                if (!any_human || autoplay) && run.t > if fast { 0.05 } else { 1.4 } {
                    run.stage = MStage::Count;
                    run.t = 0.0;
                    cue(shell, "select_mission");
                    bump = true;
                }
            }
            MStage::Count => {
                let before = run.t;
                run.t += dt as f32;
                let limit = if fast { 0.1 } else { 2.4 };
                if (before * 1.0).floor() != (run.t * 1.0).floor() && !fast {
                    cue(shell, "move_menu");
                }
                let mut s = Stage::default();
                run.session.game.scene(&mut s);
                run.scene = s;
                if run.t >= limit {
                    run.stage = MStage::Play;
                    run.t = 0.0;
                    run.acc = 0.0;
                    cue(shell, "star_sound");
                    bump = true;
                }
            }
            MStage::Play => {
                gather_in(run, 0);
                run.t += dt as f32;
                // With the four Marios in the world the physics steps the game (`view::drive_pieces`, 30 Hz); without them
                // (unit tests) the kinematic stand-in does.
                if !physics {
                    run.acc += dt as f32 * if fast { 6.0 } else { 1.0 };
                    let mut steps = 0;
                    while run.acc >= DT && steps < 12 && !run.session.done() {
                        let human: [Drive; 4] = std::array::from_fn(|p| run.human_drive(p));
                        run.session.step_kin(&human);
                        run.acc -= DT;
                        steps += 1;
                        for p in 0..4 {
                            run.inp[p].act = false;
                            run.inp[p].b = false;
                            run.inp[p].pick = None;
                        }
                    }
                }
                let mut s = Stage::default();
                run.session.game.scene(&mut s);
                run.scene = s;
                let sc = run.session.game.scores();
                run.prev_scores = sc;
                if run.session.done() {
                    run.final_scores = sc;
                    run.stage = MStage::Done;
                    run.t = 0.0;
                    let ranks = ranks(&sc, &run.session.active);
                    let human_won = (0..4).any(|p| run.human[p] && ranks[p] == 1);
                    cue(shell, if human_won { "one_up" } else { "land" });
                    bump = true;
                }
            }
            MStage::Done => {
                run.t += dt as f32;
                if autoplay && run.t > 0.2 {
                    auto_finish = true;
                }
                if !autoplay && !any_human && run.t > if fast { 0.2 } else { 2.2 } && !run.practice {
                    auto_finish = true;
                }
            }
        }
    }
    // Records (once) as the game ends, when a person played.
    let mut rec = None;
    if let Some(run) = app.mini.as_ref() {
        if run.stage == MStage::Done && run.t == 0.0 && run.human[0] && run.session.active[0] {
            rec = Some((run.kind, run.final_scores[0]));
        }
    }
    if let Some((k, s)) = rec {
        let new = app.record_mini(k, s);
        if let Some(run) = app.mini.as_mut() {
            run.new_record = new;
        }
    }
    if bump {
        app.bump();
    }
    let _ = t_now;
    if auto_finish {
        handle_pending_finish(app);
    }
}

fn handle_pending_finish(app: &mut PartyApp) {
    let practice = app.mini.as_ref().is_some_and(|m| m.practice);
    if practice {
        app.cmds.push(UiCmd::MiniFinish);
    } else {
        app.finish_mini_board();
    }
}

fn board_flow(app: &mut PartyApp, shell: &mut Shell, dt: f64) {
    let _ = dt;
    app.floaters.retain(|f| app.t - f.t0 < 1.4);
    // Animations play one at a time.
    if let Some((anim, t0)) = app.cur.clone() {
        if anim_done(app, &anim, t0) {
            app.cur = None;
            if matches!(anim.ev, Ev::Rolled { .. }) {
                app.dice = None;
            }
            app.bump();
        } else {
            return;
        }
    }
    if let Some(a) = app.queue.pop_front() {
        start_anim(app, shell, &a);
        app.cur = Some((a, app.t));
        return;
    }
    let Some(g) = app.game.as_ref() else { return };
    let phase = g.phase.clone();
    let waiting = g.waiting_for_human();
    match phase {
        Phase::Over => {
            let since = *app.over_since.get_or_insert(app.t);
            if app.t - since > if app.fast { 0.1 } else { 1.2 } {
                app.screen = Screen::Results;
                app.results_t0 = app.t;
                app.record_game();
                app.place = true;
                app.confetti = app.t;
                cue(shell, "one_up");
                app.bump();
            }
        }
        Phase::Mini(ctx) => {
            cue(shell, "select_mission");
            app.start_board_mini(ctx);
        }
        Phase::MiniResult(_) => {
            if app.autoplay || (!app.is_any_human()) {
                if app.t - app.think > if app.fast { 0.05 } else { 2.0 } {
                    app.send(Cmd::Ack);
                    app.think = app.t;
                }
            }
        }
        _ => {
            if waiting {
                app.think = app.t;
                return;
            }
            let delay = if app.fast { 0.04 } else { 0.7 * app.cpu_pace() };
            if app.t - app.think < delay {
                return;
            }
            app.think = app.t;
            let cur = app.game.as_ref().map_or(0, |g| g.cur);
            // A call is open on this CPU's roll: wait for it (a little), then roll.
            if let Some(c) = app.call.as_ref().filter(|c| !c.rolled && c.seat == cur) {
                if c.high.is_none() && app.t - c.t0 < app.call_window() {
                    return;
                }
                if c.high.is_none() {
                    app.unanswered += 1;
                }
                if let Some(c) = app.call.as_mut() {
                    c.rolled = true;
                }
                app.bump();
                app.send(Cmd::Roll(None));
                return;
            }
            let cmd = app.game.as_mut().and_then(|g| g.bot_cmd());
            if let Some(c) = cmd {
                // A CPU about to roll: the person watching may call it HIGH or LOW first (not on fast-forward).
                if c == Cmd::Roll(None) && app.cpu_watched() && !app.cpu_fast {
                    let die = app.game.as_ref().map_or(Die::Normal, |g| g.die);
                    app.call = Some(Call { seat: cur, t0: app.t, high: None, rolled: false, die });
                    app.bump();
                    return;
                }
                app.send(c);
            }
        }
    }
}

impl PartyApp {
    pub fn is_any_human(&self) -> bool {
        self.game.as_ref().is_some_and(|g| (0..4).any(|p| !g.is_cpu(p)))
    }
}

#[allow(clippy::too_many_arguments)]
/// One frame of the party flow, without any engine resources (the tests drive this directly).
pub fn tick(app: &mut PartyApp, shell: &mut Shell, slot: &mut ExperimentSlot, dt: f64, gather_in: &mut dyn FnMut(&mut MiniRun, usize)) {
    app.t += dt;
    app.shake = (app.shake - dt as f32 * 1.5).max(0.0);
    if !app.hooks_done {
        app.hooks_done = true;
        first_frame_hooks(app);
    }
    // Scripted commands.
    while app.script.first().is_some_and(|(t, _)| *t <= app.t) {
        let (_, c) = app.script.remove(0);
        app.cmds.push(c);
    }
    if app.autoplay && app.screen == Screen::Title {
        app.start_game();
    }
    let cmds = std::mem::take(&mut app.cmds);
    for c in cmds {
        handle_ui(app, shell, slot, c);
    }
    // Expire the banner / toast.
    if app.banner.as_ref().is_some_and(|b| app.t - b.1 > 1.6) {
        app.banner = None;
    }
    if app.toast.as_ref().is_some_and(|b| app.t - b.1 > 2.4) {
        app.toast = None;
    }
    match app.screen {
        Screen::Board => board_flow(app, shell, dt),
        Screen::Mini => mini_flow(app, shell, dt, gather_in),
        _ => {}
    }
    for (kind, pos) in std::mem::take(&mut app.cues) {
        shell.cues.push(SfxCue::Effect { kind, pos: Some(pos) });
    }
    if (app.t * 2.0).floor() != ((app.t - dt) * 2.0).floor() {
        set_status(status_line(app));
    }
}

/// One line on the state of the party (the `JUMPMAN_XP_LOG` hook prints it through the experiment's `debug`).
pub fn status_line(app: &PartyApp) -> String {
    let mut s = format!("screen {:?}", app.screen);
    if let Some(g) = app.game.as_ref() {
        s += &format!(" | round {}/{} turn {} phase {:?} | sun {:?} coins {:?}", g.round.min(g.cfg.turns), g.cfg.turns, g.cur, std::mem::discriminant(&g.phase), g.players.iter().map(|p| p.sun).collect::<Vec<_>>(), g.players.iter().map(|p| p.coins).collect::<Vec<_>>());
    }
    if let Some(m) = app.mini.as_ref() {
        let pos: Vec<[i64; 3]> = app.pieces.iter().map(|p| p.mario.runtime.pos.map(|v| v as i64)).collect();
        s += &format!(" | {} {:?} t {:.1} left {:.1} scores {:?} at {:?}", m.session.kind.info().name, m.stage, m.t, m.session.game.time_left(), m.session.game.scores().map(|v| if v.is_finite() { (v * 10.0).round() / 10.0 } else { -1.0 }), pos);
    }
    s
}

#[allow(clippy::too_many_arguments)]
pub fn flow(
    mut app: ResMut<PartyApp>,
    mut shell: ResMut<Shell>,
    mut slot: ResMut<ExperimentSlot>,
    time: Res<Time>,
    state: Res<AppState>,
    input: Res<PlayerInput>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut touch: ResMut<crate::touch::TouchControls>,
    mut prev_a: Local<[bool; 8]>,
) {
    if *state != AppState::Playing {
        return;
    }
    let dt = time.delta_secs_f64().min(0.1);
    app.pads = pads.iter().count();
    if app.screen == Screen::Board {
        // Keys for watching the CPUs: F fast-forwards their turns, H / L call a roll.
        if keys.just_pressed(KeyCode::KeyF) {
            app.cmds.push(UiCmd::CpuFast);
        }
        if app.call_open() {
            if keys.just_pressed(KeyCode::KeyH) {
                app.cmds.push(UiCmd::Call(true));
            }
            if keys.just_pressed(KeyCode::KeyL) {
                app.cmds.push(UiCmd::Call(false));
            }
        }
    }
    let mut gather_in = |run: &mut MiniRun, tiles: usize| gather(run, &input, &keys, &pads, &mut prev_a, tiles);
    tick(&mut app, &mut shell, &mut slot, dt, &mut gather_in);
    // The touch pad (stick, A, B) only shows while a mini-game is being played.
    let want_pad = app.screen == Screen::Mini && app.mini.as_ref().is_some_and(|m| m.stage == MStage::Play && m.human[0]);
    if touch.hide_pad == want_pad {
        touch.hide_pad = !want_pad;
        touch.suppress = !want_pad;
    }
}

fn first_frame_hooks(app: &mut PartyApp) {
    if let Some(m) = std::env::var("JUMPMAN_PARTY_MINIGAME").ok().and_then(|s| Mini::from_key(&s)) {
        app.start_practice(m);
        return;
    }
    let start = std::env::var("JUMPMAN_PARTY_START").unwrap_or_default();
    match start.as_str() {
        "minis" => {
            app.screen = Screen::Minis;
            app.bump();
        }
        "title" => {}
        "results" => {
            app.autoplay = true;
            app.start_game();
            let mut g = app.game.take().unwrap();
            g.autoplay();
            g.drain_events();
            app.game = Some(g);
            app.queue.clear();
            app.autoplay = false;
            app.fast = false;
            app.screen = Screen::Results;
            app.results_t0 = app.t - env_f("JUMPMAN_PARTY_REVEAL", 0.0);
            app.place = true;
            app.bump();
        }
        "overview" => {
            app.start_game();
            app.overview = true;
        }
        "shop" | "keeper" | "duel" | "branch" | "mini" | "miniresult" => {
            // Fast-forward with bots until seat 0 faces that decision (tests / screenshots).
            let want = start.clone();
            app.start_game();
            if let Some(mut g) = app.game.take() {
                let mut guard = 0;
                let hit = |g: &Party| match (&g.phase, want.as_str()) {
                    (Phase::ShopMenu, "shop") | (Phase::KeeperOffer, "keeper") | (Phase::DuelPick, "duel") | (Phase::Branch(_), "branch") => g.cur == 0,
                    (Phase::Mini(_), "mini") | (Phase::MiniResult(_), "miniresult") => true,
                    _ => false,
                };
                while !g.over() && !hit(&g) && guard < 200_000 {
                    g.auto_step();
                    guard += 1;
                }
                g.drain_events();
                app.queue.clear();
                app.game = Some(g);
                app.place = true;
                app.bump();
            }
        }
        _ => {
            if std::env::var_os("JUMPMAN_PARTY_BOARD").is_some() || std::env::var_os("JUMPMAN_PARTY_SKIP").is_some() {
                app.start_game();
            }
        }
    }
}

fn env_f(k: &str, d: f64) -> f64 {
    std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::CollisionWorld;
    use std::sync::Arc;

    fn app(autoplay: bool, board: usize, seed: u64) -> PartyApp {
        let mut w = CollisionWorld::new(1024.0, 1e9);
        w.finalize();
        let mut a = PartyApp::new(Arc::new(w), None, 1);
        a.autoplay = autoplay;
        a.fast = autoplay;
        a.cfg.board = board;
        a.cfg.seed = seed;
        a.hooks_done = true;
        a.script.clear();
        a
    }

    fn step(a: &mut PartyApp, secs: f64) {
        let (mut shell, mut slot) = (Shell::default(), ExperimentSlot::default());
        let mut n = (secs / 0.04).ceil() as usize;
        while n > 0 {
            tick(a, &mut shell, &mut slot, 0.04, &mut |_, _| {});
            n -= 1;
        }
    }

    fn run_until(a: &mut PartyApp, limit: f64, done: impl Fn(&PartyApp) -> bool) -> bool {
        let (mut shell, mut slot) = (Shell::default(), ExperimentSlot::default());
        let mut t = 0.0;
        while t < limit {
            if done(a) {
                return true;
            }
            tick(a, &mut shell, &mut slot, 0.04, &mut |_, _| {});
            t += 0.04;
        }
        done(a)
    }

    #[test]
    fn the_whole_ui_flow_plays_a_game_to_the_results_screen_on_every_board() {
        for board in 0..3 {
            let mut a = app(true, board, 40 + board as u64);
            a.cfg.turns = 10;
            assert!(run_until(&mut a, 3600.0, |a| a.screen == Screen::Results), "board {board} never reached the results screen");
            let g = a.game.as_ref().unwrap();
            assert!(g.over());
            assert_eq!(g.awards.len(), 3);
        }
    }

    #[test]
    fn a_person_must_roll_and_stop_the_die_themselves() {
        let mut a = app(false, 0, 7);
        a.start_game();
        assert!(run_until(&mut a, 30.0, |a| !a.busy() && a.game.as_ref().is_some_and(|g| matches!(g.phase, Phase::TurnStart) && g.waiting_for_human())));
        // Nothing happens until we act.
        step(&mut a, 3.0);
        assert!(!a.game.as_ref().unwrap().rolled);
        a.cmds.push(UiCmd::RollStart);
        step(&mut a, 0.5);
        let d = a.dice.clone().unwrap();
        assert!(d.rolling);
        let seen: std::collections::HashSet<u8> = (0..40).map(|i| dice_face(&d, a.t + i as f64 * 0.05)).collect();
        assert!(seen.len() >= 6, "the die should cycle through many faces, saw {seen:?}");
        assert!(seen.iter().all(|v| (1..=10).contains(v)));
        a.cmds.push(UiCmd::RollStop);
        step(&mut a, 0.1);
        assert!(a.game.as_ref().unwrap().rolled, "stopping the die rolls it");
    }

    #[test]
    fn dice_faces_follow_the_die_and_freeze_when_stopped() {
        for die in [Die::Normal, Die::Dash, Die::Twin] {
            let (lo, hi) = die.range();
            let d = Dice { die, rolling: true, t0: 0.0, value: 1 };
            for i in 0..200 {
                let f = dice_face(&d, i as f64 * 0.013);
                assert!(f >= lo && f <= hi, "{die:?} showed {f}");
            }
            let stopped = Dice { rolling: false, value: 6, ..d };
            assert_eq!(dice_face(&stopped, 12.3), 6);
        }
    }

    #[test]
    fn every_script_word_parses() {
        for (word, want) in [
            ("roll", UiCmd::RollStart),
            ("stop", UiCmd::RollStop),
            ("choose.12", UiCmd::Game(Cmd::Choose(12))),
            ("keeper.1", UiCmd::Game(Cmd::Keeper(true))),
            ("keeper.0", UiCmd::Game(Cmd::Keeper(false))),
            ("buy.2", UiCmd::Game(Cmd::ShopBuy(Item::Whistle))),
            ("leave", UiCmd::Game(Cmd::ShopLeave)),
            ("duel.3", UiCmd::Game(Cmd::DuelPick(3))),
            ("item.1", UiCmd::UseItem(1)),
            ("target.0.2", UiCmd::ItemTarget(0, 2)),
            ("ack", UiCmd::Ack),
            ("go", UiCmd::MiniGo),
            ("again", UiCmd::MiniAgain),
            ("finish", UiCmd::MiniFinish),
            ("next", UiCmd::Next),
            ("overview", UiCmd::Overview),
            ("tap", UiCmd::Tap),
            ("tile.4", UiCmd::Tile(4)),
            ("back", UiCmd::Back),
            ("start", UiCmd::Start),
            ("board.2", UiCmd::PickBoard(2)),
            ("minis", UiCmd::OpenMinis),
            ("practice.crown", UiCmd::Practice(Mini::SummitCrown)),
            ("high", UiCmd::Call(true)),
            ("low", UiCmd::Call(false)),
            ("cpufast", UiCmd::CpuFast),
            ("pick.1", UiCmd::PickBranch(1)),
        ] {
            assert_eq!(parse_ui_cmd(word), Some(want), "{word}");
        }
        assert_eq!(parse_ui_cmd("nonsense"), None);
        assert_eq!(parse_ui_cmd("practice.nope"), None);
    }

    #[test]
    fn practice_runs_from_the_list_to_the_result_and_back() {
        let mut a = app(false, 0, 3);
        a.cmds.push(UiCmd::OpenMinis);
        step(&mut a, 0.1);
        assert_eq!(a.screen, Screen::Minis);
        a.cmds.push(UiCmd::Practice(Mini::CoinScramble));
        step(&mut a, 0.1);
        assert_eq!(a.screen, Screen::Mini);
        assert_eq!(a.mini.as_ref().unwrap().stage, MStage::Intro);
        // The intro waits for START (a person is playing).
        step(&mut a, 5.0);
        assert_eq!(a.mini.as_ref().unwrap().stage, MStage::Intro);
        a.cmds.push(UiCmd::MiniGo);
        assert!(run_until(&mut a, 120.0, |a| a.mini.as_ref().is_some_and(|m| m.stage == MStage::Done)));
        let m = a.mini.as_ref().unwrap();
        assert!(m.final_scores[1].is_finite(), "CPUs finished with scores");
        a.cmds.push(UiCmd::MiniFinish);
        step(&mut a, 0.1);
        assert_eq!(a.screen, Screen::Minis);
        assert!(a.mini.is_none());
        a.cmds.push(UiCmd::Back);
        step(&mut a, 0.1);
        assert_eq!(a.screen, Screen::Title);
    }

    #[test]
    fn the_save_file_round_trips() {
        let s = Saved { games: 5, wins: 2, best_sun: [3, 0, 4], records: vec![("bombs".into(), 12.5), ("meter".into(), 210.0)] };
        let back = saved_from_json(&saved_to_json(&s));
        assert_eq!((back.games, back.wins, back.best_sun), (5, 2, [3, 0, 4]));
        assert_eq!(back.records.len(), 2);
        assert!(back.records.iter().any(|r| r.0 == "bombs" && (r.1 - 12.5).abs() < 1e-4));
        assert_eq!(saved_from_json(&serde_json::json!({})).games, 0);
    }

    #[test]
    fn a_board_mini_game_hands_its_scores_back_to_the_game() {
        let mut a = app(true, 1, 12);
        a.cfg.turns = 10;
        a.start_game();
        assert!(run_until(&mut a, 600.0, |a| a.screen == Screen::Mini));
        let before = a.game.as_ref().unwrap().minis_played;
        assert!(run_until(&mut a, 600.0, |a| a.screen == Screen::Board && a.game.as_ref().is_some_and(|g| !matches!(g.phase, Phase::Mini(_)))));
        let g = a.game.as_ref().unwrap();
        assert!(g.minis_played >= before);
        assert!(matches!(g.phase, Phase::MiniResult(_)) || g.round > 1 || g.players.iter().any(|p| p.wins > 0 || p.earned > 0));
    }

    /// One tap-level action of a person: what a player at the screen would press right now (`None` when nothing waits on a person).
    fn person_acts(a: &PartyApp, n: u32) -> Option<UiCmd> {
        match a.screen {
            Screen::Board if !a.busy() => {
                let g = a.game.as_ref()?;
                match g.phase.clone() {
                    Phase::TurnStart if !g.is_cpu(g.cur) => match &a.dice {
                        None => Some(UiCmd::RollStart),
                        Some(d) if d.rolling && a.t - d.t0 > 0.3 => Some(UiCmd::RollStop),
                        _ => None,
                    },
                    Phase::Branch(opts) if !g.is_cpu(g.cur) => Some(UiCmd::Game(Cmd::Choose(opts[(n as usize) % opts.len()]))),
                    Phase::KeeperOffer if !g.is_cpu(g.cur) => Some(UiCmd::Game(Cmd::Keeper(n % 2 == 0))),
                    Phase::ShopMenu if !g.is_cpu(g.cur) => Some(UiCmd::Game(Cmd::ShopLeave)),
                    Phase::DuelPick if !g.is_cpu(g.cur) => Some(UiCmd::Game(Cmd::DuelPick((g.cur + 1 + (n as usize) % 3) % 4))),
                    Phase::MiniResult(_) => Some(UiCmd::Ack),
                    _ => None,
                }
            }
            Screen::Mini => {
                let m = a.mini.as_ref()?;
                match m.stage {
                    MStage::Intro => Some(UiCmd::MiniGo),
                    MStage::Play => Some(if n % 2 == 0 { UiCmd::Tap } else { UiCmd::Tile((n as usize / 2) % 3) }),
                    MStage::Done => Some(UiCmd::MiniFinish),
                    MStage::Count => None,
                }
            }
            _ => None,
        }
    }

    /// Solo, the CPUs' turns play at half the animation time (a fifth or less with fast-forward), and before each CPU roll
    /// the person may call it HIGH or LOW: a right call pays a coin.
    #[test]
    fn cpu_turns_play_quicker_and_a_person_can_call_their_rolls() {
        let run = |fast_fwd: bool, call: Option<bool>| {
            let mut a = app(false, 0, 21);
            a.humans = 1;
            a.cfg.turns = 5;
            a.cpu_fast = fast_fwd;
            a.cmds.push(UiCmd::Start);
            let (mut shell, mut slot) = (Shell::default(), ExperimentSlot::default());
            let (mut cpu_t, mut n, mut opened) = (0.0, 0u32, 0u32);
            let mut was_open = false;
            while a.game.as_ref().is_none_or(|g| g.round <= 2 && !g.over()) && n < 300_000 {
                if let Some(c) = person_acts(&a, n / 7) {
                    if n % 7 == 0 {
                        a.cmds.push(c);
                    }
                }
                let open = a.call_open();
                opened += (open && !was_open) as u32;
                was_open = open;
                if let (Some(h), true) = (call, open) {
                    if n % 9 == 0 {
                        a.cmds.push(UiCmd::Call(h));
                    }
                }
                let cpu = a.screen == Screen::Board && a.game.as_ref().is_some_and(|g| g.is_cpu(g.cur) && matches!(g.phase, Phase::TurnStart | Phase::Branch(_) | Phase::KeeperOffer | Phase::ShopMenu | Phase::DuelPick));
                tick(&mut a, &mut shell, &mut slot, 0.04, &mut |_, _| {});
                if cpu {
                    cpu_t += 0.04;
                }
                n += 1;
            }
            (cpu_t / 6.0, opened, a.calls)
        };
        let (paced, opened, none) = run(false, None);
        let (skipped, opened_fast, _) = run(true, None);
        let (_, _, calls) = run(false, Some(true));
        eprintln!("CPU turn: {paced:.2}s paced, {skipped:.2}s fast-forward; calls offered {opened}, calls made {calls:?}");
        assert!(opened >= 6, "every CPU roll is offered to call ({opened})");
        assert_eq!(none, (0, 0), "nothing is called unless the person calls");
        assert_eq!(opened_fast, 0, "fast-forward skips the calls");
        assert!(calls.1 >= 6 && calls.0 <= calls.1, "{calls:?}");
        assert!(skipped < paced * 0.6, "fast-forward {skipped:.2}s against {paced:.2}s");
        assert!(paced < 6.0, "a CPU turn takes {paced:.2}s");
    }

    #[test]
    fn a_right_call_pays_a_coin_and_a_wrong_one_nothing() {
        let mut a = app(false, 0, 5);
        a.humans = 1;
        a.start_game();
        let mut shell = Shell::default();
        let coins = |a: &PartyApp| a.game.as_ref().unwrap().players[0].coins;
        let c0 = coins(&a);
        a.call = Some(Call { seat: 1, t0: 0.0, high: Some(true), rolled: true, die: Die::Normal });
        settle_call(&mut a, &mut shell, 1, 6, Die::Normal);
        assert_eq!(coins(&a), c0 + CALL_PRIZE, "6 is HIGH on the plain die");
        assert!(a.call.is_none() && a.calls == (1, 1));
        a.call = Some(Call { seat: 2, t0: 0.0, high: Some(true), rolled: true, die: Die::Normal });
        settle_call(&mut a, &mut shell, 2, 5, Die::Normal);
        assert_eq!(coins(&a), c0 + CALL_PRIZE, "5 is LOW");
        assert_eq!(a.calls, (1, 2));
        assert_eq!((Die::Normal.call_line(), Die::Dash.call_line(), Die::Twin.call_line()), (6, 8, 7));
    }

    /// One solo party measured against the research checklist (`MINIGAMES_RESEARCH.md` section 8): how long the person waits
    /// between moments they act (their own choices, a call on a CPU roll, playing a mini-game), how long a CPU turn takes, the
    /// real time per round (mini-game to mini-game), the mini-game length and the session length. CPU turns are paced
    /// (`fast_fwd` off) unless asked.
    struct SoloAudit {
        session: f64,
        gaps: Vec<f64>,
        cpu_turn: f64,
        rounds: Vec<f64>,
        minis: Vec<f64>,
        calls: u32,
    }

    fn solo_audit(board: usize, seed: u64, fast_fwd: bool, call: bool) -> SoloAudit {
        let mut a = app(false, board, seed);
        a.humans = 1;
        a.cfg.turns = DEFAULT_TURNS;
        a.cpu_fast = fast_fwd;
        a.cmds.push(UiCmd::Start);
        let (mut shell, mut slot) = (Shell::default(), ExperimentSlot::default());
        let (mut n, mut t, mut cpu_t, mut cpu_turns) = (0u32, 0.0f64, 0.0f64, 0u32);
        let (mut last_act, mut gaps) = (0.0f64, vec![]);
        let (mut mini_starts, mut minis, mut play_t0) = (vec![], vec![], None::<f64>);
        let mut was_cpu = None::<usize>;
        while a.screen != Screen::Results && n < 600_000 {
            let mut acted = false;
            if let Some(c) = person_acts(&a, n / 7) {
                if n % 7 == 0 {
                    a.cmds.push(c);
                }
                acted = true;
            }
            if call && a.call_open() {
                if n % 9 == 0 {
                    a.cmds.push(UiCmd::Call(n % 2 == 0));
                }
                acted = true;
            }
            if let Some(m) = a.mini.as_ref() {
                match (m.stage, play_t0) {
                    (MStage::Play, None) => {
                        play_t0 = Some(t);
                        mini_starts.push(t);
                    }
                    (MStage::Play, _) => {}
                    (_, Some(t0)) => {
                        minis.push(t - t0);
                        play_t0 = None;
                    }
                    _ => {}
                }
            }
            if acted {
                if t - last_act > 0.5 {
                    gaps.push(t - last_act);
                }
                last_act = t;
            }
            let cpu = a.screen == Screen::Board && a.game.as_ref().is_some_and(|g| g.is_cpu(g.cur) && matches!(g.phase, Phase::TurnStart | Phase::Branch(_) | Phase::KeeperOffer | Phase::ShopMenu | Phase::DuelPick));
            let cur = a.game.as_ref().map(|g| g.cur);
            if cpu && was_cpu != cur {
                cpu_turns += 1;
            }
            was_cpu = if cpu { cur } else { None };
            tick(&mut a, &mut shell, &mut slot, 0.04, &mut |_, _| {});
            if cpu {
                cpu_t += 0.04;
            }
            t += 0.04;
            n += 1;
        }
        let rounds = mini_starts.windows(2).map(|w| w[1] - w[0]).collect();
        SoloAudit { session: t, gaps, cpu_turn: cpu_t / cpu_turns.max(1) as f64, rounds, minis, calls: a.calls.1 }
    }

    /// Solo, a person who never plays the call game does not sit through the CPU's wait for a call: CPU turns stay under 4 s
    /// and the person is never left more than 15 s without something to do (the research checklist's solo downtime).
    #[test]
    fn a_person_who_never_calls_is_not_kept_waiting() {
        for seed in 0..3u64 {
            let r = solo_audit(seed as usize % 3, 300 + seed, false, false);
            let worst = r.gaps.iter().copied().fold(0.0, f64::max);
            eprintln!("seed {seed}: CPU turn {:.1}s, longest wait {worst:.1}s, session {:.0}s", r.cpu_turn, r.session);
            assert!(r.cpu_turn < 4.0, "a CPU turn takes {:.1}s", r.cpu_turn);
            assert!(worst <= 15.0, "the person waited {worst:.1}s");
        }
    }

    /// `cargo test --release -p jumpman --lib audit_shindig_solo -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn audit_shindig_solo() {
        for (label, fast, call) in [("paced, calls", false, true), ("paced, no calls", false, false), ("fast-forward", true, false)] {
            let (mut sess, mut mean_gap, mut worst, mut cpu, mut round, mut mini, mut over15, mut calls, mut n) = (0.0, 0.0, 0.0f64, 0.0, 0.0, 0.0, 0usize, 0, 0.0);
            let mut rounds_n = 0usize;
            for seed in 0..6u64 {
                let r = solo_audit((seed % 3) as usize, 300 + seed, fast, call);
                sess += r.session;
                // The person's wait between acts, averaged over the session (time not acting / number of waits).
                mean_gap += r.gaps.iter().sum::<f64>() / r.gaps.len().max(1) as f64;
                worst = worst.max(r.gaps.iter().copied().fold(0.0, f64::max));
                over15 += r.gaps.iter().filter(|g| **g > 15.0).count();
                cpu += r.cpu_turn;
                round += r.rounds.iter().sum::<f64>() / r.rounds.len().max(1) as f64;
                rounds_n += r.rounds.iter().filter(|x| **x > 90.0).count();
                mini += r.minis.iter().sum::<f64>() / r.minis.len().max(1) as f64;
                calls += r.calls;
                n += 1.0;
            }
            eprintln!("AUDIT SHINDIG [{label}]: session {:.0}s, mean wait between acts {:.1}s, longest wait {:.1}s, waits over 15 s {over15} in 6 games, CPU turn {:.1}s, round {:.0}s (rounds over 90 s: {rounds_n}), mini-game {:.0}s, calls {calls}", sess / n, mean_gap / n, worst, cpu / n, round / n, mini / n);
        }
    }

    /// Whole games with every people count, played only through the commands the touch UI sends.
    #[test]
    fn one_to_four_people_play_a_whole_game_to_the_results_screen_by_touch() {
        for humans in 1..=4usize {
            let board = humans % 3;
            let mut a = app(false, board, 90 + humans as u64);
            a.fast = true;
            a.humans = humans;
            a.cfg.turns = 10;
            a.cmds.push(UiCmd::Start);
            let (mut shell, mut slot) = (Shell::default(), ExperimentSlot::default());
            let mut n = 0u32;
            while a.screen != Screen::Results && n < 400_000 {
                if let Some(c) = person_acts(&a, n / 7) {
                    if n % 7 == 0 {
                        a.cmds.push(c);
                    }
                }
                tick(&mut a, &mut shell, &mut slot, 0.04, &mut |_, _| {});
                n += 1;
            }
            assert_eq!(a.screen, Screen::Results, "{humans} people, board {board}: stuck at {:?} / {:?}", a.game.as_ref().map(|g| g.phase.clone()), a.mini.as_ref().map(|m| m.stage));
            let g = a.game.as_ref().unwrap();
            assert!(g.over() && g.awards.len() == 3);
            assert_eq!((0..4).filter(|p| !g.is_cpu(*p)).count(), humans);
            // Leaving works from the results screen and nothing of the game leaks into the title.
            a.cmds.push(UiCmd::Menu);
            step(&mut a, 0.2);
            assert_eq!(a.screen, Screen::Title);
            assert!(a.game.is_none() && a.mini.is_none());
        }
    }
}
