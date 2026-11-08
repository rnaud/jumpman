//! Display settings (`displaySettings.ts`): resolution / aspect / look / motion / scaling.
//! Read from env (`JUMPMAN_PRESET=classic|modern`, `JUMPMAN_LOOK=classic|modern`,
//! `JUMPMAN_RES=240|480|native`, `JUMPMAN_ASPECT=classic|wide`, `JUMPMAN_MOTION`, `JUMPMAN_SCALING`)
//! with the MODERN preset as the native default; F6 toggles the classic/modern look at runtime.
//! The look drives shadows (classic = blob shadows only, modern = sun shadow map).
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    R240,
    R480,
    Native,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Classic,
    Wide,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Classic,
    Modern,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Smooth,
    Stepped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    Soft,
    Sharp,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySettings {
    pub resolution: Resolution,
    pub aspect: Aspect,
    pub look: Look,
    pub motion: Motion,
    pub scaling: Scaling,
}

impl DisplaySettings {
    pub const CLASSIC: Self = Self { resolution: Resolution::R480, aspect: Aspect::Classic, look: Look::Classic, motion: Motion::Smooth, scaling: Scaling::Soft };
    pub const MODERN: Self = Self { resolution: Resolution::Native, aspect: Aspect::Wide, look: Look::Modern, motion: Motion::Smooth, scaling: Scaling::Soft };

    /// Apply one `key=value` choice (unknown values are ignored, like the TS `choices` check).
    pub fn set(&mut self, key: &str, value: &str) {
        match (key, value) {
            ("resolution", "240") => self.resolution = Resolution::R240,
            ("resolution", "480") => self.resolution = Resolution::R480,
            ("resolution", "native") => self.resolution = Resolution::Native,
            ("aspect", "classic") => self.aspect = Aspect::Classic,
            ("aspect", "wide") => self.aspect = Aspect::Wide,
            ("look", "classic") => self.look = Look::Classic,
            ("look", "modern") => self.look = Look::Modern,
            ("motion", "smooth") => self.motion = Motion::Smooth,
            ("motion", "stepped") => self.motion = Motion::Stepped,
            ("scaling", "soft") => self.scaling = Scaling::Soft,
            ("scaling", "sharp") => self.scaling = Scaling::Sharp,
            _ => {}
        }
    }

    pub fn from_env() -> Self {
        let mut s = match std::env::var("JUMPMAN_PRESET").as_deref() {
            Ok("classic") => Self::CLASSIC,
            _ => Self::MODERN,
        };
        for (env, key) in [("JUMPMAN_RES", "resolution"), ("JUMPMAN_ASPECT", "aspect"), ("JUMPMAN_LOOK", "look"), ("JUMPMAN_MOTION", "motion"), ("JUMPMAN_SCALING", "scaling")] {
            if let Ok(v) = std::env::var(env) {
                s.set(key, &v);
            }
        }
        s
    }
}

/// `displaySize`: letterboxed size (logical window pixels) and the internal render size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplaySize {
    pub css_width: f32,
    pub css_height: f32,
    pub width: u32,
    pub height: u32,
    pub aspect: f32,
}

pub fn display_size(s: &DisplaySettings, width: f32, height: f32, dpr: f32) -> DisplaySize {
    let aspect = if s.aspect == Aspect::Classic { 4.0 / 3.0 } else { width / height };
    let css_height = height.min(width / aspect);
    let css_width = css_height * aspect;
    let render_height = match s.resolution {
        Resolution::Native => (css_height * dpr.min(2.0)).round(),
        Resolution::R240 => 240.0,
        Resolution::R480 => 480.0,
    };
    DisplaySize { css_width, css_height, width: (render_height * aspect).round() as u32, height: render_height as u32, aspect }
}

pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DisplaySettings::from_env()).add_systems(Update, (toggle_look, apply_shadows).chain());
    }
}

fn toggle_look(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<DisplaySettings>) {
    if keys.just_pressed(KeyCode::F6) {
        settings.look = if settings.look == Look::Classic { Look::Modern } else { Look::Classic };
    }
}

/// `shadowMap.enabled = !classic`.
fn apply_shadows(settings: Res<DisplaySettings>, mut lights: Query<&mut DirectionalLight>) {
    for mut l in &mut lights {
        let on = settings.look == Look::Modern;
        if l.shadow_maps_enabled != on {
            l.shadow_maps_enabled = on;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_is_letterboxed_480() {
        let d = display_size(&DisplaySettings::CLASSIC, 1600.0, 800.0, 2.0);
        assert_eq!((d.width, d.height), (640, 480));
        assert!((d.css_width - 1066.6667).abs() < 0.01 && d.css_height == 800.0);
    }

    #[test]
    fn modern_is_native_wide() {
        let d = display_size(&DisplaySettings::MODERN, 1000.0, 500.0, 3.0);
        assert_eq!((d.width, d.height), (2000, 1000));
    }

    #[test]
    fn set_rejects_unknown() {
        let mut s = DisplaySettings::MODERN;
        s.set("look", "bogus");
        s.set("resolution", "240");
        assert_eq!((s.look, s.resolution), (Look::Modern, Resolution::R240));
    }
}
