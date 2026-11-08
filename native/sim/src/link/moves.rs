//! The faithful OoT Link controller layered over `characters::step_link`.
//!
//! `LinkExt` lives on `MarioRuntime::link`. While `enabled` (the app turns it on for Link and
//! Young Link) `step_link` first calls [`step`], which owns every state the legacy controller does
//! not have: sword swings with the combo / stab / spin / jump slash rules of `z_player.c`, the
//! spin-attack charge, item use (aiming the bow, slingshot, hookshot, boomerang, bombs, spells,
//! bottles, lens, ocarina), the hookshot pull and the passive effects of the boots and tunics. It
//! returns false for plain walking / rolling / guarding so the legacy (golden-tested) code keeps
//! running those.
//!
//! C buttons ride in the otherwise unused bits of `ControllerSnapshot::button_*` (N64 layout).
use super::combat::*;
use super::items::*;
use super::ocarina::{Learned, Note, Song, Staff};
use super::projectiles::*;
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::{attack_object, ObjectBehaviorKind, ATTACK_GROUND_POUND_OR_TWIRL, ATTACK_KICK_OR_TRIP, INTERACT_BREAKABLE, INTERACT_DAMAGE, INTERACT_KOOPA, INTERACT_MR_BLIZZARD};
use crate::state::*;

// N64 controller bits for the C buttons (`CONT_C*`).
pub const C_RIGHT: u32 = 0x0001;
pub const C_LEFT: u32 = 0x0002;
pub const C_DOWN: u32 = 0x0004;
pub const C_UP: u32 = 0x0008;
pub const C_MASK: u32 = C_RIGHT | C_LEFT | C_DOWN | C_UP;

/// `action_arg` values of the faithful controller (the legacy ones are 0x4c, 0x50-0x54).
pub const ARG_SWING: f64 = 0x70 as f64;
pub const ARG_CHARGE: f64 = 0x71 as f64;
pub const ARG_AIM: f64 = 0x72 as f64;
pub const ARG_ITEM: f64 = 0x73 as f64;
pub const ARG_HOOK: f64 = 0x74 as f64;
pub const ARG_OCARINA: f64 = 0x75 as f64;
pub const ARG_JUMP_FORWARD: f64 = 0x76 as f64;
pub const ARG_HOVER: f64 = 0x77 as f64;

/// Id offset for non-object targets (goombas) handed in by the app.
pub const EXTRA_TARGET_BASE: u32 = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mv {
    #[default]
    Free,
    Swing,
    Charge,
    Aim,
    /// Standing through the throw / cast animation of an item.
    ItemAnim,
    /// Flying to a hookshot anchor.
    HookFly,
    Ocarina,
}

/// The clip the renderer should play instead of the legacy choice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseHint {
    pub clip: &'static str,
    /// Animation frame in 30 Hz ticks (the exported clips run at 30 fps).
    pub frame: f64,
    pub looping: bool,
}

/// Things the app turns into sounds, toasts and world changes.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkEvent {
    Swing(Swing),
    SpinCharged,
    Shot(Kind),
    ItemUsed(Item),
    NoAmmo(Item),
    NoMagic(Item),
    WrongAge(Item),
    Explosion { pos: Vec3, radius: f64 },
    Flash { pos: Vec3, radius: f64 },
    Ignite { pos: Vec3, radius: f64 },
    Freeze { pos: Vec3, radius: f64 },
    HookAttached,
    Note(Note),
    Song(Song),
    UnknownSong,
    /// An app-level target (goomba etc., id >= [`EXTRA_TARGET_BASE`]) was hit.
    HitExtra { id: u32, dmg: Dmg, pos: Vec3 },
    /// A pickup the boomerang / hookshot fetched reached Link.
    Fetched { id: u32 },
    FaroreSet(Vec3),
    FaroreWarp(Vec3),
    NayruOn,
    LensToggled(bool),
    BeanPlanted(Vec3),
    BottleUsed(Bottle),
    FairyReleased,
    Boomerang,
    StickBroke,
    HammerSlam(Vec3),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub item: Item,
    pub yaw: f64,
    pub pitch: f64,
    /// The C bit that was pressed to start aiming; releasing it fires.
    pub bit: u32,
}

#[derive(Clone, Debug)]
pub struct LinkExt {
    /// The faithful controller replaces the legacy one for the moves it implements.
    pub enabled: bool,
    pub age: Age,
    pub inv: Inventory,
    pub learned: Learned,
    pub mv: Mv,
    pub timer: f64,
    pub swing: Option<Swing>,
    pub combo: u32,
    pub combo_dir: Direction,
    pub queued: bool,
    pub charge: Charge,
    dirs: [i8; 4],
    dir_timer: f64,
    pub weapon_stick: bool,
    pub stick_hits: u32,
    pub projectiles: ProjectileSet,
    pub aim: Option<Aim>,
    pub hook_anchor: Option<Vec3>,
    pub hook_long: bool,
    pub staff: Staff,
    pub pose: Option<PoseHint>,
    pub events: Vec<LinkEvent>,
    /// Ticks of Nayru's Love left.
    pub nayru: f64,
    pub lens: bool,
    lens_timer: f64,
    pub farore: Option<Vec3>,
    /// Hover Boots: ticks of hovering left (reset on landing).
    pub hover: f64,
    /// Position of the Z-targeted opponent, supplied by the app for aim assist.
    pub target_point: Option<Vec3>,
    /// An item action in progress: which C bit started it (for hold-to-use items).
    item_timer: f64,
    pending_item: Option<Item>,
    air_slashed: bool,
    pose_clip: Option<&'static str>,
    /// Hit-once bookkeeping for the current swing.
    swing_hit: Vec<u32>,
}

impl Default for LinkExt {
    fn default() -> Self {
        Self {
            enabled: false,
            age: Age::Adult,
            inv: Inventory::full(Age::Adult),
            learned: Learned::default(),
            mv: Mv::Free,
            timer: 0.0,
            swing: None,
            combo: 0,
            combo_dir: Direction::Right,
            queued: false,
            charge: Charge::default(),
            dirs: [-1; 4],
            dir_timer: 0.0,
            weapon_stick: false,
            stick_hits: 0,
            projectiles: ProjectileSet::new(),
            aim: None,
            hook_anchor: None,
            hook_long: false,
            staff: Staff::default(),
            pose: None,
            events: vec![],
            nayru: 0.0,
            lens: false,
            lens_timer: 0.0,
            farore: None,
            hover: 0.0,
            target_point: None,
            item_timer: 0.0,
            pending_item: None,
            air_slashed: false,
            pose_clip: None,
            swing_hit: vec![],
        }
    }
}

