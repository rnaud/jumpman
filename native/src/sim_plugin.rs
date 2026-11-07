//! Drives [`GameSim`] at a fixed 30 Hz and re-emits what happened as Bevy events so feature
//! modules (audio, particles, dialogs, warps...) can react without touching the runner.
use crate::camera::CameraRig;
use crate::game::runner::{GameSim, SimEvent};
use crate::input::PlayerInput;
use bevy::prelude::*;

/// One sim event from the last 30 Hz tick (Mario sounds, object events, coin pickups).
#[derive(Event, Clone, Debug)]
pub struct SimEventMsg(pub SimEvent);

/// The sim was restarted at the course spawn (R key); feature modules reset their own state.
#[derive(Event, Clone, Copy, Debug)]
pub struct SimResetMsg;

/// While true the regular 30 Hz tick (and everything in [`PreTickSet`]) is replaced by a modal
/// step owned by a feature module (sign reading, NPC dialogs, cannon operator).
#[derive(Resource, Default)]
pub struct ModalActive(pub bool);

/// Systems that run before the main tick (moving platforms, ...); skipped during modals.
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct PreTickSet;

/// Systems that run after the main tick (object colliders, goomba interaction, ...).
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct PostTickSet;

pub struct SimPlugin;

fn not_modal(modal: Res<ModalActive>) -> bool {
    !modal.0
}

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(30.))
            .init_resource::<ModalActive>()
            .add_event::<SimEventMsg>()
            .add_event::<SimResetMsg>()
            .configure_sets(FixedUpdate, (PreTickSet.run_if(not_modal).run_if(crate::progress::sim_running).before(tick), PostTickSet.run_if(not_modal).run_if(crate::progress::sim_running).after(tick)))
            .add_systems(FixedUpdate, tick.in_set(crate::SimSet).run_if(not_modal).run_if(crate::progress::sim_running))
            .add_systems(Update, handle_reset.before(crate::CameraSet));
        if std::env::var_os("JUMPMAN_FRAME_LOG").is_some() {
            app.init_resource::<FrameLog>()
                .add_systems(FixedUpdate, |mut l: ResMut<FrameLog>| l.steps += 1)
                .add_systems(Last, frame_log);
        }
    }
}

/// Debug (`JUMPMAN_FRAME_LOG=1`): per-frame dt, fixed steps run, overstep fraction, and the
/// rendered Mario / camera positions with their per-frame deltas, to spot judder.
#[derive(Resource, Default)]
struct FrameLog {
    steps: u32,
    last_mario: Option<Vec3>,
    last_cam: Option<Vec3>,
}

fn frame_log(
    time: Res<Time<Real>>,
    fixed: Res<Time<Fixed>>,
    mut log: ResMut<FrameLog>,
    mario: Option<Single<&Transform, With<crate::render::mario_view::MarioRoot>>>,
    cam: Option<Single<&Transform, With<crate::camera::MainCamera>>>,
) {
    let (Some(mario), Some(cam)) = (mario, cam) else { return };
    let (m, c) = (mario.translation, cam.translation);
    let dm = log.last_mario.map_or(Vec3::ZERO, |l| m - l);
    let dc = log.last_cam.map_or(Vec3::ZERO, |l| c - l);
    println!(
        "frame dt={:.2}ms steps={} frac={:.2} mario=({:.3},{:.3},{:.3}) dmario={:.4} dcam={:.4}",
        time.delta_secs() * 1000.0,
        log.steps,
        fixed.overstep_fraction(),
        m.x,
        m.y,
        m.z,
        dm.length(),
        dc.length()
    );
    log.steps = 0;
    log.last_mario = Some(m);
    log.last_cam = Some(c);
}

#[allow(clippy::too_many_arguments)]
fn tick(
    mut sim: ResMut<GameSim>,
    mut input: ResMut<PlayerInput>,
    rig: Res<CameraRig>,
    mut out: EventWriter<SimEventMsg>,
    mut control: ResMut<crate::experiments::StepControl>,
    colliders: Option<ResMut<crate::objects::colliders::ObjectColliders>>,
    cannons: Option<ResMut<crate::objects::cannons::CannonSet>>,
    progress: Res<crate::progress::flow::Progress>,
    mut modal: ResMut<crate::objects::dialog::ModalState>,
    mut active: ResMut<ModalActive>,
) {
    // An active experiment may replace the step's inputs or freeze the player (experiments/).
    sim.camera_yaw = control.camera_yaw.take().unwrap_or_else(|| rig.yaw_s16());
    sim.focus_held = input.focus_held;
    let controller = control.controller.take().unwrap_or_else(|| input.consume_step());
    if std::mem::take(&mut control.skip) {
        return;
    }
    let course = sim.course.request.id.clone();
    let mut colliders = colliders;
    let mut cannons = cannons;
    let mut hooks = crate::objects::stage::StageHooks {
        colliders: colliders.as_deref_mut(),
        cannons: cannons.as_deref_mut().map(|c| &mut c.0),
        wing_unlocked: progress.save.wing_switch_on(),
        course_id: &course,
        cannon_unlocked: progress.save.cannon_open(&course),
        operator: None,
    };
    sim.tick_with(&controller, &mut hooks);
    // The Bob-omb buddy started a cannon operator: this tick ended before Mario's step and the
    // operator's first step is the next tick's (`Game.stepWorld` returns right after `new CannonOperator`).
    if let Some(op) = hooks.operator.take() {
        crate::objects::dialog::start_operator(&mut modal, &mut active, op);
    }
    for e in sim.events.drain(..) {
        out.write(SimEventMsg(e));
    }
}

pub fn handle_reset(mut input: ResMut<PlayerInput>, mut sim: ResMut<GameSim>, mut rig: ResMut<CameraRig>, mut resets: EventWriter<SimResetMsg>) {
    if input.reset {
        input.reset = false;
        sim.reset();
        rig.snap_behind(sim.mario.face_angle[1]);
        resets.write(SimResetMsg);
    }
}
