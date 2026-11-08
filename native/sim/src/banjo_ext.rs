//! Banjo-Kazooie's extended moveset on top of the SM64 engine (see `native/BANJO_CONTROLLER.md`).
//!
//! `characters::step_banjo` keeps the original nine moves (jump, Feathery Flap, Flap Flip, Claw,
//! Roll, Beak Barge, Rat-a-tat Rap ...). This module adds the rest of the `bs*` state machine from
//! `src/core2/bs/*.c` of the decomp as extra `action_arg` codes on top of SM64 actions:
//!
//! | move | decomp | SM64 action + arg |
//! |------|--------|-------------------|
//! | Talon Trot | `bTrot.c` | `ACT_WALKING` / `ACT_JUMP` + `BANJO_TROT` / `BANJO_TROT_JUMP` |
//! | Beak Buster | `bbuster.c` | `ACT_GROUND_POUND`, `ACT_GROUND_POUND_LAND`, `ACT_FREEFALL` + `BANJO_BUSTER*` |
//! | Shock Spring Jump | `bShock.c` | `ACT_IDLE` / `ACT_JUMP` + `BANJO_SHOCK*` |
//! | Blue egg (forward / backward) | `bEggHead.c`, `bEggAss.c`, `code_CC1E0.c` | `ACT_IDLE` + `BANJO_EGG_HEAD/ASS` |
//! | Wonderwing | `bWhirl.c` | `ACT_WALKING` / `ACT_JUMP` + `BANJO_WONDER*` |
//! | Flight, Beak Bomb | `bFly.c` | `ACT_FREEFALL` / `ACT_DIVE` + `BANJO_FLY*`, `BANJO_BOMB` |
//!
//! Units follow `characters.rs`: Rare units per second are converted to 30 Hz frames at Banjo's
//! 1.25 model scale (`speed()` / `accel()`). Inputs: the C buttons and R are real N64 masks
//! (`state_consts`); the host sets them from keys, touch buttons and the D-pad.
use crate::characters::*;
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

pub const BANJO_TROT: f64 = 0x67 as f64;
pub const BANJO_TROT_JUMP: f64 = 0x68 as f64;
pub const BANJO_BUSTER: f64 = 0x69 as f64;
pub const BANJO_BUSTER_HOP: f64 = 0x6a as f64;
pub const BANJO_SHOCK_CHARGE: f64 = 0x6b as f64;
pub const BANJO_SHOCK_JUMP: f64 = 0x6c as f64;
pub const BANJO_EGG_HEAD: f64 = 0x6d as f64;
pub const BANJO_EGG_ASS: f64 = 0x6e as f64;
pub const BANJO_WONDER: f64 = 0x6f as f64;
pub const BANJO_WONDER_JUMP: f64 = 0x70 as f64;
pub const BANJO_FLY_ENTER: f64 = 0x71 as f64;
pub const BANJO_FLY: f64 = 0x72 as f64;
pub const BANJO_BOMB: f64 = 0x73 as f64;
pub const BANJO_BOMB_END: f64 = 0x74 as f64;
pub const BANJO_LAST: f64 = BANJO_BOMB_END;

// bTrot.c .data
const TROT_MIN: f64 = speed(30.0);
const TROT_MAX: f64 = speed(700.0);
const TURBO_MIN: f64 = speed(80.0);
const TURBO_MAX: f64 = speed(1000.0);
const TROT_JUMP_VELOCITY: f64 = speed(693.5);
const TROT_GRAVITY: f64 = accel(1200.0);
// bWhirl.c
const WONDER_MIN: f64 = speed(80.0);
const WONDER_MAX: f64 = speed(425.0);
const WONDER_JUMP_VELOCITY: f64 = speed(693.5);
const WONDER_GRAVITY: f64 = accel(1200.0);
/// `func_802AA58C`: one gold feather every 2.0 s.
const WONDER_FEATHER_FRAMES: f64 = 60.0;
// bbuster.c
const BUSTER_WINDUP: f64 = 11.0; // anim 0.35 of 1.02 s = 0.357 s
const BUSTER_DIVE: f64 = speed(2300.0);
const BUSTER_DIVE_TERMINAL: f64 = -150.0; // decomp: -5000 u/s; capped so a 30 Hz quarter step cannot tunnel
const BUSTER_GRAVITY: f64 = accel(20000.0);
const BUSTER_FREEZE: f64 = 3.0; // 0.09 s on impact
const BUSTER_HOP: f64 = speed(730.0);
const BUSTER_HOP_GRAVITY: f64 = accel(2110.0);
// bShock.c
const SHOCK_VELOCITY: f64 = speed(1250.0);
const SHOCK_GRAVITY: f64 = accel(1200.0);
/// Anim 0.3637 of the 2.8 s charge: the spring only fires if A is still held then.
const SHOCK_CHARGE_FRAMES: f64 = 31.0;
// bEggHead.c / bEggAss.c / code_CC1E0.c
const EGG_SPEED: f64 = speed(800.0);
const EGG_LIFETIME: f64 = 60.0; // fxegg_head_update: 2.0 s
const EGG_FIRE_FRAME: f64 = 14.0; // anim 0.4704 of 1.0 s
const EGG_REPEAT_FRAMES: f64 = 6.0; // 0.3878 -> 0.5919 loop
const EGG_END_FRAME: f64 = 30.0;
const EGG_TURN: f64 = 0x600 as f64; // yaw_setVelocityBounded(350 deg/s)
const EGG_ASS_BACK: f64 = speed(200.0);
const EGG_ASS_UP: f64 = speed(710.0);
const EGG_ASS_GRAVITY: f64 = accel(2200.0);
const EGG_ASS_LIFETIME: f64 = 120.0;
pub const EGG_MAX: i32 = 100;
// bFly.c
const FLY_LIFT: f64 = speed(1600.0);
const FLY_ENTER_GRAVITY: f64 = accel(1200.0);
const FLY_LIFT_FRAME: f64 = 10.0; // anim 0.2416 of 1.4 s ~ 0.34 s
const FLY_SPEED: f64 = speed(600.0);
const FLY_GRAVITY: f64 = accel(300.0);
const FLY_FLAP_VELOCITY: f64 = speed(400.0);
const FLY_FLAP_FRAMES: f64 = 10.0; // baflap_add(1.0)
const FLY_YAW: f64 = 3.0 * 182.04; // yaw_range 3 deg per update (6 with R)
const FLY_PITCH_MAX: f64 = 80.0;
const BOMB_WINDUP: f64 = 21.0; // anim 0.6905 of 1.0 s
const BOMB_SPEED: f64 = speed(4200.0);
const BOMB_RANGE: f64 = 4000.0; // 16,000,000 squared distance
const BOMB_END_FRAMES: f64 = 11.0; // 0.35 s
const BOMB_COOLDOWN: f64 = 30.0; // D_8037D320 = 1.0 s after taking flight

