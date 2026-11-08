//! Bevy host of the SM64 camera port (`jumpman_sim::sm64cam`, a port of `src/game/camera.c`).
//!
//! * `step` runs once per 30 Hz simulation tick, after Mario's step, feeding [`SmCam`] Mario's state,
//!   the controller (touch drag / mouse drag / Q E / wheel become C-left, C-right, C-down, C-up
//!   presses) and the collision world. It writes `CameraRig::yaw` so Mario's stick direction uses
//!   `c->yaw` like the game.
//! * `render_pose` (in `camera::update_camera`) interpolates the last two Lakitu poses by the
//!   fixed-step overstep and replaces the orbit rig's pose, FOV and roll. The orbit rig still runs
//!   everywhere the original camera does not apply (Link, Banjo, non-SM64 courses, experiments) or
//!   when Settings says "Camera: Free orbit".
//! * Systems that take the camera over *after* [`crate::CameraSet`] (Z-targeting, first-person aim,
//!   kart chase, cannon, dialog, experiments via `CameraOverride`) keep working unchanged.
//! * [`CutsceneRequest`] is the hook for Mario's cutscene actions and objects: a message other
//!   modules write to ask the camera for a cutscene or an event.
use super::CameraRig;
use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::shell::settings::{CameraStyle, Settings};
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::math::to_s16;
use jumpman_sim::object::ObjectBehaviorKind;
use jumpman_sim::sm64cam::cutscenes::{CutFocus, PaintingInfo};
use jumpman_sim::sm64cam::data::{level_for_course, LEVEL_BOB, LEVEL_WF};
use jumpman_sim::sm64cam::{self as sc, Env, MarioCam, Pad, SecondFocus, SmCam, UsedObj};
use jumpman_sim::state::*;
use std::f64::consts::TAU;

pub use jumpman_sim::sm64cam::cutscenes as cutscene_ids;

/// Ask the SM64 camera for something (the cutscene agent's / objects' entry point).
#[derive(Message, Clone, Debug)]
pub enum CutsceneRequest {
    /// Set Mario's `cameraEvent` (`sm64cam::CAM_EVENT_*`: door, door warp, cannon, ...). Cleared by the camera.
    Event(i32),
    /// An object asks for a cutscene (`cutscene_object`): `sm64cam::cutscenes::CUTSCENE_STAR_SPAWN`, ... with
    /// the object it looks at (its position, home and hitbox height).
    Object { cutscene: i32, focus: Option<CutsceneFocus> },
    /// The object that started the cutscene is done (`gObjCutsceneDone`): star spawn / red coin star fly back.
    ObjectDone,
    /// Start a cutscene directly (`start_cutscene`).
    Start(i32),
    /// `set_camera_mode(c, mode, frames)`.
    Mode { mode: i32, frames: i32 },
    /// `set_camera_shake_from_hit(SHAKE_*)`.
    ShakeHit(i32),
    /// `set_environmental_camera_shake(SHAKE_ENV_*)`.
    ShakeEnv(i32),
    /// `set_camera_shake_from_point(SHAKE_POS_*, pos)`.
    ShakePoint { kind: i32, pos: [f64; 3] },
    /// Mario teleported by this much (`warp_camera`).
    Warp([f64; 3]),
    /// The rippling painting Mario enters (`gRipplingPainting`).
    Painting(Option<PaintingInfo>),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CutsceneFocus {
    pub pos: [f64; 3],
    pub home: [f64; 3],
    pub hitbox_height: f64,
}

/// Pixels of drag per C-left / C-right press, and per zoom press.
const DRAG_PX: f32 = 56.0;
const ZOOM_PX: f32 = 96.0;
const WHEEL_PER_PRESS: f32 = 40.0;

#[derive(Resource)]
pub struct Sm64Camera {
    pub cam: SmCam,
    /// Interpolation endpoints in SM64 space: (eye, focus, roll radians).
    pub prev: (DVec3, DVec3, f64),
    pub cur: (DVec3, DVec3, f64),
    /// Vertical FOV of the last tick (`geo_camera_fov`, including the FOV shake).
    pub fov_out: f64,
    /// Whether the original camera drove the last tick (the orbit rig otherwise).
    pub active: bool,
    key: Option<(String, i32)>,
    last_ticks: u64,
    last_pos: [f64; 3],
    last_action: u32,
    last_hurt: f64,
    drag: Vec2,
    wheel: f32,
    orbit_held: i32,
    orbit_timer: u32,
    pending_c: u16,
    pending_events: Vec<CutsceneRequest>,
    painting: Option<PaintingInfo>,
    /// `CutsceneFocus` of the running object cutscene (refreshed by [`CutsceneRequest::Object`]).
    focus: Option<CutFocus>,
}

impl Default for Sm64Camera {
    fn default() -> Self {
        Self {
            cam: SmCam::new(),
            prev: (DVec3::ZERO, DVec3::ZERO, 0.0),
            cur: (DVec3::ZERO, DVec3::ZERO, 0.0),
            fov_out: 45.0,
            active: false,
            key: None,
            last_ticks: 0,
            last_pos: [0.0; 3],
            last_action: 0,
            last_hurt: 0.0,
            drag: Vec2::ZERO,
            wheel: 0.0,
            orbit_held: 0,
            orbit_timer: 0,
            pending_c: 0,
            pending_events: vec![],
            painting: None,
            focus: None,
        }
    }
}

pub struct Sm64CameraPlugin;

impl Plugin for Sm64CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sm64Camera>().add_message::<CutsceneRequest>().add_systems(FixedUpdate, step.after(crate::SimSet));
    }
}

