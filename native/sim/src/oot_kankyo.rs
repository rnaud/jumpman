//! Ocarina of Time's time of day: the clock (`gSaveContext.save.dayTime`, `gTimeSpeed`) and the time-based environment lights
//! of `z_kankyo.c` (`Environment_UpdateLights`, `LIGHT_MODE_TIME`): the scene's `EnvLightSettings` list is blended between its
//! night / dawn / day / dusk entries by `sTimeBasedLightConfigs`, and the sun and moon directions follow the clock.
//!
//! Engine-free. The settings come from the scene header (`scripts/export-oot-enemies.py`, key `environment`).
//!
//! Not ported: the config change blend (`changeLightEnabled`, used by a few actors), weather and lightning overrides,
//! the skybox palette blends (`gTimeBasedSkyboxConfigs`; the sky colour here is derived from the fog colour), Sun's Song.
use serde::Deserialize;

/// `CLOCK_TIME(hr, min)`.
pub fn clock_time(hour: u32, min: u32) -> u16 {
    (((hour * 60 + min) as f32) * 65536.0 / (24.0 * 60.0) + 0.5) as u16
}

/// The time a new file starts at (`Sram_InitSave`): 10:00, the daytime sample the scene vertex colours are baked at.
pub fn start_time() -> u16 {
    clock_time(10, 0)
}

/// One `EnvLightSettings` of a scene (22 bytes in the ROM).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LightSetting {
    pub ambient: [u8; 3],
    pub light1_dir: [i8; 3],
    pub light1_color: [u8; 3],
    pub light2_dir: [i8; 3],
    pub light2_color: [u8; 3],
    pub fog_color: [u8; 3],
    #[serde(default)]
    pub blend_rate: u8,
    pub fog_near: i16,
    pub z_far: i16,
}

/// The `environment` block of a scene's placement file.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    /// 0 = `LIGHT_MODE_TIME`: the lights follow the clock; 1 = `LIGHT_MODE_SETTINGS`: a fixed setting (indoors).
    #[serde(default)]
    pub light_mode: u8,
    #[serde(default)]
    pub skybox_id: u8,
    #[serde(default)]
    pub skybox_config: u8,
    #[serde(default)]
    pub lights: Vec<LightSetting>,
    /// `SCENE_CMD_SOUND_SETTINGS`: the background music sequence and the nature ambience of the scene.
    #[serde(default)]
    pub music: SceneMusic,
}

/// The `SoundSettings` scene command (`play->sceneSequences`).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct SceneMusic {
    /// `seqId`: an `NA_BGM_*` index; 255 when the scene has none.
    pub seq: u8,
    pub nature: u8,
}

impl Default for SceneMusic {
    fn default() -> Self {
        Self { seq: 255, nature: 255 }
    }
}

/// `sTimeBasedLightConfigs[config][phase]`: (start, end, light setting, next light setting) with the setting numbers
/// relative to the config's block of four (night = 3, dawn = 0, day = 1, dusk = 2).
pub fn phases() -> [(u16, u16, usize, usize); 7] {
    let t = clock_time;
    [
        (t(0, 0), t(4, 0) + 1, 3, 3),
        (t(4, 0) + 1, t(6, 0), 3, 0),
        (t(6, 0), t(8, 0) + 1, 0, 1),
        (t(8, 0) + 1, t(16, 0), 1, 1),
        (t(16, 0), t(17, 0) + 1, 1, 2),
        (t(17, 0) + 1, t(19, 0) + 1, 2, 3),
        // CLOCK_TIME(24, 0) is 0x10000 as an s32: the last phase ends at 0xFFFF.
        (t(19, 0) + 1, 0xFFFF, 3, 3),
    ]
}

/// First light setting of each `lightConfig` (0..=4): configs 0..3 use settings 4c..4c+3, config 4 uses 20..23.
pub fn config_base(config: usize) -> usize {
    [0, 4, 8, 12, 20][config.min(4)]
}

/// `Environment_LerpWeight(max, min, val)`.
pub fn lerp_weight(max: u16, min: u16, val: u16) -> f32 {
    let diff = max as f32 - min as f32;
    if diff != 0.0 {
        let ret = 1.0 - (max as f32 - val as f32) / diff;
        if ret < 1.0 {
            return ret;
        }
    }
    1.0
}

