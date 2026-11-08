//! Engine-free tests of the SM64 camera port: decomp constants (behind-Mario distances, pitch
//! limits, 8-direction 45-degree snaps, radial following, parallel tracking), the exported
//! level data and the cutscene plumbing.
use super::util::*;
use super::*;
use crate::collision::{CollisionWorld, SurfaceOptions};
use crate::math::{atan2s, coss, sins};

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let s = 7000.0;
    w.add_quad([-s, 0.0, -s], [-s, 0.0, s], [s, 0.0, s], [s, 0.0, -s], &SurfaceOptions::default());
    w
}

fn env<'a>(world: &'a CollisionWorld, level: i32, area: i32, pos: V3, yaw: i32) -> Env<'a> {
    Env {
        world,
        m: MarioCam { pos, face_angle: [0, yaw, 0], action: 0x0C400201, ..Default::default() },
        pad: Pad::default(),
        level,
        area,
        act: 1,
        second_focus: None,
        dialog_active: false,
        course_num: 1,
        last_star_num: 1,
        cutscene_focus: None,
        painting: None,
    }
}

fn run(cam: &mut SmCam, e: &Env, frames: usize) {
    for _ in 0..frames {
        cam.update_camera(e);
    }
}

fn started(world: &CollisionWorld, level: i32, area: i32, pos: V3, yaw: i32) -> (SmCam, Env<'_>) {
    let e = env(world, level, area, pos, yaw);
    let mut cam = SmCam::new();
    cam.reset_camera(&e);
    cam.update_camera(&e);
    (cam, e)
}

#[test]
fn exported_level_data_matches_the_decomp() {
    // levels/level_defines.h order and camera.c tables
    assert_eq!(LEVEL_BBH, 4);
    assert_eq!(LEVEL_BOB, 9);
    assert_eq!(LEVEL_CASTLE_GROUNDS, 16);
    assert_eq!(LEVEL_TTM, 36);
    assert_eq!(level_for_course("castle_inside"), Some(LEVEL_CASTLE));
    assert_eq!(level_triggers(LEVEL_BBH).len(), 60);
    assert_eq!(level_triggers(LEVEL_CASTLE).len(), 34);
    // sCamBOB exists in camera.c but level_defines.h does not attach it to LEVEL_BOB
    assert!(level_triggers(LEVEL_BOB).is_empty());
    assert_eq!(SCAMBOB.len(), 2);
    assert_eq!(SCAMBOB[1].area, -1);
    assert_eq!(SCAMBOB[0].center, [2468, 2720, -4608]);
    assert_eq!(SCAMBOB[0].bounds, [3263, 1696, 3072]);
    assert_eq!(level_triggers(LEVEL_SL)[0].yaw, -0x1D27);
    assert!(level_triggers(LEVEL_WF).is_empty());
    // GEO_CAMERA of each area: default mode and the area centre
    let bob = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_BOB && a.1 == 1).unwrap();
    assert_eq!((bob.2, bob.3, bob.4, bob.5), (CAMERA_MODE_RADIAL, 3072.0, 0.0, -4608.0));
    let wf = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_WF && a.1 == 1).unwrap();
    assert_eq!((wf.2, wf.3, wf.4, wf.5), (CAMERA_MODE_RADIAL, 0.0, 2000.0, 0.0));
    let castle = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_CASTLE && a.1 == 1).unwrap();
    assert_eq!(castle.2, CAMERA_MODE_FIXED);
    let ddd = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_DDD && a.1 == 2).unwrap();
    assert_eq!(ddd.2, CAMERA_MODE_CLOSE);
    let rr = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_RR).unwrap();
    assert_eq!(rr.2, CAMERA_MODE_8_DIRECTIONS);
    let ssl2 = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_SSL && a.1 == 2).unwrap();
    assert_eq!(ssl2.2, CAMERA_MODE_CLOSE);
    let hmc = AREA_CAMERAS.iter().find(|a| a.0 == LEVEL_HMC).unwrap();
    assert_eq!(hmc.2, CAMERA_MODE_FREE_ROAM);
    assert_eq!(FIXED_MODE_BASE_POSITION, [646.0, 143.0, -1513.0]);
    assert_eq!(BBH_LIBRARY_PAR_TRACK[1].1, [-2118.0, 1619.0, -1490.0]);
    // DROT(0x00022240, i): BoB's dance row
    assert_eq!(DANCE_CUTSCENE_INDEX[1], [0x00, 0x20, 0x22, 0x04]);
}

