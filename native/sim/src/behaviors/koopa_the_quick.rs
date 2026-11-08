//! Koopa the Quick race (src/sim/behaviors/koopaTheQuick.ts).
use super::koopa::move_koopa;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub const KOOPA_RACE_WAITING: i32 = 0;
pub const KOOPA_RACE_INVITATION: i32 = 1;
pub const KOOPA_RACE_COUNTDOWN: i32 = 2;
pub const KOOPA_RACE_RUNNING: i32 = 3;
pub const KOOPA_RACE_DECELERATING: i32 = 4;
pub const KOOPA_RACE_AT_SUMMIT: i32 = 5;
pub const KOOPA_RACE_RESULT_DIALOG: i32 = 6;
pub const KOOPA_RACE_FINISHED: i32 = 7;

pub const KOOPA_RACE_STAR: Vec3 = [3030.0, 4500.0, -4600.0];
/// `sKoopaTheQuickProperties[1].starPos` (koopa.inc.c): the THI race (race index 1, object param 1).
pub const KOOPA_RACE_STAR_THI: Vec3 = [7100.0, -1300.0, -6000.0];

/// The star of the race with this `oKoopaTheQuickRaceIndex` (0 = BoB, 1 = THI).
pub fn race_star(index: i32) -> Vec3 {
    if index == 1 { KOOPA_RACE_STAR_THI } else { KOOPA_RACE_STAR }
}

/// The waypoints of this Koopa's race: the object's own route (`thi_seg7_trajectory_koopa`) or the BoB one.
fn route(o: &SimObject) -> Vec<Vec3> {
    match &o.ball_route {
        Some(r) if !r.waypoints.is_empty() => r.waypoints.clone(),
        _ => KOOPA_RACE_ROUTE.to_vec(),
    }
}

pub const KOOPA_RACE_ROUTE: [Vec3; 36] = [
    [-2220.0, 204.0, 5520.0], [-2020.0, 204.0, 5820.0], [760.0, 765.0, 5680.0], [1920.0, 768.0, 5040.0],
    [2000.0, 768.0, 4360.0], [1240.0, 768.0, 3600.0], [-280.0, 768.0, 2720.0], [-1680.0, 768.0, 1840.0],
    [-2280.0, 0.0, 1800.0], [-2720.0, 0.0, 1120.0], [-2520.0, 0.0, 480.0], [-1720.0, 0.0, 120.0],
    [560.0, 630.0, -840.0], [2700.0, 1571.0, -1500.0], [4520.0, 1830.0, -2600.0], [6240.0, 1943.0, -3500.0],
    [6380.0, 2066.0, -6180.0], [4000.0, 2390.0, -7720.0], [-120.0, 2608.0, -5515.0], [-180.0, 2641.0, -4860.0],
    [580.0, 2769.0, -3380.0], [1670.0, 2867.0, -2580.0], [2216.0, 2900.0, -2126.0], [3167.0, 2965.0, -1870.0],
    [4069.0, 3050.0, -2487.0], [4880.0, 3037.0, -3416.0], [5020.0, 3181.0, -5760.0], [4660.0, 3354.0, -6300.0],
    [3940.0, 3514.0, -6800.0], [3200.0, 3619.0, -6850.0], [1290.0, 3768.0, -5793.0], [1150.0, 3900.0, -3670.0],
    [2980.0, 4046.0, -2930.0], [4334.0, 4186.0, -3680.0], [4180.0, 4242.0, -4220.0], [3660.0, 4242.0, -4380.0],
];

const KOOPA_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: 0,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 180.0,
    height: 300.0,
    hurtbox_radius: 180.0,
    hurtbox_height: 300.0,
};

pub fn init_koopa_the_quick(object: &mut SimObject) {
    object.scale = 3.0;
    object.gravity = 4.0;
    object.friction = 1.0;
    object.set_hitbox(KOOPA_HITBOX);
    object.set_var("animation", 7.0);
}

