//! Super Smash Bros. 64 special moves for the twelve fighters that play as skins on Mario's controller
//! (`native/src/characters/smash.rs`). Engine-free and deterministic, at the sim's 30 Hz, in SM64 units.
//!
//! While a fighter is selected (`MarioRuntime::smash.fighter`), the special button (`R_TRIG`) starts a special:
//! neutral, up (stick up or A held) or down (stick down or Z held). A running special owns Mario for its ticks
//! ([`step_smash`] consumes the frame like `step_banjo` does): it moves him with the sim's own ground / air steps
//! (walls, floors, ceilings and ledge grabs stay SM64's), spawns projectiles, emits hitboxes and sound cues. Anything
//! that changes Mario's action from outside (a knockback, a death, a warp) cancels it.
//!
//! Numbers come from the decomp (`reference/ssb64`, `src/ft/ftchar/ft<name>/*.h`, `src/wp/wpdef.h`, `wpvars.h`,
//! `src/it/itvars.h`) and are converted with [`K`] (SSB64 world units -> SM64 units: Smash Mario is 460 units tall,
//! SM64 Mario 160), [`KV`] (per 60 Hz frame -> per 30 Hz tick speeds) and [`KA`] (accelerations). Rising moves that
//! SSB64 drives by root motion (Super Jump Punch, Falcon Dive, Final Cutter, Falcon Kick, Pound) follow the root
//! translation of their clips, measured from the user's import once (peak height / distance / frame), not read at run
//! time. Damage is in SSB64 percent ([`SmashHit::damage`]); each game mode scales it to its own health units.
//!
//! Hits: hitboxes ([`Hitbox`], rebuilt every tick) and projectiles ([`Projectile`]) are resolved against any
//! [`SmashTargets`] (SM64 objects here in [`after_step`]; the app's walking goombas and the minigames' enemies through
//! their own adapters). Each source hits a target once (or once per `rehit` ticks for multi-hit moves).
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::{AIR_GRABBED_LEDGE, AIR_LANDED, GROUND_STEP_LEFT_GROUND_I};
use crate::math::*;
use crate::object::*;
use crate::smash_hitbox::{Element, Knockback, Motion, MotionEvents};
use crate::state::*;
use std::sync::Arc;

/// SSB64 world units -> SM64 units (Smash Mario 460 tall -> SM64 Mario 160).
pub const K: f64 = 0.348;
/// SSB64 speed (units per 60 Hz frame) -> SM64 units per 30 Hz tick.
pub const KV: f64 = 2.0 * K;
/// SSB64 acceleration (units per frame^2) -> SM64 units per tick^2.
pub const KA: f64 = 4.0 * K;
/// Mario's own air gravity and terminal speed (the helpless fall uses them, so recoveries read like his jumps).
pub const GRAVITY: f64 = 4.0;
pub const TERMINAL: f64 = -75.0;
/// Stick deflection (raw, of 64) that picks the up / down special.
pub const STICK_SPECIAL: f64 = 40.0;

/// SSB64 animation frames (60 Hz) -> sim ticks.
pub fn ticks(frames: f64) -> u32 {
    (frames / 2.0).round().max(1.0) as u32
}

// ---------------------------------------------------------------------------------------------
// Fighters and moves
// ---------------------------------------------------------------------------------------------

/// The twelve playable fighters (the app's `smash::Fighter`, same order).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SmashFighter {
    Mario,
    Luigi,
    Donkey,
    Link,
    Samus,
    Yoshi,
    Kirby,
    Fox,
    Pikachu,
    Purin,
    Captain,
    Ness,
}

impl SmashFighter {
    pub const ALL: [SmashFighter; 12] = [
        SmashFighter::Mario,
        SmashFighter::Luigi,
        SmashFighter::Donkey,
        SmashFighter::Link,
        SmashFighter::Samus,
        SmashFighter::Yoshi,
        SmashFighter::Kirby,
        SmashFighter::Fox,
        SmashFighter::Pikachu,
        SmashFighter::Purin,
        SmashFighter::Captain,
        SmashFighter::Ness,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    /// The decomp's name (`ft<name>`, the `Copy<Name>` prefix of Kirby's copied motions).
    pub fn decomp_name(self) -> &'static str {
        match self {
            SmashFighter::Mario => "Mario",
            SmashFighter::Luigi => "Luigi",
            SmashFighter::Donkey => "Donkey",
            SmashFighter::Link => "Link",
            SmashFighter::Samus => "Samus",
            SmashFighter::Yoshi => "Yoshi",
            SmashFighter::Kirby => "Kirby",
            SmashFighter::Fox => "Fox",
            SmashFighter::Pikachu => "Pikachu",
            SmashFighter::Purin => "Purin",
            SmashFighter::Captain => "Captain",
            SmashFighter::Ness => "Ness",
        }
    }

    /// The move in a slot.
    pub fn special(self, slot: SpecialSlot) -> SpecialMove {
        use SpecialMove::*;
        use SpecialSlot::*;
        match (self, slot) {
            (SmashFighter::Mario | SmashFighter::Luigi, Neutral) => Fireball,
            (SmashFighter::Mario | SmashFighter::Luigi, Up) => SuperJumpPunch,
            (SmashFighter::Mario | SmashFighter::Luigi, Down) => Tornado,
            (SmashFighter::Donkey, Neutral) => GiantPunch,
            (SmashFighter::Donkey, Up) => SpinningKong,
            (SmashFighter::Donkey, Down) => HandSlap,
            (SmashFighter::Link, Neutral) => Boomerang,
            (SmashFighter::Link, Up) => SpinAttack,
            (SmashFighter::Link, Down) => LinkBomb,
            (SmashFighter::Samus, Neutral) => ChargeShot,
            (SmashFighter::Samus, Up) => ScrewAttack,
            (SmashFighter::Samus, Down) => SamusBomb,
            (SmashFighter::Yoshi, Neutral) => EggLay,
            (SmashFighter::Yoshi, Up) => EggThrow,
            (SmashFighter::Yoshi, Down) => YoshiBomb,
            (SmashFighter::Kirby, Neutral) => Inhale,
            (SmashFighter::Kirby, Up) => FinalCutter,
            (SmashFighter::Kirby, Down) => Stone,
            (SmashFighter::Fox, Neutral) => Blaster,
            (SmashFighter::Fox, Up) => FireFox,
            (SmashFighter::Fox, Down) => Reflector,
            (SmashFighter::Pikachu, Neutral) => ThunderJolt,
            (SmashFighter::Pikachu, Up) => QuickAttack,
            (SmashFighter::Pikachu, Down) => Thunder,
            (SmashFighter::Purin, Neutral) => Pound,
            (SmashFighter::Purin, Up) => Sing,
            (SmashFighter::Purin, Down) => Rest,
            (SmashFighter::Captain, Neutral) => FalconPunch,
            (SmashFighter::Captain, Up) => FalconDive,
            (SmashFighter::Captain, Down) => FalconKick,
            (SmashFighter::Ness, Neutral) => PkFire,
            (SmashFighter::Ness, Up) => PkThunder,
            (SmashFighter::Ness, Down) => PsiMagnet,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SpecialSlot {
    Neutral,
    Up,
    Down,
}

/// Every special of the roster (Mario and Luigi share theirs; the fighter tells them apart).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SpecialMove {
    Fireball,
    SuperJumpPunch,
    Tornado,
    GiantPunch,
    SpinningKong,
    HandSlap,
    Boomerang,
    SpinAttack,
    LinkBomb,
    ChargeShot,
    ScrewAttack,
    SamusBomb,
    EggLay,
    EggThrow,
    YoshiBomb,
    Inhale,
    FinalCutter,
    Stone,
    Blaster,
    FireFox,
    Reflector,
    ThunderJolt,
    QuickAttack,
    Thunder,
    Pound,
    Sing,
    Rest,
    FalconPunch,
    FalconDive,
    FalconKick,
    PkFire,
    PkThunder,
    PsiMagnet,
    /// Not a special: a held / thrown item's hit (`items`).
    Item,
}

/// Where a special is in its course (each move uses the ones it needs).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Phase {
    /// Wind-up.
    Start,
    /// Charging (Giant Punch, Charge Shot, Fire Fox's hold) / holding (Reflector, PSI Magnet, Hand Slap's loop).
    Hold,
    /// The move proper.
    Attack,
    /// A second stage (Quick Attack's second zip, Final Cutter's drop, PK Thunder 2, Yoshi Bomb's slam).
    Travel,
    /// Something caught (Kirby's mouthful, Yoshi's egg, Falcon Dive's grab).
    Caught,
    /// Recovery animation.
    End,
    /// Helpless fall after a recovery (SSB64 `FallSpecial`): drift only, until landing or a ledge.
    Fall,
    /// Landing lag (`LandingFallSpecial`, Yoshi Bomb / Stone impact).
    Landing,
    /// Jigglypuff's Rest.
    Sleep,
}

/// What a hit does beyond damage.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HitKind {
    Normal,
    Fire,
    Electric,
    Explosion,
    /// Jigglypuff's Sing: puts the target to sleep, no damage.
    Sleep,
    /// Kirby's inhale / Yoshi's tongue: the target is swallowed (removed) if the mode allows it.
    Capture,
    /// Falcon Dive's grab.
    Grab,
}

/// What became of a target that was hit (the adapter's answer).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitOutcome {
    /// Immune or not affected.
    Miss,
    Hit,
    Killed,
    /// Swallowed (Kirby / Yoshi): gone from the field; `Some` when it was a fighter Kirby can copy.
    Captured(Option<SmashFighter>),
}

/// One hit, in the frame of the world (SM64 units).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SmashHit {
    /// SSB64 percent.
    pub damage: f32,
    pub kind: HitKind,
    /// Unit knockback direction (away from the source, tilted up).
    pub dir: Vec3,
    /// Knockback strength, SM64 units per tick of launch speed (a mode may ignore it).
    pub knock: f64,
    /// Where the hit landed.
    pub pos: Vec3,
    pub fighter: SmashFighter,
    /// The special it came from; `None` for a normal attack (jab, aerial, dash attack... see [`normal_attack_motion`]).
    pub mv: Option<SpecialMove>,
    /// The SSB64 knockback inputs of the hit: the scripted hitbox's, a weapon's `WPAttributes`, or an approximation
    /// (`smash_knockback`); `resolve` always fills it (`None` only from hand-made hits).
    pub knockback: Option<Knockback>,
    /// The SSB64 motion id that hit (scripted hitboxes only).
    pub motion: Option<u32>,
}

/// A body a special can hit: feet position, radius and height (a vertical cylinder), a key stable across ticks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Target {
    pub key: u64,
    pub pos: Vec3,
    pub radius: f64,
    pub height: f64,
    /// A Smash fighter (Kirby copies its neutral special when he swallows it).
    pub fighter: Option<SmashFighter>,
}

/// An enemy projectile Fox's Reflector can send back or Ness's PSI Magnet can absorb.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Incoming {
    pub id: u64,
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f64,
    /// Energy shots (lasers, fire, PK) heal Ness when absorbed; physical ones do not.
    pub energy: bool,
}

/// What a game mode exposes to the specials.
pub trait SmashTargets {
    /// Every hittable body this tick.
    fn targets(&self) -> Vec<Target>;
    /// Apply a hit to `targets()[i]` (same tick, same order).
    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome;
    /// Enemy projectiles in flight.
    fn incoming(&self) -> Vec<Incoming> {
        vec![]
    }
    /// The Reflector sent this projectile back with velocity `vel` (now the player's).
    fn reflect(&mut self, _id: u64, _vel: Vec3) {}
    /// The PSI Magnet absorbed this projectile.
    fn absorb(&mut self, _id: u64) {}
}

/// Sound moments, for the sound layer (`smash_sfx(fighter, ..)`). One cue per moment, never per frame.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SfxPhase {
    /// The special starts (voice / wind-up).
    Start,
    /// A charge step (Giant Punch, Charge Shot levels, Fire Fox's flames).
    Charge,
    /// Fully charged.
    Full,
    /// The attack comes out (the punch, the shot, the lunge, the launch).
    Release,
    /// A projectile left (fireball, laser, boomerang, bolt...).
    Projectile,
    /// One of the move's hits landed.
    Hit,
    /// A projectile or bomb exploded.
    Explode,
    /// Something reflected / absorbed / swallowed / copied.
    Special,
    /// The move hits the ground (Yoshi Bomb, Stone, Final Cutter, Falcon Kick landing).
    Land,
    /// The move ends (Rest wakes up, Stone reverts, the boomerang is caught).
    End,
}

/// A sound cue: `fighter` does `mv` (from `slot`), at `phase`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SmashSfx {
    pub fighter: SmashFighter,
    pub mv: SpecialMove,
    pub slot: SpecialSlot,
    pub phase: SfxPhase,
}

/// What the view and the sound layer hear about (drained by the app every tick).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SmashEvent {
    Sfx(SmashSfx),
    /// A hit landed at `pos`.
    Hit { pos: Vec3, kind: HitKind, damage: f32 },
    /// A blast (bomb, egg, Thunder, PK Thunder 2...) at `pos`.
    Explode { pos: Vec3, radius: f64 },
    Reflected { pos: Vec3 },
    Absorbed { pos: Vec3 },
    /// Kirby swallowed something (and copied a fighter's neutral special).
    Swallowed { copied: Option<SmashFighter> },
}

// ---------------------------------------------------------------------------------------------
// Projectiles and hitboxes
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ProjKind {
    /// Mario's: bounces along the floor.
    Fireball,
    /// Luigi's: straight, no gravity.
    GreenFireball,
    /// Samus's, `level` 0..7.
    ChargeShot,
    Boomerang,
    LinkBomb,
    SamusBomb,
    /// Yoshi's thrown egg.
    Egg,
    /// Kirby's spat star / Yoshi Bomb's landing stars.
    Star,
    Laser,
    /// Final Cutter's shock wave.
    Cutter,
    /// Pikachu's Thunder Jolt: crawls along the floor.
    ThunderJolt,
    /// Pikachu's Thunder: falls from the sky.
    Thunderbolt,
    PkFireSpark,
    PkFirePillar,
    PkThunder,
    /// A short-lived blast (bombs, eggs, Thunder's burst, Falcon Dive's throw).
    Blast,
    /// An item's hit volume (thrown item, sliding shell, armed bomb): moved by `items`, not stepped here.
    Item,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Projectile {
    pub id: u32,
    pub kind: ProjKind,
    pub fighter: SmashFighter,
    pub mv: SpecialMove,
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: u32,
    pub life: u32,
    pub radius: f64,
    pub damage: f32,
    pub hit: HitKind,
    pub gravity: f64,
    pub terminal: f64,
    /// Ticks between two hits on the same target (0: one hit, then the projectile is spent unless it `pierces`).
    pub rehit: u32,
    pub pierce: bool,
    /// Charge Shot level, boomerang stage (0 out, 1 back), bomb armed flag...
    pub level: u8,
    pub dead: bool,
}

/// One attack volume of the running special, for this tick only: a sphere.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hitbox {
    /// Hit identity: a target is hit once per source (or every `rehit` ticks).
    pub source: u32,
    pub pos: Vec3,
    pub radius: f64,
    pub damage: f32,
    pub kind: HitKind,
    pub knock: f64,
    pub rehit: u32,
    /// From the motion event scripts (`smash_hitbox`): the SSB64 knockback inputs and the motion id.
    pub knockback: Option<Knockback>,
    pub motion: Option<u32>,
    /// A normal attack's hitbox (not a special's).
    pub normal: bool,
}

