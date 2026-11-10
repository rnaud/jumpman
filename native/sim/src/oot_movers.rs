//! Moving dungeon platforms (dynapoly actors): the state machines of `Bg_Mori_Elevator`, `Bg_Mori_Hashira4` (the rotating pillars
//! and the three gates), `Bg_Bdan_Objects` (the elevator, the lowering platform and the water-level changer), `Bg_Bdan_Switch`
//! (floor switches), `Bg_Jya_Lift`, `Bg_Jya_1flift`, `Bg_Mizu_Movebg` (dragon statues and floating platforms) and `Bg_Haka`
//! (the graveyard slabs Link pulls) and `Bg_Mori_Hineri` (the Forest Temple's twisted hallway).
//!
//! Engine-free. The actors run at 20 Hz; [`Movers::tick`] runs at the sim's 30 Hz and executes two actor frames per three ticks,
//! while [`Mover::pos`] interpolates between the last two frames so the collision and the model move smoothly. Heights are kept
//! in OoT units (`y`) and turned into world units (`x4.4`) only for the outputs.
use crate::math::Vec3;

pub const U: f64 = 4.4;
/// Actor frames per sim tick (20 Hz / 30 Hz).
const FRAME_PER_TICK: f64 = 20.0 / 30.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `Bg_Mori_Elevator` (single instance, reads the current room).
    ForestElevator,
    /// `Bg_Mori_Hashira4` params 0: the four platforms that turn about the pillar.
    ForestPillars,
    /// `Bg_Mori_Hashira4` params 1: a gate that rises when its switch is set.
    ForestGate,
    /// `Bg_Bdan_Objects` type 1.
    JabuElevator,
    /// `Bg_Bdan_Objects` type 3.
    JabuLowering,
    /// `Bg_Bdan_Objects` type 2: moves the water box 7.
    JabuWater,
    /// `Bg_Bdan_Switch` BLUE / YELLOW_HEAVY / YELLOW (param & 0xFF 0 / 1 / 2).
    JabuSwitch,
    /// `Bg_Jya_Lift`.
    JyaLift,
    /// `Bg_Jya_1flift`.
    Jya1fLift,
    /// `Bg_Mizu_Movebg` types 3..=6.
    WaterStatue,
    /// `Bg_Mizu_Movebg` types 0..=2: follows the water.
    WaterFloat,
    /// `Bg_Haka`: a gravestone Link pulls out of its grave.
    Grave,
    /// `Bg_Mori_Hineri` (the persistent hall, bit 14 clear): the model turns about its length as Link walks along it.
    ForestHineri,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "forest_elevator" => Kind::ForestElevator,
            "forest_pillars" => Kind::ForestPillars,
            "forest_gate" => Kind::ForestGate,
            "jabu_elevator" => Kind::JabuElevator,
            "jabu_lowering" => Kind::JabuLowering,
            "jabu_water" => Kind::JabuWater,
            "jabu_switch" => Kind::JabuSwitch,
            "jya_lift" => Kind::JyaLift,
            "jya_1flift" => Kind::Jya1fLift,
            "water_statue" => Kind::WaterStatue,
            "water_float" => Kind::WaterFloat,
            "grave" => Kind::Grave,
            "forest_hineri" => Kind::ForestHineri,
            _ => return None,
        })
    }

    /// At most one of these exists at a time (`sIsSpawned`).
    fn single(self) -> bool {
        matches!(self, Kind::ForestElevator | Kind::JyaLift | Kind::Jya1fLift)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// `Flags_SetSwitch` / `Flags_UnsetSwitch`.
    Set(i32),
    Unset(i32),
    /// A sound effect by `NA_SE_` name without the prefix.
    Sfx(&'static str),
    /// A message box by id (`Message_StartTextbox`).
    Message(u16),
}

/// What the world tells the movers each tick.
pub struct World<'a> {
    /// Link's feet, world units.
    pub player: Vec3,
    /// `play->roomCtx.curRoom.num`.
    pub room: Option<i32>,
    /// Link stands on mover `i` (`DYNA_INTERACT_PLAYER_ON_TOP`), or something heavy does (a switch).
    pub on_top: &'a [bool],
    /// A switch is pressed by a block or Link.
    pub pressed: &'a [bool],
    /// `Flags_GetSwitch`.
    pub flag: &'a dyn Fn(i32) -> bool,
    pub adult: bool,
    /// The Water Temple's water surface `waterBoxes[2].ySurface`, OoT units.
    pub water_y: Option<f64>,
    /// Link's hold on a mover: `(index, force)` with +1 pushing, -1 pulling (`dyna.unk_150`).
    pub grip: Option<(usize, i8)>,
    /// `!IS_DAY`.
    pub night: bool,
    /// The course id (`graveyard`, `lake_hylia`, ...).
    pub scene: &'a str,
}

#[derive(Clone, Debug)]
pub struct Mover {
    pub kind: Kind,
    /// Placement, world units.
    pub home: Vec3,
    pub param: i32,
    pub room: i32,
    /// Placement yaw, degrees.
    pub yaw0: f64,
    /// Height of the actor, OoT units (last frame / frame before).
    pub y: f64,
    prev_y: f64,
    /// Rotation about Y added to the placement yaw, degrees.
    pub turn: f64,
    prev_turn: f64,
    /// The mover is gone (a gate that finished rising).
    pub gone: bool,
    /// Visual lowering of a pressed switch, OoT units (not part of the collision actor in the game: the switch squashes).
    switch_c8: f64,
    home_y: f64,
    vel: f64,
    target: f64,
    timer: i32,
    state: u8,
    was_top: bool,
    was_flag: bool,
    hold: i32,
    dir_down: bool,
    /// How far a pulled slab has come out of its grave, OoT units (and the frame before).
    pub dist: f64,
    prev_dist: f64,
    /// `Bg_Mori_Hineri`: which of the four halls it is (`params`: 0 / 1 and 2 / 3 are the two before / after pairs) and how far
    /// the model has turned about its length, degrees.
    pub variant: u8,
    pub roll: f64,
}

