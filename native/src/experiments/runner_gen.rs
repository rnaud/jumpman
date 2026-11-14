//! `runnerGen.ts`: the endless runner's pure, deterministic track generator.
//!
//! The track runs along +X as a chain of segments; each starts where the last ended, so the
//! course is continuous by construction. Every gap and step is clamped against Mario's measured
//! running jump.
use jumpman_sim::math::js_round;

/// Measured with MarioRuntime at full run speed (see the test below).
pub const RUNNING_JUMP_DISTANCE: f64 = 841.0;
pub const RUNNING_JUMP_HEIGHT: f64 = 335.0;
/// Longest gap, easy -> hard.
pub const GAP_LIMIT: [f64; 2] = [260.0, 460.0];
/// Highest step up onto the next floor.
pub const RISE_LIMIT: [f64; 2] = [90.0, 160.0];
/// Hurdles must clear with a plain jump.
pub const HURDLE_LIMIT: f64 = 110.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Floor,
    Hurdle,
    Pillar,
    Bridge,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub x0: f64,
    pub x1: f64,
    /// Centre and width across Z.
    pub z: f64,
    pub width: f64,
    /// Top surface height and thickness below it.
    pub top: f64,
    pub height: f64,
    pub theme: usize,
}

/// A boulder that rolls back and forth across the track.
#[derive(Clone, Debug)]
pub struct Hazard {
    pub x: f64,
    pub top: f64,
    pub radius: f64,
    /// Half the sweep across Z, and the sweep period in ticks.
    pub sweep: f64,
    pub period: f64,
    pub phase: f64,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub index: usize,
    pub kind: &'static str,
    pub difficulty: f64,
    pub theme: usize,
    pub blocks: Vec<Block>,
    pub coins: Vec<[f64; 3]>,
    pub hazards: Vec<Hazard>,
    pub start: f64,
    pub end: f64,
    pub exit_top: f64,
}

pub const THEMES: [(&str, usize); 3] = [("Grass Run", 0), ("Brick Fortress", 12), ("Rainbow Sky", 26)];

