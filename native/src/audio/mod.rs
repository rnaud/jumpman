//! Audio: the original SM64 sequence player and sound mixer (`sm64-us.wasm`, interpreted by wasmi on
//! the audio thread) driven by the same cues as `Game.ts`/`Sfx.ts`.
//!
//! * [`Sfx`] (port of `Sfx.ts`) names sounds and posts [`Cmd`]s; the engine does panning, distance
//!   attenuation, reverb/underwater filtering and the course music itself.
//! * [`GameAudioPlugin`] plays the engine as a streaming Bevy audio source, picks music per course,
//!   turns `SimEventMsg`s into sounds, runs the footstep clock and tracks listener state.
//! * Other feature modules that need a sound send an [`SfxCue`] event (world-space positions).
//!
//! Env: `JUMPMAN_MUTE=1` starts muted; `M` toggles mute; `JUMPMAN_NO_AUDIO=1` skips loading the engine.
#![allow(dead_code)] // `Sfx`/`SfxCue` are the API other feature modules call into
mod engine;
mod footsteps;
mod processor;
mod sfx;
mod terrain;

pub use processor::MusicEvent;
pub use sfx::{course_music, MusicTrack, PeachVoice, SignCue, Sfx};

use crate::camera::MainCamera;
use crate::game::runner::{GameSim, SimEvent};
use crate::render::{to_bevy, AssetRoot, SCALE};
use crate::sim_plugin::SimEventMsg;
use bevy::audio::AddAudioSource;
use bevy::prelude::*;
use footsteps::{Footsteps, Step};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;
use processor::{MasterGain, Sm64Audio};
use sfx::{PlayerState, Pos};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

/// Request a sound from outside this module. Positions are sim-space (world) coordinates.
#[derive(Event, Clone, Debug)]
pub enum SfxCue {
    Coin { underwater: bool },
    Star,
    StarSound,
    OneUp,
    Teleport,
    CapSwitch,
    DeathWarp,
    Bonk,
    StompGoomba,
    SignMessage(SignCue),
    CourseCoin { count: u32, record: bool },
    Secret(u32),
    RedCoin(u32),
    SpawnedStar { pos: [f64; 3], appearing: bool },
    CoordinateStar { pos: [f64; 3], landed: bool },
    CannonDoor { pos: [f64; 3] },
    /// `cue`: moving | stop | lower | upper
    Lift { pos: [f64; 3], cue: &'static str },
    MoveMenu,
    SelectMission,
    Peach(PeachVoice),
    Music(MusicEvent),
    /// Replace the course music (victory, star select, credits...).
    StartMusic(MusicTrack),
    /// Go back to the current course's music.
    ResumeCourseMusic,
    StopMusic,
    SetMuted(bool),
}

/// Handle to the sound player for systems that need more than [`SfxCue`].
#[derive(Resource)]
pub struct GameAudio {
    pub sfx: Sfx,
    footsteps: Footsteps,
    /// Coin pickups not yet heard (one tick-pair each, `hudCoins` counting up in the TS).
    pending_coins: u32,
    hud_tick: u32,
    last_action: u32,
    course: (String, i32),
    override_music: bool,
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Sm64Audio>().add_event::<SfxCue>().add_systems(Startup, setup).add_systems(
            FixedUpdate,
            (
                listener.before(crate::SimSet),
                (react_to_sim, apply_cues).chain().after(crate::SimSet),
            ),
        );
        app.add_systems(Update, toggle_mute);
    }
}

fn setup(mut commands: Commands, mut assets: ResMut<Assets<Sm64Audio>>, root: Res<AssetRoot>) {
    let (tx, rx) = channel();
    let muted = std::env::var("JUMPMAN_MUTE").is_ok_and(|v| !v.is_empty() && v != "0");
    let gain = MasterGain::new(0.0);
    let sfx = Sfx::new(tx, gain.clone(), muted);
    if std::env::var("JUMPMAN_NO_AUDIO").is_err() {
        let blocks = processor::spawn_render_thread(root.0.join("audio/music/sm64-us.wasm"), rx);
        commands.spawn(AudioPlayer(assets.add(Sm64Audio { blocks: Arc::new(Mutex::new(blocks)), gain })));
    }
    commands.insert_resource(GameAudio {
        sfx,
        footsteps: Footsteps::default(),
        pending_coins: 0,
        hud_tick: 0,
        last_action: 0,
        course: (String::new(), 0),
        override_music: false,
    });
}

