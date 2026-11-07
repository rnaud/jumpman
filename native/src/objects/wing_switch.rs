//! The Wing Cap switch in Bevy: pressing it makes every wing box solid (`WingBoxes`).
use super::colliders::{set_wing_boxes, WingBoxes};
use crate::game::runner::GameSim;
use crate::game::wing_switch::{WingSwitch, SWITCH_BASE, SWITCH_TOP};
use crate::hud::Toast;
use crate::render::{scene, to_bevy, AssetRoot};
use crate::sim_plugin::{PostTickSet, SimResetMsg};
use bevy::prelude::*;

pub struct WingSwitchPlugin;

impl Plugin for WingSwitchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(FixedUpdate, step_switch.in_set(PostTickSet))
            .add_systems(Update, (reset_switch, sync_top).chain().before(crate::CameraSet));
    }
}

#[derive(Resource)]
struct SwitchState(WingSwitch);

#[derive(Component)]
struct SwitchTop;

fn setup(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut sim: ResMut<GameSim>, wing: Res<WingBoxes>) {
    let sim = &mut *sim;
    let Some(pos) = sim.course.extras.wing_switch else { return };
    let (Some(top), Some(base)) = (sim.course.extras.extra_collision.get("switchTop"), sim.course.extras.extra_collision.get("switchBase")) else { return };
    let mut state = WingSwitch::new(pos, top, base);
    state.install(&mut sim.mario.world, wing.unlocked);
    if root.exists(SWITCH_BASE) {
        commands.spawn((scene(&assets, SWITCH_BASE), Transform::from_translation(to_bevy(pos)).with_scale(Vec3::splat(0.5))));
    }
    if root.exists(SWITCH_TOP) {
        commands.spawn((scene(&assets, SWITCH_TOP), Transform::from_translation(to_bevy(state.top_pos)).with_scale(Vec3::splat(0.5)), SwitchTop));
    }
    commands.insert_resource(SwitchState(state));
}

fn step_switch(mut sim: ResMut<GameSim>, state: Option<ResMut<SwitchState>>, mut wing: ResMut<WingBoxes>, mut toast: ResMut<Toast>) {
    let Some(mut state) = state else { return };
    if state.0.step(&mut sim.mario) {
        wing.unlocked = true;
        set_wing_boxes(&mut sim, true);
        toast.show("Wing Cap boxes are now solid!", 3.0);
    }
}

fn reset_switch(mut resets: EventReader<SimResetMsg>, mut sim: ResMut<GameSim>, state: Option<ResMut<SwitchState>>, wing: Res<WingBoxes>) {
    let Some(mut state) = state else { return };
    if resets.read().count() > 0 {
        state.0.reset(&mut sim.mario.world, wing.unlocked);
    }
}

fn sync_top(state: Option<Res<SwitchState>>, mut top: Query<&mut Transform, With<SwitchTop>>) {
    let Some(state) = state else { return };
    for mut tf in &mut top {
        tf.scale = Vec3::new(0.5, 0.5 * state.0.height.max(0.0) as f32, 0.5);
    }
}
