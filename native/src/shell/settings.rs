//! Persisted user settings (`displaySettings.ts` + the pause-menu toggles), stored as JSON in the
//! platform config directory (`~/Library/Application Support/Jumpman/settings.json` on macOS).
//! Unknown or invalid values fall back per field so an old or hand-edited file never breaks launch.
use crate::fx::display as fxd;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

macro_rules! choice_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $id:literal => $label:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $id)] $variant),+ }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub fn id(self) -> &'static str { match self { $($name::$variant => $id),+ } }
            pub fn label(self) -> &'static str { match self { $($name::$variant => $label),+ } }
            /// Next (or previous) choice, wrapping.
            pub fn cycle(self, dir: i32) -> Self {
                let i = Self::ALL.iter().position(|c| *c == self).unwrap_or(0) as i32;
                Self::ALL[(i + dir).rem_euclid(Self::ALL.len() as i32) as usize]
            }
        }
    };
}

choice_enum!(Resolution { Low = "240" => "320 x 240 - low", Standard = "480" => "640 x 480 - standard", Native = "native" => "Native display" });
choice_enum!(Aspect { Classic = "classic" => "4:3 - classic", Wide = "wide" => "Fit window - widescreen" });
choice_enum!(Look { Classic = "classic" => "Classic - simple", Modern = "modern" => "Modern - filmic" });
choice_enum!(Motion { Stepped = "stepped" => "30 Hz steps", Smooth = "smooth" => "Smooth interpolation" });
choice_enum!(Scaling { Soft = "soft" => "Soft", Sharp = "sharp" => "Sharp pixels" });

// Camera behaviour: `Original` is the SM64 `camera.c` port (Lakitu, area cameras, C buttons) for Mario in SM64 courses; `FreeOrbit` keeps the modern orbit camera.
choice_enum!(CameraStyle { Original = "original" => "Original (Lakitu)", FreeOrbit = "orbit" => "Free orbit" });

// Lighting quality tier. RayTraced is hardware ray tracing (bevy_solari); it is only reachable when the GPU supports it (see `lighting.rs`).
choice_enum!(Lighting { Standard = "standard" => "Off", Enhanced = "enhanced" => "On - SSAO, soft shadows", RayTraced = "raytraced" => "Ray traced (hardware, restart)" });

/// Mirror of `DisplaySettings` in `displaySettings.ts`. Published as a resource so the renderer
/// can honour it (`aspect` is applied by the shell itself).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub resolution: Resolution,
    pub aspect: Aspect,
    pub look: Look,
    pub motion: Motion,
    pub scaling: Scaling,
}

impl DisplaySettings {
    /// `CLASSIC` in `displaySettings.ts`.
    pub const CLASSIC: Self = Self { resolution: Resolution::Standard, aspect: Aspect::Classic, look: Look::Classic, motion: Motion::Smooth, scaling: Scaling::Soft };
    pub const MODERN: Self = Self { resolution: Resolution::Native, aspect: Aspect::Wide, look: Look::Modern, motion: Motion::Smooth, scaling: Scaling::Soft };
}

/// The native default is the modern preset (as `fx::display`).
impl Default for DisplaySettings {
    fn default() -> Self {
        Self::MODERN
    }
}

impl DisplaySettings {
    /// Push into the renderer-facing resource owned by `fx::display`.
    pub fn apply_to_fx(&self, fx: &mut fxd::DisplaySettings) {
        fx.set("resolution", self.resolution.id());
        fx.set("aspect", self.aspect.id());
        fx.set("look", self.look.id());
        fx.set("motion", self.motion.id());
        fx.set("scaling", self.scaling.id());
    }

    pub fn from_fx(fx: &fxd::DisplaySettings) -> Self {
        Self {
            resolution: match fx.resolution {
                fxd::Resolution::R240 => Resolution::Low,
                fxd::Resolution::R480 => Resolution::Standard,
                fxd::Resolution::Native => Resolution::Native,
            },
            aspect: if fx.aspect == fxd::Aspect::Classic { Aspect::Classic } else { Aspect::Wide },
            look: if fx.look == fxd::Look::Classic { Look::Classic } else { Look::Modern },
            motion: if fx.motion == fxd::Motion::Stepped { Motion::Stepped } else { Motion::Smooth },
            scaling: if fx.scaling == fxd::Scaling::Sharp { Scaling::Sharp } else { Scaling::Soft },
        }
    }
}

/// Everything the shell persists between launches.
#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub display: DisplaySettings,
    /// "Mute game audio" (the audio plugin reads this).
    pub muted: bool,
    /// Character id (`sm64`, `link`, `young-link`, `banjo`).
    pub character: String,
    /// Last course started from the menu.
    pub course: String,
    pub act: i32,
    /// Performance overlay (FPS, frame time, sim tick rate). Applies live.
    pub show_fps: bool,
    /// Uncapped frame rate: no vsync on desktop, the 120 Hz request on ProMotion iPhones. The 30 Hz simulation is unaffected.
    pub uncap_fps: bool,
    /// "Enhanced lighting" (screen-space ambient occlusion + soft shadows + FXAA). Not hardware ray tracing.
    pub lighting: Lighting,
    /// "Camera: Original / Free orbit" (default Original).
    pub camera: CameraStyle,
    /// "Classic SM64 extras": the castle-grounds Peach letter and warp-pipe jump (`ACT_INTRO_CUTSCENE`), the full credits
    /// roll at the end, the cap pickup cutscene and getting stuck in snow. Off by default: the shell menus stay the way in.
    pub classic_intro: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { display: DisplaySettings::default(), muted: false, character: "sm64".into(), course: "castle_grounds".into(), act: 1, show_fps: false, uncap_fps: false, lighting: Lighting::Standard, camera: CameraStyle::Original, classic_intro: false }
    }
}

