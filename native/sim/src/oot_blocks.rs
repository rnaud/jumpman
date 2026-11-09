//! Pushable dungeon blocks (`Obj_Oshihiki`, z_obj_oshihiki.c). Engine-free: [`Blocks::step`] takes where Link stands and
//! which way he pushes, and moves blocks the way the actor does:
//!
//! * the block's size comes from `sScales[params & 7]` (1/10, 1/6, 1/5, 1/3); the large ones (index 3 / 7) need the Goron's
//!   bracelet (`ObjOshihiki_StrongEnough`);
//! * a push is a fixed 20-unit step along one axis of the block (`ObjOshihiki_Push`: `pushSpeed += 0.5` up to 2 per frame
//!   until `pushDist` reaches 20), started once Link has leaned on a face for [`PUSH_WAIT`] ticks;
//! * a step into a wall is refused; a step over an edge ends in a fall (`ObjOshihiki_Fall`, gravity -1) to the floor below.
use crate::math::Vec3;

pub const U: f64 = 4.4;
/// `pushDist` target of one push, OoT units.
pub const PUSH_STEP: f64 = 20.0;
/// Ticks Link has to lean on a block before it moves.
pub const PUSH_WAIT: u32 = 10;
/// `sScales`.
pub const SCALES: [f64; 4] = [0.1, 1.0 / 6.0, 0.2, 1.0 / 3.0];

#[derive(Clone, Debug)]
pub struct Block {
    pub home: Vec3,
    pub pos: Vec3,
    /// Half the side of the cube, world units (`gPushBlockCol` is a 2-unit cube times the scale x 100... see [`Block::new`]).
    pub half: f64,
    /// Needs the Goron's bracelet.
    pub heavy: bool,
    pub switch_flag: i32,
    hold: u32,
    push: Option<Push>,
    fall: f64,
}

#[derive(Clone, Copy, Debug)]
struct Push {
    dir: [f64; 2],
    dist: f64,
    speed: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockEvent {
    /// A push step started / finished on block `i`.
    Pushing(usize),
    Settled(usize),
    /// The step was refused by a wall.
    Bumped(usize),
    Fell(usize),
}

impl Block {
    /// `params & 0xF` picks the scale (the cube is 100 model units a side before the scale is applied, 100 x 0.1 x 4 =
    /// 40 OoT units for the smallest), `params >> 8 & 0x3F` is the switch flag.
    pub fn new(pos: Vec3, params: i32) -> Self {
        let scale = SCALES[(params & 3) as usize];
        let half = 0.5 * 400.0 * scale * U / 2.0;
        Self { home: pos, pos, half, heavy: params & 3 == 3, switch_flag: (params >> 8) & 0x3F, hold: 0, push: None, fall: 0.0 }
    }

