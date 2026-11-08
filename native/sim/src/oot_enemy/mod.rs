//! Ocarina of Time enemies, engine-free: the enemies placed in the exported OoT scenes
//! (`assets/levels/oot_enemies/<course>.json`, written by `scripts/export-oot-enemies.py`).
//!
//! Every enemy keeps the hit points, the `DamageTable` row per damage type (parsed from the C
//! overlay into [`tables`]) and the drop table of its `ovl_En_*` actor; the AI is a simplified
//! but recognisable port of the overlay's action functions (each one is cited next to its kind in
//! [`Kind::spec`] and in `ai.rs`). Distances are OoT units x4.4 (`link::combat::len`), time is
//! the 30 Hz tick (an OoT frame is 1.5 ticks).
//!
//! Who can hurt them: any character. Link's sword swings, spin and jump slashes use `Dmg`
//! (see `link::combat::swing_damage`), his projectiles arrive as `Target` hits (the `dmg` of
//! `LinkEvent::HitExtra`); Mario and Banjo go through [`player::strikes`], which classifies the
//! SM64 attack states they share (punch/kick/dive/ground pound/stomp, Beak Barge, Beak Buster...)
//! into the same `Dmg` rows. They hurt every character through `MarioRuntime::take_damage`.
pub mod ai;
pub mod drop_table;
pub mod player;
pub mod tables;

use crate::collision::CollisionWorld;
use crate::link::combat::Dmg;
use crate::math::Vec3;

/// OoT units -> Jumpman units.
pub const U: f64 = 4.4;
/// 30 Hz ticks per 20 Hz OoT frame.
pub const T: f64 = 1.5;

pub fn frames(n: f64) -> f64 {
    n * T
}
/// OoT units per frame -> Jumpman units per tick.
pub fn speed(oot: f64) -> f64 {
    oot * U / T
}

/// Link's lock-on and shot targets: ids `TARGET_BASE + index` (the goombas use `EXTRA_TARGET_BASE + i`).
pub const TARGET_BASE: u32 = 1_000_000 + 100_000;
pub const TARGET_END: u32 = 1_000_000 + 400_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    DekuBaba,
    BigDekuBaba,
    WitheredBaba,
    GomaLarva,
    Skulltula,
    GoldSkulltula,
    Peahat,
    PeahatLarva,
    Tektite,
    RedTektite,
    Octorok,
    MadScrub,
    Wolfos,
    Guay,
    Leever,
}

pub const KINDS: [Kind; 15] = [
    Kind::DekuBaba,
    Kind::BigDekuBaba,
    Kind::WitheredBaba,
    Kind::GomaLarva,
    Kind::Skulltula,
    Kind::GoldSkulltula,
    Kind::Peahat,
    Kind::PeahatLarva,
    Kind::Tektite,
    Kind::RedTektite,
    Kind::Octorok,
    Kind::MadScrub,
    Kind::Wolfos,
    Kind::Guay,
    Kind::Leever,
];

/// What a kill leaves behind. `Random(n)` is `Item_DropCollectibleRandom` with table n.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drops {
    None,
    Random(u8),
    /// `Item_DropCollectible(ITEM00_NUTS)` x n, then a Deku Stick when cut down (Deku Babas).
    Nuts(u8),
    /// A Gold Skulltula token.
    Token,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    /// glb under `levels/oot_enemies/`.
    pub model: &'static str,
    /// `colChkInfo.health`.
    pub hp: i32,
    /// Body cylinder (`cylRadius`/`cylHeight`) in OoT units.
    pub radius: f64,
    pub height: f64,
    /// The AT collider's damage in 1/16 hearts (`{ 0xFFCFFFFF, ..., 0x08 }`).
    pub touch: i32,
    pub table: Option<&'static [(u8, u8); 32]>,
    pub drops: Drops,
    /// `lockOnArrowOffset` (OoT units above the actor origin).
    pub lock_offset: f64,
    /// Can Mario stomp it (an attack from above counts as `Dmg::KokiriJump`)?
    pub stompable: bool,
    /// Actor scale relative to the one baked into the model (Big Deku Baba 2.5, larva 0.5...).
    pub scale: f64,
}

