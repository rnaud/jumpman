//! Engine-free projectile physics for Link's items. Every constant is the decomp's value
//! converted with [`combat::speed`] / [`combat::ticks`] (OoT 20 Hz frames -> 30 Hz ticks, x4.4
//! model units):
//!
//! * arrows: `z_en_arrow.c` `EnArrow_Shoot` (speed 150, 12 frames, gravity -0.4 once under 7.2
//!   frames left); seeds / nuts speed 80, 15 frames;
//! * boomerang: `z_en_boom.c` (speed 12, `returnTimer` 20 from `z_player.c`, catch within 40);
//! * hookshot / longshot: `z_arms_hook.c` (speed 20, 13 / 26 frames);
//! * bombs: `z_en_bom.c` (70-frame fuse, gravity -4, explosion growing for 10 frames);
//! * bombchu: `z_en_bom_chu.c` (speed 8, 120 frames);
//! * Din's Fire: `z_magic_fire.c` (20 frame wind-up, expand 30 frames to scale 0.4 = radius 130,
//!   hold 25, quick expand 15).
use super::combat::{accel, len, speed, ticks, Dmg};
use crate::collision::CollisionWorld;
use crate::math::Vec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Arrow,
    FireArrow,
    IceArrow,
    LightArrow,
    Seed,
    Nut,
    Boomerang,
    Hookshot,
    Longshot,
    Bomb,
    Bombchu,
    DinsFire,
}

impl Kind {
    pub fn damage(self) -> Dmg {
        match self {
            Kind::Arrow => Dmg::Arrow,
            Kind::FireArrow => Dmg::FireArrow,
            Kind::IceArrow => Dmg::IceArrow,
            Kind::LightArrow => Dmg::LightArrow,
            Kind::Seed => Dmg::Slingshot,
            Kind::Nut => Dmg::Nut,
            Kind::Boomerang => Dmg::Boomerang,
            Kind::Hookshot | Kind::Longshot => Dmg::Hookshot,
            Kind::Bomb | Kind::Bombchu => Dmg::Explosive,
            Kind::DinsFire => Dmg::FireMagic,
        }
    }
    pub fn is_arrow(self) -> bool {
        matches!(self, Kind::Arrow | Kind::FireArrow | Kind::IceArrow | Kind::LightArrow)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TargetKind {
    /// Takes weapon damage.
    Enemy,
    /// A crate, pot or bomb wall: broken by weapons, explosions.
    Breakable,
    /// A coin, rupee or heart the boomerang / hookshot can fetch.
    Pickup,
    /// A hookshot target (`ACTOR_FLAG_HOOKSHOT_ATTACHED` actors: the dungeon eye / ring targets).
    Hook,
    /// A crystal / eye switch: seeds, arrows and the boomerang trigger it.
    Switch,
}

/// Something projectiles can hit. A vertical cylinder from `pos` (feet) up by `height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: u32,
    pub pos: Vec3,
    pub radius: f64,
    pub height: f64,
    pub kind: TargetKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Fly,
    /// Boomerang / hook flying back to Link.
    Return,
    /// An arrow stuck in a wall, fading.
    Stuck,
    /// The hookshot latched on: Link is pulled to `pos`.
    Pulling,
    /// A bomb or chu counting down.
    Fuse,
    /// The explosion / spell volume is live.
    Burst,
    Done,
}

#[derive(Clone, Debug)]
pub struct Projectile {
    pub kind: Kind,
    pub pos: Vec3,
    pub prev: Vec3,
    /// Velocity per 30 Hz tick.
    pub vel: Vec3,
    pub yaw: f64,
    pub phase: Phase,
    /// Ticks left in the current phase.
    pub timer: f64,
    pub age: f64,
    /// Pickup carried back by the boomerang.
    pub carried: Option<u32>,
    /// Growing volume for `Burst`.
    pub radius: f64,
    /// Targets this projectile already hurt (spells and explosions hit each once).
    pub hit: Vec<u32>,
    /// Boomerang steering: yaw change (s16 units) applied per tick while flying out.
    pub steer: f64,
    /// Whether the hookshot may latch on any wall (see [`ProjectileSet::hook_any_wall`]).
    pub latched: bool,
}

/// What happened during a step.
#[derive(Clone, Debug, PartialEq)]
pub enum Impact {
    Hit { kind: Kind, dmg: Dmg, target: u32, tkind: TargetKind, pos: Vec3 },
    Terrain { kind: Kind, pos: Vec3, normal: Vec3 },
    /// A bomb / chu detonated. `hurts_link` is the player-hurting 2 hearts of `EnBom`.
    Explosion { pos: Vec3, radius: f64 },
    /// A Deku nut went off: stuns everything within `radius`.
    Flash { pos: Vec3, radius: f64 },
    /// The boomerang picked a pickup up / delivered it to Link.
    Grab { target: u32 },
    Delivered { target: u32 },
    /// The boomerang came home.
    Caught,
    /// The hookshot latched onto `pos`.
    HookAttach { pos: Vec3 },
    /// Din's Fire swept target `target` (once per cast).
    Scorch { target: u32, tkind: TargetKind },
    /// A fire arrow / Din's Fire touched the world at `pos` (lights torches, melts ice).
    Ignite { pos: Vec3, radius: f64 },
    /// An ice arrow touched the world.
    Freeze { pos: Vec3, radius: f64 },
}

const STEP: f64 = 22.0;

pub fn explosion_radius() -> f64 {
    len(70.0)
}

/// Hit test of a moving point of radius `r` against `t`.
fn touches(p: &Vec3, r: f64, t: &Target) -> bool {
    let dx = p[0] - t.pos[0];
    let dz = p[2] - t.pos[2];
    let rr = t.radius + r;
    dx * dx + dz * dz < rr * rr && p[1] > t.pos[1] - r && p[1] < t.pos[1] + t.height + r
}

/// First scenery contact on the segment `a -> b`: (point, surface normal).
pub fn terrain_hit(world: &CollisionWorld, a: &Vec3, b: &Vec3) -> Option<(Vec3, Vec3)> {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let n = ((length / STEP).ceil() as usize).max(1);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let mut p = [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t];
        let floor = world.find_floor(p[0], p[1] + 30.0, p[2]);
        if let Some(s) = &floor.surface {
            if p[1] <= floor.height {
                p[1] = floor.height;
                return Some((p, s.normal));
            }
        }
        let mut q = p;
        if let Some(w) = world.resolve_and_return_wall_collisions(&mut q, 0.0, 14.0) {
            return Some((p, w.normal));
        }
    }
    None
}

