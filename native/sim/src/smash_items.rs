//! Super Smash Bros. 64 items for the Smash fighters (`smash::items`, a child module of [`crate::smash`]). Engine-free and
//! deterministic, 30 Hz, SM64 units, like the specials.
//!
//! - **Spawning**: while [`Items::enabled`] (the app turns it on for the courses where a Smash fighter plays), an item drops
//!   from above a random floor point near the fighter every [`SPAWN_TICKS`] (at most [`MAX_ON_FIELD`] on the field),
//!   picked from the classic set; [`Items::spawn`] drops a given one (tests, `JUMPMAN_SMASH_ITEM`).
//! - **Pick up**: the attack button (B) while standing on / next to an item. Food (Maxim Tomato, Heart Container) is eaten
//!   on pick-up, the Star is taken on touch. Crates and barrels are heavy (held over the head).
//! - **Use**: B with the stick near neutral uses the held item: Beam Sword / Home-Run Bat / Fan / Star Rod swing (jab,
//!   tilt while walking, smash with Z held: the Home-Run Bat's smash is the home run; the Star Rod's tilt / smash also
//!   shoot a star), Ray Gun shoots (16 shots), Fire Flower breathes fire while B is held (60 frames of fuel), the Hammer
//!   is worn on pick-up and swings by itself for 12 s.
//! - **Throw**: B with the stick pushed hard throws toward the stick (heavy items forward). Thrown items hit what they
//!   meet (ITAttributes damage); Bob-omb, Motion-Sensor Bomb and the containers react: the Bob-omb explodes, the
//!   Motion-Sensor Bomb sticks where it lands and arms, shells slide and bounce, the Bumper stays and bumps, the Poké Ball
//!   bursts open, Capsule / Crate / Barrel break and release another item.
//! - **Hits** go through the specials' path: a thrown item, an armed bomb or a shell is a [`ProjKind::Item`] projectile
//!   kept on the item; swings, the hammer and the food-free shots are [`Hitbox`]es / projectiles of
//!   [`SpecialMove::Item`]. [`resolve`](super::resolve) applies them to every [`SmashTargets`](super::SmashTargets), so a
//!   mode's knockback / percent applies to items without anything here.
//!
//! Numbers: `src/it/itvars.h` (gravity, terminal speeds, ammo, timers), the ITAttributes / WPAttributes / ITAttackEvent
//! tables of `src/relocData/251_ITCommonData.c` (damage, sizes), converted with [`K`] / [`KV`] / [`KA`]. The swing damage
//! lives in the fighters' motion event scripts (not read yet): the swings use the item's ITAttributes damage scaled per
//! swing (approximate).
use super::*;

/// Ticks between two spawns (SSB64's medium item rate is roughly one every 10-20 s).
pub const SPAWN_TICKS: u32 = 330;
/// Items on the field at once.
pub const MAX_ON_FIELD: usize = 4;
/// Ticks an untouched item stays (it blinks for the last 60).
pub const ITEM_LIFE: u32 = 900;
/// Reach of a pick-up (horizontal, from Mario's feet) and of the Star's touch.
pub const PICKUP_RADIUS: f64 = 95.0;
/// Stick deflection (raw, of 64) that makes B a throw.
pub const STICK_THROW: f64 = 56.0;
/// Hit source ids of items (never a special's `instance << 4`).
const SOURCE: u32 = 0x8000_0000;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ItemKind {
    BeamSword,
    HomeRunBat,
    Fan,
    StarRod,
    RayGun,
    FireFlower,
    Hammer,
    MaximTomato,
    Heart,
    Star,
    BobOmb,
    GreenShell,
    RedShell,
    MotionBomb,
    Bumper,
    PokeBall,
    Capsule,
    Crate,
    Barrel,
}

/// What an item does when held.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Swing,
    Shoot,
    Hammer,
    Food,
    Star,
    Throw,
    Container,
}

impl ItemKind {
    pub const ALL: [ItemKind; 19] = [
        ItemKind::BeamSword,
        ItemKind::HomeRunBat,
        ItemKind::Fan,
        ItemKind::StarRod,
        ItemKind::RayGun,
        ItemKind::FireFlower,
        ItemKind::Hammer,
        ItemKind::MaximTomato,
        ItemKind::Heart,
        ItemKind::Star,
        ItemKind::BobOmb,
        ItemKind::GreenShell,
        ItemKind::RedShell,
        ItemKind::MotionBomb,
        ItemKind::Bumper,
        ItemKind::PokeBall,
        ItemKind::Capsule,
        ItemKind::Crate,
        ItemKind::Barrel,
    ];