impl Kind {
    pub fn name(self) -> &'static str {
        self.spec().name
    }

    pub fn from_name(s: &str) -> Option<Kind> {
        KINDS.iter().copied().find(|k| k.name() == s)
    }

    pub fn spec(self) -> Spec {
        use Kind::*;
        let s = |name, model, hp, radius, height, touch, table, drops, lock_offset, stompable, scale| Spec { name, model, hp, radius, height, touch, table, drops, lock_offset, stompable, scale };
        match self {
            // EnDekubaba_Init: sColChkInfoInit { 2, 25, 25 }, big: health 4, scaleFac 2.5.
            DekuBaba => s("deku_baba", "deku_baba", 2, 25.0, 25.0, 8, Some(&tables::DEKUBABA_NORMAL), Drops::Nuts(1), 15.0, false, 1.0),
            BigDekuBaba => s("big_deku_baba", "deku_baba", 4, 25.0 * 2.5, 25.0 * 2.5, 8, Some(&tables::DEKUBABA_BIG), Drops::Nuts(3), 15.0, false, 2.5),
            // EnKarebaba_Init: { 1, 15, 80 }, DamageTable_Get(1).
            WitheredBaba => s("withered_baba", "deku_baba", 1, 15.0, 80.0, 8, Some(&tables::KAREBABA), Drops::Nuts(1), 25.0, false, 1.0),
            // EnGoma_Init: health 2; default weapon damage; table 3 on death.
            GomaLarva => s("goma_larva", "goma_larva", 2, 15.0, 40.0, 8, None, Drops::Random(3), 0.2, true, 0.5),
            // EnSt_Init: CollisionCheckInfoInit2 { 2, .. }, DamageTable_Get(2), table 14.
            Skulltula => s("skulltula", "skulltula", 2, 20.0, 40.0, 4, Some(&tables::SKULLTULA), Drops::Random(14), 20.0, true, 1.0),
            // EnSw_Init: { 1, 2, 25, 25 }, DamageTable_Get(0xE); a token, table 3.
            GoldSkulltula => s("gold_skulltula", "skulltula", 1, 20.0, 25.0, 8, Some(&tables::GOLD_SKULLTULA), Drops::Token, 20.0, true, 0.5),
            // EnPeehat_Init: health 6, sDamageTable, table 2 (flying body hit 0x10 = a heart).
            Peahat => s("peahat", "peahat", 6, 50.0, 80.0, 16, Some(&tables::PEAHAT), Drops::Random(2), 7.0, false, 1.0),
            PeahatLarva => s("peahat_larva", "peahat", 2, 25.0, 40.0, 4, Some(&tables::PEAHAT), Drops::Random(4), 7.0, true, 0.17),
            // EnTite_Init: health 2 (red) / 4 (blue); table 14 (red), 4 (blue).
            Tektite => s("tektite", "tektite", 4, 20.0, 40.0, 8, Some(&tables::TITE), Drops::Random(4), 20.0, true, 1.0),
            RedTektite => s("red_tektite", "red_tektite", 2, 20.0, 40.0, 8, Some(&tables::TITE), Drops::Random(14), 20.0, true, 1.0),
            // EnOkuta_Init: { 1, 15, 60, 100 }, table 7, rock damage 0x08.
            Octorok => s("octorok", "octorok", 1, 15.0, 60.0, 8, Some(&tables::OKUTA), Drops::Random(7), 65.0, true, 1.0),
            // EnDekunuts_Init: { 1, 18, 32 }, table 3.
            MadScrub => s("mad_scrub", "mad_scrub", 1, 18.0, 32.0, 8, Some(&tables::DEKUNUTS), Drops::Random(3), 26.0, true, 1.0),
            // EnWf_Init: health 8, cylinder 50 x 100, table 13.
            Wolfos => s("wolfos", "wolfos", 8, 50.0, 100.0, 4, Some(&tables::WOLFOS), Drops::Random(13), 20.0, false, 1.0),
            // EnCrow_Init: { 1, 15, 30, 30 }, table 0.
            Guay => s("guay", "guay", 1, 15.0, 30.0, 8, Some(&tables::GUAY), Drops::Random(0), 20.0, true, 1.0),
            // EnReeba_Init: health 4 (20 for the big one), table 14.
            Leever => s("leever", "leever", 4, 15.0, 30.0, 8, Some(&tables::LEEVER), Drops::Random(14), 20.0, true, 1.0),
        }
    }

    pub fn is_baba(self) -> bool {
        matches!(self, Kind::DekuBaba | Kind::BigDekuBaba | Kind::WitheredBaba)
    }
}