/// The running special.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Special {
    /// Whose body plays it (Kirby for his copied moves).
    pub fighter: SmashFighter,
    pub mv: SpecialMove,
    pub slot: SpecialSlot,
    /// Kirby playing a copied neutral special (its `Copy<Name>` motions).
    pub copied: Option<SmashFighter>,
    pub phase: Phase,
    /// Ticks in the phase (0 on its first tick).
    pub t: u32,
    /// Ticks since the special started.
    pub total: u32,
    /// Airborne variant (`SpecialAir*` clips).
    pub air: bool,
    /// Facing (s16 yaw) and a second direction (Fire Fox / Quick Attack travel: yaw, pitch in radians).
    pub yaw: f64,
    pub dir: [f64; 2],
    pub charge: u32,
    /// Hit identity of this special (projectiles carry their own ids).
    pub instance: u32,
    /// Mario's action as the special left it: anything else next tick means it was interrupted.
    pub sm64_action: u32,
    /// Landing lag after a helpless fall (ticks).
    pub land_lag: u32,
    /// Helpless-fall drift (fraction of Mario's air speed).
    pub drift: f64,
    /// A hit of this special landed (grabs, Thunder's self-hit...).
    pub landed: bool,
    fresh: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SmashExt {
    /// The selected fighter (`None`: Mario's own moves, nothing here runs).
    pub fighter: Option<SmashFighter>,
    pub special: Option<Special>,
    pub projectiles: Vec<Projectile>,
    /// This tick's attack volumes.
    pub hitboxes: Vec<Hitbox>,
    /// Cues since the app last drained them (`characters::smash_fx` takes them every frame).
    pub events: Vec<SmashEvent>,
    /// Donkey Kong's stored Giant Punch charge (0..10) and Samus's stored Charge Shot level (0..7).
    pub dk_charge: u32,
    pub samus_charge: u32,
    /// Kirby's copied neutral special.
    pub copy: Option<SmashFighter>,
    /// Kirby's mouthful: `Some(fighter?)` while he holds something.
    pub mouth: Option<Option<SmashFighter>>,
    /// Yoshi's egg throw hop is spent until he lands.
    pub egg_hop_used: bool,
    /// Health to give back (PSI Magnet), in SM64 health units.
    pub heal: f64,
    /// Mario's centre and facing this tick (for reflection / absorption outside the step).
    pub center: Vec3,
    pub yaw: f64,
    pub tick: u64,
    next_id: u32,
    /// The motion event scripts (hitboxes) this runtime uses; `None`: the registered table ([`crate::smash_hitbox::register`]),
    /// and without one the specials keep their approximate hitboxes.
    pub scripts: Option<Arc<MotionEvents>>,
    /// The normal attack playing (its motion and start tick), for its scripted hitboxes.
    attack: Option<NormalAttack>,
    /// (source, target key, tick of the last hit).
    hit_log: Vec<(u32, u64, u64)>,
    /// Hits of the running special that landed since its last tick: (source, outcome).
    landed: Vec<(u32, HitKind, HitOutcome)>,
    /// The SSB64 items (`items`).
    pub items: items::Items,
}

impl SmashExt {
    /// Kirby's Stone and Jigglypuff's Rest... nothing hurts the fighter.
    pub fn armor(&self) -> bool {
        self.special.as_ref().is_some_and(|s| s.mv == SpecialMove::Stone && matches!(s.phase, Phase::Hold | Phase::Travel | Phase::Landing)) || self.items.star > 0
    }
    pub fn reflecting(&self) -> bool {
        self.special.as_ref().is_some_and(|s| s.mv == SpecialMove::Reflector && matches!(s.phase, Phase::Start | Phase::Hold))
    }
    pub fn absorbing(&self) -> bool {
        self.special.as_ref().is_some_and(|s| s.mv == SpecialMove::PsiMagnet && matches!(s.phase, Phase::Start | Phase::Hold))
    }
    pub fn asleep(&self) -> bool {
        self.special.as_ref().is_some_and(|s| s.mv == SpecialMove::Rest && s.phase == Phase::Sleep)
    }
    /// In a helpless fall (no specials, no moves until landing).
    pub fn helpless(&self) -> bool {
        self.special.as_ref().is_some_and(|s| s.phase == Phase::Fall)
    }
    pub fn active(&self) -> bool {
        self.special.is_some()
    }

    /// The decomp motion names that play the running special, best first (the app resolves them through the import's
    /// motion table), the seconds into that motion, and whether it loops.
    pub fn motion(&self) -> Option<(Vec<String>, f32, bool)> {
        self.special.as_ref().map(special_motion).or_else(|| self.items.motion(false))
    }

