//! Port of `src/sim/actions/submerged.ts` (normal-cap swimming, `submergedStep`).
//!
//! Called from `MarioRuntime::step` when Mario is swimming. Port `submergedStep` here
//! (`submerged_step`); it may call `m.set_action`, `m.step_air` etc. Helpers in the TS file:
//! - `submergedStep`
use crate::mario_runtime::MarioRuntime;

/// STUB: does nothing. `surface` is the water surface height at Mario's x/z.
pub fn submerged_step(_m: &mut MarioRuntime, _surface: f64) {}
