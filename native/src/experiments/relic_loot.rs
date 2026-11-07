//! Relic Descent loot: items, rarity tiers, affix rolls, item-level scaling, equipment totals and
//! the vendor value. Engine-free and seeded (the [`Rng`] is Star Swarm's splitmix64), so a drop
//! is a pure function of the generator state.
//!
//! Every name here is invented for this game: base item names and the six legendary effects.
use super::swarm_core::Rng;

pub const MAX_AFFIX: usize = 4;
pub const EQUIP_SLOTS: usize = 5;
pub const BAG_CAP: usize = 24;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum Rarity {
    Common = 0,
    Uncommon = 1,
    Rare = 2,
    Epic = 3,
    Legendary = 4,
}

pub const RARITIES: [Rarity; 5] = [Rarity::Common, Rarity::Uncommon, Rarity::Rare, Rarity::Epic, Rarity::Legendary];

impl Rarity {
    pub fn name(self) -> &'static str {
        ["COMMON", "UNCOMMON", "RARE", "EPIC", "LEGENDARY"][self as usize]
    }
    /// Label colour (sRGB bytes): grey, green, blue, violet, orange.
    pub fn color(self) -> [u8; 3] {
        [[0xc8, 0xc8, 0xc8], [0x5c, 0xe0, 0x5c], [0x4d, 0x9b, 0xff], [0xc0, 0x6c, 0xff], [0xff, 0x9a, 0x2a]][self as usize]
    }
    fn quality(self) -> f32 {
        [1.0, 1.0, 1.1, 1.25, 1.45][self as usize]
    }
    fn sell_mult(self) -> u32 {
        [1, 2, 5, 12, 40][self as usize]
    }
    fn affix_range(self) -> (u32, u32) {
        match self {
            Rarity::Common => (0, 1),
            Rarity::Uncommon => (1, 2),
            Rarity::Rare => (2, 3),
            Rarity::Epic => (3, 3),
            Rarity::Legendary => (3, 4),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Slot {
    Weapon = 0,
    Armor = 1,
    Boots = 2,
    Charm = 3,
}

impl Slot {
    pub fn name(self) -> &'static str {
        ["WEAPON", "ARMOR", "BOOTS", "CHARM"][self as usize]
    }
}

/// What an equipment slot index (0..5) holds: weapon, armor, boots, charm, charm.
pub fn equip_slot_kind(i: usize) -> Slot {
    match i {
        0 => Slot::Weapon,
        1 => Slot::Armor,
        2 => Slot::Boots,
        _ => Slot::Charm,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Stat {
    Damage = 0,
    CritChance,
    AttackSpeed,
    MoveSpeed,
    LifeSteal,
    MaxHp,
    CooldownReduction,
    DodgeFrames,
    PickupRadius,
    Thorns,
}

pub const STAT_COUNT: usize = 10;
pub const ALL_STATS: [Stat; STAT_COUNT] = [Stat::Damage, Stat::CritChance, Stat::AttackSpeed, Stat::MoveSpeed, Stat::LifeSteal, Stat::MaxHp, Stat::CooldownReduction, Stat::DodgeFrames, Stat::PickupRadius, Stat::Thorns];

impl Stat {
    pub fn label(self) -> &'static str {
        ["Damage", "Crit chance", "Attack speed", "Move speed", "Life steal", "Max health", "Cooldown cut", "Dodge frames", "Pickup radius", "Thorns"][self as usize]
    }
    /// Percent stats are shown with a % sign.
    pub fn is_percent(self) -> bool {
        !matches!(self, Stat::Damage | Stat::MaxHp | Stat::DodgeFrames | Stat::Thorns)
    }
    /// Roll range at item level 1: (min, max).
    fn range(self) -> (f32, f32) {
        match self {
            Stat::Damage => (2.0, 5.0),
            Stat::CritChance => (2.0, 6.0),
            Stat::AttackSpeed => (4.0, 10.0),
            Stat::MoveSpeed => (3.0, 8.0),
            Stat::LifeSteal => (1.0, 3.0),
            Stat::MaxHp => (8.0, 20.0),
            Stat::CooldownReduction => (3.0, 8.0),
            Stat::DodgeFrames => (1.0, 2.0),
            Stat::PickupRadius => (10.0, 30.0),
            Stat::Thorns => (2.0, 5.0),
        }
    }
    /// How fast the value grows with item level (percentages and tick counts grow slower).
    fn growth(self) -> f32 {
        match self {
            Stat::Damage | Stat::MaxHp | Stat::Thorns => 0.12,
            Stat::DodgeFrames => 0.04,
            _ => 0.05,
        }
    }
    /// Value in "power score" units per point, to compare items.
    fn weight(self) -> f32 {
        match self {
            Stat::Damage => 3.0,
            Stat::CritChance => 1.6,
            Stat::AttackSpeed => 1.2,
            Stat::MoveSpeed => 0.8,
            Stat::LifeSteal => 3.0,
            Stat::MaxHp => 0.5,
            Stat::CooldownReduction => 1.2,
            Stat::DodgeFrames => 5.0,
            Stat::PickupRadius => 0.15,
            Stat::Thorns => 1.5,
        }
    }
    fn allowed(self, slot: Slot) -> bool {
        use Stat::*;
        match slot {
            Slot::Weapon => matches!(self, Damage | CritChance | AttackSpeed | LifeSteal | CooldownReduction),
            Slot::Armor => matches!(self, MaxHp | Thorns | LifeSteal | CooldownReduction | PickupRadius),
            Slot::Boots => matches!(self, MoveSpeed | DodgeFrames | MaxHp | PickupRadius | AttackSpeed),
            Slot::Charm => true,
        }
    }
}

/// Multiplier on affix values and base stats at an item level (1 = the first floor's low end).
pub fn ilvl_scale(ilvl: u32, growth: f32) -> f32 {
    1.0 + growth * (ilvl.saturating_sub(1)) as f32
}

/// Item level of drops on `floor` (1-based) for a character of `level`.
pub fn item_level(floor: u32, level: u32) -> u32 {
    floor.max(1) * 4 + level / 2
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Affix {
    pub stat: Stat,
    pub value: f32,
}

/// Six unique effects; only legendary items carry one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Legendary {
    /// Every 4th swing also launches a wave of force.
    Echo = 0,
    /// Ground-pound shockwaves are wider and hit harder.
    Tremor,
    /// Thorns hit twice as hard.
    Briar,
    /// Dodges recharge twice as fast and leave you invulnerable longer.
    Slipstream,
    /// Once per floor, a killing blow leaves you at 30% health instead.
    Lastlight,
    /// More gold, and potions drop more often.
    Magpie,
}

pub const LEGENDARIES: [Legendary; 6] = [Legendary::Echo, Legendary::Tremor, Legendary::Briar, Legendary::Slipstream, Legendary::Lastlight, Legendary::Magpie];

impl Legendary {
    pub fn item_name(self) -> &'static str {
        ["Sunsplit Blade", "Quakebrand", "Mantle of Ten Thorns", "Skiff Treads", "Lantern of Second Chances", "Magpie's Knot"][self as usize]
    }
    pub fn slot(self) -> Slot {
        match self {
            Legendary::Echo | Legendary::Tremor => Slot::Weapon,
            Legendary::Briar => Slot::Armor,
            Legendary::Slipstream => Slot::Boots,
            Legendary::Lastlight | Legendary::Magpie => Slot::Charm,
        }
    }
    pub fn text(self) -> &'static str {
        match self {
            Legendary::Echo => "Every 4th swing sends out a wave of force.",
            Legendary::Tremor => "Ground pounds hit 40% harder over a wider ring.",
            Legendary::Briar => "Thorns deal double damage.",
            Legendary::Slipstream => "Dodge recharges twice as fast; +4 dodge frames.",
            Legendary::Lastlight => "Once per floor, a killing blow leaves you at 30% health.",
            Legendary::Magpie => "+50% gold, and potions drop more often.",
        }
    }
}

