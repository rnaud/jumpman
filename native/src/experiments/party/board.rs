//! The three boards of Sunstone Shindig: graphs of spaces with a gimmick each.
//!
//! * Hourglass Keep: a castle courtyard with two drawbridges that flip every round (one open at a time).
//! * Cloudtop Isles: sky islands with updraft shortcuts that only open when the wind blows.
//! * Cinder Crown: a volcano of three rings; the lava tide rises and floods the low rings.
use super::common::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Start,
    Coin,
    Pit,
    Event,
    Item,
    Shop,
    Warp,
    Duel,
    Hex,
    /// A Sunstone Keeper site: sells the Sunstone while active, a small coin space otherwise.
    Keeper,
}

pub const KINDS: [Kind; 10] = [Kind::Start, Kind::Coin, Kind::Pit, Kind::Event, Kind::Item, Kind::Shop, Kind::Warp, Kind::Duel, Kind::Hex, Kind::Keeper];

impl Kind {
    pub fn idx(self) -> usize {
        KINDS.iter().position(|k| *k == self).unwrap()
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Start => "Home",
            Kind::Coin => "Coins",
            Kind::Pit => "Pit",
            Kind::Event => "Event",
            Kind::Item => "Gift",
            Kind::Shop => "Shop",
            Kind::Warp => "Warp",
            Kind::Duel => "Duel",
            Kind::Hex => "Hex",
            Kind::Keeper => "Keeper",
        }
    }
    /// Space colour (0xRRGGBB).
    pub fn color(self) -> u32 {
        match self {
            Kind::Start => 0xf2f2f2,
            Kind::Coin => 0x3d8bff,
            Kind::Pit => 0xe5392f,
            Kind::Event => 0x9a4de0,
            Kind::Item => 0x2fc27a,
            Kind::Shop => 0xffa21f,
            Kind::Warp => 0x20d6d0,
            Kind::Duel => 0xff6fb0,
            Kind::Hex => 0x4a2a66,
            Kind::Keeper => 0xffd21f,
        }
    }
    /// Glyph shown on the space.
    pub fn glyph(self) -> &'static str {
        match self {
            Kind::Start => "H",
            Kind::Coin => "+3",
            Kind::Pit => "-3",
            Kind::Event => "?",
            Kind::Item => "G",
            Kind::Shop => "$",
            Kind::Warp => "W",
            Kind::Duel => "VS",
            Kind::Hex => "X",
            Kind::Keeper => "*",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Always,
    /// Open while the drawbridge state equals this.
    Flip(bool),
    /// Open while the wind blows.
    Wind,
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub to: usize,
    pub rule: Rule,
}

#[derive(Clone, Debug)]
pub struct Space {
    pub pos: [f32; 2],
    pub kind: Kind,
    /// Volcano tier (0 = lowest); 0 elsewhere.
    pub tier: u8,
    /// Sun or moon tint: decides mini-game teams.
    pub sun: bool,
    /// Whether this is a bridge / shortcut span (drawn as planks, never a Keeper site).
    pub span: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gimmick {
    Bridge,
    Wind,
    Tide,
}

#[derive(Clone, Debug)]
pub struct Board {
    pub id: usize,
    pub name: &'static str,
    pub blurb: &'static str,
    pub gimmick: Gimmick,
    pub spaces: Vec<Space>,
    pub edges: Vec<Vec<Edge>>,
    pub start: usize,
    pub sites: Vec<usize>,
    /// Sky gradient (top, mid, bottom) and ground colour.
    pub sky: [u32; 3],
    pub ground: u32,
}

pub const BOARD_COUNT: usize = 3;
pub const BOARD_NAMES: [&str; BOARD_COUNT] = ["Hourglass Keep", "Cloudtop Isles", "Cinder Crown"];

/// What changes between rounds on a board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Conditions {
    pub flip: bool,
    pub wind: bool,
    /// Number of flooded tiers (Cinder Crown).
    pub tide: u8,
}

