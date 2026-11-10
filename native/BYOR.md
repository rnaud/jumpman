# Bring your own ROM (BYOR)

`target/Jumpman-BYOR.app` contains no Nintendo game data. On first launch it asks for your own ROMs and
extracts everything the full build ships (models, courses, textures, music engine, karts, characters) on
your machine. After the import the game is functionally identical to the full build.

Platform: **macOS** (Apple Silicon build; no toolchain needed) and **iOS** (the separate `JumpmanBYOR` target, no toolchain
needed; Android uses the same importer, `ANDROID_IMPORT.md`). The import is the pure-Rust `byor-rs` crate everywhere. The existing
TestFlight iOS app ships the full assets and is unchanged. Windows/Linux relaunch-after-setup and toolchain detection are
out of scope: `byor.rs` compiles there, but only macOS, iOS and Android are packaged and tested.

## User flow

1. Launch `Jumpman-BYOR.app` (first time: right-click > Open, the ad-hoc signature is not notarised).
   With no extracted assets a **setup window** opens (also: `Jumpman --setup`).
2. Drag your US ROMs onto the window (`.z64`, `.v64`, `.n64` byte orders all work; files are identified by
   checksum, not name). Super Mario 64 is required; the others are optional and unlock content:

   | ROM | Unlocks |
   |---|---|
   | Super Mario 64 (US) **required** | everything Mario: courses, models, animations, SFX, music |
   | Mario Kart 64 (US 1.0) | Mario Kart mode (Luigi Raceway, karts, HUD) |
   | Ocarina of Time (US 1.2) | Zelda courses, Link and Young Link |
   | Banjo-Kazooie (US 1.0) | Banjo-Kazooie character |

3. Press Enter. `byor/pipeline.py` extracts into the per-user data dir (`~/Library/Application Support/Jumpman`;
   `$XDG_DATA_HOME/jumpman`, `%APPDATA%\Jumpman`; override with `JUMPMAN_DATA_DIR`). SM64 takes a few minutes.
4. The setup process re-executes the game; later launches go straight to it.
5. **Adding a game later**: Settings > *Import / re-extract ROMs* (only shown for a BYOR install) shows each game's
   status and re-opens the setup window (relaunch with `--setup`). Drop only the new ROM and press Enter; the other
   games' output is untouched (a failing game never removes another game's files). Dropping an already installed
   game's ROM re-extracts it.
6. Games you have not imported are greyed out in the menus with the reason *Import <game> ROM in Setup*: the
   Mario Kart 64 mode card, Link / Young Link / Banjo in the PLAY AS row, and the Zelda courses.
   (`src/content.rs`, `AvailableContent`, built at startup by probing the exported files, so the full build is unchanged.)

Headless/test: `Jumpman --rom PATH [--rom PATH ...]` queues ROMs and starts extraction immediately.
`JUMPMAN_DATA_DIR=/tmp/x` isolates a run; a BYOR bundle never falls back to the build machine's repo checkout.

## iOS (JumpmanBYOR)

Same data model as macOS (`<Application Support>/Jumpman`, `stamps/`, atomic per-game swap in `.stage`, `rust-<game>-<revision>` stamp hashes), different engine and UI; design and trade-offs in `IOS_IMPORT.md`.

- Build:
  `xcodebuild -project ios/Jumpman.xcodeproj -scheme JumpmanBYOR -sdk iphonesimulator|iphoneos build`
  (`BYOR_REPO_ROOT=<checkout with populated reference/ submodules>` if the current one lacks them). Phases: Build Rust
  (`build-rust-byor.sh`, no asset copy), compile `ios/import/jm_import.m` (document picker, data dir, keep-awake), `bundle-byor-src.sh`. Verify with `byor/verify-byor-app.py <JumpmanBYOR.app>`.
