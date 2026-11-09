//! The kart experiment's Mario Kart 64 sound effects: kart and race state in, calls to the decomp's own SFX system
//! out (`crate::mk64_music::sfx`). The engine note, tyre squeal and surface sounds are not synthesised any more:
//! the library starts and retunes them itself from the kart state mirrored into `gPlayers[]` (external.c
//! `func_800C8CCC`, the same code the cartridge runs), so this file only
//!
//! * maps [`SoundFrame`] / the CPU karts to the fields that logic reads ([`player_state`], [`cpu_state`]);
//! * maps the cue API of `kart_sound` ([`Cue`], what `kart_audio::play` takes) to the sound ids of the game call
//!   sites ([`calls_for_cue`], each documented with the decomp function that plays it);
//! * offers [`Sfx`] / [`Menu`] for sounds that have no `Cue` (star, thunder, falling rock, the menu clicks).
//!
//! The synthesised stand-ins of `kart_audio` keep playing until the real library reports ready
//! (`mk64_music::sfx_ready`), i.e. when the MK64 audio is missing (BYOR without the ROM) nothing changes.
use super::kart_sound::{Cue, SoundFrame};
use crate::mk64_music::sfx::{self, flags, id, voice, PlayerIn, SfxCmd};
use std::sync::Mutex;

/// One thing to play for a cue, in the terms of the game's call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// `func_800C9060(player, id)`: a sound of the kart.
    Player(u32),
    /// `func_800C90F4(player, characterId * 0x10 + 0x29008000 + sub)`: the character's voice.
    Voice(u32),
    /// `func_800C9250`: the boost voice, take 1 or 2 at random.
    VoiceRandom,
    /// `play_sound2(id)`: not tied to a kart.
    Global(u32),
}

/// The MK64 sounds of a `kart_sound` cue, in the order the game starts them. Cues without a sound in the
/// original return nothing (the item roulette ticks silently there; the intermediate drift charge has no sound).
pub fn calls_for_cue(cue: Cue) -> Vec<Call> {
    use Call::*;
    match cue {
        // kart_hop (player_controller.c func_80037CFC)
        Cue::Hop => vec![Player(id::HOP)],
        // The squeal of the drift comes from the tyre logic (DRIFTING_EFFECT -> func_800C70A8 / func_800C683C).
        Cue::DriftStart => vec![],
        // update_drift_state_counter plays one sound, when the counter reaches 100 (the full charge).
        Cue::Charge1 => vec![],
        Cue::Charge2 => vec![Player(id::DRIFT_READY)],
        // func_8002A79C: the mini-turbo fires with a voice only.
        Cue::Turbo => vec![VoiceRandom],
        // func_802903D8: kart against kart; the wall bump uses the same.
        Cue::Bump => vec![Player(id::BUMP)],
        // func_8007C7B4 (box broken) and func_8007ABFC (item window starts).
        Cue::Box => vec![Player(id::ITEM_BOX), Player(id::ITEM_ROULETTE)],
        Cue::Roulette | Cue::ItemGet => vec![],
        // func_8002A704 (MUSHROOM_EFFECT): voice, then the boost sound.
        Cue::Mushroom => vec![Voice(voice::BOOST), Player(id::BOOST)],
        // update_actor_banana (Z released).
        Cue::BananaDrop => vec![Player(id::BANANA_DROP)],
        // add_spinout_effect.
        Cue::BananaHit => vec![Voice(voice::SPINOUT)],
        // update_actor_green_shell (the throw).
        Cue::ShellFire => vec![Player(id::SHELL_FIRE), Voice(voice::THROW)],
        // func_8008C528 (HIT_BY_GREEN_SHELL_EFFECT).
        Cue::ShellHit => vec![Voice(voice::HIT), Player(id::EXPLOSION)],
        // update_object_lakitu_countdown: SOUND_ACTION_COUNTDOWN_LIGHT for the two red/orange lights (this mode's
        // 3-2-1 beeps all three), SOUND_ACTION_GREEN_LIGHT on the green.
        Cue::Count => vec![Global(id::COUNTDOWN_LIGHT)],
        Cue::Go => vec![Global(id::GREEN_LIGHT)],
        // func_8005CB60: the lap counter (the final lap's jingle is the music cue).
        Cue::Lap | Cue::FinalLap => vec![Player(id::LAP)],
        // func_800C76C0 plays the finish sound by itself when the music cue `Finish` reaches func_800CA118.
        Cue::Finish => vec![],
        // update_object_lakitu_fishing2.
        Cue::Lakitu => vec![Player(id::LAKITU_APPEAR)],
    }
}

