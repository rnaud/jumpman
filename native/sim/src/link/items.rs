//! Link's inventory model (OoT `z64item.h`, `z_inventory.c`, `z_kaleido_scope.c` age tables,
//! `z_parameter.c` magic). Engine-free: the pause screen, the HUD and the item actions all read
//! and write this struct.
//!
//! Counts: 16 health units per heart (`Health_ChangeBy`), magic meter 0x30 (single) / 0x60
//! (double), see `include/save.h`.

pub const HEART: i32 = 16;
pub const MAGIC_NORMAL_METER: i32 = 0x30;
pub const MAGIC_DOUBLE_METER: i32 = 0x60;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Age {
    Child,
    Adult,
}

/// `gItemAgeReqs` / `gEquipAgeReqs`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgeReq {
    Any,
    Child,
    Adult,
}

impl AgeReq {
    pub fn allows(self, age: Age) -> bool {
        matches!((self, age), (AgeReq::Any, _) | (AgeReq::Child, Age::Child) | (AgeReq::Adult, Age::Adult))
    }
}

/// Usable items in `SLOT_*` order (the pause screen grid). Hookshot/Longshot and the two ocarinas
/// share a slot, so they are an upgrade flag on [`Inventory`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Item {
    DekuStick,
    DekuNut,
    Bomb,
    Bow,
    FireArrow,
    DinsFire,
    Slingshot,
    Ocarina,
    Bombchu,
    Hookshot,
    IceArrow,
    FaroresWind,
    Boomerang,
    LensOfTruth,
    MagicBean,
    MegatonHammer,
    LightArrow,
    NayrusLove,
}

impl Item {
    pub const ALL: [Item; 18] = [
        Item::DekuStick, Item::DekuNut, Item::Bomb, Item::Bow, Item::FireArrow, Item::DinsFire, Item::Slingshot, Item::Ocarina,
        Item::Bombchu, Item::Hookshot, Item::IceArrow, Item::FaroresWind, Item::Boomerang, Item::LensOfTruth, Item::MagicBean,
        Item::MegatonHammer, Item::LightArrow, Item::NayrusLove,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// `gItemAgeReqs`.
    pub fn age_req(self) -> AgeReq {
        use Item::*;
        match self {
            DekuStick | Slingshot | Boomerang | MagicBean => AgeReq::Child,
            Bow | FireArrow | Hookshot | IceArrow | MegatonHammer | LightArrow => AgeReq::Adult,
            _ => AgeReq::Any,
        }
    }

    pub fn usable_by(self, age: Age) -> bool {
        self.age_req().allows(age)
    }

    pub fn label(self) -> &'static str {
        use Item::*;
        match self {
            DekuStick => "DEKU STICK",
            DekuNut => "DEKU NUT",
            Bomb => "BOMB",
            Bow => "FAIRY BOW",
            FireArrow => "FIRE ARROW",
            DinsFire => "DIN'S FIRE",
            Slingshot => "FAIRY SLINGSHOT",
            Ocarina => "OCARINA",
            Bombchu => "BOMBCHU",
            Hookshot => "HOOKSHOT",
            IceArrow => "ICE ARROW",
            FaroresWind => "FARORE'S WIND",
            Boomerang => "BOOMERANG",
            LensOfTruth => "LENS OF TRUTH",
            MagicBean => "MAGIC BEAN",
            MegatonHammer => "MEGATON HAMMER",
            LightArrow => "LIGHT ARROW",
            NayrusLove => "NAYRU'S LOVE",
        }
    }

    /// Short HUD label for a C button (touch buttons are small).
    pub fn short(self) -> &'static str {
        use Item::*;
        match self {
            DekuStick => "STK",
            DekuNut => "NUT",
            Bomb => "BMB",
            Bow => "BOW",
            FireArrow => "F.AR",
            DinsFire => "DIN",
            Slingshot => "SLG",
            Ocarina => "OCA",
            Bombchu => "CHU",
            Hookshot => "HKS",
            IceArrow => "I.AR",
            FaroresWind => "FAR",
            Boomerang => "BMR",
            LensOfTruth => "LNS",
            MagicBean => "BEAN",
            MegatonHammer => "HAM",
            LightArrow => "L.AR",
            NayrusLove => "NAY",
        }
    }

    /// Magic spent per use (`sMagicSpellCosts` { 12, 24, 24 }, `sMagicArrowCosts` { 4, 4, 8 }).
    pub fn magic_cost(self) -> i32 {
        match self {
            Item::DinsFire => 12,
            Item::FaroresWind | Item::NayrusLove => 24,
            Item::FireArrow | Item::IceArrow => 4,
            Item::LightArrow => 8,
            _ => 0,
        }
    }
}

