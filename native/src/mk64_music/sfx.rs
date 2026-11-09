//! Mario Kart 64 sound effects: the decomp's own SFX system (`src/audio/external.c`: sound requests, sound banks,
//! sequence player 2 playing sequence 0, the per-player engine / tyre / lightning logic) run inside the same wasm
//! library as the music. This file is the host side of its `sfx_*` exports:
//!
//! * [`id`]: the sound ids (`SOUND_ARG_LOAD(bank|flags, priority-ish, byte3, id)` words of `include/sounds.h` and
//!   the literals at the call sites in `src/`), each with the function that plays it in the game;
//! * [`flags`]: the `Player.effects` / `kartProps` / `type` / surface values `external.c` reads;
//! * [`PlayerIn`]: one kart's state mirrored into `gPlayers[]`; [`SfxCmd`]: everything the host can ask for.
//!
//! The engine, tyre-squeal and surface sounds are not triggered here: the library starts, retunes and stops them
//! itself from the mirrored kart state (`func_800C8CCC`, run by the library's frame update), exactly as in the game.
use super::engine::Mk64Engine;

/// Sound words. Layout (include/sounds.h): top nibble = sound bank, next nibble = flags, then priority / more
/// flags, then the sound id.
pub mod id {
    // --- bank 1 ("player"): `func_800C9060(player, id)` --------------------------------------------------
    /// `kart_hop` (player_controller.c `func_80037CFC`).
    pub const HOP: u32 = 0x1900_8000;
    /// Kart against kart (race_logic.c `func_802903D8`); also used for the wall bump.
    pub const BUMP: u32 = 0x1900_8001;
    /// `func_8008D570`.
    pub const STOP: u32 = 0x1900_8002;
    /// Fire a shell (`update_actor_green_shell`, with voice [`voice::THROW`]).
    pub const SHELL_FIRE: u32 = 0x1900_8004;
    /// Fall into water (`func_8002C4F8`).
    pub const SPLASH: u32 = 0x1900_8008;
    /// Banana dropped behind the kart (`update_actor_banana`, Z released).
    pub const BANANA_DROP: u32 = 0x1900_8012;
    /// Mini-turbo charged to the level that fires (`update_drift_state_counter`, counter 100).
    pub const DRIFT_READY: u32 = 0x1900_851E;
    /// `SOUND_ACTION_EXPLOSION`: falling rock / bomb / green shell hit (`func_8008C528`).
    pub const EXPLOSION: u32 = 0x1900_9005;
    /// Falling rock explosion variant (`SOUND_ACTION_EXPLOSION_2`).
    pub const EXPLOSION_2: u32 = 0x1900_F00C;
    /// Hit by something (thwomp squish, `apply_hit_effect`).
    pub const HIT: u32 = 0x1901_904B;
    /// Lap counted (`func_8005CB60`).
    pub const LAP: u32 = 0x1900_F015;
    /// Kart bounced off a tree / sign with a star (`func_800C98B8` at actors.c).
    pub const STAR_BUMP: u32 = 0x1901_8010;
    /// The race is over for this player (`func_800C76C0` plays it itself).
    pub const FINISH: u32 = 0x1900_F103;
    /// Mushroom / boost pad (`trigger_shroom`, `func_8002A704`, ramp boosts).
    pub const BOOST: u32 = 0x1900_A40B;
    /// Boost ramp take-off (`func_8002D268`).
    pub const RAMP: u32 = 0x1900_A60A;
    /// Lakitu appears with the fishing rod (`update_object_lakitu_fishing2`).
    pub const LAKITU_APPEAR: u32 = 0x1900_A055;
    /// Lakitu drops the kart back on the track.
    pub const LAKITU_DROP: u32 = 0x1900_A056;
    /// Item box broken (`func_8007C7B4`).
    pub const ITEM_BOX: u32 = 0x1900_705A;
    /// The item window starts spinning (`func_8007ABFC`).
    pub const ITEM_ROULETTE: u32 = 0x1900_8406;
    /// The roulette lands on "nothing" (`func_8007B040`).
    pub const ITEM_NONE: u32 = 0x1900_A058;
    /// Battle balloon popped (`pop_player_balloon`).
    pub const BALLOON: u32 = 0x1900_9051;

