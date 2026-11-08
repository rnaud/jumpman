# Decompilation tally: what is NOT ported yet

Living document. Last refreshed 2026-11-08 (MK64 music row) against `main` at `0a2d36a` (+ this commit). Pinned references: SM64 `reference/sm64` (vendored, US, commit 9921382), MK64 `reference/mk64` @ `58cfcb0`, OoT `reference/oot` @ `14759ab`, BK `reference/banjo-kazooie` @ `e8d31fa`; ROMs under `reference/roms/` (OoT US 1.2, BK US 1.0, MK64 US 1.0; the SM64 ROM is local-only, gitignored).

Status vocabulary: **Ported (bit-exact)** = file-for-file port checked against golden fixtures generated from the decomp-derived TS (the TS itself is checked against the decomp by reading, not by an emulator); **Ported (approximate)** = same intent, different implementation or numbers; **Partial**; **Not ported**; **Out of scope** (N64 hardware, debug, localisation, things the project deliberately replaces). `unknown` means I could not determine it from the repo.

## Summary

| Game | Estimate | Bit-exact share | How computed |
|------|---------:|----------------:|--------------|
| Super Mario 64 | **~63%** | ~30 points of the 63 | Weighted sum of the SM64 subsystem estimates below. Weights (gameplay impact guess): actions 20, collision 8, objects/behaviors 21, levels 12, camera 8, menus/HUD/shell 6, audio 8, animation 5, cutscenes/ending 5, saves/caps 4, rendering/fx 3. Inputs: actions 156/233 handled = 67%; collision 80% (judgement); behaviors 63% (OBJECT placements in exported levels that the exporters name explicitly, 471/743); levels 26/31 = 84%; camera 25% (judgement, 0/15 modes ported verbatim); menus 50% (judgement); audio 75% (judgement: engine and 219 samples are exact, usage is partial); animation 146/209 = 70%; cutscenes 30% (judgement); saves/caps 85%; rendering 80% (120/129 actor dirs). |
| Mario Kart 64 | **~38%** | ~11 points | Weights: player physics 25, courses 20, course objects 10, items 8, CPU AI 8, audio 6, race logic/modes 6, collision 5, camera 5, menus 3, characters 2, replays 2. Inputs: physics 40% (judgement; 25/91 `player_controller.c` names appear in the kart sources), courses 20/20 = 100% (geometry, textures, collision with surface types and the lap path exported and playable on foot and in the kart; props and course objects are counted under objects, not here), items 3/15 = 20%, camera 4/15 functions = 27%, characters 1/8 = 12%, objects 3%, CPU AI 10%, race 15%, collision 15%, audio 40% (judgement: the sound library itself is the decomp's own code, all 29 music sequences load, the 20 course themes plus the start / final-lap / finish / star / lightning logic are ported; the SFX banks, engine and item sounds, character voices and the menu / title flow are not wired), menus and replays 0. |
| Ocarina of Time | **~12%** | ~1 point | Weights: scenes 25, Link/player 20, actors 20, dungeons 10, items 8, audio 7, bosses 5, textbox/cutscenes 5. Inputs: scenes 27/110 = 24% (geometry/collision; 17 of them terrain-only), Link ~10% (of 89 `Player_Action_*` functions, 1 is named; the moveset is a 106-line adaptation), actors ~6.5% (about 28 of 427 overlays have a simplified counterpart, 13 of them the enemies of `native/sim/src/oot_enemy/`), dungeons ~8%, items ~6%, audio ~2% (only Link's voice effects: 8 `NA_SE_VO_LI_*` sounds, adult and child, via `scripts/export-character-voices.py`), Gohma 1/11 bosses, textbox ~40%. |
| Banjo-Kazooie | **~28% of the character, ~6% of the game** | 0 | Playable character plus the 10 world terrains (geometry and collision only). Moveset 60% (about 20 of ~33 player-facing moves, see `native/BANJO_CONTROLLER.md`) at weight 35, model/animation 55% at weight 10, collectibles 25% as sandbox pickups, worlds ~30% of the level weight (no objects/setups/sub-areas), audio ~2% (Banjo and Kazooie voice effects only), enemies/cutscenes 0. |

These are estimates, not measurements; the mechanical inputs are re-countable with `node scripts/decomp-tally-check.mjs` (see the last section). Judgement items are marked "judgement" and should be revised when a port lands.

## Biggest gaps / suggested next ports (ranked by gameplay impact)

1. **SM64 swimming set (20 of 32 submerged actions missing)**: holding-object water actions, metal-cap underwater walking/jump/land variants (`ACT_*_METAL_WATER_*`), water knockback, shock, water punch/throw, whirlpool. Metal-cap water is "simplified" today. Pure sim work, golden-testable. Unlocks JRB/DDD/WDW fidelity.
2. **SM64 enemy/obstacle behaviors not ported**: Thwomp, Grindel, Spindel, Chuckya, Amp, Bullet Bill (WF cannon is a static model), Piranha Plant, Ukiki, Mips, Scuttlebug, Fly Guy, Moneybag, Tweester, Hoot, Bubba, Eyerok, Mr. I, Boo-in-castle, Unagi, Manta Ray, Sushi shark, Chirp Chirp. 106 of 198 distinct behaviors placed in exported levels have no explicit handling (was 123; TTC, RR, BITDW/FS/S and the three Bowser arenas now have their platforms, switches, flame jets and the faithful Bowser); most of LLL, SSL, WDW, DDD, JRB and the castle grounds are scenery-only.
3. **SM64 `camera.c`**: 0 of 15 camera modes ported verbatim, 48 `CUTSCENE_*` sequences not ported (only a few bespoke ones exist). The orbit camera is custom, so every fixed/parallel/spiral-stairs/8-direction/slide/Hoot area differs from the original.
4. **SM64 cutscene and special actions**: 19 of 51 cutscene actions (intro, credits, `ACT_BBH_ENTER_*`, electrocution, suffocation, squished, stuck-in-ground, putting on cap, warp-door spawn, emerge-from-pipe, Bubba) plus the intro/title/file-select flow.
5. **SM64 missing levels/areas**: the 3 cap levels (`cotmc`, `wmotr`, `vcutm`) and the castle courtyard (Boos/Mips), so cap-switch missions and the basement/courtyard stars cannot be completed. Bubble-heavy dynamic stages (WDW water level puzzle, THI size change, LLL volcano, SSL pyramid) are terrain-only; TTC's clock objects and RR's carpets/platforms are no longer.
6. **MK64 course props**: all 20 courses (16 race tracks, 4 battle arenas) are exported as geometry, textures, collision and lap path (`scripts/mk64_courses.py`), walkable in the LEVELS tab and drivable in the kart mode (16 tracks). Still missing per course: props (trees, signs, Yoshi Valley branching, Wario Stadium/Kalimari trains, Toad's Turnpike traffic, Banshee Boardwalk bats, Rainbow Road, Bowser's Castle thwomps, Koopa Troopa Beach crabs), water/lava as volumes, the in-game minimaps and skies (palettes are hand-picked), per-surface kart tyre behaviour beyond grass/asphalt.
7. **MK64 driving completeness**: spinouts and the full `apply_effect` set, boost ramps, AB-spin, terrain tumble, triple-A/B combos, non-grass/asphalt tyre surfaces (sand, snow, dirt, wood, water), star/boo/lightning/mini-turbo variants, kart-to-kart collision, start-line boost. Items: 12 of 15 missing (red/blue shells, triple items, fake box, star, boo, lightning, super mushroom).
8. **MK64 CPU AI and modes**: the real `cpu_vehicles_camera_path.c` path logic (the current CPUs are a custom path follower), Grand Prix/cup structure, Time Trials with staff ghosts, 2-4 player VS, Battle, replays, menus.
9. **OoT Link/z_player.c**: 88 of 89 `Player_Action_*` functions have no counterpart (ledge climbing, swimming/diving, crawl, ladders, hookshot, bow, bombs, boomerang, ocarina, spin attack, jump slash; Epona is now a separate port, see the row below). Plus the item/inventory/pause system and every dungeon other than the Deku Tree.
10. **BK**: everything beyond the 9 imported moves: 148 of 157 `BS_*` states (Talon Trot, Wonderwing, eggs, Beak Bomb, flight, swim/dive, climb, Wading Boots, Turbo Trainers, Shock Spring), all collectibles (notes, Jiggies, Mumbo tokens), 12 enemy species now exist in the 14 worlds (the other character/enemy classes stay unported), 104 BK maps exported (the 14 worlds + 90 sub-areas) with their enemy and collectible placements and 361 warps (doors, pipes, windows, lair doors, return pads) that join them, but no door actors, switches or other setup actors, no BK music or sound engine; only Banjo and Kazooie's voice effects (13 `SFX_*` sounds) are extracted, by `scripts/export-character-voices.py`.

## Super Mario 64 (reference/sm64)

### Mario actions (mechanical: `decomp-tally-check.mjs`)

233 actions in `include/sm64.h`. 161 are defined in the TS sim (`src/sim/state.ts`) and the Rust sim (identical set); 156 are referenced by a handler or transition. The 192 `ACT_*` constants the sim defines include 31 flag/group/mask constants, which is why older notes say "all 192"; the real figure is 161 of 233 (**69% defined, 67% handled**). The Rust sim equals the TS sim, so the same numbers apply to `native/sim/src/actions/*.rs`.

| Group | Decomp | Defined / handled | Status | Missing (not ported) |
|-------|-------:|------------------:|--------|----------------------|
| Stationary (`mario_actions_stationary.c`) | 38 | 26 / 26 | Partial | panting (+ hold variant), coughing, shivering, standing against wall, first person, heavy-object idle, `ACT_IN_QUICKSAND`, shockwave bounce, twirl land, uninitialized, unknown_0002020E |
| Moving (`mario_actions_moving.c`) | 40 | 35 / 36 | Ported (bit-exact), minor gaps | burning ground, heavy-object walking, ground bonk, quicksand jump land (x2) |
| Airborne (`mario_actions_airborne.c`) | 45 | 33 / 33 | Partial | forward/backward rollout, burning jump/fall, twirling, vertical wind, thrown forward/backward, special triple jump, crazy-box bounce, hold-water jump, riding Hoot |
| Submerged (`mario_actions_submerged.c`) | 32 | 12 / 12 | Partial | all hold-object water actions, breaststroke/flutter kick variants, water punch/throw, water shocked, water knockback x2, shell swimming, whirlpool, metal-cap walk/jump/fall/land x8 |
| Cutscene (`mario_actions_cutscene.c`) | 51 | 32 / 26 | Partial | intro, credits, end-waving, electrocution, suffocation, eaten by Bubba, warp-door spawn, emerge from pipe, BBH enter jump/spin, putting on cap, squished, head/butt/feet stuck in ground, reading automatic dialog, waiting for dialog, debug free move, disappeared |
| Automatic (`mario_actions_automatic.c`) | 17 | 15 / 15 | Ported (bit-exact), minor gaps | ledge climb down, tornado twirling |
| Object (`mario_actions_object.c`) | 10 | 8 / 8 | Ported (bit-exact), minor gaps | stomach slide stop, heavy throw |
| **Total** | **233** | **161 / 156** | **69% / 67%** | 72 actions (12+5+12+20+19+2+2); full list printed by the script |

Core pipeline (all **Ported (bit-exact)**, golden-tested against TS in `native/sim/tests/`, differential tests in `native/src/game/{longrun,game}_tests.rs`): `update_mario_inputs`, `perform_ground_step`/`perform_air_step` quarter-steps (12 of 22 `mario_step.c` names match), `execute_mario_action` same-frame cancels, `surface_collision.c` floors/ceilings/walls with the 50-unit cell buffer, `set_mario_action`, health/hurt/heal counters, cap timers.

Function-name overlap per decomp file (an upper bound: names appear in sim sources): `mario.c` 26/64, `mario_step.c` 12/22, `mario_actions_stationary.c` 31/44, `_moving.c` 43/73, `_airborne.c` 34/64, `_automatic.c` 16/28, `_object.c` 8/14, `_submerged.c` 4/57, `_cutscene.c` 4/94, `interaction.c` 11/70, `surface_collision.c` 6/16, `platform_displacement.c` 0/6 (platform carry is re-done in `src/game/movingPlatform.ts`). The low ratios on submerged/cutscene/interaction are partly renamings (the port uses table-driven handlers), so read them together with the action counts above.

### Physics, collision, surfaces, interaction

| Area | Status | Note | Lives in |
|------|--------|------|----------|
| Floor/wall/ceiling collision, cell grid, `find_floor` order | Ported (bit-exact) | golden-tested on bob/castle_inside plus 34 long-run courses | `src/sim/collision.ts`, `native/sim/src/collision.rs` |
| Surface classes (slippery, ice, lava, quicksand, shallow quicksand, water, wind...) | Partial | 37 of 151 `SURFACE_*` types are named in the sim; the rest are footstep/sound variants or unused. Wind (`SURFACE_HORIZONTAL_WIND`, `VERTICAL_WIND`, flowing water, `SURFACE_VERY_SLIPPERY` etc.) need checking per type: unknown | `src/sim/surfaces.ts`, `native/sim/src/surfaces.rs` |
| Moving-platform carry (`platform_displacement.c`) | Ported (approximate) | rebuilt colliders each tick; platform carry re-implemented | `src/game/movingPlatform.ts`, `native/src/game/platform.rs` |
| Object/Mario interaction (`interaction.c`, 70 functions) | Partial | grab, pick up, throw, punch, damage knockback, bounce, cap pickups, hit-by-shell; coin/star/door/warp/cannon/pole/flame interactions exist; `INTERACT_*` handlers for Bully/Boo/Koopa-shell exist; many others (Igloo/Whirlpool/Tornado/Clam/Mr.I/Bubba) do not | `src/sim/objectManager.ts`, `native/sim/src/object_manager.rs` |
| Object-object collision (`object_collision.c`) | Not ported | only explosion chains; objects do not push each other | `object_manager` |
| JavaScript f64 vs N64 f32 | Out of scope | documented divergence (apex 242, run speed 32 asserted) | README "Fidelity" |

### Objects and behaviors

533 `BehaviorScript`s in `data/behavior_data.c`, 226 `src/game/behaviors/*.inc.c` files. The project has 54 sim `ObjectBehaviorKind`s (TS and Rust, 30 behavior files each, golden-tested: `native/sim/tests/behaviors_golden.rs`, `focus`/`longrun`/`game` fixtures) and an app layer for goombas, moving platforms, cannons, signposts, wing switch, doors, warps, coin formations, star spawning and cap boxes. The exporters (`scripts/export-courses.py` and, for SM64 surface objects and the Bowser arenas, `scripts/export-sm64-solids.py`) explicitly handle 92 of the 198 distinct behaviors placed in the 26 exported levels (471 of 743 `OBJECT` placements = 63%, 46% of distinct behaviors; was 75 / 389 before the TTC, RR, BitDW/FS/S and Bowser work; bbh, hmc and lll gain a few placements only because `bhvFlamethrower` is now named in the second exporter, the flame jets themselves are exported for rr, bitdw, bitfs and bits only); the rest are exported as inert models, are decor, or are dropped. This is a name-mention proxy: "named" does not guarantee a faithful behavior.

Per-level coverage (placements / explicitly named by the exporter; mechanical): bob 28/27, bowser_1 1/1, bowser_2 6/6, bowser_3 16/16, ccm 13/13, wf 27/23, ttm 28/23, thi 23/18, ttc 9/7, totwc 3/3, bbh 34/26, hmc 39/28, castle_inside 104/79, sl 17/10, lll 57/24, ssl 33/15, jrb 75/18, rr 44/37, wdw 39/8, ddd 26/7, castle_grounds 49/9, bitdw 18/18, bitfs 26/24, bits 23/21, sa 4/2, pss 1/1.

| Behavior family | Status | Note | Lives in |
|-----------------|--------|------|----------|
| Bob-omb, Bob-omb Buddy (advice, cannon opener), King Bob-omb, breakable boxes, 1-Up, star/wing/cap boxes, chests, coins (moving yellow, formations, loot) | Ported (bit-exact) | golden-tested; King Bob-omb cutscene simplified | `src/sim/behaviors/`, `native/sim/src/behaviors/` |
| Chain Chomp + posts + gate | Ported (approximate) | chain segments constrained on a line, lunge speed lower than source (README) | `chain_chomp.rs` |
| Koopa, Koopa the Quick (race), shells | Ported (approximate) | source path collision approximated | `koopa.rs`, `koopa_the_quick.rs` |
| Boo, Big Boo, Bully/Big Bully, Whomp / Whomp King, Pokey, Klepto, Mr. Blizzard, Penguins (baby/mother/racing), Snowman (body/head), Flame, tree/pole climbing, poison gas, Bowser + mines + flame | Ported (bit-exact) | covered by longrun/focus goldens; bosses' cutscenes simplified | `native/sim/src/behaviors/*` |
| Goomba | Ported (approximate) | app layer, chase/stomp | `src/game/goomba.ts`, `native/src/game/goomba.rs` |
| Bowser 1-3 (`bowser.inc.c`, `bowser_bomb`, `bowser_flame`, `bowser_falling_platform`, tilting lava platform, flamethrowers) | Ported (approximate) | faithful action machine (20 actions, animation-frame driven, 3-health Bowser in the Sky, BitFS tilt/teleport, BitS shock wave + falling platforms, key / Grand Star event) in `native/sim/src/behaviors/bowser_sm64.rs` (kind `sm64Bowser`; the old simplified `bowser` stays for its golden). Scripted full-fight tests (grab tail, spin, throw into the mine, defeat event) in `sim/tests/sm64_bowser.rs`. Not ported: arena camera cutscenes, defeat dialog, eye/rainbow geo switches, tail joint (root-relative tail zone), mist/smoke puffs, sounds, the sky flame's floating landing flames (lands as ground flames) | `native/sim/src/behaviors/bowser_sm64.rs`, `native/sim/src/solids/bowser_arena.rs` |
| Cannons (player cannon, water bomb cannon) | Partial | player cannon action ported; the WF Bullet Bill cannon and tower are static models, Bullet Bill has no behavior | `src/game/cannons.ts` |
| Moving/rotating platforms (WF towers, BBH mesh elevator/merry-go-round/bookshelf/tilting trap, CCM track platform, BoB checkerboard) | Ported (approximate) | frozen or simplified to exported motion clips | `movingPlatform.ts` |
| | Thwomp, Grindel, Spindel, Chuckya, Amp, Piranha Plant, Ukiki/cage, Mips, Scuttlebug, Fly Guy, Moneybag, Tweester, Hoot, Bubba, Eyerok, Mr. I, Boo-in-castle, Unagi, Manta Ray, Sushi, Chirp Chirp, Skeeter, Blargg, Spiny, Swoop, Heave-Ho, Bookend | Not ported | `grep -i` of these names finds no behavior in `src/sim`, `native/sim` or the app layer (hits are only course-table text or the Zelda/experiment code). Not grepped, assumed absent: Lakitu enemy, Yoshi, Snowman wind, Big Chill Bully | none |
| LLL: fire bars, rotating hexagonal/octagonal platforms, sinking/volcano traps, drawbridge, rolling log | Not ported | terrain only | none |
| SSL: pyramid elevator/top/walls, quicksand-pit objects | Not ported | | none |
| WDW: changing water level (diamond/pillar), floating platforms, express elevator | Not ported | water level puzzle absent | none |
| TTC: rotating cubes/prisms, hexagon/triangle cogs, clock hand, 2D gears, pendulums, spinners, treadmills, push blocks, pit block, elevators, clock-speed setting | Ported (approximate) | all nine `ttc_*.inc.c` behaviors (64 placed objects) in `native/sim/src/solids/ttc.rs`: f64 for f32, decomp constants and action machines asserted in `solids/tests.rs`; `gTTCSpeedSetting` comes from the castle minute hand (`clock_arm.inc.c`, simulated as -0x180 roll/tick of castle time) or `JUMPMAN_TTC_SPEED`. Not ported: treadmill belt texture scroll (the surface moves Mario but the texture is static), sounds, TTC thwomp/Heave-Ho/fire spitters/amps/bob-ombs placed by macro (other owners) | `native/sim/src/solids/ttc.rs`, `scripts/export-sm64-solids.py` |
| RR: carpets (platform on track), cruiser wings, swinging platforms, rotating bridge, octagons, sliding platforms, seesaws, elevator, tricky triangles + purple switch, flamethrowers | Ported (approximate) | `solids/platforms.rs` (`platform_on_track` without the marker balls, the rotating bridge without the flames it spawns), `behaviors/flamethrower_sm64.rs` (INTERACT_FLAME is plain damage). Not ported: donut platform spawner/donut platforms, fire spitters, lakitu/fly guy/chuckya/amps | `native/sim/src/solids/platforms.rs` |
| THI: huge/tiny island top (size change) | Not ported | | none |
| DDD: submarine door/Bowser sub (rigged as colliders), whirlpool, pole, manta | Partial | sub is a moving collider only | `export-courses.py` |
| Warps, painting warps, fading/death warps, door warps, star doors | Ported (bit-exact) | differential-tested vs TS | `native/src/progress/` |
| Macro-object presets (`macro.inc.c`: coins, trees, signs, boxes, goombas) | Partial | the main presets are exported; full preset table coverage: unknown | `export-courses.py` |
| Behavior-script interpreter (`behavior_script.c`, 67 functions) | Out of scope | behaviors are hand-ported to Rust/TS, not interpreted | n/a |

### Levels / courses / areas

31 `DEFINE_LEVEL` entries. 26 exported with collision, placements, warps, coins and (where applicable) stars; `castle_inside` also has areas 2 and 3, and several courses have per-act variants (`ccm.2`, `ddd.2`, `jrb.2`).

| Course | Status | Note |
|--------|--------|------|
| BoB, WF, CCM (slide, penguin, snowman acts), JRB (ship, chests), PSS, Bowser 1-3 + obstacle courses, castle grounds/inside, Tower of the Wing Cap, Secret Aquarium | Ported (approximate) to Partial | star missions exist for BoB, WF, CCM, JRB; others have an authored star objective |
| BBH, HMC, LLL, SSL, DDD, SL, WDW, TTM, THI, TTC, RR | Partial | terrain, music and a subset of enemies; most course-specific gimmicks not ported (see behaviors) |
| BITDW, BITFS, BITS | Partial | red coins, the Bowser arenas and the platforms work: squarish path platforms, ferris wheels, sliding/octagon/seesaw platforms, activated back-and-forth platform, sinking/cage/squishable platforms, tilting pyramids, BitFS grate on track, collapsing staircases, flamethrowers (`solids/platforms.rs`). Not ported: BitFS tumbling bridge, the bully/goomba/amp/chuckya/fire-piranha/whomp objects of the courses |
| Cap levels: Cavern of the Metal Cap (`cotmc`), Wing Mario over the Rainbow (`wmotr`), Vanish Cap under the Moat (`vcutm`) | Not ported | caps are granted by `Y/U/I` and boxes; cap switches have no level |
| Castle courtyard (`castle_courtyard`) | Not ported | Boo/Mips area |
| Ending (`ending`) | Ported (approximate) | Peach cake cutscene, Toad, text and voices (`castleEnding.ts`, `native/src/progress/ending.rs`); credits scroll: unknown |
| Intro (`intro`), menu (`menu`) levels | Out of scope | the shell replaces them |

### Camera (`camera.c`: 15 `CAMERA_MODE`s, 48 `CUTSCENE_*`, 236 functions)

| Item | Status | Note | Lives in |
|------|--------|------|----------|
| Lakitu/radial/behind/close/C-up style modes | Ported (approximate) | custom orbit camera with presets (`lakitu`, `close`, `topdown`, `side`) | `src/game/CameraRig.ts`, `native/src/camera/rig.rs` |
| Water surface, slide, boss fight, inside-cannon, spiral stairs, fixed, 8-direction, parallel tracking, free roam, Hoot | Partial | water/slide/flight/interior contexts, summit/boss framing, cannon follow, Kokiri fixed rooms; the rest not ported | `CameraRig.ts`, `SummitCamera.ts`, `cameraWall.ts` |
| Mario-facing collision (wall push, transitions) | Ported (approximate) | `cameraWall.ts`, `CameraTransition.ts`, pitch shake | |
| The 48 camera cutscenes (door pull, star dance, dialog, ending, Bowser, Whomp, King Bob-omb, ...) | Partial | star dance, dialog, king, ending, cannon, painting enter implemented bespoke; most others not | `NpcDialogCamera.ts`, `kingDialog.ts`, `StarWarp.ts` |

### Menus, HUD, text, saves

| Item | Status | Note | Lives in |
|------|--------|------|----------|
| HUD (power meter, lives, coins, stars, camera icon) | Ported (approximate) | `hud.c` re-implemented (1/17 names) | `src/game/HUD.ts`, `native/src/hud.rs` |
| Pause menu, course select, settings, controls | Ported (approximate) | own 90s-themed shell | `index.html`, `native/src/shell/` |
| File select (`file_select.c`) | Ported (approximate) | 4 save slots with star counts; no Mario-hand UI, no erase/copy: unknown | `src/game/save.ts`, `native/src/progress/save.rs` |
| Star select / act selector | Ported (approximate) | | `ActSelector.ts`, `progress/menus.rs` |
| Course result / star dance / key and cap messages | Ported (approximate) | `CourseResult.ts`, `StarWarp.ts` | |
| Dialog boxes (`ingame_menu.c`, 72 functions) and signposts | Partial | signposts and NPC dialog with the US font; message table coverage: unknown | `signpost.ts`, `smfont.rs` |
| Title screen / intro Mario head / game over screen | Not ported | replaced by the shell title | n/a |
| Save format (EEPROM, checksums, flags) | Ported (approximate) | JSON slots; stars, coins, keys, caps; EEPROM bit layout not reproduced | `save.ts` |
| Debug level select, crash screen, profiler | Out of scope | | n/a |

### Audio

| Item | Status | Note | Lives in |
|------|--------|------|----------|
| Sequence player, mixer, instruments, reverb | Ported (bit-exact) | sm64-port compiled to wasm, 36 sequence ids available | `assets/audio/music/sm64-us.wasm`, `music-worklet.js`, `native/src/audio/engine.rs` |
| Music selection | Partial | per-course/area (30 entries in `courses.json`), star, victory, power-up, Bowser, ending; dynamic layers for JRB/DDD/HMC/WDW/BBH. Other seq triggers (e.g. all 36 ids, Koopa race, merry-go-round, slide): unknown | `music.ts`, `audio/sfx.rs` |
| Sound effects | Partial | 219 sample WAVs, 99 named sounds; SFX bank/ID dispatch is a name table, not the `external.c` bank/priority engine | `Sfx.ts`, `sfx.rs` |
| Sound priority/distance/doppler (`audio/external.c`) | Not ported | positional pan/volume approximated | |
| Per-character voices | Ported (approximate) | Mario native, Link/Banjo silent | `character-sounds.json` |

### Animation, rendering, assets

| Item | Status | Note |
|------|--------|------|
| Mario animation tables | Partial | 146 of 209 Mario anims mapped in `SM64_ANIM`; every source table preserved in `assets/animations.json` |
| Actor animations | Partial | compatible native tracks exported; per-object clip logic exists for the ported actors only |
| Actor/level geo layouts (`geo_layout.c`) | Ported (approximate) | 120 of 129 actor dirs exported to GLB; callbacks (water, shadows, skybox, face deform) stored as metadata, re-done in `fx/` |
| Water, fog, sky, shadows, cap/metal/vanish looks, particles | Ported (approximate) | `fx/`, `Particles.ts`; envfx snow/bubbles (`envfx_*.c`, 0/31 names) not ported: snow and bubble particles are separate |
| Moving textures (`moving_texture.c`: water/lava/quicksand/waterfall scroll) | Partial | water surfaces animate; other scrolling textures: unknown |
| Paintings (ripple, warp) | Ported (approximate) | `painting.ts` |
| Graphics microcode (RSP/RDP), Goddard (Mario face), debug | Out of scope | |

## Mario Kart 64 (reference/mk64; project files `src/game/experiments/kart*.ts`, `native/src/experiments/kart*.rs`)

### Player physics (`src/player_controller.c`, 91 functions, 25 name-matched)

| Function / feature | Status | Note |
|--------------------|--------|------|
| Speed-tier acceleration, thrust vs drag (`player_accelerate_alternative`, `player_decelerate_alternative`) | Ported (bit-exact) | shared golden trace TS/Rust (`kart_physics_trace.txt`) |
| Stepped steering (`update_steering_large`/`small`, `func_80033AE0`), yaw rate, sideways grip | Ported (bit-exact) | |
| Hop and drift (`kart_hop`, `update_drift_state_counter`, `update_player_drift_duration`, `cancel_drift_effect`), mini-turbo | Ported (bit-exact) | |
| Floor reaction and bounce (`func_8003F46C`/`func_8003E9EC`), wall response (`func_8003F734`/`func_8003FBAC`, `func_8002C954`), slope accel (`func_80029B4C`), tyre heights | Ported (bit-exact) | Mario at 150cc only |
| Kart attributes tables (`kart_attributes.c`) | Partial | Mario only (1 of 8 characters), 150cc only |
| Off-road (grass) thrust loss | Ported (bit-exact) | grass/asphalt only |
| Other tyre surfaces (sand, snow, dirt, wood, water, boost pad) | Not ported | only grass vs asphalt is classified |
| Boost ramps, boost pads | Not ported | |
| Spinouts and `apply_effect` (hit by banana/shell/bomb, lightning shrink, star, boo, squish), tumble/terrain tumble, AB-spin, triple-A/B combos (`detect_triple_*`) | Not ported | race items use a thin custom spinout in `kartRace.ts`, not `apply_effect` |
| Lakitu retrieval (`func_80029200`... out-of-bounds respawn) | Ported (approximate) | graceful rescue to nearest path node (`kartRescue.ts`) |
| Start-line boost, `player_accelerate_during_start_sequence` | Not ported | |
| Kart-to-kart collision (`func_80036DB4`... family), item/actor collision (`racing/collision.c`, 40 functions) | Not ported | karts pass through each other |
| CPU control (`control_cpu_movement`, `apply_cpu_turn`) | Not ported | CPUs use the same physics with a custom path follower |
| Walking out of the kart | Out of scope | Jumpman addition on Mario's runtime |

### Camera (`src/camera.c`, 15 functions, 4 matched)

| Feature | Status | Note |
|---------|--------|------|
| 1P chase (`camera_init`, `func_8001E45C` heading follow + drift swing, `func_8001CCEC` smoothing, mushroom push-in, wall-bump kick, FOV `func_8001F394`/`func_80014EE4`) | Ported (bit-exact) | numeric tests in `kartCamera.test.ts` |
| Spinout/zoom variants, other camera ids, split-screen, Lakitu reverse look, finish cutscene, course intro flyover | Not ported | |

### Items (`code_80057C60.c`, `update_objects.c`; 15 items)

| Item | Status | Note |
|------|--------|------|
| Mushroom, banana, green shell | Ported (approximate) | custom rules in `kartRace.ts`: mushroom uses the mini-turbo thrust, one wall bounce for the shell; roulette weighting custom |
| Triple mushroom, double mushroom, super mushroom, banana bunch, triple green, red shells, triple red, blue spiny shell, thunderbolt, fake item box, star, boo | Not ported | 12 of 15 |
| Item boxes | Ported (approximate) | placed along the lap procedurally, not from the decomp's `D_` object tables |
| Item roulette (`func_8007...` roulette and weights table) | Ported (approximate) | not the decomp probability tables |

### Courses and objects

| Course | Status |
|--------|--------|
| Luigi Raceway | Ported (approximate): vertices, 40 textures, display-list groups and the path are real; no props, no course objects, no skybox, no signs animation. Kart surface types: grass/asphalt only; the walkable course uses the real `TrackSections` collision |
| Mario Raceway, Koopa Troopa Beach, Kalimari Desert, Toad's Turnpike, Frappe Snowland, Choco Mountain, Moo Moo Farm, Yoshi Valley, Banshee Boardwalk, Sherbet Land, Royal Raceway, Bowser's Castle, DK's Jungle Parkway, Wario Stadium, Rainbow Road | Ported (approximate) (15): all vertices, textures (RGBA16/IA16 with wrap modes), display-list order, `TrackSections` collision with MK64 surface types mapped to SM64 kinds, section wall/floor flags, vertex no-collision flag, lap path and item boxes. Walkable with every hero (`mk64_<dir>` in LEVELS) and drivable (kart track picker, `V`). Water alpha lists are blended; no props, no objects, hand-picked sky/fog, minimap drawn from the path |
| Battle: Big Donut, Block Fort, Double Deck, Skyscraper | Ported as walkable levels only (4): geometry, textures, collision; no battle mode, no balloons, no kart play (no lap path) |
| Course objects/animations (`update_objects.c`, 381 functions; `racing/actors.c`, 57; 44 actor types): trees, cows, piranha plants, crabs, bats, Thwomps, Chain Chomps, trains, cars/trucks/buses, boats, Yoshi eggs, penguins, Mr. Blizzard-style props, water, fire, signs | Not ported (0) |
| Luigi Raceway's balloons/blimp, Lakitu start/lap signal | Not ported |

### Race logic, modes, menus, audio, characters

| Item | Status | Note |
|------|--------|------|
| 3-2-1-GO, laps, place, finish card, best lap/total saves | Ported (approximate) | `kart_race.rs`; the decomp's `race_logic.c` (33 functions, 1 matched) is not followed: checkpoints are 24 gates cut from the path |
| CPU AI (`cpu_vehicles_camera_path.c`: path tables, rubber band, item use, 79 functions) | Not ported (custom) | |
| Grand Prix, cups, points, Time Trials + staff ghosts (`staff_ghost_data.c`), VS, Battle | Not ported | Jumpman's own ghost system (daily runner) is unrelated |
| Menus (`menus.c`, `menu_items.c`, 326 functions), character select, course select, options | Not ported | |
| Replays, save (`replays.c`, `save.c`) | Not ported | |
| Music (30 sequences in `seq_ids.h`, 29 music + the SFX player) | Ported (engine exact, mixer re-implemented) | the decomp's own `src/audio/*.c` (sequence player, instruments, envelopes, reverb, `external.c` game-side music logic) compiled to wasm with a portable RSP mixer (sm64-port `mixer.c`, SM64 EU/MK64 microcode family) and the ROM's own sequences / banks / VADPCM samples converted to little endian at import: `scripts/render-music-mk64.py`, `scripts/export-mk64-music.py`, `native/src/mk64_music/`. Used: the 20 course themes (`play_music_for_current_track` table: 11 distinct sequences), the pre-race fanfare, the final-lap jingle + 25 % faster theme, finish fanfares (win / other / lose) then the results theme, star and lightning handling (engine side; the kart mode has no such items). Not used: title/menu (1-2), ceremony (20, 26-29), VS fanfares (22-24); no boo sequence exists in MK64. Not verified by ear |
| SFX (engine, items, voices) | Not ported | the SFX banks are in the same library (sequence 0 loads and runs) but no game event requests them; engine-free synth stand-ins in `assets/audio/kart-voices.json` |
| Characters (8 + kart sprites) | Partial | Mario plus CPU Luigi/Peach/Toad drive; all 8 racers' 321 sprite frames are exported (`<racer>-kart-frames.png`) and `kart_sprite.rs` follows `func_8002934C` / `func_80029200` / `kart_dma.c` frame selection (slope group, view angle with slip, mirroring, spin-out table); tumble frames exported, not used |
| Effects (`effects.c`: sparks, smoke, exhaust, 110 functions) | Ported (approximate) | `kart_puffs.rs` follows `code_80057C60.c`: exhaust (`func_80060504`, `func_80062C74`), drift smoke (`set_drift_particles`, `func_80063408`), tyre grass (`setup_tyre_particles`, `func_80063BD4`), mushroom sparks (`func_800621BC`, `func_80064C74`), 30 Hz ring pools; `kart_fx.rs` keeps spin-out stars and box shards. Not ported: dirt/sand/snow dust per surface, star/boo/lightning/crash/explosion particles, boost-ramp fire, water splash, the translucent boost afterimage (`func_80025DE8`) |
| Kart shadow (`render_player_shadow`) | Ported | `kart_sprite_view.rs`: ROM `gTextureKartShadow` on a floor-aligned quad, 1 unit lift, 2 units towards the camera, shrinks with height, gone beyond 50 units; no ice exception |
| HUD (position, lap, item window) | Ported (approximate) | |

## Ocarina of Time (reference/oot; US 1.2 ROM)

| Area | Decomp scope | Status | Note / lives in |
|------|--------------|--------|-----------------|
| Scenes | 110 | Partial | 27 exported (Kokiri Forest, Link's House, Kokiri Shop, 4 Kokiri houses, Lon Lon Ranch, Deku Tree + Gohma lair, Hyrule Field, Kakariko, Graveyard, Lost Woods, Sacred Forest Meadow, Zora's River/Domain/Fountain, Lake Hylia, Gerudo Valley/Fortress, Haunted Wasteland, Desert Colossus, Hyrule Castle grounds, Death Mountain Trail/Crater, Goron City); 17 of them terrain/collision-only (`explorationOnly`). Not exported: Temple of Time, Market, Castle courtyard, Ganon's, Zora/Goron/Gerudo interiors, 8 other dungeons, shops, grottos, etc. (`scripts/export-oot-courses.py`) |
| Link movement (`z_player.c`, 89 `Player_Action_*`) | 89 | Partial | free movement, roll, sword slashes by stick direction + combo finisher + stab + charged spin attack + jump/flip slash, hammer, shield guard, Z-focus (strafe, side hop, backflip, forward jump), ledge grab and swim via Mario's runtime, bow/slingshot/hookshot aiming, item throw/cast states, ocarina, hookshot pull (`sim/src/link/moves.rs`); not ported: ladders, crawl-space, shield-crouch, rolling-attack variants, climb, Zora dive, ~60 cutscene/talk actions. `src/sim/linkController.ts` (106 lines), `native/sim/src/characters.rs` |
| Epona (`z_en_horse.c`, `object_horse`) | ~1 actor | Partial | `native/sim/src/link/epona.rs` ports the rider-controlled and follow-player paths: gaits, carrots, low / high jump, braking, reverse, obstruction, Epona's Song summon; adult Link mounts and rides (`epona_rider.rs`); skinned model, 9 animations and Link's `uma_*` clips (`scripts/export-oot-epona.py`), 8 horse sound effects. Not ported: Ingo / races / horseback archery, cutscene and bridge-jump paths, other horse variants (`ovl_En_Horse_*`), weapons from horseback |
| Link model/animations | | Ported (approximate) | adult + young Link, 21 limbs, 57 ROM clips (`scripts/export-oot-link.py`, `EXTRA_CLIPS`: swings, spin, jump slash, bow/hook/boomerang, bombs, spells, ocarina, bottle, landing); jump clip is now frame-synced to the arc |
| Z-targeting / `z_camera.c` | | Partial | `LinkFocus.ts` target choice (toggle or hold) plus `native/src/link/zcam.rs`: Battle1 / Parallel1 placement and fov from `z_camera_data.inc.c` `sSetNormal0Mode*`, eased in/out; the other ~25 settings x modes and the swing-yaw interpolation of `Camera_Battle1` are not ported |
| Items | ~70 | Partial | Presentation (this pass): real item models exported from the ROM (`scripts/export-oot-items.py`: bomb body + cap with the `EnBom` flash / fuse sparks, bombchu, boomerang, arrow, hookshot tip + chain, Deku nut / stick; held bow / slingshot / hookshot / boomerang / hammer hand display lists), first-person aim camera (`CAM_MODE_AIM_ADULT` / `_CHILD`, `Camera_Subj3` numbers) and reticle, combat effects (hit marks, `EffectSsBomb2` fireball + shockwave + quake, blade trail, enemy colour filters, hit-stop). Native Link controller (`native/LINK_CONTROLLER.md`, `native/sim/src/link/`, `native/src/link/`): inventory + C-slots + age rules + item screen, Slingshot, Bow + Fire/Ice/Light arrows, Boomerang, Hookshot/Longshot, Bombs, Bombchu, Deku Nut/Stick, Megaton Hammer, Din's Fire, Farore's Wind, Nayru's Love, Lens (toggle), Magic Beans (plant only), bottles, Ocarina with all 12 songs, magic meter, hearts, Iron/Hover Boots, Zora Tunic (no drowning); partial: Epona, masks, trade items, Goron Tunic effects, medallions, collectibles, Skulltula tokens. Kokiri Sword quest / Deku Shield shop / chest slingshot remain in the village scripts |
| Actors | 427 overlays | Partial (about 28) | Enemies placed from the real scene/room actor lists in 11 courses (Kokiri Forest, Deku Tree, Hyrule Field, Lost Woods, Sacred Forest Meadow, Zora's River, Lake Hylia, Gerudo Valley, Haunted Wasteland, Desert Colossus, Death Mountain Trail; `scripts/export-oot-enemies.py`, `native/sim/src/oot_enemy/`, `native/src/zelda_enemies/`): `En_Dekubaba`, `En_Karebaba`, `En_Goma` (larva), `En_St`, `En_Sw`, `En_Peehat`, `En_Tite`, `En_Okuta`, `En_Dekunuts`, `En_Wf`, `En_Crow`, `En_Reeba` + `En_Encount1` (Leever spawner): 78 placed enemies + 2 Leever spawners with the real hit points, `DamageTable`s and drop tables, simplified AI, every playable character can fight them. The same hit points / touch damage / models are reused as content of the Relic Descent, Gem Clash, Star Swarm and Hold the Gate modes (`native/src/experiments/foes.rs`): no new decomp coverage, just a second use of the existing tables. Earlier actors: Kokiri children/Saria/Mido/shopkeeper/signs etc. (below). Not ported enemy overlays: ~45 (Keese `En_Firefly`, Stalfos `En_Test`, Like Like `En_Rr`, Redead/Gibdo `En_Rd`, Moblin `En_Mb`, Armos `En_Am`, Dodongo, Lizalfos `En_Zf`, Beamos `En_Vm`, Big Octo, ...; none is placed in the exported courses) and the night spawners (Stalchildren/Wolfos). Kokiri children/Saria/Mido/shopkeeper/signs, chests, Deku Scrub-style shooter, Gohma, push block, torches, nut/flame, Navi (model only), Deku Baba/other dungeon props as scripted routes: `src/game/dekuDungeon.ts`, `native/src/zelda/` |
| Dungeons | 11+ | Partial | Deku Tree (lobby ladder/web, push block, burning stick, scrub nut, water room, brothers puzzle, boss door) as scripted traversal routes, not the decomp's room scripts. Others: Not ported |
| Bosses | 11 `Boss_*` | Partial | Gohma 1/11 (`gohma.ts`, `zelda/gohma.rs`); Not ported: Dodongo, Barinade, Phantom Ganon, Volvagia, Morpha, Bongo Bongo, Twinrova, Ganondorf, Ganon |
| Text (`z_message.c`) | | Ported (approximate) | US font/background/choices (`ootTextbox.ts`, `zelda/textbox.rs`); message table coverage beyond the Kokiri set: unknown |
| Audio (sequences, SFX, voices) | | Not ported | web build plays no Zelda sounds beyond the coin cue (native README) |
| Cutscenes (`z_demo`, scene cutscene scripts) | | Not ported | the Deku Tree meetings are bespoke |
| Time of day, skyboxes, collectibles (Skulltulas, hearts pieces, heart containers), Epona, minigames, save system | | Not ported / unknown | Zelda save flags live in `zelda-slotN.json` |

## Banjo-Kazooie (reference/banjo-kazooie; US 1.0 ROM)

| Area | Decomp scope | Status | Note / lives in |
|------|--------------|--------|-----------------|
| Banjo moveset (`core2/bs/*.c`, 49 files / 157 `BS_*` states) | 49 | Partial | 20 moves from 15 state files: jump, Feathery Flap, Flap Flip, Claw Swipe, Forward Roll, Rat-a-tat Rap, Beak Barge (`sim/src/characters.rs`), plus Beak Buster (`bbuster`, real dive/hop numbers), Talon Trot and its jump (`bTrot`), Shock Spring Jump (`bShock`), blue eggs forward/back (`bEggHead`, `bEggAss`, `fxegg_*`), Wonderwing (`bWhirl`), flight and Beak Bomb (`bFly`), Turbo Trainers timer (`bTrot`), Wading Boots hop (`bLongLeg`, partial) in `sim/src/banjo_ext.rs`. Status table: `native/BANJO_CONTROLLER.md` |
| Not ported moves | | Not ported | swimming/diving (`bSwim` + BK air meter; SM64 swim is used), BK climb states (`climb`), sled/surf/slide variants, the 12 transformations (`croc`, `walrus`, `pumpkin`, `bee`, ...), `splat`, `talk`, `jig`, drones/cutscene locks |
| Model + animation | | Ported (approximate) | 63 bones, 37 keyframe clips via the decomp's Catmull-Rom sampler (`scripts/export-bk-banjo.py`, `BanjoView.ts`, `native/src/characters/banjo_pose.rs`) |
| Worlds / levels | 10 worlds + lair | Terrain + setup placements + warps | 104 of 128 maps (24 cutscene/intro/ending/file-select maps skipped): the 14 world terrains (SM, MM, TTC, CC, BGS, GV, FP, RBB, MMM, CCW hub + 4 seasons) and 90 sub-areas (21 lair rooms, ship/shell/sandcastle, Clanker's insides, Mumbo's skulls, pyramids, mansion rooms, ship rooms, Click Clock Wood houses/hives per season, Banjo's house...): `OPA`+`XLU` level models to glTF (88 files, 14.4 MB, 150k triangles), collision triangles, water boxes, sky/fog (`scripts/export-bk-levels.py`, `scripts/bk-maps.py`, `scripts/bk-catalogue.py`; `bk_*` courses, `src/game/bk_tests.rs`). The map **setup files** are parsed (`scripts/export-bk-setup.py`, `NodeProp`/`Prop` cubes of `gccube.c`/`code_A5BC0.c`): enemy placements, notes/eggs/feathers/Jiggies per map, category-3 **warp** nodes -> `sWarpFunctions` handlers (`code_AD110.c`, `warp_*` in `code_956B0.c`) -> target map + exit id, exit nodes (`nodeprop_getExitActorId`) = arrival pose; 361 warps, 100 maps reachable from Spiral Mountain, no one-way connections (`src/game/bk_warp_tests.rs`). Replaced by hand: world exit = pause-menu "exit to lair" (`D_8036C560`) as a return pad, actor-triggered entrances (Clanker, Mr. Vile, rain barrel) as mirror pads. Not ported: door/switch/NPC actors and note/Jiggy door locks, 4 script-only maps unreachable by warps (Furnace Fun, Dingpot, battlements, Sharkfood Island), cameras, surface-type mapping, animated textures, music |
| Enemies / NPCs (`core2/ch/`, 53 files + per-world `ch/`) | 53 + per-world `ch/` | Partial | 12 species from 10 source files, 127 placements over the 14 worlds (Grublin 5, Grublin Hood 17, Limbo 6, Ticker 4, Bigbutt 4, Snippet 6, Mutie Snippet 4, Sir Slush 11, Bat 6, Shrapnel 4, Snare-bear 25, Beehive 35): `sim/src/bk_enemy.rs`, `src/bk_enemies/`, models `scripts/export-bk-enemies.py`. All four characters can hit them and are hurt by them; 9 of the species (and their decomp hit points / damage) also appear as monsters in the special game modes (`native/src/experiments/foes.rs`). Skipped with reasons (listed in `bk_setup_report.json`): Buzzbomb, Flibbit, Clucker, Caterpillar, Grille Chompa, Grimlet, Flotsam, Scarab Beetle (skeletal-animation models), Nipper, Slappa, Tanktup and the other bosses, minigame props (Croctus, Twinkly, clam), Tee-Hee, Spiral Mountain's tutorial veggies, Snacker (spawned by script). Behaviors are simplified: no enemy-boundary (category 7) leash volumes, no sound |
| Collectibles (notes, Jiggies, Mumbo tokens, honeycombs, eggs, feathers) and abilities progression (`abilityprogress.c`) | | Partial | notes, eggs, red/gold feathers, honeycombs and Jiggies are pickups with counters and a HUD (`sim/src/banjo_ext.rs`, `src/banjo_ctl.rs`); in the 14 BK worlds they sit at their **real setup positions** (653 notes, 334 red and 64 gold feathers, 312 eggs, 25 Jiggies, 12 empty honeycombs over the main maps; streamed by `src/bk_enemies/pickups.rs`); elsewhere the sandbox scatter remains. No Mumbo tokens (75 placed), no ability unlock progression (everything starts unlocked) |
| Audio (music, SFX, voices) | | Not ported | not extracted; Banjo is silent (`character-sounds.json`) |
| Cutscenes, Gruntilda fight, menus, save | | Not ported / Out of scope | |

## Other references under `reference/`

| Item | Status | Note |
|------|--------|------|
| `reference/sm64` (vendored n64decomp/sm64, US) | primary source | assets extracted locally and gitignored |
| `reference/mk64`, `reference/oot`, `reference/banjo-kazooie` | git submodules | empty in many worktrees: `git submodule update --init reference/<name>` before the exporters or this script's decomp counts will work (the script prints "not checked out" otherwise) |
| `reference/roms/{mk64-us-1.0,oot-us-1.2,bk-us-1.0}.z64` | tracked ROMs | read in place by the exporters; checksum-verified |
| `reference/c2obj.py` | helper | converts decomp C collision to OBJ; no other game material exists |
| sm64-port (outside the repo) | build input only | used once to compile the SM64 audio wasm (`scripts/render-music.py`, commit `2b17d08`) and again for the MK64 one (`scripts/render-music-mk64.py`: only its `src/pc/mixer.c`) |

## Known stale statements found while building this tally

* `native/PORTING.md` / `AUDIT.md` said "all 192 `ACT_*` ids": 31 of the 192 are flag/group constants; the decomp has 233 actions and 161 are defined (fixed in PORTING.md).
* README "What is not ported" predates several items now done (death/drowning, quicksand, animation clips, Zelda content); treat this file as the source of truth for gaps and the README as history.

## How to update this tally

1. When a port lands, change only the affected row's **Status** and **Note** (rows are stable on purpose, so diffs stay small). Do not reorder rows or rewrite whole paragraphs.
2. Re-run `node scripts/decomp-tally-check.mjs` (read-only, no dependencies). It prints: SM64 action counts by group and the missing list, function-name overlap per decomp file, behavior and level counts, per-level placements, animation/audio/surface/actor counts, the MK64/OoT/BK equivalents (course/scene/state/function counts, items, actor overlays). Paste changed numbers into the matching rows and the Summary table. `--json` prints the same data for diffing (`node scripts/decomp-tally-check.mjs --json > /tmp/new.json`).
3. The MK64, OoT and BK counts need those submodules populated (`git submodule update --init reference/mk64 reference/oot reference/banjo-kazooie`). From a worktree without them, point at a checkout: `MK64_DIR=… OOT_DIR=… BK_DIR=… SM64_DIR=… node scripts/decomp-tally-check.mjs`. When a decomp is absent the script says so and the old numbers stay.
4. Judgement items (marked "judgement" in the Summary and a few "unknown"s) are not computed by the script; revisit them by hand and replace "unknown" with a measurement when someone checks.
5. Update the Summary percentages and the date/commit line at the top, keep the "Biggest gaps" list ranked by gameplay impact, and mention the change in the same commit as the port.
6. Caveats: function-name overlap is an upper bound for "ported" (comments count) and a lower bound for renamed or table-driven ports; "explicitly named by the exporter" is a proxy for behavior coverage, not proof of fidelity.
