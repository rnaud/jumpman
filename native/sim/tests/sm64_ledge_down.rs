//! `check_ledge_climb_down` / `act_ledge_climb_down`.
mod r2a_common;
use jumpman_sim::collision::*;
use jumpman_sim::state::*;
use r2a_common::*;

fn cliff(flip: bool) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-1000.0, 0.0, -500.0], [-1000.0, 0.0, 500.0], [0.0, 0.0, 500.0], &o);
    w.add_triangle([-1000.0, 0.0, -500.0], [0.0, 0.0, 500.0], [0.0, 0.0, -500.0], &o);
    w.add_triangle([0.0, -500.0, -500.0], [0.0, -500.0, 500.0], [1000.0, -500.0, 500.0], &o);
    w.add_triangle([0.0, -500.0, -500.0], [1000.0, -500.0, 500.0], [1000.0, -500.0, -500.0], &o);
    let (a, b, c, d) = ([0.0, -500.0, -500.0], [0.0, -500.0, 500.0], [0.0, 0.0, 500.0], [0.0, 0.0, -500.0]);
    if flip {
        w.add_triangle(a, c, b, &o);
        w.add_triangle(a, d, c, &o);
    } else {
        w.add_triangle(a, b, c, &o);
        w.add_triangle(a, c, d, &o);
    }
    w
}

fn run(flip: bool) -> Option<Vec<u32>> {
    jumpman_sim::actions::water::set_faithful(true);
    let mut m = runtime(cliff(flip), vec![]);
    m.state.pos = [-13.0, 0.0, 0.0];
    m.state.face_angle[1] = 0x4000 as f64;
    m.state.forward_vel = 4.0;
    m.set_action(ACT_WALKING, 0.0);
    let mut seen = vec![];
    for _ in 0..40 {
        // Stick right with the camera looking along +z: Mario walks towards +x, gently (magnitude below 10).
        let pad = ControllerSnapshot { focus_yaw: None, stick_x: 20.0, stick_y: 0.0, stick_mag: 20.0, button_down: 0, button_pressed: 0 };
        m.step(&pad, 0.0, false, false);
        if seen.last() != Some(&m.action) {
            seen.push(m.action);
        }
    }
    seen.contains(&ACT_LEDGE_CLIMB_DOWN).then_some(seen)
}

#[test]
fn walking_into_a_cliff_edge_lowers_mario_onto_the_ledge() {
    let seen = run(true).expect("the ledge climb down starts");
    assert!(seen.contains(&ACT_LEDGE_GRAB), "ends hanging: {seen:x?}");
}

