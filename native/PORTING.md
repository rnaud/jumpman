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
| 6 | Course loading, warps, missions, save data | `game/world.ts` `sm64World.ts` `save.ts` | todo |
| 7 | Rendering: Mario model/animation, course, sky, water, particles | `game/*View*` `MarioModel.ts` | partly in Bevy prototype |
| 8 | Camera rig, input, UI/HUD, audio | `CameraRig.ts` `Input.ts` `ui/*` | todo |
| 9 | Link/Banjo characters, experiments | `linkController.ts` `banjoController.ts` `experiments/*` | todo |

Replace the prototype's simplified `movement.rs` with `jumpman-sim` once stage 4 lands.
