//! Mario Across Dimensions (`dimensions.ts`): the same course and physics, seen and played as
//! 3D, a side-scrolling cross-section, a top-down map or a 2.5D path that follows the
//! mountain route.
//!
//! Not ported: the renderer clipping planes that slice away geometry between the camera and
//! the lane (and the earth-coloured cut profile). The camera math, lane locking, rail
//! projection and dolly-zoom transitions are all here.
use super::crowd_route::mario_route;
use super::rail_path::{RailHit, RailPath};
use super::{CamPose, Experiment, ExperimentId, Host, StepOverride};
use crate::render::SCALE;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use jumpman_sim::behaviors::koopa_the_quick::KOOPA_RACE_ROUTE;
use jumpman_sim::mario_runtime::MovementMode;
use jumpman_sim::math::{angle_to_radians, atan2s, coss, sins, to_s16};
use jumpman_sim::state::ControllerSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionMode {
    ThreeD,
    Side,
    Top,
    Path,
}

impl DimensionMode {
    fn label(self) -> &'static str {
        match self {
            Self::ThreeD => "3D",
            Self::Side => "2D side",
            Self::Top => "Top-down",
            Self::Path => "2.5D path",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::ThreeD => "3d",
            Self::Side => "side",
            Self::Top => "top",
            Self::Path => "path",
        }
    }
}

/// 2.5D: a side-on perspective camera that swings with the path (Klonoa, Pandemonium).
const PATH_FOV: f64 = 40.0;
const PATH_DISTANCE: f64 = 1250.0;
const PATH_HEIGHT: f64 = 260.0;
/// Near-orthographic lenses: a narrow FOV from far away flattens perspective.
const FLAT_FOV: f64 = 7.0;
const SIDE_VIEW_HEIGHT: f64 = 1500.0;
const TOP_VIEW_HEIGHT: f64 = 2600.0;
const TRANSITION: f64 = 0.75;
const BASE_FOV: f64 = 50.0;

