//! Space effects, events, mini-game flow, round changes and the end-of-game awards.
use super::*;

impl Party {
    pub(super) fn after_offer(&mut self) {
        if self.moves_left > 0 {
            self.phase = Phase::TurnStart;
            self.advance();
        } else if !self.rolled {
            // The Whistle warped us here before the roll: the turn goes on.
            self.phase = Phase::TurnStart;
        } else {
            self.finish_turn();
        }
    }

    pub(super) fn buy_sunstone(&mut self, p: usize) {
        self.coins(p, -SUNSTONE_COST);
        self.players[p].sun += 1;
        self.events.push(Ev::Sunstone { p });
        self.move_keeper();
    }

    pub(super) fn move_keeper(&mut self) {
        let options: Vec<usize> = self.board.sites.iter().copied().filter(|&s| s != self.keeper && !self.board.flooded(s, &self.cond)).collect();
        if options.is_empty() {
            return;
        }
        self.keeper = options[self.rng.below(options.len())];
        self.events.push(Ev::KeeperMoved { to: self.keeper });
    }

    /// Resolve the space the current player stopped on.
    pub(super) fn land(&mut self) {
        let p = self.cur;
        let s = self.players[p].space;
        self.events.push(Ev::Landed { p, space: s });
        let flooded = self.board.flooded(s, &self.cond);
        let kind = if flooded { Kind::Pit } else { self.board.spaces[s].kind };
        self.players[p].tint = self.board.spaces[s].sun && !flooded;
        self.players[p].lands[kind.idx()] += 1;
        match kind {
            Kind::Start => {
                self.coins(p, PAYDAY);
                self.events.push(Ev::Payday { p });
            }
            Kind::Coin => {
                self.coins(p, 3);
            }
            Kind::Pit => {
                if flooded {
                    self.text("Blasted! The barrage costs you coins.");
                    self.coins(p, -4);
                } else {
                    self.text("Into the pit!");
                    self.coins(p, -3);
                }
            }
            Kind::Event => {
                self.event();
                return;
            }
            Kind::Item => self.give_item(p),
            Kind::Shop => {
                let affordable = ITEMS.iter().any(|i| i.price() <= self.players[p].coins);
                if affordable && self.players[p].items.len() < MAX_ITEMS {
                    self.phase = Phase::ShopMenu;
                    return;
                }
                self.text("The shop is closed to you: not enough coins or hands full.");
            }
            Kind::Warp => {
                let pads: Vec<usize> = (0..self.board.spaces.len()).filter(|&i| i != s && self.board.spaces[i].kind == Kind::Warp).collect();
                if !pads.is_empty() {
                    let to = pads[self.rng.below(pads.len())];
                    self.players[p].space = to;
                    self.events.push(Ev::Warped { p, from: s, to });
                }
            }
            Kind::Duel => {
                self.phase = Phase::DuelPick;
                return;
            }
            Kind::Hex => self.hex(p),
            Kind::Keeper => {
                if s == self.keeper {
                    if self.players[p].coins >= SUNSTONE_COST {
                        self.phase = Phase::KeeperOffer;
                        return;
                    }
                    self.text(format!("The Keeper wants {SUNSTONE_COST} coins for a Sunstone."));
                } else {
                    self.coins(p, 2);
                }
            }
        }
        self.finish_turn();
    }

    fn give_item(&mut self, p: usize) {
        if self.players[p].items.len() >= MAX_ITEMS {
            self.text("Hands full: take 3 coins instead.");
            self.coins(p, 3);
            return;
        }
        let item = ITEMS[self.rng.below(ITEMS.len())];
        self.players[p].items.push(item);
        self.events.push(Ev::Gift { p, item });
    }

    fn hex(&mut self, p: usize) {
        match self.rng.below(3) {
            0 => {
                self.players[p].slow = true;
                self.text("Slow Boots hex: your next roll is capped at 3.");
            }
            1 => {
                self.text("Leaky Pockets hex: lose 5 coins.");
                self.coins(p, -5);
            }
            _ => {
                if self.players[p].items.pop().is_some() {
                    self.text("Glum hex: you lose an item.");
                } else {
                    self.text("Glum hex: lose 3 coins.");
                    self.coins(p, -3);
                }
            }
        }
    }

