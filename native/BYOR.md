# Bring your own ROM (BYOR)

`target/Jumpman-BYOR.app` contains no Nintendo game data. On first launch it asks for your own ROMs and
extracts everything the full build ships (models, courses, textures, music engine, karts, characters) on
your machine. After the import the game is functionally identical to the full build.

Platform: **macOS only** (Apple Silicon build; the pipeline needs the Xcode Command Line Tools). iOS has no
setup flow (the App Store build ships the full assets), and Windows/Linux relaunch-after-setup and toolchain
detection are out of scope: `byor.rs` compiles there and the pipeline is plain Python, but only macOS is
packaged and tested.

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

## Requirements

Xcode Command Line Tools (provides `python3`, `make`, `cc`, `clang`; the decomp builds its `tools/sm64tools` and
the audio bank encoder at extract time). The setup window detects a missing toolchain and Press I runs
`xcode-select --install`. No other dependency; the exporters use only the Python standard library. About 400 MB
of disk for the extracted data (about 300 MB) plus a temporary stage.

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

## What ships vs what is extracted

Ships (original): `assets/ui/*` (menu backdrop, quilt tile, fonts; the backdrop is original artwork),
`audio/kart-voices.json`, `audio/character-sounds.json`, `public/mk64/item-*.png` (procedural),
`public/models/quaternius` (CC0), icons, the data-less audio engine template (`scripts/music/sm64-us-template.*`,
decomp CODE compiled to wasm with its four sound blobs zero-filled), and the decomp source trees (below).

Extracted from the ROMs (not in the bundle): `assets/{actors,levels,extracted,courses,oot-dialogue,audio/*}`,
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
