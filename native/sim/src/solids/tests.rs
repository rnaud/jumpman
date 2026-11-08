use super::*;
use crate::collision::CollisionWorld;
use crate::mario_runtime::SpawnPoint;
use crate::state::ControllerSnapshot;

fn world_floor(y: f64) -> Arc<CollisionWorld> {
    let mut w = CollisionWorld::default();
    w.add_triangle([-9000.0, y, -9000.0], [9000.0, y, 9000.0], [9000.0, y, -9000.0], &SurfaceOptions::default());
    w.add_triangle([-9000.0, y, -9000.0], [-9000.0, y, 9000.0], [9000.0, y, 9000.0], &SurfaceOptions::default());
    Arc::new(w)
}

fn mario(pos: Vec3) -> MarioRuntime {
    MarioRuntime::new(world_floor(-3000.0), SpawnPoint { action: None, pos, face_yaw: 0.0 }, -9000.0)
}

/// A flat 400 x 400 square top at y = 0 (two NOT_SLIPPERY triangles), the shape most of the
/// TTC platforms reduce to for these scenarios.
fn slab() -> SolidMesh {
    SolidMesh {
        vertices: vec![[-200.0, 0.0, -200.0], [200.0, 0.0, -200.0], [200.0, 0.0, 200.0], [-200.0, 0.0, 200.0]],
        triangles: vec![[0, 2, 1, crate::surfaces::SURFACE_NOT_SLIPPERY, 0], [0, 3, 2, crate::surfaces::SURFACE_NOT_SLIPPERY, 0]],
    }
}

fn spec(behavior: &str, pos: Vec3, angle: [f64; 3], bparams: u32) -> SolidSpec {
    SolidSpec { behavior: behavior.into(), model: String::new(), collision: Some("slab".into()), pos, angle, bparams, acts: None, track: vec![] }
}

fn solids(specs: &[SolidSpec], speed: i32) -> Solids {
    ttc::reset_shared();
    let mut meshes = HashMap::new();
    meshes.insert("slab".to_string(), slab());
    Solids::new(specs, &meshes, speed)
}

#[test]
fn matrix_matches_math_util() {
    // mtxf_rotate_zxy_and_translate with yaw 0x4000: +Z (forward at yaw 0) maps to +X.
    let m = mtxf_rotate_zxy_and_translate([10.0, 20.0, 30.0], [0.0, 0x4000 as f64, 0.0]);
    assert_eq!(transform_vertex(&m, [0.0, 0.0, 100.0]), [110.0, 20.0, 30.0]);
    assert_eq!(transform_vertex(&m, [100.0, 0.0, 0.0]), [10.0, 20.0, -70.0]);
    // Vertices are truncated to s16 (TerrainData), not rounded.
    let m = mtxf_rotate_zxy_and_translate([0.5, 0.5, 0.5], [0.0; 3]);
    assert_eq!(transform_vertex(&m, [1.0, 1.0, 1.0]), [1.0, 1.0, 1.0]);
}

#[test]
fn minute_hand_picks_the_clock_speed() {
    // clock_arm.inc.c thresholds
    assert_eq!(ttc::speed_from_minute_hand(0x0000), TTC_SPEED_STOPPED);
    assert_eq!(ttc::speed_from_minute_hand(0x0AAA), TTC_SPEED_FAST);
    assert_eq!(ttc::speed_from_minute_hand(0x6AA3), TTC_SPEED_FAST);
    assert_eq!(ttc::speed_from_minute_hand(0x6AA4), TTC_SPEED_RANDOM);
    assert_eq!(ttc::speed_from_minute_hand(0x954B), TTC_SPEED_RANDOM);
    assert_eq!(ttc::speed_from_minute_hand(0x954C), TTC_SPEED_SLOW);
    assert_eq!(ttc::speed_from_minute_hand(0xF545), TTC_SPEED_SLOW);
    assert_eq!(ttc::speed_from_minute_hand(0xF546), TTC_SPEED_STOPPED);
    // The hand starts at roll 0 and counts down by 0x180 per frame.
    assert_eq!(ttc::minute_hand_roll(1), 0xFE80);
    // just after the painting loads the hand is within 15 degrees of 12 o'clock: stopped
    assert_eq!(ttc::speed_from_minute_hand(ttc::minute_hand_roll(1)), TTC_SPEED_STOPPED);
    // 100 frames later (0x6A00) it is in the 210..345 degree band: fast
    assert_eq!(ttc::minute_hand_roll(100), 0x6A00);
    assert_eq!(ttc::speed_from_minute_hand(ttc::minute_hand_roll(100)), TTC_SPEED_FAST);
}

