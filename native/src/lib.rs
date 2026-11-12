//! Jumpman native (library: the desktop binary and the iOS static library both enter through `main`): the Bevy app around `jumpman-sim`. See `native/PORTING.md` for the module map.
//!
//! ```text
//! cargo run                 # Castle Grounds
//! cargo run -- bob          # any course id from assets/courses (see --list)
//! cargo run -- bob --act 2 --entry WARP_NODE_0A
//! ```
//! Env: `JUMPMAN_COURSE`, `JUMPMAN_ACT`, `JUMPMAN_ENTRY`, `JUMPMAN_EXIT_AFTER=<seconds>`.
#[allow(dead_code)]
mod action_names;
#[cfg(target_os = "android")]
pub mod android;
mod button_marker;
mod audio;
mod banjo_ctl;
mod hero_feel;
mod bk_enemies;
mod bk_warps;
mod bindings;
pub mod byor;
pub mod content;
mod camera;
#[cfg(test)]
mod collectible_tests;
mod characters;
mod controller;
pub mod diag;
mod fx;
mod experiments;
mod game;
mod hud;
mod input;
mod kart_anywhere;
mod last_input;
mod link;
mod lighting;
mod rt_fog;
mod rt_scene;
mod load_log;
mod bk_anim_tex;
mod bk_music;
pub mod hints;
mod mk64_music;
mod perf;
mod progress;
mod safe_area;
mod objects;
mod render;
mod shell;
mod smfont;
mod touch;
mod touch_script;
mod sim_plugin;
mod zelda;
mod zelda_enemies;

use bevy::prelude::*;
use game::courses;
use game::runner::GameSim;
use game::world::CourseRequest;
use std::path::PathBuf;

/// The 30 Hz simulation step (FixedUpdate).
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct SimSet;

/// Camera update (Update); view systems order themselves around it.
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct CameraSet;

pub(crate) const DEFAULT_COURSE: &str = "castle_grounds";

/// Where `assets/` lives, first match wins: `JUMPMAN_ASSETS`, the app bundle
/// (`Jumpman.app/Contents/Resources/assets`), next to the executable (`assets/`), the repo's
/// `assets/` when run from `native/target/{debug,release}`, the compile-time repo checkout, and
/// finally `./assets`. A candidate counts when it contains `courses/`.
fn asset_root() -> PathBuf {
    let valid = |p: &PathBuf| p.join("courses").is_dir();
    if let Some(p) = std::env::var_os("JUMPMAN_ASSETS").filter(|v| !v.is_empty()).map(PathBuf::from) {
        if valid(&p) {
            return p;
        }
        eprintln!("warning: JUMPMAN_ASSETS={} has no courses/; searching elsewhere", p.display());
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(PathBuf::from));
    let data = byor::data_dir();
    let baked = exe_dir.as_ref().is_some_and(|d| d.join("../Resources/assets/courses").is_dir());
    let imported = data.join("stamps").is_dir();
    let candidates = asset_candidates(exe_dir, &data, baked && imported, byor::is_byor_bundle());
    candidates.into_iter().find(valid).unwrap_or_else(|| data.join("assets"))
}

/// The asset-root candidates in order (pure). `imported_wins`: the app carries baked-in assets but the user imported ROMs into
/// the data dir (it has `stamps/`), so the import comes first; if its `assets/courses` is missing the search falls through
/// to the baked-in data. Without baked-in assets (BYOR, dev builds) the order is unchanged.
pub(crate) fn asset_candidates(exe_dir: Option<PathBuf>, data: &std::path::Path, imported_wins: bool, byor_bundle: bool) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if imported_wins {
        candidates.push(data.join("assets"));
    }
    if let Some(dir) = exe_dir {
        candidates.push(dir.join("../Resources/assets"));
        candidates.push(dir.join("assets"));
        candidates.push(dir.join("../../../assets"));
    }
    // BYOR: assets extracted from the user's own ROMs (see byor.rs).
    candidates.push(data.join("assets"));
    // A BYOR app bundle never falls back to the build machine's checkout (it would hide the setup window there).
    if !byor_bundle {
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"));
        candidates.push(PathBuf::from("assets"));
    }
    candidates
}

