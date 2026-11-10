//! Progression and flow: warps, doors, painting entrances, missions/acts, stars, save data, the
//! course result, lives/game over and the screen transitions between courses.
//!
//! * `save` / `data` / `paintings` / `doors` / `wing` / `stars` / `menus` / `wipe` / `flow` are
//!   engine-free ports (unit tested). `flow::Progress` is the state machine.
//! * This file is the Bevy glue: it steps `Progress` around each 30 Hz sim tick, performs course
//!   switches (replacing the `GameSim` resource and rebuilding course-scoped views), and installs
//!   the overlay / menu / star / painting views from `ui.rs` and `views.rs`.
//!
//! Course switching convention for other feature modules: anything you spawn that belongs to the
//! loaded course should carry [`CourseScoped`] (it is despawned on a switch), and the system that
//! (re)builds it can be registered with [`CourseRebuild::push`] so it runs after each switch.
pub mod data;
pub mod doors;
pub mod coin_fx;
pub mod castle_npcs;
mod cap_view;
pub mod credits;
pub mod cutscene_glue;
pub mod ending;
mod shots;
pub mod flow;
pub mod menus;
pub mod paintings;
pub mod save;
pub mod stars;
mod audio_glue;
mod ui;
mod views;
pub mod wing;
#[cfg(test)]
mod sm64_progress_tests;
pub mod wipe;

use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::render::course_view::spawn_course;
use crate::render::object_view::{ObjectView, ObjectViews};
use crate::render::pickup_view::{PickupView, PickupViews};
use crate::render::AssetRoot;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use flow::Progress;
use jumpman_sim::state::ControllerSnapshot;
use std::collections::HashSet;

/// Startup systems that spawn long-lived entities (HUD, camera, Mario, overlays...) join this set so they exist
/// before `mark_entities` and are never tagged [`CourseScoped`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PersistentSpawn;

/// Marks entities that belong to the currently loaded course and must be despawned on a switch.
#[derive(Component)]
pub struct CourseScoped;

/// Systems that rebuild course-dependent views after a switch (run with exclusive world access).
#[derive(Resource, Default)]
pub struct CourseRebuild(pub Vec<fn(&mut World)>);

impl CourseRebuild {
    pub fn push(&mut self, f: fn(&mut World)) {
        self.0.push(f);
    }
}

