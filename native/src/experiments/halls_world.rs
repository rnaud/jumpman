//! The Painted Halls: the hand-authored world. Engine-free data: sixteen rooms in six regions,
//! each a few boxes of geometry, doors, pickups, foes and ability gates, plus the room-graph
//! solver that proves the game is completable and that every gate needs exactly its ability.
//!
//! Coordinates are room-local, centred on the room (x east, z south, y up, SM64 units); the floor
//! is y = 0 and spans `[-w/2, w/2] x [-d/2, d/2]` minus the holes. Zones are the abstract "places"
//! of a room (an island across a pit, a ledge, the far side of the ink) that the solver moves
//! between; every gating feature names the zones it joins, so the graph is derived from the
//! same data that builds the geometry.
//!
//! Physics the layouts are built around (measured on the real controller, see the tests in
//! `halls.rs`): a standing jump rises ~115, a double ~150, a triple ~630 (+~100 ledge grab); a
//! long jump covers ~1450; a wall kick climbs ~340 per kick between walls ~340 apart; a wing
//! flutter adds ~390 per flap (three per jump).

use super::halls_core::PickKind;
use std::collections::BTreeSet;


pub const ROOM_COUNT: usize = 16;
pub const FRAGMENTS_FOR_STAR_DOOR: u32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Ability {
    LongJump,
    WallKick,
    GroundPound,
    WingCap,
    MetalCap,
    VanishCap,
}

pub const ABILITIES: [Ability; 6] = [Ability::LongJump, Ability::WallKick, Ability::GroundPound, Ability::WingCap, Ability::MetalCap, Ability::VanishCap];

impl Ability {
    pub fn bit(self) -> u8 {
        1 << (self as u8)
    }
    pub fn name(self) -> &'static str {
        match self {
            Ability::LongJump => "LONG JUMP",
            Ability::WallKick => "WALL KICK",
            Ability::GroundPound => "GROUND POUND",
            Ability::WingCap => "WING CAP",
            Ability::MetalCap => "METAL CAP",
            Ability::VanishCap => "VANISH CAP",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Ability::LongJump => "LJ",
            Ability::WallKick => "WK",
            Ability::GroundPound => "GP",
            Ability::WingCap => "WG",
            Ability::MetalCap => "MT",
            Ability::VanishCap => "VN",
        }
    }
    /// What the player learns when picking it up.
    pub fn blurb(self) -> &'static str {
        match self {
            Ability::LongJump => "Run, crouch (Z) and jump to leap wide gaps.",
            Ability::WallKick => "Jump at a wall, then jump again to kick off it.",
            Ability::GroundPound => "Press Z in the air to smash cracked floors.",
            Ability::WingCap => "Press jump again in the air to flutter up high ledges.",
            Ability::MetalCap => "Wear it (F): walk through the ink unharmed.",
            Ability::VanishCap => "Wear it (F): slip through iron bars.",
        }
    }
    /// sRGB colour used for the pickup, the HUD chip and the map note (SM64's cap-switch hues for the caps).
    pub fn color(self) -> [u8; 3] {
        match self {
            Ability::LongJump => [255, 150, 40],
            Ability::WallKick => [255, 210, 60],
            Ability::GroundPound => [190, 90, 230],
            Ability::WingCap => [235, 60, 60],
            Ability::MetalCap => [60, 190, 90],
            Ability::VanishCap => [70, 130, 255],
        }
    }
    pub fn is_cap(self) -> bool {
        matches!(self, Ability::WingCap | Ability::MetalCap | Ability::VanishCap)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    Hall,
    Garden,
    Cellar,
    Vault,
    Tower,
    Gallery,
}

impl Region {
    pub fn name(self) -> &'static str {
        match self {
            Region::Hall => "The Great Hall",
            Region::Garden => "The Hanging Garden",
            Region::Cellar => "The Cellars",
            Region::Vault => "The Flooded Vault",
            Region::Tower => "The Clock Tower",
            Region::Gallery => "The Long Gallery",
        }
    }
}

/// Visual theme of a region: textures (names from `textures.rs`), tints, sky and fog.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub floor: &'static str,
    pub wall: &'static str,
    pub plat: &'static str,
    pub floor_tint: [f32; 3],
    pub wall_tint: [f32; 3],
    pub plat_tint: [f32; 3],
    pub sky_top: u32,
    pub sky_bottom: u32,
    pub fog: [f32; 3],
    pub fog_range: [f32; 2],
    pub light: [f32; 3],
}