#[derive(Default, Debug, Clone)]
pub struct ProjectileSet {
    pub list: Vec<Projectile>,
    /// SM64 courses carry no OoT surface materials: let the hookshot latch onto any wall.
    pub hook_any_wall: bool,
}

impl ProjectileSet {
    pub fn new() -> Self {
        Self { list: vec![], hook_any_wall: true }
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    pub fn count(&self, k: Kind) -> usize {
        self.list.iter().filter(|p| p.kind == k && p.phase != Phase::Done).count()
    }

    pub fn active(&self) -> bool {
        self.list.iter().any(|p| p.phase != Phase::Done)
    }

    fn base(kind: Kind, from: Vec3, vel: Vec3, yaw: f64, phase: Phase, timer: f64) -> Projectile {
        Projectile { kind, pos: from, prev: from, vel, yaw, phase, timer, age: 0.0, carried: None, radius: 0.0, hit: vec![], steer: 0.0, latched: false }
    }

    /// Launch an arrow / seed / nut along (`yaw` s16, `pitch` radians).
    pub fn shoot(&mut self, kind: Kind, from: Vec3, yaw: f64, pitch: f64) {
        let (v, frames) = match kind {
            Kind::Seed | Kind::Nut => (speed(80.0), 15.0),
            _ => (speed(150.0), 12.0),
        };
        let a = yaw * std::f64::consts::PI / 32768.0;
        let vel = [a.sin() * pitch.cos() * v, pitch.sin() * v, a.cos() * pitch.cos() * v];
        self.list.push(Self::base(kind, from, vel, yaw, Phase::Fly, ticks(frames)));
    }

    pub fn throw_boomerang(&mut self, from: Vec3, yaw: f64) {
        let a = yaw * std::f64::consts::PI / 32768.0;
        let v = speed(12.0);
        self.list.push(Self::base(Kind::Boomerang, from, [a.sin() * v, 0.0, a.cos() * v], yaw, Phase::Fly, ticks(20.0)));
    }

    pub fn shoot_hook(&mut self, long: bool, from: Vec3, yaw: f64, pitch: f64) {
        let a = yaw * std::f64::consts::PI / 32768.0;
        let v = speed(20.0);
        let vel = [a.sin() * pitch.cos() * v, pitch.sin() * v, a.cos() * pitch.cos() * v];
        let kind = if long { Kind::Longshot } else { Kind::Hookshot };
        self.list.push(Self::base(kind, from, vel, yaw, Phase::Fly, ticks(if long { 26.0 } else { 13.0 })));
    }

    /// A bomb dropped / thrown with initial `vel`.
    pub fn drop_bomb(&mut self, from: Vec3, vel: Vec3) {
        self.list.push(Self::base(Kind::Bomb, from, vel, 0.0, Phase::Fuse, ticks(70.0)));
    }

    pub fn launch_bombchu(&mut self, from: Vec3, yaw: f64) {
        let a = yaw * std::f64::consts::PI / 32768.0;
        let v = speed(8.0);
        self.list.push(Self::base(Kind::Bombchu, from, [a.sin() * v, 0.0, a.cos() * v], yaw, Phase::Fuse, ticks(120.0)));
    }

    /// Din's Fire: a sphere that follows Link (set `pos` each tick through [`ProjectileSet::follow`]).
    pub fn cast_dins(&mut self, at: Vec3) {
        let mut p = Self::base(Kind::DinsFire, at, [0.0; 3], 0.0, Phase::Burst, ticks(20.0 + 30.0 + 25.0 + 15.0));
        p.radius = 0.0;
        self.list.push(p);
    }

    /// Re-anchor Din's Fire to Link.
    pub fn follow(&mut self, at: Vec3) {
        for p in self.list.iter_mut().filter(|p| p.kind == Kind::DinsFire && p.phase == Phase::Burst) {
            p.pos = at;
        }
    }

    /// Detonate every bomb / chu now (`this->timer = 0`).
    pub fn detonate_all_within(&mut self, center: &Vec3, radius: f64) {
        for p in self.list.iter_mut().filter(|p| matches!(p.kind, Kind::Bomb | Kind::Bombchu) && p.phase == Phase::Fuse) {
            let d = [p.pos[0] - center[0], p.pos[1] - center[1], p.pos[2] - center[2]];
            if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= radius {
                p.timer = 0.0;
            }
        }
    }

    /// Advance everything one tick. `link` is Link's chest position (boomerang catch point).
    pub fn step(&mut self, world: &CollisionWorld, link: &Vec3, targets: &[Target]) -> Vec<Impact> {
        let mut out = vec![];
        let hook_any_wall = self.hook_any_wall;
        let mut chain: Vec<(Vec3, f64)> = vec![];
        for p in self.list.iter_mut() {
            p.age += 1.0;
            p.prev = p.pos;
            match p.kind {
                Kind::Arrow | Kind::FireArrow | Kind::IceArrow | Kind::LightArrow | Kind::Seed | Kind::Nut => step_shot(p, world, targets, &mut out),
                Kind::Boomerang => step_boomerang(p, world, link, targets, &mut out),
                Kind::Hookshot | Kind::Longshot => step_hook(p, world, link, targets, hook_any_wall, &mut out),
                Kind::Bomb => step_bomb(p, world, &mut out, &mut chain),
                Kind::Bombchu => step_chu(p, world, targets, &mut out, &mut chain),
                Kind::DinsFire => step_dins(p, targets, &mut out),
            }
        }
        // Chain reactions: an explosion sets nearby fuses to 0 and they pop on the next tick.
        for (pos, radius) in chain {
            self.detonate_all_within(&pos, radius);
        }
        self.list.retain(|p| p.phase != Phase::Done);
        out
    }
}

fn step_shot(p: &mut Projectile, world: &CollisionWorld, targets: &[Target], out: &mut Vec<Impact>) {
    match p.phase {
        Phase::Stuck => {
            p.timer -= 1.0;
            if p.timer <= 0.0 {
                p.phase = Phase::Done;
            }
            return;
        }
        Phase::Fly => {}
        _ => return,
    }
    p.timer -= 1.0;
    if p.timer <= 0.0 {
        p.phase = Phase::Done;
        return;
    }
    // `EnArrow_Fly`: gravity -0.4 only in the last 7.2 frames.
    if p.timer < ticks(7.2) {
        p.vel[1] -= accel(0.4);
    }
    let next = [p.pos[0] + p.vel[0], p.pos[1] + p.vel[1], p.pos[2] + p.vel[2]];
    let reach = if p.kind == Kind::Seed { 10.0 } else { 14.0 };
    // Walk the segment: first scenery or target contact wins.
    let d = [next[0] - p.pos[0], next[1] - p.pos[1], next[2] - p.pos[2]];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
    let n = ((length / STEP).ceil() as usize).max(1);
    let terrain = terrain_hit(world, &p.pos, &next);
    let terrain_t = terrain.as_ref().map(|(hp, _)| ((hp[0] - p.pos[0]).powi(2) + (hp[1] - p.pos[1]).powi(2) + (hp[2] - p.pos[2]).powi(2)).sqrt() / length);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        if terrain_t.is_some_and(|tt| tt <= t) {
            break;
        }
        let q = [p.pos[0] + d[0] * t, p.pos[1] + d[1] * t, p.pos[2] + d[2] * t];
        if let Some(target) = targets.iter().filter(|tg| tg.kind != TargetKind::Pickup && tg.kind != TargetKind::Hook).find(|tg| touches(&q, reach, tg)) {
            out.push(Impact::Hit { kind: p.kind, dmg: p.kind.damage(), target: target.id, tkind: target.kind, pos: q });
            match p.kind {
                Kind::Nut => out.push(Impact::Flash { pos: q, radius: len(50.0) }),
                Kind::FireArrow => out.push(Impact::Ignite { pos: q, radius: len(20.0) }),
                Kind::IceArrow => out.push(Impact::Freeze { pos: q, radius: len(20.0) }),
                _ => {}
            }
            p.pos = q;
            p.phase = Phase::Done;
            return;
        }
    }
    if let Some((hp, normal)) = terrain {
        p.pos = hp;
        out.push(Impact::Terrain { kind: p.kind, pos: hp, normal });
        match p.kind {
            Kind::Nut => out.push(Impact::Flash { pos: hp, radius: len(50.0) }),
            Kind::FireArrow => out.push(Impact::Ignite { pos: hp, radius: len(20.0) }),
            Kind::IceArrow => out.push(Impact::Freeze { pos: hp, radius: len(20.0) }),
            _ => {}
        }
        if p.kind.is_arrow() {
            // `func_809B45E0`: the arrow sticks in the wall and lingers for a moment.
            p.phase = Phase::Stuck;
            p.timer = ticks(30.0);
            p.vel = [0.0; 3];
        } else {
            p.phase = Phase::Done;
        }
        return;
    }
    p.pos = next;
}

