//! Star Swarm, exploring the whole course: where the chests are hidden.
//!
//! The mode no longer fences the fight into an arena; the player may walk anywhere the course allows. To give
//! that a reason, chests are scattered over the whole level: some in the open, many in nooks behind walls, on
//! high ledges and roofs, in covered caves and at the ends of long side paths. [`plan`] picks them from the
//! course's own collision (through the [`Probe`] trait, so the logic is engine-free and unit-tested):
//!
//! 1. A flood fill over a coarse grid of standable floors (up to [`LAYERS`] per cell, so roofs and caves count)
//!    from the start finds what can be reached by walking, then by walking plus jumps up ledges.
//! 2. Every reached spot is scored by how hidden it is (enclosed pocket, ledge, covered cave, far dead end).
//! 3. A seeded weighted draw with minimum spacing chooses the sites; their number scales with the reachable area;
//!    the best hidden ones become golden, the next rare, the rest common (a quarter are left in the open).
//!
//! Everything is deterministic for a seed. Positions are returned relative to `origin` (the run's local frame).
use super::swarm_core::Rng;

/// Grid cell of the planning flood fill (SM64 units).
pub const CELL: f32 = 100.0;
/// Floors remembered per cell (the ground, a roof, a cave under it).
pub const LAYERS: usize = 3;
/// Steepest slope walked without a jump (rise over run: about 27 degrees; steeper ground makes the player slide back).
const WALK_SLOPE: f32 = 0.5;
/// Highest ledge one jump gets onto, the longest drop taken, and how many jumps a route may chain (the second phase).
const JUMP_UP: f32 = 190.0;
const JUMPS: usize = 1;
const DROP: f32 = 190.0;
/// Clearance needed over a standing spot (a floor closer than this under a ceiling is a slab, not a room).
pub const HEADROOM: f32 = 150.0;
/// Walls closer than this to a spot (at chest height) make it unusable.
const WALL_R: f32 = 56.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestTier {
    Common,
    Rare,
    /// Needs a key or a survived fight.
    Golden,
}

pub const HIDE_NOOK: u8 = 1;
pub const HIDE_LEDGE: u8 = 2;
pub const HIDE_CAVE: u8 = 4;
pub const HIDE_DEADEND: u8 = 8;

#[derive(Clone, Copy, Debug)]
pub struct ChestSite {
    /// Local x/z (course position minus the run's origin).
    pub pos: [f32; 2],
    /// Floor height (course units).
    pub y: f32,
    pub tier: ChestTier,
    /// Bits of `HIDE_*`: why it is hard to find (0 = lies in the open).
    pub hide: u8,
}

/// What the planner needs to know about the course.
pub trait Probe {
    /// Standable floors at a course x/z, highest first; returns how many were written.
    fn floors(&self, x: f32, z: f32, out: &mut [f32; LAYERS]) -> usize;
    /// Is a wall within `r` of the point?
    fn wall_near(&self, x: f32, y: f32, z: f32, r: f32) -> bool;
}

struct Grid {
    x0: f32,
    z0: f32,
    nx: i32,
    nz: i32,
    /// Per cell: computed flag and floors.
    floors: Vec<([f32; LAYERS], u8)>,
    done: Vec<bool>,
    /// Per node (cell * LAYERS + layer): 0 unvisited, 1 reached walking, 2 reached with jumps. Also the walk distance.
    reach: Vec<u8>,
    dist: Vec<u16>,
    stand: Vec<u8>, // 0 unknown, 1 yes, 2 no
    /// The node each node was reached from (for routes).
    parent: Vec<u32>,
}

