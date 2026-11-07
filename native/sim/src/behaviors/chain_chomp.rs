//! Port of `src/sim/behaviors/chainChomp.ts` (chain_chomp.inc.c): Chain Chomp, wooden post, gate.
use super::king_bobomb::ready_for_npc_dialog;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

const CHAIN_CHOMP_ACT_UNINITIALIZED: i32 = 0;
const CHAIN_CHOMP_ACT_MOVE: i32 = 1;

const SUB_ACT_TURN: f64 = 0.0;
const SUB_ACT_LUNGE: f64 = 1.0;

const CHAIN_MAX_DIST: f64 = 750.0;
const CHAIN_MAX_DIST_LUNGE: f64 = 900.0;

const CHAIN_CHOMP_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_MR_BLIZZARD,
    down_offset: 0.0,
    damage_or_coin_value: 3.0,
    health: 1.0,
    num_loot_coins: 0.0,
    radius: 80.0,
    height: 160.0,
    hurtbox_radius: 80.0,
    hurtbox_height: 160.0,
};

pub fn init_chain_chomp(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.buoyancy = 2.0;
    object.scale = 2.0;
    object.graph_y_offset = 240.0;
    object.set_var("jawFrame", 0.0);
    object.set_var("segmentRadius", 0.0);
    object.set_var("linkSpacing", 0.0);
    object.set_var("restricted", 0.0);
    object.set_var("recoil", 0.0);
    object.set_var("facePitch", 0.0);
    object.set_var("targetPitch", 0.0);
    object.visible = false;
    object.set_hitbox(CHAIN_CHOMP_HITBOX);
    object.set_action(CHAIN_CHOMP_ACT_UNINITIALIZED);
    object.set_var("timer", 0.0);
    object.set_var("subAction", SUB_ACT_TURN);
    object.set_var("release", 0.0);
    object.set_var("releaseTimer", 0.0);
    object.set_var("hitGate", 0.0);
}

pub fn init_wooden_post(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.buoyancy = 2.0;
    object.scale = 0.5;
    object.visible = true;
    object.set_var("offsetY", 0.0);
    object.set_var("speedY", 0.0);
    object.set_var("pounded", 0.0);
    object.set_var("totalAngle", 0.0);
    object.set_var("prevAngle", 0.0);
    object.set_var("rewarded", 0.0);
}

pub fn init_chain_chomp_gate(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 1.0;
    object.visible = true;
}

pub fn init_chain_part(object: &mut SimObject) {
    object.gravity = 0.0;
    object.friction = 1.0;
    object.visible = true;
    object.scale = 2.0;
    object.graph_y_offset = 40.0;
    object.billboard = true;
}

fn spawn_chain(id: ObjId, m: &mut MarioRuntime) {
    let home = m.objects[id].home;
    let mut spec = ObjectSpec::new(ObjectBehaviorKind::WoodenPost, home, 0.0);
    spec.model = Some("actors/poundable_pole/wooden_post_geo.glb".to_string());
    let post = m.objects.spawn(&spec);
    let post_floor = m.world.find_floor(home[0], home[1] + 200.0, home[2]);
    let p = &mut m.objects[post];
    if post_floor.surface.is_some() { p.pos[1] = post_floor.height; }
    p.home[1] = p.pos[1];
    p.link = Some(id);

    let mut parts = vec![];
    for i in 0..4 {
        let mut spec = ObjectSpec::new(ObjectBehaviorKind::ChainPart, home, 0.0);
        spec.model = Some("actors/chain_ball/metallic_ball_geo.glb".to_string());
        let part = m.objects.spawn(&spec);
        let pp = &mut m.objects[part];
        pp.link = Some(id);
        pp.link_index = Some(i as f64);
        parts.push(part);
    }

    let mut links = vec![post];
    links.extend(parts);
    m.objects[id].links = Some(links);
    if let Some(gate) = m.objects.find_by_behavior(ObjectBehaviorKind::ChainChompGate) {
        m.objects[gate].link = Some(id);
    }
    update_chain(id, m, None);
}

/// `updateChain(object, pivot = object.links[0].home)`.
fn update_chain(id: ObjId, m: &mut MarioRuntime, pivot: Option<Vec3>) {
    let links = m.objects[id].links.clone().expect("chain links");
    let pivot = pivot.unwrap_or(m.objects[links[0]].home);
    let (mut previous, vel_y, spacing, seg_radius, vis) = {
        let o = &m.objects[id];
        (o.pos, o.vel[1], o.var("linkSpacing"), o.var("segmentRadius"), o.visible && o.active)
    };
    for (index, &part) in links[1..].iter().enumerate() {
        let p = &mut m.objects[part];
        p.pos[1] = pivot[1].max(p.pos[1] + if vel_y < 0.0 { vel_y } else { -20.0 });
        limit_distance(&mut p.pos, &previous, spacing);
        limit_distance(&mut p.pos, &pivot, seg_radius * (4 - index as i32) as f64);
        p.visible = vis;
        previous = p.pos;
    }
}

