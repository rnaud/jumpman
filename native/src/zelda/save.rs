//! The Zelda part of `save.ts` (village wallet, treasure chests, slingshot, Deku dungeon flags)
//! plus `zeldaTreasure.ts`. Persistence is left to whoever ports the rest of the save file; this
//! is the in-memory model with the same rules.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreasureReward {
    pub name: &'static str,
    pub rupees: u32,
    pub heal: bool,
    /// 1: a small key, 2: the boss key (dungeon chests, `GI_KEY_SMALL` 0x42 / `GI_KEY_BOSS` 0x3F).
    pub key: u8,
}

/// `treasureRewards` (item id -> reward).
pub fn treasure_reward(item: u32) -> Option<TreasureReward> {
    let r = |name, rupees, heal| Some(TreasureReward { name, rupees, heal, key: 0 });
    match item {
        0x05 => r("Fairy Slingshot", 0, false),
        0x1f => r("Odd Mushroom", 0, false),
        0x2b => r("Mirror Shield", 0, false),
        0x35 => r("Silver Gauntlets", 0, false),
        0x3e => r("Piece of Heart", 0, false),
        0x3f => Some(TreasureReward { name: "Boss Key", rupees: 0, heal: false, key: 2 }),
        0x42 => Some(TreasureReward { name: "Small Key", rupees: 0, heal: false, key: 1 }),
        0x40 => r("Compass", 0, false),
        0x41 => r("Dungeon Map", 0, false),
        0x48 => r("Recovery Heart", 0, true),
        0x4c => r("Green Rupee", 1, false),
        0x4d => r("Blue Rupee", 5, false),
        0x55 => r("Purple Rupee", 50, false),
        0x56 => r("Huge Rupee", 200, false),
        _ => None,
    }
}

/// `warpDestinations`: (course id, label).
pub const WARP_DESTINATIONS: [(&str, &str); 5] = [
    ("sacred_forest_meadow", "Sacred Forest Meadow"),
    ("death_mountain_crater", "Death Mountain Crater"),
    ("lake_hylia", "Lake Hylia"),
    ("desert_colossus", "Desert Colossus"),
    ("graveyard", "Graveyard"),
];

pub const MAX_RUPEES: u32 = 99;
pub const MAX_SEEDS: u32 = 30;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[derive(bevy::prelude::Resource)]
pub struct ZeldaSave {
    pub rupees: u32,
    pub shield: bool,
    pub sword: bool,
    pub deku_path_open: bool,
    pub treasure: BTreeMap<String, u32>,
    pub collected: BTreeSet<String>,
    pub spoken: BTreeSet<String>,
    pub equipped: bool,
    seeds: u32,
    pub flags: BTreeSet<String>,
}

impl Default for ZeldaSave {
    fn default() -> Self {
        Self {
            rupees: 0,
            shield: false,
            sword: false,
            deku_path_open: false,
            treasure: BTreeMap::new(),
            collected: BTreeSet::new(),
            spoken: BTreeSet::new(),
            equipped: false,
            seeds: MAX_SEEDS,
            flags: BTreeSet::new(),
        }
    }
}