const WEAPONS: [&str; 4] = ["Cudgel", "Gale Blade", "Hollow Spike", "Tidal Mace"];
const ARMORS: [&str; 4] = ["Reed Jerkin", "Lantern Plate", "Moss Cloak", "Slate Coat"];
const BOOTS: [&str; 4] = ["Hopper Boots", "Dune Sandals", "Ember Greaves", "Skimmers"];
const CHARMS: [&str; 4] = ["Tin Locket", "Glass Bead", "Bone Whistle", "Copper Knot"];
const PREFIX: [&str; 5] = ["Worn", "Sturdy", "Tempered", "Exalted", "Mythic"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Item {
    /// Unique within a run; lets the UI refer to bag entries without index races.
    pub id: u32,
    pub slot: Slot,
    pub rarity: Rarity,
    pub ilvl: u16,
    pub base_idx: u8,
    /// Weapon damage / armor defence / boots move-speed % / charm max-spark.
    pub base: f32,
    pub n: u8,
    pub affixes: [Affix; MAX_AFFIX],
    pub legendary: Option<Legendary>,
}

impl Item {
    pub fn affixes(&self) -> &[Affix] {
        &self.affixes[..self.n as usize]
    }
    pub fn name(&self) -> String {
        if let Some(l) = self.legendary {
            return l.item_name().to_string();
        }
        let base = match self.slot {
            Slot::Weapon => WEAPONS,
            Slot::Armor => ARMORS,
            Slot::Boots => BOOTS,
            Slot::Charm => CHARMS,
        }[self.base_idx as usize % 4];
        format!("{} {}", PREFIX[self.rarity as usize], base)
    }
    pub fn base_label(&self) -> &'static str {
        match self.slot {
            Slot::Weapon => "Damage",
            Slot::Armor => "Defence",
            Slot::Boots => "Move speed",
            Slot::Charm => "Max spark",
        }
    }
    pub fn base_text(&self) -> String {
        match self.slot {
            Slot::Boots => format!("{} +{:.0}%", self.base_label(), self.base),
            _ => format!("{} +{:.0}", self.base_label(), self.base),
        }
    }
    pub fn sell_value(&self) -> u32 {
        (6 + self.ilvl as u32 * 3) * self.rarity.sell_mult()
    }
    /// A single number for the "better / worse" arrow in the compare card.
    pub fn power(&self) -> f32 {
        let base = match self.slot {
            Slot::Weapon => self.base * 3.0,
            Slot::Armor => self.base * 1.2,
            Slot::Boots => self.base * 0.8,
            Slot::Charm => self.base * 0.4,
        };
        let aff: f32 = self.affixes().iter().map(|a| a.value * a.stat.weight()).sum();
        base + aff + if self.legendary.is_some() { 30.0 } else { 0.0 }
    }
}

