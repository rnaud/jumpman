# Jumpman completeness audit

Updated November 5, 2026. This audit distinguishes **working and verified** behavior from
**prototype** and **missing** behavior. It is intentionally conservative: a feature is only
listed as complete when a unit or browser smoke exercises it.

The overall objective is **not complete**: the verified Bob-omb Battlefield slice and four
experiment foundations are substantial, but full SM64 course coverage, source music sequencing,
most remaining object behaviors and exact camera scripts are still open.

## Verified foundation

| Area | Evidence |
| --- | --- |
| Bowser arena boss, fire breath and event-gated stars | `tests/bowser.test.ts`, course smoke |
| Shifting Sand Land Klepto patrol and Pokeys | `tests/klepto.test.ts`, `tests/pokey.test.ts`, course smoke |
| Source mission names for every native course | `tests/missions.test.ts` |
| Exporter/deploy validation skips meshless decor | `scripts/check-deploy-assets.mjs` |
| Native course terrain: Bob, Castle Grounds/Lobby, Whomp's Fortress, Big Boo's Haunt, Cool Cool Mountain, Jolly Roger Bay, Lethal Lava Land, Shifting Sand Land, Wet-Dry World, Snowman's Land, Tall Tall Mountain, Tiny-Huge Island, Tick Tock Clock, Rainbow Ride, Bowser in the Dark World, Bowser in the Fire Sea, Bowser in the Sky, Bowser Arenas 1–3, PSS, Playground | course smoke, native collision tests |
| Whomp's Fortress Whomps/Whomp King, event-gated star and 1-Up | `tests/whomp.test.ts`, `tests/oneUp.test.ts`, course smoke |
| Big Boo's Haunt Boos, Big Boo boss, flames, crates and moving platforms | `tests/boo.test.ts`, `tests/bigBoo.test.ts`, `tests/flame.test.ts`, course smoke |
| Lethal Lava Land terrain/lava, Bullies and Big Bully boss | `tests/bully.test.ts`, `tests/bigBully.test.ts`, course smoke |
| Cool, Cool Mountain terrain, penguins and Mr. Blizzard | `tests/penguin.test.ts`, `tests/mrBlizzard.test.ts`, course smoke |
| 30 Hz Mario runtime, input mapping, ground/air steps | movement/action Vitest suites, native browser smoke |
| Actions: jumps, dives, ground pound, ledge/ceiling, poles, holding, punches, knockback, death/drowning/lava/quicksand | `tests/actions.test.ts`, `tests/health.test.ts`, browser smokes |
| Bob-omb Battlefield object layer | Bob-ombs/crates/Chain Chomp/stakes/gate/switch/cannon/Koopa/King | `tests/objects.test.ts`, `scripts/object-smoke.mjs` |
| King Bob-omb grab/throw and intro dialogue | `tests/kingGrab.test.ts`, king-grab smoke |
| Bob mission acts 1–4 spawns, red coins, star locations and per-act persistence | `tests/missions.test.ts`, `scripts/missions-smoke.mjs` |
| Save slots, per-act stars, best coins, active slot | `tests/save.test.ts`, object/mission smokes |
| Star collection, win overlay and castle return | `scripts/sm64-smoke.mjs` |
| Koopa race countdown/timer HUD | `scripts/object-smoke.mjs` |
| Painting/castle/PSS warps (BoB, WF, PSS) | painting/warp smokes, painting tests |
| Camera modes/contexts (normal, boss, water, slide, interior, flight) | `tests/cameraModes.test.ts`, dimensions/warp/wing smokes |
| Audio samples/sequences, rendered source-instrument music stems and mute preference | `tests/audioAssets.test.mjs`, `tests/music.test.ts`, sm64 smoke |
| JRB four-chest puzzle and event-gated star | `tests/chest.test.ts`, course smoke |
| Water surfaces, splash/fog, metal-water walking | `tests/waterSurfaces.test.ts`, water/metal-water smokes |
| Wing/Metal/Vanish cap prototypes, enemy aggro suppression and cannon launch | `tests/wing.test.ts`, caps/vanish/cannon smokes |
| Koopa shell riding and jump-off | `tests/shellRide.test.ts`, shell smoke |
| Shared experiment launcher + URL persistence | experiments smoke |
| Across Dimensions 3D/side/top switching + safe side-view transitions | `tests/viewTransitions.test.ts`, dimensions smoke |
| Runtime God editor (spawn/drag/resize/scale/ramp/bounce/actors/coins, gravity, undo, persistence) | editor tests/smokes |
| Infinite Dream streaming (seed, reachability, biomes, distance difficulty, recovery) | `tests/dream.test.ts`, dream smoke |
| Hundred Marios full-runtime inspector entries and shared real-model crowd | `tests/crowdMario.test.ts`, `tests/hundred.test.ts`, hundred smoke |
| Diorama board, free-fly camera and clickable actors | diorama smoke |
| Redesigned experiment launcher + pause-menu experiment grid | menu screenshots, experiments smoke |
| Authored Mushroom Kingdom 1-1 side level | `scripts/mushroom2d-smoke.mjs` |
| Dream real tree/coin props | dream smoke |
| A Hundred Marios (48–100 real 3D Mario meshes, 8 full runtime agents, pickable inspector) | hundred test/smoke, measured ~61 FPS |

