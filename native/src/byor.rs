//! Bring-your-own-ROM setup. The shipped app contains no Nintendo-derived data: on first launch
//! (or `jumpman --setup`) a small separate Bevy app asks for ROMs (drag and drop, or `--rom PATH`),
//! extracts the assets into the per-user data dir (in process with the pure-Rust `byor-rs` crate for every game that is
//! ported, else with `byor/pipeline.py`), then re-executes the game. See `native/BYOR.md`.
use bevy::prelude::*;
use bevy::window::WindowResolution;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "ios")]
#[path = "byor_ios.rs"]
pub mod ios_host;

/// Phones (iOS, Android): touch UI, a file picker instead of drag and drop, and no re-exec after the import.
const MOBILE: bool = cfg!(any(target_os = "ios", target_os = "android"));

/// Phone ROM picker glue: the system file picker copies the chosen files into `<data>/inbox` and reports them here.
#[cfg(any(target_os = "ios", target_os = "android"))]
fn pick_roms() {
    #[cfg(target_os = "ios")]
    ios_host::pick_roms();
    #[cfg(target_os = "android")]
    crate::android::pick_roms();
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn picked_pop() -> Option<PathBuf> {
    #[cfg(target_os = "ios")]
    return ios_host::picked_pop();
    #[cfg(target_os = "android")]
    return crate::android::picked_pop();
}

pub const GAMES: [(&str, &str); 4] = [
    ("sm64", "Super Mario 64 (US)  [required]"),
    ("mk64", "Mario Kart 64 (US)   [kart racing]"),
    ("oot", "Ocarina of Time (US 1.2)  [Zelda courses, Link]"),
    ("bk", "Banjo-Kazooie (US)  [Banjo, BK worlds]"),
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

/// Present while an import runs. A launch that finds it (and no running importer) knows the last import was cut short (iOS
/// closed the app in the background, a crash, the user swiping it away); the staged files were never installed, and the ROM copies
/// in `inbox/` are still there, so the import is simply started again.
const ACTIVE_MARKER: &str = ".import-active";

/// Appends a timestamped line to `<data>/import.log` (the file the iOS device log reader and a bug report get).
pub fn log_line(msg: &str) {
    use std::io::Write;
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = T0.get_or_init(std::time::Instant::now).elapsed().as_secs_f32();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("import.log")) {
        let _ = writeln!(f, "[{t:8.1}s] {msg}");
    }
}

/// ROM-sized files directly inside `dir` (the copies the picker left in `inbox/`).
fn rom_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| e.metadata().is_ok_and(|m| m.is_file() && (4 << 20..=80 << 20).contains(&m.len()))).map(|e| e.path()).collect();
    out.sort();
    out
}

/// What a game's stamp says about its installed data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StampState {
    /// No stamp: never imported (or an import was interrupted while swapping files in).
    Missing,
    /// A stamp from an older pipeline version, or whose exporter hash differs from the bundled one: the exporters
    /// changed since this game was imported, so its data may lack newer content ("Update available").
    Stale,
    Current,
}

/// Compare `dir/stamps/<game>.json` with the pipeline version and, when known, the exporter hash this app bundles
/// for the game (`expected`, from `byor-src/exporters.json`; `None` in a dev checkout = compare versions only).
pub fn stamp_state(dir: &Path, game: &str, expected: Option<&str>) -> StampState {
    let Ok(text) = std::fs::read_to_string(dir.join("stamps").join(format!("{game}.json"))) else {
        return StampState::Missing;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return StampState::Stale;
    };
    let hash_ok = expected.is_none_or(|e| v["hash"].as_str() == Some(e));
    if v["version"].as_u64() == Some(PIPELINE_VERSION) && hash_ok { StampState::Current } else { StampState::Stale }
}

/// The stamp hash of the importer for `game` (the pure-Rust importer's revision).
fn bundled_hash(game: &str) -> Option<String> {
    byor_rs::pipeline::exporter_hash(game)
}

/// Whether `game`'s extraction finished with the current exporters.
pub fn installed(game: &str) -> bool {
    installed_in(&data_dir(), game)
}

pub fn installed_in(dir: &Path, game: &str) -> bool {
    stamp_state(dir, game, bundled_hash(game).as_deref()) == StampState::Current
}

/// Imported with older exporters ("update available"); a game that was never imported is not stale.
pub fn game_stale(game: &str) -> bool {
    stamp_state(&data_dir(), game, bundled_hash(game).as_deref()) == StampState::Stale
}

/// True when assets come from a ROM import (the data dir has stamps), i.e. not the full build.
pub fn is_byor_install() -> bool {
    data_dir().join("stamps").is_dir()
}

/// True when running from a `Jumpman-BYOR.app` (macOS: the bundle carries `Contents/Resources/byor-src`; iOS: `byor-src`
/// sits at the top of the .app).
pub fn is_byor_bundle() -> bool {
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(PathBuf::from));
    let has = |d: &Path, rel: &str| d.join(rel).join("byor/sm64-us-sound-player.bin").is_file();
    exe_dir.is_some_and(|d| has(&d, "../Resources/byor-src") || (cfg!(target_os = "ios") && has(&d, "byor-src")))
}

/// Whether this build can run the ROM import: a BYOR bundle, an install that was imported before, a bundle that carries `byor-src`
/// next to baked-in assets (the local Mac app), or an explicit `JUMPMAN_BYOR_SRC`. Pure core: [`importer_available_for`].
pub fn importer_available() -> bool {
    importer_available_for(is_byor_bundle(), is_byor_install(), std::env::var_os("JUMPMAN_BYOR_SRC").is_some_and(|v| !v.is_empty()))
}

pub fn importer_available_for(bundle_has_byor_src: bool, imported_before: bool, env_src: bool) -> bool {
    bundle_has_byor_src || imported_before || env_src
}

/// What Settings shows for one game: `INSTALLED` (current import), `UPDATE AVAILABLE` (older exporters), `BUILT-IN` (never
/// imported, but the app carries its own baked-in data), `NOT IMPORTED`.
pub fn status_label(state: StampState, builtin: bool) -> &'static str {
    match state {
        StampState::Current => "INSTALLED",
        StampState::Stale => "UPDATE AVAILABLE",
        StampState::Missing if builtin => "BUILT-IN",
        StampState::Missing => "NOT IMPORTED",
    }
}

/// True when the running game reads the data dir (an import) instead of baked-in assets.
pub fn using_data_dir() -> bool {
    crate::asset_root_path().starts_with(data_dir())
}

/// `SM64: INSTALLED   MK64: BUILT-IN ...` for the Settings page.
pub fn status_line() -> String {
    let dir = data_dir();
    let builtin = !using_data_dir() && crate::asset_root_path().join("courses").is_dir();
    let part = |g: &str, name: &str| format!("{name}: {}", status_label(stamp_state(&dir, g, bundled_hash(g).as_deref()), builtin));
    format!("{}   {}   {}   {}", part("sm64", "SM64"), part("mk64", "MK64"), part("oot", "OOT"), part("bk", "BK"))
}

/// Assets in the data dir were produced by an older pipeline (e.g. before music was added): run setup again.
pub fn needs_update(root: &Path) -> bool {
    root.starts_with(data_dir()) && data_dir().join("stamps").is_dir() && !installed("sm64")
}

/// Which screen a launch opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchScreen {
    /// The game, at the title menu (or in the course a test hook / command line asked for).
    Game,
    /// The import screen of a first run, a re-import request or data that is missing.
    Setup,
    /// "UPDATE AVAILABLE": games imported earlier (these ids) were extracted with older exporters. The import screen opens on
    /// them with IMPORT / UPDATE and SKIP, PLAY ANYWAY.
    Update(Vec<String>),
}

/// The launch decision, pure: `stamps` is each game's [`StampState`], `skipped` whether the player already skipped the update
/// for this exact set of exporters, `direct_start` a launch that names its own course / screen (command line, test hooks).
pub fn launch_decision(setup_requested: bool, assets_present: bool, stamps: &[(&str, StampState)], skipped: bool, direct_start: bool) -> LaunchScreen {
    if setup_requested || !assets_present {
        return LaunchScreen::Setup;
    }
    let stale: Vec<String> = stamps.iter().filter(|(_, s)| *s == StampState::Stale).map(|(g, _)| g.to_string()).collect();
    if stale.is_empty() || skipped || direct_start {
        LaunchScreen::Game
    } else {
        LaunchScreen::Update(stale)
    }
}

/// Identifies the exporters this app bundles (the pipeline version and every game's exporter hash): a skipped update is
/// remembered for this key and asked again once an app update changes it.
pub fn exporters_key() -> String {
    let hashes: Vec<String> = GAMES.iter().map(|(g, _)| bundled_hash(g).unwrap_or_default()).collect();
    format!("v{PIPELINE_VERSION}:{}", hashes.join(","))
}

fn skip_file(dir: &Path) -> PathBuf {
    dir.join("update-skipped")
}

/// The player chose SKIP, PLAY ANYWAY for this set of exporters.
pub fn update_skipped_in(dir: &Path, key: &str) -> bool {
    std::fs::read_to_string(skip_file(dir)).is_ok_and(|t| t.trim() == key)
}

fn record_skip() {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(skip_file(&data_dir()), exporters_key());
    log_line("update skipped (remembered until the exporters change)");
}

/// A launch that names its own course or screen (command line course, `--experiment`, `JUMPMAN_COURSE` / `JUMPMAN_SCREEN` / the
/// scripted-run hooks) never stops at the update prompt; `JUMPMAN_SKIP_UPDATE_PROMPT=1` forces that for any run.
pub fn direct_start() -> bool {
    let env = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    let capture = env("JUMPMAN_SETUP_SHOT");
    env("JUMPMAN_SKIP_UPDATE_PROMPT") || (!capture && (crate::shell::detect_launch().0 != crate::shell::AppState::Menu || env("JUMPMAN_SCREEN") || env("JUMPMAN_SHELL_SCRIPT")))
}

/// What this launch shows, from the data dir's stamps (BYOR installs only: a dev checkout has no stamps and never prompts).
pub fn launch_screen(setup_requested: bool, assets_present: bool) -> LaunchScreen {
    let dir = data_dir();
    let stamps: Vec<(&str, StampState)> = if is_byor_install() { GAMES.iter().map(|(g, _)| (*g, stamp_state(&dir, g, bundled_hash(g).as_deref()))).collect() } else { Vec::new() };
    launch_decision(setup_requested, assets_present, &stamps, update_skipped_in(&dir, &exporters_key()), direct_start())
}

/// Set by `main` when the launch opens on the update prompt (read by `prepare_setup` and the screen).
static UPDATE_PROMPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_update_prompt() {
    UPDATE_PROMPT.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Marker file that asks the next start of the app to open the setup screen (written by Settings > Import / re-extract ROMs
/// right before the process is replaced; its content `reimport` also makes the setup queue every remembered ROM).
const SETUP_MARKER: &str = ".setup-requested";

/// True when this run was started by a re-import request (see [`SETUP_MARKER`]).
static REIMPORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Replaces this process with a fresh one that opens the setup screen: the game is torn down completely (memory, GPU, audio),
/// exactly as on a first run. Android restarts the activity through `MainActivity.restartApp`; desktop starts `--setup` (the
/// caller then quits). The marker is written first, so if the restart itself fails the next manual launch still lands on the setup
/// screen.
fn restart_into_setup() -> Result<(), String> {
    let data = data_dir();
    let _ = std::fs::create_dir_all(&data);
    std::fs::write(data.join(SETUP_MARKER), b"reimport").map_err(|e| format!("cannot write the setup marker: {e}"))?;
    log_line("reimport: restarting into the setup screen");
    #[cfg(target_os = "android")]
    {
        crate::android::restart_app()?;
        // If the system has not replaced the process within a few seconds, quit: the marker brings the setup screen up next launch.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(6));
            log_line("reimport: the restart did not happen; quitting (the next launch opens the setup screen)");
            std::process::exit(0);
        });
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("jumpman"));
        match Command::new(exe).arg("--setup").spawn() {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(data.join(SETUP_MARKER));
                Err(format!("could not start the setup: {e}"))
            }
        }
    }
}

/// Reads and removes the setup marker; true when it was there.
fn take_setup_marker() -> bool {
    take_setup_marker_in(&data_dir())
}

fn take_setup_marker_in(dir: &Path) -> bool {
    let marker = dir.join(SETUP_MARKER);
    let Ok(text) = std::fs::read_to_string(&marker) else { return false };
    let _ = std::fs::remove_file(&marker);
    REIMPORT.store(text.trim() == "reimport", std::sync::atomic::Ordering::Relaxed);
    true
}

/// Must match `VERSION` in `byor-rs/src/pipeline.rs` (and the reference `byor/pipeline.py`); bump together to force re-extraction of every game. (Exporter changes
/// need no bump: the per-game hashes in the stamps catch them.)
pub const PIPELINE_VERSION: u64 = 3;

