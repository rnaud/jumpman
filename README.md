# Jumpman

Super Mario 64's controller and physics, ported from the [n64decomp/sm64](https://github.com/n64decomp/sm64)
decompilation to Rust, plus a pile of game modes built on top of that engine. The game is a
[Bevy](https://bevy.org) 0.20 app in `native/` that runs on macOS and iOS (shipped to TestFlight as
**Jumpman Medley**). There is also a bring-your-own-ROM macOS build.

> The earlier Three.js browser build and its Cloudflare deployment have been removed. Everything
> here describes the native app. Older notes in `docs/` and in some Rust comments still mention the
> TypeScript original; they are kept as history.

![Mario in Bob-omb Battlefield](screenshots/23-sm64-battlefield.png)

## Contents

1. [Quick start](#quick-start)
2. [What is in the game](#what-is-in-the-game)
3. [Controls](#controls)
4. [Characters](#characters)
5. [Courses](#courses)
6. [Settings](#settings)
7. [Fidelity: what is ported and what is not](#fidelity-what-is-ported-and-what-is-not)
8. [Build, run and test](#build-run-and-test)
9. [Packaging: iOS, macOS, bring-your-own-ROM](#packaging-ios-macos-bring-your-own-rom)
10. [Asset pipeline](#asset-pipeline)
11. [Repository layout](#repository-layout)
12. [Documentation map](#documentation-map)
13. [Credits and legal](#credits-and-legal)

## Quick start

```sh
export PATH=/opt/homebrew/opt/rustup/bin:$PATH   # only if rustup came from Homebrew
cd native
cargo run --release                              # title screen, assets read from ../assets
cargo run --release -- bob --act 2               # jump straight into a course
cargo run --release -- --list                    # list course ids
cargo test --workspace --locked                  # simulation goldens + headless gameplay tests
```

The repository already contains the exported assets (`assets/`, `public/`), so a plain checkout
runs without any ROM. ROMs are only needed to regenerate those assets
(see [Asset pipeline](#asset-pipeline)). Jumpman is not an emulator and does not read a ROM at
runtime.

## What is in the game

**Movement playground.** The simulation (`native/sim`, crate `jumpman-sim`) is engine-free Rust:
the SM64 analog-stick pipeline, the action state machine, 30 Hz quarter-step integration, slope and
surface handling, triangle collision, health, caps, and ports of many object behaviours
(Bob-ombs, Goombas, Chain Chomp, King Bob-omb, Koopa the Quick, Whomp, Boos, Bullies, Pokeys,
Klepto and others). The Bevy app renders it with the original Mario model and 209 native
animation clips, SM64 music and sound effects, a save file system (four slots) and a title/pause
hub.

**Courses.** 54 course ids in the catalogue (28 Mario, 26 Zelda); see [Courses](#courses).

**Game modes.** The title screen and pause menu have a **MODES** tab with one card per mode. Tapping a
card always starts the mode from a freshly loaded course. 17 modes exist
(`native/src/experiments/mod.rs`); `native/README.md` has per-mode goals, controls and the tests that
prove each is completable.

| Mode | Key | One line |
| --- | --- | --- |
| Across Dimensions | `dimensions` | Cycle the course between 3D, 2D side, top-down and 2.5D |
| Mario vs. God | `god` | Move the world with the mouse while Mario keeps playing |
| Endless Run | `runner` | Seeded auto-runner with daily seed and ghost |
| A Hundred Marios | `hundred` | 25/50/100 full-physics Marios with five personalities |
| Diorama | `diorama` | The course as a plastic toy case; poke the cast |
| Summit Race | `race` | Race seven rivals up Bob-omb Battlefield |
| Shrink Ray | `shrink` | Rescale the whole course around Mario |
| Tiny Planet | `planet` | Spherical mini-world with core-pointing gravity |
| Star Swarm | `swarm` | Survive-the-swarm with upgrades |
| Mario Kart 64 | `kart` | Luigi Raceway free roam, items and a three-lap race with CPUs |
| Last Star Standing | `laststar` | Forty Marios drop in, one survives |
| Relic Descent | `relic` | Dungeon crawl ending at the Bomb King |
| Hold the Gate | `gate` | Tower-defence around a castle gate |
| Star Deck | `deck` | Deckbuilder roguelike built from Mario's moves |
| The Painted Halls | `halls` | Metroidvania in Mario's moves |
| Gem Clash | `brawl` | Original arena brawler: throwable props, gems, a timed power form, Versus and a boss Gauntlet |
| Sunstone Shindig | `party` | Original four-player board party: three boards, 15 mini-games, CPU opponents |

Start a mode from the hub, from the in-game dock/`X` menu, or with
`cargo run --release -- --experiment <key>` (also `JUMPMAN_EXPERIMENT=<key>`).
Mode thumbnails are in `assets/ui/modes/` (see [Credits and legal](#credits-and-legal)).
The original design brief for the first modes is [docs/experiments/BRIEF.md](docs/experiments/BRIEF.md).

### Mario Kart 64

Luigi Raceway is imported from the MK64 decomp (original vertices and display lists, ROM textures,
Mario's directional kart sprites). It starts in free roam with rainbow item boxes (mushroom, banana,
green shell); the **Race** button starts a three-lap race against three CPU karts with checkpoints,
lap and best-time tracking. Driving is a port of the human-player model in `player_controller.c`
(speed-tier acceleration, steering ladder, drift and mini-turbo, floor and wall response) at 60 Hz;
the chase camera follows `camera.c`. Get out of the kart to walk with Mario's normal controller.
Kart sounds are synthesised from `assets/audio/kart-voices.json`; MK64's own samples and music are
not extracted. Not ported: boost ramps, AB-spin, terrain tumble, non-grass surface types, kart-to-kart
collision, the other 19 courses and the original items beyond the three above.
Details: [native/README.md](native/README.md#mario-kart-64-free-roam).

## Controls

Keyboard bindings are rebindable (Settings, then Controls; stored in `controls.json`). Defaults, from
`native/src/bindings.rs`:

| Input | Action |
| --- | --- |
| `W A S D` / arrow keys | Move (camera-relative analog stick) |
| `Space` | Jump, double and triple jump; hold for height |
| `Shift` or `J` | Punch, grab, throw, dive, jump kick |
| `Ctrl` or `K` | Crouch; ground pound in the air |
| `Alt` | Walk (half stick) |
| `Q` / `E`, mouse drag | Orbit camera |
| Mouse wheel | Zoom |
| `C` | Cycle camera mode |
| `X` | Snap camera behind Mario; opens the mode menu / leaves a mode |
| `L` | Focus / strafe (Link) |
| `H` | Aim the slingshot (Young Link) |
| `Space` (Link next to Epona / on her) | Mount; carrot (dash) while she moves; dismount when she stands. Stick steers her, `Shift` / `J` also dismounts. Gamepad A / touch A the same |
| `V` | Swap character |
| `R` | Restart the run |
| `M` | Mute |
| `Esc` / Start | Pause |

**Gamepad.** GameController.framework is bridged into Bevy gamepads on macOS and iOS (Xbox,
DualSense/DualShock 4, Switch Pro and MFi pads; several pads take player seats in connect order).
Left stick moves, right stick orbits, South jumps, East punches, West/North/triggers crouch,
left bumper focuses, Start pauses. Pad buttons are rebindable too.

**Touch.** On iOS and touchscreens a virtual stick (bottom-left), L/Z/B/A buttons (bottom-right)
and swipe-to-orbit appear; menus switch to a compact phone layout under 960x560 logical pixels.

The moves reproduced from the decomp include: run and analog turning, single/double/triple jumps,
backflip, side flip, long jump, dive, jump kick, ground pound, wall kick, ledge grab and climb,
ceiling hang, pole climbing, swimming, grab/carry/throw (Bob-ombs, crates), butt/stomach slides, crawling,
and the full set of surface-friction classes (normal, slippery, very slippery, ice, quicksand, lava).

## Characters

Press `V` or use PLAY AS in the hub. Every character plays every course.

| Character | Model and animation source | Moveset |
| --- | --- | --- |
| **Mario** | Original SM64 model and 209 animation clips, exported from the ROM and decomp by `export-sm64.py` | The ported SM64 controller |
| **Link** (adult) | OoT US 1.2 model with Master Sword and Hylian Shield, 21 limbs, 19 ROM animation clips (`public/models/oot/link.json`) | Mario's controller with Link's roll, sword slash, shield and focus/strafe |
| **Young Link** | OoT child model (`public/models/oot/young-link.json`), same exporter | Slingshot, sword, jab-style attacks |
| **Banjo** (with Kazooie) | BK US 1.0 model (`ASSET_34E`, 63 bones, CI4 textures) and 37 Rare keyframe animations (`public/models/bk/banjo.json`) | Jump, Feathery Flap, Claw Swipe, Forward Roll, Rat-a-tat Rap, Crouch, Flap Flip, Beak Barge, Beak Buster, translated from `bs*.c` states |
| Quaternius mannequin | Quaternius Universal Animation Library, CC0 (`public/models/quaternius/`) | Fallback rig for custom characters |

Each Link/Banjo move maps onto the SM64 action with the same role (punch, jump kick, ground pound),
so enemies, boxes and objects react through their normal interactions. Not ported for Banjo: Talon
Trot, eggs, Wonderwing, flight, swimming moves.

**Voices.** `assets/audio/character-sounds.json` maps each action to sound layers per character. Mario
uses his SM64 voice clips. Link, Young Link and Banjo have no voice yet (the OoT and BK audio banks
are not extracted); their voice actions are silent and damage uses the generic hit sound. Footsteps,
landings, swimming and coins are shared. `JUMPMAN_VOICE_LOG=1` prints the clip chosen for each voice action.

Custom rigs: drop a character into `public/models/local/` (everything except its README is
gitignored). See [public/models/README.md](public/models/README.md).

## Courses

The catalogue (`native/src/game/courses_table.rs`) has **54 course ids**:

- **Mario (28):** Castle Grounds, Inside the Castle, the 15 numbered courses (Bob-omb Battlefield, Whomp's
  Fortress, Jolly Roger Bay, Cool Cool Mountain, Big Boo's Haunt, Hazy Maze Cave, Lethal Lava Land, Shifting
  Sand Land, Dire Dire Docks, Snowman's Land, Wet-Dry World, Tall Tall Mountain, Tiny-Huge Island, Tick Tock
  Clock, Rainbow Ride), The Secret Aquarium, Tower of the Wing Cap, Princess's Secret Slide, the three Bowser
  courses (Dark World, Fire Sea, Sky) and three Bowser arenas, the side-scrolling Mushroom Kingdom 1-1, and the
  Movement Playground.
- **Zelda (26):** Link's House, Kokiri Forest and Shop, the Mido's, Saria's, Know-It-All Brothers' and twins'
  houses, Inside the Deku Tree, Lost Woods, Sacred Forest Meadow, Hyrule Field, Lon Lon Ranch, Kakariko Village,
  Graveyard, Zora's River/Domain/Fountain, Lake Hylia, Gerudo Valley and Fortress, Haunted Wasteland, Desert
  Colossus, Hyrule Castle, Death Mountain Trail and Crater, Goron City.

Exact ids: `cargo run --release -- --list`; mission names and acts: `native/src/game/courses_table.rs`.
Bob-omb Battlefield has all six missions (King Bob-omb, Koopa the Quick, red coins, flying rings, Chain
Chomp gate). Other SM64 courses range from partial mission logic to terrain plus a subset of objects; see
[DECOMP_TALLY.md](DECOMP_TALLY.md) for what each is missing.

The Kokiri Forest route has the village children, Saria, Mido and the shopkeeper with ROM dialogue boxes, a
rupee wallet and shop, and the Deku Tree with Gohma; the other Zelda courses are mostly exploration
terrain. Scenes, collision and dialogue are exported from OoT US 1.2.

## Settings

Settings are saved in `settings.json` (`~/Library/Application Support/Jumpman/` on macOS; override with
`JUMPMAN_CONFIG_DIR`). Under *Audio and performance*:

- **Show FPS**: an overlay with frame rate, frame time and simulation tick rate.
- **Uncap FPS**: off uses vsync; on removes the app-side frame limiter (on iPhone it requests
  80 to 120 Hz instead, because wgpu cannot disable display sync there). The simulation is a fixed
  30 Hz step with interpolated rendering, so frame rate never changes game speed.
- **Light quality**: *Off*; *On* (screen-space ambient occlusion, soft shadows with a 4096 map and FXAA; subtle on the
  flat-lit N64 materials and not ray tracing); and, only on Macs whose GPU exposes Metal ray queries, *Ray traced (hardware,
  restart)*: real hardware ray tracing through `bevy_solari` (traced sun shadows, sky light and bounce light, accumulated with
  TAA). Ray tracing switches the renderer to deferred, so it is read at launch (the Settings note says whether it is ACTIVE
  or applies after a restart). It is never offered on iPhone/iPad or the simulator: it would need an A17 Pro / M-class GPU and
  iOS 18, and has not been tried on a device.
- Env overrides (not saved): `JUMPMAN_SHOW_FPS=1`, `JUMPMAN_UNCAP=1`, `JUMPMAN_RT=off|enhanced|rt`.

Other settings: character, save slot, music/effects volume, key and pad rebinding.

## Fidelity: what is ported and what is not

The reference is `n64decomp/sm64` (US, commit `9921382`, vendored at `reference/sm64`). The method is
file-for-file porting: the Rust simulation was originally ported from a TypeScript simulation that was
itself ported from the decomp, and is checked bit-for-bit against golden fixtures
(`native/sim/tests/fixtures/`, committed). Arithmetic is f64, not the N64's f32, so individual frames
are not guaranteed to match hardware.

**Ported (checked against golden fixtures):**

- Input pipeline (`update_mario_inputs`), joystick mapping and camera-relative direction.
- Ground and air quarter-step movement, `set_mario_action`, same-frame action cancels.
- Floor, wall and ceiling collision with the 50-unit cell grid and surface classes.
- 161 of the decomp's 233 Mario actions are defined (156 handled): stationary, moving, airborne,
  automatic, object, and parts of submerged and cutscene groups.
- Health wedges, hurt/heal counters, knockback, invincibility, death animations, caps (wing, metal, vanish).
- Object system and behaviours for the courses listed above.
- Original SM64 sound engine (sequence player compiled to WebAssembly, run with wasmi), 219 samples,
  terrain footsteps, water effects, per-course music and reverb presets.

**Not ported or approximate (summary):**

- 72 Mario actions: swimming holding objects, metal-cap water actions, rollouts, burning, many
  cutscene actions (intro, credits, entering BBH, squished, stuck in ground), first-person.
- `camera.c`: 0 of 15 camera modes ported verbatim. The camera is a custom orbit camera with
  Lakitu/close/top-down/side presets.
- Roughly half of the SM64 object behaviours placed in levels have no handling (Thwomp, Grindel,
  Chuckya, Amp, Bullet Bill, Piranha Plant, Ukiki, Mips, many more). Several levels are
  scenery-only. The three cap levels and the castle courtyard missions are incomplete.
- Object-to-object collision, `platform_displacement.c` (platform carry is a re-implementation).
- Cutscenes, intro, file-select flow, ending.
- MK64: 1 of 20 courses; OoT: about a quarter of the scenes as geometry only, Link's moveset is an
  adaptation (1 of 89 `Player_Action_*` functions named); BK: only Banjo's model, animations and nine moves.

The authoritative, regularly updated per-subsystem accounting (with counting method and a script,
`node scripts/decomp-tally-check.mjs`) is [DECOMP_TALLY.md](DECOMP_TALLY.md). Roadmaps:
[docs/SM64-PORTING-PLAN.md](docs/SM64-PORTING-PLAN.md), [docs/COMPLETENESS.md](docs/COMPLETENESS.md),
[native/PORTING.md](native/PORTING.md), [native/AUDIT.md](native/AUDIT.md).

## Build, run and test

All commands run from `native/`. Requirements: Rust stable (via rustup), macOS for the Metal build.

```sh
cargo run --release                         # run from the checkout
cargo run --release -- <course> [--act N] [--area N] [--entry NODE]
cargo run --release -- --experiment kart    # start a mode directly
cargo test --workspace --locked             # about 300 tests
cargo test --lib experiments::              # per-mode "can be completed" tests
```

Short names `kokiri`, `dungeon`, `shop`, `link` select Zelda courses. Asset lookup order: `JUMPMAN_ASSETS`,
the app bundle's `Resources/assets`, `assets/` beside the executable, then the source checkout.

Tests include sim goldens (`native/sim/tests/`), differential and long-run course ticks
(`native/src/game/*_tests.rs`), per-mode completion tests with a scripted player, character audits
on every course, and audio cue tests. CI is `.github/workflows/`.

**Headless and debug hooks** (full table in [native/README.md](native/README.md#env-hooks-selection)):

| Variable | Effect |
| --- | --- |
| `JUMPMAN_COURSE`, `JUMPMAN_ACT`, `JUMPMAN_ENTRY`, `JUMPMAN_EXPERIMENT`, `JUMPMAN_CHARACTER` | Start state |
| `JUMPMAN_EXIT_AFTER=<s>`, `JUMPMAN_SCREENSHOT=<png>` | Headless smoke run and screenshot |
| `JUMPMAN_SCREEN=title\|pause\|courses\|settings\|controls`, `JUMPMAN_SHELL_SCRIPT` | Force or script menus |
| `JUMPMAN_TOUCH=1`, `JUMPMAN_WINDOW=WxH`, `JUMPMAN_SAFE_AREA=l,r,b,t` | Phone-layout checks on desktop |
| `JUMPMAN_DEBUG=1`, `JUMPMAN_MUTE=1`, `JUMPMAN_FPS=1` | Debug panel, silence, FPS to stdout |

## Packaging: iOS, macOS, bring-your-own-ROM

| Target | Command (from `native/`) | Output / notes |
| --- | --- | --- |
| macOS app | `./package-mac.sh` | `target/Jumpman.app`; bundles the whole `assets/` tree (about 380 MB). `JUMPMAN_BUNDLE_COURSES=bob,ccm` bundles fewer courses |
| iOS simulator | see `native/README.md` ("iOS / TestFlight") | Xcode project in `native/ios/`; the game is a Rust static library |
| iOS TestFlight | `TEAM=<team id> BUNDLE_ID=<id> UPLOAD=1 ./package-ios.sh` | Needs `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, Xcode signing and App Store Connect access |
| Bring-your-own-ROM macOS app | `./byor/package-byor.sh` | `target/Jumpman-BYOR.app`: engine plus original assets only; the user drops their own ROMs on a setup window and extraction runs on their machine |

The packaging scripts read the committed `assets/` and `public/` (including `public/mk64/`); they do
not need ROMs. iOS 26 and UIScene specifics, the universal iPhone/iPad setup and controller
support are documented in [native/README.md](native/README.md). The BYOR flow, requirements, what
ships versus what is extracted, known gaps and the licensing decision are in [native/BYOR.md](native/BYOR.md).

## Asset pipeline

The Python exporters in `scripts/` use only the standard library. They read your ROMs and the decomp
checkouts in `reference/` and write the shared `assets/` (SM64, audio, UI) and `public/` (models,
MK64) trees that the native app, the packaging scripts and the BYOR pipeline consume. Run them from
the repository root.

| Command | Produces |
| --- | --- |
| `(cd reference/sm64 && python3 extract_assets.py us)` | Extracts the ROM's resources into `reference/sm64` (needs `reference/sm64/baserom.us.z64`, your own SM64 US ROM, which is not in the repo). Prerequisite for the next four |
| `python3 scripts/export-sm64.py` | Actor and level GeoLayouts, Fast3D display lists, textures and native animation tracks to GLB under `assets/actors`, `assets/levels`; `assets/animations.json`, `assets/layouts.json`, `assets/export-report.json`. `--required-only` for a quick export |
| `python3 scripts/export-courses.py` | Collision, placements and surface data for each course to `assets/courses/*.json` |
| `python3 scripts/export-audio.py` | All 219 extracted AIFF samples to PCM WAV in `assets/audio/` plus `manifest.json` |
| `python3 scripts/check-sm64-library.py` | Validates the exported GLB library and its coverage report |
| `python3 scripts/export-mk64.py reference/roms/mk64-us-1.0.z64` | Luigi Raceway geometry, textures, kart sprites and HUD art to `public/mk64/` (needs `git submodule update --init reference/mk64`) |
| `python3 scripts/export-oot-courses.py` | OoT US 1.2 scenes, rooms, collision, text font and dialogue to `assets/levels`, `assets/courses`, `assets/oot-dialogue` (needs `reference/oot`; helpers `oot-residents.py`, `oot-scenery.py`) |
| `python3 scripts/export-oot-link.py` | Adult and Young Link models and animations to `public/models/oot/` (needs `reference/oot`) |
| `python3 scripts/export-oot-epona.py` | Epona (`object_horse`: skinned model, nine animations) and Link's riding clips to `public/models/oot/epona.json` |
| `python3 scripts/export-bk-banjo.py` | Banjo model and animations to `public/models/bk/banjo.json` from `reference/roms/bk-us-1.0.z64` and `reference/banjo-kazooie` |
| `python3 scripts/render-music.py --port <sm64-port> --wasi-sdk <path>` | Builds the SM64 sequence-player WebAssembly and `courses.json`/`sounds.json` in `assets/audio/music/`. Needs WASI SDK 34 and the sm64-port checkout at `2b17d081`; the built output is committed |
| `python3 scripts/make-kart-items.py` | Procedural kart item icons and the item-box mark in `public/mk64/` (no ROM) |
| `python3 scripts/make-mode-thumbnails.py [--force] [key ...]` | Mode-card thumbnails in `assets/ui/modes/` via the OpenAI image API (needs `OPENAI_API_KEY` and Pillow) |

Other scripts: `scripts/decomp-tally-check.mjs` recounts the numbers in `DECOMP_TALLY.md`;
`scripts/publish-experiment-issues.py` files the mode design briefs as issues.
Submodules: `git submodule update --init reference/oot reference/mk64 reference/banjo-kazooie`
(`reference/sm64` is vendored).

## Repository layout

```
native/                 Rust workspace (the game)
  sim/                  jumpman-sim: engine-free simulation (math, collision, actions, objects)
  src/                  Bevy app: game/, render/, camera/, audio/, shell/ (menus), progress/ (saves),
                        characters/, zelda/, experiments/ (all game modes), controller/, touch.rs
  ios/                  Xcode wrapper (Rust static library entry point)
  byor/                 bring-your-own-ROM extraction pipeline and packaging
  package-mac.sh        macOS app bundle
  package-ios.sh        iOS build and TestFlight upload
assets/                 exported game data: actors, levels, courses, audio, UI art, mode thumbnails
public/                 models (oot, bk, quaternius, local slot), mk64/ export, app icon
scripts/                Python exporters and tools (see Asset pipeline)
reference/              decomp sources (sm64 vendored; oot, mk64, banjo-kazooie submodules) and roms/
docs/                   roadmaps, audits and mode design briefs (partly historical)
screenshots/            images used by docs
DECOMP_TALLY.md         what is not ported, per game
```

## Documentation map

- [native/README.md](native/README.md): detailed native doc (menus, env hooks, per-mode design and tests, controllers, performance, iOS, MK64).
- [native/PORTING.md](native/PORTING.md): porting method, stages and sim parity audit.
- [native/AUDIT.md](native/AUDIT.md): native audit notes.
- [native/BYOR.md](native/BYOR.md): bring-your-own-ROM app.
- [DECOMP_TALLY.md](DECOMP_TALLY.md): remaining work per decompilation.
- [docs/](docs/): SM64 porting plan, completeness audit, animation audit, experiment briefs (historical, written against the browser build).

## Credits and legal

- Physics, action IDs, input logic and course data are ported from the decompilation projects
  [n64decomp/sm64](https://github.com/n64decomp/sm64) (the sm64-port sound engine is also used),
  [n64decomp/mk64](https://github.com/n64decomp/mk64),
  [zeldaret/oot](https://github.com/zeldaret/oot) and
  [n64decomp/banjo-kazooie](https://github.com/n64decomp/banjo-kazooie).
- Models, textures and animations for Mario, Link, Young Link, Banjo, the MK64 course and the OoT
  scenes are extracted from the original games by the exporters above.
- [Quaternius' Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html)
  is CC0 1.0 (`public/models/quaternius/`).
- Mode thumbnails in `assets/ui/modes/` are AI-generated (OpenAI `gpt-image-2`) with an original
  character, not Nintendo artwork; prompts and method are in `assets/ui/modes/README.md` and
  `scripts/make-mode-thumbnails.py`.
- Gem Clash, Sunstone Shindig and the other non-Nintendo modes are original designs on this engine.
- Super Mario 64, Mario Kart 64, Ocarina of Time and Banjo-Kazooie and their characters are
  trademarks of their owners. This is a non-commercial fan project.
- Jumpman's own code is MIT licensed (`LICENSE`). That license does not cover the Nintendo- and
  Rare-derived game data.

**Repository status, stated plainly.** The goal is that public releases contain no ROMs and no
extracted game assets: the bring-your-own-ROM app exists for that reason. The repository itself does
not meet that goal today. It is private and currently tracks the OoT, MK64 and BK ROMs under
`reference/roms/`, and the generated `assets/` and `public/` data derived from the SM64 ROM and the
other ROMs. The SM64 ROM itself (`reference/sm64/baserom.us.z64`) is not tracked. Do not make this
repository public or redistribute builds made from it without resolving that first. The licensing
questions about bundling decompilation sources in the BYOR app are recorded in
[native/BYOR.md](native/BYOR.md).
