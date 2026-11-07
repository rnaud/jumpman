//! Snowman body/head quest in Cool, Cool Mountain (src/sim/behaviors/snowman.ts).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

const JUNCTION: Vec3 = [-4230.0, -1344.0, 1813.0];
const CHECKPOINT: Vec3 = [-402.0, 461.0, -2898.0];

pub fn init_snowman_body(o: &mut SimObject) {
    if o.param == -1.0 {
        o.scale = 1.0;
        o.graph_y_offset = 180.0;
        o.set_var("pitch", 0.0);
        o.visible = false;
        return;
    }
    o.scale = 0.4;
    o.graph_y_offset = 72.0;
    o.gravity = 10.0;
    o.buoyancy = 2.0;
    o.friction = 0.999;
    o.set_var("point", 0.0);
    o.set_var("pitch", 0.0);
    o.set_var("checked", 0.0);
    o.hitbox.interact_type = INTERACT_DAMAGE;
    o.hitbox.damage_or_coin_value = 3.0;
    o.hitbox.radius = 84.0;
    o.hitbox.height = 140.0;
}

pub fn init_snowman_head(o: &mut SimObject) {
    o.scale = 0.7;
    o.graph_y_offset = 110.0;
    o.gravity = 5.0;
    o.friction = 0.999;
    o.buoyancy = 2.0;
}

fn turn(o: &mut SimObject, target: f64) {
    o.move_yaw = to_s16(o.move_yaw + clamp(to_s16(target - o.move_yaw) as f64, -0x400 as f64, 0x400 as f64)) as f64;
}

fn push(id: ObjId, m: &mut MarioRuntime, radius: f64, height: f64) {
    let o = &m.objects[id];
    let (opos,) = (o.pos,);
    let dx = m.pos[0] - opos[0];
    let dz = m.pos[2] - opos[2];
    let d = js_hypot(dx, dz);
    if d < radius && (m.pos[1] - opos[1]).abs() < height {
        m.pos[0] += dx * (radius - d) / radius;
        m.pos[2] += dz * (radius - d) / radius;
    }
}

fn path_at(m: &MarioRuntime, i: usize) -> Vec3 { m.objects.snowman_path.as_ref().unwrap()[i] }