#[test]
fn camera_math_helpers_match_math_util() {
    // vec3f_set_dist_and_angle / vec3f_get_dist_and_angle round trip
    let from = [100.0, 50.0, -30.0];
    let to = set_dist_and_angle(from, 800.0, 0x1000, 0x2000);
    let (d, p, y) = dist_and_angle(from, to);
    assert!((d - 800.0).abs() < 0.5);
    assert!((p - 0x1000).abs() <= 2);
    assert!((y - 0x2000).abs() <= 1);
    // camera_approach_s16_symmetric_bool never overshoots and wraps through s16
    let mut v = 0;
    assert!(approach_s16_symmetric_bool(&mut v, 1000, 300));
    assert_eq!(v, 300);
    let mut v = 10;
    assert!(!approach_s16_symmetric_bool(&mut v, 0, 100));
    assert_eq!(v, 0);
    // approach_s16_asymptotic: divisor 0 snaps
    assert_eq!(approach_s16_asymptotic(100, 0, 0), 0);
    assert_eq!(approach_s16_asymptotic(160, 0, 8), 140);
    // is_pos_in_bounds rotates the box by its yaw
    assert!(is_pos_in_bounds([0.0, 0.0, 0.0], [0.0, 0.0, 50.0], [10.0, 10.0, 100.0], 0));
    assert!(!is_pos_in_bounds([0.0, 0.0, 0.0], [0.0, 0.0, 50.0], [10.0, 10.0, 40.0], 0));
    assert_eq!(degrees(45), 0x2000);
    assert_eq!(degrees(-105), -0x4AAA);
}

#[test]
fn behind_mario_camera_keeps_max_800_and_faces_mario_back() {
    // update_behind_mario_camera: dist capped at maxDist 800 (350 in the Mario angle), yaw eases to Mario's back.
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0x4000);
    cam.set_camera_mode(&e, CAMERA_MODE_BEHIND_MARIO, 1);
    cam.c.pos = [-3000.0, 200.0, 0.0];
    run(&mut cam, &e, 140);
    let m = e.m.pos;
    let (dist, _p, yaw) = dist_and_angle([m[0], m[1] + 125.0, m[2]], cam.lakitu.goal_pos);
    assert!(dist <= 800.0 + 1.0, "dist {dist}");
    assert!(dist >= 300.0);
    // Mario faces +X (yaw 0x4000): the camera sits at -X, i.e. yaw 0xC000
    assert!((w_(yaw - 0xC000)).abs() < 0x600, "yaw {yaw:#x}");
}

fn w_(v: i32) -> i32 { super::util::w(v) }

#[test]
fn behind_mario_c_buttons_pitch_and_distance() {
    // D-C goal pitch -0x3000, U-C +0x3000 (update_behind_mario_camera)
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_BOB, 1, [0.0, 800.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_BEHIND_MARIO, 1);
    e.pad.down = U_CBUTTONS;
    e.pad.pressed = U_CBUTTONS;
    cam.update_camera(&e);
    e.pad.pressed = 0;
    run(&mut cam, &e, 20);
    assert_eq!(cam.behind_mario_sound_timer.min(30), cam.behind_mario_sound_timer);
    let m = e.m.pos;
    let (_d, pitch, _y) = dist_and_angle([m[0], m[1] + 125.0, m[2]], cam.c.pos);
    assert!(pitch > 0x800, "looking up from below: pitch {pitch:#x}");
}

