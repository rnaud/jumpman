//! Course loading: `assets/courses/*.json` -> collision world, spawn, warps/doors, water and the
//! [`ObjectManager`] (ports of `sm64World.ts` `prepareArea`, `world.ts` types, `courseObjects.ts`
//! spec building). Everything here is engine-free (no Bevy types) so it can be unit tested.
use super::course_data::{CourseData, CourseObject, Door, FadingWarp, WarpSpawn, WarpTarget, V3};
use super::native_collision::{add_native_collision, formation_coins};
use jumpman_sim::collision::{CollisionWorld, WaterBox};
use jumpman_sim::math::deg_to_angle;
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::object::{BallRoute, ObjectBehaviorKind, ObjectSpec, PenguinRace};
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::path::Path;
use std::sync::Arc;

/// What to load (`prepareCourse` arguments).
#[derive(Clone, Debug)]
pub struct CourseRequest {
    pub id: String,
    pub area: i32,
    pub act: i32,
    /// Warp node to arrive through (`WARP_NODE_xx`), if any.
    pub entry: Option<String>,
    /// Arriving through a door by pushing (spawn on the far side).
    pub push: bool,
}

impl CourseRequest {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string(), area: 1, act: 1, entry: None, push: false }
    }
}

/// A non-interactive decoration (a course object without a behavior).
#[derive(Clone, Debug)]
pub struct Prop {
    pub model: String,
    pub pos: V3,
    /// Degrees about Y.
    pub yaw: f64,
    /// s16 (pitch, yaw, roll) override (`rotation` in the export, used by ship parts).
    pub rotation: Option<V3>,
    pub scale: f64,
    /// Rotate to face the camera (trees).
    pub billboard: bool,
}

/// What to draw for the static course geometry.
#[derive(Clone, Debug)]
pub enum CourseVisual {
    /// Display model(s) from the export; the first is shown (room selection is a TODO).
    Glb { models: Vec<String> },
    /// Procedural movement playground.
    Playground(Vec<super::playground::Piece>),
}

/// `warpActions`: the action Mario starts in when arriving via each warp behavior.
pub fn warp_action(behavior: &str) -> Option<u32> {
    Some(match behavior {
        "bhvDeathWarp" => ACT_FALLING_DEATH_EXIT,
        "bhvPaintingDeathWarp" => ACT_DEATH_EXIT,
        "bhvAirborneDeathWarp" => ACT_UNUSED_DEATH_EXIT,
        "bhvLaunchDeathWarp" => ACT_SPECIAL_DEATH_EXIT,
        "bhvFlyingWarp" => ACT_FLYING,
        "bhvSwimmingWarp" => ACT_WATER_IDLE,
        "bhvLaunchStarCollectWarp" => ACT_SPECIAL_EXIT_AIRBORNE,
        "bhvAirborneWarp" => ACT_SPAWN_NO_SPIN_AIRBORNE,
        "bhvSpinAirborneCircleWarp" | "bhvSpinAirborneWarp" => ACT_SPAWN_SPIN_AIRBORNE,
        "bhvPaintingStarCollectWarp" => ACT_EXIT_AIRBORNE,
        "bhvAirborneStarCollectWarp" => ACT_FALLING_EXIT_AIRBORNE,
        _ => return None,
    })
}

pub struct LoadedCourse {
    pub request: CourseRequest,
    pub name: String,
    pub visual: CourseVisual,
    pub world: Arc<CollisionWorld>,
    pub spawn: SpawnPoint,
    pub death_height: f64,
    pub objects: ObjectManager,
    pub props: Vec<Prop>,
    /// Static coins (plain coins + formations) and red coins.
    pub coins: Vec<V3>,
    pub red_coins: Vec<V3>,
    pub doors: Vec<Door>,
    pub warp_spawns: std::collections::HashMap<String, WarpSpawn>,
    pub fading_warps: Vec<FadingWarp>,
    pub death_warp: Option<WarpTarget>,
    pub fall_warp: Option<WarpTarget>,
    pub success_entry: Option<String>,
    pub success_area: Option<i32>,
    /// Specs for behaviors the sim has no kind for yet (kept for diagnostics).
    pub skipped_behaviors: Vec<String>,
}

