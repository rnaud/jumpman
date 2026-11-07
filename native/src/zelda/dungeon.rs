//! Port of `dekuDungeon.ts`: the Inside-the-Deku-Tree puzzle logic (lobby ladder and web, push
//! block, burning stick, Deku Scrub nut deflecting, the water room ferry / spiked log / switch,
//! the Scrub brothers and the boss door). Engine-free: parts carry their pose/visibility for the
//! view and their (movable) collision for the sim.
use super::save::ZeldaSave;
use super::scene::ZeldaScene;
use super::slingshot::SeedTarget;
use crate::game::course_data::{CollisionData, CourseObject};
use crate::game::native_collision::add_native_collision;
use jumpman_sim::collision::{CollisionWorld, Surface};
use jumpman_sim::math::Vec3;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;
use std::collections::HashMap;
use std::sync::Arc;

const SCALE: f64 = 4.4;

/// `DoorCollider`: a block of collision that can be moved or removed at run time.
#[derive(Clone)]
pub struct Collider {
    data: CollisionData,
    yaw: f64,
    faces: Vec<Arc<Surface>>,
    /// What is currently in the world (`None` = not yet placed).
    applied: Option<Option<Vec3>>,
}

impl Collider {
    pub fn new(data: CollisionData, yaw_degrees: f64) -> Self {
        Self { data, yaw: yaw_degrees, faces: vec![], applied: None }
    }

    pub fn owns(&self, surface: &Option<Arc<Surface>>) -> bool {
        surface.as_ref().is_some_and(|s| self.faces.iter().any(|f| Arc::ptr_eq(f, s)))
    }

    pub fn move_to(&mut self, world: &mut CollisionWorld, pos: Option<Vec3>) {
        self.applied = Some(pos);
        if !self.faces.is_empty() {
            let owned: Vec<*const Surface> = self.faces.iter().map(Arc::as_ptr).collect();
            world.remove_surfaces(&|s| owned.contains(&(s as *const Surface)));
            self.faces.clear();
        }
        let Some(pos) = pos else { return };
        let start = world.surfaces().len();
        add_native_collision(world, &self.data, pos, self.yaw);
        self.faces = world.surfaces()[start..].to_vec();
    }
}

/// One scene object with a `dungeonRole`.
pub struct Part {
    pub model: String,
    pub home: Vec3,
    pub pos: Vec3,
    pub yaw: f64,
    pub scale: f64,
    pub visible: bool,
    /// Extra rotation about X in radians (the spiked log).
    pub rot_x: f64,
    pub collider: Option<Collider>,
}

#[derive(Clone)]
struct Traverse {
    from: Vec3,
    to: Vec3,
    ticks: f64,
    elapsed: f64,
    arc: f64,
    done: Done,
}

#[derive(Clone, Copy, PartialEq)]
enum Done {
    None,
    DrainWater,
}

