//! Sunstone Shindig rules: an engine-free, seeded board-game core.
//!
//! Four players (seats 0..3, seat 0 is usually the human) race around a board for `turns` rounds. Each turn: optionally use an
//! item, roll (1-10), walk the path (choosing at forks), and resolve the space you land on. After all four have moved a MINI-GAME
//! plays: its format depends on which spaces everyone ended on (sun/moon tint): free-for-all, 2v2 or 1v3. A duel space starts a
//! one-on-one at once. The Sunstone is bought from the Keeper for 20 coins and the Keeper then moves. At the end three bonus awards
//! are given, each worth a Sunstone; most Sunstones wins, coins break ties.
//!
//! The core never touches the engine: the view reads [`Party::events`] and [`Party::phase`] and sends [`Cmd`]s.
use super::board::*;
use super::common::*;
use super::minis::*;

mod bots;
mod rules;
#[cfg(test)]
mod tests;

pub const SUNSTONE_COST: i32 = 20;
pub const START_COINS: i32 = 10;
pub const MAX_ITEMS: usize = 2;
pub const PAYDAY: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    /// Roll 5-10 this turn.
    Dash,
    /// Roll two six-sided dice (2-12).
    Twin,
    /// Warp straight to the Keeper.
    Whistle,
    /// Steal up to 6 coins from a rival.
    Pickpocket,
}

pub const ITEMS: [Item; 4] = [Item::Dash, Item::Twin, Item::Whistle, Item::Pickpocket];

