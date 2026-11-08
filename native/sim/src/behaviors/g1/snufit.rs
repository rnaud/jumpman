//! Snufit (`bhvSnufit`) and its bullets (`bhvSnufitBalls`), the masked Hazy Maze Cave shooters.
//! Port of `src/game/behaviors/snufit.inc.c`. The mask / body-scale display-list callbacks are
//! not ported: the model is drawn at the uniform `oSnufitScale`.
use super::*;

pub const ACT_IDLE: i32 = 0;
pub const ACT_SHOOT: i32 = 1;
pub const BALL_MODEL: &str = "actors/bowling_ball/bowling_ball_geo.glb";

/// sSnufitHitbox.
pub const HITBOX: ObjectHitbox = hitbox(INTERACT_HIT_FROM_BELOW, 0.0, 2.0, 0.0, 2.0, 100.0, 60.0, 70.0, 50.0);
/// sSnufitBulletHitbox.
pub const BALL_HITBOX: ObjectHitbox = hitbox(INTERACT_SNUFIT_BULLET, 50.0, 1.0, 0.0, 0.0, 100.0, 50.0, 100.0, 50.0);

pub fn init(o: &mut SimObject) {
    o.hitbox = HITBOX;
    o.set_var("realInteract", HITBOX.interact_type as f64);
    o.visible = true;
    o.action = ACT_IDLE;
    o.prev_action = ACT_IDLE;
}

pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let o = &mut m.objects[id];
    let dist = o.distance_to_mario;
    let angle_to_mario = o.angle_to_mario;
    let tick = o.timer;
    let mut pitch = o.var("movePitch");
    if dist < 800.0 {
        // obj_turn_pitch_toward_mario(120.0, 2000)
        let horiz = js_hypot(mpos[0] - o.pos[0], mpos[2] - o.pos[2]);
        let target = atan2s(horiz, o.pos[1] - 120.0 - mpos[1]) as f64;
        // (o->oPosY -= 120 is undone right after, so the target uses the lowered Y)
        pitch = approach_s16(pitch, target, 2000.0);
        pitch = (to_s16(pitch).clamp(-0x2000, 0x2000)) as f64;
        rotate_yaw_toward(o, angle_to_mario, 2000.0);
    } else {
        pitch = approach_s16(pitch, 0.0, 0x200 as f64); // obj_move_pitch_approach(0, 0x200)
        o.move_yaw = to_s16(o.move_yaw + 200.0) as f64;
    }
    o.set_var("movePitch", pitch);
    o.set_var("facePitch", pitch);
    let mut pos_for_ball = None;
    match o.action {
        // snufit_act_idle
        ACT_IDLE => {
            let mario_dist = (dist / 10.0).trunc();
            if o.action_timer > mario_dist && dist < 800.0 {
                let period = approach_s16(o.var("bodyScalePeriod"), 0.0, 1500.0);
                let base = approach_s16(o.var("bodyBaseScale"), 600.0, 15.0);
                o.set_var("bodyScalePeriod", period);
                o.set_var("bodyBaseScale", base);
                if to_s16(period) == 0 && base == 600.0 {
                    o.action = ACT_SHOOT;
                    o.set_var("bullets", 0.0);
                }
            } else {
                o.set_var("circularPeriod", o.var("circularPeriod") + 400.0);
            }
        }
        // snufit_act_shoot
        _ => {
            let period = approach_s16(o.var("bodyScalePeriod"), -0x8000 as f64, 3000.0);
            let base = approach_s16(o.var("bodyBaseScale"), 167.0, 20.0);
            o.set_var("bodyScalePeriod", period);
            o.set_var("bodyBaseScale", base);
            if to_u16(period) == 0x8000 && base == 167.0 {
                o.action = ACT_IDLE;
            } else if o.var("bullets") < 3.0 && o.action_timer >= 3.0 {
                o.set_var("bullets", o.var("bullets") + 1.0);
                // spawn_object_relative(0, 0, -20, 40, o, MODEL_BOWLING_BALL, bhvSnufitBalls)
                let yaw = o.move_yaw;
                let p = [o.pos[0] + 40.0 * sins(yaw), o.pos[1] - 20.0, o.pos[2] + 40.0 * coss(yaw)];
                pos_for_ball = Some((p, yaw, pitch));
                o.action_timer = 0.0; // o->oTimer = 0
            }
        }
    }
    let o = &mut m.objects[id];
    let circ = o.var("circularPeriod");
    o.pos[0] = o.home[0] + 100.0 * coss(circ);
    o.pos[1] = o.home[1] + 8.0 * coss(4000.0 * tick);
    o.pos[2] = o.home[2] + 100.0 * sins(circ);
    let body = (o.var("bodyBaseScale") + 666.0 + o.var("bodyBaseScale") * coss(o.var("bodyScalePeriod"))).trunc();
    o.scale = if body > 1000.0 { (body - 1000.0) / 1000.0 + 1.0 } else { 1.0 };
    o.face_yaw = o.move_yaw;
    let action = o.action;
    if let Some((p, yaw, pitch)) = pos_for_ball {
        m.objects.sound(p, "OBJ_CANNON1"); // SOUND_OBJ_SNUFIT_SHOOT
        let mut s = ObjectSpec::new(ObjectBehaviorKind::SnufitBall, p, yaw);
        s.model = Some(BALL_MODEL.to_string());
        let b = m.objects.spawn(&s);
        m.objects[b].set_var("movePitch", pitch);
    }
    check_attacks(id, m, action, "OBJ_KOOPA_FLYGUY_DEATH"); // SOUND_OBJ_SNUFIT_SKEETER_DEATH
    end_frame(&mut m.objects[id]);
}