/// Mulberry32, exactly as in the TS (`Math.imul` = wrapping 32-bit multiply).
pub fn rng(seed: f64) -> impl FnMut() -> f64 {
    let s = (seed.rem_euclid(4294967296.0)) as u32;
    let mut state: u32 = if s == 0 { 0x2545f491 } else { s };
    move || {
        state = state.wrapping_add(0x6d2b79f5);
        let mut t = state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
}

/// About a minute and three quarters of running from gentle to hardest (a 55-segment ramp left the first two minutes flat:
/// a walk bot that only taps jump ran 200 m without a fall).
pub fn difficulty_at(index: usize) -> f64 {
    ((index as f64 - 2.0) / 40.0).clamp(0.0, 1.0)
}

pub fn theme_at(index: usize) -> usize {
    let mut theme = 0;
    for (i, (_, from)) in THEMES.iter().enumerate() {
        if index >= *from {
            theme = i;
        }
    }
    theme
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
pub fn max_gap(d: f64) -> f64 {
    lerp(GAP_LIMIT[0], GAP_LIMIT[1], d)
}
pub fn max_rise(d: f64) -> f64 {
    lerp(RISE_LIMIT[0], RISE_LIMIT[1], d)
}

const WIDE: f64 = 1100.0;

pub struct RunnerTrack {
    segments: Vec<Segment>,
    pub seed: f64,
    pub origin_x: f64,
    pub origin_top: f64,
}

impl RunnerTrack {
    pub fn new(seed: f64, origin_x: f64, origin_top: f64) -> Self {
        Self { segments: vec![], seed, origin_x, origin_top }
    }

    pub fn segment(&mut self, index: usize) -> &Segment {
        while self.segments.len() <= index {
            let next = self.build(self.segments.len());
            self.segments.push(next);
        }
        &self.segments[index]
    }

    fn build(&self, index: usize) -> Segment {
        let mut random = rng(self.seed * 92821.0 + index as f64 * 68917.0);
        let previous = if index > 0 { self.segments.get(index - 1) } else { None };
        let start = previous.map_or(self.origin_x, |p| p.end);
        let top = previous.map_or(self.origin_top, |p| p.exit_top);
        let difficulty = difficulty_at(index);
        let theme = theme_at(index);
        let mut seg = Segment { index, kind: "run", difficulty, theme, blocks: vec![], coins: vec![], hazards: vec![], start, end: start, exit_top: top };

        macro_rules! range {
            ($a:expr, $b:expr) => {{
                let (a, b): (f64, f64) = ($a, $b);
                a + (b - a) * random()
            }};
        }
        let floor = |seg: &mut Segment, x0: f64, x1: f64, floor_top: f64, z: f64, width: f64, kind: BlockKind| {
            seg.blocks.push(Block { kind, x0, x1, z, width, top: floor_top, height: if kind == BlockKind::Bridge { 40.0 } else { 260.0 }, theme });
        };
        let coin_line = |seg: &mut Segment, x0: f64, x1: f64, y: f64, z: f64, count: usize| {
            for i in 0..count {
                seg.coins.push([x0 + (x1 - x0) * (i as f64 + 0.5) / count as f64, y + 90.0, z]);
            }
        };
        // Gap before the next floor, clamped to reach.
        let gap = |want: f64| want.min(max_gap(difficulty));
        let rise = |want: f64| want.clamp(-260.0, max_rise(difficulty).max(-260.0)).min(want.max(-260.0));

        let mut x = start;
        let mut y = top;
        // Only the start floor is plain: segment 1 may already hold an obstacle (at difficulty 0), so the first one
        // comes about 2.7 s after GO instead of ~5 s (MINIGAMES_AUDIT_3.md).
        let table: Vec<(&str, f64)> = if index < 1 {
            vec![("run", 1.0)]
        } else {
            vec![
                ("run", 1.2 - difficulty * 0.8),
                ("gap", 2.0),
                ("hurdles", 1.6),
                ("slalom", 1.2 + difficulty),
                ("narrow", 0.6 + difficulty * 1.4),
                ("stairs", 1.0),
                ("split", 0.4 + difficulty * 1.2),
                ("islands", 0.5 + difficulty * 1.5),
                ("boulders", if difficulty > 0.15 { 0.6 + difficulty * 1.6 } else { 0.0 }),
            ]
        };
        let total: f64 = table.iter().map(|(_, w)| w).sum();
        let mut roll = random() * total;
        let mut kind = table[0].0;
        for (name, weight) in &table {
            roll -= weight;
            if roll < 0.0 {
                kind = name;
                break;
            }
        }
        seg.kind = if index == 0 { "start" } else { kind };

        match seg.kind {
            "start" => {
                floor(&mut seg, x - 1200.0, x + 2600.0, y, 0.0, WIDE, BlockKind::Floor);
                coin_line(&mut seg, x + 900.0, x + 2400.0, y, 0.0, 5);
                x += 2600.0;
            }
            "run" => {
                let length = range!(1600.0, 2400.0);
                floor(&mut seg, x, x + length, y, 0.0, WIDE, BlockKind::Floor);
                let lane = js_round(range!(-1.0, 1.0)) * 300.0;
                coin_line(&mut seg, x + 300.0, x + length - 300.0, y, lane, 6);
                x += length;
            }
            "gap" => {
                let g = gap(range!(220.0, 300.0 + difficulty * 260.0));
                let r = rise(range!(-120.0, 80.0 + difficulty * 80.0));
                // Coins arc over the gap to show the jump.
                for i in 0..4 {
                    let t = (i as f64 + 0.5) / 4.0;
                    seg.coins.push([x + g * t, y + r.max(0.0) * t + 120.0 + (t * std::f64::consts::PI).sin() * 180.0, 0.0]);
                }
                x += g;
                y += r;
                let length = range!(1000.0, 1600.0);
                floor(&mut seg, x, x + length, y, 0.0, WIDE, BlockKind::Floor);
                x += length;
            }
            "hurdles" => {
                let length = range!(2000.0, 2800.0);
                floor(&mut seg, x, x + length, y, 0.0, WIDE, BlockKind::Floor);
                let count = 2 + (random() * (2.0 + difficulty * 2.0)).floor() as usize;
                for i in 0..count {
                    let hx = x + 600.0 + (length - 900.0) * (i as f64 + 0.5) / count as f64;
                    seg.blocks.push(Block { kind: BlockKind::Hurdle, x0: hx - 30.0, x1: hx + 30.0, z: 0.0, width: WIDE, top: y + HURDLE_LIMIT, height: HURDLE_LIMIT, theme });
                    seg.coins.push([hx, y + 300.0, 0.0]);
                }
                x += length;
            }
            "slalom" => {
                let length = range!(2200.0, 3000.0);
                floor(&mut seg, x, x + length, y, 0.0, WIDE, BlockKind::Floor);
                let count = 3 + (random() * (2.0 + difficulty * 3.0)).floor() as usize;
                let mut side = if random() < 0.5 { -1.0 } else { 1.0 };
                for i in 0..count {
                    let px = x + 500.0 + (length - 800.0) * (i as f64 + 0.5) / count as f64;
                    // Each pillar blocks one half of the track; the coins mark the open half.
                    seg.blocks.push(Block { kind: BlockKind::Pillar, x0: px - 90.0, x1: px + 90.0, z: side * 300.0, width: 380.0, top: y + 380.0, height: 380.0, theme });
                    seg.coins.push([px, y + 90.0, -side * 300.0]);
                    side = -side;
                }
                x += length;
            }
            "boulders" => {
                // Boulders sweep across the track; time your run (or jump) past each.
                let length = range!(2200.0, 3000.0);
                floor(&mut seg, x, x + length, y, 0.0, WIDE, BlockKind::Floor);
                let count = 1 + (random() * (1.0 + difficulty * 2.5)).floor() as usize;
                for i in 0..count {
                    let bx = x + 600.0 + (length - 1000.0) * (i as f64 + 0.5) / count as f64;
                    seg.hazards.push(Hazard { x: bx, top: y, radius: 150.0, sweep: WIDE / 2.0 - 160.0, period: js_round(lerp(110.0, 70.0, difficulty)), phase: random() });
                    seg.coins.push([bx, y + 380.0, 0.0]);
                }
                x += length;
            }
            "narrow" => {
                let length = range!(1400.0, 2200.0);
                let width = lerp(520.0, 300.0, difficulty);
                let z = js_round(range!(-1.0, 1.0)) * 200.0;
                floor(&mut seg, x, x + length, y, z, width, BlockKind::Bridge);
                coin_line(&mut seg, x + 200.0, x + length - 200.0, y, z, 7);
                x += length;
            }
            "stairs" => {
                let steps = 3 + (random() * 3.0).floor() as usize;
                let up = random() < 0.65;
                for i in 0..steps {
                    // Change height before each stair after the first, never after the last.
                    if i > 0 {
                        y += if up { rise(range!(70.0, 130.0)) } else { -range!(80.0, 160.0) };
                    }
                    let length = range!(500.0, 700.0);
                    floor(&mut seg, x, x + length, y, 0.0, WIDE, BlockKind::Floor);
                    if i % 2 == 1 {
                        seg.coins.push([x + length / 2.0, y + 90.0, 0.0]);
                    }
                    x += length;
                }
            }
            "split" => {
                // Two lanes over the void; pick one, then they merge again.
                let length = range!(1600.0, 2400.0);
                let width = lerp(420.0, 320.0, difficulty);
                floor(&mut seg, x, x + length, y, -380.0, width, BlockKind::Bridge);
                floor(&mut seg, x, x + length, y, 380.0, width, BlockKind::Bridge);
                let lane = if random() < 0.5 { -380.0 } else { 380.0 };
                coin_line(&mut seg, x + 200.0, x + length - 200.0, y, lane, 6);
                x += length;
            }
            "islands" => {
                let count = 3 + (random() * 3.0).floor() as usize;
                for _ in 0..count {
                    let g = gap(range!(180.0, 260.0 + difficulty * 200.0));
                    let r = rise(range!(-90.0, 60.0 + difficulty * 60.0));
                    seg.coins.push([x + g / 2.0, y + r.max(0.0) + 260.0, 0.0]);
                    x += g;
                    y += r;
                    let length = range!(420.0, 620.0 - difficulty * 120.0);
                    let z = range!(-1.0, 1.0) * 200.0;
                    floor(&mut seg, x, x + length, y, z, lerp(700.0, 460.0, difficulty), BlockKind::Floor);
                    x += length;
                }
            }
            _ => {}
        }
        seg.end = x;
        seg.exit_top = y;
        seg
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Hop {
    pub gap: f64,
    pub rise: f64,
    pub difficulty: f64,
}

/// Every floor-to-floor transition along the centre-most path, for reachability tests.
pub fn floor_hops(segments: &[Segment]) -> Vec<Hop> {
    let mut floors: Vec<(&Block, f64)> =
        segments.iter().flat_map(|seg| seg.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Floor | BlockKind::Bridge)).map(move |b| (b, seg.difficulty))).collect();
    // Stable sort, as Array.prototype.sort.
    floors.sort_by(|p, q| p.0.x0.partial_cmp(&q.0.x0).unwrap());
    // Parallel split lanes share x-ranges; keep one per x-range.
    let mut chain: Vec<(&Block, f64)> = vec![];
    for entry in floors {
        if let Some(last) = chain.last() {
            if (last.0.x0 - entry.0.x0).abs() < 1.0 {
                continue;
            }
        }
        chain.push(entry);
    }
    (1..chain.len())
        .map(|i| Hop { gap: chain[i].0.x0 - chain[i - 1].0.x1, rise: chain[i].0.top - chain[i - 1].0.top, difficulty: chain[i].1.max(chain[i - 1].1) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
    use jumpman_sim::state::*;
    use std::sync::Arc;

    /// A running single jump measured through the real MarioRuntime.
    fn measure_running_jump() -> (f64, f64) {
        let mut world = CollisionWorld::default();
        world.add_quad([-8000., 0., -8000.], [-8000., 0., 8000.], [8000., 0., 8000.], [8000., 0., -8000.], &SurfaceOptions::default());
        world.finalize();
        let mut m = MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0., 0., -7000.], face_yaw: 0.0 }, -1600.0);
        let forward = ControllerSnapshot { stick_x: 0.0, stick_y: 64.0, stick_mag: 64.0, ..Default::default() };
        for _ in 0..60 {
            m.step(&forward, 0x8000 as f64, false, false);
        }
        let start = m.pos[2];
        let mut peak = 0.0f64;
        for i in 0..120 {
            let air = m.action & ACT_FLAG_AIR != 0;
            let mut c = forward.clone();
            c.button_down = A_BUTTON;
            c.button_pressed = if i == 0 { A_BUTTON } else { 0 };
            m.step(&c, 0x8000 as f64, false, false);
            peak = peak.max(m.pos[1]);
            if air && m.action & ACT_FLAG_AIR == 0 {
                break;
            }
        }
        (m.pos[2] - start, peak)
    }

    fn segments(seed: f64, count: usize) -> Vec<Segment> {
        let mut track = RunnerTrack::new(seed, 0.0, 1000.0);
        (0..count).map(|i| track.segment(i).clone()).collect()
    }

    #[test]
    fn limits_stay_well_inside_marios_measured_running_jump() {
        let (distance, height) = measure_running_jump();
        assert!(distance > RUNNING_JUMP_DISTANCE * 0.95, "distance {distance}");
        assert!(height > RUNNING_JUMP_HEIGHT * 0.95, "height {height}");
        assert!(GAP_LIMIT[1] < distance * 0.6);
        assert!(RISE_LIMIT[1] < height * 0.55);
        assert!(HURDLE_LIMIT < height * 0.4);
    }

    #[test]
    fn is_continuous_and_deterministic_per_seed() {
        let (a, b, c) = (segments(11.0, 30), segments(11.0, 30), segments(12.0, 30));
        let key = |v: &[Segment]| v.iter().map(|s| (s.kind, s.start.to_bits(), s.end.to_bits())).collect::<Vec<_>>();
        assert_eq!(key(&a), key(&b));
        assert_ne!(a.iter().map(|s| s.kind).collect::<Vec<_>>(), c.iter().map(|s| s.kind).collect::<Vec<_>>());
        for i in 1..a.len() {
            assert_eq!(a[i].start, a[i - 1].end);
        }
    }

    #[test]
    fn never_asks_for_a_gap_or_step_mario_cannot_make() {
        for seed in 1..=40 {
            for hop in floor_hops(&segments(seed as f64, 40)) {
                assert!(hop.gap <= max_gap(hop.difficulty) + 1e-6, "seed {seed}: gap {}", hop.gap);
                assert!(hop.rise <= max_rise(hop.difficulty) + 1e-6, "seed {seed}: rise {}", hop.rise);
            }
        }
    }

    #[test]
    fn gets_harder_and_changes_theme_the_further_you_run() {
        let run = segments(5.0, 40);
        assert!(run[35].difficulty > run[3].difficulty);
        let themes: std::collections::HashSet<_> = run.iter().map(|s| s.theme).collect();
        assert_eq!(themes.len(), THEMES.len());
        let kinds: std::collections::HashSet<_> = run.iter().map(|s| s.kind).collect();
        assert!(kinds.len() >= 6, "{kinds:?}");
    }
}
