//! Mario Kart 64 surface types and what they do to the kart (engine-free).
//!
//! The ROM exporter (`scripts/mk64_courses.py`, `scripts/mk64_props.py`) keeps the decomp's surface id
//! (`include/mk64.h` `SURFACE_TYPE`) of every collision triangle in the triangle's `force` slot; `WorldGround` hands it
//! to the physics as `Floor::surface`. The numbers below are Mario's rows of the decomp's per-surface tables
//! (`src/data/kart_attributes.c`, indexed `[characterId][surfaceType]`, 15 entries each):
//!
//! | table | use (decomp function) |
//! |---|---|
//! | `D_800E2E90` | thrust loss, back tyres and the front left tyre (`func_80030150`) |
//! | `D_800E2EB0` | thrust loss, front right tyre (`func_80030150`) |
//! | `D_800E2A90` | side grip loss, back tyres (`func_8002FCA8`) |
//! | `D_800E2AB0` | side grip loss, front tyres (`func_8002FCA8`) |
//! | `D_800E3210` | brake loss, back tyres (`func_800323E4`) |
//!
//! Also here: the floor reaction each course uses (`func_8003F46C` picks `func_8003E048` / `E37C` / `E6EC` / `E9EC` /
//! `EE2C` / `F138` by course), the boost ramps (`BOOST_RAMP_ASPHALT` Royal Raceway, `BOOST_RAMP_WOOD` DK's Jungle
//! Parkway: `player_controller.c` `func_8002A194`, `effects.c` `trigger_*_ramp_boost`), and the fluid / out-of-bounds
//! level (`collision.c` `func_802AAB4C`, `render_courses.c` `course_update_water`).
pub const AIRBORNE: u8 = 0;
pub const ASPHALT: u8 = 1;
pub const DIRT: u8 = 2;
pub const SAND: u8 = 3;
pub const STONE: u8 = 4;
pub const SNOW: u8 = 5;
pub const BRIDGE: u8 = 6;
pub const SAND_OFFROAD: u8 = 7;
pub const GRASS: u8 = 8;
pub const ICE: u8 = 9;
pub const WET_SAND: u8 = 0xA;
pub const SNOW_OFFROAD: u8 = 0xB;
pub const CLIFF: u8 = 0xC;
pub const DIRT_OFFROAD: u8 = 0xD;
pub const TRAIN_TRACK: u8 = 0xE;
pub const CAVE: u8 = 0xF;
pub const ROPE_BRIDGE: u8 = 0x10;
pub const WOOD_BRIDGE: u8 = 0x11;
pub const BOOST_RAMP_WOOD: u8 = 0xFC;
pub const OUT_OF_BOUNDS: u8 = 0xFD;
pub const BOOST_RAMP_ASPHALT: u8 = 0xFE;
pub const RAMP: u8 = 0xFF;

// Mario's rows. Index = surface id (0..=14); ids >= 0xF are skipped by the decomp (`surfaceType < 0xF`).
const SIDE_BACK: [f64; 15] = [0., 0., 0.3, 0.3, 0., 0.3, 0., 0.58, 0.58, 0., 0.58, 0.28, 0., 0.58, 0.]; // D_800E2A90
const SIDE_FRONT: [f64; 15] = [0., 0., 0., 0., 0., 0., 0., 0.38, 0.38, 0., 0.38, 0.1, 0., 0.38, 0.]; // D_800E2AB0
const THRUST_BACK: [f64; 15] = [0., 0., 0., 0.03, 0., 0., 0., 0.09, 0.09, 0., 0., 0.09, 0., 0.09, 0.09]; // D_800E2E90
const THRUST_FRONT: [f64; 15] = [0., 0., 0., 0., 0., 0., 0., 0.03, 0.03, 0., 0., 0.03, 0., 0.03, 0.03]; // D_800E2EB0
const BRAKE: [f64; 15] = [0., 0., 0.2, 0.2, 0., 0.4, 0.1, 0.2, 0.2, 0., 0., 0., 0., 0., 0.]; // D_800E3210

fn row(table: &[f64; 15], surface: u8) -> f64 {
    table.get(surface as usize).copied().unwrap_or(0.)
}

/// Tyre order everywhere: front left, front right, back left, back right (`player->tyres[]`).
pub const FRONT_LEFT: usize = 0;
pub const FRONT_RIGHT: usize = 1;
pub const BACK_LEFT: usize = 2;
pub const BACK_RIGHT: usize = 3;

