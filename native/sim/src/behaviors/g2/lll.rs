//! Lethal Lava Land objects (src/game/behaviors/lll_*.inc.c, bowser_puzzle_piece.inc.c,
//! rolling_log.inc.c, drawbridge.inc.c, flamethrower.inc.c, bouncing_fireball.inc.c,
//! tilting_inverted_pyramid.inc.c, pole.inc.c). Object flags and collision models come from
//! `data/behavior_data.c`; every function cites the decomp function it is a port of.
use super::dyn_collision::*;
use super::interact::INTERACT_FLAME;
use super::obj::*;
use super::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub const RED_FLAME: &str = "actors/flame/red_flame_geo.glb";
pub const BLUE_FLAME: &str = "actors/flame/blue_flame_geo.glb";

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

/// `oBhvParams2ndByte`.
pub fn bparam2(o: &SimObject) -> i32 {
    ((o.param as u32) >> 16 & 0xFF) as i32
}

/// `cur_obj_change_action`.
pub fn change_action(o: &mut SimObject, action: i32) {
    o.action = action;
    o.prev_action = action;
    o.timer = 0.0;
    o.action_state = 0.0;
}

/// `SET_HITBOX_WITH_OFFSET(radius, height, down_offset)` with an interact type.
pub fn hitbox(interact_type: u32, radius: f64, height: f64, down_offset: f64) -> ObjectHitbox {
    ObjectHitbox { interact_type, down_offset, damage_or_coin_value: 1.0, health: 0.0, num_loot_coins: 0.0, radius, height, hurtbox_radius: radius, hurtbox_height: height }
}

/// A flame: `INTERACT_FLAME` hitbox, billboard.
fn init_flame_object(o: &mut SimObject, radius: f64, height: f64, down: f64) {
    o.hitbox = hitbox(INTERACT_FLAME, radius, height, down);
    o.billboard = true;
    o.visible = true;
}

// ---------------------------------------------------------------------------------------------
// Rotating hexagonal platform (bhvLLLRotatingHexagonalPlatform) and ring
// ---------------------------------------------------------------------------------------------

const K_HEX_PLATFORM: &str = "g2/lll_seg7_collision_hexagonal_platform";

pub fn init_plain(o: &mut SimObject) {
    o.visible = true;
}

/// behavior_data.c bhvLLLRotatingHexagonalPlatform: `oAngleVelYaw = 0x100; oMoveAngleYaw += 0x100`.
pub fn hex_platform(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let o = &mut m.objects[id];
        o.set_var("avYaw", 0x100 as f64);
        o.move_yaw = s16(o.move_yaw + 0x100 as f64);
        load_object_collision_model(m, id, K_HEX_PLATFORM, 1000.0);
    });
}

const K_HEX_RING: &str = "g2/lll_seg7_collision_rotating_platform";

/// lll_hexagonal_ring.inc.c `hexagonal_ring_spawn_flames`.
fn hexagonal_ring_spawn_flames(m: &mut MarioRuntime, id: ObjId) {
    let f = spawn_child(m, id, ObjectBehaviorKind::VolcanoFlames, Some(RED_FLAME), 0.0);
    let c = &mut m.objects[f];
    c.pos[1] += 550.0;
    c.move_yaw = random_u16();
    c.forward_vel = random_float() * 40.0 + 20.0;
    c.vel[1] = random_float() * 50.0 + 10.0;
    c.scale = random_float() * 6.0 + 3.0;
}

/// lll_hexagonal_ring.inc.c `bhv_lll_rotating_hexagonal_ring_loop`.
pub fn hex_ring(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, riding| {
        let (action, timer) = {
            let o = &m.objects[id];
            (o.action, o.timer)
        };
        let mut av = m.objects[id].var("avYaw");
        match action {
            0 => {
                if riding {
                    m.objects[id].action += 1;
                }
                av = 0x100 as f64;
            }
            1 => {
                av = (256.0 - sins(timer * 128.0) * 256.0).trunc();
                if timer > 128.0 {
                    m.objects[id].action += 1;
                }
            }
            2 => {
                if !riding {
                    m.objects[id].action += 1;
                }
                if timer > 128.0 {
                    m.objects[id].action += 1;
                }
                av = 0.0;
                hexagonal_ring_spawn_flames(m, id);
            }
            3 => {
                av = (sins(timer * 128.0) * 256.0).trunc();
                if timer > 128.0 {
                    m.objects[id].action = 0;
                }
            }
            4 => m.objects[id].action = 0,
            _ => {}
        }
        let o = &mut m.objects[id];
        av = -av;
        o.set_var("avYaw", av);
        o.move_yaw = s16(o.move_yaw + av);
        load_object_collision_model(m, id, K_HEX_RING, 4000.0);
    });
}

