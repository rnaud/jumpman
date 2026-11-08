//! Faithful port of `src/game/behaviors/bowser.inc.c` (Bowser in the Dark World / Fire Sea / Sky
//! arenas), `bowser_bomb.inc.c` and `bowser_flame.inc.c`, plus the `cur_obj_move_standard`
//! physics (`object_helpers.c`) Bowser relies on.
//!
//! This is a separate behavior kind (`sm64Bowser`) so the simplified `bowser` kind and its golden
//! scenario in `tests/bob_behaviors.rs` keep working. Differences from the game, all listed in
//! `DECOMP_TALLY.md`: `f64` instead of `f32`; the defeat dialog, camera events, eye/rainbow geo
//! switches, sounds and the small particle puffs are not ported; the sky flame (`bhvBlueBowserFlame`)
//! lands as ground flames directly; Bowser's tail is the root-relative approximation the
//! Bob-omb/Bowser golden already used (the real one follows the animated joint).
use crate::behaviors::bowser_flames_data::BOWSER_FLAMES_ORIENTATION;
use crate::collision::{CollisionWorld, FLOOR_LOWER_LIMIT};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::object_manager;
use crate::solids::random_float;
use crate::surfaces::*;

// ---- object_constants.h ---------------------------------------------------------------------
pub const ACT_DEFAULT: i32 = 0;
pub const ACT_THROWN: i32 = 1;
pub const ACT_JUMP_ONTO_STAGE: i32 = 2;
pub const ACT_DANCE: i32 = 3;
pub const ACT_DEAD: i32 = 4;
pub const ACT_WAIT: i32 = 5;
pub const ACT_INTRO_WALK: i32 = 6;
pub const ACT_CHARGE_MARIO: i32 = 7;
pub const ACT_SPIT_FIRE_INTO_SKY: i32 = 8;
pub const ACT_SPIT_FIRE_ONTO_FLOOR: i32 = 9;
pub const ACT_HIT_EDGE: i32 = 10;
pub const ACT_TURN_FROM_EDGE: i32 = 11;
pub const ACT_HIT_MINE: i32 = 12;
pub const ACT_BIG_JUMP: i32 = 13;
pub const ACT_WALK_TO_MARIO: i32 = 14;
pub const ACT_BREATH_FIRE: i32 = 15;
pub const ACT_TELEPORT: i32 = 16;
pub const ACT_QUICK_JUMP: i32 = 17;
pub const ACT_IDLE: i32 = 18;
pub const ACT_TILT_LAVA_PLATFORM: i32 = 19;

pub const BP_BITDW: i32 = 0;
pub const BP_BITFS: i32 = 1;
pub const BP_BITS: i32 = 2;

pub const STATUS_ANGLE_MARIO: u32 = 1 << 1;
pub const STATUS_ANGLE_CENTER: u32 = 1 << 2;
pub const STATUS_DIST_MARIO: u32 = 1 << 3;
pub const STATUS_DIST_CENTER: u32 = 1 << 4;
pub const STATUS_BIG_JUMP: u32 = 1 << 16;
pub const STATUS_FIRE_SKY: u32 = 1 << 17;

pub const GRAB_NONE: i32 = 0;
pub const GRAB_GRABBED: i32 = 1;
pub const GRAB_HOLDING: i32 = 2;

pub const SUB_DEAD_FLY_BACK: i32 = 0;
pub const SUB_DEAD_BOUNCE: i32 = 1;
pub const SUB_DEAD_WAIT: i32 = 2;
pub const SUB_DEAD_DEFAULT_END: i32 = 3;
pub const SUB_DEAD_DEFAULT_END_OVER: i32 = 4;
pub const SUB_DEAD_FINAL_END: i32 = 10;
pub const SUB_DEAD_FINAL_END_OVER: i32 = 11;

pub const SUB_CHARGE_START: i32 = 0;
pub const SUB_CHARGE_RUN: i32 = 1;
pub const SUB_CHARGE_END: i32 = 2;
pub const SUB_CHARGE_SLIP: i32 = 3;

pub const SUB_JUMP_ON_STAGE_IDLE: i32 = 0;
pub const SUB_JUMP_ON_STAGE_START: i32 = 1;
pub const SUB_JUMP_ON_STAGE_LAND: i32 = 2;
pub const SUB_JUMP_ON_STAGE_STOP: i32 = 3;

// ---- Bowser animation enum (`BOWSER_ANIM_*`) -------------------------------------------------
pub const ANIM_STAND_UP: i32 = 0;
pub const ANIM_SHAKING: i32 = 2;
pub const ANIM_GRABBED: i32 = 3;
pub const ANIM_BREATH: i32 = 6;
pub const ANIM_JUMP_STOP: i32 = 8;
pub const ANIM_JUMP_START: i32 = 9;
pub const ANIM_DANCE: i32 = 10;
pub const ANIM_BREATH_UP: i32 = 11;
pub const ANIM_IDLE: i32 = 12;
pub const ANIM_SLOW_GAIT: i32 = 13;
pub const ANIM_LOOK_DOWN_STOP_WALK: i32 = 14;
pub const ANIM_LOOK_UP_START_WALK: i32 = 15;
pub const ANIM_FLIP_DOWN: i32 = 16;
pub const ANIM_LAY_DOWN: i32 = 17;
pub const ANIM_RUN_START: i32 = 18;
pub const ANIM_RUN: i32 = 19;
pub const ANIM_RUN_STOP: i32 = 20;
pub const ANIM_RUN_SLIP: i32 = 21;
pub const ANIM_BREATH_QUICK: i32 = 22;
pub const ANIM_EDGE_MOVE: i32 = 23;
pub const ANIM_EDGE_STOP: i32 = 24;
pub const ANIM_FLIP: i32 = 25;
pub const ANIM_STAND_UP_FROM_FLIP: i32 = 26;

/// `loopEnd` of each `bowser_seg6_anims_06057690` entry (from `assets/animations.json`).
pub const ANIM_LOOP_END: [i32; 27] = [44, 40, 30, 36, 100, 76, 92, 45, 20, 16, 70, 60, 80, 42, 22, 44, 20, 30, 23, 19, 18, 17, 20, 27, 15, 45, 44];
/// Animations with `ANIM_FLAG_NOLOOP`.
pub fn anim_noloop(id: i32) -> bool { matches!(id, 9 | 16 | 17) }

// ---- OBJ_MOVE_* -------------------------------------------------------------------------------
pub const MOVE_LANDED: u32 = 1 << 0;
pub const MOVE_ON_GROUND: u32 = 1 << 1;
pub const MOVE_LEFT_GROUND: u32 = 1 << 2;
pub const MOVE_IN_AIR: u32 = 1 << 7;
pub const MOVE_HIT_WALL: u32 = 1 << 9;
pub const MOVE_HIT_EDGE: u32 = 1 << 10;
pub const MOVE_ABOVE_LAVA: u32 = 1 << 11;
pub const MOVE_BOUNCE: u32 = 1 << 13;
pub const MOVE_ABOVE_DEATH_BARRIER: u32 = 1 << 14;
pub const FLOOR_LOWER_LIMIT_MISC: f64 = FLOOR_LOWER_LIMIT + 1000.0;

/// `sBowsertiltPlatformData`: { flag, angSpeed, time }.
pub const TILT_PLATFORM_DATA: [(i32, i32, i32); 12] = [
    (1, 10, 40), (0, 0, 74), (-1, -10, 114), (1, -20, 134), (-1, 20, 154), (1, 40, 164),
    (-1, -40, 174), (1, -80, 179), (-1, 80, 184), (1, 160, 186), (-1, -160, 186), (1, 0, 0),
];
/// `sBowserHealth`.
pub const HEALTH: [f64; 3] = [1.0, 1.0, 3.0];

fn bowser_hitbox() -> ObjectHitbox {
    ObjectHitbox { interact_type: INTERACT_GRABBABLE, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 50.0, radius: 400.0, height: 400.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 }
}

// ---------------------------------------------------------------------------
// Object helpers (object_helpers.c)
// ---------------------------------------------------------------------------

/// `abs_angle_diff`.
pub fn abs_angle_diff(x0: f64, x1: f64) -> f64 {
    let mut diff = to_s16(x1 - x0);
    if diff == -0x8000 { diff = -0x7FFF; }
    (diff as f64).abs()
}

fn flags(o: &SimObject) -> u32 { o.var("moveFlags") as u32 }
fn set_flags(o: &mut SimObject, f: u32) { o.set_var("moveFlags", f as f64); }

