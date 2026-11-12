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
    // The empty menu course is not a level: it is printed but not counted.
    if id != crate::game::playground::VOID_COURSE {
        LOADS.fetch_add(1, Ordering::Relaxed);
    }
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

/// CPU time (user + system, all threads) this process has used, in milliseconds. Unlike wall-clock frame time it is not
/// inflated by other processes competing for the cores, so `cpu_ms` (per frame) is the number to compare when profiling on
/// a busy machine; on a one-core phone it is also the frame cost.
#[cfg(target_os = "macos")]
pub fn cpu_time_ms() -> f64 {
    #[repr(C)]
    struct Rusage {
        times: [i64; 4],
        rest: [i64; 14],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut u = Rusage { times: [0; 4], rest: [0; 14] };
    // SAFETY: `Rusage` matches the macOS `struct rusage` (two timevals first, then fourteen longs).
    unsafe { getrusage(0, &mut u) };
    // tv_usec is a 32-bit int followed by padding
    (u.times[0] + u.times[2]) as f64 * 1000.0 + (u.times[1] as i32 + u.times[3] as i32) as f64 / 1000.0
}

#[cfg(not(target_os = "macos"))]
pub fn cpu_time_ms() -> f64 {
    0.0
}

pub struct LoadLogPlugin;

impl Plugin for LoadLogPlugin {
    fn build(&self, app: &mut App) {
        if enabled() {
            app.add_systems(Last, report);
            // `JUMPMAN_LOAD_LOG=2`: once, 4 s in, list the top-level entities grouped by their components.
            if std::env::var("JUMPMAN_LOAD_LOG").is_ok_and(|v| v == "2") {
                app.add_systems(Last, census);
            }
            // `JUMPMAN_LOAD_LOG=3`: a few times, 6 s in, which entities had their Transform / Visibility written this frame.
            if std::env::var("JUMPMAN_LOAD_LOG").is_ok_and(|v| v == "3") {
                app.add_systems(Last, churn);
            }
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
    cpu: f64,
}

#[allow(clippy::too_many_arguments)]
fn report(
    time: Res<Time<Real>>,
    state: Option<Res<AppState>>,
    ui: Option<Res<ShellUi>>,
    sim: Option<Res<GameSim>>,
    roots: Query<(), (With<CourseScoped>, Without<ChildOf>)>,
    all: Query<()>,
    drawn: Query<(&ViewVisibility, Has<bevy::light::NotShadowCaster>), With<Mesh3d>>,
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
    let cpu_now = cpu_time_ms();
    let cpu_ms = if seen.frames > 0 { (cpu_now - seen.cpu) / seen.frames as f64 } else { 0.0 };
    seen.cpu = cpu_now;
    let mesh_ents = drawn.iter().count();
    let in_view = drawn.iter().filter(|(v, _)| v.get()).count();
    let casters = drawn.iter().filter(|(v, no_shadow)| v.get() && !*no_shadow).count();
    println!(
        "[load-log] t={now:.1} state={st} screen={screen} course={id} act={act} area={area} roots={} entities={} mesh_ents={mesh_ents} in_view={in_view} casters={casters} meshes={} images={} materials={} surfaces={surfaces} objects={objects} ticks={ticks} (+{}) frame_ms={frame_ms:.1} cpu_ms={cpu_ms:.1} rss_mb={} peak_mb={} loads={}",
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

fn census(world: &mut World, mut done: Local<bool>) {
    if *done || world.resource::<Time<Real>>().elapsed_secs() < 4.0 {
        return;
    }
    *done = true;
    let mut groups: std::collections::BTreeMap<String, usize> = Default::default();
    let mut q = world.query_filtered::<EntityRef, Without<ChildOf>>();
    for e in q.iter(world) {
        let mut names: Vec<String> = world.inspect_entity(e.id()).into_iter().flatten().map(|(_, info)| info.name().shortname().to_string()).collect();
        names.sort();
        names.truncate(8);
        *groups.entry(names.join(" ")).or_default() += 1;
    }
    let mut list: Vec<_> = groups.into_iter().collect();
    list.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in list.into_iter().take(40) {
        println!("[load-log] census {n:5} {k}");
    }
}

/// Entities whose `Transform` or `Visibility` was written during the last frame, grouped by their components. Every write
/// (even of an equal value) wakes transform propagation and visibility propagation, so a pool of idle entities that is
/// rewritten each frame shows up here.
fn churn(world: &mut World, mut seen: Local<(u32, f32)>) {
    let now = world.resource::<Time<Real>>().elapsed_secs();
    if seen.0 >= 3 || now < 6.0 + seen.0 as f32 {
        return;
    }
    seen.0 += 1;
    let (last, this) = (world.last_change_tick(), world.change_tick());
    let mut groups: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut q = world.query::<EntityRef>();
    let mut hits = Vec::new();
    for e in q.iter(world) {
        let tf = e.get_change_ticks::<Transform>().is_some_and(|t| t.is_changed(last, this));
        let vis = e.get_change_ticks::<Visibility>().is_some_and(|t| t.is_changed(last, this));
        if tf || vis {
            hits.push((e.id(), tf, vis));
        }
    }
    let total = hits.len();
    for (id, tf, vis) in hits {
        let mut names: Vec<String> = world
            .inspect_entity(id)
            .into_iter()
            .flatten()
            .map(|(_, info)| info.name().shortname().to_string())
            .filter(|n| !matches!(n.as_str(), "GlobalTransform" | "InheritedVisibility" | "ViewVisibility" | "Transform" | "Visibility" | "TransformTreeChanged" | "Aabb" | "Children" | "ChildOf"))
            .collect();
        names.truncate(7);
        let g = groups.entry(names.join(" ")).or_default();
        g.0 += tf as usize;
        g.1 += vis as usize;
    }
    println!("[load-log] churn: {total} entities written this frame");
    let mut list: Vec<_> = groups.into_iter().collect();
    list.sort_by_key(|(_, (a, b))| std::cmp::Reverse(a + b));
    for (k, (t, v)) in list.into_iter().take(14) {
        println!("[load-log] churn transform={t:4} visibility={v:4}  {k}");
    }
}
