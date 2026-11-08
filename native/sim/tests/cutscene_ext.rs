//! Engine-free tests for `cutscene_ext.rs`: SM64 cutscene and special actions (gap #4).
//! Every expected tick count is derived from the decomp (`src/game/mario_actions_cutscene.c`,
//! `assets/anims/*.inc.c` loopEnd/startFrame, `level_update.c` credits) and noted next to it.
//! Frame accounting: `set_mario_animation` starts a new animation at `startFrame - 1` and the
//! frame advances once after each tick, so on call `n` (1-based) of an action whose animation
//! starts at 0 the action sees `animFrame == n - 2`.
use jumpman_sim::collision::*;
use jumpman_sim::cutscene_ext::*;
use jumpman_sim::mario_anim_meta::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::{ObjId, ObjectBehaviorKind, ObjectSpec};
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::sync::{Arc, Mutex};

fn flat(ceiling: Option<f64>) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-7000.0, 0.0, -7000.0], [-7000.0, 0.0, 7000.0], [7000.0, 0.0, 7000.0], &o);
    w.add_triangle([-7000.0, 0.0, -7000.0], [7000.0, 0.0, 7000.0], [7000.0, 0.0, -7000.0], &o);
    if let Some(y) = ceiling {
        // Opposite winding: a ceiling.
        w.add_triangle([-7000.0, y, -7000.0], [7000.0, y, 7000.0], [-7000.0, y, 7000.0], &o);
        w.add_triangle([-7000.0, y, -7000.0], [7000.0, y, -7000.0], [7000.0, y, 7000.0], &o);
    }
    w
}

fn mario(world: CollisionWorld) -> MarioRuntime {
    MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0)
}

/// A host that accepts the death warp (like `progress/flow.rs`): the sim must keep running the action.
fn with_death_hook(m: &mut MarioRuntime) -> Arc<Mutex<u32>> {
    let count = Arc::new(Mutex::new(0u32));
    let c = count.clone();
    m.request_death_warp = Some(Box::new(move |_| {
        *c.lock().unwrap() += 1;
        true
    }));
    count
}

fn idle() -> ControllerSnapshot {
    ControllerSnapshot::default()
}

fn step(m: &mut MarioRuntime) {
    m.step(&idle(), 0.0, false, false);
}

fn has_cue(m: &MarioRuntime, c: &CutCue) -> bool {
    m.cut.cues.contains(c)
}

#[test]
fn animation_headers_match_the_decomp() {
    // assets/anims/anim_*.inc.c: (flags, startFrame, loopStart, loopEnd).
    assert_eq!(MARIO_ANIM_META[MARIO_ANIM_ELECTROCUTION], (1, 0, 0, 53));
    assert_eq!(MARIO_ANIM_META[MARIO_ANIM_PUT_CAP_ON], (1, 10, 0, 46));
    assert_eq!(MARIO_ANIM_META[MARIO_ANIM_HEAD_STUCK_IN_GROUND], (1, 0, 0, 155));
    assert_eq!(MARIO_ANIM_META[MARIO_ANIM_UNLOCK_DOOR].3, 147);
    assert_eq!(MARIO_ANIM_META.len(), 209);
}

#[test]
fn anim_clock_follows_set_mario_animation_and_geo_update() {
    let mut c = AnimClock::default();
    assert_eq!(c.set(MARIO_ANIM_ELECTROCUTION), -1);
    c.advance();
    assert_eq!(c.set(MARIO_ANIM_ELECTROCUTION), 0);
    for _ in 0..60 {
        c.advance();
    }
    // NOLOOP: pinned at loopEnd - 1.
    assert_eq!(c.frame, 52);
    assert!(c.at_end());
    // A looping animation wraps to loopStart.
    let mut w = AnimClock::default();
    w.set(MARIO_ANIM_WALKING);
    let (_, _, ls, le) = MARIO_ANIM_META[MARIO_ANIM_WALKING];
    for _ in 0..(le + 3) {
        w.advance();
    }
    assert!(w.frame >= ls && w.frame < le);
}

#[test]
fn electrocution_triggers_the_death_warp_on_animation_frame_43() {
    let mut m = mario(flat(None));
    let warps = with_death_hook(&mut m);
    m.set_action(ACT_ELECTROCUTION, 0.0);
    m.health = 0xff as f64;
    let mut call = 0;
    while *warps.lock().unwrap() == 0 {
        step(&mut m);
        call += 1;
        assert!(call < 200);
    }
    // frame = call - 2 == 43  ->  call 45.
    assert_eq!(call, 45);
    assert_eq!(m.action, ACT_ELECTROCUTION, "the action keeps running while the host fades out");
}

