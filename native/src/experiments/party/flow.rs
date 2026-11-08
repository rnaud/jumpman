//! The per-frame flow of Sunstone Shindig: UI commands, the animation queue that plays the core's events, CPU turns, and the
//! mini-game runner (human input in, fixed 30 Hz steps, results out).
use super::app::*;
use super::board::*;
use super::common::*;
use super::game::*;
use super::minis::*;
use crate::audio::SfxCue;
use crate::experiments::{ExperimentSlot, Shell};
use jumpman_sim::state::A_BUTTON;
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
        Ev::Rolled { value, die, .. } => {
            cue(shell, "land");
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
            app.say(if *n == 1 { "The lava tide rises!" } else { "The lava rises again!" }, 0xff8a4a);
            cue(shell, "explosion");
            app.shake = 0.6;
        }
        Ev::MiniStart | Ev::MiniEnd | Ev::GameOver | Ev::Bonus { .. } => {}
    }
}

fn anim_done(app: &PartyApp, a: &Anim, t0: f64) -> bool {
    match a.ev {
        Ev::Step { p, .. } => {
            let elapsed = app.t - t0;
            let limit = if app.fast { 0.12 } else { 0.9 };
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
    }
}

/// Gather the live device state for the human seats of a running mini-game.
fn gather(run: &mut MiniRun, input: &PlayerInput, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, prev_a: &mut [bool; 4], tiles: usize) {
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
    let mut seats: Vec<(usize, f32, f32, bool, i32)> = vec![(0, x, y, a_down, 0)];
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
        seats.push((seat, ls.x, -ls.y, pad.pressed(GamepadButton::South), dir));
        seat += 1;
    }
    for (s, sx, sy, a, dir) in seats {
        if !run.human[s] {
            continue;
        }
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

fn mini_flow(app: &mut PartyApp, shell: &mut Shell, dt: f64, input: &PlayerInput, keys: &ButtonInput<KeyCode>, pads: &Query<&Gamepad>, prev_a: &mut [bool; 4]) {
    let (fast, autoplay) = (app.fast, app.autoplay);
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
                let tiles = run.scene.tiles.len();
                gather(run, input, keys, pads, prev_a, tiles);
                run.acc += dt as f32 * if fast { 6.0 } else { 1.0 };
                run.t += dt as f32;
                let mut steps = 0;
                while run.acc >= DT && steps < 12 && !run.session.done() {
                    let inp = run.inp;
                    run.session.step(&inp);
                    run.acc -= DT;
                    steps += 1;
                    for p in 0..4 {
                        run.inp[p].act = false;
                        run.inp[p].pick = None;
                    }
                }
                let mut s = Stage::default();
                run.session.game.scene(&mut s);
                run.scene = s;
                // Human feedback: score up = coin, down = bonk.
                let sc = run.session.game.scores();
                if run.human[0] {
                    if sc[0] > run.prev_scores[0] + 1e-3 && sc[0].is_finite() {
                        cue(shell, "coin");
                    } else if sc[0] < run.prev_scores[0] - 1e-3 && sc[0].is_finite() {
                        cue(shell, "bonk");
                    }
                }
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
            let delay = if app.fast { 0.04 } else { 0.7 };
            if app.t - app.think < delay {
                return;
            }
            app.think = app.t;
            let cmd = app.game.as_mut().and_then(|g| g.bot_cmd());
            if let Some(c) = cmd {
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
    mut first: Local<bool>,
    mut prev_a: Local<[bool; 4]>,
) {
    if *state != AppState::Playing {
        return;
    }
    let dt = time.delta_secs_f64().min(0.1);
    app.t += dt;
    app.shake = (app.shake - dt as f32 * 1.5).max(0.0);
    if !*first {
        *first = true;
        first_frame_hooks(&mut app);
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
        handle_ui(&mut app, &mut shell, &mut slot, c);
    }
    // Expire the banner / toast.
    if app.banner.as_ref().is_some_and(|b| app.t - b.1 > 1.6) {
        app.banner = None;
    }
    if app.toast.as_ref().is_some_and(|b| app.t - b.1 > 2.4) {
        app.toast = None;
    }
    match app.screen {
        Screen::Board => board_flow(&mut app, &mut shell, dt),
        Screen::Mini => mini_flow(&mut app, &mut shell, dt, &input, &keys, &pads, &mut prev_a),
        _ => {}
    }
    // The touch pad only shows while a stick game is being played.
    let want_pad = app.screen == Screen::Mini && app.mini.as_ref().is_some_and(|m| m.stage == MStage::Play && matches!(m.kind.info().control, Control::Stick | Control::StickA) && m.human[0]);
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
