//! Balloon Battle on the four arenas (Big Donut, Block Fort, Double Deck, Skyscraper), engine-free. It runs on top of
//! [`KartRace`]: the race machinery supplies the karts, item boxes, bananas / green shells and the spin-out; this
//! module adds the battle rules (`code_80057C60.c`: `init_all_player_balloons` gives three balloons,
//! `pop_player_balloon` removes one per hit, the kart whose last balloon pops is out), the arena set-up (item box
//! spawns from `d_course_<arena>_item_box_spawns`, start grid from `spawn_players_4p_battle`, both exported into
//! `public/mk64/modes.json`) and a wander / chase brain for the CPU karts.
//!
//! The arenas have no lap path; the export gives them a ring through the start grid that only serves the Lakitu
//! rescue, so laps are switched off (`KartRace::no_progress`).
use super::kart_physics::{Ground, Input, Kart};
use super::kart_race::{Event, ItemBox, KartRace, Phase, Rng};
use std::path::Path;

pub const BALLOONS: u8 = 3;
/// Time limit of a battle (the game has none; this keeps a stalemate from running forever).
pub const TIME_LIMIT: u32 = 30 * 180;

/// One arena from `modes.json`, in course units.
#[derive(Clone, Debug, PartialEq)]
pub struct Arena {
    pub dir: String,
    pub name: String,
    /// Four start slots: position (x, y hint, z) and `rotation[1]`.
    pub grid: Vec<([f64; 3], i32)>,
    pub boxes: Vec<[f64; 3]>,
}
impl Arena {
    pub fn radius(&self) -> f64 {
        self.grid.iter().map(|(p, _)| p[0].hypot(p[2])).fold(0., f64::max)
    }
}

