//! Banjo-Kazooie enemies as sim objects (`ObjectBehaviorKind::BkEnemy`), placed by
//! `scripts/export-bk-setup.py` from the maps' setup files and drawn by `native/src/bk_enemies/`.
//!
//! One behavior serves every species; `param = actor id | scale percent << 16` picks the [`Kind`]
//! (`ActorInfo.actorId` of the decomp). Each species keeps the decomp's actor state numbers, so the exported
//! `ActorAnimationInfo` table indexes the animation: `SimObject::action` IS the BK `Actor::state`.
//!
//! Sources (`reference/banjo-kazooie/src`): humanoid baddies (Grublin, Grublin Hood, Limbo) `core2/code_53C10.c
//! humanoidBaddie_update` + `MM/ch/grublin.c`, `CCW/ch/grublinhood.c`, `core2/ch/skeleton.c`; Ticker `core2/ch/termite.c`;
//! Bigbutt `core2/ch/bigbutt.c`; Snippet and Mutie Snippet `core2/ch/crab.c`; Sir Slush + snowball `core2/ch/snowman.c`,
//! `snowball.c`; Bat `core2/ch/bat.c`; Shrapnel `core2/ch/shrapnel.c`; Snare-bear `CCW/ch/snarebear.c`; Beehive
//! `core2/ch/beehive.c`.
//!
//! Units: BK speeds are per 30 Hz frame and lengths are BK units, drawn at the worlds' 1.25 scale (`S`).
//! Characters hit the objects through the shared interaction pass (`object_manager::detect_mario_interactions`):
//! Mario's punches/kicks/stomps/ground pound/dive, Link's sword and projectiles (`link::moves::hit_object`) and
//! Banjo's moves and eggs (`banjo_ext::step_eggs`) all arrive as `INT_STATUS_WAS_ATTACKED` + an `ATTACK_*` type:
//! punch/kick (claw, peck, egg, sword) are *weak* (the decomp's `ow`), everything else *strong* (`die`).
//! Contact damage uses the same pass (`INTERACT_DAMAGE`), so every character takes wedges and knockback.
use crate::collision::CollisionWorld;
use crate::math::*;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;
use crate::object_manager::ObjectEvent;

/// World scale of the exported BK maps (`export-bk-levels.py`).
pub const S: f64 = 1.25;
/// Enemies further than this from the player are frozen and hidden (iPhone budget).
pub const ACTIVE_RANGE: f64 = 7000.0;
const TICKS: f64 = 30.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Grublin,
    GrublinHood,
    Limbo,
    Termite,
    Bigbutt,
    Snippet,
    MutieSnippet,
    SirSlush,
    Bat,
    Shrapnel,
    Snarebear,
    Beehive,
    Snowball,
    Clucker,
    GrilleChompa,
    Flibbit,
    Buzzbomb,
    TeeHee,
    Nipper,
    Slappa,
    Mumbo,
    Podium,
    Tanktup,
    Grabba,
}

pub const ALL_KINDS: [Kind; 24] = [
    Kind::Grublin,
    Kind::GrublinHood,
    Kind::Limbo,
    Kind::Termite,
    Kind::Bigbutt,
    Kind::Snippet,
    Kind::MutieSnippet,
    Kind::SirSlush,
    Kind::Bat,
    Kind::Shrapnel,
    Kind::Snarebear,
    Kind::Beehive,
    Kind::Snowball,
    Kind::Clucker,
    Kind::GrilleChompa,
    Kind::Flibbit,
    Kind::Buzzbomb,
    Kind::TeeHee,
    Kind::Nipper,
    Kind::Slappa,
    Kind::Mumbo,
    Kind::Podium,
    Kind::Tanktup,
    Kind::Grabba,
];

/// Actor id of the snowball Sir Slush lobs (`ACTOR_125_SNOWBALL`).
pub const ACTOR_SNOWBALL: u32 = 0x125;

impl Kind {
    /// `ActorInfo.actorId` (include/enums.h `enum actor_e`).
    pub fn actor(self) -> u32 {
        match self {
            Kind::Grublin => 0x06,
            Kind::GrublinHood => 0x375,
            Kind::Limbo => 0x34E,
            Kind::Termite => 0x05,
            Kind::Bigbutt => 0x04,
            Kind::Snippet => 0x67,
            Kind::MutieSnippet => 0xF5,
            Kind::SirSlush => 0x124,
            Kind::Bat => 0x163,
            Kind::Shrapnel => 0x56,
            Kind::Snarebear => 0x1E9,
            Kind::Beehive => 0x12,
            Kind::Snowball => ACTOR_SNOWBALL,
            Kind::Clucker => 0x29F,
            Kind::GrilleChompa => 0x1CC,
            Kind::Flibbit => 0x133,
            Kind::Buzzbomb => 0x134,
            Kind::TeeHee => 0xCA,
            Kind::Nipper => 0x117,
            Kind::Slappa => 0x120,
            Kind::Mumbo => 0x7,
            Kind::Podium => 0x3B7,
            Kind::Tanktup => 0xE8,
            Kind::Grabba => 0x118,
        }
    }

    pub fn from_actor(id: u32) -> Option<Kind> {
        if id == 0x3BC {
            return Some(Kind::Podium);
        }
        ALL_KINDS.iter().copied().find(|k| k.actor() == id)
    }

    pub fn from_index(i: usize) -> Kind {
        ALL_KINDS[i.min(ALL_KINDS.len() - 1)]
    }

    pub fn index(self) -> usize {
        ALL_KINDS.iter().position(|k| *k == self).unwrap()
    }

    /// File stem under `public/models/bk/enemies/` ("" = drawn procedurally).
    pub fn model(self) -> &'static str {
        match self {
            Kind::Grublin => "grublin",
            Kind::GrublinHood => "grublin_hood",
            Kind::Limbo => "limbo",
            Kind::Termite => "termite",
            Kind::Bigbutt => "bigbutt",
            Kind::Snippet => "snippet",
            Kind::MutieSnippet => "mutie_snippet",
            Kind::SirSlush => "sir_slush",
            Kind::Bat => "bat",
            Kind::Shrapnel => "shrapnel",
            Kind::Snarebear => "snarebear",
            Kind::Beehive => "beehive",
            Kind::Snowball => "",
            Kind::Clucker => "clucker",
            Kind::GrilleChompa => "grille_chompa",
            Kind::Flibbit => "flibbit",
            Kind::Buzzbomb => "buzzbomb",
            Kind::TeeHee => "teehee",
            Kind::Nipper => "nipper",
            Kind::Slappa => "slappa",
            Kind::Mumbo => "mumbo",
            Kind::Podium => "podium",
            Kind::Tanktup => "tanktup",
            Kind::Grabba => "grabba",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Grublin => "Grublin",
            Kind::GrublinHood => "Grublin Hood",
            Kind::Limbo => "Limbo",
            Kind::Termite => "Ticker",
            Kind::Bigbutt => "Bigbutt",
            Kind::Snippet => "Snippet",
            Kind::MutieSnippet => "Mutie Snippet",
            Kind::SirSlush => "Sir Slush",
            Kind::Bat => "Bat",
            Kind::Shrapnel => "Shrapnel",
            Kind::Snarebear => "Snare-bear",
            Kind::Beehive => "Beehive",
            Kind::Snowball => "Snowball",
            Kind::Clucker => "Clucker",
            Kind::GrilleChompa => "Grille Chompa",
            Kind::Flibbit => "Flibbit",
            Kind::Buzzbomb => "Buzzbomb",
            Kind::TeeHee => "Tee-Hee",
            Kind::Nipper => "Nipper",
            Kind::Slappa => "Slappa",
            Kind::Mumbo => "Mumbo",
            Kind::Podium => "Jiggy podium",
            Kind::Tanktup => "Tanktup",
            Kind::Grabba => "Grabba",
        }
    }

    /// Hit box (radius, height) in world units at scale 1, hit points, contact damage in wedges, loot coins.
    pub fn spec(self) -> Spec {
        let (radius, height, hp, damage, coins) = match self {
            Kind::Grublin => (75.0, 210.0, 1, 1.0, 2.0),
            Kind::GrublinHood => (80.0, 250.0, 1, 1.0, 3.0),
            Kind::Limbo => (80.0, 250.0, 2, 1.0, 3.0),
            Kind::Termite => (60.0, 110.0, 2, 0.0, 1.0),
            Kind::Bigbutt => (115.0, 330.0, 99, 2.0, 5.0),
            Kind::Snippet => (100.0, 100.0, 2, 1.0, 2.0),
            Kind::MutieSnippet => (200.0, 150.0, 2, 1.0, 5.0),
            Kind::SirSlush => (190.0, 560.0, 1, 0.0, 5.0),
            Kind::Bat => (80.0, 90.0, 1, 1.0, 2.0),
            Kind::Shrapnel => (110.0, 140.0, 1, 1.0, 2.0),
            Kind::Snarebear => (200.0, 300.0, 99, 1.0, 0.0),
            Kind::Beehive => (70.0, 210.0, 1, 0.0, 0.0),
            Kind::Snowball => (70.0, 90.0, 1, 1.0, 0.0),
            Kind::Clucker => (150.0, 260.0, 1, 1.0, 3.0),
            Kind::GrilleChompa => (110.0, 150.0, 1, 1.0, 2.0),
            Kind::Flibbit => (70.0, 100.0, 1, 1.0, 2.0),
            Kind::Buzzbomb => (80.0, 90.0, 1, 1.0, 2.0),
            Kind::TeeHee => (90.0, 200.0, 1, 1.0, 3.0),
            Kind::Nipper => (70.0, 100.0, 3, 1.0, 10.0),
            Kind::Slappa => (200.0, 400.0, 1, 2.0, 10.0),
            Kind::Mumbo => (50.0, 120.0, 99, 0.0, 0.0),
            Kind::Podium => (10.0, 10.0, 99, 0.0, 0.0),
            Kind::Tanktup => (300.0, 400.0, 4, 0.0, 10.0),
            Kind::Grabba => (150.0, 250.0, 1, 0.0, 10.0),
        };
        Spec { radius, height, hp, damage, coins }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub radius: f64,
    pub height: f64,
    pub hp: i32,
    pub damage: f64,
    pub coins: f64,
}

