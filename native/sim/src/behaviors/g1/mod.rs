//! SM64 object behaviors, group 1 (castle + the first courses): Thwomp, Piranha Plant, Bullet
//! Bill, Swoop, Scuttlebug, Spindrift, Mips, Boo in the castle, Monty Mole, Snufit, butterflies
//! and birds. Each file cites the decomp function it ports (`src/game/behaviors/*.inc.c`).
//!
//! Conventions shared by every file here:
//! * decomp `o->oAction` / `oTimer` map to [`SimObject::action`] / [`SimObject::timer`] (the object
//!   manager increments `timer` once per tick, before the behavior runs; `set_action` resets it
//!   like `cur_obj_change_action` + the oTimer = 0 that the behavior loop does on an action change);
//! * decomp animation state (`animFrame`, `loopEnd`, ...) lives in the object's vars, see [`anim`];
//!   the app maps the `animation` index of the actor's animation table to the glb clip;
//! * `SET_OBJ_PHYSICS` gravity `-400` is `gravity = 4.0` here (the sim subtracts `gravity`);
//! * sounds are emitted with `ObjectManager::sound` using names from `assets/audio/music/sounds.json`;
//! * "intangible" is `hitbox.interact_type == 0` (the real type is restored on `become_tangible`).
use crate::math::*;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::object_manager::BehaviorTable;
use ObjectBehaviorKind as K;

pub mod anim;
pub mod bullet_bill;
pub mod butterfly;
pub mod interact;
pub mod mips;
pub mod monty_mole;
pub mod piranha_plant;
pub mod scuttlebug;
pub mod snufit;
pub mod spindrift;
pub mod swoop;
pub mod thwomp;

pub const INTERACT_BOUNCE_TOP: u32 = 1 << 15;
pub const INTERACT_HIT_FROM_BELOW: u32 = 1 << 22;
pub const INTERACT_SNUFIT_BULLET: u32 = 1 << 28;
pub const INT_SUBTYPE_TWIRL_BOUNCE: u32 = 0x80;

pub fn register(t: &mut BehaviorTable) {
    t.register(K::Thwomp, thwomp::init_thwomp, thwomp::thwomp_behavior);
    t.register(K::Thwomp2, thwomp::init_thwomp, thwomp::thwomp_behavior);
    t.register(K::PiranhaPlant, piranha_plant::init, piranha_plant::update);
    t.register(K::BulletBill, bullet_bill::init, bullet_bill::update);
    t.register(K::Swoop, swoop::init, swoop::update);
    t.register(K::Scuttlebug, scuttlebug::init_scuttlebug, scuttlebug::scuttlebug_update);
    t.register(K::ScuttlebugSpawner, scuttlebug::init_spawner, scuttlebug::spawner_update);
    t.register(K::Spindrift, spindrift::init, spindrift::update);
    t.register(K::Mips, mips::init, mips::update);
    t.register(K::BooInCastle, mips::init_boo_in_castle, mips::boo_in_castle_update);
    t.register(K::MontyMole, monty_mole::init_mole, monty_mole::mole_update);
    t.register(K::MontyMoleHole, monty_mole::init_hole, monty_mole::hole_update);
    t.register(K::MontyMoleRock, monty_mole::init_rock, monty_mole::rock_update);
    t.register(K::Snufit, snufit::init, snufit::update);
    t.register(K::SnufitBall, snufit::init_ball, snufit::ball_update);
    t.register(K::Butterfly, butterfly::init_butterfly, butterfly::butterfly_update);
    t.register(K::Bird, butterfly::init_bird, butterfly::bird_update);
}

/// Behavior kinds of this group whose model faces `face_yaw`/pitch/roll rather than `move_yaw`
/// (`OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW` is not set or the behavior turns the model separately).
pub fn is_g1(kind: ObjectBehaviorKind) -> bool {
    matches!(
        kind,
        K::Thwomp | K::Thwomp2 | K::PiranhaPlant | K::BulletBill | K::Swoop | K::Scuttlebug | K::Spindrift | K::Mips
            | K::BooInCastle | K::MontyMole | K::MontyMoleRock | K::Snufit | K::SnufitBall | K::Butterfly | K::Bird
    )
}

// ---------------------------------------------------------------------------------------------
// Shared helpers (object_helpers.c / obj_behaviors.c / math_util.c)
// ---------------------------------------------------------------------------------------------

/// approach_s16_symmetric (math_util.c).
pub fn approach_s16(value: f64, target: f64, inc: f64) -> f64 {
    let value = to_s16(value) as f64;
    let dist = to_s16(target - value) as f64;
    to_s16(if dist >= 0.0 {
        if dist > inc { value + inc } else { target }
    } else if dist < -inc {
        value - inc
    } else {
        target
    }) as f64
}

