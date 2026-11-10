//! OoT parity of Link's movement on the real SM64 courses (and Kokiri Forest as the reference scene).
//! Needs the imported courses; every test returns early without them. The numeric table is in
//! `native/LINK_PARITY.md`; `sim/tests/link_parity.rs` pins the same numbers on synthetic worlds.
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use jumpman_sim::link::combat::speed;
use jumpman_sim::link::items::Age;
use jumpman_sim::state::*;

fn assets() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn load(course: &str) -> Option<GameSim> {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).ok()?;
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    for _ in 0..40 {
        sim.tick(&ControllerSnapshot::default());
    }
    Some(sim)
}

fn ctl(x: f64, y: f64, down: u32, pressed: u32) -> ControllerSnapshot {
    let mag = (x * x + y * y).sqrt().min(64.0);
    ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: mag, button_down: down | pressed, button_pressed: pressed, ..Default::default() }
}

fn floor_at(sim: &GameSim, x: f64, z: f64, from: f64) -> f64 {
    sim.mario.world.find_floor_height(x, from, z)
}

/// A straight, level run of `len` units from the spawn: (start, yaw in sim angle units).
/// The first of 16 headings around the spawn (and a few rings of start points) whose floor stays within
/// 3 units of the start height the whole way.
fn level_run(sim: &GameSim, len: f64) -> Option<([f64; 3], f64)> {
    let p = sim.mario.pos;
    for ring in [0.0, 300.0, 600.0, 1000.0, 1600.0] {
        for s in 0..8 {
            let a = s as f64 * std::f64::consts::FRAC_PI_4;
            let start = [p[0] + a.sin() * ring, p[2] + a.cos() * ring];
            let y0 = floor_at(sim, start[0], start[1], p[1] + 200.0);
            if (y0 - p[1]).abs() > 300.0 || y0 < -9000.0 {
                continue;
            }
            for h in 0..16 {
                let ha = h as f64 * std::f64::consts::FRAC_PI_8;
                let ok = (0..=(len / 50.0) as usize).all(|i| {
                    let d = i as f64 * 50.0;
                    let y = floor_at(sim, start[0] + ha.sin() * d, start[1] + ha.cos() * d, y0 + 200.0);
                    (y - y0).abs() < 3.0
                });
                if ok {
                    return Some(([start[0], y0, start[1]], ha * 65536.0 / std::f64::consts::TAU));
                }
            }
        }
    }
    None
}

/// The full stick that makes `intended_yaw` (`atan2s(-y, x) + camera_yaw`) equal `yaw`.
fn stick_for(yaw: f64, camera_yaw: f64) -> (f64, f64) {
    let want = jumpman_sim::math::to_s16(yaw - camera_yaw) as f64;
    let mut best = (f64::MAX, (0.0, 64.0));
    for i in 0..720 {
        let a = i as f64 * std::f64::consts::TAU / 720.0;
        let (sx, sy) = (a.sin() * 64.0, a.cos() * 64.0);
        let err = (jumpman_sim::math::to_s16(jumpman_sim::math::atan2s(-sy, sx) as f64 - want) as f64).abs();
        if err < best.0 {
            best = (err, (sx, sy));
        }
    }
    best.1
}

fn place(sim: &mut GameSim, pos: [f64; 3], yaw: f64) {
    let m = &mut sim.mario;
    m.pos = pos;
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.face_angle[1] = yaw;
    m.set_action(ACT_IDLE, 0.0);
    for _ in 0..4 {
        sim.tick(&ControllerSnapshot::default());
    }
}