#[test]
fn eight_direction_camera_snaps_in_45_degree_steps() {
    // mode_8_directions_camera: R-C adds DEGREES(45), L-C subtracts it from s8DirModeYawOffset.
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_RR, 1, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_8_DIRECTIONS, 1);
    cam.dir8_base_yaw = 0;
    cam.dir8_yaw_offset = 0;
    cam.update_camera(&e);
    assert_eq!(cam.area_yaw, 0);
    e.pad.pressed = R_CBUTTONS;
    e.pad.down = R_CBUTTONS;
    cam.update_camera(&e);
    e.pad.pressed = 0;
    assert_eq!(cam.dir8_yaw_offset, 0x2000);
    assert_eq!(cam.area_yaw, 0x2000);
    for k in 2..=8 {
        e.pad.pressed = R_CBUTTONS;
        cam.update_camera(&e);
        e.pad.pressed = 0;
        assert_eq!(cam.dir8_yaw_offset, w_(0x2000 * k));
    }
    // eight presses is a full turn
    assert_eq!(cam.dir8_yaw_offset, 0);
    e.pad.pressed = L_CBUTTONS;
    cam.update_camera(&e);
    assert_eq!(cam.dir8_yaw_offset, -0x2000);
    // the camera's yaw towards Mario follows area yaw: pos = mario + (1000 + lakitu dist) back along areaYaw
    e.pad.pressed = 0;
    run(&mut cam, &e, 40);
    assert_eq!(cam.c.next_yaw, -0x2000);
}

#[test]
fn radial_camera_looks_from_the_far_side_of_the_area_centre() {
    // update_radial_camera: camera yaw = atan2s(mario - areaCenter) + sModeOffsetYaw, 1000 + lakituDist away.
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_BOB, 1, [3072.0 + 2000.0, 0.0, -4608.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_RADIAL, 1);
    run(&mut cam, &e, 60);
    assert_eq!(cam.c.area_cen[0], 3072.0);
    let expect = atan2s(0.0, 2000.0);
    assert_eq!(cam.area_yaw, expect);
    assert_eq!(cam.c.next_yaw, w_(expect + cam.mode_offset_yaw));
    // camera is 1000 (+ zoom) from Mario horizontally, directly outward from the centre
    let m = e.m.pos;
    let hd = hor_dist(m, cam.c.pos);
    assert!((hd - 1000.0).abs() < 60.0, "hor dist {hd}");
    assert!(cam.c.pos[0] > m[0]);
}

#[test]
fn radial_c_buttons_rotate_by_up_to_sixty_degrees() {
    // radial_camera_move: CAM_MOVE_ROTATE_RIGHT drives sModeOffsetYaw to DEGREES(60)
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_TTC, 1, [2000.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_OUTWARD_RADIAL, 1);
    run(&mut cam, &e, 5);
    e.pad.pressed = R_CBUTTONS;
    cam.update_camera(&e);
    e.pad.pressed = 0;
    run(&mut cam, &e, 30);
    assert_eq!(cam.mode_offset_yaw, degrees(60));
}

#[test]
fn zoom_out_with_c_down_adds_400() {
    // update_default_camera: zoomDist = gCameraZoomDist (800) + 400 when CAM_MOVE_ZOOMED_OUT
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_SA, 1, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_CLOSE, 1);
    run(&mut cam, &e, 120);
    let m = e.m.pos;
    let near = abs_dist(m, cam.c.pos);
    e.pad.pressed = D_CBUTTONS;
    cam.update_camera(&e);
    e.pad.pressed = 0;
    assert!(cam.movement_flags & CAM_MOVE_ZOOMED_OUT != 0);
    run(&mut cam, &e, 240);
    let far = abs_dist(m, cam.c.pos);
    assert!(far > near + 150.0, "near {near} far {far}");
    assert!(far <= 1200.0 + 130.0, "far {far}");
    // U-C leaves the zoom
    e.pad.pressed = U_CBUTTONS;
    cam.update_camera(&e);
    assert!(cam.movement_flags & CAM_MOVE_ZOOMED_OUT == 0);
}

#[test]
fn lakitu_pitch_is_limited_to_0x3e00() {
    // update_lakitu ends with clamp_pitch(pos, focus, 0x3E00, -0x3E00)
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_CLOSE, 1);
    cam.c.pos = [0.0, 6000.0, 1.0];
    cam.c.focus = [0.0, 0.0, 0.0];
    cam.lakitu.cur_pos = cam.c.pos;
    cam.update_lakitu(&e);
    let (_d, pitch, _y) = dist_and_angle(cam.lakitu.pos, cam.lakitu.focus);
    assert!(pitch >= -0x3E00 - 8 && pitch <= 0x3E00 + 8, "{pitch:#x}");
}

