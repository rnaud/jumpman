//! A Hundred Marios (`hundred.ts`): a crowd of full-physics, fully animated Marios with five
//! colour-coded personalities flooding the course around the player.
//!
//! Keys instead of the DOM dock: `1`-`3` crowd size, `M` mix, `[`/`]` sim speed, `N` select the
//! next Mario, `Tab` watch it, `P` change its personality. No mouse picking yet (`pick` is
//! ported and tested; it needs a camera-projection hook in `Host`).
//!
//! The game (Bob-omb Battlefield): a race to the summit against the crowd. Every start (and `R`) puts the player and the
//! whole crowd back at the foot of the mountain together; the place is one plus the crowd Marios who made the summit
//! first. Best place and best time are kept; the medal is for the time (`TIME_PARS`).
use super::crowd::init_clips;
use super::mario_crowd::*;
use super::dock::DockItem;
use super::race::at_finish;
use super::rail_path::RailPath;
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::camera::rig::camera_ray;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::behaviors::koopa_the_quick::KOOPA_RACE_ROUTE;
use jumpman_sim::math::angle_to_radians;
use jumpman_sim::state::ControllerSnapshot;

const COUNTS: [usize; 3] = [25, 50, 100];
const MIXES: [CrowdMix; 5] = [CrowdMix::Mixed, CrowdMix::Race, CrowdMix::Gold, CrowdMix::Parade, CrowdMix::Brawl];
const SPEEDS: [f64; 4] = [0.5, 1.0, 2.0, 3.0];

pub struct HundredMariosExperiment {
    crowd: Option<MarioCrowd>,
    watching: Option<u32>,
    watch_pos: DVec3,
    watch_look: DVec3,
    count: usize,
    speed_index: usize,
    refresh: f64,
    /// Goal (Bob-omb Battlefield): get yourself up the mountain through the crowd.
    rail: Option<RailPath>,
    ticks: u32,
    /// (ticks, Marios that had already made a summit when you did)
    summit: Option<(u32, u32)>,
    /// The crowd's summit count when this race started (the crowd keeps lapping).
    summits_at_start: u32,
    /// Best place and best time, kept between sessions; and what this finish was worth.
    best_place: Option<f64>,
    best_time: Option<f64>,
    verdict: String,
}

/// Summit times (seconds) for gold, silver and bronze, measured (`bob_audit`): a scripted climb of the route takes 130 s
/// (100 s without its warps); the crowd's fastest racers summit at 140-170 s since they jump at ledges (they needed
/// about 220 s before), so silver and bronze are times the leading pack also makes.
pub const TIME_PARS: [f64; 3] = [100.0, 135.0, 190.0];

