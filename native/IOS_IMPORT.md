> **Superseded (2026-11-09):** the embedded CPython + `_jtools` design below was retired when MK64, BK, OoT and SM64 were all
> ported to the pure-Rust importer (`native/byor-rs`, see `BYOR.md` and `ANDROID_IMPORT.md`). The JumpmanBYOR target no longer
> bundles Python or the decomp C tools; this file is kept as the history of the Python route, which the repository still has as
> the reference implementation (`byor/pipeline.py`).

# Jumpman on iOS with ROM import (BYOR)

Goal: an iPhone/iPad app with **no ROM-derived assets** that extracts everything on the device from ROMs the user owns, with the
same feature set and the same stamp / atomic / update semantics as the macOS BYOR app (`BYOR.md`).

## Phase 0: design decision

Facts (measured on this Mac, M-series, real ROMs, `/usr/bin/time -l`; the iOS numbers below are estimates):

| step | wall (subprocess) | wall (in-process, same machine) | peak RSS (in-process) |
|---|---|---|---|
| Super Mario 64 (3039 files, incl. audio banks) | 63 s | 57-95 s | 205 MB |
| Mario Kart 64 (20 courses + karts) | 7.6 s | 9.3 s | **658 MB** (largest) |
| Ocarina of Time | 27 s | 25 s | 118 MB |
| Banjo-Kazooie | 20 s | 30 s | 91 MB |

The pipeline is 6.5k lines of stdlib-only Python (struct, zlib, json, hashlib, pathlib, runpy). The only things it spawns
are: `python3` (the exporters, `extract_assets.py`, `assemble_sound.py`, `disassemble_sound.py`), `make`, and C tools
(`mio0`, `n64graphics`, `skyconv`, `aifc_decode`, `aiff_extract_codebook`, `vadpcm_enc`, all < 1.1k lines of C each),
plus `cc`/`clang` in `render-music.py` for three things: compiling the 6 tools above, a 70-line `ids.c` that prints
`SOUND_*` constants, and one assembly of the decomp's `00_sound_player.s`.

| option | verdict |
|---|---|
| **A** CPython 3.13 for iOS (PEP 730, BeeWare Python-Apple-Support, `Python.xcframework`) | Runs the 6.5k lines unchanged. 34 MB download, **5 MB** dylib + ~15 MB pruned stdlib + ~1 MB of extension-module frameworks in the app. Needs every `subprocess`/`make`/`cc` replaced. |
| **B** RustPython | Not on disk (`~/.cargo/registry`: none), adds a large Rust dependency, and the exporters are CPU-bound pure-Python loops (struct unpack of display lists, zlib PNG writing); RustPython is an interpreter an order of magnitude slower than CPython there and incomplete for `zlib`/`hashlib`. Not built or measured: ruled out by risk, not by a benchmark. |
| **C** rewrite exporters in Rust | 6.5k lines of dense format knowledge that must stay **byte-identical** to the browser/macOS output (3890 files). Months of work and a second source of truth. Rejected; only worth it for a hot spot, and nothing is hot (SM64 is 1 minute). |
| **D** hybrid = **A + the C tools linked into the app as a built-in module** | **Chosen.** The unmodified decomp C sources are compiled into the app and exposed as `_jtools.run(name, argv) -> (rc, stdout, stderr)`; Python keeps orchestrating. Everything else the subprocess world did is emulated in `byor/inproc.py`. |

Why D works without touching the exporters: `byor/inproc.py` replaces `subprocess.run/check_call/check_output`:
`python3 script.py` runs in-process (`runpy`, cwd/argv/stdout captured), `make` is a no-op, tool names go to `_jtools`,
`cc tool.c -o X` writes a stub `X` that dispatches to `_jtools`, `cc ids.c` is evaluated by a 100-line mini C preprocessor
(`sounds.h` is plain `#define`s), and the one thing that really needs an assembler (`00_sound_player.s`, 8 k lines of
`.byte` macros) is substituted by its precomputed bytes `byor/sm64-us-sound-player.bin` (13 KB, a *decomp source*
product, not ROM data) after checking the source files' sha256 recorded in the `.json` next to it
(`byor/gen-precomputed.py` regenerates; a changed decomp file fails loudly instead of using stale bytes).
`pipeline.py` itself only gained a 10-line `run()` branch (`INPROC`), so macOS and iOS share one source of truth, and the same
layer runs on a Mac with `JUMPMAN_INPROC=1` (a Mac build of `_jtools`, `ios/import/build-mac-jtools.sh`) which is how
parity is proven without a device.

The tools are one-shot programs (global state, `exit()`, `printf`). `ios/import/jt_compat.h` redirects `exit` to a `longjmp`,
`stdout/stderr/printf` to per-call memory streams, and each tool's file-scope data is linked into its own Mach-O section
that `jtools.c` saves before the first call and restores before every later one (found necessary: `skyconv` refuses a second
run otherwise). Calls are serialised (the encoder is run from a 6-thread pool in `render-music.py`).

