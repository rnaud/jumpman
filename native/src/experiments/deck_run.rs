//! Star Deck, run structure: the branching map, encounters, rewards, shops, rest sites, events and
//! treasure, all engine-free and seeded. [`Run`] owns the persistent state between fights (health,
//! coins, deck, relics, potions) and hands a [`Combat`] its inputs.
use super::deck_core::*;

pub const ROW_WIDTHS: [usize; 6] = [2, 3, 3, 3, 2, 2];
pub const BOSS_ROW: usize = ROW_WIDTHS.len();
pub const START_COINS: i32 = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum NodeKind {
    Battle,
    Elite,
    Shop,
    Rest,
    Event,
    Treasure,
    Boss,
}

impl NodeKind {
    pub fn name(self) -> &'static str {
        match self {
            NodeKind::Battle => "Battle",
            NodeKind::Elite => "Elite",
            NodeKind::Shop => "Shop",
            NodeKind::Rest => "Campfire",
            NodeKind::Event => "Mystery",
            NodeKind::Treasure => "Treasure",
            NodeKind::Boss => "Boss",
        }
    }
    /// Letter on the map pin.
    pub fn glyph(self) -> &'static str {
        match self {
            NodeKind::Battle => "B",
            NodeKind::Elite => "E",
            NodeKind::Shop => "$",
            NodeKind::Rest => "R",
            NodeKind::Event => "?",
            NodeKind::Treasure => "T",
            NodeKind::Boss => "K",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: usize,
    pub row: usize,
    /// Horizontal position within the row, 0..1.
    pub x: f32,
    pub kind: NodeKind,
    pub next: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Map {
    pub nodes: Vec<Node>,
    pub rows: Vec<Vec<usize>>,
}

impl Map {
    pub fn generate(rng: &mut Rng) -> Map {
        let mut nodes: Vec<Node> = vec![];
        let mut rows: Vec<Vec<usize>> = vec![];
        for (r, w) in ROW_WIDTHS.iter().enumerate() {
            let mut row = vec![];
            for i in 0..*w {
                // A little jitter so the map does not look like a grid.
                let x = ((i as f32 + 0.5) / *w as f32 + (rng.f32() - 0.5) * 0.08).clamp(0.05, 0.95);
                nodes.push(Node { id: nodes.len(), row: r, x, kind: NodeKind::Battle, next: vec![] });
                row.push(nodes.len() - 1);
            }
            rows.push(row);
        }
        nodes.push(Node { id: nodes.len(), row: BOSS_ROW, x: 0.5, kind: NodeKind::Boss, next: vec![] });
        rows.push(vec![nodes.len() - 1]);
        // Edges: each node reaches its nearest next-row node(s), then orphans get a parent.
        for r in 0..BOSS_ROW {
            let (cur, nxt) = (rows[r].clone(), rows[r + 1].clone());
            if r + 1 == BOSS_ROW {
                for c in &cur {
                    nodes[*c].next.push(nxt[0]);
                }
                continue;
            }
            for c in &cur {
                let cx = nodes[*c].x;
                let mut order: Vec<usize> = nxt.clone();
                order.sort_by(|a, b| (nodes[*a].x - cx).abs().partial_cmp(&(nodes[*b].x - cx).abs()).unwrap());
                nodes[*c].next.push(order[0]);
                if order.len() > 1 && rng.chance(0.55) && (nodes[order[1]].x - cx).abs() < 0.5 {
                    nodes[*c].next.push(order[1]);
                }
            }
            for n in &nxt {
                if !cur.iter().any(|c| nodes[*c].next.contains(n)) {
                    let nx = nodes[*n].x;
                    let parent = *cur.iter().min_by(|a, b| (nodes[**a].x - nx).abs().partial_cmp(&(nodes[**b].x - nx).abs()).unwrap()).unwrap();
                    nodes[parent].next.push(*n);
                }
            }
            for c in &cur {
                let mut next = std::mem::take(&mut nodes[*c].next);
                next.sort_by(|a, b| nodes[*a].x.partial_cmp(&nodes[*b].x).unwrap());
                next.dedup();
                nodes[*c].next = next;
            }
        }
        // Kinds.
        for r in 1..BOSS_ROW {
            for id in rows[r].clone() {
                let roll = rng.below(100);
                nodes[id].kind = if r == BOSS_ROW - 1 {
                    NodeKind::Rest
                } else if roll < 44 {
                    NodeKind::Battle
                } else if roll < 62 {
                    NodeKind::Event
                } else if roll < 72 {
                    NodeKind::Shop
                } else if roll < 82 {
                    NodeKind::Rest
                } else if roll < 92 && r >= 2 {
                    NodeKind::Elite
                } else if r >= 2 {
                    NodeKind::Treasure
                } else {
                    NodeKind::Battle
                };
            }
        }
        // The last row before the boss: a campfire and a shop.
        let last = rows[BOSS_ROW - 1].clone();
        nodes[last[0]].kind = NodeKind::Rest;
        nodes[last[1]].kind = NodeKind::Shop;
        if rng.chance(0.5) {
            let (a, b) = (nodes[last[0]].kind, nodes[last[1]].kind);
            nodes[last[0]].kind = b;
            nodes[last[1]].kind = a;
        }
        // Guarantees: at least one of each special kind, each on its own node (rows 1..=4).
        let mut locked: Vec<usize> = vec![];
        for (kind, rlo, rhi) in [(NodeKind::Elite, 3, 4), (NodeKind::Treasure, 2, 3), (NodeKind::Event, 1, 3), (NodeKind::Shop, 1, 3), (NodeKind::Rest, 1, 3)] {
            let have = (1..=BOSS_ROW - 1).any(|r| rows[r].iter().any(|id| nodes[*id].kind == kind));
            if !have {
                let count = |nodes: &Vec<Node>, k: NodeKind| (1..BOSS_ROW).map(|r| rows[r].iter().filter(|id| nodes[**id].kind == k).count()).sum::<usize>();
                let cands: Vec<usize> = (rlo..=rhi).flat_map(|r| rows[r].iter().copied()).filter(|id| !locked.contains(id) && (!matches!(nodes[*id].kind, NodeKind::Elite | NodeKind::Treasure | NodeKind::Event | NodeKind::Shop | NodeKind::Rest) || count(&nodes, nodes[*id].kind) > 1)).collect();
                let id = cands[rng.below(cands.len() as u32) as usize];
                nodes[id].kind = kind;
                locked.push(id);
            }
        }
        Map { nodes, rows }
    }

    pub fn boss(&self) -> usize {
        self.nodes.len() - 1
    }

    pub fn start_nodes(&self) -> &[usize] {
        &self.rows[0]
    }

    /// Whether `from` can reach the boss (every node must).
    pub fn reaches_boss(&self, from: usize) -> bool {
        let mut seen = vec![false; self.nodes.len()];
        let mut stack = vec![from];
        while let Some(n) = stack.pop() {
            if n == self.boss() {
                return true;
            }
            if std::mem::replace(&mut seen[n], true) {
                continue;
            }
            stack.extend(self.nodes[n].next.iter().copied());
        }
        false
    }

    /// Nodes reachable from the start row.
    pub fn reachable(&self) -> Vec<bool> {
        let mut seen = vec![false; self.nodes.len()];
        let mut stack: Vec<usize> = self.rows[0].clone();
        while let Some(n) = stack.pop() {
            if std::mem::replace(&mut seen[n], true) {
                continue;
            }
            stack.extend(self.nodes[n].next.iter().copied());
        }
        seen
    }
}

// ---------------------------------------------------------------------------------------------
// Encounters, rewards, shop pricing
// ---------------------------------------------------------------------------------------------

pub fn encounter(kind: NodeKind, row: usize, rng: &mut Rng) -> Vec<EnemyKind> {
    use EnemyKind::*;
    match kind {
        NodeKind::Boss => vec![FuseKing],
        NodeKind::Elite => vec![if rng.chance(0.5) { TetherHound } else { HornCaptain }],
        _ => {
            let easy: &[&[EnemyKind]] = &[&[Scuttler, Scuttler], &[Scuttler, Swooper], &[Prickle], &[Swooper, Swooper, Swooper], &[FusePup, Scuttler], &[Scuttler, Scuttler, Swooper]];
            let hard: &[&[EnemyKind]] = &[&[Rammer, Scuttler], &[Snapvine, Zapper], &[Gloomwisp, FusePup], &[Shellback, Prickle], &[Slab], &[Zapper, Swooper, Swooper], &[Rammer, Gloomwisp], &[Snapvine, Scuttler, FusePup], &[Shellback, Gloomwisp]];
            let pool = if row <= 1 { easy } else if row == 2 && rng.chance(0.4) { easy } else { hard };
            pool[rng.below(pool.len() as u32) as usize].to_vec()
        }
    }
}

/// Rarity weights (per 100) for card rewards: normal fights, elites, shop stock.
pub fn rarity_weights(kind: NodeKind) -> [u32; 3] {
    match kind {
        NodeKind::Elite | NodeKind::Boss => [45, 40, 15],
        NodeKind::Shop => [55, 35, 10],
        _ => [60, 33, 7],
    }
}

pub fn roll_rarity(w: [u32; 3], rng: &mut Rng) -> Rarity {
    let r = rng.below(w[0] + w[1] + w[2]);
    if r < w[0] {
        Rarity::Common
    } else if r < w[0] + w[1] {
        Rarity::Uncommon
    } else {
        Rarity::Rare
    }
}

/// `n` distinct card choices.
pub fn roll_cards(n: usize, kind: NodeKind, rng: &mut Rng) -> Vec<Card> {
    let mut out: Vec<Card> = vec![];
    let w = rarity_weights(kind);
    let mut guard = 0;
    while out.len() < n && guard < 100 {
        guard += 1;
        let pool = reward_pool(roll_rarity(w, rng));
        let id = pool[rng.below(pool.len() as u32) as usize];
        if !out.iter().any(|c| c.id == id) {
            out.push(Card::new(id));
        }
    }
    out
}

pub fn card_price(c: Card, rng: &mut Rng) -> i32 {
    let base = match c.def().rarity {
        Rarity::Common => 45,
        Rarity::Uncommon => 75,
        Rarity::Rare => 150,
        _ => 30,
    };
    let jitter = 90 + rng.below(21) as i32;
    base * jitter / 100
}

pub fn remove_price(removed: u32) -> i32 {
    75 + 25 * removed as i32
}

#[derive(Clone, Debug, Default)]
pub struct Reward {
    pub gold: i32,
    pub cards: Vec<Card>,
    pub relic: Option<Relic>,
    pub potion: Option<Potion>,
    pub gold_taken: bool,
    pub relic_taken: bool,
    pub potion_taken: bool,
    pub card_done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Shop {
    pub cards: Vec<(Card, i32, bool)>,
    pub relics: Vec<(Relic, i32, bool)>,
    pub potions: Vec<(Potion, i32, bool)>,
    pub removed: u32,
    pub removal_used: bool,
}

// ---------------------------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventId {
    WarpPipe,
    ToadStall,
    MushroomRing,
    StarFountain,
    CannonAlcove,
}

pub const EVENTS: [EventId; 5] = [EventId::WarpPipe, EventId::ToadStall, EventId::MushroomRing, EventId::StarFountain, EventId::CannonAlcove];

impl EventId {
    pub fn title(self) -> &'static str {
        match self {
            EventId::WarpPipe => "The Humming Pipe",
            EventId::ToadStall => "Toad's Back-Alley Stall",
            EventId::MushroomRing => "Ring of Mushrooms",
            EventId::StarFountain => "The Star Fountain",
            EventId::CannonAlcove => "Cannon Alcove",
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            EventId::WarpPipe => "A green pipe hums with warm light. Something rattles deep inside.",
            EventId::ToadStall => "A hooded Toad lays out wares on a crate. \"Psst. Cheap. Don't ask.\"",
            EventId::MushroomRing => "Two toadstools glow in a fairy ring: one red, one blue-spotted.",
            EventId::StarFountain => "Star-water trickles over mossy stone. Coins glint at the bottom.",
            EventId::CannonAlcove => "An old cannon points at a distant ledge, fuse still dry.",
        }
    }
    /// (label, detail) per option; `Run::event_enabled` says which can be chosen.
    pub fn options(self) -> Vec<(&'static str, &'static str)> {
        match self {
            EventId::WarpPipe => vec![("Jump in", "50%: a relic. 50%: lose 10 HP, find 30 coins."), ("Peek inside", "Gain 15 coins."), ("Leave", "Walk away.")],
            EventId::ToadStall => vec![("Buy a card (25 coins)", "A random uncommon card."), ("Haggle", "Gain 12 coins. Add a Bruise to your deck."), ("Leave", "Walk away.")],
            EventId::MushroomRing => vec![("Eat the red one", "Heal 25% of max HP."), ("Eat the blue-spotted one", "+6 max HP, take 6 damage."), ("Leave", "Walk away.")],
            EventId::StarFountain => vec![("Toss in 30 coins", "Upgrade a random card."), ("Drink deeply", "Heal 10 HP. Add a Bruise."), ("Leave", "Walk away.")],
            EventId::CannonAlcove => vec![("Fire yourself", "Lose 12 HP. Gain a random rare card."), ("Scrap the cannon", "Gain 40 coins, lose 5 HP."), ("Leave", "Walk away.")],
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Map,
    Combat,
    Reward,
    Shop,
    Rest,
    Event,
    Treasure,
    Won,
    Lost,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Stats {
    pub nodes_cleared: u32,
    pub battles: u32,
    pub elites: u32,
    pub cards_played: u32,
    pub damage: i32,
    pub best_chain: i32,
    pub coins_earned: i32,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub seed: u64,
    pub hero: Hero,
    pub rng: Rng,
    pub hp: i32,
    pub max_hp: i32,
    pub coins: i32,
    pub deck: Vec<Card>,
    pub relics: Vec<Relic>,
    pub potions: [Option<Potion>; POTION_SLOTS],
    pub map: Map,
    pub pos: Option<usize>,
    pub cleared: Vec<usize>,
    pub screen: Screen,
    pub combat: Option<Combat>,
    pub reward: Option<Reward>,
    pub shop: Option<Shop>,
    pub event: Option<EventId>,
    pub event_result: Option<String>,
    pub treasure: Option<Relic>,
    pub rested: bool,
    pub seen_events: Vec<EventId>,
    pub stats: Stats,
    /// Node being fought (kind, row) for rewards.
    fight_node: Option<usize>,
}

impl Run {
    pub fn new(seed: u64, hero: Hero) -> Run {
        let mut rng = Rng::new(seed ^ 0x57A2_DEC4);
        let map = Map::generate(&mut rng);
        Run {
            seed,
            hero,
            rng,
            hp: hero.max_hp(),
            max_hp: hero.max_hp(),
            coins: START_COINS,
            deck: hero.starter_deck(),
            relics: vec![],
            potions: [None; POTION_SLOTS],
            map,
            pos: None,
            cleared: vec![],
            screen: Screen::Map,
            combat: None,
            reward: None,
            shop: None,
            event: None,
            event_result: None,
            treasure: None,
            rested: false,
            seen_events: vec![],
            stats: Stats::default(),
            fight_node: None,
        }
    }

    pub fn has(&self, r: Relic) -> bool {
        self.relics.contains(&r)
    }

    /// Nodes you may step to now.
    pub fn available(&self) -> Vec<usize> {
        match self.pos {
            None => self.map.rows[0].clone(),
            Some(p) => self.map.nodes[p].next.clone(),
        }
    }

    pub fn add_relic(&mut self, r: Relic) {
        if self.has(r) {
            return;
        }
        self.relics.push(r);
        match r {
            Relic::BigMushroom => {
                self.max_hp += 12;
                self.hp += 12;
            }
            Relic::CoinPouch => {
                self.coins += 30;
                self.stats.coins_earned += 30;
            }
            _ => {}
        }
    }

    pub fn random_relic(&mut self) -> Option<Relic> {
        let pool: Vec<Relic> = RELICS.iter().copied().filter(|r| !self.has(*r)).collect();
        if pool.is_empty() {
            return None;
        }
        Some(pool[self.rng.below(pool.len() as u32) as usize])
    }

    pub fn gain_coins(&mut self, n: i32) {
        self.coins += n;
        if n > 0 {
            self.stats.coins_earned += n;
        }
    }

    pub fn heal(&mut self, n: i32) {
        self.hp = (self.hp + n).min(self.max_hp);
    }

    pub fn add_potion(&mut self, p: Potion) -> bool {
        if let Some(slot) = self.potions.iter_mut().find(|s| s.is_none()) {
            *slot = Some(p);
            true
        } else {
            false
        }
    }

    pub fn enter_node(&mut self, id: usize) -> Result<(), &'static str> {
        if self.screen != Screen::Map {
            return Err("not on the map");
        }
        if !self.available().contains(&id) {
            return Err("not reachable");
        }
        self.pos = Some(id);
        let node = self.map.nodes[id].clone();
        match node.kind {
            NodeKind::Battle | NodeKind::Elite | NodeKind::Boss => {
                let foes = encounter(node.kind, node.row, &mut self.rng);
                self.start_combat(&foes);
                self.fight_node = Some(id);
            }
            NodeKind::Shop => {
                self.shop = Some(self.stock_shop());
                self.screen = Screen::Shop;
            }
            NodeKind::Rest => {
                self.rested = false;
                self.screen = Screen::Rest;
            }
            NodeKind::Event => {
                let pool: Vec<EventId> = EVENTS.iter().copied().filter(|e| !self.seen_events.contains(e)).collect();
                let pool = if pool.is_empty() { EVENTS.to_vec() } else { pool };
                let ev = pool[self.rng.below(pool.len() as u32) as usize];
                self.seen_events.push(ev);
                self.event = Some(ev);
                self.event_result = None;
                self.screen = Screen::Event;
            }
            NodeKind::Treasure => {
                self.treasure = self.random_relic();
                self.screen = Screen::Treasure;
            }
        }
        Ok(())
    }

    /// Start a fight with an explicit foe list (also used by test hooks).
    pub fn start_combat(&mut self, foes: &[EnemyKind]) {
        let seed = self.rng.next_u32() as u64 | (self.rng.next_u32() as u64) << 32;
        self.combat = Some(Combat::new(seed, self.hero, self.hp, self.max_hp, &self.deck, &self.relics, self.potions, foes));
        self.screen = Screen::Combat;
    }

    /// Call when the combat is over: banks health and potions, builds rewards or ends the run.
    pub fn finish_combat(&mut self) {
        let Some(c) = self.combat.take() else { return };
        self.hp = c.hp.max(0);
        self.potions = c.potions;
        self.stats.cards_played += c.cards_played;
        self.stats.damage += c.damage_dealt;
        self.stats.best_chain = self.stats.best_chain.max(c.best_chain);
        if c.outcome == Outcome::Lost {
            self.screen = Screen::Lost;
            return;
        }
        let node = self.fight_node.take().map(|i| self.map.nodes[i].clone());
        let kind = node.as_ref().map_or(NodeKind::Battle, |n| n.kind);
        self.stats.battles += 1;
        if self.has(Relic::HeartLocket) {
            self.heal(6);
        }
        let mut gold = match kind {
            NodeKind::Elite => 28 + self.rng.below(12) as i32,
            NodeKind::Boss => 60,
            _ => 12 + self.rng.below(10) as i32,
        };
        if self.has(Relic::CoinPouch) {
            gold += 6;
        }
        if kind == NodeKind::Elite {
            self.stats.elites += 1;
        }
        let mut r = Reward { gold, cards: roll_cards(3, kind, &mut self.rng), ..Default::default() };
        if kind == NodeKind::Elite {
            r.relic = self.random_relic();
        }
        if kind != NodeKind::Boss && self.rng.chance(0.4) && self.potions.iter().any(|p| p.is_none()) {
            r.potion = Some(POTIONS[self.rng.below(POTIONS.len() as u32) as usize]);
        }
        if kind == NodeKind::Boss {
            r.cards.clear();
            r.card_done = true;
        }
        self.reward = Some(r);
        self.screen = Screen::Reward;
    }

    fn node_done(&mut self) {
        if let Some(p) = self.pos {
            self.cleared.push(p);
            self.stats.nodes_cleared += 1;
            if self.map.nodes[p].kind == NodeKind::Boss {
                self.screen = Screen::Won;
                return;
            }
        }
        self.screen = Screen::Map;
    }

    // Rewards -------------------------------------------------------------------------------

    pub fn take_card(&mut self, i: usize) -> bool {
        let Some(r) = self.reward.as_mut() else { return false };
        if r.card_done || i >= r.cards.len() {
            return false;
        }
        let c = r.cards[i];
        r.card_done = true;
        self.deck.push(c);
        true
    }
    pub fn skip_card(&mut self) {
        if let Some(r) = self.reward.as_mut() {
            r.card_done = true;
        }
    }
    pub fn take_gold(&mut self) {
        let Some(r) = self.reward.as_mut() else { return };
        if !r.gold_taken {
            r.gold_taken = true;
            let g = r.gold;
            self.gain_coins(g);
        }
    }
    pub fn take_relic(&mut self) {
        let Some(r) = self.reward.as_mut() else { return };
        if !r.relic_taken {
            if let Some(x) = r.relic {
                r.relic_taken = true;
                self.add_relic(x);
            }
        }
    }
    pub fn take_potion(&mut self) -> bool {
        let Some(r) = self.reward.as_mut() else { return false };
        let Some(p) = r.potion else { return false };
        if r.potion_taken {
            return false;
        }
        if self.potions.iter().any(|s| s.is_none()) {
            r.potion_taken = true;
            self.add_potion(p);
            true
        } else {
            false
        }
    }
    /// Collect everything left (gold/relic) and move on.
    pub fn leave_reward(&mut self) {
        self.take_gold();
        self.take_relic();
        self.reward = None;
        self.node_done();
    }

    // Shop ----------------------------------------------------------------------------------

    fn stock_shop(&mut self) -> Shop {
        let cards = roll_cards(5, NodeKind::Shop, &mut self.rng);
        let mut shop = Shop::default();
        for c in cards {
            let p = card_price(c, &mut self.rng);
            shop.cards.push((c, p, false));
        }
        for _ in 0..2 {
            if let Some(r) = self.random_relic() {
                if !shop.relics.iter().any(|(x, _, _)| *x == r) {
                    shop.relics.push((r, r.price(), false));
                }
            }
        }
        for _ in 0..2 {
            let p = POTIONS[self.rng.below(POTIONS.len() as u32) as usize];
            shop.potions.push((p, p.price(), false));
        }
        shop
    }

    pub fn buy_card(&mut self, i: usize) -> bool {
        let Some(shop) = self.shop.as_mut() else { return false };
        let Some(&(c, price, sold)) = shop.cards.get(i) else { return false };
        if sold || self.coins < price {
            return false;
        }
        shop.cards[i].2 = true;
        self.coins -= price;
        self.deck.push(c);
        true
    }
    pub fn buy_relic(&mut self, i: usize) -> bool {
        let Some(shop) = self.shop.as_mut() else { return false };
        let Some(&(r, price, sold)) = shop.relics.get(i) else { return false };
        if sold || self.coins < price {
            return false;
        }
        shop.relics[i].2 = true;
        self.coins -= price;
        self.add_relic(r);
        true
    }
    pub fn buy_potion(&mut self, i: usize) -> bool {
        let Some(shop) = self.shop.as_mut() else { return false };
        let Some(&(p, price, sold)) = shop.potions.get(i) else { return false };
        if sold || self.coins < price || !self.potions.iter().any(|s| s.is_none()) {
            return false;
        }
        shop.potions[i].2 = true;
        self.coins -= price;
        self.add_potion(p);
        true
    }
    pub fn removal_price(&self) -> i32 {
        remove_price(self.shop.as_ref().map_or(0, |s| s.removed) + self.stats_removed())
    }
    fn stats_removed(&self) -> u32 {
        0
    }
    /// Pay to remove deck card `i` (one removal per visit).
    pub fn remove_card(&mut self, i: usize) -> bool {
        let price = self.removal_price();
        let Some(shop) = self.shop.as_mut() else { return false };
        if shop.removal_used || self.coins < price || i >= self.deck.len() || self.deck.len() <= 5 {
            return false;
        }
        shop.removal_used = true;
        shop.removed += 1;
        self.coins -= price;
        self.deck.remove(i);
        true
    }
    pub fn leave_shop(&mut self) {
        self.shop = None;
        self.node_done();
    }

    // Rest ----------------------------------------------------------------------------------

    pub fn rest_heal(&mut self) -> bool {
        if self.rested {
            return false;
        }
        self.rested = true;
        let n = (self.max_hp as f32 * 0.3).round() as i32;
        self.heal(n);
        true
    }
    pub fn rest_upgrade(&mut self, i: usize) -> bool {
        if self.rested || i >= self.deck.len() || !self.deck[i].can_upgrade() {
            return false;
        }
        self.rested = true;
        self.deck[i].up = true;
        true
    }
    pub fn leave_rest(&mut self) {
        self.node_done();
    }

    // Events --------------------------------------------------------------------------------

    pub fn event_enabled(&self, i: usize) -> bool {
        match (self.event, i) {
            (Some(EventId::ToadStall), 0) => self.coins >= 25,
            (Some(EventId::StarFountain), 0) => self.coins >= 30 && self.deck.iter().any(|c| c.can_upgrade()),
            _ => true,
        }
    }

    pub fn event_choose(&mut self, i: usize) -> bool {
        let Some(ev) = self.event else { return false };
        if self.event_result.is_some() || i >= ev.options().len() || !self.event_enabled(i) {
            return false;
        }
        let msg: String = match (ev, i) {
            (EventId::WarpPipe, 0) => {
                if self.rng.chance(0.5) {
                    match self.random_relic() {
                        Some(r) => {
                            self.add_relic(r);
                            format!("A trinket tumbles out: {}!", r.name())
                        }
                        None => {
                            self.gain_coins(30);
                            "Only coins this time: +30.".into()
                        }
                    }
                } else {
                    self.hp = (self.hp - 10).max(1);
                    self.gain_coins(30);
                    "Something bites! -10 HP, but you grab 30 coins.".into()
                }
            }
            (EventId::WarpPipe, 1) => {
                self.gain_coins(15);
                "You fish out 15 coins.".into()
            }
            (EventId::ToadStall, 0) => {
                self.coins -= 25;
                let pool = reward_pool(Rarity::Uncommon);
                let c = Card::new(pool[self.rng.below(pool.len() as u32) as usize]);
                self.deck.push(c);
                format!("Toad slips you {}.", c.name())
            }
            (EventId::ToadStall, 1) => {
                self.gain_coins(12);
                self.deck.push(Card::new(card_id("bruise")));
                "+12 coins, and a Bruise from the scuffle.".into()
            }
            (EventId::MushroomRing, 0) => {
                let n = (self.max_hp as f32 * 0.25).round() as i32;
                self.heal(n);
                format!("Sweet and warm. Healed {n}.")
            }
            (EventId::MushroomRing, 1) => {
                self.max_hp += 6;
                self.hp = (self.hp - 6).max(1);
                "Bitter! +6 max HP, -6 HP.".into()
            }
            (EventId::StarFountain, 0) => {
                self.coins -= 30;
                let idx: Vec<usize> = (0..self.deck.len()).filter(|i| self.deck[*i].can_upgrade()).collect();
                let k = idx[self.rng.below(idx.len() as u32) as usize];
                self.deck[k].up = true;
                format!("The water glows. {} is upgraded.", self.deck[k].name())
            }
            (EventId::StarFountain, 1) => {
                self.heal(10);
                self.deck.push(Card::new(card_id("bruise")));
                "Refreshing, but it leaves a Bruise. Healed 10.".into()
            }
            (EventId::CannonAlcove, 0) => {
                self.hp = (self.hp - 12).max(1);
                let pool = reward_pool(Rarity::Rare);
                let c = Card::new(pool[self.rng.below(pool.len() as u32) as usize]);
                self.deck.push(c);
                format!("BOOM! -12 HP. On the ledge: {}.", c.name())
            }
            (EventId::CannonAlcove, 1) => {
                self.gain_coins(40);
                self.hp = (self.hp - 5).max(1);
                "Scrap sells well: +40 coins, -5 HP.".into()
            }
            _ => "You walk on.".into(),
        };
        self.event_result = Some(msg);
        true
    }

    pub fn leave_event(&mut self) {
        self.event = None;
        self.event_result = None;
        self.node_done();
    }

    // Treasure ------------------------------------------------------------------------------

    pub fn take_treasure(&mut self) {
        if let Some(r) = self.treasure.take() {
            self.add_relic(r);
        }
        let g = 25 + self.rng.below(16) as i32;
        self.gain_coins(g);
        self.node_done();
    }

    // Potions outside combat ---------------------------------------------------------------

    pub fn drink_tonic(&mut self, slot: usize) -> bool {
        if self.potions.get(slot).copied().flatten() == Some(Potion::Tonic) && self.screen != Screen::Combat {
            self.potions[slot] = None;
            self.heal(15);
            true
        } else {
            false
        }
    }

    pub fn discard_potion(&mut self, slot: usize) {
        if slot < POTION_SLOTS && self.screen != Screen::Combat {
            self.potions[slot] = None;
        }
    }

    /// Score shown on the summary.
    pub fn score(&self) -> i32 {
        let s = &self.stats;
        let boss = (self.screen == Screen::Won) as i32 * 250;
        s.nodes_cleared as i32 * 15 + s.elites as i32 * 25 + s.battles as i32 * 10 + s.best_chain * 5 + self.coins / 2 + self.hp + boss
    }

    pub fn won(&self) -> bool {
        self.screen == Screen::Won
    }
    pub fn lost(&self) -> bool {
        self.screen == Screen::Lost
    }
    pub fn ended(&self) -> bool {
        self.won() || self.lost()
    }
    /// How deep the run got (1-based stops cleared).
    pub fn depth(&self) -> u32 {
        self.stats.nodes_cleared + (self.screen == Screen::Combat) as u32
    }

    /// A simple automatic policy used by tests and `JUMPMAN_DECK_AUTOPLAY`: take the first safe route, add the
    /// first reward card, heal at rest, buy nothing.
    pub fn bot_act(&mut self) {
        match self.screen {
            Screen::Map => {
                let av = self.available();
                // Prefer rest/shop/battle over elites when hurt.
                let pick = av.iter().copied().min_by_key(|id| {
                    let k = self.map.nodes[*id].kind;
                    let weight = match k {
                        NodeKind::Rest => 0,
                        NodeKind::Treasure => 1,
                        NodeKind::Event => 2,
                        NodeKind::Shop => 3,
                        NodeKind::Battle => 4,
                        NodeKind::Elite => 6,
                        NodeKind::Boss => 0,
                    };
                    weight + if self.hp * 2 < self.max_hp && k == NodeKind::Elite { 10 } else { 0 }
                });
                if let Some(id) = pick {
                    let _ = self.enter_node(id);
                }
            }
            Screen::Combat => {
                if let Some(c) = self.combat.as_mut() {
                    c.bot_fight(80);
                }
                self.finish_combat();
            }
            Screen::Reward => {
                let pick = self.reward.as_ref().and_then(|r| {
                    r.cards.iter().position(|c| matches!(c.def().kind, Kind::Attack) && c.def().rarity != Rarity::Common).or_else(|| r.cards.iter().position(|c| c.def().kind != Kind::Power && c.def().cost <= 1))
                });
                match pick {
                    Some(i) => {
                        self.take_card(i);
                    }
                    None => self.skip_card(),
                }
                self.take_potion();
                self.leave_reward();
            }
            Screen::Shop => {
                // Spend on the cheapest relic or removal of a Bruise/punch.
                for i in 0..self.shop.as_ref().map_or(0, |s| s.relics.len()) {
                    self.buy_relic(i);
                }
                self.leave_shop();
            }
            Screen::Rest => {
                if self.hp * 10 < self.max_hp * 7 {
                    self.rest_heal();
                } else if let Some(i) = (0..self.deck.len()).find(|i| self.deck[*i].can_upgrade() && self.deck[*i].def().rarity != Rarity::Starter).or_else(|| (0..self.deck.len()).find(|i| self.deck[*i].can_upgrade())) {
                    self.rest_upgrade(i);
                }
                self.leave_rest();
            }
            Screen::Event => {
                let last = self.event.map_or(0, |e| e.options().len() - 1);
                self.event_choose(last);
                self.leave_event();
            }
            Screen::Treasure => self.take_treasure(),
            Screen::Won | Screen::Lost => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_are_connected_and_varied() {
        let mut kinds_seen = std::collections::HashSet::new();
        for seed in 0..300u64 {
            let mut rng = Rng::new(seed);
            let m = Map::generate(&mut rng);
            let total = ROW_WIDTHS.iter().sum::<usize>() + 1;
            assert_eq!(m.nodes.len(), total);
            assert!((14..=16).contains(&total));
            let reach = m.reachable();
            for n in &m.nodes {
                assert!(reach[n.id], "seed {seed}: node {} unreachable", n.id);
                assert!(m.reaches_boss(n.id), "seed {seed}: node {} is a dead end", n.id);
                if n.row < BOSS_ROW {
                    assert!(!n.next.is_empty() && n.next.len() <= 2);
                    for t in &n.next {
                        assert_eq!(m.nodes[*t].row, n.row + 1);
                    }
                }
                kinds_seen.insert(n.kind);
            }
            assert!(m.rows[0].iter().all(|id| m.nodes[*id].kind == NodeKind::Battle), "the first row is all battles");
            assert_eq!(m.nodes[m.boss()].kind, NodeKind::Boss);
            for k in [NodeKind::Elite, NodeKind::Shop, NodeKind::Rest, NodeKind::Event, NodeKind::Treasure] {
                assert!(m.nodes.iter().any(|n| n.kind == k), "seed {seed}: no {k:?}");
            }
            assert!(m.nodes.iter().all(|n| n.kind != NodeKind::Boss || n.id == m.boss()));
            let pre = &m.rows[BOSS_ROW - 1];
            assert!(pre.iter().any(|id| m.nodes[*id].kind == NodeKind::Rest));
        }
        assert_eq!(kinds_seen.len(), 7);
    }

    #[test]
    fn maps_are_deterministic() {
        let a = Map::generate(&mut Rng::new(42));
        let b = Map::generate(&mut Rng::new(42));
        let sig = |m: &Map| m.nodes.iter().map(|n| (n.kind, n.next.clone())).collect::<Vec<_>>();
        assert_eq!(sig(&a), sig(&b));
        let c = Map::generate(&mut Rng::new(43));
        assert_ne!(sig(&a), sig(&c));
    }

    #[test]
    fn reward_rarity_weights_hold() {
        let mut rng = Rng::new(7);
        let mut counts = [0u32; 3];
        let n = 20000;
        for _ in 0..n {
            match roll_rarity(rarity_weights(NodeKind::Battle), &mut rng) {
                Rarity::Common => counts[0] += 1,
                Rarity::Uncommon => counts[1] += 1,
                _ => counts[2] += 1,
            }
        }
        let f = |c: u32| c as f32 / n as f32;
        assert!((f(counts[0]) - 0.60).abs() < 0.02, "{counts:?}");
        assert!((f(counts[1]) - 0.33).abs() < 0.02);
        assert!((f(counts[2]) - 0.07).abs() < 0.015);
        // Elites are richer.
        let mut rare = 0;
        for _ in 0..n {
            rare += (roll_rarity(rarity_weights(NodeKind::Elite), &mut rng) == Rarity::Rare) as u32;
        }
        assert!(f(rare) > 0.12);
    }

    #[test]
    fn card_offers_are_distinct_and_valid() {
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let offer = roll_cards(3, NodeKind::Battle, &mut rng);
            assert_eq!(offer.len(), 3);
            assert!(offer[0].id != offer[1].id && offer[1].id != offer[2].id && offer[0].id != offer[2].id);
            assert!(offer.iter().all(|c| !c.up && matches!(c.def().rarity, Rarity::Common | Rarity::Uncommon | Rarity::Rare)));
        }
    }

    #[test]
    fn shop_prices_scale_with_rarity() {
        let mut rng = Rng::new(1);
        let price = |k: &str, rng: &mut Rng| card_price(Card::new(card_id(k)), rng);
        for _ in 0..50 {
            let (c, u, r) = (price("cap_throw", &mut rng), price("twirl", &mut rng), price("mega_quake", &mut rng));
            assert!((40..=55).contains(&c), "{c}");
            assert!((67..=91).contains(&u), "{u}");
            assert!((135..=181).contains(&r), "{r}");
            assert!(c < u && u < r);
        }
        assert_eq!(remove_price(0), 75);
        assert_eq!(remove_price(2), 125);
    }

    #[test]
    fn shop_buying_rules() {
        let mut run = Run::new(3, Hero::Mario);
        run.pos = Some(0);
        run.screen = Screen::Map;
        run.shop = Some(run.stock_shop());
        run.screen = Screen::Shop;
        let shop = run.shop.clone().unwrap();
        assert_eq!(shop.cards.len(), 5);
        assert!(!shop.relics.is_empty());
        run.coins = 0;
        assert!(!run.buy_card(0), "cannot afford");
        run.coins = 1000;
        let deck = run.deck.len();
        assert!(run.buy_card(0));
        assert!(!run.buy_card(0), "already sold");
        assert_eq!(run.deck.len(), deck + 1);
        assert_eq!(run.coins, 1000 - shop.cards[0].1);
        assert!(run.buy_relic(0));
        assert!(run.has(shop.relics[0].0));
        // Removal: once per visit, costs 75.
        let before = run.coins;
        assert!(run.remove_card(0));
        assert_eq!(run.coins, before - 75);
        assert!(!run.remove_card(0));
        // Potion slots fill up.
        run.potions = [Some(Potion::Tonic); 3];
        assert!(!run.buy_potion(0), "no room");
    }

    #[test]
    fn rest_site_heals_or_upgrades_once() {
        let mut run = Run::new(3, Hero::Mario);
        run.screen = Screen::Rest;
        run.hp = 10;
        assert!(run.rest_heal());
        assert_eq!(run.hp, 10 + (72.0f32 * 0.3).round() as i32);
        assert!(!run.rest_heal() && !run.rest_upgrade(0), "one action per campfire");
        run.rested = false;
        assert!(run.rest_upgrade(0));
        assert!(run.deck[0].up);
        run.rested = false;
        assert!(!run.rest_upgrade(0), "already upgraded");
    }

    #[test]
    fn every_event_option_resolves() {
        for ev in EVENTS {
            assert!(ev.options().len() >= 2);
            for i in 0..ev.options().len() {
                let mut run = Run::new(9, Hero::Link);
                run.coins = 100;
                run.hp = 40;
                run.event = Some(ev);
                run.screen = Screen::Event;
                let deck = run.deck.len();
                assert!(run.event_choose(i), "{ev:?} option {i}");
                assert!(run.event_result.is_some());
                assert!(!run.event_choose(i), "only one choice per event");
                assert!(run.hp >= 1 && run.deck.len() >= deck);
                run.pos = Some(0);
                run.leave_event();
                assert_eq!(run.screen, Screen::Map);
            }
        }
        // A fountain needs the coins.
        let mut run = Run::new(1, Hero::Link);
        run.coins = 5;
        run.event = Some(EventId::StarFountain);
        assert!(!run.event_enabled(0) && run.event_enabled(1));
        let count = EVENTS.len();
        assert!((3..=5).contains(&count));
    }

    #[test]
    fn rewards_and_flow_through_a_battle() {
        let mut run = Run::new(21, Hero::Mario);
        let first = run.map.rows[0][0];
        run.enter_node(first).unwrap();
        assert_eq!(run.screen, Screen::Combat);
        assert!(run.enter_node(first).is_err(), "cannot walk during a fight");
        run.combat.as_mut().unwrap().bot_fight(60);
        run.finish_combat();
        assert_eq!(run.screen, Screen::Reward);
        let r = run.reward.clone().unwrap();
        assert_eq!(r.cards.len(), 3);
        let coins = run.coins;
        assert!(run.take_card(1));
        assert!(!run.take_card(0), "pick one of three");
        run.leave_reward();
        assert_eq!(run.screen, Screen::Map);
        assert_eq!(run.coins, coins + r.gold);
        assert_eq!(run.deck.len(), 11);
        assert_eq!(run.cleared, vec![first]);
        assert!(run.available().iter().all(|n| run.map.nodes[first].next.contains(n)));
    }

    #[test]
    fn relic_pickups_apply() {
        let mut run = Run::new(1, Hero::Banjo);
        run.add_relic(Relic::BigMushroom);
        let big = Hero::Banjo.max_hp() + 12;
        assert_eq!((run.max_hp, run.hp), (big, big));
        run.add_relic(Relic::CoinPouch);
        assert_eq!(run.coins, 90);
        run.add_relic(Relic::CoinPouch);
        assert_eq!(run.coins, 90, "relics do not stack");
    }

    #[test]
    fn full_runs_are_deterministic_and_terminate() {
        let run_once = |seed: u64, hero: Hero| {
            let mut run = Run::new(seed, hero);
            let mut steps = 0;
            while !run.ended() && steps < 400 {
                run.bot_act();
                steps += 1;
            }
            assert!(run.ended(), "seed {seed} did not finish");
            (run.screen, run.stats, run.hp, run.coins, run.deck.len(), run.relics.clone(), run.score())
        };
        let mut wins = 0;
        let mut finished_depth = 0;
        for seed in 0..12u64 {
            let hero = Hero::ALL[seed as usize % 4];
            let a = run_once(seed, hero);
            let b = run_once(seed, hero);
            assert_eq!(a, b, "same seed, same run");
            wins += (a.0 == Screen::Won) as u32;
            finished_depth += a.1.nodes_cleared;
        }
        assert!(finished_depth >= 12 * 3, "the bot gets somewhere ({finished_depth})");
        assert!(wins <= 12);
    }

    /// The four heroes stay within a healthy band for the policy bot (a lopsided hero spread was a known problem).
    #[test]
    fn every_hero_has_a_healthy_bot_win_rate() {
        let n = 600u64;
        let mut rates = [0.0f64; 4];
        let mut wins = [0u32; 4];
        for seed in 0..n {
            let h = (seed % 4) as usize;
            let mut run = Run::new(seed, Hero::ALL[h]);
            let mut steps = 0;
            while !run.ended() && steps < 400 {
                run.bot_act();
                steps += 1;
            }
            assert!(run.ended(), "{:?} seed {seed} never ended", Hero::ALL[h]);
            wins[h] += run.won() as u32;
        }
        for h in 0..4 {
            rates[h] = wins[h] as f64 / (n / 4) as f64;
        }
        eprintln!("hero win rates: {rates:?}");
        for (h, r) in rates.iter().enumerate() {
            assert!((0.33..=0.67).contains(r), "{:?} wins {:.0}% (all: {rates:?})", Hero::ALL[h], r * 100.0);
        }
    }

    /// `cargo test --release deck_run::tests::win_rate_report -- --ignored --nocapture`: the policy bot's results over many seeds.
    #[test]
    #[ignore]
    fn win_rate_report() {
        let n = 1200u64;
        let mut per_hero = [(0u32, 0u32); 4];
        let mut depth = 0u32;
        for seed in 0..n {
            let h = (seed % 4) as usize;
            let mut run = Run::new(seed, Hero::ALL[h]);
            let mut steps = 0;
            while !run.ended() && steps < 400 {
                run.bot_act();
                steps += 1;
            }
            per_hero[h].0 += run.won() as u32;
            per_hero[h].1 += 1;
            depth += run.stats.nodes_cleared;
        }
        let wins: u32 = per_hero.iter().map(|p| p.0).sum();
        println!("star deck bot: {wins}/{n} wins ({:.1}%), avg stops cleared {:.2}", wins as f64 * 100.0 / n as f64, depth as f64 / n as f64);
        for (i, p) in per_hero.iter().enumerate() {
            println!("  hero {i}: {}/{} ({:.1}%)", p.0, p.1, p.0 as f64 * 100.0 / p.1 as f64);
        }
    }

    #[test]
    fn boss_victory_ends_the_run_with_a_score() {
        let mut run = Run::new(4, Hero::Mario);
        let boss = run.map.boss();
        run.pos = Some(run.map.rows[BOSS_ROW - 1][0]);
        run.enter_node(boss).unwrap();
        let c = run.combat.as_mut().unwrap();
        c.foes[0].hp = 1;
        c.hand.clear();
        c.hand.push(Inst { card: Card::new(card_id("punch")), uid: 77 });
        c.play(0, Some(0)).unwrap();
        assert_eq!(c.outcome, Outcome::Won);
        run.finish_combat();
        assert_eq!(run.screen, Screen::Reward);
        run.leave_reward();
        assert!(run.won());
        assert!(run.score() > 250);
    }

    #[test]
    fn losing_ends_the_run() {
        let mut run = Run::new(4, Hero::Mario);
        let n = run.map.rows[0][0];
        run.enter_node(n).unwrap();
        run.combat.as_mut().unwrap().hp = 0;
        run.combat.as_mut().unwrap().outcome = Outcome::Lost;
        run.finish_combat();
        assert!(run.lost() && run.ended());
    }

    #[test]
    fn encounters_scale_with_depth() {
        let mut rng = Rng::new(1);
        for _ in 0..100 {
            let e = encounter(NodeKind::Battle, 0, &mut rng);
            assert!((1..=3).contains(&e.len()));
            assert!(e.iter().all(|k| !k.info().elite && !k.info().boss));
        }
        assert!(encounter(NodeKind::Elite, 3, &mut rng)[0].info().elite);
        assert_eq!(encounter(NodeKind::Boss, 6, &mut rng), vec![EnemyKind::FuseKing]);
        let used: std::collections::HashSet<_> = (0..400).flat_map(|i| encounter(NodeKind::Battle, 1 + i % 5, &mut rng)).collect();
        assert!(used.len() >= 10, "ten regular enemy types appear: {}", used.len());
    }
}