/// Read every arena of `<root>/modes.json` (`root` is `public/mk64`).
pub fn load_arenas(root: &Path) -> Vec<Arena> {
    let Ok(text) = std::fs::read_to_string(root.join("modes.json")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    super::kart_modes::ARENAS
        .iter()
        .filter_map(|dir| {
            let a = &v["arenas"][*dir];
            let num = |x: &serde_json::Value| x.as_f64();
            let grid = a["grid"]
                .as_array()?
                .iter()
                .map(|g| {
                    let p = g["pos"].as_array()?;
                    Some(([num(&p[0])?, num(&p[1])?, num(&p[2])?], g["rotation"].as_i64()? as i32))
                })
                .collect::<Option<Vec<_>>>()?;
            let boxes = a["boxes"].as_array()?.iter().map(|b| Some([num(&b[0])?, num(&b[1])?, num(&b[2])?])).collect::<Option<Vec<_>>>()?;
            (grid.len() == 4).then(|| Arena { dir: dir.to_string(), name: a["name"].as_str().unwrap_or(dir).to_string(), grid, boxes })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    LastStanding,
    TimeUp,
}

/// The rules alone: balloons per racer (slot 0 is the player), who is out and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    pub balloons: Vec<u8>,
    pub out_at: Vec<Option<u32>>,
    pub tick: u32,
    pub limit: u32,
    pub over: Option<End>,
    /// Times each racer was hit.
    pub hits: Vec<u32>,
}
impl Rules {
    pub fn new(racers: usize) -> Self {
        Self { balloons: vec![BALLOONS; racers], out_at: vec![None; racers], tick: 0, limit: TIME_LIMIT, over: None, hits: vec![0; racers] }
    }
    pub fn alive(&self) -> usize {
        self.balloons.iter().filter(|b| **b > 0).count()
    }
    pub fn is_out(&self, i: usize) -> bool {
        self.balloons.get(i).is_none_or(|b| *b == 0)
    }
    /// A hit: one balloon pops (`pop_player_balloon`). True when this was the kart's last.
    pub fn pop(&mut self, i: usize) -> bool {
        if self.over.is_some() || self.is_out(i) {
            return false;
        }
        self.balloons[i] -= 1;
        self.hits[i] += 1;
        if self.balloons[i] == 0 {
            self.out_at[i] = Some(self.tick);
        }
        self.check_end();
        self.balloons[i] == 0
    }
    /// The human (slot 0) is out: the battle is over for the player, whoever is left wins.
    pub fn end_if_player_out(&mut self) {
        if self.over.is_none() && self.is_out(0) {
            self.over = Some(End::LastStanding);
        }
    }
    /// One 30 Hz tick of the clock.
    pub fn step(&mut self) {
        if self.over.is_some() {
            return;
        }
        self.tick += 1;
        if self.tick >= self.limit {
            self.over = Some(End::TimeUp);
        }
    }
    fn check_end(&mut self) {
        if self.over.is_none() && self.alive() <= 1 {
            self.over = Some(End::LastStanding);
        }
    }
    pub fn time_left(&self) -> u32 {
        self.limit.saturating_sub(self.tick)
    }
    /// Racers best first: more balloons, then the one that stayed in longest, then fewer hits taken, then the lower slot.
    pub fn standings(&self) -> Vec<usize> {
        let mut o: Vec<usize> = (0..self.balloons.len()).collect();
        o.sort_by(|a, b| {
            self.balloons[*b]
                .cmp(&self.balloons[*a])
                .then(self.out_at[*b].unwrap_or(u32::MAX).cmp(&self.out_at[*a].unwrap_or(u32::MAX)))
                .then(self.hits[*a].cmp(&self.hits[*b]))
                .then(a.cmp(b))
        });
        o
    }
    pub fn winner(&self) -> Option<usize> {
        self.over.map(|_| self.standings()[0])
    }
}

/// Steer a kart at a goal on the ground plane; `reverse` backs out of a corner.
pub fn seek_input(kart: &Kart, goal: [f64; 2], reverse: bool) -> Input {
    use std::f64::consts::PI;
    let (dx, dz) = (goal[0] - kart.pos[0], goal[1] - kart.pos[2]);
    let want = dx.atan2(dz);
    let mut diff = want - kart.yaw();
    while diff > PI {
        diff -= 2. * PI;
    }
    while diff < -PI {
        diff += 2. * PI;
    }
    let stick = (-diff * 120.).clamp(-64., 64.);
    if reverse {
        return Input { raw_x: (-stick * 80. / 64.).round() as i32, raw_y: -80, accel: false, brake: true, hop_down: false, hop_pressed: false };
    }
    let slow = diff.abs() > 1.1 && kart.speed > 3.;
    Input { raw_x: (stick * 80. / 64.).round() as i32, raw_y: if slow { 0 } else { 80 }, accel: !slow, brake: false, hop_down: false, hop_pressed: false }
}

/// What a CPU kart is after.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Goal {
    Wander([f64; 2]),
    Box([f64; 2]),
    Chase(usize),
}

/// The CPU brains: wander towards random spots, divert to a nearby item box, chase a kart that is close.
#[derive(Clone, Debug)]
pub struct Brains {
    rng: Rng,
    goals: Vec<Option<(Goal, u32)>>,
    stuck: Vec<u32>,
    reverse: Vec<u32>,
    arena_radius: f64,
}
impl Brains {
    pub fn new(seed: u32, cpus: usize, arena_radius: f64) -> Self {
        Self { rng: Rng(seed.max(1)), goals: vec![None; cpus], stuck: vec![0; cpus], reverse: vec![0; cpus], arena_radius }
    }
    /// Pads for each CPU. `karts` holds every racer's position (slot 0 the player), `out` who is out, `has_item` who
    /// holds an item already, `boxes` the live item boxes.
    pub fn pads(&mut self, cpus: &[&Kart], karts: &[[f64; 2]], out: &[bool], has_item: &[bool], boxes: &[[f64; 2]]) -> Vec<Option<Input>> {
        let mut pads = Vec::with_capacity(cpus.len());
        for (c, kart) in cpus.iter().enumerate() {
            let me = c + 1;
            if out.get(me).copied().unwrap_or(false) {
                pads.push(None);
                continue;
            }
            let pos = [kart.pos[0], kart.pos[2]];
            // Wedged against a wall: back out for a second.
            self.stuck[c] = if kart.speed < 0.3 { self.stuck[c] + 1 } else { 0 };
            if self.stuck[c] > 20 && self.reverse[c] == 0 {
                self.reverse[c] = 24;
                self.stuck[c] = 0;
                self.goals[c] = None;
            }
            // Re-plan every ~1.5 s, or when the target is reached.
            let replan = match self.goals[c] {
                None => true,
                Some((g, age)) => {
                    let at = self.goal_pos(g, karts);
                    age > 45 || at.is_some_and(|p| (p[0] - pos[0]).hypot(p[1] - pos[1]) < 40.)
                }
            };
            if replan {
                let goal = self.plan(me, pos, karts, out, has_item[me], boxes);
                self.goals[c] = Some((goal, 0));
            } else if let Some((g, age)) = self.goals[c] {
                self.goals[c] = Some((g, age + 1));
            }
            let (goal, _) = self.goals[c].unwrap();
            let target = self.goal_pos(goal, karts).unwrap_or(pos);
            let reverse = self.reverse[c] > 0;
            self.reverse[c] = self.reverse[c].saturating_sub(1);
            pads.push(Some(seek_input(kart, target, reverse)));
        }
        pads
    }
    fn goal_pos(&self, g: Goal, karts: &[[f64; 2]]) -> Option<[f64; 2]> {
        match g {
            Goal::Wander(p) | Goal::Box(p) => Some(p),
            Goal::Chase(i) => karts.get(i).copied(),
        }
    }
    fn plan(&mut self, me: usize, pos: [f64; 2], karts: &[[f64; 2]], out: &[bool], has_item: bool, boxes: &[[f64; 2]]) -> Goal {
        let d = |p: [f64; 2]| (p[0] - pos[0]).hypot(p[1] - pos[1]);
        // Without an item: the nearest box, if it is not far.
        if !has_item {
            if let Some(b) = boxes.iter().copied().min_by(|a, b| d(*a).total_cmp(&d(*b))) {
                if d(b) < self.arena_radius * 1.2 && self.rng.unit() < 0.8 {
                    return Goal::Box(b);
                }
            }
        }
        // With an item: hunt the nearest kart still in.
        let prey = (0..karts.len()).filter(|j| *j != me && !out.get(*j).copied().unwrap_or(false)).min_by(|a, b| d(karts[*a]).total_cmp(&d(karts[*b])));
        if let Some(j) = prey {
            if has_item || d(karts[j]) < self.arena_radius * 0.6 {
                return Goal::Chase(j);
            }
        }
        // Otherwise wander somewhere in the arena.
        let ang = self.rng.unit() * std::f64::consts::TAU;
        let r = self.rng.unit().sqrt() * self.arena_radius;
        Goal::Wander([r * ang.sin(), r * ang.cos()])
    }
}

/// A battle in progress: rules, brains and the arena, driving a [`KartRace`].
pub struct Battle {
    pub rules: Rules,
    pub brains: Brains,
    pub arena: Arena,
    pub events: Vec<Event>,
}
impl Battle {
    /// Put the race on the arena: start grid, item boxes, no laps, battle brains. `player` and the CPU karts are
    /// replaced on the grid. Call right after `KartRace::start`.
    pub fn setup(race: &mut KartRace, player: &mut Kart, arena: &Arena, ground: &dyn Ground, seed: u32) -> Self {
        let floor = |p: [f64; 3]| ground.floor(p[0], p[1] + 50., p[2]).map_or(p[1], |f| f.height);
        race.no_progress = true;
        race.benched = 0;
        let stats: Vec<_> = (0..race.racers.len()).map(|i| race.stats_for(i)).collect();
        let kart_at = |slot: usize| {
            let (p, rot) = arena.grid[slot % 4];
            let mut k = Kart::new([p[0], floor(p), p[2]], rot);
            k.set_stats(stats[slot]);
            k
        };
        *player = kart_at(0);
        race.racers[0].last = [player.pos[0], player.pos[2]];
        let karts: Vec<Kart> = (0..race.cpus.len()).map(|c| kart_at(c + 1)).collect();
        for (c, k) in karts.into_iter().enumerate() {
            race.racers[c + 1].last = [k.pos[0], k.pos[2]];
            race.cpus[c].kart = k;
        }
        // A few decomp spawns hang over nothing (Big Donut's two low ones): look for a floor below them, else drop the box.
        race.boxes = arena
            .boxes
            .iter()
            .filter_map(|b| {
                let f = ground.floor(b[0], b[1] + 50., b[2]).or_else(|| ground.floor(b[0], b[1] + 600., b[2]))?;
                Some(ItemBox { pos: [b[0], b[2]], y: f.height, respawn: 0 })
            })
            .collect();
        let n = race.racers.len();
        Self { rules: Rules::new(n), brains: Brains::new(seed, race.cpus.len(), arena.radius()), arena: arena.clone(), events: vec![] }
    }

    /// Before `KartRace::tick`: decide the CPU pads.
    pub fn before_tick(&mut self, race: &mut KartRace, player: &Kart) {
        let n = race.racers.len();
        let karts: Vec<[f64; 2]> = (0..n).map(|i| { let k = race.kart_pos(i, player); [k[0], k[2]] }).collect();
        let out: Vec<bool> = (0..n).map(|i| self.rules.is_out(i)).collect();
        let has_item: Vec<bool> = race.racers.iter().map(|r| r.item.is_some() || r.roulette > 0).collect();
        let boxes: Vec<[f64; 2]> = race.boxes.iter().filter(|b| b.respawn == 0).map(|b| b.pos).collect();
        let cpus: Vec<&Kart> = race.cpus.iter().map(|c| &c.kart).collect();
        let pads = self.brains.pads(&cpus, &karts, &out, &has_item, &boxes);
        race.cpu_override = pads;
    }

    /// After `KartRace::tick` with the events it produced: balloons, eliminations, the end of the battle. Keeps the
    /// race in the racing phase (the lap machinery never finishes a battle). A balloon pops when an item hits a kart
    /// (`Event::Smash`; the start-line spinout of an early throttle is only an `Event::Hit`).
    pub fn after_tick(&mut self, race: &mut KartRace, events: &[Event]) {
        if race.phase == Phase::Countdown {
            return;
        }
        self.rules.step();
        for e in events {
            if let Event::Smash(i, _) = e {
                if self.rules.pop(*i) {
                    self.bench(race, *i);
                }
            }
        }
        self.rules.end_if_player_out();
        race.phase = Phase::Racing;
        race.results = None;
        race.best_dirty = false;
        race.best_lap = None;
        race.best_total = None;
        self.events = events.to_vec();
    }

    /// Take a kart out of the game: it stops, cannot be hit, picks nothing up and is hidden by the view.
    fn bench(&self, race: &mut KartRace, i: usize) {
        race.benched |= 1 << i;
        race.racers[i].finish = Some(race.race_tick);
        race.racers[i].item = None;
        race.racers[i].roulette = 0;
        race.items.discard(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::kart_items::Item;
    use super::super::kart_physics::{Floor, Wall};

    struct Flat;
    impl Ground for Flat {
        fn floor(&self, _x: f64, _y: f64, _z: f64) -> Option<Floor> {
            Some(Floor { height: 0., normal: [0., 1., 0.], grass: false })
        }
        fn wall(&self, _pos: [f64; 3]) -> Option<Wall> {
            None
        }
    }
    fn arena() -> Arena {
        Arena {
            dir: "block_fort".into(),
            name: "Block Fort".into(),
            grid: vec![([0., 0., 200.], 32768), ([0., 0., -200.], 0), ([-200., 0., 0.], -16384), ([200., 0., 0.], 16384)],
            boxes: vec![[100., 0., 100.], [-100., 0., -100.]],
        }
    }
    fn ring(r: f64) -> Vec<[f64; 3]> {
        (0..32).map(|k| { let a = k as f64 / 32. * std::f64::consts::TAU; [r * a.sin(), 0., r * a.cos()] }).collect()
    }

    #[test]
    fn three_balloons_one_per_hit_last_standing_wins() {
        let mut r = Rules::new(4);
        assert_eq!(r.balloons, vec![3; 4]);
        for _ in 0..30 {
            r.step();
        }
        assert!(!r.pop(1));
        assert!(!r.pop(1));
        assert_eq!(r.balloons[1], 1);
        assert!(r.pop(1), "the third hit takes the last balloon");
        assert_eq!(r.out_at[1], Some(30));
        assert!(!r.pop(1), "a kart that is out cannot lose more");
        assert_eq!(r.alive(), 3);
        assert!(r.over.is_none());
        r.step();
        r.pop(2);
        r.pop(2);
        r.pop(2);
        r.step();
        r.pop(3);
        r.pop(3);
        assert!(r.over.is_none());
        r.pop(3);
        assert_eq!(r.over, Some(End::LastStanding));
        assert_eq!(r.winner(), Some(0));
        // Out later is better: 3 (out last) ahead of 2 ahead of 1.
        assert_eq!(r.standings(), vec![0, 3, 2, 1]);
        r.step();
        assert!(!r.pop(0), "nothing counts after the end");
    }

    #[test]
    fn time_up_ranks_by_balloons_then_hits() {
        let mut r = Rules::new(4);
        r.limit = 100;
        r.pop(0);
        r.pop(1);
        r.pop(1);
        r.pop(2);
        r.pop(3);
        r.pop(3);
        for _ in 0..100 {
            r.step();
        }
        assert_eq!(r.over, Some(End::TimeUp));
        assert_eq!(r.time_left(), 0);
        // 0 and 2 keep two balloons, 1 and 3 one; ties by hits taken then slot.
        assert_eq!(r.standings(), vec![0, 2, 1, 3]);
        assert_eq!(r.winner(), Some(0));
    }

    #[test]
    fn seeking_drives_at_the_goal_and_backs_out_when_asked() {
        let mut kart = Kart::new([0., 0., 0.], 0);
        let goal = [300., 300.];
        let d0 = (goal[0] - kart.pos[0]).hypot(goal[1] - kart.pos[2]);
        for _ in 0..120 {
            let pad = seek_input(&kart, goal, false);
            for _ in 0..2 {
                kart.step(&pad, &Flat);
            }
        }
        let d1 = (goal[0] - kart.pos[0]).hypot(goal[1] - kart.pos[2]);
        assert!(d1 < d0 * 0.4, "{d0} -> {d1}");
        let pad = seek_input(&kart, goal, true);
        assert!(pad.brake && pad.raw_y < 0 && !pad.accel);
    }

    #[test]
    fn battle_runs_on_the_race_machinery_cpus_move_and_hits_pop_balloons() {
        let a = arena();
        let mut race = KartRace::new(&ring(200.), 99);
        let mut player = Kart::new([0.; 3], 0);
        race.start(&mut player, &Flat);
        let mut battle = Battle::setup(&mut race, &mut player, &a, &Flat, 5);
        assert_eq!(race.boxes.len(), 2);
        assert!(race.no_progress);
        assert_eq!((player.pos[0], player.pos[2]), (0., 200.));
        let starts: Vec<[f64; 3]> = race.cpus.iter().map(|c| c.kart.pos).collect();
        let idle = Input::default();
        let mut last = starts.clone();
        let mut travelled = [0.; 3];
        for _ in 0..900 {
            battle.before_tick(&mut race, &player);
            race.tick(&idle, false, &mut player, &Flat);
            let ev = race.drain_events();
            battle.after_tick(&mut race, &ev);
            for (i, c) in race.cpus.iter().enumerate() {
                travelled[i] += (c.kart.pos[0] - last[i][0]).hypot(c.kart.pos[2] - last[i][2]);
                last[i] = c.kart.pos;
            }
        }
        assert_eq!(race.phase, Phase::Racing, "laps never end a battle");
        assert!(travelled.iter().all(|t| *t > 300.), "a CPU kart stood still ({travelled:?})");
        assert!(race.racers[0].laps.is_empty(), "no laps in a battle");
        // A scripted hit pops the balloon and a third takes the kart out.
        let before = battle.rules.balloons[2];
        battle.after_tick(&mut race, &[Event::Smash(2, Item::GreenShell)]);
        assert_eq!(battle.rules.balloons[2], before - 1);
        battle.after_tick(&mut race, &[Event::Smash(2, Item::GreenShell), Event::Smash(2, Item::GreenShell)]);
        assert!(battle.rules.is_out(2));
        assert_eq!(race.benched & 0b100, 0b100);
        battle.before_tick(&mut race, &player);
        assert!(race.cpu_override[1].is_none(), "a benched kart has no pad");
        let frozen = race.cpus[1].kart.pos;
        for _ in 0..90 {
            battle.before_tick(&mut race, &player);
            race.tick(&idle, false, &mut player, &Flat);
            let ev = race.drain_events();
            battle.after_tick(&mut race, &ev);
        }
        assert_eq!(race.cpus[1].kart.pos, frozen, "a benched kart is not stepped");
    }

    #[test]
    fn battles_are_deterministic() {
        let run = || {
            let a = arena();
            let mut race = KartRace::new(&ring(200.), 99);
            let mut player = Kart::new([0.; 3], 0);
            race.start(&mut player, &Flat);
            let mut battle = Battle::setup(&mut race, &mut player, &a, &Flat, 5);
            for _ in 0..600 {
                battle.before_tick(&mut race, &player);
                race.tick(&Input { raw_x: 20, raw_y: 80, accel: true, ..Default::default() }, false, &mut player, &Flat);
                let ev = race.drain_events();
                battle.after_tick(&mut race, &ev);
            }
            (race.cpus.iter().map(|c| c.kart.pos).collect::<Vec<_>>(), player.pos, battle.rules.balloons.clone())
        };
        assert_eq!(run(), run());
    }

    /// The real arenas: starts have floor under them, boxes sit on the floor, and the CPUs drive around in them.
    #[test]
    fn real_arenas_load_and_the_cpus_wander_without_being_wedged() {
        use super::super::kart::{Raceway, WorldGround};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../public/mk64");
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let arenas = load_arenas(&root);
        if arenas.is_empty() || !root.join("big-donut.json").exists() {
            return;
        }
        assert_eq!(arenas.len(), 4);
        let mut report = String::new();
        for arena in &arenas {
            let data = Raceway::read_course(&root, Some(&assets), &arena.dir).unwrap_or_else(|e| panic!("{}: {e}", arena.name));
            let world = data.world();
            let ground = WorldGround(&world);
            for (p, _) in &arena.grid {
                assert!(ground.floor(p[0], p[1] + 50., p[2]).is_some(), "{} start {p:?} has no floor", arena.name);
            }
            let mut race = KartRace::new(&data.path, 7);
            let mut player = Kart::new([0.; 3], 0);
            race.start(&mut player, &ground);
            let mut battle = Battle::setup(&mut race, &mut player, arena, &ground, 11);
            let on_floor = race.boxes.iter().filter(|b| ground.floor(b.pos[0], b.y + 50., b.pos[1]).is_some()).count();
            let off: Vec<_> = race.boxes.iter().filter(|b| ground.floor(b.pos[0], b.y + 50., b.pos[1]).is_none()).map(|b| (b.pos, b.y)).collect();
            assert_eq!(on_floor, race.boxes.len(), "{}: item boxes off the floor: {off:?}", arena.name);
            assert!(race.boxes.len() + 3 >= arena.boxes.len() && race.boxes.len() >= 20, "{}: too few boxes left", arena.name);
            let idle = Input::default();
            let mut travelled = [0.; 3];
            let mut last: Vec<[f64; 3]> = race.cpus.iter().map(|c| c.kart.pos).collect();
            for _ in 0..1800 {
                battle.before_tick(&mut race, &player);
                race.tick(&idle, false, &mut player, &ground);
                let ev = race.drain_events();
                battle.after_tick(&mut race, &ev);
                for (i, c) in race.cpus.iter().enumerate() {
                    travelled[i] += (c.kart.pos[0] - last[i][0]).hypot(c.kart.pos[2] - last[i][2]);
                    last[i] = c.kart.pos;
                }
            }
            let rescues: Vec<u32> = race.racers[1..].iter().map(|r| r.rescues).collect();
            report.push_str(&format!("{:<12} boxes {:>2} travelled {:?} rescues {rescues:?} balloons {:?}\n", arena.name, arena.boxes.len(), travelled.map(|t| t.round()), battle.rules.balloons));
            assert!(travelled.iter().all(|t| *t > 400.), "{}: a CPU barely moved in 60 s: {travelled:?}", arena.name);
        }
        println!("{report}");
    }
}