    fn event(&mut self) {
        let p = self.cur;
        let k = EVENTS[self.rng.below(EVENTS.len())];
        match k {
            EventKind::CoinRain => {
                self.text("Coin Rain! +6 coins.");
                self.coins(p, 6);
            }
            EventKind::GhostThief => {
                self.text("A ghost picks your pocket: -4 coins.");
                self.coins(p, -4);
            }
            EventKind::GiftWrap => {
                self.text("A gift-wrapped surprise!");
                self.give_item(p);
            }
            EventKind::Gust => {
                self.text("Gust of Fortune! Blown 3 spaces ahead.");
                self.moves_left = 3;
                self.advance();
                return;
            }
            EventKind::SwapSpots => {
                let others: Vec<usize> = (0..4).filter(|&q| q != p).collect();
                let q = others[self.rng.below(others.len())];
                let (a, b) = (self.players[p].space, self.players[q].space);
                self.players[p].space = b;
                self.players[q].space = a;
                self.events.push(Ev::Swapped { a: p, b: q });
                self.text(format!("Swap Spots with {}!", self.players[q].name));
            }
            EventKind::RobinHood => {
                let rich = (0..4).max_by_key(|&q| (self.players[q].coins, 4 - q)).unwrap();
                let poor = (0..4).min_by_key(|&q| (self.players[q].coins, q)).unwrap();
                if rich != poor {
                    let amt = (self.players[rich].coins / 4).clamp(1, 5);
                    self.coins(rich, -amt);
                    self.coins(poor, amt);
                    self.text(format!("Robin Hood: {} shares {amt} coins with {}.", self.players[rich].name, self.players[poor].name));
                }
            }
            EventKind::TaxToad => {
                let tax = (self.players[p].coins / 8).max(2);
                self.text(format!("The Tax Toad collects {tax} coins."));
                self.coins(p, -tax);
            }
            EventKind::LuckyCharm => {
                self.players[p].lucky = 2;
                self.text("Lucky Charm: +2 on your next roll.");
            }
            EventKind::PartyFavor => {
                self.text("Party Favor! Everyone gets 2 coins.");
                for q in 0..4 {
                    self.coins(q, 2);
                }
            }
            EventKind::Rumor => {
                self.text("A rumor: the Keeper has moved!");
                self.move_keeper();
            }
        }
        self.finish_turn();
    }

    pub(super) fn finish_turn(&mut self) {
        self.die = Die::Normal;
        self.used_item = false;
        self.rolled = false;
        self.moves_left = 0;
        self.cur += 1;
        if self.cur >= 4 {
            self.cur = 0;
            self.begin_round_mini();
        } else {
            self.phase = Phase::TurnStart;
            self.events.push(Ev::TurnStart(self.cur));
        }
    }

    /// A shuffle bag: one of the least played games (never the last one again), so every game comes up before any repeats.
    fn pick_mini(&mut self) -> Mini {
        let fresh: Vec<Mini> = ALL_MINIS.iter().copied().filter(|m| Some(*m) != self.last_mini).collect();
        let least = fresh.iter().map(|m| self.mini_plays[m.idx()]).min().unwrap_or(0);
        let pool: Vec<Mini> = fresh.into_iter().filter(|m| self.mini_plays[m.idx()] == least).collect();
        let m = pool[self.rng.below(pool.len())];
        self.mini_plays[m.idx()] += 1;
        m
    }

    fn begin_round_mini(&mut self) {
        let suns = self.players.iter().filter(|p| p.tint).count();
        let (format, teams) = match suns {
            4 | 0 => (Format::Ffa, [0, 1, 2, 3]),
            2 => (Format::TwoVsTwo, std::array::from_fn(|i| if self.players[i].tint { 0 } else { 1 })),
            // The lone player is team 0.
            1 => (Format::OneVsThree, std::array::from_fn(|i| if self.players[i].tint { 0 } else { 1 })),
            _ => (Format::OneVsThree, std::array::from_fn(|i| if self.players[i].tint { 1 } else { 0 })),
        };
        self.start_mini(format, [true; 4], teams, true);
    }

    pub(super) fn start_mini(&mut self, format: Format, active: [bool; 4], teams: [u8; 4], round_end: bool) {
        let kind = self.pick_mini();
        let seed = self.rng.next_u64();
        self.last_mini = Some(kind);
        self.minis_played += 1;
        self.events.push(Ev::MiniStart);
        self.phase = Phase::Mini(MiniCtx { kind, format, active, teams, round_end, seed });
    }

    pub(super) fn finish_mini(&mut self, ctx: MiniCtx, scores: [f32; 4]) {
        let mut scores = scores;
        for p in 0..4 {
            if !ctx.active[p] {
                scores[p] = f32::NEG_INFINITY;
            }
        }
        let ranks = ranks(&scores, &ctx.active);
        let mut won = [false; 4];
        let mut coins = [0i32; 4];
        let team_mean = |t: u8| {
            let m: Vec<f32> = (0..4).filter(|&p| ctx.active[p] && ctx.teams[p] == t).map(|p| scores[p]).collect();
            if m.is_empty() {
                f32::NEG_INFINITY
            } else {
                m.iter().sum::<f32>() / m.len() as f32
            }
        };
        match ctx.format {
            Format::Ffa => {
                let all_tied = (0..4).all(|p| ranks[p] == 1);
                for p in 0..4 {
                    if all_tied {
                        coins[p] = 4;
                    } else if ranks[p] == 1 {
                        coins[p] = 8;
                        won[p] = true;
                    } else if ranks[p] == 2 {
                        coins[p] = 4;
                    }
                }
            }
            Format::TwoVsTwo | Format::OneVsThree => {
                let (a, b) = (team_mean(0), team_mean(1));
                let winner = if (a - b).abs() < 1e-4 { None } else if a > b { Some(0) } else { Some(1) };
                for p in 0..4 {
                    match winner {
                        None => coins[p] = 2,
                        Some(w) if ctx.teams[p] == w => {
                            won[p] = true;
                            let solo = (0..4).filter(|&q| ctx.teams[q] == w).count() == 1;
                            coins[p] = if ctx.format == Format::TwoVsTwo { 6 } else if solo { 10 } else { 5 };
                        }
                        _ => {}
                    }
                }
            }
            Format::Duel => {
                let ids: Vec<usize> = (0..4).filter(|&p| ctx.active[p]).collect();
                let (a, b) = (ids[0], ids[1]);
                if (scores[a] - scores[b]).abs() > 1e-4 {
                    let (w, l) = if scores[a] > scores[b] { (a, b) } else { (b, a) };
                    let amt = self.players[l].coins.min(5);
                    won[w] = true;
                    coins[w] = amt;
                    coins[l] = -amt;
                }
            }
        }
        let mut applied = [0i32; 4];
        for p in 0..4 {
            if coins[p] != 0 {
                applied[p] = self.coins(p, coins[p]);
            }
            if won[p] {
                self.players[p].wins += 1;
            }
        }
        self.phase = Phase::MiniResult(MiniReport { kind: ctx.kind, format: ctx.format, active: ctx.active, teams: ctx.teams, scores, ranks, won, coins: applied, round_end: ctx.round_end });
    }