fn smooth_step(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Visible height of a camera at `distance` with vertical FOV in degrees.
pub fn view_height(distance: f64, fov: f64) -> f64 {
    2.0 * distance * (fov.to_radians() / 2.0).tan()
}
pub fn distance_for_height(height: f64, fov: f64) -> f64 {
    height / (2.0 * (fov.to_radians() / 2.0).tan())
}

/// Lane direction for a side view: the camera's screen-right, snapped to 45 degrees.
pub fn lane_yaw_from_camera(camera_forward_yaw: f64) -> f64 {
    to_s16(jumpman_sim::math::js_round(to_s16(camera_forward_yaw - 0x4000 as f64) as f64 / 0x2000 as f64) * 0x2000 as f64) as f64
}

pub struct DimensionsOptions {
    pub course: Option<&'static str>,
    pub initial: DimensionMode,
    /// Force the side-scrolling axis (authored 2D levels).
    pub lane_yaw: Option<f64>,
}

impl Default for DimensionsOptions {
    fn default() -> Self {
        Self { course: Some("bob"), initial: DimensionMode::ThreeD, lane_yaw: None }
    }
}

pub struct DimensionsExperiment {
    course: Option<&'static str>,
    initial: DimensionMode,
    forced_lane: Option<f64>,
    pub mode: DimensionMode,
    lane_yaw: f64,
    lane_origin: DVec3,
    top_yaw: f64,
    from: Option<CamPose>,
    elapsed: f64,
    focus_y: f64,
    switches: u32,
    modes: Vec<DimensionMode>,
    rail: Option<RailPath>,
    rail_hit: Option<RailHit>,
    /// +1: camera on the rail's right so progress runs screen-right; -1: the other side.
    rail_side: f64,
    rail_yaw: f64,
    guide: Vec<Entity>,
    summit_announced: bool,
    last_pose: CamPose,
}

impl DimensionsExperiment {
    pub fn new(options: DimensionsOptions) -> Self {
        Self {
            course: options.course,
            initial: options.initial,
            forced_lane: options.lane_yaw,
            mode: DimensionMode::ThreeD,
            lane_yaw: 0x4000 as f64,
            lane_origin: DVec3::ZERO,
            top_yaw: 0.0,
            from: None,
            elapsed: TRANSITION,
            focus_y: 0.0,
            switches: 0,
            modes: vec![DimensionMode::ThreeD, DimensionMode::Side, DimensionMode::Top],
            rail: None,
            rail_hit: None,
            rail_side: 1.0,
            rail_yaw: 0.0,
            guide: vec![],
            summit_announced: false,
            last_pose: CamPose { eye: DVec3::ZERO, look: DVec3::ZERO, up: DVec3::Y, fov: BASE_FOV, near: 5.0, far: 40000.0 },
        }
    }

    pub fn set_mode(&mut self, host: &mut Host, mode: DimensionMode) {
        if mode == self.mode {
            return;
        }
        // Transitions start from wherever the camera is right now, so a switch mid-transition stays smooth.
        self.from = Some(self.last_pose);
        self.elapsed = 0.0;
        let forward_yaw = to_s16(host.rig.yaw_s16() as f64 + 0x8000 as f64) as f64;
        match mode {
            DimensionMode::Side => {
                self.lane_yaw = self.forced_lane.unwrap_or(if self.mode == DimensionMode::Top { lane_yaw_from_camera(self.top_yaw) } else { lane_yaw_from_camera(forward_yaw) });
                self.lane_origin = DVec3::from(host.sim.mario.pos);
                host.sim.mario.side_yaw = self.lane_yaw;
                host.sim.mario.movement_mode = MovementMode::Side;
            }
            DimensionMode::Path if self.rail.is_some() => {
                self.join_rail(host);
                host.sim.mario.movement_mode = MovementMode::Side;
            }
            _ => host.sim.mario.movement_mode = MovementMode::Default,
        }
        if mode == DimensionMode::Top {
            self.top_yaw = if self.mode == DimensionMode::Side { to_s16(self.lane_yaw + 0x4000 as f64) as f64 } else { forward_yaw };
        }
        self.focus_y = host.sim.mario.pos[1] + 120.0;
        self.mode = mode;
        self.switches += 1;
        let text = match mode {
            DimensionMode::ThreeD => "BACK TO 3D",
            DimensionMode::Side => "2D - SIDE-SCROLLING",
            DimensionMode::Path => "2.5D - FOLLOW THE PATH TO THE SUMMIT",
            DimensionMode::Top => "TOP-DOWN",
        };
        host.toast(text, 1.6);
        self.update_status(host);
    }

    fn update_status(&self, host: &mut Host) {
        let list: Vec<String> = self.modes.iter().enumerate().map(|(i, m)| if *m == self.mode { format!("[{}:{}]", i + 1, m.label()) } else { format!("{}:{}", i + 1, m.label()) }).collect();
        host.status(list.join("   "));
    }

    /// Step onto the rail where it is closest (or at its start when Mario is far away).
    fn join_rail(&mut self, host: &mut Host) {
        let rail = self.rail.as_ref().unwrap();
        let pos = host.sim.mario.pos;
        let mut hit = rail.nearest(&pos, None, 40);
        let away = (hit.point[0] - pos[0]).hypot(hit.point[2] - pos[2]);
        if away > 500.0 {
            hit = rail.at(0.0);
            let yaw = atan2s(hit.tangent[1], hit.tangent[0]) as f64;
            host.place_mario([hit.point[0], hit.point[1] + 200.0, hit.point[2]], Some(yaw));
        }
        self.rail_yaw = atan2s(hit.tangent[1], hit.tangent[0]) as f64;
        self.rail_hit = Some(hit);
        self.summit_announced = false;
    }

    /// Put the camera on whichever side of the route has lower ground more often, so it looks
    /// across the mountain path instead of into the hillside.
    fn choose_rail_side(&self, host: &Host) -> f64 {
        let rail = self.rail.as_ref().unwrap();
        let world = &host.sim.course.world;
        let mut vote = 0.0;
        let mut s = 0.0;
        while s < rail.total {
            let hit = rail.at(s);
            let [tx, tz] = hit.tangent;
            let left = world.find_floor(hit.point[0] - tz * 700.0, hit.point[1] + 3000.0, hit.point[2] + tx * 700.0).height;
            let right = world.find_floor(hit.point[0] + tz * 700.0, hit.point[1] + 3000.0, hit.point[2] - tx * 700.0).height;
            let d: f64 = right - left;
            vote += if d == 0.0 { 0.0 } else { d.signum() };
            s += 300.0;
        }
        if vote >= 0.0 {
            1.0
        } else {
            -1.0
        }
    }

    /// Project Mario onto the rail and aim his side-scrolling stick along it.
    fn follow_rail(&mut self, host: &mut Host) {
        let Some(rail) = self.rail.as_ref() else { return };
        let m = &mut host.sim.mario;
        let hit = rail.nearest(&m.pos, self.rail_hit.map(|h| h.index), 40);
        self.rail_hit = Some(hit);
        let [tx, tz] = hit.tangent;
        m.pos[0] = hit.point[0];
        m.pos[2] = hit.point[2];
        let along = m.vel[0] * tx + m.vel[2] * tz;
        m.vel[0] = tx * along;
        m.vel[2] = tz * along;
        let tangent_yaw = atan2s(tz, tx) as f64;
        // Stick right always moves screen-right.
        m.side_yaw = if self.rail_side > 0.0 { tangent_yaw } else { to_s16(tangent_yaw + 0x8000 as f64) as f64 };
        if !self.summit_announced && hit.s > rail.total - 250.0 {
            self.summit_announced = true;
            host.toast("SUMMIT!", 2.0);
        }
    }

    fn guide_update(&mut self, host: &mut Host) {
        let (Some(rail), Some(hit)) = (self.rail.as_ref(), self.rail_hit) else { return };
        let world = host.sim.course.world.clone();
        let start = (hit.s / 140.0).floor() * 140.0;
        let render_y = host.render_pos()[1];
        let t = host.time();
        let mut n = 0;
        for i in -3i32..(self.guide.len() as i32 - 3) {
            let s = start + i as f64 * 140.0 * self.rail_side;
            if s < 0.0 || s > rail.total {
                continue;
            }
            let p = rail.at(s).point;
            let floor = world.find_floor(p[0], p[1].max(render_y) + 600.0, p[2]);
            if floor.surface.is_none() {
                continue;
            }
            let pulse = 1.0 + 0.25 * (t * 5.0 - i as f64 * 0.6).sin();
            let tf = Transform::from_xyz(p[0] as f32 * SCALE, (floor.height as f32 + 14.0) * SCALE, p[2] as f32 * SCALE).with_scale(Vec3::splat(pulse as f32));
            host.commands.entity(self.guide[n]).insert((tf, Visibility::Inherited));
            n += 1;
        }
        for e in &self.guide[n..] {
            host.commands.entity(*e).insert(Visibility::Hidden);
        }
    }

    /// Dolly-zoom between poses: the visible height around Mario is preserved while the lens narrows.
    fn blend(from: &CamPose, to: &CamPose, focus: DVec3, t: f64) -> CamPose {
        let from_offset = from.eye - focus;
        let to_offset = to.eye - focus;
        let from_height = view_height(from_offset.length(), from.fov);
        let to_height = view_height(to_offset.length(), to.fov);
        let fov = (from.fov.ln() + (to.fov.ln() - from.fov.ln()) * t).exp();
        let height = from_height + (to_height - from_height) * t;
        let a = from_offset.normalize();
        let b = to_offset.normalize();
        let rotation = DQuat::from_rotation_arc(a, b);
        let partial = DQuat::IDENTITY.slerp(rotation, t);
        let direction = (partial * a).normalize();
        let mut up = from.up.lerp(to.up, t);
        if up.length_squared() < 1e-4 {
            up = to.up;
        }
        CamPose { eye: focus + direction * distance_for_height(height, fov), look: focus, up: up.normalize(), fov, near: to.near, far: to.far }
    }
}

impl Experiment for DimensionsExperiment {
    fn id(&self) -> ExperimentId {
        ExperimentId::Dimensions
    }
    fn course(&self) -> Option<&'static str> {
        self.course
    }

    fn enter(&mut self, host: &mut Host) {
        if host.world_name() == "bob" {
            let world = host.course_world();
            let rail = RailPath::new(&mario_route(&world, &KOOPA_RACE_ROUTE));
            self.rail = Some(rail);
            self.rail_side = self.choose_rail_side(host);
            self.modes.push(DimensionMode::Path);
            let mesh = host.meshes.add(Sphere::new(13.0 * SCALE));
            let mat = host.unlit(Color::srgb(1.0, 0.82, 0.29));
            for _ in 0..28 {
                let e = host.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden));
                self.guide.push(e);
            }
        }
        host.card(
            "01",
            "Across Dimensions",
            "One level, one Mario, many ways to play it. Switch any time - even mid-jump.",
            &[("Tab", "next dimension"), ("1-4", "3D / 2D / top-down / 2.5D path"), ("A D", "run (2D, path)"), ("WASD", "run (3D, top-down)"), ("X", "leave")],
        );
        self.focus_y = host.sim.mario.pos[1] + 120.0;
        if self.initial != DimensionMode::ThreeD {
            self.set_mode(host, self.initial);
            // Start already flat: no transition from the 3D camera.
            self.from = None;
        } else {
            host.toast("PRESS TAB TO FLATTEN THE WORLD", 3.2);
            self.update_status(host);
        }
    }

    fn exit(&mut self, host: &mut Host) {
        host.sim.mario.movement_mode = MovementMode::Default;
        host.cam.pose = None;
    }

    fn keydown(&mut self, host: &mut Host, key: KeyCode) -> bool {
        if matches!(key, KeyCode::Tab | KeyCode::KeyM) {
            let i = self.modes.iter().position(|m| *m == self.mode).unwrap();
            let next = self.modes[(i + 1) % self.modes.len()];
            self.set_mode(host, next);
            return true;
        }
        let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
        if let Some(i) = digits.iter().position(|k| *k == key) {
            if i < self.modes.len() {
                let mode = self.modes[i];
                self.set_mode(host, mode);
                return true;
            }
        }
        false
    }

    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        // Top-down: the stick is screen-relative to the fixed map orientation.
        if self.mode == DimensionMode::Top {
            return StepOverride { controller: None, camera_yaw: Some(to_s16(self.top_yaw + 0x8000 as f64)) };
        }
        StepOverride::default()
    }

    fn after_step(&mut self, host: &mut Host) {
        match self.mode {
            DimensionMode::Path => self.follow_rail(host),
            DimensionMode::Side => {
                // Keep Mario on the lane: remove any drift across it (wall pushes, knockbacks).
                let m = &mut host.sim.mario;
                let (lx, lz) = (sins(self.lane_yaw), coss(self.lane_yaw));
                let (ox, oz) = (m.pos[0] - self.lane_origin.x, m.pos[2] - self.lane_origin.z);
                let along = ox * lx + oz * lz;
                m.pos[0] = self.lane_origin.x + lx * along;
                m.pos[2] = self.lane_origin.z + lz * along;
                let velocity_along = m.vel[0] * lx + m.vel[2] * lz;
                m.vel[0] = lx * velocity_along;
                m.vel[2] = lz * velocity_along;
            }
            _ => {}
        }
    }

    fn frame(&mut self, host: &mut Host, dt: f64) {
        let render = DVec3::from(host.render_pos());
        let mut focus = render + DVec3::new(0.0, 110.0, 0.0);
        let t = if self.from.is_some() {
            self.elapsed += dt;
            smooth_step((self.elapsed / TRANSITION).min(1.0))
        } else {
            1.0
        };
        let default_pose = CamPose { eye: host.rig.eye, look: host.rig.look_at, up: DVec3::Y, fov: BASE_FOV, near: 5.0, far: 40000.0 };
        let target = match self.mode {
            DimensionMode::ThreeD => {
                if t >= 1.0 {
                    self.from = None;
                    host.cam.pose = None;
                    self.last_pose = default_pose;
                    return;
                }
                default_pose
            }
            DimensionMode::Side => {
                // Platformer camera: exact along the lane, lazy vertically.
                let desired_y = render.y + 140.0;
                if (desired_y - self.focus_y).abs() > 160.0 {
                    self.focus_y += (desired_y - self.focus_y - (desired_y - self.focus_y).signum() * 160.0) * (dt * 6.0).min(1.0);
                }
                self.focus_y += (desired_y - self.focus_y) * (dt * 1.6).min(1.0);
                focus.y = self.focus_y;
                let view = angle_to_radians(self.lane_yaw + 0x4000 as f64);
                let distance = distance_for_height(SIDE_VIEW_HEIGHT, FLAT_FOV);
                CamPose { eye: focus + DVec3::new(-view.sin() * distance, 0.0, -view.cos() * distance), look: focus, up: DVec3::Y, fov: FLAT_FOV, near: 5.0, far: 40000.0 }
            }
            DimensionMode::Path if self.rail_hit.is_some() => {
                let [tx, tz] = self.rail_hit.unwrap().tangent;
                // Swing smoothly around bends rather than snapping with every rail sample.
                let tangent_yaw = atan2s(tz, tx) as f64;
                self.rail_yaw = to_s16(self.rail_yaw + to_s16(tangent_yaw - self.rail_yaw) as f64 * (dt * 3.0).min(1.0)) as f64;
                let view = angle_to_radians(self.rail_yaw + self.rail_side * 0x4000 as f64);
                let lead = angle_to_radians(self.rail_yaw);
                let desired_y = render.y + 120.0;
                self.focus_y += (desired_y - self.focus_y) * (dt * 3.0).min(1.0);
                focus = DVec3::new(render.x + lead.sin() * 120.0 * self.rail_side, self.focus_y, render.z + lead.cos() * 120.0 * self.rail_side);
                self.guide_update(host);
                CamPose {
                    eye: focus + DVec3::new(-view.sin() * PATH_DISTANCE, PATH_HEIGHT, -view.cos() * PATH_DISTANCE),
                    look: focus,
                    up: DVec3::Y,
                    fov: PATH_FOV,
                    near: 5.0,
                    far: 40000.0,
                }
            }
            _ => {
                focus.y = render.y;
                let up = angle_to_radians(self.top_yaw);
                let distance = distance_for_height(TOP_VIEW_HEIGHT, FLAT_FOV);
                CamPose { eye: focus + DVec3::new(0.0, distance, 0.0), look: focus, up: DVec3::new(up.sin(), 0.0, up.cos()), fov: FLAT_FOV, near: 5.0, far: 40000.0 }
            }
        };
        if self.mode != DimensionMode::Path {
            for e in &self.guide {
                host.commands.entity(*e).insert(Visibility::Hidden);
            }
        }
        let mut pose = match &self.from {
            Some(from) if t < 1.0 => Self::blend(from, &target, focus, t),
            _ => target,
        };
        if t >= 1.0 {
            self.from = None;
        }
        // A far-away narrow lens needs its depth range pulled in to avoid z-fighting.
        let distance = pose.eye.distance(focus);
        pose.near = (distance - 3000.0).max(5.0);
        pose.far = (distance + 26000.0).max(40000.0);
        self.last_pose = pose;
        host.cam.pose = Some(pose);
    }

    fn debug(&self) -> String {
        format!(
            "mode={} lane_yaw={} switches={} transitioning={} rail={:?}",
            self.mode.name(),
            self.lane_yaw,
            self.switches,
            self.from.is_some(),
            self.rail.as_ref().map(|r| (r.total.round(), self.rail_hit.map_or(-1.0, |h| h.s.round()), self.rail_side))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
    use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
    use std::sync::Arc;

    fn mario() -> MarioRuntime {
        let mut world = CollisionWorld::default();
        world.add_quad([-2000., 0., -2000.], [-2000., 0., 2000.], [2000., 0., 2000.], [2000., 0., -2000.], &SurfaceOptions::default());
        world.finalize();
        MarioRuntime::new(Arc::new(world), SpawnPoint { action: None, pos: [0., 0., 0.], face_yaw: 0.0 }, -1600.0)
    }

    fn stick(x: f64, y: f64) -> ControllerSnapshot {
        ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: x.abs().max(y.abs()), ..Default::default() }
    }

    #[test]
    fn maps_horizontal_stick_input_to_world_x_in_side_view() {
        let mut m = mario();
        m.movement_mode = MovementMode::Side;
        m.step(&stick(64., 0.), 0.0, false, false);
        assert_eq!(m.intended_yaw, 0x4000 as f64);
        m.step(&stick(-64., 0.), 0.0, false, false);
        assert_eq!(to_s16(m.intended_yaw), to_s16(0xc000 as f64));
    }

    #[test]
    fn runs_along_any_lane_direction_in_side_view() {
        let mut m = mario();
        m.movement_mode = MovementMode::Side;
        m.side_yaw = 0x2000 as f64;
        m.step(&stick(64., 0.), 0.0, false, false);
        assert_eq!(m.intended_yaw, 0x2000 as f64);
        m.step(&stick(-64., 0.), 0.0, false, false);
        assert_eq!(m.intended_yaw, to_s16(0xa000 as f64) as f64);
    }

    #[test]
    fn snaps_the_side_lane_to_the_cameras_screen_right_in_45_degree_steps() {
        // Camera looking toward -Z (yaw 0x8000): screen-right is +X (yaw 0x4000).
        assert_eq!(lane_yaw_from_camera(0x8000 as f64 - 0x10000 as f64), 0x4000 as f64);
        assert_eq!(lane_yaw_from_camera(0.0), to_s16(0xc000 as f64) as f64);
        assert_eq!(lane_yaw_from_camera(0x8000 as f64 + 0x0900 as f64 - 0x10000 as f64), 0x4000 as f64);
        assert_eq!(lane_yaw_from_camera(0x8000 as f64 + 0x1900 as f64 - 0x10000 as f64), 0x6000 as f64);
    }

    #[test]
    fn keeps_the_visible_height_constant_through_the_flattening_dolly_zoom() {
        let distance = distance_for_height(1500.0, 7.0);
        assert!((view_height(distance, 7.0) - 1500.0).abs() < 1e-6);
        assert!(distance > 10000.0);
    }

    #[test]
    fn maps_screen_up_to_negative_z_in_top_down_view() {
        let mut m = mario();
        m.movement_mode = MovementMode::Topdown;
        m.step(&stick(0., 64.), 0.0, false, false);
        assert_eq!(m.intended_yaw, -0x8000 as f64);
        m.step(&stick(64., 0.), 0.0, false, false);
        assert_eq!(m.intended_yaw, 0x4000 as f64);
    }

    #[test]
    fn dolly_blend_keeps_visible_height_between_poses() {
        let focus = DVec3::new(0.0, 100.0, 0.0);
        let from = CamPose { eye: focus + DVec3::new(0.0, 300.0, 700.0), look: focus, up: DVec3::Y, fov: 50.0, near: 1.0, far: 1e5 };
        let to = CamPose { eye: focus + DVec3::new(0.0, 0.0, 12000.0), look: focus, up: DVec3::Y, fov: 7.0, near: 1.0, far: 1e5 };
        let end = DimensionsExperiment::blend(&from, &to, focus, 1.0);
        assert!((end.fov - 7.0).abs() < 1e-9);
        assert!((end.eye.distance(focus) - to.eye.distance(focus)).abs() < 1.0);
        let mid = DimensionsExperiment::blend(&from, &to, focus, 0.5);
        assert!(mid.fov < 50.0 && mid.fov > 7.0);
    }
}