    pub fn moving(&self) -> bool {
        self.push.is_some() || self.fall != 0.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Blocks {
    pub list: Vec<Block>,
}

impl Blocks {
    pub fn new(list: Vec<Block>) -> Self {
        Self { list }
    }

    /// One tick. `intended` is Link's stick `(magnitude, yaw)` (SM64 convention: forward = `(sin yaw, cos yaw)`), `strong` the
    /// bracelet, `wall(pos, half, i)` whether block `i` centred at `pos` would be inside a wall, `floor(pos)` the floor height
    /// below a position.
    pub fn step(&mut self, player: Vec3, intended: (f64, f64), strong: bool, wall: &dyn Fn(Vec3, f64, usize) -> bool, floor: &dyn Fn(Vec3) -> Option<f64>) -> Vec<BlockEvent> {
        let mut ev = vec![];
        let (mag, yaw) = intended;
        let angle = yaw * std::f64::consts::PI / 32768.0;
        let want = [angle.sin(), angle.cos()];
        for i in 0..self.list.len() {
            let b = &mut self.list[i];
            if let Some(p) = &mut b.push {
                p.speed = (p.speed + 0.5 * U).min(2.0 * U);
                p.dist = (p.dist + p.speed).min(PUSH_STEP * U);
                b.pos[0] = b.home[0] + p.dir[0] * p.dist;
                b.pos[2] = b.home[2] + p.dir[1] * p.dist;
                if p.dist >= PUSH_STEP * U {
                    b.home = b.pos;
                    b.push = None;
                    b.hold = 0;
                    match floor(b.pos) {
                        Some(h) if (b.pos[1] - h).abs() < 15.0 * U => ev.push(BlockEvent::Settled(i)),
                        Some(_) | None => {
                            b.fall = -0.01;
                            ev.push(BlockEvent::Fell(i));
                        }
                    }
                }
                continue;
            }
            if b.fall != 0.0 {
                b.fall -= 1.0 * U;
                b.pos[1] += b.fall;
                match floor(b.pos) {
                    Some(h) if b.pos[1] <= h => {
                        b.pos[1] = h;
                        b.home = b.pos;
                        b.fall = 0.0;
                        ev.push(BlockEvent::Settled(i));
                    }
                    None if b.pos[1] < b.home[1] - 4000.0 * U => b.fall = 0.0,
                    _ => {}
                }
                continue;
            }
            // Leaning on a face?
            let (rx, rz) = (player[0] - b.pos[0], player[2] - b.pos[2]);
            let axis_x = want[0].abs() > want[1].abs();
            let (along, across, sign) = if axis_x { (rx, rz, want[0].signum()) } else { (rz, rx, want[1].signum()) };
            let touching = mag > 0.0 && along.signum() == -sign && along.abs() < b.half + 40.0 * U && along.abs() > b.half - 10.0 * U && across.abs() < b.half && (player[1] - b.pos[1]).abs() < 60.0 * U;
            if !touching || (b.heavy && !strong) {
                b.hold = 0;
                continue;
            }
            b.hold += 1;
            if b.hold < PUSH_WAIT {
                continue;
            }
            let dir = if axis_x { [sign, 0.0] } else { [0.0, sign] };
            let dest = [b.pos[0] + dir[0] * PUSH_STEP * U, b.pos[1], b.pos[2] + dir[1] * PUSH_STEP * U];
            if wall(dest, b.half, i) {
                b.hold = 0;
                ev.push(BlockEvent::Bumped(i));
            } else {
                b.push = Some(Push { dir, dist: 0.0, speed: 0.0 });
                ev.push(BlockEvent::Pushing(i));
            }
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT: fn(Vec3) -> Option<f64> = |_| Some(0.0);
    const OPEN: fn(Vec3, f64, usize) -> bool = |_, _, _| false;

    /// Link south of the block pushing north (+z is "forward" at yaw 0).
    fn lean(b: &Block) -> Vec3 {
        [b.pos[0], 0.0, b.pos[2] - b.half - 20.0 * U]
    }

    fn run(blocks: &mut Blocks, player: Vec3, strong: bool, wall: &dyn Fn(Vec3, f64, usize) -> bool, floor: &dyn Fn(Vec3) -> Option<f64>, ticks: usize) -> Vec<BlockEvent> {
        (0..ticks).flat_map(|_| blocks.step(player, (32.0, 0.0), strong, wall, floor)).collect()
    }

    #[test]
    fn a_push_moves_the_block_exactly_one_20_unit_step() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let p = lean(&blocks.list[0]);
        let ev = run(&mut blocks, p, false, &OPEN, &FLAT, 80);
        assert!(ev.contains(&BlockEvent::Pushing(0)));
        assert_eq!(ev.iter().filter(|e| **e == BlockEvent::Settled(0)).count(), 1);
        let b = &blocks.list[0];
        assert!((b.home[2] - 20.0 * U).abs() < 1e-6 && b.home[0] == 0.0, "{:?}", b.home);
    }

    #[test]
    fn nothing_moves_until_link_has_leaned_on_it_for_a_moment() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let p = lean(&blocks.list[0]);
        let ev = run(&mut blocks, p, false, &OPEN, &FLAT, PUSH_WAIT as usize - 2);
        assert!(ev.is_empty() && !blocks.list[0].moving());
        // walking past it does not push
        let ev = run(&mut blocks, [3000.0, 0.0, 0.0], false, &OPEN, &FLAT, 40);
        assert!(ev.is_empty());
    }

    #[test]
    fn a_wall_refuses_the_step_and_an_edge_drops_the_block() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let p = lean(&blocks.list[0]);
        let ev = run(&mut blocks, p, false, &|_, _, _| true, &FLAT, 30);
        assert!(ev.contains(&BlockEvent::Bumped(0)) && blocks.list[0].home[2] == 0.0);
        // a floor 200 units lower beyond the first step
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let step = |pos: Vec3| if pos[2] > 1.0 { Some(-200.0 * U) } else { Some(0.0) };
        let ev = run(&mut blocks, p, false, &OPEN, &step, 120);
        assert!(ev.contains(&BlockEvent::Fell(0)));
        assert!(blocks.list[0].pos[1] <= -200.0 * U + 1e-6, "it landed on the lower floor: {:?}", blocks.list[0].pos);
    }

    #[test]
    fn the_large_block_needs_the_bracelet() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 3)]);
        assert!(blocks.list[0].heavy && Block::new([0.0; 3], 0).half < blocks.list[0].half);
        let p = lean(&blocks.list[0]);
        assert!(run(&mut blocks, p, false, &OPEN, &FLAT, 40).is_empty());
        assert!(run(&mut blocks, p, true, &OPEN, &FLAT, 40).contains(&BlockEvent::Pushing(0)));
    }
}
