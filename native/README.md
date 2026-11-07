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
| `JUMPMAN_COURSE/ACT/ENTRY`, `JUMPMAN_EXPERIMENT` (`dimensions god runner hundred diorama race shrink planet swarm relic`), `JUMPMAN_CHARACTER` | start state |
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

## Star Swarm (experiment)
An original arena-survivor ("horde survivor") mode, native-only. Pick **Star Swarm** from the green experiment
buttons on the title or pause menu, or run `cargo run --release -- bob --experiment swarm`.

You run, jump, dive and ground-pound on Mario's real physics inside a walled plaza; your stars fire at the nearest
foes on their own. Goombas ("skitters"), thwomps ("brutes"), spinies ("dashers" that wind up, then lunge) and boos
("wisps" that shoot) swarm in from the edge with wave scaling over time (count, health, speed, damage), a ring of
skitters closes in every 40 s and a king bob-omb mini-boss arrives every 2 min. Kills drop gems (gold/blue/red by
value) and sometimes a heart; gems get magnetised, fill the XP bar, and every level-up pauses the game and offers
three upgrade cards (tap, keys 1/2/3, arrows + Enter, or gamepad d-pad + A): Star Power, Rapid Fire, Twin Stars, Big
Bang, Piercing Light, Swift Boots, Spring Legs (jump), Ground Quake (ground-pound shockwave), Hearty, Mending, Gem
Magnet, Lucky Star (crit) and the rare Nova Pulse, Orbit Stars and Second Chance. Contact has i-frames and a jump
clears small foes. The run ends at 0 health; the summary card offers Retry / Main Menu and the best survival time
is kept in `swarm.json` beside the save files (`progress::save::data_dir`, so it works in the iOS app container).

Code: `experiments/swarm_core.rs` (engine-free, seeded, unit-tested: XP curve, wave scaling, upgrade rolls and caps,
i-frames, determinism, peak-load tick cost), `swarm.rs` (arena, Mario glue, render lists), `swarm_view.rs` (pooled
actor models, HUD, cards, summary on `bevy_ui`; ~220 live foes on desktop, 160 on iOS, gems culled by distance).
Env hooks: `JUMPMAN_SWARM_SEED=<n>` (reproducible run), and for screenshots `JUMPMAN_SWARM_START=<secs>` (start with
that minute's waves), `JUMPMAN_SWARM_XP=<n>`, `JUMPMAN_SWARM_GOD=1`, `JUMPMAN_SWARM_SHOWCASE=1` (one of each foe);
runs using these never write the best time. Scripted card picks: `JUMPMAN_XP_KEYS="3:1"`.

## Last Star Standing (experiment)
An original last-one-standing match, native-only. Pick **Last Star Standing** from the green experiment buttons on the
title or pause menu, or run `cargo run --release -- bob --experiment laststar`.

You and 39 AI Marios (23 on iOS) glide down onto Bob-omb Battlefield from a launch height: steer with the stick
(any speed), hold A to dive down faster, and choose where to land. Then scavenge: loot camps, chests and falling
airdrops hold **Bounce Shells** (thrown, they roll and ricochet), **Lit Bob-ombs** (lobbed, fused, big blast),
**Fire Flowers** (rapid fireballs), **Star Bursts** (radial blast + a moment of invincibility), **Quake Charges**
(your next ground pound lands as a huge shockwave), **Shield Caps**, **Swift Boots**, **Hearts** and **Lucky Shrooms**,
in three rarities (damage and uses scale). The inventory is two weapon slots plus one consumable slot: walk over an
item to take it (a rarer one swaps out your worst). Mario's own moves are the skill ceiling: punch, **dive** (hit
at range, big knockback), **ground-pound** (stomp a head, or a shockwave that only hurts grounded rivals: jump it) and
wall kicks all matter. Hits do damage with brief i-frames and real knockback; knocking a Mario off the island or into
water counts as an elimination, credited to whoever hit them last. **The Blight** (a purple fog wall) closes in over
six phases (hold, warning, shrink) and drains health outside the safe circle faster each phase; the next circle is
marked on the minimap and by a gold beam. The match lasts about six minutes; the last Mario standing wins.

