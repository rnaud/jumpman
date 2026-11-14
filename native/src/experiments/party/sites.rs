//! Where Sunstone Shindig happens on the real Bob-omb Battlefield: the board laid on the course floor (the Hold the
//! Gate pattern: a frame, a ground sampled from the course collision once, bilinear lookups after) and one real spot per
//! mini-game (an arena sampled into a [`Field`] of standable points the rules and the bots use).
use super::board::Board;
use super::minis::{Field, Mini};
use crate::experiments::crowd_route::walkable_normal;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::surfaces::SURFACE_DEATH_PLANE;

/// The course everything happens on.
pub const COURSE: &str = "bob";
/// SM64 units per board unit (board layouts are authored in board units).
pub const BOARD_SCALE: f64 = 150.0;

/// A board laid on the course: centre (course x, z) and quarter turns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: [f64; 3],
    pub turn: u8,
}

impl Frame {
    /// Course (x, z) of a board-local point (board units).
    pub fn to_world(&self, p: [f32; 2]) -> [f64; 2] {
        let (x, z) = (p[0] as f64 * BOARD_SCALE, p[1] as f64 * BOARD_SCALE);
        let (x, z) = match self.turn % 4 {
            0 => (x, z),
            1 => (-z, x),
            2 => (-x, -z),
            _ => (z, -x),
        };
        [self.origin[0] + x, self.origin[2] + z]
    }
}

/// Height of a standable floor at (x, z) found from `probe` down, or None (no floor, too steep for Mario, the death
/// plane, under water).
pub fn standable(world: &CollisionWorld, x: f64, z: f64, probe: f64) -> Option<f64> {
    let f = world.find_floor(x, probe, z);
    let s = f.surface.as_ref()?;
    if s.kind == SURFACE_DEATH_PLANE || s.normal[1] < walkable_normal(s).max(0.8) || world.water_level(x, z) > f.height - 30.0 {
        return None;
    }
    Some(f.height)
}

/// A good spot for a board space: standable, near level over a disc of `r`, no wall in it and open sky above.
pub fn space_ground(world: &CollisionWorld, x: f64, z: f64, probe: f64, r: f64) -> Option<f64> {
    let h = standable(world, x, z, probe)?;
    for (dx, dz) in [(-r, 0.0), (r, 0.0), (0.0, -r), (0.0, r)] {
        let k = standable(world, x + dx, z + dz, h + 200.0)?;
        if (k - h).abs() > 70.0 {
            return None;
        }
    }
    let mut at = [x, h + 60.0, z];
    if world.f32_find_wall_collision(&mut at, 0.0, r * 0.8) > 0 || world.find_ceil(x, h + 20.0, z).height < h + 500.0 {
        return None;
    }
    Some(h)
}

/// Board-local spot of place `i` (0 = winner) in the closing ceremony: a row in the middle of the board, the winner in
/// the centre.
pub fn podium_spot(i: usize) -> [f32; 2] {
    [[0.0, 0.0], [-1.4, 0.35], [1.4, 0.35], [2.8, 0.7]][i]
}

/// How well `board` fits at `origin` (x, z) / `turn`: (spread of space heights, mean height), or None when a space has
/// no good ground or a path between two spaces climbs too steeply for a piece to walk.
pub fn board_fit(world: &CollisionWorld, obstacles: &[[f64; 3]], board: &Board, origin: [f64; 2], turn: u8) -> Option<(f64, f64)> {
    let frame = Frame { origin: [origin[0], 0.0, origin[1]], turn };
    let mut hs = Vec::with_capacity(board.spaces.len());
    for s in &board.spaces {
        let [x, z] = frame.to_world(s.pos);
        let h = space_ground(world, x, z, 20000.0, 55.0)?;
        if obstacles.iter().any(|o| (o[0] - x).hypot(o[2] - z) < 150.0 && (o[1] - h).abs() < 400.0) {
            return None;
        }
        hs.push(h);
    }
    // The closing ceremony's spots in the middle of the board are clear too.
    for i in 0..4 {
        let [x, z] = frame.to_world(podium_spot(i));
        let h = standable(world, x, z, 20000.0)?;
        if obstacles.iter().any(|o| (o[0] - x).hypot(o[2] - z) < 160.0 && (o[1] - h).abs() < 400.0) {
            return None;
        }
    }
    for (a, edges) in board.edges.iter().enumerate() {
        for e in edges {
            let (pa, pb) = (frame.to_world(board.spaces[a].pos), frame.to_world(board.spaces[e.to].pos));
            let len = (pb[0] - pa[0]).hypot(pb[1] - pa[1]);
            let n = (len / 60.0).ceil().max(1.0) as usize;
            let mut prev = hs[a];
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let h = standable(world, pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t, prev + 250.0)?;
                if (h - prev).abs() > 60.0 {
                    return None;
                }
                prev = h;
            }
        }
    }
    let mean = hs.iter().sum::<f64>() / hs.len() as f64;
    let (lo, hi) = hs.iter().fold((f64::MAX, f64::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
    Some((hi - lo, mean))
}

/// Where each board lies on the battlefield: course (x, z) of its centre and quarter turns. Chosen with
/// [`tests::scan_for_board_sites`] (every space on good level ground, every path walkable) and checked again by a test.
pub const BOARD_SITES: [([f64; 2], u8); 3] = [([-5500.0, 5100.0], 0), ([3100.0, 6100.0], 0), ([-1100.0, 3900.0], 0)];

/// A mini-game's spot on the battlefield.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MiniSite {
    /// Course (x, z) of the arena's centre.
    pub center: [f64; 2],
    /// Direction the camera looks (x, z), so the stick's "up" runs away from it.
    pub look: [f64; 2],
}

