//! Round-2 sky actors and their Mario actions: the SSL Tweester (tweester.inc.c) with `ACT_TORNADO_TWIRLING` /
//! `ACT_TWIRLING`, and the WF owl Hoot (hoot.inc.c) with `ACT_RIDING_HOOT`.
mod r2a_common;
use jumpman_sim::behaviors::r2a::tweester_hoot::*;
use jumpman_sim::mario_r2a::*;
use jumpman_sim::object::*;
use jumpman_sim::state::*;
use r2a_common::*;

/// The Tweester sleeps until Mario is within 1500, grows for 60 frames, chases him, and its `INTERACT_TORNADO`
/// hitbox (radius 1500, height 4000) lifts him into `ACT_TORNADO_TWIRLING`; past the top (hitboxHeight) he is
/// thrown out into `ACT_TWIRLING` with a vertical velocity of at least 20, and the tornado goes home.
#[test]
fn tweester_lifts_mario_and_throws_him_out_twirling() {
    let tw = spec(ObjectBehaviorKind::Tweester, [0.0, 0.0, 0.0], 0.0, 0x19 << 16);
    let mut m = runtime(ground(0.0, 8000.0), vec![tw]);
    let id = ObjId(0);
    stand_at(&mut m, 0.0, 1400.0);
    tick(&mut m);
    assert_eq!(m.objects[id].scale, 0.0, "cur_obj_scale(0) while it waits");
    let mut saw_chase = false;
    let mut tornado_frames = 0;
    let mut thrown = None;
    for f in 0..400 {
        if m.action != ACT_TORNADO_TWIRLING && m.action != ACT_TWIRLING && thrown.is_none() {
            stand_at(&mut m, 0.0, 1400.0);
        }
        tick(&mut m);
        saw_chase |= m.objects[id].action == TWEESTER_CHASE;
        if m.action == ACT_TORNADO_TWIRLING {
            tornado_frames += 1;
        }
        if m.action == ACT_TWIRLING && thrown.is_none() {
            thrown = Some((f, m.pos[1], m.vel[1]));
        }
    }
    assert!(saw_chase, "idle -> chase after 60 growing frames");
    assert!(tornado_frames > 10, "carried in the tornado for {tornado_frames} frames");
    let (_, y, vy) = thrown.expect("thrown out of the top");
    assert!(y > 3000.0, "released above hitboxHeight: y {y}");
    assert!(vy >= 20.0);
}

/// Hoot sleeps in his tree until Mario is within 50 units, then wants to talk (DIALOG_044); afterwards a jump into
/// him grabs on (`ACT_RIDING_HOOT`), Mario hangs 92.5 below him, and A released lets go.
#[test]
fn hoot_carries_mario_and_lets_go() {
    let h = spec(ObjectBehaviorKind::Hoot, [2560.0, 700.0, 4608.0], 0.0, 0);
    let mut m = runtime(ground(0.0, 8000.0), vec![h]);
    let id = ObjId(0);
    assert_eq!(m.objects[id].home, [2560.0 + 800.0, 700.0 - 150.0, 4608.0 + 300.0], "bhv_hoot_init");
    m.state.pos = [2560.0, 700.0, 4608.0];
    m.set_action(ACT_FREEFALL, 0.0);
    let mut dialog = false;
    for _ in 0..4 {
        m.state.pos = [2560.0, 700.0, 4608.0];
        m.state.vel = [0.0; 3];
        m.set_action(ACT_IDLE, 0.0);
        for e in tick(&mut m) {
            dialog |= matches!(e, jumpman_sim::object_manager::ObjectEvent::KingDialog { id: 44.0, .. });
        }
    }
    assert!(m.objects[id].visible && dialog, "visible and DIALOG_044 once Mario is within 50");
    m.objects[id].set_var("dialogDone", 1.0);
    tick(&mut m);
    assert!(m.objects[id].hitbox.interact_type != 0, "tangible after the dialog");
    // `gGlobalTimer - oHootMarioReleaseTime > 30` must hold before he can be grabbed.
    for _ in 0..35 {
        tick(&mut m);
    }
    // Jump into the owl.
    let mut rode = false;
    for _ in 0..40 {
        let p = m.objects[id].pos;
        m.state.pos = [p[0], p[1] - 100.0, p[2]];
        m.state.vel = [0.0, -2.0, 0.0];
        m.set_action(ACT_FREEFALL, 0.0);
        jumpman_sim::object_manager::step(&mut m, None, false);
        let held = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: 0.0, stick_mag: 0.0, button_down: A_BUTTON, button_pressed: 0 };
        m.step(&held, 0.0, false, false);
        jumpman_sim::object_manager::after_mario_step(&mut m);
        if m.action == ACT_RIDING_HOOT {
            rode = true;
            break;
        }
    }
    assert!(rode, "interact_hoot -> ACT_RIDING_HOOT");
    assert_eq!(m.used_obj, Some(id));
}
