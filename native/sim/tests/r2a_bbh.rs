//! BBH round-2 behaviors: Mr. I (mr_i.inc.c) and the Merry-Go-Round Boo manager (boo.inc.c).
mod r2a_common;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectEvent;
use r2a_common::*;

/// Mr. I looks at Mario inside 1500 units, shoots particles, and dies when Mario runs a full circle around it
/// (`oMrIUnkFC > 65536`): spin (64 + 32 frames), shrink, then the big BBH one spawns the star at (1370, 2000, -320).
#[test]
fn mr_i_dies_when_circled_and_the_big_one_drops_the_star() {
    for big in [false, true] {
        let param = if big { (6u32 << 24) | (1 << 16) } else { 0 };
        let mut m = runtime(ground(0.0, 6000.0), vec![spec(ObjectBehaviorKind::MrI, [0.0, 0.0, 0.0], 0.0, param)]);
        let id = ObjId(0);
        let mut events = vec![];
        // Mario stands 500 units away, facing Mr. I (|moveYaw - marioFaceYaw| > 0x4000 makes it stare back).
        let mut angle = 0.0;
        let mut set = |m: &mut jumpman_sim::mario_runtime::MarioRuntime, angle: f64| {
            stand_at(m, 500.0 * sins(angle), 500.0 * coss(angle));
            m.state.face_angle[1] = to_s16(angle + 0x8000 as f64) as f64; // facing the eye
        };
        set(&mut m, angle);
        for _ in 0..30 {
            set(&mut m, angle);
            events.extend(tick(&mut m));
        }
        assert_eq!(m.objects[id].scale, if big { 2.0 } else { 1.0 }, "cur_obj_scale(oBhvParams2ndByte + 1)");
        assert!(m.objects[id].action >= 1, "woke up inside 1500");
        assert_eq!(find(&m, ObjectBehaviorKind::MrIBody).len(), 1, "SPAWN_CHILD iris");
        // Run a circle: 0x600 per frame is slower than its 0x800 turn rate, so it keeps up and accumulates.
        let mut died_at = None;
        for f in 0..400 {
            angle += 0x600 as f64;
            set(&mut m, angle);
            events.extend(tick(&mut m));
            if m.objects[id].action == 3 && died_at.is_none() {
                died_at = Some(f);
            }
            if died_at.is_some() {
                // Once it spins, hold still.
                set(&mut m, angle);
            }
            if !m.objects[id].active || m.objects[id].var("g2dead") != 0.0 {
                break;
            }
        }
        assert!(died_at.is_some(), "Mario's circle killed it (big: {big})");
        let stars: Vec<_> = events.iter().filter(|e| matches!(e, ObjectEvent::DefaultStar { .. })).collect();
        if big {
            assert_eq!(stars, vec![&ObjectEvent::DefaultStar { pos: [1370.0, 2000.0, -320.0], act: 6 }]);
        } else {
            assert!(stars.is_empty());
            assert!(events.iter().any(|e| matches!(e, ObjectEvent::Coin { value, .. } if *value == 5.0)), "blue coin from oNumLootCoins 5");
        }
    }
}

/// The manager lets out two Boos at a time while Mario is on the merry-go-round (floor type 0x1A), five in all,
/// then the Big Boo, which takes three hits and spawns the star of act 2 at (-1600, -2100, 205).
#[test]
fn merry_go_round_boos_then_big_boo_then_star() {
    let mgr = spec(ObjectBehaviorKind::MerryGoRoundBooManager, [-1100.0, -2372.0, 1100.0], to_s16(135.0 * 65536.0 / 360.0) as f64, 2 << 24);
    let mut m = runtime(ground_kind(-2560.0, 8000.0, 0x1A), vec![mgr]);
    let mut stars = vec![];
    let mut max_alive = 0;
    let mut big_hits = 0;
    for frame in 0..6000 {
        stand_at(&mut m, -1100.0 + 400.0, 1100.0);
        // Boos are shy: Mario must have his back to them (his face yaw near their move yaw) or they fade out.
        if let Some(&b) = find(&m, ObjectBehaviorKind::MerryGoRoundBigBoo).first().or(find(&m, ObjectBehaviorKind::MerryGoRoundBoo).first()) {
            m.state.face_angle[1] = m.objects[b].move_yaw;
        }
        // The scripted Mario kills whatever boo is out: a punch from the side.
        for kind in [ObjectBehaviorKind::MerryGoRoundBoo, ObjectBehaviorKind::MerryGoRoundBigBoo] {
            for id in find(&m, kind) {
                let o = &mut m.objects[id];
                if o.action == 1 && o.timer > 40.0 && o.hitbox.interact_type != 0 && frame % 7 == 0 {
                    o.interact_status = INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED | ATTACK_PUNCH as u32;
                    if kind == ObjectBehaviorKind::MerryGoRoundBigBoo {
                        big_hits += 1;
                    }
                }
            }
        }
        for e in tick(&mut m) {
            if let ObjectEvent::DefaultStar { pos, act } = e {
                stars.push((pos, act));
            }
        }
        max_alive = max_alive.max(find(&m, ObjectBehaviorKind::MerryGoRoundBoo).len());
        if !stars.is_empty() {
            break;
        }
    }
    assert!(max_alive <= 2, "spawned - killed < 2 keeps at most two out ({max_alive})");
    assert_eq!(m.objects[ObjId(0)].var("spawned"), 5.0, "five Boos in all");
    assert_eq!(m.objects[ObjId(0)].var("killed"), 5.0);
    assert_eq!(find(&m, ObjectBehaviorKind::MerryGoRoundBoo).len(), 0);
    assert_eq!(big_hits, 3, "the Big Boo has three hit points");
    assert_eq!(stars, vec![([-1600.0, -2100.0, 205.0], 2)]);
}

/// Off the merry-go-round (floor type 0) nothing wakes up.
#[test]
fn merry_go_round_boos_sleep_unless_mario_rides() {
    let mgr = spec(ObjectBehaviorKind::MerryGoRoundBooManager, [-1100.0, -2372.0, 1100.0], 0.0, 2 << 24);
    let mut m = runtime(ground(-2560.0, 8000.0), vec![mgr]);
    for _ in 0..200 {
        stand_at(&mut m, -900.0, 1100.0);
        tick(&mut m);
    }
    let boos = find(&m, ObjectBehaviorKind::MerryGoRoundBoo);
    assert!(boos.iter().all(|&b| m.objects[b].action == 0), "boos stay in action 0 without gMarioOnMerryGoRound");
}
