//! Debug aid for the CPU drivers (ignored test): `JUMPMAN_TRACE_TRACK=<dir> JUMPMAN_TRACE_RACER=<0..3>
//! cargo test --release -p jumpman --lib trace_first_rescue -- --ignored --nocapture` prints the 80 ticks before that
//! racer's first rescue (position, heading, speed against the target, path point, lane, air/hop/drift flags).

use super::kart::{Raceway, WorldGround};
use super::kart_cpu::{Bot, CpuAi, RaceSetup};
use super::kart_physics::Kart;
use super::kart_race::{KartRace, Phase};
use super::kart_stats::Cc;

#[test]
#[ignore]
fn trace_first_rescue() {
    let dir = std::env::var("JUMPMAN_TRACE_TRACK").unwrap_or_else(|_| "wario_stadium".into());
    let who: usize = std::env::var("JUMPMAN_TRACE_RACER").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let skip: usize = std::env::var("JUMPMAN_TRACE_SKIP").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let (root, assets) = (base.join("../public/mk64"), base.join("../assets"));
    let ai = CpuAi::load(&root).unwrap();
    let data = Raceway::read_course(&root, Some(&assets), &dir).unwrap();
    let data = if std::env::var("JUMPMAN_TRACE_MIRROR").is_ok() { data.mirrored() } else { data };
    let world = data.world();
    let ground = WorldGround(&world);
    let mut race = KartRace::new(&data.path, 20240607);
    let setup = RaceSetup { cc: Cc::Cc150, character: 0, course: ai.course(&dir), tables: Some(ai.kart.clone()) };
    race.configure(setup.clone(), dir == "toads_turnpike");
    race.assist_player = true;
    let mut player = Kart::new([0.; 3], 0);
    race.start(&mut player, &ground);
    let mut bot = Bot::new(race.racers[0].hint);
    let mut history: std::collections::VecDeque<String> = Default::default();
    for t in 0..30 * 60 * 6 {
        let pad = bot.input(&setup.course, &race.path, &player, race.phase != Phase::Countdown);
        race.tick(&pad, false, &mut player, &ground);
        let k = if who == 0 { &player } else { &race.cpus[who - 1].kart };
        let (point, factor, target) = if who == 0 {
            (race.racers[0].hint, 0., 0.)
        } else {
            let d = &race.cpus[who - 1].driver;
            (d.point, d.factor, d.target_speed)
        };
        let floor = super::kart_physics::Ground::floor(&ground, k.pos[0], k.pos[1] + 30., k.pos[2]).map(|f| f.height);
        history.push_back(format!(
            "t={t} pos=({:.0},{:.1},{:.0}) floor={:?} rot={} spd={:.2} tgt={:.2} pt={point} f={:.2} air={} hop={} drift={} turn={:?} dec={}",
            k.pos[0], k.pos[1], k.pos[2], floor.map(|h| h.round()), k.rotation, k.speed, target, factor, k.midair, k.hop, k.drifting, k.cpu_turn, k.cpu_decel
        ));
        if history.len() > 80 {
            history.pop_front();
        }
        if race.racers[who].rescues as usize > skip {
            println!("{}", history.iter().cloned().collect::<Vec<_>>().join("\n"));
            println!("cause: {:?}", race.rescue_log.last());
            return;
        }
    }
    println!("no rescue for racer {who}");
}
