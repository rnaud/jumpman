# On-device ROM import for Android (BYOR): measurement, decision, status

Goal: the Android build (Retroid Pocket, arm64, Android 8+) imports the user's own ROMs on the device, like iOS, with no
Nintendo data in the APK. iOS does it with an embedded CPython 3.13 running `scripts/export-*.py` plus the decomp C tools
(`native/IOS_IMPORT.md`). The suggestion for Android was a pure-Rust port instead of embedding Python.

**Nothing was run on a real Retroid.** Verified on the Mac (aarch64-apple-darwin) against real ROMs: all Rust logic. Verified
on an arm64 API 35 *emulator* (AVD `retroid`, headless, swiftshader GLES, 320x640 screen, no gamepad): the APK built locally with
cargo-ndk + gradle installs and starts, shows the setup screen, CHOOSE ROM FILES opens the system picker (Rust -> JNI -> Kotlin),
picking a byte-swapped SM64 .v64 copies it into the app inbox, Rust identifies it as SM64 (JNI callback -> setup screen), IMPORT
starts by itself and fails with the "exporters not ported" message, and the inbox copy is deleted. Not exercised: multiple
files at once, gamepad navigation, real Vulkan/Adreno, performance.

## 1. What the pipeline does (inventory)

`native/byor/pipeline.py` (395 lines) is the driver: byte-swap and identify the ROM by checksum (SM64 and MK64 by SHA-1, OoT and
BK by MD5), stage a copy of `scripts/`, the decomp sources and the original assets, run one `do_<game>` step (a list of exporter
scripts), then swap the result into the data dir, write `stamps/<game>.json` (pipeline version + a hash of exporters and decomp
inputs) and `roms.json`. Progress goes out as `@@PROGRESS/@@DONE/@@FAIL` lines that the setup screen (`src/byor.rs`) parses.

Per game (line counts are the Python in `scripts/`, `reference/c2obj.py`; decomp sources are what the exporters read, listed in
`native/byor/decomp-manifest.txt`, bundled in the app because they are code and text only):

| Game | Exporter chain | Python lines | Decomp inputs | External tools / libs |
|---|---|---|---|---|
| SM64 | `extract_assets.py us` (decomp, ~290) then export-courses, sm64_world, export_sm64_g2, g1-actors, solids, bosses, castle, export-sm64, export-audio, render-music-template (+render-music) | 3,182 (incl. c2obj 141) | 2,279 files, 25 MB (levels/actors C, model.inc.c, collision, behaviors) | decomp C tools via `make`: mio0 (575 lines C), n64graphics (1,077, with stb_image_write), skyconv (671), aifc_decode (669), vadpcm codebook/encoder; `tools/disassemble_sound.py` (811); prebuilt music engine `music/sm64-us-template.wasm` (2.5 MB, patched, not compiled) |
| MK64 | export-mk64, mk64_courses, export-mk64-cpu, export-mk64-music | 1,504 | 117 files (not present on this Mac: `reference/mk64` is an empty submodule, so MK64 could not be run or compared here) | zlib, `music/mk64-template.wasm` |
| OoT | link, items, epona, courses (1,240 alone), enemies, residents, scenery, character-voices, sfx | 2,963 | 573 files, 12 MB | Yaz0 decompression (in Python), zlib |
| BK | maps, banjo, levels (707), enemies, setup, character-voices | 1,822 | 12 files (`reference/banjo-kazooie` is an empty submodule here) + `scripts/data/bk-catalogue.json` | zlib, ROM-internal compression |

Total: about 9,500 lines of Python plus about 3,400 lines of C in the decomp tools. Python dependencies are the standard library only
(`struct re json zlib wave hashlib`; no numpy, no PIL, no external packages). The only subprocesses are the decomp's own tools
(`make`, the C compiler, `mio0`, `n64graphics`, `skyconv`, `aifc_decode`), which is why iOS links them in (`ios/import/jt_*.c`, 475
lines) and patches `subprocess` (`byor/inproc.py`, 378 lines).

What the work consists of (by kind, not by game):

1. **Shared low level** (needed by all games): ROM identify/byte-swap, MIO0 and Yaz0 decompression, N64 texture decoders
   (RGBA16/32, IA, I, CI), PNG writing, WAV writing, VADPCM decode, display-list and collision parsing, glTF/GLB and JSON
   writers.
2. **Decomp-source readers**: the SM64 exporters parse the decomp's C (`levels/*/script.c`, `model.inc.c`, `geo.inc.c`,
   `collision.inc.c`, behaviors) with regular expressions (34 regex call sites in `export-courses.py` alone) and `c2obj.py`.
3. **Exporters proper**: course/model/animation/actor-placement conversion into the game's JSON and GLB.

A finding that matters for the plan: several exporters write PNGs with Python's `zlib.compress(rows, 9)` (BK, MK64, OoT, SM64
models), and the decomp's `n64graphics` writes textures with `stb_image_write` (its own small deflate). A pure-Rust port cannot
reproduce `zlib -9` output bit for bit without linking the C zlib; it can reproduce stb's exactly (it is a 150-line
deterministic algorithm, and that is done, see below). For `zlib.compress`-written PNGs the comparison has to be on decoded pixels.

