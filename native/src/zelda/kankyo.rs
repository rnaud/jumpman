//! Time of day in the OoT courses: drives the sun / moon, ambient light, fog and sky dome of an outdoor scene from the
//! clock of `jumpman_sim::oot_kankyo` (`Environment_UpdateLights`).
//!
//! The scene's `EnvLightSettings` list is read from the course's placement file (`environment`, written by
//! `scripts/export-oot-enemies.py`). The terrain vertex colours were baked at 10:00, so the relighting is a change of the
//! Bevy lights the scene is drawn with, not a re-bake. Indoor scenes (`LIGHT_MODE_SETTINGS`) keep their fixed light.
//!
//! Test hooks: `JUMPMAN_TIME=HH:MM` starts the clock there, `JUMPMAN_TIME_SPEED=n` sets `gTimeSpeed` (retail 10; 0 freezes).
use crate::game::runner::GameSim;
use bevy::prelude::*;
use jumpman_sim::oot_kankyo::{self as kankyo, Clock, EnvLights, Environment};

#[derive(Resource)]
pub struct OotTime {
    pub clock: Clock,
    pub env: Environment,
    /// The course the environment was read for.
    pub course: String,
}

impl Default for OotTime {
    fn default() -> Self {
        let mut clock = Clock::default();
        if let Some((h, m)) = std::env::var("JUMPMAN_TIME").ok().and_then(|v| parse_hhmm(&v)) {
            clock = Clock::at(h, m);
        }
        if let Some(speed) = std::env::var("JUMPMAN_TIME_SPEED").ok().and_then(|v| v.parse().ok()) {
            clock.speed = speed;
        }
        Self { clock, env: Environment::default(), course: String::new() }
    }
}

pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

impl OotTime {
    /// The lights for the clock, or `None` when the scene has none to follow (indoors / no settings).
    pub fn lights(&self) -> Option<EnvLights> {
        if self.env.lights.is_empty() {
            return None;
        }
        if self.env.light_mode == 0 {
            kankyo::lights_at(&self.env.lights, 0, self.clock.time)
        } else {
            None
        }
    }
}

fn rgb(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

/// Sky dome colours: the clear-day blue by day, the fog colour at the horizon (a deeper version of it above) at dawn, dusk
/// and night; `day` (0..1) says how much of the day sky shows (the retail sky textures are not used).
pub fn sky_for(fog: [u8; 3], day: f32) -> crate::fx::sky::SkyPalette {
    let f = rgb(fog);
    let dusk = crate::fx::sky::SkyPalette { top: [f[0] * 0.45, f[1] * 0.45, f[2] * 0.55 + 0.04], mid: [f[0] * 0.8, f[1] * 0.8, f[2] * 0.85], bottom: f, clouds: 0.0 };
    let mut p = dusk;
    p.lerp_to(&crate::fx::sky::SkyPalette::default(), day.clamp(0.0, 1.0));
    p
}

/// How much of the clear-day sky shows for a sun colour: nothing below half brightness (dawn / dusk), all of it at noon.
pub fn day_weight(sun: [u8; 3]) -> f32 {
    ((luma(sun) - 0.5) * 2.0).clamp(0.0, 1.0)
}

/// Full-screen grade: the baked terrain colours are the 10:00 look, so the darker times of day dim the picture by the ratio
/// of ambient brightness to the 10:00 one.
#[derive(Component)]
struct Grade;

fn spawn_grade(mut commands: Commands) {
    commands.spawn((
        Grade,
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(0.02, 0.03, 0.12, 0.0)),
        GlobalZIndex(-20),
    ));
}

/// Alpha of the grade for the current lights against the 10:00 reference ambient.
pub fn grade_alpha(now: &EnvLights, reference: &EnvLights) -> f32 {
    let r = (luma(now.ambient) + 0.5 * luma(now.sun_color).max(luma(now.moon_color))) / (luma(reference.ambient) + 0.5 * luma(reference.sun_color).max(luma(reference.moon_color))).max(0.01);
    ((1.0 - r.min(1.0)) * 1.1).clamp(0.0, 0.78)
}

/// Brightness of a light colour (0..1).
fn luma(c: [u8; 3]) -> f32 {
    (c[0] as f32 * 0.3 + c[1] as f32 * 0.59 + c[2] as f32 * 0.11) / 255.0
}

/// `(sun colour, brightness, direction)`: the sun by day, the moon (the opposite direction) when it is the brighter light.
pub fn key_light(l: &EnvLights) -> ([u8; 3], [f32; 3]) {
    if luma(l.moon_color) > luma(l.sun_color) { (l.moon_color, l.moon_dir) } else { (l.sun_color, l.sun_dir) }
}

pub fn load_env(sim: Res<GameSim>, root: Res<crate::render::AssetRoot>, mut time: ResMut<OotTime>) {
    let req = &sim.course.request;
    if time.course == req.id {
        return;
    }
    time.course = req.id.clone();
    time.env = crate::zelda_enemies::load_placements(&root.0, &req.id, req.area).map(|p| p.environment).unwrap_or_default();
}