fn toggle_mute(keys: Res<ButtonInput<KeyCode>>, mut audio: ResMut<GameAudio>) {
    if keys.just_pressed(KeyCode::KeyM) {
        let m = !audio.sfx.muted;
        audio.sfx.set_muted(m);
    }
}

/// World (sim units) -> camera space in sim units (`applyMatrix4(camera.matrixWorldInverse)`).
fn camera_space(cam: &GlobalTransform, p: [f64; 3]) -> Pos {
    let v = cam.affine().inverse().transform_point3(to_bevy(p));
    [(v.x / SCALE) as f64, (v.y / SCALE) as f64, (v.z / SCALE) as f64]
}

fn course_track(sim: &GameSim) -> MusicTrack {
    let mut track = course_music(&sim.course.request.id, sim.course.request.area);
    if sim.mario.action == ACT_END_PEACH_CUTSCENE {
        track.preset = 7;
    }
    track
}

/// `Sfx.setPlayer` + course music selection, once per tick before Mario steps.
fn listener(sim: Res<GameSim>, mut audio: ResMut<GameAudio>, camera: Query<&GlobalTransform, With<MainCamera>>) {
    let Ok(cam) = camera.single() else { return };
    let m = &sim.mario;
    let key = (sim.course.request.id.clone(), sim.course.request.area);
    if audio.course != key {
        audio.course = key;
        audio.override_music = false;
        let track = course_track(&sim);
        audio.sfx.start_music(track);
    } else if !audio.override_music && audio.sfx.current_music().is_none() {
        let track = course_track(&sim);
        audio.sfx.start_music(track);
    }
    let lava = sim.course.request.id == "lll";
    let wet = !lava && m.floor_height < m.world.water_level(m.pos[0], m.pos[2]) - 10.0;
    audio.sfx.set_player(&PlayerState {
        camera_space_pos: camera_space(cam, m.pos),
        metal: m.metal_cap_timer > 0.0,
        floor_type: m.floor.as_ref().map(|f| f.kind),
        underwater_floor: wet,
    });
}

const CASTLE_BIRDS: [[f64; 3]; 3] = [[-7430.0, 1500.0, 873.0], [-80.0, 1500.0, 5004.0], [7131.0, 1500.0, -2989.0]];