## 2. Options and the decision

| | (a) full Rust port per exporter | (b) Rust shared layer + staged exporters | (c) embed CPython 3.13 for Android |
|---|---|---|---|
| New code | ~9.5k Python lines to ~13-15k Rust, plus ~4k for the tools | same total, delivered in stages, each stage independently verified and shippable | CPython-for-Android build in CI (about 30 min, cacheable), NDK build of the `jt_*` C tools (~3.9k lines of C, already written for iOS), a JNI host (`byor_ios.rs` twin) |
| Reuses | nothing | `pipeline.py` protocol, stamps, `compare-data-dir.py` | all of `pipeline.py`, `inproc.py`, every exporter unchanged |
| Output vs Mac | needs verification per exporter (byte-exact except zlib PNGs) | same | identical by construction (same code) |
| APK size | +1-2 MB | +1-2 MB | +25-30 MB (interpreter + stdlib subset) |
| Import time | seconds (SM64 asset stage below: 0.3 s vs 12 s Python+C) | same | like iOS: minutes for SM64 |
| Verifiable on this Mac | yes, every step | yes, every step | no: needs the Android NDK build to even link; I have no NDK here |
| Risk | long tail of exporters, many `re`-heavy | same, but front-loaded value | CI/toolchain risk, big opaque dependency, untestable here |

**Decision: (b).** Reasons, with numbers:

* It is the only path where each piece can be proven correct on this machine against the Python output. The SM64 asset stage
  below was ported and compared in one session: 1,309 output files, all byte-identical, 0.3 s instead of 12 s.
* The shared layer is a small fraction of the work and unblocks everything: the low-level tools are about 3.4k lines of C
  (about 1.2k lines of Rust expected; the ROM identify, MIO0, texture and PNG parts, about 600 lines, are done), versus 9.5k lines of
  exporters that sit on top of them.
* (c) costs a 25-30 MB interpreter, a 30-minute CI build and an NDK link that cannot be validated before a device test, to gain
  the exporters that (a)/(b) would otherwise have to port. It remains the **fallback** if the long tail of exporters stalls; it
  is not built. The pieces it would need (`jt_*.c`, `inproc.py`, `byor_ios.rs`) are already in the repo for iOS.
* Games are independent. SM64 is required by the app, so it goes first; MK64 (1.5k lines, no regex-heavy decomp parsing, but no
  local decomp checkout here to verify against) is the next cheapest; OoT and BK last.

The honest estimate for the rest, by line count and the 1.3-1.6x Python-to-Rust expansion seen so far: SM64 exporters about
4-5k Rust lines, MK64 about 2k, OoT about 4k, BK about 2.5k. Weeks, not days, for the whole set.

## 3. What is implemented (this change)

### Storage Access Framework file picker (Kotlin + JNI) - compiled, run on the emulator only

* `native/android/.../MainActivity.kt`: `pickRoms()` opens `ACTION_OPEN_DOCUMENT` (multiple files, `*/*`), no storage permission
  needed. `onActivityResult` copies each chosen document on a worker thread into `<external files>/Jumpman/inbox/<sanitised
  name>` (128 MB cap, `..`-safe names, partial files deleted on error) and calls `Native.romPicked(path)`.
* `native/src/android.rs`: `Java_..._Native_romPicked` queues the path; `pick_roms()` calls `MainActivity.pickRoms()` through the
  `jni` crate (android-only dependency; jni 0.21.1 was already in the lock file, pulled in by another crate, so no new download); `picked_pop()` is polled by the
  setup screen.
* `native/src/byor.rs`: the setup screen now runs on Android (touch buttons CHOOSE ROM FILES and IMPORT, same flow as iOS: auto
  start two seconds after SM64 is picked, "close and reopen" after the import because winit allows one event loop per process,
  inbox copies deleted afterwards). `reopen_setup` and the `.setup-requested` marker work for Android as for iOS.

### `native/byor-rs` (workspace member, std + serde_json only) - **verified on the Mac**

* `rom`/`hash`: SHA-1 and MD5, byte-swap for .z64/.v64/.n64, identification of the four supported ROMs. Verified on real ROMs
  (SM64 in all three byte orders, MK64, OoT 1.2, BK identified; Paper Mario correctly rejected).
* `pipeline`/`cli`: the `pipeline.py` command line and `@@` protocol (`--identify`, `--status`, `--print-hashes`, game ROM flags),
  the same stamp and `roms.json` formats and staleness rule. The game calls it in process (`byor::run_rust`, a channel standing in
  for stdout), always on Android and on desktop with `JUMPMAN_BYOR_RUST=1`, so the same setup screen can be tested here.
* `n64`: MIO0 decoder, RGBA16/32, IA16/8/4/1, I8/4 decoders (n64graphics).
* `png`: byte-exact port of `stb_image_write`'s PNG writer (filter heuristic, its deflate with the hash-bucket match finder, CRC,
  Adler).
