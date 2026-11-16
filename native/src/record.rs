//! Developer-only frame recorder (trailers, bug clips). Off unless `JUMPMAN_RECORD=<dir>` is set; nothing here runs in
//! normal play.
//!
//! * `JUMPMAN_RECORD=<dir>`: from `JUMPMAN_RECORD_START` (game seconds, default 0) on, every rendered frame is read back
//!   and written to `<dir>/f_00000.rgba` (raw RGBA8, sRGB, tightly packed rows, the size of the offscreen image or the
//!   window). `<dir>/size.txt` says `WxH`. Pair it with `JUMPMAN_OFFSCREEN=1920x1080` for a fixed size.
//! * `JUMPMAN_RECORD_FPS=30` (default 30): once recording starts the clock no longer follows the wall clock: every frame
//!   advances game time by exactly `1/fps` (`TimeUpdateStrategy::ManualDuration`), so the 30 Hz sim runs one tick per
//!   frame at 30 fps and the footage is smooth however slow the readback is. Before the start the game runs normally
//!   (loading, scripted menu steps).
//! * `JUMPMAN_RECORD_LEN=<seconds>`: stop after that much game time, wait for the last frames to reach the disk and quit.
//! * `JUMPMAN_RECORD_CLEAN=1|ui|text`: capture only, hide overlays: `1`/`ui` hides every `bevy_ui` node (HUD, menus,
//!   toasts, FPS counter), `text` hides only text (`Text`, `TextSpan`, SM64 bitmap text) and keeps the HUD graphics.
//!   It forces the computed visibility in `Last`, after the game's own visibility updates, so the normal HUD code is
//!   untouched.
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The image an offscreen run (`JUMPMAN_OFFSCREEN`) renders into; the recorder captures it instead of the window.
#[derive(Resource, Clone)]
pub struct OffscreenTarget(pub Handle<Image>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clean {
    Off,
    /// Every UI node.
    Ui,
    /// Text only.
    Text,
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct RecordConfig {
    pub dir: PathBuf,
    pub fps: f64,
    pub start: f32,
    pub len: Option<f32>,
    pub clean: Clean,
}

impl RecordConfig {
    /// Reads the hooks through `get` (the environment in the game, a map in tests). `None` without `JUMPMAN_RECORD`.
    pub fn parse(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let dir = get("JUMPMAN_RECORD").filter(|d| !d.is_empty())?;
        let num = |k: &str| get(k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0);
        let clean = match get("JUMPMAN_RECORD_CLEAN").as_deref().map(str::trim) {
            None | Some("") | Some("0") | Some("off") => Clean::Off,
            Some("text") => Clean::Text,
            Some(_) => Clean::Ui,
        };
        Some(Self { dir: dir.into(), fps: num("JUMPMAN_RECORD_FPS").filter(|f| *f > 0.0).unwrap_or(30.0), start: num("JUMPMAN_RECORD_START").unwrap_or(0.0) as f32, len: num("JUMPMAN_RECORD_LEN").map(|l| l as f32), clean })
    }

    /// Frames to capture, or `None` for no limit.
    pub fn frames(&self) -> Option<usize> {
        self.len.map(|l| (l as f64 * self.fps).round() as usize)
    }
}

pub struct RecordPlugin(pub RecordConfig);

impl Plugin for RecordPlugin {
    fn build(&self, app: &mut App) {
        let _ = std::fs::create_dir_all(&self.0.dir);
        app.insert_resource(self.0.clone()).init_resource::<RecordState>().add_systems(Last, capture);
        if self.0.clean != Clean::Off {
            app.add_systems(Last, hide_overlays);
        }
    }
}

#[derive(Resource, Default)]
struct RecordState {
    started: bool,
    spawned: usize,
    saved: Arc<AtomicUsize>,
    /// Frames waited for the readbacks after the last capture.
    draining: u32,
}

fn capture(mut commands: Commands, config: Res<RecordConfig>, mut state: ResMut<RecordState>, time: Res<Time>, target: Option<Res<OffscreenTarget>>, mut strategy: ResMut<TimeUpdateStrategy>, mut exit: MessageWriter<AppExit>) {
    if !state.started {
        if time.elapsed_secs() < config.start {
            return;
        }
        state.started = true;
        // From the next frame on the clock advances exactly one frame of footage per rendered frame.
        *strategy = TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / config.fps));
        info!("recording to {} at {} fps", config.dir.display(), config.fps);
    }
    if config.frames().is_some_and(|n| state.spawned >= n) {
        state.draining += 1;
        if state.saved.load(Ordering::Relaxed) >= state.spawned || state.draining > 600 {
            info!("recorded {} frames", state.saved.load(Ordering::Relaxed));
            exit.write(AppExit::Success);
        }
        return;
    }
    let index = state.spawned;
    state.spawned += 1;
    let path = config.dir.join(format!("f_{index:05}.rgba"));
    let size_path = config.dir.join("size.txt");
    let saved = state.saved.clone();
    let shot = match target {
        Some(t) => Screenshot::image(t.0.clone()),
        None => Screenshot::primary_window(),
    };
    commands.spawn(shot).observe(move |captured: On<ScreenshotCaptured>| {
        let img = &captured.image;
        if let Some(data) = img.data.as_ref() {
            if index == 0 {
                let _ = std::fs::write(&size_path, format!("{}x{}", img.width(), img.height()));
            }
            if let Err(e) = std::fs::write(&path, data) {
                error!("record: {}: {e}", path.display());
            }
        }
        saved.fetch_add(1, Ordering::Relaxed);
    });
}

