//! The Painted Halls: rules, engine-free and deterministic. Owns the save (abilities, keys,
//! flags, discovered rooms, last save room), the live room (foes, hazards, boss, drops) and the
//! player's combat state. The glue in `halls.rs` feeds it Mario's position and moves each 30 Hz
//! tick and applies its requests (knock-backs, bounces, room changes); `halls_world.rs` is the data.
//!
//! Combat borrows Mario's own moves: a stomp (falling onto a foe), the punch / kick, the dive and
//! the ground-pound landing. Foes touch for damage with i-frames in between; hearts are two hit
//! points each. Dying returns to the last save room (progress items are kept the moment they are
//! picked up, so only the walk back is lost).

use super::halls_world::*;
use super::swarm_core::Rng;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const START_HP: i32 = 6;
pub const MAX_HP: i32 = 14;
pub const IFRAMES: u16 = 45;
pub const FOE_SLOTS: usize = 10;
pub const HAZARD_SLOTS: usize = 12;
pub const DROP_SLOTS: usize = 6;
const FOE_H: f32 = 110.0;
const PLAYER_R: f32 = 36.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickKind {
    Skill(Ability),
    Key(u8),
    Fragment,
    HeartUp,
    HeartOfHalls,
}

// ---------------------------------------------------------------------------------------------
// Save data
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SaveData {
    pub abilities: u8,
    pub keys: u8,
    pub bosses: u8,
    pub max_hp: i32,
    /// Collected pickups, opened chests, broken floors, opened doors (see `halls_world::flag`).
    pub flags: BTreeSet<u16>,
    pub fragments: BTreeSet<u16>,
    pub discovered: u32,
    /// Gate features the player has laid eyes on (flag ids), shown as notes on the map.
    pub seen_gates: BTreeSet<u16>,
    /// Last save pedestal: room and position.
    pub save_room: u8,
    pub save_pos: [f32; 2],
    pub cap: Option<Ability>,
    pub ticks: u64,
    pub deaths: u32,
    pub won: bool,
}

impl Default for SaveData {
    fn default() -> Self {
        Self { abilities: 0, keys: 0, bosses: 0, max_hp: START_HP, flags: BTreeSet::new(), fragments: BTreeSet::new(), discovered: 0, seen_gates: BTreeSet::new(), save_room: ROOM_FOYER as u8, save_pos: [0.0, 150.0], cap: None, ticks: 0, deaths: 0, won: false }
    }
}

impl SaveData {
    pub fn has(&self, a: Ability) -> bool {
        self.abilities & a.bit() != 0
    }
    pub fn caps(&self) -> Vec<Ability> {
        [Ability::WingCap, Ability::MetalCap, Ability::VanishCap].into_iter().filter(|a| self.has(*a)).collect()
    }
    pub fn is_new(&self) -> bool {
        self.discovered == 0
    }

    pub fn to_json(&self) -> Value {
        json!({
            "version": 1,
            "abilities": self.abilities,
            "keys": self.keys,
            "bosses": self.bosses,
            "max_hp": self.max_hp,
            "flags": self.flags.iter().collect::<Vec<_>>(),
            "fragments": self.fragments.iter().collect::<Vec<_>>(),
            "discovered": self.discovered,
            "seen_gates": self.seen_gates.iter().collect::<Vec<_>>(),
            "save_room": self.save_room,
            "save_pos": self.save_pos,
            "cap": self.cap.map(|a| a as u8),
            "ticks": self.ticks,
            "deaths": self.deaths,
            "won": self.won,
        })
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        let set = |k: &str| -> BTreeSet<u16> { v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u16)).collect()).unwrap_or_default() };
        let pos = v["save_pos"].as_array().map(|a| [a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32, a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32]).unwrap_or([0.0, 150.0]);
        let save_room = v["save_room"].as_u64()? as u8;
        if save_room as usize >= ROOM_COUNT {
            return None;
        }
        Some(Self {
            abilities: v["abilities"].as_u64()? as u8,
            keys: v["keys"].as_u64().unwrap_or(0) as u8,
            bosses: v["bosses"].as_u64().unwrap_or(0) as u8,
            max_hp: (v["max_hp"].as_i64().unwrap_or(START_HP as i64) as i32).clamp(START_HP, MAX_HP),
            flags: set("flags"),
            fragments: set("fragments"),
            discovered: v["discovered"].as_u64().unwrap_or(0) as u32,
            seen_gates: set("seen_gates"),
            save_room,
            save_pos: pos,
            cap: v["cap"].as_u64().and_then(|n| ABILITIES.get(n as usize).copied()),
            ticks: v["ticks"].as_u64().unwrap_or(0),
            deaths: v["deaths"].as_u64().unwrap_or(0) as u32,
            won: v["won"].as_bool().unwrap_or(false),
        })
    }
}

pub fn save_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("halls.json")
}

pub fn load_save() -> Option<SaveData> {
    let text = std::fs::read_to_string(save_path()).ok()?;
    SaveData::from_json(&serde_json::from_str(&text).ok()?)
}

pub fn write_save(s: &SaveData) {
    let path = save_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, s.to_json().to_string());
}

// ---------------------------------------------------------------------------------------------
// Live state
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FState {
    Idle,
    Chase,
    Windup,
    Charge,
    Rest,
    Fuse,
    Hide,
    Rise,
    Bite,
    Dead,
}

#[derive(Clone, Copy, Debug)]
pub struct Foe {
    pub kind: FoeKind,
    pub alive: bool,
    pub pos: [f32; 3],
    pub home: [f32; 3],
    pub hp: i32,
    pub state: FState,
    pub t: u16,
    pub dir: [f32; 2],
    pub flash: u8,
    pub radius: f32,
    pub id: u16,
    /// Boos: currently solid (the player is not looking at it).
    pub solid: bool,
}