/// Thrust lost (`func_80030150`): back right + back left + front right + front left (which reads the back table).
pub fn thrust_loss(t: &[u8; 4]) -> f64 {
    row(&THRUST_BACK, t[BACK_RIGHT])
        + row(&THRUST_BACK, t[BACK_LEFT])
        + row(&THRUST_FRONT, t[FRONT_RIGHT])
        + row(&THRUST_BACK, t[FRONT_LEFT])
}

/// Side grip lost (`func_8002FCA8`).
pub fn side_loss(t: &[u8; 4]) -> f64 {
    row(&SIDE_BACK, t[BACK_RIGHT]) + row(&SIDE_BACK, t[BACK_LEFT]) + row(&SIDE_FRONT, t[FRONT_RIGHT]) + row(&SIDE_FRONT, t[FRONT_LEFT])
}

/// Brake grip lost (`func_800323E4`): back tyres.
pub fn brake_loss(t: &[u8; 4]) -> f64 {
    row(&BRAKE, t[BACK_RIGHT]) + row(&BRAKE, t[BACK_LEFT])
}

/// Off the road and slowing the kart (the surfaces whose thrust row is non-zero).
pub fn slows(surface: u8) -> bool {
    row(&THRUST_BACK, surface) > 0.
}

/// What the tyre-dust particles are made of (`code_80057C60.c` `setup_tyre_particles`: the `func_8005DAD8` colour
/// index per surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dust {
    None,
    /// Smoke off tarmac, only when sliding / braking.
    Tarmac,
    Dirt,
    Grass,
    SandOffroad,
    Sand,
    WetSand,
    DirtOffroad,
    Snow,
}

pub fn dust(surface: u8) -> Dust {
    match surface {
        DIRT => Dust::Dirt,
        GRASS => Dust::Grass,
        SAND_OFFROAD => Dust::SandOffroad,
        SAND => Dust::Sand,
        WET_SAND => Dust::WetSand,
        DIRT_OFFROAD => Dust::DirtOffroad,
        SNOW | SNOW_OFFROAD => Dust::Snow,
        ASPHALT | STONE | BRIDGE => Dust::Tarmac,
        _ => Dust::None,
    }
}

/// RGB (0..=255) of the dust (`func_8005DAD8` colour table; courses with `GROUND_PARTICLE` dirt colour the dust
/// themselves).
pub fn dust_colour(surface: u8, course: &str) -> [u8; 3] {
    match dust(surface) {
        Dust::Dirt => match course {
            "choco_mountain" | "royal_raceway" => [0x8B, 0x5A, 0x2B],
            "kalimari_desert" => [0xD2, 0xB4, 0x8C],
            "moo_moo_farm" => [0x9C, 0x6B, 0x3C],
            "wario_stadium" => [0xB0, 0x90, 0x70],
            "yoshi_valley" => [0x80, 0x60, 0x40],
            "dks_jungle_parkway" => [0x7A, 0x5C, 0x38],
            _ => [0xA0, 0x80, 0x60],
        },
        Dust::SandOffroad => [0xE0, 0xC8, 0x90],
        Dust::Sand => [0xF0, 0xE0, 0xA8],
        Dust::WetSand => [0xB8, 0xA0, 0x70],
        Dust::DirtOffroad => [0xFF, 0xA5, 0x4F],
        Dust::Snow => [0xF0, 0xF8, 0xFF],
        Dust::Grass => [0xFF, 0xFF, 0xFF],
        Dust::Tarmac => [0xC8, 0xC8, 0xC8],
        Dust::None => [0, 0, 0],
    }
}

/// One of the decomp's per-course floor reaction functions (`func_8003F46C` dispatch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reaction {
    /// Restitution given to `func_8002A5F4` (bounce off the floor).
    pub restitution: f64,
    /// Share of the penetration that is pushed straight up.
    pub push_y: f64,
    /// Floors steeper than this (`orientationVector[1] <=`) are walls for the kart.
    pub steep_ny: f64,
    /// `surfaceType == CLIFF` counts as steep too (`func_8003E37C`).
    pub cliff_steep: bool,
    /// Steep floor: roll and slope multipliers, braking and the vertical-velocity divisor reset.
    pub steep_roll: f64,
    pub steep_slope: f64,
    pub steep_decel: f64,
    /// `decelerate` only above this speed (km/h), 0 = always.
    pub steep_decel_min_kmh: f64,
    /// `unk_DAC` after a steep floor (0.5 in `E048` / `E37C` / `F138`, 1.0 through `func_8003DC40` in `E6EC` / `E9EC` / `EE2C`).
    pub steep_dac: Option<f64>,
    /// Faster than 20 km/h: roll multiplier on asphalt / elsewhere, slope multiplier.
    pub fast_roll_asphalt: f64,
    pub fast_roll_other: f64,
    pub fast_slope: f64,
    /// Below 20 km/h: slope multiplier.
    pub slow_slope: f64,
}

