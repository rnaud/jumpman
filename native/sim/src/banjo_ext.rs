//! Banjo-Kazooie's extended moveset on top of the SM64 engine (see `native/BANJO_CONTROLLER.md`).
//!
//! `characters::step_banjo` keeps the original nine moves (jump, Feathery Flap, Flap Flip, Claw,
//! Roll, Beak Barge, Rat-a-tat Rap ...). This module adds the rest of the `bs*` state machine from
//! `src/core2/bs/*.c` of the decomp as extra `action_arg` codes on top of SM64 actions:
//!
//! | move | decomp | SM64 action + arg |
//! |------|--------|-------------------|
//! | Talon Trot | `bTrot.c` | `ACT_WALKING` / `ACT_JUMP` + `BANJO_TROT` / `BANJO_TROT_JUMP` |
//! | Beak Buster | `bbuster.c` | `ACT_GROUND_POUND`, `ACT_GROUND_POUND_LAND`, `ACT_FREEFALL` + `BANJO_BUSTER*` |
//! | Shock Spring Jump | `bShock.c` | `ACT_IDLE` / `ACT_JUMP` + `BANJO_SHOCK*` |
//! | Blue egg (forward / backward) | `bEggHead.c`, `bEggAss.c`, `code_CC1E0.c` | `ACT_IDLE` + `BANJO_EGG_HEAD/ASS` |
//! | Wonderwing | `bWhirl.c` | `ACT_WALKING` / `ACT_JUMP` + `BANJO_WONDER*` |
//! | Flight, Beak Bomb | `bFly.c` | `ACT_FREEFALL` / `ACT_DIVE` + `BANJO_FLY*`, `BANJO_BOMB` |
//!
//! Units follow `characters.rs`: Rare units per second are converted to 30 Hz frames at Banjo's
//! 1.25 model scale (`speed()` / `accel()`). Inputs: the C buttons and R are real N64 masks
//! (`state_consts`); the host sets them from keys, touch buttons and the D-pad.
use crate::characters::*;
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub const BANJO_TROT: f64 = 0x67 as f64;
pub const BANJO_TROT_JUMP: f64 = 0x68 as f64;
pub const BANJO_BUSTER: f64 = 0x69 as f64;
pub const BANJO_BUSTER_HOP: f64 = 0x6a as f64;
pub const BANJO_SHOCK_CHARGE: f64 = 0x6b as f64;
pub const BANJO_SHOCK_JUMP: f64 = 0x6c as f64;
pub const BANJO_EGG_HEAD: f64 = 0x6d as f64;
pub const BANJO_EGG_ASS: f64 = 0x6e as f64;
pub const BANJO_WONDER: f64 = 0x6f as f64;
pub const BANJO_WONDER_JUMP: f64 = 0x70 as f64;
pub const BANJO_FLY_ENTER: f64 = 0x71 as f64;
pub const BANJO_FLY: f64 = 0x72 as f64;
pub const BANJO_BOMB: f64 = 0x73 as f64;
pub const BANJO_BOMB_END: f64 = 0x74 as f64;
pub const BANJO_CLIMB: f64 = 0x75 as f64;
pub const BANJO_CLIMB_EXIT: f64 = 0x76 as f64;
pub const BANJO_LAST: f64 = BANJO_CLIMB_EXIT;

// bs/climb.c (func_802AB5C0 / func_802AB654): vertical target 100..300 u/s over the stick zone 0.03..1.0.
const CLIMB_MIN: f64 = speed(100.0);
const CLIMB_MAX: f64 = speed(300.0);
/// `baphysics_set_acceleration(10.0)` in climb physics: how quickly the vertical speed follows the stick.
const CLIMB_ACCEL: f64 = speed(45.0);
/// `climb_release`: 0.6 s before the same pole can be grabbed again.
const CLIMB_REGRAB: f64 = 18.0;
/// `ch/climbBase.c`: the pole grabs within `radius + 25` of its axis.
const CLIMB_REACH: f64 = 25.0 * 1.25;
/// bsjump_climbExit: 60 u/s forward, 710 u/s up, -1350 gravity.
const CLIMB_EXIT_FORWARD: f64 = speed(60.0);
const CLIMB_JUMP_BACK: f64 = speed(150.0);

/// `ITEM_17_AIR` maximum (`code_BEF20.c`, US 1.0: `VER_SELECT(3600, ...)`): 6 bubbles of 600.
pub const AIR_MAX: i32 = 3600;
/// `item_adjustByDiffWithHud(ITEM_17_AIR, (s32)(dt * FRAMERATE * 1.1))` at the game's 2/60 s step.
pub const AIR_DRAIN: i32 = 2;
/// Polluted water (Rusty Bucket Bay, Click Clock Wood winter) drains `* 2.1` while submerged.
pub const AIR_DRAIN_POLLUTED: i32 = 4;
/// Refill on the surface: `(s32)(dt * FRAMERATE * 100.0 * 1.1)` per 30 Hz frame.
pub const AIR_REFILL: i32 = 220;
/// `D_80385FEC = 2.0` s: refill waits this long after leaving polluted water.
pub const POLLUTION_DELAY: f64 = 60.0;
/// `fxairscore_count_to_time(1)`: one bubble of the meter.
pub const AIR_BUBBLE: i32 = 600;

// bTrot.c .data
const TROT_MIN: f64 = speed(30.0);
const TROT_MAX: f64 = speed(700.0);
const TURBO_MIN: f64 = speed(80.0);
const TURBO_MAX: f64 = speed(1000.0);
const TROT_JUMP_VELOCITY: f64 = speed(693.5);
const TROT_GRAVITY: f64 = accel(1200.0);
// bWhirl.c
const WONDER_MIN: f64 = speed(80.0);
const WONDER_MAX: f64 = speed(425.0);
const WONDER_JUMP_VELOCITY: f64 = speed(693.5);
const WONDER_GRAVITY: f64 = accel(1200.0);
/// `func_802AA58C`: one gold feather every 2.0 s.
const WONDER_FEATHER_FRAMES: f64 = 60.0;
// bbuster.c
const BUSTER_WINDUP: f64 = 11.0; // anim 0.35 of 1.02 s = 0.357 s
const BUSTER_DIVE: f64 = speed(2300.0);
const BUSTER_DIVE_TERMINAL: f64 = -150.0; // decomp: -5000 u/s; capped so a 30 Hz quarter step cannot tunnel
const BUSTER_GRAVITY: f64 = accel(20000.0);
const BUSTER_FREEZE: f64 = 3.0; // 0.09 s on impact
const BUSTER_HOP: f64 = speed(730.0);
const BUSTER_HOP_GRAVITY: f64 = accel(2110.0);
// bShock.c
const SHOCK_VELOCITY: f64 = speed(1250.0);
const SHOCK_GRAVITY: f64 = accel(1200.0);
/// Anim 0.3637 of the 2.8 s charge: the spring only fires if A is still held then.
const SHOCK_CHARGE_FRAMES: f64 = 31.0;
// bEggHead.c / bEggAss.c / code_CC1E0.c
const EGG_SPEED: f64 = speed(800.0);
const EGG_LIFETIME: f64 = 60.0; // fxegg_head_update: 2.0 s
const EGG_FIRE_FRAME: f64 = 14.0; // anim 0.4704 of 1.0 s
const EGG_REPEAT_FRAMES: f64 = 6.0; // 0.3878 -> 0.5919 loop
const EGG_END_FRAME: f64 = 30.0;
const EGG_TURN: f64 = 0x600 as f64; // yaw_setVelocityBounded(350 deg/s)
const EGG_ASS_BACK: f64 = speed(200.0);
const EGG_ASS_UP: f64 = speed(710.0);
const EGG_ASS_GRAVITY: f64 = accel(2200.0);
const EGG_ASS_LIFETIME: f64 = 120.0;
pub const EGG_MAX: i32 = 100;
// bFly.c
const FLY_LIFT: f64 = speed(1600.0);
const FLY_ENTER_GRAVITY: f64 = accel(1200.0);
const FLY_LIFT_FRAME: f64 = 10.0; // anim 0.2416 of 1.4 s ~ 0.34 s
const FLY_SPEED: f64 = speed(600.0);
const FLY_GRAVITY: f64 = accel(300.0);
const FLY_FLAP_VELOCITY: f64 = speed(400.0);
const FLY_FLAP_FRAMES: f64 = 10.0; // baflap_add(1.0)
const FLY_YAW: f64 = 3.0 * 182.04; // yaw_range 3 deg per update (6 with R)
const FLY_PITCH_MAX: f64 = 80.0;
const BOMB_WINDUP: f64 = 21.0; // anim 0.6905 of 1.0 s
const BOMB_SPEED: f64 = speed(4200.0);
const BOMB_RANGE: f64 = 4000.0; // 16,000,000 squared distance
const BOMB_END_FRAMES: f64 = 11.0; // 0.35 s
const BOMB_COOLDOWN: f64 = 30.0; // D_8037D320 = 1.0 s after taking flight

/// `enum ability_e` (`core2/abilityprogress.h`): the bit index of each learnable move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ability {
    Barge = 0,
    BeakBomb = 1,
    BeakBuster = 2,
    CameraControl = 3,
    ClawSwipe = 4,
    Climb = 5,
    Eggs = 6,
    FeatheryFlap = 7,
    FlapFlip = 8,
    Flight = 9,
    HoldAJumpHigher = 10,
    RatATatRap = 11,
    Roll = 12,
    ShockJump = 13,
    WadingBoots = 14,
    Dive = 15,
    TalonTrot = 16,
    TurboTalon = 17,
    Wonderwing = 18,
    FirstNoteDoor = 19,
}

