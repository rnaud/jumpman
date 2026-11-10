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

/// Assets in the data dir were produced by an older pipeline (e.g. before music was added): run setup again.
pub fn needs_update(root: &Path) -> bool {
    root.starts_with(data_dir()) && data_dir().join("stamps").is_dir() && !installed("sm64")
}

/// Settings > Import / re-extract ROMs: start a second copy of the game in setup mode and quit this one.
/// (iOS cannot start a process: it leaves a marker that the next launch turns into the setup screen, and quits.)
pub fn reopen_setup(exit: &mut MessageWriter<AppExit>) {
    if MOBILE {
        let _ = std::fs::write(data_dir().join(".setup-requested"), b"1");
        std::process::exit(0);
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("jumpman"));
    match Command::new(exe).arg("--setup").spawn() {
        Ok(_) => {
            exit.write(AppExit::Success);
        }
        Err(e) => eprintln!("could not reopen setup: {e}"),
    }
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
    let worker = std::thread::spawn(move || byor_rs::cli::run(args, &mut ChanWriter(tx)));
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

fn start(shared: &Shared, src: &Path) {
    let (roms, data) = {
        let mut s = shared.0.lock().unwrap();
        if s.phase == Phase::Running || s.roms.is_empty() {
            return;
        }
        s.phase = Phase::Running;
        s.failed.clear();
        s.started = Some(std::time::Instant::now());
        s.last_progress = Some(std::time::Instant::now());
        (s.roms.clone(), data_dir())
    };
    let _ = std::fs::write(data.join(ACTIVE_MARKER), roms.iter().map(|(g, _)| g.as_str()).collect::<Vec<_>>().join(" "));
    log_line(&format!("import started: {}", roms.iter().map(|(g, _)| g.as_str()).collect::<Vec<_>>().join(", ")));
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
                        s.progress.insert(g.into(), (p.parse().unwrap_or(0), m.into()));
                    }
                } else if let Some(g) = line.strip_prefix("@@DONE ") {
                    s.progress.insert(g.into(), (100, "installed".into()));
                } else if let Some(rest) = line.strip_prefix("@@FAIL ") {
                    let (g, m) = rest.split_once(' ').unwrap_or((rest, ""));
                    s.failed.insert(g.into(), m.into());
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
        shared.0.lock().unwrap().phase = Phase::Done;
    });
}

#[derive(Component)]
struct Board;

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

/// Present while the import screen is laid over a RUNNING game (Settings > Import / re-extract ROMs): the game keeps its state and
/// is not drawn over; when the import ends `crate::reload_content` picks the new data up and the screen is removed.
#[derive(Resource)]
pub struct ImportOverlay;

/// Marks an App whose game can open the import screen in place (see [`add_overlay`]).
#[derive(Resource)]
pub struct OverlayReady;

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

fn ui(mut commands: Commands, overlay: Option<Res<ImportOverlay>>) {
    // Above every camera of the game when it runs underneath (the overlay), plain when the setup screen is alone.
    let cam = commands.spawn((Camera2d, Camera { order: 1000, ..default() }, SetupEntity)).id();
    let ios = MOBILE;
    if overlay.is_some() {
        commands.spawn((
            SetupEntity,
            UiTargetCamera(cam),
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            BackgroundColor(Color::srgb(0.02, 0.02, 0.06)),
            GlobalZIndex(-1),
        ));
    }
    commands.spawn((
        Board,
        SetupEntity,
        UiTargetCamera(cam),
        Text::new(""),
        TextFont { font_size: FontSize::Px(if ios { 15.0 } else { 18.0 }), ..default() },
        TextColor(Color::WHITE),
        // iOS: clear of the notch / Dynamic Island in landscape and of the buttons along the bottom
        Node { position_type: PositionType::Absolute, left: Val::Px(if ios { 60.0 } else { 24.0 }), top: Val::Px(if ios { 28.0 } else { 20.0 }), right: Val::Px(if ios { 60.0 } else { 24.0 }), ..default() },
    ));
    #[cfg(any(target_os = "ios", target_os = "android"))]
    touch_buttons(&mut commands, cam, overlay.is_some());
}