/// SM64 `oTimer`/`oSubAction` bookkeeping around one update (`cur_obj_update`).
fn begin_frame(o: &mut SimObject) {
    if o.action != o.prev_action {
        o.set_var("oTimer", 0.0);
        o.action_state = 0.0;
        o.prev_action = o.action;
    }
}
fn end_frame(o: &mut SimObject) {
    let t = o.var("oTimer") + 1.0;
    o.set_var("oTimer", t);
    if o.action != o.prev_action {
        o.set_var("oTimer", 0.0);
        o.action_state = 0.0;
        o.prev_action = o.action;
    }
}
fn otimer(o: &SimObject) -> f64 { o.var("oTimer") }
fn sub(o: &SimObject) -> i32 { o.action_state as i32 }
fn sub_inc(o: &mut SimObject) { o.action_state += 1.0; }
fn health(o: &SimObject) -> f64 { o.var("health") }
fn bp(o: &SimObject) -> i32 { o.var("bp") as i32 }
fn status(o: &SimObject) -> u32 { o.var("status") as u32 }
fn set_status(o: &mut SimObject, s: u32) { o.set_var("status", s as f64); }

// ---- animation (geo_obj_init_animation / geo_update_animation_frame) ----

/// `cur_obj_init_animation_with_sound`.
fn anim_init(o: &mut SimObject, id: i32) {
    if !o.has_var("animId") || o.var("animId") as i32 != id {
        o.set_var("animId", id as f64);
        o.set_var("animation", id as f64);
        o.set_var("animFrame", -1.0);
    }
    o.set_var("soundState", id as f64);
}
fn anim_frame(o: &SimObject) -> i32 { o.var("animFrame") as i32 }
fn anim_id(o: &SimObject) -> i32 { o.var("animId") as i32 }
/// `cur_obj_check_anim_frame`.
fn check_anim_frame(o: &SimObject, f: i32) -> bool { anim_frame(o) == f }
/// `cur_obj_check_if_near_animation_end`.
fn near_end(o: &SimObject) -> bool {
    let id = anim_id(o);
    let near_loop_end = ANIM_LOOP_END[id as usize] - 2;
    let f = anim_frame(o);
    (anim_noloop(id) && near_loop_end + 1 == f) || f == near_loop_end
}
/// `cur_obj_init_animation_and_check_if_near_end`.
fn anim_init_near_end(o: &mut SimObject, id: i32) -> bool {
    anim_init(o, id);
    near_end(o)
}
/// `cur_obj_extend_animation_if_at_end`.
fn extend_if_at_end(o: &mut SimObject) {
    let f = anim_frame(o);
    if f == ANIM_LOOP_END[anim_id(o) as usize] - 2 {
        o.set_var("animFrame", (f - 1) as f64);
    }
}
/// `geo_update_animation_frame` (once per frame after the behavior ran).
fn advance_anim(o: &mut SimObject) {
    if !o.has_var("animId") { return; }
    let id = anim_id(o);
    let end = ANIM_LOOP_END[id as usize];
    let mut next = anim_frame(o) + 1;
    if next >= end {
        next = if anim_noloop(id) { end - 1 } else { 0 };
    }
    o.set_var("animFrame", next as f64);
}

// ---- floor / movement ----

struct Floor {
    height: f64,
    kind: i32,
    dynamic: bool,
    object: Option<u32>,
    normal_y: f64,
}

fn floor_at(world: &CollisionWorld, x: f64, y: f64, z: f64) -> Floor {
    let hit = world.find_floor(x, y, z);
    match hit.surface {
        Some(s) => Floor { height: hit.height, kind: s.kind, dynamic: s.flags & SURFACE_FLAG_DYNAMIC != 0, object: s.object, normal_y: s.normal[1] },
        None => Floor { height: FLOOR_LOWER_LIMIT, kind: 0, dynamic: false, object: None, normal_y: 1.0 },
    }
}

/// `cur_obj_update_floor_and_walls` for an object with `oWallHitboxRadius == 0`.
fn update_floor_and_walls(o: &mut SimObject, world: &CollisionWorld) {
    let mut f = flags(o) & !(MOVE_ABOVE_LAVA | MOVE_ABOVE_DEATH_BARRIER | MOVE_HIT_WALL);
    let floor = floor_at(world, o.pos[0], o.pos[1], o.pos[2]);
    o.set_var("floorHeight", floor.height);
    o.set_var("floorType", floor.kind as f64);
    o.set_var("floorDynamic", floor.dynamic as u8 as f64);
    if floor.kind == SURFACE_BURNING { f |= MOVE_ABOVE_LAVA; }
    if floor.kind == SURFACE_DEATH_PLANE { f |= MOVE_ABOVE_DEATH_BARRIER; }
    if o.pos[1] > floor.height { f |= MOVE_IN_AIR; }
    // cur_obj_detect_steep_floor(60)
    if o.forward_vel != 0.0 {
        let steep_normal_y = coss((60 * (0x10000 / 360)) as f64);
        let ix = o.pos[0] + o.vel[0];
        let iz = o.pos[2] + o.vel[2];
        let intended = floor_at(world, ix, o.pos[1], iz);
        let delta = intended.height - floor.height;
        if intended.height < FLOOR_LOWER_LIMIT_MISC {
            f |= MOVE_HIT_WALL;
        } else if intended.normal_y < steep_normal_y && delta > 0.0 && intended.height > o.pos[1] {
            f |= MOVE_HIT_WALL;
        }
    }
    set_flags(o, f);
}

fn apply_drag_to_value(value: &mut f64, drag: f64) {
    if *value != 0.0 {
        let decel = *value * *value * (drag * 0.0001);
        if *value > 0.0 {
            *value -= decel;
            if *value < 0.001 { *value = 0.0; }
        } else {
            *value += decel;
            if *value > -0.001 { *value = 0.0; }
        }
    }
}

/// `cur_obj_move_standard(-78)` with Bowser's physics (gravity -4, bounciness -0.7, drag 10).
fn move_standard(o: &mut SimObject, world: &CollisionWorld) {
    let (gravity, bounciness, drag) = (-4.0, -0.7, 10.0);
    let steep_normal_y = coss((78 * (0x10000 / 360)) as f64);
    let negative = o.forward_vel < 0.0;
    o.vel[0] = o.forward_vel * sins(o.move_yaw);
    o.vel[2] = o.forward_vel * coss(o.move_yaw);
    let mut vx = o.vel[0];
    let mut vz = o.vel[2];
    apply_drag_to_value(&mut vx, drag);
    apply_drag_to_value(&mut vz, drag);
    o.vel[0] = vx;
    o.vel[2] = vz;

    // cur_obj_move_xz(steepNormalY, careAboutEdges = true)
    let mut f = flags(o) & !MOVE_HIT_EDGE;
    let floor_h = o.var("floorHeight");
    let ix = o.pos[0] + o.vel[0];
    let iz = o.pos[2] + o.vel[2];
    let intended = floor_at(world, ix, o.pos[1], iz);
    let delta = intended.height - floor_h;
    if intended.height < FLOOR_LOWER_LIMIT_MISC {
        f |= MOVE_HIT_EDGE;
    } else if delta < 5.0 {
        if delta < -50.0 && f & MOVE_ON_GROUND != 0 {
            f |= MOVE_HIT_EDGE;
        } else if intended.normal_y > steep_normal_y {
            o.pos[0] = ix;
            o.pos[2] = iz;
        } else {
            f |= MOVE_HIT_EDGE;
        }
    } else if intended.normal_y > steep_normal_y || o.pos[1] > intended.height {
        o.pos[0] = ix;
        o.pos[2] = iz;
    }

    // cur_obj_move_y(gravity, bounciness, buoyancy)
    f &= !MOVE_LEFT_GROUND;
    o.vel[1] += gravity;
    if o.vel[1] < -78.0 { o.vel[1] = -78.0; }
    o.pos[1] += o.vel[1];
    // cur_obj_move_update_ground_air_flags
    f &= !MOVE_BOUNCE;
    if o.pos[1] < floor_h {
        if f & MOVE_ON_GROUND == 0 {
            if f & MOVE_LANDED != 0 {
                f &= !MOVE_LANDED;
                f |= MOVE_ON_GROUND;
            } else {
                f |= MOVE_LANDED;
            }
        }
        o.pos[1] = floor_h;
        if o.vel[1] < 0.0 { o.vel[1] *= bounciness; }
        if o.vel[1] > 5.0 { f |= MOVE_BOUNCE; }
    } else {
        f &= !MOVE_LANDED;
        if f & MOVE_ON_GROUND != 0 {
            f &= !MOVE_ON_GROUND;
            f |= MOVE_LEFT_GROUND;
        }
    }
    if f & (MOVE_LANDED | MOVE_ON_GROUND) != 0 { f &= !MOVE_IN_AIR; } else { f |= MOVE_IN_AIR; }
    set_flags(o, f);

    o.forward_vel = js_hypot(o.vel[0], o.vel[2]);
    if negative { o.forward_vel = -o.forward_vel; }
}

