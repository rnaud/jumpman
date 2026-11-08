//! BBH act 2, "Ride Big Boo's Merry-Go-Round": `bhvMerryGoRoundBooManager`, `bhvMerryGoRoundBoo` and
//! `bhvMerryGoRoundBigBoo`. Port of `src/game/behaviors/boo.inc.c` (`bhv_boo_loop`, `bhv_big_boo_loop`,
//! `bhv_merry_go_round_boo_manager_loop` and the helpers they call), restricted to the merry-go-round
//! variants: the Boos are only active while Mario is on the merry-go-round (`gMarioOnMerryGoRound`, here:
//! his floor has the surface type `SURFACE_MGR_MUSIC` 0x1A), so the per-room checks are not needed.
//!
//! The manager lets two Boos out at a time (five in total); once five are dead it releases the Big Boo,
//! which dies after three hits and spawns the star at (-1600, -2100, 205). The Ghost Hunt and Balcony
//! Boos keep the older simplified ports (`behaviors/boo.rs`, `big_boo.rs`).
//!
//! Left out: opacity (the fade is shown through the scale only), sounds, the merry-go-round stopping when
//! the Big Boo dies (`oMerryGoRoundStopped`; the platform is a moving platform of the app).
use super::helpers::*;
use crate::behaviors::g1::INTERACT_BOUNCE_TOP;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::ACT_FLAG_AIR;
use ObjectBehaviorKind as K;

/// `SURFACE_MGR_MUSIC`: every floor of the merry-go-round enclosure.
pub const SURFACE_MGR_MUSIC: i32 = 0x1A;
const BOO_MODEL: &str = "actors/boo/boo_geo.glb";

/// `sBooHitRotations`.
pub const HIT_ROTATIONS: [f64; 31] = [
    6047.0, 5664.0, 5292.0, 4934.0, 4587.0, 4254.0, 3933.0, 3624.0, 3329.0, 3046.0, 2775.0, 2517.0, 2271.0, 2039.0, 1818.0, 1611.0, 1416.0,
    1233.0, 1063.0, 906.0, 761.0, 629.0, 509.0, 402.0, 308.0, 226.0, 157.0, 100.0, 56.0, 25.0, 4.0,
];

/// The hitbox of `bhvGhostHuntBoo` (damage 2) and `sBooGivingStarHitbox` (the Big Boo, damage 3, health 3).
pub const BOO_HITBOX: ObjectHitbox = hitbox(0, 0.0, 2.0, 0.0, 0.0, 140.0, 80.0, 40.0, 60.0);
pub const BIG_BOO_HITBOX: ObjectHitbox = hitbox(0, 0.0, 3.0, 3.0, 0.0, 140.0, 80.0, 40.0, 60.0);

/// `gMarioOnMerryGoRound`.
pub fn mario_on_merry_go_round(m: &MarioRuntime) -> bool {
    let f = m.world.find_floor(m.pos[0], m.pos[1], m.pos[2]);
    f.surface.as_ref().is_some_and(|s| s.kind == SURFACE_MGR_MUSIC)
}