- Flow: first launch (no data) opens the setup screen. CHOOSE ROMS (document picker, any file; Files, iCloud, AirDrop "Open
  in..." also work), or copy ROMs into the app's Documents folder with the Files app and tap SCAN DOCUMENTS; IMPORT runs the
  pipeline; progress and per-game status come from the same `@@PROGRESS` lines. The screen stays awake and a background-task
  assertion lets a short app switch finish cleanly.
  When the import finishes the app **closes** (iOS cannot restart itself and winit allows one event loop per process); open
  it again to play. Settings > Import / re-extract leaves a marker and quits the same way.
- Games missing or stale are disabled in the menus exactly as on macOS (`AvailableContent`).
- Test hook: `JUMPMAN_IOS_IMPORT_ROMS=<dir>` imports every ROM-sized file in `<dir>` at launch (no tapping needed).
- ROM bookmarks: the picker saves security-scoped bookmark data next to the inbox copy; re-using it for silent updates is not
  wired up yet (a stale game asks for its ROM again). Picker copies are deleted after the import.

## The importer (pure Rust; Python is the test oracle only)

All four games are imported by `byor-rs` (`native/byor-rs`, `ANDROID_IMPORT.md`), in process, on every platform (macOS, iOS,
Android): the setup screen (`src/byor.rs`) calls `byor_rs::cli::run`, which speaks the `@@PROGRESS/@@DONE/@@FAIL/@@STATUS`
protocol and keeps the `stamps/<game>.json` format and the atomic stage-and-swap install. No python3, Xcode Command Line
Tools, C compiler or embedded interpreter is needed or bundled; the apps carry `byor-src` with the decomp files of
`byor/decomp-manifest.txt`, the music engine templates, the BK map catalogue, the SM64 sound-player sequence and the original assets.
`byor/pipeline.py` and `scripts/export-*.py` stay in the repository as the reference implementation: `byor-rs` is verified
byte for byte against their output (`byor/compare-data-dir.py`, `byor-rs/tests/*_rom.rs`) but they are not shipped.
Stamps carry the hash `rust-<game>-<revision>` (`byor_rs::pipeline::PORTED`); an install made by the old Python pipeline shows
"update available" once and is re-imported from the remembered ROM. An import cannot be cancelled mid-way any more (it takes
seconds to a minute or two); an interrupted one leaves the previous install untouched.

## Layout

- `src/byor.rs`: data dir, stamps (`stamps/<game>.json`, version `PIPELINE_VERSION`), setup window, relaunch,
  `reopen_setup` (Settings entry). Asset lookup (`asset_root` in `lib.rs`) tries the data dir after the bundle and
  before the repo checkout. Stale stamps (older pipeline) re-open setup.
- `src/content.rs`: `AvailableContent` (which optional games are installed) used by `shell/screens.rs`.
- `byor/pipeline.py`: stages scripts + decomp sources in `<data>/.stage`, runs the exporters per game, installs
  results, writes the stamp, deletes the stage. Prints `@@PROGRESS game pct msg`, `@@DONE game`, `@@FAIL game msg`.
- `byor/package-byor.sh`: builds `Jumpman-BYOR.app` (engine + original assets + `byor-src`), ad-hoc codesigns it and
  runs the audit.
- `byor/verify-byor-app.py`: **fails** if the bundle holds ROM magic (0x80371240 / 0x37804012 / 0x40123780), `*.z64`
  `*.n64` `*.v64` files, any path the exporters produce, binary assets inside the decomp trees, or any file
  byte-identical to a ROM-derived file of the repo checkout.

## Bundle size and the decomp manifest

The app needs no network at run time; everything the extraction reads ships inside it. The decomp trees are not
bundled whole: `byor/decomp-manifest.txt` lists exactly the files the exporters open (paths relative to
`reference/`, one per line, sorted), and `package-byor.sh` copies those with `rsync --files-from`. The scripts
bundled are the ones the pipeline runs (`byor/scripts-manifest.txt`), the audio template is gzipped
(2.5 MB -> 34 KB, `render-music-template.py` reads either), and the pipeline's `.md` / unused art are left out.

| component | before | after |
|---|---|---|
| binary (unchanged) | 116.3 MB | 116.7 MB (rebuilt from the current tip) |
| decomp sm64 | 39.9 MB | 29.2 MB (2276 files) |
| decomp mk64 | 61.9 MB | 19.5 MB (98 files) |
| decomp oot | 35.9 MB | 11.9 MB (564 files) |
| decomp banjo-kazooie | 7.9 MB | 0.3 MB (12 files) |
| scripts + pipeline | 2.8 MB | 0.4 MB |
| original assets (ui, Quaternius, item sprites, icons) | 8.7 MB | 8.7 MB (all of it is loaded at run time) |
| **.app total (du)** | **275.9 MB** | **187.7 MB** |

How the manifest was made and how to keep it right:

- `byor/trace_reads.py` runs the real pipeline over a scratch copy of the decomp trees after resetting every file's
  atime; whatever the kernel marks as read (Python, `make`, `cc`, the SM64 asset tools, `assemble`...) is the
  list. (`fs_usage`/`dtruss` need sudo; the atime trace works for children too.) The SM64 tree is traced in the
  stage copy the pipeline makes.
- The list was then replayed: the pipeline run against ONLY those files produces output byte-identical to the
  committed `assets/` and `public/` (compare-data-dir.py), and removing any directory group (`levels`, `actors`,
  `tools`, `src/overlays`, `courses`, ...) makes the run fail or differ, so nothing in it is spare.
- **When an exporter starts reading a new decomp file**, the bundled run fails (file missing). Run
  `byor/check-manifest.py --trace --sm64 ROM [--mk64 ROM --oot ROM --bk ROM] --update` on a machine with the
  full `reference/` trees checked out: it traces the pipeline, prints every read file that is not in the manifest and
  (with `--update`) appends them. Without `--trace` it is a static, ROM-free check (CI-able): every manifest line
  exists, the list is sorted, and `scripts-manifest.txt` matches the scripts the pipeline runs. Re-run the end-to-end
  parity check after changing the manifest.
- A new exporter script must be added to `scripts-manifest.txt` and to `GAME_SCRIPTS` in `pipeline.py`.

## Updates (what survives, what triggers a re-import)

Everything extracted lives in the data dir, not in the app, so replacing `Jumpman-BYOR.app` with a newer build keeps
it: `assets/`, `public/`, `stamps/<game>.json`, and `roms.json` (the path of the ROM each game was last imported from).

- A stamp records `version` (`PIPELINE_VERSION`, bumped to force every game), a `hash` and the `files` the import
  produced. The hash is a sha256 over what decides that game's output: the step function in `pipeline.py`, the exporter
  scripts the step runs (`GAME_SCRIPTS`), `c2obj.py`, and the manifest's decomp files (paths and contents). The packager
  computes the hashes into `byor-src/exporters.json`; the app only compares strings. So any exporter change marks
  just the affected games stale, with no manual version bump. A v2 stamp (no hash) is stale. A dev checkout has no
  `exporters.json` and compares versions only.
- Game states: *missing* (never imported), *stale* ("Update available", imported by older exporters), *current*.
  Stale SM64 re-opens setup at launch (the game needs its courses). When setup opens, `pipeline.py --status` reports each game; a
  stale game whose remembered ROM still exists and still identifies by checksum is re-imported straight away,
  without asking. If the ROM moved, the window shows "Update available - re-import (drop the ROM again)".
- A stale optional game (Mario Kart 64, Ocarina of Time, Banjo-Kazooie) is switched off like a missing one with the
  reason "Update needed in Setup"; Settings > Import shows "update available". Nothing panics on a mismatch: the content probes
  and tolerant loaders treat missing files as unavailable.
- Atomic re-import: output is built in `<data>/.stage` (inside the data dir, so moves are renames). Only after the whole
  game succeeded are the files swapped in, then files the previous import produced but this one does not
  (and no other game still owns) are removed, and the stamp is rewritten last (it is removed first, so a crash
  during the swap reads as "not imported", never as a current install). A failed or cancelled import leaves the previous
  install and stamp untouched.
- Tests: `python3 -m unittest byor/test_pipeline.py` (upgrade from a v2 stamp, remembered-ROM reuse, a failing run
  leaves the install intact, obsolete-file removal, shared files, hash sensitivity) and `cargo test --lib byor::`
  (stamp missing / current / hash mismatch / old format).

## What ships vs what is extracted

Ships (original): `assets/ui/*` (menu backdrop, quilt tile, fonts; the backdrop is original artwork),
`audio/kart-voices.json`, `audio/character-sounds.json`, `public/mk64/item-*.png` (procedural),
`public/models/quaternius` (CC0), icons, the data-less audio engine template (`scripts/music/sm64-us-template.*`,
decomp CODE compiled to wasm with its four sound blobs zero-filled), and the decomp source trees (below).

Extracted from the ROMs (not in the bundle): `assets/{actors,levels,extracted,courses,oot-dialogue,oot-icons,audio/*}`,
`animations.json`, `layouts.json`, `public/models/{oot,bk}`, and all other `public/mk64` files (course geometry,
`gLR*` course textures, `hud-*` item/minimap/portrait/rank textures, `lakitu-*`, `*-kart.png`, `mk64-*`).

## Audio

The engine (`assets/audio/music/sm64-us.wasm`, the sm64-port audio code) embeds four ROM-derived blobs in one
data segment: `gMusicData` (sequences), `gBankSetsData`, `gSoundDataADSR` (instrument banks) and `gSoundDataRaw`
(VADPCM sample data). `scripts/render-music-template.py`:

- **maintainers** (`--make-template sm64-us.wasm`): finds the blobs in a wasm produced by `scripts/render-music.py`
  (WASI SDK 34 + pinned sm64-port, done once) and zero-fills them: `scripts/music/sm64-us-template.{wasm,json}`.
- **extraction** (pipeline, SM64 stage, after `export-audio.py`): rebuilds the blobs from the extracted
  samples/sequences with the decomp's tools (`cc` for the codebook extractor and VADPCM encoder, Apple clang's
  assembler for sequence 0, `assemble_sound.py`), patches them into the template and writes `sm64-us.wasm`,
  `sounds.json` and `courses.json`. The result is **byte-identical** to the committed files (verified).