fn react_to_sim(
    mut events: EventReader<SimEventMsg>,
    sim: Res<GameSim>,
    mut audio: ResMut<GameAudio>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
) {
    let Ok(cam) = camera.single() else { return };
    let m = &sim.mario;
    let batch: Vec<&SimEvent> = events.read().map(|e| &e.0).collect();
    let a = &mut *audio;
    let pos = |p: [f64; 3]| camera_space(cam, p);
    let swimming = m.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0;
    let has_pound_land = batch.iter().any(|e| matches!(e, SimEvent::Mario(MarioEvent::GroundpoundLand)));

    let before = a.last_action;
    a.last_action = m.action;
    // Mario actions that make sound every tick or on entry.
    if m.action == ACT_RIDING_SHELL_GROUND {
        a.sfx.ride_shell(m.floor.as_ref().is_some_and(|f| f.kind == jumpman_sim::surfaces::SURFACE_BURNING));
    }
    if m.action != before && (m.action == ACT_RIDING_SHELL_JUMP || m.action == ACT_RIDING_SHELL_FALL) {
        a.sfx.shell_jump();
    }

    for e in &batch {
        match e {
            SimEvent::Mario(ev) => match ev {
                MarioEvent::Jump { action, .. } => match *action {
                    ACT_DOUBLE_JUMP => a.sfx.double_jump(),
                    ACT_TRIPLE_JUMP => a.sfx.triple_jump(),
                    ACT_BACKFLIP => a.sfx.backflip(),
                    ACT_JUMP_KICK => a.sfx.kick(),
                    _ => a.sfx.jump(),
                },
                MarioEvent::Triplejump => a.sfx.triple_jump(),
                MarioEvent::Land { .. } => {
                    if !has_pound_land {
                        a.sfx.land();
                    }
                }
                MarioEvent::WingFlight { pull_up } => a.sfx.wing_flight(*pull_up),
                MarioEvent::FlightBoundary => a.sfx.flight_boundary(),
                MarioEvent::FlightBurn => a.sfx.flight_burn(),
                MarioEvent::Bonk { .. } => a.sfx.bonk(),
                MarioEvent::Wallkick => a.sfx.wall_kick(),
                MarioEvent::Groundpound => a.sfx.ground_pound_start(),
                MarioEvent::CannonMove { phase } => a.sfx.cannon_move(*phase),
                MarioEvent::CannonFire => a.sfx.cannon_fire(),
                MarioEvent::BowserGrab => a.sfx.bowser_grab(),
                MarioEvent::BowserSpin => a.sfx.bowser_spin(),
                MarioEvent::BowserThrow { fast } => a.sfx.bowser_throw(*fast),
                MarioEvent::WaterPlunge => a.sfx.splash(),
                MarioEvent::SwimStroke { fast } => a.sfx.swim(*fast),
                MarioEvent::FlutterKick => a.sfx.flutter(),
                MarioEvent::GroundpoundDrop => a.sfx.ground_pound_drop(),
                MarioEvent::GroundpoundLand => a.sfx.ground_pound(),
                MarioEvent::Longjump => a.sfx.long_jump(),
                MarioEvent::Dive => a.sfx.dive(),
                MarioEvent::Hurt { .. } => a.sfx.hurt(),
                MarioEvent::Death => a.sfx.death(),
                MarioEvent::DeathLanding => a.sfx.death_landing(),
                MarioEvent::MamaMia => a.sfx.mama_mia(),
                MarioEvent::Step { .. } | MarioEvent::CastleEnding | MarioEvent::EndingCredits | MarioEvent::Respawn => {}
            },
            SimEvent::Object(ev) => match ev {
                ObjectEvent::WaterBomb { pos: p, phase } => a.sfx.water_bomb(pos(*p), phase),
                ObjectEvent::WaterCannon { pos: p } => a.sfx.water_cannon(pos(*p)),
                ObjectEvent::Explosion { pos: p, mine, .. } => a.sfx.explosion(pos(*p), *mine),
                ObjectEvent::Break { pos: p, wall } => {
                    if let Some(w) = wall {
                        a.sfx.blast_wall(pos(*p));
                        if *w == "right" {
                            a.sfx.music_event(MusicEvent::Puzzle);
                        }
                    } else {
                        a.sfx.break_box(pos(*p));
                    }
                }
                ObjectEvent::Hurt { .. } => a.sfx.hurt(),
                ObjectEvent::Chest { number } => a.sfx.chest_answer(*number != 0.0),
                ObjectEvent::RacerSound { pos: p, cue } => a.sfx.racer_sound(pos(*p), cue),
                ObjectEvent::PenguinSound { pos: p, diving } => a.sfx.penguin(pos(*p), *diving),
                ObjectEvent::Snowman { pos: p, cue } => {
                    if *cue != "reward" {
                        a.sfx.snowman(pos(*p), cue);
                        if *cue == "assembled" {
                            a.sfx.music_event(MusicEvent::Puzzle);
                        }
                    }
                }
                ObjectEvent::ChestReward => a.sfx.music_event(MusicEvent::Puzzle),
                ObjectEvent::Bowser { .. } | ObjectEvent::BigBully { .. } | ObjectEvent::WhompKing { .. } | ObjectEvent::BigBoo { .. } => {
                    a.sfx.music_event(MusicEvent::Spawn)
                }
                ObjectEvent::OneUp { .. } => a.sfx.one_up(),
                ObjectEvent::KingLeave => a.sfx.music_event(MusicEvent::KingLeave),
                ObjectEvent::KingDialog { id, .. } => {
                    if *id == 17.0 {
                        a.sfx.music_event(MusicEvent::KingIntro);
                    }
                }
                ObjectEvent::Race { state } => match *state {
                    "start" => a.sfx.music_event(MusicEvent::RaceStart),
                    "won" => {
                        a.sfx.music_event(MusicEvent::RaceEnd);
                        a.sfx.star();
                    }
                    _ => {
                        a.sfx.music_event(MusicEvent::RaceEnd);
                        a.sfx.bonk();
                    }
                },
                ObjectEvent::BowlingBall { pos: p, cue } => {
                    if *cue != "end" {
                        a.sfx.ball_bounce(pos(*p));
                    }
                }
                ObjectEvent::KoopaSound { pos: p, cue } => a.sfx.koopa_sound(pos(*p), cue),
                ObjectEvent::CoinBounce { pos: p } => a.sfx.coin_bounce(pos(*p)),
                _ => {}
            },
            SimEvent::CoinCollected { value } => a.pending_coins += (*value).max(0) as u32,
        }
    }

    // Footsteps (walking clips' two step frames).
    let step = a.footsteps.step(&crate::game::sm64_animation::AnimInput::from_mario(m));
    if let (Some(step), Some(_)) = (step, m.floor.as_ref()) {
        a.sfx.footstep(step == Step::Tiptoe);
    }
    // The HUD coin counter ticks up every other step with a coin sound.
    a.hud_tick = a.hud_tick.wrapping_add(1);
    if a.hud_tick & 1 == 1 && a.pending_coins > 0 {
        a.pending_coins -= 1;
        a.sfx.coin(swimming);
    }

    // Power-up music, ambience and area/position-dependent music layers.
    let (metal, wing, vanish) = (m.metal_cap_timer, m.wing_cap_timer, m.vanish_cap_timer);
    let cap = metal.max(wing).max(vanish);
    let riding = m.riding_shell.is_some();
    let seq = if riding {
        0x8e
    } else if metal > 0.0 {
        0x0f
    } else if cap > 0.0 {
        0x0e
    } else {
        0
    };
    a.sfx.set_power_music(seq as f64, !riding && cap == 60.0);
    let stars = sim.stars;
    a.sfx.set_music_location(sim.course.request.area, m.floor.as_ref().and_then(|f| f.room).unwrap_or(0), m.pos, stars);
    if sim.course.request.id == "castle_grounds" && m.action != ACT_END_PEACH_CUTSCENE {
        a.sfx.set_outdoor_sources(CASTLE_BIRDS.iter().map(|b| pos(*b)).collect());
    }
}

