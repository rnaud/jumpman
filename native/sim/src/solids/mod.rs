//! SM64 "surface objects" (`OBJ_LIST_SURFACE`): moving collision models driven by the decomp
//! behaviors of Tick Tock Clock, Rainbow Ride, Bowser in the Dark World / Fire Sea / Sky and the
//! Bowser arenas.
//!
//! This is the engine-free framework; the behaviors live next to it (`ttc`, `platforms`,
//! `bowser_arena`). It mirrors the decomp's object model closely so each port can be read next
//! to its `src/game/behaviors/*.inc.c`:
//!
//! * [`Obj`] carries the `o*` fields the behaviors touch (`oPos*`, `oHome*`, `oVel*`,
//!   `oFaceAngle*`, `oAngleVel*`, `oMoveAngleYaw`, `oForwardVel`, `oTimer`, `oAction`, ...).
//! * `load_object_collision_model` (`surface_load.c: transform_object_vertices`) is
//!   [`Solids::refresh`]: vertices go through `mtxf_rotate_zxy_and_translate` and are truncated to
//!   s16 exactly like the decomp, then re-added to the [`CollisionWorld`] tagged with the object.
//! * Mario riding a platform is `platform_displacement.c: apply_platform_displacement`
//!   ([`apply_platform_displacement`]).
//!
//! Float semantics: the sim uses `f64` where the game uses `f32`; integer/angle paths are exact.
use crate::collision::{CollisionWorld, SurfaceOptions};
use crate::math::*;
use crate::mario_runtime::MarioRuntime;
use crate::object::ObjectBehaviorKind;
use crate::state::ACT_FLAG_AIR;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub mod bowser_arena;
pub mod platforms;
pub mod ttc;

/// `Surface::object` tag base of the surface objects (the older `MovingPlatform`s use 0x4000_0000).
pub const SOLID_TAG: u32 = 0x4100_0000;

/// `gTTCSpeedSetting` values (`object_constants.h`).
pub const TTC_SPEED_SLOW: i32 = 0;
pub const TTC_SPEED_FAST: i32 = 1;
pub const TTC_SPEED_RANDOM: i32 = 2;
pub const TTC_SPEED_STOPPED: i32 = 3;

// ---------------------------------------------------------------------------
// Course data
// ---------------------------------------------------------------------------

/// One placed surface object, as written by `scripts/export-sm64-solids.py`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolidSpec {
    /// Decomp behavior symbol (`bhvTTCRotatingSolid`).
    pub behavior: String,
    /// Display model (GLB, relative to the asset root); empty when the object is invisible.
    #[serde(default)]
    pub model: String,
    /// Key into the course's `extraCollision`; `None` for objects without collision.
    #[serde(default)]
    pub collision: Option<String>,
    pub pos: [f64; 3],
    /// s16 (pitch, yaw, roll).
    #[serde(default)]
    pub angle: [f64; 3],
    /// Full `oBhvParams` word (`BPARAM1..4` packed).
    #[serde(default)]
    pub bparams: u32,
    #[serde(default)]
    pub acts: Option<Vec<i32>>,
    /// Waypoints for `bhvPlatformOnTrack` ([index, x, y, z] rows).
    #[serde(default)]
    pub track: Vec<[f64; 4]>,
}

/// Collision geometry in object space (`COL_VERTEX` / `COL_TRI` rows: a, b, c, surface type, force).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolidMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[i32; 5]>,
}

// ---------------------------------------------------------------------------
// Random helpers (`behavior_script.c`, `obj_behaviors_2.c`)
// ---------------------------------------------------------------------------

/// `random_u16`.
pub fn random_u16() -> i64 { (random() * 65536.0).floor() as i64 }
/// `random_float`: [0, 1).
pub fn random_float() -> f64 { random() }
/// `random_sign`.
pub fn random_sign() -> f64 { if random_u16() >= 0x7FFF { 1.0 } else { -1.0 } }
/// `random_mod_offset` (obj_behaviors_2.c).
pub fn random_mod_offset(base: i64, step: i64, modulo: i64) -> i64 { base + step * (random_u16() % modulo) }
/// `random_linear_offset` (obj_behaviors_2.c).
pub fn random_linear_offset(base: i64, range: i64) -> i64 { base + (range as f64 * random_float()).trunc() as i64 }

/// `approach_s16_symmetric` (object_helpers.c); the result wraps to s16.
pub fn approach_s16_symmetric(value: f64, target: f64, increment: f64) -> f64 {
    let value = to_s16(value) as f64;
    let target = to_s16(target) as f64;
    let dist = to_s16(target - value) as f64;
    let out = if dist >= 0.0 {
        if dist > increment { value + increment } else { target }
    } else if dist < -increment {
        value - increment
    } else {
        target
    };
    to_s16(out) as f64
}