impl Reaction {
    /// `func_8003E048`: Mario Raceway and every course without its own function.
    pub const STANDARD: Reaction = Reaction {
        restitution: 1.,
        push_y: 0.1,
        steep_ny: 0.8829,
        cliff_steep: false,
        steep_roll: 180.,
        steep_slope: 180.,
        steep_decel: 4.,
        steep_decel_min_kmh: 0.,
        steep_dac: Some(0.5),
        fast_roll_asphalt: 50.,
        fast_roll_other: 50.,
        fast_slope: 60.,
        slow_slope: 50.,
    };
    /// `func_8003E37C`: Choco Mountain and Koopa Troopa Beach.
    pub const CLIFFS: Reaction = Reaction {
        restitution: 0.5,
        push_y: 0.2,
        steep_ny: 0.7318,
        cliff_steep: true,
        steep_decel: 5.,
        steep_decel_min_kmh: 8.,
        fast_slope: 50.,
        ..Self::STANDARD
    };
    /// `func_8003E6EC`: Bowser's Castle.
    pub const CASTLE: Reaction = Reaction { restitution: 0.5, steep_dac: Some(1.), ..Self::STANDARD };
    /// `func_8003E9EC`: Luigi Raceway (the original port of this crate).
    pub const LUIGI: Reaction = Reaction {
        restitution: 1.2,
        steep_ny: 0.8357,
        steep_roll: 120.,
        steep_dac: Some(1.),
        fast_roll_asphalt: 5.,
        fast_roll_other: 40.,
        fast_slope: 40.,
        ..Self::STANDARD
    };
    /// `func_8003EE2C`: Wario Stadium.
    pub const STADIUM: Reaction = Reaction { restitution: 0.5, steep_ny: 0.8357, steep_roll: 120., steep_dac: Some(1.), ..Self::STANDARD };
    /// `func_8003F138`: DK's Jungle Parkway (the sideways / slope reaction is zeroed at the end of the function).
    pub const JUNGLE: Reaction = Reaction {
        restitution: 0.5,
        steep_ny: 0.8357,
        steep_roll: 0.,
        steep_slope: 0.,
        fast_roll_asphalt: 0.,
        fast_roll_other: 0.,
        fast_slope: 0.,
        slow_slope: 0.,
        ..Self::STANDARD
    };

    /// The reaction of a course by decomp directory name.
    pub fn for_course(dir: &str) -> Reaction {
        match dir {
            "choco_mountain" | "koopa_troopa_beach" => Self::CLIFFS,
            "bowsers_castle" => Self::CASTLE,
            "luigi_raceway" => Self::LUIGI,
            "wario_stadium" => Self::STADIUM,
            "dks_jungle_parkway" => Self::JUNGLE,
            _ => Self::STANDARD,
        }
    }
}

/// Boost ramps (`effects.c`). `kart_gravity` and the vertical-velocity divisor `unk_DAC` while the effect runs
/// (`player_controller.c` `func_8002AB70`), the boost power's target and rate while on the ramp or airborne.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    pub gravity: f64,
    pub dac: f64,
    pub power: f64,
    pub rate: f64,
}
pub const RAMP_ASPHALT: Ramp = Ramp { gravity: 3500., dac: 20., power: 400., rate: 0.01 };
pub const RAMP_WOOD: Ramp = Ramp { gravity: 1800., dac: 25., power: 300., rate: 0.1 };

pub fn ramp(surface: u8) -> Option<Ramp> {
    match surface {
        BOOST_RAMP_ASPHALT => Some(RAMP_ASPHALT),
        BOOST_RAMP_WOOD => Some(RAMP_WOOD),
        _ => None,
    }
}

