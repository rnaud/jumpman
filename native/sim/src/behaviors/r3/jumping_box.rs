//! Jumping Box (`bhvJumpingBox`, BBH, LLL, SSL, TTM): port of `src/game/behaviors/jumping_box.inc.c`.
//!
//! A small crate that hops about by itself (jump speed 15..20 every 30..90 frames on the ground). Mario can pick it up
//! (`INTERACT_GRABBABLE`); once thrown or dropped it bursts on the first wall, water or landing.
//!
//! Left out: the held model swap to the small box, the mist puff, the break-by-attack path (`INT_STATUS_STOP_RIDING`).
use crate::behaviors::g1::hitbox;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys::{self, MASK_IN_WATER, MASK_ON_GROUND, MOVE_HIT_WALL};
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

pub const MODEL: &str = "actors/breakable_box/breakable_box_geo.glb";

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 0.5;
    // sJumpingBoxHitbox: GRABBABLE, down offset 20, 5 loot coins, 150 x 250.
    o.hitbox = hitbox(INTERACT_GRABBABLE, 20.0, 0.0, 1.0, 5.0, 150.0, 250.0, 150.0, 250.0);
    o.set_var("realInteract", INTERACT_GRABBABLE as f64);
    phys::set_physics(o, 30.0, -400.0, -50.0, 1000.0, 1000.0, 600.0);
}

/// `bhv_jumping_box_loop`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let o = &mut m.objects[id];
        match o.held_state {
            HELD_HELD => return,
            HELD_THROWN => {
                // cur_obj_get_thrown_or_placed(40, 20, 1)
                o.held_state = HELD_FREE;
                o.forward_vel = 40.0;
                o.vel[1] = 20.0;
                o.action = 1;
            }
            HELD_DROPPED => {
                o.held_state = HELD_FREE;
                o.forward_vel = 0.0;
                o.action = 1;
            }
            _ => {}
        }
        phys::update_floor_and_walls(o, &world);
        phys::move_standard(o, &world, 78.0);
        let flags = phys::flags(o);
        match o.action {
            0 => {
                if o.action_state == 0.0 {
                    let wait = o.var("wait") - 1.0;
                    o.set_var("wait", wait);
                    if wait < 0.0 {
                        o.action_state = 1.0;
                    }
                    if o.timer > 0.0 {
                        o.vel[1] = random_float() * 5.0 + 15.0;
                        o.action_state = 1.0;
                    }
                } else if flags & MASK_ON_GROUND != 0 {
                    o.action_state = 0.0;
                    o.set_var("wait", (random_float() * 60.0 + 30.0).trunc());
                }
            }
            _ => {
                if flags & (MOVE_HIT_WALL | MASK_IN_WATER | phys::MOVE_LANDED) != 0 {
                    o.deactivate(-1.0);
                }
            }
        }
        o.interact_status = 0;
    });
}