impl Grid {
    fn cell(&self, ix: i32, iz: i32) -> Option<usize> {
        (ix >= 0 && iz >= 0 && ix < self.nx && iz < self.nz).then(|| (iz * self.nx + ix) as usize)
    }
    fn centre(&self, ix: i32, iz: i32) -> (f32, f32) {
        (self.x0 + (ix as f32 + 0.5) * CELL, self.z0 + (iz as f32 + 0.5) * CELL)
    }
    fn ensure<P: Probe + ?Sized>(&mut self, probe: &P, c: usize) {
        if self.done[c] {
            return;
        }
        let (ix, iz) = (c as i32 % self.nx, c as i32 / self.nx);
        let (x, z) = self.centre(ix, iz);
        let mut f = [f32::NAN; LAYERS];
        let n = probe.floors(x, z, &mut f);
        self.floors[c] = (f, n as u8);
        self.done[c] = true;
    }
    /// Can something stand on layer `l` of cell `c` (a floor, and no wall at chest height around it)?
    fn standable<P: Probe + ?Sized>(&mut self, probe: &P, c: usize, l: usize) -> bool {
        self.ensure(probe, c);
        if l >= self.floors[c].1 as usize {
            return false;
        }
        let n = c * LAYERS + l;
        if self.stand[n] == 0 {
            let (ix, iz) = (c as i32 % self.nx, c as i32 / self.nx);
            let (x, z) = self.centre(ix, iz);
            let h = self.floors[c].0[l];
            // Not a wall at chest height, and ground under the feet on every side (a ridge a hand wide is no place to stand).
            let mut ok = !probe.wall_near(x, h + 60.0, z, WALL_R);
            for (dx, dz) in [(40.0f32, 0.0f32), (-40.0, 0.0), (0.0, 40.0), (0.0, -40.0)] {
                if !ok {
                    break;
                }
                let mut f = [f32::NAN; LAYERS];
                let cnt = probe.floors(x + dx, z + dz, &mut f);
                ok = f.iter().take(cnt).any(|v| (v - h).abs() <= 60.0);
            }
            self.stand[n] = if ok { 1 } else { 2 };
        }
        self.stand[n] == 1
    }
}

/// Is the step from cell `a` (floor `ha`) to its neighbour `b` (floor `hb`) walled off at foot height? Mario's own wall
/// checks sit 30 and 60 units above his feet (radii 24 and 50); a low wall, a curb or a fence post between two open cells
/// stops him there even though both cells are standable (the walker used to push against such a wall for minutes).
///
/// A diagonal step is checked at its quarter points too, and every checked point needs ground under it near the step's own
/// height: a diagonal used to cut the corner of a thin fence's end (Graveyard) and to cross a crack between two floors that
/// has no floor at all (Koopa Troopa Beach), where Mario's step stops dead although both cells are fine.
fn crossing_blocked<P: Probe + ?Sized>(probe: &P, a: (f32, f32), ha: f32, b: (f32, f32), hb: f32) -> bool {
    let diagonal = a.0 != b.0 && a.1 != b.1;
    let ts: &[f32] = if diagonal { &[0.25, 0.5, 0.75] } else { &[0.5] };
    for &t in ts {
        let (x, z) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        let h = ha + (hb - ha) * t;
        if probe.wall_near(x, ha.max(hb) + 30.0, z, 24.0) || probe.wall_near(x, ha.max(hb) + 60.0, z, 40.0) {
            return true;
        }
        // Ground under the step and a body width to either side of it (Mario stalls at the lip of a crack).
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let l = dx.hypot(dz).max(1.0);
        let (px, pz) = (-dz / l * 35.0, dx / l * 35.0);
        for (ox, oz) in [(0.0, 0.0), (px, pz), (-px, -pz)] {
            let mut f = [f32::NAN; LAYERS];
            let n = probe.floors(x + ox, z + oz, &mut f);
            if !f[..n].iter().any(|v| (v - h).abs() <= 45.0) {
                return true;
            }
        }
    }
    false
}

/// The result of the flood fill, kept for the scoring (and for tests).
pub struct Reach {
    grid: Grid,
    pub start_cell: usize,
    /// Reachable nodes (walking).
    pub walked: usize,
    /// Reachable nodes (walking, then jumping).
    pub total: usize,
}

const DIRS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)];