/// A cue for the audio layer (names like `"star"`, `"menu_move"`, `"death_warp"`).
#[derive(Message, Clone, Debug)]
pub struct ProgressCue(pub &'static str);

/// Emitted after a course switch (new `GameSim`).
#[derive(Message, Clone, Debug)]
pub struct CourseSwitched;

pub struct ProgressPlugin;

/// Builds `Progress` from the save file and the loaded `GameSim` (and applies the saved star count to it).
fn init_progress(world: &mut World) {
    let root = world.resource::<AssetRoot>().0.clone();
    let dir = save::data_dir();
    let slot = save::active_slot(&dir);
    let mut sim = world.remove_resource::<GameSim>().expect("GameSim must be loaded before the progress state");
    let mut progress = Progress::new(&mut sim, &root, save::SaveFile::open(&dir, slot));
    sim.stars = progress.save.stars();
    // Debug hook: JUMPMAN_FLOW=selector|result|gameover|bonus|death|star|doorstar
    if let Ok(demo) = std::env::var("JUMPMAN_FLOW") {
        views::debug_flow(&demo, &mut sim, &mut progress);
    }
    world.insert_resource(sim);
    world.insert_resource(progress);
}

impl Plugin for ProgressPlugin {
    fn build(&self, app: &mut App) {
        // Dormant build (the game waits for the ROM import): the save/progress state needs the course data, so it is created at boot.
        if app.world().contains_resource::<GameSim>() {
            init_progress(app.world_mut());
        } else {
            crate::on_boot(app, init_progress);
        }
        app.init_resource::<CourseRebuild>()
            .add_message::<ProgressCue>()
            .add_message::<CourseSwitched>()
            .add_plugins((ui::ProgressUiPlugin, views::ProgressViewsPlugin, ending::EndingPlugin, cutscene_glue::CutsceneGluePlugin, credits::CreditsPlugin, shots::ShotsPlugin, coin_fx::CoinFxPlugin, audio_glue::ProgressAudioPlugin, cap_view::CapSwitchViewPlugin))
            .add_systems(Startup, (mark_entities.before(spawn_course).after(PersistentSpawn), tag_course_entities.after(spawn_course)))
            .add_systems(FixedUpdate, flow_pre.before(crate::SimSet).before(crate::sim_plugin::PreTickSet).run_if(crate::shell::is_playing))
            .add_systems(FixedUpdate, flow_post.after(crate::SimSet).run_if(crate::shell::is_playing))
            .add_systems(Update, (first_person_camera, apply_rig_snap).before(crate::CameraSet))
            .add_systems(Update, (reset_progress.before(crate::sim_plugin::handle_reset), apply_area_switch, apply_switch.before(crate::CameraSet)).chain().run_if(resource_exists::<Progress>));
    }
}

/// Run condition for `SimPlugin::tick`: the sim is paused during menus, wipes and star reveals.
pub fn sim_running(progress: Res<Progress>) -> bool {
    progress.sim_running()
}

#[derive(Resource)]
struct EntitiesBefore(HashSet<Entity>);

fn mark_entities(world: &mut World) {
    let set = world.iter_entities().map(|e| e.id()).collect();
    world.insert_resource(EntitiesBefore(set));
}

fn tag_course_entities(world: &mut World) {
    tag_new(world);
}

fn tag_new(world: &mut World) {
    let Some(before) = world.remove_resource::<EntitiesBefore>() else { return };
    let new: Vec<Entity> = world.iter_entities().filter(|e| !e.contains::<bevy::ecs::resource::IsResource>()).map(|e| e.id()).filter(|e| !before.0.contains(e)).collect();
    for e in new {
        world.entity_mut(e).insert(CourseScoped);
    }
}

fn flow_pre(mut sim: ResMut<GameSim>, mut progress: ResMut<Progress>, input: Res<PlayerInput>) {
    progress.look_input = (input.looking_up, input.head_turn);
    progress.pre_tick(&mut sim);
}

fn flow_post(
    mut sim: ResMut<GameSim>,
    mut progress: ResMut<Progress>,
    mut input: ResMut<PlayerInput>,
    mut toast: ResMut<crate::hud::Toast>,
    mut cues: MessageWriter<ProgressCue>,
    mut sfx: MessageWriter<crate::audio::SfxCue>,
) {
    let controller = if progress.wants_input() { input.consume_step() } else { ControllerSnapshot::default() };
    progress.post_tick(&mut sim, &controller);
    for (text, seconds) in progress.toasts.drain(..) {
        toast.show(text, seconds);
    }
    for cue in progress.sfx.drain(..) {
        sfx.write(cue);
    }
    for cue in progress.cues.drain(..) {
        cues.write(ProgressCue(cue));
    }
}

/// `R`: restart the run in the current course (`Game.restart`).
fn reset_progress(input: Res<PlayerInput>, mut sim: ResMut<GameSim>, mut progress: ResMut<Progress>) {
    if input.reset {
        progress.restart(&mut sim);
    }
}

/// `changeLinkedArea`: swap to the other area of a ddd/ccm style course without a loading screen.
fn apply_area_switch(world: &mut World) {
    let Some(change) = world.resource_mut::<Progress>().pending_area.take() else { return };
    let root = world.resource::<AssetRoot>().0.clone();
    let mut sim = world.remove_resource::<GameSim>().expect("GameSim");
    let mut progress = world.remove_resource::<Progress>().expect("Progress");
    let delta = change.delta;
    let result = progress.change_area(&mut sim, &root, change);
    let ok = result.is_ok();
    if let Err(e) = result {
        error!("area switch failed: {e}");
    }
    world.insert_resource(sim);
    world.insert_resource(progress);
    if !ok {
        return;
    }
    if let Some(mut rig) = world.get_resource_mut::<crate::camera::CameraRig>() {
        rig.displace(bevy::math::DVec3::from(delta));
    }
    rebuild_course_views(world);
    world.write_message(CourseSwitched);
}

/// Fading warps (`stepFadingWarp`): the camera snaps behind Mario at the destination pad.
fn apply_rig_snap(mut progress: ResMut<Progress>, mut rig: ResMut<crate::camera::CameraRig>) {
    if let Some(yaw) = progress.rig_snap.take() {
        rig.snap_behind(yaw);
    }
}

fn apply_switch(world: &mut World) {
    if world.resource::<Progress>().pending.is_none() {
        return;
    }
    let root = world.resource::<AssetRoot>().0.clone();
    let mut sim = world.remove_resource::<GameSim>().expect("GameSim");
    let mut progress = world.remove_resource::<Progress>().expect("Progress");
    let old_title = sim.course.name.clone();
    let result = progress.perform_switch(&mut sim, &root);
    let ok = match &result {
        Ok(_) => true,
        Err(e) => {
            error!("course load failed: {e}");
            progress.flow = flow::Flow::Playing;
            progress.toasts.push(("COURSE COULD NOT LOAD".into(), 5.0));
            false
        }
    };
    let new_title = course_title(&sim);
    let face = sim.mario.face_angle[1];
    world.insert_resource(sim);
    world.insert_resource(progress);
    if !ok {
        return;
    }
    if let Some(mut rig) = world.get_resource_mut::<crate::camera::CameraRig>() {
        rig.snap_behind(face);
    }
    rebuild_course_views(world);
    // The HUD swaps the curly apostrophe for a plain one (the default font lacks it).
    let (old_title, new_title) = (old_title.replace('\u{2019}', "'"), new_title.replace('\u{2019}', "'"));
    let mut titles = world.query::<&mut Text>();
    for mut t in titles.iter_mut(world) {
        if t.0 == old_title || t.0.starts_with(&format!("{old_title} - ")) {
            t.0 = new_title.clone();
        }
    }
    world.write_message(CourseSwitched);
}

/// First-person look (`lookPitch`, tower warp floor): the camera sits at Mario's head and Mario
/// is hidden.
fn first_person_camera(
    progress: Res<Progress>,
    sim: Res<GameSim>,
    mut script: ResMut<crate::camera::CameraScript>,
    mut mario: Query<&mut Visibility, With<crate::render::mario_view::MarioRoot>>,
    mut active: Local<bool>,
) {
    if progress.look_pitch != 0.0 {
        let yaw = (sim.mario.face_angle[1] + progress.look_yaw) * std::f64::consts::PI / 32768.0;
        let pitch = progress.look_pitch * std::f64::consts::PI / 32768.0;
        let eye = bevy::math::DVec3::new(sim.mario.pos[0], sim.mario.pos[1] + 145.0, sim.mario.pos[2]);
        let look = eye + bevy::math::DVec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos());
        script.0 = Some((eye, look));
        if !*active {
            *active = true;
            for mut v in &mut mario {
                *v = Visibility::Hidden;
            }
        }
    } else if *active {
        *active = false;
        script.0 = None;
        for mut v in &mut mario {
            *v = Visibility::Inherited;
        }
    }
}

