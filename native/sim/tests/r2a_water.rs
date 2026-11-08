//! Round-2 water actors: Unagi (unagi.inc.c), Dorrie (dorrie.inc.c), the manta ray and jet stream rings
//! (manta_ray.inc.c, water_ring.inc.c).
mod r2a_common;
use jumpman_sim::behaviors::r2a::water_rings::MANTA_TRAJECTORY;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectEvent;
use r2a_common::*;

/// The eel of act 2 (`BPARAM2` 1): eight body parts appear within 3000 units, it waits at home, and its tail tip
/// (part -4) gives `spawn_default_star(6833, -3654, 2230)` for the star index of its placement (act 2).
#[test]
fn unagi_tail_gives_the_star() {
    let eel = spec(ObjectBehaviorKind::Unagi, [8270.0, -3130.0, 1846.0], to_s16(285.0 * 65536.0 / 360.0) as f64, (2 << 24) | (1 << 16));
    let mut m = runtime(ground(-3200.0, 9000.0), vec![eel]);
    stand_at(&mut m, 8270.0, 4000.0);
    let mut star = None;
    for _ in 0..3 {
        stand_at(&mut m, 8270.0, 4000.0);
        tick(&mut m);
    }
    let parts = find(&m, ObjectBehaviorKind::UnagiPart);
    assert_eq!(parts.len(), 8, "for (i = -4; i < 4; i++) spawn_object_relative(i, ..)");
    assert_eq!(m.objects[ObjId(0)].var("animState"), 1.0, "oAnimState = 1 marks the eel that carries the star");
    for _ in 0..5 {
        stand_at(&mut m, 8270.0, 4000.0);
        tick(&mut m);
    }
    let tail = *parts.iter().find(|&&p| m.objects[p].var("part") == -4.0).unwrap();
    for _ in 0..10 {
        let t = m.objects[tail].pos;
        stand_at(&mut m, t[0], t[2]);
        m.state.pos[1] = t[1];
        for e in tick(&mut m) {
            if let ObjectEvent::DefaultStar { pos, act } = e {
                star = Some((pos, act));
            }
        }
    }
    assert_eq!(star, Some(([6833.0, -3654.0, 2230.0], 2)));
}

/// The jet stream spawner releases a ring at oTimer 0, 50, 150, 200 and 250 of every 300 frames; crossing five
/// in index order gives the star of act 4 at (3400, -3200, -500).
#[test]
fn jet_stream_rings_in_order_give_the_star() {
    let sp = spec(ObjectBehaviorKind::JetStreamRingSpawner, [3404.0, -3319.0, -489.0], 0.0, 4 << 24);
    let mut m = runtime(ground(-6000.0, 9000.0), vec![sp]);
    let mut star = None;
    let mut spawn_times = vec![];
    let mut collected_idx = std::collections::HashSet::new();
    for frame in 0..400 {
        // Mario tries to cross every ring that exists: he stands in its plane, once on each side.
        let rings = find(&m, ObjectBehaviorKind::WaterRing);
        let target = rings.iter().filter(|&&r| m.objects[r].action == 0 && !collected_idx.contains(&(m.objects[r].var("ringIndex") as i64))).min_by_key(|&&r| m.objects[r].var("ringIndex") as i64).copied();
        if let Some(r) = target {
            let o = &m.objects[r];
            let n = [o.var("nx"), o.var("ny"), o.var("nz")];
            let side = if frame % 2 == 0 { 1.0 } else { -1.0 };
            let p = [o.pos[0] + n[0] * 20.0 * side, o.pos[1] + n[1] * 20.0 * side - 80.0, o.pos[2] + n[2] * 20.0 * side];
            m.state.pos = p;
            m.state.vel = [0.0; 3];
            m.set_action(jumpman_sim::state::ACT_WATER_IDLE, 0.0);
            m.invinc_timer = 100.0;
        } else {
            m.state.pos = [3404.0, -5000.0, 3000.0];
        }
        let before = find(&m, ObjectBehaviorKind::WaterRing).len();
        for e in tick(&mut m) {
            if let ObjectEvent::DefaultStar { pos, act } = e {
                star = Some((pos, act));
            }
        }
        if find(&m, ObjectBehaviorKind::WaterRing).len() > before {
            spawn_times.push(frame);
        }
        for r in find(&m, ObjectBehaviorKind::WaterRing) {
            if m.objects[r].action == 1 {
                collected_idx.insert(m.objects[r].var("ringIndex") as i64);
            }
        }
        if star.is_some() {
            break;
        }
    }
    assert_eq!(&spawn_times[..5], &[0, 50, 150, 200, 250], "ring release frames");
    assert_eq!(star, Some(([3400.0, -3200.0, -500.0], 4)));
}

/// The manta ray follows `sMantaRayTraj` at 10 units a frame and releases its rings behind it.
#[test]
fn manta_ray_follows_its_path_and_drops_rings() {
    let ray = spec(ObjectBehaviorKind::MantaRay, MANTA_TRAJECTORY[0], 0.0, 5 << 24);
    let mut m = runtime(ground(-6000.0, 9000.0), vec![ray]);
    let mut visited = vec![];
    for _ in 0..1200 {
        m.state.pos = [0.0, -5000.0, 8000.0];
        m.invinc_timer = 100.0;
        tick(&mut m);
        let i = m.objects[ObjId(0)].var("prevWaypoint") as usize;
        if visited.last() != Some(&i) {
            visited.push(i);
        }
        assert!(m.objects[ObjId(0)].forward_vel == 10.0 || m.objects[ObjId(0)].forward_vel == 0.0);
    }
    assert!(visited.len() >= 6, "visited waypoints {visited:?}");
    assert!(m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::WaterRing).count() >= 5, "rings at oTimer 0/50/150/200/250");
    assert_eq!(m.objects[ObjId(0)].scale, 2.5, "cur_obj_scale(2.5)");
}

/// Dorrie starts 2000 units east of home, keeps her distance to home in [1650, 2300] (plus the neck shift) and
/// turns to Mario's facing while he rides her.
#[test]
fn dorrie_swims_in_her_ring_and_follows_mario() {
    let dorrie = spec(ObjectBehaviorKind::Dorrie, [-3533.0, -4969.0, 3558.0], 0.0, 0);
    let mut m = runtime(ground(-6000.0, 9000.0), vec![dorrie]);
    m.state.pos = [0.0, -5000.0, 0.0];
    tick(&mut m);
    let o = &m.objects[ObjId(0)];
    assert_eq!(o.pos[0], -3533.0 + 2000.0, "ADD_FLOAT(oPosX, 2000)");
    assert_eq!(o.home, [-3533.0, -4969.0, 3558.0]);
    for _ in 0..600 {
        m.state.pos = [0.0, -5000.0, 0.0];
        tick(&mut m);
        let o = &m.objects[ObjId(0)];
        let d = dist3([o.pos[0], 0.0, o.pos[2]], [o.home[0], 0.0, o.home[2]]);
        assert!(d > 1000.0 && d < 2800.0, "dist to home {d}");
    }
    assert_eq!(m.objects[ObjId(0)].forward_vel, 5.0, "targetSpeed 5 when nobody rides her");
}