/// The result of the blend, in the units of `LightContext`: colours 0..255, directions as the game stores them.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvLights {
    pub ambient: [u8; 3],
    pub sun_dir: [f32; 3],
    pub sun_color: [u8; 3],
    pub moon_dir: [f32; 3],
    pub moon_color: [u8; 3],
    pub fog_color: [u8; 3],
    pub fog_near: i16,
    pub z_far: i16,
}

fn lerp8(a: u8, b: u8, t: f32) -> u8 {
    ((b as f32 - a as f32) * t + a as f32) as u8
}

fn lerp16(a: i16, b: i16, t: f32) -> i16 {
    ((b as f32 - a as f32) * t + a as f32) as i16
}

fn mix3(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [lerp8(a[0], b[0], t), lerp8(a[1], b[1], t), lerp8(a[2], b[2], t)]
}

/// `Math_SinS` / `Math_CosS` of an s16 angle.
fn sins(a: i16) -> f32 {
    (a as f32 * std::f32::consts::PI / 32768.0).sin()
}
fn coss(a: i16) -> f32 {
    (a as f32 * std::f32::consts::PI / 32768.0).cos()
}

/// The sun's direction at `time` (`light1Dir`); the moon is the opposite.
pub fn sun_dir(time: u16) -> [f32; 3] {
    let a = (time as i32 - clock_time(12, 0) as i32) as i16;
    [-(sins(a) * 120.0), coss(a) * 120.0, coss(a) * 20.0]
}

/// `Environment_UpdateLights` for `LIGHT_MODE_TIME` without a config change: blend the settings of `config` for `time`.
pub fn lights_at(list: &[LightSetting], config: usize, time: u16) -> Option<EnvLights> {
    let base = config_base(config);
    for (start, end, a, b) in phases() {
        if time >= start && (time < end || end == 0xFFFF) {
            let (x, y) = (list.get(base + a)?, list.get(base + b)?);
            let t = lerp_weight(end, start, time);
            let sun = sun_dir(time);
            return Some(EnvLights {
                ambient: mix3(x.ambient, y.ambient, t),
                sun_dir: sun,
                sun_color: mix3(x.light1_color, y.light1_color, t),
                moon_dir: [-sun[0], -sun[1], -sun[2]],
                moon_color: mix3(x.light2_color, y.light2_color, t),
                fog_color: mix3(x.fog_color, y.fog_color, t),
                fog_near: lerp16(x.fog_near, y.fog_near, t),
                z_far: lerp16(x.z_far, y.z_far, t),
            });
        }
    }
    None
}

/// A fixed setting (`LIGHT_MODE_SETTINGS`, indoors): the setting itself, sun direction from its own `light1Dir`.
pub fn fixed_lights(list: &[LightSetting], index: usize) -> Option<EnvLights> {
    let s = list.get(index)?;
    let f = |d: [i8; 3]| [d[0] as f32, d[1] as f32, d[2] as f32];
    Some(EnvLights {
        ambient: s.ambient,
        sun_dir: f(s.light1_dir),
        sun_color: s.light1_color,
        moon_dir: f(s.light2_dir),
        moon_color: s.light2_color,
        fog_color: s.fog_color,
        fog_near: s.fog_near,
        z_far: s.z_far,
    })
}

/// The clock: `dayTime` advances by `gTimeSpeed` per 20 Hz game update (twice as fast at night, `Environment_Update`).
#[derive(Clone, Debug)]
pub struct Clock {
    pub time: u16,
    /// `gTimeSpeed` (the retail value outdoors is 10: a day is about 5.5 minutes of daylight and night together).
    pub speed: u16,
    accumulator: f64,
    /// Sun's Song in progress (`SUNSSONG_SPEED_TIME`): the speed to restore and whether it began in daylight.
    suns: Option<(u16, bool)>,
}

impl Default for Clock {
    fn default() -> Self {
        Self { time: start_time(), speed: 10, accumulator: 0.0, suns: None }
    }
}

impl Clock {
    pub fn at(hour: u32, min: u32) -> Self {
        Self { time: clock_time(hour, min), ..Default::default() }
    }

