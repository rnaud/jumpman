//! Course data the object layer needs beyond the sim's `ObjectSpec`s: moving platforms,
//! signposts, cannons, walking goombas and the exported `extraCollision` meshes
//! (the parts of `sm64World.ts` that build `MovingPlatform`, `Signpost`, `Cannons`, `WalkingGoomba`).
use super::course_data::{CannonPlacement, CollisionData, CourseData, CourseObject, Goomba, V3};
use std::collections::HashMap;

/// Collision kinds that `sm64World.ts` turns into a `MovingPlatform` (first path segment).
pub const PLATFORM_KINDS: &[&str] = &[
    "checkerboard_lift",
    "ropeway_lift",
    "tower_solid",
    "tower_slide",
    "tower_elevator",
    "seesaw_platform",
    "rotating_platform",
    "rotating_wooden_platform",
    "sliding_platform",
    "small_bomp",
    "large_bomp",
    "mesh_elevator",
    "merry_go_round",
    "moving_bookshelf",
    "tilting_trap_platform",
];

/// The glb -> collision table (`extras` in `prepareArea`): display models whose collision is implied.
pub fn implied_collision(model: &str) -> Option<&'static str> {
    Some(match model {
        "levels/bob/bob_geo_000458.glb" => "seesaw_platform/collision.inc.c",
        "levels/wf/wf_geo_0008A8.glb" => "areas/1/10/collision.inc.c",
        "levels/wf/wf_geo_0008E8.glb" => "areas/1/11/collision.inc.c",
        _ => return None,
    })
}

/// Platform kind of a collision key (`kind` in `prepareArea`).
pub fn platform_kind(collision_key: &str) -> Option<&'static str> {
    let kind = collision_key.split('/').next().unwrap_or("");
    PLATFORM_KINDS.iter().copied().find(|k| *k == kind)
}

#[derive(Clone, Debug)]
pub struct PlatformSpec {
    pub model: String,
    pub pos: V3,
    /// Degrees about Y.
    pub yaw: f64,
    pub kind: &'static str,
    pub param: f64,
    pub track: Option<Vec<V3>>,
    pub scale: f64,
    pub collision: CollisionData,
}

#[derive(Clone, Debug)]
pub struct Signpost {
    pub pos: V3,
    /// s16 yaw.
    pub yaw: f64,
    pub pages: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CourseExtras {
    pub platforms: Vec<PlatformSpec>,
    pub signs: Vec<Signpost>,
    /// Cannon spots active in this act.
    pub cannons: Vec<CannonPlacement>,
    pub goombas: Vec<Goomba>,
    pub extra_collision: HashMap<String, CollisionData>,
}

impl CourseExtras {
    pub fn build(data: &CourseData, active: &[&CourseObject], act: i32, id: &str) -> Self {
        let mut platforms = vec![];
        let mut signs = vec![];
        for o in active.iter().filter(|o| o.behavior.is_none()) {
            if let Some(key) = Self::platform_key(o, data) {
                let kind = platform_kind(key).unwrap();
                let default_param = match kind {
                    "rotating_platform" => 8.0,
                    "sliding_platform" => 2.0,
                    _ => 0.0,
                };
                platforms.push(PlatformSpec {
                    model: o.model.clone(),
                    pos: o.pos,
                    yaw: o.yaw,
                    kind,
                    param: o.param.unwrap_or(default_param),
                    track: o.track.clone(),
                    scale: o.actor_scale.unwrap_or(1.0),
                    collision: data.extra_collision[key].clone(),
                });
            }
            if o.collision.as_deref() == Some("signpost") && !o.pages.is_empty() {
                signs.push(Signpost { pos: o.pos, yaw: (o.yaw * 32768.0 / 180.0).round(), pages: o.pages.clone() });
            }
        }
        let mut goombas = data.goombas.clone();
        if goombas.is_empty() && id == "bob" {
            goombas = vec![Goomba { pos: [-4800.0, 20000.0, 5700.0], triplet: false }, Goomba { pos: [-3700.0, 20000.0, 5300.0], triplet: false }];
        }
        let cannons = data.cannons.iter().filter(|c| c.acts.as_ref().map_or(true, |a| a.is_empty() || a.contains(&act))).cloned().collect();
        Self { platforms, signs, cannons, goombas, extra_collision: data.extra_collision.clone() }
    }

    /// The `extraCollision` key when this behavior-less object is a moving platform.
    pub fn platform_key<'a>(o: &'a CourseObject, data: &CourseData) -> Option<&'a str> {
        if o.behavior.is_some() {
            return None;
        }
        let key = o.collision.as_deref().or_else(|| implied_collision(&o.model))?;
        (platform_kind(key).is_some() && data.extra_collision.contains_key(key)).then_some(key)
    }
}
