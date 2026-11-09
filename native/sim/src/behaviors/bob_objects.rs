//! Port of `src/sim/behaviors/bobObjects.ts`: battlefield switch, grill and bowling-ball behaviors.
use crate::collision::{CollisionWorld, Surface, SurfaceOptions};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use std::sync::Arc;

const PURPLE_SWITCH_ACT_IDLE: f64 = 0.0;
const PURPLE_SWITCH_ACT_PRESSED: f64 = 1.0;
const PURPLE_SWITCH_ACT_TICKING: f64 = 2.0;
const PURPLE_SWITCH_ACT_UNPRESSED: f64 = 3.0;
const PURPLE_SWITCH_ACT_WAIT: f64 = 4.0;

pub const PURPLE_SWITCH_TICKING: f64 = PURPLE_SWITCH_ACT_TICKING;

const BOWLING_BALL_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE,
    down_offset: 0.0,
    damage_or_coin_value: 2.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 100.0,
    height: 150.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

/// Returns (rx, rz, fx, fz).
fn yaw_vector(yaw: f64) -> (f64, f64, f64, f64) {
    (coss(yaw), -sins(yaw), sins(yaw), coss(yaw))
}

/// Add a removable wall at an object's position.
pub fn add_barrier(world: &mut CollisionWorld, object: &mut SimObject, half_width: f64, height: f64, half_depth: f64) {
    if object.collision_surfaces.as_ref().is_some_and(|s| !s.is_empty()) { return; }
    let (rx, rz, fx, fz) = yaw_vector(object.move_yaw);
    let base_y = object.pos[1];
    let top_y = base_y + height;
    let mut surfaces: Vec<Arc<Surface>> = vec![];

    for side in [-1.0f64, 1.0] {
        let cx = object.pos[0] + fx * half_depth * side;
        let cz = object.pos[2] + fz * half_depth * side;
        let ax = cx - rx * half_width;
        let az = cz - rz * half_width;
        let bx = cx + rx * half_width;
        let bz = cz + rz * half_width;
        let quad: [Vec3; 4] = if side < 0.0 {
            [[ax, base_y, az], [bx, base_y, bz], [bx, top_y, bz], [ax, top_y, az]]
        } else {
            [[bx, base_y, bz], [ax, base_y, az], [ax, top_y, az], [bx, top_y, bz]]
        };
        surfaces.extend(world.add_quad(quad[0], quad[1], quad[2], quad[3], &SurfaceOptions::default()));
    }
    object.collision_surfaces = Some(surfaces);
}

pub fn remove_barrier(world: &mut CollisionWorld, object: &mut SimObject) {
    let Some(surfaces) = object.collision_surfaces.as_ref() else { return };
    if surfaces.is_empty() { return; }
    let ptrs: Vec<*const Surface> = surfaces.iter().map(Arc::as_ptr).collect();
    world.remove_surfaces(&|s: &Surface| ptrs.contains(&(s as *const Surface)));
    object.collision_surfaces = Some(vec![]);
}

pub fn init_purple_switch(object: &mut SimObject) {
    object.scale = 1.5;
    object.visible = true;
    object.set_var("action", PURPLE_SWITCH_ACT_IDLE);
    object.set_var("timer", 0.0);
}

fn mario_on_switch(object: &SimObject, m: &MarioRuntime) -> bool {
    let dx = m.pos[0] - object.pos[0];
    let dz = m.pos[2] - object.pos[2];
    dx * dx + dz * dz < 127.5 * 127.5
        && (m.pos[1] - object.pos[1]).abs() < 80.0
        && (m.pos[1] - m.floor_height).abs() < 20.0
}