pub fn fmt_value(stat: Stat, v: f32) -> String {
    if stat.is_percent() { format!("{:.1}%", v) } else { format!("{:.0}", v) }
}

// ---------------------------------------------------------------------------------------------
// Drops
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropSource {
    Normal,
    Elite,
    Chest,
    Boss,
}

/// Rarity weights out of 1000 for each tier (common .. legendary). Deeper floors lean up a little.
pub fn rarity_weights(source: DropSource, floor: u32) -> [f32; 5] {
    let mut w: [f32; 5] = match source {
        DropSource::Normal => [640.0, 250.0, 85.0, 20.0, 5.0],
        DropSource::Elite => [250.0, 400.0, 250.0, 80.0, 20.0],
        DropSource::Chest => [0.0, 520.0, 330.0, 120.0, 30.0],
        DropSource::Boss => [0.0, 0.0, 600.0, 300.0, 100.0],
    };
    let f = floor.saturating_sub(1) as f32;
    // Shift a few points per floor from the lowest populated tier into the top tiers.
    let low = w.iter().position(|x| *x > 0.0).unwrap_or(0);
    let shift = (12.0 * f).min(w[low] * 0.5);
    w[low] -= shift;
    w[2] += shift * 0.55;
    w[3] += shift * 0.33;
    w[4] += shift * 0.12;
    w
}

