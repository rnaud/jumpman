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

pub struct GoombasPlugin;

impl Plugin for GoombasPlugin {
    fn build(&self, app: &mut App) {
        super::register_rebuild(app, rebuild);
        app.add_message::<GoombaFxMsg>()
            .add_systems(Startup, setup)
            .add_systems(FixedFirst, remember_previous)
            .add_systems(FixedUpdate, (step_goombas.in_set(PreTickSet).after(super::dialog::TriggerSet), hit_goombas.in_set(PostTickSet)))
            .add_systems(Update, (reset_goombas, sync_views).chain().before(crate::CameraSet));
    }
}

#[derive(Resource)]
pub struct Goombas(pub Vec<WalkingGoomba>);

/// Position and yaw each goomba had before the latest 30 Hz step, for render interpolation.
#[derive(Resource, Default)]
struct GoombaPrev(Vec<([f64; 3], f64)>);

/// Blend between two sim states `t` (0..1) of the way; a jump over `snap` units (respawn, teleport)
/// is not blended. Angles are s16 and take the short way round.
pub fn interpolate_pose(prev: ([f64; 3], f64), cur: ([f64; 3], f64), t: f64) -> ([f64; 3], f64) {
    let d = [0, 1, 2].map(|i| cur.0[i] - prev.0[i]);
    if d.iter().map(|v| v * v).sum::<f64>().sqrt() > 400.0 {
        return cur;
    }
    let pos = [0, 1, 2].map(|i| prev.0[i] + d[i] * t);
    let dy = jumpman_sim::math::to_s16(cur.1 - prev.1) as f64;
    (pos, prev.1 + dy * t)
}

fn remember_previous(goombas: Option<Res<Goombas>>, mut prev: ResMut<GoombaPrev>) {
    let Some(goombas) = goombas else { return };
    prev.0.clear();
    prev.0.extend(goombas.0.iter().map(|g| (g.pos.to_array(), g.yaw)));
}

/// Goomba sound/particle cues (stomp, bonk, sparks, dust) for audio and particle modules.
#[derive(Message, Clone, Copy, Debug)]
pub struct GoombaFxMsg(pub GoombaEvent);

#[derive(Component)]
pub struct GoombaView(usize);

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
    commands.insert_resource(GoombaPrev(goombas.iter().map(|g| (g.pos.to_array(), g.yaw)).collect()));
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
    // Link's Z-focus can target the goombas that are tangible at this point of the tick.
    sim.focus_goombas = crate::game::goomba::focus_targets(&goombas.0);
}

fn hit_goombas(mut sim: ResMut<GameSim>, mut goombas: ResMut<Goombas>, mut fx: MessageWriter<GoombaFxMsg>, xp: Res<crate::experiments::Shell>) {
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

fn reset_goombas(mut resets: MessageReader<SimResetMsg>, mut goombas: ResMut<Goombas>) {
    if resets.read().count() > 0 {
        for g in &mut goombas.0 {
            g.reset();
        }
    }
}

fn sync_views(goombas: Res<Goombas>, prev: Res<GoombaPrev>, fixed: Res<Time<Fixed>>, mut views: Query<(&GoombaView, &mut Transform, &mut Visibility)>) {
    let t = fixed.overstep_fraction() as f64;
    for (view, mut tf, mut vis) in &mut views {
        let Some(g) = goombas.0.get(view.0) else { continue };
        let cur = (g.pos.to_array(), g.yaw);
        let (pos, yaw) = interpolate_pose(prev.0.get(view.0).copied().unwrap_or(cur), cur, t);
        tf.translation = to_bevy(pos);
        tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(yaw) as f32);
        tf.scale = Vec3::splat(GOOMBA_SCALE as f32) * g.squash.as_vec3();
        *vis = if g.visible { Visibility::Inherited } else { Visibility::Hidden };
    }
}

#[cfg(test)]
mod tests {
    use super::interpolate_pose;

    #[test]
    fn render_pose_blends_between_sim_steps() {
        let (p, y) = interpolate_pose(([0.0, 0.0, 0.0], 0.0), ([10.0, 0.0, 20.0], 1000.0), 0.5);
        assert_eq!((p, y), ([5.0, 0.0, 10.0], 500.0));
        // Short way round the s16 wrap.
        let (_, y) = interpolate_pose(([0.0; 3], 65000.0), ([0.0; 3], 200.0), 0.5);
        assert!((y - (65000.0 + 368.0)).abs() < 1.0, "{y}");
        // A respawn is not smeared across the map.
        let (p, _) = interpolate_pose(([0.0; 3], 0.0), ([5000.0, 0.0, 0.0], 0.0), 0.5);
        assert_eq!(p[0], 5000.0);
    }
}