fn step_boomerang(p: &mut Projectile, world: &CollisionWorld, link: &Vec3, targets: &[Target], out: &mut Vec<Impact>) {
    let v = speed(12.0);
    if p.phase == Phase::Fly {
        // Steering (Z-target homing in the original: `Math_ScaledStepToS` toward the target).
        p.yaw += p.steer;
        let a = p.yaw * std::f64::consts::PI / 32768.0;
        p.vel = [a.sin() * v, 0.0, a.cos() * v];
        p.timer -= 1.0;
        if p.timer <= 0.0 {
            p.phase = Phase::Return;
        }
    }
    if p.phase == Phase::Return {
        let to = [link[0] - p.pos[0], link[1] - p.pos[1], link[2] - p.pos[2]];
        let dist = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
        // `distFromLink < 40.0` catches it.
        if dist < len(40.0) {
            if let Some(id) = p.carried.take() {
                out.push(Impact::Delivered { target: id });
            }
            out.push(Impact::Caught);
            p.phase = Phase::Done;
            return;
        }
        let s = v.min(dist);
        p.vel = [to[0] / dist * s, to[1] / dist * s, to[2] / dist * s];
    }
    let next = [p.pos[0] + p.vel[0], p.pos[1] + p.vel[1], p.pos[2] + p.vel[2]];
    if p.phase == Phase::Fly {
        if let Some((hp, normal)) = terrain_hit(world, &p.pos, &next) {
            out.push(Impact::Terrain { kind: Kind::Boomerang, pos: hp, normal });
            p.phase = Phase::Return;
            return;
        }
    }
    let r = 20.0;
    let d = [next[0] - p.pos[0], next[1] - p.pos[1], next[2] - p.pos[2]];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let n = ((length / STEP).ceil() as usize).max(1);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let q = [p.pos[0] + d[0] * t, p.pos[1] + d[1] * t, p.pos[2] + d[2] * t];
        for tg in targets {
            if p.hit.contains(&tg.id) || !touches(&q, r, tg) {
                continue;
            }
            match tg.kind {
                TargetKind::Pickup => {
                    if p.carried.is_none() {
                        p.carried = Some(tg.id);
                        out.push(Impact::Grab { target: tg.id });
                    }
                }
                TargetKind::Hook => {}
                _ => {
                    p.hit.push(tg.id);
                    out.push(Impact::Hit { kind: Kind::Boomerang, dmg: Dmg::Boomerang, target: tg.id, tkind: tg.kind, pos: q });
                    // A hit bounces it back (`collided` -> `returnTimer = 0`).
                    p.phase = Phase::Return;
                }
            }
        }
    }
    p.pos = next;
}

