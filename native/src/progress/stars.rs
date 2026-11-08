//! Power-star / key placement and collection (`sm64World.ts` star placement, `Game.ts`
//! `checkStar`, `finishReward`, `stepSecretStar`, `stepSpawnedStar`, `checkSecrets`).
//!
//! Engine-free: [`StarDirector`] owns every reward in the course (the primary star, placed and
//! event stars, the 100-coin star), decides visibility, drives the appear animations and reports
//! pickups. Rendering reads [`StarDirector::rewards`]; the flow plugin reacts to [`Collected`].
use super::data::{ProgressData, SecretStar, V3};
use super::save::{CastleKey, SaveFile};
use crate::game::courses::SCORE_COURSES;
use jumpman_sim::behaviors::koopa_the_quick::KOOPA_RACE_STAR;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::math::sins;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::object::{mario_hitbox, ObjectBehaviorKind};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::spawned_star::SpawnedStar;
use jumpman_sim::state::*;
use std::collections::{HashMap, HashSet};

/// The act number used for the 100-coin star.
pub const HUNDRED_COIN_ACT: i32 = 7;

/// `starLocations`: per-course fallback goal when no star is placed.
fn star_location(id: &str) -> Option<V3> {
    Some(match id {
        "bob" => [1636., 20000., -5567.],
        "castle_grounds" => [0., 1000., -1300.],
        "wf" => [180., 3880., 340.],
        "bbh" => [102., 20000., 218.],
        "ccm" => [512., 20000., -511.],
        "lll" => [0., 20000., -4385.],
        "ssl" => [-8191., 20000., 1536.],
        "wdw" => [-255., 20000., -4351.],
        "sl" => [-5688., 20000., -8191.],
        "ttm" => [0., 20000., 0.],
        "thi" => [-1023., 20000., -2555.],
        "ttc" => [1457., 20000., -602.],
        "rr" => [-2880., 20000., -6570.],
        "bitdw" => [6611., 20000., -518.],
        "bitfs" => [1536., 20000., -262.],
        "bits" => [214., 20000., -5698.],
        "bowser_1" | "bowser_2" | "bowser_3" => [0., 20000., 0.],
        "pss" => [-6385., -4000., 5770.],
        "mushroom2d" => [2380., 760., 0.],
        "playground" => [2900., 820., -4200.],
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RewardKind {
    Primary,
    Placed,
    Red,
    Snowman,
    Penguin,
    Secret,
    Chest,
    Box,
    Hundred,
}

#[derive(Clone, Debug)]
pub struct Reward {
    pub kind: RewardKind,
    pub act: i32,
    pub pos: V3,
    pub visible: bool,
    /// Already in the save file: drawn as the transparent ghost star.
    pub saved: bool,
    /// Spin angle (s16) for the red-star reveal.
    pub yaw: f64,
}

/// A pickup the flow layer must celebrate.
#[derive(Clone, Debug, PartialEq)]
pub struct Collected {
    /// Index into [`StarDirector::rewards`], or `None` for the 100-coin star.
    pub reward: Option<usize>,
    pub act: i32,
    pub pos: V3,
    /// 100-coin star / no-exit star: Mario stays in the course after dancing.
    pub bonus: bool,
    pub key: Option<CastleKey>,
}

#[derive(Default, Debug)]
pub struct StarStep {
    /// Toast messages to show.
    pub toasts: Vec<String>,
    pub collected: Option<Collected>,
    /// A star cutscene is running: the rest of the world is frozen.
    pub freeze: bool,
}

#[derive(Clone, Debug)]
struct ObjectStar {
    act: i32,
    origin: Option<V3>,
    pos: V3,
}

/// What the secret / red-coin star reveal camera needs (`stepSecretStar`).
#[derive(Debug, Clone, Copy)]
pub struct RevealInfo {
    /// The reveal tick just processed (4..=119).
    pub t: i32,
    /// Position of the animated star.
    pub group: V3,
    /// Where the star ends up.
    pub target: V3,
    /// Height of the bounce at `t`.
    pub y: f64,
    /// Red-coin reveal in bbh keeps the regular camera shot.
    pub regular: bool,
}

/// A spawned star in flight (100 coins, boxes): `stepSpawnedStar`'s camera input.
#[derive(Debug, Clone, Copy)]
pub struct SpawnInfo {
    pub pos: V3,
    pub returning: bool,
}

#[derive(Debug)]
pub struct StarDirector {
    /// Set while a reveal cutscene runs (read by `progress::shots`).
    pub reveal: Option<RevealInfo>,
    pub spawn_shot: Option<SpawnInfo>,
    pub course: String,
    pub act: i32,
    /// `rewards[0]` is the primary star.
    pub rewards: Vec<Reward>,
    pub hundred: Option<Reward>,
    star_requires_event: bool,
    course_star_visible: bool,
    pub mission_complete: bool,
    pub star_ready: bool,
    pub reward_key: Option<CastleKey>,
    pub no_exit: bool,
    primary_act: i32,
    red_pos: Option<V3>,
    secret: Option<SecretStar>,
    secret_hits: HashSet<usize>,
    /// Position of the most recent secret trigger touched (`coinEffects.number` anchor).
    pub last_secret: Option<(u32, [f64; 3])>,
    red_timer: i32,
    secret_timer: i32,
    object_star: Option<ObjectStar>,
    box_flights: HashMap<i32, SpawnedStar>,
    coin_flight: Option<SpawnedStar>,
    collected_no_exit: bool,
    hint_cooldown: i32,
    /// Direction in the star's final home position for the primary reward (reset target).
    primary_home: V3,
}

fn touches(m: &MarioRuntime, pos: V3, radius: f64, height: f64) -> bool {
    let hb = mario_hitbox(m);
    let [x, y, z] = m.pos;
    m.action & ACT_FLAG_INTANGIBLE == 0 && (x - pos[0]).powi(2) + (z - pos[2]).powi(2) < (radius + hb.radius).powi(2) && y <= pos[1] + height && y + hb.height >= pos[1]
}

impl StarDirector {
    /// `prepareArea` star placement for `course` act `act`.
    pub fn new(course: &str, act: i32, data: &ProgressData, spawn_pos: V3, world: &CollisionWorld, m: &MarioRuntime, save: &SaveFile) -> Self {
        let red_act = data.red_coin_act.unwrap_or(1);
        let red_star = if act == red_act { data.red_coin_star } else { None };
        let placements: Vec<_> = data.placed_stars.iter().filter(|s| s.acts.is_empty() || s.acts.contains(&act)).collect();
        let selected = placements.iter().find(|s| s.act == act).copied();
        let mission = if course == "bob" && act == 2 { Some(KOOPA_RACE_STAR) } else { star_location(course) };
        let [x, y, z] = red_star.or(selected.map(|s| s.pos)).or(mission).unwrap_or(spawn_pos);
        let fixed_y = (course == "bob" && act == 2) || ["ccm", "wf", "castle_inside", "ddd", "jrb", "mushroom2d", "playground"].contains(&course) || red_star.is_some() || selected.is_some();
        let sy = if fixed_y {
            y
        } else {
            let hit = world.find_floor(x, y, z);
            if hit.surface.is_some() { hit.height + 100.0 } else { y }
        };
        let reward_key = match course {
            "bowser_1" => Some(CastleKey::Basement),
            "bowser_2" => Some(CastleKey::Upstairs),
            _ => None,
        };
        let primary_visible = !data.exploration_only
            && course != "castle_inside"
            && course != "castle_grounds"
            && (!["ddd", "jrb", "bob", "ccm"].contains(&course) || (course == "bob" && act <= 2) || selected.is_some() || red_star.is_some())
            // The goal stand-in belongs to the first area; the pyramid, the volcano, tiny THI ... only hold their own stars.
            && (data.area <= 1 || selected.is_some() || red_star.is_some());
        let star_requires_event = red_star.is_some()
            || (selected.is_none() && ["bob", "bbh", "wf", "lll", "jrb", "bitdw", "bitfs", "bits", "bowser_1", "bowser_2", "bowser_3", "mushroom2d", "playground"].contains(&course));
        let primary_act = if red_star.is_some() { data.red_coin_act.unwrap_or(act) } else { act };
        let mut rewards = vec![Reward { kind: RewardKind::Primary, act: primary_act, pos: [x, sy, z], visible: false, saved: false, yaw: 0.0 }];

        let mut side = |kind: RewardKind, act: i32, pos: V3, rewards: &mut Vec<Reward>| rewards.push(Reward { kind, act, pos, visible: false, saved: false, yaw: 0.0 });
        for p in placements.iter().filter(|p| Some(**p).map(|s| s as *const _) != selected.map(|s| s as *const _)) {
            side(RewardKind::Placed, p.act, p.pos, &mut rewards);
        }
        if let (Some(pos), None) = (data.red_coin_star, red_star) {
            side(RewardKind::Red, red_act, pos, &mut rewards);
        }
        if let Some(s) = &data.snowman_star {
            if act == 5 {
                side(RewardKind::Snowman, s.act, s.pos, &mut rewards);
            }
        }
        let has_penguin_race = m.objects.find_by_behavior(ObjectBehaviorKind::RacingPenguin).is_some();
        if let Some(s) = &data.penguin_star {
            if has_penguin_race {
                side(RewardKind::Penguin, s.act, s.pos, &mut rewards);
            }
        }
        if let Some(s) = &data.chest_star {
            side(RewardKind::Chest, s.act, s.pos, &mut rewards);
        }
        if let Some(s) = &data.secret_star {
            side(RewardKind::Secret, s.act, s.pos, &mut rewards);
        }
        for o in m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::StarBox) {
            side(RewardKind::Box, o.param as i32, o.pos, &mut rewards);
        }

        let hundred = SCORE_COURSES.contains(&course).then(|| Reward { kind: RewardKind::Hundred, act: HUNDRED_COIN_ACT, pos: [0.0; 3], visible: false, saved: false, yaw: 0.0 });
        let mut d = Self {
            course: course.to_string(),
            act,
            rewards,
            hundred,
            star_requires_event,
            course_star_visible: primary_visible,
            mission_complete: false,
            star_ready: false,
            reward_key,
            no_exit: matches!(course, "bitdw" | "bitfs" | "bits"),
            primary_act,
            red_pos: data.red_coin_star,
            reveal: None,
            spawn_shot: None,
            secret: data.secret_star.clone(),
            secret_hits: HashSet::new(),
            last_secret: None,
            red_timer: -1,
            secret_timer: -1,
            object_star: None,
            box_flights: HashMap::new(),
            coin_flight: None,
            collected_no_exit: false,
            hint_cooldown: 0,
            primary_home: [x, sy, z],
        };
        d.reset(save);
        d
    }

    pub fn primary(&self) -> &Reward {
        &self.rewards[0]
    }

    /// `resetMissionStar`.
    pub fn reset(&mut self, save: &SaveFile) {
        self.red_timer = -1;
        self.secret_timer = -1;
        self.object_star = None;
        self.secret_hits.clear();
        self.box_flights.clear();
        self.coin_flight = None;
        self.collected_no_exit = false;
        self.hint_cooldown = 0;
        let course = self.course.clone();
        if let Some(h) = &mut self.hundred {
            h.visible = false;
            h.saved = save.has_star(&course, HUNDRED_COIN_ACT);
        }
        let primary_act = self.primary_act;
        let key = self.reward_key.is_some();
        let bowser3 = self.course == "bowser_3";
        for (i, r) in self.rewards.iter_mut().enumerate() {
            r.yaw = 0.0;
            if i == 0 {
                r.pos = self.primary_home;
                r.saved = save.has_star(&course, primary_act) && !key && !bowser3;
                continue;
            }
            r.pos = match r.kind {
                RewardKind::Red => self.red_pos.unwrap_or(r.pos),
                _ => r.pos,
            };
            r.visible = !matches!(r.kind, RewardKind::Snowman | RewardKind::Penguin | RewardKind::Secret | RewardKind::Red | RewardKind::Chest | RewardKind::Box);
            r.saved = save.has_star(&course, r.act);
        }
        self.mission_complete = !self.star_requires_event;
        let saved = self.rewards[0].saved;
        // Collected primary stars stay visible as ghosts; uncollected ones need `mission_complete`.
        self.rewards[0].visible = self.course_star_visible && self.mission_complete;
        let _ = saved;
        self.star_ready = !self.star_requires_event && self.rewards[0].visible;
    }

    /// Coin count crossed 100: the 100-coin star bursts out of Mario (`addCoins`).
    pub fn hundred_coins(&mut self, mario_pos: V3) {
        if self.hundred.is_none() || self.coin_flight.is_some() {
            return;
        }
        let flight = SpawnedStar::new(mario_pos, Some(mario_pos));
        if let Some(h) = &mut self.hundred {
            h.pos = flight.pos;
            h.visible = true;
        }
        self.coin_flight = Some(flight);
    }

    /// All 8 red coins collected.
    pub fn red_coins_done(&mut self) -> bool {
        if self.red_pos.is_some() {
            self.red_timer = 0;
            true
        } else {
            false
        }
    }

    /// `requiredCoins` reached in an authored coin course (mushroom2d): the finish star appears.
    pub fn coins_goal_reached(&mut self) {
        if !self.mission_complete {
            self.mission_complete = true;
            self.star_ready = true;
            self.rewards[0].visible = self.course_star_visible;
        }
    }

    /// `processObjectEvents` cases that gate stars.
    pub fn on_event(&mut self, e: &ObjectEvent) -> Option<String> {
        match e {
            ObjectEvent::BoxStar { pos, act } => {
                self.box_flights.insert(*act as i32, SpawnedStar::new(*pos, None));
                None
            }
            ObjectEvent::Boss { pos } => {
                self.object_star = Some(ObjectStar { act: 1, origin: Some([pos[0], pos[1] + 200.0, pos[2]]), pos: [2000., 4500., -4500.] });
                self.secret_timer = 4;
                Some("KING BOB-OMB DEFEATED".into())
            }
            ObjectEvent::Snowman { pos, cue: "reward" } => {
                if let Some(r) = self.rewards.iter().find(|r| r.kind == RewardKind::Snowman) {
                    self.object_star = Some(ObjectStar { act: r.act, origin: Some(*pos), pos: r.pos });
                    self.secret_timer = 4;
                }
                None
            }
            ObjectEvent::PenguinReward { pos } => {
                if let Some(r) = self.rewards.iter().find(|r| r.kind == RewardKind::Penguin) {
                    self.object_star = Some(ObjectStar { act: r.act, origin: Some(*pos), pos: r.pos });
                    self.secret_timer = 4;
                }
                None
            }
            ObjectEvent::ChestReward => {
                if let Some(r) = self.rewards.iter_mut().find(|r| r.kind == RewardKind::Chest) {
                    r.visible = true;
                    return Some("ALL CHESTS OPEN".into());
                }
                None
            }
            ObjectEvent::Bowser { pos } => {
                self.mission_complete = true;
                self.star_ready = true;
                if self.reward_key.is_some() {
                    // The key drops to the floor where Bowser fell.
                    self.rewards[0].pos = *pos;
                } else if self.course == "bowser_3" {
                    self.rewards[0].pos = *pos;
                }
                Some("BOWSER DEFEATED".into())
            }
            ObjectEvent::BigBully { .. } => {
                self.mission_complete = true;
                self.star_ready = true;
                Some("BIG BULLY DEFEATED".into())
            }
            ObjectEvent::WhompKing { .. } => {
                self.mission_complete = true;
                self.star_ready = true;
                Some("WHOMP KING DEFEATED".into())
            }
            ObjectEvent::BigBoo { .. } => {
                self.mission_complete = true;
                self.star_ready = true;
                Some("BIG BOO DEFEATED".into())
            }
            ObjectEvent::Race { state: "end" } | ObjectEvent::Race { state: "won" } => None,
            _ => None,
        }
    }

    /// Koopa race won: the star for act 2 appears at its goal.
    pub fn race_won(&mut self) {
        if self.course == "bob" && self.act == 2 {
            self.object_star = Some(ObjectStar { act: 2, origin: None, pos: KOOPA_RACE_STAR });
            self.secret_timer = 4;
        }
    }

    fn reveal_y(t: i32) -> f64 {
        let arc = t - 26;
        if (0..=30).contains(&arc) {
            return 400.0 * sins((f64::from(arc) * 32768.0 / 30.0).trunc());
        }
        if arc > 30 {
            let bounce = arc - 30;
            let rising = f64::from(20.min(bounce));
            return (rising * (41.0 - rising) / 2.0 - f64::from((bounce - 20).max(0)) * 10.0).max(0.0);
        }
        0.0
    }

    /// bhv_hidden_star_loop / bhv_star_spawn_loop. Returns true while the reveal freezes the world.
    fn step_secret(&mut self, toasts: &mut Vec<String>) -> bool {
        let secret_active = (0..120).contains(&self.secret_timer);
        let red_active = !secret_active && (0..120).contains(&self.red_timer);
        self.reveal = None;
        if !secret_active && !red_active {
            return false;
        }
        let (target_pos, origin) = if red_active {
            (self.red_pos.unwrap(), None)
        } else if let Some(o) = &self.object_star {
            (o.pos, o.origin)
        } else if let Some(s) = &self.secret {
            (s.pos, s.origin)
        } else {
            return false;
        };
        let t = if red_active { let v = self.red_timer; self.red_timer += 1; v } else { let v = self.secret_timer; self.secret_timer += 1; v };
        // Which reward group is animated?
        let idx = if red_active {
            if self.act == self.red_act() { Some(0) } else { self.rewards.iter().position(|r| r.kind == RewardKind::Red) }
        } else if let Some(o) = &self.object_star {
            self.rewards.iter().position(|r| r.act == o.act && r.kind != RewardKind::Primary).or(if self.course == "bob" && o.act == self.act { Some(0) } else { None })
        } else {
            self.rewards.iter().position(|r| r.kind == RewardKind::Secret)
        };
        let Some(idx) = idx else { return false };
        if t < 4 {
            return false;
        }
        let y = Self::reveal_y(t);
        let arc = t - 26;
        let r = &mut self.rewards[idx];
        r.visible = true;
        if red_active {
            let spin = if t < 57 { 0x1000 as f64 } else if t <= 98 { 0x1000 as f64 - f64::from(t - 57) * 16.0 } else { 0x800 as f64 };
            r.yaw += spin;
        }
        if let Some(origin) = origin {
            let progress = (f64::from(arc + 1) / 30.0).clamp(0.0, 1.0);
            r.pos = [0, 1, 2].map(|i| origin[i] + (target_pos[i] - origin[i]) * progress);
            r.pos[1] += y;
        } else {
            r.pos[1] = target_pos[1] + y;
        }
        self.reveal = Some(RevealInfo { t, group: r.pos, target: target_pos, y, regular: red_active && self.course == "bbh" });
        if t == 119 {
            r.pos = [r.pos[0], target_pos[1], r.pos[2]];
            if idx == 0 {
                self.mission_complete = true;
                self.star_ready = true;
            }
            toasts.push(String::new());
            toasts.pop();
        }
        true
    }

    fn red_act(&self) -> i32 {
        self.primary_act
    }

    /// `checkSecrets`: the five hidden triggers of a secret star.
    fn check_secrets(&mut self, m: &MarioRuntime) {
        let Some(puzzle) = self.secret.clone() else { return };
        if self.secret_timer >= 0 {
            return;
        }
        for (i, pos) in puzzle.triggers.iter().enumerate() {
            if self.secret_hits.contains(&i) || !touches(m, *pos, 100.0, 100.0) {
                continue;
            }
            self.secret_hits.insert(i);
            let count = 5 - puzzle.triggers.len() as i32 + self.secret_hits.len() as i32;
            if count == 5 {
                self.secret_timer = 0;
            } else {
                self.last_secret = Some((count as u32, *pos));
            }
        }
    }

    pub fn secret_triggers(&self) -> usize {
        self.secret.as_ref().map_or(0, |s| s.triggers.len())
    }

    /// Number of secret triggers hit (for HUD sparkles/sounds).
    pub fn secret_count(&self) -> usize {
        self.secret_hits.len()
    }

    /// True while a spawned-star flight or reveal freezes the rest of the world.
    pub fn cutscene_active(&self) -> bool {
        (0..120).contains(&self.secret_timer)
            || (0..120).contains(&self.red_timer)
            || self.coin_flight.as_ref().map_or(false, |f| !f.tangible())
            || self.box_flights.values().any(|f| !f.tangible() && self.rewards.iter().any(|r| r.kind == RewardKind::Box && self.box_flights.contains_key(&r.act)))
    }

    /// One 30 Hz tick (after Mario's step).
    pub fn step(&mut self, m: &MarioRuntime, save: &SaveFile, coins: u32) -> StarStep {
        let mut out = StarStep::default();
        if self.hint_cooldown > 0 {
            self.hint_cooldown -= 1;
        }
        self.check_secrets(m);
        // Spawned stars (100 coins, boxes) fly before they can be touched and freeze the world.
        let mut froze = false;
        if let Some(flight) = &mut self.coin_flight {
            if !flight.tangible() {
                flight.step();
                froze = true;
            }
        }
        for r in self.rewards.iter_mut().filter(|r| r.kind == RewardKind::Box) {
            if let Some(f) = self.box_flights.get_mut(&r.act) {
                if !f.tangible() {
                    f.step();
                    froze = true;
                }
            }
            let _ = r;
        }
        for r in self.rewards.iter_mut().filter(|r| r.kind == RewardKind::Box) {
            if let Some(f) = self.box_flights.get(&r.act) {
                r.pos = f.pos;
                r.yaw = f.yaw;
                r.visible = true;
            }
        }
        if let (Some(h), Some(f)) = (&mut self.hundred, &self.coin_flight) {
            h.pos = f.pos;
            h.yaw = f.yaw;
        }
        self.spawn_shot = None;
        if froze {
            let flight = self.coin_flight.as_ref().filter(|f| !f.tangible()).or_else(|| self.box_flights.values().find(|f| !f.tangible()));
            self.spawn_shot = flight.map(|f| SpawnInfo { pos: f.pos, returning: f.returning() });
            out.freeze = true;
            return out;
        }
        if self.step_secret(&mut out.toasts) {
            out.freeze = true;
            return out;
        }
        if m.action == ACT_JUMBO_STAR_CUTSCENE || m.action == ACT_END_PEACH_CUTSCENE || m.health < 0x100 as f64 || self.collected_no_exit {
            return out;
        }
        // Tangible spawned stars spin (their yaw follows the flight).
        if let Some(flight) = &mut self.coin_flight {
            flight.step();
            if let Some(h) = &mut self.hundred {
                h.pos = flight.pos;
                h.yaw = flight.yaw;
                if touches(m, h.pos, 80.0, 50.0) {
                    out.collected = Some(Collected { reward: None, act: HUNDRED_COIN_ACT, pos: h.pos, bonus: true, key: None });
                    h.visible = false;
                    self.coin_flight = None;
                    return out;
                }
            }
        }
        for (i, r) in self.rewards.iter_mut().enumerate().skip(1) {
            if r.kind == RewardKind::Box {
                if let Some(f) = self.box_flights.get_mut(&r.act) {
                    f.step();
                    r.pos = f.pos;
                    r.yaw = f.yaw;
                    r.visible = true;
                    if !f.tangible() {
                        continue;
                    }
                }
            }
            if r.visible && touches(m, r.pos, 80.0, 50.0) {
                r.visible = false;
                out.collected = Some(Collected { reward: Some(i), act: r.act, pos: r.pos, bonus: false, key: None });
                return out;
            }
        }
        if self.star_requires_event && self.mission_complete && self.course_star_visible {
            self.rewards[0].visible = true;
        }
        let key = self.reward_key.is_some() || self.course == "bowser_3";
        let (radius, height) = if key { (160.0, 100.0) } else { (80.0, 50.0) };
        let primary = &self.rewards[0];
        if primary.visible && touches(m, primary.pos, radius, height) {
            if !self.star_ready {
                if self.hint_cooldown == 0 {
                    self.hint_cooldown = 90;
                    out.toasts.push("THE STAR IS ASLEEP".into());
                }
                return out;
            }
            let (act, pos) = (self.primary_act, primary.pos);
            self.star_ready = false;
            self.rewards[0].visible = false;
            let _ = (save, coins);
            out.collected = Some(Collected { reward: Some(0), act, pos, bonus: self.no_exit, key: self.reward_key });
        }
        out
    }

    pub fn finish_collect(&mut self, c: &Collected) {
        if c.bonus && c.reward.is_some() {
            self.collected_no_exit = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::runner::GameSim;
    use crate::game::world::CourseRequest;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn director(course: &str, act: i32) -> (GameSim, StarDirector, SaveFile) {
        let mut req = CourseRequest::new(course);
        req.act = act;
        let sim = GameSim::load(&assets(), req).unwrap();
        let data = ProgressData::load(&assets(), course, 1);
        let save = SaveFile::in_memory(1);
        let d = StarDirector::new(course, act, &data, sim.course.spawn.pos, &sim.mario.world, &sim.mario, &save);
        (sim, d, save)
    }

    #[test]
    fn reveal_arc_matches_the_ts_curve() {
        assert_eq!(StarDirector::reveal_y(10), 0.0);
        assert_eq!(StarDirector::reveal_y(26), 0.0);
        assert!((StarDirector::reveal_y(41) - 400.0).abs() < 1.0);
        assert_eq!(StarDirector::reveal_y(100), 0.0);
        assert!(StarDirector::reveal_y(60) > 0.0 && StarDirector::reveal_y(60) < 220.0);
    }

    #[test]
    fn bob_star_sleeps_until_king_is_defeated() {
        let (sim, mut d, save) = director("bob", 1);
        assert!(!d.primary().visible && !d.star_ready);
        let note = d.on_event(&ObjectEvent::Boss { pos: [3000.0, 4000.0, -4500.0] });
        assert!(note.is_some());
        // Place Mario far away and run the reveal to completion.
        let mut m_sim = sim;
        m_sim.mario.pos = [0.0, 0.0, 0.0];
        let mut frozen = 0;
        for _ in 0..200 {
            let step = d.step(&m_sim.mario, &save, 0);
            if step.freeze {
                frozen += 1;
            }
        }
        assert!((110..=125).contains(&frozen), "reveal freezes the world ~120 ticks, got {frozen}");
        assert!(d.star_ready && d.mission_complete && d.primary().visible);
        assert_eq!(d.primary().pos[0], 2000.0);
        assert_eq!(d.primary().pos[1], 4500.0);
    }

    #[test]
    fn touching_a_ready_star_collects_it() {
        let (mut sim, mut d, save) = director("wf", 1);
        // WF act 1 star needs the Whomp King.
        d.on_event(&ObjectEvent::WhompKing { pos: [0.0; 3] });
        d.rewards[0].visible = true;
        sim.mario.pos = d.primary().pos;
        sim.mario.set_action(ACT_FREEFALL, 0.0);
        let step = d.step(&sim.mario, &save, 0);
        let c = step.collected.expect("collected");
        assert_eq!((c.act, c.reward, c.bonus), (1, Some(0), false));
        assert!(!d.star_ready && !d.primary().visible);
    }

    #[test]
    fn asleep_star_hints_and_does_not_collect() {
        let (mut sim, mut d, save) = director("wf", 1);
        d.rewards[0].visible = true;
        sim.mario.pos = d.primary().pos;
        sim.mario.set_action(ACT_FREEFALL, 0.0);
        let step = d.step(&sim.mario, &save, 0);
        assert!(step.collected.is_none());
        assert_eq!(step.toasts, vec!["THE STAR IS ASLEEP".to_string()]);
    }

    #[test]
    fn hundred_coin_star_flies_then_collects() {
        let (mut sim, mut d, save) = director("bob", 1);
        sim.mario.pos = [100.0, 100.0, 100.0];
        d.hundred_coins(sim.mario.pos);
        assert!(d.cutscene_active());
        let mut seen_freeze = false;
        let mut collected = None;
        for _ in 0..200 {
            // Jump up to the hovering star.
            let star_y = d.hundred.as_ref().unwrap().pos[1];
            sim.mario.pos = [100.0, star_y - 20.0, 100.0];
            let step = d.step(&sim.mario, &save, 100);
            seen_freeze |= step.freeze;
            if step.collected.is_some() {
                collected = step.collected;
                break;
            }
        }
        assert!(seen_freeze);
        let c = collected.expect("100-coin star collected near Mario");
        assert!(c.bonus && c.act == HUNDRED_COIN_ACT && c.reward.is_none());
    }

    #[test]
    fn castle_has_no_course_star_and_keys_come_from_bowser() {
        let (_, d, _) = director("castle_inside", 1);
        assert!(!d.primary().visible);
        let (_, d, _) = director("bowser_1", 1);
        assert_eq!(d.reward_key, Some(CastleKey::Basement));
        assert!(!d.star_ready);
    }

    #[test]
    fn placed_and_red_stars_are_registered() {
        let (_, d, _) = director("wf", 1);
        assert!(d.rewards.iter().any(|r| r.kind == RewardKind::Placed && r.act == 3), "{:?}", d.rewards);
        assert!(d.rewards.iter().any(|r| r.kind == RewardKind::Red));
        assert!(d.hundred.is_some());
    }
}
