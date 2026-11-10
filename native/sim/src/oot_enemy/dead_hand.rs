//! The Dead Hand (`En_Dh`, z_en_dh.c) and its hands (`En_Dha`, z_en_dha.c).
//!
//! Both actors run at 20 Hz: [`step`] executes whole actor frames (two per three ticks), so the counters, the `SmoothStepTo*`
//! steps and the animation lengths are the decomp's own. Distances, speeds and heights are in OoT units and turned into world
//! units (`x4.4`) where they meet the rest of the sim.
//!
//! The pair talks through the Dead Hand's `params` (`EnDhParams`): a hand that grabs Link sets `ENDH_START_ATTACK_GRAB` (1), a
//! blast near the buried body sets `ENDH_START_ATTACK_BOMB` (5), every hand that dies counts it down from 0 to -4, and the body's
//! death is 10. The body comes out of the ground when `params >= 1` or `params <= -4`, and goes back to 0 whenever it burrows.
//!
//! The head sphere (`colliderJntSph`, radius 20 at limb 13 + (20, 10, 0)) is the only soft spot of the body and only while it
//! is out of the ground; where it is comes from the exported skeleton's forward kinematics over each clip, sampled below.
//! Approximations: the hands' arm angles (the IK that reaches for Link) are not simulated, only the point of the hand
//! (`handPos[0]`, 16 units per frame toward Link); the hand's five spheres are one cylinder at that point; the dirt wave, debris
//! and the fade-out are left to the view.
use super::*;
use crate::math::{atan2s, coss, sins, to_s16};

/// Actor frames per sim tick.
const FRAME_PER_TICK: f64 = 1.0 / T;

pub const ENDH_START_ATTACK_GRAB: i32 = 1;
pub const ENDH_START_ATTACK_BOMB: i32 = 5;
pub const ENDH_DEATH: i32 = 10;
const ENDH_HANDS_KILLED_4: i32 = -4;

/// Frame counts of the animations (`Animation_GetLastFrame`).
const IDLE: f64 = 40.0; // object_dh_Anim_005880
const WALK: f64 = 20.0; // 003A8C
const ATTACK: f64 = 40.0; // 004658
const BITE: f64 = 20.0; // 001A3C
const BURROW: f64 = 30.0; // 002148
const DAMAGE: f64 = 12.0; // 003D6C
const DEATH: f64 = 70.0; // 0032BC
const DEATH_LOOP: f64 = 21.0; // 00375C

/// Where the head sphere's centre is `(forward, up)` from the actor's origin, OoT units, every `step` frames of the clip.
const HEAD_ATTACK: (f64, [(f64, f64); 7]) = (6.0, [(27.4, 131.8), (35.6, 128.4), (57.2, 107.9), (66.4, 71.8), (55.8, 35.7), (37.8, 14.7), (33.2, 13.0)]);
const HEAD_BITE: (f64, [(f64, f64); 7]) = (3.0, [(40.1, 19.5), (5.0, 42.6), (41.2, 46.0), (70.2, 28.1), (61.1, 23.9), (48.5, 20.1), (41.0, 19.1)]);
const HEAD_DAMAGE: (f64, [(f64, f64); 7]) = (2.0, [(36.1, 17.0), (-6.7, 38.6), (28.1, 45.8), (17.9, 36.1), (27.6, 32.3), (40.6, 24.7), (36.1, 17.0)]);
const HEAD_BURROW: (f64, [(f64, f64); 6]) = (5.0, [(40.7, 19.4), (41.2, 67.0), (25.0, 9.2), (10.1, -8.0), (9.7, -40.8), (6.9, -77.3)]);
const HEAD_DEATH: (f64, [(f64, f64); 7]) = (11.0, [(36.1, 17.1), (-28.8, 130.3), (-7.8, 135.9), (-8.5, 123.2), (13.9, 105.0), (113.5, 18.1), (113.0, 3.1)]);
const HEAD_WALK: (f64, f64) = (27.7, 130.5);
const HEAD_IDLE: (f64, f64) = (38.0, 19.0);

fn sample(table: &[(f64, f64)], step: f64, frame: f64) -> (f64, f64) {
    let x = (frame / step).max(0.0);
    let i = (x.floor() as usize).min(table.len() - 1);
    let j = (i + 1).min(table.len() - 1);
    let f = (x - i as f64).clamp(0.0, 1.0);
    (table[i].0 + (table[j].0 - table[i].0) * f, table[i].1 + (table[j].1 - table[i].1) * f)
}

/// `Math_SmoothStepToS` on an s16 angle held as `i32`.
fn smooth_s(value: &mut i32, target: i32, scale: i32, step: i32, min_step: i32) -> i32 {
    let w = |v: i32| to_s16(v as f64);
    let diff = w(target - *value);
    if *value != target {
        let mut size = diff / scale;
        if size > min_step || size < -min_step {
            size = size.clamp(-step, step);
            *value = w(*value + size);
        } else if diff >= 0 {
            *value = w(*value + min_step);
            if w(target - *value) <= 0 {
                *value = target;
            }
        } else {
            *value = w(*value - min_step);
            if w(target - *value) >= 0 {
                *value = target;
            }
        }
    }
    diff
}

