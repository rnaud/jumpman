//! Star Deck, deterministic combat core: an original deckbuilder ruleset with no engine dependency.
//!
//! The cards are Mario moves. Instead of a flat per-turn energy, combat runs on **momentum**: a meter that
//! every card spends from and that builds when you *chain* moves in the order Jump -> Dive -> Pound (-> Jump...).
//! Each link of a chain refunds one momentum and adds bonus damage to attacks; Strike and Spin cards break a
//! chain, Brace and Trick cards leave it alone. Momentum only fades by one per turn and never below your stride.
//!
//! Everything random comes from the seeded [`Rng`], so a fight is reproducible from `(seed, inputs)`.
//! The run structure (map, rewards, shops, events) lives in `deck_run.rs`; presentation in `deck.rs`/`deck_ui.rs`.
pub use super::swarm_core::Rng;

pub const MAX_MOMENTUM: i32 = 8;
pub const BASE_STRIDE: i32 = 3;
pub const HAND_SIZE: usize = 5;
pub const MAX_HAND: usize = 10;
pub const MAX_ENEMIES: usize = 3;
pub const POTION_SLOTS: usize = 3;

// ---------------------------------------------------------------------------------------------
// Heroes, moves, statuses
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Hero {
    Mario,
    Link,
    YoungLink,
    Banjo,
}

impl Hero {
    pub const ALL: [Hero; 4] = [Hero::Mario, Hero::Link, Hero::YoungLink, Hero::Banjo];
    pub fn name(self) -> &'static str {
        match self {
            Hero::Mario => "Mario",
            Hero::Link => "Link",
            Hero::YoungLink => "Young Link",
            Hero::Banjo => "Banjo",
        }
    }
    pub fn max_hp(self) -> i32 {
        match self {
            Hero::Mario => 72,
            Hero::Link => 82,
            Hero::YoungLink => 68,
            Hero::Banjo => 64,
        }
    }
    /// One-line flavour of the starter deck.
    pub fn flavor(self) -> &'static str {
        match self {
            Hero::Mario => "Fire Flower scorches, Triple Jump flurries.",
            Hero::Link => "Spin Slash sweeps, Raise Shield holds.",
            Hero::YoungLink => "Quick seeds and Deku Nuts: dizzy, draw, repeat.",
            Hero::Banjo => "Beak Barge hits hard, Flap Flip keeps the hand full.",
        }
    }
    /// The two hero starter cards (keys).
    pub fn signature(self) -> [&'static str; 2] {
        match self {
            Hero::Mario => ["fire_flower", "triple_jump"],
            Hero::Link => ["spin_slash", "raise_shield"],
            Hero::YoungLink => ["seed_shot", "deku_nut"],
            Hero::Banjo => ["beak_barge", "flap_flip"],
        }
    }
    pub fn starter_deck(self) -> Vec<Card> {
        let mut keys = vec!["hop", "hop", "hop", "dive", "dive", "pound", "crouch", "crouch"];
        keys.extend(self.signature());
        keys.iter().map(|k| Card::new(card_id(k))).collect()
    }
}

/// What a card does physically; drives chaining and the hero's animation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Move {
    Jump,
    Dive,
    Pound,
    Strike,
    Spin,
    Brace,
    Trick,
}

impl Move {
    /// The move that continues a chain after this one.
    pub fn next(self) -> Option<Move> {
        match self {
            Move::Jump => Some(Move::Dive),
            Move::Dive => Some(Move::Pound),
            Move::Pound => Some(Move::Jump),
            _ => None,
        }
    }
    /// Brace and Trick cards neither extend nor break a chain.
    pub fn neutral(self) -> bool {
        matches!(self, Move::Brace | Move::Trick)
    }
    pub fn name(self) -> &'static str {
        match self {
            Move::Jump => "JUMP",
            Move::Dive => "DIVE",
            Move::Pound => "POUND",
            Move::Strike => "STRIKE",
            Move::Spin => "SPIN",
            Move::Brace => "BRACE",
            Move::Trick => "TRICK",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Status {
    /// Lose n health at turn end (ignores Shell), then n-1.
    Scorch,
    /// Deals 25% less damage for n turns.
    Dizzy,
    /// Takes 50% more damage for n turns.
    Shaken,
    /// +n damage on every hit.
    Pumped,
    /// +n Shell whenever Shell is gained.
    Tough,
    /// Attackers take n damage per hit.
    Spiky,
    /// Momentum never fades below the base stride plus n.
    Stride,
    /// Heal n at turn end, then n-1.
    Regen,
    /// Draw n extra cards each turn.
    Quick,
}

pub const STATUSES: [Status; 9] = [Status::Scorch, Status::Dizzy, Status::Shaken, Status::Pumped, Status::Tough, Status::Spiky, Status::Stride, Status::Regen, Status::Quick];

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Scorch => "Scorch",
            Status::Dizzy => "Dizzy",
            Status::Shaken => "Shaken",
            Status::Pumped => "Pumped",
            Status::Tough => "Tough",
            Status::Spiky => "Spiky",
            Status::Stride => "Stride",
            Status::Regen => "Regen",
            Status::Quick => "Quick",
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Status::Scorch => "Lose this much health at turn end, then it fades by 1.",
            Status::Dizzy => "Deals 25% less damage for this many turns.",
            Status::Shaken => "Takes 50% more damage for this many turns.",
            Status::Pumped => "Every hit deals this much extra damage.",
            Status::Tough => "Every Shell gained is this much bigger.",
            Status::Spiky => "Attackers take this much damage per hit.",
            Status::Stride => "Momentum never fades below the base stride plus this.",
            Status::Regen => "Heal this much at turn end, then it fades by 1.",
            Status::Quick => "Draw this many extra cards each turn.",
        }
    }
    pub fn debuff(self) -> bool {
        matches!(self, Status::Scorch | Status::Dizzy | Status::Shaken)
    }
    /// Short badge text.
    pub fn tag(self) -> &'static str {
        match self {
            Status::Scorch => "SCO",
            Status::Dizzy => "DIZ",
            Status::Shaken => "SHK",
            Status::Pumped => "PMP",
            Status::Tough => "TGH",
            Status::Spiky => "SPK",
            Status::Stride => "STR",
            Status::Regen => "REG",
            Status::Quick => "QCK",
        }
    }
    fn idx(self) -> usize {
        STATUSES.iter().position(|s| *s == self).unwrap()
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash)]
pub struct Statuses([i32; 9]);

impl Statuses {
    pub fn get(&self, s: Status) -> i32 {
        self.0[s.idx()]
    }
    pub fn add(&mut self, s: Status, n: i32) {
        let v = &mut self.0[s.idx()];
        *v = (*v + n).max(0);
    }
    pub fn set(&mut self, s: Status, n: i32) {
        self.0[s.idx()] = n.max(0);
    }
    pub fn iter(&self) -> impl Iterator<Item = (Status, i32)> + '_ {
        STATUSES.iter().map(|s| (*s, self.get(*s))).filter(|(_, n)| *n > 0)
    }
}

// ---------------------------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Rarity {
    Starter,
    Common,
    Uncommon,
    Rare,
    Curse,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Attack,
    Skill,
    Power,
    Curse,
}

/// One effect of a card; a card runs its list in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fx {
    /// Damage to the chosen foe.
    Dmg(i32),
    /// Damage to every foe.
    DmgAll(i32),
    /// `hits` x damage to the chosen foe.
    Multi(i32, i32),
    /// Damage per momentum spent (the card costs X).
    XHits(i32),
    /// `base + per * chain` damage to the chosen foe (replaces the chain bonus).
    ChainDmg(i32, i32),
    /// Damage equal to your Shell.
    ShellDmg,
    Shell(i32),
    Draw(i32),
    Mom(i32),
    Heal(i32),
    /// Lose health.
    Hurt(i32),
    Apply(Status, i32),
    ApplyAll(Status, i32),
    Own(Status, i32),
}