    fn new_id(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    /// Drop everything (the fighter changed, the course reloaded).
    pub fn reset(&mut self) {
        let (fighter, scripts) = (self.fighter, self.scripts.take());
        let enabled = self.items.enabled;
        *self = SmashExt { fighter, scripts, ..Default::default() };
        self.items.enabled = enabled;
    }

    /// The motion event scripts in use: this runtime's own, else the registered table.
    pub fn motion_events(&self) -> Option<Arc<MotionEvents>> {
        self.scripts.clone().or_else(crate::smash_hitbox::registered)
    }

    /// Clear the per-tick outputs. Events stay until the app drains them (`std::mem::take(&mut smash.events)`), capped so
    /// a headless run without a reader does not grow them.
    pub fn begin_tick(&mut self) {
        self.hitboxes.clear();
        if self.events.len() > 512 {
            self.events.drain(..256);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Animation names
// ---------------------------------------------------------------------------------------------

pub fn special_motion(sp: &Special) -> (Vec<String>, f32, bool) {
    use Phase as P;
    use SpecialMove::*;
    let a = sp.air;
    let air = |ground: &'static str, air: &'static str| -> Vec<&'static str> { if a { vec![air, ground] } else { vec![ground, air] } };
    let secs = sp.t as f32 / 30.0;
    let mut looped = false;
    let mut since_start = false;
    let names: Vec<&str> = match (sp.mv, sp.phase) {
        (Fireball | Blaster | ThunderJolt | PkFire, _) => {
            since_start = true;
            air("SpecialN", "SpecialAirN")
        }
        (SuperJumpPunch, P::Fall | P::Landing) | (SpinningKong, P::Fall | P::Landing) | (SpinAttack, P::Fall | P::Landing) | (ScrewAttack, P::Fall | P::Landing)
        | (QuickAttack, P::Fall | P::Landing) | (FalconDive, P::Fall | P::Landing) | (FireFox, P::Fall | P::Landing) | (PkThunder, P::Fall | P::Landing) => {
            if sp.phase == P::Fall {
                vec!["FallSpecial", "Fall"]
            } else {
                vec!["LandingFallSpecial", "LandingLight"]
            }
        }
        (SuperJumpPunch | SpinningKong | ScrewAttack, _) => {
            since_start = true;
            air("SpecialHi", "SpecialAirHi")
        }
        (Tornado, _) => {
            since_start = true;
            air("SpecialLw", "SpecialAirLw")
        }
        (GiantPunch, P::Start) => air("SpecialNStart", "SpecialAirNStart"),
        (GiantPunch, P::Hold) => {
            looped = true;
            air("SpecialNLoop", "SpecialAirNLoop")
        }
        (GiantPunch, _) => {
            if sp.charge >= 10 {
                air("SpecialNFull", "SpecialAirNFull")
            } else {
                air("SpecialNEnd", "SpecialAirNEnd")
            }
        }
        (HandSlap, P::Start) => vec!["SpecialLwStart"],
        (HandSlap, P::Hold) => {
            looped = true;
            vec!["SpecialLwLoop"]
        }
        (HandSlap, _) => vec!["SpecialLwEnd"],
        (Boomerang, P::End) => air("SpecialNGet", "SpecialAirNReturn"),
        (Boomerang, _) => {
            if sp.charge == 1 {
                air("SpecialNEmpty", "SpecialAirNEmpty")
            } else {
                air("SpecialN", "SpecialAirN")
            }
        }
        (SpinAttack, P::End) => vec!["SpecialHiEnd", "Wait"],
        (SpinAttack, _) => air("SpecialHi", "SpecialAirHi"),
        (LinkBomb | SamusBomb, _) => {
            since_start = true;
            air("SpecialLw", "SpecialAirLw")
        }
        (ChargeShot, P::Start) => air("SpecialNStart", "SpecialAirNStart"),
        (ChargeShot, P::Hold) => {
            looped = true;
            vec!["SpecialNLoop"]
        }
        (ChargeShot, _) => air("SpecialNEnd", "SpecialAirNEnd"),
        (EggLay, P::Caught | P::End) => air("SpecialNRelease", "SpecialAirNRelease"),
        (EggLay, _) => air("SpecialN", "SpecialAirN"),
        (EggThrow, _) => {
            since_start = true;
            air("SpecialHi", "SpecialAirHi")
        }
        (YoshiBomb, P::Landing) => vec!["SpecialLwLanding"],
        (YoshiBomb, _) => {
            since_start = true;
            air("SpecialLwStart", "SpecialAirLwStart")
        }
        (Inhale, P::Start) => air("SpecialNStart", "SpecialAirNStart"),
        (Inhale, P::Hold) => {
            looped = true;
            air("SpecialNLoop", "SpecialAirNLoop")
        }
        (Inhale, P::Caught) => {
            looped = true;
            air("SpecialNWait", "SpecialAirNWait")
        }
        (Inhale, P::Travel) => air("SpecialNThrow", "SpecialAirNThrow"),
        (Inhale, P::Landing) => air("SpecialNEat", "SpecialAirNEat"),
        (Inhale, _) => air("SpecialNEnd", "SpecialAirNEnd"),
        (FinalCutter, P::Landing) => vec!["SpecialHiLanding"],
        (FinalCutter, P::Fall) => vec!["SpecialAirHiFall", "Fall"],
        (FinalCutter, _) => {
            since_start = true;
            air("SpecialHi", "SpecialAirHi")
        }
        (Stone, P::Start) => air("SpecialLwStart", "SpecialAirLwStart"),
        (Stone, P::End) => air("SpecialLwEnd", "SpecialAirLwEnd"),
        (Stone, _) => air("SpecialLwHold", "SpecialAirLwHold"),
        (FireFox, P::Start) => air("SpecialHiStart", "SpecialAirHiStart"),
        (FireFox, P::Hold) => {
            looped = true;
            air("SpecialHiHold", "SpecialAirHiHold")
        }
        (FireFox, P::Attack) => {
            looped = true;
            air("SpecialHi", "SpecialAirHi")
        }
        (FireFox, _) => air("SpecialHiEnd", "SpecialAirHiEnd"),
        (Reflector, P::Start) => air("SpecialLwStart", "SpecialAirLwStart"),
        (Reflector, P::Hold) => {
            looped = true;
            air("SpecialLwLoop", "SpecialAirLwLoop")
        }
        (Reflector, _) => air("SpecialLwEnd", "SpecialAirLwEnd"),
        (QuickAttack, _) => air("SpecialHi", "SpecialAirHi"),
        (Thunder, P::Start) => air("SpecialLwStart", "SpecialAirLwStart"),
        (Thunder, P::Hold) => {
            looped = true;
            air("SpecialLwLoop", "SpecialAirLwLoop")
        }
        (Thunder, P::Attack) => {
            looped = true;
            air("SpecialLwHit", "SpecialAirLwHit")
        }
        (Thunder, _) => air("SpecialLwEnd", "SpecialAirLwEnd"),
        (Pound, _) => {
            since_start = true;
            air("SpecialN", "SpecialAirN")
        }
        (Sing, _) => {
            since_start = true;
            vec!["SpecialHi", "SpecialAirHi"]
        }
        (Rest, _) => {
            since_start = true;
            vec!["SpecialLw", "SpecialAirLw"]
        }
        (FalconPunch, _) => {
            since_start = true;
            air("SpecialN", "SpecialAirN")
        }
        (FalconDive, P::Caught) => vec!["SpecialHiCatch", "SpecialHiThrow"],
        (FalconDive, P::End) => vec!["SpecialHiThrow", "SpecialAirHi"],
        (FalconDive, _) => air("SpecialHi", "SpecialAirHi"),
        (FalconKick, P::Landing) => vec!["SpecialLwLanding", "LandingLight"],
        (FalconKick, _) => air("SpecialLw", "SpecialAirLw"),
        (PkThunder, P::Start) => air("SpecialHiStart", "SpecialAirHiStart"),
        (PkThunder, P::Hold) => {
            looped = true;
            air("SpecialHiHold", "SpecialAirHiHold")
        }
        (PkThunder, P::Travel) => {
            looped = true;
            air("SpecialHiJibaku", "SpecialAirHiJibaku")
        }
        (PkThunder, _) => air("SpecialHiEnd", "SpecialAirHiEnd"),
        (PsiMagnet, P::Start) => air("SpecialLwStart", "SpecialAirLwStart"),
        (PsiMagnet, P::Hold) => {
            looped = true;
            air("SpecialLwHold", "SpecialAirLwHold")
        }
        (PsiMagnet, _) => air("SpecialLwEnd", "SpecialAirLwEnd"),
        (Item, _) => vec!["Wait"],
    };
    let secs = if since_start { sp.total as f32 / 30.0 } else { secs };
    let names = match sp.copied {
        Some(f) => names.iter().map(|n| format!("Copy{}{}", f.decomp_name(), n)).chain(names.iter().map(|n| n.to_string())).collect(),
        None => names.iter().map(|n| n.to_string()).collect(),
    };
    (names, secs, looped)
}

// ---------------------------------------------------------------------------------------------
// The step
// ---------------------------------------------------------------------------------------------

/// Whether Mario's current action lets a special start.
pub fn can_start(m: &MarioRuntime) -> bool {
    let a = m.action;
    let group = a & ACT_GROUP_MASK;
    if !matches!(group, ACT_GROUP_STATIONARY | ACT_GROUP_MOVING | ACT_GROUP_AIRBORNE) {
        return false;
    }
    if a & (ACT_FLAG_INVULNERABLE | ACT_FLAG_INTANGIBLE | ACT_FLAG_HANGING | ACT_FLAG_RIDING_SHELL | ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER) != 0 {
        return false;
    }
    if m.held_obj.is_some() || m.riding_shell.is_some() || m.grabbed_by.is_some() || m.cannon.is_some() {
        return false;
    }
    !matches!(
        a,
        ACT_FLYING
            | ACT_SHOT_FROM_CANNON
            | ACT_LAVA_BOOST
            | ACT_BURNING_JUMP
            | ACT_BURNING_FALL
            | ACT_BURNING_GROUND
            | ACT_GETTING_BLOWN
            | ACT_BACKWARD_AIR_KB
            | ACT_FORWARD_AIR_KB
            | ACT_HARD_BACKWARD_AIR_KB
            | ACT_HARD_FORWARD_AIR_KB
            | ACT_SOFT_BACKWARD_GROUND_KB
            | ACT_SOFT_FORWARD_GROUND_KB
            | ACT_HARD_BACKWARD_GROUND_KB
            | ACT_HARD_FORWARD_GROUND_KB
            | ACT_BACKWARD_GROUND_KB
            | ACT_FORWARD_GROUND_KB
            | ACT_STANDING_DEATH
            | ACT_DEATH_ON_BACK
            | ACT_DEATH_ON_STOMACH
            | ACT_QUICKSAND_DEATH
            | ACT_SQUISHED
            | ACT_SHOCKED
            | ACT_ELECTROCUTION
            | ACT_STAR_DANCE_EXIT
            | ACT_STAR_DANCE_NO_EXIT
            | ACT_STAR_DANCE_WATER
            | ACT_FALL_AFTER_STAR_GRAB
            | ACT_READING_SIGN
            | ACT_READING_NPC_DIALOG
            | ACT_READING_AUTOMATIC_DIALOG
            | ACT_WAITING_FOR_DIALOG
            | ACT_IN_QUICKSAND
    )
}

/// Which special the controller asks for: special button + stick up / down (or A / Z held).
pub fn slot_from_input(c: &ControllerSnapshot) -> SpecialSlot {
    let up = c.stick_y >= STICK_SPECIAL && c.stick_y.abs() > c.stick_x.abs();
    let down = c.stick_y <= -STICK_SPECIAL && c.stick_y.abs() > c.stick_x.abs();
    if c.button_down & Z_TRIG != 0 || (down && c.button_down & A_BUTTON == 0) {
        SpecialSlot::Down
    } else if c.button_down & A_BUTTON != 0 || up {
        SpecialSlot::Up
    } else {
        SpecialSlot::Neutral
    }
}

fn pressed(m: &MarioRuntime) -> bool {
    m.controller.button_pressed & R_TRIG != 0
}
fn held(m: &MarioRuntime) -> bool {
    m.controller.button_down & R_TRIG != 0
}

/// The special layer of one Mario step. Returns true when it consumed the frame (Mario's action loop is skipped).
pub fn step_smash(m: &mut MarioRuntime) -> bool {
    let Some(fighter) = m.smash.fighter else { return false };
    m.smash.center = [m.pos[0], m.pos[1] + 70.0, m.pos[2]];
    m.smash.yaw = m.face_angle[1];
    if m.action & ACT_FLAG_AIR == 0 {
        m.smash.egg_hop_used = false;
    }
    if m.smash.heal > 0.0 {
        let h = m.smash.heal;
        m.health = (m.health + h).min(m.max_health);
        m.smash.heal = 0.0;
    }
    if let Some(sp) = m.smash.special {
        if m.action != sp.sm64_action {
            // Interrupted from outside (knockback, death, warp, a mode moving Mario on).
            m.smash.special = None;
            m.smash.landed.clear();
            return false;
        }
        return run(m);
    }
    if items::step(m) {
        return true;
    }
    if !pressed(m) || !can_start(m) {
        return false;
    }
    let slot = slot_from_input(&m.controller);
    let mut mv = fighter.special(slot);
    let mut copied = None;
    if fighter == SmashFighter::Kirby && slot == SpecialSlot::Neutral {
        if let Some(c) = m.smash.copy {
            if c != SmashFighter::Kirby {
                mv = c.special(SpecialSlot::Neutral);
                copied = Some(c);
            }
        }
    }
    let air = m.action & ACT_FLAG_AIR != 0;
    // Donkey Kong's Hand Slap is a ground move.
    if mv == SpecialMove::HandSlap && air {
        return false;
    }
    // Face the stick for neutral and up specials (a down special keeps the facing when the stick picked it).
    let stick_down = m.controller.stick_y <= -STICK_SPECIAL;
    if m.intended_mag > 8.0 && !(slot == SpecialSlot::Down && stick_down) {
        m.face_angle[1] = m.intended_yaw;
    }
    let instance = m.smash.new_id();
    let sp = Special {
        fighter,
        mv,
        slot,
        copied,
        phase: Phase::Start,
        t: 0,
        total: 0,
        air,
        yaw: m.face_angle[1],
        dir: [0.0, 0.0],
        charge: 0,
        instance,
        sm64_action: m.action,
        land_lag: 8,
        drift: 0.6,
        landed: false,
        fresh: false,
    };
    m.smash.special = Some(sp);
    m.smash.landed.clear();
    sfx(m, &sp, SfxPhase::Start);
    run(m)
}

/// Run one tick of the special in `m.smash.special`.
fn run(m: &mut MarioRuntime) -> bool {
    let Some(mut sp) = m.smash.special.take() else { return false };
    // Hits of this special that landed since last tick.
    let landed: Vec<(u32, HitKind, HitOutcome)> = std::mem::take(&mut m.smash.landed);
    for (src, kind, out) in landed {
        if owned_by(src, sp.instance) && out != HitOutcome::Miss {
            sp.landed = true;
            if let HitOutcome::Captured(f) = out {
                if matches!(sp.mv, SpecialMove::Inhale) {
                    m.smash.mouth = Some(f);
                }
            }
            if kind == HitKind::Grab && sp.mv == SpecialMove::FalconDive && sp.phase == Phase::Attack {
                set_phase(&mut sp, Phase::Caught);
            }
        }
    }
    m.face_angle[1] = sp.yaw;
    let flow = tick(m, &mut sp);
    if matches!(flow, Flow::Continue) {
        scripted_hitboxes(m, &sp);
    }
    if !sp.fresh {
        sp.t += 1;
    }
    sp.fresh = false;
    sp.total += 1;
    match flow {
        Flow::Continue => {
            sp.sm64_action = m.action;
            m.smash.special = Some(sp);
            true
        }
        Flow::End(action) => {
            m.smash.special = None;
            if m.action != action {
                // A fall keeps the move's speed (`set_action` does not touch the velocity for these actions).
                m.set_action(action, 0.0);
            }
            false
        }
        Flow::Handed => {
            m.smash.special = None;
            false
        }
    }
}

enum Flow {
    Continue,
    /// The special is over: Mario goes on in this action (his own action runs this same frame).
    End(u32),
    /// Over; Mario's action was already set (a ledge grab).
    Handed,
}

fn set_phase(sp: &mut Special, p: Phase) {
    sp.phase = p;
    sp.t = 0;
    sp.fresh = true;
}

fn sfx(m: &mut MarioRuntime, sp: &Special, phase: SfxPhase) {
    m.smash.events.push(SmashEvent::Sfx(SmashSfx { fighter: sp.copied.unwrap_or(sp.fighter), mv: sp.mv, slot: sp.slot, phase }));
}

fn fwd(yaw: f64) -> [f64; 2] {
    [sins(yaw), coss(yaw)]
}

/// Point `ahead` units in front of Mario and `up` above his feet.
fn at(m: &MarioRuntime, yaw: f64, ahead: f64, up: f64) -> Vec3 {
    let f = fwd(yaw);
    [m.pos[0] + f[0] * ahead, m.pos[1] + up, m.pos[2] + f[1] * ahead]
}

fn set_act(m: &mut MarioRuntime, action: u32) {
    if m.action != action {
        m.state.prev_action = m.action;
        m.state.action = action;
        m.state.action_state = 0.0;
        m.state.action_timer = 0.0;
    }
}

/// Stick direction as a world unit vector (camera relative) and its strength 0..1.
fn stick_dir(m: &MarioRuntime) -> ([f64; 2], f64) {
    let mag = (m.intended_mag / 32.0).clamp(0.0, 1.0);
    (fwd(m.intended_yaw), mag)
}

/// Horizontal air velocity eased toward the stick (`drift` of `max`).
fn drift(m: &mut MarioRuntime, max: f64, accel: f64) {
    let (d, mag) = stick_dir(m);
    let want = [d[0] * max * mag, d[1] * max * mag];
    for (k, w) in [(0usize, want[0]), (2usize, want[1])] {
        let v = m.vel[k];
        m.vel[k] = if (w - v).abs() <= accel { w } else { v + accel * (w - v).signum() };
    }
}

/// One air step with this tick's velocity `m.vel`; gravity is ours (`g`, clamped at `terminal`), not the sim's.
fn air(m: &mut MarioRuntime, g: f64, terminal: f64, ledges: bool) -> i32 {
    let vy = m.vel[1];
    set_act(m, if m.action & ACT_FLAG_AIR != 0 { m.action } else { ACT_FREEFALL });
    m.state.forward_vel = m.vel[0] * sins(m.face_angle[1]) + m.vel[2] * coss(m.face_angle[1]);
    let arg = if ledges && vy <= 0.0 { m.air_step_arg() } else { 0 };
    let r = m.step_air(arg);
    if r != AIR_LANDED && r != AIR_GRABBED_LEDGE {
        m.vel[1] = (vy - g).max(terminal);
    }
    r
}

/// Ground movement at `speed` along the facing; returns false when Mario walked off a ledge.
fn ground(m: &mut MarioRuntime, speed: f64) -> bool {
    mario_set_forward_vel(m, speed);
    m.vel[1] = 0.0;
    let r = m.step_ground();
    r != GROUND_STEP_LEFT_GROUND_I
}

/// Stand (or fall) in place: ground friction, or air drift with gravity. Returns true if airborne.
fn hold(m: &mut MarioRuntime, sp: &mut Special, g: f64, drift_max: f64) -> bool {
    if sp.air {
        drift(m, drift_max, 1.0);
        let r = air(m, g, TERMINAL, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        let v = approach(m.forward_vel, 0.0, 2.0);
        if !ground(m, v) {
            sp.air = true;
            set_act(m, ACT_FREEFALL);
        } else {
            set_act(m, ACT_IDLE);
        }
    }
    sp.air
}

fn approach(v: f64, target: f64, step: f64) -> f64 {
    if (target - v).abs() <= step {
        target
    } else {
        v + step * (target - v).signum()
    }
}

/// Move by an exact displacement this tick (root motion): `ahead` along the facing and `up`.
fn displace(m: &mut MarioRuntime, sp: &mut Special, ahead: f64, up: f64) -> i32 {
    let f = fwd(sp.yaw);
    if !sp.air && up <= 0.0 {
        let ok = ground(m, ahead);
        if !ok {
            sp.air = true;
            set_act(m, ACT_FREEFALL);
        }
        return 0;
    }
    if !sp.air {
        sp.air = true;
        set_act(m, ACT_FREEFALL);
    }
    m.vel = [f[0] * ahead, up, f[1] * ahead];
    let r = air(m, 0.0, -1000.0, false);
    if r == AIR_LANDED {
        sp.air = false;
    } else {
        m.vel[1] = up;
    }
    r
}

/// Ease-out height curve: fraction of a rise done after `t` of `n` ticks.
fn ease_out(t: f64, n: f64) -> f64 {
    let x = (t / n).clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x)
}

/// Per-tick delta of an ease-out rise of `total` over `n` ticks, at tick `t`.
fn rise(total: f64, n: f64, t: u32) -> f64 {
    total * (ease_out(t as f64 + 1.0, n) - ease_out(t as f64, n))
}

/// The helpless fall: drift, Mario's gravity, ledges grab; landing lag then idle.
fn fall_special(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Fall => {
            drift(m, 24.0 * sp.drift, 1.2);
            let r = air(m, GRAVITY, TERMINAL, true);
            if r == AIR_GRABBED_LEDGE {
                m.set_action(ACT_LEDGE_GRAB, 0.0);
                m.vel = [0.0; 3];
                return Flow::Handed;
            }
            if r == AIR_LANDED {
                sp.air = false;
                set_act(m, ACT_IDLE);
                mario_set_forward_vel(m, 0.0);
                set_phase(sp, Phase::Landing);
                sfx(m, sp, SfxPhase::Land);
            }
            Flow::Continue
        }
        _ => {
            ground(m, approach(m.forward_vel, 0.0, 4.0));
            if sp.t + 1 >= sp.land_lag {
                Flow::End(ACT_IDLE)
            } else {
                Flow::Continue
            }
        }
    }
}

/// End of a move in place: idle on the ground, a normal fall in the air.
fn done(sp: &Special) -> Flow {
    Flow::End(if sp.air { ACT_FREEFALL } else { ACT_IDLE })
}

/// Start the helpless fall (or land at once on the ground).
fn to_fall(m: &mut MarioRuntime, sp: &mut Special, drift_mul: f64, lag_frames: f64) -> Flow {
    sp.drift = drift_mul;
    sp.land_lag = ticks(lag_frames);
    if sp.air {
        set_phase(sp, Phase::Fall);
        Flow::Continue
    } else {
        set_act(m, ACT_IDLE);
        Flow::End(ACT_IDLE)
    }
}

#[allow(clippy::too_many_arguments)]
fn hitbox(m: &mut MarioRuntime, sp: &Special, sub: u32, pos: Vec3, radius: f64, damage: f32, kind: HitKind, knock: f64, rehit: u32) {
    m.smash.hitboxes.push(Hitbox { source: (sp.instance << 4) | (sub & 15), pos, radius, damage, kind, knock, rehit, knockback: None, motion: None, normal: false });
}

// ---------------------------------------------------------------------------------------------
// Scripted hitboxes (the motion event scripts, `smash_hitbox`)
// ---------------------------------------------------------------------------------------------

/// Hit identities of scripted hitboxes: bit 31 (a special's, the special's instance in bits 12..30) or bit 30 (a normal
/// attack's), the window's hit-record set in the low 12 bits.
const SCRIPT_SOURCE: u32 = 1 << 31;
const NORMAL_SOURCE: u32 = 1 << 30;

/// Whether a hit source belongs to the special with this instance (its own hitboxes or its scripted ones).
fn owned_by(src: u32, instance: u32) -> bool {
    if src & SCRIPT_SOURCE != 0 {
        (src >> 12) & 0x7_FFFF == instance & 0x7_FFFF
    } else {
        src & NORMAL_SOURCE == 0 && src >> 4 == instance
    }
}

/// The sim's hit kind for an SSB64 element.
pub fn hit_kind(e: Element) -> HitKind {
    match e {
        Element::Fire => HitKind::Fire,
        Element::Electric => HitKind::Electric,
        Element::Sleep => HitKind::Sleep,
        _ => HitKind::Normal,
    }
}

/// The motion of `names` (best first) for a fighter in the scripts, if it has hitboxes.
fn scripted_motion<'a>(ev: &'a MotionEvents, fighter: SmashFighter, names: &[String]) -> Option<&'a Motion> {
    ev.fighter(fighter.decomp_name())?.first(names).filter(|m| m.has_hitboxes())
}

/// Push the hitboxes a motion has on in frames `[from, to)` as this tick's attack volumes (centres on the animated
/// joints, turned to `yaw` around Mario's feet).
fn push_scripted(m: &mut MarioRuntime, motion: &Motion, from: u32, to: u32, yaw: f64, source_base: u32, normal: bool) {
    let feet = m.pos;
    let mut seen: Vec<(u8, u32)> = vec![];
    // the later frame of the tick first: the one the hit is logged at
    for hit in motion.active_in_frames(from, to).iter().rev() {
        let w = hit.window;
        if seen.contains(&(w.id, hit.frame)) {
            continue;
        }
        seen.push((w.id, hit.frame));
        let kb = w.knockback;
        m.smash.hitboxes.push(Hitbox {
            source: source_base | (w.set & 0xFFF),
            pos: hit.world(feet, yaw, K),
            radius: w.radius() * K,
            damage: kb.damage as f32,
            kind: hit_kind(kb.element),
            knock: 10.0 + kb.damage as f64,
            rehit: 0,
            knockback: Some(kb),
            motion: Some(motion.id),
            normal,
        });
    }
}

/// A running special whose motion has scripted hitboxes takes them instead of its own damaging volumes: damage, timing,
/// size, angle and knockback from the script, centres on the animated skeleton. The move's capture / grab / sleep volumes
/// and projectiles stay. The motion time is the view's (`SmashExt::motion`), 60 Hz frames `2t` and `2t + 1` of this tick.
fn scripted_hitboxes(m: &mut MarioRuntime, sp: &Special) {
    // Catch moves (`ftParamSetCatchParams`: Falcon Dive, Kirby's inhale, Yoshi's egg lay and Kirby's copy of it) use their
    // attack collisions as grab detectors, not as hits: the move's own capture / grab volume stays the only one.
    if matches!(sp.mv, SpecialMove::FalconDive | SpecialMove::Inhale | SpecialMove::EggLay) {
        return;
    }
    let Some(ev) = m.smash.motion_events() else { return };
    let (names, secs, _) = special_motion(sp);
    let body = sp.copied.map(|_| SmashFighter::Kirby).unwrap_or(sp.fighter);
    let Some(motion) = scripted_motion(&ev, body, &names) else { return };
    let inst = sp.instance;
    m.smash.hitboxes.retain(|h| {
        !(h.source & (SCRIPT_SOURCE | NORMAL_SOURCE) == 0 && h.source >> 4 == inst && matches!(h.kind, HitKind::Normal | HitKind::Fire | HitKind::Electric | HitKind::Explosion))
    });
    let frame = (secs as f64 * 60.0).round() as u32;
    push_scripted(m, motion, frame, frame + 2, sp.yaw, SCRIPT_SOURCE | ((inst & 0x7_FFFF) << 12), false);
}

/// The normal attack playing: its first motion name, punch stage, start tick and hit identity.
#[derive(Clone, Copy, PartialEq, Debug)]
struct NormalAttack {
    key: (&'static str, u8),
    start: u64,
    instance: u32,
}

/// The SSB64 motions (best first) and playback rate a Smash fighter plays for Mario's attacking actions, as the view maps
/// them (`characters::smash` `clip_for`: punches 1.15x, ground pound 1.2x), with the punch stage from `actionArg`.
pub fn normal_attack_motion(action: u32, action_arg: f64) -> Option<(&'static [&'static str], u8, f64)> {
    Some(match action {
        ACT_PUNCHING | ACT_MOVE_PUNCHING if action_arg < 3.0 => (&["Attack11"][..], 0, 1.15),
        ACT_PUNCHING | ACT_MOVE_PUNCHING if action_arg < 6.0 => (&["Attack12", "Attack11"][..], 1, 1.15),
        ACT_PUNCHING | ACT_MOVE_PUNCHING => (&["Attack13", "AttackS3"][..], 2, 1.15),
        ACT_JUMP_KICK => (&["AttackAirN"][..], 0, 1.0),
        ACT_DIVE => (&["AttackAirF"][..], 0, 1.0),
        ACT_SLIDE_KICK | ACT_SLIDE_KICK_SLIDE => (&["AttackDash"][..], 0, 1.0),
        ACT_GROUND_POUND => (&["AttackAirLw"][..], 0, 1.2),
        ACT_GROUND_POUND_LAND => (&["LandingAirLw"][..], 0, 1.0),
        _ => return None,
    })
}

/// Mario's punches, kicks, dive, slide kick and ground pound as the fighter's SSB64 normal attacks: the motion's scripted
/// hitboxes at the clip's time (frames since the move started x 2 x rate), resolved like the specials' (`normal` set: the
/// course's SM64 objects and the modes that score Mario's own punches ignore them, SM64 already handles those).
fn normal_attacks(m: &mut MarioRuntime) {
    let Some(fighter) = m.smash.fighter else { return };
    let current = if m.smash.special.is_some() { None } else { normal_attack_motion(m.action, m.action_arg) };
    let Some((names, stage, rate)) = current else {
        m.smash.attack = None;
        return;
    };
    let key = (names[0], stage);
    let now = m.smash.tick;
    let attack = match m.smash.attack {
        Some(a) if a.key == key => a,
        _ => {
            let instance = m.smash.new_id();
            let a = NormalAttack { key, start: now, instance };
            m.smash.attack = Some(a);
            a
        }
    };
    let Some(ev) = m.smash.motion_events() else { return };
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let Some(motion) = scripted_motion(&ev, fighter, &names) else { return };
    let t = (now - attack.start) as f64;
    let (from, to) = ((t * 2.0 * rate).floor() as u32, ((t + 1.0) * 2.0 * rate).floor() as u32);
    let yaw = m.face_angle[1];
    push_scripted(m, motion, from, to, yaw, NORMAL_SOURCE | ((attack.instance & 0x3_FFFF) << 12), true);
}

#[allow(clippy::too_many_arguments)]
fn spawn(m: &mut MarioRuntime, sp: &Special, kind: ProjKind, pos: Vec3, vel: Vec3, life: u32, radius: f64, damage: f32, hit: HitKind, gravity: f64) -> usize {
    let id = m.smash.new_id();
    m.smash.projectiles.push(Projectile {
        id,
        kind,
        fighter: sp.copied.unwrap_or(sp.fighter),
        mv: sp.mv,
        pos,
        vel,
        age: 0,
        life,
        radius,
        damage,
        hit,
        gravity,
        terminal: -60.0,
        rehit: 0,
        pierce: false,
        level: 0,
        dead: false,
    });
    m.smash.projectiles.len() - 1
}

fn blast(m: &mut MarioRuntime, fighter: SmashFighter, mv: SpecialMove, pos: Vec3, radius: f64, damage: f32, life: u32) {
    m.smash.add_blast(fighter, mv, pos, radius, damage, life);
}

impl SmashExt {
    /// A short-lived blast volume (and its cue).
    pub fn add_blast(&mut self, fighter: SmashFighter, mv: SpecialMove, pos: Vec3, radius: f64, damage: f32, life: u32) {
    let id = self.new_id();
    self.projectiles.push(Projectile {
        id,
        kind: ProjKind::Blast,
        fighter,
        mv,
        pos,
        vel: [0.0; 3],
        age: 0,
        life,
        radius,
        damage,
        hit: HitKind::Explosion,
        gravity: 0.0,
        terminal: 0.0,
        rehit: 0,
        pierce: true,
        level: 0,
        dead: false,
    });
    self.events.push(SmashEvent::Explode { pos, radius });
    }