fn step_hook(p: &mut Projectile, world: &CollisionWorld, link: &Vec3, targets: &[Target], any_wall: bool, out: &mut Vec<Impact>) {
    let v = speed(20.0);
    match p.phase {
        Phase::Fly => {
            p.timer -= 1.0;
            if p.timer <= 0.0 {
                p.phase = Phase::Return;
                return;
            }
            let next = [p.pos[0] + p.vel[0], p.pos[1] + p.vel[1], p.pos[2] + p.vel[2]];
            let d = [next[0] - p.pos[0], next[1] - p.pos[1], next[2] - p.pos[2]];
            let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
            let terrain = terrain_hit(world, &p.pos, &next);
            let terrain_t = terrain.as_ref().map(|(hp, _)| ((hp[0] - p.pos[0]).powi(2) + (hp[1] - p.pos[1]).powi(2) + (hp[2] - p.pos[2]).powi(2)).sqrt() / length);
            let n = ((length / STEP).ceil() as usize).max(1);
            for i in 1..=n {
                let t = i as f64 / n as f64;
                if terrain_t.is_some_and(|tt| tt <= t) {
                    break;
                }
                let q = [p.pos[0] + d[0] * t, p.pos[1] + d[1] * t, p.pos[2] + d[2] * t];
                for tg in targets {
                    if !touches(&q, 16.0, tg) {
                        continue;
                    }
                    match tg.kind {
                        TargetKind::Hook => {
                            p.pos = q;
                            p.phase = Phase::Pulling;
                            out.push(Impact::HookAttach { pos: q });
                            return;
                        }
                        TargetKind::Pickup => {
                            p.carried = Some(tg.id);
                            out.push(Impact::Grab { target: tg.id });
                            p.phase = Phase::Return;
                            p.pos = q;
                            return;
                        }
                        _ => {
                            out.push(Impact::Hit { kind: p.kind, dmg: Dmg::Hookshot, target: tg.id, tkind: tg.kind, pos: q });
                            p.phase = Phase::Return;
                            p.pos = q;
                            return;
                        }
                    }
                }
            }
            if let Some((hp, normal)) = terrain {
                out.push(Impact::Terrain { kind: p.kind, pos: hp, normal });
                // Walls and ceilings hold the hook (wood in the original); floors do not.
                if any_wall && normal[1].abs() < 0.7 {
                    p.pos = hp;
                    p.phase = Phase::Pulling;
                    out.push(Impact::HookAttach { pos: hp });
                } else {
                    p.pos = hp;
                    p.phase = Phase::Return;
                }
                return;
            }
            p.pos = next;
        }
        Phase::Return => {
            let to = [link[0] - p.pos[0], link[1] - p.pos[1], link[2] - p.pos[2]];
            let dist = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
            if dist < v * 1.5 {
                if let Some(id) = p.carried.take() {
                    out.push(Impact::Delivered { target: id });
                }
                p.phase = Phase::Done;
                return;
            }
            p.pos = [p.pos[0] + to[0] / dist * v, p.pos[1] + to[1] / dist * v, p.pos[2] + to[2] / dist * v];
        }
        Phase::Pulling => {
            // Link flies to `pos` (moves.rs). A hook that nobody pulls on expires.
            p.timer -= 1.0;
            if p.timer < -ticks(60.0) {
                p.phase = Phase::Done;
            }
        }
        _ => {}
    }
}