/// `approach_f32_ptr` (obj_behaviors_2.c): returns true when the target is reached.
pub fn approach_f32_ptr(px: &mut f64, target: f64, delta: f64) -> bool {
    let delta = if *px > target { -delta } else { delta };
    *px += delta;
    if (*px - target) * delta >= 0.0 {
        *px = target;
        return true;
    }
    false
}

/// `clamp_f32` (obj_behaviors_2.c): true when the value had to be clamped.
pub fn clamp_f32(value: &mut f64, min: f64, max: f64) -> bool {
    if *value <= min {
        *value = min;
    } else if *value >= max {
        *value = max;
    } else {
        return false;
    }
    true
}

// ---------------------------------------------------------------------------
// Matrices (math_util.c / object_helpers.c)
// ---------------------------------------------------------------------------

pub type Mat4 = [[f64; 4]; 4];

/// `mtxf_rotate_zxy_and_translate` (math_util.c).
pub fn mtxf_rotate_zxy_and_translate(translate: Vec3, rotate: [f64; 3]) -> Mat4 {
    let (sx, cx) = (sins(rotate[0]), coss(rotate[0]));
    let (sy, cy) = (sins(rotate[1]), coss(rotate[1]));
    let (sz, cz) = (sins(rotate[2]), coss(rotate[2]));
    let mut d = [[0.0; 4]; 4];
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

/// `linear_mtxf_mul_vec3f` (object_helpers.c).
pub fn linear_mtxf_mul_vec3f(m: &Mat4, v: Vec3) -> Vec3 {
    [0, 1, 2].map(|i| m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2])
}

/// `linear_mtxf_transpose_mul_vec3f` (object_helpers.c).
pub fn linear_mtxf_transpose_mul_vec3f(m: &Mat4, v: Vec3) -> Vec3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// `transform_object_vertices` for one vertex: row-vector product, truncated to the s16
/// `TerrainData` it is stored in.
pub fn transform_vertex(m: &Mat4, v: Vec3) -> Vec3 {
    [0, 1, 2].map(|i| to_s16((v[0] * m[0][i] + v[1] * m[1][i] + v[2] * m[2][i] + m[3][i]).trunc()) as f64)
}

// ---------------------------------------------------------------------------
// Object
// ---------------------------------------------------------------------------

/// The `o*` fields of a surface object.
#[derive(Clone, Debug)]
pub struct Obj {
    pub pos: Vec3,
    pub home: Vec3,
    /// `oVelX/Y/Z`; X and Z are what platform displacement carries Mario with.
    pub vel: Vec3,
    /// `oFaceAnglePitch/Yaw/Roll` (s32 in the game, kept unwrapped here).
    pub face: [f64; 3],
    /// `oAngleVelPitch/Yaw/Roll`.
    pub angle_vel: [f64; 3],
    pub move_yaw: f64,
    pub forward_vel: f64,
    pub gravity: f64,
    /// `oBhvParams`.
    pub bparams: u32,
    pub timer: i64,
    pub action: i32,
    pub prev_action: i32,
    pub sub_action: i32,
    /// Hidden objects keep their collision (the game only skips the draw).
    pub visible: bool,
    /// 0..255, for the Bowser BitFS teleport style fades and the sinking-cage fade.
    pub opacity: f64,
    pub scale: f64,
    /// Marked for deletion (`obj_mark_for_deletion`): the collision is removed and the model hidden.
    pub deleted: bool,
    /// `header.gfx.scale`; the collision is scaled with it (`obj_apply_scale_to_matrix`).
    pub scale3: [f64; 3],
    /// `o->transform` when the behavior builds its own (`mtxf_align_terrain_normal`); otherwise
    /// the collision uses `mtxf_rotate_zxy_and_translate(pos, face)`.
    pub transform: Option<Mat4>,
    /// Extra displacement to give Mario this frame (tilting pyramid, which moves him itself).
    pub mario_offset: Vec3,
    /// `OBJ_FLAG_SET_FACE_YAW_TO_MOVE_YAW`.
    pub sync_face_yaw: bool,
}