/// `cur_obj_rotate_yaw_toward`.
fn rotate_yaw_toward(o: &mut SimObject, target: f64, inc: f64) -> bool {
    let start = to_s16(o.move_yaw) as f64;
    o.move_yaw = approach_s16(o.move_yaw, target, inc);
    o.set_var("angleVelYaw", to_s16(o.move_yaw) as f64 - start);
    to_s16(o.move_yaw) == to_s16(target)
}

/// `approach_s16_symmetric` (object_helpers.c).
pub fn approach_s16(value: f64, target: f64, inc: f64) -> f64 {
    let value = to_s16(value) as f64;
    let target = to_s16(target) as f64;
    let dist = to_s16(target - value) as f64;
    let out = if dist >= 0.0 { if dist > inc { value + inc } else { target } } else if dist < -inc { value - inc } else { target };
    to_s16(out) as f64
}

/// `approach_f32_signed` (object_helpers.c).
fn approach_f32_signed(value: &mut f64, target: f64, inc: f64) -> bool {
    let mut reached = false;
    *value += inc;
    if inc >= 0.0 {
        if *value > target { *value = target; reached = true; }
    } else if *value < target {
        *value = target;
        reached = true;
    }
    reached
}

/// `cur_obj_forward_vel_approach_upward`.
fn forward_vel_approach_upward(o: &mut SimObject, target: f64, inc: f64) {
    if o.forward_vel >= target {
        o.forward_vel = target;
    } else {
        o.forward_vel += inc;
    }
}

// ---------------------------------------------------------------------------
// Effects an action wants to cause outside the object
// ---------------------------------------------------------------------------

enum Effect {
    /// `obj_spit_fire` / breath flame: (position, yaw, pitch, kind)
    Flame { pos: Vec3, yaw: f64, pitch: f64, kind: f64 },
    Shockwave { pos: Vec3 },
    MarioKnockback { pos: Vec3 },
    HitMine(ObjId),
    Defeated(Vec3),
}

struct Ctx<'a> {
    world: &'a CollisionWorld,
    mario_pos: Vec3,
    mario_yaw: f64,
    mario_wedges: f64,
    mines: Vec<(ObjId, Vec3)>,
    out: Vec<Effect>,
    /// `gDebugInfo`-free platform handle: tilt data written for `TiltingBowserLavaPlatform`.
    dist_to_mario: f64,
    angle_to_mario: f64,
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

