//! MK64 items: all 15 of `enum item` (`include/defines.h`), the roulette curves, and the item actors (bananas, the
//! banana bunch, green / red / blue shells, triple shells, the fake item box). Engine-free; one `step` is one 60 Hz
//! actor update (`update_course_actors`), the race runs it twice per 30 Hz tick like the karts.
//!
//! Ported from the decomp: `src/actors/banana/update.inc.c` (`update_actor_banana`), `src/actors/green_shell/update.inc.c`
//! (`update_actor_green_shell`), `src/actors/blue_and_red_shells/update.inc.c` (`update_actor_red_blue_shell`,
//! `func_802B3B44` path following, `func_802B3E7C` homing, `func_802B4104` wall pop), `src/actors/fake_item_box/update.inc.c`,
//! `src/racing/actors_extended.c` (`player_use_item`, `use_*_item`, `update_actor_banana_bunch`, `update_actor_triple_shell`,
//! `init_triple_shell`, `prevent_item_use`), `src/racing/actors.c` (`evaluate_collision_between_player_actor`,
//! `evaluate_collision_for_destructible_actors`, `destroy_destructable_actor`, `query_collision_player_vs_actor_item`),
//! `src/racing/collision.c` (`shell_collision` / `process_shell_collision`) and `src/update_objects.c` (`gen_random_item`,
//! `func_8007B040` the boo's steal, `consume_item`, the item window state machine's timings).
//!
//! Positions are the decomp's: course units, the kart centre `KART_RADIUS` above the physics origin (`Body::from_kart`
//! does the shift). Approximations, all listed in `native/README.md`: the kart orientation matrix is yaw only (no slope
//! pitch when placing held items), shells use the physics `Ground` (floor + wall normals) instead of the decomp's
//! collision grid, and a shell that bounces for 20 s is retired (the decomp has none).
use super::kart_effects as fx;
use super::kart_item_curves as curves;
use super::kart_physics::{Ground, Kart};
use std::f64::consts::PI;

pub const ANGLE: f64 = PI / 32768.;
pub const DEG: i32 = 182;
/// `ACTOR_LIST_SIZE`
pub const ACTOR_LIST_SIZE: usize = 100;
/// `gKartBoundingBoxSizeTable` (Mario); the actor reach is `radius + actor radius`.
pub const KART_RADIUS: f64 = fx::KART_RADIUS;
pub const BANANA_RADIUS: f64 = 2.;
pub const SHELL_RADIUS: f64 = 4.;
/// `ACTOR_FAKE_ITEM_BOX`: `unk_08 = 0.35`, `boundingBoxSize = 5.5 * sizeScaling`.
pub const FAKE_BOX_RADIUS: f64 = 5.5;
/// An item box (`ACTOR_ITEM_BOX`: `boundingBoxSize = 20`) reaches a kart at 20 + 5.5.
pub const ITEM_BOX_RADIUS: f64 = 20.;
/// The item window roulette: 15 icons at 2 frames, 6 at 8, 4 at 16 (`func_8007B34C` states 3, 4, 5), 142 frames.
pub const ROULETTE_FRAMES: u32 = 15 * 2 + 6 * 8 + 4 * 16;
/// `object->unk_04C = 0x32`: Z cannot skip the roulette during the first 50 frames.
pub const ROULETTE_NO_SKIP_FRAMES: u32 = 50;
/// `goldenMushroomTimer = 0x258`
pub const GOLDEN_FRAMES: i32 = 0x258;
/// The delay after a mushroom of a stack is used (`func_800726CC(.., 0x28)`).
pub const MUSHROOM_STACK_DELAY: u32 = 0x28;
/// The owner cannot hit its own shell / banana / box for this long after it left (`someTimer = 0x1E` / banana `0xB4` / box 100).
pub const SHELL_PROTECT: i32 = 30;
pub const BANANA_PROTECT: i32 = 0xB4;
pub const FAKE_BOX_PROTECT: i32 = 100;
/// A bouncing green shell never expires in the decomp; this retires it (the decomp's out-of-bounds test needs course bounds).
pub const SHELL_MAX_FRAMES: i32 = 20 * 60;

/// `enum item` (ids as in the ROM tables).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Item {
    Banana,
    BananaBunch,
    GreenShell,
    TripleGreenShell,
    RedShell,
    TripleRedShell,
    BlueShell,
    Thunderbolt,
    FakeBox,
    Star,
    Boo,
    Mushroom,
    DoubleMushroom,
    TripleMushroom,
    SuperMushroom,
}
impl Item {
    pub const ALL: [Item; 15] = [
        Item::Banana,
        Item::BananaBunch,
        Item::GreenShell,
        Item::TripleGreenShell,
        Item::RedShell,
        Item::TripleRedShell,
        Item::BlueShell,
        Item::Thunderbolt,
        Item::FakeBox,
        Item::Star,
        Item::Boo,
        Item::Mushroom,
        Item::DoubleMushroom,
        Item::TripleMushroom,
        Item::SuperMushroom,
    ];
    /// `ITEM_*` id (`ITEM_NONE` is 0).
    pub fn id(self) -> u8 {
        Self::ALL.iter().position(|i| *i == self).unwrap() as u8 + 1
    }
    pub fn from_id(id: u8) -> Option<Item> {
        (id >= 1).then(|| Self::ALL.get(id as usize - 1).copied()).flatten()
    }
    pub fn name(self) -> &'static str {
        match self {
            Item::Banana => "Banana",
            Item::BananaBunch => "Banana bunch",
            Item::GreenShell => "Green shell",
            Item::TripleGreenShell => "Triple green shells",
            Item::RedShell => "Red shell",
            Item::TripleRedShell => "Triple red shells",
            Item::BlueShell => "Spiny shell",
            Item::Thunderbolt => "Thunderbolt",
            Item::FakeBox => "Fake item box",
            Item::Star => "Star",
            Item::Boo => "Boo",
            Item::Mushroom => "Mushroom",
            Item::DoubleMushroom => "Double mushroom",
            Item::TripleMushroom => "Triple mushroom",
            Item::SuperMushroom => "Super mushroom",
        }
    }
    /// The `hud-item-<icon>.png` exported from the ROM's item window.
    pub fn icon(self) -> &'static str {
        match self {
            Item::Banana => "banana",
            Item::BananaBunch => "banana-bunch",
            Item::GreenShell => "green-shell",
            Item::TripleGreenShell => "triple-green-shell",
            Item::RedShell => "red-shell",
            Item::TripleRedShell => "triple-red-shell",
            Item::BlueShell => "blue-shell",
            Item::Thunderbolt => "thunder-bolt",
            Item::FakeBox => "fake-item-box",
            Item::Star => "star",
            Item::Boo => "boo",
            Item::Mushroom => "mushroom",
            Item::DoubleMushroom => "double-mushroom",
            Item::TripleMushroom => "triple-mushroom",
            Item::SuperMushroom => "super-mushroom",
        }
    }
    /// The HUD / toast tint.
    pub fn colour(self) -> [f32; 3] {
        match self {
            Item::Banana | Item::BananaBunch => [0.95, 0.8, 0.17],
            Item::GreenShell | Item::TripleGreenShell => [0.27, 0.71, 0.29],
            Item::RedShell | Item::TripleRedShell => [0.9, 0.2, 0.2],
            Item::BlueShell => [0.25, 0.4, 0.95],
            Item::Thunderbolt => [1., 0.9, 0.2],
            Item::FakeBox => [0.9, 0.45, 0.2],
            Item::Star => [1., 0.85, 0.3],
            Item::Boo => [0.85, 0.85, 0.95],
            Item::Mushroom | Item::DoubleMushroom | Item::TripleMushroom => [0.9, 0.28, 0.24],
            Item::SuperMushroom => [1., 0.8, 0.1],
        }
    }
    pub fn is_mushroom(self) -> bool {
        matches!(self, Item::Mushroom | Item::DoubleMushroom | Item::TripleMushroom | Item::SuperMushroom)
    }
    /// Items that hang behind the kart until released (`DRAG_ITEM_EFFECT`) or orbit it.
    pub fn is_held_actor(self) -> bool {
        !self.is_mushroom() && !matches!(self, Item::Star | Item::Boo | Item::Thunderbolt)
    }
}

// ---- roulette -------------------------------------------------------------------------------------------------

/// Which of `gen_random_item`'s curve tables is in force (`gModeSelection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    GrandPrix,
    /// 2, 3 or 4 players.
    Versus(usize),
    Battle,
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}
/// The 100 ids of one rank row.
pub fn curve_row(curve: Curve, cpu: bool, row: usize) -> Vec<u8> {
    let table: &[&str] = match curve {
        Curve::GrandPrix if cpu => &curves::CPU_GP,
        Curve::GrandPrix => &curves::HUMAN_GP,
        Curve::Versus(2) => &curves::VERSUS_2,
        Curve::Versus(3) => &curves::VERSUS_3,
        Curve::Versus(_) => &curves::VERSUS_4,
        Curve::Battle => &curves::BATTLE,
    };
    hex(table[row.min(table.len() - 1)])
}
/// Map a 0-based position in a field of `racers` onto the 8 grand prix rows (the game always has 8 racers):
/// first stays first, last stays last.
pub fn gp_row(rank: usize, racers: usize) -> usize {
    if racers >= 8 {
        rank.min(7)
    } else if racers <= 1 {
        0
    } else {
        (rank.min(racers - 1) as f64 * 7. / (racers - 1) as f64).round() as usize
    }
}

/// xorshift32 for the roulette and the shells' random bits.
#[derive(Clone, Debug)]
pub struct ItemRng(pub u32);
impl ItemRng {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// `random_int(n)`
    pub fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n.max(1)
    }
}

/// `gen_random_item(rank, isCpu)`: `curve[rank * 100 + sRandomItemIndex]`.
pub fn gen_random_item(curve: Curve, cpu: bool, rank: usize, racers: usize, rng: &mut ItemRng) -> Item {
    let row = match curve {
        Curve::GrandPrix => gp_row(rank, racers),
        Curve::Versus(n) => rank.min(n.clamp(2, 4) - 1),
        Curve::Battle => 0,
    };
    let id = curve_row(curve, cpu, row)[rng.below(100) as usize];
    Item::from_id(id).unwrap_or(Item::Banana)
}

// ---- kart-side inputs -----------------------------------------------------------------------------------------

/// Everything the item code reads about a kart. Positions are in the decomp's frame (centre = physics origin + radius).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    /// `rotation[1]` (s16).
    pub rotation: i32,
    /// `unk_0C0` (s16): the drift slip angle added to the heading.
    pub slip: i32,
    pub speed: f64,
    pub radius: f64,
    /// 0 = first place.
    pub rank: usize,
    /// Nearest path node (`gNearestPathPointByPlayerId`).
    pub node: usize,
    pub exists: bool,
    pub human: bool,
    /// `player->effects`
    pub flags: u32,
    /// `player->triggers` already pending.
    pub triggers: u32,
}
impl Body {
    pub fn from_kart(k: &Kart, fx: &fx::Effects, rank: usize, node: usize, human: bool) -> Self {
        Self {
            pos: [k.pos[0], k.pos[1] + KART_RADIUS, k.pos[2]],
            vel: k.velocity,
            rotation: k.rotation,
            slip: k.slip_raw(),
            speed: k.speed,
            radius: fx.bounding_box,
            rank,
            node,
            exists: true,
            human,
            flags: fx.flags,
            triggers: fx.triggers,
        }
    }
    /// `rotation[1] + unk_0C0`: where the kart's nose points.
    pub fn aim(&self) -> i32 {
        (self.rotation + self.slip) as i16 as i32
    }
}

/// The item button (Z in the original) and the stick, one frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pad {
    /// `buttonPressed & Z_TRIG`: use the held item.
    pub press: bool,
    /// `buttonDepressed & Z_TRIG`: let go of what is held behind you.
    pub release: bool,
    pub stick_x: i32,
    pub stick_y: i32,
}
/// The call a CPU controller makes: the same two edges and stick a human pad would give. Pass it to
/// `ItemWorld::set_use`; it applies on the next `step`.
pub type ItemUse = Pad;

