//! Summit Race (`race.ts`): you against seven full-physics rivals up Bob-omb Battlefield's race
//! route. The route is the one Mario can actually run (re-planned from Koopa the Quick's path);
//! rivals that fall far behind get a gentle catch-up.
use super::crowd::init_clips;
use super::mario_crowd::*;
use super::rail_path::{RailHit, RailPath};
use super::{store, Experiment, ExperimentId, Host, StepOverride};
use crate::render::SCALE;
use bevy::prelude::*;
use jumpman_sim::behaviors::koopa_the_quick::KOOPA_RACE_ROUTE;
use jumpman_sim::math::{atan2s, Vec3 as V3};
use jumpman_sim::state::ControllerSnapshot;

const RIVALS: usize = 7;
const COUNTDOWN: u32 = 3 * 30 + 15;
const BEST_KEY: &str = "jumpman.race.best";
const ORDINALS: [&str; 8] = ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th"];
const RIVAL_COLORS: [u32; 7] = [0x17b3a6, 0xf5c518, 0x1fa3e0, 0x8c3fd9, 0xff6fb5, 0xff8a1f, 0xeeeeee];
const RIVAL_NAMES: [&str; 7] = ["Acqua", "Oro", "Azul", "Viola", "Rosa", "Arancia", "Bianco"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RaceState {
    Grid,
    Racing,
    Finished,
}

struct Runner {
    name: String,
    player: bool,
    /// Agent id for rivals.
    agent: Option<u32>,
    hit: Option<RailHit>,
    finish_ticks: Option<u32>,
}

pub struct SummitRaceExperiment {
    crowd: Option<MarioCrowd>,
    rail: Option<RailPath>,
    runners: Vec<Runner>,
    state: RaceState,
    ticks: u32,
    race_ticks: u32,
    best: f64,
    guide: Vec<Entity>,
    result: String,
}

pub fn format_time(seconds: f64) -> String {
    let m = (seconds / 60.0).floor();
    format!("{}:{:04.1}", m, seconds - m * 60.0)
}

/// Rank by finish time, then by progress along the rail. Returns indices best first.
fn standings(finish: &[Option<u32>], progress: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..finish.len()).collect();
    order.sort_by(|&a, &b| match (finish[a], finish[b]) {
        (None, None) => progress[b].partial_cmp(&progress[a]).unwrap(),
        (fa, fb) => fa.unwrap_or(u32::MAX).cmp(&fb.unwrap_or(u32::MAX)),
    });
    order
}

impl SummitRaceExperiment {
    pub fn new() -> Self {
        Self { crowd: None, rail: None, runners: vec![], state: RaceState::Grid, ticks: 0, race_ticks: 0, best: f64::INFINITY, guide: vec![], result: String::new() }
    }

    /// Line everyone up on a starting grid at the foot of the route.
    fn restart(&mut self, host: &mut Host) {
        let (Some(crowd), Some(rail)) = (self.crowd.as_mut(), self.rail.as_ref()) else { return };
        let start = rail.at(250.0);
        let [tx, tz] = start.tangent;
        let yaw = atan2s(tz, tx) as f64;
        let world = host.course_world();
        let mut spots: Vec<V3> = vec![];
        for i in 0..RIVALS + 1 {
            let (row, col) = ((i / 4) as f64, (i % 4) as f64 - 1.5);
            let x = start.point[0] - tx * row * 220.0 - tz * col * 170.0;
            let z = start.point[2] - tz * row * 220.0 + tx * col * 170.0;
            let floor = world.find_floor(x, start.point[1] + 400.0, z);
            spots.push([x, if floor.surface.is_some() { floor.height } else { start.point[1] }, z]);
        }
        let player_spot = spots.remove(1);
        host.place_mario(player_spot, Some(yaw));
        self.state = RaceState::Grid;
        self.ticks = 0;
        self.race_ticks = 0;
        self.result.clear();
        crowd.populate_at(&spots, Personality::Racer, yaw, Some(&mut *host));
        let mut runners = vec![Runner { name: "You".into(), player: true, agent: None, hit: None, finish_ticks: None }];
        for (i, a) in crowd.agents.iter_mut().enumerate() {
            a.name = RIVAL_NAMES.get(i).copied().unwrap_or("Rival").into();
            // Cap colour per rival.
            if let Some(e) = a.body.entity {
                host.commands.entity(e).insert((super::crowd::CrowdColors(super::crowd::CrowdLook { cap: RIVAL_COLORS[i % RIVAL_COLORS.len()], overalls: None }), super::crowd::NeedsTint));
            }
            runners.push(Runner { name: a.name.clone(), player: false, agent: Some(a.id), hit: None, finish_ticks: None });
        }
        self.runners = runners;
        host.toast("ON YOUR MARKS...", 1.2);
    }

