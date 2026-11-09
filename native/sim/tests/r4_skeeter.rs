//! Skeeter (`skeeter.inc.c`): idles on the shore, walks toward Mario inside 500 units, dies to a stomp for 3 coins.
mod r2a_common;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectEvent;
use r2a_common::*;

fn rig(mario: [f64; 3]) -> jumpman_sim::mario_runtime::MarioRuntime {
    let mut m = runtime(ground(0.0, 6000.0), vec![spec(ObjectBehaviorKind::Skeeter, [0.0, 0.0, 0.0], 0.0, 0)]);
    stand_at(&mut m, mario[0], mario[2]);
    m.invinc_timer = 100.0;
    m
}

#[test]
fn a_skeeter_on_the_ground_idles_then_walks_and_chases_mario_within_500() {
    let mut m = rig([0.0, 0.0, 450.0]);
    let id = ObjId(0);
    let mut walked = false;
    let mut closest = f64::MAX;
    for _ in 0..400 {
        tick(&mut m);
        let o = &m.objects[id];
        if o.action == 2 {
            walked = true;
        }
        closest = closest.min(dist3(o.pos, [0.0, 0.0, 450.0]));
    }
    assert!(walked, "idle -> walk once the idle animation ends");
    assert!(closest < 400.0, "it came at Mario (closest {closest})");
}

#[test]
fn a_far_skeeter_wanders_near_home() {
    // Mario 3000 away: `treat_far_home_as_mario(1000)` only matters once it strays; it stays within ~1000 of home.
    let mut m = rig([3000.0, 0.0, 0.0]);
    let id = ObjId(0);
    for _ in 0..600 {
        tick(&mut m);
    }
    assert!(dist3(m.objects[id].pos, [0.0, 0.0, 0.0]) < 1500.0);
}

#[test]
fn stomping_a_skeeter_drops_three_coins() {
    let mut m = rig([0.0, 400.0, 0.0]);
    let id = ObjId(0);
    // Let it settle, then drop on its head (hitbox 100 high, hurtbox 90, damage 2, 3 loot coins).
    for _ in 0..5 {
        tick(&mut m);
    }
    m.state.pos = [0.0, 95.0, 0.0];
    m.state.vel = [0.0, -30.0, 0.0];
    m.set_action(jumpman_sim::state::ACT_FREEFALL, 0.0);
    m.invinc_timer = 0.0;
    let mut coins = 0;
    for _ in 0..6 {
        for e in tick(&mut m) {
            if let ObjectEvent::Coin { .. } = e {
                coins += 1;
            }
        }
        coins += m.objects.events.iter().filter(|e| matches!(e, ObjectEvent::Coin { .. })).count();
    }
    assert!(!m.objects[id].active, "dead");
    assert!(coins >= 3, "three coins, saw {coins}");
}
