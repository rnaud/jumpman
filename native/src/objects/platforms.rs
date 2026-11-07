//! Moving platforms in Bevy: steps [`Platforms`] before the main tick (`Game.stepWorld` runs
//! `platform.step(mario)` ahead of the object pass) and draws the platform models.
use crate::game::platform::{LiftSound, Platforms};
use crate::game::runner::GameSim;
use crate::render::{scene, to_bevy, AssetRoot};
use crate::sim_plugin::{PreTickSet, SimResetMsg};
use crate::progress::CourseScoped;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

pub struct PlatformsPlugin;

impl Plugin for PlatformsPlugin {
    fn build(&self, app: &mut App) {
        super::register_rebuild(app, rebuild);
        app.add_event::<LiftSoundMsg>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, step_platforms.in_set(PreTickSet).after(super::dialog::TriggerSet))
            .add_systems(Update, (reset_platforms, sync_views).chain().before(crate::CameraSet));
    }
}

#[derive(Resource)]
pub struct PlatformWorld(pub Platforms);

/// A lift sound cue at a sim-space position (audio consumes this).
#[derive(Event, Clone, Debug)]
pub struct LiftSoundMsg {
    pub pos: [f64; 3],
    pub sound: LiftSound,
}

#[derive(Component)]
struct PlatformView(usize);

fn rebuild(world: &mut World) {
    let _ = world.run_system_once(setup);
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut sim: ResMut<GameSim>) {
    let mut platforms = Platforms::new(&sim.course.extras.platforms);
    platforms.install(&mut sim.mario);
    for (i, p) in platforms.list.iter().enumerate() {
        if !root.exists(&p.spec.model) {
            warn!("platform model missing: {}", p.spec.model);
            continue;
        }
        commands.spawn((scene(&assets, &p.spec.model), Transform::default(), Visibility::Hidden, PlatformView(i), CourseScoped));
    }
    commands.insert_resource(PlatformWorld(platforms));
}

fn step_platforms(mut sim: ResMut<GameSim>, mut platforms: ResMut<PlatformWorld>, mut sounds: EventWriter<LiftSoundMsg>) {
    for (pos, sound) in platforms.0.step(&mut sim.mario) {
        sounds.write(LiftSoundMsg { pos: pos.to_array(), sound });
    }
}

fn reset_platforms(mut resets: EventReader<SimResetMsg>, mut sim: ResMut<GameSim>, mut platforms: ResMut<PlatformWorld>) {
    if resets.read().count() > 0 {
        platforms.0.reset(&mut sim.mario);
    }
}

fn sync_views(platforms: Res<PlatformWorld>, fixed: Res<Time<Fixed>>, mut views: Query<(&PlatformView, &mut Transform, &mut Visibility)>) {
    let t = fixed.overstep_fraction() as f64;
    for (view, mut tf, mut vis) in &mut views {
        let Some(p) = platforms.0.list.get(view.0) else { continue };
        let jump = (p.pos - p.prev_pos).abs().max_element() > 300.0;
        let (pos, from_rot) = if jump { (p.pos, p.rot) } else { (p.prev_pos.lerp(p.pos, t), p.prev_rot) };
        let quat = |r: bevy::math::DVec3| Quat::from_euler(EulerRot::YXZ, r.y as f32, r.x as f32, r.z as f32);
        tf.translation = to_bevy(pos.to_array());
        tf.rotation = quat(from_rot).slerp(quat(p.rot), if jump { 1.0 } else { t as f32 });
        tf.scale = p.scale.as_vec3();
        *vis = if p.visible { Visibility::Inherited } else { Visibility::Hidden };
    }
}