#[test]
fn rotating_cube_waits_dips_then_quarter_turns() {
    let mut m = mario([0.0, -2000.0, 0.0]);
    let mut s = solids(&[spec("bhvTTCRotatingSolid", [0.0, 100.0, 0.0], [0.0; 3], 0)], TTC_SPEED_SLOW);
    s.install(&mut m);
    // ttc_rotating_solid.inc.c: waits sTTCRotatingSolidInitialDelays[SLOW] = 120 frames
    for _ in 0..121 {
        s.step(&mut m);
    }
    assert_eq!(s.list[0].o.pos[1], 100.0);
    // frame 122: velY -5 -> -4.5
    s.step(&mut m);
    assert_eq!(s.list[0].o.pos[1], 100.0 - 4.5);
    // 19 frames of dip and return (sum of -4.5..0 then 0.5..4.5 is zero)
    for _ in 0..18 {
        s.step(&mut m);
    }
    assert_eq!(s.list[0].o.pos[1], 100.0);
    // 6 frames of sound countdown, then roll by 1200 per frame toward 1/4 turn (0x4000)
    for _ in 0..6 {
        s.step(&mut m);
    }
    assert_eq!(s.list[0].o.face[2], 0.0);
    s.step(&mut m);
    assert_eq!(s.list[0].o.face[2], 1200.0);
    assert_eq!(s.list[0].o.angle_vel[2], 1200.0);
    for _ in 0..13 {
        s.step(&mut m);
    }
    assert_eq!(to_s16(s.list[0].o.face[2]), 0x4000);
}

#[test]
fn stopped_clock_freezes_everything() {
    let mut m = mario([0.0, -2000.0, 0.0]);
    let specs = [
        spec("bhvTTCRotatingSolid", [0.0, 100.0, 0.0], [0.0; 3], 0),
        spec("bhvTTCSpinner", [900.0, 100.0, 0.0], [0.0; 3], 0),
        spec("bhvTTCElevator", [1300.0, 100.0, 0.0], [0.0; 3], 0),
        spec("bhvTTCCog", [1700.0, 100.0, 0.0], [0.0; 3], 0),
    ];
    let mut s = solids(&specs, TTC_SPEED_STOPPED);
    s.install(&mut m);
    let before: Vec<_> = s.list.iter().map(|x| (x.o.pos, x.o.face)).collect();
    for _ in 0..300 {
        s.step(&mut m);
    }
    for (x, b) in s.list.iter().zip(before) {
        assert_eq!((x.o.pos, x.o.face), b, "{}", x.spec.behavior);
    }
    // bhv_ttc_pendulum_init: the stopped pendulum hangs at 6371.5557 (truncated into the roll)
    let mut s = solids(&[spec("bhvTTCPendulum", [0.0, 100.0, 0.0], [0.0; 3], 0)], TTC_SPEED_STOPPED);
    s.install(&mut m);
    s.step(&mut m);
    assert_eq!(s.list[0].o.face[2], 6371.0);
}

#[test]
fn pendulum_swings_with_the_slow_acceleration() {
    let mut m = mario([0.0, -2000.0, 0.0]);
    let mut s = solids(&[spec("bhvTTCPendulum", [0.0, 100.0, 0.0], [0.0; 3], 0)], TTC_SPEED_SLOW);
    s.install(&mut m);
    // starts at 6500 with accel 13: the accel direction flips (angle * dir > 0), vel = -13, -26, ...
    s.step(&mut m);
    assert_eq!(s.list[0].o.face[2], 6500.0 - 13.0);
    s.step(&mut m);
    assert_eq!(s.list[0].o.face[2], 6500.0 - 13.0 - 26.0);
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for _ in 0..600 {
        s.step(&mut m);
        min = min.min(s.list[0].o.face[2]);
        max = max.max(s.list[0].o.face[2]);
    }
    assert!(min < -6000.0 && max > 6000.0, "swing {min}..{max}");
    // the pendulum never gives platform displacement
    assert_eq!(s.list[0].o.angle_vel, [0.0; 3]);
}

