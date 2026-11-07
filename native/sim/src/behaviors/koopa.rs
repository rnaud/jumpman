//! Regular Koopa and the rideable shell (src/sim/behaviors/koopa.ts). Koopa the Quick shares
//! the terrain movement ([`move_koopa`]).
use crate::collision::CollisionWorld;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

/// `Number(o.vars[key] ?? default)`.
pub(super) fn var_or(o: &SimObject, key: &str, default: f64) -> f64 {
    if o.has_var(key) { o.var(key) } else { default }
}

/// `Math.sin(yaw * Math.PI * 2 / 65536)` (JS evaluation order).
pub(super) fn js_sin_yaw(yaw: f64) -> f64 { js_sin(yaw * std::f64::consts::PI * 2.0 / 65536.0) }
/// `Math.cos(yaw * Math.PI * 2 / 65536)`.
pub(super) fn js_cos_yaw(yaw: f64) -> f64 { js_cos(yaw * std::f64::consts::PI * 2.0 / 65536.0) }

const KOOPA_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0x80,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: -1.0,
    radius: 60.0,
    height: 40.0,
    hurtbox_radius: 40.0,
    hurtbox_height: 30.0,
};

const SHELL_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0x80000,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 50.0,
    height: 50.0,
    hurtbox_radius: 50.0,
    hurtbox_height: 50.0,
};

pub fn init_koopa(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.set_hitbox(KOOPA_HITBOX);
    object.set_var("rest", 0.0);
    object.set_var("animation", 7.0);
    object.set_var("hit", 0.0);
    object.visible = true;
}

pub fn init_koopa_shell(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.set_hitbox(SHELL_HITBOX);
    object.visible = true;
}

/// cur_obj_move_standard(-78) for Koopa's zero drag and bounciness.
pub fn move_koopa(o: &mut SimObject, world: &CollisionWorld) {
    let wall = world.resolve_and_return_wall_collisions(&mut o.pos, 10.0, 50.0);
    o.hit_wall = match &wall {
        Some(w) => w.normal[0] * sins(o.move_yaw) + w.normal[2] * coss(o.move_yaw) < 0.0,
        None => false,
    };
    let floor = world.find_floor(o.pos[0], o.pos[1], o.pos[2]);
    let x = o.pos[0] + o.forward_vel * sins(o.move_yaw);
    let z = o.pos[2] + o.forward_vel * coss(o.move_yaw);
    let next = world.find_floor(x, o.pos[1], z);
    let delta = next.height - floor.height;
    o.set_var("edge", 0.0);
    let edge = match &next.surface {
        None => true,
        Some(s) => (delta < -50.0 && o.grounded) || (delta < 5.0 && s.normal[1] <= 0.208),
    };
    if edge {
        o.set_var("edge", 1.0);
    } else if next.surface.as_ref().unwrap().normal[1] > 0.208 || o.pos[1] > next.height {
        o.pos[0] = x;
        o.pos[2] = z;
    } else {
        o.hit_wall = true;
    }
    o.vel[1] = (-78f64).max(o.vel[1] - o.gravity);
    o.pos[1] += o.vel[1];
    o.grounded = o.pos[1] <= floor.height;
    if o.grounded {
        o.pos[1] = floor.height;
        o.vel[1] = 0.0;
    }
}

pub const KOOPA_IDLE: i32 = 0;
pub const KOOPA_WALK: i32 = 1;
pub const KOOPA_FLEE: i32 = 2;
pub const KOOPA_LYING: i32 = 3;
pub const KOOPA_RUN: i32 = 4;
pub const KOOPA_DIVE: i32 = 5;
pub const KOOPA_SQUISHED: i32 = 6;
pub const KOOPA_KNOCKBACK: i32 = 7;

