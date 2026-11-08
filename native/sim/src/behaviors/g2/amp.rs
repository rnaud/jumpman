//! Circling and fixed Amps (src/game/behaviors/amp.inc.c): Snowman's Land, Wet-Dry World's town,
//! the Shifting Sand Land pyramid. The homing amp (`bhvHomingAmp`) is not placed in these courses.
use super::interact::INTERACT_SHOCK;
use super::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

/// `sAmpHitbox`.
const AMP_HITBOX: ObjectHitbox =
    ObjectHitbox { interact_type: INTERACT_SHOCK, down_offset: 40.0, damage_or_coin_value: 1.0, health: 0.0, num_loot_coins: 0.0, radius: 40.0, height: 50.0, hurtbox_radius: 50.0, hurtbox_height: 60.0 };

/// `AMP_ACT_*`.
const AMP_ACT_IDLE: i32 = 2;
const AMP_ACT_ATTACK_COOLDOWN: i32 = 4;

/// `bhv_circling_amp_init` (`oGraphYOffset = 40`).
pub fn init_circling_amp(o: &mut SimObject) {
    o.visible = true;
    o.graph_y_offset = 40.0;
    o.home = o.pos;
    o.hitbox = AMP_HITBOX;
    let radius = match ((o.param as u32) >> 16) & 0xFF {
        0 => 200.0,
        1 => 300.0,
        2 => 400.0,
        _ => 0.0,
    };
    o.set_var("radius", radius);
    // The move angle is the position along the circle.
    o.move_yaw = random_u16();
    o.action = AMP_ACT_IDLE;
    o.prev_action = AMP_ACT_IDLE;
}

/// amp.inc.c `check_amp_attack`: an interaction sends the amp into its cooldown.
fn check_amp_attack(o: &mut SimObject) {
    o.hitbox = AMP_HITBOX;
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        o.action = AMP_ACT_ATTACK_COOLDOWN;
        o.interact_status = 0;
    }
}

/// `bhv_circling_amp_loop`.
pub fn circling_amp(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_MOVE_XZ, |m, id| {
        let (mario, mario_pos) = (m.pos, m.pos);
        let _ = mario;
        let o = &mut m.objects[id];
        match o.action {
            AMP_ACT_IDLE => {
                if bparam2_amp(o) == 3 {
                    // fixed_circling_amp_idle_loop: turn toward Mario in yaw and pitch.
                    let (dx, dz) = (mario_pos[0] - o.pos[0], mario_pos[2] - o.pos[2]);
                    let dy = mario_pos[1] + 120.0 - o.pos[1];
                    let v_angle = atan2s((dx * dx + dz * dz).sqrt(), -dy) as f64;
                    let target = atan2s(dz, dx) as f64;
                    o.turn_toward_angle(target, 0x1000 as f64);
                    o.face_yaw = o.move_yaw;
                    let pitch = approach_s16_symmetric(o.var("pitch"), v_angle, 0x1000 as f64);
                    o.set_var("pitch", pitch);
                    o.pos[1] = o.home[1] + coss(o.var("phase") * 0x458 as f64) * 20.0;
                    check_amp_attack(o);
                    o.set_var("phase", o.var("phase") + 1.0);
                } else {
                    // circling_amp_idle_loop
                    let radius = o.var("radius");
                    o.pos[0] = o.home[0] + sins(o.move_yaw) * radius;
                    o.pos[2] = o.home[2] + coss(o.move_yaw) * radius;
                    o.pos[1] = o.home[1] + coss(o.var("phase") * 0x8B0 as f64) * 30.0;
                    o.move_yaw = to_s16(o.move_yaw + 0x400 as f64) as f64;
                    o.face_yaw = to_s16(o.move_yaw + 0x4000 as f64) as f64;
                    check_amp_attack(o);
                    o.set_var("phase", o.var("phase") + 1.0);
                }
            }
            AMP_ACT_ATTACK_COOLDOWN => {
                // amp_attack_cooldown_loop
                o.forward_vel = 0.0;
                o.hitbox.interact_type = 0;
                if o.timer > 90.0 {
                    o.hitbox = AMP_HITBOX;
                    o.action = AMP_ACT_IDLE;
                }
            }
            _ => {}
        }
    });
}

fn bparam2_amp(o: &SimObject) -> i32 {
    (((o.param as u32) >> 16) & 0xFF) as i32
}