    /// A PK Fire pillar standing at `pos` (feet).
    pub fn add_pillar(&mut self, fighter: SmashFighter, mv: SpecialMove, pos: Vec3) {
        let id = self.new_id();
        self.projectiles.push(Projectile {
            id,
            kind: ProjKind::PkFirePillar,
            fighter,
            mv,
            pos: [pos[0], pos[1] + 60.0, pos[2]],
            vel: [0.0; 3],
            age: 0,
            life: 40,
            radius: 75.0,
            damage: 2.0,
            hit: HitKind::Fire,
            gravity: 0.0,
            terminal: 0.0,
            rehit: 4,
            pierce: true,
            level: 0,
            dead: false,
        });
        self.events.push(SmashEvent::Sfx(SmashSfx { fighter, mv, slot: SpecialSlot::Neutral, phase: SfxPhase::Hit }));
    }
}

/// The per-move logic: one tick.
fn tick(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    use SpecialMove::*;
    match sp.mv {
        Fireball => fireball(m, sp),
        SuperJumpPunch => super_jump_punch(m, sp),
        Tornado => tornado(m, sp),
        GiantPunch => giant_punch(m, sp),
        SpinningKong => spinning_kong(m, sp),
        HandSlap => hand_slap(m, sp),
        Boomerang => boomerang(m, sp),
        SpinAttack => spin_attack(m, sp),
        LinkBomb => link_bomb(m, sp),
        ChargeShot => charge_shot(m, sp),
        ScrewAttack => screw_attack(m, sp),
        SamusBomb => samus_bomb(m, sp),
        EggLay => egg_lay(m, sp),
        EggThrow => egg_throw(m, sp),
        YoshiBomb => yoshi_bomb(m, sp),
        Inhale => inhale(m, sp),
        FinalCutter => final_cutter(m, sp),
        Stone => stone(m, sp),
        Blaster => blaster(m, sp),
        FireFox => fire_fox(m, sp),
        Reflector => reflector(m, sp),
        ThunderJolt => thunder_jolt(m, sp),
        QuickAttack => quick_attack(m, sp),
        Thunder => thunder(m, sp),
        Pound => pound(m, sp),
        Sing => sing(m, sp),
        Rest => rest(m, sp),
        FalconPunch => falcon_punch(m, sp),
        FalconDive => falcon_dive(m, sp),
        FalconKick => falcon_kick(m, sp),
        PkFire => pk_fire(m, sp),
        PkThunder => pk_thunder(m, sp),
        PsiMagnet => psi_magnet(m, sp),
        Item => Flow::Handed,
    }
}

// ----- Mario / Luigi ---------------------------------------------------------------------------

/// Fireball (`wpmariofireball.c`): Mario's bounces (base 50, -5 degrees, gravity 1.2, 140 frames), Luigi's flies straight
/// (36, no gravity, 80 frames). The clip is 47 frames; the ball leaves on frame ~15.
fn fireball(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 12.0);
    if sp.total == 7 {
        let luigi = sp.copied.unwrap_or(sp.fighter) == SmashFighter::Luigi;
        let f = fwd(sp.yaw);
        let pos = at(m, sp.yaw, 60.0, 80.0);
        if luigi {
            let v = 36.0 * KV;
            spawn(m, sp, ProjKind::GreenFireball, pos, [f[0] * v, 0.0, f[1] * v], ticks(80.0), 40.0, 6.0, HitKind::Fire, 0.0);
        } else {
            let (v, ang) = (50.0 * KV, (-5.0f64).to_radians());
            let i = spawn(m, sp, ProjKind::Fireball, pos, [f[0] * v * ang.cos(), v * ang.sin(), f[1] * v * ang.cos()], ticks(140.0), 40.0, 7.0, HitKind::Fire, 1.2 * KA);
            m.smash.projectiles[i].terminal = -55.0 * KV;
        }
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(47.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Super Jump Punch: rises 1573 root units (x size 1.12, peak on frame 24) and moves 868 forward over its 41 frames, then
/// the helpless fall (drift 0.6, landing lag 25 frames). Coins of 1% on the way up, a final blow at the peak (Luigi: a
/// 25% fire sweet spot on the first frames instead).
fn super_jump_punch(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.phase != Phase::Start {
        return fall_special(m, sp);
    }
    let t = sp.total;
    let (peak_t, dur) = (ticks(24.0), ticks(41.0));
    let rise_total = 1573.0 * 1.12 * K;
    let drop = (1573.0 - 1064.0) * 1.12 * K;
    let ahead_total = 868.0 * 1.12 * K;
    let up = if t < peak_t { rise(rise_total, peak_t as f64, t) } else { -rise(drop, (dur - peak_t) as f64, dur - 1 - t) };
    let ahead = rise(ahead_total, dur as f64, t);
    if t == 0 {
        m.vel = [0.0; 3];
        sfx(m, sp, SfxPhase::Release);
    }
    displace(m, sp, ahead, up);
    let luigi = sp.copied.unwrap_or(sp.fighter) == SmashFighter::Luigi;
    let body = at(m, sp.yaw, 40.0, 110.0);
    if luigi {
        if t < 2 {
            hitbox(m, sp, 1, body, 90.0, 25.0, HitKind::Fire, 30.0, 0);
        } else if t < peak_t {
            hitbox(m, sp, 2, body, 90.0, 1.0, HitKind::Normal, 4.0, 0);
        }
    } else if t < peak_t - 2 {
        hitbox(m, sp, 1, body, 90.0, 1.0, HitKind::Normal, 4.0, 3);
    } else if t < peak_t + 1 {
        let top = at(m, sp.yaw, 30.0, 170.0);
        hitbox(m, sp, 2, top, 100.0, 6.0, HitKind::Normal, 18.0, 0);
    }
    if t + 1 >= dur {
        sp.air = true;
        set_act(m, ACT_FREEFALL);
        m.vel = [0.0, 0.0, 0.0];
        return to_fall(m, sp, 0.6, 25.0);
    }
    Flow::Continue
}

/// Mario Tornado / Luigi Cyclone: 88 frames of spinning; the stick steers (0.025 / 0.03 accel, 17 max), each special
/// press adds 22 of lift (clamped at 40), so mashing rises. Multi-hits of 2%, a 6% finisher.
fn tornado(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    let t = sp.total;
    let dur = if sp.air { ticks(84.0) } else { ticks(88.0) };
    let (d, mag) = stick_dir(m);
    let max = 17.0 * KV;
    let accel = if sp.air { 0.03 } else { 0.025 } * 64.0 * KV;
    let want = [d[0] * max * mag, d[1] * max * mag];
    for (k, w) in [(0usize, want[0]), (2usize, want[1])] {
        m.vel[k] = approach(m.vel[k], w, accel);
    }
    if t > 0 && pressed(m) {
        m.vel[1] = (m.vel[1].max(0.0) + 22.0 * KV).min(40.0 * KV);
        if !sp.air {
            sp.air = true;
        }
    }
    if sp.air {
        let r = air(m, 1.2 * KA, -30.0, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        let speed = (m.vel[0] * m.vel[0] + m.vel[2] * m.vel[2]).sqrt();
        if speed > 0.5 {
            let yaw = atan2s(m.vel[2], m.vel[0]) as f64;
            m.face_angle[1] = yaw;
        }
        if !ground(m, speed) {
            sp.air = true;
            set_act(m, ACT_FREEFALL);
        } else {
            set_act(m, ACT_IDLE);
        }
        m.face_angle[1] = sp.yaw;
    }
    let center = at(m, sp.yaw, 0.0, 80.0);
    if t >= 3 && t + 6 < dur {
        hitbox(m, sp, 1, center, 130.0, 2.0, HitKind::Normal, 6.0, 3);
    } else if t + 6 == dur || t + 5 == dur {
        hitbox(m, sp, 2, center, 140.0, 6.0, HitKind::Normal, 22.0, 0);
    }
    if t + 1 >= dur {
        done(sp)
    } else {
        Flow::Continue
    }
}

// ----- Donkey Kong -----------------------------------------------------------------------------

/// Giant Punch (`ftdonkeyspecialn.c`): a 9-frame start, then a 13-frame loop that adds one charge level each pass (10
/// max: stored, flashing). Special again punches (damage = base + 2 x charge, the full punch 28%); jump or Z store the
/// charge.
fn giant_punch(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start => {
            hold(m, sp, GRAVITY, 10.0);
            if sp.t == 0 {
                sp.charge = m.smash.dk_charge;
                if sp.charge >= 10 {
                    set_phase(sp, Phase::Attack);
                    sfx(m, sp, SfxPhase::Release);
                    return Flow::Continue;
                }
            }
            if sp.t + 1 >= ticks(9.0) {
                set_phase(sp, Phase::Hold);
            }
            Flow::Continue
        }
        Phase::Hold => {
            hold(m, sp, GRAVITY, 10.0);
            if sp.t > 0 && sp.t.is_multiple_of(ticks(13.0)) && sp.charge < 10 {
                sp.charge += 1;
                sfx(m, sp, if sp.charge == 10 { SfxPhase::Full } else { SfxPhase::Charge });
                if sp.charge >= 10 {
                    m.smash.dk_charge = 10;
                    return done(sp);
                }
            }
            let c = m.controller.button_pressed;
            if pressed(m) && sp.t > 0 {
                set_phase(sp, Phase::Attack);
                sfx(m, sp, SfxPhase::Release);
            } else if c & (A_BUTTON | Z_TRIG) != 0 {
                m.smash.dk_charge = sp.charge;
                return done(sp);
            }
            Flow::Continue
        }
        _ => {
            let t = sp.t;
            if t == 0 {
                m.smash.dk_charge = 0;
            }
            let lunge = if (2..10).contains(&t) && !sp.air { (4.0 + 0.8 * sp.charge as f64) * (1.0 - (t - 2) as f64 / 8.0) } else { 0.0 };
            if sp.air {
                hold(m, sp, GRAVITY, 10.0);
            } else if !ground(m, lunge) {
                sp.air = true;
                set_act(m, ACT_FREEFALL);
            } else {
                set_act(m, ACT_IDLE);
            }
            if (6..10).contains(&t) {
                let dmg = 10.0 + 1.8 * sp.charge as f32;
                hitbox(m, sp, 1, at(m, sp.yaw, 130.0, 100.0), 110.0, dmg, HitKind::Normal, 12.0 + 2.5 * sp.charge as f64, 0);
            }
            if t + 1 >= ticks(if sp.charge >= 10 { 60.0 } else { 50.0 }) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

/// Spinning Kong: 101 frames of spinning. On the ground it travels (0.025 accel, 26 max); in the air it lifts at 20.3
/// with 0.07 of DK's gravity (3.0) and drifts up to 38, then falls helpless (drift 1.0, lag 0.3).
fn spinning_kong(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.phase != Phase::Start {
        return fall_special(m, sp);
    }
    let t = sp.total;
    let dur = ticks(101.0);
    let (d, mag) = stick_dir(m);
    if sp.air {
        if t == 0 {
            m.vel[1] = 20.3 * KV;
        }
        let max = 38.0 * KV;
        m.vel[0] = approach(m.vel[0], d[0] * max * mag, 0.05 * 64.0 * KV);
        m.vel[2] = approach(m.vel[2], d[1] * max * mag, 0.05 * 64.0 * KV);
        let r = air(m, 3.0 * 0.07 * KA, TERMINAL, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        let max = 26.0 * KV;
        m.vel[0] = approach(m.vel[0], d[0] * max * mag, 0.025 * 64.0 * KV);
        m.vel[2] = approach(m.vel[2], d[1] * max * mag, 0.025 * 64.0 * KV);
        let speed = m.vel[0].hypot(m.vel[2]);
        if speed > 0.5 {
            m.face_angle[1] = atan2s(m.vel[2], m.vel[0]) as f64;
        }
        if !ground(m, speed) {
            sp.air = true;
            set_act(m, ACT_FREEFALL);
        } else {
            set_act(m, ACT_IDLE);
        }
        m.face_angle[1] = sp.yaw;
    }
    if t >= 4 && t + 4 < dur {
        hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 110.0), 160.0, 2.0, HitKind::Normal, 8.0, 3);
    } else if t + 4 == dur {
        hitbox(m, sp, 2, at(m, sp.yaw, 0.0, 110.0), 170.0, 8.0, HitKind::Normal, 24.0, 0);
    }
    if t + 1 >= dur {
        return to_fall(m, sp, 1.0, 22.0);
    }
    Flow::Continue
}

/// Hand Slap (ground only): a 4-frame start, a 35-frame loop of two slaps (shock waves along the floor, 12%), repeated
/// while special is pressed again, then a 6-frame end.
fn hand_slap(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if hold(m, sp, GRAVITY, 0.0) {
        return done(sp);
    }
    match sp.phase {
        Phase::Start => {
            if sp.t + 1 >= ticks(4.0) {
                set_phase(sp, Phase::Hold);
            }
            Flow::Continue
        }
        Phase::Hold => {
            let t = sp.t;
            if t > 0 && pressed(m) {
                sp.charge = 1;
            }
            if t == 4 || t == 13 {
                sp.instance = m.smash.new_id();
                let p = at(m, sp.yaw, 70.0, 20.0);
                hitbox(m, sp, 1, p, 230.0, 12.0, HitKind::Normal, 26.0, 0);
                sfx(m, sp, SfxPhase::Hit);
                m.smash.events.push(SmashEvent::Explode { pos: p, radius: 160.0 });
            }
            if t + 1 >= ticks(35.0) {
                if sp.charge == 1 {
                    sp.charge = 0;
                    set_phase(sp, Phase::Hold);
                } else {
                    set_phase(sp, Phase::End);
                }
            }
            Flow::Continue
        }
        _ => {
            if sp.t + 1 >= ticks(6.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

// ----- Link ------------------------------------------------------------------------------------

/// Boomerang (`wplinkboomerang.c`): thrown on frame ~18 from (150, 290), flies out decelerating, turns and homes back
/// (one at a time: the `Empty` clip when it is out). 9% out, 8% back (`WPBOOMERANG_RETURN_DAMAGE`).
fn boomerang(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 10.0);
    if sp.phase == Phase::End {
        return if sp.t + 1 >= ticks(21.0) { done(sp) } else { Flow::Continue };
    }
    if sp.total == 0 && m.smash.projectiles.iter().any(|p| p.kind == ProjKind::Boomerang && !p.dead) {
        sp.charge = 1;
    }
    if sp.total == 9 && sp.charge == 0 {
        let f = fwd(sp.yaw);
        let v = 42.0;
        let pos = at(m, sp.yaw, 150.0 * K, 290.0 * K);
        let i = spawn(m, sp, ProjKind::Boomerang, pos, [f[0] * v, 0.0, f[1] * v], ticks(160.0), 45.0, 9.0, HitKind::Normal, 0.0);
        m.smash.projectiles[i].pierce = true;
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(47.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Spin Attack: on the ground a 61-frame spin (multi-hit); in the air a lift of 69 with 0.23 of Link's gravity (3.4)
/// until the motion's flag, drift x0.5, then the helpless fall (drift 0.6, lag 0.65).
fn spin_attack(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if matches!(sp.phase, Phase::Fall | Phase::Landing) {
        return fall_special(m, sp);
    }
    let t = sp.total;
    if !sp.air && sp.phase == Phase::Start {
        hold(m, sp, GRAVITY, 0.0);
        if (4..20).contains(&t) {
            hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 90.0), 170.0, 4.0, HitKind::Normal, 14.0, 4);
        }
        if t + 1 >= ticks(61.0) {
            return done(sp);
        }
        return Flow::Continue;
    }
    if t == 0 {
        m.vel[1] = 69.0 * KV;
        sfx(m, sp, SfxPhase::Release);
    }
    drift(m, 31.0 * KV * 0.5, 0.04 * 64.0 * KV * 0.5);
    let g = if t < 8 { 3.4 * 0.23 * KA } else { 3.4 * KA * 0.85 };
    let r = air(m, g, TERMINAL, t > 12);
    if r == AIR_GRABBED_LEDGE {
        m.set_action(ACT_LEDGE_GRAB, 0.0);
        return Flow::Handed;
    }
    if t < 20 {
        hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 90.0), 160.0, 3.0, HitKind::Normal, 12.0, 4);
    }
    if r == AIR_LANDED && t > 2 {
        sp.air = false;
        set_act(m, ACT_IDLE);
        sp.land_lag = ticks(30.0);
        set_phase(sp, Phase::Landing);
        return Flow::Continue;
    }
    if t + 1 >= ticks(101.0) / 2 {
        return to_fall(m, sp, 0.6, 30.0);
    }
    Flow::Continue
}

/// Bomb (`itvars.h` `ITLINKBOMB_*`): Link pulls a bomb and lobs it (gravity 1.2, 300-frame fuse); it bursts on a body, a
/// wall, a hard landing or when the fuse runs out.
fn link_bomb(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 10.0);
    if sp.total == 12 {
        let f = fwd(sp.yaw);
        let pos = at(m, sp.yaw, 50.0, 120.0);
        let base = [m.vel[0] * 0.5, 0.0, m.vel[2] * 0.5];
        spawn(m, sp, ProjKind::LinkBomb, pos, [base[0] + f[0] * 22.0, 26.0, base[2] + f[1] * 22.0], ticks(300.0), 40.0, 12.0, HitKind::Explosion, 1.2 * KA);
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(46.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

// ----- Samus -----------------------------------------------------------------------------------

/// Charge Shot level `n`: (speed, damage, hit size) of `dWPSamusChargeShotWeaponAttributes`.
pub const CHARGE_SHOT: [(f64, f32, f64); 8] = [(60.0, 3.0, 100.0), (62.0, 6.0, 120.0), (64.0, 9.0, 140.0), (66.0, 12.0, 160.0), (68.0, 15.0, 180.0), (70.0, 18.0, 200.0), (72.0, 21.0, 240.0), (74.0, 26.0, 260.0)];

/// Charge Shot (`ftsamusspecialn.c`): on the ground a 17-frame start then charging, one level every 20 frames up to 7
/// (stored when full or when jump / Z cancels); special fires. In the air it fires the stored level at once.
fn charge_shot(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start => {
            hold(m, sp, GRAVITY, 10.0);
            if sp.t == 0 {
                sp.charge = m.smash.samus_charge;
            }
            if sp.t + 1 >= ticks(17.0) {
                if sp.air || sp.charge >= 7 {
                    set_phase(sp, Phase::Attack);
                } else {
                    set_phase(sp, Phase::Hold);
                }
            }
            Flow::Continue
        }
        Phase::Hold => {
            hold(m, sp, GRAVITY, 10.0);
            if sp.air {
                set_phase(sp, Phase::Attack);
                return Flow::Continue;
            }
            if sp.t > 0 && sp.t.is_multiple_of(ticks(20.0)) && sp.charge < 7 {
                sp.charge += 1;
                sfx(m, sp, if sp.charge == 7 { SfxPhase::Full } else { SfxPhase::Charge });
                if sp.charge == 7 {
                    m.smash.samus_charge = 7;
                    return done(sp);
                }
            }
            if pressed(m) && sp.t > 0 {
                set_phase(sp, Phase::Attack);
            } else if m.controller.button_pressed & (A_BUTTON | Z_TRIG) != 0 {
                m.smash.samus_charge = sp.charge;
                return done(sp);
            }
            Flow::Continue
        }
        _ => {
            hold(m, sp, GRAVITY, 10.0);
            if sp.t == 2 {
                let lvl = sp.charge.min(7) as usize;
                let (v, dmg, size) = CHARGE_SHOT[lvl];
                let f = fwd(sp.yaw);
                let pos = at(m, sp.yaw, 180.0 * K, 90.0);
                let i = spawn(m, sp, ProjKind::ChargeShot, pos, [f[0] * v * KV, 0.0, f[1] * v * KV], 60, size * K * 0.5, dmg, HitKind::Normal, 0.0);
                m.smash.projectiles[i].level = lvl as u8;
                m.smash.samus_charge = 0;
                sfx(m, sp, SfxPhase::Release);
                // Recoil in the air (`FTSAMUS_CHARGE_RECOIL_*`).
                if sp.air {
                    let back = (10.0 + 2.0 * lvl as f64) * KV * 0.3;
                    m.vel[0] -= f[0] * back;
                    m.vel[2] -= f[1] * back;
                }
            }
            if sp.t + 1 >= ticks(31.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

/// Screw Attack: a lift of 62 (10 forward) with Samus's gravity (1.9), multi-hits, then the helpless fall (drift 0.66,
/// lag 0.4). From the ground it is the same jump.
fn screw_attack(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.phase != Phase::Start {
        return fall_special(m, sp);
    }
    let t = sp.total;
    if t == 0 {
        let f = fwd(sp.yaw);
        m.vel = [f[0] * 10.0 * KV, 62.0 * KV, f[1] * 10.0 * KV];
        sp.air = true;
        set_act(m, ACT_FREEFALL);
        sfx(m, sp, SfxPhase::Release);
    }
    drift(m, 20.0 * KV * 0.5, 0.6);
    let r = air(m, 1.9 * KA, TERMINAL, t > 10);
    if r == AIR_GRABBED_LEDGE {
        m.set_action(ACT_LEDGE_GRAB, 0.0);
        return Flow::Handed;
    }
    if (2..20).contains(&t) {
        hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 90.0), 120.0, 2.0, HitKind::Normal, 8.0, 3);
    }
    if r == AIR_LANDED && t > 2 {
        sp.air = false;
        return to_fall(m, sp, 0.66, 18.0);
    }
    if t + 1 >= ticks(49.0) {
        return to_fall(m, sp, 0.66, 18.0);
    }
    Flow::Continue
}

/// Bomb (`WPSAMUSBOMB_*`): Samus curls up and drops a bomb (gravity 1, 100-frame fuse, a 180-wide blast); in the air she
/// hops (40) once.
fn samus_bomb(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.total == 0 && sp.air {
        m.vel[1] = m.vel[1].max(0.0) + (40.0 - 10.0) * KV * 0.5;
    }
    hold(m, sp, if sp.air { 1.9 * KA } else { GRAVITY }, 0.66 * 20.0);
    if sp.total == 6 {
        let pos = at(m, sp.yaw, 0.0, 60.0 * K);
        spawn(m, sp, ProjKind::SamusBomb, pos, [0.0, 0.0, 0.0], ticks(100.0), 25.0, 5.0, HitKind::Explosion, 1.0 * KA);
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(58.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

// ----- Yoshi -----------------------------------------------------------------------------------

/// Egg Lay: the tongue darts out (frames ~10-20); a body it catches is swallowed into an egg (the mode decides: weak foes
/// go, others take 7% and a stun); then the release.
fn egg_lay(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 10.0);
    match sp.phase {
        Phase::Start => {
            let t = sp.t;
            if (4..10).contains(&t) {
                let reach = 60.0 + (t - 4) as f64 * 30.0;
                hitbox(m, sp, 1, at(m, sp.yaw, reach, 90.0), 60.0, 7.0, HitKind::Capture, 0.0, 0);
            }
            if sp.landed && t >= 6 {
                set_phase(sp, Phase::Caught);
                sfx(m, sp, SfxPhase::Special);
                return Flow::Continue;
            }
            if t + 1 >= ticks(39.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
        _ => {
            if sp.t + 1 >= ticks(36.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

/// Egg Throw (`WPEGGTHROW_*`): the egg leaves on frame ~30 in an arc (gravity 2.7, 50-frame life) and bursts (340 wide)
/// on contact; the stick tilts the throw. In the air the first throw gives a small hop.
fn egg_throw(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.total == 0 && sp.air && !m.smash.egg_hop_used {
        m.smash.egg_hop_used = true;
        m.vel[1] = m.vel[1].max(0.0) + 20.0;
    }
    hold(m, sp, GRAVITY, 12.0);
    if sp.total == 15 {
        let (d, mag) = stick_dir(m);
        let f = fwd(sp.yaw);
        let dir = if mag > 0.3 { d } else { f };
        let speed = 22.0 + 14.0 * mag;
        let pos = at(m, sp.yaw, 40.0, 150.0);
        spawn(m, sp, ProjKind::Egg, pos, [dir[0] * speed, 34.0, dir[1] * speed], 40, 40.0, 12.0, HitKind::Explosion, 2.7 * KA * 0.6);
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(73.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Yoshi Bomb: from the ground a hop (the 31-frame start), then the slam at a constant -150; on impact a 15% blast and
/// two stars sliding out front and back (`WPYOSHISTAR_*`: 30 speed, 16 frames).
fn yoshi_bomb(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start => {
            if !sp.air {
                sp.air = true;
                m.vel[1] = 42.0;
            }
            if sp.t < ticks(14.0) && sp.total < ticks(31.0) {
                m.vel[0] *= 0.8;
                m.vel[2] *= 0.8;
                let r = air(m, 3.5, TERMINAL, false);
                if r == AIR_LANDED {
                    set_phase(sp, Phase::Travel);
                }
                set_act(m, ACT_GROUND_POUND);
                return Flow::Continue;
            }
            set_phase(sp, Phase::Travel);
            Flow::Continue
        }
        Phase::Travel => {
            m.vel = [0.0, -150.0 * KV * 0.7, 0.0];
            set_act(m, ACT_GROUND_POUND);
            let r = air(m, 0.0, -150.0, false);
            m.vel[1] = -150.0 * KV * 0.7;
            hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 40.0), 80.0, 12.0, HitKind::Normal, 20.0, 0);
            if r == AIR_LANDED {
                sp.air = false;
                set_act(m, ACT_GROUND_POUND_LAND);
                let p = m.pos;
                hitbox(m, sp, 2, [p[0], p[1] + 40.0, p[2]], 130.0, 15.0, HitKind::Normal, 24.0, 0);
                let f = fwd(sp.yaw);
                for s in [1.0, -1.0] {
                    let pos = [p[0] + f[0] * s * 300.0 * K * 0.4, p[1] + 20.0 * K + 20.0, p[2] + f[1] * s * 300.0 * K * 0.4];
                    let i = spawn(m, sp, ProjKind::Star, pos, [f[0] * s * 30.0 * KV * 0.6, 0.0, f[1] * s * 30.0 * KV * 0.6], ticks(32.0), 35.0, 6.0, HitKind::Normal, 0.0);
                    m.smash.projectiles[i].pierce = true;
                }
                m.smash.events.push(SmashEvent::Explode { pos: p, radius: 130.0 });
                sfx(m, sp, SfxPhase::Land);
                set_phase(sp, Phase::Landing);
            }
            Flow::Continue
        }
        _ => {
            ground(m, 0.0);
            set_act(m, ACT_GROUND_POUND_LAND);
            if sp.t + 1 >= ticks(45.0) {
                Flow::End(ACT_IDLE)
            } else {
                Flow::Continue
            }
        }
    }
}

// ----- Kirby -----------------------------------------------------------------------------------

/// Inhale (`ftkirbyspecialn.c`): a 21-frame start, a 7-frame suction loop while special is held (a cone ahead pulls a
/// body in: captured), then the end. With a mouthful Kirby waits (`InhaleStuffed`); special spits it out as a star (10%,
/// `FTKIRBY_VACUUM_THROW_DAMAGE`), Z (or 40 frames) swallows it: a fighter's neutral special is copied.
fn inhale(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 6.0);
    match sp.phase {
        Phase::Start | Phase::Hold => {
            let t = sp.total;
            if t >= 6 {
                for (k, reach) in [(1u32, 90.0), (2, 170.0), (3, 250.0)] {
                    hitbox(m, sp, k, at(m, sp.yaw, reach, 80.0), 45.0 + reach * 0.3, 0.0, HitKind::Capture, 0.0, 0);
                }
            }
            if m.smash.mouth.is_some() {
                set_phase(sp, Phase::Caught);
                sfx(m, sp, SfxPhase::Special);
                return Flow::Continue;
            }
            if sp.phase == Phase::Start && sp.t + 1 >= ticks(21.0) {
                set_phase(sp, Phase::Hold);
            } else if sp.phase == Phase::Hold && !held(m) && sp.t >= ticks(14.0) {
                set_phase(sp, Phase::End);
            } else if sp.phase == Phase::Hold && sp.t >= 90 {
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        Phase::Caught => {
            if sp.t > ticks(10.0) && pressed(m) {
                set_phase(sp, Phase::Travel);
                return Flow::Continue;
            }
            if (sp.t > 2 && m.controller.button_pressed & Z_TRIG != 0) || sp.t >= ticks(80.0) {
                set_phase(sp, Phase::Landing);
            }
            Flow::Continue
        }
        Phase::Travel => {
            if sp.t == 4 {
                m.smash.mouth = None;
                let f = fwd(sp.yaw);
                let v = 120.0 * KV * 0.4;
                let pos = at(m, sp.yaw, 60.0, 80.0);
                spawn(m, sp, ProjKind::Star, pos, [f[0] * v, 0.0, f[1] * v], 24, 45.0, 10.0, HitKind::Normal, 0.0);
                sfx(m, sp, SfxPhase::Projectile);
            }
            if sp.t + 1 >= ticks(29.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
        Phase::Landing => {
            if sp.t == 0 {
                let copied = m.smash.mouth.take().flatten();
                if let Some(f) = copied {
                    m.smash.copy = Some(f);
                }
                m.smash.events.push(SmashEvent::Swallowed { copied });
                sfx(m, sp, SfxPhase::Special);
            }
            if sp.t + 1 >= ticks(21.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
        _ => {
            if sp.t + 1 >= ticks(21.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

/// Final Cutter: up 2100 root units (x size 0.91) by frame 29, then straight down; on landing a shock wave runs along the
/// floor (`WPFINALCUTTER_*`: 100 speed, 20 frames). Ends helpless if it never lands.
fn final_cutter(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start => {
            let t = sp.t;
            let rise_n = ticks(29.0);
            if t == 0 {
                m.vel = [0.0; 3];
                sfx(m, sp, SfxPhase::Release);
            }
            if t < rise_n {
                let up = rise(2100.0 * 0.91 * K, rise_n as f64, t);
                displace(m, sp, 1.5, up);
                hitbox(m, sp, 1, at(m, sp.yaw, 50.0, 90.0), 80.0, 2.0, HitKind::Normal, 6.0, 3);
            } else if t < rise_n + 3 {
                // A short hang at the top before the drop.
                displace(m, sp, 0.0, 0.5);
            } else {
                set_phase(sp, Phase::Travel);
            }
            Flow::Continue
        }
        Phase::Travel => {
            sp.air = true;
            m.vel = [0.0, -42.0, 0.0];
            let r = air(m, 0.0, -42.0, false);
            m.vel[1] = -42.0;
            hitbox(m, sp, 2, at(m, sp.yaw, 50.0, 60.0), 80.0, 6.0, HitKind::Normal, 10.0, 0);
            if r == AIR_LANDED {
                sp.air = false;
                set_act(m, ACT_IDLE);
                let f = fwd(sp.yaw);
                let v = 100.0 * KV * 0.5;
                let pos = at(m, sp.yaw, 60.0, 30.0);
                spawn(m, sp, ProjKind::Cutter, pos, [f[0] * v, 0.0, f[1] * v], ticks(20.0) * 2, 40.0, 3.0, HitKind::Normal, 0.0);
                sfx(m, sp, SfxPhase::Land);
                sp.land_lag = ticks(36.0);
                set_phase(sp, Phase::Landing);
            } else if sp.t >= 40 {
                sp.drift = 0.6;
                sp.land_lag = ticks(36.0);
                set_phase(sp, Phase::Fall);
            }
            Flow::Continue
        }
        _ => fall_special(m, sp),
    }
}

/// Stone: a 13-frame (24 in the air) change, then a rock that cannot be hurt, falling at up to -140 with an 18% hit
/// while it drops; special (after 18 frames) or 160 frames turn Kirby back (`StoneCancel`, 31 frames).
fn stone(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start => {
            if sp.air {
                m.vel[0] *= 0.7;
                m.vel[2] *= 0.7;
                m.vel[1] = m.vel[1].max(0.0);
                air(m, 0.5, TERMINAL, false);
            } else {
                ground(m, 0.0);
            }
            if sp.t + 1 >= ticks(if sp.air { 24.0 } else { 13.0 }) {
                set_phase(sp, Phase::Hold);
                sfx(m, sp, SfxPhase::Release);
            }
            Flow::Continue
        }
        Phase::Hold | Phase::Travel | Phase::Landing => {
            if sp.air {
                m.vel[0] = 0.0;
                m.vel[2] = 0.0;
                set_act(m, ACT_GROUND_POUND);
                let r = air(m, 140.0 * KA * 0.05, -140.0 * KV * 0.7, false);
                hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 40.0), 75.0, 18.0, HitKind::Normal, 22.0, 0);
                if r == AIR_LANDED {
                    sp.air = false;
                    set_act(m, ACT_GROUND_POUND_LAND);
                    sfx(m, sp, SfxPhase::Land);
                    m.smash.events.push(SmashEvent::Explode { pos: m.pos, radius: 90.0 });
                }
            } else {
                if !ground(m, approach(m.forward_vel, 0.0, 3.0)) {
                    sp.air = true;
                }
                if m.action != ACT_GROUND_POUND_LAND {
                    set_act(m, ACT_IDLE);
                }
            }
            let since = sp.total;
            if (since >= ticks(18.0) + ticks(13.0) && pressed(m)) || since >= ticks(160.0) {
                set_phase(sp, Phase::End);
                sfx(m, sp, SfxPhase::End);
            }
            Flow::Continue
        }
        _ => {
            hold(m, sp, GRAVITY, 6.0);
            if sp.t + 1 >= ticks(31.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

// ----- Fox -------------------------------------------------------------------------------------

/// Blaster (`WPBLASTER_VEL_X` 160): a fast laser from the gun (60 ahead), 3%, no flinch. Each press fires.
fn blaster(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 12.0);
    if sp.total == 3 {
        let f = fwd(sp.yaw);
        let v = 160.0 * KV * 0.6;
        let pos = at(m, sp.yaw, 60.0 * K + 30.0, 90.0);
        spawn(m, sp, ProjKind::Laser, pos, [f[0] * v, 0.0, f[1] * v], 30, 22.0, 3.0, HitKind::Normal, 0.0);
        sfx(m, sp, SfxPhase::Projectile);
    }
    // Fires again on a repeated press after the shot.
    if sp.total >= 8 && pressed(m) {
        sp.total = 0;
        sp.instance = m.smash.new_id();
        return Flow::Continue;
    }
    if sp.total + 1 >= ticks(if sp.air { 46.0 } else { 56.0 }) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Fire Fox (`ftfoxspecialhi.c`): 35 frames of flames in place (gravity after 15), then 30 frames of flight at 115 in
/// the stick's direction (straight up without one), a deceleration, and the helpless fall (drift 1.0, lag 0.34).
fn fire_fox(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start | Phase::Hold => {
            if sp.total == 0 {
                m.vel = [m.vel[0] * 0.3, 0.0, m.vel[2] * 0.3];
            }
            if sp.air || m.floor.is_none() {
                sp.air = true;
                let g = if sp.total >= ticks(15.0) { 0.6 } else { 0.0 };
                let r = air(m, g, -12.0, false);
                if r == AIR_LANDED {
                    sp.air = false;
                    set_act(m, ACT_IDLE);
                }
            } else {
                ground(m, 0.0);
                set_act(m, ACT_IDLE);
            }
            if sp.phase == Phase::Start && sp.total + 1 >= ticks(9.0) {
                set_phase(sp, Phase::Hold);
            }
            if sp.total.is_multiple_of(4) && sp.total > 2 {
                hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 80.0), 100.0, 2.0, HitKind::Fire, 3.0, 4);
            }
            if sp.total + 1 >= ticks(35.0) {
                // Aim: the stick's direction, 45 degrees up; straight up without a stick.
                let (d, mag) = stick_dir(m);
                if mag > 0.3 {
                    sp.yaw = atan2s(d[1], d[0]) as f64;
                    m.face_angle[1] = sp.yaw;
                    sp.dir = [sp.yaw, 45.0f64.to_radians()];
                } else {
                    sp.dir = [sp.yaw, 90.0f64.to_radians()];
                }
                sp.instance = m.smash.new_id();
                set_phase(sp, Phase::Attack);
                sfx(m, sp, SfxPhase::Release);
            }
            Flow::Continue
        }
        Phase::Attack | Phase::End => {
            let travel = ticks(30.0);
            let v = if sp.phase == Phase::Attack { 115.0 * KV * 0.5 } else { (115.0 * KV * 0.5 - 3.0357 * KV * 2.0 * sp.t as f64).max(0.0) };
            let f = fwd(sp.yaw);
            let (c, s) = (sp.dir[1].cos(), sp.dir[1].sin());
            m.vel = [f[0] * c * v, s * v, f[1] * c * v];
            sp.air = true;
            let r = air(m, 0.0, -200.0, false);
            m.vel[1] = s * v;
            if sp.phase == Phase::Attack {
                hitbox(m, sp, 2, at(m, sp.yaw, 0.0, 80.0), 90.0, 16.0, HitKind::Fire, 20.0, 0);
            }
            if r == AIR_LANDED && sp.dir[1] < 1.2 {
                sp.air = false;
                set_act(m, ACT_IDLE);
                sp.land_lag = ticks(21.0);
                set_phase(sp, Phase::Landing);
                return Flow::Continue;
            }
            if sp.phase == Phase::Attack && sp.t + 1 >= travel {
                set_phase(sp, Phase::End);
            } else if sp.phase == Phase::End && sp.t + 1 >= ticks(21.0) {
                return to_fall(m, sp, 1.0, 21.0);
            }
            Flow::Continue
        }
        _ => fall_special(m, sp),
    }
}

/// Reflector (`ftfoxspeciallw.c`): a 5-frame start (a 5% shine hit), held while special is held (18 frames at least),
/// then a 19-frame end. While up it sends enemy projectiles back (`SmashTargets::reflect`). In the air it hangs, then
/// falls at 0.8 gravity after 4 frames.
fn reflector(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.air {
        if sp.total == 0 {
            m.vel[1] = 0.0;
        }
        m.vel[0] *= 0.9;
        m.vel[2] *= 0.9;
        let g = if sp.total >= ticks(4.0) { 0.8 * KA * 0.5 } else { 0.0 };
        let r = air(m, g, -25.0, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        ground(m, 0.0);
        set_act(m, ACT_IDLE);
    }
    match sp.phase {
        Phase::Start => {
            if sp.t < 2 {
                hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 80.0), 75.0, 5.0, HitKind::Normal, 10.0, 0);
            }
            if sp.t + 1 >= ticks(5.0) {
                set_phase(sp, Phase::Hold);
            }
            Flow::Continue
        }
        Phase::Hold => {
            if sp.total >= ticks(18.0) && !held(m) {
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        _ => {
            if sp.t + 1 >= ticks(19.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

// ----- Pikachu ---------------------------------------------------------------------------------

/// Thunder Jolt (`WPPIKACHUJOLT_*`): a ball of electricity at 55 that crawls along the floor (down steps, up slopes), from
/// the air it drops at -45 degrees first; 100-frame life.
fn thunder_jolt(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 10.0);
    if sp.total == 9 {
        let f = fwd(sp.yaw);
        let v = 55.0 * KV * 0.5;
        let pos = at(m, sp.yaw, 70.0, if sp.air { 60.0 } else { 30.0 });
        let vel = if sp.air { [f[0] * v * 0.707, -v * 0.707, f[1] * v * 0.707] } else { [f[0] * v, 0.0, f[1] * v] };
        spawn(m, sp, ProjKind::ThunderJolt, pos, vel, ticks(100.0), 35.0, 8.0, HitKind::Electric, 1.5);
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(65.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Quick Attack (`ftpikachuspecialhi.c`): a short start, a zip at 90 in the stick's direction (up without one), a second
/// zip if the stick (or special) asks for one, then the helpless fall (drift 0.4, lag 0.4).
fn quick_attack(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    let zip = ticks(16.0);
    let aim = |m: &mut MarioRuntime, sp: &mut Special| {
        let (d, mag) = stick_dir(m);
        if mag > 0.3 {
            sp.yaw = atan2s(d[1], d[0]) as f64;
            m.face_angle[1] = sp.yaw;
            sp.dir = [sp.yaw, if m.controller.stick_y > STICK_SPECIAL { 50.0f64.to_radians() } else { 25.0f64.to_radians() }];
        } else {
            sp.dir = [sp.yaw, 90.0f64.to_radians()];
        }
    };
    match sp.phase {
        Phase::Start => {
            hold(m, sp, 0.5, 0.0);
            if sp.t + 1 >= 3 {
                aim(m, sp);
                set_phase(sp, Phase::Attack);
                sfx(m, sp, SfxPhase::Release);
            }
            Flow::Continue
        }
        Phase::Attack | Phase::Travel => {
            let v = 90.0 * KV * 0.42;
            let f = fwd(sp.yaw);
            let (c, s) = (sp.dir[1].cos(), sp.dir[1].sin());
            m.vel = [f[0] * c * v, s * v, f[1] * c * v];
            sp.air = true;
            let r = air(m, 0.0, -200.0, false);
            m.vel[1] = s * v;
            hitbox(m, sp, if sp.phase == Phase::Attack { 1 } else { 2 }, at(m, sp.yaw, 0.0, 50.0), 70.0, 4.0, HitKind::Electric, 8.0, 0);
            if sp.phase == Phase::Attack && sp.t > 2 && (pressed(m) || m.controller.button_pressed & A_BUTTON != 0) {
                sp.charge = 1;
            }
            if r == AIR_LANDED && s < 0.9 {
                sp.air = false;
                set_act(m, ACT_IDLE);
            }
            if sp.t + 1 >= zip {
                if sp.phase == Phase::Attack && sp.charge == 1 {
                    aim(m, sp);
                    set_phase(sp, Phase::Travel);
                    sfx(m, sp, SfxPhase::Release);
                } else {
                    m.vel = [m.vel[0] * 0.2, m.vel[1] * 0.2, m.vel[2] * 0.2];
                    if !sp.air {
                        return Flow::End(ACT_IDLE);
                    }
                    return to_fall(m, sp, 0.4, 18.0);
                }
            }
            Flow::Continue
        }
        _ => fall_special(m, sp),
    }
}

/// Thunder (`WPPIKACHUTHUNDER_*`, `FTPIKACHU_THUNDER_*`): after the 25-frame start a bolt falls from the sky at -450;
/// it hurts what it passes (8%) and, when it strikes Pikachu, bursts around him (17%) and bumps him up (20).
fn thunder(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, if sp.air && sp.phase == Phase::Attack { 0.5 * KA } else { GRAVITY }, 8.0);
    match sp.phase {
        Phase::Start => {
            if sp.t + 1 >= ticks(25.0) {
                let top = (m.pos[1] + 900.0).min(m.ceil_height - 20.0).max(m.pos[1] + 200.0);
                let pos = [m.pos[0], top, m.pos[2]];
                let i = spawn(m, sp, ProjKind::Thunderbolt, pos, [0.0, -150.0, 0.0], ticks(40.0) * 2, 45.0, 8.0, HitKind::Electric, 0.0);
                m.smash.projectiles[i].pierce = true;
                sfx(m, sp, SfxPhase::Projectile);
                set_phase(sp, Phase::Hold);
            }
            Flow::Continue
        }
        Phase::Hold => {
            // The bolt is stepped after Mario: it strikes when it reaches his head (`after_step` sets `landed`).
            let bolt = m.smash.projectiles.iter().any(|p| p.kind == ProjKind::Thunderbolt && !p.dead);
            if sp.landed {
                sp.landed = false;
                set_phase(sp, Phase::Attack);
                let c = at(m, sp.yaw, 0.0, 70.0);
                blast(m, sp.fighter, sp.mv, c, 150.0, 17.0, 3);
                m.vel[1] = m.vel[1].max(0.0) + 20.0 * KV;
                sfx(m, sp, SfxPhase::Hit);
            } else if !bolt || sp.t > 40 {
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        Phase::Attack => {
            if sp.t + 1 >= ticks(30.0) {
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        _ => {
            if sp.t + 1 >= ticks(39.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

// ----- Jigglypuff ------------------------------------------------------------------------------

/// Pound (SSB64's neutral special; Rollout is a later game's): a lunge of 456 root units (x size 1.05) over the 56-frame
/// clip with a 12% slap around frames 14-22; in the air it hangs a little (`FTPURIN_POUND_VEL_*`).
fn pound(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    let t = sp.total;
    let dur = ticks(56.0);
    let ahead = rise(456.0 * 1.05 * K, dur as f64, t);
    if sp.air {
        drift(m, 4.0, 0.5);
        let f = fwd(sp.yaw);
        m.vel[0] = f[0] * ahead;
        m.vel[2] = f[1] * ahead;
        if t == 0 {
            m.vel[1] = m.vel[1].max(0.0) * 0.3 + 6.0;
        }
        let r = air(m, 1.0, -30.0, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        displace(m, sp, ahead, 0.0);
        set_act(m, ACT_IDLE);
    }
    if (7..11).contains(&t) {
        hitbox(m, sp, 1, at(m, sp.yaw, 70.0, 60.0), 70.0, 12.0, HitKind::Normal, 18.0, 0);
    }
    if t + 1 >= dur {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Sing: 181 frames of song; bodies around Jigglypuff fall asleep (no damage).
fn sing(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, 1.2, 6.0);
    let t = sp.total;
    if (20..80).contains(&t) {
        hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 60.0), 230.0, 0.0, HitKind::Sleep, 0.0, 0);
        if t % 15 == 5 {
            sfx(m, sp, SfxPhase::Charge);
        }
    }
    if t + 1 >= ticks(181.0) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Rest: a tiny, huge-knockback hit on the first frames (20%), then Jigglypuff sleeps for the rest of the 251-frame clip:
/// no moving, no acting.
fn rest(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 0.0);
    let t = sp.total;
    if t < 2 {
        hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 50.0), 65.0, 20.0, HitKind::Normal, 45.0, 0);
    }
    if t == 2 {
        set_phase(sp, Phase::Sleep);
    }
    if t + 1 >= ticks(251.0) {
        sfx(m, sp, SfxPhase::End);
        done(sp)
    } else {
        Flow::Continue
    }
}

// ----- Captain Falcon --------------------------------------------------------------------------

/// Falcon Punch: 91 frames; Falcon winds up (the flame bird comes out around frame 53) and lunges 354 root units (x size
/// 1.05) by frame 73; the punch hits 25% on frames 53-60. In the air it keeps its height (`FTCAPTAIN_FALCONPUNCH_VEL_*`).
fn falcon_punch(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    let t = sp.total;
    let (hit_from, lunge_end, dur) = (ticks(53.0), ticks(73.0), ticks(91.0));
    let lunge_from = ticks(44.0);
    let ahead = if t >= lunge_from && t < lunge_end { rise(354.0 * 1.05 * K, (lunge_end - lunge_from) as f64, t - lunge_from) } else { 0.0 };
    if sp.air {
        let f = fwd(sp.yaw);
        m.vel[0] = f[0] * ahead + m.vel[0] * 0.92;
        m.vel[2] = f[1] * ahead + m.vel[2] * 0.92;
        if t == 0 {
            m.vel[1] = m.vel[1].max(0.0) * 0.5;
        }
        let r = air(m, 1.0, -40.0, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        displace(m, sp, ahead, 0.0);
        set_act(m, ACT_IDLE);
    }
    if t == hit_from - 4 {
        sfx(m, sp, SfxPhase::Release);
    }
    if t >= hit_from && t < hit_from + 4 {
        hitbox(m, sp, 1, at(m, sp.yaw, 110.0, 100.0), 90.0, 25.0, HitKind::Fire, 38.0, 0);
    }
    if t + 1 >= dur {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// Falcon Dive: up 1536 root units (x size 1.05) by frame 38 and forward 386; a body caught on the way up is grabbed and
/// blown off (a 10% blast) as Falcon jumps away. Then the helpless fall (drift 0.72, lag 0.65).
fn falcon_dive(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start | Phase::Attack => {
            let t = sp.total;
            let (peak, dur) = (ticks(38.0), ticks(66.0));
            if t == 0 {
                m.vel = [0.0; 3];
                set_phase(sp, Phase::Attack);
                sfx(m, sp, SfxPhase::Release);
            }
            let up = if t < peak { rise(1536.0 * 1.05 * K, peak as f64, t) } else { -2.0 };
            let ahead = rise(386.0 * 1.05 * K, dur as f64, t);
            displace(m, sp, ahead, up);
            if t >= 2 && t < peak {
                hitbox(m, sp, 1, at(m, sp.yaw, 60.0, 120.0), 70.0, 0.0, HitKind::Grab, 0.0, 0);
            }
            if t + 1 >= dur {
                return to_fall(m, sp, 0.72, 30.0);
            }
            Flow::Continue
        }
        Phase::Caught => {
            m.vel = [0.0; 3];
            sp.air = true;
            air(m, 0.0, 0.0, false);
            m.vel[1] = 0.0;
            if sp.t + 1 >= ticks(17.0) {
                let c = at(m, sp.yaw, 70.0, 100.0);
                sp.instance = m.smash.new_id();
                blast(m, sp.fighter, sp.mv, c, 90.0, 10.0, 3);
                m.vel = [-fwd(sp.yaw)[0] * 6.0, 30.0, -fwd(sp.yaw)[1] * 6.0];
                sfx(m, sp, SfxPhase::Hit);
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        Phase::End => {
            sp.air = true;
            let r = air(m, GRAVITY, TERMINAL, false);
            if r == AIR_LANDED {
                sp.air = false;
                return Flow::End(ACT_IDLE);
            }
            if sp.t + 1 >= ticks(30.0) {
                sp.drift = 0.72;
                sp.land_lag = ticks(30.0);
                set_phase(sp, Phase::Fall);
            }
            Flow::Continue
        }
        _ => fall_special(m, sp),
    }
}

/// Falcon Kick: on the ground a burning dash of 2705 root units (x size 1.05) over 75 frames (15% early, weaker late);
/// in the air a dive forward and down (960 / -2853 over 51 frames) that lands into `LandingDownSpecial` (46 frames).
fn falcon_kick(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    let t = sp.total;
    match sp.phase {
        Phase::Landing => {
            ground(m, approach(m.forward_vel, 0.0, 3.0));
            set_act(m, ACT_IDLE);
            if sp.t + 1 >= ticks(46.0) / 2 {
                return Flow::End(ACT_IDLE);
            }
            Flow::Continue
        }
        _ if sp.slot == SpecialSlot::Down && sp.charge == 0 && !sp.air => {
            let dur = ticks(86.0);
            let n = ticks(75.0);
            let ahead = if t < n { rise(2705.0 * 1.05 * K, n as f64, t) } else { 0.0 };
            if t == 3 {
                sfx(m, sp, SfxPhase::Release);
            }
            displace(m, sp, ahead, 0.0);
            if !sp.air {
                set_act(m, ACT_IDLE);
            }
            if (4..14).contains(&t) {
                hitbox(m, sp, 1, at(m, sp.yaw, 60.0, 50.0), 75.0, 15.0, HitKind::Fire, 22.0, 0);
            } else if (14..26).contains(&t) {
                hitbox(m, sp, 2, at(m, sp.yaw, 60.0, 50.0), 70.0, 9.0, HitKind::Fire, 12.0, 0);
            }
            if sp.air {
                // Ran off a ledge: carry on as the falling kick.
                sp.charge = 1;
            }
            if t + 1 >= dur {
                return done(sp);
            }
            Flow::Continue
        }
        _ => {
            sp.charge = 1;
            if !sp.air {
                sp.air = true;
            }
            let n = ticks(51.0) as f64;
            let ahead = (960.0 * 1.05 * K / n).max(4.0);
            let down = 2853.0 * 1.05 * K / n;
            let f = fwd(sp.yaw);
            m.vel = [f[0] * ahead, -down, f[1] * ahead];
            let r = air(m, 0.0, -300.0, false);
            m.vel[1] = -down;
            if t == 1 {
                sfx(m, sp, SfxPhase::Release);
            }
            hitbox(m, sp, 3, at(m, sp.yaw, 50.0, 40.0), 75.0, 15.0, HitKind::Fire, 20.0, 0);
            if r == AIR_LANDED {
                sp.air = false;
                set_act(m, ACT_IDLE);
                sfx(m, sp, SfxPhase::Land);
                set_phase(sp, Phase::Landing);
            } else if t > 90 {
                return done(sp);
            }
            Flow::Continue
        }
    }
}

// ----- Ness ------------------------------------------------------------------------------------

/// PK Fire (`ftnessspecialn.c`, `WPPKFIRE_*`): a spark at 73 (-3.6 degrees) from the ground, 95 (-38) from the air; on a
/// body or the floor it becomes a pillar of fire (multi-hit).
fn pk_fire(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    hold(m, sp, GRAVITY, 10.0);
    if sp.total == 9 {
        let f = fwd(sp.yaw);
        let (v, ang) = if sp.air { (95.0 * KV * 0.45, (-38.0f64).to_radians()) } else { (73.0 * KV * 0.45, (-3.6f64).to_radians()) };
        let pos = at(m, sp.yaw, 100.0 * K, 180.0 * K);
        spawn(m, sp, ProjKind::PkFireSpark, pos, [f[0] * v * ang.cos(), v * ang.sin(), f[1] * v * ang.cos()], 18, 30.0, 4.0, HitKind::Fire, 0.0);
        sfx(m, sp, SfxPhase::Projectile);
    }
    if sp.total + 1 >= ticks(if sp.air { 61.0 } else { 73.0 }) {
        done(sp)
    } else {
        Flow::Continue
    }
}

/// PK Thunder (`ftnessspecialhi.c`, `wpnesspkthunder.c`): Ness stands still (falls slowly after 25 frames) and steers a
/// ball of lightning (60 speed, 160-frame life, 6 degrees a frame) with the stick: left / right turn it, up / down pitch
/// it. It hurts what it touches (8%); steered into Ness it launches him (PK Thunder 2: 200, decelerating, 25%). Released or
/// expired: the end, and in the air the helpless fall (drift 0.6).
fn pk_thunder(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    match sp.phase {
        Phase::Start | Phase::Hold => {
            if sp.air {
                m.vel[0] *= 0.85;
                m.vel[2] *= 0.85;
                if sp.total == 0 {
                    m.vel[1] = 0.0;
                }
                let g = if sp.total >= ticks(25.0) { 0.4 } else { 0.0 };
                let r = air(m, g, -8.0, false);
                if r == AIR_LANDED {
                    sp.air = false;
                    set_act(m, ACT_IDLE);
                }
            } else {
                ground(m, 0.0);
                set_act(m, ACT_IDLE);
            }
            if sp.phase == Phase::Start && sp.t == ticks(14.0) {
                let pos = at(m, sp.yaw, 20.0, 200.0);
                let i = spawn(m, sp, ProjKind::PkThunder, pos, [0.0, 60.0 * KV * 0.5, 0.0], ticks(160.0), 40.0, 8.0, HitKind::Electric, 0.0);
                m.smash.projectiles[i].pierce = false;
                sfx(m, sp, SfxPhase::Projectile);
            }
            if sp.phase == Phase::Start && sp.t + 1 >= ticks(25.0) {
                set_phase(sp, Phase::Hold);
                return Flow::Continue;
            }
            if sp.landed {
                // The ball struck Ness: PK Thunder 2, along the ball's heading.
                sp.landed = false;
                sp.instance = m.smash.new_id();
                set_phase(sp, Phase::Travel);
                sfx(m, sp, SfxPhase::Release);
                return Flow::Continue;
            }
            let ball = m.smash.projectiles.iter().any(|p| p.kind == ProjKind::PkThunder && !p.dead);
            if sp.phase == Phase::Hold && (!ball || sp.t > ticks(160.0)) {
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        Phase::Travel => {
            let n = ticks(28.0 * 2.0);
            let v0 = 200.0 * KV * 0.4;
            let v = v0 * (1.0 - sp.t as f64 / n as f64).max(0.15);
            let (yaw, pitch) = (sp.dir[0], sp.dir[1]);
            let f = fwd(yaw);
            if pitch.cos().abs() > 0.2 {
                sp.yaw = yaw;
                m.face_angle[1] = yaw;
            }
            m.vel = [f[0] * pitch.cos() * v, pitch.sin() * v, f[1] * pitch.cos() * v];
            sp.air = true;
            let r = air(m, 0.0, -300.0, false);
            m.vel[1] = pitch.sin() * v;
            hitbox(m, sp, 1, at(m, sp.yaw, 0.0, 70.0), 85.0, 25.0, HitKind::Electric, 34.0, 0);
            if r == AIR_LANDED && pitch < 0.3 {
                sp.air = false;
                set_act(m, ACT_IDLE);
                sp.land_lag = ticks(30.0);
                set_phase(sp, Phase::Landing);
                return Flow::Continue;
            }
            if sp.t + 1 >= n {
                return to_fall(m, sp, 0.6, 30.0);
            }
            Flow::Continue
        }
        Phase::End => {
            hold(m, sp, 1.0, 6.0);
            if sp.t + 1 >= ticks(17.0) {
                if sp.air {
                    return to_fall(m, sp, 0.6, 30.0);
                }
                return done(sp);
            }
            Flow::Continue
        }
        _ => fall_special(m, sp),
    }
}

/// PSI Magnet: a 16-frame start, held while special is held (30 frames at least), a 12-frame end; energy projectiles that
/// reach it are absorbed and heal Ness. In the air it hangs, then falls slowly after 4 frames.
fn psi_magnet(m: &mut MarioRuntime, sp: &mut Special) -> Flow {
    if sp.air {
        if sp.total == 0 {
            m.vel[1] = 0.0;
        }
        m.vel[0] *= 0.9;
        m.vel[2] *= 0.9;
        let g = if sp.total >= ticks(4.0) { 0.5 } else { 0.0 };
        let r = air(m, g, -20.0, false);
        if r == AIR_LANDED {
            sp.air = false;
            set_act(m, ACT_IDLE);
        }
    } else {
        ground(m, 0.0);
        set_act(m, ACT_IDLE);
    }
    match sp.phase {
        Phase::Start => {
            if sp.t + 1 >= ticks(16.0) {
                set_phase(sp, Phase::Hold);
            }
            Flow::Continue
        }
        Phase::Hold => {
            if sp.total >= ticks(16.0) + ticks(30.0) && !held(m) {
                set_phase(sp, Phase::End);
            }
            Flow::Continue
        }
        _ => {
            if sp.t + 1 >= ticks(12.0) {
                done(sp)
            } else {
                Flow::Continue
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Projectiles, after Mario moved
// ---------------------------------------------------------------------------------------------

/// After Mario's step: move the projectiles (level geometry included), let PK Thunder / Thunder find Ness / Pikachu, then
/// hit the course's SM64 objects. The app resolves the other targets (goombas, a mode's enemies) with [`resolve`].
pub fn after_step(m: &mut MarioRuntime) {
    if m.smash.fighter.is_none() {
        if !m.smash.projectiles.is_empty() || m.smash.special.is_some() {
            m.smash.reset();
        }
        return;
    }
    m.smash.tick += 1;
    m.smash.center = [m.pos[0], m.pos[1] + 70.0, m.pos[2]];
    m.smash.yaw = m.face_angle[1];
    items::after(m);
    step_projectiles(m);
    normal_attacks(m);
    let mut ext = std::mem::take(&mut m.smash);
    resolve(&mut ext, &mut Sm64Objects { objects: &mut m.objects });
    ext.projectiles.retain(|p| !p.dead);
    m.smash = ext;
}

fn step_projectiles(m: &mut MarioRuntime) {
    let mut list = std::mem::take(&mut m.smash.projectiles);
    let mario_center = [m.pos[0], m.pos[1] + 80.0, m.pos[2]];
    let mut spawned: Vec<(ProjKind, Vec3, SmashFighter, SpecialMove)> = vec![];
    let mut blasts: Vec<(Vec3, f64, f32, SmashFighter, SpecialMove)> = vec![];
    let special = m.smash.special;
    for p in list.iter_mut() {
        if p.dead {
            continue;
        }
        p.age += 1;
        match p.kind {
            ProjKind::Boomerang => {
                if p.level == 0 {
                    // Out: decelerate, then turn back.
                    let sp = (p.vel[0].hypot(p.vel[2]) - 2.0).max(0.0);
                    let h = p.vel[0].hypot(p.vel[2]).max(1e-6);
                    p.vel = [p.vel[0] / h * sp, 0.0, p.vel[2] / h * sp];
                    if sp <= 0.5 || p.age > 24 {
                        p.level = 1;
                        p.damage = 8.0;
                        p.id = p.id.wrapping_add(1 << 20);
                    }
                } else {
                    let d = [mario_center[0] - p.pos[0], mario_center[1] + 20.0 - p.pos[1], mario_center[2] - p.pos[2]];
                    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
                    let sp = (p.vel[0].hypot(p.vel[2]).hypot(p.vel[1]) + 2.5).min(42.0);
                    p.vel = [d[0] / l * sp, d[1] / l * sp, d[2] / l * sp];
                    if l < 70.0 {
                        p.dead = true;
                        m.smash.events.push(SmashEvent::Sfx(SmashSfx { fighter: p.fighter, mv: p.mv, slot: SpecialSlot::Neutral, phase: SfxPhase::End }));
                        continue;
                    }
                }
            }
            ProjKind::PkThunder => {
                // Steering: stick x turns, stick y pitches (6 degrees a frame).
                let c = &m.controller;
                let speed = p.vel[0].hypot(p.vel[2]).hypot(p.vel[1]).max(1.0);
                let mut yaw = p.vel[0].atan2(p.vel[2]);
                let mut pitch = (p.vel[1] / speed).clamp(-1.0, 1.0).asin();
                let step = 12.0f64.to_radians();
                let horiz = p.vel[0].hypot(p.vel[2]);
                if horiz < 0.5 * speed && c.stick_mag > 20.0 {
                    // Nearly vertical: the stick picks the heading outright (camera relative), then pitches it over.
                    yaw = (m.intended_yaw * std::f64::consts::PI / 32768.0).rem_euclid(std::f64::consts::TAU);
                    let yaw_s16 = yaw * 32768.0 / std::f64::consts::PI;
                    let f = fwd(yaw_s16);
                    yaw = f[0].atan2(f[1]);
                    pitch -= step * (c.stick_mag / 64.0);
                } else {
                    yaw -= step * (c.stick_x / 64.0);
                    pitch += step * (c.stick_y / 64.0);
                }
                pitch = pitch.clamp(-1.45, 1.45);
                p.vel = [yaw.sin() * pitch.cos() * speed, pitch.sin() * speed, yaw.cos() * pitch.cos() * speed];
                // PK Thunder 2: the ball runs into Ness.
                let d = [p.pos[0] - mario_center[0], p.pos[1] - mario_center[1], p.pos[2] - mario_center[2]];
                if p.age > 8 && d[0].hypot(d[2]) < 250.0 * K * 0.5 && d[1].abs() < 370.0 * K * 0.4 {
                    if let Some(sp) = special {
                        if sp.mv == SpecialMove::PkThunder && matches!(sp.phase, Phase::Hold | Phase::Start) {
                            p.dead = true;
                            let heading = [-d[0], -d[1], -d[2]];
                            let h = heading[0].hypot(heading[2]);
                            let yaw16 = atan2s(heading[2], heading[0]) as f64;
                            let pitch = heading[1].atan2(h.max(1e-6));
                            if let Some(s) = m.smash.special.as_mut() {
                                s.landed = true;
                                s.dir = [yaw16, pitch];
                            }
                            continue;
                        }
                    }
                }
            }
            ProjKind::Thunderbolt => {
                // Strikes Pikachu when it reaches his head.
                let d = [p.pos[0] - m.pos[0], p.pos[2] - m.pos[2]];
                if d[0].hypot(d[1]) < 200.0 * K && p.pos[1] <= m.pos[1] + 160.0 && p.pos[1] > m.pos[1] - 40.0 {
                    if let Some(s) = m.smash.special.as_mut() {
                        if s.mv == SpecialMove::Thunder && s.phase == Phase::Hold {
                            s.landed = true;
                            p.dead = true;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        if p.kind != ProjKind::Blast {
            p.vel[1] = (p.vel[1] - p.gravity).max(p.terminal.min(p.vel[1]));
            let before = p.pos;
            for k in 0..3 {
                p.pos[k] += p.vel[k];
            }
            // Walls.
            if !matches!(p.kind, ProjKind::Thunderbolt | ProjKind::PkFirePillar | ProjKind::Item) {
                let mut probe = p.pos;
                let walls = m.world.f32_find_wall_collision(&mut probe, 0.0, p.radius.min(40.0));
                if walls > 0 {
                    match p.kind {
                        ProjKind::Boomerang => {
                            p.pos = probe;
                            if p.level == 0 {
                                p.level = 1;
                                p.id = p.id.wrapping_add(1 << 20);
                            }
                        }
                        ProjKind::SamusBomb => {
                            p.pos = probe;
                            p.vel[0] = 0.0;
                            p.vel[2] = 0.0;
                        }
                        ProjKind::LinkBomb | ProjKind::Egg => {
                            p.dead = true;
                            blasts.push((before, if p.kind == ProjKind::Egg { 340.0 * K * 0.5 } else { 100.0 }, p.damage, p.fighter, p.mv));
                            continue;
                        }
                        _ => {
                            p.dead = true;
                            m.smash.events.push(SmashEvent::Hit { pos: before, kind: p.hit, damage: 0.0 });
                            continue;
                        }
                    }
                }
            }
            // Floor.
            let floor = m.world.find_floor(p.pos[0], p.pos[1] + 80.0, p.pos[2]);
            let ground = floor.surface.is_some().then_some(floor.height);
            match p.kind {
                ProjKind::Fireball => {
                    if let Some(h) = ground {
                        if p.pos[1] - p.radius * 0.5 <= h {
                            p.pos[1] = h + p.radius * 0.5;
                            p.vel[1] = 18.0;
                        }
                    }
                }
                ProjKind::ThunderJolt | ProjKind::Cutter => {
                    // Crawl: stick to the floor under it; fall off edges.
                    let snap = m.world.find_floor(p.pos[0], before[1] + 60.0, p.pos[2]);
                    match snap.surface.is_some().then_some(snap.height) {
                        Some(h) if p.pos[1] - h < 70.0 && p.pos[1] - h > -60.0 => {
                            p.pos[1] = h + p.radius * 0.6;
                            p.vel[1] = 0.0;
                            let hs = p.vel[0].hypot(p.vel[2]);
                            if hs < 1.0 {
                                // An air jolt landing: run along its facing.
                                let f = [p.vel[0], p.vel[2]];
                                let l = f[0].hypot(f[1]).max(1e-6);
                                p.vel = [f[0] / l * 19.0, 0.0, f[1] / l * 19.0];
                            } else if hs < 19.0 && p.kind == ProjKind::ThunderJolt {
                                p.vel = [p.vel[0] / hs * 19.0, 0.0, p.vel[2] / hs * 19.0];
                            }
                        }
                        _ => {
                            if p.kind == ProjKind::Cutter {
                                p.dead = true;
                                continue;
                            }
                        }
                    }
                }
                ProjKind::PkFireSpark => {
                    if let Some(h) = ground {
                        if p.pos[1] <= h + 10.0 {
                            p.dead = true;
                            spawned.push((ProjKind::PkFirePillar, [p.pos[0], h, p.pos[2]], p.fighter, p.mv));
                            continue;
                        }
                    }
                }
                ProjKind::LinkBomb => {
                    if let Some(h) = ground {
                        if p.pos[1] - p.radius * 0.5 <= h {
                            p.dead = true;
                            blasts.push(([p.pos[0], h + 30.0, p.pos[2]], 100.0, p.damage, p.fighter, p.mv));
                            continue;
                        }
                    }
                }
                ProjKind::Egg => {
                    if let Some(h) = ground {
                        if p.pos[1] - p.radius * 0.5 <= h {
                            p.dead = true;
                            blasts.push(([p.pos[0], h + 40.0, p.pos[2]], 340.0 * K * 0.5, p.damage, p.fighter, p.mv));
                            continue;
                        }
                    }
                }
                ProjKind::SamusBomb => {
                    if let Some(h) = ground {
                        if p.pos[1] - p.radius <= h {
                            p.pos[1] = h + p.radius;
                            p.vel = [p.vel[0] * 0.5, 0.0, p.vel[2] * 0.5];
                        }
                    }
                }
                ProjKind::Thunderbolt => {
                    if let Some(h) = ground {
                        if p.pos[1] <= h {
                            p.dead = true;
                            m.smash.events.push(SmashEvent::Hit { pos: [p.pos[0], h, p.pos[2]], kind: HitKind::Electric, damage: 0.0 });
                            continue;
                        }
                    }
                }
                ProjKind::Star | ProjKind::Laser | ProjKind::GreenFireball | ProjKind::ChargeShot | ProjKind::PkThunder => {
                    if let Some(h) = ground {
                        if p.pos[1] < h - 5.0 && p.kind != ProjKind::Star {
                            p.dead = true;
                            continue;
                        }
                        if p.kind == ProjKind::Star && p.pos[1] < h + p.radius * 0.5 {
                            p.pos[1] = h + p.radius * 0.5;
                        }
                    }
                }
                _ => {}
            }
        }
        if p.age >= p.life {
            p.dead = true;
            match p.kind {
                ProjKind::LinkBomb => blasts.push((p.pos, 100.0, p.damage, p.fighter, p.mv)),
                ProjKind::SamusBomb => blasts.push((p.pos, 180.0 * K, p.damage, p.fighter, p.mv)),
                ProjKind::Egg => blasts.push((p.pos, 340.0 * K * 0.5, p.damage, p.fighter, p.mv)),
                _ => {}
            }
        }
    }
    m.smash.projectiles = list;
    for (pos, r, dmg, f, mv) in blasts {
        blast(m, f, mv, pos, r, dmg, 3);
        m.smash.events.push(SmashEvent::Sfx(SmashSfx { fighter: f, mv, slot: slot_of(f, mv), phase: SfxPhase::Explode }));
    }
    for (kind, pos, f, mv) in spawned {
        if kind == ProjKind::PkFirePillar {
            m.smash.add_pillar(f, mv, pos);
        }
    }
}

fn slot_of(f: SmashFighter, mv: SpecialMove) -> SpecialSlot {
    [SpecialSlot::Neutral, SpecialSlot::Up, SpecialSlot::Down].into_iter().find(|s| f.special(*s) == mv).unwrap_or(SpecialSlot::Neutral)
}

/// Whether a sphere touches a target cylinder.
fn touches(c: Vec3, r: f64, t: &Target) -> bool {
    let (dx, dz) = (c[0] - t.pos[0], c[2] - t.pos[2]);
    let rr = r + t.radius;
    dx * dx + dz * dz < rr * rr && c[1] + r > t.pos[1] && c[1] - r < t.pos[1] + t.height
}

/// Resolve this tick's hitboxes and projectiles against a set of targets (and the Reflector / PSI Magnet against its
/// incoming projectiles). Safe to call with several target sets in one tick.
pub fn resolve(ext: &mut SmashExt, t: &mut dyn SmashTargets) {
    let Some(_) = ext.fighter else { return };
    let now = ext.tick;
    // Reflector and PSI Magnet.
    if ext.reflecting() || ext.absorbing() {
        let reflect = ext.reflecting();
        let c = ext.center;
        let f = fwd(ext.yaw);
        for inc in t.incoming() {
            let d = [inc.pos[0] - c[0], inc.pos[1] - c[1], inc.pos[2] - c[2]];
            let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if reflect && dist < 110.0 + inc.radius {
                let v = [-inc.vel[0] * 1.5, inc.vel[1].abs() * 0.2, -inc.vel[2] * 1.5];
                t.reflect(inc.id, v);
                ext.events.push(SmashEvent::Reflected { pos: inc.pos });
                if let Some(sp) = ext.special {
                    ext.events.push(SmashEvent::Sfx(SmashSfx { fighter: sp.fighter, mv: sp.mv, slot: sp.slot, phase: SfxPhase::Special }));
                }
            } else if !reflect && dist < 120.0 + inc.radius && (d[0] * f[0] + d[2] * f[1]) > -30.0 && inc.energy {
                t.absorb(inc.id);
                ext.heal += 0x40 as f64;
                ext.events.push(SmashEvent::Absorbed { pos: inc.pos });
                if let Some(sp) = ext.special {
                    ext.events.push(SmashEvent::Sfx(SmashSfx { fighter: sp.fighter, mv: sp.mv, slot: sp.slot, phase: SfxPhase::Special }));
                }
            }
        }
    }
    let targets = t.targets();
    if targets.is_empty() {
        return;
    }
    let fighter = ext.special.map(|s| s.copied.unwrap_or(s.fighter)).or(ext.fighter).unwrap_or(SmashFighter::Mario);
    let mv = ext.special.map(|s| s.mv).or((!ext.hitboxes.is_empty()).then_some(SpecialMove::Item));
    let center = ext.center;
    let mut log = std::mem::take(&mut ext.hit_log);
    log.retain(|(_, _, at)| now.saturating_sub(*at) < 400);
    let can_hit = |log: &Vec<(u32, u64, u64)>, src: u32, key: u64, rehit: u32| -> bool {
        match log.iter().rev().find(|(s, k, _)| *s == src && *k == key) {
            None => true,
            Some((_, _, at)) => rehit > 0 && now.saturating_sub(*at) >= rehit as u64,
        }
    };
    let dir_from = |from: Vec3, to: Vec3| -> Vec3 {
        let d = [to[0] - from[0], 0.0, to[2] - from[2]];
        let l = d[0].hypot(d[2]);
        if l < 1e-3 {
            [0.0, 1.0, 0.0]
        } else {
            let k = 0.8 / l;
            [d[0] * k, 0.6, d[2] * k]
        }
    };
    let mut gone: Vec<bool> = vec![false; targets.len()];
    // Hitboxes of the special and of the normal attack.
    let hitboxes = ext.hitboxes.clone();
    for hb in &hitboxes {
        let hit_mv = if hb.normal { None } else { mv };
        if !hb.normal && mv.is_none() {
            continue;
        }
        for (i, tg) in targets.iter().enumerate() {
            if gone[i] || !touches(hb.pos, hb.radius, tg) || !can_hit(&log, hb.source, tg.key, hb.rehit) {
                continue;
            }
            let hit = SmashHit { damage: hb.damage, kind: hb.kind, dir: dir_from(center, tg.pos), knock: hb.knock, pos: hb.pos, fighter, mv: hit_mv, knockback: Some(hb.knockback.or_else(|| hit_mv.and_then(|mv| crate::smash_knockback::weapon(mv, None, hb.damage).filter(|_| mv == SpecialMove::Item))).unwrap_or_else(|| crate::smash_knockback::approximate(hb.damage, hb.kind))), motion: hb.motion };
            let out = t.hit(i, &hit);
            log.push((hb.source, tg.key, now));
            if out != HitOutcome::Miss {
                if !hb.normal {
                    ext.landed.push((hb.source, hb.kind, out));
                }
                if hb.kind != HitKind::Sleep && hb.kind != HitKind::Capture && hb.kind != HitKind::Grab {
                    ext.events.push(SmashEvent::Hit { pos: [tg.pos[0], tg.pos[1] + tg.height * 0.5, tg.pos[2]], kind: hb.kind, damage: hb.damage });
                }
                if matches!(out, HitOutcome::Killed | HitOutcome::Captured(_)) {
                    gone[i] = true;
                }
                if let (Some(sp), false) = (ext.special, hb.normal) {
                    if hb.kind != HitKind::Sleep {
                        ext.events.push(SmashEvent::Sfx(SmashSfx { fighter: sp.copied.unwrap_or(sp.fighter), mv: sp.mv, slot: sp.slot, phase: SfxPhase::Hit }));
                    }
                }
            }
        }
    }
    // Projectiles.
    let mut pillars: Vec<(Vec3, SmashFighter, SpecialMove)> = vec![];
    let mut blasts: Vec<(Vec3, f64, f32, SmashFighter, SpecialMove)> = vec![];
    for p in ext.projectiles.iter_mut() {
        if p.dead {
            continue;
        }
        let src = p.id;
        for (i, tg) in targets.iter().enumerate() {
            if p.dead || gone[i] || !touches(p.pos, p.radius, tg) || !can_hit(&log, src, tg.key, p.rehit) {
                continue;
            }
            let hit = SmashHit { damage: p.damage, kind: p.hit, dir: dir_from(p.pos, tg.pos), knock: 10.0 + p.damage as f64, pos: p.pos, fighter: p.fighter, mv: Some(p.mv), knockback: Some(crate::smash_knockback::weapon(p.mv, Some(p.kind), p.damage).unwrap_or_else(|| crate::smash_knockback::approximate(p.damage, p.hit))), motion: None };
            let out = t.hit(i, &hit);
            log.push((src, tg.key, now));
            if out == HitOutcome::Miss {
                continue;
            }
            if matches!(out, HitOutcome::Killed | HitOutcome::Captured(_)) {
                gone[i] = true;
            }
            ext.events.push(SmashEvent::Hit { pos: p.pos, kind: p.hit, damage: p.damage });
            ext.events.push(SmashEvent::Sfx(SmashSfx { fighter: p.fighter, mv: p.mv, slot: slot_of(p.fighter, p.mv), phase: SfxPhase::Hit }));
            match p.kind {
                ProjKind::Boomerang => {
                    if p.level == 0 {
                        p.level = 1;
                        p.damage = 8.0;
                        p.id = p.id.wrapping_add(1 << 20);
                    }
                }
                ProjKind::PkFireSpark => {
                    p.dead = true;
                    pillars.push((tg.pos, p.fighter, p.mv));
                }
                ProjKind::LinkBomb | ProjKind::Egg | ProjKind::SamusBomb => {
                    p.dead = true;
                    blasts.push((p.pos, if p.kind == ProjKind::Egg { 340.0 * K * 0.5 } else { 100.0 }, p.damage * 0.5, p.fighter, p.mv));
                }
                _ => {
                    if !p.pierce {
                        p.dead = true;
                    }
                }
            }
        }
    }
    ext.hit_log = log;
    for (pos, f, mv) in pillars {
        ext.add_pillar(f, mv, pos);
    }
    for (pos, r, dmg, f, mv) in blasts {
        ext.add_blast(f, mv, pos, r, dmg, 3);
    }
}



/// The course's SM64 objects as targets: what Banjo's eggs and Mario's punches can hit (`INTERACT_DAMAGE`, breakables,
/// grabbables), attacked like a punch.
struct Sm64Objects<'a> {
    objects: &'a mut crate::object_manager::ObjectManager,
}

impl SmashTargets for Sm64Objects<'_> {
    fn targets(&self) -> Vec<Target> {
        let mut out = vec![];
        for i in 0..self.objects.len() {
            let o = &self.objects[ObjId(i as u32)];
            if !o.active || o.held_state == HELD_HELD {
                continue;
            }
            if o.hitbox.interact_type & (INTERACT_DAMAGE | INTERACT_BREAKABLE | INTERACT_GRABBABLE) == 0 {
                continue;
            }
            out.push(Target { key: (1u64 << 40) | i as u64, pos: [o.pos[0], o.pos[1] - o.hitbox.down_offset, o.pos[2]], radius: o.hitbox.radius, height: o.hitbox.height, fighter: None });
        }
        out
    }

    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        // a normal attack is Mario's own punch / kick on the SM64 side, which already hits these objects
        if hit.mv.is_none() || hit.kind == HitKind::Sleep || (hit.damage <= 0.0 && hit.kind != HitKind::Capture) {
            return HitOutcome::Miss;
        }
        // `targets()` order: walk it again to find the object.
        let mut n = 0;
        for k in 0..self.objects.len() {
            let id = ObjId(k as u32);
            let o = &self.objects[id];
            if !o.active || o.held_state == HELD_HELD || o.hitbox.interact_type & (INTERACT_DAMAGE | INTERACT_BREAKABLE | INTERACT_GRABBABLE) == 0 {
                continue;
            }
            if n == i {
                let t = o.hitbox.interact_type;
                let o = &mut self.objects[id];
                attack_object(o, ATTACK_PUNCH);
                if t & INTERACT_GRABBABLE != 0 {
                    o.interact_status |= INT_STATUS_MARIO_KNOCKBACK_DMG;
                }
                return HitOutcome::Hit;
            }
            n += 1;
        }
        HitOutcome::Miss
    }
}

/// SSB64 items for the Smash fighters (`smash_items.rs`).
#[path = "smash_items.rs"]
pub mod items;