    fn position_of(&self, host: &Host, runner: &Runner) -> V3 {
        match runner.agent {
            None => host.sim.mario.pos,
            Some(id) => self.crowd.as_ref().and_then(|c| c.agents.iter().find(|a| a.id == id)).map_or([0.0; 3], |a| a.body.pos()),
        }
    }

    /// Rivals far behind the player get nudged up the route so the race stays close.
    fn catch_up(&mut self) {
        let (Some(crowd), Some(rail)) = (self.crowd.as_mut(), self.rail.as_ref()) else { return };
        let Some(player_s) = self.runners[0].hit.map(|h| h.s) else { return };
        if self.state != RaceState::Racing {
            return;
        }
        for r in self.runners.iter_mut().skip(1) {
            let (Some(hit), None, Some(id)) = (r.hit, r.finish_ticks, r.agent) else { continue };
            if player_s - hit.s > 7000.0 && self.race_ticks % 90 == 0 {
                let ahead = rail.at(hit.s + 2500.0);
                let route = crowd.route.clone().unwrap_or_default();
                let mut best = 0;
                let mut distance = f64::INFINITY;
                for (i, p) in route.iter().enumerate() {
                    let d = (p[0] - ahead.point[0]).hypot(p[2] - ahead.point[2]) + (p[1] - ahead.point[1]).abs();
                    if d < distance {
                        distance = d;
                        best = i;
                    }
                }
                if let Some(a) = crowd.agents.iter_mut().find(|a| a.id == id) {
                    a.body.teleport([ahead.point[0], ahead.point[1] + 200.0, ahead.point[2]], None);
                    a.waypoint = (route.len() - 1).min(best + 1);
                }
                r.hit = None;
            }
        }
    }

    fn finish(&mut self, host: &mut Host) {
        self.state = RaceState::Finished;
        let finish: Vec<_> = self.runners.iter().map(|r| r.finish_ticks).collect();
        let progress: Vec<_> = self.runners.iter().map(|r| r.hit.map_or(0.0, |h| h.s)).collect();
        let place = standings(&finish, &progress).iter().position(|&i| self.runners[i].player).unwrap_or(0);
        let seconds = self.race_ticks as f64 / 30.0;
        let record = seconds < self.best;
        if record {
            self.best = seconds;
            store::set(BEST_KEY, serde_json::json!(seconds));
        }
        let head = if place == 0 { "YOU WIN!".to_string() } else { format!("{} PLACE", ORDINALS[place].to_uppercase()) };
        self.result = format!("{head}  {}  {}  -  R to race again", format_time(seconds), if record { "NEW BEST TIME!".into() } else { format!("best {}", format_time(self.best)) });
        host.toast(&head, 2.5);
    }