fn rot_y(v: [f64; 3], a: i32) -> [f64; 3] {
    let (s, c) = ((a as f64 * ANGLE).sin(), (a as f64 * ANGLE).cos());
    [v[0] * c - v[2] * s, v[1], v[0] * s + v[2] * c]
}
fn fwd(a: i32) -> [f64; 3] {
    rot_y([0., 0., 1.], a)
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn len(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn s16(v: i64) -> i32 {
    v as i16 as i32
}

// ---- actors ---------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Banana,
    Green,
    Red,
    Blue,
    Fake,
    BananaBunch,
    TripleGreen,
    TripleRed,
}
impl Kind {
    pub fn is_parent(self) -> bool {
        matches!(self, Kind::BananaBunch | Kind::TripleGreen | Kind::TripleRed)
    }
}

/// `enum ShellState`
pub mod shell {
    pub const HELD: u8 = 0;
    pub const RELEASED: u8 = 1;
    pub const MOVING: u8 = 2;
    pub const RED_LOCK_ON: u8 = 3;
    pub const TRIPLE_GREEN: u8 = 4;
    pub const HIT_A_RACER: u8 = 5;
    pub const TRIPLE_RED: u8 = 6;
    pub const DESTROYED: u8 = 7;
    pub const BLUE_LOCK_ON: u8 = 8;
    pub const BLUE_ELIMINATED: u8 = 9;
}
/// `enum BananaState`
pub mod banana {
    pub const HELD: u8 = 0;
    pub const DROPPED: u8 = 1;
    pub const FIRST_BUNCH: u8 = 2;
    pub const BUNCH: u8 = 3;
    pub const ON_GROUND: u8 = 4;
    pub const DESTROYED: u8 = 5;
}
pub mod fake {
    pub const HELD: u8 = 0;
    pub const ON_GROUND: u8 = 1;
    pub const DESTROYED: u8 = 2;
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub id: u32,
    pub kind: Kind,
    pub state: u8,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub owner: usize,
    pub radius: f64,
    /// `someTimer` / `unk_04`.
    pub timer: i32,
    /// `rotAngle` (s16 arithmetic).
    pub rot_angle: i32,
    /// `rotVelocity`: the shell's sprite spin (+10 degrees per frame) / a triple shell parent's orbit speed.
    pub rot_vel: i32,
    /// `flags & 0x4000`: can hit karts.
    pub collidable: bool,
    /// `flags & 0x1000`: the owner passes through it.
    pub protect: bool,
    pub target: usize,
    pub path_index: usize,
    pub parent: Option<u32>,
    pub elder: Option<u32>,
    pub younger: Option<u32>,
    /// Triple shell parent: child ids; banana bunch parent: the five bananas.
    pub children: [Option<u32>; 5],
    pub slot: usize,
    /// Triple shell parent / bunch: the spawn state, shells or bananas still available.
    pub spawn: u8,
    pub available: i32,
    /// `unk_08` of a triple shell parent: Z presses waiting to fire.
    pub fire: i32,
    pub size: f64,
    pub age: i32,
    /// Spin of a destroyed actor (radians drawn).
    pub spin: f64,
}
impl Actor {
    fn new(id: u32, kind: Kind, owner: usize, pos: [f64; 3]) -> Self {
        let (radius, protect) = match kind {
            Kind::Banana => (BANANA_RADIUS, true),
            Kind::Green | Kind::Red | Kind::Blue => (SHELL_RADIUS, true),
            Kind::Fake => (FAKE_BOX_RADIUS * 0.35, true),
            _ => (0., false),
        };
        Self {
            id,
            kind,
            state: 0,
            pos,
            vel: [0.; 3],
            owner,
            radius,
            timer: 0,
            rot_angle: 0,
            rot_vel: 0,
            collidable: !kind.is_parent(),
            protect,
            target: 0,
            path_index: 0,
            parent: None,
            elder: None,
            younger: None,
            children: [None; 5],
            slot: 0,
            spawn: 0,
            available: 0,
            fire: 0,
            size: 0.35,
            age: 0,
            spin: 0.,
        }
    }
    /// Does this actor take part in the world (is drawn, collides) rather than being an invisible parent.
    pub fn visible(&self) -> bool {
        !self.kind.is_parent()
    }
    pub fn is_shell(&self) -> bool {
        matches!(self.kind, Kind::Green | Kind::Red | Kind::Blue)
    }
    /// Gone for good a moment after (the decomp's `destroy_actor`).
    fn dying(&self) -> bool {
        match self.kind {
            Kind::Banana => self.state == banana::DESTROYED,
            Kind::Fake => self.state == fake::DESTROYED,
            Kind::Green => self.state == shell::HIT_A_RACER && !self.collidable,
            Kind::Red | Kind::Blue => self.state == shell::DESTROYED,
            // A triple shell / bunch parent with nothing left.
            _ => self.state == 255,
        }
    }
}

/// What a kart ran into: pushed into the kart's `Effects::triggers` by the race.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub racer: usize,
    pub trigger: u32,
    pub item: Item,
    pub owner: usize,
}

/// For sounds and the HUD toasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemEvent {
    /// The roulette began / stopped on this item.
    Roll(usize),
    Got(usize, Item),
    /// Used (the Z press consumed it).
    Use(usize, Item),
    /// Dropped / thrown / fired a held actor.
    Release(usize, Item),
    Hit(Hit),
    /// A shell or banana was destroyed (hit something that is not a kart).
    Pop(Item),
    /// A shell hit a wall (`SOUND_ARG_LOAD(0x19, 0x00, 0x80, 0x54)`).
    Bounce(Item),
    Thunder(usize),
    /// The boo took an item (or tried: `None`).
    Steal(usize, Option<Item>),
    /// A red or blue shell locked on.
    LockOn(Item),
    /// A triple item spawned one more of its three.
    Orbit(usize),
}

/// One racer's item window.
#[derive(Clone, Debug, Default)]
pub struct Slot {
    pub held: Option<Item>,
    /// Frames of roulette left (0 = not rolling).
    pub roulette: u32,
    rolled: u32,
    pending: Option<Item>,
    /// `DRAG_ITEM_EFFECT`: something hangs behind the kart.
    pub drag: bool,
    pub golden: i32,
    cooldown: u32,
    pub held_actor: Option<u32>,
    pub parent_actor: Option<u32>,
}

pub struct Ctx<'a> {
    pub bodies: &'a [Body],
    pub pads: &'a [Pad],
    pub ground: &'a dyn Ground,
    /// `gCurrentTrackPath` (course units, floor level).
    pub path: &'a [[f64; 3]],
    /// Course bounds: below `min_y` an actor is destroyed (`gCourseMinY`).
    pub min_y: f64,
}

pub struct ItemWorld {
    pub actors: Vec<Actor>,
    next_id: u32,
    pub slots: Vec<Slot>,
    pub events: Vec<ItemEvent>,
    pub rng: ItemRng,
    pub curve: Curve,
    /// Versus / battle boo: steal from another racer instead of rolling a random item (`func_8007B040`).
    pub boo_steals: bool,
    pub frame: u32,
    queued: Vec<Pad>,
    /// Triggers to give each racer on the next `drain_triggers` (items used on yourself and on others).
    out: Vec<(usize, u32)>,
}

