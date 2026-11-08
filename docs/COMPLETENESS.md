# Jumpman completeness audit

> **Historical:** written against the removed Three.js browser build (pnpm/Vite, `?level=` URLs, dev-server scripts). The game is now the Rust/Bevy app in `native/`; see the root [README](../README.md) for current build and run instructions.

Updated November 6, 2026. This audit distinguishes **working and verified** behavior from
**prototype** and **missing** behavior. It is intentionally conservative: a feature is only
listed as complete when a unit or browser smoke exercises it.

The overall objective is **not complete**: the verified Bob-omb Battlefield slice and four
experiment foundations are substantial, but full SM64 course coverage, remaining sound-event wiring,
most remaining object behaviors and exact camera scripts are still open.

## Verified foundation

| Area | Evidence |
| --- | --- |
| Grand Star takeoff and flight, castle grounds arrival and voiced Peach ending | `scripts/keys-smoke.mjs` |
| Bowser tail grabs, spin throws, arena mines, Grand Star, basement/upstairs keys and persistent door unlocks | `tests/bowser.test.ts`, `tests/save.test.ts`, `scripts/keys-smoke.mjs` |
| Castle room collision, star gates, first unlock and door animation travel | `tests/roomDoors.test.ts` |
| Hazy Maze Cave star IDs, red-coin rewards and 100-coin collection during another mission | key and mission browser smokes |
| Shifting Sand Land Klepto patrol and Pokeys | `tests/klepto.test.ts`, `tests/pokey.test.ts`, course smoke |
| Source mission names for every native course | `tests/missions.test.ts` |
| Exporter/deploy validation skips meshless decor | `scripts/check-deploy-assets.mjs` |
| Native course terrain: Bob, Castle Grounds/Lobby, Whomp's Fortress, Big Boo's Haunt, Cool Cool Mountain, Jolly Roger Bay, Lethal Lava Land, Shifting Sand Land, Wet-Dry World, Snowman's Land, Tall Tall Mountain, Tiny-Huge Island, Tick Tock Clock, Rainbow Ride, Bowser in the Dark World, Bowser in the Fire Sea, Bowser in the Sky, Bowser Arenas 1–3, PSS, Playground | course smoke, native collision tests |
| Whomp's Fortress Whomps/Whomp King, event-gated star and 1-Up | `tests/whomp.test.ts`, `tests/oneUp.test.ts`, course smoke |
| Big Boo's Haunt Boos, Big Boo boss, flames, crates and moving platforms | `tests/boo.test.ts`, `tests/bigBoo.test.ts`, `tests/flame.test.ts`, course smoke |
| Lethal Lava Land terrain/lava, Bullies and Big Bully boss | `tests/bully.test.ts`, `tests/bigBully.test.ts`, course smoke |
| Cool, Cool Mountain painting entry, indoor slide, cabin exit, first star and castle return | `scripts/mountain-slide-smoke.mjs` |
| Snowman body path, checkpoint guidance, reunion reward, miss and retry, saved snowman | `scripts/snowman-smoke.mjs` |
| Big Penguin Race, main course victory, decline, loss, shortcut and airborne disqualification | `scripts/penguin-race-smoke.mjs` |
| Li’l Penguin Lost, decoy rejection, carrying on snow slopes and the reunion star | `scripts/penguin-smoke.mjs` |
| Cool, Cool Mountain terrain, penguins and Mr. Blizzard | `tests/penguin.test.ts`, `tests/mrBlizzard.test.ts`, course smoke |
| 30 Hz Mario runtime, input mapping, ground/air steps | movement/action Vitest suites, native browser smoke |
| Actions: jumps, dives, ground pound, ledge/ceiling, poles, holding, punches, knockback, death/drowning/lava/quicksand | `tests/actions.test.ts`, `tests/health.test.ts`, browser smokes |
| Bob island box, flying ring secrets, three-pound gate and legacy star migration | `scripts/missions-smoke.mjs`, `tests/save.test.ts` |
| Bob-omb Battlefield object layer | Bob-ombs/crates/Chain Chomp/stakes/gate/switch/cannon/Koopa/King | `tests/objects.test.ts`, `scripts/object-smoke.mjs` |
| King Bob-omb grab/throw and intro dialogue | `tests/kingGrab.test.ts`, king-grab smoke |
| Bob mission acts 1–4 spawns, red coins, star locations and per-act persistence | `tests/missions.test.ts`, `scripts/missions-smoke.mjs` |
| Save slots, per-act stars, best coins, active slot | `tests/save.test.ts`, object/mission smokes |
| Star collection, win overlay and castle return | `scripts/sm64-smoke.mjs` |
| Koopa race countdown/timer HUD | `scripts/object-smoke.mjs` |
| Castle grounds start, front doors, BoB/CCM/WF/JRB paintings, star return nodes and PSS entrance | painting, mission and warp browser smokes, native collision tests |
| Dire, Dire Docks area transfers, submarine star and basement entrance relocation | `scripts/docks-smoke.mjs` |
| Castle paintings and floor-selected room geometry | `scripts/painting-smoke.mjs`, rendered BoB room inspection |
| Bowser obstacle course entrances to all three arenas and their landing platforms | warp browser smoke |
| Quantized sine, cosine and arctangent lookup tables | exhaustive comparison against the decomp in `tests/math.test.ts` |
| Camera modes/contexts (normal, boss, water, slide, interior, flight) | `tests/cameraModes.test.ts`, dimensions/warp/wing smokes |
| Audio samples/sequences, US sequence interpreter and portable RSP mixer and mute preference | `tests/audioAssets.test.mjs`, `tests/music.test.ts`, sm64 smoke |
| DDD current and JRB cave chest rewards, sunken ship entry, drain and star box | `tests/chest.test.ts`, `scripts/chests-smoke.mjs` |
| Water surfaces, splash/fog, metal-water walking | `tests/waterSurfaces.test.ts`, water/metal-water smokes |
| Wing/Metal/Vanish cap prototypes, enemy aggro suppression and Battlefield cannon unlock, aim and island landing | `tests/wing.test.ts`, caps/vanish/cannon smokes |
| Koopa shell riding and jump-off | `tests/shellRide.test.ts`, shell smoke |
| Shared experiment framework (`src/game/experiments/`), launcher, URL persistence, clean exit | `scripts/experiments-smoke.mjs` |
| Across Dimensions: dolly-zoom flatten to a lane-locked 2D cross-section (any lane direction), a sliced top-down view, and a 2.5D path up the Battlefield | `tests/dimensions.test.ts`, experiments smoke |
| Mario vs. God: grab/drag/resize/delete pieces and course actors, bridge painting at Mario's height, blocks/ramps/springs, gravity, slow-mo, undo | experiments smoke (bridge catches a falling Mario) |
| Endless Run: seeded auto-run track, own collision world, themes, coins, game over and restart | `tests/runnerGen.test.ts` (reachability vs. measured jump), experiments smoke |
| A Hundred Marios: 100 animated full-runtime Marios, five personalities, Mario-walkable race route, brawling, inspector/watch cam | `tests/crowdRoute.test.ts`, experiments smoke |
| Pause menu character swap and main menu from anywhere | experiments smoke |
| Summit Race, Shrink Ray, Tiny Planet, God Mode portals | experiments smoke, `tests/railPath.test.ts` |
| Diorama: course in a compact case under a sky dome, tilt-shift lens, free-fly camera, real Marios and Goombas, pokes | experiments smoke |
| Authored Mushroom Kingdom 1-1 side level (opens in the 2D experiment) | `scripts/mushroom2d-smoke.mjs` |

