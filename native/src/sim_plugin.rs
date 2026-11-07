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
    }
}

fn tick(mut sim: ResMut<GameSim>, mut input: ResMut<PlayerInput>, rig: Res<CameraRig>, mut out: EventWriter<SimEventMsg>, mut control: ResMut<crate::experiments::StepControl>) {
    // An active experiment may replace the step's inputs or freeze the player (experiments/).
    sim.camera_yaw = control.camera_yaw.take().unwrap_or_else(|| rig.yaw_s16());
    sim.focus_held = input.focus_held;
    let controller = control.controller.take().unwrap_or_else(|| input.consume_step());
    if std::mem::take(&mut control.skip) {
        return;
    }
    sim.tick(&controller);
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
