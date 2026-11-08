//! Thwomp (`bhvThwomp`, `bhvThwomp2`), a Whomp's Fortress slab that rises, hangs and slams down.
//! Port of `src/game/behaviors/thwomp.inc.c` (the Grindel/Thwomp action table; the Thwomp has
//! no hitbox, it hurts by squashing Mario against the floor with its dynamic collision).
use super::*;

pub const ACT_RAISE: i32 = 0;
pub const ACT_IDLE_AT_TOP: i32 = 1;
pub const ACT_LOWER: i32 = 2;
pub const ACT_LAND: i32 = 3;
pub const ACT_IDLE_AT_BOTTOM: i32 = 4;

/// `SCALE(0, 140)` in bhvThwomp.
pub const THWOMP_SCALE: f64 = 1.4;
/// Half extents of `thwomp_seg5_collision_0500B92C` (x/z +-157 at the base) times the scale.
pub const THWOMP_HALF_WIDTH: f64 = 157.0 * THWOMP_SCALE;
/// Height of the collision box's underside above the object origin (vertex y = 3, scaled).
pub const THWOMP_UNDERSIDE: f64 = 3.0 * THWOMP_SCALE;

pub fn init_thwomp(o: &mut SimObject) {
    o.scale = THWOMP_SCALE;
    o.visible = true;
    o.hitbox = ObjectHitbox { interact_type: 0, ..Default::default() };
    o.action = ACT_RAISE;
    o.prev_action = ACT_RAISE;
}

/// bhv_grindel_thwomp_loop (+ the bhvThwomp script prologue DROP_TO_FLOOR / ADD_FLOAT(oPosY, 1) /
/// SET_HOME, which the sim runs on the first tick because it needs the collision world).
pub fn thwomp_behavior(id: ObjId, m: &mut MarioRuntime) {
    if !m.objects[id].has_var("homed") {
        let pos = m.objects[id].pos;
        let floor = m.world.find_floor(pos[0], pos[1] + 100.0, pos[2]);
        let o = &mut m.objects[id];
        if floor.surface.is_some() { o.pos[1] = floor.height; }
        o.pos[1] += 1.0;
        o.home = o.pos;
        o.set_var("homed", 1.0);
    }
    let dist = m.objects[id].distance_to_mario;
    let pos = m.objects[id].pos;
    let o = &mut m.objects[id];
    match o.action {
        // grindel_thwomp_act_raise
        ACT_RAISE => {
            if o.action_timer > o.param + 40.0 {
                o.action = ACT_IDLE_AT_TOP;
                o.pos[1] += 5.0;
            } else {
                o.pos[1] += 10.0;
            }
        }
        // grindel_thwomp_act_idle_at_top
        ACT_IDLE_AT_TOP => {
            if o.action_timer == 0.0 { o.set_var("randomTimer", random() * 30.0 + 10.0); }
            if o.action_timer > o.var("randomTimer") { o.action = ACT_LOWER; }
        }
        // grindel_thwomp_act_lower
        ACT_LOWER => {
            o.vel[1] += -4.0;
            o.pos[1] += o.vel[1];
            if o.pos[1] < o.home[1] {
                o.pos[1] = o.home[1];
                o.vel[1] = 0.0;
                o.action = ACT_LAND;
            }
        }
        // grindel_thwomp_act_land
        ACT_LAND => {
            if o.action_timer == 0.0 && dist < 1500.0 {
                // cur_obj_shake_screen(SHAKE_POS_SMALL); cur_obj_play_sound_2(SOUND_OBJ_THWOMP)
                m.objects.sound(pos, "OBJ_THWOMP");
            }
            let o = &mut m.objects[id];
            if o.action_timer >= 10.0 { o.action = ACT_IDLE_AT_BOTTOM; }
        }
        // grindel_thwomp_act_idle_at_bottom
        ACT_IDLE_AT_BOTTOM => {
            if o.action_timer == 0.0 { o.set_var("randomTimer", random() * 10.0 + 20.0); }
            if o.action_timer > o.var("randomTimer") { o.action = ACT_RAISE; }
        }
        _ => {}
    }
    crush_mario(id, m);
    end_frame(&mut m.objects[id]);
}

/// Squash Mario when the slab comes down on him (`check_squish` in mario.c: ceiling-floor gap
/// under 160 while the dynamic ceiling moves down). Approximated here with the slab's footprint:
/// the sim has no squish action, so Mario takes one wedge of damage and is knocked back.
fn crush_mario(id: ObjId, m: &mut MarioRuntime) {
    let o = &m.objects[id];
    if o.action != ACT_LOWER && o.action != ACT_LAND { return; }
    let dx = (m.pos[0] - o.pos[0]).abs();
    let dz = (m.pos[2] - o.pos[2]).abs();
    let underside = o.pos[1] + THWOMP_UNDERSIDE;
    let squashed = dx < THWOMP_HALF_WIDTH - 20.0 && dz < THWOMP_HALF_WIDTH - 20.0
        && m.pos[1] < underside && m.pos[1] + 160.0 > underside;
    if squashed && m.floor_height + 1.0 >= m.pos[1] - 1.0 && m.pos[1] - m.floor_height < 5.0 {
        let p = o.pos;
        m.take_damage(1.0, Some(p));
    }
}