// Negative spacing is intentional: the source alternates its sign during recoil.
fn limit_distance(pos: &mut Vec3, origin: &Vec3, limit: f64) -> bool {
    let dx = pos[0] - origin[0];
    let dy = pos[1] - origin[1];
    let dz = pos[2] - origin[2];
    let distance = js_hypot3(dx, dy, dz);
    if distance == 0.0 || distance <= limit { return false; }
    let ratio = limit / distance;
    pos[0] = origin[0] + dx * ratio;
    pos[1] = origin[1] + dy * ratio;
    pos[2] = origin[2] + dz * ratio;
    true
}

fn move_chomp(id: ObjId, m: &mut MarioRuntime, wall_radius: f64) {
    let object = &mut m.objects[id];
    if wall_radius != 0.0 {
        let wall = m.world.resolve_and_return_wall_collisions(&mut object.pos, 10.0, wall_radius);
        object.hit_wall = match &wall {
            Some(w) => w.normal[0] * sins(object.move_yaw) + w.normal[2] * coss(object.move_yaw) < 0.0,
            None => false,
        };
        if object.hit_wall {
            if let Some(w) = &wall {
                object.set_var("wallYaw", atan2s(w.normal[2], w.normal[0]) as f64);
            }
        }
    }
    // cur_obj_move_standard(78), zero drag, -50% bounce.
    let floor = m.world.find_floor(object.pos[0], object.pos[1], object.pos[2]);
    let x = object.pos[0] + object.forward_vel * sins(object.move_yaw);
    let z = object.pos[2] + object.forward_vel * coss(object.move_yaw);
    let next = m.world.find_floor(x, object.pos[1], z);
    if let Some(s) = &next.surface {
        if next.height - floor.height < 5.0 || s.normal[1] > coss(78.0 * 182.0) || object.pos[1] > next.height {
            object.pos[0] = x;
            object.pos[2] = z;
        }
    }
    object.vel[1] = (-78.0f64).max(object.vel[1] - object.gravity);
    object.pos[1] += object.vel[1];
    object.grounded = floor.surface.is_some() && object.pos[1] < floor.height;
    if object.grounded {
        object.pos[1] = floor.height;
        if object.vel[1] < 0.0 { object.vel[1] *= -0.5; }
    }
}