    // --- bank 4 ("menu / global"): `play_sound2(id)` ---------------------------------------------------------
    /// Starting light (red / orange) (`SOUND_ACTION_COUNTDOWN_LIGHT`, `update_object_lakitu_countdown`).
    pub const COUNTDOWN_LIGHT: u32 = 0x4900_8003;
    /// Green light (`SOUND_ACTION_GREEN_LIGHT`).
    pub const GREEN_LIGHT: u32 = 0x4900_8004;
    /// Engines rev at the start of a Grand Prix race (`func_8028FCBC`).
    pub const REV_ENGINE: u32 = 0x4900_8018;
    pub const REV_ENGINE_2: u32 = 0x4900_8019;
    /// `SOUND_ACTION_PING`.
    pub const PING: u32 = 0x4900_801C;
    pub const MENU_CURSOR: u32 = 0x4900_8000;
    pub const MENU_SELECT: u32 = 0x4900_8001;
    pub const MENU_BACK: u32 = 0x4900_8002;
    pub const MENU_OK: u32 = 0x4900_8016;
    pub const MENU_ERROR: u32 = 0x4900_FF07;
    pub const MENU_ENTER: u32 = 0x4900_801A;
    pub const SCORE_COUNT: u32 = 0x4900_8017;

    // --- bank 5 ("environment"): `func_800C9D80` / `func_800C98B8` at a place -----------------------------------
    /// Green / red shell passing (`update_actor_red_blue_shell`).
    pub const SHELL_FLY: u32 = 0x5101_8008;
    /// Thunderbolt (`SOUND_ITEM_THUNDERBOLT`).
    pub const THUNDER: u32 = 0x5101_C00C;

    // --- bank 3 ("items / CPU engines") -------------------------------------------------------------------
    /// A kart shrunk by the thunderbolt (`func_800CAB4C`: `func_800C9060(player, 0x1900F013)`).
    pub const LIGHTNING_HIT: u32 = 0x1900_F013;
    /// Star (`SOUND_ITEM_STAR`).
    pub const STAR: u32 = 0x3102_9008;
    /// A CPU kart's engine: `0x31028000 + characterId` (`func_800C9A88`).
    pub const CPU_ENGINE: u32 = 0x3102_8000;
    /// The human kart's engine idle: `0x0104FF00 + characterId` (`func_800C94A4`).
    pub const ENGINE: u32 = 0x0104_FF00;

    /// Bank (top nibble) of a sound word.
    pub fn bank(bits: u32) -> usize {
        (bits >> 28) as usize
    }
}

/// Character voices: `func_800C90F4(player, characterId * 0x10 + 0x29008000 + sub)`.
pub mod voice {
    /// Shell thrown (`update_actor_green_shell`).
    pub const THROW: u32 = 0;
    /// "Yeah / wahoo" on a boost (mushroom, start boost, ramp), `func_8002A704`; sub 2 is the second take
    /// `func_800C9250` picks at random between 1 and 2.
    pub const BOOST: u32 = 1;
    pub const BOOST_2: u32 = 2;
    /// Spun out (`add_spinout_effect`, `func_8008D7B0`).
    pub const SPINOUT: u32 = 3;
    /// Put back on the track by Lakitu (`func_8008F494`).
    pub const RESCUED: u32 = 4;
    /// Hit hard: green shell, explosion, tumble, lightning (`func_8008C528`, `trigger_vertical_tumble`).
    pub const HIT: u32 = 5;
    /// Recovering from a spin (`apply_banana_near_spinout_effect`, `apply_hit_effect`).
    pub const RECOVER: u32 = 8;
    /// `func_8008D570`.
    pub const STOPPED: u32 = 0xC;
    /// Fastest lap / star bump (`func_8005CB60`, actors.c).
    pub const CHEER: u32 = 0xD;
    /// Characters in the order of the decomp's `characterId`.
    pub const CHARACTERS: [&str; 8] = ["Mario", "Luigi", "Yoshi", "Toad", "DK", "Wario", "Peach", "Bowser"];
    /// Full sound word of a voice.
    pub fn word(character: u32, sub: u32) -> u32 {
        0x2900_8000 + (character & 7) * 0x10 + sub
    }
}