impl LinkExt {
    /// Switch to Link of `age` with the sandbox loadout.
    pub fn enable(&mut self, age: Age) {
        self.enabled = true;
        self.age = age;
        self.inv = Inventory::full(age);
        self.reset_state();
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.reset_state();
        self.projectiles.clear();
    }

    pub fn reset_state(&mut self) {
        self.mv = Mv::Free;
        self.swing = None;
        self.timer = 0.0;
        self.combo = 0;
        self.queued = false;
        self.charge = Charge::default();
        self.aim = None;
        self.hook_anchor = None;
        self.pose = None;
        self.nayru = 0.0;
        self.lens = false;
        self.hover = 0.0;
        self.item_timer = 0.0;
        self.pending_item = None;
        self.staff.clear();
        self.air_slashed = false;
    }

    /// The weapon in Link's hand.
    pub fn weapon(&self) -> Weapon {
        if self.weapon_stick && self.inv.ammo(Ammo::Stick) > 0 {
            Weapon::Stick
        } else if self.inv.sword.is_none() {
            Weapon::Stick
        } else {
            Weapon::Sword(self.inv.sword.unwrap_or(Sword::Kokiri))
        }
    }

    pub fn shield_up(m: &MarioRuntime) -> bool {
        m.action == ACT_IDLE && m.action_arg == 0x4c as f64
    }

    /// Link is mid-swing with a live hitbox (for the reach / hit helpers).
    pub fn striking(&self, m: &MarioRuntime) -> bool {
        self.enabled && self.swing.is_some() && m.flags & (MARIO_PUNCHING | MARIO_KICKING) != 0
    }

    pub fn take_events(&mut self) -> Vec<LinkEvent> {
        std::mem::take(&mut self.events)
    }

    /// The C-button entry on `bit`.
    fn c_item(&self, bit: u32) -> Option<CItem> {
        let slot = match bit {
            C_LEFT => 0,
            C_DOWN => 1,
            C_RIGHT => 2,
            _ => return None,
        };
        self.inv.c[slot]
    }
}

/// `swordReach` extension: the faithful swings' radius for `sword_reach` (0 otherwise).
pub fn extra_reach(m: &MarioRuntime) -> f64 {
    if m.link.striking(m) {
        m.link.weapon().reach()
    } else {
        0.0
    }
}

/// Entry from `step_link`. Returns true when the frame was fully handled.
pub fn step(m: &mut MarioRuntime) -> bool {
    if !m.link.enabled {
        return false;
    }
    let mut ext = std::mem::take(&mut m.link);
    let handled = ext.run(m);
    m.link = ext;
    handled
}

fn forward(yaw: f64, dist: f64) -> Vec3 {
    [sins(yaw) * dist, 0.0, coss(yaw) * dist]
}

impl LinkExt {
    fn run(&mut self, m: &mut MarioRuntime) -> bool {
        self.pose = None;
        self.absorb_damage(m);
        self.passive(m);
        let c = m.controller.clone();
        let pressed = c.button_pressed;
        let down = c.button_down;
        let c_pressed = c.button_pressed & C_MASK;
        let c_down = c.button_down & C_MASK;

        // Stick history for the circle spin (`controlStickSpinAngles`).
        self.dir_timer += 1.0;
        let oct = stick_octant(c.stick_x, c.stick_y);
        if oct >= 0 && oct != self.dirs[3] {
            self.dirs = [self.dirs[1], self.dirs[2], self.dirs[3], oct];
            self.dir_timer = 0.0;
        } else if self.dir_timer > 20.0 {
            self.dirs = [-1; 4];
        }

        // Non-interrupting abilities first.
        if c_pressed & C_UP != 0 && self.mv == Mv::Free && self.inv.has(Item::Ocarina) && m.action & ACT_FLAG_AIR == 0 && m.action != ACT_LEDGE_GRAB {
            // Touch-friendly shortcut: C-up opens the ocarina (the original needs it on a C button).
            self.begin_ocarina(m);
        }

        match self.mv {
            Mv::Swing => return self.run_swing(m, pressed),
            Mv::Charge => return self.run_charge(m, down),
            Mv::Aim => return self.run_aim(m, c_down, pressed),
            Mv::ItemAnim => return self.run_item_anim(m),
            Mv::HookFly => return self.run_hook_fly(m),
            Mv::Ocarina => return self.run_ocarina(m, pressed, c_pressed),
            Mv::Free => {}
        }

        // Free: only act from plain ground / simple air states so everything else (ledges,
        // knock-back, water, cutscenes, grabbed objects) keeps its SM64 behaviour.
        if m.held_obj.is_some() || m.riding_shell.is_some() || m.cannon.is_some() {
            return false;
        }
        let on_ground = matches!(m.action, ACT_IDLE | ACT_WALKING) && m.action_arg != ROLL_ARG;
        let hop = m.action == ACT_FREEFALL;
        if !on_ground && !hop {
            return false;
        }
        if m.action & ACT_FLAG_AIR == 0 && m.input & INPUT_OFF_FLOOR != 0 && !hop {
            return false;
        }

        if hop {
            return self.free_air(m, pressed, c_pressed);
        }

        // C buttons: use the assigned item.
        for bit in [C_LEFT, C_DOWN, C_RIGHT] {
            if c_pressed & bit != 0 {
                if let Some(ci) = self.c_item(bit) {
                    if self.use_c(m, ci, bit) {
                        return self.run_current(m, pressed, down);
                    }
                }
            }
        }

        if pressed & B_BUTTON != 0 && !Self::shield_up(m) {
            self.start_swing_from_input(m);
            return self.run_swing(m, 0);
        }
        // Z-target + A + forward: jump forward (the OoT front flip), later B = flip slash.
        if c.focus_yaw.is_some() && pressed & A_BUTTON != 0 && c.stick_mag > 20.0 {
            let delta = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
            if delta.abs() <= 0x2000 as f64 {
                m.set_action(ACT_FREEFALL, ARG_JUMP_FORWARD);
                m.forward_vel = 27.0;
                m.vel = [sins(m.face_angle[1]) * 27.0, 30.0, coss(m.face_angle[1]) * 27.0];
                self.air_slashed = false;
                return true;
            }
        }
        false
    }