/// The camera style in force: Settings, overridden by `JUMPMAN_CAMERA=orbit|original`.
pub fn camera_style(settings: Option<&Settings>) -> CameraStyle {
    match std::env::var("JUMPMAN_CAMERA").as_deref() {
        Ok("orbit") | Ok("free") => CameraStyle::FreeOrbit,
        Ok("original") => CameraStyle::Original,
        _ => settings.map_or(CameraStyle::Original, |s| s.camera),
    }
}

/// Does the SM64 camera drive this frame? Mario on an SM64 course, no experiment running, style Original.
pub fn applicable(sim: &GameSim, style: CameraStyle, experiment_active: bool) -> bool {
    style == CameraStyle::Original && !experiment_active && !sim.as_link && !sim.as_banjo && level_for_course(&sim.course.request.id).is_some()
}

fn pose_of(cam: &SmCam) -> (DVec3, DVec3, f64) {
    let l = &cam.lakitu;
    (DVec3::from(l.pos), DVec3::from(l.focus), l.roll as f64 * TAU / 65536.0)
}

/// Turn the render-frame camera input into C-button presses (called from `camera::camera_controls`).
/// Touch / mouse drag left-right is C-left / C-right, up-down and the wheel zoom (C-down / C-up while zoomed out).
pub fn feed_input(st: &mut Sm64Camera, dx: f32, dy: f32, wheel: f32, orbit_x: f32) {
    st.drag.x += dx;
    st.drag.y += dy;
    st.wheel += wheel;
    // Dragging right pulls the view with the finger: the camera swings left (C-left).
    while st.drag.x >= DRAG_PX {
        st.drag.x -= DRAG_PX;
        st.pending_c |= sc::L_CBUTTONS;
    }
    while st.drag.x <= -DRAG_PX {
        st.drag.x += DRAG_PX;
        st.pending_c |= sc::R_CBUTTONS;
    }
    while st.drag.y >= ZOOM_PX {
        st.drag.y -= ZOOM_PX;
        st.pending_c |= sc::D_CBUTTONS;
    }
    while st.drag.y <= -ZOOM_PX {
        st.drag.y += ZOOM_PX;
        st.pending_c |= sc::U_CBUTTONS;
    }
    while st.wheel >= WHEEL_PER_PRESS {
        st.wheel -= WHEEL_PER_PRESS;
        st.pending_c |= sc::D_CBUTTONS;
    }
    while st.wheel <= -WHEEL_PER_PRESS {
        st.wheel += WHEEL_PER_PRESS;
        st.pending_c |= sc::U_CBUTTONS;
    }
    // Q / E / right stick: a press now and every 10 ticks while held.
    let dir = if orbit_x > 0.5 { 1 } else if orbit_x < -0.5 { -1 } else { 0 };
    if dir != st.orbit_held {
        st.orbit_held = dir;
        st.orbit_timer = 0;
    }
}