/// Bottle contents (`ITEM_BOTTLE_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Bottle {
    Empty,
    RedPotion,
    GreenPotion,
    BluePotion,
    Fairy,
    Fish,
    Milk,
    HalfMilk,
    RutoLetter,
    BlueFire,
    Bug,
    BigPoe,
    Poe,
}

impl Bottle {
    pub fn label(self) -> &'static str {
        match self {
            Bottle::Empty => "EMPTY BOTTLE",
            Bottle::RedPotion => "RED POTION",
            Bottle::GreenPotion => "GREEN POTION",
            Bottle::BluePotion => "BLUE POTION",
            Bottle::Fairy => "FAIRY",
            Bottle::Fish => "FISH",
            Bottle::Milk => "LON LON MILK",
            Bottle::HalfMilk => "MILK (HALF)",
            Bottle::RutoLetter => "RUTO'S LETTER",
            Bottle::BlueFire => "BLUE FIRE",
            Bottle::Bug => "BUGS",
            Bottle::BigPoe => "BIG POE",
            Bottle::Poe => "POE",
        }
    }
}

/// What drinking / using a bottle does (`Player_Action_8084EAC0` table `D_808549FC`: red 0x01 =
/// health 0x140, blue 0x03 = both, green 0x02 = `Magic_Fill`, milk 0x04 = health 0x50).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottleUse {
    /// Heal `health` units, fill magic when set; the bottle becomes `becomes`.
    Drink { health: i32, magic: bool, becomes: Bottle },
    /// Release a fairy that heals 8 hearts (0x80) over a second.
    ReleaseFairy,
    /// Release a fish / bug / blue fire / poe in the world.
    Release(Bottle),
    /// The Poe's random +/- health roll.
    Poe,
    Nothing,
}

pub fn bottle_use(b: Bottle) -> BottleUse {
    match b {
        Bottle::Empty => BottleUse::Nothing,
        Bottle::RedPotion => BottleUse::Drink { health: 0x140, magic: false, becomes: Bottle::Empty },
        Bottle::GreenPotion => BottleUse::Drink { health: 0, magic: true, becomes: Bottle::Empty },
        Bottle::BluePotion => BottleUse::Drink { health: 0x140, magic: true, becomes: Bottle::Empty },
        Bottle::Milk => BottleUse::Drink { health: 0x50, magic: false, becomes: Bottle::HalfMilk },
        Bottle::HalfMilk => BottleUse::Drink { health: 0x50, magic: false, becomes: Bottle::Empty },
        Bottle::Fairy => BottleUse::ReleaseFairy,
        Bottle::Fish | Bottle::Bug | Bottle::BlueFire | Bottle::BigPoe => BottleUse::Release(b),
        Bottle::Poe => BottleUse::Poe,
        Bottle::RutoLetter => BottleUse::Nothing,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Sword {
    Kokiri,
    Master,
    Biggoron,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Shield {
    Deku,
    Hylian,
    Mirror,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tunic {
    Kokiri,
    Goron,
    Zora,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Boots {
    Kokiri,
    Iron,
    Hover,
}

impl Sword {
    /// `gEquipAgeReqs` row 0.
    pub fn age_req(self) -> AgeReq {
        if self == Sword::Kokiri { AgeReq::Child } else { AgeReq::Adult }
    }
    pub fn label(self) -> &'static str {
        match self {
            Sword::Kokiri => "KOKIRI SWORD",
            Sword::Master => "MASTER SWORD",
            Sword::Biggoron => "BIGGORON'S SWORD",
        }
    }
    /// Biggoron's sword is the two-handed one (`Player_HoldsTwoHandedWeapon`).
    pub fn two_handed(self) -> bool {
        self == Sword::Biggoron
    }
}

impl Shield {
    pub fn age_req(self) -> AgeReq {
        match self {
            Shield::Deku => AgeReq::Child,
            Shield::Hylian => AgeReq::Any,
            Shield::Mirror => AgeReq::Adult,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Shield::Deku => "DEKU SHIELD",
            Shield::Hylian => "HYLIAN SHIELD",
            Shield::Mirror => "MIRROR SHIELD",
        }
    }
}

impl Tunic {
    pub fn age_req(self) -> AgeReq {
        if self == Tunic::Kokiri { AgeReq::Any } else { AgeReq::Adult }
    }
    pub fn label(self) -> &'static str {
        match self {
            Tunic::Kokiri => "KOKIRI TUNIC",
            Tunic::Goron => "GORON TUNIC",
            Tunic::Zora => "ZORA TUNIC",
        }
    }
}

impl Boots {
    pub fn age_req(self) -> AgeReq {
        if self == Boots::Kokiri { AgeReq::Any } else { AgeReq::Adult }
    }
    pub fn label(self) -> &'static str {
        match self {
            Boots::Kokiri => "KOKIRI BOOTS",
            Boots::Iron => "IRON BOOTS",
            Boots::Hover => "HOVER BOOTS",
        }
    }
}

/// What a C button points at: a slot item or one of the four bottles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CItem {
    Item(Item),
    Bottle(u8),
}

/// Capacity upgrade levels (0..=3), `gUpgradeCapacities`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Upgrades {
    pub quiver: u8,
    pub bomb_bag: u8,
    pub bullet_bag: u8,
    pub stick_cap: u8,
    pub nut_cap: u8,
    pub wallet: u8,
}