struct Route {
    from: Vec3,
    to: Vec3,
    prompt: &'static str,
    arc: f64,
    ticks: f64,
    one_way: bool,
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn lerp(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}
fn len(a: Vec3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn dist(a: Vec3, b: Vec3) -> f64 {
    len(sub(a, b))
}
fn scaled_to(a: Vec3, l: f64) -> Vec3 {
    let n = len(a);
    if n == 0.0 {
        return [0.0; 3];
    }
    [a[0] / n * l, a[1] / n * l, a[2] / n * l]
}

#[derive(Default)]
pub struct DungeonStep {
    /// The step handled the tick (the player is not simulated).
    pub handled: bool,
    /// Walk into Gohma's lair.
    pub enter_boss: bool,
}

pub struct DekuDungeon {
    pub parts: HashMap<String, Part>,
    routes: Vec<Route>,
    climb: Option<Traverse>,
    flame_ticks: i32,
    push_ticks: i32,
    brothers_solved: usize,
    shooter: String,
    pub flame_visible: bool,
    pub flame_pos: Vec3,
    pub torch_pos: Vec3,
    peak: f64,
    ladder_drop: f64,
    clock: u32,
    water_timer: i32,
    water_drop: f64,
    normal_water: f64,
    water_level: f64,
    ridden: bool,
    nut_age: u32,
    reflected: bool,
    pub nut_visible: bool,
    pub nut_pos: Vec3,
    nut_vel: Vec3,
    actor_pos: Vec3,
    pub prompt: Option<String>,
    pub toasts: Vec<String>,
    origin: Vec3,
}

impl DekuDungeon {
    /// Build from the scene's role objects; the colliders are placed by [`DekuDungeon::restore`].
    pub fn new(scene: &ZeldaScene, world: &CollisionWorld) -> Option<Self> {
        let mut parts = HashMap::new();
        for o in scene.objects.iter().filter(|o| o.dungeon_role.is_some()) {
            parts.insert(o.dungeon_role.clone().unwrap(), Self::part_from(o, scene));
        }
        let origin = parts.get("web")?.home;
        let at = |x: f64, y: f64, z: f64| add([x * SCALE, y * SCALE, z * SCALE], origin);
        let r = |a: Vec3, b: Vec3, prompt: &'static str, ticks: f64, arc: f64, one_way: bool| Route { from: at(a[0], a[1], a[2]), to: at(b[0], b[1], b[2]), prompt, ticks, arc, one_way };
        let mut routes = vec![
            r([-228., 0., 197.], [-263., 195., 236.], "CLIMB THE LOBBY LADDER", 55., 0., false),
            r([-284., 195., 259.], [-340., 400., 311.], "CLIMB TO THE UPPER WALKWAY", 65., 0., false),
            r([-1020., 400., 1020.], [-1188., 390., 1190.], "JUMP TO THE PLATFORM", 27., 225., true),
            r([-1188., 390., 1190.], [-1322., 480., 1322.], "JUMP TO THE CHEST LEDGE", 28., 260., true),
            r([-1280., 280., 1280.], [-1322., 480., 1322.], "CLIMB THE VINES", 54., 0., false),
            r([-1068., 280., 1068.], [-1020., 400., 1020.], "CLIMB THE FALLEN LADDER", 43., 0., false),
            r([-340., 400., 311.], [-20., 400., 20.], "LEAP TOWARD THE WEB", 35., 160., true),
            r([-432., 400., 432.], [-480., 400., 480.], "PASS THROUGH THE DOOR", 16., 0., false),
            r([-910., 400., 910.], [-962., 400., 962.], "PASS THROUGH THE DOOR", 16., 0., false),
            r([-20., -913., 20.], [-55., -880., 240.], "SWIM TO THE BANK", 65., 12., true),
            r([-565., -880., 990.], [-835., -915., 1245.], "SWIM TO THE SWITCH", 76., 0., true),
            r([-75., -880., 545.], [-75., -880., 615.], "PASS THROUGH THE DOOR", 16., 0., false),
            r([-300., -880., 960.], [-372., -880., 960.], "ENTER THE WATER ROOM", 16., 0., false),
            r([-1380., -880., 1110.], [-1480., -760., 1070.], "CLIMB THE PUSH BLOCK", 28., 90., false),
            r([-1500., -760., 1070.], [-1570., -760., 1070.], "PASS THROUGH THE DOOR", 16., 0., false),
            r([-1855., -760., 810.], [-1855., -760., 720.], "PASS THROUGH THE DOOR", 16., 0., false),
            r([-1370., -820., 3.], [-850., -820., 0.], "CRAWL TO THE BASEMENT", 55., 0., false),
            r([-635., -1910., -40.], [-662., -1880., -710.], "SWIM TO THE SCRUBS", 75., 5., true),
            r([-55., -880., 240.], [-470., -790., 365.], "CLIMB THE SHORTCUT", 52., 80., true),
        ];
        routes.reverse();
        let normal_water = world.water_boxes.get(1).map_or(0.0, |w| w.5);
        Some(Self {
            parts,
            routes,
            climb: None,
            flame_ticks: 0,
            push_ticks: 0,
            brothers_solved: 0,
            shooter: String::new(),
            flame_visible: false,
            flame_pos: [0.0; 3],
            torch_pos: at(-465., -731., 430.),
            peak: f64::NEG_INFINITY,
            ladder_drop: 0.0,
            clock: 0,
            water_timer: 0,
            water_drop: 0.0,
            normal_water,
            water_level: normal_water,
            ridden: false,
            nut_age: 0,
            reflected: false,
            nut_visible: false,
            nut_pos: [0.0; 3],
            nut_vel: [0.0; 3],
            actor_pos: [0.0; 3],
            prompt: None,
            toasts: vec![],
            origin,
        })
    }

    fn part_from(o: &CourseObject, scene: &ZeldaScene) -> Part {
        let extra = o.collision.as_ref().and_then(|k| scene.extra_collision.get(k));
        Part {
            model: o.model.clone(),
            home: o.pos,
            pos: o.pos,
            yaw: o.yaw,
            scale: o.actor_scale.unwrap_or(1.0),
            visible: true,
            rot_x: 0.0,
            collider: extra.map(|d| Collider::new(d.clone(), o.yaw)),
        }
    }

    fn part(&self, id: &str) -> &Part {
        &self.parts[id]
    }

    fn at(&self, x: f64, y: f64, z: f64) -> Vec3 {
        add([x * SCALE, y * SCALE, z * SCALE], self.origin)
    }

    fn mv(&mut self, id: &str, pos: Vec3) {
        if let Some(p) = self.parts.get_mut(id) {
            p.pos = pos;
        }
    }

    fn set_visible(&mut self, id: &str, visible: bool) {
        if let Some(p) = self.parts.get_mut(id) {
            p.visible = visible;
        }
    }

    /// Push the parts' poses (and the water level) into the sim's collision world. Copy-on-write:
    /// the world is only cloned when something actually changed.
    pub fn sync(&mut self, m: &mut MarioRuntime) {
        let dirty = self.parts.values().any(|p| p.collider.as_ref().is_some_and(|c| c.applied != Some(p.visible.then_some(p.pos))));
        let water = m.world.water_boxes.get(1).is_some_and(|w| w.5 != self.water_level);
        if !dirty && !water {
            return;
        }
        let world = Arc::make_mut(&mut m.world);
        for p in self.parts.values_mut() {
            let want = p.visible.then_some(p.pos);
            if let Some(c) = &mut p.collider {
                if c.applied != Some(want) {
                    c.move_to(world, want);
                }
            }
        }
        if let Some(w) = world.water_boxes.get_mut(1) {
            w.5 = self.water_level;
        }
    }

    fn nearby(&self, p: Vec3, r: f64, height: f64) -> bool {
        (p[0] - self.actor_pos[0]).hypot(p[2] - self.actor_pos[2]) < r && (p[1] - self.actor_pos[1]).abs() < height
    }

    fn toast(&mut self, s: &str) {
        self.toasts.push(s.to_string());
    }

    pub fn restore(&mut self, m: &mut MarioRuntime, save: &ZeldaSave) {
        self.climb = None;
        self.flame_ticks = 0;
        self.push_ticks = 0;
        self.brothers_solved = 0;
        self.shooter.clear();
        self.flame_visible = false;
        let block_home = self.part("block").home;
        let block_to = if save.deku_cleared("block") { self.at(-1400., -880., 1110.) } else { block_home };
        self.mv("block", block_to);
        self.set_visible("lowerWeb", !save.deku_cleared("burned"));
        self.set_visible("bossDoor", !save.deku_cleared("brothers"));
        let stick_home = self.part("stick").home;
        self.mv("stick", stick_home);
        for id in ["brother1", "brother2", "brother3"] {
            self.set_visible(id, !save.deku_cleared("brothers"));
        }
        self.peak = f64::NEG_INFINITY;
        self.clock = 0;
        self.water_timer = 0;
        self.water_drop = 0.0;
        self.nut_age = 0;
        self.nut_visible = false;
        self.ridden = false;
        self.prompt = None;
        self.ladder_drop = if save.deku_cleared("ladder") { 280.0 } else { 0.0 };
        let lh = self.part("ladder").home;
        self.mv("ladder", add(lh, [0.0, -self.ladder_drop * SCALE, 0.0]));
        self.set_visible("web", !save.deku_cleared("web"));
        self.set_visible("steppingStone", !save.has_slingshot());
        self.set_visible("scrub", !save.deku_cleared("scrub"));
        self.set_visible("gate", !save.deku_cleared("eye"));
        self.set_visible("eye", !save.deku_cleared("eye"));
        self.water_level = self.normal_water;
        let fh = self.part("ferry").home;
        self.mv("ferry", fh);
        self.sync(m);
    }

    /// Seed targets. Ids: 0 = ladder, 1 = golden eye.
    pub fn targets(&self, save: &ZeldaSave) -> Vec<SeedTarget> {
        let mut t = vec![];
        if !save.deku_cleared("ladder") {
            t.push(SeedTarget { center: add(self.part("ladder").home, [0.0, 60.0 * SCALE, 0.0]), radius: 185.0, id: 0 });
        }
        if !save.deku_cleared("eye") {
            t.push(SeedTarget { center: self.part("eye").home, radius: 80.0, id: 1 });
        }
        t
    }

    pub fn seed_hit(&mut self, id: usize, save: &mut ZeldaSave) {
        match id {
            0 => {
                save.clear_deku_challenge("ladder");
                self.toast("The ladder is coming down!");
            }
            1 => {
                if !save.deku_cleared("scrub") {
                    self.toast("The Deku Scrub is guarding this door.");
                    return;
                }
                save.clear_deku_challenge("eye");
                self.set_visible("eye", false);
                self.set_visible("gate", false);
                self.toast("The eye switch opened the water room.");
            }
            _ => {}
        }
    }

    fn scrub(&mut self, m: &mut MarioRuntime, c: &ControllerSnapshot, save: &mut ZeldaSave) {
        let trio = m.pos[1] < -7600.0;
        let ids: Vec<&'static str> = if trio && save.deku_cleared("burned") && !save.deku_cleared("brothers") {
            vec!["brother1", "brother2", "brother3"]
        } else if !save.deku_cleared("scrub") {
            vec!["scrub"]
        } else {
            vec![]
        };
        let actor = self.actor_pos;
        let mut choices: Vec<&'static str> = ids.iter().copied().filter(|id| self.part(id).visible).collect();
        choices.sort_by(|a, b| {
            let da = dist(self.part(a).home, actor).powi(2);
            let db = dist(self.part(b).home, actor).powi(2);
            da.partial_cmp(&db).unwrap()
        });
        let id: String = if self.nut_visible { self.shooter.clone() } else { choices.first().map(|s| s.to_string()).unwrap_or_default() };
        if id.is_empty() || !choices.contains(&id.as_str()) {
            self.nut_visible = false;
            return;
        }
        self.shooter = id.clone();
        let mut origin = self.part(&id).home;
        origin[1] += 85.0;
        let mut player = self.actor_pos;
        player[1] += 80.0;
        if !self.nearby(origin, 1500.0, 330.0) {
            self.nut_visible = false;
            self.nut_age = 0;
            return;
        }
        self.prompt = Some(if trio { "2-3-1 - HOLD K / Z TO RETURN THE NUT" } else { "HOLD K / Z TO DEFLECT THE DEKU NUT" }.into());
        if !self.nut_visible {
            self.nut_age += 1;
            if self.nut_age < 42 {
                return;
            }
            self.nut_pos = origin;
            self.nut_vel = scaled_to(sub(player, origin), 28.0);
            self.nut_visible = true;
            self.reflected = false;
            self.nut_age = 0;
        }
        self.nut_pos = add(self.nut_pos, self.nut_vel);
        if !self.reflected && dist(self.nut_pos, player) < 105.0 {
            if c.button_down & Z_TRIG != 0 {
                self.reflected = true;
                self.nut_vel = scaled_to(sub(origin, self.nut_pos), 42.0);
            } else {
                m.take_damage(1.0, Some(origin));
                self.nut_visible = false;
            }
        }
        if self.reflected && dist(self.nut_pos, origin) < 100.0 {
            if trio {
                self.nut_visible = false;
                self.nut_age = 0;
                let order = ["brother1", "brother2", "brother3"];
                if id != order[self.brothers_solved] {
                    self.brothers_solved = 0;
                    for b in ids {
                        self.set_visible(b, true);
                    }
                    self.toast("They surfaced again. Middle, right, left: 2-3-1.");
                } else {
                    self.set_visible(&id, false);
                    self.brothers_solved += 1;
                    if self.brothers_solved == 3 {
                        save.clear_deku_challenge("brothers");
                        self.set_visible("bossDoor", false);
                        self.toast("The brothers yield. The boss door is open.");
                    } else {
                        self.toast("That Scrub yielded!");
                    }
                }
                return;
            }
            save.clear_deku_challenge("scrub");
            self.set_visible("scrub", false);
            self.nut_visible = false;
            self.toast("The Scrub yields. Shoot the golden eye above the door.");
        }
        self.nut_age += 1;
        if self.nut_age > 110 {
            self.nut_visible = false;
            self.nut_age = 0;
        }
    }

    fn water(&mut self, m: &mut MarioRuntime, pressed_a: bool, save: &mut ZeldaSave) {
        let (ferry_home, ferry_pos) = (self.part("ferry").home, self.part("ferry").pos);
        let log_home = self.part("log").home;
        let aboard = self.part("ferry").collider.as_ref().is_some_and(|c| c.owns(&m.floor)) && (m.pos[1] - m.floor_height).abs() < 35.0;
        if self.water_timer > 0 {
            self.water_timer -= 1;
        }
        self.water_drop = (self.water_drop + if self.water_timer > 0 { 1.5 } else { -1.5 }).clamp(0.0, 47.0);
        let water = self.normal_water - self.water_drop * SCALE;
        self.water_level = water;
        let mut next = ferry_home;
        next[0] += (self.clock as f64 * std::f64::consts::PI / 192.0).sin() * 165.0 * SCALE;
        next[1] = water + 20.0 * SCALE;
        if aboard {
            m.pos[0] += next[0] - ferry_pos[0];
            m.pos[1] += next[1] - ferry_pos[1];
            if self.water_drop > 30.0 && next[0] < log_home[0] - 150.0 {
                self.ridden = true;
            }
        }
        self.mv("ferry", next);
        self.parts.get_mut("log").unwrap().rot_x = self.clock as f64 * 0.056;
        let actor = self.actor_pos;
        let in_path = (actor[2] - log_home[2]).abs() < 340.0 && (actor[0] - log_home[0]).abs() < 145.0;
        if in_path && actor[1] > log_home[1] - 170.0 && actor[1] < log_home[1] + 200.0 {
            m.take_damage(1.0, Some(log_home));
            m.pos = self.at(-575., -880., 1020.);
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.set_action(ACT_FREEFALL, 0.0);
            self.toast("Lower the water to clear the spiked log.");
        }
        let floor_switch = self.part("switch").home;
        if self.water_timer > 0 && self.nearby(floor_switch, 270.0, 770.0) {
            self.prompt = Some("SPACE / A - SWIM TO THE PLATFORM".into());
            if pressed_a && self.climb.is_none() {
                let mut landing = ferry_home;
                landing[0] += ((self.clock as f64 + 44.0) * std::f64::consts::PI / 192.0).sin() * 165.0 * SCALE;
                landing[1] = self.normal_water - 27.0 * SCALE + 12.0;
                self.climb = Some(Traverse { from: self.actor_pos, to: landing, ticks: 45.0, elapsed: 0.0, arc: 0.0, done: Done::None });
            }
        } else if aboard && self.ridden && next[0] < ferry_home[0] - 120.0 * SCALE {
            self.prompt = Some("SPACE / A - JUMP TO THE FAR BANK".into());
            if pressed_a && self.climb.is_none() {
                self.climb = Some(Traverse { from: self.actor_pos, to: self.at(-1150., -880., 1050.), ticks: 32.0, elapsed: 0.0, arc: 230.0, done: Done::None });
                m.set_action(ACT_JUMP, 0.0);
            }
        } else if self.nearby(floor_switch, 270.0, 770.0) {
            self.prompt = Some("SPACE / A - DIVE TO THE WATER SWITCH".into());
            if pressed_a {
                self.climb = Some(Traverse { from: self.actor_pos, to: floor_switch, ticks: 27.0, elapsed: 0.0, arc: 0.0, done: Done::DrainWater });
            }
        } else if aboard {
            self.prompt = Some(if self.water_timer > 0 { "RIDE UNDER THE LOG - STEP OFF AT THE FAR BANK" } else { "DIVE TO THE BLUE SWITCH TO LOWER THE WATER" }.into());
        }
        if self.ridden && actor[0] < self.at(-1130., 0., 0.)[0] && (actor[2] - log_home[2]).abs() < 750.0 && actor[1] > self.at(0., -910., 0.)[1] && !save.deku_cleared("crossed") {
            save.clear_deku_challenge("crossed");
            self.toast("Water room crossed!");
        }
    }

    /// One 30 Hz step (before the player's). `can_interact` is false while aiming/talking.
    pub fn step(&mut self, m: &mut MarioRuntime, c: &ControllerSnapshot, save: &mut ZeldaSave, can_interact: bool) -> DungeonStep {
        let mut out = DungeonStep::default();
        self.clock += 1;
        self.prompt = None;
        self.actor_pos = m.pos;
        if save.deku_cleared("ladder") && self.ladder_drop < 280.0 {
            self.ladder_drop = (self.ladder_drop + 8.0).min(280.0);
            let p = add(self.part("ladder").home, [0.0, -self.ladder_drop * SCALE, 0.0]);
            self.mv("ladder", p);
        }
        if save.has_slingshot() && self.part("steppingStone").visible {
            self.set_visible("steppingStone", false);
        }
        let on_ground = m.floor.is_some() && (m.floor_height - m.pos[1]).abs() < 25.0;
        let web_ok = {
            let web = self.part("web");
            web.visible && web.collider.as_ref().is_some_and(|c| c.owns(&m.floor))
        };
        if web_ok && on_ground && self.peak - m.pos[1] > 260.0 * SCALE {
            save.clear_deku_challenge("web");
            self.set_visible("web", false);
            m.set_action(ACT_FREEFALL, 0.0);
            m.vel[1] = -42.0;
            self.toast("The web broke!");
        }
        self.peak = if on_ground { m.pos[1] } else { self.peak.max(m.pos[1]) };
        self.scrub(m, c, save);
        self.water(m, can_interact && c.button_pressed & A_BUTTON != 0, save);
        let result = self.interact(m, c, save, can_interact, &mut out);
        self.sync(m);
        out.handled = result;
        out
    }

    fn interact(&mut self, m: &mut MarioRuntime, c: &ControllerSnapshot, save: &mut ZeldaSave, can_interact: bool, out: &mut DungeonStep) -> bool {
        let pressed = c.button_pressed & A_BUTTON != 0;
        if let Some(mv) = self.climb.as_mut() {
            mv.elapsed += 1.0;
            let t = mv.elapsed / mv.ticks;
            let mut p = lerp(mv.from, mv.to, t);
            p[1] += 4.0 * t * (1.0 - t) * mv.arc;
            m.pos = p;
            m.vel = [0.0; 3];
            m.forward_vel = 0.0;
            m.anim_frame += 1.0;
            if t >= 1.0 {
                let done = mv.done;
                self.climb = None;
                m.set_action(ACT_FREEFALL, 0.0);
                if done == Done::DrainWater {
                    self.water_timer = 480;
                    self.toast("Water draining. Board the floating platform.");
                    m.set_action(ACT_WATER_IDLE, 0.0);
                }
            }
            return true;
        }
        if !can_interact {
            return false;
        }
        if save.deku_cleared("brothers") && self.nearby(self.part("bossDoor").home, 475.0, 260.0) {
            self.prompt = Some("SPACE / A - ENTER GOHMA'S LAIR".into());
            if pressed {
                out.enter_boss = true;
                return true;
            }
        }
        let block_home = self.part("block").home;
        if self.push_ticks > 0 || (!save.deku_cleared("block") && self.nearby(block_home, 420.0, 160.0)) {
            self.prompt = Some("SPACE / A - PUSH THE BLOCK".into());
            if self.push_ticks > 0 || pressed {
                self.push_ticks += 1;
                let fraction = self.push_ticks as f64 / 40.0;
                let to = lerp(block_home, self.at(-1400., -880., 1110.), fraction);
                self.mv("block", to);
                if self.push_ticks == 40 {
                    self.push_ticks = 0;
                    save.clear_deku_challenge("block");
                    self.toast("Climb the block to reach the door.");
                }
                return true;
            }
        }
        let stick_home = self.part("stick").home;
        if self.flame_ticks > 0 {
            let mut sp = m.pos;
            sp[1] += 96.0;
            sp[0] += 45.0;
            self.mv("stick", sp);
            self.flame_pos = [sp[0], sp[1] + 48.0, sp[2]];
            self.prompt = Some(format!("DEKU STICK - {}s", (self.flame_ticks as f64 / 30.0).ceil()));
            self.flame_ticks -= 1;
            if self.flame_ticks == 0 {
                self.mv("stick", stick_home);
                self.flame_visible = false;
            }
            if !save.deku_cleared("burned") && self.nearby(self.part("lowerWeb").home, 390.0, 220.0) {
                save.clear_deku_challenge("burned");
                self.set_visible("lowerWeb", false);
                self.flame_ticks = 0;
                self.flame_visible = false;
                self.mv("stick", stick_home);
                self.toast("The web burned away. Drop into the pool below.");
            }
        }
        if save.deku_cleared("block") && self.nearby(stick_home, 390.0, 360.0) {
            self.prompt = Some("SPACE / A - LIGHT A DEKU STICK".into());
            if pressed {
                self.flame_ticks = 420;
                self.flame_visible = true;
                return true;
            }
        }
        let supply = self.at(-1391., 480., 1391.);
        if save.has_slingshot() && save.deku_seeds() < 30 && self.nearby(supply, 250.0, 150.0) {
            self.prompt = Some("SPACE / A - REFILL DEKU SEEDS".into());
            if pressed {
                save.refill_seeds();
                self.toast("Deku Seeds - 30/30");
                return true;
            }
        }
        for i in 0..self.routes.len() {
            let (from, to, prompt, arc, ticks, one_way) = {
                let r = &self.routes[i];
                (r.from, r.to, r.prompt, r.arc, r.ticks, r.one_way)
            };
            if (prompt.contains("PUSH BLOCK") || prompt.contains("SHORTCUT")) && !save.deku_cleared("block") {
                continue;
            }
            if prompt.contains("SCRUBS") && !save.deku_cleared("burned") {
                continue;
            }
            if prompt.contains("WATER ROOM") && !save.deku_cleared("eye") {
                continue;
            }
            if prompt.contains("FALLEN") && self.ladder_drop < 280.0 {
                continue;
            }
            if prompt.contains("PLATFORM") && save.has_slingshot() {
                continue;
            }
            if prompt.contains("WEB") && save.deku_cleared("web") {
                continue;
            }
            let radius = if prompt.contains("SWIM TO THE SCRUBS") {
                1800.0
            } else if prompt.contains("SWIM") {
                425.0
            } else if prompt.contains("PUSH BLOCK") {
                320.0
            } else {
                195.0
            };
            let forward = self.nearby(from, radius, if prompt.contains("SWIM TO THE SCRUBS") { 400.0 } else { 120.0 });
            if !forward && (one_way || !self.nearby(to, 165.0, 100.0)) {
                continue;
            }
            if !forward && prompt.contains("UPPER WALKWAY") && !save.deku_cleared("web") {
                continue;
            }
            let dest = if forward { to } else { from };
            let label = if forward {
                prompt
            } else if prompt.contains("DOOR") || prompt.contains("WATER ROOM") {
                "RETURN THROUGH THE DOOR"
            } else {
                "CLIMB DOWN"
            };
            self.prompt = Some(format!("SPACE / A - {label}"));
            if pressed {
                self.climb = Some(Traverse { from: self.actor_pos, to: dest, ticks, elapsed: 0.0, arc, done: Done::None });
                m.set_action(if arc != 0.0 { ACT_JUMP } else { ACT_CLIMBING_POLE }, 0.0);
                m.face_angle[1] = ((dest[0] - m.pos[0]).atan2(dest[2] - m.pos[2]) * 32768.0 / std::f64::consts::PI).round();
                return true;
            }
            break;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn setup() -> (GameSim, DekuDungeon, ZeldaSave) {
        let mut sim = GameSim::load(&assets(), CourseRequest::new("deku_tree")).unwrap();
        let scene = ZeldaScene::load(&assets(), "deku_tree", 1).unwrap();
        let mut d = DekuDungeon::new(&scene, &sim.mario.world).unwrap();
        let save = ZeldaSave::default();
        d.restore(&mut sim.mario, &save);
        (sim, d, save)
    }

    fn press() -> ControllerSnapshot {
        ControllerSnapshot { button_pressed: A_BUTTON, button_down: A_BUTTON, ..Default::default() }
    }

    #[test]
    fn restore_places_colliders_and_moves_the_block() {
        let (mut sim, mut d, mut save) = setup();
        let n = sim.mario.world.surfaces().len();
        let before_shared = sim.course.world.surfaces().len();
        assert!(n > before_shared, "dungeon colliders were added to Mario's world only");
        // Pushing the block permanently moves its collision.
        save.clear_deku_challenge("block");
        d.restore(&mut sim.mario, &save);
        assert_eq!(sim.mario.world.surfaces().len(), n);
        let block = d.part("block");
        assert!(block.pos != block.home);
        assert!(block.collider.as_ref().unwrap().applied == Some(Some(block.pos)));
    }

    #[test]
    fn eye_and_ladder_targets_and_hits() {
        let (mut sim, mut d, mut save) = setup();
        assert_eq!(d.targets(&save).len(), 2);
        d.seed_hit(0, &mut save);
        assert!(save.deku_cleared("ladder"));
        let top = d.part("ladder").pos[1];
        for _ in 0..40 {
            d.step(&mut sim.mario, &ControllerSnapshot::default(), &mut save, true);
        }
        assert!(d.part("ladder").pos[1] < top - 1000.0, "ladder lowered");
        // The eye needs the scrub gone first.
        d.seed_hit(1, &mut save);
        assert!(!save.deku_cleared("eye"));
        save.clear_deku_challenge("scrub");
        d.seed_hit(1, &mut save);
        assert!(save.deku_cleared("eye") && !d.part("gate").visible);
        d.sync(&mut sim.mario);
        assert!(d.part("gate").collider.as_ref().unwrap().applied == Some(None));
    }

    #[test]
    fn lobby_ladder_route_climbs() {
        let (mut sim, mut d, mut save) = setup();
        let start = d.routes.last().unwrap().from;
        sim.mario.pos = start;
        let out = d.step(&mut sim.mario, &press(), &mut save, true);
        assert!(out.handled, "{:?}", d.prompt);
        for _ in 0..80 {
            d.step(&mut sim.mario, &ControllerSnapshot::default(), &mut save, true);
        }
        assert!(sim.mario.pos[1] > start[1] + 500.0);
    }

    #[test]
    fn scrub_nut_hurts_unless_deflected() {
        let (mut sim, mut d, mut save) = setup();
        let scrub = d.part("scrub").home;
        sim.mario.pos = [scrub[0] + 400.0, scrub[1], scrub[2]];
        let health = sim.mario.health;
        for _ in 0..200 {
            d.step(&mut sim.mario, &ControllerSnapshot::default(), &mut save, true);
        }
        assert!(sim.mario.health < health || sim.mario.hurt_counter > 0.0);
        let (mut sim, mut d, mut save) = setup();
        sim.mario.pos = [scrub[0] + 400.0, scrub[1], scrub[2]];
        let hold = ControllerSnapshot { button_down: Z_TRIG, ..Default::default() };
        for _ in 0..200 {
            d.step(&mut sim.mario, &hold, &mut save, true);
        }
        assert!(save.deku_cleared("scrub"));
    }
}