/// The ability pads of BK, selectable in this port (no pads exist in the SM64 courses): each call of
/// [`BanjoExt::cycle_power`] equips the next one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Power {
    #[default]
    None,
    /// Shock Spring pad: A on the ground charges a 650-unit jump.
    Shock,
    /// Flight pad: A on the ground takes off; A flaps (red feathers), B Beak Bombs.
    Flight,
    /// Turbo Trainers: Talon Trot at 1000 u/s that no longer needs Z held, for 30 s.
    Turbo,
    /// Wading Boots: shorter hops (the mud/lava immunity is not ported).
    Wading,
}

impl Power {
    pub fn next(self) -> Self {
        match self {
            Power::None => Power::Shock,
            Power::Shock => Power::Flight,
            Power::Flight => Power::Turbo,
            Power::Turbo => Power::Wading,
            Power::Wading => Power::None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Power::None => "NO PAD",
            Power::Shock => "SHOCK SPRING",
            Power::Flight => "FLIGHT",
            Power::Turbo => "TURBO TRAINERS",
            Power::Wading => "WADING BOOTS",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EggKind {
    /// `fxegg_head`: shot forward at 800 u/s, shatters on contact or after 2 s.
    Head,
    /// `fxegg_ass`: dropped backwards, lobbed up; can be picked up again.
    Ass,
}

#[derive(Clone, Debug)]
pub struct Egg {
    pub kind: EggKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind {
    Note,
    Egg,
    RedFeather,
    GoldFeather,
    Honeycomb,
    Jiggy,
}

impl PickupKind {
    pub fn label(self) -> &'static str {
        match self {
            PickupKind::Note => "NOTE",
            PickupKind::Egg => "EGGS",
            PickupKind::RedFeather => "RED FEATHERS",
            PickupKind::GoldFeather => "GOLD FEATHER",
            PickupKind::Honeycomb => "HONEYCOMB",
            PickupKind::Jiggy => "JIGGY",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BkPickup {
    pub kind: PickupKind,
    pub pos: Vec3,
    pub collected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BanjoEvent {
    EggFired { pos: Vec3, backwards: bool },
    EggShattered { pos: Vec3, hit: bool },
    Collected(PickupKind),
    BeakBuster { pos: Vec3 },
    ShockSpring,
    TakeOff,
    BeakBomb,
    WonderwingOn,
    WonderwingOff,
    OutOf(PickupKind),
}

/// Everything BK-specific that the SM64 `MarioState` cannot hold.
#[derive(Clone, Debug)]
pub struct BanjoExt {
    // Item counters (`ITEM_*` of the decomp). Starting loadout is generous: every course is a sandbox.
    pub eggs: i32,
    pub red_feathers: i32,
    pub gold_feathers: i32,
    pub notes: i32,
    pub jiggies: i32,
    pub power: Power,
    /// Frames of Turbo Trainers left (`STATE_TIMER_3_TURBO_TALON`).
    pub turbo_frames: f64,
    pub eggs_in_flight: Vec<Egg>,
    pub pickups: Vec<BkPickup>,
    pub events: Vec<BanjoEvent>,
    // Move scratch state.
    /// The trot/wonderwing was entered without Z held (touch): stays on until Z or the button is pressed again.
    pub latched: bool,
    pub queued_eggs: i32,
    pub fly_pitch: f64,
    pub fly_flap: f64,
    pub fly_cooldown: f64,
    pub bomb_start: Vec3,
    pub bomb_dir: Vec3,
    pub wonder_timer: f64,
    pub shock_armed: bool,
    pub buster_stage: f64,
}

impl Default for BanjoExt {
    fn default() -> Self {
        Self {
            eggs: 50,
            red_feathers: 50,
            gold_feathers: 10,
            notes: 0,
            jiggies: 0,
            power: Power::None,
            turbo_frames: 0.0,
            eggs_in_flight: vec![],
            pickups: vec![],
            events: vec![],
            latched: false,
            queued_eggs: 0,
            fly_pitch: 0.0,
            fly_flap: 0.0,
            fly_cooldown: 0.0,
            bomb_start: [0.0; 3],
            bomb_dir: [0.0, 0.0, 1.0],
            wonder_timer: 0.0,
            shock_armed: false,
            buster_stage: 0.0,
        }
    }
}

impl BanjoExt {
    pub fn on_respawn(&mut self) {
        self.eggs_in_flight.clear();
        self.latched = false;
        self.queued_eggs = 0;
        self.turbo_frames = 0.0;
    }

    pub fn cycle_power(&mut self) {
        self.power = self.power.next();
        if self.power == Power::Turbo {
            self.turbo_frames = 30.0 * 30.0;
        } else {
            self.turbo_frames = 0.0;
        }
    }

    pub fn drop_pickup(&mut self, kind: PickupKind, pos: Vec3) {
        self.pickups.push(BkPickup { kind, pos, collected: false });
    }

    fn take(&mut self, kind: PickupKind) {
        match kind {
            PickupKind::Note => self.notes += 1,
            PickupKind::Egg => self.eggs = (self.eggs + 5).min(EGG_MAX),
            PickupKind::RedFeather => self.red_feathers = (self.red_feathers + 5).min(100),
            PickupKind::GoldFeather => self.gold_feathers = (self.gold_feathers + 1).min(10),
            PickupKind::Jiggy => self.jiggies += 1,
            PickupKind::Honeycomb => {}
        }
        self.events.push(BanjoEvent::Collected(kind));
    }
}

pub fn wonderwing_active(m: &MarioRuntime) -> bool { m.action_arg == BANJO_WONDER || m.action_arg == BANJO_WONDER_JUMP }
pub fn talon_trotting(m: &MarioRuntime) -> bool { m.action_arg == BANJO_TROT || m.action_arg == BANJO_TROT_JUMP }
pub fn flying(m: &MarioRuntime) -> bool { [BANJO_FLY_ENTER, BANJO_FLY, BANJO_BOMB, BANJO_BOMB_END].contains(&m.action_arg) }

fn pressed(m: &MarioRuntime, mask: u32) -> bool { m.controller.button_pressed & mask != 0 }
fn down(m: &MarioRuntime, mask: u32) -> bool { m.controller.button_down & mask != 0 }
fn lerp(t: f64, a: f64, b: f64) -> f64 { a + (b - a) * t.clamp(0.0, 1.0) }
fn forward_dir(m: &MarioRuntime) -> Vec3 { [sins(m.face_angle[1]), 0.0, coss(m.face_angle[1])] }

/// `bastick_getZonePosition` with the zone (0.03..1.0) the trot and wonderwing states set.
fn zone_position(m: &MarioRuntime) -> f64 {
    let frac = (m.intended_mag / 32.0).clamp(0.0, 1.0);
    if frac <= 0.03 { 0.0 } else { (frac - 0.03) / 0.97 }
}

fn turn_toward_stick(m: &mut MarioRuntime, rate: f64) {
    if m.intended_mag > 0.0 {
        let turn = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        m.face_angle[1] = to_s16(m.face_angle[1] + clamp(turn, -rate, rate)) as f64;
    }
}

/// Per-frame bookkeeping that runs whatever Banjo is doing: egg projectiles, pickups, timers.
pub fn tick(m: &mut MarioRuntime) {
    if m.banjo.turbo_frames > 0.0 {
        m.banjo.turbo_frames -= 1.0;
        if m.banjo.turbo_frames <= 0.0 && m.banjo.power == Power::Turbo { m.banjo.power = Power::None; }
    }
    if m.banjo.fly_cooldown > 0.0 { m.banjo.fly_cooldown -= 1.0; }
    if pressed(m, R_TRIG) && !flying(m) {
        m.banjo.cycle_power();
    }
    step_eggs(m);
    collect_pickups(m);
    if wonderwing_active(m) {
        m.invinc_timer = m.invinc_timer.max(2.0);
    }
}

fn collect_pickups(m: &mut MarioRuntime) {
    let pos = m.pos;
    let mut taken = vec![];
    for (i, p) in m.banjo.pickups.iter().enumerate() {
        if p.collected { continue; }
        let dx = p.pos[0] - pos[0];
        let dz = p.pos[2] - pos[2];
        let dy = p.pos[1] - pos[1];
        if dx * dx + dz * dz < 90.0 * 90.0 && dy > -60.0 && dy < 170.0 {
            taken.push(i);
        }
    }
    for i in taken {
        let kind = m.banjo.pickups[i].kind;
        m.banjo.pickups[i].collected = true;
        if kind == PickupKind::Honeycomb {
            m.heal_counter += 4.0;
        }
        m.banjo.take(kind);
    }
    m.banjo.pickups.retain(|p| !p.collected);
}

// ---------------------------------------------------------------------------------------------
// Eggs
// ---------------------------------------------------------------------------------------------

fn shatter(m: &mut MarioRuntime, pos: Vec3, hit: bool) {
    m.banjo.events.push(BanjoEvent::EggShattered { pos, hit });
}

fn step_eggs(m: &mut MarioRuntime) {
    if m.banjo.eggs_in_flight.is_empty() { return; }
    let mut eggs = std::mem::take(&mut m.banjo.eggs_in_flight);
    let mario = m.pos;
    eggs.retain_mut(|egg| {
        egg.age += 1.0;
        let before = egg.pos;
        if egg.kind == EggKind::Ass {
            egg.vel[1] = (egg.vel[1] - EGG_ASS_GRAVITY).max(-80.0);
        }
        for k in 0..3 { egg.pos[k] += egg.vel[k]; }
        // Pickup: a dropped egg that landed can be walked over again (chCollectible_collectEgg).
        if egg.kind == EggKind::Ass && egg.age > 12.0 {
            let (dx, dz) = (egg.pos[0] - mario[0], egg.pos[2] - mario[2]);
            if dx * dx + dz * dz < 80.0 * 80.0 && (egg.pos[1] - mario[1]).abs() < 140.0 {
                m.banjo.eggs = (m.banjo.eggs + 1).min(EGG_MAX);
                m.banjo.events.push(BanjoEvent::Collected(PickupKind::Egg));
                return false;
            }
        }
        // Walls.
        let mut probe = egg.pos;
        let walls = m.world.f32_find_wall_collision(&mut probe, 0.0, 20.0);
        if walls > 0 && egg.kind == EggKind::Head {
            let at = before;
            shatter(m, at, false);
            return false;
        }
        // Floor.
        let floor = m.world.find_floor(egg.pos[0], egg.pos[1] + 20.0, egg.pos[2]);
        if floor.surface.is_some() && egg.pos[1] <= floor.height + 12.0 {
            if egg.kind == EggKind::Head {
                let at = [egg.pos[0], floor.height + 12.0, egg.pos[2]];
                shatter(m, at, false);
                return false;
            }
            egg.pos[1] = floor.height + 12.0;
            egg.vel = [egg.vel[0] * 0.5, 0.0, egg.vel[2] * 0.5];
        }
        // Objects.
        if egg.kind == EggKind::Head {
            for i in 0..m.objects.len() {
                let id = ObjId(i as u32);
                let o = &m.objects[id];
                if !o.active || o.held_state == HELD_HELD { continue; }
                let t = o.hitbox.interact_type;
                if t & (INTERACT_DAMAGE | INTERACT_BREAKABLE | INTERACT_GRABBABLE) == 0 { continue; }
                let egg_box = ObjectHitbox { interact_type: 0, down_offset: 20.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0, radius: 20.0, height: 40.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 };
                if hitboxes_overlap((&egg.pos, &egg_box), (&o.pos, &o.hitbox)) {
                    let o = &mut m.objects[id];
                    attack_object(o, ATTACK_PUNCH);
                    if t & INTERACT_GRABBABLE != 0 { o.interact_status |= INT_STATUS_MARIO_KNOCKBACK_DMG; }
                    let at = egg.pos;
                    shatter(m, at, true);
                    return false;
                }
            }
        }
        let life = if egg.kind == EggKind::Head { EGG_LIFETIME } else { EGG_ASS_LIFETIME };
        if egg.age >= life {
            let at = egg.pos;
            shatter(m, at, false);
            return false;
        }
        true
    });
    // Eggs spawned by shatter() callbacks cannot occur; keep ones fired this frame.
    eggs.append(&mut m.banjo.eggs_in_flight);
    m.banjo.eggs_in_flight = eggs;
}

fn fire_egg(m: &mut MarioRuntime, backwards: bool) {
    if m.banjo.eggs <= 0 {
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::Egg));
        return;
    }
    m.banjo.eggs -= 1;
    let dir = forward_dir(m);
    if !backwards {
        // fxegg_head_spawn: player + (0, 80, 0) + 70 along the facing.
        let pos = [m.pos[0] + dir[0] * 70.0, m.pos[1] + 80.0, m.pos[2] + dir[2] * 70.0];
        let vel = [dir[0] * EGG_SPEED, 0.0, dir[2] * EGG_SPEED];
        m.banjo.eggs_in_flight.push(Egg { kind: EggKind::Head, pos, vel, age: 0.0 });
    } else {
        // fxegg_ass_spawn: dropped 18 behind at +60, thrown back at 200 u/s and up at ~700 u/s.
        let pos = [m.pos[0] - dir[0] * 18.0, m.pos[1] + 60.0, m.pos[2] - dir[2] * 18.0];
        let vel = [-dir[0] * EGG_ASS_BACK, EGG_ASS_UP, -dir[2] * EGG_ASS_BACK];
        m.banjo.eggs_in_flight.push(Egg { kind: EggKind::Ass, pos, vel, age: 0.0 });
    }
    m.banjo.events.push(BanjoEvent::EggFired { pos: m.pos, backwards });
}

fn step_egg_pose(m: &mut MarioRuntime, backwards: bool) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    // bsegghead_update: more presses of the button queue up to three eggs.
    if pressed(m, if backwards { C_DOWN } else { C_UP }) && m.banjo.queued_eggs < 3 && t > 2.0 {
        m.banjo.queued_eggs += 1;
    }
    if !backwards { turn_toward_stick(m, EGG_TURN); }
    mario_set_forward_vel(m, 0.0);
    let fire_at = if backwards { 8.0 } else { EGG_FIRE_FRAME };
    if t == fire_at || (t > fire_at && m.banjo.queued_eggs > 1 && (t - fire_at) % EGG_REPEAT_FRAMES == 0.0 && (t - fire_at) / EGG_REPEAT_FRAMES < m.banjo.queued_eggs as f64) {
        fire_egg(m, backwards);
    }
    // After 0.65 of the animation other moves may interrupt (func_802ADCD4).
    if t >= EGG_END_FRAME * 0.65 && pressed(m, A_BUTTON) {
        start_jump(m);
        return true;
    }
    if t >= EGG_END_FRAME * 0.65 && pressed(m, C_LEFT | C_RIGHT) {
        m.set_action(ACT_IDLE, 0.0);
        return start_moves(m).unwrap_or(true);
    }
    let _ = m.step_ground();
    if m.input & INPUT_OFF_FLOOR != 0 {
        m.set_action(ACT_FREEFALL, 0.0);
        return true;
    }
    if t + 1.0 >= EGG_END_FRAME {
        m.set_action(if down(m, Z_TRIG) { ACT_CROUCHING } else { ACT_IDLE }, 0.0);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------------------------

fn start_jump(m: &mut MarioRuntime) {
    if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
    start(m, ACT_JUMP, BANJO_JUMP);
    let wading = m.banjo.power == Power::Wading;
    m.vel[1] = if wading { speed(400.0) } else { JUMP_VELOCITY };
    m.jump_takeoff_speed = MAX_WALK.max(m.forward_vel.abs());
}

/// The extended moves. `Some(consumed)` replaces the rest of `step_banjo` for this frame.
pub fn step(m: &mut MarioRuntime) -> Option<bool> {
    let mv = banjo_move(m);
    if mv >= BANJO_TROT {
        let a = m.action;
        let consumed = if mv == BANJO_TROT && a == ACT_WALKING {
            Some(step_trot_ground(m))
        } else if mv == BANJO_TROT_JUMP && a & ACT_FLAG_AIR != 0 {
            Some(step_trot_air(m))
        } else if mv == BANJO_BUSTER && a == ACT_GROUND_POUND {
            Some(step_buster_dive(m))
        } else if mv == BANJO_BUSTER && a == ACT_GROUND_POUND_LAND {
            Some(step_buster_impact(m))
        } else if mv == BANJO_BUSTER_HOP && a & ACT_FLAG_AIR != 0 {
            Some(step_buster_hop(m))
        } else if mv == BANJO_SHOCK_CHARGE && a == ACT_IDLE {
            Some(step_shock_charge(m))
        } else if mv == BANJO_SHOCK_JUMP && a & ACT_FLAG_AIR != 0 {
            Some(step_shock_jump(m))
        } else if mv == BANJO_EGG_HEAD && a == ACT_IDLE {
            Some(step_egg_pose(m, false))
        } else if mv == BANJO_EGG_ASS && a == ACT_IDLE {
            Some(step_egg_pose(m, true))
        } else if mv == BANJO_WONDER && a == ACT_WALKING {
            Some(step_wonder_ground(m))
        } else if mv == BANJO_WONDER_JUMP && a & ACT_FLAG_AIR != 0 {
            Some(step_wonder_air(m))
        } else if (mv == BANJO_FLY_ENTER || mv == BANJO_FLY) && a & ACT_FLAG_AIR != 0 {
            Some(step_fly(m, mv == BANJO_FLY_ENTER))
        } else if (mv == BANJO_BOMB || mv == BANJO_BOMB_END) && a & ACT_FLAG_AIR != 0 {
            Some(step_bomb(m))
        } else {
            None
        };
        if consumed.is_some() { return consumed; }
    }
    start_moves(m)
}

fn start_moves(m: &mut MarioRuntime) -> Option<bool> {
    // Airborne: Z = Beak Buster (bainput_should_beak_bust), from any jump or flap.
    if m.action & ACT_FLAG_AIR != 0 {
        if pressed(m, Z_TRIG) && FLAP_FROM.contains(&m.action) && banjo_move(m) != BANJO_BUSTER {
            return Some(start_buster(m));
        }
        return None;
    }
    if !on_basic_ground(m) && !is_crouched(m.action) { return None; }
    let crouched = is_crouched(m.action);
    let shoulder = m.controller.button_pressed;
    // Eggs: Z + C-up / C-down in BK. C-up alone is first-person there, so the port accepts it alone.
    if shoulder & C_UP != 0 && m.banjo.eggs > 0 {
        return Some(start_egg(m, false));
    }
    if shoulder & C_DOWN != 0 && m.banjo.eggs > 0 {
        return Some(start_egg(m, true));
    }
    if shoulder & C_UP != 0 || shoulder & C_DOWN != 0 {
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::Egg));
    }
    if shoulder & C_LEFT != 0 {
        return Some(start_trot(m));
    }
    if shoulder & C_RIGHT != 0 {
        if m.banjo.gold_feathers > 0 {
            return Some(start_wonder(m));
        }
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::GoldFeather));
    }
    if pressed(m, A_BUTTON) && !crouched {
        match m.banjo.power {
            Power::Shock => return Some(start_shock(m)),
            Power::Flight => return Some(start_fly(m)),
            _ => {}
        }
    }
    if m.banjo.turbo_frames > 0.0 && m.banjo.power == Power::Turbo && crouched {
        return Some(start_trot(m));
    }
    None
}

fn start_egg(m: &mut MarioRuntime, backwards: bool) -> bool {
    mario_set_forward_vel(m, 0.0);
    start(m, ACT_IDLE, if backwards { BANJO_EGG_ASS } else { BANJO_EGG_HEAD });
    m.banjo.queued_eggs = 1;
    step_egg_pose(m, backwards)
}

// -- Talon Trot --------------------------------------------------------------------------------

fn start_trot(m: &mut MarioRuntime) -> bool {
    m.banjo.latched = !down(m, Z_TRIG);
    start(m, ACT_WALKING, BANJO_TROT);
    // The entry frame only switches state (the same press must not end a latched trot).
    true
}

fn trot_should_end(m: &MarioRuntime) -> bool {
    if m.banjo.turbo_frames > 0.0 { return false; }
    if m.banjo.latched { pressed(m, Z_TRIG) || pressed(m, C_LEFT) } else { !down(m, Z_TRIG) }
}

fn trot_limits(m: &MarioRuntime) -> (f64, f64) {
    if m.banjo.turbo_frames > 0.0 { (TURBO_MIN, TURBO_MAX) } else { (TROT_MIN, TROT_MAX) }
}

fn step_trot_ground(m: &mut MarioRuntime) -> bool {
    if trot_should_end(m) {
        m.banjo.latched = false;
        m.set_action(if m.forward_vel.abs() > 3.0 { ACT_WALKING } else { ACT_IDLE }, 0.0);
        return false;
    }
    if pressed(m, A_BUTTON) {
        return start_trot_jump(m);
    }
    let (lo, hi) = trot_limits(m);
    let target = if m.intended_mag > 0.0 { lerp(zone_position(m), lo, hi) } else { 0.0 };
    turn_toward_stick(m, 0x500 as f64);
    let fwd = approach_f32(m.forward_vel, target, 2.0, 3.0);
    if m.input & INPUT_OFF_FLOOR != 0 {
        start(m, ACT_FREEFALL, BANJO_TROT_JUMP);
        m.vel[1] = 0.0;
        return step_trot_air(m);
    }
    mario_set_forward_vel(m, fwd);
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        start(m, ACT_FREEFALL, BANJO_TROT_JUMP);
        m.vel[1] = 0.0;
    } else if hit == GROUND_STEP_HIT_WALL as i32 {
        mario_set_forward_vel(m, 0.0);
    }
    true
}

fn start_trot_jump(m: &mut MarioRuntime) -> bool {
    start(m, ACT_JUMP, BANJO_TROT_JUMP);
    m.vel[1] = TROT_JUMP_VELOCITY;
    step_trot_air(m)
}

fn step_trot_air(m: &mut MarioRuntime) -> bool {
    let (_, hi) = trot_limits(m);
    air_control(m, hi, 1.0);
    m.action_timer += 1.0;
    let held = down(m, A_BUTTON) && m.vel[1] > 0.0;
    if pressed(m, Z_TRIG) && !m.banjo.latched && trot_should_end(m) {
        // Z released mid-air ends the trot after landing; nothing to do now.
    }
    let landed = air_step(m, if held { TROT_GRAVITY } else { GRAVITY }, TERMINAL);
    if landed && m.action & ACT_FLAG_AIR == 0 && m.action != ACT_LEDGE_GRAB {
        if trot_should_end(m) {
            m.banjo.latched = false;
            m.set_action(ACT_IDLE, 0.0);
        } else {
            start(m, ACT_WALKING, BANJO_TROT);
        }
    }
    true
}

// -- Beak Buster -------------------------------------------------------------------------------

fn start_buster(m: &mut MarioRuntime) -> bool {
    start(m, ACT_GROUND_POUND, BANJO_BUSTER);
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.action_timer = 0.0;
    m.banjo.buster_stage = 0.0;
    step_buster_dive(m)
}

fn step_buster_dive(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if t < BUSTER_WINDUP {
        // bsbbuster_init: gravity 0, velocity cleared while Banjo tucks his beak.
        m.vel = [0.0; 3];
        m.forward_vel = 0.0;
        return true;
    }
    if m.banjo.buster_stage == 0.0 {
        m.banjo.buster_stage = 1.0;
        m.vel[1] = -BUSTER_DIVE;
    }
    m.vel[0] = 0.0;
    m.vel[2] = 0.0;
    m.vel[1] = (m.vel[1] - BUSTER_GRAVITY).max(BUSTER_DIVE_TERMINAL);
    let vy = m.vel[1];
    let hit = m.step_air(0);
    if hit == AIR_STEP_LANDED as i32 {
        m.land_event(vy, 0.0);
        let at = m.pos;
        m.banjo.events.push(BanjoEvent::BeakBuster { pos: at });
        m.events.push(MarioEvent::GroundpoundLand);
        // Hand the frame to the impact state; its first frame carries the ground-pound hitbox.
        m.set_action(ACT_GROUND_POUND_LAND, BANJO_BUSTER);
        m.vel = [0.0; 3];
        m.action_timer = 0.0;
        return true;
    }
    if hit != AIR_STEP_NONE as i32 { m.vel[1] = vy; }
    true
}

fn step_buster_impact(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    m.vel = [0.0; 3];
    mario_set_forward_vel(m, 0.0);
    if m.action_timer >= BUSTER_FREEZE {
        // The rebound hop (D_80364998 / D_8036499C).
        start(m, ACT_FREEFALL, BANJO_BUSTER_HOP);
        m.vel = [0.0, BUSTER_HOP, 0.0];
        m.forward_vel = 0.0;
    }
    true
}

fn step_buster_hop(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    air_control(m, speed(120.0), 1.0);
    air_step(m, BUSTER_HOP_GRAVITY, TERMINAL);
    true
}

// -- Shock Spring Jump -------------------------------------------------------------------------

fn start_shock(m: &mut MarioRuntime) -> bool {
    mario_set_forward_vel(m, 0.0);
    start(m, ACT_IDLE, BANJO_SHOCK_CHARGE);
    m.banjo.shock_armed = true;
    step_shock_charge(m)
}

fn step_shock_charge(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    mario_set_forward_vel(m, 0.0);
    let _ = m.step_ground();
    // Anim < 0.3637 and A released cancels the jump (D_8037D380 = 0).
    if !down(m, A_BUTTON) && m.action_timer < SHOCK_CHARGE_FRAMES {
        m.banjo.shock_armed = false;
    }
    if m.action_timer >= SHOCK_CHARGE_FRAMES {
        if m.banjo.shock_armed {
            if m.intended_mag > 0.0 { m.face_angle[1] = m.intended_yaw; }
            start(m, ACT_JUMP, BANJO_SHOCK_JUMP);
            m.vel[1] = SHOCK_VELOCITY;
            m.jump_takeoff_speed = MAX_WALK;
            m.banjo.events.push(BanjoEvent::ShockSpring);
            m.events.push(MarioEvent::Jump { action: ACT_JUMP, vy: SHOCK_VELOCITY });
        } else {
            m.set_action(ACT_IDLE, 0.0);
        }
    }
    true
}

fn step_shock_jump(m: &mut MarioRuntime) -> bool {
    if pressed(m, Z_TRIG) { return start_buster(m); }
    if pressed(m, A_BUTTON) && m.vel[1] < 12.0 && m.banjo_flap == 0.0 {
        start(m, ACT_FREEFALL, BANJO_FLAP);
        m.banjo_flap = 75.0;
        m.vel[1] = 0.0;
        return false;
    }
    air_control(m, MAX_WALK, 1.5);
    m.action_timer += 1.0;
    let held = down(m, A_BUTTON) && m.vel[1] > 0.0;
    air_step(m, if held { SHOCK_GRAVITY } else { GRAVITY }, TERMINAL);
    true
}

// -- Wonderwing --------------------------------------------------------------------------------

fn start_wonder(m: &mut MarioRuntime) -> bool {
    m.banjo.latched = !down(m, Z_TRIG);
    m.banjo.wonder_timer = 0.0;
    start(m, ACT_WALKING, BANJO_WONDER);
    m.invinc_timer = m.invinc_timer.max(2.0);
    m.banjo.events.push(BanjoEvent::WonderwingOn);
    true
}

fn wonder_should_end(m: &mut MarioRuntime) -> bool {
    // func_802AA58C: a gold feather every 2 seconds, out of feathers = exit.
    m.banjo.wonder_timer += 1.0;
    if m.banjo.wonder_timer >= WONDER_FEATHER_FRAMES {
        m.banjo.wonder_timer = 0.0;
        if m.banjo.gold_feathers <= 0 { return true; }
        m.banjo.gold_feathers -= 1;
    }
    if m.banjo.gold_feathers <= 0 && m.banjo.wonder_timer == 0.0 { return true; }
    if m.banjo.latched { pressed(m, Z_TRIG) || pressed(m, C_RIGHT) } else { !down(m, Z_TRIG) }
}

fn end_wonder(m: &mut MarioRuntime) {
    m.banjo.latched = false;
    m.banjo.events.push(BanjoEvent::WonderwingOff);
    m.set_action(if m.action & ACT_FLAG_AIR != 0 { ACT_FREEFALL } else { ACT_IDLE }, 0.0);
}

fn step_wonder_ground(m: &mut MarioRuntime) -> bool {
    if wonder_should_end(m) {
        end_wonder(m);
        return false;
    }
    if pressed(m, A_BUTTON) {
        start(m, ACT_JUMP, BANJO_WONDER_JUMP);
        m.vel[1] = WONDER_JUMP_VELOCITY;
        return step_wonder_air(m);
    }
    let target = if m.intended_mag > 0.0 { lerp(zone_position(m), WONDER_MIN, WONDER_MAX) } else { 0.0 };
    turn_toward_stick(m, 0x500 as f64);
    let fwd = approach_f32(m.forward_vel, target, 2.0, 3.0);
    mario_set_forward_vel(m, fwd);
    let hit = m.step_ground();
    if hit == GROUND_STEP_LEFT_GROUND as i32 {
        start(m, ACT_FREEFALL, BANJO_WONDER_JUMP);
        m.vel[1] = 0.0;
    } else if hit == GROUND_STEP_HIT_WALL as i32 {
        mario_set_forward_vel(m, 0.0);
    }
    true
}

fn step_wonder_air(m: &mut MarioRuntime) -> bool {
    if wonder_should_end(m) {
        end_wonder(m);
        return false;
    }
    air_control(m, WONDER_MAX, 1.0);
    m.action_timer += 1.0;
    let held = down(m, A_BUTTON) && m.vel[1] > 0.0;
    let landed = air_step(m, if held { WONDER_GRAVITY } else { GRAVITY }, TERMINAL);
    if landed && m.action & ACT_FLAG_AIR == 0 && m.action != ACT_LEDGE_GRAB {
        start(m, ACT_WALKING, BANJO_WONDER);
    }
    true
}

// -- Flight and Beak Bomb ----------------------------------------------------------------------

fn start_fly(m: &mut MarioRuntime) -> bool {
    mario_set_forward_vel(m, 0.0);
    start(m, ACT_FREEFALL, BANJO_FLY_ENTER);
    m.vel = [0.0; 3];
    m.action_timer = 0.0;
    m.banjo.fly_pitch = 0.0;
    m.banjo.fly_flap = 0.0;
    m.banjo.fly_cooldown = BOMB_COOLDOWN;
    m.banjo.events.push(BanjoEvent::TakeOff);
    // The take-off itself is a hop out of the ground; step_fly applies the lift.
    m.pos[1] += 1.0;
    step_fly(m, true)
}

fn step_fly(m: &mut MarioRuntime, entering: bool) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if entering {
        // bsbfly_enter: crouch, lift at 1600 u/s on anim 0.2416, fly once the climb turns over.
        m.vel[0] = 0.0;
        m.vel[2] = 0.0;
        if t == FLY_LIFT_FRAME { m.vel[1] = FLY_LIFT + FLY_ENTER_GRAVITY; }
        if t < FLY_LIFT_FRAME {
            return true;
        }
        let landed = fly_move(m, FLY_ENTER_GRAVITY, -75.0);
        if landed { return true; }
        if m.vel[1] < 0.0 {
            m.action_arg = BANJO_FLY;
            m.banjo.fly_pitch = 0.0;
            m.forward_vel = 0.0;
        }
        return true;
    }
    // Steering: stick X turns (R = tighter), stick Y pitches (forward = nose down, 0..80 deg).
    let stick_x = (m.controller.stick_x / 64.0).clamp(-1.0, 1.0);
    let stick_y = (m.controller.stick_y / 64.0).clamp(-1.0, 1.0);
    let yaw_rate = if down(m, R_TRIG) { 2.0 * FLY_YAW } else { FLY_YAW };
    m.face_angle[1] = to_s16(m.face_angle[1] - stick_x * yaw_rate) as f64;
    // pitch_setIdeal: >0 dives to 80 degrees, <0 climbs to 300..360 (-60..0), stored here as -60..80.
    let ideal = if stick_y >= 0.0 { stick_y * FLY_PITCH_MAX } else { stick_y * 60.0 };
    m.banjo.fly_pitch = approach_f32(m.banjo.fly_pitch, ideal, 4.0, 4.0);
    let pitch = m.banjo.fly_pitch;
    m.face_angle[0] = to_s16(pitch * 182.04) as f64;