pub fn capacity(table: &[u16; 4], level: u8) -> i32 {
    table[(level as usize).min(3)] as i32
}

const QUIVER: [u16; 4] = [0, 30, 40, 50];
const BOMB_BAG: [u16; 4] = [0, 20, 30, 40];
const BULLET_BAG: [u16; 4] = [0, 30, 40, 50];
const STICKS: [u16; 4] = [0, 10, 20, 30];
const NUTS: [u16; 4] = [0, 20, 30, 40];
const WALLET: [u16; 4] = [99, 200, 500, 500];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Ammo {
    Stick,
    Nut,
    Bomb,
    Arrow,
    Seed,
    Bombchu,
    Bean,
}

impl Upgrades {
    pub fn cap(&self, a: Ammo) -> i32 {
        match a {
            Ammo::Stick => capacity(&STICKS, self.stick_cap),
            Ammo::Nut => capacity(&NUTS, self.nut_cap),
            Ammo::Bomb => capacity(&BOMB_BAG, self.bomb_bag),
            Ammo::Arrow => capacity(&QUIVER, self.quiver),
            Ammo::Seed => capacity(&BULLET_BAG, self.bullet_bag),
            Ammo::Bombchu => 50,
            Ammo::Bean => 10,
        }
    }
    pub fn wallet_cap(&self) -> i32 {
        capacity(&WALLET, self.wallet)
    }
}

/// Everything the pause screen and HUD show.
#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub owned: [bool; 18],
    pub longshot: bool,
    pub ocarina_of_time: bool,
    pub bottles: [Option<Bottle>; 4],
    pub ammo: [i32; 7],
    pub upgrades: Upgrades,
    pub swords: [bool; 3],
    pub shields: [bool; 3],
    pub tunics: [bool; 3],
    pub boots_owned: [bool; 3],
    pub sword: Option<Sword>,
    pub shield: Option<Shield>,
    pub tunic: Tunic,
    pub boots: Boots,
    /// C-left, C-down, C-right.
    pub c: [Option<CItem>; 3],
    /// Health units, 16 per heart.
    pub health: i32,
    pub max_health: i32,
    pub magic: i32,
    pub magic_cap: i32,
    pub rupees: i32,
    pub small_keys: i32,
    /// The dungeon's boss key (one flag for the dungeon Link is in).
    pub boss_key: bool,
    /// Pieces of Heart towards the next container (0..=3, `QUEST_HEART_PIECE_COUNT`).
    pub heart_pieces: u8,
    /// Gold Skulltula tokens (`gsTokens`, 0..=100).
    pub skulltulas: u8,
    /// Gold Skulltula rewards already given (`EVENTCHKINF_SKULLTULA_REWARD_*`): bit n is the (n+1) x 10 milestone, bit 9 the 100.
    pub skull_rewards: u16,
    /// Stone of Agony (the 20-token reward).
    pub stone_of_agony: bool,
    /// Strength upgrade (0 none, 1 Goron's bracelet, 2 silver, 3 gold) and scale (0 none, 1 silver, 2 gold).
    pub strength: u8,
    pub scale: u8,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            owned: [false; 18],
            longshot: false,
            ocarina_of_time: false,
            bottles: [None; 4],
            ammo: [0; 7],
            upgrades: Upgrades::default(),
            swords: [false; 3],
            shields: [false; 3],
            tunics: [true, false, false],
            boots_owned: [true, false, false],
            sword: None,
            shield: None,
            tunic: Tunic::Kokiri,
            boots: Boots::Kokiri,
            c: [None; 3],
            health: 3 * HEART,
            max_health: 3 * HEART,
            magic: 0,
            magic_cap: 0,
            rupees: 0,
            small_keys: 0,
            boss_key: false,
            heart_pieces: 0,
            skulltulas: 0,
            skull_rewards: 0,
            stone_of_agony: false,
            strength: 0,
            scale: 0,
        }
    }
}