pub fn init_volcano_flames(o: &mut SimObject) {
    o.billboard = true;
    o.visible = true;
    phys::set_physics(o, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
}

/// lll_volcano_flames.inc.c `bhv_volcano_flames_loop`.
pub fn volcano_flames(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        let o = &mut m.objects[id];
        phys::update_floor_height(o, &world);
        cur_obj_compute_vel_xz(o);
        o.pos[0] += o.vel[0];
        o.pos[2] += o.vel[2];
        phys::move_y(o, &world, -4.0, -0.7, 2.0);
        if phys::flags(o) & (phys::MASK_ON_GROUND | phys::MOVE_AT_WATER_SURFACE | phys::MOVE_UNDERWATER_OFF_GROUND) != 0 {
            mark_for_deletion(o);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Sinking / tilting platforms
// ---------------------------------------------------------------------------------------------

const K_SLOW_TILT: &str = "g2/lll_seg7_collision_slow_tilting_platform";
const K_SINK_PYRAMIDS: &str = "g2/lll_seg7_collision_sinking_pyramids";

/// lll_sinking_rectangle.inc.c `sinking_rectangular_plat_actions`.
fn sinking_rectangular_plat_actions(o: &mut SimObject, a0: f64, a1: f64) {
    match o.action {
        0 => o.action += 1,
        1 => {
            o.pos[1] -= sins(o.var("osc")) * a0;
            let t = o.var("osc") + a1;
            o.set_var("osc", t);
        }
        _ => {}
    }
}

/// bhvLLLSinkingRectangularPlatform: `oCollisionDistance = 2000`, SET_HOME.
pub fn sink_rect(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let o = &mut m.objects[id];
        // bhv_lll_sinking_rectangular_platform_loop
        if o.move_yaw != 0.0 && s16(o.move_yaw) != 0.0 {
            sinking_rectangular_plat_actions(o, 0.4, 0x100 as f64);
        } else {
            let osc = o.var("osc");
            o.set_var("pitch", (sins(osc) * 0x200 as f64).trunc());
            o.set_var("osc", osc + 0x100 as f64);
        }
        load_object_collision_model(m, id, K_SLOW_TILT, 2000.0);
    });
}

pub fn init_sink_square(o: &mut SimObject) {
    o.visible = true;
    o.pos[1] += 5.0;
    o.home = o.pos;
}

/// bhvLLLSinkingSquarePlatforms: `bhv_lll_sinking_square_platforms_loop`.
pub fn sink_square(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        sinking_rectangular_plat_actions(&mut m.objects[id], 0.5, 0x100 as f64);
        load_object_collision_model(m, id, K_SINK_PYRAMIDS, 2000.0);
    });
}

/// lll_octagonal_rotating_mesh.inc.c `lll_octagonal_mesh_find_y_offset`.
fn octagonal_find_y_offset(o: &mut SimObject, state_key: &str, y_key: &str, riding: bool, step: f64, amplitude: f64) -> bool {
    let mut s = o.var(state_key);
    if riding {
        if s < 0x4000 as f64 {
            s += step;
        } else {
            s = 0x4000 as f64;
        }
    } else if s > 0.0 {
        s -= step;
    } else {
        s = 0.0;
    }
    o.set_var(state_key, s);
    o.set_var(y_key, sins(s) * amplitude);
    s == 0.0 || s == 0x4000 as f64
}

const D_8032F860: [i32; 26] = [2, 30, 0x4000, 0, 1, 220, 900, 30, 1, 30, 0, -30, 2, 30, -0x4000, 0, 1, 220, 900, 30, 1, 30, 0, -30, 3, 0];
const D_8032F894: [i32; 26] = [4, 0, 0, 0, 1, 475, 900, 30, 1, 30, 0, -30, 2, 30, -0x8000, 0, 1, 475, 900, 30, 1, 30, 0, -30, 3, 0];

/// lll_octagonal_rotating_mesh.inc.c `lll_octagonal_mesh_move`.
fn octagonal_mesh_move(o: &mut SimObject, path: &[i32; 26], mut a1: usize, riding: bool) -> usize {
    match path[a1] {
        4 => {
            o.move_yaw = path[a1 + 2] as f64;
            o.forward_vel = path[a1 + 3] as f64 / 100.0;
            if riding {
                a1 += 4;
                o.timer = 0.0;
            }
        }
        2 => {
            o.move_yaw = path[a1 + 2] as f64;
            o.forward_vel = path[a1 + 3] as f64 / 100.0;
            if o.timer > path[a1 + 1] as f64 {
                a1 += 4;
                o.timer = 0.0;
            }
        }
        1 => {
            let mut v = o.forward_vel;
            approach_f32_signed(&mut v, path[a1 + 2] as f64 / 100.0, path[a1 + 3] as f64 / 100.0);
            o.forward_vel = v;
            if o.timer > path[a1 + 1] as f64 {
                a1 += 4;
                o.timer = 0.0;
            }
        }
        3 => {
            o.forward_vel = 0.0;
            a1 = 0;
        }
        _ => {}
    }
    a1
}

const K_OCT_MOVING: &str = "g2/lll_seg7_collision_octagonal_moving_platform";

pub fn init_oct_mesh(o: &mut SimObject) {
    o.visible = true;
    // ADD_FLOAT(oPosY, -50) without SET_HOME: oHomeY stays 0 (the platforms are placed at y = 0).
    o.pos[1] -= 50.0;
    o.home[1] = 0.0;
}

/// lll_octagonal_rotating_mesh.inc.c `bhv_lll_moving_octagonal_mesh_platform_loop`.
pub fn oct_mesh(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, riding| {
        let o = &mut m.objects[id];
        if o.action == 0 {
            o.set_var("f8", 0.0);
            o.action += 1;
        } else {
            let path = if bparam2(o) == 0 { &D_8032F860 } else { &D_8032F894 };
            let idx = octagonal_mesh_move(o, path, o.var("f8") as usize, riding);
            o.set_var("f8", idx as f64);
        }
        cur_obj_move_using_fvel_and_gravity(o, 0.0);
        if octagonal_find_y_offset(o, "u104", "u108", riding, 0x400 as f64, -80.0) {
            let f4 = o.var("f4") + 0x800 as f64;
            o.set_var("f4", f4);
            let u100 = o.var("u100") - sins(f4) * 2.0;
            o.set_var("u100", u100);
        }
        o.pos[1] = o.var("u100") + o.home[1] + o.var("u108");
        load_object_collision_model(m, id, K_OCT_MOVING, 1000.0);
    });
}

