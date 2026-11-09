//! Big Boo's Haunt furniture: Mad Piano, Haunted Chair, Flying Bookend and its spawner
//! (`mad_piano.inc.c`, `haunted_chair.inc.c`, `flying_bookend_switch.inc.c`).
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

fn rig(specs: Vec<ObjectSpec>) -> jumpman_sim::mario_runtime::MarioRuntime {
    let mut m = runtime(ground(0.0, 6000.0), specs);
    stand_at(&mut m, 0.0, 0.0);
    m
}

fn put_mario(m: &mut jumpman_sim::mario_runtime::MarioRuntime, pos: [f64; 3]) {
    m.state.pos = pos;
    m.state.vel = [0.0; 3];
    m.invinc_timer = 100.0;
}

/// The piano sleeps until Mario has been near for 20 frames and is running, then bites and chases
/// at 5 per frame but never leaves 400 units around home; it goes back to sleep 80 frames after Mario left.
#[test]
fn mad_piano_wakes_when_mario_runs_close_and_sleeps_again() {
    let mut m = rig(vec![spec(ObjectBehaviorKind::MadPiano, [0.0, 0.0, 0.0], 0.0, 0)]);
    let id = ObjId(0);
    assert_eq!(m.objects[id].hitbox.interact_type, 0, "intangible while it waits");
    for _ in 0..30 {
        put_mario(&mut m, [0.0, 0.0, 450.0]);
        m.state.forward_vel = 5.0; // walking slowly does not wake it
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0);
    for _ in 0..3 {
        put_mario(&mut m, [0.0, 0.0, 450.0]);
        m.state.forward_vel = 20.0;
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 1, "running past wakes the piano");
    assert_ne!(m.objects[id].hitbox.interact_type, 0, "tangible while it bites");
    let start = m.objects[id].pos;
    for _ in 0..60 {
        put_mario(&mut m, [0.0, 0.0, 450.0]);
        tick(&mut m);
    }
    let o = &m.objects[id];
    let dist = ((o.pos[0] - o.home[0]).powi(2) + (o.pos[2] - o.home[2]).powi(2)).sqrt();
    assert!(o.pos[2] > start[2] + 50.0, "chases Mario");
    assert!(dist <= 401.0, "leash of 400 units, got {dist}");
    // Mario leaves: after 80 frames and the end of the bite animation it sleeps again.
    for _ in 0..140 {
        put_mario(&mut m, [0.0, 0.0, 5000.0]);
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0);
    assert_eq!(m.objects[id].hitbox.interact_type, 0);
}

#[test]
fn mad_piano_bites_for_three_wedges() {
    let mut m = rig(vec![spec(ObjectBehaviorKind::MadPiano, [0.0, 0.0, 0.0], 0.0, 0)]);
    for _ in 0..25 {
        put_mario(&mut m, [0.0, 0.0, 450.0]);
        m.state.forward_vel = 20.0;
        tick(&mut m);
    }
    m.invinc_timer = 0.0;
    m.state.pos = [0.0, 0.0, 150.0];
    tick(&mut m);
    assert_eq!(m.hurt_counter, 11.0, "damage 3 = 12 hurt-counter units, one already paid out");
}

/// A lone chair waits until Mario has been within 500 for a frame, shakes for 30 frames, floats up
/// for 50, tips back, spins and then flies at Mario at 50 units per frame.
#[test]
fn haunted_chair_shakes_floats_and_charges() {
    let mut m = rig(vec![spec(ObjectBehaviorKind::HauntedChair, [0.0, 0.0, 0.0], 0.0, 0)]);
    let id = ObjId(0);
    for _ in 0..5 {
        put_mario(&mut m, [0.0, 0.0, 2000.0]);
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0, "waits for Mario");
    for _ in 0..3 {
        put_mario(&mut m, [0.0, 0.0, 400.0]);
        tick(&mut m);
    }
    for _ in 0..40 {
        put_mario(&mut m, [0.0, 0.0, 400.0]);
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 1, "after 30 shaking frames");
    let mut peak = 0.0f64;
    let mut charged = false;
    for _ in 0..140 {
        put_mario(&mut m, [0.0, 0.0, 400.0]);
        tick(&mut m);
        peak = peak.max(m.objects[id].pos[1]);
        if m.objects[id].forward_vel.abs() > 40.0 {
            charged = true;
            break;
        }
    }
    assert!(peak > 100.0, "floats up, peak {peak}");
    assert!(charged, "launches at Mario");
}

