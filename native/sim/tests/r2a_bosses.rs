//! SM64 round 2 part A (behaviors/r2a): scripted scenarios against the decomp's constants.
//! Engine-free (no app, no render): a flat floor, a `MarioRuntime`, the object manager's step.
use jumpman_sim::behaviors::g2::dyn_collision::register_meshes_from_json;
use jumpman_sim::behaviors::r2a;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::to_s16;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

fn load_meshes(course: &str) {
    let path = format!("{}/../../assets/courses/{course}.json", env!("CARGO_MANIFEST_DIR"));
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(register_meshes_from_json(&v["extraCollision"]) > 0, "no g2 meshes in {course}");
}

fn ground(y: f64, r: f64) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-r, y, -r], [-r, y, r], [r, y, r], &o);
    w.add_triangle([-r, y, -r], [r, y, r], [r, y, -r], &o);
    w
}

fn runtime(world: CollisionWorld, specs: Vec<ObjectSpec>) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

fn tick(m: &mut MarioRuntime) -> Vec<ObjectEvent> {
    object_manager::step(m, None, false);
    let events = m.objects.events.clone();
    m.step(&ControllerSnapshot::default(), 0.0, false, false);
    object_manager::after_mario_step(m);
    events
}

fn stand_at(m: &mut MarioRuntime, x: f64, z: f64) {
    let y = m.world.find_floor(x, 9000.0, z).height;
    m.state.pos = [x, y, z];
    m.state.vel = [0.0; 3];
    m.state.forward_vel = 0.0;
    m.set_action(ACT_IDLE, 0.0);
}

fn find(m: &MarioRuntime, kind: ObjectBehaviorKind) -> Vec<ObjId> {
    m.objects.ids().filter(|&i| m.objects[i].behavior == kind && m.objects[i].active).collect()
}

// ---------------------------------------------------------------------------------------------
// Eyerok (eyerok.inc.c)
// ---------------------------------------------------------------------------------------------

/// The whole Eyerok fight, scripted: the boss spawns both hands at (-500 * side, 0, 300) with scale 1.5, wakes
/// at 500 units, waits for the hands (`oEyerokBossNumHands == 2`) and the intro dialog 117; each hand dies on its
/// third frontal hit (`--oHealth >= 2` from 4), and after both are gone dialog 118 and `spawn_default_star(0,
/// -900, -3700)` follow.
#[test]
fn eyerok_fight_ends_with_the_default_star() {
    load_meshes("ssl.3");
    let mut boss = ObjectSpec::new(ObjectBehaviorKind::EyerokBoss, [0.0, -1534.0, -3693.0], 0.0);
    boss.param = Some((1u32 << 24) as f64);
    let mut m = runtime(ground(-1534.0, 6000.0), vec![boss]);
    stand_at(&mut m, 0.0, -3693.0 + 2500.0);
    for _ in 0..3 {
        tick(&mut m);
    }
    // Both hands exist at their decomp offsets while the boss sleeps.
    let hands = find(&m, ObjectBehaviorKind::EyerokHand);
    assert_eq!(hands.len(), 2);
    let sides: Vec<f64> = hands.iter().map(|&h| m.objects[h].var("side")).collect();
    assert_eq!(sides, vec![-1.0, 1.0]);
    for &h in &hands {
        let o = &m.objects[h];
        let side = o.var("side");
        assert_eq!(o.home, [-500.0 * side, -1534.0, -3693.0 + 300.0], "spawn_object_relative(-500 * side, 0, 300)");
        assert_eq!(o.scale, 1.5);
        assert_eq!(o.face_yaw, to_s16(-(0x4000 as f64) * side) as f64, "oFaceAngleYaw -= 0x4000 * side");
    }
    let boss_id = ObjId(0);
    assert_eq!(m.objects[boss_id].action, 0);
    // Mario walks up to 500 units: the boss wakes up, the hands rise (anim 6, 80 frames) and become IDLE.
    stand_at(&mut m, 0.0, -3693.0 + 400.0);
    let mut woke = false;
    let mut asked_dialog = None;
    for _ in 0..600 {
        stand_at(&mut m, 0.0, -3693.0 + 400.0);
        for e in tick(&mut m) {
            if let ObjectEvent::KingDialog { id, .. } = e {
                asked_dialog = Some(id);
            }
        }
        woke |= m.objects[boss_id].action >= 1;
        if asked_dialog.is_some() {
            break;
        }
    }
    assert!(woke);
    assert_eq!(m.objects[boss_id].var("numHands"), 2.0, "both hands counted themselves in");
    assert_eq!(asked_dialog, Some(117.0), "DIALOG_117 intro");
    // The dialog ends; the fight begins.
    m.objects[boss_id].set_var("dialogDone", 1.0);
    tick(&mut m);
    assert_eq!(m.objects[boss_id].action, r2a::eyerok::BOSS_FIGHT);
    // Hit every hand that shows its eye, from the front, until both are gone.
    let mut star = None;
    for frame in 0..6000 {
        stand_at(&mut m, 0.0, -3693.0 + 2000.0);
        for &h in &hands {
            let o = &mut m.objects[h];
            if o.active && o.action == r2a::eyerok::HAND_SHOW_EYE {
                o.interact_status = INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED | ATTACK_PUNCH as u32;
            }
        }
        for e in tick(&mut m) {
            match e {
                ObjectEvent::DefaultStar { pos, act } => {
                    assert_eq!(act, 4, "BPARAM1(STAR_INDEX_ACT_4)");
                    star = Some((frame, pos))
                },
                ObjectEvent::KingDialog { id, .. } if id == 118.0 => {
                    m.objects[boss_id].set_var("dialogDone", 1.0);
                }
                _ => {}
            }
        }
        if star.is_some() {
            break;
        }
    }
    let (_, pos) = star.expect("spawn_default_star after both hands died");
    assert_eq!(pos, [0.0, -900.0, -3700.0]);
    assert_eq!(m.objects[boss_id].var("numHands"), 0.0);
}

