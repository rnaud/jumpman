//! The ride itself, engine-free: the MK64 kart (`experiments::kart_physics`) stepped over whatever collision world
//! the level has, the driver's on-foot state parked while it lasts, and the rescue when the kart leaves the level.
use crate::experiments::kart_physics::{Floor, Ground, Input as KartInput, Kart, Wall, WallHit};
use crate::experiments::kart_surface::ASPHALT;
use crate::game::runner::GameSim;
use jumpman_sim::collision::{CollisionWorld, Surface, WallData};
use jumpman_sim::link::moves::{PoseHint, ARG_RIDE};
use jumpman_sim::math::{to_s16, Vec3 as V3};
use jumpman_sim::state::*;
use jumpman_sim::surfaces::*;
use std::collections::VecDeque;

/// World units per MK64 course unit (the same scale the kart mode loads its tracks with).
pub const UNIT: f64 = 8.;
/// `action_arg` that marks "sitting in the summoned kart" (the action itself is the butt slide, whose pose every
/// character already has: Mario's slide, Banjo's `slideFront`; Link gets a riding pose hint).
pub const ARG_KART: f64 = 0x4B41 as f64;
/// The driver's origin above the kart's floor point. Under 20, so everything that asks "is the character standing
/// on this floor" (an Ocarina exit, a loading zone) still says yes from the seat; the model is drawn on the seat
/// pan a little higher (`view::SEAT_DRAWN`).
pub const SEAT: f64 = 16.;
/// The kart climbs steps up to this high (world units); anything taller is a wall or a different storey.
const STEP_UP: f64 = 80.;
/// `find_floor` reports floors up to this far above the queried height.
const FLOOR_BUFFER: f64 = 78.;
/// A 150cc kart tops out near 5.9 course units a frame (94 world units a tick, three times Mario's run). Levels
/// built for walking get a calmer one: capped at 4.5 a frame, 72 units a tick, a little over twice a run.
pub const SPEED_CAP: f64 = 4.5;
/// Safe positions remembered (one every [`SAFE_EVERY`] ticks): the rescue puts the kart on the oldest.
const SAFE_KEPT: usize = 12;
const SAFE_EVERY: u32 = 5;
/// Ticks in the air before the rescue gives up on a landing (5 s).
const MAX_AIR: u32 = 150;

/// The level's collision as the kart's ground. `ceil` is the highest floor the kart may be put on this step.
pub struct LevelGround<'a> {
    pub world: &'a CollisionWorld,
    pub ceil: f64,
}

impl LevelGround<'_> {
    pub fn around<'a>(world: &'a CollisionWorld, kart: &Kart) -> LevelGround<'a> {
        LevelGround { world, ceil: kart.pos[1] * UNIT + STEP_UP }
    }
}

impl Ground for LevelGround<'_> {
    fn floor(&self, x: f64, y: f64, z: f64) -> Option<Floor> {
        let y = (y * UNIT).min(self.ceil - FLOOR_BUFFER);
        let hit = self.world.find_floor(x * UNIT, y, z * UNIT);
        let s = hit.surface?;
        // Other games' surface types mean nothing to MK64's surface table: every floor drives like road.
        Some(Floor { height: hit.height / UNIT, normal: [s.normal[0], s.normal[1], s.normal[2]], surface: ASPHALT })
    }
    fn wall(&self, pos: [f64; 3]) -> Option<Wall> {
        let mut data = WallData { x: pos[0] * UNIT, y: pos[1] * UNIT, z: pos[2] * UNIT, offset_y: 50., radius: 65., walls: vec![] };
        if self.world.find_wall_collisions(&mut data, false) == 0 {
            return None;
        }
        Some(Wall {
            pos: [data.x / UNIT, data.y / UNIT, data.z / UNIT],
            hits: data.walls.iter().map(|s| WallHit { normal: [s.normal[0], s.normal[1], s.normal[2]], x_facing: s.flags & SURFACE_FLAG_X_PROJECTION != 0 }).collect(),
        })
    }
}

/// Floors the kart must not stay on (the on-foot character would burn, sink or die there).
fn hazard(s: &Surface) -> bool {
    matches!(s.kind, SURFACE_BURNING | SURFACE_DEATH_PLANE | SURFACE_DEEP_QUICKSAND | SURFACE_INSTANT_QUICKSAND | SURFACE_INSTANT_MOVING_QUICKSAND)
}