impl LoadedCourse {
    pub fn load(root: &Path, request: CourseRequest) -> Result<Self, String> {
        if request.id == "playground" {
            return Ok(super::playground::load(request));
        }
        let data = CourseData::load(root, &request.id, request.area)?;
        Self::from_data(data, request)
    }

    pub fn from_data(mut data: CourseData, request: CourseRequest) -> Result<Self, String> {
        let id = request.id.as_str();
        let act = request.act;

        // Static, native collision.
        let mut world = CollisionWorld::new(
            jumpman_sim::collision::CELL_SIZE,
            data.horizontal_boundary.unwrap_or(jumpman_sim::collision::LEVEL_BOUNDARY_MAX),
        );
        world.slide_terrain = data.slide_terrain;
        world.snow_terrain = data.snow_terrain;
        world.water_boxes = data.water_boxes.iter().map(|b| WaterBox(b[0], b[1], b[2], b[3], b[4], b[5])).collect();
        add_native_collision(&mut world, &data.collision, [0.0; 3], 0.0);

        // The TS adds the Bob seesaw display model + collision in code.
        if id == "bob" {
            data.objects.push(CourseObject {
                model: "levels/bob/bob_geo_000458.glb".into(),
                pos: [-2303.0, 717.0, 1024.0],
                yaw: 45.0,
                param: None,
                behavior: None,
                acts: None,
                warp_to: None,
                warp_entry: None,
                warp_area: None,
                collision: Some("seesaw_platform/collision.inc.c".into()),
                actor_scale: None,
                rotation: None,
                billboard: false,
                grounded_prop: false,
            });
        }
        for warp in &data.warps {
            data.objects.push(CourseObject {
                model: String::new(),
                pos: warp.pos,
                yaw: 0.0,
                param: Some(warp.param),
                behavior: Some("warp".into()),
                acts: None,
                warp_to: Some(warp.warp_to.clone()),
                warp_entry: Some(warp.warp_entry.clone()),
                warp_area: warp.warp_area,
                collision: None,
                actor_scale: None,
                rotation: None,
                billboard: false,
                grounded_prop: false,
            });
        }

        let active: Vec<&CourseObject> = data
            .objects
            .iter()
            .filter(|o| o.acts.as_ref().map_or(true, |a| a.is_empty() || a.contains(&act)))
            .collect();

        // Static decoration + static collision for behavior-less objects.
        let mut props = vec![];
        for object in active.iter().filter(|o| o.behavior.is_none()) {
            let mut pos = object.pos;
            if object.grounded_prop {
                let hit = world.find_floor(pos[0], pos[1] + 150.0, pos[2]);
                if hit.surface.is_some() {
                    pos[1] = hit.height;
                }
            }
            if !object.model.is_empty() {
                props.push(Prop {
                    model: object.model.clone(),
                    pos,
                    yaw: object.yaw,
                    rotation: object.rotation,
                    scale: object.actor_scale.unwrap_or(1.0),
                    billboard: object.billboard || object.model == "actors/tree/bubbly_tree_geo.glb",
                });
            }
            if let Some(extra) = object.collision.as_ref().and_then(|k| data.extra_collision.get(k)) {
                // TODO(moving-platforms): lifts/rotating platforms are static at their home pose.
                add_native_collision(&mut world, extra, pos, object.yaw);
            }
        }

        // Spawn.
        let spawn = resolve_spawn(&data, &world, &request);

        // Static coins.
        let mut coins = data.coins.clone();
        for f in &data.coin_formations {
            coins.extend(formation_coins(f, &world));
        }

        // Interactive objects.
        let mut skipped = vec![];
        let mut specs: Vec<ObjectSpec> = vec![];
        let mut push_spec = |o: &CourseObject, ball_route: Option<BallRoute>| {
            let Some(name) = o.behavior.as_deref() else { return };
            let Some(kind) = ObjectBehaviorKind::from_name(name) else {
                skipped.push(name.to_string());
                return;
            };
            let mut s = ObjectSpec::new(kind, o.pos, deg_to_angle(o.yaw) as f64);
            s.param = Some(o.param.unwrap_or(0.0));
            s.model = if o.model.is_empty() { None } else { Some(o.model.clone()) };
            s.warp_to = o.warp_to.clone();
            s.warp_area = o.warp_area;
            s.warp_entry = o.warp_entry.clone();
            s.ball_route = ball_route;
            specs.push(s);
        };
        for o in &active {
            push_spec(o, None);
        }
        for sp in data.mountain_ball_spawners.iter().filter(|s| s.acts.contains(&act)) {
            let Some(path) = data.mountain_ball_paths.get(sp.path) else { continue };
            let o = CourseObject {
                model: String::new(),
                pos: sp.pos,
                yaw: 0.0,
                param: None,
                behavior: Some("mountainBallSpawner".into()),
                acts: None,
                warp_to: None,
                warp_entry: None,
                warp_area: None,
                collision: None,
                actor_scale: None,
                rotation: None,
                billboard: false,
                grounded_prop: false,
            };
            push_spec(
                &o,
                Some(BallRoute {
                    waypoints: path.clone(),
                    period: sp.period,
                    max_distance: if sp.path == 0 { 7000.0 } else { 6000.0 },
                }),
            );
        }
        for sp in &data.water_bomb_spawners {
            let o = CourseObject {
                model: String::new(),
                pos: sp.pos,
                yaw: 0.0,
                param: Some(sp.param),
                behavior: Some("waterBombSpawner".into()),
                acts: None,
                warp_to: None,
                warp_entry: None,
                warp_area: None,
                collision: None,
                actor_scale: None,
                rotation: None,
                billboard: false,
                grounded_prop: false,
            };
            push_spec(&o, None);
        }
        let penguin = data.penguin_race.as_ref().map(|p| PenguinRace {
            path: p.path.clone(),
            finish: p.finish,
            shortcut: p.shortcut,
        });
        let objects = ObjectManager::new(specs, penguin, data.snowman_path.clone());

        let min_y = data.collision.vertices.iter().map(|v| v[1]).fold(f64::INFINITY, f64::min);
        let models = if data.room_models.is_empty() { vec![data.model.clone()] } else { data.room_models.clone() };
        let name = super::courses::info(id).map_or(id.to_string(), |c| c.name.to_string());
        Ok(Self {
            name,
            visual: CourseVisual::Glb { models },
            world: Arc::new(world),
            spawn,
            death_height: if min_y.is_finite() { min_y - 500.0 } else { -1600.0 },
            objects,
            props,
            coins,
            red_coins: data.red_coins.clone(),
            doors: data.doors.clone(),
            warp_spawns: data.warp_spawns.clone(),
            fading_warps: data.fading_warps.clone(),
            death_warp: data.death_warp.clone(),
            fall_warp: data.fall_warp.clone(),
            success_entry: data.success_entry.clone(),
            success_area: data.success_area,
            skipped_behaviors: skipped,
            request,
        })
    }