    // Flap: A spends a red feather (bsbfly_update / func_80346C10 on ITEM_F_RED_FEATHER).
    if pressed(m, A_BUTTON) {
        if m.banjo.red_feathers > 0 {
            m.banjo.red_feathers -= 1;
            m.banjo.fly_flap = FLY_FLAP_FRAMES;
        } else {
            m.banjo.events.push(BanjoEvent::OutOf(PickupKind::RedFeather));
        }
    }
    let (gravity, terminal, horizontal);
    if pitch > 60.0 {
        // Steep dive: slow forward, fast fall (map 60..80 -> 600..60, -99.9..-1000).
        let k = (pitch - 60.0) / 20.0;
        gravity = lerp(k, accel(300.0), accel(700.0));
        terminal = -lerp(k, 99.9, 1000.33) * 1.25 / 30.0;
        horizontal = lerp(k, FLY_SPEED, speed(60.0));
    } else if pitch < 0.0 {
        // Nose up: bleed speed (map 300..340 -> 0..600).
        let k = (pitch + 60.0) / 60.0;
        gravity = FLY_GRAVITY;
        terminal = -speed(99.9);
        horizontal = lerp(k, 0.0, FLY_SPEED);
    } else {
        gravity = FLY_GRAVITY;
        terminal = -speed(99.9);
        horizontal = FLY_SPEED;
    }
    let terminal = terminal.max(-110.0);
    m.forward_vel = approach_f32(m.forward_vel, horizontal, 3.0, 3.0);
    m.vel[0] = sins(m.face_angle[1]) * m.forward_vel;
    m.vel[2] = coss(m.face_angle[1]) * m.forward_vel;
    if m.banjo.fly_flap > 0.0 {
        m.banjo.fly_flap -= 1.0;
        m.vel[1] = m.vel[1].max(FLY_FLAP_VELOCITY);
    }
    if pressed(m, Z_TRIG) {
        m.face_angle[0] = 0.0;
        return start_buster(m);
    }
    if pressed(m, B_BUTTON) && m.banjo.fly_cooldown <= 0.0 {
        if m.banjo.red_feathers > 0 {
            m.banjo.red_feathers -= 1;
            return start_bomb(m);
        }
        m.banjo.events.push(BanjoEvent::OutOf(PickupKind::RedFeather));
    }
    let landed = fly_move(m, gravity, terminal);
    if landed {
        m.face_angle[0] = 0.0;
        m.banjo.fly_pitch = 0.0;
    }
    true
}