/// `bhv_bowser_init` + the behavior script header (`SET_HITBOX(400, 400)`, physics).
/// `param` carries the arena (`gCurrLevelNum`): 0 = Bowser 1, 1 = Bowser 2, 2 = Bowser 3.
pub fn init_bowser(o: &mut SimObject) {
    let level = (o.param as i32).clamp(0, 2);
    o.set_hitbox(bowser_hitbox());
    o.gravity = -4.0;
    o.set_var("bp", level as f64);
    o.set_var("opacity", 255.0);
    o.set_var("targetOpacity", 255.0);
    o.set_var("camAct", 1.0);
    o.set_var("rainbow", if level == BP_BITS { 1.0 } else { 0.0 });
    o.set_var("health", HEALTH[level as usize]);
    o.set_var("isReacting", 1.0);
    o.set_var("eyesShut", 0.0);
    o.set_var("moveFlags", 0.0);
    o.set_var("oTimer", 0.0);
    o.set_var("status", 0.0);
    o.set_var("tailCooldown", 0.0);
    o.set_var("grabbed", GRAB_NONE as f64);
    o.action = ACT_WAIT;
    o.prev_action = ACT_WAIT;
    o.action_state = 0.0;
    anim_init(o, ANIM_IDLE);
    o.set_var("animFrame", 0.0);
    o.visible = true;
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

fn set_anim_look_up_and_walk(o: &mut SimObject) -> bool {
    anim_init(o, ANIM_LOOK_UP_START_WALK);
    if check_anim_frame(o, 21) { o.forward_vel = 3.0; }
    near_end(o)
}
fn set_anim_slow_gait(o: &mut SimObject) -> bool {
    o.forward_vel = 3.0;
    anim_init(o, ANIM_SLOW_GAIT);
    near_end(o)
}
fn set_anim_look_down_stop_walk(o: &mut SimObject) -> bool {
    anim_init(o, ANIM_LOOK_DOWN_STOP_WALK);
    if check_anim_frame(o, 20) { o.forward_vel = 0.0; }
    near_end(o)
}

/// `bowser_init_camera_actions`.
fn init_camera_actions(o: &mut SimObject) {
    let cam_act = o.var("camAct") as i32;
    if cam_act == 0 {
        o.action = ACT_WAIT;
    } else if cam_act == 1 {
        o.action = ACT_INTRO_WALK;
    } else if bp(o) == BP_BITFS {
        o.action = ACT_BIG_JUMP;
    } else {
        o.action = ACT_DEFAULT;
    }
}

fn act_wait(o: &mut SimObject, _c: &mut Ctx) {
    o.forward_vel = 0.0;
    anim_init(o, ANIM_IDLE);
    init_camera_actions(o);
}

fn act_intro_walk(o: &mut SimObject, _c: &mut Ctx) {
    if sub(o) == 0 {
        if set_anim_look_up_and_walk(o) { sub_inc(o); }
    } else if sub(o) == 1 {
        if set_anim_slow_gait(o) { sub_inc(o); }
    } else if set_anim_look_down_stop_walk(o) {
        // The arena camera cutscene (`cutscene_bowser_arena`) is not ported: where it would leave
        // `oBowserCamAct` at IDLE until its end sets `BOWSER_CAM_ACT_END`, go straight to END.
        if o.var("camAct") as i32 == 1 { o.set_var("camAct", 2.0); }
        init_camera_actions(o);
    }
}

/// `bowser_bitdw_actions`.
fn bitdw_actions(o: &mut SimObject) {
    let rand = random_float();
    if o.var("isReacting") == 0.0 {
        if status(o) & STATUS_ANGLE_MARIO != 0 {
            o.action = if o.distance_to_mario < 1500.0 { ACT_BREATH_FIRE } else { ACT_QUICK_JUMP };
        } else {
            o.action = ACT_WALK_TO_MARIO;
        }
        o.set_var("isReacting", o.var("isReacting") + 1.0);
    } else {
        o.set_var("isReacting", 0.0);
        o.action = if rand < 0.1 { ACT_DANCE } else { ACT_WALK_TO_MARIO };
    }
}

/// `bowser_bitfs_actions`.
fn bitfs_actions(o: &mut SimObject) {
    let rand = random_float();
    if o.var("isReacting") == 0.0 {
        if status(o) & STATUS_ANGLE_MARIO != 0 {
            if o.distance_to_mario < 1300.0 {
                o.action = if rand < 0.5 { ACT_TELEPORT } else { ACT_SPIT_FIRE_ONTO_FLOOR };
            } else {
                o.action = ACT_CHARGE_MARIO;
                let d = o.var("distToCenter");
                if 500.0 < d && d < 1500.0 && rand < 0.5 {
                    o.action = ACT_BIG_JUMP;
                }
            }
        } else {
            o.action = ACT_WALK_TO_MARIO;
        }
        o.set_var("isReacting", o.var("isReacting") + 1.0);
    } else {
        o.set_var("isReacting", 0.0);
        o.action = ACT_WALK_TO_MARIO;
    }
}

/// `bowser_bits_action_list`.
fn bits_action_list(o: &mut SimObject) {
    let rand = random_float();
    if status(o) & STATUS_ANGLE_MARIO != 0 {
        if o.distance_to_mario < 1000.0 {
            o.action = if rand < 0.4 { ACT_SPIT_FIRE_ONTO_FLOOR } else if rand < 0.8 { ACT_SPIT_FIRE_INTO_SKY } else { ACT_BREATH_FIRE };
        } else {
            o.action = if rand < 0.5 { ACT_BIG_JUMP } else { ACT_CHARGE_MARIO };
        }
    } else {
        o.action = ACT_WALK_TO_MARIO;
    }
}

/// `bowser_bits_actions` (`oBowserBitSJustJump` is always FALSE).
fn bits_actions(o: &mut SimObject) {
    if o.var("isReacting") == 0.0 {
        bits_action_list(o);
        o.set_var("isReacting", 1.0);
    } else {
        o.set_var("isReacting", 0.0);
        o.action = ACT_WALK_TO_MARIO;
    }
}

/// `bowser_act_default`.
fn act_default(o: &mut SimObject, _c: &mut Ctx) {
    o.set_var("eyesShut", 0.0);
    anim_init(o, ANIM_IDLE);
    o.set_var("angleVelYaw", 0.0);
    o.forward_vel = 0.0;
    o.vel[1] = 0.0;
    match bp(o) {
        BP_BITDW => bitdw_actions(o),
        BP_BITFS => bitfs_actions(o),
        _ => bits_actions(o),
    }
}

fn act_breath_fire(o: &mut SimObject, _c: &mut Ctx) {
    o.forward_vel = 0.0;
    if anim_init_near_end(o, ANIM_BREATH) { o.action = ACT_DEFAULT; }
}

fn act_walk_to_mario(o: &mut SimObject, _c: &mut Ctx) {
    let angle_from_mario = abs_angle_diff(o.move_yaw, o.angle_to_mario);
    let turn_speed = if bp(o) == BP_BITFS {
        0x400 as f64
    } else if health(o) >= 3.0 {
        0x400 as f64
    } else if health(o) == 2.0 {
        0x300 as f64
    } else {
        0x200 as f64
    };
    rotate_yaw_toward(o, o.angle_to_mario, turn_speed);
    if sub(o) == 0 {
        o.set_var("bowserTimer", 0.0);
        if set_anim_look_up_and_walk(o) { sub_inc(o); }
    } else if sub(o) == 1 {
        if set_anim_slow_gait(o) {
            o.set_var("bowserTimer", o.var("bowserTimer") + 1.0);
            if status(o) & STATUS_FIRE_SKY != 0 {
                if o.var("bowserTimer") > 4.0 {
                    let s = status(o) & !STATUS_FIRE_SKY;
                    set_status(o, s);
                }
            } else if angle_from_mario < 0x2000 as f64 {
                sub_inc(o);
            }
        }
    } else if set_anim_look_down_stop_walk(o) {
        o.action = ACT_DEFAULT;
    }
}

fn act_teleport(o: &mut SimObject, _c: &mut Ctx) {
    match sub(o) {
        0 => {
            o.set_var("targetOpacity", 0.0);
            o.set_var("bowserTimer", 30.0);
            if o.var("opacity") == 0.0 {
                sub_inc(o);
                o.move_yaw = o.angle_to_mario;
            }
        }
        1 => {
            let t = o.var("bowserTimer");
            o.set_var("bowserTimer", t - 1.0);
            if t != 0.0 {
                o.forward_vel = 100.0;
            } else {
                o.action_state = 2.0;
                o.move_yaw = o.angle_to_mario;
            }
            if abs_angle_diff(o.move_yaw, o.angle_to_mario) > 0x4000 as f64 && o.distance_to_mario > 500.0 {
                o.action_state = 2.0;
                o.move_yaw = o.angle_to_mario;
            }
        }
        _ => {
            o.forward_vel = 0.0;
            o.set_var("targetOpacity", 255.0);
            if o.var("opacity") == 255.0 { o.action = ACT_DEFAULT; }
        }
    }
}

fn act_spit_fire_into_sky(o: &mut SimObject, c: &mut Ctx) {
    anim_init(o, ANIM_BREATH_UP);
    let f = anim_frame(o);
    if f > 24 && f < 36 {
        // bhvBlueBowserFlame at (0, 400, 100) relative to Bowser
        let kind = if f == 35 { 2.0 } else { 2.0 };
        let (s, co) = (sins(o.move_yaw), coss(o.move_yaw));
        c.out.push(Effect::Flame { pos: [o.pos[0] + 100.0 * s, o.pos[1] + 400.0, o.pos[2] + 100.0 * co], yaw: o.move_yaw, pitch: 0.0, kind });
    }
    if near_end(o) { o.action = ACT_DEFAULT; }
    let s = status(o) | STATUS_FIRE_SKY;
    set_status(o, s);
}

fn act_hit_mine(o: &mut SimObject, _c: &mut Ctx) {
    if otimer(o) == 0.0 {
        o.forward_vel = -400.0;
        o.vel[1] = 100.0;
        o.move_yaw = to_s16(o.var("angleToCenter") + 0x8000 as f64) as f64;
        o.set_var("eyesShut", 1.0);
    }
    match sub(o) {
        0 => {
            anim_init(o, ANIM_FLIP);
            sub_inc(o);
            o.set_var("bowserTimer", 0.0);
        }
        1 => {
            anim_init(o, ANIM_FLIP);
            extend_if_at_end(o);
            let mut t = o.var("bowserTimer");
            bounce_effects(o, &mut t);
            o.set_var("bowserTimer", t);
            if t > 2.0 {
                anim_init(o, ANIM_STAND_UP_FROM_FLIP);
                o.vel[1] = 0.0;
                o.forward_vel = 0.0;
                sub_inc(o);
            }
        }
        2 => {
            if near_end(o) {
                o.action = if health(o) == 1.0 { ACT_DANCE } else { ACT_DEFAULT };
                o.set_var("eyesShut", 0.0);
            }
        }
        _ => {}
    }
}

/// `bowser_bounce_effects`: mist and camera shake only.
fn bounce_effects(o: &mut SimObject, timer: &mut f64) {
    if flags(o) & MOVE_LANDED != 0 {
        *timer += 1.0;
    }
}

fn set_anim_jump(o: &mut SimObject) -> bool {
    anim_init(o, ANIM_JUMP_START);
    check_anim_frame(o, 11)
}

/// `bowser_land`.
fn land(o: &mut SimObject, c: &mut Ctx) -> bool {
    if flags(o) & MOVE_LANDED != 0 {
        o.forward_vel = 0.0;
        o.vel[1] = 0.0;
        anim_init(o, ANIM_JUMP_STOP);
        o.set_var("animFrame", 0.0);
        if bp(o) == BP_BITDW && o.distance_to_mario < 850.0 {
            c.out.push(Effect::MarioKnockback { pos: o.pos });
        }
        return true;
    }
    false
}

/// `bowser_spawn_shockwave` (BitS only).
fn spawn_shockwave(o: &mut SimObject, c: &mut Ctx) -> bool {
    if bp(o) == BP_BITS {
        c.out.push(Effect::Shockwave { pos: [o.pos[0], o.var("floorHeight"), o.pos[2]] });
        return true;
    }
    false
}

fn act_big_jump(o: &mut SimObject, c: &mut Ctx) {
    if sub(o) == 0 {
        if set_anim_jump(o) {
            o.vel[1] = if bp(o) == BP_BITS && status(o) & STATUS_BIG_JUMP != 0 { 70.0 } else { 80.0 };
            o.set_var("bowserTimer", 0.0);
            // bowser_short_second_hop
            if bp(o) == BP_BITS && status(o) & STATUS_BIG_JUMP != 0 && o.var("distToCenter") > 1000.0 {
                o.forward_vel = 60.0;
            }
            sub_inc(o);
        }
    } else if sub(o) == 1 {
        if bp(o) == BP_BITS && status(o) & STATUS_BIG_JUMP != 0 {
            reset_fallen_off_stage(o);
        }
        if land(o, c) {
            let s = status(o) & !STATUS_BIG_JUMP;
            set_status(o, s);
            o.forward_vel = 0.0;
            sub_inc(o);
            spawn_shockwave(o, c);
            if bp(o) == BP_BITFS { o.action = ACT_TILT_LAVA_PLATFORM; }
        }
    } else if near_end(o) {
        o.action = ACT_DEFAULT;
    }
}

/// `bowser_reset_fallen_off_stage` (BUGFIX_BOWSER_FALLEN_OFF_STAGE is on in this build).
fn reset_fallen_off_stage(o: &mut SimObject) {
    if o.vel[1] < 0.0 && o.pos[1] < o.home[1] - 300.0 {
        o.pos[0] = 0.0;
        o.pos[2] = 0.0;
        o.pos[1] = o.home[1] + 2000.0;
        o.vel[1] = 0.0;
        o.forward_vel = 0.0;
    }
}

fn act_quick_jump(o: &mut SimObject, c: &mut Ctx) {
    if sub(o) == 0 {
        if set_anim_jump(o) {
            o.vel[1] = 60.0;
            o.forward_vel = 50.0;
            o.set_var("bowserTimer", 0.0);
            sub_inc(o);
        }
    } else if sub(o) == 1 {
        if land(o, c) { sub_inc(o); }
    } else if near_end(o) {
        o.action = ACT_DEFAULT;
    }
}

fn act_hit_edge(o: &mut SimObject, _c: &mut Ctx) {
    o.forward_vel = 0.0;
    if otimer(o) == 0.0 { o.set_var("bowserTimer", 0.0); }
    match sub(o) {
        0 => {
            anim_init(o, ANIM_EDGE_MOVE);
            if near_end(o) { o.set_var("bowserTimer", o.var("bowserTimer") + 1.0); }
            if o.var("bowserTimer") > 0.0 { sub_inc(o); }
        }
        1 => {
            anim_init(o, ANIM_EDGE_STOP);
            if near_end(o) { o.action = ACT_TURN_FROM_EDGE; }
        }
        _ => {}
    }
}

fn act_spit_fire_onto_floor(o: &mut SimObject, c: &mut Ctx) {
    o.set_var("randSplitFloor", if c.mario_wedges < 4.0 { 3.0 } else { (random_float() * 3.0 + 1.0).trunc() });
    anim_init(o, ANIM_BREATH_QUICK);
    if check_anim_frame(o, 5) {
        // obj_spit_fire(0, 200, 180, 7.0, MODEL_RED_FLAME, 30.0, 10.0, 0x1000)
        let (s, co) = (sins(o.move_yaw), coss(o.move_yaw));
        c.out.push(Effect::Flame { pos: [o.pos[0] + 180.0 * s, o.pos[1] + 200.0, o.pos[2] + 180.0 * co], yaw: o.move_yaw, pitch: 0x1000 as f64, kind: 3.0 });
    }
    if near_end(o) { sub_inc(o); }
    if o.action_state >= o.var("randSplitFloor") { o.action = ACT_DEFAULT; }
}

/// `bowser_turn_on_timer`.
fn turn_on_timer(o: &mut SimObject, time: f64, yaw: f64) -> bool {
    if sub(o) == 0 {
        if anim_init_near_end(o, ANIM_LOOK_UP_START_WALK) { sub_inc(o); }
    } else if sub(o) == 1 {
        if anim_init_near_end(o, ANIM_LOOK_DOWN_STOP_WALK) { sub_inc(o); }
    } else {
        anim_init(o, ANIM_IDLE);
    }
    o.forward_vel = 0.0;
    o.move_yaw += yaw;
    otimer(o) >= time
}

fn act_turn_from_edge(o: &mut SimObject, _c: &mut Ctx) {
    if turn_on_timer(o, 63.0, 0x200 as f64) { o.action = ACT_DEFAULT; }
}

fn act_charge_mario(o: &mut SimObject, _c: &mut Ctx) {
    if otimer(o) == 0.0 { o.forward_vel = 0.0; }
    match sub(o) {
        SUB_CHARGE_START => {
            o.set_var("bowserTimer", 0.0);
            if anim_init_near_end(o, ANIM_RUN_START) { o.action_state = SUB_CHARGE_RUN as f64; }
        }
        SUB_CHARGE_RUN => {
            o.forward_vel = 50.0;
            if anim_init_near_end(o, ANIM_RUN) {
                o.set_var("bowserTimer", o.var("bowserTimer") + 1.0);
                if o.var("bowserTimer") >= 6.0 { o.action_state = SUB_CHARGE_SLIP as f64; }
                if o.var("bowserTimer") >= 2.0 && abs_angle_diff(o.angle_to_mario, o.move_yaw) > 0x2000 as f64 {
                    o.action_state = SUB_CHARGE_SLIP as f64;
                }
            }
            rotate_yaw_toward(o, o.angle_to_mario, 0x200 as f64);
        }
        SUB_CHARGE_SLIP => {
            o.set_var("bowserTimer", 0.0);
            anim_init(o, ANIM_RUN_SLIP);
            let mut v = o.forward_vel;
            if approach_f32_signed(&mut v, 0.0, -1.0) { o.action_state = SUB_CHARGE_END as f64; }
            o.forward_vel = v;
            extend_if_at_end(o);
        }
        SUB_CHARGE_END => {
            o.forward_vel = 0.0;
            anim_init(o, ANIM_RUN_STOP);
            if near_end(o) {
                let time = if bp(o) == BP_BITS { 10.0 } else { 30.0 };
                if o.var("bowserTimer") > time { o.action = ACT_DEFAULT; }
                o.set_var("bowserTimer", o.var("bowserTimer") + 1.0);
            }
            extend_if_at_end(o);
        }
        _ => {}
    }
    if flags(o) & MOVE_HIT_EDGE != 0 { o.action = ACT_HIT_EDGE; }
}

/// `bowser_act_thrown`.
fn act_thrown(o: &mut SimObject, c: &mut Ctx) {
    if otimer(o) < 2.0 { o.set_var("bowserTimer", 0.0); }
    if sub(o) == 0 {
        anim_init(o, ANIM_SHAKING);
        let mut t = o.var("bowserTimer");
        bounce_effects(o, &mut t);
        o.set_var("bowserTimer", t);
        if flags(o) & (MOVE_LANDED | MOVE_ON_GROUND) != 0 {
            o.forward_vel = 0.0;
            sub_inc(o);
        }
    } else if anim_init_near_end(o, ANIM_STAND_UP) {
        o.action = ACT_DEFAULT;
    }
    // bowser_check_hit_mine
    let mut best: Option<(ObjId, f64)> = None;
    for (id, mp) in &c.mines {
        let d = dist_3d(&o.pos, mp);
        if best.map_or(true, |(_, bd)| d < bd) { best = Some((*id, d)); }
    }
    if let Some((id, d)) = best {
        if d < 800.0 {
            c.out.push(Effect::HitMine(id));
            o.set_var("health", health(o) - 1.0);
            if health(o) <= 0.0 { o.action = ACT_DEAD; } else { o.action = ACT_HIT_MINE; }
        }
    }
}

fn set_goal_invisible(o: &mut SimObject) {
    o.set_var("targetOpacity", 0.0);
    if o.var("opacity") == 0.0 {
        o.forward_vel = 0.0;
        o.vel[1] = 0.0;
        o.pos[1] = o.home[1] - 1000.0;
    }
}

fn act_jump_onto_stage(o: &mut SimObject, c: &mut Ctx) {
    let on_dynamic_floor = o.var("floorDynamic") != 0.0;
    let s = status(o) | STATUS_BIG_JUMP;
    set_status(o, s);
    match sub(o) {
        SUB_JUMP_ON_STAGE_IDLE => {
            if otimer(o) == 0.0 {
                o.set_var("facePitch", 0.0);
                o.set_var("faceRoll", 0.0);
            }
            o.set_var("facePitch", o.var("facePitch") + 0x800 as f64);
            o.set_var("faceRoll", o.var("faceRoll") + 0x800 as f64);
            if (o.var("facePitch") as i64) & 0xFFFF == 0 { sub_inc(o); }
            set_goal_invisible(o);
        }
        SUB_JUMP_ON_STAGE_START => {
            anim_init(o, ANIM_JUMP_START);
            if check_anim_frame(o, 11) {
                o.move_yaw = o.var("angleToCenter");
                o.vel[1] = 150.0;
                o.set_var("targetOpacity", 255.0);
                o.set_var("bowserTimer", 0.0);
                sub_inc(o);
            } else {
                set_goal_invisible(o);
            }
        }
        SUB_JUMP_ON_STAGE_LAND => {
            if o.pos[1] > o.home[1] {
                o.set_var("drag", 0.0);
                if o.var("distToCenter") < 2500.0 {
                    if (o.var("floorHeight") - o.home[1]).abs() < 100.0 {
                        let mut v = o.forward_vel;
                        approach_f32_signed(&mut v, 0.0, -5.0);
                        o.forward_vel = v;
                    } else {
                        forward_vel_approach_upward(o, 150.0, 2.0);
                    }
                } else {
                    forward_vel_approach_upward(o, 150.0, 2.0);
                }
            }
            if land(o, c) {
                o.set_var("drag", 10.0);
                sub_inc(o);
                if !on_dynamic_floor {
                    spawn_shockwave(o, c);
                } else if bp(o) == BP_BITS {
                    o.action = ACT_BIG_JUMP;
                }
                if bp(o) == BP_BITFS { o.action = ACT_TILT_LAVA_PLATFORM; }
            }
            reset_fallen_off_stage(o);
        }
        SUB_JUMP_ON_STAGE_STOP => {
            if near_end(o) {
                o.action = ACT_DEFAULT;
                let s = status(o) & !STATUS_BIG_JUMP;
                set_status(o, s);
                extend_if_at_end(o);
            }
        }
        _ => {}
    }
}

fn act_dance(o: &mut SimObject, _c: &mut Ctx) {
    if anim_init_near_end(o, ANIM_DANCE) { o.action = ACT_DEFAULT; }
}

fn act_dead(o: &mut SimObject, c: &mut Ctx) {
    match sub(o) {
        SUB_DEAD_FLY_BACK => {
            anim_init(o, ANIM_FLIP_DOWN);
            o.forward_vel = if bp(o) == BP_BITS { -400.0 } else { -200.0 };
            o.vel[1] = 100.0;
            o.move_yaw = to_s16(o.var("angleToCenter") + 0x8000 as f64) as f64;
            o.set_var("bowserTimer", 0.0);
            sub_inc(o);
        }
        SUB_DEAD_BOUNCE => {
            o.set_var("eyesShut", 1.0);
            let mut t = o.var("bowserTimer");
            bounce_effects(o, &mut t);
            o.set_var("bowserTimer", t);
            if flags(o) & MOVE_ON_GROUND != 0 {
                o.forward_vel = 0.0;
                sub_inc(o);
            }
        }
        SUB_DEAD_WAIT => {
            // bowser_dead_wait_for_mario
            let mut ret = false;
            if anim_init_near_end(o, ANIM_LAY_DOWN) && o.distance_to_mario < 700.0 && abs_angle_diff(c.mario_yaw, o.angle_to_mario) > 0x6000 as f64 {
                ret = true;
            }
            extend_if_at_end(o);
            o.set_var("bowserTimer", 0.0);
            if ret {
                o.set_var("bowserTimer", 0.0);
                o.action_state = if bp(o) == BP_BITS { SUB_DEAD_FINAL_END } else { SUB_DEAD_DEFAULT_END } as f64;
            }
        }
        SUB_DEAD_DEFAULT_END => {
            // bowser_dead_default_stage_ending: the dialog is skipped, then twirl up.
            if o.var("bowserTimer") < 2.0 {
                o.set_var("bowserTimer", 2.0);
            } else if dead_twirl_up(o) {
                dead_hide(o);
                c.out.push(Effect::Defeated(o.pos));
                sub_inc(o);
            }
        }
        SUB_DEAD_FINAL_END => {
            // bowser_dead_final_stage_ending: spawn the Grand Star, then fade out.
            if o.var("bowserTimer") < 2.0 {
                c.out.push(Effect::Defeated(o.pos));
                o.set_var("bowserTimer", 2.0);
            } else if o.var("opacity") > 4.0 {
                o.set_var("opacity", o.var("opacity") - 4.0);
            } else {
                dead_hide(o);
                sub_inc(o);
            }
        }
        _ => {}
    }
}

/// `bowser_dead_twirl_up`.
fn dead_twirl_up(o: &mut SimObject) -> bool {
    let mut ret = false;
    let mut sx = if o.has_var("scaleX") { o.var("scaleX") } else { 1.0 };
    let mut sy = if o.has_var("scaleY") { o.var("scaleY") } else { 1.0 };
    let mut sz = if o.has_var("scaleZ") { o.var("scaleZ") } else { 1.0 };
    if sx < 0.8 { o.set_var("angleVelYaw", o.var("angleVelYaw") + 0x80 as f64); }
    if sx > 0.2 {
        sx -= 0.02;
        sz -= 0.02;
    } else {
        sy -= 0.01;
        o.vel[1] = 20.0;
        o.gravity = 0.0;
    }
    if sy < 0.5 { ret = true; }
    o.move_yaw += o.var("angleVelYaw");
    if o.var("opacity") >= 3.0 { o.set_var("opacity", o.var("opacity") - 2.0); }
    o.set_var("scaleX", sx);
    o.set_var("scaleY", sy);
    o.set_var("scaleZ", sz);
    ret
}

/// `bowser_dead_hide`.
fn dead_hide(o: &mut SimObject) {
    o.scale = 0.0;
    o.forward_vel = 0.0;
    o.vel[1] = 0.0;
    o.gravity = 0.0;
    o.visible = false;
}

/// `bowser_act_tilt_lava_platform`: publishes the platform's angular velocity (read by
/// `solids::bowser_arena::TiltingPlatform`); `platReset` is set for the frame the tilt ends.
fn act_tilt_lava_platform(o: &mut SimObject, _c: &mut Ctx) {
    o.set_var("platReset", 0.0);
    let timer = otimer(o) as i32;
    let angle = to_s16(o.var("angleToCenter") + 0x8000 as f64) as f64;
    let mut tilting = false;
    let mut i = 0;
    while TILT_PLATFORM_DATA[i].2 != 0 {
        let (flag, ang_speed, time) = TILT_PLATFORM_DATA[i];
        if timer < time {
            let ang_speed = if flag > 0 { (time - timer - 1) * ang_speed } else { (timer - TILT_PLATFORM_DATA[i - 1].2) * ang_speed };
            o.set_var("platPitchVel", (coss(angle) * ang_speed as f64).trunc());
            o.set_var("platRollVel", (-sins(angle) * ang_speed as f64).trunc());
            tilting = true;
            break;
        }
        i += 1;
    }
    if !tilting {
        o.action = ACT_DEFAULT;
        o.set_var("platPitchVel", 0.0);
        o.set_var("platRollVel", 0.0);
        o.set_var("platReset", 1.0);
    }
    extend_if_at_end(o);
}

fn act_idle(o: &mut SimObject, _c: &mut Ctx) {
    if anim_init_near_end(o, ANIM_IDLE) { o.action = ACT_DEFAULT; }
}

fn run_action(o: &mut SimObject, c: &mut Ctx) {
    match o.action {
        ACT_DEFAULT => act_default(o, c),
        ACT_THROWN => act_thrown(o, c),
        ACT_JUMP_ONTO_STAGE => act_jump_onto_stage(o, c),
        ACT_DANCE => act_dance(o, c),
        ACT_DEAD => act_dead(o, c),
        ACT_WAIT => act_wait(o, c),
        ACT_INTRO_WALK => act_intro_walk(o, c),
        ACT_CHARGE_MARIO => act_charge_mario(o, c),
        ACT_SPIT_FIRE_INTO_SKY => act_spit_fire_into_sky(o, c),
        ACT_SPIT_FIRE_ONTO_FLOOR => act_spit_fire_onto_floor(o, c),
        ACT_HIT_EDGE => act_hit_edge(o, c),
        ACT_TURN_FROM_EDGE => act_turn_from_edge(o, c),
        ACT_HIT_MINE => act_hit_mine(o, c),
        ACT_BIG_JUMP => act_big_jump(o, c),
        ACT_WALK_TO_MARIO => act_walk_to_mario(o, c),
        ACT_BREATH_FIRE => act_breath_fire(o, c),
        ACT_TELEPORT => act_teleport(o, c),
        ACT_QUICK_JUMP => act_quick_jump(o, c),
        ACT_IDLE => act_idle(o, c),
        ACT_TILT_LAVA_PLATFORM => act_tilt_lava_platform(o, c),
        _ => {}
    }
}

/// `bowser_check_fallen_off_stage`.
fn check_fallen_off_stage(o: &SimObject) -> bool {
    if o.action != ACT_JUMP_ONTO_STAGE && o.action != ACT_TILT_LAVA_PLATFORM {
        if o.pos[1] < o.home[1] - 1000.0 { return true; }
        if flags(o) & MOVE_LANDED != 0 {
            let t = o.var("floorType") as i32;
            if t == SURFACE_BURNING || t == SURFACE_DEATH_PLANE { return true; }
        }
    }
    false
}

/// `bhv_bowser_flame_spawn_loop`: the breath attack's flames (frames 46..84 of `ANIM_BREATH`).
fn flame_spawn(o: &SimObject, c: &mut Ctx) {
    if o.var("soundState") as i32 != ANIM_BREATH { return; }
    let mut f = anim_frame(o) + 1;
    if ANIM_LOOP_END[ANIM_BREATH as usize] == f { f = 0; }
    if f > 45 && f < 85 {
        let d = BOWSER_FLAMES_ORIENTATION[f as usize];
        let (px, py, pz, rot_y, rot_x) = (d[0] as f64, d[1] as f64, d[2] as f64, d[3] as f64, d[4] as f64);
        let (cs, sn) = (coss(o.move_yaw), sins(o.move_yaw));
        let pos = [o.pos[0] + (pz * sn + px * cs), o.pos[1] + py, o.pos[2] + (pz * cs - px * sn)];
        if f & 1 == 0 {
            c.out.push(Effect::Flame { pos, yaw: rot_y + to_s16(o.move_yaw) as f64, pitch: rot_x + 0xC00 as f64, kind: 0.0 });
        }
    }
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

fn free_update(o: &mut SimObject, c: &mut Ctx) {
    o.set_var("grabbed", GRAB_NONE as f64);
    update_floor_and_walls(o, c.world);
    run_action(o, c);
    move_standard(o, c.world);
    if check_fallen_off_stage(o) { o.action = ACT_JUMP_ONTO_STAGE; }
    let f = floor_at(c.world, o.pos[0], o.pos[1], o.pos[2]);
    match f.object {
        Some(tag) => o.set_var("platformTag", tag as f64),
        None => { o.vars.remove("platformTag"); }
    }
}

fn held_update(o: &mut SimObject, _c: &mut Ctx) {
    let s = status(o) & !STATUS_FIRE_SKY;
    set_status(o, s);
    match o.var("grabbed") as i32 {
        GRAB_NONE => {
            anim_init(o, ANIM_GRABBED);
            o.action = ACT_THROWN;
            o.set_var("grabbed", GRAB_GRABBED as f64);
        }
        GRAB_GRABBED => {
            if near_end(o) {
                anim_init(o, ANIM_SHAKING);
                o.set_var("grabbed", GRAB_HOLDING as f64);
            }
        }
        _ => {}
    }
    set_flags(o, 0);
}

fn thrown_dropped_update(o: &mut SimObject, _c: &mut Ctx) {
    o.set_var("grabbed", GRAB_NONE as f64);
    // cur_obj_get_thrown_or_placed(1, 1, BOWSER_ACT_THROWN)
    o.set_held_state(HELD_FREE);
    o.action = ACT_THROWN;
    set_flags(o, 0);
    // swing speed from Mario's angular velocity when he let go
    let mut swing = o.var("spin") / 3000.0 * 70.0;
    if swing < 0.0 { swing = -swing; }
    if swing > 90.0 { swing *= 2.5; }
    let pitch = 0.0;
    o.forward_vel = coss(pitch) * swing;
    o.vel[1] = -sins(pitch) * swing;
    o.set_var("tailCooldown", 30.0);
    o.set_var("oTimer", 0.0);
    o.action_state = 0.0;
    o.prev_action = ACT_THROWN;
}

/// `bhv_bowser_loop`.
pub fn bowser_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mario_pos = m.pos;
    let mario_yaw = m.face_angle[1];
    let wedges = (m.health / 256.0).floor();
    let mines: Vec<(ObjId, Vec3)> = m.objects.objects.iter().enumerate()
        .filter(|(_, b)| b.active && b.behavior == ObjectBehaviorKind::SmBowserBomb)
        .map(|(i, b)| (ObjId(i as u32), b.pos)).collect();
    let mut c = Ctx { world: &world, mario_pos, mario_yaw, mario_wedges: wedges, mines, out: vec![], dist_to_mario: 0.0, angle_to_mario: 0.0 };
    {
        let o = &mut m.objects[id];
        c.dist_to_mario = o.distance_to_mario;
        c.angle_to_mario = o.angle_to_mario;
        begin_frame(o);
        // distance/angle values
        let d = js_hypot(o.pos[0], o.pos[2]);
        o.set_var("distToCenter", d);
        let a = atan2s(-o.pos[2], -o.pos[0]) as f64;
        o.set_var("angleToCenter", a);
        let angle_to_mario = abs_angle_diff(o.move_yaw, o.angle_to_mario);
        let angle_to_center = abs_angle_diff(o.move_yaw, a);
        let mut st = status(o) & !0xFF;
        if angle_to_mario < 0x2000 as f64 { st |= STATUS_ANGLE_MARIO; }
        if angle_to_center < 0x3800 as f64 { st |= STATUS_ANGLE_CENTER; }
        if d < 1000.0 { st |= STATUS_DIST_CENTER; }
        if o.distance_to_mario < 850.0 { st |= STATUS_DIST_MARIO; }
        set_status(o, st);
        let cooldown = o.var("tailCooldown");
        if cooldown > 0.0 { o.set_var("tailCooldown", cooldown - 1.0); }

        match o.held_state {
            HELD_FREE => free_update(o, &mut c),
            HELD_HELD => held_update(o, &mut c),
            _ => thrown_dropped_update(o, &mut c),
        }
        if o.held_state == HELD_FREE && o.action != ACT_DEAD {
            flame_spawn(o, &mut c);
        }
        // adjust opacity (when not dead)
        if o.action != ACT_DEAD {
            let target = o.var("targetOpacity");
            let mut op = o.var("opacity");
            if target != op {
                if target > op { op = (op + 20.0).min(255.0); } else { op = (op - 20.0).max(0.0); }
                o.set_var("opacity", op);
            }
        }
        o.face_yaw = o.move_yaw;
        o.visible = o.var("opacity") >= 60.0 && o.scale > 0.0;
        advance_anim(o);
        end_frame(o);
    }
    // Effects.
    let (opos, oyaw) = { let o = &m.objects[id]; (o.pos, o.move_yaw) };
    let _ = (opos, oyaw);
    for e in c.out {
        match e {
            Effect::Flame { pos, yaw, pitch, kind } => {
                let mut spec = ObjectSpec::new(ObjectBehaviorKind::SmBowserFlame, pos, yaw);
                spec.param = Some(kind);
                spec.model = Some("actors/flame/red_flame_geo.glb".to_string());
                let nid = m.objects.spawn(&spec);
                m.objects[nid].set_var("pitch", pitch);
            }
            Effect::Shockwave { pos } => {
                let spec = ObjectSpec::new(ObjectBehaviorKind::SmBowserShockWave, pos, 0.0);
                m.objects.spawn(&spec);
            }
            Effect::MarioKnockback { pos } => {
                m.take_damage(1.0, Some(pos));
            }
            Effect::HitMine(mid) => {
                m.objects[mid].interact_status |= INT_STATUS_HIT_MINE;
            }
            Effect::Defeated(pos) => m.objects.bowser_defeated(pos),
        }
    }
}

/// `INT_STATUS_HIT_MINE`.
pub const INT_STATUS_HIT_MINE: u32 = 1 << 24;

/// Tail grab / body contact (`bowser_tail_anchor_*`, `bhv_bowser_body_anchor_loop`), replacing the
/// generic interaction for this kind. Returns after handling Mario's overlap with Bowser's grabbable hitbox.
pub fn touch(m: &mut MarioRuntime, id: ObjId, can_grab: bool) {
    let (opos, yaw, action, held, opacity, cooldown) = {
        let o = &m.objects[id];
        (o.pos, o.move_yaw, o.action, o.held_state, o.var("opacity"), o.var("tailCooldown"))
    };
    if held != HELD_FREE || action == ACT_DEAD || action == ACT_TILT_LAVA_PLATFORM { return; }
    if at_tail(m, opos, yaw) {
        if cooldown > 0.0 { return; }
        m.near_grabbable = true;
        if can_grab {
            m.interact_obj = Some(id);
            m.interact_grabbable = true;
        }
    } else if js_hypot(m.pos[0] - opos[0], m.pos[2] - opos[2]) < 100.0 + 37.0 && opacity >= 100.0 {
        // bhvBowserBodyAnchor: radius 100, height 300, INTERACT_DAMAGE 2 (big knockback)
        if m.pos[1] < opos[1] + 300.0 && m.pos[1] + 160.0 > opos[1] {
            m.take_damage(2.0, Some(opos));
        }
    }
}

/// Root-relative stand-in for the animated tail joint (hitbox r=100, h=50 below the tail tip).
pub fn at_tail(m: &MarioRuntime, opos: Vec3, yaw: f64) -> bool {
    let dx = m.pos[0] - (opos[0] - 300.0 * sins(yaw));
    let dz = m.pos[2] - (opos[2] - 300.0 * coss(yaw));
    dx * dx + dz * dz < 137.0 * 137.0 && m.pos[1] < opos[1] + 100.0 && m.pos[1] + 160.0 > opos[1] + 50.0
}

// ---------------------------------------------------------------------------
// bowser_bomb.inc.c
// ---------------------------------------------------------------------------

/// `bhvBowserBomb`: `SET_HITBOX_WITH_OFFSET(40, 40, 40)`.
pub fn init_bowser_bomb(o: &mut SimObject) {
    o.set_hitbox(ObjectHitbox { interact_type: 0, down_offset: 40.0, damage_or_coin_value: 0.0, health: 1.0, num_loot_coins: 0.0, radius: 40.0, height: 40.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 });
}

/// `bhv_bowser_bomb_loop`.
pub fn bowser_bomb_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].interact_status & INT_STATUS_HIT_MINE != 0 {
        m.objects[id].interact_status &= !INT_STATUS_HIT_MINE;
        object_manager::explode(m, id, 150.0, 0.0);
        m.objects[id].deactivate(-1.0);
        return;
    }
    if mario_overlaps_object(m, &m.objects[id]) {
        object_manager::explode(m, id, 150.0, 2.0);
        m.objects[id].deactivate(-1.0);
    }
}

