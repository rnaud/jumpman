//! Ports of `src/sim/behaviors/*.ts`.
//!
//! One file per TS behavior file. Each exposes `init_*(o: &mut SimObject)` and
//! `*_behavior(id: ObjId, m: &mut MarioRuntime)` and is wired up in [`register_all`] with
//! `t.register(ObjectBehaviorKind::Bobomb, bobomb::init_bobomb, bobomb::bobomb_behavior)`
//! (mirrors the INITIALIZERS / UPDATERS tables in objectManager.ts). Kinds whose init is
//! inline in objectManager.ts (grillDoor, mountainBallSpawner, waterBombSpawner,
//! waterBombShadow, blastWall) already have it in `BehaviorTable::new`; register only their
//! update with `t.set_update(..)`.
use crate::object_manager::BehaviorTable;

/// Add new behavior modules here.
pub fn register_all(_t: &mut BehaviorTable) {}