fn flood<P: Probe + ?Sized>(probe: &P, start: [f32; 3], bounds: [f32; 4]) -> Option<Reach> {
    let nx = (((bounds[2] - bounds[0]) / CELL).ceil() as i32).max(1);
    let nz = (((bounds[3] - bounds[1]) / CELL).ceil() as i32).max(1);
    let cells = (nx * nz) as usize;
    let mut g = Grid { x0: bounds[0], z0: bounds[1], nx, nz, floors: vec![([f32::NAN; LAYERS], 0); cells], done: vec![false; cells], reach: vec![0; cells * LAYERS], dist: vec![0; cells * LAYERS], stand: vec![0; cells * LAYERS], parent: vec![u32::MAX; cells * LAYERS] };
    let sx = ((start[0] - g.x0) / CELL).floor() as i32;
    let sz = ((start[2] - g.z0) / CELL).floor() as i32;
    let sc = g.cell(sx, sz)?;
    g.ensure(probe, sc);
    if g.floors[sc].1 == 0 {
        return None;
    }
    // The start layer: the one nearest to the start's own ground (the lowest floor is wrong under a roof).
    let mut queue: std::collections::VecDeque<(usize, usize)> = Default::default();
    let sl = (0..g.floors[sc].1 as usize).min_by(|a, b| (g.floors[sc].0[*a] - start[1]).abs().total_cmp(&(g.floors[sc].0[*b] - start[1]).abs())).unwrap_or(0);
    g.reach[sc * LAYERS + sl] = 1;
    queue.push_back((sc, sl));
    let mut walked = 0usize;
    // Phase 1: walking only.
    while let Some((c, l)) = queue.pop_front() {
        walked += 1;
        let h = g.floors[c].0[l];
        let d = g.dist[c * LAYERS + l];
        let (ix, iz) = (c as i32 % nx, c as i32 / nx);
        for (dx, dz) in DIRS {
            let Some(q) = g.cell(ix + dx, iz + dz) else { continue };
            g.ensure(probe, q);
            for ql in 0..g.floors[q].1 as usize {
                let n = q * LAYERS + ql;
                if g.reach[n] != 0 || (g.floors[q].0[ql] - h).abs() > WALK_SLOPE * if dx != 0 && dz != 0 { 141.0 } else { 100.0 } || !g.standable(probe, q, ql) {
                    continue;
                }
                if crossing_blocked(probe, g.centre(ix, iz), h, g.centre(ix + dx, iz + dz), g.floors[q].0[ql]) {
                    continue;
                }
                g.reach[n] = 1;
                g.dist[n] = d.saturating_add(1);
                g.parent[n] = (c * LAYERS + l) as u32;
                queue.push_back((q, ql));
            }
        }
    }
    // Phase 2: from everything reached so far, jump up a ledge (or drop down) and walk on from there; up to JUMPS times.
    let mut total = walked;
    for _ in 0..JUMPS {
        let mut seeds: Vec<(usize, usize)> = vec![];
        for c in 0..cells {
            if !g.done[c] {
                continue;
            }
            for l in 0..g.floors[c].1 as usize {
                if g.reach[c * LAYERS + l] != 0 {
                    seeds.push((c, l));
                }
            }
        }
        let mut queue: std::collections::VecDeque<(usize, usize)> = Default::default();
        for (c, l) in seeds {
            let h = g.floors[c].0[l];
            let d = g.dist[c * LAYERS + l];
            let (ix, iz) = (c as i32 % nx, c as i32 / nx);
            for (dx, dz) in DIRS {
                let Some(q) = g.cell(ix + dx, iz + dz) else { continue };
                g.ensure(probe, q);
                for ql in 0..g.floors[q].1 as usize {
                    let n = q * LAYERS + ql;
                    let dh = g.floors[q].0[ql] - h;
                    if g.reach[n] != 0 || dh > JUMP_UP || dh < -DROP || !g.standable(probe, q, ql) {
                        continue;
                    }
                    g.reach[n] = 2;
                    g.dist[n] = d.saturating_add(1);
                    g.parent[n] = (c * LAYERS + l) as u32;
                    total += 1;
                    queue.push_back((q, ql));
                }
            }
        }
        if queue.is_empty() {
            break;
        }
        // Walk (or fall) on from the landing spots.
        while let Some((c, l)) = queue.pop_front() {
            let h = g.floors[c].0[l];
            let d = g.dist[c * LAYERS + l];
            let (ix, iz) = (c as i32 % nx, c as i32 / nx);
            for (dx, dz) in DIRS {
                let Some(q) = g.cell(ix + dx, iz + dz) else { continue };
                g.ensure(probe, q);
                let allow = WALK_SLOPE * if dx != 0 && dz != 0 { 141.0 } else { 100.0 };
                for ql in 0..g.floors[q].1 as usize {
                    let n = q * LAYERS + ql;
                    let dh = g.floors[q].0[ql] - h;
                    if g.reach[n] != 0 || dh > allow || dh < -DROP || !g.standable(probe, q, ql) {
                        continue;
                    }
                    if crossing_blocked(probe, g.centre(ix, iz), h, g.centre(ix + dx, iz + dz), g.floors[q].0[ql]) {
                        continue;
                    }
                    g.reach[n] = 2;
                    g.dist[n] = d.saturating_add(1);
                    g.parent[n] = (c * LAYERS + l) as u32;
                    total += 1;
                    queue.push_back((q, ql));
                }
            }
        }
    }
    Some(Reach { grid: g, start_cell: sc, walked, total })
}