## Prototype (works, but not source-complete)

- **Mario Across Dimensions** — camera and movement modes work; full 3D→2D collision
  remapping is not implemented. Side mode chooses a nearby safe lane (including near walls,
  ledges and airborne transitions) and occludes the side camera, but level geometry is not
  converted into a 2D collision plane.
- **Mario vs. God** — platform/ramp/bounce collision is synchronized with visuals; actors and
  coins can be moved, scaled and deleted. Spawned actors/coins are not persisted across reloads.
- **Infinite Dream** — main islands are connected, with blue-biome towers and violet-biome
  bridges; architecture is still primitive and lacks spiral structures/difficulty curve.
- **A Hundred Marios** — eight agents use full `MarioRuntime` inputs/physics while the rest use
  simplified sprites; no coin/goal interactions, crowd physics, or pathfinding.
- **Caps** — Wing flight/cannon launch use a source-derived flight model; source takeoff
  animations and exact Vanish/Metal interaction rules remain approximations.
- **Audio** — course/race/boss jingles use extracted samples and synth fallback; grass,
  castle and slide play rendered stems built from source sequence layers and instrument samples,
  but full sequencer/acoustic mix and spatial mix are missing.
- **Camera** — orbit camera with Lakitu contexts, collision, auto-follow and speed
  look-ahead; `camera.c` regions/scripted shots are missing.

## Missing

- Full SM64 course coverage (all 22 native terrain maps load; Bowser arenas have a simplified boss; JRB has a chest puzzle; BBH has terrain, Boos, a Big Boo event-gated star, flames, crates and four moving platforms; CCM has penguins/Mr. Blizzard; JRB is terrain/water-only; LLL has Bullies/Big Bully; SSL/WDW/TTM/THI/TTC/RR/Bowser arenas are terrain-only; SL has Mr. Blizzard; most objects remain) and painting destinations.
- Mission-specific coin totals and exact source mission conditions beyond the boss/race/gate/red-coin events.
- King Bob-omb full cutscene, Koopa source path collision and remaining object behaviors.
- Poison gas placement in a playable area, remaining particle/environmental effects and source-timed sequences.
- Metal/Vanish cap full behavior and remaining special mechanics.
- Exact source save-file layout and all castle-area scripts.

## Next milestones

1. Full SM64 course coverage and remaining painting destinations; exact mission coin totals.
2. Source music sequencing from `assets/extracted/sound/sequences/*.m64` with instruments.
3. Exact `camera.c` regions, scripted shots and Lakitu behavior.
4. Remaining object behaviors from `reference/sm64/src/game/behaviors/`.
5. Full-Mario-agent simulation for A Hundred Marios and final experiment polish.

## Verification snapshot (round 100)

- `tsc --noEmit`: clean.
- Vitest: **161 passed, 1 skipped**.
- `node scripts/check-deploy-assets.mjs`: all 22 native courses, shared actors, Bowser/flame actors, and converted audio present; every course object model has a mesh.
- Production Vite build: clean.
- Browser smokes passing: native `sm64`, all courses (including all Bowser arenas and Bowser fire breath), objects, missions, experiments, dimensions, dream, and A Hundred Marios at ~61 FPS.
- Live Worker returns the intentional `401` password gate.

The objective is still **not complete**: mission-specific object coverage, source-exact camera regions/scripts, full instrument-accurate audio, and several special mechanics remain as documented above.