pub fn init_manager(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_merry_go_round_boo_manager_loop`.
pub fn manager(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        match m.objects[id].action {
            0 => {
                if m.objects[id].distance_to_mario < 1000.0 {
                    let (killed, spawned) = (m.objects[id].var("killed"), m.objects[id].var("spawned"));
                    if killed < 5.0 {
                        if spawned != 5.0 && spawned - killed < 2.0 {
                            spawn_child(m, id, K::MerryGoRoundBoo, Some(BOO_MODEL), 2.0);
                            m.objects[id].set_var("spawned", spawned + 1.0);
                        }
                        m.objects[id].action += 1;
                    }
                    if killed >= 5.0 {
                        let big = spawn_child(m, id, K::MerryGoRoundBigBoo, Some(BOO_MODEL), 0.0);
                        m.objects[big].param = m.objects[id].param;
                        m.objects[id].action = 2;
                    }
                }
            }
            1 => {
                if m.objects[id].timer > 60.0 {
                    m.objects[id].action = 0;
                }
            }
            _ => {}
        }
    });
}

fn init_common(o: &mut SimObject, hb: ObjectHitbox, base_scale: f64) {
    o.visible = true;
    o.hitbox = hb;
    o.set_var("hbInit", 1.0);
    o.set_var("wantType", 0.0);
    o.set_var("opacity", 255.0);
    o.set_var("targetOpacity", 255.0);
    o.set_var("baseScale", base_scale);
    phys::set_physics(o, 30.0, 0.0, -50.0, 1000.0, 1000.0, 200.0);
}

pub fn init_boo(o: &mut SimObject) {
    init_common(o, BOO_HITBOX, 1.0);
    o.graph_y_offset = 30.0;
}

pub fn init_big_boo(o: &mut SimObject) {
    init_common(o, BIG_BOO_HITBOX, 3.0);
    o.set_var("minionsKilled", 10.0);
}

fn boo_stop(o: &mut SimObject) {
    o.forward_vel = 0.0;
    o.vel[1] = 0.0;
    o.set_var("grav", 0.0);
}

fn tangible(o: &SimObject) -> bool {
    o.var("intang") == 0.0
}

/// `boo_approach_target_opacity_and_update_scale`.
fn approach_opacity_and_scale(o: &mut SimObject) {
    let (mut op, target) = (o.var("opacity"), o.var("targetOpacity"));
    if target != op {
        if target > op {
            op += 20.0;
            if target < op {
                op = target;
            }
        } else {
            op -= 20.0;
            if target > op {
                op = target;
            }
        }
    }
    o.set_var("opacity", op);
    o.scale = (op / 255.0 * 0.4 + 0.6) * o.var("baseScale");
}

/// `boo_oscillate(FALSE)`.
fn oscillate(o: &mut SimObject) {
    let t = o.var("osc");
    o.set_var("facePitch", (sins(t) * 0x400 as f64).trunc());
    if o.var("opacity") == 255.0 {
        o.set_var("grav", sins(t) * o.var("baseScale"));
        o.set_var("osc", t + 0x400 as f64);
    }
}

/// `boo_vanish_or_appear`: true when it is visible enough to chase.
fn vanish_or_appear(o: &mut SimObject, mario_face: f64) -> bool {
    let rel_to_mario = abs_angle_diff(o.angle_to_mario, o.move_yaw);
    let rel_face = abs_angle_diff(o.move_yaw, mario_face);
    let mut done = false;
    o.vel[1] = 0.0;
    if rel_to_mario > 0x1568 as f64 || rel_face < 0x6B58 as f64 {
        if o.var("opacity") == 40.0 {
            o.set_var("targetOpacity", 255.0);
        }
        if o.var("opacity") > 180.0 {
            done = true;
        }
    } else if o.var("opacity") == 255.0 {
        o.set_var("targetOpacity", 40.0);
    }
    done
}

/// `increment_velocity_toward_range`.
fn increment_velocity_toward_range(value: f64, center: f64, zero_threshold: f64, increment: f64) -> f64 {
    let relative = value - center;
    if relative > 0.0 {
        if relative < zero_threshold { 0.0 } else { -increment }
    } else if relative > -zero_threshold {
        0.0
    } else {
        increment
    }
}

/// `boo_chase_mario`.
fn chase_mario(m: &mut MarioRuntime, id: ObjId, a0: f64, turn: f64, mult: f64) {
    let (mario, mario_vel, mario_face, mario_air) = (m.pos, m.forward_vel, m.face_angle[1], m.action & ACT_FLAG_AIR != 0);
    let o = &mut m.objects[id];
    if vanish_or_appear(o, mario_face) {
        o.set_var("wantType", INTERACT_BOUNCE_TOP as f64);
        let (dx, dz) = (o.home[0] - mario[0], o.home[2] - mario[2]);
        let target = if (dx * dx + dz * dz).sqrt() > 1500.0 { angle_to_home(o) } else { o.angle_to_mario };
        rotate_yaw_toward(o, target, turn);
        o.vel[1] = 0.0;
        if !mario_air {
            let dy = o.pos[1] - mario[1];
            if a0 < dy && dy < 500.0 {
                o.vel[1] = increment_velocity_toward_range(o.pos[1], mario[1] + 50.0, 10.0, 2.0);
            }
        }
        // cur_obj_set_vel_from_mario_vel(10 - oBooNegatedAggressiveness, mult)
        let base = (10.0 - o.var("negAggr")) * mult;
        o.forward_vel = if mario_vel < base { base } else { mario_vel * mult };
        if o.forward_vel != 0.0 {
            oscillate(o);
        }
    } else {
        o.set_var("wantType", 0.0);
        boo_stop(o);
    }
}

/// `boo_get_attack_status`: 0 not attacked, 1 attacked, -1 bounced on.
fn attack_status(o: &mut SimObject) -> i32 {
    let mut status = 0;
    if o.interact_status & INT_STATUS_INTERACTED != 0 {
        if o.interact_status & INT_STATUS_WAS_ATTACKED != 0 && (o.interact_status & 0xFF) as i32 != ATTACK_FROM_ABOVE {
            o.set_var("intang", 1.0);
            status = 1;
        } else {
            status = -1;
        }
        o.interact_status = 0;
    }
    status
}

fn set_move_yaw_for_hit(o: &mut SimObject, hurt: bool, mario_yaw: f64) {
    o.set_var("intang", 1.0);
    o.set_var("noFaceYaw", 1.0);
    o.set_var("yawBeforeHit", o.move_yaw);
    if hurt {
        o.set_var("yawDuringHit", mario_yaw);
    } else if coss(to_s16(o.move_yaw) as f64 - to_s16(o.angle_to_mario) as f64) < 0.0 {
        o.set_var("yawDuringHit", o.move_yaw);
    } else {
        o.set_var("yawDuringHit", to_s16(o.move_yaw + 0x8000 as f64) as f64);
    }
}

fn move_during_hit(o: &mut SimObject, roll: bool, fvel: f64) {
    let osc = o.timer * 0x800 as f64 + 0x800 as f64;
    o.forward_vel = fvel;
    o.vel[1] = coss(osc);
    o.move_yaw = o.var("yawDuringHit");
    if roll {
        let r = HIT_ROTATIONS[(o.timer as usize).min(30)];
        o.face_yaw = to_s16(o.face_yaw + r) as f64;
        o.set_var("faceRoll", to_s16(o.var("faceRoll") + r) as f64);
    }
}

fn reset_after_hit(o: &mut SimObject) {
    o.move_yaw = o.var("yawBeforeHit");
    o.set_var("noFaceYaw", 0.0);
    o.interact_status = 0;
}

/// `boo_update_after_bounced_on`.
fn update_after_bounced_on(o: &mut SimObject, mario_yaw: f64, a0: f64) -> bool {
    boo_stop(o);
    if o.timer == 0.0 {
        set_move_yaw_for_hit(o, false, mario_yaw);
    }
    if o.timer < 32.0 {
        let v = HIT_ROTATIONS[(o.timer as usize).min(30)] / 5000.0 * a0;
        move_during_hit(o, false, v);
        false
    } else {
        o.set_var("intang", 0.0);
        reset_after_hit(o);
        o.action = 1;
        true
    }
}

/// `big_boo_update_during_nonlethal_hit`.
fn update_during_nonlethal_hit(o: &mut SimObject, mario_yaw: f64, a0: f64) -> bool {
    boo_stop(o);
    if o.timer == 0.0 {
        set_move_yaw_for_hit(o, true, mario_yaw);
    }
    if o.timer < 32.0 {
        let v = HIT_ROTATIONS[(o.timer as usize).min(30)] / 5000.0 * a0;
        move_during_hit(o, true, v);
        false
    } else if o.timer < 48.0 {
        // big_boo_shake_after_hit
        let osc = o.timer * 0x2000 as f64 - 0x3E000 as f64;
        o.face_yaw = to_s16(o.face_yaw + (coss(osc) * 0x400 as f64).trunc()) as f64;
        false
    } else {
        o.set_var("intang", 0.0);
        reset_after_hit(o);
        o.action = 1;
        true
    }
}

/// `boo_update_during_death`: true once the death is complete.
fn update_during_death(o: &mut SimObject, mario_yaw: f64) -> bool {
    if o.timer == 0.0 {
        o.forward_vel = 40.0;
        o.move_yaw = mario_yaw;
        o.set_var("noFaceYaw", 1.0);
    } else {
        if o.timer == 5.0 {
            o.set_var("targetOpacity", 0.0);
        }
        if o.timer > 30.0 || phys::flags(o) & phys::MOVE_HIT_WALL != 0 {
            return true;
        }
    }
    o.vel[1] = 5.0;
    o.set_var("faceRoll", to_s16(o.var("faceRoll") + 0x800 as f64) as f64);
    o.face_yaw = to_s16(o.face_yaw + 0x800 as f64) as f64;
    false
}

fn apply_interact_type(o: &mut SimObject) {
    o.hitbox.interact_type = if tangible(o) { o.var("wantType") as u32 } else { 0 };
}

/// `bhv_boo_loop` for `bhvMerryGoRoundBoo`.
pub fn boo(id: ObjId, m: &mut MarioRuntime) {
    let on = mario_on_merry_go_round(m);
    run(m, id, 0, |m, id| {
        let world = m.world.clone();
        let mario_yaw = m.face_angle[1];
        if first_frame(&mut m.objects[id]) {
            let yaw = m.objects[id].move_yaw; // bhv_boo_init
            m.objects[id].set_var("initialYaw", yaw);
        }
        phys::update_floor_and_walls(&mut m.objects[id], &world);
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                o.set_pos_home();
                o.move_yaw = o.var("initialYaw");
                boo_stop(o);
                o.set_var("baseScale", 1.0);
                o.set_var("targetOpacity", 255.0);
                if on {
                    o.action = 5;
                }
            }
            5 => {
                if o.timer < 30.0 {
                    o.vel[1] = 0.0;
                    o.forward_vel = 13.0;
                    oscillate(o);
                    o.set_var("wall", 0.0);
                } else {
                    o.action = 1;
                    o.set_var("wall", 30.0);
                }
            }
            1 => {
                if o.timer == 0.0 {
                    o.set_var("negAggr", -random_float() * 5.0);
                    o.set_var("turnSpeed", (random_float() * 128.0).trunc());
                }
                let turn = o.var("turnSpeed") + 0x180 as f64;
                let status;
                {
                    let _ = o;
                    chase_mario(m, id, -100.0, turn, 0.5);
                    status = attack_status(&mut m.objects[id]);
                }
                let o = &mut m.objects[id];
                if !on {
                    o.action = 0;
                }
                if status == -1 {
                    o.action = 2;
                }
                if status == 1 {
                    o.action = 3;
                }
            }
            2 => {
                if update_after_bounced_on(o, mario_yaw, 20.0) {
                    o.action = 1;
                }
            }
            3 => {
                if update_during_death(o, mario_yaw) {
                    mark_for_deletion(o);
                }
            }
            _ => {}
        }
        let o = &mut m.objects[id];
        phys::move_standard(o, &world, 78.0);
        approach_opacity_and_scale(o);
        o.interact_status = 0;
        apply_interact_type(o);
        let deleted = !o.active;
        let dead_flag = o.action == 3;
        let _ = dead_flag;
        if deleted {
            if let Some(parent) = m.objects[id].link {
                let n = m.objects[parent].var("killed") + 1.0;
                m.objects[parent].set_var("killed", n);
            }
        }
        if m.objects[id].var("noFaceYaw") == 0.0 {
            m.objects[id].face_yaw = m.objects[id].move_yaw;
        }
    });
}

/// `bhv_big_boo_loop` for `bhvMerryGoRoundBigBoo`.
pub fn big_boo(id: ObjId, m: &mut MarioRuntime) {
    let on = mario_on_merry_go_round(m);
    run(m, id, 0, |m, id| {
        let world = m.world.clone();
        let mario_yaw = m.face_angle[1];
        if first_frame(&mut m.objects[id]) {
            let yaw = m.objects[id].move_yaw;
            m.objects[id].set_var("initialYaw", yaw);
        }
        let mut star = false;
        {
            let o = &mut m.objects[id];
            o.graph_y_offset = o.var("baseScale") * 60.0;
        }
        phys::update_floor_and_walls(&mut m.objects[id], &world);
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                if on && o.var("minionsKilled") >= 5.0 {
                    o.action = 1;
                    o.pos = o.home;
                    o.move_yaw = o.var("initialYaw");
                    o.visible = true;
                    o.set_var("targetOpacity", 255.0);
                    o.set_var("baseScale", 3.0);
                    o.hitbox.health = 3.0;
                    o.scale = 3.0;
                    o.set_var("intang", 0.0);
                } else {
                    o.visible = false;
                    o.set_var("intang", 1.0);
                    boo_stop(o);
                }
            }
            1 => {
                let (turn, mult) = match o.hitbox.health as i32 {
                    3 => (0x180 as f64, 0.5),
                    2 => (0x240 as f64, 0.6),
                    _ => (0x300 as f64, 0.8),
                };
                let status;
                {
                    let _ = o;
                    chase_mario(m, id, -100.0, turn, mult);
                    status = attack_status(&mut m.objects[id]);
                }
                let o = &mut m.objects[id];
                if !on {
                    o.action = 0;
                }
                if status == -1 {
                    o.action = 2;
                }
                if status == 1 {
                    o.action = 3;
                }
            }
            2 => {
                if update_after_bounced_on(o, mario_yaw, 20.0) {
                    o.action = 1;
                }
            }
            3 => {
                if o.timer == 0.0 {
                    o.hitbox.health -= 1.0;
                }
                if o.hitbox.health == 0.0 {
                    if update_during_death(o, mario_yaw) {
                        // cur_obj_disable, big_boo_spawn_merry_go_round_star
                        o.action = 4;
                        o.visible = false;
                        o.set_var("intang", 1.0);
                        o.set_var("noFaceYaw", 0.0);
                        o.face_yaw = 0.0;
                        o.move_yaw = 0.0;
                        star = true;
                    }
                } else {
                    if o.timer == 0.0 {
                        let b = o.var("baseScale") - 0.5;
                        o.set_var("baseScale", b);
                    }
                    if update_during_nonlethal_hit(o, mario_yaw, 40.0) {
                        o.action = 1;
                    }
                }
            }
            4 => {
                boo_stop(o);
                mark_for_deletion(o);
            }
            _ => {}
        }
        let o = &mut m.objects[id];
        phys::move_standard(o, &world, 78.0);
        approach_opacity_and_scale(o);
        o.interact_status = 0;
        apply_interact_type(o);
        if o.var("noFaceYaw") == 0.0 {
            o.face_yaw = o.move_yaw;
        }
        if star {
            spawn_default_star(m, id, [-1600.0, -2100.0, 205.0]);
        }
    });
}

trait HomePos {
    fn set_pos_home(&mut self);
}
impl HomePos for SimObject {
    /// `cur_obj_set_pos_to_home`.
    fn set_pos_home(&mut self) {
        self.pos = self.home;
    }
}