/// One airborne frame for flight; lands into the ordinary landing actions.
fn fly_move(m: &mut MarioRuntime, gravity: f64, terminal: f64) -> bool {
    m.vel[1] = terminal.max(m.vel[1] - gravity);
    let vy = m.vel[1];
    let hit = m.step_air(0);
    if hit == AIR_STEP_LANDED as i32 {
        let fv = m.forward_vel;
        m.land_event(vy, fv);
        m.face_angle[0] = 0.0;
        m.set_action(ACT_FREEFALL_LAND, 0.0);
        return true;
    }
    if hit == AIR_STEP_HIT_WALL as i32 {
        m.forward_vel *= 0.3;
    }
    m.vel[1] = vy;
    false
}

fn start_bomb(m: &mut MarioRuntime) -> bool {
    let pitch = m.banjo.fly_pitch.max(10.0) * std::f64::consts::PI / 180.0;
    let dir = forward_dir(m);
    m.banjo.bomb_dir = [dir[0] * pitch.cos(), -pitch.sin(), dir[2] * pitch.cos()];
    m.banjo.bomb_start = m.pos;
    start(m, ACT_FREEFALL, BANJO_BOMB);
    m.vel = [0.0; 3];
    m.forward_vel = 0.0;
    m.action_timer = 0.0;
    m.banjo.events.push(BanjoEvent::BeakBomb);
    step_bomb(m)
}