pub fn snowman_body(id: ObjId, m: &mut MarioRuntime) {
    let Some(path_len) = m.objects.snowman_path.as_ref().map(|p| p.len()) else { return };
    if m.objects[id].param == -1.0 { return; }
    let world = m.world.clone();
    if m.objects[id].var("placed") == 0.0 {
        let home = m.objects[id].home;
        let floor = world.find_floor(home[0], home[1], home[2]);
        let o = &mut m.objects[id];
        if floor.surface.is_some() { o.home[1] = floor.height; o.pos[1] = floor.height; }
        o.set_var("placed", 1.0);
    }
    let frame = m.objects[id].action_timer;
    m.objects[id].action_timer += 1.0;
    if m.objects[id].var("checked") == 0.0 && dist_3d(&m.pos, &CHECKPOINT) < 800.0 { m.objects[id].set_var("checked", 1.0); }
    let action = m.objects[id].action;
    if action == 0 {
        if m.objects[id].distance_to_mario < 400.0 && m.action & (ACT_FLAG_AIR | ACT_FLAG_INTANGIBLE) == 0 {
            m.objects.dialogue("I need a good head on my shoulders! Please follow me.");
            let o = &mut m.objects[id];
            o.forward_vel = 10.0;
            o.set_action(1);
        }
    } else if action == 1 || action == 2 {
        {
            let o = &mut m.objects[id];
            let r = o.hitbox.radius;
            o.step_physics(&world, Some(r));
            o.forward_vel = 70f64.min(o.forward_vel);
        }
        if m.objects[id].action == 1 {
            let point = m.objects[id].var("point") as usize;
            let last = path_at(m, point);
            let next = path_at(m, point + 1);
            let o = &mut m.objects[id];
            let dx = next[0] - o.pos[0];
            let dy = next[1] - o.pos[1];
            let dz = next[2] - o.pos[2];
            turn(o, atan2s(dz, dx) as f64);
            if dx * (next[0] - last[0]) + dy * (next[1] - last[1]) + dz * (next[2] - last[2]) <= 0.0 {
                let p = o.var("point") + 1.0;
                o.set_var("point", p);
                if p == (path_len - 1) as f64 {
                    let target = if o.var("checked") != 0.0 && (to_s16(o.angle_to_mario - o.move_yaw) as f64).abs() < 0x2000 as f64 { o.angle_to_mario } else { o.move_yaw };
                    o.set_var("target", target);
                    o.set_action(2);
                }
            }
        } else {
            let o = &mut m.objects[id];
            let target = o.var("target");
            turn(o, target);
            if dist_3d(&o.pos, &JUNCTION) < 300.0 {
                o.move_yaw = atan2s(JUNCTION[2] - o.pos[2], JUNCTION[0] - o.pos[0]) as f64;
                o.vel[1] = 80.0;
                o.forward_vel = 15.0;
                o.set_action(3);
                let pos = o.pos;
                let head = m.objects.find_by_behavior(ObjectBehaviorKind::SnowmanHead).expect("snowmanHead");
                m.objects[head].set_action(2);
                m.objects[head].vel[1] = 100.0;
                m.objects.snowman_event(pos, "bounce");
            } else if frame == 200.0 {
                o.visible = false;
                o.hitbox.interact_type = 0;
                o.pos = o.home;
                o.set_action(5);
                return;
            }
        }
        let o = &mut m.objects[id];
        let pitch = to_s16(o.var("pitch") + (o.forward_vel * 100.0 / o.scale).trunc()) as f64;
        o.set_var("pitch", pitch);
        o.scale = 1f64.min(o.scale + o.forward_vel * 0.0001);
        o.hitbox.radius = 210.0 * o.scale;
        o.hitbox.height = 350.0 * o.scale;
        o.graph_y_offset = 180.0 * o.scale;
        o.face_yaw = o.move_yaw;
        let pos = o.pos;
        m.objects.snowman_event(pos, "roll");
    } else if action == 3 {
        let o = &mut m.objects[id];
        let r = o.hitbox.radius;
        o.step_physics(&world, Some(r));
        if o.grounded {
            o.pos[0] = JUNCTION[0];
            o.pos[2] = JUNCTION[2];
            o.forward_vel = 0.0;
            if o.vel[1].trunc() == 0.0 {
                o.hitbox.interact_type = 0;
                o.set_action(4);
            }
        }
    } else if action == 4 {
        push(id, m, 210.0, 550.0);
    } else if m.objects[id].distance_to_mario >= 3000.0 {
        let o = &mut m.objects[id];
        o.reactivate();
        o.move_yaw = 0.0;
        o.face_yaw = 0.0;
        o.set_action(0);
        init_snowman_body(o);
    }
}

pub fn snowman_head(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    if m.objects[id].var("placed") == 0.0 {
        let home = m.objects[id].home;
        let floor = world.find_floor(home[0], home[1], home[2]);
        let o = &mut m.objects[id];
        if floor.surface.is_some() { o.pos[1] = floor.height; }
        o.set_var("placed", 1.0);
    }
    let action = m.objects[id].action;
    if action == 0 || action == 4 {
        let o = &m.objects[id];
        let limit = if action == 4 { 700.0 } else { 400.0 };
        if o.distance_to_mario < limit && m.action & (ACT_FLAG_AIR | ACT_FLAG_INTANGIBLE) == 0
            && (to_s16(o.face_yaw - m.face_angle[1] - 0x8000 as f64) as f64).abs() < 0x1000 as f64
            && (to_s16(o.move_yaw - o.angle_to_mario) as f64).abs() < 0x1000 as f64
        {
            let pos = o.pos;
            if action == 4 {
                m.objects.dialogue("Perfect! What a great new body! Here is a present for you.");
                m.objects.snowman_event(pos, "reward");
            } else {
                m.objects.dialogue("I need a body! Have you seen one rolling by?");
            }
            m.objects[id].set_action(1);
        }
    } else if action == 2 {
        if m.objects[id].step_physics(&world, Some(0.0)) & OBJ_COL_FLAG_NO_Y_VEL != 0 { m.objects[id].set_action(3); }
    } else if action == 3 {
        let o = &mut m.objects[id];
        o.step_physics(&world, Some(0.0));
        if o.pos[1] < -994.0 {
            o.pos[1] = -994.0;
            o.set_action(4);
            let pos = o.pos;
            m.objects.snowman_event(pos, "assembled");
        }
    }
    push(id, m, 180.0, 150.0);
}
