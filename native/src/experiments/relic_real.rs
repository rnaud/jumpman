//! Relic Descent on real dungeons: the floors are stretches of Ocarina of Time's own dungeons (the scenes the player
//! imported from their ROM), not generated boxes.
//!
//! A [`Site`] is a hand-picked entrance into one scene (the Deku Tree, Dodongo's Cavern, Jabu-Jabu, the Forest
//! Temple, a boss lair). [`build`] turns it into the tile [`Dungeon`] the rules already play on:
//!
//! 1. A flood fill over a 110-unit grid from the entrance, through the scene's own collision (the [`Probe`] seam, so
//!    this is engine-free and tested on toys), keeps every spot a fighter can stand on and walk to: a floor no steeper
//!    than 45 degrees, no wall within reach, head room, steps of at most [`STEP`] between neighbours and no wall across
//!    the step. It stops taking in new scene rooms after the site's `rooms`, which keeps a floor short.
//! 2. The tiles are grouped into rooms by the scene's own room boxes (`roomBounds`); slivers are merged into a
//!    neighbour and very large rooms are cut in two, so every room is one pack's worth of fight.
//! 3. The room farthest (in doors) from the entrance holds the exit (or the boss), a dead end holds the treasure, and
//!    every room's centre is its most open spot.
//!
//! The presentation (`relic.rs`) draws the scene's real model, gives Mario the scene's real collision plus an invisible
//! fence along the edge of the flood fill ([`fence`]), so the player, the foes and the drops share one floor plan.
//! Everything here is deterministic for a seed ([`pick_sites`]).
use super::relic_dungeon::*;
use super::swarm_core::Rng;
use jumpman_sim::collision::CollisionWorld;
use std::sync::Arc;

/// Grid step of a real floor (SM64 units; the OoT scenes are exported at about 2.2 times OoT's own units).
pub const RTILE: f32 = 110.0;
/// Biggest rise or drop between neighbouring tiles that still counts as walking.
pub const STEP: f32 = 70.0;
/// No wall may be closer than this to a tile centre (at chest height).
const WALL_R: f32 = 48.0;
/// Clearance needed above a floor.
const HEADROOM: f32 = 200.0;
/// Rooms smaller than this many tiles are merged into a neighbour; rooms bigger than `MAX_ZONE` are cut in two.
const MIN_ZONE: usize = 28;
const MAX_ZONE: usize = 760;
/// The rules' room list stays this short (the minimap shows twelve).
const MAX_ROOMS: usize = 8;
/// A floor never holds more tiles than this (a short floor; also bounds the build time).
const MAX_TILES: usize = 4200;

/// A floor found under a point.
#[derive(Clone, Copy, Debug)]
pub struct FloorHit {
    pub y: f32,
    /// Normal's y (1 = flat).
    pub ny: f32,
    /// Something to stand on (not lava, a death plane, quicksand or under water).
    pub ok: bool,
}

/// What the builder needs to know about the scene.
pub trait Probe {
    /// The highest floor at (x, z) no higher than `max_y`.
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<FloorHit>;
    /// Is a wall within `r` of the point (at that height)?
    fn wall_near(&self, x: f32, y: f32, z: f32, r: f32) -> bool;
    /// The lowest ceiling above (x, y, z), if any.
    fn ceiling(&self, x: f32, y: f32, z: f32) -> Option<f32>;
    /// The scene room a standing spot belongs to.
    fn room(&self, x: f32, y: f32, z: f32) -> Option<i32>;
}

/// One entrance into a real dungeon.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub key: &'static str,
    /// Shown on arrival.
    pub title: &'static str,
    pub course: &'static str,
    pub area: i32,
    /// Course position just above the entrance floor (the flood fill starts on the floor under it).
    pub start: [f32; 3],
    /// Half size of the square searched around the start.
    pub reach: f32,
    /// Scene rooms the floor may take in.
    pub rooms: usize,
    /// 1 or 2: an ordinary floor of that depth; 3: a boss lair.
    pub tier: u8,
}

/// The hand-picked entrances, checked by the tests below (every one builds, is connected and has room for its fights).
pub const SITES: &[Site] = &[
    // ---- floor 1: the Deku Tree ----
    Site { key: "deku_hollow", title: "DEKU TREE - THE GREAT HOLLOW", course: "deku_tree", area: 1, start: [4728.0, 60.0, 3364.0], reach: 4400.0, rooms: 2, tier: 1 },
    Site { key: "deku_basement", title: "DEKU TREE - THE ROOTS BELOW", course: "deku_tree", area: 1, start: [-345.0, -3548.0, 2633.0], reach: 5400.0, rooms: 3, tier: 1 },
    Site { key: "deku_halls", title: "DEKU TREE - THE LOWER HALLS", course: "deku_tree", area: 1, start: [2308.0, -3548.0, 1406.0], reach: 5400.0, rooms: 3, tier: 1 },
    // ---- floor 2: Dodongo's Cavern, Jabu-Jabu's Belly, the Forest Temple ----
    Site { key: "dodongo_hall", title: "DODONGO'S CAVERN - THE GREAT HALL", course: "dodongos_cavern", area: 1, start: [-5110.0, 60.0, 10000.0], reach: 5400.0, rooms: 3, tier: 2 },
    Site { key: "dodongo_east", title: "DODONGO'S CAVERN - THE EAST CHAMBERS", course: "dodongos_cavern", area: 1, start: [10490.0, 236.0, 986.0], reach: 5400.0, rooms: 3, tier: 2 },
    Site { key: "jabu_belly", title: "JABU-JABU'S BELLY", course: "jabu_jabu", area: 1, start: [-1514.0, -1436.0, -1822.0], reach: 5400.0, rooms: 3, tier: 2 },
    Site { key: "jabu_tunnels", title: "JABU-JABU'S BELLY - THE DEEP PASSAGES", course: "jabu_jabu", area: 1, start: [-123.0, -1436.0, -8202.0], reach: 5400.0, rooms: 3, tier: 2 },
    Site { key: "forest_court", title: "FOREST TEMPLE - THE ENTRY HALL", course: "forest_temple", area: 1, start: [-40.0, 1420.0, 6000.0], reach: 5400.0, rooms: 3, tier: 2 },
    Site { key: "forest_cellar", title: "FOREST TEMPLE - THE CELLAR", course: "forest_temple", area: 1, start: [944.0, -3368.0, -4464.0], reach: 5400.0, rooms: 3, tier: 2 },
    // ---- floor 3: a boss lair ----
    Site { key: "gohma_lair", title: "GOHMA'S LAIR", course: "deku_tree", area: 2, start: [464.0, -2760.0, -499.0], reach: 4400.0, rooms: 2, tier: 3 },
    Site { key: "king_dodongo_lair", title: "KING DODONGO'S LAIR", course: "dodongos_cavern_boss", area: 1, start: [-22.0, -3210.0, 739.0], reach: 4400.0, rooms: 2, tier: 3 },
    Site { key: "barinade_lair", title: "BARINADE'S LAIR", course: "jabu_jabu_boss", area: 1, start: [59.0, 60.0, -1879.0], reach: 4400.0, rooms: 2, tier: 3 },
    Site { key: "phantom_lair", title: "PHANTOM GANON'S LAIR", course: "forest_temple_boss", area: 1, start: [1936.0, -790.0, -953.0], reach: 4400.0, rooms: 2, tier: 3 },
];