fn released_step(id: ObjId, m: &mut MarioRuntime) {
    let phase = m.objects[id].var("release");
    m.objects[id].gravity = 4.0;
    if phase == 1.0 {
        m.objects[id].forward_vel = 0.0;
        if m.action == ACT_READING_NPC_DIALOG && m.used_obj == Some(id) {
            if m.action_state == 8.0 && m.objects[id].grounded {
                let o = &mut m.objects[id];
                o.set_var("release", 2.0);
                o.set_var("releaseTimer", 0.0);
                o.set_var("lunges", 0.0);
            }
        } else if ready_for_npc_dialog(m) {
            m.used_obj = Some(id);
            m.set_action(ACT_READING_NPC_DIALOG, 2.0);
        }
    } else if phase == 2.0 {
        let o = &mut m.objects[id];
        let seg = approach_f32(o.var("segmentRadius"), CHAIN_MAX_DIST / 5.0, 4.0, 4.0);
        o.set_var("segmentRadius", seg);
        o.set_var("linkSpacing", seg);
        let rt = o.var("releaseTimer") + 1.0;
        o.set_var("releaseTimer", rt);
        if o.grounded && o.var("lunges") == 0.0 {
            o.turn_toward_angle(o.angle_to_mario, 800.0);
            if to_s16(o.move_yaw - o.angle_to_mario) != 0 {
                o.set_var("releaseTimer", 0.0);
            } else if rt > 60.0 {
                o.set_var("lunges", 1.0);
            }
        } else if o.grounded {
            let lunges = o.var("lunges") + 1.0;
            o.set_var("lunges", lunges);
            if lunges <= 5.0 {
                o.move_yaw = to_s16(o.angle_to_mario + if random() < 0.5 { -0x2000 as f64 } else { 0x2000 as f64 }) as f64;
                o.forward_vel = 30.0;
                o.vel[1] = 50.0;
            } else {
                let dx = 1450.0 - o.pos[0];
                let dz = 562.0 - o.pos[2];
                o.move_yaw = atan2s(dz, dx) as f64;
                o.forward_vel = js_hypot(dx, dz) / 8.0;
                o.set_var("release", 3.0);
                o.vel[1] = 50.0;
            }
        }
    } else if phase == 3.0 && m.objects[id].hit_wall && m.objects[id].var("hitGate") == 0.0 {
        let o = &mut m.objects[id];
        o.set_var("hitGate", 1.0);
        o.move_yaw = to_s16(2.0 * o.var("wallYaw") - o.move_yaw + 0x8000 as f64) as f64;
        o.forward_vel *= 0.4;
    } else if phase == 3.0 && m.objects[id].var("hitGate") != 0.0 && m.objects[id].grounded {
        let o = &mut m.objects[id];
        let dx = 3288.0 - o.pos[0];
        let dz = -1770.0 - o.pos[2];
        o.move_yaw = atan2s(dz, dx) as f64;
        o.forward_vel = js_hypot(dx, dz) / 50.0;
        o.vel[1] = 120.0;
        o.set_var("release", 4.0);
    } else if phase == 4.0 && m.objects[id].grounded {
        let o = &mut m.objects[id];
        o.set_var("release", 5.0);
        o.set_var("releaseTimer", 0.0);
    } else if phase == 5.0 && {
        let rt = m.objects[id].var("releaseTimer") + 1.0;
        m.objects[id].set_var("releaseTimer", rt);
        rt >= 15.0
    } {
        if m.action == ACT_READING_NPC_DIALOG && m.used_obj == Some(id) { m.action_state = 9.0; }
        if let Some(links) = m.objects[id].links.clone() {
            for &part in &links[1..] { m.objects[part].deactivate(-1.0); }
        }
        m.objects[id].deactivate(-1.0);
        return;
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    let wall_radius = if o.var("lunges") > 0.0 { 200.0 } else { 0.0 };
    move_chomp(id, m, wall_radius);
    // The free pivot follows the Chomp once the tether reaches its full length.
    let links = m.objects[id].links.clone().expect("chain links");
    let post_home = m.objects[links[0]].home;
    let o = &mut m.objects[id];
    let mut pivot = *o.chain_pivot.get_or_insert(post_home);
    let previous = pivot;
    limit_distance(&mut pivot, &o.pos, o.var("segmentRadius") * 5.0);
    o.chain_pivot = Some(pivot);
    for &link in &links[1..] {
        let l = &mut m.objects[link];
        for axis in 0..3 { l.pos[axis] += pivot[axis] - previous[axis]; }
    }
    update_chain(id, m, Some(pivot));
}

pub fn chain_chomp_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].action == CHAIN_CHOMP_ACT_UNINITIALIZED {
        if m.objects[id].links.is_none() { spawn_chain(id, m); }
        if m.objects[id].distance_to_mario > 3000.0 { return; }
        let o = &mut m.objects[id];
        o.set_action(CHAIN_CHOMP_ACT_MOVE);
        o.visible = true;
        o.pos = o.home;
        update_chain(id, m, None);
        let o = &mut m.objects[id];
        o.set_var("subAction", SUB_ACT_TURN);
        o.set_var("timer", 0.0);
        return;
    }

    if m.objects[id].var("release") != 0.0 {
        let o = &mut m.objects[id];
        let j = (o.var("jawFrame") + 1.0) % 20.0;
        o.set_var("jawFrame", j);
        released_step(id, m);
        return;
    }

    let post_home = {
        let links = m.objects[id].links.as_ref().expect("chain links");
        m.objects[links[0]].home
    };
    let pivot = post_home;
    let object = &mut m.objects[id];
    let t = object.var("timer") + 1.0;
    object.set_var("timer", t);
    let mut hold_jaw = false;
    if object.var("subAction") == SUB_ACT_TURN {
        object.gravity = 4.0;
        let seg = approach_f32(object.var("segmentRadius"), CHAIN_MAX_DIST / 5.0, 4.0, 4.0);
        object.set_var("segmentRadius", seg);
        object.set_var("linkSpacing", seg);
        object.turn_toward_angle(object.angle_to_mario, if object.grounded { 0x400 as f64 } else { 400.0 });
        if !object.grounded {
            object.set_var("timer", 0.0);
        } else if (to_s16(object.angle_to_mario - object.move_yaw) as f64).abs() >= 0x800 as f64 {
            object.forward_vel = 10.0;
            object.vel[1] = 20.0;
        } else if object.var("timer") <= 30.0 {
            object.forward_vel = 0.0;
        } else if object.var("jawFrame") != 0.0 {
            let t = object.var("timer") - 1.0;
            object.set_var("timer", t);
        } else {
            hold_jaw = true;
            if object.var("timer") > 40.0 {
                object.set_var("subAction", SUB_ACT_LUNGE);
                object.set_var("segmentRadius", CHAIN_MAX_DIST_LUNGE / 5.0);
                object.forward_vel = 140.0;
                object.vel[1] = 20.0;
                object.gravity = 0.0;
                object.set_var("targetPitch", -(atan2s(object.forward_vel, object.vel[1]) as f64));
            }
        }
    } else {
        let fp = approach_f32(object.var("facePitch"), object.var("targetPitch"), 0x400 as f64, 0x400 as f64);
        object.set_var("facePitch", fp);
        if object.forward_vel != 0.0 {
            if object.var("restricted") != 0.0 {
                object.forward_vel = 0.0;
                object.vel[1] = 0.0;
                object.set_var("recoil", 30.0);
            }
            let distance = js_hypot3(object.pos[0] - pivot[0], object.pos[1] - pivot[1], object.pos[2] - pivot[2]);
            object.set_var("linkSpacing", (220.0f64).min(CHAIN_MAX_DIST_LUNGE - distance) / 220.0 * object.var("segmentRadius"));
            object.set_var("timer", 0.0);
        } else {
            object.turn_toward_angle(atan2s(object.pos[2] - pivot[2], object.pos[0] - pivot[0]) as f64, 0x1000 as f64);
            let recoil = object.var("recoil");
            if recoil > 0.0 {
                object.set_var("recoil", (0.0f64).max(recoil - 0.8));
            } else {
                object.set_var("subAction", SUB_ACT_TURN);
            }
            let recoil = object.var("recoil");
            object.set_var("linkSpacing", if object.var("timer") % 2.0 != 0.0 { -recoil } else { recoil });
        }
        hold_jaw = object.var("timer") < 30.0;
    }
    if !hold_jaw {
        let j = (object.var("jawFrame") + 1.0) % 20.0;
        object.set_var("jawFrame", j);
    }
    move_chomp(id, m, 0.0);
    let object = &mut m.objects[id];
    let seg5 = object.var("segmentRadius") * 5.0;
    let restricted = limit_distance(&mut object.pos, &pivot, seg5);
    object.set_var("restricted", if restricted { 1.0 } else { 0.0 });
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.set_var("subAction", SUB_ACT_LUNGE);
        object.set_var("segmentRadius", CHAIN_MAX_DIST_LUNGE / 5.0);
        object.forward_vel = 0.0;
        object.vel[1] = 300.0;
        object.gravity = 4.0;
        object.set_var("targetPitch", -0x3000 as f64);
    }
    object.interact_status = 0;
    object.face_yaw = object.move_yaw;
    update_chain(id, m, None);
}