fn shed_shell(id: ObjId, m: &mut MarioRuntime, stomp: bool) {
    let pos = m.objects[id].pos;
    m.objects.koopa_sound(pos, "shell");
    {
        let o = &mut m.objects[id];
        o.set_var("unshelled", 1.0);
        o.set_var("animation", 2.0);
        o.set_action(KOOPA_LYING);
        o.set_var("rest", 0.0);
        o.forward_vel = 20.0;
        if !stomp { o.move_yaw = to_s16(o.angle_to_mario + 0x8000 as f64) as f64; }
    }
    let link = m.objects[id].link;
    match link {
        Some(l) if !m.objects[l].active => {
            let pos = m.objects[id].pos;
            let s = &mut m.objects[l];
            s.reactivate();
            init_koopa_shell(s);
            s.pos = pos;
        }
        _ => {
            let (pos, yaw) = { let o = &m.objects[id]; (o.pos, o.move_yaw) };
            let mut spec = ObjectSpec::new(ObjectBehaviorKind::KoopaShell, pos, yaw);
            spec.model = Some("actors/koopa_shell/koopa_shell_geo.glb".to_string());
            let new = m.objects.spawn(&spec);
            m.objects[id].link = Some(new);
        }
    }
}

pub fn koopa_behavior(id: ObjId, m: &mut MarioRuntime) {
    let attack = m.objects[id].var("hit");
    let t = m.objects[id].action_timer;
    m.objects[id].action_timer += 1.0;
    if attack != 0.0 {
        m.objects[id].set_var("hit", 0.0);
        let attack_i = attack as i32;
        let stomp = attack_i == ATTACK_FROM_ABOVE || attack_i == ATTACK_GROUND_POUND_OR_TWIRL;
        if m.objects[id].var("unshelled") == 0.0 {
            shed_shell(id, m, stomp);
        } else {
            let pos = m.objects[id].pos;
            if stomp { m.objects.koopa_sound(pos, "stomp"); }
            let o = &mut m.objects[id];
            o.set_action(if stomp { KOOPA_SQUISHED } else { KOOPA_KNOCKBACK });
            let vertical = attack_i == ATTACK_FAST_ATTACK || attack_i == ATTACK_KICK_OR_TRIP;
            o.set_var("vertical", if vertical { 1.0 } else { 0.0 });
            o.forward_vel = if stomp { 0.0 } else if vertical { 20.0 } else { 50.0 };
            o.vel[1] = if stomp { 0.0 } else if vertical { 50.0 } else { 30.0 };
            o.move_yaw = to_s16(o.angle_to_mario + 0x8000 as f64) as f64;
        }
    }
    let action = m.objects[id].action;
    if action == KOOPA_SQUISHED || action == KOOPA_KNOCKBACK {
        let finished = {
            let o = &m.objects[id];
            if o.action == KOOPA_SQUISHED { o.action_timer > 15.0 }
            else { o.action_timer > 1.0 && (o.grounded || o.hit_wall || (o.var("vertical") != 0.0 && o.action_timer >= 9.0)) }
        };
        if finished {
            let pos = m.objects[id].pos;
            m.objects.koopa_sound(pos, "defeated");
            m.objects.spawn_coin(pos, true, 5.0);
            m.objects[id].deactivate(-1.0);
            return;
        }
    } else if action == KOOPA_LYING || action == KOOPA_DIVE {
        {
            let o = &mut m.objects[id];
            if o.grounded { o.forward_vel = 0f64.max(o.forward_vel - if o.var("unshelled") != 0.0 { 0.5 } else { 0.3 }); }
        }
        let link = m.objects[id].link;
        if action == KOOPA_DIVE && t > 10.0 {
            if let Some(l) = link {
                let (opos, lpos, lactive) = (m.objects[id].pos, m.objects[l].pos, m.objects[l].active);
                if lactive && dist_3d(&opos, &lpos) < 50.0 && dist_3d(&m.pos, &lpos) > 200.0 {
                    m.objects[l].deactivate(-1.0);
                    let o = &mut m.objects[id];
                    o.set_var("unshelled", 0.0);
                    o.set_var("animation", 5.0);
                    o.forward_vel *= 0.5;
                    o.set_action(KOOPA_LYING);
                }
            }
        }
        let o = &mut m.objects[id];
        if o.forward_vel == 0.0 && o.var("rest") >= 20.0 { o.set_var("animation", 6.0); }
        if o.forward_vel == 0.0 {
            let rest = o.var("rest") + 1.0;
            o.set_var("rest", rest);
            if rest >= 34.0 {
                o.set_var("rest", 0.0);
                o.set_action(if o.var("unshelled") != 0.0 { KOOPA_RUN } else { KOOPA_IDLE });
            }
        }
    } else if action == KOOPA_RUN {
        let shell = m.objects[id].link.filter(|l| m.objects[*l].active);
        let shell_pos = shell.map(|s| m.objects[s].pos);
        let o = &m.objects[id];
        let yaw = match shell_pos { Some(sp) => atan2s(sp[2] - o.pos[2], sp[0] - o.pos[0]) as f64, None => o.move_yaw };
        let away = to_s16(o.angle_to_mario + 0x8000 as f64) as f64;
        let goal = if o.distance_to_mario > 800.0 || (to_s16(yaw - away) as f64).abs() < 0x2000 as f64 { yaw } else { away };
        let o = &mut m.objects[id];
        o.set_var("animation", 3.0);
        o.turn_toward_angle(goal, 0x600 as f64);
        o.forward_vel = approach_f32(o.forward_vel, 20.0, 1.0, 1.0);
        if let Some(sp) = shell_pos {
            if o.forward_vel == 20.0 && dist_3d(&o.pos, &sp) < 600.0 && (to_s16(yaw - o.move_yaw) as f64).abs() < 0xc00 as f64 {
                o.set_action(KOOPA_DIVE);
                o.set_var("animation", 5.0);
                o.move_yaw = yaw;
                o.forward_vel *= 1.2;
                o.vel[1] = dist_3d(&o.pos, &sp) / 20.0;
            }
        }
    } else if action == KOOPA_FLEE {
        let o = &mut m.objects[id];
        o.set_var("animation", 1.0);
        let homeward = dist_3d(&o.home, &o.pos) > 1000.0;
        if t > 30.0 && !homeward && o.distance_to_mario > 800.0 {
            o.forward_vel = 0f64.max(o.forward_vel - 1.0);
            if o.forward_vel == 0.0 { o.set_action(KOOPA_IDLE); }
        } else {
            let goal = if homeward { atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64 } else { o.angle_to_mario + 0x8000 as f64 };
            o.turn_toward_angle(goal, 0x400 as f64);
            o.forward_vel = approach_f32(o.forward_vel, 17.0, 1.0, 1.0);
        }
    } else {
        let o = &mut m.objects[id];
        if o.action == KOOPA_IDLE {
            o.forward_vel = 0.0;
            o.set_var("animation", 7.0);
            if t >= 59.0 {
                let target = o.move_yaw + if random() < 0.5 { -0x2000 as f64 } else { 0x2000 as f64 };
                o.set_var("target", target);
                o.set_var("walkFrames", 34.0 + 30.0 + (random() * 100.0).floor());
                o.set_action(KOOPA_WALK);
            }
        } else {
            let walk_frames = o.var("walkFrames");
            o.set_var("animation", if t < 34.0 { 11.0 } else if t < walk_frames { 9.0 } else { 10.0 });
            if o.hit_wall || o.var("edge") != 0.0 { o.set_var("target", o.move_yaw + 0x8000 as f64); }
            let target = o.var("target");
            o.turn_toward_angle(target, 0x200 as f64);
            o.forward_vel = approach_f32(o.forward_vel, if t < walk_frames { 3.0 } else { 0.0 }, 0.3, 1.0);
            if t >= walk_frames + 14.0 { o.set_action(KOOPA_IDLE); }
        }
        if o.distance_to_mario < 300.0 && (to_s16(o.angle_to_mario - o.move_yaw) as f64).abs() < 0x3000 as f64 { o.set_action(KOOPA_FLEE); }
    }
    let world = m.world.clone();
    let o = &mut m.objects[id];
    move_koopa(o, &world);
    o.face_yaw = o.move_yaw;
}

pub fn koopa_shell_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.riding_shell == Some(id) { return; }
    let world = m.world.clone();
    let o = &mut m.objects[id];
    o.step_physics(&world, None);
    o.face_yaw = to_s16(o.face_yaw + 0x1000 as f64) as f64;
}
