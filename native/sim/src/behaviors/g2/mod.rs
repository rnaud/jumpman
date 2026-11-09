//! SM64 group 2 (courses 7-13: Lethal Lava Land, Shifting Sand Land, Dire Dire Docks,
//! Snowman's Land, Wet-Dry World, Tall Tall Mountain, Tiny-Huge Island): behaviors ported from
//! `reference/sm64/src/game/behaviors/*.inc.c`. Each submodule documents which decomp files it
//! covers; [`register`] is the only entry point `behaviors::register_all` calls.
use crate::object::ObjectBehaviorKind as K;
use crate::object_manager::BehaviorTable;

pub mod amp;
pub mod ddd;
pub mod dyn_collision;
pub mod interact;
pub mod lll;
pub mod obj;
pub mod phys;
pub mod ssl;
pub mod thi;
pub mod wdw;


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

    // Shifting Sand Land
    t.register(K::PyramidTop, ssl::init_plain, ssl::pyramid_top);
    t.register(K::PyramidPillarDetector, ssl::init_pillar_detector, ssl::pillar_detector);
    t.register(K::PyramidTopFragment, ssl::init_pyramid_fragment, ssl::pyramid_fragment);
    t.register(K::PyramidElevator, ssl::init_plain, ssl::pyramid_elevator);
    t.register(K::PyramidWall, ssl::init_pyramid_wall, ssl::pyramid_wall);
    t.register(K::ToxBox, ssl::init_tox_box, ssl::tox_box);
    t.register(K::Grindel, ssl::init_grindel, ssl::grindel);
    t.register(K::HorizontalGrindel, ssl::init_horizontal_grindel, ssl::horizontal_grindel);
    t.register(K::Spindel, ssl::init_spindel, ssl::spindel);

    // Wet-Dry World
    t.register(K::WaterLevelInit, wdw::init_hidden, wdw::water_level_init);
    t.register(K::WaterLevelDiamond, wdw::init_water_diamond, wdw::water_diamond);
    t.register(K::ArrowLift, wdw::init_plain, wdw::arrow_lift);
    t.register(K::SquareFloatingPlatform, wdw::init_floating_platform, wdw::square_floating_platform);
    t.register(K::RectFloatingPlatform, wdw::init_floating_platform, wdw::rect_floating_platform);
    t.register(K::ExpressElevator, wdw::init_plain, wdw::express_elevator);
    t.register(K::ExpressElevatorPlatform, wdw::init_plain, wdw::express_elevator_platform);
    t.register(K::RotatingPlatformWdw, wdw::init_plain, wdw::rotating_platform);
    t.register(K::FloorSwitchHidden, wdw::init_floor_switch_hidden, wdw::floor_switch_hidden);
    t.register(K::HiddenObject, wdw::init_hidden_object, wdw::hidden_object);

    // Tiny-Huge Island
    t.register(K::ThiHugeIslandTop, thi::init_plain, thi::huge_island_top);
    t.register(K::ThiTinyIslandTop, thi::init_plain, thi::tiny_island_top);
    t.register(K::FirePiranhaPlant, thi::init_piranha_plant, thi::piranha_plant);
    t.register(K::SmallPiranhaFlame, thi::init_piranha_flame, thi::small_piranha_flame);
    t.set_interact(K::FirePiranhaPlant, interact::interact);
    t.set_interact(K::SmallPiranhaFlame, interact::interact);

    // Amps (SL, WDW, SSL pyramid)
    t.register(K::CirclingAmp, amp::init_circling_amp, amp::circling_amp);
    t.set_interact(K::CirclingAmp, interact::interact);

    // Dire, Dire Docks
    t.register(K::DddPole, ddd::init_ddd_pole, ddd::ddd_pole);
    t.register(K::SushiShark, ddd::init_sushi_shark, ddd::sushi_shark);
    t.set_interact(K::SushiShark, interact::interact);

    for k in [K::LllHexFlame, K::FlamethrowerFlame, K::BouncingFireFlame] {
        t.set_interact(k, interact::interact);
    }
}

/// Kinds drawn with their face angles (pitch, yaw, roll) rather than the move yaw.
pub fn uses_face_angles(k: K) -> bool {
    matches!(
        k,
        K::LllHexPlatform | K::LllHexRing | K::LllSinkRect | K::LllSinkSquare | K::LllOctMesh | K::LllSinkRock | K::RollingLogLll | K::RollingLogTtm | K::VolcanoTrap | K::LllWoodPiece | K::LllDrawbridge | K::LllFireBars | K::BowserPuzzlePiece | K::PyramidTop | K::PyramidElevator | K::PyramidWall | K::ToxBox | K::Grindel | K::HorizontalGrindel | K::Spindel | K::WaterLevelDiamond | K::ArrowLift | K::SquareFloatingPlatform | K::RectFloatingPlatform | K::ExpressElevator | K::ExpressElevatorPlatform | K::RotatingPlatformWdw | K::FloorSwitchHidden | K::HiddenObject | K::ThiHugeIslandTop | K::CirclingAmp | K::TumblingBridgePlatform
    )
}