**Byte parity, on a Mac, no process spawned (PATH empty):** all four games through the in-process path produce 3890 files,
**3890 byte-identical** to the committed `assets/` + `public/` (same as the subprocess path). In the iOS simulator see
"Verified" in `BYOR.md`.

### Memory and jetsam
Peak is Mario Kart 64 at ~660 MB resident (course glTF + texture tables in Python objects), one game at a time. Phones kill
foreground apps at roughly 2-3 GB (more on the 8 GB iPhones; `com.apple.developer.kernel.increased-memory-limit` raises it),
so the extractor itself fits. On top comes the Bevy setup screen (~150-250 MB with wgpu). The pipeline processes one game at a
time and each exporter script is run via `runpy` and dropped (modules unloaded, `gc.collect()`), so Python's peak is the
largest step, not the sum. No chunking was needed; if a device shows pressure the first lever is not holding the decoded ROM
(64 MB OoT) twice (the stage copy), then splitting `export-mk64.py` per course.
**Time on device** (estimate: A-series is within ~1.5-2x of this Mac for single-thread Python): SM64 ~1.5-3 min,
all four ~3-5 min. Needs real-device validation.

### Packaging decision
Same Xcode project, **second target `JumpmanBYOR`** (product `JumpmanBYOR.app`, display name "Jumpman Medley (BYOR)", bundle id
`com.anf.jumpman.byor`), built from the same Rust static library. Reasons: the existing TestFlight app bundles every extracted asset
(`../../assets`, `public/models`, `public/mk64`), so it is **not distributable**; a runtime flag cannot remove files from an
IPA. The BYOR target ships only: binary, `byor-src/` (scripts, original art, manifest-listed decomp sources, `exporters.json`),
`Frameworks/` (Python + extension modules) and `python/` (stdlib). `package-ios.sh`, the workflows and the existing target are
untouched (`-project Jumpman.xcodeproj -scheme Jumpman` still builds the same product). The Rust crate is shared: the import code
finds the host (`jm_import.m`) with `dlsym`, so it is inert in the full app.
**You must create a new App Store Connect app record for `com.anf.jumpman.byor`** (and a provisioning profile / App ID) before
uploading it; it cannot be an update of the existing app.

### Restart after import
winit allows one event loop per process (`EventLoop cannot be run after a call to UIApplicationMain`, winit-0.30.13
`ios/event_loop.rs`) and iOS cannot `exec`, so unlike macOS the setup App cannot hand over to a freshly built game App.
Chosen: the setup runs as its own Bevy `App` when the data is missing/stale; on success it says so and **closes the app**, the next
launch finds the data. (Settings > Import leaves a `.setup-requested` marker and quits.) Ranked alternatives: (1) build one App with
all game plugins gated behind a `GameLoaded` state and `AssetRoot` made mutable (large refactor of ~25 plugins' startup systems);
(2) wait for winit 0.31 which lifts the single-run restriction.

## App Store / policy risks (honest list)
- Guideline 4.7 (software that is not embedded / emulators) and 5.2.1-5.2.3 (third-party IP): the app contains no Nintendo
  assets, but its whole purpose is to read user-supplied commercial ROMs and rebuild a game from them. Apple allows retro-game
  emulators since 2024 only when the developer is responsible for the content a user loads; a reviewer may still reject this as
  "unlicensed content". The decomp **source** that ships (n64decomp/zeldaret code, text) is itself third-party copyrighted code.
- 2.5.2 (no downloaded executable code): the bundled Python scripts are part of the app binary package, not downloaded; no
  code is fetched at runtime. Compiled-in C tools are fine. JIT is not used.
- 5.1.1 privacy: ROMs are copied from Files into the sandbox temporarily and deleted after import; data dir is excluded from backup.
- TestFlight internal/external testing is lower risk than App Review; plan for ad-hoc/TestFlight distribution and expect rejection
  risk for the public store.

## Verified (iOS simulator, iPhone 18 Pro, iOS 27.2) vs not verified

Verified:
- `xcodebuild -scheme JumpmanBYOR -sdk iphonesimulator` builds; `byor/verify-byor-app.py JumpmanBYOR.app --repo-root .` passes
  (3692 files scanned, no ROM magic / ROM-derived path / ROM-derived content). Simulator .app: 307 MB (the Rust binary is 212 MB
  unstripped), of which `byor-src` 70 MB, Python frameworks 14 MB, stdlib 10 MB.
- Launch with an empty data dir shows the setup screen (touch buttons, per-game status); `JUMPMAN_IOS_IMPORT_ROMS=<Documents/dir>`
  identifies the 4 ROMs by checksum and imports them through the embedded CPython: **all four games `@@DONE`**, wall time from launch
  to the last stamp **191 s** (Super Mario 64 ~120 s) on the Mac's simulator; process RSS sampled every 10 s peaked at 260-380 MB (the
  7-second Mario Kart step, 660 MB on the Mac, fell between samples: not captured on the simulator).