/// Full stick along `yaw` for `n` ticks from `pos`: (speed after the ramp-up, distance in the last 30 ticks).
fn run_along(sim: &mut GameSim, pos: [f64; 3], yaw: f64, n: usize) -> (f64, f64) {
    place(sim, pos, yaw);
    // the stick is relative to the camera: aim it at the wanted world yaw (the sim turns `intended_yaw` from camera_yaw)
    let (sx, sy) = stick_for(yaw, sim.camera_yaw as f64);
    let mut p = sim.mario.pos;
    for t in 0..n {
        if t == n - 30 {
            p = sim.mario.pos;
        }
        sim.tick(&ctl(sx, sy, 0, 0));
    }
    let m = &sim.mario;
    (m.forward_vel, ((m.pos[0] - p[0]).powi(2) + (m.pos[2] - p[2]).powi(2)).sqrt())
}

/// Link covers 6 OoT units per frame on the SM64 courses exactly as in Kokiri Forest: 17.6 world units per tick,
/// a third of the old 32 (Mario's run).
#[test]
fn full_stick_is_the_oot_run_speed_on_sm64_courses_and_in_kokiri_forest() {
    let mut checked = 0;
    for course in ["bob", "wf", "ccm", "castle_grounds", "castle_inside", "kokiri_forest"] {
        let Some(mut sim) = load(course) else { continue };
        let Some((pos, yaw)) = level_run(&sim, 1400.0) else {
            println!("{course}: no level 1400 unit run near the spawn");
            continue;
        };
        let (fv, dist30) = run_along(&mut sim, pos, yaw, 70);
        println!("{course}: fv {fv:.2}, {dist30:.0} units in 30 ticks");
        assert!((fv - speed(6.0)).abs() < 0.6, "{course}: forward speed {fv}");
        assert!(dist30 < 30.0 * speed(6.0) + 30.0 && dist30 > 30.0 * speed(6.0) * 0.7, "{course}: {dist30} units in 30 ticks");
        checked += 1;
    }
    if assets().join("courses").exists() {
        assert!(checked > 0, "no course had a level run");
    }
}

/// Running off a tall ledge on Cool, Cool Mountain (2970 units = 675 OoT): with the stick forward he rolls out unhurt
/// (`Player_Action_8084411C`: a fall under 800 OoT units rolls), where Mario's landing never cared.
#[test]
fn a_long_run_off_ccm_rolls_out_unhurt() {
    let Some(mut sim) = load("ccm") else { return };
    place(&mut sim, [1038.0, 2548.0, -2605.0], 32768.0);
    let (sx, sy) = stick_for(32768.0, sim.camera_yaw as f64);
    let hp0 = sim.mario.link.inv.health;
    let (mut left, mut fall) = (false, 0.0f64);
    for _ in 0..400 {
        sim.tick(&ctl(sx, sy, 0, 0));
        let m = &sim.mario;
        if m.action & ACT_FLAG_AIR != 0 {
            left = true;
            fall = fall.max(m.link.phys.fall_start - m.pos[1]);
        } else if left {
            break;
        }
    }
    println!("ccm ledge: fell {fall:.0} units ({:.0} OoT), action {:#x}/{}", fall / 4.4, sim.mario.action, sim.mario.action_arg);
    assert!(left && fall > 80.0 * 4.4 && fall < 800.0 * 4.4, "fell {fall}");
    assert_eq!(sim.mario.link.inv.health, hp0, "rolled out of a {:.0} OoT unit fall", fall / 4.4);
    assert!(link_rolled(&sim), "landing roll");
}

fn link_rolled(sim: &GameSim) -> bool {
    sim.mario.action == ACT_WALKING && sim.mario.action_arg == 1.0
}

/// 4000 units straight down onto the floor of Bob-omb Battlefield (909 OoT units): a heart and the stagger.
#[test]
fn a_4000_unit_drop_costs_a_heart_on_bob() {
    let Some(mut sim) = load("bob") else { return };
    let Some((pos, yaw)) = level_run(&sim, 400.0) else { return };
    place(&mut sim, pos, yaw);
    let hp0 = sim.mario.link.inv.health;
    sim.mario.pos[1] += 4000.0;
    sim.mario.set_action(ACT_FREEFALL, 0.0);
    sim.mario.link.phys.fall_start = sim.mario.pos[1];
    for _ in 0..200 {
        sim.tick(&ControllerSnapshot::default());
        if sim.mario.action & ACT_FLAG_AIR == 0 {
            break;
        }
    }
    assert_eq!(hp0 - sim.mario.link.inv.health, 16);
    assert!(sim.mario.link.phys.stun > 0.0);
}