impl Fx {
    fn targeted(self) -> bool {
        matches!(self, Fx::Dmg(_) | Fx::Multi(..) | Fx::XHits(_) | Fx::ChainDmg(..) | Fx::ShellDmg | Fx::Apply(..))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CardDef {
    pub key: &'static str,
    pub name: &'static str,
    pub mv: Move,
    pub kind: Kind,
    pub rarity: Rarity,
    /// Momentum cost; -1 = X (spend everything).
    pub cost: i32,
    pub fx: &'static [Fx],
    pub up: &'static [Fx],
    pub up_cost: Option<i32>,
    pub exhaust: bool,
    pub retain: bool,
    pub unplayable: bool,
    /// Hero whose starter deck it belongs to.
    pub hero: Option<Hero>,
}

const fn card(key: &'static str, name: &'static str, mv: Move, kind: Kind, rarity: Rarity, cost: i32, fx: &'static [Fx], up: &'static [Fx]) -> CardDef {
    CardDef { key, name, mv, kind, rarity, cost, fx, up, up_cost: None, exhaust: false, retain: false, unplayable: false, hero: None }
}

impl CardDef {
    const fn ex(self) -> Self {
        CardDef { exhaust: true, ..self }
    }
    const fn keep(self) -> Self {
        CardDef { retain: true, ..self }
    }
    const fn upc(self, c: i32) -> Self {
        CardDef { up_cost: Some(c), ..self }
    }
    const fn of(self, h: Hero) -> Self {
        CardDef { hero: Some(h), ..self }
    }
    const fn dead(self) -> Self {
        CardDef { unplayable: true, ..self }
    }
}

use Fx::*;
use Kind::{Attack as A, Curse as Cu, Power as Pw, Skill as S};
use Move::{Brace as B, Dive as D, Jump as J, Pound as P, Spin as W, Strike as T, Trick as K};
use Rarity::{Common as C, Rare as R, Starter as St, Uncommon as U};
use Status::*;

pub static CARDS: &[CardDef] = &[
    // Shared starters
    card("hop", "Hop", J, A, St, 1, &[Dmg(5)], &[Dmg(8)]),
    card("dive", "Dive", D, A, St, 1, &[Dmg(6)], &[Dmg(9)]),
    card("pound", "Ground Pound", P, A, St, 2, &[Dmg(11)], &[Dmg(15)]),
    card("crouch", "Crouch", B, S, St, 1, &[Shell(5)], &[Shell(8)]),
    card("punch", "Punch", T, A, St, 0, &[Dmg(3)], &[Dmg(5)]),
    // Hero starters
    card("fire_flower", "Fire Flower", K, S, St, 1, &[Dmg(2), Apply(Scorch, 4)], &[Dmg(3), Apply(Scorch, 6)]).of(Hero::Mario),
    card("triple_jump", "Triple Jump", J, A, St, 2, &[Multi(3, 3)], &[Multi(3, 4)]).of(Hero::Mario),
    card("spin_slash", "Spin Slash", W, A, St, 2, &[DmgAll(9)], &[DmgAll(13)]).of(Hero::Link),
    card("raise_shield", "Raise Shield", B, S, St, 1, &[Shell(9)], &[Shell(12)]).of(Hero::Link),
    card("seed_shot", "Seed Shot", K, A, St, 0, &[Dmg(5)], &[Dmg(7)]).of(Hero::YoungLink),
    card("deku_nut", "Deku Nut", K, S, St, 1, &[ApplyAll(Dizzy, 1), Draw(1)], &[ApplyAll(Dizzy, 2), Draw(1)]).of(Hero::YoungLink),
    card("beak_barge", "Beak Barge", D, A, St, 1, &[Dmg(6), Apply(Dizzy, 1)], &[Dmg(9), Apply(Dizzy, 2)]).of(Hero::Banjo),
    card("flap_flip", "Flap Flip", J, S, St, 1, &[Shell(2), Draw(1)], &[Shell(4), Draw(1)]).of(Hero::Banjo),
    // Common
    card("cap_throw", "Cap Throw", K, A, C, 1, &[Dmg(7)], &[Dmg(10)]),
    card("sweep_kick", "Sweep Kick", T, A, C, 1, &[DmgAll(4)], &[DmgAll(6)]),
    card("wall_kick", "Wall Kick", J, A, C, 1, &[Dmg(4), Mom(1)], &[Dmg(6), Mom(1)]),
    card("belly_slide", "Belly Slide", D, A, C, 1, &[Dmg(8), Shell(3)], &[Dmg(11), Shell(4)]),
    card("quake_pound", "Quake Pound", P, A, C, 2, &[DmgAll(8)], &[DmgAll(11)]),
    card("side_flip", "Side Flip", J, S, C, 1, &[Shell(5), Draw(1)], &[Shell(7), Draw(1)]),
    card("long_jump", "Long Jump", J, A, C, 2, &[Dmg(14)], &[Dmg(18)]),
    card("spin_dash", "Spin Dash", W, A, C, 1, &[Multi(2, 4)], &[Multi(2, 6)]),
    card("pocket_change", "Pocket Change", K, A, C, 0, &[Dmg(4)], &[Dmg(7)]).ex(),
    card("shell_kick", "Shell Kick", T, A, C, 1, &[Dmg(5), Shell(5)], &[Dmg(7), Shell(7)]),
    card("rolling_bash", "Rolling Bash", D, A, C, 1, &[Dmg(5), Apply(Shaken, 1)], &[Dmg(7), Apply(Shaken, 2)]),
    card("backflip", "Backflip", J, S, C, 1, &[Shell(6)], &[Shell(9)]),
    card("heel_stomp", "Heel Stomp", P, A, C, 1, &[Dmg(6), Apply(Dizzy, 1)], &[Dmg(9), Apply(Dizzy, 2)]),
    card("butt_bounce", "Butt Bounce", P, A, C, 0, &[Dmg(4)], &[Dmg(7)]),
    card("ember_toss", "Ember Toss", K, A, C, 1, &[Apply(Scorch, 5)], &[Apply(Scorch, 8)]),
    card("super_mushroom", "Super Mushroom", K, S, C, 1, &[Heal(5)], &[Heal(8)]).ex(),
    card("shoulder_bash", "Shoulder Bash", T, A, C, 1, &[Dmg(9), Hurt(2)], &[Dmg(12), Hurt(2)]),
    // Uncommon
    card("twirl", "Twirl", W, S, U, 1, &[Shell(4), ApplyAll(Dizzy, 1)], &[Shell(6), ApplyAll(Dizzy, 2)]),
    card("combo_stomp", "Combo Stomp", P, A, U, 1, &[ChainDmg(4, 4)], &[ChainDmg(6, 5)]),
    card("star_spin", "Star Spin", W, A, U, -1, &[XHits(4)], &[XHits(6)]),
    card("fire_storm", "Fire Storm", K, S, U, 2, &[ApplyAll(Scorch, 4)], &[ApplyAll(Scorch, 6)]),
    card("metal_cap", "Metal Cap", B, S, U, 2, &[Shell(10), Own(Spiky, 2)], &[Shell(14), Own(Spiky, 3)]),
    card("vanish_cap", "Vanish Cap", B, S, U, 1, &[Shell(7)], &[Shell(10)]).keep(),
    card("wing_dash", "Wing Dash", D, A, U, 2, &[Dmg(10), Draw(1), Mom(1)], &[Dmg(14), Draw(1), Mom(1)]),
    card("slide_kick", "Slide Kick", T, A, U, 1, &[Dmg(7), Mom(1)], &[Dmg(9), Mom(1)]),
    card("shell_smash", "Shell Smash", P, A, U, 2, &[ShellDmg], &[ShellDmg, Dmg(4)]).upc(1),
    card("run_up", "Run Up", K, S, U, 0, &[Mom(2)], &[Mom(3)]).ex(),
    card("second_wind", "Second Wind", K, S, U, 0, &[Draw(2)], &[Draw(3)]).ex(),
    card("taunt", "Taunt", K, S, U, 1, &[ApplyAll(Shaken, 2)], &[ApplyAll(Shaken, 3)]),
    // Powers
    card("stride_training", "Stride Training", K, Pw, U, 1, &[Own(Stride, 1)], &[Own(Stride, 1)]).upc(0),
    card("spiky_boots", "Spiky Boots", K, Pw, U, 1, &[Own(Spiky, 3)], &[Own(Spiky, 5)]),
    card("mushroom_diet", "Mushroom Diet", K, Pw, U, 2, &[Own(Pumped, 2)], &[Own(Pumped, 3)]),
    card("fast_hands", "Fast Hands", K, Pw, R, 1, &[Own(Quick, 1)], &[Own(Quick, 1)]).upc(0),
    card("regen_cap", "Regen Cap", K, Pw, U, 1, &[Own(Regen, 2)], &[Own(Regen, 3)]),
    card("tough_hide", "Tough Hide", K, Pw, U, 1, &[Own(Tough, 2)], &[Own(Tough, 3)]),
    // Rare
    card("mega_quake", "Mega Quake", P, A, R, 3, &[DmgAll(20), ApplyAll(Dizzy, 1)], &[DmgAll(26), ApplyAll(Dizzy, 2)]),
    card("super_star", "Super Star", K, S, R, 3, &[Shell(20), Own(Pumped, 2)], &[Shell(26), Own(Pumped, 3)]).ex(),
    card("rapid_hops", "Rapid Hops", J, A, R, 1, &[Multi(4, 3)], &[Multi(5, 3)]),
    card("final_flourish", "Final Flourish", D, A, R, 3, &[Dmg(24)], &[Dmg(32)]),
    card("star_burst", "Star Burst", K, A, R, 2, &[Dmg(8), Apply(Scorch, 8)], &[Dmg(11), Apply(Scorch, 11)]),
    // Curse
    card("bruise", "Bruise", K, Cu, Rarity::Curse, 0, &[], &[]).dead(),
];

pub fn card_id(key: &str) -> u8 {
    CARDS.iter().position(|c| c.key == key).unwrap_or_else(|| panic!("unknown card {key}")) as u8
}

pub fn find_card(key: &str) -> Option<u8> {
    CARDS.iter().position(|c| c.key == key).map(|i| i as u8)
}

/// A card in a deck: definition plus upgrade flag.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Card {
    pub id: u8,
    pub up: bool,
}

impl Card {
    pub fn new(id: u8) -> Self {
        Card { id, up: false }
    }
    pub fn upgraded(id: u8) -> Self {
        Card { id, up: true }
    }
    pub fn def(&self) -> &'static CardDef {
        &CARDS[self.id as usize]
    }
    pub fn fx(&self) -> &'static [Fx] {
        let d = self.def();
        if self.up { d.up } else { d.fx }
    }
    /// Momentum cost (-1 = X).
    pub fn cost(&self) -> i32 {
        let d = self.def();
        if self.up { d.up_cost.unwrap_or(d.cost) } else { d.cost }
    }
    pub fn can_upgrade(&self) -> bool {
        !self.up && self.def().kind != Kind::Curse
    }
    pub fn name(&self) -> String {
        format!("{}{}", self.def().name, if self.up { "+" } else { "" })
    }
    /// Needs a chosen foe?
    pub fn targeted(&self) -> bool {
        self.fx().iter().any(|f| f.targeted())
    }
    /// Rules text (what the card face prints).
    pub fn text(&self) -> String {
        let d = self.def();
        if d.unplayable {
            return "Unplayable. Clutters your hand.".into();
        }
        let mut parts: Vec<String> = self.fx().iter().map(|f| fx_text(*f)).collect();
        if d.retain {
            parts.push("Retain.".into());
        }
        if d.exhaust {
            parts.push("Exhaust.".into());
        }
        parts.join(" ")
    }
}

pub fn fx_text(f: Fx) -> String {
    match f {
        Dmg(n) => format!("Deal {n}."),
        DmgAll(n) => format!("Deal {n} to ALL."),
        Multi(h, n) => format!("Deal {n} x{h}."),
        XHits(n) => format!("Deal {n} x X."),
        ChainDmg(b, p) => format!("Deal {b}, +{p} per chain."),
        ShellDmg => "Deal damage = your Shell.".into(),
        Shell(n) => format!("Gain {n} Shell."),
        Draw(n) => format!("Draw {n}."),
        Mom(n) => format!("+{n} Momentum."),
        Heal(n) => format!("Heal {n}."),
        Hurt(n) => format!("Lose {n} HP."),
        Apply(s, n) => format!("{} {n}.", s.name()),
        ApplyAll(s, n) => format!("ALL {} {n}.", s.name()),
        Own(s, n) => format!("Gain {} {n}.", s.name()),
    }
}