pub fn pick_rarity(rng: &mut Rng, source: DropSource, floor: u32) -> Rarity {
    let w = rarity_weights(source, floor);
    let total: f32 = w.iter().sum();
    let mut r = rng.f32() * total;
    for (i, x) in w.iter().enumerate() {
        if r < *x {
            return RARITIES[i];
        }
        r -= *x;
    }
    RARITIES[w.iter().rposition(|x| *x > 0.0).unwrap_or(0)]
}

fn pick_slot(rng: &mut Rng) -> Slot {
    match rng.below(100) {
        0..=24 => Slot::Weapon,
        25..=49 => Slot::Armor,
        50..=69 => Slot::Boots,
        _ => Slot::Charm,
    }
}

/// Roll an item of the given slot and rarity at an item level.
pub fn roll_item(rng: &mut Rng, id: u32, slot: Slot, rarity: Rarity, ilvl: u32) -> Item {
    let ilvl = ilvl.max(1);
    let legendary = if rarity == Rarity::Legendary {
        let options: Vec<Legendary> = LEGENDARIES.iter().copied().filter(|l| l.slot() == slot).collect();
        Some(options[rng.below(options.len() as u32) as usize])
    } else {
        None
    };
    let (lo, hi) = rarity.affix_range();
    let n = (lo + rng.below(hi - lo + 1)) as usize;
    let mut affixes = [Affix { stat: Stat::Damage, value: 0.0 }; MAX_AFFIX];
    let mut count = 0usize;
    while count < n {
        let stat = ALL_STATS[rng.below(STAT_COUNT as u32) as usize];
        if !stat.allowed(slot) || affixes[..count].iter().any(|a| a.stat == stat) {
            continue;
        }
        let (min, max) = stat.range();
        let mut value = (min + (max - min) * rng.f32()) * ilvl_scale(ilvl, stat.growth()) * rarity.quality();
        if !stat.is_percent() {
            value = value.round().max(1.0);
        } else {
            value = (value * 10.0).round() / 10.0;
        }
        affixes[count] = Affix { stat, value };
        count += 1;
    }
    let q = rarity.quality();
    let base = match slot {
        Slot::Weapon => (5.0 * ilvl_scale(ilvl, 0.14) * q).round(),
        Slot::Armor => (3.0 * ilvl_scale(ilvl, 0.14) * q).round(),
        Slot::Boots => (4.0 + (ilvl as f32 / 8.0).min(8.0) * q).round(),
        Slot::Charm => (10.0 * ilvl_scale(ilvl, 0.06) * q).round(),
    };
    Item { id, slot, rarity, ilvl: ilvl as u16, base_idx: rng.below(4) as u8, base, n: n as u8, affixes, legendary }
}

/// A full drop: slot, rarity (from the source's table) and affixes.
pub fn roll_drop(rng: &mut Rng, id: u32, source: DropSource, floor: u32, level: u32) -> Item {
    let rarity = pick_rarity(rng, source, floor);
    let slot = pick_slot(rng);
    roll_item(rng, id, slot, rarity, item_level(floor, level))
}

// ---------------------------------------------------------------------------------------------
// Equipment
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Equipment {
    pub slots: [Option<Item>; EQUIP_SLOTS],
}

/// Everything equipped, summed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    pub weapon_dmg: f32,
    pub defence: f32,
    pub boots_move: f32,
    pub max_spark: f32,
    pub stat: [f32; STAT_COUNT],
    pub legendary: [bool; 6],
}