- Data dir after import: 331 MB (`stamps/`, `assets/`, `public/`, `roms.json`, `import.log`); no picker copies left behind.
  Compared with the committed `assets/` + `public/`: **3827 of 3889 files byte-identical, 62 differ** (all OoT glTF files:
  `kokiri_forest`, `deku_tree`, `oot_enemies`, `oot_residents`, `oot_scenery`). The differences are in the glTF JSON only
  (accessor `min`/`max` last digit, e.g. `-0.49576331150284525` vs `-0.4957633115028453`). Probable cause (not isolated): the
  interpreter version, CPython 3.13 here vs 3.9 on the Mac that produced the committed files; `export-oot-courses.py` uses
  `math.hypot(*v)`, whose rounding changed in 3.10. The same code path on the Mac with 3.9 is byte-identical (3890/3890).
- Relaunch after the import: the menu comes up with Link / Young Link / Banjo and Mario Kart enabled, and Bob-omb Battlefield runs
  from the imported data (screenshots taken).
- The same in-process layer on the Mac (Python 3.9, a staged bundle, `PATH` empty so nothing can be spawned) imports all four games
  byte-identical to the repo (3889 files; READMEs are not generated).

Not verified / known gaps:
- **No real device run.** Jetsam, thermals, the document picker (the simulator tooling here cannot tap), Files/AirDrop "Open in",
  background-task expiry and `idleTimerDisabled` are implemented but only compiled.
- ROM bookmarks are stored but not yet used for silent update re-imports (a stale game asks for its ROM again).
- Restart-after-import (the app closes) is a workaround, see above. Per-game IMPORT buttons are replaced by one picker (ROMs are
  identified by checksum, so a per-game button would not change anything).
- CANCEL stops at the next output line; a step that prints nothing for a long time (the SM64 audio encode) stops late.
- A bug this found in the existing macOS bundle: `byor/scripts-manifest.txt` lacked `bk-maps.py`, `data/bk-catalogue.json`,
  `export-mk64-music.py`, `music/mk64-template.*` and `export-oot-items.py`, so the macOS `Jumpman-BYOR.app` would have failed for
  Mario Kart / Ocarina / Banjo (the repo checkout hid it). Added to the manifest and `GAME_SCRIPTS` (the hashes of those three games
  change, so macOS installs show "update available" once).

## Import speed, memory and leaving the app (importer-perf)

**Speed** (this Mac, release build, load average about 12 from other jobs, same ROMs, output verified byte-identical with
`byor/compare-data-dir.py` against a single-thread import of the previous code for all four games: 3052 / 389 / 687 / 836 files, 0 differ):

| game | before | after (default threads) | peak RSS before -> after |
|---|---|---|---|
| Super Mario 64 | 130 s | 5.5 s (4 threads: 7.0 s, 1 thread: 13.7 s) | 266 -> 572 MB (4 threads: 455 MB) |
| Mario Kart 64 | 6.7 s | 2.8 s | **15,100 -> 250 MB** |
| Ocarina of Time | 8.3 s | 8.7 s | 740 MB |
| Banjo-Kazooie | 3.8 s | 4.0 s | 210 MB |

The SM64 model exporter spent 98 percent of its time in one quadratic loop (`split()` rescanned the rest of the text for every
comma of the big vertex arrays): fixing that alone is 133 s -> 11 s on one thread; display-list parsing is cached, and the 2,700 model
jobs run on `std::thread::scope` workers (`byor_rs::worker_threads()`: available parallelism, at most 4 on iOS, `BYOR_THREADS=1`
gives the sequential reference). MK64 kept every decompressed MIO0 block it had ever touched (the kart frames alone touch thousands):
the cache is now capped at 48 MB.

**Leaving the app.** The simulator does not suspend a backgrounded process, so I could not reproduce a frozen import there:
(a) HOME mid-import: the import keeps running, UI correct on return; (b) `kill -STOP` for 30 s then `kill -CONT` plus bringing the app
back (what a real suspension does to the threads): the import and the UI continue and finish; (c) `kill -9` mid-import (what
jetsam does): before this change the relaunch showed an idle setup screen; now the launch sees `.import-active`, removes the
half-written `.stage`, logs it, shows "The last import was interrupted ... importing again" and restarts from the ROM copies still in
`inbox/` (all four games complete, stamps written last, nothing half-written ever looks installed). The most likely real-device
causes of "paused and never recovers" are the 15 GB MK64 peak (jetsam kills the app, the old UI then looked idle) and the 130 s SM64
step (a long window for the system to suspend or kill a backgrounded app); both are fixed. Not verified on a real iPhone.

Hardening added: the setup App uses `WinitSettings::continuous()` (no low-power timer when unfocused), a lifecycle system logs
`AppLifecycle` to `import.log` and forces a redraw on resume, the UI shows "Running for N s; no progress for M s", the Objective-C
host logs didEnterBackground / willEnterForeground / memory warnings / background-task expiry (with `os_proc_available_memory`)
into the same `import.log`, and two dumps of one game are imported once.