/// Hand collision follows the hand: the open hand (`ssl_seg7_collision_070282F8`) is a solid surface
/// owned by the hand while Mario is within oCollisionDistance.
#[test]
fn eyerok_hand_owns_collision() {
    load_meshes("ssl.3");
    let mut boss = ObjectSpec::new(ObjectBehaviorKind::EyerokBoss, [0.0, -1534.0, -3693.0], 0.0);
    boss.param = Some((1u32 << 24) as f64);
    let mut m = runtime(ground(-1534.0, 6000.0), vec![boss]);
    stand_at(&mut m, 0.0, -3693.0 + 700.0);
    for _ in 0..4 {
        stand_at(&mut m, 0.0, -3693.0 + 700.0);
        tick(&mut m);
    }
    let hands = find(&m, ObjectBehaviorKind::EyerokHand);
    let owned: usize = hands.iter().map(|h| m.world.surfaces().iter().filter(|s| s.object == Some(h.0)).count()).sum();
    assert!(owned > 4, "sleeping fists have collision ({owned} surfaces)");
}

// ---------------------------------------------------------------------------------------------
// Wiggler (wiggler.inc.c)
// ---------------------------------------------------------------------------------------------

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// `wiggler_init_segments` spawns three body parts; `wiggler_update_segments` keeps them 35 * scale (= 140) apart;
/// the walk waits 60 frames before it can be hurt, three jumps (dialogs 151, 168, 152) shrink it and
/// `spawn_default_star(0, 2048, 0)` follows.
#[test]
fn wiggler_three_stomps_spawn_the_star() {
    let mut head_spec = ObjectSpec::new(ObjectBehaviorKind::WigglerHead, [17.0, 1843.0, -62.0], 0.0);
    head_spec.param = Some((6u32 << 24) as f64);
    let mut m = runtime(ground(1700.0, 7000.0), vec![head_spec]);
    let head = ObjId(0);
    let near = |m: &mut MarioRuntime| {
        stand_at(m, 0.0, 700.0);
        m.invinc_timer = 100.0; // the scripted Mario is not hurt by the wiggler walking into him
    };
    let mut dialog = None;
    for _ in 0..2 {
        near(&mut m);
        for e in tick(&mut m) {
            if let ObjectEvent::KingDialog { id, .. } = e {
                dialog = Some(id);
            }
        }
    }
    assert_eq!(m.objects[head].pos[1], 1700.0, "DROP_TO_FLOOR");
    assert_eq!(m.objects[head].scale, 4.0, "SCALE 400");
    let bodies = find(&m, ObjectBehaviorKind::WigglerBody);
    assert_eq!(bodies.len(), 3);
    assert_eq!(m.objects[head].hitbox.health, 4.0);
    // The intro dialog (DIALOG_150) is shown first, because Mario is not below the wiggler.
    for _ in 0..5 {
        near(&mut m);
        for e in tick(&mut m) {
            if let ObjectEvent::KingDialog { id, .. } = e {
                dialog = Some(id);
            }
        }
    }
    assert_eq!(dialog, Some(150.0));
    m.objects[head].set_var("dialogDone", 1.0);
    // Walk: forward velocity approaches sWigglerSpeeds[4 - 1] = 16 by 1 per frame.
    for _ in 0..30 {
        near(&mut m);
        tick(&mut m);
    }
    assert_eq!(m.objects[head].action, 1);
    assert!((m.objects[head].forward_vel - 16.0).abs() < 0.5, "drag trims the 16: {}", m.objects[head].forward_vel);
    for i in 1..=3 {
        let a = |k: usize| {
            let o = &m.objects[head];
            [o.var(&format!("s{k}x")), o.var(&format!("s{k}y")), o.var(&format!("s{k}z"))]
        };
        let d = dist3(a(i), a(i - 1));
        assert!((d - 140.0).abs() < 1.0, "segment {i} is {d} from the previous one");
    }
    // Invulnerable for the first 60 frames of walking.
    m.objects[head].interact_status = INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED | ATTACK_FROM_ABOVE as u32;
    near(&mut m);
    tick(&mut m);
    assert_eq!(m.objects[head].action, 1, "no reaction while oTimer < 60");
    for _ in 0..60 {
        near(&mut m);
        tick(&mut m);
    }
    let mut star = None;
    let mut dialogs = vec![];
    let mut jumps = 0;
    for _ in 0..4000 {
        near(&mut m);
        if m.objects[head].action == 1 && m.objects[head].timer > 61.0 && m.objects[head].interact_status == 0 && jumps < 3 && star.is_none() {
            m.objects[head].interact_status = INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED | ATTACK_FROM_ABOVE as u32;
            jumps += 1;
        }
        for e in tick(&mut m) {
            match e {
                ObjectEvent::KingDialog { id, .. } => {
                    dialogs.push(id);
                    m.objects[head].set_var("dialogDone", 1.0);
                }
                ObjectEvent::DefaultStar { pos, act } => {
                    assert_eq!(act, 6);
                    star = Some(pos)
                },
                _ => {}
            }
        }
        if star.is_some() {
            break;
        }
    }
    assert_eq!(dialogs, vec![151.0, 168.0, 152.0], "attackText[health - 2]");
    assert_eq!(star, Some([0.0, 2048.0, 0.0]));
    assert_eq!(m.objects[head].hitbox.health, 1.0, "health stays 1 so it does not disappear");
}