/// The card pool rewards and shops draw from (no starters or curses).
pub fn reward_pool(r: Rarity) -> Vec<u8> {
    (0..CARDS.len() as u8).filter(|i| CARDS[*i as usize].rarity == r).collect()
}

// ---------------------------------------------------------------------------------------------
// Relics and potions
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Relic {
    CoinPouch,
    SpringBoots,
    BrickHelm,
    EmberBadge,
    CloudSeat,
    StarShard,
    BigMushroom,
    ChainAnklet,
    BlastPin,
    HeartLocket,
    ThornCharm,
}

pub const RELICS: [Relic; 11] = [Relic::CoinPouch, Relic::SpringBoots, Relic::BrickHelm, Relic::EmberBadge, Relic::CloudSeat, Relic::StarShard, Relic::BigMushroom, Relic::ChainAnklet, Relic::BlastPin, Relic::HeartLocket, Relic::ThornCharm];

impl Relic {
    pub fn name(self) -> &'static str {
        match self {
            Relic::CoinPouch => "Coin Pouch",
            Relic::SpringBoots => "Spring Boots",
            Relic::BrickHelm => "Brick Helm",
            Relic::EmberBadge => "Ember Badge",
            Relic::CloudSeat => "Cloud Seat",
            Relic::StarShard => "Star Shard",
            Relic::BigMushroom => "Big Mushroom",
            Relic::ChainAnklet => "Chain Anklet",
            Relic::BlastPin => "Blast Pin",
            Relic::HeartLocket => "Heart Locket",
            Relic::ThornCharm => "Thorn Charm",
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Relic::CoinPouch => "+30 coins now, +6 after every battle.",
            Relic::SpringBoots => "Base stride +1: momentum never fades below 4.",
            Relic::BrickHelm => "Start every battle with 6 Shell.",
            Relic::EmberBadge => "Your first attack each battle applies Scorch 3.",
            Relic::CloudSeat => "Draw 2 extra cards on the first turn.",
            Relic::StarShard => "Start every battle Pumped 1.",
            Relic::BigMushroom => "+12 max HP.",
            Relic::ChainAnklet => "Reaching a 3-link chain grants +2 momentum.",
            Relic::BlastPin => "When a foe falls, the others take 4.",
            Relic::HeartLocket => "Heal 6 after every battle.",
            Relic::ThornCharm => "Start every battle Spiky 2.",
        }
    }
    pub fn tag(self) -> &'static str {
        match self {
            Relic::CoinPouch => "CP",
            Relic::SpringBoots => "SB",
            Relic::BrickHelm => "BH",
            Relic::EmberBadge => "EB",
            Relic::CloudSeat => "CS",
            Relic::StarShard => "SS",
            Relic::BigMushroom => "BM",
            Relic::ChainAnklet => "CA",
            Relic::BlastPin => "BP",
            Relic::HeartLocket => "HL",
            Relic::ThornCharm => "TC",
        }
    }
    pub fn price(self) -> i32 {
        match self {
            Relic::BigMushroom | Relic::HeartLocket | Relic::CoinPouch => 120,
            Relic::BlastPin | Relic::ChainAnklet | Relic::CloudSeat => 150,
            _ => 135,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Potion {
    Tonic,
    FireFlask,
    ShellBrew,
    SpeedBrew,
    StarBrew,
}

pub const POTIONS: [Potion; 5] = [Potion::Tonic, Potion::FireFlask, Potion::ShellBrew, Potion::SpeedBrew, Potion::StarBrew];

impl Potion {
    pub fn name(self) -> &'static str {
        match self {
            Potion::Tonic => "Mushroom Tonic",
            Potion::FireFlask => "Fire Flask",
            Potion::ShellBrew => "Shell Brew",
            Potion::SpeedBrew => "Speed Brew",
            Potion::StarBrew => "Star Brew",
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Potion::Tonic => "Heal 15 HP.",
            Potion::FireFlask => "Deal 20 to a foe.",
            Potion::ShellBrew => "Gain 14 Shell.",
            Potion::SpeedBrew => "+3 Momentum, draw 1.",
            Potion::StarBrew => "Pumped 3 this battle.",
        }
    }
    pub fn targeted(self) -> bool {
        self == Potion::FireFlask
    }
    pub fn price(self) -> i32 {
        match self {
            Potion::Tonic | Potion::FireFlask => 50,
            _ => 40,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Enemies and intents
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EnemyKind {
    Scuttler,
    FusePup,
    Rammer,
    Prickle,
    Gloomwisp,
    Slab,
    Snapvine,
    Swooper,
    Zapper,
    Shellback,
    TetherHound,
    HornCaptain,
    FuseKing,
}

pub const ENEMY_KINDS: [EnemyKind; 13] = [
    EnemyKind::Scuttler,
    EnemyKind::FusePup,
    EnemyKind::Rammer,
    EnemyKind::Prickle,
    EnemyKind::Gloomwisp,
    EnemyKind::Slab,
    EnemyKind::Snapvine,
    EnemyKind::Swooper,
    EnemyKind::Zapper,
    EnemyKind::Shellback,
    EnemyKind::TetherHound,
    EnemyKind::HornCaptain,
    EnemyKind::FuseKing,
];

pub struct EnemyInfo {
    pub name: &'static str,
    pub model: &'static str,
    pub hp: (i32, i32),
    /// Model scale on the stage (1 = about Mario's height).
    pub scale: f32,
    pub hover: f32,
    pub elite: bool,
    pub boss: bool,
    /// Model height in world units at scale 1 (health plates sit above it).
    pub height: f32,
}

impl EnemyKind {
    pub fn info(self) -> EnemyInfo {
        let i = |name, model, lo, hi, scale, hover, elite, boss| EnemyInfo { name, model, hp: (lo, hi), scale, hover, elite, boss, height: self.native_height() };
        match self {
            EnemyKind::Scuttler => i("Scuttler", "actors/goomba/goomba_geo.glb", 13, 17, 1.5, 0.0, false, false),
            EnemyKind::FusePup => i("Fuse Pup", "actors/bobomb/black_bobomb_geo.glb", 11, 14, 0.8, 0.0, false, false),
            EnemyKind::Rammer => i("Rammer", "actors/bully/bully_geo.glb", 24, 29, 0.75, 0.0, false, false),
            EnemyKind::Prickle => i("Prickle", "actors/spiny/spiny_geo.glb", 15, 19, 1.5, 0.0, false, false),
            EnemyKind::Gloomwisp => i("Gloomwisp", "actors/boo/boo_geo.glb", 19, 24, 1.0, 0.45, false, false),
            EnemyKind::Slab => i("Slab", "actors/thwomp/thwomp_geo.glb", 30, 36, 0.8, 0.0, false, false),
            EnemyKind::Snapvine => i("Snapvine", "actors/piranha_plant/piranha_plant_geo.glb", 21, 26, 0.9, 0.0, false, false),
            EnemyKind::Swooper => i("Swooper", "actors/swoop/swoop_geo.glb", 9, 12, 1.2, 0.7, false, false),
            EnemyKind::Zapper => i("Zapper", "actors/amp/dAmpGeo.glb", 17, 21, 1.1, 0.45, false, false),
            EnemyKind::Shellback => i("Shellback", "actors/koopa/koopa_with_shell_geo.glb", 25, 30, 1.0, 0.0, false, false),
            EnemyKind::TetherHound => i("Tether Hound", "actors/chain_chomp/chain_chomp_geo.glb", 54, 60, 0.7, 0.0, true, false),
            EnemyKind::HornCaptain => i("Horn Captain", "actors/bully/bully_boss_geo.glb", 62, 68, 1.0, 0.0, true, false),
            EnemyKind::FuseKing => i("Fuse King", "actors/king_bobomb/king_bobomb_geo.glb", 150, 150, 0.62, 0.0, false, true),
        }
    }
    fn native_height(self) -> f32 {
        match self {
            EnemyKind::Scuttler => 1.0,
            EnemyKind::FusePup => 1.2,
            EnemyKind::Rammer => 1.6,
            EnemyKind::Prickle => 0.8,
            EnemyKind::Gloomwisp => 1.5,
            EnemyKind::Slab => 2.0,
            EnemyKind::Snapvine => 1.8,
            EnemyKind::Swooper => 0.8,
            EnemyKind::Zapper => 1.2,
            EnemyKind::Shellback => 1.4,
            EnemyKind::TetherHound => 1.8,
            EnemyKind::HornCaptain => 2.0,
            EnemyKind::FuseKing => 3.6,
        }
    }
    pub fn name(self) -> &'static str {
        self.info().name
    }
    /// Index into the model's animation list for the resting loop (-1 = no clips).
    pub fn idle_clip(self) -> usize {
        0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Intent {
    pub name: &'static str,
    pub dmg: i32,
    pub hits: i32,
    pub shell: i32,
    pub buff: Option<(Status, i32)>,
    pub debuff: Option<(Status, i32)>,
    /// Bruise cards shuffled into your discard pile.
    pub junk: i32,
    pub summon: Option<EnemyKind>,
    /// The foe is spent after acting (a blast).
    pub dies: bool,
    /// Nothing happens yet: a warning turn.
    pub winding: bool,
}

impl Intent {
    pub fn attacks(&self) -> bool {
        self.dmg > 0 && self.hits > 0
    }
    /// Display category for the icon.
    pub fn category(&self) -> IntentIcon {
        if self.winding {
            IntentIcon::Warn
        } else if self.attacks() {
            IntentIcon::Attack
        } else if self.junk > 0 || self.debuff.is_some() {
            IntentIcon::Debuff
        } else if self.summon.is_some() || self.buff.is_some() {
            IntentIcon::Buff
        } else if self.shell > 0 {
            IntentIcon::Guard
        } else {
            IntentIcon::Warn
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntentIcon {
    Attack,
    Guard,
    Buff,
    Debuff,
    Warn,
}

fn atk(name: &'static str, dmg: i32, hits: i32) -> Intent {
    Intent { name, dmg, hits, ..Default::default() }
}
fn guard(name: &'static str, shell: i32) -> Intent {
    Intent { name, shell, ..Default::default() }
}
fn buff(name: &'static str, s: Status, n: i32) -> Intent {
    Intent { name, buff: Some((s, n)), ..Default::default() }
}
fn debuff(name: &'static str, s: Status, n: i32) -> Intent {
    Intent { name, debuff: Some((s, n)), ..Default::default() }
}
fn wind(name: &'static str) -> Intent {
    Intent { name, winding: true, ..Default::default() }
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub hp: i32,
    pub max_hp: i32,
    pub shell: i32,
    pub st: Statuses,
    pub intent: Intent,
    /// Times this foe has acted (drives its pattern).
    pub acts: u32,
    /// Boss/elite phase (0-based) and acts taken within it.
    pub phase: u8,
    pub phase_acts: u32,
    pub alive: bool,
    /// Pending phase-change intent.
    pending: Option<Intent>,
    last: u8,
}

impl Enemy {
    pub fn new(kind: EnemyKind, rng: &mut Rng) -> Self {
        let (lo, hi) = kind.info().hp;
        let hp = lo + rng.below((hi - lo + 1) as u32) as i32;
        let mut e = Enemy { kind, hp, max_hp: hp, shell: 0, st: Statuses::default(), intent: Intent::default(), acts: 0, phase: 0, phase_acts: 0, alive: true, pending: None, last: 255 };
        e.intent = e.choose(rng);
        e
    }

    /// Phase thresholds (fraction of health) for elites and the boss.
    fn phase_for(&self) -> u8 {
        let f = self.hp as f32 / self.max_hp as f32;
        match self.kind {
            EnemyKind::TetherHound | EnemyKind::HornCaptain => (f <= 0.5) as u8,
            EnemyKind::FuseKing => (f <= 0.6) as u8 + (f <= 0.25) as u8,
            _ => 0,
        }
    }

    /// Pick the next intent (called after acting and at spawn).
    pub fn choose(&mut self, rng: &mut Rng) -> Intent {
        let want = self.phase_for();
        if want > self.phase {
            self.phase = want;
            self.phase_acts = 0;
            self.pending = Some(match (self.kind, want) {
                (EnemyKind::TetherHound, _) => Intent { name: "Frenzy", shell: 8, buff: Some((Status::Pumped, 3)), ..Default::default() },
                (EnemyKind::HornCaptain, _) => Intent { name: "Seeing Red", shell: 10, buff: Some((Status::Pumped, 3)), ..Default::default() },
                (_, 1) => Intent { name: "Bellow", shell: 14, buff: Some((Status::Pumped, 3)), ..Default::default() },
                _ => Intent { name: "Meltdown", shell: 10, buff: Some((Status::Pumped, 4)), ..Default::default() },
            });
        }
        if let Some(p) = self.pending.take() {
            return p;
        }
        let t = self.phase_acts;
        let mut pick = |n: u32, rng: &mut Rng, last: u8| -> u8 {
            // Random pick that avoids repeating the previous choice.
            let mut v = rng.below(n) as u8;
            if v == last {
                v = (v + 1) % n as u8;
            }
            v
        };
        let it = match self.kind {
            EnemyKind::Scuttler => {
                let v = pick(3, rng, self.last);
                self.last = v;
                match v {
                    0 => atk("Bonk", 5, 1),
                    1 => atk("Pounce", 7, 1),
                    _ => guard("Hunker", 5),
                }
            }
            EnemyKind::FusePup => match t % 3 {
                0 => buff("Light Fuse", Status::Pumped, 3),
                1 => atk("Skitter", 5, 1),
                _ => Intent { name: "Blast", dmg: 16, hits: 1, dies: true, ..Default::default() },
            },
            EnemyKind::Rammer => match t % 3 {
                0 => atk("Shove", 8, 1),
                1 => Intent { name: "Rev Up", shell: 6, buff: Some((Status::Pumped, 2)), ..Default::default() },
                _ => Intent { name: "Ram", dmg: 8, hits: 1, debuff: Some((Status::Dizzy, 1)), ..Default::default() },
            },
            EnemyKind::Prickle => {
                if t == 0 {
                    buff("Bristle", Status::Spiky, 3)
                } else if t % 2 == 1 {
                    atk("Poke", 6, 1)
                } else {
                    Intent { name: "Curl Up", dmg: 4, hits: 1, shell: 5, ..Default::default() }
                }
            }
            EnemyKind::Gloomwisp => match (t + (self.max_hp as u32 & 1)) % 3 {
                0 => debuff("Moan", Status::Dizzy, 2),
                1 => atk("Haunt", 9, 1),
                _ => guard("Fade", 8),
            },
            EnemyKind::Slab => match t % 3 {
                0 => guard("Rise", 6),
                1 => wind("Looming..."),
                _ => atk("Slam", 18, 1),
            },
            EnemyKind::Snapvine => match t % 3 {
                0 => atk("Chomp", 5, 2),
                1 => Intent { name: "Spit", dmg: 4, hits: 1, debuff: Some((Status::Shaken, 2)), ..Default::default() },
                _ => atk("Lunge", 11, 1),
            },
            EnemyKind::Swooper => {
                if rng.chance(0.5) {
                    atk("Swipe", 3, 1)
                } else {
                    atk("Flurry", 2, 2)
                }
            }
            EnemyKind::Zapper => match t % 3 {
                0 => Intent { name: "Static Shock", dmg: 4, hits: 1, junk: 1, ..Default::default() },
                1 => atk("Zap", 7, 1),
                _ => guard("Crackle", 6),
            },
            EnemyKind::Shellback => match t % 3 {
                0 => guard("Withdraw", 10),
                1 => atk("Shell Spin", 12, 1),
                _ => Intent { name: "Guard Swipe", dmg: 6, hits: 1, shell: 6, ..Default::default() },
            },
            EnemyKind::TetherHound => {
                let phase_off = if self.phase > 0 { 2 } else { 0 };
                match t % 4 {
                    0 => atk("Lunge", 14 + phase_off, 1),
                    1 => buff("Howl", Status::Pumped, 2),
                    2 => atk("Snap", 6, 2),
                    _ => Intent { name: "Maul", dmg: 10, hits: 1, debuff: Some((Status::Dizzy, 1)), ..Default::default() },
                }
            }
            EnemyKind::HornCaptain => {
                let extra = if self.phase > 0 { 3 } else { 0 };
                match t % 4 {
                    0 => guard("Lower Horns", 12),
                    1 => atk("Gore", 8 + extra, 2),
                    2 => atk("Charge", 18 + extra, 1),
                    _ => Intent { name: "Taunt", dmg: 5, hits: 1, debuff: Some((Status::Shaken, 2)), ..Default::default() },
                }
            }
            EnemyKind::FuseKing => match self.phase {
                0 => match t % 4 {
                    0 => atk("Royal Stomp", 14, 1),
                    1 => Intent { name: "Call the Guard", shell: 8, summon: Some(EnemyKind::FusePup), ..Default::default() },
                    2 => Intent { name: "Scepter Swing", dmg: 10, hits: 1, debuff: Some((Status::Dizzy, 1)), ..Default::default() },
                    _ => buff("Stoke Fuse", Status::Pumped, 2),
                },
                1 => match t % 3 {
                    0 => Intent { name: "Bomb Barrage", dmg: 7, hits: 3, junk: 1, ..Default::default() },
                    1 => Intent { name: "Lob Bombs", shell: 10, junk: 2, ..Default::default() },
                    _ => atk("Crown Slam", 24, 1),
                },
                _ => match t % 3 {
                    0 => atk("Rampage", 14, 2),
                    1 => wind("Winding Up!"),
                    _ => atk("Boom Stomp", 32, 1),
                },
            },
        };
        self.phase_acts += 1;
        it
    }

    pub fn boss(&self) -> bool {
        self.kind.info().boss
    }
}

// ---------------------------------------------------------------------------------------------
// Combat
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Who {
    Player,
    Foe(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Fighting,
    Won,
    Lost,
}

/// A card instance in a combat pile; `uid` is stable for the whole fight (animations key on it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inst {
    pub card: Card,
    pub uid: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayErr {
    NotYourTurn,
    BadIndex,
    Unplayable,
    NeedMomentum,
    NeedTarget,
}

/// What happened, in order, for the presentation to animate. Numbers are the state *after* the event.
#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    TurnStart { turn: u32 },
    Draw { uid: u32 },
    Reshuffle,
    Play { uid: u32, target: Option<usize>, mv: Move, attack: bool },
    Discard { uid: u32 },
    Exhaust { uid: u32 },
    Hit { to: Who, from: Option<usize>, dealt: i32, blocked: i32, hp: i32, shell: i32 },
    GainShell { who: Who, amount: i32, shell: i32 },
    Heal { who: Who, amount: i32, hp: i32 },
    Status { who: Who, status: Status, now: i32 },
    Momentum { now: i32, chain: i32, link: bool },
    Intent { foe: usize },
    FoeActs { foe: usize, attack: bool },
    Died { foe: usize },
    Summoned { foe: usize },
    Junk { count: i32 },
    Potion { slot: usize },
    Won,
    Lost,
}

#[derive(Clone, Debug)]
pub struct Combat {
    pub rng: Rng,
    pub hero: Hero,
    pub hp: i32,
    pub max_hp: i32,
    pub shell: i32,
    pub st: Statuses,
    pub foes: Vec<Enemy>,
    pub draw: Vec<Inst>,
    pub hand: Vec<Inst>,
    pub discard: Vec<Inst>,
    pub exhaust: Vec<Inst>,
    pub momentum: i32,
    pub chain: i32,
    pub last_move: Option<Move>,
    pub turn: u32,
    pub relics: Vec<Relic>,
    pub potions: [Option<Potion>; POTION_SLOTS],
    pub outcome: Outcome,
    pub events: Vec<Ev>,
    pub cards_played: u32,
    pub damage_dealt: i32,
    pub best_chain: i32,
    next_uid: u32,
    badge_used: bool,
}

pub fn apply_damage_mods(base: i32, attacker: &Statuses, defender: &Statuses) -> i32 {
    let mut d = base + attacker.get(Status::Pumped);
    if attacker.get(Status::Dizzy) > 0 {
        d = (d as f32 * 0.75).floor() as i32;
    }
    if defender.get(Status::Shaken) > 0 {
        d = (d as f32 * 1.5).floor() as i32;
    }
    d.max(0)
}

impl Combat {
    pub fn new(seed: u64, hero: Hero, hp: i32, max_hp: i32, deck: &[Card], relics: &[Relic], potions: [Option<Potion>; POTION_SLOTS], foes: &[EnemyKind]) -> Self {
        let mut rng = Rng::new(seed);
        let mut next_uid = 0;
        let mut draw: Vec<Inst> = deck
            .iter()
            .map(|c| {
                next_uid += 1;
                Inst { card: *c, uid: next_uid }
            })
            .collect();
        shuffle(&mut draw, &mut rng);
        let foes: Vec<Enemy> = foes.iter().take(MAX_ENEMIES).map(|k| Enemy::new(*k, &mut rng)).collect();
        let mut c = Combat {
            rng,
            hero,
            hp,
            max_hp,
            shell: 0,
            st: Statuses::default(),
            foes,
            draw,
            hand: vec![],
            discard: vec![],
            exhaust: vec![],
            momentum: 0,
            chain: 0,
            last_move: None,
            turn: 0,
            relics: relics.to_vec(),
            potions,
            outcome: Outcome::Fighting,
            events: vec![],
            cards_played: 0,
            damage_dealt: 0,
            best_chain: 0,
            next_uid,
            badge_used: false,
        };
        if c.has(Relic::StarShard) {
            c.st.add(Status::Pumped, 1);
        }
        if c.has(Relic::ThornCharm) {
            c.st.add(Status::Spiky, 2);
        }
        c.momentum = c.stride();
        c.begin_turn();
        c
    }

    pub fn has(&self, r: Relic) -> bool {
        self.relics.contains(&r)
    }

    pub fn stride(&self) -> i32 {
        BASE_STRIDE + self.st.get(Status::Stride) + self.has(Relic::SpringBoots) as i32
    }

    pub fn take_events(&mut self) -> Vec<Ev> {
        std::mem::take(&mut self.events)
    }

    pub fn alive_foes(&self) -> impl Iterator<Item = usize> + '_ {
        self.foes.iter().enumerate().filter(|(_, f)| f.alive).map(|(i, _)| i)
    }

    pub fn living(&self) -> usize {
        self.foes.iter().filter(|f| f.alive).count()
    }

    pub fn over(&self) -> bool {
        self.outcome != Outcome::Fighting
    }

    fn begin_turn(&mut self) {
        self.turn += 1;
        self.shell = 0;
        self.chain = 0;
        self.last_move = None;
        if self.turn > 1 {
            self.momentum = self.stride().max(self.momentum - 1).min(MAX_MOMENTUM);
        }
        self.events.push(Ev::TurnStart { turn: self.turn });
        self.events.push(Ev::Momentum { now: self.momentum, chain: 0, link: false });
        if self.turn == 1 && self.has(Relic::BrickHelm) {
            self.gain_shell(Who::Player, 6, false);
        }
        let mut n = HAND_SIZE as i32 + self.st.get(Status::Quick);
        if self.turn == 1 && self.has(Relic::CloudSeat) {
            n += 2;
        }
        self.draw_cards(n);
    }

    pub fn draw_cards(&mut self, n: i32) {
        for _ in 0..n {
            if self.hand.len() >= MAX_HAND {
                break;
            }
            if self.draw.is_empty() {
                if self.discard.is_empty() {
                    break;
                }
                self.draw = std::mem::take(&mut self.discard);
                shuffle(&mut self.draw, &mut self.rng);
                self.events.push(Ev::Reshuffle);
            }
            let c = self.draw.pop().unwrap();
            self.events.push(Ev::Draw { uid: c.uid });
            self.hand.push(c);
        }
    }

    /// Cost of the card at `idx` right now (X cards report the whole meter).
    pub fn cost_of(&self, idx: usize) -> i32 {
        let c = self.hand[idx].card.cost();
        if c < 0 { self.momentum } else { c }
    }

    pub fn can_play(&self, idx: usize) -> Result<(), PlayErr> {
        if self.outcome != Outcome::Fighting {
            return Err(PlayErr::NotYourTurn);
        }
        let Some(inst) = self.hand.get(idx) else { return Err(PlayErr::BadIndex) };
        let d = inst.card.def();
        if d.unplayable {
            return Err(PlayErr::Unplayable);
        }
        let c = inst.card.cost();
        if c >= 0 && c > self.momentum {
            return Err(PlayErr::NeedMomentum);
        }
        Ok(())
    }

    /// Would playing `idx` right now extend the chain?
    pub fn links(&self, idx: usize) -> bool {
        let mv = self.hand[idx].card.def().mv;
        match (self.last_move, mv.next()) {
            (Some(last), _) => last.next() == Some(mv),
            _ => false,
        }
    }

    /// Chain damage bonus the card at `idx` would get if played now.
    pub fn chain_bonus(&self, idx: usize) -> i32 {
        let mv = self.hand[idx].card.def().mv;
        if mv.neutral() { 0 } else if self.links(idx) { (self.chain + 1) * 2 } else { 0 }
    }

    fn gain_shell(&mut self, who: Who, n: i32, with_tough: bool) {
        match who {
            Who::Player => {
                let amount = n + if with_tough { self.st.get(Status::Tough) } else { 0 };
                self.shell += amount;
                self.events.push(Ev::GainShell { who, amount, shell: self.shell });
            }
            Who::Foe(i) => {
                let f = &mut self.foes[i];
                let amount = n + f.st.get(Status::Tough);
                f.shell += amount;
                let shell = f.shell;
                self.events.push(Ev::GainShell { who, amount, shell });
            }
        }
    }

    fn status_of(&self, who: Who) -> Statuses {
        match who {
            Who::Player => self.st,
            Who::Foe(i) => self.foes[i].st,
        }
    }

    fn add_status(&mut self, who: Who, s: Status, n: i32) {
        match who {
            Who::Player => self.st.add(s, n),
            Who::Foe(i) => self.foes[i].st.add(s, n),
        }
        let now = self.status_of(who).get(s);
        self.events.push(Ev::Status { who, status: s, now });
    }

    /// Apply `amount` to `to` after shell. `direct` skips shell (scorch, recoil).
    fn lose(&mut self, to: Who, from: Option<usize>, amount: i32, direct: bool) -> i32 {
        let amount = amount.max(0);
        let (dealt, blocked, hp, shell) = match to {
            Who::Player => {
                let blocked = if direct { 0 } else { amount.min(self.shell) };
                self.shell -= blocked;
                let dealt = amount - blocked;
                self.hp = (self.hp - dealt).max(0);
                (dealt, blocked, self.hp, self.shell)
            }
            Who::Foe(i) => {
                let f = &mut self.foes[i];
                let blocked = if direct { 0 } else { amount.min(f.shell) };
                f.shell -= blocked;
                let dealt = amount - blocked;
                f.hp = (f.hp - dealt).max(0);
                (dealt, blocked, f.hp, f.shell)
            }
        };
        self.events.push(Ev::Hit { to, from, dealt, blocked, hp, shell });
        if let Who::Foe(_) = to {
            self.damage_dealt += dealt;
        }
        if hp <= 0 {
            match to {
                Who::Foe(i) => self.kill(i),
                Who::Player => {
                    if self.outcome == Outcome::Fighting {
                        self.outcome = Outcome::Lost;
                        self.events.push(Ev::Lost);
                    }
                }
            }
        }
        dealt + blocked
    }

    fn kill(&mut self, i: usize) {
        if !self.foes[i].alive {
            return;
        }
        self.foes[i].alive = false;
        self.foes[i].hp = 0;
        self.events.push(Ev::Died { foe: i });
        if self.has(Relic::BlastPin) {
            for j in 0..self.foes.len() {
                if j != i && self.foes[j].alive {
                    let f = &mut self.foes[j];
                    f.hp = (f.hp - 4).max(0);
                    let hp = f.hp;
                    self.events.push(Ev::Hit { to: Who::Foe(j), from: None, dealt: 4, blocked: 0, hp, shell: self.foes[j].shell });
                    if hp <= 0 {
                        self.kill(j);
                    }
                }
            }
        }
        if self.living() == 0 && self.outcome == Outcome::Fighting {
            self.outcome = Outcome::Won;
            self.events.push(Ev::Won);
        }
    }

    /// One attack hit from `from` (None = the player) onto `to`.
    fn strike(&mut self, from: Option<usize>, to: Who, base: i32) {
        if self.over() {
            return;
        }
        let (atk, def) = match (from, to) {
            (None, Who::Foe(i)) => (self.st, self.foes[i].st),
            (Some(f), Who::Player) => (self.foes[f].st, self.st),
            _ => return,
        };
        let amount = apply_damage_mods(base, &atk, &def);
        self.lose(to, from, amount, false);
        let spiky = def.get(Status::Spiky);
        if spiky > 0 {
            let back = match from {
                None => Who::Player,
                Some(f) => Who::Foe(f),
            };
            let alive = match back {
                Who::Foe(f) => self.foes[f].alive,
                Who::Player => true,
            };
            if alive && !self.over() {
                self.lose(back, None, spiky, true);
            }
        }
    }

    fn heal(&mut self, n: i32) {
        let before = self.hp;
        self.hp = (self.hp + n).min(self.max_hp);
        self.events.push(Ev::Heal { who: Who::Player, amount: self.hp - before, hp: self.hp });
    }

    /// Valid foe targets (alive).
    pub fn targets(&self) -> Vec<usize> {
        self.alive_foes().collect()
    }

    pub fn play(&mut self, idx: usize, target: Option<usize>) -> Result<(), PlayErr> {
        self.can_play(idx)?;
        let card = self.hand[idx].card;
        let def = card.def();
        let target = if card.targeted() {
            match target {
                Some(t) if self.foes.get(t).is_some_and(|f| f.alive) => Some(t),
                Some(_) => return Err(PlayErr::NeedTarget),
                None => {
                    let alive: Vec<usize> = self.alive_foes().collect();
                    if alive.len() == 1 { Some(alive[0]) } else { return Err(PlayErr::NeedTarget) }
                }
            }
        } else {
            None
        };
        let inst = self.hand.remove(idx);
        // Pay.
        let x = if card.cost() < 0 {
            let x = self.momentum;
            self.momentum = 0;
            x
        } else {
            self.momentum -= card.cost();
            0
        };
        // Chain.
        let mut link = false;
        let mut bonus = 0;
        if !def.mv.neutral() {
            if self.last_move.is_some_and(|l| l.next() == Some(def.mv)) {
                link = true;
                self.chain += 1;
                self.momentum = (self.momentum + 1).min(MAX_MOMENTUM);
                self.best_chain = self.best_chain.max(self.chain);
                if self.chain == 3 && self.has(Relic::ChainAnklet) {
                    self.momentum = (self.momentum + 2).min(MAX_MOMENTUM);
                }
                bonus = self.chain * 2;
            } else {
                self.chain = 0;
            }
            self.last_move = Some(def.mv);
        }
        let attack = def.kind == Kind::Attack;
        self.events.push(Ev::Play { uid: inst.uid, target, mv: def.mv, attack });
        self.events.push(Ev::Momentum { now: self.momentum, chain: self.chain, link });
        self.cards_played += 1;
        // Effects.
        for fx in card.fx() {
            if self.over() {
                break;
            }
            self.run_fx(*fx, target, bonus, x, attack);
        }
        // Where the card goes.
        if def.kind == Kind::Power {
            self.exhaust.push(inst);
            self.events.push(Ev::Exhaust { uid: inst.uid });
        } else if def.exhaust {
            self.exhaust.push(inst);
            self.events.push(Ev::Exhaust { uid: inst.uid });
        } else {
            self.discard.push(inst);
            self.events.push(Ev::Discard { uid: inst.uid });
        }
        Ok(())
    }

    fn run_fx(&mut self, fx: Fx, target: Option<usize>, bonus: i32, x: i32, attack: bool) {
        let live_target = |s: &Self| target.filter(|t| s.foes[*t].alive);
        match fx {
            Dmg(n) => {
                if let Some(t) = live_target(self) {
                    self.player_hit(t, n + bonus, attack);
                }
            }
            DmgAll(n) => {
                for t in self.targets() {
                    if self.foes[t].alive {
                        self.player_hit(t, n + bonus, attack);
                    }
                }
            }
            Multi(h, n) => {
                for _ in 0..h {
                    if let Some(t) = live_target(self) {
                        self.player_hit(t, n + bonus / 2, attack);
                    }
                }
            }
            XHits(n) => {
                for _ in 0..x {
                    if let Some(t) = live_target(self) {
                        self.player_hit(t, n + bonus / 2, attack);
                    }
                }
            }
            ChainDmg(b, p) => {
                if let Some(t) = live_target(self) {
                    self.player_hit(t, b + p * self.chain, attack);
                }
            }
            ShellDmg => {
                if let Some(t) = live_target(self) {
                    self.player_hit(t, self.shell + bonus, attack);
                }
            }
            Shell(n) => self.gain_shell(Who::Player, n, true),
            Draw(n) => self.draw_cards(n),
            Mom(n) => {
                self.momentum = (self.momentum + n).min(MAX_MOMENTUM);
                self.events.push(Ev::Momentum { now: self.momentum, chain: self.chain, link: false });
            }
            Heal(n) => self.heal(n),
            Hurt(n) => {
                self.lose(Who::Player, None, n, true);
            }
            Apply(s, n) => {
                if let Some(t) = live_target(self) {
                    self.add_status(Who::Foe(t), s, n);
                }
            }
            ApplyAll(s, n) => {
                for t in self.targets() {
                    self.add_status(Who::Foe(t), s, n);
                }
            }
            Own(s, n) => self.add_status(Who::Player, s, n),
        }
    }

    fn player_hit(&mut self, t: usize, base: i32, attack: bool) {
        if attack && !self.badge_used && self.has(Relic::EmberBadge) {
            self.badge_used = true;
            self.add_status(Who::Foe(t), Status::Scorch, 3);
        }
        self.strike(None, Who::Foe(t), base);
    }

    pub fn use_potion(&mut self, slot: usize, target: Option<usize>) -> Result<(), PlayErr> {
        if self.outcome != Outcome::Fighting {
            return Err(PlayErr::NotYourTurn);
        }
        let Some(p) = self.potions.get(slot).copied().flatten() else { return Err(PlayErr::BadIndex) };
        let t = if p.targeted() {
            match target {
                Some(t) if self.foes.get(t).is_some_and(|f| f.alive) => Some(t),
                _ => {
                    let alive: Vec<usize> = self.alive_foes().collect();
                    if alive.len() == 1 { Some(alive[0]) } else { return Err(PlayErr::NeedTarget) }
                }
            }
        } else {
            None
        };
        self.potions[slot] = None;
        self.events.push(Ev::Potion { slot });
        match p {
            Potion::Tonic => self.heal(15),
            Potion::FireFlask => {
                if let Some(t) = t {
                    self.lose(Who::Foe(t), None, 20, false);
                }
            }
            Potion::ShellBrew => self.gain_shell(Who::Player, 14, false),
            Potion::SpeedBrew => {
                self.momentum = (self.momentum + 3).min(MAX_MOMENTUM);
                self.events.push(Ev::Momentum { now: self.momentum, chain: self.chain, link: false });
                self.draw_cards(1);
            }
            Potion::StarBrew => self.add_status(Who::Player, Status::Pumped, 3),
        }
        Ok(())
    }

    /// End the player's turn and run the whole enemy turn; leaves the next player turn dealt.
    pub fn end_turn(&mut self) {
        if self.over() {
            return;
        }
        // Discard the hand (Retain cards stay).
        let hand = std::mem::take(&mut self.hand);
        for c in hand {
            if c.card.def().retain {
                self.hand.push(c);
            } else {
                self.discard.push(c);
                self.events.push(Ev::Discard { uid: c.uid });
            }
        }
        // Player turn-end statuses.
        let scorch = self.st.get(Status::Scorch);
        if scorch > 0 {
            self.lose(Who::Player, None, scorch, true);
            self.st.set(Status::Scorch, scorch - 1);
            self.events.push(Ev::Status { who: Who::Player, status: Status::Scorch, now: scorch - 1 });
        }
        let regen = self.st.get(Status::Regen);
        if regen > 0 && !self.over() {
            self.heal(regen);
            self.st.set(Status::Regen, regen - 1);
            self.events.push(Ev::Status { who: Who::Player, status: Status::Regen, now: regen - 1 });
        }
        for s in [Status::Dizzy, Status::Shaken] {
            let v = self.st.get(s);
            if v > 0 {
                self.st.set(s, v - 1);
                self.events.push(Ev::Status { who: Who::Player, status: s, now: v - 1 });
            }
        }
        if self.over() {
            return;
        }
        // Enemy turn.
        let n = self.foes.len();
        for i in 0..n {
            if self.over() {
                return;
            }
            if !self.foes[i].alive {
                continue;
            }
            self.foes[i].shell = 0;
            let sc = self.foes[i].st.get(Status::Scorch);
            if sc > 0 {
                self.lose(Who::Foe(i), None, sc, true);
                self.foes[i].st.set(Status::Scorch, sc - 1);
                self.events.push(Ev::Status { who: Who::Foe(i), status: Status::Scorch, now: sc - 1 });
                if !self.foes[i].alive {
                    continue;
                }
            }
            self.foe_acts(i);
            if self.over() {
                return;
            }
            for s in [Status::Dizzy, Status::Shaken] {
                let v = self.foes[i].st.get(s);
                if v > 0 {
                    self.foes[i].st.set(s, v - 1);
                    self.events.push(Ev::Status { who: Who::Foe(i), status: s, now: v - 1 });
                }
            }
        }
        // New intents.
        for i in 0..self.foes.len() {
            if self.foes[i].alive {
                let mut f = self.foes[i].clone();
                let it = f.choose(&mut self.rng);
                f.intent = it;
                f.acts += 1;
                self.foes[i] = f;
                self.events.push(Ev::Intent { foe: i });
            }
        }
        self.begin_turn();
    }

    fn foe_acts(&mut self, i: usize) {
        let it = self.foes[i].intent;
        self.events.push(Ev::FoeActs { foe: i, attack: it.attacks() });
        if it.shell > 0 {
            self.gain_shell(Who::Foe(i), it.shell, true);
        }
        if let Some((s, n)) = it.buff {
            self.add_status(Who::Foe(i), s, n);
        }
        if it.attacks() {
            for _ in 0..it.hits {
                if self.over() || !self.foes[i].alive {
                    break;
                }
                self.strike(Some(i), Who::Player, it.dmg);
            }
        }
        if let Some((s, n)) = it.debuff {
            if !self.over() {
                self.add_status(Who::Player, s, n);
            }
        }
        if it.junk > 0 && !self.over() {
            for _ in 0..it.junk {
                self.next_uid += 1;
                self.discard.push(Inst { card: Card::new(card_id("bruise")), uid: self.next_uid });
            }
            self.events.push(Ev::Junk { count: it.junk });
        }
        if let Some(k) = it.summon {
            if self.foes.iter().filter(|f| f.alive).count() < MAX_ENEMIES && self.foes.len() < MAX_ENEMIES + 2 {
                let e = Enemy::new(k, &mut self.rng);
                self.foes.push(e);
                let idx = self.foes.len() - 1;
                self.events.push(Ev::Summoned { foe: idx });
            }
        }
        if it.dies && self.foes[i].alive {
            self.foes[i].hp = 0;
            self.kill(i);
        }
    }

    /// Damage per hit and hit count the foe's intent would deal to you right now (before your Shell).
    pub fn intent_damage(&self, i: usize) -> Option<(i32, i32)> {
        let f = &self.foes[i];
        if !f.alive || !f.intent.attacks() {
            return None;
        }
        Some((apply_damage_mods(f.intent.dmg, &f.st, &self.st), f.intent.hits))
    }

    /// Total incoming damage this coming enemy turn, before Shell.
    pub fn incoming(&self) -> i32 {
        self.alive_foes().filter_map(|i| self.intent_damage(i)).map(|(d, h)| d * h).sum()
    }

    pub fn pile_counts(&self) -> (usize, usize, usize, usize) {
        (self.draw.len(), self.hand.len(), self.discard.len(), self.exhaust.len())
    }
}

pub fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        v.swap(i, j);
    }
}

// ---------------------------------------------------------------------------------------------
// A simple bot (tests, `JUMPMAN_DECK_AUTOPLAY`)
// ---------------------------------------------------------------------------------------------

impl Combat {
    /// Play one sensible card if any; returns false when nothing is worth playing.
    pub fn bot_step(&mut self) -> bool {
        let mut best: Option<(i32, usize, Option<usize>)> = None;
        let weakest = self.alive_foes().min_by_key(|i| self.foes[*i].hp + self.foes[*i].shell);
        let danger = self.incoming() - self.shell;
        for idx in 0..self.hand.len() {
            if self.can_play(idx).is_err() {
                continue;
            }
            let c = self.hand[idx].card;
            let mut score = 10 + self.chain_bonus(idx) * 3 + if self.links(idx) { 6 } else { 0 };
            score -= self.cost_of(idx).min(4);
            match c.def().kind {
                Kind::Attack => score += 8,
                Kind::Power => score += 12,
                _ => {
                    // Skills: shell when under threat.
                    if c.fx().iter().any(|f| matches!(f, Shell(_))) && danger > 0 {
                        score += 10;
                    }
                }
            }
            if c.cost() < 0 && self.momentum < 2 {
                score -= 20;
            }
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, idx, weakest));
            }
        }
        let Some((_, idx, target)) = best else { return false };
        self.play(idx, target).is_ok()
    }

    /// Play a whole turn with the bot, then end it.
    pub fn bot_turn(&mut self) {
        let mut guard = 0;
        while !self.over() && guard < 60 && self.bot_step() {
            guard += 1;
        }
        // Drink a tonic when low.
        if !self.over() && self.hp * 3 < self.max_hp {
            if let Some(slot) = self.potions.iter().position(|p| *p == Some(Potion::Tonic)) {
                let _ = self.use_potion(slot, None);
            }
        }
        self.end_turn();
    }

    /// Fight to the end (or `max_turns`).
    pub fn bot_fight(&mut self, max_turns: u32) -> Outcome {
        while !self.over() && self.turn <= max_turns {
            self.bot_turn();
        }
        self.outcome
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fight(seed: u64, hero: Hero, foes: &[EnemyKind]) -> Combat {
        Combat::new(seed, hero, hero.max_hp(), hero.max_hp(), &hero.starter_deck(), &[], [None; POTION_SLOTS], foes)
    }

    fn put(c: &mut Combat, keys: &[&str]) {
        c.hand.clear();
        for k in keys {
            c.next_uid += 1;
            c.hand.push(Inst { card: Card::new(card_id(k)), uid: c.next_uid });
        }
    }

    #[test]
    fn card_table_is_sane() {
        let mut keys = std::collections::HashSet::new();
        for c in CARDS.iter() {
            assert!(keys.insert(c.key), "duplicate key {}", c.key);
            assert!(c.cost >= -1 && c.cost <= MAX_MOMENTUM);
            if c.kind != Kind::Curse {
                assert!(!c.fx.is_empty() && !c.up.is_empty(), "{} has no effects", c.key);
            }
        }
        assert!(CARDS.len() >= 45);
        assert!(CARDS.iter().filter(|c| c.kind == Kind::Power).count() >= 5);
        for h in Hero::ALL {
            let d = h.starter_deck();
            assert_eq!(d.len(), 10);
            assert!(d.iter().all(|c| c.def().rarity == Rarity::Starter));
        }
        // Heroes differ in their starters.
        let a = Hero::Mario.starter_deck();
        assert!(Hero::ALL.iter().skip(1).all(|h| h.starter_deck() != a));
    }

    #[test]
    fn upgrades_are_strictly_better() {
        for (i, c) in CARDS.iter().enumerate() {
            if c.kind == Kind::Curse {
                continue;
            }
            let (base, up) = (Card::new(i as u8), Card::upgraded(i as u8));
            assert!(base.text() != up.text() || up.cost() < base.cost(), "{} upgrade changes nothing", c.key);
            assert!(up.cost() <= base.cost() || base.cost() < 0);
            assert!(up.name().ends_with('+'));
        }
    }

    #[test]
    fn deck_draw_discard_and_reshuffle() {
        let mut c = fight(3, Hero::Mario, &[EnemyKind::Scuttler]);
        assert_eq!(c.hand.len(), HAND_SIZE);
        assert_eq!(c.draw.len(), 5);
        let total = |c: &Combat| c.draw.len() + c.hand.len() + c.discard.len() + c.exhaust.len();
        assert_eq!(total(&c), 10);
        c.end_turn();
        assert_eq!(total(&c), 10, "no cards lost or duplicated");
        // Second turn draws the other five; third turn reshuffles.
        c.end_turn();
        assert_eq!(c.hand.len(), HAND_SIZE);
        assert_eq!(total(&c), 10);
        assert!(c.events.iter().any(|e| *e == Ev::Reshuffle));
    }

    #[test]
    fn shuffle_is_seeded() {
        let a = fight(9, Hero::Link, &[EnemyKind::Scuttler]);
        let b = fight(9, Hero::Link, &[EnemyKind::Scuttler]);
        let d = fight(10, Hero::Link, &[EnemyKind::Scuttler]);
        let ids = |c: &Combat| c.hand.iter().map(|i| i.card.id).collect::<Vec<_>>();
        assert_eq!(ids(&a), ids(&b));
        assert!(ids(&a) != ids(&d) || a.draw.iter().map(|i| i.card.id).collect::<Vec<_>>() != d.draw.iter().map(|i| i.card.id).collect::<Vec<_>>());
    }

    #[test]
    fn damage_shell_and_statuses() {
        let none = Statuses::default();
        assert_eq!(apply_damage_mods(10, &none, &none), 10);
        let mut a = Statuses::default();
        a.add(Status::Pumped, 2);
        assert_eq!(apply_damage_mods(10, &a, &none), 12);
        a.add(Status::Dizzy, 1);
        assert_eq!(apply_damage_mods(10, &a, &none), 9, "12 * 0.75");
        let mut d = Statuses::default();
        d.add(Status::Shaken, 1);
        assert_eq!(apply_damage_mods(10, &none, &d), 15);
        // Shell soaks before health.
        let mut c = fight(1, Hero::Mario, &[EnemyKind::Rammer]);
        c.foes[0].shell = 4;
        let hp = c.foes[0].hp;
        c.strike(None, Who::Foe(0), 6);
        assert_eq!(c.foes[0].shell, 0);
        assert_eq!(c.foes[0].hp, hp - 2);
    }

    #[test]
    fn momentum_spend_and_chain() {
        let mut c = fight(5, Hero::Mario, &[EnemyKind::Slab]);
        assert_eq!(c.momentum, BASE_STRIDE);
        put(&mut c, &["hop", "dive", "pound", "hop"]);
        c.play(0, Some(0)).unwrap(); // Hop (1): 3 -> 2
        assert_eq!((c.momentum, c.chain), (2, 0));
        assert!(c.links(0), "Dive follows Jump");
        let hp0 = c.foes[0].hp + c.foes[0].shell;
        c.play(0, Some(0)).unwrap(); // Dive (1): 2 -> 1, link +1 -> 2, bonus +2
        assert_eq!((c.momentum, c.chain), (2, 1));
        assert_eq!(hp0 - (c.foes[0].hp + c.foes[0].shell), 6 + 2);
        c.play(0, Some(0)).unwrap(); // Pound (2): 2 -> 0, link -> 1, chain 2
        assert_eq!((c.momentum, c.chain), (1, 2));
        c.play(0, Some(0)).unwrap(); // Hop after Pound loops the chain
        assert_eq!(c.chain, 3);
        assert_eq!(c.best_chain, 3);
    }

    #[test]
    fn strike_breaks_and_brace_keeps_chain() {
        let mut c = fight(5, Hero::Mario, &[EnemyKind::Slab]);
        put(&mut c, &["hop", "crouch", "dive", "hop", "punch", "dive"]);
        c.play(0, Some(0)).unwrap();
        c.play(0, None).unwrap(); // Crouch is neutral
        assert_eq!(c.last_move, Some(Move::Jump));
        c.play(0, Some(0)).unwrap();
        assert_eq!(c.chain, 1);
        c.play(0, Some(0)).unwrap(); // Hop after Dive: not a link (Pound expected)
        assert_eq!(c.chain, 0);
        let m = c.momentum;
        c.play(0, Some(0)).unwrap(); // Punch (strike, 0 cost)
        assert_eq!((c.chain, c.last_move, c.momentum), (0, Some(Move::Strike), m));
    }

    #[test]
    fn insufficient_momentum_and_targeting_errors() {
        let mut c = fight(5, Hero::Mario, &[EnemyKind::Scuttler, EnemyKind::Scuttler]);
        put(&mut c, &["pound", "pound", "hop", "bruise"]);
        c.momentum = 1;
        assert_eq!(c.play(0, Some(0)), Err(PlayErr::NeedMomentum));
        assert_eq!(c.play(2, None), Err(PlayErr::NeedTarget), "two foes: must choose");
        assert_eq!(c.play(3, None), Err(PlayErr::Unplayable));
        assert_eq!(c.play(9, None), Err(PlayErr::BadIndex));
        assert_eq!(c.hand.len(), 4, "failed plays keep the card");
        // One foe left: auto-target.
        c.kill(1);
        c.play(2, None).unwrap();
    }

    #[test]
    fn x_cost_spends_everything() {
        let mut c = fight(5, Hero::Mario, &[EnemyKind::Slab]);
        c.hand.clear();
        c.hand.push(Inst { card: Card::new(card_id("star_spin")), uid: 999 });
        c.momentum = 3;
        let hp = c.foes[0].hp;
        c.play(0, Some(0)).unwrap();
        assert_eq!(c.momentum, 0);
        assert_eq!(c.foes[0].hp, hp - 12 + c.foes[0].shell.min(0) - 0, "3 hits of 4 (shell was 0)");
    }

    #[test]
    fn scorch_burns_and_fades() {
        let mut c = fight(2, Hero::Mario, &[EnemyKind::Slab]);
        c.foes[0].st.add(Status::Scorch, 3);
        let hp = c.foes[0].hp;
        c.end_turn();
        assert_eq!(c.foes[0].hp, hp - 3);
        assert_eq!(c.foes[0].st.get(Status::Scorch), 2);
    }

    #[test]
    fn spiky_reflects_on_attackers() {
        let mut c = fight(2, Hero::Mario, &[EnemyKind::Prickle]);
        c.foes[0].st.add(Status::Spiky, 3);
        let hp = c.hp;
        c.strike(None, Who::Foe(0), 5);
        assert_eq!(c.hp, hp - 3, "recoil ignores shell");
    }

    #[test]
    fn retain_exhaust_and_powers() {
        let mut c = fight(2, Hero::Mario, &[EnemyKind::Slab]);
        put(&mut c, &["vanish_cap", "pocket_change", "stride_training", "hop"]);
        c.momentum = 5;
        c.play(1, Some(0)).unwrap();
        assert_eq!(c.exhaust.len(), 1);
        c.play(1, None).unwrap(); // stride training
        assert_eq!(c.st.get(Status::Stride), 1);
        assert_eq!(c.exhaust.len(), 2, "powers leave the fight");
        c.end_turn();
        // Stride raised the floor: 3 + 1.
        assert!(c.momentum >= 4);
    }

    #[test]
    fn spring_boots_and_relic_starts() {
        let relics = [Relic::SpringBoots, Relic::BrickHelm, Relic::StarShard, Relic::ThornCharm, Relic::CloudSeat];
        let c = Combat::new(4, Hero::Banjo, 80, 80, &Hero::Banjo.starter_deck(), &relics, [None; 3], &[EnemyKind::Scuttler]);
        assert_eq!(c.momentum, 4);
        assert_eq!(c.shell, 6);
        assert_eq!(c.st.get(Status::Pumped), 1);
        assert_eq!(c.st.get(Status::Spiky), 2);
        assert_eq!(c.hand.len(), HAND_SIZE + 2);
    }

    #[test]
    fn blast_pin_and_ember_badge() {
        let relics = [Relic::BlastPin, Relic::EmberBadge];
        let mut c = Combat::new(4, Hero::Mario, 72, 72, &Hero::Mario.starter_deck(), &relics, [None; 3], &[EnemyKind::Swooper, EnemyKind::Rammer]);
        put(&mut c, &["hop", "hop"]);
        let rammer = c.foes[1].hp;
        c.foes[0].hp = 3;
        c.play(0, Some(0)).unwrap();
        assert!(!c.foes[0].alive);
        assert_eq!(c.foes[1].hp, rammer - 4);
        assert_eq!(c.foes[0].st.get(Status::Scorch), 3, "badge fired on the first attack");
    }

    #[test]
    fn potions_work_once() {
        let mut c = Combat::new(4, Hero::Mario, 30, 72, &Hero::Mario.starter_deck(), &[], [Some(Potion::Tonic), Some(Potion::SpeedBrew), None], &[EnemyKind::Scuttler]);
        c.use_potion(0, None).unwrap();
        assert_eq!(c.hp, 45);
        assert_eq!(c.use_potion(0, None), Err(PlayErr::BadIndex));
        let m = c.momentum;
        c.use_potion(1, None).unwrap();
        assert_eq!(c.momentum, m + 3);
    }

    #[test]
    fn enemy_intents_are_telegraphed_and_resolve() {
        let mut c = fight(11, Hero::Link, &[EnemyKind::Rammer]);
        let it = c.foes[0].intent;
        assert!(it.dmg > 0 || it.shell > 0 || it.buff.is_some());
        // Rammer opens with Shove 8.
        assert_eq!(c.intent_damage(0), Some((8, 1)));
        let hp = c.hp;
        c.end_turn();
        assert_eq!(c.hp, hp - 8);
        assert_ne!(c.foes[0].intent.name, "Shove", "next intent advances");
    }

    #[test]
    fn dizzy_foe_hits_softer_and_shaken_you_take_more() {
        let mut c = fight(11, Hero::Link, &[EnemyKind::Rammer]);
        c.foes[0].st.add(Status::Dizzy, 1);
        assert_eq!(c.intent_damage(0), Some((6, 1)));
        c.st.add(Status::Shaken, 1);
        assert_eq!(c.intent_damage(0), Some((9, 1)), "6 * 1.5");
    }

    #[test]
    fn fuse_pup_blast_kills_itself() {
        let mut c = fight(11, Hero::Link, &[EnemyKind::FusePup, EnemyKind::Slab]);
        c.hp = 200;
        c.max_hp = 200;
        for _ in 0..3 {
            c.hand.clear();
            c.end_turn();
        }
        assert!(!c.foes[0].alive, "blasted itself");
        assert!(c.hp < 200);
    }

    #[test]
    fn zapper_adds_bruises() {
        let mut c = fight(11, Hero::Link, &[EnemyKind::Zapper]);
        c.end_turn();
        let n = c.draw.iter().chain(&c.hand).chain(&c.discard).filter(|i| i.card.def().key == "bruise").count();
        assert_eq!(n, 1);
    }

    #[test]
    fn boss_phases_progress_with_health() {
        let mut rng = Rng::new(1);
        let mut boss = Enemy::new(EnemyKind::FuseKing, &mut rng);
        assert_eq!(boss.max_hp, 150);
        assert_eq!(boss.intent.name, "Royal Stomp");
        boss.hp = (boss.max_hp as f32 * 0.55) as i32;
        let it = boss.choose(&mut rng);
        assert_eq!((boss.phase, it.name), (1, "Bellow"));
        let it2 = boss.choose(&mut rng);
        assert_eq!(it2.name, "Bomb Barrage");
        boss.hp = (boss.max_hp as f32 * 0.2) as i32;
        assert_eq!(boss.choose(&mut rng).name, "Meltdown");
        let seen: Vec<&str> = (0..3).map(|_| boss.choose(&mut rng).name).collect();
        assert_eq!(seen, ["Rampage", "Winding Up!", "Boom Stomp"]);
    }

    #[test]
    fn boss_can_summon_but_not_overfill() {
        let mut c = fight(7, Hero::Banjo, &[EnemyKind::FuseKing]);
        c.hp = 999;
        c.max_hp = 999;
        c.end_turn();
        assert_eq!(c.intent_damage(0).is_none(), true);
        c.end_turn(); // Call the Guard
        assert_eq!(c.living(), 2);
        for _ in 0..8 {
            c.end_turn();
        }
        assert!(c.living() <= MAX_ENEMIES + 2);
    }

    #[test]
    fn every_enemy_has_a_valid_pattern() {
        for k in ENEMY_KINDS {
            let mut rng = Rng::new(3);
            let mut e = Enemy::new(k, &mut rng);
            for _ in 0..30 {
                let it = e.choose(&mut rng);
                assert!(it.attacks() || it.shell > 0 || it.buff.is_some() || it.debuff.is_some() || it.junk > 0 || it.summon.is_some() || it.winding, "{:?} idles pointlessly: {it:?}", k);
                assert!(!it.name.is_empty());
                e.acts += 1;
            }
            assert!(std::path::Path::new(&format!("{}/../assets/{}", env!("CARGO_MANIFEST_DIR"), k.info().model)).exists(), "{} model missing", k.name());
        }
        assert!(ENEMY_KINDS.iter().filter(|k| !k.info().elite && !k.info().boss).count() >= 10);
        assert_eq!(ENEMY_KINDS.iter().filter(|k| k.info().elite).count(), 2);
    }

    #[test]
    fn bot_beats_easy_fights_deterministically() {
        let mut wins = 0;
        for seed in 0..40 {
            let hero = Hero::ALL[seed as usize % 4];
            let mut c = fight(seed, hero, &[EnemyKind::Scuttler, EnemyKind::Swooper]);
            let out = c.bot_fight(40);
            assert_ne!(out, Outcome::Fighting, "fight must finish (seed {seed})");
            wins += (out == Outcome::Won) as i32;
            // Determinism.
            let mut d = fight(seed, hero, &[EnemyKind::Scuttler, EnemyKind::Swooper]);
            d.bot_fight(40);
            assert_eq!((c.hp, c.turn, c.cards_played), (d.hp, d.turn, d.cards_played));
        }
        assert!(wins >= 34, "starter decks should clear a starter fight ({wins}/40)");
    }

    #[test]
    fn starter_deck_can_lose_to_the_boss() {
        let mut wins = 0;
        for seed in 0..20 {
            let mut c = fight(seed, Hero::Mario, &[EnemyKind::FuseKing]);
            wins += (c.bot_fight(60) == Outcome::Won) as i32;
        }
        assert!(wins < 6, "an unimproved starter deck should not routinely beat the boss ({wins}/20)");
    }

    #[test]
    fn every_card_is_playable_and_loops_terminate() {
        for (i, def) in CARDS.iter().enumerate() {
            if def.unplayable {
                continue;
            }
            for up in [false, true] {
                let card = Card { id: i as u8, up };
                // A deck of ten copies, with plenty of momentum: each copy must be playable at least once.
                let deck = vec![card; 10];
                let mut c = Combat::new(i as u64, Hero::Mario, 500, 500, &deck, &[], [None; 3], &[EnemyKind::Slab, EnemyKind::Rammer]);
                c.momentum = MAX_MOMENTUM;
                assert!(c.can_play(0).is_ok(), "{} must be playable with full momentum", card.name());
                let mut plays = 0;
                // Bot-play 30 turns; card counts must stay conserved and the loop bounded.
                for _ in 0..30 {
                    if c.over() {
                        break;
                    }
                    let mut guard = 0;
                    while c.bot_step() {
                        plays += 1;
                        guard += 1;
                        assert!(guard < 200, "{} loops forever", card.name());
                    }
                    c.end_turn();
                }
                assert!(plays > 0, "{} was never played", card.name());
                assert!(c.draw.len() + c.hand.len() + c.discard.len() + c.exhaust.len() >= 10, "{} lost cards", card.name());
            }
        }
    }

    #[test]
    fn card_text_mentions_numbers_and_keywords() {
        let t = Card::new(card_id("pocket_change")).text();
        assert!(t.contains("Deal 4") && t.contains("Exhaust"));
        assert!(Card::new(card_id("star_spin")).text().contains("X"));
        assert!(Card::upgraded(card_id("hop")).text().contains('8'));
    }
}
