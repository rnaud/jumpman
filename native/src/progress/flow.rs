//! The game flow around the sim (`Game.ts` glue): warps, painting/door entrances, act selection,
//! star collection and the star dance, the course result, death/fall warps, game over, fading
//! warps and the wing switch. Engine-free: [`Progress`] is driven by `pre_tick`/`post_tick`
//! around each `GameSim::tick`, and asks for a course change through [`Progress::pending`]; the
//! Bevy plugin (`mod.rs`) performs the switch and draws the overlays.
use super::data::{ProgressData, V3};
use super::doors::{begin_entrance, probe_entrance, DoorCollider, EntranceHit, RoomDoors};
use super::menus::{ActSelector, BonusResult, BonusSave, CompletedCourse, CourseResult, SaveChoice};
use super::paintings::{painting_destination, PaintingAnim, PaintingArt};
use super::save::{CastleKey, SaveFile};
use super::stars::{Collected, StarDirector, HUNDRED_COIN_ACT};
use super::wing::WingSwitch;
use super::wipe::WipeKind;
use crate::game::courses::{self, SCORE_COURSES};
use crate::game::runner::{GameSim, SimEvent};
use crate::game::world::CourseRequest;
use jumpman_sim::math::to_s16;
use jumpman_sim::object::{mario_hitbox, INTERACT_BREAKABLE};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

/// What survives a course change besides lives.
#[derive(Clone, Debug, Default)]
pub struct RunCarry {
    pub health: f64,
    pub hurt_counter: f64,
    pub heal_counter: f64,
    pub coins: u32,
    pub red_coins: u32,
}

/// A queued course change.
#[derive(Debug)]
pub struct Switch {
    pub request: CourseRequest,
    /// Fall warps inside a course keep the run's health and coins.
    pub carry: Option<RunCarry>,
    /// Play the opening star wipe after loading.
    pub reveal: bool,
    /// A completed course whose result screen shows on arrival.
    pub completed: Option<CompletedCourse>,
    /// New run (game over / save-and-quit): lives back to 4 and unsaved progress dropped.
    pub fresh_run: bool,
}

/// The star dance after grabbing a star (`celebration`).
#[derive(Debug)]
pub struct Dance {
    /// Bonus stars (100 coins, no-exit): Mario stays in the course.
    pub bonus: bool,
    pub hundred: bool,
    pub yaw: f64,
    pub diameter: f64,
    pub model_visible: bool,
    pub model_pos: V3,
    pub model_scale: f64,
    pub model_rot: f64,
    pub prompt: Option<BonusSave>,
    pub frame: i32,
    /// Where the star model was grabbed (the model spawns at Mario).
    pub saved: bool,
}

#[derive(Debug)]
pub enum Flow {
    Playing,
    ActSelect(ActSelector),
    /// Closing screen wipe; at the end `then` runs.
    Closing { kind: WipeKind, frame: u32, then: Then },
    Dance(Dance),
    /// Waiting for Mario to land so the result can be shown.
    AwaitResult(CompletedCourse),
    Result(CourseResult),
    /// Castle entrance door cutscene.
    Entering { door: usize, locked: bool, frame: u32 },
    GameOver { timer: u32 },
    /// The credits finished (`won` overlay): A returns to the castle grounds.
    Credits { timer: u32 },
}

#[derive(Debug)]
pub enum Then {
    Load(Switch),
    GameOver,
}

/// Shared with the sim's `request_death_warp` closure.
#[derive(Default)]
struct DeathHook {
    requested: Mutex<Option<bool>>,
    /// bit 0: death warp exists, bit 1: fall warp exists.
    targets: AtomicU8,
}

#[derive(bevy::prelude::Resource)]
pub struct Progress {
    pub save: SaveFile,
    pub data: ProgressData,
    pub director: StarDirector,
    pub doors: Option<RoomDoors>,
    /// The closed castle entrance doors' colliders (`sm64World.ts`) and the two Dire Dire Docks warp
    /// walls (front: before the star, back: after it).
    pub entrance: Vec<DoorCollider>,
    /// This tick a `pre_tick` cutscene froze the world (see `freeze_world`).
    pub world_frozen: bool,
    pub ddd_warp: Option<[DoorCollider; 2]>,
    pub paintings: Option<PaintingAnim>,
    pub painting_art: PaintingArt,
    painting_table: HashMap<i32, String>,
    pub wing: Option<WingSwitch>,
    pub flow: Flow,
    /// Opening star wipe frame (0..16) after a course change.
    pub reveal: Option<u32>,
    pub pending: Option<Switch>,
    /// Opacity of the fading-warp overlay (0..1).
    pub fade: f32,
    fading: Option<usize>,
    just_teleported: bool,
    death: Arc<DeathHook>,
    prev_coins: u32,
    prev_red: u32,
    /// HUD messages: (text, seconds).
    pub toasts: Vec<(String, f32)>,
    pub lives_awarded: bool,
    floor_warp_timer: u32,
    pending_result: Option<CompletedCourse>,
    /// Sound/effect cues for the audio layer: names like "star", "coin", "menu_move".
    pub cues: Vec<&'static str>,
    /// Typed sound requests (forwarded to `audio::SfxCue`).
    pub sfx: Vec<crate::audio::SfxCue>,
    /// Ending sequence bookkeeping (see `progress/ending.rs`).
    pub ending: EndingState,
    /// Tower warp (`castle_inside` look-up floor): fade frames and the first-person look angles
    /// (s16), plus this tick's look input.
    pub tower_fade: u32,
    pub look_pitch: f64,
    pub look_yaw: f64,
    pub look_input: (bool, f32),
    pub menu_click: Option<MenuClick>,
    /// The other half of a linked-area course (ddd/ccm): its sim parts and star state, kept while
    /// the player is in the other area so both keep their objects and coins.
    pub linked: Option<Linked>,
    /// Queued area change for the Bevy glue (`apply_area_switch`).
    pub pending_area: Option<AreaChange>,
    initial_area: i32,
}

/// A mouse click on a progression menu (set by the UI, consumed in `post_tick`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuClick {
    /// An act star in the course selector.
    Act(i32),
    /// A save option of the course result.
    Result(usize),
    /// A bonus-star prompt button (Yes / No).
    Bonus(usize),
    /// Anywhere else: the A button (next page, game over, credits).
    Confirm,
}

/// The cached other area of a linked-area course.
pub struct Linked {
    pub area: i32,
    sim: Box<GameSim>,
    director: StarDirector,
}

/// A same-course area switch (`changeLinkedArea`).
#[derive(Debug, Clone)]
pub struct AreaChange {
    pub area: i32,
    /// Mario's displacement (instant warps shift him by the areas' offset).
    pub delta: [f64; 3],
    /// Arrive at this warp node / door node (and place Mario there) instead of staying put.
    pub entry: Option<String>,
    pub push: bool,
    /// Place Mario at the area's course spawn (restart).
    pub to_spawn: bool,
}

/// Progress through the Grand Star / Peach ending, driven by Mario's action and sim events.
#[derive(Default, Clone, Debug)]
pub struct EndingState {
    /// The credits are rolling (the web build's `won` overlay).
    pub won: bool,
    /// Canvas opacity override (None = untouched), 0..1.
    pub opacity: Option<f32>,
}

fn is_score(course: &str) -> bool {
    SCORE_COURSES.contains(&course)
}