pub fn drive(
    real: Res<Time>,
    mut time: ResMut<OotTime>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform)>,
    mut palette: ResMut<crate::fx::sky::SkyPalette>,
    mut fog: Query<&mut bevy::pbr::DistanceFog, With<crate::camera::MainCamera>>,
    mut grade: Query<&mut BackgroundColor, With<Grade>>,
    fx: Option<ResMut<crate::link::WorldFx>>,
) {
    if let Some(mut fx) = fx {
        if std::mem::take(&mut fx.sun_song) {
            time.clock.suns_song();
        }
    }
    time.clock.advance(real.delta_secs_f64());
    let Some(l) = time.lights() else {
        for mut bg in &mut grade {
            bg.0 = Color::srgba(0.02, 0.03, 0.12, 0.0);
        }
        return;
    };
    let norm = |c: [u8; 3]| {
        let m = c[0].max(c[1]).max(c[2]).max(1) as f32;
        (Color::srgb(c[0] as f32 / m, c[1] as f32 / m, c[2] as f32 / m), m / 255.0)
    };
    let (ac, am) = norm(l.ambient);
    ambient.color = ac;
    ambient.brightness = 160.0 + 900.0 * am;
    let (kc, dir) = key_light(&l);
    let (kcol, km) = norm(kc);
    for (mut light, mut tf) in &mut sun {
        light.color = kcol;
        light.illuminance = 300.0 + 1900.0 * km * luma(kc).sqrt();
        let d = Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero();
        *tf = Transform::from_translation(d * 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    let day = day_weight(l.sun_color);
    let want = sky_for(l.fog_color, day);
    if let Some(reference) = kankyo::lights_at(&time.env.lights, 0, kankyo::start_time()) {
        let alpha = grade_alpha(&l, &reference);
        for mut bg in &mut grade {
            bg.0 = Color::srgba(0.02, 0.03, 0.12, alpha);
        }
    }
    palette.lerp_to(&want, 0.2);
    for mut f in &mut fog {
        f.color = Color::srgb(l.fog_color[0] as f32 / 255.0, l.fog_color[1] as f32 / 255.0, l.fog_color[2] as f32 / 255.0);
    }
}

pub struct KankyoPlugin;

impl Plugin for KankyoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OotTime>().init_resource::<super::music::OotMusic>().add_systems(Startup, spawn_grade.in_set(crate::progress::PersistentSpawn)).add_systems(Update, (load_env, drive, super::music::drive).chain().after(crate::fx::sky::update_environment));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::oot_kankyo::LightSetting;

    fn env() -> Environment {
        let s = |a: u8, sun: u8, moon: u8| LightSetting { ambient: [a; 3], light1_color: [sun; 3], light2_color: [moon; 3], fog_color: [a; 3], fog_near: 900, z_far: 1000, ..Default::default() };
        // dawn, day, dusk, night
        Environment { light_mode: 0, lights: vec![s(80, 180, 0), s(160, 255, 0), s(100, 200, 0), s(30, 0, 120)], ..Default::default() }
    }

    fn at(h: u32, m: u32) -> OotTime {
        OotTime { clock: Clock::at(h, m), env: env(), course: "x".into() }
    }

    #[test]
    fn hhmm_parses() {
        assert_eq!(parse_hhmm("22:30"), Some((22, 30)));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("x"), None);
    }

    #[test]
    fn noon_is_bright_with_the_sun_overhead_and_midnight_dark_with_the_moon() {
        let noon = at(12, 0).lights().unwrap();
        let (c, d) = key_light(&noon);
        assert!(c[0] > 200 && d[1] > 100.0, "{c:?} {d:?}");
        let night = at(1, 0).lights().unwrap();
        let (c, d) = key_light(&night);
        assert_eq!(c, [120; 3], "the moon lights the night");
        assert!(d[1] > 0.0, "the moon is above the horizon at 1 AM: {d:?}");
        assert!(luma(noon.ambient) > 3.0 * luma(night.ambient));
    }

    #[test]
    fn the_sky_follows_the_fog_and_darkens_at_night() {
        let day = sky_for([160; 3], 1.0);
        let night = sky_for([30; 3], 0.0);
        assert!(day.top[2] > night.top[2] && day.bottom[0] > night.bottom[0]);
        let dusk = sky_for([120, 70, 50], 0.0);
        assert!(dusk.bottom[0] > dusk.bottom[2], "an orange horizon at dusk");
        assert!(dusk.top[0] < dusk.bottom[0], "zenith deeper than the horizon");
    }

    #[test]
    fn the_grade_dims_the_night_and_leaves_ten_am_alone() {
        let reference = at(10, 0).lights().unwrap();
        assert!(grade_alpha(&reference, &reference) < 0.01);
        let night = at(1, 0).lights().unwrap();
        assert!(grade_alpha(&night, &reference) > 0.3);
        assert!(day_weight([255; 3]) > 0.99 && day_weight([20; 3]) == 0.0);
    }

    #[test]
    fn indoor_scenes_do_not_follow_the_clock() {
        let mut t = at(3, 0);
        t.env.light_mode = 1;
        assert!(t.lights().is_none());
        assert!(OotTime { clock: Clock::default(), env: Environment::default(), course: String::new() }.lights().is_none());
    }

    #[test]
    fn real_scene_lights_blend_through_a_day() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let Some(p) = crate::zelda_enemies::load_placements(&root, "hyrule_field", 1) else { return };
        let env = p.environment;
        assert_eq!(env.light_mode, 0);
        assert!(env.lights.len() >= 4, "{} settings", env.lights.len());
        let mut last = None;
        for t in (0..65536u32).step_by(64) {
            let l = kankyo::lights_at(&env.lights, 0, t as u16).expect("a phase");
            if let Some(prev) = last {
                let p: EnvLights = prev;
                for i in 0..3 {
                    assert!((p.ambient[i] as i32 - l.ambient[i] as i32).abs() < 40, "ambient jumps at {t}");
                }
            }
            last = Some(l);
        }
        let noon = kankyo::lights_at(&env.lights, 0, kankyo::clock_time(12, 0)).unwrap();
        let night = kankyo::lights_at(&env.lights, 0, 0).unwrap();
        assert_ne!(noon.fog_color, night.fog_color);
    }
}
