//! Sound-cue trigger parity against `tests/rustFixtures.cues.gen.test.ts`: the web build's `Sfx.ts` calls for every Mario
//! event, object event and progression cue (engine-call streams recorded from the real `Sfx.ts`) must equal the engine
//! calls the Rust dispatch (`dispatch_sim_event`, `apply_cue`: the functions the Bevy systems use) produces.
use super::processor::{Cmd, MasterGain, MusicEvent, Processor};
use super::sfx::{course_music, PeachVoice, PlayerState, Sfx, SignCue};
use super::{apply_cue, dispatch_sim_event, SfxCue};
use crate::game::runner::SimEvent;
use jumpman_sim::object::ObjId;
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::MarioEvent;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::mpsc::channel;

fn leak(v: &Value) -> &'static str {
    Box::leak(v.as_str().unwrap().to_string().into_boxed_str())
}

fn pos(v: &Value) -> [f64; 3] {
    let a = v.as_array().unwrap();
    [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()]
}

fn mario_event(e: &Value) -> MarioEvent {
    let b = |k: &str| e[k].as_bool().unwrap();
    match e["type"].as_str().unwrap() {
        "jump" => MarioEvent::Jump { action: e["action"].as_u64().unwrap() as u32, vy: e["vy"].as_f64().unwrap() },
        "land" => MarioEvent::Land { speed: e["speed"].as_f64().unwrap(), vy: e["vy"].as_f64().unwrap() },
        "wingFlight" => MarioEvent::WingFlight { pull_up: b("pullUp") },
        "flightBoundary" => MarioEvent::FlightBoundary,
        "flightBurn" => MarioEvent::FlightBurn,
        "bonk" => MarioEvent::Bonk { speed: e["speed"].as_f64().unwrap() },
        "wallkick" => MarioEvent::Wallkick,
        "groundpound" => MarioEvent::Groundpound,
        "cannonMove" => MarioEvent::CannonMove { phase: e["phase"].as_u64().unwrap() as u8 },
        "cannonFire" => MarioEvent::CannonFire,
        "waterPlunge" => MarioEvent::WaterPlunge,
        "flutterKick" => MarioEvent::FlutterKick,
        "bowserGrab" => MarioEvent::BowserGrab,
        "bowserSpin" => MarioEvent::BowserSpin,
        "bowserThrow" => MarioEvent::BowserThrow { fast: b("fast") },
        "swimStroke" => MarioEvent::SwimStroke { fast: b("fast") },
        "groundpounddrop" => MarioEvent::GroundpoundDrop,
        "groundpoundland" => MarioEvent::GroundpoundLand,
        "death" => MarioEvent::Death,
        "longjump" => MarioEvent::Longjump,
        "triplejump" => MarioEvent::Triplejump,
        "dive" => MarioEvent::Dive,
        "step" => MarioEvent::Step { speed: e["speed"].as_f64().unwrap() },
        "castleEnding" => MarioEvent::CastleEnding,
        "endingCredits" => MarioEvent::EndingCredits,
        "deathLanding" => MarioEvent::DeathLanding,
        "mamaMia" => MarioEvent::MamaMia,
        "respawn" => MarioEvent::Respawn,
        "hurt" => MarioEvent::Hurt { damage: e["damage"].as_f64().unwrap() },
        other => panic!("mario event {other}"),
    }
}

fn object_event(e: &Value) -> ObjectEvent {
    let p = || pos(&e["pos"]);
    match e["type"].as_str().unwrap() {
        "waterBomb" => ObjectEvent::WaterBomb { pos: p(), phase: leak(&e["phase"]) },
        "waterCannon" => ObjectEvent::WaterCannon { pos: p() },
        "explosion" => ObjectEvent::Explosion { pos: p(), radius: e["radius"].as_f64().unwrap(), mine: e["mine"].as_bool().unwrap() },
        "break" => ObjectEvent::Break { pos: p(), wall: e.get("wall").map(leak) },
        "bowlingBall" => ObjectEvent::BowlingBall { pos: p(), cue: leak(&e["cue"]) },
        "koopaSound" => ObjectEvent::KoopaSound { pos: p(), cue: leak(&e["cue"]) },
        "coinBounce" => ObjectEvent::CoinBounce { pos: p() },
        "movingCoinCollected" => ObjectEvent::MovingCoinCollected { pos: p() },
        "coin" => ObjectEvent::Coin { pos: p(), loot: e["loot"].as_bool().unwrap(), value: e["value"].as_f64().unwrap() },
        "hurt" => ObjectEvent::Hurt { pos: p() },
        "kingLeave" => ObjectEvent::KingLeave,
        "kingDialog" => ObjectEvent::KingDialog { king: ObjId(0), id: e["id"].as_f64().unwrap() },
        "boss" => ObjectEvent::Boss { pos: p() },
        "bigBoo" => ObjectEvent::BigBoo { pos: p() },
        "oneUp" => ObjectEvent::OneUp { pos: p() },
        "whompKing" => ObjectEvent::WhompKing { pos: p() },
        "bigBully" => ObjectEvent::BigBully { pos: p() },
        "bowser" => ObjectEvent::Bowser { pos: p() },
        "chest" => ObjectEvent::Chest { number: e["number"].as_f64().unwrap() },
        "boxStar" => ObjectEvent::BoxStar { pos: p(), act: e["act"].as_f64().unwrap() },
        "snowman" => ObjectEvent::Snowman { pos: p(), cue: leak(&e["cue"]) },
        "racerSound" => ObjectEvent::RacerSound { pos: p(), cue: leak(&e["cue"]) },
        "penguinSound" => ObjectEvent::PenguinSound { pos: p(), diving: e["diving"].as_bool().unwrap() },
        "penguinReward" => ObjectEvent::PenguinReward { pos: p() },
        "chestReward" => ObjectEvent::ChestReward,
        "race" => ObjectEvent::Race { state: leak(&e["state"]) },
        "dialogue" => ObjectEvent::Dialogue { text: e["text"].as_str().unwrap().to_string() },
        other => panic!("object event {other}"),
    }
}

