//! Bring-your-own-ROM setup. The shipped app contains no Nintendo-derived data: on first launch
//! (or `jumpman --setup`) a small separate Bevy app asks for ROMs (drag and drop, or `--rom PATH`),
//! extracts the assets into the per-user data dir (in process with the pure-Rust `byor-rs` crate for every game that is
//! ported, else with `byor/pipeline.py`), then re-executes the game. See `native/BYOR.md`.
use bevy::prelude::*;
use bevy::window::WindowResolution;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "ios")]
#[path = "byor_ios.rs"]
pub mod ios_host;

/// Phones (iOS, Android): touch UI, a file picker instead of drag and drop, and no re-exec after the import.
const MOBILE: bool = cfg!(any(target_os = "ios", target_os = "android"));

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Whether the import of `game` runs in-process with the pure-Rust `byor-rs` crate instead of `byor/pipeline.py`:
/// on every platform for each game whose exporters are all ported (`byor_rs::pipeline::PORTED`, verified byte for byte
/// against the Python pipeline), always on Android (no Python there), and for every game with `JUMPMAN_BYOR_RUST=1`.
/// `JUMPMAN_BYOR_PYTHON=1` (not on Android) forces the Python pipeline, the reference implementation, for comparisons.
fn rust_for(game: &str) -> bool {
    if env_flag("JUMPMAN_BYOR_PYTHON") && !cfg!(target_os = "android") {
        return false;
    }
    cfg!(target_os = "android") || env_flag("JUMPMAN_BYOR_RUST") || byor_rs::pipeline::exporter_hash(game).is_some()
}

/// True while some game still has to go through the Python pipeline (python3 on desktop, the embedded interpreter on iOS).
/// TODO(retire-python): once `byor_rs::pipeline::PORTED` has all four games this is false everywhere and the Python
/// pieces can go: see "Retiring Python" in native/BYOR.md.
fn python_needed() -> bool {
    GAMES.iter().any(|(g, _)| !rust_for(g))
}

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

/// The exporter hashes of the bundle (`byor-src/exporters.json`, written by package-byor.sh).
fn bundled_hash(game: &str) -> Option<String> {
    if rust_for(game) {
        return byor_rs::pipeline::exporter_hash(game);
    }
    static HASHES: std::sync::OnceLock<Option<serde_json::Value>> = std::sync::OnceLock::new();
    HASHES
        .get_or_init(|| std::fs::read_to_string(source_root()?.join("exporters.json")).ok().and_then(|s| serde_json::from_str(&s).ok()))
        .as_ref()
        .and_then(|v| v["games"][game].as_str().map(str::to_string))
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
    let has = |d: &Path, rel: &str| d.join(rel).join("byor/pipeline.py").is_file() || d.join(rel).join("exporters.json").is_file();
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

/// Must match `VERSION` in `byor/pipeline.py`; bump both to force re-extraction of every game. (Exporter changes
/// need no bump: the per-game hashes in the stamps catch them.)
pub const PIPELINE_VERSION: u64 = 3;

/// Where `byor/pipeline.py`, `scripts/` and `reference/` live: `JUMPMAN_BYOR_SRC`, the app bundle
/// (`Contents/Resources/byor-src`), next to the executable, or the repo checkout.
pub fn source_root() -> Option<PathBuf> {
    // The Rust importer needs `reference/` (the decomp files) but no script tree; Python is only required while some game lacks a port.
    let ok = |p: &PathBuf| p.join("native/byor/pipeline.py").is_file() || p.join("byor/pipeline.py").is_file() || (!python_needed() && p.join("reference").is_dir());
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
    /// Games imported with older exporters.
    stale: Vec<String>,
}

#[derive(Resource, Clone)]
struct Shared(Arc<Mutex<State>>);

#[derive(Resource)]
struct Src(PathBuf);

/// One run of `byor/pipeline.py`: its stdout (the `@@` protocol) and a way to wait for the exit status. macOS/desktop
/// spawn `python3`; iOS has no processes, so the pipeline runs inside the embedded interpreter (`ios_host`, jm_import.m)
/// with its stdout redirected to a pipe, which makes both look the same to the readers below.
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

/// One run of the importer: the Rust one in process (`rust`), else `byor/pipeline.py`.
fn run_pipeline(src: &Path, args: Vec<OsString>, rust: bool) -> std::io::Result<PipelineRun> {
    if rust {
        let _ = src;
        return Ok(run_rust(args));
    }
    #[cfg(target_os = "ios")]
    {
        ios_host::run(src, args)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let mut child = Command::new("python3").arg("-I").arg(pipeline_path(src)).args(args).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
        let out = child.stdout.take().unwrap();
        Ok(PipelineRun { out: Box::new(out), finish: Box::new(move || child.wait().is_ok_and(|s| s.success())) })
    }
}

fn run_to_string(src: &Path, args: Vec<OsString>, rust: bool) -> Option<(String, bool)> {
    let mut run = run_pipeline(src, args, rust).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut run.out, &mut text).ok()?;
    Some((text, run.finish()))
}