#[test]
fn elevator_runs_to_its_peak_and_back() {
    let mut m = mario([0.0, -2000.0, 0.0]);
    let mut s = solids(&[spec("bhvTTCElevator", [0.0, 100.0, 0.0], [0.0; 3], 0)], TTC_SPEED_SLOW);
    s.install(&mut m);
    // speed 6 per frame up to home + 500 (no bparam): 83 frames reach 598, the 84th clamps at 600
    for _ in 0..83 {
        s.step(&mut m);
    }
    assert_eq!(s.list[0].o.pos[1], 100.0 + 6.0 * 83.0);
    s.step(&mut m);
    assert_eq!(s.list[0].o.pos[1], 600.0);
    // then it flips and comes down
    s.step(&mut m);
    assert_eq!(s.list[0].o.pos[1], 594.0);
    for _ in 0..400 {
        s.step(&mut m);
        let y = s.list[0].o.pos[1];
        assert!((100.0..=600.0).contains(&y));
    }
}

#[test]
fn treadmill_carries_mario_by_0_084_of_its_speed() {
    let idle = ControllerSnapshot::default();
    for (speed, expect) in [(TTC_SPEED_SLOW, 4.2), (TTC_SPEED_FAST, 8.4)] {
        let mut m = mario([0.0, 0.0, 0.0]);
        // yaw 0x4000: the belt moves toward +X
        let mut s = solids(&[spec("bhvTTCTreadmill", [0.0, 0.0, 0.0], [0.0, 0x4000 as f64, 0.0], 0)], speed);
        s.install(&mut m);
        m.pos = [0.0, 0.0, 0.0];
        for _ in 0..8 {
            m.step(&idle, 0.0, false, false);
            s.step(&mut m);
        }
        let x0 = m.pos[0];
        m.step(&idle, 0.0, false, false);
        s.step(&mut m);
        assert!((m.pos[0] - x0 - expect).abs() < 1e-9, "carried {}", m.pos[0] - x0);
    }
}

#[test]
fn moving_bar_cycle_matches_the_decomp_constants() {
    let mut m = mario([0.0, -2000.0, 0.0]);
    let mut s = solids(&[spec("bhvTTCMovingBar", [0.0, 100.0, 0.0], [0.0, 0.0, 0.0], 0)], TTC_SPEED_SLOW);
    s.install(&mut m);
    // wait 55 frames, then pull back at -8 (+0.73 per frame), then launch out at 29 and oscillate at 250
    let mut max_out: f64 = 0.0;
    let mut max_back: f64 = 0.0;
    let mut first_move = None;
    for f in 0..400 {
        s.step(&mut m);
        let p = s.list[0].o.pos;
        let d = js_hypot(p[0], p[2]);
        if d > 0.0 && first_move.is_none() {
            first_move = Some(f);
        }
        // set_dist_from_home uses cos for X and sin for Z with move_yaw 0x4000 - 0: all along Z
        max_out = max_out.max(p[2].abs());
        max_back = max_back.max(-p[2]);
    }
    // the wait action ends on the frame oTimer exceeds 55 (index 56); the pull-back moves on the next
    assert_eq!(first_move, Some(57));
    // launched at 29/frame it overshoots 250 (oscillating back) before retracting
    assert!(max_out > 250.0 && max_out < 450.0, "{max_out}");
}

