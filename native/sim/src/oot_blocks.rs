//! Pushable dungeon blocks (`Obj_Oshihiki`, z_obj_oshihiki.c). Engine-free: [`Blocks::step`] takes where Link stands and
//! which way he pushes, and moves blocks the way the actor does:
//!
//! * the block's size comes from `sScales[params & 7]` (1/10, 1/6, 1/5, 1/3); the large ones (index 3 / 7) need the Goron's
//!   bracelet (`ObjOshihiki_StrongEnough`);
//! * a push or a pull is a fixed 20-unit step along one axis of the block (`ObjOshihiki_Push`: `pushSpeed += 0.5` up to 2 per
//!   frame until `pushDist` reaches 20), started when Link holds on to a face (A) and leans on the stick (`unk_150`), then a
//!   10-frame pause ([`COOLDOWN`] ticks) before the next one;
//! * a step into a wall is refused; a step over an edge ends in a fall (`ObjOshihiki_Fall`, gravity -1) to the floor below.
use crate::math::Vec3;

pub const U: f64 = 4.4;
/// `pushDist` target of one push, OoT units.
pub const PUSH_STEP: f64 = 20.0;
/// Ticks between two steps (`timer = 10` actor frames after a push, 20 Hz -> 30 Hz).
pub const COOLDOWN: u32 = 15;
/// How far from a face Link can take hold of it (OoT units outside it / inside it).
const GRAB_OUT: f64 = 40.0;
const GRAB_IN: f64 = 10.0;
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
    cooldown: u32,
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
        Self { home: pos, pos, half, heavy: params & 3 == 3, switch_flag: (params >> 8) & 0x3F, cooldown: 0, push: None, fall: 0.0 }
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

    /// The block face Link could take hold of: `(block, yaw)`, the yaw (s16 units, `(sin, cos)` = forward) he has to look to
    /// face it. He has to stand against a face (not a corner), look at it within `0x3000`, and be strong enough for it.
    pub fn grab_face(&self, player: Vec3, facing: f64, strong: bool) -> Option<(usize, f64)> {
        let mut best: Option<(usize, f64, f64)> = None;
        for (i, b) in self.list.iter().enumerate() {
            if (b.heavy && !strong) || (player[1] - b.pos[1]).abs() >= 60.0 * U {
                continue;
            }
            let (rx, rz) = (player[0] - b.pos[0], player[2] - b.pos[2]);
            let axis_x = rx.abs() > rz.abs();
            let (along, across) = if axis_x { (rx, rz) } else { (rz, rx) };
            if along.abs() >= b.half + GRAB_OUT * U || along.abs() <= b.half - GRAB_IN * U || across.abs() >= b.half {
                continue;
            }
            // Link looks toward the block: opposite to the side he stands on
            let (dx, dz) = if axis_x { (-rx.signum(), 0.0) } else { (0.0, -rz.signum()) };
            let yaw = dx.atan2(dz) * 32768.0 / std::f64::consts::PI;
            if (crate::math::to_s16(facing - yaw) as f64).abs() >= 0x3000 as f64 {
                continue;
            }
            let gap = along.abs() - b.half;
            if best.is_none_or(|(_, _, g)| gap < g) {
                best = Some((i, yaw, gap));
            }
        }
        best.map(|(i, y, _)| (i, y))
    }

    /// One tick. `grip` is Link's hold: the block and the unit step `dir` (x, z) the leaning stick asks for (toward the face
    /// pushes, away from it pulls). `strong` is the bracelet, `wall(pos, half, i)` whether block `i` centred at `pos` would be
    /// inside a wall, `floor(pos)` the floor height below a position.
    pub fn step(&mut self, grip: Option<Grip>, strong: bool, wall: &dyn Fn(Vec3, f64, usize) -> bool, floor: &dyn Fn(Vec3) -> Option<f64>) -> Vec<BlockEvent> {
        let mut ev = vec![];
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
                    b.cooldown = COOLDOWN;
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
            b.cooldown = b.cooldown.saturating_sub(1);
            let Some(g) = grip.filter(|g| g.block == i) else { continue };
            if b.cooldown > 0 || (b.heavy && !strong) {
                continue;
            }
            let dest = [b.pos[0] + g.dir[0] * PUSH_STEP * U, b.pos[1], b.pos[2] + g.dir[1] * PUSH_STEP * U];
            if wall(dest, b.half, i) {
                b.cooldown = COOLDOWN;
                ev.push(BlockEvent::Bumped(i));
            } else {
                b.push = Some(Push { dir: g.dir, dist: 0.0, speed: 0.0 });
                ev.push(BlockEvent::Pushing(i));
            }
        }
        ev
    }
}

