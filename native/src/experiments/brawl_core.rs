//! Arena Brawl, rules (engine-free, seeded, 30 Hz): fighters with SM64-flavoured moves (jump,
//! double jump, punch-punch-kick, dive, ground pound), throwable crates / barrels / rocks / bombs,
//! a hammer, a boomerang and a blaster, three gems per round that make a timed power form,
//! best-of-three rounds, a quick Versus mode and the single-player Gauntlet (escalating bots and
//! a boss). Everything is plain data advanced by [`Brawl::tick`]; `brawl_ai.rs` drives the bots,
//! `brawl.rs` / `brawl_view.rs` draw and feed it.
//!
//! An original design in the "grab anything, collect the gems, transform and fight" arena-brawler
//! tradition: its rules, arenas, items and names are our own.
use super::brawl_arena::*;
pub use super::brawl_hero::{Blow, Hero, HeroStyle, HEROES};
pub use super::brawl_ai::{Brain, Diff};

#[path = "brawl_foes.rs"]
mod foes;
#[path = "brawl_hazard.rs"]
mod hazard;
#[path = "brawl_body.rs"]
mod body;
pub use body::Solid;
pub use hazard::{build_hazards, Hazard, HazardDef, HzKind, HzState, Look, Spec, Zone, HAZARD_GRACE, MAXH};
pub use foes::{roster, visual_radius, Mon, Monsters, MAXM, MON_RESPAWN};


pub const MAXF: usize = 4;
pub const MAXI: usize = 24;
pub const MAXP: usize = 24;
pub const GEMS: usize = 3;
pub const GRAV: f32 = 3300.0;
pub const JUMP_V: f32 = 1130.0;
/// Run speed (units a second). Power Stone pace: a fighter crosses a 1600-unit arena in about two seconds.
pub const RUN: f32 = 760.0;
/// Ground acceleration and braking (units a second, per second): full speed in three ticks, a dead stop in three.
pub const ACCEL_GROUND: f32 = 8400.0;
pub const BRAKE: f32 = 9000.0;
/// Steering in the air.
pub const ACCEL_AIR: f32 = 3400.0;
/// Pulling the stick against your own motion turns you round this much harder (a snap reversal, not a skid).
pub const TURN_BOOST: f32 = 2.0;
/// Most the body turns toward the stick in one tick (radians): about three ticks for a half turn.
pub const YAW_STEP: f32 = 1.1;
pub const RADIUS: f32 = 38.0;
pub const HEIGHT: f32 = 150.0;
pub const POWER_SECS: f32 = 14.0;
pub const POWER_SIZE: f32 = 1.5;
pub const ROUND_SECS: f32 = 100.0;
pub const INTRO_TICKS: u32 = 96;
pub const END_TICKS: u32 = 105;
pub const MAX_ROUNDS: u32 = 6;
pub const GAUNTLET_STAGES: usize = 6;
pub const GAUNTLET_LIVES: u8 = 3;
/// Seconds of power form each special burns. Lower costs keep the window near 8-15 s even when it is spent on specials
/// (it was ~6 s in bot matches with 2.5 / 1.6); the power form's blows hit 1.35x (was 1.5x) so the longer window is
/// still a comeback, not an automatic knockout (MINIGAMES_AUDIT_3.md).
pub const NOVA_COST: f32 = 1.6;
pub const BEAM_COST: f32 = 1.0;
pub const POWER_HIT: f32 = 1.35;
/// Ticks after a blow during which a hazard / fall still credits the knock-out to whoever landed it (4 s).
pub const CREDIT_TICKS: u32 = 120;
/// Quicksand depth (2 per tick standing still, -1 per tick running) at which it spits a fighter out: 1.5 s of standing.
pub const SINK_TICKS: u16 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mario,
    Goomba,
    Bomb,
    Koopa,
    Bully,
    Chuckya,
    KingBomb,
}

pub struct Stats {
    pub hp: f32,
    pub speed: f32,
    pub power: f32,
    pub weight: f32,
    pub scale: f32,
    pub name: &'static str,
}

