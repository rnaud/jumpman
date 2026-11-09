//! Shifting Sand Land, Wet-Dry World and Tiny-Huge Island ports (behaviors/g2/{ssl,wdw,thi}.rs):
//! scripted scenarios against the decomp's constants. Collision models come from the exported
//! course jsons.
use jumpman_sim::behaviors::g2::dyn_collision::register_meshes_from_json;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectEvent, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

fn load_meshes(course: &str) {
    let path = format!("{}/../../assets/courses/{course}.json", env!("CARGO_MANIFEST_DIR"));
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(register_meshes_from_json(&v["extraCollision"]) > 0, "no g2 meshes in {course}");
}

fn ground(y: f64, boxes: Vec<WaterBox>) -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let r = 6000.0;
    w.add_triangle([-r, y, -r], [-r, y, r], [r, y, r], &o);
    w.add_triangle([-r, y, -r], [r, y, r], [r, y, -r], &o);
    w.water_boxes = boxes;
    w
}

fn runtime(world: CollisionWorld, specs: Vec<ObjectSpec>) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

fn spec(kind: ObjectBehaviorKind, pos: Vec3, yaw: f64) -> ObjectSpec {
    ObjectSpec::new(kind, pos, yaw)
}

fn tick(m: &mut MarioRuntime) {
    object_manager::step(m, None, false);
    m.step(&ControllerSnapshot::default(), 0.0, false, false);
    object_manager::after_mario_step(m);
}

fn stand_at(m: &mut MarioRuntime, x: f64, z: f64) {
    let y = m.world.find_floor(x, 9000.0, z).height;
    m.state.pos = [x, y, z];
    m.state.vel = [0.0; 3];
    m.state.forward_vel = 0.0;
    m.set_action(ACT_IDLE, 0.0);
}

// ---------------------------------------------------------------------------------------------
// Shifting Sand Land
// ---------------------------------------------------------------------------------------------