// ---------------------------------------------------------------------------
// bowser_flame.inc.c (growing flame, flame on the floor)
// ---------------------------------------------------------------------------

/// `sGrowingBowserFlameHitbox` / `sBowserFlameHitbox` (INTERACT_FLAME is drawn as damage).
fn flame_hitbox(down_offset: f64) -> ObjectHitbox {
    ObjectHitbox { interact_type: INTERACT_DAMAGE, down_offset, damage_or_coin_value: 1.0, health: 0.0, num_loot_coins: 0.0, radius: 10.0, height: 40.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 }
}

/// kind 0: `bhvFlameMovingForwardGrowing`; 1: `bhvFlameBowser` (burning on the floor); 2: sky flame;
/// 3: small spit flame (`bhvSmallPiranhaFlame` stand-in).
pub fn init_bowser_flame(o: &mut SimObject) {
    o.visible = true;
    o.billboard = true;
    o.set_var("oTimer", 0.0);
    match o.param as i32 {
        0 => {
            o.forward_vel = 30.0;
            o.pos[0] += random_float() * 80.0 - 40.0;
            o.pos[2] += random_float() * 80.0 - 40.0;
            o.set_var("flameScale", 3.0);
            o.set_hitbox(flame_hitbox(20.0));
        }
        2 => {
            o.pos[0] += random_float() * 80.0 - 40.0;
            o.pos[2] += random_float() * 80.0 - 40.0;
            o.vel[1] = 7.0;
            o.forward_vel = 35.0;
            o.set_var("flameScale", 3.0);
            o.gravity = 1.0;
            o.set_hitbox(flame_hitbox(20.0));
        }
        3 => {
            o.forward_vel = 30.0;
            o.set_var("flameScale", 3.0);
            o.set_hitbox(flame_hitbox(20.0));
        }
        _ => {
            // bhv_flame_bowser_init
            o.move_yaw = random_u16_f();
            o.vel[1] = if random_float() < 0.2 { 80.0 } else { 20.0 };
            o.forward_vel = 10.0;
            o.gravity = -1.0;
            o.set_var("flameScale", random_float() + 1.0);
            o.set_hitbox(flame_hitbox(0.0));
        }
    }
    o.scale = o.var("flameScale");
}