impl Obj {
    pub fn new(spec: &SolidSpec) -> Self {
        Self {
            pos: spec.pos,
            home: spec.pos,
            vel: [0.0; 3],
            face: spec.angle,
            angle_vel: [0.0; 3],
            move_yaw: spec.angle[1],
            forward_vel: 0.0,
            gravity: 0.0,
            bparams: spec.bparams,
            timer: 0,
            action: 0,
            prev_action: 0,
            sub_action: 0,
            visible: true,
            opacity: 255.0,
            scale: 1.0,
            deleted: false,
            scale3: [1.0; 3],
            transform: None,
            mario_offset: [0.0; 3],
            sync_face_yaw: false,
        }
    }

    /// `oBhvParams2ndByte`.
    pub fn bparam2(&self) -> u32 { (self.bparams >> 16) & 0xFF }
    /// `(u16)(oBhvParams >> 16)` (the sliding platform / octagon tables read the upper half).
    pub fn bparams_hi(&self) -> u32 { (self.bparams >> 16) & 0xFFFF }

    /// `cur_obj_compute_vel_xz`.
    pub fn compute_vel_xz(&mut self) {
        self.vel[0] = self.forward_vel * sins(self.move_yaw);
        self.vel[2] = self.forward_vel * coss(self.move_yaw);
    }

    /// `cur_obj_move_using_vel_and_gravity` (the 12k bounds check never fails for these objects).
    pub fn move_using_vel_and_gravity(&mut self) {
        self.pos[0] += self.vel[0];
        self.pos[2] += self.vel[2];
        self.vel[1] += self.gravity;
        self.pos[1] += self.vel[1];
    }

    /// `cur_obj_move_using_fvel_and_gravity`.
    pub fn move_using_fvel_and_gravity(&mut self) {
        self.compute_vel_xz();
        self.move_using_vel_and_gravity();
    }

    /// `cur_obj_rotate_face_angle_using_vel`.
    pub fn rotate_face_angle_using_vel(&mut self) {
        for i in 0..3 {
            self.face[i] += self.angle_vel[i];
        }
    }

    /// `obj_set_dist_from_home` (obj_behaviors_2.c; note cos for X and sin for Z).
    pub fn set_dist_from_home(&mut self, dist: f64) {
        self.pos[0] = self.home[0] + dist * coss(self.move_yaw);
        self.pos[2] = self.home[2] + dist * sins(self.move_yaw);
    }

    /// `obj_face_yaw_approach`.
    pub fn face_yaw_approach(&mut self, target: f64, delta: f64) -> bool {
        self.face[1] = approach_s16_symmetric(self.face[1], target, delta);
        to_s16(self.face[1]) as f64 == to_s16(target) as f64
    }

    /// `obj_face_roll_approach`.
    pub fn face_roll_approach(&mut self, target: f64, delta: f64) -> bool {
        self.face[2] = approach_s16_symmetric(self.face[2], target, delta);
        to_s16(self.face[2]) as f64 == to_s16(target) as f64
    }

    /// Top of `cur_obj_update` (behavior_script.c): a changed action resets timer and sub-action.
    pub fn begin_frame(&mut self) {
        if self.action != self.prev_action {
            self.timer = 0;
            self.sub_action = 0;
            self.prev_action = self.action;
        }
    }

    /// Bottom of `cur_obj_update`: bump `oTimer`, then re-check the action change.
    pub fn end_frame(&mut self) {
        if self.timer < 0x3FFF_FFFF {
            self.timer += 1;
        }
        if self.action != self.prev_action {
            self.timer = 0;
            self.sub_action = 0;
            self.prev_action = self.action;
        }
    }
}

/// What a behavior can see besides its own object.
pub struct Cx<'a> {
    pub m: &'a MarioRuntime,
    /// `gTTCSpeedSetting`.
    pub ttc_speed: i32,
    /// Mario is standing on this object (`gMarioObject->platform == o`).
    pub riding: bool,
    /// This frame's index into `Solids::list` (for cross-object lookups).
    pub index: usize,
}

impl Cx<'_> {
    pub fn mario_pos(&self) -> Vec3 { self.m.pos }

    /// `cur_obj_nearest_object_with_behavior(bhvBowser)`: the sim object, if any.
    pub fn bowser(&self) -> Option<&crate::object::SimObject> {
        self.m.objects.objects.iter().find(|o| o.behavior == ObjectBehaviorKind::Bowser && o.active)
    }
}

/// A surface-object behavior. `update` is the native function(s) of the behavior script's loop
/// (everything before `load_object_collision_model`); it must not call `begin_frame`/`end_frame`.
pub trait Behavior: Send + Sync {
    /// The behavior's `bhv_*_init` (after `SET_HOME`, which `Obj::new` already did).
    fn init(&mut self, _o: &mut Obj) {}
    fn update(&mut self, o: &mut Obj, cx: &Cx);
    /// Whether this behavior is only a visual (no collision: TTC's 2D cogs).
    fn solid(&self) -> bool { true }
}