pub fn purple_switch_behavior(id: ObjId, m: &mut MarioRuntime) {
    let t = m.objects[id].var("timer") + 1.0;
    m.objects[id].set_var("timer", t);
    let on = mario_on_switch(&m.objects[id], m);
    let object = &mut m.objects[id];
    let action = object.var("action");
    if action == PURPLE_SWITCH_ACT_IDLE {
        object.scale = 1.5;
        if on {
            object.set_var("action", PURPLE_SWITCH_ACT_PRESSED);
            object.set_var("timer", 0.0);
        }
    } else if action == PURPLE_SWITCH_ACT_PRESSED {
        object.scale = 1.5 - 1.3 * (1.0f64).min(t / 3.0);
        if t >= 3.0 {
            object.set_var("action", PURPLE_SWITCH_ACT_TICKING);
            object.set_var("timer", 0.0);
        }
    } else if action == PURPLE_SWITCH_ACT_TICKING {
        if t > 400.0 {
            object.set_var("action", PURPLE_SWITCH_ACT_WAIT);
            object.set_var("timer", 0.0);
        }
    } else if action == PURPLE_SWITCH_ACT_WAIT {
        if !on {
            object.set_var("action", PURPLE_SWITCH_ACT_UNPRESSED);
            object.set_var("timer", 0.0);
        }
    } else if action == PURPLE_SWITCH_ACT_UNPRESSED {
        object.scale = 0.2 + 1.3 * (1.0f64).min(t / 3.0);
        if t >= 3.0 {
            object.set_var("action", PURPLE_SWITCH_ACT_IDLE);
            object.set_var("timer", 0.0);
        }
    }
}

pub fn init_openable_grill(object: &mut SimObject) {
    object.visible = true;
    object.scale = 1.0;
    object.set_var("spawned", 0.0);
    object.set_var("open", 0.0);
}

fn spawn_grill_doors(id: ObjId, m: &mut MarioRuntime) {
    let (opos, oyaw) = { let o = &m.objects[id]; (o.pos, o.move_yaw) };
    let (rx, rz, _, _) = yaw_vector(oyaw);
    for side in [-1.0f64, 1.0] {
        let mut spec = ObjectSpec::new(
            ObjectBehaviorKind::GrillDoor,
            [opos[0] + rx * 320.0 * side, opos[1], opos[2] + rz * 320.0 * side],
            if side < 0.0 { to_s16(oyaw + 0x8000 as f64) as f64 } else { oyaw },
        );
        spec.model = Some("levels/bob/bob_geo_000470.glb".to_string());
        let door = m.objects.spawn(&spec);
        let d = &mut m.objects[door];
        d.link = Some(id);
        d.set_var("side", side);
        d.set_var("timer", 0.0);
    }
    m.objects[id].set_var("spawned", 1.0);
    let world = Arc::make_mut(&mut m.world);
    add_barrier(world, &mut m.objects[id], 320.0, 400.0, 40.0);
}

pub fn openable_grill_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].var("spawned") == 0.0 { spawn_grill_doors(id, m); }
    let switch_object = m.objects.find_by_behavior(ObjectBehaviorKind::PurpleSwitch);
    if let Some(sw) = switch_object {
        if m.objects[sw].var("action") == PURPLE_SWITCH_ACT_TICKING && m.objects[id].var("open") == 0.0 {
            m.objects[id].set_var("open", 1.0);
            let world = Arc::make_mut(&mut m.world);
            remove_barrier(world, &mut m.objects[id]);
            m.objects.break_object(id);
        }
    }
}

pub fn grill_door_behavior(id: ObjId, m: &mut MarioRuntime) {
    let Some(link) = m.objects[id].link else { return };
    if m.objects[link].var("open") == 0.0 { return; }
    let object = &mut m.objects[id];
    if object.var("timer") >= 64.0 { return; }
    let t = object.var("timer") + 1.0;
    object.set_var("timer", t);
    object.move_yaw = to_s16(object.move_yaw - object.var("side") * 0x100 as f64) as f64;
    object.face_yaw = object.move_yaw;
}

pub fn init_pit_bowling_ball(object: &mut SimObject) {
    object.gravity = 12.0;
    object.friction = 1.0;
    object.buoyancy = 2.0;
    object.set_hitbox(BOWLING_BALL_HITBOX);
    object.graph_y_offset = 130.0;
    object.billboard = true;
    object.visible = true;
}

pub fn pit_bowling_ball_behavior(id: ObjId, m: &mut MarioRuntime) {
    let object = &mut m.objects[id];
    object.step_physics(&m.world, None);
    let floor = m.world.find_floor(object.pos[0], object.pos[1], object.pos[2]);
    if let Some(s) = &floor.surface {
        if s.normal[0].abs() < 0.01 && s.normal[2].abs() < 0.01 {
            object.forward_vel = 28.0;
        }
    }

    object.visible = object.distance_to_mario < 3000.0;
    object.set_hitbox(BOWLING_BALL_HITBOX);
    object.face_yaw = object.move_yaw;

    if object.pos[1] < -3000.0 { object.deactivate(-1.0); }
}

