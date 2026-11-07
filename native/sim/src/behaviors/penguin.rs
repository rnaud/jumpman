//! Penguin behaviors for Cool, Cool Mountain (src/sim/behaviors/penguin.ts).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

const PENGUIN_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 45.0,
    height: 90.0,
    hurtbox_radius: 40.0,
    hurtbox_height: 80.0,
};

pub fn init_penguin(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.set_hitbox(PENGUIN_HITBOX);
    object.set_var("dir", 1.0);
    object.set_var("timer", 0.0);
    object.visible = true;
}

pub fn penguin_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let object = &mut m.objects[id];
    let timer = object.var("timer") + 1.0;
    object.set_var("timer", timer);
    if timer % 150.0 == 0.0 { let d = -object.var("dir"); object.set_var("dir", d); }
    object.forward_vel = 1.4 * object.var("dir");
    object.face_yaw = object.move_yaw;
    let step = object.step_physics(&world, None);
    if step == 1 {
        let d = -object.var("dir");
        object.set_var("dir", d);
        object.forward_vel = -object.forward_vel;
    }
}

// Small penguins and Tuxie's mother in tuxie.inc.c use the second behavior byte as a family ID.
pub fn init_baby_penguin(o: &mut SimObject) {
    o.gravity = 4.0;
    o.set_hitbox(ObjectHitbox { interact_type: INTERACT_GRABBABLE, radius: 40.0, height: 40.0, ..PENGUIN_HITBOX });
    o.interact_subtype = INT_SUBTYPE_HOLDABLE_NPC;
    o.set_var("animation", 0.0);
}

pub fn init_mother(o: &mut SimObject) {
    o.scale = 4.0;
    o.gravity = 4.0;
    o.set_var("animation", 3.0);
    o.set_hitbox(ObjectHitbox { radius: 200.0, height: 300.0, ..PENGUIN_HITBOX });
}

fn place_on_floor(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].var("placed") != 0.0 { return; }
    let pos = m.objects[id].pos;
    let hit = m.world.find_floor(pos[0], pos[1] + 200.0, pos[2]);
    let o = &mut m.objects[id];
    if hit.surface.is_some() { o.pos[1] = hit.height; o.home[1] = o.pos[1]; }
    o.set_var("placed", 1.0);
}

pub fn baby_penguin_behavior(id: ObjId, m: &mut MarioRuntime) {
    place_on_floor(id, m);
    if m.objects[id].held_state == HELD_HELD {
        let o = &mut m.objects[id];
        o.set_var("animation", 0.0);
        let (timer, pos) = (o.timer, o.pos);
        if o.var("rejected") != 0.0 { o.set_var("rejected", 0.0); }
        if timer % 30.0 == 0.0 { m.objects.penguin_sound(pos, false); }
        return;
    }
    if m.objects[id].held_state != HELD_FREE {
        let o = &mut m.objects[id];
        o.set_held_state(HELD_FREE);
        o.forward_vel = 0.0;
        o.vel[1] = 0.0;
        let a = if o.var("stayNearMother") != 0.0 { 5 } else { 0 };
        o.set_action(a);
    }
    let mother = m.objects.find_by_behavior(ObjectBehaviorKind::TuxiesMother);
    let mother_pos = mother.map(|mo| m.objects[mo].pos);
    let distance = match mother_pos { Some(mp) => dist_3d(&m.objects[id].pos, &mp), None => f64::INFINITY };
    let frame = m.objects[id].action_timer;
    m.objects[id].action_timer += 1.0;
    let mario_pos = m.pos;
    let mario_action = m.action;
    let action = m.objects[id].action;
    match action {
        0 => {
            let o = &mut m.objects[id];
            o.set_var("animation", 3.0);
            if frame == 0.0 {
                o.set_var("turn", (random() * 0x400 as f64).floor());
                o.set_var("range", random() * 100.0);
                o.set_var("speed", random());
                o.forward_vel = 0.0;
            }
            let range = o.var("range");
            if frame == 0.0 && distance < 1000.0 { o.set_action(5); }
            else if o.distance_to_mario < 1000.0 && o.distance_to_mario > 600.0 + range { o.set_action(1); }
            else if o.distance_to_mario < 300.0 + range { o.set_action(2); }
            if o.distance_to_mario > 2000.0 && dist_3d(&o.home, &mario_pos) > 2000.0 { o.pos = o.home; }
        }
        1 | 2 => {
            let o = &mut m.objects[id];
            let flee = o.action == 2;
            o.set_var("animation", 0.0);
            o.forward_vel = 3.0 + o.var("speed");
            let goal = o.angle_to_mario + if flee { 0x8000 as f64 } else { 0.0 };
            o.turn_toward_angle(goal, 0x600 as f64 + o.var("turn"));
            let range = o.var("range");
            if flee && frame == 0.0 && distance < 1000.0 { o.set_action(5); }
            else if if flee { o.distance_to_mario > 500.0 + range } else { o.distance_to_mario < 300.0 + range || o.distance_to_mario > 1100.0 } { o.set_action(0); }
        }
        3 => {
            if frame == 6.0 { let pos = m.objects[id].pos; m.objects.penguin_sound(pos, true); }
            let o = &mut m.objects[id];
            if frame >= 6.0 { o.set_var("animation", 1.0); }
            if frame > 25.0 && mario_action != ACT_DIVE_SLIDE { o.set_action(4); }
        }
        4 => {
            let o = &mut m.objects[id];
            if frame > 20.0 { o.forward_vel = 0.0; o.set_var("animation", 2.0); }
            if frame > 40.0 { let a = o.var("beforeDive") as i32; o.set_action(a); }
        }
        5 => {
            if let Some(mp) = mother_pos {
                let o = &mut m.objects[id];
                o.set_var("animation", 0.0);
                o.forward_vel = if o.distance_to_mario < 1000.0 { 2.0 } else { 0.0 };
                let yaw = atan2s(mp[2] - o.pos[2], mp[0] - o.pos[0]) as f64;
                o.turn_toward_angle(yaw + if distance > 200.0 { 0.0 } else { 0x8000 as f64 }, 0x400 as f64);
            }
        }
        _ => {}
    }
    let world = m.world.clone();
    let o = &mut m.objects[id];
    if (o.action == 1 || o.action == 2 || o.action == 5) && mario_action == ACT_DIVE_SLIDE {
        o.set_var("beforeDive", o.action as f64);
        o.set_action(3);
    }
    o.face_yaw = o.move_yaw;
    o.step_physics(&world, Some(30.0));
}