fn random_u16_f() -> f64 { (random_float() * 65536.0).floor() }

pub fn bowser_flame_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let kind = m.objects[id].param as i32;
    let o = &mut m.objects[id];
    begin_frame(o);
    match kind {
        0 => {
            // bhv_flame_moving_forward_growing_loop
            let s = o.var("flameScale") + 0.5;
            o.set_var("flameScale", s);
            o.scale = s;
            let mut pitch = o.var("pitch");
            if pitch > 0x800 as f64 { pitch -= 0x200 as f64; }
            o.set_var("pitch", pitch);
            // cur_obj_set_pos_via_transform: (0, 0, fvel) through the (pitch, yaw) rotation
            o.pos[0] += o.forward_vel * coss(pitch) * sins(o.move_yaw);
            o.pos[1] += -o.forward_vel * sins(pitch);
            o.pos[2] += o.forward_vel * coss(pitch) * coss(o.move_yaw);
            let fh = world.find_floor(o.pos[0], o.pos[1], o.pos[2]).height;
            if s > 30.0 {
                o.deactivate(-1.0);
            } else if o.pos[1] < fh {
                o.pos[1] = fh;
                let pos = o.pos;
                o.deactivate(-1.0);
                spawn_floor_flame(m, pos);
            }
        }
        2 => {
            // bhv_blue_bowser_flame_loop (simplified landing: ground flames after 20 frames)
            let mut s = o.var("flameScale");
            if s < 16.0 { s += 0.5; }
            o.set_var("flameScale", s);
            o.scale = s;
            o.pos[0] += o.forward_vel * sins(o.move_yaw) * 0.3;
            o.pos[2] += o.forward_vel * coss(o.move_yaw) * 0.3;
            o.vel[1] -= 1.0;
            o.pos[1] += o.vel[1];
            if otimer(o) > 20.0 {
                let pos = o.pos;
                o.deactivate(-1.0);
                for _ in 0..3 { spawn_floor_flame(m, pos); }
            }
        }
        3 => {
            // small spit flame: flies forward and down, burns on landing
            let s = o.var("flameScale");
            o.scale = s;
            o.pos[0] += o.forward_vel * sins(o.move_yaw);
            o.pos[2] += o.forward_vel * coss(o.move_yaw);
            o.vel[1] -= 2.0;
            o.pos[1] += o.vel[1];
            let fh = world.find_floor(o.pos[0], o.pos[1], o.pos[2]).height;
            if o.pos[1] < fh || otimer(o) > 60.0 {
                o.pos[1] = fh;
                let pos = o.pos;
                o.deactivate(-1.0);
                spawn_floor_flame(m, pos);
            }
        }
        _ => {
            // bhv_flame_bowser_loop
            update_floor_and_walls(o, &world);
            move_standard_flame(o, &world);
            if o.vel[1] < -4.0 { o.vel[1] = -4.0; }
            if o.action == 0 {
                o.hitbox.interact_type = 0;
                if flags(o) & MOVE_LANDED != 0 {
                    o.action = 1;
                    o.set_var("flameScale", random_float() * 2.0 + 6.0);
                    o.forward_vel = 0.0;
                    o.vel[1] = 0.0;
                    o.gravity = 0.0;
                }
            } else {
                o.hitbox.interact_type = INTERACT_DAMAGE;
                let s = o.var("flameScale");
                if otimer(o) > s * 10.0 + 5.0 {
                    o.set_var("flameScale", s - 0.15);
                    if o.var("flameScale") <= 0.0 { o.deactivate(-1.0); }
                }
            }
            let t = o.var("floorType") as i32;
            if (t == SURFACE_BURNING || t == SURFACE_DEATH_PLANE) && otimer(o) > 0.0 { o.deactivate(-1.0); }
            if otimer(o) > 300.0 { o.deactivate(-1.0); }
            o.scale = o.var("flameScale");
            o.graph_y_offset = o.scale * 14.0;
        }
    }
    let o = &mut m.objects[id];
    end_frame(o);
    if o.active && mario_hit_flame(m, id) {
        let pos = m.objects[id].pos;
        m.take_damage(1.0, Some(pos));
    }
}