    /// The model in the import's `objects.json`.
    pub fn model(self) -> &'static str {
        use ItemKind::*;
        match self {
            BeamSword => "beam_sword",
            HomeRunBat => "bat",
            Fan => "fan",
            StarRod => "star_rod",
            RayGun => "ray_gun",
            FireFlower => "fire_flower",
            Hammer => "hammer",
            MaximTomato => "maxim_tomato",
            Heart => "heart",
            Star => "star",
            BobOmb => "bob_omb",
            GreenShell => "green_shell",
            RedShell => "red_shell",
            MotionBomb => "motion_bomb",
            Bumper => "bumper",
            PokeBall => "poke_ball",
            Capsule => "capsule",
            Crate => "crate",
            Barrel => "barrel",
        }
    }

    /// Parse a model name or a loose label (`JUMPMAN_SMASH_ITEM=sword`).
    pub fn parse(s: &str) -> Option<ItemKind> {
        let s = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        ItemKind::ALL.into_iter().find(|k| k.model() == s).or(match s.as_str() {
            "sword" => Some(ItemKind::BeamSword),
            "home_run_bat" | "homerun_bat" => Some(ItemKind::HomeRunBat),
            "harisen" => Some(ItemKind::Fan),
            "tomato" => Some(ItemKind::MaximTomato),
            "heart_container" => Some(ItemKind::Heart),
            "starman" => Some(ItemKind::Star),
            "bombhei" | "bobomb" => Some(ItemKind::BobOmb),
            "msbomb" | "motion_sensor_bomb" => Some(ItemKind::MotionBomb),
            "pokeball" => Some(ItemKind::PokeBall),
            "box" => Some(ItemKind::Crate),
            "taru" => Some(ItemKind::Barrel),
            _ => None,
        })
    }

    pub fn class(self) -> Class {
        use ItemKind::*;
        match self {
            BeamSword | HomeRunBat | Fan | StarRod => Class::Swing,
            RayGun | FireFlower => Class::Shoot,
            Hammer => Class::Hammer,
            MaximTomato | Heart => Class::Food,
            Star => Class::Star,
            BobOmb | GreenShell | RedShell | MotionBomb | Bumper | PokeBall => Class::Throw,
            Capsule | Crate | Barrel => Class::Container,
        }
    }

    /// Held over the head with both hands (`ITAttributes.weight` = heavy).
    pub fn heavy(self) -> bool {
        matches!(self, ItemKind::Crate | ItemKind::Barrel)
    }

    /// ITAttributes damage (SSB64 percent): what the item does when thrown into something.
    pub fn damage(self) -> f32 {
        use ItemKind::*;
        match self {
            BeamSword => 10.0,
            HomeRunBat => 12.0,
            Fan => 1.0,
            StarRod => 6.0,
            RayGun => 2.0,
            FireFlower => 2.0,
            Hammer => 10.0,
            MaximTomato | Heart | Star | BobOmb => 1.0,
            GreenShell => 18.0,
            RedShell => 10.0,
            MotionBomb => 5.0,
            Bumper => 4.0,
            PokeBall => 4.0,
            Capsule => 5.0,
            Crate => 15.0,
            Barrel => 12.0,
        }
    }

    /// `IT*_GRAVITY` / `IT*_TVEL` (itvars.h), SSB64 units per frame.
    fn fall(self) -> (f64, f64) {
        use ItemKind::*;
        match self {
            BeamSword => (1.2, 100.0),
            HomeRunBat => (1.5, 120.0),
            Fan => (1.0, 80.0),
            StarRod => (1.2, 100.0),
            RayGun => (1.5, 130.0),
            FireFlower => (1.2, 100.0),
            Hammer => (1.5, 120.0),
            MaximTomato => (1.2, 100.0),
            Heart => (0.25, 30.0),
            Star => (1.2, 100.0),
            BobOmb => (1.2, 100.0),
            GreenShell | RedShell => (1.2, 100.0),
            MotionBomb => (1.5, 80.0),
            Bumper | PokeBall => (1.2, 100.0),
            Capsule => (1.2, 100.0),
            Crate => (4.0, 120.0),
            Barrel => (4.0, 90.0),
        }
    }

    /// Shots / fuel of a full item (`ITLGUN_AMMO_MAX`, `ITFFLOWER_AMMO_MAX` frames, `ITSTARROD_AMMO_MAX`).
    pub fn ammo(self) -> u32 {
        match self {
            ItemKind::RayGun => 16,
            ItemKind::FireFlower => 60,
            ItemKind::StarRod => 20,
            _ => 0,
        }
    }

    /// Radius of the item's body (pick-up / thrown hit), SM64 units.
    fn radius(self) -> f64 {
        if self.heavy() {
            0.5 * 400.0 * K
        } else {
            0.5 * 150.0 * K
        }
    }
}

/// Where an item is in its life.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemState {
    /// Falling or bouncing, not held.
    Air,
    /// Resting on a floor.
    Ground,
    /// Flying from a throw: hits what it meets.
    Thrown,
    /// Bob-omb walking, shell sliding, Motion-Sensor Bomb armed, Bumper planted, Star bouncing: its own behaviour.
    Active,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Item {
    pub id: u32,
    pub kind: ItemKind,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Facing / spin (radians) for the view.
    pub yaw: f64,
    pub spin: f64,
    pub state: ItemState,
    /// Ticks since it appeared / since its state changed.
    pub age: u32,
    pub t: u32,
    pub ammo: u32,
    /// The hit volume (a [`ProjKind::Item`] projectile) while thrown / active.
    pub proj: Option<u32>,
    pub dead: bool,
}

/// What the held item is doing this tick (the item use owns Mario's frame, like a special).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UseAct {
    /// Picking it up (`LightGet` / `HeavyGet`).
    Get,
    /// Throwing it toward `yaw` (`LightThrow*` / `HeavyThrow*`).
    Throw,
    /// A swing: 1 jab, 3 tilt, 4 smash.
    Swing(u8),
    /// Ray Gun shot.
    Shoot,
    /// Fire Flower (while B is held).
    Flame,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ItemUse {
    pub act: UseAct,
    pub kind: ItemKind,
    pub t: u32,
    pub len: u32,
    pub air: bool,
    pub yaw: f64,
    /// Hit identity of this use (a target is hit once per swing).
    source: u32,
    /// Mario's action as the use left it (anything else next tick: interrupted).
    sm64_action: u32,
}

/// What the view and the sound layer hear about (drained by the app).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemEvent {
    Spawned { kind: ItemKind, pos: Vec3 },
    PickedUp { kind: ItemKind },
    Eaten { kind: ItemKind },
    StarGot,
    HammerOn,
    Thrown { kind: ItemKind, heavy: bool },
    Swing { kind: ItemKind, level: u8 },
    Shot { kind: ItemKind },
    Empty { kind: ItemKind },
    /// A swing / shot / thrown item hit something.
    Hit { kind: ItemKind, pos: Vec3 },
    Landed { kind: ItemKind, pos: Vec3 },
    Exploded { kind: ItemKind, pos: Vec3, radius: f64 },
    Broke { kind: ItemKind, pos: Vec3 },
    Armed { pos: Vec3 },
    Vanished { kind: ItemKind, pos: Vec3 },
}

