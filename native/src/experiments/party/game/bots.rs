//! CPU decisions on the board, and the headless driver used by tests, balance runs and `JUMPMAN_PARTY_AUTOPLAY`.
use super::*;

impl Party {
    fn space_value(&self, s: usize, p: usize) -> f32 {
        let coins = self.players[p].coins;
        if self.board.flooded(s, &self.cond) {
            return -4.0;
        }
        match self.board.spaces[s].kind {
            Kind::Keeper if s == self.keeper => {
                if coins >= SUNSTONE_COST {
                    14.0
                } else {
                    0.0
                }
            }
            Kind::Keeper => 2.0,
            Kind::Coin => 3.0,
            Kind::Item => 2.0,
            Kind::Shop => 1.0,
            Kind::Start => 3.0,
            Kind::Pit => -3.0,
            Kind::Hex => -3.0,
            Kind::Duel => -0.5,
            Kind::Event => 0.5,
            Kind::Warp => 0.0,
        }
    }

    fn look(&self, s: usize, depth: u8, p: usize) -> f32 {
        let mut v = self.space_value(s, p);
        if depth == 0 {
            return v;
        }
        let best = self.board.out(s, &self.cond).into_iter().map(|t| self.look(t, depth - 1, p)).fold(f32::NEG_INFINITY, f32::max);
        if best.is_finite() {
            v += best;
        }
        v
    }

    /// Steps to the Keeper ignoring bridge / wind rules (a guide, not a promise).
    pub fn keeper_distance(&self, s: usize) -> u32 {
        self.board.distance(s, self.keeper).unwrap_or(99)
    }

    /// What the CPU in the current seat would do now. `None` when it is a person's decision.
    pub fn bot_cmd(&mut self) -> Option<Cmd> {
        let p = self.cur;
        match self.phase.clone() {
            Phase::Over | Phase::Mini(_) => None,
            Phase::MiniResult(_) => Some(Cmd::Ack),
            phase => {
                let lvl = self.players[p].cpu?;
                Some(self.decide(p, lvl, phase))
            }
        }
    }

    fn decide(&mut self, p: usize, lvl: Level, phase: Phase) -> Cmd {
        match phase {
            Phase::TurnStart => {
                if !self.used_item && !self.rolled {
                    if let Some(c) = self.item_choice(p, lvl) {
                        return c;
                    }
                }
                Cmd::Roll(None)
            }
            Phase::Branch(opts) => {
                if self.brng.chance(match lvl {
                    Level::Easy => 0.5,
                    Level::Normal => 0.15,
                    Level::Hard => 0.0,
                }) {
                    return Cmd::Choose(opts[self.brng.below(opts.len())]);
                }
                let depth = self.moves_left.saturating_sub(1).min(7);
                let coins = self.players[p].coins;
                let want_keeper = coins >= SUNSTONE_COST - 2;
                let mut best = (f32::NEG_INFINITY, opts[0]);
                for &o in &opts {
                    let mut v = self.look(o, depth, p);
                    if want_keeper {
                        v -= 0.6 * self.keeper_distance(o) as f32;
                    }
                    if v > best.0 {
                        best = (v, o);
                    }
                }
                Cmd::Choose(best.1)
            }
            Phase::KeeperOffer => Cmd::Keeper(true),
            Phase::ShopMenu => {
                let coins = self.players[p].coins;
                if self.players[p].items.len() >= MAX_ITEMS {
                    return Cmd::ShopLeave;
                }
                // Keep enough for a Sunstone unless the purchase is cheap relative to the purse.
                let reserve = match lvl {
                    Level::Easy => 0,
                    Level::Normal => 4,
                    Level::Hard => 8,
                };
                let order = [Item::Twin, Item::Dash, Item::Pickpocket, Item::Whistle];
                for it in order {
                    let have = self.players[p].items.contains(&it);
                    if !have && coins - it.price() >= reserve && (it != Item::Whistle || coins - it.price() >= SUNSTONE_COST - 4) {
                        if it == Item::Pickpocket && (0..4).filter(|&q| q != p).all(|q| self.players[q].coins < 6) {
                            continue;
                        }
                        return Cmd::ShopBuy(it);
                    }
                }
                Cmd::ShopLeave
            }
            Phase::DuelPick => {
                let others: Vec<usize> = (0..4).filter(|&q| q != p).collect();
                let q = if lvl == Level::Easy {
                    others[self.brng.below(3)]
                } else {
                    // Rival with the most coins to take (but not the one who nearly has a Sunstone).
                    *others.iter().max_by_key(|&&q| (self.players[q].coins.min(5), self.players[q].coins)).unwrap()
                };
                Cmd::DuelPick(q)
            }
            Phase::MiniResult(_) => Cmd::Ack,
            Phase::Mini(_) | Phase::Over => Cmd::Ack,
        }
    }

    fn item_choice(&mut self, p: usize, lvl: Level) -> Option<Cmd> {
        let pl = &self.players[p];
        let dist = self.keeper_distance(pl.space);
        for (slot, it) in pl.items.clone().into_iter().enumerate() {
            let use_it = match it {
                Item::Dash | Item::Twin => match lvl {
                    Level::Easy => true,
                    // Save dice for when the Keeper is a long way off and we can afford the Sunstone soon.
                    _ => dist > 5 || self.players[p].coins < SUNSTONE_COST - 6,
                },
                Item::Whistle => self.players[p].coins >= SUNSTONE_COST && dist > 4,
                Item::Pickpocket => (0..4).any(|q| q != p && self.players[q].coins >= 6),
            };
            if use_it {
                let target = if it == Item::Pickpocket { (0..4).filter(|&q| q != p).max_by_key(|&q| self.players[q].coins) } else { None };
                return Some(Cmd::Item(slot, target));
            }
        }
        None
    }

    /// Drive a CPU-only game one command forward (mini-games are simulated with bots).
    /// Returns false when the game is over.
    pub fn auto_step(&mut self) -> bool {
        match self.phase.clone() {
            Phase::Over => false,
            Phase::Mini(ctx) => {
                let levels: [Level; 4] = std::array::from_fn(|p| self.players[p].cpu.unwrap_or(Level::Normal));
                let (scores, _) = run_bots(ctx.kind, ctx.seed, ctx.active, levels);
                self.apply(Cmd::MiniDone(scores)).expect("mini done");
                true
            }
            _ => {
                let cmd = self.bot_cmd().unwrap_or_else(|| {
                    // A person's decision in a headless run: play it like a Normal CPU.
                    let p = self.cur;
                    let phase = self.phase.clone();
                    self.decide(p, Level::Normal, phase)
                });
                if let Err(e) = self.apply(cmd.clone()) {
                    panic!("bot sent {cmd:?} in {:?}: {e}", self.phase);
                }
                true
            }
        }
    }

    /// Play to the end with bots everywhere.
    pub fn autoplay(&mut self) {
        let mut guard = 0;
        while self.auto_step() {
            guard += 1;
            assert!(guard < 200_000, "autoplay did not finish");
        }
    }
}