/// Value sets `external.c` reads from `Player` (include/defines.h, include/mk64.h).
pub mod flags {
    // Player.effects
    pub const BRAKING: u32 = 0x1;
    pub const HOP: u32 = 0x2;
    pub const MIDAIR: u32 = 0x8;
    pub const DRIFTING: u32 = 0x10;
    pub const AB_SPIN: u32 = 0x20;
    pub const DRIVING_SPINOUT: u32 = 0x40;
    pub const BANANA_SPINOUT: u32 = 0x80;
    pub const MINI_TURBO: u32 = 0x100;
    pub const STAR: u32 = 0x200;
    pub const HIT_BY_GREEN_SHELL: u32 = 0x400;
    pub const BANANA_NEAR_SPINOUT: u32 = 0x800;
    pub const MUSHROOM: u32 = 0x2000;
    pub const EARLY_START_SPINOUT: u32 = 0x4000;
    pub const LIGHTNING: u32 = 0x4000_0000;
    // Player.kartProps
    pub const THROTTLE: i32 = 0x20;
    pub const DRIVING_NEAR_SPINOUT: i32 = 0x4000;
    // Player.type
    pub const PLAYER_INVISIBLE_OR_BOMB: i32 = 1 << 8;
    pub const PLAYER_START_SEQUENCE: i32 = 1 << 13;
    pub const PLAYER_HUMAN: i32 = 1 << 14;
    pub const PLAYER_EXISTS: i32 = 1 << 15;
    // Player.oobProps
    pub const UNDER_OOB_OR_FLUID_LEVEL: i32 = 0x1;
    // Surface types of the tyres (enum SURFACE_TYPE)
    pub const AIRBORNE: i32 = 0;
    pub const ASPHALT: i32 = 1;
    pub const DIRT: i32 = 2;
    pub const SAND: i32 = 3;
    pub const STONE: i32 = 4;
    pub const SNOW: i32 = 5;
    pub const BRIDGE: i32 = 6;
    pub const SAND_OFFROAD: i32 = 7;
    pub const GRASS: i32 = 8;
    pub const ICE: i32 = 9;
    pub const WET_SAND: i32 = 10;
    pub const SNOW_OFFROAD: i32 = 11;
    pub const DIRT_OFFROAD: i32 = 13;
    pub const TRAIN_TRACK: i32 = 14;
    pub const ROPE_BRIDGE: i32 = 16;
    pub const WOOD_BRIDGE: i32 = 17;
}

/// The decomp's `COURSE_*` id (include/course.h) of a course directory (`mario_raceway`, with or without `mk64_`);
/// `gCurrentCourseId` changes a few sounds (the Koopa Beach water, the DK Jungle rev).
pub fn course_id(dir: &str) -> u32 {
    const ORDER: [&str; 20] = [
        "mario_raceway", "choco_mountain", "bowsers_castle", "banshee_boardwalk", "yoshi_valley", "frappe_snowland",
        "koopa_troopa_beach", "royal_raceway", "luigi_raceway", "moo_moo_farm", "toads_turnpike", "kalimari_desert",
        "sherbet_land", "rainbow_road", "wario_stadium", "block_fort", "skyscraper", "double_deck", "dks_jungle_parkway",
        "big_donut",
    ];
    let dir = dir.strip_prefix("mk64_").unwrap_or(dir);
    ORDER.iter().position(|d| *d == dir).unwrap_or(0) as u32
}