/// Is the spot (course x/z, floor height near `y`) on a node the flood fill reached? Used by tests.
impl Reach {
    pub fn reached(&self, x: f32, z: f32, y: f32) -> bool {
        let g = &self.grid;
        let (ix, iz) = (((x - g.x0) / CELL).floor() as i32, ((z - g.z0) / CELL).floor() as i32);
        let Some(c) = g.cell(ix, iz) else { return false };
        (0..g.floors[c].1 as usize).any(|l| (g.floors[c].0[l] - y).abs() < 40.0 && g.reach[c * LAYERS + l] != 0)
    }
}

/// A walking route (course x, floor y, z) from `start` to the node nearest `goal` (x, y, z), over the same grid the chests were
/// planned on; for the bots that check a chest can really be walked to. `None` when the goal was not reached.
pub fn route<P: Probe + ?Sized>(probe: &P, start: [f32; 3], bounds: [f32; 4], goal: [f32; 3]) -> Option<Vec<[f32; 3]>> {
    let r = flood(probe, start, bounds)?;
    let g = &r.grid;
    let (ix, iz) = (((goal[0] - g.x0) / CELL).floor() as i32, ((goal[2] - g.z0) / CELL).floor() as i32);
    let c = g.cell(ix, iz)?;
    let l = (0..g.floors[c].1 as usize).filter(|l| g.reach[c * LAYERS + l] != 0).min_by(|a, b| (g.floors[c].0[*a] - goal[1]).abs().total_cmp(&(g.floors[c].0[*b] - goal[1]).abs()))?;
    let mut node = (c * LAYERS + l) as u32;
    let mut out = vec![];
    while node != u32::MAX {
        let (cc, ll) = (node as usize / LAYERS, node as usize % LAYERS);
        let (x, z) = g.centre(cc as i32 % g.nx, cc as i32 / g.nx);
        out.push([x, g.floors[cc].0[ll], z]);
        node = g.parent[node as usize];
    }
    out.reverse();
    Some(out)
}

struct Cand {
    pos: [f32; 2],
    y: f32,
    w: f32,
    hide: u8,
    key: f32,
    /// Level, reached neighbours two cells out (of 8): how much room there is around the spot.
    open: i32,
}

/// The shrines of a level (Megabonk-style interactables, see `swarm_shrines.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShrineKind {
    /// Stand in its circle while it charges (the swarm comes for you): a stat boost and a card.
    Charge,
    /// Touch it to call a hard wave with an elite; survive it for two cards and gold.
    Challenge,
    /// A cursed bargain: gold, a card and more XP now, tougher and more numerous foes for the rest of the run.
    Greed,
    /// Summons a boss on the spot (it drops a chest like any boss) and pays a little gold.
    Altar,
    /// The duplicator: pay gold for a free level of something you already carry (up to three times, dearer each time).
    Duplicator,
}

pub const SHRINE_KINDS: [ShrineKind; 5] = [ShrineKind::Charge, ShrineKind::Challenge, ShrineKind::Greed, ShrineKind::Altar, ShrineKind::Duplicator];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShrineSite {
    /// Local x/z, like a chest's.
    pub pos: [f32; 2],
    pub y: f32,
    pub kind: ShrineKind,
}

/// A breakable pot (gold inside, now and then a heart).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PotSite {
    pub pos: [f32; 2],
    pub y: f32,
}

/// Everything the planner places on a level.
#[derive(Clone, Debug, Default)]
pub struct WorldPlan {
    pub chests: Vec<ChestSite>,
    pub shrines: Vec<ShrineSite>,
    pub pots: Vec<PotSite>,
}

/// Plan the chests of a course. `bounds` is `[x0, z0, x1, z1]` in course units around everything that may be walked;
/// `start` the player's start (x, floor y, z); the sites come back relative to `origin`. `seed` fixes the layout.
pub fn plan<P: Probe + ?Sized>(probe: &P, start: [f32; 3], bounds: [f32; 4], origin: [f32; 2], seed: u64) -> Vec<ChestSite> {
    plan_world(probe, start, bounds, origin, seed).chests
}

