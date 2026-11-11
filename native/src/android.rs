//! Android glue (compiled only for `target_os = "android"`, see `native/ANDROID.md`).
//!
//! * `init_env`: points the data / save / config dirs at the app's own storage (Android has no `HOME`).
//! * Gamepad bridge: `MainActivity.kt` forwards `KeyEvent` / `MotionEvent`s from gamepads and joysticks through JNI
//!   (`Native.padEvent`); they are queued here and drained by `controller::poll_native`, which feeds the same
//!   `map::decode` pipeline as the GameController.framework shim. (winit's Android backend drops joystick axes
//!   and hands gamepad buttons over as unidentified keys, and gilrs has no Android backend.)
//! * ROM picker bridge: `pick_roms` -> `MainActivity.pickRoms()` (SAF), picked files come back through `Native.romPicked`.
//! * `run_missing_data`: the phase-1 screen (copy a Mac BYOR data folder over); the setup screen in `byor.rs` replaced it.
use bevy::prelude::*;
use std::path::PathBuf;
use std::sync::Mutex;

/// `[kind, uid, id, value]`, the same layout as `gc_shim.m` (`map::decode`).
pub type RawPadEvent = (i32, i32, i32, f32);

static PAD_QUEUE: Mutex<Vec<RawPadEvent>> = Mutex::new(Vec::new());

/// Drained once per frame by the controller bridge.
pub fn drain_pad_events() -> Vec<RawPadEvent> {
    std::mem::take(&mut *PAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// `Native.padEvent(kind, uid, id, value)` (see `MainActivity.kt`). `kind`: 0 connect, 1 disconnect, 2 button, 3 axis.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_anf_jumpman_android_Native_padEvent(_env: *mut std::ffi::c_void, _class: *mut std::ffi::c_void, kind: i32, uid: i32, id: i32, value: f32) {
    let mut q = PAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    // A runaway producer (the app is paused and nothing drains) must not grow the queue forever.
    if q.len() < 4096 {
        q.push((kind, uid, id, value));
    }
}

/// ROM files the system picker (`MainActivity.pickRoms`, Storage Access Framework) has copied into `<data>/inbox`.
static PICKED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Next picked ROM copy (polled once per frame by the setup screen, `byor::poll_picked`).
pub fn picked_pop() -> Option<PathBuf> {
    let mut q = PICKED.lock().unwrap_or_else(|e| e.into_inner());
    if q.is_empty() { None } else { Some(q.remove(0)) }
}

/// `Native.romPicked(path)`: called from the Kotlin copy thread once per file after it has been copied to app storage.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_anf_jumpman_android_Native_romPicked(mut env: jni::JNIEnv, _class: jni::objects::JClass, path: jni::objects::JString) {
    if let Ok(p) = env.get_string(&path) {
        PICKED.lock().unwrap_or_else(|e| e.into_inner()).push(PathBuf::from(String::from(p)));
    }
}

/// Opens the system document picker (ACTION_OPEN_DOCUMENT, several files): calls `MainActivity.pickRoms()` on the activity.
pub fn pick_roms() {
    let Some(app) = bevy::android::ANDROID_APP.get() else {
        return;
    };
    let call = || -> jni::errors::Result<()> {
        // SAFETY: both pointers come from android-activity and stay valid for the life of the process / activity.
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM)? };
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        env.call_method(&activity, "pickRoms", "()V", &[])?;
        Ok(())
    };
    if let Err(e) = call() {
        eprintln!("android: could not open the ROM picker: {e}");
    }
}