const K_FLOATING_BLOCK: &str = "g2/lll_seg7_collision_floating_block";

pub fn init_sink_rock(o: &mut SimObject) {
    o.visible = true;
    o.pos[1] -= 50.0;
    o.home = o.pos;
}

/// lll_sinking_rock_block.inc.c `bhv_lll_sinking_rock_block_loop`.
pub fn sink_rock(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, riding| {
        let o = &mut m.objects[id];
        octagonal_find_y_offset(o, "u104", "u108", riding, 124.0, -110.0);
        o.graph_y_offset = 0.0;
        o.pos[1] = o.home[1] + o.var("u108");
        load_object_collision_model(m, id, K_FLOATING_BLOCK, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Tilting inverted pyramids (tilting_inverted_pyramid.inc.c)
// ---------------------------------------------------------------------------------------------

const K_INVERTED_PYRAMID: &str = "g2/lll_seg7_collision_inverted_pyramid";

/// math_util.c `mtxf_align_terrain_normal(dest, normal, pos, 0)`.
pub fn align_terrain_normal(up: [f32; 3], pos: [f32; 3]) -> Mat4 {
    fn normalize(v: [f32; 3]) -> [f32; 3] {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l == 0.0 {
            return v;
        }
        [v[0] / l, v[1] / l, v[2] / l]
    }
    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    }
    let lateral = [0.0f32, 0.0, 1.0];
    let up = normalize(up);
    let left = normalize(cross(up, lateral));
    let forward = normalize(cross(left, up));
    [[left[0], left[1], left[2], 0.0], [up[0], up[1], up[2], 0.0], [forward[0], forward[1], forward[2], 0.0], [pos[0], pos[1], pos[2], 1.0]]
}

/// `approach_by_increment` of tilting_inverted_pyramid.inc.c.
fn approach_by_increment(goal: f64, src: f64, inc: f64) -> f64 {
    if src <= goal {
        if goal - src < inc {
            goal
        } else {
            src + inc
        }
    } else if goal - src > -inc {
        goal
    } else {
        src - inc
    }
}

fn store_basis(o: &mut SimObject, mat: &Mat4) {
    for r in 0..3 {
        for c in 0..3 {
            o.set_var(["m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8"][r * 3 + c], mat[r][c] as f64);
        }
    }
}

fn load_basis(o: &SimObject) -> Mat4 {
    let mut m = [[0.0f32; 4]; 4];
    for r in 0..3 {
        for c in 0..3 {
            m[r][c] = o.var(["m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8"][r * 3 + c]) as f32;
        }
    }
    m[3][3] = 1.0;
    m
}

/// `bhv_platform_normals_init` (after `ADD_FLOAT(oPosY, 5)` and `SET_HOME`).
pub fn init_tilt_pyramid(o: &mut SimObject) {
    o.visible = true;
    o.pos[1] += 5.0;
    o.home = o.pos;
    o.set_var("nx", 0.0);
    o.set_var("ny", 1.0);
    o.set_var("nz", 0.0);
    let mat = align_terrain_normal([0.0, 1.0, 0.0], [o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32]);
    store_basis(o, &mat);
}

/// `bhv_tilting_inverted_pyramid_loop`: the platform leans toward Mario and carries him with it.
pub fn tilt_pyramid(id: ObjId, m: &mut MarioRuntime) {
    begin(m, id);
    let riding = mario_on_platform(m, id);
    let o = &m.objects[id];
    let transform = load_basis(o);
    let (mut dx, mut dy, mut dz);
    let mut mario_on = false;
    let mut dist = [0.0f32; 3];
    let mut before = [0.0f32; 3];
    if riding {
        dist = [(m.pos[0] - o.pos[0]) as f32, (m.pos[1] - o.pos[1]) as f32, (m.pos[2] - o.pos[2]) as f32];
        before = linear_mul(&transform, dist);
        dx = m.pos[0] - o.pos[0];
        dy = 500.0;
        dz = m.pos[2] - o.pos[2];
        let d = (dx * dx + dy * dy + dz * dz).sqrt();
        if d != 0.0 {
            let d = 1.0 / d;
            dx *= d;
            dy *= d;
            dz *= d;
        } else {
            dx = 0.0;
            dy = 1.0;
            dz = 0.0;
        }
        if o.var("onPlat") == 1.0 {
            mario_on = true;
        }
    } else {
        dx = 0.0;
        dy = 1.0;
        dz = 0.0;
    }
    let o = &mut m.objects[id];
    o.set_var("onPlat", if riding { 1.0 } else { 0.0 });
    let nx = approach_by_increment(dx, o.var("nx"), 0.01);
    let ny = approach_by_increment(dy, o.var("ny"), 0.01);
    let nz = approach_by_increment(dz, o.var("nz"), 0.01);
    o.set_var("nx", nx);
    o.set_var("ny", ny);
    o.set_var("nz", nz);
    let mat = align_terrain_normal([nx as f32, ny as f32, nz as f32], [o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32]);
    store_basis(o, &mat);
    if mario_on {
        let after = linear_mul(&mat, dist);
        m.state.pos[0] += (after[0] - before[0]) as f64;
        m.state.pos[1] += (after[1] - before[1]) as f64;
        m.state.pos[2] += (after[2] - before[2]) as f64;
    }
    load_collision_with_matrix(m, id, K_INVERTED_PYRAMID, 1000.0, &mat);
    finish(m, id, F_FACE_YAW);
}

// ---------------------------------------------------------------------------------------------
// Rolling log (rolling_log.inc.c)
// ---------------------------------------------------------------------------------------------

const K_LLL_LOG: &str = "g2/lll_seg7_collision_pitoune";
const K_TTM_LOG: &str = "g2/ttm_seg7_collision_pitoune_2";

/// `bhv_lll_rolling_log_init`.
pub fn init_rolling_log_lll(o: &mut SimObject) {
    o.visible = true;
    o.set_var("f8", 5120.0);
    o.set_var("fc", 6016.0);
    o.set_var("f4", 1048576.0);
    o.move_yaw = 0x3FFF as f64;
    o.forward_vel = 0.0;
    o.vel = [0.0; 3];
    o.set_var("pitch", 0.0);
    o.set_var("avPitch", 0.0);
}

/// `bhv_ttm_rolling_log_init`.
pub fn init_rolling_log_ttm(o: &mut SimObject) {
    o.visible = true;
    o.set_var("f8", 3970.0);
    o.set_var("fc", 3654.0);
    o.set_var("f4", 271037.0);
    o.move_yaw = 8810.0;
    o.forward_vel = 0.0;
    o.vel = [0.0; 3];
    o.set_var("pitch", 0.0);
    o.set_var("avPitch", 0.0);
}

/// `rolling_log_roll_log`.
fn rolling_log_roll(o: &mut SimObject, mario_pos: Vec3, riding: bool) {
    let mut av = o.var("avPitch");
    if riding {
        let side = (mario_pos[2] - o.pos[2]) * coss(-o.move_yaw) - (mario_pos[0] - o.pos[0]) * sins(-o.move_yaw);
        if side > 0.0 {
            av += 0x10 as f64;
        } else {
            av -= 0x10 as f64;
        }
        av = av.clamp(-0x200 as f64, 0x200 as f64);
    } else {
        let (dx, dy, dz) = (o.home[0] - o.pos[0], o.home[1] - o.pos[1], o.home[2] - o.pos[2]);
        if dx * dx + dy * dy + dz * dz < 100.0 * 100.0 {
            if av != 0.0 {
                if av > 0.0 {
                    av -= 0x10 as f64;
                } else {
                    av += 0x10 as f64;
                }
                if av < 0x10 as f64 && av > -0x10 as f64 {
                    av = 0.0;
                }
            }
        } else if av != 0x100 as f64 {
            if av > 0x100 as f64 {
                av -= 0x10 as f64;
            } else {
                av += 0x10 as f64;
            }
            if av < 0x110 as f64 && av > 0xF0 as f64 {
                av = 0x100 as f64;
            }
        }
    }
    o.set_var("avPitch", av);
}

fn rolling_log(id: ObjId, m: &mut MarioRuntime, key: &str, dist: f64) {
    run_platform(m, id, 0, |m, id, riding| {
        let mario = m.pos;
        let o = &mut m.objects[id];
        let (prev_x, prev_z) = (o.pos[0], o.pos[2]);
        rolling_log_roll(o, mario, riding);
        let av = o.var("avPitch");
        o.forward_vel = (av / 0x40 as f64).trunc();
        o.vel[0] = o.forward_vel * sins(o.move_yaw);
        o.vel[2] = o.forward_vel * coss(o.move_yaw);
        o.pos[0] += o.vel[0];
        o.pos[2] += o.vel[2];
        let (cx, cz, limit) = (o.var("f8"), o.var("fc"), o.var("f4"));
        if limit < (o.pos[0] - cx).powi(2) + (o.pos[2] - cz).powi(2) {
            o.forward_vel = 0.0;
            o.pos[0] = prev_x;
            o.pos[2] = prev_z;
            o.vel[0] = 0.0;
            o.vel[2] = 0.0;
        }
        let pitch = o.var("pitch") + av;
        o.set_var("pitch", pitch);
        load_object_collision_model(m, id, key, dist);
    });
}

/// `bhv_rolling_log_loop` for bhvLLLRollingLog (`oCollisionDistance = 2000`).
pub fn rolling_log_lll(id: ObjId, m: &mut MarioRuntime) {
    rolling_log(id, m, K_LLL_LOG, 2000.0);
}

/// `bhv_rolling_log_loop` for bhvTTMRollingLog.
pub fn rolling_log_ttm(id: ObjId, m: &mut MarioRuntime) {
    rolling_log(id, m, K_TTM_LOG, 2000.0);
}

// ---------------------------------------------------------------------------------------------
// Volcano falling trap (rolling_log.inc.c `bhv_volcano_trap_loop`)
// ---------------------------------------------------------------------------------------------

const K_FALLING_WALL: &str = "g2/lll_seg7_collision_falling_wall";

pub fn volcano_trap(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        let mario_close = {
            let o = &m.objects[id];
            point_within_radius_of_mario(m, o.pos, 1000.0)
        };
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                if mario_close {
                    o.action = 1;
                }
            }
            1 => {
                // volcano_act_1
                let f4 = o.var("f4") + 4.0;
                o.set_var("f4", f4);
                let av = o.var("avPitch") + f4;
                o.set_var("avPitch", av);
                let pitch = o.var("pitch") - av;
                o.set_var("pitch", pitch);
                if pitch < -0x4000 as f64 {
                    o.set_var("pitch", -0x4000 as f64);
                    o.set_var("avPitch", 0.0);
                    o.set_var("f4", 0.0);
                    o.action = 2;
                }
            }
            2 => {
                if o.timer < 8.0 {
                    o.pos[1] = o.home[1] + sins(o.timer * 0x1000 as f64) * 10.0;
                }
                if o.timer == 50.0 {
                    o.action = 3;
                }
            }
            3 => {
                // volcano_act_3
                o.set_var("avPitch", 0x90 as f64);
                let pitch = o.var("pitch") + 0x90 as f64;
                o.set_var("pitch", if pitch > 0.0 { 0.0 } else { pitch });
                if o.timer == 200.0 {
                    o.action = 0;
                }
            }
            _ => {}
        }
        load_object_collision_model(m, id, K_FALLING_WALL, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Floating wood bridge (lll_floating_wood_piece.inc.c)
// ---------------------------------------------------------------------------------------------

const K_WOOD_PIECE: &str = "g2/lll_seg7_collision_wood_piece";
pub const WOOD_BRIDGE_MODEL: &str = "levels/lll/lll_geo_000B50.glb";

pub fn init_wood_bridge(o: &mut SimObject) {
    o.visible = false;
}

/// `spawn_object_relative`: the child's offset is rotated by the parent's transform.
pub fn spawn_relative(m: &mut MarioRuntime, parent: ObjId, kind: ObjectBehaviorKind, model: Option<&str>, param: f64, rel: [f64; 3]) -> ObjId {
    let mat = object_matrix(&m.objects[parent]);
    let id = spawn_child(m, parent, kind, model, param);
    let o = &mut m.objects[id];
    for axis in 0..3 {
        o.pos[axis] += (mat[0][axis] * rel[0] as f32 + mat[1][axis] * rel[1] as f32 + mat[2][axis] * rel[2] as f32) as f64;
    }
    o.param = ((param as u32 & 0xFF) << 16) as f64;
    id
}

/// `bhv_lll_floating_wood_bridge_loop`.
pub fn wood_bridge(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let (action, dist) = (m.objects[id].action, m.objects[id].distance_to_mario);
        match action {
            0 => {
                if dist < 2500.0 {
                    for i in 1..4 {
                        let p = spawn_relative(m, id, ObjectBehaviorKind::LllWoodPiece, Some(WOOD_BRIDGE_MODEL), 0.0, [(i as f64 - 2.0) * 300.0, 0.0, 0.0]);
                        m.objects[p].set_var("osc", i as f64 * 0x1000 as f64);
                    }
                    m.objects[id].action = 1;
                }
            }
            1 => {
                if dist > 2600.0 {
                    m.objects[id].action = 2;
                }
            }
            2 => m.objects[id].action = 0,
            _ => {}
        }
    });
}

