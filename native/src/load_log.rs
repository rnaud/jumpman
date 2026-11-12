//! `JUMPMAN_LOAD_LOG=1`: what is loaded, simulated and alive at each moment (see `native/LOADING_AUDIT.md`).
//!
//! One `[load-log]` stdout line per second and one on every app-state / course change or course load: app state, screen, the
//! course in `GameSim`, course roots (top-level `CourseScoped` entities) and all entities, loaded mesh / image / material
//! assets, collision surfaces and sim objects, the sim tick counter and how many ticks ran since the last line, frame time,
//! process RSS (now and peak) and the number of course loads so far. Off by default (one env lookup at startup).
use crate::game::runner::GameSim;
use crate::progress::CourseScoped;
use crate::shell::{AppState, ShellUi};
use bevy::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static LOADS: AtomicUsize = AtomicUsize::new(0);

/// Called by `LoadedCourse::load` after every course the app builds (the "void" menu course included).
pub fn note_load(id: &str, millis: u64) {
    LOADS.fetch_add(1, Ordering::Relaxed);
    if enabled() {
        println!("[load-log] LOAD course={id} took={millis}ms peak_rss_mb={}", peak_rss_kb() / 1024);
    }
}

/// How many courses were built so far (tests and the audit table).
pub fn loads() -> usize {
    LOADS.load(Ordering::Relaxed)
}

pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("JUMPMAN_LOAD_LOG").is_some_and(|v| !v.is_empty()))
}

pub fn rss_kb() -> u64 {
    std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}

#[cfg(target_os = "macos")]
pub fn peak_rss_kb() -> u64 {
    #[repr(C)]
    struct Rusage {
        times: [i64; 4],
        maxrss: i64,
        rest: [i64; 13],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut u = Rusage { times: [0; 4], maxrss: 0, rest: [0; 13] };
    // SAFETY: `Rusage` matches the macOS `struct rusage` (two timevals, then maxrss and thirteen more longs).
    unsafe { getrusage(0, &mut u) };
    (u.maxrss / 1024) as u64
}

#[cfg(not(target_os = "macos"))]
pub fn peak_rss_kb() -> u64 {
    0
}

pub struct LoadLogPlugin;

impl Plugin for LoadLogPlugin {
    fn build(&self, app: &mut App) {
        if enabled() {
            app.add_systems(Last, report);
        }
    }
}

#[derive(Default)]
struct Seen {
    next: f32,
    key: String,
    ticks: u64,
    frames: u32,
    ms: f32,
}

#[allow(clippy::too_many_arguments)]
fn report(
    time: Res<Time<Real>>,
    state: Option<Res<AppState>>,
    ui: Option<Res<ShellUi>>,
    sim: Option<Res<GameSim>>,
    roots: Query<(), (With<CourseScoped>, Without<ChildOf>)>,
    all: Query<()>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<StandardMaterial>>,
    mut seen: Local<Seen>,
) {
    seen.frames += 1;
    seen.ms += time.delta_secs() * 1000.0;
    let now = time.elapsed_secs();
    let (id, act, area, ticks, surfaces, objects) = match &sim {
        Some(s) => (s.course.request.id.clone(), s.course.request.act, s.course.request.area, s.ticks, s.course.world.surfaces().len(), s.mario.objects.objects.len()),
        None => ("<none>".into(), 0, 0, 0, 0, 0),
    };
    let st = state.map_or("?".to_string(), |s| format!("{:?}", *s));
    let key = format!("{st}/{id}/{act}/{area}/{}", loads());
    if now < seen.next && key == seen.key {
        return;
    }
    seen.next = now + 1.0;
    seen.key = key;
    let screen = ui.map_or("?".to_string(), |u| format!("{:?}", u.screen));
    let frame_ms = if seen.frames > 0 { seen.ms / seen.frames as f32 } else { 0.0 };
    println!(
        "[load-log] t={now:.1} state={st} screen={screen} course={id} act={act} area={area} roots={} entities={} meshes={} images={} materials={} surfaces={surfaces} objects={objects} ticks={ticks} (+{}) frame_ms={frame_ms:.1} rss_mb={} peak_mb={} loads={}",
        roots.iter().count(),
        all.iter().count(),
        meshes.len(),
        images.len(),
        materials.len(),
        ticks.saturating_sub(seen.ticks),
        rss_kb() / 1024,
        peak_rss_kb() / 1024,
        loads()
    );
    seen.ticks = ticks;
    seen.frames = 0;
    seen.ms = 0.0;
}
