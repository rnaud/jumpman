//! Full-race regression for the CPU drivers: every one of the 16 race tracks, a scripted player (`kart_cpu::Bot`)
//! plus the three CPU karts, three laps, from the real exports in `public/mk64` and `assets/courses`.

use super::kart::{Raceway, WorldGround, TRACKS};
use super::kart_cpu::{Bot, CpuAi, RaceSetup};
use super::kart_physics::Kart;
use super::kart_race::{format_ticks, KartRace, Phase, LAPS, RACERS};
use super::kart_stats::Cc;

/// Most Lakitu rescues one CPU may need in a 3-lap race.
const MAX_CPU_RESCUES: u32 = 10;

fn roots() -> (std::path::PathBuf, std::path::PathBuf) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    (base.join("../public/mk64"), base.join("../assets"))
}

pub struct Outcome {
    pub name: &'static str,
    pub ticks: u32,
    pub finish: Vec<Option<u32>>,
    pub laps: Vec<Vec<u32>>,
    pub rescues: Vec<u32>,
    pub order: Vec<usize>,
    pub log: Vec<String>,
}

/// Race `dir` with the player scripted; stops when every racer finished or after `max_ticks`.
pub fn race_track(dir: &str, name: &'static str, cc: Cc, character: usize, max_ticks: u32) -> Option<Outcome> {
    race_track_mirrored(dir, name, cc, character, max_ticks, false)
}

pub fn race_track_mirrored(dir: &str, name: &'static str, cc: Cc, character: usize, max_ticks: u32, mirror: bool) -> Option<Outcome> {
    let (root, assets) = roots();
    if !root.join(format!("{}.json", dir.replace('_', "-"))).exists() || !root.join("cpu-ai.json").exists() {
        return None;
    }
    let ai = CpuAi::load(&root).expect("cpu-ai.json");
    let data = Raceway::read_course(&root, Some(&assets), dir).unwrap_or_else(|e| panic!("{name}: {e}"));
    let data = if mirror { data.mirrored() } else { data };
    let world = data.world();
    let ground = WorldGround(&world);
    let mut race = KartRace::new(&data.path, 20240607);
    let setup = RaceSetup { cc, character, course: ai.course(dir), tables: Some(ai.kart.clone()) };
    race.configure(setup.clone(), dir == "toads_turnpike");
    race.assist_player = true;
    let mut player = Kart::new([0.; 3], 0);
    race.start(&mut player, &ground);
    let mut bot = Bot::new(race.racers[0].hint);
    let mut ticks = 0;
    while ticks < max_ticks && race.racers.iter().any(|r| r.finish.is_none()) {
        let pad = bot.input(&setup.course, &race.path, &player, race.phase != Phase::Countdown);
        race.tick(&pad, false, &mut player, &ground);
        ticks += 1;
    }
    Some(Outcome {
        name,
        ticks,
        finish: race.racers.iter().map(|r| r.finish).collect(),
        laps: race.racers.iter().map(|r| r.laps.clone()).collect(),
        rescues: race.racers.iter().map(|r| r.rescues).collect(),
        order: race.order(),
        log: race.rescue_log.iter().take(40).map(|(i, t, p, n, why)| format!("  racer {i} t={t} {why} at ({:.0},{:.0},{:.0}) point {n}/{}", p[0], p[1], p[2], race.path.len())).collect(),
    })
}

/// Every race track: three CPU karts and the scripted player finish three laps, with few Lakitu rescues, and the
/// CPUs' pace is within reach of the player's.
#[test]
fn every_track_can_be_raced_for_three_laps() {
    let mut report = String::new();
    let mut failures = vec![];
    let mut winners = std::collections::BTreeSet::new();
    for track in TRACKS.iter() {
        let Some(o) = race_track(track.dir, track.name, Cc::Cc150, 0, 30 * 60 * 9) else { continue };
        let best = |i: usize| o.laps[i].iter().copied().min();
        let line = format!(
            "{:<22} {:>6} ticks | finish {:?} | best lap {:?} | rescues {:?} | order {:?}\n",
            o.name,
            o.ticks,
            o.finish.iter().map(|f| f.map(format_ticks)).collect::<Vec<_>>(),
            (0..RACERS).map(|i| best(i).map(format_ticks)).collect::<Vec<_>>(),
            o.rescues,
            o.order
        );
        report.push_str(&line);
        let finished = o.finish.iter().all(|f| f.is_some()) && o.laps.iter().all(|l| l.len() as u32 >= LAPS);
        // Lakitu: jumps (Wario Stadium, Royal Raceway, D.K.'s Jungle Parkway) cost one rescue each because the ramp
        // boost is not ported; rail-less edges (Bowser's Castle, Banshee Boardwalk, Rainbow Road) a few more.
        let calm = o.rescues[1..].iter().all(|r| *r <= MAX_CPU_RESCUES);
        // Pace: the CPUs' mean best lap within 15 % of the scripted player's best lap.
        let cpu_best: Vec<f64> = (1..RACERS).filter_map(|i| best(i).map(|t| t as f64)).collect();
        let pace = match (best(0), cpu_best.len()) {
            (Some(me), 3) => (cpu_best.iter().sum::<f64>() / 3. / me as f64 - 1.).abs() <= 0.15,
            _ => false,
        };
        winners.insert(o.order[0]);
        report.push_str(&o.log.join("\n"));
        report.push('\n');
        if !finished || !calm || !pace {
            failures.push(line.trim_end().to_string());
            report.push_str(&o.log.join("\n"));
            report.push('\n');
        }
    }
    println!("{report}");
    assert!(failures.is_empty(), "races that did not complete cleanly:\n{}", failures.join("\n"));
    assert!(winners.len() >= 2, "the same racer won everywhere: {winners:?}");
}

/// `gCCSelection`: the same field is slower at 50cc than at 150cc (top speeds 290 vs 320, CPU target speeds 3.75 vs 5.75).
#[test]
fn engine_classes_set_the_pace() {
    let total = |cc| race_track("luigi_raceway", "Luigi Raceway", cc, 0, 30 * 60 * 6).map(|o| o.finish[1..].iter().map(|f| f.expect("cpu finished") as f64).sum::<f64>());
    let (Some(slow), Some(fast)) = (total(Cc::Cc50), total(Cc::Cc150)) else { return };
    println!("50cc CPU total {slow} ticks, 150cc {fast}");
    assert!(slow > fast * 1.1, "50cc {slow} vs 150cc {fast}");
}

/// Extra mode mirrors the course (x negated for the drawing, the collision and the path); the CPUs still lap it.
#[test]
fn the_mirrored_course_is_raced_too() {
    let Some(o) = race_track_mirrored("mario_raceway", "Mario Raceway", Cc::Extra, 0, 30 * 60 * 6, true) else { return };
    assert!(o.finish.iter().all(|f| f.is_some()), "finish {:?} rescues {:?}", o.finish, o.rescues);
    assert!(o.rescues[1..].iter().all(|r| *r <= 3), "rescues {:?}", o.rescues);
}

/// The racer matters: a light kart (Toad) and a heavy one (Bowser) do not post the same time as Mario.
#[test]
fn racers_drive_differently() {
    let time = |c| race_track("luigi_raceway", "Luigi Raceway", Cc::Cc150, c, 30 * 60 * 6).map(|o| o.finish[0].expect("player finished"));
    let (Some(mario), Some(toad), Some(bowser)) = (time(0), time(3), time(7)) else { return };
    println!("player Mario {mario}, Toad {toad}, Bowser {bowser}");
    assert!(mario != toad || mario != bowser);
}