fn ammo_of(item: Item) -> Option<Ammo> {
    Some(match item {
        Item::DekuStick => Ammo::Stick,
        Item::DekuNut => Ammo::Nut,
        Item::Bomb => Ammo::Bomb,
        Item::Bow => Ammo::Arrow,
        Item::Slingshot => Ammo::Seed,
        Item::Bombchu => Ammo::Bombchu,
        Item::MagicBean => Ammo::Bean,
        _ => return None,
    })
}

impl Inventory {
    /// A sandbox loadout: every item, every upgrade at its top level, double magic, 20 hearts,
    /// every equipment piece. The courses outside the Deku Tree have no item progression, so this
    /// is the default ("I can use all powers and all items").
    pub fn full(age: Age) -> Self {
        let mut inv = Self {
            owned: [true; 18],
            longshot: true,
            ocarina_of_time: true,
            bottles: [Some(Bottle::RedPotion), Some(Bottle::GreenPotion), Some(Bottle::BluePotion), Some(Bottle::Fairy)],
            upgrades: Upgrades { quiver: 3, bomb_bag: 3, bullet_bag: 3, stick_cap: 3, nut_cap: 3, wallet: 2 },
            swords: [true; 3],
            shields: [true; 3],
            tunics: [true; 3],
            boots_owned: [true; 3],
            strength: 3,
            scale: 2,
            max_health: 20 * HEART,
            health: 20 * HEART,
            magic_cap: MAGIC_DOUBLE_METER,
            magic: MAGIC_DOUBLE_METER,
            rupees: 500,
            small_keys: 5,
            boss_key: false,
            ..Default::default()
        };
        inv.refill();
        inv.set_age_defaults(age);
        inv
    }

    /// The beginning of the game: nothing but the starting sword and shield.
    pub fn starter(age: Age) -> Self {
        let mut inv = Self::default();
        match age {
            Age::Child => {
                inv.swords[0] = true;
                inv.sword = Some(Sword::Kokiri);
            }
            Age::Adult => {
                inv.swords[1] = true;
                inv.sword = Some(Sword::Master);
            }
        }
        inv.shields[1] = true;
        inv.shield = Some(Shield::Hylian);
        inv
    }

    pub fn ammo(&self, a: Ammo) -> i32 {
        self.ammo[a as usize]
    }

    pub fn set_ammo(&mut self, a: Ammo, n: i32) {
        self.ammo[a as usize] = n.clamp(0, self.upgrades.cap(a));
    }

    pub fn refill(&mut self) {
        for a in [Ammo::Stick, Ammo::Nut, Ammo::Bomb, Ammo::Arrow, Ammo::Seed, Ammo::Bombchu, Ammo::Bean] {
            self.ammo[a as usize] = self.upgrades.cap(a);
        }
        self.ammo[Ammo::Bombchu as usize] = 20;
    }

    pub fn has(&self, item: Item) -> bool {
        self.owned[item.index()]
    }

