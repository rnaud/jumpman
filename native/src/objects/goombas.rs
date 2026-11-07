//! Walking goombas (`goomba.ts`): stepped before the object pass, resolved against Mario after
//! his step, drawn as the goomba model scaled by 1.5 and squashed on a stomp.
use crate::game::goomba::{interact, spawn_goombas, GoombaEvent, WalkingGoomba, GOOMBA_MODEL, GOOMBA_SCALE};
use crate::game::runner::GameSim;
use crate::render::object_view::spawn_model;
use crate::render::{to_bevy, AssetRoot};
use crate::sim_plugin::{PostTickSet, PreTickSet, SimResetMsg};
use crate::progress::CourseScoped;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use jumpman_sim::object_manager::ObjectEvent;

pub struct GoombasPlugin;

impl Plugin for GoombasPlugin {
    fn build(&self, app: &mut App) {
        super::register_rebuild(app, rebuild);
        app.add_event::<GoombaFxMsg>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, (step_goombas.in_set(PreTickSet).after(super::dialog::TriggerSet), hit_goombas.in_set(PostTickSet)))
            .add_systems(Update, (reset_goombas, sync_views).chain().before(crate::CameraSet));
    }
}

#[derive(Resource)]
pub struct Goombas(pub Vec<WalkingGoomba>);

/// Goomba sound/particle cues (stomp, bonk, sparks, dust) for audio and particle modules.
#[derive(Event, Clone, Copy, Debug)]
pub struct GoombaFxMsg(pub GoombaEvent);

#[derive(Component)]
struct GoombaView(usize);

fn rebuild(world: &mut World) {
    let _ = world.run_system_once(setup);
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, sim: Res<GameSim>) {
    let goombas = spawn_goombas(&sim.course.extras.goombas, &sim.mario.world);
    if root.exists(GOOMBA_MODEL) {
        for (i, g) in goombas.iter().enumerate() {
            let mut e = spawn_model(&mut commands, &assets, &root, GOOMBA_MODEL, true);
            e.insert((CourseScoped, GoombaView(i), Transform::from_translation(to_bevy(g.pos.to_array())), Visibility::Hidden));
        }
    } else {
        warn!("goomba model missing");
    }
    commands.insert_resource(Goombas(goombas));
}

fn step_goombas(mut sim: ResMut<GameSim>, mut goombas: ResMut<Goombas>, xp: Res<crate::experiments::Shell>) {
    if xp.goombas_driven {
        return;
    }
    let sim = &mut *sim;
    let mario = sim.mario.pos;
    for g in &mut goombas.0 {
        if g.step(&mario, &sim.mario.world) {
            // `objects.spawnCoin(...)` happens right here, before this tick's object pass.
            sim.spawn_coin(g.pos.to_array(), true, 1.0);
        }
    }
}

fn hit_goombas(mut sim: ResMut<GameSim>, mut goombas: ResMut<Goombas>, mut fx: EventWriter<GoombaFxMsg>, xp: Res<crate::experiments::Shell>) {
    if xp.goombas_driven {
        return;
    }
    let sim = &mut *sim;
    let prev = sim.prev.pos;
    for e in interact(&mut goombas.0, &mut sim.mario, &prev) {
        if let GoombaEvent::Coin(pos) = e {
            sim.spawn_coin(pos, true, 1.0);
        }
        fx.write(GoombaFxMsg(e));
    }
}

fn reset_goombas(mut resets: EventReader<SimResetMsg>, mut goombas: ResMut<Goombas>) {
    if resets.read().count() > 0 {
        for g in &mut goombas.0 {
            g.reset();
        }
    }
}

fn sync_views(goombas: Res<Goombas>, mut views: Query<(&GoombaView, &mut Transform, &mut Visibility)>) {
    for (view, mut tf, mut vis) in &mut views {
        let Some(g) = goombas.0.get(view.0) else { continue };
        tf.translation = to_bevy(g.pos.to_array());
        tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(g.yaw) as f32);
        tf.scale = Vec3::splat(GOOMBA_SCALE as f32) * g.squash.as_vec3();
        *vis = if g.visible { Visibility::Inherited } else { Visibility::Hidden };
    }
}
