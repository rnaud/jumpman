//! Butterflies (`bhvButterfly`) and the castle grounds birds (`bhvBird`), the ambient life of
//! the first courses. Ports of `src/game/behaviors/butterfly.inc.c` and `bird.inc.c`.
//! Not ported: the triplet butterflies (`bhvTripletButterfly`, a bomb-carrying variant used by
//! `macro_butterfly_triplet_no_bombs`) and the bird flap / chirp sounds (not in sounds.json).
use super::anim::{self, spec, AnimSpec};
use super::*;

pub const ACT_RESTING: i32 = 0;
pub const ACT_FOLLOW_MARIO: i32 = 1;
pub const ACT_RETURN_HOME: i32 = 2;

/// `butterfly_seg3_anims_030056B0`: 0 = flying (8 frames), 1 = resting (30 frames).
pub const BUTTERFLY_ANIMS: [AnimSpec; 2] = [spec(7, 0, 0x08, false), spec(7, 0, 0x1E, false)];
pub const BUTTERFLY_ANIM_NAMES: [&str; 2] = ["butterfly_seg3_anim_030055B0", "butterfly_seg3_anim_03005698"];
/// `birds_seg5_anims_050009E8`.
pub const BIRD_ANIMS: [AnimSpec; 2] = [spec(0, 0, 0x07, false), spec(0, 0, 0x08, false)];
pub const BIRD_ANIM_NAMES: [&str; 2] = ["birds_seg5_anim_050008D0", "birds_seg5_anim_050009D0"];
pub const BIRD_MODEL: &str = "actors/bird/birds_geo.glb";

/// bhvButterfly: DROP_TO_FLOOR, oGraphYOffset 5, bhv_butterfly_init (done on the first update,
/// it needs the floor).
pub fn init_butterfly(o: &mut SimObject) {
    o.hitbox = ObjectHitbox { interact_type: 0, ..Default::default() };
    o.graph_y_offset = 5.0;
    o.visible = true;
    o.action = ACT_RESTING;
    o.prev_action = ACT_RESTING;
    anim::init(o, 1, BUTTERFLY_ANIMS[1]);
}

/// butterfly_step
fn butterfly_step(o: &mut SimObject, world: &crate::collision::CollisionWorld, speed: f64) {
    let yaw = o.move_yaw;
    let pitch = o.var("movePitch");
    let phase = o.var("yPhase");
    let vel_x = sins(yaw) * speed;
    let vel_y = sins(pitch) * speed;
    let vel_z = coss(yaw) * speed;
    o.pos[0] += vel_x;
    o.pos[2] += vel_z;
    if o.action == ACT_FOLLOW_MARIO {
        o.pos[1] -= vel_y + coss((phase * 655.36).trunc()) * 20.0 / 4.0;
    } else {
        o.pos[1] -= vel_y;
    }
    let f = world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
    if f.surface.is_some() && o.pos[1] < f.height + 2.0 { o.pos[1] = f.height + 2.0; }
    let mut phase = phase + 1.0;
    if phase > 100.0 { phase = 0.0; }
    o.set_var("yPhase", phase);
}

/// obj_turn_toward_object for the yaw / pitch indices toward `target`.
fn turn_yaw(o: &mut SimObject, target: &Vec3, amount: f64) {
    let t = atan2s(target[2] - o.pos[2], target[0] - o.pos[0]) as f64;
    o.move_yaw = approach_s16(o.move_yaw, t, amount);
}
fn turn_pitch(o: &mut SimObject, target: &Vec3, amount: f64) {
    let horiz = js_hypot(target[0] - o.pos[0], target[2] - o.pos[2]);
    let t = atan2s(horiz, o.pos[1] - target[1]) as f64;
    let p = approach_s16(o.var("movePitch"), t, amount);
    o.set_var("movePitch", p);
}

/// bhv_butterfly_loop.
pub fn butterfly_update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let myaw = m.face_angle[1];
    let o = &mut m.objects[id];
    if !o.has_var("homed") {
        // DROP_TO_FLOOR, then bhv_butterfly_init.
        let f = world.find_floor(o.pos[0], o.pos[1] + 100.0, o.pos[2]);
        if f.surface.is_some() { o.pos[1] = f.height; }
        anim::init(o, 1, BUTTERFLY_ANIMS[1]);
        o.set_var("yPhase", (random() * 100.0).trunc());
        let fr = (random() * 7.0).trunc();
        o.set_var("animFrame", fr);
        o.set_var("animAssist", fr);
        o.home = o.pos;
        o.set_var("homed", 1.0);
    }
    match o.action {
        // butterfly_act_rest
        ACT_RESTING => {
            if dist_3d(&o.pos, &mpos) < 1000.0 {
                anim::init(o, 0, BUTTERFLY_ANIMS[0]);
                o.action = ACT_FOLLOW_MARIO;
                o.move_yaw = myaw;
            }
        }
        // butterfly_act_follow_mario
        ACT_FOLLOW_MARIO => {
            // butterfly_calculate_angle: aim at a point offset by the y phase.
            let phase = o.var("yPhase");
            let off_xz = (5.0 * phase / 4.0).trunc();
            turn_yaw(o, &[mpos[0] + off_xz, mpos[1], mpos[2] + off_xz], 0x300 as f64);
            let off_y = ((5.0 * phase + 0x100 as f64) / 4.0).trunc();
            turn_pitch(o, &[mpos[0], mpos[1] + off_y, mpos[2]], 0x500 as f64);
            butterfly_step(o, &world, 7.0);
            if dist_3d(&o.home, &mpos) >= 1200.0 { o.action = ACT_RETURN_HOME; }
        }
        // butterfly_act_return_home
        _ => {
            let d = [o.home[0] - o.pos[0], o.home[1] - o.pos[1], o.home[2] - o.pos[2]];
            let h_angle = atan2s(d[2], d[0]) as f64;
            let v_angle = atan2s(js_hypot(d[0], d[2]), -d[1]) as f64;
            o.move_yaw = approach_s16(o.move_yaw, h_angle, 0x800 as f64);
            let p = approach_s16(o.var("movePitch"), v_angle, 0x50 as f64);
            o.set_var("movePitch", p);
            butterfly_step(o, &world, 7.0);
            if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 144.0 {
                anim::init(o, 1, BUTTERFLY_ANIMS[1]);
                o.action = ACT_RESTING;
                o.pos = o.home;
            }
        }
    }
    // set_object_visibility(o, 3000)
    o.visible = dist_3d(&o.pos, &mpos) < 3000.0;
    o.face_yaw = o.move_yaw;
    end_frame(o);
}