pub fn site(key: &str) -> Option<&'static Site> {
    SITES.iter().find(|s| s.key == key)
}

/// One site per floor for a run: tier 1, tier 2, then a lair, drawn from the seed among the sites whose scene is
/// imported (`have`). `force` (keys, `JUMPMAN_RELIC_SITES`) overrides the draw floor by floor.
pub fn pick_sites(seed: u64, have: &dyn Fn(&Site) -> bool, force: &[&str]) -> Option<Vec<&'static Site>> {
    let mut rng = Rng::new(seed ^ 0x0C7A_51DE);
    let mut out = vec![];
    for tier in 1..=3u8 {
        let forced = force.get(tier as usize - 1).and_then(|k| site(k)).filter(|s| have(s));
        let pool: Vec<&'static Site> = SITES.iter().filter(|s| s.tier == tier && have(s)).collect();
        let pick = match forced {
            Some(s) => s,
            None if !pool.is_empty() => pool[rng.below(pool.len() as u32) as usize],
            None => return None,
        };
        out.push(pick);
    }
    Some(out)
}

// ---------------------------------------------------------------------------------------------
// The flood fill
// ---------------------------------------------------------------------------------------------

struct Fill {
    x0: f32,
    z0: f32,
    w: usize,
    h: usize,
    /// Per tile: floor height (NaN = not on the floor) and scene room.
    y: Vec<f32>,
    room: Vec<i32>,
    order: Vec<usize>,
}

impl Fill {
    fn centre(&self, i: usize) -> (f32, f32) {
        (self.x0 + ((i % self.w) as f32 + 0.5) * RTILE, self.z0 + ((i / self.w) as f32 + 0.5) * RTILE)
    }
    fn on(&self, i: usize) -> bool {
        !self.y[i].is_nan()
    }
    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let (x, z) = ((i % self.w) as i32, (i / self.w) as i32);
        [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().filter_map(move |(dx, dz)| {
            let (nx, nz) = (x + dx, z + dz);
            (nx >= 0 && nz >= 0 && nx < self.w as i32 && nz < self.h as i32).then(|| nz as usize * self.w + nx as usize)
        })
    }
}

/// The floor a fighter stands on at (x, z) near height `near`, if it is a good standing spot.
fn standable<P: Probe + ?Sized>(probe: &P, x: f32, z: f32, near: f32) -> Option<f32> {
    let hit = probe.floor(x, z, near + STEP)?;
    if hit.y < near - STEP || !hit.ok || hit.ny < 0.7 {
        return None;
    }
    let y = hit.y;
    if probe.wall_near(x, y + 60.0, z, WALL_R) || probe.wall_near(x, y + 120.0, z, WALL_R) {
        return None;
    }
    if probe.ceiling(x, y + 20.0, z).is_some_and(|c| c - y < HEADROOM) {
        return None;
    }
    // Ground under a body width on every side (not the lip of a pit or a ridge).
    for (dx, dz) in [(42.0f32, 0.0f32), (-42.0, 0.0), (0.0, 42.0), (0.0, -42.0)] {
        let ok = probe.floor(x + dx, z + dz, y + 46.0).is_some_and(|f| f.ok && (f.y - y).abs() <= 46.0);
        if !ok {
            return None;
        }
    }
    Some(y)
}

/// Is the step between two neighbouring standing spots blocked (a wall across it, or no floor in between)?
fn crossing_blocked<P: Probe + ?Sized>(probe: &P, a: (f32, f32), ha: f32, b: (f32, f32), hb: f32) -> bool {
    let (x, z) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let top = ha.max(hb);
    if probe.wall_near(x, top + 30.0, z, 24.0) || probe.wall_near(x, top + 60.0, z, 40.0) {
        return true;
    }
    let mid = (ha + hb) / 2.0;
    !probe.floor(x, z, top + 20.0).is_some_and(|f| f.ok && (f.y - mid).abs() <= 45.0)
}

fn flood<P: Probe + ?Sized>(probe: &P, site: &Site) -> Result<Fill, String> {
    let n = ((2.0 * site.reach / RTILE).ceil() as usize).clamp(8, 220);
    let (x0, z0) = (site.start[0] - n as f32 * RTILE / 2.0, site.start[2] - n as f32 * RTILE / 2.0);
    let mut f = Fill { x0, z0, w: n, h: n, y: vec![f32::NAN; n * n], room: vec![-1; n * n], order: vec![] };
    // The start: the tile under the entrance, or the nearest good tile around it.
    let base = probe.floor(site.start[0], site.start[2], site.start[1]).ok_or_else(|| format!("{}: no floor under the start", site.key))?.y;
    let centre = (n / 2) * n + n / 2;
    let mut start = None;
    'search: for r in 0..4i32 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                let i = (centre as i32 + dz * n as i32 + dx) as usize;
                let (cx, cz) = f.centre(i);
                if let Some(y) = standable(probe, cx, cz, base) {
                    start = Some((i, y));
                    break 'search;
                }
            }
        }
    }
    let (si, sy) = start.ok_or_else(|| format!("{}: the entrance is not a standing spot", site.key))?;
    let (cx, cz) = f.centre(si);
    f.y[si] = sy;
    f.room[si] = probe.room(cx, sy, cz).unwrap_or(-1);
    let mut allowed = vec![f.room[si]];
    let mut queue = std::collections::VecDeque::from([si]);
    let mut seen = vec![false; n * n];
    seen[si] = true;
    while let Some(i) = queue.pop_front() {
        f.order.push(i);
        if f.order.len() >= MAX_TILES {
            break;
        }
        let (ax, az) = f.centre(i);
        let ha = f.y[i];
        let nbrs: Vec<usize> = f.neighbours(i).collect();
        for j in nbrs {
            if seen[j] {
                continue;
            }
            let (bx, bz) = f.centre(j);
            let Some(hb) = standable(probe, bx, bz, ha) else { continue };
            if crossing_blocked(probe, (ax, az), ha, (bx, bz), hb) {
                continue;
            }
            let room = probe.room(bx, hb, bz).unwrap_or(f.room[i]);
            if !allowed.contains(&room) {
                if allowed.len() >= site.rooms {
                    continue;
                }
                allowed.push(room);
            }
            seen[j] = true;
            f.y[j] = hb;
            f.room[j] = room;
            queue.push_back(j);
        }
    }
    // Tiles queued but never expanded (the tile cap) stay on the floor: they are connected.
    Ok(f)
}

