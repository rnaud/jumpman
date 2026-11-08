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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::{CourseRequest, LoadedCourse};
    use jumpman_sim::solids::SOLID_TAG;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn build(id: &str, speed: i32) -> (LoadedCourse, jumpman_sim::mario_runtime::MarioRuntime, Solids) {
        let course = LoadedCourse::load(&assets(), CourseRequest::new(id)).unwrap();
        let mut m = jumpman_sim::mario_runtime::MarioRuntime::new(course.world.clone(), course.spawn.clone(), course.death_height);
        let meshes: HashMap<String, SolidMesh> = course
            .extras
            .extra_collision
            .iter()
            .map(|(k, c)| (k.clone(), SolidMesh { vertices: c.vertices.clone(), triangles: c.triangles.clone() }))
            .collect();
        let mut solids = Solids::new(&course.extras.solids, &meshes, speed);
        solids.install(&mut m);
        (course, m, solids)
    }

    /// Every exported surface object of the SM64 stages has a behavior, a model on disk and
    /// collision that lands in the world under its tag.
    #[test]
    fn every_exported_surface_object_is_ported_and_has_collision() {
        for id in ["ttc", "rr", "bitdw", "bitfs", "bits", "bowser_2", "bowser_3"] {
            let (course, m, mut solids) = build(id, 0);
            assert!(!course.extras.solids.is_empty(), "{id} exports no solids");
            assert_eq!(solids.unported(), 0, "{id}: unported behaviors");
            for (i, s) in solids.list.iter().enumerate() {
                if !s.spec.model.is_empty() {
                    assert!(assets().join(&s.spec.model).is_file(), "{id}: missing model {}", s.spec.model);
                }
                if s.spec.collision.is_some() {
                    let tag = SOLID_TAG + i as u32;
                    assert!(m.world.surfaces().iter().any(|f| f.object == Some(tag)), "{id}: {} has no collision", s.spec.behavior);
                }
            }
            let mut m = m;
            let n = m.world.surfaces().len();
            for _ in 0..300 {
                solids.step(&mut m);
            }
            let after = m.world.surfaces().len();
            assert!((after as i64 - n as i64).abs() < 400, "{id}: surface count drifted {n} -> {after}");
        }
    }

    #[test]
    fn tick_tock_clock_settings_change_the_objects() {
        for speed in 0..4 {
            let (_, mut m, mut solids) = build("ttc", speed);
            let before: Vec<_> = solids.list.iter().map(|s| (s.o.pos, s.o.face)).collect();
            for _ in 0..400 {
                solids.step(&mut m);
            }
            let moved = solids.list.iter().zip(&before).filter(|(s, b)| (s.o.pos, s.o.face) != **b).count();
            if speed == jumpman_sim::solids::TTC_SPEED_STOPPED {
                // the stopped setting only extends the 12 push blocks to their 250-unit offset on the
                // first frame (`bhv_ttc_moving_bar_init`) and parks the pit block at the top
                let names: Vec<_> = solids.list.iter().zip(&before).filter(|(s, b)| (s.o.pos, s.o.face) != **b).map(|(s, _)| s.spec.behavior.as_str()).collect();
                assert!(names.iter().all(|n| matches!(*n, "bhvTTCMovingBar" | "bhvTTCPendulum")), "stopped clock moved {names:?}");
            } else {
                assert!(moved > 20, "speed {speed}: only {moved} objects moved");
            }
        }
    }

    /// Bowser in the Fire Sea: when he lands from his big jump he tilts the lava platform
    /// (`bowser_act_tilt_lava_platform`), then it snaps level again and he walks on.
    #[test]
    fn bowser_tilts_the_fire_sea_platform() {
        use jumpman_sim::behaviors::bowser_sm64 as bw;
        use jumpman_sim::object_manager;
        let mut course = LoadedCourse::load(&assets(), CourseRequest::new("bowser_2")).unwrap();
        let mut m = jumpman_sim::mario_runtime::MarioRuntime::new(course.world.clone(), course.spawn.clone(), course.death_height);
        m.set_objects(std::mem::take(&mut course.objects));
        let meshes: HashMap<String, SolidMesh> = course
            .extras
            .extra_collision
            .iter()
            .map(|(k, c)| (k.clone(), SolidMesh { vertices: c.vertices.clone(), triangles: c.triangles.clone() }))
            .collect();
        let mut solids = Solids::new(&course.extras.solids, &meshes, 0);
        solids.install(&mut m);
        let bowser = m.objects.objects.iter().position(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::SmBowser).unwrap();
        for _ in 0..5 {
            object_manager::step(&mut m, None, false);
            solids.step(&mut m);
        }
        m.objects.objects[bowser].action = bw::ACT_TILT_LAVA_PLATFORM;
        let mut max_pitch: f64 = 0.0;
        let mut max_roll: f64 = 0.0;
        for _ in 0..260 {
            // the arena camera would keep Mario out of the way
            m.pos = [0.0, 3000.0, 0.0];
            object_manager::step(&mut m, None, false);
            solids.step(&mut m);
            max_pitch = max_pitch.max(solids.list[0].o.face[0].abs());
            max_roll = max_roll.max(solids.list[0].o.face[2].abs());
        }
        assert!(max_pitch.max(max_roll) > 1000.0, "platform never tilted: {max_pitch} {max_roll}");
        // sBowsertiltPlatformData ends at frame 186: afterwards the angles are reset to 0
        assert_eq!(solids.list[0].o.face[0], 0.0);
        assert_eq!(solids.list[0].o.face[2], 0.0);
        assert_ne!(m.objects.objects[bowser].action, bw::ACT_TILT_LAVA_PLATFORM);
    }

    /// Bowser in the Sky: a platform falls when Bowser lands from a big jump on it; the rest of the
    /// arena follows once he is down to one health (timers of 20 * (n - 1) frames).
    #[test]
    fn falling_platforms_follow_bowsers_big_jump() {
        use jumpman_sim::behaviors::bowser_sm64 as bw;
        use jumpman_sim::object_manager;
        let mut course = LoadedCourse::load(&assets(), CourseRequest::new("bowser_3")).unwrap();
        let mut m = jumpman_sim::mario_runtime::MarioRuntime::new(course.world.clone(), course.spawn.clone(), course.death_height);
        m.set_objects(std::mem::take(&mut course.objects));
        let meshes: HashMap<String, SolidMesh> = course
            .extras
            .extra_collision
            .iter()
            .map(|(k, c)| (k.clone(), SolidMesh { vertices: c.vertices.clone(), triangles: c.triangles.clone() }))
            .collect();
        let mut solids = Solids::new(&course.extras.solids, &meshes, 0);
        solids.install(&mut m);
        assert_eq!(solids.list.len(), 10);
        let bowser = m.objects.objects.iter().position(|o| o.behavior == jumpman_sim::object::ObjectBehaviorKind::SmBowser).unwrap();
        m.pos = [0.0, 3000.0, 0.0];
        for _ in 0..10 {
            object_manager::step(&mut m, None, false);
            solids.step(&mut m);
        }
        // all platforms start in the check action; none fell
        assert!(solids.list.iter().all(|s| s.o.action == 1 && s.o.pos[1] == 0.0));
        // Bowser on health 1 dancing: the timers run and platform n falls after 20 * (n - 1) frames
        {
            let b = &mut m.objects.objects[bowser];
            b.set_var("health", 1.0);
            b.action = bw::ACT_DANCE;
        }
        for _ in 0..70 {
            m.objects.objects[bowser].action = bw::ACT_DANCE;
            solids.step(&mut m);
        }
        let fallen: Vec<usize> = (0..10).filter(|&i| solids.list[i].o.action == 2).collect();
        assert!(fallen.len() >= 3 && fallen.len() <= 5, "fallen after 70 frames: {fallen:?}");
        assert!(fallen.contains(&0));
        // a platform that fell rises 8 per frame for 22 frames, then drops
        let p = &solids.list[0].o;
        assert!(p.pos[1] > 0.0 || p.timer > 22);
    }

    #[test]
    fn probe_floors_for_screenshots() {
        let (_, m, solids) = build("ttc", 0);
        for s in &solids.list {
            if matches!(s.spec.behavior.as_str(), "bhvTTCRotatingSolid" | "bhvTTCPendulum" | "bhvTTCCog" | "bhvTTCSpinner" | "bhvTTCElevator") {
                let h = m.world.find_floor(s.o.pos[0], s.o.pos[1] + 400.0, s.o.pos[2]);
                eprintln!("{} at {:?} floor {} tag {:?}", s.spec.behavior, s.o.pos, h.height, h.surface.map(|x| x.object));
            }
        }
    }
}