// ---------------------------------------------------------------------------------------------
// Birds
// ---------------------------------------------------------------------------------------------

pub const BIRD_ACT_INACTIVE: i32 = 0;
pub const BIRD_ACT_FLY: i32 = 1;
/// `BIRD_BP_SPAWNER`.
pub const BIRD_BP_SPAWNER: f64 = 1.0;

/// bhvBird: HIDE, SCALE 70.
pub fn init_bird(o: &mut SimObject) {
    o.hitbox = ObjectHitbox { interact_type: 0, ..Default::default() };
    o.scale = 0.7;
    o.visible = false;
    o.action = BIRD_ACT_INACTIVE;
    o.prev_action = BIRD_ACT_INACTIVE;
    anim::init(o, 0, BIRD_ANIMS[0]);
}

/// bird_act_inactive + bird_act_fly (bhv_bird_update).
pub fn bird_update(id: ObjId, m: &mut MarioRuntime) {
    let spawned = m.objects[id].param != BIRD_BP_SPAWNER;
    match m.objects[id].action {
        BIRD_ACT_INACTIVE => {
            let dist = m.objects[id].distance_to_mario;
            if spawned || dist < 2000.0 {
                if !spawned {
                    let (pos, yaw) = (m.objects[id].pos, m.objects[id].move_yaw);
                    m.objects.sound(pos, "OBJ_STOMPED"); // SOUND_GENERAL_BIRDS_FLY_AWAY (not in sounds.json)
                    let mut kids = vec![];
                    for _ in 0..6 {
                        let mut s = ObjectSpec::new(ObjectBehaviorKind::Bird, pos, yaw);
                        s.model = Some(BIRD_MODEL.to_string());
                        s.param = Some(0.0);
                        let kid = m.objects.spawn(&s);
                        m.objects[kid].link = Some(id);
                        kids.push(kid);
                    }
                    let o = &mut m.objects[id];
                    o.home[0] = -20.0;
                    o.home[2] = -3990.0;
                    o.links = Some(kids);
                }
                let o = &mut m.objects[id];
                o.action = BIRD_ACT_FLY;
                o.set_var("movePitch", (5000.0 - (4000.0 * random()).trunc()).trunc());
                o.move_yaw = (random() * 65536.0).trunc();
                o.set_var("speed", 40.0);
                o.visible = true;
            }
        }
        _ => {
            let parent = m.objects[id].link;
            let parent_pos = Some(parent.map(|p| m.objects[p].pos).unwrap_or(m.objects[id].pos));
            let o = &mut m.objects[id];
            let speed = o.var("speed");
            let pitch = o.var("movePitch");
            // obj_compute_vel_from_move_pitch
            o.forward_vel = speed * coss(pitch);
            o.vel[1] = speed * -sins(pitch);
            if parent_pos.map_or(false, |p| p[1] > 8000.0) {
                o.deactivate(-1.0);
                return;
            }
            let (target_pitch, target_yaw);
            if spawned {
                let pp = parent_pos.unwrap_or(o.home);
                let d = js_hypot(pp[0] - o.pos[0], pp[2] - o.pos[2]);
                target_pitch = atan2s(d, o.pos[1] - pp[1]) as f64;
                target_yaw = atan2s(pp[2] - o.pos[2], pp[0] - o.pos[0]) as f64;
                o.set_var("speed", 0.04 * dist_3d(&o.pos, &pp) + 20.0);
            } else {
                let d = js_hypot(o.home[0] - o.pos[0], o.home[2] - o.pos[2]);
                target_pitch = atan2s(d, o.pos[1] - 10000.0) as f64;
                target_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
            }
            let p = approach_s16(o.var("movePitch"), target_pitch, 140.0);
            o.set_var("movePitch", p);
            rotate_yaw_toward(o, target_yaw, 800.0);
            let roll_target = to_s16(o.move_yaw - target_yaw).clamp(-0x3000, 0x3000) as f64;
            let roll = approach_s16(o.var("faceRoll"), roll_target, 600.0);
            o.set_var("faceRoll", roll);
            // cur_obj_move_using_fvel_and_gravity
            o.pos[0] += o.forward_vel * sins(o.move_yaw);
            o.pos[2] += o.forward_vel * coss(o.move_yaw);
            o.pos[1] += o.vel[1];
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    end_frame(o);
}