/// Where the importer's inputs (`reference/`, `scripts/music`, `byor/`, original assets) live: `JUMPMAN_BYOR_SRC`, the app bundle
/// (`Contents/Resources/byor-src`), next to the executable, or the repo checkout.
pub fn source_root() -> Option<PathBuf> {
    // The importer reads `reference/` (the decomp files), the music templates and the original assets.
    let ok = |p: &PathBuf| p.join("reference/sm64").is_dir() || p.join("reference/oot").is_dir();
    let mut c: Vec<PathBuf> = std::env::var_os("JUMPMAN_BYOR_SRC").map(PathBuf::from).into_iter().collect();
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok()).and_then(|e| e.parent().map(PathBuf::from)) {
        c.push(d.join("../Resources/byor-src"));
        c.push(d.join("byor-src"));
    }
    c.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."));
    c.into_iter().find(ok)
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
    /// Games imported with older exporters.
    stale: Vec<String>,
    /// When the importer last reported progress (shown as a heartbeat while it runs).
    last_progress: Option<std::time::Instant>,
    started: Option<std::time::Instant>,
    /// Ask the system file picker to open on the next frame (set when the remembered ROM copies are gone).
    pick_requested: bool,
    /// Opened by Settings > Import / re-extract ROMs (a re-import), not by a first run.
    reimport: bool,
    /// Opened at launch because a game imported earlier has an update ("UPDATE AVAILABLE", with SKIP, PLAY ANYWAY).
    update_prompt: bool,
    /// The importer's latest stage text (`@@PROGRESS` message), shown as "Now: ...".
    stage: String,
    /// How long the last import took.
    took: Option<std::time::Duration>,
    /// Seconds until the game starts by itself after a successful import (`None` = not counting).
    auto_in: Option<f32>,
}

#[derive(Resource, Clone)]
struct Shared(Arc<Mutex<State>>);

#[derive(Resource)]
struct Src(PathBuf);

/// One run of the importer: its stdout (the `@@` protocol) and a way to wait for the exit status.
pub struct PipelineRun {
    pub out: Box<dyn std::io::Read + Send>,
    finish: Box<dyn FnOnce() -> bool + Send>,
}

impl PipelineRun {
    pub fn finish(self) -> bool {
        (self.finish)()
    }
}

/// The pure-Rust importer on a worker thread; its `@@` output is piped through a channel so the readers below cannot tell
/// it from the Python pipeline's stdout.
fn run_rust(args: Vec<OsString>) -> PipelineRun {
    struct ChanWriter(std::sync::mpsc::Sender<Vec<u8>>);
    impl std::io::Write for ChanWriter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let _ = self.0.send(b.to_vec());
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    struct ChanReader {
        rx: std::sync::mpsc::Receiver<Vec<u8>>,
        buf: Vec<u8>,
        pos: usize,
    }
    impl std::io::Read for ChanReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            while self.pos >= self.buf.len() {
                match self.rx.recv() {
                    Ok(b) => (self.buf, self.pos) = (b, 0),
                    Err(_) => return Ok(0),
                }
            }
            let n = out.len().min(self.buf.len() - self.pos);
            out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let args: Vec<String> = args.into_iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let worker = std::thread::spawn(move || {
        // A panic inside the importer must surface as a failure on the screen, not as a silent "complete" (the channel just closes).
        let mut w = ChanWriter(tx);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| byor_rs::cli::run(args, &mut w))) {
            Ok(code) => code,
            Err(e) => {
                let why = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                let _ = std::io::Write::write_all(&mut w, format!("@@FAIL all the importer crashed: {why}\n").as_bytes());
                2
            }
        }
    });
    PipelineRun { out: Box::new(ChanReader { rx, buf: Vec::new(), pos: 0 }), finish: Box::new(move || worker.join().is_ok_and(|code| code == 0)) }
}

/// One run of the importer, in process (`byor-rs`).
fn run_pipeline(src: &Path, args: Vec<OsString>) -> std::io::Result<PipelineRun> {
    let _ = src;
    Ok(run_rust(args))
}

fn run_to_string(src: &Path, args: Vec<OsString>) -> Option<(String, bool)> {
    let mut run = run_pipeline(src, args).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut run.out, &mut text).ok()?;
    Some((text, run.finish()))
}

/// `Ok(Some(game))`, `Ok(None)` for a file that is not a supported ROM, `Err` when the importer itself could not run (so
/// the screen does not blame a good ROM for a broken runtime).
fn identify(src: &Path, rom: &Path) -> Result<Option<String>, String> {
    let args = ["--source-root", ".", "--data-dir", ".", "--identify"].map(OsString::from).into_iter().chain([rom.as_os_str().to_owned()]).collect();
    let mut run = run_pipeline(src, args).map_err(|e| e.to_string())?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut run.out, &mut text).map_err(|e| e.to_string())?;
    Ok(run.finish().then(|| text.trim().to_string()).filter(|g| !g.is_empty()))
}

/// `--status`: per game `(state, remembered ROM path if the file is still there and identifies)`.
fn query_status(src: &Path, data: &Path) -> Vec<(String, String, Option<PathBuf>)> {
    let args = vec!["--source-root".into(), src.as_os_str().to_owned(), "--data-dir".into(), data.as_os_str().to_owned(), "--status".into()];
    let Some((text, _)) = run_to_string(src, args) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut it = l.strip_prefix("@@STATUS ")?.splitn(3, ' ');
            let (g, state) = (it.next()?.to_string(), it.next()?.to_string());
            Some((g, state, it.next().filter(|p| !p.is_empty()).map(PathBuf::from)))
        })
        .collect()
}

fn add_rom(shared: &Shared, src: &Path, path: PathBuf) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    shared.0.lock().unwrap().messages.push(format!("{name}: checking..."));
    let (shared, src) = (shared.clone(), src.to_path_buf());
    std::thread::spawn(move || {
        let found = identify(&src, &path);
        let mut s = shared.0.lock().unwrap();
        match found {
            Ok(Some(g)) => {
                s.roms.retain(|(x, _)| *x != g);
                s.roms.push((g.clone(), path));
                s.messages.push(format!("{name}: recognised {g}"));
            }
            Ok(None) => s.messages.push(format!("{name}: not a supported US ROM (z64/v64/n64 all work)")),
            Err(e) => s.messages.push(format!("{name}: the importer could not run ({e})")),
        }
    });
}

/// Games that have a stamp (imported before) whose remembered ROM file in `roms.json` no longer exists.
fn games_with_missing_rom(data: &Path) -> Vec<String> {
    let remembered: serde_json::Value = std::fs::read_to_string(data.join("roms.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    GAMES
        .iter()
        .filter(|(g, _)| data.join("stamps").join(format!("{g}.json")).is_file())
        .filter(|(g, _)| !remembered[*g]["path"].as_str().is_some_and(|p| Path::new(p).is_file()))
        .map(|(g, _)| g.to_string())
        .collect()
}

/// The on-screen explanation when a re-import has no ROM to read (`None` when every imported game still has its ROM).
fn missing_roms_message(data: &Path) -> Option<String> {
    let gone = games_with_missing_rom(data);
    let how = if MOBILE { "Tap CHOOSE ROM FILES and pick them again" } else { "Drop them on this window again" };
    (!gone.is_empty()).then(|| format!("ROM files are gone ({}): the app deletes its copy after an import. {how}.", gone.join(", ")))
}

/// IMPORT pressed with nothing to import: say why on screen and (phones) open the file picker.
fn explain_no_roms(s: &mut State, data: &Path) {
    let msg = missing_roms_message(data).unwrap_or_else(|| (if MOBILE { "No ROM chosen yet: tap CHOOSE ROM FILES first." } else { "No ROM yet: drop one on this window." }).into());
    log_line(&format!("import requested with no ROM: {msg}"));
    s.messages.push(msg);
    s.pick_requested = MOBILE;
}

fn start(shared: &Shared, src: &Path) {
    let (roms, data) = {
        let mut s = shared.0.lock().unwrap();
        if s.phase == Phase::Running {
            return;
        }
        if s.roms.is_empty() {
            // Always say something: a press that does nothing looks like a hang.
            explain_no_roms(&mut s, &data_dir());
            return;
        }
        s.phase = Phase::Running;
        s.failed.clear();
        s.started = Some(std::time::Instant::now());
        s.last_progress = Some(std::time::Instant::now());
        (s.roms.clone(), data_dir())
    };
    log_line(&format!("import started: {}", roms.iter().map(|(g, _)| g.as_str()).collect::<Vec<_>>().join(", ")));
    if let Some(secs) = fake_import_seconds() {
        let (shared, fail) = (shared.clone(), std::env::var("JUMPMAN_FAKE_IMPORT_FAIL").ok());
        std::thread::spawn(move || run_fake(shared, roms, secs, fail));
        return;
    }
    let _ = std::fs::write(data.join(ACTIVE_MARKER), roms.iter().map(|(g, _)| g.as_str()).collect::<Vec<_>>().join(" "));
    let (shared, src) = (shared.clone(), src.to_path_buf());
    std::thread::spawn(move || {
        #[cfg(target_os = "ios")]
        ios_host::keep_awake(true);
        use std::io::BufRead;
        let mut log = MOBILE.then(|| std::fs::OpenOptions::new().create(true).append(true).open(data.join("import.log")).ok()).flatten();
        for group in [roms.clone()] {
            if group.is_empty() {
                continue;
            }
            let mut args: Vec<OsString> = vec!["--source-root".into(), src.as_os_str().to_owned(), "--data-dir".into(), data.as_os_str().to_owned()];
            for (g, p) in &group {
                args.push(format!("--{g}").into());
                args.push(p.as_os_str().to_owned());
            }
            let mut child = match run_pipeline(&src, args) {
                Ok(c) => c,
                Err(e) => {
                    let mut s = shared.0.lock().unwrap();
                    s.messages.push(format!("could not start the importer: {e}"));
                    for (g, _) in &group {
                        s.failed.insert(g.clone(), format!("could not start the importer: {e}"));
                    }
                    continue;
                }
            };
            for line in std::io::BufReader::new(std::mem::replace(&mut child.out, Box::new(std::io::empty()))).lines().map_while(Result::ok) {
                if let Some(f) = log.as_mut() {
                    use std::io::Write;
                    let _ = writeln!(f, "{line}");
                }
                let mut s = shared.0.lock().unwrap();
                s.last_progress = Some(std::time::Instant::now());
                if let Some(rest) = line.strip_prefix("@@PROGRESS ") {
                    let mut it = rest.splitn(3, ' ');
                    if let (Some(g), Some(p), Some(m)) = (it.next(), it.next(), it.next()) {
                        s.stage = format!("{g}: {m}");
                        s.progress.insert(g.into(), (p.parse().unwrap_or(0), m.into()));
                    }
                } else if let Some(g) = line.strip_prefix("@@DONE ") {
                    s.progress.insert(g.into(), (100, "installed".into()));
                } else if let Some(rest) = line.strip_prefix("@@FAIL ") {
                    let (g, m) = rest.split_once(' ').unwrap_or((rest, ""));
                    s.failed.insert(g.into(), m.into());
                    if g == "all" {
                        s.messages.push(format!("Import failed: {m}"));
                    }
                    drop(s);
                    log_line(&format!("import failed: {g}: {m}"));
                }
            }
            let _ = child.finish();
        }
        #[cfg(target_os = "ios")]
        ios_host::keep_awake(false);
        if MOBILE {
            // the picker's copies (up to ~100 MB) are only needed during the import
            for (_, p) in &roms {
                if p.starts_with(data.join("inbox")) {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        let _ = std::fs::remove_file(data.join(ACTIVE_MARKER));
        log_line("import finished");
        set_done(&shared);
    });
}

fn set_done(shared: &Shared) {
    let mut s = shared.0.lock().unwrap();
    s.took = s.started.map(|t| t.elapsed());
    s.phase = Phase::Done;
}

/// `JUMPMAN_FAKE_IMPORT=<seconds>` (test hook): the import is simulated (smooth progress over every queued game, nothing is written) so the
/// progress screen can be screenshotted headlessly. `JUMPMAN_FAKE_IMPORT_FAIL=<game>` makes that game fail.
fn fake_import_seconds() -> Option<f32> {
    std::env::var("JUMPMAN_FAKE_IMPORT").ok().and_then(|v| v.parse::<f32>().ok()).filter(|s| *s > 0.0)
}

fn run_fake(shared: Shared, roms: Vec<(String, PathBuf)>, secs: f32, fail: Option<String>) {
    let stages = ["reading the ROM", "decoding textures", "converting models", "writing courses", "building audio", "installing"];
    let steps = 100;
    for i in 1..=steps {
        std::thread::sleep(std::time::Duration::from_secs_f32(secs / steps as f32));
        let mut s = shared.0.lock().unwrap();
        s.last_progress = Some(std::time::Instant::now());
        for (k, (g, _)) in roms.iter().enumerate() {
            // staggered so the bars differ
            let p = ((i * 100) as f32 / steps as f32 * (1.0 + 0.5 * (roms.len() - 1 - k) as f32 / roms.len() as f32)).min(99.0) as u32;
            if fail.as_deref() == Some(g.as_str()) && p > 40 {
                s.failed.insert(g.clone(), "simulated failure: the ROM looks damaged".into());
                continue;
            }
            let m = stages[(p as usize * stages.len() / 100).min(stages.len() - 1)];
            s.stage = format!("{g}: {m}");
            s.progress.insert(g.clone(), (p, m.into()));
        }
    }
    let mut s = shared.0.lock().unwrap();
    for (g, _) in &roms {
        if !s.failed.contains_key(g) {
            s.progress.insert(g.clone(), (100, "installed".into()));
        }
    }
    drop(s);
    set_done(&shared);
}

#[derive(Component)]
struct Board;

/// The messages / footer text under the game rows.
#[derive(Component)]
struct Msgs;

/// Status text of a row (0..GAMES.len() = games, GAMES.len() = the overall row).
#[derive(Component)]
struct RowStatus(usize);

/// The filled part of a row's progress bar.
#[derive(Component)]
struct BarFill(usize);

/// Everything the setup screen spawns (camera, text, buttons): despawned when the game boots.
#[derive(Component)]
struct SetupEntity;

/// Present while the setup screen owns the window. The game's plugins are built up front in the same App (winit allows one event
/// loop per process and iOS cannot relaunch itself), but dormant: their startup is held back until `boot_game` and the systems that
/// need the game's resources are skipped (see `setup_error_handler`).
#[derive(Resource)]
pub struct SetupActive;

/// What the game should load once the data exists (the command line's course request).
#[derive(Resource)]
pub struct PendingBoot(pub crate::CourseRequest);

/// Present while the import screen is laid over a RUNNING game (iOS, which cannot restart itself; elsewhere only as the fallback when the
/// restart fails): the game keeps its state but is switched off (cameras, UI, update loop) and when the import ends
/// `crate::reload_content` picks the new data up and the screen is removed.
#[derive(Resource)]
pub struct ImportOverlay;

/// Marks an App whose game can open the import screen in place (see [`add_overlay`]).
#[derive(Resource)]
pub struct OverlayReady;

/// Set from the moment Settings > Import is pressed until the process is replaced: the "Leaving the game..." screen is up and the game is
/// switched off; `leaving_tick` restarts into the setup screen once the screen had a few frames on display.
#[derive(Resource)]
pub struct Leaving {
    frames: u32,
    since: Option<f32>,
    restarted: bool,
}

/// How the process is replaced (swapped for a stub in tests).
#[derive(Resource)]
struct RestartHook(fn() -> Result<(), String>);

/// What `deactivate_game` switched off, so the in-place import screen can give it back.
#[derive(Resource, Default)]
struct GameHidden {
    cameras: Vec<Entity>,
    roots: Vec<(Entity, Display)>,
}

/// Stand-in for `Update` (and the fixed-step loop) while the overlay is up: the game's own systems stand still, the importer's
/// run.
#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct ImportUpdate;

static SETUP_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct SetupStartup;

/// While the game is dormant its systems that need resources the game inserts at boot fail parameter validation every frame:
/// that is expected, not an error. Everything else keeps Bevy's default behaviour (panic).
fn setup_error_handler(error: BevyError, ctx: bevy::ecs::error::ErrorContext) {
    if SETUP_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) && error.downcast_ref::<bevy::ecs::system::SystemParamValidationError>().is_some() {
        return;
    }
    bevy::ecs::error::panic(error, ctx);
}

/// The window size of a headless run (`JUMPMAN_OFFSCREEN=WxH`).
fn offscreen_size() -> Option<(u32, u32)> {
    std::env::var("JUMPMAN_OFFSCREEN").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))))
}