fn step_bomb(p: &mut Projectile, world: &CollisionWorld, out: &mut Vec<Impact>, chain: &mut Vec<(Vec3, f64)>) {
    if p.phase == Phase::Burst {
        p.timer -= 1.0;
        p.radius = (p.radius + len(8.0) / 1.5).min(explosion_radius());
        if p.timer <= 0.0 {
            p.phase = Phase::Done;
        }
        return;
    }
    p.timer -= 1.0;
    // Gravity -4 per frame^2, bounce at 0.5 with floor friction.
    p.vel[1] = (p.vel[1] - accel(4.0)).max(-90.0);
    let next = [p.pos[0] + p.vel[0], p.pos[1] + p.vel[1], p.pos[2] + p.vel[2]];
    let floor = world.find_floor(next[0], next[1] + 40.0, next[2]);
    let mut pos = next;
    if floor.surface.is_some() && pos[1] <= floor.height {
        pos[1] = floor.height;
        if p.vel[1] < -12.0 {
            p.vel[1] = -p.vel[1] * 0.5;
        } else {
            p.vel[1] = 0.0;
        }
        p.vel[0] *= 0.82;
        p.vel[2] *= 0.82;
    }
    let mut q = pos;
    if let Some(w) = world.resolve_and_return_wall_collisions(&mut q, 20.0, 18.0) {
        let dot = p.vel[0] * w.normal[0] + p.vel[2] * w.normal[2];
        p.vel[0] = (p.vel[0] - 1.6 * dot * w.normal[0]) * 0.5;
        p.vel[2] = (p.vel[2] - 1.6 * dot * w.normal[2]) * 0.5;
        pos = q;
    }
    p.pos = pos;
    if p.timer <= 0.0 {
        explode(p, out, chain);
    }
}

fn explode(p: &mut Projectile, out: &mut Vec<Impact>, chain: &mut Vec<(Vec3, f64)>) {
    p.phase = Phase::Burst;
    p.timer = ticks(10.0);
    p.radius = 0.0;
    out.push(Impact::Explosion { pos: p.pos, radius: explosion_radius() });
    chain.push((p.pos, explosion_radius()));
}

