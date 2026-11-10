//! Performance toggles: the FPS overlay (`Settings::show_fps`) and the frame-rate cap (`Settings::uncap_fps`).
//!
//! * Capped (default): `PresentMode::AutoVsync` (Fifo), the display's refresh rate. On iPhone the app asks for 60 Hz.
//! * Uncapped: `PresentMode::AutoNoVsync` (Immediate, else Mailbox) on desktop; wgpu's Metal backend can only turn
//!   display sync off on macOS, so on iOS uncapped instead requests the maximum (120 Hz ProMotion) refresh rate
//!   through `CADisplayLink.preferredFrameRateRange` (`ios/Sources/refresh.m`). The winit frame loop (continuous
//!   while focused) is untouched.
//! * Simulation is a fixed 30 Hz step (`Time<Fixed>`, see `sim_plugin.rs`) with interpolated rendering, so the frame rate
//!   never changes game speed; `fixed_step_count_is_independent_of_frame_rate` below pins that.
//!
//! Test hooks (override the settings, never persisted): `JUMPMAN_SHOW_FPS=1`, `JUMPMAN_UNCAP=1`
//! (`JUMPMAN_NOVSYNC=1` is the old alias), and `JUMPMAN_FPS=1` prints the average once per two seconds.
use crate::button_marker::BorderRadius;
use crate::safe_area::SafeArea;
use crate::shell::settings::Settings;
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};
use bevy::winit::{UpdateMode, WinitSettings};

fn env_flag(name: &str) -> Option<bool> {
    std::env::var(name).ok().filter(|v| !v.is_empty()).map(|v| !matches!(v.as_str(), "0" | "off" | "false"))
}

pub fn show_fps(s: &Settings) -> bool {
    env_flag("JUMPMAN_SHOW_FPS").unwrap_or(s.show_fps)
}

pub fn uncapped(s: &Settings) -> bool {
    env_flag("JUMPMAN_UNCAP").or_else(|| env_flag("JUMPMAN_NOVSYNC")).unwrap_or(s.uncap_fps)
}

/// Rolling statistics over ~1 s of real time.
#[derive(Resource, Default, Debug, Clone)]
pub struct FpsStats {
    pub fps: f32,
    pub avg_ms: f32,
    pub worst_ms: f32,
    /// Fixed simulation steps per real second.
    pub tick_hz: f32,
    frames: u32,
    ticks: u32,
    elapsed: f32,
    worst: f32,
}

impl FpsStats {
    pub fn record(&mut self, dt: f32) {
        self.frames += 1;
        self.elapsed += dt;
        self.worst = self.worst.max(dt);
        if self.elapsed >= 1.0 {
            self.fps = self.frames as f32 / self.elapsed;
            self.avg_ms = 1000.0 * self.elapsed / self.frames as f32;
            self.worst_ms = 1000.0 * self.worst;
            self.tick_hz = self.ticks as f32 / self.elapsed;
            self.frames = 0;
            self.ticks = 0;
            self.elapsed = 0.0;
            self.worst = 0.0;
        }
    }
}

#[derive(Component)]
struct FpsRoot;
#[derive(Component)]
struct FpsText;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FpsStats>()
            .add_systems(Startup, spawn_overlay)
            .add_systems(FixedFirst, |mut s: ResMut<FpsStats>| s.ticks += 1)
            .add_systems(Update, (collect, update_overlay, apply_present_mode, apply_refresh_request).chain());
    }
}

fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            FpsRoot,
            Visibility::Hidden,
            GlobalZIndex(1100),
            Node { position_type: PositionType::Absolute, top: Val::Px(38.0), left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-100.0)), width: Val::Px(200.0), padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)), justify_content: JustifyContent::Center, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            BorderRadius::all(Val::Px(6.0)),
        ))
        .with_children(|p| {
            p.spawn((FpsText, Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextLayout::no_wrap(), TextColor(Color::srgb(0.85, 1.0, 0.85))));
        });
}

fn collect(time: Res<Time<Real>>, mut stats: ResMut<FpsStats>, mut acc: Local<(f32, u32, f32)>, all: Query<Entity>) {
    let dt = time.delta_secs();
    stats.record(dt);
    if std::env::var_os("JUMPMAN_FPS").is_some() {
        acc.0 += dt;
        acc.1 += 1;
        acc.2 = acc.2.max(dt);
        if acc.0 >= 2.0 {
            println!("fps {:.1} ({:.1} ms/frame, worst {:.0} ms, sim {:.1} Hz, {} entities, {} MB rss)", acc.1 as f32 / acc.0, 1000.0 * acc.0 / acc.1 as f32, 1000.0 * acc.2, stats.tick_hz, all.iter().count(), rss_mb());
            *acc = (0.0, 0, 0.0);
        }
    }
}