fn mario_hit_flame(m: &MarioRuntime, id: ObjId) -> bool {
    let o = &m.objects[id];
    o.hitbox.interact_type != 0 && mario_overlaps_object(m, o)
}

/// Flames use the same `cur_obj_move_standard` with gravity from the object (`oGravity`).
fn move_standard_flame(o: &mut SimObject, world: &CollisionWorld) {
    let g = o.gravity;
    let saved = o.gravity;
    o.gravity = 0.0;
    // reuse Bowser's mover with this object's own gravity by temporarily encoding it in vel
    let _ = g;
    o.vel[1] += saved;
    move_standard_g(o, world, 0.0);
    o.gravity = saved;
}

/// `move_standard` with an explicit extra gravity of 0 (the caller already applied `oGravity`).
fn move_standard_g(o: &mut SimObject, world: &CollisionWorld, _extra: f64) {
    // Same routine as Bowser's, with the drag/bounciness of the flame physics block.
    let steep_normal_y = coss((78 * (0x10000 / 360)) as f64);
    o.vel[0] = o.forward_vel * sins(o.move_yaw);
    o.vel[2] = o.forward_vel * coss(o.move_yaw);
    let mut vx = o.vel[0];
    let mut vz = o.vel[2];
    apply_drag_to_value(&mut vx, 10.0);
    apply_drag_to_value(&mut vz, 10.0);
    o.vel[0] = vx;
    o.vel[2] = vz;
    let floor_h = o.var("floorHeight");
    let ix = o.pos[0] + o.vel[0];
    let iz = o.pos[2] + o.vel[2];
    let intended = floor_at(world, ix, o.pos[1], iz);
    if intended.height >= FLOOR_LOWER_LIMIT_MISC && (intended.height - floor_h < 5.0 && intended.normal_y > steep_normal_y || o.pos[1] > intended.height) {
        o.pos[0] = ix;
        o.pos[2] = iz;
    }
    let mut f = flags(o);
    o.pos[1] += o.vel[1];
    f &= !MOVE_LANDED;
    if o.pos[1] < floor_h {
        if f & MOVE_ON_GROUND == 0 { f |= MOVE_LANDED | MOVE_ON_GROUND; }
        o.pos[1] = floor_h;
        if o.vel[1] < 0.0 { o.vel[1] *= -0.7; }
    }
    set_flags(o, f);
}