#[test]
fn suffocation_triggers_the_death_warp_on_animation_frame_86() {
    let mut m = mario(flat(None));
    let warps = with_death_hook(&mut m);
    m.set_action(ACT_SUFFOCATION, 0.0);
    m.health = 0xff as f64;
    let mut call = 0;
    while *warps.lock().unwrap() == 0 {
        step(&mut m);
        call += 1;
        assert!(call < 300);
    }
    assert_eq!(call, 88);
}

#[test]
fn eaten_by_bubba_hides_mario_and_warps_after_61_calls() {
    let mut m = mario(flat(None));
    let warps = with_death_hook(&mut m);
    m.set_action(ACT_EATEN_BY_BUBBA, 0.0);
    for _ in 0..60 {
        step(&mut m);
    }
    assert_eq!(*warps.lock().unwrap(), 0);
    assert!(!m.cut.mario_visible);
    assert_eq!(m.health, 0xff as f64);
    step(&mut m);
    assert_eq!(*warps.lock().unwrap(), 1, "actionTimer++ == 60 on the 61st call");
}

#[test]
fn shocked_lands_then_electrocutes_when_health_is_gone() {
    let mut m = mario(flat(None));
    let _w = with_death_hook(&mut m);
    m.set_action(ACT_SHOCKED, 1.0); // grounded: skip the air phase
    m.health = 0x180 as f64;
    // SHOCKED animation (flags 0, loopEnd 6) loops: actionTimer++ every time it wraps to frame 0.
    let mut calls = 0;
    while m.action == ACT_SHOCKED {
        step(&mut m);
        calls += 1;
        assert!(calls < 100);
    }
    assert_eq!(m.action, ACT_IDLE, "health >= 0x100 -> idle");
    assert_eq!(m.invinc_timer, 30.0);

    let mut m = mario(flat(None));
    let _w = with_death_hook(&mut m);
    m.set_action(ACT_SHOCKED, 1.0);
    m.health = 0xff as f64;
    let mut calls = 0;
    while m.action == ACT_SHOCKED {
        step(&mut m);
        calls += 1;
        assert!(calls < 100);
    }
    assert_eq!(m.action, ACT_ELECTROCUTION, "health < 0x100 -> electrocution");
}

#[test]
fn shock_in_the_air_falls_first() {
    let mut m = mario(flat(None));
    m.pos[1] = 400.0;
    m.set_action(ACT_SHOCKED, 0.0);
    step(&mut m);
    assert_eq!(m.action_arg, 0.0);
    let mut calls = 0;
    while m.action_arg == 0.0 {
        step(&mut m);
        calls += 1;
        assert!(calls < 100);
    }
    assert_eq!(m.action_arg, 1.0, "landing switches to the grounded half");
    assert_eq!(m.pos[1], 0.0);
}

#[test]
fn squished_pancake_stays_until_there_is_room() {
    // 100 units of headroom: >= 10.1 -> scaled pancake, state stays 0.
    let mut m = mario(flat(Some(100.0)));
    m.set_action(ACT_SQUISHED, 0.0);
    step(&mut m);
    assert_eq!(m.action, ACT_SQUISHED);
    assert_eq!(m.action_state, 0.0);
    assert_eq!(m.squish_timer, 255.0);
    assert!((m.cut.scale[1] - 100.0 / 160.0).abs() < 1e-9);
    assert!((m.cut.scale[0] - (2.0 - 100.0 / 160.0)).abs() < 1e-9);
    // Room again (> 160): back to idle on the next call.
    m.world = Arc::new(flat(None));
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);
    assert_eq!(m.squish_timer, 0.0, "squishTimer = 0 then idle (the model un-squishes)");
}

#[test]
fn squished_flat_hurts_and_recovers_after_15_ticks_and_the_hurt_counter() {
    let mut m = mario(flat(Some(5.0)));
    m.set_action(ACT_SQUISHED, 0.0);
    m.flags |= MARIO_CAP_ON_HEAD;
    step(&mut m);
    assert_eq!(m.action_state, 1.0, "< 10.1 units: flattened");
    assert_eq!(m.cut.scale, [1.8, 0.05, 1.8]);
    // cap on: 12 hurt-counter units (3 wedges); the runtime drains one per tick, this tick included.
    assert_eq!(m.hurt_counter, 11.0);
    // Lift the ceiling to 40 (>= 30): state 2.
    m.world = Arc::new(flat(Some(40.0)));
    step(&mut m);
    assert_eq!(m.action_state, 2.0);
    let mut calls = 0;
    while m.action == ACT_SQUISHED {
        step(&mut m);
        calls += 1;
        assert!(calls < 100);
    }
    assert_eq!(m.action, ACT_IDLE);
    assert_eq!(m.squish_timer, 29.0, "30 set by the action, then squish_mario_model decrements it once");
    assert!(m.hurt_counter == 0.0);
}