/// Next to a Mad Piano the chair has a parent and does not charge: it tips away from the piano instead.
#[test]
fn haunted_chair_next_to_a_piano_tips_over() {
    let mut m = rig(vec![
        spec(ObjectBehaviorKind::MadPiano, [-1300.0, 0.0, 2310.0], 243.0, 0),
        spec(ObjectBehaviorKind::HauntedChair, [-1250.0, 0.0, 2200.0], 66.0, 0),
    ]);
    let chair = ObjId(1);
    for _ in 0..40 {
        put_mario(&mut m, [0.0, 0.0, 0.0]);
        tick(&mut m);
    }
    let o = &m.objects[chair];
    assert!(o.link.is_some(), "piano within 300 becomes the parent");
    assert_eq!(o.action, 0);
    assert!(o.var("u104") != 0.0, "picked a tipping direction");
    assert!(o.var("facePitch") != 0.0 || o.var("faceRoll") != 0.0, "leans");
}

/// A bookend that sees Mario within 400 opens up (act 1, 30-frame animation), then hovers (act 2) and
/// lunges at 50 units per frame after 30 frames.
#[test]
fn flying_bookend_wakes_opens_and_lunges() {
    let mut m = rig(vec![spec(ObjectBehaviorKind::FlyingBookend, [0.0, 300.0, 0.0], 0.0, 0)]);
    let id = ObjId(0);
    for _ in 0..5 {
        put_mario(&mut m, [0.0, 0.0, 2000.0]);
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0);
    put_mario(&mut m, [0.0, 0.0, 200.0]);
    tick(&mut m);
    assert_eq!(m.objects[id].action, 1);
    assert_eq!(m.objects[id].var("partModel"), 1.0, "swaps to the biting model");
    let mut act2 = false;
    for _ in 0..60 {
        put_mario(&mut m, [0.0, 0.0, 200.0]);
        tick(&mut m);
        if m.objects[id].action == 2 {
            act2 = true;
            break;
        }
    }
    assert!(act2);
    assert!(m.objects[id].scale > 0.7, "grows while opening");
    let mut lunged = false;
    for _ in 0..80 {
        put_mario(&mut m, [0.0, 0.0, 200.0]);
        tick(&mut m);
        let o = &m.objects[id];
        if (o.forward_vel.powi(2) + o.vel[1].powi(2)).sqrt() > 49.0 {
            lunged = true;
            break;
        }
    }
    assert!(lunged, "lunges at 50");
}

/// The spawner shoots a bookend (already in its lunge, action 3 -> 2 at 50) when Mario is in front within 600.
#[test]
fn bookend_spawner_shoots_books_at_mario() {
    let mut m = rig(vec![spec(ObjectBehaviorKind::BookendSpawn, [0.0, 300.0, 0.0], 0.0, 0)]);
    for _ in 0..30 {
        put_mario(&mut m, [0.0, 0.0, 400.0]);
        tick(&mut m);
    }
    assert!(find(&m, ObjectBehaviorKind::FlyingBookend).is_empty(), "waits 40 frames");
    for _ in 0..15 {
        put_mario(&mut m, [0.0, 0.0, 400.0]);
        tick(&mut m);
    }
    let books = find(&m, ObjectBehaviorKind::FlyingBookend);
    assert_eq!(books.len(), 1);
    for _ in 0..8 {
        put_mario(&mut m, [0.0, 0.0, 400.0]);
        tick(&mut m);
    }
    let b = &m.objects[books[0]];
    assert!(b.action == 2 || !b.active, "the shot book flies on");
}

/// Stomped from above... no: hit from below by a jump kills it for a blue coin; touching it at
/// flight speed costs health and destroys it.
#[test]
fn flying_bookend_dies_when_it_hits_mario() {
    let mut m = rig(vec![spec(ObjectBehaviorKind::FlyingBookend, [0.0, 0.0, 0.0], 0.0, 0)]);
    let id = ObjId(0);
    m.objects[id].action = 2;
    m.objects[id].forward_vel = 0.0;
    m.invinc_timer = 0.0;
    m.state.pos = [0.0, 0.0, 20.0];
    for _ in 0..4 {
        m.invinc_timer = 0.0;
        tick(&mut m);
    }
    assert!(!m.objects[id].active, "a bookend that bit Mario is gone");
    assert!(m.hurt_counter > 0.0 || m.health < m.max_health);
}