/// Sounds that are not a `kart_sound::Cue`: other agents' items and hazards. All play on the player's kart or as
/// a global sound; use [`play_at`] for things in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    /// Hit by something hard (thwomp squish): `apply_hit_effect`.
    Hit,
    /// Falling rock / bomb explosion: `SOUND_ACTION_EXPLOSION`.
    Explosion,
    /// Spun out voice (`add_spinout_effect`).
    SpinoutVoice,
    /// "Ow" voice for tumbles, lightning, squish (`trigger_vertical_tumble`).
    HitVoice,
    /// Recovering voice after a spin.
    RecoverVoice,
    /// Put back on the track by Lakitu: the drop sound and voice (`update_object_lakitu_fishing2`, `func_8008F494`).
    LakituDrop,
    /// Item roulette says "nothing" (`func_8007B040`).
    NoItem,
    /// Fell into water (`func_8002C4F8`).
    Splash,
    /// Boost pad / ramp (`trigger_*_ramp_boost`).
    BoostPad,
    /// Kart bounced off a tree or sign with a star (`func_800C98B8` at actors.c).
    StarBump,
    /// Battle balloon popped (`pop_player_balloon`).
    BalloonPop,
    /// Fastest-lap cheer (`func_8005CB60`).
    Cheer,
    /// Engines rev when the light turns green (`func_8028FCBC`): both revs.
    RevEngines,
    /// Cursor, select and the other menu sounds (see [`Menu`]).
    Menu(Menu),
}

/// The menu sounds of the kart front end (`menus.c`: `play_sound2(SOUND_MENU_*)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    /// `SOUND_MENU_CURSOR_MOVE`.
    Cursor,
    /// `SOUND_MENU_SELECT`.
    Select,
    /// `SOUND_MENU_GO_BACK`.
    Back,
    /// `SOUND_MENU_OK_CLICKED`.
    Ok,
    /// `SOUND_MENU_FILE_NOT_FOUND`: the "can't" buzz.
    Error,
    /// `SOUND_INTRO_ENTER_MENU`.
    Enter,
    /// `SOUND_ACTION_COUNT_SCORE`: the coin tick of the results.
    ScoreTick,
    /// `SOUND_ACTION_PING`.
    Ping,
}

impl Menu {
    pub fn word(self) -> u32 {
        match self {
            Menu::Cursor => id::MENU_CURSOR,
            Menu::Select => id::MENU_SELECT,
            Menu::Back => id::MENU_BACK,
            Menu::Ok => id::MENU_OK,
            Menu::Error => id::MENU_ERROR,
            Menu::Enter => id::MENU_ENTER,
            Menu::ScoreTick => id::SCORE_COUNT,
            Menu::Ping => id::PING,
        }
    }
}

pub fn calls_for_sfx(s: Sfx) -> Vec<Call> {
    use Call::*;
    match s {
        Sfx::Hit => vec![Player(id::HIT)],
        Sfx::Explosion => vec![Player(id::EXPLOSION)],
        Sfx::SpinoutVoice => vec![Voice(voice::SPINOUT)],
        Sfx::HitVoice => vec![Voice(voice::HIT)],
        Sfx::RecoverVoice => vec![Voice(voice::RECOVER)],
        Sfx::LakituDrop => vec![Player(id::LAKITU_DROP), Voice(voice::RESCUED)],
        Sfx::NoItem => vec![Player(id::ITEM_NONE)],
        Sfx::Splash => vec![Player(id::SPLASH)],
        Sfx::BoostPad => vec![Voice(voice::BOOST), Player(id::BOOST)],
        Sfx::StarBump => vec![Player(id::STAR_BUMP), Voice(voice::CHEER)],
        Sfx::BalloonPop => vec![Player(id::BALLOON)],
        Sfx::Cheer => vec![Voice(voice::CHEER)],
        Sfx::RevEngines => vec![Global(id::REV_ENGINE), Global(id::REV_ENGINE_2)],
        Sfx::Menu(m) => vec![Global(m.word())],
    }
}