fn step_chu(p: &mut Projectile, world: &CollisionWorld, targets: &[Target], out: &mut Vec<Impact>, chain: &mut Vec<(Vec3, f64)>) {
    if p.phase == Phase::Burst {
        p.timer -= 1.0;
        p.radius = (p.radius + len(8.0) / 1.5).min(explosion_radius());
        if p.timer <= 0.0 {
            p.phase = Phase::Done;
        }
        return;
    }
    p.timer -= 1.0;
    let next = [p.pos[0] + p.vel[0], p.pos[1], p.pos[2] + p.vel[2]];
    let mut q = next;
    let blocked = world.resolve_and_return_wall_collisions(&mut q, 20.0, 16.0).is_some();
    let floor = world.find_floor(next[0], p.pos[1] + 60.0, next[2]);
    let touches_enemy = targets.iter().any(|t| matches!(t.kind, TargetKind::Enemy) && touches(&next, 18.0, t));
    if floor.surface.is_some() && floor.height > p.pos[1] - 120.0 {
        p.pos = [next[0], floor.height, next[2]];
    } else {
        // Ran off a ledge: fall.
        p.pos = [next[0], p.pos[1] - 40.0, next[2]];
    }
    if blocked || touches_enemy || p.timer <= 0.0 {
        explode(p, out, chain);
    }
}

fn step_dins(p: &mut Projectile, targets: &[Target], out: &mut Vec<Impact>) {
    let elapsed = ticks(20.0 + 30.0 + 25.0 + 15.0) - p.timer;
    p.timer -= 1.0;
    if p.timer <= 0.0 {
        p.phase = Phase::Done;
        return;
    }
    let wind = ticks(20.0);
    let expand = ticks(30.0);
    let hold = ticks(25.0);
    // Radius 0 -> 130 over the expansion (scale 0.4 * 325), hold, then quick growth to 200.
    p.radius = if elapsed < wind {
        0.0
    } else if elapsed < wind + expand {
        len(130.0) * (elapsed - wind) / expand
    } else if elapsed < wind + expand + hold {
        len(130.0)
    } else {
        let t = (elapsed - wind - expand - hold) / ticks(15.0);
        len(130.0) + len(70.0) * t
    };
    if p.radius <= 0.0 {
        return;
    }
    out.push(Impact::Ignite { pos: p.pos, radius: p.radius });
    for tg in targets {
        if p.hit.contains(&tg.id) || tg.kind == TargetKind::Hook {
            continue;
        }
        let dx = p.pos[0] - tg.pos[0];
        let dz = p.pos[2] - tg.pos[2];
        let rr = p.radius + tg.radius;
        let dy_ok = tg.pos[1] < p.pos[1] + p.radius && tg.pos[1] + tg.height > p.pos[1] - p.radius;
        if dx * dx + dz * dz < rr * rr && dy_ok {
            p.hit.push(tg.id);
            out.push(Impact::Scorch { target: tg.id, tkind: tg.kind });
        }
    }
}