impl Foe {
    pub fn new(kind: FoeKind, pos: [f32; 3], id: u16) -> Self {
        let (hp, radius) = match kind {
            FoeKind::Goomba => (1, 46.0),
            FoeKind::Bobomb => (1, 44.0),
            FoeKind::Spiny => (2, 48.0),
            FoeKind::Boo => (2, 55.0),
            FoeKind::Bully => (3, 70.0),
            FoeKind::Piranha => (2, 55.0),
        };
        Self { kind, alive: true, pos, home: pos, hp, state: if kind == FoeKind::Piranha { FState::Hide } else { FState::Idle }, t: (id as u16 * 7) % 40, dir: [0.0, 1.0], flash: 0, radius, id, solid: true }
    }
    pub fn height(&self) -> f32 {
        match self.kind {
            FoeKind::Bully => 120.0,
            FoeKind::Piranha => 150.0,
            _ => FOE_H,
        }
    }
    pub fn is_boo(&self) -> bool {
        self.kind == FoeKind::Boo
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HazardKind {
    Blast,
    Shock,
    Bomb,
}

#[derive(Clone, Copy, Debug)]
pub struct Hazard {
    pub alive: bool,
    pub kind: HazardKind,
    pub pos: [f32; 3],
    pub radius: f32,
    pub delay: u16,
    pub total: u16,
    pub dmg: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Drop {
    pub alive: bool,
    pub pos: [f32; 3],
    pub age: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BState {
    Intro,
    Walk,
    Telegraph,
    Slam,
    Prone,
    Hurt,
    Charge,
    Stunned,
    Dying,
}

#[derive(Clone, Copy, Debug)]
pub struct Boss {
    pub kind: BossKind,
    pub id: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub pos: [f32; 3],
    pub dir: [f32; 2],
    pub state: BState,
    pub t: u16,
    pub phase: u8,
    pub slams_left: u8,
    pub engaged: bool,
    pub flash: u8,
    pub toss_cd: u16,
    pub x: f32,
    pub z: f32,
}

impl Boss {
    pub fn radius(&self) -> f32 {
        match self.kind {
            BossKind::Whomp => 210.0,
            BossKind::KingBobomb => 150.0,
        }
    }
    pub fn vulnerable(&self) -> bool {
        matches!(self.state, BState::Prone | BState::Stunned)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Player {
    pub pos: [f32; 3],
    pub prev: [f32; 3],
    pub hp: i32,
    pub iframes: u16,
    pub dead: bool,
    pub ink_t: u8,
    pub face: f32,
}

/// What the glue tells the core about Mario each tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerFrame {
    pub pos: [f32; 3],
    pub vel_y: f32,
    /// Facing (radians; direction is `(sin, cos)` in x / z).
    pub face: f32,
    pub grounded: bool,
    pub punch: bool,
    pub dive: bool,
    pub pound_land: bool,
    /// Equipped cap.
    pub cap: Option<Ability>,
}

/// One-tick event flags for sound, toasts and glue requests.
#[derive(Clone, Debug, Default)]
pub struct Events {
    pub hurt: bool,
    pub knock: Option<[f32; 2]>,
    pub ink_launch: bool,
    pub bounce: bool,
    pub hit: bool,
    pub kill: Option<[f32; 3]>,
    pub blast: Option<[f32; 3]>,
    pub shock: bool,
    pub pickup: Option<PickKind>,
    pub chest: bool,
    pub saved: bool,
    pub cracked: Option<Rect>,
    pub door: Option<usize>,
    pub drop_to: Option<(u8, u8, [f32; 2])>,
    pub blocked: Option<String>,
    pub unlocked: bool,
    pub pit: bool,
    pub died: bool,
    pub boss_start: Option<BossKind>,
    pub boss_phase: Option<u8>,
    pub boss_down: Option<BossKind>,
    pub heart: bool,
    pub won: bool,
    pub heal: bool,
    pub new_room: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MapRoom {
    pub idx: usize,
    pub rect: (i32, i32, i32, i32),
    pub discovered: bool,
    pub here: bool,
    pub save: bool,
    pub boss: bool,
    pub fragments: (u32, u32),
    /// Seen, still-unresolved gates (ability, have it).
    pub gates: [Option<(Ability, bool)>; 3],
    pub locked: bool,
}

pub struct Halls {
    pub rooms: Vec<RoomDef>,
    pub save: SaveData,
    pub room: usize,
    pub geom: Geom,
    pub player: Player,
    pub foes: Vec<Foe>,
    pub hazards: Vec<Hazard>,
    pub drops: Vec<Drop>,
    pub boss: Option<Boss>,
    pub tick: u64,
    pub ev: Events,
    pub freeze: bool,
    pub entry: [f32; 4],
    pub safe: [f32; 3],
    door_cd: u16,
    save_cd: u16,
    rng: Rng,
    kills: u32,
    dirty: bool,
    pub god: bool,
    /// Door index the player came through (None after a drop or respawn).
    pub came_from: Option<usize>,
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn norm(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-4);
    [v[0] / l, v[1] / l]
}

impl Halls {
    pub fn new(save: SaveData, seed: u64) -> Self {
        let rooms = rooms();
        let mut h = Self {
            rooms,
            save,
            room: 0,
            geom: Geom::default(),
            player: Player::default(),
            foes: vec![],
            hazards: vec![],
            drops: vec![],
            boss: None,
            tick: 0,
            ev: Events::default(),
            freeze: false,
            entry: [0.0, 0.0, 600.0, 0.0],
            safe: [0.0, 0.0, 600.0],
            door_cd: 0,
            save_cd: 0,
            rng: Rng::new(seed),
            kills: 0,
            dirty: false,
            god: false,
            came_from: None,
        };
        h.player.hp = h.save.max_hp;
        let r = h.save.save_room as usize;
        let p = h.save.save_pos;
        let spawn = h.spawn_at_save(r, p);
        h.enter_room(r, spawn, None);
        h
    }

    fn spawn_at_save(&self, _room: usize, p: [f32; 2]) -> [f32; 4] {
        // Stand just in front of the pedestal (towards the room's centre).
        let to_c = norm([-p[0], -p[1]]);
        [p[0] + to_c[0] * 180.0, 0.0, p[1] + to_c[1] * 180.0, to_c[0].atan2(to_c[1])]
    }

    pub fn def(&self) -> &RoomDef {
        &self.rooms[self.room]
    }

    pub fn is_broken(&self, f: u16) -> bool {
        self.save.flags.contains(&f)
    }

    fn rebuild_geom(&mut self) {
        let flags = self.save.flags.clone();
        self.geom = self.rooms[self.room].build_geom(self.room, &|f| flags.contains(&f));
        self.dirty = true;
    }

    /// Collision state changed (a floor broke, a flag was set); the glue rebuilds the world when this flips.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn fragments(&self) -> u32 {
        self.save.fragments.len() as u32
    }

    pub fn enter_room(&mut self, room: usize, spawn: [f32; 4], came_from: Option<usize>) {
        self.room = room;
        self.came_from = came_from;
        self.rebuild_geom();
        self.entry = spawn;
        self.safe = [spawn[0], spawn[1], spawn[2]];
        self.player.pos = [spawn[0], spawn[1], spawn[2]];
        self.player.prev = self.player.pos;
        self.hazards.clear();
        self.drops.clear();
        self.door_cd = 40;
        self.save.discovered |= 1 << room;
        self.spawn_foes();
        self.boss = None;
        // A boss still standing is awake as soon as you step in.
        let defs: Vec<Feat> = self.rooms[room].feats.clone();
        for (i, f) in defs.iter().enumerate() {
            if let Feat::Boss { kind, boss, x, z, .. } = *f {
                if self.save.bosses & (1 << boss) == 0 {
                    let hp = match kind {
                        BossKind::Whomp => 3,
                        BossKind::KingBobomb => 4,
                    };
                    self.boss = Some(Boss { kind, id: boss, hp, max_hp: hp, pos: [x, 0.0, z], dir: [0.0, 1.0], state: BState::Intro, t: 70, phase: 0, slams_left: 0, engaged: true, flash: 0, toss_cd: 90, x, z });
                    self.ev.boss_start = Some(kind);
                    let _ = i;
                }
            }
        }
        self.ev.new_room = true;
        self.mark_gates_seen(true);
    }

    fn spawn_foes(&mut self) {
        let defs = self.rooms[self.room].foes.clone();
        self.foes.clear();
        for (i, d) in defs.iter().enumerate() {
            let y = self.geom.floor_y(d.x, d.z, d.y + 60.0).unwrap_or(d.y);
            let mut f = Foe::new(d.kind, [d.x, y, d.z], i as u16);
            if f.is_boo() {
                f.pos[1] = y + 90.0;
                f.home[1] = f.pos[1];
            }
            self.foes.push(f);
        }
    }

    fn mark_gates_seen(&mut self, all: bool) {
        let me = [self.player.pos[0], self.player.pos[2]];
        let def = &self.rooms[self.room];
        for (i, f) in def.feats.iter().enumerate() {
            let at = match *f {
                Feat::Gate { hint, .. } => Some([hint[0], hint[2]]),
                Feat::Ink { rect, .. } | Feat::Bars { rect, .. } | Feat::Crack { rect, .. } => Some([rect.cx, rect.cz]),
                _ => None,
            };
            if let Some(p) = at {
                if all || dist2(p, me) < 1500.0 {
                    self.save.seen_gates.insert(flag(self.room, i));
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The tick
    // -----------------------------------------------------------------------------------------

    pub fn tick(&mut self, f: &PlayerFrame) {
        let keep_new = self.ev.new_room;
        let keep_boss = self.ev.boss_start;
        self.ev = Events::default();
        self.ev.new_room = keep_new;
        self.ev.boss_start = keep_boss;
        if self.freeze || self.player.dead {
            return;
        }
        self.tick += 1;
        self.save.ticks += 1;
        self.player.prev = self.player.pos;
        self.player.pos = f.pos;
        self.player.face = f.face;
        self.door_cd = self.door_cd.saturating_sub(1);
        self.save_cd = self.save_cd.saturating_sub(1);
        self.player.iframes = self.player.iframes.saturating_sub(1);
        if self.god {
            self.player.hp = self.save.max_hp;
        }
        self.track_safe(f);
        self.hazard_tick(f);
        self.terrain(f);
        self.foes_tick(f);
        self.boss_tick(f);
        self.player_attacks(f);
        self.contact(f);
        self.pickups(f);
        self.doors(f);
        self.drops_tick(f);
        if self.tick % 15 == 0 {
            self.mark_gates_seen(false);
        }
        if self.player.hp <= 0 && !self.player.dead {
            self.player.dead = true;
            self.save.deaths += 1;
            self.ev.died = true;
        }
    }

    fn track_safe(&mut self, f: &PlayerFrame) {
        if !f.grounded || self.player.iframes > 0 && self.ev.hurt {
            return;
        }
        let [x, y, z] = f.pos;
        let ok = |dx: f32, dz: f32| self.geom.floor_y(x + dx, z + dz, y + 40.0).is_some_and(|h| (h - y).abs() < 30.0);
        if ok(0.0, 0.0) && ok(110.0, 0.0) && ok(-110.0, 0.0) && ok(0.0, 110.0) && ok(0.0, -110.0) && !self.geom.over_ink(x, z) {
            self.safe = f.pos;
        }
    }

    fn terrain(&mut self, f: &PlayerFrame) {
        let [x, y, z] = f.pos;
        // Ink burns anything but the metal cap.
        if self.geom.in_ink(x, y, z) && f.cap != Some(Ability::MetalCap) {
            self.player.ink_t = self.player.ink_t.saturating_add(1);
            if self.player.iframes == 0 {
                self.hurt(1, None);
                self.ev.ink_launch = true;
                self.ev.knock = Some([0.0, 0.0]);
            }
        } else {
            self.player.ink_t = 0;
        }
        // Cracked floors break under a ground pound.
        if f.pound_land {
            let defs = self.rooms[self.room].feats.clone();
            for (i, d) in defs.iter().enumerate() {
                if let Feat::Crack { rect, .. } = *d {
                    let fl = flag(self.room, i);
                    if !self.save.flags.contains(&fl) && y.abs() < 40.0 && Rect::new(rect.cx, rect.cz, rect.w + 160.0, rect.d + 160.0).contains(x, z) {
                        self.save.flags.insert(fl);
                        self.rebuild_geom();
                        self.ev.cracked = Some(rect);
                    }
                }
            }
        }
        // Falling.
        let defs = &self.rooms[self.room].feats;
        let kill = self.rooms[self.room].kill_y;
        if y < kill + 150.0 {
            for (i, d) in defs.iter().enumerate() {
                if let Feat::Crack { rect, to_room, to_zone, land, .. } = *d {
                    if self.save.flags.contains(&flag(self.room, i)) && Rect::new(rect.cx, rect.cz, rect.w + 100.0, rect.d + 100.0).contains(x, z) && y < kill + 100.0 {
                        self.ev.drop_to = Some((to_room, to_zone, land));
                        return;
                    }
                }
            }
        }
        if y < kill {
            self.hurt(1, None);
            self.player.iframes = IFRAMES;
            self.ev.pit = true;
        }
    }

    pub fn hurt(&mut self, amount: i32, from: Option<[f32; 2]>) {
        if self.player.iframes > 0 || self.player.dead || self.god {
            return;
        }
        self.player.hp = (self.player.hp - amount).max(0);
        self.player.iframes = IFRAMES;
        self.ev.hurt = true;
        if let Some(src) = from {
            let me = [self.player.pos[0], self.player.pos[2]];
            self.ev.knock = Some(norm([me[0] - src[0], me[1] - src[1]]));
        }
    }

    fn hazard_tick(&mut self, f: &PlayerFrame) {
        let me = [f.pos[0], f.pos[2]];
        let mut dmg: Option<(i32, [f32; 2])> = None;
        for h in self.hazards.iter_mut() {
            if !h.alive {
                continue;
            }
            if h.delay > 0 {
                h.delay -= 1;
                continue;
            }
            h.alive = false;
            match h.kind {
                HazardKind::Blast | HazardKind::Bomb => self.ev.blast = Some(h.pos),
                HazardKind::Shock => self.ev.shock = true,
            }
            if dist2(me, [h.pos[0], h.pos[2]]) < h.radius + PLAYER_R && f.pos[1] < h.pos[1] + 150.0 {
                dmg = Some((h.dmg, [h.pos[0], h.pos[2]]));
            }
        }
        if let Some((d, src)) = dmg {
            self.hurt(d, Some(src));
        }
        self.hazards.retain(|h| h.alive);
    }

    fn add_hazard(&mut self, kind: HazardKind, pos: [f32; 3], radius: f32, delay: u16, dmg: i32) {
        if self.hazards.len() < HAZARD_SLOTS {
            self.hazards.push(Hazard { alive: true, kind, pos, radius, delay, total: delay.max(1), dmg });
        }
    }

    /// Step toward `d` (x, z) by `speed`, staying on the platform the foe stands on.
    fn try_move(geom: &Geom, pos: &mut [f32; 3], d: [f32; 2], r: f32) -> bool {
        let mut moved = false;
        for step in [[d[0], d[1]], [d[0], 0.0], [0.0, d[1]]] {
            if step == [0.0, 0.0] {
                continue;
            }
            let (nx, nz) = (pos[0] + step[0], pos[2] + step[1]);
            let ahead = [nx + step[0].signum() * r * (step[0] != 0.0) as i32 as f32, nz + step[1].signum() * r * (step[1] != 0.0) as i32 as f32];
            let ok = |x: f32, z: f32| geom.floor_y(x, z, pos[1] + 50.0).is_some_and(|h| (h - pos[1]).abs() < 30.0) && !geom.blocked(x, pos[1] + 40.0, z, 0.0);
            if ok(nx, nz) && ok(ahead[0], ahead[1]) {
                pos[0] = nx;
                pos[2] = nz;
                moved = true;
                break;
            }
        }
        moved
    }

    fn foes_tick(&mut self, f: &PlayerFrame) {
        let me = [f.pos[0], f.pos[2]];
        let facing = [f.face.sin(), f.face.cos()];
        let n = self.foes.len();
        for i in 0..n {
            let mut fo = self.foes[i];
            if !fo.alive {
                continue;
            }
            fo.flash = fo.flash.saturating_sub(1);
            fo.t = fo.t.saturating_add(1);
            let fp = [fo.pos[0], fo.pos[2]];
            let d = dist2(fp, me);
            let to_me = norm([me[0] - fp[0], me[1] - fp[1]]);
            let level = (f.pos[1] - fo.pos[1]).abs() < 260.0;
            match fo.kind {
                FoeKind::Goomba => {
                    if d < 950.0 && level {
                        fo.state = FState::Chase;
                        let sp = if d > 500.0 { 4.5 } else { 6.5 };
                        Self::try_move(&self.geom, &mut fo.pos, [to_me[0] * sp, to_me[1] * sp], fo.radius);
                        fo.dir = to_me;
                    } else {
                        fo.state = FState::Idle;
                        if fo.t % 90 == 0 {
                            let a = (fo.id as f32 * 2.1 + fo.t as f32 * 0.37).sin() * 3.0;
                            fo.dir = [a.sin(), a.cos()];
                        }
                        if fo.t % 90 < 40 && dist2(fp, [fo.home[0], fo.home[2]]) < 300.0 {
                            Self::try_move(&self.geom, &mut fo.pos, [fo.dir[0] * 2.0, fo.dir[1] * 2.0], fo.radius);
                        } else if dist2(fp, [fo.home[0], fo.home[2]]) >= 300.0 {
                            let back = norm([fo.home[0] - fp[0], fo.home[2] - fp[1]]);
                            Self::try_move(&self.geom, &mut fo.pos, [back[0] * 2.5, back[1] * 2.5], fo.radius);
                        }
                    }
                }
                FoeKind::Bobomb => match fo.state {
                    FState::Fuse => {
                        let sp = 6.5;
                        Self::try_move(&self.geom, &mut fo.pos, [to_me[0] * sp, to_me[1] * sp], fo.radius);
                        fo.dir = to_me;
                        if fo.t >= 46 {
                            fo.alive = false;
                            fo.state = FState::Dead;
                            let pos = fo.pos;
                            self.foes[i] = fo;
                            self.add_hazard(HazardKind::Blast, pos, 270.0, 0, 2);
                            self.hazard_tick(f);
                            continue;
                        }
                    }
                    _ => {
                        if d < 420.0 && level {
                            fo.state = FState::Fuse;
                            fo.t = 0;
                        } else if d < 1000.0 && level {
                            Self::try_move(&self.geom, &mut fo.pos, [to_me[0] * 3.0, to_me[1] * 3.0], fo.radius);
                            fo.dir = to_me;
                        }
                    }
                },
                FoeKind::Spiny => match fo.state {
                    FState::Windup => {
                        if fo.t >= 22 {
                            fo.state = FState::Charge;
                            fo.t = 0;
                        }
                    }
                    FState::Charge => {
                        let ok = Self::try_move(&self.geom, &mut fo.pos, [fo.dir[0] * 15.0, fo.dir[1] * 15.0], fo.radius);
                        if fo.t >= 26 || !ok {
                            fo.state = FState::Rest;
                            fo.t = 0;
                        }
                    }
                    FState::Rest => {
                        if fo.t >= 36 {
                            fo.state = FState::Idle;
                            fo.t = 0;
                        }
                    }
                    _ => {
                        // Patrol back and forth along its axis.
                        if fo.t % 100 == 0 || fo.t == 1 {
                            let sgn = if (fo.t / 100) % 2 == 0 { 1.0 } else { -1.0 };
                            fo.dir = if fo.id % 2 == 0 { [sgn, 0.0] } else { [0.0, sgn] };
                        }
                        let ok = Self::try_move(&self.geom, &mut fo.pos, [fo.dir[0] * 3.0, fo.dir[1] * 3.0], fo.radius);
                        if !ok {
                            fo.dir = [-fo.dir[0], -fo.dir[1]];
                        }
                        if d < 700.0 && level && fo.t > 20 {
                            fo.state = FState::Windup;
                            fo.t = 0;
                            fo.dir = to_me;
                        }
                    }
                },
                FoeKind::Boo => {
                    // Shy: solid and chasing only while your back is turned.
                    let dot = facing[0] * to_me[0] + facing[1] * to_me[1];
                    fo.solid = dot < 0.15 || d < 130.0 && dot < 0.5;
                    let target_y = f.pos[1] + 70.0 + (fo.t as f32 * 0.07 + fo.id as f32).sin() * 25.0;
                    fo.pos[1] += (target_y.max(fo.home[1] - 60.0) - fo.pos[1]).clamp(-4.0, 4.0);
                    if d < 1500.0 && !fo.solid {
                        // Looked at: hangs back.
                    } else if d < 1500.0 {
                        let sp = 3.8;
                        let (w, dd) = (self.rooms[self.room].w / 2.0 - 120.0, self.rooms[self.room].d / 2.0 - 120.0);
                        fo.pos[0] = (fo.pos[0] + to_me[0] * sp).clamp(-w, w);
                        fo.pos[2] = (fo.pos[2] + to_me[1] * sp).clamp(-dd, dd);
                        fo.dir = to_me;
                    }
                }
                FoeKind::Bully => match fo.state {
                    FState::Windup => {
                        if fo.t >= 20 {
                            fo.state = FState::Charge;
                            fo.t = 0;
                        }
                    }
                    FState::Charge => {
                        let ok = Self::try_move(&self.geom, &mut fo.pos, [fo.dir[0] * 14.0, fo.dir[1] * 14.0], fo.radius);
                        if fo.t >= 20 || !ok {
                            fo.state = FState::Rest;
                            fo.t = 0;
                        }
                    }
                    FState::Rest => {
                        if fo.t >= 45 {
                            fo.state = FState::Idle;
                            fo.t = 0;
                        }
                    }
                    _ => {
                        if d < 1100.0 && level {
                            Self::try_move(&self.geom, &mut fo.pos, [to_me[0] * 3.8, to_me[1] * 3.8], fo.radius);
                            fo.dir = to_me;
                            if d < 520.0 && fo.t > 30 {
                                fo.state = FState::Windup;
                                fo.t = 0;
                            }
                        }
                    }
                },
                FoeKind::Piranha => match fo.state {
                    FState::Hide => {
                        if d < 360.0 && level && fo.t > 20 {
                            fo.state = FState::Rise;
                            fo.t = 0;
                        }
                    }
                    FState::Rise => {
                        fo.dir = to_me;
                        if fo.t >= 18 {
                            fo.state = FState::Bite;
                            fo.t = 0;
                            let p = [fo.pos[0] + fo.dir[0] * 120.0, fo.pos[1], fo.pos[2] + fo.dir[1] * 120.0];
                            let me_hit = dist2([p[0], p[2]], me) < 200.0 && f.pos[1] < fo.pos[1] + 140.0;
                            if me_hit {
                                self.foes[i] = fo;
                                self.hurt(1, Some(fp));
                                continue;
                            }
                        }
                    }
                    FState::Bite => {
                        if fo.t >= 30 {
                            fo.state = FState::Hide;
                            fo.t = 0;
                        }
                    }
                    _ => fo.state = FState::Hide,
                },
            }
            self.foes[i] = fo;
        }
    }

    fn damage_foe(&mut self, i: usize, dmg: i32) -> bool {
        let fo = &mut self.foes[i];
        if !fo.alive {
            return false;
        }
        fo.hp -= dmg;
        fo.flash = 6;
        self.ev.hit = true;
        if fo.hp <= 0 {
            fo.alive = false;
            fo.state = FState::Dead;
            self.ev.kill = Some(fo.pos);
            self.kills += 1;
            let roll = Rng::new(self.tick ^ (fo.id as u64) << 8 ^ self.room as u64).f32();
            if roll < 0.28 && self.drops.len() < DROP_SLOTS {
                let pos = [fo.pos[0], fo.pos[1] + 30.0, fo.pos[2]];
                self.drops.push(Drop { alive: true, pos, age: 0 });
            }
            return true;
        }
        false
    }

    fn player_attacks(&mut self, f: &PlayerFrame) {
        let me = [f.pos[0], f.pos[2]];
        let facing = [f.face.sin(), f.face.cos()];
        let falling = !f.grounded && f.vel_y < -6.0;
        let mut bounced = false;
        for i in 0..self.foes.len() {
            let fo = self.foes[i];
            if !fo.alive {
                continue;
            }
            if fo.is_boo() && !fo.solid {
                continue;
            }
            let fp = [fo.pos[0], fo.pos[2]];
            let d = dist2(fp, me);
            let dy = f.pos[1] - fo.pos[1];
            let to = norm([fp[0] - me[0], fp[1] - me[1]]);
            let in_front = facing[0] * to[0] + facing[1] * to[1] > 0.15;
            // Stomp.
            if falling && d < fo.radius + 52.0 && dy > fo.height() * 0.45 && dy < fo.height() + 90.0 && fo.kind != FoeKind::Spiny && !(fo.kind == FoeKind::Piranha && fo.state == FState::Bite) {
                self.damage_foe(i, 1);
                bounced = true;
                continue;
            }
            if f.pound_land && d < 320.0 && dy.abs() < 150.0 {
                self.damage_foe(i, 2);
                continue;
            }
            if (f.punch && d < fo.radius + 150.0 && in_front || f.dive && d < fo.radius + 190.0 && in_front) && dy.abs() < 140.0 {
                self.damage_foe(i, 1);
                // A little shove.
                let mut p = self.foes[i].pos;
                Self::try_move(&self.geom, &mut p, [to[0] * 50.0, to[1] * 50.0], 0.0);
                self.foes[i].pos = p;
            }
        }
        // Bosses.
        if let Some(mut b) = self.boss {
            let bp = [b.pos[0], b.pos[2]];
            let d = dist2(bp, me);
            let dy = f.pos[1] - b.pos[1];
            let r = b.radius();
            let mut hit = 0;
            if b.vulnerable() {
                if falling && d < r + 60.0 && dy > 40.0 && dy < 330.0 {
                    hit = 1;
                    bounced = true;
                } else if f.pound_land && d < r + 190.0 && dy.abs() < 160.0 {
                    hit = if b.kind == BossKind::KingBobomb { 2 } else { 1 };
                } else if (f.punch || f.dive) && d < r + 150.0 && b.kind == BossKind::KingBobomb && dy.abs() < 160.0 {
                    hit = 1;
                }
            } else if falling && d < r && dy > 60.0 && b.kind == BossKind::KingBobomb && b.state != BState::Charge && b.state != BState::Telegraph && b.state != BState::Intro {
                // A stomp on his crown just bounces (and stuns the rush).
                bounced = true;
            }
            if hit > 0 {
                b.hp -= hit;
                b.flash = 10;
                self.ev.hit = true;
                if b.hp <= 0 {
                    b.state = BState::Dying;
                    b.t = 0;
                } else {
                    b.state = BState::Hurt;
                    b.t = 0;
                    let phase = (((b.max_hp - b.hp) * 3 / b.max_hp) as u8).min(2);
                    if phase > b.phase {
                        b.phase = phase;
                        self.ev.boss_phase = Some(phase);
                    }
                }
            }
            self.boss = Some(b);
        }
        if bounced {
            self.ev.bounce = true;
        }
    }

    fn contact(&mut self, f: &PlayerFrame) {
        if self.player.iframes > 0 {
            return;
        }
        let me = [f.pos[0], f.pos[2]];
        let mut best: Option<([f32; 2], i32)> = None;
        for fo in &self.foes {
            if !fo.alive || fo.is_boo() && !fo.solid {
                continue;
            }
            if fo.kind == FoeKind::Piranha && fo.state != FState::Bite {
                continue;
            }
            let fp = [fo.pos[0], fo.pos[2]];
            let dy = f.pos[1] - fo.pos[1];
            if dist2(fp, me) < fo.radius + PLAYER_R && dy < fo.height() * 0.8 && dy > -150.0 {
                let dmg = match fo.kind {
                    FoeKind::Bully => 1,
                    FoeKind::Spiny if fo.state == FState::Charge => 2,
                    _ => 1,
                };
                best = Some((fp, dmg));
                break;
            }
        }
        if let Some(b) = self.boss {
            let bp = [b.pos[0], b.pos[2]];
            let dy = f.pos[1] - b.pos[1];
            let solid = !matches!(b.state, BState::Prone | BState::Stunned | BState::Dying | BState::Intro);
            if solid && dist2(bp, me) < b.radius() * 0.8 + PLAYER_R && dy < 260.0 && dy > -150.0 {
                best = Some((bp, 2));
            }
        }
        if let Some((src, dmg)) = best {
            self.hurt(dmg, Some(src));
        }
    }

    fn drops_tick(&mut self, f: &PlayerFrame) {
        let me = [f.pos[0], f.pos[2]];
        let mut healed = false;
        for d in self.drops.iter_mut() {
            if !d.alive {
                continue;
            }
            d.age += 1;
            if d.age > 600 {
                d.alive = false;
            } else if d.age > 12 && dist2([d.pos[0], d.pos[2]], me) < 100.0 && (f.pos[1] - d.pos[1]).abs() < 180.0 && self.player.hp < self.save.max_hp {
                d.alive = false;
                healed = true;
            }
        }
        self.drops.retain(|d| d.alive);
        if healed {
            self.player.hp = (self.player.hp + 2).min(self.save.max_hp);
            self.ev.heal = true;
        }
    }

    fn pickups(&mut self, f: &PlayerFrame) {
        let me = [f.pos[0], f.pos[2]];
        let defs = self.rooms[self.room].feats.clone();
        let boss_alive = self.boss.is_some();
        for (i, d) in defs.iter().enumerate() {
            let fl = flag(self.room, i);
            match *d {
                Feat::Pick { x, y, z, kind, .. } if !self.save.flags.contains(&fl) => {
                    if dist2([x, z], me) < 95.0 && (f.pos[1] + 70.0 - y).abs() < 150.0 {
                        self.collect(kind, fl);
                    }
                }
                Feat::Chest { x, z, y, kind, .. } if !self.save.flags.contains(&fl) => {
                    if dist2([x, z], me) < 130.0 && (f.pos[1] - y).abs() < 100.0 {
                        self.ev.chest = true;
                        self.collect(kind, fl);
                    }
                }
                Feat::Save { x, z } => {
                    if dist2([x, z], me) < 120.0 && f.pos[1] < 60.0 && self.save_cd == 0 && !boss_alive {
                        self.save_cd = 120;
                        let moved = self.save.save_room as usize != self.room || self.save.save_pos != [x, z];
                        self.save.save_room = self.room as u8;
                        self.save.save_pos = [x, z];
                        let healed = self.player.hp < self.save.max_hp;
                        self.player.hp = self.save.max_hp;
                        if moved || healed {
                            self.ev.saved = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The boss reward, held back by the boss fight, is a normal pickup placed where it fell.
    fn collect(&mut self, kind: PickKind, fl: u16) {
        self.save.flags.insert(fl);
        match kind {
            PickKind::Skill(a) => {
                self.save.abilities |= a.bit();
                if a.is_cap() && self.save.cap.is_none() {
                    self.save.cap = Some(a);
                }
            }
            PickKind::Key(k) => self.save.keys |= 1 << k,
            PickKind::Fragment => {
                self.save.fragments.insert(fl);
            }
            PickKind::HeartUp => {
                self.save.max_hp = (self.save.max_hp + 2).min(MAX_HP);
                self.player.hp = self.save.max_hp;
            }
            PickKind::HeartOfHalls => {
                self.save.won = true;
                self.ev.won = true;
            }
        }
        self.ev.pickup = Some(kind);
    }

    fn doors(&mut self, f: &PlayerFrame) {
        if self.door_cd > 0 || self.ev.drop_to.is_some() {
            return;
        }
        let def = &self.rooms[self.room];
        let me = [f.pos[0], f.pos[2]];
        let sealed = self.boss.is_some_and(|b| b.engaged);
        let frags = self.save.fragments.len() as u32;
        let mut hit: Option<(usize, DoorDef, usize)> = None;
        for (i, (di, d)) in def.doors().enumerate() {
            let w = d.wall_pos(def.w, def.d);
            let n = d.inward();
            let off = [me[0] - w[0], me[1] - w[1]];
            let depth = off[0] * n[0] + off[1] * n[1];
            let along = (off[0] * -n[1] + off[1] * n[0]).abs();
            if depth < 95.0 && along < 130.0 && (f.pos[1] - d.y).abs() < 160.0 {
                hit = Some((di, *d, i));
                break;
            }
        }
        let Some((di, d, _)) = hit else { return };
        let fl = flag(self.room, def.feats.iter().position(|x| matches!(x, Feat::Door(dd) if dd.side == d.side && dd.off == d.off && dd.y == d.y)).unwrap_or(0));
        let open = self.save.flags.contains(&fl);
        let allowed = if sealed {
            self.ev.blocked = Some("THE DOOR SEALS BEHIND YOU".into());
            false
        } else {
            match d.lock {
                Lock::None => true,
                Lock::Key(k) => {
                    if open || self.save.keys & (1 << k) != 0 {
                        if !open {
                            self.save.flags.insert(fl);
                            self.ev.unlocked = true;
                        }
                        true
                    } else {
                        self.ev.blocked = Some("LOCKED - THE GARDEN KEY FITS THIS LOCK".into());
                        false
                    }
                }
                Lock::Stars(n) => {
                    if frags >= n || open {
                        if !open {
                            self.save.flags.insert(fl);
                            self.ev.unlocked = true;
                        }
                        true
                    } else {
                        self.ev.blocked = Some(format!("THE STAR DOOR WANTS {n} FRAGMENTS  ({frags}/{n})"));
                        false
                    }
                }
                Lock::Boss(b) => {
                    if self.save.bosses & (1 << b) != 0 {
                        true
                    } else {
                        self.ev.blocked = Some("SEALED".into());
                        false
                    }
                }
                Lock::OneWay => {
                    if open {
                        true
                    } else {
                        self.ev.blocked = Some("THE SHUTTER WON'T BUDGE FROM THIS SIDE".into());
                        false
                    }
                }
            }
        };
        self.door_cd = if allowed { 40 } else { 45 };
        if allowed {
            // Passing through a door opens its partner shutter on the far side.
            let target = &self.rooms[d.to_room as usize];
            if let Some(td) = target.door(d.to_door as usize) {
                if td.lock == Lock::OneWay {
                    let idx = target.feats.iter().position(|x| matches!(x, Feat::Door(dd) if dd.side == td.side && dd.off == td.off && dd.y == td.y)).unwrap_or(0);
                    self.save.flags.insert(flag(d.to_room as usize, idx));
                }
            }
            self.ev.door = Some(di);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Bosses
    // -----------------------------------------------------------------------------------------

    fn boss_tick(&mut self, f: &PlayerFrame) {
        let Some(mut b) = self.boss else { return };
        let me = [f.pos[0], f.pos[2]];
        b.flash = b.flash.saturating_sub(1);
        b.t = b.t.saturating_add(1);
        let bp = [b.pos[0], b.pos[2]];
        let to_me = norm([me[0] - bp[0], me[1] - bp[1]]);
        let d = dist2(bp, me);
        let (hw, hd) = (self.rooms[self.room].w / 2.0 - 200.0, self.rooms[self.room].d / 2.0 - 200.0);
        match b.kind {
            BossKind::Whomp => match b.state {
                BState::Intro => {
                    if b.t >= 70 {
                        b.state = BState::Walk;
                        b.t = 0;
                    }
                }
                BState::Walk => {
                    let sp = [3.6, 5.0, 6.2][b.phase as usize];
                    b.dir = to_me;
                    b.pos[0] = (b.pos[0] + to_me[0] * sp).clamp(-hw, hw);
                    b.pos[2] = (b.pos[2] + to_me[1] * sp).clamp(-hd, hd);
                    if d < 620.0 && b.t > 50 {
                        b.state = BState::Telegraph;
                        b.t = 0;
                        b.slams_left = if b.phase >= 2 { 2 } else { 1 };
                    }
                }
                BState::Telegraph => {
                    // Rears back, locking his aim.
                    if b.t == 1 {
                        b.dir = to_me;
                    }
                    let pos = [b.pos[0] + b.dir[0] * 330.0, b.pos[1], b.pos[2] + b.dir[1] * 330.0];
                    if b.t == 2 {
                        self.add_hazard(HazardKind::Shock, pos, 330.0, 26, 2);
                    }
                    if b.t >= 28 {
                        b.state = BState::Slam;
                        b.t = 0;
                    }
                }
                BState::Slam => {
                    if b.t == 1 {
                        b.pos[0] = (b.pos[0] + b.dir[0] * 230.0).clamp(-hw, hw);
                        b.pos[2] = (b.pos[2] + b.dir[1] * 230.0).clamp(-hd, hd);
                        self.ev.shock = true;
                    }
                    if b.t >= 8 {
                        if b.slams_left > 1 {
                            b.slams_left -= 1;
                            b.state = BState::Telegraph;
                            b.t = 0;
                        } else {
                            b.state = BState::Prone;
                            b.t = 0;
                        }
                    }
                }
                BState::Prone => {
                    if b.t >= 110 {
                        b.state = BState::Walk;
                        b.t = 0;
                        // Gets up angry: calls a couple of goombas in phase 2+.
                        if b.phase >= 1 && self.foes.iter().filter(|x| x.alive).count() < 3 {
                            for k in 0..2 {
                                let a = if k == 0 { 1.0 } else { -1.0 };
                                let p = [(b.pos[0] + a * 500.0).clamp(-hw, hw), 0.0, (b.pos[2] + 300.0).clamp(-hd, hd)];
                                let mut g = Foe::new(FoeKind::Goomba, p, 40 + k);
                                g.state = FState::Chase;
                                if self.foes.len() < FOE_SLOTS {
                                    self.foes.push(g);
                                }
                            }
                        }
                    }
                }
                BState::Hurt => {
                    if b.t >= 40 {
                        b.state = BState::Walk;
                        b.t = 0;
                    }
                }
                BState::Dying => {
                    if b.t >= 60 {
                        self.boss_defeated(&b);
                        self.boss = None;
                        return;
                    }
                }
                _ => {}
            },
            BossKind::KingBobomb => match b.state {
                BState::Intro => {
                    if b.t >= 70 {
                        b.state = BState::Walk;
                        b.t = 0;
                    }
                }
                BState::Walk => {
                    let sp = [4.0, 5.0, 6.0][b.phase as usize];
                    b.dir = to_me;
                    b.pos[0] = (b.pos[0] + to_me[0] * sp).clamp(-hw, hw);
                    b.pos[2] = (b.pos[2] + to_me[1] * sp).clamp(-hd, hd);
                    b.toss_cd = b.toss_cd.saturating_sub(1);
                    if b.toss_cd == 0 {
                        b.toss_cd = [150, 110, 80][b.phase as usize];
                        let n = 1 + b.phase as usize;
                        for k in 0..n {
                            let jitter = if k == 0 { [0.0, 0.0] } else { [(self.rng.f32() - 0.5) * 500.0, (self.rng.f32() - 0.5) * 500.0] };
                            let p = [(me[0] + jitter[0]).clamp(-hw, hw), 0.0, (me[1] + jitter[1]).clamp(-hd, hd)];
                            self.add_hazard(HazardKind::Bomb, p, 240.0, 44, 1);
                        }
                    }
                    if b.t > 80 && d > 600.0 {
                        b.state = BState::Telegraph;
                        b.t = 0;
                    } else if b.t > 240 {
                        b.state = BState::Telegraph;
                        b.t = 0;
                    }
                }
                BState::Telegraph => {
                    if b.t == 1 {
                        b.dir = to_me;
                    }
                    if b.t >= 26 {
                        b.state = BState::Charge;
                        b.t = 0;
                    }
                }
                BState::Charge => {
                    let sp = [20.0, 23.0, 27.0][b.phase as usize];
                    b.pos[0] += b.dir[0] * sp;
                    b.pos[2] += b.dir[1] * sp;
                    let hit_wall = b.pos[0].abs() > hw || b.pos[2].abs() > hd;
                    b.pos[0] = b.pos[0].clamp(-hw, hw);
                    b.pos[2] = b.pos[2].clamp(-hd, hd);
                    if hit_wall || b.t > 60 {
                        b.state = BState::Stunned;
                        b.t = 0;
                        self.ev.shock = true;
                    }
                }
                BState::Stunned => {
                    if b.t >= 110 {
                        b.state = BState::Walk;
                        b.t = 0;
                    }
                }
                BState::Hurt => {
                    if b.t >= 40 {
                        b.state = BState::Walk;
                        b.t = 0;
                    }
                }
                BState::Dying => {
                    if b.t >= 60 {
                        self.boss_defeated(&b);
                        self.boss = None;
                        return;
                    }
                }
                _ => {}
            },
        }
        self.boss = Some(b);
    }

    fn boss_defeated(&mut self, b: &Boss) {
        self.save.bosses |= 1 << b.id;
        self.ev.boss_down = Some(b.kind);
        self.hazards.clear();
        for fo in &mut self.foes {
            fo.alive = false;
        }
        // The reward is a regular pickup of the room; place it at the boss's feet by marking it uncollected
        // (the feature stays where it was authored, in the boss room's pickup list).
        let feats = self.rooms[self.room].feats.clone();
        for (i, f) in feats.iter().enumerate() {
            if let Feat::Boss { boss, .. } = f {
                if *boss == b.id {
                    self.save.flags.remove(&flag(self.room, i));
                }
            }
        }
        self.player.hp = self.save.max_hp.min(self.player.hp + 4);
    }

    /// The pending boss reward of this room: (position, kind) while the boss is down and it is not yet taken.
    pub fn boss_reward(&self) -> Option<([f32; 3], PickKind)> {
        for (i, f) in self.rooms[self.room].feats.iter().enumerate() {
            if let Feat::Boss { boss, reward, x, z, .. } = *f {
                if self.save.bosses & (1 << boss) != 0 && !self.save.flags.contains(&flag(self.room, i)) {
                    return Some(([x, 90.0, z], reward));
                }
            }
        }
        None
    }

    /// Take the boss reward when the player walks onto it (called every tick by the glue after `tick`).
    pub fn take_boss_reward(&mut self, f: &PlayerFrame) {
        if let Some((p, kind)) = self.boss_reward() {
            if dist2([p[0], p[2]], [f.pos[0], f.pos[2]]) < 110.0 && f.pos[1] < 120.0 {
                let idx = self.rooms[self.room].feats.iter().position(|x| matches!(x, Feat::Boss { .. })).unwrap_or(0);
                self.collect(kind, flag(self.room, idx));
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Death, respawn, map
    // -----------------------------------------------------------------------------------------

    /// Back to the last save pedestal with full health.
    pub fn respawn(&mut self) {
        self.player.dead = false;
        self.player.hp = self.save.max_hp;
        self.player.iframes = 90;
        let r = self.save.save_room as usize;
        let spawn = self.spawn_at_save(r, self.save.save_pos);
        self.enter_room(r, spawn, None);
    }

    /// Back on solid ground after a fall.
    pub fn safe_spot(&self) -> [f32; 3] {
        self.safe
    }

    pub fn cap_cycle(&mut self) -> Option<Ability> {
        let caps = self.save.caps();
        if caps.is_empty() {
            return None;
        }
        let next = match self.save.cap.and_then(|c| caps.iter().position(|x| *x == c)) {
            Some(i) if i + 1 < caps.len() => Some(caps[i + 1]),
            Some(_) => None,
            None => Some(caps[0]),
        };
        self.save.cap = next;
        next
    }

    pub fn total_fragments_in_room(&self, room: usize) -> u32 {
        self.rooms[room].feats.iter().filter(|f| matches!(f, Feat::Pick { kind: PickKind::Fragment, .. })).count() as u32
    }

    pub fn map(&self) -> Vec<MapRoom> {
        let mut known = self.save.discovered;
        // Rooms one door away from a visited room show up as "?" cells.
        let mut frontier = 0u32;
        for (ri, r) in self.rooms.iter().enumerate() {
            if known & (1 << ri) != 0 {
                for (_, d) in r.doors() {
                    if known & (1 << d.to_room) == 0 {
                        frontier |= 1 << d.to_room;
                    }
                }
            }
        }
        known |= 0;
        let mut out = vec![];
        for (ri, r) in self.rooms.iter().enumerate() {
            let discovered = known & (1 << ri) != 0;
            if !discovered && frontier & (1 << ri) == 0 {
                continue;
            }
            let mut gates = [None; 3];
            let mut n = 0;
            let mut got = 0;
            let mut locked = false;
            for (i, f) in r.feats.iter().enumerate() {
                let seen = self.save.seen_gates.contains(&flag(ri, i));
                let gate = match *f {
                    Feat::Gate { req, .. } => Some(req),
                    Feat::Ink { .. } => Some(Ability::MetalCap),
                    Feat::Bars { .. } => Some(Ability::VanishCap),
                    Feat::Crack { .. } => Some(Ability::GroundPound),
                    _ => None,
                };
                if let (Some(a), true, true) = (gate, seen, discovered) {
                    if n < 3 && !gates.iter().flatten().any(|(x, _)| *x == a) {
                        gates[n] = Some((a, self.save.has(a)));
                        n += 1;
                    }
                }
                if let Feat::Pick { kind: PickKind::Fragment, .. } = f {
                    if self.save.flags.contains(&flag(ri, i)) {
                        got += 1;
                    }
                }
                if let Feat::Door(d) = f {
                    if discovered && d.lock != Lock::None && !self.save.flags.contains(&flag(ri, i)) && !matches!(d.lock, Lock::Boss(b) if self.save.bosses & (1 << b) != 0) {
                        locked = true;
                    }
                }
            }
            out.push(MapRoom {
                idx: ri,
                rect: r.map,
                discovered,
                here: ri == self.room,
                save: discovered && r.feats.iter().any(|f| matches!(f, Feat::Save { .. })),
                boss: discovered && r.feats.iter().any(|f| matches!(f, Feat::Boss { .. })),
                fragments: (got, self.total_fragments_in_room(ri)),
                gates,
                locked,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(h: &Halls, x: f32, y: f32, z: f32) -> PlayerFrame {
        let _ = h;
        PlayerFrame { pos: [x, y, z], vel_y: 0.0, face: 0.0, grounded: true, ..Default::default() }
    }

    fn start() -> Halls {
        Halls::new(SaveData::default(), 7)
    }

    #[test]
    fn save_round_trips_through_json() {
        let mut s = SaveData::default();
        s.abilities = Ability::LongJump.bit() | Ability::MetalCap.bit();
        s.keys = 1;
        s.bosses = 2;
        s.max_hp = 10;
        s.flags.extend([3u16, 77, 200]);
        s.fragments.extend([77u16, 200]);
        s.discovered = 0b1011;
        s.seen_gates.insert(130);
        s.save_room = ROOM_RACKS as u8;
        s.save_pos = [-400.0, -1000.0];
        s.cap = Some(Ability::MetalCap);
        s.ticks = 12345;
        s.deaths = 2;
        s.won = true;
        let text = s.to_json().to_string();
        let back = SaveData::from_json(&serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(s, back);
        assert!(SaveData::from_json(&json!({"abilities": 1, "save_room": 99})).is_none(), "bad room rejected");
        assert!(SaveData::from_json(&json!({"nonsense": true})).is_none());
    }

    #[test]
    fn a_new_game_starts_in_the_foyer_with_three_hearts() {
        let h = start();
        assert_eq!(h.room, ROOM_FOYER);
        assert_eq!(h.player.hp, START_HP);
        assert!(h.save.discovered & 1 != 0);
        assert!(h.save.is_new() == false, "entering marks the foyer discovered");
    }

    #[test]
    fn damage_grants_iframes_and_blocks_more_damage() {
        let mut h = start();
        h.hurt(2, Some([0.0, 0.0]));
        assert_eq!(h.player.hp, START_HP - 2);
        assert_eq!(h.player.iframes, IFRAMES);
        h.hurt(2, None);
        assert_eq!(h.player.hp, START_HP - 2, "i-frames absorb the second hit");
        for _ in 0..IFRAMES {
            let f = frame(&h, 0.0, 0.0, 600.0);
            h.tick(&f);
        }
        h.hurt(1, None);
        assert_eq!(h.player.hp, START_HP - 3);
    }

    #[test]
    fn dying_returns_to_the_last_save_room_with_full_health_and_keeps_progress() {
        let mut h = start();
        h.save.abilities |= Ability::LongJump.bit();
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        h.player.hp = 1;
        h.hurt(2, None);
        let f = frame(&h, 0.0, 0.0, 0.0);
        h.tick(&f);
        assert!(h.player.dead && h.ev.died);
        h.respawn();
        assert_eq!(h.room, ROOM_FOYER);
        assert_eq!(h.player.hp, h.save.max_hp);
        assert!(h.save.has(Ability::LongJump), "abilities survive death");
        assert!(!h.player.dead);
    }

    #[test]
    fn the_save_pedestal_heals_and_sets_the_respawn_point() {
        let mut h = start();
        h.enter_room(ROOM_RACKS, h.rooms[ROOM_RACKS].door_spawn(0), Some(0));
        h.player.hp = 2;
        let f = frame(&h, -400.0, 0.0, -1000.0);
        h.tick(&f);
        assert!(h.ev.saved);
        assert_eq!(h.player.hp, h.save.max_hp);
        assert_eq!(h.save.save_room as usize, ROOM_RACKS);
        h.respawn();
        assert_eq!(h.room, ROOM_RACKS);
    }

    #[test]
    fn a_goomba_chases_and_is_stomped_or_punched() {
        let mut h = start();
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        h.foes.truncate(1);
        h.foes[0].pos = [0.0, 0.0, 400.0];
        h.foes[0].home = h.foes[0].pos;
        let start_d = 400.0;
        for _ in 0..30 {
            let f = frame(&h, 0.0, 0.0, 0.0);
            h.tick(&f);
        }
        let d = dist2([h.foes[0].pos[0], h.foes[0].pos[2]], [0.0, 0.0]);
        assert!(d < start_d - 100.0, "goomba closes in ({d})");
        // Stomp: falling onto it.
        h.player.iframes = 0;
        let (fx, fz) = (h.foes[0].pos[0], h.foes[0].pos[2]);
        let mut f = frame(&h, fx, 80.0, fz);
        f.grounded = false;
        f.vel_y = -20.0;
        h.tick(&f);
        assert!(!h.foes[0].alive && h.ev.bounce, "stomp kills and bounces");
        // Punch a second one.
        h.foes[0].alive = true;
        h.foes[0].hp = 1;
        h.foes[0].pos = [200.0, 0.0, 0.0];
        h.foes[0].state = FState::Idle;
        h.player.iframes = 0;
        let mut f = frame(&h, 100.0, 0.0, 0.0);
        f.face = std::f32::consts::FRAC_PI_2;
        f.punch = true;
        h.tick(&f);
        assert!(!h.foes[0].alive, "punch in front kills");
    }

    #[test]
    fn contact_damage_respects_iframes_and_spinies_ignore_stomps() {
        let mut h = start();
        h.enter_room(ROOM_STAIRS, h.rooms[ROOM_STAIRS].door_spawn(0), Some(0));
        h.foes.clear();
        let mut sp = Foe::new(FoeKind::Spiny, [0.0, 0.0, 0.0], 0);
        sp.state = FState::Rest;
        h.foes.push(sp);
        let mut f = frame(&h, 0.0, 70.0, 0.0);
        f.grounded = false;
        f.vel_y = -20.0;
        h.tick(&f);
        assert!(h.foes[0].alive, "a stomp does not hurt a spiny");
        assert!(h.player.hp < START_HP, "it hurts the stomper");
        let hp = h.player.hp;
        h.tick(&f);
        assert_eq!(h.player.hp, hp, "i-frames protect the second tick");
    }

    #[test]
    fn a_bobomb_lights_its_fuse_then_blasts_the_player() {
        let mut h = start();
        h.enter_room(ROOM_STAIRS, h.rooms[ROOM_STAIRS].door_spawn(0), Some(0));
        h.foes.clear();
        h.foes.push(Foe::new(FoeKind::Bobomb, [0.0, 0.0, 300.0], 0));
        let mut blasted = false;
        for _ in 0..120 {
            let f = frame(&h, 0.0, 0.0, 0.0);
            h.tick(&f);
            blasted |= h.ev.blast.is_some();
        }
        assert!(blasted, "it exploded");
        assert!(h.player.hp < START_HP);
    }

    #[test]
    fn cracked_floor_breaks_only_under_a_ground_pound_and_then_drops_you() {
        let mut h = start();
        h.enter_room(ROOM_CRYPT, h.rooms[ROOM_CRYPT].door_spawn(0), Some(0));
        let (cx, cz) = (450.0, 100.0);
        assert_eq!(h.geom.floor_y(cx, cz, 10.0), Some(0.0), "plate is solid");
        let f = frame(&h, cx, 0.0, cz);
        h.tick(&f);
        assert!(h.ev.cracked.is_none(), "walking on it does nothing");
        let mut f = frame(&h, cx, 0.0, cz);
        f.pound_land = true;
        h.tick(&f);
        assert!(h.ev.cracked.is_some());
        assert!(h.take_dirty());
        assert!(h.geom.floor_y(cx, cz, 10.0).is_none(), "the hole is open");
        let f = frame(&h, cx, -420.0, cz);
        h.tick(&f);
        let (room, _, _) = h.ev.drop_to.expect("falls into the room below");
        assert_eq!(room as usize, ROOM_DRAIN);
    }

    #[test]
    fn ink_hurts_unless_the_metal_cap_is_worn() {
        let mut h = start();
        h.enter_room(ROOM_DRAIN, h.rooms[ROOM_DRAIN].door_spawn(1), Some(1));
        h.foes.clear();
        let mut f = frame(&h, 0.0, -50.0, 100.0);
        h.tick(&f);
        assert!(h.ev.ink_launch && h.player.hp < START_HP, "ink burns");
        h.player.hp = START_HP;
        h.player.iframes = 0;
        f.cap = Some(Ability::MetalCap);
        h.tick(&f);
        assert!(!h.ev.ink_launch && h.player.hp == START_HP, "metal is immune");
    }

    #[test]
    fn falling_into_a_pit_costs_a_heart_and_has_a_safe_spot_to_return_to() {
        let mut h = start();
        h.enter_room(ROOM_HEDGE, h.rooms[ROOM_HEDGE].door_spawn(0), Some(0));
        let ground = frame(&h, 0.0, 0.0, 1800.0);
        h.tick(&ground);
        assert_eq!(h.safe_spot(), [0.0, 0.0, 1800.0]);
        let fall = frame(&h, 0.0, -600.0, 750.0);
        h.tick(&fall);
        assert!(h.ev.pit && h.player.hp == START_HP - 1);
        assert_eq!(h.safe_spot()[2], 1800.0);
    }

    #[test]
    fn locked_doors_need_their_key_stars_or_boss() {
        let mut h = start();
        // Wine Racks south door needs the garden key.
        h.enter_room(ROOM_RACKS, h.rooms[ROOM_RACKS].door_spawn(0), Some(0));
        let door_pos = [0.0, 1500.0 - 40.0];
        let f = frame(&h, door_pos[0], 0.0, door_pos[1]);
        h.door_cd = 0;
        h.tick(&f);
        assert!(h.ev.door.is_none() && h.ev.blocked.as_deref().unwrap().contains("LOCKED"));
        h.save.keys |= 1;
        h.door_cd = 0;
        h.tick(&f);
        assert_eq!(h.ev.door, Some(2));
        assert!(h.ev.unlocked);
        // The foyer's star door.
        h.enter_room(ROOM_FOYER, h.rooms[ROOM_FOYER].door_spawn(1), Some(1));
        let f = frame(&h, 0.0, 0.0, -1200.0 + 40.0);
        h.door_cd = 0;
        h.tick(&f);
        assert!(h.ev.blocked.as_deref().unwrap().contains("FRAGMENTS"));
        for i in 0..8u16 {
            h.save.fragments.insert(i);
        }
        h.door_cd = 0;
        h.tick(&f);
        assert_eq!(h.ev.door, Some(3));
        // A one-way shutter opens once its far door has been used.
        h.enter_room(ROOM_RACKS, h.rooms[ROOM_RACKS].door_spawn(1), Some(1));
        let f = frame(&h, 1700.0 - 40.0, 0.0, 600.0);
        h.door_cd = 0;
        h.tick(&f);
        assert!(h.ev.blocked.is_some(), "shutter shut");
        h.enter_room(ROOM_DRAIN, h.rooms[ROOM_DRAIN].door_spawn(0), Some(0));
        let f = frame(&h, -1500.0 + 40.0, 0.0, -1400.0);
        h.door_cd = 0;
        h.tick(&f);
        assert_eq!(h.ev.door, Some(0));
        h.enter_room(ROOM_RACKS, h.rooms[ROOM_RACKS].door_spawn(1), Some(1));
        let f = frame(&h, 1700.0 - 40.0, 0.0, 600.0);
        h.door_cd = 0;
        h.tick(&f);
        assert_eq!(h.ev.door, Some(1), "now it opens both ways");
    }

    #[test]
    fn pickups_apply_and_persist() {
        let mut h = start();
        h.enter_room(ROOM_HEDGE, h.rooms[ROOM_HEDGE].door_spawn(0), Some(0));
        let f = frame(&h, 0.0, 0.0, -330.0);
        h.tick(&f);
        assert_eq!(h.ev.pickup, Some(PickKind::Skill(Ability::LongJump)));
        assert!(h.save.has(Ability::LongJump));
        h.tick(&f);
        assert!(h.ev.pickup.is_none(), "taken once");
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        let f = frame(&h, 0.0, 80.0, 0.0);
        h.tick(&f);
        assert_eq!(h.fragments(), 1);
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        h.tick(&f);
        assert_eq!(h.fragments(), 1, "no double counting after re-entry");
        // Heart containers raise max health and heal.
        h.enter_room(ROOM_CRYPT, h.rooms[ROOM_CRYPT].door_spawn(0), Some(0));
        let f = frame(&h, -1050.0, 0.0, 0.0);
        h.tick(&f);
        assert_eq!(h.save.max_hp, START_HP + 2);
        assert!(h.ev.chest);
    }

    #[test]
    fn foes_respawn_when_a_room_is_reentered() {
        let mut h = start();
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        let n = h.foes.len();
        for fo in &mut h.foes {
            fo.alive = false;
        }
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        assert_eq!(h.foes.iter().filter(|f| f.alive).count(), n);
    }

    fn boss_room(kind_room: usize) -> Halls {
        let mut h = start();
        h.enter_room(kind_room, h.rooms[kind_room].door_spawn(0), Some(0));
        h
    }

    #[test]
    fn the_warden_is_only_hurt_while_prone_and_goes_through_three_phases() {
        let mut h = boss_room(ROOM_WARDEN);
        assert!(h.boss.is_some());
        let mut phases = vec![];
        let mut f = frame(&h, 0.0, 0.0, 900.0);
        let mut guard = 0;
        while h.boss.is_some() && guard < 6000 {
            guard += 1;
            h.player.iframes = 99;
            h.player.hp = h.save.max_hp;
            let b = h.boss.unwrap();
            // A bot: hover near the Warden; ground-pound beside it whenever it lies prone.
            let (bx, bz) = (b.pos[0], b.pos[2]);
            f.pound_land = false;
            if b.state == BState::Prone && b.t > 4 {
                f.pos = [bx, 0.0, bz];
                f.pound_land = true;
            } else if matches!(b.state, BState::Walk | BState::Telegraph | BState::Slam) {
                // Pounding an upright Warden does nothing.
                f.pos = [bx + 300.0, 0.0, bz + 380.0];
                f.pound_land = guard % 17 == 0;
            } else {
                f.pos = [bx + 700.0, 0.0, bz + 700.0];
            }
            f.pos[0] = f.pos[0].clamp(-1200.0, 1200.0);
            f.pos[2] = f.pos[2].clamp(-1200.0, 1200.0);
            let hp_before = b.hp;
            h.tick(&f);
            if let Some(b2) = h.boss {
                if !matches!(b.state, BState::Prone | BState::Hurt | BState::Slam) {
                    assert_eq!(b2.hp, hp_before, "only a prone Warden takes damage ({:?})", b.state);
                }
            }
            if let Some(p) = h.ev.boss_phase {
                phases.push(p);
            }
        }
        assert!(h.boss.is_none(), "the Warden fell (after {guard} ticks)");
        assert!(h.save.bosses & 1 != 0);
        assert_eq!(phases, vec![1, 2], "enraged at 2 hp, frantic at 1 hp");
        assert!(h.boss_reward().is_some(), "the ground-pound sigil appears");
        // Taking it grants the ability.
        let (p, _) = h.boss_reward().unwrap();
        let f = frame(&h, p[0], 0.0, p[2]);
        h.take_boss_reward(&f);
        assert!(h.save.has(Ability::GroundPound));
        assert!(h.boss_reward().is_none());
    }

    #[test]
    fn king_fuse_charges_into_walls_and_is_stunned_there() {
        let mut h = boss_room(ROOM_BELFRY);
        let mut stunned = false;
        let mut f = frame(&h, 0.0, 0.0, 900.0);
        for _ in 0..1500 {
            h.player.iframes = 99;
            h.hazards.clear();
            f.pos = [0.0, 0.0, 900.0];
            h.tick(&f);
            if h.boss.is_some_and(|b| b.state == BState::Stunned) {
                stunned = true;
                break;
            }
        }
        assert!(stunned, "a charge ends against a wall");
        // A ground pound next to the stunned king deals double damage.
        let b = h.boss.unwrap();
        let hp = b.hp;
        let mut f = frame(&h, b.pos[0], 0.0, b.pos[2] + 200.0);
        f.pound_land = true;
        h.player.iframes = 99;
        h.tick(&f);
        assert_eq!(h.boss.unwrap().hp, hp - 2);
    }

    #[test]
    fn a_boss_seals_the_doors_until_it_is_down() {
        let mut h = boss_room(ROOM_WARDEN);
        let spawn = h.rooms[ROOM_WARDEN].door_spawn(0);
        let _ = spawn;
        let f = frame(&h, 0.0, 0.0, -1500.0 + 40.0);
        h.door_cd = 0;
        h.tick(&f);
        assert!(h.ev.door.is_none(), "sealed during the fight");
        assert!(h.ev.blocked.is_some());
    }

    #[test]
    fn discovery_and_the_map_show_visited_rooms_and_the_frontier() {
        let mut h = start();
        let m = h.map();
        let visited: Vec<usize> = m.iter().filter(|r| r.discovered).map(|r| r.idx).collect();
        assert_eq!(visited, vec![ROOM_FOYER]);
        let frontier: Vec<usize> = m.iter().filter(|r| !r.discovered).map(|r| r.idx).collect();
        assert!(frontier.contains(&ROOM_COURT) && frontier.contains(&ROOM_STAIRS) && frontier.contains(&ROOM_GEARS) && frontier.contains(&ROOM_GALLERY));
        assert!(m.iter().find(|r| r.idx == ROOM_FOYER).unwrap().here);
        assert!(m.iter().find(|r| r.idx == ROOM_FOYER).unwrap().save);
        // The foyer's wing ledge is a seen gate noted on its map cell.
        let g = m.iter().find(|r| r.idx == ROOM_FOYER).unwrap().gates;
        assert_eq!(g[0], Some((Ability::WingCap, false)));
        h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
        assert!(h.map().iter().find(|r| r.idx == ROOM_COURT).unwrap().discovered);
        assert!(h.map().iter().any(|r| r.idx == ROOM_HEDGE && !r.discovered));
        // Owning the ability flips the note to "open".
        h.save.abilities |= Ability::WingCap.bit();
        assert_eq!(h.map().iter().find(|r| r.idx == ROOM_FOYER).unwrap().gates[0], Some((Ability::WingCap, true)));
    }

    #[test]
    fn caps_cycle_through_owned_caps_and_off() {
        let mut h = start();
        assert_eq!(h.cap_cycle(), None);
        h.save.abilities |= Ability::WingCap.bit() | Ability::VanishCap.bit();
        assert_eq!(h.cap_cycle(), Some(Ability::WingCap));
        assert_eq!(h.cap_cycle(), Some(Ability::VanishCap));
        assert_eq!(h.cap_cycle(), None);
    }

    #[test]
    fn the_simulation_is_deterministic() {
        let run = || {
            let mut h = Halls::new(SaveData::default(), 99);
            h.enter_room(ROOM_COURT, h.rooms[ROOM_COURT].door_spawn(0), Some(0));
            let mut trace = vec![];
            for i in 0..200 {
                let f = PlayerFrame { pos: [(i as f32 * 3.0).sin() * 300.0, 0.0, (i as f32 * 2.0).cos() * 300.0], grounded: true, ..Default::default() };
                h.player.iframes = 99;
                h.tick(&f);
                trace.push(h.foes.iter().map(|f| (f.pos[0] * 10.0) as i32 + (f.pos[2] * 10.0) as i32).sum::<i32>());
            }
            trace
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn enemies_never_leave_their_platform_or_fall_off() {
        let mut h = start();
        h.enter_room(ROOM_HEDGE, h.rooms[ROOM_HEDGE].door_spawn(0), Some(0));
        for i in 0..600 {
            let f = PlayerFrame { pos: [((i / 40) as f32 * 1.3).sin() * 400.0, 0.0, 200.0 + ((i / 30) as f32).cos() * 1500.0], grounded: true, ..Default::default() };
            h.player.iframes = 99;
            h.tick(&f);
            for fo in h.foes.iter().filter(|f| f.alive && !f.is_boo()) {
                let under = h.geom.floor_y(fo.pos[0], fo.pos[2], fo.pos[1] + 60.0);
                assert!(under.is_some_and(|y| (y - fo.pos[1]).abs() < 31.0), "foe {:?} at {:?} stepped off", fo.kind, fo.pos);
            }
        }
    }

    #[test]
    fn every_door_opens_exactly_when_its_requirement_is_met() {
        let rs = rooms();
        let mut full = SaveData::default();
        full.keys = 1;
        full.bosses = 0b11;
        for i in 0..13u16 {
            full.fragments.insert(1000 + i);
        }
        for (ri, def) in rs.iter().enumerate() {
            for (di, d) in def.doors() {
                let w = d.wall_pos(def.w, def.d);
                let n = d.inward();
                let f = PlayerFrame { pos: [w[0] + n[0] * 40.0, d.y, w[1] + n[1] * 40.0], grounded: true, ..Default::default() };
                let try_door = |save: SaveData| -> bool {
                    let mut h = Halls::new(save, 1);
                    h.enter_room(ri, def.door_spawn(di), None);
                    h.boss = None;
                    h.door_cd = 0;
                    h.tick(&f);
                    h.ev.door == Some(di)
                };
                match d.lock {
                    Lock::None => assert!(try_door(SaveData::default()), "{} door {di} is free", def.name),
                    Lock::OneWay => {
                        assert!(!try_door(full.clone()), "{} door {di} is a one-way shutter", def.name);
                    }
                    _ => {
                        assert!(!try_door(SaveData::default()), "{} door {di} should be shut", def.name);
                        assert!(try_door(full.clone()), "{} door {di} should open with everything", def.name);
                    }
                }
            }
        }
    }

    #[test]
    fn boss_rewards_are_the_only_way_to_these_abilities() {
        // The ground pound and vanish cap exist only as boss rewards; no plain pickup grants them.
        let rs = rooms();
        for r in &rs {
            for f in &r.feats {
                if let Feat::Pick { kind: PickKind::Skill(a), .. } | Feat::Chest { kind: PickKind::Skill(a), .. } = f {
                    assert!(!matches!(a, Ability::GroundPound | Ability::VanishCap), "{} hands out {a:?} outside its boss", r.name);
                }
            }
        }
    }
}