fn flag_param(p: i32, shift: u32) -> i32 {
    (p >> shift) & 0x3F
}

/// `func_808A1800` of the forest elevator.
fn approach(value: &mut f64, target: f64, scale: f64, max_step: f64, min_step: f64) -> f64 {
    let mut var = (target - *value) * scale;
    if *value < target {
        if max_step < var {
            var = max_step;
        } else if var < min_step {
            var = min_step;
        }
        *value += var;
        if target < *value {
            *value = target;
        }
    } else if target < *value {
        if var < -max_step {
            var = -max_step;
        } else if -min_step < var {
            var = -min_step;
        }
        *value += var;
        if *value < target {
            *value = target;
        }
    } else {
        var = 0.0;
    }
    var
}

/// `Math_SmoothStepToF`.
pub fn smooth_step(value: &mut f64, target: f64, fraction: f64, step: f64, min_step: f64) -> f64 {
    if *value != target {
        let mut s = (target - *value) * fraction;
        if s >= min_step || s <= -min_step {
            s = s.min(step).max(-step);
            *value += s;
        } else {
            if s < min_step {
                *value += min_step;
                s = min_step;
                if target < *value {
                    *value = target;
                }
            }
            if s > -min_step {
                *value += -min_step;
                if *value < target {
                    *value = target;
                }
            }
        }
    }
    (target - *value).abs()
}

/// `Math_StepToF`: true when the value arrived.
fn step_to(value: &mut f64, target: f64, step: f64) -> bool {
    if *value == target {
        return true;
    }
    if *value < target {
        *value = (*value + step).min(target);
    } else {
        *value = (*value - step).max(target);
    }
    *value == target
}

const ELEVATOR_UP: f64 = 233.0;
const ELEVATOR_DOWN: f64 = -779.0;
const ELEVATOR_MID: f64 = -275.0;

impl Mover {
    /// `pos` is the placement in world units, `yaw` in degrees.
    pub fn new(kind: Kind, pos: Vec3, yaw: f64, param: i32, room: i32, flag: &dyn Fn(i32) -> bool, adult: bool, water_y: Option<f64>) -> Self {
        let y = pos[1] / U;
        let mut m = Mover {
            kind,
            home: pos,
            param,
            room,
            yaw0: yaw,
            y,
            prev_y: y,
            turn: 0.0,
            prev_turn: 0.0,
            gone: false,
            switch_c8: 1.0,
            home_y: y,
            vel: 0.0,
            target: y,
            timer: 0,
            state: 0,
            was_top: false,
            was_flag: false,
            hold: 0,
            dir_down: false,
            dist: 0.0,
            prev_dist: 0.0,
            variant: 0,
            roll: 0.0,
        };
        match kind {
            Kind::ForestElevator => {
                // `WaitAfterInit`: with the switch already set the elevator starts at 73 (room 2 only)
                if flag(flag_param(param, 0)) {
                    m.y = 73.0;
                }
                m.was_flag = flag(flag_param(param, 0));
            }
            Kind::ForestGate => {
                let sw = flag_param(param, 8);
                if flag(sw) {
                    m.gone = true;
                }
            }
            Kind::JabuElevator => m.timer = 512,
            Kind::JabuLowering => {
                if flag(flag_param(param, 8)) {
                    m.y = y - 400.0;
                    m.state = 9;
                }
            }
            Kind::ForestPillars | Kind::JabuWater | Kind::Grave => {}
            Kind::ForestHineri => {
                // `BgMoriHineri_Init`: `params` bit 15 picks the pair, the switch flag (low 6 bits) the half of it
                let base = if param & 0x8000 != 0 { 2 } else { 0 };
                m.variant = base + u8::from(flag(param & 0x3F));
            }
            Kind::JabuSwitch => {
                let on = flag(flag_param(param, 8));
                m.switch_c8 = if on { 0.1 } else { 1.0 };
                m.state = if on { 2 } else { 0 };
                m.hold = 6;
            }
            Kind::JyaLift => {
                if flag(flag_param(param, 0)) {
                    m.y = 973.0;
                    m.state = 9;
                } else {
                    m.y = 1613.0;
                }
            }
            Kind::Jya1fLift => {
                m.y = 443.0;
                if flag(flag_param(param, 0)) {
                    if adult {
                        m.dir_down = true;
                        m.state = 2; // Move
                    } else {
                        m.state = 9; // DoNothing
                    }
                }
            }
            Kind::WaterStatue => {
                let t = (param >> 12) & 15;
                m.y = y + if t == 3 { -115.2 } else if flag(param & 0x3F) { 115.2 } else { 0.0 };
            }
            Kind::WaterFloat => {
                let t = (param >> 12) & 15;
                let drop = if t == 1 { 710.0 } else { 700.0 };
                m.y = water_y.map_or(y, |w| (w + 15.0).max(y - drop));
            }
        }
        m.prev_y = m.y;
        m.target = m.y;
        m
    }

    /// Current place, world units, interpolated between the last two actor frames by `frac` (0..1).
    fn pos_at(&self, frac: f64) -> Vec3 {
        let y = self.prev_y + (self.y - self.prev_y) * frac;
        let y = if self.kind == Kind::JabuSwitch { y - (1.0 - self.switch_c8) * 12.0 } else { y };
        // a pulled slab comes out toward the back of its placement yaw (`world.rot.y = shape.rot.y + 0x8000`)
        let d = (self.prev_dist + (self.dist - self.prev_dist) * frac) * U;
        let a = self.yaw0.to_radians();
        [self.home[0] - a.sin() * d, y * U, self.home[2] - a.cos() * d]
    }