#[test]
fn squished_for_ten_seconds_kills_mario_and_vanishes() {
    let mut m = mario(flat(Some(100.0)));
    let warps = with_death_hook(&mut m);
    m.set_action(ACT_SQUISHED, 0.0);
    let mut calls = 0;
    while *warps.lock().unwrap() == 0 {
        step(&mut m);
        calls += 1;
        assert!(calls < 400);
    }
    // actionArg++ > 300 is true on the 302nd call.
    assert_eq!(calls, 302);
    assert_eq!(m.action, ACT_DISAPPEARED);
    assert_eq!(m.health, 0xff as f64);
}

#[test]
fn disappeared_counts_down_the_warp_argument() {
    let mut m = mario(flat(None));
    let warps = with_death_hook(&mut m);
    // (WARP_OP_DEATH << 16) + 3: warps when the low half reaches 0, i.e. on the 3rd call.
    m.set_action(ACT_DISAPPEARED, ((0x12u32 << 16) + 3) as f64);
    step(&mut m);
    step(&mut m);
    assert_eq!(*warps.lock().unwrap(), 0);
    step(&mut m);
    assert_eq!(*warps.lock().unwrap(), 1);
    assert!(!m.cut.mario_visible);
}

#[test]
fn head_stuck_in_ground_ends_after_the_whole_animation() {
    let mut m = mario(flat(None));
    m.set_action(ACT_HEAD_STUCK_IN_GROUND, 0.0);
    // loopEnd 155: at_end at frame 154 = call 156.
    for _ in 0..155 {
        step(&mut m);
        assert_eq!(m.action, ACT_HEAD_STUCK_IN_GROUND);
    }
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn mashing_a_skips_to_the_unstick_frame() {
    let mut m = mario(flat(None));
    m.set_action(ACT_HEAD_STUCK_IN_GROUND, 0.0);
    let a = ControllerSnapshot { button_down: A_BUTTON, button_pressed: A_BUTTON, ..Default::default() };
    let mut calls = 0;
    while m.action == ACT_HEAD_STUCK_IN_GROUND {
        m.step(&a, 0.0, false, false);
        calls += 1;
        assert!(calls < 200);
    }
    // 5th press (call 5) sets the frame to unstuckFrame - 1 = 95, the clock then reads 96 on call 6,
    // and reaches 154 on call 64.
    assert_eq!(calls, 64);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn butt_and_feet_stuck_end_in_their_actions() {
    for (act, end, anim, expect_calls) in [
        (ACT_BUTT_STUCK_IN_GROUND, ACT_GROUND_POUND_LAND, MARIO_ANIM_BOTTOM_STUCK_IN_GROUND, 0),
        (ACT_FEET_STUCK_IN_GROUND, ACT_IDLE, MARIO_ANIM_LEGS_STUCK_IN_GROUND, 0),
    ] {
        let mut m = mario(flat(None));
        m.set_action(act, 0.0);
        let mut calls = 0;
        while m.action == act {
            step(&mut m);
            calls += 1;
            assert!(calls < 400);
        }
        let _ = expect_calls;
        // at_end when frame + 1 == loopEnd; frame = call - 2 (start frame 0) -> call = loopEnd + 1.
        assert_eq!(calls as i32, MARIO_ANIM_META[anim].3 + 1, "{act:#x}");
        // ACT_GROUND_POUND_LAND / ACT_IDLE (the first may chain into idle on the same tick).
        assert!(m.action == end || m.action & ACT_GROUP_MASK == ACT_GROUP_STATIONARY, "{:#x}", m.action);
    }
}

#[test]
fn snow_makes_a_long_fall_stick_feet_first() {
    let mut w = flat(None);
    w.snow_terrain = true;
    let mut m = mario(w);
    m.pos[1] = 0.0;
    m.peak_height = 1500.0; // fell 1500 > 1000 onto a soft, flat floor
    assert!(!should_get_stuck_in_ground(&m), "opt-in: the TS goldens never stick");
    m.cut.stuck_in_ground = true;
    assert!(should_get_stuck_in_ground(&m));
    assert!(check_stuck_feet(&mut m));
    assert_eq!(m.action, ACT_FEET_STUCK_IN_GROUND);
    let mut dry = mario(flat(None));
    dry.cut.stuck_in_ground = true;
    dry.peak_height = 1500.0;
    assert!(!should_get_stuck_in_ground(&dry), "only snow/sand terrain sticks");
}

#[test]
fn putting_on_cap_follows_the_animation() {
    let mut m = mario(flat(None));
    m.cut.cap_cutscene = true;
    m.set_action(ACT_IDLE, 0.0);
    assert!(cap_pickup_cutscene(&mut m));
    assert_eq!(m.action, ACT_PUTTING_ON_CAP);
    assert!(m.flags & MARIO_CAP_IN_HAND != 0 && m.flags & MARIO_CAP_ON_HEAD == 0);
    // PUT_CAP_ON starts at frame 10 (flags NOLOOP, loopEnd 46): the cap goes on at frame 28 (call 20)
    // and the action ends at frame 45 (call 37).
    for call in 1..=36 {
        step(&mut m);
        assert_eq!(m.action, ACT_PUTTING_ON_CAP, "call {call}");
        if call == 19 {
            assert!(m.flags & MARIO_CAP_ON_HEAD == 0);
        }
        if call == 20 {
            assert!(m.flags & MARIO_CAP_ON_HEAD != 0 && m.flags & MARIO_CAP_IN_HAND == 0);
        }
    }
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);
    // Disabled by default: the cap simply goes on.
    let mut d = mario(flat(None));
    assert!(!cap_pickup_cutscene(&mut d));
    assert!(d.flags & MARIO_CAP_ON_HEAD != 0);
}

#[test]
fn bbh_entrance_jump_then_spin_then_shrink_and_warp() {
    let mut m = mario(flat(None));
    let warps = with_death_hook(&mut m);
    let mgr = ObjectManager::new(vec![ObjectSpec::new(ObjectBehaviorKind::Bobomb, [600.0, 0.0, 0.0], 0.0)], None, None);
    m.set_objects(mgr);
    assert!(interact_bbh_entrance(&mut m, ObjId(0)));
    assert_eq!(m.action, ACT_BBH_ENTER_JUMP);
    assert!(!interact_bbh_entrance(&mut m, ObjId(0)) || m.action == ACT_BBH_ENTER_JUMP, "re-entering is ignored");
    step(&mut m);
    // vel.y = 60, forward = distance / 20 toward the cage.
    assert!(m.vel[1] > 0.0 && m.vel[1] <= 60.0);
    assert!((m.forward_vel - 600.0 / 20.0).abs() < 1.0);
    let mut calls = 0;
    while m.action == ACT_BBH_ENTER_JUMP {
        step(&mut m);
        calls += 1;
        assert!(calls < 100);
    }
    assert_eq!(m.action, ACT_BBH_ENTER_SPIN, "apex -> spin");
    calls = 0;
    while m.action == ACT_BBH_ENTER_SPIN && m.action_state < 4.0 {
        step(&mut m);
        calls += 1;
        assert!(calls < 200);
    }
    assert_eq!(m.action_state, 4.0, "landed in the cage");
    assert_eq!(*warps.lock().unwrap(), 0, "WARP_OP_UNKNOWN_02 is not the death warp");
    assert!(m.cues_contains_warp(WarpOp::Unknown02));
    step(&mut m);
    assert!(!m.cut.mario_visible);
    assert!(m.cut.scale[0] < 1.0, "shrunk while falling in: {:?}", m.cut.scale);
}

trait CueExt {
    fn cues_contains_warp(&self, op: WarpOp) -> bool;
}
impl CueExt for MarioRuntime {
    fn cues_contains_warp(&self, op: WarpOp) -> bool {
        has_cue(self, &CutCue::Warp(op))
    }
}

#[test]
fn emerge_from_pipe_is_hidden_for_11_calls_then_hops_out_and_lands() {
    let mut m = mario(flat(None));
    m.pos[1] = 200.0;
    m.set_action(ACT_EMERGE_FROM_PIPE, 0.0);
    for _ in 0..11 {
        step(&mut m);
        assert!(!m.cut.mario_visible);
    }
    step(&mut m);
    assert!(m.cut.mario_visible);
    let mut calls = 0;
    while m.action == ACT_EMERGE_FROM_PIPE {
        step(&mut m);
        calls += 1;
        assert!(calls < 100);
    }
    assert_eq!(m.action, ACT_JUMP_LAND_STOP);
    assert_eq!(m.forward_vel, 0.0);
    assert_eq!(m.pos[1], 0.0);
}

#[test]
fn warp_door_spawn_waits_one_call_then_idles_or_greets() {
    let mut m = mario(flat(None));
    m.set_action(ACT_WARP_DOOR_SPAWN, 1.0);
    step(&mut m);
    assert_eq!(m.action, ACT_WARP_DOOR_SPAWN);
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);

    let mut m = mario(flat(None));
    m.cut.never_entered_castle = true;
    m.set_action(ACT_WARP_DOOR_SPAWN, 1.0);
    step(&mut m);
    step(&mut m);
    assert_eq!(m.action, ACT_READING_AUTOMATIC_DIALOG);
    assert_eq!(m.action_arg, 21.0, "DIALOG_021");
}

