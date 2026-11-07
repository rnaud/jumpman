//! Jumpman native: the Bevy app around `jumpman-sim`. See `native/PORTING.md` for the module map.
//!
//! ```text
//! cargo run                 # Castle Grounds
//! cargo run -- bob          # any course id from assets/courses (see --list)
//! cargo run -- bob --act 2 --entry WARP_NODE_0A
//! ```
//! Env: `JUMPMAN_COURSE`, `JUMPMAN_ACT`, `JUMPMAN_ENTRY`, `JUMPMAN_EXIT_AFTER=<seconds>`.
#[allow(dead_code)]
mod action_names;
mod audio;
mod camera;
mod characters;
mod fx;
mod experiments;
mod game;
mod hud;
mod input;
mod progress;
mod objects;
mod render;
mod shell;
mod sim_plugin;
mod zelda;

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
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"));
    candidates.push(PathBuf::from("assets"));
    candidates.into_iter().find(valid).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"))
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

fn main() {
    let request = match parse_args(std::env::args().skip(1)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
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
    let mut app = App::new();
    app.insert_resource(AssetRoot(root.clone()))
        .insert_resource(sim)
        .add_plugins(DefaultPlugins.set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), unapproved_path_mode: bevy::asset::UnapprovedPathMode::Deny, ..default() }).set(WindowPlugin {
            primary_window: Some(Window { title, resolution: (1040_f32, 680_f32).into(), ..default() }),
            ..default()
        }))
        .configure_sets(Update, CameraSet)
        .add_plugins((input::InputPlugin, sim_plugin::SimPlugin, camera::CameraPlugin, render::RenderPlugin, hud::HudPlugin))
        .add_plugins(characters::CharactersPlugin)
        .add_plugins(audio::GameAudioPlugin)
        .add_plugins(fx::FxPlugin)
        .add_plugins(experiments::ExperimentsPlugin)
        .add_plugins(zelda::ZeldaPlugin)
        .add_plugins(progress::ProgressPlugin)
        .add_plugins(shell::ShellPlugin)
        .add_plugins(objects::ObjectsPlugin);
    // Test hooks: JUMPMAN_WALK=1 holds the stick forward and taps jump; JUMPMAN_SCREENSHOT=path
    // saves a PNG one second before JUMPMAN_EXIT_AFTER (or after 5 s).
    if std::env::var("JUMPMAN_WALK").is_ok() {
        app.add_systems(PreUpdate, (|mut input: ResMut<input::PlayerInput>, time: Res<Time>| input.script_walk(time.elapsed_secs())).after(input::read_input));
    }
    if let Ok(path) = std::env::var("JUMPMAN_SCREENSHOT") {
        let at = exit_after.map_or(5.0, |s| (s - 1.0).max(0.5));
        app.add_systems(Update, move |mut commands: Commands, time: Res<Time>, mut done: Local<bool>| {
            if !*done && time.elapsed_secs() > at {
                *done = true;
                commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
            }
        });
    }
    if let Some(seconds) = exit_after {
        app.add_systems(Update, move |time: Res<Time>, mut exit: EventWriter<AppExit>| {
            if time.elapsed_secs() > seconds {
                exit.write(AppExit::Success);
            }
        });
    }
    app.run();
}

use render::AssetRoot;

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