/// Why a ride stopped on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Something else took the driver's state (a hit, a door, a cutscene, a grab, death): they are on foot in it.
    Interrupted,
    /// The driver was moved somewhere else (warp, room change): they stand there now.
    Teleported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    Driving,
    /// The kart left the level (void, lava, water, a fall with no end) and was put back.
    Rescued,
    Ended(End),
}

/// One-off things the view and the sound answer (dust, squash, splash, bump).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fx {
    /// Back on the ground after this many ticks in the air.
    Landed(u32),
    /// Drove into (or out of) water shallow enough to drive through.
    Splash,
    /// Hit a wall.
    Bump,
}

#[derive(Clone, Debug)]
pub struct Ride {
    pub kart: Kart,
    /// Where the kart was called (world units, floor level) and its heading there.
    origin: (V3, i32),
    safe: VecDeque<(V3, i32)>,
    /// The driver position written last tick; anything else moving him shows up as a difference.
    expected: V3,
    ticks: u32,
    air_ticks: u32,
    lost_ticks: u32,
    /// Front wheel steer (-1..1) for the view.
    pub steer: f64,
    /// Throttle held this tick (engine note).
    pub accel: bool,
    /// The wheels are in water the kart can drive through.
    pub wading: bool,
    /// Since the view last looked (bounded; see [`Ride::note`]).
    pub fx: Vec<Fx>,
}

/// Whether the character can get in right now, or why not.
pub fn can_summon(sim: &GameSim) -> Result<(), &'static str> {
    let m = &sim.mario;
    let group = m.action & ACT_GROUP_MASK;
    let grounded = group == ACT_GROUP_STATIONARY || group == ACT_GROUP_MOVING;
    if !grounded || m.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE | ACT_FLAG_SWIMMING) != 0 {
        return Err("Stand on solid ground to call the kart");
    }
    if sim.as_link && m.action == ACT_IDLE && m.action_arg == ARG_RIDE {
        return Err("Get off the horse first");
    }
    if m.held_obj.is_some() || m.riding_shell.is_some() || m.grabbed_by.is_some() || m.cannon.is_some() {
        return Err("Hands full: no kart right now");
    }
    if m.health < 256. {
        return Err("No kart right now");
    }
    let hit = m.world.find_floor(m.pos[0], m.pos[1] + 2., m.pos[2]);
    match hit.surface {
        Some(s) if !hazard(&s) && m.pos[1] - hit.height < 60. && m.world.water_level(m.pos[0], m.pos[2]) < hit.height + 40. => Ok(()),
        _ => Err("No room for the kart here"),
    }
}