    /// Whatever mode a successful `use_c` entered runs this very tick.
    fn run_current(&mut self, m: &mut MarioRuntime, pressed: u32, down: u32) -> bool {
        match self.mv {
            Mv::Aim => {
                let (cd, cp) = (m.controller.button_down & C_MASK, pressed);
                self.run_aim(m, cd, cp)
            }
            Mv::ItemAnim => self.run_item_anim(m),
            Mv::Ocarina => self.run_ocarina(m, 0, 0),
            Mv::Swing => self.run_swing(m, 0),
            Mv::Charge => self.run_charge(m, down),
            _ => false,
        }
    }

    // ---------------------------------------------------------------- passive effects

    /// SM64 wedges -> hearts: each wedge Mario loses costs a heart; Mario's meter is refilled.
    fn absorb_damage(&mut self, m: &mut MarioRuntime) {
        let full = m.max_health;
        if m.health < full && m.health >= 0x100 as f64 && m.hurt_counter == 0.0 {
            let lost = ((full - m.health) / 0x100 as f64).floor();
            if lost >= 1.0 {
                if self.nayru <= 0.0 {
                    self.inv.health = (self.inv.health - lost as i32 * HEART).max(0);
                }
                m.state.health = if self.inv.health > 0 { full } else { 0xff as f64 };
            }
        }
    }

    fn passive(&mut self, m: &mut MarioRuntime) {
        if self.nayru > 0.0 {
            self.nayru -= 1.0;
            m.invinc_timer = m.invinc_timer.max(2.0);
        }
        // Lens of Truth: `lensMagicConsumptionTimer = 80` frames per magic unit.
        if self.lens {
            self.lens_timer += 1.0;
            if self.lens_timer >= ticks(80.0) {
                self.lens_timer = 0.0;
                if self.inv.magic > 0 {
                    self.inv.magic -= 1;
                }
            }
            if self.inv.magic <= 0 {
                self.lens = false;
                self.events.push(LinkEvent::LensToggled(false));
            }
        }
        // Boots and tunics.
        let surface = m.world.water_level(m.pos[0], m.pos[2]);
        let underwater = m.pos[1] < surface - 20.0;
        if self.inv.boots == Boots::Iron && m.pos[1] < surface + 40.0 {
            // Metal Mario walks along the bottom (`step_metal_water`): the Iron Boots' behaviour.
            m.metal_cap_timer = m.metal_cap_timer.max(3.0);
            m.state.flags |= MARIO_METAL_CAP;
        }
        if self.inv.tunic == Tunic::Zora && underwater {
            // Breathe under water: SM64 drains the meter while swimming, so keep it topped up.
            m.state.health = m.max_health.max(m.health);
        }
        if m.action & ACT_FLAG_AIR == 0 {
            self.air_slashed = false;
            self.hover = if self.inv.boots == Boots::Hover { ticks(40.0) } else { 0.0 };
        }
    }

    // ---------------------------------------------------------------- air

    fn free_air(&mut self, m: &mut MarioRuntime, pressed: u32, c_pressed: u32) -> bool {
        let _ = c_pressed;
        // Hover Boots: float for a moment after leaving the ground (jump + hold A).
        let hopping = m.action_arg == ARG_JUMP_FORWARD;
        let left_ground = m.action_arg == 0.0;
        if self.inv.boots == Boots::Hover && self.hover > 0.0 && m.controller.button_down & A_BUTTON != 0 && m.vel[1] <= 0.0 && (left_ground || hopping) {
            self.hover -= 1.0;
            m.vel[1] = 0.0;
            m.forward_vel = approach_f32(m.forward_vel, if m.intended_mag > 0.0 { 18.0 } else { 0.0 }, 2.0, 2.0);
            if m.intended_mag > 0.0 {
                m.face_angle[1] = to_s16(m.face_angle[1] + clamp(to_s16(m.intended_yaw - m.face_angle[1]) as f64, -0x500 as f64, 0x500 as f64)) as f64;
            }
            m.vel[0] = sins(m.face_angle[1]) * m.forward_vel;
            m.vel[2] = coss(m.face_angle[1]) * m.forward_vel;
            self.pose = Some(PoseHint { clip: "fall", frame: 0.0, looping: true });
            let hit = m.step_air(AIR_STEP_CHECK_LEDGE_GRAB);
            m.vel[1] = 0.0;
            if hit == AIR_STEP_LANDED as i32 {
                m.set_action(ACT_IDLE, 0.0);
            } else if hit == AIR_STEP_GRABBED_LEDGE as i32 {
                m.set_action(ACT_LEDGE_GRAB, 0.0);
            }
            return true;
        }
        // B in the air: jump slash / flip slash.
        if pressed & B_BUTTON != 0 && !self.air_slashed && self.inv.sword.is_some() {
            self.air_slashed = true;
            let swing = if hopping { Swing::FlipSlash } else { Swing::JumpSlash };
            let (fv, vy) = (m.forward_vel, m.vel[1]);
            m.set_action(ACT_JUMP_KICK, ARG_SWING);
            m.forward_vel = fv;
            m.vel[1] = vy;
            self.begin_swing(m, swing);
            return self.run_swing(m, 0);
        }
        false
    }

    // ---------------------------------------------------------------- swings