pub fn init_wood_piece(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_lll_wood_piece_loop`.
pub fn wood_piece(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let parent_action = m.objects[id].link.map(|p| m.objects[p].action);
        let o = &mut m.objects[id];
        if o.timer == 0.0 {
            o.pos[1] -= 100.0;
        }
        o.pos[1] += sins(o.var("osc")) * 3.0;
        let osc = o.var("osc") + 0x400 as f64;
        o.set_var("osc", osc);
        if parent_action == Some(2) {
            mark_for_deletion(o);
            unload_object_collision(m, id);
            return;
        }
        load_object_collision_model(m, id, K_WOOD_PIECE, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Drawbridge (drawbridge.inc.c)
// ---------------------------------------------------------------------------------------------

const K_DRAWBRIDGE: &str = "g2/lll_seg7_collision_drawbridge";

pub fn init_drawbridge_spawner(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_lll_drawbridge_spawner_loop`: two bridge halves, then the spawner deactivates.
pub fn drawbridge_spawner(id: ObjId, m: &mut MarioRuntime) {
    let model = m.objects[id].model.clone();
    let yaw = m.objects[id].move_yaw;
    for (sign, extra) in [(1.0, 0.0), (-1.0, 0x8000 as f64)] {
        let b = spawn_child(m, id, ObjectBehaviorKind::LllDrawbridge, model.as_deref(), 0.0);
        let o = &mut m.objects[b];
        o.move_yaw = yaw + extra;
        o.face_yaw = o.move_yaw;
        o.pos[0] += coss(yaw) * 640.0 * sign;
        o.pos[2] += sins(yaw) * 640.0 * sign;
    }
    mark_for_deletion(&mut m.objects[id]);
}

pub fn init_drawbridge(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_lll_drawbridge_loop`. gGlobalTimer is approximated by the bridge's own frame counter.
pub fn drawbridge(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let o = &mut m.objects[id];
        let gt = o.var("gt") + 1.0;
        o.set_var("gt", gt);
        let mut roll = o.var("roll");
        match o.action {
            0 => roll += 0x100 as f64,
            1 => roll -= 0x100 as f64,
            _ => {}
        }
        if s16(roll) < -0x1FFD as f64 {
            roll = 0xDFFF as f64;
            if o.timer > 50.0 && gt % 8.0 == 0.0 {
                o.action = 0;
            }
        }
        if s16(roll) >= 0.0 {
            roll = 0.0;
            if o.timer > 50.0 && gt % 8.0 == 0.0 {
                o.action = 1;
            }
        }
        o.set_var("roll", roll);
        load_object_collision_model(m, id, K_DRAWBRIDGE, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Rotating fire bars (lll_rotating_hex_flame.inc.c)
// ---------------------------------------------------------------------------------------------

const K_FIRE_BARS: &str = "g2/lll_seg7_collision_rotating_fire_bars";

pub fn init_fire_bars(o: &mut SimObject) {
    o.visible = true;
}

/// `fire_bar_spawn_flames`.
fn fire_bar_spawn_flames(m: &mut MarioRuntime, id: ObjId, a0: f64) {
    let mut sp1c = sins(a0) * 200.0;
    let mut sp18 = coss(a0) * 200.0;
    let count = if bparam2(&m.objects[id]) == 0 { 4 } else { 3 };
    let y = m.objects[id].pos[1];
    for _ in 0..count {
        let f = spawn_child(m, id, ObjectBehaviorKind::LllHexFlame, Some(RED_FLAME), 0.0);
        let c = &mut m.objects[f];
        c.set_var("f4", sp1c);
        c.set_var("f8", y - 200.0);
        c.set_var("fc", sp18);
        c.scale = 6.0;
        sp1c += sins(a0) * 150.0;
        sp18 += coss(a0) * 150.0;
    }
}

/// `bhv_lll_rotating_block_fire_bars_loop`.
pub fn fire_bars(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let (action, dist) = (m.objects[id].action, m.objects[id].distance_to_mario);
        match action {
            0 => {
                if dist < 3000.0 {
                    m.objects[id].action = 1;
                }
            }
            1 => {
                fire_bar_spawn_flames(m, id, 0.0);
                fire_bar_spawn_flames(m, id, -0x8000 as f64);
                let o = &mut m.objects[id];
                o.set_var("avYaw", 0.0);
                o.move_yaw = 0.0;
                o.action = 2;
            }
            2 => {
                let o = &mut m.objects[id];
                o.set_var("avYaw", -0x100 as f64);
                o.move_yaw = s16(o.move_yaw - 0x100 as f64);
                if dist > 3200.0 {
                    o.action = 3;
                }
            }
            3 => m.objects[id].action = 0,
            _ => {}
        }
        if bparam2(&m.objects[id]) == 0 {
            load_object_collision_model(m, id, K_FIRE_BARS, 4000.0);
        }
    });
}

pub fn init_hex_flame(o: &mut SimObject) {
    init_flame_object(o, 50.0, 100.0, 50.0);
}

/// `bhv_lll_rotating_hex_flame_loop`.
pub fn hex_flame(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let Some(parent) = m.objects[id].link else { return };
        let (ppos, pyaw, paction) = {
            let p = &m.objects[parent];
            (p.pos, p.move_yaw, p.action)
        };
        let o = &mut m.objects[id];
        let (f4, f8, fc) = (o.var("f4"), o.var("f8"), o.var("fc"));
        set_pos_relative(o, ppos, pyaw, f4, f8, fc);
        o.pos[1] = ppos[1] + 100.0;
        if paction == 3 {
            mark_for_deletion(o);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Flamethrower (flamethrower.inc.c)
// ---------------------------------------------------------------------------------------------

pub fn init_flamethrower(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_flamethrower_loop`.
pub fn flamethrower(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let (action, timer, dist, kind) = {
            let o = &m.objects[id];
            (o.action, o.timer, o.distance_to_mario, bparam2(o))
        };
        if action == 0 {
            if dist < 2000.0 {
                m.objects[id].action += 1;
            }
        } else if action == 1 {
            let model = if kind == 1 { BLUE_FLAME } else { RED_FLAME };
            let flame_vel = if kind == 2 { 50.0 } else { 95.0 };
            let mut sp34 = 1.0;
            if timer < 60.0 {
                sp34 = 15.0;
            } else if timer < 74.0 {
                sp34 = 75.0 - timer;
            } else {
                m.objects[id].action += 1;
            }
            m.objects[id].set_var("f110", sp34);
            let f = spawn_relative(m, id, ObjectBehaviorKind::FlamethrowerFlame, Some(model), kind as f64, [0.0; 3]);
            m.objects[f].forward_vel = flame_vel;
        } else if timer > 60.0 {
            m.objects[id].action = 0;
        }
    });
}