#[test]
fn parallel_tracking_follows_mario_along_the_path() {
    // update_parallel_tracking_camera: camera stays on the path's side, sliding along it with Mario
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_SA, 1, [-1500.0, 1619.0, -1490.0], 0);
    // a path along X at z = 500 (the camera looks along +Z), segment 1 pos..pos
    let path = vec![
        ParPoint { start_of_path: 1, pos: [-3000.0, 1600.0, 500.0], dist_thresh: 50.0, zoom: 0.0 },
        ParPoint { start_of_path: 0, pos: [3000.0, 1600.0, 500.0], dist_thresh: 50.0, zoom: 0.0 },
        ParPoint { start_of_path: 0, pos: [0.0, 0.0, 0.0], dist_thresh: 0.0, zoom: 0.0 },
    ];
    e.m.pos = [-1500.0, 1619.0, -1490.0];
    cam.c.mode = CAMERA_MODE_CLOSE;
    cam.parallel_tracking_init(path);
    cam.lakitu.mode = cam.c.mode;
    assert_eq!(cam.c.mode, CAMERA_MODE_PARALLEL_TRACKING);
    assert_eq!(cam.c.pos, [0.0, 1600.0, 500.0]);
    let mut last_x = f64::NEG_INFINITY;
    for step in 0..6 {
        e.m.pos[0] = -1500.0 + 600.0 * step as f64;
        run(&mut cam, &e, 12);
        assert_eq!(cam.c.mode, CAMERA_MODE_PARALLEL_TRACKING);
        let cx = cam.lakitu.goal_pos[0];
        // zoom 0: the camera x follows Mario's x (distThresh 50 keeps it within 50 of him)
        assert!((cx - e.m.pos[0]).abs() < 60.0, "cam x {cx} mario x {}", e.m.pos[0]);
        assert!(cx > last_x);
        last_x = cx;
        // always on the z = 500 side of the path
        assert!((cam.lakitu.goal_pos[2] - 500.0).abs() < 1.0);
    }
}

#[test]
fn fixed_camera_sits_at_the_base_position_and_pans_to_mario() {
    // update_fixed_camera in the SA / castle lobby layout: camera x/z = base + (mario - base) * scaleToMario
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_CASTLE, 1, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode_fixed(-577, 143, 1443);
    assert_eq!(cam.c.mode, CAMERA_MODE_FIXED);
    e.m.pos = [100.0, 0.0, 600.0];
    run(&mut cam, &e, 3);
    let base = [-577.0, 143.0, 1443.0];
    let k = 0.3; // AREA_CASTLE_LOBBY scaleToMario
    assert!((cam.c.pos[0] - (base[0] + (100.0 - base[0]) * k)).abs() < 1.0);
    assert!((cam.c.pos[2] - (base[2] + (600.0 - base[2]) * k)).abs() < 1.0);
    // the fixed mode ignores the left / right C buttons (buzz only)
    e.pad.pressed = L_CBUTTONS;
    cam.update_camera(&e);
    assert!(cam.sounds.contains(&CamSound::Buzz));
}

#[test]
fn surface_types_choose_modes() {
    // camera_course_processing: SURFACE_CLOSE_CAMERA -> CAMERA_MODE_CLOSE, FREE_ROAM -> FREE_ROAM
    let mut w = CollisionWorld::default();
    let s = 7000.0;
    w.add_quad([-s, 0.0, -s], [-s, 0.0, s], [0.0, 0.0, s], [0.0, 0.0, -s], &SurfaceOptions::default());
    w.add_quad([0.0, 0.0, -s], [0.0, 0.0, s], [s, 0.0, s], [s, 0.0, -s], &SurfaceOptions { kind: SURFACE_CAMERA_FREE_ROAM, ..Default::default() });
    let (mut cam, mut e) = started(&w, LEVEL_BOB, 1, [-2000.0, 0.0, 0.0], 0);
    run(&mut cam, &e, 3);
    assert_eq!(cam.c.mode, CAMERA_MODE_RADIAL, "BoB's default area camera");
    e.m.pos = [2000.0, 0.0, 0.0];
    run(&mut cam, &e, 3);
    assert_eq!(cam.c.mode, CAMERA_MODE_FREE_ROAM);
}