impl Item {
    pub fn label(self) -> &'static str {
        match self {
            Item::Dash => "Dash Die",
            Item::Twin => "Twin Dice",
            Item::Whistle => "Keeper Whistle",
            Item::Pickpocket => "Pickpocket",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Item::Dash => "Roll 5 to 10 this turn.",
            Item::Twin => "Roll two dice: 2 to 12.",
            Item::Whistle => "Warp straight to the Keeper.",
            Item::Pickpocket => "Steal up to 6 coins from a rival.",
        }
    }
    pub fn price(self) -> i32 {
        match self {
            Item::Dash => 6,
            Item::Twin => 5,
            Item::Whistle => 10,
            Item::Pickpocket => 7,
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Item::Dash => "D",
            Item::Twin => "2D",
            Item::Whistle => "W",
            Item::Pickpocket => "P",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Die {
    Normal,
    Dash,
    Twin,
}

impl Die {
    pub fn range(self) -> (u8, u8) {
        match self {
            Die::Normal => (1, 10),
            Die::Dash => (5, 10),
            Die::Twin => (2, 12),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    pub name: &'static str,
    /// `None` = a person; `Some(level)` = a CPU.
    pub cpu: Option<Level>,
    pub coins: i32,
    pub sun: u32,
    pub space: usize,
    pub items: Vec<Item>,
    pub wins: u32,
    pub slow: bool,
    pub lucky: i32,
    pub tint: bool,
    pub lands: [u16; 10],
    pub earned: i32,
    pub bonus: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ffa,
    TwoVsTwo,
    OneVsThree,
    Duel,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Ffa => "Free-for-all",
            Format::TwoVsTwo => "2 vs 2",
            Format::OneVsThree => "1 vs 3",
            Format::Duel => "Duel",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MiniCtx {
    pub kind: Mini,
    pub format: Format,
    pub active: [bool; 4],
    /// Team id per seat (0 / 1); free-for-all uses the seat number.
    pub teams: [u8; 4],
    pub round_end: bool,
    pub seed: u64,
}

#[derive(Clone, Debug)]
pub struct MiniReport {
    pub kind: Mini,
    pub format: Format,
    pub active: [bool; 4],
    pub teams: [u8; 4],
    pub scores: [f32; 4],
    pub ranks: [u8; 4],
    pub won: [bool; 4],
    pub coins: [i32; 4],
    pub round_end: bool,
}

#[derive(Clone, Debug)]
pub enum Phase {
    /// The current player may use an item and must roll.
    TurnStart,
    /// Choose which space to walk onto next.
    Branch(Vec<usize>),
    /// Buy the Sunstone?
    KeeperOffer,
    ShopMenu,
    DuelPick,
    /// A mini-game must be played; report scores with [`Cmd::MiniDone`].
    Mini(MiniCtx),
    /// Shows the mini-game result; [`Cmd::Ack`] continues.
    MiniResult(MiniReport),
    Over,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Use item in slot (with a target rival for the Pickpocket).
    Item(usize, Option<usize>),
    /// Roll; `Some(v)` is the value a timing-stopped die showed (clamped to the die), `None` draws one.
    Roll(Option<u8>),
    /// Walk onto this space (a fork).
    Choose(usize),
    Keeper(bool),
    ShopBuy(Item),
    ShopLeave,
    DuelPick(usize),
    MiniDone([f32; 4]),
    Ack,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    RoundStart(u32),
    TurnStart(usize),
    Rolled { p: usize, value: u8, die: Die },
    Step { p: usize, from: usize, to: usize },
    Landed { p: usize, space: usize },
    Coins { p: usize, delta: i32 },
    Payday { p: usize },
    Sunstone { p: usize },
    KeeperMoved { to: usize },
    Warped { p: usize, from: usize, to: usize },
    Gift { p: usize, item: Item },
    ItemUsed { p: usize, item: Item },
    Stolen { from: usize, to: usize, amount: i32 },
    Swapped { a: usize, b: usize },
    Text(String),
    Flip(bool),
    Wind(bool),
    Tide(u8),
    MiniStart,
    MiniEnd,
    Bonus { name: &'static str, winners: Vec<usize> },
    GameOver,
}

#[derive(Clone, Debug)]
pub struct Award {
    pub name: &'static str,
    pub desc: &'static str,
    pub winners: Vec<usize>,
    pub value: i32,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub board: usize,
    pub turns: u32,
    pub seed: u64,
    /// Per seat: `None` = human, `Some(level)` = CPU.
    pub players: [Option<Level>; 4],
}

impl Default for Config {
    fn default() -> Self {
        Config { board: 0, turns: 12, seed: 1, players: [None, Some(Level::Normal), Some(Level::Normal), Some(Level::Normal)] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    CoinRain,
    GhostThief,
    GiftWrap,
    Gust,
    SwapSpots,
    RobinHood,
    TaxToad,
    LuckyCharm,
    PartyFavor,
    Rumor,
}

const EVENTS: [EventKind; 10] = [EventKind::CoinRain, EventKind::GhostThief, EventKind::GiftWrap, EventKind::Gust, EventKind::SwapSpots, EventKind::RobinHood, EventKind::TaxToad, EventKind::LuckyCharm, EventKind::PartyFavor, EventKind::Rumor];

#[derive(Clone)]
pub struct Party {
    pub cfg: Config,
    pub board: Board,
    pub cond: Conditions,
    pub players: [Player; 4],
    /// 1-based round.
    pub round: u32,
    /// Seat whose turn it is.
    pub cur: usize,
    pub phase: Phase,
    pub moves_left: u8,
    pub keeper: usize,
    pub die: Die,
    pub used_item: bool,
    pub events: Vec<Ev>,
    pub last_mini: Option<Mini>,
    pub awards: Vec<Award>,
    pub bonus_pick: usize,
    pub minis_played: u32,
    /// The current player has rolled this turn.
    pub rolled: bool,
    pub(super) rng: Rng,
    pub(super) brng: Rng,
    /// Each mini-game's arena (index [`Mini::idx`]): flat stand-ins until the app hands in the real spots.
    pub fields: Vec<std::sync::Arc<Field>>,
}

impl Party {
    pub fn new(cfg: Config) -> Self {
        let board = board(cfg.board);
        let mut rng = Rng::new(cfg.seed);
        let names = PLAYER_NAMES;
        let players = std::array::from_fn(|i| Player { name: names[i], cpu: cfg.players[i], coins: START_COINS, sun: 0, space: board.start, items: vec![], wins: 0, slow: false, lucky: 0, tint: true, lands: [0; 10], earned: 0, bonus: 0 });
        let keeper = board.sites[rng.below(board.sites.len())];
        let bonus_pick = rng.below(5);
        let mut p = Party {
            board,
            cond: Conditions::default(),
            players,
            round: 1,
            cur: 0,
            phase: Phase::TurnStart,
            moves_left: 0,
            keeper,
            die: Die::Normal,
            used_item: false,
            events: vec![Ev::RoundStart(1), Ev::TurnStart(0)],
            last_mini: None,
            awards: vec![],
            bonus_pick,
            minis_played: 0,
            rolled: false,
            brng: Rng::new(cfg.seed ^ 0xb07),
            rng,
            cfg,
            fields: stand_in_fields(),
        };
        p.cond = Conditions { flip: false, wind: p.board.gimmick == Gimmick::Wind && p.rng.chance(0.5), tide: 0 };
        p
    }

    pub fn drain_events(&mut self) -> Vec<Ev> {
        std::mem::take(&mut self.events)
    }

    pub fn is_cpu(&self, p: usize) -> bool {
        self.players[p].cpu.is_some()
    }

    pub fn over(&self) -> bool {
        matches!(self.phase, Phase::Over)
    }

    /// Whether the player whose input the phase waits on is a person.
    pub fn waiting_for_human(&self) -> bool {
        match &self.phase {
            Phase::Over => false,
            Phase::Mini(_) | Phase::MiniResult(_) => true,
            _ => !self.is_cpu(self.cur),
        }
    }

    pub fn space_kind(&self, s: usize) -> Kind {
        if self.board.flooded(s, &self.cond) {
            return Kind::Pit;
        }
        self.board.spaces[s].kind
    }

    pub fn keeper_open(&self, s: usize) -> bool {
        s == self.keeper
    }

    pub(super) fn coins(&mut self, p: usize, delta: i32) -> i32 {
        let before = self.players[p].coins;
        let after = (before + delta).max(0);
        self.players[p].coins = after;
        if after != before {
            self.events.push(Ev::Coins { p, delta: after - before });
        }
        if after > before {
            self.players[p].earned += after - before;
        }
        after - before
    }

    pub(super) fn text(&mut self, s: impl Into<String>) {
        self.events.push(Ev::Text(s.into()));
    }

    // ----------------------------------------------------------------------------------------
    // Commands
    // ----------------------------------------------------------------------------------------

    pub fn apply(&mut self, cmd: Cmd) -> Result<(), &'static str> {
        let p = self.cur;
        match (self.phase.clone(), cmd) {
            (Phase::TurnStart, Cmd::Item(slot, target)) => self.use_item(slot, target),
            (Phase::TurnStart, Cmd::Roll(v)) => {
                self.roll(v);
                Ok(())
            }
            (Phase::Branch(opts), Cmd::Choose(s)) => {
                if !opts.contains(&s) {
                    return Err("not a legal path");
                }
                self.step_to(s);
                self.advance();
                Ok(())
            }
            (Phase::KeeperOffer, Cmd::Keeper(buy)) => {
                if buy {
                    if self.players[p].coins < SUNSTONE_COST {
                        return Err("not enough coins");
                    }
                    self.buy_sunstone(p);
                }
                self.after_offer();
                Ok(())
            }
            (Phase::ShopMenu, Cmd::ShopBuy(item)) => {
                let pl = &self.players[p];
                if pl.items.len() >= MAX_ITEMS {
                    return Err("hands full");
                }
                if pl.coins < item.price() {
                    return Err("not enough coins");
                }
                self.coins(p, -item.price());
                self.players[p].items.push(item);
                self.events.push(Ev::Gift { p, item });
                Ok(())
            }
            (Phase::ShopMenu, Cmd::ShopLeave) => {
                self.finish_turn();
                Ok(())
            }
            (Phase::DuelPick, Cmd::DuelPick(q)) => {
                if q == p || q > 3 {
                    return Err("pick a rival");
                }
                let mut active = [false; 4];
                active[p] = true;
                active[q] = true;
                let teams = [0, 1, 2, 3];
                self.start_mini(Format::Duel, active, teams, false);
                Ok(())
            }
            (Phase::Mini(ctx), Cmd::MiniDone(scores)) => {
                self.finish_mini(ctx, scores);
                Ok(())
            }
            (Phase::MiniResult(rep), Cmd::Ack) => {
                self.events.push(Ev::MiniEnd);
                if rep.round_end {
                    self.end_round();
                } else {
                    self.finish_turn();
                }
                Ok(())
            }
            _ => Err("command not valid in this phase"),
        }
    }

    fn use_item(&mut self, slot: usize, target: Option<usize>) -> Result<(), &'static str> {
        let p = self.cur;
        if self.used_item {
            return Err("one item per turn");
        }
        let Some(&item) = self.players[p].items.get(slot) else { return Err("no item there") };
        match item {
            Item::Dash => self.die = Die::Dash,
            Item::Twin => self.die = Die::Twin,
            Item::Whistle => {
                let from = self.players[p].space;
                self.players[p].space = self.keeper;
                self.events.push(Ev::Warped { p, from, to: self.keeper });
            }
            Item::Pickpocket => {
                let Some(q) = target.filter(|&q| q != p && q < 4 && self.players[q].coins > 0) else { return Err("pick a rival with coins") };
                let amt = self.players[q].coins.min(6);
                self.coins(q, -amt);
                self.coins(p, amt);
                self.events.push(Ev::Stolen { from: q, to: p, amount: amt });
            }
        }
        self.players[p].items.remove(slot);
        self.used_item = true;
        self.events.push(Ev::ItemUsed { p, item });
        if item == Item::Whistle && self.players[p].coins >= SUNSTONE_COST {
            self.moves_left = 0;
            self.phase = Phase::KeeperOffer;
        }
        Ok(())
    }

    fn roll(&mut self, v: Option<u8>) {
        let p = self.cur;
        let (lo, hi) = self.die.range();
        let mut val = match v {
            Some(x) => x.clamp(lo, hi),
            None => match self.die {
                Die::Normal => self.rng.below(10) as u8 + 1,
                Die::Dash => 5 + self.rng.below(6) as u8,
                Die::Twin => self.rng.below(6) as u8 + self.rng.below(6) as u8 + 2,
            },
        };
        let die = self.die;
        self.rolled = true;
        if self.players[p].slow {
            self.players[p].slow = false;
            if val > 3 {
                val = 3;
                self.text("Slow Boots: the roll is capped at 3.");
            }
        }
        if self.players[p].lucky > 0 {
            val = (val as i32 + self.players[p].lucky).min(15) as u8;
            self.players[p].lucky = 0;
        }
        self.events.push(Ev::Rolled { p, value: val, die });
        if die == Die::Twin && val % 2 == 0 && val > 2 {
            // Matching halves are not tracked; every even Twin total pays a small bonus.
            self.coins(p, 1);
        }
        self.moves_left = val;
        self.advance();
    }

    fn step_to(&mut self, to: usize) {
        let p = self.cur;
        let from = self.players[p].space;
        self.players[p].space = to;
        self.events.push(Ev::Step { p, from, to });
        self.moves_left = self.moves_left.saturating_sub(1);
        if to == self.board.start && self.moves_left > 0 {
            self.coins(p, PAYDAY);
            self.events.push(Ev::Payday { p });
        }
    }

    /// Walk until the dice run out or a decision is needed.
    fn advance(&mut self) {
        let p = self.cur;
        loop {
            if self.moves_left == 0 {
                self.land();
                return;
            }
            let here = self.players[p].space;
            let opts = self.board.out(here, &self.cond);
            if opts.len() > 1 {
                self.phase = Phase::Branch(opts);
                return;
            }
            let next = opts[0];
            self.step_to(next);
            if next == self.keeper && self.moves_left > 0 && self.players[p].coins >= SUNSTONE_COST {
                self.phase = Phase::KeeperOffer;
                return;
            }
        }
    }

}
