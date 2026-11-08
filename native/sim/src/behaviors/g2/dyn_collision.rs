//! Object collision that follows its object, and Mario's platform carry.
//!
//! * `surface_load.c`: `load_object_collision_model`, `transform_object_vertices`: the vertices
//!   of an object's collision model are rotated (`mtxf_rotate_zxy_and_translate`), scaled and
//!   translated, then truncated to integers (`TerrainData` is `s16`) every frame, and only while
//!   Mario is closer than `oCollisionDistance` (default 1000).
//! * `platform_displacement.c`: `apply_platform_displacement(TRUE, platform)` moves Mario along
//!   with the platform's velocity and rotation of the last frame.
//!
//! The collision models themselves are exported per course (`extraCollision` entries whose key
//! starts with `g2/`, see `scripts/export_sm64_g2.py`) and handed to [`register_meshes_from_json`]
//! by the course loader. Meshes are keyed by the decomp symbol, so one process-wide table is
//! enough (the same symbol always has the same data).
use super::obj::*;
use crate::collision::SurfaceOptions;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::surfaces::SURFACE_FLAG_NO_CAM_COLLISION;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// One collision model: `COL_VERTEX` list and `[a, b, c, surface type, force]` triangles.
#[derive(Clone, Debug, Default)]
pub struct CollisionMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[i32; 5]>,
}

fn store() -> &'static RwLock<HashMap<String, Arc<CollisionMesh>>> {
    static S: OnceLock<RwLock<HashMap<String, Arc<CollisionMesh>>>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_mesh(key: &str, mesh: CollisionMesh) {
    store().write().unwrap().insert(key.to_string(), Arc::new(mesh));
}

pub fn mesh(key: &str) -> Option<Arc<CollisionMesh>> {
    store().read().unwrap().get(key).cloned()
}

/// Register every `extraCollision` entry of a course json (`{key: {vertices, triangles}}`).
/// Returns how many were registered.
pub fn register_meshes_from_json(extra: &serde_json::Value) -> usize {
    let Some(map) = extra.as_object() else { return 0 };
    let mut n = 0;
    for (key, v) in map {
        if !key.starts_with("g2/") {
            continue;
        }
        let vertices = v["vertices"].as_array().map(|a| a.iter().map(|p| [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0), p[2].as_f64().unwrap_or(0.0)]).collect()).unwrap_or_default();
        let triangles = v["triangles"]
            .as_array()
            .map(|a| a.iter().map(|t| [0, 1, 2, 3, 4].map(|i| t[i].as_i64().unwrap_or(0) as i32)).collect())
            .unwrap_or_default();
        register_mesh(key, CollisionMesh { vertices, triangles });
        n += 1;
    }
    n
}

/// Row-major like the decomp's `Mat4`: rows 0..2 are the basis vectors, row 3 the translation.
pub type Mat4 = [[f32; 4]; 4];

/// math_util.c `mtxf_rotate_zxy_and_translate`.
pub fn rotate_zxy_and_translate(translate: [f32; 3], rotate: [i32; 3]) -> Mat4 {
    let (sx, cx) = (sins(rotate[0] as f64) as f32, coss(rotate[0] as f64) as f32);
    let (sy, cy) = (sins(rotate[1] as f64) as f32, coss(rotate[1] as f64) as f32);
    let (sz, cz) = (sins(rotate[2] as f64) as f32, coss(rotate[2] as f64) as f32);
    let mut d = [[0.0f32; 4]; 4];
    d[0][0] = cy * cz + sx * sy * sz;
    d[1][0] = -cy * sz + sx * sy * cz;
    d[2][0] = cx * sy;
    d[3][0] = translate[0];
    d[0][1] = cx * sz;
    d[1][1] = cx * cz;
    d[2][1] = -sx;
    d[3][1] = translate[1];
    d[0][2] = -sy * cz + sx * cy * sz;
    d[1][2] = sy * sz + sx * cy * cz;
    d[2][2] = cx * cy;
    d[3][2] = translate[2];
    d[3][3] = 1.0;
    d
}

/// object_helpers.c `linear_mtxf_mul_vec3f`.
pub fn linear_mul(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2])
}