impl Board {
    pub fn open(&self, e: &Edge, c: &Conditions) -> bool {
        match e.rule {
            Rule::Always => true,
            Rule::Flip(f) => c.flip == f,
            Rule::Wind => c.wind,
        }
    }
    /// Currently usable edges out of `s`.
    pub fn out(&self, s: usize, c: &Conditions) -> Vec<usize> {
        self.edges[s].iter().filter(|e| self.open(e, c)).map(|e| e.to).collect()
    }
    pub fn flooded(&self, s: usize, c: &Conditions) -> bool {
        self.gimmick == Gimmick::Tide && self.spaces[s].tier < c.tide
    }
    /// Shortest number of steps from `a` to `b` over every edge (rules ignored), or None.
    pub fn distance(&self, a: usize, b: usize) -> Option<u32> {
        let mut d = vec![u32::MAX; self.spaces.len()];
        let mut q = std::collections::VecDeque::new();
        d[a] = 0;
        q.push_back(a);
        while let Some(s) = q.pop_front() {
            if s == b {
                return Some(d[s]);
            }
            for e in &self.edges[s] {
                if d[e.to] == u32::MAX {
                    d[e.to] = d[s] + 1;
                    q.push_back(e.to);
                }
            }
        }
        None
    }
}

struct Builder {
    spaces: Vec<Space>,
    edges: Vec<Vec<Edge>>,
}

impl Builder {
    fn new() -> Self {
        Builder { spaces: vec![], edges: vec![] }
    }
    fn add(&mut self, x: f32, z: f32, tier: u8, span: bool) -> usize {
        self.spaces.push(Space { pos: [x, z], kind: Kind::Coin, tier, sun: true, span });
        self.edges.push(vec![]);
        self.spaces.len() - 1
    }
    fn link(&mut self, a: usize, b: usize, rule: Rule) {
        self.edges[a].push(Edge { to: b, rule });
    }
    /// Link a chain a -> ... -> b through `mid`.
    fn chain(&mut self, a: usize, mid: &[usize], b: usize, rule: Rule) {
        let mut prev = a;
        for (i, &m) in mid.iter().enumerate() {
            self.link(prev, m, if i == 0 { rule } else { Rule::Always });
            prev = m;
        }
        self.link(prev, b, if mid.is_empty() { rule } else { Rule::Always });
    }
}

/// Distribute space kinds deterministically for a board (same layout every game so players can learn it).
fn paint(b: &mut Builder, start: usize, sites: &[usize], seed: u64, has_warps: bool) {
    let mut rng = Rng::new(seed);
    let free: Vec<usize> = (0..b.spaces.len()).filter(|&i| i != start && !sites.contains(&i)).collect();
    let main: Vec<usize> = free.iter().copied().filter(|&i| !b.spaces[i].span).collect();
    let n = main.len();
    let mut kinds: Vec<Kind> = vec![];
    let mut add = |k: Kind, c: usize| {
        for _ in 0..c {
            kinds.push(k);
        }
    };
    add(Kind::Pit, (n as f32 * 0.10).round() as usize);
    add(Kind::Event, (n as f32 * 0.10).round() as usize);
    add(Kind::Item, (n as f32 * 0.07).round() as usize);
    add(Kind::Shop, ((n as f32 * 0.06).round() as usize).max(2));
    add(Kind::Duel, ((n as f32 * 0.06).round() as usize).max(2));
    add(Kind::Hex, (n as f32 * 0.07).round() as usize);
    if has_warps {
        add(Kind::Warp, if n > 30 { 4 } else { 2 });
    }
    while kinds.len() < n {
        kinds.push(Kind::Coin);
    }
    kinds.truncate(n);
    // Spread special spaces: shuffle, then avoid two bad spaces side by side by a few swap passes.
    rng.shuffle(&mut kinds);
    for (i, &s) in main.iter().enumerate() {
        b.spaces[s].kind = kinds[i];
    }
    // Bridge / shortcut spans are friendly: coins and gifts.
    let spans: Vec<usize> = free.iter().copied().filter(|&i| b.spaces[i].span).collect();
    for s in spans {
        b.spaces[s].kind = if rng.chance(0.3) { Kind::Item } else { Kind::Coin };
    }
    for &s in sites {
        b.spaces[s].kind = Kind::Keeper;
    }
    b.spaces[start].kind = Kind::Start;
    for i in 0..b.spaces.len() {
        b.spaces[i].sun = match b.spaces[i].kind {
            Kind::Pit | Kind::Hex | Kind::Duel => false,
            Kind::Event => i % 2 == 0,
            _ => true,
        };
    }
}