impl Ability {
    pub const COUNT: u32 = 20;
    pub const ALL: [Ability; 20] = [
        Ability::Barge, Ability::BeakBomb, Ability::BeakBuster, Ability::CameraControl, Ability::ClawSwipe, Ability::Climb,
        Ability::Eggs, Ability::FeatheryFlap, Ability::FlapFlip, Ability::Flight, Ability::HoldAJumpHigher, Ability::RatATatRap,
        Ability::Roll, Ability::ShockJump, Ability::WadingBoots, Ability::Dive, Ability::TalonTrot, Ability::TurboTalon,
        Ability::Wonderwing, Ability::FirstNoteDoor,
    ];
    pub fn bit(self) -> u32 { 1 << (self as u32) }
    pub fn label(self) -> &'static str {
        match self {
            Ability::Barge => "BEAK BARGE",
            Ability::BeakBomb => "BEAK BOMB",
            Ability::BeakBuster => "BEAK BUSTER",
            Ability::CameraControl => "CAMERA CONTROL",
            Ability::ClawSwipe => "CLAW SWIPE",
            Ability::Climb => "CLIMB",
            Ability::Eggs => "EGG FIRING",
            Ability::FeatheryFlap => "FEATHERY FLAP",
            Ability::FlapFlip => "FLAP FLIP",
            Ability::Flight => "FLIGHT",
            Ability::HoldAJumpHigher => "HIGH JUMP",
            Ability::RatATatRap => "RAT-A-TAT RAP",
            Ability::Roll => "ROLL",
            Ability::ShockJump => "SHOCK SPRING JUMP",
            Ability::WadingBoots => "WADING BOOTS",
            Ability::Dive => "DIVE",
            Ability::TalonTrot => "TALON TROT",
            Ability::TurboTalon => "TURBO TALON TRAINERS",
            Ability::Wonderwing => "WONDERWING",
            Ability::FirstNoteDoor => "NOTE DOORS",
        }
    }
}

/// Every ability learned: the default, so every course stays a sandbox until a host starts the progression.
pub const ALL_ABILITIES: u32 = (1 << Ability::COUNT) - 1;

/// What a Bottles molehill teaches. `sm` = the Spiral Mountain tutorial molehills (`ACTOR_12B_TUTORIAL_BOTTLES`,
/// `chSmBottlesDialogTable`, id 1..8, whose lessons also grant the sibling moves of the tutorial: the attack lesson
/// Claw / Roll / Rat-a-tat, the jump lesson Feathery Flap / Flap Flip / holding A), otherwise `ACTOR_37A_BOTTLES`
/// (`moleTable`, id 9..0x12).
pub fn mole_abilities(sm: bool, id: u32) -> &'static [Ability] {
    if sm {
        match id {
            2 => &[Ability::CameraControl],
            3 => &[Ability::Dive],
            4 => &[Ability::ClawSwipe, Ability::Roll, Ability::RatATatRap],
            5 => &[Ability::Barge],
            6 => &[Ability::HoldAJumpHigher, Ability::FeatheryFlap, Ability::FlapFlip],
            7 => &[Ability::Climb],
            _ => &[],
        }
    } else {
        match id {
            9 => &[Ability::BeakBomb],
            0xA => &[Ability::Eggs],
            0xB => &[Ability::BeakBuster],
            0xC => &[Ability::TalonTrot],
            0xD => &[Ability::ShockJump],
            0xE => &[Ability::Flight],
            0xF => &[Ability::Wonderwing],
            0x10 => &[Ability::WadingBoots],
            0x11 => &[Ability::TurboTalon],
            0x12 => &[Ability::FirstNoteDoor],
            _ => &[],
        }
    }
}

/// A Bottles molehill placed by the map setup (`bkMoles`).
#[derive(Clone, Debug)]
pub struct Molehill {
    pub pos: Vec3,
    pub id: u32,
    pub sm: bool,
}

/// A climbable pole (`ch/climbBase.c`): `bottom` and `top` are the base actor and the 0x27 / 0x28 node.
#[derive(Clone, Debug)]
pub struct ClimbPole {
    pub bottom: Vec3,
    pub top: Vec3,
    pub radius: f64,
    /// 1 = ledge (`ACTOR_27`: the climber stops), 2 = exit (`ACTOR_28`: climbing past the top hops onto it).
    pub kind: u8,
}

/// A note door of Gruntilda's lair (`ACTOR_203_NOTE_DOOR`, `bkNoteDoors`): `index` 1..12, `yaw` in degrees.
#[derive(Clone, Debug)]
pub struct NoteDoor {
    pub pos: Vec3,
    pub yaw: f64,
    pub index: u32,
}

/// A pressable floor switch (`ACTOR_13F_GV_SUN_SWITCH`, `ACTOR_1E2/16C/16A/168` season switches, `bkSwitches`): `flag` is the bit of
/// [`BanjoExt::switches`] it sets (see [`switch_flag`]). Banjo presses it with a ground pound or a Beak Buster landing on it.
#[derive(Clone, Debug)]
pub struct SwitchActor {
    pub pos: Vec3,
    pub flag: u32,
}

/// A door that a switch opens (`ACTOR_140_GV_SUN_DOOR`, `ACTOR_1E3/16D/16B/169` season doors, `bkSwitchDoors`); closed it is the same
/// slab as a note door.
#[derive(Clone, Debug)]
pub struct SwitchDoor {
    pub pos: Vec3,
    pub yaw: f64,
    pub flag: u32,
}

/// Switch / door actor id -> flag bit: 0 Gobi's sun door, 1 spring, 2 summer, 3 autumn, 4 winter season door of Click Clock Wood.
/// The season switches sit in the previous season's map (spring switch in the hub, summer's in spring, ...).
pub fn switch_flag(actor: i64) -> Option<(u32, bool)> {
    Some(match actor {
        0x13F => (0, true),
        0x140 => (0, false),
        0x1E2 => (1, true),
        0x1E3 => (1, false),
        0x16C => (2, true),
        0x16D => (2, false),
        0x16A => (3, true),
        0x16B => (3, false),
        0x168 => (4, true),
        0x169 => (4, false),
        _ => return None,
    })
}

/// The door model is not extracted: a closed door blocks a slab this big (`chnotedoor_update` opens at 290 units).
pub const DOOR_HALF_WIDTH: f64 = 330.0;
pub const DOOR_HEIGHT: f64 = 700.0;
pub const DOOR_THICKNESS: f64 = 40.0;
pub const DOOR_OPEN_RANGE: f64 = 290.0;

/// Which world a `bk_*` course is (1..9, the order of `PICTURE_INFO`): the lair door that leads there is locked by that
/// world's jiggy puzzle.
pub fn puzzle_world(course: &str) -> Option<u32> {
    Some(match course {
        "bk_mumbos_mountain" => 1,
        "bk_treasure_trove_cove" => 2,
        "bk_clankers_cavern" => 3,
        "bk_bubblegloop_swamp" => 4,
        "bk_freezeezy_peak" => 5,
        "bk_gobis_valley" => 6,
        "bk_mad_monster_mansion" => 7,
        "bk_rusty_bucket_bay" => 8,
        c if c.starts_with("bk_click_clock_wood") => 9,
        _ => return None,
    })
}

/// Jiggy cost of each world's puzzle picture (`PICTURE_INFO` of `lair/jigsawpicture.c`, worlds 1..9).
pub const PUZZLE_COST: [i32; 9] = [1, 2, 5, 7, 8, 9, 10, 12, 15];
/// Notes needed by each note door (`note_door_cost_data`, `lair/lairspawnqueue.c`).
pub const NOTE_DOOR_COST: [i32; 12] = [50, 180, 260, 350, 450, 640, 765, 810, 828, 846, 864, 882];

/// The ability pads of BK, selectable in this port (no pads exist in the SM64 courses): each call of
/// [`BanjoExt::cycle_power`] equips the next one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Power {
    #[default]
    None,
    /// Shock Spring pad: A on the ground charges a 650-unit jump.
    Shock,
    /// Flight pad: A on the ground takes off; A flaps (red feathers), B Beak Bombs.
    Flight,
    /// Turbo Trainers: Talon Trot at 1000 u/s that no longer needs Z held, for 30 s.
    Turbo,
    /// Wading Boots: shorter hops (the mud/lava immunity is not ported).
    Wading,
}

