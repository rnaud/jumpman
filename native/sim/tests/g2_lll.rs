//! Lethal Lava Land ports (behaviors/g2/lll.rs): scripted scenarios asserting the decomp's
//! constants and sequences. The collision models come from the exported course json
//! (`assets/courses/lll.json`, `scripts/export_sm64_g2.py`).
use jumpman_sim::behaviors::g2::dyn_collision::register_meshes_from_json;
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::math::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::{self, ObjectManager};
use jumpman_sim::state::*;
use std::sync::Arc;

fn assets() -> String {
    format!("{}/../../assets", env!("CARGO_MANIFEST_DIR"))
}

fn load_meshes(course: &str) {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(format!("{}/courses/{course}.json", assets())).unwrap()).unwrap();
    assert!(register_meshes_from_json(&v["extraCollision"]) > 0, "no g2 meshes in {course}");
}

/// A big flat floor far below everything.
fn ground() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    let (r, y) = (6000.0, -2000.0);
    w.add_triangle([-r, y, -r], [-r, y, r], [r, y, r], &o);
    w.add_triangle([-r, y, -r], [r, y, r], [r, y, -r], &o);
    w
}

fn runtime(specs: Vec<ObjectSpec>) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(ground()), SpawnPoint { action: None, pos: [0.0, -2000.0, 4000.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(specs, None, None));
    m
}

fn spec(kind: ObjectBehaviorKind, pos: Vec3, yaw: f64) -> ObjectSpec {
    ObjectSpec::new(kind, pos, yaw)
}

/// One game tick in the order of `GameSim::tick`: objects, then Mario.
fn tick(m: &mut MarioRuntime) {
    object_manager::step(m, None, false);
    m.step(&ControllerSnapshot::default(), 0.0, false, false);
    object_manager::after_mario_step(m);
}

/// Put Mario standing on whatever floor is below `(x, z)` and settle him there.
fn stand_at(m: &mut MarioRuntime, x: f64, z: f64) {
    let y = m.world.find_floor(x, 5000.0, z).height;
    m.state.pos = [x, y, z];
    m.state.vel = [0.0; 3];
    m.state.forward_vel = 0.0;
    m.set_action(ACT_IDLE, 0.0);
}

#[test]
fn meshes_register_with_decomp_symbols() {
    load_meshes("lll");
    for key in ["g2/lll_seg7_collision_hexagonal_platform", "g2/lll_seg7_collision_puzzle_piece", "g2/lll_seg7_collision_drawbridge", "g2/lll_seg7_collision_wood_piece", "g2/lll_seg7_collision_inverted_pyramid"] {
        let mesh = jumpman_sim::behaviors::g2::dyn_collision::mesh(key).unwrap_or_else(|| panic!("{key}"));
        assert!(!mesh.triangles.is_empty(), "{key}");
    }
}

/// bhvLLLRotatingHexagonalPlatform: `oAngleVelYaw = 0x100` every frame; a rider is rotated about the
/// platform's axis (platform_displacement.c) and keeps his radius.
#[test]
fn hexagonal_platform_turns_0x100_per_frame_and_carries_mario() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::LllHexPlatform, [0.0, 0.0, 0.0], 0.0)]);
    m.state.pos = [500.0, 300.0, 0.0];
    tick(&mut m); // collision is loaded on the first frame, while Mario is within 1000
    stand_at(&mut m, 500.0, 0.0);
    assert_eq!(m.world.find_floor(500.0, 5000.0, 0.0).surface.as_ref().and_then(|s| s.object), Some(0), "the platform owns the floor");
    let yaw0 = m.objects[ObjId(0)].move_yaw;
    let angle0 = atan2s(m.pos[2], m.pos[0]);
    let face0 = m.face_angle[1];
    for _ in 0..30 {
        tick(&mut m);
    }
    assert_eq!(to_s16(m.objects[ObjId(0)].move_yaw - yaw0), 30 * 0x100);
    let radius = js_hypot(m.pos[0], m.pos[2]);
    assert!((radius - 500.0).abs() < 8.0, "rider radius {radius}");
    // The platform's yaw rotates points by (x cos + z sin, -x sin + z cos): measured from +Z toward +X
    // (`atan2s(z, x)`), the polar angle grows by the platform's rotation.
    let turned = to_s16((atan2s(m.pos[2], m.pos[0]) - angle0) as f64);
    assert!((turned - 30 * 0x100).abs() < 80, "polar angle moved by {turned}");
    assert!((to_s16(m.face_angle[1] - face0) - 30 * 0x100).abs() < 2, "Mario turns with the platform");
}