#[test]
fn bob_summit_floor_selects_the_boss_fight_camera() {
    // camera_course_processing AREA_BOB: SURFACE_BOSS_FIGHT_CAMERA -> set_camera_mode_boss_fight, else radial
    let mut w = CollisionWorld::default();
    let s = 7000.0;
    w.add_quad([-s, 0.0, -s], [-s, 0.0, s], [0.0, 0.0, s], [0.0, 0.0, -s], &SurfaceOptions::default());
    w.add_quad([0.0, 0.0, -s], [0.0, 0.0, s], [s, 0.0, s], [s, 0.0, -s], &SurfaceOptions { kind: SURFACE_BOSS_FIGHT_CAMERA, ..Default::default() });
    let (mut cam, mut e) = started(&w, LEVEL_BOB, 1, [-2000.0, 0.0, 0.0], 0);
    run(&mut cam, &e, 3);
    assert_eq!(cam.c.mode, CAMERA_MODE_RADIAL);
    e.m.pos = [2000.0, 0.0, 0.0];
    run(&mut cam, &e, 3);
    assert_eq!(cam.c.mode, CAMERA_MODE_BOSS_FIGHT);
    e.m.pos = [-2000.0, 0.0, 0.0];
    run(&mut cam, &e, 3);
    assert_eq!(cam.c.mode, CAMERA_MODE_RADIAL);
}

#[test]
fn ccm_slide_shortcut_pulls_the_camera_in() {
    // update_slide_camera: lakitu dist approaches -600 inside the shortcut trigger
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_CCM, 2, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_SLIDE_HOOT, 1);
    cam.status_flags |= CAM_FLAG_CCM_SLIDE_SHORTCUT;
    run(&mut cam, &e, 5);
    assert_eq!(cam.lakitu_dist, -100);
    run(&mut cam, &e, 40);
    assert_eq!(cam.lakitu_dist, -600);
    cam.status_flags &= !CAM_FLAG_CCM_SLIDE_SHORTCUT;
    run(&mut cam, &e, 40);
    assert_eq!(cam.lakitu_dist, 0);
}

#[test]
fn slide_camera_trails_behind_mario_at_800() {
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_CCM, 2, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_SLIDE_HOOT, 1);
    // the slide area starts the camera in front of Mario (init_camera marioOffset z = -300); it swings behind at 0x80 per tick
    run(&mut cam, &e, 400);
    let m = e.m.pos;
    let foc = [m[0], m[1] + 50.0, m[2]];
    let (d, _p, yaw) = dist_and_angle(foc, cam.c.pos);
    assert!(d <= 800.0 + 2.0, "dist {d}");
    // Mario faces +Z (yaw 0): the camera is behind him at yaw 0x8000
    assert!(w_(yaw - 0x8000).abs() < 0x400, "yaw {yaw:#x}");
}

#[test]
fn boss_fight_camera_frames_mario_and_the_boss() {
    // update_boss_fight_camera: focus at the midpoint (+100 y), distance >= 800 (1.6x separation)
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_SA, 1, [0.0, 0.0, 0.0], 0);
    e.second_focus = Some(SecondFocus { pos: [3000.0, 0.0, 0.0], held_state: 0 });
    cam.set_camera_mode(&e, CAMERA_MODE_BOSS_FIGHT, 1);
    run(&mut cam, &e, 60);
    let mid = [1500.0, 100.0, 0.0];
    assert!(abs_dist(cam.c.focus, mid) < 5.0, "focus {:?}", cam.c.focus);
    let (d, _p, _y) = dist_and_angle(cam.c.focus, cam.c.pos);
    assert!(d > 1000.0, "dist {d}");
}

#[test]
fn water_surface_camera_hugs_the_surface() {
    // mode_behind_mario with CAMERA_MODE_WATER_SURFACE: height goal = waterLevel + 120
    let mut w = flat_world();
    w.water_boxes.push(crate::collision::WaterBox(1.0, -7000.0, -7000.0, 7000.0, 7000.0, 500.0));
    let (mut cam, mut e) = started(&w, LEVEL_JRB, 1, [0.0, 0.0, 0.0], 0);
    e.m.water_level = 500.0;
    e.m.action = act::FLAG_SWIMMING | 0x300;
    cam.set_camera_mode(&e, CAMERA_MODE_WATER_SURFACE, 1);
    run(&mut cam, &e, 120);
    assert!((cam.c.pos[1] - 620.0).abs() < 1.0, "cam y {}", cam.c.pos[1]);
}