/// AI states shared by the archetypes in `ai.rs`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum St {
    /// Waiting out of sight (Baba in the ground, Octorok/Scrub/Leever under cover, Wolfos "WaitAppear").
    Hidden,
    /// Coming out.
    Rising,
    /// Idle / patrol / swaying.
    Idle,
    Windup,
    Attack,
    /// After a miss or an attack; the Baba's head lies on the ground.
    Recover,
    /// Knocked back by a hit.
    Hurt,
    /// A Baba head on the floor that dies to the next sword hit.
    Vulnerable,
    Stunned,
    Chase,
    Retreat,
    Dying,
    /// Finished and invisible (respawns when `respawn` is set).
    Gone,
}

/// A thing for the app to show or hear. `pos` is the world position.
#[derive(Clone, Debug, PartialEq)]
pub enum Ev {
    /// The player was touched: damage in 1/16 hearts and where from (knockback direction).
    HurtPlayer { units: i32, from: Vec3, kind: Kind },
    /// A rock/nut left an enemy.
    Spit { kind: ShotKind, pos: Vec3, vel: Vec3 },
    /// Hit spark; `fire`/`ice` for the element.
    Spark { pos: Vec3, hard: bool },
    /// A sword bounced off (hidden Baba, shielded Wolfos...).
    Clank { pos: Vec3 },
    /// Dead: ring + drops.
    Died { id: usize, kind: Kind, pos: Vec3 },
    Drop { pos: Vec3, item: Item, qty: u8 },
    Sfx { name: &'static str, pos: Vec3 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShotKind {
    /// `gOctorokProjectileDL`
    Rock,
    /// `EnNutsball`
    Nut,
}

/// `ITEM00_*` values that Link's pickups understand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    RupeeGreen,
    RupeeBlue,
    RupeeRed,
    RupeePurple,
    RupeeOrange,
    Heart,
    Bombs,
    ArrowsSingle,
    Arrows(u8),
    Nuts,
    Stick,
    MagicSmall,
    MagicLarge,
    Seeds,
    Fairy,
    Token,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shot {
    pub kind: ShotKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f64,
    pub dead: bool,
    /// Turned around by a raised shield: now it hurts enemies, not the player.
    pub reflected: bool,
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub kind: Kind,
    pub index: usize,
    pub home: Vec3,
    pub home_yaw: f64,
    pub pos: Vec3,
    /// s16 angle.
    pub yaw: f64,
    pub vel: Vec3,
    pub hp: i32,
    pub st: St,
    /// Ticks in the current state.
    pub t: f64,
    pub age: f64,
    pub param: i32,
    pub stun: f64,
    pub frozen: f64,
    pub burn: f64,
    /// Ticks until the same melee swing may hit again.
    pub grace: f64,
    pub flash: f64,
    /// 0..1: how far a Baba's head is out of the ground / a Scrub is up.
    pub reach: f64,
    pub head: Vec3,
    pub visible: bool,
    pub clip: &'static str,
    pub clip_restart: bool,
    pub respawn: f64,
    pub aux: [f64; 3],
    pub vulnerable_hit: bool,
    pub killed_by: Option<Dmg>,
    /// Out of the active radius this tick (not simulated).
    pub asleep: bool,
}

impl Enemy {
    pub fn new(kind: Kind, index: usize, pos: Vec3, yaw_deg: f64, param: i32) -> Self {
        let yaw = crate::math::to_s16(yaw_deg / 180.0 * 32768.0) as f64;
        let mut e = Enemy {
            kind,
            index,
            home: pos,
            home_yaw: yaw,
            pos,
            yaw,
            vel: [0.0; 3],
            hp: kind.spec().hp,
            st: St::Idle,
            t: 0.0,
            age: 0.0,
            param,
            stun: 0.0,
            frozen: 0.0,
            burn: 0.0,
            grace: 0.0,
            flash: 0.0,
            reach: 0.0,
            head: pos,
            visible: true,
            clip: "Idle",
            clip_restart: false,
            respawn: 0.0,
            aux: [0.0; 3],
            vulnerable_hit: false,
            killed_by: None,
            asleep: true,
        };
        ai::init(&mut e);
        e
    }