HUD (all `bevy_ui`, phone-first): health and shield, alive and knockout counts, the zone timer line, a minimap with
the safe and next circles and a rim pointer at the next centre, an elimination feed ("X knocked Y out"), name tags on
nearby rivals. Slots are keys `1` `2` `3` (hotbar buttons, clickable), d-pad left/up/right on a pad, and on touch
three round buttons above the A/B cluster inside the safe-area insets. After you are out the camera follows a rival
(`<` `>` / Q E / shoulder buttons switch) and the placement card shows "#N of M", knockouts and survival time with
Spectate / Retry / Main Menu; your best placement and win count live in `lss.json` beside the save files
(`progress::save::data_dir`, so it works in the iOS app container), as does the chosen difficulty.

Bots: each has an aggression (how readily it picks fights), lands near loot, scavenges, rotates into the next circle
before the Blight arrives, engages when it meets you (shells, flames and bombs at range, dives, pounds, punches), flees
when hurt, and uses consumables. Bots near the action (the nearest 14, 8 on iOS) run on the full SM64 physics as
`CrowdMario` bodies (A Hundred Marios' tech) and press real stick and buttons; the rest are moved by a cheap kinematic
mover in the core. The whole field costs about 30 microseconds per 30 Hz step on an M-series Mac.

Code: `experiments/lss_core.rs` (engine-free, seeded, unit-tested: zone schedule and damage, loot tables and rarity,
inventory, damage/shield/i-frames/knockback, melee shapes, projectiles, eliminations, placements, bot state
transitions, a whole simulated match for determinism, 40-bot tick cost), `lss.rs` (island measurement, terrain adapter,
bodies, glue), `lss_view.rs` (pickups/shots/blasts/Blight wall, HUD, minimap, feed, tags, cards on `bevy_ui`).
`touch.rs` gained `ModeButtons` (extra round touch buttons for a mode).
Env hooks (screenshots and tests; runs using the state-changing ones never write records): `JUMPMAN_LSS_SEED=<n>`,
`_BOTS=<n>`, `_DIFF=easy|normal|hard`, `_START=<secs>` (skip the drop, fast-forward the clock), `_PHASE=<n>` (start in
phase n's shrink), `_ALIVE=<n>`, `_GOD=1`, `_KIT=1`, `_DIE=<secs>`, `_SPECTATE=1`, `_FIGHT=1`, `_LOOT=1`, `_EDGE=1`,
`_TOPDOWN=<height>` (overview camera), `_LOG=1` (step cost every 5 s). Scripted item use: `JUMPMAN_XP_KEYS="4.0:1,4.4:2"`.
## Relic Descent (experiment)
An original action-RPG "looter dungeon crawler", native-only. Pick **Relic Descent** from the green experiment buttons
on the title or pause menu, or run `cargo run --release -- bob --experiment relic`. It plays as whichever character you
picked (Mario, Link, Young Link, Banjo) on that character's real physics.

* **Dungeon**: three seeded floors of 6-8 rooms joined by three-tile corridors on a tile grid (walls are merged boxes: one
  wall mesh + a few floor meshes per floor), low brick walls so the camera sees over them, torches, a fog/point-light mood,
  a minimap (rooms light up as you visit them, treasure gold, stairs, boss red) and a stairs ring. Packs sleep until you
  enter their room (so only one pack is ever awake; cap 80 live foes, 64 on iOS). The farthest room holds the stairs; floor 3
  ends in the **Bomb King** (three phases: stomp rings, then a call for help and bomb volleys, then enraged charges).
* **Combat on Mario's moves**: B (or the touch ATK button, `Enter` held) swings (cone in front: Mario's punch combo is wide and
  quick, Link's slash long, Young Link jabs fast, Banjo's claw heavy); **running + B is the dive = the dodge, with i-frames**
  (touch DODGE / `B` key forces a dive from standing); the ground-pound landing is the **Quake Slam** shockwave (the SLAM
  button leaps and slams for you). Skills with spark cost and cooldowns: Whirl, Star Volley (fan of stars), Aegis (shield);
  a 12-perk skill tree (11 nodes unlock/upgrade the four skills, plus Vigor) spends one point per level (two to start).
  Foes: Scamp (goomba swarmer), Bruiser (bully, telegraphed ground slam), Lancer (spiny: wind-up then charge), Wraith (boo:
  keeps range, shoots), Sapper (black bob-omb: lit fuse then blast), Seer (summoner), plus elite modifiers (Swift, Stout,
  Leeching, Volatile, Warded) with a ring under them. Hit numbers, crit gold numbers, flashes, knockback, health bars.
* **Loot**: kills and chests drop gear (weapon, armor, boots, two charms) in five rarity tiers with random affixes (damage, crit,
  attack speed, move speed, life steal, max health, cooldown cut, dodge frames, pickup radius, thorns; no duplicates), item
  level from floor and character level, six legendary effects (Sunsplit Blade, Quakebrand, Mantle of Ten Thorns, Skiff
  Treads, Lantern of Second Chances, Magpie's Knot), gold, potions. Auto-pickup; ground labels in rarity colours with
  beams from epic up; the loot filter (default on) turns commons into gold on the spot. The pack page (BAG button / `I`)
  shows worn gear, the bag, and a compare card with green/red deltas, EQUIP / SELL, SELL COMMONS and the filter toggle.
* **Meta**: gold from a run is banked in `relic.json` beside the save files (`progress::save::data_dir`, so it works in the
  iOS container): 60% on death, 100% on victory; every 500 banked gold gives +2% damage and health (max +20%). Also the
  deepest floor and the loot-filter choice.
* **Controls**: touch uses the stick, A/B/Z/L as ever plus ATK, DODGE, SLAM/WHIRL/STARS/AEGIS, POT, BAG and TREE (all inside the
  safe area and clear of the pause button; they dim while recharging). Keyboard: `Enter` attack (hold), `B` dodge, `1`-`4` skills,
  `H` potion, `I` pack, `T` skills, `Y` filter. Gamepad: d-pad left/right/down = skills, up = potion, RB attack, Select pack, L3
  skills (East still punches/dives).

Code: `experiments/relic_core.rs` (engine-free, seeded, unit-tested: damage and crit, armour, i-frames, XP and skill tree, pack
and boss scaling and phases, room wake-up, descending, pickups and bag, determinism, a bot clearing a room, tick cost with a
full crowd), `relic_loot.rs` (rarity weights, affixes, item-level scaling, compare, equipment totals), `relic_dungeon.rs`
(generator: connectivity, determinism, wall boxes), `relic.rs` (floor meshes/collision, Mario-action glue, publishing),
`relic_view.rs` (bridge, pooled models, overlays, input) and `relic_ui.rs` (HUD, pack, skill tree, summary on `bevy_ui`).
The touch layer has mode buttons (`touch.rs`: `Control::Extra`, `extra_layout`).
Env hooks: `JUMPMAN_RELIC_SEED`, `_FLOOR` (start floor), `_LEVEL`, `_GOD=1`, `_GIVE=legendary:weapon,epic:armor,...`, `_BAG=<n>`,
`_DROPS=<n>` (loot on the floor), `_TREE=2:1,3:2` (perk index:ranks), `_POINTS`, `_SCREEN=bag|tree`, `_SELECT=<bag index>|worn<slot>`,
`_TELEPORT=boss|stairs|treasure|combat|<room>`, `_BOT=1` (walks at foes and swings), `_HORDE=<n>`, `_SHOWCASE=1`, `_END=win|lose`
(summary screens). Runs using them never touch the banked gold / best floor.

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

## Mario Kart 64 free roam

Choose **Mario Kart 64** in the experiment menu, or launch directly:

```sh
cargo run --release -- playground --experiment kart
```

Uses the same `public/mk64/` export as the Three.js version. From the repository
root run `pnpm export:mk64 '/path/to/Mario Kart 64 (U) [!].z64'` once. The exporter
checks the US ROM checksum; the ROM is never copied or bundled. Mac and iOS
packaging copy the generated track, textures and sprite atlas.

**W/S** accelerate, brake and reverse; **A/D** steer. The gamepad or touch stick
also works. **F**, gamepad **B**, or the dock's **Get out** button parks the kart
and switches to Mario's full native walking/jumping simulation. Return nearby
and press it again to get in. **R** or the **Reset** button resets both to the
starting line. **X** leaves the experiment.

Luigi Raceway geometry and textures and the occupied kart's directional sprites
come from MK64. Driving is a custom 30 Hz controller matching `kart.ts`, with
quarter-step track collision and off-road slowdown. The empty parked kart is a
procedural mesh. Native MK64 audio, opponents, items and racing rules are not yet
ported. `cargo test --lib experiments::kart::tests` verifies driving and transfers;
the extracted-track test also runs when the local ROM export is present.
