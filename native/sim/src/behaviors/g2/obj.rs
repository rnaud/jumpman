//! Object-field helpers shared by the SM64 group-2 ports (courses 7-13). They mirror the
//! decomp's `object_helpers.c` / `behavior_script.c` conventions so a port reads like its source.
//!
//! * `oTimer` is [`SimObject::timer`]. The object manager adds 1 before every update, the
//!   decomp adds 1 after the script, so [`begin`] undoes the manager's increment and [`finish`]
//!   performs `cur_obj_update`'s tail (timer++, action-change reset, object flags).
//! * `oAction` / `oPrevAction` / `oSubAction` are `action` / `prev_action` / `action_state`.
//!   Write `o.action = n` directly (not `set_action`) so the timer reset happens in [`finish`].
//! * Fields without a native member live in `vars` under the decomp spelling without the `o`:
//!   `pitch`, `roll` (oFaceAnglePitch/Roll), `avYaw`, `avPitch`, `avRoll` (oAngleVel*).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

/// `OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW`.
pub const F_FACE_YAW: u32 = 1;
/// `OBJ_FLAG_MOVE_XZ_USING_FVEL`.
pub const F_MOVE_XZ: u32 = 2;

/// Start of `cur_obj_update`'s script execution for an object the manager already ticked.
pub fn begin(m: &mut MarioRuntime, id: ObjId) {
    let o = &mut m.objects[id];
    o.timer -= 1.0;
    if o.action != o.prev_action {
        o.timer = 0.0;
        o.action_state = 0.0;
        o.prev_action = o.action;
    }
}

/// `cur_obj_update` after the behavior script: timer, action change, object flags.
pub fn finish(m: &mut MarioRuntime, id: ObjId, flags: u32) {
    let o = &mut m.objects[id];
    o.timer += 1.0;
    if o.action != o.prev_action {
        o.timer = 0.0;
        o.action_state = 0.0;
        o.prev_action = o.action;
    }
    if flags & F_FACE_YAW != 0 {
        o.face_yaw = o.move_yaw;
    }
    if flags & F_MOVE_XZ != 0 {
        cur_obj_move_xz_using_fvel_and_yaw(o);
    }
}

/// Run `body` between [`begin`] and [`finish`].
pub fn run(m: &mut MarioRuntime, id: ObjId, flags: u32, body: impl FnOnce(&mut MarioRuntime, ObjId)) {
    begin(m, id);
    body(m, id);
    finish(m, id, flags);
}

/// object_helpers.c `cur_obj_move_xz_using_fvel_and_yaw`.
pub fn cur_obj_move_xz_using_fvel_and_yaw(o: &mut SimObject) {
    o.vel[0] = o.forward_vel * sins(o.move_yaw);
    o.vel[2] = o.forward_vel * coss(o.move_yaw);
    o.pos[0] += o.vel[0];
    o.pos[2] += o.vel[2];
}

/// object_helpers.c `cur_obj_compute_vel_xz`.
pub fn cur_obj_compute_vel_xz(o: &mut SimObject) {
    o.vel[0] = o.forward_vel * sins(o.move_yaw);
    o.vel[2] = o.forward_vel * coss(o.move_yaw);
}

/// object_helpers.c `cur_obj_move_using_vel_and_gravity`. `gravity` is the decomp's (negative)
/// `oGravity`.
pub fn cur_obj_move_using_vel_and_gravity(o: &mut SimObject, gravity: f64) {
    if o.pos[0].abs() < 12000.0 && o.pos[1].abs() < 12000.0 && o.pos[2].abs() < 12000.0 {
        o.pos[0] += o.vel[0];
        o.pos[2] += o.vel[2];
        o.vel[1] += gravity;
        o.pos[1] += o.vel[1];
    }
}

/// object_helpers.c `cur_obj_move_using_fvel_and_gravity`.
pub fn cur_obj_move_using_fvel_and_gravity(o: &mut SimObject, gravity: f64) {
    cur_obj_compute_vel_xz(o);
    cur_obj_move_using_vel_and_gravity(o, gravity);
}