pub(crate) fn asset_root_path() -> PathBuf {
    asset_root()
}

/// `<course> [--act N] [--area N] [--entry NODE] [--push] [--list]`, falling back to env vars.
fn parse_args(args: impl Iterator<Item = String>) -> Result<CourseRequest, String> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let mut course = env("JUMPMAN_COURSE");
    let mut request = CourseRequest::new(DEFAULT_COURSE);
    request.act = env("JUMPMAN_ACT").and_then(|v| v.parse().ok()).unwrap_or(1);
    request.entry = env("JUMPMAN_ENTRY");
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--list" => {
                for c in courses::COURSES {
                    println!("{:<22} {}", c.id, c.name);
                }
                std::process::exit(0);
            }
            "--act" => request.act = value("--act")?.parse().map_err(|_| "--act needs a number")?,
            "--area" => request.area = value("--area")?.parse().map_err(|_| "--area needs a number")?,
            "--entry" => request.entry = Some(value("--entry")?),
            "--push" => request.push = true,
            "--experiment" => std::env::set_var("JUMPMAN_EXPERIMENT", value("--experiment")?),
            "--setup" => {}
            "--rom" => {
                value("--rom")?;
            }
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => course = Some(s.to_string()),
        }
    }
    if let Some(c) = course {
        // Short names for the Zelda entry points (`cargo run -- kokiri`).
        let c = match c.as_str() {
            "kokiri" | "village" | "forest" => "kokiri_forest".to_string(),
            "dungeon" | "deku" | "deku_tree_dungeon" => "deku_tree".to_string(),
            "shop" => "kokiri_shop".to_string(),
            "link" | "house" => "links_house".to_string(),
            _ => c,
        };
        if courses::info(&c).is_none() {
            return Err(format!("unknown course '{c}' (try --list)"));
        }
        request.id = c;
    }
    Ok(request)
}

