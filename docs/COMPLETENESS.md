# Jumpman completeness audit

Updated November 5, 2026. This audit distinguishes **working and verified** behavior from
**prototype** and **missing** behavior. It is intentionally conservative: a feature is only
listed as complete when a unit or browser smoke exercises it.

## Verified foundation

| Area | Evidence |
| --- | --- |
| 30 Hz Mario runtime, input mapping, ground/air steps | movement/action Vitest suites, native browser smoke |
| Actions: jumps, dives, ground pound, ledge/ceiling, poles, holding, punches, knockback, death/drowning/lava/quicksand | `tests/actions.test.ts`, `tests/health.test.ts`, browser smokes |
| Bob-omb Battlefield object layer | Bob-ombs/crates/Chain Chomp/stakes/gate/switch/cannon/Koopa/King | `tests/objects.test.ts`, `scripts/object-smoke.mjs` |
| Bob mission acts 1–3 spawning and selectors | `tests/missions.test.ts`, object smoke |
| Save slots, stars, best coins, active slot | `tests/save.test.ts`, object smoke |
| Painting/castle/PSS warps | `scripts/warp-smoke.mjs`, painting tests |
| Camera modes and contexts | `tests/cameraModes.test.ts`, dimensions smoke |
| Audio samples/sequences and mute preference | `tests/audioAssets.test.mjs`, asset validation |
| Water surfaces, splash/fog, metal-water walking | `tests/waterSurfaces.test.ts`, water/metal-water smokes |
| Wing/Metal/Vanish cap prototypes and cannon launch | `tests/wing.test.ts`, caps/cannon smokes |
| Across Dimensions 3D/side/top switching | dimensions smoke |
| Runtime God editor (spawn/drag/resize/scale/ramp/bounce/actors/coins, gravity, undo, persistence) | editor tests/smokes |
| Infinite Dream streaming (seed, reachability, biomes, recovery) | `tests/dream.test.ts`, dream smoke |
| A Hundred Marios (48–100 agents, personalities, speed, inspector) | hundred test/smoke, measured ~62 FPS |

## Prototype (works, but not source-complete)

- **Mario Across Dimensions** — camera and movement modes work; full 3D→2D collision
  remapping is not implemented. Side mode constrains a lane and now chooses a clear lane and
  occludes the side camera, but level geometry is not converted into a 2D collision plane.
- **Mario vs. God** — platform/ramp/bounce collision is synchronized with visuals; actors and
  coins can be moved and scaled. Spawned actors/coins are not persisted and there is no
  generic delete/inspector for arbitrary objects.
- **Infinite Dream** — main islands are connected, but architecture is still primitive boxes
  and satellites; no towers/spiral bridges or difficulty curve.
- **A Hundred Marios** — simplified sprite physics rather than `MarioRuntime` inputs; no
  coin/goal interactions, crowd physics, or agent pathfinding.
- **Caps** — Wing flight/cannon launch use a source-derived flight model; source takeoff
  animations and full Vanish/Metal interaction rules are missing.
- **Audio** — course/race/boss jingles use extracted samples and synth fallback; full
  sequencing, instruments and spatial mix are missing.
- **Camera** — orbit camera with Lakitu contexts, collision, auto-follow and speed
  look-ahead; `camera.c` regions/scripted shots are missing.

## Missing

- Full SM64 course coverage and remaining painting destinations.
- Red coins, mission-specific coin totals and exact source mission conditions.
- King Bob-omb grab/cutscene, Koopa dialogue/timer/path collision, remaining object behaviors.
- Poison gas, remaining particle/environmental effects, source-timed sequences.
- Metal/Vanish cap full behavior, shell riding, remaining special mechanics.
- Exact source save-file layout and all castle-area scripts.

## Next milestones

1. Mission-correct Bob-omb Battlefield objectives (red coins, per-act star positions and
   conditions, star persistence per act).
2. Castle/level script completion and remaining painting destinations.
3. Source camera regions and remaining audio sequences.
4. Continue object behavior coverage from `reference/sm64/src/game/behaviors/`.