    /// Yaw in degrees at `frac`.
    fn yaw_at(&self, frac: f64) -> f64 {
        self.yaw0 + self.prev_turn + (self.turn - self.prev_turn) * frac
    }

    fn near(&self, w: &World, range: f64) -> bool {
        (w.player[0] - self.home[0]).hypot(w.player[2] - self.home[2]) < range * U
    }

    /// One 20 Hz actor frame.
    fn frame(&mut self, i: usize, w: &World, ev: &mut Vec<Event>, flag: &dyn Fn(i32) -> bool) {
        self.prev_y = self.y;
        self.prev_turn = self.turn;
        self.prev_dist = self.dist;
        let top = w.on_top.get(i).copied().unwrap_or(false);
        let pressed = w.pressed.get(i).copied().unwrap_or(top);
        match self.kind {
            Kind::ForestElevator => {
                let sw = flag_param(self.param, 0);
                match self.state {
                    // SetPosition
                    0 => {
                        let riding = top && !self.was_top && w.player[1] / U - self.y < 80.0;
                        let room = w.room;
                        // Stepping on it sends it to the other end: the game reads the current room (2 at the top of the shaft,
                        // 17 at the bottom); the shaft's own height says the same without depending on the room boxes.
                        if riding {
                            self.target = if self.y > ELEVATOR_MID { ELEVATOR_DOWN } else { ELEVATOR_UP };
                            self.state = 1;
                            self.vel = 0.0;
                        } else if !top && room == Some(2) && self.y < ELEVATOR_MID {
                            self.target = ELEVATOR_UP;
                            self.state = 1;
                            self.vel = 0.0;
                        } else if !top && room == Some(17) && ELEVATOR_MID < self.y {
                            self.target = ELEVATOR_DOWN;
                            self.state = 1;
                            self.vel = 0.0;
                        } else if !top && room == Some(2) && !flag(sw) && self.was_flag {
                            self.target = ELEVATOR_UP;
                            self.state = 2;
                        }
                    }
                    // StopMovement -> func_808A2008: ride to `target`
                    1 => {
                        approach(&mut self.vel, 12.0, 0.1, 1.0, 0.0);
                        let d = approach(&mut self.y, self.target, 0.1, self.vel, 0.3);
                        if d.abs() < 0.001 {
                            self.state = 0;
                            ev.push(Event::Sfx("EV_ELEVATOR_STOP"));
                        } else {
                            ev.push(Event::Sfx("EV_ELEVATOR_MOVE2"));
                        }
                    }
                    // MoveAboveGround (the switch was turned off)
                    _ => {
                        approach(&mut self.vel, 2.0, 0.05, 1.0, 0.0);
                        let d = approach(&mut self.y, ELEVATOR_UP, 0.08, self.vel, 1.5);
                        if d.abs() < 0.001 {
                            self.state = 0;
                            ev.push(Event::Sfx("EV_ELEVATOR_STOP"));
                        }
                    }
                }
                self.was_top = top;
                self.was_flag = flag(sw);
            }
            Kind::ForestPillars => {
                // `shape.rot.y += 0x96`
                self.turn += 150.0 * 360.0 / 65536.0;
            }
            Kind::ForestGate => {
                if self.gone {
                    return;
                }
                let sw = flag_param(self.param, 8);
                if self.state == 0 {
                    if flag(sw) || self.timer != 0 {
                        self.timer += 1;
                        if self.timer > 30 {
                            ev.push(Event::Sfx("EV_METALDOOR_OPEN"));
                            self.state = 1;
                        }
                    }
                } else if step_to(&mut self.y, self.home_y + 120.0, 10.0) {
                    self.gone = true;
                }
            }
            Kind::JabuElevator => {
                if self.timer != 0 {
                    self.timer -= 1;
                }
                self.y = self.home_y - (f64::from(self.timer) * std::f64::consts::PI / 256.0).sin() * 471.24;
                if self.timer == 0 {
                    self.timer = 512;
                }
            }
            Kind::JabuLowering => {
                let sw = flag_param(self.param, 8);
                match self.state {
                    0 => {
                        if top {
                            ev.push(Event::Set(sw));
                            self.timer = 50;
                            self.state = 1;
                            self.home_y -= 200.0;
                        }
                    }
                    1 => {
                        if self.timer != 0 {
                            self.timer -= 1;
                        }
                        self.y = self.home_y - (f64::from(self.timer) * std::f64::consts::PI / 50.0).cos() * 200.0;
                        if self.timer == 0 {
                            ev.push(Event::Sfx("EV_BUYOSTAND_STOP_U"));
                            self.state = 9;
                        } else {
                            ev.push(Event::Sfx("EV_BUYOSTAND_FALL"));
                        }
                    }
                    _ => {}
                }
            }
            Kind::JabuWater => {
                let sw = flag_param(self.param, 8);
                match self.state {
                    // WaitForSwitch
                    0 => {
                        if flag(sw) {
                            self.timer = 100;
                            self.state = 1;
                        }
                    }
                    // ChangeWaterBoxLevel
                    1 => {
                        if self.timer == 0 {
                            if step_to(&mut self.y, self.home_y, 0.5) {
                                ev.push(Event::Unset(sw));
                                self.state = 0;
                            }
                        } else if step_to(&mut self.y, self.home_y + 75.0, 0.5) {
                            self.state = 2;
                        }
                        ev.push(Event::Sfx("EV_WATER_LEVEL_DOWN"));
                    }
                    // WaitForTimerExpired
                    _ => {
                        if self.timer != 0 {
                            self.timer -= 1;
                        }
                        if self.timer == 0 {
                            self.state = 1;
                        }
                    }
                }
            }
            Kind::JabuSwitch => self.switch_frame(pressed, top, flag, ev),
            Kind::JyaLift => {
                let sw = flag_param(self.param, 0);
                match self.state {
                    0 => {
                        if flag(sw) || self.timer > 0 {
                            self.timer += 1;
                            if self.timer >= 20 {
                                self.state = 1;
                                self.vel = 0.0;
                            }
                        }
                    }
                    1 => {
                        smooth_step(&mut self.vel, 4.0, 0.1, 1.0, 0.0);
                        let v = if self.vel < 0.2 { 0.2 } else { self.vel };
                        let d = smooth_step(&mut self.y, 973.0, 0.1, v, 0.2);
                        if d.abs() < 0.001 {
                            self.y = 973.0;
                            self.state = 9;
                            ev.push(Event::Sfx("EV_ELEVATOR_STOP"));
                        } else {
                            ev.push(Event::Sfx("EV_BRIDGE_OPEN"));
                        }
                    }
                    _ => {}
                }
            }
            Kind::Jya1fLift => {
                const FINAL: [f64; 2] = [443.0, -50.0];
                // only runs in the rooms the lift lives in
                if !matches!(w.room, Some(0) | Some(6) | None) {
                    return;
                }
                let sw = flag_param(self.param, 0);
                match self.state {
                    // WaitForSwitch
                    0 => {
                        if flag(sw) {
                            self.dir_down = !self.dir_down;
                            self.vel = 0.0;
                            self.state = 2;
                        }
                    }
                    // DelayMove
                    1 => {
                        self.timer += 1;
                        if self.timer >= 21 {
                            self.dir_down = !self.dir_down;
                            self.vel = 0.0;
                            self.state = 2;
                        }
                    }
                    // Move
                    2 => {
                        step_to(&mut self.vel, 6.0, 0.4);
                        let v = if self.vel < 1.0 { 1.0 } else { self.vel };
                        let target = FINAL[usize::from(self.dir_down)];
                        if smooth_step(&mut self.y, target, 0.5, v, 1.0).abs() < 0.001 {
                            self.y = target;
                            self.timer = 0;
                            self.state = 1;
                            ev.push(Event::Sfx("EV_BLOCK_BOUND"));
                        } else {
                            ev.push(Event::Sfx("EV_ELEVATOR_MOVE3"));
                        }
                    }
                    _ => {}
                }
            }
            Kind::WaterStatue => {
                let t = (self.param >> 12) & 15;
                let target = if t == 3 {
                    // the flags of the three water levels pick the offset (`BgMizuMovebg_GetDragonStatueBossRoomOffsetIndex`)
                    let idx = if flag(0x1C) { 1 } else if flag(0x1D) { 2 } else if flag(0x1E) { 3 } else { 1 };
                    self.home_y + if idx == 3 { 0.0 } else { -115.2 }
                } else {
                    self.home_y + if flag(self.param & 0x3F) { 115.2 } else { 0.0 }
                };
                if !step_to(&mut self.y, target, 1.0) {
                    ev.push(Event::Sfx("EV_ELEVATOR_MOVE"));
                }
            }
            Kind::ForestHineri => {
                let base = if self.param & 0x8000 != 0 { 2 } else { 0 };
                // the halls are reloaded with the room: the new variant shows when Link is away (`BgMoriHineri_Init` reads the flag)
                let want = base + u8::from(flag(self.param & 0x3F));
                if want != self.variant && (w.player[0] - self.home[0]).hypot(w.player[2] - self.home[2]) > 1500.0 * U {
                    self.variant = want;
                }
                // `func_808A3C8C` (variants 0 and 3): `rot.z = CLAMP(1100 - (player.z - actor.z), 0, 1000) * 16.384`, mirrored for 3
                self.roll = 0.0;
                if self.variant == 0 || self.variant == 3 {
                    let f = (1100.0 - (w.player[2] - self.home[2]) / U).clamp(0.0, 1000.0);
                    let turn = f * 16.384 * 360.0 / 65536.0;
                    self.roll = if self.variant == 0 { turn } else { -turn };
                }
            }
            Kind::Grave => {
                let force = w.grip.filter(|g| g.0 == i).map_or(0, |g| g.1);
                match self.state {
                    // IdleClosed
                    0 => {
                        if force != 0 {
                            let graveyard = w.scene == "graveyard";
                            if graveyard && !w.adult && !w.night {
                                // Graveyard Boy's warning, then a 100-frame lock
                                ev.push(Event::Message(0x5073));
                                self.timer = 100;
                                self.state = 3;
                            } else if force > 0 || (w.scene == "lake_hylia" && !w.adult && !flag(0x23)) {
                                // pushing does nothing; the Lake Hylia grave stays shut for the child until the switch is set
                            } else {
                                self.vel = 0.0;
                                self.state = 1;
                            }
                        }
                    }
                    // Pull: speed 0.05 per frame up to 1.5, 60 units out
                    1 => {
                        self.vel = (self.vel + 0.05).min(1.5);
                        let done = step_to(&mut self.dist, 60.0, self.vel);
                        ev.push(Event::Sfx("EV_ROCK_SLIDE"));
                        if done {
                            if self.param & 0xFF == 1 {
                                ev.push(Event::Sfx("SY_CORRECT_CHIME"));
                            }
                            self.state = 2;
                        }
                    }
                    // IdleLockedClosed
                    3 => {
                        if self.timer > 0 {
                            self.timer -= 1;
                        }
                        if self.timer == 0 {
                            self.state = 0;
                        }
                    }
                    _ => {}
                }
            }
            Kind::WaterFloat => {
                let t = (self.param >> 12) & 15;
                let drop = if t == 1 { 710.0 } else { 700.0 };
                if let Some(water) = w.water_y {
                    self.y = (water + 15.0).max(self.home_y - drop);
                }
            }
        }
    }