/// Where the player finished: one plus the crowd Marios who summited since the start.
pub fn race_place(summits_now: u32, summits_at_start: u32) -> u32 {
    summits_now.saturating_sub(summits_at_start) + 1
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

impl HundredMariosExperiment {
    pub fn new() -> Self {
        Self { crowd: None, watching: None, watch_pos: DVec3::ZERO, watch_look: DVec3::ZERO, count: 100, speed_index: 1, refresh: 0.0, rail: None, ticks: 0, summit: None, summits_at_start: 0, best_place: None, best_time: None, verdict: String::new() }
    }

    fn select_next(&mut self) {
        let Some(crowd) = self.crowd.as_mut() else { return };
        if crowd.agents.is_empty() {
            return;
        }
        let at = crowd.selected.and_then(|id| crowd.agents.iter().position(|a| a.id == id));
        let next = at.map_or(0, |i| (i + 1) % crowd.agents.len());
        crowd.selected = Some(crowd.agents[next].id);
    }

    fn watch_camera(&mut self, host: &mut Host, id: u32, dt: f64) {
        let crowd = self.crowd.as_ref().unwrap();
        let Some(agent) = crowd.agents.iter().find(|a| a.id == id) else { return };
        let (target, _) = agent.body.interpolated(host.alpha);
        let yaw = angle_to_radians(agent.body.runtime.face_angle[1]);
        let mut desired = DVec3::new(target[0] - yaw.sin() * 900.0, target[1] + 420.0, target[2] - yaw.cos() * 900.0);
        // Keep the chase camera out of walls and hills.
        let head = DVec3::new(target[0], target[1] + 120.0, target[2]);
        let toward = desired - head;
        let length = toward.length();
        if length > 1.0 {
            if let Some(hit) = camera_ray(crowd.world(), head, toward / length, length) {
                desired = head + toward / length * (hit - 60.0).max(120.0);
            }
        }
        let k = 1.0 - (-dt * 3.5).exp();
        if self.watch_pos == DVec3::ZERO || self.watch_pos.distance(desired) > 4000.0 {
            self.watch_pos = desired;
            self.watch_look = DVec3::new(target[0], target[1] + 110.0, target[2]);
        }
        self.watch_pos = self.watch_pos.lerp(desired, k);
        self.watch_look = self.watch_look.lerp(DVec3::new(target[0], target[1] + 110.0, target[2]), (k * 2.5).min(1.0));
        host.cam.pose = Some(CamPose { eye: self.watch_pos, look: self.watch_look, up: DVec3::Y, fov: 50.0, near: 5.0, far: 40000.0 });
    }

    fn key(&mut self, host: &mut Host, key: KeyCode) -> bool {
        let Some(crowd) = self.crowd.as_mut() else { return false };
        match key {
            KeyCode::Tab => {
                if self.watching.is_some() {
                    self.watching = None;
                    host.cam.pose = None;
                } else {
                    self.watching = crowd.selected.or_else(|| crowd.agents.first().map(|a| a.id));
                    crowd.selected = self.watching;
                }
                host.toast(if self.watching.is_some() { "WATCHING - TAB TO RETURN" } else { "BACK TO MARIO" }, 1.6);
                true
            }
            KeyCode::KeyN => {
                self.select_next();
                true
            }
            KeyCode::KeyP => {
                if let Some(i) = crowd.selected.and_then(|id| crowd.agents.iter().position(|a| a.id == id)) {
                    let next = crowd.cycle_personality(i, Some(host));
                    host.toast(&format!("NOW A {}", next.info().label.to_uppercase()), 1.6);
                }
                true
            }
            KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 => {
                let n = COUNTS[match key {
                    KeyCode::Digit1 => 0,
                    KeyCode::Digit2 => 1,
                    _ => 2,
                }];
                self.count = n;
                let pos = host.sim.mario.pos;
                crowd.populate(n, pos, 1600.0, Some(host));
                host.toast(&format!("{n} MARIOS INCOMING"), 1.6);
                true
            }
            KeyCode::KeyM => {
                let i = MIXES.iter().position(|m| *m == crowd.mix).unwrap_or(0);
                crowd.set_mix(MIXES[(i + 1) % MIXES.len()], Some(host));
                true
            }
            KeyCode::BracketLeft | KeyCode::BracketRight => {
                self.speed_index = if key == KeyCode::BracketRight { (self.speed_index + 1).min(SPEEDS.len() - 1) } else { self.speed_index.saturating_sub(1) };
                crowd.speed = SPEEDS[self.speed_index];
                true
            }
            _ => false,
        }
    }

    fn dock(&self, host: &mut Host) {
        let Some(crowd) = self.crowd.as_ref() else { return };
        let mut items = vec![];
        for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3].into_iter().enumerate() {
            let mut it = DockItem::new(&COUNTS[i].to_string(), k, 0);
            it.selected = self.count == COUNTS[i];
            items.push(it);
        }
        items.push(DockItem::new(&format!("Mix: {}", crowd.mix.label()), KeyCode::KeyM, 1));
        items.push(DockItem::new("Slower", KeyCode::BracketLeft, 2));
        items.push(DockItem::new(&format!("{}x", crowd.speed), KeyCode::BracketRight, 2));
        items.push(DockItem::new("Next", KeyCode::KeyN, 3).essential().primary());
        let mut watch = DockItem::new("Watch", KeyCode::Tab, 3).essential();
        watch.selected = self.watching.is_some();
        items.push(watch);
        items.push(DockItem::new("Personality", KeyCode::KeyP, 3));
        items.push(DockItem::new("Start over", KeyCode::KeyR, 4));
        host.set_dock(items);
    }

    /// Back to the foot of the mountain with the whole crowd: a fresh race.
    fn restart(&mut self, host: &mut Host) {
        let spawn = host.sim.course.spawn.clone();
        host.place_mario(spawn.pos, Some(spawn.face_yaw));
        if let Some(crowd) = self.crowd.as_mut() {
            crowd.populate(self.count, spawn.pos, 1600.0, Some(host));
            self.summits_at_start = crowd.stats.summits;
        }
        self.summit = None;
        self.ticks = 0;
        self.verdict.clear();
        host.toast("RACE THE CROWD TO THE SUMMIT - GO!", 1.8);
    }

    /// Goal: the player reaches the end of the mountain route.
    fn track_summit(&mut self, host: &mut Host) {
        let (Some(rail), Some(crowd)) = (self.rail.as_ref(), self.crowd.as_ref()) else { return };
        if self.summit.is_some() {
            return;
        }
        self.ticks += 1;
        let pos = host.sim.mario.pos;
        let hit = rail.nearest(&pos, None, 40);
        if at_finish(rail.total, &hit, &pos) {
            let place = race_place(crowd.stats.summits, self.summits_at_start);
            self.summit = Some((self.ticks, place));
            let secs = self.ticks as f64 / 30.0;
            let (_, new_place) = super::records::submit("hundred.place", place as f64, true);
            let (before_t, new_time) = super::records::submit("hundred.time", secs, true);
            self.best_place = super::records::best("hundred.place");
            self.best_time = super::records::best("hundred.time");
            let medal = super::records::medal_for_time(secs, TIME_PARS);
            self.verdict = format!("{}{}", medal.map_or(String::new(), |m| format!("{} - ", m.label())), if new_place { "NEW BEST PLACE".to_string() } else { super::records::verdict(before_t, new_time, format_lap) });
            host.toast(&format!("SUMMIT - {} OF {}!", ordinal(place).to_uppercase(), crowd.agents.len() + 1), 2.8);
            host.fx("star_sound", pos);
        }
    }

    fn panel(&self, host: &mut Host) {
        let Some(crowd) = self.crowd.as_ref() else { return };
        let counts = crowd.counts();
        let mut legend = String::new();
        for p in PERSONALITY_ORDER {
            let n = counts.get(&p).copied().unwrap_or(0);
            if n > 0 || crowd.mix == CrowdMix::Mixed {
                legend.push_str(&format!("{} {}   ", p.info().label, n));
            }
        }
        let s = &crowd.stats;
        let mut line = format!(
            "{}x {} | {} | speed {}x | summits {} best {} coins {} knockbacks {}",
            crowd.agents.len(),
            crowd.mix.label(),
            legend.trim_end(),
            crowd.speed,
            s.summits,
            s.best_lap.map_or("-".into(), format_lap),
            s.coins,
            s.hits
        );
        if let Some(a) = crowd.selected.and_then(|id| crowd.agents.iter().find(|a| a.id == id)) {
            let d = crowd.describe(a);
            line.push_str(&format!(
                "\n> {} [{}] {} | {} {:.0} u/f | coins {}{}",
                d.name,
                d.personality.info().label,
                d.label,
                d.action.to_lowercase().replace('_', " "),
                d.speed.abs(),
                d.coins,
                if self.watching == Some(a.id) { " | WATCHING" } else { "" }
            ));
        }
        if self.rail.is_some() {
            match self.summit {
                Some((t, place)) => line.push_str(&format!("\nSUMMIT - {} place in {} - {} - {} to race again", ordinal(place), format_lap(t as f64 / 30.0), self.verdict, host.prompt("R", "tap START OVER", "pick START OVER on the Select menu"))),
                None => {
                    let ahead = crowd.stats.summits.saturating_sub(self.summits_at_start);
                    let best = match (self.best_place, self.best_time) {
                        (Some(p), Some(t)) => format!(" - best {} / {}", ordinal(p as u32), format_lap(t)),
                        _ => String::new(),
                    };
                    line.push_str(&format!("\nRACE: beat the crowd to the summit - {} - {} already up{best}", format_lap(self.ticks as f64 / 30.0), ahead));
                }
            }
        }
        host.status(line);
    }
}