pub fn init_koopa_flag(object: &mut SimObject) {
    object.gravity = 0.0;
    object.visible = true;
}

pub fn koopa_flag_behavior(id: ObjId, m: &mut MarioRuntime) {
    let Some(koopa) = m.objects.find_by_behavior(ObjectBehaviorKind::KoopaTheQuick) else { return };
    if m.objects[koopa].action < KOOPA_RACE_RUNNING || m.objects[id].var("result") != 0.0 { return; }
    let e = m.objects[koopa].var("elapsed") + 1.0;
    m.objects[koopa].set_var("elapsed", e);
    let (fpos, kpos, kcannon) = (m.objects[id].pos, m.objects[koopa].pos, m.objects[koopa].var("cannon"));
    if dist_3d(&fpos, &kpos) < 400.0 {
        m.objects[id].set_var("result", -2.0);
    } else if m.objects[id].distance_to_mario < 400.0 {
        m.objects[id].set_var("result", if kcannon != 0.0 { -1.0 } else { 1.0 });
    }
}

fn follow_path(id: ObjId, m: &mut MarioRuntime, result: f64) {
    let world = m.world.clone();
    let o = &mut m.objects[id];
    let index_f = o.var("waypoint");
    let index = index_f as usize;
    let waypoints = route(o);
    let prev = waypoints[index];
    let next = waypoints[index + 1];
    let dx = next[0] - o.pos[0];
    let dy = next[1] - o.pos[1];
    let dz = next[2] - o.pos[2];
    if (next[0] - prev[0]) * dx + (next[1] - prev[1]) * dy + (next[2] - prev[2]) * dz <= 0.0 {
        let w = o.var("waypoint") + 1.0;
        o.set_var("waypoint", w);
        if w == (waypoints.len() - 1) as f64 {
            o.set_action(KOOPA_RACE_DECELERATING);
            return;
        }
    }
    // koopa.inc.c: 8 once Mario finished / cheated, else 6 for the THI race and 4 for BoB.
    let agility = if result != 0.0 && o.distance_to_mario > 1500.0 && index_f < 28.0 { 8.0 } else if o.param == 1.0 { 6.0 } else { 4.0 };
    let delta = to_s16(atan2s(dz, dx) as f64 - o.move_yaw) as f64;
    o.move_yaw = to_s16(o.move_yaw + clamp(delta, -150.0 * agility, 150.0 * agility)) as f64;
    o.face_yaw = o.move_yaw;
    let starting = o.action_timer < 34.0;
    o.action_timer += 1.0;
    let pitch = atan2s(js_hypot(dx, dz), -dy) as f64;
    let speed = if starting { 3.0 * agility } else { 6.0 * agility * (1.0 + sins(pitch)) };
    let accel = agility * if starting { 0.3 } else { 0.1 };
    o.forward_vel = approach_f32(o.forward_vel, speed, accel, accel);
    o.set_var("animation", if starting { 11.0 } else { 9.0 });
    if o.hit_wall { o.vel[1] = 20.0; }
    if o.var("edge") != 0.0 && o.grounded {
        o.vel[1] = if index_f < 8.0 { 40.0 } else { 80.0 };
        o.gravity = 6.0;
    }
    move_koopa(o, &world);
    if !o.grounded && o.vel[1] > 0.0 { o.set_var("animation", 13.0); }
}