pub struct Solid {
    pub spec: SolidSpec,
    pub tag: u32,
    pub o: Obj,
    pub behavior: Box<dyn Behavior>,
    mesh: Option<Arc<SolidMesh>>,
    /// Transform at the start of the last step (view interpolation).
    pub prev_pos: Vec3,
    pub prev_face: [f64; 3],
    built: Option<(Vec3, [f64; 3], bool, [f64; 3], Option<Mat4>)>,
}

impl Solid {
    pub fn model(&self) -> &str { &self.spec.model }
}

pub struct Solids {
    pub list: Vec<Solid>,
    pub ttc_speed: i32,
    specs: Vec<SolidSpec>,
    meshes: HashMap<String, Arc<SolidMesh>>,
}

/// Create the behavior for a spec; `None` when the symbol is not ported (the caller keeps a count).
pub fn make_behavior(spec: &SolidSpec, ttc_speed: i32) -> Option<Box<dyn Behavior>> {
    ttc::make(spec, ttc_speed)
        .or_else(|| platforms::make(spec))
        .or_else(|| bowser_arena::make(spec))
}

impl Solids {
    pub fn new(specs: &[SolidSpec], meshes: &HashMap<String, SolidMesh>, ttc_speed: i32) -> Self {
        let meshes: HashMap<String, Arc<SolidMesh>> = meshes.iter().map(|(k, v)| (k.clone(), Arc::new(v.clone()))).collect();
        let mut s = Self { list: vec![], ttc_speed, specs: specs.to_vec(), meshes };
        s.rebuild_list();
        s
    }

    fn rebuild_list(&mut self) {
        self.list.clear();
        for spec in &self.specs {
            let Some(behavior) = make_behavior(spec, self.ttc_speed) else { continue };
            let mut o = Obj::new(spec);
            // SET_HOME + per-behavior init
            let mut solid_init = behavior;
            solid_init.init(&mut o);
            let mesh = spec.collision.as_ref().and_then(|k| self.meshes.get(k)).cloned();
            let tag = SOLID_TAG + self.list.len() as u32;
            self.list.push(Solid { spec: spec.clone(), tag, prev_pos: o.pos, prev_face: o.face, o, behavior: solid_init, mesh, built: None });
        }
    }

    /// Number of specs the framework has no behavior for.
    pub fn unported(&self) -> usize { self.specs.len() - self.list.len() }

    /// Build the initial collision.
    pub fn install(&mut self, m: &mut MarioRuntime) { self.refresh(m, true); }

    /// New course run / death: restart every object from its placement.
    pub fn reset(&mut self, m: &mut MarioRuntime) {
        self.remove_all(m);
        self.rebuild_list();
        self.refresh(m, true);
    }

    fn remove_all(&mut self, m: &mut MarioRuntime) {
        let world = Arc::make_mut(&mut m.world);
        world.remove_surfaces(&|s| s.object.map_or(false, |t| (SOLID_TAG..SOLID_TAG + 0x10000).contains(&t)));
        for s in &mut self.list { s.built = None; }
    }

    /// `load_object_collision_model` for every object whose transform changed (all when `force`).
    pub fn refresh(&mut self, m: &mut MarioRuntime, force: bool) {
        let dirty: Vec<usize> = self.list.iter().enumerate()
            .filter(|(_, s)| s.mesh.is_some() && s.behavior.solid())
            .filter(|(_, s)| {
                let state = (s.o.pos, s.o.face, s.o.deleted, s.o.scale3, s.o.transform);
                force || s.built != Some(state)
            })
            .map(|(i, _)| i)
            .collect();
        if dirty.is_empty() { return; }
        let tags: HashSet<u32> = dirty.iter().map(|&i| self.list[i].tag).collect();
        let world = Arc::make_mut(&mut m.world);
        world.remove_surfaces(&|s| s.object.map_or(false, |t| tags.contains(&t)));
        for i in dirty {
            let s = &mut self.list[i];
            s.built = Some((s.o.pos, s.o.face, s.o.deleted, s.o.scale3, s.o.transform));
            if s.o.deleted { continue; }
            let matrix = object_matrix(&s.o);
            add_object_surfaces(world, s.mesh.as_ref().unwrap(), &matrix, s.tag);
        }
    }