pub fn init_flamethrower_flame(o: &mut SimObject) {
    init_flame_object(o, 50.0, 25.0, 25.0);
}

/// `bhv_flamethrower_flame_loop`.
pub fn flamethrower_flame(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let parent_f110 = m.objects[id].link.map(|p| m.objects[p].var("f110")).unwrap_or(0.0);
        let o = &mut m.objects[id];
        let kind = bparam2(o);
        if o.timer == 0.0 {
            o.action_state = 0.0;
            for axis in 0..3 {
                o.pos[axis] += random_float() * 10.0 - 10.0 * 0.5;
            }
            o.home = o.pos;
        }
        let mut scale = if kind == 2 { o.timer * (o.forward_vel - 6.0) / 100.0 + 2.0 } else { o.timer * (o.forward_vel - 20.0) / 100.0 + 1.0 };
        let life;
        if kind == 3 {
            o.hitbox.height = 200.0;
            o.hitbox.down_offset = 150.0;
            o.vel[1] = -28.0;
            phys::update_floor_height(o, &world);
            let floor = phys::floor_height(o);
            if o.pos[1] - 25.0 * scale < floor {
                o.vel[1] = 0.0;
                o.pos[1] = floor + 25.0 * scale;
            }
            life = (parent_f110 / 1.2).trunc();
        } else {
            life = parent_f110;
        }
        if scale < 0.0 {
            scale = 0.0;
        }
        o.scale = scale;
        if kind == 4 {
            o.pos[1] += o.forward_vel;
        } else {
            cur_obj_move_using_fvel_and_gravity(o, 0.0);
        }
        if o.timer > life {
            mark_for_deletion(o);
        }
        o.interact_status = 0;
    });
}