pub fn koopa_the_quick_behavior(id: ObjId, m: &mut MarioRuntime) {
    let Some(flag) = m.objects.find_by_behavior(ObjectBehaviorKind::KoopaFlag) else { return };
    let (opos, oaction, angle_to_mario) = { let o = &m.objects[id]; (o.pos, o.action, o.angle_to_mario) };
    let dy = m.pos[1] - opos[1];
    let lateral = js_hypot(m.pos[0] - opos[0], m.pos[2] - opos[2]);
    let can_talk = lateral < 400.0 && dy > -160.0 && dy < 400.0 && m.action & ACT_FLAG_AIR == 0;
    let radius = if oaction == KOOPA_RACE_RUNNING { 180.0 } else { 140.0 };
    if dy.abs() < 300.0 && lateral < radius {
        m.pos[0] = opos[0] + radius * sins(angle_to_mario);
        m.pos[2] = opos[2] + radius * coss(angle_to_mario);
    }
    match oaction {
        KOOPA_RACE_WAITING => {
            if m.objects[id].var("cooldown") > 0.0 {
                let c = m.objects[id].var("cooldown") - 1.0;
                m.objects[id].set_var("cooldown", c);
            } else if can_talk {
                let thi = m.objects[id].param == 1.0;
                m.objects.dialogue(if thi { "KOOPA THE QUICK: Long time, no see! How about a rematch? The goal is in Windswept Valley." } else { "KOOPA THE QUICK: How about a race to the mountaintop?" });
                m.objects[id].set_action(KOOPA_RACE_INVITATION);
            }
        }
        KOOPA_RACE_INVITATION => {
            let o = &mut m.objects[id];
            let answer = o.var("answer");
            if answer < 0.0 {
                o.set_var("cooldown", 60.0);
                o.set_action(KOOPA_RACE_WAITING);
            } else if answer > 0.0 {
                o.set_var("waypoint", 0.0);
                o.set_var("elapsed", 0.0);
                o.set_var("cannon", 0.0);
                o.set_action(KOOPA_RACE_COUNTDOWN);
            }
        }
        KOOPA_RACE_COUNTDOWN => {
            let o = &mut m.objects[id];
            let t = o.action_timer;
            o.action_timer += 1.0;
            if t == 50.0 {
                m.objects.race_event("start");
                m.objects[id].set_action(KOOPA_RACE_RUNNING);
            }
        }
        KOOPA_RACE_RUNNING => {
            if m.action == ACT_SHOT_FROM_CANNON { m.objects[id].set_var("cannon", 1.0); }
            let result = m.objects[flag].var("result");
            follow_path(id, m, result);
        }
        KOOPA_RACE_DECELERATING => {
            let world = m.world.clone();
            let o = &mut m.objects[id];
            o.forward_vel = approach_f32(o.forward_vel, 3.0, 1.0, 1.0);
            move_koopa(o, &world);
            let t = o.action_timer;
            o.action_timer += 1.0;
            if t >= 20.0 {
                o.set_var("animation", 10.0);
                o.set_action(KOOPA_RACE_AT_SUMMIT);
            }
        }
        KOOPA_RACE_AT_SUMMIT => {
            let world = m.world.clone();
            let o = &mut m.objects[id];
            o.forward_vel = 0.0;
            move_koopa(o, &world);
            let t = o.action_timer;
            o.action_timer += 1.0;
            let result = m.objects[flag].var("result");
            if t >= 14.0 && can_talk && result != 0.0 {
                m.objects[id].set_var("animation", 7.0);
                let thi = m.objects[id].param == 1.0;
                m.objects.dialogue(if result == 1.0 {
                    if thi { "No way! You beat me...again! Here, I guess I have to hand over this Star. Congrats, Mario!" } else { "Whoa! You...really...are...fast! Here you go--you've won it, fair and square!" }
                } else if result == -1.0 { "You've gotta run the whole course!" } else { "I win! You lose! Better luck next time!" });
                m.objects[id].set_action(KOOPA_RACE_RESULT_DIALOG);
            }
        }
        KOOPA_RACE_RESULT_DIALOG
            if m.objects[id].var("answer") != 0.0 => {
                let result = m.objects[flag].var("result");
                m.objects.race_event(if result == 1.0 { "won" } else if result == -1.0 { "disqualified" } else { "lost" });
                m.objects[id].set_action(KOOPA_RACE_FINISHED);
            }
        _ => {}
    }
}