impl Kind {
    pub fn stats(self) -> Stats {
        let s = |hp, speed, power, weight, scale, name| Stats { hp, speed, power, weight, scale, name };
        match self {
            Kind::Mario => s(100.0, 1.0, 1.0, 1.0, 1.0, "Mario"),
            Kind::Goomba => s(100.0, 1.0, 1.0, 1.0, 1.0, "Goomba"),
            Kind::Bomb => s(100.0, 1.0, 1.0, 1.0, 1.0, "Bob-omb"),
            Kind::Koopa => s(100.0, 1.0, 1.0, 1.0, 1.0, "Koopa"),
            Kind::Bully => s(100.0, 1.0, 1.0, 1.0, 1.0, "Bully"),
            Kind::Chuckya => s(100.0, 1.0, 1.0, 1.0, 1.0, "Chuckya"),
            Kind::KingBomb => s(340.0, 0.82, 1.3, 2.3, 2.3, "King Bob-omb"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Crate,
    Barrel,
    Rock,
    Bomb,
    Hammer,
    Boomerang,
    Blaster,
    Heal,
    /// Boots: a burst of speed for a few seconds (picked up by touch).
    Boots,
    /// Bubble: a shield that cuts the damage taken (picked up by touch).
    Shield,
}

impl ItemKind {
    /// Consumed on touch (hearts, boots, bubbles): never carried or thrown.
    pub fn pickup(self) -> bool {
        matches!(self, Self::Heal | Self::Boots | Self::Shield)
    }
    pub fn weapon(self) -> bool {
        matches!(self, Self::Hammer | Self::Boomerang | Self::Blaster)
    }
    pub fn radius(self) -> f32 {
        match self {
            Self::Barrel => 46.0,
            Self::Crate => 42.0,
            Self::Rock => 38.0,
            _ => 34.0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Crate => "Crate",
            Self::Barrel => "Barrel",
            Self::Rock => "Rock",
            Self::Bomb => "Bob-omb",
            Self::Hammer => "Hammer",
            Self::Boomerang => "Boomerang",
            Self::Blaster => "Blaster",
            Self::Heal => "1-Up Mushroom",
            Self::Boots => "Winged Boots",
            Self::Shield => "Bubble",
        }
    }
    /// (damage, knockback, lift, throw speed) of the thrown item.
    pub fn throw_stats(self) -> (f32, f32, f32, f32) {
        match self {
            // A prop that connects is worth at least a three-blow punch combo (punch, punch, kick lands 24 with the combo
            // bonus): Power Stone's "the map is the arsenal" (MINIGAMES_AUDIT_3.md; it was 11 / 16 / 14).
            Self::Crate => (24.0, 560.0, 260.0, 980.0),
            Self::Barrel => (27.0, 720.0, 280.0, 760.0),
            Self::Rock => (25.0, 640.0, 260.0, 880.0),
            Self::Bomb => (0.0, 0.0, 0.0, 780.0),
            _ => (6.0, 300.0, 120.0, 800.0),
        }
    }
    fn uses(self) -> u8 {
        match self {
            Self::Hammer => 4,
            Self::Boomerang => 6,
            Self::Blaster => 12,
            _ => 1,
        }
    }
}

const DROP_TABLE: [(ItemKind, u32); 10] = [(ItemKind::Crate, 4), (ItemKind::Barrel, 3), (ItemKind::Rock, 3), (ItemKind::Bomb, 3), (ItemKind::Hammer, 2), (ItemKind::Boomerang, 2), (ItemKind::Blaster, 2), (ItemKind::Heal, 2), (ItemKind::Boots, 1), (ItemKind::Shield, 1)];

/// Ticks (30 per second) of each timed pickup and of a combo window.
pub const HASTE_TICKS: u16 = 300;
pub const SHIELD_TICKS: u16 = 270;
pub const COMBO_WINDOW: u16 = 24;
/// Seconds before the bell at which a real-world arena's rim starts closing in.
pub const RIM_CLOSE_SECS: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IState {
    Rest,
    Held(u8),
    Flying,
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub alive: bool,
    pub kind: ItemKind,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub state: IState,
    pub uses: u8,
    /// Bomb fuse (ticks); 0 = unlit.
    pub fuse: i16,
    pub owner: i8,
    pub grace: u8,
    pub spin: f32,
    pub grounded: bool,
}

impl Item {
    const NONE: Item = Item { alive: false, kind: ItemKind::Crate, pos: [0.0; 3], vel: [0.0; 3], state: IState::Rest, uses: 0, fuse: 0, owner: -1, grace: 0, spin: 0.0, grounded: false };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PKind {
    Bullet,
    Boomerang,
    Shell,
    /// A monster's rock (damage in `mask`): straight, hits the first fighter.
    Rock,
    /// A monster's lobbed snowball (damage in `mask`, lands at `aim`).
    Snow,
}

#[derive(Clone, Copy, Debug)]
pub struct Proj {
    pub alive: bool,
    pub kind: PKind,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub owner: i8,
    pub t: i16,
    pub mask: u8,
    /// Shell: where it will land.
    pub aim: [f32; 3],
}

impl Proj {
    const NONE: Proj = Proj { alive: false, kind: PKind::Bullet, pos: [0.0; 3], vel: [0.0; 3], owner: -1, t: 0, mask: 0, aim: [0.0; 3] };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GemState {
    /// Falls from the sky at this tick.
    Pending(u32),
    Free,
    Held(u8),
    /// Used up in a transformation until the next batch.
    Spent,
}

#[derive(Clone, Copy, Debug)]
pub struct Gem {
    pub state: GemState,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub spot: usize,
    pub immune: i8,
    pub immune_t: u8,
}

pub const GEM_COLORS: [[f32; 3]; GEMS] = [[1.0, 0.25, 0.2], [0.25, 0.55, 1.0], [0.3, 0.95, 0.4]];
pub const GEM_NAMES: [&str; GEMS] = ["Ember", "Tide", "Leaf"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Idle,
    Air,
    Punch(u8),
    Dive,
    DiveSlide,
    PoundWind,
    Pound,
    PoundLand,
    Swing,
    /// Link's sword spin (hold B after the first blow): hits all around.
    Spin,
    Shoot,
    Toss,
    Nova,
    Beam,
    Hurt,
    Down,
    GetUp,
    Dead,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Wanted direction in the world (x, z), length 0..1.
    pub mv: V2,
    pub jump: bool,
    pub jump_held: bool,
    pub attack: bool,
    /// B is still held (Link: hold after the first blow for the spin).
    pub attack_held: bool,
    /// Z: ground pound in the air, grab / throw on the ground.
    pub z: bool,
    /// Explicit grab / throw (works in the air too).
    pub grab: bool,
    pub s1: bool,
    pub s2: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Fighter {
    pub alive: bool,
    pub kind: Kind,
    /// Which playable style a `Kind::Mario` fighter uses.
    pub hero: Hero,
    pub team: u8,
    pub human: bool,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub yaw: f32,
    pub grounded: bool,
    pub support: i16,
    pub hp: f32,
    pub max_hp: f32,
    pub act: Act,
    pub act_t: u16,
    pub stun: u16,
    pub inv: u16,
    pub jumps: u8,
    pub coyote: u8,
    pub hit_mask: u8,
    pub buffer: bool,
    pub held: i8,
    pub boom_out: bool,
    pub gems: u8,
    pub power: f32,
    pub size: f32,
    pub cd_s1: u16,
    pub cd_s2: u16,
    pub cd_grab: u16,
    pub cd_shoot: u16,
    pub burn: u8,
    pub last_hit_by: i8,
    pub ko_tick: u32,
    pub dead_t: u16,
    pub wins: u8,
    pub kos: u16,
    pub dmg_dealt: f32,
    pub transforms: u8,
    pub gems_got: u8,
    pub lunge: f32,
    /// Impact freeze (ticks): both fighters of a solid hit hold still for a beat so the blow lands.
    pub hitstop: u8,
    /// Consecutive hits landed (resets when `combo_t` runs out) and the window left to extend it.
    pub combo: u8,
    pub combo_t: u16,
    /// Ticks left in which a launched fighter splats against a wall.
    pub splat: u8,
    /// Boots (speed) and bubble (damage cut) pickups, in ticks.
    pub haste: u16,
    pub shield: u16,
    /// Tick of the last blow a fighter landed on this one (a hazard or a fall soon after still credits them).
    pub hit_tick: u32,
    /// Ticks stood in quicksand (the course's own surfaces) since the last time it spat this fighter out.
    pub sink: u16,
}

impl Fighter {
    fn blank() -> Self {
        Self {
            alive: false,
            kind: Kind::Mario,
            hero: Hero::Mario,
            team: 0,
            human: false,
            pos: [0.0; 3],
            vel: [0.0; 3],
            yaw: 0.0,
            grounded: true,
            support: -1,
            hp: 100.0,
            max_hp: 100.0,
            act: Act::Idle,
            act_t: 0,
            stun: 0,
            inv: 0,
            jumps: 0,
            coyote: 0,
            hit_mask: 0,
            buffer: false,
            held: -1,
            boom_out: false,
            gems: 0,
            power: 0.0,
            size: 1.0,
            cd_s1: 0,
            cd_s2: 0,
            cd_grab: 0,
            cd_shoot: 0,
            burn: 0,
            last_hit_by: -1,
            ko_tick: 0,
            dead_t: 0,
            wins: 0,
            kos: 0,
            dmg_dealt: 0.0,
            transforms: 0,
            gems_got: 0,
            lunge: 0.0,
            hitstop: 0,
            combo: 0,
            combo_t: 0,
            splat: 0,
            haste: 0,
            shield: 0,
            hit_tick: 0,
            sink: 0,
        }
    }
    /// Kind's stats with the hero's style laid over them (the player's fighter).
    pub fn stats(&self) -> Stats {
        let mut st = self.kind.stats();
        if self.kind == Kind::Mario {
            let h = self.hero.style();
            st.hp = h.hp;
            st.speed = h.speed;
            st.power = h.power;
            st.weight = h.weight;
            st.scale = h.scale;
            st.name = h.name;
        }
        st
    }
    pub fn radius(&self) -> f32 {
        RADIUS * self.size * (if self.kind == Kind::KingBomb { 1.5 } else { 1.0 })
    }
    pub fn height(&self) -> f32 {
        HEIGHT * self.size
    }
    pub fn hp_frac(&self) -> f32 {
        (self.hp / self.max_hp).clamp(0.0, 1.0)
    }
    pub fn powered(&self) -> bool {
        self.power > 0.0
    }
    pub fn speed2(&self) -> f32 {
        self.vel[0].hypot(self.vel[2])
    }
    pub fn gem_count(&self) -> u32 {
        self.gems.count_ones()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Versus,
    Gauntlet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Intro,
    Fight,
    RoundEnd,
    MatchEnd,
}

#[derive(Clone, Copy, Debug)]
pub enum Ev {
    Jump(usize),
    Punch { id: usize, step: u8 },
    Swing(usize),
    Dive(usize),
    PoundStart(usize),
    PoundLand { pos: [f32; 3], r: f32 },
    Hit { pos: [f32; 3], big: bool, victim: usize },
    Ko { id: usize, pos: [f32; 3] },
    Grab(usize),
    Throw(usize),
    Break([f32; 3]),
    Boom { pos: [f32; 3], r: f32 },
    Gem { id: usize, n: u32, pos: [f32; 3] },
    GemDrop(usize),
    GemLand([f32; 3]),
    Transform { id: usize, pos: [f32; 3] },
    PowerEnd(usize),
    Nova { pos: [f32; 3], r: f32 },
    Beam { from: [f32; 3], to: [f32; 3], id: usize },
    Shot([f32; 3]),
    Bump([f32; 3]),
    Fell(usize),
    Heal(usize),
    /// A timed pickup was taken (`kind` is the `ItemKind` as u8).
    Buff { id: usize, kind: u8 },
    /// `n` hits in a row (n >= 2) landed by `id` on `victim`.
    Combo { id: usize, n: u8, pos: [f32; 3] },
    /// A launched fighter slammed into a wall.
    Splat { id: usize, pos: [f32; 3] },
    /// A hazard shell was fired at a spot (the view warns with the usual red ring).
    Barrage([f32; 3]),
    Land(usize),
    Lava(usize),
    Cannon,
    Plank([f32; 3]),
    RoundStart(u32),
    RoundEnd { winner: i8 },
    MatchEnd { winner: i8 },
    Click(usize),
    /// A monster was hit / knocked out / came back.
    MonHit { pos: [f32; 3], big: bool },
    MonKo { pos: [f32; 3] },
    MonSpawn([f32; 3]),
    /// A course hazard starts its telegraph (`strike` false) or strikes (`strike` true) at `pos`.
    Hazard { look: Look, pos: [f32; 3], strike: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub seed: u64,
    pub mode: Mode,
    pub arena: ArenaKind,
    /// Human-controlled fighters (slots 0..humans).
    pub humans: usize,
    pub bots: usize,
    pub diff: Diff,
    /// Per-bot difficulty overrides (balance tests).
    pub diffs: [Option<Diff>; MAXF],
    pub same_kind: Option<Kind>,
    /// The player's character (slot 0 when it is a `Kind::Mario` fighter).
    pub hero: Hero,
    pub rounds_to_win: u8,
    pub stage: usize,
    /// Neutral OoT / BK monsters roaming the arena (the setup screen's default is Few; the core's is Off).
    pub monsters: Monsters,
}

impl Default for Config {
    fn default() -> Self {
        Self { seed: 1, mode: Mode::Versus, arena: ArenaKind::Bumpers, humans: 1, bots: 1, diff: Diff::Normal, diffs: [None; MAXF], same_kind: None, hero: Hero::Mario, rounds_to_win: 2, stage: 0, monsters: Monsters::Off }
    }
}

/// Gauntlet stages: (arena, [(kind, difficulty)], boss).
pub fn stage_def(stage: usize) -> (ArenaKind, &'static [(Kind, Diff)], bool) {
    match stage.min(GAUNTLET_STAGES - 1) {
        0 => (ArenaKind::Bumpers, &[(Kind::Goomba, Diff::Easy)], false),
        1 => (ArenaKind::Bridge, &[(Kind::Goomba, Diff::Normal), (Kind::Bomb, Diff::Easy)], false),
        2 => (ArenaKind::Gale, &[(Kind::Koopa, Diff::Normal), (Kind::Bully, Diff::Normal)], false),
        3 => (ArenaKind::Ember, &[(Kind::Chuckya, Diff::Hard), (Kind::Koopa, Diff::Normal)], false),
        4 => (ArenaKind::Gearworks, &[(Kind::Bully, Diff::Hard), (Kind::Chuckya, Diff::Hard), (Kind::Koopa, Diff::Hard)], false),
        _ => (ArenaKind::Cannon, &[(Kind::KingBomb, Diff::Hard), (Kind::Goomba, Diff::Easy), (Kind::Goomba, Diff::Easy)], true),
    }
}

pub struct Brawl {
    pub cfg: Config,
    pub arena: Arena,
    /// The arena every round starts from when it is not one of the built-in boxes (a real-world region).
    template: Option<Arena>,
    /// Gauntlet on real ground: stage `k` is played on `stage_arenas[k % len]` (empty = the box arena `stage_def` names).
    stage_arenas: Vec<Arena>,
    pub f: [Fighter; MAXF],
    pub brains: [Option<Brain>; MAXF],
    pub n: usize,
    pub items: [Item; MAXI],
    pub proj: [Proj; MAXP],
    pub gems: [Gem; GEMS],
    pub phase: Phase,
    pub phase_t: u32,
    pub tick: u32,
    pub round: u32,
    pub round_t: f32,
    pub rng: Rng,
    pub ev: Vec<Ev>,
    pub item_timer: i32,
    pub winner: i8,
    pub match_winner: i8,
    pub stage: usize,
    pub lives: u8,
    pub stage_clears: u32,
    pub overtime: bool,
    pub sudden: bool,
    stood: Vec<bool>,
    gem_batch: u32,
    pub kos_total: u32,
    pub total_ticks: u32,
    pub banner: Option<&'static str>,
    pub mons: [Mon; MAXM],
    /// Monsters each fighter knocked out (all rounds).
    pub mkos: [u16; MAXF],
    pub mon_kos: u32,
    mon_blast: bool,
    /// Audit / perf hook: skip the solid-body pass (`brawl_body.rs`), to measure what it changes and costs.
    pub bodies_off: bool,
}

fn pos_of(f: &Fighter) -> V2 {
    [f.pos[0], f.pos[2]]
}

impl Brawl {
    pub fn new(cfg: Config) -> Self {
        Self::with_arena(cfg, None)
    }

    /// A match on a ready-made arena (a real-world region, see `brawl_region.rs`); every round starts from a
    /// fresh copy of it. `None` builds the classic arena `cfg.arena` names.
    pub fn with_arena(cfg: Config, template: Option<Arena>) -> Self {
        Self::with_arenas(cfg, template, vec![])
    }

    /// A match whose Gauntlet stages each play on one of `stages` (real-world regions of one course, in order).
    pub fn with_arenas(cfg: Config, template: Option<Arena>, stage_arenas: Vec<Arena>) -> Self {
        let mut cfg = cfg;
        cfg.humans = cfg.humans.min(MAXF);
        if cfg.mode == Mode::Gauntlet {
            cfg.humans = cfg.humans.max(1).min(1);
        }
        let mut b = Self {
            cfg,
            arena: stage_arenas.first().cloned().or_else(|| template.clone()).unwrap_or_else(|| Arena::build(cfg.arena)),
            template,
            stage_arenas,
            f: [Fighter::blank(); MAXF],
            brains: [None, None, None, None],
            n: 0,
            items: [Item::NONE; MAXI],
            proj: [Proj::NONE; MAXP],
            gems: [Gem { state: GemState::Spent, pos: [0.0; 3], vel: [0.0; 3], spot: 0, immune: -1, immune_t: 0 }; GEMS],
            phase: Phase::Intro,
            phase_t: INTRO_TICKS,
            tick: 0,
            round: 0,
            round_t: ROUND_SECS,
            rng: Rng::new(cfg.seed),
            ev: Vec::with_capacity(128),
            item_timer: 120,
            winner: -1,
            match_winner: -1,
            stage: cfg.stage.min(GAUNTLET_STAGES - 1),
            lives: GAUNTLET_LIVES,
            stage_clears: 0,
            overtime: false,
            sudden: false,
            stood: vec![false; 32],
            gem_batch: 0,
            kos_total: 0,
            total_ticks: 0,
            banner: None,
            mons: [Mon::NONE; MAXM],
            mkos: [0; MAXF],
            mon_kos: 0,
            mon_blast: false,
            bodies_off: false,
        };
        b.setup_roster();
        b.begin_round();
        b
    }

    /// Fighters and their brains for the current mode / stage; wins persist across rounds.
    fn setup_roster(&mut self) {
        let cfg = self.cfg;
        let old_wins: [u8; MAXF] = std::array::from_fn(|i| self.f[i].wins);
        let mut f = [Fighter::blank(); MAXF];
        let mut brains: [Option<Brain>; MAXF] = [None, None, None, None];
        let pool = [Kind::Goomba, Kind::Bomb, Kind::Koopa, Kind::Bully, Kind::Chuckya];
        let mut rng = Rng::new(cfg.seed ^ 0xABCD);
        let off = rng.below(pool.len());
        let mut n = 0;
        match cfg.mode {
            Mode::Versus => {
                n = (cfg.humans + cfg.bots).clamp(2, MAXF);
                for (i, fi) in f.iter_mut().enumerate().take(n) {
                    let human = i < cfg.humans;
                    fi.kind = cfg.same_kind.unwrap_or(if i == 0 && human { Kind::Mario } else { pool[(off + i) % pool.len()] });
                    fi.human = human;
                    fi.hero = if i == 0 { cfg.hero } else { Hero::Mario };
                    fi.team = i as u8;
                    if !human {
                        let d = cfg.diffs[i].unwrap_or(cfg.diff);
                        brains[i] = Some(Brain::new(d, false, cfg.seed.wrapping_add(i as u64 * 7919)));
                    }
                }
            }
            Mode::Gauntlet => {
                let (_, bots, _) = stage_def(self.stage);
                f[0].kind = Kind::Mario;
                f[0].hero = cfg.hero;
                f[0].human = true;
                f[0].team = 0;
                n = 1;
                for (k, d) in bots {
                    f[n].kind = *k;
                    f[n].team = 1;
                    brains[n] = Some(Brain::new(*d, *k == Kind::KingBomb, cfg.seed.wrapping_add(n as u64 * 104729 + self.stage as u64)));
                    n += 1;
                }
            }
        }
        for i in 0..n {
            let st = f[i].stats();
            f[i].max_hp = st.hp;
            f[i].hp = st.hp;
            f[i].size = st.scale;
            f[i].wins = old_wins[i];
        }
        self.f = f;
        self.brains = brains;
        self.n = n;
    }

    /// Reset the arena, the fighters' health and positions, items and gems for a fresh round.
    fn begin_round(&mut self) {
        if self.cfg.mode == Mode::Gauntlet && !self.stage_arenas.is_empty() {
            self.arena = self.stage_arenas[self.stage % self.stage_arenas.len()].clone();
        } else if self.cfg.mode == Mode::Gauntlet {
            self.arena = Arena::build(stage_def(self.stage).0);
        } else if let Some(t) = &self.template {
            self.arena = t.clone();
        } else {
            self.arena = Arena::build(self.cfg.arena);
        }
        self.round += 1;
        self.phase = Phase::Intro;
        self.phase_t = INTRO_TICKS;
        self.round_t = ROUND_SECS;
        self.overtime = false;
        self.sudden = false;
        self.winner = -1;
        self.items = [Item::NONE; MAXI];
        self.proj = [Proj::NONE; MAXP];
        self.item_timer = 130;
        self.banner = None;
        // Spawns rotate a little so the same player is not always in the same corner.
        let rot = (self.round as usize + self.cfg.seed as usize) % 4;
        for i in 0..self.n {
            let s = self.arena.spawns[(i + if self.cfg.mode == Mode::Versus { rot } else { 0 }) % 4];
            let st = self.f[i].stats();
            let f = &mut self.f[i];
            let (wins, kind, hero, team, human) = (f.wins, f.kind, f.hero, f.team, f.human);
            let (kos, dmg, tr, gg) = (f.kos, f.dmg_dealt, f.transforms, f.gems_got);
            *f = Fighter::blank();
            f.wins = wins;
            f.kind = kind;
            f.hero = hero;
            f.team = team;
            f.human = human;
            f.kos = kos;
            f.dmg_dealt = dmg;
            f.transforms = tr;
            f.gems_got = gg;
            f.alive = true;
            f.max_hp = st.hp;
            f.hp = st.hp;
            f.size = st.scale;
            f.pos = [s[0], 0.0, s[1]];
            f.yaw = yaw_of([-s[0], -s[1]]);
            let top = self.arena.support(s[0], s[1], 400.0).map_or(0.0, |t| t.0);
            self.f[i].pos[1] = top;
            if let Some(b) = self.brains[i].as_mut() {
                b.reset();
            }
        }
        // Initial items: five scattered around, weapons spaced out; gems follow after a beat.
        for k in 0..5 {
            let spot = self.arena.item_spots[(k * 3 + self.rng.below(3)) % self.arena.item_spots.len()];
            let kind = self.roll_item(k < 2);
            self.spawn_item(kind, spot, false);
        }
        self.assign_gems(90);
        self.mons_begin_round();
        self.ev.push(Ev::RoundStart(self.round));
    }

    fn assign_gems(&mut self, delay: u32) {
        // Only spots that really are solid ground clear of the lava and the pillars ever hold a gem.
        let valid: Vec<usize> = (0..self.arena.gem_spots.len()).filter(|&k| self.arena.valid_spot(self.arena.gem_spots[k]) && self.arena.lava_floor() < self.arena.gem_spots[k][1] - 4.0).collect();
        let spots = valid.len().max(GEMS);
        let mut chosen = [usize::MAX; GEMS];
        for g in 0..GEMS {
            loop {
                let c = self.rng.below(spots);
                let c = valid.get(c).copied().unwrap_or(c % self.arena.gem_spots.len());
                if !chosen[..g].contains(&c) {
                    chosen[g] = c;
                    break;
                }
            }
        }
        self.gem_batch += 1;
        for g in 0..GEMS {
            let s = self.arena.gem_spots[chosen[g]];
            self.gems[g] = Gem { state: GemState::Pending(self.tick + delay + g as u32 * 14), pos: [s[0], s[1] + 560.0, s[2]], vel: [0.0; 3], spot: chosen[g], immune: -1, immune_t: 0 };
        }
    }

    fn roll_item(&mut self, no_weapon: bool) -> ItemKind {
        let weapons = self.items.iter().filter(|i| i.alive && i.kind.weapon()).count();
        let mut total = 0;
        for (k, w) in DROP_TABLE {
            if (no_weapon || weapons >= 2) && k.weapon() {
                continue;
            }
            total += w;
        }
        let mut r = self.rng.below(total as usize) as u32;
        for (k, w) in DROP_TABLE {
            if (no_weapon || weapons >= 2) && k.weapon() {
                continue;
            }
            if r < w {
                return k;
            }
            r -= w;
        }
        ItemKind::Crate
    }

    pub fn spawn_item(&mut self, kind: ItemKind, pos: [f32; 3], sky: bool) -> Option<usize> {
        let slot = self.items.iter().position(|i| !i.alive)?;
        self.items[slot] = Item { alive: true, kind, pos: [pos[0], pos[1] + if sky { 700.0 } else { 2.0 }, pos[2]], vel: [0.0; 3], state: IState::Rest, uses: kind.uses(), fuse: 0, owner: -1, grace: 0, spin: 0.0, grounded: false };
        Some(slot)
    }

    pub fn alive_count(&self) -> usize {
        (0..self.n).filter(|&i| self.f[i].alive).count()
    }

    pub fn nearest_foe(&self, i: usize) -> Option<(usize, f32)> {
        let me = &self.f[i];
        let mut best: Option<(usize, f32)> = None;
        for j in 0..self.n {
            let o = &self.f[j];
            if j == i || !o.alive || o.team == me.team {
                continue;
            }
            let d = dist2(pos_of(me), pos_of(o));
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        best
    }

    /// Advance one 30 Hz tick. `input[i]` drives human fighters; bots think for themselves.
    pub fn tick(&mut self, input: &[Input; MAXF]) {
        self.ev.clear();
        self.tick += 1;
        self.total_ticks += 1;
        match self.phase {
            Phase::Intro => {
                self.world_step(&[Input::default(); MAXF], false);
                if self.phase_t > 0 {
                    self.phase_t -= 1;
                }
                if self.phase_t == 0 {
                    self.phase = Phase::Fight;
                }
            }
            Phase::Fight => {
                let mut inputs = *input;
                for i in 0..self.n {
                    if self.f[i].alive && !self.f[i].human {
                        if let Some(mut b) = self.brains[i].take() {
                            inputs[i] = self.bot_input(i, &mut b);
                            self.brains[i] = Some(b);
                        }
                    }
                }
                self.world_step(&inputs, true);
                self.round_t -= DT;
                self.check_round_end();
            }
            Phase::RoundEnd => {
                self.world_step(&[Input::default(); MAXF], false);
                if self.phase_t > 0 {
                    self.phase_t -= 1;
                } else {
                    self.after_round();
                }
            }
            Phase::MatchEnd => {
                self.world_step(&[Input::default(); MAXF], false);
            }
        }
    }

    fn world_step(&mut self, inputs: &[Input; MAXF], live: bool) {
        // Planks crumble under whoever stands on them.
        for s in self.stood.iter_mut() {
            *s = false;
        }
        for i in 0..self.n {
            let f = &self.f[i];
            if f.alive && f.grounded && f.support >= 0 && (f.support as usize) < self.stood.len() {
                self.stood[f.support as usize] = true;
            }
        }
        let planks_before = self.arena.solids.iter().filter(|s| matches!(s.plank, Plank::Shaking(_))).count();
        self.arena.step(&self.stood);
        let planks_after = self.arena.solids.iter().filter(|s| matches!(s.plank, Plank::Shaking(_))).count();
        if planks_after > planks_before {
            if let Some(s) = self.arena.solids.iter().find(|s| matches!(s.plank, Plank::Shaking(14))) {
                self.ev.push(Ev::Plank([s.x, s.top, s.z]));
            }
        }
        for i in 0..self.n {
            self.step_fighter(i, inputs[i]);
        }
        if live {
            // Before the gems: a monster's blow can shake gems loose and they must be put right this tick.
            self.step_monsters();
        }
        self.step_items();
        self.step_projectiles();
        self.step_gems();
        if live {
            self.step_hazards();
            self.spawn_drops();
        }
        // Last: nobody stands inside anybody (fighters, monsters, set pieces), after this tick's blows have landed.
        if !self.bodies_off {
            self.step_bodies();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Fighters
    // -----------------------------------------------------------------------------------------

    fn target_size(&self, i: usize) -> f32 {
        let base = self.f[i].stats().scale;
        if self.f[i].power > 0.0 { base * POWER_SIZE } else { base }
    }

    fn step_fighter(&mut self, i: usize, mut inp: Input) {
        if !self.f[i].alive {
            let f = &mut self.f[i];
            f.dead_t = f.dead_t.saturating_add(1);
            // A knocked-out fighter tumbles away and sinks out of view.
            f.vel[1] -= GRAV * DT;
            for k in [0, 2] {
                f.vel[k] *= 0.97;
                f.pos[k] += f.vel[k] * DT;
            }
            f.pos[1] += f.vel[1] * DT;
            return;
        }
        // Impact freeze: the fighters of a solid blow hold still for a beat (timers, gravity and motion wait).
        if self.f[i].hitstop > 0 {
            self.f[i].hitstop -= 1;
            return;
        }
        let st = self.f[i].stats();
        let target = self.target_size(i);
        {
            let f = &mut self.f[i];
            f.combo_t = f.combo_t.saturating_sub(1);
            if f.combo_t == 0 {
                f.combo = 0;
            }
            f.splat = f.splat.saturating_sub(1);
            f.haste = f.haste.saturating_sub(1);
            f.shield = f.shield.saturating_sub(1);
            f.act_t = f.act_t.saturating_add(1);
            f.inv = f.inv.saturating_sub(1);
            f.cd_s1 = f.cd_s1.saturating_sub(1);
            f.cd_s2 = f.cd_s2.saturating_sub(1);
            f.cd_grab = f.cd_grab.saturating_sub(1);
            f.cd_shoot = f.cd_shoot.saturating_sub(1);
            f.burn = f.burn.saturating_sub(1);
            f.size = approach(f.size, target, 1.8 * DT);
            if f.power > 0.0 {
                f.power -= DT;
                if f.power <= 0.0 {
                    f.power = 0.0;
                    self.ev.push(Ev::PowerEnd(i));
                    self.power_ended();
                }
            }
        }
        let powered = self.f[i].powered();
        let speed_mult = st.speed * if powered { 1.3 } else { 1.0 } * if self.f[i].haste > 0 { 1.32 } else { 1.0 } * if self.f[i].held >= 0 { 0.88 } else { 1.0 } * if self.f[i].sink > 30 { 0.75 } else { 1.0 };
        // Frozen during the countdown / after the bell.
        let can_act = !matches!(self.phase, Phase::Intro | Phase::RoundEnd | Phase::MatchEnd);
        if !can_act {
            inp = Input::default();
        }
        // Boomerang bookkeeping: lose the flag when the projectile is gone.
        if self.f[i].boom_out && !self.proj.iter().any(|p| p.alive && p.kind == PKind::Boomerang && p.owner == i as i8) {
            self.f[i].boom_out = false;
        }
        let act = self.f[i].act;
        let free = matches!(act, Act::Idle | Act::Air);
        let mut wish = [0.0f32, 0.0f32];
        let mut accel_ground = ACCEL_GROUND;
        let mut gravity = true;
        let mut face_to: Option<f32> = None;

        // ---- actions ----
        if free {
            // Specials first (power form only).
            if powered && inp.s1 && self.f[i].cd_s1 == 0 && self.f[i].power > NOVA_COST * 0.6 {
                self.start_act(i, Act::Nova);
                self.f[i].power = (self.f[i].power - NOVA_COST).max(0.2);
                self.f[i].cd_s1 = 36;
                self.f[i].inv = self.f[i].inv.max(22);
            } else if powered && inp.s2 && self.f[i].cd_s2 == 0 && self.f[i].power > BEAM_COST * 0.6 {
                self.start_act(i, Act::Beam);
                self.f[i].power = (self.f[i].power - BEAM_COST).max(0.2);
                self.f[i].cd_s2 = 26;
            } else if inp.attack {
                self.begin_attack(i);
            } else if inp.grab || (inp.z && self.f[i].grounded) {
                self.grab_or_throw(i);
            } else if inp.z && !self.f[i].grounded && self.f[i].act == Act::Air {
                self.start_act(i, Act::PoundWind);
                self.ev.push(Ev::PoundStart(i));
            }
        } else if matches!(act, Act::Punch(_) | Act::Swing | Act::Shoot | Act::Toss) && inp.attack {
            self.f[i].buffer = true;
        }
        let act = self.f[i].act;
        let free = matches!(act, Act::Idle | Act::Air);

        // ---- per-action motion ----
        match act {
            Act::Idle | Act::Air => {
                wish = inp.mv;
                let l = len2(wish);
                if l > 1.0 {
                    wish = [wish[0] / l, wish[1] / l];
                }
            }
            Act::Punch(step) => {
                let hero = self.f[i].hero.style();
                let base = if self.f[i].kind == Kind::Mario { st.scale } else { 1.0 };
                let b = hero.blow(step);
                let (total, hit_at) = (b.total, b.hit_at);
                if self.f[i].act_t == hit_at {
                    // Power form and the hero's size stretch the reach exactly as before.
                    let reach = b.reach + 12.0 * (self.f[i].size - base) * 6.0;
                    self.ev.push(Ev::Punch { id: i, step });
                    if self.melee(i, reach, 0.3, b.dmg, b.kb, b.up) {
                        // landing a punch keeps the chain alive a touch longer
                    }
                }
                self.f[i].vel[0] *= 0.82;
                self.f[i].vel[2] *= 0.82;
                if self.f[i].act_t >= total {
                    if inp.attack_held && !self.f[i].buffer && step == 0 && hero.spin.is_some() {
                        self.start_act(i, Act::Spin);
                        self.f[i].inv = self.f[i].inv.max(4);
                        self.ev.push(Ev::Swing(i));
                    } else if self.f[i].buffer && step < 2 {
                        self.f[i].buffer = false;
                        self.start_act(i, Act::Punch(step + 1));
                        self.lunge(i, hero.lunge[(step as usize + 1).min(2)]);
                    } else {
                        self.end_act(i);
                    }
                }
            }
            Act::Spin => {
                let hero = self.f[i].hero.style();
                if let Some(b) = hero.spin {
                    if self.f[i].act_t == b.hit_at {
                        let reach = b.reach + 12.0 * (self.f[i].size - st.scale) * 6.0;
                        self.ev.push(Ev::Punch { id: i, step: 3 });
                        self.melee(i, reach, -1.0, b.dmg, b.kb, b.up);
                    }
                    self.f[i].vel[0] *= 0.9;
                    self.f[i].vel[2] *= 0.9;
                    if self.f[i].act_t >= b.total {
                        self.end_act(i);
                    }
                } else {
                    self.end_act(i);
                }
            }
            Act::Swing => {
                if self.f[i].act_t == 8 {
                    self.ev.push(Ev::Swing(i));
                    let reach = 215.0 + (self.f[i].size - 1.0) * 60.0;
                    self.melee(i, reach, 0.0, 22.0, 800.0, 380.0);
                    self.use_held(i);
                }
                self.f[i].vel[0] *= 0.8;
                self.f[i].vel[2] *= 0.8;
                if self.f[i].act_t >= 17 {
                    self.end_act(i);
                }
            }
            Act::Shoot => {
                self.f[i].vel[0] *= 0.8;
                self.f[i].vel[2] *= 0.8;
                if self.f[i].act_t >= 6 {
                    self.end_act(i);
                }
            }
            Act::Toss => {
                self.f[i].vel[0] *= 0.8;
                self.f[i].vel[2] *= 0.8;
                if self.f[i].act_t >= 9 {
                    self.end_act(i);
                }
            }
            Act::Dive => {
                // Sliding belly-first through the air, hitting whatever it meets.
                let d = self.f[i].hero.style().dive;
                self.melee(i, d.reach, 0.2, d.dmg, d.kb, d.up);
                if self.f[i].grounded && self.f[i].act_t > 2 {
                    self.start_act(i, Act::DiveSlide);
                }
            }
            Act::DiveSlide => {
                let f = &mut self.f[i];
                f.vel[0] *= 0.9;
                f.vel[2] *= 0.9;
                if f.act_t >= 12 {
                    self.end_act(i);
                }
            }
            Act::PoundWind => {
                gravity = false;
                let f = &mut self.f[i];
                f.vel[0] *= 0.7;
                f.vel[2] *= 0.7;
                f.vel[1] = 0.0;
                if f.act_t >= 9 {
                    f.act = Act::Pound;
                    f.act_t = 0;
                    f.hit_mask = 0;
                    f.vel[1] = -2500.0;
                }
            }
            Act::Pound => {
                let f = &mut self.f[i];
                f.vel[0] = 0.0;
                f.vel[2] = 0.0;
                f.vel[1] = f.vel[1].min(-2500.0);
                // Stomping on someone's head on the way down.
                self.stomp(i);
                if self.f[i].grounded {
                    let pos = self.f[i].pos;
                    let pd = self.f[i].hero.style().pound;
                    let r = pd.radius * (self.f[i].size / st.scale).max(1.0);
                    self.ev.push(Ev::PoundLand { pos, r });
                    self.shockwave(i, r, pd.dmg, pd.kb, pd.up);
                    self.start_act(i, Act::PoundLand);
                }
            }
            Act::PoundLand => {
                let f = &mut self.f[i];
                f.vel[0] = 0.0;
                f.vel[2] = 0.0;
                if f.act_t >= 12 {
                    self.end_act(i);
                }
            }
            Act::Nova => {
                let f = &mut self.f[i];
                f.vel[0] *= 0.7;
                f.vel[2] *= 0.7;
                if f.act_t == 11 {
                    let pos = f.pos;
                    let r = 430.0 * f.size / POWER_SIZE.min(f.size).max(1.0) * 1.0;
                    let r = r.max(430.0);
                    self.ev.push(Ev::Nova { pos, r });
                    self.shockwave(i, r, 24.0, 920.0, 640.0);
                    self.fling_items(pos, r, i);
                }
                if self.f[i].act_t >= 20 {
                    self.end_act(i);
                }
            }
            Act::Beam => {
                let f = &mut self.f[i];
                f.vel[0] *= 0.6;
                f.vel[2] *= 0.6;
                if f.act_t == 6 {
                    self.fire_beam(i);
                }
                if self.f[i].act_t >= 14 {
                    self.end_act(i);
                }
            }
            Act::Hurt => {
                accel_ground = 0.0;
                let f = &mut self.f[i];
                let fr = if f.grounded { 1900.0 * DT } else { 70.0 * DT };
                for k in [0, 2] {
                    f.vel[k] = approach(f.vel[k], 0.0, fr);
                }
                if f.act_t >= f.stun {
                    self.end_act(i);
                }
            }
            Act::Down => {
                accel_ground = 0.0;
                let f = &mut self.f[i];
                let fr = if f.grounded { 1600.0 * DT } else { 60.0 * DT };
                for k in [0, 2] {
                    f.vel[k] = approach(f.vel[k], 0.0, fr);
                }
                if f.grounded && f.act_t >= 22 {
                    f.act = Act::GetUp;
                    f.act_t = 0;
                    f.inv = f.inv.max(16);
                }
            }
            Act::GetUp => {
                let f = &mut self.f[i];
                f.vel[0] = 0.0;
                f.vel[2] = 0.0;
                if f.act_t >= 8 {
                    self.end_act(i);
                }
            }
            Act::Dead => {}
        }

        // ---- locomotion ----
        let controllable = free;
        let top_speed = RUN * speed_mult;
        {
            let f = &mut self.f[i];
            if controllable {
                let l = len2(wish);
                if l > 0.05 {
                    let want = [wish[0] * top_speed, wish[1] * top_speed];
                    let against = f.vel[0] * want[0] + f.vel[2] * want[1] < 0.0;
                    let a = if f.grounded { accel_ground } else { ACCEL_AIR } * DT * if against { TURN_BOOST } else { 1.0 };
                    f.vel[0] = approach(f.vel[0], want[0], a);
                    f.vel[2] = approach(f.vel[2], want[1], a);
                    face_to = Some(yaw_of(wish));
                } else if f.grounded {
                    f.vel[0] = approach(f.vel[0], 0.0, BRAKE * DT);
                    f.vel[2] = approach(f.vel[2], 0.0, BRAKE * DT);
                }
            }
            if let Some(y) = face_to {
                let d = angle_diff(f.yaw, y);
                f.yaw += d.clamp(-YAW_STEP, YAW_STEP);
            }
        }
        // Jumping (A): the first jump from the ground (with a little coyote time), one more in the air.
        if (controllable || matches!(act, Act::Dive)) && inp.jump && can_act && controllable {
            let f = &mut self.f[i];
            if f.grounded || f.coyote > 0 {
                f.vel[1] = JUMP_V * if powered { 1.1 } else { 1.0 };
                f.jumps = 1;
                f.grounded = false;
                f.coyote = 0;
                self.ev.push(Ev::Jump(i));
            } else if f.jumps < 2 {
                f.vel[1] = JUMP_V * 0.88;
                f.jumps = 2;
                self.ev.push(Ev::Jump(i));
            }
        }
        {
            let f = &mut self.f[i];
            if controllable && !inp.jump_held && f.vel[1] > 480.0 && !f.grounded && f.jumps == 1 {
                f.vel[1] *= 0.55;
            }
            if gravity {
                f.vel[1] = (f.vel[1] - GRAV * DT).max(-2400.0);
            }
        }
        // Wind pushes everything not rooted to the spot.
        if self.arena.wind > 0.0 {
            let w = self.arena.wind;
            let a = self.arena.wind_dir;
            let f = &mut self.f[i];
            let k = (if f.grounded { 700.0 } else { 1100.0 }) * w * DT / st.weight.sqrt();
            f.vel[0] += a[0] * k;
            f.vel[2] += a[1] * k;
        }
        self.integrate(i);
    }

    /// Move, collide with the arena and land / fall.
    fn integrate(&mut self, i: usize) {
        let r = self.f[i].radius();
        let was_grounded = self.f[i].grounded;
        let (dx, dy, dz) = (self.f[i].vel[0] * DT, self.f[i].vel[1] * DT, self.f[i].vel[2] * DT);
        let mut p = self.f[i].pos;
        // Ride the platform we stood on.
        if was_grounded && self.f[i].support >= 0 {
            if let Some(s) = self.arena.solids.get(self.f[i].support as usize) {
                p[0] += s.d[0];
                p[2] += s.d[2];
            }
        }
        p[0] += dx;
        p[2] += dz;
        p[1] += dy;
        let before = [p[0], p[2]];
        self.arena.push_out(&mut p, r);
        // A launched fighter that is stopped dead by a wall splats: extra damage, a longer stun and a rebound.
        if self.f[i].splat > 0 && matches!(self.f[i].act, Act::Hurt | Act::Down) {
            let moved = len2([p[0] - before[0], p[2] - before[1]]);
            let speed = len2([self.f[i].vel[0], self.f[i].vel[2]]);
            let inside_rim = self.arena.bounds.is_none_or(|b| p[0].abs() < b[0] - r - 1.0 && p[2].abs() < b[1] - r - 1.0);
            if moved > 1.5 && speed > 380.0 && inside_rim {
                let dmg = 5.0 + speed / 160.0;
                let f = &mut self.f[i];
                f.splat = 0;
                f.vel[0] *= -0.3;
                f.vel[2] *= -0.3;
                f.stun = f.stun.max(14);
                f.hp -= dmg;
                f.hitstop = 5;
                let at = [p[0], p[1] + 70.0 * f.size, p[2]];
                self.ev.push(Ev::Splat { id: i, pos: at });
                if self.f[i].hp <= 0.0 {
                    self.f[i].pos = p;
                    let by = self.f[i].last_hit_by;
                    self.ko(i, by);
                    return;
                }
            }
        }
        // Vertical: land on the highest top within a step of the feet (the same rule as the walls).
        let prev_y = self.f[i].pos[1];
        let vy = self.f[i].vel[1];
        let sup = self.arena.support(p[0], p[2], prev_y + STEP);
        let f = &mut self.f[i];
        f.support = -1;
        let mut grounded = false;
        if let Some((top, idx)) = sup {
            if vy <= 0.0 && p[1] <= top + if was_grounded { STEP } else { 0.5 } {
                if !was_grounded && vy < -900.0 {
                    self.ev.push(Ev::Land(i));
                }
                p[1] = top;
                f.vel[1] = 0.0;
                f.support = idx as i16;
                grounded = true;
            }
        }
        f.pos = p;
        f.grounded = grounded;
        if grounded {
            f.jumps = 0;
            f.coyote = 5;
            if f.act == Act::Air {
                f.act = Act::Idle;
            }
        } else {
            f.coyote = f.coyote.saturating_sub(1);
            if f.act == Act::Idle {
                f.act = Act::Air;
                f.act_t = 0;
            }
        }
        // Lava.
        if self.arena.lava > f.pos[1] + 4.0 && f.alive && self.arena.kind == ArenaKind::Ember && f.pos[1] < self.arena.lava {
            f.burn = 8;
            f.vel[1] = f.vel[1].max(820.0);
            f.grounded = false;
            f.hp -= 6.0;
            if f.hp <= 0.0 {
                let id = i;
                self.ev.push(Ev::Lava(id));
                self.ko(id, -1);
                return;
            }
            self.ev.push(Ev::Lava(i));
        }
        // The course's own floor: quicksand swallows and spits out, lava burns.
        if self.f[i].grounded && self.arena.ground.is_some() {
            self.floor_effects(i);
        } else if self.f[i].sink > 0 && self.f[i].grounded {
            self.f[i].sink = 0;
        }
        // The void.
        if self.f[i].alive && self.f[i].pos[1] < self.arena.void_y {
            self.fall_out(i);
        }
    }

    /// Quicksand (from the course's surfaces) slows a fighter and, after two seconds, spits them out hurt; lava burns and bounces.
    fn floor_effects(&mut self, i: usize) {
        let Some(g) = self.arena.ground.clone() else { return };
        let p = self.f[i].pos;
        match g.floor_effect(p[0], p[2], p[1]) {
            FloorFx::Sink => {
                // Keep moving or go under: standing (or fighting) in place sinks, running drains it.
                let f = &mut self.f[i];
                let moving = f.speed2() > 220.0 && matches!(f.act, Act::Idle | Act::Air);
                f.sink = if moving { f.sink.saturating_sub(1) } else { f.sink.saturating_add(2) };
                if f.sink >= SINK_TICKS {
                    f.sink = 0;
                    f.hp -= 8.0;
                    f.vel = [0.0, 950.0, 0.0];
                    f.grounded = false;
                    f.inv = f.inv.max(15);
                    f.act = Act::Hurt;
                    f.act_t = 0;
                    f.stun = 10;
                    let size = f.size;
                    self.ev.push(Ev::Hit { pos: [p[0], p[1] + 70.0 * size, p[2]], big: true, victim: i });
                    if self.f[i].hp <= 0.0 {
                        let by = if self.tick.saturating_sub(self.f[i].hit_tick) < CREDIT_TICKS { self.f[i].last_hit_by } else { -1 };
                        self.ko(i, by);
                    }
                }
            }
            FloorFx::Burn => {
                let f = &mut self.f[i];
                f.sink = 0;
                f.burn = 8;
                f.vel[1] = f.vel[1].max(820.0);
                f.grounded = false;
                f.hp -= 6.0;
                self.ev.push(Ev::Lava(i));
                if self.f[i].hp <= 0.0 {
                    let by = if self.tick.saturating_sub(self.f[i].hit_tick) < CREDIT_TICKS { self.f[i].last_hit_by } else { -1 };
                    self.ko(i, by);
                }
            }
            FloorFx::None => {
                let f = &mut self.f[i];
                f.sink = f.sink.saturating_sub(3);
            }
        }
    }

    fn fall_out(&mut self, i: usize) {
        self.ev.push(Ev::Fell(i));
        self.drop_gems(i, true);
        if self.f[i].held >= 0 {
            self.drop_held(i);
        }
        let dmg = 18.0;
        self.f[i].hp -= dmg;
        if self.f[i].hp <= 0.0 {
            let by = self.f[i].last_hit_by;
            self.ko(i, by);
            return;
        }
        // Back on the nearest spawn with a breather.
        let mut best = self.arena.spawns[0];
        let mut bd = f32::MAX;
        for s in self.arena.spawns {
            let d = dist2(s, pos_of(&self.f[i]));
            // Never back into the tide.
            let dry = self.arena.support(s[0], s[1], 600.0).is_none_or(|t| t.0 >= self.arena.lava_floor() + 4.0);
            if d < bd && dry {
                bd = d;
                best = s;
            }
        }
        let top = self.arena.support(best[0], best[1], 600.0).map_or(0.0, |t| t.0);
        let f = &mut self.f[i];
        f.pos = [best[0], top + 30.0, best[1]];
        f.vel = [0.0; 3];
        f.act = Act::Air;
        f.act_t = 0;
        f.inv = 60;
        f.stun = 0;
    }

    pub fn start_act(&mut self, i: usize, a: Act) {
        let f = &mut self.f[i];
        f.act = a;
        f.act_t = 0;
        f.hit_mask = 0;
        f.buffer = false;
    }

    fn end_act(&mut self, i: usize) {
        let f = &mut self.f[i];
        f.act = if f.grounded { Act::Idle } else { Act::Air };
        f.act_t = 0;
        f.buffer = false;
    }

    fn lunge(&mut self, i: usize, speed: f32) {
        let f = &mut self.f[i];
        let d = dir_of(f.yaw);
        f.vel[0] = d[0] * speed;
        f.vel[2] = d[1] * speed;
    }

    /// Snap the facing toward a foe in front within reach (aim assist for everyone).
    fn assist_aim(&mut self, i: usize, range: f32) {
        let me = self.f[i];
        let fwd = dir_of(me.yaw);
        let mut best: Option<(f32, f32)> = None;
        for j in 0..self.n {
            let o = &self.f[j];
            if j == i || !o.alive || o.team == me.team {
                continue;
            }
            let d = [o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]];
            let dist = len2(d);
            if dist > range {
                continue;
            }
            let ang = angle_diff(me.yaw, yaw_of(d)).abs();
            if ang < 1.1 && best.is_none_or(|(s, _)| dist + ang * 80.0 < s) {
                best = Some((dist + ang * 80.0, yaw_of(d)));
            }
        }
        let _ = fwd;
        if let Some((_, y)) = best {
            self.f[i].yaw = y;
        }
    }

    fn begin_attack(&mut self, i: usize) {
        let held = self.f[i].held;
        if held >= 0 {
            let kind = self.items[held as usize].kind;
            match kind {
                ItemKind::Hammer => {
                    self.assist_aim(i, 330.0);
                    self.start_act(i, Act::Swing);
                    self.lunge(i, 120.0);
                    return;
                }
                ItemKind::Blaster => {
                    if self.f[i].cd_shoot == 0 {
                        self.assist_aim(i, 1300.0);
                        self.start_act(i, Act::Shoot);
                        self.f[i].cd_shoot = 7;
                        self.fire_bullet(i);
                    } else {
                        self.ev.push(Ev::Click(i));
                    }
                    return;
                }
                ItemKind::Boomerang => {
                    if !self.f[i].boom_out {
                        self.assist_aim(i, 1000.0);
                        self.start_act(i, Act::Toss);
                        self.throw_boomerang(i);
                    }
                    return;
                }
                _ => {
                    self.assist_aim(i, 900.0);
                    self.throw_held(i);
                    return;
                }
            }
        }
        let f = self.f[i];
        if !f.grounded {
            // B in the air: dive.
            self.assist_aim(i, 500.0);
            self.start_act(i, Act::Dive);
            let d = dir_of(self.f[i].yaw);
            let sp = self.f[i].hero.style().dive.speed * if self.f[i].powered() { 1.2 } else { 1.0 };
            self.f[i].vel = [d[0] * sp, 160.0, d[1] * sp];
            self.ev.push(Ev::Dive(i));
        } else {
            self.assist_aim(i, 260.0);
            self.start_act(i, Act::Punch(0));
            let l = self.f[i].hero.style().lunge[0];
            self.lunge(i, l);
        }
    }

    fn grab_or_throw(&mut self, i: usize) {
        if self.f[i].held >= 0 {
            let held = self.f[i].held as usize;
            if self.items[held].kind.weapon() {
                self.drop_held(i);
            } else {
                self.assist_aim(i, 900.0);
                self.throw_held(i);
            }
            return;
        }
        if self.f[i].cd_grab > 0 {
            return;
        }
        let me = self.f[i];
        let mut best: Option<(usize, f32)> = None;
        for (k, it) in self.items.iter().enumerate() {
            if !it.alive || it.state != IState::Rest || it.kind.pickup() {
                continue;
            }
            let d = dist2([it.pos[0], it.pos[2]], pos_of(&me));
            if d < 135.0 + me.radius() && (it.pos[1] - me.pos[1]).abs() < 160.0 && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((k, d));
            }
        }
        if let Some((k, _)) = best {
            self.items[k].state = IState::Held(i as u8);
            self.items[k].owner = i as i8;
            if self.items[k].kind == ItemKind::Bomb && self.items[k].fuse == 0 {
                self.items[k].fuse = 78;
            }
            self.f[i].held = k as i8;
            self.f[i].cd_grab = 8;
            self.ev.push(Ev::Grab(i));
        }
    }

    fn drop_held(&mut self, i: usize) {
        let h = self.f[i].held;
        if h < 0 {
            return;
        }
        let d = dir_of(self.f[i].yaw);
        let it = &mut self.items[h as usize];
        it.state = IState::Rest;
        it.vel = [d[0] * 160.0, 200.0, d[1] * 160.0];
        it.owner = -1;
        self.f[i].held = -1;
        self.f[i].cd_grab = 12;
    }

    fn throw_held(&mut self, i: usize) {
        let h = self.f[i].held;
        if h < 0 {
            return;
        }
        let d = dir_of(self.f[i].yaw);
        let power = if self.f[i].powered() { 1.25 } else { 1.0 };
        let it = &mut self.items[h as usize];
        let (_, _, _, sp) = it.kind.throw_stats();
        it.state = IState::Flying;
        it.owner = i as i8;
        it.grace = 8;
        it.vel = [d[0] * sp * power, if it.kind == ItemKind::Bomb { 520.0 } else { 330.0 }, d[1] * sp * power];
        it.pos[0] += d[0] * 20.0;
        it.pos[2] += d[1] * 20.0;
        self.f[i].held = -1;
        self.f[i].cd_grab = 10;
        self.start_act(i, Act::Toss);
        self.ev.push(Ev::Throw(i));
    }

    fn use_held(&mut self, i: usize) {
        let h = self.f[i].held;
        if h < 0 {
            return;
        }
        let it = &mut self.items[h as usize];
        it.uses = it.uses.saturating_sub(1);
        if it.uses == 0 {
            it.alive = false;
            self.f[i].held = -1;
            self.ev.push(Ev::Break(self.f[i].pos));
        }
    }

    fn fire_bullet(&mut self, i: usize) {
        let f = self.f[i];
        let d = dir_of(f.yaw);
        let pos = [f.pos[0] + d[0] * 60.0, f.pos[1] + 80.0 * f.size, f.pos[2] + d[1] * 60.0];
        self.spawn_proj(PKind::Bullet, pos, [d[0] * 1700.0, 0.0, d[1] * 1700.0], i as i8, [0.0; 3]);
        self.ev.push(Ev::Shot(pos));
        self.use_held(i);
    }

    fn throw_boomerang(&mut self, i: usize) {
        let f = self.f[i];
        let d = dir_of(f.yaw);
        let pos = [f.pos[0] + d[0] * 50.0, f.pos[1] + 80.0 * f.size, f.pos[2] + d[1] * 50.0];
        if self.spawn_proj(PKind::Boomerang, pos, [d[0] * 1250.0, 0.0, d[1] * 1250.0], i as i8, [0.0; 3]) {
            self.f[i].boom_out = true;
            self.ev.push(Ev::Throw(i));
        }
    }

    fn spawn_proj(&mut self, kind: PKind, pos: [f32; 3], vel: [f32; 3], owner: i8, aim: [f32; 3]) -> bool {
        if let Some(p) = self.proj.iter_mut().find(|p| !p.alive) {
            *p = Proj { alive: true, kind, pos, vel, owner, t: 0, mask: 0, aim };
            true
        } else {
            false
        }
    }

    fn fire_beam(&mut self, i: usize) {
        let me = self.f[i];
        let from = [me.pos[0], me.pos[1] + 90.0 * me.size, me.pos[2]];
        let mut best: Option<(usize, f32)> = None;
        for j in 0..self.n {
            let o = &self.f[j];
            if j == i || !o.alive || o.team == me.team {
                continue;
            }
            let d = dist2(pos_of(&me), pos_of(o));
            if d < 1500.0 && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        match best {
            Some((j, _)) => {
                let o = self.f[j];
                let to = [o.pos[0], o.pos[1] + 80.0 * o.size, o.pos[2]];
                let dir = norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw));
                self.ev.push(Ev::Beam { from, to, id: i });
                self.hit(j, i as i8, 22.0, dir, 700.0, 330.0);
            }
            None => {
                if let Some(m) = self.beam_mon(i, 900.0) {
                    self.hit_mon_by_beam(i, m);
                    return;
                }
                let d = dir_of(me.yaw);
                let to = [from[0] + d[0] * 700.0, from[1], from[2] + d[1] * 700.0];
                self.ev.push(Ev::Beam { from, to, id: i });
            }
        }
    }

    fn melee(&mut self, i: usize, range: f32, arc_cos: f32, dmg: f32, kb: f32, up: f32) -> bool {
        let me = self.f[i];
        let fwd = dir_of(me.yaw);
        let mut any = false;
        let range = range * (1.0 + (me.size - me.stats().scale).max(0.0) * 0.5);
        for j in 0..self.n {
            if j == i || !self.f[j].alive || self.f[i].hit_mask & (1 << j) != 0 || self.f[j].team == me.team {
                continue;
            }
            let o = self.f[j];
            let d = [o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]];
            let dist = len2(d);
            if dist > range + o.radius() + me.radius() * 0.6 {
                continue;
            }
            let dn = norm2(d, fwd);
            if dn[0] * fwd[0] + dn[1] * fwd[1] < arc_cos && dist > o.radius() * 1.1 {
                continue;
            }
            if (o.pos[1] - me.pos[1]).abs() > 130.0 * me.size.max(o.size) {
                continue;
            }
            self.f[i].hit_mask |= 1 << j;
            let dir = norm2([fwd[0] * 0.55 + dn[0] * 0.45, fwd[1] * 0.55 + dn[1] * 0.45], fwd);
            if self.hit(j, i as i8, dmg, dir, kb, up) {
                any = true;
            }
        }
        any | self.melee_mons(i, range, arc_cos, dmg, kb, up)
    }

    fn stomp(&mut self, i: usize) {
        let me = self.f[i];
        for j in 0..self.n {
            if j == i || !self.f[j].alive || me.hit_mask & (1 << j) != 0 || self.f[j].team == me.team {
                continue;
            }
            let o = self.f[j];
            let d = dist2(pos_of(&me), pos_of(&o));
            let above = me.pos[1] - o.pos[1];
            if d < me.radius() + o.radius() && above > 20.0 && above < o.height() + 60.0 {
                self.f[i].hit_mask |= 1 << j;
                self.hit(j, i as i8, 10.0, norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw)), 380.0, 0.0);
            }
        }
        self.stomp_mons(i);
    }

    /// A ring blast around fighter `i` that hurts everyone on the ground (or low) within `r`.
    fn shockwave(&mut self, i: usize, r: f32, dmg: f32, kb: f32, up: f32) {
        let me = self.f[i];
        for j in 0..self.n {
            if j == i || !self.f[j].alive || self.f[j].team == me.team {
                continue;
            }
            let o = self.f[j];
            let d = dist2(pos_of(&me), pos_of(&o));
            if d > r + o.radius() || (o.pos[1] - me.pos[1]) > 170.0 {
                continue;
            }
            let fall = 1.0 - 0.4 * (d / r).min(1.0);
            let dir = norm2([o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]], dir_of(me.yaw));
            self.hit(j, i as i8, dmg * fall, dir, kb * fall, up);
        }
        self.shock_mons(i, r, dmg, kb, up);
    }

    fn fling_items(&mut self, pos: [f32; 3], r: f32, owner: usize) {
        let _ = owner;
        for it in self.items.iter_mut().filter(|i| i.alive && i.state == IState::Rest) {
            let d = dist2([it.pos[0], it.pos[2]], [pos[0], pos[2]]);
            if d < r {
                let dir = norm2([it.pos[0] - pos[0], it.pos[2] - pos[2]], [1.0, 0.0]);
                it.vel = [dir[0] * 900.0, 700.0, dir[1] * 900.0];
                it.grounded = false;
            }
        }
    }

    /// Apply damage and knockback. Returns false when the blow was ignored (invulnerable, ally).
    pub fn hit(&mut self, v: usize, a: i8, dmg: f32, dir: V2, kb: f32, up: f32) -> bool {
        let victim = self.f[v];
        if !victim.alive || victim.inv > 0 {
            return false;
        }
        let (mult, atk_team) = if a >= 0 {
            let af = &self.f[a as usize];
            (af.stats().power * if af.powered() { POWER_HIT } else { 1.0 }, af.team)
        } else {
            (1.0, u8::MAX)
        };
        if a >= 0 && a as usize != v && atk_team == victim.team {
            return false;
        }
        let taken = (if victim.powered() { 0.7 } else { 1.0 }) * if victim.shield > 0 { 0.4 } else { 1.0 };
        // Combos: each blow that lands inside the window of the last one hits a little harder (up to +32 %).
        let combo = if a >= 0 && a as usize != v {
            let af = &mut self.f[a as usize];
            af.combo = if af.combo_t > 0 { af.combo.saturating_add(1) } else { 1 };
            af.combo_t = COMBO_WINDOW;
            af.combo
        } else {
            0
        };
        let dmg = dmg * mult * taken * (1.0 + 0.08 * combo.saturating_sub(1).min(4) as f32);
        let st = victim.stats();
        let weight = st.weight * if victim.powered() { 1.35 } else { 1.0 };
        let ratio = 1.0 + 0.55 * (1.0 - victim.hp_frac());
        // The fourth blow of a combo is a finisher: it launches.
        let kbv = kb * mult.sqrt() * ratio / weight * if combo >= 4 { 1.3 } else { 1.0 };
        let upv = if kb > 0.0 { (up * ratio / weight.sqrt()).max(if kbv > 300.0 { 180.0 } else { 0.0 }) } else { 0.0 };
        let heavy = kbv >= 560.0;
        let pos = victim.pos;
        // A blow with no attacker (a hazard, a monster, a shell) keeps the credit of whoever hit this fighter in the last few seconds.
        let credit = if a >= 0 { a } else if self.tick.saturating_sub(victim.hit_tick) < CREDIT_TICKS { victim.last_hit_by } else { -1 };
        {
            let f = &mut self.f[v];
            f.hp -= dmg;
            f.last_hit_by = credit;
            if a >= 0 && a as usize != v {
                f.hit_tick = self.tick;
            }
            f.vel = [dir[0] * kbv, if upv > 0.0 { upv } else { f.vel[1].min(0.0) }, dir[1] * kbv];
            if upv > 0.0 {
                f.grounded = false;
            }
            f.hit_mask = 0;
            f.buffer = false;
            f.stun = ((9.0 / weight.sqrt()) as u16).max(4);
            // Launched hard enough to splat against a wall for the next two thirds of a second.
            f.splat = if kbv >= 420.0 { 20 } else { 0 };
        }
        // Impact freeze on both sides, longer for a harder blow.
        let stop = (1.0 + dmg / 9.0 + if heavy { 2.0 } else { 0.0 }).min(6.0) as u8;
        if a >= 0 || heavy {
            self.f[v].hitstop = stop;
        }
        if a >= 0 && a as usize != v {
            self.f[a as usize].hitstop = self.f[a as usize].hitstop.max(stop.saturating_sub(1));
            if combo >= 2 {
                self.ev.push(Ev::Combo { id: a as usize, n: combo, pos: [pos[0], pos[1] + 70.0 * victim.size, pos[2]] });
            }
        }
        if a >= 0 && a as usize != v {
            self.f[a as usize].dmg_dealt += dmg;
        }
        self.ev.push(Ev::Hit { pos: [pos[0], pos[1] + 70.0 * victim.size, pos[2]], big: heavy || dmg >= 14.0, victim: v });
        if dmg >= 8.0 || heavy {
            self.drop_gems(v, heavy);
        }
        if self.f[v].held >= 0 && (heavy || dmg >= 14.0) {
            self.drop_held(v);
        }
        if self.f[v].hp <= 0.0 {
            self.ko(v, credit);
            return true;
        }
        let boss_tough = weight >= 2.0 && kbv < 900.0;
        let f = &mut self.f[v];
        if heavy && !boss_tough {
            f.act = Act::Down;
            f.act_t = 0;
            f.inv = 46;
        } else {
            f.act = if f.grounded && upv <= 0.0 { Act::Hurt } else { Act::Hurt };
            f.act_t = 0;
            if boss_tough {
                f.stun = f.stun.min(3);
            }
            f.inv = 7;
        }
        true
    }

    fn ko(&mut self, v: usize, by: i8) {
        if !self.f[v].alive {
            return;
        }
        self.drop_gems(v, true);
        if self.f[v].held >= 0 {
            self.drop_held(v);
        }
        let had_power = self.f[v].power > 0.0;
        let pos = self.f[v].pos;
        {
            let f = &mut self.f[v];
            f.alive = false;
            f.hp = 0.0;
            f.act = Act::Dead;
            f.ko_tick = self.tick;
            f.dead_t = 0;
            f.power = 0.0;
            if f.vel[1] < 400.0 {
                f.vel[1] = 520.0;
            }
        }
        if by >= 0 && by as usize != v {
            self.f[by as usize].kos += 1;
        }
        self.kos_total += 1;
        self.ev.push(Ev::Ko { id: v, pos });
        if had_power {
            self.power_ended();
        }
    }

    fn power_ended(&mut self) {
        // A fresh batch of gems falls a few seconds after a power form ends, if nobody holds any.
        let any_live = self.gems.iter().any(|g| matches!(g.state, GemState::Free | GemState::Held(_) | GemState::Pending(_)));
        if !any_live && matches!(self.phase, Phase::Fight | Phase::Intro) {
            self.assign_gems(150);
        }
    }

    /// Gems fly out of a hurt fighter: one on a solid hit, all of them on a knock-down.
    fn drop_gems(&mut self, v: usize, all: bool) {
        let mut dropped = 0;
        for g in 0..GEMS {
            if self.gems[g].state == GemState::Held(v as u8) {
                if dropped > 0 && !all {
                    continue;
                }
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let pos = self.f[v].pos;
                self.gems[g] = Gem { state: GemState::Free, pos: [pos[0], pos[1] + 90.0, pos[2]], vel: [a.cos() * 320.0, 760.0, a.sin() * 320.0], spot: self.gems[g].spot, immune: v as i8, immune_t: 45 };
                self.f[v].gems &= !(1 << g);
                dropped += 1;
                self.ev.push(Ev::GemDrop(g));
            }
        }
    }

    fn collect_gem(&mut self, i: usize, g: usize) {
        self.gems[g].state = GemState::Held(i as u8);
        self.f[i].gems |= 1 << g;
        self.f[i].gems_got += 1;
        let n = self.f[i].gems.count_ones();
        self.ev.push(Ev::Gem { id: i, n, pos: self.f[i].pos });
        if n as usize == GEMS && !self.f[i].powered() {
            self.transform(i);
        }
    }

    fn transform(&mut self, i: usize) {
        for g in 0..GEMS {
            self.gems[g].state = GemState::Spent;
        }
        let f = &mut self.f[i];
        f.gems = 0;
        f.power = POWER_SECS;
        f.transforms += 1;
        f.inv = f.inv.max(24);
        f.cd_s1 = 8;
        f.cd_s2 = 8;
        if matches!(f.act, Act::Hurt | Act::Down | Act::GetUp) {
            f.act = if f.grounded { Act::Idle } else { Act::Air };
        }
        let pos = f.pos;
        self.ev.push(Ev::Transform { id: i, pos });
        // The change blasts everyone nearby away: the payoff of three gems is felt at once.
        self.ev.push(Ev::Nova { pos, r: 520.0 });
        self.shockwave(i, 520.0, 12.0, 860.0, 560.0);
        self.fling_items(pos, 520.0, i);
    }

    // -----------------------------------------------------------------------------------------
    // Items, projectiles, gems
    // -----------------------------------------------------------------------------------------

    fn step_items(&mut self) {
        for k in 0..MAXI {
            if !self.items[k].alive {
                continue;
            }
            let it = self.items[k];
            match it.state {
                IState::Held(h) => {
                    let f = &self.f[h as usize];
                    if !f.alive {
                        self.items[k].state = IState::Rest;
                        self.items[k].owner = -1;
                    } else {
                        let d = dir_of(f.yaw);
                        let lift = if it.kind.weapon() { 55.0 } else { 105.0 } * f.size;
                        self.items[k].pos = [f.pos[0] + d[0] * 46.0 * f.size, f.pos[1] + lift, f.pos[2] + d[1] * 46.0 * f.size];
                        self.items[k].vel = [0.0; 3];
                    }
                    if it.kind == ItemKind::Bomb {
                        self.items[k].fuse -= 1;
                        if self.items[k].fuse <= 0 {
                            let (owner, pos) = (it.owner, self.items[k].pos);
                            self.items[k].alive = false;
                            if let IState::Held(h) = it.state {
                                self.f[h as usize].held = -1;
                            }
                            self.explode(pos, 280.0, 20.0, 920.0, owner);
                        }
                    }
                    continue;
                }
                IState::Rest | IState::Flying => {}
            }
            self.step_item_physics(k);
            if !self.items[k].alive {
                continue;
            }
            // Resting pickups: hearts heal, boots and bubbles buff, all on touch.
            let it = self.items[k];
            if it.kind.pickup() && it.state == IState::Rest {
                for j in 0..self.n {
                    let f = &self.f[j];
                    let wants = f.alive && (it.kind != ItemKind::Heal || f.hp < f.max_hp);
                    if wants && dist2([it.pos[0], it.pos[2]], pos_of(f)) < 80.0 + f.radius() && (it.pos[1] - f.pos[1]).abs() < 140.0 {
                        let f = &mut self.f[j];
                        match it.kind {
                            ItemKind::Boots => f.haste = HASTE_TICKS,
                            ItemKind::Shield => f.shield = SHIELD_TICKS,
                            _ => f.hp = (f.hp + 32.0).min(f.max_hp),
                        }
                        self.items[k].alive = false;
                        self.ev.push(if it.kind == ItemKind::Heal { Ev::Heal(j) } else { Ev::Buff { id: j, kind: it.kind as u8 } });
                        break;
                    }
                }
                continue;
            }
            if it.kind == ItemKind::Bomb && it.fuse > 0 {
                self.items[k].fuse -= 1;
                if self.items[k].fuse <= 0 {
                    self.items[k].alive = false;
                    self.explode(it.pos, 280.0, 20.0, 920.0, it.owner);
                    continue;
                }
            }
            // Flying hits.
            if it.state == IState::Flying {
                self.items[k].grace = it.grace.saturating_sub(1);
                let (dmg, kb, up, _) = it.kind.throw_stats();
                let speed = it.vel[0].hypot(it.vel[2]);
                for j in 0..self.n {
                    let f = self.f[j];
                    if !f.alive || (j as i8 == it.owner && self.items[k].grace > 0) {
                        continue;
                    }
                    let near = dist2([it.pos[0], it.pos[2]], pos_of(&f)) < f.radius() + it.kind.radius();
                    if !near || it.pos[1] - f.pos[1] < -30.0 || it.pos[1] - f.pos[1] > f.height() {
                        continue;
                    }
                    if it.owner >= 0 && self.f[it.owner as usize].team == f.team && j as i8 != it.owner {
                        continue;
                    }
                    if it.kind == ItemKind::Bomb {
                        self.items[k].alive = false;
                        self.explode(it.pos, 280.0, 20.0, 920.0, it.owner);
                        break;
                    }
                    let dir = norm2([it.vel[0], it.vel[2]], [f.pos[0] - it.pos[0], f.pos[2] - it.pos[2]]);
                    let scale = (speed / 800.0).clamp(0.5, 1.2);
                    if self.hit(j, it.owner, dmg, dir, kb * scale, up) {
                        match it.kind {
                            ItemKind::Crate | ItemKind::Barrel => {
                                self.items[k].alive = false;
                                self.ev.push(Ev::Break(it.pos));
                            }
                            _ => {
                                self.items[k].vel = [-it.vel[0] * 0.25, 260.0, -it.vel[2] * 0.25];
                                self.items[k].state = IState::Rest;
                            }
                        }
                        break;
                    }
                }
                if self.items[k].alive && self.items[k].state == IState::Flying && it.owner >= 0 && it.kind != ItemKind::Bomb {
                    let dir = norm2([it.vel[0], it.vel[2]], [1.0, 0.0]);
                    let scale = (speed / 800.0).clamp(0.5, 1.2);
                    if self.point_hits_mon(it.pos, it.kind.radius(), it.owner, dmg, dir, kb * scale, up) {
                        match it.kind {
                            ItemKind::Crate | ItemKind::Barrel => {
                                self.items[k].alive = false;
                                self.ev.push(Ev::Break(it.pos));
                            }
                            _ => {
                                self.items[k].vel = [-it.vel[0] * 0.25, 260.0, -it.vel[2] * 0.25];
                                self.items[k].state = IState::Rest;
                            }
                        }
                    }
                }
            }
        }
    }

    fn step_item_physics(&mut self, k: usize) {
        let mut it = self.items[k];
        it.vel[1] = (it.vel[1] - GRAV * 0.9 * DT).max(-2200.0);
        if self.arena.wind > 0.1 && it.state == IState::Rest {
            it.vel[0] += self.arena.wind_dir[0] * 520.0 * self.arena.wind * DT;
            it.vel[2] += self.arena.wind_dir[1] * 520.0 * self.arena.wind * DT;
        }
        let prev_y = it.pos[1];
        for a in 0..3 {
            it.pos[a] += it.vel[a] * DT;
        }
        it.spin += it.vel[0].hypot(it.vel[2]) * DT * 0.01 + if it.grounded { 0.0 } else { 0.18 };
        // Walls.
        let r = it.kind.radius();
        let mut p = it.pos;
        self.arena.push_out(&mut p, r);
        if p[0] != it.pos[0] {
            it.vel[0] *= -0.35;
        }
        if p[2] != it.pos[2] {
            it.vel[2] *= -0.35;
        }
        it.pos[0] = p[0];
        it.pos[2] = p[2];
        // Ground.
        it.grounded = false;
        if let Some((top, _)) = self.arena.support(it.pos[0], it.pos[2], prev_y + 6.0) {
            if it.pos[1] <= top && it.vel[1] <= 0.0 {
                it.pos[1] = top;
                let bounce = match it.kind {
                    ItemKind::Rock | ItemKind::Bomb => 0.42,
                    ItemKind::Crate | ItemKind::Heal | ItemKind::Boots | ItemKind::Shield => 0.15,
                    _ => 0.1,
                };
                if it.vel[1] < -500.0 {
                    it.vel[1] = -it.vel[1] * bounce;
                } else {
                    it.vel[1] = 0.0;
                    it.grounded = true;
                }
                let fr = match it.kind {
                    ItemKind::Barrel => 0.99,
                    ItemKind::Rock => 0.96,
                    _ => 0.9,
                };
                it.vel[0] *= fr;
                it.vel[2] *= fr;
                if it.state == IState::Flying && it.vel[0].hypot(it.vel[2]) < 150.0 {
                    it.state = IState::Rest;
                    it.owner = -1;
                }
                if it.state == IState::Flying && it.kind == ItemKind::Crate && it.vel[0].hypot(it.vel[2]) < 400.0 {
                    it.state = IState::Rest;
                    it.owner = -1;
                }
            }
        }
        // Bumpers fling items.
        for b in self.arena.bumpers.iter_mut() {
            let d = dist2([it.pos[0], it.pos[2]], [b.x, b.z]);
            if d < b.r + r && it.pos[1] < b.h {
                let dir = norm2([it.pos[0] - b.x, it.pos[2] - b.z], [1.0, 0.0]);
                it.vel = [dir[0] * 800.0, 420.0, dir[1] * 800.0];
                it.pos[0] = b.x + dir[0] * (b.r + r + 2.0);
                it.pos[2] = b.z + dir[1] * (b.r + r + 2.0);
                b.flash = 8;
            }
        }
        if it.pos[1] < self.arena.void_y {
            it.alive = false;
        }
        if self.arena.kind == ArenaKind::Ember && it.pos[1] < self.arena.lava - 10.0 && !it.kind.pickup() && it.grounded {
            it.alive = false;
            self.ev.push(Ev::Break(it.pos));
        }
        self.items[k] = it;
    }

    pub fn explode(&mut self, pos: [f32; 3], r: f32, dmg: f32, kb: f32, owner: i8) {
        self.ev.push(Ev::Boom { pos, r });
        for j in 0..self.n {
            let f = self.f[j];
            if !f.alive {
                continue;
            }
            let d = dist2([pos[0], pos[2]], pos_of(&f));
            if d > r + f.radius() || (f.pos[1] - pos[1]).abs() > 240.0 {
                continue;
            }
            let fall = 1.0 - 0.5 * (d / r).min(1.0);
            let dir = norm2([f.pos[0] - pos[0], f.pos[2] - pos[2]], [0.0, 1.0]);
            // A blast also gets through the victim's recovery frames: it is not a melee hit.
            let was = self.f[j].inv;
            if owner >= 0 && self.f[owner as usize].team == f.team && owner as usize != j {
                continue;
            }
            self.f[j].inv = 0;
            if !self.hit(j, owner, dmg * fall, dir, kb * fall, 560.0 * fall) {
                self.f[j].inv = was;
            }
        }
        self.explode_mons(pos, r, dmg, kb, owner);
        for k in 0..MAXI {
            let it = &mut self.items[k];
            if it.alive && it.state == IState::Rest && dist2([it.pos[0], it.pos[2]], [pos[0], pos[2]]) < r {
                let dir = norm2([it.pos[0] - pos[0], it.pos[2] - pos[2]], [1.0, 0.0]);
                it.vel = [dir[0] * 700.0, 600.0, dir[1] * 700.0];
            }
        }
    }

    fn step_projectiles(&mut self) {
        for k in 0..MAXP {
            if !self.proj[k].alive {
                continue;
            }
            let mut p = self.proj[k];
            p.t += 1;
            match p.kind {
                PKind::Bullet => {
                    for a in 0..3 {
                        p.pos[a] += p.vel[a] * DT;
                    }
                    let mut dead = p.t > 40 || p.pos[1] < self.arena.void_y;
                    if self.wall_at(p.pos) {
                        dead = true;
                    }
                    for j in 0..self.n {
                        let f = self.f[j];
                        if !f.alive || j as i8 == p.owner || (p.owner >= 0 && self.f[p.owner as usize].team == f.team) {
                            continue;
                        }
                        if dist2([p.pos[0], p.pos[2]], pos_of(&f)) < f.radius() + 18.0 && p.pos[1] > f.pos[1] - 20.0 && p.pos[1] < f.pos[1] + f.height() {
                            let dir = norm2([p.vel[0], p.vel[2]], [1.0, 0.0]);
                            self.hit(j, p.owner, 7.0, dir, 190.0, 80.0);
                            dead = true;
                            break;
                        }
                    }
                    if !dead && p.owner >= 0 && self.point_hits_mon(p.pos, 18.0, p.owner, 7.0, norm2([p.vel[0], p.vel[2]], [1.0, 0.0]), 190.0, 80.0) {
                        dead = true;
                    }
                    p.alive = !dead;
                }
                PKind::Rock => {
                    for a in 0..3 {
                        p.pos[a] += p.vel[a] * DT;
                    }
                    let mut dead = p.t > 90 || p.pos[1] < self.arena.void_y || self.wall_at(p.pos);
                    if !self.arena.over_floor(p.pos[0], p.pos[2]) && p.t > 6 {
                        dead = true;
                    }
                    for j in 0..self.n {
                        let f = self.f[j];
                        if !f.alive {
                            continue;
                        }
                        if dist2([p.pos[0], p.pos[2]], pos_of(&f)) < f.radius() + 24.0 && p.pos[1] > f.pos[1] - 20.0 && p.pos[1] < f.pos[1] + f.height() {
                            let dir = norm2([p.vel[0], p.vel[2]], [1.0, 0.0]);
                            self.hit(j, -1, p.mask as f32, dir, 280.0, 140.0);
                            dead = true;
                            break;
                        }
                    }
                    p.alive = !dead;
                }
                PKind::Snow => {
                    p.vel[1] -= GRAV * 0.55 * DT;
                    for a in 0..3 {
                        p.pos[a] += p.vel[a] * DT;
                    }
                    let land = p.aim[1];
                    if p.pos[1] <= land && p.vel[1] < 0.0 {
                        p.alive = false;
                        self.proj[k] = p;
                        self.mon_blast = true;
                        self.explode([p.aim[0], land, p.aim[2]], 175.0, p.mask as f32, 760.0, -1);
                        self.mon_blast = false;
                        continue;
                    }
                }
                PKind::Boomerang => {
                    let owner = p.owner as usize;
                    if p.t == 13 {
                        p.mask = 0;
                    }
                    if p.t > 13 {
                        let o = self.f[owner];
                        let to = [o.pos[0] - p.pos[0], o.pos[2] - p.pos[2]];
                        let d = norm2(to, [1.0, 0.0]);
                        p.vel = [d[0] * 1250.0, 0.0, d[1] * 1250.0];
                        if len2(to) < 70.0 || !o.alive || p.t > 90 {
                            p.alive = false;
                            if o.alive && o.held >= 0 {
                                // caught
                                let h = o.held as usize;
                                self.items[h].uses = self.items[h].uses.saturating_sub(1);
                                if self.items[h].uses == 0 {
                                    self.items[h].alive = false;
                                    self.f[owner].held = -1;
                                    self.ev.push(Ev::Break(o.pos));
                                }
                            }
                            self.f[owner].boom_out = false;
                        }
                    }
                    for a in 0..3 {
                        p.pos[a] += p.vel[a] * DT;
                    }
                    if p.t <= 13 && self.wall_at(p.pos) {
                        p.t = 14;
                    }
                    if p.alive {
                        for j in 0..self.n {
                            let f = self.f[j];
                            if !f.alive || j == owner || p.mask & (1 << j) != 0 || self.f[owner].team == f.team {
                                continue;
                            }
                            if dist2([p.pos[0], p.pos[2]], pos_of(&f)) < f.radius() + 36.0 && p.pos[1] > f.pos[1] - 40.0 && p.pos[1] < f.pos[1] + f.height() {
                                p.mask |= 1 << j;
                                let dir = norm2([p.vel[0], p.vel[2]], [1.0, 0.0]);
                                self.hit(j, p.owner, 9.0, dir, 330.0, 150.0);
                            }
                        }
                    }
                }
                PKind::Shell => {
                    p.vel[1] -= GRAV * 0.55 * DT;
                    for a in 0..3 {
                        p.pos[a] += p.vel[a] * DT;
                    }
                    let land = p.aim[1];
                    if p.pos[1] <= land && p.vel[1] < 0.0 {
                        p.alive = false;
                        self.proj[k] = p;
                        self.explode([p.aim[0], land, p.aim[2]], 235.0, 18.0, 900.0, -1);
                        continue;
                    }
                }
            }
            self.proj[k] = p;
        }
    }

    /// Is `pos` inside a wall (a solid taller than it)?
    fn wall_at(&self, pos: [f32; 3]) -> bool {
        self.arena.blocked_point(pos) || self.arena.solids.iter().any(|s| s.active() && pos[1] < s.top - 10.0 && (pos[0] - s.x).abs() < s.hx && (pos[2] - s.z).abs() < s.hz && s.top > 60.0 && pos[1] > -400.0 && self.arena.support(pos[0], pos[2], 0.0).is_none_or(|(t, _)| t < s.top - STEP))
    }

    fn step_gems(&mut self) {
        for g in 0..GEMS {
            let mut gem = self.gems[g];
            match gem.state {
                GemState::Pending(at) => {
                    if self.tick >= at {
                        gem.state = GemState::Free;
                        gem.vel = [0.0; 3];
                    }
                }
                GemState::Held(h) => {
                    let f = &self.f[h as usize];
                    gem.pos = f.pos;
                    if !f.alive {
                        gem.state = GemState::Free;
                    }
                }
                GemState::Spent => {}
                GemState::Free => {
                    gem.immune_t = gem.immune_t.saturating_sub(1);
                    gem.vel[1] -= GRAV * 0.7 * DT;
                    let prev = gem.pos[1];
                    for a in 0..3 {
                        gem.pos[a] += gem.vel[a] * DT;
                    }
                    let mut p = gem.pos;
                    self.arena.push_out(&mut p, 24.0);
                    gem.pos[0] = p[0];
                    gem.pos[2] = p[2];
                    if let Some((top, _)) = self.arena.support(gem.pos[0], gem.pos[2], prev + self.arena.gem_reach()) {
                        if gem.pos[1] <= top + 4.0 && gem.vel[1] <= 0.0 {
                            gem.pos[1] = top + 4.0;
                            if gem.vel[1] < -400.0 {
                                gem.vel[1] = -gem.vel[1] * 0.42;
                                self.ev.push(Ev::GemLand(gem.pos));
                            } else {
                                gem.vel[1] = 0.0;
                            }
                            gem.vel[0] *= 0.85;
                            gem.vel[2] *= 0.85;
                        }
                    }
                    for b in self.arena.bumpers.iter_mut() {
                        if dist2([gem.pos[0], gem.pos[2]], [b.x, b.z]) < b.r + 26.0 && gem.pos[1] < b.h {
                            let dir = norm2([gem.pos[0] - b.x, gem.pos[2] - b.z], [1.0, 0.0]);
                            gem.vel = [dir[0] * 700.0, 500.0, dir[1] * 700.0];
                            gem.pos[0] = b.x + dir[0] * (b.r + 28.0);
                            gem.pos[2] = b.z + dir[1] * (b.r + 28.0);
                            b.flash = 8;
                        }
                    }
                    // A gem never leaves the level: flung over the edge it stops at the rim of the floor ...
                    if !self.arena.over_floor(gem.pos[0], gem.pos[2]) {
                        let q = self.arena.nearest_floor_point(gem.pos[0], gem.pos[2], 30.0);
                        gem.pos[0] = q[0];
                        gem.pos[2] = q[1];
                        gem.vel[0] = 0.0;
                        gem.vel[2] = 0.0;
                    }
                    // ... and one that sinks into the lava, drops through a gap or falls out of the world comes
                    // back down on solid ground (its home spot, else the nearest floor).
                    // (Below the level with no surface at or under it: off the rim, through a gap, under the floor.)
                    let lost = gem.pos[1] < -20.0 && self.arena.support(gem.pos[0], gem.pos[2], gem.pos[1] + 25.0).is_none();
                    let drowned = self.arena.kind == ArenaKind::Ember && gem.pos[1] < self.arena.lava - 20.0;
                    if gem.pos[1] < self.arena.void_y || drowned || lost {
                        let s = self.arena.respawn_spot(self.arena.gem_spots[gem.spot]);
                        gem.pos = [s[0], s[1] + 500.0, s[2]];
                        gem.vel = [0.0; 3];
                    }
                    // Collect.
                    for i in 0..self.n {
                        let f = &self.f[i];
                        if !f.alive || (gem.immune == i as i8 && gem.immune_t > 0) || matches!(f.act, Act::Hurt | Act::Down | Act::GetUp) {
                            continue;
                        }
                        if f.kind == Kind::KingBomb {
                            continue;
                        }
                        if dist2([gem.pos[0], gem.pos[2]], pos_of(f)) < 78.0 + f.radius() && gem.pos[1] - f.pos[1] > -50.0 && gem.pos[1] - f.pos[1] < f.height() + 30.0 {
                            self.gems[g] = gem;
                            self.collect_gem(i, g);
                            gem = self.gems[g];
                            break;
                        }
                    }
                }
            }
            self.gems[g] = gem;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Hazards, drops, rounds
    // -----------------------------------------------------------------------------------------

    fn step_hazards(&mut self) {
        // Bumper pillars.
        for bi in 0..self.arena.bumpers.len() {
            let b = self.arena.bumpers[bi];
            for i in 0..self.n {
                let f = self.f[i];
                if !f.alive || f.pos[1] > b.h {
                    continue;
                }
                let d = dist2(pos_of(&f), [b.x, b.z]);
                if d < b.r + f.radius() {
                    let dir = norm2([f.pos[0] - b.x, f.pos[2] - b.z], [1.0, 0.0]);
                    let speed = f.speed2();
                    let push = (900.0f32).max(speed * 1.1) / f.stats().weight.sqrt();
                    let mut dead = false;
                    let fp;
                    {
                        let f = &mut self.f[i];
                        f.pos[0] = b.x + dir[0] * (b.r + f.radius() + 1.0);
                        f.pos[2] = b.z + dir[1] * (b.r + f.radius() + 1.0);
                        f.vel = [dir[0] * push, 380.0, dir[1] * push];
                        f.grounded = false;
                        if f.inv == 0 {
                            f.hp -= 2.0;
                            f.act = Act::Hurt;
                            f.act_t = 0;
                            f.stun = 8;
                            f.inv = 10;
                            dead = f.hp <= 0.0;
                        }
                        fp = f.pos[1];
                    }
                    if dead {
                        let by = self.f[i].last_hit_by;
                        self.ko(i, by);
                    }
                    self.arena.bumpers[bi].flash = 8;
                    self.ev.push(Ev::Bump([b.x + dir[0] * b.r, fp + 60.0, b.z + dir[1] * b.r]));
                }
            }
        }
        // The cannon takes aim at someone.
        if self.arena.has_cannon {
            self.arena.cannon_cd -= 1;
            // Turn the barrel toward the next victim for the last second before it fires.
            if self.arena.cannon_cd <= 40 {
                if let Some(t) = self.cannon_target() {
                    let want = yaw_of([self.f[t].pos[0], self.f[t].pos[2]]);
                    self.arena.cannon_yaw += angle_diff(self.arena.cannon_yaw, want).clamp(-0.2, 0.2);
                }
            }
            if self.arena.cannon_cd <= 0 {
                self.arena.cannon_cd = if self.round_t < 35.0 { 105 } else { 150 };
                if let Some(t) = self.cannon_target() {
                    let f = self.f[t];
                    let aim = [f.pos[0] + f.vel[0] * 0.45, self.arena.support(f.pos[0], f.pos[2], 1000.0).map_or(0.0, |s| s.0), f.pos[2] + f.vel[2] * 0.45];
                    let from = [0.0, 190.0, 0.0];
                    let tt = 40.0 * DT;
                    let g = GRAV * 0.55;
                    let vel = [(aim[0] - from[0]) / tt, (aim[1] - from[1]) / tt + g * tt * 0.5, (aim[2] - from[2]) / tt];
                    if self.spawn_proj(PKind::Shell, from, vel, -1, aim) {
                        self.ev.push(Ev::Cannon);
                    }
                }
            }
        }
        // The region's own set pieces (Chomp, Whomps, rollers, drifters ...).
        if !self.arena.hazards.is_empty() {
            self.step_course_hazards();
        }
        // Real-world regions: shells rain on the fighters after the first moments of a round, faster in sudden death.
        if self.arena.barrage > 0 && self.round_t < ROUND_SECS - 12.0 {
            self.arena.barrage_cd -= 1;
            if self.arena.barrage_cd <= 0 {
                let base = if self.sudden { self.arena.barrage / 2 } else { self.arena.barrage };
                self.arena.barrage_cd = base + self.rng.below(30) as i32;
                self.strike_somewhere();
            }
        }
        // A real-world arena's rim closes in over the last half minute of the round, down to 55 % of its size.
        if self.arena.ground.is_some() && self.round_t < RIM_CLOSE_SECS {
            let k = 1.0 - 0.45 * ((RIM_CLOSE_SECS - self.round_t) / RIM_CLOSE_SECS).clamp(0.0, 1.0);
            let half = self.arena.half;
            if let Some(b) = self.arena.bounds.as_mut() {
                b[0] = b[0].min(half[0] * k);
                b[1] = b[1].min(half[1] * k);
            }
        }
        // Sudden death: after the bell, the round bleeds everyone slowly.
        if self.round_t <= 0.0 {
            self.sudden = true;
        }
    }

    /// A shell falls from the sky on a random live fighter's (led) position, with the cannon shell's red warning ring.
    fn strike_somewhere(&mut self) {
        let alive = self.alive_count();
        if alive == 0 {
            return;
        }
        let mut pick = self.rng.below(alive);
        let mut who = 0;
        for i in 0..self.n {
            if self.f[i].alive {
                if pick == 0 {
                    who = i;
                    break;
                }
                pick -= 1;
            }
        }
        let f = self.f[who];
        let jitter = [self.rng.range(-110.0, 110.0), self.rng.range(-110.0, 110.0)];
        let (x, z) = (f.pos[0] + f.vel[0] * 0.5 + jitter[0], f.pos[2] + f.vel[2] * 0.5 + jitter[1]);
        let Some((top, _)) = self.arena.support(x, z, f.pos[1] + 400.0) else { return };
        let aim = [x, top, z];
        if self.spawn_proj(PKind::Shell, [x, top + 950.0, z], [0.0, -200.0, 0.0], -1, aim) {
            self.ev.push(Ev::Barrage(aim));
        }
    }

    fn cannon_target(&self) -> Option<usize> {
        // The healthiest fighter draws the fire, so the cannon keeps the leader honest.
        let mut best: Option<(usize, f32)> = None;
        for i in 0..self.n {
            let f = &self.f[i];
            if f.alive && best.is_none_or(|(_, h)| f.hp > h) {
                best = Some((i, f.hp));
            }
        }
        best.map(|b| b.0)
    }

    fn spawn_drops(&mut self) {
        self.item_timer -= 1;
        if self.item_timer > 0 {
            return;
        }
        let live = self.items.iter().filter(|i| i.alive).count();
        let cap = 10 + self.n;
        self.item_timer = 120 + self.rng.below(60) as i32;
        if live >= cap {
            return;
        }
        let spot = self.arena.random_drop_spot(&mut self.rng);
        let low_hp = (0..self.n).any(|i| self.f[i].alive && self.f[i].hp_frac() < 0.45);
        let kind = if low_hp && self.rng.chance(0.3) { ItemKind::Heal } else { self.roll_item(false) };
        self.spawn_item(kind, spot, true);
    }

    fn check_round_end(&mut self) {
        let mut winner: i8 = -2;
        match self.cfg.mode {
            Mode::Versus => {
                let n_alive = self.alive_count();
                if n_alive <= 1 {
                    winner = (0..self.n).find(|&i| self.f[i].alive).map_or_else(
                        || (0..self.n).max_by_key(|&i| self.f[i].ko_tick).map_or(-1, |i| i as i8),
                        |i| i as i8,
                    );
                } else if self.round_t <= 0.0 {
                    let mut best = (-1i8, -1.0f32);
                    let mut tie = false;
                    for i in 0..self.n {
                        if !self.f[i].alive {
                            continue;
                        }
                        let h = self.f[i].hp;
                        if h > best.1 + 0.01 {
                            best = (i as i8, h);
                            tie = false;
                        } else if (h - best.1).abs() <= 0.01 {
                            tie = true;
                        }
                    }
                    winner = if tie { -1 } else { best.0 };
                }
            }
            Mode::Gauntlet => {
                let bots_alive = (1..self.n).any(|i| self.f[i].alive);
                if !self.f[0].alive {
                    winner = 1;
                } else if !bots_alive {
                    winner = 0;
                } else if self.round_t <= 0.0 {
                    winner = 1;
                }
            }
        }
        if winner != -2 {
            self.end_round(winner);
        }
    }

    fn end_round(&mut self, winner: i8) {
        self.winner = winner;
        self.phase = Phase::RoundEnd;
        self.phase_t = END_TICKS;
        if winner >= 0 {
            self.f[winner as usize].wins += 1;
        }
        self.ev.push(Ev::RoundEnd { winner });
    }

    fn after_round(&mut self) {
        match self.cfg.mode {
            Mode::Versus => {
                let target = self.cfg.rounds_to_win;
                let leader = (0..self.n).max_by_key(|&i| (self.f[i].wins, std::cmp::Reverse(i)));
                if let Some(l) = leader {
                    if self.f[l].wins >= target || self.round >= MAX_ROUNDS {
                        self.match_winner = l as i8;
                        self.phase = Phase::MatchEnd;
                        self.ev.push(Ev::MatchEnd { winner: l as i8 });
                        return;
                    }
                }
                self.begin_round();
            }
            Mode::Gauntlet => {
                if self.winner == 0 {
                    self.stage_clears += 1;
                    self.stage += 1;
                    if self.stage >= GAUNTLET_STAGES {
                        self.stage = GAUNTLET_STAGES - 1;
                        self.match_winner = 0;
                        self.phase = Phase::MatchEnd;
                        self.ev.push(Ev::MatchEnd { winner: 0 });
                        return;
                    }
                } else {
                    self.lives = self.lives.saturating_sub(1);
                    if self.lives == 0 {
                        self.match_winner = 1;
                        self.phase = Phase::MatchEnd;
                        self.ev.push(Ev::MatchEnd { winner: 1 });
                        return;
                    }
                }
                self.setup_roster();
                self.begin_round();
            }
        }
    }

    /// Hash of the whole simulation state, for determinism tests.
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: f32| {
            h ^= v.to_bits() as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        };
        for f in &self.f[..self.n] {
            for v in f.pos.iter().chain(f.vel.iter()) {
                mix(*v);
            }
            mix(f.hp);
            mix(f.power);
            if f.hero != Hero::Mario {
                mix(f.hero as u8 as f32);
            }
        }
        for it in self.items.iter().filter(|i| i.alive) {
            for v in it.pos {
                mix(v);
            }
        }
        for g in &self.gems {
            for v in g.pos {
                mix(v);
            }
        }
        mix(self.round_t);
        self.mons_digest(&mut mix);
        self.hazards_digest(&mut mix);
        h
    }

    /// Test hook: hand fighter `i` the first `n` gems (all three transform them).
    pub fn give_gems(&mut self, i: usize, n: usize) {
        for g in 0..n.min(GEMS) {
            self.gems[g].state = GemState::Free;
            self.gems[g].pos = self.f[i].pos;
            self.collect_gem(i, g);
        }
    }

    /// Test hook: start fighter `i` in the power form.
    pub fn give_power(&mut self, i: usize) {
        for g in 0..GEMS {
            self.gems[g].state = GemState::Spent;
        }
        self.f[i].power = POWER_SECS;
        self.f[i].transforms += 1;
    }

    /// Gem owned by nobody and sitting on the floor.
    pub fn free_gem_count(&self) -> usize {
        self.gems.iter().filter(|g| g.state == GemState::Free).count()
    }
}

#[cfg(test)]
#[path = "brawl_tests.rs"]
mod tests;