    pub fn spec(&self) -> Spec {
        self.kind.spec()
    }

    /// Not dying or gone (a Baba at 0 hit points is still standing until its recoil finishes).
    pub fn alive(&self) -> bool {
        !matches!(self.st, St::Dying | St::Gone)
    }

    pub fn set_state(&mut self, st: St) {
        self.st = st;
        self.t = 0.0;
    }

    pub fn play(&mut self, clip: &'static str) {
        if self.clip != clip {
            self.clip = clip;
            self.clip_restart = true;
        }
    }

    /// The hurtbox cylinder at the current pose: feet, radius, height (Jumpman units).
    pub fn body(&self) -> (Vec3, f64, f64) {
        let s = self.spec();
        if self.kind.is_baba() {
            // The head is a sphere on the end of the stem.
            let r = s.radius * U;
            let c = self.head;
            return ([c[0], c[1] - r, c[2]], r, 2.0 * r);
        }
        (self.pos, s.radius * U, s.height * U)
    }

    /// Where the hurtbox stands: a Baba's head moves, the rest sit on the origin.
    pub fn hit_center(&self) -> Vec3 {
        if self.kind.is_baba() {
            self.head
        } else {
            self.pos
        }
    }

    /// Can Link lock on / can projectiles hit it right now?
    pub fn targetable(&self) -> bool {
        self.visible && !matches!(self.st, St::Gone | St::Dying)
    }

    /// Link's lock-on skips a Baba that is still in the ground.
    pub fn lockable(&self) -> bool {
        self.targetable() && !(self.kind.is_baba() && matches!(self.st, St::Hidden))
    }