/// Headless runs (`JUMPMAN_OFFSCREEN=WxH`) have no window surface: the setup screen's camera renders into this image instead, and
/// `JUMPMAN_SETUP_SHOT` / `JUMPMAN_SETUP_SHOT_AT` save it (see `setup_shots`).
#[derive(Resource)]
struct ShotTarget(Handle<Image>);

/// The camera every setup screen (setup, import overlay, "Leaving the game...") draws with: above every camera of the game.
fn spawn_setup_camera(world: &mut World) -> Entity {
    let target = offscreen_size().map(|(w, h)| {
        if let Some(t) = world.get_resource::<ShotTarget>() {
            return t.0.clone();
        }
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
        let mut img = Image::new_fill(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, default());
        img.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let handle = world.resource_mut::<Assets<Image>>().add(img);
        world.insert_resource(ShotTarget(handle.clone()));
        handle
    });
    let mut cam = world.spawn((Camera2d, Camera { order: 1000, ..default() }, SetupEntity));
    if let Some(h) = target {
        cam.insert(bevy::camera::RenderTarget::Image(h.into()));
    }
    cam.id()
}

fn spawn_backdrop(world: &mut World, cam: Entity) {
    world.spawn((
        SetupEntity,
        UiTargetCamera(cam),
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgb(0.043, 0.071, 0.251)),
        BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(Color::srgb(0.11, 0.165, 0.5), 0.0), ColorStop::percent(Color::srgb(0.043, 0.071, 0.251), 55.0), ColorStop::percent(Color::srgb(0.02, 0.04, 0.133), 100.0)])),
        GlobalZIndex(-1),
    ));
}

/// The v3 pixel face (`shell::retro::font`) for the setup screens: the game is dormant here, so the shell's own start-up (which would
/// install it as the default font) has not run.
#[derive(Resource, Clone)]
struct SetupFont(Handle<Font>);

fn setup_font(world: &mut World) -> Handle<Font> {
    if let Some(f) = world.get_resource::<SetupFont>() {
        return f.0.clone();
    }
    let handle = world.get_resource_or_insert_with(Assets::<Font>::default).add(crate::shell::retro::font::pixel_font());
    world.insert_resource(SetupFont(handle.clone()));
    handle
}

/// The v3 dialog frame: navy gradient, light top-left and dark bottom-right bevel, dark outline ring.
fn frame() -> impl Bundle {
    (
        BackgroundColor(Color::srgb(0.078, 0.11, 0.345)),
        BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(Color::srgb(0.165, 0.227, 0.541), 0.0), ColorStop::percent(Color::srgb(0.078, 0.11, 0.345), 100.0)])),
        BorderColor { top: Color::srgb(0.435, 0.525, 0.878), left: Color::srgb(0.435, 0.525, 0.878), right: Color::srgb(0.04, 0.06, 0.21), bottom: Color::srgb(0.04, 0.06, 0.21) },
        BoxShadow::new(Color::srgb(0.016, 0.027, 0.125), Val::Px(0.0), Val::Px(0.0), Val::Px(2.0), Val::Px(0.0)),
    )
}

/// Pixel text with the skin's hard drop shadow.
fn pixel_text(font: &Handle<Font>, text: impl Into<String>, px: f32, color: Color) -> impl Bundle {
    (Text::new(text), TextFont { font: FontSource::Handle(font.clone()), font_size: FontSize::Px(px), ..default() }, TextColor(color), TextShadow { offset: Vec2::splat((px * 0.1).round().max(1.0)), color: Color::srgb(0.02, 0.04, 0.13) })
}

/// Layout numbers of the setup screen.
fn metrics() -> (f32, f32, f32, f32, f32) {
    // (side padding, top padding, font size, label width, bar width): iOS / Android keep clear of the notch and the bottom buttons
    if MOBILE { (60.0, 28.0, 15.0, 230.0, 150.0) } else { (24.0, 20.0, 17.0, 250.0, 200.0) }
}

/// The full-screen setup / import screen: heading, one row with a progress bar per game, an overall bar, messages (and on phones
/// the buttons).
fn ui(world: &mut World) {
    let cam = spawn_setup_camera(world);
    spawn_backdrop(world, cam);
    let (pad, top, size, label_w, bar_w) = metrics();
    let face = setup_font(world);
    let white = Color::WHITE;
    world
        .spawn((
            SetupEntity,
            UiTargetCamera(cam),
            Node { position_type: PositionType::Absolute, left: Val::Px(pad), right: Val::Px(pad), top: Val::Px(top), flex_direction: FlexDirection::Column, row_gap: Val::Px(5.0), padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)), border: UiRect::all(Val::Px(3.0)), ..default() },
            frame(),
        ))
        .with_children(|root| {
            root.spawn((Board, pixel_text(&face, "", size, white)));
            let names: Vec<String> = GAMES.iter().map(|(_, l)| l.split("  ").next().unwrap_or(l).to_string()).chain(["Overall".to_string()]).collect();
            for (i, name) in names.into_iter().enumerate() {
                root.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: Val::Px(10.0), margin: UiRect::top(Val::Px(if i == GAMES.len() { 6.0 } else { 0.0 })), ..default() }).with_children(|row| {
                    row.spawn((pixel_text(&face, name, size, white), Node { width: Val::Px(label_w), flex_shrink: 0.0, ..default() }));
                    row.spawn((
                        Node { width: Val::Px(bar_w), height: Val::Px(16.0), flex_shrink: 0.0, border: UiRect::all(Val::Px(2.0)), ..default() },
                        BackgroundColor(Color::srgb(0.02, 0.04, 0.133)),
                        BorderColor { top: Color::srgb(0.04, 0.06, 0.21), left: Color::srgb(0.04, 0.06, 0.21), right: Color::srgb(0.36, 0.44, 0.78), bottom: Color::srgb(0.36, 0.44, 0.78) },
                    ))
                    .with_child((BarFill(i), Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Bar::Idle.color())));
                    row.spawn((RowStatus(i), pixel_text(&face, "", size, white), Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), ..default() }));
                });
            }
            root.spawn((Msgs, pixel_text(&face, "", size, Color::srgb(0.68, 0.78, 1.0)), Node { margin: UiRect::top(Val::Px(8.0)), ..default() }));
        });
    #[cfg(any(target_os = "ios", target_os = "android"))]
    touch_buttons(world, cam, world.contains_resource::<ImportOverlay>());
}

/// "Leaving the game..." (what the user sees from the press on Settings > Import until the process is replaced).
fn spawn_leaving_screen(world: &mut World, text: &str) {
    let cam = spawn_setup_camera(world);
    spawn_backdrop(world, cam);
    let (_, _, size, _, _) = metrics();
    let face = setup_font(world);
    world.spawn((
        SetupEntity,
        UiTargetCamera(cam),
        pixel_text(&face, text, size + 4.0, Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Px(metrics().0), right: Val::Px(metrics().0), top: Val::Px(metrics().1 + 30.0), padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)), border: UiRect::all(Val::Px(3.0)), ..default() },
        frame(),
    ));
}

const LEAVING_TEXT: &str = "Leaving the game...\n\nClosing the course and restarting into the import screen.\nThis takes a few seconds.";

/// Which of the setup screen's steps is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetupScreen {
    /// Choose the ROMs (the remembered ones are already listed).
    Select,
    Importing,
    Finished,
    Failed,
}

