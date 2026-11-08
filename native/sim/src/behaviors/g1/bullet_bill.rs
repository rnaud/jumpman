//! Bullet Bill (`bhvBulletBill`), the Whomp's Fortress tower cannon projectile.
//! Port of `src/game/behaviors/bullet_bill.inc.c`. The cannon itself (`bhvBulletBillCannon`) is a
//! static collision model in the level; the Bill sits inside it, charges toward Mario when he is
//! in front within 400..1500 units and then resets at its home.
use super::*;

/// `SCALE(0, 40)`.
pub const SCALE: f64 = 0.4;

/// bhvBulletBill: SET_HITBOX_WITH_OFFSET(50, 50, 50), INTERACT_DAMAGE, oDamageOrCoinValue 3,
/// wall hitbox radius 30, SET_HOME, bhv_bullet_bill_init.
pub fn init(o: &mut SimObject) {
    o.hitbox = hitbox(INTERACT_DAMAGE, 50.0, 3.0, 0.0, 0.0, 50.0, 50.0, 50.0, 50.0);
    o.set_var("realInteract", INTERACT_DAMAGE as f64);
    o.set_var("pureDamage", 1.0);
    o.scale = SCALE;
    o.visible = true;
    o.set_var("initialMoveYaw", o.move_yaw);
    o.action = 0;
    o.prev_action = 0;
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let dist = m.objects[id].distance_to_mario;
    let angle_to_mario = m.objects[id].angle_to_mario;
    let o = &mut m.objects[id];
    match o.action {
        // bullet_bill_act_0
        0 => {
            become_tangible(o);
            o.forward_vel = 0.0;
            o.move_yaw = o.var("initialMoveYaw");
            o.set_var("facePitch", 0.0);
            o.set_var("faceRoll", 0.0);
            o.pos = o.home;
            o.action = 1;
        }
        // bullet_bill_act_1
        1 => {
            let diff = abs_angle_diff(angle_to_mario, o.move_yaw);
            if diff < 0x2000 as f64 && 400.0 < dist && dist < 1500.0 { o.action = 2; }
        }
        // bullet_bill_act_2
        2 => {
            let t = o.action_timer;
            if t < 40.0 {
                o.forward_vel = 3.0;
            } else if t < 50.0 {
                o.forward_vel = if (t as i64) % 2 != 0 { 3.0 } else { -3.0 };
            } else {
                // cur_obj_update_floor_and_walls from the 71st frame on: wall hit detection.
                let mut hit_wall = false;
                if t > 70.0 {
                    let mut p = o.pos;
                    hit_wall = world.resolve_and_return_wall_collisions(&mut p, 10.0, 30.0).is_some();
                }
                o.forward_vel = 30.0;
                if dist > 300.0 { rotate_yaw_toward(o, angle_to_mario, 0x100 as f64); }
                if t == 50.0 {
                    let pos = o.pos;
                    m.objects.sound(pos, "OBJ_POUNDING_CANNON");
                }
                let o = &mut m.objects[id];
                if o.action_timer > 150.0 || hit_wall { o.action = 3; }
            }
        }
        // bullet_bill_act_3
        3 => o.action = 0,
        // bullet_bill_act_4
        _ => {
            if o.action_timer == 0.0 {
                o.forward_vel = -30.0;
                become_intangible(o);
            }
            o.set_var("facePitch", o.var("facePitch") + 0x1000 as f64);
            o.set_var("faceRoll", o.var("faceRoll") + 0x1000 as f64);
            o.pos[1] += 20.0;
            if o.action_timer > 90.0 { o.action = 0; }
        }
    }
    let o = &mut m.objects[id];
    // bhv_bullet_bill_loop
    if check_interacted(o) { o.action = 4; }
    // OBJ_FLAG_MOVE_XZ_USING_FVEL
    o.pos[0] += o.forward_vel * sins(o.move_yaw);
    o.pos[2] += o.forward_vel * coss(o.move_yaw);
    o.face_yaw = o.move_yaw;
    end_frame(o);
}