    /// Position of Link's lock-on marker.
    pub fn lock_point(&self) -> Vec3 {
        let c = self.hit_center();
        [c[0], c[1] + self.spec().lock_offset * U + if self.kind.is_baba() { 0.0 } else { 0.0 }, c[2]]
    }
}

/// Damage and reaction of a weapon against a kind (`DamageTable` row, or the weapon's own damage).
pub fn damage_of(kind: Kind, dmg: Dmg) -> (i32, u8) {
    match kind.spec().table {
        Some(t) => {
            let (d, r) = t[dmg as usize];
            (d as i32, r)
        }
        None => (dmg.typical(), 0),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct HitResult {
    pub applied: bool,
    pub damage: i32,
    pub killed: bool,
    pub clank: bool,
}

/// A minimal deterministic generator (the shared `math::random` is reserved for SM64 replays).
#[derive(Clone, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 32) as u32
    }
    /// `Rand_ZeroOne`.
    pub fn zero_one(&mut self) -> f64 {
        self.next_u32() as f64 / 4294967296.0
    }
}

/// What the dropper knows about the collector (`Item_DropCollectibleRandom` consults the save).
#[derive(Clone, Copy, Debug)]
pub struct Needs {
    /// Link is playing (otherwise drops are plain hearts/rupees for the other characters).
    pub link: bool,
    pub adult: bool,
    /// 1/16 hearts.
    pub health: i32,
    pub magic: i32,
    pub magic_level: i32,
    pub seeds: i32,
    pub arrows: i32,
    pub bombs: i32,
    pub rupees: i32,
}

impl Default for Needs {
    fn default() -> Self {
        Needs { link: false, adult: false, health: 48, magic: 48, magic_level: 1, seeds: 30, arrows: 30, bombs: 10, rupees: 100 }
    }
}

/// `Item_DropCollectibleRandom(table * 16)`: the item (and quantity), or None.
pub fn roll_drop(rng: &mut Rng, table: u8, needs: &Needs) -> Option<(Item, u8)> {
    let row = (rng.zero_one() * 16.0) as usize;
    let base = table as usize * 16;
    let id = drop_table::ITEM_DROP_IDS[base + row];
    let mut qty = drop_table::DROP_QUANTITIES[base + row];
    let mut base = base;
    let mut id = id;
    // ITEM00_FLEXIBLE: pick what the player needs most.
    if id == 18 {
        let (bi, item) = if needs.health <= 16 {
            return Some((Item::Fairy, 1));
        } else if needs.health <= 0x30 {
            (0xB * 16, 3)
        } else if needs.health <= 0x50 {
            (0xA * 16, 3)
        } else if needs.magic_level != 0 && needs.magic == 0 {
            (0xA * 16, 14)
        } else if needs.magic_level != 0 && needs.magic <= needs.magic_level >> 1 {
            (0xA * 16, 15)
        } else if !needs.adult && needs.seeds < 6 {
            (0xA * 16, 16)
        } else if needs.adult && needs.arrows < 6 {
            (0xA * 16, 9)
        } else if needs.bombs < 6 {
            (0xD * 16, 4)
        } else if needs.rupees < 11 {
            (0xA * 16, 2)
        } else {
            return None;
        };
        base = bi;
        id = item;
        qty = drop_table::DROP_QUANTITIES[base];
    }
    if id == 0xFF {
        return None;
    }
    // func_8001F404: arrows and seeds follow the age, nuts and sticks need the item.
    let item = match id {
        0 => Item::RupeeGreen,
        1 => Item::RupeeBlue,
        2 => Item::RupeeRed,
        3 => Item::Heart,
        4 | 11 => Item::Bombs,
        5 => Item::ArrowsSingle,
        8..=10 => {
            if needs.link && !needs.adult {
                Item::Seeds
            } else {
                Item::Arrows(id - 7)
            }
        }
        12 => Item::Nuts,
        13 => Item::Stick,
        14 => Item::MagicLarge,
        15 => Item::MagicSmall,
        16 => {
            if needs.link && needs.adult {
                Item::Arrows(2)
            } else {
                Item::Seeds
            }
        }
        20 => Item::RupeePurple,
        19 => Item::RupeeOrange,
        _ => return None,
    };
    Some((item, qty.max(1)))
}

/// An `EnEncount1` spawner: keeps up to `max_live` enemies of `kind` alive near the player.
#[derive(Clone, Debug)]
pub struct Spawner {
    pub kind: Kind,
    pub pos: Vec3,
    pub range: f64,
    pub max_live: usize,
    pub total: usize,
    pub spawned: usize,
    pub timer: f64,
    pub live: Vec<usize>,
}

/// The enemies of one course plus their shots; stepped once per 30 Hz tick.
#[derive(Clone, Debug)]
pub struct Field {
    pub enemies: Vec<Enemy>,
    pub shots: Vec<Shot>,
    pub spawners: Vec<Spawner>,
    pub rng: Rng,
    pub events: Vec<Ev>,
    /// Enemies farther than this (Jumpman units) from the player are not simulated.
    pub active_radius: f64,
    pub tick: u64,
}

impl Field {
    pub fn new(seed: u64) -> Self {
        Field { enemies: vec![], shots: vec![], spawners: vec![], rng: Rng::new(seed), events: vec![], active_radius: 9000.0, tick: 0 }
    }

    pub fn add(&mut self, kind: Kind, pos: Vec3, yaw_deg: f64, param: i32) -> usize {
        let i = self.enemies.len();
        self.enemies.push(Enemy::new(kind, i, pos, yaw_deg, param));
        i
    }

    /// One 30 Hz tick. `player` is Mario/Link/Banjo's feet.
    pub fn step(&mut self, player: Vec3, world: &CollisionWorld) {
        self.tick += 1;
        let mut events = std::mem::take(&mut self.events);
        events.clear();
        let radius2 = self.active_radius * self.active_radius;
        self.spawn_step(player, world);
        for i in 0..self.enemies.len() {
            let e = &mut self.enemies[i];
            let dx = e.pos[0] - player[0];
            let dz = e.pos[2] - player[2];
            e.asleep = dx * dx + dz * dz > radius2 && e.st != St::Gone;
            if e.asleep {
                continue;
            }
            ai::step(e, player, world, &mut self.rng, &mut events);
        }
        for e in &events {
            if let Ev::Spit { kind, pos, vel } = e {
                self.shots.push(Shot { kind: *kind, pos: *pos, vel: *vel, age: 0.0, dead: false, reflected: false });
            }
        }
        ai::step_shots(&mut self.shots, player, world, &mut events);
        self.reflected_shots(&mut events);
        self.events = events;
    }

