//! Relic Descent against the research checklist (`MINIGAMES_RESEARCH.md` section 9, `MINIGAMES_AUDIT_3B.md`): the
//! criteria the deterministic core can measure, on the generated floors and on the real Ocarina of Time floors (from
//! the local ROM import; skipped without it). The `audit_*` tests print their numbers (`--nocapture`) and pin the
//! thresholds the audit fixed.
use super::tests::{play_run_as, BotStyle, EXPERT};
use super::*;
use std::sync::Arc;

const T: f32 = TICKS_PER_SEC as f32;

/// A new player: attacks all the time, dodges one telegraph in four, never casts a skill or drinks a potion.
const NOVICE: BotStyle = BotStyle { dodge_every: 4, skills: false, potions: false, stop_after_first_clear: false };

fn assets() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

/// The real floors of a run on the imported data, if any.
fn real_plan(seed: u64) -> Option<Arc<Vec<Dungeon>>> {
    super::super::relic_real::plan_run(&assets(), seed, &[]).ok().map(Arc::new)
}

fn secs(t: Option<u32>) -> String {
    t.map_or("-".into(), |t| format!("{:.0}", t as f32 / T))
}

/// Checklist 1: the first fight is winnable by a dodge-light new player (9 of 10 seeds clear their first room).
fn first_fights(plan: &dyn Fn(u64) -> Option<Arc<Vec<Dungeon>>>, label: &str) -> Option<(u32, u32)> {
    let mut won = 0;
    let n = 10u32;
    for seed in 1..=n as u64 {
        let p = plan(seed);
        if label == "real" && p.is_none() {
            return None;
        }
        let mut r = Relic::with_plan(seed, Hero::Mario, 1, 0, p);
        let log = play_run_as(&mut r, 30 * 60 * 6, 26.0, BotStyle { stop_after_first_clear: true, ..NOVICE });
        let ok = log.first_clear.is_some() && r.player.hp > 0.0;
        won += ok as u32;
        eprintln!("FIRST FIGHT {label} seed {seed}: cleared={ok} at {}s hp {:.0}/{:.0} hits {} dodges {}", secs(log.first_clear), r.player.hp, r.stats.max_hp, log.hits_taken, log.dodges);
    }
    eprintln!("FIRST FIGHT {label}: {won}/{n}");
    Some((won, n))
}

#[test]
fn audit_first_fight_is_winnable_by_a_dodge_light_player() {
    let (g, n) = first_fights(&|_| None, "generated").unwrap();
    let real = first_fights(&real_plan, "real");
    assert!(g * 10 >= n * 9, "generated floors: {g}/{n}");
    if let Some((w, n)) = real {
        assert!(w * 10 >= n * 9, "real floors: {w}/{n}");
    } else {
        eprintln!("no OoT data imported; real floors skipped");
    }
}

/// Whole runs on the real floors: the scripted player walks every room and then the exit, floor after floor, and kills
/// the Bomb King; with the numbers the checklist asks for (first drop, first rare, first skill point, minutes a floor,
/// run length).
fn runs(label: &str, style: BotStyle, seeds: std::ops::RangeInclusive<u64>, real: bool) -> Option<Vec<(bool, u32, Vec<u32>, RunSummary)>> {
    let mut out = vec![];
    for seed in seeds {
        let plan = if real {
            match real_plan(seed) {
                Some(p) => Some(p),
                None => return None,
            }
        } else {
            None
        };
        let keys: Vec<String> = plan.as_ref().map(|p| p.iter().filter_map(|d| d.real.as_ref().map(|r| r.key.clone())).collect()).unwrap_or_default();
        let mut r = Relic::with_plan(seed, Hero::Mario, 1, 0, plan);
        r.filter_commons = false;
        let log = play_run_as(&mut r, 30 * 60 * 45, 26.0, style);
        let mut floors = vec![];
        let mut last = 0;
        for e in &log.floor_ends {
            floors.push(e - last);
            last = *e;
        }
        let s = r.summary();
        eprintln!(
            "RUN {label} seed {seed} {keys:?}: won={} floor={} level={} run {:.1} min, floors {:?} s, first item {} s, first rare {} s, first level-up {} s, stuck {} ticks, potions {}, hits {}",
            r.won,
            r.floor,
            r.player.level,
            log.ticks as f32 / T / 60.0,
            floors.iter().map(|t| (*t as f32 / T).round() as u32).collect::<Vec<_>>(),
            secs(log.first_item),
            secs(log.first_rare),
            secs(log.first_point),
            log.stuck_ticks,
            log.potions_used,
            log.hits_taken
        );
        out.push((r.won, log.stuck_ticks, floors, s));
    }
    Some(out)
}