impl ZeldaSave {
    pub fn has_spoken_to(&self, id: &str) -> bool {
        self.spoken.contains(id)
    }
    pub fn finish_conversation(&mut self, id: &str) {
        self.spoken.insert(id.to_string());
    }
    pub fn chest_opened(&self, key: &str) -> bool {
        self.treasure.contains_key(key)
    }
    pub fn open_treasure_chest(&mut self, key: &str, item: u32) -> bool {
        let Some(reward) = treasure_reward(item) else { return false };
        if self.chest_opened(key) {
            return false;
        }
        self.treasure.insert(key.to_string(), item);
        self.rupees = (self.rupees + reward.rupees).min(MAX_RUPEES);
        true
    }
    pub fn has_slingshot(&self) -> bool {
        self.chest_opened("deku_tree:1")
    }
    pub fn slingshot_equipped(&self) -> bool {
        self.has_slingshot() && self.equipped
    }
    pub fn deku_seeds(&self) -> u32 {
        if self.has_slingshot() { self.seeds } else { 0 }
    }
    pub fn set_slingshot_equipped(&mut self, equipped: bool) {
        if self.has_slingshot() {
            self.equipped = equipped;
        }
    }
    pub fn use_seed(&mut self) -> bool {
        if !self.slingshot_equipped() || self.deku_seeds() == 0 {
            return false;
        }
        self.seeds -= 1;
        true
    }
    pub fn refill_seeds(&mut self) {
        if self.has_slingshot() && self.deku_seeds() != MAX_SEEDS {
            self.seeds = MAX_SEEDS;
        }
    }
    pub fn deku_cleared(&self, flag: &str) -> bool {
        self.flags.contains(flag)
    }
    pub fn clear_deku_challenge(&mut self, flag: &str) {
        self.flags.insert(flag.to_string());
    }
    /// Named key items (not rupees or hearts), in chest-key order.
    pub fn treasure_inventory(&self) -> Vec<&'static str> {
        self.treasure.values().filter_map(|&i| treasure_reward(i)).filter(|r| r.rupees == 0 && !r.heal).map(|r| r.name).collect()
    }
    pub fn collect_kokiri_sword(&mut self) -> bool {
        if self.sword {
            return false;
        }
        self.sword = true;
        true
    }
    pub fn open_deku_path(&mut self) -> bool {
        if !self.sword || !self.shield || self.deku_path_open {
            return false;
        }
        self.deku_path_open = true;
        true
    }
    pub fn has_village_pickup(&self, key: &str) -> bool {
        self.collected.contains(key)
    }
    pub fn collect_village_rupees(&mut self, key: &str, value: u32, permanent: bool) -> bool {
        if self.rupees >= MAX_RUPEES || self.has_village_pickup(key) {
            return false;
        }
        self.rupees = (self.rupees + value).min(MAX_RUPEES);
        if permanent {
            self.collected.insert(key.to_string());
        }
        true
    }
    /// Shop purchase; shield can only be bought once.
    pub fn buy(&mut self, item: &str, cost: u32) -> bool {
        if self.rupees < cost || (item == "shield" && self.shield) {
            return false;
        }
        self.rupees -= cost;
        if item == "shield" {
            self.shield = true;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chests_pay_once_and_cap_at_99() {
        let mut s = ZeldaSave::default();
        assert!(s.open_treasure_chest("a", 0x56));
        assert_eq!(s.rupees, 99);
        assert!(!s.open_treasure_chest("a", 0x4c));
        assert!(!s.open_treasure_chest("b", 0x99), "unknown item");
        assert!(s.open_treasure_chest("deku_tree:1", 0x05));
        assert_eq!(s.treasure_inventory(), vec!["Fairy Slingshot"]);
    }

    #[test]
    fn slingshot_needs_the_chest_and_equipping() {
        let mut s = ZeldaSave::default();
        assert_eq!(s.deku_seeds(), 0);
        assert!(!s.use_seed());
        s.open_treasure_chest("deku_tree:1", 0x05);
        assert!(!s.use_seed(), "not equipped");
        s.set_slingshot_equipped(true);
        for _ in 0..30 {
            assert!(s.use_seed());
        }
        assert!(!s.use_seed());
        s.refill_seeds();
        assert_eq!(s.deku_seeds(), 30);
    }

    #[test]
    fn deku_path_needs_sword_and_shield() {
        let mut s = ZeldaSave::default();
        assert!(!s.open_deku_path());
        s.collect_kokiri_sword();
        s.rupees = 40;
        assert!(s.buy("shield", 40));
        assert!(!s.buy("shield", 0));
        assert!(s.open_deku_path());
        assert!(!s.open_deku_path());
    }
}