Mario Kart 64 music works the same way with its own engine (`scripts/music/mk64-template.{wasm,json}`: the decomp's `src/audio` plus a
portable mixer, built by `scripts/render-music-mk64.py`): in the MK64 stage, after `export-mk64.py`, `scripts/export-mk64-music.py <ROM>`
converts the ROM's sequences, banks and samples (little endian) and patches them in: `assets/audio/music/mk64/{mk64-audio.wasm,sequences.json}`.
Installs made before this need MK64 re-imported to get music (without the file the game is silent on those courses; it does not crash).

`sounds.json`/`courses.json` are also compiled into the binary (`include_str!`), so SFX ids and course music work
even before extraction. Test: `JUMPMAN_TEST_ASSETS=<data>/assets cargo test --lib audio` renders the web-parity
scenes and a music + SFX scene from the extracted engine and asserts it is non-silent and equal to the committed wasm.

## Parity status

Pipeline output for all four ROMs vs the committed `assets/` + `public/` (`byor/compare-data-dir.py`, byte
comparison): 3474 files, **all byte-identical** (music wasm, sounds.json, courses.json, Mario Kart HUD/portrait/lakitu/kart
PNGs, models, courses). Only `public/models/README.md` and `public/models/local/README.md` (docs) are not generated.
(The 11 door LOD variant glbs were stale in the repo since exporter fix b35b445 and were refreshed.)