// ---------------------------------------------------------------------------------------------
// Bouncing fireball (bouncing_fireball.inc.c)
// ---------------------------------------------------------------------------------------------

pub fn init_bouncing_fireball(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_bouncing_fireball_loop`.
pub fn bouncing_fireball(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let (action, timer, dist) = {
            let o = &m.objects[id];
            (o.action, o.timer, o.distance_to_mario)
        };
        match action {
            0 => {
                if dist < 2000.0 {
                    m.objects[id].action = 1;
                }
            }
            1 => {
                let f = spawn_child(m, id, ObjectBehaviorKind::BouncingFireFlame, Some(RED_FLAME), 0.0);
                m.objects[f].scale = (10.0 - timer) * 0.5;
                if timer > 10.0 {
                    m.objects[id].action += 1;
                }
            }
            2 => {
                if timer == 0.0 {
                    m.objects[id].set_var("f4", random_float() * 100.0);
                }
                if m.objects[id].var("f4") + 100.0 < timer {
                    m.objects[id].action = 0;
                }
            }
            _ => {}
        }
    });
}

pub fn init_bouncing_fire_flame(o: &mut SimObject) {
    init_flame_object(o, 50.0, 25.0, 25.0);
    o.graph_y_offset = 30.0;
    phys::set_physics(o, 30.0, -400.0, -70.0, 1000.0, 1000.0, 200.0);
    o.set_var("noWater", 1.0);
}

/// `bhv_bouncing_fireball_flame_loop`.
pub fn bouncing_fire_flame(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let o = &mut m.objects[id];
        phys::update_floor_and_walls(o, &world);
        match o.action {
            0 => {
                if o.timer == 0.0 {
                    o.vel[1] = 30.0;
                }
                if phys::flags(o) & phys::MOVE_LANDED != 0 {
                    o.action += 1;
                }
            }
            1 => {
                if o.timer == 0.0 {
                    o.vel[1] = 50.0;
                    o.forward_vel = 30.0;
                }
                if phys::flags(o) & (phys::MASK_ON_GROUND | phys::MOVE_AT_WATER_SURFACE | phys::MOVE_UNDERWATER_ON_GROUND) != 0 && o.timer > 100.0 {
                    mark_for_deletion(o);
                }
            }
            _ => {}
        }
        if o.timer > 300.0 {
            mark_for_deletion(o);
        }
        phys::move_standard(o, &world, 78.0);
        o.interact_status = 0;
    });
}

// ---------------------------------------------------------------------------------------------
// Bowser puzzle (bowser_puzzle_piece.inc.c)
// ---------------------------------------------------------------------------------------------

const K_PUZZLE_PIECE: &str = "g2/lll_seg7_collision_puzzle_piece";
const PIECE_MODELS: [&str; 14] = [
    "levels/lll/lll_geo_000C10.glb",
    "levels/lll/lll_geo_000C30.glb",
    "levels/lll/lll_geo_000C50.glb",
    "levels/lll/lll_geo_000C70.glb",
    "levels/lll/lll_geo_000C90.glb",
    "levels/lll/lll_geo_000CB0.glb",
    "levels/lll/lll_geo_000CD0.glb",
    "levels/lll/lll_geo_000CF0.glb",
    "levels/lll/lll_geo_000D10.glb",
    "levels/lll/lll_geo_000D30.glb",
    "levels/lll/lll_geo_000D50.glb",
    "levels/lll/lll_geo_000D70.glb",
    "levels/lll/lll_geo_000D90.glb",
    "levels/lll/lll_geo_000DB0.glb",
];

/// (xOffset, zOffset, initialAction) of `sBowserPuzzlePieces`.
const PIECES: [(i32, i32, i32); 14] = [
    (-5, -15, 1),
    (5, -15, 0),
    (-15, -5, 0),
    (-5, -5, 0),
    (5, -5, 0),
    (15, -5, 0),
    (-15, 5, 0),
    (-5, 5, 0),
    (5, 5, 0),
    (15, 5, 0),
    (-15, 15, 0),
    (-5, 15, 0),
    (5, 15, 0),
    (15, 15, 0),
];

/// `sPieceActions01` ... `sPieceActions14` (0-based by piece), `-1` terminates.
const PIECE_ACTIONS: [[i8; 27]; 14] = [
    [2, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 4, 2, -1],
    [2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 4, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 6, 2, 2, 5, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 4, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 5, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 6, 2, 2, 2, -1],
    [2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 6, 2, 2, 2, 2, 2, 2, 5, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 5, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 6, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 5, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 6, 2, 2, 2, 2, 2, 2, 2, -1],
    [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1],
];

pub fn init_bowser_puzzle(o: &mut SimObject) {
    o.visible = false;
    o.pos[2] -= 50.0;
}

/// `bhv_lll_bowser_puzzle_loop`.
pub fn bowser_puzzle(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| match m.objects[id].action {
        0 => {
            // bhv_lll_bowser_puzzle_spawn_pieces(480)
            for (i, &(xo, zo, initial)) in PIECES.iter().enumerate() {
                let p = spawn_child(m, id, ObjectBehaviorKind::BowserPuzzlePiece, Some(PIECE_MODELS[i]), 0.0);
                let o = &mut m.objects[p];
                o.pos[0] += xo as f64 * 480.0 / 10.0;
                o.pos[1] += 50.0;
                o.pos[2] += zo as f64 * 480.0 / 10.0;
                o.action = initial;
                o.set_var("pi", i as f64);
                o.set_var("ni", 0.0);
            }
            m.objects[id].action += 1;
        }
        1 => {
            let flags = m.objects[id].var("flags") as i32;
            if flags == 3 && m.objects[id].distance_to_mario < 1000.0 {
                // The decomp spawns 5 `bhvSingleCoinGetsSpawned` coins here.
                let pos = m.objects[id].pos;
                for _ in 0..5 {
                    m.objects.spawn_coin(pos, true, 1.0);
                }
                m.objects[id].set_var("flags", 0.0);
                m.objects[id].action += 1;
            }
        }
        _ => {}
    });
}

pub fn init_puzzle_piece(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_lll_bowser_puzzle_piece_move`.
fn piece_move(o: &mut SimObject, x: f64, z: f64, duration: f64) {
    if o.timer < 20.0 {
        o.set_var("offY", if (o.timer as i64) % 2 != 0 { 0.0 } else { -6.0 });
    } else if o.timer < duration + 20.0 {
        o.set_var("offX", o.var("offX") + x);
        o.set_var("offZ", o.var("offZ") + z);
    } else {
        o.action = 2;
        o.set_var("cont", 0.0);
    }
}

/// `bhv_lll_bowser_puzzle_piece_loop`.
pub fn puzzle_piece(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, riding| {
        let first = !m.objects[id].has_var("homed");
        if first {
            let o = &mut m.objects[id];
            o.home = o.pos; // SET_HOME on the first script frame, after the spawner placed the piece
            o.set_var("homed", 1.0);
        }
        // bhv_lll_bowser_puzzle_piece_update
        let parent = m.objects[id].link;
        if riding {
            if let Some(p) = parent {
                m.objects[p].set_var("flags", 1.0);
            }
        }
        if m.objects[id].var("cont") == 0.0 {
            let (list, ni) = {
                let o = &m.objects[id];
                (&PIECE_ACTIONS[o.var("pi") as usize], o.var("ni") as usize)
            };
            let action = list[ni] as i32;
            let mut ni = ni + 1;
            let wrapped = list[ni] == -1;
            if wrapped {
                ni = 0;
            }
            change_action(&mut m.objects[id], action);
            m.objects[id].set_var("ni", ni as f64);
            if wrapped {
                if let Some(p) = parent {
                    let f = m.objects[p].var("flags") as i32 | 2;
                    m.objects[p].set_var("flags", f as f64);
                }
            }
            m.objects[id].set_var("cont", 1.0);
        }
        let o = &mut m.objects[id];
        match o.action {
            1 => {
                o.pos[1] += 50.0;
                o.action = 3;
            }
            2 => {
                if o.timer >= 24.0 {
                    o.set_var("cont", 0.0);
                }
            }
            3 => piece_move(o, -120.0, 0.0, 4.0),
            4 => piece_move(o, 120.0, 0.0, 4.0),
            5 => piece_move(o, 0.0, -120.0, 4.0),
            6 => piece_move(o, 0.0, 120.0, 4.0),
            _ => {}
        }
        let o = &mut m.objects[id];
        o.pos = [o.var("offX") + o.home[0], o.var("offY") + o.home[1], o.var("offZ") + o.home[2]];
        load_object_collision_model(m, id, K_PUZZLE_PIECE, 3000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Pole (pole.inc.c, pole_base.inc.c)
// ---------------------------------------------------------------------------------------------

/// `bhvPoleGrabbing` + `bhv_pole_init`: `hitboxHeight = oBhvParams2ndByte * 10`.
pub fn init_pole(o: &mut SimObject) {
    let tenth = ((o.param as u32) >> 16 & 0xFF) as f64;
    o.hitbox = ObjectHitbox { interact_type: INTERACT_POLE, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0, radius: 80.0, height: tenth * 10.0, hurtbox_radius: 80.0, hurtbox_height: tenth * 10.0 };
    o.visible = true;
}

/// `bhv_pole_base_loop`.
pub fn pole(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let (y, h, timer) = {
            let o = &m.objects[id];
            (o.pos[1], o.hitbox.height, o.timer)
        };
        if y - 10.0 < m.pos[1] && m.pos[1] < y + h + 30.0 && timer > 10.0 && (m.action & MARIO_PUNCHING) == 0 {
            push_mario_away(m, id, 70.0);
        }
    });
}