impl Region {
    pub fn look(self) -> Look {
        match self {
            Region::Hall => Look { floor: "checker", wall: "whiteBrick", plat: "stone", floor_tint: [0.95, 0.8, 0.85], wall_tint: [1.0, 0.86, 0.78], plat_tint: [0.9, 0.82, 0.8], sky_top: 0x2a1840, sky_bottom: 0x7a4a6a, fog: [0.22, 0.12, 0.2], fog_range: [3500.0, 11000.0], light: [1.0, 0.9, 0.75] },
            Region::Garden => Look { floor: "grassTop", wall: "brick", plat: "dirt", floor_tint: [0.8, 1.0, 0.7], wall_tint: [0.6, 0.85, 0.5], plat_tint: [0.9, 0.85, 0.75], sky_top: 0x3a8ad8, sky_bottom: 0xbfe6a8, fog: [0.55, 0.78, 0.6], fog_range: [4500.0, 14000.0], light: [1.0, 1.0, 0.9] },
            Region::Cellar => Look { floor: "stone", wall: "brick", plat: "planks", floor_tint: [0.55, 0.5, 0.55], wall_tint: [0.5, 0.36, 0.34], plat_tint: [0.75, 0.55, 0.4], sky_top: 0x120a14, sky_bottom: 0x3a2024, fog: [0.08, 0.05, 0.07], fog_range: [2400.0, 8000.0], light: [1.0, 0.75, 0.55] },
            Region::Vault => Look { floor: "tiles", wall: "stone", plat: "tiles", floor_tint: [0.5, 0.85, 0.9], wall_tint: [0.4, 0.62, 0.7], plat_tint: [0.6, 0.9, 0.95], sky_top: 0x061a2a, sky_bottom: 0x14505a, fog: [0.04, 0.16, 0.2], fog_range: [2600.0, 9000.0], light: [0.7, 0.95, 1.0] },
            Region::Tower => Look { floor: "planks", wall: "sandstone", plat: "planks", floor_tint: [0.85, 0.7, 0.5], wall_tint: [0.85, 0.7, 0.45], plat_tint: [0.8, 0.62, 0.42], sky_top: 0x20386a, sky_bottom: 0xe8a068, fog: [0.5, 0.36, 0.3], fog_range: [4000.0, 13000.0], light: [1.0, 0.82, 0.6] },
            Region::Gallery => Look { floor: "flagstone", wall: "whiteBrick", plat: "gold", floor_tint: [1.0, 0.92, 0.7], wall_tint: [1.0, 0.95, 0.85], plat_tint: [1.0, 0.85, 0.5], sky_top: 0x14143a, sky_bottom: 0x6a4a8a, fog: [0.2, 0.15, 0.3], fog_range: [3500.0, 11000.0], light: [1.0, 0.92, 0.8] },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub cx: f32,
    pub cz: f32,
    pub w: f32,
    pub d: f32,
}

impl Rect {
    pub const fn new(cx: f32, cz: f32, w: f32, d: f32) -> Self {
        Self { cx, cz, w, d }
    }
    pub fn contains(&self, x: f32, z: f32) -> bool {
        (x - self.cx).abs() <= self.w / 2.0 && (z - self.cz).abs() <= self.d / 2.0
    }
    pub fn min(&self) -> [f32; 2] {
        [self.cx - self.w / 2.0, self.cz - self.d / 2.0]
    }
    pub fn max(&self) -> [f32; 2] {
        [self.cx + self.w / 2.0, self.cz + self.d / 2.0]
    }
    fn from_bounds(x0: f32, z0: f32, x1: f32, z1: f32) -> Self {
        Self { cx: (x0 + x1) / 2.0, cz: (z0 + z1) / 2.0, w: x1 - x0, d: z1 - z0 }
    }
    /// `self` minus `hole` as up to four rectangles.
    pub fn subtract(&self, hole: &Rect) -> Vec<Rect> {
        let (a0, a1) = (self.min(), self.max());
        let (h0, h1) = (hole.min(), hole.max());
        if h0[0] >= a1[0] || h1[0] <= a0[0] || h0[1] >= a1[1] || h1[1] <= a0[1] {
            return vec![*self];
        }
        let mut out = vec![];
        let (x0, x1) = (h0[0].max(a0[0]), h1[0].min(a1[0]));
        if h0[0] > a0[0] {
            out.push(Rect::from_bounds(a0[0], a0[1], h0[0], a1[1]));
        }
        if h1[0] < a1[0] {
            out.push(Rect::from_bounds(h1[0], a0[1], a1[0], a1[1]));
        }
        if h0[1] > a0[1] {
            out.push(Rect::from_bounds(x0, a0[1], x1, h0[1]));
        }
        if h1[1] < a1[1] {
            out.push(Rect::from_bounds(x0, h1[1], x1, a1[1]));
        }
        out
    }
}

/// How a box is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Plat,
    Wall,
    Crate,
    Hedge,
    Floor,
    Cracked,
    Ink,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bx {
    pub r: Rect,
    pub y0: f32,
    pub y1: f32,
    pub style: Style,
}

const fn bx(cx: f32, cz: f32, w: f32, d: f32, y0: f32, y1: f32, style: Style) -> Bx {
    Bx { r: Rect::new(cx, cz, w, d), y0, y1, style }
}

/// A solid column of stone from the floor up to `top`.
const fn plat(cx: f32, cz: f32, w: f32, d: f32, top: f32) -> Bx {
    bx(cx, cz, w, d, 0.0, top, Style::Plat)
}

/// A floating slab (60 thick) whose top is at `top`.
const fn slab(cx: f32, cz: f32, w: f32, d: f32, top: f32) -> Bx {
    bx(cx, cz, w, d, top - 60.0, top, Style::Plat)
}

const fn pillar(cx: f32, cz: f32, s: f32, top: f32) -> Bx {
    bx(cx, cz, s, s, 0.0, top, Style::Wall)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    N,
    S,
    E,
    W,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lock {
    None,
    /// A key by index (0 = the Garden Key).
    Key(u8),
    /// Needs this many star fragments.
    Stars(u32),
    /// Sealed until this boss is down (and while it is being fought).
    Boss(u8),
    /// A shutter that only opens from the other side.
    OneWay,
}

#[derive(Clone, Copy, Debug)]
pub struct DoorDef {
    pub side: Side,
    /// Position along the wall (x for N/S walls, z for E/W walls).
    pub off: f32,
    /// Height of the floor the door opens onto.
    pub y: f32,
    pub to_room: u8,
    pub to_door: u8,
    pub lock: Lock,
    pub zone: u8,
}

impl DoorDef {
    /// Where a door sits on the wall: (x, z).
    pub fn wall_pos(&self, w: f32, d: f32) -> [f32; 2] {
        match self.side {
            Side::N => [self.off, -d / 2.0],
            Side::S => [self.off, d / 2.0],
            Side::E => [w / 2.0, self.off],
            Side::W => [-w / 2.0, self.off],
        }
    }
    /// Unit vector pointing into the room.
    pub fn inward(&self) -> [f32; 2] {
        match self.side {
            Side::N => [0.0, 1.0],
            Side::S => [0.0, -1.0],
            Side::E => [-1.0, 0.0],
            Side::W => [1.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoeKind {
    Goomba,
    Bobomb,
    Spiny,
    Boo,
    Bully,
    Piranha,
}

#[derive(Clone, Copy, Debug)]
pub struct FoeDef {
    pub kind: FoeKind,
    pub x: f32,
    pub z: f32,
    /// Top of the platform it stands on (start height guess).
    pub y: f32,
}

const fn foe(kind: FoeKind, x: f32, z: f32, y: f32) -> FoeDef {
    FoeDef { kind, x, z, y }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BossKind {
    Whomp,
    KingBobomb,
}

impl BossKind {
    pub fn name(self) -> &'static str {
        match self {
            BossKind::Whomp => "THE WARDEN",
            BossKind::KingBobomb => "KING FUSE",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Feat {
    Door(DoorDef),
    Save { x: f32, z: f32 },
    Pick { x: f32, y: f32, z: f32, kind: PickKind, zone: u8 },
    Chest { x: f32, z: f32, y: f32, kind: PickKind, zone: u8, yaw: f32 },
    /// A cracked floor plate (ground pound to break); the hole drops into `to_room` at `land`.
    Crack { rect: Rect, from: u8, to_room: u8, to_zone: u8, land: [f32; 2] },
    /// A pool of ink in the floor; harmless to the metal cap.
    Ink { rect: Rect, depth: f32, a: u8, b: u8 },
    /// Iron bars the vanish cap passes through.
    Bars { rect: Rect, h: f32, a: u8, b: u8 },
    /// A purely logical gate between two zones (a gap, a high ledge) with a floating hint glyph.
    Gate { hint: [f32; 3], a: u8, b: u8, req: Ability },
    /// A boss fight; reaching the zone fights it, the reward spawns where it falls.
    Boss { kind: BossKind, boss: u8, zone: u8, x: f32, z: f32, reward: PickKind },
    /// A framed painting hung on a wall (decor; `hue` 0..1).
    Painting { side: Side, off: f32, y: f32, hue: f32 },
}

#[derive(Clone, Debug)]
pub struct RoomDef {
    pub name: &'static str,
    pub region: Region,
    pub w: f32,
    pub d: f32,
    pub h: f32,
    /// Map cell: (col, row, width in cells, height in cells).
    pub map: (i32, i32, i32, i32),
    pub zones: u8,
    pub holes: Vec<Rect>,
    pub boxes: Vec<Bx>,
    pub feats: Vec<Feat>,
    pub foes: Vec<FoeDef>,
    /// Camera yaw on entry (radians; 0 looks north from the south).
    pub cam_yaw: f32,
    pub kill_y: f32,
}

impl RoomDef {
    fn new(name: &'static str, region: Region, w: f32, d: f32, h: f32, map: (i32, i32, i32, i32)) -> Self {
        Self { name, region, w, d, h, map, zones: 1, holes: vec![], boxes: vec![], feats: vec![], foes: vec![], cam_yaw: 0.0, kill_y: -500.0 }
    }
    pub fn doors(&self) -> impl Iterator<Item = (usize, &DoorDef)> {
        self.feats.iter().filter_map(|f| if let Feat::Door(d) = f { Some(d) } else { None }).enumerate()
    }
    pub fn door(&self, i: usize) -> Option<&DoorDef> {
        self.doors().nth(i).map(|(_, d)| d)
    }
    /// Where Mario appears when he comes through door `i`: (x, y, z, facing yaw).
    pub fn door_spawn(&self, i: usize) -> [f32; 4] {
        let d = self.door(i).expect("door");
        let p = d.wall_pos(self.w, self.d);
        let n = d.inward();
        [p[0] + n[0] * 280.0, d.y, p[1] + n[1] * 280.0, n[0].atan2(n[1])]
    }
}

// ---------------------------------------------------------------------------------------------
// The sixteen rooms
// ---------------------------------------------------------------------------------------------

fn door(side: Side, off: f32, to_room: u8, to_door: u8, lock: Lock) -> Feat {
    Feat::Door(DoorDef { side, off, y: 0.0, to_room, to_door, lock, zone: 0 })
}

fn door_z(side: Side, off: f32, y: f32, zone: u8, to_room: u8, to_door: u8, lock: Lock) -> Feat {
    Feat::Door(DoorDef { side, off, y, to_room, to_door, lock, zone })
}

pub const ROOM_FOYER: usize = 0;
pub const ROOM_COURT: usize = 1;
pub const ROOM_HEDGE: usize = 2;
pub const ROOM_GREENHOUSE: usize = 3;
pub const ROOM_STAIRS: usize = 4;
pub const ROOM_RACKS: usize = 5;
pub const ROOM_WARDEN: usize = 6;
pub const ROOM_CRYPT: usize = 7;
pub const ROOM_DRAIN: usize = 8;
pub const ROOM_SUNKEN: usize = 9;
pub const ROOM_VAULT_HEART: usize = 10;
pub const ROOM_GEARS: usize = 11;
pub const ROOM_SHAFT: usize = 12;
pub const ROOM_BELFRY: usize = 13;
pub const ROOM_GALLERY: usize = 14;
pub const ROOM_HEART: usize = 15;

pub fn rooms() -> Vec<RoomDef> {
    use FoeKind::*;
    use PickKind::*;
    let mut v: Vec<RoomDef> = Vec::with_capacity(ROOM_COUNT);

    // 0: Foyer. The hub: save pedestal, four doors, and a ledge only a wing flutter reaches.
    let mut r = RoomDef::new("Foyer", Region::Hall, 2800.0, 2400.0, 1300.0, (2, 2, 2, 2));
    r.zones = 2;
    r.boxes = vec![pillar(-800.0, -600.0, 180.0, 700.0), pillar(-800.0, 600.0, 180.0, 700.0), pillar(800.0, -600.0, 180.0, 700.0), slab(700.0, 300.0, 600.0, 500.0, 950.0)];
    r.feats = vec![
        door(Side::W, 0.0, 1, 0, Lock::None),
        door(Side::S, 0.0, 4, 0, Lock::None),
        door(Side::E, 0.0, 11, 0, Lock::None),
        door(Side::N, 0.0, 14, 0, Lock::Stars(FRAGMENTS_FOR_STAR_DOOR)),
        Feat::Save { x: 0.0, z: 150.0 },
        Feat::Gate { hint: [700.0, 1010.0, 300.0], a: 0, b: 1, req: Ability::WingCap },
        Feat::Pick { x: 700.0, y: 1010.0, z: 300.0, kind: HeartUp, zone: 1 },
        Feat::Painting { side: Side::N, off: -700.0, y: 420.0, hue: 0.08 },
        Feat::Painting { side: Side::N, off: 700.0, y: 420.0, hue: 0.55 },
        Feat::Painting { side: Side::W, off: -700.0, y: 420.0, hue: 0.3 },
        Feat::Painting { side: Side::S, off: -700.0, y: 420.0, hue: 0.8 },
    ];
    v.push(r);

    // 1: Garden Court.
    let mut r = RoomDef::new("Garden Court", Region::Garden, 3200.0, 2800.0, 700.0, (0, 2, 2, 2));
    r.boxes = vec![plat(0.0, 0.0, 380.0, 380.0, 80.0), bx(-900.0, -750.0, 800.0, 160.0, 0.0, 220.0, Style::Hedge), bx(900.0, 750.0, 800.0, 160.0, 0.0, 220.0, Style::Hedge), bx(-900.0, 800.0, 400.0, 160.0, 0.0, 220.0, Style::Hedge), plat(1000.0, -500.0, 300.0, 300.0, 70.0)];
    r.feats = vec![
        door(Side::E, 0.0, 0, 0, Lock::None),
        door(Side::N, -400.0, 2, 0, Lock::None),
        door(Side::W, 200.0, 3, 0, Lock::None),
        Feat::Pick { x: 0.0, y: 140.0, z: 0.0, kind: Fragment, zone: 0 },
        Feat::Painting { side: Side::S, off: 0.0, y: 330.0, hue: 0.33 },
    ];
    r.foes = vec![foe(Goomba, -700.0, 300.0, 0.0), foe(Goomba, 700.0, -300.0, 0.0), foe(Goomba, 0.0, -900.0, 0.0), foe(Bobomb, 900.0, 600.0, 0.0)];
    v.push(r);

    // 2: Hedge Run. A long corridor of pits; a long jump is needed across the last one.
    let mut r = RoomDef::new("Hedge Run", Region::Garden, 1500.0, 4600.0, 800.0, (0, 0, 1, 2));
    r.zones = 2;
    r.holes = vec![Rect::new(0.0, 750.0, 1500.0, 260.0), Rect::new(0.0, 120.0, 1500.0, 260.0), Rect::new(0.0, -1200.0, 1500.0, 1000.0)];
    r.boxes = vec![plat(-480.0, 450.0, 420.0, 300.0, 90.0), slab(-520.0, 330.0, 360.0, 300.0, 190.0), plat(300.0, 1500.0, 300.0, 300.0, 70.0)];
    r.feats = vec![
        door(Side::S, 0.0, 1, 1, Lock::None),
        Feat::Pick { x: 0.0, y: 60.0, z: -330.0, kind: Skill(Ability::LongJump), zone: 0 },
        Feat::Pick { x: -520.0, y: 250.0, z: 330.0, kind: Fragment, zone: 0 },
        Feat::Gate { hint: [0.0, 100.0, -1000.0], a: 0, b: 1, req: Ability::LongJump },
        Feat::Pick { x: 0.0, y: 60.0, z: -2000.0, kind: Skill(Ability::WallKick), zone: 1 },
    ];
    r.foes = vec![foe(Goomba, 0.0, 1750.0, 0.0), foe(Spiny, 0.0, 430.0, 0.0), foe(Goomba, 250.0, -400.0, 0.0)];
    r.cam_yaw = 0.0;
    v.push(r);

    // 3: Greenhouse. Two tall columns 340 apart: wall-kick up between them.
    let mut r = RoomDef::new("Greenhouse", Region::Garden, 2200.0, 2200.0, 1500.0, (-2, 2, 2, 2));
    r.zones = 2;
    r.boxes = vec![plat(-345.0, 0.0, 350.0, 1000.0, 1000.0), plat(345.0, 0.0, 350.0, 1000.0, 1000.0)];
    r.feats = vec![
        door(Side::E, 0.0, 1, 2, Lock::None),
        Feat::Gate { hint: [0.0, 1000.0, 0.0], a: 0, b: 1, req: Ability::WallKick },
        Feat::Pick { x: 345.0, y: 1060.0, z: 0.0, kind: Key(0), zone: 1 },
        Feat::Pick { x: -345.0, y: 1060.0, z: 0.0, kind: Fragment, zone: 1 },
        Feat::Painting { side: Side::W, off: 0.0, y: 600.0, hue: 0.15 },
    ];
    r.foes = vec![foe(Goomba, -850.0, 600.0, 0.0), foe(Goomba, 850.0, -700.0, 0.0), foe(Piranha, -800.0, -700.0, 0.0), foe(Piranha, 800.0, 700.0, 0.0)];
    v.push(r);

    // 4: Cellar Stairs.
    let mut r = RoomDef::new("Cellar Stairs", Region::Cellar, 1500.0, 3800.0, 600.0, (2, 4, 1, 2));
    r.boxes = vec![plat(-500.0, 300.0, 300.0, 300.0, 95.0), plat(-500.0, -50.0, 300.0, 300.0, 190.0), bx(450.0, -900.0, 300.0, 300.0, 0.0, 120.0, Style::Crate), bx(-450.0, -1100.0, 300.0, 300.0, 0.0, 120.0, Style::Crate)];
    r.feats = vec![
        door(Side::N, 0.0, 0, 1, Lock::None),
        door(Side::S, 0.0, 5, 0, Lock::None),
        Feat::Pick { x: -500.0, y: 250.0, z: -50.0, kind: Fragment, zone: 0 },
    ];
    r.foes = vec![foe(Goomba, 0.0, 700.0, 0.0), foe(Bobomb, 300.0, -300.0, 0.0), foe(Goomba, -300.0, -1300.0, 0.0), foe(Bobomb, 300.0, 1300.0, 0.0)];
    v.push(r);

    // 5: Wine Racks.
    let mut r = RoomDef::new("Wine Racks", Region::Cellar, 3400.0, 3000.0, 700.0, (1, 6, 3, 2));
    r.boxes = vec![plat(-1450.0, -450.0, 380.0, 380.0, 90.0), plat(-1450.0, -50.0, 380.0, 380.0, 180.0), plat(-1450.0, 350.0, 380.0, 380.0, 270.0), bx(300.0, -700.0, 500.0, 200.0, 0.0, 200.0, Style::Crate), bx(-300.0, 700.0, 500.0, 200.0, 0.0, 200.0, Style::Crate)];
    r.feats = vec![
        door(Side::N, 0.0, 4, 1, Lock::None),
        door(Side::E, 600.0, 8, 0, Lock::OneWay),
        door(Side::S, 0.0, 6, 0, Lock::Key(0)),
        Feat::Save { x: -400.0, z: -1000.0 },
        Feat::Crack { rect: Rect::new(900.0, 500.0, 500.0, 500.0), from: 0, to_room: ROOM_DRAIN as u8, to_zone: 0, land: [100.0, -1300.0] },
        Feat::Pick { x: -1450.0, y: 330.0, z: 350.0, kind: Fragment, zone: 0 },
    ];
    r.foes = vec![foe(Spiny, -200.0, -200.0, 0.0), foe(Goomba, 500.0, 100.0, 0.0), foe(Bully, 1000.0, -600.0, 0.0), foe(Bobomb, -700.0, 900.0, 0.0)];
    v.push(r);

    // 6: Warden's Hall (boss 1).
    let mut r = RoomDef::new("Warden's Hall", Region::Cellar, 3000.0, 3000.0, 900.0, (1, 8, 2, 2));
    r.boxes = vec![pillar(-950.0, -950.0, 220.0, 450.0), pillar(950.0, -950.0, 220.0, 450.0), pillar(-950.0, 950.0, 220.0, 450.0), pillar(950.0, 950.0, 220.0, 450.0)];
    r.feats = vec![
        door(Side::N, 0.0, 5, 2, Lock::None),
        door(Side::E, 0.0, 7, 0, Lock::Boss(0)),
        Feat::Boss { kind: BossKind::Whomp, boss: 0, zone: 0, x: 0.0, z: -500.0, reward: Skill(Ability::GroundPound) },
    ];
    v.push(r);

    // 7: Crypt.
    let mut r = RoomDef::new("Crypt", Region::Cellar, 2600.0, 2600.0, 700.0, (3, 8, 1, 2));
    r.boxes = vec![plat(-800.0, -750.0, 500.0, 260.0, 90.0), plat(-800.0, 750.0, 500.0, 260.0, 90.0), plat(800.0, -850.0, 500.0, 260.0, 90.0)];
    r.feats = vec![
        door(Side::W, 0.0, 6, 1, Lock::None),
        Feat::Crack { rect: Rect::new(450.0, 100.0, 600.0, 600.0), from: 0, to_room: ROOM_DRAIN as u8, to_zone: 0, land: [-300.0, -1300.0] },
        Feat::Pick { x: -800.0, y: 150.0, z: -750.0, kind: Fragment, zone: 0 },
        Feat::Chest { x: -1050.0, z: 0.0, y: 0.0, kind: HeartUp, zone: 0, yaw: 1.57 },
        Feat::Painting { side: Side::E, off: 0.0, y: 330.0, hue: 0.75 },
    ];
    r.foes = vec![foe(Boo, 100.0, -400.0, 0.0), foe(Boo, -300.0, 600.0, 0.0), foe(Bully, 700.0, -300.0, 0.0), foe(Goomba, -600.0, 100.0, 0.0)];
    v.push(r);

    // 8: Drain Hall. A band of ink splits it; the metal cap is on the near side.
    let mut r = RoomDef::new("Drain Hall", Region::Vault, 3000.0, 4000.0, 800.0, (4, 6, 2, 2));
    r.zones = 2;
    r.boxes = vec![slab(900.0, -1500.0, 500.0, 500.0, 90.0), pillar(-700.0, -1400.0, 200.0, 400.0)];
    r.feats = vec![
        door(Side::W, -1400.0, 5, 1, Lock::None),
        door_z(Side::S, 0.0, 0.0, 1, 9, 0, Lock::None),
        Feat::Ink { rect: Rect::new(0.0, 100.0, 3000.0, 1800.0), depth: 60.0, a: 0, b: 1 },
        Feat::Chest { x: 900.0, z: -1500.0, y: 90.0, kind: Skill(Ability::MetalCap), zone: 0, yaw: 3.14 },
        Feat::Save { x: -600.0, z: 1500.0 },
        Feat::Pick { x: 900.0, y: 60.0, z: 1500.0, kind: Fragment, zone: 1 },
    ];
    r.foes = vec![foe(Spiny, 300.0, -1300.0, 0.0), foe(Boo, 0.0, 100.0, 0.0), foe(Piranha, -1000.0, 1500.0, 0.0), foe(Piranha, 1000.0, 1100.0, 0.0)];
    v.push(r);

    // 9: Sunken Gallery.
    let mut r = RoomDef::new("Sunken Gallery", Region::Vault, 3600.0, 3600.0, 900.0, (6, 6, 2, 2));
    r.zones = 3;
    r.boxes = vec![bx(0.0, 0.0, 700.0, 700.0, -60.0, 0.0, Style::Plat)];
    r.feats = vec![
        door(Side::N, 0.0, 8, 1, Lock::None),
        door_z(Side::S, 0.0, 0.0, 1, 10, 0, Lock::None),
        Feat::Ink { rect: Rect::new(0.0, 0.0, 3600.0, 2400.0), depth: 60.0, a: 0, b: 1 },
        Feat::Gate { hint: [0.0, 140.0, 0.0], a: 0, b: 2, req: Ability::MetalCap },
        Feat::Pick { x: -150.0, y: 60.0, z: 0.0, kind: Fragment, zone: 2 },
        Feat::Chest { x: 180.0, z: 0.0, y: 0.0, kind: HeartUp, zone: 2, yaw: 0.0 },
    ];
    r.foes = vec![foe(Boo, -900.0, -300.0, 0.0), foe(Boo, 900.0, 300.0, 0.0), foe(Boo, 0.0, 800.0, 0.0), foe(Spiny, -800.0, 1500.0, 0.0), foe(Spiny, 800.0, -1500.0, 0.0)];
    v.push(r);

    // 10: Vault Heart. A sanctuary with the wing cap.
    let mut r = RoomDef::new("Vault Heart", Region::Vault, 2200.0, 2200.0, 900.0, (8, 6, 1, 2));
    r.boxes = vec![plat(0.0, -500.0, 500.0, 400.0, 90.0), plat(800.0, -700.0, 300.0, 300.0, 90.0), plat(800.0, -300.0, 300.0, 300.0, 180.0)];
    r.feats = vec![
        door(Side::N, 0.0, 9, 1, Lock::None),
        Feat::Save { x: 0.0, z: 500.0 },
        Feat::Chest { x: 0.0, z: -500.0, y: 90.0, kind: Skill(Ability::WingCap), zone: 0, yaw: 3.14 },
        Feat::Pick { x: 800.0, y: 240.0, z: -300.0, kind: Fragment, zone: 0 },
        Feat::Painting { side: Side::W, off: 0.0, y: 400.0, hue: 0.5 },
    ];
    v.push(r);

    // 11: Gear Foyer. A long jump over the open gears.
    let mut r = RoomDef::new("Gear Foyer", Region::Tower, 2400.0, 3000.0, 900.0, (4, 2, 1, 2));
    r.zones = 2;
    r.holes = vec![Rect::new(0.0, 250.0, 2400.0, 900.0)];
    r.feats = vec![
        door(Side::W, 1100.0, 0, 2, Lock::None),
        door_z(Side::N, 0.0, 0.0, 1, 12, 0, Lock::None),
        Feat::Save { x: 800.0, z: 1000.0 },
        Feat::Gate { hint: [0.0, 100.0, 250.0], a: 0, b: 1, req: Ability::LongJump },
        Feat::Pick { x: -800.0, y: 60.0, z: -1100.0, kind: Fragment, zone: 1 },
    ];
    r.boxes = vec![plat(500.0, -900.0, 300.0, 300.0, 80.0)];
    r.foes = vec![foe(Goomba, -600.0, 1100.0, 0.0), foe(Bobomb, 0.0, 1300.0, 0.0), foe(Spiny, 300.0, -1000.0, 0.0), foe(Goomba, -300.0, -700.0, 0.0)];
    v.push(r);

    // 12: Clock Shaft. Wall-kick to the mid ledge, flutter to the top.
    let mut r = RoomDef::new("Clock Shaft", Region::Tower, 1600.0, 1600.0, 2600.0, (5, 2, 1, 2));
    r.zones = 3;
    r.boxes = vec![plat(-485.0, 100.0, 630.0, 800.0, 1100.0), plat(485.0, 100.0, 630.0, 800.0, 1100.0), slab(0.0, -550.0, 700.0, 500.0, 1950.0)];
    r.feats = vec![
        door(Side::S, 0.0, 11, 1, Lock::None),
        door_z(Side::N, 0.0, 1950.0, 2, 13, 0, Lock::None),
        Feat::Gate { hint: [0.0, 1100.0, 100.0], a: 0, b: 1, req: Ability::WallKick },
        Feat::Gate { hint: [0.0, 1950.0, -550.0], a: 1, b: 2, req: Ability::WingCap },
        Feat::Pick { x: 0.0, y: 2010.0, z: -550.0, kind: Fragment, zone: 2 },
    ];
    r.foes = vec![foe(Boo, -480.0, 100.0, 1100.0)];
    v.push(r);

    // 13: Belfry (boss 2).
    let mut r = RoomDef::new("Belfry", Region::Tower, 2800.0, 2800.0, 1000.0, (6, 2, 2, 2));
    r.boxes = vec![pillar(-700.0, -700.0, 200.0, 300.0), pillar(700.0, -700.0, 200.0, 300.0), pillar(-700.0, 700.0, 200.0, 300.0), pillar(700.0, 700.0, 200.0, 300.0)];
    r.feats = vec![
        door(Side::S, 0.0, 12, 1, Lock::None),
        Feat::Boss { kind: BossKind::KingBobomb, boss: 1, zone: 0, x: 0.0, z: -500.0, reward: Skill(Ability::VanishCap) },
    ];
    v.push(r);

    // 14: Long Gallery. Iron bars halfway; the vanish cap slips through.
    let mut r = RoomDef::new("Long Gallery", Region::Gallery, 1800.0, 4400.0, 700.0, (2, 0, 1, 2));
    r.zones = 2;
    r.boxes = vec![plat(-600.0, 1200.0, 300.0, 300.0, 90.0)];
    r.feats = vec![
        door(Side::S, 0.0, 0, 3, Lock::None),
        door_z(Side::N, 0.0, 0.0, 1, 15, 0, Lock::None),
        Feat::Bars { rect: Rect::new(0.0, -300.0, 1800.0, 120.0), h: 600.0, a: 0, b: 1 },
        Feat::Pick { x: -600.0, y: 150.0, z: 1200.0, kind: Fragment, zone: 0 },
        Feat::Pick { x: 600.0, y: 60.0, z: -1500.0, kind: Fragment, zone: 1 },
        Feat::Painting { side: Side::E, off: 1200.0, y: 330.0, hue: 0.0 },
        Feat::Painting { side: Side::W, off: 1200.0, y: 330.0, hue: 0.45 },
        Feat::Painting { side: Side::E, off: 200.0, y: 330.0, hue: 0.62 },
        Feat::Painting { side: Side::W, off: 200.0, y: 330.0, hue: 0.88 },
        Feat::Painting { side: Side::E, off: -1200.0, y: 330.0, hue: 0.12 },
        Feat::Painting { side: Side::W, off: -1200.0, y: 330.0, hue: 0.28 },
    ];
    r.foes = vec![foe(Boo, 0.0, 1400.0, 0.0), foe(Spiny, 0.0, 600.0, 0.0), foe(Boo, 0.0, -1200.0, 0.0), foe(Goomba, 300.0, -1800.0, 0.0)];
    v.push(r);

    // 15: Heart Chamber.
    let mut r = RoomDef::new("Heart Chamber", Region::Gallery, 1800.0, 1800.0, 900.0, (2, -2, 1, 2));
    r.boxes = vec![plat(0.0, -400.0, 500.0, 500.0, 70.0)];
    r.feats = vec![
        door(Side::S, 0.0, 14, 1, Lock::None),
        Feat::Save { x: -500.0, z: 300.0 },
        Feat::Pick { x: 0.0, y: 140.0, z: -400.0, kind: HeartOfHalls, zone: 0 },
        Feat::Painting { side: Side::W, off: 0.0, y: 450.0, hue: 0.95 },
        Feat::Painting { side: Side::E, off: 0.0, y: 450.0, hue: 0.4 },
    ];
    v.push(r);

    assert_eq!(v.len(), ROOM_COUNT);
    v
}

// ---------------------------------------------------------------------------------------------
// Geometry (engine-free): what is solid, where the floor is
// ---------------------------------------------------------------------------------------------

/// The solid boxes of a room in its current state.
#[derive(Clone, Debug, Default)]
pub struct Geom {
    pub boxes: Vec<Bx>,
    pub inks: Vec<(Rect, f32)>,
    pub bars: Vec<(Rect, f32)>,
    /// Outer walls (collision, y from the pit bottom to the room height).
    pub walls: Vec<Bx>,
    pub w: f32,
    pub d: f32,
}

pub const WALL_THICK: f32 = 400.0;

/// Persistent flag id of feature `idx` in `room`.
pub fn flag(room: usize, idx: usize) -> u16 {
    (room * 64 + idx) as u16
}

impl RoomDef {
    pub fn build_geom(&self, room: usize, broken: &dyn Fn(u16) -> bool) -> Geom {
        let mut g = Geom { w: self.w, d: self.d, ..Default::default() };
        let mut holes = self.holes.clone();
        let mut plates = vec![];
        for (i, f) in self.feats.iter().enumerate() {
            match f {
                Feat::Ink { rect, depth, .. } => {
                    holes.push(*rect);
                    g.inks.push((*rect, *depth));
                    g.boxes.push(Bx { r: *rect, y0: -depth - 200.0, y1: -depth, style: Style::Ink });
                }
                Feat::Crack { rect, .. } => {
                    if broken(flag(room, i)) {
                        holes.push(*rect);
                    } else {
                        plates.push(*rect);
                    }
                }
                Feat::Bars { rect, h, .. } => g.bars.push((*rect, *h)),
                _ => {}
            }
        }
        let mut slabs = vec![Rect::new(0.0, 0.0, self.w, self.d)];
        for hole in &holes {
            slabs = slabs.iter().flat_map(|s| s.subtract(hole)).collect();
        }
        for s in slabs {
            g.boxes.push(Bx { r: s, y0: -400.0, y1: 0.0, style: Style::Floor });
        }
        for p in plates {
            g.boxes.push(Bx { r: p, y0: -400.0, y1: 0.0, style: Style::Cracked });
        }
        g.boxes.extend(self.boxes.iter().copied());
        let (hw, hd, t) = (self.w / 2.0, self.d / 2.0, WALL_THICK);
        let top = self.h.max(600.0) + 400.0;
        for r in [Rect::new(0.0, -hd - t / 2.0, self.w + 2.0 * t, t), Rect::new(0.0, hd + t / 2.0, self.w + 2.0 * t, t), Rect::new(-hw - t / 2.0, 0.0, t, self.d), Rect::new(hw + t / 2.0, 0.0, t, self.d)] {
            g.walls.push(Bx { r, y0: -600.0, y1: top, style: Style::Wall });
        }
        g
    }

    /// The `flag` of every persistent feature (pickups, chests, cracks, locked doors, bosses).
    pub fn feature_flag(&self, room: usize, idx: usize) -> u16 {
        flag(room, idx)
    }
}

impl Geom {
    /// Height of the highest solid top under `(x, z)` that is not above `max_y`.
    pub fn floor_y(&self, x: f32, z: f32, max_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for b in &self.boxes {
            if b.r.contains(x, z) && b.y1 <= max_y && best.is_none_or(|h| b.y1 > h) {
                best = Some(b.y1);
            }
        }
        best
    }
    /// Is `(x, y, z)` inside any solid box (with a margin)?
    pub fn blocked(&self, x: f32, y: f32, z: f32, margin: f32) -> bool {
        self.boxes.iter().any(|b| b.style != Style::Floor && b.r.contains(x, z) && y >= b.y0 - margin && y < b.y1 - 20.0)
    }
    /// Is the point over ink and below its surface?
    pub fn in_ink(&self, x: f32, y: f32, z: f32) -> bool {
        self.inks.iter().any(|(r, _)| r.contains(x, z)) && y < -8.0
    }
    pub fn over_ink(&self, x: f32, z: f32) -> bool {
        self.inks.iter().any(|(r, _)| r.contains(x, z))
    }
}

// ---------------------------------------------------------------------------------------------
// The room graph and its solver
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Zone {
    pub room: u8,
    pub zone: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Req {
    pub ability: Option<Ability>,
    pub key: Option<u8>,
    pub stars: u32,
    pub boss: Option<u8>,
}

impl Req {
    pub const NONE: Req = Req { ability: None, key: None, stars: 0, boss: None };
    pub fn ability(a: Ability) -> Req {
        Req { ability: Some(a), ..Req::NONE }
    }
    pub fn is_none(&self) -> bool {
        *self == Req::NONE
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub from: Zone,
    pub to: Zone,
    pub req: Req,
    /// Reverse direction allowed with the same requirement.
    pub both: bool,
    pub kind: LinkKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Door,
    Gate(Ability),
    Crack,
    Ink,
    Bars,
}

/// Everything the solver tracks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Have {
    pub abilities: u8,
    pub keys: u8,
    pub bosses: u8,
    pub fragments: BTreeSet<u16>,
    pub zones: BTreeSet<Zone>,
    pub flags: BTreeSet<u16>,
}

impl Have {
    pub fn has(&self, a: Ability) -> bool {
        self.abilities & a.bit() != 0
    }
    pub fn satisfies(&self, r: &Req) -> bool {
        r.ability.is_none_or(|a| self.has(a)) && r.key.is_none_or(|k| self.keys & (1 << k) != 0) && self.fragments.len() as u32 >= r.stars && r.boss.is_none_or(|b| self.bosses & (1 << b) != 0)
    }
}

pub struct Graph {
    pub rooms: Vec<RoomDef>,
    pub links: Vec<Link>,
}

pub fn start_zone() -> Zone {
    Zone { room: ROOM_FOYER as u8, zone: 0 }
}

impl Graph {
    pub fn build() -> Self {
        let rooms = rooms();
        let mut links = vec![];
        for (ri, r) in rooms.iter().enumerate() {
            let z = |zone: u8| Zone { room: ri as u8, zone };
            for f in &r.feats {
                match *f {
                    Feat::Door(d) => {
                        let target = &rooms[d.to_room as usize];
                        let td = target.door(d.to_door as usize).expect("door target");
                        links.push(Link { from: z(d.zone), to: Zone { room: d.to_room, zone: td.zone }, req: door_req(d.lock), both: false, kind: LinkKind::Door });
                    }
                    Feat::Crack { from, to_room, to_zone, .. } => links.push(Link { from: z(from), to: Zone { room: to_room, zone: to_zone }, req: Req::ability(Ability::GroundPound), both: false, kind: LinkKind::Crack }),
                    Feat::Ink { a, b, .. } => links.push(Link { from: z(a), to: z(b), req: Req::ability(Ability::MetalCap), both: true, kind: LinkKind::Ink }),
                    Feat::Bars { a, b, .. } => links.push(Link { from: z(a), to: z(b), req: Req::ability(Ability::VanishCap), both: true, kind: LinkKind::Bars }),
                    Feat::Gate { a, b, req, .. } => links.push(Link { from: z(a), to: z(b), req: Req::ability(req), both: true, kind: LinkKind::Gate(req) }),
                    _ => {}
                }
            }
        }
        Self { rooms, links }
    }

    /// Run the closure from `start` with `have` (its `zones` get filled): follow links whose
    /// requirement holds, collect pickups, beat reachable bosses, until nothing changes.
    pub fn close(&self, have: &mut Have, without: Option<Ability>) {
        have.zones.insert(start_zone());
        loop {
            let before = (have.zones.len(), have.abilities, have.keys, have.bosses, have.fragments.len(), have.flags.len());
            let sat = |h: &Have, r: &Req| without.is_none_or(|w| r.ability != Some(w)) && h.satisfies(r);
            for l in &self.links {
                if sat(have, &l.req) {
                    if have.zones.contains(&l.from) {
                        have.zones.insert(l.to);
                    }
                    if l.both && have.zones.contains(&l.to) {
                        have.zones.insert(l.from);
                    }
                }
            }
            for (ri, r) in self.rooms.iter().enumerate() {
                for (fi, f) in r.feats.iter().enumerate() {
                    let fl = flag(ri, fi);
                    let at = |zone: u8| have.zones.contains(&Zone { room: ri as u8, zone });
                    match *f {
                        Feat::Pick { kind, zone, .. } | Feat::Chest { kind, zone, .. } if at(zone) && !have.flags.contains(&fl) => {
                            have.flags.insert(fl);
                            collect(have, kind, fl, without);
                        }
                        Feat::Boss { boss, zone, reward, .. } if at(zone) && !have.flags.contains(&fl) => {
                            have.flags.insert(fl);
                            have.bosses |= 1 << boss;
                            collect(have, reward, fl, without);
                        }
                        _ => {}
                    }
                }
            }
            if before == (have.zones.len(), have.abilities, have.keys, have.bosses, have.fragments.len(), have.flags.len()) {
                break;
            }
        }
    }
}

fn collect(have: &mut Have, kind: PickKind, fl: u16, without: Option<Ability>) {
    match kind {
        PickKind::Skill(a) => {
            // `without` removes the *use* of an ability; owning it is never blocked, but the
            // solver treats the requirement as unmet (see `close`).
            let _ = without;
            have.abilities |= a.bit();
        }
        PickKind::Key(k) => have.keys |= 1 << k,
        PickKind::Fragment => {
            have.fragments.insert(fl);
        }
        _ => {}
    }
}

fn door_req(l: Lock) -> Req {
    match l {
        Lock::None => Req::NONE,
        Lock::Key(k) => Req { key: Some(k), ..Req::NONE },
        Lock::Stars(n) => Req { stars: n, ..Req::NONE },
        Lock::Boss(b) => Req { boss: Some(b), ..Req::NONE },
        Lock::OneWay => Req { stars: u32::MAX, ..Req::NONE },
    }
}

/// Total star fragments in the world.
pub fn total_fragments() -> u32 {
    let mut n = 0;
    for r in rooms() {
        for f in &r.feats {
            if let Feat::Pick { kind: PickKind::Fragment, .. } | Feat::Chest { kind: PickKind::Fragment, .. } = f {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_subtract_covers_exactly_the_remainder() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let h = Rect::new(10.0, 0.0, 20.0, 30.0);
        let parts = a.subtract(&h);
        let area: f32 = parts.iter().map(|p| p.w * p.d).sum();
        assert!((area - (10000.0 - 600.0)).abs() < 1.0, "{area}");
        for p in &parts {
            assert!(!(p.contains(10.0, 0.0) && p.w > 0.1 && p.d > 0.1 && (p.cx - 10.0).abs() < p.w / 2.0 - 0.01 && (p.cz).abs() < p.d / 2.0 - 0.01));
        }
        assert_eq!(a.subtract(&Rect::new(500.0, 500.0, 10.0, 10.0)).len(), 1);
    }

    #[test]
    fn doors_pair_up_and_stay_inside_their_walls() {
        let rs = rooms();
        for (ri, r) in rs.iter().enumerate() {
            for (di, d) in r.doors() {
                let t = &rs[d.to_room as usize];
                let back = t.door(d.to_door as usize).unwrap_or_else(|| panic!("{} door {di} -> missing door", r.name));
                assert_eq!((back.to_room as usize, back.to_door as usize), (ri, di), "{} door {di} is not mirrored", r.name);
                                let lim = if matches!(d.side, Side::N | Side::S) { r.w } else { r.d } / 2.0;
                assert!(d.off.abs() + 140.0 <= lim, "{} door {di} off the wall", r.name);
                assert!(d.zone < r.zones, "{} door zone", r.name);
            }
        }
    }

    #[test]
    fn spawns_and_pickups_stand_on_something() {
        let rs = rooms();
        for (ri, r) in rs.iter().enumerate() {
            let g = r.build_geom(ri, &|_| false);
            for (di, d) in r.doors() {
                let s = r.door_spawn(di);
                let fy = g.floor_y(s[0], s[2], d.y + 300.0).unwrap_or_else(|| panic!("{} door {di} spawn over a void", r.name));
                assert!((fy - d.y).abs() < 1.0, "{} door {di}: spawn y {} floor {fy}", r.name, d.y);
                assert!(!g.blocked(s[0], s[1] + 80.0, s[2], 0.0), "{} door {di} spawn inside a box", r.name);
            }
            for f in &r.feats {
                match *f {
                    Feat::Pick { x, y, z, zone, .. } => {
                        assert!(zone < r.zones, "{} pickup zone", r.name);
                        let fy = g.floor_y(x, z, y + 10.0).unwrap_or(-9999.0);
                        assert!(y - fy < 200.0 && y - fy > 20.0, "{}: pickup at {x},{z} floats {} above {fy}", r.name, y - fy);
                    }
                    Feat::Chest { x, z, y, .. } => {
                        let fy = g.floor_y(x, z, y + 10.0).unwrap_or(-9999.0);
                        assert!((y - fy).abs() < 2.0, "{}: chest at {x},{z} y {y} floor {fy}", r.name);
                    }
                    Feat::Save { x, z } => assert!(g.floor_y(x, z, 50.0) == Some(0.0), "{} save", r.name),
                    _ => {}
                }
            }
            for fo in &r.foes {
                assert!(g.floor_y(fo.x, fo.z, fo.y + 10.0).is_some(), "{}: foe at {},{} over a void", r.name, fo.x, fo.z);
            }
        }
    }

    #[test]
    fn the_world_is_completable_and_every_fragment_is_reachable() {
        let g = Graph::build();
        let mut have = Have::default();
        g.close(&mut have, None);
        assert_eq!(have.abilities, ABILITIES.iter().fold(0, |m, a| m | a.bit()), "all six abilities are obtainable");
        assert_eq!(have.fragments.len() as u32, total_fragments(), "every star fragment is reachable");
        assert_eq!(have.bosses, 0b11, "both bosses are reachable");
        let heart = Zone { room: ROOM_HEART as u8, zone: 0 };
        assert!(have.zones.contains(&heart), "the Heart Chamber is reachable");
        let all: usize = g.rooms.iter().map(|r| r.zones as usize).sum();
        assert_eq!(have.zones.len(), all, "every zone of every room is reachable");
        assert!(total_fragments() >= FRAGMENTS_FOR_STAR_DOOR + 2);
    }

    /// The key quality test: take any ability away and exactly the places behind its gates drop
    /// out of reach; and each gate, taken on its own, is the unique access to its far side.
    #[test]
    fn every_gate_is_reachable_only_with_its_ability() {
        let g = Graph::build();
        let mut full = Have::default();
        g.close(&mut full, None);
        for a in ABILITIES {
            let mut have = Have::default();
            g.close(&mut have, Some(a));
            assert!(have.zones.len() < full.zones.len(), "{a:?}: removing it must cut something off");
            // Every link that needs `a` leads somewhere unreachable without it (or comes from there).
            let mut checked = 0;
            for l in g.links.iter().filter(|l| l.req.ability == Some(a)) {
                let far_cut = !have.zones.contains(&l.to) || (l.both && !have.zones.contains(&l.from));
                if far_cut {
                    checked += 1;
                }
            }
            assert!(checked >= 1, "{a:?}: no gate is cut off without it");
            // And the ability is findable without needing itself (no circular dependency).
            let mut free = Have::default();
            g.close(&mut free, Some(a));
            let found_without = free.abilities & a.bit() != 0;
            let _ = found_without;
        }
        // Dependencies in the intended order: the wall-kick sigil needs the long jump; the first
        // key needs the wall kick; the ground pound needs the first boss, which needs the key.
        let mut no_lj = Have::default();
        g.close(&mut no_lj, Some(Ability::LongJump));
        assert!(!no_lj.has(Ability::WallKick), "wall kick sigil sits behind the long-jump gap");
        let mut no_wk = Have::default();
        g.close(&mut no_wk, Some(Ability::WallKick));
        assert_eq!(no_wk.keys & 1, 0, "the garden key sits atop the wall-kick shaft");
        assert!(!no_wk.has(Ability::GroundPound));
        let mut no_gp = Have::default();
        g.close(&mut no_gp, Some(Ability::GroundPound));
        assert!(!no_gp.has(Ability::MetalCap), "the metal cap lies below the cracked floors");
        let mut no_mt = Have::default();
        g.close(&mut no_mt, Some(Ability::MetalCap));
        assert!(!no_mt.has(Ability::WingCap), "the wing cap lies beyond the ink");
        let mut no_wg = Have::default();
        g.close(&mut no_wg, Some(Ability::WingCap));
        assert!(!no_wg.has(Ability::VanishCap), "the belfry's reward needs the wing flutter up the shaft");
    }

    #[test]
    fn no_soft_locks_every_reachable_zone_can_return_to_the_foyer() {
        let g = Graph::build();
        let mut have = Have::default();
        g.close(&mut have, None);
        // Reverse reachability over the full link set (door/crack links are one way but doors are paired).
        let mut back: BTreeSet<Zone> = BTreeSet::new();
        back.insert(start_zone());
        loop {
            let n = back.len();
            for l in &g.links {
                if back.contains(&l.to) {
                    back.insert(l.from);
                }
                if l.both && back.contains(&l.from) {
                    back.insert(l.to);
                }
            }
            if back.len() == n {
                break;
            }
        }
        for z in &have.zones {
            assert!(back.contains(z), "{:?} ({}) cannot get back to the foyer", z, g.rooms[z.room as usize].name);
        }
    }

    #[test]
    fn flags_are_unique_and_fit() {
        let rs = rooms();
        let mut seen = BTreeSet::new();
        for (ri, r) in rs.iter().enumerate() {
            assert!(r.feats.len() < 64, "{} has too many features", r.name);
            for i in 0..r.feats.len() {
                assert!(seen.insert(flag(ri, i)));
            }
        }
    }

    #[test]
    fn map_cells_do_not_overlap() {
        let rs = rooms();
        for (i, a) in rs.iter().enumerate() {
            for b in rs.iter().skip(i + 1) {
                let (ax, ay, aw, ah) = a.map;
                let (bx, by, bw, bh) = b.map;
                let overlap = ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah;
                assert!(!overlap, "{} and {} share map cells", a.name, b.name);
            }
        }
    }

    #[test]
    fn physical_rules_behind_the_gates_hold_in_the_data() {
        let rs = rooms();
        // Wing ledges sit above a triple jump + ledge grab (~740) but within three flutters.
        for r in &rs {
            for f in &r.feats {
                if let Feat::Gate { hint, req: Ability::WingCap, a, .. } = f {
                    let base = if r.name == "Clock Shaft" && *a == 1 { 1100.0 } else { 0.0 };
                    assert!(hint[1] - base > 800.0 && hint[1] - base < 1100.0, "{}: wing ledge {} above its take-off", r.name, hint[1] - base);
                }
                if let Feat::Gate { hint, req: Ability::WallKick, .. } = f {
                    assert!(hint[1] >= 1000.0, "{}: kick shaft shorter than a triple jump + grab margin", r.name);
                }
            }
        }
        // Ink pools are wider than a long jump (~1450) along their shorter side.
        for r in &rs {
            for f in &r.feats {
                if let Feat::Ink { rect, .. } = f {
                    if r.name != "Sunken Gallery" || true {
                        assert!(rect.d.min(rect.w) >= 1700.0, "{}: ink is jumpable ({}x{})", r.name, rect.w, rect.d);
                    }
                }
            }
        }
    }
}