fn music_event(name: &str) -> MusicEvent {
    match name {
        "clear" => MusicEvent::Clear,
        "exitKey" => MusicEvent::ExitKey,
        "exitStar" => MusicEvent::ExitStar,
        "spawn" => MusicEvent::Spawn,
        "puzzle" => MusicEvent::Puzzle,
        "kingIntro" => MusicEvent::KingIntro,
        "kingFight" => MusicEvent::KingFight,
        "kingDefeat" => MusicEvent::KingDefeat,
        "kingLeave" => MusicEvent::KingLeave,
        "raceStart" => MusicEvent::RaceStart,
        "raceEnd" => MusicEvent::RaceEnd,
        other => panic!("music event {other}"),
    }
}

fn cue(c: &Value) -> SfxCue {
    let p = || pos(&c["pos"]);
    match c["name"].as_str().unwrap() {
        "coin" => SfxCue::Coin { underwater: c["underwater"].as_bool().unwrap() },
        "star" => SfxCue::Star,
        "starSound" => SfxCue::StarSound,
        "oneUp" => SfxCue::OneUp,
        "teleport" => SfxCue::Teleport,
        "capSwitch" => SfxCue::CapSwitch,
        "deathWarp" => SfxCue::DeathWarp,
        "bonk" => SfxCue::Bonk,
        "stompGoomba" => SfxCue::StompGoomba,
        "signMessage" => SfxCue::SignMessage(match c["cue"].as_str().unwrap() {
            "read" => SignCue::Read,
            "open" => SignCue::Open,
            "page" => SignCue::Page,
            _ => SignCue::Close,
        }),
        "courseCoin" => SfxCue::CourseCoin { count: c["count"].as_u64().unwrap() as u32, record: c["record"].as_bool().unwrap() },
        "secret" => SfxCue::Secret(c["count"].as_u64().unwrap() as u32),
        "redCoin" => SfxCue::RedCoin(c["count"].as_u64().unwrap() as u32),
        "spawnedStar" => SfxCue::SpawnedStar { pos: p(), appearing: c["appearing"].as_bool().unwrap() },
        "coordinateStar" => SfxCue::CoordinateStar { pos: p(), landed: c["landed"].as_bool().unwrap() },
        "cannonDoor" => SfxCue::CannonDoor { pos: p() },
        "lift" => SfxCue::Lift { pos: p(), cue: leak(&c["cue"]) },
        "moveMenu" => SfxCue::MoveMenu,
        "selectMission" => SfxCue::SelectMission,
        "peachVoice" => SfxCue::Peach(match c["voice"].as_str().unwrap() {
            "peachMario" => PeachVoice::Mario,
            "peachStars" => PeachVoice::Stars,
            "peachThanks" => PeachVoice::Thanks,
            "peachThankYou" => PeachVoice::ThankYou,
            "peachSpecial" => PeachVoice::Special,
            "peachCake" => PeachVoice::Cake,
            "peachForMario" => PeachVoice::ForMario,
            "peachMario2" => PeachVoice::Mario2,
            other => panic!("voice {other}"),
        }),
        "musicEvent" => SfxCue::Music(music_event(c["event"].as_str().unwrap())),
        "stopMusic" => SfxCue::StopMusic,
        other => panic!("cue {other}"),
    }
}

#[test]
fn sound_cues_match_the_web_build() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sim/tests/fixtures/cues.json");
    let fx: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let (tx, rx) = channel();
    let mut sfx = Sfx::new(tx, MasterGain::new(0.0), false);
    sfx.start_music(course_music("bob", 1));
    sfx.set_player(&PlayerState { camera_space_pos: [0.0; 3], metal: false, floor_type: Some(1), underwater_floor: false });
    while rx.try_recv().is_ok() {}
    let identity = |p: [f64; 3]| p;
    let mut failures = vec![];
    let mut override_music = false;
    for c in fx["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        match c["kind"].as_str().unwrap() {
            "mario" => {
                let events: Vec<SimEvent> = c["events"].as_array().unwrap().iter().map(|e| SimEvent::Mario(mario_event(e))).collect();
                let has_pound_land = events.iter().any(|e| matches!(e, SimEvent::Mario(MarioEvent::GroundpoundLand)));
                let mut coins = 0;
                for e in &events {
                    dispatch_sim_event(&mut sfx, e, has_pound_land, &identity, &mut coins);
                }
            }
            "object" => {
                let e = SimEvent::Object(object_event(&c["events"][0]));
                let mut coins = 0;
                dispatch_sim_event(&mut sfx, &e, false, &identity, &mut coins);
            }
            _ => apply_cue(&mut sfx, &cue(&c["cue"]), &identity, &mut override_music, &|| course_music("bob", 1)),
        }
        let mut got: Vec<(String, Vec<f64>)> = vec![];
        while let Ok(cmd) = rx.try_recv() {
            let cmd: Cmd = cmd;
            for (n, a) in Processor::calls(&cmd) {
                got.push((n.to_string(), a));
            }
        }
        let want: Vec<(String, Vec<f64>)> = c["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (x[0].as_str().unwrap().to_string(), x[1].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()))
            .collect();
        if got != want {
            failures.push(format!("{name}: Rust {got:?}  vs TS {want:?}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), fx["cases"].as_array().unwrap().len(), failures.join("\n"));
}