#[test]
fn mode_transition_flies_between_poses() {
    // transition_to_camera_mode + next_lakitu_state: framesLeft counts down from the requested frames
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    run(&mut cam, &e, 10);
    cam.transition_to_camera_mode(CAMERA_MODE_OUTWARD_RADIAL, 90);
    assert_eq!(cam.mode_transition.frames_left, 90);
    cam.update_camera(&e);
    assert_eq!(cam.mode_transition.frames_left, 89);
    run(&mut cam, &e, 100);
    assert_eq!(cam.mode_transition.frames_left, 0);
}

#[test]
fn c_up_enters_and_exits_with_head_rotation() {
    // set_mode_c_up / mode_c_up_camera: stick moves Mario's head (3/4 of the camera offset)
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_CCM, 1, [0.0, 0.0, 0.0], 0);
    cam.set_camera_mode(&e, CAMERA_MODE_C_UP, 0x10);
    run(&mut cam, &e, 0x12);
    e.pad.stick_y = 20.0;
    run(&mut cam, &e, 3);
    assert_eq!(cam.c_up_camera_pitch, 600);
    assert_eq!(cam.head_rotation[0], 450);
    e.pad.stick_y = 0.0;
    e.pad.pressed = A_BUTTON;
    cam.update_camera(&e);
    assert!(cam.movement_flags & CAM_MOVE_STARTED_EXITING_C_UP != 0 || cam.movement_flags & CAM_MOVE_C_UP_MODE == 0);
}

#[test]
fn camera_shakes_use_the_decomp_constants() {
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    cam.set_camera_shake_from_hit(SHAKE_GROUND_POUND);
    assert_eq!(cam.lakitu.shake_magnitude[0], 0x60);
    assert_eq!(cam.lakitu.shake_pitch_decay, 0xC);
    assert_eq!(cam.lakitu.shake_pitch_vel, 0x8000);
    cam.set_environmental_camera_shake(SHAKE_ENV_BOWSER_JUMP);
    assert_eq!(cam.lakitu.shake_magnitude[0], 0x100);
    cam.set_camera_shake_from_hit(SHAKE_SMALL_DAMAGE);
    assert_eq!(cam.lakitu.shake_magnitude[1], 0x80);
    assert_eq!(cam.lakitu.shake_magnitude[2], 0x80);
    assert_eq!(cam.fov.shake_amplitude, 256.0);
    run(&mut cam, &e, 200);
    assert_eq!(cam.lakitu.shake_magnitude, [0, 0, 0]);
}

#[test]
fn fov_functions_use_the_decomp_targets() {
    let w = flat_world();
    let (mut cam, _e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    cam.set_fov_function(CAM_FOV_SET_30);
    assert_eq!(cam.step_fov(), 30.0);
    cam.set_fov_function(CAM_FOV_SET_29);
    assert_eq!(cam.step_fov(), 29.0);
    cam.set_fov_function(CAM_FOV_APP_80);
    for _ in 0..40 {
        cam.step_fov();
    }
    assert_eq!(cam.fov.fov, 80.0);
    cam.set_fov_function(CAM_FOV_DEFAULT);
    for _ in 0..400 {
        cam.step_fov();
    }
    assert!((cam.fov.fov - 45.0).abs() < 0.01);
    // BBH fixed rooms widen to 60
    cam.c.mode = CAMERA_MODE_FIXED;
    cam.set_fov_function(CAM_FOV_BBH);
    for _ in 0..20 {
        cam.step_fov();
    }
    assert_eq!(cam.fov.fov, 60.0);
}

#[test]
fn door_cutscene_runs_the_decomp_shot_sequence() {
    // sCutsceneDoorPull: start (1), fix cam (30), move behind Mario (1), follow (50), end
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_CCM, 1, [0.0, 0.0, 0.0], 0);
    run(&mut cam, &e, 60);
    e.m.action = act::PULLING_DOOR;
    cam.camera_event = CAM_EVENT_DOOR;
    cam.update_camera(&e);
    assert_eq!(cam.c.cutscene, cutscenes::CUTSCENE_DOOR_PULL);
    // 1 + 30 + 1 + 50 = 82 frames; then door_end clears it
    let mut frames = 0;
    while cam.c.cutscene != 0 && frames < 200 {
        cam.update_camera(&e);
        frames += 1;
    }
    assert!((80..=84).contains(&frames), "{frames}");
    assert!(cam.status_flags & CAM_FLAG_BEHIND_MARIO_POST_DOOR != 0 || cam.recent_cutscene == cutscenes::CUTSCENE_DOOR_PULL);
    assert_eq!(cam.c.mode, CAMERA_MODE_CLOSE);
}

