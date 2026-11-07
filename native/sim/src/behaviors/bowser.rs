//! Port of `src/sim/behaviors/bowser.ts` (Bowser and the Bowser mines).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::object_manager;
use crate::surfaces::SURFACE_BURNING;

const WALK: i32 = 0;
const THROWN: i32 = 1;
const RETURNING: i32 = 2;
const DEFEATED: i32 = 3;

pub fn init_bowser(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    let health = if object.param == 3.0 { 3.0 } else { 1.0 };
    object.set_hitbox(ObjectHitbox {
        interact_type: INTERACT_GRABBABLE, down_offset: 0.0, damage_or_coin_value: 2.0,
        health, num_loot_coins: 0.0, radius: 400.0, height: 400.0,
        hurtbox_radius: 0.0, hurtbox_height: 0.0,
    });
    object.set_var("health", object.hitbox.health);
}

pub fn at_bowser_tail(o: &SimObject, mario: &MarioRuntime) -> bool {
    // Tail anchor projected behind the root; radius and vertical offset from bhvBowserTailAnchor.
    let dx = mario.pos[0] - (o.pos[0] - 300.0 * sins(o.move_yaw));
    let dz = mario.pos[2] - (o.pos[2] - 300.0 * coss(o.move_yaw));
    dx * dx + dz * dz < 137.0 * 137.0 && mario.pos[1] < o.pos[1] + 100.0 && mario.pos[1] + 160.0 > o.pos[1] + 50.0
}

pub fn init_bowser_mine(o: &mut SimObject) {
    o.set_hitbox(ObjectHitbox {
        interact_type: 0, down_offset: 40.0, damage_or_coin_value: 0.0, health: 1.0, num_loot_coins: 0.0,
        radius: 40.0, height: 40.0, hurtbox_radius: 0.0, hurtbox_height: 0.0,
    });
}

pub fn bowser_mine_behavior(id: ObjId, m: &mut MarioRuntime) {
    if mario_overlaps_object(m, &m.objects[id]) {
        object_manager::explode(m, id, 150.0, 2.0);
        m.objects[id].deactivate(-1.0);
    }
}

fn rebound_to_arena(o: &mut SimObject) {
    o.move_yaw = atan2s(-o.pos[2], -o.pos[0]) as f64;
    o.vel[1] = 100.0;
    o.forward_vel = if o.param == 3.0 { 400.0 } else { 200.0 };
    o.set_action(RETURNING);
}

pub fn bowser_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].held_state == HELD_HELD {
        let o = &mut m.objects[id];
        let ticks = o.var("heldTicks") + 1.0;
        o.set_var("heldTicks", ticks);
        o.set_var("animation", if ticks < 36.0 { 3.0 } else { 2.0 });
        return;
    }
    let mface = m.face_angle[1];
    {
        let o = &mut m.objects[id];
        o.set_var("heldTicks", 0.0);
        if o.held_state != HELD_FREE {
            let spin = if o.held_state == HELD_THROWN { o.var("spin").abs() } else { 0.0 };
            let mut speed = spin / 3000.0 * 70.0;
            if speed > 90.0 { speed *= 2.5; }
            o.forward_vel = coss(-spin) * speed;
            o.vel[1] = -sins(-spin) * speed;
            o.set_held_state(HELD_FREE);
            o.set_action(THROWN);
        }
    }

    if m.objects[id].action == DEFEATED {
        let o = &mut m.objects[id];
        o.set_var("animation", 17.0);
        if o.distance_to_mario < 700.0 && (to_s16(mface - o.angle_to_mario) as f64).abs() > 0x6000 as f64 {
            let pos = o.pos;
            m.objects.bowser_defeated(pos);
            m.objects[id].deactivate(-1.0);
        }
        return;
    }
    if m.objects[id].action == WALK {
        let (timer, pos, face_yaw) = { let o = &m.objects[id]; (o.timer, o.pos, o.face_yaw) };
        if timer % 120.0 == 0.0 {
            let mut spec = ObjectSpec::new(ObjectBehaviorKind::BowserFlame, [pos[0], pos[1] + 220.0, pos[2]], face_yaw);
            spec.model = Some("actors/bowser_flame/bowser_flames_geo.glb".to_string());
            m.objects.spawn(&spec);
        }
        let o = &mut m.objects[id];
        let health = o.var("health");
        let turn = if o.param == 2.0 || health >= 3.0 { 0x400 as f64 } else if health == 2.0 { 0x300 as f64 } else { 0x200 as f64 };
        o.move_yaw = to_s16(o.move_yaw + clamp(to_s16(o.angle_to_mario - o.move_yaw) as f64, -turn, turn)) as f64;
        o.face_yaw = o.move_yaw;
        o.forward_vel = if o.distance_to_mario < 3000.0 { 3.5 } else { 0.0 };
        o.set_var("animation", if o.forward_vel != 0.0 { 13.0 } else { 12.0 });
    }
    if m.objects[id].action == RETURNING {
        let o = &mut m.objects[id];
        o.set_var("animation", if o.var("health") > 0.0 { 25.0 } else { 16.0 });
        o.forward_vel = o.forward_vel.min(js_hypot(o.pos[0], o.pos[2]));
    }
    if m.objects[id].action == THROWN { m.objects[id].set_var("animation", 2.0); }
    m.objects[id].step_physics(&m.world, None);
    if m.objects[id].action == THROWN {
        let mut distance = 800.0;
        let mut nearest: Option<ObjId> = None;
        let opos = m.objects[id].pos;
        for (i, mine) in m.objects.objects.iter().enumerate() {
            if !mine.active || mine.behavior != ObjectBehaviorKind::BowserMine { continue; }
            let d = dist_3d(&opos, &mine.pos);
            if d < distance { nearest = Some(ObjId(i as u32)); distance = d; }
        }
        if let Some(nearest) = nearest {
            object_manager::explode(m, nearest, 150.0, 2.0);
            m.objects[nearest].deactivate(-1.0);
            let o = &mut m.objects[id];
            let h = o.var("health") - 1.0;
            o.set_var("health", h);
            rebound_to_arena(o);
            return;
        }
        let o = &mut m.objects[id];
        if o.grounded { o.set_action(WALK); }
    } else if m.objects[id].action == RETURNING && m.objects[id].grounded {
        let o = &mut m.objects[id];
        o.forward_vel = 0.0;
        let next = if o.var("health") > 0.0 { WALK } else { DEFEATED };
        o.set_action(next);
    }
    let o = &m.objects[id];
    let floor_kind = if o.grounded { m.world.find_floor(o.pos[0], o.pos[1], o.pos[2]).surface.map(|s| s.kind) } else { None };
    if o.pos[1] < o.home[1] - 1000.0 || o.pos[0].abs().max(o.pos[2].abs()) > 8000.0 || floor_kind == Some(SURFACE_BURNING) {
        let o = &mut m.objects[id];
        o.pos = [0.0, o.home[1] + 2000.0, 0.0];
        o.forward_vel = 0.0;
        o.vel[1] = 0.0;
        o.set_action(RETURNING);
    }
}