impl Power {
    pub fn next(self) -> Self {
        match self {
            Power::None => Power::Shock,
            Power::Shock => Power::Flight,
            Power::Flight => Power::Turbo,
            Power::Turbo => Power::Wading,
            Power::Wading => Power::None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Power::None => "NO PAD",
            Power::Shock => "SHOCK SPRING",
            Power::Flight => "FLIGHT",
            Power::Turbo => "TURBO TRAINERS",
            Power::Wading => "WADING BOOTS",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EggKind {
    /// `fxegg_head`: shot forward at 800 u/s, shatters on contact or after 2 s.
    Head,
    /// `fxegg_ass`: dropped backwards, lobbed up; can be picked up again.
    Ass,
}

#[derive(Clone, Debug)]
pub struct Egg {
    pub kind: EggKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind {
    Note,
    Egg,
    RedFeather,
    GoldFeather,
    Honeycomb,
    Jiggy,
    /// `ACTOR_2D_MUMBO_TOKEN` (`ch/mumbotoken.c`, `ITEM_1C_MUMBO_TOKEN`).
    MumboToken,
}

impl PickupKind {
    pub fn label(self) -> &'static str {
        match self {
            PickupKind::Note => "NOTE",
            PickupKind::Egg => "EGGS",
            PickupKind::RedFeather => "RED FEATHERS",
            PickupKind::GoldFeather => "GOLD FEATHER",
            PickupKind::Honeycomb => "HONEYCOMB",
            PickupKind::Jiggy => "JIGGY",
            PickupKind::MumboToken => "MUMBO TOKEN",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BkPickup {
    pub kind: PickupKind,
    pub pos: Vec3,
    pub collected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BanjoEvent {
    EggFired { pos: Vec3, backwards: bool },
    EggShattered { pos: Vec3, hit: bool },
    Collected(PickupKind),
    BeakBuster { pos: Vec3 },
    ShockSpring,
    TakeOff,
    BeakBomb,
    WonderwingOn,
    WonderwingOff,
    OutOf(PickupKind),
    /// `SFX_AF_BANJO_CATCHING_BREATH`: surfaced after being under water (`bsswim_idle_init`).
    CatchBreath { air: i32 },
    /// The air meter reached zero (`BS_INTR_11_DROWN` -> `BS_54_SWIM_DIE`).
    Drowning,
    /// A molehill taught Banjo this move.
    Learned(Ability),
    /// Banjo grabbed a climbable pole / let go of it.
    ClimbGrab,
    ClimbRelease,
    /// A step of the climb animation (the sniffle / ring of `bsclimb_move_update`), every 14 frames while moving.
    ClimbStep,
    /// A note door opened (door index 1..12).
    NoteDoorOpened(u32),
    SwitchPressed(u32),
    /// A world's jiggy puzzle was completed (world index 1..9).
    PuzzleComplete(u32),
}

/// Everything BK-specific that the SM64 `MarioState` cannot hold.
#[derive(Clone, Debug)]
pub struct BanjoExt {
    // Item counters (`ITEM_*` of the decomp). Starting loadout is generous: every course is a sandbox.
    pub eggs: i32,
    pub red_feathers: i32,
    pub gold_feathers: i32,
    pub notes: i32,
    pub jiggies: i32,
    /// `ITEM_1C_MUMBO_TOKEN`.
    pub mumbo_tokens: i32,
    /// Learned moves (`enum ability_e` bits). Everything by default; [`BanjoExt::begin_progression`] resets it.
    pub abilities: u32,
    /// A progression game is running: moves, puzzle and note locks are enforced.
    pub progression: bool,
    pub moles: Vec<Molehill>,
    pub climbs: Vec<ClimbPole>,
    /// Pole being climbed (index into `climbs`) and the frames before it can be grabbed again.
    pub climb_pole: usize,
    pub climb_regrab: f64,
    /// Jiggies spent on world puzzles (bit i = world i+1 complete).
    pub puzzles: u32,
    /// Note doors opened (bit i = door i+1).
    pub note_doors: u32,
    /// The lair's note doors in this map and the side of each one Banjo was last seen on.
    pub door_actors: Vec<NoteDoor>,
    pub door_side: Vec<f64>,
    /// Switches pressed so far (bit = [`switch_flag`]); carried across maps.
    pub switches: u32,
    pub switch_actors: Vec<SwitchActor>,
    pub switch_doors: Vec<SwitchDoor>,
    pub switch_door_side: Vec<f64>,
    pub power: Power,
    /// Frames of Turbo Trainers left (`STATE_TIMER_3_TURBO_TALON`).
    pub turbo_frames: f64,
    pub eggs_in_flight: Vec<Egg>,
    pub pickups: Vec<BkPickup>,
    pub events: Vec<BanjoEvent>,
    // Move scratch state.
    /// The trot/wonderwing was entered without Z held (touch): stays on until Z or the button is pressed again.
    pub latched: bool,
    pub queued_eggs: i32,
    pub fly_pitch: f64,
    pub fly_flap: f64,
    pub fly_cooldown: f64,
    pub bomb_start: Vec3,
    pub bomb_dir: Vec3,
    pub wonder_timer: f64,
    pub shock_armed: bool,
    pub buster_stage: f64,
    /// Air meter (`ITEM_17_AIR`, 0..=[`AIR_MAX`]).
    pub air: i32,
    /// The current water is polluted / freezing (RBB, CCW winter): set by the host per map.
    pub polluted_water: bool,
    /// Frames left before the meter refills after polluted water (`D_80385FEC`).
    pub pollution_timer: f64,
    /// Was Banjo under water last frame (for the catch-breath event).
    pub was_underwater: bool,
    /// The sandcastle's infinite-air flag (`VOLATILE_FLAG_96_SANDCASTLE_INFINITE_AIR`).
    pub infinite_air: bool,
    /// Set by `MarioRuntime::step` while the character is Banjo (the SM64 breath drain is then off).
    pub active: bool,
}

impl Default for BanjoExt {
    fn default() -> Self {
        Self {
            eggs: 50,
            red_feathers: 50,
            gold_feathers: 10,
            notes: 0,
            jiggies: 0,
            mumbo_tokens: 0,
            abilities: ALL_ABILITIES,
            progression: false,
            moles: vec![],
            climbs: vec![],
            climb_pole: 0,
            climb_regrab: 0.0,
            puzzles: 0,
            note_doors: 0,
            door_actors: vec![],
            door_side: vec![],
            switches: 0,
            switch_actors: vec![],
            switch_doors: vec![],
            switch_door_side: vec![],
            power: Power::None,
            turbo_frames: 0.0,
            eggs_in_flight: vec![],
            pickups: vec![],
            events: vec![],
            latched: false,
            queued_eggs: 0,
            fly_pitch: 0.0,
            fly_flap: 0.0,
            fly_cooldown: 0.0,
            bomb_start: [0.0; 3],
            bomb_dir: [0.0, 0.0, 1.0],
            wonder_timer: 0.0,
            shock_armed: false,
            buster_stage: 0.0,
            air: AIR_MAX,
            polluted_water: false,
            pollution_timer: 0.0,
            was_underwater: false,
            infinite_air: false,
            active: false,
        }
    }
}

impl BanjoExt {
    pub fn on_respawn(&mut self) {
        self.eggs_in_flight.clear();
        self.latched = false;
        self.queued_eggs = 0;
        self.turbo_frames = 0.0;
        self.air = AIR_MAX;
        self.pollution_timer = 0.0;
        self.was_underwater = false;
    }

    /// Number of full-or-partial bubbles shown by `fxairscore` (`fxairscore_time_to_count`).
    pub fn air_bubbles(&self) -> i32 { (self.air + AIR_BUBBLE - 1) / AIR_BUBBLE }

    /// What survives a map change (`transitionToMap` keeps the item counters and file progress): the counters, learned
    /// moves and lock progress, not the projectiles and the per-map actor lists.
    pub fn carried(&self) -> BanjoExt {
        BanjoExt {
            eggs_in_flight: vec![],
            pickups: vec![],
            events: vec![],
            moles: vec![],
            climbs: vec![],
            door_actors: vec![],
            door_side: vec![],
            switch_actors: vec![],
            switch_doors: vec![],
            switch_door_side: vec![],
            latched: false,
            queued_eggs: 0,
            turbo_frames: self.turbo_frames,
            air: AIR_MAX,
            was_underwater: false,
            polluted_water: false,
            pollution_timer: 0.0,
            ..self.clone()
        }
    }

    pub fn has(&self, a: Ability) -> bool { self.abilities & a.bit() != 0 }

    pub fn learn(&mut self, a: Ability) {
        if !self.has(a) {
            self.abilities |= a.bit();
            self.events.push(BanjoEvent::Learned(a));
        }
    }

    /// Start a new game's ability progression: only the basic jump and the water moves are known, the rest comes from
    /// Bottles' molehills (`bkMoles`). Pads need their move too (see [`BanjoExt::cycle_power`]).
    pub fn begin_progression(&mut self) {
        self.abilities = 0;
        self.progression = true;
        self.eggs = 0;
        self.notes = 0;
        self.jiggies = 0;
        self.mumbo_tokens = 0;
        self.puzzles = 0;
        self.note_doors = 0;
        self.switches = 0;
    }

    pub fn in_progression(&self) -> bool { self.progression }

    /// Jiggies in stock (collected minus those placed in puzzle pictures).
    pub fn jiggy_stock(&self) -> i32 {
        self.jiggies - PUZZLE_COST.iter().enumerate().filter(|(i, _)| self.puzzles & (1 << i) != 0).map(|(_, c)| *c).sum::<i32>()
    }

    pub fn puzzle_complete(&self, world: u32) -> bool { world >= 1 && world <= 9 && self.puzzles & (1 << (world - 1)) != 0 }

    /// Try to complete a world's picture with the jiggies in stock (`jigsawPicture_*`: the podium takes `cost`
    /// jiggies). True when the picture is (now) complete.
    pub fn try_complete_puzzle(&mut self, world: u32) -> bool {
        if self.puzzle_complete(world) { return true; }
        if world < 1 || world > 9 { return false; }
        if self.jiggy_stock() >= PUZZLE_COST[(world - 1) as usize] {
            self.puzzles |= 1 << (world - 1);
            self.events.push(BanjoEvent::PuzzleComplete(world));
            true
        } else {
            false
        }
    }

    pub fn note_door_open(&self, index: u32) -> bool { index >= 1 && index <= 12 && self.note_doors & (1 << (index - 1)) != 0 }

    /// `chnotedoor_update`: with the first-note-door lesson learned and enough notes the door opens. Returns true if open.
    pub fn try_open_note_door(&mut self, index: u32) -> bool {
        if self.note_door_open(index) { return true; }
        if index < 1 || index > 12 { return false; }
        if self.has(Ability::FirstNoteDoor) && self.notes >= NOTE_DOOR_COST[(index - 1) as usize] {
            self.note_doors |= 1 << (index - 1);
            self.events.push(BanjoEvent::NoteDoorOpened(index));
            true
        } else {
            false
        }
    }

    pub fn cycle_power(&mut self) {
        let mut next = self.power.next();
        // Skip the pads whose move has not been learned.
        for _ in 0..5 {
            let ok = match next {
                Power::None => true,
                Power::Shock => self.has(Ability::ShockJump),
                Power::Flight => self.has(Ability::Flight),
                Power::Turbo => self.has(Ability::TurboTalon),
                Power::Wading => self.has(Ability::WadingBoots),
            };
            if ok { break; }
            next = next.next();
        }
        self.power = next;
        if self.power == Power::Turbo {
            self.turbo_frames = 30.0 * 30.0;
        } else {
            self.turbo_frames = 0.0;
        }
    }

    pub fn drop_pickup(&mut self, kind: PickupKind, pos: Vec3) {
        self.pickups.push(BkPickup { kind, pos, collected: false });
    }

    fn take(&mut self, kind: PickupKind) {
        match kind {
            PickupKind::Note => self.notes += 1,
            PickupKind::Egg => self.eggs = (self.eggs + 5).min(EGG_MAX),
            PickupKind::RedFeather => self.red_feathers = (self.red_feathers + 5).min(100),
            PickupKind::GoldFeather => self.gold_feathers = (self.gold_feathers + 1).min(10),
            PickupKind::Jiggy => self.jiggies += 1,
            PickupKind::MumboToken => self.mumbo_tokens += 1,
            PickupKind::Honeycomb => {}
        }
        self.events.push(BanjoEvent::Collected(kind));
    }
}

/// Rusty Bucket Bay and Click Clock Wood's winter are the polluted / freezing waters of
/// `code_BEF20.c` (`is_in_polluted_or_winter_water`). `course` is a Jumpman course id.
pub fn polluted_course(course: &str) -> bool {
    course == "bk_rusty_bucket_bay" || course.starts_with("bk_rbb_") || course == "bk_click_clock_wood_winter" || course.starts_with("bk_ccw_winter")
}

/// Is Banjo's head under water (`BSWATERGROUP_2_UNDERWATER`)? Same threshold as the SM64 breath rule.
pub fn underwater(m: &MarioRuntime) -> bool {
    m.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED && m.pos[1] < water_level(m) - 140.0
}

/// On the water surface (`BSWATERGROUP_1_SURFACE`): swimming with the head out.
pub fn on_water_surface(m: &MarioRuntime) -> bool {
    m.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED && !underwater(m)
}

fn water_level(m: &MarioRuntime) -> f64 {
    if crate::actions::water::faithful() { m.water.level } else { m.world.water_level(m.pos[0], m.pos[2]) }
}

/// The air meter (`code_BEF20.c`, the `ITEM_17_AIR` block of the per-frame item update). Runs every
/// frame while the character is Banjo, also in the submerged actions where `step_banjo` is skipped.
/// Draining on the surface only happens in polluted water; at zero the meter drowns Banjo through
/// the SM64 drowning actions (health below 0x100).
pub fn water_tick(m: &mut MarioRuntime) {
    if m.action == ACT_DROWNING || m.action == ACT_WATER_DEATH { return; }
    let under = underwater(m);
    let surface = on_water_surface(m);
    let polluted = m.banjo.polluted_water;
    if polluted && (under || surface) {
        m.banjo.pollution_timer = POLLUTION_DELAY;
    } else {
        m.banjo.pollution_timer = (m.banjo.pollution_timer - 1.0).max(0.0);
    }
    let drain = if (!polluted && under) || (polluted && surface) {
        AIR_DRAIN
    } else if polluted && under {
        AIR_DRAIN_POLLUTED
    } else {
        0
    };
    if drain > 0 {
        if !m.banjo.infinite_air { m.banjo.air = (m.banjo.air - drain).max(0); }
    } else if m.banjo.pollution_timer == 0.0 && m.banjo.air < AIR_MAX {
        m.banjo.air = (m.banjo.air + AIR_REFILL).min(AIR_MAX);
    }
    // Catching breath on the way up.
    if m.banjo.was_underwater && !under && m.banjo.air < AIR_MAX {
        let air = m.banjo.air;
        m.banjo.events.push(BanjoEvent::CatchBreath { air });
    }
    m.banjo.was_underwater = under;
    if m.banjo.air == 0 && (under || (polluted && surface)) && m.action & ACT_FLAG_INTANGIBLE == 0 {
        if m.state.health >= 0x100 as f64 {
            m.state.health = 0xff as f64;
            m.banjo.events.push(BanjoEvent::Drowning);
        }
    }
}

pub fn wonderwing_active(m: &MarioRuntime) -> bool { m.action_arg == BANJO_WONDER || m.action_arg == BANJO_WONDER_JUMP }
pub fn talon_trotting(m: &MarioRuntime) -> bool { m.action_arg == BANJO_TROT || m.action_arg == BANJO_TROT_JUMP }
pub fn flying(m: &MarioRuntime) -> bool { [BANJO_FLY_ENTER, BANJO_FLY, BANJO_BOMB, BANJO_BOMB_END].contains(&m.action_arg) }

fn pressed(m: &MarioRuntime, mask: u32) -> bool { m.controller.button_pressed & mask != 0 }
fn down(m: &MarioRuntime, mask: u32) -> bool { m.controller.button_down & mask != 0 }
fn lerp(t: f64, a: f64, b: f64) -> f64 { a + (b - a) * t.clamp(0.0, 1.0) }
fn forward_dir(m: &MarioRuntime) -> Vec3 { [sins(m.face_angle[1]), 0.0, coss(m.face_angle[1])] }

/// `bastick_getZonePosition` with the zone (0.03..1.0) the trot and wonderwing states set.
fn zone_position(m: &MarioRuntime) -> f64 {
    let frac = (m.intended_mag / 32.0).clamp(0.0, 1.0);
    if frac <= 0.03 { 0.0 } else { (frac - 0.03) / 0.97 }
}

fn turn_toward_stick(m: &mut MarioRuntime, rate: f64) {
    if m.intended_mag > 0.0 {
        let turn = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        m.face_angle[1] = to_s16(m.face_angle[1] + clamp(turn, -rate, rate)) as f64;
    }
}

/// Per-frame bookkeeping that runs whatever Banjo is doing: egg projectiles, pickups, timers.
pub fn tick(m: &mut MarioRuntime) {
    if m.banjo.turbo_frames > 0.0 {
        m.banjo.turbo_frames -= 1.0;
        if m.banjo.turbo_frames <= 0.0 && m.banjo.power == Power::Turbo { m.banjo.power = Power::None; }
    }
    if m.banjo.fly_cooldown > 0.0 { m.banjo.fly_cooldown -= 1.0; }
    if pressed(m, R_TRIG) && !flying(m) {
        m.banjo.cycle_power();
    }
    step_eggs(m);
    collect_pickups(m);
    visit_molehills(m);
    if m.banjo.climb_regrab > 0.0 { m.banjo.climb_regrab -= 1.0; }
    if wonderwing_active(m) {
        m.invinc_timer = m.invinc_timer.max(2.0);
    }
}

/// The lair's note doors: closed ones are a wall; with the Note Door lesson and enough notes, walking up opens them
/// (`chnotedoor_update`). Outside a progression game every door is open, like every other lock.
pub fn tend_note_doors(m: &mut MarioRuntime) {
    tend_switches(m);
    if !m.banjo.in_progression() { return; }
    if !m.banjo.switch_doors.is_empty() {
        m.banjo.switch_door_side.resize(m.banjo.switch_doors.len(), 0.0);
        for i in 0..m.banjo.switch_doors.len() {
            let d = m.banjo.switch_doors[i].clone();
            if m.banjo.switches & (1 << d.flag) != 0 { continue; }
            let mut side = m.banjo.switch_door_side[i];
            block_slab(m, d.pos, d.yaw, &mut side);
            m.banjo.switch_door_side[i] = side;
        }
    }
    if m.banjo.door_actors.is_empty() { return; }
    m.banjo.door_side.resize(m.banjo.door_actors.len(), 0.0);
    for i in 0..m.banjo.door_actors.len() {
        let d = m.banjo.door_actors[i].clone();
        if m.banjo.note_door_open(d.index) { continue; }
        let dx = [m.pos[0] - d.pos[0], m.pos[2] - d.pos[2]];
        if (dx[0] * dx[0] + dx[1] * dx[1]).sqrt() < DOOR_OPEN_RANGE && m.banjo.try_open_note_door(d.index) { continue; }
        let mut side = m.banjo.door_side[i];
        block_slab(m, d.pos, d.yaw, &mut side);
        m.banjo.door_side[i] = side;
    }
}

/// Floor switches: a ground pound (landing) or Beak Buster impact within 160 units, no higher than 160 above the switch, presses it.
pub fn tend_switches(m: &mut MarioRuntime) {
    if m.banjo.switch_actors.is_empty() { return; }
    let busted: Vec<Vec3> = m.banjo.events.iter().filter_map(|e| if let BanjoEvent::BeakBuster { pos } = e { Some(*pos) } else { None }).collect();
    let pound = m.action == ACT_GROUND_POUND_LAND;
    for i in 0..m.banjo.switch_actors.len() {
        let sw = m.banjo.switch_actors[i].clone();
        if m.banjo.switches & (1 << sw.flag) != 0 { continue; }
        let near = |p: &Vec3| ((p[0] - sw.pos[0]).powi(2) + (p[2] - sw.pos[2]).powi(2)).sqrt() < 160.0 && p[1] > sw.pos[1] - 40.0 && p[1] < sw.pos[1] + 160.0;
        if (pound && near(&m.pos)) || busted.iter().any(near) {
            m.banjo.switches |= 1 << sw.flag;
            m.banjo.events.push(BanjoEvent::SwitchPressed(sw.flag));
        }
    }
}

/// Keep Banjo out of a closed door's slab (the shared part of note doors and switch doors).
fn block_slab(m: &mut MarioRuntime, pos: Vec3, yaw: f64, side_state: &mut f64) {
    let dx = [m.pos[0] - pos[0], m.pos[2] - pos[2]];
    let a = yaw.to_radians();
    let (sy, cy) = (a.sin(), a.cos());
    let (normal, lateral) = (dx[0] * sy + dx[1] * cy, dx[0] * cy - dx[1] * sy);
    let side = *side_state;
    if lateral.abs() < DOOR_HALF_WIDTH && m.pos[1] > pos[1] - 60.0 && m.pos[1] < pos[1] + DOOR_HEIGHT {
        if side == 0.0 && normal.abs() >= DOOR_THICKNESS {
            *side_state = normal.signum();
        } else if side != 0.0 && normal * side >= DOOR_THICKNESS {
            // Beyond the band on the original side: nothing to do.
        } else if side != 0.0 {
            let push = side * DOOR_THICKNESS - normal;
            m.pos[0] += sy * push;
            m.pos[2] += cy * push;
        }
    }
}

/// Standing next to a molehill, Bottles pops out and teaches the move (`ch/mole.c`; the dialog is skipped).
fn visit_molehills(m: &mut MarioRuntime) {
    if m.banjo.moles.is_empty() || m.action & ACT_FLAG_AIR != 0 { return; }
    let pos = m.pos;
    let mut learn = vec![];
    for mole in &m.banjo.moles {
        let (dx, dz) = (mole.pos[0] - pos[0], mole.pos[2] - pos[2]);
        if dx * dx + dz * dz < 260.0 * 260.0 && (mole.pos[1] - pos[1]).abs() < 300.0 {
            learn.extend_from_slice(mole_abilities(mole.sm, mole.id));
        }
    }
    for a in learn { m.banjo.learn(a); }
}

fn collect_pickups(m: &mut MarioRuntime) {
    let pos = m.pos;
    let mut taken = vec![];
    for (i, p) in m.banjo.pickups.iter().enumerate() {
        if p.collected { continue; }
        let dx = p.pos[0] - pos[0];
        let dz = p.pos[2] - pos[2];
        let dy = p.pos[1] - pos[1];
        if dx * dx + dz * dz < 90.0 * 90.0 && dy > -60.0 && dy < 170.0 {
            taken.push(i);
        }
    }
    for i in taken {
        let kind = m.banjo.pickups[i].kind;
        m.banjo.pickups[i].collected = true;
        if kind == PickupKind::Honeycomb {
            m.heal_counter += 4.0;
        }
        m.banjo.take(kind);
    }
    m.banjo.pickups.retain(|p| !p.collected);
}

// ---------------------------------------------------------------------------------------------
// Eggs
// ---------------------------------------------------------------------------------------------

fn shatter(m: &mut MarioRuntime, pos: Vec3, hit: bool) {
    m.banjo.events.push(BanjoEvent::EggShattered { pos, hit });
}

fn step_eggs(m: &mut MarioRuntime) {
    if m.banjo.eggs_in_flight.is_empty() { return; }
    let mut eggs = std::mem::take(&mut m.banjo.eggs_in_flight);
    let mario = m.pos;
    eggs.retain_mut(|egg| {
        egg.age += 1.0;
        let before = egg.pos;
        if egg.kind == EggKind::Ass {
            egg.vel[1] = (egg.vel[1] - EGG_ASS_GRAVITY).max(-80.0);
        }
        for k in 0..3 { egg.pos[k] += egg.vel[k]; }
        // Pickup: a dropped egg that landed can be walked over again (chCollectible_collectEgg).
        if egg.kind == EggKind::Ass && egg.age > 12.0 {
            let (dx, dz) = (egg.pos[0] - mario[0], egg.pos[2] - mario[2]);
            if dx * dx + dz * dz < 80.0 * 80.0 && (egg.pos[1] - mario[1]).abs() < 140.0 {
                m.banjo.eggs = (m.banjo.eggs + 1).min(EGG_MAX);
                m.banjo.events.push(BanjoEvent::Collected(PickupKind::Egg));
                return false;
            }
        }
        // Walls.
        let mut probe = egg.pos;
        let walls = m.world.f32_find_wall_collision(&mut probe, 0.0, 20.0);
        if walls > 0 && egg.kind == EggKind::Head {
            let at = before;
            shatter(m, at, false);
            return false;
        }
        // Floor.
        let floor = m.world.find_floor(egg.pos[0], egg.pos[1] + 20.0, egg.pos[2]);
        if floor.surface.is_some() && egg.pos[1] <= floor.height + 12.0 {
            if egg.kind == EggKind::Head {
                let at = [egg.pos[0], floor.height + 12.0, egg.pos[2]];
                shatter(m, at, false);
                return false;
            }
            egg.pos[1] = floor.height + 12.0;
            egg.vel = [egg.vel[0] * 0.5, 0.0, egg.vel[2] * 0.5];
        }
        // Objects.
        if egg.kind == EggKind::Head {
            for i in 0..m.objects.len() {
                let id = ObjId(i as u32);
                let o = &m.objects[id];
                if !o.active || o.held_state == HELD_HELD { continue; }
                let t = o.hitbox.interact_type;
                if t & (INTERACT_DAMAGE | INTERACT_BREAKABLE | INTERACT_GRABBABLE) == 0 { continue; }
                let egg_box = ObjectHitbox { interact_type: 0, down_offset: 20.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0, radius: 20.0, height: 40.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 };
                if hitboxes_overlap((&egg.pos, &egg_box), (&o.pos, &o.hitbox)) {
                    let o = &mut m.objects[id];
                    attack_object(o, ATTACK_PUNCH);
                    if t & INTERACT_GRABBABLE != 0 { o.interact_status |= INT_STATUS_MARIO_KNOCKBACK_DMG; }
                    let at = egg.pos;
                    shatter(m, at, true);
                    return false;
                }
            }
        }
        let life = if egg.kind == EggKind::Head { EGG_LIFETIME } else { EGG_ASS_LIFETIME };
        if egg.age >= life {
            let at = egg.pos;
            shatter(m, at, false);
            return false;
        }
        true
    });
    // Eggs spawned by shatter() callbacks cannot occur; keep ones fired this frame.
    eggs.append(&mut m.banjo.eggs_in_flight);
    m.banjo.eggs_in_flight = eggs;
}

fn fire_egg(m: &mut MarioRuntime, backwards: bool) {
    if m.banjo.eggs <= 0 {
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::Egg));
        return;
    }
    m.banjo.eggs -= 1;
    let dir = forward_dir(m);
    if !backwards {
        // fxegg_head_spawn: player + (0, 80, 0) + 70 along the facing.
        let pos = [m.pos[0] + dir[0] * 70.0, m.pos[1] + 80.0, m.pos[2] + dir[2] * 70.0];
        let vel = [dir[0] * EGG_SPEED, 0.0, dir[2] * EGG_SPEED];
        m.banjo.eggs_in_flight.push(Egg { kind: EggKind::Head, pos, vel, age: 0.0 });
    } else {
        // fxegg_ass_spawn: dropped 18 behind at +60, thrown back at 200 u/s and up at ~700 u/s.
        let pos = [m.pos[0] - dir[0] * 18.0, m.pos[1] + 60.0, m.pos[2] - dir[2] * 18.0];
        let vel = [-dir[0] * EGG_ASS_BACK, EGG_ASS_UP, -dir[2] * EGG_ASS_BACK];
        m.banjo.eggs_in_flight.push(Egg { kind: EggKind::Ass, pos, vel, age: 0.0 });
    }
    m.banjo.events.push(BanjoEvent::EggFired { pos: m.pos, backwards });
}

fn step_egg_pose(m: &mut MarioRuntime, backwards: bool) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    // bsegghead_update: more presses of the button queue up to three eggs.
    if pressed(m, if backwards { C_DOWN } else { C_UP }) && m.banjo.queued_eggs < 3 && t > 2.0 {
        m.banjo.queued_eggs += 1;
    }
    if !backwards { turn_toward_stick(m, EGG_TURN); }
    mario_set_forward_vel(m, 0.0);
    let fire_at = if backwards { 8.0 } else { EGG_FIRE_FRAME };
    if t == fire_at || (t > fire_at && m.banjo.queued_eggs > 1 && (t - fire_at) % EGG_REPEAT_FRAMES == 0.0 && (t - fire_at) / EGG_REPEAT_FRAMES < m.banjo.queued_eggs as f64) {
        fire_egg(m, backwards);
    }
    // After 0.65 of the animation other moves may interrupt (func_802ADCD4).
    if t >= EGG_END_FRAME * 0.65 && pressed(m, A_BUTTON) {
        start_jump(m);
        return true;
    }
    if t >= EGG_END_FRAME * 0.65 && pressed(m, C_LEFT | C_RIGHT) {
        m.set_action(ACT_IDLE, 0.0);
        return start_moves(m).unwrap_or(true);
    }
    let _ = m.step_ground();
    if m.input & INPUT_OFF_FLOOR != 0 {
        m.set_action(ACT_FREEFALL, 0.0);
        return true;
    }
    if t + 1.0 >= EGG_END_FRAME {
        m.set_action(if down(m, Z_TRIG) { ACT_CROUCHING } else { ACT_IDLE }, 0.0);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------------------------

fn start_jump(m: &mut MarioRuntime) {
    if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
    start(m, ACT_JUMP, BANJO_JUMP);
    let wading = m.banjo.power == Power::Wading;
    m.vel[1] = if wading { speed(400.0) } else { JUMP_VELOCITY };
    m.jump_takeoff_speed = MAX_WALK.max(m.forward_vel.abs());
}

/// The extended moves. `Some(consumed)` replaces the rest of `step_banjo` for this frame.
pub fn step(m: &mut MarioRuntime) -> Option<bool> {
    let mv = banjo_move(m);
    if mv >= BANJO_TROT {
        let a = m.action;
        let consumed = if mv == BANJO_TROT && a == ACT_WALKING {
            Some(step_trot_ground(m))
        } else if mv == BANJO_TROT_JUMP && a & ACT_FLAG_AIR != 0 {
            Some(step_trot_air(m))
        } else if mv == BANJO_BUSTER && a == ACT_GROUND_POUND {
            Some(step_buster_dive(m))
        } else if mv == BANJO_BUSTER && a == ACT_GROUND_POUND_LAND {
            Some(step_buster_impact(m))
        } else if mv == BANJO_BUSTER_HOP && a & ACT_FLAG_AIR != 0 {
            Some(step_buster_hop(m))
        } else if mv == BANJO_SHOCK_CHARGE && a == ACT_IDLE {
            Some(step_shock_charge(m))
        } else if mv == BANJO_SHOCK_JUMP && a & ACT_FLAG_AIR != 0 {
            Some(step_shock_jump(m))
        } else if mv == BANJO_EGG_HEAD && a == ACT_IDLE {
            Some(step_egg_pose(m, false))
        } else if mv == BANJO_EGG_ASS && a == ACT_IDLE {
            Some(step_egg_pose(m, true))
        } else if mv == BANJO_WONDER && a == ACT_WALKING {
            Some(step_wonder_ground(m))
        } else if mv == BANJO_WONDER_JUMP && a & ACT_FLAG_AIR != 0 {
            Some(step_wonder_air(m))
        } else if (mv == BANJO_FLY_ENTER || mv == BANJO_FLY) && a & ACT_FLAG_AIR != 0 {
            Some(step_fly(m, mv == BANJO_FLY_ENTER))
        } else if (mv == BANJO_BOMB || mv == BANJO_BOMB_END) && a & ACT_FLAG_AIR != 0 {
            Some(step_bomb(m))
        } else if mv == BANJO_CLIMB && a & ACT_FLAG_AIR != 0 {
            Some(step_climb(m))
        } else if mv == BANJO_CLIMB_EXIT && a & ACT_FLAG_AIR != 0 {
            Some(step_climb_exit(m))
        } else {
            None
        };
        if consumed.is_some() { return consumed; }
    }
    if let Some(c) = try_grab_pole(m) { return Some(c); }
    start_moves(m)
}

fn start_moves(m: &mut MarioRuntime) -> Option<bool> {
    // Airborne: Z = Beak Buster (bainput_should_beak_bust), from any jump or flap.
    if m.action & ACT_FLAG_AIR != 0 {
        if pressed(m, Z_TRIG) && FLAP_FROM.contains(&m.action) && banjo_move(m) != BANJO_BUSTER && m.banjo.has(Ability::BeakBuster) {
            return Some(start_buster(m));
        }
        return None;
    }
    if !on_basic_ground(m) && !is_crouched(m.action) { return None; }
    let crouched = is_crouched(m.action);
    let shoulder = m.controller.button_pressed;
    // Eggs: Z + C-up / C-down in BK. C-up alone is first-person there, so the port accepts it alone.
    let eggs_ok = m.banjo.has(Ability::Eggs);
    if shoulder & C_UP != 0 && m.banjo.eggs > 0 && eggs_ok {
        return Some(start_egg(m, false));
    }
    if shoulder & C_DOWN != 0 && m.banjo.eggs > 0 && eggs_ok {
        return Some(start_egg(m, true));
    }
    if (shoulder & C_UP != 0 || shoulder & C_DOWN != 0) && eggs_ok {
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::Egg));
    }
    if shoulder & C_LEFT != 0 && m.banjo.has(Ability::TalonTrot) {
        return Some(start_trot(m));
    }
    if shoulder & C_RIGHT != 0 && m.banjo.has(Ability::Wonderwing) {
        if m.banjo.gold_feathers > 0 {
            return Some(start_wonder(m));
        }
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::GoldFeather));
    }
    if pressed(m, A_BUTTON) && !crouched {
        match m.banjo.power {
            Power::Shock if m.banjo.has(Ability::ShockJump) => return Some(start_shock(m)),
            Power::Flight if m.banjo.has(Ability::Flight) => return Some(start_fly(m)),
            _ => {}
        }
    }
    if m.banjo.turbo_frames > 0.0 && m.banjo.power == Power::Turbo && crouched && m.banjo.has(Ability::TalonTrot) {
        return Some(start_trot(m));
    }
    None
}

fn start_egg(m: &mut MarioRuntime, backwards: bool) -> bool {
    mario_set_forward_vel(m, 0.0);
    start(m, ACT_IDLE, if backwards { BANJO_EGG_ASS } else { BANJO_EGG_HEAD });
    m.banjo.queued_eggs = 1;
    step_egg_pose(m, backwards)
}

// -- Talon Trot --------------------------------------------------------------------------------

fn start_trot(m: &mut MarioRuntime) -> bool {
    m.banjo.latched = !down(m, Z_TRIG);
    start(m, ACT_WALKING, BANJO_TROT);
    // The entry frame only switches state (the same press must not end a latched trot).
    true
}

fn trot_should_end(m: &MarioRuntime) -> bool {
    if m.banjo.turbo_frames > 0.0 { return false; }
    if m.banjo.latched { pressed(m, Z_TRIG) || pressed(m, C_LEFT) } else { !down(m, Z_TRIG) }
}

fn trot_limits(m: &MarioRuntime) -> (f64, f64) {
    if m.banjo.turbo_frames > 0.0 { (TURBO_MIN, TURBO_MAX) } else { (TROT_MIN, TROT_MAX) }
}

fn step_trot_ground(m: &mut MarioRuntime) -> bool {
    if trot_should_end(m) {
        m.banjo.latched = false;
        m.set_action(if m.forward_vel.abs() > 3.0 { ACT_WALKING } else { ACT_IDLE }, 0.0);
        return false;
    }
    if pressed(m, A_BUTTON) {
        return start_trot_jump(m);
    }
    let (lo, hi) = trot_limits(m);
    let target = if m.intended_mag > 0.0 { lerp(zone_position(m), lo, hi) } else { 0.0 };
    turn_toward_stick(m, 0x500 as f64);
    let fwd = approach_f32(m.forward_vel, target, 2.0, 3.0);
    if m.input & INPUT_OFF_FLOOR != 0 {
        start(m, ACT_FREEFALL, BANJO_TROT_JUMP);
        m.vel[1] = 0.0;
        return step_trot_air(m);
    }
    mario_set_forward_vel(m, fwd);
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        start(m, ACT_FREEFALL, BANJO_TROT_JUMP);
        m.vel[1] = 0.0;
    } else if hit == GROUND_STEP_HIT_WALL as i32 {
        mario_set_forward_vel(m, 0.0);
    }
    true
}

fn start_trot_jump(m: &mut MarioRuntime) -> bool {
    start(m, ACT_JUMP, BANJO_TROT_JUMP);
    m.vel[1] = TROT_JUMP_VELOCITY;
    step_trot_air(m)
}

fn step_trot_air(m: &mut MarioRuntime) -> bool {
    let (_, hi) = trot_limits(m);
    air_control(m, hi, 1.0);
    m.action_timer += 1.0;
    let held = down(m, A_BUTTON) && m.vel[1] > 0.0;
    if pressed(m, Z_TRIG) && !m.banjo.latched && trot_should_end(m) {
        // Z released mid-air ends the trot after landing; nothing to do now.
    }
    let landed = air_step(m, if held { TROT_GRAVITY } else { GRAVITY }, TERMINAL);
    if landed && m.action & ACT_FLAG_AIR == 0 && m.action != ACT_LEDGE_GRAB {
        if trot_should_end(m) {
            m.banjo.latched = false;
            m.set_action(ACT_IDLE, 0.0);
        } else {
            start(m, ACT_WALKING, BANJO_TROT);
        }
    }
    true
}

// -- Beak Buster -------------------------------------------------------------------------------

fn start_buster(m: &mut MarioRuntime) -> bool {
    start(m, ACT_GROUND_POUND, BANJO_BUSTER);
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.action_timer = 0.0;
    m.banjo.buster_stage = 0.0;
    step_buster_dive(m)
}

fn step_buster_dive(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t < BUSTER_WINDUP {
        // bsbbuster_init: gravity 0, velocity cleared while Banjo tucks his beak.
        m.vel = [0.0; 3];
        m.forward_vel = 0.0;
        return true;
    }
    if m.banjo.buster_stage == 0.0 {
        m.banjo.buster_stage = 1.0;
        m.vel[1] = -BUSTER_DIVE;
    }
    m.vel[0] = 0.0;
    m.vel[2] = 0.0;
    m.vel[1] = (m.vel[1] - BUSTER_GRAVITY).max(BUSTER_DIVE_TERMINAL);
    let vy = m.vel[1];
    let hit = m.step_air(0);
    if hit == AIR_STEP_LANDED as i32 {
        m.land_event(vy, 0.0);
        let at = m.pos;
        m.banjo.events.push(BanjoEvent::BeakBuster { pos: at });
        m.events.push(MarioEvent::GroundpoundLand);
        // Hand the frame to the impact state; its first frame carries the ground-pound hitbox.
        m.set_action(ACT_GROUND_POUND_LAND, BANJO_BUSTER);
        m.vel = [0.0; 3];
        m.action_timer = 0.0;
        return true;
    }
    if hit != AIR_STEP_NONE as i32 { m.vel[1] = vy; }
    true
}

fn step_buster_impact(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    m.vel = [0.0; 3];
    mario_set_forward_vel(m, 0.0);
    if m.action_timer >= BUSTER_FREEZE {
        // The rebound hop (D_80364998 / D_8036499C).
        start(m, ACT_FREEFALL, BANJO_BUSTER_HOP);
        m.vel = [0.0, BUSTER_HOP, 0.0];
        m.forward_vel = 0.0;
    }
    true
}

fn step_buster_hop(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    air_control(m, speed(120.0), 1.0);
    air_step(m, BUSTER_HOP_GRAVITY, TERMINAL);
    true
}

// -- Shock Spring Jump -------------------------------------------------------------------------

fn start_shock(m: &mut MarioRuntime) -> bool {
    mario_set_forward_vel(m, 0.0);
    start(m, ACT_IDLE, BANJO_SHOCK_CHARGE);
    m.banjo.shock_armed = true;
    step_shock_charge(m)
}

fn step_shock_charge(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    mario_set_forward_vel(m, 0.0);
    let _ = m.step_ground();
    // Anim < 0.3637 and A released cancels the jump (D_8037D380 = 0).
    if !down(m, A_BUTTON) && m.action_timer < SHOCK_CHARGE_FRAMES {
        m.banjo.shock_armed = false;
    }
    if m.action_timer >= SHOCK_CHARGE_FRAMES {
        if m.banjo.shock_armed {
            if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
            start(m, ACT_JUMP, BANJO_SHOCK_JUMP);
            m.vel[1] = SHOCK_VELOCITY;
            m.jump_takeoff_speed = MAX_WALK;
            m.banjo.events.push(BanjoEvent::ShockSpring);
            m.events.push(MarioEvent::Jump { action: ACT_JUMP, vy: SHOCK_VELOCITY });
        } else {
            m.set_action(ACT_IDLE, 0.0);
        }
    }
    true
}

fn step_shock_jump(m: &mut MarioRuntime) -> bool {
    if pressed(m, Z_TRIG) { return start_buster(m); }
    if pressed(m, A_BUTTON) && m.vel[1] < 12.0 && m.banjo_flap == 0.0 {
        start(m, ACT_FREEFALL, BANJO_FLAP);
        m.banjo_flap = 75.0;
        m.vel[1] = 0.0;
        return false;
    }
    air_control(m, MAX_WALK, 1.5);
    m.action_timer += 1.0;
    let held = down(m, A_BUTTON) && m.vel[1] > 0.0;
    air_step(m, if held { SHOCK_GRAVITY } else { GRAVITY }, TERMINAL);
    true
}

// -- Wonderwing --------------------------------------------------------------------------------

fn start_wonder(m: &mut MarioRuntime) -> bool {
    m.banjo.latched = !down(m, Z_TRIG);
    m.banjo.wonder_timer = 0.0;
    start(m, ACT_WALKING, BANJO_WONDER);
    m.invinc_timer = m.invinc_timer.max(2.0);
    m.banjo.events.push(BanjoEvent::WonderwingOn);
    true
}

fn wonder_should_end(m: &mut MarioRuntime) -> bool {
    // func_802AA58C: a gold feather every 2 seconds, out of feathers = exit.
    m.banjo.wonder_timer += 1.0;
    if m.banjo.wonder_timer >= WONDER_FEATHER_FRAMES {
        m.banjo.wonder_timer = 0.0;
        if m.banjo.gold_feathers <= 0 { return true; }
        m.banjo.gold_feathers -= 1;
    }
    if m.banjo.gold_feathers <= 0 && m.banjo.wonder_timer == 0.0 { return true; }
    if m.banjo.latched { pressed(m, Z_TRIG) || pressed(m, C_RIGHT) } else { !down(m, Z_TRIG) }
}

fn end_wonder(m: &mut MarioRuntime) {
    m.banjo.latched = false;
    m.banjo.events.push(BanjoEvent::WonderwingOff);
    m.set_action(if m.action & ACT_FLAG_AIR != 0 { ACT_FREEFALL } else { ACT_IDLE }, 0.0);
}

fn step_wonder_ground(m: &mut MarioRuntime) -> bool {
    if wonder_should_end(m) {
        end_wonder(m);
        return false;
    }
    if pressed(m, A_BUTTON) {
        start(m, ACT_JUMP, BANJO_WONDER_JUMP);
        m.vel[1] = WONDER_JUMP_VELOCITY;
        return step_wonder_air(m);
    }
    let target = if m.intended_mag > 0.0 { lerp(zone_position(m), WONDER_MIN, WONDER_MAX) } else { 0.0 };
    turn_toward_stick(m, 0x500 as f64);
    let fwd = approach_f32(m.forward_vel, target, 2.0, 3.0);
    mario_set_forward_vel(m, fwd);
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        start(m, ACT_FREEFALL, BANJO_WONDER_JUMP);
        m.vel[1] = 0.0;
    } else if hit == GROUND_STEP_HIT_WALL as i32 {
        mario_set_forward_vel(m, 0.0);
    }
    true
}