fn make_env<'a>(sim: &'a GameSim, pad: Pad, st: &Sm64Camera) -> Option<Env<'a>> {
    let m = &sim.mario;
    let level = level_for_course(&sim.course.request.id)?;
    let objects = &m.objects.objects;
    // gSecondCameraFocus: King Bob-omb on the summit, Whomp King, the Bowser / big bully of the boss areas.
    let second_focus = objects
        .iter()
        .find(|o| {
            o.active
                && match level {
                    LEVEL_BOB => o.behavior == ObjectBehaviorKind::KingBobomb,
                    LEVEL_WF => o.behavior == ObjectBehaviorKind::WhompKing,
                    _ => matches!(o.behavior, ObjectBehaviorKind::Bowser | ObjectBehaviorKind::BigBully),
                }
        })
        .map(|o| SecondFocus { pos: o.pos, held_state: i32::from(m.held_obj.is_some_and(|h| m.objects[h].id == o.id)) });
    let used_obj = m.used_obj.map(|id| {
        let o = &m.objects[id];
        UsedObj { pos: o.pos, hitbox_height: o.hitbox.height, move_angle: [0, to_s16(o.move_yaw), 0] }
    });
    let wl = m.world.water_level(m.pos[0], m.pos[2]);
    Some(Env {
        world: &m.world,
        m: MarioCam {
            pos: m.pos,
            face_angle: [to_s16(m.face_angle[0]), to_s16(m.face_angle[1]), to_s16(m.face_angle[2])],
            action: m.action,
            forward_vel: m.forward_vel,
            angle_vel_y: m.angle_vel[1],
            water_level: if wl.is_finite() { wl } else { sc::FLOOR_LOWER_LIMIT },
            used_obj,
        },
        pad,
        level,
        area: sim.course.request.area,
        act: sim.course.request.act,
        second_focus,
        dialog_active: false,
        course_num: sc::data::course_for_level(level),
        last_star_num: sim.course.request.act,
        cutscene_focus: st.focus,
        painting: st.painting,
    })
}

/// The 30 Hz camera tick (after Mario's step).
#[allow(clippy::too_many_arguments)]
fn step(
    mut st: ResMut<Sm64Camera>,
    sim: Res<GameSim>,
    input: Res<PlayerInput>,
    settings: Option<Res<Settings>>,
    xp: Res<crate::experiments::ExperimentSlot>,
    mut rig: ResMut<CameraRig>,
    mut requests: MessageReader<CutsceneRequest>,
) {
    let style = camera_style(settings.as_deref());
    st.pending_events.extend(requests.read().cloned());
    if !applicable(&sim, style, xp.active.is_some()) {
        st.active = false;
        st.pending_c = 0;
        st.pending_events.clear();
        return;
    }
    let st = &mut *st;
    // Controller: stick (+-64) and C buttons (presses live for exactly one tick).
    if st.orbit_held != 0 {
        if st.orbit_timer % 10 == 0 {
            st.pending_c |= if st.orbit_held > 0 { sc::R_CBUTTONS } else { sc::L_CBUTTONS };
        }
        st.orbit_timer += 1;
    }
    let pressed = std::mem::take(&mut st.pending_c);
    let pad = Pad { stick_x: input.stick.x as f64, stick_y: input.stick.y as f64, pressed, down: pressed };
    let key = (sim.course.request.id.clone(), sim.course.request.area);
    let m_pos = sim.mario.pos;
    let teleported = (0..3).any(|i| (m_pos[i] - st.last_pos[i]).abs() > 1500.0);
    let restarted = sim.ticks < st.last_ticks;
    let fresh = st.key.as_ref() != Some(&key) || restarted || teleported || !st.active;
    let prev_action = st.last_action;
    let Some(env) = make_env(&sim, pad, st) else { return };
    st.cam.mario = env.m;
    if fresh {
        st.cam.reset_camera(&env);
        st.key = Some(key);
    }
    // CutsceneRequest messages.
    for r in std::mem::take(&mut st.pending_events) {
        match r {
            CutsceneRequest::Event(e) => st.cam.camera_event = e,
            CutsceneRequest::Object { cutscene, focus } => {
                let f = focus.map(|f| CutFocus { pos: f.pos, home: f.home, hitbox_height: f.hitbox_height });
                st.focus = f;
                st.cam.start_object_cutscene(cutscene, f);
            }
            CutsceneRequest::ObjectDone => st.cam.cs.obj_done = true,
            CutsceneRequest::Start(c) => st.cam.start_cutscene(&env, c),
            CutsceneRequest::Mode { mode, frames } => st.cam.set_camera_mode(&env, mode, frames),
            CutsceneRequest::ShakeHit(s) => st.cam.set_camera_shake_from_hit(s),
            CutsceneRequest::ShakeEnv(s) => st.cam.set_environmental_camera_shake(s),
            CutsceneRequest::ShakePoint { kind, pos } => st.cam.set_camera_shake_from_point(kind, pos),
            CutsceneRequest::Warp(d) => st.cam.warp_camera(&env, d),
            CutsceneRequest::Painting(p) => st.painting = p,
        }
    }
    if st.cam.c.cutscene == 0 && st.cam.cs.obj_done {
        st.cam.cs.obj_done = false;
        st.focus = None;
    }
    // Camera events derived from Mario's actions (doors open on the pull / push action).
    let a = sim.mario.action;
    if (a == sc::act::PULLING_DOOR || a == sc::act::PUSHING_DOOR) && prev_action != a && st.cam.camera_event == 0 {
        st.cam.camera_event = sc::CAM_EVENT_DOOR;
    }
    // Mario-side hooks + shakes the decomp raises from Mario / objects.
    st.cam.mario_camera_hooks(&env, prev_action, false);
    if a == ACT_GROUND_POUND_LAND && prev_action != a {
        st.cam.set_camera_shake_from_hit(sc::SHAKE_GROUND_POUND);
    }
    let hurt = sim.mario.hurt_counter;
    if hurt > st.last_hurt && hurt > 0.0 {
        st.cam.set_camera_shake_from_hit(if hurt >= 12.0 { sc::SHAKE_LARGE_DAMAGE } else if hurt >= 8.0 { sc::SHAKE_MED_DAMAGE } else { sc::SHAKE_SMALL_DAMAGE });
    }
    st.last_hurt = hurt;
    for o in &sim.mario.objects.objects {
        if o.active && matches!(o.behavior, ObjectBehaviorKind::PitBowlingBall | ObjectBehaviorKind::MountainBall) {
            st.cam.set_camera_shake_from_point(sc::SHAKE_POS_BOWLING_BALL, o.pos);
        }
    }
    st.cam.update_camera(&env);
    st.fov_out = st.cam.step_fov();
    let pose = pose_of(&st.cam);
    st.prev = if fresh || (pose.0 - st.cur.0).length() > 700.0 { pose } else { st.cur };
    st.cur = pose;
    st.active = true;
    st.last_ticks = sim.ticks;
    st.last_pos = m_pos;
    st.last_action = a;
    // Mario's stick direction follows c->yaw (radians in the rig's convention).
    rig.yaw = (st.cam.c.yaw as f64 * TAU / 65536.0 + TAU) % TAU;
    st.cam.sounds.clear();
}