    /// A rock or nut turned around by a shield hurts whatever it hits (Octoroks and Scrubs die to their own shots).
    fn reflected_shots(&mut self, events: &mut Vec<Ev>) {
        for si in 0..self.shots.len() {
            let s = self.shots[si];
            if !s.reflected || s.dead {
                continue;
            }
            for i in 0..self.enemies.len() {
                let e = &self.enemies[i];
                if !e.targetable() {
                    continue;
                }
                let (c, r, h) = e.body();
                if ((c[0] - s.pos[0]).powi(2) + (c[2] - s.pos[2]).powi(2)).sqrt() < r + 15.0 && s.pos[1] > c[1] - 20.0 && s.pos[1] < c[1] + h + 20.0 {
                    let from = s.pos;
                    ai::on_hit(&mut self.enemies[i], Dmg::Slingshot, from, &mut self.rng, events);
                    self.shots[si].dead = true;
                    break;
                }
            }
        }
        self.shots.retain(|s| !s.dead);
    }

    fn spawn_step(&mut self, player: Vec3, world: &CollisionWorld) {
        for si in 0..self.spawners.len() {
            let (kind, pos, range, max_live, total) = {
                let s = &self.spawners[si];
                (s.kind, s.pos, s.range, s.max_live, s.total)
            };
            let sp = &mut self.spawners[si];
            sp.live.retain(|&i| self.enemies[i].st != St::Gone);
            let d = ((pos[0] - player[0]).powi(2) + (pos[2] - player[2]).powi(2)).sqrt();
            if sp.timer > 0.0 {
                sp.timer -= 1.0;
                continue;
            }
            if d > range * U * 4.0 || sp.live.len() >= max_live || sp.spawned >= total {
                continue;
            }
            // EnEncount1_SpawnLeevers: ring around the player at 150..400.
            let a = self.rng.zero_one() * 65536.0;
            let dist = (150.0 + self.rng.zero_one() * 250.0) * U;
            let x = player[0] + crate::math::sins(a) * dist;
            let z = player[2] + crate::math::coss(a) * dist;
            let floor = world.find_floor(x, player[1] + 200.0, z);
            if floor.surface.is_none() || (floor.height - player[1]).abs() > 300.0 {
                continue;
            }
            let sp = &mut self.spawners[si];
            sp.spawned += 1;
            sp.timer = frames(30.0);
            let i = self.enemies.len();
            let mut e = Enemy::new(kind, i, [x, floor.height, z], a / 65536.0 * 360.0, 0);
            e.asleep = false;
            self.enemies.push(e);
            self.spawners[si].live.push(i);
        }
    }

    /// Apply a weapon hit. `from` is where the blow came from (knockback / facing checks).
    pub fn hit(&mut self, i: usize, dmg: Dmg, from: Vec3) -> HitResult {
        let Some(e) = self.enemies.get_mut(i) else { return HitResult::default() };
        ai::on_hit(e, dmg, from, &mut self.rng, &mut self.events)
    }

    /// Roll and emit this enemy's drops (called by the app once a kill is final, with the player's needs).
    pub fn drops_for(&mut self, kind: Kind, pos: Vec3, needs: &Needs) -> Vec<(Vec3, Item, u8)> {
        let mut out = vec![];
        let spec = kind.spec();
        match spec.drops {
            Drops::None => {}
            Drops::Random(t) => {
                let n = if matches!(kind, Kind::PeahatLarva) { 1 } else { 1 };
                for _ in 0..n {
                    if let Some((item, qty)) = roll_drop(&mut self.rng, t, needs) {
                        out.push((pos, item, qty));
                    }
                }
                // EnPeehat: three table-4 rolls when the flying body dies.
                if kind == Kind::Peahat {
                    for _ in 0..2 {
                        if let Some((item, qty)) = roll_drop(&mut self.rng, 4, needs) {
                            out.push((pos, item, qty));
                        }
                    }
                }
            }
            Drops::Nuts(n) => {
                for _ in 0..n {
                    out.push((pos, Item::Nuts, 1));
                }
                out.push((pos, Item::Stick, 1));
            }
            Drops::Token => out.push((pos, Item::Token, 1)),
        }
        out
    }

    pub fn alive_count(&self) -> usize {
        self.enemies.iter().filter(|e| e.st != St::Gone).count()
    }
}

#[cfg(test)]
mod tests;