/// object_helpers.c `linear_mtxf_transpose_mul_vec3f`.
pub fn linear_transpose_mul(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// `obj_build_transform_from_pos_and_angle(o, O_POS_INDEX, O_FACE_ANGLE_INDEX)`.
pub fn object_matrix(o: &SimObject) -> Mat4 {
    let rot = [to_s16(o.var("pitch")), to_s16(o.face_yaw), to_s16(o.var("roll"))];
    rotate_zxy_and_translate([o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32], rot)
}

fn remove_owned(m: &mut MarioRuntime, id: ObjId) {
    let owner = Some(id.0);
    let world = Arc::make_mut(&mut m.world);
    world.remove_surfaces(&|s| s.object == owner);
}

/// `load_object_collision_model` for object `id` with the collision model `key`, loaded only
/// while Mario is within `collision_distance`. Rebuilds the surfaces only when the transform
/// changed since the last call.
pub fn load_object_collision_model(m: &mut MarioRuntime, id: ObjId, key: &str, collision_distance: f64) {
    let mat = object_matrix(&m.objects[id]);
    load_collision_with_matrix(m, id, key, collision_distance, &mat);
}

/// [`load_object_collision_model`] with an explicit `o->transform` (objects that build their own).
pub fn load_collision_with_matrix(m: &mut MarioRuntime, id: ObjId, key: &str, collision_distance: f64, mat: &Mat4) {
    let in_range = m.objects[id].distance_to_mario < collision_distance;
    // The surfaces live in this world only: an area swap or a rebuilt world needs a fresh load.
    let world_ptr = Arc::as_ptr(&m.world) as usize as f64;
    let any_placed = m.objects[id].has_var("dcPlaced");
    let placed = any_placed && m.objects[id].var("dcWorld") == world_ptr;
    if !in_range {
        if any_placed {
            remove_owned(m, id);
            m.objects[id].vars.remove("dcPlaced");
        }
        return;
    }
    let Some(mesh) = mesh(key) else { return };
    let scale = m.objects[id].scale as f32;
    // obj_apply_scale_to_matrix: the basis rows are scaled by the object's scale.
    let mut s = *mat;
    for row in s.iter_mut().take(3) {
        for v in row.iter_mut().take(3) {
            *v *= scale;
        }
    }
    let mut cache = [0.0f64; 12];
    for r in 0..4 {
        for c in 0..3 {
            cache[r * 3 + c] = s[r][c] as f64;
        }
    }
    let unchanged = placed && (0..12).all(|i| m.objects[id].var(CACHE_KEYS[i]) == cache[i]);
    if unchanged {
        return;
    }
    // Triangles keep the order of the model; vertices are truncated like `(TerrainData)(...)`.
    let verts: Vec<[f64; 3]> = mesh
        .vertices
        .iter()
        .map(|v| {
            let (vx, vy, vz) = (v[0] as f32, v[1] as f32, v[2] as f32);
            [0, 1, 2].map(|i| (vx * s[0][i] + vy * s[1][i] + vz * s[2][i] + s[3][i]).trunc() as f64)
        })
        .collect();
    remove_owned(m, id);
    let world = Arc::make_mut(&mut m.world);
    for &[a, b, c, kind, force] in &mesh.triangles {
        let (Some(&va), Some(&vb), Some(&vc)) = (verts.get(a as usize), verts.get(b as usize), verts.get(c as usize)) else { continue };
        let flags = if (0x76..=0x79).contains(&kind) { SURFACE_FLAG_NO_CAM_COLLISION } else { 0 };
        world.add_triangle(va, vb, vc, &SurfaceOptions { room: None, kind, force, flags, object: Some(id.0) });
    }
    let world_ptr = Arc::as_ptr(&m.world) as usize as f64;
    let o = &mut m.objects[id];
    for i in 0..12 {
        o.set_var(CACHE_KEYS[i], cache[i]);
    }
    o.set_var("dcPlaced", 1.0);
    o.set_var("dcWorld", world_ptr);
}

const CACHE_KEYS: [&str; 12] = ["dc0", "dc1", "dc2", "dc3", "dc4", "dc5", "dc6", "dc7", "dc8", "dc9", "dc10", "dc11"];

/// Drop an object's collision (deactivated / replaced objects).
pub fn unload_object_collision(m: &mut MarioRuntime, id: ObjId) {
    if m.objects[id].has_var("dcPlaced") {
        remove_owned(m, id);
        m.objects[id].vars.remove("dcPlaced");
    }
}

fn s16_add(a: f64, b: f64) -> f64 {
    to_s16(a + b) as f64
}

/// platform_displacement.c `apply_platform_displacement(TRUE, platform)`: uses the platform's
/// `oVel{X,Z}`, `oAngleVel*` and `oFaceAngle*` of the frame that just ran.
pub fn apply_platform_displacement(m: &mut MarioRuntime, id: ObjId) {
    let o = &m.objects[id];
    let av = [to_s16(o.var("avPitch")), to_s16(o.var("avYaw")), to_s16(o.var("avRoll"))];
    let face = [to_s16(o.var("pitch")), to_s16(o.face_yaw), to_s16(o.var("roll"))];
    let ppos = [o.pos[0] as f32, o.pos[1] as f32, o.pos[2] as f32];
    let pvel = o.vel;
    let (mut x, mut y, mut z) = (m.pos[0] as f32, m.pos[1] as f32, m.pos[2] as f32);
    x += pvel[0] as f32;
    z += pvel[2] as f32;
    if av != [0, 0, 0] {
        m.state.face_angle[1] = s16_add(m.state.face_angle[1], av[1] as f64);
        let offset = [x - ppos[0], y - ppos[1], z - ppos[2]];
        let old = rotate_zxy_and_translate(offset, [face[0] - av[0], face[1] - av[1], face[2] - av[2]].map(|a| to_s16(a as f64)));
        let relative = linear_transpose_mul(&old, offset);
        let new = rotate_zxy_and_translate(offset, face);
        let moved = linear_mul(&new, relative);
        x = ppos[0] + moved[0];
        y = ppos[1] + moved[1];
        z = ppos[2] + moved[2];
    }
    m.state.pos = [x as f64, y as f64, z as f64];
}

/// The common platform prologue/epilogue: was Mario on the platform at the end of last frame,
/// run `body` (the behavior script, which loads the collision), `cur_obj_update`'s tail, then
/// carry Mario like `apply_mario_platform_displacement`.
pub fn run_platform(m: &mut MarioRuntime, id: ObjId, flags: u32, body: impl FnOnce(&mut MarioRuntime, ObjId, bool)) {
    let riding = mario_on_platform(m, id);
    begin(m, id);
    body(m, id, riding);
    finish(m, id, flags);
    if riding {
        apply_platform_displacement(m, id);
    }
}
