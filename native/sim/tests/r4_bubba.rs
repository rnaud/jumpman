//! Bubba (`bubba.inc.c`): wanders underwater, chases Mario inside 2000 and swallows him with its mouth open.
mod r2a_common;
use jumpman_sim::collision::*;
use jumpman_sim::object::*;
use jumpman_sim::state::*;
use r2a_common::*;

fn world() -> CollisionWorld {
    let mut w = ground(-3200.0, 12000.0);
    w.water_boxes.push(WaterBox(0.0, -12000.0, -12000.0, 12000.0, 12000.0, -1700.0));
    w
}

#[test]
fn bubba_wakes_chases_and_eats_mario() {
    let mut m = runtime(world(), vec![spec(ObjectBehaviorKind::Bubba, [0.0, -2000.0, 0.0], 0.0, 0)]);
    let id = ObjId(0);
    let mario = [1400.0, -2000.0, 0.0];
    let mut chased = false;
    let mut eaten = false;
    for _ in 0..900 {
        m.state.pos = mario;
        m.state.vel = [0.0; 3];
        m.invinc_timer = 0.0;
        tick(&mut m);
        if m.objects[id].action == 1 {
            chased = true;
        }
        if m.action == ACT_EATEN_BY_BUBBA || m.used_obj == Some(id) {
            eaten = true;
            break;
        }
    }
    assert!(chased, "within 2000 and after 30 frames Bubba chases");
    assert!(eaten, "the open mouth within 250 swallows Mario");
}

#[test]
fn bubba_ignores_a_distant_mario() {
    let mut m = runtime(world(), vec![spec(ObjectBehaviorKind::Bubba, [0.0, -2000.0, 0.0], 0.0, 0)]);
    let id = ObjId(0);
    for _ in 0..200 {
        m.state.pos = [9000.0, -2000.0, 0.0];
        m.state.vel = [0.0; 3];
        tick(&mut m);
        assert_eq!(m.objects[id].action, 0);
    }
}