fn step_wonder_air(m: &mut MarioRuntime) -> bool {
    if wonder_should_end(m) {
        end_wonder(m);
        return false;
    }
    air_control(m, WONDER_MAX, 1.0);
    m.action_timer += 1.0;
    let held = down(m, A_BUTTON) && m.vel[1] > 0.0;
    let landed = air_step(m, if held { WONDER_GRAVITY } else { GRAVITY }, TERMINAL);
    if landed && m.action & ACT_FLAG_AIR == 0 && m.action != ACT_LEDGE_GRAB {
        start(m, ACT_WALKING, BANJO_WONDER);
    }
    true
}

// -- Flight and Beak Bomb ----------------------------------------------------------------------

fn start_fly(m: &mut MarioRuntime) -> bool {
    mario_set_forward_vel(m, 0.0);
    start(m, ACT_FREEFALL, BANJO_FLY_ENTER);
    m.vel = [0.0; 3];
    m.action_timer = 0.0;
    m.banjo.fly_pitch = 0.0;
    m.banjo.fly_flap = 0.0;
    m.banjo.fly_cooldown = BOMB_COOLDOWN;
    m.banjo.events.push(BanjoEvent::TakeOff);
    // The take-off itself is a hop out of the ground; step_fly applies the lift.
    m.pos[1] += 1.0;
    step_fly(m, true)
}