#[test]
fn automatic_dialog_timeline() {
    let mut m = mario(flat(None));
    m.set_action(ACT_READING_AUTOMATIC_DIALOG, 20.0);
    let mut dialog_cues = 0;
    let mut stopped_at = None;
    for call in 1..=40 {
        step(&mut m);
        if call == 24 {
            assert_eq!(m.action_timer, 0.0, "head tilts back down by the same amount");
        }
        let cues = m.cut.take_cues();
        dialog_cues += cues.iter().filter(|c| matches!(c, CutCue::Dialog(20))).count();
        if cues.contains(&CutCue::TimeStop(true)) {
            stopped_at = Some(call);
        }
        if call == 12 {
            assert!(m.cut.dialog_open, "the box stays open until the host closes it");
            assert_eq!(m.action, ACT_READING_AUTOMATIC_DIALOG);
            assert_eq!(m.action_timer, -8.0 * 1024.0, "head tilts up by 1024 for the 8 states before the box");
            m.cut.dialog_open = false;
        }
    }
    assert_eq!(dialog_cues, 1);
    assert_eq!(stopped_at, Some(2), "enable_time_stop at actionState == 2");
    assert_eq!(m.action, ACT_IDLE);
    assert!(!m.cut.time_stop);
}

#[test]
fn star_door_dialog_after_unlocking_walks_into_the_door() {
    let mut m = mario(flat(None));
    m.prev_action = ACT_UNLOCKING_STAR_DOOR;
    m.set_action(ACT_READING_AUTOMATIC_DIALOG, 38.0);
    m.prev_action = ACT_UNLOCKING_STAR_DOOR;
    let mut calls = 0;
    while m.action == ACT_READING_AUTOMATIC_DIALOG {
        step(&mut m);
        m.cut.dialog_open = false;
        calls += 1;
        assert!(calls < 60);
    }
    assert_eq!(calls, 25, "actionState 25 ends the dialog action");
    assert_eq!(m.action, ACT_WALKING);
}