    pub fn give(&mut self, item: Item) {
        self.owned[item.index()] = true;
        if let Some(a) = ammo_of(item) {
            if self.upgrades.cap(a) == 0 {
                match a {
                    Ammo::Stick => self.upgrades.stick_cap = 1,
                    Ammo::Nut => self.upgrades.nut_cap = 1,
                    Ammo::Bomb => self.upgrades.bomb_bag = 1,
                    Ammo::Arrow => self.upgrades.quiver = 1,
                    Ammo::Seed => self.upgrades.bullet_bag = 1,
                    _ => {}
                }
            }
        }
    }

    /// Whether `item` may be placed on a C button right now (owned + the age requirement).
    pub fn can_assign(&self, item: Item, age: Age) -> bool {
        self.has(item) && item.usable_by(age)
    }

    pub fn can_assign_c(&self, c: CItem, age: Age) -> bool {
        match c {
            CItem::Item(i) => self.can_assign(i, age),
            CItem::Bottle(n) => self.bottles.get(n as usize).copied().flatten().is_some(),
        }
    }

    /// Put `c` on C button `slot` (0 left, 1 down, 2 right). The same item is removed from the other buttons.
    pub fn set_c(&mut self, slot: usize, c: Option<CItem>, age: Age) -> bool {
        if slot > 2 {
            return false;
        }
        if let Some(item) = c {
            if !self.can_assign_c(item, age) {
                return false;
            }
            for s in 0..3 {
                if s != slot && self.c[s] == Some(item) {
                    self.c[s] = None;
                }
            }
        }
        self.c[slot] = c;
        true
    }

    /// Drop assignments the current age cannot use (after turning into the other Link).
    pub fn sanitize(&mut self, age: Age) {
        for s in 0..3 {
            if let Some(c) = self.c[s] {
                if !self.can_assign_c(c, age) {
                    self.c[s] = None;
                }
            }
        }
        if let Some(s) = self.sword {
            if !s.age_req().allows(age) {
                self.sword = None;
            }
        }
        if let Some(s) = self.shield {
            if !s.age_req().allows(age) {
                self.shield = None;
            }
        }
        if !self.tunic.age_req().allows(age) {
            self.tunic = Tunic::Kokiri;
        }
        if !self.boots.age_req().allows(age) {
            self.boots = Boots::Kokiri;
        }
    }

    /// The loadout a fresh Link of `age` starts with (canonical picks: child sling + boomerang +
    /// bomb, adult bow + hookshot + bomb).
    pub fn set_age_defaults(&mut self, age: Age) {
        match age {
            Age::Child => {
                self.sword = Some(Sword::Kokiri).filter(|_| self.swords[0]);
                self.shield = Some(Shield::Deku).filter(|_| self.shields[0]);
                self.c = [Some(CItem::Item(Item::Slingshot)), Some(CItem::Item(Item::Boomerang)), Some(CItem::Item(Item::Bomb))];
            }
            Age::Adult => {
                self.sword = Some(Sword::Master).filter(|_| self.swords[1]);
                self.shield = Some(Shield::Hylian).filter(|_| self.shields[1]);
                self.c = [Some(CItem::Item(Item::Bow)), Some(CItem::Item(Item::Hookshot)), Some(CItem::Item(Item::Bomb))];
            }
        }
        self.sanitize(age);
    }

    pub fn equip_sword(&mut self, s: Sword, age: Age) -> bool {
        if !self.swords[s as usize] || !s.age_req().allows(age) {
            return false;
        }
        self.sword = Some(s);
        true
    }
    pub fn equip_shield(&mut self, s: Shield, age: Age) -> bool {
        if !self.shields[s as usize] || !s.age_req().allows(age) {
            return false;
        }
        self.shield = Some(s);
        true
    }
    pub fn equip_tunic(&mut self, t: Tunic, age: Age) -> bool {
        if !self.tunics[t as usize] || !t.age_req().allows(age) {
            return false;
        }
        self.tunic = t;
        true
    }
    pub fn equip_boots(&mut self, b: Boots, age: Age) -> bool {
        if !self.boots_owned[b as usize] || !b.age_req().allows(age) {
            return false;
        }
        self.boots = b;
        true
    }

    /// Spend one unit of ammo; false when empty.
    pub fn use_ammo(&mut self, a: Ammo) -> bool {
        if self.ammo(a) <= 0 {
            return false;
        }
        self.ammo[a as usize] -= 1;
        true
    }