fn screen_of(s: &State) -> SetupScreen {
    match s.phase {
        Phase::Running => SetupScreen::Importing,
        Phase::Done if s.failed.is_empty() => SetupScreen::Finished,
        Phase::Done => SetupScreen::Failed,
        Phase::Idle if !s.failed.is_empty() => SetupScreen::Failed,
        Phase::Idle => SetupScreen::Select,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bar {
    Idle,
    Running,
    Done,
    Failed,
}

impl Bar {
    fn color(self) -> Color {
        match self {
            Bar::Idle => Color::srgb(0.2, 0.27, 0.6),
            Bar::Running => Color::srgb(1.0, 0.82, 0.23),
            Bar::Done => Color::srgb(0.29, 0.82, 0.38),
            Bar::Failed => Color::srgb(0.82, 0.29, 0.23),
        }
    }
}

/// What the setup screen shows for one state: pure data, so the transitions are testable without a window.
#[allow(dead_code)]
struct View {
    screen: SetupScreen,
    head: String,
    /// (status text, bar fill 0..=1, bar colour) per game, then the overall row.
    rows: Vec<(String, f32, Bar)>,
    foot: String,
    msgs: String,
}

/// What the view depends on besides the state.
struct ViewCtx {
    overlay: bool,
    mobile: bool,
    ios: bool,
    android: bool,
    pad_prompts: bool,
    /// The game can start from the data that is installed.
    playable: bool,
    installed: [bool; 4],
    data_dir: String,
    /// Seconds since the import started (or took, once done).
    elapsed: u64,
    quiet: u64,
}

fn clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn build_view(s: &State, c: &ViewCtx) -> View {
    let screen = screen_of(s);
    let updating = s.update_prompt && !c.overlay && screen == SetupScreen::Select;
    let title = if updating {
        "UPDATE AVAILABLE"
    } else if c.overlay {
        "IMPORT ROMS"
    } else if s.reimport {
        "RE-IMPORT ROMS"
    } else {
        "JUMPMAN SETUP"
    };
    let names = s.stale.iter().map(|g| g.to_uppercase()).collect::<Vec<_>>().join(", ");
    let step = match screen {
        SetupScreen::Select if updating => format!("re-import needed for {names}"),
        SetupScreen::Select => "Step 1 of 3: choose the ROMs".to_string(),
        SetupScreen::Importing => "Step 2 of 3: importing".to_string(),
        SetupScreen::Finished | SetupScreen::Failed => "Step 3 of 3: result".to_string(),
    };
    let intro = match screen {
        SetupScreen::Select => {
            let mut t = if c.android {
                "Import your own US ROMs (.z64 / .v64 / .n64): tap CHOOSE ROM FILES, then IMPORT.\nThe app deletes its copy of each ROM after an import, so a re-import asks for the files again.".to_string()
            } else if c.ios {
                "This app ships without any Nintendo game data: import your own legally-dumped US ROMs (.z64 / .v64 / .n64).\nTap CHOOSE ROM FILES and select them in Files (you can pick several at once), or copy them into Files > On My iPhone > Jumpman and tap SCAN JUMPMAN FOLDER.\nKeep the app open while importing (the screen will not lock).".to_string()
            } else if c.playable {
                "Drop your own legally-dumped US ROMs on this window (.z64 / .v64 / .n64) to import them.\nThere is no file-picker button on desktop: drag the files from Finder onto this window. The game keeps working with its current data if you go back.".to_string()
            } else {
                "This app ships without any Nintendo game data.\nDrop your own legally-dumped US ROMs on this window (.z64 / .v64 / .n64).".to_string()
            };
            if c.android {
                t += &format!("\nOr copy a Mac BYOR data folder to {}", c.data_dir);
            }
            if updating {
                t = if c.mobile { format!("A newer version of the app extracts {names} differently.\nTap UPDATE to re-import them (choose the ROMs again if they are not listed), or SKIP, PLAY ANYWAY:\nyou are not asked again until the next app update.") } else { format!("A newer version of the app extracts {names} differently.\nPress Enter to re-import them (drop the ROMs again if they are not listed), or Esc to SKIP, PLAY ANYWAY:\nyou are not asked again until the next app update.") };
            } else if s.reimport && !s.roms.is_empty() {
                t += "\nThe ROMs of your earlier import were found again (listed below): tap IMPORT, or add more files first.";
            }
            t
        }
        SetupScreen::Importing => "The game is switched off while the importer works. This screen updates every second; keep the app open.".to_string() + if c.mobile { " The Back button is ignored until it finishes." } else { "" },
        SetupScreen::Finished => format!("Everything was imported in {}.", clock(s.took.map_or(c.elapsed, |d| d.as_secs()))),
        SetupScreen::Failed => "At least one game failed (red rows). Choose its ROM again and tap IMPORT, or go back and play what is installed.".to_string(),
    };
    let head = format!("{title}   -   {step}\n{intro}\n");

    let mut rows = Vec::new();
    for (i, (g, _)) in GAMES.iter().enumerate() {
        let row = if let Some(f) = s.failed.get(*g) {
            (format!("FAILED: {}", f.chars().take(if c.mobile { 60 } else { 110 }).collect::<String>()), s.progress.get(*g).map_or(0.0, |(p, _)| *p as f32 / 100.0), Bar::Failed)
        } else if let Some((p, m)) = s.progress.get(*g) {
            (format!("{p:>3}%  {m}"), *p as f32 / 100.0, if *p >= 100 { Bar::Done } else { Bar::Running })
        } else if s.roms.iter().any(|(x, _)| x == g) {
            ("ROM ready".into(), 0.0, Bar::Idle)
        } else if c.installed[i] {
            ("installed".into(), 1.0, Bar::Done)
        } else if s.stale.iter().any(|x| x == g) {
            (if c.mobile { "Update available - choose the ROM again".into() } else { "Update available - re-import (drop the ROM again)".to_string() }, 0.0, Bar::Idle)
        } else {
            (if c.mobile { "not imported".into() } else { "drop ROM here".to_string() }, 0.0, Bar::Idle)
        };
        rows.push(row);
    }
    // Overall: the average of the games being imported.
    let queued: Vec<f32> = s.roms.iter().map(|(g, _)| s.progress.get(g).map_or(0.0, |(p, _)| *p as f32 / 100.0)).collect();
    let overall = if screen == SetupScreen::Finished {
        1.0
    } else if queued.is_empty() {
        0.0
    } else {
        queued.iter().sum::<f32>() / queued.len() as f32
    };
    let ok = screen != SetupScreen::Failed;
    rows.push((
        match screen {
            SetupScreen::Select if s.roms.is_empty() => "nothing queued".into(),
            SetupScreen::Select => format!("{} ROM{} queued", s.roms.len(), if s.roms.len() == 1 { "" } else { "s" }),
            SetupScreen::Importing => format!("{:>3}%  {}", (overall * 100.0) as u32, clock(c.elapsed)),
            SetupScreen::Finished => format!("100%  done in {}", clock(s.took.map_or(c.elapsed, |d| d.as_secs()))),
            SetupScreen::Failed => format!("{:>3}%  stopped", (overall * 100.0) as u32),
        },
        overall,
        match (screen, ok) {
            (SetupScreen::Importing, _) => Bar::Running,
            (SetupScreen::Finished, _) => Bar::Done,
            (SetupScreen::Failed, _) => Bar::Failed,
            _ => Bar::Idle,
        },
    ));

    let foot_hint = match (screen, c.mobile) {
        (SetupScreen::Select, true) if updating && s.roms.is_empty() => "Choose the ROMs again to update, or SKIP, PLAY ANYWAY.".to_string(),
        (SetupScreen::Select, true) if updating => "Tap UPDATE to re-import, or SKIP, PLAY ANYWAY.".to_string(),
        (SetupScreen::Select, false) if updating && s.roms.is_empty() => "Drop the ROMs again to update.  Esc: SKIP, PLAY ANYWAY.".into(),
        (SetupScreen::Select, false) if updating => "Press Enter to update.  Esc: SKIP, PLAY ANYWAY.".into(),
        (SetupScreen::Select, true) if s.roms.is_empty() => "Waiting for a ROM.".to_string(),
        (SetupScreen::Select, true) => "Tap IMPORT to extract.".to_string(),
        (SetupScreen::Select, false) if s.roms.is_empty() && c.overlay && c.pad_prompts => "Waiting for a ROM.  B goes back to the game.".into(),
        (SetupScreen::Select, false) if !s.roms.is_empty() && c.overlay && c.pad_prompts => "Press A to extract.  B goes back to the game.".into(),
        (SetupScreen::Select, false) if !s.roms.is_empty() && c.pad_prompts => "Press A to extract.".into(),
        (SetupScreen::Select, false) if s.roms.is_empty() && c.overlay => "Waiting for a ROM.  Esc goes back to the game.".into(),
        (SetupScreen::Select, false) if s.roms.is_empty() => if c.playable { "Waiting for a ROM.  Esc starts the game with what is installed.".into() } else { "Waiting for a ROM.  Esc quits.".into() },
        (SetupScreen::Select, false) if c.overlay => "Press Enter to extract.  Esc goes back to the game.".into(),
        (SetupScreen::Select, false) => "Press Enter to extract.".into(),
        (SetupScreen::Importing, _) => {
            let spin = ["|", "/", "-", "\\"][((c.elapsed * 4 + c.quiet) % 4) as usize];
            let mut t = format!("Importing {spin}  {}%   elapsed {}\nNow: {}", (overall * 100.0) as u32, clock(c.elapsed), if s.stage.is_empty() { "starting the importer...".to_string() } else { s.stage.clone() });
            if c.quiet >= 8 {
                t += &format!("\nNo progress for {} (a big step, or the system paused the app: it continues by itself)", clock(c.quiet));
            }
            t
        }
        (SetupScreen::Finished, _) if c.overlay => "Import complete!  Loading the new games...".into(),
        (SetupScreen::Finished, _) => match s.auto_in {
            Some(n) => format!("Import complete!  Starting the game in {}...  ({} starts it now)", n.ceil() as u32, if c.mobile { "PLAY / A" } else { "Enter" }),
            None => "Import complete!  Starting the game...".into(),
        },
        (SetupScreen::Failed, _) => "Some imports failed (see above). Fix and tap IMPORT again.".into(),
    };
    let mut msgs = String::new();
    for m in s.messages.iter().rev().take(4) {
        msgs += &format!("{m}\n");
    }
    msgs += &format!("\n{foot_hint}\n");
    View { screen, head, rows, foot: foot_hint, msgs }
}

fn view_ctx(s: &State, overlay: bool) -> ViewCtx {
    let have = data_dir();
    ViewCtx {
        overlay,
        mobile: MOBILE,
        ios: cfg!(target_os = "ios"),
        android: cfg!(target_os = "android"),
        pad_prompts: crate::hints::pad_active(),
        playable: can_play(),
        installed: [0, 1, 2, 3].map(|i| installed_in(&have, GAMES[i].0)),
        data_dir: have.display().to_string(),
        elapsed: if s.phase == Phase::Running { s.started.map_or(0, |t| t.elapsed().as_secs()) } else { s.took.map_or(0, |d| d.as_secs()) },
        quiet: s.last_progress.map_or(0, |t| t.elapsed().as_secs()),
    }
}

/// Phone touch controls: the picker, (iOS) a Documents scan, Import, and Play (or Back to the game over a running one) (the macOS
/// window is driven by drag-and-drop and Enter).
#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Act {
    Pick,
    #[cfg(target_os = "ios")]
    Scan,
    Start,
    /// Back to the running game (only while the import screen is laid over it).
    Back,
    /// Start the game with the installed data (the setup screen that owns the window).
    Play,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn touch_buttons(world: &mut World, cam: Entity, overlay: bool) {
    let face = setup_font(world);
    let updating = !overlay && world.get_resource::<Shared>().is_some_and(|s| s.0.lock().unwrap().update_prompt);
    world
        .spawn((SetupEntity, UiTargetCamera(cam), Node { position_type: PositionType::Absolute, left: Val::Px(60.0), right: Val::Px(60.0), bottom: Val::Px(28.0), column_gap: Val::Px(12.0), ..default() }))
        .with_children(|row| {
            #[cfg(target_os = "ios")]
            let buttons = [(Act::Pick, "CHOOSE ROM FILES"), (Act::Scan, "SCAN JUMPMAN FOLDER"), (Act::Start, if updating { "UPDATE" } else { "IMPORT" })];
            #[cfg(target_os = "android")]
            let buttons = [(Act::Pick, "CHOOSE ROM FILES"), (Act::Start, if updating { "UPDATE" } else { "IMPORT" })];
            let back = Some(if overlay { (Act::Back, "BACK TO THE GAME") } else { if updating { (Act::Play, "SKIP, PLAY ANYWAY") } else { (Act::Play, "PLAY") } });
            for (act, label) in buttons.into_iter().chain(back) {
                row.spawn((
                    Interaction::default(),
                    act,
                    Node { padding: UiRect::axes(Val::Px(18.0), Val::Px(14.0)), border: UiRect::all(Val::Px(2.0)), ..default() },
                    BackgroundColor(Color::srgb(0.2, 0.27, 0.6)),
                    BorderColor { top: Color::srgb(0.36, 0.44, 0.78), left: Color::srgb(0.36, 0.44, 0.78), right: Color::srgb(0.04, 0.06, 0.25), bottom: Color::srgb(0.04, 0.06, 0.25) },
                ))
                .with_child(pixel_text(&face, label, 17.0, Color::WHITE));
            }
        });
}

/// Buttons that cannot do anything right now (an import is running, nothing to import, no game to go back to) are dimmed.
#[cfg(any(target_os = "ios", target_os = "android"))]
fn style_buttons(shared: Res<Shared>, mut q: Query<(&Act, &mut BackgroundColor)>) {
    let s = shared.0.lock().unwrap();
    let running = s.phase == Phase::Running;
    for (act, mut bg) in &mut q {
        let on = match act {
            Act::Pick => !running,
            #[cfg(target_os = "ios")]
            Act::Scan => !running,
            Act::Start => !running && !s.roms.is_empty(),
            Act::Back => !running,
            Act::Play => !running && can_play(),
        };
        let want = if on { Color::srgb(0.2, 0.27, 0.6) } else { Color::srgb(0.1, 0.12, 0.28) };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn touch_actions(q: Query<(&Interaction, &Act), Changed<Interaction>>, shared: Res<Shared>, src: Res<Src>, mut commands: Commands) {
    for (i, act) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        log_line(&format!("button pressed: {}", match act { Act::Pick => "choose ROM files", Act::Start => "import", Act::Back => "back", Act::Play => "play", #[cfg(target_os = "ios")] Act::Scan => "scan" }));
        let running = shared.0.lock().unwrap().phase == Phase::Running;
        if running && !matches!(act, Act::Start) {
            shared.0.lock().unwrap().messages.push("The import is running: wait until it finishes.".into());
            continue;
        }
        match act {
            Act::Pick => {
                shared.0.lock().unwrap().messages.push("Opening the file picker...".into());
                pick_roms()
            }
            #[cfg(target_os = "ios")]
            Act::Scan => scan_documents(&shared, &src.0),
            Act::Start => start(&shared, &src.0),
            Act::Back => {
                commands.queue(|world: &mut World| leave_overlay(world));
            }
            Act::Play => {
                commands.queue(|world: &mut World| play_now(world));
            }
        }
    }
}

/// `JUMPMAN_IOS_PICK=1` opens the file picker by itself a moment after launch: a test hook for the simulator, which
/// cannot tap. Release builds never open it unasked (the setup screen explains the steps; CHOOSE ROM FILES opens it).
#[cfg(target_os = "ios")]
fn auto_pick(time: Res<Time>, mut done: Local<bool>) {
    if *done || time.elapsed_secs() < 1.0 {
        return;
    }
    *done = true;
    if std::env::var_os("JUMPMAN_IOS_PICK").is_some_and(|v| !v.is_empty()) {
        ios_host::pick_roms();
    }
}

/// ROMs the picker / Files / AirDrop copied into the sandbox since the last frame.
#[cfg(any(target_os = "ios", target_os = "android"))]
fn poll_picked(shared: Res<Shared>, src: Res<Src>, time: Res<Time>, mut last_pick: Local<Option<f32>>) {
    if std::mem::take(&mut shared.0.lock().unwrap().pick_requested) {
        pick_roms();
    }
    while let Some(p) = picked_pop() {
        add_rom(&shared, &src.0, p);
        *last_pick = Some(time.elapsed_secs());
    }
    // Files picked: start the import by itself once they are identified (2 s after the last one arrived), as long as
    // Super Mario 64 is among them; otherwise the IMPORT button stays the way in.
    if let Some(t) = *last_pick {
        if time.elapsed_secs() - t > 2.0 {
            *last_pick = None;
            // SM64 is required for a first import; once the game data exists any recognised ROM (a re-import of one game) starts too.
            let ready = {
                let s = shared.0.lock().unwrap();
                s.roms.iter().any(|(g, _)| g == "sm64") || (!s.roms.is_empty() && data_dir().join("assets/courses").is_dir())
            };
            if ready {
                start(&shared, &src.0);
            }
        }
    }
}

#[cfg(target_os = "ios")]
fn scan_documents(shared: &Shared, src: &Path) {
    let found = ios_host::rom_candidates(&ios_host::documents_dir());
    if found.is_empty() {
        shared.0.lock().unwrap().messages.push("No ROM files in Files > On My iPhone > Jumpman yet. Tap CHOOSE ROM FILES to pick them from anywhere instead.".into());
    }
    for f in found {
        add_rom(shared, src, f);
    }
}

/// While an import runs, Android's Back button is swallowed by `MainActivity` (it must not leave the app half way through) and
/// reported here; and the activity decides "an import is running" from this file's freshness, so it is touched every second.
fn android_guard(shared: Res<Shared>, time: Res<Time<Real>>, mut last: Local<f32>) {
    #[cfg(target_os = "android")]
    {
        let mut s = shared.0.lock().unwrap();
        if crate::android::take_back_pressed() {
            let msg = if s.phase == Phase::Running { "Back is ignored while the import runs: it finishes by itself." } else { "Use PLAY to start the game." };
            s.messages.push(msg.into());
        }
        let running = s.phase == Phase::Running;
        drop(s);
        if running && time.elapsed_secs() - *last >= 1.0 {
            *last = time.elapsed_secs();
            let _ = std::fs::write(data_dir().join(ACTIVE_MARKER), b"running");
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = (shared, time, &mut *last);
}

/// App lifecycle (iOS / Android suspend the process in the background): logged to import.log so a device log shows what happened,
/// with a note on screen and a forced redraw when the app comes back. The screen itself always renders the real state (the import
/// thread keeps running while the app is paused), so after a pause it shows where the import really is.
fn lifecycle(mut ev: MessageReader<bevy::window::AppLifecycle>, shared: Res<Shared>, mut redraw: MessageWriter<bevy::window::RequestRedraw>) {
    for e in ev.read() {
        log_line(&format!("lifecycle: {e:?}"));
        if matches!(e, bevy::window::AppLifecycle::WillResume | bevy::window::AppLifecycle::Running) {
            let mut s = shared.0.lock().unwrap();
            if s.phase == Phase::Running {
                s.messages.push("Back in the app: the import is still running.".into());
            }
            redraw.write(bevy::window::RequestRedraw);
        }
    }
}

fn drops(mut ev: MessageReader<FileDragAndDrop>, shared: Res<Shared>, src: Res<Src>) {
    for e in ev.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = e {
            add_rom(&shared, &src.0, path_buf.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn keys(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, shared: Res<Shared>, src: Res<Src>, mut exit: MessageWriter<AppExit>, overlay: Option<Res<ImportOverlay>>, mut commands: Commands) {
    // Gamepad: A extracts (Enter), B goes back (Esc), X opens the file picker and Y scans the Documents folder (the phone buttons).
    let pad = |b: GamepadButton| pads.iter().any(|p| p.just_pressed(b));
    let screen = screen_of(&shared.0.lock().unwrap());
    if keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South) {
        if screen == SetupScreen::Finished && overlay.is_none() {
            commands.queue(|world: &mut World| play_now(world));
        } else {
            start(&shared, &src.0);
        }
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if pad(GamepadButton::West) && screen != SetupScreen::Importing {
        pick_roms();
    }
    #[cfg(target_os = "ios")]
    if pad(GamepadButton::North) {
        scan_documents(&shared, &src.0);
    }
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) || pad(GamepadButton::Start) {
        if screen == SetupScreen::Importing {
            shared.0.lock().unwrap().messages.push("The import is running: wait until it finishes (nothing is left half done).".into());
        } else if overlay.is_some() {
            commands.queue(|world: &mut World| leave_overlay(world));
        } else if can_play() {
            commands.queue(|world: &mut World| play_now(world));
        } else if !MOBILE {
            exit.write(AppExit::Success);
        } else {
            shared.0.lock().unwrap().messages.push("Super Mario 64 must be imported before the game can start.".into());
        }
    }
}

fn render(
    shared: Res<Shared>,
    mut board: Single<&mut Text, (With<Board>, Without<Msgs>)>,
    mut msgs: Single<&mut Text, (With<Msgs>, Without<Board>)>,
    mut rows: Query<(&RowStatus, &mut Text), (Without<Board>, Without<Msgs>)>,
    mut fills: Query<(&BarFill, &mut Node, &mut BackgroundColor)>,
    overlay: Option<Res<ImportOverlay>>,
) {
    let s = shared.0.lock().unwrap();
    let v = build_view(&s, &view_ctx(&s, overlay.is_some()));
    drop(s);
    if board.0 != v.head {
        board.0 = v.head;
    }
    if msgs.0 != v.msgs {
        msgs.0 = v.msgs;
    }
    for (r, mut t) in &mut rows {
        if let Some((status, _, _)) = v.rows.get(r.0) {
            if t.0 != *status {
                t.0 = status.clone();
            }
        }
    }
    for (f, mut node, mut bg) in &mut fills {
        if let Some((_, frac, kind)) = v.rows.get(f.0) {
            node.width = Val::Percent(frac.clamp(0.0, 1.0) * 100.0);
            bg.0 = kind.color();
        }
    }
}

/// `JUMPMAN_SETUP_SHOT=<stem>` with `JUMPMAN_SETUP_SHOT_AT=2,5,8` (seconds since the process started) and `JUMPMAN_OFFSCREEN=WxH`: saves the
/// setup screen as `<stem>_<time>.png` (headless captures; nothing else can see this screen without a window server).
fn setup_shots(mut commands: Commands, target: Option<Res<ShotTarget>>, time: Res<Time<Real>>, mut next: Local<usize>) {
    let (Some(target), Ok(stem), Ok(list)) = (target, std::env::var("JUMPMAN_SETUP_SHOT"), std::env::var("JUMPMAN_SETUP_SHOT_AT")) else { return };
    let times: Vec<f32> = list.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    if let Some(t) = times.get(*next).filter(|t| time.elapsed_secs() > **t) {
        *next += 1;
        commands.spawn(bevy::render::view::screenshot::Screenshot::image(target.0.clone())).observe(bevy::render::view::screenshot::save_to_disk(format!("{stem}_{t}.png")));
    }
}

/// `JUMPMAN_FAKE_IMPORT` runs have nobody to tap IMPORT: press it 2 s after the screen came up (and pick the four games).
fn fake_tap(shared: Res<Shared>, src: Res<Src>, time: Res<Time<Real>>, mut local: Local<Option<f32>>) {
    if fake_import_seconds().is_none() || std::env::var_os("JUMPMAN_FAKE_IMPORT_NOTAP").is_some() {
        return;
    }
    let t0 = *local.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - t0 > 2.0 && shared.0.lock().unwrap().phase == Phase::Idle && shared.0.lock().unwrap().failed.is_empty() && shared.0.lock().unwrap().took.is_none() {
        start(&shared, &src.0);
    }
}

/// After a successful run the game starts in this same process (`boot_game`): winit allows one event loop per process and iOS
/// cannot relaunch itself, so the data is loaded into the App that is already running. After a re-import the result stays up for a
/// few seconds (long enough to read it; PLAY / Enter skips the wait).
fn finish(shared: Res<Shared>, mut commands: Commands, mut waited: Local<f32>, time: Res<Time>, overlay: Option<Res<ImportOverlay>>) {
    let mut s = shared.0.lock().unwrap();
    if s.phase != Phase::Done {
        return;
    }
    if !s.failed.is_empty() {
        // errors stay visible; the screen goes back to idle so the user can retry
        s.phase = Phase::Idle;
        *waited = 0.0;
        return;
    }
    if overlay.is_none() && !data_dir().join("assets/courses").is_dir() && !can_play() {
        s.messages.push("Super Mario 64 is required to play: import its ROM too.".into());
        s.phase = Phase::Idle;
        *waited = 0.0;
        return;
    }
    let wait = if s.reimport && overlay.is_none() { 6.0 } else { 1.2 };
    *waited += time.delta_secs();
    s.auto_in = Some((wait - *waited).max(0.0));
    if *waited > wait {
        *waited = f32::NEG_INFINITY; // queue the boot once
        s.auto_in = None;
        if overlay.is_some() {
            log_line("import complete: loading the new data into the running game");
            commands.queue(|world: &mut World| {
                end_setup(world);
                crate::reload_content(world);
            });
        } else {
            log_line("import complete: starting the game in this process");
            commands.queue(|world: &mut World| crate::boot_game(world));
        }
    }
}

/// Start the game from the setup screen with the data that is installed (PLAY / Esc / Enter after an import).
fn play_now(world: &mut World) {
    if world.contains_resource::<ImportOverlay>() || !world.contains_resource::<PendingBoot>() {
        return;
    }
    if world.get_resource::<Shared>().is_some_and(|s| s.0.lock().unwrap().phase == Phase::Running) || !can_play() {
        return;
    }
    // SKIP, PLAY ANYWAY on the update prompt: not asked again until the exporters change.
    if world.get_resource::<Shared>().is_some_and(|s| {
        let s = s.0.lock().unwrap();
        s.update_prompt && s.phase != Phase::Done
    }) {
        record_skip();
    }
    log_line("starting the game from the setup screen");
    crate::boot_game(world);
}

/// The installed data is enough to start the game.
fn can_play() -> bool {
    crate::asset_root().join("courses").is_dir()
}

/// Despawns the setup screen and hands the window to the game. Called by `crate::boot_game`.
pub fn end_setup(world: &mut World) {
    SETUP_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
    world.remove_resource::<SetupActive>();
    if world.remove_resource::<ImportOverlay>().is_some() {
        world.insert_resource(bevy::winit::WinitSettings::game());
        restore_game(world);
    }
    world.remove_resource::<Leaving>();
    let ents: Vec<Entity> = world.query_filtered::<Entity, With<SetupEntity>>().iter(world).collect();
    for e in ents {
        if let Ok(ec) = world.get_entity_mut(e) {
            ec.despawn();
        }
    }
}

/// Adds the setup screen to the game's own App (the game's plugins are already in it, dormant): the in-process replacement for
/// `run_setup`'s separate App.
pub fn add_setup(app: &mut App, roms: Vec<PathBuf>, request: crate::CourseRequest) {
    let (shared, src) = prepare_setup(&roms);
    SETUP_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
    app.set_error_handler(setup_error_handler);
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(shared).insert_resource(Src(src)).insert_resource(SetupActive).insert_resource(PendingBoot(request));
    // The game's Startup / PreStartup / PostStartup must wait for the data: only the setup screen starts now; `boot_game` runs the
    // others once the import is done.
    {
        let mut order = app.world_mut().resource_mut::<bevy::app::MainScheduleOrder>();
        order.startup_labels.clear();
        order.startup_labels.push(bevy::ecs::schedule::ScheduleLabel::intern(&SetupStartup));
    }
    register(app);
}

/// The schedules and systems of the import screen (idle until `SetupActive` exists). The screen's systems live in `Update` for the
/// setup that owns the window and in [`ImportUpdate`] for the overlay over a running game.
fn register(app: &mut App) {
    if app.world().contains_resource::<OverlayReady>() {
        return;
    }
    app.world_mut().insert_resource(OverlayReady);
    install_gates(app);
    app.add_schedule(Schedule::new(SetupStartup));
    app.add_schedule(Schedule::new(ImportUpdate));
    app.add_systems(SetupStartup, ui);
    let active = resource_exists::<SetupActive>;
    app.add_systems(Update, (drops, keys, render, finish, lifecycle, android_guard, fake_tap).run_if(active));
    app.add_systems(Update, (test_overlay, test_reimport).run_if(not(active)));
    app.add_systems(Update, leaving_tick.run_if(resource_exists::<Leaving>));
    app.add_systems(Update, setup_shots);
    app.add_systems(ImportUpdate, (drops, keys, render, finish, lifecycle, android_guard, fake_tap, setup_shots).run_if(active));
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        app.add_systems(Update, (touch_actions, poll_picked, style_buttons).run_if(active));
        app.add_systems(ImportUpdate, (touch_actions, poll_picked, style_buttons).run_if(active));
    }
    #[cfg(target_os = "ios")]
    {
        app.add_systems(Update, auto_pick.run_if(active));
        app.add_systems(ImportUpdate, auto_pick.run_if(active));
    }
}

/// Test hook: `JUMPMAN_TEST_OVERLAY_ROMS=<dir or ROM files separated by ':'>` opens the import screen over the running game 6 s after
/// the game started and imports those ROMs (what Settings > Import does after the user picks files).
fn test_overlay(world: &mut World, mut seen: Local<Option<f32>>) {
    let Some(spec) = std::env::var_os("JUMPMAN_TEST_OVERLAY_ROMS").filter(|v| !v.is_empty()) else { return };
    if !world.contains_resource::<crate::GameSim>() || world.contains_resource::<SetupActive>() {
        return;
    }
    let now = world.resource::<Time<Real>>().elapsed_secs();
    let first = *seen.get_or_insert(now);
    if now - first < 6.0 {
        return;
    }
    std::env::remove_var("JUMPMAN_TEST_OVERLAY_ROMS");
    let mut roms = Vec::new();
    for part in std::env::split_paths(&spec) {
        if part.is_dir() {
            roms.extend(rom_files_in(&part));
        } else {
            roms.push(part);
        }
    }
    begin_overlay_with(world, &roms);
}

/// Test hook: `JUMPMAN_TEST_REIMPORT=<seconds>` presses Settings > Import / re-extract ROMs that long after the game started (the
/// shell script hook `import@<t>` does the same through the menu).
fn test_reimport(world: &mut World, mut seen: Local<Option<f32>>) {
    let Some(at) = std::env::var("JUMPMAN_TEST_REIMPORT").ok().and_then(|v| v.parse::<f32>().ok()) else { return };
    if !world.contains_resource::<crate::GameSim>() || world.contains_resource::<SetupActive>() {
        return;
    }
    let now = world.resource::<Time<Real>>().elapsed_secs();
    if now - *seen.get_or_insert(now) >= at {
        std::env::remove_var("JUMPMAN_TEST_REIMPORT");
        begin_reimport(world);
    }
}

/// Lets a running game open the import screen (Settings > Import / re-extract ROMs).
pub fn add_overlay(app: &mut App) {
    register(app);
}

/// Settings > Import / re-extract ROMs: the game is turned off and the user lands on the setup screen, the one a first run shows.
///
/// * The game is switched off at once (cameras, game UI and touch controls hidden, the sim and audio no longer updated) and
///   "Leaving the game..." is shown.
/// * Android and desktop then replace the process (`restart_into_setup`): a clean process holds no course, no GPU resources and no
///   audio, and starts on the setup screen with the remembered ROMs queued. When the import is done the same process starts the game
///   on the fresh data (`boot_game`), exactly as after a first run.
/// * iOS cannot restart itself: the setup screen is laid over the switched-off game and the new data is loaded in place.
pub fn begin_reimport(world: &mut World) {
    if world.contains_resource::<SetupActive>() || world.contains_resource::<Leaving>() {
        return;
    }
    log_line("reimport requested: leaving the game");
    if source_root().or_else(|| std::env::var_os("JUMPMAN_BYOR_SRC").map(PathBuf::from)).is_none() {
        log_line("reimport: the importer files are missing");
        if let Some(mut toast) = world.get_resource_mut::<crate::hud::Toast>() {
            toast.show("IMPORT NOT AVAILABLE: IMPORTER FILES MISSING".to_string(), 3.0);
        }
        return;
    }
    begin_reimport_inner(world);
}

fn begin_reimport_inner(world: &mut World) {
    if !world.contains_resource::<OverlayReady>() {
        return;
    }
    deactivate_game(world);
    if cfg!(target_os = "ios") {
        begin_overlay_with(world, &[]);
        return;
    }
    spawn_leaving_screen(world, LEAVING_TEXT);
    world.insert_resource(Leaving { frames: 0, since: None, restarted: false });
}

/// Gives the "Leaving the game..." screen a few frames on screen, then replaces the process; if that fails, falls back to the setup screen
/// over the switched-off game.
fn leaving_tick(world: &mut World) {
    let now = world.resource::<Time<Real>>().elapsed_secs();
    let (frames, since, restarted) = {
        let mut l = world.resource_mut::<Leaving>();
        l.frames += 1;
        let since = *l.since.get_or_insert(now);
        (l.frames, since, l.restarted)
    };
    if restarted || frames < 4 || now - since < 0.4 {
        return;
    }
    let hook = world.get_resource::<RestartHook>().map_or(restart_into_setup as fn() -> Result<(), String>, |h| h.0);
    match hook() {
        Ok(()) => {
            world.resource_mut::<Leaving>().restarted = true;
            let mut q = world.query_filtered::<&mut Text, (With<SetupEntity>, Without<Board>)>();
            for mut t in q.iter_mut(world) {
                t.0 = "Restarting...\n\nThe import screen opens in a moment.".to_string();
            }
            if !cfg!(target_os = "android") {
                world.write_message(AppExit::Success);
            }
        }
        Err(e) => {
            log_line(&format!("reimport: restart failed ({e}); using the in-place import screen"));
            world.remove_resource::<Leaving>();
            let ents: Vec<Entity> = world.query_filtered::<Entity, With<SetupEntity>>().iter(world).collect();
            for e in ents {
                if let Ok(ec) = world.get_entity_mut(e) {
                    ec.despawn();
                }
            }
            if begin_overlay_with(world, &[]) {
                if let Some(s) = world.get_resource::<Shared>() {
                    s.0.lock().unwrap().messages.push("Could not restart the app; this screen runs inside it instead.".into());
                }
            }
        }
    }
}

/// Switches the running game off: its cameras stop rendering, its UI roots (HUD, menus, touch controls) are hidden. (The sim, the
/// audio feed and every other game system are stopped by the update gate / the restart.) `restore_game` undoes it.
fn deactivate_game(world: &mut World) {
    let mut hidden = GameHidden::default();
    let mut cams = world.query_filtered::<(Entity, &mut Camera), Without<SetupEntity>>();
    for (e, mut c) in cams.iter_mut(world) {
        if c.is_active {
            c.is_active = false;
            hidden.cameras.push(e);
        }
    }
    let mut roots = world.query_filtered::<(Entity, &mut Node), (Without<ChildOf>, Without<SetupEntity>)>();
    for (e, mut n) in roots.iter_mut(world) {
        if n.display != Display::None {
            hidden.roots.push((e, n.display));
            n.display = Display::None;
        }
    }
    log_line(&format!("game switched off ({} cameras, {} UI roots hidden)", hidden.cameras.len(), hidden.roots.len()));
    world.insert_resource(hidden);
}

fn restore_game(world: &mut World) {
    let Some(hidden) = world.remove_resource::<GameHidden>() else { return };
    for e in hidden.cameras {
        if let Some(mut c) = world.get_mut::<Camera>(e) {
            c.is_active = true;
        }
    }
    for (e, d) in hidden.roots {
        if let Some(mut n) = world.get_mut::<Node>(e) {
            n.display = d;
        }
    }
}

/// The main loop runs these two in place of `Update` and the fixed-step loop (see [`register`]).
#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct UpdateGate;
#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct FixedGate;

/// While the overlay is up the game's `Update` and fixed-step loop stand still and the importer's [`ImportUpdate`] runs instead; the
/// frame bookkeeping (input, UI layout, transforms, rendering) goes on. (`MainScheduleOrder` cannot be edited from inside the main
/// loop, so the order holds these gates permanently and they decide each frame.)
fn update_gate(world: &mut World) {
    use bevy::ecs::schedule::ScheduleLabel;
    let _ = world.try_run_schedule(if world.contains_resource::<ImportOverlay>() { ImportUpdate.intern() } else { Update.intern() });
}

fn fixed_gate(world: &mut World) {
    if !world.contains_resource::<ImportOverlay>() {
        let _ = world.try_run_schedule(bevy::app::RunFixedMainLoop);
    }
}

fn install_gates(app: &mut App) {
    use bevy::ecs::schedule::ScheduleLabel;
    app.add_schedule(Schedule::new(UpdateGate)).add_schedule(Schedule::new(FixedGate));
    app.add_systems(UpdateGate, update_gate).add_systems(FixedGate, fixed_gate);
    let mut order = app.world_mut().resource_mut::<bevy::app::MainScheduleOrder>();
    for (from, to) in [(Update.intern(), UpdateGate.intern()), (bevy::app::RunFixedMainLoop.intern(), FixedGate.intern())] {
        if let Some(at) = order.labels.iter().position(|l| *l == from) {
            order.labels[at] = to;
        }
    }
}

/// Opens the import screen over the running game. False when it cannot (not set up for it, or no importer sources: the caller falls
/// back to the separate setup process).
pub fn begin_overlay(world: &mut World) -> bool {
    begin_overlay_with(world, &[])
}

fn begin_overlay_with(world: &mut World, roms: &[PathBuf]) -> bool {
    if !world.contains_resource::<OverlayReady>() || world.contains_resource::<SetupActive>() {
        return world.contains_resource::<SetupActive>();
    }
    if source_root().or_else(|| std::env::var_os("JUMPMAN_BYOR_SRC").map(PathBuf::from)).is_none() {
        return false;
    }
    let (shared, src) = prepare_setup(roms);
    if !world.contains_resource::<GameHidden>() {
        deactivate_game(world);
    }
    enter_overlay(world, shared, src);
    true
}

fn enter_overlay(world: &mut World, shared: Shared, src: PathBuf) {
    log_line("import screen opened over the running game");
    world.insert_resource(shared);
    world.insert_resource(Src(src));
    world.insert_resource(SetupActive);
    world.insert_resource(ImportOverlay);
    world.insert_resource(bevy::winit::WinitSettings::continuous());
    let _ = world.try_run_schedule(SetupStartup);
}

/// Back to the game without importing (Esc / BACK TO THE GAME).
fn leave_overlay(world: &mut World) {
    end_setup(world);
    log_line("import screen closed");
    if let Some(mut ui) = world.get_resource_mut::<crate::shell::ShellUi>() {
        ui.dirty = true;
    }
}

/// Runs the setup screen. Returns once the window closes; the caller re-evaluates assets.
/// `roms` are pre-queued (from `--rom`) and start immediately.
pub fn run_setup(roms: Vec<PathBuf>) {
    run_setup_inner(roms);
}

/// Everything the setup screen needs before its first frame: the importer's source dir, queued ROMs (`--rom`, the stale re-import
/// list, an interrupted import's inbox) and the auto-start.
fn prepare_setup(roms: &[PathBuf]) -> (Shared, PathBuf) {
    // The Rust importer needs no script tree on disk (only the decomp tables, once exporters are ported): the data dir
    // stands in for the source root.
    let Some(src) = source_root().or_else(|| std::env::var_os("JUMPMAN_BYOR_SRC").map(PathBuf::from)) else {
        eprintln!("error: extraction scripts not found (set JUMPMAN_BYOR_SRC)");
        std::process::exit(1);
    };
    let reimport = REIMPORT.load(std::sync::atomic::Ordering::Relaxed);
    let update_prompt = UPDATE_PROMPT.load(std::sync::atomic::Ordering::Relaxed);
    let shared = Shared(Arc::new(Mutex::new(State { reimport, update_prompt, ..State::default() })));
    for r in roms.iter() {
        // Identify synchronously so the auto-start below sees them.
        if let Ok(Some(g)) = identify(&src, r) {
            let mut st = shared.0.lock().unwrap();
            st.roms.retain(|(x, _)| *x != g); // two dumps of one game: import it once
            st.roms.push((g, r.clone()));
        } else {
            eprintln!("warning: {} is not a supported ROM", r.display());
            let args = ["--source-root", ".", "--data-dir", ".", "--identify"].map(OsString::from).into_iter().chain([r.as_os_str().to_owned()]).collect();
            let why = match run_pipeline(&src, args) {
                Err(e) => e.to_string(),
                Ok(mut run) => {
                    let mut text = String::new();
                    let _ = std::io::Read::read_to_string(&mut run.out, &mut text);
                    text.trim().chars().rev().take(160).collect::<Vec<_>>().into_iter().rev().collect()
                }
            };
            shared.0.lock().unwrap().messages.push(format!("{}: not recognised ({why})", r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        }
    }
    // A game imported with older exporters is re-imported from the ROM it came from, if that file is still there
    // (identified by checksum again), without asking; a ROM that moved is asked for in the window.
    // A re-import (Settings > Import / re-extract ROMs) queues the remembered ROM of every game that was imported, current or not.
    let explicit = !shared.0.lock().unwrap().roms.is_empty();
    for (g, state, rom) in query_status(&src, &data_dir()) {
        if state == "stale" || (reimport && state != "missing") {
            let mut s = shared.0.lock().unwrap();
            if state == "stale" {
                s.stale.push(g.clone());
            }
            if let Some(p) = rom.filter(|_| !s.roms.iter().any(|(x, _)| *x == g)) {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                s.messages.push(format!("{} {g} from {name}", if state == "stale" { "Updating" } else { "Re-importing" }));
                s.roms.push((g, p));
            }
        }
    }
    // Games imported earlier whose remembered ROM copy is gone (the app deletes its inbox copies after an import): say so now and open
    // the picker, instead of showing an idle screen that looks dead.
    if let Some(msg) = missing_roms_message(&data_dir()).filter(|_| reimport || shared.0.lock().unwrap().roms.is_empty()) {
        log_line(&format!("re-import: {msg}"));
        let mut s = shared.0.lock().unwrap();
        s.messages.push(msg);
        s.pick_requested = MOBILE && s.roms.is_empty();
    }
    if fake_import_seconds().is_some() && shared.0.lock().unwrap().roms.is_empty() {
        // simulated import (test hook): pretend all four ROMs were chosen
        shared.0.lock().unwrap().roms = GAMES.iter().map(|(g, _)| (g.to_string(), PathBuf::from("fake.z64"))).collect();
    }
    // A previous import that never finished (the process was killed or the app closed): nothing it staged was installed (the stamp
    // is written last and the stage dir is swapped in only on success), so drop the leftovers, say so, and run it again from the
    // ROM copies the picker left behind.
    if MOBILE {
        let data = data_dir();
        let _ = std::fs::remove_dir_all(data.join(".stage"));
        if data.join(ACTIVE_MARKER).exists() {
            let _ = std::fs::remove_file(data.join(ACTIVE_MARKER));
            log_line("the previous import was interrupted; starting again from the copied ROMs");
            shared.0.lock().unwrap().messages.push("The last import was interrupted (the app was closed or paused for too long). Nothing half-written was kept; importing again.".into());
            for f in rom_files_in(&data.join("inbox")) {
                if let Ok(Some(g)) = identify(&src, &f) {
                    let mut s = shared.0.lock().unwrap();
                    s.roms.retain(|(x, _)| *x != g);
                    s.roms.push((g, f));
                }
            }
        }
    }
    // A re-import the user asked for waits on the selection step (the ROMs found are listed, IMPORT starts it); ROMs given on the command
    // line, a stale game and an interrupted import start by themselves.
    let auto = !shared.0.lock().unwrap().roms.is_empty() && (!reimport || explicit) && !(update_prompt && !explicit) && fake_import_seconds().is_none();
    if auto {
        start(&shared, &src);
    }
    (shared, src)
}

fn run_setup_inner(roms: Vec<PathBuf>) {
    let (shared, src) = prepare_setup(&roms);
    let mut app = App::new();
    // The setup screen is its own App: keep it updating at full rate focused or not, so coming back from the background (or the
    // system dialogs of the file picker) always redraws instead of waiting on a low-power timer.
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(shared).insert_resource(Src(src)).add_plugins(crate::no_pipelined_rendering(DefaultPlugins.build()).set(WindowPlugin {
        primary_window: Some(Window {
            title: "Jumpman setup".into(),
            resolution: WindowResolution::new(900, 420),
            // Phones: take the whole screen (as the game window does).
            #[cfg(any(target_os = "ios", target_os = "android"))]
            resizable: false,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            mode: bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Primary),
            ..default()
        }),
        ..default()
    }));
    app.add_systems(Startup, ui).add_systems(Update, (drops, keys, render, finish, lifecycle, android_guard, fake_tap, setup_shots));
    #[cfg(any(target_os = "ios", target_os = "android"))]
    app.add_systems(Update, (touch_actions, poll_picked, style_buttons));
    #[cfg(target_os = "ios")]
    app.add_systems(Update, auto_pick);
    app.run();
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
    #[cfg(target_os = "ios")]
    {
        // Settings > Import leaves a marker (a process cannot start itself on iOS). JUMPMAN_IOS_IMPORT_ROMS=<dir> is the
        // test hook: import every ROM-sized file in that directory without the picker (the simulator tooling cannot tap).
        let _ = std::fs::create_dir_all(data_dir());
        if ios_host::available() {
            ios_host::prepare_data_dir();
            if let Some(dir) = std::env::var_os("JUMPMAN_IOS_IMPORT_ROMS").filter(|v| !v.is_empty()) {
                roms.extend(ios_host::rom_candidates(Path::new(&dir)));
                setup = true;
            }
        }
    }
    // Settings > Import / re-extract ROMs leaves a marker, then replaces the process (see `restart_into_setup`).
    if take_setup_marker() {
        setup = true;
    }
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

    use StampState::{Current, Missing, Stale};

    #[test]
    fn launch_decision_cases() {
        let all_current = [("sm64", Current), ("mk64", Current), ("oot", Current), ("bk", Current)];
        let oot_stale = [("sm64", Current), ("mk64", Missing), ("oot", Stale), ("bk", Stale)];
        // Everything current: the title.
        assert_eq!(launch_decision(false, true, &all_current, false, false), LaunchScreen::Game);
        // A game imported earlier is stale: the update prompt, naming the stale games only (never-imported ones are not asked about).
        assert_eq!(launch_decision(false, true, &oot_stale, false, false), LaunchScreen::Update(vec!["oot".into(), "bk".into()]));
        // ...unless the player skipped it for these exporters, or the launch names its own course / screen (test hooks, CLI).
        assert_eq!(launch_decision(false, true, &oot_stale, true, false), LaunchScreen::Game);
        assert_eq!(launch_decision(false, true, &oot_stale, false, true), LaunchScreen::Game);
        // A first run (nothing installed) and an explicit setup request keep the existing import screen, skip memory or not.
        assert_eq!(launch_decision(false, false, &[], true, false), LaunchScreen::Setup);
        assert_eq!(launch_decision(false, false, &[("sm64", Missing)], false, true), LaunchScreen::Setup);
        assert_eq!(launch_decision(true, true, &all_current, true, true), LaunchScreen::Setup);
        // A dev checkout (no stamps at all) never prompts.
        assert_eq!(launch_decision(false, true, &[], false, false), LaunchScreen::Game);
        // Only missing games (never imported): nothing to update.
        assert_eq!(launch_decision(false, true, &[("sm64", Current), ("oot", Missing)], false, false), LaunchScreen::Game);
    }

    #[test]
    fn a_skipped_update_is_remembered_per_exporter_set() {
        let d = std::env::temp_dir().join(format!("jumpman-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert!(!update_skipped_in(&d, "v3:a,b"));
        std::fs::write(skip_file(&d), "v3:a,b").unwrap();
        assert!(update_skipped_in(&d, "v3:a,b"));
        assert!(!update_skipped_in(&d, "v3:a,c"), "an app update changed an exporter: ask again");
        let _ = std::fs::remove_dir_all(&d);
        assert!(!exporters_key().is_empty());
    }

    #[test]
    fn the_update_screen_names_the_games_and_both_choices() {
        let s = State { update_prompt: true, stale: vec!["oot".into(), "bk".into()], roms: vec![("oot".into(), PathBuf::from("oot.z64"))], ..State::default() };
        let ctx = |mobile| ViewCtx { overlay: false, mobile, ios: false, android: false, pad_prompts: false, playable: true, installed: [true; 4], data_dir: String::new(), elapsed: 0, quiet: 0 };
        let v = build_view(&s, &ctx(false));
        assert!(v.head.contains("UPDATE AVAILABLE") && v.head.contains("re-import needed for OOT, BK"), "{}", v.head);
        assert!(v.head.contains("SKIP, PLAY ANYWAY") && v.foot.contains("SKIP, PLAY ANYWAY") && v.foot.contains("Enter"));
        assert_eq!(v.rows[2].0, "ROM ready");
        // The same screen on a phone says UPDATE.
        let v = build_view(&s, &ctx(true));
        assert!(v.head.contains("UPDATE") && v.foot.contains("UPDATE"));
        // A plain re-import keeps its own title.
        let v = build_view(&State { reimport: true, ..State::default() }, &ctx(false));
        assert!(v.head.contains("RE-IMPORT ROMS") && !v.head.contains("UPDATE AVAILABLE"));
    }

    #[derive(Resource, Default)]
    struct GameTicks(u32);

    #[test]
    fn overlay_pauses_the_game_and_hands_it_back() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<GameTicks>();
        app.add_message::<bevy::window::AppLifecycle>().add_message::<bevy::window::RequestRedraw>().add_message::<FileDragAndDrop>().add_message::<AppExit>().init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(Update, |mut t: ResMut<GameTicks>| t.0 += 1);
        add_overlay(&mut app);
        app.update();
        app.update();
        assert_eq!(app.world().resource::<GameTicks>().0, 2);
        enter_overlay(app.world_mut(), Shared(Arc::new(Mutex::new(State::default()))), PathBuf::from("."));
        assert!(app.world().contains_resource::<ImportOverlay>() && app.world().contains_resource::<SetupActive>());
        app.update();
        app.update();
        assert_eq!(app.world().resource::<GameTicks>().0, 2, "the game does not run while the import screen is up");
        let n = app.world_mut().query_filtered::<Entity, With<SetupEntity>>().iter(app.world()).count();
        assert!(n >= 3, "camera, backdrop and text were spawned (got {n})");
        let mut text = app.world_mut().query_filtered::<&Text, With<Board>>();
        assert!(text.single(app.world()).unwrap().0.contains("IMPORT ROMS"), "the overlay's own heading");
        end_setup(app.world_mut());
        assert!(!app.world().contains_resource::<ImportOverlay>() && !app.world().contains_resource::<SetupActive>());
        assert_eq!(app.world_mut().query_filtered::<Entity, With<SetupEntity>>().iter(app.world()).count(), 0);
        app.update();
        assert_eq!(app.world().resource::<GameTicks>().0, 3, "the game runs again");
    }


    fn game_world(app: &mut App) -> (Entity, Entity, Entity) {
        // what a running game has: a 3D camera, a HUD root with a child, no setup screen yet
        let cam = app.world_mut().spawn((Camera3d::default(), Camera::default())).id();
        let root = app.world_mut().spawn(Node::default()).id();
        let child = app.world_mut().spawn((Node::default(), ChildOf(root))).id();
        (cam, root, child)
    }

    fn setup_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<GameTicks>();
        app.add_message::<bevy::window::AppLifecycle>().add_message::<bevy::window::RequestRedraw>().add_message::<FileDragAndDrop>().add_message::<AppExit>().init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(Update, |mut t: ResMut<GameTicks>| t.0 += 1);
        add_overlay(&mut app);
        app
    }

    #[test]
    fn reimport_leaves_the_game_then_restarts_into_setup() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static RESTARTS: AtomicU32 = AtomicU32::new(0);
        let mut app = setup_app();
        let (cam, root, child) = game_world(&mut app);
        app.update();
        app.world_mut().insert_resource(RestartHook(|| {
            RESTARTS.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));
        begin_reimport_inner(app.world_mut());
        // at once: the game is off (no render, HUD and touch controls hidden) and the user is told
        assert!(!app.world().get::<Camera>(cam).unwrap().is_active, "the game's camera stops rendering");
        assert_eq!(app.world().get::<Node>(root).unwrap().display, Display::None, "the game's UI is hidden");
        assert_eq!(app.world().get::<Node>(child).unwrap().display, Display::DEFAULT, "children follow their root");
        assert!(app.world().contains_resource::<Leaving>());
        let mut texts = app.world_mut().query_filtered::<&Text, With<SetupEntity>>();
        assert!(texts.iter(app.world()).any(|t| t.0.contains("Leaving the game")), "the Leaving screen is up");
        assert_eq!(RESTARTS.load(Ordering::SeqCst), 0, "not restarted before the screen had a few frames");
        // a second press while leaving does nothing
        begin_reimport(app.world_mut());
        assert_eq!(app.world_mut().query_filtered::<Entity, With<SetupEntity>>().iter(app.world()).count(), 3, "camera, backdrop, text: spawned once");
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(RESTARTS.load(Ordering::SeqCst), 0, "frames alone are not enough: it must have been visible for a moment");
        std::thread::sleep(std::time::Duration::from_millis(450));
        app.update();
        app.update();
        assert_eq!(RESTARTS.load(Ordering::SeqCst), 1, "the process is replaced exactly once");
        let mut texts = app.world_mut().query_filtered::<&Text, With<SetupEntity>>();
        assert!(texts.iter(app.world()).any(|t| t.0.contains("Restarting")), "and the screen says so");
        assert!(!app.world().resource::<Messages<AppExit>>().is_empty() || cfg!(target_os = "android"), "desktop quits after starting the new process");
    }

    #[test]
    fn game_off_state_is_given_back_when_the_overlay_closes() {
        let mut app = setup_app();
        let (cam, root, _) = game_world(&mut app);
        deactivate_game(app.world_mut());
        assert!(!app.world().get::<Camera>(cam).unwrap().is_active);
        enter_overlay(app.world_mut(), Shared(Arc::new(Mutex::new(State::default()))), PathBuf::from("."));
        let setup_cam = app.world_mut().query_filtered::<&Camera, With<SetupEntity>>().single(app.world()).unwrap().is_active;
        assert!(setup_cam, "the setup screen's own camera is not switched off");
        app.update();
        end_setup(app.world_mut());
        assert!(app.world().get::<Camera>(cam).unwrap().is_active, "the game renders again");
        assert_eq!(app.world().get::<Node>(root).unwrap().display, Display::DEFAULT);
        assert!(!app.world().contains_resource::<GameHidden>());
    }

    fn ctx(android: bool) -> ViewCtx {
        ViewCtx { overlay: false, mobile: android, ios: false, android, pad_prompts: false, playable: true, installed: [true; 4], data_dir: "/data".into(), elapsed: 0, quiet: 0 }
    }

    fn queued(s: &mut State) {
        s.roms = GAMES.iter().map(|(g, _)| (g.to_string(), PathBuf::from(format!("{g}.z64")))).collect();
    }

    #[test]
    fn setup_screen_walks_select_importing_finished() {
        // reimport arrives on the selection step with the remembered ROMs listed
        let shared = Shared(Arc::new(Mutex::new(State { reimport: true, ..State::default() })));
        queued(&mut shared.0.lock().unwrap());
        let v = build_view(&shared.0.lock().unwrap(), &ctx(true));
        assert_eq!(v.screen, SetupScreen::Select);
        assert!(v.head.contains("RE-IMPORT ROMS") && v.head.contains("Step 1 of 3") && v.head.contains("tap IMPORT"), "{}", v.head);
        assert_eq!(v.rows.len(), GAMES.len() + 1);
        assert!(v.rows[..4].iter().all(|r| r.0 == "ROM ready"));
        assert_eq!(v.rows[4].0, "4 ROMs queued");
        // import starts: a worker reports progress
        {
            let mut s = shared.0.lock().unwrap();
            s.phase = Phase::Running;
            s.started = Some(std::time::Instant::now());
        }
        let roms = shared.0.lock().unwrap().roms.clone();
        let worker = { let sh = shared.clone(); std::thread::spawn(move || run_fake(sh, roms, 0.6, None)) };
        let mut seen_mid = false;
        let mut last_overall = 0.0;
        while !worker.is_finished() {
            let s = shared.0.lock().unwrap();
            let v = build_view(&s, &ctx(true));
            if s.progress.len() == 4 {
                assert_eq!(v.screen, SetupScreen::Importing);
                assert!(v.rows[4].1 >= last_overall, "the overall bar only moves forward");
                last_overall = v.rows[4].1;
                if v.rows[4].1 > 0.1 && v.rows[4].1 < 0.9 {
                    seen_mid = true;
                    assert!(v.head.contains("Step 2 of 3"), "{}", v.head);
                    assert!(v.foot.contains("Importing") && v.foot.contains("Now: "), "{}", v.foot);
                    assert!(v.rows[0].2 == Bar::Running && v.rows[0].1 > 0.0 && v.rows[0].0.contains('%'));
                }
            }
            drop(s);
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        worker.join().unwrap();
        assert!(seen_mid, "the screen showed progress while the import ran");
        // done: everything at 100%, result step, a way to go on
        let s = shared.0.lock().unwrap();
        let v = build_view(&s, &ctx(true));
        assert_eq!(v.screen, SetupScreen::Finished);
        assert!(v.head.contains("Step 3 of 3") && v.head.contains("imported in"), "{}", v.head);
        assert!(v.rows.iter().all(|r| r.1 == 1.0 && r.2 == Bar::Done), "{:?}", v.rows.iter().map(|r| (&r.0, r.1)).collect::<Vec<_>>());
        assert!(v.foot.contains("Starting the game"), "{}", v.foot);
    }

    #[test]
    fn failed_import_shows_red_rows_and_a_way_back() {
        let shared = Shared(Arc::new(Mutex::new(State::default())));
        queued(&mut shared.0.lock().unwrap());
        shared.0.lock().unwrap().phase = Phase::Running;
        let roms = shared.0.lock().unwrap().roms.clone();
        run_fake(shared.clone(), roms, 0.2, Some("oot".into()));
        let mut s = shared.0.lock().unwrap();
        let v = build_view(&s, &ctx(true));
        assert_eq!(v.screen, SetupScreen::Failed);
        let oot = GAMES.iter().position(|(g, _)| *g == "oot").unwrap();
        assert!(v.rows[oot].0.starts_with("FAILED") && v.rows[oot].2 == Bar::Failed);
        assert_eq!(v.rows[0].2, Bar::Done, "the other games still finished");
        assert_eq!(v.rows[4].2, Bar::Failed);
        // `finish` returns the screen to idle so IMPORT can be pressed again; the failure stays visible
        s.phase = Phase::Idle;
        assert_eq!(screen_of(&s), SetupScreen::Failed);
        assert!(build_view(&s, &ctx(true)).head.contains("go back and play"));
    }

    #[test]
    fn first_run_and_overlay_headings() {
        let s = State::default();
        assert!(build_view(&s, &ctx(true)).head.starts_with("JUMPMAN SETUP"));
        let mut c = ctx(false);
        c.overlay = true;
        assert!(build_view(&s, &c).head.starts_with("IMPORT ROMS"));
    }

    #[test]
    fn setup_marker_asks_for_a_reimport_once() {
        let d = stamp_dir("marker", None);
        assert!(!take_setup_marker_in(&d));
        std::fs::write(d.join(SETUP_MARKER), "reimport").unwrap();
        assert!(take_setup_marker_in(&d));
        assert!(REIMPORT.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!d.join(SETUP_MARKER).exists(), "the request is consumed, so the next start is a normal one");
        std::fs::write(d.join(SETUP_MARKER), "1").unwrap();
        assert!(take_setup_marker_in(&d));
        assert!(!REIMPORT.load(std::sync::atomic::Ordering::Relaxed), "the old iOS marker is a plain setup request");
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn stamp_dir(name: &str, stamp: Option<&str>) -> PathBuf {
        let d = std::env::temp_dir().join(format!("byor-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("stamps")).unwrap();
        if let Some(s) = stamp {
            std::fs::write(d.join("stamps/sm64.json"), s).unwrap();
        }
        d
    }

    #[test]
    fn stamp_missing_current_and_mismatch() {
        let h = "abc123";
        let cur = format!("{{\"version\":{PIPELINE_VERSION},\"hash\":\"{h}\"}}");
        let d = stamp_dir("a", None);
        assert_eq!(stamp_state(&d, "sm64", Some(h)), StampState::Missing);
        assert!(!installed_in(&d, "sm64"));
        std::fs::write(d.join("stamps/sm64.json"), &cur).unwrap();
        assert_eq!(stamp_state(&d, "sm64", Some(h)), StampState::Current);
        assert_eq!(stamp_state(&d, "sm64", Some("other")), StampState::Stale, "exporters changed since the import");
        assert_eq!(stamp_state(&d, "sm64", None), StampState::Current, "dev checkout: no bundled hash, version only");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rust_importer_speaks_the_pipeline_protocol() {
        let d = stamp_dir("rust", None);
        let junk = d.join("junk.z64");
        std::fs::write(&junk, b"not a rom").unwrap();
        let args = ["--source-root", ".", "--data-dir", ".", "--identify"].map(OsString::from).into_iter().chain([junk.as_os_str().to_owned()]).collect();
        let mut run = run_rust(args);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut run.out, &mut text).unwrap();
        assert_eq!(text.trim(), "unknown");
        assert!(!run.finish(), "an unknown ROM exits non-zero, like pipeline.py --identify");
        // --status lists the four games, all missing in a fresh dir
        let args = ["--source-root", ".", "--data-dir"].map(OsString::from).into_iter().chain([d.as_os_str().to_owned(), "--status".into()]).collect();
        let mut run = run_rust(args);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut run.out, &mut text).unwrap();
        assert_eq!(text.lines().filter(|l| l.starts_with("@@STATUS ") && l.ends_with(" missing")).count(), 4, "{text}");
        assert!(run.finish());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn reimport_with_the_roms_gone_explains_itself() {
        // The state of an Android device after a first import: stamps and roms.json for every game, inbox empty.
        let d = stamp_dir("gone", None);
        std::fs::create_dir_all(d.join("inbox")).unwrap();
        let mut roms = serde_json::Map::new();
        for (g, _) in GAMES {
            std::fs::write(d.join("stamps").join(format!("{g}.json")), "{}").unwrap();
            roms.insert(g.into(), serde_json::json!({"path": d.join("inbox").join(format!("{g}.z64"))}));
        }
        std::fs::write(d.join("roms.json"), serde_json::Value::Object(roms).to_string()).unwrap();
        assert_eq!(games_with_missing_rom(&d).len(), 4);
        let msg = missing_roms_message(&d).expect("a message");
        assert!(msg.contains("ROM files are gone") && msg.contains("oot"), "{msg}");
        let mut s = State::default();
        explain_no_roms(&mut s, &d);
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.pick_requested, MOBILE, "phones open the picker by themselves");
        // a ROM that is still there is not reported
        std::fs::write(d.join("inbox/oot.z64"), b"x").unwrap();
        assert!(!games_with_missing_rom(&d).contains(&"oot".to_string()));
        // never imported games are not "gone"
        std::fs::remove_file(d.join("stamps/bk.json")).unwrap();
        assert!(!games_with_missing_rom(&d).contains(&"bk".to_string()));
        // nothing imported at all: the plain hint
        let e = stamp_dir("gone-none", None);
        let mut s = State::default();
        explain_no_roms(&mut s, &e);
        assert!(s.messages[0].contains("No ROM"));
        std::fs::remove_dir_all(&d).unwrap();
        std::fs::remove_dir_all(&e).unwrap();
    }

    #[test]
    fn importer_failures_reach_the_screen_protocol() {
        let mut run = run_rust(vec!["--bogus".into()]);
        let mut text = String::new();
        std::io::Read::read_to_string(&mut run.out, &mut text).unwrap();
        assert!(text.contains("@@FAIL all"), "{text}");
    }

    #[test]
    fn old_format_and_broken_stamps_are_stale_not_missing() {
        // pipeline v2 stamps had no hash and no file list
        let d = stamp_dir("b", Some("{\"version\":2,\"game\":\"sm64\",\"time\":1}"));
        assert_eq!(stamp_state(&d, "sm64", Some("abc")), StampState::Stale);
        assert_eq!(stamp_state(&d, "sm64", None), StampState::Stale, "an older version is stale even without a bundled hash");
        std::fs::write(d.join("stamps/sm64.json"), format!("{{\"version\":{PIPELINE_VERSION}}}")).unwrap();
        assert_eq!(stamp_state(&d, "sm64", Some("abc")), StampState::Stale, "current version but no recorded hash");
        std::fs::write(d.join("stamps/sm64.json"), "not json").unwrap();
        assert_eq!(stamp_state(&d, "sm64", None), StampState::Stale);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_importer_is_offered_by_a_byor_bundle_an_earlier_import_or_an_explicit_source() {
        assert!(!importer_available_for(false, false, false), "a plain dev build or a baked app without byor-src");
        assert!(importer_available_for(true, false, false), "the Mac app bundling byor-src next to baked assets");
        assert!(importer_available_for(false, true, false), "data imported before");
        assert!(importer_available_for(false, false, true), "JUMPMAN_BYOR_SRC");
    }

    #[test]
    fn game_status_labels_distinguish_builtin_data_from_missing_data() {
        assert_eq!(status_label(Current, false), "INSTALLED");
        assert_eq!(status_label(Current, true), "INSTALLED");
        assert_eq!(status_label(Stale, true), "UPDATE AVAILABLE");
        assert_eq!(status_label(Missing, true), "BUILT-IN");
        assert_eq!(status_label(Missing, false), "NOT IMPORTED");
    }

    #[test]
    fn baked_assets_without_stamps_never_prompt_for_an_update() {
        // The local Mac app: baked data, no stamps yet -> nothing to update.
        assert_eq!(launch_decision(false, true, &[], false, false), LaunchScreen::Game);
        // After one game was imported, only that game can be stale; the others (Missing) stay quiet.
        let one = [("sm64", Stale), ("mk64", Missing), ("oot", Missing), ("bk", Missing)];
        assert_eq!(launch_decision(false, true, &one, false, false), LaunchScreen::Update(vec!["sm64".into()]));
    }
}
