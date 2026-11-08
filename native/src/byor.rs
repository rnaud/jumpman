//! Bring-your-own-ROM setup. The shipped app contains no Nintendo-derived data: on first launch
//! (or `jumpman --setup`) a small separate Bevy app asks for ROMs (drag and drop, or `--rom PATH`),
//! runs `byor/pipeline.py` to extract the assets into the per-user data dir, then re-executes the
//! game. See `native/BYOR.md`.
use bevy::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const GAMES: [(&str, &str); 4] = [
    ("sm64", "Super Mario 64 (US)  [required]"),
    ("mk64", "Mario Kart 64 (US)   [kart racing]"),
    ("oot", "Ocarina of Time (US 1.2)  [Zelda courses, Link]"),
    ("bk", "Banjo-Kazooie (US)  [Banjo]"),
];

/// Per-user data dir holding `assets/`, `public/` and `stamps/`. `JUMPMAN_DATA_DIR` overrides.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("JUMPMAN_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        home.join("Library/Application Support/Jumpman")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("Jumpman")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or(home.join(".local/share")).join("jumpman")
    }
}

/// Whether `game`'s extraction finished (its stamp exists with the current pipeline version).
pub fn installed(game: &str) -> bool {
    installed_in(&data_dir(), game)
}

pub fn installed_in(dir: &Path, game: &str) -> bool {
    std::fs::read_to_string(dir.join("stamps").join(format!("{game}.json")))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .is_some_and(|v| v["version"].as_u64() == Some(PIPELINE_VERSION))
}

/// True when assets come from a ROM import (the data dir has stamps), i.e. not the full build.
pub fn is_byor_install() -> bool {
    data_dir().join("stamps").is_dir()
}

/// True when running from a `Jumpman-BYOR.app` (the bundle carries `Contents/Resources/byor-src`).
pub fn is_byor_bundle() -> bool {
    std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(|d| d.join("../Resources/byor-src/byor/pipeline.py"))).is_some_and(|p| p.is_file())
}

/// Assets in the data dir were produced by an older pipeline (e.g. before music was added): run setup again.
pub fn needs_update(root: &Path) -> bool {
    root.starts_with(data_dir()) && data_dir().join("stamps").is_dir() && !installed("sm64")
}

/// Settings > Import / re-extract ROMs: start a second copy of the game in setup mode and quit this one.
pub fn reopen_setup(exit: &mut EventWriter<AppExit>) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("jumpman"));
    match Command::new(exe).arg("--setup").spawn() {
        Ok(_) => {
            exit.write(AppExit::Success);
        }
        Err(e) => eprintln!("could not reopen setup: {e}"),
    }
}

/// Must match `VERSION` in `byor/pipeline.py`; bump both to force re-extraction.
pub const PIPELINE_VERSION: u64 = 2;

/// Where `byor/pipeline.py`, `scripts/` and `reference/` live: `JUMPMAN_BYOR_SRC`, the app bundle
/// (`Contents/Resources/byor-src`), next to the executable, or the repo checkout.
pub fn source_root() -> Option<PathBuf> {
    let ok = |p: &PathBuf| p.join("native/byor/pipeline.py").is_file() || p.join("byor/pipeline.py").is_file();
    let mut c: Vec<PathBuf> = std::env::var_os("JUMPMAN_BYOR_SRC").map(PathBuf::from).into_iter().collect();
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(PathBuf::from)) {
        c.push(d.join("../Resources/byor-src"));
        c.push(d.join("byor-src"));
    }
    c.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."));
    c.into_iter().find(ok)
}

fn pipeline_path(root: &Path) -> PathBuf {
    let a = root.join("native/byor/pipeline.py");
    if a.is_file() { a } else { root.join("byor/pipeline.py") }
}

/// macOS: the extractor builds the decomp's C tools, which needs the Xcode Command Line Tools.
pub fn have_build_tools() -> bool {
    !cfg!(target_os = "macos") || Command::new("xcode-select").arg("-p").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

#[derive(Clone, Default, Debug, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Running,
    Done,
}

#[derive(Default)]
struct State {
    roms: Vec<(String, PathBuf)>,
    progress: std::collections::HashMap<String, (u32, String)>,
    failed: std::collections::HashMap<String, String>,
    messages: Vec<String>,
    phase: Phase,
    tools_missing: bool,
}