#[test]
fn intro_cutscene_script() {
    let mut m = mario(flat(None));
    m.set_action(ACT_INTRO_CUTSCENE, 0.0);
    step(&mut m);
    assert_eq!(m.action_arg, 1.0);
    assert!(!m.cut.mario_visible);
    assert_eq!(m.cut.camera_event, Some(CameraEvent::StartIntro));
    let cues = m.cut.take_cues();
    assert!(cues.contains(&CutCue::Hud(false)) && cues.contains(&CutCue::Camera(CameraEvent::StartIntro)));
    // The Peach/Lakitu camera scene holds the action until it clears the camera event.
    for _ in 0..50 {
        step(&mut m);
    }
    assert_eq!(m.action_arg, 1.0);
    m.cut.camera_event = None;
    m.cut.take_cues();
    // actionTimer++ == 37 on the 38th call spawns the pipe.
    for _ in 0..37 {
        step(&mut m);
        assert_eq!(m.action_arg, 1.0);
    }
    step(&mut m);
    assert_eq!(m.action_arg, 2.0);
    assert_eq!(m.cut.intro_pipe_y, Some(60.0));
    assert!(has_cue(&m, &CutCue::IntroPipe(Some(INTRO_PIPE_POS))));
    // Raise: +10 per call up to 260, `actionTimer++ == 38` -> vel.y = 60.
    for _ in 0..38 {
        step(&mut m);
        assert_eq!(m.action_arg, 2.0);
    }
    assert_eq!(m.cut.intro_pipe_y, Some(260.0));
    step(&mut m);
    assert_eq!(m.action_arg, 3.0);
    assert_eq!(m.vel[1], 60.0);
    // Jump out: HUD returns at actionTimer == 25, Mario appears at >= 118.
    m.cut.take_cues();
    m.pos[1] = 260.0; // the pipe top (MARIO_POS y 260 over a floor at 0)
    for call in 1..=118 {
        step(&mut m);
        assert!(!m.cut.mario_visible, "hidden until actionTimer >= 118 (call {call})");
        if call == 26 {
            assert!(has_cue(&m, &CutCue::Hud(true)));
        }
    }
    let mut calls = 0;
    while m.action_arg == 3.0 {
        step(&mut m);
        calls += 1;
        assert!(calls < 200);
    }
    assert!(m.cut.mario_visible);
    assert_eq!(m.action_arg, 4.0, "landed outside the pipe");
    while m.action_arg == 4.0 {
        step(&mut m);
        calls += 1;
        assert!(calls < 400);
    }
    // Lower pipe: -5 per call from 260 until <= 50 (42 calls), then deleted.
    assert_eq!(m.action_arg, 5.0);
    m.cut.take_cues();
    for _ in 0..42 {
        step(&mut m);
    }
    assert_eq!(m.action_arg, 6.0);
    assert_eq!(m.cut.intro_pipe_y, None);
    assert!(has_cue(&m, &CutCue::IntroPipe(None)));
    // Idle only once the camera cutscene is over.
    m.cut.camera_cutscene_done = false;
    step(&mut m);
    assert_eq!(m.action, ACT_INTRO_CUTSCENE);
    m.cut.camera_cutscene_done = true;
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn credits_cutscene_timers_and_viewport() {
    assert_eq!(TIMER_CREDITS_SHOW, 61.0);
    assert_eq!(TIMER_CREDITS_PROGRESS, 90.0);
    assert_eq!(TIMER_CREDITS_WARP, 200.0);
    // 20 steps of +2 reach the 40% cap; flags 1 -> negative offsets.
    assert_eq!(credits_viewport(40, 1), ([384, 288], [497, 354]));
    assert_eq!(credits_viewport(40, 0x30), ([384, 288], [640 + 256 * 56 / 100, 480 + 192 * 66 / 100]));
    let entry = &CREDITS_SEQUENCE[2]; // WF
    assert_eq!(entry.level, "wf");
    let mut m = mario(flat(None));
    start_credits_scene(&mut m, entry);
    assert_eq!(m.action, ACT_CREDITS_CUTSCENE);
    assert_eq!(m.pos, [347.0, 5376.0, 326.0]);
    assert_eq!(m.face_angle[1], (46i32 << 8) as f64);
    for call in 1..=205 {
        step(&mut m);
        if call == 61 {
            assert!(m.cut.credits_viewport.is_none(), "viewport shrinks from actionTimer >= 61 (call 62)");
        }
        if call == 62 {
            assert_eq!(m.action_state, 2.0);
            assert!(m.cut.credits_viewport.is_some());
        }
        if call == 90 {
            assert!(!m.cut.credits_text_visible);
        }
        if call == 91 {
            assert!(m.cut.credits_text_visible);
        }
        if call == 200 {
            assert!(!has_cue(&m, &CutCue::Warp(WarpOp::CreditsNext)));
        }
        if call == 201 {
            assert!(has_cue(&m, &CutCue::Warp(WarpOp::CreditsNext)), "actionTimer++ == 200 on call 201");
        }
    }
    assert_eq!(m.action_state, 40.0);
    assert_eq!(m.cut.model_yaw_offset, ((50 & 0xc0) << 8) as f64);
}

#[test]
fn credits_table_matches_level_update_c() {
    assert_eq!(CREDITS_SEQUENCE.len(), 22);
    assert_eq!(CREDITS_SEQUENCE[0].mario_pos, [0, 8000, 0]);
    assert_eq!(CREDITS_SEQUENCE[1].text, &["1GAME DIRECTOR", "SHIGERU MIYAMOTO"]);
    assert_eq!(CREDITS_SEQUENCE[20].text[0], "1EXECUTIVE PRODUCER");
    assert_eq!(CREDITS_SEQUENCE[21].mario_pos, [0, 906, -1200]);
    // HMC: unk02 = -15 as u8; DDD area 2: -111.
    assert_eq!(CREDITS_SEQUENCE[6].flags, 241);
    assert_eq!(CREDITS_SEQUENCE[19].flags, 145);
    assert_eq!(CREDITS_SEQUENCE.iter().filter(|e| !e.text.is_empty()).count(), 20);
}

#[test]
fn end_waving_spawns_the_actors_and_ends_after_301_calls() {
    let mut m = mario(flat(None));
    m.set_action(ACT_END_WAVING_CUTSCENE, 0.0);
    step(&mut m);
    assert_eq!(m.cut.camera_event, Some(CameraEvent::StartEndWaving));
    assert_eq!(m.cut.end_waving_actors, Some([[60.0, 906.0, -1180.0], [180.0, 906.0, -1170.0], [-180.0, 906.0, -1170.0]]));
    for _ in 0..299 {
        step(&mut m);
    }
    assert!(!has_cue(&m, &CutCue::Warp(WarpOp::CreditsEnd)));
    step(&mut m);
    step(&mut m);
    assert!(has_cue(&m, &CutCue::Warp(WarpOp::CreditsEnd)));
}

#[test]
fn entering_star_door_takes_48_calls_and_walks_through() {
    let mut m = mario(flat(None));
    m.set_action(ACT_ENTERING_STAR_DOOR, 1.0);
    let start = m.pos;
    for call in 1..=47 {
        step(&mut m);
        assert_eq!(m.action, ACT_ENTERING_STAR_DOOR, "call {call}");
    }
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);
    // Frames 35..47 walk 12 units each in the door's facing.
    let moved = ((m.pos[0] - start[0]).powi(2) + (m.pos[2] - start[2]).powi(2)).sqrt();
    assert!(moved > 100.0, "{moved}");
}

