//! OoT-parity checks for Link's movement on synthetic collision worlds (the same `MarioRuntime` and
//! `step_ground` / `step_air` code every SM64 course runs). The reference numbers come from
//! `z_player.c` and are converted exactly like the rest of the Link port: 1 OoT frame = 1.5 ticks,
//! 1 OoT unit = 4.4 world units, so speeds scale by 4.4 / 1.5 and accelerations by 4.4 / 1.5^2.
//! See `native/LINK_PARITY.md` for the table of what each test pins.
use jumpman_sim::collision::*;
use jumpman_sim::link::combat::{accel, len, speed};
use jumpman_sim::link::items::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use jumpman_sim::math::Vec3;
use std::sync::Arc;

fn tri(w: &mut CollisionWorld, a: Vec3, b: Vec3, c: Vec3) {
    // Wound so the normal points up for a floor seen from above.
    let o = SurfaceOptions::default();
    let n = (b[1] - a[1]) * (c[2] - b[2]) - (b[2] - a[2]) * (c[1] - b[1]);
    let _ = n;
    w.add_triangle(a, b, c, &o);
}

fn quad(w: &mut CollisionWorld, x0: f64, x1: f64, z0: f64, z1: f64, y: f64) {
    tri(w, [x0, y, z0], [x0, y, z1], [x1, y, z1]);
    tri(w, [x0, y, z0], [x1, y, z1], [x1, y, z0]);
}

fn flat() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    quad(&mut w, -8000.0, 8000.0, -8000.0, 8000.0, 0.0);
    w
}

/// A plateau at y = 0 for z < 2000, then the ground `drop` units lower.
fn ledge(drop: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    quad(&mut w, -8000.0, 8000.0, -8000.0, 2000.0, 0.0);
    quad(&mut w, -8000.0, 8000.0, 2000.0, 8000.0, -drop);
    // the cliff face so walls exist
    let o = SurfaceOptions::default();
    w.add_triangle([-8000.0, 0.0, 2000.0], [-8000.0, -drop, 2000.0], [8000.0, -drop, 2000.0], &o);
    w.add_triangle([-8000.0, 0.0, 2000.0], [8000.0, -drop, 2000.0], [8000.0, 0.0, 2000.0], &o);
    w
}

fn link_in(world: CollisionWorld, pos: Vec3) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos, face_yaw: 0.0 }, -20000.0);
    m.set_objects(ObjectManager::new(vec![], None, None));
    m.link.enable(Age::Adult);
    m
}

#[derive(Default, Clone)]
struct In {
    x: f64,
    y: f64,
    down: u32,
    pressed: u32,
    focus: Option<f64>,
}

fn tick(m: &mut MarioRuntime, i: &In) {
    let mag = (i.x * i.x + i.y * i.y).sqrt().min(64.0);
    let c = ControllerSnapshot { focus_yaw: i.focus, stick_x: i.x, stick_y: i.y, stick_mag: mag, button_down: i.down | i.pressed, button_pressed: i.pressed };
    m.step(&c, 32768.0, true, false);
}

fn settle(m: &mut MarioRuntime) {
    for _ in 0..6 {
        tick(m, &In::default());
    }
}