#[derive(Resource, Clone)]
struct Shared(Arc<Mutex<State>>);

#[derive(Resource)]
struct Src(PathBuf);

fn identify(src: &Path, rom: &Path) -> Option<String> {
    let out = Command::new("python3").arg("-I").arg(pipeline_path(src)).args(["--source-root", ".", "--data-dir", "."]).arg("--identify").arg(rom).output().ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    out.status.success().then_some(id)
}

fn add_rom(shared: &Shared, src: &Path, path: PathBuf) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (shared, src) = (shared.clone(), src.to_path_buf());
    std::thread::spawn(move || {
        let found = identify(&src, &path);
        let mut s = shared.0.lock().unwrap();
        match found {
            Some(g) => {
                s.roms.retain(|(x, _)| *x != g);
                s.roms.push((g.clone(), path));
                s.messages.push(format!("{name}: recognised {g}"));
            }
            None => s.messages.push(format!("{name}: not a supported US ROM (z64/v64/n64 all work)")),
        }
    });
}

fn start(shared: &Shared, src: &Path) {
    let (roms, data) = {
        let mut s = shared.0.lock().unwrap();
        if s.phase == Phase::Running || s.roms.is_empty() {
            return;
        }
        s.phase = Phase::Running;
        s.failed.clear();
        (s.roms.clone(), data_dir())
    };
    let (shared, src) = (shared.clone(), src.to_path_buf());
    std::thread::spawn(move || {
        let mut cmd = Command::new("python3");
        cmd.arg("-I").arg(pipeline_path(&src)).arg("--source-root").arg(&src).arg("--data-dir").arg(&data);
        for (g, p) in &roms {
            cmd.arg(format!("--{g}")).arg(p);
        }
        let child = cmd.stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let mut s = shared.0.lock().unwrap();
                s.messages.push(format!("could not start python3: {e}"));
                s.phase = Phase::Idle;
                return;
            }
        };
        use std::io::BufRead;
        for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            let mut s = shared.0.lock().unwrap();
            if let Some(rest) = line.strip_prefix("@@PROGRESS ") {
                let mut it = rest.splitn(3, ' ');
                if let (Some(g), Some(p), Some(m)) = (it.next(), it.next(), it.next()) {
                    s.progress.insert(g.into(), (p.parse().unwrap_or(0), m.into()));
                }
            } else if let Some(g) = line.strip_prefix("@@DONE ") {
                s.progress.insert(g.into(), (100, "installed".into()));
            } else if let Some(rest) = line.strip_prefix("@@FAIL ") {
                let (g, m) = rest.split_once(' ').unwrap_or((rest, ""));
                s.failed.insert(g.into(), m.into());
            }
        }
        let _ = child.wait();
        shared.0.lock().unwrap().phase = Phase::Done;
    });
}

#[derive(Component)]
struct Board;

fn ui(mut commands: Commands, shared: Res<Shared>) {
    commands.spawn(Camera2d);
    commands.spawn((
        Board,
        Text::new(""),
        TextFont { font_size: 18.0, ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Px(24.0), top: Val::Px(20.0), right: Val::Px(24.0), ..default() },
    ));
    let _ = shared;
}

fn drops(mut ev: EventReader<FileDragAndDrop>, shared: Res<Shared>, src: Res<Src>) {
    for e in ev.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = e {
            add_rom(&shared, &src.0, path_buf.clone());
        }
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, shared: Res<Shared>, src: Res<Src>, mut exit: EventWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Enter) {
        start(&shared, &src.0);
    }
    if keys.just_pressed(KeyCode::KeyI) && shared.0.lock().unwrap().tools_missing {
        let _ = Command::new("xcode-select").arg("--install").spawn();
    }
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn render(shared: Res<Shared>, mut board: Single<&mut Text, With<Board>>) {
    let s = shared.0.lock().unwrap();
    let mut t = String::from("JUMPMAN SETUP\n\nThis app ships without any Nintendo game data.\nDrop your own legally-dumped US ROMs on this window (.z64 / .v64 / .n64).\n\n");
    if s.tools_missing {
        t += "Xcode Command Line Tools are missing. Press I to install them, then relaunch.\n\n";
    }
    let have = data_dir();
    for (g, label) in GAMES {
        let status = if let Some(f) = s.failed.get(g) {
            format!("FAILED: {}", f.chars().take(110).collect::<String>())
        } else if let Some((p, m)) = s.progress.get(g) {
            format!("{p:>3}%  {m}")
        } else if s.roms.iter().any(|(x, _)| x == g) {
            "ROM ready".into()
        } else if installed_in(&have, g) {
            "installed".into()
        } else {
            "drop ROM here".into()
        };
        t += &format!("  {label:<54}{status}\n");
    }
    t += "\n";
    for m in s.messages.iter().rev().take(4) {
        t += &format!("{m}\n");
    }
    t += match s.phase {
        Phase::Idle if s.roms.is_empty() => "\nWaiting for a ROM.  Esc quits.\n",
        Phase::Idle => "\nPress Enter to extract.\n",
        Phase::Running => "\nExtracting (a few minutes for Super Mario 64)...\n",
        Phase::Done => "\nFinished. Restarting the game...\n",
    };
    board.0 = t;
}

