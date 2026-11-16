//! Balance harness: AI-vs-AI round robin in Gem Clash between the 12 Smash fighters and Gem Clash's own heroes (Mario,
//! Link, Young Link, Banjo), headless and seeded, both seat orders, on the six box arenas; and a minigame score per
//! fighter (specials against a Star Swarm pack, `smash_targets::SwarmTargets`).
//!
//! - `cargo test --release -p jumpman balance_full -- --ignored --nocapture` prints the full table
//!   (`JUMPMAN_BALANCE_SEEDS`, default 6 per arena; `JUMPMAN_BALANCE_ITEMS=1` with SSB64 items;
//!   `JUMPMAN_SMASH_MOTION_EVENTS=<motion-events.json>` with the import's scripted hitboxes, as the game plays).
//! - `smash_balance_regression` (always on) runs a small fast version and checks every fighter stays viable.
use super::*;
use jumpman_sim::smash_hitbox::MotionEvents;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Who {
    Hero(Hero),
    Smash(SmashFighter),
}

impl Who {
    pub fn name(self) -> &'static str {
        match self {
            Who::Hero(h) => h.name(),
            Who::Smash(s) => smash::name(s),
        }
    }
}

/// Everyone in the round robin.
pub fn roster() -> Vec<Who> {
    let mut v: Vec<Who> = SmashFighter::ALL.iter().map(|s| Who::Smash(*s)).collect();
    v.extend(HEROES.iter().map(|h| Who::Hero(*h)));
    v
}

