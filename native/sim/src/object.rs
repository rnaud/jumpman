//! SM64 object runtime core (src/sim/object.ts): hitboxes, held states, `object_step`
//! and the interaction checks used by Bob-ombs and breakable crates.
//!
//! Objects live in `ObjectManager::objects` (an arena `Vec<SimObject>`); everything that
//! refers to another object (Mario's held/used/interact object, `link`, `links`, ...) uses
//! an [`ObjId`] index into that arena. Objects are never removed, only deactivated, except
//! on `ObjectManager::reset`, so an `ObjId` stays valid for the life of a course.
use crate::collision::{CollisionWorld, Surface, FLOOR_LOWER_LIMIT};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::state::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Interaction constants (interaction.h / object_constants.h / obj_behaviors.c)
// ---------------------------------------------------------------------------

pub const INTERACT_POLE: u32 = 1 << 6;
pub const INTERACT_KOOPA: u32 = 1 << 7;
pub const INTERACT_KOOPA_SHELL: u32 = 1 << 19;
pub const INTERACT_GRABBABLE: u32 = 1 << 1;
pub const INTERACT_DAMAGE: u32 = 1 << 3;
pub const INTERACT_BREAKABLE: u32 = 1 << 9;
pub const INTERACT_MR_BLIZZARD: u32 = 1 << 21;

pub const INT_SUBTYPE_GRABS_MARIO: u32 = 0x00000004;
pub const INT_SUBTYPE_HOLDABLE_NPC: u32 = 0x00000010;
pub const INT_SUBTYPE_DROP_IMMEDIATELY: u32 = 0x00000040;
pub const INT_SUBTYPE_DELAY_INVINCIBILITY: u32 = 0x00000002;
pub const INT_SUBTYPE_NPC: u32 = 0x00004000;
pub const INT_SUBTYPE_KICKABLE: u32 = 0x00000100;
pub const INT_SUBTYPE_NOT_GRABBABLE: u32 = 0x00000200;

pub const INT_STATUS_MARIO_STUNNED: u32 = 1 << 0;
pub const INT_STATUS_MARIO_KNOCKBACK_DMG: u32 = 1 << 1;
pub const INT_STATUS_MARIO_DROP_OBJECT: u32 = 1 << 3;
pub const INT_STATUS_STOP_RIDING: u32 = 1 << 22;
pub const INTERACT_WHIRLPOOL: u32 = 1 << 25;
pub const INT_STATUS_ATTACKED_MARIO: u32 = 1 << 13;
pub const INT_STATUS_WAS_ATTACKED: u32 = 1 << 14;
pub const INT_STATUS_INTERACTED: u32 = 1 << 15;
pub const INT_STATUS_TOUCHED_BOB_OMB: u32 = 1 << 23;

pub const ATTACK_PUNCH: i32 = 1;
pub const ATTACK_KICK_OR_TRIP: i32 = 2;
pub const ATTACK_FROM_ABOVE: i32 = 3;
pub const ATTACK_GROUND_POUND_OR_TWIRL: i32 = 4;
pub const ATTACK_FAST_ATTACK: i32 = 5;
pub const ATTACK_FROM_BELOW: i32 = 6;

pub const HELD_FREE: i32 = 0;
pub const HELD_HELD: i32 = 1;
pub const HELD_THROWN: i32 = 2;
pub const HELD_DROPPED: i32 = 3;

pub const OBJ_COL_FLAG_GROUNDED: u32 = 1 << 0;
pub const OBJ_COL_FLAG_HIT_WALL: u32 = 1 << 1;
pub const OBJ_COL_FLAG_UNDERWATER: u32 = 1 << 2;
pub const OBJ_COL_FLAG_NO_Y_VEL: u32 = 1 << 3;

/// Index of an object in `ObjectManager::objects`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ObjId(pub u32);

impl ObjId {
    pub fn idx(self) -> usize { self.0 as usize }
}