/// After a successful run, re-exec the game so it comes up with the new assets in a fresh process
/// (winit allows only one event loop per process).
fn finish(shared: Res<Shared>, mut exit: EventWriter<AppExit>, mut waited: Local<f32>, time: Res<Time>) {
    let s = shared.0.lock().unwrap();
    if s.phase != Phase::Done {
        return;
    }
    *waited += time.delta_secs();
    if *waited > 1.5 {
        exit.write(AppExit::Success);
    }
}

/// Runs the setup screen. Returns once the window closes; the caller re-evaluates assets.
/// `roms` are pre-queued (from `--rom`) and start immediately.
pub fn run_setup(roms: Vec<PathBuf>) {
    let Some(src) = source_root() else {
        eprintln!("error: extraction scripts not found (set JUMPMAN_BYOR_SRC)");
        std::process::exit(1);
    };
    let shared = Shared(Arc::new(Mutex::new(State { tools_missing: !have_build_tools(), ..default() })));
    for r in roms.iter() {
        // Identify synchronously so the auto-start below sees them.
        if let Some(g) = identify(&src, r) {
            shared.0.lock().unwrap().roms.push((g, r.clone()));
        } else {
            eprintln!("warning: {} is not a supported ROM", r.display());
        }
    }
    let auto = !roms.is_empty();
    if auto {
        start(&shared, &src);
    }
    App::new()
        .insert_resource(shared)
        .insert_resource(Src(src))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Jumpman setup".into(), resolution: (900.0, 420.0).into(), ..default() }),
            ..default()
        }))
        .add_systems(Startup, ui)
        .add_systems(Update, (drops, keys, render, finish))
        .run();
}

/// Replace this process with a fresh copy of itself (no `--setup`/`--rom`), so a first run needs no manual restart.
pub fn relaunch() -> ! {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("jumpman"));
    let mut args = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--setup" => {}
            "--rom" => {
                it.next();
            }
            _ => args.push(a),
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = Command::new(&exe).args(&args).exec();
        eprintln!("relaunch failed: {e}");
        std::process::exit(1);
    }
    #[cfg(not(unix))]
    {
        let _ = Command::new(&exe).args(&args).spawn();
        std::process::exit(0);
    }
}

/// `(--setup requested, ROMs from repeated --rom PATH)`
pub fn parse_setup_args() -> (bool, Vec<PathBuf>) {
    let mut setup = std::env::var_os("JUMPMAN_SETUP").is_some_and(|v| !v.is_empty());
    let mut roms = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--setup" => setup = true,
            "--rom" => {
                if let Some(p) = it.next() {
                    roms.push(PathBuf::from(p));
                    setup = true;
                }
            }
            _ => {}
        }
    }
    (setup, roms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_gates_installed() {
        let d = std::env::temp_dir().join(format!("byor-test-{}", std::process::id()));
        std::fs::create_dir_all(d.join("stamps")).unwrap();
        assert!(!installed_in(&d, "sm64"));
        std::fs::write(d.join("stamps/sm64.json"), format!("{{\"version\":{PIPELINE_VERSION}}}")).unwrap();
        assert!(installed_in(&d, "sm64"));
        std::fs::write(d.join("stamps/sm64.json"), "{\"version\":0}").unwrap();
        assert!(!installed_in(&d, "sm64"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