impl Ride {
    /// Put the character in a kart where they stand.
    pub fn summon(sim: &mut GameSim) -> Result<Ride, &'static str> {
        can_summon(sim)?;
        let m = &sim.mario;
        let floor = m.world.find_floor(m.pos[0], m.pos[1] + 2., m.pos[2]).height;
        let pos = [m.pos[0], floor, m.pos[2]];
        let rotation = to_s16(-m.face_angle[1]);
        let mut ride = Ride { kart: new_kart(pos, rotation), origin: (pos, rotation), safe: VecDeque::new(), expected: pos, ticks: 0, air_ticks: 0, lost_ticks: 0, steer: 0., accel: false, wading: false, fx: vec![] };
        ride.seat_driver(sim);
        Ok(ride)
    }

    /// Kart floor point in world units.
    pub fn pos(&self) -> V3 {
        self.kart.pos.map(|v| v * UNIT)
    }
    /// Heading as an SM64 face angle.
    pub fn yaw(&self) -> f64 {
        to_s16(-self.kart.rotation as f64) as f64
    }
    /// World units per 30 Hz tick.
    pub fn speed(&self) -> f64 {
        self.kart.speed * UNIT * 2.
    }
    pub fn grounded(&self) -> bool {
        !self.kart.midair
    }
    pub fn ticks(&self) -> u32 {
        self.ticks
    }
    /// Speed over the capped top speed, 0..1 (a little over on a downhill).
    pub fn speed_ratio(&self) -> f64 {
        self.kart.speed / SPEED_CAP
    }
    fn note(&mut self, fx: Fx) {
        if self.fx.len() < 8 {
            self.fx.push(fx);
        }
    }

    /// Hold the driver in the seat: position, heading, the sitting action, nothing else moving.
    fn seat_driver(&mut self, sim: &mut GameSim) {
        let (pos, yaw, speed) = (self.pos(), self.yaw(), self.speed());
        let backwards = self.kart.back_up;
        let as_link = sim.as_link;
        let ticks = self.ticks;
        let m = &mut sim.mario;
        m.pos = [pos[0], pos[1] + SEAT, pos[2]];
        m.vel = [0.; 3];
        m.face_angle = [0., yaw, 0.];
        m.forward_vel = if backwards { -speed } else { speed };
        // Assigned, not `set_action`: nothing of the slide's own start-up (sounds, timers) should run.
        m.action = ACT_BUTT_SLIDE;
        m.action_arg = ARG_KART;
        m.action_state = 0.;
        m.action_timer = 0.;
        m.peak_height = m.pos[1];
        let hit = m.world.find_floor(pos[0], pos[1] + 2., pos[2]);
        m.floor_height = hit.height;
        m.floor = hit.surface;
        if as_link {
            m.link.pose = Some(PoseHint { clip: "uma_wait1", frame: ticks as f64, looping: true });
        }
        self.expected = m.pos;
        sim.skip_mario_step = true;
    }

    /// One 30 Hz tick, before the simulation's own: drive the kart with the player's controller and park the
    /// character in it (the caller hands the simulation an empty controller for this tick).
    pub fn pre_tick(&mut self, sim: &mut GameSim, c: &ControllerSnapshot) -> Tick {
        {
            let m = &mut sim.mario;
            if m.action != ACT_BUTT_SLIDE || m.action_arg != ARG_KART || m.health < 256. {
                m.link.pose = None;
                return Tick::Ended(End::Interrupted);
            }
            let d = [m.pos[0] - self.expected[0], m.pos[1] - self.expected[1], m.pos[2] - self.expected[2]];
            let moved = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if moved > 300. {
                self.stand_up(sim, false);
                return Tick::Ended(End::Teleported);
            }
            if moved > 1e-6 {
                // A platform or a current carried the driver: the kart goes with him.
                for i in 0..3 {
                    self.kart.pos[i] += d[i] / UNIT;
                }
            }
        }
        self.ticks += 1;
        let world = sim.mario.world.clone();
        let before = self.kart.pos;
        let pad = KartInput {
            raw_x: (c.stick_x * 80. / 64.).round() as i32,
            raw_y: (c.stick_y * 80. / 64.).round() as i32,
            accel: c.stick_y > 16.,
            brake: c.button_down & Z_TRIG != 0 || c.stick_y < -16.,
            hop_down: c.button_down & A_BUTTON != 0,
            hop_pressed: c.button_pressed & A_BUTTON != 0,
        };
        let (was_air, was_bumped, air) = (self.kart.midair, self.kart.bumped, self.air_ticks);
        self.kart.step(&pad, &LevelGround::around(&world, &self.kart));
        self.kart.step(&KartInput { hop_pressed: false, ..pad }, &LevelGround::around(&world, &self.kart));
        self.steer += ((c.stick_x / 64.).clamp(-1., 1.) - self.steer) * 0.4;
        self.accel = pad.accel;
        if was_air && !self.kart.midair && air >= 2 {
            self.note(Fx::Landed(air));
        }
        if self.kart.bumped && !was_bumped {
            self.note(Fx::Bump);
        }

        let rescued = self.check_bounds(sim, &world, before);
        let p = self.pos();
        let wading = !rescued && world.water_level(p[0], p[2]) > p[1] + 4.;
        if wading != self.wading {
            self.wading = wading;
            self.note(Fx::Splash);
        }
        self.seat_driver(sim);
        if sim.as_banjo {
            // Banjo's own step is skipped, and his notes, eggs, feathers and honeycombs are picked up inside it:
            // run just that overlap from the seat. (SM64 coins and OoT drops are collected by the simulation's own
            // position checks, which run whoever is driving.)
            jumpman_sim::banjo_ext::collect_pickups(&mut sim.mario);
        }
        if rescued { Tick::Rescued } else { Tick::Driving }
    }

    /// The level's edges: no floor at all is a wall (as it is on foot); the void, lava, sand that swallows, deep
    /// water and an endless fall put the kart back where it last drove safely.
    fn check_bounds(&mut self, sim: &GameSim, world: &CollisionWorld, before: [f64; 3]) -> bool {
        let p = self.pos();
        let hit = world.find_floor(p[0], p[1] + 2., p[2]);
        let outside = p[0].abs() >= world.horizontal_boundary || p[2].abs() >= world.horizontal_boundary;
        let rescue;
        match &hit.surface {
            None => {
                // Bonk: back to where there was ground, stopped.
                self.kart.pos = before;
                self.kart.velocity = [0.; 3];
                self.kart.speed = 0.;
                self.kart.current_speed = 0.;
                self.lost_ticks += 1;
                rescue = self.lost_ticks > 10;
            }
            Some(s) => {
                self.lost_ticks = 0;
                let void = s.kind == SURFACE_DEATH_PLANE && p[1] < hit.height + 2048.;
                let on_hazard = self.grounded() && hazard(s);
                let drowned = world.water_level(p[0], p[2]) > p[1] + 70.;
                rescue = void || on_hazard || drowned;
                if !rescue && self.grounded() && self.ticks % SAFE_EVERY == 0 && world.water_level(p[0], p[2]) < p[1] + 20. {
                    self.safe.push_back((p, self.kart.rotation));
                    if self.safe.len() > SAFE_KEPT {
                        self.safe.pop_front();
                    }
                }
            }
        }
        self.air_ticks = if self.grounded() { 0 } else { self.air_ticks + 1 };
        if !(rescue || outside || p[1] < sim.mario.death_height || self.air_ticks > MAX_AIR) {
            return false;
        }
        // The oldest remembered spot is two seconds back: far enough from the edge the kart just went over.
        let (pos, rotation) = self.safe.front().copied().unwrap_or(self.origin);
        self.safe.clear();
        self.safe.push_back((pos, rotation));
        self.kart = new_kart(pos, rotation);
        self.air_ticks = 0;
        self.lost_ticks = 0;
        true
    }

    /// The driver back on foot where the kart is (or wherever something just moved him to, for `at_kart == false`).
    fn stand_up(&self, sim: &mut GameSim, at_kart: bool) {
        let m = &mut sim.mario;
        m.link.pose = None;
        let grounded = if at_kart {
            let p = self.pos();
            m.pos = p;
            m.face_angle = [0., self.yaw(), 0.];
            self.grounded()
        } else {
            let hit = m.world.find_floor(m.pos[0], m.pos[1] + 2., m.pos[2]);
            hit.surface.is_some() && m.pos[1] - hit.height < 40.
        };
        m.vel = [0.; 3];
        m.forward_vel = 0.;
        m.peak_height = m.pos[1];
        // Out of the kart's marker state first, so `set_action` sees a plain slide ending.
        m.action_arg = 0.;
        m.set_action(if grounded { ACT_IDLE } else { ACT_FREEFALL }, 0.);
    }

    /// The player parks the kart: the character stands where it was.
    pub fn dismiss(self, sim: &mut GameSim) {
        let m = &sim.mario;
        if m.action == ACT_BUTT_SLIDE && m.action_arg == ARG_KART {
            self.stand_up(sim, true);
        } else {
            sim.mario.link.pose = None;
        }
    }
}