/// approach_f32_ptr (object_helpers.c): returns true when `*value == target` afterwards.
pub fn approach_f32_ptr(value: &mut f64, target: f64, delta: f64) -> bool {
    if *value > target {
        *value -= delta;
        if *value < target { *value = target; }
    } else {
        *value += delta;
        if *value > target { *value = target; }
    }
    *value == target
}

/// cur_obj_rotate_yaw_toward (object_helpers.c): true once the yaw has reached `target`
/// (no change this frame and equal to the target).
pub fn rotate_yaw_toward(o: &mut SimObject, target: f64, increment: f64) -> bool {
    let start = o.move_yaw;
    o.move_yaw = approach_s16(o.move_yaw, target, increment);
    to_s16(o.move_yaw - start) == 0 && to_s16(o.move_yaw) == to_s16(target)
}

/// abs_angle_diff (math_util.c).
pub fn abs_angle_diff(a: f64, b: f64) -> f64 {
    let diff = to_s16(b - a);
    (if diff == -0x8000 { 0x8000 } else { diff.abs() }) as f64
}

/// cur_obj_become_intangible / tangible, with the real interact type kept in `realInteract`.
pub fn become_intangible(o: &mut SimObject) {
    if o.hitbox.interact_type != 0 { o.set_var("realInteract", o.hitbox.interact_type as f64); }
    o.hitbox.interact_type = 0;
}
pub fn become_tangible(o: &mut SimObject) {
    if o.has_var("realInteract") { o.hitbox.interact_type = o.var("realInteract") as u32; }
}
/// Set the interaction type, remembering it as the tangible one (`o->oInteractType = ...`).
pub fn set_interact(o: &mut SimObject, t: u32) {
    o.set_var("realInteract", t as f64);
    o.hitbox.interact_type = t;
}
pub fn is_tangible(o: &SimObject) -> bool { o.hitbox.interact_type != 0 }

/// cur_obj_check_interacted: consumes INTERACTED.
pub fn check_interacted(o: &mut SimObject) -> bool {
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        o.interact_status = 0;
        true
    } else {
        false
    }
}

/// `cur_obj_set_hitbox_and_die_if_attacked` after the hitbox is set: loot coins and removal when
/// attacked (`obj_spawn_loot_yellow_coins` + `obj_mark_for_deletion`), true if merely touched.
pub fn die_if_attacked(id: ObjId, m: &mut MarioRuntime, sound: &'static str, no_loot: bool) -> bool {
    let o = &mut m.objects[id];
    let mut interacted = false;
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        if o.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
            let pos = o.pos;
            let loot = if no_loot { 0.0 } else { o.hitbox.num_loot_coins };
            o.deactivate(-1.0);
            m.objects.sound(pos, sound);
            for _ in 0..loot as i32 { m.objects.spawn_coin(pos, true, 1.0); }
            return false;
        }
        interacted = true;
    }
    o.interact_status = 0;
    interacted
}

/// obj_check_attacks (obj_behaviors_2.c) without the lava check: returns the attack type when
/// the object was attacked (dying when its health is exhausted), setting `action` to
/// `attacked_mario_action` when it hurt Mario.
pub fn check_attacks(id: ObjId, m: &mut MarioRuntime, attacked_mario_action: i32, death_sound: &'static str) -> i32 {
    let o = &mut m.objects[id];
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        if o.interact_status & INT_STATUS_ATTACKED_MARIO != 0 {
            if o.action != attacked_mario_action {
                o.action = attacked_mario_action;
                o.timer = 0.0;
                o.action_state = 0.0;
            }
        } else {
            let attack = (o.interact_status & 0xFF) as i32;
            // obj_die_if_health_non_positive
            if o.hitbox.health <= 0.0 {
                let pos = o.pos;
                let loot = o.hitbox.num_loot_coins;
                o.deactivate(-1.0);
                m.objects.sound(pos, death_sound);
                for _ in 0..loot as i32 { m.objects.spawn_coin(pos, true, 1.0); }
            }
            m.objects[id].interact_status = 0;
            return attack;
        }
    }
    m.objects[id].interact_status = 0;
    0
}