/// `unk_098` of the decomp's `Player`, the engine's speed measure (player_controller.c `func_8002FE84`): the
/// square of `currentSpeed` over 25, 10 % more while in the air or spun out.
pub fn engine_speed(current_speed: f64, airborne_or_spinning: bool) -> f64 {
    let v = current_speed * current_speed / 25.0;
    if airborne_or_spinning {
        v * 1.1
    } else {
        v
    }
}

/// One kart as the sound system sees it. Positions and velocities are in the decomp's world units (velocity per
/// 60 Hz kart update), exactly what `Player.pos` / `Player.velocity` hold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerIn {
    /// `characterId`: 0 Mario, 1 Luigi, 2 Yoshi, 3 Toad, 4 DK, 5 Wario, 6 Peach, 7 Bowser.
    pub character: u32,
    pub human: bool,
    pub kart_props: i32,
    pub effects: u32,
    pub oob: i32,
    pub speed: f32,
    pub current_speed: f32,
    pub engine_speed: f32,
    /// `unk_0C0`: the drift / steering angle (s16 units).
    pub unk_0c0: i32,
    pub unk_20c: f32,
    pub surface_left: i32,
    pub surface_right: i32,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
}

impl Default for PlayerIn {
    fn default() -> Self {
        Self {
            character: 0,
            human: false,
            kart_props: 0,
            effects: 0,
            oob: 0,
            speed: 0.,
            current_speed: 0.,
            engine_speed: 0.,
            unk_0c0: 0,
            unk_20c: 0.,
            surface_left: flags::ASPHALT,
            surface_right: flags::ASPHALT,
            pos: [0.; 3],
            vel: [0.; 3],
        }
    }
}

impl PlayerIn {
    /// `Player.type` word.
    pub fn type_word(&self) -> i32 {
        flags::PLAYER_EXISTS | if self.human { flags::PLAYER_HUMAN } else { 0 }
    }
}

/// Everything the host can ask of the sound effects.
#[derive(Clone, Debug, PartialEq)]
pub enum SfxCmd {
    /// A race (or free roam) starts: reset the game-side state. `course` is the decomp's `COURSE_*` id.
    Begin { course: u32, humans: u32 },
    /// The kart mode ends: stop every sound and drop back to the music-only update.
    End,
    /// The listener (camera) place and yaw (s16 units).
    Camera { pos: [f32; 3], yaw: i32 },
    /// Mirror kart `slot` (0 = the player).
    Player { slot: usize, kart: PlayerIn },
    /// Place `slot` is at a position/velocity (things that make sound where they are).
    Place { slot: usize, pos: [f32; 3], vel: [f32; 3] },
    /// A kart's engine starts (human: `func_800C94A4`, CPU: `func_800C9A88`) or ends.
    Engine { slot: usize, on: bool },
    /// `func_800C9060`: a sound of this player's kart.
    PlayerSound { slot: usize, bits: u32 },
    /// `func_800C9018`: end that sound.
    PlayerStop { slot: usize, bits: u32 },
    /// `func_800C90F4`: a character voice (`voice::*`).
    Voice { slot: usize, sub: u32 },
    /// `func_800C9250`: the random boost voice.
    VoiceRandom { slot: usize },
    /// `play_sound2`: menus, starting lights.
    Global { bits: u32 },
    /// `func_800C98B8`: a one-shot sound at a pooled place (`Place`).
    PlaceSound { slot: usize, bits: u32 },
    /// `func_800C9D80`: a looping sound at a place.
    PlaceLoop { slot: usize, bits: u32 },
    /// `func_800C9EF4`: end it.
    PlaceStop { slot: usize, bits: u32 },
    /// Pause (`func_800CA0A0`) / resume (`func_800CA0B8`): new sound requests are ignored while paused.
    Pause(bool),
}

impl Mk64Engine {
    /// True for a library built with the `sfx_*` exports (older `mk64-audio.wasm` files lack them).
    pub fn has_sfx(&self) -> bool {
        self.has_export("sfx_begin")
    }