/// `Math_SmoothStepToF` (minimum step 0).
fn smooth_f(value: &mut f64, target: f64, fraction: f64, step: f64) {
    if *value != target {
        let s = ((target - *value) * fraction).clamp(-step, step);
        *value += s;
        if (s > 0.0 && *value > target) || (s < 0.0 && *value < target) {
            *value = target;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Anim {
    pub clip: &'static str,
    pub frame: f64,
    end: f64,
    rate: f64,
    looping: bool,
}

impl Anim {
    fn new(clip: &'static str, end: f64, looping: bool) -> Self {
        Anim { clip, frame: 0.0, end, rate: 1.0, looping }
    }

    fn change(&mut self, clip: &'static str, rate: f64, start: f64, end: f64, looping: bool) {
        *self = Anim { clip, frame: start, end, rate, looping };
    }

    /// `SkelAnime_Update`: true when a play-once animation has reached its end (or a loop has wrapped).
    fn update(&mut self) -> bool {
        self.frame += self.rate;
        if self.looping {
            if self.rate > 0.0 && self.frame >= self.end {
                self.frame -= self.end;
                return true;
            }
            if self.rate < 0.0 && self.frame < 0.0 {
                self.frame += self.end;
                return true;
            }
            false
        } else if (self.rate > 0.0 && self.frame >= self.end) || (self.rate < 0.0 && self.frame <= self.end) {
            self.frame = self.end;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Wait,
    Retreat,
    Burrow,
    Walk,
    Attack,
    Death,
    Damage,
}

/// `EnDh` beyond the common enemy fields.
#[derive(Clone, Debug)]
pub struct Body {
    acc: f64,
    inited: bool,
    pub action: Action,
    action_state: i32,
    retreat: bool,
    pub params: i32,
    timer: i32,
    dirt_phase: i32,
    pub dirt_spread: f64,
    pub draw_dirt: bool,
    /// `shape.yOffset` in OoT units (it is -15000 model units of scale 0.01 underground).
    pub y_offset: f64,
    speed: f64,
    pub anim: Anim,
    /// The bite's AT sphere is on (head), and the burrow's AT cylinder (radius `dirt_spread * 0.6`).
    at_head: bool,
    at_dirt: bool,
    /// The world heading, which differs from the facing only while it is hurt.
    world_yaw: f64,
    cooldown: f64,
    /// `play->gameplayFrames` stand-in (the laugh every 0x5F frames).
    frames: u32,
}

impl Body {
    fn new(yaw: f64) -> Self {
        Body {
            acc: 0.0,
            inited: false,
            action: Action::Wait,
            action_state: 0,
            retreat: false,
            params: 0,
            timer: 0,
            dirt_phase: 0,
            dirt_spread: 0.0,
            draw_dirt: false,
            y_offset: -150.0,
            speed: 0.0,
            anim: Anim::new("Walk", WALK, true),
            at_head: false,
            at_dirt: false,
            world_yaw: yaw,
            cooldown: 0.0,
            frames: 0,
        }
    }

    /// The head's AC sphere is live (`curAction != DH_DAMAGE && shape.yOffset == 0`) and it is not dying.
    pub fn hittable(&self) -> bool {
        self.action != Action::Death && self.action != Action::Damage && self.y_offset == 0.0
    }

    /// `(forward, up)` of the head sphere from the origin.
    fn head(&self) -> (f64, f64) {
        let a = &self.anim;
        let (f, u) = match a.clip {
            "Attack" => sample(&HEAD_ATTACK.1, HEAD_ATTACK.0, a.frame),
            "Bite" => sample(&HEAD_BITE.1, HEAD_BITE.0, a.frame),
            "Damage" => sample(&HEAD_DAMAGE.1, HEAD_DAMAGE.0, a.frame),
            "Burrow" => sample(&HEAD_BURROW.1, HEAD_BURROW.0, a.frame),
            "Death" => sample(&HEAD_DEATH.1, HEAD_DEATH.0, a.frame),
            "DeathLoop" => (113.0, 5.0),
            "Walk" => HEAD_WALK,
            _ => HEAD_IDLE,
        };
        (f, u + self.y_offset)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HandAction {
    Wait,
    TakeDamage,
    Die,
}

/// `EnDha` beyond the common enemy fields.
#[derive(Clone, Debug)]
pub struct Hand {
    acc: f64,
    inited: bool,
    parent: Option<usize>,
    action: HandAction,
    /// `unk_1C0`: 8 and up while dying / hurt.
    state: i32,
    /// A killing blow landed since the last frame.
    dying: bool,
    action_timer: i32,
    timer: i32,
    grabbed: bool,
    /// The point of the hand (`handPos[0]`), world units.
    pub tip: Vec3,
    pub y_offset: f64,
    home_rot_z: i32,
    pub anim: Anim,
}

impl Hand {
    fn new() -> Self {
        Hand {
            acc: 0.0,
            inited: false,
            parent: None,
            action: HandAction::Wait,
            state: 0,
            dying: false,
            action_timer: 0,
            timer: 0,
            grabbed: false,
            tip: [0.0; 3],
            y_offset: 0.0,
            home_rot_z: 1,
            anim: Anim::new("Idle", 30.0, true),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Dh {
    Body(Box<Body>),
    Hand(Box<Hand>),
}

pub fn is_dh(kind: Kind) -> bool {
    matches!(kind, Kind::DeadHand | Kind::DeadHandHand)
}

pub fn init(e: &mut Enemy) {
    e.dh = Some(match e.kind {
        Kind::DeadHand => Dh::Body(Box::new(Body::new(e.yaw))),
        _ => Dh::Hand(Box::new(Hand::new())),
    });
    e.visible = true;
}

/// What the app tells the Dead Hand about Link each tick, and what the Dead Hand asks of him.
#[derive(Clone, Debug, Default)]
pub struct LinkView {
    pub adult: bool,
    /// His facing (`shape.rot.y`, s16 units).
    pub yaw: f64,
    /// `PLAYER_STATE2_7`: a hand holds him.
    pub held: bool,
    /// `play->grabPlayer` would succeed (not in a cutscene, not swimming, not flying with the hookshot).
    pub grabbable: bool,
    /// Which enemy holds him.
    pub held_by: Option<usize>,
    /// Outputs, taken by the app after the step.
    pub grab_request: bool,
    pub release_request: bool,
}

fn dist_xz(a: &Vec3, b: &Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn yaw_to(from: &Vec3, to: &Vec3) -> f64 {
    atan2s(to[2] - from[2], to[0] - from[0]) as f64
}

/// `Actor_IsFacingPlayer(&actor, 60 * 0x10000 / 360)`.
fn facing_of(yaw: f64, to: f64) -> bool {
    (to_s16(to - yaw) as f64).abs() < 10922.0
}

fn floor_y(world: &CollisionWorld, x: f64, y: f64, z: f64) -> Option<f64> {
    let h = world.find_floor(x, y + 120.0, z);
    h.surface.as_ref().map(|_| h.height)
}

fn head_of(pos: Vec3, yaw: f64, b: &Body) -> Vec3 {
    let (fwd, up) = b.head();
    [pos[0] + sins(yaw) * fwd * U, pos[1] + up * U, pos[2] + coss(yaw) * fwd * U]
}

/// The head sphere's centre in the world (a hand: the point of the hand).
pub fn head_center(e: &Enemy) -> Vec3 {
    match &e.dh {
        Some(Dh::Body(b)) => head_of(e.pos, e.yaw, b),
        Some(Dh::Hand(h)) => h.tip,
        None => e.pos,
    }
}

/// The hurt-box as `(feet, radius, height)`: the head sphere of the body (the whole body cylinder while it is buried, which
/// is what a bomb's blast tests), the point of a hand.
pub fn body_of(e: &Enemy) -> (Vec3, f64, f64) {
    if let Some(Dh::Body(b)) = &e.dh {
        if b.action == Action::Wait && b.y_offset < 0.0 {
            return (e.pos, 35.0 * U, 70.0 * U);
        }
    }
    let c = head_center(e);
    let r = if e.kind == Kind::DeadHand { 20.0 * U } else { 30.0 * U };
    ([c[0], c[1] - r, c[2]], r, 2.0 * r)
}

/// Can Link lock on / can a blow land right now?
pub fn targetable(e: &Enemy) -> bool {
    if !e.visible && !matches!(&e.dh, Some(Dh::Body(b)) if b.action == Action::Wait) {
        return false;
    }
    if matches!(e.st, St::Gone | St::Dying) {
        return false;
    }
    match &e.dh {
        Some(Dh::Body(b)) => b.hittable() || b.action == Action::Wait,
        Some(Dh::Hand(h)) => h.state < 8,
        None => true,
    }
}

/// `sDamageTable` of both actors: the blade, stick, spin and jump rows (reaction 0xF), nothing else.
fn damage_of(dmg: Dmg) -> Option<i32> {
    use Dmg::*;
    Some(match dmg {
        Stick | KokiriSword | MasterSword | KokiriSpin | MasterSpin => 2,
        GiantSword | GiantSpin | KokiriJump | MasterJump | HammerJump => 4,
        GiantJump => 8,
        _ => return None,
    })
}

/// A blow (`EnDh_CollisionCheck` / `EnDha_UpdateHealth` at the moment it lands).
pub fn on_hit(e: &mut Enemy, dmg: Dmg, ev: &mut Vec<Ev>) -> HitResult {
    let mut r = HitResult::default();
    let spark = head_center(e);
    let kind = e.kind;
    let Some(mut dh) = e.dh.take() else { return r };
    match &mut dh {
        Dh::Body(b) => {
            if b.action == Action::Wait && dmg == Dmg::Explosive {
                // `Actor_GetCollidedExplosive(collider1)`: a blast wakes it
                b.params = ENDH_START_ATTACK_BOMB;
            } else if b.hittable() && !b.retreat {
                if let Some(damage) = damage_of(dmg) {
                    r.applied = true;
                    r.damage = damage;
                    ev.push(Ev::Spark { pos: spark, hard: false });
                    ev.push(Ev::Hurt { kind, dmg, pos: spark });
                    e.flash = frames(8.0);
                    e.killed_by = Some(dmg);
                    b.at_head = false;
                    let last = e.hp;
                    e.hp -= damage;
                    if e.hp <= 0 {
                        e.hp = 0;
                        r.killed = true;
                        setup_death(e, b, ev);
                    } else {
                        // it retreats underground when its hit points cross 15, 9 and 3
                        if (last >= 15 && e.hp < 15) || (last >= 9 && e.hp < 9) || (last >= 3 && e.hp < 3) {
                            b.retreat = true;
                        }
                        setup_damage(e, b, ev);
                    }
                }
            }
        }
        Dh::Hand(h) => {
            if h.state < 8 {
                if let Some(damage) = damage_of(dmg) {
                    r.applied = true;
                    r.damage = damage;
                    ev.push(Ev::Spark { pos: spark, hard: false });
                    ev.push(Ev::Hurt { kind, dmg, pos: spark });
                    e.flash = frames(8.0);
                    e.killed_by = Some(dmg);
                    e.hp -= damage;
                    if e.hp <= 0 {
                        // `EnDha_SetupDeath` follows in the next frame (it needs the body); the drop is rolled now
                        e.hp = 8;
                        r.killed = true;
                        h.state = 8;
                        h.dying = true;
                        ev.push(Ev::Died { id: e.index, kind, pos: e.pos });
                    } else {
                        ev.push(Ev::Sfx { name: "EN_DEADHAND_DAMAGE", pos: e.pos });
                        h.state = 9;
                        h.action_timer = 15;
                        h.action = HandAction::TakeDamage;
                    }
                }
            }
        }
    }
    e.dh = Some(dh);
    r
}

// ---------------------------------------------------------------------------------------------- the body

fn setup_wait(e: &mut Enemy, b: &mut Body, rng: &mut Rng) {
    b.anim.change("Walk", 1.0, 0.0, WALK, true);
    b.action = Action::Wait;
    // it pops up somewhere within 300 units of where it was placed
    e.pos[0] = e.home[0] + (rng.zero_one() - 0.5) * 600.0 * U;
    e.pos[2] = e.home[2] + (rng.zero_one() - 0.5) * 600.0 * U;
    b.y_offset = -150.0;
    b.dirt_spread = 0.0;
    b.speed = 0.0;
    b.world_yaw = e.yaw;
    b.dirt_phase = 0;
    b.action_state = 0;
    b.params = 0;
}

fn setup_walk(b: &mut Body) {
    b.anim.change("Walk", 1.0, 0.0, WALK, true);
    b.action = Action::Walk;
    b.timer = 300;
    b.speed = 1.0;
}

fn setup_attack(b: &mut Body) {
    b.anim.change("Attack", 1.0, 0.0, ATTACK, false);
    b.timer = 0;
    b.action_state = 0;
    b.action = Action::Attack;
    b.speed = 0.0;
}

fn setup_burrow(e: &Enemy, b: &mut Body, ev: &mut Vec<Ev>) {
    b.anim.change("Burrow", 1.0, 0.0, BURROW, false);
    b.action = Action::Burrow;
    b.dirt_spread = 0.0;
    b.speed = 0.0;
    b.world_yaw = e.yaw;
    b.dirt_phase = 0;
    b.action_state = 0;
    ev.push(Ev::Sfx { name: "EN_DEADHAND_HIDE", pos: e.pos });
}

fn setup_retreat(b: &mut Body) {
    b.anim.change("Idle", 1.0, 0.0, IDLE, true);
    b.action = Action::Retreat;
    b.timer = 70;
    b.speed = 1.0;
}

fn setup_damage(e: &Enemy, b: &mut Body, ev: &mut Vec<Ev>) {
    b.anim.change("Damage", 1.0, 0.0, DAMAGE, false);
    b.speed = -1.0;
    b.action = Action::Damage;
    b.at_head = false;
    ev.push(Ev::Sfx { name: "EN_DEADHAND_DAMAGE", pos: e.pos });
}

fn setup_death(e: &Enemy, b: &mut Body, ev: &mut Vec<Ev>) {
    b.anim.change("Death", 1.0, 0.0, DEATH, false);
    b.action = Action::Death;
    b.timer = 300;
    b.speed = 0.0;
    b.params = ENDH_DEATH;
    b.at_head = false;
    b.at_dirt = false;
    ev.push(Ev::Sfx { name: "EN_DEADHAND_DEAD", pos: e.pos });
    ev.push(Ev::Died { id: e.index, kind: e.kind, pos: e.pos });
}

/// One 20 Hz frame of the body (`EnDh_Update`).
fn body_frame(e: &mut Enemy, player: Vec3, link: &LinkView, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let Some(Dh::Body(mut b)) = e.dh.take() else { return };
    if !b.inited {
        b.inited = true;
        // `EnDh_Init`: 14 hit points for the adult, 20 for the child
        e.hp = if link.adult { 14 } else { 20 };
    }
    let to_player = yaw_to(&e.pos, &player);
    let dist = dist_xz(&e.pos, &player) / U;
    b.cooldown = (b.cooldown - 1.0).max(0.0);
    b.frames = b.frames.wrapping_add(1);
    match b.action {
        Action::Wait => {
            if b.params >= ENDH_START_ATTACK_GRAB || b.params <= ENDH_HANDS_KILLED_4 {
                if b.action_state == 2 {
                    setup_walk(&mut b);
                } else {
                    if b.action_state == 0 {
                        e.yaw = to_player;
                        b.action_state = 1;
                        b.draw_dirt = true;
                        ev.push(Ev::Sfx { name: "EN_DEADHAND_HIDE", pos: e.pos });
                    }
                    b.dirt_phase = to_s16((b.dirt_phase + 0x3A7) as f64) as i32;
                    smooth_f(&mut b.dirt_spread, 300.0, 1.0, 5.0);
                    if b.y_offset == 0.0 {
                        b.draw_dirt = false;
                        b.action_state = 2;
                    } else if b.dirt_phase > 0x12C0 {
                        b.y_offset = (b.y_offset + 5.0).min(0.0);
                    }
                    let mut y = e.yaw as i32;
                    smooth_s(&mut y, to_player as i32, 1, 0x7D0, 0);
                    e.yaw = y as f64;
                    b.anim.update();
                }
            }
        }
        Action::Walk => {
            let mut y = e.yaw as i32;
            smooth_s(&mut y, to_player as i32, 1, 0xFA, 0);
            e.yaw = y as f64;
            b.world_yaw = e.yaw;
            b.anim.update();
            if (b.anim.frame as i32) % 8 == 0 {
                ev.push(Ev::Sfx { name: "EN_DEADHAND_WALK", pos: e.pos });
            }
            if b.frames & 0x5F == 0 {
                ev.push(Ev::Sfx { name: "EN_DEADHAND_LAUGH", pos: e.pos });
            }
            if dist <= 100.0 {
                b.speed = 0.0;
                if facing_of(e.yaw, to_player) {
                    setup_attack(&mut b);
                }
            } else {
                b.timer -= 1;
                if b.timer == 0 {
                    setup_burrow(e, &mut b, ev);
                }
            }
        }
        Action::Retreat => {
            b.timer -= 1;
            if b.timer == 0 {
                b.retreat = false;
                setup_burrow(e, &mut b, ev);
            } else {
                let mut y = e.yaw as i32;
                smooth_s(&mut y, to_s16(to_player + 32768.0), 1, 0xBB8, 0);
                e.yaw = y as f64;
            }
            b.world_yaw = e.yaw;
            b.anim.update();
        }
        Action::Attack => attack_frame(e, &mut b, player, dist, to_player, ev),
        Action::Burrow => {
            if b.action_state == 2 {
                b.draw_dirt = false;
                b.at_dirt = false;
                setup_wait(e, &mut b, rng);
            } else {
                if b.action_state == 0 {
                    b.action_state = 1;
                    b.draw_dirt = true;
                    b.at_dirt = true;
                }
                b.dirt_phase = to_s16((b.dirt_phase + 0x47E) as f64) as i32;
                smooth_f(&mut b.dirt_spread, 300.0, 1.0, 8.0);
                if b.anim.update() {
                    b.action_state = 2;
                }
                // the dirt wave hurts: an AT cylinder of radius `spread * 0.6`, 4 units
                if b.at_dirt && b.cooldown <= 0.0 && dist_xz(&e.pos, &player) < b.dirt_spread * 0.6 * U + 37.0 && (player[1] - e.pos[1]).abs() < 70.0 * U {
                    b.cooldown = 20.0;
                    ev.push(Ev::HurtPlayer { units: 4, from: e.pos, kind: Kind::DeadHand });
                }
            }
        }
        Action::Damage => {
            if b.speed < 0.0 {
                b.speed += 0.15;
            }
            b.world_yaw = to_player;
            if b.anim.update() {
                b.world_yaw = e.yaw;
                if b.retreat {
                    setup_retreat(&mut b);
                } else if dist <= 105.0 && facing_of(e.yaw, to_player) {
                    setup_attack(&mut b);
                    b.anim.change("Attack", 1.0, 20.0, ATTACK, false);
                } else {
                    setup_walk(&mut b);
                }
            }
        }
        Action::Death => death_frame(e, &mut b, ev),
    }
    // `Actor_MoveXZGravity` along the heading, then the wall and the floor
    let f = [sins(b.world_yaw), coss(b.world_yaw)];
    e.pos[0] += f[0] * b.speed * U;
    e.pos[2] += f[1] * b.speed * U;
    let mut at = [e.pos[0], e.pos[1] + 45.0 * U, e.pos[2]];
    if world.f32_find_wall_collision(&mut at, 0.0, 20.0 * U) > 0 {
        e.pos[0] = at[0];
        e.pos[2] = at[2];
    }
    if let Some(h) = floor_y(world, e.pos[0], e.pos[1], e.pos[2]) {
        e.pos[1] = h;
    }
    e.dh = Some(Dh::Body(b));
}

/// `EnDh_Attack`.
fn attack_frame(e: &mut Enemy, b: &mut Body, player: Vec3, dist: f64, to_player: f64, ev: &mut Vec<Ev>) {
    if b.anim.update() {
        b.action_state += 1;
    } else if dist > 100.0 || !facing_of(e.yaw, to_player) {
        let f = b.anim.frame;
        b.anim.change("Attack", -1.0, f, 0.0, false);
        b.action_state = 4;
        b.at_head = false;
    }
    let mut turn_to_player = false;
    match b.action_state {
        0 | 1 => {
            if b.action_state == 1 {
                b.anim.change("Bite", 1.0, 0.0, BITE, false);
                b.action_state = 2;
                ev.push(Ev::Sfx { name: "EN_DEADHAND_BITE", pos: e.pos });
            }
            turn_to_player = true;
        }
        2 => {
            // the head's AT sphere (8 units) is live from frame 4 of the bite
            if b.anim.frame >= 4.0 {
                b.at_head = true;
            }
        }
        3 => {
            if dist <= 100.0 && facing_of(e.yaw, to_player) {
                b.anim.change("Attack", 1.0, 20.0, ATTACK, false);
                b.action_state = 0;
            } else {
                b.anim.change("Attack", -1.0, ATTACK, 0.0, false);
                b.action_state += 1;
                b.at_head = false;
            }
        }
        5 => {
            setup_walk(b);
            return;
        }
        _ => {}
    }
    if turn_to_player {
        let mut y = e.yaw as i32;
        smooth_s(&mut y, to_player as i32, 1, 0x5DC, 0);
        e.yaw = y as f64;
    }
    // the bite: Link inside the head sphere is hurt for 8 (`AT_HIT`), which ends the sphere's life
    let head = head_of(e.pos, e.yaw, b);
    if b.at_head && dist_xz(&head, &player) < 20.0 * U + 37.0 && head[1] + 20.0 * U > player[1] && head[1] - 20.0 * U < player[1] + 160.0 {
        b.at_head = false;
        ev.push(Ev::HurtPlayer { units: 8, from: head, kind: Kind::DeadHand });
    }
    b.world_yaw = e.yaw;
}

/// `EnDh_Death`.
fn death_frame(e: &mut Enemy, b: &mut Body, ev: &mut Vec<Ev>) {
    let done = b.anim.update();
    if done || b.timer != 300 {
        if b.timer == 300 {
            b.anim.change("DeathLoop", 1.0, 0.0, DEATH_LOOP, true);
        }
        b.timer -= 1;
        // it fades out over the last 150 frames (alpha 255 -> 0 at 5 a frame), then the actor is killed
        if b.timer < 150 - 51 {
            e.st = St::Gone;
            e.visible = false;
        }
    } else {
        let f = b.anim.frame as i32;
        if f == 53 || f == 56 || f == 61 {
            ev.push(Ev::Sfx { name: "EN_RIZA_DOWN", pos: e.pos });
        }
        if f >= 61 {
            // `Actor_ChangeCategory(ACTORCAT_PROP)`: it no longer counts as a living enemy
            e.st = St::Dying;
        }
    }
}

// ---------------------------------------------------------------------------------------------- the hands

fn rest_tip(pos: Vec3) -> Vec3 {
    // the relaxed arm points straight up (`limbAngleX[0] = -0x4000`): two segments of 55 and 54 units and the hand
    [pos[0], pos[1] + (55.0 + 54.0 + 11.0) * U, pos[2]]
}

/// `EnDha_SetupDeath`: the hand sinks for 300 frames; each one that dies counts the body's `params` down.
fn start_hand_death(h: &mut Hand, enemies: &mut [Enemy], ev: &mut Vec<Ev>, pos: Vec3) {
    h.state = 8;
    h.action_timer = 300;
    h.action = HandAction::Die;
    h.dying = false;
    if let Some(p) = h.parent {
        if let Some(Dh::Body(b)) = &mut enemies[p].dh {
            if b.params != ENDH_DEATH {
                ev.push(Ev::Sfx { name: "EN_DEADHAND_HAND_DEAD", pos });
            }
            if b.params <= 0 {
                b.params -= 1;
            }
        }
    }
}

/// One 20 Hz frame of a hand (`EnDha_Update`).
fn hand_frame(enemies: &mut [Enemy], i: usize, player: Vec3, link: &mut LinkView, rng: &mut Rng, ev: &mut Vec<Ev>) {
    let pos = enemies[i].pos;
    let Some(Dh::Hand(mut h)) = enemies[i].dh.take() else { return };
    if !h.inited {
        h.inited = true;
        enemies[i].hp = 8;
        h.tip = rest_tip(pos);
        h.action_timer = (rng.zero_one() * 10.0 + 5.0) as i32;
        // `Actor_FindNearby(ACTOR_EN_DH, 10000)`
        h.parent = enemies.iter().position(|o| o.kind == Kind::DeadHand);
    }
    let parent_dying = h.parent.is_some_and(|p| matches!(&enemies[p].dh, Some(Dh::Body(b)) if b.params == ENDH_DEATH));
    let holding = link.held && link.held_by == Some(i);
    // `EnDha_UpdateHealth`: a killing blow, or the body is dead
    if (h.dying || parent_dying) && h.action != HandAction::Die {
        start_hand_death(&mut h, enemies, ev, pos);
    }
    let mut let_go = |link: &mut LinkView| {
        link.held = false;
        link.held_by = None;
        link.release_request = true;
    };
    match h.action {
        HandAction::Wait => {
            let dist = dist_xz(&pos, &player) / U;
            // the point Link is held at: a little behind him, at chest height (38 child, 56 adult)
            let target = [player[0] - sins(link.yaw) * 5.0 * U, player[1] + if link.adult { 56.0 } else { 38.0 } * U, player[2] - coss(link.yaw) * 5.0 * U];
            if dist <= 100.0 {
                let d3 = ((target[0] - h.tip[0]).powi(2) + (target[1] - h.tip[1]).powi(2) + (target[2] - h.tip[2]).powi(2)).sqrt() / U;
                if d3 <= 12.0 {
                    if !h.grabbed {
                        if link.grabbable && !link.held {
                            h.timer = 0;
                            h.grabbed = true;
                            link.grab_request = true;
                            link.held = true;
                            link.held_by = Some(i);
                            if let Some(p) = h.parent {
                                if let Some(Dh::Body(b)) = &mut enemies[p].dh {
                                    b.params = ENDH_START_ATTACK_GRAB;
                                }
                            }
                            ev.push(Ev::Sfx { name: "EN_DEADHAND_GRIP", pos });
                        }
                    } else {
                        h.timer = to_s16((h.timer + 0x1194) as f64) as i32;
                        if !link.held {
                            // he broke free: the hand recoils
                            h.grabbed = false;
                            h.action = HandAction::TakeDamage;
                            h.action_timer = 15;
                            enemies[i].dh = Some(Dh::Hand(h));
                            return;
                        }
                        if h.timer < -0x6E6B {
                            ev.push(Ev::Sfx { name: "EN_DEADHAND_GRIP", pos });
                        }
                    }
                } else {
                    if holding {
                        let_go(link);
                    }
                    if h.home_rot_z != 0 {
                        ev.push(Ev::Sfx { name: "EN_DEADHAND_HAND_AT", pos });
                        h.home_rot_z = 0;
                    }
                }
                enemies[i].yaw = yaw_to(&pos, &target);
                // `Math_SmoothStepToF(&handPos[0].x, playerPos.x, 1.0, 16.0, 0.0)` on each axis
                for k in 0..3 {
                    smooth_f(&mut h.tip[k], target[k], 1.0, 16.0 * U);
                }
            } else {
                if holding {
                    let_go(link);
                }
                h.grabbed = false;
                h.home_rot_z = 1;
                // the arm relaxes upright
                let rest = rest_tip(pos);
                for k in 0..3 {
                    smooth_f(&mut h.tip[k], rest[k], 1.0, 16.0 * U);
                }
            }
            h.anim.update();
        }
        HandAction::TakeDamage => {
            if holding {
                let_go(link);
            }
            h.grabbed = false;
            let rest = rest_tip(pos);
            for k in 0..3 {
                smooth_f(&mut h.tip[k], rest[k], 1.0, 16.0 * U);
            }
            h.anim.update();
            h.action_timer -= 1;
            if h.action_timer == 0 {
                h.action = HandAction::Wait;
                h.state = 0;
                h.home_rot_z = 1;
            }
        }
        HandAction::Die => {
            if holding {
                let_go(link);
            }
            h.grabbed = false;
            let rest = rest_tip(pos);
            let mut relaxed = true;
            for k in 0..3 {
                smooth_f(&mut h.tip[k], rest[k], 1.0, 16.0 * U);
                if (h.tip[k] - rest[k]).abs() > 0.5 {
                    relaxed = false;
                }
            }
            h.anim.update();
            if relaxed {
                if h.action_timer != 0 {
                    if h.y_offset > -120.0 {
                        // sinks 10 units a frame
                        h.y_offset -= 10.0;
                    } else {
                        h.action_timer -= 1;
                        if parent_dying {
                            enemies[i].st = St::Gone;
                            enemies[i].visible = false;
                        }
                    }
                } else {
                    // comes back up 5 a frame and waits again with full health
                    h.y_offset += 5.0;
                    if h.y_offset >= 0.0 {
                        h.y_offset = 0.0;
                        h.action = HandAction::Wait;
                        h.state = 0;
                        h.home_rot_z = 1;
                        h.action_timer = (rng.zero_one() * 10.0 + 5.0) as i32;
                        enemies[i].hp = 8;
                    }
                }
            }
        }
    }
    enemies[i].dh = Some(Dh::Hand(h));
}

/// One sim tick of every Dead Hand and hand: whole 20 Hz frames, two per three ticks.
pub fn step(enemies: &mut [Enemy], player: Vec3, link: &mut LinkView, world: &CollisionWorld, rng: &mut Rng, ev: &mut Vec<Ev>) {
    for i in 0..enemies.len() {
        if !is_dh(enemies[i].kind) || enemies[i].asleep || enemies[i].st == St::Gone {
            continue;
        }
        let frames_due = {
            let acc = match &mut enemies[i].dh {
                Some(Dh::Body(b)) => &mut b.acc,
                Some(Dh::Hand(h)) => &mut h.acc,
                None => continue,
            };
            *acc += FRAME_PER_TICK;
            let mut n = 0;
            while *acc >= 1.0 {
                *acc -= 1.0;
                n += 1;
            }
            n
        };
        for _ in 0..frames_due {
            match enemies[i].kind {
                Kind::DeadHand => body_frame(&mut enemies[i], player, link, world, rng, ev),
                _ => hand_frame(enemies, i, player, link, rng, ev),
            }
        }
        // the clip the view plays; the body is out of sight while it is deep underground
        let clip = match &enemies[i].dh {
            Some(Dh::Body(b)) => b.anim.clip,
            Some(Dh::Hand(h)) => h.anim.clip,
            None => "Idle",
        };
        enemies[i].play(clip);
        if let Some(Dh::Body(b)) = &enemies[i].dh {
            if enemies[i].st != St::Gone {
                enemies[i].visible = b.y_offset > -100.0 || b.action == Action::Death || b.action == Action::Burrow;
            }
        }
    }
}

/// The body's or hand's height below the ground for the view, world units (`shape.yOffset`).
pub fn view_offset(e: &Enemy) -> f64 {
    match &e.dh {
        Some(Dh::Body(b)) => b.y_offset * U,
        Some(Dh::Hand(h)) => h.y_offset * U,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::SurfaceOptions;
    use std::sync::Arc;

    fn flat() -> CollisionWorld {
        let mut w = CollisionWorld::default();
        w.add_triangle([-30000.0, 0.0, -30000.0], [30000.0, 0.0, 30000.0], [30000.0, 0.0, -30000.0], &SurfaceOptions::default());
        w.add_triangle([-30000.0, 0.0, -30000.0], [-30000.0, 0.0, 30000.0], [30000.0, 0.0, 30000.0], &SurfaceOptions::default());
        w
    }

    /// The Well's room: four hands around the body.
    fn room() -> (Field, Vec<usize>, usize) {
        let mut f = Field::new(7);
        let hands: Vec<usize> = [[-700.0, 0.0, -700.0], [700.0, 0.0, -700.0], [-700.0, 0.0, 700.0], [700.0, 0.0, 700.0]].iter().map(|p| f.add(Kind::DeadHandHand, *p, 0.0, -1)).collect();
        let body = f.add(Kind::DeadHand, [0.0; 3], 0.0, -1);
        f.link.grabbable = true;
        (f, hands, body)
    }

    fn body_of(f: &Field, i: usize) -> &Body {
        match &f.enemies[i].dh {
            Some(Dh::Body(b)) => b,
            _ => panic!("not a body"),
        }
    }

    fn hand_of(f: &Field, i: usize) -> &Hand {
        match &f.enemies[i].dh {
            Some(Dh::Hand(h)) => h,
            _ => panic!("not a hand"),
        }
    }

    fn run(f: &mut Field, link: Vec3, ticks: usize, w: &CollisionWorld) -> Vec<Ev> {
        let mut all = vec![];
        for _ in 0..ticks {
            f.step(link, w);
            all.extend(f.events.clone());
        }
        all
    }

    #[test]
    fn the_body_waits_underground_until_a_hand_grabs_link_then_comes_out_and_walks_at_him() {
        let w = flat();
        let (mut f, hands, body) = room();
        // Link stands within reach of the first hand, at the foot of its arm
        let link = [-700.0 + 60.0 * U, 0.0, -700.0];
        run(&mut f, [0.0, 0.0, 3000.0], 20, &w);
        assert_eq!(body_of(&f, body).action, Action::Wait);
        assert_eq!(body_of(&f, body).y_offset, -150.0, "buried");
        assert_eq!(f.enemies[body].hp, 20, "a child: 20 hit points");
        // the hand reaches for his chest at 16 units a frame and takes hold
        let mut grabbed = false;
        for _ in 0..120 {
            f.step(link, &w);
            if std::mem::take(&mut f.link.grab_request) {
                grabbed = true;
                break;
            }
        }
        assert!(grabbed && f.link.held && f.link.held_by == Some(hands[0]));
        assert_eq!(body_of(&f, body).params, ENDH_START_ATTACK_GRAB);
        // the body rises (yOffset +5 a frame after the dirt wave has built up) and starts walking
        run(&mut f, link, 400, &w);
        assert!(body_of(&f, body).y_offset == 0.0, "{}", body_of(&f, body).y_offset);
        assert!(matches!(body_of(&f, body).action, Action::Walk | Action::Attack), "{:?}", body_of(&f, body).action);
    }

    #[test]
    fn he_shakes_free_and_the_hand_recoils_for_15_frames() {
        let w = flat();
        let (mut f, hands, _) = room();
        f.link.adult = true;
        let link = [-700.0 + 60.0 * U, 0.0, -700.0];
        for _ in 0..120 {
            f.step(link, &w);
            if f.link.grab_request {
                break;
            }
        }
        assert!(hand_of(&f, hands[0]).grabbed);
        // he breaks free (the app clears `held`): the hand goes into `TakeDamage` and lets go
        f.link.grab_request = false;
        f.link.held = false;
        run(&mut f, link, 3, &w);
        assert_eq!(hand_of(&f, hands[0]).action, HandAction::TakeDamage);
        run(&mut f, link, 30, &w);
        assert_eq!(hand_of(&f, hands[0]).action, HandAction::Wait);
    }

    #[test]
    fn killing_the_four_hands_brings_the_body_out_and_a_hand_comes_back_after_300_frames() {
        let w = flat();
        let (mut f, hands, body) = room();
        let link = [0.0, 0.0, 3000.0];
        run(&mut f, link, 3, &w);
        for (n, h) in hands.iter().enumerate() {
            // four blows of 2 for 8 hit points, 15 frames (23 ticks) apart: a hand ignores blows while it recoils
            for k in 0..4 {
                let r = f.hit(*h, Dmg::MasterSword, link);
                assert!(r.applied, "hand {n} blow {k}");
                run(&mut f, link, 25, &w);
            }
            assert_eq!(hand_of(&f, *h).action, HandAction::Die, "hand {n} is dying");
            assert_eq!(body_of(&f, body).params, -(n as i32 + 1), "each dead hand counts the body's params down");
        }
        run(&mut f, link, 300, &w);
        assert!(body_of(&f, body).y_offset > -150.0, "the body is coming out");
        // a hand sinks 120 units, waits 300 frames (450 ticks), then rises again with 8 hit points
        run(&mut f, link, 1500, &w);
        assert_eq!(hand_of(&f, hands[0]).action, HandAction::Wait);
        assert_eq!(f.enemies[hands[0]].hp, 8);
    }

    #[test]
    fn a_blast_wakes_the_buried_body_but_blades_cannot_touch_it() {
        let w = flat();
        let (mut f, _, body) = room();
        let link = [0.0, 0.0, 3000.0];
        run(&mut f, link, 3, &w);
        let r = f.hit(body, Dmg::MasterSword, link);
        assert!(!r.applied && body_of(&f, body).params == 0);
        f.hit(body, Dmg::Explosive, link);
        assert_eq!(body_of(&f, body).params, ENDH_START_ATTACK_BOMB);
        run(&mut f, link, 300, &w);
        assert_eq!(body_of(&f, body).y_offset, 0.0);
    }

    /// Bring the body out with a blast and let it settle in front of Link.
    fn emerged() -> (Field, usize, CollisionWorld) {
        let w = flat();
        let (mut f, _, body) = room();
        f.link.adult = true;
        let link = [0.0, 0.0, 3000.0];
        run(&mut f, link, 3, &w);
        f.hit(body, Dmg::Explosive, link);
        run(&mut f, link, 300, &w);
        (f, body, w)
    }

    #[test]
    fn it_retreats_underground_each_time_its_hit_points_cross_15_9_and_3_and_dies_at_zero() {
        let (mut f, body, w) = emerged();
        assert_eq!(f.enemies[body].hp, 14, "an adult: 14 hit points");
        let link = [0.0, 0.0, 3000.0];
        // the head is where the model's head is: aim blows at it from just in front of Link's reach
        let mut retreats = 0;
        let mut guard = 0;
        while f.enemies[body].hp > 0 && guard < 4000 {
            guard += 1;
            f.step(link, &w);
            let b = body_of(&f, body);
            if b.action == Action::Wait && b.params == 0 {
                // buried again: a blast brings it back (it would take its hands otherwise)
                f.hit(body, Dmg::Explosive, link);
            }
            let b = body_of(&f, body);
            if b.hittable() && !b.retreat {
                let before = f.enemies[body].hp;
                f.enemies[body].grace = 0.0;
                let r = f.hit(body, Dmg::MasterSword, link);
                if r.applied {
                    assert_eq!(f.enemies[body].hp, before - 2);
                    if body_of(&f, body).retreat {
                        retreats += 1;
                    }
                }
            }
        }
        assert_eq!(retreats, 2, "14 -> 12 -> ... -> 8 crosses 9; 4 -> 2 crosses 3 (15 was never above)");
        assert_eq!(f.enemies[body].hp, 0);
        assert_eq!(body_of(&f, body).action, Action::Death);
        assert_eq!(body_of(&f, body).params, ENDH_DEATH);
        // it lies down, stops counting as a living enemy after frame 61, and is gone 150 frames later
        run(&mut f, link, 200, &w);
        assert_eq!(f.enemies[body].st, St::Dying);
        run(&mut f, link, 500, &w);
        assert_eq!(f.enemies[body].st, St::Gone);
    }

    #[test]
    fn the_bite_hurts_for_half_a_heart_once_its_frame_4_has_passed() {
        let (mut f, body, w) = emerged();
        // let it come to Link: it walks at 1 unit a frame and bites inside 100 units
        let link = [0.0, 0.0, 400.0];
        let mut bitten = None;
        for _ in 0..1500 {
            f.step(link, &w);
            for e in &f.events {
                if let Ev::HurtPlayer { units, kind: Kind::DeadHand, .. } = e {
                    bitten = Some(*units);
                }
            }
            if bitten.is_some() {
                break;
            }
        }
        assert_eq!(bitten, Some(8));
        let _ = body;
    }

    #[test]
    fn smooth_step_to_s_and_the_animation_clock_follow_the_decomp() {
        let mut a = 0;
        // far from the target a step is the largest allowed
        smooth_s(&mut a, 0x4000, 1, 0x7D0, 0);
        assert_eq!(a, 0x7D0);
        // within a step it arrives
        let mut b = 0x4000 - 100;
        smooth_s(&mut b, 0x4000, 1, 0x7D0, 0);
        assert_eq!(b, 0x4000);
        // wrapping the short way round
        let mut c = 0x7F00;
        smooth_s(&mut c, -0x7F00, 1, 0x7D0, 0);
        assert!(c > 0x7F00 || c < -0x7000, "{c}");
        let mut an = Anim::new("Bite", BITE, false);
        let mut n = 0;
        while !an.update() {
            n += 1;
        }
        assert_eq!(n + 1, 20, "a 20 frame animation is done on its 20th update");
        let _ = Arc::new(0);
    }
}