impl Settings {
    /// Parse leniently: every field is validated on its own, bad ones keep their default.
    pub fn from_json(text: &str) -> Self {
        let mut out = Self::default();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else { return out };
        let get = |v: &serde_json::Value, k: &str| v.get(k).cloned();
        if let Some(d) = value.get("display") {
            macro_rules! field {
                ($f:ident) => {
                    if let Some(v) = get(d, stringify!($f)).and_then(|v| serde_json::from_value(v).ok()) {
                        out.display.$f = v;
                    }
                };
            }
            field!(resolution);
            field!(aspect);
            field!(look);
            field!(motion);
            field!(scaling);
        }
        if let Some(b) = get(&value, "muted").and_then(|v| v.as_bool()) {
            out.muted = b;
        }
        if let Some(s) = get(&value, "character").and_then(|v| v.as_str().map(str::to_owned)) {
            if super::Character::from_id(&s).is_some() {
                out.character = s;
            }
        }
        if let Some(s) = get(&value, "course").and_then(|v| v.as_str().map(str::to_owned)) {
            if crate::game::courses::info(&s).is_some() {
                out.course = s;
            }
        }
        if let Some(a) = get(&value, "act").and_then(|v| v.as_i64()) {
            out.act = (a as i32).clamp(1, 6);
        }
        if let Some(b) = get(&value, "show_fps").and_then(|v| v.as_bool()) {
            out.show_fps = b;
        }
        if let Some(b) = get(&value, "uncap_fps").and_then(|v| v.as_bool()) {
            out.uncap_fps = b;
        }
        if let Some(b) = get(&value, "classic_intro").and_then(|v| v.as_bool()) {
            out.classic_intro = b;
        }
        if let Some(l) = get(&value, "lighting").and_then(|v| serde_json::from_value(v).ok()) {
            out.lighting = l;
        }
        if let Some(c) = get(&value, "camera").and_then(|v| serde_json::from_value(v).ok()) {
            out.camera = c;
        }
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|t| Self::from_json(&t)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_json())
    }
}

/// Where `settings.json` lives. `JUMPMAN_CONFIG_DIR` overrides (tests, headless runs).
pub fn config_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("JUMPMAN_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir).join("settings.json");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(any(target_os = "macos", target_os = "ios")) {
        home.map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("Jumpman").join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings { display: DisplaySettings::MODERN, muted: true, character: "banjo".into(), course: "bob".into(), act: 3, show_fps: true, uncap_fps: true, lighting: Lighting::Enhanced, camera: CameraStyle::FreeOrbit, classic_intro: true };
        assert_eq!(Settings::from_json(&s.to_json()), s);
    }

    #[test]
    fn invalid_fields_fall_back_individually() {
        let s = Settings::from_json(r#"{"display":{"resolution":"9000","look":"classic"},"muted":"yes","character":"wario","course":"nope","act":99}"#);
        assert_eq!(s.display.resolution, Resolution::Native);
        assert_eq!(s.display.look, Look::Classic);
        assert!(!s.muted);
        assert_eq!(s.character, "sm64");
        assert_eq!(s.course, "castle_grounds");
        assert_eq!(s.act, 6);
        assert_eq!(Settings::from_json("not json"), Settings::default());
    }

    #[test]
    fn round_trips_through_the_fx_resource() {
        for d in [DisplaySettings::CLASSIC, DisplaySettings::MODERN, DisplaySettings { motion: Motion::Stepped, scaling: Scaling::Sharp, resolution: Resolution::Low, ..DisplaySettings::MODERN }] {
            let mut fx = fxd::DisplaySettings::MODERN;
            d.apply_to_fx(&mut fx);
            assert_eq!(DisplaySettings::from_fx(&fx), d);
        }
    }

    #[test]
    fn performance_fields_default_off_and_survive_bad_values() {
        let d = Settings::from_json("{}");
        assert!(!d.show_fps && !d.uncap_fps && d.lighting == Lighting::Standard);
        let b = Settings::from_json(r#"{"show_fps":"yes","uncap_fps":1,"lighting":"bogus"}"#);
        assert!(!b.show_fps && !b.uncap_fps && b.lighting == Lighting::Standard);
        let ok = Settings::from_json(r#"{"show_fps":true,"uncap_fps":true,"lighting":"enhanced"}"#);
        assert!(ok.show_fps && ok.uncap_fps && ok.lighting == Lighting::Enhanced);
    }

    #[test]
    fn cycling_wraps_both_ways() {
        assert_eq!(Resolution::Native.cycle(1), Resolution::Low);
        assert_eq!(Resolution::Low.cycle(-1), Resolution::Native);
        assert_eq!(Aspect::Classic.id(), "classic");
    }

    #[test]
    fn saves_and_loads_from_disk() {
        let dir = std::env::temp_dir().join(format!("jumpman-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings { muted: true, ..Settings::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        assert_eq!(Settings::load(&dir.join("missing.json")), Settings::default());
        let _ = std::fs::remove_dir_all(dir);
    }
}