/// bhvSnufitBalls: SET_OBJ_PHYSICS(wall radius 10, gravity 0, ...), SCALE 10, billboard.
pub fn init_ball(o: &mut SimObject) {
    o.hitbox = BALL_HITBOX;
    o.set_var("realInteract", BALL_HITBOX.interact_type as f64);
    o.gravity = 0.0;
    o.friction = 0.0;
    o.scale = 0.1;
    o.billboard = true;
    o.graph_y_offset = 10.0;
    o.visible = true;
}

/// bhv_snufit_balls_loop.
pub fn ball_update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    if m.objects[id].timer != 0.0 && m.objects[id].distance_to_mario > 1500.0 {
        m.objects[id].deactivate(-1.0);
        return;
    }
    if m.objects[id].gravity == 0.0 {
        // obj_compute_vel_from_move_pitch(40)
        let pitch = m.objects[id].var("movePitch");
        {
            let o = &mut m.objects[id];
            o.forward_vel = 40.0 * coss(pitch);
            o.vel[1] = 40.0 * -sins(pitch);
        }
        let attack = check_attacks(id, m, 1, "OBJ_KOOPA_FLYGUY_DEATH");
        let o = &mut m.objects[id];
        if !o.active { return; }
        if attack != 0 {
            // Mario (metal cap) knocked it back.
            o.move_yaw = to_s16(o.move_yaw + 0x8000 as f64) as f64;
            o.forward_vel *= 0.05;
            o.vel[1] = 30.0;
            o.gravity = 4.0;
            become_intangible(o);
        } else {
            let f = move_standard(o, &world, 78.0, false, 10.0);
            if o.action == 1 || f.on_ground || f.hit_wall {
                o.deactivate(-1.0); // obj_die_if_health_non_positive (health 0, no death sound)
                return;
            }
        }
    } else {
        // cur_obj_move_using_fvel_and_gravity
        let o = &mut m.objects[id];
        o.pos[0] += o.forward_vel * sins(o.move_yaw);
        o.pos[2] += o.forward_vel * coss(o.move_yaw);
        o.vel[1] -= o.gravity;
        o.pos[1] += o.vel[1];
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    end_frame(o);
}