impl Totals {
    pub fn get(&self, s: Stat) -> f32 {
        self.stat[s as usize]
    }
    pub fn has(&self, l: Legendary) -> bool {
        self.legendary[l as usize]
    }
}

impl Equipment {
    /// Which equipment slot an item of `slot` would take (an empty charm slot first, else the weaker charm).
    pub fn target(&self, slot: Slot) -> usize {
        match slot {
            Slot::Weapon => 0,
            Slot::Armor => 1,
            Slot::Boots => 2,
            Slot::Charm => {
                if self.slots[3].is_none() {
                    3
                } else if self.slots[4].is_none() {
                    4
                } else if self.slots[3].map_or(0.0, |c| c.power()) <= self.slots[4].map_or(0.0, |c| c.power()) {
                    3
                } else {
                    4
                }
            }
        }
    }
    /// Equip, returning whatever was in the way.
    pub fn equip(&mut self, item: Item) -> Option<Item> {
        let i = self.target(item.slot);
        self.slots[i].replace(item)
    }
    pub fn totals(&self) -> Totals {
        let mut t = Totals::default();
        for it in self.slots.iter().flatten() {
            match it.slot {
                Slot::Weapon => t.weapon_dmg += it.base,
                Slot::Armor => t.defence += it.base,
                Slot::Boots => t.boots_move += it.base,
                Slot::Charm => t.max_spark += it.base,
            }
            for a in it.affixes() {
                t.stat[a.stat as usize] += a.value;
            }
            if let Some(l) = it.legendary {
                t.legendary[l as usize] = true;
            }
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_weights_sum_and_monotone_by_source() {
        for f in 1..=3 {
            for s in [DropSource::Normal, DropSource::Elite, DropSource::Chest, DropSource::Boss] {
                let w = rarity_weights(s, f);
                assert!((w.iter().sum::<f32>() - 1000.0).abs() < 0.01, "{s:?} floor {f}: {w:?}");
                assert!(w.iter().all(|x| *x >= 0.0));
            }
        }
        let good = |s| rarity_weights(s, 1)[2..].iter().sum::<f32>();
        assert!(good(DropSource::Elite) > good(DropSource::Normal));
        assert!(good(DropSource::Chest) > good(DropSource::Elite));
        assert!(good(DropSource::Boss) > good(DropSource::Chest));
        assert!(rarity_weights(DropSource::Normal, 3)[4] > rarity_weights(DropSource::Normal, 1)[4]);
    }

    #[test]
    fn empirical_rarity_distribution_matches_the_table() {
        let mut rng = Rng::new(5);
        let n = 40_000;
        let mut counts = [0u32; 5];
        for _ in 0..n {
            counts[pick_rarity(&mut rng, DropSource::Normal, 1) as usize] += 1;
        }
        let w = rarity_weights(DropSource::Normal, 1);
        for i in 0..5 {
            let want = w[i] / 1000.0;
            let got = counts[i] as f32 / n as f32;
            assert!((got - want).abs() < 0.012, "tier {i}: {got} vs {want}");
        }
        for _ in 0..2000 {
            assert!(pick_rarity(&mut rng, DropSource::Chest, 1) >= Rarity::Uncommon);
            assert!(pick_rarity(&mut rng, DropSource::Boss, 2) >= Rarity::Rare);
        }
    }

    #[test]
    fn affix_counts_follow_rarity_and_never_repeat() {
        let mut rng = Rng::new(11);
        for r in RARITIES {
            let (lo, hi) = r.affix_range();
            for i in 0..500 {
                let slot = [Slot::Weapon, Slot::Armor, Slot::Boots, Slot::Charm][i % 4];
                let it = roll_item(&mut rng, i as u32, slot, r, 1 + (i as u32 % 30));
                assert!(it.n as u32 >= lo && it.n as u32 <= hi, "{r:?} {}", it.n);
                let list = it.affixes();
                for (a, x) in list.iter().enumerate() {
                    assert!(x.stat.allowed(slot), "{:?} on {slot:?}", x.stat);
                    assert!(x.value > 0.0);
                    assert!(list[a + 1..].iter().all(|y| y.stat != x.stat), "duplicate affix");
                }
                assert_eq!(it.legendary.is_some(), r == Rarity::Legendary);
                if let Some(l) = it.legendary {
                    assert_eq!(l.slot(), slot);
                }
            }
        }
    }

    #[test]
    fn item_level_scales_values_and_floors() {
        assert!(item_level(3, 5) > item_level(1, 5));
        assert!(ilvl_scale(20, 0.12) > ilvl_scale(5, 0.12));
        let avg = |ilvl: u32| {
            let mut rng = Rng::new(3);
            let mut sum = 0.0;
            for i in 0..400 {
                sum += roll_item(&mut rng, i, Slot::Weapon, Rarity::Rare, ilvl).base;
            }
            sum / 400.0
        };
        assert!(avg(24) > avg(4) * 1.5);
        let dmg = |ilvl: u32| {
            let mut rng = Rng::new(4);
            (0..300).map(|i| roll_item(&mut rng, i, Slot::Weapon, Rarity::Epic, ilvl)).flat_map(|it| it.affixes().iter().filter(|a| a.stat == Stat::Damage).map(|a| a.value).collect::<Vec<_>>()).sum::<f32>()
        };
        assert!(dmg(30) > dmg(2));
    }

    #[test]
    fn higher_rarity_means_more_power_on_average() {
        let mut rng = Rng::new(9);
        let mut avg = [0.0f32; 5];
        for (ri, r) in RARITIES.iter().enumerate() {
            for i in 0..300 {
                avg[ri] += roll_item(&mut rng, i, Slot::Charm, *r, 12).power();
            }
            avg[ri] /= 300.0;
        }
        for i in 1..5 {
            assert!(avg[i] > avg[i - 1], "{avg:?}");
        }
    }

    #[test]
    fn equipment_totals_and_charm_slots() {
        let mut rng = Rng::new(21);
        let mut eq = Equipment::default();
        let w = roll_item(&mut rng, 1, Slot::Weapon, Rarity::Rare, 8);
        assert!(eq.equip(w).is_none());
        let c1 = roll_item(&mut rng, 2, Slot::Charm, Rarity::Uncommon, 8);
        let c2 = roll_item(&mut rng, 3, Slot::Charm, Rarity::Uncommon, 8);
        eq.equip(c1);
        eq.equip(c2);
        assert!(eq.slots[3].is_some() && eq.slots[4].is_some());
        let c3 = roll_item(&mut rng, 4, Slot::Charm, Rarity::Epic, 8);
        let back = eq.equip(c3).expect("a charm is replaced");
        assert!(back.id == 2 || back.id == 3);
        let t = eq.totals();
        assert_eq!(t.weapon_dmg, w.base);
        let want: f32 = eq.slots.iter().flatten().flat_map(|i| i.affixes().iter()).filter(|a| a.stat == Stat::Damage).map(|a| a.value).sum();
        assert!((t.get(Stat::Damage) - want).abs() < 1e-3);
    }

    #[test]
    fn rolls_are_deterministic_per_seed_and_names_are_ascii() {
        let mut a = Rng::new(77);
        let mut b = Rng::new(77);
        for i in 0..50 {
            let x = roll_drop(&mut a, i, DropSource::Elite, 2, 6);
            let y = roll_drop(&mut b, i, DropSource::Elite, 2, 6);
            assert_eq!(x, y);
            assert!(x.name().is_ascii() && x.sell_value() > 0);
        }
        for l in LEGENDARIES {
            assert!(l.item_name().is_ascii() && l.text().is_ascii());
        }
    }
}