    fn update_guide(&self, host: &mut Host, hit: RailHit) {
        let rail = self.rail.as_ref().unwrap();
        let world = host.course_world();
        let render_y = host.render_pos()[1];
        let t = host.time();
        let mut n = 0;
        for i in 1..=self.guide.len() {
            let s = hit.s + i as f64 * 160.0;
            if s > rail.total {
                break;
            }
            let p = rail.at(s).point;
            let floor = world.find_floor(p[0], p[1].max(render_y) + 600.0, p[2]);
            if floor.surface.is_none() {
                continue;
            }
            let pulse = 1.0 + 0.3 * (t * 6.0 - i as f64 * 0.7).sin();
            let tf = Transform::from_xyz(p[0] as f32 * SCALE, (floor.height as f32 + 16.0) * SCALE, p[2] as f32 * SCALE).with_scale(Vec3::splat(pulse as f32));
            host.commands.entity(self.guide[n]).insert((tf, Visibility::Inherited));
            n += 1;
        }
        for e in &self.guide[n..] {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
    }
}

impl Experiment for SummitRaceExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Race
    }
    fn course(&self) -> Option<&'static str> {
        Some("bob")
    }

    fn enter(&mut self, host: &mut Host) {
        self.best = store::get_f64(BEST_KEY).filter(|b| *b > 0.0).unwrap_or(f64::INFINITY);
        host.card(
            "06",
            "Summit Race",
            "Beat seven rivals to the top of the mountain. Follow the gold dots - or find a shortcut.",
            &[("WASD Space", "run / jump"), ("Gold dots", "the race route"), ("R", "race again"), ("X", "leave")],
        );
        let mesh = host.meshes.add(Sphere::new(13.0 * SCALE));
        let mat = host.unlit(Color::srgb(1.0, 0.82, 0.29));
        for _ in 0..24 {
            let e = host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden));
            self.guide.push(e);
        }
        let meta = init_clips(host);
        let opts = MarioCrowdOptions { world: host.course_world(), route: Some(KOOPA_RACE_ROUTE.to_vec()), has_leader: false, coin_count: 0, center: [0.0; 3], radius: 6000.0, seed: 0x5eed, single_lap: true };
        let crowd = MarioCrowd::new(opts, meta, Some(&mut *host));
        self.rail = Some(RailPath::new(crowd.route.as_ref().unwrap()));
        self.crowd = Some(crowd);
        self.restart(host);
    }

    fn exit(&mut self, host: &mut Host) {
        if let Some(mut c) = self.crowd.take() {
            c.dispose(host);
        }
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if key == KeyCode::KeyR {
            self.restart(host);
            return true;
        }
        false
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        if self.state == RaceState::Grid {
            return StepOverride { controller: Some(ControllerSnapshot::default()), camera_yaw: None };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        if self.crowd.is_none() || self.rail.is_none() {
            return;
        }
        self.ticks += 1;
        if self.state == RaceState::Grid {
            let left = COUNTDOWN as i64 - self.ticks as i64;
            if left % 30 == 0 && left > 0 {
                host.toast(&(left / 30).to_string(), 0.7);
            }
            if left <= 0 {
                self.state = RaceState::Racing;
                host.toast("GO!", 0.9);
            }
            return;
        }
        self.race_ticks += 1;
        self.crowd.as_mut().unwrap().update(None);
        let total = self.rail.as_ref().unwrap().total;
        let mut player_finished = false;
        for i in 0..self.runners.len() {
            let pos = self.position_of(host, &self.runners[i]);
            let rail = self.rail.as_ref().unwrap();
            let hit = rail.nearest(&pos, self.runners[i].hit.map(|h| h.index), 60);
            self.runners[i].hit = Some(hit);
            if self.runners[i].finish_ticks.is_none() && hit.s > total - 260.0 && (pos[1] - hit.point[1]).abs() < 500.0 {
                self.runners[i].finish_ticks = Some(self.race_ticks);
                player_finished |= self.runners[i].player;
            }
        }
        if player_finished {
            self.finish(host);
        }
        self.catch_up();
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        let (Some(_), Some(rail)) = (self.crowd.as_ref(), self.rail.as_ref()) else { return };
        let eye = host.rig.eye;
        let alpha = host.alpha;
        self.crowd.as_mut().unwrap().render(host, alpha, dt, [eye.x, eye.y, eye.z]);
        let player_hit = self.runners.first().and_then(|r| r.hit);
        let hit = player_hit.unwrap_or_else(|| rail.nearest(&host.sim.mario.pos, None, 40));
        self.update_guide(host, hit);
        let finish: Vec<_> = self.runners.iter().map(|r| r.finish_ticks).collect();
        let progress: Vec<_> = self.runners.iter().map(|r| r.hit.map_or(0.0, |h| h.s)).collect();
        let order = standings(&finish, &progress);
        let place = order.iter().position(|&i| self.runners[i].player).unwrap_or(0);
        let rail_total = rail.total;
        let shown = self.runners.first().and_then(|r| r.finish_ticks).unwrap_or(self.race_ticks);
        let mut text = format!(
            "{} / {}   time {}   to summit {} m   best {}",
            ORDINALS[place.min(7)],
            self.runners.len(),
            format_time(shown as f64 / 30.0),
            ((rail_total - hit.s) / 100.0).max(0.0).round(),
            if self.best.is_finite() { format_time(self.best) } else { "-".into() }
        );
        if self.state == RaceState::Finished {
            text.push_str(&format!("\n{}", self.result));
        } else {
            let board: Vec<String> = order
                .iter()
                .enumerate()
                .take(4)
                .map(|(rank, &i)| {
                    let r = &self.runners[i];
                    format!("{} {} {}", ORDINALS[rank], if r.player { "YOU" } else { r.name.as_str() }, if r.finish_ticks.is_some() { "*".to_string() } else { format!("{}%", (100.0 * r.hit.map_or(0.0, |h| h.s) / rail_total).round()) })
                })
                .collect();
            text.push_str(&format!("\n{}", board.join("  |  ")));
        }
        host.status(text);
    }

    fn debug(&self) -> String {
        format!(
            "state={:?} race_s={:.1} route={:.0} runners={:?}",
            self.state,
            self.race_ticks as f64 / 30.0,
            self.rail.as_ref().map_or(0.0, |r| r.total),
            self.runners.iter().map(|r| (r.name.as_str(), r.hit.map_or(0, |h| h.s.round() as i64), r.finish_ticks)).collect::<Vec<_>>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standings_rank_finishers_by_time_then_progress() {
        let finish = [None, Some(300), Some(250), None];
        let progress = [900.0, 3000.0, 3000.0, 1200.0];
        assert_eq!(standings(&finish, &progress), vec![2, 1, 3, 0]);
    }

    #[test]
    fn time_formatting() {
        assert_eq!(format_time(75.34), "1:15.3");
        assert_eq!(format_time(5.0), "0:05.0");
    }
}