    /// Apply one command to the library.
    pub fn sfx(&mut self, cmd: &SfxCmd) {
        let f = |v: f32| v as f64;
        match cmd {
            SfxCmd::Begin { course, humans } => drop(self.call("sfx_begin", &[*course as f64, *humans as f64])),
            SfxCmd::End => drop(self.call("sfx_end", &[])),
            SfxCmd::Camera { pos, yaw } => drop(self.call("sfx_camera", &[f(pos[0]), f(pos[1]), f(pos[2]), *yaw as f64])),
            SfxCmd::Player { slot, kart: k } => drop(self.call(
                "sfx_player",
                &[
                    *slot as f64,
                    k.character as f64,
                    k.type_word() as f64,
                    k.kart_props as f64,
                    k.effects as f64,
                    k.oob as f64,
                    f(k.speed),
                    f(k.current_speed),
                    f(k.engine_speed),
                    k.unk_0c0 as f64,
                    f(k.unk_20c),
                    k.surface_left as f64,
                    k.surface_right as f64,
                    f(k.pos[0]),
                    f(k.pos[1]),
                    f(k.pos[2]),
                    f(k.vel[0]),
                    f(k.vel[1]),
                    f(k.vel[2]),
                ],
            )),
            SfxCmd::Place { slot, pos, vel } => {
                drop(self.call("sfx_place_set", &[*slot as f64, f(pos[0]), f(pos[1]), f(pos[2]), f(vel[0]), f(vel[1]), f(vel[2])]))
            }
            SfxCmd::Engine { slot, on } => drop(self.call(if *on { "sfx_engine_start" } else { "sfx_engine_stop" }, &[*slot as f64, (*slot == 0) as u8 as f64])),
            SfxCmd::PlayerSound { slot, bits } => drop(self.call("sfx_player_sound", &[*slot as f64, *bits as f64])),
            SfxCmd::PlayerStop { slot, bits } => drop(self.call("sfx_player_stop", &[*slot as f64, *bits as f64])),
            SfxCmd::Voice { slot, sub } => drop(self.call("sfx_voice", &[*slot as f64, *sub as f64])),
            SfxCmd::VoiceRandom { slot } => drop(self.call("sfx_voice_random", &[*slot as f64])),
            SfxCmd::Global { bits } => drop(self.call("sfx_global", &[*bits as f64])),
            SfxCmd::PlaceSound { slot, bits } => drop(self.call("sfx_place_play", &[*slot as f64, *bits as f64])),
            SfxCmd::PlaceLoop { slot, bits } => drop(self.call("sfx_place_loop", &[*slot as f64, *bits as f64])),
            SfxCmd::PlaceStop { slot, bits } => drop(self.call("sfx_place_stop", &[*slot as f64, *bits as f64])),
            SfxCmd::Pause(on) => drop(self.call("sfx_pause", &[*on as u8 as f64])),
        }
    }

    /// Sounds currently held by a sound bank (playing or fading).
    pub fn sfx_bank_sounds(&mut self, bank: usize) -> u32 {
        self.call("sfx_bank_sounds", &[bank as f64]).ok().and_then(|v| v.first().copied()).unwrap_or(0.0) as u32
    }
    /// Is exactly this sound word in its bank?
    pub fn sfx_bank_has(&mut self, bits: u32) -> bool {
        self.call("sfx_bank_has", &[id::bank(bits) as f64, bits as f64]).ok().and_then(|v| v.first().copied()).unwrap_or(0.0) != 0.0
    }
    /// The pitch scale the library is playing player `slot`'s engine at (`D_800E9F7C[slot].unk_0C`).
    pub fn sfx_engine_pitch(&mut self, slot: usize) -> f32 {
        self.call("sfx_engine_pitch", &[slot as f64]).ok().and_then(|v| v.first().copied()).unwrap_or(0.0) as f32
    }
    /// Total sounds in the six banks.
    pub fn sfx_busy(&mut self) -> u32 {
        (0..6).map(|b| self.sfx_bank_sounds(b)).sum()
    }
}
