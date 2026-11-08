//! Animation choice for the extended Banjo moveset (`jumpman_sim::banjo_ext`): the clip and the
//! phase of the bs state it plays, using the exported `banjo-extra.json` clips (see
//! `scripts/export-bk-banjo.py --extras`) and the base clips where they already exist.
use super::banjo_pose::{Choice, Part};
use jumpman_sim::banjo_ext::*;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::ACT_GROUND_POUND_LAND;

const UPPER: &[Part] = &[Part::KazooieUpper];
const UPPER_FEET: &[Part] = &[Part::KazooieUpper, Part::KazooieFeet];
const ASS: &[Part] = &[Part::KazooieAss];

pub fn choose_ext(m: &MarioRuntime) -> Option<Choice> {
    let t = m.action_timer;
    let mv = jumpman_sim::characters::banjo_move(m);
    if mv < BANJO_TROT {
        return None;
    }
    let moving = m.forward_vel.abs() > 1.5;
    Some(if mv == BANJO_TROT {
        let turbo = m.banjo.turbo_frames > 0.0;
        if !moving {
            Choice::new("trotIdle", true).parts(UPPER_FEET)
        } else {
            let top = if turbo { 1000.0 } else { 700.0 } * 1.25 / 30.0;
            let d = if turbo { 0.29 } else { 0.34 } + (0.56 - 0.34) * (1.0 - (m.forward_vel.abs() / top).min(1.0));
            Choice::new(if turbo { "trotTurbo" } else { "trot" }, true).duration(d.max(0.25)).parts(UPPER_FEET)
        }
    } else if mv == BANJO_TROT_JUMP {
        Choice::new(if m.vel[1] > 0.0 { "jump" } else { "fall" }, m.vel[1] <= 0.0).start(if m.vel[1] > 0.0 { 0.35 } else { 0.0 }).parts(UPPER_FEET)
    } else if mv == BANJO_BUSTER {
        // 0..0.35 tuck, 0.35..0.73 dive, the impact frames hold the end pose.
        let phase = if m.action == ACT_GROUND_POUND_LAND {
            0.75
        } else if t < 11.0 {
            t / 11.0 * 0.35
        } else {
            (0.35 + (t - 11.0) / 12.0 * 0.38).min(0.73)
        };
        Choice::new("bust", false).at(phase).parts(UPPER)
    } else if mv == BANJO_BUSTER_HOP {
        Choice::new("bust", false).at((0.75 + t / 30.0 * 0.2).min(0.97)).parts(UPPER)
    } else if mv == BANJO_SHOCK_CHARGE {
        Choice::new("shockCharge", false).at((t / 31.0 * 0.3637).min(0.3637)).parts(UPPER_FEET)
    } else if mv == BANJO_SHOCK_JUMP {
        Choice::new("shockJump", false).at(((t / 24.0) * 0.5 + 0.5304).min(0.999)).parts(UPPER_FEET)
    } else if mv == BANJO_EGG_HEAD {
        Choice::new("eggHead", false).at((t / 30.0).min(0.999)).parts(UPPER)
    } else if mv == BANJO_EGG_ASS {
        Choice::new("eggAss", false).at((t / 30.0).min(0.999)).parts(ASS)
    } else if mv == BANJO_WONDER {
        if moving {
            Choice::new("wonderWalk", true).duration(0.53).parts(UPPER)
        } else {
            Choice::new("wonderIdle", true).parts(UPPER)
        }
    } else if mv == BANJO_WONDER_JUMP {
        Choice::new("wonderJump", false).at(if m.vel[1] > 0.0 { (t / 24.0 * 0.45).min(0.45) } else { 0.6 }).parts(UPPER)
    } else if mv == BANJO_FLY_ENTER {
        Choice::new("flyEnter", false).at((t / 42.0).min(0.999)).parts(UPPER)
    } else if mv == BANJO_FLY {
        let flapping = m.banjo.fly_flap > 0.0;
        Choice::new("fly", true).duration(if flapping { 0.3 } else { 0.62 }).parts(UPPER)
    } else if mv == BANJO_BOMB {
        Choice::new("bombStart", false).at(if t < 21.0 { t / 21.0 * 0.6905 } else { 0.9 }).parts(UPPER)
    } else if mv == BANJO_BOMB_END {
        Choice::new("bombEnd", true).parts(UPPER)
    } else {
        return None;
    })
}

/// Visual pitch (radians, nose down positive) while flying or bombing.
pub fn flight_pitch(m: &MarioRuntime) -> f32 {
    let mv = jumpman_sim::characters::banjo_move(m);
    if mv == BANJO_FLY {
        m.banjo.fly_pitch.to_radians() as f32
    } else if mv == BANJO_BOMB && m.action_timer >= 21.0 {
        (-m.banjo.bomb_dir[1]).asin() as f32
    } else {
        0.0
    }
}