fn step_bomb(m: &mut MarioRuntime) -> bool {
    let t = m.action_timer;
    m.action_timer += 1.0;
    if m.action_arg == BANJO_BOMB_END {
        // bsbfly_beakbombend: tumble out for 0.35 s, then back into flight.
        m.face_angle[0] = 0.0;
        if t >= BOMB_END_FRAMES {
            start(m, ACT_FREEFALL, BANJO_FLY);
            m.banjo.fly_cooldown = BOMB_COOLDOWN;
            m.banjo.fly_pitch = 0.0;
            m.action_timer = 0.0;
            return true;
        }
        if fly_move(m, accel(1200.0), TERMINAL) { return true; }
        return true;
    }
    if t < BOMB_WINDUP {
        m.vel = [0.0; 3];
        return true;
    }
    // Hitbox: an attacking, fast move (detect_interaction -> ATTACK_FAST_ATTACK).
    m.action = ACT_DIVE;
    m.action_arg = BANJO_BOMB;
    let d = m.banjo.bomb_dir;
    m.vel = [d[0] * BOMB_SPEED, d[1] * BOMB_SPEED, d[2] * BOMB_SPEED];
    m.forward_vel = BOMB_SPEED;
    m.face_angle[1] = atan2s(d[2], d[0]) as f64;
    let before = m.pos;
    let hit = m.step_air(0);
    let travelled = ((m.pos[0] - m.banjo.bomb_start[0]).powi(2) + (m.pos[1] - m.banjo.bomb_start[1]).powi(2) + (m.pos[2] - m.banjo.bomb_start[2]).powi(2)).sqrt();
    let _ = before;
    if hit == AIR_STEP_LANDED as i32 {
        // BS_58_BEAKBOMB_CRASH: bounces off the floor.
        let fv = m.forward_vel;
        m.land_event(m.vel[1], fv);
        m.events.push(MarioEvent::GroundpoundLand);
        let at = m.pos;
        m.banjo.events.push(BanjoEvent::BeakBuster { pos: at });
        m.face_angle[0] = 0.0;
        m.forward_vel = 0.0;
        m.set_action(ACT_FREEFALL_LAND, 0.0);
        return true;
    }
    if hit == AIR_STEP_HIT_WALL as i32 {
        // func_802A40E0: crashing into a wall costs a honeycomb and knocks Banjo back.
        let at = m.pos;
        m.banjo.events.push(BanjoEvent::BeakBuster { pos: at });
        m.face_angle[0] = 0.0;
        m.forward_vel = 0.0;
        if !m.take_damage(1.0, Some([at[0] + d[0] * 100.0, at[1], at[2] + d[2] * 100.0])) {
            m.set_action(ACT_FREEFALL, 0.0);
        }
        return true;
    }
    if travelled > BOMB_RANGE {
        start(m, ACT_FREEFALL, BANJO_BOMB_END);
        m.vel = [d[0] * speed(300.0), 0.0, d[2] * speed(300.0)];
        m.forward_vel = speed(300.0);
        m.action_timer = 0.0;
    }
    true
}