    /// Water surface height at a point, if any (the first box, like the TS `waterBoxes[0]`).
    pub fn water_boxes(&self) -> &[WaterBox] {
        &self.world.water_boxes
    }
}

/// Spawn selection from `prepareArea`: door arrival, warp-node arrival, or the course spawn
/// snapped onto the floor.
pub fn resolve_spawn(data: &CourseData, world: &CollisionWorld, request: &CourseRequest) -> SpawnPoint {
    let mut entry = request.entry.clone();
    if entry.is_none() && request.id == "totwc" {
        entry = Some("WARP_NODE_0A".into());
    }
    if entry.is_none() && request.id == "ccm" && request.area == 2 {
        entry = Some("WARP_NODE_0A".into());
    }
    let mut spawn = SpawnPoint {
        action: data.spawn.action,
        pos: data.spawn.pos,
        face_yaw: data.spawn.face_yaw,
    };
    if let Some(node) = entry.as_deref() {
        if let Some(door) = data.doors.iter().find(|d| d.node == node) {
            let angle = door.yaw.to_radians() + if request.push { std::f64::consts::PI } else { 0.0 };
            spawn = SpawnPoint {
                action: None,
                pos: [door.pos[0] + 300.0 * angle.sin(), door.pos[1], door.pos[2] + 300.0 * angle.cos()],
                face_yaw: (angle * 32768.0 / std::f64::consts::PI).round(),
            };
        }
        if let Some(w) = data.warp_spawns.get(node) {
            spawn = SpawnPoint { action: warp_action(&w.behavior), pos: w.pos, face_yaw: w.face_yaw };
        }
    }
    if spawn.action.is_none() {
        let hit = world.find_floor(spawn.pos[0], spawn.pos[1] + 200.0, spawn.pos[2]);
        if hit.surface.is_some() {
            spawn.pos[1] = hit.height;
        }
    }
    spawn
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn every_course_loads() {
        for info in super::super::courses::COURSES {
            if info.id == "playground" || info.id == "mushroom2d" {
                continue;
            }
            if !CourseData::path(&assets(), info.id, 1).is_file() {
                continue;
            }
            let c = LoadedCourse::load(&assets(), CourseRequest::new(info.id)).unwrap_or_else(|e| panic!("{}: {e}", info.id));
            assert!(c.world.surfaces().len() > 0, "{}", info.id);
        }
    }

    /// The loader must build the exact collision world the sim golden tests use
    /// (tests/mario_run.rs `world("bob")`).
    #[test]
    fn bob_collision_matches_golden_world() {
        let data: serde_json::Value = serde_json::from_slice(&std::fs::read(assets().join("courses/bob.json")).unwrap()).unwrap();
        let expected = data["triangles"].as_array().unwrap().len();
        let c = LoadedCourse::load(&assets(), CourseRequest::new("bob")).unwrap();
        // Native triangles first (degenerate ones are dropped like in the TS), then object collision.
        let mut reference = CollisionWorld::default();
        add_native_collision(&mut reference, &CourseData::load(&assets(), "bob", 1).unwrap().collision, [0.0; 3], 0.0);
        assert!(reference.surfaces().len() <= expected);
        for (a, b) in reference.surfaces().iter().zip(c.world.surfaces()) {
            assert_eq!(a.v1, b.v1);
            assert_eq!(a.normal, b.normal);
            assert_eq!(a.kind, b.kind);
        }
        let hit = c.world.find_floor(c.spawn.pos[0], c.spawn.pos[1] + 200.0, c.spawn.pos[2]);
        assert!(hit.surface.is_some());
        assert_eq!(hit.height, c.spawn.pos[1]);
        assert!(c.objects.len() > 20);
    }

    #[test]
    fn warp_entry_selects_warp_spawn() {
        let mut req = CourseRequest::new("bob");
        req.entry = Some("WARP_NODE_0A".into());
        let c = LoadedCourse::load(&assets(), req).unwrap();
        assert_eq!(c.spawn.action, Some(ACT_SPAWN_SPIN_AIRBORNE));
        assert_eq!(c.spawn.pos, [-6558.0, 1000.0, 6464.0]);
    }

    #[test]
    fn castle_door_entry_spawns_in_front() {
        let mut req = CourseRequest::new("castle_grounds");
        req.entry = Some("WARP_NODE_01".into());
        let c = LoadedCourse::load(&assets(), req).unwrap();
        assert!(c.spawn.action.is_none());
    }
}