fn hdist(a: Vec3, b: Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Tick with full stick forward (screen up = the way Link faces after the first tick) until `n` ticks passed.
fn run_forward(m: &mut MarioRuntime, n: usize) {
    for _ in 0..n {
        tick(m, &In { y: 64.0, ..Default::default() });
    }
}

// ---- OoT truth, in port units ----

/// `R_RUN_SPEED_LIMIT` 600 (Kokiri boots): 6 units per frame.
fn oot_run_cap() -> f64 {
    speed(6.0)
}

// ---- the decomp, stepped at 20 Hz in OoT units ----

/// `Actor_UpdateVelocityXZGravity` then `Actor_UpdatePos`: (apex height, frames in the air) of a jump.
fn oot_arc(v0: f64, g: f64) -> (f64, f64) {
    let (mut y, mut vy, mut apex, mut frames) = (0.0f64, v0, 0.0f64, 0.0);
    loop {
        vy = (vy - g).max(-20.0);
        y += vy;
        frames += 1.0;
        apex = apex.max(y);
        if y <= 0.0 || frames > 200.0 {
            return (apex, frames);
        }
    }
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= b.abs() * tol
}

fn heading(m: &MarioRuntime) -> f64 {
    m.face_angle[1]
}

#[test]
fn full_stick_runs_at_the_oot_cap_not_mario_speed() {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    settle(&mut m);
    run_forward(&mut m, 30);
    // R_RUN_SPEED_LIMIT 600 = 6.0 units per frame (z_player_lib.c sBootData), 4.4 / 1.5 per tick
    assert!((m.forward_vel - oot_run_cap()).abs() < 1e-6, "{}", m.forward_vel);
    let p = m.pos;
    run_forward(&mut m, 30);
    let per_frame = hdist(p, m.pos) / 30.0 * 1.5 / 4.4;
    assert!((per_frame - 6.0).abs() < 0.01, "{per_frame} OoT units per frame");
}

#[test]
fn walking_speed_follows_the_curved_stick_map() {
    // Player_CalcSpeedAndYawFromControlStick: ((1 - cos((mag - 20) * 450))^2 * 30 + 7) * 0.14
    for mag in [32.0f64, 40.0, 50.0] {
        let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
        settle(&mut m);
        for _ in 0..40 {
            tick(&mut m, &In { y: mag, ..Default::default() });
        }
        let c = 1.0 - (((mag - 20.0) * 450.0) / 65536.0 * std::f64::consts::TAU).cos();
        let exact = speed((c * c * 30.0 + 7.0) * 0.14);
        assert!((m.forward_vel - exact).abs() < 0.05, "mag {mag}: {} vs {exact}", m.forward_vel);
    }
}

#[test]
fn releasing_the_stick_stops_in_the_oot_time() {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    settle(&mut m);
    run_forward(&mut m, 30);
    // 1.5 units per frame^2 off 6.0: four frames = six ticks
    let mut n = 0;
    while m.forward_vel > 0.0 && n < 30 {
        tick(&mut m, &In::default());
        n += 1;
    }
    assert!((5..=7).contains(&n), "{n} ticks to stop");
}

#[test]
fn a_side_stick_turns_at_2000_per_frame_and_a_stick_pushed_back_stops_him() {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    settle(&mut m);
    run_forward(&mut m, 30);
    let h0 = heading(&m);
    tick(&mut m, &In { x: 64.0, y: 0.0, ..Default::default() });
    let turned = (jumpman_sim::math::to_s16(heading(&m) - h0) as f64).abs();
    assert!(turned <= 2000.0 / 1.5 + 1.0, "turned {turned} in one tick, REG(27) is 2000 per frame");
    assert!(turned > 600.0);
    // the stick straight back: func_8083C484
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    settle(&mut m);
    run_forward(&mut m, 30);
    tick(&mut m, &In { y: -64.0, ..Default::default() });
    assert!(m.forward_vel <= accel(2.0) + 0.01, "stopped dead and restarted from 0, not a U-turn at speed: {}", m.forward_vel);
}

#[test]
fn the_roll_is_one_and_a_half_times_the_walk_speed_for_sixteen_ticks() {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    settle(&mut m);
    run_forward(&mut m, 30);
    let p0 = m.pos;
    tick(&mut m, &In { y: 64.0, pressed: A_BUTTON, ..Default::default() });
    let (mut n, mut top, mut invinc_from) = (1, 0.0f64, None);
    while m.action == ACT_WALKING && m.action_arg == 1.0 && n < 60 {
        top = top.max(m.forward_vel);
        if m.invinc_timer > 0.0 && invinc_from.is_none() {
            invinc_from = Some(n);
        }
        tick(&mut m, &In { y: 64.0, ..Default::default() });
        n += 1;
    }
    assert_eq!(n, 16, "anim frame 20 at 1.875 frames per game frame = 10.7 frames");
    assert!((top - speed(9.0)).abs() < 0.2, "roll tops out at 1.5 x 6.0: {top}");
    let d = hdist(p0, m.pos);
    assert!(near(d, 410.0, 0.15), "roll travelled {d} units, the decomp's 9 units per frame over ~10.7 frames is ~410");
    assert!(invinc_from.is_some_and(|t| (6..=8).contains(&t)), "invulnerable from animation frame 8: {invinc_from:?}");
}

fn hop(ix: f64, iy: f64) -> (f64, f64, f64) {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    settle(&mut m);
    let p0 = m.pos;
    let face = m.face_angle[1];
    let (mut apex, mut ticks) = (0.0f64, 0.0);
    tick(&mut m, &In { x: ix, y: iy, pressed: A_BUTTON, focus: Some(face), ..Default::default() });
    while m.action & ACT_FLAG_AIR != 0 && ticks < 120.0 {
        apex = apex.max(m.pos[1]);
        tick(&mut m, &In { x: ix, y: iy, focus: Some(face), ..Default::default() });
        ticks += 1.0;
    }
    (apex, ticks, hdist(p0, m.pos))
}

#[test]
fn backflip_and_side_hops_have_the_oot_arc_while_z_targeting() {
    // func_8083BCD0: backflip speed 6.0, vy 5.8; side hop speed 8.5, vy 3.5; Z-target gravity -1.2
    for (name, (ix, iy), v0, vxz) in [("backflip", (0.0, -64.0), 5.8, 6.0), ("hop left", (-64.0, 0.0), 3.5, 8.5), ("hop right", (64.0, 0.0), 3.5, 8.5)] {
        let (apex, ticks, dist) = hop(ix, iy);
        let (oot_apex, frames) = oot_arc(v0, 1.2);
        assert!(near(apex, oot_apex * 4.4, 0.25), "{name}: apex {apex} vs OoT {}", oot_apex * 4.4);
        assert!(near(ticks, frames * 1.5, 0.25), "{name}: {ticks} ticks vs {}", frames * 1.5);
        assert!(near(dist, frames * vxz * 4.4, 0.25), "{name}: {dist} units vs {}", frames * vxz * 4.4);
    }
}

#[test]
fn z_forward_jump_is_the_jump_slash_take_off() {
    let (apex, _, dist) = hop(0.0, 64.0);
    let (oot_apex, frames) = oot_arc(5.0, 1.2);
    assert!(near(apex, oot_apex * 4.4, 0.25), "apex {apex}");
    assert!(near(dist, frames * 5.0 * 4.4, 0.3), "dist {dist}");
}

fn ledge_run(drop: f64, stick: f64) -> (Option<f64>, f64, f64) {
    let mut m = link_in(ledge(drop), [0.0, 0.0, if stick < 40.0 { 1900.0 } else { 1000.0 }]);
    settle(&mut m);
    let (mut apex, mut left) = (0.0f64, None);
    for _ in 0..300 {
        tick(&mut m, &In { y: stick, ..Default::default() });
        if m.action & ACT_FLAG_AIR != 0 && left.is_none() {
            left = Some(m.pos[2]);
        }
        apex = apex.max(m.pos[1]);
        if left.is_some() && m.action & ACT_FLAG_AIR == 0 {
            break;
        }
    }
    (left, apex, m.pos[2] - left.unwrap_or(0.0))
}

#[test]
fn running_off_a_ledge_jumps_like_func_8083a4a8() {
    let (left, apex, dist) = ledge_run(900.0, 64.0);
    assert!(left.is_some());
    // vy 7.5 at the run cap, gravity 1.0: 28 units up (x 4.4)
    let (oot_apex, frames) = oot_arc(7.5, 1.0);
    assert!(near(apex, oot_apex * 4.4, 0.25), "apex {apex} vs {}", oot_apex * 4.4);
    assert!(apex < 160.0, "Mario's 38-per-tick auto-jump reached 200");
    // 6.0 units per frame the whole way down (a 900 drop is 205 OoT units: more airtime than the arc)
    assert!(dist > frames * 6.0 * 4.4 * 0.9, "{dist}");
}

#[test]
fn a_slow_walk_off_a_ledge_does_not_jump_and_a_small_drop_is_walked_down() {
    let (left, apex, _) = ledge_run(900.0, 32.0);
    assert!(left.is_some());
    assert!(apex < 1.0, "walking speed 3.09 < 3.0 x 4.4/1.5: no auto jump, apex {apex}");
    let (left, apex, _) = ledge_run(60.0, 64.0);
    assert!(left.is_none() && apex < 1.0, "a 60 unit step is stepped down, not jumped");
}

#[test]
fn falling_is_capped_at_minus_20_units_per_frame() {
    let mut m = link_in(ledge(60000.0), [0.0, 0.0, 1900.0]);
    settle(&mut m);
    let mut low = 0.0f64;
    for _ in 0..200 {
        tick(&mut m, &In { y: 32.0, ..Default::default() });
        low = low.min(m.vel[1]);
    }
    assert!((low + speed(20.0)).abs() < 1e-6, "terminal {low}");
}

fn drop_hp(drop: f64, stick: f64) -> (i32, f64, f64) {
    let mut m = link_in(ledge(drop), [0.0, 0.0, 1900.0]);
    settle(&mut m);
    let hp0 = m.link.inv.health;
    let mut t = 0;
    let mut left = false;
    while t < 400 {
        // walk to the edge slowly, then (maybe) push the stick forward
        let y = if m.action & ACT_FLAG_AIR != 0 { stick } else { 32.0 };
        tick(&mut m, &In { y, ..Default::default() });
        if m.action & ACT_FLAG_AIR != 0 {
            left = true;
        }
        if left && m.action & ACT_FLAG_AIR == 0 {
            break;
        }
        t += 1;
    }
    (hp0 - m.link.inv.health, m.link.phys.stun, if m.action == ACT_WALKING && m.action_arg == 1.0 { 1.0 } else { 0.0 })
}

#[test]
fn hard_landings_cost_half_a_heart_then_a_heart_and_stagger() {
    // func_80843E64 / D_80854600: 400 units -> -8, 800 units -> -16 (x 4.4 world units)
    assert_eq!(drop_hp(1000.0, 0.0).0, 0, "227 OoT units: a crouch, no damage");
    let (hp, stun, _) = drop_hp(2000.0, 0.0);
    assert_eq!(hp, 8, "455 OoT units");
    assert!(stun > 0.0, "he staggers for 10 frames");
    assert_eq!(drop_hp(4000.0, 0.0).0, 16, "909 OoT units");
}

#[test]
fn a_stick_forward_turns_a_fall_under_800_into_a_roll_without_damage() {
    let (hp, _, rolling) = drop_hp(2000.0, 64.0);
    assert_eq!(hp, 0);
    assert_eq!(rolling, 1.0);
    // but 800 units or more still hurts
    assert_eq!(drop_hp(4000.0, 64.0).0, 16);
}

fn ramp(angle_deg: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let (rise, run) = (angle_deg.to_radians().sin(), angle_deg.to_radians().cos());
    let len = 4000.0;
    quad(&mut w, -8000.0, 8000.0, -8000.0, 0.0, 0.0);
    // rises along +z from z = 0
    tri(&mut w, [-8000.0, 0.0, 0.0], [-8000.0, len * rise, len * run], [8000.0, len * rise, len * run]);
    tri(&mut w, [-8000.0, 0.0, 0.0], [8000.0, len * rise, len * run], [8000.0, 0.0, 0.0]);
    w
}

#[test]
fn he_climbs_a_30_degree_slope_and_slides_down_a_55_degree_one() {
    // OoT: only flagged floors slide; SM64 steepness is mapped to that at 45 degrees (ordinary floors)
    let mut m = link_in(ramp(30.0), [0.0, 0.0, -300.0]);
    settle(&mut m);
    for _ in 0..90 {
        tick(&mut m, &In { y: 64.0, ..Default::default() });
    }
    assert!(m.pos[1] > 300.0, "walked up a 30 degree slope: y {}", m.pos[1]);

    let mut m = link_in(ramp(55.0), [0.0, 0.0, -100.0]);
    settle(&mut m);
    for _ in 0..40 {
        tick(&mut m, &In { y: 64.0, ..Default::default() });
    }
    let mut peak = 0.0f64;
    for _ in 0..90 {
        tick(&mut m, &In { y: 64.0, ..Default::default() });
        peak = peak.max(m.pos[1]);
    }
    assert!(peak < 400.0, "pushing uphill on a 55 degree slope gets nowhere: peak {peak}");
    assert!(m.forward_vel <= speed(10.0) + 0.01, "slide speed (1 - ny) * 40 capped at 10: {}", m.forward_vel);
}

/// `steps` stairs of `rise` x `run` going up along +z from z = 0, risers as real walls.
fn stairs(steps: usize, rise: f64, run: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    quad(&mut w, -8000.0, 8000.0, -8000.0, 0.0, 0.0);
    let o = SurfaceOptions::default();
    for i in 0..steps {
        let (z0, z1, y0, y1) = (i as f64 * run, (i + 1) as f64 * run, i as f64 * rise, (i + 1) as f64 * rise);
        quad(&mut w, -8000.0, 8000.0, z0, z1, y1);
        // the riser faces -z (towards the climber)
        let (a, b, c, d) = ([-8000.0, y0, z0], [8000.0, y0, z0], [8000.0, y1, z0], [-8000.0, y1, z0]);
        let probe = |p: [Vec3; 3]| {
            let mut t = CollisionWorld::default();
            t.add_triangle(p[0], p[1], p[2], &o).map(|s| s.normal[2])
        };
        let flip = probe([a, b, c]).is_some_and(|nz| nz > 0.0);
        if flip {
            w.add_triangle(a, c, b, &o);
            w.add_triangle(a, d, c, &o);
        } else {
            w.add_triangle(a, b, c, &o);
            w.add_triangle(a, c, d, &o);
        }
    }
    quad(&mut w, -8000.0, 8000.0, steps as f64 * run, 8000.0, steps as f64 * rise);
    w
}

#[test]
fn he_walks_up_stairs_of_castle_proportions_without_stopping_or_hopping() {
    // 8 steps of 25 x 90: risers under 30 units are ignored by the SM64 wall check (taller ones block Mario and Link alike)
    let mut m = link_in(stairs(8, 25.0, 90.0), [0.0, 0.0, -400.0]);
    settle(&mut m);
    let mut airborne = 0;
    for _ in 0..80 {
        tick(&mut m, &In { y: 64.0, ..Default::default() });
        if m.action & ACT_FLAG_AIR != 0 {
            airborne += 1;
        }
    }
    assert!(m.pos[1] > 190.0, "reached the top landing: y {} z {}", m.pos[1], m.pos[2]);
    assert!(airborne <= 2, "stair steps are walked, not jumped: {airborne} airborne ticks");
}

#[test]
fn the_oot_numbers_are_the_documented_conversion() {
    assert!((speed(6.0) - 17.6).abs() < 1e-9);
    assert!((accel(1.0) - 1.955_555_555_555_555_5).abs() < 1e-9);
    assert!((len(80.0) - 352.0).abs() < 1e-9);
}

// ---- A jumps where the roll on A is off (`Phys::a_jump`: Mario / Banjo courses and every game mode) ----

/// One A jump from the ground: (apex, ticks in the air, horizontal distance, last action, saw a roll).
fn a_jump(m: &mut MarioRuntime, stick: f64, hold_ticks: usize) -> (f64, usize, f64, u32, bool) {
    let p0 = m.pos;
    let y0 = m.pos[1];
    let (mut apex, mut n, mut rolled) = (0.0f64, 0usize, false);
    tick(m, &In { y: stick, pressed: A_BUTTON, ..Default::default() });
    while m.action & ACT_FLAG_AIR != 0 && n < 200 {
        apex = apex.max(m.pos[1] - y0);
        rolled |= jumpman_sim::characters::link_rolling(m);
        n += 1;
        tick(m, &In { y: stick, down: if n < hold_ticks { A_BUTTON } else { 0 }, ..Default::default() });
    }
    (apex, n, hdist(p0, m.pos), m.action, rolled)
}

fn a_jumper(legacy: bool) -> MarioRuntime {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    if legacy {
        m.link.disable();
    }
    m.link.phys.a_jump = true;
    settle(&mut m);
    m
}

#[test]
fn a_makes_a_real_jump_with_an_arc_and_a_landing_when_the_roll_is_off() {
    let mut m = a_jumper(false);
    tick(&mut m, &In { pressed: A_BUTTON, ..Default::default() });
    assert_eq!((m.action, m.action_arg), (ACT_FREEFALL, jumpman_sim::link::physics::ARG_A_JUMP), "standing A is a jump, not nothing");
    let mut m = a_jumper(false);
    let (apex, ticks, dist, last, _) = a_jump(&mut m, 0.0, 100);
    let (oot_apex, frames) = oot_arc(28.0 / speed(1.0), 1.0);
    assert!(near(apex, oot_apex * 4.4, 0.15), "apex {apex} vs {}", oot_apex * 4.4);
    assert!(near(ticks as f64, frames * 1.5, 0.15), "{ticks} ticks vs {}", frames * 1.5);
    assert!(dist < 30.0, "a standing jump goes straight up: {dist}");
    assert_eq!(last, ACT_IDLE, "lands on his feet, no stagger or roll");
    assert!(m.pos[1].abs() < 1.0);
    assert!(apex > 150.0, "a jump a player can read: {apex}");
}

#[test]
fn a_running_jump_carries_the_run_and_never_rolls() {
    let mut m = a_jumper(false);
    run_forward(&mut m, 30);
    let (apex, _, dist, last, rolled) = a_jump(&mut m, 64.0, 100);
    assert!(!rolled, "A must not roll");
    assert!(dist > 350.0, "the run's speed carries the jump: {dist}");
    assert!(apex > 150.0);
    assert!(last == ACT_IDLE || last == ACT_WALKING);
    assert_ne!(m.action_arg, 1.0, "no landing roll after a plain jump");
}

#[test]
fn letting_go_of_a_cuts_the_jump_short() {
    let mut m = a_jumper(false);
    let (full, ..) = a_jump(&mut m, 0.0, 100);
    let mut m = a_jumper(false);
    let (hop, ..) = a_jump(&mut m, 0.0, 0);
    assert!(hop < full * 0.5, "a tap hops lower: {hop} vs {full}");
    assert!(hop > 20.0, "but still leaves the ground: {hop}");
}

#[test]
fn without_the_flag_a_still_rolls_inside_the_real_oot_courses() {
    let mut m = link_in(flat(), [0.0, 0.0, 0.0]);
    assert!(!m.link.phys.a_jump, "the sim default is the faithful controller");
    settle(&mut m);
    run_forward(&mut m, 30);
    tick(&mut m, &In { y: 64.0, pressed: A_BUTTON, ..Default::default() });
    assert!(jumpman_sim::characters::link_rolling(&m));
}

#[test]
fn the_game_mode_link_without_the_faithful_controller_jumps_on_a_too() {
    // An experiment disables the controller (`link.enabled` false) but still steps Link's legacy movement: A used to roll there.
    let mut m = a_jumper(true);
    run_forward(&mut m, 20);
    let (apex, ticks, dist, last, rolled) = a_jump(&mut m, 64.0, 100);
    assert!(!rolled);
    assert!(near(apex, 220.0, 0.15), "apex {apex}");
    assert!(ticks > 15 && ticks < 30, "{ticks}");
    assert!(dist > 200.0, "{dist}");
    assert!(last == ACT_IDLE || last == ACT_WALKING, "lands: {last:#x}");
}

#[test]
fn the_a_jump_steers_in_the_air() {
    for legacy in [false, true] {
        let mut m = a_jumper(legacy);
        tick(&mut m, &In { pressed: A_BUTTON, ..Default::default() });
        let x0 = m.pos[0];
        for _ in 0..8 {
            tick(&mut m, &In { x: 64.0, down: A_BUTTON, ..Default::default() });
        }
        assert!((m.pos[0] - x0).abs() > 40.0, "legacy {legacy}: pushing right in the air moves him right ({})", m.pos[0] - x0);
    }
}