impl Progress {
    pub fn new(sim: &mut GameSim, root: &Path, save: SaveFile) -> Self {
        let req = sim.course.request.clone();
        let data = ProgressData::load(root, &req.id, req.area);
        let death = Arc::new(DeathHook::default());
        let director = StarDirector::new(&req.id, req.act, &data, sim.course.spawn.pos, &sim.mario.world, &sim.mario, &save);
        let mut p = Self {
            save,
            data,
            director,
            doors: None,
            entrance: vec![],
            world_frozen: false,
            ddd_warp: None,
            paintings: None,
            painting_art: PaintingArt::default(),
            painting_table: HashMap::new(),
            wing: None,
            flow: Flow::Playing,
            reveal: None,
            pending: None,
            fade: 0.0,
            fading: None,
            just_teleported: false,
            death,
            prev_coins: 0,
            prev_red: 0,
            toasts: vec![],
            lives_awarded: false,
            floor_warp_timer: 0,
            pending_result: None,
            cues: vec![],
            sfx: vec![],
            ending: EndingState::default(),
            tower_fade: 0,
            look_pitch: 0.0,
            look_yaw: 0.0,
            look_input: (false, 0.0),
            menu_click: None,
            linked: None,
            pending_area: None,
            initial_area: sim.course.request.area,
        };
        p.attach(sim, root);
        p
    }

    /// (Re)build everything that depends on the loaded course.
    fn attach(&mut self, sim: &mut GameSim, root: &Path) {
        let _ = self.attach_with(sim, root, None);
    }

