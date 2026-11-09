//! Moneybag (`macro_moneybag` -> `bhvMoneybagHidden`, Snowman's Land): port of `src/game/behaviors/moneybag.inc.c`.
//!
//! A yellow coin that is really a Moneybag in hiding: when Mario comes within 400 it fades into a hopping Moneybag
//! (jumps 40 up / 20 forward, sometimes strolls) that wanders around its home, returns when Mario is further than 800
//! from home and turns back into the coin. It bites for 2 when touched and bursts into 5 coins when stomped.
//!
//! Left out: the opacity fade (the sim keeps the `opacity` var), sounds, the mist puff.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::{become_intangible, become_tangible, INTERACT_BOUNCE_TOP};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::behaviors::r2a::helpers::hitbox;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

pub const BAG_MODEL: &str = "actors/moneybag/moneybag_geo.glb";
pub const COIN_MODEL: &str = "actors/coin/yellow_coin_geo.glb";
/// `moneybag_seg6_anims_06005E5C`.
pub const ANIMS: [AnimSpec; 5] = [spec(0, 0, 0x1E, false), spec(0, 0, 0x0A, false), spec(0, 0, 0x04, false), spec(0, 0, 0x03, false), spec(0, 0, 0x08, false)];
pub const ANIM_NAMES: [&str; 5] = ["moneybag_seg6_anim_06005AD8", "moneybag_seg6_anim_06005BEC", "moneybag_seg6_anim_06005C98", "moneybag_seg6_anim_06005D3C", "moneybag_seg6_anim_06005E44"];

const BAG_HITBOX: ObjectHitbox = hitbox(INTERACT_BOUNCE_TOP, 0.0, 2.0, 1.0, 0.0, 120.0, 60.0, 100.0, 50.0);
const HIDDEN_HITBOX: ObjectHitbox = hitbox(crate::object::INTERACT_DAMAGE, 0.0, 2.0, 1.0, 0.0, 110.0, 100.0, 110.0, 100.0);

const APPEAR: i32 = 0;
const MOVE_AROUND: i32 = 2;
const RETURN_HOME: i32 = 3;
const DISAPPEAR: i32 = 4;
const DEATH: i32 = 5;

const LANDING: f64 = 0.0;
const PREPARE: f64 = 1.0;
const JUMP: f64 = 2.0;
const WALK_AROUND: f64 = 4.0;
const WALK_HOME: f64 = 5.0;

pub fn init_hidden(o: &mut SimObject) {
    o.visible = true;
    o.billboard = true;
    o.hitbox = HIDDEN_HITBOX;
    o.graph_y_offset = 27.0;
}

/// `bhv_moneybag_hidden_loop`.
pub fn hidden(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let pos = m.objects[id].pos;
        if m.objects[id].action == 0 && dist_3d(&pos, &m.pos) < 400.0 {
            let bag = spawn_child(m, id, K::Moneybag, Some(BAG_MODEL), 0.0);
            m.objects[bag].home = pos;
            m.objects[id].action = 1;
        }
        m.objects[id].interact_status = 0;
    });
}

pub fn init_bag(o: &mut SimObject) {
    o.visible = true;
    o.gravity = 3.0;
    o.friction = 1.0;
    o.buoyancy = 2.0;
    o.set_var("opacity", 0.0);
    become_intangible(o);
    anim::init(o, 0, ANIMS[0]);
    phys::set_physics(o, 30.0, -300.0, 0.0, 0.0, 1000.0, 200.0);
}

fn anim_i(o: &mut SimObject, i: usize) {
    anim::init(o, i, ANIMS[i]);
}

fn jump(o: &mut SimObject, landed: bool) {
    let frame = o.var("animFrame") as i32;
    match o.var("jumpState") {
        s if s == PREPARE => {
            anim_i(o, 1);
            if frame == 5 {
                o.forward_vel = 20.0;
                o.vel[1] = 40.0;
            }
            if anim::near_end(o) {
                o.set_var("jumpState", JUMP);
            }
        }
        s if s == JUMP => {
            anim_i(o, 2);
            if landed {
                o.forward_vel = 0.0;
                o.vel[1] = 0.0;
                o.set_var("jumpState", LANDING);
            }
        }
        s if s == WALK_AROUND => {
            anim_i(o, 4);
            o.forward_vel = 10.0;
            if o.timer > 60.0 {
                o.set_var("jumpState", LANDING);
                o.forward_vel = 0.0;
                o.set_var("animFrame", 0.0);
            }
        }
        s if s == WALK_HOME => {
            anim_i(o, 4);
            o.forward_vel = 5.0;
        }
        _ => {}
    }
}