    /// One 30 Hz tick (before Mario's step, like the platform pass): run the behaviors, carry
    /// Mario with the platform he stands on, rebuild changed collision.
    pub fn step(&mut self, m: &mut MarioRuntime) {
        for i in 0..self.list.len() {
            let tag = self.list[i].tag;
            let riding = m.floor.as_ref().map_or(false, |f| f.object == Some(tag))
                && m.action & ACT_FLAG_AIR == 0
                && (m.pos[1] - m.floor_height).abs() < 4.0;
            let s = &mut self.list[i];
            s.prev_pos = s.o.pos;
            s.prev_face = s.o.face;
            s.o.mario_offset = [0.0; 3];
            s.o.begin_frame();
            let cx = Cx { m, ttc_speed: self.ttc_speed, riding, index: i };
            s.behavior.update(&mut s.o, &cx);
            s.o.end_frame();
            if s.o.sync_face_yaw {
                s.o.face[1] = s.o.move_yaw;
            }
            if s.o.deleted || !s.behavior.solid() {
                continue;
            }
            if riding {
                apply_platform_displacement(m, &s.o);
            }
            // Bowser standing on it (`bowser_free_update`: apply_platform_displacement(FALSE, platform)).
            if let Some(b) = m.objects.objects.iter_mut().find(|b| b.behavior == ObjectBehaviorKind::Bowser && b.active) {
                if b.has_var("platformTag") && b.var("platformTag") == tag as f64 {
                    b.pos = platform_displacement(&s.o, b.pos).0;
                }
            }
        }
        self.refresh(m, false);
    }
}

/// `obj_build_transform_from_pos_and_angle` + `obj_apply_scale_to_matrix` for a surface object.
pub fn object_matrix(o: &Obj) -> Mat4 {
    let mut m = o.transform.unwrap_or_else(|| {
        mtxf_rotate_zxy_and_translate(o.pos, [to_s16(o.face[0]) as f64, to_s16(o.face[1]) as f64, to_s16(o.face[2]) as f64])
    });
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] *= o.scale3[i];
        }
    }
    m
}

/// `load_object_surfaces` for one object.
pub fn add_object_surfaces(world: &mut CollisionWorld, mesh: &SolidMesh, m: &Mat4, tag: u32) {
    let verts: Vec<Vec3> = mesh.vertices.iter().map(|v| transform_vertex(m, *v)).collect();
    for &[a, b, c, kind, force] in &mesh.triangles {
        let (Some(&va), Some(&vb), Some(&vc)) = (verts.get(a as usize), verts.get(b as usize), verts.get(c as usize)) else { continue };
        world.add_triangle(va, vb, vc, &SurfaceOptions { room: None, kind, force, flags: crate::surfaces::SURFACE_FLAG_DYNAMIC, object: Some(tag) });
    }
}

/// `apply_platform_displacement` (platform_displacement.c) for anything standing on `o`:
/// returns the new position and the yaw change to add to the rider's facing.
pub fn platform_displacement(o: &Obj, pos: Vec3) -> (Vec3, f64) {
    let mut p = pos;
    p[0] += o.vel[0];
    p[2] += o.vel[2];
    let rot = [to_s16(o.angle_vel[0]) as f64, to_s16(o.angle_vel[1]) as f64, to_s16(o.angle_vel[2]) as f64];
    let mut dyaw = 0.0;
    if rot[0] != 0.0 || rot[1] != 0.0 || rot[2] != 0.0 {
        dyaw = rot[1];
        let offset = [p[0] - o.pos[0], p[1] - o.pos[1], p[2] - o.pos[2]];
        let before = [0, 1, 2].map(|i| to_s16(o.face[i] - o.angle_vel[i]) as f64);
        let mtx = mtxf_rotate_zxy_and_translate(offset, before);
        let relative = linear_mtxf_transpose_mul_vec3f(&mtx, offset);
        let after = [0, 1, 2].map(|i| to_s16(o.face[i]) as f64);
        let mtx = mtxf_rotate_zxy_and_translate(offset, after);
        let new_offset = linear_mtxf_mul_vec3f(&mtx, relative);
        p = [o.pos[0] + new_offset[0], o.pos[1] + new_offset[1], o.pos[2] + new_offset[2]];
    }
    (p, dyaw)
}

/// `apply_platform_displacement(TRUE, platform)`.
pub fn apply_platform_displacement(m: &mut MarioRuntime, o: &Obj) {
    let (p, dyaw) = platform_displacement(o, m.pos);
    m.face_angle[1] = to_s16(m.face_angle[1] + dyaw) as f64;
    m.pos = p;
    m.pos[0] += o.mario_offset[0];
    m.pos[1] += o.mario_offset[1];
    m.pos[2] += o.mario_offset[2];
}

#[cfg(test)]
mod tests;