    /// `attach` that reuses a cached star director (returning to a linked area) and hands back the
    /// replaced one.
    fn attach_with(&mut self, sim: &mut GameSim, root: &Path, cached: Option<StarDirector>) -> StarDirector {
        let req = sim.course.request.clone();
        self.data = ProgressData::load(root, &req.id, req.area);
        let director = cached.unwrap_or_else(|| StarDirector::new(&req.id, req.act, &self.data, sim.course.spawn.pos, &sim.mario.world, &sim.mario, &self.save));
        let old = std::mem::replace(&mut self.director, director);
        self.painting_table = self.data.painting_table();
        // Painting art (shapes + layouts) lives next to the paintings table in the course json.
        self.painting_art = std::fs::read(crate::game::course_data::CourseData::path(root, &req.id, req.area))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| serde_json::from_value::<PaintingArt>(v.get("paintingArt")?.clone()).ok())
            .unwrap_or_default();
        self.paintings = (!self.painting_art.layouts.is_empty()).then(|| PaintingAnim::new(self.painting_art.layouts.clone(), &self.save));
        let world = Arc::make_mut(&mut sim.mario.world);
        self.doors = match (&self.data.door_motion, self.data.extra_collision.get("door"), self.data.extra_collision.get("star_door")) {
            (Some(motion), Some(door), Some(star)) if !self.data.room_doors.is_empty() => {
                Some(RoomDoors::new(&self.data.room_doors, motion.clone(), door, star, world))
            }
            _ => None,
        };
        self.entrance.clear();
        if let Some(shape) = self.data.extra_collision.get("door") {
            for d in &self.data.doors {
                // A door that is also a star/key room door shares its collider with `RoomDoors`.
                if self.data.room_doors.iter().any(|r| r.pos == d.pos) {
                    continue;
                }
                let mut c = DoorCollider::new(shape.clone(), d.yaw);
                c.move_to(world, Some(d.pos));
                self.entrance.push(c);
            }
        }
        self.ddd_warp = match (self.data.extra_collision.get("ddd_warp"), self.data.extra_collision.get("ddd_warp_2")) {
            (Some(front), Some(back)) => Some([DoorCollider::new(front.clone(), 0.0), DoorCollider::new(back.clone(), 0.0)]),
            _ => None,
        };
        Self::apply_save_geometry(&mut self.ddd_warp, world, &self.save);
        self.wing = self.data.wing_switch.map(|pos| WingSwitch::new(pos, &self.data.extra_collision, world, &self.save));
        // The DDD warp wall flips once the Dire Dire Docks star is owned.
        self.set_wing_boxes(sim);
        self.apply_snowman(sim);
        self.flow = Flow::Playing;
        self.fading = None;
        self.just_teleported = false;
        self.fade = 0.0;
        self.prev_coins = 0;
        self.prev_red = 0;
        self.floor_warp_timer = 0;
        self.install_death_hook(sim);
        old
    }

    /// `updateSaveGeometry`: the DDD warp wall flips once the Dire Dire Docks star is owned.
    fn apply_save_geometry(ddd: &mut Option<[DoorCollider; 2]>, world: &mut jumpman_sim::collision::CollisionWorld, save: &SaveFile) {
        if let Some([front, back]) = ddd {
            let moved = save.has_star("ddd", 1);
            front.move_to(world, (!moved).then_some([0.0; 3]));
            back.move_to(world, moved.then_some([0.0; 3]));
        }
    }

    /// Re-apply the save-dependent geometry (after a star was collected or the save changed).
    pub fn update_save_geometry(&mut self, sim: &mut GameSim) {
        let world = Arc::make_mut(&mut sim.mario.world);
        Self::apply_save_geometry(&mut self.ddd_warp, world, &self.save);
    }

    /// Switch to the other area of this course. Mario keeps his state; the course parts, objects
    /// and star state of each area are swapped with the cached other half (loaded on first use).
    pub fn change_area(&mut self, sim: &mut GameSim, root: &Path, ch: AreaChange) -> Result<(), String> {
        let current = sim.course.request.area;
        if ch.area == current {
            return Ok(());
        }
        let cached = match self.linked.take() {
            Some(l) if l.area == ch.area => Some(l),
            other => {
                self.linked = other;
                None
            }
        };
        let (mut other, cached_director) = match cached {
            Some(l) => (l.sim, Some(l.director)),
            None => {
                let mut request = CourseRequest::new(&sim.course.request.id);
                request.act = sim.course.request.act;
                request.area = ch.area;
                (Box::new(GameSim::load(root, request)?), None)
            }
        };
        sim.mario.drop_held_object();
        sim.mario.cannon = None;
        sim.mario.riding_shell = None;
        sim.mario.used_obj = None;
        sim.mario.interact_obj = None;
        sim.mario.grabbed_by = None;
        std::mem::swap(&mut sim.course, &mut other.course);
        std::mem::swap(&mut sim.pickups, &mut other.pickups);
        std::mem::swap(&mut sim.mario.objects, &mut other.mario.objects);
        sim.mario.change_area(sim.course.world.clone(), sim.course.spawn.clone(), sim.course.death_height, ch.delta);
        if ch.entry.is_some() || ch.to_spawn {
            let mut request = sim.course.request.clone();
            request.entry = ch.entry.clone();
            request.push = ch.push;
            if let Ok(data) = crate::game::course_data::CourseData::load(root, &request.id, request.area) {
                let spawn = crate::game::world::resolve_spawn(&data, &sim.course.world, &request);
                let m = &mut sim.mario;
                m.pos = spawn.pos;
                m.vel = [0.0; 3];
                m.forward_vel = 0.0;
                m.face_angle = [0.0, spawn.face_yaw, 0.0];
                m.peak_height = spawn.pos[1];
                if let Some(action) = spawn.action {
                    m.set_action(action, 0.0);
                }
                let spawn_point = spawn;
                m.change_area(sim.course.world.clone(), spawn_point, sim.course.death_height, [0.0; 3]);
            }
        }
        let old_director = self.attach_with(sim, root, cached_director);
        self.linked = Some(Linked { area: current, sim: other, director: old_director });
        sim.curr = sim.snapshot();
        sim.prev = sim.curr.clone();
        sim.stars = self.save.stars();
        Ok(())
    }

    /// `Game.restart` for the progression state (the sim itself is reset by the caller).
    pub fn restart(&mut self, sim: &mut GameSim) {
        if let Some(l) = &mut self.linked {
            l.sim.mario.objects.reset();
            for p in &mut l.sim.pickups {
                p.collected = false;
            }
            if sim.course.request.area != self.initial_area {
                self.pending_area = Some(AreaChange { area: self.initial_area, delta: [0.0; 3], entry: None, push: false, to_spawn: true });
            }
        }
        self.director.reset(&self.save);
        let world = Arc::make_mut(&mut sim.mario.world);
        if let Some(d) = &mut self.doors {
            d.reset(world);
        }
        if let Some(w) = &mut self.wing {
            w.reset(world, self.save.wing_switch_on());
        }
        if let Some(p) = &mut self.paintings {
            p.reset(&self.save);
        }
        Self::apply_save_geometry(&mut self.ddd_warp, world, &self.save);
        self.set_wing_boxes(sim);
        self.apply_snowman(sim);
        self.flow = Flow::Playing;
        self.reveal = None;
        self.fade = 0.0;
        self.fading = None;
        self.just_teleported = false;
        self.prev_coins = 0;
        self.prev_red = 0;
        self.floor_warp_timer = 0;
        self.pending_result = None;
        *self.death.requested.lock().unwrap() = None;
        self.toast("RUN RESET", 1.0);
    }

    fn install_death_hook(&mut self, sim: &mut GameSim) {
        let targets = u8::from(sim.course.death_warp.is_some()) | (u8::from(sim.course.fall_warp.is_some()) << 1);
        self.death.targets.store(targets, Ordering::Relaxed);
        *self.death.requested.lock().unwrap() = None;
        let hook = self.death.clone();
        sim.mario.request_death_warp = Some(Box::new(move |fall| {
            let t = hook.targets.load(Ordering::Relaxed);
            let handled = if fall { t & 2 != 0 || t & 1 != 0 } else { t & 1 != 0 };
            if handled {
                let mut slot = hook.requested.lock().unwrap();
                if slot.is_none() {
                    *slot = Some(fall);
                }
            }
            handled
        }));
    }

    /// `applySnowman`: show the finished snowman when the Snowman's Lost His Head star is owned.
    fn apply_snowman(&self, sim: &mut GameSim) {
        if let Some(star) = &self.data.snowman_star {
            if star.act != sim.course.request.act {
                let done = self.save.has_star(&sim.course.request.id, star.act);
                sim.mario.objects.set_snowman_completed(done);
            }
        }
    }

    fn set_wing_boxes(&self, sim: &mut GameSim) {
        let on = self.save.wing_switch_on();
        for o in sim.mario.objects.objects.iter_mut().filter(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::WingBox) {
            o.set_var("unlocked", if on { 1.0 } else { 0.0 });
            if o.action == 0 {
                o.hitbox.interact_type = if on { INTERACT_BREAKABLE } else { 0 };
            }
        }
    }

    fn toast(&mut self, text: impl Into<String>, seconds: f32) {
        self.toasts.push((text.into(), seconds));
    }

    pub fn course(&self, sim: &GameSim) -> String {
        sim.course.request.id.clone()
    }

    /// Whether the 30 Hz sim tick should run this step.
    pub fn sim_running(&self) -> bool {
        if self.world_frozen {
            return false;
        }
        match &self.flow {
            Flow::Playing | Flow::AwaitResult(_) | Flow::Entering { .. } => !self.director.cutscene_active(),
            Flow::Dance(d) => d.prompt.is_none() && !self.director.cutscene_active(),
            Flow::ActSelect(_) | Flow::Closing { .. } | Flow::Result(_) | Flow::GameOver { .. } | Flow::Credits { .. } => false,
        }
    }

    /// Menus read the controller themselves when the sim is paused.
    pub fn wants_input(&self) -> bool {
        matches!(&self.flow, Flow::ActSelect(_) | Flow::Result(_) | Flow::GameOver { .. } | Flow::Credits { .. }) || matches!(&self.flow, Flow::Dance(d) if d.prompt.is_some())
    }

    // ---------------------------------------------------------------- before Mario's step

    /// Door cutscenes and fading warps run before Mario's own step and can replace it.
    /// A cutscene driven from `pre_tick` (door passage, tower fade, castle entrance) owns Mario and,
    /// like the TS `stepWorld`'s early returns, freezes everything else: platforms, goombas and
    /// objects do not move, the sim tick does not run. Only the view (pose, snapshots) advances.
    fn freeze_world(&mut self, sim: &mut GameSim) {
        self.world_frozen = true;
        sim.refresh_view();
    }

    pub fn pre_tick(&mut self, sim: &mut GameSim) {
        self.world_frozen = false;
        if !matches!(self.flow, Flow::Playing | Flow::AwaitResult(_) | Flow::Entering { .. }) || self.director.cutscene_active() {
            return;
        }
        if let Flow::Entering { door, locked, frame } = &mut self.flow {
            // The entrance cutscene drives Mario's timer itself (Game.ts `enteringDoor`).
            let (door_idx, locked_now) = (*door, *locked);
            *frame += 1;
            sim.mario.action_timer += 1.0;
            let tick = sim.mario.action_timer as u32;
            let d = self.data.doors[door_idx].clone();
            self.freeze_world(sim);
            if sim.mario.action == ACT_UNLOCKING_KEY_DOOR {
                if tick >= 147 {
                    if let Some(k) = d.required_key.and_then(CastleKey::from_number) {
                        self.save.unlock_door(k);
                    }
                    let push = sim.mario.action_arg as i32 & 2 != 0;
                    sim.mario.set_action(if push { ACT_PUSHING_DOOR } else { ACT_PULLING_DOOR }, if push { 6.0 } else { 5.0 });
                    sim.mario.pos = d.pos;
                    self.flow = Flow::Entering { door: door_idx, locked: false, frame: 0 };
                }
                return;
            }
            let _ = locked_now;
            if tick == 36 {
                let push = sim.mario.action == ACT_PUSHING_DOOR;
                if d.destination == sim.course.request.id && d.dest_area != sim.course.request.area {
                    self.pending_area = Some(AreaChange { area: d.dest_area, delta: [0.0; 3], entry: Some(d.dest_node.clone()), push, to_spawn: false });
                    self.flow = Flow::Playing;
                    return;
                }
                let mut request = CourseRequest::new(&d.destination);
                request.entry = Some(d.dest_node.clone());
                request.push = push;
                request.area = d.dest_area;
                self.pending = Some(Switch { request, carry: None, reveal: false, completed: None, fresh_run: false });
            }
            return;
        }
        if self.step_tower(sim) {
            return;
        }
        if let Some(doors) = &mut self.doors {
            let mut notes = vec![];
            let consumed = doors.step(&mut sim.mario, &mut self.save, &mut |n| notes.push(n));
            for n in notes {
                self.toasts.push((n, 1.8));
            }
            if consumed {
                self.freeze_world(sim);
                return;
            }
        }
        self.step_fading_warps(sim);
    }

    /// Looking straight up backwards on the castle's tower floor (type 0x2f) with 10 stars fades
    /// out into the Tower of the Wing Cap. Freezes Mario while looking.
    fn step_tower(&mut self, sim: &mut GameSim) -> bool {
        let m = &mut sim.mario;
        let on_floor = sim.course.request.id == "castle_inside" && m.floor.as_ref().is_some_and(|f| f.kind == 0x2f) && m.action & ACT_FLAG_AIR == 0 && self.look_input.0;
        if self.tower_fade == 0 && !on_floor {
            self.look_pitch = 0.0;
            self.look_yaw = 0.0;
            return false;
        }
        self.look_pitch = (self.look_pitch + 0x100 as f64).min(0x1800 as f64);
        self.look_yaw = f64::from(to_s16(self.look_yaw + (f64::from(self.look_input.1) * 0x240 as f64).round()));
        let yaw = f64::from(to_s16(m.face_angle[1] + self.look_yaw));
        if self.tower_fade > 0 || (self.look_pitch == 0x1800 as f64 && !(-0x6fff as f64..0x7000 as f64).contains(&yaw) && self.save.stars() >= 10) {
            if self.tower_fade == 0 {
                self.cues.push("star_sound");
            }
            self.tower_fade += 1;
            self.fade = (self.tower_fade as f32 / 30.0).min(1.0);
            if self.tower_fade == 30 {
                let mut request = CourseRequest::new("totwc");
                request.entry = Some("WARP_NODE_0A".into());
                self.pending = Some(Switch { request, carry: None, reveal: false, completed: None, fresh_run: false });
            }
        }
        self.freeze_world(sim);
        true
    }

    /// `stepFadingWarp`.
    fn step_fading_warps(&mut self, sim: &mut GameSim) {
        let pads = sim.course.fading_warps.clone();
        if pads.is_empty() {
            return;
        }
        let m = &mut sim.mario;
        if m.action == ACT_TELEPORT_FADE_OUT {
            if let Some(i) = self.fading {
                let t = m.action_timer;
                self.fade = ((t - 20.0) / 20.0).clamp(0.0, 1.0) as f32;
                if t >= 40.0 {
                    let pad = &pads[i];
                    m.pos = pad.target;
                    m.vel = [0.0; 3];
                    m.forward_vel = 0.0;
                    m.face_angle = [0.0, pad.face_yaw, 0.0];
                    m.peak_height = pad.target[1];
                    m.set_action(ACT_TELEPORT_FADE_IN, 0.0);
                    self.fading = None;
                    self.cues.push("teleport");
                }
            }
            return;
        }
        if m.action == ACT_TELEPORT_FADE_IN {
            self.fade = (1.0 - m.action_timer / 20.0).clamp(0.0, 1.0) as f32;
            return;
        }
        self.fade = 0.0;
        let hb = mario_hitbox(m);
        let found = pads.iter().position(|p| (m.pos[0] - p.pos[0]).hypot(m.pos[2] - p.pos[2]) < 85.0 + hb.radius && m.pos[1] <= p.pos[1] + 50.0 && m.pos[1] + hb.height >= p.pos[1]);
        match found {
            None => self.just_teleported = false,
            Some(i) if !self.just_teleported && (m.action == ACT_IDLE || m.action == ACT_CROUCHING) => {
                self.just_teleported = true;
                self.fading = Some(i);
                self.cues.push("teleport");
                let crouching = m.action == ACT_CROUCHING;
                m.set_action(ACT_TELEPORT_FADE_OUT, if crouching { 1.0 } else { 0.0 });
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- after Mario's step

    /// `controller` is only meaningful when [`wants_input`](Self::wants_input).
    pub fn post_tick(&mut self, sim: &mut GameSim, controller: &ControllerSnapshot) {
        if let Some(r) = &mut self.reveal {
            *r += 1;
            if *r >= 16 {
                self.reveal = None;
            }
        }
        // Death requests come from inside Mario's step.
        let requested = self.death.requested.lock().unwrap().take();
        if let (Some(fall), true) = (requested, matches!(self.flow, Flow::Playing | Flow::AwaitResult(_))) {
            self.begin_death_warp(sim, fall);
        }
        self.step_ending_events(sim);
        let click = self.menu_click.take();
        if let (Some(MenuClick::Bonus(i)), Flow::Dance(d)) = (&click, &mut self.flow) {
            if let Some(p) = d.prompt.as_mut().filter(|p| p.has_choices()) {
                p.choice = *i;
            }
        }
        let a_press = ControllerSnapshot { button_pressed: A_BUTTON, button_down: A_BUTTON, ..Default::default() };
        let controller = if matches!(click, Some(MenuClick::Confirm | MenuClick::Bonus(_))) { &a_press } else { controller };
        match std::mem::replace(&mut self.flow, Flow::Playing) {
            Flow::Playing => self.step_playing(sim),
            Flow::Credits { timer } => {
                let timer = timer + 1;
                if timer > 20 && controller.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
                    let request = CourseRequest::new("castle_grounds");
                    self.pending = Some(Switch { request, carry: None, reveal: true, completed: None, fresh_run: false });
                    self.ending = EndingState::default();
                    self.flow = Flow::Playing;
                } else {
                    self.flow = Flow::Credits { timer };
                }
            }
            Flow::AwaitResult(done) => {
                let m = &mut sim.mario;
                if m.action == ACT_IDLE || (m.action == ACT_EXIT_LAND_SAVE_DIALOG && m.action_timer >= 14.0) {
                    m.action_state = if done.key { 1.0 } else { 3.0 };
                    self.flow = Flow::Result(CourseResult::new(done));
                } else {
                    self.flow = Flow::AwaitResult(done);
                    self.step_playing_world(sim);
                }
            }
            Flow::ActSelect(mut sel) => {
                let was = sel.selection;
                let act = sel.step(controller).or(if let Some(MenuClick::Act(a)) = click { sel.click(a) } else { None });
                if sel.moved && sel.selection != was {
                    self.cues.push("menu_move");
                }
                match act {
                    Some(act) => {
                        self.cues.push("select_mission");
                        let mut request = CourseRequest::new(&sel.course);
                        request.act = act;
                        request.entry = sel.entry.clone();
                        self.pending = Some(Switch { request, carry: None, reveal: false, completed: None, fresh_run: false });
                        self.flow = Flow::Playing;
                    }
                    None => self.flow = Flow::ActSelect(sel),
                }
            }
            Flow::Closing { kind, frame, then } => {
                let duration = kind.duration();
                if frame + 1 < duration + 1 {
                    self.flow = Flow::Closing { kind, frame: frame + 1, then };
                } else {
                    match then {
                        Then::Load(switch) => {
                            self.pending = Some(switch);
                            self.flow = Flow::Playing;
                        }
                        Then::GameOver => self.flow = Flow::GameOver { timer: 0 },
                    }
                }
            }
            Flow::Dance(d) => self.step_dance(sim, d, controller),
            Flow::Result(mut r) => {
                let out = r.step(controller);
                if let Some((count, record)) = out.counted {
                    self.sfx.push(crate::audio::SfxCue::CourseCoin { count, record });
                    if [50, 100, 150].contains(&count) {
                        sim.mario.lives = (sim.mario.lives + 1.0).min(100.0);
                    }
                }
                let clicked = match click {
                    Some(MenuClick::Result(i)) => {
                        r.selection = i.min(2);
                        r.confirm(i)
                    }
                    _ => None,
                };
                match out.choice.or(clicked) {
                    Some(choice) => self.finish_result(sim, choice),
                    None => self.flow = Flow::Result(r),
                }
            }
            Flow::Entering { door, locked, frame } => {
                self.flow = Flow::Entering { door, locked, frame };
            }
            Flow::GameOver { timer } => {
                let timer = timer + 1;
                if timer > 20 && controller.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
                    self.save.revert_course_save();
                    let mut request = CourseRequest::new("castle_grounds");
                    request.act = 1;
                    self.pending = Some(Switch { request, carry: None, reveal: true, completed: None, fresh_run: true });
                    self.flow = Flow::Playing;
                } else {
                    self.flow = Flow::GameOver { timer };
                }
            }
        }
        sim.stars = self.save.stars();
    }

    /// `castleEnding` loads the grounds for the Peach cutscene; `endingCredits` shows the end card.
    fn step_ending_events(&mut self, sim: &mut GameSim) {
        use jumpman_sim::state::MarioEvent;
        for e in &sim.events {
            if let SimEvent::Mario(ev) = e {
                match ev {
                    MarioEvent::CastleEnding if self.pending.is_none() => {
                        let mut request = CourseRequest::new("castle_grounds");
                        request.entry = Some("WARP_NODE_CREDITS_START".into());
                        self.pending = Some(Switch { request, carry: None, reveal: false, completed: None, fresh_run: false });
                    }
                    MarioEvent::EndingCredits => {
                        self.ending.won = true;
                        self.flow = Flow::Credits { timer: 0 };
                    }
                    _ => {}
                }
            }
        }
    }

    fn step_playing(&mut self, sim: &mut GameSim) {
        if sim.mario.action == ACT_JUMBO_STAR_CUTSCENE || sim.mario.action == ACT_END_PEACH_CUTSCENE {
            return;
        }
        // Entrance doors are checked before the world logic (Game.ts).
        if matches!(self.flow, Flow::Playing) {
            self.step_playing_world(sim);
        }
    }

    fn step_playing_world(&mut self, sim: &mut GameSim) {
        if !matches!(self.flow, Flow::Playing | Flow::AwaitResult(_)) {
            return;
        }
        let course = sim.course.request.id.clone();
        // Sim events.
        let events: Vec<SimEvent> = sim.events.clone();
        for e in &events {
            match e {
                SimEvent::Object(obj) => {
                    if let ObjectEvent::Warp { destination, entry, area } = obj {
                        self.begin_warp(sim, destination, entry.clone(), *area);
                    } else if let Some(msg) = self.director.on_event(obj) {
                        self.toast(format!("* {msg}"), 3.2);
                        self.cues.push("star_spawn");
                    }
                    match obj {
                        ObjectEvent::OneUp { .. } => self.toast("1-UP", 2.4),
                        ObjectEvent::Dialogue { text } => self.toast(text.clone(), 3.6),
                        ObjectEvent::Race { state } if *state == "won" || *state == "end" => self.director.race_won(),
                        _ => {}
                    }
                }
                SimEvent::CoinCollected { .. } => {}
                SimEvent::Mario(_) => {}
            }
        }
        // 100 coins; 8 red coins.
        if self.prev_coins < 100 && sim.coins >= 100 {
            self.director.hundred_coins(sim.mario.pos);
        }
        if sim.red_coins == 8 && self.prev_red != 8 && self.director.red_coins_done() {
            self.toast("* ALL 8 RED COINS - REACH THE STAR", 3.6);
        }
        if course == "mushroom2d" && sim.coins >= crate::game::mushroom2d::REQUIRED_COINS {
            self.director.coins_goal_reached();
        }
        // Playground: every meadow coin (`requiredCoins ?? coins.length`).
        if course == "playground" && !sim.course.coins.is_empty() && sim.coins as usize >= sim.course.coins.len() {
            self.director.coins_goal_reached();
        }
        self.prev_coins = sim.coins;
        self.prev_red = sim.red_coins;

        // Stars.
        let step = self.director.step(&sim.mario, &self.save, sim.coins);
        for t in step.toasts.into_iter().filter(|t| !t.is_empty()) {
            self.toast(t, 2.2);
        }
        if let Some(c) = step.collected {
            self.finish_reward(sim, c);
            return;
        }
        if step.freeze {
            return;
        }

        // Paintings and entrances.
        if course == "castle_inside" {
            if let Some(p) = &mut self.paintings {
                p.step(sim.mario.floor.as_ref().map_or(-1, |f| f.kind), sim.mario.pos, &mut self.save);
            }
            if sim.mario.action & ACT_FLAG_INTANGIBLE == 0 && matches!(self.flow, Flow::Playing) {
                let kind = sim.mario.floor.as_ref().map_or(-1, |f| f.kind);
                if let Some(dest) = painting_destination(kind, sim.mario.pos[1] - sim.mario.floor_height, &self.painting_table) {
                    self.enter_course(sim, &dest, Some("WARP_NODE_0A".into()), None);
                }
            }
        }
        if matches!(self.flow, Flow::Playing) {
            self.step_entrances(sim);
        }
        if matches!(self.flow, Flow::Playing) {
            self.step_floor_warps(sim, &course);
        }
        if matches!(self.flow, Flow::Playing) {
            if let Some(w) = &mut self.wing {
                let (floor, y, fh) = (sim.mario.floor.clone(), sim.mario.pos[1], sim.mario.floor_height);
                let world = Arc::make_mut(&mut sim.mario.world);
                if w.step(&floor, y, fh, world) {
                    self.save.press_wing_switch();
                    self.set_wing_boxes(sim);
                    self.cues.push("cap_switch");
                    self.toast("Wing Cap boxes are now solid!", 3.0);
                }
            }
            if let Some(w) = self.data.instant_warps.get(&(sim.mario.floor.as_ref().map_or(-1, |f| f.kind) - 0x1b).to_string()) {
                if course != "castle_inside" || self.save.stars() < 70 {
                    if w.area == sim.course.request.area {
                        for axis in 0..3 {
                            sim.mario.pos[axis] += w.delta[axis];
                        }
                    } else if self.pending_area.is_none() {
                        self.pending_area = Some(AreaChange { area: w.area, delta: w.delta, entry: None, push: false, to_spawn: false });
                    }
                }
            }
        }
    }

    fn step_entrances(&mut self, sim: &mut GameSim) {
        if self.data.doors.is_empty() {
            return;
        }
        match probe_entrance(&to_entrance_doors(&self.data), &sim.mario, &self.save) {
            Some(EntranceHit::NeedsKey) => {
                self.toast("YOU NEED A KEY TO OPEN THIS DOOR", 1.8);
                sim.mario.forward_vel = 0.0;
                // Back off so the message does not repeat every tick.
                let prev = sim.prev.pos;
                sim.mario.pos[0] = prev[0];
                sim.mario.pos[2] = prev[2];
            }
            Some(EntranceHit::Enter { index, locked, pull }) => {
                let door = self.data.doors[index].clone();
                begin_entrance(&mut sim.mario, &door, locked, pull);
                self.flow = Flow::Entering { door: index, locked, frame: 0 };
            }
            None => {}
        }
    }

    fn step_floor_warps(&mut self, sim: &mut GameSim, course: &str) {
        let kind = sim.mario.floor.as_ref().map_or(-1, |f| f.kind);
        // jrb: the sunken-ship floor warp keeps health and coins.
        if let Some(fw) = self.data.floor_warp.clone() {
            if self.floor_warp_timer > 0 || kind == 0x32 {
                self.floor_warp_timer += 1;
                if self.floor_warp_timer == 1 {
                    let mut request = CourseRequest::new(course);
                    request.act = sim.course.request.act;
                    request.entry = Some(fw.entry.clone());
                    request.area = fw.area;
                    let carry = Some(self.carry(sim));
                    self.floor_warp_timer = 0;
                    self.flow = Flow::Closing { kind: WipeKind::Iris, frame: 0, then: Then::Load(Switch { request, carry, reveal: false, completed: None, fresh_run: false }) };
                }
                return;
            }
        }
        // totwc: falling through the floor returns to the castle.
        if course == "totwc" && (self.floor_warp_timer > 0 || (kind == 0x0a && sim.mario.pos[1] < sim.mario.floor_height + 2048.0)) {
            self.floor_warp_timer = 0;
            let mut request = CourseRequest::new("castle_inside");
            request.entry = Some("WARP_NODE_20".into());
            self.flow = Flow::Closing { kind: WipeKind::Iris, frame: 0, then: Then::Load(Switch { request, carry: None, reveal: false, completed: None, fresh_run: false }) };
        }
    }

    fn carry(&self, sim: &GameSim) -> RunCarry {
        let m = &sim.mario;
        RunCarry { health: m.health, hurt_counter: m.hurt_counter, heal_counter: m.heal_counter, coins: sim.coins, red_coins: sim.red_coins }
    }

    // ---------------------------------------------------------------- warps

    fn begin_warp(&mut self, sim: &GameSim, destination: &str, entry: Option<String>, area: Option<i32>) {
        if self.pending.is_some() || !matches!(self.flow, Flow::Playing) || courses::info(destination).is_none() {
            return;
        }
        if destination == sim.course.request.id && area.is_some_and(|a| a != sim.course.request.area) {
            if self.pending_area.is_none() {
                self.pending_area = Some(AreaChange { area: area.unwrap_or(1), delta: [0.0; 3], entry, push: false, to_spawn: false });
            }
            return;
        }
        let area = area.filter(|a| *a != sim.course.request.area || destination != sim.course.request.id);
        self.enter_course(sim, destination, entry, area);
    }

    /// `enterCourseWarp`: score courses ask for an act first.
    fn enter_course(&mut self, sim: &GameSim, destination: &str, entry: Option<String>, area: Option<i32>) {
        if self.pending.is_some() || !matches!(self.flow, Flow::Playing) {
            return;
        }
        if is_score(destination) {
            let collected = std::array::from_fn(|i| self.save.has_star(destination, i as i32 + 1));
            self.flow = Flow::ActSelect(ActSelector::new(destination, entry, collected, self.save.has_star(destination, HUNDRED_COIN_ACT), self.save.best_coins(destination)));
            self.cues.push("star_select_music");
        } else {
            let mut request = CourseRequest::new(destination);
            request.entry = entry;
            request.area = area.unwrap_or(1);
            let _ = sim;
            self.pending = Some(Switch { request, carry: None, reveal: false, completed: None, fresh_run: false });
        }
    }

    // ---------------------------------------------------------------- death

    fn begin_death_warp(&mut self, sim: &mut GameSim, fall: bool) {
        let course = &sim.course;
        let target = (if fall { course.fall_warp.clone() } else { None }).or_else(|| course.death_warp.clone());
        let Some(target) = target else { return };
        let game_over = sim.mario.lives <= 0.0 && !(fall && course.fall_warp.is_some());
        let keep_run = fall && course.fall_warp.is_some() && target.course == course.request.id;
        let mut request = CourseRequest::new(&target.course);
        request.entry = Some(target.entry.clone());
        request.area = target.area;
        request.act = if target.course == course.request.id { course.request.act } else { 1 };
        let carry = keep_run.then(|| self.carry(sim));
        let kind = if fall { WipeKind::Circle } else { WipeKind::Bowser };
        if !fall {
            self.cues.push("death_warp");
        }
        let switch = Switch { request, carry, reveal: true, completed: None, fresh_run: false };
        self.flow = Flow::Closing { kind, frame: 0, then: if game_over { Then::GameOver } else { Then::Load(switch) } };
        // The sim is frozen while the wipe plays.
        sim.skip_mario_step = true;
    }

    // ---------------------------------------------------------------- stars

    fn finish_reward(&mut self, sim: &mut GameSim, c: Collected) {
        let course = sim.course.request.id.clone();
        let coins = sim.coins;
        self.director.finish_collect(&c);
        let hundred = c.reward.is_none();
        let grand = course == "bowser_3" && !hundred;
        let native_exit = !hundred && !self.director.no_exit && !grand;
        let mut completed = None;
        if native_exit {
            let number = SCORE_COURSES.iter().position(|id| *id == course).map_or(0, |i| i + 1);
            let key = c.key.is_some();
            let mission = courses::missions_for(&course).into_iter().find(|m| m.act == c.act);
            completed = Some(CompletedCourse {
                heading: if number > 0 { format!("COURSE {number}") } else if key { "CONGRATULATIONS!".into() } else { "CASTLE SECRET STAR".into() },
                title: if key {
                    format!("{} Clear", courses::info(if c.key == Some(CastleKey::Basement) { "bitdw" } else { "bitfs" }).map_or("", |i| i.name))
                } else {
                    mission.map_or_else(|| courses::info(&course).map_or("", |i| i.name).to_string(), |m| m.name.to_string())
                },
                coins,
                high_score: is_score(&course) && coins > self.save.best_coins(&course),
                key,
            });
            self.save.defer_course_save();
        }
        if self.director.no_exit || hundred {
            self.save.defer_course_save();
        }
        if let Some(key) = c.key {
            self.save.collect_key(key);
        } else {
            self.save.collect_star(&course, c.act, coins);
        }
        if native_exit || self.director.no_exit {
            self.cues.push("star_sound");
        } else {
            self.cues.push("star");
            if !grand {
                self.cues.push("clear");
            }
        }
        if grand {
            self.cues.push("stop_music");
            // Final star: the ending cutscene runs in the sim.
            let m = &mut sim.mario;
            m.hurt_counter = 0.0;
            m.heal_counter = 0.0;
            m.set_action(ACT_JUMBO_STAR_CUTSCENE, 0.0);
            return;
        }
        let bonus = hundred || self.director.no_exit;
        self.begin_dance(sim, bonus, hundred, completed);
    }

    fn begin_dance(&mut self, sim: &mut GameSim, bonus: bool, hundred: bool, completed: Option<CompletedCourse>) {
        let m = &mut sim.mario;
        if !bonus {
            m.hurt_counter = 0.0;
            m.heal_counter = 0.0;
            m.wing_cap_timer = m.wing_cap_timer.min(1.0);
            m.metal_cap_timer = m.metal_cap_timer.min(1.0);
            m.vanish_cap_timer = m.vanish_cap_timer.min(1.0);
        }
        if !bonus {
            self.cues.push("stop_music");
        }
        let mut action = if bonus { ACT_STAR_DANCE_NO_EXIT } else { ACT_STAR_DANCE_EXIT };
        if m.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0 {
            action = ACT_STAR_DANCE_WATER;
        }
        if m.action & ACT_FLAG_AIR != 0 {
            action = ACT_FALL_AFTER_STAR_GRAB;
        }
        m.set_action(action, if bonus { 1.0 } else { 0.0 });
        self.flow = Flow::Dance(Dance {
            bonus,
            hundred,
            yaw: 0.0,
            diameter: 100.0,
            model_visible: false,
            model_pos: [0.0; 3],
            model_scale: 0.4,
            model_rot: 0.0,
            prompt: None,
            frame: 0,
            saved: false,
        });
        if let Some(done) = completed {
            self.pending_result = Some(done);
        }
    }

    fn step_dance(&mut self, sim: &mut GameSim, mut d: Dance, controller: &ControllerSnapshot) {
        let m = &mut sim.mario;
        if let Some(prompt) = &mut d.prompt {
            match prompt.step(controller) {
                BonusResult::NextPage => self.cues.push("sign_page"),
                BonusResult::Answered(choice) => {
                    if choice == 0 {
                        self.save.confirm_course_save();
                    }
                    d.prompt = None;
                    m.action_state = 2.0;
                    m.action_timer = 0.0;
                    self.cues.push("sign_close");
                }
                BonusResult::None => {}
            }
            self.flow = Flow::Dance(d);
            return;
        }
        if d.bonus && m.action_state == 2.0 {
            let water = m.action == ACT_STAR_DANCE_WATER;
            if m.action_timer >= if water { 24.0 } else { 18.0 } {
                d.model_visible = false;
                m.set_action(if water { ACT_WATER_IDLE } else { ACT_IDLE }, 0.0);
                self.flow = Flow::Playing;
                return;
            }
            self.flow = Flow::Dance(d);
            return;
        }
        if m.action == ACT_FALL_AFTER_STAR_GRAB || m.action_timer == 0.0 {
            self.flow = Flow::Dance(d);
            return;
        }
        let frame = m.action_timer as i32;
        d.frame = frame;
        if frame == 1 {
            self.cues.push(if d.bonus { "clear" } else if self.pending_result.as_ref().is_some_and(|c| c.key) { "exit_key" } else { "exit_star" });
            d.model_visible = true;
            d.model_scale = 0.4;
            d.model_rot = 0.0;
            d.model_pos = [m.pos[0], m.pos[1] + 30.0, m.pos[2]];
            d.yaw = m.face_angle[1] + 32768.0;
        }
        if frame <= 41 && frame >= 1 {
            let half = (d.diameter / 2.0).trunc();
            d.model_pos[0] = m.pos[0] + jumpman_sim::math::sins(d.yaw) * half;
            d.model_pos[2] = m.pos[2] + jumpman_sim::math::sins(d.yaw + 0x4000 as f64) * half;
            d.model_pos[1] += 5.0;
            d.model_rot += std::f64::consts::PI / 8.0;
            d.yaw += 0x2000 as f64;
            d.diameter += if frame <= 35 { 1.0 } else { -20.0 };
        } else if frame > 41 && frame < 52 {
            d.model_scale = f64::from(frame - 42) / 10.0;
            d.model_rot += std::f64::consts::PI / 8.0;
        } else if frame >= 52 {
            d.model_scale = 1.0;
            d.model_rot = m.face_angle[1] * std::f64::consts::PI / 32768.0;
        }
        if frame == 42 {
            self.cues.push("star");
        }
        if d.bonus && frame == 80 {
            m.action_state = 1.0;
            d.prompt = Some(BonusSave::new(d.hundred));
            self.cues.push("sign_open");
            self.flow = Flow::Dance(d);
            return;
        }
        if frame == 101 {
            d.model_visible = false;
        }
        if !d.bonus && frame == 80 {
            // Closing wipe starts; the course changes after it.
            let (entry, area) = (sim.course.success_entry.clone(), sim.course.success_area);
            let destination = if entry.is_some() { "castle_inside" } else { "castle_grounds" };
            let mut request = CourseRequest::new(destination);
            request.entry = entry;
            request.area = area.unwrap_or(1);
            let completed = self.pending_result.take();
            self.flow = Flow::Closing { kind: WipeKind::Mario, frame: 0, then: Then::Load(Switch { request, carry: None, reveal: true, completed, fresh_run: false }) };
            return;
        }
        self.flow = Flow::Dance(d);
    }

    fn finish_result(&mut self, sim: &mut GameSim, choice: SaveChoice) {
        if choice != SaveChoice::Continue {
            self.save.confirm_course_save();
        }
        let m = &mut sim.mario;
        m.face_angle[1] = to_s16(m.face_angle[1] + 32768.0) as f64;
        m.set_action(ACT_IDLE, 0.0);
        self.cues.push("star");
        self.flow = Flow::Playing;
        if choice == SaveChoice::Quit {
            // Back to the start of a new session, as the web build reloaded the page.
            let request = CourseRequest::new("castle_grounds");
            self.pending = Some(Switch { request, carry: None, reveal: true, completed: None, fresh_run: true });
        }
    }

    // ---------------------------------------------------------------- course change

    /// Load the queued course into `sim`. Returns the finished switch for the view layer.
    pub fn perform_switch(&mut self, sim: &mut GameSim, root: &Path) -> Result<Switch, String> {
        let Some(switch) = self.pending.take() else { return Err("no pending switch".into()) };
        self.linked = None;
        self.pending_area = None;
        self.tower_fade = 0;
        self.look_pitch = 0.0;
        self.look_yaw = 0.0;
        self.fade = 0.0;
        let mut next = GameSim::load(root, switch.request.clone())?;
        next.mario.lives = if switch.fresh_run { 4.0 } else { sim.mario.lives };
        next.camera_yaw = sim.camera_yaw;
        if let Some(c) = &switch.carry {
            next.mario.health = c.health;
            next.mario.hurt_counter = c.hurt_counter;
            next.mario.heal_counter = c.heal_counter;
            next.coins = c.coins;
            next.red_coins = c.red_coins;
        }
        *sim = next;
        self.initial_area = sim.course.request.area;
        self.attach(sim, root);
        if let Some(c) = &switch.carry {
            self.prev_coins = c.coins;
            self.prev_red = c.red_coins;
        }
        self.reveal = switch.reveal.then_some(0);
        if let Some(done) = &switch.completed {
            self.flow = Flow::AwaitResult(done.clone());
        }
        if switch.fresh_run {
            self.save.revert_course_save();
        }
        let name = courses::missions_for(&switch.request.id).into_iter().find(|m| m.act == switch.request.act).map(|m| m.name.to_uppercase());
        self.toast(name.unwrap_or_else(|| sim.course.name.to_uppercase()), 2.6);
        sim.stars = self.save.stars();
        Ok(switch)
    }
}