## Prototype (works, but not source-complete)

- **Experiments** — each is a self-contained toy; see [the experiments brief](experiments/BRIEF.md) for known limits.
- **Caps and cannons** — The Battlefield operator unlocks the course cannons per save slot. A browser test verifies the aimed ballistic shot from the hillside to the floating island. The operator uses a HUD message. His original dialogue and camera cutscene remain open, along with Metal and Vanish switch progression. The ten-star lobby warp, tower red switch, persistent red boxes, and Wing Cap pickup are exercised by `scripts/wing-switch-smoke.mjs`.
- **Audio** — The game runs the US sequencer and instrument banks in an AudioWorklet. Battlefield playback has also been compared with a separate soundtrack excerpt. Object sound coverage remains incomplete.
- **Camera** — orbit camera with Lakitu contexts, collision, auto-follow and speed
  look-ahead; `camera.c` regions/scripted shots are missing.

## Missing

- Full SM64 course coverage (the exported areas load; Bowser attack patterns, arena transformations, remaining ending effects and the credits tour remain incomplete; JRB has cave and ship chest puzzles; BBH has terrain, Boos, a Big Boo event-gated star, flames, crates and four moving platforms; CCM has penguins/Mr. Blizzard; LLL has Bullies/Big Bully; SSL/WDW/TTM/THI/TTC/RR are terrain-only; SL has Mr. Blizzard; most objects remain) and painting destinations.
- Mission-specific coin totals and exact source mission conditions beyond the boss/race/gate/red-coin events.
- King Bob-omb full cutscene, Koopa source path collision and remaining object behaviors.
- Poison gas placement in a playable area, remaining particle/environmental effects and source-timed sequences.
- Metal/Vanish cap full behavior and remaining special mechanics.
- Exact source save-file layout and all castle-area scripts.

## Next milestones

1. Full SM64 course coverage and remaining painting destinations; exact mission coin totals.
2. Wire the remaining scene-specific audio events and replace approximated movement effects.
3. Exact `camera.c` regions, scripted shots and Lakitu behavior.
4. Remaining object behaviors from `reference/sm64/src/game/behaviors/`.
5. Full-Mario-agent simulation for A Hundred Marios and final experiment polish.

## Verification snapshot (round 100)

- `tsc --noEmit`: clean.
- Vitest: **161 passed, 1 skipped**.
- `node scripts/check-deploy-assets.mjs`: exported native courses and castle areas, shared actors, Bowser/flame actors, and converted audio present; every course object model has a mesh.
- Production Vite build: clean.
- Browser smokes passing: native `sm64`, all courses (including all Bowser arenas and Bowser fire breath), objects, missions, and the experiments smoke (all five experiments, with 100 animated full-physics Marios).
- Live Worker returns the intentional `401` password gate.

The objective is still **not complete**: mission-specific object coverage, source-exact camera regions/scripts, full instrument-accurate audio, and several special mechanics remain as documented above.
