# Native Mario animation audit — 2026-11-04

> **Historical:** written against the removed Three.js browser build (pnpm/Vite, `?level=` URLs, dev-server scripts). The game is now the Rust/Bevy app in `native/`; see the root [README](../README.md) for current build and run instructions.

Scope: all **55 implemented controller actions**, their clip variants, native playback timing, and graphics-only orientation. Compared with `reference/sm64/include/mario_animation_ids.h`, `reference/sm64/src/game/mario*.c`, `reference/sm64/src/engine/graph_node.c`, and the copied Mario GLB's 209 clips. This is an animation integration audit, not a claim that Jumpman implements the entire SM64 game.

## Corrected

| Area | Finding and correction |
| --- | --- |
| Run/walk | Running was normalized to 1× at speed 32. Source acceleration is speed / 4: **8×** at 32, a 72-frame run cycle every 0.30 seconds. Use the greater of intended speed and forward velocity, with the original 22/18 walk/run hysteresis. |
| Slow movement | Added start-tiptoe/tiptoe and their distinct rates. Deceleration uses the walking clip with a minimum 1/16× rate; ice uses the source standing/sliding pose. Crawling uses intended magnitude × 2, after the controller's 0.1 scaling. |
| Side flips | Input already reached SIDE_FLIP. The missing graphics-only half-turn made the native clip face the wrong way. Apply +180° for the flip, both landing states, and finished turn-around; leave physics/camera yaw alone. |
| Turns | Select turning part 1 at speed >=18, part 2 below that. |
| Wall kicks | AIR_HIT_WALL uses the single-frame START_WALLKICK pose (`CC`); WALL_KICK_AIR uses the animated SLIDEJUMP clip (`CB`). These were incorrectly assigned. |
| Landings | Map all seven LAND_STOP states explicitly. Backflip landing uses the triple-jump landing clip. Preserve animation phase from moving landing to stopped landing. |
| Long jumps | Record the slow/fast choice **before** the 1.5 velocity multiplier at takeoff. Use the corresponding jump and landing clips throughout the action. |
| Dive/slides | Continue DIVE into DIVE_SLIDE; STOMACH_SLIDE uses SLIDE_DIVE. CROUCH_SLIDE uses START_CROUCHING. Respect fall-from-butt-slide and fall-from-slide-kick arguments. |
| Dive orientation | Apply the controller's descending dive pitch to the visual model, then clear it on other actions. |
| Punches | Respect this controller's simplified 0/1/2 punch/punch/kick stages instead of the C game's 0/3/6 states; argument 9 selects breakdance. |
| Ground pound | Respect the alternate windup argument and start the falling hold at native frame 10. |
| Start frames | Read the GLB metadata: e.g. jump landing begins at frame 22, freefall landing at 9, and ground-pound hold at 10. Previously every clip incorrectly began at zero. |
| Loop clock | Sample the source frame clock. N frames last N/30 seconds, although glTF's final key is at (N−1)/30. Keep the last frame for its full duration, respect loopStart/backward flags, and clamp one-shots. Use exact stored key times to avoid float32 rounding selecting the preceding STEP key. |
| Transitions | Continue shared clips through landing and slide transitions. Replay genuinely new actions. Missing mappings/clips fail visibly instead of silently displaying idle. |

## Remaining fidelity gaps identified

These are existing controller/export limitations, not fixed by selecting the right clip:

- **Action completion still uses simplified simulation timers.** For example, ledge climb ends after 12 ticks although its clip has 16 frames; standing landing stops use 8 ticks, while native recoveries vary; the ground-pound windup is 12 ticks versus the source's 11+4. Punch/kick and breakdance timers also truncate native recovery. Full parity requires moving the native animation clock into the simulation and replacing those timers with animation-end checks, while retaining input cancel windows. The renderer deliberately does not change physics timing or stretch these clips to hide it.
- **Procedural geometry callbacks are not reconstructed.** Floor alignment, running torso lean, head/eye/hand switches, punch-limb scaling, and cap effects are not all available in the selected exported layout. Wing-cap geometry is hidden for the current normal-cap controller.
- Idle deliberately loops the center breathing clip; the original left/right/center sequence and eventual sleep are not implemented by the controller. Unsupported action families (swimming, flying, carrying, etc.) have no physics handlers here, even though their clips exist in the GLB.
- Exported Goombas are decorative. Imported level collision uses render triangles and does not reproduce source collision types or object behaviors. These are separate from Mario's animation wiring.

## Verification

- `npm test`: 46 passing tests, including explicit coverage of every implemented action; controller-input side flip; slow/fast long-jump capture; source rates and hysteresis; landing continuity; start frames; loop boundaries; single-frame/backward playback; 30/60/144 Hz equivalence.
- `npm run build`: TypeScript and production bundle.
- `URL=http://127.0.0.1:5173/ node scripts/sm64-animation-audit.mjs`: development-server browser audit rendering all 55 actions from the real GLB; verifies timing/orientation and saves run/side-flip/side-landing/wall-kick screenshots under `/tmp/jumpman-animation-audit`.
- `URL=http://127.0.0.1:5173/ node scripts/sm64-smoke.mjs`: Battlefield, run/jump, character swapping, coins/star/reset, playground, and CC0 character; no browser errors.

Runtime diagnostics: `window.__JUMPMAN__.debugState.characterAnimation` exposes clip name, source frame, cycle count, speed and loop mode; `characterRotation` exposes visual yaw/pitch.