/// object_helpers.c `approach_f32_signed`.
pub fn approach_f32_signed(value: &mut f64, target: f64, increment: f64) -> bool {
    *value += increment;
    if increment >= 0.0 {
        if *value > target {
            *value = target;
            return true;
        }
    } else if *value < target {
        *value = target;
        return true;
    }
    false
}

/// object_helpers.c `cur_obj_set_pos_relative` (offsets in the parent's move-yaw frame).
pub fn set_pos_relative(o: &mut SimObject, parent_pos: Vec3, parent_yaw: f64, dleft: f64, dy: f64, dforward: f64) {
    let facing_z = coss(parent_yaw);
    let facing_x = sins(parent_yaw);
    let dz = dforward * facing_z - dleft * facing_x;
    let dx = dforward * facing_x + dleft * facing_z;
    o.move_yaw = parent_yaw;
    o.pos = [parent_pos[0] + dx, parent_pos[1] + dy, parent_pos[2] + dz];
}

/// `gMarioObject->platform == o` (platform_displacement.c `update_mario_platform`): Mario is
/// within 4 units of a floor that belongs to object `id`.
pub fn mario_on_platform(m: &MarioRuntime, id: ObjId) -> bool {
    let floor = m.world.find_floor(m.pos[0], m.pos[1], m.pos[2]);
    (m.pos[1] - floor.height).abs() < 4.0 && floor.surface.as_ref().is_some_and(|s| s.object == Some(id.0))
}

/// object_helpers.c `is_point_within_radius_of_mario`.
pub fn point_within_radius_of_mario(m: &MarioRuntime, p: Vec3, dist: f64) -> bool {
    let dx = p[0] - m.pos[0];
    let dy = p[1] - m.pos[1];
    let dz = p[2] - m.pos[2];
    dx * dx + dy * dy + dz * dz < dist * dist
}

/// object_helpers.c `cur_obj_push_mario_away`.
pub fn push_mario_away(m: &mut MarioRuntime, id: ObjId, radius: f64) {
    let opos = m.objects[id].pos;
    let rx = m.pos[0] - opos[0];
    let rz = m.pos[2] - opos[2];
    let dist = (rx * rx + rz * rz).sqrt();
    if dist < radius {
        m.state.pos[0] += (radius - dist) / radius * rx;
        m.state.pos[2] += (radius - dist) / radius * rz;
    }
}

/// `spawn_object`: a child at the parent's position and angles, `link` = parentObj. Dead
/// children of the same kind and model are recycled so spawners (flames, wood pieces) do not
/// grow the object arena without bound.
pub fn spawn_child(m: &mut MarioRuntime, parent: ObjId, kind: ObjectBehaviorKind, model: Option<&str>, param: f64) -> ObjId {
    let p = &m.objects[parent];
    let mut spec = ObjectSpec::new(kind, p.pos, p.face_yaw);
    spec.param = Some(param);
    spec.model = model.map(str::to_string);
    let move_yaw = p.move_yaw;
    let slot = m.objects.objects.iter().position(|o| !o.active && o.has_var("g2dead") && o.behavior == kind && o.model.as_deref() == model);
    let id = match slot {
        Some(i) => {
            let mut fresh = SimObject::new(&spec);
            if let Some(init) = m.objects.behaviors.init_fn(kind) {
                init(&mut fresh);
            }
            m.objects.objects[i] = fresh;
            ObjId(i as u32)
        }
        None => m.objects.spawn(&spec),
    };
    let c = &mut m.objects[id];
    c.link = Some(parent);
    c.move_yaw = move_yaw;
    id
}

/// `obj_mark_for_deletion` (the slot may be recycled by [`spawn_child`]).
pub fn mark_for_deletion(o: &mut SimObject) {
    o.active = false;
    o.visible = false;
    o.respawn_timer = -1.0;
    o.respawn_distance = -1.0;
    o.set_var("g2dead", 1.0);
}

/// `random_float()` in [0, 1).
pub fn random_float() -> f64 {
    random()
}

/// `random_u16()`.
pub fn random_u16() -> f64 {
    (random() * 65536.0).floor()
}
