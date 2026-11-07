# Porting Jumpman from Three.js/TypeScript to Rust

The Rust game lives in `native/`: `jumpman-sim` (no engine dependency, f64 like the TS original) and the Bevy app (`jumpman`) that renders it.

## Method
Every sim module is ported file-for-file from `src/sim/*.ts` and checked against **golden fixtures generated from the TypeScript** (`GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts`, then `cargo test` in `native/`). Values must match bit-for-bit, not approximately.

## Stages
| # | Stage | Source | Status |
|---|-------|--------|--------|
| 1 | Math, surfaces, collision world | `math.ts` `surfaces.ts` `collision.ts` | done, golden-tested (bob, castle_inside) |
| 2 | Mario state, helpers, ground/air steps | `state.ts` `marioHelpers.ts` `marioStep.ts` | ported, compiles; golden tests pending |
| 3 | Action handlers (stationary, moving, airborne, automatic, submerged, cutscene, holding, object, cannon, ending) | `actions/*.ts` | stubs in `native/sim/src/actions/*.rs` (one file per TS file, `<name>_action(m) -> Option<bool>` match tables); ports todo (~3.7k lines) |
| 4 | `MarioRuntime` frame pipeline, input mapping, health, caps | `MarioRuntime.ts` | ported (`mario_runtime.rs`; owns `m.objects`); golden `tests/mario_run.rs` passes the idle prefix, rest awaits stage 3 |
| 5 | Object system and behaviors | `object.ts` `objectManager.ts` `behaviors/*` | core ported (`object.rs`, `object_manager.rs`, `BehaviorTable` hook in `behaviors/mod.rs`); world actors (koopa, koopaTheQuick, penguins, snowman, whomp, boo, bully, pokey, klepto, buddy, ...) ported and golden-tested (`tests/behaviors_golden.rs`); use `js_math::js_sin/js_cos` for `Math.sin/cos` |
| 6 | Course loading, warps, missions, save data | `game/world.ts` `sm64World.ts` `save.ts` | course loading, catalogue/missions, spawns, warp/door data, coins, playground done (`src/game/`); warps/doors/save todo (see below) |
| 7 | Rendering: Mario model/animation, course, sky, water, particles | `game/*View*` `MarioModel.ts` | Mario + animation, course/props/water/playground, generic object views, coins done (`src/render/`); particles/sky todo |
| 8 | Camera rig, input, UI/HUD, audio | `CameraRig.ts` `Input.ts` `ui/*` | camera, input, HUD done (`src/camera/`, `input.rs`, `hud.rs`); audio todo |
| 9 | Link/Banjo characters, experiments | `linkController.ts` `banjoController.ts` `experiments/*` | todo |


## Bevy app architecture (`native/src`)
`main.rs` parses the course (`cargo run -- bob --act 2 --entry WARP_NODE_0A`, `--list`; env `JUMPMAN_COURSE/ACT/ENTRY`), builds a `GameSim` resource and adds plugins. Debug hooks: `JUMPMAN_EXIT_AFTER=<s>`, `JUMPMAN_SCREENSHOT=<png>`, `JUMPMAN_WALK=1`.

| Module | Role | TS source |
|--------|------|-----------|
| `game/course_data.rs` | serde mirror of `assets/courses/*.json` | `sm64World.ts` `CourseData` |
| `game/courses.rs` (+`courses_table.rs`) | catalogue, missions | `courses.ts` |
| `game/native_collision.rs` | exported collision -> `CollisionWorld`, coin formations | `nativeCollision.ts` `coinFormation.ts` |
| `game/world.rs` | `LoadedCourse::load`: world, spawn/warp-node arrival, props, `ObjectSpec`s -> `ObjectManager`, death height | `sm64World.ts` `courseObjects.ts` `world.ts` |
| `game/playground.rs` | procedural movement playground (collision + render pieces) | `playgroundLayout.ts` `sm64Playground.ts` |
| `game/runner.rs` | `GameSim` (engine-free apart from `Resource`): 30 Hz `tick` = coins -> `object_manager::step` -> `MarioRuntime::step` -> `after_mario_step`; interpolation snapshots; pickups; `SimEvent`s | `Game.ts` update loop |
| `game/sm64_animation.rs` | action -> clip choice, frame clock, glb clip metadata | `sm64Animation.ts` `Sm64AnimationPlayer.ts` |
| `sim_plugin.rs` | `Time<Fixed>` 30 Hz, `SimEventMsg` Bevy event, reset | |
| `input.rs` | keyboard/gamepad/mouse -> `ControllerSnapshot` (edges latched to the fixed step) | `Input.ts` |
| `camera/` | `rig` (orbit, radial Bob camera, summit/boss, cannon follow), `transition`, `wall`, `pitch_shake`, `summit`, `npc_dialog`, `link_focus`; `mod.rs` Bevy glue | `CameraRig.ts` and friends |
| `render/` | `course_view` (level glb, props, water, rooms), `mario_view`, `object_view`, `pickup_view`, `billboard` | `Sm64MarioView.ts` `CourseObjects` ... |
| `hud.rs` | bevy_ui power meter/lives/coins/stars/act title/toast/debug | `HUD.ts` |

`cargo test --workspace` covers course loading (golden-world match for bob), spawn selection, headless ticking of every Mario course, camera, input and animation selection.

### Open TODOs (clearly-named hooks for teammates)
- **warps/doors/paintings**: `LoadedCourse::{doors, warp_spawns, fading_warps, death_warp, fall_warp}` are loaded but unused; `MarioRuntime::request_death_warp` is unset; `ObjectEvent::Warp` is ignored. Needs course switching via `GameSim::load` + `CourseRequest{entry,push}`, door colliders (`roomDoors.ts`) and `castlePaintings.ts`.
- **save data / stars / missions** (`save.ts`, `StarWarp.ts`, `ActSelector.ts`, `CourseResult.ts`): `GameSim::stars` is never incremented; star models (`sideStars`, `redReward`, `hundredCoinStar`).
- **object_view behavior rigs** (`render/object_view.rs` header lists them), object colliders (starBox, chainGate, woodenPost, blast walls), moving platforms (`movingPlatform.ts`; lifts are static at home pose), cannons (`cannons.ts`; `CameraRig::follow_cannon_shot` is ported), wing switch, signposts/dialogs (`signpost.ts`; `camera/npc_dialog.rs` ported but unused).
- **mario_view variants**: hand/cap/eye switch nodes, metal/vanish materials, teleport fade, clip crossfade, shadows.
- **sky/particles/audio** (`Particles.ts`, `coinEffects.ts`, `Sfx.ts`, `music.ts`; consume `SimEventMsg`), `actorShadows.ts`, goombas (`goomba.ts`), `TouchControls.ts`.
- **Link/Banjo** (`as_link`/`as_banjo` flags; `camera/link_focus.rs` ported but not wired), Zelda courses' `kokiri`/`dungeon` extras, `mushroom2d`, `shipCollision`, linked areas (ddd/ccm area switching).
- camera: lookPitch (first person), king/chomp/ending cameras, `CameraTransition` on course change, dynamic collider blockers.