/// Link's hold on block `block`: `dir` is the unit step (x, z) of the push or pull.
#[derive(Clone, Copy, Debug)]
pub struct Grip {
    pub block: usize,
    pub dir: [f64; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT: fn(Vec3) -> Option<f64> = |_| Some(0.0);
    const OPEN: fn(Vec3, f64, usize) -> bool = |_, _, _| false;

    /// Link south of the block (-z side), holding on to its face.
    fn stand(b: &Block) -> Vec3 {
        [b.pos[0], 0.0, b.pos[2] - b.half - 20.0 * U]
    }

    /// Push north (+z): the leaning stick asks for a step along +z.
    fn push(b: usize) -> Option<Grip> {
        Some(Grip { block: b, dir: [0.0, 1.0] })
    }

    fn pull(b: usize) -> Option<Grip> {
        Some(Grip { block: b, dir: [0.0, -1.0] })
    }

    fn run(blocks: &mut Blocks, grip: Option<Grip>, strong: bool, wall: &dyn Fn(Vec3, f64, usize) -> bool, floor: &dyn Fn(Vec3) -> Option<f64>, ticks: usize) -> Vec<BlockEvent> {
        (0..ticks).flat_map(|_| blocks.step(grip, strong, wall, floor)).collect()
    }

    #[test]
    fn a_push_moves_the_block_exactly_one_20_unit_step_and_a_pull_brings_it_back() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let ev = run(&mut blocks, push(0), false, &OPEN, &FLAT, 14);
        assert!(ev.contains(&BlockEvent::Pushing(0)));
        assert_eq!(ev.iter().filter(|e| **e == BlockEvent::Settled(0)).count(), 1, "one step however long the stick leans");
        let b = &blocks.list[0];
        assert!((b.home[2] - 20.0 * U).abs() < 1e-6 && b.home[0] == 0.0, "{:?}", b.home);
        // a pause of COOLDOWN ticks, then the next step
        let before = blocks.list[0].home[2];
        run(&mut blocks, push(0), false, &OPEN, &FLAT, COOLDOWN as usize - 2);
        assert_eq!(blocks.list[0].home[2], before, "the 10-frame pause");
        // pull it back twice
        run(&mut blocks, pull(0), false, &OPEN, &FLAT, 60);
        assert!(blocks.list[0].home[2] < before - 19.0 * U, "{:?}", blocks.list[0].home);
    }

    #[test]
    fn nothing_moves_without_a_hold_and_other_blocks_stay_put() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0), Block::new([1000.0, 0.0, 0.0], 0)]);
        let ev = run(&mut blocks, None, false, &OPEN, &FLAT, 40);
        assert!(ev.is_empty() && !blocks.list[0].moving());
        run(&mut blocks, push(0), false, &OPEN, &FLAT, 40);
        assert_eq!(blocks.list[1].home, [1000.0, 0.0, 0.0]);
    }

    #[test]
    fn a_wall_refuses_the_step_and_an_edge_drops_the_block() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let ev = run(&mut blocks, push(0), false, &|_, _, _| true, &FLAT, 30);
        assert!(ev.contains(&BlockEvent::Bumped(0)) && blocks.list[0].home[2] == 0.0);
        // a floor 200 units lower beyond the first step
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let step = |pos: Vec3| if pos[2] > 1.0 { Some(-200.0 * U) } else { Some(0.0) };
        let ev = run(&mut blocks, push(0), false, &OPEN, &step, 120);
        assert!(ev.contains(&BlockEvent::Fell(0)));
        assert!(blocks.list[0].pos[1] <= -200.0 * U + 1e-6, "it landed on the lower floor: {:?}", blocks.list[0].pos);
    }

    #[test]
    fn the_large_block_needs_the_bracelet() {
        let mut blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 3)]);
        assert!(blocks.list[0].heavy && Block::new([0.0; 3], 0).half < blocks.list[0].half);
        assert!(run(&mut blocks, push(0), false, &OPEN, &FLAT, 40).is_empty());
        assert!(run(&mut blocks, push(0), true, &OPEN, &FLAT, 40).contains(&BlockEvent::Pushing(0)));
        // and Link cannot even take hold of it without
        let blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 3)]);
        let p = stand(&blocks.list[0]);
        assert!(blocks.grab_face(p, 0.0, false).is_none());
        assert!(blocks.grab_face(p, 0.0, true).is_some());
    }

    #[test]
    fn link_takes_hold_of_the_face_he_stands_against_and_looks_at() {
        let blocks = Blocks::new(vec![Block::new([0.0, 0.0, 0.0], 0)]);
        let p = stand(&blocks.list[0]);
        // south of the block looking north (yaw 0): holds its -z face
        let (i, yaw) = blocks.grab_face(p, 0.0, false).unwrap();
        assert_eq!(i, 0);
        assert!(yaw.abs() < 1e-9);
        // looking away: no hold
        assert!(blocks.grab_face(p, 32768.0, false).is_none());
        // from the east side the face looks west (-x): yaw -16384
        let east = [blocks.list[0].half + 20.0 * U, 0.0, 0.0];
        let (_, yaw) = blocks.grab_face(east, -16384.0, false).unwrap();
        assert!((yaw + 16384.0).abs() < 1e-6, "{yaw}");
        // far away or on a different level: none
        assert!(blocks.grab_face([500.0 * U, 0.0, 0.0], 0.0, false).is_none());
        assert!(blocks.grab_face([p[0], 200.0 * U, p[2]], 0.0, false).is_none());
    }
}
