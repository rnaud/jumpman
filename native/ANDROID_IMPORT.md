# On-device ROM import for Android (BYOR): measurement, decision, status

**Status (latest): the SM64 import is complete in pure Rust.** `byor-rs --source-root <repo or byor-src> --data-dir D --sm64 ROM`
writes a data dir for which `native/byor/compare-data-dir.py <python reference dir> D` reports **3075 files, 3075 byte-identical,
0 differ, 0 only-in-either** (the reference is a full `pipeline.py` run on the same ROM; 82 s Python vs 155 s Rust on this Mac).
It also works from a minimal bundle of only the 2,279 files of `decomp-manifest.txt` plus the original assets (41 MB, 9.2 MB
zipped), which proves the manifest is sufficient for the Rust exporters too. MK64, OoT and BK are not ported.

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
| SM64 | done, verified | all done and byte-identical | all done and byte-identical: courses (`courses*.rs`, `sm64_world.rs`), post-processors (`courses_post.rs`), models/animations/layouts (`models.rs`), audio, music engine | yes on the Mac CLI (verified); on Android see section 9 |
| MK64 | done, verified | n/a (reads the ROM's MIO0 blocks directly) | all done, 339 files byte-identical to the Python pipeline (section 11) | yes on the Mac CLI (verified); packaging as SM64 |
| OoT | done, verified (identify only) | not started | not started (2,963) | no |
| BK | done, verified | n/a (rarezip asset table read directly) | all done, 646 files byte-identical to the Python pipeline (section 11) | yes on the Mac CLI (verified); packaging as SM64 |

So, today, **only SM64 can be imported end to end** (MK64, OoT, BK cannot). On Android the pure-Rust importer identifies the picked ROMs,
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

1. (done) SM64 music engine: `sound_banks.rs` ports `assemble_sound.py` (ctl/tbl/sequences/sets, including the N64 DMA-garbage
   padding model) and `sm64::music_wasm` the template patch; the ADPCM bytes come straight from the ROM (so `vadpcm_enc` is not
   needed). `assets/audio/music/sm64-us.wasm` is byte-identical to the Python pipeline's (`tests/sm64_rom.rs`, against a full
   `pipeline.py` run: 82 s, 266 MB `assets/`, 6.4 MB `public/` on this Mac).
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

## 7. Bundling the decomp sources in the APK (plan, not implemented)

The SM64 exporters read 2,279 decomp files listed in `native/byor/decomp-manifest.txt` (25.0 MB raw, **4.4 MB** as one
deflated zip, measured). Plan that keeps `verify-apk.py` meaningful:

1. `byor/stage-byor-src.sh` already stages exactly that set (the iOS bundle). Add a step to `.github/workflows/android.yml` that
   zips the staged `byor-src/reference/sm64` plus `scripts/music/*` and `native/byor/sm64-us-sound-player.bin` into one file
   `byor-src.zip`, and a Gradle `assets` source set that packages only that file as `assets/byor-src.zip` (the `.zip` suffix is
   not in `BAD_EXT`).
2. `verify-apk.py` gets one narrow exception: `assets/byor-src.zip` may exist if and only if every member (a) is listed in
   `decomp-manifest.txt` (or is one of the three non-decomp helper files above), (b) has no ROM magic, (c) is not in the
   binary-asset denylist of `verify-byor-app.py` check 5 (png/jpg/glb/wav/aiff/aifc/m64/bin/tbl/ctl/z64), json allowed only for the
   manifest-listed paths (`assets.json`, `sound_banks/*.json`, `sequences.json`). The check reads the zip, so it cannot be
   bypassed by naming.
3. On first launch `MainActivity` extracts the zip to `<files>/byor-src` (4.4 MB, one-off); `byor.rs` passes that dir as
   `--source-root`, and `pipeline::sm64_assets_json` already looks under `reference/sm64`. (`assets.json` is 467 KB of offsets
   and sizes, no ROM data.)

## 8. What stops "full SM64 import on the emulator" today

Remaining Python to port for SM64: `export-courses.py` (745, regex over the level scripts, writes `assets/courses/*.json`),
`sm64_world.py` (407), `export_sm64_g2.py` (260), `export-sm64-g1-actors.py` (160), `export-sm64-solids.py` (334),
`export-sm64-bosses.py` (153), `export-sm64-castle.py` (198), `export-sm64.py` (489, Fast3D display lists and geo layouts to GLB, with
`ast`-evaluated C expressions and PNG decoding/re-encoding through `zlib`), `c2obj.py` (141). Notes for whoever does it:

* all of it is regex/`re.findall` over decomp C: add the `regex` crate (the only two lookaheads, in `export-sm64.py` lines 64 and 86,
  are easy to hand-code);
* GLB and JSON writers must match Python's `json.dumps` (key order: serde_json `preserve_order` is already enabled in byor-rs) and
  `repr(float)`;
* PNGs that `export-sm64.py` re-encodes with `zlib.compress` (decal baking) are only reproducible bit for bit with the C zlib
  (`flate2` with the `zlib` feature, level 6); with miniz they would match on decoded pixels only;
* verification: a full Python reference data dir takes 82 s (`python3 -I native/byor/pipeline.py --source-root . --data-dir D
  --sm64 ROM`); `native/byor/compare-data-dir.py REPO D` then works unchanged once the Rust pipeline writes `assets/` and `public/`.

## 9. Android packaging of the SM64 import (this stage) and what was run

* `native/android/make-byor-src.py` builds `byor-src.zip` (2,313 files, 9.2 MB): the 2,279 sm64 decomp files, the music template, the
  sound-player sequence and the original assets. Gradle packages only that file (`src/byorAssets`, git-ignored), the CI workflow got one
  step to build it, `verify-apk.py` allows `assets/byor-src.zip` and nothing else under `assets/`, checking every member against the same
  allowlist, ROM magic, size, and that the music template's four ROM blob ranges are all zero.