/// Sample of edges and steep floors around the spawn of each course, for the capture scripts (`--ignored --nocapture`).
#[test]
#[ignore]
fn find_spots() {
    let only = std::env::var("LP_COURSE").ok();
    for course in ["bob", "wf", "ccm", "castle_grounds", "castle_inside"] {
        if only.as_deref().is_some_and(|o| o != course) {
            continue;
        }
        let Some(sim) = load(course) else { continue };
        let p = sim.mario.pos;
        println!("== {course} spawn {:?} yaw {:.0}", p.map(|v| v.round()), sim.mario.face_angle[1]);
        let (mut ledges, mut slopes, mut stairs): (Vec<(f64, f64, f64, f64, f64)>, Vec<(f64, f64, f64, f64, f64)>, Vec<(f64, f64, f64, f64)>) = (vec![], vec![], vec![]);
        for ix in -40..=40 {
            for iz in -40..=40 {
                let (x, z) = (p[0] + ix as f64 * 150.0, p[2] + iz as f64 * 150.0);
                let y0 = floor_at(&sim, x, z, p[1] + 1500.0);
                if y0 < -9000.0 {
                    continue;
                }
                for h in 0..8 {
                    let ha = h as f64 * std::f64::consts::FRAC_PI_4;
                    let at = |d: f64| floor_at(&sim, x + ha.sin() * d, z + ha.cos() * d, y0 + 200.0);
                    // a level stretch of 400 units ending in a drop of 500..900
                    if (0..=8).all(|i| (at(i as f64 * 50.0) - y0).abs() < 3.0) && at(550.0) < y0 - 450.0 && at(550.0) > y0 - 1200.0 && at(700.0) < y0 - 450.0 {
                        ledges.push((x, y0, z, ha, y0 - at(700.0)));
                    }
                    // a staircase: sampled every 10 units over 600, four or more risers of 15..90, never going down
                    let s: Vec<f64> = (0..60).map(|i| at(i as f64 * 10.0)).collect();
                    let risers = s.windows(2).filter(|w| w[1] - w[0] > 12.0).count();
                    if risers >= 4 && s.windows(2).all(|w| w[1] - w[0] > -1.0 && w[1] - w[0] < 90.0) && s[59] - s[0] > 100.0 {
                        stairs.push((x, y0, z, ha));
                    }
                }
                if let Some(f) = sim.mario.world.find_floor(x, y0 + 100.0, z).surface {
                    if f.normal[1] < 0.62 && f.normal[1] > 0.3 && f.kind == 0 {
                        // the way up is against the horizontal part of the normal
                        let up = (-f.normal[0]).atan2(-f.normal[2]).to_degrees();
                        slopes.push((x, y0, z, f.normal[1], up));
                    }
                }
            }
        }
        let show = |name: &str, n: usize, lines: Vec<String>| {
            println!("{name}: {n}");
            for l in lines.iter().step_by((lines.len() / 4).max(1)).take(5) {
                println!("   {l}");
            }
        };
        show("ledges", ledges.len(), ledges.iter().map(|(x, y, z, a, d)| format!("x {x:.0} y {y:.0} z {z:.0} heading {:.0} deg drop {d:.0}", a.to_degrees())).collect());
        show("stairs", stairs.len(), stairs.iter().map(|(x, y, z, a)| format!("x {x:.0} y {y:.0} z {z:.0} heading {:.0} deg", a.to_degrees())).collect());
        show("slopes", slopes.len(), slopes.iter().map(|(x, y, z, n, up)| format!("x {x:.0} y {y:.0} z {z:.0} ny {n:.2} uphill heading {up:.0} deg")).collect());
    }
}
