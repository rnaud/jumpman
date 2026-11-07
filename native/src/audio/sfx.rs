//! Port of `Sfx.ts` / `music.ts`: names sounds, tracks the listener state and posts [`Cmd`]s to the
//! wasm sound engine (which does the mixing, attenuation/panning from the camera-space positions and
//! the underwater reverb/filters of the original game).
use super::processor::{Cmd, MasterGain, MusicEvent};
use super::terrain::terrain_sound;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::mpsc::Sender;

const SOUNDS_JSON: &str = include_str!("../../../assets/audio/music/sounds.json");
const COURSES_JSON: &str = include_str!("../../../assets/audio/music/courses.json");
/// Master gain of the original (`0.24`).
pub const MASTER_VOLUME: f32 = 0.24;

#[derive(Clone, Debug, PartialEq)]
pub struct MusicTrack {
    pub name: String,
    pub sequence: u32,
    pub preset: u32,
    pub terrain: usize,
    pub level: u32,
}

#[derive(Deserialize)]
struct AreaJson {
    name: String,
    sequence: u32,
    preset: u32,
    terrain: usize,
}
#[derive(Deserialize)]
struct LevelJson {
    level: u32,
    areas: Vec<AreaJson>,
}

/// `courseMusic(course, area)`; unknown courses fall back to Bob-omb Battlefield.
pub fn course_music(course: &str, area: i32) -> MusicTrack {
    let levels: HashMap<String, LevelJson> = serde_json::from_str(COURSES_JSON).expect("courses.json");
    let source = levels.get(course).or_else(|| levels.get("bob")).expect("bob music");
    let a = source.areas.get((area.max(1) - 1) as usize).or_else(|| source.areas.first()).expect("area");
    MusicTrack { name: a.name.clone(), sequence: a.sequence, preset: a.preset, terrain: a.terrain, level: source.level }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeachVoice {
    Mario,
    Stars,
    Thanks,
    ThankYou,
    Special,
    Cake,
    ForMario,
    Mario2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignCue {
    Read,
    Open,
    Page,
    Close,
}

/// A position in camera space (three.js convention: x right, y up, -z forward), in sim units.
pub type Pos = [f64; 3];

/// Everything `Game` hands the sound player each frame (`Sfx.setPlayer`).
pub struct PlayerState {
    pub camera_space_pos: Pos,
    pub metal: bool,
    pub floor_type: Option<i32>,
    /// Floor is more than 10 units under the water surface (and not lava).
    pub underwater_floor: bool,
}

pub struct Sfx {
    tx: Sender<Cmd>,
    gain: MasterGain,
    sounds: HashMap<String, f64>,
    pub muted: bool,
    current_music: Option<MusicTrack>,
    power: f64,
    location: [f64; 7],
    surface: f64,
    metal: bool,
    mario_pos: Pos,
    next_source: u32,
    /// Names asked for that sounds.json lacks (should stay empty).
    pub missing: Vec<String>,
}

impl Sfx {
    pub fn new(tx: Sender<Cmd>, gain: MasterGain, muted: bool) -> Self {
        let sounds: HashMap<String, f64> = serde_json::from_str(SOUNDS_JSON).expect("sounds.json");
        gain.set(if muted { 0.0 } else { MASTER_VOLUME });
        Self {
            tx,
            gain,
            sounds,
            muted,
            current_music: None,
            power: 0.0,
            location: [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            surface: 0.0,
            metal: false,
            mario_pos: [0.0; 3],
            next_source: 2,
            missing: Vec::new(),
        }
    }

    fn post(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.gain.set(if muted { 0.0 } else { MASTER_VOLUME });
    }

    // ---- music ----

    /// `startMusic`: restarts only when the sequence, preset or level changed.
    pub fn start_music(&mut self, track: MusicTrack) {
        let previous = self.current_music.replace(track.clone());
        self.location[0] = track.level as f64;
        let same = previous.as_ref().is_some_and(|p| p.sequence == track.sequence && p.preset == track.preset && p.level == track.level);
        if !same {
            self.post(Cmd::Location(self.location));
            self.post(Cmd::Track { sequence: track.sequence, preset: track.preset });
            self.post(Cmd::Power(self.power, 0.0));
        }
    }

    pub fn set_music_location(&mut self, area: i32, room: i32, pos: [f64; 3], stars: u32) {
        if self.current_music.is_none() {
            return;
        }
        let new = [self.location[0], area as f64, room as f64, pos[0], pos[1], pos[2], stars as f64];
        if new == self.location {
            return;
        }
        self.location = new;
        self.post(Cmd::Location(new));
    }

    pub fn set_power_music(&mut self, seq: f64, fade: bool) {
        if seq == self.power && !fade {
            return;
        }
        self.power = seq;
        self.post(Cmd::Power(seq, if fade { 1.0 } else { 0.0 }));
    }

    pub fn stop_music(&mut self) {
        self.power = 0.0;
        self.current_music = None;
        self.post(Cmd::Stop);
    }

    pub fn music_track(&self) -> Option<&str> {
        self.current_music.as_ref().map(|t| t.name.as_str())
    }
    pub fn current_music(&self) -> Option<&MusicTrack> {
        self.current_music.as_ref()
    }
    fn terrain_kind(&self) -> usize {
        self.current_music.as_ref().map_or(0, |t| t.terrain)
    }
    pub fn music_event(&self, e: MusicEvent) {
        self.post(Cmd::Event(e));
    }
    pub fn set_outdoor_sources(&self, birds: Vec<Pos>) {
        self.post(Cmd::Birds(birds));
    }

    // ---- listener ----

    /// `setPlayer`.
    pub fn set_player(&mut self, p: &PlayerState) {
        self.mario_pos = p.camera_space_pos;
        self.metal = p.metal;
        self.surface = terrain_sound(self.terrain_kind(), p.floor_type, p.underwater_floor) as f64;
    }

    // ---- effects ----

    fn effect_at(&mut self, name: &str, offset: f64, variants: f64, pos: Pos, source: f64) {
        let Some(&id) = self.sounds.get(name) else {
            self.missing.push(name.to_string());
            return;
        };
        self.post(Cmd::Sound([id + offset * 65536.0, variants, source, pos[0], pos[1], pos[2]]));
    }
    fn effect(&mut self, name: &str) {
        self.effect_at(name, 0.0, 1.0, self.mario_pos, 0.0);
    }
    fn effect_v(&mut self, name: &str, offset: f64, variants: f64) {
        self.effect_at(name, offset, variants, self.mario_pos, 0.0);
    }
    fn terrain(&mut self, normal: &str, metal: &str) {
        if self.metal {
            self.effect_v(metal, 0.0, 1.0);
        } else {
            self.effect_v(normal, self.surface, 1.0);
        }
    }
    fn object_sound(&mut self, name: &str, pos: Pos) {
        let source = self.next_source as f64;
        self.effect_at(name, 0.0, 1.0, pos, source);
        self.next_source = 2 + (self.next_source - 1) % 12;
    }

    pub fn peach_voice(&mut self, v: PeachVoice) {
        self.effect(match v {
            PeachVoice::Mario => "PEACH_MARIO",
            PeachVoice::Stars => "PEACH_POWER_OF_THE_STARS",
            PeachVoice::Thanks => "PEACH_THANKS_TO_YOU",
            PeachVoice::ThankYou => "PEACH_THANK_YOU_MARIO",
            PeachVoice::Special => "PEACH_SOMETHING_SPECIAL",
            PeachVoice::Cake => "PEACH_BAKE_A_CAKE",
            PeachVoice::ForMario => "PEACH_FOR_MARIO",
            PeachVoice::Mario2 => "PEACH_MARIO2",
        });
    }
    fn jump_with(&mut self, voice: &str, variants: f64) {
        self.terrain("ACTION_TERRAIN_JUMP", "ACTION_METAL_JUMP");
        self.effect_v(voice, 0.0, variants);
    }
    pub fn jump(&mut self) {
        self.jump_with("MARIO_YAH_WAH_HOO", 3.0);
    }
    pub fn backflip(&mut self) {
        self.jump_with("MARIO_YAH_WAH_HOO", 1.0);
    }
    pub fn double_jump(&mut self) {
        self.jump_with("MARIO_HOOHOO", 1.0);
    }
    pub fn triple_jump(&mut self) {
        self.jump_with("MARIO_YAHOO_WAHA_YIPPEE", 5.0);
    }
    pub fn long_jump(&mut self) {
        self.jump_with("MARIO_YAHOO", 1.0);
    }
    pub fn kick(&mut self) {
        self.effect("MARIO_PUNCH_HOO");
    }
    pub fn wall_kick(&mut self) {
        self.effect_v("MARIO_YAH_WAH_HOO", 0.0, 3.0);
    }
    pub fn splash(&mut self) {
        self.effect("ACTION_UNKNOWN430");
    }
    pub fn swim(&mut self, fast: bool) {
        self.effect(if fast { "ACTION_SWIM_FAST" } else { "ACTION_SWIM" });
    }
    pub fn flutter(&mut self) {
        self.effect("ACTION_UNKNOWN434");
    }
    pub fn footstep(&mut self, tiptoe: bool) {
        if tiptoe {
            self.terrain("ACTION_TERRAIN_STEP_TIPTOE", "ACTION_METAL_STEP_TIPTOE");
        } else {
            self.terrain("ACTION_TERRAIN_STEP", "ACTION_METAL_STEP");
        }
    }
    pub fn ride_shell(&mut self, lava: bool) {
        if lava {
            self.effect("MOVING_RIDING_SHELL_LAVA");
        } else {
            self.effect_v("MOVING_TERRAIN_RIDING_SHELL", self.surface, 1.0);
        }
    }
    pub fn stomp_goomba(&mut self) {
        self.effect("ACTION_BOUNCE_OFF_OBJECT");
        self.effect("OBJ_STOMPED");
    }
    pub fn teleport(&mut self) {
        self.effect("ACTION_TELEPORT");
    }
    pub fn shell_jump(&mut self) {
        self.effect_v("ACTION_TERRAIN_JUMP", self.surface, 1.0);
    }
    pub fn land(&mut self) {
        self.terrain("ACTION_TERRAIN_LANDING", "ACTION_METAL_LANDING");
    }
    pub fn ground_pound(&mut self) {
        self.terrain("ACTION_TERRAIN_HEAVY_LANDING", "ACTION_METAL_HEAVY_LANDING");
    }
    pub fn sign_message(&mut self, cue: SignCue) {
        self.effect(match cue {
            SignCue::Read => "ACTION_READ_SIGN",
            SignCue::Open => "MENU_MESSAGE_APPEAR",
            SignCue::Page => "MENU_MESSAGE_NEXT_PAGE",
            SignCue::Close => "MENU_MESSAGE_DISAPPEAR",
        });
    }
    pub fn cannon_door(&mut self, pos: Pos) {
        self.object_sound("GENERAL_CANNON_UP", pos);
    }
    pub fn cannon_move(&mut self, phase: u8) {
        self.effect(["OBJ_CANNON1", "OBJ_CANNON2", "OBJ_CANNON3", "MOVING_AIM_CANNON"][phase.min(3) as usize]);
    }
    pub fn water_bomb(&mut self, pos: Pos, phase: &str) {
        let name = match phase {
            "drop" => "OBJ_SOMETHING_LANDING",
            "bounce" => "OBJ_WATER_BOMB_BOUNCING",
            "splash" => "OBJ_DIVING_IN_WATER",
            _ => return,
        };
        self.object_sound(name, pos);
    }
    /// `cue`: moving | stop | lower | upper.
    pub fn lift(&mut self, pos: Pos, cue: &str) {
        match cue {
            "lower" | "upper" => self.effect_at("ENV_ELEVATOR4", 0.0, 1.0, pos, if cue == "lower" { 14.0 } else { 15.0 }),
            "moving" => self.effect_at("ENV_ELEVATOR3", 0.0, 1.0, pos, 16.0),
            _ => self.effect_at("GENERAL_UNKNOWN4_LOWPRIO", 0.0, 1.0, pos, 16.0),
        }
    }
    pub fn snowman(&mut self, pos: Pos, cue: &str) {
        let name = match cue {
            "roll" => "ENV_UNKNOWN2",
            "bounce" => "OBJ_SNOWMAN_BOUNCE",
            "assembled" => "OBJ_SNOWMAN_EXPLODE",
            _ => return,
        };
        self.effect_at(name, 0.0, 1.0, pos, 17.0);
    }
    pub fn racer_sound(&mut self, pos: Pos, cue: &str) {
        let name = match cue {
            "gun" => "GENERAL_RACE_GUN_SHOT",
            "slide" => "AIR_ROUGH_SLIDE",
            "impact" => "OBJ_POUNDING_LOUD",
            _ => return,
        };
        self.effect_at(name, 0.0, 1.0, pos, 17.0);
    }
    pub fn penguin(&mut self, pos: Pos, diving: bool) {
        self.object_sound(if diving { "OBJ_BABY_PENGUIN_DIVE" } else { "OBJ2_BABY_PENGUIN_YELL" }, pos);
    }
    pub fn blast_wall(&mut self, pos: Pos) {
        self.object_sound("GENERAL_WALL_EXPLOSION", pos);
    }
    pub fn water_cannon(&mut self, pos: Pos) {
        self.object_sound("OBJ_CANNON4", pos);
    }
    pub fn cap_switch(&mut self) {
        self.effect("GENERAL_ACTIVATE_CAP_SWITCH");
    }
    pub fn spawned_star(&mut self, pos: Pos, appearing: bool) {
        if appearing {
            self.object_sound("GENERAL2_STAR_APPEARS", pos);
        }
        self.effect_at("ENV_STAR", 0.0, 1.0, pos, 17.0);
    }
    pub fn coordinate_star(&mut self, pos: Pos, landed: bool) {
        self.effect_at("ENV_STAR", 0.0, 1.0, pos, 17.0);
        if landed {
            self.object_sound("GENERAL_STAR_APPEARS", pos);
        }
    }
    pub fn move_menu(&mut self) {
        self.effect("MENU_CHANGE_SELECT");
    }
    pub fn select_mission(&mut self) {
        self.effect("MENU_STAR_SOUND_LETS_A_GO");
    }
    pub fn star_sound(&mut self) {
        self.effect("MENU_STAR_SOUND");
    }
    pub fn wing_flight(&mut self, pull_up: bool) {
        if pull_up {
            self.effect("ACTION_FLYING_FAST");
            self.effect_v("MARIO_YAHOO_WAHA_YIPPEE", 0.0, 5.0);
        }
        self.effect("MOVING_FLYING");
    }
    pub fn flight_boundary(&mut self) {
        self.effect("ACTION_HIT");
    }
    pub fn flight_burn(&mut self) {
        self.effect("MARIO_ON_FIRE");
    }
    pub fn cannon_fire(&mut self) {
        self.effect("ACTION_FLYING_FAST");
        self.effect("OBJ_POUNDING_CANNON");
        self.effect("MARIO_YAHOO");
    }
    pub fn bowser_grab(&mut self) {
        self.effect("MARIO_HRMM");
        self.effect("OBJ_BOWSER_TAIL_PICKUP");
    }
    pub fn bowser_spin(&mut self) {
        self.effect("OBJ_BOWSER_SPINNING");
    }
    pub fn bowser_throw(&mut self, fast: bool) {
        self.effect(if fast { "MARIO_SO_LONGA_BOWSER" } else { "MARIO_HERE_WE_GO" });
    }
    pub fn chest_answer(&mut self, correct: bool) {
        self.effect_at(if correct { "GENERAL2_RIGHT_ANSWER" } else { "MENU_CAMERA_BUZZ" }, 0.0, 1.0, [0.0; 3], 1.0);
    }
    pub fn bonk(&mut self) {
        self.effect("ACTION_BONK");
    }
    pub fn hurt(&mut self) {
        self.effect("MARIO_ATTACKED");
    }
    pub fn death(&mut self) {
        self.effect("MARIO_DYING");
    }
    pub fn ground_pound_start(&mut self) {
        self.effect("ACTION_THROW");
        self.effect("ACTION_SPIN");
    }
    pub fn ground_pound_drop(&mut self) {
        self.effect("MARIO_GROUND_POUND_WAH");
    }
    pub fn dive(&mut self) {
        self.effect("ACTION_THROW");
        self.effect("MARIO_HOOHOO");
    }
    pub fn koopa_sound(&mut self, pos: Pos, cue: &str) {
        let name = match cue {
            "shell" => "OBJ_KOOPA_DAMAGE",
            "stomp" => "OBJ_STOMPED",
            "defeated" => "OBJ_KOOPA_FLYGUY_DEATH",
            _ => return,
        };
        self.object_sound(name, pos);
    }
    pub fn ball_bounce(&mut self, pos: Pos) {
        self.object_sound("GENERAL_QUIET_POUND1_LOWPRIO", pos);
    }
    pub fn coin_bounce(&mut self, pos: Pos) {
        self.object_sound("GENERAL_COIN_DROP", pos);
    }
    pub fn one_up(&mut self) {
        self.effect("GENERAL_COLLECT_1UP");
    }
    pub fn death_warp(&mut self) {
        self.effect("MENU_BOWSER_LAUGH");
    }
    pub fn death_landing(&mut self) {
        self.effect("MARIO_OOOF2");
    }
    pub fn mama_mia(&mut self) {
        self.effect("MARIO_MAMA_MIA");
    }
    pub fn course_coin(&mut self, count: u32, record: bool) {
        self.effect("MENU_YOSHI_GAIN_LIVES");
        if [50, 100, 150].contains(&count) {
            self.effect("GENERAL_COLLECT_1UP");
        }
        if record {
            self.effect("MENU_MARIO_CASTLE_WARP2");
        }
    }
    pub fn coin(&mut self, underwater: bool) {
        self.effect(if underwater { "GENERAL_COIN_WATER" } else { "GENERAL_COIN" });
    }
    pub fn secret(&mut self, count: u32) {
        self.effect_at("MENU_COLLECT_SECRET", count.saturating_sub(1) as f64, 1.0, [0.0; 3], 1.0);
    }
    pub fn red_coin(&mut self, count: u32) {
        self.effect_at("MENU_COLLECT_RED_COIN", count.saturating_sub(1) as f64, 1.0, [0.0; 3], 1.0);
    }
    pub fn star(&mut self) {
        self.effect("MARIO_HERE_WE_GO");
    }
    pub fn explosion(&mut self, pos: Pos, mine: bool) {
        self.object_sound(if mine { "GENERAL_BOWSER_BOMB_EXPLOSION" } else { "GENERAL2_BOBOMB_EXPLOSION" }, pos);
    }
    pub fn break_box(&mut self, pos: Pos) {
        self.object_sound("GENERAL_BREAK_BOX", pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver};

    fn sfx() -> (Sfx, Receiver<Cmd>) {
        let (tx, rx) = channel();
        (Sfx::new(tx, MasterGain::new(0.0), false), rx)
    }

    #[test]
    fn every_named_sound_exists_in_sounds_json() {
        let (mut s, _rx) = sfx();
        let p = [1.0, 2.0, 3.0];
        for v in [
            PeachVoice::Mario,
            PeachVoice::Stars,
            PeachVoice::Thanks,
            PeachVoice::ThankYou,
            PeachVoice::Special,
            PeachVoice::Cake,
            PeachVoice::ForMario,
            PeachVoice::Mario2,
        ] {
            s.peach_voice(v);
        }
        s.jump();
        s.backflip();
        s.double_jump();
        s.triple_jump();
        s.long_jump();
        s.kick();
        s.wall_kick();
        s.splash();
        s.swim(true);
        s.swim(false);
        s.flutter();
        s.footstep(true);
        s.footstep(false);
        s.ride_shell(true);
        s.ride_shell(false);
        s.stomp_goomba();
        s.teleport();
        s.shell_jump();
        s.land();
        s.ground_pound();
        for c in [SignCue::Read, SignCue::Open, SignCue::Page, SignCue::Close] {
            s.sign_message(c);
        }
        s.cannon_door(p);
        for i in 0..4 {
            s.cannon_move(i);
        }
        for c in ["drop", "bounce", "splash"] {
            s.water_bomb(p, c);
        }
        for c in ["moving", "stop", "lower", "upper"] {
            s.lift(p, c);
        }
        for c in ["roll", "bounce", "assembled"] {
            s.snowman(p, c);
        }
        for c in ["gun", "slide", "impact"] {
            s.racer_sound(p, c);
        }
        s.penguin(p, true);
        s.penguin(p, false);
        s.blast_wall(p);
        s.water_cannon(p);
        s.cap_switch();
        s.spawned_star(p, true);
        s.coordinate_star(p, true);
        s.move_menu();
        s.select_mission();
        s.star_sound();
        s.wing_flight(true);
        s.wing_flight(false);
        s.flight_boundary();
        s.flight_burn();
        s.cannon_fire();
        s.bowser_grab();
        s.bowser_spin();
        s.bowser_throw(true);
        s.bowser_throw(false);
        s.chest_answer(true);
        s.chest_answer(false);
        s.bonk();
        s.hurt();
        s.death();
        s.ground_pound_start();
        s.ground_pound_drop();
        s.dive();
        for c in ["shell", "stomp", "defeated"] {
            s.koopa_sound(p, c);
        }
        s.ball_bounce(p);
        s.coin_bounce(p);
        s.one_up();
        s.death_warp();
        s.death_landing();
        s.mama_mia();
        s.course_coin(50, true);
        s.coin(true);
        s.coin(false);
        s.secret(3);
        s.red_coin(2);
        s.star();
        s.explosion(p, true);
        s.explosion(p, false);
        s.break_box(p);
        assert!(s.missing.is_empty(), "{:?}", s.missing);
    }

    #[test]
    fn terrain_offset_goes_into_the_high_word_and_metal_ignores_it() {
        let (mut s, rx) = sfx();
        s.start_music(course_music("bob", 1));
        while rx.try_recv().is_ok() {}
        s.set_player(&PlayerState { camera_space_pos: [0.0, 0.0, -100.0], metal: false, floor_type: Some(0x30), underwater_floor: false });
        s.land();
        let base = s.sounds["ACTION_TERRAIN_LANDING"];
        match rx.try_recv().unwrap() {
            Cmd::Sound(a) => {
                assert_eq!(a[0], base + 3.0 * 65536.0);
                assert_eq!(a[5], -100.0);
            }
            c => panic!("{c:?}"),
        }
        s.set_player(&PlayerState { camera_space_pos: [0.0; 3], metal: true, floor_type: Some(0x30), underwater_floor: false });
        s.land();
        match rx.try_recv().unwrap() {
            Cmd::Sound(a) => assert_eq!(a[0], s.sounds["ACTION_METAL_LANDING"]),
            c => panic!("{c:?}"),
        }
    }

    #[test]
    fn object_sources_cycle_two_to_thirteen() {
        let (mut s, rx) = sfx();
        let mut sources = vec![];
        for _ in 0..14 {
            s.break_box([0.0; 3]);
            if let Cmd::Sound(a) = rx.try_recv().unwrap() {
                sources.push(a[2] as u32);
            }
        }
        assert_eq!(sources, vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 2, 3]);
    }

    #[test]
    fn music_per_course_and_restart_rules() {
        let t = course_music("ccm", 2);
        assert_eq!((t.sequence, t.preset, t.terrain, t.level), (9, 1, 6, 5));
        assert_eq!(course_music("nonexistent", 1).level, 9);
        let (mut s, rx) = sfx();
        s.start_music(course_music("bob", 1));
        let sent: Vec<Cmd> = rx.try_iter().collect();
        assert!(sent.iter().any(|c| matches!(c, Cmd::Track { sequence: 3, preset: 0 })));
        s.start_music(course_music("bob", 1));
        assert!(rx.try_recv().is_err(), "same track must not restart");
        s.stop_music();
        assert_eq!(rx.try_recv().unwrap(), Cmd::Stop);
        s.set_music_location(1, 0, [1.0, 2.0, 3.0], 5);
        assert!(rx.try_recv().is_err(), "no location without music");
    }

    #[test]
    fn mute_zeroes_gain() {
        let (tx, _rx) = channel();
        let g = MasterGain::new(1.0);
        let mut s = Sfx::new(tx, g.clone(), false);
        assert_eq!(g.get(), MASTER_VOLUME);
        s.set_muted(true);
        assert_eq!(g.get(), 0.0);
    }
}