// ---------------------------------------------------------------------------------------------
// Rooms
// ---------------------------------------------------------------------------------------------

/// Multi-source breadth-first labelling of `tiles` (all in one zone) from `seeds`: each tile goes to its nearest seed.
fn split_by_seeds(f: &Fill, tiles: &[usize], seeds: &[usize]) -> Vec<(usize, usize)> {
    let set: std::collections::HashSet<usize> = tiles.iter().copied().collect();
    let mut owner: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::new();
    for (k, s) in seeds.iter().enumerate() {
        owner.insert(*s, k);
        queue.push_back(*s);
    }
    while let Some(i) = queue.pop_front() {
        let k = owner[&i];
        for j in f.neighbours(i) {
            if set.contains(&j) && !owner.contains_key(&j) {
                owner.insert(j, k);
                queue.push_back(j);
            }
        }
    }
    let mut out: Vec<(usize, usize)> = owner.into_iter().collect();
    out.sort_unstable();
    out
}

/// Tile-graph distances inside a set of tiles from one source (usize::MAX = unreached).
fn distances(f: &Fill, within: &dyn Fn(usize) -> bool, from: usize) -> Vec<usize> {
    let mut d = vec![usize::MAX; f.w * f.h];
    d[from] = 0;
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(i) = queue.pop_front() {
        for j in f.neighbours(i) {
            if d[j] == usize::MAX && f.on(j) && within(j) {
                d[j] = d[i] + 1;
                queue.push_back(j);
            }
        }
    }
    d
}