    fn start_swing_from_input(&mut self, m: &mut MarioRuntime) {
        let targeted = m.controller.focus_yaw.is_some();
        let stick = m.controller.stick_mag > 20.0;
        let delta = to_s16(m.intended_yaw - m.face_angle[1]) as f64;
        // `func_80837818`: stick direction relative to Link picks the swing.
        let (swing, dir) = if self.weapon() == Weapon::Hammer {
            (if stick && delta.abs() > 0x2000 as f64 { Swing::HammerSide } else { Swing::HammerForward }, Direction::Forward)
        } else if !stick {
            if targeted { (Swing::Forward, Direction::Forward) } else { (Swing::Right, Direction::Right) }
        } else if delta.abs() <= 0x2000 as f64 {
            (Swing::Stab, Direction::Stab)
        } else if delta.abs() >= 0x6000 as f64 {
            (Swing::Right, Direction::Right)
        } else if delta > 0.0 {
            (Swing::Left, Direction::Left)
        } else {
            (Swing::Right, Direction::Right)
        };
        if !targeted && stick {
            m.face_angle[1] = m.intended_yaw;
        }
        self.combo = 0;
        self.combo_dir = dir;
        self.begin_swing(m, swing);
        let (fv, vy) = (m.forward_vel, m.vel[1]);
        if m.action != ACT_JUMP_KICK {
            m.set_action(ACT_PUNCHING, ARG_SWING);
            m.forward_vel = fv.min(8.0);
            m.vel[1] = vy;
        }
    }

    fn begin_swing(&mut self, m: &mut MarioRuntime, swing: Swing) {
        self.mv = Mv::Swing;
        self.swing = Some(swing);
        self.timer = 0.0;
        self.queued = false;
        self.swing_hit.clear();
        self.charge = Charge::default();
        let _ = m;
        self.events.push(LinkEvent::Swing(swing));
    }

    fn run_swing(&mut self, m: &mut MarioRuntime, pressed: u32) -> bool {
        let Some(swing) = self.swing else {
            self.mv = Mv::Free;
            return false;
        };
        let air = swing.is_jump() && m.action & ACT_FLAG_AIR != 0;
        let (w0, w1) = swing.window();
        let t = self.timer;
        let total = swing.total_ticks();
        if pressed & B_BUTTON != 0 && t >= w0 {
            self.queued = true;
        }
        self.pose = Some(PoseHint { clip: swing.info().clip, frame: t, looping: false });

        if t >= w0 && t < w1 {
            m.flags |= if air || swing == Swing::FlipSlash { MARIO_KICKING } else { MARIO_PUNCHING };
            if matches!(swing, Swing::HammerForward | Swing::HammerSide) && t >= w0 && t < w0 + 1.0 {
                let fwd = forward(m.face_angle[1], len(22.0));
                let at = [m.pos[0] + fwd[0], m.pos[1], m.pos[2] + fwd[2]];
                self.events.push(LinkEvent::HammerSlam(at));
                self.hammer_slam(m, at);
            }
        }
        if swing.is_spin() {
            // The sweep covers a full turn between the hit window's start and end.
            let spin_total = (w1 - w0).max(1.0);
            if t >= w0 && t < w1 {
                m.face_angle[1] = to_s16(m.face_angle[1] + 65536.0 / spin_total) as f64;
            }
        }
        self.timer += 1.0;

        if air {
            let vy = m.vel[1];
            let hit = m.step_air(AIR_STEP_CHECK_LEDGE_GRAB);
            if hit == AIR_STEP_LANDED as i32 {
                let fv = m.forward_vel;
                m.land_event(vy, fv);
                self.finish_swing(m, true);
                return true;
            } else if hit == AIR_STEP_GRABBED_LEDGE as i32 {
                self.end_swing_state();
                m.set_action(ACT_LEDGE_GRAB, 0.0);
                return true;
            }
            if self.timer >= total && m.action & ACT_FLAG_AIR != 0 {
                // Slash over, still falling: back to the free-fall.
                self.end_swing_state();
                m.set_action(ACT_FREEFALL, 0.0);
            }
            return true;
        }
        if m.input & INPUT_OFF_FLOOR != 0 && m.action & ACT_FLAG_AIR == 0 {
            // Walked off an edge mid-swing.
            self.end_swing_state();
            m.set_action(ACT_FREEFALL, 0.0);
            return true;
        }
        let fv = approach_f32(m.forward_vel, 0.0, 4.0, 6.0);
        mario_set_forward_vel(m, fv);
        m.step_ground();
        if self.timer >= total {
            self.finish_swing(m, false);
        }
        true
    }

    fn end_swing_state(&mut self) {
        self.mv = Mv::Free;
        self.swing = None;
        self.timer = 0.0;
        self.queued = false;
    }

    fn finish_swing(&mut self, m: &mut MarioRuntime, landed: bool) {
        let swing = self.swing;
        let next_combo = self.queued && !landed && swing.is_some_and(|s| !s.is_spin() && !s.is_jump() && !matches!(s, Swing::HammerForward | Swing::HammerSide));
        let held_b = m.controller.button_down & B_BUTTON != 0;
        if next_combo {
            // `unk_845`: the third consecutive swing becomes the combo finisher.
            self.combo += 1;
            let dir = self.combo_dir;
            let next = if self.combo >= 2 {
                Swing::Combo(dir)
            } else {
                match dir {
                    Direction::Forward => Swing::Forward,
                    Direction::Right => Swing::Left,
                    Direction::Left => Swing::Right,
                    Direction::Stab => Swing::Stab,
                }
            };
            if self.combo >= 3 {
                self.combo = 0;
            }
            self.begin_swing(m, next);
            m.set_action(ACT_PUNCHING, ARG_SWING);
            return;
        }
        if held_b && swing.is_some_and(|s| !s.is_spin() && !s.is_jump() && !matches!(s, Swing::HammerForward | Swing::HammerSide)) {
            // Keep B down after a slash: the spin-attack charge (`Player_Action_80844E68`).
            self.mv = Mv::Charge;
            self.swing = None;
            self.timer = 0.0;
            self.charge = Charge::default();
            m.set_action(ACT_IDLE, ARG_CHARGE);
            return;
        }
        self.end_swing_state();
        self.combo = 0;
        self.air_slashed = false;
        m.set_action(ACT_IDLE, 0.0);
    }