#[test]
fn spinner_speeds_follow_the_setting() {
    let mut m = mario([0.0, -2000.0, 0.0]);
    for (speed, expect) in [(TTC_SPEED_SLOW, 200.0), (TTC_SPEED_FAST, 600.0), (TTC_SPEED_STOPPED, 0.0)] {
        let mut s = solids(&[spec("bhvTTCSpinner", [0.0, 100.0, 0.0], [0.0; 3], 0)], speed);
        s.install(&mut m);
        s.step(&mut m);
        assert_eq!(s.list[0].o.angle_vel[0], expect);
        assert_eq!(s.list[0].o.face[0], expect);
    }
}

#[test]
fn rotating_platform_carries_mario_around_the_axis() {
    let mut m = mario([300.0, 0.0, 0.0]);
    // RR octagon: BPARAM1(angular velocity index 0) spins at 300 per frame about Y
    let mut s = solids(&[spec("bhvOctagonalPlatformRotating", [0.0, 0.0, 0.0], [0.0; 3], 0)], 0);
    s.install(&mut m);
    let idle = ControllerSnapshot::default();
    m.pos = [150.0, 0.0, 0.0];
    for _ in 0..8 {
        m.step(&idle, 0.0, false, false);
        s.step(&mut m);
    }
    let r0 = js_hypot(m.pos[0], m.pos[2]);
    let a0 = atan2s(m.pos[2], m.pos[0]) as f64;
    for _ in 0..10 {
        m.step(&idle, 0.0, false, false);
        s.step(&mut m);
    }
    // the distance from the axis is preserved and he has been swung by about 10 * 300 angle units
    assert!((js_hypot(m.pos[0], m.pos[2]) - r0).abs() < 3.0);
    let swung = to_s16(atan2s(m.pos[2], m.pos[0]) as f64 - a0).abs();
    assert!((swung as f64 - 3000.0).abs() < 400.0, "swung {swung}");
}

#[test]
fn sliding_platform_2_reads_its_path_from_bparams() {
    // bitfs MOVING_SQUARE_PLATFORM: BPARAM1(1) | BPARAM2(0x0C) -> upper half 0x010C: length 50 * 12
    let mut m = mario([0.0, -2000.0, 0.0]);
    let yaw = 90.0 * 65536.0 / 360.0;
    let mut s = solids(&[spec("bhvSlidingPlatform2", [0.0, 0.0, 0.0], [0.0, yaw, 0.0], (0x01 << 24) | (0x0C << 16))], 0);
    s.install(&mut m);
    let mut reached: f64 = 0.0;
    for _ in 0..400 {
        s.step(&mut m);
        let d = js_hypot(s.list[0].o.pos[0], s.list[0].o.pos[2]);
        reached = reached.max(d);
        assert!(d <= 600.0 + 1e-9);
    }
    assert_eq!(reached, 600.0);
}

#[test]
fn bowser_animation_table_and_tilt_data_match_the_decomp() {
    use crate::behaviors::bowser_sm64 as b;
    // assets/animations.json loopEnd of bowser_seg6_anims_06057690
    assert_eq!(b::ANIM_LOOP_END[b::ANIM_BREATH as usize], 92);
    assert_eq!(b::ANIM_LOOP_END[b::ANIM_JUMP_START as usize], 16);
    assert_eq!(b::ANIM_LOOP_END[b::ANIM_IDLE as usize], 80);
    assert!(b::anim_noloop(b::ANIM_JUMP_START) && b::anim_noloop(b::ANIM_FLIP_DOWN) && b::anim_noloop(b::ANIM_LAY_DOWN));
    // sBowsertiltPlatformData ends with the zero-time sentinel, times are increasing
    assert_eq!(b::TILT_PLATFORM_DATA[10], (-1, -160, 186));
    assert_eq!(b::TILT_PLATFORM_DATA[11].2, 0);
    // sBowserHealth: 1, 1, 3
    assert_eq!(b::HEALTH, [1.0, 1.0, 3.0]);
    assert_eq!(b::BP_BITS, 2);
}

#[test]
fn falling_bowser_platform_table_matches_the_decomp() {
    let (x, z, angle) = bowser_arena::FALLING_PLATFORM_DATA[4];
    assert_eq!((x, z, angle), (0.0, 1240.0, -6144.0));
    assert_eq!(bowser_arena::FALLING_PLATFORM_DATA[10], (-800.0, -1000.0, 31744.0));
}