fn step_fly(m: &mut MarioRuntime, entering: bool) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if entering {
        // bsbfly_enter: crouch, lift at 1600 u/s on anim 0.2416, fly once the climb turns over.
        m.vel[0] = 0.0;
        m.vel[2] = 0.0;
        if t == FLY_LIFT_FRAME { m.vel[1] = FLY_LIFT + FLY_ENTER_GRAVITY; }
        if t < FLY_LIFT_FRAME {
            return true;
        }
        let landed = fly_move(m, FLY_ENTER_GRAVITY, -75.0);
        if landed { return true; }
        if m.vel[1] < 0.0 {
            m.action_arg = BANJO_FLY;
            m.banjo.fly_pitch = 0.0;
            m.forward_vel = 0.0;
        }
        return true;
    }
    // Steering: stick X turns (R = tighter), stick Y pitches (forward = nose down, 0..80 deg).
    let stick_x = (m.controller.stick_x / 64.0).clamp(-1.0, 1.0);
    let stick_y = (m.controller.stick_y / 64.0).clamp(-1.0, 1.0);
    let yaw_rate = if down(m, R_TRIG) { 2.0 * FLY_YAW } else { FLY_YAW };
    m.face_angle[1] = to_s16(m.face_angle[1] - stick_x * yaw_rate) as f64;
    // pitch_setIdeal: >0 dives to 80 degrees, <0 climbs to 300..360 (-60..0), stored here as -60..80.
    let ideal = if stick_y >= 0.0 { stick_y * FLY_PITCH_MAX } else { stick_y * 60.0 };
    m.banjo.fly_pitch = approach_f32(m.banjo.fly_pitch, ideal, 4.0, 4.0);
    let pitch = m.banjo.fly_pitch;
    m.face_angle[0] = to_s16(pitch * 182.04) as f64;

    // Flap: A spends a red feather (bsbfly_update / func_80346C10 on ITEM_F_RED_FEATHER).
    if pressed(m, A_BUTTON) {
        if m.banjo.red_feathers > 0 {
            m.banjo.red_feathers -= 1;
            m.banjo.fly_flap = FLY_FLAP_FRAMES;
        } else {
            m.banjo.events.push(BanjoEvent::OutOf(PickupKind::RedFeather));
        }
    }
    let (gravity, terminal, horizontal);
    if pitch > 60.0 {
        // Steep dive: slow forward, fast fall (map 60..80 -> 600..60, -99.9..-1000).
        let k = (pitch - 60.0) / 20.0;
        gravity = lerp(k, accel(300.0), accel(700.0));
        terminal = -lerp(k, 99.9, 1000.33) * 1.25 / 30.0;
        horizontal = lerp(k, FLY_SPEED, speed(60.0));
    } else if pitch < 0.0 {
        // Nose up: bleed speed (map 300..340 -> 0..600).
        let k = (pitch + 60.0) / 60.0;
        gravity = FLY_GRAVITY;
        terminal = -speed(99.9);
        horizontal = lerp(k, 0.0, FLY_SPEED);
    } else {
        gravity = FLY_GRAVITY;
        terminal = -speed(99.9);
        horizontal = FLY_SPEED;
    }
    let terminal = terminal.max(-110.0);
    m.forward_vel = approach_f32(m.forward_vel, horizontal, 3.0, 3.0);
    m.vel[0] = sins(m.face_angle[1]) * m.forward_vel;
    m.vel[2] = coss(m.face_angle[1]) * m.forward_vel;
    if m.banjo.fly_flap > 0.0 {
        m.banjo.fly_flap -= 1.0;
        m.vel[1] = m.vel[1].max(FLY_FLAP_VELOCITY);
    }
    if pressed(m, Z_TRIG) {
        m.face_angle[0] = 0.0;
        return start_buster(m);
    }
    if pressed(m, B_BUTTON) && m.banjo.fly_cooldown <= 0.0 && m.banjo.has(Ability::BeakBomb) {
        if m.banjo.red_feathers > 0 {
            m.banjo.red_feathers -= 1;
            return start_bomb(m);
        }
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::RedFeather));
    }
    let landed = fly_move(m, gravity, terminal);
    if landed {
        m.face_angle[0] = 0.0;
        m.banjo.fly_pitch = 0.0;
    }
    true
}