/// tox_box.inc.c: `sToxBoxActionTable1` starts FORWARD (roll 8 frames at 64 units), LAND (21
/// frames), FORWARD again.
#[test]
fn tox_box_rolls_forward_then_lands_in_the_decomp_rhythm() {
    load_meshes("ssl");
    let mut m = runtime(ground(-2000.0, vec![]), vec![spec(ObjectBehaviorKind::ToxBox, [0.0, 0.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 500.0, 300.0];
    let home_y = m.objects[ObjId(0)].home[1];
    assert_eq!(home_y, 256.0, "ADD_FLOAT(oPosY, 256), SET_HOME");
    // Frame 0 runs tox_box_act_init (picks table[0] = FORWARD), then 8 rolling frames.
    for _ in 0..9 {
        tick(&mut m);
        m.state.pos = [0.0, 500.0, 300.0];
    }
    assert_eq!(m.objects[ObjId(0)].pos[2], 8.0 * 64.0);
    assert_eq!(m.objects[ObjId(0)].action, 1, "TOX_BOX_ACT_ROLL_LAND");
    assert_eq!(m.objects[ObjId(0)].pos[1], home_y + 3.0 + 0.0 * 99.41124, "lands at oHomeY + 3");
    for _ in 0..21 + 8 {
        tick(&mut m);
        m.state.pos = [0.0, 500.0, 300.0];
    }
    assert_eq!(m.objects[ObjId(0)].pos[2], 16.0 * 64.0, "second FORWARD of the table");
    // Pitch advanced 0x800 per rolling frame: 16 frames = a half turn.
    assert_eq!(to_s16(m.objects[ObjId(0)].var("pitch")), to_s16(16.0 * 0x800 as f64));
}

/// bhvSSLMovingPyramidWall: the middle walls start 256 down with `oTimer = 50`, so they flip after 50 frames.
#[test]
fn pyramid_walls_move_5_12_per_frame() {
    load_meshes("ssl.2");
    let mut s = spec(ObjectBehaviorKind::PyramidWall, [0.0, 1000.0, 0.0], 0.0);
    s.param = Some((1u32 << 16) as f64); // PYRAMID_WALL_BP_POSITION_MIDDLE
    let mut m = runtime(ground(-2000.0, vec![]), vec![s]);
    assert_eq!(m.objects[ObjId(0)].pos[1], 1000.0 - 256.0);
    m.state.pos = [0.0, 0.0, 600.0];
    for _ in 0..50 {
        tick(&mut m);
        m.state.pos = [0.0, 0.0, 600.0];
    }
    let y = m.objects[ObjId(0)].pos[1];
    assert!((y - (1000.0 - 256.0 - 50.0 * 5.12)).abs() < 1e-6, "moved down 50 frames: {y}");
    tick(&mut m); // oTimer == 100 flips the direction
    assert_eq!(m.objects[ObjId(0)].action, 1, "now moving up");
}

/// bhvPyramidElevator: waits for Mario, jolts for 8 frames, then descends 10 per frame to y = 128.
#[test]
fn pyramid_elevator_descends_to_128() {
    load_meshes("ssl.2");
    let mut m = runtime(ground(-2000.0, vec![]), vec![spec(ObjectBehaviorKind::PyramidElevator, [0.0, 600.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 700.0, 0.0];
    tick(&mut m);
    stand_at(&mut m, 0.0, 0.0);
    assert!(m.world.find_floor(0.0, 9000.0, 0.0).surface.as_ref().is_some_and(|s| s.object == Some(0)), "Mario stands on the elevator");
    for _ in 0..30 {
        tick(&mut m);
    }
    assert!(m.objects[ObjId(0)].action >= 1, "started moving once ridden");
    for _ in 0..80 {
        tick(&mut m);
    }
    assert!(m.objects[ObjId(0)].pos[1] < 600.0 - 100.0);
    // Mario is carried down with it.
    assert!((m.pos[1] - m.objects[ObjId(0)].pos[1]).abs() < 30.0, "mario {} lift {}", m.pos[1], m.objects[ObjId(0)].pos[1]);
}

/// bhvPyramidTop: standing on all four pillars (the touch detectors) starts the spin, and after
/// 150 frames the top explodes into 30 fragments.
#[test]
fn pyramid_top_solves_with_four_pillars() {
    load_meshes("ssl");
    let mut m = runtime(ground(-2000.0, vec![]), vec![spec(ObjectBehaviorKind::PyramidTop, [-2047.0, 1536.0, -1023.0], 0.0)]);
    tick(&mut m);
    let detectors: Vec<Vec3> = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::PyramidPillarDetector).map(|o| o.pos).collect();
    assert_eq!(detectors.len(), 4);
    assert_eq!(detectors[0], [1789.0, 1024.0, 764.0]);
    for d in &detectors {
        m.state.pos = *d;
        tick(&mut m);
    }
    m.state.pos = [0.0, 5000.0, 0.0];
    tick(&mut m);
    assert_eq!(m.objects[ObjId(0)].var("touched"), 4.0);
    tick(&mut m);
    assert_eq!(m.objects[ObjId(0)].action, 1, "PYRAMID_TOP_ACT_SPINNING");
    for _ in 0..160 {
        m.state.pos = [0.0, 5000.0, 0.0];
        tick(&mut m);
    }
    assert!(!m.objects[ObjId(0)].active, "exploded");
    let fragments = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::PyramidTopFragment).count();
    assert!(fragments >= 30, "{fragments} fragments");
}

/// bhvGrindel: raises for `oBhvParams2ndByte + 40` frames at 10 per frame (then 5), waits at the top,
/// and slams back down with gravity -4.
#[test]
fn grindel_raises_then_drops() {
    load_meshes("ssl.2");
    let mut s = spec(ObjectBehaviorKind::Grindel, [0.0, 0.0, 0.0], 0.0);
    s.param = Some((28u32 << 16) as f64);
    let mut m = runtime(ground(0.0, vec![]), vec![s]);
    m.state.pos = [0.0, 0.0, 700.0];
    tick(&mut m);
    let home = m.objects[ObjId(0)].home[1];
    assert_eq!(home, 1.0, "DROP_TO_FLOOR then ADD_FLOAT(oPosY, 1)");
    let mut top = 0.0f64;
    for _ in 0..200 {
        m.state.pos = [0.0, 0.0, 700.0];
        tick(&mut m);
        top = top.max(m.objects[ObjId(0)].pos[1]);
    }
    // 69 frames of +10 then one +5 step.
    assert_eq!(top, home + 69.0 * 10.0 + 5.0);
}

/// bhvSpindel: the first segment of its schedule moves 5 units of Z per frame (20 / 4) and spins 256.
#[test]
fn spindel_first_move() {
    load_meshes("ssl.2");
    let mut m = runtime(ground(-2000.0, vec![]), vec![spec(ObjectBehaviorKind::Spindel, [0.0, 500.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 500.0, 600.0];
    tick(&mut m);
    let o = &m.objects[ObjId(0)];
    assert_eq!(o.vel[2], 5.0);
    assert_eq!(o.var("avPitch"), 256.0);
}

// ---------------------------------------------------------------------------------------------
// Wet-Dry World
// ---------------------------------------------------------------------------------------------

fn wdw_world() -> CollisionWorld {
    ground(-2000.0, vec![WaterBox(0.0, -3839.0, -3839.0, 4608.0, 4608.0, 31.0)])
}

/// wdw_water_level.inc.c: touching a diamond moves the water level 10 units per frame to the
/// diamond's height; the level keeps bobbing by +-20 (`sins(oWaterLevelTriggerUnkF4) * 20`).
#[test]
fn water_level_diamond_raises_the_water() {
    load_meshes("wdw");
    let mut m = runtime(
        wdw_world(),
        vec![spec(ObjectBehaviorKind::WaterLevelInit, [0.0, 0.0, 0.0], 0.0), spec(ObjectBehaviorKind::WaterLevelDiamond, [500.0, 1024.0, 0.0], 0.0)],
    );
    // The init object samples the level for 10 frames, the diamond waits 10 frames.
    for _ in 0..14 {
        m.state.pos = [0.0, 3000.0, 3000.0];
        tick(&mut m);
    }
    assert_eq!(m.objects[ObjId(1)].action, 1, "WATER_LEVEL_DIAMOND_ACT_IDLE");
    let before = m.world.water_boxes[0].5;
    assert!((before - 31.0).abs() <= 20.0);
    m.state.pos = [500.0, 1024.0, 0.0];
    tick(&mut m);
    assert_eq!(m.objects[ObjId(1)].action, 2);
    let mut last = 31.0;
    for i in 0..120 {
        m.state.pos = [500.0, 1024.0, 0.0];
        tick(&mut m);
        let level = m.world.water_boxes[0].5;
        if i == 5 {
            assert!(level > before + 30.0, "rising: {level}");
        }
        last = level;
    }
    assert!((last - 1024.0).abs() <= 20.0, "settled at {last}");
    // The diamond keeps spinning until it is back at yaw 0 (0x800 per frame), then idles.
    assert!(m.objects[ObjId(1)].action == 3 || m.objects[ObjId(1)].action == 2);
    // Water at the new level: Mario's `water_level` follows.
    assert!((m.world.water_level(500.0, 0.0) - last).abs() < 1e-9);
}

/// arrow_lift.inc.c: waits 61 frames, then a rider moves it 384 units (12 per frame) perpendicular
/// to its facing, waits again and returns.
#[test]
fn arrow_lift_carries_mario_384_units() {
    load_meshes("wdw");
    let mut m = runtime(wdw_world(), vec![spec(ObjectBehaviorKind::ArrowLift, [0.0, 500.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 600.0, 0.0];
    tick(&mut m);
    stand_at(&mut m, 0.0, 0.0);
    for _ in 0..140 {
        tick(&mut m);
    }
    let o = &m.objects[ObjId(0)];
    assert!((o.pos[0] + 384.0).abs() < 1e-9 || o.action == 2, "moved to x = {} (action {})", o.pos[0], o.action);
    assert!((m.pos[0] - o.pos[0]).abs() < 60.0, "rider follows: mario {} lift {}", m.pos[0], o.pos[0]);
}

/// express_elevator.inc.c: 133 frames at -20, wait 110, back up at +10.
#[test]
fn express_elevator_goes_down_and_comes_back() {
    load_meshes("wdw");
    let mut m = runtime(wdw_world(), vec![spec(ObjectBehaviorKind::ExpressElevator, [0.0, 3000.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 3100.0, 0.0];
    tick(&mut m);
    stand_at(&mut m, 0.0, 0.0);
    let mut min_y = f64::MAX;
    for _ in 0..140 {
        tick(&mut m);
        min_y = min_y.min(m.objects[ObjId(0)].pos[1]);
    }
    // The move runs on frames with oTimer 0..=133 (the action changes after `oTimer > 132` was checked).
    assert_eq!(min_y, 3000.0 - 20.0 * 134.0);
    m.state.pos = [3000.0, 0.0, 3000.0];
    for _ in 0..400 {
        tick(&mut m);
        m.state.pos = [3000.0, 0.0, 3000.0];
    }
    assert_eq!(m.objects[ObjId(0)].pos[1], 3000.0, "back at the top");
    assert_eq!(m.objects[ObjId(0)].action, 0);
}

/// switch_hidden_objects.inc.c: the purple switch makes the hidden platforms appear for 360 frames.
#[test]
fn hidden_platforms_appear_while_the_switch_ticks() {
    load_meshes("wdw");
    let mut p = spec(ObjectBehaviorKind::HiddenObject, [800.0, 300.0, 0.0], 0.0);
    p.param = Some((1u32 << 16) as f64);
    let mut m = runtime(wdw_world(), vec![spec(ObjectBehaviorKind::FloorSwitchHidden, [0.0, 0.0, 0.0], 0.0), p]);
    m.state.pos = [0.0, 300.0, 0.0];
    tick(&mut m);
    assert!(!m.objects[ObjId(1)].visible, "hidden until the switch is pressed");
    stand_at(&mut m, 0.0, 0.0);
    for _ in 0..20 {
        tick(&mut m);
        let _ = m.pos;
    }
    assert_eq!(m.objects[ObjId(0)].action, 2, "PURPLE_SWITCH_ACT_TICKING");
    assert!(m.objects[ObjId(1)].visible && m.objects[ObjId(1)].action == 1);
    m.state.pos = [3000.0, 0.0, 3000.0];
    for _ in 0..400 {
        m.state.pos = [3000.0, 0.0, 3000.0];
        tick(&mut m);
    }
    assert_eq!(m.objects[ObjId(1)].action, 0, "vanished after 360 frames + 20 blinks");
}

// ---------------------------------------------------------------------------------------------
// Tiny-Huge Island
// ---------------------------------------------------------------------------------------------

/// thi_top.inc.c: ground pounding the tiny island's top drains its pool by 50 and then sets 700; the
/// huge island's plug disappears and its pool falls to 3000.
#[test]
fn tiny_top_drains_the_huge_pool() {
    use jumpman_sim::behaviors::g2::thi;
    load_meshes("thi");
    thi::reset_water_drained();
    let tiny_boxes = vec![WaterBox(0.0, -2457.0, -2457.0, -1535.0, 1874.0, -921.0), WaterBox(1.0, -1381.0, 1352.0, 2089.0, 2458.0, -921.0), WaterBox(2.0, -306.0, -766.0, 307.0, -152.0, 1204.0)];
    let mut tiny = runtime(ground(-2000.0, tiny_boxes), vec![spec(ObjectBehaviorKind::ThiTinyIslandTop, [0.0, 1167.0, -460.0], 0.0)]);
    tiny.state.pos = [0.0, 1167.0, -400.0];
    tick(&mut tiny);
    assert!(!thi::water_drained());
    tiny.state.pos = [0.0, 1167.0, -400.0];
    tiny.state.action = ACT_GROUND_POUND_LAND;
    object_manager::step(&mut tiny, None, false);
    assert_eq!(tiny.objects[ObjId(0)].action, 1);
    for _ in 0..40 {
        tiny.state.pos = [0.0, 1167.0, -400.0];
        tick(&mut tiny);
    }
    assert!(thi::region_y(&tiny, 2).unwrap() < 1204.0 && thi::region_y(&tiny, 2).unwrap() > 1150.0);
    for _ in 0..30 {
        tiny.state.pos = [0.0, 1167.0, -400.0];
        tick(&mut tiny);
    }
    assert!(thi::water_drained());
    assert_eq!(thi::region_y(&tiny, 2), Some(700.0));

    let huge_boxes = vec![WaterBox(0.0, -8191.0, -8191.0, -5119.0, 6246.0, -3071.0), WaterBox(1.0, -4607.0, 4506.0, 6963.0, 8192.0, -3071.0), WaterBox(2.0, -1023.0, -2555.0, 1024.0, -507.0, 4014.0)];
    let mut huge = runtime(ground(-2000.0, huge_boxes), vec![spec(ObjectBehaviorKind::ThiHugeIslandTop, [0.0, 3891.0, -1533.0], 0.0)]);
    huge.state.pos = [0.0, 3891.0, -1533.0];
    tick(&mut huge);
    assert!(!huge.objects[ObjId(0)].visible, "the plug is gone once drained");
    assert_eq!(thi::region_y(&huge, 2), Some(3000.0));
    thi::reset_water_drained();
}

/// fire_piranha_plant.inc.c: a plant grows to its neutral scale (2.0 for `oBhvParams2ndByte = 1`) when
/// Mario is 100..800 away, and the fifth killed plant spawns the star at (-6300, -1850, -6300).
#[test]
fn fire_piranha_plants_grow_and_the_fifth_kill_drops_the_star() {
    load_meshes("thi");
    let spots = [[-6336.0, -2047.0, -3861.0], [-5740.0, -2047.0, -6578.0], [-6481.0, -2047.0, -5998.0], [-5577.0, -2047.0, -4961.0], [-6865.0, -2047.0, -4568.0]];
    let specs: Vec<ObjectSpec> = spots
        .iter()
        .map(|p| {
            let mut s = spec(ObjectBehaviorKind::FirePiranhaPlant, *p, 0.0);
            s.param = Some((1u32 << 16) as f64);
            s
        })
        .collect();
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-8000.0, -2047.0, -8000.0], [-8000.0, -2047.0, 0.0], [0.0, -2047.0, 0.0], &o);
    w.add_triangle([-8000.0, -2047.0, -8000.0], [0.0, -2047.0, 0.0], [0.0, -2047.0, -8000.0], &o);
    let mut m = runtime(w, specs);
    assert_eq!(m.objects[ObjId(0)].var("neutral"), 2.0);
    assert_eq!(m.objects[ObjId(0)].hitbox.interact_type, jumpman_sim::behaviors::g2::interact::INTERACT_BOUNCE_TOP);
    let mut boss_pos = None;
    let mut grew = false;
    for frame in 0..4000 {
        // Mario circles at 400 units from the plant that is awake or the next one to kill.
        let target = (0..5).find(|i| m.objects[ObjId(*i as u32)].active).unwrap_or(0);
        let p = m.objects[ObjId(target as u32)].pos;
        m.state.pos = [p[0] + 400.0, p[1], p[2]];
        m.invinc_timer = 100.0;
        object_manager::step(&mut m, None, false);
        for e in m.objects.drain_events() {
            // spawn_default_star with the plants' BPARAM1 (STAR_INDEX_ACT_1): the act 1 star.
            if let ObjectEvent::DefaultStar { pos, act } = e {
                boss_pos = Some((pos, act));
            }
        }
        grew |= m.objects.objects.iter().any(|o| o.behavior == ObjectBehaviorKind::FirePiranhaPlant && o.var("scale") >= 2.0);
        // Stomp whichever plant is fully grown.
        for i in 0..5u32 {
            let o = &mut m.objects[ObjId(i)];
            if o.active && o.action == 1 && o.var("scale") >= 2.0 && frame % 7 == 0 {
                o.interact_status = INT_STATUS_WAS_ATTACKED | INT_STATUS_INTERACTED;
            }
        }
        if boss_pos.is_some() {
            break;
        }
    }
    assert!(grew, "a plant reached its neutral scale 2.0");
    assert_eq!(boss_pos, Some(([-6300.0, -1850.0, -6300.0], 1)));
}