fn send(call: Call, slot: usize) {
    use crate::mk64_music;
    mk64_music::sfx(match call {
        Call::Player(bits) => SfxCmd::PlayerSound { slot, bits },
        Call::Voice(sub) => SfxCmd::Voice { slot, sub },
        Call::VoiceRandom => SfxCmd::VoiceRandom { slot },
        Call::Global(bits) => SfxCmd::Global { bits },
    });
}

/// Play a `kart_sound` cue as the player's kart (slot 0). Called by `kart_audio::play`, so everything that already
/// emits cues gets the MK64 sound.
pub fn cue(c: Cue) {
    for call in calls_for_cue(c) {
        send(call, 0);
    }
}

/// Play one of the extra sounds as the player's kart.
pub fn play(s: Sfx) {
    for call in calls_for_sfx(s) {
        send(call, 0);
    }
}

/// A menu sound; works without a race session (front ends).
pub fn menu(m: Menu) {
    send(Call::Global(m.word()), 0);
}

/// Something in the world makes a one-shot sound (`func_800C98B8`: volume, pan and doppler from the distance to
/// the camera). `place` is a slot 0..32 the caller owns for as long as the thing moves; positions in decomp units.
pub fn play_at(place: usize, bits: u32, pos: [f64; 3], vel: [f64; 3]) {
    use crate::mk64_music;
    let f = |v: [f64; 3]| [v[0] as f32, v[1] as f32, v[2] as f32];
    mk64_music::sfx(SfxCmd::Place { slot: place, pos: f(pos), vel: f(vel) });
    mk64_music::sfx(SfxCmd::PlaceSound { slot: place, bits });
}

/// What the Rust kart physics knows that the sound logic reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriveIn {
    pub frame: SoundFrame,
    /// `currentSpeed` and `speed` of the kart (decomp units).
    pub current_speed: f64,
    pub speed: f64,
    /// `unk_0C0` (s16 units) and `unk_20C`.
    pub steer: i32,
    pub unk_20c: f64,
    /// Position and per-kart-update velocity in decomp units.
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub character: u32,
}

fn v3(v: [f64; 3]) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

/// The player's kart as `Player` fields for external.c.
pub fn player_state(d: &DriveIn) -> PlayerIn {
    let f = &d.frame;
    let mut effects = 0;
    if f.hop {
        effects |= flags::HOP;
    }
    if f.drifting {
        effects |= flags::DRIFTING;
    }
    if f.midair {
        effects |= flags::MIDAIR;
    }
    if f.mini_turbo {
        effects |= flags::MINI_TURBO;
    }
    if f.boosting {
        effects |= flags::MUSHROOM;
    }
    if f.spinning {
        effects |= flags::BANANA_SPINOUT;
    }
    if f.reversing {
        effects |= flags::BRAKING;
    }
    let surface = if f.grass { flags::GRASS } else { flags::ASPHALT };
    PlayerIn {
        character: d.character,
        human: true,
        kart_props: if f.accel && f.driving { flags::THROTTLE } else { 0 },
        effects,
        oob: 0,
        speed: d.speed as f32,
        current_speed: d.current_speed as f32,
        engine_speed: sfx::engine_speed(d.current_speed, f.midair || f.spinning) as f32,
        unk_0c0: d.steer,
        unk_20c: d.unk_20c as f32,
        surface_left: surface,
        surface_right: surface,
        pos: v3(d.pos),
        vel: v3(d.vel),
    }
}

/// A CPU kart (`func_800C9A88`'s engine) from what the race view knows of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuIn {
    pub character: u32,
    pub current_speed: f64,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub drifting: bool,
    pub midair: bool,
    pub spun: bool,
    pub boosting: bool,
}

pub fn cpu_state(c: &CpuIn) -> PlayerIn {
    let mut effects = 0;
    if c.drifting {
        effects |= flags::DRIFTING;
    }
    if c.midair {
        effects |= flags::MIDAIR;
    }
    if c.spun {
        effects |= flags::BANANA_SPINOUT;
    }
    if c.boosting {
        effects |= flags::MUSHROOM;
    }
    PlayerIn {
        character: c.character,
        human: false,
        kart_props: flags::THROTTLE,
        effects,
        speed: (c.current_speed / 30.0) as f32,
        current_speed: c.current_speed as f32,
        engine_speed: sfx::engine_speed(c.current_speed, c.midair || c.spun) as f32,
        pos: v3(c.pos),
        vel: v3(c.vel),
        ..PlayerIn::default()
    }
}