/// The real spot each mini-game is played on (by [`Mini::idx`]).
pub fn mini_site(m: Mini) -> MiniSite {
    let (center, look) = match m {
        // The rolling hills east of the mountain, below the floating island.
        Mini::CoinScramble => ([5000.0, 3600.0], [0.0, -1.0]),
        // King Bob-omb's summit.
        Mini::SummitCrown => ([1700.0, -5450.0], [0.0, -1.0]),
        // The fenced field of wooden posts north of the start.
        Mini::BombRain => ([-5100.0, 2100.0], [0.0, -1.0]),
        // The floating island.
        Mini::StarTag => ([3750.0, 1250.0], [0.0, -1.0]),
        // The upper meadow in the north-west.
        Mini::GoombaStomp => ([-5900.0, -3000.0], [0.0, -1.0]),
    };
    MiniSite { center, look }
}

/// Sample a mini-game's arena from the course collision.
pub fn sample_field(world: &CollisionWorld, m: Mini) -> Field {
    let site = mini_site(m);
    let c = site.center;
    let top = standable(world, c[0], c[1], 20000.0).unwrap_or_else(|| world.find_floor(c[0], 20000.0, c[1]).height);
    Field::from_heights(c, m.radius(), 100.0, |x, z| standable(world, x, z, top + 700.0))
}

/// The ground under a laid board: heights sampled on a grid once (bilinear lookups after).
pub struct Ground {
    pub frame: Frame,
    x0: f32,
    z0: f32,
    step: f32,
    nx: usize,
    nz: usize,
    h: Vec<f32>,
}

impl Ground {
    /// A flat stand-in (no game data, unit tests).
    pub fn flat(frame: Frame) -> Self {
        Self { frame, x0: 0.0, z0: 0.0, step: 1.0, nx: 1, nz: 1, h: vec![frame.origin[1] as f32] }
    }

    pub fn sample(world: &CollisionWorld, origin: [f64; 2], turn: u8, half: [f32; 2]) -> Self {
        let base = standable(world, origin[0], origin[1], 20000.0).unwrap_or_else(|| world.find_floor(origin[0], 20000.0, origin[1]).height);
        let frame = Frame { origin: [origin[0], base, origin[1]], turn };
        let step = 0.5f32;
        let (x0, z0) = (-half[0], -half[1]);
        let (nx, nz) = ((half[0] * 2.0 / step).ceil() as usize + 1, (half[1] * 2.0 / step).ceil() as usize + 1);
        let mut h = Vec::with_capacity(nx * nz);
        for j in 0..nz {
            for i in 0..nx {
                let w = frame.to_world([x0 + i as f32 * step, z0 + j as f32 * step]);
                let f = world.find_floor(w[0], base + 1500.0, w[1]);
                let y = if f.surface.is_some() && (f.height - base).abs() < 1500.0 { f.height } else { base };
                h.push(y as f32);
            }
        }
        Self { frame, x0, z0, step, nx, nz, h }
    }

    /// Course height under a board-local point.
    pub fn height(&self, p: [f32; 2]) -> f64 {
        if self.nx < 2 {
            return self.h[0] as f64;
        }
        let fx = ((p[0] - self.x0) / self.step).clamp(0.0, (self.nx - 1) as f32 - 0.001);
        let fz = ((p[1] - self.z0) / self.step).clamp(0.0, (self.nz - 1) as f32 - 0.001);
        let (i, j) = (fx as usize, fz as usize);
        let (tx, tz) = (fx - i as f32, fz - j as f32);
        let at = |i: usize, j: usize| self.h[j * self.nx + i];
        let a = at(i, j) + (at(i + 1, j) - at(i, j)) * tx;
        let b = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * tx;
        (a + (b - a) * tz) as f64
    }