fn new_kart(pos: V3, rotation: i32) -> Kart {
    let mut kart = Kart::new(pos.map(|v| v / UNIT), rotation);
    let mut stats = kart.stats;
    stats.speed_cap = SPEED_CAP;
    kart.set_stats(stats);
    kart
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::CourseRequest;
    use jumpman_sim::collision::SurfaceOptions;
    use std::path::PathBuf;

    fn flat(kind: i32, half: f64) -> CollisionWorld {
        let mut world = CollisionWorld::new(1024., 100000.);
        let o = SurfaceOptions { kind, ..Default::default() };
        world.add_triangle([-half, 0., -half], [-half, 0., half], [half, 0., half], &o);
        world.add_triangle([-half, 0., -half], [half, 0., half], [half, 0., -half], &o);
        world
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Who {
        Mario,
        Link,
        Banjo,
    }

    fn load_as(id: &str, who: Who) -> GameSim {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let mut sim = GameSim::load(&root, CourseRequest::new(id)).unwrap();
        sim.as_link = who == Who::Link;
        sim.as_banjo = who == Who::Banjo;
        sim
    }

    fn load(id: &str) -> GameSim {
        load_as(id, Who::Mario)
    }

    fn pad(stick_x: f64, stick_y: f64) -> ControllerSnapshot {
        ControllerSnapshot { stick_x, stick_y, stick_mag: stick_x.hypot(stick_y).min(64.), ..Default::default() }
    }

    /// The app's tick while driving: the kart gets the controller, the simulation an empty one.
    fn drive(sim: &mut GameSim, ride: &mut Ride, c: &ControllerSnapshot) -> Tick {
        let t = ride.pre_tick(sim, c);
        sim.tick(&ControllerSnapshot::default());
        t
    }

    fn settle(sim: &mut GameSim) {
        for _ in 0..40 {
            sim.tick(&ControllerSnapshot::default());
        }
    }

    /// A kart alone on a collision world (no character): `n` ticks of full throttle.
    fn run(world: &CollisionWorld, kart: &mut Kart, n: usize) {
        let go = KartInput { raw_x: 0, raw_y: 80, accel: true, brake: false, hop_down: false, hop_pressed: false };
        for _ in 0..n * 2 {
            kart.step(&go, &LevelGround::around(world, kart));
        }
    }

    #[test]
    fn the_kart_stays_on_a_floor_and_is_faster_than_running_but_not_wild() {
        let world = flat(0, 60000.);
        let mut kart = new_kart([0., 0., 0.], 0);
        run(&world, &mut kart, 150);
        let a = kart.pos[2] * UNIT;
        run(&world, &mut kart, 30);
        let per_tick = (kart.pos[2] * UNIT - a) / 30.;
        assert!(!kart.midair && kart.pos[1].abs() < 0.5, "kart left the floor: {:?}", kart.pos);
        // Mario runs at 32 units a tick.
        assert!((55. ..90.).contains(&per_tick), "top speed {per_tick} units/tick");
    }

    #[test]
    fn a_wall_stops_the_kart() {
        let mut world = flat(0, 20000.);
        // A wall across the kart's way at z = 3000, facing it.
        let o = SurfaceOptions::default();
        world.add_triangle([-4000., -200., 3000.], [4000., -200., 3000.], [4000., 1500., 3000.], &o);
        world.add_triangle([-4000., -200., 3000.], [4000., 1500., 3000.], [-4000., 1500., 3000.], &o);
        world.add_triangle([4000., -200., 3000.], [-4000., -200., 3000.], [4000., 1500., 3000.], &o);
        world.add_triangle([4000., 1500., 3000.], [-4000., -200., 3000.], [-4000., 1500., 3000.], &o);
        let mut kart = new_kart([0., 0., 0.], 0);
        run(&world, &mut kart, 240);
        let z = kart.pos[2] * UNIT;
        assert!(z > 2000. && z < 3000., "kart at z = {z}");
    }

    #[test]
    fn a_ledge_taller_than_a_step_is_not_climbed() {
        let mut world = flat(0, 20000.);
        // A platform 150 units up starting at z = 2000, with no wall under its edge.
        let o = SurfaceOptions::default();
        world.add_triangle([-4000., 150., 2000.], [-4000., 150., 9000.], [4000., 150., 9000.], &o);
        world.add_triangle([-4000., 150., 2000.], [4000., 150., 9000.], [4000., 150., 2000.], &o);
        let mut kart = new_kart([0., 0., 0.], 0);
        run(&world, &mut kart, 120);
        assert!(kart.pos[1] * UNIT < 10., "the kart popped onto the platform: {:?}", kart.pos);
    }

    /// Summon in a real level, drive, and check the kart is on that level's floor with its driver seated.
    fn drives(id: &str, who: Who) {
        let mut sim = load_as(id, who);
        settle(&mut sim);
        let start = sim.mario.pos;
        let mut ride = Ride::summon(&mut sim).unwrap_or_else(|e| panic!("{id}: {e}"));
        let mut far: f64 = 0.;
        let mut grounded = 0;
        // Throttle with a slow weave, so a wall in front of the spawn does not end the trip.
        for i in 0..300 {
            let steer = if (i / 60) % 2 == 0 { 20. } else { -34. };
            let t = drive(&mut sim, &mut ride, &pad(steer, 64.));
            assert!(!matches!(t, Tick::Ended(_)), "{id}: the ride ended at tick {i}: {t:?} (action {:#x})", sim.mario.action);
            let p = ride.pos();
            assert!(p.iter().all(|v| v.is_finite()), "{id}: kart at {p:?}");
            // Solid objects may push the driver (and with him the kart, next tick); nothing else moves him.
            let m = sim.mario.pos;
            assert!((m[0] - p[0]).hypot(m[2] - p[2]) < 60. && (m[1] - p[1] - SEAT).abs() < 60., "{id}: the driver at {m:?} is not in the kart at {p:?}");
            assert_eq!(sim.mario.action, ACT_BUTT_SLIDE);
            assert_eq!(sim.mario.link.pose.is_some(), who == Who::Link, "{id}: Link (and only Link) gets the riding pose");
            if ride.grounded() {
                grounded += 1;
                let floor = sim.mario.world.find_floor(p[0], p[1] + 2., p[2]);
                assert!(floor.surface.is_some() && (floor.height - p[1]).abs() < 12., "{id}: kart at {p:?}, floor {}", floor.height);
            }
            far = far.max((p[0] - start[0]).hypot(p[2] - start[2]));
        }
        assert!(grounded > 150, "{id}: grounded only {grounded} of 300 ticks");
        assert!(far > 400., "{id}: the kart only got {far} units away");
        // And out again: on foot at the kart, in an ordinary state that the simulation keeps running.
        let p = ride.pos();
        ride.dismiss(&mut sim);
        assert_eq!(sim.mario.pos, p);
        assert!(matches!(sim.mario.action, ACT_IDLE | ACT_FREEFALL));
        assert!(sim.mario.link.pose.is_none());
        settle(&mut sim);
        assert_ne!(sim.mario.action, ACT_BUTT_SLIDE, "{id}");
        assert!(sim.mario.health >= 256., "{id}");
    }

    #[test]
    fn the_kart_drives_in_an_sm64_course() {
        drives("bob", Who::Mario);
    }

    #[test]
    fn the_kart_drives_in_an_oot_scene() {
        drives("kokiri_forest", Who::Link);
    }

    #[test]
    fn the_kart_drives_in_an_oot_dungeon() {
        drives("deku_tree", Who::Link);
    }

    #[test]
    fn the_kart_drives_in_a_bk_level() {
        drives("bk_mumbos_mountain", Who::Banjo);
        drives("bk_spiral_mountain", Who::Banjo);
    }

    #[test]
    fn the_kart_picks_up_what_the_driver_touches() {
        // SM64: a coin where the kart stands is collected by the simulation's own check, from the seat.
        let mut sim = load("bob");
        settle(&mut sim);
        let mut ride = Ride::summon(&mut sim).unwrap();
        let coins = sim.coins;
        sim.spawn_coin(ride.pos(), false, 1.);
        for _ in 0..3 {
            drive(&mut sim, &mut ride, &ControllerSnapshot::default());
        }
        assert_eq!(sim.coins, coins + 1);

        // Banjo-Kazooie: notes and eggs are taken inside Banjo's own (skipped) step; the ride runs that overlap.
        use jumpman_sim::banjo_ext::{BkPickup, PickupKind};
        let mut sim = load_as("bk_mumbos_mountain", Who::Banjo);
        settle(&mut sim);
        let mut ride = Ride::summon(&mut sim).unwrap();
        let p = ride.pos();
        let (notes, eggs, placed) = (sim.mario.banjo.notes, sim.mario.banjo.eggs, sim.mario.banjo.pickups.len());
        sim.mario.banjo.pickups.push(BkPickup { kind: PickupKind::Note, pos: [p[0], p[1] + 40., p[2]], collected: false });
        sim.mario.banjo.pickups.push(BkPickup { kind: PickupKind::Egg, pos: [p[0] + 30., p[1] + 40., p[2]], collected: false });
        // One far away stays.
        sim.mario.banjo.pickups.push(BkPickup { kind: PickupKind::Note, pos: [p[0] + 900., p[1] + 40., p[2]], collected: false });
        drive(&mut sim, &mut ride, &ControllerSnapshot::default());
        assert_eq!(sim.mario.banjo.notes, notes + 1);
        assert!(sim.mario.banjo.eggs > eggs);
        assert_eq!(sim.mario.banjo.pickups.len(), placed + 1);
    }

    #[test]
    fn landings_bumps_and_water_are_reported_once_each() {
        let mut sim = load("bob");
        settle(&mut sim);
        let mut ride = Ride::summon(&mut sim).unwrap();
        // A hop: up, then one landing.
        drive(&mut sim, &mut ride, &ControllerSnapshot { button_down: A_BUTTON, button_pressed: A_BUTTON, ..pad(0., 64.) });
        for _ in 0..40 {
            drive(&mut sim, &mut ride, &pad(0., 64.));
        }
        let landings = ride.fx.iter().filter(|f| matches!(f, Fx::Landed(_))).count();
        assert!(landings <= 1, "{:?}", ride.fx);
        // Nobody reading them does not let them pile up.
        for _ in 0..200 {
            ride.note(Fx::Bump);
        }
        assert!(ride.fx.len() <= 8);
    }

    #[test]
    fn summon_and_dismiss_round_trip_the_character() {
        let mut sim = load("bob");
        settle(&mut sim);
        let (pos, yaw, health, action) = (sim.mario.pos, sim.mario.face_angle[1], sim.mario.health, sim.mario.action);
        let ride = Ride::summon(&mut sim).unwrap();
        assert_eq!((sim.mario.action, sim.mario.action_arg), (ACT_BUTT_SLIDE, ARG_KART));
        assert!(sim.skip_mario_step);
        ride.dismiss(&mut sim);
        assert!((sim.mario.pos[0] - pos[0]).abs() < 1e-9 && (sim.mario.pos[2] - pos[2]).abs() < 1e-9 && (sim.mario.pos[1] - pos[1]).abs() < 60.);
        assert_eq!(to_s16(sim.mario.face_angle[1]), to_s16(yaw));
        assert_eq!(sim.mario.health, health);
        assert_eq!(sim.mario.action, action);
        assert_eq!(sim.mario.action_arg, 0.);
        // The on-foot controller is live again.
        for _ in 0..30 {
            sim.tick(&pad(0., 64.));
        }
        assert!((sim.mario.pos[0] - pos[0]).hypot(sim.mario.pos[2] - pos[2]) > 100.);
    }

    #[test]
    fn the_on_foot_controller_is_suspended_while_driving() {
        let mut sim = load("bob");
        settle(&mut sim);
        let mut ride = Ride::summon(&mut sim).unwrap();
        // No throttle; the simulation is even handed a full stick and a jump: the driver must not move or jump.
        for _ in 0..30 {
            assert_eq!(ride.pre_tick(&mut sim, &ControllerSnapshot::default()), Tick::Driving);
            let seat = sim.mario.pos;
            sim.tick(&ControllerSnapshot { stick_y: 64., stick_mag: 64., button_down: A_BUTTON, button_pressed: A_BUTTON, ..Default::default() });
            assert_eq!(sim.mario.pos, seat);
            assert_eq!((sim.mario.action, sim.mario.action_arg), (ACT_BUTT_SLIDE, ARG_KART));
        }
    }

    #[test]
    fn something_else_taking_the_driver_ends_the_ride() {
        let mut sim = load("bob");
        settle(&mut sim);
        let mut ride = Ride::summon(&mut sim).unwrap();
        drive(&mut sim, &mut ride, &pad(0., 64.));
        // A hit (any other action) knocks him out of the kart, in that state.
        sim.mario.set_action(ACT_BACKWARD_GROUND_KB, 0.);
        assert_eq!(ride.pre_tick(&mut sim, &pad(0., 64.)), Tick::Ended(End::Interrupted));
        assert_eq!(sim.mario.action, ACT_BACKWARD_GROUND_KB);

        // A warp (the driver suddenly far away) leaves him standing there.
        let mut ride = { settle(&mut sim); Ride::summon(&mut sim).unwrap() };
        drive(&mut sim, &mut ride, &pad(0., 64.));
        let spawn = sim.course.spawn.pos;
        sim.mario.pos = [spawn[0] + 900., spawn[1], spawn[2]];
        let there = sim.mario.pos;
        assert_eq!(ride.pre_tick(&mut sim, &pad(0., 64.)), Tick::Ended(End::Teleported));
        assert_eq!(sim.mario.pos, there);
        assert!(matches!(sim.mario.action, ACT_IDLE | ACT_FREEFALL));
    }

    /// A character on a synthetic world: Bob-omb Battlefield's simulation with its collision swapped.
    fn on(world: CollisionWorld) -> GameSim {
        let mut sim = load("bob");
        sim.mario.world = std::sync::Arc::new(world);
        sim.mario.pos = [0., 0., 0.];
        sim.mario.vel = [0.; 3];
        sim.mario.forward_vel = 0.;
        sim.mario.face_angle = [0.; 3];
        sim.mario.death_height = -8000.;
        sim.mario.set_action(ACT_IDLE, 0.);
        sim
    }

    /// Drive straight until the rescue; panics if it never comes. Returns the kart position after it.
    fn until_rescued(sim: &mut GameSim, ride: &mut Ride, ticks: usize) -> V3 {
        for _ in 0..ticks {
            // `pre_tick` alone: the swapped world has none of the course's objects to tick against.
            match ride.pre_tick(sim, &pad(0., 64.)) {
                Tick::Rescued => return ride.pos(),
                Tick::Driving => {}
                t => panic!("{t:?}"),
            }
        }
        panic!("no rescue; kart at {:?}", ride.pos());
    }

    #[test]
    fn the_edge_of_the_level_is_a_wall() {
        // Nothing beyond z = 3000: no floor at all, as outside any SM64 level.
        let mut sim = on(flat(0, 3000.));
        let mut ride = Ride::summon(&mut sim).unwrap();
        for _ in 0..400 {
            assert!(!matches!(ride.pre_tick(&mut sim, &pad(0., 64.)), Tick::Ended(_)));
            assert!(ride.pos()[2] <= 3010. && ride.pos()[1] > -8., "kart at {:?}", ride.pos());
        }
        assert!(ride.pos()[2] > 2000.);
    }

    #[test]
    fn falling_onto_a_death_plane_puts_the_kart_back() {
        let mut world = flat(0, 3000.);
        // The void under and around the island.
        let o = SurfaceOptions { kind: SURFACE_DEATH_PLANE, ..Default::default() };
        world.add_triangle([-30000., -6000., -30000.], [-30000., -6000., 30000.], [30000., -6000., 30000.], &o);
        world.add_triangle([-30000., -6000., -30000.], [30000., -6000., 30000.], [30000., -6000., -30000.], &o);
        let mut sim = on(world);
        let mut ride = Ride::summon(&mut sim).unwrap();
        let p = until_rescued(&mut sim, &mut ride, 600);
        assert!(p[2] < 3000. && p[1].abs() < 3., "rescued to {p:?}");
        assert_eq!(sim.mario.pos, [p[0], p[1] + SEAT, p[2]]);
        // And it drives on from there.
        assert_eq!(ride.pre_tick(&mut sim, &pad(0., -64.)), Tick::Driving);
    }

    #[test]
    fn lava_water_and_the_death_height_put_the_kart_back() {
        // Lava beyond z = 2000.
        let mut world = CollisionWorld::new(1024., 100000.);
        let (road, lava) = (SurfaceOptions::default(), SurfaceOptions { kind: SURFACE_BURNING, ..Default::default() });
        world.add_triangle([-3000., 0., -3000.], [-3000., 0., 2000.], [3000., 0., 2000.], &road);
        world.add_triangle([-3000., 0., -3000.], [3000., 0., 2000.], [3000., 0., -3000.], &road);
        world.add_triangle([-3000., 0., 2000.], [-3000., 0., 9000.], [3000., 0., 9000.], &lava);
        world.add_triangle([-3000., 0., 2000.], [3000., 0., 9000.], [3000., 0., 2000.], &lava);
        let mut sim = on(world);
        let mut ride = Ride::summon(&mut sim).unwrap();
        let p = until_rescued(&mut sim, &mut ride, 300);
        assert!(p[2] < 2000., "rescued to {p:?}");

        // A pool beyond z = 2000: the floor drops 400 under a water surface at -50.
        let mut world = CollisionWorld::new(1024., 100000.);
        world.add_triangle([-3000., 0., -3000.], [-3000., 0., 2000.], [3000., 0., 2000.], &road);
        world.add_triangle([-3000., 0., -3000.], [3000., 0., 2000.], [3000., 0., -3000.], &road);
        world.add_triangle([-3000., -400., 2000.], [-3000., -400., 9000.], [3000., -400., 9000.], &road);
        world.add_triangle([-3000., -400., 2000.], [3000., -400., 9000.], [3000., -400., 2000.], &road);
        world.water_boxes.push(jumpman_sim::collision::WaterBox(0., -3000., 2000., 3000., 9000., -50.));
        let mut sim = on(world);
        let mut ride = Ride::summon(&mut sim).unwrap();
        let p = until_rescued(&mut sim, &mut ride, 300);
        assert!(p[2] < 2000. && p[1].abs() < 3., "rescued to {p:?}");

        // A long drop below the course's death height.
        let mut world = CollisionWorld::new(1024., 100000.);
        world.add_triangle([-3000., 0., -3000.], [-3000., 0., 2000.], [3000., 0., 2000.], &road);
        world.add_triangle([-3000., 0., -3000.], [3000., 0., 2000.], [3000., 0., -3000.], &road);
        world.add_triangle([-3000., -20000., 2000.], [-3000., -20000., 9000.], [3000., -20000., 9000.], &road);
        world.add_triangle([-3000., -20000., 2000.], [3000., -20000., 9000.], [3000., -20000., 2000.], &road);
        let mut sim = on(world);
        let mut ride = Ride::summon(&mut sim).unwrap();
        let p = until_rescued(&mut sim, &mut ride, 400);
        assert!(p[2] < 2000. && p[1].abs() < 3., "rescued to {p:?}");
    }

    #[test]
    fn no_kart_in_the_air_in_water_or_on_lava() {
        let mut sim = on(flat(0, 3000.));
        sim.mario.pos[1] = 500.;
        sim.mario.set_action(ACT_FREEFALL, 0.);
        assert!(Ride::summon(&mut sim).is_err());
        let mut sim = on(flat(SURFACE_BURNING, 3000.));
        assert!(Ride::summon(&mut sim).is_err());
        let mut sim = on(flat(0, 3000.));
        sim.mario.held_obj = None;
        sim.mario.health = 0xFF as f64;
        assert!(Ride::summon(&mut sim).is_err());
    }
}