/// Process-wide state of the session (like `kart_audio`, so `Host` and `XpParams` need no new field).
struct Session {
    active: bool,
    /// The player's engine is running (the kart is being driven).
    human_engine: bool,
    /// The CPU karts' engines are running (the light turned green).
    cpu_engines: bool,
    cpus: usize,
    /// CPU positions of the previous frame, for their velocity (doppler).
    prev: Vec<Option<[f64; 3]>>,
    vel: Vec<[f64; 3]>,
    /// Karts out of the game (`KartRace::benched`: a solo Time Trial's CPUs, a battle's eliminated karts): bit i =
    /// racer i. Their engines are off.
    benched: u32,
    /// Benched karts whose engine was already switched off.
    silenced: u32,
}

impl Session {
    const fn new() -> Self {
        Self { active: false, human_engine: false, cpu_engines: false, cpus: 0, prev: Vec::new(), vel: Vec::new(), benched: 0, silenced: 0 }
    }
}

/// The karts out of the game (bit i = racer i): their engines stay off. Call after every change of the set.
pub fn set_benched(mask: u32) {
    if let Ok(mut s) = SESSION.lock() {
        if s.benched != mask {
            s.benched = mask;
            s.silenced &= mask;
        }
    }
}

static SESSION: Mutex<Session> = Mutex::new(Session::new());

/// The kart mode opened on this track: start the sound-effects session.
pub fn session_start(dir: &str) {
    if let Ok(mut s) = SESSION.lock() {
        *s = Session { active: true, ..Session::new() };
    }
    crate::mk64_music::sfx(SfxCmd::Begin { course: sfx::course_id(dir), humans: 1 });
}

/// The kart mode closed.
pub fn session_end() {
    if let Ok(mut s) = SESSION.lock() {
        *s = Session::new();
    }
    crate::mk64_music::sfx(SfxCmd::End);
}

/// A race (or the practice lap) starts: reset the game-side sound state, and rev the engines on the grid
/// (`func_8028FCBC`).
pub fn race_start(cpus: usize) {
    if let Ok(mut s) = SESSION.lock() {
        if !s.active {
            return;
        }
        s.cpus = cpus;
        s.human_engine = false;
        s.cpu_engines = false;
        s.prev.clear();
        s.vel.clear();
        s.benched = 0;
        s.silenced = 0;
    }
    let course = crate::mk64_music::kart_course();
    crate::mk64_music::sfx(SfxCmd::Begin { course: sfx::course_id(&course), humans: 1 });
    play(Sfx::RevEngines);
}

/// The light turned green: the CPU karts' engines start (the player's runs whenever the kart is driven).
pub fn engines_on() {
    let (cpus, benched) = match SESSION.lock() {
        Ok(mut s) if s.active => {
            s.cpu_engines = true;
            (s.cpus, s.benched)
        }
        _ => return,
    };
    for i in (1..=cpus).filter(|i| benched & (1 << i) == 0) {
        crate::mk64_music::sfx(SfxCmd::Engine { slot: i, on: true });
    }
}

/// Race events that start sounds of their own.
pub fn on_events(events: &[super::kart_race::Event]) {
    for e in events {
        if matches!(e, super::kart_race::Event::Go) {
            engines_on();
        }
    }
}

/// The kart is not being driven (the player walks): no engine.
pub fn parked() {
    if let Ok(mut s) = SESSION.lock() {
        if s.active && s.human_engine {
            s.human_engine = false;
            crate::mk64_music::sfx(SfxCmd::Engine { slot: 0, on: false });
        }
    }
}

/// The CPU karts of the race view (`kart_race_view::cpu_fx_frames`) as the sound system takes them. `fx.pos` is in
/// world units (decomp units times 8).
pub fn cpu_from_fx(fx: &super::kart_fx::FxFrame) -> CpuIn {
    let p = &fx.puff;
    CpuIn {
        character: p.character as u32,
        current_speed: p.current_speed,
        pos: [fx.pos[0] / 8., fx.pos[1] / 8., fx.pos[2] / 8.],
        vel: [0.; 3],
        drifting: p.drifting,
        midair: fx.midair,
        spun: fx.spun,
        boosting: p.mushroom,
    }
}