/// The chests (exactly as [`plan`]), then the shrines and the pots, from the same flood fill: every site is a spot the
/// player can reach. Shrines want room around them (a charge circle is fought in) and keep apart from the chests and each
/// other; pots come in little clusters of two or three, mostly in the hidden corners.
pub fn plan_world<P: Probe + ?Sized>(probe: &P, start: [f32; 3], bounds: [f32; 4], origin: [f32; 2], seed: u64) -> WorldPlan {
    let Some(mut r) = flood(probe, start, bounds) else { return WorldPlan::default() };
    let g = &mut r.grid;
    let (nx, nz) = (g.nx, g.nz);
    let cells = (nx * nz) as usize;
    // The far end of the walking distances (dead ends sit in the last third).
    let max_d = g.dist.iter().copied().max().unwrap_or(0).max(1);
    let mut rng = Rng::new(seed ^ 0xC4E5_7000_5EED);
    let mut cands: Vec<Cand> = vec![];
    for c in 0..cells {
        if !g.done[c] {
            continue;
        }
        let (ix, iz) = (c as i32 % nx, c as i32 / nx);
        for l in 0..g.floors[c].1 as usize {
            let n = c * LAYERS + l;
            if g.reach[n] == 0 {
                continue;
            }
            let h = g.floors[c].0[l];
            let (x, z) = g.centre(ix, iz);
            let far = ((x - start[0]).powi(2) + (z - start[2]).powi(2)).sqrt();
            if far < 700.0 {
                continue;
            }
            // Room around the spot: neighbours two cells out that are level, standable ground reached too.
            let mut open = 0;
            for (dx, dz) in DIRS {
                let Some(q) = g.cell(ix + dx * 2, iz + dz * 2) else { continue };
                if !g.done[q] {
                    g.ensure(probe, q);
                }
                let any = (0..g.floors[q].1 as usize).any(|ql| (g.floors[q].0[ql] - h).abs() <= 80.0 && g.stand[q * LAYERS + ql] != 2 && g.reach[q * LAYERS + ql] != 0);
                open += any as i32;
            }
            if open < 2 {
                continue; // a pole top or a sliver
            }
            // Exposure: the tallest floor nearby above this one (a roof over a cave).
            let covered = l > 0 && g.floors[c].0[l - 1] - h >= HEADROOM;
            // Height over the typical ground around: the neighbours six cells out.
            let (mut sum, mut cnt) = (0.0, 0);
            for (dx, dz) in DIRS {
                if let Some(q) = g.cell(ix + dx * 6, iz + dz * 6) {
                    g.ensure(probe, q);
                    if g.floors[q].1 > 0 {
                        sum += g.floors[q].0[g.floors[q].1 as usize - 1];
                        cnt += 1;
                    }
                }
            }
            let rel = if cnt > 0 { h - sum / cnt as f32 } else { 0.0 };
            let mut hide = 0u8;
            let mut w = 0.25f32;
            if open <= 4 {
                hide |= HIDE_NOOK;
                w += 2.0;
            }
            if g.reach[n] == 2 && rel > 90.0 {
                hide |= HIDE_LEDGE;
                w += 2.6;
            } else if rel > 260.0 {
                hide |= HIDE_LEDGE;
                w += 1.6;
            }
            if covered {
                hide |= HIDE_CAVE;
                w += 2.2;
            }
            if g.reach[n] == 1 && g.dist[n] as f32 > max_d as f32 * 0.66 && open <= 6 {
                hide |= HIDE_DEADEND;
                w += 1.4;
            }
            // Efraimidis-Spirakis key: bigger weights sort first.
            let u = rng.f32().max(1e-6);
            cands.push(Cand { pos: [x - origin[0], z - origin[1]], y: h, w, hide, key: u.ln() / w, open });
        }
    }
    if cands.is_empty() {
        return WorldPlan::default();
    }
    cands.sort_by(|a, b| b.key.total_cmp(&a.key));
    let area = r.total as f32 * CELL * CELL;
    let want = ((6.0 + area / 1.8e6) as usize).clamp(10, 36);
    let open_quota = want / 4;
    let mut picked: Vec<usize> = vec![];
    let mut spacing = 1100.0f32;
    let far_enough = |picked: &[usize], cands: &[Cand], i: usize, sp: f32| picked.iter().all(|&j| {
        let (a, b) = (cands[i].pos, cands[j].pos);
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) > sp * sp || (cands[i].y - cands[j].y).abs() > 300.0 && (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) > 400.0 * 400.0
    });
    for pass in 0..6 {
        // First the open-air quota (lowest weights), then the hidden ones in weight-draw order.
        if pass == 0 {
            for i in (0..cands.len()).rev() {
                if picked.len() >= open_quota {
                    break;
                }
                if cands[i].hide == 0 && far_enough(&picked, &cands, i, spacing) && rng.chance(0.02) {
                    picked.push(i);
                }
            }
        }
        for i in 0..cands.len() {
            if picked.len() >= want {
                break;
            }
            // The first passes take hidden spots only; open ground fills what is left.
            if pass < 3 && cands[i].hide == 0 {
                continue;
            }
            if !picked.contains(&i) && far_enough(&picked, &cands, i, spacing) {
                picked.push(i);
            }
        }
        if picked.len() >= want {
            break;
        }
        spacing *= 0.75;
    }
    // Tiers: the best hidden spots are golden, the next rare.
    picked.sort_by(|&a, &b| cands[b].w.total_cmp(&cands[a].w));
    let golden = (picked.len() / 6).max(1);
    let rare = picked.len() / 3;
    let chests: Vec<ChestSite> = picked
        .iter()
        .enumerate()
        .map(|(rank, &i)| {
            let c = &cands[i];
            let tier = if rank < golden { ChestTier::Golden } else if rank < golden + rare { ChestTier::Rare } else { ChestTier::Common };
            ChestSite { pos: c.pos, y: c.y, tier, hide: c.hide }
        })
        .collect();
    // Shrines and pots draw from their own stream, so the chests stay exactly what they were.
    let mut rng = Rng::new(seed ^ 0x5A21_7E00_0B07);
    let d2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let used: Vec<[f32; 2]> = chests.iter().map(|c| c.pos).collect();
    // Shrines: roomy, level spots in a random order, apart from chests (600) and from each other.
    let want_shrines = ((3.0 + area / 4.0e6) as usize).clamp(4, 9);
    let mut order: Vec<usize> = (0..cands.len()).filter(|&i| cands[i].open >= 7).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.below(i as u32 + 1) as usize);
    }
    let mut shrine_at: Vec<usize> = vec![];
    let mut spacing = 1800.0f32;
    for _ in 0..4 {
        for &i in &order {
            if shrine_at.len() >= want_shrines {
                break;
            }
            let p = cands[i].pos;
            if !shrine_at.contains(&i) && used.iter().all(|&u| d2(u, p) > 600.0 * 600.0) && shrine_at.iter().all(|&j| d2(cands[j].pos, p) > spacing * spacing) {
                shrine_at.push(i);
            }
        }
        spacing *= 0.7;
    }
    // One of each kind first (the ones the level has the most use for first), then more charge, challenge and greed.
    let kinds = [ShrineKind::Charge, ShrineKind::Duplicator, ShrineKind::Challenge, ShrineKind::Altar, ShrineKind::Greed, ShrineKind::Charge, ShrineKind::Challenge, ShrineKind::Charge, ShrineKind::Greed];
    let mut kinds: Vec<ShrineKind> = kinds[..shrine_at.len()].to_vec();
    for i in (1..kinds.len()).rev() {
        kinds.swap(i, rng.below(i as u32 + 1) as usize);
    }
    let shrines: Vec<ShrineSite> = shrine_at.iter().zip(kinds).map(|(&i, kind)| ShrineSite { pos: cands[i].pos, y: cands[i].y, kind }).collect();
    // Pots: clusters of two or three around a seeded pick of the spots, hidden corners first.
    let want_pots = ((8.0 + area / 1.5e6) as usize).clamp(10, 30);
    let mut keys: Vec<(f32, usize)> = (0..cands.len()).map(|i| (rng.f32().max(1e-6).ln() / (if cands[i].hide != 0 { 3.0 } else { 1.0 }), i)).collect();
    keys.sort_by(|a, b| b.0.total_cmp(&a.0));
    let taken = |p: [f32; 2]| used.iter().any(|&u| d2(u, p) < 220.0 * 220.0) || shrines.iter().any(|s| d2(s.pos, p) < 450.0 * 450.0);
    let mut pots: Vec<PotSite> = vec![];
    let mut centres: Vec<[f32; 2]> = vec![];
    for &(_, i) in &keys {
        if pots.len() >= want_pots {
            break;
        }
        let c = &cands[i];
        if taken(c.pos) || centres.iter().any(|&q| d2(q, c.pos) < 900.0 * 900.0) {
            continue;
        }
        centres.push(c.pos);
        pots.push(PotSite { pos: c.pos, y: c.y });
        // Neighbours on the same floor join the cluster.
        let extra = 1 + rng.below(2) as usize;
        let mut n = 0;
        for o in cands.iter() {
            if n >= extra || pots.len() >= want_pots {
                break;
            }
            let dd = d2(o.pos, c.pos);
            if dd > 90.0 * 90.0 && dd < 260.0 * 260.0 && (o.y - c.y).abs() < 30.0 && !taken(o.pos) && pots.iter().all(|q| d2(q.pos, o.pos) > 90.0 * 90.0) {
                pots.push(PotSite { pos: o.pos, y: o.y });
                n += 1;
            }
        }
    }
    WorldPlan { chests, shrines, pots }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A flat plane with rectangles of wall, plateaus (raised floors) and roofs, for the placement tests.
    pub struct Toy {
        pub walls: Vec<[f32; 4]>,
        /// (x0, z0, x1, z1, height)
        pub plateaus: Vec<([f32; 4], f32)>,
        /// A roof over a rectangle (a cave under it).
        pub roofs: Vec<([f32; 4], f32)>,
        pub half: f32,
    }

    impl Probe for Toy {
        fn floors(&self, x: f32, z: f32, out: &mut [f32; LAYERS]) -> usize {
            if x.abs() > self.half || z.abs() > self.half {
                return 0;
            }
            let inside = |r: &[f32; 4]| x >= r[0] && x <= r[2] && z >= r[1] && z <= r[3];
            let mut v = vec![];
            for (r, h) in &self.roofs {
                if inside(r) {
                    v.push(*h);
                }
            }
            let mut ground = 0.0;
            for (r, h) in &self.plateaus {
                if inside(r) {
                    ground = *h;
                }
            }
            v.push(ground);
            v.sort_by(|a, b| b.total_cmp(a));
            for (i, h) in v.iter().take(LAYERS).enumerate() {
                out[i] = *h;
            }
            v.len().min(LAYERS)
        }
        fn wall_near(&self, x: f32, _y: f32, z: f32, r: f32) -> bool {
            self.walls.iter().any(|w| x >= w[0] - r && x <= w[2] + r && z >= w[1] - r && z <= w[3] + r)
        }
    }

    pub fn toy() -> Toy {
        Toy {
            half: 3000.0,
            walls: vec![
                // A walled pocket with one gap at its south side, and a long fence.
                [1000.0, 1000.0, 1100.0, 1800.0],
                [1000.0, 1000.0, 1800.0, 1100.0],
                [1700.0, 1000.0, 1800.0, 1800.0],
                [1000.0, 1700.0, 1350.0, 1800.0],
                [-2500.0, -400.0, -500.0, -300.0],
            ],
            plateaus: vec![([-2000.0, 1500.0, -1400.0, 2100.0], 180.0), ([2000.0, -2000.0, 2600.0, -1400.0], 60.0)],
            roofs: vec![([-1800.0, -2600.0, -1200.0, -2000.0], 400.0)],
        }
    }

    fn bounds() -> [f32; 4] {
        [-3100.0, -3100.0, 3100.0, 3100.0]
    }

    #[test]
    fn plans_are_deterministic_and_seed_dependent() {
        let t = toy();
        let a = plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], 7);
        let b = plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], 7);
        assert!(!a.is_empty());
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(&b) {
            assert_eq!((p.pos, p.y, p.tier, p.hide), (q.pos, q.y, q.tier, q.hide));
        }
        let c = plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], 8);
        assert!(a.iter().zip(&c).any(|(p, q)| p.pos != q.pos), "another seed should lay the chests out differently");
    }

    #[test]
    fn every_chest_is_reachable_standable_and_spaced() {
        let t = toy();
        for seed in 1..6u64 {
            let sites = plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], seed);
            let r = flood(&t, [0.0, 0.0, 0.0], bounds()).unwrap();
            for s in &sites {
                assert!(r.reached(s.pos[0], s.pos[1], s.y), "seed {seed}: {s:?} is not reached");
                assert!(!t.wall_near(s.pos[0], s.y, s.pos[1], 40.0), "seed {seed}: {s:?} is in a wall");
                assert!(((s.pos[0]).powi(2) + (s.pos[1]).powi(2)).sqrt() > 600.0, "too near the start");
            }
            for (i, a) in sites.iter().enumerate() {
                for b in &sites[i + 1..] {
                    let d = ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2)).sqrt();
                    assert!(d > 380.0, "seed {seed}: chests {d} apart");
                }
            }
        }
    }

    #[test]
    fn counts_scale_with_the_level_and_tiers_are_mixed() {
        let big = toy();
        let small = Toy { half: 1200.0, walls: vec![], plateaus: vec![], roofs: vec![], ..toy() };
        let a = plan(&big, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], 3);
        let b = plan(&small, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], 3);
        assert!(a.len() > b.len(), "{} vs {}", a.len(), b.len());
        assert!((10..=36).contains(&a.len()) && b.len() >= 5, "{} {}", a.len(), b.len());
        let n = |t: ChestTier| a.iter().filter(|s| s.tier == t).count();
        assert!(n(ChestTier::Golden) >= 1 && n(ChestTier::Rare) >= 1 && n(ChestTier::Common) >= 1);
        assert!(a.iter().any(|s| s.hide == 0), "some chests lie in the open");
        assert!(a.iter().filter(|s| s.hide != 0).count() * 3 >= a.len(), "a good share of the chests are hidden");
    }

    #[test]
    fn ledges_roofs_and_caves_are_used_and_sealed_rooms_are_not() {
        let mut t = toy();
        // A fully sealed room nobody can enter.
        t.walls.extend([[-2800.0, 2300.0, -2200.0, 2400.0], [-2800.0, 2800.0, -2200.0, 2900.0], [-2800.0, 2300.0, -2700.0, 2900.0], [-2300.0, 2300.0, -2200.0, 2900.0]]);
        let mut hit = [false; 3];
        for seed in 1..30u64 {
            for s in plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], seed) {
                assert!(!(s.pos[0] > -2700.0 && s.pos[0] < -2300.0 && s.pos[1] > 2400.0 && s.pos[1] < 2800.0), "chest inside the sealed room: {s:?}");
                hit[0] |= s.hide & HIDE_NOOK != 0;
                hit[1] |= s.hide & HIDE_LEDGE != 0;
                hit[2] |= s.hide & HIDE_CAVE != 0;
            }
        }
        assert_eq!(hit, [true, true, true], "nook, ledge and cave chests all occur");
    }

    #[test]
    fn origin_shifts_the_positions_only() {
        let t = toy();
        let a = plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], 5);
        let b = plan(&t, [0.0, 0.0, 0.0], bounds(), [100.0, -50.0], 5);
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(&b) {
            assert!((p.pos[0] - 100.0 - q.pos[0]).abs() < 1e-3 && (p.pos[1] + 50.0 - q.pos[1]).abs() < 1e-3);
        }
    }

    #[test]
    fn shrines_and_pots_are_reachable_spaced_and_deterministic() {
        let t = toy();
        let r = flood(&t, [0.0, 0.0, 0.0], bounds()).unwrap();
        let d = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let mut kinds_seen = std::collections::HashSet::new();
        for seed in 1..8u64 {
            let w = plan_world(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], seed);
            let again = plan_world(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], seed);
            assert_eq!(w.shrines, again.shrines);
            assert_eq!(w.pots, again.pots);
            // The chests are exactly what `plan` gives.
            let c = plan(&t, [0.0, 0.0, 0.0], bounds(), [0.0, 0.0], seed);
            assert_eq!(c.iter().map(|c| c.pos).collect::<Vec<_>>(), w.chests.iter().map(|c| c.pos).collect::<Vec<_>>());
            assert!(w.shrines.len() >= 4, "seed {seed}: {} shrines", w.shrines.len());
            assert!(w.pots.len() >= 10, "seed {seed}: {} pots", w.pots.len());
            for s in &w.shrines {
                kinds_seen.insert(format!("{:?}", s.kind));
                assert!(r.reached(s.pos[0], s.pos[1], s.y), "seed {seed}: {s:?} is not reached");
                assert!(w.chests.iter().all(|c| d(c.pos, s.pos) > 590.0), "seed {seed}: a shrine on a chest");
            }
            for (i, a) in w.shrines.iter().enumerate() {
                for b in &w.shrines[i + 1..] {
                    assert!(d(a.pos, b.pos) > 600.0, "seed {seed}: shrines too close");
                }
            }
            for p in &w.pots {
                assert!(r.reached(p.pos[0], p.pos[1], p.y), "seed {seed}: pot {p:?} is not reached");
                assert!(w.shrines.iter().all(|s| d(s.pos, p.pos) > 440.0));
            }
            // One of each kind on a level with room for five.
            if w.shrines.len() >= 5 {
                for k in SHRINE_KINDS {
                    assert!(w.shrines.iter().any(|s| s.kind == k), "seed {seed}: no {k:?}");
                }
            }
        }
        assert_eq!(kinds_seen.len(), SHRINE_KINDS.len());
    }
}