/// `moneybag_check_mario_collision`.
fn check_collision(o: &mut SimObject) {
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        if o.interact_status & INT_STATUS_ATTACKED_MARIO != 0 {
            o.move_yaw = to_s16(o.angle_to_mario + 0x8000 as f64) as f64;
            o.vel[1] = 30.0;
        }
        if o.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
            o.action = DEATH;
        }
        o.interact_status = 0;
    }
}

/// `bhv_moneybag_loop`.
pub fn bag(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        let mario = m.pos;
        let near_home = |o: &SimObject, d: f64| dist_3d(&o.home, &mario) < d;
        let mut spawn_hidden = None;
        let mut kill_parent = false;
        let mut coins = None;
        {
            let o = &mut m.objects[id];
            match o.action {
                APPEAR | 1 => {
                    let op = o.var("opacity") + 12.0;
                    o.set_var("opacity", op);
                    if op > 255.0 {
                        o.set_var("opacity", 255.0);
                        kill_parent = true;
                        o.action = MOVE_AROUND;
                    }
                }
                MOVE_AROUND => {
                    // obj_return_and_displace_home(o, home, 200)
                    if (random_float() * 50.0) as i64 == 0 {
                        o.set_var("wanderX", 400.0 * random_float() - 200.0 + o.home[0]);
                        o.set_var("wanderZ", 400.0 * random_float() - 200.0 + o.home[2]);
                    }
                    let (tx, tz) = if o.has_var("wanderX") { (o.var("wanderX"), o.var("wanderZ")) } else { (o.home[0], o.home[2]) };
                    let angle = atan2s(tz - o.pos[2], tx - o.pos[0]) as f64;
                    o.move_yaw = approach_s16_symmetric(o.move_yaw, angle, 320.0);
                    phys::update_floor_and_walls(o, &world);
                    phys::move_standard(o, &world, -78.0);
                    let landed = phys::flags(o) & phys::MASK_ON_GROUND != 0;
                    if landed && o.var("jumpState") == LANDING {
                        if (random_float() * 6.0) as i64 == 1 {
                            o.set_var("jumpState", WALK_AROUND);
                            o.timer = 0.0;
                        } else {
                            o.set_var("jumpState", PREPARE);
                        }
                    }
                    jump(o, phys::flags(o) & phys::MASK_ON_GROUND != 0);
                    check_collision(o);
                    if !near_home(o, 800.0) && landed {
                        o.action = RETURN_HOME;
                    }
                    if o.timer > 30.0 {
                        become_tangible(o);
                        o.hitbox = BAG_HITBOX;
                    }
                }
                RETURN_HOME => {
                    let angle = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
                    o.move_yaw = approach_s16_symmetric(o.move_yaw, angle, 0x800 as f64);
                    phys::update_floor_and_walls(o, &world);
                    phys::move_standard(o, &world, -78.0);
                    let landed = phys::flags(o) & phys::MASK_ON_GROUND != 0;
                    if landed && o.var("jumpState") == LANDING {
                        o.set_var("jumpState", WALK_HOME);
                    }
                    jump(o, phys::flags(o) & phys::MASK_ON_GROUND != 0);
                    check_collision(o);
                    if dist_3d(&o.pos, &o.home) < 100.0 {
                        spawn_hidden = Some(o.home);
                        anim_i(o, 0);
                        o.action = DISAPPEAR;
                        o.set_var("jumpState", LANDING);
                    }
                    if near_home(o, 800.0) {
                        o.action = MOVE_AROUND;
                        o.set_var("jumpState", LANDING);
                    }
                }
                DISAPPEAR => {
                    let op = o.var("opacity") - 6.0;
                    o.set_var("opacity", op.max(0.0));
                    if op < 0.0 {
                        o.deactivate(-1.0);
                    }
                }
                _ => {
                    if o.timer == 1.0 {
                        coins = Some(o.pos);
                        o.deactivate(-1.0);
                    }
                }
            }
            if o.active {
                anim::advance(o);
            }
        }
        if kill_parent {
            if let Some(p) = m.objects[id].link {
                m.objects[p].deactivate(-1.0);
            }
        }
        if let Some(home) = spawn_hidden {
            let h = spawn_child(m, id, K::MoneybagHidden, Some(COIN_MODEL), 0.0);
            m.objects[h].pos = home;
            m.objects[h].home = home;
        }
        if let Some(pos) = coins {
            for _ in 0..5 {
                m.objects.spawn_coin(pos, false, 1.0);
            }
        }
    });
}