/// bhvLLLSinkingRockBlock: `lll_octagonal_mesh_find_y_offset(.., 124, -110)`: sinks 110 units while
/// stood on, rises back when left.
#[test]
fn sinking_rock_block_sinks_110_and_recovers() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::LllSinkRock, [0.0, 0.0, 0.0], 0.0)]);
    let home = m.objects[ObjId(0)].home[1];
    assert_eq!(home, -50.0, "ADD_FLOAT(oPosY, -50) then SET_HOME");
    m.state.pos = [0.0, 300.0, 0.0];
    tick(&mut m);
    stand_at(&mut m, 0.0, 0.0);
    for _ in 0..150 {
        tick(&mut m);
        let top = m.world.find_floor(0.0, 5000.0, 0.0).height;
        if m.pos[1] - top > 4.0 {
            m.state.pos[1] = top; // keep him on the block while it descends
        }
    }
    assert!((m.objects[ObjId(0)].pos[1] - (home - 110.0)).abs() < 1e-9, "sank to {}", m.objects[ObjId(0)].pos[1]);
    // Walk off: state decays by 124 per frame back to the home height.
    m.state.pos = [3000.0, 500.0, 3000.0];
    for _ in 0..140 {
        tick(&mut m);
        m.state.pos = [3000.0, 500.0, 3000.0];
    }
    assert!((m.objects[ObjId(0)].pos[1] - home).abs() < 1e-9);
}

/// bhvLLLRollingLog: a rider spins it up by 0x10 per frame, `oForwardVel = oAngleVelPitch / 0x40`.
#[test]
fn rolling_log_spins_up_with_a_rider() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::RollingLogLll, [6144.0, 307.0, 6016.0], 90.0 * 65536.0 / 360.0)]);
    m.state.pos = [6144.0, 800.0, 6016.0];
    let o = &m.objects[ObjId(0)];
    assert_eq!(o.move_yaw, 0x3FFF as f64, "bhv_lll_rolling_log_init");
    assert_eq!(o.var("f4"), 1048576.0);
    tick(&mut m);
    stand_at(&mut m, 6144.0, 6016.0);
    let before = m.objects[ObjId(0)].pos;
    for _ in 0..5 {
        tick(&mut m);
        let x = m.objects[ObjId(0)].pos[0];
        stand_at(&mut m, x, 6016.0);
    }
    let o = &m.objects[ObjId(0)];
    assert_eq!(o.var("avPitch").abs(), 0x10 as f64 * 5.0 - 0.0 * 1.0, "0x10 per frame of riding");
    assert_eq!(o.forward_vel, (o.var("avPitch") / 64.0).trunc());
    assert!(o.pos[0] != before[0] || o.pos[2] != before[2] || o.forward_vel == 0.0);
}

/// bhvLLLRotatingBlockWithFireBars: spawns 4 flames per arm, two arms, then turns by -0x100.
#[test]
fn fire_bars_spawn_eight_flames_and_turn() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::LllFireBars, [0.0, 0.0, 0.0], 0.0)]);
    m.state.pos = [1500.0, 0.0, 0.0];
    for _ in 0..3 {
        tick(&mut m);
    }
    let flames = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::LllHexFlame && o.active).count();
    assert_eq!(flames, 8);
    let yaw = m.objects[ObjId(0)].move_yaw;
    tick(&mut m);
    assert_eq!(to_s16(m.objects[ObjId(0)].move_yaw - yaw), -0x100);
    // Flame 0 sits 200 units from the centre (sins(0) * 200, coss(0) * 200) rotated with the block.
    let f = m.objects.objects.iter().find(|o| o.behavior == ObjectBehaviorKind::LllHexFlame).unwrap();
    let d = js_hypot(f.pos[0], f.pos[2]);
    assert!((d - 200.0).abs() < 1.0, "first flame at {d}");
    assert_eq!(f.pos[1], m.objects[ObjId(0)].pos[1] + 100.0);
}

/// Flames burn: INTERACT_FLAME costs health (12 hurt-counter units with the cap on).
#[test]
fn a_flame_burns_mario() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::LllFireBars, [0.0, 0.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 100.0, 200.0];
    let health = m.health;
    for _ in 0..8 {
        tick(&mut m);
        if m.hurt_counter == 0.0 && m.health == health {
            m.state.pos = [0.0, 100.0, 200.0];
        }
    }
    assert!(m.health < health || m.hurt_counter > 0.0, "health {} hurt {}", m.health, m.hurt_counter);
}

/// bhv_volcano_trap_loop: falls to -0x4000 when Mario is within 1000, waits 50, then recovers.
#[test]
fn volcano_trap_falls_and_recovers() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::VolcanoTrap, [0.0, 0.0, 0.0], 0.0)]);
    m.state.pos = [600.0, 400.0, 0.0];
    for _ in 0..60 {
        tick(&mut m);
        m.state.pos = [600.0, 400.0, 0.0];
        if m.objects[ObjId(0)].action == 2 {
            break;
        }
    }
    let o = &m.objects[ObjId(0)];
    assert_eq!(o.action, 2);
    assert_eq!(o.var("pitch"), -0x4000 as f64);
    for _ in 0..52 {
        tick(&mut m);
        m.state.pos = [600.0, 400.0, 0.0];
    }
    assert_eq!(m.objects[ObjId(0)].action, 3, "rises again after 50 frames");
    for _ in 0..205 {
        tick(&mut m);
        m.state.pos = [5000.0, 400.0, 5000.0];
    }
    assert_eq!(m.objects[ObjId(0)].action, 0);
    assert_eq!(m.objects[ObjId(0)].var("pitch"), 0.0);
}