fn scripts() -> Option<Arc<MotionEvents>> {
    let p = std::env::var_os("JUMPMAN_SMASH_MOTION_EVENTS")?;
    let text = std::fs::read_to_string(p).ok()?;
    MotionEvents::from_json(&text).ok().map(Arc::new)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Tally {
    pub won: u32,
    pub played: u32,
    pub damage: f32,
    pub seconds: f32,
    pub kos: u32,
    pub blast_kos: u32,
    pub specials: u32,
}

/// One duel to one round: `a` in seat 0, `b` in seat 1, Normal bots. Returns (winner seat or -1, per-seat tallies).
pub fn duel(a: Who, b: Who, seed: u64, arena: ArenaKind, items: bool, scripts: &Option<Arc<MotionEvents>>) -> (i8, [Tally; 2]) {
    let mut cfg = Config { seed, humans: 0, bots: 2, arena, same_kind: Some(Kind::Mario), rounds_to_win: 1, smash_items: items, ..Default::default() };
    cfg.diffs = [Some(Diff::Normal), Some(Diff::Normal), None, None];
    for (i, w) in [a, b].into_iter().enumerate() {
        match w {
            Who::Hero(h) => {
                if i == 0 {
                    cfg.hero = h;
                } else {
                    cfg.heroes[1] = Some(h);
                }
            }
            Who::Smash(s) => cfg.smash[i] = Some(s),
        }
    }
    let mut br = Brawl::new(cfg);
    br.set_scripts(scripts.clone());
    let mut t = [Tally::default(); 2];
    let mut ticks = 0;
    let idle = [Input::default(); MAXF];
    while br.phase != Phase::MatchEnd && ticks < 30 * 60 * 4 {
        let before = [br.f[0].alive, br.f[1].alive];
        br.tick(&idle);
        ticks += 1;
        for i in 0..2 {
            if br.f[i].act == Act::Special && br.f[i].act_t == 1 {
                t[i].specials += 1;
            }
            if before[i] && !br.f[i].alive {
                let o = 1 - i;
                t[o].kos += 1;
                if br.f[i].hp > 0.0 {
                    t[o].blast_kos += 1;
                }
            }
        }
    }
    for i in 0..2 {
        t[i].damage = br.f[i].dmg_dealt;
        t[i].seconds = br.total_ticks as f32 / 30.0;
        t[i].played = (br.match_winner >= 0) as u32;
        t[i].won = (br.match_winner == i as i8) as u32;
    }
    (br.match_winner, t)
}

/// Round robin: every pair, `seeds` seeds per arena, both seat orders, on up to 3 threads. Returns per-roster tallies.
pub fn round_robin(seeds: u64, arenas: &[ArenaKind], items: bool) -> Vec<(Who, Tally)> {
    let r = roster();
    let sc = scripts();
    let mut jobs = vec![];
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            for (k, arena) in arenas.iter().enumerate() {
                for s in 0..seeds {
                    jobs.push((i, j, *arena, s * 31 + k as u64 * 7 + (i * 131 + j) as u64));
                }
            }
        }
    }
    let threads = 3;
    let chunks: Vec<Vec<(usize, usize, ArenaKind, u64)>> = (0..threads).map(|t| jobs.iter().skip(t).step_by(threads).copied().collect()).collect();
    let results: Vec<Vec<(usize, usize, [Tally; 2])>> = std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                let r = r.clone();
                let sc = sc.clone();
                scope.spawn(move || {
                    let mut out = vec![];
                    for (i, j, arena, seed) in chunk {
                        let (_, t) = duel(r[i], r[j], seed, arena, items, &sc);
                        out.push((i, j, t));
                        let (_, t) = duel(r[j], r[i], seed ^ 0x55, arena, items, &sc);
                        out.push((j, i, t));
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut tally = vec![Tally::default(); r.len()];
    for (a, b, t) in results.into_iter().flatten() {
        for (who, tt) in [(a, t[0]), (b, t[1])] {
            let x = &mut tally[who];
            x.won += tt.won;
            x.played += tt.played;
            x.damage += tt.damage;
            x.seconds += tt.seconds;
            x.kos += tt.kos;
            x.blast_kos += tt.blast_kos;
            x.specials += tt.specials;
        }
    }
    r.into_iter().zip(tally).collect()
}

pub fn print_table(title: &str, rows: &[(Who, Tally)]) {
    println!("BALANCE {title}");
    println!("BALANCE {:<12} {:>6} {:>7} {:>6} {:>6} {:>8} {:>9}", "fighter", "win%", "played", "dmg/s", "KOs", "blastKO", "specials");
    for (w, t) in rows {
        println!(
            "BALANCE {:<12} {:>5.0}% {:>7} {:>6.2} {:>6} {:>8} {:>9}",
            w.name(),
            100.0 * t.won as f32 / t.played.max(1) as f32,
            t.played,
            t.damage / t.seconds.max(1.0),
            t.kos,
            t.blast_kos,
            t.specials
        );
    }
}

/// Minigame score: a fighter alone on a flat floor against a Star Swarm pack (24 Skitters in a ring 500 away, closing in
/// at 240 units a second)
/// for 30 s, firing a special whenever free (cycling neutral / up / down, facing the nearest foe). The foes cannot die
/// (huge health): the score is the SSB64 percent dealt per second (`SwarmTargets` turns percent into swarm damage).
/// The fighter's score is its best special's (a player spams the move that works; cycling all three makes a dash move
/// like the Falcon Kick carry the fighter away from the pack and measures the bot, not the fighter).
pub fn swarm_score(f: SmashFighter, seed: u64, scripts: &Option<Arc<MotionEvents>>) -> f32 {
    (0..3u8).map(|k| swarm_score_slots(f, seed, scripts, &[k])).fold(0.0, f32::max)
}

/// [`swarm_score`] using only the specials in `slots` (0 neutral, 1 up, 2 down).
pub fn swarm_score_slots(f: SmashFighter, seed: u64, scripts: &Option<Arc<MotionEvents>>, slots: &[u8]) -> f32 {
    use super::super::smash_targets::tests::smash_runtime;
    use super::super::smash_targets::SwarmTargets;
    use super::super::swarm_core::{EnemyKind, Swarm};
    use jumpman_sim::state::{ControllerSnapshot, A_BUTTON, R_TRIG, Z_TRIG};
    let mut m = smash_runtime(f);
    m.smash.scripts = scripts.clone();
    let mut s = Swarm::new(seed);
    for k in 0..24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        s.spawn_enemy(EnemyKind::Skitter, [a.sin() * 500.0, a.cos() * 500.0], 0.0);
    }
    for e in s.enemies.iter_mut().filter(|e| e.alive) {
        e.hp = 1.0e6;
        e.max_hp = 1.0e6;
    }
    let hp0: f32 = s.enemies.iter().filter(|e| e.alive).map(|e| e.hp).sum();
    let mut slot = 0;
    for t in 0..30 * 30 {
        // foes walk at the hero (the core's own AI needs a whole run: here they just close in)
        for e in s.enemies.iter_mut().filter(|e| e.alive) {
            let d = (e.pos[0] - m.pos[0] as f32).hypot(e.pos[1] - m.pos[2] as f32).max(1.0);
            if d > 60.0 {
                e.pos[0] -= (e.pos[0] - m.pos[0] as f32) / d * 8.0;
                e.pos[1] -= (e.pos[1] - m.pos[2] as f32) / d * 8.0;
            }
        }
        let near = s.enemies.iter().filter(|e| e.alive).min_by(|a, b| a.pos[0].hypot(a.pos[1]).total_cmp(&b.pos[0].hypot(b.pos[1])));
        let mut c = ControllerSnapshot::default();
        let mut aim = None;
        if let Some(e) = near {
            aim = Some(jumpman_sim::math::atan2s((e.pos[1] - m.pos[2] as f32) as f64, (e.pos[0] - m.pos[0] as f32) as f64) as f64);
            let d = [e.pos[0] - m.pos[0] as f32, e.pos[1] - m.pos[2] as f32];
            let l = d[0].hypot(d[1]).max(1.0);
            // camera yaw 0: world (x, z) = stick (x, -y)
            c.stick_x = (d[0] / l * 64.0) as f64;
            c.stick_y = (-d[1] / l * 64.0) as f64;
            c.stick_mag = 64.0;
        }
        if !m.smash.active() && t % 6 == 0 {
            // the bot turns to the nearest foe, then presses the special
            if let Some(y) = aim {
                m.face_angle[1] = y;
            }
            c.button_down |= R_TRIG;
            c.button_pressed |= R_TRIG;
            match slots[slot % slots.len()] {
                1 => c.button_down |= A_BUTTON,
                2 => c.button_down |= Z_TRIG,
                _ => {}
            }
            c.stick_x = 0.0;
            c.stick_y = 0.0;
            c.stick_mag = 0.0;
            slot += 1;
        } else if let Some(sp) = m.smash.special {
            // release a charge (Giant Punch, Charge Shot) once it has built up
            use jumpman_sim::smash::{Phase as P, SpecialMove as M};
            if matches!(sp.mv, M::GiantPunch | M::ChargeShot) && sp.phase == P::Hold && sp.t > 40 {
                c.button_pressed |= R_TRIG;
                c.button_down |= R_TRIG;
            }
        }
        m.step(&c, 0.0, false, false);
        if std::env::var_os("JUMPMAN_BALANCE_TRACE").is_some() && t % 15 == 0 {
            eprintln!("t {t} {f:?} act {:x} pos {:?} special {:?}", m.action, m.pos, m.smash.special.map(|s| (s.mv, s.phase, s.t)));
        }
        let mut tg = SwarmTargets::new(&mut s, [0.0, 0.0], |_| 0.0);
        jumpman_sim::smash::resolve(&mut m.smash, &mut tg);
        if m.pos[1] < -500.0 {
            break;
        }
    }
    let hp1: f32 = s.enemies.iter().filter(|e| e.alive).map(|e| e.hp).sum();
    let per_percent = s.stats.damage.max(1.0) / super::super::smash_targets::SWARM_PERCENT_PER_HIT;
    (hp0 - hp1).max(0.0) / per_percent / 30.0
}

/// The always-on version: each Smash fighter against Mario and against one other fighter, a few seeds on two arenas;
/// every fighter must win some and lose some (no one-sided matchup), and no minigame score may run away.
#[test]
fn smash_balance_regression() {
    let arenas = [ArenaKind::Bumpers, ArenaKind::Gearworks];
    let sc = scripts();
    let mut rates = vec![];
    for (n, f) in SmashFighter::ALL.iter().enumerate() {
        let foe = SmashFighter::ALL[(n + 5) % 12];
        let (mut won, mut played, mut secs) = (0, 0, 0.0f32);
        for (k, arena) in arenas.iter().enumerate() {
            for s in 0..3u64 {
                for (opp, flip) in [(Who::Hero(Hero::Mario), false), (Who::Smash(foe), true)] {
                    let seed = s * 17 + k as u64 + n as u64 * 101;
                    let (w, t) = if flip { duel(opp, Who::Smash(*f), seed, *arena, false, &sc) } else { duel(Who::Smash(*f), opp, seed, *arena, false, &sc) };
                    secs += t[0].seconds;
                    if w >= 0 {
                        played += 1;
                        won += ((w == 0) != flip) as u32;
                    }
                }
            }
        }
        let r = won as f32 / played.max(1) as f32;
        println!("BALANCE regression {:<12} {:>3.0}% of {played}, {:.0} s a duel", smash::name(*f), r * 100.0, secs / 12.0);
        rates.push((*f, r, played));
    }
    for (f, r, played) in &rates {
        assert!(*played >= 6, "{f:?}: too few decided duels ({played})");
        assert!((0.08..=0.92).contains(r), "{f:?} wins {:.0}%: one-sided", r * 100.0);
    }
    let scores: Vec<f32> = SmashFighter::ALL.iter().map(|f| swarm_score(*f, 3, &sc)).collect();
    let mut sorted = scores.clone();
    sorted.sort_by(f32::total_cmp);
    let median = (sorted[5] + sorted[6]) / 2.0;
    for (f, s) in SmashFighter::ALL.iter().zip(&scores) {
        println!("BALANCE swarm score {:<12} {:>6.1} (median {median:.1})", smash::name(*f), s);
        // tuned on the import's scripted hitboxes (the game's); the approximate fallback hitboxes only get a loose bound
        let bound = if sc.is_some() { 1.35 } else { 2.5 };
        assert!(*s <= median * bound + 1.0, "{f:?} swarm score {s} runs away from the median {median}");
    }
}

/// Per-special minigame scores (`-- --ignored --nocapture`).
#[test]
#[ignore]
fn balance_swarm_slots() {
    let sc = scripts();
    for f in SmashFighter::ALL {
        let s: Vec<String> = (0..3u8).map(|k| format!("{:>8.1}", swarm_score_slots(f, 3, &sc, &[k]))).collect();
        let all = swarm_score_slots(f, 3, &sc, &[0, 1, 2]);
        println!("BALANCE slots {:<12} {}  cycling {:>8.1}", smash::name(f), s.join(" "), all);
    }
}

/// The full table (slow: `-- --ignored --nocapture`).
#[test]
#[ignore]
fn balance_full() {
    let seeds = std::env::var("JUMPMAN_BALANCE_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
    let items = std::env::var("JUMPMAN_BALANCE_ITEMS").is_ok_and(|v| v == "1");
    let t0 = std::time::Instant::now();
    let rows = round_robin(seeds, &ARENAS, items);
    print_table(&format!("round robin, {seeds} seeds x {} arenas x 2 seats, items {items}, scripts {}", ARENAS.len(), scripts().is_some()), &rows);
    let sc = scripts();
    let scores: Vec<f32> = SmashFighter::ALL.iter().map(|f| (0..3).map(|k| swarm_score(*f, 3 + k, &sc)).sum::<f32>() / 3.0).collect();
    let mut sorted = scores.clone();
    sorted.sort_by(f32::total_cmp);
    let median = (sorted[5] + sorted[6]) / 2.0;
    for (f, s) in SmashFighter::ALL.iter().zip(&scores) {
        println!("BALANCE swarm {:<12} {:>6.1}  x{:.2} of the median", smash::name(*f), s, s / median);
    }
    println!("BALANCE took {:.0} s", t0.elapsed().as_secs_f32());
}