#[test]
fn unlocking_key_door_runs_the_unlock_animation_then_walks() {
    let mut m = mario(flat(None));
    let mgr = ObjectManager::new(vec![ObjectSpec::new(ObjectBehaviorKind::Bobomb, [0.0, 0.0, 300.0], 0.0)], None, None);
    m.set_objects(mgr);
    m.used_obj = Some(ObjId(0));
    m.set_action(ACT_UNLOCKING_KEY_DOOR, 0.0);
    // UNLOCK_DOOR: loopEnd 147 -> at_end at frame 146 = call 148.
    for call in 1..=147 {
        step(&mut m);
        assert_eq!(m.action, ACT_UNLOCKING_KEY_DOOR, "call {call}");
    }
    assert!(has_cue(&m, &CutCue::SpawnKeyDoorKey));
    step(&mut m);
    assert_eq!(m.action, ACT_WALKING);
    assert!(m.cut.cues.iter().any(|c| matches!(c, CutCue::UnlockDoor { star: false, .. })));
}

#[test]
fn unlocking_star_door_summons_the_star_then_reads_dialog_38() {
    let mut m = mario(flat(None));
    m.set_action(ACT_UNLOCKING_STAR_DOOR, 1.0);
    let mut spawned_at = None;
    let mut calls = 0;
    while m.action == ACT_UNLOCKING_STAR_DOOR {
        step(&mut m);
        calls += 1;
        if spawned_at.is_none() && has_cue(&m, &CutCue::SpawnUnlockDoorStar) {
            spawned_at = Some(calls);
        }
        assert!(calls < 600);
    }
    assert_eq!(m.action, ACT_READING_AUTOMATIC_DIALOG);
    assert_eq!(m.action_arg, 38.0);
    let spawned = spawned_at.unwrap();
    // SUMMON_STAR is a one-shot clip; the star appears when it ends, then 70 calls of waiting
    // (actionTimer++ == 70) and the RETURN_STAR_APPROACH_DOOR clip.
    let summon_len = MARIO_ANIM_META[MARIO_ANIM_SUMMON_STAR].3;
    assert_eq!(spawned, summon_len + 1);
    let ret = MARIO_ANIM_META[MARIO_ANIM_RETURN_STAR_APPROACH_DOOR].3;
    assert_eq!(calls, spawned + 71 + ret);
    assert!(m.cut.cues.iter().any(|c| matches!(c, CutCue::UnlockDoor { star: true, .. })));
}

