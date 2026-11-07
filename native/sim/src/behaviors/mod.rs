//! Ports of `src/sim/behaviors/*.ts`.
//!
//! One file per TS behavior file. Each exposes `init_*(o: &mut SimObject)` and
//! `*_behavior(id: ObjId, m: &mut MarioRuntime)` and is wired up in [`register_all`] with
//! `t.register(ObjectBehaviorKind::Bobomb, bobomb::init_bobomb, bobomb::bobomb_behavior)`
//! (mirrors the INITIALIZERS / UPDATERS tables in objectManager.ts). Kinds whose init is
//! inline in objectManager.ts (grillDoor, mountainBallSpawner, waterBombSpawner,
//! waterBombShadow, blastWall) already have it in `BehaviorTable::new`; register only their
//! update with `t.set_update(..)`.
use crate::object::ObjectBehaviorKind as K;
use crate::object_manager::BehaviorTable;

pub mod bob_objects;
pub mod bobomb;
pub mod bowser;
pub mod bowser_flame;
pub mod breakable_box;
pub mod chain_chomp;
pub mod chest;
pub mod king_bobomb;
pub mod moving_yellow_coin;
pub mod star_box;
pub mod water_bomb_cannon;

/// Add new behavior modules here.
pub fn register_all(t: &mut BehaviorTable) {
    // bobObjects.ts, bobomb.ts, kingBobomb.ts, chainChomp.ts, waterBombCannon.ts, bowser.ts,
    // bowserFlame.ts, breakableBox.ts, movingYellowCoin.ts, starBox.ts, chest.ts
    t.register(K::Bobomb, bobomb::init_bobomb, bobomb::bobomb_behavior);
    t.register(K::MovingYellowCoin, moving_yellow_coin::init_moving_yellow_coin, moving_yellow_coin::moving_yellow_coin);
    t.register(K::BreakableBox, breakable_box::init_breakable_box, breakable_box::breakable_box_behavior);
    t.register(K::BreakableBoxSmall, breakable_box::init_breakable_box_small, breakable_box::breakable_box_small_behavior);
    t.register(K::ChainChomp, chain_chomp::init_chain_chomp, chain_chomp::chain_chomp_behavior);
    t.register(K::WoodenPost, chain_chomp::init_wooden_post, chain_chomp::wooden_post_behavior);
    t.register(K::ChainChompGate, chain_chomp::init_chain_chomp_gate, chain_chomp::chain_chomp_gate_behavior);
    t.register(K::ChainPart, chain_chomp::init_chain_part, chain_chomp::chain_part_behavior);
    t.register(K::PurpleSwitch, bob_objects::init_purple_switch, bob_objects::purple_switch_behavior);
    t.register(K::OpenableGrill, bob_objects::init_openable_grill, bob_objects::openable_grill_behavior);
    t.set_update(K::GrillDoor, bob_objects::grill_door_behavior);
    t.register(K::PitBowlingBall, bob_objects::init_pit_bowling_ball, bob_objects::pit_bowling_ball_behavior);
    t.register(K::MountainBall, bob_objects::init_mountain_ball, bob_objects::mountain_ball_behavior);
    t.set_update(K::MountainBallSpawner, bob_objects::mountain_ball_spawner);
    t.register(K::KingBobomb, king_bobomb::init_king_bobomb, king_bobomb::king_bobomb_behavior);
    t.register(K::WaterBombCannon, water_bomb_cannon::init_water_bomb_cannon, water_bomb_cannon::water_bomb_cannon_behavior);
    t.register(K::CannonBarrel, water_bomb_cannon::init_cannon_barrel, water_bomb_cannon::cannon_barrel_behavior);
    t.register(K::WaterBomb, water_bomb_cannon::init_water_bomb, water_bomb_cannon::water_bomb_behavior);
    t.set_update(K::WaterBombSpawner, water_bomb_cannon::water_bomb_spawner);
    t.set_update(K::WaterBombShadow, water_bomb_cannon::water_bomb_shadow);
    t.register(K::BowserMine, bowser::init_bowser_mine, bowser::bowser_mine_behavior);
    t.register(K::Bowser, bowser::init_bowser, bowser::bowser_behavior);
    t.register(K::BowserFlame, bowser_flame::init_bowser_flame, bowser_flame::bowser_flame_behavior);
    t.register(K::WingCap, star_box::init_wing_cap, star_box::wing_cap_behavior);
    t.register(K::WingBox, star_box::init_star_box, star_box::star_box_behavior);
    t.register(K::StarBox, star_box::init_star_box, star_box::star_box_behavior);
    t.register(K::Chest, chest::init_chest, chest::chest_behavior);
}