fn to_entrance_doors(data: &ProgressData) -> Vec<super::data::CastleDoor> {
    data.doors.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn start(course: &str, act: i32) -> (GameSim, Progress) {
        let mut req = CourseRequest::new(course);
        req.act = act;
        let mut sim = GameSim::load(&assets(), req).unwrap();
        let p = Progress::new(&mut sim, &assets(), SaveFile::in_memory(1));
        (sim, p)
    }

    #[test]
    fn linked_area_switch_keeps_mario_and_caches_both_areas() {
        let (mut sim, mut p) = start("ddd", 1);
        let objects_a = sim.mario.objects.objects.len();
        sim.mario.pos = [1000.0, 100.0, 0.0];
        let ch = AreaChange { area: 2, delta: [-8192.0, 0.0, 0.0], entry: None, push: false, to_spawn: false };
        p.change_area(&mut sim, &assets(), ch).unwrap();
        assert_eq!(sim.course.request.area, 2);
        assert_eq!(sim.mario.pos[0], 1000.0 - 8192.0);
        assert!(p.linked.as_ref().is_some_and(|l| l.area == 1));
        let ch = AreaChange { area: 1, delta: [8192.0, 0.0, 0.0], entry: None, push: false, to_spawn: false };
        p.change_area(&mut sim, &assets(), ch).unwrap();
        assert_eq!(sim.course.request.area, 1);
        assert_eq!(sim.mario.pos[0], 1000.0);
        assert_eq!(sim.mario.objects.objects.len(), objects_a);
        assert!(p.linked.as_ref().is_some_and(|l| l.area == 2));
    }

    fn idle() -> ControllerSnapshot {
        ControllerSnapshot::default()
    }

    fn press(b: u32) -> ControllerSnapshot {
        ControllerSnapshot { button_pressed: b, button_down: b, ..Default::default() }
    }

    /// One engine-free frame: pre, sim (when running), post.
    fn frame(sim: &mut GameSim, p: &mut Progress, c: &ControllerSnapshot) {
        p.pre_tick(sim);
        if p.sim_running() {
            sim.tick(c);
        }
        p.post_tick(sim, c);
    }

    #[test]
    fn warp_into_score_course_opens_selector_then_loads_chosen_act() {
        let (mut sim, mut p) = start("castle_inside", 1);
        p.begin_warp(&sim, "bob", Some("WARP_NODE_0A".into()), None);
        assert!(matches!(p.flow, Flow::ActSelect(_)));
        assert!(!p.sim_running());
        for _ in 0..12 {
            frame(&mut sim, &mut p, &idle());
        }
        frame(&mut sim, &mut p, &press(A_BUTTON));
        let sw = p.pending.as_ref().expect("switch queued");
        assert_eq!((sw.request.id.as_str(), sw.request.act), ("bob", 1));
        let done = p.perform_switch(&mut sim, &assets()).unwrap();
        assert_eq!(done.request.entry.as_deref(), Some("WARP_NODE_0A"));
        assert_eq!(sim.course.request.id, "bob");
        assert!(p.sim_running());
        assert_eq!(p.director.course, "bob");
    }

    #[test]
    fn non_score_warps_load_directly() {
        let (sim, mut p) = start("castle_inside", 1);
        p.begin_warp(&sim, "pss", Some("WARP_NODE_0A".into()), None);
        assert_eq!(p.pending.as_ref().unwrap().request.id, "pss");
    }

    #[test]
    fn painting_floor_opens_the_course_selector() {
        let (mut sim, mut p) = start("castle_inside", 1);
        // Find a floor surface with the Bob painting warp type (0xd3).
        let kind = 0xd3;
        let surface = sim.mario.world.surfaces().iter().find(|s| s.kind == kind).cloned();
        if let Some(surface) = surface {
            sim.mario.floor = Some(surface.clone());
            sim.mario.floor_height = surface.v1[1];
            sim.mario.pos[1] = surface.v1[1];
            p.step_playing_world(&mut sim);
            assert!(matches!(p.flow, Flow::ActSelect(ref s) if s.course == "bob"));
        } else {
            panic!("castle has no painting warp floor");
        }
    }

    #[test]
    fn clicking_an_act_in_the_selector_starts_it_after_the_input_delay() {
        let (mut sim, mut p) = start("castle_inside", 1);
        p.enter_course(&sim, "bob", Some("WARP_NODE_0A".into()), None);
        assert!(matches!(p.flow, Flow::ActSelect(_)));
        p.menu_click = Some(MenuClick::Act(1));
        frame(&mut sim, &mut p, &idle());
        assert!(p.pending.is_none(), "too early");
        for _ in 0..12 {
            frame(&mut sim, &mut p, &idle());
        }
        p.menu_click = Some(MenuClick::Act(1));
        frame(&mut sim, &mut p, &idle());
        assert_eq!(p.pending.as_ref().unwrap().request.id, "bob");
    }

    #[test]
    fn death_warp_wipes_then_reloads_the_castle_and_game_over_at_zero_lives() {
        let (mut sim, mut p) = start("bob", 1);
        sim.mario.lives = 2.0;
        sim.mario.respawn(false);
        frame(&mut sim, &mut p, &idle());
        let Flow::Closing { kind, .. } = &p.flow else { panic!("expected wipe, got {:?}", p.flow) };
        assert_eq!(*kind, WipeKind::Bowser);
        for _ in 0..50 {
            frame(&mut sim, &mut p, &idle());
            if p.pending.is_some() {
                break;
            }
        }
        let sw = p.pending.as_ref().expect("reload queued after the wipe");
        assert_eq!(sw.request.id, "castle_inside");
        assert!(sw.reveal);
        p.perform_switch(&mut sim, &assets()).unwrap();
        assert_eq!(sim.mario.lives, 2.0, "lives carry over");
        assert!(matches!(p.flow, Flow::Playing));

        // Out of lives: the same warp ends at game over.
        sim.mario.lives = 0.0;
        let (mut sim, mut p) = start("bob", 1);
        sim.mario.lives = 0.0;
        sim.mario.respawn(false);
        for _ in 0..60 {
            frame(&mut sim, &mut p, &idle());
        }
        assert!(matches!(p.flow, Flow::GameOver { .. }), "{:?}", p.flow);
        for _ in 0..25 {
            frame(&mut sim, &mut p, &idle());
        }
        frame(&mut sim, &mut p, &press(A_BUTTON));
        let sw = p.pending.as_ref().unwrap();
        assert!(sw.fresh_run && sw.request.id == "castle_grounds");
        p.perform_switch(&mut sim, &assets()).unwrap();
        assert_eq!(sim.mario.lives, 4.0);
    }

    #[test]
    fn collecting_a_star_dances_wipes_and_returns_to_the_castle_with_a_result() {
        let (mut sim, mut p) = start("wf", 1);
        p.director.on_event(&ObjectEvent::WhompKing { pos: [0.0; 3] });
        sim.coins = 17;
        // Stand on the star.
        let pos = p.director.primary().pos;
        let hit = sim.mario.world.find_floor(pos[0], pos[1] + 10.0, pos[2]);
        sim.mario.pos = pos;
        sim.mario.floor_height = hit.height;
        for _ in 0..3 {
            frame(&mut sim, &mut p, &idle());
            if matches!(p.flow, Flow::Dance(_)) {
                break;
            }
        }
        assert!(matches!(p.flow, Flow::Dance(_)), "dance began: {:?}", p.flow);
        assert!(p.save.has_star("wf", 1));
        assert!(p.save.has_unsaved(), "course stars wait for the save prompt");
        let mut closing = false;
        for _ in 0..400 {
            frame(&mut sim, &mut p, &idle());
            closing |= matches!(p.flow, Flow::Closing { kind: WipeKind::Mario, .. });
            if p.pending.is_some() {
                break;
            }
        }
        assert!(closing, "Mario wipe played");
        let sw = p.pending.take().expect("return to castle queued");
        assert_eq!(sw.request.id, "castle_inside");
        let done = sw.completed.clone().expect("result data");
        assert_eq!((done.heading.as_str(), done.coins), ("COURSE 2", 17));
        p.pending = Some(sw);
        p.perform_switch(&mut sim, &assets()).unwrap();
        assert!(matches!(p.flow, Flow::AwaitResult(_)));
        // The result appears once Mario lands, then Save persists.
        for _ in 0..600 {
            frame(&mut sim, &mut p, &idle());
            if matches!(p.flow, Flow::Result(_)) {
                break;
            }
        }
        assert!(matches!(p.flow, Flow::Result(_)), "{:?} action 0x{:x}", p.flow, sim.mario.action);
        for _ in 0..140 {
            frame(&mut sim, &mut p, &idle());
        }
        frame(&mut sim, &mut p, &press(A_BUTTON));
        assert!(matches!(p.flow, Flow::Playing));
        assert!(!p.save.has_unsaved());
        assert_eq!(p.save.stars(), 1);
    }

    #[test]
    fn hundred_coin_star_prompts_to_save() {
        let (mut sim, mut p) = start("bob", 1);
        sim.coins = 100;
        let mario = sim.mario.pos;
        p.director.hundred_coins(mario);
        let mut prompt = false;
        for i in 0..600 {
            if matches!(p.flow, Flow::Playing) && i > 40 {
                let star_y = p.director.hundred.as_ref().unwrap().pos[1];
                sim.mario.pos = [mario[0], star_y - 20.0, mario[2]];
            }
            frame(&mut sim, &mut p, &idle());
            if let Flow::Dance(d) = &p.flow {
                if d.prompt.is_some() {
                    prompt = true;
                    break;
                }
            }
        }
        assert!(prompt, "{:?}", p.flow);
        assert!(p.save.has_star("bob", 7));
        assert!(!p.sim_running());
        frame(&mut sim, &mut p, &press(A_BUTTON));
        assert!(!p.save.has_unsaved(), "Yes saved");
    }

    #[test]
    fn castle_entrance_door_loads_the_other_side() {
        let (mut sim, mut p) = start("castle_grounds", 1);
        let door = p.data.doors.iter().position(|d| d.destination == "castle_inside" && d.required_key.is_none()).expect("main door");
        let d = p.data.doors[door].clone();
        sim.mario.pos = [d.pos[0], d.pos[1], d.pos[2] + 10.0];
        sim.mario.set_action(ACT_WALKING, 0.0);
        p.step_playing_world(&mut sim);
        assert!(matches!(p.flow, Flow::Entering { .. }));
        for _ in 0..60 {
            frame(&mut sim, &mut p, &idle());
            if p.pending.is_some() {
                break;
            }
        }
        let sw = p.pending.as_ref().expect("door load queued");
        assert_eq!(sw.request.id, "castle_inside");
        assert_eq!(sw.request.entry.as_deref(), Some(d.dest_node.as_str()));
    }

    #[test]
    fn fading_warp_teleports_after_forty_frames() {
        let (mut sim, mut p) = start("bob", 1);
        let pad = sim.course.fading_warps[0].clone();
        sim.mario.pos = pad.pos;
        sim.mario.set_action(ACT_IDLE, 0.0);
        let mut faded_out = false;
        for _ in 0..60 {
            frame(&mut sim, &mut p, &idle());
            faded_out |= p.fade > 0.9;
        }
        assert!(faded_out);
        let d = (sim.mario.pos[0] - pad.target[0]).hypot(sim.mario.pos[2] - pad.target[2]);
        assert!(d < 300.0, "arrived at the paired pad, {d}");
    }
}