    fn run_charge(&mut self, m: &mut MarioRuntime, down: u32) -> bool {
        let held = down & B_BUTTON != 0;
        self.charge.step();
        self.timer += 1.0;
        self.pose = Some(PoseHint { clip: "spinCharge", frame: self.timer, looping: true });
        if !held {
            if self.charge.value < CHARGE_START {
                // Released too early: no spin (`func_80844D68`).
                self.mv = Mv::Free;
                self.charge = Charge::default();
                m.set_action(ACT_IDLE, 0.0);
                return true;
            }
            let big = self.charge.is_big() || stick_circle(&self.dirs);
            let swing = if big { Swing::BigSpin } else { Swing::Spin };
            self.begin_swing(m, swing);
            m.set_action(ACT_PUNCHING, ARG_SWING);
            return self.run_swing(m, 0);
        }
        if self.charge.is_big() && self.charge.value - CHARGE_STEP < CHARGE_BIG {
            self.events.push(LinkEvent::SpinCharged);
        }
        // Creep along at a third of the stick (charging walk).
        let target = if m.intended_mag > 20.0 { 6.0 } else { 0.0 };
        if m.intended_mag > 20.0 {
            m.face_angle[1] = m.intended_yaw;
        }
        let fv = approach_f32(m.forward_vel, target, 2.0, 4.0);
        mario_set_forward_vel(m, fv);
        let hit = m.step_ground();
        if hit == GROUND_STEP_LEFT_GROUND as i32 {
            self.mv = Mv::Free;
            m.set_action(ACT_FREEFALL, 0.0);
        }
        true
    }

    /// Hammer slam: breakables near the head are smashed (`Player` hammer quake).
    fn hammer_slam(&mut self, m: &mut MarioRuntime, at: Vec3) {
        let radius = len(30.0);
        let mut hit = vec![];
        for id in m.objects.ids() {
            let o = &m.objects[id];
            if o.active && o.hitbox.interact_type & INTERACT_BREAKABLE != 0 && dist_3d(&o.pos, &at) < radius + o.hitbox.radius {
                hit.push(id);
            }
        }
        for id in hit {
            attack_object(&mut m.objects[id], ATTACK_GROUND_POUND_OR_TWIRL);
        }
    }

    // ---------------------------------------------------------------- items

    /// Chest height Link shoots from.
    fn muzzle(&self, m: &MarioRuntime, yaw: f64) -> Vec3 {
        let f = forward(yaw, 40.0);
        [m.pos[0] + f[0], m.pos[1] + 100.0, m.pos[2] + f[2]]
    }

    fn deny(&mut self, e: LinkEvent) -> bool {
        self.events.push(e);
        false
    }

    /// Press of the C button on `bit` carrying `ci`. True when it started an action.
    fn use_c(&mut self, m: &mut MarioRuntime, ci: CItem, bit: u32) -> bool {
        match ci {
            CItem::Bottle(n) => self.use_bottle(m, n as usize),
            CItem::Item(item) => self.use_item(m, item, bit),
        }
    }