/// Checklist 7 and the "bot walks to the exit" gap: on the real dungeons a scripted player clears each floor, finds the
/// exit and takes it, and beats the boss lair, on nine seeds in ten; no floor takes it more than four minutes.
#[test]
fn audit_a_bot_completes_real_floors_end_to_end() {
    let Some(res) = runs("real", EXPERT, 1..=10, true) else { return eprintln!("no OoT data imported; skipped") };
    let wins = res.iter().filter(|r| r.0).count();
    eprintln!("REAL RUNS: {wins}/{}", res.len());
    assert!(wins * 10 >= res.len() * 9, "{wins}/{}", res.len());
    for (won, stuck, floors, s) in &res {
        if *won {
            assert_eq!(floors.len(), FLOORS as usize, "{s:?}");
            for t in floors {
                assert!(*t as f32 / T < 240.0, "a floor took {:.0} s", *t as f32 / T);
            }
        }
        assert!(*stuck < 30 * 20, "stuck {stuck} ticks");
    }
}

/// Numbers only (ignored by default, slow): the novice on whole runs, generated and real.
#[test]
#[ignore]
fn audit_novice_runs() {
    let _ = runs("generated-novice", NOVICE, 1..=10, false);
    let _ = runs("real-novice", NOVICE, 1..=10, true);
    let _ = runs("generated-expert", EXPERT, 1..=10, false);
}

/// Checklist 3: every enemy attack telegraphs for at least half a second (15 ticks): a wind-up pose (`Windup` /
/// `Fuse`) before the blow lands, or a ground marker (a delayed blast disc) at least that long. Measured on a lone foe
/// against a hero who stands still: for each hit taken, how long the warning before it lasted.
pub(super) fn telegraph_ticks(kind: EnemyKind) -> Option<u32> {
    let mut r = Relic::new(7, Hero::Mario, 2, 0);
    let c = r.start_pos();
    r.player.pos = c;
    r.stats.max_hp *= 100.0;
    r.player.hp = r.stats.max_hp;
    let i = r.spawn_enemy(kind, [c[0], c[1] + 260.0], 0, 0)?;
    let mut states: Vec<EState> = vec![];
    let mut worst: Option<u32> = None;
    // The last finished wind-up (length, tick it ended): a shot is warned by the wind-up that fired it.
    let mut last_windup: Option<(u32, usize)> = None;
    let mut run = 0u32;
    for _ in 0..30 * 30 {
        let blasts_before: Vec<(u16, bool)> = r.hazards.iter().map(|h| (h.total, h.alive && h.delay == 0)).collect();
        let f = PlayerFrame { pos: c, grounded: true, ..Default::default() };
        r.tick(&f, &Intent::default());
        r.player.iframes = 0;
        r.player.hp = r.stats.max_hp;
        let alive = r.enemies[i].alive;
        if !alive && !r.hazards.iter().any(|h| h.alive) && !r.ev.hurt {
            break;
        }
        let st = if alive { r.enemies[i].state } else { EState::Recover };
        if matches!(st, EState::Windup | EState::Fuse) {
            run += 1;
        } else if run > 0 {
            last_windup = Some((run, states.len()));
            run = 0;
        }
        states.push(st);
        if !r.ev.hurt {
            continue;
        }
        // A blast disc that went off this tick: its fuse was the warning.
        let warn = if r.ev.blast {
            // (A self-destruct's blast has no fuse of its own: the lit fuse before it was the warning.)
            let fuse = last_windup.filter(|(_, at)| states.len() - at < 5).map_or(0, |(n, _)| n);
            blasts_before.iter().filter(|b| b.1).map(|b| b.0 as u32).max().unwrap_or(0).max(fuse)
        } else if !matches!(st, EState::Dash | EState::Air | EState::Windup | EState::Fuse) {
            // Back in the chase: a shot in flight (warned by the wind-up that fired it) or an unannounced bite.
            last_windup.filter(|(_, at)| states.len() - at < 90).map_or(0, |(n, _)| n)
        } else {
            // Skip the strike itself (a dash, a hop, a dive), then count the wind-up before it.
            let mut k = states.len();
            while k > 0 && matches!(states[k - 1], EState::Dash | EState::Air) {
                k -= 1;
            }
            let mut n = 0;
            while k > 0 && matches!(states[k - 1], EState::Windup | EState::Fuse) {
                k -= 1;
                n += 1;
            }
            n
        };
        worst = Some(worst.map_or(warn, |w: u32| w.min(warn)));
    }
    worst
}

#[test]
fn audit_every_attack_telegraphs_half_a_second() {
    // (The Seer has no blow of its own: it summons Scamps, measured on their own.)
    let mut kinds = vec![EnemyKind::Scamp, EnemyKind::Bruiser, EnemyKind::Lancer, EnemyKind::Wraith, EnemyKind::Sapper];
    kinds.extend_from_slice(relic_foe_kinds());
    let mut short = vec![];
    for k in kinds {
        let t = telegraph_ticks(k);
        eprintln!("TELEGRAPH {k:?}: {}", t.map_or("never hit".into(), |t| format!("{t} ticks ({:.2} s)", t as f32 / T)));
        if let Some(t) = t {
            if t < 15 {
                short.push((k, t));
            }
        }
    }
    assert!(short.is_empty(), "attacks with less than 0.5 s of warning: {short:?}");
}