pub fn init_mountain_ball(ball: &mut SimObject) {
    init_pit_bowling_ball(ball);
    ball.gravity = 5.5;
    // The behavior installs the damage cylinder after its first physics step.
    ball.set_hitbox(ObjectHitbox { interact_type: 0, radius: 50.0, height: 100.0, ..BOWLING_BALL_HITBOX });
    ball.set_action(0);
    ball.set_var("waypoint", 0.0);
}

pub fn mountain_ball_spawner(id: ObjId, m: &mut MarioRuntime) {
    let route = m.objects[id].ball_route.clone().expect("mountainBallSpawner needs ball_route");
    if m.objects[id].timer == 256.0 { m.objects[id].timer = 0.0; }
    let (timer, dist, spos) = { let o = &m.objects[id]; (o.timer, o.distance_to_mario, o.pos) };
    let rem = timer % route.period;
    // JS truthiness: NaN (period 0) counts as false.
    if (rem != 0.0 && !rem.is_nan()) || dist < route.min_distance || spos[1] < m.pos[1] || dist >= route.max_distance || (random() * route.odds).trunc() != 0.0 { return; }

    let previous = m.objects.objects.iter().position(|o| !o.active && o.behavior == ObjectBehaviorKind::MountainBall && o.ball_route.as_ref() == Some(&route));
    if let Some(p) = previous {
        let ball = &mut m.objects.objects[p];
        ball.reactivate();
        init_mountain_ball(ball);
        apply_route_look(ball);
    } else {
        let mut spec = ObjectSpec::new(ObjectBehaviorKind::MountainBall, spos, 0.0);
        spec.ball_route = Some(route);
        spec.model = Some("actors/bowling_ball/bowling_ball_geo.glb".to_string());
        let id = m.objects.spawn(&spec);
        apply_route_look(&mut m.objects[id]);
    }
}

/// `bhv_bowling_ball_initialize_loop` for THI_SMALL: `cur_obj_scale(0.3)`, `oGraphYOffset = 39`.
fn apply_route_look(ball: &mut SimObject) {
    let scale = ball.ball_route.as_ref().map_or(1.0, |r| r.scale);
    if scale != 1.0 {
        ball.scale = scale;
        ball.graph_y_offset = 39.0;
    }
}

pub fn mountain_ball_behavior(id: ObjId, m: &mut MarioRuntime) {
    let initializing = m.objects[id].action == 0;
    if !initializing {
        let ball = &mut m.objects[id];
        ball.step_physics(&m.world, None);
        if ball.grounded && ball.vel[1] > 5.0 {
            let pos = ball.pos;
            m.objects.bowling_ball_event(pos, "bounce");
        }
        let ball = &mut m.objects[id];
        ball.set_hitbox(BOWLING_BALL_HITBOX);
        ball.interact_status = 0;
    }
    let mpos = m.pos;
    let ball = &mut m.objects[id];
    let path = &ball.ball_route.as_ref().expect("mountainBall needs ball_route").waypoints;
    let wp = ball.var("waypoint") as usize;
    let (previous, next) = (path[wp], path[wp + 1]);
    let path_len = path.len();
    let dx = next[0] - ball.pos[0];
    let dy = next[1] - ball.pos[1];
    let dz = next[2] - ball.pos[2];
    let yaw = atan2s(dz, dx) as f64;
    if initializing {
        ball.move_yaw = yaw;
        ball.forward_vel = ball.ball_route.as_ref().map_or(20.0, |r| r.speed);
        ball.set_action(1);
    } else {
        ball.turn_toward_angle(yaw, 0x400 as f64);
        ball.forward_vel = (70.0f64).min(ball.forward_vel);
    }
    ball.visible = js_hypot3(ball.pos[0] - mpos[0], ball.pos[1] - mpos[1], ball.pos[2] - mpos[2]) < 4000.0;
    if (next[0] - previous[0]) * dx + (next[1] - previous[1]) * dy + (next[2] - previous[2]) * dz <= 0.0 {
        let w = ball.var("waypoint") + 1.0;
        ball.set_var("waypoint", w);
        if w == (path_len - 1) as f64 {
            let pos = ball.pos;
            if js_hypot3(pos[0] - mpos[0], pos[1] - mpos[1], pos[2] - mpos[2]) < 7000.0 {
                m.objects.bowling_ball_event(pos, "end");
            }
            m.objects[id].deactivate(-1.0);
        }
    }
}