pub fn mother_behavior(id: ObjId, m: &mut MarioRuntime) {
    place_on_floor(id, m);
    {
        let o = &mut m.objects[id];
        o.forward_vel = 0.0;
        o.set_var("animation", 3.0);
    }
    if m.objects[id].action == 1 {
        if let Some(link) = m.objects[id].link {
            if m.objects[link].held_state != HELD_FREE { return; }
            let pos = m.objects[id].pos;
            if m.objects[link].param == m.objects[id].param {
                m.objects[link].set_var("returned", 1.0);
                m.objects.penguin_reward([pos[0], pos[1] + 200.0, pos[2]]);
            } else {
                m.objects[link].set_var("rejected", 1.0);
            }
            m.objects[id].set_action(2);
        }
    }
    let mut nearest: Option<ObjId> = None;
    let mut distance = f64::INFINITY;
    let opos = m.objects[id].pos;
    for i in 0..m.objects.objects.len() {
        let baby = &m.objects.objects[i];
        if !baby.active || baby.behavior != ObjectBehaviorKind::BabyPenguin || baby.var("returned") != 0.0 || baby.var("rejected") != 0.0 { continue; }
        let d = dist_3d(&baby.pos, &opos);
        if d < distance { nearest = Some(ObjId(i as u32)); distance = d; }
    }
    let (action, dist_to_mario, told, param) = { let o = &m.objects[id]; (o.action, o.distance_to_mario, o.var("told"), o.param) };
    match nearest {
        Some(n) if distance < 300.0 && m.objects[n].held_state != HELD_FREE => {
            m.objects[id].link = Some(n);
            m.objects[n].set_var("stayNearMother", 1.0);
            m.objects.dialogue(if m.objects[n].param == param { "You found my precious, precious baby!" } else { "That's not my baby! She looks nothing like me!" });
            if m.held_obj == Some(n) {
                m.drop_held_object();
                m.set_action(ACT_IDLE, 0.0);
            }
            m.objects[id].set_action(1);
        }
        _ if action == 0 && distance >= 500.0 && dist_to_mario < 300.0 && told == 0.0 => {
            m.objects.dialogue("Egad! My baby!! Have you seen my baby???");
            m.objects[id].set_var("told", 1.0);
        }
        _ if dist_to_mario > 450.0 => m.objects[id].set_var("told", 0.0),
        _ => {}
    }
    if m.objects[id].action == 2 {
        if let Some(h) = m.held_obj {
            if m.objects[h].var("returned") != 0.0 && dist_3d(&m.objects[id].pos, &m.objects[h].pos) < 1000.0 {
                let (mx, mz) = (m.pos[0], m.pos[2]);
                let o = &mut m.objects[id];
                let range = js_hypot(mx - o.home[0], mz - o.home[2]);
                if range > 800.0 { o.set_var("stopped", 1.0); }
                if range < 700.0 { o.set_var("stopped", 0.0); }
                if o.var("stopped") == 0.0 {
                    o.forward_vel = 10.0;
                    o.set_var("animation", 0.0);
                    o.turn_toward_angle(o.angle_to_mario, 0x400 as f64);
                }
            }
        }
    }
    let world = m.world.clone();
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    o.step_physics(&world, Some(30.0));
}
