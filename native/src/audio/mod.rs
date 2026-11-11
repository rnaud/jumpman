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
#[cfg(test)]
mod audio_tests;
#[cfg(test)]
mod cue_tests;
#[cfg(test)]
mod sound_table_tests;
mod ambient;
mod engine;
mod footsteps;
mod mario_sounds;
pub mod bk_events;
pub mod bk_sfx;
pub mod oot_music;
pub mod oot_sfx;
mod processor;
mod sfx;
mod terrain;
pub(crate) mod voices;

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
#[derive(Message, Clone, Debug)]
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
    /// Small one-shots used by the experiments: coin, coin_bounce, bonk, star_sound, teleport, triple_jump, footstep, break_box,
    /// cannon_fire, explosion, water_cannon, blast_wall, cap_switch, move_menu, select_mission, one_up, star, secret, red_coin, hurt,
    /// dive, kick, flutter, wall_kick, ground_pound_drop, land, splash, stomp.
    Effect { kind: &'static str, pos: Option<[f64; 3]> },
    /// Any SM64 sound by its `SOUND_*` name without the prefix, in any case (`sounds.json` holds all 441 of
    /// `include/sounds.h`): `Sound { name: "OBJ_THWOMP", pos: Some(world_pos) }`. `pos: None` plays it at Mario.
    /// `terrain_*` / `action_terrain_*` names pick the floor's variant, `MARIO_*` voices only play for Mario's set.
    Sound { name: &'static str, pos: Option<[f64; 3]> },
    /// Like [`SfxCue::Sound`] for a sound the decomp requests every tick (fuse, wind, flames): `key` identifies the
    /// emitter so the engine keeps one entry playing instead of restarting it.
    SoundLoop { name: &'static str, pos: [f64; 3], key: u32 },
}

/// Handle to the sound player for systems that need more than [`SfxCue`].
#[derive(Resource)]
pub struct GameAudio {
    pub sfx: Sfx,
    footsteps: Footsteps,
    /// Coin pickups not yet heard (one tick-pair each, `hudCoins` counting up in the TS).
    pending_coins: u32,
    hud_tick: u32,
    hud_tick_actions: u32,
    last_action: u32,
    course: (String, i32),
    override_music: bool,
}

impl GameAudio {
    /// The global mute switch (M); synthesised effects outside the SM64 engine follow it.
    pub fn is_muted(&self) -> bool {
        self.sfx.muted
    }
}

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Sm64Audio>().add_message::<SfxCue>().add_systems(Startup, setup.in_set(crate::progress::PersistentSpawn)).add_systems(
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
    let muted = match std::env::var("JUMPMAN_MUTE") {
        Ok(v) => !v.is_empty() && v != "0",
        Err(_) => load_muted(),
    };
    voices::set_root(root.0.clone());
    let gain = MasterGain::new(0.0);
    let mut sfx = Sfx::new(tx, gain.clone(), muted);
    sfx.limit_per_tick(true);
    if std::env::var("JUMPMAN_NO_AUDIO").is_err() {
        let blocks = processor::spawn_render_thread(root.0.join("audio/music/sm64-us.wasm"), rx);
        commands.spawn(AudioPlayer(assets.add(Sm64Audio { blocks: Arc::new(Mutex::new(blocks)), gain })));
    }
    commands.insert_resource(GameAudio {
        sfx,
        footsteps: Footsteps::default(),
        pending_coins: 0,
        hud_tick: 0,
        hud_tick_actions: 0,
        last_action: 0,
        course: (String::new(), 0),
        override_music: false,
    });
}

fn toggle_mute(keys: Res<ButtonInput<KeyCode>>, mut audio: ResMut<GameAudio>) {
    if keys.just_pressed(KeyCode::KeyM) {
        let m = !audio.sfx.muted;
        audio.sfx.set_muted(m);
        store_muted(m);
    }
}

/// The mute switch survives restarts (the web build kept it in `localStorage`): a `muted` flag
/// file next to the save slots. `JUMPMAN_MUTE` overrides it for a run.
fn muted_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("muted")
}

fn load_muted() -> bool {
    std::fs::read_to_string(muted_path()).is_ok_and(|s| s.trim() == "1")
}

fn store_muted(muted: bool) {
    let path = muted_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, if muted { "1" } else { "0" });
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
fn listener(sim: Res<GameSim>, root: Res<AssetRoot>, mut audio: ResMut<GameAudio>, camera: Query<&GlobalTransform, With<MainCamera>>, character: Option<Res<crate::characters::Character>>) {
    let Ok(cam) = camera.single() else { return };
    audio.sfx.set_character(character.map_or("sm64", |c| c.sound_id()));
    let m = &sim.mario;
    let key = (sim.course.request.id.clone(), sim.course.request.area);
    if audio.course != key {
        // The exported Mario Kart and Banjo-Kazooie courses play their own music (`mk64_music`, `bk_music`).
        // The exported OoT scenes play the ROM's own sequence (`zelda::music`).
        let mk64 = crate::mk64_music::owns_course(&key.0)
            || crate::bk_music::owns_course(&key.0)
            || crate::zelda_enemies::load_placements(&root.0, &key.0, key.1).is_some_and(|p| p.environment.music.seq != 255);
        audio.course = key;
        audio.override_music = mk64;
        if !mk64 {
            let track = course_track(&sim);
            audio.sfx.start_music(track);
        }
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

/// Play an ocarina-like tone (`freq` Hz for `secs`) through the voice mixer.
/// Play a ROM-extracted voice clip (`voices/<set>/<NAME>`, `variants` files) through the mixer.
pub fn play_clip(name: &str, variants: f64) {
    voices::play(name, variants);
}

pub fn play_tone(freq: f64, secs: f64) {
    let rate = 22050u32;
    let n = (secs * rate as f64) as usize;
    let pcm = (0..n)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let env = (t / 0.03).min(1.0) * (1.0 - (t / secs)).max(0.0).powf(0.5);
            let w = (std::f64::consts::TAU * freq * t).sin() + 0.25 * (std::f64::consts::TAU * freq * 2.0 * t).sin() + 0.08 * (std::f64::consts::TAU * freq * 3.0 * t).sin();
            (w * env * 0.35) as f32
        })
        .collect();
    voices::play_pcm(pcm, rate);
}

/// The `Game.processEvents` / `processObjectEvents` sound dispatch for one sim event.
pub(crate) fn dispatch_sim_event(sfx: &mut Sfx, e: &SimEvent, has_pound_land: bool, pos: &dyn Fn([f64; 3]) -> Pos, pending_coins: &mut u32) {
    match e {
        SimEvent::Mario(ev) => match ev {
            MarioEvent::Jump { action, .. } => match *action {
                ACT_DOUBLE_JUMP => sfx.double_jump(),
                ACT_TRIPLE_JUMP => sfx.triple_jump(),
                ACT_BACKFLIP => sfx.backflip(),
                ACT_JUMP_KICK => sfx.kick(),
                _ => sfx.jump(),
            },
            MarioEvent::Triplejump => sfx.triple_jump(),
            MarioEvent::Land { .. } => {
                if !has_pound_land {
                    sfx.land();
                }
            }
            MarioEvent::WingFlight { pull_up } => sfx.wing_flight(*pull_up),
            MarioEvent::FlightBoundary => sfx.flight_boundary(),
            MarioEvent::FlightBurn => sfx.flight_burn(),
            MarioEvent::Bonk { .. } => sfx.bonk(),
            MarioEvent::Wallkick => sfx.wall_kick(),
            MarioEvent::Groundpound => sfx.ground_pound_start(),
            MarioEvent::CannonMove { phase } => sfx.cannon_move(*phase),
            MarioEvent::CannonFire => sfx.cannon_fire(),
            MarioEvent::BowserGrab => sfx.bowser_grab(),
            MarioEvent::BowserSpin => sfx.bowser_spin(),
            MarioEvent::BowserThrow { fast } => sfx.bowser_throw(*fast),
            MarioEvent::WaterPlunge => sfx.splash(),
            MarioEvent::SwimStroke { fast } => sfx.swim(*fast),
            MarioEvent::FlutterKick => sfx.flutter(),
            MarioEvent::GroundpoundDrop => sfx.ground_pound_drop(),
            MarioEvent::GroundpoundLand => sfx.ground_pound(),
            MarioEvent::Longjump => sfx.long_jump(),
            MarioEvent::Dive => sfx.dive(),
            MarioEvent::Hurt { .. } => sfx.hurt(),
            MarioEvent::Death => sfx.death(),
            MarioEvent::DeathLanding => sfx.death_landing(),
            MarioEvent::MamaMia => sfx.mama_mia(),
            MarioEvent::Step { .. } | MarioEvent::CastleEnding | MarioEvent::EndingCredits | MarioEvent::Respawn => {}
        },
        SimEvent::Object(ev) => match ev {
            ObjectEvent::WaterBomb { pos: p, phase } => sfx.water_bomb(pos(*p), phase),
            ObjectEvent::WaterCannon { pos: p } => sfx.water_cannon(pos(*p)),
            ObjectEvent::Explosion { pos: p, mine, .. } => sfx.explosion(pos(*p), *mine),
            ObjectEvent::Break { pos: p, wall } => {
                if let Some(w) = wall {
                    sfx.blast_wall(pos(*p));
                    if *w == "right" {
                        sfx.music_event(MusicEvent::Puzzle);
                    }
                } else {
                    sfx.break_box(pos(*p));
                }
            }
            ObjectEvent::Hurt { .. } => sfx.hurt(),
            ObjectEvent::Chest { number } => sfx.chest_answer(*number != 0.0),
            ObjectEvent::RacerSound { pos: p, cue } => sfx.racer_sound(pos(*p), cue),
            ObjectEvent::PenguinSound { pos: p, diving } => sfx.penguin(pos(*p), *diving),
            ObjectEvent::Snowman { pos: p, cue } => {
                if *cue != "reward" {
                    sfx.snowman(pos(*p), cue);
                    if *cue == "assembled" {
                        sfx.music_event(MusicEvent::Puzzle);
                    }
                }
            }
            ObjectEvent::ChestReward => sfx.music_event(MusicEvent::Puzzle),
            ObjectEvent::Bowser { .. } | ObjectEvent::BigBully { .. } | ObjectEvent::WhompKing { .. } | ObjectEvent::BigBoo { .. } => {
                sfx.music_event(MusicEvent::Spawn)
            }
            ObjectEvent::OneUp { .. } => sfx.one_up(),
            ObjectEvent::KingLeave => sfx.music_event(MusicEvent::KingLeave),
            ObjectEvent::KingDialog { id, .. } => {
                if *id == 17.0 {
                    sfx.music_event(MusicEvent::KingIntro);
                }
            }
            ObjectEvent::Race { state } => match *state {
                "start" => sfx.music_event(MusicEvent::RaceStart),
                "won" => {
                    sfx.music_event(MusicEvent::RaceEnd);
                    sfx.star();
                }
                _ => {
                    sfx.music_event(MusicEvent::RaceEnd);
                    sfx.bonk();
                }
            },
            ObjectEvent::BowlingBall { pos: p, cue } => {
                if *cue != "end" {
                    sfx.ball_bounce(pos(*p));
                }
            }
            ObjectEvent::KoopaSound { pos: p, cue } => sfx.koopa_sound(pos(*p), cue),
            ObjectEvent::CoinBounce { pos: p } => sfx.coin_bounce(pos(*p)),
            // `play_puzzle_jingle` (the Ukiki cage) is music, not a sound effect.
            ObjectEvent::Sound { name: "PUZZLE_JINGLE", .. } => sfx.music_event(MusicEvent::Puzzle),
            ObjectEvent::Sound { pos: p, name } => sfx.object_sound_named(name, pos(*p)),
            ObjectEvent::SoundLoop { pos: p, name, key } => sfx.object_sound_keyed(name, pos(*p), *key),
            // Loot coins pop out of defeated enemies with `SOUND_GENERAL_COIN_SPURT` (moving_yellow_coin init).
            ObjectEvent::Coin { pos: p, loot: true, .. } => sfx.object_sound_named("GENERAL_COIN_SPURT", pos(*p)),
            _ => {}
        },
        SimEvent::CoinCollected { value } => *pending_coins += (*value).max(0) as u32,
    }
}

/// The collect sound of the currency the player counts (the collectible theme): the OoT rupee sound for Link, the BK note sound
/// for Banjo. Returns `false` for Mario, whose SM64 coin sound the caller plays itself.
pub fn themed_collect_sound(theme: jumpman_sim::collectible_theme::Theme) -> bool {
    use jumpman_sim::collectible_theme::Theme;
    match theme {
        Theme::Coin => false,
        Theme::Rupee => {
            oot_sfx::play(COLLECT_RUPEE);
            true
        }
        Theme::Note => {
            bk_events::play_event(COLLECT_NOTE);
            true
        }
    }
}

/// `NA_SE_SY_GET_RUPY`, the sound of every rupee pickup in OoT.
pub const COLLECT_RUPEE: &str = "SY_GET_RUPY";
/// The BK event key of a note pickup (`C7_SHWOOP`).
pub const COLLECT_NOTE: &str = "collect.note";

/// A piranha plant is asleep within lullaby range (`piranha_plant_act_sleeping`).
fn piranha_lullaby(sim: &GameSim) -> bool {
    use jumpman_sim::object::ObjectBehaviorKind;
    sim.mario.objects.objects.iter().any(|o| {
        o.active && o.behavior == ObjectBehaviorKind::PiranhaPlant && o.action == 1 && (400.0..1000.0).contains(&o.distance_to_mario)
    })
}

fn react_to_sim(
    mut events: MessageReader<SimEventMsg>,
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
        dispatch_sim_event(&mut a.sfx, e, has_pound_land, &pos, &mut a.pending_coins);
    }
    // Action sounds without a sim event (punches, ledges, slides...) and the level's ambient emitters.
    a.hud_tick_actions = a.hud_tick_actions.wrapping_add(1);
    if m.action != before {
        for name in mario_sounds::entry(m.action, m.action_arg).iter().chain(mario_sounds::transition(before, m.action)) {
            a.sfx.named(name, None);
        }
    }
    for l in mario_sounds::each_tick(m.action) {
        if a.hud_tick_actions % l.period == 0 {
            a.sfx.named(l.name, None);
        }
    }
    for (i, name, at) in ambient::audible(&sim.course.request.id, m.pos) {
        a.sfx.object_sound_keyed(name, pos(at), i as u32);
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
        if !themed_collect_sound(sim.theme()) {
            a.sfx.coin(swimming);
        }
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
    } else if piranha_lullaby(&sim) {
        // `play_secondary_music(SEQ_EVENT_PIRANHA_PLANT)` while a sleeping piranha plant is 400..1000 units away.
        0x0b
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

fn apply_cues(mut cues: MessageReader<SfxCue>, mut audio: ResMut<GameAudio>, sim: Res<GameSim>, camera: Query<&GlobalTransform, With<MainCamera>>) {
    let Ok(cam) = camera.single() else { return };
    let a = &mut *audio;
    for cue in cues.read() {
        let pos = |p: [f64; 3]| camera_space(cam, p);
        if matches!(cue, SfxCue::Coin { .. }) && themed_collect_sound(sim.theme()) {
            continue;
        }
        apply_cue(&mut a.sfx, cue, &pos, &mut a.override_music, &|| course_track(&sim));
    }
}

/// `Sfx` calls for one [`SfxCue`] (the progression / object glue of `Game.ts`).
pub(crate) fn apply_cue(sfx: &mut Sfx, cue: &SfxCue, pos: &dyn Fn([f64; 3]) -> Pos, override_music: &mut bool, course: &dyn Fn() -> MusicTrack) {
    match cue {
        SfxCue::Coin { underwater } => sfx.coin(*underwater),
        SfxCue::Star => sfx.star(),
        SfxCue::StarSound => sfx.star_sound(),
        SfxCue::OneUp => sfx.one_up(),
        SfxCue::Teleport => sfx.teleport(),
        SfxCue::CapSwitch => sfx.cap_switch(),
        SfxCue::DeathWarp => sfx.death_warp(),
        SfxCue::Bonk => sfx.bonk(),
        SfxCue::StompGoomba => sfx.stomp_goomba(),
        SfxCue::SignMessage(c) => sfx.sign_message(*c),
        SfxCue::CourseCoin { count, record } => sfx.course_coin(*count, *record),
        SfxCue::Secret(n) => sfx.secret(*n),
        SfxCue::RedCoin(n) => sfx.red_coin(*n),
        SfxCue::SpawnedStar { pos: p, appearing } => sfx.spawned_star(pos(*p), *appearing),
        SfxCue::CoordinateStar { pos: p, landed } => sfx.coordinate_star(pos(*p), *landed),
        SfxCue::CannonDoor { pos: p } => sfx.cannon_door(pos(*p)),
        SfxCue::Lift { pos: p, cue } => sfx.lift(pos(*p), cue),
        SfxCue::MoveMenu => sfx.move_menu(),
        SfxCue::SelectMission => sfx.select_mission(),
        SfxCue::Peach(v) => sfx.peach_voice(*v),
        SfxCue::Music(e) => sfx.music_event(*e),
        SfxCue::StartMusic(t) => {
            *override_music = true;
            sfx.start_music(t.clone());
        }
        SfxCue::ResumeCourseMusic => {
            *override_music = false;
            let track = course();
            sfx.start_music(track);
        }
        SfxCue::StopMusic => {
            *override_music = true;
            sfx.stop_music();
        }
        SfxCue::Effect { kind, pos: at } => match *kind {
            "coin" => sfx.coin(false),
            "coin_bounce" => sfx.coin_bounce(at.map_or(Default::default(), pos)),
            "bonk" => sfx.bonk(),
            "star_sound" => sfx.star_sound(),
            "teleport" => sfx.teleport(),
            "triple_jump" => sfx.triple_jump(),
            "footstep" => sfx.footstep(false),
            "break_box" => sfx.break_box(at.map_or(Default::default(), pos)),
            "cannon_fire" => sfx.cannon_fire(),
            "explosion" => sfx.explosion(at.map_or(Default::default(), pos), false),
            "water_cannon" => sfx.water_cannon(at.map_or(Default::default(), pos)),
            "blast_wall" => sfx.blast_wall(at.map_or(Default::default(), pos)),
            "cap_switch" => sfx.cap_switch(),
            "move_menu" => sfx.move_menu(),
            "select_mission" => sfx.select_mission(),
            "one_up" => sfx.one_up(),
            "star" => sfx.star(),
            "secret" => sfx.secret(0),
            "red_coin" => sfx.red_coin(0),
            "hurt" => sfx.hurt(),
            "dive" => sfx.dive(),
            "kick" => sfx.kick(),
            "flutter" => sfx.flutter(),
            "wall_kick" => sfx.wall_kick(),
            "ground_pound_drop" => sfx.ground_pound_drop(),
            "land" => sfx.land(),
            "splash" => sfx.splash(),
            "stomp" => sfx.stomp_goomba(),
            // Gem Clash cues: three rising gem pings, the transformation, the round bell.
            "gem1" => sfx.red_coin(0),
            "gem2" => sfx.red_coin(3),
            "gem3" => sfx.red_coin(7),
            "power_up" => sfx.star(),
            "round_end" => sfx.one_up(),
            _ => {}
        },
        SfxCue::Sound { name, pos: at } => sfx.named(name, at.map(pos)),
        SfxCue::SoundLoop { name, pos: at, key } => sfx.object_sound_keyed(&name.to_ascii_uppercase(), pos(*at), *key),
        SfxCue::SetMuted(m) => {
            sfx.set_muted(*m);
            store_muted(*m);
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
