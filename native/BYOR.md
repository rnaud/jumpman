# Bring your own ROM (BYOR)

The shipped app contains no Nintendo game data. On first launch it asks for your own ROMs and
extracts the assets on your machine.

## User flow

1. Launch `Jumpman`. With no extracted assets a **setup window** opens (also: `jumpman --setup`).
2. Drag your US ROMs onto the window (`.z64`, `.v64` and `.n64` byte orders all work; files are
   identified by checksum, not file name). Super Mario 64 is required; the others are optional:
   Mario Kart 64 (kart racing), Ocarina of Time US 1.2 (Zelda courses, Link), Banjo-Kazooie (Banjo).
3. Press Enter. `native/byor/pipeline.py` extracts into the per-user data dir
   (`~/Library/Application Support/Jumpman`, `$XDG_DATA_HOME/jumpman`, `%APPDATA%\Jumpman`;
   override with `JUMPMAN_DATA_DIR`). SM64 takes a few minutes.
4. The setup process re-executes the game (no manual restart). Later launches go straight to the game.
   Add another game later with `jumpman --setup`.

Headless/test: `jumpman --rom PATH [--rom PATH ...]` queues ROMs and starts extraction immediately.

## Requirements

macOS: Xcode Command Line Tools (provides `python3`, `make`, `cc`; the decomp builds its
`tools/sm64tools` at extract time). The setup window detects a missing toolchain and Press I runs
`xcode-select --install`. No other dependency; the exporters use only the Python standard library.

## Layout

- `src/byor.rs`: data dir, stamps (`stamps/<game>.json`, version `PIPELINE_VERSION`), setup window, relaunch.
  Asset lookup (`asset_root` in `lib.rs`) tries the data dir after the bundle and before the repo checkout.
- `byor/pipeline.py`: stages scripts + decomp sources in `<data>/.stage`, runs the exporters per game,
  installs results, writes the stamp, deletes the stage. One game failing never removes another's output.
  Prints `@@PROGRESS game pct msg`, `@@DONE game`, `@@FAIL game msg`.
- `byor/package-byor.sh`: builds `Jumpman-BYOR.app` (engine + original assets + `byor-src`).

## What ships vs what is extracted

Ships (original): `assets/ui/*` (menu backdrop, quilt tile, fonts), `audio/kart-voices.json`,
`audio/character-sounds.json`, `public/mk64/item-*.png`, `public/models/quaternius` (CC0), icons.
Everything else under `assets/` and `public/` (about 99% by size) is ROM-derived and produced by the pipeline.

Verified: running the pipeline on all four ROMs reproduces the repo's `assets/` and `public/` file
sets exactly, except the files below.

## Known gaps

- **Music**: `assets/audio/music/{sm64-us.wasm,courses.json,sounds.json}` come from
  `scripts/render-music.py`, which needs WASI SDK 34 and a pinned sm64-port checkout (not available to end
  users). Without the wasm the game still runs: `audio::processor` logs "audio engine unavailable" and the
  SM64 engine output (music and engine-mixed SFX) is silent. A data-less wasm template patched after extraction is the planned fix.
- Non-SM64 availability filtering in the Zelda/kart/character pickers (`shell/*`) is not done: a missing
  game's entries still appear and fail to load.
- Only macOS/unix `exec` relaunch and Xcode CLT detection are implemented; iOS has no setup flow.
- `menu-backdrop.png` provenance is undocumented; confirm it is original before distributing.

## OPEN LICENSING QUESTION (owner decision)

`byor-src/reference` bundles the n64decomp/sm64, n64decomp/mk64, zeldaret/oot and n64decomp/banjo-kazooie
source trees (Nintendo/Rare-derived code; these repos contain no ROM data but are not clearly
redistributable) and the SM64 audio engine wasm template would carry the same issue. Options:
fetch them on first run from their public repos instead of shipping them, or ship only after review.
