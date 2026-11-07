//! Entering the side-scroller: `Game.ts` starts the Dimensions experiment in 2D side mode
//! right after the `mushroom2d` course loads (the side camera is the experiment's).
use crate::experiments::{ExperimentId, ExperimentSlot};
use crate::game::runner::GameSim;
use crate::progress::CourseSwitched;
use bevy::prelude::*;

pub struct MushroomPlugin;

impl Plugin for MushroomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, enter_side_mode);
    }
}

/// Once per course load: request the Dimensions experiment while on `mushroom2d`.
fn enter_side_mode(sim: Res<GameSim>, mut switched: EventReader<CourseSwitched>, mut slot: ResMut<ExperimentSlot>, mut done: Local<bool>) {
    if switched.read().count() > 0 {
        *done = false;
    }
    if *done || sim.course.request.id != "mushroom2d" {
        return;
    }
    *done = true;
    if slot.active.is_none() && slot.pending.is_none() {
        slot.pending = Some(ExperimentId::Dimensions);
    }
}
