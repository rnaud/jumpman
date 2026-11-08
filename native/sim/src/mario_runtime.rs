//! The Mario controller runtime (src/sim/MarioRuntime.ts).
//!
//! `MarioRuntime` owns Mario's state and the per-frame pipeline from mario.c:
//! `update_mario_inputs -> update_mario_geometry_inputs -> action loop`.
//! Physics runs at a fixed 30 Hz.
//!
//! # Layout
//! * The fields that `MarioState` (state.rs) already has live in `self.state` and are
//!   reachable as `m.pos`, `m.action`, ... through `Deref`/`DerefMut`, so a `&mut MarioRuntime`
//!   also coerces to the `&mut MarioState` the ground/air step helpers want. Runtime-only
//!   fields (caps, held object, controller, events, ...) are direct fields.
//! * `m.world` is an `Arc<CollisionWorld>` (copy-on-write through `Arc::make_mut`).
//!   `m.step_ground()` / `m.step_air(arg)` split-borrow the state and the world; do not call
//!   `perform_*_step(m, &m.world)` yourself.
//! * **The runtime owns the object arena**: `m.objects` is the [`ObjectManager`]. Mario refers
//!   to objects through `Option<ObjId>` (`held_obj`, `used_obj`, `interact_obj`,
//!   `riding_shell`, `grabbed_by`). Because objects are reachable from `m`, no method here
//!   needs an extra parameter; the object step itself is `object_manager::step(m, ..)`.
//! * Actions are `fn(&mut MarioRuntime) -> Option<bool>` (see `actions/`): return true when
//!   the action changed so the frame loop runs again.
use crate::characters::{link_guarding, step_banjo, step_link};
use crate::actions::cannon::CannonSeat;
use crate::actions::submerged::submerged_step;
use crate::actions::water;
use crate::actions::execute_mario_action;
use crate::collision::{CollisionWorld, Surface};
use crate::mario_helpers::*;
use crate::mario_step::*;
use crate::math::*;
use crate::object::*;
use crate::object_manager::ObjectManager;
use crate::state::*;
use crate::surfaces::*;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SpawnPoint {
    pub action: Option<u32>,
    pub pos: Vec3,
    pub face_yaw: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementMode {
    Default,
    /// Side-scrolling: `side_yaw` is where stick-right moves.
    Side,
    Topdown,
}

/// Debug record of the first 60 `set_action` calls (TS `transitions`).
#[derive(Clone, Debug)]
pub struct Transition {
    pub from: u32,
    pub to: u32,
    pub input: u32,
    pub floor_n: Option<Vec3>,
    pub floor_type: i32,
    pub pos: Vec3,
    pub fv: f64,
    pub intended: f64,
    pub face: f64,
}

pub const MAX_ACTION_ITERATIONS: u32 = 12;

/// Actions that may keep an object in Mario's hands.
const HOLDING_ACTIONS: &[u32] = &[
    ACT_HOLD_BEGIN_SLIDING, ACT_HOLD_BUTT_SLIDE, ACT_HOLD_STOMACH_SLIDE, ACT_HOLD_BUTT_SLIDE_AIR, ACT_HOLD_BUTT_SLIDE_STOP, ACT_DIVE_PICKING_UP,
    ACT_PICKING_UP_BOWSER, ACT_HOLDING_BOWSER, ACT_RELEASING_BOWSER,
    ACT_PICKING_UP,
    // Talking while carrying a holdable NPC (Mips: `bhv_mips_held`) keeps him in hand until the dialog is over.
    ACT_READING_NPC_DIALOG, ACT_WAITING_FOR_DIALOG,
    ACT_HOLD_IDLE,
    ACT_HOLD_WALKING,
    ACT_HOLD_DECELERATING,
    ACT_HOLD_JUMP,
    ACT_HOLD_FREEFALL,
    ACT_HOLD_JUMP_LAND,
    ACT_HOLD_FREEFALL_LAND,
    ACT_HOLD_JUMP_LAND_STOP,
    ACT_HOLD_FREEFALL_LAND_STOP,
    ACT_THROWING,
    ACT_PLACING_DOWN,
    ACT_AIR_THROW,
    ACT_AIR_THROW_LAND,
];

pub struct MarioRuntime {
    /// Core Mario state, also reachable through `Deref`.
    pub state: MarioState,
    pub world: Arc<CollisionWorld>,
    pub spawn: SpawnPoint,
    /// The object arena and behavior table (see module docs).
    pub objects: ObjectManager,

    pub twirl_yaw: f64,
    pub stick_x: f64,
    pub stick_y: f64,
    pub cannon: Option<CannonSeat>,
    /// The seat Mario was just shot out of (final state, `cooldown` = 6). The TS shares one seat
    /// object with the cannon list; the host takes this once to keep its copy in step.
    pub fired_cannon: Option<CannonSeat>,
    pub wing_cap_timer: f64,
    pub riding_shell: Option<ObjId>,
    pub shell_roll: f64,
    pub grabbed_by: Option<ObjId>,
    pub grab_timer: f64,
    pub metal_cap_timer: f64,
    pub vanish_cap_timer: f64,

    pub long_jump_is_slow: bool,
    pub movement_mode: MovementMode,
    /// Side-scrolling: world yaw that stick-right moves toward (0x4000 = +X).
    pub side_yaw: f64,

    pub swim_strength: f64,
    pub swim_yaw_vel: f64,
    /// Submerged-action state (`actions/water.rs`): water level, animation clock, bobbing, graphics copy.
    pub water: water::WaterState,
    /// Feathery Flap frames left; -1 once Kazooie is spent until Banjo lands.
    pub banjo_flap: f64,
    /// Banjo-Kazooie's extended moveset state (`banjo_ext.rs`): counters, eggs, pickups.
    pub banjo: crate::banjo_ext::BanjoExt,
    /// Air speed cap Banjo took off with (bsjump keeps his running momentum).
    pub jump_takeoff_speed: f64,

    pub invinc_timer: f64,
    pub max_health: f64,
    pub lives: f64,
    /// Called with `fall`; returns true if the game handled the death (e.g. a warp).
    pub request_death_warp: Option<Box<dyn FnMut(bool) -> bool + Send + Sync>>,

    /// Camera yaw used to rotate the analog stick into world space.
    pub camera_yaw: f64,
    pub controller: ControllerSnapshot,
    /// Buttons pressed in the tick that is about to run (set by the tick before the object pass), so object
    /// interactions that read A / B (`check_npc_talk`) see this frame's press, like `m->input` in the original.
    pub pending_pressed: u32,
    pub events: Vec<MarioEvent>,
    pub transitions: Vec<Transition>,

    /// Object interaction state, mirroring MarioState.heldObj/usedObj/interactObj.
    pub held_obj: Option<ObjId>,
    pub sign_motion: Vec3,
    pub used_obj: Option<ObjId>,
    pub pole_pos: f64,
    pub pole_yaw_velocity: f64,
    pub interact_obj: Option<ObjId>,
    pub interact_grabbable: bool,
    pub near_grabbable: bool,
    pub near_breakable: bool,

    pub action_loops: u32,
    pub death_height: f64,
    /// The faithful OoT Link controller (`link/moves.rs`); inert unless `link.enabled`.
    pub link: crate::link::LinkExt,
    /// Cutscene/special action state (`cutscene_ext.rs`): animation clock, cues for the host.
    pub cut: crate::cutscene_ext::CutExt,
}

impl Deref for MarioRuntime {
    type Target = MarioState;
    fn deref(&self) -> &MarioState { &self.state }
}
impl DerefMut for MarioRuntime {
    fn deref_mut(&mut self) -> &mut MarioState { &mut self.state }
}

impl MarioRuntime {
    pub fn new(world: Arc<CollisionWorld>, spawn: SpawnPoint, death_height: f64) -> Self {
        let mut m = Self {
            state: MarioState::default(),
            world,
            spawn,
            objects: ObjectManager::default(),
            twirl_yaw: 0.0,
            stick_x: 0.0,
            stick_y: 0.0,
            cannon: None,
            fired_cannon: None,
            wing_cap_timer: 0.0,
            riding_shell: None,
            shell_roll: 0.0,
            grabbed_by: None,
            grab_timer: 0.0,
            metal_cap_timer: 0.0,
            vanish_cap_timer: 0.0,
            long_jump_is_slow: false,
            movement_mode: MovementMode::Default,
            side_yaw: 0x4000 as f64,
            swim_strength: 160.0,
            swim_yaw_vel: 0.0,
            water: Default::default(),
            banjo_flap: 0.0,
            banjo: Default::default(),
            jump_takeoff_speed: 0.0,
            invinc_timer: 0.0,
            max_health: 0x880 as f64,
            lives: 4.0,
            request_death_warp: None,
            camera_yaw: 0.0,
            controller: ControllerSnapshot::default(),
            pending_pressed: 0,
            events: vec![],
            transitions: vec![],
            held_obj: None,
            sign_motion: [0.0; 3],
            used_obj: None,
            pole_pos: 0.0,
            pole_yaw_velocity: 0.0,
            interact_obj: None,
            interact_grabbable: false,
            near_grabbable: false,
            near_breakable: false,
            action_loops: 0,
            link: Default::default(),
            cut: Default::default(),
            death_height,
        };
        m.state.health = m.max_health;
        m.state.ceil_height = 20000.0;
        m.state.slide_terrain = m.world.slide_terrain;
        m.reset_to_spawn();
        m
    }

    /// Convenience with the TS default death height (-1600).
    pub fn with_default_death_height(world: Arc<CollisionWorld>, spawn: SpawnPoint) -> Self { Self::new(world, spawn, -1600.0) }

    /// Replace the object manager (e.g. after loading a course's object specs). Clears every
    /// object reference Mario holds.
    pub fn set_objects(&mut self, objects: ObjectManager) {
        self.objects = objects;
        self.riding_shell = None;
        self.held_obj = None;
        self.used_obj = None;
        self.interact_obj = None;
        self.grabbed_by = None;
    }

    pub fn change_area(&mut self, world: Arc<CollisionWorld>, spawn: SpawnPoint, death_height: f64, offset: Vec3) {
        self.world = world;
        self.state.slide_terrain = self.world.slide_terrain;
        self.spawn = spawn;
        self.death_height = death_height;
        for axis in 0..3 { self.state.pos[axis] += offset[axis]; }
        self.state.peak_height += offset[1];
        let floor = self.world.find_floor(self.pos[0], self.pos[1], self.pos[2]);
        self.state.floor_angle = floor.surface.as_ref().map_or(0.0, |f| atan2s(f.normal[2], f.normal[0]) as f64);
        self.state.floor = floor.surface;
        self.state.floor_height = floor.height;
        let ceil = self.world.vec3f_find_ceil(&self.state.pos, floor.height);
        self.state.ceil = ceil.surface;
        self.state.ceil_height = ceil.height;
        self.state.wall = None;
    }

    pub fn reset_to_spawn(&mut self) {
        self.riding_shell = None;
        if let Some(h) = self.held_obj { self.objects[h].set_held_state(HELD_FREE); }
        self.cannon = None;
        self.held_obj = None;
        self.used_obj = None;
        self.interact_obj = None;
        self.interact_grabbable = false;
        self.near_grabbable = false;
        self.near_breakable = false;
        let [x, sy, z] = self.spawn.pos;
        let hit = self.world.find_floor(x, sy + 200.0, z);
        let spawn_action = self.spawn.action.unwrap_or(0);
        let s = &mut self.state;
        s.pos = [x, if spawn_action != 0 || hit.surface.is_none() { sy } else { hit.height }, z];
        s.vel = [0.0; 3];
        s.forward_vel = 0.0;
        s.slide_vel_x = 0.0;
        s.slide_vel_z = 0.0;
        s.slide_yaw = 0.0;
        s.face_angle = [0.0, self.spawn.face_yaw, 0.0];
        s.intended_mag = 0.0;
        s.intended_yaw = self.spawn.face_yaw;
        s.action = ACT_IDLE;
        s.prev_action = 0;
        s.action_state = 0.0;
        s.action_timer = 0.0;
        s.action_arg = 0.0;
        s.input = 0;
        s.flags = 0;
        s.particle_flags = 0;
        s.floor_height = if hit.surface.is_some() { hit.height } else { sy };
        s.floor_angle = hit.surface.as_ref().map_or(0.0, |f| atan2s(f.normal[2], f.normal[0]) as f64);
        s.floor = hit.surface;
        s.ceil = None;
        s.wall = None;
        s.ceil_height = 20000.0;
        s.peak_height = s.pos[1];
        s.quicksand_depth = 0.0;
        s.squish_timer = 0.0;
        s.double_jump_timer = 0.0;
        s.wall_kick_timer = 0.0;
        s.health = self.max_health;
        s.hurt_counter = 0.0;
        s.heal_counter = 0.0;
        s.jump_combo = 0.0;
        s.anim_frame = 0.0;
        s.anim_length = 1.0;
        self.swim_strength = 160.0;
        self.swim_yaw_vel = 0.0;
        self.water = Default::default();
        self.banjo_flap = 0.0;
        self.banjo.on_respawn();
        self.cut.on_respawn();
        self.invinc_timer = 0.0;
        self.controller = ControllerSnapshot::default();
        if self.spawn.action == Some(ACT_FLYING) {
            self.grant_wing_cap(1200.0);
            self.set_action(ACT_FLYING, 2.0);
        } else if spawn_action != 0 {
            self.set_action(spawn_action, 0.0);
        }
        self.camera_yaw = to_s16(self.spawn.face_yaw + 0x8000 as f64) as f64;
    }

    pub fn respawn(&mut self, fall: bool) {
        if let Some(mut f) = self.request_death_warp.take() {
            let handled = f(fall);
            self.request_death_warp = Some(f);
            if handled { return; }
        }
        self.state.respawns += 1.0;
        self.events.push(MarioEvent::Respawn);
        self.reset_to_spawn();
    }

    /// Advance one 30 Hz simulation frame.
    pub fn step(&mut self, controller: &ControllerSnapshot, camera_yaw: f64, as_link: bool, as_banjo: bool) {
        self.events.clear();
        self.controller = controller.clone();
        self.camera_yaw = camera_yaw;

        // Scripted flight crosses empty space outside the final arena.
        if self.action == ACT_JUMBO_STAR_CUTSCENE && self.action_arg > 0.0 {
            execute_mario_action(self);
            return;
        }
        if !self.update_mario_inputs() { return; }

        if self.wing_cap_timer > 0.0 { self.wing_cap_timer -= 1.0; }
        if self.metal_cap_timer > 0.0 { self.metal_cap_timer -= 1.0; }
        if self.vanish_cap_timer > 0.0 { self.vanish_cap_timer -= 1.0; }
        if tick_cap(&mut self.state.flags, self.wing_cap_timer, MARIO_WING_CAP) { self.wing_cap_timer = 0.0; }
        if tick_cap(&mut self.state.flags, self.metal_cap_timer, MARIO_METAL_CAP) { self.metal_cap_timer = 0.0; }
        if tick_cap(&mut self.state.flags, self.vanish_cap_timer, MARIO_VANISH_CAP) { self.vanish_cap_timer = 0.0; }

        if self.grabbed_by.is_some() {
            self.step_grabbed();
            self.check_void();
            self.update_mario_health();
            self.advance_anim();
            return;
        }

        if [ACT_FALL_AFTER_STAR_GRAB, ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_NO_EXIT, ACT_STAR_DANCE_WATER].contains(&self.action) {
            execute_mario_action(self);
            return;
        }
        let faithful = water::faithful();
        if !faithful && self.step_legacy_water() { return; }
        // check_common_*_cancels: set_water_plunge_action below waterLevel - 100.
        if faithful && water::plunges_into_water(self.action) && self.pos[1] < self.water.level - 100.0 {
            water::set_water_plunge_action(self);
        }
        let submerged = faithful && self.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED;
        if submerged {
            // Link and Banjo swim with Mario's own submerged actions.
            self.link_boots_in_water(as_link);
        }

        let mut in_loop = submerged || (!(as_link && step_link(self)) && !(as_banjo && step_banjo(self)));
        self.action_loops = 0;
        while in_loop && self.action_loops < MAX_ACTION_ITERATIONS {
            self.action_loops += 1;
            in_loop = execute_mario_action(self);
        }

        if let Some(shell) = self.riding_shell {
            let (pos, yaw) = (self.pos, self.face_angle[1]);
            let s = &mut self.objects[shell];
            s.pos = pos;
            s.face_yaw = yaw;
        }
        let in_water = faithful && self.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED;
        // Swimming never ran the dry surface hazards (lava/quicksand are not checked under water).
        if !in_water { self.update_surface_hazards(); }
        self.check_void();
        self.update_mario_health();
        // squish_mario_model: 0xFF (flattened by ACT_SQUISHED / BBH) is held, other values count down.
        if self.squish_timer > 0.0 && self.squish_timer != 255.0 { self.state.squish_timer -= 1.0; }
        self.advance_anim();
        // The submerged code tracks the real animation; other groups do not use the clock.
        if in_water { self.water.clock.advance(); } else { self.water.clock.invalidate(); }
        self.water.drop_object = false;
    }

    /// Link's Iron Boots act as a metal cap and the Zora Tunic breathes under water. The Link
    /// controller applies both while he is on dry land (`passive`); under water Mario's own
    /// submerged actions run instead, so repeat them here.
    fn link_boots_in_water(&mut self, as_link: bool) {
        use crate::link::items::{Boots, Tunic};
        if !as_link || !self.link.enabled { return; }
        if self.link.inv.boots == Boots::Iron && self.pos[1] < self.water.level + 40.0 {
            self.metal_cap_timer = self.metal_cap_timer.max(3.0);
            self.state.flags |= MARIO_METAL_CAP;
        }
        if self.link.inv.tunic == Tunic::Zora && self.pos[1] < self.water.level - 20.0 {
            self.state.health = self.max_health.max(self.health);
        }
    }

    /// The pre-port swimming path (TS `MarioRuntime.step`): metal-cap bottom walking, the plunge and
    /// `submerged_step`. Only used while [`water::faithful`] is off (see there). True = frame done.
    fn step_legacy_water(&mut self) -> bool {
        let surface = self.world.water_level(self.pos[0], self.pos[2]);
        if self.metal_cap_timer > 0.0 && self.pos[1] < surface - 20.0 {
            self.step_metal_water();
            self.check_void();
            self.update_mario_health();
            self.advance_anim();
            return true;
        }
        if self.action & ACT_FLAG_SWIMMING == 0 && self.pos[1] < surface - 100.0 {
            self.set_action(ACT_WATER_PLUNGE, 0.0);
            self.state.pos[1] = surface - 100.0;
            self.state.face_angle[0] = 0.0;
            self.state.face_angle[2] = 0.0;
            self.swim_yaw_vel = 0.0;
            self.state.forward_vel *= 0.25;
            self.state.vel[1] *= 0.5;
        }
        if self.action & ACT_FLAG_SWIMMING != 0 && self.action != ACT_DROWNING && self.action != ACT_WATER_DEATH {
            submerged_step(self, surface);
            self.check_void();
            self.update_mario_health();
            return true;
        }
        false
    }

    /// Simplified metal-water mode: sink to the bottom and walk there.
    fn step_metal_water(&mut self) {
        let floor = self.world.find_floor(self.pos[0], self.pos[1] + 100.0, self.pos[2]);
        let bottom = if floor.surface.is_some() { Some(floor.height) } else { None };
        let Some(bottom) = bottom else {
            self.state.vel[1] -= 5.0;
            self.state.pos[1] += self.vel[1];
            self.set_action(ACT_METAL_WATER_FALLING, 0.0);
            return;
        };

        if self.pos[1] > bottom + 4.0 {
            self.state.vel[1] -= 5.0;
            self.state.pos[1] += self.vel[1];
            self.set_action(ACT_METAL_WATER_FALLING, 0.0);
            if self.pos[1] <= bottom {
                self.state.pos[1] = bottom;
                self.state.vel[1] = 0.0;
            }
            return;
        }

        self.state.pos[1] = bottom;
        self.state.vel[1] = 0.0;
        self.state.forward_vel = 0.0;
        if self.controller.button_pressed & A_BUTTON != 0 {
            self.state.vel[1] = 34.0;
            self.state.pos[1] += 2.0;
            self.set_action(ACT_METAL_WATER_JUMP, 0.0);
            return;
        }

        if self.intended_mag > 5.0 {
            let speed = 7f64.min(self.intended_mag / 4.0);
            let yaw = self.intended_yaw;
            self.state.face_angle[1] = yaw;
            self.state.pos[0] += js_sin(yaw * std::f64::consts::PI * 2.0 / 65536.0) * speed;
            self.state.pos[2] += js_cos(yaw * std::f64::consts::PI * 2.0 / 65536.0) * speed;
            self.set_action(ACT_METAL_WATER_WALKING, 0.0);
        } else {
            self.set_action(ACT_METAL_WATER_STANDING, 0.0);
        }
    }

    fn advance_anim(&mut self) {
        self.state.anim_frame += 1.0;
        if self.anim_frame > self.anim_length { self.state.anim_frame = 0.0; }
    }

    /// check_lava_boost / mario_update_quicksand from interaction.c and mario_step.c.
    fn update_surface_hazards(&mut self) {
        let Some(floor) = self.floor.clone() else { return };
        if self.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE | ACT_FLAG_RIDING_SHELL) != 0 { return; }

        if floor.kind == SURFACE_BURNING && self.pos[1] < self.floor_height + 10.0 {
            self.state.hurt_counter += 18.0;
            self.state.vel[1] = 84.0;
            self.events.push(MarioEvent::Hurt { damage: 0.0 });
            self.set_action(ACT_LAVA_BOOST, 0.0);
            return;
        }

        if self.pos[1] > self.floor_height + 1.0 || !is_quicksand(floor.kind) { return; }
        let kind = floor.kind;
        if kind == SURFACE_INSTANT_QUICKSAND || kind == SURFACE_INSTANT_MOVING_QUICKSAND {
            self.state.quicksand_depth = 180.0;
            self.events.push(MarioEvent::Death);
            self.set_action(ACT_QUICKSAND_DEATH, 0.0);
            return;
        }

        self.state.quicksand_depth = 1.1f64.max(self.quicksand_depth + 0.5);
        let cap: f64 = if kind == SURFACE_SHALLOW_QUICKSAND {
            10.0
        } else if kind == SURFACE_SHALLOW_MOVING_QUICKSAND {
            25.0
        } else if kind == SURFACE_DEEP_MOVING_QUICKSAND || kind == SURFACE_DEEP_QUICKSAND {
            160.0
        } else {
            60.0
        };
        self.state.quicksand_depth = cap.min(self.quicksand_depth);
        if self.quicksand_depth >= 160.0 {
            self.events.push(MarioEvent::Death);
            self.set_action(ACT_QUICKSAND_DEATH, 0.0);
        }
    }

    /// update_mario_health (mario.c).
    fn update_mario_health(&mut self) {
        if self.health < 0x100 as f64 {
            if self.action & ACT_FLAG_INTANGIBLE == 0 { self.on_health_depleted(); }
            return;
        }

        if self.heal_counter == 0.0 && self.hurt_counter == 0.0
            && self.action & ACT_FLAG_SWIMMING != 0 && self.action & ACT_FLAG_INTANGIBLE == 0
        {
            // mario.c reads the level sampled at the start of the frame; the pre-port path looked it up again here.
            let level = if water::faithful() { self.water.level } else { self.world.water_level(self.pos[0], self.pos[2]) };
            if self.pos[1] >= level - 140.0 && !self.world.snow_terrain {
                self.state.health += 0x1a as f64;
            } else {
                self.state.health -= if self.world.snow_terrain { 3.0 } else { 1.0 };
            }
        }

        if self.heal_counter > 0.0 {
            self.state.health += 0x40 as f64;
            self.state.heal_counter -= 1.0;
        }
        if self.hurt_counter > 0.0 {
            self.state.health -= 0x40 as f64;
            self.state.hurt_counter -= 1.0;
        }

        if self.health > self.max_health { self.state.health = self.max_health; }
        if self.health < 0x100 as f64 { self.on_health_depleted(); }
    }

    /// Health ran out. Submerged actions start `ACT_DROWNING` / `ACT_WATER_DEATH` themselves
    /// (`check_common_submerged_cancels`, `common_water_knockback_step`) once the knockback or
    /// shock they are in finishes; everything else dies at once.
    fn on_health_depleted(&mut self) {
        if !water::faithful() {
            // The pre-port path started the drowning sequence from here.
            if self.action & ACT_FLAG_SWIMMING != 0 { self.start_drowning(); } else { self.start_death(); }
        } else if self.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED {
            // The source clamps to 0xFF and lets the action table react next frame.
            self.state.health = 0xff as f64;
        } else {
            self.start_death();
        }
    }

    /// Start the drowning sequence while submerged (legacy path).
    fn start_drowning(&mut self) {
        if self.action == ACT_DROWNING || self.action == ACT_WATER_DEATH { return; }
        self.state.health = 0xff as f64;
        self.events.push(MarioEvent::Death);
        self.set_action(ACT_DROWNING, 0.0);
    }

    /// Start the source death sequence appropriate to the current knockback.
    fn start_death(&mut self) {
        if self.action == ACT_STANDING_DEATH || self.action == ACT_DEATH_ON_BACK || self.action == ACT_DEATH_ON_STOMACH { return; }
        // These cutscene actions run their own death sequence (cutscene_ext.rs).
        if crate::cutscene_ext::runs_own_death(self.action) { return; }

        self.state.health = 0xff as f64;
        let a = self.action;
        let action = if a == ACT_DEATH_ON_BACK || a == ACT_HARD_BACKWARD_GROUND_KB || a == ACT_HARD_BACKWARD_AIR_KB
            || a == ACT_BACKWARD_GROUND_KB || a == ACT_BACKWARD_AIR_KB
        {
            ACT_DEATH_ON_BACK
        } else if a == ACT_HARD_FORWARD_GROUND_KB || a == ACT_HARD_FORWARD_AIR_KB || a == ACT_FORWARD_GROUND_KB || a == ACT_FORWARD_AIR_KB {
            ACT_DEATH_ON_STOMACH
        } else {
            ACT_STANDING_DEATH
        };

        self.events.push(MarioEvent::Death);
        self.set_action(action, 0.0);
    }

    /// take_damage_and_knock_back / determine_knockback_action (interaction.c).
    /// Damage is in source units: 1 = one health wedge (0x100).
    pub fn take_damage(&mut self, damage: f64, source_position: Option<Vec3>) -> bool {
        if damage <= 0.0 { return false; }
        if self.action & (ACT_FLAG_INVULNERABLE | ACT_FLAG_INTANGIBLE) != 0 || self.invinc_timer > 0.0 || self.vanish_cap_timer > 0.0 { return false; }

        let angle_to_object = match source_position {
            Some(p) => atan2s(p[2] - self.pos[2], p[0] - self.pos[0]) as f64,
            None => to_s16(self.face_angle[1] + 0x8000 as f64) as f64,
        };
        let facing_d_yaw = to_s16(angle_to_object - self.face_angle[1]) as f64;
        if source_position.is_some() && link_guarding(self) && facing_d_yaw.abs() < 0x4000 as f64 { return false; }
        self.state.hurt_counter += 4.0 * damage;
        if water::faithful() && water::is_in_water_action(self.action) {
            // determine_knockback_action, terrainIndex 2: water knockback ignores the strength.
            let object_y = source_position.map_or(self.pos[1], |p| p[1]);
            let action = water::water_knockback(self, angle_to_object, object_y);
            self.events.push(MarioEvent::Hurt { damage });
            self.drop_and_set_action(action, damage);
            return true;
        }
        let remaining_health = self.health - 0x40 as f64 * self.hurt_counter;
        let mut strength: i32 = if remaining_health < 0x100 as f64 || damage >= 4.0 { 2 } else if damage >= 2.0 { 1 } else { 0 };
        if self.metal_cap_timer > 0.0 { strength = (strength - 1).max(0); }
        let in_front = (-0x4000 as f64..=0x4000 as f64).contains(&facing_d_yaw);
        let air = self.action & ACT_FLAG_AIR != 0;

        let action;
        if air {
            if in_front {
                self.state.face_angle[1] = angle_to_object;
                self.state.forward_vel = -(16f64.max(self.forward_vel.abs()));
                action = if strength == 2 { ACT_HARD_BACKWARD_AIR_KB } else { ACT_BACKWARD_AIR_KB };
            } else {
                self.state.face_angle[1] = to_s16(angle_to_object + 0x8000 as f64) as f64;
                self.state.forward_vel = 16f64.max(self.forward_vel.abs());
                action = if strength == 2 { ACT_HARD_FORWARD_AIR_KB } else { ACT_FORWARD_AIR_KB };
            }
        } else if in_front {
            self.state.face_angle[1] = angle_to_object;
            self.state.forward_vel = -(16f64.max(self.forward_vel.abs()));
            action = if strength == 2 {
                ACT_HARD_BACKWARD_GROUND_KB
            } else if strength == 1 {
                ACT_BACKWARD_GROUND_KB
            } else {
                ACT_SOFT_BACKWARD_GROUND_KB
            };
        } else {
            self.state.face_angle[1] = to_s16(angle_to_object + 0x8000 as f64) as f64;
            self.state.forward_vel = 16f64.max(self.forward_vel.abs());
            action = if strength == 2 {
                ACT_HARD_FORWARD_GROUND_KB
            } else if strength == 1 {
                ACT_FORWARD_GROUND_KB
            } else {
                ACT_SOFT_FORWARD_GROUND_KB
            };
        }

        self.events.push(MarioEvent::Hurt { damage });
        self.drop_and_set_action(action, damage);
        true
    }

    fn check_void(&mut self) {
        let death_plane = self.floor.as_ref().is_some_and(|f| f.kind == SURFACE_DEATH_PLANE) && self.pos[1] < self.floor_height + 2048.0;
        if death_plane || self.pos[1] < self.death_height
            || self.pos[0].abs() >= self.world.horizontal_boundary
            || self.pos[2].abs() >= self.world.horizontal_boundary
        {
            self.respawn(true);
        }
    }

    // -------------------------------------------------------------------------
    // Input
    // -------------------------------------------------------------------------

    fn update_mario_inputs(&mut self) -> bool {
        self.state.particle_flags = 0;
        self.state.input = 0;
        self.state.flags &= 0xffffff;
        self.state.flags &= !(MARIO_PUNCHING | MARIO_KICKING | MARIO_TRIPPING | MARIO_METAL_SHOCK);

        self.update_mario_button_inputs();
        self.update_mario_joystick_inputs();
        if !self.update_mario_geometry_inputs() { return false; }

        if self.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED) == 0 { self.state.input |= INPUT_UNKNOWN_5; }

        if self.wall_kick_timer > 0.0 { self.state.wall_kick_timer -= 1.0; }
        if self.double_jump_timer > 0.0 { self.state.double_jump_timer -= 1.0; }
        true
    }

    fn update_mario_button_inputs(&mut self) {
        let (pressed, down) = (self.controller.button_pressed, self.controller.button_down);
        if pressed & A_BUTTON != 0 { self.state.input |= INPUT_A_PRESSED; }
        if down & A_BUTTON != 0 { self.state.input |= INPUT_A_DOWN; }

        if self.squish_timer == 0.0 {
            if pressed & B_BUTTON != 0 { self.state.input |= INPUT_B_PRESSED; }
            if down & Z_TRIG != 0 { self.state.input |= INPUT_Z_DOWN; }
            if pressed & Z_TRIG != 0 { self.state.input |= INPUT_Z_PRESSED; }
        }

        if self.interact_grabbable { self.state.input |= INPUT_INTERACT_OBJ_GRABBABLE; }

        if self.input & INPUT_A_PRESSED != 0 {
            self.state.frames_since_a = 0.0;
        } else if self.frames_since_a < 255.0 {
            self.state.frames_since_a += 1.0;
        }

        if self.input & INPUT_B_PRESSED != 0 {
            self.state.frames_since_b = 0.0;
        } else if self.frames_since_b < 255.0 {
            self.state.frames_since_b += 1.0;
        }
    }

    fn update_mario_joystick_inputs(&mut self) {
        let (stick_x, stick_y, stick_mag) = (self.controller.stick_x, self.controller.stick_y, self.controller.stick_mag);
        self.stick_x = stick_x;
        self.stick_y = stick_y;

        if self.movement_mode == MovementMode::Side {
            let mag = (stick_x.abs() / 64.0) * (stick_x.abs() / 64.0) * 64.0;
            self.state.intended_mag = if self.squish_timer == 0.0 { mag / 2.0 } else { mag / 8.0 };
            if self.intended_mag > 0.0 {
                self.state.intended_yaw = to_s16(if stick_x > 0.0 { self.side_yaw } else { self.side_yaw + 0x8000 as f64 }) as f64;
                self.state.input |= INPUT_NONZERO_ANALOG;
            } else {
                self.state.intended_yaw = self.face_angle[1];
            }
            return;
        }

        let mag = (stick_mag / 64.0) * (stick_mag / 64.0) * 64.0;
        self.state.intended_mag = if self.squish_timer == 0.0 { mag / 2.0 } else { mag / 8.0 };

        if self.intended_mag > 0.0 {
            let base = atan2s(-stick_y, stick_x) as f64;
            let yaw = if self.movement_mode == MovementMode::Topdown { base } else { base + self.camera_yaw };
            self.state.intended_yaw = to_s16(yaw) as f64;
            self.state.input |= INPUT_NONZERO_ANALOG;
        } else {
            self.state.intended_yaw = self.face_angle[1];
        }
    }

    fn update_mario_geometry_inputs(&mut self) -> bool {
        let world = self.world.clone();
        world.f32_find_wall_collision(&mut self.state.pos, 60.0, 50.0);
        world.f32_find_wall_collision(&mut self.state.pos, 30.0, 24.0);

        let floor_hit = find_movement_floor(&world, &self.state.pos, self.action);
        self.state.floor_height = floor_hit.height;
        self.state.floor = floor_hit.surface;

        let Some(floor) = self.floor.clone() else {
            self.respawn(true);
            return false;
        };

        let ceil_hit = world.vec3f_find_ceil(&self.state.pos, self.floor_height);
        self.state.ceil_height = ceil_hit.height;
        self.state.ceil = ceil_hit.surface;

        self.state.floor_angle = atan2s(floor.normal[2], floor.normal[0]) as f64;

        self.water.level = world.water_level(self.pos[0], self.pos[2]).max(water::NO_WATER_LEVEL);

        // mario.c also requires pos[1] > waterLevel - 40 here; the TS goldens do not, so that stays off (see DECOMP_TALLY).
        if mario_floor_is_slippery(&self.state) { self.state.input |= INPUT_ABOVE_SLIDE; }

        if crate::cutscene_ext::squish_input(self) { self.state.input |= INPUT_SQUISHED; }
        if self.pos[1] > self.floor_height + 100.0 { self.state.input |= INPUT_OFF_FLOOR; }
        if water::faithful() && self.pos[1] < self.water.level - 10.0 { self.state.input |= INPUT_IN_WATER; }
        true
    }

    // -------------------------------------------------------------------------
    // Object interactions
    // -------------------------------------------------------------------------

    /// mario_check_object_grab (interaction.c).
    pub fn mario_check_object_grab(&mut self) -> bool {
        let Some(obj) = self.interact_obj else { return false };
        if self.input & INPUT_INTERACT_OBJ_GRABBABLE == 0 { return false; }
        if matches!(self.objects[obj].behavior, ObjectBehaviorKind::Bowser | ObjectBehaviorKind::SmBowser) {
            let yaw = self.objects[obj].move_yaw;
            if (to_s16(self.face_angle[1] - yaw) as f64).abs() > 0x5555 as f64 { return false; }
            self.state.face_angle[1] = yaw;
            self.used_obj = Some(obj);
            return self.set_action(ACT_PICKING_UP_BOWSER, 0.0);
        }
        let o = &self.objects[obj];
        let facing = to_s16(atan2s(o.pos[2] - self.pos[2], o.pos[0] - self.pos[0]) as f64 - self.face_angle[1]);
        if !(-0x2aaa..=0x2aaa).contains(&facing) { return false; }

        self.used_obj = Some(obj);
        if self.action & ACT_FLAG_AIR == 0 { self.set_action(ACT_PICKING_UP, 0.0); }
        true
    }

    /// Grab a pole (TS `tryGrabPole`; interaction.c `mario_grab_used_object`/pole branch).
    pub fn try_grab_pole(&mut self, object: ObjId) -> bool {
        let id = self.action & ACT_ID_MASK;
        if !(0x80..0xa0).contains(&id) || (self.prev_action & ACT_FLAG_ON_POLE != 0 && self.used_obj == Some(object)) { return false; }
        let speed = self.forward_vel;
        self.drop_held_object();
        self.pole_yaw_velocity = if speed <= 10.0 { 0.0 } else { 0x1000 as f64 };
        self.used_obj = Some(object);
        self.pole_pos = self.pos[1] - self.objects[object].pos[1];
        self.state.forward_vel = 0.0;
        self.state.vel = [0.0; 3];
        self.set_action(if speed <= 10.0 { ACT_GRAB_POLE_SLOW } else { ACT_GRAB_POLE_FAST }, 0.0);
        true
    }

    /// mario_grab_used_object
    pub fn grab_used_object(&mut self) {
        if self.held_obj.is_some() { return; }
        let Some(used) = self.used_obj else { return };
        self.held_obj = Some(used);
        self.objects[used].set_held_state(HELD_HELD);
    }

    /// mario_drop_held_object
    pub fn drop_held_object(&mut self) {
        let Some(id) = self.held_obj else { return };
        let yaw = self.face_angle[1];
        let pos = self.pos;
        let o = &mut self.objects[id];
        o.set_held_state(HELD_DROPPED);
        o.pos = [pos[0] + 80.0 * sins(yaw), pos[1], pos[2] + 80.0 * coss(yaw)];
        o.move_yaw = yaw;
        o.face_yaw = yaw;
        o.forward_vel = 0.0;
        o.vel[1] = 0.0;
        self.held_obj = None;
    }

    /// mario_throw_held_object
    pub fn throw_held_object(&mut self) {
        let Some(id) = self.held_obj else { return };
        let yaw = self.face_angle[1];
        let pos = self.pos;
        let o = &mut self.objects[id];
        o.set_held_state(HELD_THROWN);
        o.pos = [pos[0] + 180.0 * sins(yaw), pos[1] + 75.0, pos[2] + 180.0 * coss(yaw)];
        o.move_yaw = yaw;
        o.face_yaw = yaw;
        o.forward_vel = 25.0;
        o.vel[1] = 20.0;
        self.held_obj = None;
    }

    // -------------------------------------------------------------------------
    // Actions / transitions
    // -------------------------------------------------------------------------

    pub fn start_king_grab(&mut self, king: ObjId) -> bool {
        let invulnerable = self.action & ACT_FLAG_INVULNERABLE != 0 || self.invinc_timer != 0.0;
        if self.grabbed_by.is_some() || (invulnerable && self.action & (ACT_FLAG_AIR | ACT_FLAG_INVULNERABLE | ACT_FLAG_ATTACKING) != 0) { return false; }
        self.grabbed_by = Some(king);
        self.grab_timer = 45.0;
        let kp = self.objects[king].pos;
        self.state.pos = [kp[0], kp[1] + 80.0, kp[2]];
        self.state.vel = [0.0; 3];
        self.state.forward_vel = 0.0;
        self.set_action(ACT_GRABBED, 0.0);
        true
    }

    fn step_grabbed(&mut self) {
        let king = self.grabbed_by.filter(|k| self.objects[*k].active);
        let Some(king) = king else {
            self.grabbed_by = None;
            self.set_action(ACT_FREEFALL, 0.0);
            return;
        };
        let k = &self.objects[king];
        let (kp, ky) = (k.pos, k.move_yaw);
        self.state.pos = [
            kp[0] + js_sin(ky * std::f64::consts::PI * 2.0 / 65536.0) * 120.0,
            kp[1] + 100.0,
            kp[2] + js_cos(ky * std::f64::consts::PI * 2.0 / 65536.0) * 120.0,
        ];
        self.state.face_angle[1] = ky;
        self.state.vel = [0.0; 3];
        self.set_action(ACT_GRABBED, 0.0);
    }

    pub fn throw_from_grab(&mut self, king: ObjId) {
        if self.grabbed_by != Some(king) { return; }
        self.grabbed_by = None;
        self.state.pos[1] += 100.0;
        self.state.vel = [0.0, 30.0, 0.0];
        self.set_action(ACT_FREEFALL, 0.0);
        let kp = self.objects[king].pos;
        self.take_damage(1.0, Some(kp));
    }

    pub fn ride_shell(&mut self, shell: ObjId) {
        self.shell_roll = 0.0;
        self.state.quicksand_depth = 0.0;
        self.riding_shell = Some(shell);
        self.drop_held_object();
        self.set_action(ACT_RIDING_SHELL_GROUND, 0.0);
    }

    pub fn vanished(&self) -> bool { self.flags & MARIO_VANISH_CAP != 0 }

    pub fn grant_wing_cap(&mut self, frames: f64) {
        self.wing_cap_timer = self.wing_cap_timer.max(frames);
        self.state.flags |= MARIO_WING_CAP;
        // interact_cap: idle / walking Mario puts the cap on (`ACT_PUTTING_ON_CAP`); opt-in, see `CutExt::cap_cutscene`.
        if self.cut.cap_cutscene { crate::cutscene_ext::cap_pickup_cutscene(self); }
    }

    pub fn grant_metal_cap(&mut self, frames: f64) {
        self.metal_cap_timer = self.metal_cap_timer.max(frames);
        self.state.flags |= MARIO_METAL_CAP;
        // interact_cap: idle / walking Mario puts the cap on (`ACT_PUTTING_ON_CAP`); opt-in, see `CutExt::cap_cutscene`.
        if self.cut.cap_cutscene { crate::cutscene_ext::cap_pickup_cutscene(self); }
    }

    pub fn grant_vanish_cap(&mut self, frames: f64) {
        self.vanish_cap_timer = self.vanish_cap_timer.max(frames);
        self.state.flags |= MARIO_VANISH_CAP;
        // interact_cap: idle / walking Mario puts the cap on (`ACT_PUTTING_ON_CAP`); opt-in, see `CutExt::cap_cutscene`.
        if self.cut.cap_cutscene { crate::cutscene_ext::cap_pickup_cutscene(self); }
    }

    /// set_mario_action. Always returns true (so actions can `return m.set_action(..)`).
    pub fn set_action(&mut self, action: u32, action_arg: f64) -> bool {
        let mut action = action;
        if let Some(shell) = self.riding_shell {
            if action & ACT_FLAG_RIDING_SHELL == 0 {
                self.objects[shell].deactivate(-1.0);
                self.riding_shell = None;
            }
        }
        if self.held_obj.is_some() && !HOLDING_ACTIONS.contains(&action) && !water::keeps_held_object(action) { self.drop_held_object(); }

        match action & 0x1c0 {
            0x40 => action = self.set_mario_action_moving(action),
            0x80 => action = self.set_mario_action_airborne(action, action_arg),
            // set_mario_action_submerged: only the metal jumps do anything.
            0xc0 if water::faithful() => if action == ACT_METAL_WATER_JUMP || action == ACT_HOLD_METAL_WATER_JUMP { self.state.vel[1] = 32.0; },
            _ => {}
        }

        self.state.flags &= !(0x10000 | 0x20000);

        if self.action & ACT_FLAG_AIR == 0 { self.state.flags &= !0x40000; }

        if self.transitions.len() < 60 {
            let t = Transition {
                from: self.prev_action,
                to: action,
                input: self.input,
                floor_n: self.floor.as_ref().map(|f| f.normal),
                floor_type: self.floor.as_ref().map_or(-1, |f| f.kind),
                pos: self.pos,
                fv: self.forward_vel,
                intended: self.intended_yaw,
                face: self.face_angle[1],
            };
            self.transitions.push(t);
        }

        self.state.prev_action = self.action;
        self.state.action = action;
        self.state.action_arg = action_arg;
        self.state.action_state = 0.0;
        self.state.action_timer = 0.0;
        self.state.anim_frame = 0.0;
        self.state.anim_length = 1.0;

        if action & ACT_FLAG_AIR == 0 { self.state.jump_combo = 0.0; }

        let vy = self.vel[1];
        match action {
            ACT_FALL_AFTER_STAR_GRAB => mario_set_forward_vel(&mut self.state, 0.0),
            ACT_SPECIAL_DEATH_EXIT | ACT_SPECIAL_EXIT_AIRBORNE => self.state.vel[1] = 64.0,
            ACT_SPAWN_SPIN_AIRBORNE => mario_set_forward_vel(&mut self.state, 2.0),
            ACT_JUMP => {
                self.state.jump_combo = 1.0;
                self.events.push(MarioEvent::Jump { action, vy });
            }
            ACT_DOUBLE_JUMP => {
                self.state.jump_combo = 2.0;
                self.events.push(MarioEvent::Jump { action, vy });
            }
            ACT_FLYING_TRIPLE_JUMP | ACT_TRIPLE_JUMP => {
                self.state.jump_combo = 3.0;
                self.events.push(MarioEvent::Triplejump);
            }
            ACT_BACKFLIP | ACT_SIDE_FLIP | ACT_TOP_OF_POLE_JUMP | ACT_WALL_KICK_AIR | ACT_JUMP_KICK | ACT_LONG_JUMP | ACT_DIVE => {
                if action == ACT_WALL_KICK_AIR {
                    self.state.wall_kicks += 1.0;
                    self.events.push(MarioEvent::Wallkick);
                } else if action == ACT_LONG_JUMP {
                    self.events.push(MarioEvent::Longjump);
                } else if action == ACT_DIVE {
                    self.events.push(MarioEvent::Dive);
                } else {
                    self.events.push(MarioEvent::Jump { action, vy });
                }
            }
            ACT_GROUND_POUND => self.events.push(MarioEvent::Groundpound),
            ACT_GROUND_POUND_LAND => self.events.push(MarioEvent::GroundpoundLand),
            _ => {}
        }
        true
    }

    fn set_mario_action_moving(&mut self, action: u32) -> u32 {
        let floor_class = mario_get_floor_class(&self.state);
        let forward_vel = self.forward_vel;
        let mag = self.intended_mag.min(8.0);

        match action {
            ACT_WALKING => {
                if floor_class != 0x13 && 0.0 <= forward_vel && forward_vel < mag { self.state.forward_vel = mag; }
            }
            ACT_HOLD_WALKING => {
                if 0.0 <= forward_vel && forward_vel < mag / 2.0 { self.state.forward_vel = mag / 2.0; }
            }
            ACT_HOLD_BEGIN_SLIDING => {
                return if facing_downhill_for_slide(&self.state) { ACT_HOLD_BUTT_SLIDE } else { ACT_HOLD_STOMACH_SLIDE };
            }
            ACT_BEGIN_SLIDING => {
                return if facing_downhill_for_slide(&self.state) { ACT_BUTT_SLIDE } else { ACT_STOMACH_SLIDE };
            }
            _ => {}
        }
        action
    }

    fn set_mario_action_airborne(&mut self, action: u32, _action_arg: f64) -> u32 {
        let mut action = action;
        if (self.squish_timer != 0.0 || self.quicksand_depth >= 1.0) && (action == ACT_DOUBLE_JUMP || action == 0x108008a4) {
            action = ACT_JUMP;
        }

        match action {
            ACT_DOUBLE_JUMP => {
                self.set_y_vel_based_on_fspeed(52.0, 0.25);
                self.state.forward_vel *= 0.8;
            }
            ACT_BACKFLIP => {
                self.state.forward_vel = -16.0;
                self.set_y_vel_based_on_fspeed(62.0, 0.0);
            }
            ACT_FLYING_TRIPLE_JUMP => self.set_y_vel_based_on_fspeed(82.0, 0.0),
            ACT_TRIPLE_JUMP => {
                self.set_y_vel_based_on_fspeed(69.0, 0.0);
                self.state.forward_vel *= 0.8;
            }
            ACT_RIDING_SHELL_JUMP => self.set_y_vel_based_on_fspeed(42.0, 0.25),
            ACT_JUMP | ACT_HOLD_JUMP => {
                self.set_y_vel_based_on_fspeed(42.0, 0.25);
                self.state.forward_vel *= 0.8;
            }
            ACT_TOP_OF_POLE_JUMP | ACT_WALL_KICK_AIR => {
                self.set_y_vel_based_on_fspeed(62.0, 0.0);
                if self.forward_vel < 24.0 { self.state.forward_vel = 24.0; }
                self.state.wall_kick_timer = 0.0;
            }
            ACT_SIDE_FLIP => {
                self.set_y_vel_based_on_fspeed(62.0, 0.0);
                self.state.forward_vel = 8.0;
                self.state.face_angle[1] = self.intended_yaw;
            }
            ACT_STEEP_JUMP => {
                self.set_y_vel_based_on_fspeed(42.0, 0.25);
                self.state.face_angle[0] = -0x2000 as f64;
            }
            ACT_DIVE => {
                let mut fv = self.forward_vel + 15.0;
                if fv > 48.0 { fv = 48.0; }
                mario_set_forward_vel(&mut self.state, fv);
            }
            ACT_LONG_JUMP => {
                self.long_jump_is_slow = self.forward_vel <= 16.0;
                self.set_y_vel_based_on_fspeed(30.0, 0.0);
                self.state.forward_vel *= 1.5;
                if self.forward_vel > 48.0 { self.state.forward_vel = 48.0; }
            }
            ACT_SLIDE_KICK => {
                self.state.vel[1] = 12.0;
                if self.forward_vel < 32.0 { self.state.forward_vel = 32.0; }
            }
            ACT_JUMP_KICK => self.state.vel[1] = 20.0,
            _ => {}
        }

        self.state.peak_height = self.pos[1];
        self.state.flags |= MARIO_UNKNOWN_08;
        action
    }

    fn set_y_vel_based_on_fspeed(&mut self, initial_vel_y: f64, multiplier: f64) {
        self.state.vel[1] = initial_vel_y + self.forward_vel * multiplier;
        if self.squish_timer != 0.0 || self.quicksand_depth > 1.0 { self.state.vel[1] *= 0.5; }
    }

    /// set_jump_from_landing
    pub fn set_jump_from_landing(&mut self) -> bool {
        if mario_floor_is_steep(&self.state) {
            self.set_steep_jump_action();
        } else if self.double_jump_timer == 0.0 || self.squish_timer != 0.0 {
            self.set_action(ACT_JUMP, 0.0);
        } else {
            match self.prev_action {
                0x04000470 | 0x04000471 | 0x0c000233 => { // ACT_JUMP_LAND, ACT_FREEFALL_LAND, ACT_SIDE_FLIP_LAND_STOP
                    self.set_action(ACT_DOUBLE_JUMP, 0.0);
                }
                0x04000472 => { // ACT_DOUBLE_JUMP_LAND
                    if self.flags & MARIO_WING_CAP != 0 {
                        self.set_action(ACT_FLYING_TRIPLE_JUMP, 0.0);
                    } else if self.forward_vel > 20.0 {
                        self.set_action(ACT_TRIPLE_JUMP, 0.0);
                    } else {
                        self.set_action(ACT_JUMP, 0.0);
                    }
                }
                _ => { self.set_action(ACT_JUMP, 0.0); }
            }
        }
        self.state.double_jump_timer = 0.0;
        true
    }

    /// set_jumping_action
    pub fn set_jumping_action(&mut self, action: u32, action_arg: f64) -> bool {
        if mario_floor_is_steep(&self.state) {
            self.set_steep_jump_action();
        } else {
            self.set_action(action, action_arg);
        }
        true
    }

    /// set_steep_jump_action
    pub fn set_steep_jump_action(&mut self) {
        if self.forward_vel > 0.0 {
            let angle_temp = to_s16(self.floor_angle + 0x8000 as f64) as f64;
            let face_angle_temp = to_s16(self.face_angle[1] - angle_temp) as f64;
            let y = js_sin(face_angle_temp / 32768.0 * std::f64::consts::PI) * self.forward_vel;
            let x = js_cos(face_angle_temp / 32768.0 * std::f64::consts::PI) * self.forward_vel * 0.75;
            self.state.forward_vel = (y * y + x * x).sqrt();
            self.state.face_angle[1] = to_s16(atan2s(x, y) as f64 + angle_temp) as f64;
        }
        self.set_action(ACT_STEEP_JUMP, 0.0);
    }

    /// drop_and_set_mario_action (the held object is dropped by `set_action` itself).
    pub fn drop_and_set_action(&mut self, action: u32, action_arg: f64) -> bool { self.set_action(action, action_arg) }

    /// check_common_action_exits
    pub fn check_common_action_exits(&mut self) -> bool {
        if self.input & INPUT_A_PRESSED != 0 { return self.set_action(ACT_JUMP, 0.0); }
        if self.input & INPUT_OFF_FLOOR != 0 { return self.set_action(ACT_FREEFALL, 0.0); }
        if self.input & INPUT_NONZERO_ANALOG != 0 { return self.set_action(ACT_WALKING, 0.0); }
        if self.input & INPUT_ABOVE_SLIDE != 0 { return self.set_action(ACT_BEGIN_SLIDING, 0.0); }
        false
    }

    /// mario_push_off_steep_floor
    pub fn push_off_steep_floor(&mut self, action: u32, action_arg: f64) -> bool {
        let floor_d_yaw = to_s16(self.floor_angle - self.face_angle[1]);
        if floor_d_yaw > -0x4000 && floor_d_yaw < 0x4000 {
            self.state.forward_vel = 16.0;
            self.state.face_angle[1] = self.floor_angle;
        } else {
            self.state.forward_vel = -16.0;
            self.state.face_angle[1] = to_s16(self.floor_angle + 0x8000 as f64) as f64;
        }
        self.set_action(action, action_arg)
    }

    /// mario_get_floor_class convenience used by actions.
    pub fn floor_class(&self) -> i32 { mario_get_floor_class(&self.state) }
    pub fn floor_is_steep(&self) -> bool { mario_floor_is_steep(&self.state) }
    pub fn floor_is_slippery(&self) -> bool { mario_floor_is_slippery(&self.state) }
    pub fn floor_height_relative_polar(&self, angle_from_mario: f64, dist: f64) -> f64 {
        find_floor_height_relative_polar(&self.state, &self.world, angle_from_mario, dist)
    }

    /// mario_bonk_reflection (with the bonk event).
    pub fn bonk_reflection(&mut self, negate_speed: bool) {
        let speed = self.forward_vel;
        if let Some(w) = self.wall.clone() {
            let wall_angle = atan2s(w.normal[2], w.normal[0]) as f64;
            self.state.face_angle[1] = to_s16(wall_angle - to_s16(self.face_angle[1] - wall_angle) as f64) as f64;
        }
        if negate_speed {
            let v = -self.forward_vel;
            mario_set_forward_vel(&mut self.state, v);
        } else {
            self.state.face_angle[1] = to_s16(self.face_angle[1] + 0x8000 as f64) as f64;
        }
        self.events.push(MarioEvent::Bonk { speed });
    }

    pub fn step_ground(&mut self) -> i32 { perform_ground_step(&mut self.state, &self.world) }

    pub fn step_air(&mut self, step_arg: u32) -> i32 { perform_air_step(&mut self.state, &self.world, step_arg) }

    pub fn land_event(&mut self, vy: f64, speed: f64) {
        self.state.lands += 1.0;
        self.events.push(MarioEvent::Land { speed, vy });
    }

    pub fn air_step_arg(&self) -> u32 { AIR_CHECK_LEDGE_GRAB }

    /// Surface Mario stands on, if any (convenience for renderers/tests).
    pub fn floor_surface(&self) -> Option<&Arc<Surface>> { self.state.floor.as_ref() }
}

/// Returns true when the cap just ran out (`timer == 1`), matching `tickCap`'s `expire`.
fn tick_cap(flags: &mut u32, timer: f64, flag: u32) -> bool {
    if timer > 0.0 {
        *flags |= flag;
        timer == 1.0
    } else {
        *flags &= !flag;
        false
    }
}

/// Local copy of mario_facing_downhill for the BEGIN_SLIDING transition.
fn facing_downhill_for_slide(m: &MarioState) -> bool {
    let face_angle_yaw = to_s16(m.floor_angle - m.face_angle[1]);
    -0x4000 < face_angle_yaw && face_angle_yaw < 0x4000
}
