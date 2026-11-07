//! Drives [`GameSim`] at a fixed 30 Hz and re-emits what happened as Bevy events so feature
//! modules (audio, particles, dialogs, warps...) can react without touching the runner.
use crate::camera::CameraRig;
use crate::game::runner::{GameSim, SimEvent};
use crate::input::PlayerInput;
use bevy::prelude::*;

/// One sim event from the last 30 Hz tick (Mario sounds, object events, coin pickups).
#[derive(Event, Clone, Debug)]
pub struct SimEventMsg(pub SimEvent);

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(30.))
            .add_event::<SimEventMsg>()
            .add_systems(FixedUpdate, tick.in_set(crate::SimSet))
            .add_systems(Update, handle_reset.before(crate::CameraSet));
    }
}

fn tick(mut sim: ResMut<GameSim>, mut input: ResMut<PlayerInput>, rig: Res<CameraRig>, mut out: EventWriter<SimEventMsg>) {
    sim.camera_yaw = rig.yaw_s16();
    let controller = input.consume_step();
    sim.tick(&controller);
    for e in sim.events.drain(..) {
        out.write(SimEventMsg(e));
    }
}

fn handle_reset(mut input: ResMut<PlayerInput>, mut sim: ResMut<GameSim>, mut rig: ResMut<CameraRig>) {
    if input.reset {
        input.reset = false;
        sim.reset();
        rig.snap_behind(sim.mario.face_angle[1]);
    }
}