    pub fn use_item(&mut self, m: &mut MarioRuntime, item: Item, bit: u32) -> bool {
        if !item.usable_by(self.age) {
            return self.deny(LinkEvent::WrongAge(item));
        }
        let yaw = m.face_angle[1];
        match item {
            Item::Bow | Item::FireArrow | Item::IceArrow | Item::LightArrow => {
                if self.inv.ammo(Ammo::Arrow) == 0 {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                if item.magic_cost() > self.inv.magic {
                    return self.deny(LinkEvent::NoMagic(item));
                }
                self.begin_aim(m, item, bit);
                true
            }
            Item::Slingshot => {
                if self.inv.ammo(Ammo::Seed) == 0 {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                self.begin_aim(m, item, bit);
                true
            }
            Item::Hookshot => {
                self.begin_aim(m, item, bit);
                true
            }
            Item::Boomerang => {
                if self.projectiles.count(Kind::Boomerang) > 0 {
                    return false;
                }
                let at = self.muzzle(m, yaw);
                self.projectiles.throw_boomerang(at, yaw);
                self.events.push(LinkEvent::Boomerang);
                self.start_item_anim(m, item, "boomThrow", ticks(10.0));
                true
            }
            Item::Bomb => {
                if self.projectiles.count(Kind::Bomb) >= 3 {
                    return false;
                }
                if !self.inv.use_ammo(Ammo::Bomb) {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                let f = forward(yaw, len(14.0));
                let at = [m.pos[0] + f[0], m.pos[1] + 60.0, m.pos[2] + f[2]];
                let toss = forward(yaw, 10.0);
                self.projectiles.drop_bomb(at, [toss[0], 14.0, toss[2]]);
                self.start_item_anim(m, item, "lightBomb", ticks(8.0));
                true
            }
            Item::Bombchu => {
                if self.projectiles.count(Kind::Bombchu) >= 1 {
                    return false;
                }
                if !self.inv.use_ammo(Ammo::Bombchu) {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                let f = forward(yaw, len(12.0));
                self.projectiles.launch_bombchu([m.pos[0] + f[0], m.floor_height.max(m.pos[1]), m.pos[2] + f[2]], yaw);
                self.start_item_anim(m, item, "throw", ticks(8.0));
                true
            }
            Item::DekuNut => {
                if !self.inv.use_ammo(Ammo::Nut) {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                let at = self.muzzle(m, yaw);
                self.projectiles.shoot(Kind::Nut, at, yaw, -0.18);
                self.events.push(LinkEvent::Shot(Kind::Nut));
                self.start_item_anim(m, item, "throw", ticks(8.0));
                true
            }
            Item::DekuStick => {
                // Take the stick out (or put it away): swings then use `Dmg::Stick`.
                if self.inv.ammo(Ammo::Stick) == 0 {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                self.weapon_stick = !self.weapon_stick;
                self.events.push(LinkEvent::ItemUsed(item));
                false
            }
            Item::MegatonHammer => {
                self.weapon_stick = false;
                self.combo = 0;
                self.combo_dir = Direction::Forward;
                self.begin_swing(m, Swing::HammerForward);
                m.set_action(ACT_PUNCHING, ARG_SWING);
                self.events.push(LinkEvent::ItemUsed(item));
                true
            }
            Item::DinsFire => self.cast(m, item, "dinsCast", ticks(40.0), |ext, m| {
                let at = [m.pos[0], m.pos[1] + 20.0, m.pos[2]];
                ext.projectiles.cast_dins(at);
            }),
            Item::FaroresWind => {
                self.cast(m, item, "farore", ticks(40.0), |ext, m| match ext.farore.take() {
                    None => {
                        ext.farore = Some(m.pos);
                        ext.events.push(LinkEvent::FaroreSet(m.pos));
                    }
                    Some(p) => {
                        ext.events.push(LinkEvent::FaroreWarp(p));
                        m.pos = p;
                        m.vel = [0.0; 3];
                        m.state.forward_vel = 0.0;
                    }
                })
            }
            Item::NayrusLove => self.cast(m, item, "nayru", ticks(40.0), |ext, _| {
                ext.nayru = ticks(1200.0);
                ext.events.push(LinkEvent::NayruOn);
            }),
            Item::LensOfTruth => {
                if !self.lens && self.inv.magic <= 0 {
                    return self.deny(LinkEvent::NoMagic(item));
                }
                self.lens = !self.lens;
                self.lens_timer = 0.0;
                self.events.push(LinkEvent::LensToggled(self.lens));
                false
            }
            Item::MagicBean => {
                if self.inv.ammo(Ammo::Bean) == 0 {
                    return self.deny(LinkEvent::NoAmmo(item));
                }
                self.inv.ammo[Ammo::Bean as usize] -= 1;
                let at = m.pos;
                self.events.push(LinkEvent::BeanPlanted(at));
                self.start_item_anim(m, item, "lightBomb", ticks(10.0));
                true
            }
            Item::Ocarina => {
                self.begin_ocarina(m);
                true
            }
        }
    }

    fn cast(&mut self, m: &mut MarioRuntime, item: Item, clip: &'static str, duration: f64, effect: fn(&mut LinkExt, &mut MarioRuntime)) -> bool {
        if item == Item::NayrusLove && self.nayru > 0.0 {
            return false;
        }
        if !self.inv.spend_magic(item.magic_cost()) {
            return self.deny(LinkEvent::NoMagic(item));
        }
        effect(self, m);
        self.events.push(LinkEvent::ItemUsed(item));
        self.start_item_anim(m, item, clip, duration);
        true
    }

    fn start_item_anim(&mut self, m: &mut MarioRuntime, item: Item, clip: &'static str, duration: f64) {
        self.mv = Mv::ItemAnim;
        self.timer = 0.0;
        self.item_timer = duration;
        self.pending_item = Some(item);
        let _ = clip;
        m.set_action(ACT_IDLE, ARG_ITEM);
        mario_set_forward_vel(m, 0.0);
        self.pose_clip = Some(clip);
    }

    fn run_item_anim(&mut self, m: &mut MarioRuntime) -> bool {
        let clip = self.pose_clip.unwrap_or("idle");
        self.pose = Some(PoseHint { clip, frame: self.timer, looping: false });
        self.timer += 1.0;
        let fv = approach_f32(m.forward_vel, 0.0, 6.0, 6.0);
        mario_set_forward_vel(m, fv);
        m.step_ground();
        if self.timer >= self.item_timer {
            self.mv = Mv::Free;
            self.pending_item = None;
            m.set_action(ACT_IDLE, 0.0);
        }
        true
    }

    fn use_bottle(&mut self, m: &mut MarioRuntime, slot: usize) -> bool {
        let Some(b) = self.inv.bottles.get(slot).copied().flatten() else { return false };
        match bottle_use(b) {
            BottleUse::Nothing => false,
            BottleUse::Drink { health, magic, becomes } => {
                self.inv.heal(health);
                if magic {
                    self.inv.magic = self.inv.magic_cap;
                }
                self.inv.set_bottle(slot, becomes);
                self.events.push(LinkEvent::BottleUsed(b));
                self.start_item_anim(m, Item::Ocarina, "drink", ticks(60.0));
                self.pending_item = None;
                true
            }
            BottleUse::ReleaseFairy => {
                self.inv.heal(0x80);
                self.inv.set_bottle(slot, Bottle::Empty);
                self.events.push(LinkEvent::FairyReleased);
                self.events.push(LinkEvent::BottleUsed(b));
                self.start_item_anim(m, Item::Ocarina, "bottleSwing", ticks(20.0));
                true
            }
            BottleUse::Release(what) => {
                self.inv.set_bottle(slot, Bottle::Empty);
                self.events.push(LinkEvent::BottleUsed(what));
                self.start_item_anim(m, Item::Ocarina, "bottleSwing", ticks(20.0));
                true
            }
            BottleUse::Poe => {
                // `Rand_S16Offset(-1, 3)`: -1, 1, 2 or 3 sixteenths; 0 rerolls to 3.
                let r = (random() * 4.0).floor() as i32 - 1;
                let r = if r == 0 { 3 } else { r };
                if r < 0 {
                    self.inv.health = (self.inv.health - 0x10).max(1);
                } else {
                    self.inv.heal(r * 0x10);
                }
                self.inv.set_bottle(slot, Bottle::Empty);
                self.events.push(LinkEvent::BottleUsed(b));
                self.start_item_anim(m, Item::Ocarina, "drink", ticks(40.0));
                true
            }
        }
    }

    // ---------------------------------------------------------------- aiming

    fn begin_aim(&mut self, m: &mut MarioRuntime, item: Item, bit: u32) {
        self.mv = Mv::Aim;
        self.timer = 0.0;
        self.aim = Some(Aim { item, yaw: m.face_angle[1], pitch: 0.0, bit });
        mario_set_forward_vel(m, 0.0);
        m.vel = [0.0; 3];
        m.set_action(ACT_IDLE, ARG_AIM);
    }

    fn run_aim(&mut self, m: &mut MarioRuntime, c_down: u32, pressed: u32) -> bool {
        let Some(mut aim) = self.aim else {
            self.mv = Mv::Free;
            return false;
        };
        let c = m.controller.clone();
        self.timer += 1.0;
        // Aim: the stick turns and tilts (`slingshot.rs` rates); a Z-target locks the aim on.
        if let Some(t) = self.target_point.filter(|_| c.focus_yaw.is_some()) {
            let eye = self.muzzle(m, aim.yaw);
            let d = [t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]];
            aim.yaw = atan2s(d[2], d[0]) as f64;
            aim.pitch = d[1].atan2(d[0].hypot(d[2])).clamp(-1.0, 1.0);
        } else {
            aim.yaw = to_s16(aim.yaw - c.stick_x * 8.0) as f64;
            aim.pitch = (aim.pitch + c.stick_y * 0.00055).clamp(-1.3, 1.35);
        }
        m.face_angle[1] = aim.yaw;
        self.aim = Some(aim);
        let clip = if aim.item == Item::Hookshot { "hookReady" } else { "bowReady" };
        self.pose = Some(PoseHint { clip, frame: self.timer.min(ticks(6.0)), looping: false });
        mario_set_forward_vel(m, 0.0);
        m.step_ground();

        // Release the C button (or press B) to fire; A cancels the draw.
        let released = c_down & aim.bit == 0;
        let _ = pressed;
        if c.button_pressed & A_BUTTON != 0 {
            self.mv = Mv::Free;
            self.aim = None;
            m.set_action(ACT_IDLE, 0.0);
            return true;
        }
        if released || c.button_pressed & B_BUTTON != 0 {
            self.fire(m, aim);
        }
        true
    }

    fn fire(&mut self, m: &mut MarioRuntime, aim: Aim) {
        let at = self.muzzle(m, aim.yaw);
        let mut clip = "bowShoot";
        match aim.item {
            Item::Bow | Item::FireArrow | Item::IceArrow | Item::LightArrow => {
                if self.inv.ammo(Ammo::Arrow) > 0 && self.inv.spend_magic(aim.item.magic_cost()) {
                    self.inv.use_ammo(Ammo::Arrow);
                    let kind = match aim.item {
                        Item::FireArrow => Kind::FireArrow,
                        Item::IceArrow => Kind::IceArrow,
                        Item::LightArrow => Kind::LightArrow,
                        _ => Kind::Arrow,
                    };
                    self.projectiles.shoot(kind, at, aim.yaw, aim.pitch);
                    self.events.push(LinkEvent::Shot(kind));
                } else {
                    self.events.push(LinkEvent::NoAmmo(aim.item));
                }
            }
            Item::Slingshot => {
                if self.inv.use_ammo(Ammo::Seed) {
                    self.projectiles.shoot(Kind::Seed, at, aim.yaw, aim.pitch);
                    self.events.push(LinkEvent::Shot(Kind::Seed));
                } else {
                    self.events.push(LinkEvent::NoAmmo(aim.item));
                }
            }
            Item::Hookshot => {
                let long = self.inv.longshot;
                self.hook_long = long;
                self.projectiles.shoot_hook(long, at, aim.yaw, aim.pitch);
                self.events.push(LinkEvent::Shot(if long { Kind::Longshot } else { Kind::Hookshot }));
                clip = "hookReady";
            }
            _ => {}
        }
        self.aim = None;
        self.mv = Mv::ItemAnim;
        self.timer = 0.0;
        self.item_timer = if aim.item == Item::Hookshot { ticks(16.0) } else { ticks(8.0) };
        self.pose_clip = Some(clip);
        m.set_action(ACT_IDLE, ARG_ITEM);
    }

    // ---------------------------------------------------------------- hookshot pull

    fn run_hook_fly(&mut self, m: &mut MarioRuntime) -> bool {
        let Some(anchor) = self.hook_anchor else {
            self.mv = Mv::Free;
            m.set_action(ACT_FREEFALL, 0.0);
            return true;
        };
        self.timer += 1.0;
        self.pose = Some(PoseHint { clip: "hookFly", frame: self.timer, looping: true });
        let chest = [m.pos[0], m.pos[1] + 100.0, m.pos[2]];
        let d = [anchor[0] - chest[0], anchor[1] - chest[1], anchor[2] - chest[2]];
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        // `ArmsHook_PullPlayer`: ends within 30 units of the anchor.
        if dist < len(30.0) || self.timer > 120.0 {
            self.hook_anchor = None;
            self.mv = Mv::Free;
            for p in self.projectiles.list.iter_mut().filter(|p| matches!(p.kind, Kind::Hookshot | Kind::Longshot)) {
                p.phase = Phase::Done;
            }
            m.vel = [0.0; 3];
            m.state.forward_vel = 0.0;
            m.set_action(ACT_FREEFALL, 0.0);
            return true;
        }
        let v = speed(20.0).min(dist);
        let step = [d[0] / dist * v, d[1] / dist * v, d[2] / dist * v];
        let mut next = [m.pos[0] + step[0], m.pos[1] + step[1], m.pos[2] + step[2]];
        // Stop when a wall blocks the way (the anchor may be on one).
        let before = next;
        m.world.resolve_and_return_wall_collisions(&mut next, 30.0, 40.0);
        if (before[0] - next[0]).abs() + (before[2] - next[2]).abs() > 1.0 && dist < len(60.0) {
            next = before;
        }
        m.pos = next;
        m.vel = [0.0; 3];
        true
    }

    // ---------------------------------------------------------------- ocarina

    fn begin_ocarina(&mut self, m: &mut MarioRuntime) {
        self.mv = Mv::Ocarina;
        self.timer = 0.0;
        self.staff.clear();
        mario_set_forward_vel(m, 0.0);
        m.vel = [0.0; 3];
        m.set_action(ACT_IDLE, ARG_OCARINA);
    }

    fn run_ocarina(&mut self, m: &mut MarioRuntime, pressed: u32, c_pressed: u32) -> bool {
        self.timer += 1.0;
        self.pose = Some(PoseHint { clip: "ocarina", frame: self.timer, looping: true });
        mario_set_forward_vel(m, 0.0);
        m.step_ground();
        if pressed & B_BUTTON != 0 {
            self.mv = Mv::Free;
            self.staff.clear();
            m.set_action(ACT_IDLE, 0.0);
            return true;
        }
        let mut notes = vec![];
        if pressed & A_BUTTON != 0 {
            notes.push(Note::A);
        }
        if c_pressed & C_DOWN != 0 {
            notes.push(Note::CDown);
        }
        if c_pressed & C_RIGHT != 0 {
            notes.push(Note::CRight);
        }
        if c_pressed & C_LEFT != 0 {
            notes.push(Note::CLeft);
        }
        if c_pressed & C_UP != 0 && self.timer > 2.0 {
            notes.push(Note::CUp);
        }
        for n in notes {
            self.events.push(LinkEvent::Note(n));
            if let Some(song) = self.staff.play(n) {
                if self.learned.knows(song) {
                    self.events.push(LinkEvent::Song(song));
                } else {
                    self.events.push(LinkEvent::UnknownSong);
                }
            }
        }
        true
    }

    // ---------------------------------------------------------------- projectiles / targets

    /// The things projectiles can hit: sim objects plus the app's own (`extra`, ids from
    /// [`EXTRA_TARGET_BASE`]).
    pub fn collect_targets(m: &MarioRuntime, extra: &[Target]) -> Vec<Target> {
        let mut out: Vec<Target> = vec![];
        for id in m.objects.ids() {
            let o = &m.objects[id];
            if !o.active || o.held_state != 0 {
                continue;
            }
            let t = o.hitbox.interact_type;
            let kind = if t & (INTERACT_DAMAGE | INTERACT_KOOPA | INTERACT_MR_BLIZZARD) != 0 || o.behavior == ObjectBehaviorKind::Bobomb {
                TargetKind::Enemy
            } else if t & INTERACT_BREAKABLE != 0 || o.behavior == ObjectBehaviorKind::BlastWall {
                TargetKind::Breakable
            } else {
                continue;
            };
            out.push(Target { id: id.0, pos: [o.pos[0], o.pos[1] - o.hitbox.down_offset, o.pos[2]], radius: o.hitbox.radius, height: o.hitbox.height, kind });
        }
        out.extend_from_slice(extra);
        out
    }

    /// Advance Link's projectiles one tick and apply what they hit. Call after `MarioRuntime::step`.
    pub fn step_projectiles(m: &mut MarioRuntime, extra: &[Target]) {
        let mut ext = std::mem::take(&mut m.link);
        ext.projectiles.follow([m.pos[0], m.pos[1] + 20.0, m.pos[2]]);
        let targets = Self::collect_targets(m, extra);
        let chest = [m.pos[0], m.pos[1] + 100.0, m.pos[2]];
        let impacts = ext.projectiles.step(&m.world, &chest, &targets);
        for imp in impacts {
            ext.apply_impact(m, &targets, imp);
        }
        // The hook anchor pull starts as soon as the tip latched.
        if ext.mv != Mv::HookFly {
            if let Some(p) = ext.projectiles.list.iter().find(|p| matches!(p.kind, Kind::Hookshot | Kind::Longshot) && p.phase == Phase::Pulling) {
                ext.hook_anchor = Some(p.pos);
                ext.mv = Mv::HookFly;
                ext.timer = 0.0;
                ext.aim = None;
                m.set_action(ACT_IDLE, ARG_HOOK);
                ext.events.push(LinkEvent::HookAttached);
            }
        }
        m.link = ext;
    }

    fn apply_impact(&mut self, m: &mut MarioRuntime, targets: &[Target], imp: Impact) {
        match imp {
            Impact::Hit { kind, dmg, target, pos, .. } => {
                if target >= EXTRA_TARGET_BASE {
                    self.events.push(LinkEvent::HitExtra { id: target, dmg, pos });
                } else {
                    hit_object(m, target, dmg);
                }
                if kind == Kind::DinsFire {
                    self.events.push(LinkEvent::Ignite { pos, radius: len(20.0) });
                }
            }
            Impact::Scorch { target, .. } => {
                let pos = targets.iter().find(|t| t.id == target).map(|t| t.pos).unwrap_or(m.pos);
                if target >= EXTRA_TARGET_BASE {
                    self.events.push(LinkEvent::HitExtra { id: target, dmg: Dmg::FireMagic, pos });
                } else {
                    hit_object(m, target, Dmg::FireMagic);
                }
            }
            Impact::Explosion { pos, radius } => {
                self.events.push(LinkEvent::Explosion { pos, radius });
                for t in in_blast(targets, &pos, radius) {
                    if t.id >= EXTRA_TARGET_BASE {
                        self.events.push(LinkEvent::HitExtra { id: t.id, dmg: Dmg::Explosive, pos: t.pos });
                    } else {
                        hit_object(m, t.id, Dmg::Explosive);
                    }
                }
                // Bombs hurt Link too (2 hearts, `EnBom`'s explosion collider) unless Nayru's Love is up.
                let dx = m.pos[0] - pos[0];
                let dz = m.pos[2] - pos[2];
                if dx * dx + dz * dz < (radius + 37.0) * (radius + 37.0) && m.pos[1] > pos[1] - 160.0 && m.pos[1] < pos[1] + radius {
                    m.take_damage(2.0, Some(pos));
                }
            }
            Impact::Flash { pos, radius } => self.events.push(LinkEvent::Flash { pos, radius }),
            Impact::Ignite { pos, radius } => self.events.push(LinkEvent::Ignite { pos, radius }),
            Impact::Freeze { pos, radius } => self.events.push(LinkEvent::Freeze { pos, radius }),
            Impact::Delivered { target } => self.events.push(LinkEvent::Fetched { id: target }),
            Impact::HookAttach { .. } | Impact::Terrain { .. } | Impact::Grab { .. } | Impact::Caught => {}
        }
    }
}

/// Apply `dmg` to sim object `id`: breakables smash, enemies are knocked out through the same
/// `attack_object` status the SM64 behaviors already react to.
pub fn hit_object(m: &mut MarioRuntime, id: u32, dmg: Dmg) {
    let oid = crate::object::ObjId(id);
    if id as usize >= m.objects.objects.len() {
        return;
    }
    if m.objects[oid].behavior == ObjectBehaviorKind::BlastWall {
        if dmg.is_explosive() {
            m.objects.break_object(oid);
            m.objects[oid].deactivate(-1.0);
        }
        return;
    }
    let strong = dmg.is_explosive() || matches!(dmg, Dmg::HammerSwing | Dmg::HammerJump | Dmg::GiantJump | Dmg::MasterJump | Dmg::FireMagic | Dmg::GiantSpin | Dmg::MasterSpin);
    let attack = if strong { ATTACK_GROUND_POUND_OR_TWIRL } else { ATTACK_KICK_OR_TRIP };
    attack_object(&mut m.objects[oid], attack);
}

const ROLL_ARG: f64 = 1.0;