    pub(super) fn end_round(&mut self) {
        let done = self.round;
        self.round += 1;
        match self.board.gimmick {
            Gimmick::Bridge => {
                self.cond.flip = !self.cond.flip;
                self.events.push(Ev::Flip(self.cond.flip));
            }
            Gimmick::Wind => {
                self.cond.wind = self.rng.chance(0.5);
                self.events.push(Ev::Wind(self.cond.wind));
            }
            Gimmick::Tide => {
                if done % 4 == 3 && self.cond.tide < 2 {
                    self.cond.tide += 1;
                    self.events.push(Ev::Tide(self.cond.tide));
                    for p in 0..4 {
                        let s = self.players[p].space;
                        if self.board.flooded(s, &self.cond) {
                            self.coins(p, -3);
                        }
                    }
                    if self.board.flooded(self.keeper, &self.cond) {
                        self.move_keeper();
                    }
                }
            }
        }
        if self.round > self.cfg.turns {
            self.finish_game();
            return;
        }
        self.cur = 0;
        self.phase = Phase::TurnStart;
        self.events.push(Ev::RoundStart(self.round));
        self.events.push(Ev::TurnStart(0));
    }

    fn finish_game(&mut self) {
        let leaders = |vals: [i32; 4]| -> (Vec<usize>, i32) {
            let m = *vals.iter().max().unwrap();
            ((0..4).filter(|&p| vals[p] == m).collect(), m)
        };
        let mut awards = vec![];
        let (w, v) = leaders(std::array::from_fn(|p| self.players[p].coins));
        awards.push(Award { name: "Coin Hoarder", desc: "Most coins at the end", winners: w, value: v });
        let (w, v) = leaders(std::array::from_fn(|p| self.players[p].wins as i32));
        awards.push(Award { name: "Mini-game Master", desc: "Most mini-game wins", winners: w, value: v });
        let (name, desc, kinds): (&'static str, &'static str, Vec<Kind>) = match self.bonus_pick {
            0 => ("Coin Collector", "Most Coin space landings", vec![Kind::Coin]),
            1 => ("Unlucky Star", "Most Pit and Hex landings", vec![Kind::Pit, Kind::Hex]),
            2 => ("Event Fan", "Most Event space landings", vec![Kind::Event]),
            3 => ("Window Shopper", "Most Shop space landings", vec![Kind::Shop]),
            _ => ("Duelist", "Most Duel space landings", vec![Kind::Duel]),
        };
        let (w, v) = leaders(std::array::from_fn(|p| kinds.iter().map(|k| self.players[p].lands[k.idx()] as i32).sum()));
        awards.push(Award { name, desc, winners: w, value: v });
        for a in &mut awards {
            // An award nobody earned (0 of something) is not given.
            if a.value <= 0 {
                a.winners.clear();
            }
            for &p in &a.winners {
                self.players[p].sun += 1;
                self.players[p].bonus += 1;
            }
            self.events.push(Ev::Bonus { name: a.name, winners: a.winners.clone() });
        }
        self.awards = awards;
        self.phase = Phase::Over;
        self.events.push(Ev::GameOver);
    }

    /// Final order: Sunstones, then coins, then mini-game wins, then seat.
    pub fn standings(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..4).collect();
        v.sort_by_key(|&p| (std::cmp::Reverse(self.players[p].sun), std::cmp::Reverse(self.players[p].coins), std::cmp::Reverse(self.players[p].wins), p));
        v
    }

    /// Players tied for first place (same Sunstones and coins).
    pub fn winners(&self) -> Vec<usize> {
        let s = self.standings();
        let top = &self.players[s[0]];
        s.into_iter().filter(|&p| self.players[p].sun == top.sun && self.players[p].coins == top.coins).collect()
    }
}