/// Phone touch controls: the picker, (iOS) a Documents scan, Import and (iOS) Cancel (the macOS window is driven by
/// drag-and-drop and Enter).
#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Component, Clone, Copy)]
enum Act {
    Pick,
    #[cfg(target_os = "ios")]
    Scan,
    Start,
    /// Back to the running game (only while the import screen is laid over it).
    Back,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn touch_buttons(commands: &mut Commands, cam: Entity, overlay: bool) {
    commands
        .spawn((SetupEntity, UiTargetCamera(cam), Node { position_type: PositionType::Absolute, left: Val::Px(60.0), right: Val::Px(60.0), bottom: Val::Px(28.0), column_gap: Val::Px(12.0), ..default() }))
        .with_children(|row| {
            #[cfg(target_os = "ios")]
            let buttons = [(Act::Pick, "CHOOSE ROM FILES"), (Act::Scan, "SCAN JUMPMAN FOLDER"), (Act::Start, "IMPORT")];
            #[cfg(target_os = "android")]
            let buttons = [(Act::Pick, "CHOOSE ROM FILES"), (Act::Start, "IMPORT")];
            let back = overlay.then_some((Act::Back, "BACK TO THE GAME"));
            for (act, label) in buttons.into_iter().chain(back) {
                row.spawn((
                    Interaction::default(),
                    act,
                    Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)), ..default() },
                    BackgroundColor(Color::srgb(0.2, 0.3, 0.55)),
                ))
                .with_child((Text::new(label), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::WHITE)));
            }
        });
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn touch_actions(q: Query<(&Interaction, &Act), Changed<Interaction>>, shared: Res<Shared>, src: Res<Src>, mut commands: Commands) {
    for (i, act) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match act {
            Act::Pick => pick_roms(),
            #[cfg(target_os = "ios")]
            Act::Scan => scan_documents(&shared, &src.0),
            Act::Start => start(&shared, &src.0),
            Act::Back => {
                if shared.0.lock().unwrap().phase != Phase::Running {
                    commands.queue(|world: &mut World| leave_overlay(world));
                }
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
    while let Some(p) = picked_pop() {
        add_rom(&shared, &src.0, p);
        *last_pick = Some(time.elapsed_secs());
    }
    // Files picked: start the import by itself once they are identified (2 s after the last one arrived), as long as
    // Super Mario 64 is among them; otherwise the IMPORT button stays the way in.
    if let Some(t) = *last_pick {
        if time.elapsed_secs() - t > 2.0 {
            *last_pick = None;
            let ready = shared.0.lock().unwrap().roms.iter().any(|(g, _)| g == "sm64");
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

/// App lifecycle (iOS suspends the process in the background): logged to import.log so a device log shows what happened, with a
/// note on screen and a forced redraw when the app comes back.
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
    if keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South) {
        start(&shared, &src.0);
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if pad(GamepadButton::West) {
        pick_roms();
    }
    #[cfg(target_os = "ios")]
    if pad(GamepadButton::North) {
        scan_documents(&shared, &src.0);
    }
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) {
        if overlay.is_none() {
            exit.write(AppExit::Success);
        } else if shared.0.lock().unwrap().phase != Phase::Running {
            commands.queue(|world: &mut World| leave_overlay(world));
        }
    }
}

fn render(shared: Res<Shared>, mut board: Single<&mut Text, With<Board>>, overlay: Option<Res<ImportOverlay>>) {
    let overlay = overlay.is_some();
    let pad_prompts = crate::hints::pad_active();
    let s = shared.0.lock().unwrap();
    let ios = MOBILE;
    let mut t = String::from(if cfg!(target_os = "android") {
        "JUMPMAN SETUP\nImport your own US ROMs (.z64 / .v64 / .n64): tap CHOOSE ROM FILES, then IMPORT.\n\n"
    } else if ios {
        "JUMPMAN SETUP\n\nThis app ships without any Nintendo game data: import your own legally-dumped US ROMs (.z64 / .v64 / .n64).\nTap CHOOSE ROM FILES and select them in Files (iCloud Drive, Downloads, On My iPhone...). You can pick several at once.\nOr copy them into Files > On My iPhone > Jumpman and tap SCAN JUMPMAN FOLDER.\nKeep the app open while importing (the screen will not lock).\n\n"
    } else {
        "JUMPMAN SETUP\n\nThis app ships without any Nintendo game data.\nDrop your own legally-dumped US ROMs on this window (.z64 / .v64 / .n64).\n\n"
    });
    if overlay {
        t = t.replacen("JUMPMAN SETUP", "IMPORT ROMS", 1);
    }
    if cfg!(target_os = "android") {
        t += &format!("Or copy a Mac BYOR data folder to {}\n", data_dir().display());
    }
    let have = data_dir();
    for (g, label) in GAMES {
        let status = if let Some(f) = s.failed.get(g) {
            format!("FAILED: {}", f.chars().take(if ios { 60 } else { 110 }).collect::<String>())
        } else if let Some((p, m)) = s.progress.get(g) {
            format!("{p:>3}%  {m}")
        } else if s.roms.iter().any(|(x, _)| x == g) {
            "ROM ready".into()
        } else if installed_in(&have, g) {
            "installed".into()
        } else if s.stale.iter().any(|x| x == g) {
            "Update available - re-import (drop the ROM again)".into()
        } else if ios {
            "not imported".into()
        } else {
            "drop ROM here".into()
        };
        t += &format!("  {label:<54}{status}\n");
    }
    t += "\n";
    for m in s.messages.iter().rev().take(4) {
        t += &format!("{m}\n");
    }
    t += match (&s.phase, ios) {
        (Phase::Idle, true) if s.roms.is_empty() => "\nWaiting for a ROM.\n",
        (Phase::Idle, true) => "\nTap IMPORT to extract.\n",
        (Phase::Idle, false) if s.roms.is_empty() && overlay && pad_prompts => "\nWaiting for a ROM.  B goes back to the game.\n",
        (Phase::Idle, false) if !s.roms.is_empty() && overlay && pad_prompts => "\nPress A to extract.  B goes back to the game.\n",
        (Phase::Idle, false) if !s.roms.is_empty() && pad_prompts => "\nPress A to extract.\n",
        (Phase::Idle, false) if s.roms.is_empty() && overlay => "\nWaiting for a ROM.  Esc goes back to the game.\n",
        (Phase::Idle, false) if s.roms.is_empty() => "\nWaiting for a ROM.  Esc quits.\n",
        (Phase::Idle, false) if overlay => "\nPress Enter to extract.  Esc goes back to the game.\n",
        (Phase::Idle, false) => "\nPress Enter to extract.\n",
        (Phase::Running, _) => "\nExtracting...\n",
        (Phase::Done, _) if s.failed.is_empty() && overlay => "\nImport complete!  Loading the new games...\n",
        (Phase::Done, _) if s.failed.is_empty() => "\nImport complete!  Starting the game...\n",
        (Phase::Done, _) => "\nSome imports failed (see above). Fix and tap IMPORT again.\n",
    };
    if s.phase == Phase::Running {
        let (since, quiet) = (s.started.map_or(0, |t| t.elapsed().as_secs()), s.last_progress.map_or(0, |t| t.elapsed().as_secs()));
        t += &format!("Running for {since} s");
        if quiet >= 8 {
            t += &format!("; no progress for {quiet} s (a big step, or the system paused the app: it continues by itself)");
        }
        t += "\n";
    }
    board.0 = t;
}

/// After a successful run the game starts in this same process (`boot_game`): winit allows one event loop per process and iOS
/// cannot relaunch itself, so the data is loaded into the App that is already running.
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
    if overlay.is_none() && !data_dir().join("assets/courses").is_dir() {
        s.messages.push("Super Mario 64 is required to play: import its ROM too.".into());
        s.phase = Phase::Idle;
        *waited = 0.0;
        return;
    }
    *waited += time.delta_secs();
    if *waited > 1.2 {
        *waited = f32::NEG_INFINITY; // queue the boot once
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

/// Despawns the setup screen and hands the window to the game. Called by `crate::boot_game`.
pub fn end_setup(world: &mut World) {
    SETUP_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
    world.remove_resource::<SetupActive>();
    if world.remove_resource::<ImportOverlay>().is_some() {
        world.insert_resource(bevy::winit::WinitSettings::game());
    }
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
    app.add_systems(Update, (drops, keys, render, finish, lifecycle).run_if(active));
    app.add_systems(Update, test_overlay.run_if(not(active)));
    app.add_systems(ImportUpdate, (drops, keys, render, finish, lifecycle).run_if(active));
    #[cfg(target_os = "ios")]
    {
        app.add_systems(Update, (touch_actions, poll_picked, auto_pick).run_if(active));
        app.add_systems(ImportUpdate, (touch_actions, poll_picked, auto_pick).run_if(active));
    }
    #[cfg(target_os = "android")]
    {
        app.add_systems(Update, (touch_actions, poll_picked).run_if(active));
        app.add_systems(ImportUpdate, (touch_actions, poll_picked).run_if(active));
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

/// Lets a running game open the import screen in place (Settings > Import / re-extract ROMs).
pub fn add_overlay(app: &mut App) {
    register(app);
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
    let shared = Shared(Arc::new(Mutex::new(State::default())));
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
    for (g, state, rom) in query_status(&src, &data_dir()) {
        if state == "stale" {
            let mut s = shared.0.lock().unwrap();
            s.stale.push(g.clone());
            if let Some(p) = rom.filter(|_| !s.roms.iter().any(|(x, _)| *x == g)) {
                s.messages.push(format!("Updating {g} from {}", p.display()));
                s.roms.push((g, p));
            }
        }
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
    let auto = !shared.0.lock().unwrap().roms.is_empty();
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
    app.insert_resource(shared).insert_resource(Src(src)).add_plugins(DefaultPlugins.set(WindowPlugin {
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
    app.add_systems(Startup, ui).add_systems(Update, (drops, keys, render, finish, lifecycle));
    #[cfg(target_os = "ios")]
    app.add_systems(Update, (touch_actions, poll_picked, auto_pick));
    #[cfg(target_os = "android")]
    app.add_systems(Update, (touch_actions, poll_picked));
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
            let marker = data_dir().join(".setup-requested");
            if marker.is_file() {
                let _ = std::fs::remove_file(marker);
                setup = true;
            }
            if let Some(dir) = std::env::var_os("JUMPMAN_IOS_IMPORT_ROMS").filter(|v| !v.is_empty()) {
                roms.extend(ios_host::rom_candidates(Path::new(&dir)));
                setup = true;
            }
        }
    }
    #[cfg(target_os = "android")]
    {
        // Settings > Import leaves a marker (the process is not restarted in place on a phone).
        let marker = data_dir().join(".setup-requested");
        if marker.is_file() {
            let _ = std::fs::remove_file(marker);
            setup = true;
        }
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
    use bevy::ecs::schedule::ScheduleLabel;

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
}