    /// `Magic_RequestChange(play, cost, MAGIC_CONSUME_NOW)`.
    pub fn spend_magic(&mut self, cost: i32) -> bool {
        if cost == 0 {
            return true;
        }
        if self.magic_cap == 0 || self.magic < cost {
            return false;
        }
        self.magic -= cost;
        true
    }

    pub fn add_magic(&mut self, n: i32) {
        self.magic = (self.magic + n).min(self.magic_cap);
    }

    /// `Item_Give(ITEM_SKULL_TOKEN)`: `gsTokens++`, capped at 100 (`z_parameter.c`). Returns the new count.
    pub fn give_skulltula(&mut self) -> u8 {
        self.skulltulas = (self.skulltulas + 1).min(100);
        self.skulltulas
    }

    /// The cursed people's rewards (`EnSth`, `sRewards`): the first milestone (10, 20, ... 50 or 100) reached by
    /// `skulltulas` that was not given yet is given now: 10 Adult's Wallet, 20 Stone of Agony, 30 Giant's Wallet,
    /// 40 ten Bombchus, 50 Piece of Heart, 100 a 200-rupee gold rupee. Returns the milestone and a label.
    pub fn claim_skulltula_reward(&mut self) -> Option<(u8, &'static str)> {
        for (bit, milestone) in [10u8, 20, 30, 40, 50, 100].into_iter().enumerate() {
            let bit = if milestone == 100 { 9 } else { bit };
            if self.skulltulas < milestone || self.skull_rewards & (1 << bit) != 0 {
                continue;
            }
            self.skull_rewards |= 1 << bit;
            let label = match milestone {
                10 => {
                    self.upgrades.wallet = self.upgrades.wallet.max(1);
                    "ADULT'S WALLET"
                }
                20 => {
                    self.stone_of_agony = true;
                    "STONE OF AGONY"
                }
                30 => {
                    self.upgrades.wallet = self.upgrades.wallet.max(2);
                    "GIANT'S WALLET"
                }
                40 => {
                    let n = self.ammo(Ammo::Bombchu);
                    self.set_ammo(Ammo::Bombchu, n + 10);
                    "10 BOMBCHUS"
                }
                50 => {
                    self.give_heart_piece();
                    "PIECE OF HEART"
                }
                _ => {
                    self.add_rupees(200);
                    "HUGE RUPEE"
                }
            };
            return Some((milestone, label));
        }
        None
    }

    /// Hearts the container count stops at in this port (`Health_...` caps at 20 hearts).
    pub const MAX_HEARTS: i32 = 20;

    /// `Item_Give(ITEM_HEART_PIECE)` and the textbox that turns the fourth piece into a container
    /// (`z_message.c`: capacity +0x10 and health +0x10). True when this piece completed a heart.
    pub fn give_heart_piece(&mut self) -> bool {
        self.heart_pieces += 1;
        if self.heart_pieces >= 4 {
            self.heart_pieces = 0;
            self.give_heart_container();
            true
        } else {
            false
        }
    }

    /// `Item_Give(ITEM_HEART_CONTAINER)`: capacity +1 heart, health +1 heart.
    pub fn give_heart_container(&mut self) {
        if self.max_health < Self::MAX_HEARTS * HEART {
            self.max_health += HEART;
        }
        self.health = (self.health + HEART).min(self.max_health);
    }

    pub fn heal(&mut self, units: i32) {
        self.health = (self.health + units).clamp(0, self.max_health);
    }

    pub fn add_rupees(&mut self, n: i32) {
        self.rupees = (self.rupees + n).clamp(0, self.upgrades.wallet_cap());
    }

    /// Replace the contents of bottle `slot` (a drunk potion leaves an empty bottle).
    pub fn set_bottle(&mut self, slot: usize, with: Bottle) {
        if slot < 4 && self.bottles[slot].is_some() {
            self.bottles[slot] = Some(with);
        }
    }

    /// Items the pause screen lists, in grid order, with whether Link of `age` can use each.
    pub fn grid(&self, age: Age) -> Vec<(Item, bool)> {
        Item::ALL.iter().filter(|i| self.has(**i)).map(|i| (*i, i.usable_by(age))).collect()
    }

