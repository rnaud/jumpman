//! Link takes hold of a block or a gravestone (`Mv::Grab`): A in front of a face, then the stick decides push, pull or hold
//! (`Player_Action_8084B78C` and the push / pull actions). The app moves the block and carries Link with it.
use jumpman_sim::collision::*;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
    w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
    w
}

fn link() -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(vec![], None, None));
    m.link.enable(Age::Adult);
    m
}

#[derive(Default, Clone)]
struct In {
    y: f64,
    down: u32,
    pressed: u32,
}

fn tick(m: &mut MarioRuntime, i: &In) {
    let c = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: i.y, stick_mag: i.y.abs().min(64.0), button_down: i.down | i.pressed, button_pressed: i.pressed };
    m.step(&c, 32768.0, true, false);
}

#[test]
fn a_takes_hold_of_a_face_in_front_and_the_stick_decides_push_pull_or_hold() {
    let mut m = link();
    for _ in 0..2 {
        tick(&mut m, &In::default());
    }
    let face = GrabFace { id: 7, yaw: 0.0 };
    // nothing to hold: A does not grab
    tick(&mut m, &In { pressed: A_BUTTON, ..Default::default() });
    assert_ne!(m.link.mv, Mv::Grab);
    for _ in 0..30 {
        tick(&mut m, &In::default());
    }
    // a face ahead: A grabs it
    m.link.grab_face = Some(face);
    tick(&mut m, &In { pressed: A_BUTTON, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Grab);
    assert_eq!(m.link.grip.map(|g| (g.id, g.force)), Some((7, 0)));
    assert_eq!(m.link.pose.map(|p| p.clip), Some("pushWait"));
    let start = m.pos;
    // the stick toward the face pushes (the camera looks along +z for yaw 32768: up on the stick is toward -z... so check both)
    let mut forces = vec![];
    for y in [64.0, -64.0] {
        m.link.grab_face = Some(face);
        tick(&mut m, &In { y, down: A_BUTTON, ..Default::default() });
        forces.push(m.link.grip.map(|g| g.force));
    }
    assert!(forces == vec![Some(1), Some(-1)] || forces == vec![Some(-1), Some(1)], "{forces:?}");
    let clips: Vec<_> = [1i8, -1].iter().map(|f| if *f == 1 { "pushing" } else { "pulling" }).collect();
    assert!(clips.contains(&m.link.pose.unwrap().clip));
    // Link himself does not walk: the app moves the block and carries him
    assert_eq!(m.pos, start);
    // a released stick holds
    m.link.grab_face = Some(face);
    tick(&mut m, &In { down: A_BUTTON, ..Default::default() });
    assert_eq!(m.link.grip.map(|g| g.force), Some(0));
    // releasing A lets go
    m.link.grab_face = Some(face);
    tick(&mut m, &In::default());
    assert_eq!(m.link.mv, Mv::Free);
    assert!(m.link.grip.is_none());
    // and so does the face going away
    m.link.grab_face = Some(face);
    tick(&mut m, &In { pressed: A_BUTTON, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Grab);
    m.link.grab_face = None;
    tick(&mut m, &In { down: A_BUTTON, ..Default::default() });
    assert_eq!(m.link.mv, Mv::Free);
}

#[test]
fn an_enemys_hand_pins_link_until_he_shakes_free_with_the_stick_and_a_or_b_or_the_hand_lets_go() {
    let mut m = link();
    for _ in 0..2 {
        tick(&mut m, &In::default());
    }
    assert!(m.link.grabbable(&m));
    m.link.grab_request = true;
    m.link.held = true;
    tick(&mut m, &In::default());
    assert_eq!(m.link.mv, Mv::Held);
    let at = m.pos;
    // standing still does not free him, and he does not walk off
    for _ in 0..60 {
        tick(&mut m, &In { y: 64.0, ..Default::default() });
    }
    assert_eq!(m.link.mv, Mv::Held, "a steady stick does nothing");
    assert_eq!(m.pos, at);
    assert!(matches!(m.link.pose.map(|p| p.clip), Some("held") | Some("heldWait")));
    // A or B adds 5 a press: 21 presses clear the 100
    let mut presses = 0;
    while m.link.mv == Mv::Held && presses < 40 {
        tick(&mut m, &In { pressed: if presses % 2 == 0 { A_BUTTON } else { B_BUTTON }, ..Default::default() });
        presses += 1;
    }
    assert_eq!(presses, 21);
    assert_eq!(m.link.mv, Mv::Free);
    assert!(!m.link.held);
    // wiggling the stick counts too: a full swing of the stick adds `angle * magnitude * 2.54e-6` per frame
    m.link.grab_request = true;
    m.link.held = true;
    tick(&mut m, &In::default());
    assert_eq!(m.link.mv, Mv::Held);
    let mut n = 0;
    while m.link.mv == Mv::Held && n < 400 {
        tick(&mut m, &In { y: if n % 2 == 0 { 64.0 } else { -64.0 }, ..Default::default() });
        n += 1;
    }
    assert!(n < 400 && m.link.mv == Mv::Free, "{n}");
    // the hand lets go
    m.link.grab_request = true;
    m.link.held = true;
    tick(&mut m, &In::default());
    assert_eq!(m.link.mv, Mv::Held);
    m.link.force_release();
    tick(&mut m, &In::default());
    assert_eq!(m.link.mv, Mv::Free);
    // and it will not take a Link on the hookshot
    m.link.mv = Mv::HookFly;
    assert!(!m.link.grabbable(&m));
}