fn course_title(sim: &GameSim) -> String {
    let missions = crate::game::courses::missions_for(&sim.course.request.id);
    match missions.iter().find(|m| m.act == sim.course.request.act) {
        Some(m) if missions.len() > 1 => format!("{} - {}", sim.course.name, m.name),
        _ => sim.course.name.clone(),
    }
}

fn rebuild_course_views(world: &mut World) {
    // Bevy 0.20 stores resources as entities: a resource created while the course was being built (and so tagged by
    // `tag_new`) must never be despawned with the course (`Resource entities are not supposed to be despawned`: the first
    // course switch of the run panicked).
    let doomed: Vec<Entity> = world.query_filtered::<Entity, (Or<(With<CourseScoped>, With<ObjectView>, With<PickupView>)>, Without<bevy::ecs::resource::IsResource>)>().iter(world).collect();
    for e in doomed {
        if let Ok(ent) = world.get_entity_mut(e) {
            ent.despawn();
        }
    }
    world.insert_resource(ObjectViews::default());
    world.insert_resource(PickupViews::default());
    let set = world.iter_entities().map(|e| e.id()).collect();
    world.insert_resource(EntitiesBefore(set));
    let _ = world.run_system_once(spawn_course);
    tag_new(world);
    let hooks = world.resource::<CourseRebuild>().0.clone();
    for hook in hooks {
        let set = world.iter_entities().map(|e| e.id()).collect();
        world.insert_resource(EntitiesBefore(set));
        hook(world);
        tag_new(world);
    }
    views::rebuild(world);
}