* `MainActivity` unpacks it once per installed build to `<files>/byor-src` (zip-slip safe); `android::init_env` sets `JUMPMAN_BYOR_SRC`
  to that dir; the setup screen passes it as `--source-root`.
* Numeric notes for a different libm: the model exporter calls the platform `sin`/`cos`/`pow` like CPython does, and float printing
  matches Python's `repr` (ties to even); on Android (bionic libm) the last bit of a rotation channel's printed accessor min/max may
  differ from a Mac import. The binary buffers are f32 and unaffected in the tests.

## 10. Emulator run of the full import (not a Retroid)

AVD `retroid` (arm64, API 35, headless, swiftshader), APK built locally with cargo-ndk + gradle (verify-apk.py: OK):
install, start, CHOOSE ROM FILES, pick `Super Mario 64 (USA).z64` from Downloads (SAF copy into the app), the import starts by itself and
**finishes in about 200 s**, the screen shows "100% installed". On the device: 3,074 files under `Jumpman/assets` + `public`
(the Mac reference has 3,075: only a README that the bundle leaves out), and md5 of `courses/bob.json`, `levels/bob/bob_geo_000488.glb`,
`audio/music/sm64-us.wasm`, `audio/manifest.json`, `layouts.json` equal the Mac pipeline's. After restarting the app the Jumpman menu
lists the SM64 levels (MK64 says "Import Mario Kart 64 ROM in Setup") and **Castle Grounds runs** with the HUD, Mario, the castle and
trees (a screenshot was taken; not committed because it shows Nintendo textures). Frame pacing on swiftshader is poor (the log shows
swap-chain timeouts), so nothing here says anything about speed on an Adreno handheld. No gamepad, no real Vulkan, no Retroid.

## 11. MK64 and BK ported (pure Rust, no Python)

`byor-rs/src/mk64/*` and `byor-rs/src/bk/*` port the complete `do_mk64` / `do_bk` chains of `byor/pipeline.py`, driven by the same
`--mk64 ROM` / `--bk ROM` command line and `@@` protocol, with the same stamp (`rust-mk64-1`, `rust-bk-1`, see `pipeline::PORTED`).

| Python | Rust |
|---|---|
| `export-mk64.py` (courses' kart JSON, karts, HUD, shells, item curves), `mk64_courses.py`, `mk64_props.py` | `mk64/export.rs`, `mk64/courses.rs` (display-list interpreter, decal layers, collision, GLB writer) |
| `export-mk64-cpu.py`, `export-mk64-modes.py` | `mk64/cpu.rs`, `mk64/modes.rs` |
| `export-mk64-music.py` (byte-swapped banks patched into `mk64-template.wasm`) | `mk64/music.rs` |
| `export-bk-banjo.py` (+ `--extras`) | `bk/banjo.rs` (rarezip reader in `bk/mod.rs`) |
| `export-bk-levels.py`, `bk-maps.py` | `bk/levels.rs`, `bk/maps.rs` (reads the cached `scripts/data/bk-catalogue.json`, which the bundle ships) |
| `export-bk-enemies.py`, `export-bk-setup.py` | `bk/enemies.rs`, `bk/setup.rs` |
| `export-character-voices.py` (BK half), `export-bk-sfx.py` | `bk/voices.rs` |
| (none: new in Rust) BK music: `comusic` sequences, sound font, map -> track table | `bk/music.rs` (`assets/audio/music/bk/`, played by `src/bk_music/`); the Rust setup / enemy export also places Clucker, Grille Chompa and Flibbit, which the frozen Python exporters skip, so `assets/courses/bk_*.json` no longer match the Python output (`tests/bk_rom.rs` ignores them) |

Shared helpers: `zpng.rs` (the Python exporters' `png()` = `zlib.compress`, `round(x, n)`, int-vs-float JSON numbers).
Not ported because the pipeline never runs it: `bk-catalogue.py` (regenerates the checked-in catalogue from the decomp).

Verification (this Mac, real US ROMs): a Python reference data dir per game (`python3 -I native/byor/pipeline.py --source-root . --data-dir D --mk64 ROM`,
about 4 min for MK64) against the Rust output (11 s MK64, 4 s BK; `byor-rs --source-root . --data-dir D2 --mk64 ROM`) with `compare-data-dir.py D D2`:
MK64 339 files, BK 646 files, **all byte-identical, 0 differ, 0 missing**. The same result with `--source-root` set to the unzipped
`byor-src.zip` (built by `android/make-byor-src.py`, which now also ships the mk64 and banjo-kazooie manifest files, both music templates and
`bk-catalogue.json`; `verify-apk.py` checks both templates are data-less), i.e. the manifest is sufficient. `tests/mk64_rom.rs` and `tests/bk_rom.rs`
repeat the comparison (`BYOR_TEST_MK64_ROM`/`BYOR_TEST_MK64_REF`, `BYOR_TEST_BK_ROM`/`BYOR_TEST_BK_REF`; skipped when unset).
Numeric notes: PNG levels match the Python calls (6 for `export-mk64.py`, 9 inside GLBs and for the BK level textures; flate2 with the `zlib` feature
reproduces the bytes); `x ** .5` / `** 2.4` call libm `pow` like CPython; `round(x, n)` is Rust's exact half-to-even formatting (unit test against CPython).
Not run: the Android emulator with MK64 / BK (only the Mac CLI and the unzipped bundle were run); the game itself was not started on the imported dirs.
