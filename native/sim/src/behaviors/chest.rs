//! Port of `src/sim/behaviors/chest.ts`.
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub fn init_chest(o: &mut SimObject) {
    o.gravity = 0.0;
    o.set_var("opened", 0.0);
    o.set_var("lidPitch", 0.0);
    o.visible = true;
}

/// bhv_treasure_chest_bottom_loop and the hinged lid in treasure_chest.inc.c.
pub fn chest_behavior(id: ObjId, m: &mut MarioRuntime) {
    let facing = (to_s16(m.objects[id].move_yaw - m.face_angle[1] - 0x8000 as f64) as f64).abs() < 0x3000 as f64;
    let (action, dist, param, opos) = { let o = &m.objects[id]; (o.action, o.distance_to_mario, o.param, o.pos) };
    let (locked, next) = (m.objects.chest_puzzle.locked, m.objects.chest_puzzle.next);
    if action == 0 && !locked && facing && dist < 150.0 {
        if next as f64 == param {
            m.objects.chest_puzzle.next += 1;
            m.objects[id].set_action(1);
            m.objects.chest_opened(param);
        } else {
            m.objects.chest_puzzle.next = 1;
            m.objects.chest_puzzle.locked = true;
            m.objects[id].set_action(2);
            m.take_damage(1.0, Some(opos));
            m.objects.chest_opened(0.0);
        }
    } else if action == 1 && locked {
        m.objects[id].set_action(0);
    } else if action == 2 && dist >= 500.0 {
        m.objects.chest_puzzle.locked = false;
        m.objects[id].set_action(0);
    }
    let o = &mut m.objects[id];
    o.set_var("opened", if o.action == 1 { 1.0 } else { 0.0 });
    let lid = o.var("lidPitch");
    let lid = if o.action == 1 { (-0x4000 as f64).max(lid - 0x200 as f64) } else { (0.0f64).min(lid + 0x800 as f64) };
    o.set_var("lidPitch", lid);

    let opos = o.pos;
    let dx = m.pos[0] - opos[0];
    let dz = m.pos[2] - opos[2];
    let distance = js_hypot(dx, dz);
    if (m.pos[1] - opos[1]).abs() < 150.0 && distance < 150.0 {
        m.pos[0] += (150.0 - distance) / 150.0 * dx;
        m.pos[2] += (150.0 - distance) / 150.0 * dz;
    }
}