fn spawn_floor_flame(m: &mut MarioRuntime, pos: Vec3) {
    let mut spec = ObjectSpec::new(ObjectBehaviorKind::SmBowserFlame, pos, 0.0);
    spec.param = Some(1.0);
    spec.model = Some("actors/flame/red_flame_geo.glb".to_string());
    m.objects.spawn(&spec);
}

// ---------------------------------------------------------------------------
// Shock wave (`bhv_bowser_shock_wave_loop`, `bowser_shock_wave.inc.c`)
// ---------------------------------------------------------------------------

/// Expanding ring on the floor: grows 40 units per frame, hurts Mario touching the ring on the ground.
pub fn init_shockwave(o: &mut SimObject) {
    o.set_var("oTimer", 0.0);
    o.scale = 0.1;
    o.visible = true;
}

pub fn shockwave_behavior(id: ObjId, m: &mut MarioRuntime) {
    let t = m.objects[id].var("oTimer");
    let (opos, scale) = {
        let o = &mut m.objects[id];
        o.scale = 0.1 + t * 0.4;
        o.set_var("oTimer", t + 1.0);
        (o.pos, o.scale)
    };
    // Hits Mario while he is on (or just above) the ground inside the thin ring.
    let ring = 100.0 * scale * 1.2;
    let d = js_hypot(m.pos[0] - opos[0], m.pos[2] - opos[2]);
    if (d - ring).abs() < 120.0 && m.pos[1] < opos[1] + 150.0 && m.pos[1] > opos[1] - 100.0 {
        m.take_damage(2.0, Some(opos));
    }
    if t > 70.0 {
        m.objects[id].deactivate(-1.0);
    }
}