#[test]
fn pulling_door_ends_when_the_clip_ends() {
    let mut m = mario(flat(None));
    m.set_action(ACT_PULLING_DOOR, 1.0);
    let len = MARIO_ANIM_META[MARIO_ANIM_PULL_DOOR_WALK_IN].3;
    // at_end when frame + 1 == loopEnd; frame = call - 2 -> the action ends on call len + 1.
    for call in 1..=len {
        step(&mut m);
        assert_eq!(m.action, ACT_PULLING_DOOR, "call {call}");
    }
    step(&mut m);
    assert_eq!(m.action, ACT_IDLE);
    // Warp doors (arg & 4) warp on actionTimer == 16.
    let mut w = mario(flat(None));
    w.set_action(ACT_PUSHING_DOOR, 6.0);
    for _ in 0..17 {
        step(&mut w);
    }
    assert!(has_cue(&w, &CutCue::Warp(WarpOp::WarpDoor)));
}

#[test]
fn waiting_for_dialog_and_ready_to_speak() {
    let mut m = mario(flat(None));
    assert!(mario_ready_to_speak(&m));
    m.set_action(ACT_WAITING_FOR_DIALOG, 0.0);
    step(&mut m);
    assert_eq!(m.action, ACT_WAITING_FOR_DIALOG);
    assert!(mario_ready_to_speak(&m));
    m.set_action(ACT_SQUISHED, 0.0);
    assert!(!mario_ready_to_speak(&m));
}

#[test]
fn debug_free_move_flies_and_a_returns_to_idle() {
    let mut m = mario(flat(None));
    m.set_action(ACT_DEBUG_FREE_MOVE, 0.0);
    let up = ControllerSnapshot { button_down: C_UP, ..Default::default() };
    m.step(&up, 0.0, false, false);
    assert!((m.pos[1] - 16.0).abs() < 1e-9);
    let a = ControllerSnapshot { button_pressed: A_BUTTON, button_down: A_BUTTON, ..Default::default() };
    m.step(&a, 0.0, false, false);
    assert_eq!(m.action, ACT_IDLE);
}

