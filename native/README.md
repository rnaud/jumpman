# Jumpman native (Rust / Bevy)

A Rust port of the TypeScript game. `jumpman-sim` (`sim/`) is the engine-free simulation (f64, bit-compatible
with the TS original); the `jumpman` crate (`src/`) is the Bevy 0.16 app that renders, plays audio and
runs menus/progression on top of it. Method, stage table and open TODOs: [PORTING.md](PORTING.md).

## Build and run

```sh
export PATH=/opt/homebrew/opt/rustup/bin:$PATH   # if rustup is from Homebrew
cd native
cargo run --release                      # title screen
cargo run --release -- bob --act 2 --entry WARP_NODE_0A
cargo run --release -- --list            # course ids
./package-mac.sh                         # -> target/Jumpman.app (open it)
```

CLI: `<course> [--act N] [--area N] [--entry NODE] [--push] [--experiment NAME] [--list]`; short names
`kokiri`, `dungeon`, `shop`, `link` map to the Zelda courses. A CLI course or any headless hook skips the title.

### Assets
The game reads the checked-in `../assets` (SM64 export, audio, UI art) and `../public/models`. The root is
resolved in order: `JUMPMAN_ASSETS`, `Jumpman.app/Contents/Resources/assets`, `assets/` beside the executable,
`../../../assets` from `target/{debug,release}`, the compile-time checkout, `./assets`.
`package-mac.sh` bundles the whole tree (~380 MB), `public/models` and an icon made from `public/icon-512.png`;
`JUMPMAN_BUNDLE_COURSES=bob,ccm` bundles only those courses; `JUMPMAN_ICON=x.icns` overrides the icon.
Saves and `settings.json` live in `~/Library/Application Support/Jumpman/` (`JUMPMAN_SAVE_DIR`,
`JUMPMAN_CONFIG_DIR` redirect them; Linux `~/.local/share/jumpman`, Windows `%APPDATA%\Jumpman`).

### Env hooks (selection)
| Variable | Effect |
|---|---|
| `JUMPMAN_COURSE/ACT/ENTRY`, `JUMPMAN_EXPERIMENT`, `JUMPMAN_CHARACTER` | start state |
| `JUMPMAN_EXIT_AFTER=<s>`, `JUMPMAN_SCREENSHOT=<png>`, `JUMPMAN_WALK=1` | headless smoke / screenshot (saved 1 s before exit) |
| `JUMPMAN_SCREEN=title\|pause\|courses\|settings\|controls\|slots\|gameover\|play`, `JUMPMAN_SHELL_SCRIPT` | force a menu / inject menu commands |
| `JUMPMAN_FLOW=selector\|result\|gameover\|bonus\|death\|star` | progression screens |
| `JUMPMAN_DEBUG=1`, `JUMPMAN_DEBUG_LOG=<n>`, `JUMPMAN_FRAME_LOG=1` | debug panel / state log |
| `JUMPMAN_MUTE=1`, `JUMPMAN_NO_AUDIO=1`, `JUMPMAN_AUDIO_DUMP=f.raw` | audio |
| `JUMPMAN_TELEPORT=x,y,z[,yaw]`, `JUMPMAN_TAP=a@3.0,b@5` | scripted input |
| `JUMPMAN_ZELDA_FLAGS/POS/PRESS/AIM`, `JUMPMAN_NO_TRAVEL` | Zelda scene states |

More are documented in the module headers and PORTING.md.

## Controls
WASD / arrows move, Space jump (hold for height), Shift or J punch/grab/throw, Ctrl or K crouch/ground pound,
Alt walk, Q/E or mouse drag orbit, wheel zoom, C camera behind, R reset, V swap character, M mute,
H aim the slingshot (Link), X leave an experiment. Gamepads work (left stick, A/B/Z, triggers, Start pauses).
Esc / Start pauses; quit from the menu or Cmd+Q.

**Touch** (`src/touch.rs`, port of `TouchControls.ts`): when touches arrive (iOS/Android builds, touchscreens) a bevy_ui
overlay appears while playing: virtual stick bottom-left (108 px, N64 64-unit stick math identical to the TS), L/Z/B/A
buttons bottom-right (each is the same action as its key/pad binding), and dragging anywhere else swipes the camera.
Touch merges into `PlayerInput` after keyboard/pad, so it coexists with them. Debug: `JUMPMAN_TOUCH=1` shows the overlay,
`JUMPMAN_TOUCH_SIM=1` replays synthetic touches (stick up-right, A held, camera swipe); combine with `JUMPMAN_SCREENSHOT=out.png`.

## Offline
The native app is inherently offline; there is no service worker equivalent to `src/offline.ts` /
`scripts/build-offline.mjs` to port. The Rust code makes no network requests (no HTTP client dependency, no URL fetches;
Bevy's web asset sources are not enabled) and reads only files under the assets root (`JUMPMAN_ASSETS`, or the bundle's
`Contents/Resources/assets` + `public/models` made by `package-mac.sh`). Saves are local files.

## Architecture map
- `sim/src`: `math`, `surfaces`, `collision`, `mario_*`, `actions/*` (one file per TS action file), `object*`,
  `behaviors/*`, `characters`; no engine dependency.
- `src/game`: course/catalogue loading and `GameSim` (30 Hz tick). `sim_plugin.rs` drives it from `FixedUpdate`.
- `src/input.rs`, `camera/`, `render/`, `fx/`, `hud.rs`: presentation. `audio/`: SM64 sequence player (wasm in wasmi).
- `src/progress`: saves, warps, stars, doors, wipes. `src/shell`: title/pause/settings UI. `src/objects`, `zelda/`,
  `characters/`, `experiments/`: gameplay extras.

## Tests and golden fixtures
`cargo test --workspace --locked` (about 300 tests). Sim tests in `sim/tests/` compare against JSON fixtures
in `sim/tests/fixtures/` that are generated from the TypeScript code and committed; values must match bit-for-bit.
Regenerate from the repo root after changing TS sim code:

```sh
GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts
cd native && cargo test --workspace
```

CI: `.github/workflows/rust.yml` runs tests (blocking) and clippy (non-blocking) on macOS for changes under `native/`.

## iOS / TestFlight

`ios/` holds an Xcode wrapper around the game as a Rust static library (`jumpman_game`, entry point `main_rs`).

```sh
# one-time
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
# simulator build (no signing)
xcodebuild -project ios/Jumpman.xcodeproj -scheme Jumpman -configuration Release -sdk iphonesimulator \
  -derivedDataPath /tmp/jumpman-ios CODE_SIGNING_ALLOWED=NO build
codesign --force --sign - /tmp/jumpman-ios/Build/Products/Release-iphonesimulator/Jumpman.app
xcrun simctl install booted /tmp/jumpman-ios/Build/Products/Release-iphonesimulator/Jumpman.app
SIMCTL_CHILD_JUMPMAN_TOUCH=1 xcrun simctl launch booted com.anf.jumpman
# TestFlight
TEAM=ABCDE12345 BUNDLE_ID=com.you.jumpman UPLOAD=1 ./package-ios.sh
```

Notes: iOS 26+ requires the UIScene life cycle, which winit 0.30 does not use, so `ios/Sources/main.m`
swizzles `UIWindow initWithFrame:` to attach winit's window to the connected scene. The window is
borderless fullscreen so Bevy sees the real size; menus are scaled down to fit (`shell::fit_menu_scale`).
Touch controls use fixed safe-area margins (`touch.rs`). Saves/settings live in the app container.