/// Resident memory in MB (leak hunting with `JUMPMAN_FPS=1`; 0 when `ps` is unavailable).
fn rss_mb() -> u64 {
    std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok()).map_or(0, |kb| kb / 1024)
}

fn update_overlay(
    settings: Res<Settings>,
    stats: Res<FpsStats>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut root: Query<(&mut Visibility, &mut Node), With<FpsRoot>>,
    mut text: Query<&mut Text, With<FpsText>>,
) {
    let (Ok((mut vis, mut node)), Ok(mut text)) = (root.single_mut(), text.single_mut()) else { return };
    let want = if show_fps(&settings) { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if want == Visibility::Hidden {
        return;
    }
    // Top-centre inside the safe area; the HUD sits in the corners and the touch controls at the bottom.
    // Desktop sits below the course title, phones (short landscape windows) at the very top.
    let short = windows.single().is_ok_and(|w| w.height() < 500.0);
    let top = Val::Px(if short { 3.0 } else { 38.0 } + area.top);
    if node.top != top {
        node.top = top;
    }
    let s = format!("{:.0} FPS  {:.1} ms  sim {:.0} Hz", stats.fps, stats.avg_ms, stats.tick_hz);
    if text.0 != s {
        text.0 = s;
    }
}

fn apply_present_mode(settings: Res<Settings>, mut windows: Query<&mut Window, With<PrimaryWindow>>, mut winit: ResMut<WinitSettings>) {
    let Ok(mut window) = windows.single_mut() else { return };
    let uncap = uncapped(&settings);
    let want = if uncap { PresentMode::AutoNoVsync } else { PresentMode::AutoVsync };
    if window.present_mode != want {
        window.present_mode = want;
    }
    // Bevy's default throttles an unfocused window to ~60 updates/s (`reactive_low_power`); uncapped lifts that too.
    let mode = if uncap { UpdateMode::Continuous } else { WinitSettings::game().unfocused_mode };
    if winit.unfocused_mode != mode {
        winit.unfocused_mode = mode;
    }
}

#[cfg(target_os = "ios")]
unsafe extern "C" {
    /// `ios/Sources/refresh.m`: `1` asks for up to 120 Hz, `0` for 60 Hz. Main thread only.
    fn jm_set_refresh_uncapped(uncapped: i32);
}

/// Pinned to the main thread (UIKit) by the `NonSend` parameter.
#[cfg_attr(not(target_os = "ios"), allow(unused_variables))]
fn apply_refresh_request(settings: Res<Settings>, _main: Option<NonSend<crate::safe_area::MainThread>>, mut last: Local<Option<bool>>) {
    let want = uncapped(&settings);
    if *last == Some(want) {
        return;
    }
    *last = Some(want);
    #[cfg(target_os = "ios")]
    // SAFETY: plain C call into refresh.m, made from the main thread (NonSend resource).
    unsafe {
        jm_set_refresh_uncapped(want as i32)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Fixed steps run in `secs` of simulated time when frames arrive every `frame_ms`.
    fn steps(frame_ms: f32, secs: f32) -> u32 {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).insert_resource(Time::<Fixed>::from_hz(30.0)).insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(frame_ms / 1000.0)));
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = count.clone();
        app.add_systems(FixedUpdate, move || {
            c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        for _ in 0..(secs * 1000.0 / frame_ms).round() as u32 {
            app.update();
        }
        count.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn fixed_step_count_is_independent_of_frame_rate() {
        // 4 s of play at 30 Hz is 120 steps (+-3 for the zero-length first frame and the unspent remainder)
        // whether frames come at 240, 120, 60, 30 or 15 fps.
        for ms in [4.0, 8.0, 16.0, 33.0, 66.0] {
            let n = steps(ms, 4.0);
            assert!((117..=121).contains(&n), "{ms} ms frames ran {n} steps");
        }
    }

    #[test]
    fn stats_report_rate_and_worst_frame() {
        let mut s = FpsStats::default();
        for _ in 0..59 {
            s.record(1.0 / 60.0);
        }
        s.record(0.1);
        assert!((s.fps - 60.0).abs() < 8.0 && s.worst_ms > 99.0 && s.avg_ms > 15.0);
    }
}
