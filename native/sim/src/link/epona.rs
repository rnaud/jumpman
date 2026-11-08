//! Epona (`z_en_horse.c`): the horse Link rides, ported as an engine-free state machine.
//!
//! The original runs at 20 Hz; the sim ticks at 30 Hz, so every per-frame quantity is scaled by
//! [`DT`] (2/3 of an OoT frame per tick) and speeds are converted to sim units with
//! [`OOT_SCALE`] (4.4). The numbers are the decomp's:
//!
//! * gaits (`EnHorse_MountedWalk/Trot/Gallop`): base speed 3 / 6 / 8, turn limit 400 / 400 / 0x190,
//!   brake `0.3` when the stick points backwards (`cos <= -0.5`), idle `decel 0.06`, stick-driven
//!   acceleration of `0.5` per frame towards `base * stick / 54` (`EnHorse_UpdateSpeed`);
//! * carrots (`EnHorse_CheckBoost` / `EnHorse_RegenBoost`): six of them, a boost smooths the speed
//!   to 14 (`boostSpeed`) over 16 frames; the next carrot returns after 60 frames, then every 11;
//!   with all six spent the whole set returns after 140 frames;
//! * jumps (`EnHorse_LowJump` / `HighJump`): speed 12 / 13, the body follows the animation's root
//!   height until frame 17 / 23, then gravity -3.5 with an initial -6 / -10.5; an obstacle 19-40 high
//!   triggers the low jump at a gallop, 19-72 (below speed 13.8) or up to 112 (above it) the high jump
//!   once the horse is "moving fast" (>= 12.8, i.e. under a carrot);
//! * obstacles (`EnHorse_CheckFloors`): water, a missing floor, a floor steeper than 35 degrees or a
//!   drop of more than 40 in front / behind restores the last pose and makes her rear up (at a gallop)
//!   or reverse;
//! * stopping (`EnHorse_Stopping`): speed -0.6 per frame, the skid animation at 1.5x;
//! * Epona's Song summons her; she gallops (> 400 away), trots (300-400) or walks (< 300) to Link
//!   (`EnHorse_SetFollowAnimation`) and stands (`EnHorse_FollowPlayer`, < 100).
//!
//! Extension (not in the original): a galloping Epona jumps a gap, the generalised
//! `EnHorse_BridgeJump` of Gerudo Valley, when there is ground to land on within ~250 units.
use super::combat::{OOT_SCALE, TICKS_PER_FRAME};
use crate::collision::{CollisionWorld, FLOOR_LOWER_LIMIT};
use crate::math::*;

/// OoT frames per 30 Hz tick.
pub const DT: f64 = 1.0 / TICKS_PER_FRAME;
/// Frame counts of the nine animations (`EnHorseAnimationIndex` order).
pub const ANIM_LEN: [f64; 9] = [120.0, 30.0, 65.0, 33.0, 35.0, 29.0, 24.0, 23.0, 30.0];
pub const BOOST_SPEED: f64 = 14.0;
pub const MAX_BOOSTS: u8 = 6;
/// Link's origin while riding: `riderPos` (the saddle limb's point, about 65 above her origin) minus 27
/// (`Player_Action_8084CC98`'s `world.pos.y = horse.pos.y + riderPos.y - 27`).
pub const RIDER_HEIGHT: f64 = 38.0 * OOT_SCALE;
/// Mount range: `EnHorse_MountSideCheck` (75 units, within 30 vertically).
pub const MOUNT_RANGE: f64 = 75.0 * OOT_SCALE;
/// Mount / dismount animation lengths (`link_uma_left_up` 70, `right_up` 74, `left_down` 65, `right_down` 48).
pub const MOUNT_LEN: [f64; 2] = [70.0, 74.0];
pub const DISMOUNT_LEN: [f64; 2] = [65.0, 48.0];
const COS_35: f64 = 0.81915206;