/// Every frame of the experiment: the player's kart, the listener and the CPU karts. `dt` is the frame time in
/// seconds (CPU velocities come from their movement between frames).
pub fn drive(dt: f64, d: &DriveIn, camera_eye: [f64; 3], camera_look: [f64; 3], cpus: &[CpuIn]) {
    let Ok(mut s) = SESSION.lock() else { return };
    if !s.active {
        return;
    }
    use crate::mk64_music;
    if d.frame.driving != s.human_engine {
        s.human_engine = d.frame.driving;
        mk64_music::sfx(SfxCmd::Engine { slot: 0, on: d.frame.driving });
    }
    // MK64 camera yaw: rot[1] = atan2s(lookAt.x - pos.x, lookAt.z - pos.z).
    let yaw = ((camera_look[0] - camera_eye[0]).atan2(camera_look[2] - camera_eye[2]) * 32768. / std::f64::consts::PI) as i32;
    mk64_music::sfx(SfxCmd::Camera { pos: v3(camera_eye), yaw });
    mk64_music::sfx(SfxCmd::Player { slot: 0, kart: player_state(d) });
    let ticks = (dt * 60.).max(0.25);
    s.prev.resize(cpus.len(), None);
    s.vel.resize(cpus.len(), [0.; 3]);
    for (i, c) in cpus.iter().enumerate().take(7) {
        let mut c = *c;
        if s.benched & (1 << (i + 1)) != 0 {
            // Out of the game: engine off (once), nothing more to say about it.
            if s.silenced & (1 << (i + 1)) == 0 {
                s.silenced |= 1 << (i + 1);
                mk64_music::sfx(SfxCmd::Engine { slot: i + 1, on: false });
            }
            continue;
        }
        if let Some(prev) = s.prev[i] {
            let moved = [(c.pos[0] - prev[0]) / ticks, (c.pos[1] - prev[1]) / ticks, (c.pos[2] - prev[2]) / ticks];
            // A jump (placed on the grid, rescued) is not a speed.
            let fast = moved.iter().map(|v| v * v).sum::<f64>().sqrt() > 80.;
            for k in 0..3 {
                s.vel[i][k] = if fast { 0. } else { s.vel[i][k] * 0.5 + moved[k] * 0.5 };
            }
        }
        s.prev[i] = Some(c.pos);
        c.vel = s.vel[i];
        mk64_music::sfx(SfxCmd::Player { slot: i + 1, kart: cpu_state(&c) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> SoundFrame {
        SoundFrame { speed_ratio: 0.6, accel: true, clock: 1, driving: true, ..Default::default() }
    }

    #[test]
    fn every_cue_maps_to_the_documented_decomp_sound() {
        let table: [(Cue, Vec<Call>); 20] = [
            (Cue::Hop, vec![Call::Player(0x1900_8000)]),
            (Cue::DriftStart, vec![]),
            (Cue::Charge1, vec![]),
            (Cue::Charge2, vec![Call::Player(0x1900_851E)]),
            (Cue::Turbo, vec![Call::VoiceRandom]),
            (Cue::Bump, vec![Call::Player(0x1900_8001)]),
            (Cue::Box, vec![Call::Player(0x1900_705A), Call::Player(0x1900_8406)]),
            (Cue::Roulette, vec![]),
            (Cue::ItemGet, vec![]),
            (Cue::Mushroom, vec![Call::Voice(1), Call::Player(0x1900_A40B)]),
            (Cue::BananaDrop, vec![Call::Player(0x1900_8012)]),
            (Cue::BananaHit, vec![Call::Voice(3)]),
            (Cue::ShellFire, vec![Call::Player(0x1900_8004), Call::Voice(0)]),
            (Cue::ShellHit, vec![Call::Voice(5), Call::Player(0x1900_9005)]),
            (Cue::Count, vec![Call::Global(0x4900_8003)]),
            (Cue::Go, vec![Call::Global(0x4900_8004)]),
            (Cue::Lap, vec![Call::Player(0x1900_F015)]),
            (Cue::FinalLap, vec![Call::Player(0x1900_F015)]),
            (Cue::Finish, vec![]),
            (Cue::Lakitu, vec![Call::Player(0x1900_A055)]),
        ];
        for (cue, want) in table {
            assert_eq!(calls_for_cue(cue), want, "{cue:?}");
        }
        // Every cue is covered by the table above.
        assert_eq!(Cue::ALL.len(), 20);
    }

    #[test]
    fn sound_words_are_built_the_way_the_decomp_builds_them() {
        // SOUND_ARG_LOAD(bank|flags, byte2, byte3, id) = bank << 24 | byte2 << 16 | byte3 << 8 | id
        let arg = |a: u32, b: u32, c: u32, d: u32| (a << 24) | (b << 16) | (c << 8) | d;
        assert_eq!(id::COUNTDOWN_LIGHT, arg(0x49, 0x00, 0x80, 0x03));
        assert_eq!(id::GREEN_LIGHT, arg(0x49, 0x00, 0x80, 0x04));
        assert_eq!(id::REV_ENGINE, arg(0x49, 0x00, 0x80, 0x18));
        assert_eq!(id::MENU_CURSOR, arg(0x49, 0x00, 0x80, 0x00));
        assert_eq!(id::MENU_ERROR, arg(0x49, 0x00, 0xFF, 0x07));
        assert_eq!(id::EXPLOSION, arg(0x19, 0x00, 0x90, 0x05));
        assert_eq!(id::LAP, arg(0x19, 0x00, 0xF0, 0x15));
        assert_eq!(id::THUNDER, arg(0x51, 0x01, 0xC0, 0x0C));
        assert_eq!(id::STAR, arg(0x31, 0x02, 0x90, 0x08));
        // Banks are the top nibble.
        assert_eq!([id::bank(id::HOP), id::bank(id::COUNTDOWN_LIGHT), id::bank(id::STAR), id::bank(id::SHELL_FLY)], [1, 4, 3, 5]);
        // Voices: character * 0x10 + 0x29008000 + sub; Luigi's spin-out.
        assert_eq!(voice::word(1, voice::SPINOUT), 0x2900_8013);
        assert_eq!(voice::word(7, voice::HIT), 0x2900_8075);
        assert_eq!(voice::CHARACTERS.len(), 8);
        // The CPU engine and the human's: characterId added.
        assert_eq!(id::CPU_ENGINE + 6, arg(0x31, 0x02, 0x80, 0x06));
        assert_eq!(id::ENGINE + 6, 0x0104_FF06);
    }

    #[test]
    fn course_dirs_map_to_the_decomp_course_ids() {
        assert_eq!(sfx::course_id("mario_raceway"), 0);
        assert_eq!(sfx::course_id("mk64_koopa_troopa_beach"), 6);
        assert_eq!(sfx::course_id("dks_jungle_parkway"), 18);
        assert_eq!(sfx::course_id("big_donut"), 19);
        assert_eq!(sfx::course_id("nowhere"), 0);
    }

    #[test]
    fn kart_state_becomes_the_fields_external_c_reads() {
        let d = DriveIn { frame: SoundFrame { drifting: true, boosting: true, ..frame() }, current_speed: 250., speed: 8., pos: [1., 2., 3.], ..Default::default() };
        let p = player_state(&d);
        assert!(p.human && p.kart_props & flags::THROTTLE != 0);
        assert_eq!(p.effects, flags::DRIFTING | flags::MUSHROOM);
        assert_eq!((p.surface_left, p.surface_right), (flags::ASPHALT, flags::ASPHALT));
        // unk_098 = currentSpeed^2 / 25 (player_controller.c func_8002FE84), +10 % in the air or spun out
        assert!((p.engine_speed - 2500.0).abs() < 1e-3);
        let air = player_state(&DriveIn { frame: SoundFrame { midair: true, ..frame() }, current_speed: 250., ..Default::default() });
        assert!((air.engine_speed - 2750.0).abs() < 1e-3);
        let grass = player_state(&DriveIn { frame: SoundFrame { grass: true, ..frame() }, ..Default::default() });
        assert_eq!((grass.surface_left, grass.surface_right), (flags::GRASS, flags::GRASS));
        let parked = player_state(&DriveIn { frame: SoundFrame { accel: true, driving: false, ..frame() }, ..Default::default() });
        assert_eq!(parked.kart_props, 0, "no throttle while on foot");
        let cpu = cpu_state(&CpuIn { character: 6, current_speed: 100., spun: true, ..Default::default() });
        assert!(!cpu.human && cpu.character == 6 && cpu.effects == flags::BANANA_SPINOUT);
        assert_eq!(cpu.type_word(), flags::PLAYER_EXISTS);
        assert_eq!(p.type_word(), flags::PLAYER_EXISTS | flags::PLAYER_HUMAN);
    }
}