/// The fluid / out-of-bounds level under `(x, z)` in course units (`func_802AAB4C`; `D_8015F8E4` is the course's
/// default, set in `render_courses.c`). `tide` is Koopa Troopa Beach's moving level (0 .. -20,
/// `course_update_water`), the others ignore it. `None` = the course has no such level.
pub fn water_level(dir: &str, x: f64, z: f64, tide: f64) -> Option<f64> {
    Some(match dir {
        // Lava pit between the fireball statues.
        "bowsers_castle" => {
            if (1549. ..=1859.).contains(&x) && (-1402. ..=-1102.).contains(&z) {
                20.
            } else {
                -50.
            }
        }
        "koopa_troopa_beach" => {
            if (67. ..=239.).contains(&x) && (2233. ..=2405.).contains(&z) {
                0.8
            } else {
                tide
            }
        }
        "choco_mountain" => -80.,
        "banshee_boardwalk" => -80.,
        "frappe_snowland" => -50.,
        "royal_raceway" => -60.,
        "sherbet_land" => -18.,
        "rainbow_road" => 0.,
        "dks_jungle_parkway" => -475.,
        "skyscraper" => -480.,
        "big_donut" => 100.,
        _ => return None,
    })
}

/// Koopa Troopa Beach's tide: `course_update_water`, one 30 Hz step. `speed` starts at -0.1 (`func_802969F8`).
pub fn tide_step(level: &mut f64, speed: &mut f64) {
    if *speed < 0. {
        if *level < -20. {
            *speed = -*speed;
        }
    } else if *level > 0. {
        *speed = -*speed;
    }
    *level += *speed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mario_grass_matches_the_decomp_sums() {
        // All four tyres on grass: 0.09 + 0.09 + 0.03 + 0.09 thrust, 0.58 * 2 + 0.38 * 2 side, 0.2 * 2 brake.
        let t = [GRASS; 4];
        assert!((thrust_loss(&t) - 0.30).abs() < 1e-12);
        assert!((side_loss(&t) - 1.92).abs() < 1e-12);
        assert!((brake_loss(&t) - 0.40).abs() < 1e-12);
        assert_eq!(thrust_loss(&[ASPHALT; 4]), 0.);
    }

    #[test]
    fn dirt_roads_only_cost_side_grip_and_snow_costs_the_brake() {
        let dirt = [DIRT; 4];
        assert_eq!(thrust_loss(&dirt), 0.);
        assert!((side_loss(&dirt) - 0.6).abs() < 1e-12);
        assert!((brake_loss(&[SNOW; 2 + 2]) - 0.8).abs() < 1e-12);
        assert!(!slows(DIRT) && !slows(SNOW) && !slows(WET_SAND));
        assert!(slows(GRASS) && slows(SAND_OFFROAD) && slows(SNOW_OFFROAD) && slows(DIRT_OFFROAD) && slows(TRAIN_TRACK));
        // Sand is the light colour on Koopa Troopa Beach: a little thrust, plain side grip.
        assert!((thrust_loss(&[SAND; 4]) - 0.09).abs() < 1e-12);
        // Rope bridges and the special ids are outside the tables.
        assert_eq!(side_loss(&[ROPE_BRIDGE; 4]), 0.);
        assert_eq!(side_loss(&[BOOST_RAMP_WOOD; 4]), 0.);
    }

    #[test]
    fn every_course_has_one_of_the_six_reactions() {
        assert_eq!(Reaction::for_course("luigi_raceway"), Reaction::LUIGI);
        assert_eq!(Reaction::for_course("koopa_troopa_beach"), Reaction::CLIFFS);
        assert_eq!(Reaction::for_course("mario_raceway"), Reaction::STANDARD);
        assert_eq!(Reaction::for_course("yoshi_valley"), Reaction::STANDARD);
        assert!(Reaction::CLIFFS.cliff_steep && !Reaction::STANDARD.cliff_steep);
    }

    #[test]
    fn ramps_and_levels() {
        assert_eq!(ramp(BOOST_RAMP_ASPHALT), Some(RAMP_ASPHALT));
        assert_eq!(ramp(BOOST_RAMP_WOOD), Some(RAMP_WOOD));
        assert_eq!(ramp(ASPHALT), None);
        // Bowser's Castle: the lava pit is a raised fluid level, the rest of the course falls away at -50.
        assert_eq!(water_level("bowsers_castle", 1700., -1200., 0.), Some(20.));
        assert_eq!(water_level("bowsers_castle", 0., 0., 0.), Some(-50.));
        assert_eq!(water_level("luigi_raceway", 0., 0., 0.), None);
        // The tide swings between 0 and -20 at 0.1 per step.
        let (mut level, mut speed) = (0., -0.1);
        let (mut lo, mut hi) = (0f64, -100f64);
        for _ in 0..2000 {
            tide_step(&mut level, &mut speed);
            lo = lo.min(level);
            hi = hi.max(level);
        }
        assert!(lo < -19.9 && lo > -20.3, "{lo}");
        assert!(hi <= 0.2, "{hi}");
    }
}