#[bevy_main]
pub fn main() {
    // Worker threads (asset loading, task pools) default to a 2 MB stack, which the glTF loader overflows when a small
    // machine leaves the IO pool one thread; std reads this once, at the first thread spawn.
    if std::env::var_os("RUST_MIN_STACK").is_none() {
        std::env::set_var("RUST_MIN_STACK", "16777216");
    }
    // Automated screenshot / offscreen runs are always silent (the re-launched copy inherits this).
    if (std::env::var_os("JUMPMAN_SCREENSHOT").is_some() || std::env::var_os("JUMPMAN_OFFSCREEN").is_some()) && std::env::var_os("JUMPMAN_NO_AUDIO").is_none() {
        std::env::set_var("JUMPMAN_NO_AUDIO", "1");
    }
    #[cfg(target_os = "android")]
    android::init_env();
    // Swimming runs the decompilation's submerged actions in the game (the TS goldens keep the old path by default).
    jumpman_sim::actions::water::set_faithful(true);
    // Mario wears his cap (`init_mario`) and can lose it to Ukiki; the TS goldens keep a capless Mario.
    progress::castle_npcs::set_init_cap(true);
    let request = match parse_args(std::env::args().skip(1)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    // A start at the title menu loads no level: the menus run on the empty course until the player starts one (a course on the
    // command line, a test hook or `JUMPMAN_SCREEN=pause|courses|...` still starts inside the course it names).
    let request = if shell::detect_launch().0 == shell::AppState::Menu { CourseRequest::void() } else { request };
    // First run (no extracted assets), data from older exporters, or `--setup` / `--rom PATH`: the ROM import screen. By default it
    // runs inside the game's own App (below) and the game starts in the same process when the import is done; the old separate
    // setup App plus relaunch stays behind `JUMPMAN_SETUP_SEPARATE=1`.
    let (setup, roms) = byor::parse_setup_args();
    // At launch: a first run / missing data / a re-import request opens the import screen; so does an update (games imported
    // with older exporters) unless the player skipped it for this app version or the launch names its own course.
    let launch = byor::launch_screen(setup, asset_root().join("courses").is_dir());
    if let byor::LaunchScreen::Update(_) = launch {
        byor::set_update_prompt();
    }
    let need_setup = launch != byor::LaunchScreen::Game;
    if need_setup && std::env::var_os("JUMPMAN_SETUP_SEPARATE").is_some() {
        byor::run_setup(roms);
        if asset_root().join("courses").is_dir() {
            byor::relaunch();
        }
        std::process::exit(0);
    }
    let root = asset_root();
    let sim = if need_setup {
        None
    } else {
        match GameSim::load(&root, request.clone()) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("Could not load the course: {e}\nSee the SM64 export instructions in README.md.");
                std::process::exit(1);
            }
        }
    };
    let title = match &sim {
        Some(sim) if sim.course.request.is_void() => "Jumpman".to_string(),
        Some(sim) => format!("Jumpman  •  {}     |     WASD move   Space jump   Shift punch   Ctrl crouch   Q/E orbit   R reset   C camera", sim.course.name),
        None => "Jumpman setup".to_string(),
    };
    let exit_after = std::env::var("JUMPMAN_EXIT_AFTER").ok().and_then(|v| v.parse::<f32>().ok());
    // Probe the GPU before the renderer exists: ray tracing needs extra wgpu features and Solari's plugins.
    let rt_launch = lighting::init_launch(&shell::settings::Settings::load(&shell::settings::config_path()));
    let mut app = App::new();
    // Android's default asset reader is the APK's AssetManager; the extracted data lives on the file system.
    #[cfg(target_os = "android")]
    app.register_asset_source(bevy::asset::io::AssetSourceId::Default, file_source(&root));
    app.register_asset_source("mk64", file_or_default_source(&root.join("../public/mk64")));
    // With data the course is loaded before the plugins are built (some of them, like the progress state, read it in `build`).
    if let Some(sim) = sim {
        app.insert_resource(sim);
    }
    app.insert_resource(content::AvailableContent::probe(&root))
        .insert_resource(AssetRoot::new(root.clone()))
        .add_plugins(default_plugins_for(rt_launch).set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), unapproved_path_mode: bevy::asset::UnapprovedPathMode::Deny, ..default() }).set(WindowPlugin {
            // `JUMPMAN_NOVSYNC=1` uncaps the frame rate so `JUMPMAN_FPS=1` measures real frame cost.
            primary_window: Some(Window {
                title,
                // Headless / off-screen runs never open a visible window.
                visible: std::env::var_os("JUMPMAN_OFFSCREEN").is_none(),
                // `JUMPMAN_WINDOW=874x402` opens a phone-sized window (layout checks and screenshots); an off-screen run (`JUMPMAN_OFFSCREEN=WxH`) lays out for its own image size unless this says otherwise, so a capture shows what a player of that screen sees.
                resolution: std::env::var("JUMPMAN_WINDOW").or_else(|_| std::env::var("JUMPMAN_OFFSCREEN")).ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))).unwrap_or((1040_u32, 680_u32)).into(),
                present_mode: if std::env::var_os("JUMPMAN_NOVSYNC").is_some() || std::env::var_os("JUMPMAN_UNCAP").is_some() { // `perf.rs` applies the Settings toggle at runtime
                     bevy::window::PresentMode::AutoNoVsync } else { bevy::window::PresentMode::AutoVsync },
                // Phones: take the whole screen so Bevy picks up the real size (as Bevy's mobile example does).
                #[cfg(any(target_os = "ios", target_os = "android"))]
                resizable: false,
                #[cfg(any(target_os = "ios", target_os = "android"))]
                mode: bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Primary),
                ..default()
            }),
            ..default()
        }))
        .configure_sets(Update, CameraSet)
        .add_plugins((safe_area::SafeAreaPlugin, bindings::BindingsPlugin, input::InputPlugin, last_input::LastInputPlugin, sim_plugin::SimPlugin, camera::CameraPlugin, render::RenderPlugin, hud::HudPlugin, smfont::SmFontPlugin, touch::TouchPlugin, touch_script::TouchScriptPlugin))
        .add_plugins(characters::CharactersPlugin)
        .add_plugins(load_log::LoadLogPlugin)
        .add_plugins(banjo_ctl::BanjoCtlPlugin)
        .add_plugins(kart_anywhere::KartAnywherePlugin)
        .add_plugins(hero_feel::HeroFeelPlugin)
        .add_plugins(bk_enemies::BkEnemiesPlugin)
        .add_plugins(bk_warps::BkWarpsPlugin)
        .add_plugins(link::LinkPlugin)
        .add_systems(Update, diag::update)
        .add_plugins(audio::GameAudioPlugin)
        .add_plugins(mk64_music::Mk64MusicPlugin)
        .add_plugins(bk_music::BkMusicPlugin)
        .add_plugins(hints::HintsPlugin)
        .add_plugins(bk_anim_tex::BkAnimTexturePlugin)
        .add_plugins(audio::bk_events::BkEventSoundsPlugin)
        .add_plugins(fx::FxPlugin)
        .add_plugins(experiments::ExperimentsPlugin)
        .add_plugins(zelda::ZeldaPlugin)
        .add_plugins(zelda_enemies::ZeldaEnemiesPlugin)
        .add_plugins(progress::ProgressPlugin)
        .add_plugins((shell::ShellPlugin, perf::PerfPlugin, lighting::LightingPlugin))
        .add_plugins((controller::ControllerPlugin, button_marker::ShimPlugin))
        .add_plugins(objects::ObjectsPlugin);
    if rt_launch {
        app.add_plugins(bevy::solari::SolariPlugins);
    }
    // No data yet: the game waits, dormant, for the ROM import (see byor::add_setup / boot_game).
    if need_setup {
        byor::add_setup(&mut app, roms, request);
    } else if std::env::var_os("JUMPMAN_SETUP_SEPARATE").is_none() {
        byor::add_overlay(&mut app);
    }
    // Test hooks: JUMPMAN_WALK=1 holds the stick forward and taps jump; JUMPMAN_SCREENSHOT=path
    // saves a PNG one second before JUMPMAN_EXIT_AFTER (or after 5 s).
    if std::env::var("JUMPMAN_WALK").is_ok() {
        app.add_systems(PreUpdate, (|mut input: ResMut<input::PlayerInput>, time: Res<Time>| input.script_walk(time.elapsed_secs())).after(input::read_input));
    }
    // `JUMPMAN_STICK="2:0,64;5:-40,64;8:0,0"` holds the stick at x,y (-64..64) from each of those times on: a
    // scripted run on foot or in the summoned kart, without the jump taps of `JUMPMAN_WALK`. A step written `9:a`
    // presses A once at that time instead (menus, doors).
    if let Ok(script) = std::env::var("JUMPMAN_STICK") {
        let taps: Vec<f32> = script.split(';').filter_map(|s| s.trim().strip_suffix(":a")?.trim().parse().ok()).collect();
        let steps: Vec<(f32, Vec2)> = script
            .split(';')
            .filter_map(|s| {
                let (t, xy) = s.trim().split_once(':')?;
                let (x, y) = xy.split_once(',')?;
                Some((t.trim().parse().ok()?, Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?)))
            })
            .collect();
        app.add_systems(
            PreUpdate,
            (move |mut input: ResMut<input::PlayerInput>, time: Res<Time>, mut tapped: Local<usize>| {
                let now = time.elapsed_secs();
                if let Some((_, stick)) = steps.iter().rev().find(|(t, _)| now >= *t) {
                    input.stick = *stick;
                    input.stick_mag = stick.length().min(64.0);
                }
                let due = taps.iter().filter(|t| now >= **t).count();
                if due > *tapped {
                    *tapped = due;
                    input.script_tap_a();
                }
            })
            .after(input::read_input),
        );
    }
    if let (Ok(path), None) = (std::env::var("JUMPMAN_SCREENSHOT"), std::env::var_os("JUMPMAN_OFFSCREEN")) {
        let at = exit_after.map_or(5.0, |s| (s - 1.0).max(0.5));
        app.add_systems(Update, move |mut commands: Commands, time: Res<Time>, mut done: Local<bool>| {
            if !*done && time.elapsed_secs() > at {
                *done = true;
                commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
            }
        });
    }
    // `JUMPMAN_OFFSCREEN=WxH` renders the main camera into an image of that size and `JUMPMAN_SCREENSHOT` captures the
    // image: window-surface screenshots come back black when there is no window-server session (CI, agents).
    if let Some((w, h)) = std::env::var("JUMPMAN_OFFSCREEN").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))) {
        app.add_systems(Update, move |mut commands: Commands, mut images: ResMut<Assets<Image>>, mut cams: Query<(Entity, &mut bevy::camera::RenderTarget), With<camera::MainCamera>>, mut state: Local<Option<Handle<Image>>>, time: Res<Time>, mut shot: Local<bool>, mut extra: Local<usize>| {
            use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
            if state.is_none() {
                let mut img = Image::new_fill(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, default());
                img.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
                *state = Some(images.add(img));
            }
            let handle = state.clone().unwrap();
            for (e, mut rt) in &mut cams {
                if !matches!(&*rt, bevy::camera::RenderTarget::Image(t) if t.handle == handle) {
                    *rt = bevy::camera::RenderTarget::Image(handle.clone().into());
                    // The UI follows the camera that renders the game (the window no longer has one).
                    commands.entity(e).insert(IsDefaultUiCamera);
                }
            }
            let at = exit_after.map_or(5.0, |s| (s - 1.0).max(0.5));
            // `JUMPMAN_SCREENSHOT_AT=3,4.5,6` takes a still at each of those times as well, saved next to
            // `JUMPMAN_SCREENSHOT` as `<stem>_<time>.png`: transitions and effects need more than one frame to judge.
            let now = time.elapsed_secs();
            if let (Ok(path), Ok(list)) = (std::env::var("JUMPMAN_SCREENSHOT"), std::env::var("JUMPMAN_SCREENSHOT_AT")) {
                for (i, t) in list.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).enumerate() {
                    if i == *extra && now > t {
                        *extra += 1;
                        let stem = path.strip_suffix(".png").unwrap_or(&path);
                        commands.spawn(bevy::render::view::screenshot::Screenshot::image(handle.clone())).observe(bevy::render::view::screenshot::save_to_disk(format!("{stem}_{t}.png")));
                    }
                }
            }
            if !*shot && now > at {
                *shot = true;
                if let Ok(path) = std::env::var("JUMPMAN_SCREENSHOT") {
                    commands.spawn(bevy::render::view::screenshot::Screenshot::image(handle)).observe(bevy::render::view::screenshot::save_to_disk(path));
                }
            }
        });
    }
    if let Some(seconds) = exit_after {
        app.add_systems(Update, move |time: Res<Time>, mut exit: MessageWriter<AppExit>| {
            if time.elapsed_secs() > seconds {
                exit.write(AppExit::Success);
            }
        });
    }
    configure_executors(&mut app);
    app.run();
}

