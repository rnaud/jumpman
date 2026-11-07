# Porting Jumpman from Three.js/TypeScript to Rust

The Rust game lives in `native/`: `jumpman-sim` (no engine dependency, f64 like the TS original) and the Bevy app (`jumpman`) that renders it.

## Method
Every sim module is ported file-for-file from `src/sim/*.ts` and checked against **golden fixtures generated from the TypeScript** (`GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts`, then `cargo test` in `native/`). Values must match bit-for-bit, not approximately.

## Stages
| # | Stage | Source | Status |
|---|-------|--------|--------|
| 1 | Math, surfaces, collision world | `math.ts` `surfaces.ts` `collision.ts` | done, golden-tested (bob, castle_inside) |
| 2 | Mario state, helpers, ground/air steps | `state.ts` `marioHelpers.ts` `marioStep.ts` | ported, compiles; golden tests pending |
| 3 | Action handlers (stationary, moving, airborne, automatic, submerged, cutscene, holding, object, cannon, ending) | `actions/*.ts` | todo (~3.7k lines) |
| 4 | `MarioRuntime` frame pipeline, input mapping, health, caps | `MarioRuntime.ts` | todo (1.3k lines); depends on 5 for held/used objects |
| 5 | Object system and behaviors | `object.ts` `objectManager.ts` `behaviors/*` | todo (~5k lines) |
| 6 | Course loading, warps, missions, save data | `game/world.ts` `sm64World.ts` `save.ts` | todo |
| 7 | Rendering: Mario model/animation, course, sky, water, particles | `game/*View*` `MarioModel.ts` | partly in Bevy prototype |
| 8 | Camera rig, input, UI/HUD, audio | `CameraRig.ts` `Input.ts` `ui/*` | todo |
| 9 | Link/Banjo characters, experiments | `linkController.ts` `banjoController.ts` `experiments/*` | todo |

Replace the prototype's simplified `movement.rs` with `jumpman-sim` once stage 4 lands.