    /// Display name of a C-button entry (Hookshot upgrades to Longshot, Ocarina to the Ocarina of Time).
    pub fn name_of(&self, c: CItem) -> &'static str {
        match c {
            CItem::Item(Item::Hookshot) if self.longshot => "LONGSHOT",
            CItem::Item(Item::Ocarina) if self.ocarina_of_time => "OCARINA OF TIME",
            CItem::Item(Item::Ocarina) => "FAIRY OCARINA",
            CItem::Item(i) => i.label(),
            CItem::Bottle(n) => self.bottles.get(n as usize).copied().flatten().map_or("EMPTY", Bottle::label),
        }
    }

    pub fn short_of(&self, c: CItem) -> &'static str {
        match c {
            CItem::Item(Item::Hookshot) if self.longshot => "LNG",
            CItem::Item(i) => i.short(),
            CItem::Bottle(_) => "BTL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skulltula_rewards_come_once_per_milestone() {
        let mut inv = Inventory::default();
        for _ in 0..9 {
            inv.give_skulltula();
        }
        assert_eq!(inv.claim_skulltula_reward(), None);
        inv.give_skulltula();
        assert_eq!(inv.claim_skulltula_reward(), Some((10, "ADULT'S WALLET")));
        assert_eq!(inv.upgrades.wallet_cap(), 200);
        assert_eq!(inv.claim_skulltula_reward(), None, "only once");
        for _ in 0..40 {
            inv.give_skulltula();
        }
        // 50 tokens: the missing 20 / 30 / 40 / 50 rewards arrive one by one
        let got: Vec<_> = std::iter::from_fn(|| inv.claim_skulltula_reward()).map(|r| r.0).collect();
        assert_eq!(got, vec![20, 30, 40, 50]);
        assert!(inv.stone_of_agony && inv.upgrades.wallet_cap() == 500 && inv.heart_pieces == 1);
        for _ in 0..50 {
            inv.give_skulltula();
        }
        assert_eq!(inv.claim_skulltula_reward(), Some((100, "HUGE RUPEE")));
    }

    #[test]
    fn age_requirements_follow_the_decomp_table() {
        assert!(Item::Slingshot.usable_by(Age::Child) && !Item::Slingshot.usable_by(Age::Adult));
        assert!(Item::Bow.usable_by(Age::Adult) && !Item::Bow.usable_by(Age::Child));
        assert!(Item::Hookshot.usable_by(Age::Adult) && !Item::Hookshot.usable_by(Age::Child));
        assert!(Item::Bomb.usable_by(Age::Child) && Item::Bomb.usable_by(Age::Adult));
        assert!(Item::Boomerang.usable_by(Age::Child) && !Item::Boomerang.usable_by(Age::Adult));
        assert!(Item::MegatonHammer.usable_by(Age::Adult) && !Item::MegatonHammer.usable_by(Age::Child));
        assert!(Item::DekuStick.usable_by(Age::Child) && !Item::DekuStick.usable_by(Age::Adult));
        assert!(Item::MagicBean.usable_by(Age::Child));
        assert!(Item::DinsFire.usable_by(Age::Adult) && Item::NayrusLove.usable_by(Age::Child));
        assert!(!Sword::Kokiri.age_req().allows(Age::Adult) && Sword::Master.age_req().allows(Age::Adult));
        assert!(Shield::Mirror.age_req().allows(Age::Adult) && !Shield::Mirror.age_req().allows(Age::Child));
        assert!(Shield::Hylian.age_req().allows(Age::Child));
        assert!(!Tunic::Zora.age_req().allows(Age::Child) && Boots::Kokiri.age_req().allows(Age::Child));
    }

    #[test]
    fn capacities_match_gupgradecapacities() {
        let mut u = Upgrades::default();
        assert_eq!(u.cap(Ammo::Arrow), 0);
        u.quiver = 1;
        assert_eq!(u.cap(Ammo::Arrow), 30);
        u.quiver = 3;
        assert_eq!(u.cap(Ammo::Arrow), 50);
        u.bomb_bag = 2;
        assert_eq!(u.cap(Ammo::Bomb), 30);
        u.bullet_bag = 1;
        assert_eq!(u.cap(Ammo::Seed), 30);
        assert_eq!(u.wallet_cap(), 99);
        u.wallet = 1;
        assert_eq!(u.wallet_cap(), 200);
    }

    #[test]
    fn full_loadout_is_age_filtered() {
        let child = Inventory::full(Age::Child);
        assert_eq!(child.sword, Some(Sword::Kokiri));
        assert!(child.c.iter().flatten().all(|c| child.can_assign_c(*c, Age::Child)));
        let adult = Inventory::full(Age::Adult);
        assert_eq!(adult.sword, Some(Sword::Master));
        assert_eq!(adult.c[0], Some(CItem::Item(Item::Bow)));
        assert_eq!(adult.ammo(Ammo::Arrow), 50);
        assert_eq!(adult.magic, MAGIC_DOUBLE_METER);
    }

    #[test]
    fn assigning_moves_an_item_between_c_buttons_and_respects_age() {
        let mut inv = Inventory::full(Age::Adult);
        assert!(!inv.set_c(0, Some(CItem::Item(Item::Slingshot)), Age::Adult), "child item refused");
        assert!(inv.set_c(2, Some(CItem::Item(Item::Bow)), Age::Adult));
        assert_eq!(inv.c[0], None, "the bow left C-left");
        assert_eq!(inv.c[2], Some(CItem::Item(Item::Bow)));
        assert!(inv.set_c(1, Some(CItem::Bottle(1)), Age::Adult));
    }

    #[test]
    fn sanitize_drops_items_of_the_other_age() {
        let mut inv = Inventory::full(Age::Child);
        inv.sword = Some(Sword::Master);
        inv.sanitize(Age::Child);
        assert_eq!(inv.sword, None);
        inv.tunic = Tunic::Zora;
        inv.boots = Boots::Iron;
        inv.sanitize(Age::Child);
        assert_eq!((inv.tunic, inv.boots), (Tunic::Kokiri, Boots::Kokiri));
    }

    #[test]
    fn ammo_and_magic_are_spent_and_clamped() {
        let mut inv = Inventory::full(Age::Adult);
        for _ in 0..50 {
            assert!(inv.use_ammo(Ammo::Arrow));
        }
        assert!(!inv.use_ammo(Ammo::Arrow));
        assert!(inv.spend_magic(Item::DinsFire.magic_cost()));
        assert_eq!(inv.magic, MAGIC_DOUBLE_METER - 12);
        inv.add_magic(500);
        assert_eq!(inv.magic, MAGIC_DOUBLE_METER);
        let mut poor = Inventory::default();
        assert!(!poor.spend_magic(4), "no magic meter, no magic arrows");
        assert!(poor.spend_magic(0));
    }

    #[test]
    fn bottles_follow_the_decomp_table() {
        assert_eq!(bottle_use(Bottle::RedPotion), BottleUse::Drink { health: 0x140, magic: false, becomes: Bottle::Empty });
        assert_eq!(bottle_use(Bottle::BluePotion), BottleUse::Drink { health: 0x140, magic: true, becomes: Bottle::Empty });
        assert_eq!(bottle_use(Bottle::Milk), BottleUse::Drink { health: 0x50, magic: false, becomes: Bottle::HalfMilk });
        assert_eq!(bottle_use(Bottle::Empty), BottleUse::Nothing);
        let mut inv = Inventory::full(Age::Adult);
        inv.health = 16;
        inv.heal(0x140);
        assert_eq!(inv.health, inv.max_health);
    }

    #[test]
    fn four_heart_pieces_are_a_container() {
        let mut inv = Inventory::starter(Age::Child);
        inv.health = 16;
        let max = inv.max_health;
        for _ in 0..3 {
            assert!(!inv.give_heart_piece());
        }
        assert_eq!((inv.heart_pieces, inv.max_health), (3, max));
        assert!(inv.give_heart_piece());
        assert_eq!((inv.heart_pieces, inv.max_health, inv.health), (0, max + HEART, 16 + HEART));
        // 20 hearts is the cap.
        let mut full = Inventory::full(Age::Adult);
        full.give_heart_container();
        assert_eq!(full.max_health, 20 * HEART);
    }

    #[test]
    fn skulltula_tokens_count_to_a_hundred() {
        let mut inv = Inventory::default();
        for i in 1..=100 {
            assert_eq!(inv.give_skulltula(), i);
        }
        assert_eq!(inv.give_skulltula(), 100);
    }

    #[test]
    fn wallet_clamps() {
        let mut inv = Inventory::default();
        inv.add_rupees(500);
        assert_eq!(inv.rupees, 99);
    }
}
