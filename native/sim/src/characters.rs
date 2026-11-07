//! Hook points for the alternate playable characters (stage 9). Mario-only builds keep
//! these as no-ops; `linkController.ts` / `banjoController.ts` will be ported into
//! `link_controller.rs` / `banjo_controller.rs` and these functions will forward to them.
use crate::mario_runtime::MarioRuntime;

/// `swordReach(m)` (linkController.ts): extra punch radius while Link slashes. Mario: 0.
/// TODO(stage 9): `m.action == ACT_PUNCHING && (arg == SLASH || arg == FINISHER) && flags & MARIO_PUNCHING ? 82 : 0`.
pub fn sword_reach(_m: &MarioRuntime) -> f64 { 0.0 }

/// `linkGuarding(m)`: Link's shield is up (`ACT_IDLE` with `action_arg == 0x4c`).
/// TODO(stage 9): not evaluated for Mario, which never has `action_arg == 0x4c` in idle.
pub fn link_guarding(m: &MarioRuntime) -> bool {
    m.action == crate::state::ACT_IDLE && m.action_arg == 0x4c as f64
}

/// `stepLink(m)`: Link's free-movement controller. Returns true if it handled the frame
/// (skip the normal action loop). TODO(stage 9).
pub fn step_link(_m: &mut MarioRuntime) -> bool { false }

/// `stepBanjo(m)`: Banjo's controller. Returns true if it handled the frame (skip the normal action loop).
/// TODO(stage 9).
pub fn step_banjo(_m: &mut MarioRuntime) -> bool { false }