/// Interpolated (eye, focus, roll) for the render frame, in SM64 space.
pub fn render_pose(st: &Sm64Camera, alpha: f64) -> (DVec3, DVec3, f64) {
    let a = alpha.clamp(0.0, 1.0);
    (st.prev.0.lerp(st.cur.0, a), st.prev.1.lerp(st.cur.1, a), st.prev.2 + (st.cur.2 - st.prev.2) * a)
}

/// Vertical FOV in degrees the SM64 camera wants.
pub fn fov_degrees(st: &Sm64Camera) -> f32 {
    st.fov_out as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drags_and_wheel_become_c_presses() {
        let mut st = Sm64Camera::default();
        // dragging right swings the camera left (C-left); a short drag does nothing yet
        feed_input(&mut st, DRAG_PX - 1.0, 0.0, 0.0, 0.0);
        assert_eq!(st.pending_c, 0);
        feed_input(&mut st, 2.0, 0.0, 0.0, 0.0);
        assert_eq!(st.pending_c, sc::L_CBUTTONS);
        st.pending_c = 0;
        st.drag = Vec2::ZERO;
        feed_input(&mut st, -DRAG_PX, 0.0, 0.0, 0.0);
        assert_eq!(st.pending_c, sc::R_CBUTTONS);
        st.pending_c = 0;
        // vertical drag and the wheel zoom: down / wheel forward-back are C-down / C-up
        feed_input(&mut st, 0.0, ZOOM_PX, 0.0, 0.0);
        assert_eq!(st.pending_c, sc::D_CBUTTONS);
        st.pending_c = 0;
        feed_input(&mut st, 0.0, 0.0, -WHEEL_PER_PRESS, 0.0);
        assert_eq!(st.pending_c, sc::U_CBUTTONS);
    }

    #[test]
    fn camera_style_setting_is_read() {
        let s = Settings { camera: CameraStyle::FreeOrbit, ..Settings::default() };
        assert_eq!(camera_style(Some(&s)), CameraStyle::FreeOrbit);
        assert_eq!(Settings::default().camera, CameraStyle::Original);
    }
}