    /// `nightFlag`: set between 18:00 and 06:30 (`Environment_Update`).
    pub fn is_night(&self) -> bool {
        self.time > clock_time(18, 0) || self.time < clock_time(6, 30)
    }

    /// One game update (20 Hz).
    pub fn update(&mut self) {
        let add = if self.is_night() && self.speed < 400 { self.speed as u32 * 2 } else { self.speed as u32 };
        self.time = self.time.wrapping_add(add as u16);
        if let Some((prev, was_day)) = self.suns {
            // `Interface_Update`: the song ends when the sun has passed dusk (started by day) or day has come (started at night).
            let daylight = self.time >= clock_time(6, 30) && self.time <= clock_time(18, 0) + 1;
            let done = if was_day { self.time > clock_time(18, 0) + 1 } else { daylight };
            if done {
                self.speed = prev;
                self.suns = None;
            }
        }
    }

    /// Sun's Song (`Interface_Update`, `sunsSongState`). Where time moves the clock races at `gTimeSpeed` 400 until the
    /// opposite half of the day starts; where the scene's clock is frozen (`sceneTimeSpeed` 0) the time jumps to
    /// midnight / just past noon (`NEXT_TIME_NIGHT` / `NEXT_TIME_DAY`). Returns true when it was daytime.
    pub fn suns_song(&mut self) -> bool {
        if self.suns.is_some() {
            return false;
        }
        let was_day = self.time >= clock_time(6, 30) && self.time < clock_time(18, 0) + 1;
        if self.speed == 0 {
            self.time = if was_day { clock_time(0, 0) } else { clock_time(12, 0) + 1 };
            return was_day;
        }
        self.suns = Some((self.speed, was_day));
        self.speed = 400;
        was_day
    }

    pub fn suns_song_running(&self) -> bool {
        self.suns.is_some()
    }

    /// Advance by wall-clock seconds.
    pub fn advance(&mut self, secs: f64) {
        self.accumulator += secs * 20.0;
        while self.accumulator >= 1.0 {
            self.accumulator -= 1.0;
            self.update();
        }
    }

