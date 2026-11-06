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
| Native course terrain: Bob, Castle Grounds/Lobby, Whomp's Fortress, Big Boo's Haunt, Cool Cool Mountain, PSS, Playground | course smoke, native collision tests |
| Big Boo's Haunt Boos, Big Boo boss, flames, crates and moving platforms | `tests/boo.test.ts`, `tests/bigBoo.test.ts`, `tests/flame.test.ts`, course smoke |
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
| Audio samples/sequences, source-derived course loops and mute preference | `tests/audioAssets.test.mjs`, `tests/music.test.ts`, sm64 smoke |
| Water surfaces, splash/fog, metal-water walking | `tests/waterSurfaces.test.ts`, water/metal-water smokes |
| Wing/Metal/Vanish cap prototypes, enemy aggro suppression and cannon launch | `tests/wing.test.ts`, caps/vanish/cannon smokes |
| Koopa shell riding and jump-off | `tests/shellRide.test.ts`, shell smoke |
| Shared experiment launcher + URL persistence | experiments smoke |
| Across Dimensions 3D/side/top switching | dimensions smoke |
| Runtime God editor (spawn/drag/resize/scale/ramp/bounce/actors/coins, gravity, undo, persistence) | editor tests/smokes |
| Infinite Dream streaming (seed, reachability, biomes, recovery) | `tests/dream.test.ts`, dream smoke |
| A Hundred Marios (48–100 agents, 4 full MarioRuntime agents, personalities, speed, inspector) | hundred test/smoke, measured ~61 FPS |

## Prototype (works, but not source-complete)

- **Mario Across Dimensions** — camera and movement modes work; full 3D→2D collision
  remapping is not implemented. Side mode constrains a lane and now chooses a clear lane and
  occludes the side camera, but level geometry is not converted into a 2D collision plane.
- **Mario vs. God** — platform/ramp/bounce collision is synchronized with visuals; actors and
  coins can be moved, scaled and deleted. Spawned actors/coins are not persisted across reloads.
- **Infinite Dream** — main islands are connected, with blue-biome towers and violet-biome
  bridges; architecture is still primitive and lacks spiral structures/difficulty curve.
- **A Hundred Marios** — four agents use full `MarioRuntime` inputs/physics while the rest use
  simplified sprites; no coin/goal interactions, crowd physics, or pathfinding.
- **Caps** — Wing flight/cannon launch use a source-derived flight model; source takeoff
  animations and exact Vanish/Metal interaction rules remain approximations.
- **Audio** — course/race/boss jingles use extracted samples and synth fallback; grass,
  castle and slide courses play source-derived note loops, but full instrument sequencing and
  spatial mix are missing.
- **Camera** — orbit camera with Lakitu contexts, collision, auto-follow and speed
  look-ahead; `camera.c` regions/scripted shots are missing.

## Missing

- Full SM64 course coverage (BBH has terrain, Boos, a Big Boo event-gated star, flames, crates and four moving platforms; CCM is terrain-only; most objects remain) and painting destinations.
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