/// Everything inside `radius` of `pos` (an explosion), by target.
pub fn in_blast<'a>(targets: &'a [Target], pos: &Vec3, radius: f64) -> impl Iterator<Item = &'a Target> {
    let pos = *pos;
    targets.iter().filter(move |t| {
        let dx = pos[0] - t.pos[0];
        let dz = pos[2] - t.pos[2];
        let rr = radius + t.radius;
        dx * dx + dz * dz < rr * rr && t.pos[1] < pos[1] + radius && t.pos[1] + t.height > pos[1] - radius * 0.6
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::SurfaceOptions;

    fn flat() -> CollisionWorld {
        let mut w = CollisionWorld::default();
        w.add_triangle([-6000.0, 0.0, -6000.0], [6000.0, 0.0, 6000.0], [6000.0, 0.0, -6000.0], &SurfaceOptions::default());
        w.add_triangle([-6000.0, 0.0, -6000.0], [-6000.0, 0.0, 6000.0], [6000.0, 0.0, 6000.0], &SurfaceOptions::default());
        w
    }

    fn enemy(id: u32, pos: Vec3) -> Target {
        Target { id, pos, radius: 40.0, height: 100.0, kind: TargetKind::Enemy }
    }

    fn run(set: &mut ProjectileSet, w: &CollisionWorld, link: Vec3, targets: &[Target], n: usize) -> Vec<Impact> {
        let mut all = vec![];
        for _ in 0..n {
            all.extend(set.step(w, &link, targets));
        }
        all
    }

    #[test]
    fn arrow_flies_at_decomp_speed_and_hits_an_enemy() {
        let w = flat();
        let mut s = ProjectileSet::new();
        // Facing +Z (yaw 0), level with the enemy's chest.
        s.shoot(Kind::Arrow, [0.0, 60.0, 0.0], 0.0, 0.0);
        let before = s.list[0].vel;
        assert!((before[2] - 440.0).abs() < 1e-6, "{before:?}");
        let imp = run(&mut s, &w, [0.0; 3], &[enemy(7, [0.0, 0.0, 1500.0])], 6);
        let hit = imp.iter().find_map(|i| match i { Impact::Hit { dmg, target, .. } => Some((*dmg, *target)), _ => None });
        assert_eq!(hit, Some((Dmg::Arrow, 7)));
        assert!(s.list.is_empty(), "the arrow is consumed on the hit");
    }

    #[test]
    fn fire_arrow_ignites_and_ice_arrow_freezes() {
        let w = flat();
        for (kind, ice) in [(Kind::FireArrow, false), (Kind::IceArrow, true)] {
            let mut s = ProjectileSet::new();
            s.shoot(kind, [0.0, 60.0, 0.0], 0.0, 0.0);
            let imp = run(&mut s, &w, [0.0; 3], &[enemy(1, [0.0, 0.0, 900.0])], 5);
            assert!(imp.iter().any(|i| matches!(i, Impact::Hit { dmg, .. } if dmg.is_fire() != ice)));
            assert!(imp.iter().any(|i| if ice { matches!(i, Impact::Freeze { .. }) } else { matches!(i, Impact::Ignite { .. }) }));
        }
    }

    #[test]
    fn arrow_range_is_150_units_over_12_frames_with_a_late_sag() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.shoot(Kind::Arrow, [0.0, 400.0, 0.0], 0.0, 0.0);
        let mut max_z: f64 = 0.0;
        let mut last_y = 400.0;
        for i in 0..40 {
            s.step(&w, &[0.0; 3], &[]);
            if let Some(p) = s.list.first() {
                max_z = max_z.max(p.pos[2]);
                if i > 14 {
                    assert!(p.pos[1] < last_y, "gravity bites in the last 7.2 frames");
                }
                last_y = p.pos[1];
            }
        }
        // 12 frames x 1.5 ticks x 440 = 7920 minus the final tick.
        assert!((max_z - 7480.0).abs() < 500.0, "{max_z}");
    }

    #[test]
    fn arrow_sticks_in_a_wall_then_fades() {
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        // A wall at z = 1000 facing -Z.
        w.add_triangle([-2000.0, -500.0, 1000.0], [2000.0, 3000.0, 1000.0], [-2000.0, 3000.0, 1000.0], &o);
        w.add_triangle([-2000.0, -500.0, 1000.0], [2000.0, -500.0, 1000.0], [2000.0, 3000.0, 1000.0], &o);
        let mut s = ProjectileSet::new();
        s.shoot(Kind::Arrow, [0.0, 200.0, 0.0], 0.0, 0.0);
        let imp = run(&mut s, &w, [0.0; 3], &[], 6);
        assert!(imp.iter().any(|i| matches!(i, Impact::Terrain { .. })) || s.list.iter().any(|p| p.phase == Phase::Stuck || p.pos[2] > 900.0));
    }

    #[test]
    fn seed_is_slower_and_shorter_than_an_arrow() {
        let mut s = ProjectileSet::new();
        s.shoot(Kind::Seed, [0.0, 60.0, 0.0], 0.0, 0.0);
        s.shoot(Kind::Arrow, [0.0, 60.0, 0.0], 0.0, 0.0);
        assert!(s.list[0].vel[2] < s.list[1].vel[2]);
        assert_eq!(s.list[0].timer, 22.5);
        assert_eq!(s.list[1].timer, 18.0);
    }

    #[test]
    fn nut_flashes_where_it_lands() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.shoot(Kind::Nut, [0.0, 60.0, 0.0], 0.0, -0.2);
        let imp = run(&mut s, &w, [0.0; 3], &[], 20);
        assert!(imp.iter().any(|i| matches!(i, Impact::Flash { .. })));
    }

    #[test]
    fn boomerang_goes_out_20_frames_and_comes_back_with_a_pickup() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.throw_boomerang([0.0, 80.0, 0.0], 0.0);
        let coin = Target { id: 5, pos: [0.0, 40.0, 500.0], radius: 20.0, height: 40.0, kind: TargetKind::Pickup };
        let imp = run(&mut s, &w, [0.0, 80.0, 0.0], &[coin], 80);
        assert!(imp.contains(&Impact::Grab { target: 5 }));
        assert!(imp.contains(&Impact::Delivered { target: 5 }));
        assert!(imp.contains(&Impact::Caught));
        assert!(s.list.is_empty());
    }

    #[test]
    fn boomerang_hurts_once_and_returns() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.throw_boomerang([0.0, 60.0, 0.0], 0.0);
        let imp = run(&mut s, &w, [0.0, 60.0, 0.0], &[enemy(2, [0.0, 0.0, 600.0])], 100);
        let hits = imp.iter().filter(|i| matches!(i, Impact::Hit { dmg: Dmg::Boomerang, .. })).count();
        assert_eq!(hits, 1);
        assert!(imp.contains(&Impact::Caught));
    }

    #[test]
    fn hookshot_range_doubles_for_the_longshot() {
        let mut s = ProjectileSet::new();
        s.shoot_hook(false, [0.0; 3], 0.0, 0.0);
        s.shoot_hook(true, [0.0; 3], 0.0, 0.0);
        assert_eq!(s.list[1].timer, s.list[0].timer * 2.0);
        let hook_reach = s.list[0].timer * s.list[0].vel[2];
        assert!((hook_reach - len(260.0)).abs() < 1.0);
    }

    #[test]
    fn hookshot_latches_onto_a_target_and_pulls() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.shoot_hook(false, [0.0, 100.0, 0.0], 0.0, 0.0);
        let anchor = Target { id: 9, pos: [0.0, 40.0, 700.0], radius: 30.0, height: 100.0, kind: TargetKind::Hook };
        let imp = run(&mut s, &w, [0.0, 100.0, 0.0], &[anchor], 15);
        assert!(imp.iter().any(|i| matches!(i, Impact::HookAttach { .. })));
        assert_eq!(s.list[0].phase, Phase::Pulling);
    }

    #[test]
    fn hookshot_out_of_range_retracts() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.shoot_hook(false, [0.0, 100.0, 0.0], 0.0, 0.0);
        let imp = run(&mut s, &w, [0.0, 100.0, 0.0], &[], 80);
        assert!(!imp.iter().any(|i| matches!(i, Impact::HookAttach { .. })));
        assert!(s.list.is_empty(), "the hook came back and vanished");
    }

    #[test]
    fn bomb_explodes_after_the_70_frame_fuse_and_chains() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.drop_bomb([0.0, 10.0, 0.0], [0.0; 3]);
        s.drop_bomb([100.0, 10.0, 0.0], [0.0; 3]);
        s.list[1].timer = 500.0;
        let mut pops = 0;
        let mut first_tick = 0;
        for i in 1..=140 {
            let imp = s.step(&w, &[0.0; 3], &[]);
            if imp.iter().any(|x| matches!(x, Impact::Explosion { .. })) && first_tick == 0 {
                first_tick = i;
            }
            pops += imp.iter().filter(|x| matches!(x, Impact::Explosion { .. })).count();
        }
        assert_eq!(first_tick, 105, "70 frames x 1.5");
        assert_eq!(pops, 2, "the second bomb was inside the blast and went off with it");
        assert!(s.list.is_empty());
    }

    #[test]
    fn bomb_falls_and_rests_on_the_floor() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.drop_bomb([0.0, 400.0, 0.0], [30.0, 0.0, 0.0]);
        for _ in 0..60 {
            s.step(&w, &[0.0; 3], &[]);
        }
        let p = &s.list[0];
        assert!(p.pos[1] >= 0.0 && p.pos[1] < 5.0, "{:?}", p.pos);
        assert!(p.pos[0] > 0.0 && p.vel[0].abs() < 5.0);
    }

    #[test]
    fn bombchu_runs_along_the_floor_and_blows_on_an_enemy() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.launch_bombchu([0.0, 0.0, 0.0], 0.0);
        let imp = run(&mut s, &w, [0.0; 3], &[enemy(3, [0.0, 0.0, 400.0])], 40);
        assert!(imp.iter().any(|i| matches!(i, Impact::Explosion { .. })));
    }

    #[test]
    fn dins_fire_scorches_each_target_once_inside_its_growing_sphere() {
        let w = flat();
        let mut s = ProjectileSet::new();
        s.cast_dins([0.0, 0.0, 0.0]);
        let imp = run(&mut s, &w, [0.0; 3], &[enemy(1, [300.0, 0.0, 0.0]), enemy(2, [3000.0, 0.0, 0.0])], 150);
        let scorched: Vec<u32> = imp.iter().filter_map(|i| if let Impact::Scorch { target, .. } = i { Some(*target) } else { None }).collect();
        assert_eq!(scorched, vec![1], "one hit on the near target, none on the far one");
        assert!(s.list.is_empty(), "20 + 30 + 25 + 15 frames, then it is gone");
    }

    #[test]
    fn explosion_blast_selects_targets_in_range() {
        let ts = [enemy(1, [100.0, 0.0, 0.0]), enemy(2, [2000.0, 0.0, 0.0])];
        let hit: Vec<u32> = in_blast(&ts, &[0.0, 0.0, 0.0], explosion_radius()).map(|t| t.id).collect();
        assert_eq!(hit, vec![1]);
    }
}