    pub fn hours(&self) -> (u32, u32) {
        let minutes = (self.time as f64 * 24.0 * 60.0 / 65536.0) as u32;
        (minutes / 60, minutes % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(c: u8) -> LightSetting {
        LightSetting { ambient: [c; 3], light1_color: [c; 3], light2_color: [c / 2; 3], fog_color: [c; 3], fog_near: c as i16, z_far: 1000 + c as i16, ..Default::default() }
    }

    /// Settings 0..3 = dawn, day, dusk, night get 40 / 200 / 120 / 10.
    fn list() -> Vec<LightSetting> {
        vec![setting(40), setting(200), setting(120), setting(10)]
    }

    #[test]
    fn suns_song_runs_the_clock_to_dusk_by_day_and_to_morning_by_night() {
        let mut c = Clock::at(10, 0);
        assert!(c.suns_song());
        assert_eq!(c.speed, 400);
        assert!(c.suns_song_running());
        let mut updates = 0;
        while c.suns_song_running() {
            c.update();
            updates += 1;
            assert!(updates < 1000);
        }
        assert_eq!(c.speed, 10, "the speed is restored");
        assert!(c.time > clock_time(18, 0) && c.is_night(), "{:?}", c.hours());
        assert!(updates < 100, "{updates} updates: a few seconds at 20 Hz");
        // at night it runs until daylight
        assert!(!c.suns_song());
        while c.suns_song_running() {
            c.update();
        }
        assert!(!c.is_night() && c.time >= clock_time(6, 30) && c.time < clock_time(8, 0), "{:?}", c.hours());
        // a frozen clock (indoors / fixed scenes) jumps
        let mut f = Clock::at(10, 0);
        f.speed = 0;
        assert!(f.suns_song());
        assert_eq!(f.time, clock_time(0, 0));
        assert!(!f.suns_song());
        assert_eq!(f.time, clock_time(12, 0) + 1);
    }

    #[test]
    fn clock_time_matches_the_decomp_macro() {
        assert_eq!(clock_time(12, 0), 0x8000);
        assert_eq!(clock_time(0, 0), 0);
        assert_eq!(clock_time(6, 0), 0x4000);
        assert_eq!(start_time(), 27307);
    }

    #[test]
    fn day_uses_the_day_setting_and_night_the_night_setting() {
        let l = list();
        let day = lights_at(&l, 0, clock_time(12, 0)).unwrap();
        assert_eq!(day.ambient, [200; 3]);
        let night = lights_at(&l, 0, clock_time(1, 0)).unwrap();
        assert_eq!(night.ambient, [10; 3]);
        assert_eq!(night.z_far, 1010);
    }

    #[test]
    fn dawn_blends_night_to_dawn_to_day() {
        let l = list();
        // 4:00 + 1 .. 6:00: night -> dawn; at 5:00 about half way (10 -> 40).
        let a = lights_at(&l, 0, clock_time(5, 0)).unwrap().ambient[0];
        assert!((24..=26).contains(&a), "{a}");
        // 6:00 .. 8:00: dawn -> day; 7:00 halfway (40 -> 200).
        let b = lights_at(&l, 0, clock_time(7, 0)).unwrap().ambient[0];
        assert!((118..=122).contains(&b), "{b}");
        // dusk 17:00.. 19:00: dusk -> night
        let c = lights_at(&l, 0, clock_time(18, 0)).unwrap().ambient[0];
        assert!((60..=70).contains(&c), "{c}");
    }

    #[test]
    fn every_minute_of_the_day_has_a_phase_and_the_blend_is_continuous() {
        let l = list();
        let mut prev = lights_at(&l, 0, 0).unwrap().ambient[0] as i32;
        for t in 0..65535u32 {
            let cur = lights_at(&l, 0, t as u16).unwrap_or_else(|| panic!("no phase at {t}")).ambient[0] as i32;
            assert!((cur - prev).abs() <= 3, "jump at {t}: {prev} -> {cur}");
            prev = cur;
        }
    }

    #[test]
    fn the_sun_is_overhead_at_noon_and_opposite_at_midnight() {
        let noon = sun_dir(clock_time(12, 0));
        assert!(noon[0].abs() < 0.01 && (noon[1] - 120.0).abs() < 0.01 && (noon[2] - 20.0).abs() < 0.01);
        let midnight = sun_dir(0);
        assert!((midnight[1] + 120.0).abs() < 0.01);
        // morning sun in the east (+x in game space = -sin of a negative angle)
        assert!(sun_dir(clock_time(8, 0))[0] > 0.0 && sun_dir(clock_time(16, 0))[0] < 0.0);
        let l = lights_at(&list(), 0, clock_time(9, 0)).unwrap();
        assert_eq!(l.moon_dir, [-l.sun_dir[0], -l.sun_dir[1], -l.sun_dir[2]]);
    }

    #[test]
    fn config_blocks_follow_the_table() {
        assert_eq!((0..5).map(config_base).collect::<Vec<_>>(), vec![0, 4, 8, 12, 20]);
        let mut l = list();
        l.extend(list().into_iter().map(|mut s| { s.ambient = [99; 3]; s }));
        assert_eq!(lights_at(&l, 1, clock_time(12, 0)).unwrap().ambient, [99; 3]);
    }

    #[test]
    fn the_clock_runs_twice_as_fast_at_night_and_wraps() {
        let mut c = Clock::at(12, 0);
        c.update();
        assert_eq!(c.time, 0x8000 + 10);
        let mut n = Clock::at(0, 0);
        n.update();
        assert_eq!(n.time, 20);
        let mut w = Clock { time: 65530, speed: 10, accumulator: 0.0, suns: None };
        w.update();
        assert_eq!(w.time, 65530u16.wrapping_add(20));
        // one second is 20 updates
        let mut d = Clock::at(12, 0);
        d.advance(1.0);
        assert_eq!(d.time, 0x8000 + 200);
        assert_eq!(Clock::at(13, 30).hours(), (13, 30));
    }

    #[test]
    fn fixed_settings_keep_their_own_directions() {
        let mut s = setting(50);
        s.light1_dir = [73, 73, 73];
        let f = fixed_lights(&[s], 0).unwrap();
        assert_eq!(f.sun_dir, [73.0, 73.0, 73.0]);
        assert!(fixed_lights(&[], 0).is_none());
    }
}