/// A mode's item spawning: ticks between drops, most on the field, and the weighted set.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemRates {
    pub every: u32,
    pub max: usize,
    pub pool: Vec<(ItemKind, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Items {
    /// Spawn items near the fighter (the app turns it on where items make sense).
    pub enabled: bool,
    pub list: Vec<Item>,
    /// The item in the fighter's hand.
    pub held: Option<Item>,
    pub using: Option<ItemUse>,
    /// Star invincibility left (ticks).
    pub star: u32,
    /// Hammer time left (ticks); the hammer swings by itself.
    pub hammer: u32,
    /// Mario walked this tick (the hammer's walk / wait motion).
    pub moving: bool,
    pub events: Vec<ItemEvent>,
    /// A mode's own spawn rule (set, rate, cap); `None`: the classic set every [`SPAWN_TICKS`], [`MAX_ON_FIELD`] at most.
    pub rates: Option<ItemRates>,
    /// Food eaten so far and the last one (a mode with its own health heals from these: Star Swarm).
    pub meals: u32,
    pub last_meal: Option<ItemKind>,
    spawn_timer: u32,
    rng: u32,
    next_id: u32,
}

impl Items {
    fn rand(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (self.rng >> 16) & 0x7FFF
    }

    fn id(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    /// Drop `kind` at `pos` (it falls to the floor below).
    pub fn spawn(&mut self, kind: ItemKind, pos: Vec3) -> u32 {
        let id = self.id();
        let ammo = kind.ammo();
        let yaw = (self.rand() % 628) as f64 / 100.0;
        self.list.push(Item { id, kind, pos, vel: [0.0; 3], yaw, spin: 0.0, state: ItemState::Air, age: 0, t: 0, ammo, proj: None, dead: false });
        self.events.push(ItemEvent::Spawned { kind, pos });
        id
    }

    /// The decomp motion names the item use plays, the seconds into it, and whether it loops (`SmashExt::motion`).
    pub fn motion(&self, fighter_air: bool) -> Option<(Vec<String>, f32, bool)> {
        if let Some(u) = self.using {
            let secs = u.t as f32 / 30.0;
            let heavy = u.kind.heavy();
            let names: Vec<&str> = match u.act {
                UseAct::Get => vec![if heavy { "HeavyGet" } else { "LightGet" }],
                UseAct::Throw if heavy => vec!["HeavyThrowF"],
                UseAct::Throw if u.air => vec!["LightThrowAirF", "LightThrowF"],
                UseAct::Throw => vec!["LightThrowF"],
                UseAct::Swing(level) => {
                    let stem = match u.kind {
                        ItemKind::HomeRunBat => "Bat",
                        ItemKind::Fan => "Harisen",
                        ItemKind::StarRod => "StarRod",
                        _ => "Sword",
                    };
                    return Some((vec![format!("{stem}Swing{level}"), format!("{stem}Swing1")], secs, false));
                }
                UseAct::Shoot if u.air => vec!["LGunShootAir", "LGunShoot"],
                UseAct::Shoot => vec!["LGunShoot"],
                UseAct::Flame if u.air => vec!["FireFlowerShootAir", "FireFlowerShoot"],
                UseAct::Flame => vec!["FireFlowerShoot"],
            };
            return Some((names.into_iter().map(String::from).collect(), secs, matches!(u.act, UseAct::Flame)));
        }
        if self.hammer > 0 && !fighter_air {
            let name = if self.moving { "HammerWalk" } else { "HammerWait" };
            return Some((vec![name.to_string()], (HAMMER_TICKS - self.hammer) as f32 / 30.0, true));
        }
        None
    }

    /// The held item's model, if any (the hammer while worn).
    pub fn in_hand(&self) -> Option<ItemKind> {
        if self.hammer > 0 {
            return Some(ItemKind::Hammer);
        }
        self.held.map(|h| h.kind)
    }
}

/// `ITHAMMER_TIME` (720 frames).
pub const HAMMER_TICKS: u32 = 360;
/// `ITSTAR_INVINCIBLE_TIME` (600 frames).
pub const STAR_TICKS: u32 = 300;

fn b_pressed(m: &MarioRuntime) -> bool {
    m.input & INPUT_B_PRESSED != 0
}

fn consume_b(m: &mut MarioRuntime) {
    m.state.input &= !INPUT_B_PRESSED;
    m.controller.button_pressed &= !B_BUTTON;
}

fn on_ground(m: &MarioRuntime) -> bool {
    m.action & ACT_FLAG_AIR == 0 && matches!(m.action & ACT_GROUP_MASK, ACT_GROUP_STATIONARY | ACT_GROUP_MOVING)
}

/// The item layer of one Mario step (no special running). Returns true when an item use consumed the frame.
pub fn step(m: &mut MarioRuntime) -> bool {
    m.smash.items.moving = m.forward_vel.abs() > 4.0;
    // The use in progress (interrupted by anything that changed Mario's action from outside).
    if let Some(u) = m.smash.items.using {
        if m.action != u.sm64_action {
            m.smash.items.using = None;
            if u.act == UseAct::Throw {
                release(m, u);
            }
        } else {
            return run_use(m, u);
        }
    }
    if m.held_obj.is_some() || m.riding_shell.is_some() || !can_start(m) {
        return false;
    }
    // Hammer: B never punches, the hammer swings by itself (`after` adds its hitbox).
    if m.smash.items.hammer > 0 {
        if b_pressed(m) {
            consume_b(m);
        }
        return false;
    }
    if let Some(h) = m.smash.items.held {
        if !b_pressed(m) {
            return false;
        }
        consume_b(m);
        let air = m.action & ACT_FLAG_AIR != 0;
        let raw = m.controller.stick_x.hypot(m.controller.stick_y);
        if raw >= STICK_THROW || h.kind.heavy() || h.kind.class() == Class::Throw || h.kind.class() == Class::Container {
            let yaw = if raw >= STICK_THROW { m.intended_yaw } else { m.face_angle[1] };
            m.face_angle[1] = yaw;
            return begin(m, UseAct::Throw, h.kind, if h.kind.heavy() { 14 } else { 10 }, air, yaw);
        }
        let yaw = m.face_angle[1];
        return match h.kind.class() {
            Class::Swing => {
                let level = if m.controller.button_down & Z_TRIG != 0 {
                    4
                } else if m.forward_vel.abs() > 6.0 && !air {
                    3
                } else {
                    1
                };
                let len = match level {
                    4 => 22,
                    3 => 16,
                    _ => 12,
                };
                m.smash.items.events.push(ItemEvent::Swing { kind: h.kind, level });
                begin(m, UseAct::Swing(level), h.kind, len, air, yaw)
            }
            Class::Shoot if h.kind == ItemKind::RayGun => begin(m, UseAct::Shoot, h.kind, 10, air, yaw),
            Class::Shoot => begin(m, UseAct::Flame, h.kind, 6, air, yaw),
            _ => false,
        };
    }
    // Pick-up: B next to an item, on the ground.
    if b_pressed(m) && on_ground(m) {
        let feet = m.pos;
        let near = m.smash.items.list.iter().position(|it| {
            !it.dead && matches!(it.state, ItemState::Ground | ItemState::Air) && it.kind != ItemKind::Star && {
                let d = [it.pos[0] - feet[0], it.pos[2] - feet[2]];
                d[0].hypot(d[1]) < PICKUP_RADIUS + it.kind.radius() && (it.pos[1] - feet[1]).abs() < 110.0
            }
        });
        if let Some(i) = near {
            consume_b(m);
            let it = m.smash.items.list.remove(i);
            let yaw = m.face_angle[1];
            match it.kind.class() {
                Class::Food => {
                    let heal = if it.kind == ItemKind::Heart { m.max_health } else { 0x400 as f64 };
                    m.smash.heal += heal;
                    m.smash.items.meals += 1;
                    m.smash.items.last_meal = Some(it.kind);
                    m.smash.items.events.push(ItemEvent::Eaten { kind: it.kind });
                    return begin(m, UseAct::Get, it.kind, 6, false, yaw);
                }
                Class::Hammer => {
                    m.smash.items.hammer = HAMMER_TICKS;
                    m.smash.items.events.push(ItemEvent::HammerOn);
                    return false;
                }
                _ => {
                    m.smash.items.held = Some(Item { state: ItemState::Air, proj: None, ..it });
                    m.smash.items.events.push(ItemEvent::PickedUp { kind: it.kind });
                    return begin(m, UseAct::Get, it.kind, if it.kind.heavy() { 10 } else { 6 }, false, yaw);
                }
            }
        }
    }
    false
}

fn begin(m: &mut MarioRuntime, act: UseAct, kind: ItemKind, len: u32, air: bool, yaw: f64) -> bool {
    let source = SOURCE | (m.smash.new_id() & 0x000F_FFFF);
    let u = ItemUse { act, kind, t: 0, len, air, yaw, source, sm64_action: m.action };
    run_use(m, u)
}

/// One tick of an item use: Mario stands (ground friction) or drifts (air), the item acts on its frames.
fn run_use(m: &mut MarioRuntime, mut u: ItemUse) -> bool {
    m.face_angle[1] = u.yaw;
    // Ground friction / air drift, like a special's `hold`.
    if u.air {
        drift(m, 16.0, 1.0);
        if air(m, GRAVITY, TERMINAL, false) == AIR_LANDED {
            u.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        let v = approach(m.forward_vel, 0.0, 3.0);
        if !ground(m, v) {
            u.air = true;
            set_act(m, ACT_FREEFALL);
        } else {
            set_act(m, ACT_IDLE);
        }
    }
    let t = u.t;
    let kind = u.kind;
    let src = u.source;
    match u.act {
        UseAct::Get | UseAct::Throw => {
            if u.act == UseAct::Throw && t == 3 {
                release(m, u);
            }
        }
        UseAct::Swing(level) => {
            let (start, end) = match level {
                4 => (9, 13),
                3 => (5, 9),
                _ => (3, 6),
            };
            if (start..end).contains(&t) {
                let (reach, radius) = match kind {
                    ItemKind::BeamSword => (115.0, 70.0),
                    ItemKind::HomeRunBat => (95.0, 60.0),
                    ItemKind::Fan => (80.0, 55.0),
                    _ => (90.0, 60.0),
                };
                // Swing damage per level (approximate: the motion scripts hold the real ones): jab, tilt, smash.
                let dmg: [f32; 3] = match kind {
                    ItemKind::BeamSword => [10.0, 14.0, 22.0],
                    ItemKind::HomeRunBat => [12.0, 15.0, 30.0],
                    ItemKind::Fan => [2.0, 3.0, 4.0],
                    _ => [6.0, 7.0, 9.0],
                };
                let d = dmg[match level {
                    4 => 2,
                    3 => 1,
                    _ => 0,
                }];
                let knock = if kind == ItemKind::HomeRunBat && level == 4 { 80.0 } else { 10.0 + d as f64 };
                let pos = at(m, u.yaw, reach, 75.0);
                m.smash.hitboxes.push(Hitbox { source: src, pos, radius, damage: d, kind: HitKind::Normal, knock, rehit: 0, knockback: None, motion: None, normal: false });
            }
            // Star Rod tilt / smash: a star flies (`ITSTARROD_AMMO_*`, WPAttributes 8% / 12%).
            if kind == ItemKind::StarRod && level >= 3 && t == start {
                if let Some(h) = m.smash.items.held.as_mut() {
                    if h.ammo > 0 {
                        h.ammo -= 1;
                        let (speed, dmg) = if level == 4 { (120.0, 12.0) } else { (80.0, 8.0) };
                        shot(m, ProjKind::Star, u.yaw, speed * KV * 0.25, 15, 100.0 * K, dmg, HitKind::Normal, kind);
                        m.smash.items.events.push(ItemEvent::Shot { kind });
                    } else {
                        m.smash.items.events.push(ItemEvent::Empty { kind });
                    }
                }
            }
        }
        UseAct::Shoot => {
            if t == 2 {
                let ammo = m.smash.items.held.map(|h| h.ammo).unwrap_or(0);
                if ammo > 0 {
                    if let Some(h) = m.smash.items.held.as_mut() {
                        h.ammo -= 1;
                    }
                    // `ITLGUN_AMMO_VEL_X` (300 / frame) is too fast for the sim's steps: a third of it, 10%.
                    shot(m, ProjKind::Laser, u.yaw, 300.0 * KV / 3.0, 12, 60.0 * K, 10.0, HitKind::Normal, kind);
                    m.smash.items.events.push(ItemEvent::Shot { kind });
                } else {
                    m.smash.items.events.push(ItemEvent::Empty { kind });
                }
            }
        }
        UseAct::Flame => {
            let held = m.controller.button_down & B_BUTTON != 0;
            let ammo = m.smash.items.held.map(|h| h.ammo).unwrap_or(0);
            if ammo > 0 {
                if let Some(h) = m.smash.items.held.as_mut() {
                    h.ammo = h.ammo.saturating_sub(2);
                }
                // `ITFFLOWER_AMMO_VEL` 30, `ITFFLOWER_AMMO_LIFETIME` 30 frames, WPAttributes 3% fire.
                shot(m, ProjKind::GreenFireball, u.yaw, 30.0 * KV, 15, 135.0 * K * 0.5, 3.0, HitKind::Fire, kind);
                if t % 8 == 0 {
                    m.smash.items.events.push(ItemEvent::Shot { kind });
                }
                if held && u.t + 1 >= u.len {
                    u.len += 1;
                }
            } else if t == 0 {
                m.smash.items.events.push(ItemEvent::Empty { kind });
            }
        }
    }
    u.t += 1;
    if u.t >= u.len {
        m.smash.items.using = None;
        return true;
    }
    u.sm64_action = m.action;
    m.smash.items.using = Some(u);
    true
}

#[allow(clippy::too_many_arguments)]
fn shot(m: &mut MarioRuntime, kind: ProjKind, yaw: f64, speed: f64, life: u32, radius: f64, damage: f32, hit: HitKind, _item: ItemKind) {
    let id = m.smash.new_id();
    let f = fwd(yaw);
    let fighter = m.smash.fighter.unwrap_or(SmashFighter::Mario);
    m.smash.projectiles.push(Projectile {
        id,
        kind,
        fighter,
        mv: SpecialMove::Item,
        pos: at(m, yaw, 60.0, 70.0),
        vel: [f[0] * speed, 0.0, f[1] * speed],
        age: 0,
        life,
        radius,
        damage,
        hit,
        gravity: 0.0,
        terminal: 0.0,
        rehit: 4,
        pierce: kind == ProjKind::GreenFireball,
        level: 0,
        dead: false,
    });
}

/// The held item leaves the hand: thrown along the use's yaw.
fn release(m: &mut MarioRuntime, u: ItemUse) {
    let Some(mut it) = m.smash.items.held.take() else { return };
    let f = fwd(u.yaw);
    // `ftCommonItemThrow`: about 50 forward, 15 up (SSB64 units per frame); heavy items slower.
    let (speed, up) = if it.kind.heavy() { (30.0, 18.0) } else { (50.0, 15.0) };
    it.pos = at(m, u.yaw, 50.0, if it.kind.heavy() { 150.0 } else { 80.0 });
    it.vel = [f[0] * speed * KV, up * KV, f[1] * speed * KV];
    it.yaw = u.yaw;
    it.state = ItemState::Thrown;
    it.t = 0;
    m.smash.items.events.push(ItemEvent::Thrown { kind: it.kind, heavy: it.kind.heavy() });
    let fighter = m.smash.fighter.unwrap_or(SmashFighter::Mario);
    it.proj = Some(sensor(&mut m.smash, fighter, &it, it.kind.damage(), HitKind::Normal, false, 0));
    m.smash.items.list.push(it);
}

/// The hit volume of an item ([`ProjKind::Item`]): kept on the item by [`after`].
#[allow(clippy::too_many_arguments)]
fn sensor(ext: &mut SmashExt, fighter: SmashFighter, it: &Item, damage: f32, hit: HitKind, pierce: bool, rehit: u32) -> u32 {
    let id = ext.new_id();
    ext.projectiles.push(Projectile {
        id,
        kind: ProjKind::Item,
        fighter,
        mv: SpecialMove::Item,
        pos: it.pos,
        vel: [0.0; 3],
        age: 0,
        life: 100_000,
        radius: it.kind.radius(),
        damage,
        hit,
        gravity: 0.0,
        terminal: 0.0,
        rehit,
        pierce,
        level: 0,
        dead: false,
    });
    id
}

fn proj_alive(ext: &SmashExt, id: Option<u32>) -> bool {
    id.is_some_and(|id| ext.projectiles.iter().any(|p| p.id == id && !p.dead))
}

fn drop_proj(ext: &mut SmashExt, id: Option<u32>) {
    if let Some(id) = id {
        for p in ext.projectiles.iter_mut().filter(|p| p.id == id) {
            p.dead = true;
        }
    }
}

/// After Mario's step: the hammer's hitbox, spawning, every item's physics and behaviour, hits landed.
pub fn after(m: &mut MarioRuntime) {
    let fighter = m.smash.fighter.unwrap_or(SmashFighter::Mario);
    let items = &mut m.smash.items;
    items.star = items.star.saturating_sub(1);
    items.hammer = items.hammer.saturating_sub(1);
    // Item hits that landed (the hit sound) and the hammer.
    let mine: Vec<(u32, HitKind, HitOutcome)> = m.smash.landed.iter().filter(|(s, ..)| s & SOURCE != 0).copied().collect();
    m.smash.landed.retain(|(s, ..)| s & SOURCE == 0);
    if !mine.is_empty() {
        let kind = m.smash.items.in_hand().unwrap_or(ItemKind::BeamSword);
        let pos = m.smash.center;
        m.smash.items.events.push(ItemEvent::Hit { kind, pos });
    }
    if m.smash.items.hammer > 0 && m.smash.special.is_none() {
        let t = HAMMER_TICKS - m.smash.items.hammer;
        // The hammer comes down in front about every 12 ticks; between, it is over the head.
        let ahead = if t % 12 < 6 { 80.0 } else { 20.0 };
        let up = if t % 12 < 6 { 60.0 } else { 190.0 };
        let pos = at(m, m.face_angle[1], ahead, up);
        m.smash.hitboxes.push(Hitbox { source: SOURCE | 0x7F_0000, pos, radius: 75.0, damage: 10.0, kind: HitKind::Normal, knock: 25.0, rehit: 8, knockback: None, motion: None, normal: false });
        if t % 12 == 0 {
            m.smash.items.events.push(ItemEvent::Swing { kind: ItemKind::Hammer, level: 1 });
        }
    }
    if !m.smash.items.enabled && m.smash.items.list.is_empty() {
        return;
    }
    // Spawning.
    if m.smash.items.enabled {
        m.smash.items.spawn_timer += 1;
        let (every, max) = m.smash.items.rates.as_ref().map_or((SPAWN_TICKS, MAX_ON_FIELD), |r| (r.every, r.max));
        if m.smash.items.spawn_timer >= every && m.smash.items.list.len() < max {
            m.smash.items.spawn_timer = 0;
            spawn_near(m);
        }
    }
    let mut list = std::mem::take(&mut m.smash.items.list);
    let mut released: Vec<(ItemKind, Vec3)> = vec![];
    for it in list.iter_mut() {
        if it.dead {
            continue;
        }
        it.age += 1;
        it.t += 1;
        step_item(m, it, fighter, &mut released);
    }
    list.retain(|it| !it.dead);
    m.smash.items.list.extend(list);
    for (kind, pos) in released {
        let id = m.smash.items.spawn(kind, pos);
        if let Some(it) = m.smash.items.list.iter_mut().find(|i| i.id == id) {
            it.vel[1] = 20.0;
        }
    }
    // Keep the newest at the end, the field bounded.
    let n = m.smash.items.list.len();
    if n > 12 {
        for it in m.smash.items.list.iter_mut().take(n - 12) {
            it.dead = true;
        }
        m.smash.items.list.retain(|i| !i.dead);
    }
}

fn spawn_near(m: &mut MarioRuntime) {
    for _ in 0..8 {
        let r = m.smash.items.rand();
        let a = (r % 360) as f64 * std::f64::consts::PI / 180.0;
        let d = 200.0 + (m.smash.items.rand() % 500) as f64;
        let x = m.pos[0] + a.sin() * d;
        let z = m.pos[2] + a.cos() * d;
        let floor = m.world.find_floor(x, m.pos[1] + 500.0, z);
        let Some(_) = floor.surface else { continue };
        if floor.height < m.pos[1] - 400.0 || floor.height > m.pos[1] + 500.0 {
            continue;
        }
        // The classic set, the healing ones rarer.
        let pool = [
            ItemKind::BeamSword,
            ItemKind::HomeRunBat,
            ItemKind::Fan,
            ItemKind::StarRod,
            ItemKind::RayGun,
            ItemKind::FireFlower,
            ItemKind::Hammer,
            ItemKind::BobOmb,
            ItemKind::GreenShell,
            ItemKind::RedShell,
            ItemKind::MotionBomb,
            ItemKind::Bumper,
            ItemKind::PokeBall,
            ItemKind::Capsule,
            ItemKind::Crate,
            ItemKind::Barrel,
            ItemKind::MaximTomato,
            ItemKind::Star,
            ItemKind::Heart,
        ];
        let kind = match m.smash.items.rates.clone().filter(|r| !r.pool.is_empty()) {
            Some(r) => {
                let total: u32 = r.pool.iter().map(|p| p.1).sum::<u32>().max(1);
                let mut x = m.smash.items.rand() % total;
                let mut pick = r.pool[0].0;
                for (k, w) in r.pool {
                    if x < w {
                        pick = k;
                        break;
                    }
                    x -= w;
                }
                pick
            }
            None => {
                let k = (m.smash.items.rand() as usize) % (pool.len() + 2);
                pool[k.min(pool.len() - 1)]
            }
        };
        m.smash.items.spawn(kind, [x, floor.height + 300.0, z]);
        return;
    }
}

/// What a container holds (`itBoxProcSpawnItem`-style: a random light item).
fn contents(m: &mut MarioRuntime) -> ItemKind {
    let pool = [ItemKind::BeamSword, ItemKind::HomeRunBat, ItemKind::RayGun, ItemKind::FireFlower, ItemKind::MaximTomato, ItemKind::BobOmb, ItemKind::GreenShell, ItemKind::StarRod, ItemKind::Fan, ItemKind::Star];
    pool[(m.smash.items.rand() as usize) % pool.len()]
}

/// Explosion of a bomb (`ITAttackEvent` size 350 / 30%, `IT*_EXPLODE_*`).
fn explode(m: &mut MarioRuntime, it: &mut Item, fighter: SmashFighter) {
    it.dead = true;
    drop_proj(&mut m.smash, it.proj);
    let radius = 0.5 * 350.0 * K * 1.4;
    m.smash.add_blast(fighter, SpecialMove::Item, [it.pos[0], it.pos[1] + 30.0, it.pos[2]], radius, 30.0, 4);
    m.smash.items.events.push(ItemEvent::Exploded { kind: it.kind, pos: it.pos, radius });
}

fn step_item(m: &mut MarioRuntime, it: &mut Item, fighter: SmashFighter, released: &mut Vec<(ItemKind, Vec3)>) {
    let (g, tv) = it.kind.fall();
    let (g, tv) = (g * KA, tv * KV);
    let hit_something = it.proj.is_some() && !proj_alive(&m.smash, it.proj);
    match it.state {
        ItemState::Air | ItemState::Ground | ItemState::Thrown => {
            if it.state == ItemState::Thrown && hit_something {
                // The throw met a body.
                it.proj = None;
                m.smash.items.events.push(ItemEvent::Hit { kind: it.kind, pos: it.pos });
                match it.kind {
                    ItemKind::BobOmb => return explode(m, it, fighter),
                    ItemKind::Capsule | ItemKind::Crate | ItemKind::Barrel => {
                        it.dead = true;
                        m.smash.items.events.push(ItemEvent::Broke { kind: it.kind, pos: it.pos });
                        released.push((contents(m), it.pos));
                        return;
                    }
                    ItemKind::GreenShell | ItemKind::RedShell => {
                        // Recoil (`ITGSHELL_REBOUND_VEL_Y`) and keep sliding the other way.
                        it.vel = [-it.vel[0] * 0.6, 38.0 * KV * 0.5, -it.vel[2] * 0.6];
                        it.state = ItemState::Active;
                        it.t = 0;
                        it.proj = Some(sensor(&mut m.smash, fighter, it, it.kind.damage(), HitKind::Normal, true, 15));
                    }
                    ItemKind::PokeBall => return open_ball(m, it, fighter),
                    _ => {
                        it.vel = [-it.vel[0] * 0.3, 10.0, -it.vel[2] * 0.3];
                        it.state = ItemState::Air;
                    }
                }
            }
            if it.state == ItemState::Ground {
                it.vel = [0.0; 3];
            } else {
                it.vel[1] = (it.vel[1] - g).max(-tv);
            }
            let before = it.pos;
            for k in 0..3 {
                it.pos[k] += it.vel[k];
            }
            if it.state == ItemState::Thrown {
                it.spin += 0.5;
            }
            // Walls.
            let mut probe = it.pos;
            if m.world.f32_find_wall_collision(&mut probe, 20.0, it.kind.radius().min(40.0)) > 0 {
                it.pos = probe;
                if it.state == ItemState::Thrown {
                    match it.kind {
                        ItemKind::BobOmb => return explode(m, it, fighter),
                        ItemKind::Capsule | ItemKind::Crate | ItemKind::Barrel => {
                            it.dead = true;
                            drop_proj(&mut m.smash, it.proj);
                            m.smash.items.events.push(ItemEvent::Broke { kind: it.kind, pos: it.pos });
                            released.push((contents(m), before));
                            return;
                        }
                        ItemKind::MotionBomb => {
                            it.vel = [0.0; 3];
                            return arm(m, it, fighter);
                        }
                        _ => {
                            it.vel = [-it.vel[0] * 0.3, it.vel[1], -it.vel[2] * 0.3];
                        }
                    }
                }
            }
            let floor = m.world.find_floor(it.pos[0], it.pos[1] + 60.0, it.pos[2]);
            match floor.surface.is_some().then_some(floor.height) {
                Some(h) if it.pos[1] <= h => {
                    it.pos[1] = h;
                    let was_thrown = it.state == ItemState::Thrown;
                    if was_thrown {
                        drop_proj(&mut m.smash, it.proj);
                        it.proj = None;
                        match it.kind {
                            ItemKind::BobOmb => return explode(m, it, fighter),
                            ItemKind::Capsule | ItemKind::Crate | ItemKind::Barrel => {
                                it.dead = true;
                                m.smash.items.events.push(ItemEvent::Broke { kind: it.kind, pos: it.pos });
                                released.push((contents(m), it.pos));
                                return;
                            }
                            ItemKind::MotionBomb => return arm(m, it, fighter),
                            ItemKind::GreenShell | ItemKind::RedShell => {
                                it.state = ItemState::Active;
                                it.t = 0;
                                it.vel[1] = 0.0;
                                it.proj = Some(sensor(&mut m.smash, fighter, it, it.kind.damage(), HitKind::Normal, true, 15));
                                return;
                            }
                            ItemKind::Bumper => {
                                it.state = ItemState::Active;
                                it.t = 0;
                                it.vel = [0.0; 3];
                                it.proj = Some(sensor(&mut m.smash, fighter, it, it.kind.damage(), HitKind::Normal, true, 10));
                                return;
                            }
                            ItemKind::PokeBall => return open_ball(m, it, fighter),
                            _ => {}
                        }
                    }
                    if it.kind == ItemKind::Star {
                        // `ITSTAR_BOUNCE_Y`: the Star hops along (`ITSTAR_VEL_X`).
                        it.vel = [sins(it.yaw) * 8.0 * KV, 50.0 * KV * 0.5, coss(it.yaw) * 8.0 * KV];
                        it.state = ItemState::Air;
                    } else if it.state != ItemState::Ground {
                        it.state = ItemState::Ground;
                        it.vel = [0.0; 3];
                        it.t = 0;
                        m.smash.items.events.push(ItemEvent::Landed { kind: it.kind, pos: it.pos });
                    }
                }
                Some(_) => {
                    if it.state == ItemState::Ground {
                        it.state = ItemState::Air;
                    }
                }
                None => {
                    if it.pos[1] < m.pos[1] - 3000.0 {
                        it.dead = true;
                        drop_proj(&mut m.smash, it.proj);
                        return;
                    }
                }
            }
            if let Some(id) = it.proj {
                if let Some(p) = m.smash.projectiles.iter_mut().find(|p| p.id == id) {
                    p.pos = [it.pos[0], it.pos[1] + 40.0, it.pos[2]];
                }
            }
            // The Star: taken on touch.
            if it.kind == ItemKind::Star && it.age > 8 {
                let d = [it.pos[0] - m.pos[0], it.pos[2] - m.pos[2]];
                if d[0].hypot(d[1]) < PICKUP_RADIUS && (it.pos[1] - m.pos[1]).abs() < 160.0 {
                    it.dead = true;
                    m.smash.items.star = STAR_TICKS;
                    m.smash.items.events.push(ItemEvent::StarGot);
                    return;
                }
            }
            // A Bob-omb left on the ground starts walking (`ITBOMBHEI_WALK_WAIT` 180 frames).
            if it.kind == ItemKind::BobOmb && it.state == ItemState::Ground && it.t >= 90 {
                it.state = ItemState::Active;
                it.t = 0;
                it.yaw = (m.smash.items.rand() % 628) as f64 / 100.0;
                it.proj = Some(sensor(&mut m.smash, fighter, it, 1.0, HitKind::Explosion, false, 0));
            }
            if it.state == ItemState::Ground && it.age > ITEM_LIFE {
                it.dead = true;
                m.smash.items.events.push(ItemEvent::Vanished { kind: it.kind, pos: it.pos });
            }
        }
        ItemState::Active => step_active(m, it, fighter, hit_something),
    }
}

/// The Motion-Sensor Bomb sticks and arms (`ITMSBOMB_DETECT_FIGHTER_DELAY` 100 frames later it senses).
fn arm(m: &mut MarioRuntime, it: &mut Item, _fighter: SmashFighter) {
    drop_proj(&mut m.smash, it.proj);
    it.proj = None;
    it.state = ItemState::Active;
    it.t = 0;
    it.vel = [0.0; 3];
    m.smash.items.events.push(ItemEvent::Armed { pos: it.pos });
}

/// The Poké Ball bursts open (the Pokémon are not exported: a burst of light that hits around it).
fn open_ball(m: &mut MarioRuntime, it: &mut Item, fighter: SmashFighter) {
    it.dead = true;
    drop_proj(&mut m.smash, it.proj);
    let radius = 0.5 * 400.0 * K;
    m.smash.add_blast(fighter, SpecialMove::Item, [it.pos[0], it.pos[1] + 60.0, it.pos[2]], radius, 12.0, 6);
    m.smash.items.events.push(ItemEvent::Broke { kind: it.kind, pos: it.pos });
}

fn step_active(m: &mut MarioRuntime, it: &mut Item, fighter: SmashFighter, hit_something: bool) {
    match it.kind {
        ItemKind::BobOmb => {
            // Walks (`ITBOMBHEI_WALK_VEL_X`), turns at walls and edges, explodes on contact or after ~5 s.
            if hit_something || it.t >= 150 {
                return explode(m, it, fighter);
            }
            let v = 24.0 * KV * 0.25;
            let next = [it.pos[0] + sins(it.yaw) * v, it.pos[1], it.pos[2] + coss(it.yaw) * v];
            let floor = m.world.find_floor(next[0], next[1] + 60.0, next[2]);
            let mut probe = next;
            let wall = m.world.f32_find_wall_collision(&mut probe, 20.0, 30.0) > 0;
            match floor.surface.is_some().then_some(floor.height) {
                Some(h) if !wall && (h - it.pos[1]).abs() < 40.0 => it.pos = [next[0], h, next[2]],
                _ => it.yaw += std::f64::consts::PI,
            }
        }
        ItemKind::GreenShell | ItemKind::RedShell => {
            // Slides (`IT*SHELL_CLAMP_VEL_X`), bounces off walls, slows down; gone after its lifetime.
            let life = if it.kind == ItemKind::GreenShell { 120 } else { 240 };
            let max = if it.kind == ItemKind::GreenShell { 90.0 } else { 70.0 } * KV * 0.5;
            let hs = it.vel[0].hypot(it.vel[2]);
            if hs > max {
                it.vel[0] *= max / hs;
                it.vel[2] *= max / hs;
            }
            if it.kind == ItemKind::RedShell {
                // The red shell turns toward the nearest... fighter in SSB64; here it keeps its line.
            }
            it.vel[1] -= 1.2 * KA;
            let mut next = [it.pos[0] + it.vel[0], it.pos[1] + it.vel[1], it.pos[2] + it.vel[2]];
            let mut probe = next;
            if m.world.f32_find_wall_collision(&mut probe, 20.0, 30.0) > 0 {
                it.vel[0] = -it.vel[0];
                it.vel[2] = -it.vel[2];
                next = probe;
                m.smash.items.events.push(ItemEvent::Landed { kind: it.kind, pos: it.pos });
            }
            let floor = m.world.find_floor(next[0], next[1] + 60.0, next[2]);
            if let Some(h) = floor.surface.is_some().then_some(floor.height) {
                if next[1] <= h {
                    next[1] = h;
                    it.vel[1] = 0.0;
                }
            }
            it.pos = next;
            it.spin += 0.6;
            if it.t >= life || hs < 12.0 * KV * 0.25 {
                drop_proj(&mut m.smash, it.proj);
                it.proj = None;
                it.state = ItemState::Ground;
                it.t = 0;
                it.vel = [0.0; 3];
            }
        }
        ItemKind::MotionBomb => {
            // Armed after `ITMSBOMB_DETECT_FIGHTER_DELAY`: a body in its sensor radius sets it off.
            if it.t == 50 {
                it.proj = Some(sensor(&mut m.smash, fighter, it, 1.0, HitKind::Explosion, false, 0));
                if let Some(id) = it.proj {
                    if let Some(p) = m.smash.projectiles.iter_mut().find(|p| p.id == id) {
                        p.radius = 0.5 * 360.0 * K;
                    }
                }
            }
            if hit_something {
                return explode(m, it, fighter);
            }
            if it.age > ITEM_LIFE * 2 {
                it.dead = true;
                drop_proj(&mut m.smash, it.proj);
            }
        }
        ItemKind::Bumper => {
            // Stays where it landed and bumps whoever touches it (rehit every 10 ticks).
            it.spin += 0.05;
            if it.t > 600 {
                it.dead = true;
                drop_proj(&mut m.smash, it.proj);
                m.smash.items.events.push(ItemEvent::Vanished { kind: it.kind, pos: it.pos });
            }
        }
        _ => {
            it.state = ItemState::Air;
        }
    }
    if let Some(id) = it.proj {
        if let Some(p) = m.smash.projectiles.iter_mut().find(|p| p.id == id) {
            p.pos = [it.pos[0], it.pos[1] + 40.0, it.pos[2]];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ControllerSnapshot;

    fn runtime(f: SmashFighter) -> MarioRuntime {
        use crate::collision::{CollisionWorld, SurfaceOptions};
        use crate::mario_runtime::SpawnPoint;
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
        w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
        let mut m = MarioRuntime::new(std::sync::Arc::new(w), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0);
        m.smash.fighter = Some(f);
        m
    }

    fn tick(m: &mut MarioRuntime, pressed: u32, down: u32, stick: (f64, f64)) {
        let c = ControllerSnapshot { button_pressed: pressed, button_down: down | pressed, stick_x: stick.0, stick_y: stick.1, stick_mag: stick.0.hypot(stick.1), ..Default::default() };
        m.step(&c, 0x8000 as f64, false, false);
    }

    fn settle(m: &mut MarioRuntime, n: usize) {
        for _ in 0..n {
            tick(m, 0, 0, (0.0, 0.0));
        }
    }

    #[test]
    fn items_fall_and_rest_on_the_floor() {
        let mut m = runtime(SmashFighter::Mario);
        settle(&mut m, 5);
        m.smash.items.spawn(ItemKind::BeamSword, [m.pos[0], m.pos[1] + 300.0, m.pos[2] + 300.0]);
        settle(&mut m, 60);
        let it = m.smash.items.list[0];
        assert_eq!(it.state, ItemState::Ground);
        assert!((it.pos[1] - m.pos[1]).abs() < 1.0, "on the floor: {} vs {}", it.pos[1], m.pos[1]);
    }

    #[test]
    fn pick_up_swing_and_throw() {
        let mut m = runtime(SmashFighter::Link);
        settle(&mut m, 5);
        m.smash.items.spawn(ItemKind::BeamSword, [m.pos[0], m.pos[1] + 10.0, m.pos[2] + 30.0]);
        settle(&mut m, 10);
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        assert_eq!(m.smash.items.held.map(|h| h.kind), Some(ItemKind::BeamSword), "picked up");
        assert!(m.smash.items.list.is_empty());
        settle(&mut m, 10);
        assert!(m.smash.items.using.is_none());
        // Swing: a hitbox in front during the active frames.
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        let mut swung = false;
        for _ in 0..12 {
            swung |= !m.smash.hitboxes.is_empty();
            assert!(m.smash.items.motion(false).is_some_and(|(n, ..)| n[0] == "SwordSwing1") || m.smash.items.using.is_none());
            settle(&mut m, 1);
        }
        assert!(swung, "the swing made a hitbox");
        // Throw: stick pushed hard.
        settle(&mut m, 5);
        tick(&mut m, B_BUTTON, 0, (0.0, 64.0));
        settle(&mut m, 4);
        assert!(m.smash.items.held.is_none(), "thrown");
        let it = m.smash.items.list.iter().find(|i| i.kind == ItemKind::BeamSword).expect("in the air");
        assert!(matches!(it.state, ItemState::Thrown | ItemState::Ground | ItemState::Air));
        assert!(m.smash.projectiles.iter().any(|p| p.kind == ProjKind::Item) || it.state != ItemState::Thrown);
    }

    #[test]
    fn food_heals_and_star_protects() {
        let mut m = runtime(SmashFighter::Mario);
        settle(&mut m, 5);
        m.health = 0x300 as f64;
        m.smash.items.spawn(ItemKind::MaximTomato, [m.pos[0], m.pos[1] + 5.0, m.pos[2] + 20.0]);
        settle(&mut m, 5);
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        settle(&mut m, 2);
        assert!(m.health >= 0x700 as f64, "healed: {:x}", m.health as u32);
        m.smash.items.spawn(ItemKind::Star, [m.pos[0], m.pos[1] + 5.0, m.pos[2] + 10.0]);
        settle(&mut m, 12);
        assert!(m.smash.items.star > 0, "star taken");
        assert!(m.smash.armor(), "invincible");
        assert!(!m.take_damage(1.0, None));
    }

    #[test]
    fn thrown_bob_omb_explodes_and_ray_gun_shoots() {
        let mut m = runtime(SmashFighter::Fox);
        settle(&mut m, 5);
        m.smash.items.spawn(ItemKind::BobOmb, [m.pos[0], m.pos[1] + 5.0, m.pos[2] + 20.0]);
        settle(&mut m, 5);
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        settle(&mut m, 10);
        assert_eq!(m.smash.items.held.map(|h| h.kind), Some(ItemKind::BobOmb));
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        let mut boom = false;
        for _ in 0..60 {
            settle(&mut m, 1);
            boom |= m.smash.items.events.iter().any(|e| matches!(e, ItemEvent::Exploded { .. }));
        }
        assert!(boom, "the thrown Bob-omb blew up on landing");
        m.smash.items.spawn(ItemKind::RayGun, [m.pos[0], m.pos[1] + 5.0, m.pos[2] + 20.0]);
        settle(&mut m, 5);
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        settle(&mut m, 10);
        tick(&mut m, B_BUTTON, 0, (0.0, 0.0));
        let mut shot = false;
        for _ in 0..6 {
            shot |= m.smash.projectiles.iter().any(|p| p.mv == SpecialMove::Item && p.kind == ProjKind::Laser);
            settle(&mut m, 1);
        }
        assert!(shot, "a ray gun shot");
        assert_eq!(m.smash.items.held.map(|h| h.ammo), Some(15));
    }

    #[test]
    fn spawner_drops_items_near_the_fighter() {
        let mut m = runtime(SmashFighter::Kirby);
        m.smash.items.enabled = true;
        settle(&mut m, SPAWN_TICKS as usize + 5);
        assert_eq!(m.smash.items.list.len(), 1);
        let it = m.smash.items.list[0];
        let d = (it.pos[0] - m.pos[0]).hypot(it.pos[2] - m.pos[2]);
        assert!(d < 800.0, "near: {d}");
    }

    #[test]
    fn every_kind_has_a_model_and_parses() {
        for k in ItemKind::ALL {
            assert_eq!(ItemKind::parse(k.model()), Some(k));
        }
        assert_eq!(ItemKind::parse("sword"), Some(ItemKind::BeamSword));
    }
}