// ---- actor states of the decomp (the animation tables are indexed by them) ----
pub mod hum {
    pub const IDLE_LOOK: i32 = 1;
    pub const IDLE: i32 = 2;
    pub const RANDOM_CHASE: i32 = 3;
    pub const ALERT: i32 = 4;
    pub const DIE: i32 = 5;
    pub const TURN_TO_PLAYER: i32 = 6;
    pub const CHASE: i32 = 7;
    pub const IDLE_TURN: i32 = 8;
    pub const INVULNERABLE: i32 = 9;
    pub const EXIT_INVULNERABLE: i32 = 10;
}
pub mod bull {
    pub const EAT: i32 = 1;
    pub const WALK: i32 = 2;
    pub const RUN: i32 = 3;
    pub const SLIDE: i32 = 4;
    pub const ATTACK: i32 = 5;
    pub const ALERT: i32 = 6;
    pub const WALK_TO_PLAYER: i32 = 8;
    pub const CHARGE: i32 = 9;
    pub const OW: i32 = 12;
    pub const FALL: i32 = 13;
    pub const DOWN: i32 = 14;
    pub const GET_UP: i32 = 15;
}
pub mod ant {
    pub const IDLE: i32 = 1;
    pub const WALK: i32 = 2;
    pub const OW: i32 = 6;
}
pub mod crab {
    pub const IDLE: i32 = 1;
    pub const WANDER: i32 = 2;
    pub const TURN_TO_PLAYER: i32 = 3;
    pub const CHASE: i32 = 4;
    pub const UPSIDE_DOWN: i32 = 5;
    pub const FLIP_OVER: i32 = 6;
    pub const FLIP_UPRIGHT: i32 = 7;
}
pub mod slush {
    pub const IDLE: i32 = 1;
    pub const ATTACK: i32 = 2;
}
pub mod bat {
    pub const ROOST: i32 = 1;
    pub const EXIT_ROOST: i32 = 2;
    pub const CHASE: i32 = 3;
    pub const ROAM: i32 = 4;
    pub const FLY_HOME: i32 = 5;
    pub const ENTER_ROOST: i32 = 6;
    pub const FALL: i32 = 7;
    pub const DIE: i32 = 8;
}
pub mod mine {
    pub const IDLE: i32 = 1;
    pub const TURN: i32 = 2;
    pub const CHASE: i32 = 3;
    pub const EXPLODE: i32 = 4;
}
pub mod bear {
    pub const IDLE: i32 = 1;
    pub const ATTACK: i32 = 2;
}
pub mod hive {
    pub const IDLE: i32 = 1;
    pub const DIE: i32 = 2;
}
/// `enum clucker_states` / `enum chompa_states` (the rooted biters).
pub mod plant {
    pub const IDLE: i32 = 1;
    pub const ALERT: i32 = 2;
    pub const WINDUP: i32 = 3;
    pub const ATTACK: i32 = 4;
    pub const DIE: i32 = 5;
    pub const DEAD: i32 = 6;
    /// Clucker's long bite (a state of its own here so the animation table can pick the clip).
    pub const ATTACK_LONG: i32 = 7;
}
/// `enum chTanktupState` (the body of the BGS boss): 1 standing, 2 hit, 3 mouth open (beaten).
pub mod tank {
    pub const IDLE: i32 = 1;
    pub const HIT: i32 = 2;
    pub const OPEN: i32 = 3;
}
/// `enum chGrabba_state_e`: 1 buried, 2 rising, 3 up, 4 hiding, 5 beaten.
pub mod grab {
    pub const WAIT: i32 = 1;
    pub const RISE: i32 = 2;
    pub const UP: i32 = 3;
    pub const HIDE: i32 = 4;
    pub const BEATEN: i32 = 5;
}
/// Mumbo's actor states (`D_80367490`): 3 idle, 5 transform spell.
pub mod mumbo {
    pub const IDLE: i32 = 3;
    pub const SPELL: i32 = 5;
}
/// `enum chSlappa_state_e`.
pub mod slap {
    pub const IDLE: i32 = 1;
    pub const RISE: i32 = 2;
    pub const CHASE: i32 = 3;
    pub const WINDUP: i32 = 4;
    pub const ATTACK: i32 = 5;
    pub const LAYING: i32 = 6;
    pub const GETTING_UP: i32 = 7;
    pub const LOWER: i32 = 8;
    pub const DIE: i32 = 9;
}
/// `enum ch_nipper_states_e`: 1 idle, 2 vulnerable (shell open), 3 pinching, 4 hit, 6 dying, 7 dead.
pub mod nip {
    pub const IDLE: i32 = 1;
    pub const VULNERABLE: i32 = 2;
    pub const ATTACK: i32 = 3;
    pub const HIT: i32 = 4;
    pub const DYING: i32 = 6;
    pub const DEAD: i32 = 7;
}
/// `enum chteehee_state_e`.
pub mod ghost {
    pub const IDLE: i32 = 1;
    pub const ALERT: i32 = 2;
    pub const CHASE: i32 = 3;
    pub const DIE: i32 = 6;
}
/// Buzzbomb (`code_45310.c`): 1 hover, 2 drift to a new spot, 3 dive at Banjo, 4 climb away after three dives, 5 hit (rises, spins),
/// 6 dying (falls), 7 fading.
pub mod buzz {
    pub const IDLE: i32 = 1;
    pub const MOVE: i32 = 2;
    pub const DIVE: i32 = 3;
    pub const RECOVER: i32 = 4;
    pub const OW: i32 = 5;
    pub const DIE: i32 = 6;
    pub const FADE: i32 = 7;
}
/// `enum chFlibbitState`.
pub mod flib {
    pub const IDLE: i32 = 1;
    pub const HOP: i32 = 2;
    pub const TURN: i32 = 3;
    pub const BUMP: i32 = 4;
    pub const OW: i32 = 5;
    pub const DIE: i32 = 6;
    pub const DEAD: i32 = 7;
}

/// Attack classes of the shared interaction pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blow {
    None,
    /// Peck, claw, egg, sword poke: the decomp's `ow` (hit) callback.
    Weak,
    /// Beak Buster, barge, dive, ground pound, stomp, explosion: the `die` callback.
    Strong,
}

pub fn blow_of(attack: i32) -> Blow {
    match attack {
        ATTACK_PUNCH | ATTACK_KICK_OR_TRIP => Blow::Weak,
        0 => Blow::None,
        _ => Blow::Strong,
    }
}

// ---- small helpers ----
pub fn kind_of(o: &SimObject) -> Kind {
    Kind::from_index(o.var("kind") as usize)
}

fn deg(a: f64) -> f64 {
    a * 65536.0 / 360.0
}

/// `randf2` from a per-object xorshift so the enemies do not share the global generator (tests stay stable).
fn rand(o: &mut SimObject) -> f64 {
    let mut x = o.var("rng") as u32;
    if x == 0 {
        // Seeded from the spawn position, not `o.id`: ids come from a process-wide counter, so they depend on how many
        // objects other threads (parallel tests) created and made the enemies' choices differ from run to run.
        let h = (o.home[0].to_bits() ^ o.home[1].to_bits().rotate_left(21) ^ o.home[2].to_bits().rotate_left(42)) as u32;
        x = 0x9E37_79B9 ^ h.wrapping_mul(2_654_435_761) | 1;
    }
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    o.set_var("rng", x as f64);
    x as f64 / 4_294_967_296.0
}

fn rand2(o: &mut SimObject, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * rand(o)
}

fn ticks(seconds: f64) -> f64 {
    seconds * TICKS
}

pub fn set_state(o: &mut SimObject, state: i32, once: bool) {
    o.set_action(state);
    o.set_var("once", if once { 1.0 } else { 0.0 });
    o.set_var("animDur", 0.0);
}

fn turn(o: &mut SimObject, ideal: f64, deg_per_tick: f64) {
    o.turn_toward_angle(ideal, deg(deg_per_tick));
    o.face_yaw = o.move_yaw;
}

fn yaw_diff(o: &SimObject, ideal: f64) -> f64 {
    (to_s16(ideal - o.move_yaw) as f64).abs() * 360.0 / 65536.0
}

/// `subaddie_set_ideal_yaw` + `func_80328CEC`: pick a new heading `min..max` degrees off `base`, either side.
fn random_heading(o: &mut SimObject, base: f64, min: f64, max: f64) {
    let swing = (rand(o) - 0.5) * (max - min) * 2.0;
    let side = if swing >= 0.0 { min } else { -min };
    o.set_var("ideal", to_s16(base + deg(side + swing)) as f64);
}

fn set_box(o: &mut SimObject, kind: Kind, interact: u32, damage: f64) {
    let s = kind.spec();
    o.hitbox.interact_type = interact;
    o.hitbox.damage_or_coin_value = damage;
    o.hitbox.radius = s.radius * o.scale;
    o.hitbox.height = s.height * o.scale;
    o.hitbox.down_offset = 0.0;
}

fn flat_distance(a: &Vec3, b: &Vec3) -> f64 {
    js_hypot(a[0] - b[0], a[2] - b[2])
}

/// Drop a floor-bound enemy onto the ground below its placement the first time it is simulated.
fn snap_to_floor(o: &mut SimObject, world: &CollisionWorld) {
    if o.var("placed") != 0.0 {
        return;
    }
    o.set_var("placed", 1.0);
    let floor = world.find_floor(o.pos[0], o.pos[1] + 300.0, o.pos[2]);
    if floor.surface.is_some() && (floor.height - o.pos[1]).abs() < 900.0 {
        o.pos[1] = floor.height;
        o.home[1] = floor.height;
    }
}

/// Defeat: puff, loot, maybe a heart, then gone until the course reloads (the decomp's `marker_despawn`).
pub fn finish(id: ObjId, m: &mut MarioRuntime) {
    let kind = kind_of(&m.objects[id]);
    let pos = m.objects[id].pos;
    m.objects.events.push(ObjectEvent::Break { pos, wall: None });
    for _ in 0..kind.spec().coins as usize {
        m.objects.spawn_coin(pos, true, 1.0);
    }
    let heart = rand(&mut m.objects[id]) < 0.25 && kind.spec().coins > 0.0;
    if heart {
        heal_a_heart(m);
    }
    let o = &mut m.objects[id];
    o.set_var("died", 1.0);
    o.deactivate(-1.0);
}

/// One wedge back for Mario and Banjo; Link keeps his health as hearts of his own (`link::items::Inventory`), the SM64 meter is not his.
pub fn heal_a_heart(m: &mut MarioRuntime) {
    if m.link.enabled {
        m.link.inv.heal(crate::link::items::HEART);
    } else {
        m.heal_counter += 4.0;
    }
}

/// Wedges of damage and the touch flag from this tick's interaction pass.
struct Contact {
    blow: Blow,
    touched: bool,
}

fn take_contact(o: &mut SimObject) -> Contact {
    let status = o.interact_status;
    o.interact_status = 0;
    if status & INT_STATUS_WAS_ATTACKED != 0 {
        // counted for the sound of the hit (`audio::bk_events`)
        o.set_var("struck", o.var("struck") + 1.0);
        return Contact { blow: blow_of((status & 0xff) as i32), touched: false };
    }
    Contact { blow: Blow::None, touched: status & INT_STATUS_INTERACTED != 0 }
}

// ---- init ----
pub fn init_bk_enemy(o: &mut SimObject) {
    let actor = (o.param as u32) & 0xFFFF;
    let pct = ((o.param as u32) >> 16) as f64;
    let kind = Kind::from_actor(actor).unwrap_or(Kind::Termite);
    o.set_var("kind", kind.index() as f64);
    o.scale = if pct > 0.0 { pct / 100.0 } else { 1.0 };
    o.set_var("hp", kind.spec().hp as f64);
    o.gravity = 4.0;
    o.friction = 1.0;
    o.buoyancy = 1.0;
    o.visible = false;
    set_box(o, kind, if matches!(kind, Kind::Podium | Kind::Mumbo) { 0 } else { INTERACT_BREAKABLE }, 0.0);
    o.hitbox.health = kind.spec().hp as f64;
    let first = match kind {
        Kind::Grublin | Kind::GrublinHood | Kind::Limbo => hum::IDLE,
        Kind::Termite => ant::IDLE,
        Kind::Bigbutt => bull::EAT,
        Kind::Snippet | Kind::MutieSnippet => crab::IDLE,
        Kind::SirSlush => slush::IDLE,
        Kind::Bat => bat::ROOST,
        Kind::Shrapnel => mine::IDLE,
        Kind::Snarebear => bear::IDLE,
        Kind::Beehive => hive::IDLE,
        Kind::Snowball => 1,
        Kind::Clucker | Kind::GrilleChompa => plant::IDLE,
        Kind::Flibbit => flib::IDLE,
        Kind::Buzzbomb => buzz::IDLE,
        Kind::TeeHee => ghost::IDLE,
        Kind::Nipper => nip::IDLE,
        Kind::Slappa => slap::IDLE,
        Kind::Mumbo => mumbo::IDLE,
        Kind::Podium => 1,
        Kind::Tanktup => tank::IDLE,
        Kind::Grabba => grab::WAIT,
    };
    set_state(o, first, false);
    o.set_var("ideal", o.move_yaw);
    if kind == Kind::Snowball {
        o.active = false;
    }
}

