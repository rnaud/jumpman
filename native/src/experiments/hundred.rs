//! A Hundred Marios (`hundred.ts`): a crowd of full-physics, fully animated Marios with five
//! colour-coded personalities flooding the course around the player.
//!
//! Keys instead of the DOM dock: `1`-`3` crowd size, `M` mix, `[`/`]` sim speed, `N` select the
//! next Mario, `Tab` watch it, `P` change its personality. No mouse picking yet (`pick` is
//! ported and tested; it needs a camera-projection hook in `Host`).
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
}

impl HundredMariosExperiment {
    pub fn new() -> Self {
        Self { crowd: None, watching: None, watch_pos: DVec3::ZERO, watch_look: DVec3::ZERO, count: 100, speed_index: 1, refresh: 0.0, rail: None, ticks: 0, summit: None }
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
        items.push(DockItem::new("Next", KeyCode::KeyN, 3).essential());
        let mut watch = DockItem::new("Watch", KeyCode::Tab, 3).essential();
        watch.selected = self.watching.is_some();
        items.push(watch);
        items.push(DockItem::new("Personality", KeyCode::KeyP, 3));
        items.push(DockItem::new("Start over", KeyCode::KeyR, 4));
        host.set_dock(items);
    }

    /// Back to the foot of the mountain for another climb through the crowd.
    fn restart(&mut self, host: &mut Host) {
        let spawn = host.sim.course.spawn.clone();
        host.place_mario(spawn.pos, Some(spawn.face_yaw));
        self.summit = None;
        self.ticks = 0;
        host.toast("BACK TO THE FOOT OF THE MOUNTAIN", 1.4);
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
            self.summit = Some((self.ticks, crowd.stats.summits));
            host.toast("YOU MADE THE SUMMIT!", 2.5);
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
                Some((t, first)) => line.push_str(&format!("\nSUMMIT REACHED in {} - {} Marios were there before you - {} to climb again", format_lap(t as f64 / 30.0), first, if host.touch { "tap START OVER" } else { "R" })),
                None => line.push_str("\nGOAL: climb to the summit through the crowd"),
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
            "Every Mario runs the real SM64 physics. Colour shows its personality.",
            &[("N", "select next Mario"), ("Tab", "watch the selected Mario"), ("P", "change its personality"), ("1 2 3", "25 / 50 / 100 Marios"), ("M", "mix"), ("[ ]", "sim speed"), ("J / Shift", "punch the crowd"), ("X", "leave")],
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
        }
        self.crowd = Some(crowd);
        self.dock(host);
        host.toast("A HUNDRED MARIOS - N TO INSPECT ONE", 3.2);
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