macro_rules! behavior_kinds {
    ($($v:ident => $s:literal),* $(,)?) => {
        /// `ObjectBehaviorKind` (the TS string union). `as_str`/`from_name` use the TS spelling.
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum ObjectBehaviorKind { $($v),* }
        impl ObjectBehaviorKind {
            pub const ALL: &'static [ObjectBehaviorKind] = &[$(ObjectBehaviorKind::$v),*];
            pub const COUNT: usize = Self::ALL.len();
            pub fn as_str(self) -> &'static str { match self { $(ObjectBehaviorKind::$v => $s),* } }
            pub fn from_name(name: &str) -> Option<Self> { match name { $($s => Some(ObjectBehaviorKind::$v),)* _ => None } }
            pub fn index(self) -> usize { self as usize }
        }
    };
}

behavior_kinds! {
    Bobomb => "bobomb", MovingYellowCoin => "movingYellowCoin", BreakableBox => "breakableBox",
    BreakableBoxSmall => "breakableBoxSmall", ChainChomp => "chainChomp", WoodenPost => "woodenPost",
    ChainChompGate => "chainChompGate", ChainPart => "chainPart", PurpleSwitch => "purpleSwitch",
    OpenableGrill => "openableGrill", GrillDoor => "grillDoor", PitBowlingBall => "pitBowlingBall",
    MountainBallSpawner => "mountainBallSpawner", MountainBall => "mountainBall", KingBobomb => "kingBobomb",
    KoopaTheQuick => "koopaTheQuick", KoopaFlag => "koopaFlag", Tree => "tree",
    WaterBombCannon => "waterBombCannon", CannonBarrel => "cannonBarrel", WaterBomb => "waterBomb",
    WaterBombSpawner => "waterBombSpawner", WaterBombShadow => "waterBombShadow", Koopa => "koopa",
    KoopaShell => "koopaShell", BobombBuddy => "bobombBuddy", BobombBuddyAdvice => "bobombBuddyAdvice",
    Warp => "warp", PoisonGas => "poisonGas", Boo => "boo", BigBoo => "bigBoo", Flame => "flame",
    Penguin => "penguin", BabyPenguin => "babyPenguin", TuxiesMother => "tuxiesMother",
    RacingPenguin => "racingPenguin", SnowmanBody => "snowmanBody", SnowmanHead => "snowmanHead",
    // SM64 group 2 (courses 7-13): behaviors/g2/*.rs. One line per decomp behavior family.
    LllHexPlatform => "lllHexPlatform", LllHexRing => "lllHexRing", VolcanoFlames => "volcanoFlames",
    LllSinkRect => "lllSinkRect", LllSinkSquare => "lllSinkSquare", LllTiltPyramid => "lllTiltPyramid",
    LllOctMesh => "lllOctMesh", LllSinkRock => "lllSinkRock", RollingLogLll => "rollingLogLll",
    RollingLogTtm => "rollingLogTtm", VolcanoTrap => "volcanoTrap", LllWoodBridge => "lllWoodBridge",
    LllWoodPiece => "lllWoodPiece", LllDrawbridgeSpawner => "lllDrawbridgeSpawner",
    LllDrawbridge => "lllDrawbridge", LllFireBars => "lllFireBars", LllHexFlame => "lllHexFlame",
    Flamethrower => "flamethrower", FlamethrowerFlame => "flamethrowerFlame",
    BouncingFireball => "bouncingFireball", BouncingFireFlame => "bouncingFireFlame",
    BowserPuzzle => "bowserPuzzle", BowserPuzzlePiece => "bowserPuzzlePiece", Pole => "pole",
    PyramidTop => "pyramidTop", PyramidPillarDetector => "pyramidPillarDetector",
    PyramidTopFragment => "pyramidTopFragment", PyramidElevator => "pyramidElevator",
    PyramidWall => "pyramidWall", ToxBox => "toxBox", Grindel => "grindel",
    HorizontalGrindel => "horizontalGrindel", Spindel => "spindel",
    WaterLevelInit => "waterLevelInit", WaterLevelDiamond => "waterLevelDiamond", ArrowLift => "arrowLift",
    SquareFloatingPlatform => "squareFloatingPlatform", RectFloatingPlatform => "rectFloatingPlatform",
    ExpressElevator => "expressElevator", ExpressElevatorPlatform => "expressElevatorPlatform",
    RotatingPlatformWdw => "rotatingPlatformWdw", FloorSwitchHidden => "floorSwitchHidden", HiddenObject => "hiddenObject",
    ThiHugeIslandTop => "thiHugeIslandTop", ThiTinyIslandTop => "thiTinyIslandTop",
    FirePiranhaPlant => "firePiranhaPlant", SmallPiranhaFlame => "smallPiranhaFlame",
    CirclingAmp => "circlingAmp",
    DddPole => "dddPole", SushiShark => "sushiShark",
    MrBlizzard => "mrBlizzard", OneUp => "oneUp", Whomp => "whomp", WhompKing => "whompKing",
    Bully => "bully", BigBully => "bigBully", Bowser => "bowser", BowserMine => "bowserMine",
    BowserFlame => "bowserFlame", Klepto => "klepto", Pokey => "pokey", Chest => "chest",
    StarBox => "starBox", WingBox => "wingBox", WingCap => "wingCap", BlastWall => "blastWall",
    BkEnemy => "bkEnemy",
    SmBowser => "sm64Bowser", SmBowserBomb => "sm64BowserBomb", SmBowserFlame => "sm64BowserFlame", SmBowserShockWave => "sm64BowserShockWave",
    SmFlamethrower => "sm64Flamethrower", SmFlamethrowerFlame => "sm64FlamethrowerFlame",
    KoopaShellUnderwater => "koopaShellUnderwater", Whirlpool => "whirlpool",
    // SM64 group 1 (castle + first courses): behaviors/g1/*
    Thwomp => "thwomp", Thwomp2 => "thwomp2", PiranhaPlant => "piranhaPlant", BulletBill => "bulletBill",
    Swoop => "swoop", Scuttlebug => "scuttlebug", ScuttlebugSpawner => "scuttlebugSpawner", Spindrift => "spindrift",
    Mips => "mips", BooInCastle => "booInCastle", MontyMole => "montyMole", MontyMoleHole => "montyMoleHole",
    MontyMoleRock => "montyMoleRock", Snufit => "snufit", SnufitBall => "snufitBall", Butterfly => "butterfly", Bird => "bird",
    // SM64 round 2 part A (bosses and big enemies): behaviors/r2a/*
    EyerokBoss => "eyerokBoss", EyerokHand => "eyerokHand", WigglerHead => "wigglerHead", WigglerBody => "wigglerBody",
    MrI => "mrI", MrIBody => "mrIBody", MrIParticle => "mrIParticle",
    MerryGoRoundBooManager => "merryGoRoundBooManager", MerryGoRoundBoo => "merryGoRoundBoo", MerryGoRoundBigBoo => "merryGoRoundBigBoo",
    Dorrie => "dorrie", Unagi => "unagi", UnagiPart => "unagiPart", MantaRay => "mantaRay",
    JetStreamRingSpawner => "jetStreamRingSpawner", WaterRing => "waterRing",
    Tweester => "tweester", Hoot => "hoot",
    // SM64 round 2 part B (castle NPCs, coins, boxes, 1-ups, races): behaviors/part_b/*
    BlueCoinSwitch => "blueCoinSwitch", HiddenBlueCoin => "hiddenBlueCoin", MovingBlueCoin => "movingBlueCoin",
    RecoveryHeart => "recoveryHeart", ExclamationBox => "exclamationBox", OneUpWalking => "oneUpWalking",
    OneUpRunning => "oneUpRunning", OneUpSliding => "oneUpSliding", Hidden1Up => "hidden1Up",
    Hidden1UpTrigger => "hidden1UpTrigger", ToadMessage => "toadMessage", PssTimer => "pssTimer",
    WaterLevelPillar => "waterLevelPillar", MoatGrill => "moatGrill",
    // SM64 last stars (round 3: Ukiki and his cage, HMC rolling rocks): behaviors/r3/*
    Ukiki => "ukiki", UkikiCage => "ukikiCage", UkikiCageStar => "ukikiCageStar",
    BigBoulder => "bigBoulder", BigBoulderGenerator => "bigBoulderGenerator", BigChillBully => "bigChillBully",
    // SM64 BBH furniture (round 4): behaviors/r3/bbh.rs
    ChirpChirp => "chirpChirp", FireSpitter => "fireSpitter", FlyGuy => "flyGuy", Bub => "bub", CoffinSpawner => "coffinSpawner", Coffin => "coffin", Bubba => "bubba", TumblingBridge => "tumblingBridge", TumblingBridgePlatform => "tumblingBridgePlatform", Skeeter => "skeeter", SkeeterWave => "skeeterWave", MadPiano => "madPiano", HauntedChair => "hauntedChair", FlyingBookend => "flyingBookend", BookendSpawn => "bookendSpawn",
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectHitbox {
    pub interact_type: u32,
    pub down_offset: f64,
    pub damage_or_coin_value: f64,
    pub health: f64,
    pub num_loot_coins: f64,
    pub radius: f64,
    pub height: f64,
    pub hurtbox_radius: f64,
    pub hurtbox_height: f64,
}

impl Default for ObjectHitbox {
    fn default() -> Self {
        Self { interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 1.0, num_loot_coins: 0.0, radius: 50.0, height: 100.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BallRoute {
    pub waypoints: Vec<Vec3>,
    pub period: f64,
    pub max_distance: f64,
    /// Spawner: no ball while Mario is closer than this (1000 for BoB / TTM, 800 for THI).
    pub min_distance: f64,
    /// Spawner: a ball rolls when `(s32)(random_float() * odds) == 0` (2 for BoB, 1 for TTM, 1.5 for THI).
    pub odds: f64,
    /// `bhv_bowling_ball_initialize_loop`: the starting speed and `cur_obj_scale`.
    pub speed: f64,
    pub scale: f64,
}

impl BallRoute {
    /// The Bob-omb Battlefield / Tooth-less ball defaults (the path alone and the spawner's period and range).
    pub fn mountain(waypoints: Vec<Vec3>, period: f64, max_distance: f64) -> Self {
        Self { waypoints, period, max_distance, min_distance: 1000.0, odds: 2.0, speed: 20.0, scale: 1.0 }
    }
}

#[derive(Clone, Debug)]
pub struct ObjectSpec {
    pub ball_route: Option<BallRoute>,
    pub behavior: ObjectBehaviorKind,
    pub pos: Vec3,
    /// s16 angle.
    pub yaw: f64,
    pub param: Option<f64>,
    pub model: Option<String>,
    pub warp_to: Option<String>,
    pub warp_area: Option<i32>,
    pub warp_entry: Option<String>,
}

impl ObjectSpec {
    pub fn new(behavior: ObjectBehaviorKind, pos: Vec3, yaw: f64) -> Self {
        Self { ball_route: None, behavior, pos, yaw, param: None, model: None, warp_to: None, warp_area: None, warp_entry: None }
    }
}

#[derive(Clone, Debug)]
pub struct PenguinRace {
    pub path: Vec<[f64; 4]>,
    pub finish: Vec3,
    pub shortcut: Vec3,
}

// ---------------------------------------------------------------------------
// Hitbox helpers
// ---------------------------------------------------------------------------

/// `hitboxesOverlap` on (position, hitbox) pairs.
pub fn hitboxes_overlap(a: (&Vec3, &ObjectHitbox), b: (&Vec3, &ObjectHitbox)) -> bool {
    let (apos, ah) = a;
    let (bpos, bh) = b;
    let a_bottom = apos[1] - ah.down_offset;
    let b_bottom = bpos[1] - bh.down_offset;
    if a_bottom > b_bottom + bh.height { return false; }
    if a_bottom + ah.height < b_bottom { return false; }
    let dx = apos[0] - bpos[0];
    let dz = apos[2] - bpos[2];
    let radius = ah.radius + bh.radius;
    dx * dx + dz * dz < radius * radius
}

pub fn mario_hitbox(m: &MarioRuntime) -> ObjectHitbox {
    let short = m.action & ACT_FLAG_SHORT_HITBOX != 0;
    ObjectHitbox {
        interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0,
        radius: 37.0, height: if short { 100.0 } else { 160.0 }, hurtbox_radius: 0.0, hurtbox_height: 0.0,
    }
}

pub fn mario_overlaps_object(m: &MarioRuntime, object: &SimObject) -> bool {
    hitboxes_overlap((&m.pos, &mario_hitbox(m)), (&object.pos, &object.hitbox))
}

/// determine_interaction (interaction.c).
pub fn detect_interaction(m: &MarioRuntime, object: &SimObject) -> i32 {
    let action = m.action;
    let mut interaction = 0;

    if action & ACT_FLAG_ATTACKING != 0 {
        if action == ACT_PUNCHING || action == ACT_MOVE_PUNCHING || action == ACT_JUMP_KICK {
            let d_yaw = to_s16(atan2s(object.pos[2] - m.pos[2], object.pos[0] - m.pos[0]) as f64 - m.face_angle[1]);
            if m.flags & MARIO_PUNCHING != 0 && (-0x2aaa..=0x2aaa).contains(&d_yaw) { interaction = ATTACK_PUNCH; }
            if m.flags & MARIO_KICKING != 0 && (-0x2aaa..=0x2aaa).contains(&d_yaw) { interaction = ATTACK_KICK_OR_TRIP; }
            if m.flags & MARIO_TRIPPING != 0 && (-0x4000..=0x4000).contains(&d_yaw) { interaction = ATTACK_KICK_OR_TRIP; }
        } else if action == ACT_GROUND_POUND {
            if m.vel[1] < 0.0 { interaction = ATTACK_GROUND_POUND_OR_TWIRL; }
        } else if action == ACT_GROUND_POUND_LAND {
            interaction = ATTACK_GROUND_POUND_OR_TWIRL;
        } else if action == ACT_SLIDE_KICK || action == ACT_SLIDE_KICK_SLIDE {
            interaction = ATTACK_FAST_ATTACK;
        } else if m.forward_vel <= -26.0 || m.forward_vel >= 26.0 {
            interaction = ATTACK_FAST_ATTACK;
        }
    }

    if interaction == 0 && action & ACT_FLAG_AIR != 0 {
        if m.vel[1] < 0.0 {
            if m.pos[1] > object.pos[1] { interaction = ATTACK_FROM_ABOVE; }
        } else if m.pos[1] < object.pos[1] {
            interaction = ATTACK_FROM_BELOW;
        }
    }
    interaction
}

pub fn attack_object(object: &mut SimObject, interaction: i32) -> i32 {
    let attack_type = match interaction {
        ATTACK_GROUND_POUND_OR_TWIRL | ATTACK_PUNCH | ATTACK_KICK_OR_TRIP | ATTACK_FAST_ATTACK | ATTACK_FROM_ABOVE | ATTACK_FROM_BELOW => interaction,
        _ => 0,
    };
    object.interact_status = attack_type as u32 + (INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED);
    attack_type
}

/// push_mario_out_of_object (interaction.c).
pub fn push_mario_out_of_object(m: &mut MarioRuntime, id: ObjId, padding: f64) {
    let (opos, oradius) = { let o = &m.objects[id]; (o.pos, o.hitbox.radius) };
    let radius = mario_hitbox(m).radius;
    let min_distance = oradius + radius + padding;
    let offset_x = m.pos[0] - opos[0];
    let offset_z = m.pos[2] - opos[2];
    let distance = js_hypot(offset_x, offset_z);
    if distance >= min_distance { return; }

    let push_angle = if distance == 0.0 { m.face_angle[1] } else { atan2s(offset_z, offset_x) as f64 };
    let mut next = [opos[0] + min_distance * sins(push_angle), m.pos[1], opos[2] + min_distance * coss(push_angle)];
    m.world.f32_find_wall_collision(&mut next, 60.0, 50.0);
    let floor = m.world.find_floor(next[0], next[1], next[2]);
    if floor.surface.is_some() {
        m.pos[0] = next[0];
        m.pos[2] = next[2];
    }
}

// ---------------------------------------------------------------------------
// SimObject
// ---------------------------------------------------------------------------

static NEXT_OBJECT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug)]
pub struct SimObject {
    /// Globally unique id (the TS `nextObjectId` counter); the arena index is the `ObjId`.
    pub id: u32,
    pub ball_route: Option<BallRoute>,
    pub behavior: ObjectBehaviorKind,
    pub model: Option<String>,
    pub warp_to: Option<String>,
    pub warp_area: Option<i32>,
    pub warp_entry: Option<String>,
    pub param: f64,

    pub pos: Vec3,
    pub home: Vec3,
    pub vel: Vec3,
    pub move_yaw: f64,
    pub face_yaw: f64,
    pub forward_vel: f64,
    pub gravity: f64,
    pub friction: f64,
    pub buoyancy: f64,

    pub hitbox: ObjectHitbox,

    pub interact_status: u32,
    pub interact_subtype: u32,
    pub held_state: i32,
    pub action: i32,
    pub prev_action: i32,
    pub action_state: f64,
    pub action_timer: f64,
    pub timer: f64,
    pub scale: f64,
    pub graph_y_offset: f64,
    pub billboard: bool,
    pub visible: bool,
    pub active: bool,
    pub respawn_distance: f64,
    pub respawn_timer: f64,
    /// Respawner copies the native behavior parameter that records a paid coin.
    pub coin_dropped: bool,
    pub distance_to_mario: f64,
    pub angle_to_mario: f64,
    pub grounded: bool,
    pub hit_wall: bool,
    /// Behavior-local scalar state, mirroring the per-behavior `o*` fields (TS `vars`).
    /// A missing key reads as 0 through [`SimObject::var`]; use `has_var` to tell it apart.
    pub vars: HashMap<String, f64>,
    /// Runtime links used by compound objects such as the Chain Chomp.
    pub link: Option<ObjId>,
    pub links: Option<Vec<ObjId>>,
    pub link_index: Option<f64>,
    pub chain_pivot: Option<Vec3>,
    pub chain_segments: Option<Vec<Vec3>>,
    pub collision_surfaces: Option<Vec<Arc<Surface>>>,
}

impl SimObject {
    pub fn new(spec: &ObjectSpec) -> Self {
        Self {
            id: NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed),
            ball_route: spec.ball_route.clone(),
            behavior: spec.behavior,
            model: spec.model.clone(),
            warp_to: spec.warp_to.clone(),
            warp_area: spec.warp_area,
            warp_entry: spec.warp_entry.clone(),
            param: spec.param.unwrap_or(0.0),
            pos: spec.pos,
            home: spec.pos,
            vel: [0.0; 3],
            move_yaw: spec.yaw,
            face_yaw: spec.yaw,
            forward_vel: 0.0,
            gravity: 0.0,
            friction: 1.0,
            buoyancy: 1.0,
            hitbox: ObjectHitbox::default(),
            interact_status: 0,
            interact_subtype: 0,
            held_state: HELD_FREE,
            action: 0,
            prev_action: 0,
            action_state: 0.0,
            action_timer: 0.0,
            timer: 0.0,
            scale: 1.0,
            graph_y_offset: 0.0,
            billboard: false,
            visible: true,
            active: true,
            respawn_distance: -1.0,
            respawn_timer: -1.0,
            coin_dropped: false,
            distance_to_mario: f64::INFINITY,
            angle_to_mario: 0.0,
            grounded: false,
            hit_wall: false,
            vars: HashMap::new(),
            link: None,
            links: None,
            link_index: None,
            chain_pivot: None,
            chain_segments: None,
            collision_surfaces: None,
        }
    }

    /// `vars[key]`, 0 when unset (TS `undefined` is falsy and `|| 0`-style in the behaviors).
    pub fn var(&self, key: &str) -> f64 { self.vars.get(key).copied().unwrap_or(0.0) }
    pub fn has_var(&self, key: &str) -> bool { self.vars.contains_key(key) }
    pub fn set_var(&mut self, key: &str, value: f64) { self.vars.insert(key.to_string(), value); }

    pub fn set_hitbox(&mut self, hitbox: ObjectHitbox) { self.hitbox = hitbox; }

    pub fn set_action(&mut self, action: i32) {
        self.prev_action = self.action;
        self.action = action;
        self.action_state = 0.0;
        self.action_timer = 0.0;
    }

    pub fn set_held_state(&mut self, state: i32) { self.held_state = state; }

    pub fn deactivate(&mut self, respawn_frames: f64) {
        self.active = false;
        self.visible = false;
        self.respawn_timer = respawn_frames;
        self.respawn_distance = -1.0;
    }

    pub fn wait_to_respawn(&mut self, min_distance: f64) {
        self.deactivate(-1.0);
        self.respawn_distance = min_distance;
    }

    pub fn reactivate(&mut self) {
        self.active = true;
        self.visible = true;
        self.respawn_timer = -1.0;
        self.respawn_distance = -1.0;
        self.pos = self.home;
        self.vel = [0.0; 3];
        self.forward_vel = 0.0;
        self.move_yaw = self.face_yaw;
        self.held_state = HELD_FREE;
        self.interact_status = 0;
        self.action_state = 0.0;
        self.action_timer = 0.0;
        self.timer = 0.0;
        self.scale = 1.0;
        self.grounded = false;
        self.hit_wall = false;
        self.vars.clear();
    }

    /// object_step (obj_behaviors.c): walls, floor, gravity, friction and water.
    /// `wall_radius` defaults to the hitbox radius. Returns the source collision flags.
    pub fn step_physics(&mut self, world: &CollisionWorld, wall_radius: Option<f64>) -> u32 {
        let wall_radius = wall_radius.unwrap_or(self.hitbox.radius);
        let mut result = 0;
        let vel_x = self.forward_vel * sins(self.move_yaw);
        let vel_z = self.forward_vel * coss(self.move_yaw);
        let mut next = [self.pos[0] + vel_x, self.pos[1], self.pos[2] + vel_z];

        let floor = world.find_floor(next[0], next[1], next[2]);
        let water_y = world.water_level(next[0], next[2]);
        let wall = world.resolve_and_return_wall_collisions(&mut next, self.hitbox.height / 2.0, wall_radius);
        if let Some(wall) = wall {
            self.pos[0] = next[0];
            self.pos[2] = next[2];
            self.turn_away_from_surface(vel_x, vel_z, &wall);
            result |= OBJ_COL_FLAG_HIT_WALL;
        }
        let floor_y = if floor.surface.is_some() { floor.height } else { FLOOR_LOWER_LIMIT };
        match &floor.surface {
            None => {
                self.move_yaw = to_s16(self.move_yaw + 0x7fff as f64) as f64;
                result |= OBJ_COL_FLAG_HIT_WALL;
            }
            Some(s) if s.normal[1] < 0.5 && floor_y > self.pos[1] => {
                self.turn_away_from_surface(vel_x, vel_z, s);
                result |= OBJ_COL_FLAG_HIT_WALL;
            }
            Some(s) if water_y > self.pos[1] => {
                self.step_underwater(s, floor_y, vel_x, vel_z);
                result |= OBJ_COL_FLAG_UNDERWATER;
            }
            Some(s) => self.step_on_floor(s, floor_y, vel_x, vel_z),
        }

        self.pos[0] += self.forward_vel * sins(self.move_yaw);
        self.pos[2] += self.forward_vel * coss(self.move_yaw);
        if self.pos[1].trunc() == floor_y.trunc() { result |= OBJ_COL_FLAG_GROUNDED; }
        if self.vel[1].trunc() == 0.0 { result |= OBJ_COL_FLAG_NO_Y_VEL; }
        self.grounded = result & OBJ_COL_FLAG_GROUNDED != 0;
        self.hit_wall = result & OBJ_COL_FLAG_HIT_WALL != 0;
        result
    }

    fn slope_push(&mut self, surface: &Surface, floor_y: f64, mut vel_x: f64, mut vel_z: f64, accel: f64) {
        if self.pos[1].trunc() >= floor_y.trunc() && self.pos[1].trunc() < floor_y.trunc() + 37.0 {
            let (nx, ny, nz) = (surface.normal[0], surface.normal[1], surface.normal[2]);
            let denom = nx * nx + ny * ny + nz * nz;
            if denom > 0.0 {
                vel_x += (nx * (nx * nx + nz * nz) / denom) * accel * 2.0;
                vel_z += (nz * (nx * nx + nz * nz) / denom) * accel * 2.0;
            }
            if vel_x.abs() < 0.000001 { vel_x = 0.0; }
            if vel_z.abs() < 0.000001 { vel_z = 0.0; }
            if vel_x != 0.0 || vel_z != 0.0 { self.move_yaw = atan2s(vel_z, vel_x) as f64; }
            let friction = if ny < 0.2 && self.friction < 0.9999 { 0.0 } else { self.friction };
            self.forward_vel = js_hypot(vel_x, vel_z) * friction;
        }
    }

    fn step_on_floor(&mut self, surface: &Surface, floor_y: f64, vel_x: f64, vel_z: f64) {
        self.vel[1] -= self.gravity;
        self.vel[1] = (-75f64).max(75f64.min(self.vel[1]));
        self.pos[1] += self.vel[1];
        if self.pos[1] < floor_y {
            self.pos[1] = floor_y;
            self.vel[1] = if self.vel[1] < -17.5 { -(self.vel[1] / 2.0) } else { 0.0 };
        }
        let g = self.gravity;
        self.slope_push(surface, floor_y, vel_x, vel_z, g);
    }

    fn step_underwater(&mut self, surface: &Surface, floor_y: f64, vel_x: f64, vel_z: f64) {
        let net_y_accel = (1.0 - self.buoyancy) * -self.gravity;
        self.vel[1] -= net_y_accel;
        self.vel[1] = (-75f64).max(75f64.min(self.vel[1]));
        self.pos[1] += self.vel[1];
        if self.pos[1] < floor_y {
            self.pos[1] = floor_y;
            self.vel[1] = if self.vel[1] < -17.5 { -(self.vel[1] / 2.0) } else { 0.0 };
        }
        self.slope_push(surface, floor_y, vel_x, vel_z, net_y_accel);
    }

    fn turn_away_from_surface(&mut self, vel_x: f64, vel_z: f64, surface: &Surface) {
        let nx = surface.normal[0];
        let nz = surface.normal[2];
        let denom = nx * nx + nz * nz;
        if denom < 0.000001 {
            // JS adds without wrapping; the yaw is wrapped by whoever reads it next.
            self.move_yaw += 0x7fff as f64;
            return;
        }
        let yaw_x = ((nz * nz - nx * nx) * vel_x) / denom - (2.0 * vel_z * (nx * nz)) / denom;
        let yaw_z = ((nx * nx - nz * nz) * vel_z) / denom - (2.0 * vel_x * (nx * nz)) / denom;
        self.move_yaw = atan2s(yaw_z, yaw_x) as f64;
    }

    /// obj_check_if_facing_toward_angle
    pub fn facing_toward_angle(&self, base: f64, goal: f64, range: f64) -> bool {
        let d_angle = (to_u16(goal) - to_u16(base)) as f64;
        sins(-range) < sins(d_angle) && sins(d_angle) < sins(range) && coss(d_angle) > 0.0
    }

    /// obj_turn_toward_object
    pub fn turn_toward_angle(&mut self, goal: f64, turn_amount: f64) {
        let shortest = to_s16(goal - self.move_yaw) as f64;
        if shortest.abs() <= turn_amount {
            self.move_yaw = goal;
        } else {
            self.move_yaw += if shortest > 0.0 { turn_amount } else { -turn_amount };
        }
    }
}

/// obj_return_home_if_safe (obj_behaviors.c).
pub fn return_home_if_safe(object: &mut SimObject, mario: &MarioRuntime, distance: f64) -> bool {
    return_home_if_safe_at(object, mario.flags, mario.pos, distance)
}

/// [`return_home_if_safe`] with Mario's flags and position passed by value, so a behavior can
/// call it while holding `&mut m.objects[id]`.
pub fn return_home_if_safe_at(object: &mut SimObject, mario_flags: u32, mario_pos: Vec3, distance: f64) -> bool {
    if mario_flags & MARIO_VANISH_CAP != 0 { return false; }
    let dx = object.home[0] - object.pos[0];
    let dz = object.home[2] - object.pos[2];
    let mdx = object.home[0] - mario_pos[0];
    let mdy = object.home[1] - mario_pos[1];
    let mdz = object.home[2] - mario_pos[2];
    if mdx * mdx + mdy * mdy + mdz * mdz < distance * distance { return true; }
    object.turn_toward_angle(atan2s(dz, dx) as f64, 320.0);
    false
}