/// `Ok(Some(game))`, `Ok(None)` for a file that is not a supported ROM, `Err` when the importer itself could not run (so
/// the screen does not blame a good ROM for a broken runtime).
fn identify(src: &Path, rom: &Path) -> Result<Option<String>, String> {
    let args = ["--source-root", ".", "--data-dir", ".", "--identify"].map(OsString::from).into_iter().chain([rom.as_os_str().to_owned()]).collect();
    // Identification needs no Python: the Rust importer recognises all four ROMs (any byte order).
    let mut run = run_pipeline(src, args, true).map_err(|e| e.to_string())?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut run.out, &mut text).map_err(|e| e.to_string())?;
    Ok(run.finish().then(|| text.trim().to_string()).filter(|g| !g.is_empty()))
}

/// `--status`: per game `(state, remembered ROM path if the file is still there and identifies)`. The games handled by the
/// Rust importer are answered by it; only the others (while there are any) ask the Python pipeline.
fn query_status(src: &Path, data: &Path) -> Vec<(String, String, Option<PathBuf>)> {
    fn parse(text: &str, keep: impl Fn(&str) -> bool) -> Vec<(String, String, Option<PathBuf>)> {
        text.lines()
            .filter_map(|l| {
                let mut it = l.strip_prefix("@@STATUS ")?.splitn(3, ' ');
                let (g, state) = (it.next()?.to_string(), it.next()?.to_string());
                keep(&g).then(|| (g, state, it.next().filter(|p| !p.is_empty()).map(PathBuf::from)))
            })
            .collect()
    }
    let args = || -> Vec<OsString> { vec!["--source-root".into(), src.as_os_str().to_owned(), "--data-dir".into(), data.as_os_str().to_owned(), "--status".into()] };
    let mut out = run_to_string(src, args(), true).map(|(t, _)| parse(&t, rust_for)).unwrap_or_default();
    if python_needed() {
        if let Some((text, _)) = run_to_string(src, args(), false) {
            out.extend(parse(&text, |g| !rust_for(g)));
        }
    }
    out
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
        (s.roms.clone(), data_dir())
    };
    let (shared, src) = (shared.clone(), src.to_path_buf());
    std::thread::spawn(move || {
        #[cfg(target_os = "ios")]
        ios_host::keep_awake(true);
        use std::io::BufRead;
        #[cfg(target_os = "ios")]
        let mut log = std::fs::File::create(data.join("import.log")).ok();
        // The games the Rust importer covers run first, in process; whatever is left goes through the Python pipeline.
        let (rust_roms, python_roms): (Vec<_>, Vec<_>) = roms.iter().cloned().partition(|(g, _)| rust_for(g));
        for (rust, group) in [(true, rust_roms), (false, python_roms)] {
            if group.is_empty() {
                continue;
            }
            let mut args: Vec<OsString> = vec!["--source-root".into(), src.as_os_str().to_owned(), "--data-dir".into(), data.as_os_str().to_owned()];
            for (g, p) in &group {
                args.push(format!("--{g}").into());
                args.push(p.as_os_str().to_owned());
            }
            let mut child = match run_pipeline(&src, args, rust) {
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
                #[cfg(target_os = "ios")]
                if let Some(f) = log.as_mut() {
                    use std::io::Write;
                    let _ = writeln!(f, "{line}");
                }
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
        shared.0.lock().unwrap().phase = Phase::Done;
    });
}

#[derive(Component)]
struct Board;

fn ui(mut commands: Commands, shared: Res<Shared>) {
    commands.spawn(Camera2d);
    let ios = MOBILE;
    commands.spawn((
        Board,
        Text::new(""),
        TextFont { font_size: FontSize::Px(if ios { 15.0 } else { 18.0 }), ..default() },
        TextColor(Color::WHITE),
        // iOS: clear of the notch / Dynamic Island in landscape and of the buttons along the bottom
        Node { position_type: PositionType::Absolute, left: Val::Px(if ios { 60.0 } else { 24.0 }), top: Val::Px(if ios { 28.0 } else { 20.0 }), right: Val::Px(if ios { 60.0 } else { 24.0 }), ..default() },
    ));
    let _ = shared;
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
    #[cfg(target_os = "ios")]
    Cancel,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn touch_buttons(mut commands: Commands) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, left: Val::Px(60.0), right: Val::Px(60.0), bottom: Val::Px(28.0), column_gap: Val::Px(12.0), ..default() })
        .with_children(|row| {
            #[cfg(target_os = "ios")]
            let buttons = [(Act::Pick, "CHOOSE ROM FILES"), (Act::Scan, "SCAN JUMPMAN FOLDER"), (Act::Start, "IMPORT"), (Act::Cancel, "CANCEL")];
            #[cfg(target_os = "android")]
            let buttons = [(Act::Pick, "CHOOSE ROM FILES"), (Act::Start, "IMPORT")];
            for (act, label) in buttons {
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
fn touch_actions(q: Query<(&Interaction, &Act), Changed<Interaction>>, shared: Res<Shared>, src: Res<Src>) {
    for (i, act) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match act {
            Act::Pick => pick_roms(),
            #[cfg(target_os = "ios")]
            Act::Scan => scan_documents(&shared, &src.0),
            Act::Start => start(&shared, &src.0),
            #[cfg(target_os = "ios")]
            Act::Cancel => {
                ios_host::cancel();
                shared.0.lock().unwrap().messages.push("Cancelling... the previous install stays as it was.".into());
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

fn drops(mut ev: MessageReader<FileDragAndDrop>, shared: Res<Shared>, src: Res<Src>) {
    for e in ev.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = e {
            add_rom(&shared, &src.0, path_buf.clone());
        }
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, shared: Res<Shared>, src: Res<Src>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Enter) {
        start(&shared, &src.0);
    }
    if keys.just_pressed(KeyCode::KeyI) && shared.0.lock().unwrap().tools_missing && python_needed() {
        let _ = Command::new("xcode-select").arg("--install").spawn();
    }
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn render(shared: Res<Shared>, mut board: Single<&mut Text, With<Board>>) {
    let s = shared.0.lock().unwrap();
    let ios = MOBILE;
    let mut t = String::from(if cfg!(target_os = "android") {
        "JUMPMAN SETUP\nImport your own US ROMs (.z64 / .v64 / .n64): tap CHOOSE ROM FILES, then IMPORT.\n\n"
    } else if ios {
        "JUMPMAN SETUP\n\nThis app ships without any Nintendo game data: import your own legally-dumped US ROMs (.z64 / .v64 / .n64).\nTap CHOOSE ROM FILES and select them in Files (iCloud Drive, Downloads, On My iPhone...). You can pick several at once.\nOr copy them into Files > On My iPhone > Jumpman and tap SCAN JUMPMAN FOLDER.\nKeep the app open while importing (the screen will not lock).\n\n"
    } else {
        "JUMPMAN SETUP\n\nThis app ships without any Nintendo game data.\nDrop your own legally-dumped US ROMs on this window (.z64 / .v64 / .n64).\n\n"
    });
    if cfg!(target_os = "android") {
        t += &format!("Or copy a Mac BYOR data folder to {}\n", data_dir().display());
    }
    if s.tools_missing && s.roms.iter().any(|(g, _)| !rust_for(g)) {
        t += "Xcode Command Line Tools are missing. Press I to install them, then relaunch.\n\n";
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
        (Phase::Idle, false) if s.roms.is_empty() => "\nWaiting for a ROM.  Esc quits.\n",
        (Phase::Idle, false) => "\nPress Enter to extract.\n",
        (Phase::Running, _) => "\nExtracting (a few minutes for Super Mario 64)...\n",
        (Phase::Done, true) if s.failed.is_empty() => "\nImport complete!  Swipe up to close Jumpman, then open it again to play.\n",
        (Phase::Done, true) => "\nSome imports failed (see above). Fix and tap IMPORT again.\n",
        (Phase::Done, false) => "\nFinished. Restarting the game...\n",
    };
    board.0 = t;
}

/// After a successful run, re-exec the game so it comes up with the new assets in a fresh process
/// (winit allows only one event loop per process).
fn finish(shared: Res<Shared>, mut exit: MessageWriter<AppExit>, mut waited: Local<f32>, time: Res<Time>) {
    let mut s = shared.0.lock().unwrap();
    if s.phase != Phase::Done {
        return;
    }
    if MOBILE {
        // iOS: winit allows one event loop per process and a process cannot start itself, so after a clean import the next
        // launch loads the new data. The app stays on the "import complete" screen (quitting by itself looked like a crash);
        // the user closes it. After a failure the screen goes back to idle (errors visible, retry possible).
        if !s.failed.is_empty() {
            s.phase = Phase::Idle;
            *waited = 0.0;
        }
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
    run_setup_inner(roms);
}

fn run_setup_inner(roms: Vec<PathBuf>) {
    // The Rust importer needs no script tree on disk (only the decomp tables, once exporters are ported): the data dir
    // stands in for the source root.
    let Some(src) = source_root().or_else(|| (!python_needed()).then(|| std::env::var_os("JUMPMAN_BYOR_SRC").map(PathBuf::from).unwrap_or_else(data_dir))) else {
        eprintln!("error: extraction scripts not found (set JUMPMAN_BYOR_SRC)");
        std::process::exit(1);
    };
    let shared = Shared(Arc::new(Mutex::new(State { tools_missing: python_needed() && !have_build_tools(), ..default() })));
    for r in roms.iter() {
        // Identify synchronously so the auto-start below sees them.
        if let Ok(Some(g)) = identify(&src, r) {
            shared.0.lock().unwrap().roms.push((g, r.clone()));
        } else {
            eprintln!("warning: {} is not a supported ROM", r.display());
            let args = ["--source-root", ".", "--data-dir", ".", "--identify"].map(OsString::from).into_iter().chain([r.as_os_str().to_owned()]).collect();
            let why = match run_pipeline(&src, args, true) {
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
    let auto = !shared.0.lock().unwrap().roms.is_empty();
    if auto {
        start(&shared, &src);
    }
    let mut app = App::new();
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
    app.add_systems(Startup, ui).add_systems(Update, (drops, keys, render, finish));
    #[cfg(target_os = "ios")]
    app.add_systems(Startup, touch_buttons).add_systems(Update, (touch_actions, poll_picked, auto_pick));
    #[cfg(target_os = "android")]
    app.add_systems(Startup, touch_buttons).add_systems(Update, (touch_actions, poll_picked));
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
    fn ported_games_use_the_rust_importer_everywhere() {
        // SM64 and OoT are fully ported; their stamp hash is the Rust one, not the bundled exporters.json
        for g in ["sm64", "oot"] {
            assert!(rust_for(g) || env_flag("JUMPMAN_BYOR_PYTHON"), "{g}");
            if rust_for(g) {
                assert_eq!(bundled_hash(g), byor_rs::pipeline::exporter_hash(g));
            }
        }
        assert_eq!(python_needed(), GAMES.iter().any(|(g, _)| byor_rs::pipeline::exporter_hash(g).is_none()) && !cfg!(target_os = "android") && !env_flag("JUMPMAN_BYOR_RUST"));
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