/// Which schedules run their systems on one thread (the single-threaded executor) instead of Bevy's multi-threaded executor.
/// The game has ~1100 systems per frame and nearly all are a few microseconds of work: dispatching them to worker threads
/// costs more than it saves (measured on Bob-omb Battlefield: 5.8 ms and 13.6 ms of CPU per frame multi-threaded against
/// 4.1 ms and 5.8 ms single-threaded, release build, Apple M5 Pro). The multi-threaded task pools stay: asset loading and
/// the `par_iter` loops inside systems (transform propagation, visibility, animation) still use them. `JUMPMAN_EXEC=multi|
/// main|render|single` picks the schedules for profiling (default `single`: main world and render world).
fn configure_executors(app: &mut App) {
    use bevy::ecs::schedule::{Schedules, SingleThreadedExecutor};
    let mode = std::env::var("JUMPMAN_EXEC").unwrap_or_else(|_| "single".into());
    let (main, render) = match mode.as_str() {
        "multi" => (false, false),
        "main" => (true, false),
        "render" => (false, true),
        _ => (true, true),
    };
    if main {
        for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
            schedule.set_executor(SingleThreadedExecutor::new());
        }
    }
    if render {
        if let Some(sub) = app.get_sub_app_mut(bevy::render::RenderApp) {
            for (_, schedule) in sub.world_mut().resource_mut::<Schedules>().iter_mut() {
                schedule.set_executor(SingleThreadedExecutor::new());
            }
        }
    }
}

