# Jumpman on Android (Retroid Pocket and other Android handhelds)

Status: **phase 1.** The APK builds in GitHub Actions and contains **no Nintendo data**. The game data cannot yet be imported on the
device (phase 2 is started, `ANDROID_IMPORT.md`); you import your ROMs with the existing macOS BYOR app and copy the result to the
handheld. **Not run on a real device or emulator yet** (see "Not verified").

## What is in the APK

- `lib/arm64-v8a/libjumpman_game.so`: the Rust/Bevy game (cdylib of the `jumpman_game` crate, entry `android_main` from `#[bevy_main]`).
- `classes.dex`: `MainActivity` (a `GameActivity` subclass: sensor-landscape, immersive fullscreen, gamepad forwarding).
- the manifest and a launcher icon. No `assets/` (checked by `native/android/verify-apk.py`, which also runs `byor/verify-byor-app.py`).

Bevy 0.20.1, `android-game-activity` (GameActivity, as in Bevy's `examples/mobile`), wgpu 30 on **Vulkan**, audio through
rodio/cpal (AAudio, min SDK 26), arm64-v8a only, `minSdk 26`, `targetSdk 34`.

## Get the APK

GitHub > Actions > "Android APK (BYOR)" > a run > Artifacts > `jumpman-android-<sha>.apk` (a zip holding
`jumpman-android-<sha>.apk`). The workflow runs on `workflow_dispatch`, and on pushes to `main` (and the `android-build` branch)
that touch `native/**`. It creates no release or tag.

The APK is signed with the auto-generated debug keystore of the CI runner: it installs fine as a sideload, but every CI build has a
*different* signing key, so Android refuses to update over an older build. Uninstall first when installing a newer one (your
saves live in the app's private storage and go with it; the game data in the external files dir is removed too on uninstall).

## Install on a Retroid Pocket

1. Download and unzip the artifact on the handheld (or on a PC).
2. Enable "Install unknown apps" for the browser / file manager you use (Settings > Apps > Special app access).
3. Tap the APK, or from a PC: `adb install -r jumpman-android-<sha>.apk`.

## Get the game data onto the device (phase 1 import path)

1. On a Mac, run the **Jumpman BYOR app** (`native/BYOR.md`) with your own ROMs (Super Mario 64 required; Mario Kart 64, Ocarina of
   Time, Banjo-Kazooie optional). It extracts everything into `~/Library/Application Support/Jumpman` (folders `assets/`, `public/`,
   `stamps/`).
2. Start Jumpman once on the handheld. It shows "No game data found" and prints the exact target path, normally
   `/storage/emulated/0/Android/data/com.anf.jumpman.android/files/Jumpman`.
3. Copy the folder there:
   - with adb: `adb push "$HOME/Library/Application Support/Jumpman" /storage/emulated/0/Android/data/com.anf.jumpman.android/files/`
     (then `Jumpman/assets`, `Jumpman/public`, `Jumpman/stamps` must exist on the device; the whole thing is several hundred MB to a few GB
     depending on the games you imported), or
   - over USB file transfer (MTP) from a PC: browse to `Android/data/com.anf.jumpman.android/files/` and drop the `Jumpman` folder in.
     Android 11+ hides `Android/data` from on-device file managers, so use adb or a PC for this step.
4. The "no data" screen closes by itself when `assets/courses` appears. Start Jumpman again.

The stamps in `stamps/` make the app skip re-import checks exactly like the macOS BYOR build. If you later re-import on the Mac with
newer exporters, copy the folder over again.

### On-device import (phase 2, started; see `ANDROID_IMPORT.md`)

Decision: a pure-Rust port (`native/byor-rs`), not an embedded CPython. Done so far: the Storage Access Framework picker
(CHOOSE ROM FILES copies the chosen ROMs into app storage), ROM identification and the SM64 asset-extraction stage. **No game can
be imported end to end on the device yet**: the exporters are not ported, so IMPORT reports that and the Mac import plus copy path
above is still the way to get data onto the handheld. The picker, JNI glue and the Android setup screen have not been compiled
or run on Android (see `ANDROID_IMPORT.md` for what was verified on the Mac and how).

## Controls

The Retroid Pocket's built-in controls (and Bluetooth/USB pads) arrive as a standard Android gamepad. `MainActivity` intercepts
their key and motion events and forwards them over JNI to the same controller pipeline the iOS/macOS GameController bridge uses
(`src/controller/`), so menus, rebinding, multi-player seats and the experiments behave as with a pad on iOS. (Bevy's gilrs backend
has no Android support, and winit drops joystick axes, so this bridge is required.)

| Android | Game |
|---|---|
| Left stick (AXIS_X/Y) | move |
| Right stick (AXIS_Z/RZ or RX/RY) | camera |
| BUTTON_A / B / X / Y (positional: south / east / west / north) | A (jump) / B (punch) / X / Y |
| L1 / R1 | shoulder buttons (menu tab switching) |
| L2 / R2 (key or analog axis) | triggers |
| D-pad (hat axis or DPAD keys) | D-pad |
| START / SELECT | Start (pause) / Select |
| MODE (Home/Guide) | ignored on purpose |
| THUMBL / THUMBR | stick clicks |

Notes: the mapping is by Android key code; some Retroid firmware has a Switch-style A/B swap setting under the handheld's own
settings (that swaps which physical button sends BUTTON_A). The touch overlay is shown until a pad connects and hides itself; touch
brings it back. Disconnecting the last pad while playing pauses the game.

## Build it yourself

CI is the supported way. Locally you need JDK 21, the Android SDK (platform 36), NDK `27.2.12479018`, `rustup target add
aarch64-linux-android` and `cargo install cargo-ndk`:

```sh
cd native
cargo ndk -t arm64-v8a --platform 26 -o android/app/src/main/jniLibs rustc --release --lib --crate-type cdylib --locked
cd android && ./gradlew assembleRelease      # app/build/outputs/apk/release/app-release.apk
python3 verify-apk.py app/build/outputs/apk/release/app-release.apk
```

`cargo ndk ... rustc --crate-type cdylib` is used instead of adding `cdylib` to `[lib] crate-type`, so desktop and iOS builds are
unchanged. Android-only code lives behind `cfg(target_os = "android")` (`src/android.rs`; small `any(ios, android)` tweaks for
phone-sized limits in `lib.rs`, `controller/mod.rs`, `experiments/`).

### Paths

`android::init_env` (first thing in `main`) sets `JUMPMAN_DATA_DIR` = `<external files dir>/Jumpman` (extracted game data),
`JUMPMAN_SAVE_DIR` = `<internal files>/saves`, `JUMPMAN_CONFIG_DIR`/`HOME` = `<internal files>`. All three can still be overridden
by setting the variables beforehand. Because Bevy's default asset reader on Android is the APK's `AssetManager`, `lib.rs` registers
a plain file reader as the default asset source (and for `mk64://`) pointing at the data dir.

## Not verified

Everything below was **not** run on a device or emulator:
- That the app starts, renders (wgpu Vulkan on Adreno), plays audio (AAudio), or runs at an acceptable frame rate. The game's
  per-platform limits (bots, enemy slots) use the iOS "phone" values on Android; they are untested guesses.
- The gamepad bridge on real Retroid firmware (axis choice for the right stick, hat vs key d-pad, trigger reporting).
- Reading a Mac-produced data folder on Android (it should be byte-identical, and paths are plain file reads).
- Ray tracing is disabled on Android (the wgpu probe is skipped).
- Touch overlay layout on a 16:9 1080p screen without notch (the non-iOS margins are used).
- Pause/resume (home button, suspend) and graphics-surface loss recovery beyond what Bevy/winit provide.
- Non-arm64 devices (the APK is arm64-v8a only).

## CI facts (first green run)

Run 42552687477 on `android-build`: ubuntu-24.04, JDK 21, NDK 27.2.12479018, cargo-ndk 3.5.4, AGP 9.4.1 / Gradle 9.8.0,
compileSdk 36. A cold Rust build takes about 25 minutes; the cargo and Gradle caches make later runs much faster. The APK is
about 117 MB (112 MB `libjumpman_game.so`, 9 MB `classes.dex`), 851 entries, arm64-v8a only; the shared library needs only
platform libs (libandroid, liblog, libaaudio, libdl, libm, libc) and exports `android_main`, `GameActivity_onCreate` and the JNI
`Native.padEvent`. androidx is pinned to versions that build against SDK 36 (`gradle/libs.versions.toml`).