/// One airborne frame for flight; lands into the ordinary landing actions.
fn fly_move(m: &mut MarioRuntime, gravity: f64, terminal: f64) -> bool {
    m.vel[1] = terminal.max(m.vel[1] - gravity);
    let vy = m.vel[1];
    let hit = m.step_air(0);
    if hit == AIR_STEP_LANDED as i32 {
        let fv = m.forward_vel;
        m.land_event(vy, fv);
        m.face_angle[0] = 0.0;
        m.set_action(ACT_FREEFALL_LAND, 0.0);
        return true;
    }
    if hit == AIR_STEP_HIT_WALL as i32 {
        m.forward_vel *= 0.3;
    }
    m.vel[1] = vy;
    false
}

fn start_bomb(m: &mut MarioRuntime) -> bool {
    let pitch = m.banjo.fly_pitch.max(10.0) * std::f64::consts::PI / 180.0;
    let dir = forward_dir(m);
    m.banjo.bomb_dir = [dir[0] * pitch.cos(), -pitch.sin(), dir[2] * pitch.cos()];
    m.banjo.bomb_start = m.pos;
    start(m, ACT_FREEFALL, BANJO_BOMB);
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.action_timer = 0.0;
    m.banjo.events.push(BanjoEvent::BeakBomb);
    step_bomb(m)
}