use render::AssetRoot;

/// iOS entry: `ios/Sources/main.m` calls this (Bevy 0.16's `#[bevy_main]` only covers Android).
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn main_rs() {
    main();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `disable` panics when the group lacks the plugin, i.e. when the `multi_threaded` feature (and with it pipelined
    /// rendering and its macOS exit deadlock) is not the way the build is configured.
    #[test]
    fn the_default_plugins_can_drop_pipelined_rendering() {
        let _ = no_pipelined_rendering(DefaultPlugins.build());
    }

    fn parse(args: &[&str]) -> Result<CourseRequest, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn course_argument_selects_course() {
        assert_eq!(parse(&["bob", "--act", "2"]).unwrap().act, 2);
        assert_eq!(parse(&["bob"]).unwrap().id, "bob");
        assert!(parse(&["nope"]).is_err());
        assert_eq!(parse(&[]).unwrap().id, DEFAULT_COURSE);
    }

    #[test]
    fn an_imported_data_dir_wins_over_baked_assets_and_a_missing_one_falls_back() {
        let root = std::env::temp_dir().join(format!("jumpman-assetroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (baked, data) = (root.join("App/Contents/Resources/assets"), root.join("data"));
        std::fs::create_dir_all(baked.join("courses")).unwrap();
        let exe_dir = root.join("App/Contents/MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let first = |c: Vec<PathBuf>| c.into_iter().find(|p| p.join("courses").is_dir());
        let baked_path = exe_dir.join("../Resources/assets");
        // Nothing imported: the baked-in assets.
        assert_eq!(first(asset_candidates(Some(exe_dir.clone()), &data, false, true)), Some(baked_path.clone()));
        // Imported (stamps + courses): the data dir comes first.
        std::fs::create_dir_all(data.join("assets/courses")).unwrap();
        assert_eq!(first(asset_candidates(Some(exe_dir.clone()), &data, true, true)), Some(data.join("assets")));
        // Stamps but the courses are gone (an interrupted import, a cleaned dir): back to the baked-in assets.
        std::fs::remove_dir_all(data.join("assets/courses")).unwrap();
        assert_eq!(first(asset_candidates(Some(exe_dir.clone()), &data, true, true)), Some(baked_path));
        // Without baked assets (BYOR / dev) the order is the old one: the data dir is not put in front of anything.
        let c = asset_candidates(Some(exe_dir.clone()), &data, false, true);
        assert_eq!(c.iter().position(|p| *p == data.join("assets")), Some(c.len() - 1));
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// Pipelined rendering (a render thread, present with the `multi_threaded` feature, which only desktop builds enable)
/// deadlocks at exit on macOS: the render thread drops the winit window, which needs the main thread, which is joining
/// the render thread. Leaving it out also trims a frame of input latency; the multi-threaded schedule and the off-thread
/// asset loading stay.
pub(crate) fn no_pipelined_rendering(group: bevy::app::PluginGroupBuilder) -> bevy::app::PluginGroupBuilder {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    return group.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>();
    #[cfg(any(target_os = "ios", target_os = "android"))]
    group
}

/// `DefaultPlugins`, with the renderer asked for Solari's wgpu features when this launch is ray traced.
fn default_plugins_for(rt: bool) -> bevy::app::PluginGroupBuilder {
    let mut group = no_pipelined_rendering(DefaultPlugins.build());
    // `JUMPMAN_THREADS=N`: cap the task pools to N threads in total (profiling: N=1 approximates a phone build, which runs
    // the schedule on one thread; 2-4 approximate a big.LITTLE phone's performance cores).
    if let Some(n) = std::env::var("JUMPMAN_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0) {
        group = group.set(bevy::app::TaskPoolPlugin { task_pool_options: bevy::app::TaskPoolOptions::with_num_threads(n) });
    }
    if rt {
        group.set(bevy::render::RenderPlugin {
            render_creation: bevy::render::settings::WgpuSettings { features: bevy::solari::SolariPlugins::required_wgpu_features(), ..default() }.into(),
            ..default()
        })
    } else {
        group
    }
}

/// An asset source reading plain files below `dir` (also on Android, whose platform default reads the APK instead).
#[cfg(target_os = "android")]
fn file_source(dir: &std::path::Path) -> bevy::asset::io::AssetSourceBuilder {
    let dir = dir.to_path_buf();
    bevy::asset::io::AssetSourceBuilder::new(move || Box::new(bevy::asset::io::file::FileAssetReader::new(dir.clone())))
}

fn file_or_default_source(dir: &std::path::Path) -> bevy::asset::io::AssetSourceBuilder {
    #[cfg(target_os = "android")]
    return file_source(dir);
    #[cfg(not(target_os = "android"))]
    bevy::asset::io::AssetSourceBuilder::platform_default(&dir.to_string_lossy(), None)
}

/// Starts the game once the import is done, in the App that already runs the setup screen: load the first course, re-probe which games
/// the data dir has, run the startup schedules that were held back, and show the title menu.
/// Work that needs the game's data and therefore waits for `boot_game` when the game was built dormant (plugins call this from
/// `build` instead of touching `GameSim`; with the data already there it is simply run at once by the caller).
#[derive(Resource, Default)]
pub struct BootHooks(Vec<Box<dyn FnOnce(&mut World) + Send + Sync>>);

pub fn on_boot(app: &mut App, hook: impl FnOnce(&mut World) + Send + Sync + 'static) {
    app.init_resource::<BootHooks>();
    app.world_mut().resource_mut::<BootHooks>().0.push(Box::new(hook));
}

/// A later import finished while the game runs: pick the new data up in place. Re-probes which games exist (menus, hub, character
/// and course pickers read `AvailableContent`), then lets the modules that cached "this game is missing" at startup load it now.
pub(crate) fn reload_content(world: &mut World) {
    let root = asset_root();
    let before = world.get_resource::<content::AvailableContent>().cloned();
    let now = content::AvailableContent::probe(&root);
    let unlocked: Vec<&str> = content::Game::ALL.iter().filter(|g| now.has(**g) && !before.as_ref().is_some_and(|b| b.has(**g))).map(|g| g.id()).collect();
    world.insert_resource(now);
    audio::voices::forget_missing();
    bk_music::reload(world);
    link::items3d::reload(world);
    if let Some(mut ui) = world.get_resource_mut::<shell::ShellUi>() {
        ui.dirty = true;
    }
    if let Some(mut toast) = world.get_resource_mut::<hud::Toast>() {
        toast.show(if unlocked.is_empty() { "IMPORT FINISHED".to_string() } else { format!("NEW GAMES READY: {}", unlocked.join(", ").to_uppercase()) }, 3.0);
    }
    byor::log_line(&format!("content reloaded; newly available: {unlocked:?}"));
}

pub(crate) fn boot_game(world: &mut World) {
    let Some(pending) = world.remove_resource::<byor::PendingBoot>() else { return };
    let root = asset_root();
    let sim = match GameSim::load(&root, pending.0) {
        Ok(s) => s,
        Err(e) => {
            byor::log_line(&format!("could not load the course after the import: {e}"));
            eprintln!("Could not load the course: {e}");
            std::process::exit(1);
        }
    };
    let title = format!("Jumpman  •  {}", sim.course.name);
    byor::end_setup(world);
    world.insert_resource(sim);
    world.insert_resource(content::AvailableContent::probe(&root));
    if let Some(hooks) = world.remove_resource::<BootHooks>() {
        for hook in hooks.0 {
            hook(world);
        }
    }
    world.insert_resource(bevy::winit::WinitSettings::game());
    use bevy::ecs::schedule::ScheduleLabel;
    for label in [bevy::app::PreStartup.intern(), bevy::app::Startup.intern(), bevy::app::PostStartup.intern()] {
        let _ = world.try_run_schedule(label);
    }
    let mut q = world.query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
    for mut w in q.iter_mut(world) {
        w.title = title.clone();
    }
    byor::log_line("game started");
}
