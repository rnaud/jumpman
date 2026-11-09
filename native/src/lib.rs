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
mod button_marker;
mod audio;
mod banjo_ctl;
mod bk_enemies;
mod bk_warps;
mod bindings;
pub mod byor;
pub mod content;
mod camera;
mod characters;
mod controller;
mod fx;
mod experiments;
mod game;
mod hud;
mod input;
mod link;
mod lighting;
mod mk64_music;
mod perf;
mod progress;
mod safe_area;
mod objects;
mod render;
mod shell;
mod smfont;
mod touch;
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

const DEFAULT_COURSE: &str = "castle_grounds";

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
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(PathBuf::from)) {
        candidates.push(dir.join("../Resources/assets"));
        candidates.push(dir.join("assets"));
        candidates.push(dir.join("../../../assets"));
    }
    // BYOR: assets extracted from the user's own ROMs (see byor.rs).
    candidates.push(byor::data_dir().join("assets"));
    // A BYOR app bundle never falls back to the build machine's checkout (it would hide the setup window there).
    if !byor::is_byor_bundle() {
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"));
        candidates.push(PathBuf::from("assets"));
    }
    candidates.into_iter().find(valid).unwrap_or_else(|| byor::data_dir().join("assets"))
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
    // First run (no extracted assets) or `--setup` / `--rom PATH`: the ROM import screen, then relaunch.
    let (setup, roms) = byor::parse_setup_args();
    if setup || !asset_root().join("courses").is_dir() || byor::needs_update(&asset_root()) {
        byor::run_setup(roms);
        if asset_root().join("courses").is_dir() && !byor::needs_update(&asset_root()) {
            byor::relaunch();
        }
        std::process::exit(0);
    }
    let root = asset_root();
    let sim = match GameSim::load(&root, request) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Could not load the course: {e}\nSee the SM64 export instructions in README.md.");
            std::process::exit(1);
        }
    };
    let title = format!("Jumpman  •  {}     |     WASD move   Space jump   Shift punch   Ctrl crouch   Q/E orbit   R reset   C camera", sim.course.name);
    let exit_after = std::env::var("JUMPMAN_EXIT_AFTER").ok().and_then(|v| v.parse::<f32>().ok());
    // Probe the GPU before the renderer exists: ray tracing needs extra wgpu features and Solari's plugins.
    let rt_launch = lighting::init_launch(&shell::settings::Settings::load(&shell::settings::config_path()));
    let mut app = App::new();
    app.register_asset_source("mk64", bevy::asset::io::AssetSourceBuilder::platform_default(
        &root.join("../public/mk64").to_string_lossy(), None));
    app.insert_resource(content::AvailableContent::probe(&root))
        .insert_resource(AssetRoot(root.clone()))
        .insert_resource(sim)
        .add_plugins(default_plugins_for(rt_launch).set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), unapproved_path_mode: bevy::asset::UnapprovedPathMode::Deny, ..default() }).set(WindowPlugin {
            // `JUMPMAN_NOVSYNC=1` uncaps the frame rate so `JUMPMAN_FPS=1` measures real frame cost.
            primary_window: Some(Window {
                title,
                // `JUMPMAN_WINDOW=874x402` opens a phone-sized window (layout checks and screenshots).
                resolution: std::env::var("JUMPMAN_WINDOW").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))).unwrap_or((1040_u32, 680_u32)).into(),
                present_mode: if std::env::var_os("JUMPMAN_NOVSYNC").is_some() || std::env::var_os("JUMPMAN_UNCAP").is_some() { // `perf.rs` applies the Settings toggle at runtime
                     bevy::window::PresentMode::AutoNoVsync } else { bevy::window::PresentMode::AutoVsync },
                // Phones: take the whole screen so Bevy picks up the real size (as Bevy's mobile example does).
                #[cfg(target_os = "ios")]
                resizable: false,
                #[cfg(target_os = "ios")]
                mode: bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Primary),
                ..default()
            }),
            ..default()
        }))
        .configure_sets(Update, CameraSet)
        .add_plugins((safe_area::SafeAreaPlugin, bindings::BindingsPlugin, input::InputPlugin, sim_plugin::SimPlugin, camera::CameraPlugin, render::RenderPlugin, hud::HudPlugin, smfont::SmFontPlugin, touch::TouchPlugin))
        .add_plugins(characters::CharactersPlugin)
        .add_plugins(banjo_ctl::BanjoCtlPlugin)
        .add_plugins(bk_enemies::BkEnemiesPlugin)
        .add_plugins(bk_warps::BkWarpsPlugin)
        .add_plugins(link::LinkPlugin)
        .add_plugins(audio::GameAudioPlugin)
        .add_plugins(mk64_music::Mk64MusicPlugin)
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
    // Test hooks: JUMPMAN_WALK=1 holds the stick forward and taps jump; JUMPMAN_SCREENSHOT=path
    // saves a PNG one second before JUMPMAN_EXIT_AFTER (or after 5 s).
    if std::env::var("JUMPMAN_WALK").is_ok() {
        app.add_systems(PreUpdate, (|mut input: ResMut<input::PlayerInput>, time: Res<Time>| input.script_walk(time.elapsed_secs())).after(input::read_input));
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
        app.add_systems(Update, move |mut commands: Commands, mut images: ResMut<Assets<Image>>, mut cams: Query<(Entity, &mut bevy::camera::RenderTarget), With<camera::MainCamera>>, mut state: Local<Option<Handle<Image>>>, time: Res<Time>, mut shot: Local<bool>| {
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
            if !*shot && time.elapsed_secs() > at {
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
    app.run();
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
}

/// `DefaultPlugins`, with the renderer asked for Solari's wgpu features when this launch is ray traced.
fn default_plugins_for(rt: bool) -> bevy::app::PluginGroupBuilder {
    let group = DefaultPlugins.build();
    if rt {
        group.set(bevy::render::RenderPlugin {
            render_creation: bevy::render::settings::WgpuSettings { features: bevy::solari::SolariPlugins::required_wgpu_features(), ..default() }.into(),
            ..default()
        })
    } else {
        group
    }
}