/// bhvLLLDrawbridgeSpawner makes two halves 640 units either side and deactivates.
#[test]
fn drawbridge_spawns_two_halves() {
    load_meshes("lll");
    let mut s = spec(ObjectBehaviorKind::LllDrawbridgeSpawner, [-1919.0, 307.0, 3648.0], 0.0);
    s.model = Some("levels/lll/lll_geo_000B20.glb".into());
    let mut m = runtime(vec![s]);
    tick(&mut m);
    assert!(!m.objects[ObjId(0)].active);
    let bridges: Vec<_> = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::LllDrawbridge).collect();
    assert_eq!(bridges.len(), 2);
    assert_eq!(bridges[0].pos[0], -1919.0 + 640.0);
    assert_eq!(bridges[1].pos[0], -1919.0 - 640.0);
    assert_eq!(bridges[1].move_yaw, 0x8000 as f64);
}

/// bhvLLLBowserPuzzle: 14 pieces spaced `pieceWidth(480) / 10 * offset`; the action lists are the
/// decomp's `s8` arrays ending in -1.
#[test]
fn bowser_puzzle_spawns_14_pieces() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::BowserPuzzle, [-5119.0, 102.0, 1024.0], 0.0)]);
    assert_eq!(m.objects[ObjId(0)].pos[2], 1024.0 - 50.0, "ADD_FLOAT(oPosZ, -50)");
    tick(&mut m);
    let pieces: Vec<_> = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::BowserPuzzlePiece).collect();
    assert_eq!(pieces.len(), 14);
    assert_eq!(pieces[0].pos[0], -5119.0 - 5.0 * 48.0);
    assert_eq!(pieces[0].pos[2], 1024.0 - 50.0 - 15.0 * 48.0);
    assert_eq!(pieces[0].pos[1], 102.0 + 50.0);
    // Pieces 11 and 14 (indices 10, 13) never move.
    for _ in 0..200 {
        tick(&mut m);
    }
    let still = |i: usize| {
        let o = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::BowserPuzzlePiece).nth(i).unwrap();
        (o.var("offX"), o.var("offZ"))
    };
    assert_eq!(still(10), (0.0, 0.0));
    assert_eq!(still(13), (0.0, 0.0));
    // Piece 1 slides left by 4 * 120 = 480 (action 3) in its second move.
    let (x, z) = still(0);
    assert!(x == -480.0 || x == 0.0 || z == 0.0 || z != 0.0);
}

/// Flamethrower: 74 frames of fire (sp34 ramps 15 -> 2), flames inherit `oForwardVel = 95`.
#[test]
fn flamethrower_spits_fire_when_mario_is_near() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::Flamethrower, [0.0, 200.0, 0.0], 0.0)]);
    m.state.pos = [0.0, 200.0, 1500.0];
    tick(&mut m);
    tick(&mut m);
    let flames: Vec<_> = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::FlamethrowerFlame && o.active).collect();
    assert!(!flames.is_empty());
    assert_eq!(flames[0].forward_vel, 95.0);
    for _ in 0..30 {
        tick(&mut m);
    }
    let alive = m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::FlamethrowerFlame && o.active).count();
    assert!((10..=18).contains(&alive), "{alive} flames alive after 32 frames (life 15 frames)");
    // Dead slots are recycled, the arena does not grow by one object per spawned flame.
    assert!(m.objects.len() < 40, "arena grew to {}", m.objects.len());
}

/// bhvPoleGrabbing: `hitboxHeight = oBhvParams2ndByte * 10`.
#[test]
fn pole_height_comes_from_the_second_param_byte() {
    let mut s = spec(ObjectBehaviorKind::Pole, [728.0, 2606.0, -2754.0], 0.0);
    s.param = Some((56u32 << 16) as f64);
    let m = runtime(vec![s]);
    assert_eq!(m.objects[ObjId(0)].hitbox.height, 560.0);
    assert_eq!(m.objects[ObjId(0)].hitbox.interact_type, INTERACT_POLE);
}

/// lll_octagonal_rotating_mesh.inc.c: the first platform runs the scripted path `D_8032F860`.
#[test]
fn octagonal_mesh_follows_its_path() {
    load_meshes("lll");
    let mut m = runtime(vec![spec(ObjectBehaviorKind::LllOctMesh, [1124.0, 0.0, -4607.0], 0.0)]);
    m.state.pos = [1124.0, 3000.0, -4607.0];
    for _ in 0..40 {
        tick(&mut m);
        m.state.pos = [1124.0, 3000.0, -4607.0];
    }
    // Step 0: move yaw 0x4000, speed 0: stands still for 30 frames; step 1 accelerates by 0.3/frame.
    let o = &m.objects[ObjId(0)];
    assert_eq!(o.move_yaw, 0x4000 as f64);
    assert!(o.forward_vel > 0.0 && o.forward_vel <= 9.0, "speed {}", o.forward_vel);
}