pub fn wooden_post_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].var("offsetY") == 0.0 && m.objects[id].var("rewarded") == 0.0 {
        let object = &mut m.objects[id];
        if object.distance_to_mario > 400.0 {
            object.timer = 0.0;
            object.set_var("totalAngle", 0.0);
        } else {
            let total = object.var("totalAngle") + to_s16(object.angle_to_mario - object.var("prevAngle")) as f64;
            object.set_var("totalAngle", total);
            if total.abs() > 0x30000 as f64 && object.timer < 200.0 {
                let pos = object.pos;
                for _ in 0..5 { m.objects.spawn_coin(pos, true, 1.0); }
                m.objects[id].set_var("rewarded", 1.0);
            }
        }
        let object = &mut m.objects[id];
        object.set_var("prevAngle", object.angle_to_mario);
    }
    let on_post = m.action == ACT_GROUND_POUND_LAND
        && m.floor.as_ref().is_some_and(|f| f.object == Some(id.0))
        && (m.pos[1] - m.floor_height).abs() < 4.0;
    let object = &mut m.objects[id];
    if object.var("pounded") == 0.0 {
        if on_post {
            object.set_var("pounded", 1.0);
            object.set_var("speedY", -70.0);
        }
        return;
    }
    let speed = (0.0f64).min(object.var("speedY") + 25.0);
    object.set_var("speedY", speed);
    if speed == 0.0 {
        object.set_var("pounded", if on_post { 1.0 } else { 0.0 });
        return;
    }
    let offset = (-190.0f64).max(object.var("offsetY") + speed);
    object.set_var("offsetY", offset);
    object.pos[1] = object.home[1] + offset;
    if offset == -190.0 {
        if let Some(link) = object.link {
            if m.objects[link].behavior == ObjectBehaviorKind::ChainChomp {
                m.objects[link].set_var("release", 1.0);
                m.objects[id].link = None;
            }
        }
    }
}

pub fn chain_chomp_gate_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].link.is_none() {
        m.objects[id].link = m.objects.find_by_behavior(ObjectBehaviorKind::ChainChomp);
    }
    if let Some(link) = m.objects[id].link {
        if m.objects[link].var("hitGate") != 0.0 {
            m.objects.break_object(id);
            m.objects[id].deactivate(-1.0);
        }
    }
}

pub fn chain_part_behavior(id: ObjId, m: &mut MarioRuntime) {
    let alive = m.objects[id].link.is_some_and(|l| m.objects[l].active);
    if !alive { m.objects[id].deactivate(-1.0); }
}
