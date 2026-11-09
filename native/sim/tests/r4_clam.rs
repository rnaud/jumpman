//! Clam Shell (`clam.inc.c`): snaps shut on a close Mario (2 damage), then reopens.
mod r2a_common;
use jumpman_sim::object::*;
use r2a_common::*;

#[test]
fn clam_shell_closes_on_a_close_mario_and_reopens() {
    let mut m = runtime(ground(-3000.0, 9000.0), vec![spec(ObjectBehaviorKind::ClamShell, [0.0, -3000.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, 4000.0, 0.0);
    let id = find(&m, ObjectBehaviorKind::ClamShell)[0];
    for _ in 0..200 {
        m.state.pos = [4000.0, -3000.0, 0.0];
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0, "a far Mario leaves it open");
    let mut closed = false;
    for _ in 0..60 {
        m.state.pos = [400.0, -3000.0, 0.0];
        tick(&mut m);
        if m.objects[id].action == 1 {
            closed = true;
            break;
        }
    }
    assert!(closed, "closed when Mario came within 500");
    // Intangible from frame 30 of the closing clip; back open after 150 frames.
    for _ in 0..40 {
        m.state.pos = [4000.0, -3000.0, 0.0];
        tick(&mut m);
    }
    assert_eq!(m.objects[id].hitbox.interact_type, 0, "intangible while shut");
    for _ in 0..160 {
        m.state.pos = [4000.0, -3000.0, 0.0];
        tick(&mut m);
    }
    assert_eq!(m.objects[id].action, 0);
    assert_ne!(m.objects[id].hitbox.interact_type, 0, "tangible again once open");
}