fn step_bomb(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if m.action_arg == BANJO_BOMB_END {
        // bsbfly_beakbombend: tumble out for 0.35 s, then back into flight.
        m.face_angle[0] = 0.0;
        if t >= BOMB_END_FRAMES {
            start(m, ACT_FREEFALL, BANJO_FLY);
            m.banjo.fly_cooldown = BOMB_COOLDOWN;
            m.banjo.fly_pitch = 0.0;
            m.action_timer = 0.0;
            return true;
        }
        if fly_move(m, accel(1200.0), TERMINAL) { return true; }
        return true;
    }
    if t < BOMB_WINDUP {
        m.vel = [0.0; 3];
        return true;
    }
    // Hitbox: an attacking, fast move (detect_interaction -> ATTACK_FAST_ATTACK).
    m.action = ACT_DIVE;
    m.action_arg = BANJO_BOMB;
    let d = m.banjo.bomb_dir;
    m.vel = [d[0] * BOMB_SPEED, d[1] * BOMB_SPEED, d[2] * BOMB_SPEED];
    m.forward_vel = BOMB_SPEED;
    m.face_angle[1] = atan2s(d[2], d[0]) as f64;
    let before = m.pos;
    let hit = m.step_air(0);
    let travelled = ((m.pos[0] - m.banjo.bomb_start[0]).powi(2) + (m.pos[1] - m.banjo.bomb_start[1]).powi(2) + (m.pos[2] - m.banjo.bomb_start[2]).powi(2)).sqrt();
    let _ = before;
    if hit == AIR_STEP_LANDED as i32 {
        // BS_58_BEAKBOMB_CRASH: bounces off the floor.
        let fv = m.forward_vel;
        m.land_event(m.vel[1], fv);
        m.events.push(MarioEvent::GroundpoundLand);
        let at = m.pos;
        m.banjo.events.push(BanjoEvent::BeakBuster { pos: at });
        m.face_angle[0] = 0.0;
        m.forward_vel = 0.0;
        m.set_action(ACT_FREEFALL_LAND, 0.0);
        return true;
    }
    if hit == AIR_STEP_HIT_WALL as i32 {
        // func_802A40E0: crashing into a wall costs a honeycomb and knocks Banjo back.
        let at = m.pos;
        m.banjo.events.push(BanjoEvent::BeakBuster { pos: at });
        m.face_angle[0] = 0.0;
        m.forward_vel = 0.0;
        if !m.take_damage(1.0, Some([at[0] + d[0] * 100.0, at[1], at[2] + d[2] * 100.0])) {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        return true;
    }
    if travelled > BOMB_RANGE {
        start(m, ACT_FREEFALL, BANJO_BOMB_END);
        m.vel = [d[0] * speed(300.0), 0.0, d[2] * speed(300.0)];
        m.forward_vel = speed(300.0);
        m.action_timer = 0.0;
    }
    true
}

// -- Climbing (bs/climb.c, climb.c, ch/climbBase.c) ---------------------------------------------

fn horizontal_distance(a: Vec3, b: Vec3) -> f64 { ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() }

/// `climbBase`'s state 2 + `climb_regrab_update`: touching a pole's column above its base (and not above its top)
/// grabs it, if the Climb move is learned.
fn try_grab_pole(m: &mut MarioRuntime) -> Option<bool> {
    if m.banjo.climbs.is_empty() || m.banjo.climb_regrab > 0.0 || !m.banjo.has(Ability::Climb) { return None; }
    if m.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED || m.held_obj.is_some() { return None; }
    let a = m.action;
    let ok = a & ACT_FLAG_AIR != 0 || on_basic_ground(m) || is_crouched(a);
    if !ok { return None; }
    // Not from the other airborne specials.
    let mv = banjo_move(m);
    if mv >= BANJO_BUSTER && mv != BANJO_TROT && mv != BANJO_TROT_JUMP && mv != BANJO_WONDER && mv != BANJO_WONDER_JUMP { return None; }
    let pos = m.pos;
    for (i, c) in m.banjo.climbs.iter().enumerate() {
        let reach = c.radius + CLIMB_REACH;
        if horizontal_distance(pos, c.bottom) < reach && pos[1] >= c.bottom[1] - 4.0 && pos[1] <= c.top[1] {
            m.banjo.climb_pole = i;
            return Some(start_climb(m, i));
        }
    }
    None
}

fn start_climb(m: &mut MarioRuntime, i: usize) -> bool {
    let c = m.banjo.climbs[i].clone();
    // Face the pole (`bsclimb_idle_init`: yaw toward climb_getBottom).
    let yaw = (c.bottom[0] - m.pos[0]).atan2(c.bottom[2] - m.pos[2]);
    m.face_angle[1] = to_s16(yaw * 32768.0 / std::f64::consts::PI) as f64;
    start(m, ACT_FREEFALL, BANJO_CLIMB);
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.action_timer = 0.0;
    m.banjo.latched = false;
    m.banjo.events.push(BanjoEvent::ClimbGrab);
    step_climb(m)
}

/// Where Banjo hangs on the pole: on the axis, backed off by the pole's radius along his facing.
fn climb_anchor(c: &ClimbPole, y: f64, face: f64) -> Vec3 {
    let back = c.radius;
    [c.bottom[0] - sins(face) * back, y, c.bottom[2] - coss(face) * back]
}

fn release_pole(m: &mut MarioRuntime) {
    m.banjo.climb_regrab = CLIMB_REGRAB;
    m.banjo.events.push(BanjoEvent::ClimbRelease);
}

fn step_climb(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    let Some(c) = m.banjo.climbs.get(m.banjo.climb_pole).cloned() else {
        m.set_action(ACT_FREEFALL, 0.0);
        return false;
    };
    // A jumps off backwards (BS_5_JUMP).
    if pressed(m, A_BUTTON) {
        release_pole(m);
        let yaw = to_s16(m.face_angle[1] + 0x8000 as f64) as f64;
        m.face_angle[1] = yaw;
        start(m, ACT_JUMP, BANJO_JUMP);
        m.vel[1] = JUMP_VELOCITY;
        m.forward_vel = CLIMB_JUMP_BACK;
        m.vel[0] = sins(yaw) * CLIMB_JUMP_BACK;
        m.vel[2] = coss(yaw) * CLIMB_JUMP_BACK;
        m.jump_takeoff_speed = MAX_WALK;
        m.pos[0] += m.vel[0];
        m.pos[2] += m.vel[2];
        return true;
    }
    // func_802AB5C0: stick Y zone 0.03..1.0 -> 100..300 u/s.
    let sy = (m.controller.stick_y / 64.0).clamp(-1.0, 1.0);
    let target = if sy.abs() < 0.03 { 0.0 } else { sy.signum() * lerp((sy.abs() - 0.03) / 0.97, CLIMB_MIN, CLIMB_MAX) };
    m.vel[1] = approach_f32(m.vel[1], target, CLIMB_ACCEL, CLIMB_ACCEL);
    let mut y = m.pos[1] + m.vel[1];
    y = y.clamp(c.bottom[1], c.top[1]);
    let at_top = y >= c.top[1] - 0.001;
    let at_bottom = y <= c.bottom[1] + 0.001;
    if at_bottom && target < 0.0 {
        // Down at the base: back to the ground (`bsclimb_move_update` -> BS_1_IDLE).
        m.pos = climb_anchor(&c, c.bottom[1], m.face_angle[1]);
        let floor = m.world.find_floor(m.pos[0], m.pos[1] + 80.0, m.pos[2]);
        if floor.surface.is_some() && (floor.height - m.pos[1]).abs() < 200.0 { m.pos[1] = floor.height; }
        m.vel = [0.0; 3];
        release_pole(m);
        m.set_action(ACT_IDLE, 0.0);
        return false;
    }
    if at_top && target > 0.0 && c.kind == 2 {
        m.pos = climb_anchor(&c, c.top[1], m.face_angle[1]);
        start(m, ACT_JUMP, BANJO_CLIMB_EXIT);
        m.banjo.climb_regrab = CLIMB_REGRAB * 3.0;
        m.action_timer = 0.0;
        m.vel = [sins(m.face_angle[1]) * CLIMB_EXIT_FORWARD, JUMP_VELOCITY, coss(m.face_angle[1]) * CLIMB_EXIT_FORWARD];
        m.forward_vel = CLIMB_EXIT_FORWARD;
        return step_climb_exit(m);
    }
    m.pos = climb_anchor(&c, y, m.face_angle[1]);
    m.forward_vel = 0.0;
    m.vel[0] = 0.0;
    m.vel[2] = 0.0;
    if m.vel[1].abs() > 0.8 && (m.action_timer as i64) % 14 == 0 {
        m.banjo.events.push(BanjoEvent::ClimbStep);
    }
    true
}

fn step_climb_exit(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    m.vel[0] = sins(m.face_angle[1]) * CLIMB_EXIT_FORWARD;
    m.vel[2] = coss(m.face_angle[1]) * CLIMB_EXIT_FORWARD;
    let landed = air_step(m, GRAVITY * 1350.0 / 2700.0, TERMINAL);
    if landed && m.action & ACT_FLAG_AIR == 0 {
        m.banjo.climb_regrab = m.banjo.climb_regrab.max(CLIMB_REGRAB);
    }
    true
}