#[test]
fn squish_input_requires_a_dynamic_surface_and_a_tight_gap() {
    // Static geometry never squishes (only dynamic floors / ceilings do).
    let mut m = mario(flat(Some(100.0)));
    m.cut.squish = true;
    step(&mut m);
    assert_eq!(m.input & INPUT_SQUISHED, 0);
    assert!(!squish_input(&m));
    // Dynamic geometry (owned by an object) with a 100-unit gap squishes, but only once the host opts in.
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-7000.0, 0.0, -7000.0], [-7000.0, 0.0, 7000.0], [7000.0, 0.0, 7000.0], &o);
    w.add_triangle([-7000.0, 0.0, -7000.0], [7000.0, 0.0, 7000.0], [7000.0, 0.0, -7000.0], &o);
    let dynamic = SurfaceOptions { object: Some(1), ..Default::default() };
    w.add_triangle([-7000.0, 100.0, -7000.0], [7000.0, 100.0, 7000.0], [-7000.0, 100.0, 7000.0], &dynamic);
    w.add_triangle([-7000.0, 100.0, -7000.0], [7000.0, 100.0, -7000.0], [7000.0, 100.0, 7000.0], &dynamic);
    let mut d = mario(w);
    step(&mut d);
    assert_eq!(d.action, ACT_IDLE, "squish is opt-in");
    d.cut.squish = true;
    step(&mut d);
    assert_eq!(d.action, ACT_SQUISHED, "INPUT_SQUISHED -> drop_and_set_mario_action(ACT_SQUISHED)");
}

#[test]
fn shock_interaction_sets_the_action_argument_like_interact_shock() {
    let mut m = mario(flat(None));
    assert!(interact_shock(&mut m, 2.0));
    assert_eq!((m.action, m.action_arg), (ACT_SHOCKED, 1.0), "grounded: arg 1");
    let mut a = mario(flat(None));
    a.pos[1] = 300.0;
    a.set_action(ACT_JUMP, 0.0);
    assert!(interact_shock(&mut a, 2.0));
    assert_eq!((a.action, a.action_arg), (ACT_SHOCKED, 0.0), "airborne: arg 0");
    assert_eq!(a.hurt_counter, 8.0);
}

#[test]
fn dialog_table_is_the_us_text() {
    use jumpman_sim::sm64_dialogs::*;
    assert_eq!(DIALOG_COUNT, 170);
    assert_eq!(dialog(0).unwrap().lines_per_box, 6);
    assert_eq!(dialog(20).unwrap().text, "Dear Mario:\nPlease come to the\ncastle. I've baked\na cake for you.\nYours truly--\nPrincess Toadstool");
    assert_eq!(dialog(38).unwrap().text, "Reacting to the Star\npower, the door slowly\nopens.");
    assert!(dialog(0).unwrap().text.contains("comrades"), "US macro expansion");
    assert!(dialog(169).is_some());
}

#[test]
fn ground_pound_into_snow_sticks_butt_first_and_a_dive_sticks_head_first() {
    let mut w = flat(None);
    w.snow_terrain = true;
    let mut m = mario(w);
    m.cut.stuck_in_ground = true;
    m.pos = [0.0, 2500.0, 0.0];
    m.peak_height = 2500.0;
    m.set_action(ACT_GROUND_POUND, 0.0);
    let mut calls = 0;
    while m.action == ACT_GROUND_POUND {
        step(&mut m);
        calls += 1;
        assert!(calls < 200);
    }
    assert_eq!(m.action, ACT_BUTT_STUCK_IN_GROUND, "act_ground_pound: should_get_stuck_in_ground");

    let mut w = flat(None);
    w.snow_terrain = true;
    let mut d = mario(w);
    d.cut.stuck_in_ground = true;
    d.pos = [0.0, 2500.0, 0.0];
    d.peak_height = 2500.0;
    d.set_action(ACT_DIVE, 0.0);
    d.forward_vel = 10.0;
    let mut calls = 0;
    while d.action == ACT_DIVE {
        step(&mut d);
        calls += 1;
        assert!(calls < 200);
    }
    assert_eq!(d.action, ACT_HEAD_STUCK_IN_GROUND, "act_dive: faceAngle[0] == -0x2AAA when it lands");

    // The same fall on dry ground never sticks.
    let mut dry = mario(flat(None));
    dry.cut.stuck_in_ground = true;
    dry.pos = [0.0, 2500.0, 0.0];
    dry.peak_height = 2500.0;
    dry.set_action(ACT_GROUND_POUND, 0.0);
    let mut calls = 0;
    while dry.action == ACT_GROUND_POUND {
        step(&mut dry);
        calls += 1;
        assert!(calls < 200);
    }
    assert_ne!(dry.action, ACT_BUTT_STUCK_IN_GROUND);
}