* `sm64`: the port of `extract_assets.py us` driven by the decomp's own `assets.json`.
  **Done and verified:** 1,267 textures (rgba16, ia16, ia8, ia4, ia1) and 42 raw assets (`.m64` sequences, `.bin`):
  all 1,309 files are byte-identical to the output of `python3 extract_assets.py us` (and the `make`-built C tools) run from a
  copy of `reference/sm64` with the real US ROM. `byor-rs/tests/sm64_rom.rs` repeats the comparison
  (`BYOR_TEST_SM64_ROM`, `BYOR_TEST_SM64_REF`; skipped when unset, no ROM or extracted data is committed). That test also
  runs with the ROM given as a byte-swapped .v64.
  Later stages, same method (byte-identical to the Python/C output, `tests/sm64_rom.rs`): the 11 skybox/cake images
  (`skyconv --combine`), the 219 sound samples (`disassemble_sound` + the bruteforcing `aifc_decode` port; 1,320 files in the
  extraction test), `export-audio.py` (219 WAV files + `manifest.json`), and `music.rs`: `sounds.json` (US-build evaluation of
  `sounds.h`, no C compiler) and `courses.json`, identical to the checked-in files (`tests/music.rs`).

## 4. Status per game

| Game | ROM identify | Asset extraction | Exporters | Importable on Android end to end |
|---|---|---|---|---|
| SM64 | done, verified | all done and byte-identical (textures, sky/cake, sequences, bins, 219 samples) | `export-audio`, `sounds.json`, `courses.json` done; music engine wasm (needs `assemble_sound.py`, 1,094 lines), export-courses, sm64_world, g1/g2/solids/bosses/castle, export-sm64 not done | **no** |
| MK64 | done, verified (identify only) | not started | not started (1,504) | no |
| OoT | done, verified (identify only) | not started | not started (2,963) | no |
| BK | done, verified (identify only) | not started | not started (1,822) | no |

So, today, **no game can be imported on the device end to end**. On Android the pure-Rust importer identifies the picked ROMs,
and when you tap IMPORT it reports per game that the exporters are not ported yet (and to use the Mac import plus copy path of
`ANDROID.md`); it never writes a stamp, so a half-imported dir is never mistaken for a good one. The Mac copy path of `ANDROID.md`
still works as before.

## 5. How to test here

```sh
cd native
cargo test -p byor-rs                                   # unit tests (digests, byte orders, stamp rules)
cargo build -p byor-rs --release
# SM64 asset stage vs the Python/C extraction (make a copy of reference/sm64, put the ROM at baserom.us.z64, run
# `python3 extract_assets.py us` in it, then:)
BYOR_TEST_SM64_ROM=/path/sm64.z64 BYOR_TEST_SM64_REF=/path/copy/sm64 cargo test -p byor-rs --release
target/release/byor-rs --identify some.rom              # prints sm64 | mk64 | oot | bk | unknown
target/release/byor-rs --source-root .. --extract-sm64-assets sm64.z64 --out /tmp/sm64-out
JUMPMAN_BYOR_RUST=1 cargo run --release -- --setup      # the real setup screen driven by the Rust importer
```

`native/byor/compare-data-dir.py REPO DATA` is the comparison to use once an exporter stage writes into a data dir (the Python
`assets/` and `public/` of a local checkout are the reference); it is not usable for the asset stage alone, which writes decomp
tree files, not game assets.

## 6. Next steps, in order

1. SM64 music engine: port `tools/assemble_sound.py` (banks and sequences to ctl/tbl/sequences/sets blobs; the ADPCM sample bytes
   can come straight from the ROM instead of re-running `vadpcm_enc`, since `aifc_decode` round-trips) and the 80-line
   template patch in `render-music-template.py` (the sound-player bytes are precomputed in `byor/sm64-us-sound-player.bin`).
2. (done) sky/cake, samples, export-audio, sounds.json, courses.json.
3. SM64 exporters in the order of `do_sm64`: export-courses (745) and `c2obj`/`sm64_world` first (everything else builds on their
   C-source parsing), then g2/g1/solids/bosses/castle, then export-sm64 (489, models and animations). Compare each against the
   Python output with `compare-data-dir.py`; PNGs written with `zlib -9` by decoded pixels.
4. Android packaging: ship the decomp subset the exporters read (`decomp-manifest.txt`; SM64 25 MB, OoT 12 MB) as APK assets or
   a one-time download. `native/android/verify-apk.py` (and `byor/verify-byor-app.py`) currently forbid an `assets/` tree and must
   learn to allow exactly `byor-src/`. Not done: the stage-in-APK step is only needed once the first exporter is ported.
5. MK64 (needs the `reference/mk64` checkout), then OoT, then BK.
6. Hook the Rust pipeline's install step (stage dir, atomic swap, stamp last) into `pipeline::run` as the first game becomes
   complete; the stamp hash is then `rust-<game>-<revision>` (`pipeline::PORTED`). Note: a data dir copied from a Mac carries
   Python hashes, so the first Rust-ported game will show "update available" for that game on Android; that is intended.
