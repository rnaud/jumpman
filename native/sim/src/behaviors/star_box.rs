//! Port of `src/sim/behaviors/starBox.ts` (star box, wing box and wing cap).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

/// The star contents of bhvExclamationBox, including its eight-frame recoil.
pub fn init_star_box(o: &mut SimObject) {
    o.scale = 2.0;
    o.set_hitbox(ObjectHitbox {
        interact_type: INTERACT_BREAKABLE, down_offset: 10.0, radius: 80.0, height: 60.0,
        health: 1.0, damage_or_coin_value: 0.0, num_loot_coins: 0.0, hurtbox_radius: 80.0, hurtbox_height: 60.0,
    });
}

pub fn star_box_behavior(id: ObjId, m: &mut MarioRuntime) {
    {
        let o = &m.objects[id];
        if o.behavior == ObjectBehaviorKind::WingBox && o.var("unlocked") == 0.0 { return; }
    }
    if m.objects[id].action == 2 {
        let o = &mut m.objects[id];
        let t = o.action_timer;
        o.action_timer += 1.0;
        if t > 300.0 {
            o.visible = true;
            o.pos = o.home;
            o.graph_y_offset = 0.0;
            o.set_var("width", 1.0);
            o.set_var("height", 1.0);
            o.set_action(0);
            init_star_box(o);
        }
        return;
    }
    if m.objects[id].action == 0 {
        let mpos = m.pos;
        let maction = m.action;
        let o = &mut m.objects[id];
        let pound = maction == ACT_GROUND_POUND_LAND && (mpos[1] - o.home[1] - 104.0).abs() < 5.0
            && (mpos[0] - o.home[0]).abs() < 52.0 && (mpos[2] - o.home[2]).abs() < 52.0;
        if o.interact_status & INT_STATUS_WAS_ATTACKED == 0 && !pound { return; }
        o.interact_status = 0;
        o.hitbox.interact_type = 0;
        o.vel[1] = 30.0;
        o.set_action(1);
        return;
    }
    let o = &mut m.objects[id];
    let t = o.action_timer;
    o.action_timer += 1.0;
    o.vel[1] = (0.0f64).max(o.vel[1] - 8.0);
    o.pos[1] += o.vel[1];
    let wave = sins(0x4000 as f64 + t * 0x1000 as f64);
    o.set_var("width", (1.0 - wave) * 0.5 + 1.0);
    o.set_var("height", (1.0 + wave) * 0.3);
    o.graph_y_offset = (1.0 - wave) * 26.0;
    if t == 8.0 {
        m.objects.break_object(id);
        if m.objects[id].behavior == ObjectBehaviorKind::WingBox {
            let face = m.face_angle[1];
            let opos = m.objects[id].pos;
            let cap = match m.objects[id].link {
                Some(c) => c,
                None => {
                    let mut spec = ObjectSpec::new(ObjectBehaviorKind::WingCap, opos, face);
                    spec.model = Some("actors/mario_cap/marios_wing_cap_geo.glb".to_string());
                    m.objects.spawn(&spec)
                }
            };
            m.objects[id].link = Some(cap);
            let c = &mut m.objects[cap];
            c.reactivate();
            c.pos = opos;
            c.forward_vel = 3.0;
            c.move_yaw = face;
            c.vel[1] = 20.0;
            let o = &mut m.objects[id];
            o.visible = false;
            o.set_action(2);
        } else {
            m.objects.star_from_box(id);
            m.objects[id].deactivate(-1.0);
        }
    }
}

pub fn init_wing_cap(o: &mut SimObject) {
    o.gravity = 1.2;
    o.friction = 0.999;
    o.buoyancy = 0.9;
    o.set_hitbox(ObjectHitbox {
        interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0,
        radius: 80.0, height: 80.0, hurtbox_radius: 90.0, hurtbox_height: 90.0,
    });
}

pub fn wing_cap_behavior(id: ObjId, m: &mut MarioRuntime) {
    let o = &mut m.objects[id];
    o.face_yaw += o.forward_vel * 128.0;
    o.step_physics(&m.world, None);
    if o.grounded { o.vel[1] = 0.0; }
    if o.timer > 20.0 && mario_overlaps_object(m, &m.objects[id]) {
        m.grant_wing_cap(1800.0);
        m.objects[id].deactivate(-1.0);
    } else if m.objects[id].timer > 320.0 {
        m.objects[id].deactivate(-1.0);
    } else if m.objects[id].timer > 300.0 {
        let o = &mut m.objects[id];
        o.visible = o.timer % 2.0 == 0.0;
    }
}
