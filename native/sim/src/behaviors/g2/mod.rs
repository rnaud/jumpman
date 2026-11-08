//! SM64 group 2 (courses 7-13: Lethal Lava Land, Shifting Sand Land, Dire Dire Docks,
//! Snowman's Land, Wet-Dry World, Tall Tall Mountain, Tiny-Huge Island): behaviors ported from
//! `reference/sm64/src/game/behaviors/*.inc.c`. Each submodule documents which decomp files it
//! covers; [`register`] is the only entry point `behaviors::register_all` calls.
use crate::object::ObjectBehaviorKind as K;
use crate::object_manager::BehaviorTable;

pub mod dyn_collision;
pub mod interact;
pub mod lll;
pub mod obj;
pub mod phys;


pub fn register(t: &mut BehaviorTable) {
    // Lethal Lava Land
    t.register(K::LllHexPlatform, lll::init_plain, lll::hex_platform);
    t.register(K::LllHexRing, lll::init_plain, lll::hex_ring);
    t.register(K::VolcanoFlames, lll::init_volcano_flames, lll::volcano_flames);
    t.register(K::LllSinkRect, lll::init_plain, lll::sink_rect);
    t.register(K::LllSinkSquare, lll::init_sink_square, lll::sink_square);
    t.register(K::LllTiltPyramid, lll::init_tilt_pyramid, lll::tilt_pyramid);
    t.register(K::LllOctMesh, lll::init_oct_mesh, lll::oct_mesh);
    t.register(K::LllSinkRock, lll::init_sink_rock, lll::sink_rock);
    t.register(K::RollingLogLll, lll::init_rolling_log_lll, lll::rolling_log_lll);
    t.register(K::RollingLogTtm, lll::init_rolling_log_ttm, lll::rolling_log_ttm);
    t.register(K::VolcanoTrap, lll::init_plain, lll::volcano_trap);
    t.register(K::LllWoodBridge, lll::init_wood_bridge, lll::wood_bridge);
    t.register(K::LllWoodPiece, lll::init_wood_piece, lll::wood_piece);
    t.register(K::LllDrawbridgeSpawner, lll::init_drawbridge_spawner, lll::drawbridge_spawner);
    t.register(K::LllDrawbridge, lll::init_drawbridge, lll::drawbridge);
    t.register(K::LllFireBars, lll::init_fire_bars, lll::fire_bars);
    t.register(K::LllHexFlame, lll::init_hex_flame, lll::hex_flame);
    t.register(K::Flamethrower, lll::init_flamethrower, lll::flamethrower);
    t.register(K::FlamethrowerFlame, lll::init_flamethrower_flame, lll::flamethrower_flame);
    t.register(K::BouncingFireball, lll::init_bouncing_fireball, lll::bouncing_fireball);
    t.register(K::BouncingFireFlame, lll::init_bouncing_fire_flame, lll::bouncing_fire_flame);
    t.register(K::BowserPuzzle, lll::init_bowser_puzzle, lll::bowser_puzzle);
    t.register(K::BowserPuzzlePiece, lll::init_puzzle_piece, lll::puzzle_piece);
    t.register(K::Pole, lll::init_pole, lll::pole);

    for k in [K::LllHexFlame, K::FlamethrowerFlame, K::BouncingFireFlame] {
        t.set_interact(k, interact::interact);
    }
}

/// Kinds drawn with their face angles (pitch, yaw, roll) rather than the move yaw.
pub fn uses_face_angles(k: K) -> bool {
    matches!(
        k,
        K::LllHexPlatform | K::LllHexRing | K::LllSinkRect | K::LllSinkSquare | K::LllOctMesh | K::LllSinkRock | K::RollingLogLll | K::RollingLogTtm | K::VolcanoTrap | K::LllWoodPiece | K::LllDrawbridge | K::LllFireBars | K::BowserPuzzlePiece
    )
}
