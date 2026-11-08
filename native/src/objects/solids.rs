//! SM64 surface objects in Bevy (`jumpman_sim::solids`): steps the moving solids of Tick Tock
//! Clock, Rainbow Ride and the Bowser stages before the main tick and draws their models.
//!
//! * `JUMPMAN_TTC_SPEED=0..3` forces `gTTCSpeedSetting` (slow, fast, random, stopped).
//! * Otherwise the speed comes from the castle clock's minute hand, which is simulated as
//!   `-0x180` roll per tick of castle time (`clock_arm.inc.c`, `bhvClockMinuteHand`).
use crate::game::runner::GameSim;
use crate::progress::CourseScoped;
use crate::render::{scene, to_bevy, AssetRoot};
use crate::sim_plugin::{PreTickSet, SimResetMsg};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use jumpman_sim::math::to_s16;
use jumpman_sim::solids::{ttc, SolidMesh, Solids};
use std::collections::HashMap;

pub struct SolidsPlugin;

impl Plugin for SolidsPlugin {
    fn build(&self, app: &mut App) {
        super::register_rebuild(app, rebuild);
        app.init_resource::<CastleClock>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, step_solids.in_set(PreTickSet).after(super::dialog::TriggerSet))
            .add_systems(Update, (reset_solids, sync_views).chain().before(crate::CameraSet));
    }
}

/// Frames spent outside Tick Tock Clock: the minute hand of the castle clock painting rotates only
/// while the castle is loaded, and the setting is read when Mario enters the painting.
#[derive(Resource, Default)]
pub struct CastleClock {
    pub frames: i64,
}

#[derive(Resource)]
pub struct SolidWorld(pub Solids);

#[derive(Component)]
struct SolidView(usize);

fn rebuild(world: &mut World) {
    let _ = world.run_system_once(setup);
}

/// `gTTCSpeedSetting` for a fresh TTC load.
pub fn ttc_speed(clock: &CastleClock) -> i32 {
    if let Ok(v) = std::env::var("JUMPMAN_TTC_SPEED") {
        if let Ok(n) = v.trim().parse::<i32>() {
            return n.clamp(0, 3);
        }
    }
    ttc::speed_from_minute_hand(ttc::minute_hand_roll(clock.frames))
}

fn setup(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut sim: ResMut<GameSim>, clock: Res<CastleClock>) {
    let extras = &sim.course.extras;
    let specs = extras.solids.clone();
    let meshes: HashMap<String, SolidMesh> = extras
        .extra_collision
        .iter()
        .map(|(k, c)| (k.clone(), SolidMesh { vertices: c.vertices.clone(), triangles: c.triangles.clone() }))
        .collect();
    ttc::reset_shared();
    let mut solids = Solids::new(&specs, &meshes, ttc_speed(&clock));
    if solids.unported() > 0 {
        warn!("{} surface objects have no behavior yet", solids.unported());
    }
    solids.install(&mut sim.mario);
    for (i, s) in solids.list.iter().enumerate() {
        let model = s.model();
        if model.is_empty() {
            continue;
        }
        if !root.exists(model) {
            warn!("surface object model missing: {model}");
            continue;
        }
        commands.spawn((scene(&assets, model), Transform::default(), Visibility::Hidden, SolidView(i), CourseScoped));
    }
    commands.insert_resource(SolidWorld(solids));
}

fn step_solids(mut sim: ResMut<GameSim>, mut solids: ResMut<SolidWorld>, mut clock: ResMut<CastleClock>) {
    if solids.0.list.is_empty() {
        clock.frames += 1;
        return;
    }
    solids.0.step(&mut sim.mario);
}

fn reset_solids(mut resets: MessageReader<SimResetMsg>, mut sim: ResMut<GameSim>, mut solids: ResMut<SolidWorld>) {
    if resets.read().count() > 0 {
        solids.0.reset(&mut sim.mario);
    }
}

fn rotation(angles: [f64; 3]) -> Quat {
    let r = |a: f64| to_s16(a) as f32 * std::f32::consts::PI / 32768.0;
    Quat::from_euler(EulerRot::YXZ, r(angles[1]), r(angles[0]), r(angles[2]))
}

fn sync_views(solids: Res<SolidWorld>, fixed: Res<Time<Fixed>>, mut views: Query<(&SolidView, &mut Transform, &mut Visibility)>) {
    let t = fixed.overstep_fraction();
    for (view, mut tf, mut vis) in &mut views {
        let Some(s) = solids.0.list.get(view.0) else { continue };
        let jump = (0..3).any(|i| (s.o.pos[i] - s.prev_pos[i]).abs() > 300.0);
        let pos = if jump { s.o.pos } else { std::array::from_fn(|i| s.prev_pos[i] + (s.o.pos[i] - s.prev_pos[i]) * t as f64) };
        tf.translation = to_bevy(pos);
        tf.rotation = match &s.o.transform {
            Some(m) => {
                let col = |r: usize| Vec3::new(m[r][0] as f32, m[r][1] as f32, m[r][2] as f32).normalize_or_zero();
                Quat::from_mat3(&Mat3::from_cols(col(0), col(1), col(2)))
            }
            None => {
                let from = rotation(s.prev_face);
                let to = rotation(s.o.face);
                if jump { to } else { from.slerp(to, t) }
            }
        };
        tf.scale = Vec3::new(s.o.scale3[0] as f32, s.o.scale3[1] as f32, s.o.scale3[2] as f32) * s.o.scale as f32;
        *vis = if s.o.visible && !s.o.deleted { Visibility::Inherited } else { Visibility::Hidden };
    }
}