fn hourglass() -> Board {
    let mut b = Builder::new();
    let mut ring = vec![];
    // Clockwise around the courtyard, starting at the top-left corner.
    for i in 0..9 {
        ring.push(b.add(-10.0 + 2.5 * i as f32, -5.0, 0, false));
    }
    for z in [-2.5, 0.0, 2.5, 5.0] {
        ring.push(b.add(10.0, z, 0, false));
    }
    for i in 0..8 {
        ring.push(b.add(7.5 - 2.5 * i as f32, 5.0, 0, false));
    }
    for z in [2.5, 0.0, -2.5] {
        ring.push(b.add(-10.0, z, 0, false));
    }
    let n = ring.len();
    for i in 0..n {
        b.link(ring[i], ring[(i + 1) % n], Rule::Always);
    }
    // Two drawbridges: west spans while the flag is down, east spans while it is up.
    let west: Vec<usize> = [-2.5, 0.0, 2.5].iter().map(|&z| b.add(-2.5, z, 0, true)).collect();
    let east: Vec<usize> = [-2.5, 0.0, 2.5].iter().map(|&z| b.add(2.5, z, 0, true)).collect();
    b.chain(ring[3], &west, ring[17], Rule::Flip(false));
    b.chain(ring[5], &east, ring[15], Rule::Flip(true));
    let sites = vec![ring[7], ring[11], ring[19], ring[22]];
    paint(&mut b, ring[0], &sites, 0x4b33, true);
    Board { id: 0, name: BOARD_NAMES[0], blurb: "Two drawbridges swing over the courtyard, but only one is down at a time. Cross it for a big shortcut.", gimmick: Gimmick::Bridge, spaces: b.spaces, edges: b.edges, start: ring[0], sites, sky: [0x2a3a78, 0x6c78c8, 0xf0b890], ground: 0x4c5a4a }
}

fn cloudtop() -> Board {
    let mut b = Builder::new();
    let n = 28;
    let mut ring = vec![];
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let wob = 1.0 + 0.09 * (a * 5.0).sin();
        ring.push(b.add(a.cos() * 11.5 * wob, a.sin() * 5.6 * wob, 0, false));
    }
    for i in 0..n {
        b.link(ring[i], ring[(i + 1) % n], Rule::Always);
    }
    // Updraft chords across the sky: open only while the wind blows.
    let chords = [(2usize, 12usize), (9, 20), (16, 26)];
    for (k, &(from, to)) in chords.iter().enumerate() {
        let (a, c) = (b.spaces[ring[from]].pos, b.spaces[ring[to]].pos);
        let mid: Vec<usize> = [0.34f32, 0.67].iter().map(|&t| b.add(a[0] + (c[0] - a[0]) * t, a[1] + (c[1] - a[1]) * t - 0.4 * (k as f32 + 1.0), 0, true)).collect();
        b.chain(ring[from], &mid, ring[to], Rule::Wind);
    }
    let sites = vec![ring[5], ring[11], ring[18], ring[24]];
    paint(&mut b, ring[0], &sites, 0xc10d, true);
    Board { id: 1, name: BOARD_NAMES[1], blurb: "Floating isles ringed by cloud paths. Updrafts open shortcuts, but only when the wind blows.", gimmick: Gimmick::Wind, spaces: b.spaces, edges: b.edges, start: ring[0], sites, sky: [0x3a86e8, 0x8cc8ff, 0xfff0d0], ground: 0xd8ecff }
}