## Adding an exporter (new game or asset family)

1. Write `scripts/export-<thing>.py` that reads the ROM from `reference/roms/<name>.z64` (or the decomp tree under
   `reference/`) and writes to `assets/` / `public/` relative to the script's own location (`ROOT`). It must be
   standard-library only and deterministic.
2. Add the ROM to `ROMS` in `byor/pipeline.py` (sha1/md5 of the z64-ordered file), a `do_<game>(stg, rom)` function
   that copies the ROM into the stage and runs the script(s) with `run([...], stg, game, lo, hi)`, and register it in `DO`.
   If it reads a decomp tree, add it to `LINKED` and `package-byor.sh`.
3. Add the game to `GAMES` in `src/byor.rs`, bump `VERSION` (pipeline.py) and `PIPELINE_VERSION` (byor.rs) together.
4. Add a probe to `Game::probes` in `src/content.rs` and gate the menu entries that need it.
5. Add its output paths to `DENY` in `byor/verify-byor-app.py`, re-run the packager, and byte-diff the pipeline output
   against the repo (`diff -r` on `assets/` and `public/`).

## Licensing decisions (owner, 2026-11-08)

- Bundling the decompilation source subsets (`byor-src/reference`: n64decomp/sm64, n64decomp/mk64,
  zeldaret/oot, n64decomp/banjo-kazooie) is acceptable. They contain code and text, no ROM data.
- `menu-backdrop.png` is original artwork and ships.

**Verified (2026-11-09, iOS simulator, iPhone 18 Pro):** `JumpmanBYOR.app` built with `xcodebuild -scheme JumpmanBYOR -sdk iphonesimulator` (no Python in the bundle: 22 files, 227 MB, `verify-byor-app.py` OK), launched with `JUMPMAN_IOS_IMPORT_ROMS=<dir with the OoT ROM>`: the setup screen imported OoT with `byor-rs` (`@@DONE oot`) and the 533 files in the app's data dir are byte-identical to the Python pipeline's output. Not run on a real device; SM64 / MK64 / BK were not re-imported in the simulator in this session (their byte parity is the Mac CLI tests'). `BYOR_ALLOW_MISSING_DECOMP=1` lets `stage-byor-src.sh` bundle a checkout whose submodules are incomplete (testing only).