fn format_lap(seconds: f64) -> String {
    let m = (seconds / 60.0).floor();
    format!("{}:{:04.1}", m, seconds - m * 60.0)
}

impl Experiment for HundredMariosExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Hundred
    }
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }

    fn enter(&mut self, host: &mut Host) {
        host.card(
            "04",
            "A Hundred Marios",
            "Race a hundred real-physics Marios up Bob-omb's mountain. Colour shows each one's personality.",
            &[("Goal", "beat the crowd to the summit: gold under 1:40, silver 2:15, bronze 3:10"), ("R", "start a new race"),("N", "select next Mario"), ("Tab", "watch the selected Mario"), ("P", "change its personality"), ("1 2 3", "25 / 50 / 100 Marios"), ("M", "mix"), ("[ ]", "sim speed"), ("J / Shift", "punch the crowd"), ("X", "leave")],
        );
        let meta = init_clips(host);
        let on_battlefield = host.world_name() == "bob";
        let pos = host.sim.mario.pos;
        let opts = MarioCrowdOptions {
            world: host.course_world(),
            route: on_battlefield.then(|| KOOPA_RACE_ROUTE.to_vec()),
            has_leader: true,
            coin_count: 40,
            center: if on_battlefield { [0.0; 3] } else { pos },
            radius: if on_battlefield { 6800.0 } else { 4000.0 },
            seed: 0x5eed,
            single_lap: false,
        };
        let mut crowd = MarioCrowd::new(opts, meta, Some(host));
        crowd.populate(self.count, pos, 1600.0, Some(host));
        if on_battlefield {
            self.rail = crowd.route.as_ref().map(|r| RailPath::new(r));
            self.summits_at_start = crowd.stats.summits;
            self.best_place = super::records::best("hundred.place");
            self.best_time = super::records::best("hundred.time");
        }
        self.crowd = Some(crowd);
        self.dock(host);
        host.toast(if self.rail.is_some() { "RACE A HUNDRED MARIOS TO THE SUMMIT" } else { "A HUNDRED MARIOS - N TO INSPECT ONE" }, 3.2);
        self.panel(host);
    }

    fn exit(&mut self, host: &mut Host) {
        if let Some(mut c) = self.crowd.take() {
            c.dispose(host);
        }
        self.watching = None;
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if key == KeyCode::KeyR && self.rail.is_some() {
            self.restart(host);
            self.dock(host);
            return true;
        }
        let handled = self.key(host, key);
        self.dock(host);
        handled
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        // While spectating, the player stands still instead of running off-screen.
        if self.watching.is_some() {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        self.track_summit(host);
        if let Some(c) = self.crowd.as_mut() {
            c.update(Some(&mut host.sim.mario));
            let fx = c.take_fx();
            host.play_fx(fx);
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        if self.crowd.is_none() {
            return;
        }
        if let Some(id) = self.watching {
            if self.crowd.as_ref().unwrap().agents.iter().any(|a| a.id == id) {
                self.watch_camera(host, id, dt);
            } else {
                self.watching = None;
                host.cam.pose = None;
            }
        }
        let eye = host.rig.eye;
        let alpha = host.alpha;
        let crowd = self.crowd.as_mut().unwrap();
        let mouse = host.mouse;
        if mouse.left_just_pressed && !mouse.over_ui {
            if let Some(cursor) = mouse.cursor {
                crowd.selected = crowd.pick(cursor, 48.0, alpha, |p| mouse.project(p));
                self.refresh = 0.0;
            }
        }
        crowd.render(host, alpha, dt, [eye.x, eye.y, eye.z]);
        self.refresh -= dt;
        if self.refresh <= 0.0 {
            self.refresh = 0.15;
            self.panel(host);
        }
    }

    fn debug(&self) -> String {
        match &self.crowd {
            None => "no crowd".into(),
            Some(c) => format!("count={} mix={} speed={} stats={:?} watching={:?}", c.agents.len(), c.mix.label(), c.speed, c.stats, self.watching),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_place_counts_only_the_summits_of_this_race() {
        assert_eq!(race_place(40, 40), 1);
        assert_eq!(race_place(43, 40), 4);
        assert_eq!(race_place(3, 7), 1, "a counter that went backwards (a fresh crowd) never gives place 0");
        assert_eq!([ordinal(1), ordinal(2), ordinal(3), ordinal(11), ordinal(22), ordinal(113)], ["1st", "2nd", "3rd", "11th", "22nd", "113th"]);
        assert!(TIME_PARS[0] < TIME_PARS[1] && TIME_PARS[1] < TIME_PARS[2]);
    }
}

/// Measurements on the real Bob-omb Battlefield for the audit (`MINIGAMES_AUDIT_3B.md`): how long a scripted, clean
/// climb of the mountain route takes (the "good player" par of A Hundred Marios, Across Dimensions and Shrink Ray x1),
/// and how long the crowd's racers need. Skipped without the imported course.
#[cfg(test)]
pub(crate) mod bob_audit {
    use super::*;
    use crate::experiments::crowd_route::mario_route;
    use crate::experiments::god::test_bot::Bot;
    use crate::game::course_data::CourseData;
    use crate::game::native_collision::add_native_collision;
    use jumpman_sim::collision::CollisionWorld;
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
    use jumpman_sim::state::*;
    use std::sync::Arc;

    pub fn bob() -> Option<(Arc<CollisionWorld>, [f64; 3], f64)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let course = CourseData::load(&root, "bob", 1).ok()?;
        let mut world = CollisionWorld::default();
        add_native_collision(&mut world, &course.collision, [0.0; 3], 0.0);
        world.finalize();
        Some((Arc::new(world), course.spawn.pos, course.spawn.face_yaw))
    }

    /// A scripted climb: full stick at the next route point, jump at ledges and walls, warp onto a point after 6 s
    /// without progress (counted). Returns (ticks to the finish, warps) or None after `max` ticks.
    pub fn scripted_climb(world: &Arc<CollisionWorld>, route: &[[f64; 3]], start: [f64; 3], max: u32, finish: &dyn Fn([f64; 3]) -> bool) -> Option<(u32, u32)> {
        let mut m = MarioRuntime::new(world.clone(), SpawnPoint { action: None, pos: [start[0], start[1] + 100.0, start[2]], face_yaw: 0.0 }, -8000.0);
        let mut bot = Bot { max_hold: 14, ..Default::default() };
        let mut k = 1usize;
        let (mut best, mut idle, mut warps) = (f64::INFINITY, 0u32, 0u32);
        for tick in 0..max {
            let target = route[k.min(route.len() - 1)];
            let (dx, dz) = (target[0] - m.pos[0], target[2] - m.pos[2]);
            if dx.hypot(dz) < 240.0 && (target[1] - m.pos[1]).abs() < 450.0 && k + 1 < route.len() {
                k += 1;
                best = f64::INFINITY;
                idle = 0;
                continue;
            }
            let d = dx.hypot(dz) + (target[1] - m.pos[1]).abs() * 0.5;
            if d < best - 60.0 {
                best = d;
                idle = 0;
            } else {
                idle += 1;
                if idle > 180 {
                    m.pos = [target[0], target[1] + 150.0, target[2]];
                    m.vel = [0.0; 3];
                    m.set_action(ACT_FREEFALL, 0.0);
                    warps += 1;
                    idle = 0;
                    best = f64::INFINITY;
                }
            }
            let heading = Bot::heading_to(&m, target);
            let grounded = m.action & ACT_FLAG_AIR == 0;
            let jump = grounded && (Bot::edge_ahead(&m, heading) || (m.wall.is_some() && m.forward_vel < 12.0) || target[1] > m.pos[1] + 120.0 && dx.hypot(dz) < 500.0);
            bot.step(&mut m, heading, jump);
            if finish(m.pos) {
                return Some((tick + 1, warps));
            }
        }
        None
    }

    /// The pars are measured, not guessed: the scripted climb makes silver in both Across Dimensions and this race,
    /// and at least three crowd racers summit inside bronze (rivals, not scenery).
    #[test]
    fn pars_hold_for_the_scripted_climb_and_the_crowd_leads() {
        let Some((world, spawn, _)) = bob() else { return eprintln!("no bob data; skipped") };
        let route = mario_route(&world, &KOOPA_RACE_ROUTE);
        let rail = RailPath::new(&route);
        let at_top = |p: [f64; 3]| crate::experiments::race::at_finish(rail.total, &rail.nearest(&p, None, 40), &p);
        let (t, _) = scripted_climb(&world, &route, spawn, 30 * 400, &at_top).expect("the scripted climb reaches the summit");
        let secs = t as f64 / 30.0;
        assert!(secs <= TIME_PARS[1] && secs <= crate::experiments::dimensions::SUMMIT_PARS[1], "scripted climb {secs:.1} s");
        let opts = MarioCrowdOptions { world: world.clone(), route: Some(KOOPA_RACE_ROUTE.to_vec()), has_leader: false, coin_count: 40, center: [0.0; 3], radius: 6800.0, seed: 0x5eed, single_lap: true };
        let mut c = MarioCrowd::new(opts, None, None);
        c.populate(100, spawn, 1600.0, None);
        for _ in 0..(30.0 * TIME_PARS[2]) as u32 {
            c.update(None);
        }
        assert!(c.stats.summits >= 3, "only {} crowd racers inside bronze", c.stats.summits);
    }

    /// Prints the numbers (`--nocapture`); run with the imported course.
    #[test]
    #[ignore]
    fn audit_bob_route_times() {
        let Some((world, spawn, _)) = bob() else { return eprintln!("no bob data; skipped") };
        let route = mario_route(&world, &KOOPA_RACE_ROUTE);
        let rail = RailPath::new(&route);
        let at_top = |p: [f64; 3]| crate::experiments::race::at_finish(rail.total, &rail.nearest(&p, None, 40), &p);
        match scripted_climb(&world, &route, route[0], 30 * 400, &at_top) {
            Some((t, w)) => eprintln!("SCRIPTED CLIMB from route start: {:.1} s, {w} warps", t as f64 / 30.0),
            None => eprintln!("SCRIPTED CLIMB: did not finish"),
        }
        match scripted_climb(&world, &route, spawn, 30 * 400, &at_top) {
            Some((t, w)) => eprintln!("SCRIPTED CLIMB from course spawn: {:.1} s, {w} warps", t as f64 / 30.0),
            None => eprintln!("SCRIPTED CLIMB from spawn: did not finish"),
        }
        // Shrink Ray: the same climb in the rescaled worlds (Mario's own jump is unchanged).
        use crate::experiments::shrink::{from_scaled, scaled_world, to_scaled, SIZES};
        for (_, label, k) in SIZES {
            let w = Arc::new(scaled_world(&world, k));
            let r: Vec<[f64; 3]> = route.iter().map(|p| to_scaled(*p, k)).collect();
            let top = |p: [f64; 3]| at_top(from_scaled(p, k));
            match scripted_climb(&w, &r, to_scaled(spawn, k), 30 * 900, &top) {
                Some((t, wp)) => eprintln!("SHRINK {label} x{k}: scripted climb {:.1} s, {wp} warps", t as f64 / 30.0),
                None => eprintln!("SHRINK {label} x{k}: no finish in 900 s"),
            }
        }
        // The crowd: one racer alone, then the full race mix of 100 from the foot.
        for (label, count) in [("solo racer", 1usize), ("100 mixed", 100)] {
            let opts = MarioCrowdOptions { world: world.clone(), route: Some(KOOPA_RACE_ROUTE.to_vec()), has_leader: false, coin_count: 40, center: [0.0; 3], radius: 6800.0, seed: 0x5eed, single_lap: true };
            let mut c = MarioCrowd::new(opts, None, None);
            if count == 1 {
                c.populate_at(&[spawn], Personality::Racer, 0.0, None);
            } else {
                c.populate(count, spawn, 1600.0, None);
            }
            let racers = c.agents.iter().filter(|a| a.personality == Personality::Racer).count();
            let mut times = vec![];
            let mut last = 0;
            for tick in 0..30 * 330 {
                c.update(None);
                while c.stats.summits > last {
                    last += 1;
                    times.push(tick as f64 / 30.0);
                }
            }
            eprintln!("CROWD {label}: {racers} racers, summits at {:?} s, knockbacks {}", times.iter().take(8).map(|t| t.round()).collect::<Vec<_>>(), c.stats.hits);
        }
    }
}