/// Set by `Native.backPressed()` when `MainActivity` swallowed a Back press during an import.
static BACK_PRESSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True once per swallowed Back press (polled by the setup screen, `byor::android_guard`).
pub fn take_back_pressed() -> bool {
    BACK_PRESSED.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// `Native.backPressed()`: Back was pressed while an import runs; the activity ignored it.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_anf_jumpman_android_Native_backPressed(_env: *mut std::ffi::c_void, _class: *mut std::ffi::c_void) {
    BACK_PRESSED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Replaces the process with a fresh one (`MainActivity.restartApp`): Settings > Import leaves the game completely and the new process
/// opens the setup screen (see `byor::restart_into_setup`).
pub fn restart_app() -> Result<(), String> {
    let Some(app) = bevy::android::ANDROID_APP.get() else {
        return Err("no activity".into());
    };
    let call = || -> jni::errors::Result<()> {
        // SAFETY: both pointers come from android-activity and stay valid for the life of the process / activity.
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM)? };
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        env.call_method(&activity, "restartApp", "()V", &[])?;
        Ok(())
    };
    call().map_err(|e| format!("restartApp failed: {e}"))
}

fn app_dirs() -> Option<(PathBuf, PathBuf)> {
    let app = bevy::android::ANDROID_APP.get()?;
    let internal = app.internal_data_path()?;
    let external = app.external_data_path().unwrap_or_else(|| internal.clone());
    Some((internal, external))
}

/// Sets `HOME` and the `JUMPMAN_*` directory overrides before anything reads them (called first in `main`).
pub fn init_env() {
    let Some((internal, external)) = app_dirs() else {
        eprintln!("android: no app data paths (ANDROID_APP not set)");
        return;
    };
    let set = |k: &str, v: PathBuf| {
        if std::env::var_os(k).is_none_or(|x| x.is_empty()) {
            // SAFETY: called at the top of `main`, before the app spawns any thread of its own.
            unsafe { std::env::set_var(k, v) };
        }
    };
    let _ = std::fs::create_dir_all(&external);
    // Extracted game data: <external files dir>/Jumpman (adb push / USB-visible); saves and settings stay private.
    set("JUMPMAN_DATA_DIR", external.join("Jumpman"));
    set("JUMPMAN_SAVE_DIR", internal.join("saves"));
    // The ROM-free import inputs MainActivity unpacks from the APK (assets/byor-src.zip).
    set("JUMPMAN_BYOR_SRC", internal.join("byor-src"));
    set("JUMPMAN_CONFIG_DIR", internal.clone());
    set("HOME", internal);
}

/// Shown instead of the game when the data dir has no `assets/courses` yet. Exits by itself once the data shows up
/// (the next launch then starts the game).
pub fn run_missing_data() {
    let dir = crate::byor::data_dir();
    let text = format!(
        "JUMPMAN\n\nNo game data found.\n\nThis app contains no Nintendo data. Import your ROMs with the Jumpman BYOR app on a Mac, then copy its\n'Library/Application Support/Jumpman' folder (assets, public, stamps) to this device as:\n\n{}\n\nUSB: connect the handheld to a computer and copy the folder in with file transfer (MTP), or use:\nadb push Jumpman \"{}\"\n\nSee ANDROID.md for details. This screen closes by itself once the data appears; then start Jumpman again.",
        dir.display(),
        dir.parent().unwrap_or(&dir).display()
    );
    let watch = dir.join("assets/courses");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Jumpman".into(),
            resizable: false,
            mode: bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Primary),
            ..default()
        }),
        ..default()
    }));
    app.add_systems(Startup, move |mut commands: Commands| {
        commands.spawn(Camera2d);
        commands.spawn((
            Text::new(text.clone()),
            TextFont { font_size: FontSize::Px(18.0), ..default() },
            TextColor(Color::WHITE),
            Node { position_type: PositionType::Absolute, left: Val::Px(40.0), top: Val::Px(30.0), right: Val::Px(40.0), ..default() },
        ));
    });
    app.add_systems(Update, move |time: Res<Time>, mut exit: MessageWriter<AppExit>, mut last: Local<f32>| {
        if time.elapsed_secs() - *last > 2.0 {
            *last = time.elapsed_secs();
            if watch.is_dir() {
                exit.write(AppExit::Success);
            }
        }
    });
    app.run();
}