// ---- the behavior ----
pub fn bk_enemy_behavior(id: ObjId, m: &mut MarioRuntime) {
    let kind = kind_of(&m.objects[id]);
    let world = m.world.clone();
    // Link's spin attacks, jump slashes and hammer are strong blows (the damage table of the swing), but the shared interaction pass hands
    // every blade hit over as a weak punch / kick.
    if m.link.blade_is_strong(m) {
        let o = &mut m.objects[id];
        if o.interact_status & INT_STATUS_WAS_ATTACKED != 0 && matches!((o.interact_status & 0xff) as i32, ATTACK_PUNCH | ATTACK_KICK_OR_TRIP) {
            o.interact_status = (o.interact_status & !0xff) | ATTACK_GROUND_POUND_OR_TWIRL as u32;
        }
    }
    {
        let o = &mut m.objects[id];
        if o.distance_to_mario > ACTIVE_RANGE && kind != Kind::Snowball {
            o.visible = false;
            o.interact_status = 0;
            return;
        }
        o.visible = true;
        if !matches!(kind, Kind::Bat | Kind::Shrapnel | Kind::Snowball | Kind::Buzzbomb | Kind::TeeHee | Kind::Grabba) {
            snap_to_floor(o, &world);
        }
        if kind == Kind::Shrapnel {
            o.set_var("placed", 1.0);
        }
    }
    match kind {
        Kind::Grublin | Kind::GrublinHood | Kind::Limbo => humanoid(id, m, kind),
        Kind::Termite => termite(id, m),
        Kind::Bigbutt => bigbutt(id, m),
        Kind::Snippet | Kind::MutieSnippet => snippet(id, m, kind),
        Kind::SirSlush => sir_slush(id, m),
        Kind::Bat => bat_flight(id, m),
        Kind::Shrapnel => shrapnel(id, m),
        Kind::Snarebear => snarebear(id, m),
        Kind::Beehive => beehive(id, m),
        Kind::Snowball => snowball(id, m),
        Kind::Clucker | Kind::GrilleChompa => plant(id, m, kind),
        Kind::Flibbit => flibbit(id, m),
        Kind::Buzzbomb => buzzbomb(id, m),
        Kind::TeeHee => teehee(id, m),
        Kind::Nipper => nipper(id, m),
        Kind::Slappa => slappa(id, m),
        Kind::Mumbo => mumbo_npc(id, m),
        Kind::Tanktup => tanktup(id, m),
        Kind::Grabba => grabba(id, m),
        Kind::Podium => {
            // scenery: the lair's jigsaw podium model (`ACTOR_3B7` / `3BC`); the logic is `banjo_ext::tend_podiums`
            let o = &mut m.objects[id];
            let _ = take_contact(o);
            set_box(o, Kind::Podium, 0, 0.0);
        }
    }
    // Link's sword steps and lunges forward (the finisher, the stab): the baddies' bodies are solid to him like an OoT enemy's collider, so he
    // is stopped at their body instead of ending up inside, or behind, them. Only the core of the hitbox pushes: the blade still reaches.
    if m.link.enabled && link_is_blocked_by(kind) {
        let o = &m.objects[id];
        if o.active && m.pos[1] < o.pos[1] + o.hitbox.height && m.pos[1] + 100.0 > o.pos[1] {
            let pad = -0.45 * o.hitbox.radius;
            push_mario_out_of_object(m, id, pad);
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
}

/// The baddies that stand on the ground and have a body to bump into (not the flyers, ghosts, mines, hives, snowballs, Mumbo, the podiums or the
/// buried biters).
fn link_is_blocked_by(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Grublin | Kind::GrublinHood | Kind::Limbo | Kind::Termite | Kind::Bigbutt | Kind::Snippet | Kind::MutieSnippet | Kind::SirSlush | Kind::Snarebear | Kind::Flibbit | Kind::Nipper | Kind::Slappa | Kind::Tanktup
    )
}

fn walk(o: &mut SimObject, world: &CollisionWorld, speed: f64) {
    o.forward_vel = speed * S;
    let radius = o.hitbox.radius.min(90.0);
    o.step_physics(world, Some(radius));
}

// ---------------------------------------------------------------------------------------------
// Humanoid baddies: Grublin, Grublin Hood, Limbo (`humanoidBaddie_update`)
// ---------------------------------------------------------------------------------------------

struct Humanoid {
    /// `unk0..unk4`: wander speed range.
    wander: (f64, f64),
    /// `unk8..unk9`: random-chase speed range.
    random_chase: (f64, f64),
    /// `local->yaw`: alert animation cycles before the chase.
    alerts: f64,
    /// `unkA`, `unkB`: chase speed and turn rate.
    chase: f64,
    turn: f64,
}

fn humanoid_params(kind: Kind) -> Humanoid {
    match kind {
        // MM/ch/grublin.c chGrublin_initialize
        Kind::Grublin => Humanoid { wander: (1.5, 2.3), random_chase: (5.0, 7.0), alerts: 2.0, chase: 9.0, turn: 9.0 },
        // CCW/ch/grublinhood.c __chgrublinhood_initialize
        Kind::GrublinHood => Humanoid { wander: (5.0, 8.0), random_chase: (6.0, 12.0), alerts: 1.0, chase: 16.0, turn: 8.0 },
        // core2/ch/skeleton.c _chskeleton_init
        _ => Humanoid { wander: (3.0, 6.0), random_chase: (4.0, 8.0), alerts: 1.0, chase: 10.0, turn: 7.0 },
    }
}

const SIGHT: f64 = 900.0 * S;

fn humanoid(id: ObjId, m: &mut MarioRuntime, kind: Kind) {
    humanoid_step(id, m, kind);
    let o = &mut m.objects[id];
    // A heap of bones, a dying baddie: solid and attackable but harmless; otherwise the touch hurts.
    let harmless = matches!(o.action, hum::DIE | hum::INVULNERABLE | hum::EXIT_INVULNERABLE);
    if harmless {
        set_box(o, kind, INTERACT_BREAKABLE, 0.0);
    } else {
        set_box(o, kind, INTERACT_DAMAGE, kind.spec().damage);
    }
}

fn humanoid_step(id: ObjId, m: &mut MarioRuntime, kind: Kind) {
    let p = humanoid_params(kind);
    let mpos = m.pos;
    let invincible = m.invinc_timer > 0.0;
    let world = m.world.clone();
    let contact = take_contact(&mut m.objects[id]);
    // Hits. Grublins have no `ow` (any hit is lethal); the hood backs off from weak hits; Limbo collapses.
    if contact.blow != Blow::None {
        let state = m.objects[id].action;
        let alive = !matches!(state, hum::DIE | hum::INVULNERABLE | hum::EXIT_INVULNERABLE);
        if alive {
            let lethal = match kind {
                Kind::Grublin => true,
                _ => contact.blow == Blow::Strong,
            };
            if lethal {
                let o = &mut m.objects[id];
                set_state(o, hum::DIE, true);
                o.forward_vel = 0.0;
                o.set_var("t", 0.0);
                set_box(o, kind, INTERACT_BREAKABLE, 0.0);
                return;
            } else if kind == Kind::Limbo {
                let o = &mut m.objects[id];
                set_state(o, hum::INVULNERABLE, true);
                let wait = rand2(o, 3.0, 6.0);
                o.set_var("lifetime", ticks(wait));
                return;
            } else {
                // humanoidBaddie_ow: flinch and turn away (func_802DAE40)
                let o = &mut m.objects[id];
                let base = o.move_yaw;
                random_heading(o, base, 160.0, 200.0);
                set_state(o, hum::IDLE, false);
                o.set_var("pause", 90.0);
            }
        }
    }
    let o = &mut m.objects[id];
    let to_player = o.angle_to_mario;
    let near_flat = flat_distance(&o.pos, &mpos);
    let level = (mpos[1] - o.pos[1]).abs() < 700.0;
    let t = o.var("t") + 1.0;
    o.set_var("t", t);
    let pause = o.var("pause");
    if pause > 0.0 {
        o.set_var("pause", pause - 1.0);
    }
    let sees_player = near_flat < SIGHT && level && o.var("pause") <= 0.0;
    let speed_var = o.var("speed");
    match o.action {
        hum::DIE => {
            if o.action_timer + 1.0 >= ticks(if kind == Kind::GrublinHood { 1.6 } else if kind == Kind::Limbo { 0.7 } else { 1.0 }) {
                finish(id, m);
                return;
            }
            o.action_timer += 1.0;
        }
        hum::INVULNERABLE => {
            o.action_timer += 1.0;
            if o.action_timer >= o.var("lifetime") {
                set_state(o, hum::EXIT_INVULNERABLE, true);
            }
        }
        hum::EXIT_INVULNERABLE => {
            o.action_timer += 1.0;
            if o.action_timer >= ticks(2.2) {
                set_state(o, hum::IDLE, false);
            }
        }
        hum::IDLE_LOOK => {
            o.action_timer += 1.0;
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(0.58 * 4.0) {
                set_state(o, hum::IDLE, false);
            }
            if sees_player {
                set_state(o, hum::TURN_TO_PLAYER, false);
            }
        }
        hum::IDLE | hum::RANDOM_CHASE | hum::IDLE_TURN => {
            o.action_timer += 1.0;
            if o.var("speed") == 0.0 {
                let s = if o.action == hum::RANDOM_CHASE { rand2(o, p.random_chase.0, p.random_chase.1) } else { rand2(o, p.wander.0, p.wander.1) };
                o.set_var("speed", s);
            }
            let ideal = o.var("ideal");
            turn(o, ideal, if o.action == hum::RANDOM_CHASE { 3.0 } else { 2.0 });
            let tick = o.timer as i64;
            if tick & 0xF == 0 {
                let base = o.var("ideal");
                random_heading(o, base, 10.0, 20.0);
            }
            // leash: head home when far from the placement
            if flat_distance(&o.pos, &o.home) > 900.0 * S {
                let home_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
                o.set_var("ideal", home_yaw);
            }
            if tick & 0xF == 0 && o.action == hum::IDLE && rand(o) < 0.13 {
                set_state(o, hum::RANDOM_CHASE, false);
                o.set_var("speed", 0.0);
            } else if tick & 0xF == 0 && o.action == hum::RANDOM_CHASE && rand(o) < 0.08 {
                set_state(o, hum::IDLE, false);
                o.set_var("speed", 0.0);
            } else if tick & 7 == 0 && o.action == hum::IDLE && rand(o) < 0.02 {
                set_state(o, hum::IDLE_LOOK, false);
            }
            let sp = o.var("speed");
            if o.var("pause") <= 0.0 || o.action != hum::IDLE {
                walk(o, &world,sp);
            } else {
                walk(o, &world,sp * 0.6);
            }
            let o = &mut m.objects[id];
            if sees_player {
                o.forward_vel = 0.0;
                set_state(o, hum::TURN_TO_PLAYER, false);
                o.set_var("speed", 0.0);
            }
        }
        hum::TURN_TO_PLAYER => {
            o.forward_vel = 0.0;
            turn(o, to_player, 4.0);
            if near_flat > SIGHT || !level {
                set_state(o, hum::IDLE_LOOK, false);
            } else if yaw_diff(o, to_player) < 5.0 {
                set_state(o, hum::ALERT, false);
                o.set_var("alerts", p.alerts);
            }
        }
        hum::ALERT => {
            o.forward_vel = 0.0;
            o.action_timer += 1.0;
            turn(o, to_player, 4.0);
            let cycle = ticks(0.9);
            if o.action_timer >= cycle / 2.0 && o.var("alertHalf") == 0.0 {
                o.set_var("alertHalf", 1.0);
                let n = o.var("alerts") - 1.0;
                o.set_var("alerts", n.max(0.0));
            }
            if o.action_timer >= cycle {
                o.action_timer = 0.0;
                o.set_var("alertHalf", 0.0);
            }
            if near_flat > SIGHT || !level {
                set_state(o, hum::IDLE_LOOK, false);
            } else if o.var("alerts") <= 0.0 || (o.var("alerts") < p.alerts && near_flat < 250.0 * S) {
                set_state(o, hum::CHASE, false);
                o.set_var("speed", p.chase);
                o.set_var("alertHalf", 0.0);
            }
        }
        hum::CHASE => {
            if (o.timer as i64) & 0xF == 9 || o.var("steer") == 0.0 {
                o.set_var("steer", 1.0);
                o.set_var("ideal", to_player);
            }
            let ideal = o.var("ideal");
            turn(o, ideal, p.turn);
            let sp = speed_var.max(p.chase);
            walk(o, &world,sp);
            let o = &mut m.objects[id];
            // After landing a hit (`func_802DB440`) the baddie loses interest and turns away.
            if contact.touched || (invincible && near_flat < o.hitbox.radius + 120.0) {
                let base = o.move_yaw;
                random_heading(o, base, 160.0, 200.0);
                set_state(o, hum::IDLE, false);
                o.set_var("speed", 0.0);
                o.set_var("pause", 90.0);
            } else if near_flat > SIGHT * 1.3 || !level {
                set_state(o, hum::IDLE, false);
                o.set_var("speed", 0.0);
            }
        }
        _ => set_state(o, hum::IDLE, false),
    }
}

// ---------------------------------------------------------------------------------------------
// Ticker (termite)
// ---------------------------------------------------------------------------------------------

fn termite(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let contact = take_contact(&mut m.objects[id]);
    if contact.blow != Blow::None {
        let o = &mut m.objects[id];
        let hp = o.var("hp") - if contact.blow == Blow::Strong { 2.0 } else { 1.0 };
        o.set_var("hp", hp);
        if hp <= 0.0 {
            finish(id, m);
            return;
        }
        set_state(o, ant::OW, true);
        o.forward_vel = 0.0;
    }
    let o = &mut m.objects[id];
    o.action_timer += 1.0;
    match o.action {
        ant::OW => {
            // `__chTermite_ow`: back to idle at 0.95 of the 0.7 s animation
            if o.action_timer >= ticks(0.7 * 0.95) {
                set_state(o, ant::IDLE, false);
            }
        }
        ant::IDLE => {
            o.forward_vel = 0.0;
            // subaddie_maybe_set_state_position_direction(..., WALK, 0.06): leave idle about 6% of the frames
            if rand(o) < 0.06 {
                set_state(o, ant::WALK, false);
                let s = rand2(o, 5.0, 12.0);
                o.set_var("speed", s);
                let spin = rand2(o, -3.1, 3.1);
                o.set_var("spin", spin + if spin >= 0.0 { 3.1 } else { -3.1 });
            }
        }
        _ => {
            // wander: a random turn rate, re-rolled now and then; heads home beyond 800 units
            o.move_yaw += deg(o.var("spin"));
            if (o.timer as i64) & 0x1F == 0xE && rand(o) > 0.45 {
                let spin = rand2(o, -3.1, 3.1);
                o.set_var("spin", spin + if spin >= 0.0 { 3.1 } else { -3.1 });
            }
            if (o.timer as i64) & 0x1F == 3 && rand(o) > 0.58 {
                let s = rand2(o, 5.0, 12.0);
                o.set_var("speed", s);
            }
            if flat_distance(&o.pos, &o.home) > 800.0 * S {
                let home_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
                o.turn_toward_angle(home_yaw, deg(8.0));
            }
            // anim duration follows the speed: ml_map_f(speed, 5, 12, 0.54, 0.36)
            let sp = o.var("speed");
            o.set_var("animDur", 0.54 + (sp - 5.0) / 7.0 * (0.36 - 0.54));
            let sp = o.var("speed");
            if rand(o) < 0.047 {
                set_state(o, ant::IDLE, false);
            }
            let o = &mut m.objects[id];
            walk(o, &world,sp);
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    // harmless, solid, attackable
    set_box(o, Kind::Termite, INTERACT_BREAKABLE, 0.0);
}

// ---------------------------------------------------------------------------------------------
// Bigbutt (`chbull_update`)
// ---------------------------------------------------------------------------------------------

fn bigbutt(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let state = m.objects[id].action;
    let down = matches!(state, bull::FALL | bull::DOWN | bull::GET_UP);
    if contact.blow != Blow::None && !down {
        let o = &mut m.objects[id];
        if contact.blow == Blow::Strong {
            // chbigbutt_die: it falls over for four seconds
            set_state(o, bull::FALL, true);
            o.forward_vel = 0.0;
            o.set_var("speed", 0.0);
        } else {
            set_state(o, bull::OW, true);
            o.forward_vel = 0.0;
            o.set_var("speed", 0.0);
        }
    }
    let o = &mut m.objects[id];
    let to_player = o.angle_to_mario;
    let near = flat_distance(&o.pos, &mpos);
    let level = (mpos[1] - o.pos[1]).abs() < 700.0;
    let aggro = near < 1200.0 * S && level;
    o.action_timer += 1.0;
    let tick = o.timer as i64;
    match o.action {
        bull::EAT => {
            o.forward_vel = 0.0;
            o.set_var("snorts", 3.0);
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if aggro {
                set_state(o, bull::WALK_TO_PLAYER, false);
            }
            if o.action_timer > ticks(5.5 * 0.65) && rand(o) < 0.58 * 0.05 {
                set_state(o, bull::WALK, false);
                let s = rand2(o, 3.5, 4.9);
                o.set_var("speed", s);
                let base = o.move_yaw;
                random_heading(o, base, 10.0, 45.0);
            }
        }
        bull::WALK | bull::RUN => {
            let ideal = o.var("ideal");
            turn(o, ideal, if o.action == bull::RUN { 3.0 } else { 2.0 });
            if tick & 0xF == 0 {
                let base = o.var("ideal");
                random_heading(o, base, 10.0, 20.0);
            }
            if tick & 0x7 == 0 && o.action == bull::WALK && rand(o) < 0.02 {
                set_state(o, bull::EAT, false);
            } else if tick & 0xF == 0 && o.action == bull::WALK && rand(o) < 0.13 {
                set_state(o, bull::RUN, false);
                let s = rand2(o, 7.1, 8.4);
                o.set_var("speed", s);
            } else if tick & 0xF == 0 && o.action == bull::RUN && rand(o) < 0.08 {
                set_state(o, bull::WALK, false);
                let s = rand2(o, 3.5, 4.9);
                o.set_var("speed", s);
            }
            if flat_distance(&o.pos, &o.home) > 1100.0 * S {
                let home_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
                o.set_var("ideal", home_yaw);
            }
            let sp = o.var("speed");
            let hit_wall_before = o.hit_wall;
            walk(o, &world,sp);
            let o = &mut m.objects[id];
            if o.hit_wall && !hit_wall_before {
                let base = o.move_yaw;
                random_heading(o, base, 120.0, 180.0);
            }
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if aggro {
                set_state(o, bull::WALK_TO_PLAYER, false);
            }
        }
        bull::WALK_TO_PLAYER => {
            o.forward_vel = 0.0;
            turn(o, to_player, 4.0);
            if !aggro {
                set_state(o, bull::EAT, false);
            } else if yaw_diff(o, to_player) < 5.0 {
                set_state(o, bull::ALERT, false);
                o.set_var("snorts", 3.0);
            }
        }
        bull::ALERT => {
            o.forward_vel = 0.0;
            turn(o, to_player, 4.0);
            // chBullAnimations[6].duration - (3 - snorts) * 0.1085 seconds per snort
            let dur = 0.6 - (3.0 - o.var("snorts")) * 0.1085;
            o.set_var("animDur", dur);
            if o.action_timer >= ticks(dur) {
                o.action_timer = 0.0;
                let n = o.var("snorts") - 1.0;
                o.set_var("snorts", n);
            }
            if !aggro {
                set_state(o, bull::EAT, false);
            } else if o.var("snorts") <= 0.0 || (o.var("snorts") < 3.0 && near < 300.0 * S) {
                set_state(o, bull::CHARGE, false);
                o.set_var("speed", 13.0);
            }
        }
        bull::CHARGE => {
            let sp = (o.var("speed") + 0.15).min(30.0);
            o.set_var("speed", sp);
            turn(o, to_player, 9.0);
            let before = o.hit_wall;
            walk(o, &world,sp);
            let o = &mut m.objects[id];
            if o.hit_wall && !before {
                // lost the player against a wall: back to grazing (func_802C5F94)
                set_state(o, bull::WALK, false);
                o.set_var("speed", 4.0);
                o.set_var("pause", 150.0);
                let base = o.move_yaw;
                random_heading(o, base, 135.0, 175.0);
            } else if near < 320.0 * S && yaw_diff(o, to_player) < 40.0 {
                // slide into the player, speed +5.7
                set_state(o, bull::SLIDE, false);
                o.set_var("speed", sp + 5.7);
            }
        }
        bull::SLIDE => {
            // decelerate (func_80329878: 0.8 close to the player, 1.2 otherwise)
            let sp = o.var("speed");
            let decel = if near < 250.0 * S { 0.8 } else { 1.2 };
            let sp = (sp - decel * 0.5).max(0.0);
            o.set_var("speed", sp);
            if o.action_timer < ticks(0.9) * 0.5 {
                turn(o, to_player, 1.0);
            }
            walk(o, &world,sp);
            let o = &mut m.objects[id];
            // the horns connect: `func_802C60AC` -> ATTACK
            if contact.touched && sp >= 5.0 {
                set_state(o, bull::ATTACK, true);
                o.forward_vel = 0.0;
                o.set_var("speed", 0.0);
            } else if sp <= 0.0 {
                set_state(o, bull::EAT, false);
                o.set_var("pause", 60.0);
            }
        }
        bull::ATTACK => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(1.1 * 0.95) {
                set_state(o, bull::WALK, false);
                o.set_var("speed", 4.0);
                o.set_var("pause", 150.0);
                let base = o.move_yaw;
                random_heading(o, base, 135.0, 175.0);
            }
        }
        bull::OW => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(0.7 * 0.95) {
                set_state(o, bull::EAT, false);
            }
        }
        bull::FALL => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(1.1 * 0.95) {
                set_state(o, bull::DOWN, true);
            }
        }
        bull::DOWN => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(4.0) {
                set_state(o, bull::GET_UP, true);
            }
        }
        bull::GET_UP => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(1.0 * 0.95) {
                set_state(o, bull::EAT, false);
            }
        }
        _ => set_state(o, bull::EAT, false),
    }
    let o = &mut m.objects[id];
    // The horns hurt while it charges, slides and gores; otherwise it is a solid, attackable body.
    let sp = o.var("speed");
    let dangerous = o.action == bull::CHARGE || o.action == bull::ATTACK || (o.action == bull::SLIDE && sp >= 5.0);
    if dangerous {
        set_box(o, Kind::Bigbutt, INTERACT_DAMAGE, 2.0);
    } else {
        set_box(o, Kind::Bigbutt, INTERACT_BREAKABLE, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Snippet and Mutie Snippet (`chCrab_update`)
// ---------------------------------------------------------------------------------------------

fn snippet(id: ObjId, m: &mut MarioRuntime, kind: Kind) {
    let world = m.world.clone();
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let state = m.objects[id].action;
    let upside = matches!(state, crab::UPSIDE_DOWN | crab::FLIP_OVER);
    if contact.blow != Blow::None {
        let o = &mut m.objects[id];
        if upside || contact.blow == Blow::Strong || state == crab::FLIP_UPRIGHT {
            // `__chCrab_die`
            finish(id, m);
            return;
        }
        // `__chCrab_ow`: flip onto its back for three seconds
        set_state(o, crab::FLIP_OVER, true);
        o.forward_vel = 0.0;
        o.set_var("flip", ticks(3.0));
    }
    let o = &mut m.objects[id];
    let to_player = o.angle_to_mario;
    let near = flat_distance(&o.pos, &mpos);
    let level = (mpos[1] - o.pos[1]).abs() < 600.0;
    o.action_timer += 1.0;
    let tick = o.timer as i64;
    match o.action {
        crab::IDLE => {
            o.forward_vel = 0.0;
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if near < 1500.0 * S && level {
                set_state(o, crab::TURN_TO_PLAYER, false);
            }
            if rand(o) < 0.03 {
                set_state(o, crab::WANDER, false);
                let s = rand2(o, 1.5, 2.3);
                o.set_var("speed", s);
                let base = o.move_yaw;
                random_heading(o, base, 0.0, 60.0);
            }
        }
        crab::WANDER => {
            let ideal = o.var("ideal");
            turn(o, ideal, 1.0);
            let sp = o.var("speed");
            walk(o, &world,sp);
            let o = &mut m.objects[id];
            if o.hit_wall {
                let base = o.move_yaw;
                random_heading(o, base, 90.0, 150.0);
            }
            if rand(o) < 0.0075 {
                set_state(o, crab::IDLE, false);
            }
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if near < 1500.0 * S && level {
                set_state(o, crab::TURN_TO_PLAYER, false);
            }
        }
        crab::TURN_TO_PLAYER => {
            o.forward_vel = 0.0;
            turn(o, to_player, 4.0);
            if yaw_diff(o, to_player) < 5.0 {
                set_state(o, crab::CHASE, false);
                o.set_var("speed", 12.0);
            }
        }
        crab::CHASE => {
            if tick & 0xF == 9 {
                o.set_var("ideal", to_player);
            }
            let ideal = o.var("ideal");
            turn(o, ideal, 7.0);
            walk(o, &world,12.0);
            let o = &mut m.objects[id];
            // `__chCrab_touch`: it scuttles off after pinching the player
            if contact.touched || o.hit_wall {
                set_state(o, crab::WANDER, false);
                o.set_var("speed", 2.0);
                o.set_var("pause", 150.0);
                let base = o.move_yaw;
                random_heading(o, base, 135.0, 175.0);
            }
        }
        crab::FLIP_OVER => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(0.8) {
                set_state(o, crab::UPSIDE_DOWN, false);
            }
        }
        crab::UPSIDE_DOWN => {
            o.forward_vel = 0.0;
            let left = o.var("flip") - 1.0;
            o.set_var("flip", left);
            if left <= 0.0 {
                set_state(o, crab::FLIP_UPRIGHT, true);
            }
        }
        crab::FLIP_UPRIGHT => {
            o.forward_vel = 0.0;
            if o.action_timer >= ticks(0.55) {
                set_state(o, crab::IDLE, false);
            }
        }
        _ => set_state(o, crab::IDLE, false),
    }
    let o = &mut m.objects[id];
    let helpless = matches!(o.action, crab::UPSIDE_DOWN | crab::FLIP_OVER | crab::FLIP_UPRIGHT);
    if helpless {
        set_box(o, kind, INTERACT_BREAKABLE, 0.0);
    } else {
        set_box(o, kind, INTERACT_DAMAGE, kind.spec().damage);
    }
}

// ---------------------------------------------------------------------------------------------
// Sir Slush and his snowballs (`chSnowman_update`, `chSnowball_update`)
// ---------------------------------------------------------------------------------------------

fn sir_slush(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    if contact.blow != Blow::None {
        // `__chSnowman_deathCallback`
        finish(id, m);
        return;
    }
    let mvel = [m.vel[0], m.vel[2]];
    let swimming = m.pos[1] < m.world.water_level(m.pos[0], m.pos[2]);
    let o = &mut m.objects[id];
    let to_player = o.angle_to_mario;
    let near = flat_distance(&o.pos, &mpos);
    o.action_timer += 1.0;
    // track the player (`__chSnowman_setYawTarget`, 6 degrees)
    turn(o, to_player, 6.0);
    let in_range = near > 500.0 * S && near < 2750.0 * S && (mpos[1] - o.pos[1]).abs() < 900.0 && !swimming;
    match o.action {
        slush::IDLE => {
            let cool = o.var("cool");
            if cool > 0.0 {
                o.set_var("cool", cool - 1.0);
            } else if in_range && yaw_diff(o, to_player) < 3.0 {
                set_state(o, slush::ATTACK, true);
                o.set_var("thrown", 0.0);
            }
        }
        _ => {
            let phase = o.action_timer / ticks(4.0);
            if phase > 0.98 || !in_range {
                set_state(o, slush::IDLE, false);
                o.set_var("cool", ticks(0.4));
            } else if phase >= 0.58 && o.var("thrown") == 0.0 {
                o.set_var("thrown", 1.0);
                let from = [o.pos[0] + sins(o.move_yaw) * 80.0 * S, o.pos[1] + 0.55 * 486.0 * S * o.scale, o.pos[2] + coss(o.move_yaw) * 80.0 * S];
                let lead = if (o.timer as i64) & 1 == 1 { 21.0 } else { 43.0 };
                let target = [mpos[0] + mvel[0] * lead * 0.5, mpos[1] + 1.0, mpos[2] + mvel[1] * lead * 0.5];
                launch_snowball(m, from, target);
            } else if phase < 0.45 {
                turn(o, to_player, 6.0);
            }
        }
    }
    let o = &mut m.objects[id];
    // Sir Slush himself is a solid, attackable body; his snowballs do the damage.
    set_box(o, Kind::SirSlush, INTERACT_DAMAGE, 1.0);
}

/// `__chSnowman_spawnSnowball` + the init of `chSnowball_update`: a lob that lands on `target` after 32 frames.
pub fn launch_snowball(m: &mut MarioRuntime, from: Vec3, target: Vec3) {
    let mut spec = ObjectSpec::new(ObjectBehaviorKind::BkEnemy, from, 0.0);
    spec.param = Some((ACTOR_SNOWBALL | (100u32 << 16)) as f64);
    // recycle a spent snowball before growing the arena
    let slot = m.objects.objects.iter().position(|o| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == Kind::Snowball && !o.active);
    let id = match slot {
        Some(i) => ObjId(i as u32),
        None => m.objects.spawn(&spec),
    };
    let o = &mut m.objects[id];
    o.active = true;
    o.visible = true;
    o.pos = from;
    o.home = from;
    o.vel = [(target[0] - from[0]) / 32.0, (target[1] - from[1]) / 32.0 + 32.0 * S, (target[2] - from[2]) / 32.0];
    o.set_var("life", ticks(6.0));
    o.set_var("armed", 0.0);
    o.set_var("spin", 0.0);
    set_box(o, Kind::Snowball, INTERACT_DAMAGE, 1.0);
    o.interact_status = 0;
}

fn snowball(id: ObjId, m: &mut MarioRuntime) {
    let contact = take_contact(&mut m.objects[id]);
    let prev = m.objects[id].pos;
    let o = &mut m.objects[id];
    let shatter = contact.touched || contact.blow != Blow::None;
    o.pos[0] += o.vel[0];
    o.pos[1] += o.vel[1];
    o.pos[2] += o.vel[2];
    o.vel[1] -= 2.0 * S;
    o.set_var("spin", o.var("spin") + 10.0);
    let armed = o.var("armed") + 1.0;
    o.set_var("armed", armed);
    let life = o.var("life") - 1.0;
    o.set_var("life", life);
    let pos = o.pos;
    let mut hit = shatter || life <= 0.0;
    if armed >= 6.0 && !hit {
        let floor = m.world.find_floor(pos[0], pos[1].max(prev[1]) + 40.0, pos[2]);
        if floor.surface.is_some() && pos[1] <= floor.height + 20.0 * S {
            hit = true;
        }
    }
    if hit {
        m.objects.events.push(ObjectEvent::Break { pos, wall: None });
        let o = &mut m.objects[id];
        o.deactivate(-1.0);
        return;
    }
    let o = &mut m.objects[id];
    set_box(o, Kind::Snowball, INTERACT_DAMAGE, 1.0);
}

// ---------------------------------------------------------------------------------------------
// Bat (`chbat_update`)
// ---------------------------------------------------------------------------------------------

fn bat_flight(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let state = m.objects[id].action;
    if contact.blow != Blow::None && !matches!(state, bat::FALL | bat::DIE) {
        let o = &mut m.objects[id];
        // chBat_fall: tumbles down with velocity 300 per second
        set_state(o, bat::FALL, false);
        o.vel = [0.0; 3];
        o.forward_vel = 0.0;
    }
    let o = &mut m.objects[id];
    o.action_timer += 1.0;
    let to_player = o.angle_to_mario;
    let dy = (mpos[1] + 50.0) - o.pos[1];
    let near = flat_distance(&o.pos, &mpos);
    let cool = o.var("cool");
    if cool > 0.0 {
        o.set_var("cool", cool - 1.0);
    }
    match o.action {
        bat::ROOST => {
            o.vel = [0.0; 3];
            o.pos = o.home;
            // chbat_nearPlayer: 800 units and below the roost
            if o.var("cool") <= 0.0 && near < 800.0 * S && mpos[1] + 50.0 <= o.home[1] - 40.0 {
                set_state(o, bat::EXIT_ROOST, true);
            }
        }
        bat::EXIT_ROOST => {
            o.pos[1] -= 3.0;
            if o.action_timer >= ticks(0.3) {
                set_state(o, bat::CHASE, false);
            }
        }
        bat::CHASE => {
            turn(o, to_player, 6.0);
            let speed = 5.0 * S * 1.6;
            let vy = clamp(dy * 0.1, -4.0, 4.0);
            o.pos[0] += sins(o.move_yaw) * speed;
            o.pos[2] += coss(o.move_yaw) * speed;
            o.pos[1] += vy;
            if contact.touched || o.action_timer > ticks(6.0) {
                // after biting it flaps away for a while (cooldown) and roams
                set_state(o, bat::ROAM, false);
                o.set_var("cool", ticks(4.0));
                let away = to_s16(to_player + 0x8000 as f64) as f64;
                o.set_var("ideal", away);
            } else if near > 1800.0 * S {
                set_state(o, bat::FLY_HOME, false);
            }
        }
        bat::ROAM => {
            let ideal = o.var("ideal");
            turn(o, ideal, 4.0);
            let speed = 5.0 * S * 1.2;
            o.pos[0] += sins(o.move_yaw) * speed;
            o.pos[2] += coss(o.move_yaw) * speed;
            o.pos[1] += clamp((o.home[1] - 250.0 - o.pos[1]) * 0.05, -3.0, 3.0);
            if o.action_timer > ticks(2.5) {
                set_state(o, bat::FLY_HOME, false);
            }
        }
        bat::FLY_HOME => {
            let home_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
            turn(o, home_yaw, 5.0);
            let d = flat_distance(&o.pos, &o.home);
            let speed = (5.0 * S * 1.2).min(d);
            o.pos[0] += sins(o.move_yaw) * speed;
            o.pos[2] += coss(o.move_yaw) * speed;
            o.pos[1] += clamp(o.home[1] - o.pos[1], -3.0, 3.0);
            if d < 20.0 && (o.pos[1] - o.home[1]).abs() < 4.0 {
                set_state(o, bat::ENTER_ROOST, true);
            }
            if o.var("cool") <= 0.0 && near < 800.0 * S && mpos[1] + 50.0 <= o.home[1] - 40.0 {
                set_state(o, bat::CHASE, false);
            }
        }
        bat::ENTER_ROOST => {
            o.pos = o.home;
            if o.action_timer >= ticks(0.2) {
                set_state(o, bat::ROOST, false);
            }
        }
        bat::FALL => {
            // velocity[0] -= 2.5 per frame while falling; lands, then dies
            o.vel[1] -= 2.5 * S;
            o.pos[1] += o.vel[1];
            let floor = m.world.find_floor(o.pos[0], o.pos[1] + 100.0, o.pos[2]);
            if (floor.surface.is_some() && o.pos[1] <= floor.height) || o.action_timer > ticks(2.0) {
                if floor.surface.is_some() {
                    o.pos[1] = floor.height;
                }
                set_state(o, bat::DIE, true);
            }
        }
        bat::DIE => {
            if o.action_timer >= ticks(0.85) {
                finish(id, m);
                return;
            }
        }
        _ => set_state(o, bat::ROOST, false),
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    let state = o.action;
    match state {
        bat::ROOST | bat::FALL | bat::DIE | bat::ENTER_ROOST => set_box(o, Kind::Bat, INTERACT_BREAKABLE, 0.0),
        _ => set_box(o, Kind::Bat, INTERACT_DAMAGE, 1.0),
    }
}

// ---------------------------------------------------------------------------------------------
// Shrapnel, the spiked water mine (`chshrapnel_update`)
// ---------------------------------------------------------------------------------------------

fn shrapnel(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let state = m.objects[id].action;
    if state != mine::EXPLODE && (contact.blow != Blow::None || contact.touched) {
        // chShrapnel_explode: the blast hurts whoever is close (the touch itself already did)
        let o = &mut m.objects[id];
        set_state(o, mine::EXPLODE, true);
        o.forward_vel = 0.0;
        let pos = o.pos;
        let radius = 260.0;
        m.objects.events.push(ObjectEvent::Explosion { pos, radius, mine: false });
        let d = dist_3d(&pos, &m.pos);
        if contact.blow != Blow::None && d < radius + 40.0 {
            m.take_damage(1.0, Some(pos));
        }
    }
    let o = &mut m.objects[id];
    o.action_timer += 1.0;
    let phase = (o.timer * 0.1) % (2.0 * std::f64::consts::PI);
    let water = m.world.water_level(o.pos[0], o.pos[2]);
    let surface = if water.is_finite() { water - 15.0 } else { o.home[1] };
    o.pos[1] = surface - phase.sin() * 10.0;
    let to_player = o.angle_to_mario;
    let near = flat_distance(&o.pos, &mpos);
    let level = (mpos[1] - surface).abs() < 250.0 * S;
    match o.action {
        mine::IDLE => {
            o.forward_vel = 0.0;
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if near < 600.0 * S && level {
                set_state(o, mine::TURN, false);
            }
        }
        mine::TURN => {
            turn(o, to_player, 4.0);
            if yaw_diff(o, to_player) < 5.0 && level {
                set_state(o, mine::CHASE, false);
                o.set_var("speed", 4.0);
            } else if !level || near > 900.0 * S {
                set_state(o, mine::IDLE, false);
            }
        }
        mine::CHASE => {
            turn(o, to_player, o.var("speed") / 2.0);
            let sp = (o.var("speed") + 1.0 / TICKS).min(50.0);
            o.set_var("speed", sp);
            o.pos[0] += sins(o.move_yaw) * sp * S;
            o.pos[2] += coss(o.move_yaw) * sp * S;
            if !level || near > 900.0 * S {
                set_state(o, mine::IDLE, false);
                o.set_var("pause", 30.0);
            }
        }
        _ => {
            if o.action_timer >= ticks(0.5) {
                finish(id, m);
                return;
            }
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    if o.action == mine::EXPLODE {
        set_box(o, Kind::Shrapnel, INTERACT_BREAKABLE, 0.0);
    } else {
        set_box(o, Kind::Shrapnel, INTERACT_DAMAGE, 1.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Snare-bear (`chSnarebear_update`): a rooted trap that snaps at Banjo within 680 units
// ---------------------------------------------------------------------------------------------

fn snarebear(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let _ = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    o.action_timer += 1.0;
    let near = flat_distance(&o.pos, &mpos);
    let level = (mpos[1] - o.pos[1]).abs() < 400.0;
    match o.action {
        bear::IDLE => {
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if near < 680.0 * S && level {
                set_state(o, bear::ATTACK, true);
            }
        }
        _ => {
            if o.action_timer >= ticks(1.2) * 0.999 {
                set_state(o, bear::IDLE, false);
                o.set_var("pause", 15.0);
            }
        }
    }
    // The jaws are closing between 25% and 75% of the 1.2 s attack.
    let phase = o.action_timer / ticks(1.2);
    let o = &mut m.objects[id];
    if o.action == bear::ATTACK && (0.25..0.75).contains(&phase) {
        set_box(o, Kind::Snarebear, INTERACT_DAMAGE, 1.0);
        o.hitbox.radius *= 1.6;
    } else {
        set_box(o, Kind::Snarebear, INTERACT_BREAKABLE, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Beehive (`chBeehive_update`): breaks open and gives a honeycomb piece
// ---------------------------------------------------------------------------------------------

fn beehive(id: ObjId, m: &mut MarioRuntime) {
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    match o.action {
        hive::IDLE => {
            if contact.blow != Blow::None {
                // chBeehive_die
                set_state(o, hive::DIE, true);
                set_box(o, Kind::Beehive, 0, 0.0);
                let pos = o.pos;
                m.objects.events.push(ObjectEvent::Break { pos, wall: None });
                // the honeycomb piece: one wedge for whoever broke it
                m.heal_counter += 4.0;
                m.objects[id].set_var("died", 1.0);
            }
        }
        _ => {
            o.action_timer += 1.0;
            if o.action_timer >= ticks(0.5) {
                o.deactivate(-1.0);
            }
        }
    }
    let o = &mut m.objects[id];
    if o.action == hive::IDLE {
        set_box(o, Kind::Beehive, INTERACT_BREAKABLE, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Clucker (`chClucker_update`, Treasure Trove Cove) and Grille Chompa (`chGrilleChompa_update`, Rusty Bucket Bay): the rooted biters.
// Both run the same state machine: idle (folded away, untouchable) -> alert when Banjo comes within `alert` -> after a pause, wind up
// (0.2 s) when he is within `near` -> bite -> alert again; back to idle beyond `far`. The bite is the only time they hurt or can be hurt.
// ---------------------------------------------------------------------------------------------

/// (alert, near, far) distances in BK units (`this->scale * 800/600/1000` for Clucker, `400/300/500` for the Chompa).
fn plant_ranges(kind: Kind) -> (f64, f64, f64) {
    match kind {
        Kind::Clucker => (800.0, 600.0, 1000.0),
        _ => (400.0, 300.0, 500.0),
    }
}

/// Seconds the bite animation takes and the share of it in which the head is out (`MARKER_*_A` -> `_B` at 0.58 / 0.63 / 0.59).
fn plant_bite(kind: Kind, long: bool) -> f64 {
    match (kind, long) {
        (Kind::Clucker, false) => 0.63,
        (Kind::Clucker, true) => 0.58,
        _ => 0.59,
    }
}

fn plant(id: ObjId, m: &mut MarioRuntime, kind: Kind) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    let (alert, near, far) = plant_ranges(kind);
    let k = S * o.scale;
    let dist = dist_3d(&o.pos, &mpos);
    o.action_timer += 1.0;
    let biting = matches!(o.action, plant::ATTACK | plant::ATTACK_LONG);
    // chClucker_die / func_80356C24: any strong blow while the head is out; weak ones only sound
    if biting && contact.blow == Blow::Strong {
        set_state(o, plant::DIE, true);
        o.set_var("animDur", 2.0);
    }
    let hold = |o: &mut SimObject, state: i32| {
        // folded pose: the bite clip at its end (`skeletalAnim_setProgress(0.99)` + stopped)
        set_state(o, state, true);
        o.set_var("animDur", 0.05);
    };
    match o.action {
        plant::IDLE => {
            if dist < alert * k {
                hold(o, plant::ALERT);
                o.set_var("pause", 0.0);
                o.set_var("cycle", 255.0);
            }
        }
        plant::ALERT => {
            if o.var("pause") > 0.0 {
                o.set_var("pause", o.var("pause") - 1.0);
            } else if dist < near * k {
                hold(o, plant::WINDUP);
            } else if dist > far * k {
                hold(o, plant::IDLE);
            }
        }
        plant::WINDUP => {
            if o.action_timer >= ticks(0.2) {
                let cycle = (o.var("cycle") + 1.0) % 3.0;
                o.set_var("cycle", cycle);
                let long = kind == Kind::Clucker && cycle >= 2.0;
                let secs = if kind == Kind::Clucker && !long { rand2(o, 0.6, 1.0) } else { rand2(o, 2.3, 2.7) };
                set_state(o, if long { plant::ATTACK_LONG } else { plant::ATTACK }, true);
                o.set_var("animDur", secs);
                o.set_var("dur", secs);
                o.set_var("lng", if long { 1.0 } else { 0.0 });
            }
        }
        plant::ATTACK | plant::ATTACK_LONG => {
            if o.action_timer >= ticks(o.var("dur")) {
                // back to alert with the pause of `chClucker_setNextState`: 0.5..0.75 s after the long bite, 0.25..0.35 s after a short one
                let pause = match kind {
                    Kind::Clucker if o.var("cycle") >= 2.0 => rand2(o, 0.5, 0.75),
                    Kind::Clucker => rand2(o, 0.25, 0.35),
                    _ => rand2(o, 0.5, 1.0),
                };
                hold(o, plant::ALERT);
                o.set_var("pause", ticks(pause));
            }
        }
        plant::DIE => {
            if o.action_timer >= ticks(1.0) {
                finish(id, m);
                return;
            }
        }
        _ => {}
    }
    let o = &mut m.objects[id];
    let out = matches!(o.action, plant::ATTACK | plant::ATTACK_LONG) && o.action_timer < ticks(o.var("dur")) * plant_bite(kind, o.action == plant::ATTACK_LONG);
    if out {
        set_box(o, kind, INTERACT_DAMAGE, 1.0);
    } else {
        set_box(o, kind, 0, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Red Flibbit (`chFlibbit_update`, Bubblegloop Swamp): hops 180 units at a time toward Banjo while he is inside its leash, back to its
// spawn when he is not. `func_80329210` (the point is inside the enemy-boundary volume of the setup file, category 7) is approximated
// by a disc around the spawn.
// ---------------------------------------------------------------------------------------------

/// Hop length in BK units and the disc around the spawn in which it will follow Banjo.
const FLIBBIT_HOP: f64 = 180.0;
const FLIBBIT_LEASH: f64 = 900.0;

fn flibbit(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let world = m.world.clone();
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    let k = S * o.scale;
    o.action_timer += 1.0;
    let alive = o.action < flib::DIE;
    if alive && contact.blow == Blow::Strong {
        // chFlibbit_die
        set_state(o, flib::DIE, true);
    } else if alive && contact.blow == Blow::Weak {
        set_state(o, flib::OW, true);
        o.set_var("wait", ticks(1.0));
    } else if alive && contact.touched && o.action != flib::OW {
        // chFlibbit_attack: it bumped Banjo and stands still for a second
        set_state(o, flib::BUMP, false);
        o.set_var("wait", ticks(1.0));
    }
    let to_player = o.angle_to_mario;
    let player_in_leash = flat_distance(&o.home, &mpos) < FLIBBIT_LEASH * k;
    let at_home = flat_distance(&o.pos, &o.home) < 30.0;
    match o.action {
        flib::IDLE => {
            if player_in_leash {
                set_state(o, flib::TURN, false);
            } else if o.distance_to_mario < 2000.0 * k {
                turn(o, to_player, 3.0);
            }
            if !at_home && !player_in_leash {
                flibbit_hop(o, None, &world);
            }
        }
        flib::TURN => {
            let toward = if player_in_leash { Some(to_player) } else { None };
            match toward {
                Some(a) => {
                    turn(o, a, 3.0);
                    // hops when it faces Banjo (side angle within 0.7 rad) half the time it checks
                    if yaw_diff(o, a) < 40.0 && rand(o) > 0.5 {
                        flibbit_hop(o, Some(a), &world);
                    }
                }
                None => {
                    if at_home {
                        set_state(o, flib::IDLE, false);
                    } else {
                        let home_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
                        turn(o, home_yaw, 4.0);
                        if yaw_diff(o, home_yaw) < 40.0 {
                            flibbit_hop(o, None, &world);
                        }
                    }
                }
            }
        }
        flib::HOP => {
            let t = o.action_timer / ticks(0.8);
            if (0.2..=0.8).contains(&t) {
                let f = (t - 0.2) / 0.6;
                let (sx, sz, tx, tz) = (o.var("sx"), o.var("sz"), o.var("tx"), o.var("tz"));
                o.pos[0] = sx + (tx - sx) * f;
                o.pos[2] = sz + (tz - sz) * f;
                let floor = world.find_floor(o.pos[0], o.pos[1] + 300.0, o.pos[2]);
                let ground = if floor.surface.is_some() { floor.height } else { o.pos[1] };
                // the chase hop arcs 70 units (the sky of `local->unk1`), the homing one stays low
                let arc = if o.var("chase") != 0.0 { 70.0 * (1.0 - ((t - 0.5) / 0.3).abs().min(1.0)) } else { 0.0 };
                o.pos[1] = ground + arc;
            }
            if o.action_timer >= ticks(0.8) {
                let floor = world.find_floor(o.pos[0], o.pos[1] + 300.0, o.pos[2]);
                if floor.surface.is_some() {
                    o.pos[1] = floor.height;
                }
                if flat_distance(&o.pos, &o.home) < 30.0 && !player_in_leash {
                    set_state(o, flib::IDLE, false);
                } else {
                    set_state(o, flib::TURN, false);
                }
            }
        }
        flib::BUMP | flib::OW => {
            let w = o.var("wait") - 1.0;
            o.set_var("wait", w);
            if w <= 0.0 {
                set_state(o, flib::TURN, false);
            }
        }
        flib::DIE => {
            if o.action_timer >= ticks(0.4) {
                finish(id, m);
                return;
            }
        }
        _ => {}
    }
    let o = &mut m.objects[id];
    if o.action < flib::DIE {
        set_box(o, Kind::Flibbit, INTERACT_DAMAGE, 1.0);
    } else {
        set_box(o, Kind::Flibbit, 0, 0.0);
    }
}

/// Start a hop of `FLIBBIT_HOP` units along `heading` (toward Banjo) or toward the spawn when `heading` is None; refuses to leave the floor.
fn flibbit_hop(o: &mut SimObject, heading: Option<f64>, world: &CollisionWorld) {
    let yaw = heading.unwrap_or_else(|| atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64);
    let mut reach = FLIBBIT_HOP * S * o.scale;
    if heading.is_none() {
        reach = reach.min(flat_distance(&o.pos, &o.home));
    }
    let (tx, tz) = (o.pos[0] + sins(yaw) * reach, o.pos[2] + coss(yaw) * reach);
    let floor = world.find_floor(tx, o.pos[1] + 300.0, tz);
    if floor.surface.is_none() || (floor.height - o.pos[1]).abs() > 300.0 {
        return;
    }
    o.set_var("sx", o.pos[0]);
    o.set_var("sz", o.pos[2]);
    o.set_var("tx", tx);
    o.set_var("tz", tz);
    o.set_var("chase", if heading.is_some() { 1.0 } else { 0.0 });
    o.move_yaw = yaw;
    o.face_yaw = yaw;
    set_state(o, flib::HOP, true);
    o.set_var("animDur", 0.8);
}

// ---------------------------------------------------------------------------------------------
// Buzzbomb (`func_802CCC5C`, Bubblegloop Swamp): hovers over its spawn, drifts to random spots within 300 units and 100..300 above the
// floor, and dives at Banjo (three dives, then climbs away) when he is within 800 units and in its leash disc.
// ---------------------------------------------------------------------------------------------

fn buzzbomb(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let world = m.world.clone();
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    let k = S * o.scale;
    if o.var("floor") == 0.0 {
        // `mapModel_getFloorY` under the spawn, or the water surface when that is higher
        let f = world.find_floor(o.pos[0], o.pos[1] + 100.0, o.pos[2]);
        let mut y = if f.surface.is_some() { f.height } else { o.pos[1] - 200.0 };
        let w = world.water_level(o.pos[0], o.pos[2]);
        if w.is_finite() && w > y {
            y = w;
        }
        o.set_var("floor", 1.0);
        o.set_var("floorY", y);
        o.pos[1] = y + 200.0 * k;
        o.set_var("bx", o.pos[0]);
        o.set_var("by", o.pos[1]);
        o.set_var("bz", o.pos[2]);
        o.home[1] = y;
    }
    o.action_timer += 1.0;
    let alive = o.action < buzz::DIE;
    if alive && contact.blow == Blow::Strong {
        set_state(o, buzz::DIE, false);
        o.set_var("vy", 1000.0 / TICKS * S);
    } else if alive && contact.blow == Blow::Weak && o.action < buzz::OW {
        set_state(o, buzz::OW, false);
        o.set_var("timer", ticks(2.0));
    }
    let floor_y = o.var("floorY");
    let dist = flat_distance(&o.pos, &mpos);
    let in_leash = flat_distance(&o.home, &mpos) < 1200.0 * k;
    let bob = (o.timer * 0.12).sin() * 4.0;
    match o.action {
        buzz::IDLE => {
            if o.var("pause") == 0.0 {
                let t = rand2(o, 0.0, 2.0);
                o.set_var("pause", ticks(t) + 1.0);
            }
            o.set_var("pause", o.var("pause") - 1.0);
            o.pos[1] += (bob - o.var("bob")) * 0.5;
            o.set_var("bob", bob);
            let player_ok = in_leash && mpos[1] < floor_y + 300.0 * k && mpos[1] > world.water_level(mpos[0], mpos[2]) - 120.0;
            if player_ok && dist < 800.0 * k {
                o.set_var("pause", 0.0);
                o.set_var("dives", 0.0);
                buzz_aim(o, [mpos[0], mpos[1] + 90.0, mpos[2]], 0.5);
                set_state(o, buzz::DIVE, false);
            } else if o.var("pause") <= 0.0 {
                o.set_var("pause", 0.0);
                let (hx, hz) = (o.var("bx"), o.var("bz"));
                let tx = hx + rand2(o, -300.0, 300.0) * k;
                let tz = hz + rand2(o, -300.0, 300.0) * k;
                let ty = floor_y + rand2(o, 100.0, 300.0) * k;
                buzz_aim(o, [tx, ty, tz], 1.0);
                set_state(o, buzz::MOVE, false);
            }
        }
        buzz::MOVE | buzz::DIVE => {
            let t = (o.action_timer / ticks(o.var("span"))).min(1.0);
            let e = t * t * (3.0 - 2.0 * t);
            for i in 0..3 {
                o.pos[i] = o.var(["sx", "sy", "sz"][i]) + (o.var(["tx", "ty", "tz"][i]) - o.var(["sx", "sy", "sz"][i])) * e;
            }
            let yaw = atan2s(o.var("tx") - o.var("sx"), o.var("tz") - o.var("sz")) as f64;
            o.move_yaw = yaw;
            if t >= 1.0 {
                if o.action == buzz::DIVE {
                    let n = o.var("dives") + 1.0;
                    o.set_var("dives", n);
                    if n >= 3.0 || !in_leash {
                        o.set_var("timer", ticks(3.5));
                        set_state(o, buzz::RECOVER, false);
                    } else {
                        buzz_aim(o, [mpos[0], mpos[1] + 90.0, mpos[2]], 0.5);
                        set_state(o, buzz::DIVE, false);
                    }
                } else {
                    set_state(o, buzz::IDLE, false);
                }
            }
        }
        buzz::RECOVER | buzz::OW => {
            o.pos[1] += 50.0 / TICKS * S;
            o.move_yaw = to_s16(o.move_yaw + deg(10.0 / TICKS)) as f64;
            let t = o.var("timer") - 1.0;
            o.set_var("timer", t);
            if t <= 0.0 {
                let (bx, by, bz) = (o.var("bx"), o.var("by"), o.var("bz"));
                buzz_aim(o, [bx, by, bz], 1.0);
                set_state(o, buzz::MOVE, false);
            }
        }
        buzz::DIE => {
            // thrown away from Banjo, then falls and lands (clip 0x111) before fading
            let ground = world.find_floor(o.pos[0], o.pos[1] + 50.0, o.pos[2]);
            let ground_y = if ground.surface.is_some() { ground.height } else { floor_y };
            let vy = o.var("vy") - 3000.0 / (TICKS * TICKS) * S;
            o.set_var("vy", vy);
            let away = atan2s(o.pos[0] - mpos[0], o.pos[2] - mpos[2]) as f64;
            o.pos[0] += sins(away) * 200.0 / TICKS * S;
            o.pos[2] += coss(away) * 200.0 / TICKS * S;
            o.pos[1] = (o.pos[1] + vy).max(ground_y);
            if o.pos[1] <= ground_y {
                set_state(o, buzz::FADE, true);
            }
        }
        _ => {
            if o.action_timer >= ticks(2.0) {
                finish(id, m);
                return;
            }
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    if o.action < buzz::DIE {
        set_box(o, Kind::Buzzbomb, INTERACT_DAMAGE, 1.0);
    } else {
        set_box(o, Kind::Buzzbomb, 0, 0.0);
    }
}

fn buzz_aim(o: &mut SimObject, to: Vec3, seconds: f64) {
    for (i, key) in ["sx", "sy", "sz"].iter().enumerate() {
        o.set_var(key, o.pos[i]);
    }
    for (i, key) in ["tx", "ty", "tz"].iter().enumerate() {
        o.set_var(key, to[i]);
    }
    o.set_var("span", seconds);
}

// ---------------------------------------------------------------------------------------------
// Tee-Hee (`chTeehee_update`, Mad Monster Mansion): a ghost that floats where it was placed; Banjo within 600 units wakes it (1.5 s
// alert), then it drifts after him, matching his height, and hurts on touch. Any blow kills it (`chTeehee_die`); it flies through walls.
// ---------------------------------------------------------------------------------------------

fn teehee(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    let k = S * o.scale;
    o.action_timer += 1.0;
    if o.action < ghost::DIE && contact.blow != Blow::None {
        set_state(o, ghost::DIE, true);
    }
    let dist = dist_3d(&o.pos, &mpos);
    match o.action {
        ghost::IDLE => {
            let bob = (o.timer * 0.1).sin() * 3.0;
            o.pos[1] += (bob - o.var("bob")) * 0.5;
            o.set_var("bob", bob);
            if dist < 600.0 * k {
                set_state(o, ghost::ALERT, true);
            }
        }
        ghost::ALERT => {
            let to = o.angle_to_mario;
            turn(o, to, 6.0);
            if o.action_timer >= ticks(1.5) {
                set_state(o, ghost::CHASE, false);
            }
        }
        ghost::CHASE => {
            let to = o.angle_to_mario;
            turn(o, to, 4.0);
            let step = 6.0 * S;
            let to_y = mpos[1] - o.pos[1];
            o.pos[0] += sins(o.move_yaw) * step;
            o.pos[2] += coss(o.move_yaw) * step;
            o.pos[1] += to_y.clamp(-2.0 * k, 2.0 * k);
            if dist > 1800.0 * k {
                set_state(o, ghost::IDLE, false);
            }
        }
        _ => {
            if o.action_timer >= ticks(1.0) {
                finish(id, m);
                return;
            }
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    if o.action < ghost::DIE {
        set_box(o, Kind::TeeHee, INTERACT_DAMAGE, 1.0);
    } else {
        set_box(o, Kind::TeeHee, 0, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Nipper (`__chNipper_updateFunc`, Treasure Trove Cove): the giant crab. Banjo in front of it within 1300 units makes it pinch twice
// (the pinch hurts); it then opens up for `lifetime_value` frames (120, 80, then 40 of them) in which a strong blow wounds it
// (`__chNipper_dieFunc`: 120 -> 80 -> 40 -> dead). Blows outside that window do nothing.
// ---------------------------------------------------------------------------------------------

fn nipper(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    if o.var("life") == 0.0 {
        o.set_var("life", 120.0);
    }
    o.action_timer += 1.0;
    // the crab sees Banjo when he is within 1300 units and within 35 degrees of its facing (`__func_80388088`)
    let in_front = yaw_diff(o, o.angle_to_mario) < 35.0;
    let sees = dist_3d(&o.pos, &mpos) < 1300.0 && in_front;
    let life = o.var("life");
    match o.action {
        nip::IDLE => {
            if sees {
                o.set_var("pinches", 0.0);
                set_state(o, nip::ATTACK, false);
                o.set_var("animDur", if life >= 120.0 { 1.2 } else if life >= 80.0 { 1.05 } else { 0.9 });
            }
        }
        nip::ATTACK => {
            let dur = o.var("animDur");
            if o.action_timer >= ticks(dur) * 2.0 {
                set_state(o, nip::VULNERABLE, false);
            }
        }
        nip::VULNERABLE => {
            if !sees {
                set_state(o, nip::IDLE, false);
            } else if o.action_timer >= life {
                o.set_var("animDur", if life >= 120.0 { 1.2 } else if life >= 80.0 { 1.05 } else { 0.9 });
                set_state(o, nip::ATTACK, false);
                o.set_var("animDur", if life >= 120.0 { 1.2 } else if life >= 80.0 { 1.05 } else { 0.9 });
            } else if contact.blow == Blow::Strong {
                let left = life - 40.0;
                o.set_var("life", left);
                if left <= 0.0 {
                    set_state(o, nip::DYING, true);
                } else {
                    set_state(o, nip::HIT, true);
                }
            }
        }
        nip::HIT => {
            if o.action_timer >= ticks(1.3) {
                set_state(o, nip::ATTACK, false);
                let life = o.var("life");
                o.set_var("animDur", if life >= 120.0 { 1.2 } else if life >= 80.0 { 1.05 } else { 0.9 });
            }
        }
        nip::DYING => {
            if o.action_timer >= ticks(3.0) {
                set_state(o, nip::DEAD, true);
            }
        }
        _ => {
            if o.action_timer >= ticks(2.0) {
                finish(id, m);
                return;
            }
        }
    }
    let o = &mut m.objects[id];
    // the pinch hurts; the open shell can be hit; otherwise it is just a wall
    match o.action {
        nip::ATTACK => set_box(o, Kind::Nipper, INTERACT_DAMAGE, 1.0),
        nip::VULNERABLE => set_box(o, Kind::Nipper, INTERACT_BREAKABLE, 0.0),
        _ => set_box(o, Kind::Nipper, 0, 0.0),
    }
}

// ---------------------------------------------------------------------------------------------
// Slappa (`chSlappa_update`, Gobi's Valley): a giant hand buried in the sand. Banjo within 800 units makes it rise (0.8 s), then it chases
// him (while he is within 1100), winds up (20 frames) when he is within 175 and slaps (0.8 s, the slam hurts), lies on the sand for 75
// frames and gets up again. Out of reach it sinks back. A strong blow while it is up kills it (`func_8038AFF4`).
// ---------------------------------------------------------------------------------------------

fn slappa(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    let k = S * o.scale;
    o.action_timer += 1.0;
    let dist = dist_3d(&o.pos, &mpos);
    let up = (slap::RISE..=slap::GETTING_UP).contains(&o.action);
    if up && contact.blow == Blow::Strong {
        set_state(o, slap::DIE, true);
    }
    let to = o.angle_to_mario;
    match o.action {
        slap::IDLE => {
            if dist < 800.0 * k {
                o.move_yaw = to;
                set_state(o, slap::RISE, true);
            }
        }
        slap::RISE => {
            turn(o, to, 8.0);
            if o.action_timer >= ticks(0.8) {
                set_state(o, slap::CHASE, false);
            }
        }
        slap::CHASE => {
            if dist < 175.0 * k {
                set_state(o, slap::WINDUP, false);
            } else if dist > 1100.0 * k {
                set_state(o, slap::LOWER, true);
            } else {
                turn(o, to, 8.0);
                o.pos[0] += sins(o.move_yaw) * 16.0 * S;
                o.pos[2] += coss(o.move_yaw) * 16.0 * S;
            }
        }
        slap::WINDUP => {
            turn(o, to, 8.0);
            if o.action_timer >= 20.0 {
                set_state(o, slap::ATTACK, true);
            }
        }
        slap::ATTACK => {
            if o.action_timer >= ticks(0.8) {
                set_state(o, slap::LAYING, true);
                o.set_var("animDur", 0.05);
            }
        }
        slap::LAYING => {
            if o.action_timer >= 75.0 {
                set_state(o, slap::GETTING_UP, true);
            }
        }
        slap::GETTING_UP => {
            if o.action_timer >= ticks(1.2) {
                set_state(o, slap::CHASE, false);
            }
        }
        slap::LOWER => {
            if o.action_timer >= ticks(1.4) {
                set_state(o, slap::IDLE, false);
            }
        }
        _ => {
            if o.action_timer >= ticks(1.4) {
                finish(id, m);
                return;
            }
        }
    }
    let o = &mut m.objects[id];
    o.face_yaw = o.move_yaw;
    match o.action {
        slap::CHASE | slap::WINDUP => set_box(o, Kind::Slappa, INTERACT_DAMAGE, 1.0),
        // the slam: hurts only in the second half of the 0.8 s swing
        slap::ATTACK if o.action_timer > ticks(0.8) * 0.6 => set_box(o, Kind::Slappa, INTERACT_DAMAGE, 2.0),
        slap::RISE | slap::ATTACK | slap::LAYING | slap::GETTING_UP => set_box(o, Kind::Slappa, INTERACT_BREAKABLE, 0.0),
        _ => set_box(o, Kind::Slappa, 0, 0.0),
    }
}

// ---------------------------------------------------------------------------------------------
// Mumbo (`chMumbo_update`, the skulls): stands on his spot, turns to face Banjo and does his spell (clip 0x6D, 7.5 s) whenever Banjo's
// transformation changes. The transformation itself is `banjo_ext::tend_mumbo`.
// ---------------------------------------------------------------------------------------------

fn mumbo_npc(id: ObjId, m: &mut MarioRuntime) {
    let form = m.banjo.form.id() as f64;
    let o = &mut m.objects[id];
    let _ = take_contact(o);
    o.action_timer += 1.0;
    let first = o.var("form") == 0.0;
    if first {
        o.set_var("form", form);
    }
    let to = o.angle_to_mario;
    turn(o, to, 3.0);
    if !first && o.var("form") != form {
        o.set_var("form", form);
        set_state(o, mumbo::SPELL, true);
        o.set_var("animDur", 2.5);
    } else if o.action == mumbo::SPELL && o.action_timer >= ticks(2.5) {
        set_state(o, mumbo::IDLE, false);
    }
    set_box(o, Kind::Mumbo, 0, 0.0);
}

// ---------------------------------------------------------------------------------------------
// Tanktup (`chTanktup_update`, Bubblegloop Swamp): the giant tank-on-legs. In the game its four legs are separate actors
// (`ACTOR_E9..EC`) that Banjo knocks off one by one, after which the body opens its mouth for the Jiggy. Here the legs are four hit
// points on the body: each strong blow wounds it (`ow`), the fourth opens the mouth for good.
// ---------------------------------------------------------------------------------------------

fn tanktup(id: ObjId, m: &mut MarioRuntime) {
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    o.action_timer += 1.0;
    if o.var("legs") == 0.0 {
        o.set_var("legs", 4.0);
    }
    match o.action {
        tank::IDLE => {
            if contact.blow == Blow::Strong {
                let left = o.var("legs") - 1.0;
                o.set_var("legs", left);
                if left <= 0.0 {
                    set_state(o, tank::OPEN, true);
                } else {
                    set_state(o, tank::HIT, true);
                }
            }
        }
        tank::HIT => {
            if o.action_timer >= ticks(1.75) {
                set_state(o, tank::IDLE, false);
            }
        }
        _ => {}
    }
    let o = &mut m.objects[id];
    if o.action == tank::OPEN {
        set_box(o, Kind::Tanktup, 0, 0.0);
        if o.var("paid") == 0.0 {
            o.set_var("paid", 1.0);
            o.set_var("died", 1.0);
        }
    } else {
        set_box(o, Kind::Tanktup, INTERACT_BREAKABLE, 0.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Grabba (`chGrabba_update`, Gobi's Valley): the hand that holds a Jiggy. It rises when Banjo is 1380..1560 units away, stays up while he
// is near (counting 4 per frame within 600 units, 1 with Turbo Trainers, hiding at 12: only the trainers give enough time) and sinks
// back. A strong blow while it is up beats it and it drops its Jiggy (`died` -> the host drops it).
// ---------------------------------------------------------------------------------------------

fn grabba(id: ObjId, m: &mut MarioRuntime) {
    let mpos = m.pos;
    let turbo = m.banjo.turbo_frames > 0.0;
    let contact = take_contact(&mut m.objects[id]);
    let o = &mut m.objects[id];
    o.action_timer += 1.0;
    let dist = dist_3d(&o.pos, &mpos);
    let k = S * o.scale;
    let up = matches!(o.action, grab::UP | grab::RISE);
    if up && contact.blow == Blow::Strong {
        set_state(o, grab::BEATEN, true);
        o.set_var("died", 1.0);
    }
    match o.action {
        grab::WAIT => {
            if dist > 1380.0 * k && dist < 1560.0 * k {
                set_state(o, grab::RISE, true);
                o.set_var("count", 0.0);
            }
        }
        grab::RISE => {
            if o.action_timer >= ticks(1.6) * 0.69 {
                set_state(o, grab::UP, false);
            }
        }
        grab::UP => {
            if o.var("count") >= 12.0 {
                set_state(o, grab::HIDE, true);
            } else if dist < 600.0 * k {
                o.set_var("count", o.var("count") + if turbo { 1.0 } else { 4.0 });
            }
        }
        grab::HIDE => {
            if o.action_timer >= ticks(0.55) * 0.89 {
                set_state(o, grab::WAIT, false);
            }
        }
        _ => {
            if o.action_timer >= ticks(0.8) {
                finish(id, m);
                return;
            }
        }
    }
    let o = &mut m.objects[id];
    match o.action {
        grab::UP | grab::RISE => set_box(o, Kind::Grabba, INTERACT_BREAKABLE, 0.0),
        _ => set_box(o, Kind::Grabba, 0, 0.0),
    }
}

/// How many wedges of damage `kind` deals on contact right now (tests and HUD hints).
pub fn contact_damage(o: &SimObject) -> f64 {
    if o.hitbox.interact_type & INTERACT_DAMAGE != 0 {
        o.hitbox.damage_or_coin_value
    } else {
        0.0
    }
}