#[test]
fn star_dance_cutscene_is_picked_by_course_and_star() {
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    // BoB (course 1) star 1 -> index 0 -> fly away; star 3 (idx 2) -> 2 -> closeup? table [0x00,0x20,0x22,0x04]
    e.course_num = 1;
    e.last_star_num = 1;
    assert_eq!(cam.determine_dance_cutscene(&e), cutscenes::CUTSCENE_DANCE_FLY_AWAY);
    e.last_star_num = 4;
    assert_eq!(cam.determine_dance_cutscene(&e), cutscenes::CUTSCENE_DANCE_CLOSEUP);
    e.last_star_num = 7;
    assert_eq!(cam.determine_dance_cutscene(&e), cutscenes::CUTSCENE_DANCE_DEFAULT);
    // bowser keys use the key dance
    e.course_num = 16;
    e.last_star_num = 1;
    assert_eq!(cam.determine_dance_cutscene(&e), cutscenes::CUTSCENE_KEY_DANCE);
    // the default dance ends when Mario's star-dance action ends
    e.m.action = act::STAR_DANCE_NO_EXIT;
    cam.update_camera(&e);
    assert_eq!(cam.c.cutscene, cutscenes::CUTSCENE_DANCE_DEFAULT);
    run(&mut cam, &e, 30);
    assert_eq!(cam.c.cutscene, cutscenes::CUTSCENE_DANCE_DEFAULT);
    e.m.action = 0x0C400201;
    run(&mut cam, &e, 2);
    assert_eq!(cam.c.cutscene, 0);
}

#[test]
fn unsupported_cutscenes_end_immediately() {
    let w = flat_world();
    let (mut cam, e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    cam.start_object_cutscene_without_focus(cutscenes::CUTSCENE_DIALOG);
    run(&mut cam, &e, 3);
    assert_eq!(cam.c.cutscene, 0);
    assert!(!SmCam::cutscene_supported(cutscenes::CUTSCENE_DIALOG));
    assert!(SmCam::cutscene_supported(cutscenes::CUTSCENE_STAR_SPAWN));
}

#[test]
fn cannon_cutscene_hands_over_to_the_inside_cannon_camera() {
    // sCutsceneEnterCannon: 1 + 121 frames, then CAMERA_MODE_INSIDE_CANNON with sCannonYOffset = 800
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0);
    e.m.action = act::IN_CANNON;
    e.m.used_obj = Some(UsedObj { pos: [0.0, 0.0, 0.0], hitbox_height: 0.0, move_angle: [0, 0, 0] });
    cam.camera_event = CAM_EVENT_CANNON;
    cam.update_camera(&e);
    assert_eq!(cam.c.cutscene, cutscenes::CUTSCENE_ENTER_CANNON);
    run(&mut cam, &e, 125);
    assert_eq!(cam.c.cutscene, 0);
    assert_eq!(cam.c.mode, CAMERA_MODE_INSIDE_CANNON);
    assert!(cam.cannon_y_offset <= 800.0 && cam.cannon_y_offset >= 0.0 && cam.cannon_y_offset % 100.0 == 0.0);
}

#[test]
fn inside_cannon_camera_sits_at_mario_looking_down_the_barrel() {
    // update_in_cannon: focus_on_mario with swapped arguments puts c->pos at Mario (+125) and c->focus 800 ahead
    let w = flat_world();
    let (mut cam, mut e) = started(&w, LEVEL_BOB, 1, [0.0, 0.0, 0.0], 0x4000);
    e.m.face_angle = [0x1000, 0x4000, 0];
    cam.set_camera_mode(&e, CAMERA_MODE_INSIDE_CANNON, 1);
    cam.cannon_y_offset = 0.0;
    run(&mut cam, &e, 3);
    assert!((cam.c.pos[1] - 125.0).abs() < 1.0);
    let expect = 800.0 * coss(0x1000 as f64) * sins(0x4000 as f64);
    assert!((cam.c.focus[0] - expect).abs() < 1.0, "{} vs {expect}", cam.c.focus[0]);
}
