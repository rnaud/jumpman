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

/// `Surface::object` tag of static collision that a surface object (`jumpman_sim::solids`) replaces.
pub const STATIC_SOLID_TAG: u32 = 0x4200_0000;

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
    /// SM64 group 2 (courses 7-13): load the objects, pipes and warps marked `g2` in the course json. On in the
    /// game, off in the unit tests that replay the TypeScript goldens (which know nothing about them).
    pub g2: bool,
}

impl CourseRequest {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string(), area: 1, act: 1, entry: None, push: false, g2: !cfg!(test) }
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
    /// `scrollT`: texture v scroll per second (0 = static).
    pub scroll_t: f64,
    /// `spinZ`: radians per second about the local Z axis (0 = static).
    pub spin_z: f64,
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
    pub sky: Option<super::course_data::Sky>,
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
    pub success_course: Option<String>,
    /// Specs for behaviors the sim has no kind for yet (kept for diagnostics).
    pub skipped_behaviors: Vec<String>,
    /// Platforms, signposts, cannons, goombas and exported collision meshes for the object layer.
    pub extras: super::course_extras::CourseExtras,
}

thread_local! {
    static LEGACY_FIXTURES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The TypeScript-generated differential fixtures (longrun, focus, game, game2, warps) were
/// recorded before the SM64 group 1 actors existed and hash every object. While this thread-local
/// flag is set, `from_data` drops the objects added by `export-sm64-g1-actors.py`, so those
/// fixtures keep describing the original object set. Test-only; the game never sets it.
pub fn legacy_fixture_mode(on: bool) {
    LEGACY_FIXTURES.with(|c| c.set(on));
}

impl LoadedCourse {
    pub fn load(root: &Path, request: CourseRequest) -> Result<Self, String> {
        if request.id == "playground" {
            return Ok(super::playground::load(request));
        }
        if request.id == "mushroom2d" {
            return Ok(super::mushroom2d::load(request));
        }
        let data = CourseData::load(root, &request.id, request.area)?;
        let mut course = Self::from_data(data, request)?;
        // Zelda scenes name their arrival points (`kokiri.arrivals`).
        if let Some(spawn) = crate::zelda::scene::arrival_spawn(root, &course.request) {
            course.spawn = spawn;
        }
        Ok(course)
    }

    pub fn from_data(mut data: CourseData, request: CourseRequest) -> Result<Self, String> {
        let id = request.id.as_str();
        let act = request.act;
        if LEGACY_FIXTURES.with(|c| c.get()) {
            data.objects.retain(|o| !o.g1);
        }

        // Static, native collision.
        let mut world = CollisionWorld::new(
            jumpman_sim::collision::CELL_SIZE,
            data.horizontal_boundary.unwrap_or(jumpman_sim::collision::LEVEL_BOUNDARY_MAX),
        );
        world.slide_terrain = data.slide_terrain;
        world.snow_terrain = data.snow_terrain;
        world.water_boxes = data.water_boxes.iter().map(|b| WaterBox(b[0], b[1], b[2], b[3], b[4], b[5])).collect();
        // level_cmd_create_whirlpool: later commands with the same index replace earlier ones.
        // The Bowser-2 condition is read as "not beaten" (the course request carries no save).
        let mut whirlpools: Vec<Option<jumpman_sim::collision::Whirlpool>> = vec![None, None];
        for w in data.whirlpools.iter().filter(|w| w.index < 2 && match w.cond { 0 => true, 1 => true, 2 => false, 3 => act >= 2, _ => false }) {
            let radius = jumpman_sim::actions::water::whirlpool_radius(id, request.area);
            whirlpools[w.index] = Some(jumpman_sim::actions::water::make_whirlpool(w.pos, w.strength, radius));
        }
        world.whirlpools = whirlpools.into_iter().flatten().collect();
        add_native_collision(&mut world, &data.collision, [0.0; 3], 0.0);
        // Static ship parts (rotated by an Euler 'YXZ', like `shipAngles` in `prepareArea`).
        for part in data.ship_collision.iter().filter(|p| p.acts.contains(&request.act)) {
            let Some(source) = data.extra_collision.get(&part.name) else { continue };
            let mut rotated = source.clone();
            for v in &mut rotated.vertices {
                *v = super::three_math::apply_euler_yxz(*v, part.rotation[0], part.rotation[1], part.rotation[2]);
            }
            add_native_collision(&mut world, &rotated, part.pos, 0.0);
        }

        // The TS adds the Bob seesaw display model + collision in code.
        if id == "bob" {
            data.objects.push(CourseObject {
                model: "levels/bob/bob_geo_000458.glb".into(),
                pos: [-2303.0, 717.0, 1024.0],
                yaw: 45.0,
                param: None,
                behavior: None,
                collision: Some("seesaw_platform/collision.inc.c".into()),
                ..Default::default()
            });
        }
        for warp in data.warps.iter().filter(|w| request.g2 || !w.g2) {
            data.objects.push(CourseObject {
                model: String::new(),
                pos: warp.pos,
                yaw: 0.0,
                param: Some(warp.param),
                behavior: Some("warp".into()),
                warp_to: Some(warp.warp_to.clone()),
                warp_entry: Some(warp.warp_entry.clone()),
                warp_area: warp.warp_area,
                ..Default::default()
            });
        }

        // `prepareArea`: outside the snowman act a hidden copy of the body marks where the finished
        // snowman stands (the head is moved onto it when the star is owned).
        if data.snowman_star.as_ref().is_some_and(|s| s.act != act) {
            if let Some(body) = data.objects.iter().find(|o| o.behavior.as_deref() == Some("snowmanBody")).cloned() {
                let mut body = body;
                body.param = Some(-1.0);
                body.pos = [-4230.0, -1344.0, 1813.0];
                body.acts = Some(vec![act]);
                data.objects.push(body);
            }
        }
        let active: Vec<&CourseObject> = data
            .objects
            .iter()
            .filter(|o| o.acts.as_ref().map_or(true, |a| a.is_empty() || a.contains(&act)))
            .filter(|o| request.g2 || !(o.g2 || o.pb))
            .collect();

        let mut extras = super::course_extras::CourseExtras::build(&data, &active, act, id);
        // Models drawn by SM64 surface objects (`objects::solids`), not as static props.
        let solid_models: std::collections::HashSet<&str> = data.solids.iter().map(|s| s.model.as_str()).filter(|m| !m.is_empty()).collect();
        // SM64 group 2: collision models of dynamic platforms (`extraCollision` keys `g2/...`).
        for (key, mesh) in data.extra_collision.iter().filter(|(k, _)| k.starts_with("g2/")) {
            jumpman_sim::behaviors::g2::dyn_collision::register_mesh(
                key,
                jumpman_sim::behaviors::g2::dyn_collision::CollisionMesh { vertices: mesh.vertices.clone(), triangles: mesh.triangles.clone() },
            );
        }
        let mut sign_floors = vec![];

        // Static decoration + static collision for behavior-less objects.
        let mut props = vec![];
        for object in active.iter().filter(|o| o.behavior.is_none()) {
            // Moving platforms are drawn and collided by `objects::platforms` (movingPlatform.ts); Zelda scene
            // actors (NPCs, chests, rupees, dungeon parts) are instantiated by `zelda::view`.
            if super::course_extras::CourseExtras::platform_key(object, &data).is_some() || crate::zelda::scene::is_managed(object) {
                continue;
            }
            // The static copy of a model that a surface object draws and collides: keep its collision
            // under `STATIC_SOLID_TAG` (spawn resolution and the headless goldens see it); the app
            // removes it when the surface object takes over (`objects::solids`).
            if solid_models.contains(object.model.as_str()) {
                let key = object.collision.as_deref().or_else(|| super::course_extras::implied_collision(&object.model));
                if let Some(extra) = key.and_then(|k| data.extra_collision.get(k)) {
                    super::native_collision::add_native_collision_tagged(&mut world, extra, object.pos, object.yaw, Some(STATIC_SOLID_TAG));
                }
                continue;
            }
            let mut pos = object.pos;
            if !object.pages.is_empty() {
                // Signposts stand on whatever floor is below them at this point of the object loop.
                pos[1] = world.find_floor(pos[0], pos[1] + 200.0, pos[2]).height;
                if object.collision.as_deref() == Some("signpost") {
                    sign_floors.push(pos[1]);
                }
            }
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
                    scroll_t: object.scroll_t.unwrap_or(0.0),
                    spin_z: object.spin_z.unwrap_or(0.0),
                });
            }
            // `extras[object.model]` in `prepareArea`: some display models imply their collision.
            let key = object.collision.as_deref().or_else(|| super::course_extras::implied_collision(&object.model));
            if let Some(extra) = key.and_then(|k| data.extra_collision.get(k)) {
                add_native_collision(&mut world, extra, pos, object.yaw);
            }
        }

        for (sign, y) in extras.signs.iter_mut().zip(sign_floors) {
            sign.pos[1] = y;
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
            // The swimming set's objects exist only with the faithful water code (the TS fixtures count objects).
            if matches!(name, "koopaShellUnderwater" | "whirlpool") && !jumpman_sim::actions::water::faithful() { return; }
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
            // Mips carries its ten escape paths in `track` (behaviors/g1/mips.rs); the THI Koopa the Quick his race route.
            let route = matches!(o.behavior.as_deref(), Some("mips" | "koopaTheQuick"))
                .then(|| o.track.clone())
                .flatten()
                .map(|waypoints| BallRoute::mountain(waypoints, 0.0, 0.0));
            push_spec(o, route);
        }
        for sp in data.mountain_ball_spawners.iter().filter(|s| s.acts.contains(&act)) {
            let Some(path) = data.mountain_ball_paths.get(sp.path) else { continue };
            let o = CourseObject {
                model: String::new(),
                pos: sp.pos,
                yaw: 0.0,
                param: None,
                behavior: Some("mountainBallSpawner".into()),
                ..Default::default()
            };
            push_spec(
                &o,
                Some({
                    let mut route = BallRoute::mountain(path.clone(), sp.period, sp.max_distance.unwrap_or(if sp.path == 0 { 7000.0 } else { 6000.0 }));
                    route.min_distance = sp.min_distance.unwrap_or(route.min_distance);
                    route.odds = sp.odds.unwrap_or(route.odds);
                    route.speed = sp.speed.unwrap_or(route.speed);
                    route.scale = sp.scale.unwrap_or(route.scale);
                    route
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
                ..Default::default()
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
            sky: data.sky,
            objects,
            props,
            coins,
            red_coins: data.red_coins.clone(),
            doors: data.doors.clone(),
            warp_spawns: data.warp_spawns.clone(),
            fading_warps: data.fading_warps.iter().filter(|w| request.g2 || !w.g2).cloned().collect(),
            death_warp: data.death_warp.clone(),
            fall_warp: data.fall_warp.clone(),
            success_entry: data.success_entry.clone(),
            success_area: data.success_area,
            success_course: data.success_course.clone(),
            skipped_behaviors: skipped,
            extras,
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
    // Standalone entry to an area whose course spawn is not where the player should start.
    if request.id == "jrb" && request.area == 2 && request.entry.is_none() {
        spawn = SpawnPoint { action: Some(ACT_WATER_IDLE), pos: [928.0, 1050.0, -1248.0], face_yaw: -32768.0 };
    }
    if request.id == "ddd" && request.area == 2 {
        spawn = SpawnPoint { action: None, pos: [3900.0, 1000.0, -600.0], face_yaw: 0.0 };
    }
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
    // The Peach cutscene (`sm64World.ts` `ending`): Mario starts high above the grounds.
    if request.id == "castle_grounds" && entry.as_deref() == Some("WARP_NODE_CREDITS_START") {
        spawn = SpawnPoint { action: Some(ACT_END_PEACH_CUTSCENE), pos: [0.0, 8000.0, 0.0], face_yaw: -32768.0 };
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