fn u(oot: f64) -> f64 {
    oot * OOT_SCALE
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Anim {
    #[default]
    Idle = 0,
    Whinny = 1,
    /// `gEponaRefuseAnim`, played for the skid.
    Stopping = 2,
    Rearing = 3,
    Walk = 4,
    Trot = 5,
    Gallop = 6,
    LowJump = 7,
    HighJump = 8,
}

impl Anim {
    pub fn index(self) -> usize {
        self as usize
    }
    /// Name of the clip in `epona.json`.
    pub fn clip(self) -> &'static str {
        ["idle", "whinny", "refuse", "rearing", "walk", "trot", "gallop", "jump", "jumpHigh"][self as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Act {
    /// Standing in the world, nobody on her.
    #[default]
    Idle,
    /// Summoned: running to Link.
    Follow,
    MountedIdle,
    MountedWhinney,
    Turn,
    Walk,
    Trot,
    Gallop,
    Rearing,
    Stopping,
    Reverse,
    LowJump,
    HighJump,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Rider {
    #[default]
    None,
    /// `side` -1 = Link climbs up on her left (`link_uma_left_up`), +1 on her right.
    Mounting { side: i8, t: f64 },
    Riding,
    Dismounting { side: i8, t: f64 },
}

/// Sounds and cues for the app (`SFX_PLAY_AT_POS` calls of the original).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EponaEvent {
    Neigh,
    Groan,
    Walk,
    Run,
    Jump,
    Land,
    Land2,
    Slip,
    SandDust,
    CarrotRecover,
    Boost,
    Summoned,
    Mounted,
    Dismounted,
}

/// What the rider asks of her this tick (`play->state.input[0]`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RideInput {
    pub stick_x: f64,
    pub stick_y: f64,
    /// A pressed this tick: carrot while she moves, dismount while she stands.
    pub a_pressed: bool,
    /// B pressed: dismount when she is at most walking (a touch-friendly extra).
    pub b_pressed: bool,
}

#[derive(Clone, Debug)]
pub struct Epona {
    /// She exists in the world (summoned, spawned or ridden).
    pub present: bool,
    pub pos: Vec3,
    pub prev_pos: Vec3,
    /// Heading as an s16 angle (SM64 convention, 0 faces +Z).
    pub yaw: f64,
    pub prev_yaw: f64,
    /// Body tilt from the slope under her feet (`shape.rot.x` in radians, negative = nose up) and the lean in turns.
    pub pitch: f64,
    pub roll: f64,
    /// OoT units per OoT frame.
    pub speed: f64,
    vel_y: f64,
    gravity: f64,
    pub act: Act,
    pub anim: Anim,
    /// Animation frame (OoT frames).
    pub frame: f64,
    play_speed: f64,
    pub rider: Rider,
    pub jumping: bool,
    jump_start_y: f64,
    /// Height of the ground the current jump is meant to land on (obstacle tops in between are flown over).
    jump_land_y: f64,
    pub boosts: u8,
    regen: f64,
    boost_timer: f64,
    pub boost: bool,
    boost_decel: bool,
    wait: f64,
    no_input: f64,
    no_input_max: f64,
    force_reverse: bool,
    force_walk: bool,
    follow_timer: f64,
    turning_to_player: bool,
    neigh_on_stop: bool,
    land2_played: bool,
    obstacle: bool,
    last_pos: Vec3,
    last_yaw: f64,
    sound_step: u8,
    pub on_ground: bool,
    pub y_front: f64,
    pub y_back: f64,
    /// Blink timer for the eyes (0 = open).
    pub blink: u8,
    idle_rand: u32,
    pub mount_side_hint: i8,
    sand_played: bool,
    obstacle_prev: bool,
    sand_cool: f64,
    no_gap_jump: f64,
    floor_surface: Option<std::sync::Arc<crate::collision::Surface>>,
    /// Root height of the jump animations, per frame, in OoT units (set from `epona.json`).
    pub low_jump_y: Vec<f64>,
    pub high_jump_y: Vec<f64>,
    /// Link's idle clip while seated (`uma_wait_*`).
    pub rider_idle: (u8, f64),
    pub events: Vec<EponaEvent>,
}

impl Default for Epona {
    fn default() -> Self {
        Self {
            present: false,
            pos: [0.0; 3],
            prev_pos: [0.0; 3],
            yaw: 0.0,
            prev_yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            speed: 0.0,
            vel_y: 0.0,
            gravity: -3.5,
            act: Act::Idle,
            anim: Anim::Idle,
            frame: 0.0,
            play_speed: 1.0,
            rider: Rider::None,
            jumping: false,
            jump_start_y: 0.0,
            jump_land_y: 0.0,
            boosts: MAX_BOOSTS,
            regen: 0.0,
            boost_timer: 0.0,
            boost: false,
            boost_decel: false,
            wait: 0.0,
            no_input: 0.0,
            no_input_max: 0.0,
            force_reverse: false,
            force_walk: false,
            follow_timer: 0.0,
            turning_to_player: false,
            neigh_on_stop: false,
            land2_played: false,
            obstacle: false,
            last_pos: [0.0; 3],
            last_yaw: 0.0,
            sound_step: 0,
            on_ground: true,
            y_front: 0.0,
            y_back: 0.0,
            blink: 0,
            idle_rand: 12345,
            mount_side_hint: -1,
            sand_played: false,
            obstacle_prev: false,
            sand_cool: 0.0,
            no_gap_jump: 0.0,
            floor_surface: None,
            low_jump_y: vec![],
            high_jump_y: vec![],
            rider_idle: (0, 0.0),
            events: vec![],
        }
    }
}

fn forward(yaw: f64, dist: f64) -> Vec3 {
    [sins(yaw) * dist, 0.0, coss(yaw) * dist]
}

/// `EnHorse_StickDirection`: magnitude (capped at 60) and the stick angle relative to her heading.
pub fn stick_direction(x: f64, y: f64) -> (f64, f64) {
    let mag = (x * x + y * y).sqrt().min(60.0);
    let angle = (-x).atan2(y) * 32768.0 / std::f64::consts::PI;
    (mag, angle)
}

fn cos_angle(a: f64) -> f64 {
    coss(a)
}

impl Epona {
    /// Animation frame at fraction `t` of the way to the next tick (for smooth drawing).
    pub fn frame_at(&self, t: f64) -> f64 {
        (self.frame + self.play_speed * DT * t).min(ANIM_LEN[self.anim.index()] - 1e-3)
    }

    pub fn is_mounted(&self) -> bool {
        matches!(self.rider, Rider::Mounting { .. } | Rider::Riding | Rider::Dismounting { .. })
    }

    /// Fully seated and in control.
    pub fn riding(&self) -> bool {
        self.rider == Rider::Riding
    }

    /// Use the animation root heights of the real jump clips.
    pub fn set_jump_profiles(&mut self, low: Vec<f64>, high: Vec<f64>) {
        self.low_jump_y = low;
        self.high_jump_y = high;
    }

    fn rnd(&mut self) -> f64 {
        self.idle_rand = self.idle_rand.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.idle_rand >> 8) as f64 / 16777216.0
    }

    fn event(&mut self, e: EponaEvent) {
        self.events.push(e);
    }

    pub fn take_events(&mut self) -> Vec<EponaEvent> {
        std::mem::take(&mut self.events)
    }

    /// Put her on the ground at `pos` facing `yaw`, idle and rideable.
    pub fn place(&mut self, world: &CollisionWorld, pos: Vec3, yaw: f64) {
        let h = world.find_floor_height(pos[0], pos[1] + 100.0, pos[2]);
        let y = if h > FLOOR_LOWER_LIMIT { h } else { pos[1] };
        *self = Self { present: true, boosts: self.boosts, low_jump_y: std::mem::take(&mut self.low_jump_y), high_jump_y: std::mem::take(&mut self.high_jump_y), idle_rand: self.idle_rand, ..Self::default() };
        self.pos = [pos[0], y, pos[2]];
        self.prev_pos = self.pos;
        self.last_pos = self.pos;
        self.yaw = yaw;
        self.prev_yaw = yaw;
        self.last_yaw = yaw;
    }

    pub fn remove(&mut self) {
        self.present = false;
        self.rider = Rider::None;
    }

    // ---- mounting -------------------------------------------------------------------------

    /// Which side Link can climb up from (`EnHorse_GetMountSide`): -1 left, +1 right, `None` when he is
    /// out of reach or she is busy. She must be standing (idle / whinny animations).
    pub fn mount_side(&self, link: Vec3) -> Option<i8> {
        if !self.present || self.act != Act::Idle || self.rider != Rider::None || !matches!(self.anim, Anim::Idle | Anim::Whinny) {
            return None;
        }
        let (dx, dz) = (link[0] - self.pos[0], link[2] - self.pos[2]);
        let dist = (dx * dx + dz * dz).sqrt();
        if dist > MOUNT_RANGE || (self.pos[1] - link[1]).abs() > u(30.0) {
            return None;
        }
        // Lateral component along her left axis (cos y, -sin y). The original wants Link within 60 degrees of the
        // flank (`EnHorse_UpdatePlayerDir`: sin > 0.866); here 30 degrees either side of front / back is excluded.
        let lateral = if dist > 1.0 { (dx * coss(self.yaw) - dz * sins(self.yaw)) / dist } else { 1.0 };
        if lateral.abs() < 0.5 {
            return None;
        }
        Some(if lateral > 0.0 { -1 } else { 1 })
    }

    /// Nobody on her any more (the player switched character): she stands and can be ridden again.
    pub fn release_rider(&mut self) {
        if self.rider != Rider::None {
            self.rider = Rider::None;
            self.act = Act::Idle;
            self.speed = 0.0;
            self.jumping = false;
            self.gravity = -3.5;
            self.start_anim(Anim::Idle);
        }
    }

    /// Start with Link already in the saddle (`HORSE_PTYPE_PLAYER_SPAWNED_RIDING`, the app's `JUMPMAN_EPONA=1` hook).
    pub fn start_mounted(&mut self) {
        self.rider = Rider::Riding;
        self.act = Act::MountedIdle;
        self.anim = Anim::Idle;
        self.frame = 0.0;
        self.no_input = 0.0;
        self.no_input_max = 0.0;
    }

    /// Link starts climbing up (`Actor_SetRideActor` + `EnHorse_Freeze`).
    pub fn begin_mount(&mut self, side: i8) {
        self.rider = Rider::Mounting { side, t: 0.0 };
        self.mount_side_hint = side;
        self.speed = 0.0;
        self.act = Act::MountedIdle;
        self.anim = Anim::Idle;
        self.frame = 0.0;
        self.no_input = 55.0;
        self.no_input_max = 55.0;
        self.event(EponaEvent::Mounted);
    }

    /// She stands still enough to get off (`EN_HORSE_CHECK_4`: idle, nothing in progress).
    pub fn can_dismount(&self) -> bool {
        self.rider == Rider::Riding && matches!(self.act, Act::MountedIdle | Act::MountedWhinney) && self.speed == 0.0
    }

    pub fn begin_dismount(&mut self, side: i8) {
        self.rider = Rider::Dismounting { side, t: 0.0 };
        self.speed = 0.0;
        self.event(EponaEvent::Dismounted);
    }

    /// Where Link stands after getting off: beside her on `side`, or the other side / a little nearer when that spot has no
    /// ground, is in a wall or is under water.
    pub fn dismount_point(&self, world: &CollisionWorld, side: i8) -> Vec3 {
        let left = [coss(self.yaw), 0.0, -sins(self.yaw)];
        for (s, d) in [(side, 60.0), (-side, 60.0), (side, 45.0), (-side, 45.0), (side, 35.0), (-side, 35.0)] {
            let off = -(s as f64) * u(d);
            let (x, z) = (self.pos[0] + left[0] * off, self.pos[2] + left[2] * off);
            let hit = world.find_floor(x, self.pos[1] + u(40.0), z);
            if hit.surface.is_none() || (hit.height - self.pos[1]).abs() > u(30.0) || world.water_level(x, z) > hit.height {
                continue;
            }
            let mut p = [x, hit.height, z];
            if world.resolve_and_return_wall_collisions(&mut p, 80.0, 50.0).is_some() {
                continue;
            }
            return [x, hit.height, z];
        }
        // Nothing around her is safe: stay put on her ground.
        [self.pos[0], self.pos[1], self.pos[2]]
    }

    /// Where the rider's origin is (`EnHorse_PostDraw`'s `riderPos` before the model's saddle is known).
    pub fn rider_point(&self) -> Vec3 {
        [self.pos[0], self.pos[1] + RIDER_HEIGHT, self.pos[2]]
    }

    // ---- summoning --------------------------------------------------------------------------

    /// Epona's Song (`EnHorse_Spawn` + `EnHorse_FollowPlayer`): she appears at the edge of view and
    /// runs to Link. `away` is the preferred direction (an s16 angle, e.g. behind the camera).
    pub fn summon(&mut self, world: &CollisionWorld, link: Vec3, away: f64) -> bool {
        if self.is_mounted() {
            return false;
        }
        // Prefer a far spot (about 600 units, the original 400+), fall back to nearer ones, scanning around.
        for dist in [u(580.0), u(480.0), u(380.0), u(280.0)] {
            for k in 0..16 {
                let turn = if k % 2 == 0 { k / 2 } else { -(k + 1) / 2 } as f64 * 2048.0;
                let a = away + turn;
                let spot = [link[0] + sins(a) * dist, link[1], link[2] + coss(a) * dist];
                if !self.spawn_ok(world, link, spot) {
                    continue;
                }
                let h = world.find_floor_height(spot[0], link[1] + 200.0, spot[2]);
                let yaw = atan2s(link[2] - spot[2], link[0] - spot[0]) as f64;
                self.place(world, [spot[0], h, spot[2]], yaw);
                self.act = Act::Follow;
                self.set_follow_animation(dist / OOT_SCALE);
                self.event(EponaEvent::Summoned);
                self.event(EponaEvent::Neigh);
                return true;
            }
        }
        false
    }

    fn spawn_ok(&self, world: &CollisionWorld, link: Vec3, spot: Vec3) -> bool {
        let hit = world.find_floor(spot[0], link[1] + 300.0, spot[2]);
        let Some(s) = hit.surface else { return false };
        if s.normal[1] < COS_35 || hit.height < link[1] - u(120.0) || hit.height > link[1] + u(60.0) {
            return false;
        }
        if world.water_level(spot[0], spot[2]) > hit.height {
            return false;
        }
        // A short walk along the line to Link should stay on level ground.
        for i in 1..=8 {
            let t = i as f64 / 8.0;
            let (x, z) = (spot[0] + (link[0] - spot[0]) * t, spot[2] + (link[2] - spot[2]) * t);
            let f = world.find_floor(x, link[1] + 300.0, z);
            if f.surface.is_none() || (f.height - link[1]).abs() > u(120.0) || world.water_level(x, z) > f.height {
                return false;
            }
        }
        true
    }

    fn start_anim(&mut self, a: Anim) {
        self.anim = a;
        self.frame = 0.0;
        self.play_speed = 1.0;
    }

    /// Advance the animation by the current play speed; true once it has run its length.
    fn anim_update(&mut self) -> bool {
        self.frame += self.play_speed * DT;
        if self.frame >= ANIM_LEN[self.anim.index()] {
            self.frame = ANIM_LEN[self.anim.index()] - 1.0e-6;
            true
        } else {
            false
        }
    }

    fn set_follow_animation(&mut self, dist: f64) {
        let id = match self.anim {
            Anim::Gallop => if dist > 400.0 { Anim::Gallop } else { Anim::Trot },
            Anim::Trot => if dist > 400.0 { Anim::Gallop } else if dist < 300.0 { Anim::Walk } else { Anim::Trot },
            Anim::Walk => if dist > 300.0 { Anim::Trot } else { Anim::Walk },
            _ => if dist > 400.0 { Anim::Gallop } else if dist > 300.0 { Anim::Trot } else { Anim::Walk },
        };
        self.act = Act::Follow;
        self.turning_to_player = false;
        self.start_anim(id);
    }

    // ---- per tick -----------------------------------------------------------------------------

    /// One 30 Hz tick. `link` is Link's position (the player Epona follows / is mounted by).
    pub fn step(&mut self, world: &CollisionWorld, link: Vec3, input: &RideInput) {
        if !self.present {
            return;
        }
        self.prev_pos = self.pos;
        self.prev_yaw = self.yaw;
        self.last_pos = self.pos;
        self.last_yaw = self.yaw;
        self.obstacle_prev = self.obstacle;
        self.obstacle = false;
        if self.sand_cool > 0.0 {
            self.sand_cool -= DT;
        }
        if self.no_gap_jump > 0.0 {
            self.no_gap_jump -= DT;
        }

        // Mount / dismount phases keep her frozen (EnHorse_Frozen with a no-input timer).
        match self.rider {
            Rider::Mounting { side, t } => {
                let t = t + DT;
                self.speed = 0.0;
                self.idle_update();
                self.rider = if t >= MOUNT_LEN[(side > 0) as usize] {
                    self.act = Act::MountedIdle;
                    Rider::Riding
                } else {
                    Rider::Mounting { side, t }
                };
                self.finish_tick(world);
                return;
            }
            Rider::Dismounting { side, t } => {
                let t = t + DT;
                self.speed = 0.0;
                self.idle_update();
                self.rider = if t >= DISMOUNT_LEN[(side > 0) as usize] {
                    self.act = Act::Idle;
                    self.anim = Anim::Idle;
                    self.frame = 0.0;
                    Rider::None
                } else {
                    Rider::Dismounting { side, t }
                };
                self.finish_tick(world);
                return;
            }
            _ => {}
        }

        let riding = self.rider == Rider::Riding;
        if riding {
            self.rider_idle_tick();
        }
        // A / B on a standing horse: dismount.
        if riding && self.can_dismount() && (input.a_pressed || input.b_pressed) && self.no_input <= 0.0 {
            let side = self.nearest_side(link);
            self.begin_dismount(side);
            return;
        }
        let (mag, angle) = stick_direction(input.stick_x, input.stick_y);
        match self.act {
            Act::Idle => self.act_idle(),
            Act::Follow => self.act_follow(link),
            Act::MountedIdle | Act::MountedWhinney => self.act_mounted_idle(mag, angle),
            Act::Turn => self.act_turn(mag, angle),
            Act::Walk => self.act_walk(mag, angle),
            Act::Trot => self.act_trot(mag, angle),
            Act::Gallop => self.act_gallop(mag, angle),
            Act::Rearing => self.act_rearing(angle),
            Act::Stopping => self.act_stopping(),
            Act::Reverse => self.act_reverse(mag, angle),
            Act::LowJump | Act::HighJump => self.act_jump(world),
        }
        if riding {
            if matches!(self.act, Act::Walk | Act::Trot | Act::Gallop) {
                self.check_boost(input.a_pressed);
            }
            self.regen_boost();
        }
        self.finish_tick(world);
    }

    /// Link gets off on the side he got on.
    fn nearest_side(&self, _link: Vec3) -> i8 {
        self.mount_side_hint
    }

    fn finish_tick(&mut self, world: &CollisionWorld) {
        // Actor_MoveXZGravity
        let d = forward(self.yaw, self.speed * OOT_SCALE * DT);
        self.pos[0] += d[0];
        self.pos[2] += d[2];
        if !(self.jumping && self.gravity == 0.0) {
            self.vel_y = (self.vel_y + self.gravity * DT).max(-20.0);
            self.pos[1] += self.vel_y * OOT_SCALE * DT;
        }
        self.bg_check(world);
        self.tilt_body();
        // Blink
        if self.blink == 0 {
            if self.rnd() < 0.025 * DT {
                self.blink = 1;
            }
        } else if self.rnd() < 0.5 {
            self.blink = (self.blink + 1) % 4;
        }
    }

    fn tilt_body(&mut self) {
        let sp = self.speed / BOOST_SPEED;
        let turn = to_s16(self.yaw - self.last_yaw) as f64 / DT;
        let target = -(1820.0 * sp * (turn / 480.00003)).trunc();
        let diff = target - self.roll_s16();
        let mut r = self.roll_s16();
        if target.abs() < 100.0 {
            r = 0.0;
        } else if diff.abs() < 100.0 {
            r = target;
        } else if diff > 0.0 {
            r += 100.0 * DT;
        } else {
            r -= 100.0 * DT;
        }
        self.roll = r * std::f64::consts::PI / 32768.0;
    }

    fn roll_s16(&self) -> f64 {
        self.roll * 32768.0 / std::f64::consts::PI
    }

    // ---- actions ------------------------------------------------------------------------------

    fn idle_update(&mut self) {
        if self.anim_update() {
            self.frame = 0.0;
        }
    }

    fn act_idle(&mut self) {
        self.speed = 0.0;
        if self.anim == Anim::Idle && self.frame > 35.0 && !self.sand_played {
            self.sand_played = true;
            self.event(EponaEvent::SandDust);
        } else if self.anim == Anim::Rearing && self.frame > 25.0 && !self.land2_played {
            self.land2_played = true;
            self.event(EponaEvent::Land2);
        }
        if self.anim_update() {
            // EnHorse_Idle: one of the other idle animations.
            let r = self.rnd() > 0.5;
            let next = match (self.anim, r) {
                (Anim::Idle, true) => Anim::Whinny,
                (Anim::Idle, false) => Anim::Idle,
                (Anim::Whinny, true) => Anim::Rearing,
                (Anim::Whinny, false) => Anim::Idle,
                (_, true) => Anim::Idle,
                (_, false) => Anim::Whinny,
            };
            self.sand_played = false;
            self.land2_played = false;
            match next {
                Anim::Whinny => self.event(EponaEvent::Groan),
                Anim::Rearing => self.event(EponaEvent::Neigh),
                _ => {}
            }
            self.start_anim(next);
        }
    }

    fn act_follow(&mut self, link: Vec3) {
        let (dx, dz) = (link[0] - self.pos[0], link[2] - self.pos[2]);
        let dist = (dx * dx + dz * dz).sqrt() / OOT_SCALE;
        // EnHorse_RotateToPlayer: 400 per OoT frame, plus 800 when blocked.
        let want = atan2s(dz, dx) as f64;
        let diff = to_s16(want - self.yaw) as f64;
        let mut turn = diff.clamp(-400.0 * DT, 400.0 * DT);
        if self.obstacle_prev {
            turn += 800.0 * DT;
        }
        self.yaw = to_s16(self.yaw + turn) as f64;
        let (speed, play) = match self.anim {
            Anim::Gallop => (8.0, 0.3),
            Anim::Trot => (6.0, 0.375),
            Anim::Walk => (3.0, 0.75),
            _ => (0.0, 1.0),
        };
        self.speed = speed;
        self.play_speed = speed * play + if speed == 0.0 { 1.0 } else { 0.0 };
        if self.anim == Anim::Walk {
            self.walk_sfx();
        }
        self.follow_timer += DT;
        if self.follow_timer > 300.0 {
            self.stand_ridable();
            self.event(EponaEvent::Neigh);
            return;
        }
        if self.anim_update() {
            match self.anim {
                Anim::Gallop => self.event(EponaEvent::Run),
                Anim::Trot => self.event(EponaEvent::Run),
                _ => {}
            }
            if dist < 100.0 {
                self.stand_ridable();
            } else {
                self.set_follow_animation(dist);
            }
        }
    }

    fn stand_ridable(&mut self) {
        self.act = Act::Idle;
        self.speed = 0.0;
        self.sand_played = false;
        self.land2_played = false;
        self.start_anim(Anim::Idle);
    }

    fn walk_sfx(&mut self) {
        // sAnimSoundFrames = { 0, 16 }
        let marks = [0.0, 16.0];
        if marks[self.sound_step as usize] < self.frame && !(self.sound_step == 0 && marks[1] < self.frame) {
            self.event(EponaEvent::Walk);
            self.sound_step = (self.sound_step + 1) % 2;
        }
    }

    fn start_walking(&mut self, from_idle: bool) {
        self.act = Act::Walk;
        self.sound_step = 0;
        self.start_anim(Anim::Walk);
        self.wait = if from_idle { 8.0 } else { 0.0 };
        self.no_input = 0.0;
        self.no_input_max = 0.0;
    }

    fn start_trotting(&mut self) {
        self.act = Act::Trot;
        self.start_anim(Anim::Trot);
    }

    fn start_galloping(&mut self) {
        self.act = Act::Gallop;
        self.start_anim(Anim::Gallop);
        self.no_input = 0.0;
        self.no_input_max = 0.0;
    }

    fn start_idle_reset(&mut self) {
        self.act = Act::MountedIdle;
        self.speed = 0.0;
        self.start_anim(Anim::Idle);
    }

    fn start_turning(&mut self) {
        self.act = Act::Turn;
        self.sound_step = 0;
        self.start_anim(Anim::Walk);
    }

    fn start_reversing(&mut self) {
        self.act = Act::Reverse;
        self.sound_step = 0;
        self.start_anim(Anim::Walk);
    }

    fn start_braking(&mut self) {
        self.act = Act::Stopping;
        self.start_anim(Anim::Stopping);
        self.play_speed = 1.5;
        self.event(EponaEvent::Slip);
        self.neigh_on_stop = true;
        self.boost = false;
    }

    fn start_rearing(&mut self) {
        self.act = Act::Rearing;
        self.start_anim(Anim::Rearing);
        self.land2_played = false;
        self.event(EponaEvent::Neigh);
    }

    fn act_mounted_idle(&mut self, mag: f64, angle: f64) {
        self.speed = 0.0;
        if self.no_input > 0.0 {
            self.no_input -= DT;
        }
        if mag > 10.0 && self.no_input <= 0.0 {
            if cos_angle(angle) <= -0.5 {
                self.start_reversing();
                return;
            } else if cos_angle(angle) <= 0.7071 {
                self.start_turning();
                return;
            } else {
                self.start_walking(true);
                return;
            }
        }
        if self.anim_update() {
            // EnHorse_MountedIdleAnim: a whinny between idles.
            if self.act == Act::MountedIdle && self.rnd() < 0.3 {
                self.act = Act::MountedWhinney;
                self.start_anim(Anim::Whinny);
                self.event(EponaEvent::Groan);
            } else {
                self.start_idle_reset();
            }
        }
    }

    fn act_turn(&mut self, mag: f64, angle: f64) {
        self.speed = 0.0;
        self.walk_sfx();
        if mag > 10.0 {
            if cos_angle(angle) <= -0.5 {
                self.start_reversing();
                return;
            } else if cos_angle(angle) <= 0.7071 {
                let c = angle.clamp(-800.0, 800.0);
                self.yaw = to_s16(self.yaw + c * DT) as f64;
            } else {
                self.start_walking(false);
                return;
            }
        }
        self.play_speed = 1.0;
        if self.anim_update() {
            if cos_angle(angle) <= 0.7071 && mag > 10.0 {
                self.start_turning();
            } else {
                self.start_idle_reset();
            }
        }
    }

    /// `EnHorse_UpdateSpeed`.
    fn update_speed(&mut self, mag: f64, angle: f64, brake_decel: f64, brake_angle: f64, min_mag: f64, decel: f64, base: f64, turn_speed: f64) {
        let slope = self.slope_multiplier();
        let base = base * slope;
        if cos_angle(angle) <= brake_angle {
            self.speed = (self.speed - brake_decel * DT).max(0.0);
            return;
        }
        if mag < min_mag {
            self.boost = false;
            self.boost_decel = false;
            self.speed = (self.speed - decel * DT).max(0.0);
            return;
        }
        if self.boost {
            let target = slope * BOOST_SPEED;
            let left = 16.0 - self.boost_timer;
            if left > 0.0 {
                self.speed += (target - self.speed) / left * DT;
            } else {
                self.speed = target;
            }
            if target <= self.speed + 0.02 {
                self.speed = self.speed.max(target - 0.02);
                self.boost = false;
                self.boost_decel = true;
            }
        } else if self.boost_decel {
            if base < self.speed {
                self.speed -= 0.06 * DT;
            } else if self.speed < base {
                self.speed = base;
                self.boost_decel = false;
            }
        } else {
            let target = base * (1.0 / 54.0) * mag;
            self.speed += if self.speed <= target { 1.0 } else { -1.0 } * 50.0 * 0.01 * DT;
            if base < self.speed {
                self.speed -= decel * DT;
                if self.speed < base {
                    self.speed = base;
                }
            }
        }
        let t = angle * (1.0 / 32236.0);
        let traction = 2.2 - self.speed / BOOST_SPEED;
        let turn = (angle * t * t * traction).clamp(-turn_speed * traction, turn_speed * traction);
        self.yaw = to_s16(self.yaw + turn * DT) as f64;
    }

    /// `EnHorse_SlopeSpeedMultiplier`: 0.7 going uphill steeper than 20 degrees.
    fn slope_multiplier(&self) -> f64 {
        if self.pitch < -0.3491 { 0.7 } else { 1.0 }
    }

    fn act_walk(&mut self, mag: f64, angle: f64) {
        self.walk_sfx();
        if self.no_input_max == 0.0 || (self.no_input > 0.0 && self.no_input < self.no_input_max - 20.0) {
            self.update_speed(mag, angle, 0.3, -0.5, 10.0, 0.06, 3.0, 400.0);
        } else {
            self.speed = 3.0;
        }
        if self.speed == 0.0 && self.wait <= 0.0 {
            self.start_idle_reset();
            return;
        } else if self.speed > 3.0 {
            self.start_trotting();
            return;
        }
        if self.no_input > 0.0 {
            self.no_input -= DT;
            if self.no_input <= 0.0 {
                self.no_input_max = 0.0;
            }
        }
        if self.wait <= 0.0 {
            self.play_speed = self.speed * 0.75;
            if self.anim_update() || self.speed == 0.0 {
                if self.no_input <= 0.0 {
                    if self.speed > 3.0 {
                        self.start_trotting();
                    } else if mag < 10.0 || cos_angle(angle) <= -0.5 {
                        self.start_idle_reset();
                    } else {
                        self.act = Act::Walk;
                        self.sound_step = 0;
                        self.start_anim(Anim::Walk);
                    }
                }
            }
        } else {
            self.speed = 0.0;
            self.wait -= DT;
        }
    }

    fn act_trot(&mut self, mag: f64, angle: f64) {
        self.update_speed(mag, angle, 0.3, -0.5, 10.0, 0.06, 6.0, 400.0);
        if self.speed < 3.0 {
            self.start_walking(false);
            return;
        }
        self.play_speed = self.speed * 0.375;
        if self.anim_update() {
            self.event(EponaEvent::Run);
            if self.speed >= 6.0 {
                self.start_galloping();
            } else if self.speed < 3.0 {
                self.start_walking(false);
            } else {
                self.start_trotting();
            }
        }
    }

    fn act_gallop(&mut self, mag: f64, angle: f64) {
        if self.no_input <= 0.0 {
            self.update_speed(mag, angle, 0.3, -0.5, 10.0, 0.06, 8.0, 400.0);
        } else {
            self.no_input -= DT;
            self.speed = 8.0;
        }
        if self.speed < 6.0 {
            self.start_trotting();
            return;
        }
        self.play_speed = self.speed * 0.3;
        if self.anim_update() {
            self.event(EponaEvent::Run);
            if mag >= 10.0 && cos_angle(angle) <= -0.5 {
                self.start_braking();
            } else if self.speed < 6.0 {
                self.start_trotting();
            } else {
                self.start_anim(Anim::Gallop);
            }
        }
    }

    fn act_rearing(&mut self, angle: f64) {
        self.speed = 0.0;
        if self.frame > 25.0 && !self.land2_played {
            self.land2_played = true;
            self.event(EponaEvent::Land2);
        }
        self.play_speed = 1.0;
        if self.anim_update() {
            if self.force_reverse {
                self.no_input = 100.0;
                self.no_input_max = 100.0;
                self.force_reverse = false;
                self.start_reversing();
            } else if self.force_walk {
                self.no_input = 100.0;
                self.no_input_max = 100.0;
                self.force_walk = false;
                self.start_walking(false);
            } else if cos_angle(angle) <= -0.5 {
                self.start_reversing();
                self.no_input = 0.0;
                self.no_input_max = 0.0;
            } else {
                self.start_idle_reset();
            }
        }
    }

    fn act_stopping(&mut self) {
        if self.speed > 0.0 {
            self.speed = (self.speed - 0.6 * DT).max(0.0);
        }
        self.play_speed = 1.5;
        if self.neigh_on_stop && self.frame > 29.0 {
            self.speed = 0.0;
            if self.rnd() > 0.5 {
                self.event(EponaEvent::Neigh);
                self.neigh_on_stop = false;
            } else {
                self.start_idle_reset();
                return;
            }
        }
        if self.frame > 29.0 {
            self.speed = 0.0;
        } else if self.speed > 3.0 && self.force_reverse {
            self.speed = 3.0;
        }
        if self.anim_update() {
            if self.force_reverse {
                self.no_input = 100.0;
                self.no_input_max = 100.0;
                self.start_reversing();
                self.force_reverse = false;
            } else {
                self.start_idle_reset();
            }
        }
    }

    fn act_reverse(&mut self, mag: f64, angle: f64) {
        self.walk_sfx();
        let mut angle = angle;
        if self.no_input_max == 0.0 || (self.no_input > 0.0 && self.no_input < self.no_input_max - 20.0) {
            if mag < 10.0 && self.no_input <= 0.0 {
                self.start_idle_reset();
                self.speed = 0.0;
                return;
            }
            if mag < 10.0 {
                angle = -32767.0;
            } else if cos_angle(angle) > -0.5 {
                self.no_input_max = 0.0;
                self.start_idle_reset();
                self.speed = 0.0;
                return;
            }
        } else if mag < 10.0 {
            angle = -32767.0;
        }
        self.speed = -2.0;
        let turn = (32767.0 - angle).clamp(-1200.0, 1200.0);
        self.yaw = to_s16(self.yaw + turn * DT) as f64;
        if self.no_input > 0.0 {
            self.no_input -= DT;
            if self.no_input <= 0.0 {
                self.no_input_max = 0.0;
            }
        }
        self.play_speed = (self.speed * 0.5 * 1.5).abs();
        if self.anim_update() && self.no_input <= 0.0 {
            if mag > 10.0 && cos_angle(angle) <= -0.5 {
                self.start_reversing();
            } else if mag < 10.0 {
                self.start_idle_reset();
            } else {
                self.start_reversing();
            }
        }
    }

    // ---- jumping ------------------------------------------------------------------------------

    /// Root height of the jump clip at the current frame, in OoT units above `jump_start_y`
    /// (`jointTable[0].y * 0.01`; the body is drawn without its translation while jumping).
    fn jump_profile(&self, high: bool) -> f64 {
        let profile = if high { &self.high_jump_y } else { &self.low_jump_y };
        if profile.is_empty() {
            // Fallback arc: the standing body height plus a half sine over the take-off part of the clip.
            let (top, amp) = if high { (23.0, 93.5) } else { (17.0, 45.3) };
            return 44.7 + amp * (std::f64::consts::PI * (self.frame / top).clamp(0.0, 1.0)).sin();
        }
        let f = self.frame.clamp(0.0, profile.len() as f64 - 1.0);
        let (i, t) = (f.floor() as usize, f.fract());
        let a = profile[i];
        let b = profile[(i + 1).min(profile.len() - 1)];
        a + (b - a) * t
    }

    fn start_jump(&mut self, high: bool, land_y: f64) {
        self.jump_land_y = land_y;
        self.act = if high { Act::HighJump } else { Act::LowJump };
        self.start_anim(if high { Anim::HighJump } else { Anim::LowJump });
        self.play_speed = 1.5;
        self.jump_start_y = self.pos[1];
        self.gravity = 0.0;
        self.vel_y = 0.0;
        self.jumping = true;
        self.event(EponaEvent::Jump);
    }

    /// The floor she lands on: the highest one below her that is not an obstacle top above the intended landing.
    fn landing_floor(&self, world: &CollisionWorld) -> f64 {
        let reach = self.jump_land_y.max(self.jump_start_y - u(45.0)) + u(30.0);
        // find_floor accepts floors up to 78 above the probe height.
        let y = reach - 78.0;
        let hit = world.find_floor(self.pos[0], y, self.pos[2]);
        if hit.surface.is_some() { hit.height } else { FLOOR_LOWER_LIMIT }
    }

    fn act_jump(&mut self, world: &CollisionWorld) {
        let high = self.act == Act::HighJump;
        let (switch, fast_v, spd) = if high { (23.0, -10.5, 13.0) } else { (17.0, -6.0, 12.0) };
        self.speed = spd;
        let floor = self.landing_floor(world);
        if self.frame > switch {
            self.gravity = -3.5;
            if self.vel_y == 0.0 {
                self.vel_y = fast_v;
            }
            self.play_speed = if self.pos[1] < floor + u(90.0) { 1.5 } else { 0.0 };
        } else {
            self.pos[1] = self.jump_start_y + u(self.jump_profile(high));
        }
        let done = self.anim_update();
        if done || (self.frame > switch && self.pos[1] < floor - self.vel_y * OOT_SCALE + u(80.0)) {
            self.event(EponaEvent::Land);
            self.jumping = false;
            self.gravity = -3.5;
            if floor > FLOOR_LOWER_LIMIT {
                self.pos[1] = floor;
            }
            self.vel_y = 0.0;
            // EnHorse_JumpLanding: back to the gallop animation.
            self.act = Act::Gallop;
            self.start_anim(Anim::Gallop);
            self.no_input = 0.0;
        }
    }

    // ---- carrots ------------------------------------------------------------------------------

    /// `EnHorse_CheckBoost`.
    fn check_boost(&mut self, a_pressed: bool) {
        if !a_pressed || self.boost || self.boosts == 0 {
            if a_pressed && self.boosts == 0 && !self.boost && self.rnd() < 0.1 {
                self.event(EponaEvent::Neigh);
            }
            return;
        }
        self.boost = true;
        self.boost_decel = false;
        self.boosts -= 1;
        self.boost_timer = 0.0;
        self.regen = if self.boosts == 0 { 140.0 } else { 60.0 };
        self.event(EponaEvent::Boost);
    }

    /// `EnHorse_RegenBoost`.
    fn regen_boost(&mut self) {
        if self.boosts > 0 && self.boosts < MAX_BOOSTS {
            self.regen -= DT;
            self.boost_timer += DT;
            if self.regen <= 0.0 {
                self.boosts += 1;
                self.event(EponaEvent::CarrotRecover);
                if self.boosts < MAX_BOOSTS {
                    self.regen = 11.0;
                }
            }
        } else if self.boosts == 0 {
            self.regen -= DT;
            self.boost_timer += DT;
            if self.regen <= 0.0 {
                self.regen = 0.0;
                self.boosts = MAX_BOOSTS;
                self.event(EponaEvent::CarrotRecover);
            }
        } else {
            self.boost_timer += DT;
        }
    }

    // ---- the rider's seated animation ---------------------------------------------------------

    fn rider_idle_tick(&mut self) {
        // uma_wait_3 (30 frames) most of the time, uma_wait_1 / 2 now and then (z_player.c action 8084CC98).
        let len = [30.0, 208.0, 195.0][self.rider_idle.0 as usize];
        self.rider_idle.1 += DT;
        if self.rider_idle.1 >= len {
            let r = self.rnd();
            self.rider_idle = (if r < 0.1 { 2 } else if r < 0.2 { 1 } else { 0 }, 0.0);
        }
    }

    // ---- collision ----------------------------------------------------------------------------

    fn obstruct(&mut self, galloping: bool, kind_front: bool) {
        self.pos = self.last_pos;
        self.yaw = self.last_yaw;
        self.obstacle = true;
        self.vel_y = 0.0;
        if self.rider != Rider::Riding {
            // The summoned horse only turns away (the original gives up here too).
            self.speed = self.speed.min(3.0);
            return;
        }
        if self.act == Act::Rearing {
            return;
        }
        if self.jumping {
            self.jumping = false;
            self.gravity = -3.5;
            self.act = Act::Gallop;
        }
        if kind_front {
            self.force_reverse = true;
        } else {
            self.force_walk = true;
        }
        if galloping {
            self.start_rearing();
        }
    }

    fn walk_floor(&self, world: &CollisionWorld, x: f64, z: f64, y_ref: f64) -> FloorProbe {
        let hit = world.find_floor(x, y_ref + u(60.0), z);
        match hit.surface {
            None => FloorProbe::None,
            Some(s) => {
                if world.water_level(x, z) > hit.height {
                    FloorProbe::Water
                } else if s.normal[1] < COS_35 {
                    FloorProbe::Steep(hit.height)
                } else {
                    FloorProbe::Ok(hit.height, s)
                }
            }
        }
    }

    fn bg_check(&mut self, world: &CollisionWorld) {
        // Walls (Actor_UpdateBgCheckInfo: radius 35 at height 40).
        let mut p = self.pos;
        let wall = world.resolve_and_return_wall_collisions(&mut p, u(40.0), u(23.0));
        if let Some(w) = wall {
            self.pos[0] = p[0];
            self.pos[2] = p[2];
            let wall_yaw = atan2s(w.normal[2], w.normal[0]) as f64;
            if cos_angle(wall_yaw - self.yaw) < -0.3 && self.speed > 4.0 {
                self.speed -= 1.0 * DT;
                self.sand_dust_tick();
            }
            self.obstacle = true;
        }
        // Floor under her origin.
        let fh = world.find_floor(self.pos[0], self.pos[1] + u(60.0), self.pos[2]);
        let floor_h = if fh.surface.is_some() { fh.height } else { FLOOR_LOWER_LIMIT };
        let floor_h = if self.jumping { self.landing_floor(world) } else { floor_h };
        if self.jumping {
            // Airborne by the animation: no snapping to what is below (act_jump lands her).
        } else if fh.surface.is_some() && self.pos[1] <= floor_h {
            self.pos[1] = floor_h;
            self.on_ground = true;
            if !self.jumping {
                self.vel_y = 0.0;
            }
        } else {
            self.on_ground = false;
        }
        self.floor_surface = fh.surface.clone();
        if !self.riding() && !self.act_follow_like() {
            return;
        }
        // Jump triggers (only under the rider's control and on the ground).
        if self.riding() && !self.jumping && self.speed >= 0.0 && !matches!(self.act, Act::Stopping | Act::Rearing) {
            if self.speed > 8.0 && self.check_obstacle_jump(world) {
                return;
            }
        }
        self.check_floors(world, floor_h);
    }

    fn act_follow_like(&self) -> bool {
        self.act == Act::Follow
    }

    fn sand_dust_tick(&mut self) {
        if self.sand_cool <= 0.0 {
            self.event(EponaEvent::SandDust);
            self.sand_cool = 6.0;
        }
    }

    /// `EnHorse_CheckFloors`. `floor_h` is the floor under her origin.
    fn check_floors(&mut self, world: &CollisionWorld, floor_h: f64) {
        let galloping = self.speed > 8.0;
        if self.jumping {
            // In the air: landing is handled by the jump itself (a gap has no floor to probe).
            self.pitch = 0.0;
            return;
        }
        // Water under her.
        if world.water_level(self.pos[0], self.pos[2]) > floor_h {
            self.obstruct(galloping, true);
            return;
        }
        let front = forward(self.yaw, u(30.0));
        let back = forward(self.yaw, -u(30.0));
        let (fx, fz) = (self.pos[0] + front[0], self.pos[2] + front[2]);
        let (bx, bz) = (self.pos[0] + back[0], self.pos[2] + back[2]);
        let ff = self.walk_floor(world, fx, fz, self.pos[1]);
        let FloorProbe::Ok(y_front, front_surface) = ff else {
            if let FloorProbe::Steep(_) | FloorProbe::None | FloorProbe::Water = ff {
                if !self.try_gap_jump(world) {
                    self.obstruct(galloping, true);
                }
            }
            return;
        };
        let bf = self.walk_floor(world, bx, bz, self.pos[1]);
        let FloorProbe::Ok(y_back, back_surface) = bf else {
            self.obstruct(galloping, false);
            return;
        };
        self.y_front = y_front;
        self.y_back = y_back;
        if let Some(cur) = self.floor_surface.clone() {
            let plane = |s: &crate::collision::Surface, x: f64, y: f64, z: f64| s.normal[0] * x + s.normal[1] * y + s.normal[2] * z + s.origin_offset;
            let same_front = std::sync::Arc::ptr_eq(&cur, &front_surface);
            if !same_front && self.speed >= 0.0 {
                let dist = plane(&cur, fx, y_front, fz);
                if (!self.jumping && dist < -u(40.0)) || (self.jumping && dist < -u(200.0)) {
                    if !self.try_gap_jump(world) {
                        self.obstruct(galloping, true);
                    }
                    return;
                }
            }
            let same_back = std::sync::Arc::ptr_eq(&cur, &back_surface);
            let dist = plane(&cur, bx, y_back, bz);
            if (!same_back && self.speed <= 0.0 && !self.jumping && dist < -u(40.0)) || (self.jumping && dist < -u(200.0)) {
                self.obstruct(galloping, false);
                return;
            }
            if cur.normal[1] < COS_35 {
                self.obstruct(galloping, self.speed >= 0.0);
                return;
            }
        }
        if self.jumping {
            self.pitch = 0.0;
            return;
        }
        let floor_slope = (y_back - y_front).atan2(u(60.0));
        if self.pos[1] > floor_h + u(4.0) {
            self.pitch = 0.0;
        } else if floor_slope.abs() <= 8191.0 * std::f64::consts::PI / 32768.0 {
            // nose up when the front is higher
            self.pitch = floor_slope;
        }
        let _ = &front_surface;
    }

    /// Gap jump (extension): at a gallop with a landing 100..330 units further on.
    fn try_gap_jump(&mut self, world: &CollisionWorld) -> bool {
        if !self.riding() || self.jumping || self.speed < 8.0 || self.no_gap_jump > 0.0 {
            return false;
        }
        let mut d = 150.0;
        while d <= 330.0 {
            let (x, z) = (self.pos[0] + sins(self.yaw) * u(d), self.pos[2] + coss(self.yaw) * u(d));
            if let FloorProbe::Ok(h, _) = self.walk_floor(world, x, z, self.pos[1]) {
                if h <= self.pos[1] + u(40.0) && h >= self.pos[1] - u(110.0) {
                    self.pos = self.last_pos;
                    self.start_jump(true, h);
                    return true;
                }
            }
            d += 30.0;
        }
        false
    }

    /// The front-wall part of `EnHorse_UpdateBgCheckInfo`: pick a low / high jump or brake.
    fn check_obstacle_jump(&mut self, world: &CollisionWorld) -> bool {
        let fast = self.speed >= 12.8;
        let reach = if fast { 230.0 } else { 160.0 };
        let start = [self.pos[0], self.pos[1] + u(19.0), self.pos[2]];
        // March along the heading looking for a wall (the original's line test).
        let step = 12.0;
        let mut d = 0.0;
        let mut hit = None;
        while d <= u(reach) {
            let mut p = [start[0] + sins(self.yaw) * d, start[1], start[2] + coss(self.yaw) * d];
            if let Some(w) = world.resolve_and_return_wall_collisions(&mut p, 0.0, 18.0) {
                hit = Some((d / OOT_SCALE, w));
                break;
            }
            d += step;
        }
        let Some((dist, wall)) = hit else { return false };
        self.obstacle = true;
        let wall_yaw = atan2s(wall.normal[2], wall.normal[0]) as f64;
        if cos_angle(self.yaw - wall_yaw - 32767.0) < 0.5 {
            return false;
        }
        if (!fast && dist < 80.0) || (fast && dist < 150.0) {
            self.force_reverse = true;
            if fast {
                self.start_braking();
            }
            return false;
        }
        // The jump clips carry her ~200 / ~135 units before they land (frame 23 / 17 of the arc), so take off only once the
        // obstacle is that close, or she would come down in front of it.
        if dist > if fast { 172.0 } else { 118.0 } {
            return false;
        }
        let top_at = |off: f64| {
            let (x, z) = (self.pos[0] + sins(self.yaw) * u(off), self.pos[2] + coss(self.yaw) * u(off));
            world.find_floor(x, self.pos[1] + u(120.0), z)
        };
        let top = top_at(dist + 5.0);
        let Some(s) = top.surface.clone() else { return false };
        if top.height <= FLOOR_LOWER_LIMIT {
            return false;
        }
        let height = (top.height - self.pos[1]) / OOT_SCALE;
        if s.normal[1] < COS_35 {
            if fast {
                self.force_reverse = true;
                self.start_braking();
            }
            return false;
        }
        let behind = top_at(if fast { 390.0 } else { 276.0 });
        let Some(_) = behind.surface else { return false };
        let behind_h = (behind.height - self.pos[1]) / OOT_SCALE;
        if behind_h < -70.0 {
            if fast {
                self.force_reverse = true;
                self.start_braking();
            }
            return false;
        }
        if !fast && height > 19.0 && height <= 40.0 {
            self.start_jump(false, behind.height);
            true
        } else if (fast && self.speed < 13.8 && height > 19.0 && height <= 72.0) || (self.speed > 13.8 && height <= 112.0) {
            self.start_jump(true, behind.height);
            true
        } else {
            false
        }
    }
}

enum FloorProbe {
    None,
    Water,
    Steep(#[allow(dead_code)] f64),
    Ok(f64, std::sync::Arc<crate::collision::Surface>),
}