/// Capture-only: force the overlays' computed visibility off after the game has set its own.
fn hide_overlays(config: Res<RecordConfig>, mut nodes: Query<(&mut InheritedVisibility, &mut ViewVisibility, Has<Text>, Has<TextSpan>, Has<crate::smfont::SmText>), With<Node>>) {
    for (mut inherited, mut view, text, span, sm) in &mut nodes {
        if config.clean == Clean::Ui || text || span || sm {
            *inherited = InheritedVisibility::HIDDEN;
            *view = ViewVisibility::HIDDEN;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(pairs: &[(&str, &str)]) -> Option<RecordConfig> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        RecordConfig::parse(|k| map.get(k).cloned())
    }

    #[test]
    fn off_without_a_directory() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&[("JUMPMAN_RECORD", ""), ("JUMPMAN_RECORD_FPS", "60")]), None);
    }

    #[test]
    fn defaults_and_overrides() {
        let c = parse(&[("JUMPMAN_RECORD", "/tmp/x")]).unwrap();
        assert_eq!((c.fps, c.start, c.len, c.clean, c.frames()), (30.0, 0.0, None, Clean::Off, None));
        let c = parse(&[("JUMPMAN_RECORD", "/tmp/x"), ("JUMPMAN_RECORD_FPS", "24"), ("JUMPMAN_RECORD_START", "3.5"), ("JUMPMAN_RECORD_LEN", "2.5"), ("JUMPMAN_RECORD_CLEAN", "1")]).unwrap();
        assert_eq!((c.fps, c.start, c.len, c.clean, c.frames()), (24.0, 3.5, Some(2.5), Clean::Ui, Some(60)));
        assert_eq!(parse(&[("JUMPMAN_RECORD", "d"), ("JUMPMAN_RECORD_CLEAN", "text")]).unwrap().clean, Clean::Text);
        // Nonsense falls back to the defaults.
        let c = parse(&[("JUMPMAN_RECORD", "d"), ("JUMPMAN_RECORD_FPS", "0"), ("JUMPMAN_RECORD_START", "-2"), ("JUMPMAN_RECORD_CLEAN", "off")]).unwrap();
        assert_eq!((c.fps, c.start, c.clean), (30.0, 0.0, Clean::Off));
    }
}