impl ItemWorld {
    pub fn new(racers: usize, seed: u32) -> Self {
        Self {
            actors: vec![],
            next_id: 1,
            slots: vec![Slot::default(); racers],
            events: vec![],
            rng: ItemRng(seed.max(1)),
            curve: Curve::GrandPrix,
            boo_steals: false,
            frame: 0,
            queued: vec![Pad::default(); racers],
            out: vec![],
        }
    }
    pub fn reset(&mut self, racers: usize, seed: u32) {
        *self = Self { curve: self.curve, boo_steals: self.boo_steals, ..Self::new(racers, seed) };
    }
    pub fn get(&self, id: u32) -> Option<&Actor> {
        self.actors.iter().find(|a| a.id == id)
    }
    fn get_mut(&mut self, id: u32) -> Option<&mut Actor> {
        self.actors.iter_mut().find(|a| a.id == id)
    }
    fn spawn(&mut self, kind: Kind, owner: usize, pos: [f64; 3]) -> u32 {
        // add_actor_to_empty_slot: a full list recycles dead things (here: the oldest that is not held).
        if self.actors.len() >= ACTOR_LIST_SIZE {
            if let Some(i) = self.actors.iter().position(|a| a.state != 0 && !a.kind.is_parent()) {
                self.actors.remove(i);
            } else {
                self.actors.remove(0);
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.actors.push(Actor::new(id, kind, owner, pos));
        id
    }

    // -- roulette and use ---------------------------------------------------------------------------------

    /// A kart touched an item box (`func_8007ABFC`): the human's window starts rolling; a CPU takes its item at once
    /// (`cpu_gen_random_item`). Does nothing while the window is busy.
    pub fn pickup(&mut self, racer: usize, rank: usize, racers: usize, human: bool) -> bool {
        let slot = &self.slots[racer];
        if slot.held.is_some() || slot.roulette > 0 {
            return false;
        }
        let item = gen_random_item(self.curve, !human, rank, racers, &mut self.rng);
        self.slots[racer].pending = Some(item);
        if human {
            let s = &mut self.slots[racer];
            s.roulette = ROULETTE_FRAMES;
            s.rolled = 0;
            self.events.push(ItemEvent::Roll(racer));
        } else {
            self.give(racer, item);
        }
        true
    }
    /// `cpu_gen_random_item(lap, rank)` = `gen_random_item(rank, true)`: a draw from the CPU curve.
    pub fn cpu_draw(&mut self, rank: usize, racers: usize) -> Item {
        gen_random_item(self.curve, true, rank, racers, &mut self.rng)
    }
    /// `random_int(n)` on the item stream.
    pub fn random_int(&mut self, n: u32) -> u32 {
        self.rng.below(n)
    }
    /// Empty a racer's window (a CPU never keeps an item there).
    pub fn discard(&mut self, racer: usize) {
        let s = &mut self.slots[racer];
        s.held = None;
        s.pending = None;
        s.roulette = 0;
        s.golden = 0;
    }
    /// Put an item in a racer's window directly (the boo's steal, debug hooks, the AI's strategy code).
    pub fn give(&mut self, racer: usize, item: Item) {
        let s = &mut self.slots[racer];
        s.held = Some(item);
        s.roulette = 0;
        s.pending = None;
        s.golden = 0;
        self.events.push(ItemEvent::Got(racer, item));
    }
    /// Put an actor on the track in a given state, unprotected (test props, course scripts).
    pub fn place(&mut self, kind: Kind, owner: usize, pos: [f64; 3], state: u8) -> u32 {
        let id = self.spawn(kind, owner, pos);
        let a = self.get_mut(id).unwrap();
        a.state = state;
        a.protect = false;
        if kind == Kind::Fake {
            a.size = 1.;
            a.radius = FAKE_BOX_RADIUS;
        }
        id
    }
    /// Queue this frame's item pad for a racer (a CPU controller calls this each tick; edges are consumed by `step`).
    pub fn set_use(&mut self, racer: usize, pad: ItemUse) {
        let q = &mut self.queued[racer];
        q.press |= pad.press;
        q.release |= pad.release;
        q.stick_x = pad.stick_x;
        q.stick_y = pad.stick_y;
    }
    pub fn held(&self, racer: usize) -> Option<Item> {
        self.slots[racer].held
    }
    /// Is a held actor waiting for its release edge (a banana dragging, a shell about to be fired)?
    pub fn is_holding_actor(&self, racer: usize) -> bool {
        self.slots[racer].held_actor.is_some() || self.slots[racer].parent_actor.is_some()
    }
    pub fn drain_events(&mut self) -> Vec<ItemEvent> {
        std::mem::take(&mut self.events)
    }
    /// Triggers the used items hand to karts (mushroom, star, boo on yourself, lightning on the others).
    pub fn drain_triggers(&mut self) -> Vec<(usize, u32)> {
        std::mem::take(&mut self.out)
    }

    /// `player_use_item` + `consume_item`.
    fn use_item(&mut self, racer: usize, ctx: &Ctx) {
        let Some(item) = self.slots[racer].held else { return };
        if self.slots[racer].roulette > 0 || self.slots[racer].cooldown > 0 {
            return;
        }
        let b = ctx.bodies[racer];
        if fx::prevents_item(b.flags, item.is_mushroom(), matches!(item, Item::Star | Item::Boo)) {
            return;
        }
        let s = &mut self.slots[racer];
        match item {
            Item::Mushroom | Item::DoubleMushroom | Item::TripleMushroom | Item::SuperMushroom => {
                self.out.push((racer, fx::SHROOM_TRIGGER));
                match item {
                    Item::SuperMushroom => {
                        if s.golden == 0 {
                            s.golden = GOLDEN_FRAMES;
                        }
                    }
                    Item::TripleMushroom => {
                        s.held = Some(Item::DoubleMushroom);
                        s.cooldown = MUSHROOM_STACK_DELAY;
                    }
                    Item::DoubleMushroom => {
                        s.held = Some(Item::Mushroom);
                        s.cooldown = MUSHROOM_STACK_DELAY;
                    }
                    _ => s.held = None,
                }
            }
            Item::Star => {
                self.out.push((racer, fx::STAR_TRIGGER));
                s.held = None;
            }
            Item::Boo => {
                self.out.push((racer, fx::BOO_TRIGGER));
                s.held = None;
                self.boo(racer, ctx);
            }
            Item::Thunderbolt => {
                // use_thunder_item: every other player gets LIGHTNING_STRIKE_TRIGGER
                s.held = None;
                for o in 0..ctx.bodies.len() {
                    if o != racer && ctx.bodies[o].exists {
                        self.out.push((o, fx::LIGHTNING_STRIKE_TRIGGER));
                    }
                }
                self.events.push(ItemEvent::Thunder(racer));
            }
            Item::Banana => {
                s.held = None;
                s.drag = true;
                let id = self.make_banana(racer, &b, banana::HELD);
                self.slots[racer].held_actor = Some(id);
            }
            Item::FakeBox => {
                s.held = None;
                s.drag = true;
                let p = self.behind(&b, -(b.radius + 4.));
                let id = self.spawn(Kind::Fake, racer, p);
                let a = self.get_mut(id).unwrap();
                a.state = fake::HELD;
                a.vel = b.vel;
                self.slots[racer].held_actor = Some(id);
            }
            Item::BananaBunch => {
                s.held = None;
                s.drag = true;
                let id = self.spawn(Kind::BananaBunch, racer, [0.; 3]);
                self.slots[racer].parent_actor = Some(id);
            }
            Item::GreenShell | Item::RedShell | Item::BlueShell => {
                s.held = None;
                let kind = match item {
                    Item::GreenShell => Kind::Green,
                    Item::RedShell => Kind::Red,
                    _ => Kind::Blue,
                };
                let p = self.behind(&b, b.radius - 4.);
                let id = self.spawn(kind, racer, p);
                let a = self.get_mut(id).unwrap();
                a.state = shell::HELD;
                a.rot_angle = if item == Item::GreenShell { s16(-180 * DEG as i64) } else { s16(b.rotation as i64 - 180 * DEG as i64) };
                a.target = racer;
                self.slots[racer].held_actor = Some(id);
            }
            Item::TripleGreenShell | Item::TripleRedShell => {
                s.held = None;
                let kind = if item == Item::TripleGreenShell { Kind::TripleGreen } else { Kind::TripleRed };
                let id = self.spawn(kind, racer, [0.; 3]);
                let a = self.get_mut(id).unwrap();
                a.rot_vel = 8 * DEG;
                a.rot_angle = s16(-180 * DEG as i64);
                self.slots[racer].parent_actor = Some(id);
            }
        }
        self.events.push(ItemEvent::Use(racer, item));
    }

    /// `func_8007B040`: the boo's steal. GP / time trials: an 81 % chance of a fresh item from the human curve.
    fn boo(&mut self, racer: usize, ctx: &Ctx) {
        if !self.boo_steals {
            let item = if self.rng.below(100) < 0x51 {
                Some(gen_random_item(self.curve, false, ctx.bodies[racer].rank, ctx.bodies.len(), &mut self.rng))
            } else {
                None
            };
            if let Some(i) = item {
                self.give(racer, i);
            }
            self.events.push(ItemEvent::Steal(racer, item));
        } else {
            let from: Vec<usize> = (0..self.slots.len()).filter(|o| *o != racer && self.slots[*o].held.is_some()).collect();
            if from.is_empty() {
                self.events.push(ItemEvent::Steal(racer, None));
                return;
            }
            let v = from[self.rng.below(from.len() as u32) as usize];
            let item = self.slots[v].held.take();
            if let Some(i) = item {
                self.give(racer, i);
            }
            self.events.push(ItemEvent::Steal(racer, item));
        }
    }

    /// `startingPos = (0, -r, -(r + 4))` rotated into the kart frame, plus the kart position.
    fn behind(&self, b: &Body, z: f64) -> [f64; 3] {
        let o = rot_y([0., -b.radius, z], b.rotation);
        [b.pos[0] + o[0], b.pos[1] + o[1], b.pos[2] + o[2]]
    }
    fn make_banana(&mut self, racer: usize, b: &Body, state: u8) -> u32 {
        let p = self.behind(b, -(b.radius + 4.));
        let id = self.spawn(Kind::Banana, racer, p);
        let a = self.get_mut(id).unwrap();
        a.state = state;
        a.timer = 0x14;
        a.vel = b.vel;
        id
    }

    // -- the frame ----------------------------------------------------------------------------------------

    /// One 60 Hz actor update: pad edges (use, release, fire), actor motion, collisions. Returns the kart hits.
    pub fn step(&mut self, ctx: &Ctx) -> Vec<Hit> {
        self.frame += 1;
        let racers = self.slots.len();
        let mut pads: Vec<Pad> = (0..racers).map(|i| ctx.pads.get(i).copied().unwrap_or_default()).collect();
        for (i, q) in self.queued.iter_mut().enumerate() {
            pads[i].press |= q.press;
            pads[i].release |= q.release;
            if i > 0 || ctx.pads.is_empty() {
                pads[i].stick_x = if q.press || q.release { q.stick_x } else { pads[i].stick_x };
                pads[i].stick_y = if q.press || q.release { q.stick_y } else { pads[i].stick_y };
            }
            *q = Pad::default();
        }
        // The item windows: roulette, golden mushroom, stack delay.
        for i in 0..racers {
            let slot = &mut self.slots[i];
            slot.cooldown = slot.cooldown.saturating_sub(1);
            if slot.roulette > 0 {
                slot.rolled += 1;
                let skip = pads[i].press && slot.rolled >= ROULETTE_NO_SKIP_FRAMES;
                slot.roulette -= 1;
                if slot.roulette == 0 || skip {
                    slot.roulette = 0;
                    if let Some(item) = slot.pending.take() {
                        self.give(i, item);
                    }
                    // The press that skipped the roulette does not also use the item.
                    if skip {
                        pads[i].press = false;
                    }
                }
            }
            let slot = &mut self.slots[i];
            if slot.golden > 0 {
                slot.golden -= 1;
                if slot.golden == 0 {
                    slot.held = None;
                }
            }
        }
        // `check_player_use_item`: Z pressed with an item that can be used.
        for i in 0..racers {
            if pads[i].press && ctx.bodies[i].exists {
                self.use_item(i, ctx);
            }
        }
        // Actors.
        let ids: Vec<u32> = self.actors.iter().map(|a| a.id).collect();
        for id in ids {
            self.update_actor(id, ctx, &pads);
        }
        self.collide_actors();
        let hits = self.collide_karts(ctx);
        self.cleanup();
        hits
    }

    fn cleanup(&mut self) {
        let drag: Vec<(usize, bool)> = (0..self.slots.len()).map(|i| (i, self.actors.iter().any(|a| a.owner == i && ((a.kind == Kind::Banana && matches!(a.state, banana::HELD | banana::FIRST_BUNCH | banana::BUNCH)) || (a.kind == Kind::Fake && a.state == fake::HELD) || a.kind == Kind::BananaBunch)))).collect();
        let before = self.actors.len();
        self.actors.retain(|a| !(a.dying() && (a.timer < 0 || a.kind.is_parent())));
        let _ = before;
        let ids: Vec<u32> = self.actors.iter().map(|a| a.id).collect();
        for s in self.slots.iter_mut() {
            if s.held_actor.is_some_and(|id| !ids.contains(&id)) {
                s.held_actor = None;
            }
            if s.parent_actor.is_some_and(|id| !ids.contains(&id)) {
                s.parent_actor = None;
            }
        }
        for (i, d) in drag {
            self.slots[i].drag = d;
        }
    }

    // -- per actor ----------------------------------------------------------------------------------------

    fn rope(pos: [f64; 3], anchor: [f64; 3], dist: f64, dy: f64) -> [f64; 3] {
        let d = sub(anchor, pos);
        let l = len(d) / dist;
        if l == 0. {
            return [anchor[0] + 0.2, anchor[1] + 0.2, anchor[2] + 0.2];
        }
        [anchor[0] - d[0] / l, anchor[1] - d[1] / l - dy, anchor[2] - d[2] / l]
    }
    fn floor_clamp(a: &mut Actor, ground: &dyn Ground, r: f64) {
        if let Some(f) = ground.floor(a.pos[0], a.pos[1] + 20., a.pos[2]) {
            if a.pos[1] < f.height + r {
                a.pos[1] = f.height + r;
                if a.vel[1] < 0. {
                    a.vel[1] = 0.;
                }
            }
        }
    }
    fn wall_hit(a: &Actor, ground: &dyn Ground) -> Option<[f64; 3]> {
        let w = ground.wall([a.pos[0], a.pos[1] - KART_RADIUS, a.pos[2]])?;
        let h = w.hits.first()?;
        Some(h.normal)
    }

    fn update_actor(&mut self, id: u32, ctx: &Ctx, pads: &[Pad]) {
        let Some(idx) = self.actors.iter().position(|a| a.id == id) else { return };
        let mut a = self.actors[idx].clone();
        a.age += 1;
        let out_of_bounds = a.pos[1] < ctx.min_y - 400.;
        match a.kind {
            Kind::Banana => self.update_banana(&mut a, ctx, pads),
            Kind::Green => self.update_green(&mut a, ctx, pads),
            Kind::Red | Kind::Blue => self.update_red_blue(&mut a, ctx, pads),
            Kind::Fake => self.update_fake(&mut a, ctx, pads),
            Kind::BananaBunch => self.update_bunch(&mut a, ctx, pads),
            Kind::TripleGreen | Kind::TripleRed => self.update_triple(&mut a, ctx, pads),
        }
        if a.visible() && out_of_bounds {
            self.actors[idx] = a;
            self.destroy(id, ctx);
            return;
        }
        if let Some(i) = self.actors.iter().position(|x| x.id == id) {
            self.actors[i] = a;
        }
    }

    /// `update_actor_banana`
    fn update_banana(&mut self, a: &mut Actor, ctx: &Ctx, pads: &[Pad]) {
        let b = ctx.bodies[a.owner];
        match a.state {
            banana::HELD => {
                a.pos = Self::rope(a.pos, b.pos, 10., 2.);
                Self::floor_clamp(a, ctx.ground, a.radius + 1.);
                if pads[a.owner].release {
                    a.state = banana::DROPPED;
                    a.timer = BANANA_PROTECT;
                    let (sx, sy) = (pads[a.owner].stick_x, pads[a.owner].stick_y);
                    if sy > 30 && (-9..10).contains(&sx) {
                        let up = (sy as f64 - 30.) / 20. + 0.5;
                        let z = if b.speed < 2. { 4. } else { b.speed * 0.75 + 3.5 + up };
                        a.vel = rot_y([0., up, z], b.aim());
                    } else {
                        a.vel = [0., 1.5, 0.];
                    }
                    self.slots[a.owner].held_actor = None;
                    self.slots[a.owner].drag = false;
                    self.events.push(ItemEvent::Release(a.owner, Item::Banana));
                }
            }
            banana::DROPPED => {
                if a.timer > 0 {
                    a.timer -= 1;
                    if a.timer == 0 {
                        a.protect = false;
                    }
                }
                a.pos[0] += a.vel[0];
                a.pos[2] += a.vel[2];
                if a.vel[1] > -1. {
                    a.vel[1] -= 0.15;
                }
                a.pos[1] += a.vel[1];
                if let Some(f) = ctx.ground.floor(a.pos[0], a.pos[1] + 20., a.pos[2]) {
                    let rest = f.height + a.radius + 1.;
                    if a.pos[1] < rest {
                        a.pos[1] = rest;
                        a.protect = false;
                        a.state = banana::ON_GROUND;
                    }
                }
            }
            banana::FIRST_BUNCH => {
                let tail = rot_y([0., 0., -5.], b.aim());
                let anchor = [b.pos[0] + tail[0], b.pos[1] + tail[1], b.pos[2] + tail[2]];
                let d = sub(anchor, a.pos);
                let l = len(d);
                if l == 0. {
                    a.pos = [b.pos[0] + 0.2, b.pos[1] + 0.2, b.pos[2] + 0.2];
                } else {
                    a.pos = [tail[0] + (anchor[0] - d[0] / l), anchor[1] - d[1] / l - 2., anchor[2] - d[2] / l];
                }
                Self::floor_clamp(a, ctx.ground, a.radius + 1.);
            }
            banana::BUNCH => {
                let elder = a.elder.and_then(|e| self.get(e)).map(|e| e.pos).unwrap_or(b.pos);
                a.pos = Self::rope(a.pos, elder, 5., 2.);
                Self::floor_clamp(a, ctx.ground, a.radius + 1.);
            }
            banana::DESTROYED => {
                a.vel[1] = (a.vel[1] - 0.3).max(-5.);
                a.pos[1] += a.vel[1];
                a.spin += 0.3;
                a.timer -= 1;
            }
            banana::ON_GROUND => {
                a.collidable = true;
                a.protect = false;
            }
            _ => {}
        }
    }

    /// `update_actor_banana_bunch`: one banana per 4 frames, then Z drops (stick neutral / down) or throws (stick up) the last.
    fn update_bunch(&mut self, a: &mut Actor, ctx: &Ctx, pads: &[Pad]) {
        let b = ctx.bodies[a.owner];
        match a.spawn {
            0..=4 => {
                if a.spawn == 0 || {
                    a.timer -= 1;
                    a.timer == 0
                } {
                    let n = a.spawn as usize;
                    let p = self.behind(&b, -(b.radius + 4.));
                    let id = self.spawn(Kind::Banana, a.owner, p);
                    let child = self.get_mut(id).unwrap();
                    child.state = if n == 0 { banana::FIRST_BUNCH } else { banana::BUNCH };
                    child.collidable = false;
                    child.parent = Some(a.id);
                    child.slot = n;
                    child.timer = 0x14;
                    child.vel = b.vel;
                    if n > 0 {
                        child.elder = a.children[n - 1];
                    }
                    a.children[n] = Some(id);
                    if n > 0 {
                        if let Some(e) = a.children[n - 1].and_then(|e| self.get_mut(e)) {
                            e.younger = Some(id);
                        }
                    }
                    a.available += 1;
                    a.timer = 4;
                    a.spawn += 1;
                    self.events.push(ItemEvent::Orbit(a.owner));
                }
            }
            5 => {
                a.spawn = 6;
                for c in a.children.iter().flatten().copied().collect::<Vec<_>>() {
                    if let Some(x) = self.get_mut(c) {
                        x.collidable = true;
                    }
                }
            }
            _ => {
                let live: Vec<u32> = a.children.iter().flatten().copied().filter(|c| self.get(*c).is_some_and(|x| matches!(x.state, banana::FIRST_BUNCH | banana::BUNCH))).collect();
                if live.is_empty() {
                    a.kind = Kind::BananaBunch;
                    a.state = 255;
                    self.slots[a.owner].parent_actor = None;
                    self.slots[a.owner].drag = false;
                } else if pads[a.owner].press || pads[a.owner].release {
                    // `controller->buttonPressed & Z_TRIG` while the bunch trails: drop or throw the newest.
                    let _ = pads;
                }
                // The use press itself was consumed by `use_item`; each *further* press releases one banana.
                if self.bunch_press(a.owner, pads) {
                    let last = (0..5).rev().find(|n| a.children[*n].is_some_and(|c| self.get(c).is_some_and(|x| matches!(x.state, banana::FIRST_BUNCH | banana::BUNCH))));
                    if let Some(n) = last {
                        let cid = a.children[n].take().unwrap();
                        let (sx, sy) = (pads[a.owner].stick_x, pads[a.owner].stick_y);
                        let throw = sy >= 0x1F && (-0x27..0x28).contains(&sx);
                        let speed = b.speed;
                        let aim = b.aim();
                        let younger_clear = if n > 0 { a.children[n - 1] } else { None };
                        if let Some(x) = self.get_mut(cid) {
                            x.state = banana::DROPPED;
                            if throw {
                                x.timer = 0x1E;
                                let up = (sy as f64 - 30.) / 20. + 1.5;
                                let z = if speed < 2. { 4. } else { speed * 0.75 + 4.5 + up };
                                x.vel = rot_y([0., up, z], aim);
                            } else {
                                x.timer = BANANA_PROTECT;
                                x.vel = [0., 1.5, 0.];
                            }
                            x.elder = None;
                            x.parent = None;
                        }
                        if let Some(e) = younger_clear.and_then(|e| self.get_mut(e)) {
                            e.younger = None;
                        }
                        a.available -= 1;
                        self.events.push(ItemEvent::Release(a.owner, Item::BananaBunch));
                    }
                }
            }
        }
    }
    /// Further Z presses reach a trailing bunch / triple shells as `press`, which `use_item` ignores (the window is empty).
    fn bunch_press(&self, owner: usize, pads: &[Pad]) -> bool {
        pads[owner].press && self.slots[owner].held.is_none() && self.slots[owner].roulette == 0
    }

    /// `update_actor_triple_shell` + `init_triple_shell`
    fn update_triple(&mut self, a: &mut Actor, ctx: &Ctx, pads: &[Pad]) {
        let b = ctx.bodies[a.owner];
        let green = a.kind == Kind::TripleGreen;
        a.rot_angle = s16(a.rot_angle as i64 + a.rot_vel as i64);
        let ang = (a.rot_angle & 0xFFFF) as i32;
        let mut spawn = |me: &mut Self, a: &mut Actor, slot: usize| {
            let p = me.behind(&b, b.radius - 4.);
            let id = me.spawn(if green { Kind::Green } else { Kind::Red }, a.owner, p);
            let s = me.get_mut(id).unwrap();
            s.state = if green { shell::TRIPLE_GREEN } else { shell::TRIPLE_RED };
            s.collidable = false;
            s.rot_vel = 0;
            s.rot_angle = s16(-180 * DEG as i64);
            s.parent = Some(a.id);
            s.slot = slot;
            a.children[slot] = Some(id);
            a.available += 1;
            me.events.push(ItemEvent::Orbit(a.owner));
        };
        match a.spawn {
            0 => {
                spawn(self, a, 0);
                a.spawn = 1;
            }
            1 => {
                if ang > 300 * DEG {
                    spawn(self, a, 1);
                    a.spawn = 2;
                }
            }
            2 => {
                if ang > 60 * DEG && ang < 70 * DEG {
                    spawn(self, a, 2);
                    a.spawn = 3;
                }
            }
            3 => {
                a.spawn = 4;
                for c in a.children.iter().flatten().copied().collect::<Vec<_>>() {
                    if let Some(x) = self.get_mut(c) {
                        x.collidable = true;
                    }
                }
            }
            _ => {
                let count = a.children.iter().flatten().filter(|c| self.get(**c).is_some_and(|s| matches!(s.state, shell::TRIPLE_GREEN | shell::TRIPLE_RED))).count();
                if count == 0 {
                    a.kind = Kind::TripleGreen;
                    a.state = 255;
                    self.slots[a.owner].parent_actor = None;
                    return;
                }
                if pads[a.owner].press && self.slots[a.owner].held.is_none() && self.slots[a.owner].roulette == 0 {
                    a.fire += 1;
                }
                if a.fire > 0 {
                    if let Some(n) = (0..3).find(|n| a.children[*n].is_some_and(|c| self.get(c).is_some_and(|s| matches!(s.state, shell::TRIPLE_GREEN | shell::TRIPLE_RED)))) {
                        let cid = a.children[n].take().unwrap();
                        let v = rot_y([0., 0., 8.], b.aim());
                        let item = if green { Item::GreenShell } else { Item::RedShell };
                        if let Some(s) = self.get_mut(cid) {
                            s.vel = v;
                            s.state = shell::MOVING;
                            s.timer = SHELL_PROTECT;
                            s.parent = None;
                            s.protect = true;
                        }
                        a.available -= 1;
                        a.fire -= 1;
                        self.events.push(ItemEvent::Release(a.owner, item));
                    }
                }
            }
        }
    }

    /// Hold position of a shell behind its owner (`HELD_SHELL`).
    fn held_shell_pos(a: &mut Actor, b: &Body, ctx: &Ctx) {
        let o = rot_y([0., 0., -(b.radius + a.radius + 2.)], b.rotation);
        a.pos[0] = b.pos[0] + o[0];
        a.pos[2] = b.pos[2] + o[2];
        let ypos = b.pos[1] - b.radius;
        let floor = ctx.ground.floor(a.pos[0], ypos + 20., a.pos[2]).map_or(ypos, |f| f.height);
        a.pos[1] = if (ypos - floor).abs() < 5. { floor + a.radius } else { ypos };
    }
    /// The swing of a released shell round the kart to the front (`RELEASED_SHELL`).
    fn swing_pos(a: &mut Actor, b: &Body, radius: f64) {
        let o = rot_y([(a.rot_angle as f64 * ANGLE).sin() * radius, a.radius - b.radius, (a.rot_angle as f64 * ANGLE).cos() * radius], b.rotation);
        a.pos = [b.pos[0] + o[0], b.pos[1] + o[1], b.pos[2] + o[2]];
    }
    fn shell_launch_speed(b: &Body) -> f64 {
        if b.speed > 8. { b.speed * 1.2 } else { 8. }
    }
    fn orbit_pos(a: &mut Actor, parent_angle_step: i32, b: &Body, ground: &dyn Ground) {
        a.rot_angle = s16(a.rot_angle as i64 + parent_angle_step as i64);
        let (s, c) = ((a.rot_angle as f64 * ANGLE).sin(), (a.rot_angle as f64 * ANGLE).cos());
        let o = rot_y([s * 8., a.radius - b.radius, c * 8.], b.rotation);
        a.pos = [b.pos[0] + o[0], b.pos[1] + o[1], b.pos[2] + o[2]];
        Self::floor_clamp(a, ground, a.radius);
    }

    /// Shell motion shared by the straight modes: gravity, move, floor and wall response.
    fn shell_move(a: &mut Actor, ctx: &Ctx) -> Option<[f64; 3]> {
        a.vel[1] = (a.vel[1] - 0.5).max(-2.);
        for i in 0..3 {
            a.pos[i] += a.vel[i];
        }
        Self::floor_clamp(a, ctx.ground, a.radius);
        Self::wall_hit(a, ctx.ground)
    }

    /// `update_actor_green_shell`
    fn update_green(&mut self, a: &mut Actor, ctx: &Ctx, pads: &[Pad]) {
        let b = ctx.bodies[a.owner];
        a.rot_vel = s16(a.rot_vel as i64 + 10 * DEG as i64);
        match a.state {
            shell::HELD => {
                Self::held_shell_pos(a, &b, ctx);
                let pad = pads[a.owner];
                if pad.release {
                    if pad.stick_y < -0x2D {
                        a.vel = rot_y([0., 0., -Self::shell_launch_speed(&b)], b.aim());
                        a.state = shell::MOVING;
                        a.timer = SHELL_PROTECT;
                        self.shell_left(a);
                    } else {
                        a.state = shell::RELEASED;
                        a.rot_angle = if b.slip > 0 { 170 * DEG } else { -170 * DEG - 1 };
                    }
                }
            }
            shell::RELEASED => {
                if a.rot_angle > 0 {
                    a.rot_angle -= 20 * DEG;
                    if a.rot_angle < 0 {
                        a.state = shell::MOVING;
                        a.timer = SHELL_PROTECT;
                    }
                } else {
                    a.rot_angle += 20 * DEG;
                    if a.rot_angle > 0 {
                        a.state = shell::MOVING;
                        a.timer = SHELL_PROTECT;
                    }
                }
                if a.state == shell::MOVING {
                    a.vel = rot_y([0., 0., Self::shell_launch_speed(&b)], b.aim());
                    self.shell_left(a);
                } else {
                    Self::swing_pos(a, &b, 6.);
                }
            }
            shell::MOVING => {
                if a.timer > 0 {
                    a.timer -= 1;
                    if a.timer == 0 {
                        a.protect = false;
                    }
                }
                if a.age > SHELL_MAX_FRAMES {
                    self.pop(a);
                    return;
                }
                if let Some(n) = Self::shell_move(a, ctx) {
                    // shell_collision -> process_shell_collision: reflect, speed 4 unless it is already 3.5..4.5
                    let dot = n[0] * a.vel[0] + n[1] * a.vel[1] + n[2] * a.vel[2];
                    let mut v = [a.vel[0] - 2. * dot * n[0], a.vel[1] - 2. * dot * n[1], a.vel[2] - 2. * dot * n[2]];
                    let mut m = len(a.vel);
                    if !(3.5..=4.5).contains(&m) {
                        m = 4.;
                    }
                    let l = len(v).max(1e-9);
                    for c in v.iter_mut() {
                        *c = *c / l * m;
                    }
                    a.vel = v;
                    // back out of the wall
                    a.pos[0] += n[0] * 1.5;
                    a.pos[2] += n[2] * 1.5;
                    self.events.push(ItemEvent::Bounce(Item::GreenShell));
                }
            }
            shell::TRIPLE_GREEN => {
                let step = self.get(a.parent.unwrap_or(0)).map_or(8 * DEG, |p| p.rot_vel);
                if a.parent.is_none_or(|p| self.get(p).is_none()) {
                    self.destroy_actor_state(a);
                    return;
                }
                Self::orbit_pos(a, step, &b, ctx.ground);
            }
            shell::HIT_A_RACER => {
                a.vel[1] = (a.vel[1] - 0.3).max(-5.);
                a.rot_angle = s16(a.rot_angle as i64 + 8 * DEG as i64);
                a.timer -= 1;
                a.pos[1] += a.vel[1];
                a.spin += 0.2;
            }
            _ => {}
        }
    }
    fn shell_left(&mut self, a: &Actor) {
        self.slots[a.owner].held_actor = None;
        self.events.push(ItemEvent::Release(a.owner, Item::GreenShell));
    }
    fn pop(&mut self, a: &mut Actor) {
        self.destroy_actor_state(a);
        self.events.push(ItemEvent::Pop(match a.kind {
            Kind::Green => Item::GreenShell,
            Kind::Red => Item::RedShell,
            Kind::Blue => Item::BlueShell,
            Kind::Banana => Item::Banana,
            _ => Item::FakeBox,
        }));
    }

    /// `func_802B3B44`: follow `gCurrentTrackPath` at up to 6 units per frame.
    fn follow_path(a: &mut Actor, path: &[[f64; 3]], ground: &dyn Ground) {
        if path.is_empty() {
            return;
        }
        let n = path.len();
        let cur = a.path_index % n;
        let nxt = (cur + 1) % n;
        let p = path[cur];
        let d = sub(p, a.pos);
        let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if d2 > 400. {
            let pn = path[nxt];
            let dn = sub(pn, a.pos);
            let d2n = dn[0] * dn[0] + dn[1] * dn[1] + dn[2] * dn[2];
            if d2n < d2 {
                a.path_index = nxt;
            } else {
                let l = d2.sqrt() * 4.;
                let mut v = [a.vel[0] + d[0] / l, a.vel[1] + d[1] / l, a.vel[2] + d[2] / l];
                let m = len(v);
                if m > 6. {
                    for c in v.iter_mut() {
                        *c /= m / 6.;
                    }
                }
                a.vel = v;
                for i in 0..3 {
                    a.pos[i] += v[i];
                }
                Self::floor_clamp(a, ground, a.radius);
            }
        } else if d2 > 5. {
            a.pos = [p[0], a.radius + p[1], p[2]];
            a.path_index = nxt;
        } else {
            let pn = path[nxt];
            a.pos = [(p[0] + pn[0]) * 0.5, (p[1] + pn[1]) * 0.5 + a.radius, (p[2] + pn[2]) * 0.5];
            a.vel = [(pn[0] - p[0]) * 0.5, (pn[1] - p[1]) * 0.5, (pn[2] - p[2]) * 0.5];
        }
    }
    /// `func_802B3E7C`: head for the target at 8 units per frame in xz, sinking 2 per frame.
    fn home_in(&mut self, a: &mut Actor, target: &Body, ctx: &Ctx) {
        let (dx, dz) = (target.pos[0] - a.pos[0], target.pos[2] - a.pos[2]);
        let l = (dx * dx + dz * dz).sqrt() / 8.;
        let (vx, vz) = if l == 0. { (0., 0.) } else { (dx / l, dz / l) };
        a.pos[0] += vx;
        a.pos[1] -= 2.;
        a.pos[2] += vz;
        a.vel = [vx, -2., vz];
        if target.flags & fx::BOO_EFFECT != 0 {
            self.pop(a);
        } else {
            Self::floor_clamp(a, ctx.ground, a.radius);
            if Self::wall_hit(a, ctx.ground).is_some() {
                self.pop(a);
            }
        }
    }

    /// `update_actor_red_blue_shell`
    fn update_red_blue(&mut self, a: &mut Actor, ctx: &Ctx, pads: &[Pad]) {
        let b = ctx.bodies[a.owner];
        let item = if a.kind == Kind::Red { Item::RedShell } else { Item::BlueShell };
        a.rot_vel = s16(a.rot_vel as i64 + 10 * DEG as i64);
        match a.state {
            shell::HELD => {
                Self::held_shell_pos(a, &b, ctx);
                if pads[a.owner].release {
                    a.state = shell::RELEASED;
                    a.rot_angle = if b.slip > 0 { 170 * DEG } else { -170 * DEG - 1 };
                }
            }
            shell::RELEASED => {
                if a.rot_angle > 0 {
                    a.rot_angle -= 10 * DEG;
                    if a.rot_angle < 0 {
                        a.state = shell::MOVING;
                    }
                } else {
                    a.rot_angle += 10 * DEG;
                    if a.rot_angle > 0 {
                        a.state = shell::MOVING;
                    }
                }
                if a.state == shell::MOVING {
                    a.timer = SHELL_PROTECT;
                    a.vel = rot_y([0., 0., Self::shell_launch_speed(&b)], b.aim());
                    self.slots[a.owner].held_actor = None;
                    self.events.push(ItemEvent::Release(a.owner, item));
                } else {
                    Self::swing_pos(a, &b, 8.);
                }
            }
            shell::MOVING => {
                a.timer -= 1;
                if a.timer == 0 {
                    a.protect = false;
                    let node = b.node;
                    let start = (node + 8) % ctx.path.len().max(1);
                    if a.kind == Kind::Blue {
                        a.target = ctx.bodies.iter().position(|x| x.exists && x.rank == 0).unwrap_or(0);
                        a.state = shell::BLUE_LOCK_ON;
                        a.path_index = start;
                        self.events.push(ItemEvent::LockOn(item));
                    } else if b.rank == 0 {
                        a.state = shell::TRIPLE_GREEN;
                        a.timer = 0x258;
                        a.path_index = start;
                    } else if b.rank >= 5 {
                        a.state = shell::HIT_A_RACER;
                        a.path_index = start;
                        a.target = ctx.bodies.iter().position(|x| x.exists && x.rank == b.rank - 1).unwrap_or(0);
                    } else {
                        a.state = shell::RED_LOCK_ON;
                        a.target = ctx.bodies.iter().position(|x| x.exists && x.rank == b.rank - 1).unwrap_or(0);
                        self.events.push(ItemEvent::LockOn(item));
                    }
                }
                if let Some(_n) = Self::shell_move(a, ctx) {
                    // func_802B4104: a red or blue shell pops on a wall
                    self.pop(a);
                }
            }
            shell::RED_LOCK_ON | shell::BLUE_ELIMINATED => {
                let t = ctx.bodies[a.target.min(ctx.bodies.len() - 1)];
                self.home_in(a, &t, ctx);
            }
            shell::TRIPLE_GREEN => {
                Self::follow_path(a, ctx.path, ctx.ground);
                a.timer -= 1;
                if a.timer <= 0 {
                    self.pop(a);
                }
            }
            shell::HIT_A_RACER => {
                Self::follow_path(a, ctx.path, ctx.ground);
                let t = ctx.bodies[a.target.min(ctx.bodies.len() - 1)];
                let (dx, dz) = (t.pos[0] - a.pos[0], t.pos[2] - a.pos[2]);
                if dx * dx + dz * dz < 40000. {
                    a.state = shell::RED_LOCK_ON;
                    self.events.push(ItemEvent::LockOn(item));
                }
            }
            shell::TRIPLE_RED => {
                let step = self.get(a.parent.unwrap_or(0)).map_or(8 * DEG, |p| p.rot_vel);
                if a.parent.is_none_or(|p| self.get(p).is_none()) {
                    self.destroy_actor_state(a);
                    return;
                }
                Self::orbit_pos(a, step, &b, ctx.ground);
            }
            shell::DESTROYED => {
                a.vel[1] = (a.vel[1] - 0.3).max(-5.);
                a.rot_angle = s16(a.rot_angle as i64 + 8 * DEG as i64);
                a.timer -= 1;
                a.pos[1] += a.vel[1];
                a.spin += 0.2;
            }
            shell::BLUE_LOCK_ON => {
                Self::follow_path(a, ctx.path, ctx.ground);
                a.target = ctx.bodies.iter().position(|x| x.exists && x.rank == 0).unwrap_or(0);
                let t = ctx.bodies[a.target];
                let (dx, dz) = (t.pos[0] - a.pos[0], t.pos[2] - a.pos[2]);
                if dx * dx + dz * dz < 40000. {
                    a.state = shell::BLUE_ELIMINATED;
                }
            }
            _ => {}
        }
    }

    /// `update_actor_fake_item_box`
    fn update_fake(&mut self, a: &mut Actor, ctx: &Ctx, pads: &[Pad]) {
        let b = ctx.bodies[a.owner];
        match a.state {
            fake::HELD => {
                a.radius = a.size * FAKE_BOX_RADIUS;
                a.rot_angle = s16(a.rot_angle as i64 - DEG as i64);
                a.rot_vel = s16(a.rot_vel as i64 + 2 * DEG as i64);
                a.pos = Self::rope(a.pos, b.pos, 10., 1.);
                Self::floor_clamp(a, ctx.ground, a.radius);
                if pads[a.owner].release {
                    // func_802A1064
                    a.state = fake::ON_GROUND;
                    a.timer = FAKE_BOX_PROTECT;
                    self.slots[a.owner].held_actor = None;
                    self.slots[a.owner].drag = false;
                    self.events.push(ItemEvent::Release(a.owner, Item::FakeBox));
                }
            }
            fake::ON_GROUND => {
                a.size = (a.size + 0.05).min(1.);
                a.radius = a.size * FAKE_BOX_RADIUS;
                let target = ctx.ground.floor(a.pos[0], a.pos[1] + 30., a.pos[2]).map_or(a.pos[1], |f| f.height + 8.66);
                if target <= a.pos[1] {
                    a.pos[1] = target;
                } else {
                    a.pos[1] += 0.2;
                }
                if a.protect {
                    if a.timer <= 0 || a.timer >= 0x12D {
                        a.protect = false;
                        a.timer = 0;
                    } else {
                        a.timer -= 1;
                    }
                }
                a.rot_angle = s16(a.rot_angle as i64 - DEG as i64);
                a.rot_vel = s16(a.rot_vel as i64 + 2 * DEG as i64);
            }
            _ => {
                // DESTROYED_FAKE_ITEM_BOX: 0x14 frames of spinning apart, then `destroy_actor`.
                if a.timer >= 0x14 || a.timer < 0 {
                    a.timer = -1;
                } else {
                    a.timer += 1;
                    a.spin += 0.1;
                }
            }
        }
    }

    fn destroy_actor_state(&mut self, a: &mut Actor) {
        a.state = match a.kind {
            Kind::Banana => banana::DESTROYED,
            Kind::Fake => fake::DESTROYED,
            Kind::Green => shell::HIT_A_RACER,
            _ => shell::DESTROYED,
        };
        a.collidable = false;
        a.timer = 60;
        a.vel[1] = 3.;
        a.rot_angle = 0;
        if a.kind == Kind::Fake {
            a.timer = 0;
        }
    }

    /// `destroy_destructable_actor`
    pub fn destroy(&mut self, id: u32, ctx: &Ctx) {
        let Some(a) = self.get(id).cloned() else { return };
        let mut a = a;
        match a.kind {
            Kind::Banana => match a.state {
                banana::FIRST_BUNCH | banana::BUNCH => {
                    // destroy_banana_in_banana_bunch: the rest of the bunch scatters.
                    let siblings: Vec<u32> = a.parent.and_then(|p| self.get(p)).map(|p| p.children.iter().flatten().copied().collect()).unwrap_or_default();
                    for s in siblings {
                        if s != a.id {
                            let (vx, vy, vz) = ((self.rng.below(200) as f64 - 100.) * 0.015, self.rng.below(200) as f64 * 0.015, (self.rng.below(200) as f64 - 100.) * 0.015);
                            if let Some(x) = self.get_mut(s) {
                                if matches!(x.state, banana::FIRST_BUNCH | banana::BUNCH) {
                                    x.state = banana::DROPPED;
                                    x.timer = BANANA_PROTECT;
                                    x.vel = [vx, vy, vz];
                                    x.elder = None;
                                    x.younger = None;
                                    x.parent = None;
                                }
                            }
                        }
                    }
                    if let Some(p) = a.parent.and_then(|p| self.get_mut(p)) {
                        p.children = [None; 5];
                    }
                    self.destroy_actor_state(&mut a);
                    a.timer = 60;
                }
                banana::HELD => {
                    self.slots[a.owner].drag = false;
                    self.slots[a.owner].held_actor = None;
                    self.destroy_actor_state(&mut a);
                }
                banana::ON_GROUND => self.destroy_actor_state(&mut a),
                _ => return,
            },
            Kind::Green => match a.state {
                shell::HIT_A_RACER => return,
                shell::TRIPLE_GREEN => self.triple_collide(&mut a, false),
                _ => {
                    self.slots[a.owner].held_actor = None;
                    self.destroy_actor_state(&mut a);
                }
            },
            Kind::Red | Kind::Blue => {
                if a.state == shell::DESTROYED {
                    return;
                }
                if a.state == shell::TRIPLE_RED {
                    self.triple_collide(&mut a, true);
                } else {
                    self.slots[a.owner].held_actor = None;
                    self.destroy_actor_state(&mut a);
                }
            }
            Kind::Fake => {
                if a.state == fake::HELD {
                    self.slots[a.owner].drag = false;
                    self.slots[a.owner].held_actor = None;
                }
                self.destroy_actor_state(&mut a);
            }
            _ => {}
        }
        if let Some(i) = self.actors.iter().position(|x| x.id == id) {
            self.actors[i] = a;
        }
    }
    /// `triple_shell_actor_collide_with_player`
    fn triple_collide(&mut self, a: &mut Actor, red: bool) {
        if let Some(p) = a.parent.and_then(|p| self.get_mut(p)) {
            p.available -= 1;
            for c in p.children.iter_mut() {
                if *c == Some(a.id) {
                    *c = None;
                }
            }
        }
        a.collidable = false;
        a.rot_angle = 0;
        a.vel[1] = 3.;
        a.timer = 60;
        a.state = if red { shell::DESTROYED } else { shell::HIT_A_RACER };
        a.parent = None;
    }

    // -- collisions ---------------------------------------------------------------------------------------

    /// `query_collision_actor_vs_actor`
    fn overlap(a: &Actor, b: &Actor) -> bool {
        let r = a.radius + b.radius;
        let d = sub(a.pos, b.pos);
        if d.iter().any(|c| c.abs() > r) {
            return false;
        }
        let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        d2 >= 0.1 && d2 <= r * r
    }

    /// `evaluate_collision_for_destructible_actors`
    fn collide_actors(&mut self) {
        let n = self.actors.len();
        let mut kill: Vec<u32> = vec![];
        for i in 0..n {
            let a = &self.actors[i];
            if !a.collidable || !a.visible() {
                continue;
            }
            for j in i + 1..n {
                let b = &self.actors[j];
                if !b.collidable || !b.visible() {
                    continue;
                }
                if a.kind == Kind::Banana && b.kind == Kind::Banana {
                    continue;
                }
                if a.kind == b.kind && a.owner == b.owner && matches!(a.kind, Kind::Green | Kind::Red) {
                    continue;
                }
                if !Self::overlap(a, b) {
                    continue;
                }
                if a.kind == Kind::Blue && b.kind == Kind::Blue {
                    kill.push(a.id);
                    kill.push(b.id);
                    continue;
                }
                // A spiny shell survives unless it is still held; everything else pops.
                if a.kind != Kind::Blue || a.state == shell::HELD {
                    kill.push(a.id);
                }
                if b.kind != Kind::Blue || b.state == shell::HELD {
                    kill.push(b.id);
                }
            }
        }
        // `destroy_destructable_actor` needs a Ctx only for ownership bookkeeping; do it without one.
        for id in kill {
            let item = self.get(id).map(|a| match a.kind {
                Kind::Green => Item::GreenShell,
                Kind::Red => Item::RedShell,
                Kind::Blue => Item::BlueShell,
                Kind::Banana => Item::Banana,
                _ => Item::FakeBox,
            });
            self.destroy_bare(id);
            if let Some(i) = item {
                self.events.push(ItemEvent::Pop(i));
            }
        }
    }
    fn destroy_bare(&mut self, id: u32) {
        struct NoGround;
        impl Ground for NoGround {
            fn floor(&self, _: f64, _: f64, _: f64) -> Option<super::kart_physics::Floor> {
                None
            }
            fn wall(&self, _: [f64; 3]) -> Option<super::kart_physics::Wall> {
                None
            }
        }
        let ctx = Ctx { bodies: &[], pads: &[], ground: &NoGround, path: &[], min_y: -1e9 };
        self.destroy(id, &ctx);
    }

    /// `query_collision_player_vs_actor_item`: the 3D box test, then the sphere.
    fn reaches(b: &Body, a: &Actor) -> bool {
        let r = b.radius + a.radius;
        let d = sub(a.pos, b.pos);
        if d.iter().any(|c| c.abs() > r) {
            return false;
        }
        let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        d2 >= 0.1 && d2 <= r * r
    }

    /// `evaluate_collision_between_player_actor` for the item actors.
    fn collide_karts(&mut self, ctx: &Ctx) -> Vec<Hit> {
        let mut hits: Vec<Hit> = vec![];
        let mut pending: Vec<u32> = vec![];
        let mut triggers: Vec<u32> = ctx.bodies.iter().map(|b| b.triggers).collect();
        for (racer, b) in ctx.bodies.iter().enumerate() {
            if !b.exists || b.flags & fx::SQUISH_EFFECT != 0 {
                continue;
            }
            for a in self.actors.iter().filter(|a| a.visible() && a.collidable) {
                let own = a.owner == racer && a.protect;
                match a.kind {
                    Kind::Banana => {
                        if b.flags & (fx::BOO_EFFECT | fx::BANANA_NEAR_SPINOUT_EFFECT | fx::BANANA_SPINOUT_EFFECT | fx::DRIVING_SPINOUT_EFFECT) != 0 || triggers[racer] & fx::HIT_BANANA_TRIGGER != 0 {
                            continue;
                        }
                        if own || !Self::reaches(b, a) {
                            continue;
                        }
                        triggers[racer] |= fx::HIT_BANANA_TRIGGER;
                        hits.push(Hit { racer, trigger: fx::HIT_BANANA_TRIGGER, item: if matches!(a.state, banana::FIRST_BUNCH | banana::BUNCH) { Item::BananaBunch } else { Item::Banana }, owner: a.owner });
                        pending.push(a.id);
                    }
                    Kind::Green => {
                        if b.flags & (fx::BOO_EFFECT | fx::HIT_BY_GREEN_SHELL_EFFECT) != 0 || triggers[racer] & fx::LOW_TUMBLE_TRIGGER != 0 {
                            continue;
                        }
                        if own || !Self::reaches(b, a) || a.state == shell::HIT_A_RACER {
                            continue;
                        }
                        triggers[racer] |= fx::LOW_TUMBLE_TRIGGER;
                        hits.push(Hit { racer, trigger: fx::LOW_TUMBLE_TRIGGER, item: Item::GreenShell, owner: a.owner });
                        pending.push(a.id);
                    }
                    Kind::Red | Kind::Blue => {
                        if triggers[racer] & fx::HIGH_TUMBLE_TRIGGER != 0 || (a.kind == Kind::Red && b.flags & fx::EXPLOSION_CRASH_EFFECT != 0) {
                            continue;
                        }
                        if own || !Self::reaches(b, a) || a.state == shell::DESTROYED {
                            continue;
                        }
                        let item = if a.kind == Kind::Red { Item::RedShell } else { Item::BlueShell };
                        if b.flags & fx::BOO_EFFECT == 0 {
                            triggers[racer] |= fx::HIGH_TUMBLE_TRIGGER;
                            hits.push(Hit { racer, trigger: fx::HIGH_TUMBLE_TRIGGER, item, owner: a.owner });
                        }
                        // A spiny shell only dies on its target; a red shell on whoever it touches.
                        if a.kind == Kind::Red || racer == a.target {
                            pending.push(a.id);
                        }
                    }
                    Kind::Fake => {
                        if b.flags & fx::BOO_EFFECT != 0 || own || !Self::reaches(b, a) {
                            continue;
                        }
                        triggers[racer] |= fx::VERTICAL_TUMBLE_TRIGGER;
                        hits.push(Hit { racer, trigger: fx::VERTICAL_TUMBLE_TRIGGER, item: Item::FakeBox, owner: a.owner });
                        pending.push(a.id);
                    }
                    _ => {}
                }
            }
        }
        for id in pending {
            self.destroy_bare(id);
        }
        for h in &hits {
            self.events.push(ItemEvent::Hit(*h));
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::super::kart_physics::{Floor, Wall, WallHit};
    use super::*;

    struct Flat {
        wall_x: Option<f64>,
    }
    impl Ground for Flat {
        fn floor(&self, _: f64, _: f64, _: f64) -> Option<Floor> {
            Some(Floor::flat(0., false))
        }
        fn wall(&self, p: [f64; 3]) -> Option<Wall> {
            self.wall_x.filter(|w| p[0] > *w).map(|w| Wall { pos: [w - 2., p[1], p[2]], hits: vec![WallHit { normal: [-1., 0., 0.], x_facing: true }] })
        }
    }
    fn open() -> Flat {
        Flat { wall_x: None }
    }
    /// A straight road along +z, a node every 20 units.
    fn road() -> Vec<[f64; 3]> {
        (0..200).map(|i| [0., 0., i as f64 * 20.]).collect()
    }
    fn body(x: f64, z: f64, rank: usize, node: usize) -> Body {
        Body { pos: [x, KART_RADIUS, z], vel: [0.; 3], rotation: 0, slip: 0, speed: 0., radius: KART_RADIUS, rank, node, exists: true, human: rank == 0, flags: 0, triggers: 0 }
    }
    fn tick(w: &mut ItemWorld, bodies: &[Body], pads: &[Pad], g: &dyn Ground, path: &[[f64; 3]]) -> Vec<Hit> {
        w.step(&Ctx { bodies, pads, ground: g, path, min_y: -1000. })
    }
    fn press() -> Pad {
        Pad { press: true, ..Pad::default() }
    }
    fn release(sy: i32) -> Pad {
        Pad { release: true, stick_y: sy, ..Pad::default() }
    }
    fn world(n: usize) -> ItemWorld {
        ItemWorld::new(n, 12345)
    }
    fn shells(w: &ItemWorld) -> Vec<&Actor> {
        w.actors.iter().filter(|a| a.is_shell()).collect()
    }

    #[test]
    fn item_ids_match_the_decomp_enum() {
        // include/defines.h `enum item`
        assert_eq!(Item::Banana.id(), 1);
        assert_eq!(Item::BananaBunch.id(), 2);
        assert_eq!(Item::GreenShell.id(), 3);
        assert_eq!(Item::TripleGreenShell.id(), 4);
        assert_eq!(Item::RedShell.id(), 5);
        assert_eq!(Item::TripleRedShell.id(), 6);
        assert_eq!(Item::BlueShell.id(), 7);
        assert_eq!(Item::Thunderbolt.id(), 8);
        assert_eq!(Item::FakeBox.id(), 9);
        assert_eq!(Item::Star.id(), 10);
        assert_eq!(Item::Boo.id(), 11);
        assert_eq!(Item::Mushroom.id(), 12);
        assert_eq!(Item::DoubleMushroom.id(), 13);
        assert_eq!(Item::TripleMushroom.id(), 14);
        assert_eq!(Item::SuperMushroom.id(), 15);
        assert_eq!(Item::ALL.len(), 15);
        for i in Item::ALL {
            assert_eq!(Item::from_id(i.id()), Some(i));
        }
    }

    fn counts(row: &[u8]) -> std::collections::BTreeMap<u8, usize> {
        let mut m = std::collections::BTreeMap::new();
        for v in row {
            *m.entry(*v).or_insert(0) += 1;
        }
        m
    }

    #[test]
    fn roulette_curves_are_the_roms_tables() {
        // First place, grand prix human curve: 30 banana, 5 bunch, 30 green, 5 triple green, 5 red, 10 fake, 5 boo, 10 mushroom.
        let c = counts(&curve_row(Curve::GrandPrix, false, 0));
        let want: Vec<(u8, usize)> = vec![(1, 30), (2, 5), (3, 30), (4, 5), (5, 5), (9, 10), (11, 5), (12, 10)];
        assert_eq!(c.into_iter().collect::<Vec<_>>(), want);
        // Last place: 20 triple red, 15 blue, 20 thunder, 30 star, 5 triple mushroom, 10 super.
        let c = counts(&curve_row(Curve::GrandPrix, false, 7));
        assert_eq!(c.into_iter().collect::<Vec<_>>(), vec![(6, 20), (7, 15), (8, 20), (10, 30), (14, 5), (15, 10)]);
        // CPU first place: no mushrooms, banana 60.
        let c = counts(&curve_row(Curve::GrandPrix, true, 0));
        assert_eq!(c.get(&1), Some(&60));
        assert!(c.get(&12).is_none());
        // CPU rank 8 is the only row with a thunderbolt (1 in 100).
        assert_eq!(counts(&curve_row(Curve::GrandPrix, true, 7)).get(&8), Some(&1));
        // Battle: 20 triple green, 20 red, 20 star...
        assert_eq!(counts(&curve_row(Curve::Battle, false, 0)).get(&10), Some(&20));
        for rank in 0..8 {
            assert_eq!(curve_row(Curve::GrandPrix, false, rank).len(), 100);
        }
        // The blue shell only ever goes to the back half.
        for rank in 0..3 {
            assert!(!curve_row(Curve::GrandPrix, false, rank).contains(&7), "rank {rank}");
        }
        assert_eq!(gp_row(0, 4), 0);
        assert_eq!(gp_row(3, 4), 7);
        assert_eq!(gp_row(5, 8), 5);
    }

    #[test]
    fn curves_match_the_exported_json() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64/item-curves.json");
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let rows = |name: &str| -> Vec<String> { v[name].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect() };
        assert_eq!(rows("grandPrixHuman"), curves::HUMAN_GP);
        assert_eq!(rows("grandPrixCpu"), curves::CPU_GP);
        assert_eq!(rows("versus2"), curves::VERSUS_2);
        assert_eq!(rows("versus3"), curves::VERSUS_3);
        assert_eq!(rows("versus4"), curves::VERSUS_4);
        assert_eq!(rows("battle"), curves::BATTLE);
    }

    #[test]
    fn rolled_items_follow_the_curve_weights() {
        let mut rng = ItemRng(99);
        let mut first = std::collections::BTreeMap::new();
        for _ in 0..20000 {
            *first.entry(gen_random_item(Curve::GrandPrix, false, 0, 8, &mut rng)).or_insert(0usize) += 1;
        }
        let share = |i: Item| *first.get(&i).unwrap_or(&0) as f64 / 20000.;
        assert!((share(Item::Banana) - 0.30).abs() < 0.02);
        assert!((share(Item::GreenShell) - 0.30).abs() < 0.02);
        assert_eq!(share(Item::BlueShell), 0.);
        assert_eq!(share(Item::Star), 0.);
        let mut last = std::collections::BTreeMap::new();
        for _ in 0..20000 {
            *last.entry(gen_random_item(Curve::GrandPrix, false, 7, 8, &mut rng)).or_insert(0usize) += 1;
        }
        assert!(*last.get(&Item::Star).unwrap() as f64 / 20000. > 0.26);
        assert_eq!(last.get(&Item::Banana), None);
    }

    #[test]
    fn the_roulette_takes_142_frames_and_z_skips_it_after_50() {
        let mut w = world(1);
        let b = [body(0., 0., 0, 0)];
        assert!(w.pickup(0, 0, 1, true));
        assert!(!w.pickup(0, 0, 1, true), "the window is busy");
        let mut n = 0;
        while w.slots[0].held.is_none() {
            tick(&mut w, &b, &[Pad::default()], &open(), &road());
            n += 1;
            assert!(n < 400);
        }
        assert_eq!(n, ROULETTE_FRAMES as usize);
        assert_eq!(ROULETTE_FRAMES, 142);
        let mut w = world(1);
        w.pickup(0, 0, 1, true);
        for _ in 0..40 {
            tick(&mut w, &b, &[press()], &open(), &road());
        }
        assert!(w.slots[0].held.is_none(), "too early to skip");
        for _ in 0..12 {
            tick(&mut w, &b, &[Pad::default()], &open(), &road());
        }
        tick(&mut w, &b, &[press()], &open(), &road());
        assert!(w.slots[0].held.is_some(), "Z after 50 frames settles the roulette");
    }

    #[test]
    fn mushrooms_stack_and_the_super_mushroom_boosts_for_ten_seconds() {
        let mut w = world(1);
        let b = [body(0., 0., 0, 0)];
        w.give(0, Item::TripleMushroom);
        tick(&mut w, &b, &[press()], &open(), &road());
        assert_eq!(w.drain_triggers(), vec![(0, fx::SHROOM_TRIGGER)]);
        assert_eq!(w.held(0), Some(Item::DoubleMushroom));
        tick(&mut w, &b, &[press()], &open(), &road());
        assert!(w.drain_triggers().is_empty(), "the next mushroom waits the 40-frame delay");
        for _ in 0..40 {
            tick(&mut w, &b, &[Pad::default()], &open(), &road());
        }
        tick(&mut w, &b, &[press()], &open(), &road());
        assert_eq!(w.held(0), Some(Item::Mushroom));
        for _ in 0..=MUSHROOM_STACK_DELAY {
            tick(&mut w, &b, &[Pad::default()], &open(), &road());
        }
        w.give(0, Item::SuperMushroom);
        tick(&mut w, &b, &[press()], &open(), &road());
        assert_eq!(w.held(0), Some(Item::SuperMushroom), "still there");
        assert_eq!(w.slots[0].golden, GOLDEN_FRAMES - 0);
        for _ in 0..30 {
            tick(&mut w, &b, &[press()], &open(), &road());
        }
        assert!(w.drain_triggers().len() > 20, "every press boosts again during the 600 frames");
        for _ in 0..GOLDEN_FRAMES {
            tick(&mut w, &b, &[Pad::default()], &open(), &road());
        }
        assert_eq!(w.held(0), None, "the golden mushroom is spent after 0x258 frames");
    }

    #[test]
    fn a_banana_trails_ten_units_behind_and_is_dropped_on_release() {
        let mut w = world(2);
        let mut bodies = [body(0., 0., 0, 0), body(0., -400., 1, 0)];
        bodies[0].speed = 5.;
        w.give(0, Item::Banana);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        let b = w.actors.iter().find(|a| a.kind == Kind::Banana).unwrap().clone();
        assert_eq!(b.state, banana::HELD);
        assert!(w.slots[0].drag && w.slots[0].held.is_none());
        for _ in 0..20 {
            bodies[0].pos[2] += 5.;
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        let b = w.actors.iter().find(|a| a.kind == Kind::Banana).unwrap();
        let d = bodies[0].pos[2] - b.pos[2];
        assert!((d - 10.).abs() < 1.5, "the rope holds it 10 behind: {d}");
        // Release with the stick neutral: it drops straight down at the kart's position.
        tick(&mut w, &bodies, &[release(0), Pad::default()], &open(), &road());
        let b = w.actors.iter().find(|a| a.kind == Kind::Banana).unwrap();
        assert_eq!(b.state, banana::DROPPED);
        assert_eq!(b.timer, BANANA_PROTECT - 0);
        assert!(!w.slots[0].drag);
        for _ in 0..30 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        assert_eq!(w.actors.iter().find(|a| a.kind == Kind::Banana).unwrap().state, banana::ON_GROUND);
    }

    #[test]
    fn a_banana_thrown_with_the_stick_up_flies_forward() {
        let mut w = world(1);
        let mut bodies = [body(0., 0., 0, 0)];
        bodies[0].speed = 6.;
        w.give(0, Item::Banana);
        tick(&mut w, &bodies, &[press()], &open(), &road());
        tick(&mut w, &bodies, &[Pad { release: true, stick_y: 80, ..Pad::default() }], &open(), &road());
        let b = w.actors[0].clone();
        // (0, up, speed*0.75 + 3.5 + up) rotated by the kart heading: forward is +z at rotation 0
        let up = (80. - 30.) / 20. + 0.5;
        assert!((b.vel[2] - (6. * 0.75 + 3.5 + up)).abs() < 0.01, "{:?}", b.vel);
        assert!(b.vel[1] > 0.);
    }

    #[test]
    fn a_kart_running_into_a_banana_gets_the_banana_trigger_and_the_owner_is_safe_for_180_frames() {
        let mut w = world(2);
        let bodies = [body(0., 0., 0, 0), body(0., -200., 1, 0)];
        w.give(0, Item::Banana);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        tick(&mut w, &bodies, &[release(0), Pad::default()], &open(), &road());
        // The owner stands on it: no hit while protected (it lands after about 20 frames).
        for _ in 0..30 {
            let hits = tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
            assert!(hits.is_empty());
        }
        // Another kart rolls over it.
        let at = w.actors[0].pos;
        let other = [bodies[0], body(at[0], at[2] - 3., 1, 0)];
        let hits = tick(&mut w, &other, &[Pad::default(); 2], &open(), &road());
        assert_eq!(hits, vec![Hit { racer: 1, trigger: fx::HIT_BANANA_TRIGGER, item: Item::Banana, owner: 0 }]);
        // The banana is gone a second later.
        for _ in 0..70 {
            tick(&mut w, &other, &[Pad::default(); 2], &open(), &road());
        }
        assert!(w.actors.iter().all(|a| a.kind != Kind::Banana));
    }

    #[test]
    fn the_banana_bunch_spawns_five_bananas_four_frames_apart_and_hitting_one_scatters_the_rest() {
        let mut w = world(2);
        let bodies = [body(0., 0., 0, 0), body(0., -300., 1, 0)];
        w.give(0, Item::BananaBunch);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        for _ in 0..30 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        let n = w.actors.iter().filter(|a| a.kind == Kind::Banana).count();
        assert_eq!(n, 5);
        let parent = w.actors.iter().find(|a| a.kind == Kind::BananaBunch).unwrap();
        assert_eq!(parent.available, 5);
        // A press drops the last one; a second throws the next forward.
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        assert_eq!(w.actors.iter().filter(|a| a.kind == Kind::Banana && a.state == banana::DROPPED).count(), 1);
        tick(&mut w, &bodies, &[Pad { press: true, stick_y: 70, ..Pad::default() }, Pad::default()], &open(), &road());
        let thrown = w.actors.iter().filter(|a| a.kind == Kind::Banana && a.state == banana::DROPPED).max_by(|a, b| a.vel[2].partial_cmp(&b.vel[2]).unwrap()).unwrap();
        assert!(thrown.vel[2] > 3., "thrown forward {:?}", thrown.vel);
        // Another kart hits a trailing banana: the rest of the bunch scatter.
        let target = w.actors.iter().find(|a| a.kind == Kind::Banana && matches!(a.state, banana::FIRST_BUNCH | banana::BUNCH)).unwrap().pos;
        let hit = [bodies[0], body(target[0], target[2], 1, 0)];
        let hits = tick(&mut w, &hit, &[Pad::default(); 2], &open(), &road());
        assert_eq!(hits.len(), 1);
        assert!(w.actors.iter().all(|a| !matches!(a.state, banana::FIRST_BUNCH | banana::BUNCH) || a.kind != Kind::Banana), "func_802B0464 / func_802B04E8 drop the others");
    }

    #[test]
    fn a_green_shell_held_then_released_swings_round_and_flies_at_8_or_1_2_times_speed() {
        for (speed, want) in [(5., 8.), (10., 12.)] {
            let mut w = world(1);
            let mut bodies = [body(0., 0., 0, 0)];
            bodies[0].speed = speed;
            w.give(0, Item::GreenShell);
            tick(&mut w, &bodies, &[press()], &open(), &road());
            assert_eq!(shells(&w)[0].state, shell::HELD);
            assert!(shells(&w)[0].pos[2] < -8., "held behind the kart: {:?}", shells(&w)[0].pos);
            tick(&mut w, &bodies, &[release(0)], &open(), &road());
            assert_eq!(shells(&w)[0].state, shell::RELEASED);
            let mut frames = 0;
            while shells(&w)[0].state == shell::RELEASED {
                tick(&mut w, &bodies, &[Pad::default()], &open(), &road());
                frames += 1;
                assert!(frames < 20);
            }
            assert!((8..=10).contains(&frames), "170 degrees at 20 per frame: {frames}");
            let s = shells(&w)[0];
            assert!((len(s.vel) - want).abs() < 1.5, "speed {:?} want {want}", s.vel);
            assert!(s.vel[2] > 5., "forward");
        }
        // Stick down: thrown backwards.
        let mut w = world(1);
        let mut bodies = [body(0., 0., 0, 0)];
        bodies[0].speed = 5.;
        w.give(0, Item::GreenShell);
        tick(&mut w, &bodies, &[press()], &open(), &road());
        tick(&mut w, &bodies, &[release(-60)], &open(), &road());
        let s = shells(&w)[0];
        assert_eq!(s.state, shell::MOVING);
        assert!(s.vel[2] < -7., "backwards at 8: {:?}", s.vel);
    }

    #[test]
    fn a_green_shell_bounces_off_a_wall_at_speed_four_and_hits_a_kart_with_the_low_tumble() {
        let mut w = world(2);
        let ground = Flat { wall_x: Some(40.) };
        let mut bodies = [body(0., 0., 0, 0), body(-3000., 0., 1, 0)];
        bodies[0].speed = 5.;
        bodies[0].rotation = -16384; // forward = +x
        w.give(0, Item::GreenShell);
        tick(&mut w, &bodies, &[press(), Pad::default()], &ground, &road());
        tick(&mut w, &bodies, &[release(0), Pad::default()], &ground, &road());
        for _ in 0..40 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &ground, &road());
        }
        let s = shells(&w)[0].clone();
        assert!(s.vel[0] < 0., "reflected off the wall: {:?}", s.vel);
        assert!((len(s.vel) - 4.).abs() < 0.6 || len(s.vel) < 4.6, "process_shell_collision: speed 4, got {}", len(s.vel));
        // Hit a kart: LOW_TUMBLE_TRIGGER.
        let hit = [body(s.pos[0], s.pos[2], 0, 0), bodies[1]];
        let mut hit2 = hit;
        hit2[0].flags = 0;
        let h = {
            // the owner is no longer protected after 30 frames
            tick(&mut w, &hit2, &[Pad::default(); 2], &ground, &road())
        };
        assert_eq!(h, vec![Hit { racer: 0, trigger: fx::LOW_TUMBLE_TRIGGER, item: Item::GreenShell, owner: 0 }], "a green shell can hit its owner after the first 30 frames");
    }

    #[test]
    fn triple_green_shells_orbit_in_three_stages_and_fire_one_per_press() {
        let mut w = world(2);
        let bodies = [body(0., 0., 0, 0), body(0., -3000., 1, 0)];
        w.give(0, Item::TripleGreenShell);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        assert_eq!(shells(&w).len(), 1, "first shell at once");
        let mut second = 0;
        let mut third = 0;
        for f in 1..80 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
            if second == 0 && shells(&w).len() == 2 {
                second = f;
            }
            if third == 0 && shells(&w).len() == 3 {
                third = f;
            }
        }
        // rotAngle starts at -180 and gains 8 per frame: > 300 after 16 frames; then 60..70 about 15 frames later
        assert!((12..=19).contains(&second), "second shell at frame {second}");
        assert!((28..=34).contains(&third), "third shell at frame {third}");
        assert_eq!(shells(&w).len(), 3);
        assert!(shells(&w).iter().all(|s| matches!(s.state, shell::TRIPLE_GREEN) && s.collidable));
        for s in shells(&w) {
            let r = ((s.pos[0] - bodies[0].pos[0]).powi(2) + (s.pos[2] - bodies[0].pos[2]).powi(2)).sqrt();
            assert!((r - 8.).abs() < 0.2, "orbit radius 8: {r}");
        }
        for k in 0..3 {
            tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
            assert_eq!(shells(&w).iter().filter(|s| s.state == shell::MOVING).count(), k + 1);
        }
        let m = shells(&w).iter().find(|s| s.state == shell::MOVING).unwrap().vel;
        assert!((len(m) - 8.).abs() < 0.6, "fired at 8: {m:?}");
        for _ in 0..3 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        assert!(w.slots[0].parent_actor.is_none(), "the parent goes when no shell is left");
    }

    /// Three racers on the road: the owner at z = 0 (given `rank`), others ahead.
    fn pack(rank: usize) -> [Body; 4] {
        let mut b = [body(0., 0., rank, 0), body(0., 400., 0, 20), body(0., 900., 0, 45), body(0., -300., 3, 0)];
        // ranks: 0 first (the farthest ahead)
        b[2].rank = 0;
        b[1].rank = 1;
        b[0].rank = rank;
        b[3].rank = 3;
        if rank == 1 {
            b[1].rank = 2;
        }
        b
    }

    #[test]
    fn a_red_shell_homes_on_the_racer_one_place_ahead_and_tumbles_it() {
        let mut w = world(4);
        let mut bodies = pack(2);
        bodies[0].rank = 2;
        bodies[1].rank = 1;
        bodies[2].rank = 0;
        w.give(0, Item::RedShell);
        let pads = [Pad::default(); 4];
        tick(&mut w, &bodies, &[press(), Pad::default(), Pad::default(), Pad::default()], &open(), &road());
        tick(&mut w, &bodies, &[release(0), Pad::default(), Pad::default(), Pad::default()], &open(), &road());
        let mut locked = false;
        let mut hit = None;
        for _ in 0..200 {
            let hits = tick(&mut w, &bodies, &pads, &open(), &road());
            locked |= shells(&w).iter().any(|s| s.state == shell::RED_LOCK_ON);
            if let Some(h) = hits.first() {
                hit = Some(*h);
                break;
            }
        }
        assert!(locked, "after 30 frames the shell locks on (RED_LOCK_ON)");
        assert_eq!(hit, Some(Hit { racer: 1, trigger: fx::HIGH_TUMBLE_TRIGGER, item: Item::RedShell, owner: 0 }), "rank - 1 is the kart directly ahead");
        assert!(w.actors.iter().all(|a| a.kind != Kind::Red || a.state == shell::DESTROYED));
    }

    #[test]
    fn a_red_shell_from_the_leader_follows_the_road_for_ten_seconds() {
        let mut w = world(2);
        let bodies = [body(0., 0., 0, 0), body(0., -400., 1, 0)];
        w.give(0, Item::RedShell);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        tick(&mut w, &bodies, &[release(0), Pad::default()], &open(), &road());
        for _ in 0..120 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        let s = &shells(&w)[0];
        assert_eq!(s.state, shell::TRIPLE_GREEN, "the leader's red shell has nobody to hunt: it follows the path");
        assert!(s.pos[2] > 100., "it drove down the road: {:?}", s.pos);
        assert!(s.pos[0].abs() < 12.);
        assert_eq!(s.timer + 120 - 30 - 8 - 1 > 0, true);
    }

    #[test]
    fn a_blue_shell_follows_the_road_to_the_leader_ignores_everyone_else_and_hits_the_leader() {
        let mut w = world(4);
        let mut bodies = pack(3);
        bodies[3].rank = 3;
        bodies[0].rank = 3;
        bodies[1].rank = 1;
        bodies[2].rank = 0;
        bodies[3] = body(0., -300., 2, 0);
        bodies[3].rank = 2;
        bodies[0].pos = [0., KART_RADIUS, 0.];
        w.give(0, Item::BlueShell);
        let mut pads = [Pad::default(); 4];
        pads[0] = press();
        tick(&mut w, &bodies, &pads, &open(), &road());
        pads[0] = release(0);
        tick(&mut w, &bodies, &pads, &open(), &road());
        pads[0] = Pad::default();
        let mut first_hit = None;
        let mut states = vec![];
        for _ in 0..400 {
            let hits = tick(&mut w, &bodies, &pads, &open(), &road());
            if let Some(s) = shells(&w).first() {
                if states.last() != Some(&s.state) {
                    states.push(s.state);
                }
            }
            if let Some(h) = hits.first() {
                first_hit.get_or_insert(*h);
            }
            if w.actors.iter().all(|a| a.kind != Kind::Blue || a.state == shell::DESTROYED) {
                break;
            }
        }
        assert!(states.contains(&shell::BLUE_LOCK_ON) && states.contains(&shell::BLUE_ELIMINATED), "path then homing: {states:?}");
        // It tumbles every kart it touches on the way (HIGH_TUMBLE) and is destroyed only on the leader (racer 2).
        let hit = first_hit.expect("the shell hits someone");
        assert_eq!(hit.trigger, fx::HIGH_TUMBLE_TRIGGER);
        assert!(w.actors.iter().all(|a| a.kind != Kind::Blue || a.state == shell::DESTROYED), "gone after the leader");
    }

    #[test]
    fn a_fake_item_box_is_held_dropped_grows_and_tumbles_whoever_hits_it_after_the_owner_grace() {
        let mut w = world(2);
        let bodies = [body(0., 0., 0, 0), body(0., -300., 1, 0)];
        w.give(0, Item::FakeBox);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        let f = w.actors.iter().find(|a| a.kind == Kind::Fake).unwrap();
        assert_eq!(f.state, fake::HELD);
        assert!((f.radius - 1.925).abs() < 0.01, "boundingBoxSize 1.925");
        tick(&mut w, &bodies, &[release(0), Pad::default()], &open(), &road());
        for _ in 0..45 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        let f = w.actors.iter().find(|a| a.kind == Kind::Fake).unwrap().clone();
        assert_eq!(f.state, fake::ON_GROUND);
        assert!((f.radius - 5.5).abs() < 0.01, "grows to 5.5 at 0.05 per frame");
        assert!((f.pos[1] - 8.66).abs() < 0.3, "floats 8.66 above the floor: {}", f.pos[1]);
        // the owner standing on it is safe for 100 frames, then it hurts
        let on_it = [body(f.pos[0], f.pos[2], 0, 0), bodies[1]];
        assert!(tick(&mut w, &on_it, &[Pad::default(); 2], &open(), &road()).is_empty());
        let away = [body(0., 500., 0, 0), bodies[1]];
        for _ in 0..90 {
            tick(&mut w, &away, &[Pad::default(); 2], &open(), &road());
        }
        let hits = tick(&mut w, &on_it, &[Pad::default(); 2], &open(), &road());
        assert_eq!(hits, vec![Hit { racer: 0, trigger: fx::VERTICAL_TUMBLE_TRIGGER, item: Item::FakeBox, owner: 0 }]);
        for _ in 0..25 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        assert!(w.actors.iter().all(|a| a.kind != Kind::Fake), "20 frames of break animation, then gone");
    }

    #[test]
    fn thunder_strikes_everyone_else_star_and_boo_target_the_user() {
        let mut w = world(3);
        let bodies = [body(0., 0., 2, 0), body(0., 100., 1, 0), body(0., 200., 0, 0)];
        w.give(0, Item::Thunderbolt);
        tick(&mut w, &bodies, &[press(), Pad::default(), Pad::default()], &open(), &road());
        let t = w.drain_triggers();
        assert_eq!(t, vec![(1, fx::LIGHTNING_STRIKE_TRIGGER), (2, fx::LIGHTNING_STRIKE_TRIGGER)]);
        assert!(w.drain_events().contains(&ItemEvent::Thunder(0)));
        w.give(0, Item::Star);
        tick(&mut w, &bodies, &[press(), Pad::default(), Pad::default()], &open(), &road());
        assert_eq!(w.drain_triggers(), vec![(0, fx::STAR_TRIGGER)]);
    }

    #[test]
    fn the_boo_rolls_a_new_item_81_times_in_100_and_can_steal_in_versus() {
        let mut got = 0;
        let mut w = world(2);
        let b = [body(0., 0., 1, 0), body(0., 50., 0, 0)];
        for _ in 0..1000 {
            w.give(0, Item::Boo);
            tick(&mut w, &b, &[press(), Pad::default()], &open(), &road());
            if matches!(w.slots[0].held, Some(i) if i != Item::Boo) {
                got += 1;
            }
            w.slots[0].held = None;
            w.drain_triggers();
        }
        assert!((750..=870).contains(&got), "random_int(100) < 0x51: {got}/1000");
        let mut w = world(2);
        w.boo_steals = true;
        w.slots[1].held = Some(Item::Star);
        w.give(0, Item::Boo);
        tick(&mut w, &b, &[press(), Pad::default()], &open(), &road());
        assert_eq!(w.held(0), Some(Item::Star));
        assert_eq!(w.held(1), None, "taken from the other racer");
    }

    #[test]
    fn items_are_refused_during_a_hit_and_mushrooms_in_the_air() {
        let mut w = world(1);
        let mut b = [body(0., 0., 0, 0)];
        w.give(0, Item::Banana);
        b[0].flags = fx::BANANA_SPINOUT_EFFECT;
        tick(&mut w, &b, &[press()], &open(), &road());
        assert_eq!(w.held(0), Some(Item::Banana), "prevent_item_use during a spinout");
        b[0].flags = 0;
        w.give(0, Item::Mushroom);
        b[0].flags = fx::MIDAIR_EFFECT;
        tick(&mut w, &b, &[press()], &open(), &road());
        assert_eq!(w.held(0), Some(Item::Mushroom), "no mushroom in mid-air");
        b[0].flags = 0;
        tick(&mut w, &b, &[press()], &open(), &road());
        assert_eq!(w.held(0), None);
    }

    #[test]
    fn shells_and_bananas_destroy_each_other_but_a_spiny_shell_survives() {
        let mut w = world(2);
        let bodies = [body(0., 0., 0, 0), body(0., -3000., 1, 0)];
        w.give(0, Item::Banana);
        tick(&mut w, &bodies, &[press(), Pad::default()], &open(), &road());
        tick(&mut w, &bodies, &[release(0), Pad::default()], &open(), &road());
        for _ in 0..20 {
            tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        }
        let bp = w.actors[0].pos;
        // a blue shell right on top of the banana: only the banana is destroyed
        let id = w.spawn(Kind::Blue, 1, bp);
        {
            let s = w.get_mut(id).unwrap();
            s.state = 200; // parked: only the collision rule is under test
            s.owner = 1;
            s.pos[0] += 1.;
        }
        tick(&mut w, &bodies, &[Pad::default(); 2], &open(), &road());
        assert!(w.actors.iter().all(|a| a.kind != Kind::Banana || a.state == banana::DESTROYED));
        assert!(w.get(id).is_some_and(|s| s.state != shell::DESTROYED), "evaluate_actor_collision_between_two_destructible_actors keeps the spiny");
    }

    #[test]
    fn a_cpu_uses_items_through_set_use() {
        let mut w = world(2);
        let b = [body(0., 0., 0, 0), body(0., -300., 1, 0)];
        w.give(1, Item::Banana);
        w.set_use(1, ItemUse { press: true, ..Pad::default() });
        tick(&mut w, &b, &[Pad::default(); 2], &open(), &road());
        assert!(w.actors.iter().any(|a| a.kind == Kind::Banana && a.owner == 1), "the CPU's press consumed its banana");
        w.set_use(1, ItemUse { release: true, stick_y: 0, ..Pad::default() });
        tick(&mut w, &b, &[Pad::default(); 2], &open(), &road());
        assert_eq!(w.actors.iter().find(|a| a.kind == Kind::Banana).unwrap().state, banana::DROPPED);
    }
}