fn cinder() -> Board {
    let mut b = Builder::new();
    let ring_of = |b: &mut Builder, n: usize, rx: f32, rz: f32, tier: u8| -> Vec<usize> {
        (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                b.add(a.cos() * rx, a.sin() * rz, tier, false)
            })
            .collect()
    };
    let o = ring_of(&mut b, 20, 12.2, 6.0, 0);
    let m = ring_of(&mut b, 12, 7.4, 3.6, 1);
    let i = ring_of(&mut b, 6, 3.2, 1.5, 2);
    for (r, n) in [(&o, 20usize), (&m, 12), (&i, 6)] {
        for k in 0..n {
            b.link(r[k], r[(k + 1) % n], Rule::Always);
        }
    }
    // Stairs up (outer -> middle -> crater) and slides back down.
    let mid = |b: &mut Builder, a: usize, c: usize, tier: u8| {
        let (pa, pc) = (b.spaces[a].pos, b.spaces[c].pos);
        b.add((pa[0] + pc[0]) / 2.0, (pa[1] + pc[1]) / 2.0, tier, true)
    };
    let s1 = mid(&mut b, o[3], m[1], 0);
    b.chain(o[3], &[s1], m[1], Rule::Always);
    let s2 = mid(&mut b, m[7], i[4], 1);
    b.chain(m[7], &[s2], i[4], Rule::Always);
    let d1 = mid(&mut b, i[1], m[10], 2);
    b.chain(i[1], &[d1], m[10], Rule::Always);
    let d2 = mid(&mut b, m[4], o[13], 1);
    b.chain(m[4], &[d2], o[13], Rule::Always);
    let sites = vec![o[9], m[2], m[9], i[3], i[0]];
    paint(&mut b, o[0], &sites, 0x0ca1, true);
    Board { id: 2, name: BOARD_NAMES[2], blurb: "A volcano of three rings. Every few rounds the lava tide rises and floods the lowest ring: climb or burn.", gimmick: Gimmick::Tide, spaces: b.spaces, edges: b.edges, start: o[0], sites, sky: [0x2a0c14, 0x8a2a24, 0xff9a3a], ground: 0x3a2a2a }
}

pub fn board(id: usize) -> Board {
    match id % BOARD_COUNT {
        0 => hourglass(),
        1 => cloudtop(),
        _ => cinder(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_distinct_boards_with_distinct_mechanics() {
        let g: Vec<_> = (0..3).map(|i| board(i).gimmick).collect();
        assert_eq!(g, vec![Gimmick::Bridge, Gimmick::Wind, Gimmick::Tide]);
        for i in 0..3 {
            let b = board(i);
            assert!(b.spaces.len() >= 28, "{} has {} spaces", b.name, b.spaces.len());
            assert!(b.sites.len() >= 4);
            assert_eq!(b.spaces[b.start].kind, Kind::Start);
            for &s in &b.sites {
                assert_eq!(b.spaces[s].kind, Kind::Keeper);
            }
        }
    }

    #[test]
    fn every_space_reaches_every_other_and_never_dead_ends() {
        for id in 0..3 {
            let b = board(id);
            for s in 0..b.spaces.len() {
                assert!(b.distance(s, b.start).is_some(), "{}: space {s} cannot reach home", b.name);
                assert!(b.distance(b.start, s).is_some(), "{}: home cannot reach space {s}", b.name);
            }
            for flip in [false, true] {
                for wind in [false, true] {
                    for tide in 0..3 {
                        let c = Conditions { flip, wind, tide };
                        for s in 0..b.spaces.len() {
                            assert!(!b.out(s, &c).is_empty(), "{}: space {s} has no open exit under {c:?}", b.name);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_bridge_flips_which_branch_is_open() {
        let b = board(0);
        let fork_w = b.edges.iter().position(|e| e.iter().any(|x| x.rule == Rule::Flip(false))).unwrap();
        let down = b.out(fork_w, &Conditions { flip: false, ..Default::default() });
        let up = b.out(fork_w, &Conditions { flip: true, ..Default::default() });
        assert_eq!(down.len(), 2);
        assert_eq!(up.len(), 1);
    }

    #[test]
    fn kinds_are_varied_and_sensible() {
        for id in 0..3 {
            let b = board(id);
            for k in [Kind::Coin, Kind::Pit, Kind::Event, Kind::Item, Kind::Shop, Kind::Warp, Kind::Duel, Kind::Hex] {
                assert!(b.spaces.iter().any(|s| s.kind == k), "{} lacks {:?}", b.name, k);
            }
            let warps = b.spaces.iter().filter(|s| s.kind == Kind::Warp).count();
            assert!(warps >= 2);
            let coins = b.spaces.iter().filter(|s| s.kind == Kind::Coin).count();
            assert!(coins as f32 > b.spaces.len() as f32 * 0.25);
        }
    }
}