/// Zone (room) of every floor tile: the scene's rooms, slivers merged, giants halved, at most [`MAX_ROOMS`].
fn zones(f: &Fill, start: usize, min_rooms: usize) -> Vec<usize> {
    const NONE: usize = usize::MAX;
    let mut zone = vec![NONE; f.w * f.h];
    let mut ids: Vec<i32> = vec![];
    for &i in &f.order {
        let k = match ids.iter().position(|r| *r == f.room[i]) {
            Some(k) => k,
            None => {
                ids.push(f.room[i]);
                ids.len() - 1
            }
        };
        zone[i] = k;
    }
    // Disconnected pieces of one scene room become separate zones (a room box can hold two floors).
    let mut next = ids.len();
    let mut done = vec![false; f.w * f.h];
    let mut first_piece: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &i in &f.order {
        if done[i] {
            continue;
        }
        let z = zone[i];
        let label = if first_piece.insert(z) { z } else {
            next += 1;
            next - 1
        };
        let mut stack = vec![i];
        done[i] = true;
        while let Some(t) = stack.pop() {
            zone[t] = label;
            for j in f.neighbours(t) {
                if !done[j] && f.on(j) && zone[j] == z {
                    done[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    let count = |zone: &Vec<usize>, z: usize| zone.iter().filter(|x| **x == z).count();
    let contacts = |zone: &Vec<usize>, z: usize| -> std::collections::BTreeMap<usize, usize> {
        let mut m = std::collections::BTreeMap::new();
        for &i in &f.order {
            if zone[i] != z {
                continue;
            }
            for j in f.neighbours(i) {
                if f.on(j) && zone[j] != z {
                    *m.entry(zone[j]).or_insert(0) += 1;
                }
            }
        }
        m
    };
    // Merge slivers into the neighbour they touch most, the smallest first, then cap the count.
    loop {
        let mut live: Vec<usize> = zone.iter().filter(|z| **z != NONE).copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        live.sort_by_key(|z| (count(&zone, *z), *z));
        let Some(&small) = live.first() else { break };
        let too_many = live.len() > MAX_ROOMS;
        if live.len() <= 1 || (count(&zone, small) >= MIN_ZONE && !too_many) {
            break;
        }
        let Some((&into, _)) = contacts(&zone, small).iter().max_by_key(|(z, c)| (**c, std::cmp::Reverse(**z))) else { break };
        zone.iter_mut().filter(|z| **z == small).for_each(|z| *z = into);
    }
    // Halve giants (and keep halving the biggest while the floor has too few rooms) across their longest walk.
    for _ in 0..12 {
        let live: std::collections::BTreeSet<usize> = zone.iter().filter(|z| **z != NONE).copied().collect();
        let Some(&big) = live.iter().max_by_key(|z| (count(&zone, **z), std::cmp::Reverse(**z))) else { break };
        let size = count(&zone, big);
        let want_more = live.len() < min_rooms;
        if live.len() >= MAX_ROOMS || !(size > MAX_ZONE || (want_more && size >= 2 * MIN_ZONE)) {
            break;
        }
        let tiles: Vec<usize> = f.order.iter().copied().filter(|i| zone[*i] == big).collect();
        let within = |j: usize| zone[j] == big;
        let a0 = if zone[start] == big { start } else { tiles[0] };
        let da = distances(f, &within, a0);
        let a = *tiles.iter().max_by_key(|i| (da[**i], std::cmp::Reverse(**i))).unwrap();
        let db = distances(f, &within, a);
        let b = *tiles.iter().max_by_key(|i| (db[**i], std::cmp::Reverse(**i))).unwrap();
        if a == b {
            break;
        }
        let new = zone.iter().filter(|z| **z != NONE).max().unwrap() + 1;
        for (i, k) in split_by_seeds(f, &tiles, &[a, b]) {
            if k == 1 {
                zone[i] = new;
            }
        }
    }
    // Renumber in flood order (the start's room first).
    let mut map: Vec<(usize, usize)> = vec![];
    for &i in std::iter::once(&start).chain(f.order.iter()) {
        if zone[i] != NONE && !map.iter().any(|(old, _)| *old == zone[i]) {
            map.push((zone[i], map.len()));
        }
    }
    zone.iter().map(|z| map.iter().find(|(old, _)| old == z).map_or(NONE, |(_, new)| *new)).collect()
}

/// Build the tile dungeon of a site from its scene.
pub fn build<P: Probe + ?Sized>(probe: &P, site: &Site, boss_floor: bool) -> Result<Dungeon, String> {
    let f = flood(probe, site)?;
    if f.order.len() < 120 {
        return Err(format!("{}: only {} floor tiles", site.key, f.order.len()));
    }
    let start = f.order[0];
    // Three rooms at least, the lairs too: the entrance, a guard room, then the exit (a lair of two rooms put the boss
    // straight after the entrance).
    let zone = zones(&f, start, 3);
    let nz = zone.iter().filter(|z| **z != usize::MAX).max().map_or(0, |m| m + 1);
    if nz < 2 {
        return Err(format!("{}: a single room", site.key));
    }
    // Links: zones touching along at least two tile edges (a doorway, not a corner).
    let mut touch = vec![vec![0usize; nz]; nz];
    for &i in &f.order {
        for j in f.neighbours(i) {
            if f.on(j) && zone[j] != zone[i] {
                touch[zone[i]][zone[j]] += 1;
            }
        }
    }
    let linked = |a: usize, b: usize| touch[a][b] >= 2;
    // Depth in doors from the start room, and the tile walk from the entrance (ties).
    let mut depth = vec![usize::MAX; nz];
    depth[0] = 0;
    let mut q = std::collections::VecDeque::from([0usize]);
    while let Some(a) = q.pop_front() {
        for b in 0..nz {
            if depth[b] == usize::MAX && linked(a, b) {
                depth[b] = depth[a] + 1;
                q.push_back(b);
            }
        }
    }
    // A zone joined only by a corner still counts as one door away from its best neighbour.
    for b in 0..nz {
        if depth[b] == usize::MAX {
            depth[b] = (0..nz).filter(|a| touch[*a][b] > 0 && depth[*a] != usize::MAX).map(|a| depth[a] + 1).min().unwrap_or(nz);
        }
    }
    let walk = distances(&f, &|_| true, start);
    // Most open spot of each zone: the tile farthest from the zone's edge (ties: nearest the zone's middle).
    let mut edge_d = vec![usize::MAX; f.w * f.h];
    let mut q = std::collections::VecDeque::new();
    for &i in &f.order {
        let border = f.neighbours(i).count() < 4 || f.neighbours(i).any(|j| !f.on(j) || zone[j] != zone[i]);
        if border {
            edge_d[i] = 0;
            q.push_back(i);
        }
    }
    while let Some(i) = q.pop_front() {
        for j in f.neighbours(i) {
            if f.on(j) && zone[j] == zone[i] && edge_d[j] == usize::MAX {
                edge_d[j] = edge_d[i] + 1;
                q.push_back(j);
            }
        }
    }
    let origin = [f.x0 as f64, f.y[start] as f64, f.z0 as f64];
    let k = RTILE / TILE;
    let mut rooms: Vec<Room> = vec![];
    for z in 0..nz {
        let tiles: Vec<usize> = f.order.iter().copied().filter(|i| zone[*i] == z).collect();
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        for &i in &tiles {
            let (x, y) = (i % f.w, i / f.w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sx += x as f32;
            sy += y as f32;
        }
        let (mx, my) = (sx / tiles.len() as f32, sy / tiles.len() as f32);
        let best = *tiles
            .iter()
            .max_by(|a, b| {
                let da = ((*a % f.w) as f32 - mx).powi(2) + ((*a / f.w) as f32 - my).powi(2);
                let db = ((*b % f.w) as f32 - mx).powi(2) + ((*b / f.w) as f32 - my).powi(2);
                edge_d[**a].min(5).cmp(&edge_d[**b].min(5)).then(db.partial_cmp(&da).unwrap()).then(b.cmp(a))
            })
            .unwrap();
        let c = if z == 0 { start } else { best };
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let links: Vec<usize> = (0..nz).filter(|b| *b != z && linked(z, *b)).collect();
        rooms.push(Room {
            x: x0,
            y: y0,
            w,
            h,
            kind: RoomKind::Combat,
            cell: (0, 0),
            links,
            c: [((c % f.w) as f32 + 0.5) * RTILE, ((c / f.w) as f32 + 0.5) * RTILE],
            hs: [w as f32 * RTILE / 2.0, h as f32 * RTILE / 2.0],
            mid: [(x0 as f32 + w as f32 / 2.0) * RTILE, (y0 as f32 + h as f32 / 2.0) * RTILE],
            area: ((tiles.len() as f32 * k * k).round() as usize).max(1),
        });
    }
    let cw: Vec<usize> = rooms.iter().map(|r| walk[(r.c[1] / RTILE) as usize * f.w + (r.c[0] / RTILE) as usize]).collect();
    let exit = (1..nz).max_by_key(|z| (depth[*z], cw[*z], std::cmp::Reverse(*z))).unwrap();
    rooms[0].kind = RoomKind::Start;
    rooms[exit].kind = if boss_floor { RoomKind::Boss } else { RoomKind::Stairs };
    if nz >= 3 {
        let others = move || (1..nz).filter(move |z| *z != exit);
        let dead_end: Vec<bool> = rooms.iter().map(|r| r.links.len() <= 1).collect();
        let treasure = others().filter(|z| dead_end[*z]).max_by_key(|z| (depth[*z], cw[*z])).or_else(|| others().max_by_key(|z| (depth[*z], cw[*z])));
        if let Some(t) = treasure {
            rooms[t].kind = RoomKind::Treasure;
        }
    }
    let mut tiles = vec![Tile::Void; f.w * f.h];
    let mut heights = vec![0.0f32; f.w * f.h];
    let mut zones = vec![NO_ZONE; f.w * f.h];
    for &i in &f.order {
        tiles[i] = if zone[i] == exit && boss_floor { Tile::BossRoom } else { Tile::Room };
        heights[i] = f.y[i] - f.y[start];
        zones[i] = zone[i] as u8;
    }
    Ok(Dungeon {
        tiles,
        rooms,
        start: 0,
        exit,
        boss_floor,
        w: f.w,
        h: f.h,
        tile: RTILE,
        origin,
        heights,
        zones,
        real: Some(RealSource { course: site.course.to_string(), area: site.area, title: site.title.to_string(), key: site.key.to_string() }),
    })
}

/// An invisible wall quad along every edge between a floor tile and a tile off the floor: (four corners in course
/// units, the normal pointing back onto the floor). Mario walks the real collision, but the fence keeps him on the
/// stretch the rules know about (a door into a room that is not part of the floor is closed by it).
pub fn fence(d: &Dungeon) -> Vec<([[f64; 3]; 4], [f64; 3])> {
    let mut out = vec![];
    let t = d.tile as f64;
    for y in 0..d.h as i32 {
        for x in 0..d.w as i32 {
            if !d.is_floor_tile(x, y) {
                continue;
            }
            let h = d.origin[1] + d.heights[y as usize * d.w + x as usize] as f64;
            let (lo, hi) = (h - 260.0, h + 1300.0);
            let (wx0, wz0) = (d.origin[0] + x as f64 * t, d.origin[2] + y as f64 * t);
            let (wx1, wz1) = (wx0 + t, wz0 + t);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if d.is_floor_tile(x + dx, y + dy) {
                    continue;
                }
                // The edge on that side, and a normal pointing back into this tile.
                let (a, b, n) = match (dx, dy) {
                    (1, 0) => ((wx1, wz0), (wx1, wz1), [-1.0, 0.0, 0.0]),
                    (-1, 0) => ((wx0, wz0), (wx0, wz1), [1.0, 0.0, 0.0]),
                    (0, 1) => ((wx0, wz1), (wx1, wz1), [0.0, 0.0, -1.0]),
                    _ => ((wx0, wz0), (wx1, wz0), [0.0, 0.0, 1.0]),
                };
                out.push(([[a.0, lo, a.1], [b.0, lo, b.1], [b.0, hi, b.1], [a.0, hi, a.1]], n));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The scene through its collision
// ---------------------------------------------------------------------------------------------

/// A scene's collision and room boxes.
pub struct WorldProbe {
    pub world: Arc<CollisionWorld>,
    pub boxes: Vec<jumpman_sim::oot_rooms::RoomBox>,
}

impl Probe for WorldProbe {
    fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<FloorHit> {
        use jumpman_sim::surfaces::*;
        let (x, z, max_y) = (x as f64, z as f64, max_y as f64);
        let mut q = max_y;
        for _ in 0..8 {
            let h = self.world.find_floor(x, q, z);
            let s = h.surface.as_ref()?;
            if h.height <= max_y + 0.01 {
                let bad = matches!(s.kind, SURFACE_BURNING | SURFACE_DEATH_PLANE | SURFACE_DEEP_QUICKSAND | SURFACE_INSTANT_QUICKSAND | SURFACE_DEEP_MOVING_QUICKSAND | SURFACE_INSTANT_MOVING_QUICKSAND);
                let dry = self.world.water_level(x, z) < h.height + 12.0;
                return Some(FloorHit { y: h.height as f32, ny: s.normal[1] as f32, ok: !bad && dry });
            }
            // find_floor accepts floors up to 78 above the query: step below the one it found.
            q = h.height - 79.0;
        }
        None
    }
    fn wall_near(&self, x: f32, y: f32, z: f32, r: f32) -> bool {
        let mut w = jumpman_sim::collision::WallData { x: x as f64, y: y as f64, z: z as f64, offset_y: 0.0, radius: r as f64, walls: vec![] };
        self.world.find_wall_collisions(&mut w, false) > 0
    }
    fn ceiling(&self, x: f32, y: f32, z: f32) -> Option<f32> {
        let h = self.world.find_ceil(x as f64, y as f64, z as f64);
        h.surface.map(|_| h.height as f32)
    }
    fn room(&self, x: f32, y: f32, z: f32) -> Option<i32> {
        let p = [x as f64, y as f64 + 50.0, z as f64];
        let inside = |b: &jumpman_sim::oot_rooms::RoomBox| (0..3).all(|k| p[k] >= b.min[k] && p[k] <= b.max[k]);
        let volume = |b: &jumpman_sim::oot_rooms::RoomBox| (0..3).map(|k| (b.max[k] - b.min[k]).max(1.0)).product::<f64>();
        self.boxes.iter().filter(|b| inside(b)).min_by(|a, b| volume(a).total_cmp(&volume(b))).map(|b| b.room)
    }
}

/// The static collision of a scene (the same triangles the game loads for it), without its actors.
pub fn scene_world(root: &std::path::Path, course: &str, area: i32) -> Result<(CollisionWorld, String), String> {
    let data = crate::game::course_data::CourseData::load(root, course, area)?;
    let mut world = CollisionWorld::new(jumpman_sim::collision::CELL_SIZE, data.horizontal_boundary.unwrap_or(jumpman_sim::collision::LEVEL_BOUNDARY_MAX));
    world.water_boxes = data.water_boxes.iter().map(|b| jumpman_sim::collision::WaterBox(b[0], b[1], b[2], b[3], b[4], b[5])).collect();
    crate::game::native_collision::add_native_collision(&mut world, &data.collision, [0.0; 3], 0.0);
    world.finalize();
    Ok((world, data.model))
}

/// Is the scene of a site imported on this machine?
pub fn site_available(root: &std::path::Path, s: &Site) -> bool {
    crate::game::course_data::CourseData::path(root, s.course, s.area).is_file()
}

/// Build a site's floor from the imported scene.
pub fn build_site(root: &std::path::Path, s: &Site, boss_floor: bool) -> Result<Dungeon, String> {
    let (world, _) = scene_world(root, s.course, s.area)?;
    let probe = WorldProbe { world: Arc::new(world), boxes: crate::zelda::rooms::load_boxes(root, s.course, s.area) };
    build(&probe, s, boss_floor)
}

/// The floors of a run on real dungeons, or why there are none (the OoT scenes are not imported, or a site failed).
pub fn plan_run(root: &std::path::Path, seed: u64, force: &[&str]) -> Result<Vec<Dungeon>, String> {
    let sites = pick_sites(seed, &|s| site_available(root, s), force).ok_or("the Ocarina of Time dungeons are not imported")?;
    sites.iter().enumerate().map(|(i, s)| build_site(root, s, i + 1 == sites.len())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toy scene: flat floors at given heights over rectangles, walls along the rectangles' open edges, rooms by box.
    struct Toy {
        /// (x0, z0, x1, z1, y, room)
        floors: Vec<(f32, f32, f32, f32, f32, i32)>,
        /// Wall segments (x0, z0, x1, z1) of infinite height.
        walls: Vec<(f32, f32, f32, f32)>,
    }

    impl Probe for Toy {
        fn floor(&self, x: f32, z: f32, max_y: f32) -> Option<FloorHit> {
            self.floors.iter().filter(|f| x >= f.0 && x <= f.2 && z >= f.1 && z <= f.3 && f.4 <= max_y).map(|f| f.4).max_by(|a, b| a.total_cmp(b)).map(|y| FloorHit { y, ny: 1.0, ok: true })
        }
        fn wall_near(&self, x: f32, _y: f32, z: f32, r: f32) -> bool {
            self.walls.iter().any(|w| {
                let (dx, dz) = (w.2 - w.0, w.3 - w.1);
                let t = (((x - w.0) * dx + (z - w.1) * dz) / (dx * dx + dz * dz).max(1e-6)).clamp(0.0, 1.0);
                let (px, pz) = (w.0 + dx * t, w.1 + dz * t);
                (x - px).hypot(z - pz) < r
            })
        }
        fn ceiling(&self, _x: f32, _y: f32, _z: f32) -> Option<f32> {
            None
        }
        fn room(&self, x: f32, y: f32, z: f32) -> Option<i32> {
            self.floors.iter().find(|f| x >= f.0 && x <= f.2 && z >= f.1 && z <= f.3 && (f.4 - y).abs() < 1.0).map(|f| f.5)
        }
    }

    /// Four rooms in a row joined by doorways, a ramp up into the third, a sealed fifth room beside the second and a
    /// pit in the first.
    fn toy() -> Toy {
        let mut floors = vec![
            (0.0, 0.0, 2000.0, 2000.0, 0.0, 0),
            (2000.0, 600.0, 2400.0, 1400.0, 0.0, 0), // doorway 0-1
            (2400.0, 0.0, 4400.0, 2000.0, 0.0, 1),
            (4400.0, 600.0, 4800.0, 1400.0, 0.0, 1), // doorway 1-2
            (4800.0, 0.0, 6800.0, 2000.0, 0.0, 2),
            (6800.0, 600.0, 7200.0, 1400.0, 0.0, 2),
            (7200.0, 0.0, 9200.0, 2000.0, 0.0, 3),
            (2400.0, 2400.0, 4400.0, 4400.0, 0.0, 4), // sealed
        ];
        // A ramp: the last room's far half climbs in 40-unit terraces.
        for k in 0..8 {
            let x0 = 8200.0 + k as f32 * 125.0;
            floors.push((x0, 0.0, x0 + 125.0, 2000.0, 40.0 * (k + 1) as f32, 3));
        }
        // The pit: a hole in the first room (a floor far below).
        floors.retain(|f| f.5 != 0 || f.0 != 0.0);
        floors.push((0.0, 0.0, 2000.0, 700.0, 0.0, 0));
        floors.push((0.0, 1300.0, 2000.0, 2000.0, 0.0, 0));
        floors.push((0.0, 700.0, 800.0, 1300.0, 0.0, 0));
        floors.push((1200.0, 700.0, 2000.0, 1300.0, 0.0, 0));
        floors.push((800.0, 700.0, 1200.0, 1300.0, -900.0, 0));
        let mut walls = vec![
            (2400.0, 2400.0, 4400.0, 2400.0),
            (2400.0, 2000.0, 2400.0, 2400.0),
            (0.0, 0.0, 9200.0, 0.0),
            (0.0, 2000.0, 2400.0, 2000.0),
            (4400.0, 2000.0, 9200.0, 2000.0),
            (2400.0, 2000.0, 4400.0, 2000.0), // seals room 4 off from room 1
        ];
        walls.push((0.0, 0.0, 0.0, 2000.0));
        Toy { floors, walls }
    }

    fn toy_site(rooms: usize) -> Site {
        Site { key: "toy", title: "TOY", course: "toy", area: 1, start: [400.0, 50.0, 1000.0], reach: 9200.0, rooms, tier: 1 }
    }

    /// Every floor tile is walkable from the start along the tile grid (the same 4-neighbour walk the foes use).
    pub(crate) fn connected(d: &Dungeon) -> bool {
        let c = d.tile_of(d.rooms[d.start].center());
        let mut seen = vec![false; d.w * d.h];
        let mut stack = vec![c];
        let mut n = 0;
        while let Some((x, y)) = stack.pop() {
            if !d.is_floor_tile(x, y) || seen[y as usize * d.w + x as usize] {
                continue;
            }
            seen[y as usize * d.w + x as usize] = true;
            n += 1;
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
        n == d.tiles.iter().filter(|t| **t != Tile::Void).count()
    }

    /// A 4-neighbour tile walk from one point to another exists.
    fn tile_walk(d: &Dungeon, from: [f32; 2], to: [f32; 2]) -> bool {
        let (goal, s) = (d.tile_of(to), d.tile_of(from));
        let mut seen = vec![false; d.w * d.h];
        let mut stack = vec![s];
        while let Some((x, y)) = stack.pop() {
            if !d.is_floor_tile(x, y) || seen[y as usize * d.w + x as usize] {
                continue;
            }
            if (x, y) == goal {
                return true;
            }
            seen[y as usize * d.w + x as usize] = true;
            stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
        }
        false
    }

    #[test]
    fn the_toy_builds_connected_rooms_with_an_exit_far_away() {
        let d = build(&toy(), &toy_site(8), false).unwrap();
        assert!(connected(&d));
        assert!(d.rooms.len() >= 4 && d.rooms.len() <= MAX_ROOMS, "{} rooms", d.rooms.len());
        assert_eq!(d.rooms[d.start].kind, RoomKind::Start);
        assert_eq!(d.rooms[d.exit].kind, RoomKind::Stairs);
        // The exit is in the last room of the row; the sealed room and the pit floor are not on the floor plan.
        assert!(d.rooms[d.exit].center()[0] + d.origin[0] as f32 > 7200.0, "exit at {:?}", d.rooms[d.exit].center());
        for y in 0..d.h as i32 {
            for x in 0..d.w as i32 {
                if d.is_floor_tile(x, y) {
                    let p = d.tile_center(x, y);
                    let w = [p[0] + d.origin[0] as f32, p[1] + d.origin[2] as f32];
                    assert!(w[1] < 2000.0, "the sealed room leaked in at {w:?}");
                    assert!(d.heights[y as usize * d.w + x as usize] > -100.0, "the pit floor is on the plan at {w:?}");
                }
            }
        }
        // The ramp is walked: its top terrace is on the plan, at its own height.
        let up = d.plane([8890.0, 0.0, 1000.0]);
        assert!(d.walkable(up) && (d.height_at(up) - 240.0).abs() <= 40.0, "terrace at {}", d.height_at(up));
    }

    /// The rules run on a built floor: walking into the exit room wakes its pack on its own open floor, the way down
    /// stays shut until that pack is beaten, then the next planned floor (not a generated one) follows.
    #[test]
    fn a_real_floor_plays_in_the_rules_and_its_exit_waits_for_the_guards() {
        use super::super::relic_core::*;
        let floors = vec![build(&toy(), &toy_site(8), false).unwrap(), build(&toy(), &toy_site(3), false).unwrap(), build(&toy(), &toy_site(8), true).unwrap()];
        let mut r = Relic::with_plan(5, Hero::Mario, 1, 0, Some(Arc::new(floors.clone())));
        assert_eq!(r.dungeon.tiles, floors[0].tiles);
        r.hold_foes = true;
        let exit = r.dungeon.exit;
        let at = r.dungeon.rooms[exit].center();
        let frame = |p: [f32; 2]| PlayerFrame { pos: p, height: 0.0, face: 0.0, pound_land: false, dive_started: false, punch_started: false, grounded: true };
        let mut locked = false;
        for _ in 0..60 {
            r.tick(&frame(at), &Intent::default());
            locked |= r.ev.stairs_locked;
        }
        assert!(locked && r.floor == 1, "the exit waits for its guards");
        assert!(r.rooms[exit].active && r.alive_enemies > 0, "the exit room's pack woke");
        for e in r.enemies.iter().filter(|e| e.alive) {
            assert!(r.dungeon.walkable(e.pos), "a foe off the floor at {:?}", e.pos);
        }
        for e in r.enemies.iter_mut() {
            e.alive = false;
        }
        r.alive_enemies = 0;
        r.rooms[exit].cleared = true;
        for _ in 0..40 {
            r.tick(&frame(at), &Intent::default());
        }
        assert_eq!(r.floor, 2);
        assert_eq!(r.dungeon.tiles, floors[1].tiles, "the second planned floor");
    }

    #[test]
    fn the_room_cap_keeps_floors_short() {
        let d = build(&toy(), &toy_site(2), false).unwrap();
        assert!(connected(&d));
        // Only the first two scene rooms: nothing past the second doorway.
        let far = d.tiles.iter().enumerate().filter(|(_, t)| **t != Tile::Void).map(|(i, _)| d.tile_center((i % d.w) as i32, (i / d.w) as i32)[0] + d.origin[0] as f32).fold(0.0f32, f32::max);
        // (The doorway out of the second room belongs to it, so the floor stops at that doorway's far end.)
        assert!(far < 4800.0 + RTILE, "reached x {far}");
        assert!(d.rooms.len() >= 3, "two scene rooms still make three fights: {}", d.rooms.len());
    }

    #[test]
    fn rooms_own_their_tiles_and_centres_are_open() {
        let d = build(&toy(), &toy_site(8), true).unwrap();
        assert_eq!(d.rooms[d.exit].kind, RoomKind::Boss);
        for (r, room) in d.rooms.iter().enumerate() {
            let c = room.center();
            assert_eq!(d.room_at(c), Some(r), "room {r}'s centre lies in it");
            assert!(d.circle_free(c, 90.0), "room {r}'s centre is open");
            assert!(room.area > 0 && d.in_room(r, c, -50.0));
        }
        assert!(d.tiles.iter().any(|t| *t == Tile::BossRoom));
    }

    #[test]
    fn the_fence_closes_every_open_edge_and_faces_the_floor() {
        let d = build(&toy(), &toy_site(8), false).unwrap();
        let quads = fence(&d);
        assert!(!quads.is_empty());
        let mut edges = 0;
        for y in 0..d.h as i32 {
            for x in 0..d.w as i32 {
                if d.is_floor_tile(x, y) {
                    edges += [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().filter(|(dx, dy)| !d.is_floor_tile(x + dx, y + dy)).count();
                }
            }
        }
        assert_eq!(quads.len(), edges);
        for (q, n) in &quads {
            // The normal points from the quad toward the floor tile it guards.
            let mid = [(q[0][0] + q[1][0]) / 2.0 + n[0] * 20.0, (q[0][2] + q[1][2]) / 2.0 + n[2] * 20.0];
            assert!(d.walkable(d.plane([mid[0], 0.0, mid[1]])));
        }
    }

    #[test]
    fn the_site_draw_is_deterministic_and_respects_the_import() {
        let all = |_: &Site| true;
        let a = pick_sites(7, &all, &[]).unwrap();
        assert_eq!(a.iter().map(|s| s.key).collect::<Vec<_>>(), pick_sites(7, &all, &[]).unwrap().iter().map(|s| s.key).collect::<Vec<_>>());
        assert_eq!(a.iter().map(|s| s.tier).collect::<Vec<_>>(), vec![1, 2, 3]);
        let keys: std::collections::BTreeSet<Vec<&str>> = (0..40).map(|s| pick_sites(s, &all, &[]).unwrap().iter().map(|x| x.key).collect()).collect();
        assert!(keys.len() > 4, "seeds vary the dungeons");
        // Only the Deku Tree imported: no tier-2 site, no plan.
        assert!(pick_sites(1, &|s: &Site| s.course == "deku_tree", &[]).is_none());
        assert!(pick_sites(1, &|_: &Site| false, &[]).is_none());
        let forced = pick_sites(3, &all, &["deku_basement", "forest_court", "phantom_lair"]).unwrap();
        assert_eq!(forced.iter().map(|s| s.key).collect::<Vec<_>>(), vec!["deku_basement", "forest_court", "phantom_lair"]);
    }

    fn assets() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// Every site on the imported data: it builds, every floor tile is reachable, every room holds its pack (open spawn
    /// spots on its own floor), the exit, the treasure and the boss stand on open, reachable floor, and the build is the
    /// same twice.
    #[test]
    fn every_site_builds_a_reachable_floor_on_the_imported_dungeons() {
        let root = assets();
        if !SITES.iter().any(|s| site_available(&root, s)) {
            return eprintln!("no OoT data imported; skipped");
        }
        let mut failures = vec![];
        for s in SITES.iter().filter(|s| site_available(&root, s)) {
            let boss = s.tier == 3;
            let d = match build_site(&root, s, boss) {
                Ok(d) => d,
                Err(e) => {
                    failures.push(e);
                    continue;
                }
            };
            let n = d.tiles.iter().filter(|t| **t != Tile::Void).count();
            eprintln!("{}: {} tiles, {} rooms, exit {} ({:?}), grid {}x{}", s.key, n, d.rooms.len(), d.exit, d.rooms[d.exit].kind, d.w, d.h);
            if !connected(&d) {
                failures.push(format!("{}: floor not connected", s.key));
            }
            if d.rooms.len() < 3 {
                failures.push(format!("{}: {} rooms", s.key, d.rooms.len()));
            }
            for (r, room) in d.rooms.iter().enumerate() {
                let c = room.center();
                if d.room_at(c) != Some(r) || !d.circle_free(c, if r == d.start { 50.0 } else { 90.0 }) {
                    failures.push(format!("{}: room {r} centre not open ({:?})", s.key, c));
                }
                // Spawn spots: the same search `activate_room` runs, from a fixed seed.
                if r != d.start && room.kind != RoomKind::Boss {
                    let mut rng = Rng::new(r as u64 + 11);
                    let (m, h) = (room.mid, room.half());
                    let spots = (0..400)
                        .filter(|_| {
                            let p = [m[0] + rng.range(-h[0] + 180.0, (h[0] - 180.0).max(-h[0] + 181.0)), m[1] + rng.range(-h[1] + 180.0, (h[1] - 180.0).max(-h[1] + 181.0))];
                            d.circle_free(p, 90.0) && d.room_at(p) == Some(r)
                        })
                        .count();
                    if spots < 20 {
                        failures.push(format!("{}: room {r} has {spots}/400 spawn spots", s.key));
                    }
                }
            }
            // The exit (stairs or boss), the treasure and every room centre are reached from the entrance along the foes'
            // own tile walk, which also crosses only steps Mario walks.
            let start = d.rooms[d.start].center();
            for (r, room) in d.rooms.iter().enumerate() {
                if !tile_walk(&d, start, room.center()) {
                    failures.push(format!("{}: room {r} unreachable", s.key));
                }
            }
            if !boss && d.rooms[d.exit].kind != RoomKind::Stairs || boss && d.rooms[d.exit].kind != RoomKind::Boss {
                failures.push(format!("{}: exit kind {:?}", s.key, d.rooms[d.exit].kind));
            }
            let again = build_site(&root, s, boss).unwrap();
            if again.tiles != d.tiles || again.zones != d.zones || again.exit != d.exit {
                failures.push(format!("{}: not deterministic", s.key));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// A whole run plan on the imported data: three floors, the last a boss floor, deterministic for a seed.
    #[test]
    fn a_run_plan_on_real_dungeons_is_deterministic() {
        let root = assets();
        let Ok(a) = plan_run(&root, 99, &[]) else { return eprintln!("no OoT data imported; skipped") };
        let b = plan_run(&root, 99, &[]).unwrap();
        assert_eq!(a.len(), 3);
        assert!(a[2].boss_floor && !a[0].boss_floor && !a[1].boss_floor);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.real, y.real);
            assert_eq!(x.tiles, y.tiles);
        }
    }

    /// Exploration aid (ignored): flood every room box of a scene from its middle and print what a site there would
    /// hold. `SURVEY=dodongos_cavern cargo test survey_scene -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn survey_scene() {
        let root = assets();
        let course = std::env::var("SURVEY").unwrap_or_else(|_| "deku_tree".into());
        let area: i32 = std::env::var("SURVEY_AREA").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        let rooms: usize = std::env::var("SURVEY_ROOMS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        let Ok((world, _)) = scene_world(&root, &course, area) else { return };
        let boxes = crate::zelda::rooms::load_boxes(&root, &course, area);
        let probe = WorldProbe { world: Arc::new(world), boxes: boxes.clone() };
        let leak: &'static str = Box::leak(course.clone().into_boxed_str());
        for b in &boxes {
            // Candidate starts: the standing spots on a 330 grid in the box, nearest the middle first.
            let mid = [(b.min[0] + b.max[0]) / 2.0, (b.min[2] + b.max[2]) / 2.0];
            let mut cands = vec![];
            let mut x = b.min[0] + 165.0;
            while x < b.max[0] {
                let mut z = b.min[2] + 165.0;
                while z < b.max[2] {
                    if let Some(y) = probe.floor(x as f32, z as f32, b.max[1] as f32).map(|h| h.y).filter(|y| (*y as f64) >= b.min[1] - 10.0) {
                        if standable(&probe, x as f32, z as f32, y).is_some() && probe.room(x as f32, y, z as f32) == Some(b.room) {
                            cands.push(([x as f32, y + 60.0, z as f32], (x - mid[0]).hypot(z - mid[1])));
                        }
                    }
                    z += 330.0;
                }
                x += 330.0;
            }
            cands.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some((start, _)) = cands.first() else {
                eprintln!("room {}: no standing spot", b.room);
                continue;
            };
            let site = Site { key: leak, title: "", course: leak, area, start: *start, reach: 5400.0, rooms, tier: 1 };
            match build(&probe, &site, false) {
                Ok(d) => {
                    let n = d.tiles.iter().filter(|t| **t != Tile::Void).count();
                    let hs: Vec<f32> = d.heights.iter().zip(&d.tiles).filter(|(_, t)| **t != Tile::Void).map(|(h, _)| *h).collect();
                    let (lo, hi) = hs.iter().fold((f32::MAX, f32::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
                    eprintln!("room {}: start {:?}: {} tiles, {} zones, heights {:.0}..{:.0}, exit room {} at {:?}", b.room, start.map(|v| v.round()), n, d.rooms.len(), lo, hi, d.exit, d.world(d.rooms[d.exit].center(), 0.0).map(|v| v.round()));
                }
                Err(e) => eprintln!("room {}: start {:?}: {e}", b.room, start.map(|v| v.round())),
            }
        }
    }
}