    /// `Bg_Bdan_Switch` for the floor types: the head lowers by 0.2 steps once pressed and rises when released (blue) or when
    /// the flag is cleared elsewhere (yellow). The heavy yellow one (needs Ruto in the hands) only sinks to 0.6.
    fn switch_frame(&mut self, pressed: bool, top: bool, flag: &dyn Fn(i32) -> bool, ev: &mut Vec<Event>) {
        let ty = self.param & 0xFF;
        let sw = flag_param(self.param, 8);
        match self.state {
            // up, waiting
            0 => {
                self.switch_c8 = 1.0;
                let hit = if ty == 0 { pressed } else { top };
                if hit {
                    if ty == 1 {
                        self.state = 5;
                    } else {
                        self.state = 1;
                        if !flag(sw) {
                            ev.push(Event::Set(sw));
                            ev.push(Event::Sfx("SY_CORRECT_CHIME"));
                        }
                    }
                }
            }
            // sinking
            1 => {
                self.switch_c8 -= 0.2;
                if self.switch_c8 <= 0.1 {
                    self.switch_c8 = 0.1;
                    self.state = 2;
                    self.hold = 6;
                    ev.push(Event::Sfx("EV_FOOT_SWITCH"));
                }
            }
            // down
            2 => {
                if ty == 0 {
                    if !pressed {
                        if self.hold <= 0 {
                            self.state = 3;
                            if flag(sw) {
                                ev.push(Event::Unset(sw));
                            }
                        } else {
                            self.hold -= 1;
                        }
                    } else {
                        self.hold = 6;
                    }
                } else if !flag(sw) {
                    self.state = 3;
                }
            }
            // rising
            3 => {
                self.switch_c8 += 0.2;
                if self.switch_c8 >= 1.0 {
                    self.switch_c8 = 1.0;
                    self.state = 0;
                    ev.push(Event::Sfx("EV_FOOT_SWITCH"));
                }
            }
            // heavy: sinks to 0.6 and holds
            5 => {
                self.switch_c8 -= 0.2;
                if self.switch_c8 <= 0.6 {
                    self.switch_c8 = 0.6;
                    self.state = 6;
                }
            }
            _ => {
                if !top {
                    self.switch_c8 = 1.0;
                    self.state = 0;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Movers {
    pub list: Vec<Mover>,
    acc: f64,
}

impl Movers {
    /// Build from placements; the single-instance kinds keep their first placement only.
    pub fn new(list: Vec<Mover>) -> Self {
        let mut seen: Vec<Kind> = vec![];
        let list = list
            .into_iter()
            .filter(|m| {
                if !m.kind.single() {
                    return true;
                }
                if seen.contains(&m.kind) {
                    return false;
                }
                seen.push(m.kind);
                true
            })
            .collect();
        Self { list, acc: 0.0 }
    }

    /// One 30 Hz tick. Returns the events of the actor frames that ran.
    pub fn tick(&mut self, w: &World) -> Vec<Event> {
        let mut ev = vec![];
        self.acc += FRAME_PER_TICK;
        // local switch changes are visible to the next frame of the same tick
        let set = std::cell::Cell::new(0u64);
        let unset = std::cell::Cell::new(0u64);
        while self.acc >= 1.0 {
            self.acc -= 1.0;
            let flag = |f: i32| -> bool {
                if !(0..64).contains(&f) {
                    return false;
                }
                let bit = 1u64 << f;
                (((w.flag)(f)) || set.get() & bit != 0) && unset.get() & bit == 0
            };
            for i in 0..self.list.len() {
                let mut frame_events = vec![];
                self.list[i].frame(i, w, &mut frame_events, &flag);
                for e in &frame_events {
                    match *e {
                        Event::Set(f) if (0..64).contains(&f) => {
                            set.set(set.get() | 1 << f);
                            unset.set(unset.get() & !(1 << f));
                        }
                        Event::Unset(f) if (0..64).contains(&f) => {
                            unset.set(unset.get() | 1 << f);
                            set.set(set.get() & !(1 << f));
                        }
                        _ => {}
                    }
                }
                ev.extend(frame_events);
            }
        }
        ev
    }

    /// Where mover `i` is now (world units), `None` once it is gone.
    pub fn pos(&self, i: usize) -> Option<Vec3> {
        let m = self.list.get(i)?;
        (!m.gone).then(|| m.pos_at(self.acc))
    }

    pub fn yaw(&self, i: usize) -> f64 {
        self.list.get(i).map_or(0.0, |m| m.yaw_at(self.acc))
    }

    /// How far the twisted hallway's model has turned (degrees) and which hall it is.
    pub fn roll(&self, i: usize) -> f64 {
        self.list.get(i).map_or(0.0, |m| m.roll)
    }

    pub fn variant(&self, i: usize) -> u8 {
        self.list.get(i).map_or(0, |m| m.variant)
    }

    /// The water surface the water-level changer wants (world units), if one is placed.
    pub fn jabu_water(&self) -> Option<f64> {
        self.list.iter().find(|m| m.kind == Kind::JabuWater).map(|m| m.y * U)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: i32) -> bool {
        false
    }

    fn world<'a>(player: Vec3, room: Option<i32>, on_top: &'a [bool], flag: &'a dyn Fn(i32) -> bool) -> World<'a> {
        World { player, room, on_top, pressed: on_top, flag, adult: true, water_y: None, grip: None, night: false, scene: "" }
    }

    fn run(m: &mut Movers, ticks: usize, player: Vec3, room: Option<i32>, top: &[bool], flag: &dyn Fn(i32) -> bool) -> Vec<Event> {
        let mut all = vec![];
        for _ in 0..ticks {
            all.extend(m.tick(&world(player, room, top, flag)));
        }
        all
    }

    #[test]
    fn the_approach_helper_ramps_and_stops_on_target() {
        let mut y = 0.0;
        let d = approach(&mut y, 10.0, 0.1, 1.0, 0.3);
        assert!((d - 1.0).abs() < 1e-9 && (y - 1.0).abs() < 1e-9, "capped at maxStep");
        let mut y = 9.9;
        approach(&mut y, 10.0, 0.1, 1.0, 0.3);
        assert!((y - 10.0).abs() < 1e-9, "never overshoots");
        let mut y = 10.0;
        assert_eq!(approach(&mut y, 10.0, 0.1, 1.0, 0.3), 0.0);
    }

    #[test]
    fn the_forest_elevator_carries_link_down_from_the_main_room_and_up_again_from_the_basement() {
        let mut m = Movers::new(vec![Mover::new(Kind::ForestElevator, [0.0, 233.0 * U, 0.0], 0.0, 27, 2, &none, true, None)]);
        // idle with nobody on it
        run(&mut m, 30, [3000.0, 0.0, 0.0], Some(2), &[false], &none);
        assert!((m.list[0].y - 233.0).abs() < 1e-9);
        // Link steps on it in room 2: it descends to -779
        run(&mut m, 2, [0.0, 233.0 * U, 0.0], Some(2), &[true], &none);
        let mut last = m.list[0].y;
        let mut monotone = true;
        for _ in 0..1500 {
            // the room changes as Link sinks below the main hall
            m.tick(&world([0.0, last * U, 0.0], Some(if last < ELEVATOR_MID { 17 } else { 2 }), &[true], &none));
            monotone &= m.list[0].y <= last + 1e-9;
            last = m.list[0].y;
            if (last - ELEVATOR_DOWN).abs() < 1e-6 {
                break;
            }
        }
        assert!(monotone && (last - ELEVATOR_DOWN).abs() < 1e-6, "{last}");
        // in room 17 stepping on it sends it back up
        run(&mut m, 2, [0.0, ELEVATOR_DOWN * U, 0.0], Some(17), &[false], &none);
        run(&mut m, 2, [0.0, ELEVATOR_DOWN * U, 0.0], Some(17), &[true], &none);
        // (the room becomes the main hall again as it rises)
        run(&mut m, 1500, [0.0, 0.0, 0.0], Some(2), &[true], &none);
        assert!((m.list[0].y - ELEVATOR_UP).abs() < 1e-6, "{}", m.list[0].y);
    }

    #[test]
    fn an_empty_elevator_below_the_midpoint_returns_when_link_is_in_the_main_room() {
        let mut m = Movers::new(vec![Mover::new(Kind::ForestElevator, [0.0, ELEVATOR_DOWN * U, 0.0], 0.0, 27, 17, &none, true, None)]);
        run(&mut m, 1500, [0.0; 3], Some(2), &[false], &none);
        assert!((m.list[0].y - ELEVATOR_UP).abs() < 1e-6, "{}", m.list[0].y);
    }

    #[test]
    fn only_one_elevator_exists() {
        let a = Mover::new(Kind::ForestElevator, [0.0, 233.0 * U, 0.0], 0.0, 27, 2, &none, true, None);
        let b = Mover::new(Kind::ForestElevator, [0.0, ELEVATOR_DOWN * U, 0.0], 0.0, 27, 17, &none, true, None);
        let m = Movers::new(vec![a, b]);
        assert_eq!(m.list.len(), 1);
        assert_eq!(m.list[0].room, 2);
    }

    #[test]
    fn the_forest_pillars_turn_and_the_gate_rises_30_frames_after_its_switch() {
        let pillars = Mover::new(Kind::ForestPillars, [0.0, 0.0, 0.0], 0.0, -256, 14, &none, true, None);
        let gate = Mover::new(Kind::ForestGate, [100.0, 0.0, 0.0], 0.0, 0x1001, 17, &none, true, None);
        let mut m = Movers::new(vec![pillars, gate]);
        run(&mut m, 31, [0.0; 3], Some(17), &[false, false], &none);
        // 31 ticks = 20 frames of 150 binary degrees
        let turned = m.list[0].turn;
        assert!((turned - 20.0 * 150.0 * 360.0 / 65536.0).abs() < 1e-6, "{turned}");
        assert_eq!(m.list[1].y, 0.0, "the gate waits for the switch");
        let on = |f: i32| f == 16;
        run(&mut m, 30, [0.0; 3], Some(17), &[false, false], &on);
        // 31 frames of delay + 12 frames of rising (120 / 10) = 43 frames = 65 ticks
        run(&mut m, 60, [0.0; 3], Some(17), &[false, false], &on);
        assert!(m.list[1].gone, "{:?}", m.list[1].y);
        assert!(m.pos(1).is_none());
        // a gate whose switch is already set never appears
        let gone = Mover::new(Kind::ForestGate, [0.0; 3], 0.0, 0x1001, 17, &on, true, None);
        assert!(gone.gone);
    }

    #[test]
    fn the_jabu_elevator_swings_the_full_471_units_and_the_lowering_platform_drops_400_when_stepped_on() {
        let mut m = Movers::new(vec![
            Mover::new(Kind::JabuElevator, [0.0, 100.0 * U, 0.0], 0.0, 0xFF01, 1, &none, true, None),
            Mover::new(Kind::JabuLowering, [500.0, 0.0, 0.0], 0.0, 0x1D03, 1, &none, true, None),
        ]);
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for _ in 0..1000 {
            m.tick(&world([9e5; 3], Some(1), &[false, false], &none));
            lo = lo.min(m.list[0].y);
            hi = hi.max(m.list[0].y);
        }
        assert!(hi > 100.0 + 470.0 && lo < 100.0 - 470.0, "{lo} {hi}");
        let ev = run(&mut m, 5, [500.0, 0.0, 0.0], Some(1), &[false, true], &none);
        assert!(ev.contains(&Event::Set(0x1D)));
        run(&mut m, 120, [500.0, 0.0, 0.0], Some(1), &[false, false], &|f| f == 0x1D);
        assert!((m.list[1].y + 400.0).abs() < 1e-6, "{}", m.list[1].y);
    }

    #[test]
    fn the_jabu_water_level_changer_raises_the_water_75_waits_and_lowers_it_again() {
        let sw = |f: i32| f == 7;
        let mut m = Movers::new(vec![Mover::new(Kind::JabuWater, [0.0, 1000.0, 0.0], 0.0, 0x0700, 1, &none, true, None)]);
        let base = m.jabu_water().unwrap();
        let mut top = base;
        let mut cleared = false;
        for _ in 0..3000 {
            let ev = m.tick(&world([0.0; 3], Some(1), &[false], &sw));
            top = top.max(m.jabu_water().unwrap());
            if ev.contains(&Event::Unset(7)) {
                cleared = true;
                break;
            }
        }
        assert!(cleared, "the switch is cleared once the water is back down");
        assert!((top - base - 75.0 * U).abs() < 1e-6, "{}", top - base);
        assert!((m.jabu_water().unwrap() - base).abs() < 1e-6);
    }

    #[test]
    fn the_blue_jabu_switch_stays_set_while_pressed_and_releases_after_a_short_delay() {
        let mut m = Movers::new(vec![Mover::new(Kind::JabuSwitch, [0.0, 0.0, 0.0], 0.0, 0x3A00, 0, &none, true, None)]);
        let ev = run(&mut m, 10, [0.0; 3], Some(0), &[true], &none);
        assert!(ev.contains(&Event::Set(0x3A)));
        let ev = run(&mut m, 30, [0.0; 3], Some(0), &[false], &|f| f == 0x3A);
        assert!(ev.contains(&Event::Unset(0x3A)), "{ev:?}");
        // a yellow one latches
        let mut y = Movers::new(vec![Mover::new(Kind::JabuSwitch, [0.0, 0.0, 0.0], 0.0, 0x3902, 0, &none, true, None)]);
        let ev = run(&mut y, 10, [0.0; 3], Some(0), &[true], &none);
        assert!(ev.contains(&Event::Set(0x39)));
        let ev = run(&mut y, 60, [0.0; 3], Some(0), &[false], &|f| f == 0x39);
        assert!(!ev.iter().any(|e| matches!(e, Event::Unset(_))));
    }

    #[test]
    fn the_spirit_lift_waits_20_frames_after_its_switch_then_settles_at_973() {
        let on = |f: i32| f == 5;
        let mut m = Movers::new(vec![Mover::new(Kind::JyaLift, [0.0, 0.0, 0.0], 0.0, 5, 5, &none, true, None)]);
        assert_eq!(m.list[0].y, 1613.0);
        run(&mut m, 60, [0.0; 3], Some(5), &[false], &none);
        assert_eq!(m.list[0].y, 1613.0);
        run(&mut m, 3000, [0.0; 3], Some(5), &[false], &on);
        assert_eq!(m.list[0].y, 973.0);
        // with the switch already set it starts at the bottom
        let done = Mover::new(Kind::JyaLift, [0.0; 3], 0.0, 5, 5, &on, true, None);
        assert_eq!(done.y, 973.0);
    }

    #[test]
    fn the_spirit_one_floor_lift_shuttles_between_443_and_minus_50_once_the_switch_is_set() {
        let on = |f: i32| f == 3;
        let mut m = Movers::new(vec![Mover::new(Kind::Jya1fLift, [0.0, 0.0, 0.0], 0.0, 3, 0, &none, true, None)]);
        run(&mut m, 100, [0.0; 3], Some(0), &[false], &none);
        assert_eq!(m.list[0].y, 443.0);
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for _ in 0..3000 {
            m.tick(&world([0.0; 3], Some(0), &[false], &on));
            lo = lo.min(m.list[0].y);
            hi = hi.max(m.list[0].y);
        }
        assert!(lo <= -49.9 && hi >= 442.9, "{lo} {hi}");
    }

    #[test]
    fn the_dragon_statues_sink_when_their_switch_is_off_and_rise_115_when_it_is_on() {
        let mut m = Movers::new(vec![Mover::new(Kind::WaterStatue, [0.0, 500.0, 0.0], 0.0, 0x4000 | 0x1A, 3, &none, true, None)]);
        let y0 = m.list[0].y;
        run(&mut m, 600, [0.0; 3], Some(3), &[false], &|f| f == 0x1A);
        assert!((m.list[0].y - (y0 + 115.2)).abs() < 1e-6);
        run(&mut m, 600, [0.0; 3], Some(3), &[false], &none);
        assert!((m.list[0].y - y0).abs() < 1e-6);
    }

    #[test]
    fn a_grave_slab_comes_out_60_units_toward_the_back_of_its_yaw_when_pulled() {
        let mut m = Movers::new(vec![Mover::new(Kind::Grave, [100.0, 0.0, 200.0], 0.0, 0, 1, &none, true, None)]);
        let w = |grip: Option<(usize, i8)>, adult: bool, night: bool, scene: &'static str| World { player: [0.0; 3], room: None, on_top: &[false], pressed: &[false], flag: &none, adult, water_y: None, grip, night, scene };
        // pushing does nothing
        for _ in 0..60 {
            m.tick(&w(Some((0, 1)), true, false, "graveyard"));
        }
        assert_eq!(m.list[0].dist, 0.0);
        // pulling: speed 0.05 per frame up to 1.5, so 60 units take 0.05+0.10+...: more than 40 frames
        let mut ticks = 0;
        let mut chimed = false;
        while m.list[0].dist < 60.0 && ticks < 400 {
            chimed |= m.tick(&w(Some((0, -1)), true, false, "graveyard")).contains(&Event::Sfx("SY_CORRECT_CHIME"));
            ticks += 1;
        }
        assert!(ticks > 40 && ticks < 120, "{ticks} ticks");
        assert!(!chimed, "only the grave of the royal family tomb chimes");
        let p = m.pos(0).unwrap();
        // yaw 0 faces +z: the slab comes out toward -z, 60 units
        assert_eq!(m.list[0].dist, 60.0);
        // (the shown pose is interpolated: at most one frame, 1.5 units, behind)
        assert!((p[2] - (200.0 - 60.0 * U)).abs() < 1.6 * U && (p[0] - 100.0).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn the_graveyard_boy_stops_a_child_pulling_by_day_but_not_at_night_or_as_an_adult() {
        let mk = || Movers::new(vec![Mover::new(Kind::Grave, [0.0; 3], 0.0, 0, 1, &none, false, None)]);
        let pull = |m: &mut Movers, adult: bool, night: bool, scene: &'static str| {
            let mut ev = vec![];
            for _ in 0..90 {
                ev.extend(m.tick(&World { player: [0.0; 3], room: None, on_top: &[false], pressed: &[false], flag: &none, adult, water_y: None, grip: Some((0, -1)), night, scene }));
            }
            ev
        };
        let mut day_child = mk();
        let ev = pull(&mut day_child, false, false, "graveyard");
        assert!(ev.contains(&Event::Message(0x5073)) && day_child.list[0].dist == 0.0);
        let mut night_child = mk();
        pull(&mut night_child, false, true, "graveyard");
        assert!(night_child.list[0].dist > 0.0);
        let mut adult = mk();
        pull(&mut adult, true, false, "graveyard");
        assert!(adult.list[0].dist > 0.0);
        // the Lake Hylia grave stays shut for the child until switch 0x23
        let mut lake = mk();
        pull(&mut lake, false, false, "lake_hylia");
        assert_eq!(lake.list[0].dist, 0.0);
    }

    #[test]
    fn the_twisted_hall_turns_a_quarter_as_link_walks_its_1000_units_and_swaps_when_its_switch_flips_behind_him() {
        let home = [0.0, 0.0, 0.0];
        let on = |f: i32| f == 0x20;
        // the hall of room 19: bit 15 clear (variant 0), switch 0x20, bit 14 clear
        let mut m = Movers::new(vec![Mover::new(Kind::ForestHineri, home, 0.0, 0x0320, 19, &none, true, None)]);
        assert_eq!(m.variant(0), 0);
        let at = |z_oot: f64| World { player: [0.0, 0.0, z_oot * U], room: Some(19), on_top: &[false], pressed: &[false], flag: &none, adult: true, water_y: None, grip: None, night: false, scene: "" };
        m.tick(&at(2000.0));
        m.tick(&at(2000.0));
        assert_eq!(m.roll(0), 0.0, "Link 1100 units or more before the hall's origin: not turned");
        for _ in 0..3 {
            m.tick(&at(0.0));
        }
        assert!((m.roll(0) - 1000.0 * 16.384 * 360.0 / 65536.0).abs() < 1e-6, "{}", m.roll(0));
        assert!((m.roll(0) - 90.0).abs() < 0.1, "a quarter turn");
        for _ in 0..3 {
            m.tick(&at(600.0));
        }
        assert!((m.roll(0) - 500.0 * 16.384 * 360.0 / 65536.0).abs() < 1e-6, "halfway down the hall: half the turn");
        // the switch flips while he is in the hall: nothing changes until he has gone
        let flipped = |z_oot: f64| World { player: [0.0, 0.0, z_oot * U], room: Some(19), on_top: &[false], pressed: &[false], flag: &on, adult: true, water_y: None, grip: None, night: false, scene: "" };
        m.tick(&flipped(600.0));
        m.tick(&flipped(600.0));
        assert_eq!(m.variant(0), 0);
        m.tick(&flipped(3000.0));
        m.tick(&flipped(3000.0));
        assert_eq!(m.variant(0), 1, "the boss key room: it does not turn any more");
        assert_eq!(m.roll(0), 0.0);
        // the other pair: bit 15 set starts as variant 2 (still), and 3 turns the opposite way
        let mut n = Movers::new(vec![Mover::new(Kind::ForestHineri, home, 0.0, 0x8421, 20, &|f| f == 0x21, true, None)]);
        assert_eq!(n.variant(0), 3);
        for _ in 0..3 {
            n.tick(&at(0.0));
        }
        assert!(n.roll(0) < -89.0, "{}", n.roll(0));
    }

    #[test]
    fn positions_are_interpolated_between_actor_frames() {
        let mut m = Movers::new(vec![Mover::new(Kind::JabuElevator, [0.0, 0.0, 0.0], 0.0, 0xFF01, 1, &none, true, None)]);
        let mut prev = m.pos(0).unwrap()[1];
        let mut steps = 0;
        for _ in 0..30 {
            m.tick(&world([9e5; 3], Some(1), &[false], &none));
            let p = m.pos(0).unwrap()[1];
            if (p - prev).abs() > 1e-9 {
                steps += 1;
            }
            prev = p;
        }
        assert!(steps >= 28, "moves on almost every tick, not in 20 Hz jumps ({steps})");
    }
}
