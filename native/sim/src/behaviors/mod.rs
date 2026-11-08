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
pub mod bowser_flames_data;
pub mod bowser_sm64;
pub mod flamethrower_sm64;
pub mod breakable_box;
pub mod chain_chomp;
pub mod chest;
pub mod king_bobomb;
pub mod moving_yellow_coin;
pub mod sounds;
pub mod star_box;
pub mod water_bomb_cannon;
pub mod water_objects;

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
    // bowser.inc.c, bowser_bomb.inc.c, bowser_flame.inc.c (faithful port, Bowser arenas)
    t.register(K::SmBowser, bowser_sm64::init_bowser, bowser_sm64::bowser_behavior);
    t.register(K::SmBowserBomb, bowser_sm64::init_bowser_bomb, bowser_sm64::bowser_bomb_behavior);
    t.register(K::SmBowserFlame, bowser_sm64::init_bowser_flame, bowser_sm64::bowser_flame_behavior);
    t.register(K::SmBowserShockWave, bowser_sm64::init_shockwave, bowser_sm64::shockwave_behavior);
    // flamethrower.inc.c (RR, BitDW, BitFS, BitS fire jets)
    t.register(K::SmFlamethrower, flamethrower_sm64::init_flamethrower, flamethrower_sm64::flamethrower_behavior);
    t.register(K::SmFlamethrowerFlame, flamethrower_sm64::init_flame, flamethrower_sm64::flame_behavior);
    t.register(K::WingCap, star_box::init_wing_cap, star_box::wing_cap_behavior);
    t.register(K::WingBox, star_box::init_star_box, star_box::star_box_behavior);
    t.register(K::StarBox, star_box::init_star_box, star_box::star_box_behavior);
    t.register(K::Chest, chest::init_chest, chest::chest_behavior);
    t.register(K::BkEnemy, crate::bk_enemy::init_bk_enemy, crate::bk_enemy::bk_enemy_behavior);
    register_world_actors(t);
    t.register(K::KoopaShellUnderwater, water_objects::init_koopa_shell_underwater, water_objects::koopa_shell_underwater_behavior);
    t.register(K::Whirlpool, water_objects::init_whirlpool, water_objects::whirlpool_behavior);
    g1::register(t);
    g2::register(t); // SM64 courses 7-13, see g2/mod.rs
}

pub mod g2;

pub mod big_boo;
pub mod big_bully;
pub mod bobomb_buddy;
pub mod boo;
pub mod bully;
pub mod flame;
pub mod klepto;
pub mod koopa;
pub mod koopa_the_quick;
pub mod mr_blizzard;
pub mod one_up;
pub mod penguin;
pub mod poison_gas;
pub mod pokey;
pub mod racing_penguin;
pub mod snowman;
pub mod tree;
pub mod warp;
pub mod whomp;
pub mod g1;



/// Koopa, penguins, snowman, ghosts, bullies, Whomps, Pokey, Mr. Blizzard, Klepto, Bob-omb
/// Buddy, 1-Up, flames, gas, trees and warps.
fn register_world_actors(t: &mut BehaviorTable) {
    t.register(K::Koopa, koopa::init_koopa, koopa::koopa_behavior);
    t.register(K::KoopaShell, koopa::init_koopa_shell, koopa::koopa_shell_behavior);
    t.register(K::KoopaTheQuick, koopa_the_quick::init_koopa_the_quick, koopa_the_quick::koopa_the_quick_behavior);
    t.register(K::KoopaFlag, koopa_the_quick::init_koopa_flag, koopa_the_quick::koopa_flag_behavior);
    t.register(K::RacingPenguin, racing_penguin::init_racer, racing_penguin::race_penguin);
    t.register(K::Penguin, penguin::init_penguin, penguin::penguin_behavior);
    t.register(K::BabyPenguin, penguin::init_baby_penguin, penguin::baby_penguin_behavior);
    t.register(K::TuxiesMother, penguin::init_mother, penguin::mother_behavior);
    t.register(K::SnowmanBody, snowman::init_snowman_body, snowman::snowman_body);
    t.register(K::SnowmanHead, snowman::init_snowman_head, snowman::snowman_head);
    t.register(K::Whomp, whomp::init_whomp, whomp::whomp_behavior);
    t.register(K::WhompKing, whomp::init_whomp_king, whomp::whomp_king_behavior);
    t.register(K::BigBoo, big_boo::init_big_boo, big_boo::big_boo_behavior);
    t.register(K::Boo, boo::init_boo, boo::boo_behavior);
    t.register(K::BigBully, big_bully::init_big_bully, big_bully::big_bully_behavior);
    t.register(K::Bully, bully::init_bully, bully::bully_behavior);
    t.register(K::Pokey, pokey::init_pokey, pokey::pokey_behavior);
    t.register(K::MrBlizzard, mr_blizzard::init_mr_blizzard, mr_blizzard::mr_blizzard_behavior);
    t.register(K::Klepto, klepto::init_klepto, klepto::klepto_behavior);
    t.register(K::BobombBuddy, bobomb_buddy::init_bobomb_buddy, bobomb_buddy::bobomb_buddy_behavior);
    t.register(K::BobombBuddyAdvice, bobomb_buddy::init_bobomb_buddy, bobomb_buddy::bobomb_buddy_behavior);
    t.register(K::OneUp, one_up::init_one_up, one_up::one_up_behavior);
    t.register(K::Flame, flame::init_flame, flame::flame_behavior);
    t.register(K::PoisonGas, poison_gas::init_poison_gas, poison_gas::poison_gas_behavior);
    t.register(K::Tree, tree::init_tree, tree::tree_behavior);
    t.register(K::Warp, warp::init_warp, warp::warp_behavior);
}