fn apply_cues(mut cues: EventReader<SfxCue>, mut audio: ResMut<GameAudio>, sim: Res<GameSim>, camera: Query<&GlobalTransform, With<MainCamera>>) {
    let Ok(cam) = camera.single() else { return };
    let a = &mut *audio;
    for cue in cues.read() {
        let pos = |p: [f64; 3]| camera_space(cam, p);
        match cue {
            SfxCue::Coin { underwater } => a.sfx.coin(*underwater),
            SfxCue::Star => a.sfx.star(),
            SfxCue::StarSound => a.sfx.star_sound(),
            SfxCue::OneUp => a.sfx.one_up(),
            SfxCue::Teleport => a.sfx.teleport(),
            SfxCue::CapSwitch => a.sfx.cap_switch(),
            SfxCue::DeathWarp => a.sfx.death_warp(),
            SfxCue::Bonk => a.sfx.bonk(),
            SfxCue::StompGoomba => a.sfx.stomp_goomba(),
            SfxCue::SignMessage(c) => a.sfx.sign_message(*c),
            SfxCue::CourseCoin { count, record } => a.sfx.course_coin(*count, *record),
            SfxCue::Secret(n) => a.sfx.secret(*n),
            SfxCue::RedCoin(n) => a.sfx.red_coin(*n),
            SfxCue::SpawnedStar { pos: p, appearing } => a.sfx.spawned_star(pos(*p), *appearing),
            SfxCue::CoordinateStar { pos: p, landed } => a.sfx.coordinate_star(pos(*p), *landed),
            SfxCue::CannonDoor { pos: p } => a.sfx.cannon_door(pos(*p)),
            SfxCue::Lift { pos: p, cue } => a.sfx.lift(pos(*p), cue),
            SfxCue::MoveMenu => a.sfx.move_menu(),
            SfxCue::SelectMission => a.sfx.select_mission(),
            SfxCue::Peach(v) => a.sfx.peach_voice(*v),
            SfxCue::Music(e) => a.sfx.music_event(*e),
            SfxCue::StartMusic(t) => {
                a.override_music = true;
                a.sfx.start_music(t.clone());
            }
            SfxCue::ResumeCourseMusic => {
                a.override_music = false;
                let track = course_track(&sim);
                a.sfx.start_music(track);
            }
            SfxCue::StopMusic => {
                a.override_music = true;
                a.sfx.stop_music();
            }
            SfxCue::SetMuted(m) => a.sfx.set_muted(*m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::engine::Sm64Engine;
    use super::processor::{Cmd, Processor};
    use super::sfx::course_music;

    /// The interpreter needs more stack than a test thread has.
    fn big_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new().stack_size(64 << 20).spawn(f).unwrap().join().unwrap();
    }

    fn engine() -> Option<Processor> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/audio/music/sm64-us.wasm");
        let bytes = std::fs::read(path).ok()?;
        Some(Processor::new(Sm64Engine::new(&bytes).expect("engine")))
    }

    fn energy(frames: &[(f32, f32)]) -> f32 {
        frames.iter().map(|(l, r)| l.abs() + r.abs()).sum()
    }

    #[test]
    fn engine_renders_course_music_and_effects() {
        big_stack(|| {
        let Some(mut p) = engine() else { return };
        let t = course_music("bob", 1);
        // Silent before a track starts.
        assert_eq!(energy(&p.render_offline(&[], 100)), 0.0);
        let music = p.render_offline(
            &[Cmd::Location([t.level as f64, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]), Cmd::Track { sequence: t.sequence, preset: t.preset }],
            32000 * 3,
        );
        assert!(energy(&music) > 1.0, "bob music is silent");
        assert!(music.iter().all(|(l, r)| l.abs() <= 1.0 && r.abs() <= 1.0));
        });
    }

    #[test]
    fn sound_effects_are_audible_and_panned() {
        big_stack(|| {
        let Some(mut p) = engine() else { return };
        let t = course_music("bob", 1);
        p.render_offline(&[Cmd::Track { sequence: t.sequence, preset: t.preset }], 32000 / 2);
        let sounds: std::collections::HashMap<String, f64> = serde_json::from_str(include_str!("../../../assets/audio/music/sounds.json")).unwrap();
        let coin = sounds["GENERAL_COIN"];
        // Listener-relative: far left of the camera vs far right.
        let mut left = 0.0;
        let mut right = 0.0;
        for (x, acc) in [(-600.0, &mut left), (600.0, &mut right)] {
            p.render_offline(&[Cmd::Sound([coin, 1.0, 5.0, x, 0.0, -300.0])], 0);
            let out = p.render_offline(&[], 32000 / 4);
            let (l, r): (f32, f32) = out.iter().fold((0.0, 0.0), |a, f| (a.0 + f.0.abs(), a.1 + f.1.abs()));
            *acc = (l - r) as f64;
            p.render_offline(&[], 32000);
        }
        assert!(left > right, "sound source on the left should be louder on the left channel ({left} vs {right})");
        });
    }
}