/// The shape of a decomp `struct ObjectHitbox`.
pub const fn hitbox(
    interact_type: u32,
    down_offset: f64,
    damage: f64,
    health: f64,
    loot: f64,
    radius: f64,
    height: f64,
    hurt_radius: f64,
    hurt_height: f64,
) -> ObjectHitbox {
    ObjectHitbox {
        interact_type, down_offset, damage_or_coin_value: damage, health, num_loot_coins: loot, radius, height,
        hurtbox_radius: hurt_radius, hurtbox_height: hurt_height,
    }
}

/// `obj_set_hitbox` (object_helpers.c): the first call copies everything, later calls only
/// refresh the radius / height / hurtbox / down offset (type, damage, health and loot are kept).
pub fn set_hitbox(o: &mut SimObject, h: ObjectHitbox) {
    if !o.has_var("hbInit") {
        o.set_var("hbInit", 1.0);
        o.set_var("realInteract", h.interact_type as f64);
        o.hitbox = h;
        return;
    }
    o.hitbox.radius = h.radius;
    o.hitbox.height = h.height;
    o.hitbox.hurtbox_radius = h.hurtbox_radius;
    o.hitbox.hurtbox_height = h.hurtbox_height;
    o.hitbox.down_offset = h.down_offset;
}

/// The end of a decomp object update (`cur_obj_update`, behavior_script.c): `oTimer++`, then the
/// action-change timer reset, then the draw pass that advances the animation.
pub fn end_frame(o: &mut SimObject) {
    o.action_timer += 1.0;
    if o.action != o.prev_action {
        o.action_timer = 0.0;
        o.action_state = 0.0;
        o.prev_action = o.action;
    }
    anim::advance(o);
}

/// Result flags of [`move_standard`] (`oMoveFlags`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveFlags {
    pub landed: bool,
    pub on_ground: bool,
    pub hit_wall: bool,
    pub hit_edge: bool,
    pub in_air: bool,
}

/// `cur_obj_move_standard(steepSlopeAngleDegrees)` for objects with gravity and bounciness -0.5
/// (`SET_OBJ_PHYSICS(.., -50, ..)`): wall push-out, optional edge / steep-slope avoidance (the
/// negative-angle form of the original), gravity, floor landing. Approximates `cur_obj_move_xz`
/// and `cur_obj_move_y` in the same way `koopa::move_koopa` does.
pub fn move_standard(o: &mut SimObject, world: &crate::collision::CollisionWorld, steep_deg: f64, care_edges: bool, wall_radius: f64) -> MoveFlags {
    let steep_y = (steep_deg * std::f64::consts::PI / 180.0).cos();
    let mut flags = MoveFlags::default();
    let wall = world.resolve_and_return_wall_collisions(&mut o.pos, 10.0, wall_radius);
    flags.hit_wall = match &wall {
        Some(w) => w.normal[0] * sins(o.move_yaw) + w.normal[2] * coss(o.move_yaw) < 0.0,
        None => false,
    };
    let floor = world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
    let x = o.pos[0] + o.forward_vel * sins(o.move_yaw);
    let z = o.pos[2] + o.forward_vel * coss(o.move_yaw);
    let next = world.find_floor(x, o.pos[1], z);
    let delta = next.height - floor.height;
    let edge = match &next.surface {
        None => true,
        Some(s) => care_edges && ((delta < -50.0 && o.grounded) || (delta < 5.0 && s.normal[1] <= steep_y)),
    };
    if edge {
        flags.hit_edge = true;
    } else if !care_edges || next.surface.as_ref().map_or(false, |s| s.normal[1] > steep_y) || o.pos[1] > next.height {
        o.pos[0] = x;
        o.pos[2] = z;
    } else {
        flags.hit_wall = true;
    }
    let was_grounded = o.grounded;
    let floor_y = if floor.surface.is_some() { floor.height } else { crate::collision::FLOOR_LOWER_LIMIT };
    o.vel[1] = (-70f64).max(o.vel[1] - o.gravity);
    o.pos[1] += o.vel[1];
    if o.pos[1] <= floor_y {
        o.pos[1] = floor_y;
        if o.vel[1] < -17.5 { o.vel[1] = -o.vel[1] / 2.0; } else { o.vel[1] = 0.0; }
        o.grounded = true;
        flags.on_ground = true;
        flags.landed = !was_grounded;
    } else {
        o.grounded = false;
        flags.in_air = true;
    }
    o.hit_wall = flags.hit_wall;
    flags
}

/// `floor_y` under `pos` (or the lower limit when there is none).
pub fn floor_height(m: &MarioRuntime, pos: &Vec3) -> f64 {
    let f = m.world.find_floor(pos[0], pos[1], pos[2]);
    if f.surface.is_some() { f.height } else { crate::collision::FLOOR_LOWER_LIMIT }
}