    /// Course position `lift` units above the ground under a board-local point.
    pub fn world(&self, p: [f32; 2], lift: f64) -> [f64; 3] {
        let w = self.frame.to_world(p);
        [w[0], self.height(p) + lift, w[1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::{CourseRequest, LoadedCourse};

    fn bob() -> Option<LoadedCourse> {
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !assets.join(format!("courses/{COURSE}.json")).exists() {
            return None;
        }
        Some(LoadedCourse::load(&assets, CourseRequest::new(COURSE)).expect("course loads"))
    }

    /// The course's objects (trees, crates, posts, signs...): no board space may sit on one.
    fn obstacles(c: &LoadedCourse) -> Vec<[f64; 3]> {
        c.objects.objects.iter().map(|o| o.pos).collect()
    }

    #[test]
    fn frames_turn_by_quarters() {
        let f = Frame { origin: [1000.0, 0.0, 2000.0], turn: 1 };
        assert_eq!(f.to_world([1.0, 0.0]), [1000.0, 2150.0]);
        assert_eq!(Frame { turn: 0, ..f }.to_world([1.0, 2.0]), [1150.0, 2300.0]);
    }

    /// Every board lies on the real battlefield: each space on good, level ground and every path walkable.
    #[test]
    fn the_boards_fit_the_real_battlefield() {
        let Some(c) = bob() else { return };
        for (id, site) in BOARD_SITES.iter().enumerate() {
            let b = super::super::board::board(id);
            let fit = board_fit(&c.world, &obstacles(&c), &b, site.0, site.1);
            eprintln!("board {id}: {fit:?}");
            let (spread, _) = fit.unwrap_or_else(|| panic!("board {id} does not fit at {site:?}"));
            assert!(spread < 400.0, "board {id} is too hilly: {spread}");
        }
    }

    /// Every mini-game arena is a real, mostly standable spot of the battlefield, at the height it is meant to be.
    #[test]
    fn the_mini_game_spots_are_real_floor() {
        let Some(c) = bob() else { return };
        for m in super::super::minis::ALL_MINIS {
            let f = sample_field(&c.world, m);
            eprintln!("{:?}: centre {:?}, coverage {:.2}, {} points, low {:.0}", m, f.center.map(|v| v.round()), f.coverage(), f.points().len(), f.low);
            assert!(f.coverage() > 0.55, "{m:?}: only {:.2} of the arena is floor", f.coverage());
            for p in 0..4 {
                assert!(f.on(f.start(p, 0.5)), "{m:?}: start {p}");
            }
        }
        // The summit and the floating island are high up; the bombs fall in the low valley.
        assert!(sample_field(&c.world, Mini::SummitCrown).center[1] > 3500.0);
        assert!(sample_field(&c.world, Mini::StarTag).center[1] > 2500.0);
    }

    /// `cargo test --release party::sites::tests::map_of_the_battlefield -- --ignored --nocapture`: an ASCII height map of
    /// the standable floor (digits = height / 500, `.` steep / none) to find spots by eye.
    #[test]
    #[ignore]
    fn map_of_the_battlefield() {
        let Some(c) = bob() else { return };
        let w = &c.world;
        let step = 250.0;
        let mut z = -8000.0;
        while z <= 8000.0 {
            let mut line = format!("{z:>6} ");
            let mut x = -8000.0;
            while x <= 8000.0 {
                line.push(match standable(w, x, z, 20000.0) {
                    Some(h) => std::char::from_digit(((h / 500.0).floor().clamp(0.0, 35.0)) as u32, 36).unwrap_or('#'),
                    None => '.',
                });
                x += step;
            }
            eprintln!("{line}");
            z += step;
        }
    }

    /// The course's objects by kind and place (to find the summit, the Chain Chomp, the start...).
    #[test]
    #[ignore]
    fn objects_of_the_battlefield() {
        let Some(c) = bob() else { return };
        for o in &c.objects.objects {
            eprintln!("{:?} {:?}", o.behavior, o.pos.map(|v| v.round()));
        }
    }

    /// Search the battlefield for spots where each board fits (the tool that picked the board sites).
    #[test]
    #[ignore]
    fn scan_for_board_sites() {
        let Some(c) = bob() else { return };
        let obs = obstacles(&c);
        for id in 0..super::super::board::BOARD_COUNT {
            let b = super::super::board::board(id);
            let mut found = vec![];
            let mut x = -7500.0;
            while x < 7500.0 {
                let mut z = -7500.0;
                while z < 7500.0 {
                    for q in 0..4u8 {
                        if let Some((spread, mean)) = board_fit(&c.world, &obs, &b, [x, z], q) {
                            found.push((spread, x, z, q, mean));
                        }
                    }
                    z += 200.0;
                }
                x += 200.0;
            }
            found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let mut picked: Vec<(f64, f64, f64, u8, f64)> = vec![];
            for f in &found {
                if picked.iter().all(|p| (p.1 - f.1).hypot(p.2 - f.2) > 1500.0) {
                    picked.push(*f);
                }
            }
            eprintln!("board {id} ({}): {} fits; distinct best {:?}", b.name, found.len(), &picked[..picked.len().min(10)]);
        }
    }
}
