//! Experiments (`src/game/experiments/*`): self-contained game modes that borrow the player,
//! the course and the camera. The TS `ExperimentHost` is the [`Host`] struct here: it is built
//! per system call from the live Bevy resources and handed to the active [`Experiment`].
//!
//! * `X` opens the menu (when no experiment runs), `1`-`9` start one, `X`/`Esc` leaves it.
//! * `--experiment <id>` / `JUMPMAN_EXPERIMENT=<id>` start one directly (and pick its course).
//! * An experiment that needs another course than the loaded one relaunches the executable
//!   with that course (the app loads exactly one course at startup).
//!
//! Hooks mirror the TS: `before_step` (may replace the controller / camera yaw), `after_step`,
//! `frame` (after the default camera, may take it over via [`CameraOverride`]).
//!
//! Chrome: `dock.rs` (clickable dock, title card, menu, mouse state); `clip.rs` (renderer clip
//! plane); `textures.rs` (SM64 textures). Crowd/God sounds and particles go through `Host::fx`.
pub mod brawl_ai;
pub mod brawl_arena;
pub mod brawl;
pub mod brawl_core;
pub mod brawl_view;
pub mod clip;
pub mod crowd;
pub mod crowd_route;
pub mod deck;
pub mod deck_core;
pub mod deck_run;
#[cfg(test)]
mod deck_tests;
pub mod deck_ui;
pub mod diorama;
pub mod dock;
pub mod dimensions;
pub mod geometry;
pub mod fps;
pub mod gate;
pub mod gate_core;
pub mod gate_view;
pub mod god;
pub mod halls;
pub mod halls_core;
pub mod halls_ui;
pub mod halls_view;
pub mod halls_world;
pub mod hundred;
pub mod mario_crowd;
pub mod party;
pub mod planet;
pub mod kart;
pub mod kart_audio;
pub mod kart_camera;
pub mod kart_fx;
pub mod kart_sound;
pub mod kart_physics;
pub mod kart_rescue;
pub mod kart_race;
pub mod kart_race_view;
pub mod race;
pub mod relic_core;
pub mod relic;
pub mod relic_dungeon;
pub mod relic_loot;
pub mod relic_ui;
pub mod relic_view;
pub mod rail_path;
pub mod runner_gen;
pub mod runner;
pub mod shrink;
pub mod store;
pub mod lss;
pub mod lss_core;
pub mod lss_view;
pub mod swarm;
pub mod swarm_core;
pub mod swarm_view;
pub mod textures;

use crate::camera::{CameraRig, MainCamera};
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::render::mario_view::MarioRoot;
use crate::render::object_view::ObjectView;
use crate::render::pickup_view::PickupView;
use crate::render::{to_bevy, SCALE};
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::VisibilitySystems;
use bevy::transform::TransformSystem;
use jumpman_sim::collision::CollisionWorld;
use jumpman_sim::mario_runtime::SpawnPoint;
use jumpman_sim::math::Vec3 as V3;
use jumpman_sim::state::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExperimentId {
    Dimensions,
    God,
    Runner,
    Hundred,
    Diorama,
    Race,
    Shrink,
    Planet,
    Swarm,
    Kart,
    LastStar,
    Relic,
    Gate,
    Deck,
    Halls,
    Brawl,
    Party,
}

pub const EXPERIMENT_IDS: [ExperimentId; 17] = [
    ExperimentId::Dimensions,
    ExperimentId::God,
    ExperimentId::Runner,
    ExperimentId::Hundred,
    ExperimentId::Diorama,
    ExperimentId::Race,
    ExperimentId::Shrink,
    ExperimentId::Planet,
    ExperimentId::Swarm,
    ExperimentId::Kart,
    ExperimentId::LastStar,
    ExperimentId::Relic,
    ExperimentId::Gate,
    ExperimentId::Deck,
    ExperimentId::Halls,
    ExperimentId::Brawl,
    ExperimentId::Party,
];

impl ExperimentId {
    pub fn key(self) -> &'static str {
        match self {
            Self::Dimensions => "dimensions",
            Self::God => "god",
            Self::Runner => "runner",
            Self::Hundred => "hundred",
            Self::Diorama => "diorama",
            Self::Race => "race",
            Self::Shrink => "shrink",
            Self::Planet => "planet",
            Self::Swarm => "swarm",
            Self::Kart => "kart",
            Self::LastStar => "laststar",
            Self::Relic => "relic",
            Self::Gate => "gate",
            Self::Deck => "stardeck",
            Self::Halls => "halls",
            Self::Brawl => "brawl",
            Self::Party => "party",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        EXPERIMENT_IDS.iter().copied().find(|id| id.key() == s)
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Dimensions => "Across Dimensions",
            Self::God => "Mario vs. God",
            Self::Runner => "Endless Run",
            Self::Hundred => "A Hundred Marios",
            Self::Diorama => "Diorama",
            Self::Race => "Summit Race",
            Self::Shrink => "Shrink Ray",
            Self::Planet => "Tiny Planet",
            Self::Swarm => "Star Swarm",
            Self::Kart => "Mario Kart 64 · Luigi Raceway",
            Self::LastStar => "Last Star Standing",
            Self::Relic => "Relic Descent",
            Self::Gate => "Hold the Gate",
            Self::Deck => "Star Deck",
            Self::Halls => "The Painted Halls",
            Self::Brawl => "Gem Clash",
            Self::Party => "Sunstone Shindig",
        }
    }
    /// Whether the experiment is ported yet.
    pub fn available(self) -> bool {
        create(self, "bob").is_some()
    }
}

/// Build a fresh experiment; each run starts from a clean slate. `None` when not ported yet.
pub fn create(id: ExperimentId, world_name: &str) -> Option<Box<dyn Experiment>> {
    Some(match id {
        ExperimentId::Dimensions => Box::new(if world_name == "mushroom2d" {
            dimensions::DimensionsExperiment::new(dimensions::DimensionsOptions { course: None, initial: dimensions::DimensionMode::Side, lane_yaw: Some(0x4000 as f64) })
        } else {
            dimensions::DimensionsExperiment::new(Default::default())
        }),
        ExperimentId::Shrink => Box::new(shrink::ShrinkRayExperiment::new()),
        ExperimentId::Hundred => Box::new(hundred::HundredMariosExperiment::new()),
        ExperimentId::Diorama => Box::new(diorama::DioramaExperiment::new()),
        ExperimentId::God => Box::new(god::GodModeExperiment::new()),
        ExperimentId::Planet => Box::new(planet::TinyPlanetExperiment::new()),
        ExperimentId::Swarm => Box::new(swarm::StarSwarmExperiment::new()),
        ExperimentId::Kart => Box::new(kart::KartExperiment::new()),
        ExperimentId::LastStar => Box::new(lss::LastStarStandingExperiment::new()),
        ExperimentId::Relic => Box::new(relic::RelicDescentExperiment::new()),
        ExperimentId::Gate => Box::new(gate::HoldTheGateExperiment::new()),
        ExperimentId::Deck => Box::new(deck::StarDeckExperiment::new()),
        ExperimentId::Halls => Box::new(halls::PaintedHallsExperiment::new()),
        ExperimentId::Brawl => Box::new(brawl::GemClashExperiment::new()),
        ExperimentId::Party => Box::new(party::app::PartyExperiment::new()),
        ExperimentId::Race => Box::new(race::SummitRaceExperiment::new()),
        ExperimentId::Runner => Box::new(runner::EndlessRunnerExperiment::new(std::env::var("JUMPMAN_RUN_SEED").ok().and_then(|s| s.parse().ok()))),
        _ => return None,
    })
}

/// Pose override for the main camera, in SM64 units.
#[derive(Clone, Copy, Debug)]
pub struct CamPose {
    pub eye: DVec3,
    pub look: DVec3,
    pub up: DVec3,
    /// Vertical field of view in degrees.
    pub fov: f64,
    pub near: f64,
    pub far: f64,
}

/// While `pose` is set the default follow camera is ignored.
#[derive(Resource)]
pub struct CameraOverride {
    pub pose: Option<CamPose>,
    /// Multiplier for the default camera's far plane (scaled-up worlds).
    pub far_scale: f64,
    /// Miniature lens: (focal distance in SM64 units, blur strength 0..1).
    pub dof: Option<(f64, f64)>,
    /// Distance fog replacing the course's: (colour, start, end) in SM64 units.
    pub fog: Option<(Color, f64, f64)>,
    /// Renderer clipping plane `(normal, constant)` in SM64 units; keeps `n . p + c >= 0`.
    pub clip: Option<(DVec3, f64)>,
}

impl Default for CameraOverride {
    fn default() -> Self {
        Self { pose: None, far_scale: 1.0, dof: None, fog: None, clip: None }
    }
}

/// Replaces the next 30 Hz step's inputs (consumed by `sim_plugin::tick`).
#[derive(Resource, Default)]
pub struct StepControl {
    pub controller: Option<ControllerSnapshot>,
    pub camera_yaw: Option<i32>,
    /// Skip the player's simulation entirely this step.
    pub skip: bool,
}

#[derive(Default)]
pub struct StepOverride {
    pub controller: Option<ControllerSnapshot>,
    pub camera_yaw: Option<i32>,
}

/// Everything spawned by an experiment carries this and is despawned when it exits.
#[derive(Component)]
pub struct XpEntity;

/// Title card / status line shown by the framework.
#[derive(Resource, Default)]
pub struct XpUi {
    pub card: String,
    pub line: String,
}

/// Framework-side state that outlives single calls.
#[derive(Resource, Default)]
pub struct Shell {
    pub player_visible: bool,
    pub world_swapped: bool,
    pub course_hidden: bool,
    /// The active experiment steps the course's walking goombas itself (Diorama).
    pub goombas_driven: bool,
    /// Disable the Bob-omb Battlefield radial camera (Mario is far from the mountain).
    pub plain_camera: bool,
    /// Sound cues queued by experiments, forwarded to the audio engine each frame.
    pub cues: Vec<crate::audio::SfxCue>,
    hidden_roots: Vec<(Entity, Visibility)>,
    /// Display entities of the course are scaled by `.0` about the origin, then moved by `.1`
    /// (Bevy units); the course's objects and pickups are hidden meanwhile (Shrink Ray).
    pub course_xform: Option<(f32, Vec3)>,
    originals: std::collections::HashMap<Entity, Transform>,
    saved_clear: Option<Color>,
    pub time: f64,
}

#[derive(Resource, Default)]
pub struct ExperimentSlot {
    pub active: Option<Box<dyn Experiment>>,
    pub id: Option<ExperimentId>,
    pub pending: Option<ExperimentId>,
    pub exit: bool,
}

use dock::{DockClicks, DockItem, MenuOpen, MouseState, XpDock};

/// The slice of the game shell an experiment may drive (`ExperimentHost`).
pub struct Host<'a, 'w, 's> {
    pub sim: &'a mut GameSim,
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub assets: &'a AssetServer,
    pub asset_root: &'a crate::render::AssetRoot,
    pub graphs: &'a mut Assets<AnimationGraph>,
    pub keys: &'a ButtonInput<KeyCode>,
    pub rig: &'a mut CameraRig,
    pub cam: &'a mut CameraOverride,
    pub ui: &'a mut XpUi,
    pub clear: &'a mut ClearColor,
    pub palette: &'a mut crate::fx::sky::SkyPalette,
    pub particles: &'a mut crate::fx::particles::Particles,
    pub toast: &'a mut Toast,
    pub shell: &'a mut Shell,
    pub mouse: &'a MouseState,
    pub goombas: &'a mut crate::objects::goombas::Goombas,
    pub dock: &'a mut XpDock,
    /// Render lists and UI state shared with Star Swarm's presentation systems.
    pub bridge: &'a mut swarm_view::SwarmBridge,
    /// Render lists and UI state shared with Last Star Standing.
    pub lss: &'a mut lss_view::LssBridge,
    /// Render lists and UI state shared with Relic Descent's presentation systems.
    pub relic: &'a mut relic_view::RelicBridge,
    /// Render lists and UI state shared with Hold the Gate's presentation systems.
    pub gate: &'a mut gate_view::GateBridge,
    /// Render lists and UI state shared with The Painted Halls.
    pub halls: &'a mut halls_view::HallsBridge,
    /// Render lists and UI state shared with Gem Clash (Arena Brawl).
    pub brawl: &'a mut brawl_view::BrawlBridge,
    /// The touch overlay is showing (phones / tablets): experiments phrase hints for fingers, not keys.
    pub touch: bool,
    /// Labels and tints of the touch action buttons (A / B / Z), reset when the experiment exits.
    pub labels: &'a mut crate::touch::ButtonLabels,
    /// Interpolation factor between the last two 30 Hz steps.
    pub alpha: f64,
}

impl Host<'_, '_, '_> {
    pub fn world_name(&self) -> &str {
        &self.sim.course.request.id
    }
    /// The loaded course's collision (what `host.playground.collision` was).
    pub fn course_world(&self) -> Arc<CollisionWorld> {
        self.sim.course.world.clone()
    }
    /// Seconds since the game shell started.
    pub fn time(&self) -> f64 {
        self.shell.time
    }
    /// Interpolated render position of the player.
    pub fn render_pos(&self) -> V3 {
        self.sim.mario_at(self.alpha).0
    }
    pub fn toast(&mut self, text: &str, seconds: f32) {
        self.toast.show(text, seconds);
    }
    /// Teleport the player, reset momentum, and snap the follow camera behind.
    pub fn place_mario(&mut self, pos: V3, face_yaw: Option<f64>) {
        let m = &mut self.sim.mario;
        m.pos = pos;
        m.vel = [0.0; 3];
        m.forward_vel = 0.0;
        m.slide_vel_x = 0.0;
        m.slide_vel_z = 0.0;
        if let Some(yaw) = face_yaw {
            m.face_angle = [0.0, yaw, 0.0];
            m.intended_yaw = yaw;
        }
        m.peak_height = pos[1];
        let floor = m.world.find_floor(pos[0], pos[1] + 100.0, pos[2]);
        m.floor_height = floor.height;
        m.floor = floor.surface;
        m.set_action(ACT_FREEFALL, 0.0);
        let yaw = m.face_angle[1];
        self.sim.curr = self.sim.snapshot();
        self.sim.prev = self.sim.curr.clone();
        self.rig.snap_behind(yaw);
    }
    /// Swap the collision world the player (and camera) collide with.
    pub fn set_mario_world(&mut self, world: Arc<CollisionWorld>, spawn: SpawnPoint, death_height: f64) {
        self.sim.mario.change_area(world, spawn, death_height, [0.0; 3]);
        self.shell.world_swapped = true;
    }
    /// Return the player and camera to the loaded course's collision.
    pub fn restore_mario_world(&mut self) {
        if self.shell.world_swapped {
            let c = &self.sim.course;
            let (world, spawn, death) = (c.world.clone(), c.spawn.clone(), c.death_height);
            self.sim.mario.change_area(world, spawn, death, [0.0; 3]);
            self.shell.world_swapped = false;
        }
    }
    pub fn set_player_visible(&mut self, visible: bool) {
        self.shell.player_visible = visible;
    }
    /// Move the player, his render interpolation and the follow camera together (no reset).
    pub fn shift_mario(&mut self, d: V3) {
        for axis in 0..3 {
            self.sim.mario.pos[axis] += d[axis];
            self.sim.prev.pos[axis] += d[axis];
            self.sim.curr.pos[axis] += d[axis];
        }
        self.sim.mario.peak_height += d[1];
        self.rig.displace(DVec3::from(d));
    }
    /// The player's current collision world, for experiments that stream geometry in and out
    /// (clones only if something else still holds it).
    pub fn world_mut(&mut self) -> &mut CollisionWorld {
        Arc::make_mut(&mut self.sim.mario.world)
    }
    /// Change the sky/background colour; restored when the experiment exits.
    /// One-shot sound + particles at `pos`, only when it is near the player (`playNear`).
    /// Kinds: coin, coin_bounce, bonk, star_sound, teleport, triple_jump, footstep, break_box, sparks.
    pub fn fx(&mut self, kind: &'static str, pos: V3) {
        let me = self.sim.mario.pos;
        if (pos[0] - me[0]).hypot(pos[2] - me[2]) > 3500.0 {
            return;
        }
        match kind {
            "coin" | "bonk" | "triple_jump" | "teleport" => self.particles.sparks(pos, if kind == "coin" { 6 } else { 12 }),
            "break_box" => self.particles.burst(pos, crate::fx::particles::BurstOptions { count: 14, color: 0xffe8a0, speed: 160.0, ..Default::default() }),
            _ => {}
        }
        if kind != "sparks" {
            self.shell.cues.push(crate::audio::SfxCue::Effect { kind, pos: Some(pos) });
        }
    }
    pub fn play_fx(&mut self, list: Vec<(&'static str, V3)>) {
        for (k, p) in list {
            self.fx(k, p);
        }
    }
    /// Re-colour the camera-following gradient sky dome (restored on exit).
    pub fn set_sky(&mut self, palette: crate::fx::sky::SkyPalette) {
        if *self.palette != palette {
            *self.palette = palette;
        }
    }
    pub fn set_clear(&mut self, color: Color) {
        if self.shell.saved_clear.is_none() {
            self.shell.saved_clear = Some(self.clear.0);
        }
        self.clear.0 = color;
    }
    pub fn add_coins(&mut self, count: u32) {
        self.sim.coins += count;
    }
    pub fn card(&mut self, number: &str, title: &str, pitch: &str, hints: &[(&str, &str)]) {
        let mut s = format!("{number}  {title}\n{pitch}\n");
        for (k, a) in hints {
            s.push_str(&format!("\n[{k}] {a}"));
        }
        self.ui.card = s;
    }
    /// Replace the clickable dock (clicking a button delivers its key to `keydown`).
    pub fn set_dock(&mut self, items: Vec<DockItem>) {
        self.dock.set(items);
    }
    pub fn status(&mut self, line: impl Into<String>) {
        self.ui.line = line.into();
    }
    /// Spawn an experiment-owned entity (despawned on exit).
    pub fn spawn(&mut self, bundle: impl Bundle) -> Entity {
        self.commands.spawn((XpEntity, bundle)).id()
    }
    pub fn unlit(&mut self, color: Color) -> Handle<StandardMaterial> {
        self.materials.add(StandardMaterial { base_color: color, unlit: true, ..default() })
    }
    pub fn lit(&mut self, color: Color) -> Handle<StandardMaterial> {
        self.materials.add(StandardMaterial { base_color: color, perceptual_roughness: 1.0, ..default() })
    }
    /// A box mesh entity given centre and size in SM64 units.
    pub fn spawn_box(&mut self, center: V3, size: V3, yaw: f64, material: Handle<StandardMaterial>) -> Entity {
        let mesh = self.meshes.add(Cuboid::new(size[0] as f32 * SCALE, size[1] as f32 * SCALE, size[2] as f32 * SCALE));
        self.spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(to_bevy(center)).with_rotation(Quat::from_rotation_y(yaw as f32))))
    }
}

pub trait Experiment: Send + Sync {
    fn id(&self) -> ExperimentId;
    /// Course to load before entering; None keeps whatever course is loaded.
    fn course(&self) -> Option<&'static str> {
        None
    }
    /// Skip the player's simulation entirely (camera-only experiments).
    fn freezes_player(&self) -> bool {
        false
    }
    /// Hide the loaded course (the experiment builds its own world).
    fn replaces_course(&self) -> bool {
        false
    }
    /// Simulation speed multiplier; 0 pauses the player and world.
    fn time_scale(&self) -> f64 {
        1.0
    }
    fn enter(&mut self, host: &mut Host);
    fn exit(&mut self, host: &mut Host);
    /// Before the player's 30 Hz step.
    fn before_step(&mut self, _host: &mut Host, _controller: &ControllerSnapshot) -> StepOverride {
        StepOverride::default()
    }
    /// After the player's 30 Hz step (or in its place when the player is frozen).
    fn after_step(&mut self, _host: &mut Host) {}
    /// Once per rendered frame, after the default follow camera has been placed.
    fn frame(&mut self, _host: &mut Host, _dt: f64) {}
    fn keydown(&mut self, _host: &mut Host, _key: KeyCode) -> bool {
        false
    }
    fn debug(&self) -> String;
}

#[derive(SystemParam)]
struct XpParams<'w, 's> {
    sim: ResMut<'w, GameSim>,
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    assets: Res<'w, AssetServer>,
    asset_root: Res<'w, crate::render::AssetRoot>,
    graphs: ResMut<'w, Assets<AnimationGraph>>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    rig: ResMut<'w, CameraRig>,
    cam: ResMut<'w, CameraOverride>,
    ui: ResMut<'w, XpUi>,
    clear: ResMut<'w, ClearColor>,
    palette: ResMut<'w, crate::fx::sky::SkyPalette>,
    particles: ResMut<'w, crate::fx::particles::Particles>,
    toast: ResMut<'w, Toast>,
    shell: ResMut<'w, Shell>,
    mouse: Res<'w, MouseState>,
    goombas: ResMut<'w, crate::objects::goombas::Goombas>,
    dock: ResMut<'w, XpDock>,
    bridge: ResMut<'w, swarm_view::SwarmBridge>,
    lss: ResMut<'w, lss_view::LssBridge>,
    relic: ResMut<'w, relic_view::RelicBridge>,
    gate: ResMut<'w, gate_view::GateBridge>,
    halls: ResMut<'w, halls_view::HallsBridge>,
    brawl: ResMut<'w, brawl_view::BrawlBridge>,
    fixed: Res<'w, Time<Fixed>>,
    touch: Res<'w, crate::touch::TouchControls>,
    labels: ResMut<'w, crate::touch::ButtonLabels>,
}

impl<'w, 's> XpParams<'w, 's> {
    fn host(&mut self) -> Host<'_, 'w, 's> {
        Host {
            sim: &mut self.sim,
            commands: &mut self.commands,
            meshes: &mut self.meshes,
            materials: &mut self.materials,
            assets: &self.assets,
            asset_root: &self.asset_root,
            graphs: &mut self.graphs,
            keys: &self.keys,
            rig: &mut self.rig,
            cam: &mut self.cam,
            ui: &mut self.ui,
            clear: &mut self.clear,
            palette: &mut self.palette,
            particles: &mut self.particles,
            toast: &mut self.toast,
            shell: &mut self.shell,
            mouse: &self.mouse,
            goombas: &mut self.goombas,
            dock: &mut self.dock,
            bridge: &mut self.bridge,
            lss: &mut self.lss,
            relic: &mut self.relic,
            gate: &mut self.gate,
            halls: &mut self.halls,
            brawl: &mut self.brawl,
            touch: self.touch.visible,
            labels: &mut self.labels,
            alpha: self.fixed.overstep_fraction() as f64,
        }
    }
}

pub struct ExperimentsPlugin;

impl Plugin for ExperimentsPlugin {
    fn build(&self, app: &mut App) {
        let start = std::env::var("JUMPMAN_EXPERIMENT").ok().and_then(|s| ExperimentId::parse(&s));
        app.add_plugins((crowd::CrowdPlugin, clip::ClipPlugin, swarm_view::SwarmViewPlugin, lss_view::LssViewPlugin, relic_view::RelicViewPlugin, gate_view::GateViewPlugin, deck::DeckPlugin, deck_ui::DeckUiPlugin, halls_view::HallsViewPlugin, brawl_view::BrawlViewPlugin, party::PartyPlugin, kart_race_view::KartRaceUiPlugin, kart_audio::KartAudioPlugin)).init_resource::<CameraOverride>()
            .init_resource::<StepControl>()
            .init_resource::<XpUi>()
            .init_resource::<MenuOpen>()
            .init_resource::<MouseState>()
            .init_resource::<XpDock>()
            .init_resource::<DockClicks>()
            .insert_resource(Shell { player_visible: true, ..default() })
            .insert_resource(ExperimentSlot { pending: start, ..default() })
            .add_systems(Startup, (spawn_ui, dock::spawn_roots).in_set(crate::progress::PersistentSpawn))
            .add_systems(Update, (fps::log_fps, flush_cues))
            .add_systems(PostUpdate, apply_course_state.before(TransformSystem::TransformPropagate).before(VisibilitySystems::VisibilityPropagate))
            .add_systems(FixedUpdate, (before_step.before(crate::SimSet), after_step.after(crate::SimSet)).run_if(crate::shell::is_playing))
            .add_systems(
                Update,
                (dock::update_mouse, dock::place_roots, dock::dock_clicks, dock::menu_clicks, menu_keys, process_requests, experiment_keys, run_frame, apply_camera, update_ui, dock::rebuild_dock, dock::rebuild_card, dock::rebuild_menu).chain().after(crate::CameraSet),
            );
    }
}

/// The experiment requested on the command line (`--experiment`) or via the environment.
pub fn requested_from_env() -> Option<ExperimentId> {
    std::env::var("JUMPMAN_EXPERIMENT").ok().and_then(|s| ExperimentId::parse(&s))
}

/// The course an experiment needs (instantiating it is cheap).
pub fn course_for(id: ExperimentId) -> Option<&'static str> {
    create(id, "").and_then(|e| e.course())
}

#[derive(Component)]
struct LineText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 15., ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.6)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(100.), left: Val::Px(380.), max_width: Val::Percent(60.), ..default() },
        LineText,
    ));
}

fn menu_keys(keys: Res<ButtonInput<KeyCode>>, mut open: ResMut<MenuOpen>, mut slot: ResMut<ExperimentSlot>) {
    if slot.active.is_some() {
        open.0 = false;
        if keys.just_pressed(KeyCode::KeyX) {
            slot.exit = true;
        }
    } else if keys.just_pressed(KeyCode::KeyX) {
        open.0 = !open.0;
    } else if open.0 {
        let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9, KeyCode::Digit0];
        for (i, key) in digits.iter().enumerate() {
            if keys.just_pressed(*key) && EXPERIMENT_IDS[i].available() {
                slot.pending = Some(EXPERIMENT_IDS[i]);
                open.0 = false;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_requests(
    mut slot: ResMut<ExperimentSlot>,
    mut p: XpParams,
    existing: Query<Entity, With<XpEntity>>,
    mut exit: EventWriter<AppExit>,
    mut starts: EventWriter<crate::shell::StartCourse>,
    state: Option<Res<crate::shell::AppState>>,
    mut awaiting: Local<Option<(ExperimentId, String)>>,
) {
    p.shell.time += 1.0 / 60.0;
    if slot.exit {
        slot.exit = false;
        if let Some(mut exp) = slot.active.take() {
            exp.exit(&mut p.host());
        }
        slot.id = None;
        for e in &existing {
            p.commands.entity(e).despawn();
        }
        p.host().restore_mario_world();
        p.cam.pose = None;
        p.shell.player_visible = true;
        p.shell.course_hidden = false;
        p.shell.goombas_driven = false;
        p.shell.plain_camera = false;
        p.cam.dof = None;
        p.cam.clip = None;
        p.ui.card.clear();
        *p.labels = default();
        p.dock.clear();
        p.ui.line.clear();
        if let Some(c) = p.shell.saved_clear.take() {
            p.clear.0 = c;
        }
        *p.palette = crate::fx::sky::SkyPalette::default();
        p.sim.mario.movement_mode = jumpman_sim::mario_runtime::MovementMode::Default;
        return;
    }
    // iOS cannot relaunch itself, so a course switch for an experiment happens in-process: start the
    // course through the shell's fade/load, then enter the experiment once it has finished loading.
    if let Some((id, need)) = awaiting.clone() {
        let loading = state.as_deref().is_some_and(|s| *s == crate::shell::AppState::Transition);
        if p.sim.course.request.id != need || loading {
            return;
        }
        *awaiting = None;
        slot.pending = Some(id);
    }
    let Some(id) = slot.pending.take() else { return };
    if slot.active.is_some() {
        return;
    }
    let course = p.sim.course.request.id.clone();
    let Some(mut exp) = create(id, &course) else {
        p.toast.show(format!("{} is not ported yet", id.title()), 2.0);
        return;
    };
    if let Some(need) = exp.course() {
        if need != course {
            if cfg!(target_os = "ios") || std::env::var_os("JUMPMAN_INPROCESS_COURSE").is_some() {
                starts.write(crate::shell::StartCourse(crate::game::world::CourseRequest::new(need)));
                *awaiting = Some((id, need.to_string()));
            } else {
                relaunch(need, id);
                exit.write(AppExit::Success);
            }
            return;
        }
    }
    p.shell.course_hidden = exp.replaces_course();
    exp.enter(&mut p.host());
    slot.id = Some(id);
    slot.active = Some(exp);
}

/// Restart the executable on the course the experiment needs.
fn relaunch(course: &str, id: ExperimentId) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg(course).arg("--experiment").arg(id.key()).spawn();
    }
}

fn before_step(mut slot: ResMut<ExperimentSlot>, mut input: ResMut<PlayerInput>, mut ctl: ResMut<StepControl>, mut p: XpParams) {
    let Some(exp) = slot.active.as_mut() else { return };
    let controller = input.consume_step();
    let ov = exp.before_step(&mut p.host(), &controller);
    ctl.controller = Some(ov.controller.unwrap_or(controller));
    ctl.camera_yaw = ov.camera_yaw;
    ctl.skip = exp.freezes_player() || exp.time_scale() <= 0.0;
}

fn after_step(mut slot: ResMut<ExperimentSlot>, mut p: XpParams) {
    if let Some(exp) = slot.active.as_mut() {
        exp.after_step(&mut p.host());
    }
}

/// Test hook: `JUMPMAN_XP_KEYS="2.0:Digit2,4.0:Tab"` presses keys inside the experiment at
/// the given times (seconds since start).
fn scripted_keys(now: f64, last: f64) -> Vec<KeyCode> {
    let Ok(script) = std::env::var("JUMPMAN_XP_KEYS") else { return vec![] };
    script
        .split(',')
        .filter_map(|item| {
            let (at, name) = item.split_once(':')?;
            let at: f64 = at.parse().ok()?;
            if !(last < at && at <= now) {
                return None;
            }
            Some(match name {
                "Tab" => KeyCode::Tab,
                "5" => KeyCode::Digit5,
                "6" => KeyCode::Digit6,
                "7" => KeyCode::Digit7,
                "8" => KeyCode::Digit8,
                "KeyG" => KeyCode::KeyG,
                "KeyT" => KeyCode::KeyT,
                "KeyN" => KeyCode::KeyN,
                "KeyM" => KeyCode::KeyM,
                "KeyJ" => KeyCode::KeyJ,
                "KeyZ" => KeyCode::KeyZ,
                "KeyY" => KeyCode::KeyY,
                "KeyC" => KeyCode::KeyC,
                "KeyB" => KeyCode::KeyB,
                "KeyV" => KeyCode::KeyV,
                "1" => KeyCode::Digit1,
                "2" => KeyCode::Digit2,
                "3" => KeyCode::Digit3,
                "4" => KeyCode::Digit4,
                "KeyP" => KeyCode::KeyP,
                "KeyR" => KeyCode::KeyR,
                "KeyF" => KeyCode::KeyF,
                "KeyH" => KeyCode::KeyH,
                "KeyI" => KeyCode::KeyI,
                "KeyE" => KeyCode::KeyE,
                _ => return None,
            })
        })
        .collect()
}

fn experiment_keys(mut slot: ResMut<ExperimentSlot>, mut p: XpParams, time: Res<Time>, mut last: Local<f64>, mut clicks: ResMut<DockClicks>) {
    let Some(exp) = slot.active.as_mut() else { return };
    let now = time.elapsed_secs_f64();
    let mut pressed: Vec<KeyCode> = p.keys.get_just_pressed().copied().collect();
    pressed.extend(scripted_keys(now, *last));
    pressed.append(&mut clicks.0);
    *last = now;
    for key in pressed {
        exp.keydown(&mut p.host(), key);
    }
}

fn run_frame(mut slot: ResMut<ExperimentSlot>, mut p: XpParams, time: Res<Time>) {
    if let Some(exp) = slot.active.as_mut() {
        exp.frame(&mut p.host(), time.delta_secs_f64().min(0.1));
    }
}

pub fn apply_camera(
    cam: Res<CameraOverride>,
    mut commands: Commands,
    camera_entity: Single<Entity, With<MainCamera>>,
    mut camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    mut base: Local<Option<PerspectiveProjection>>,
    mut fog: Query<&mut bevy::pbr::DistanceFog, With<MainCamera>>,
    mut fog_base: Local<Option<((f32, f32), (f32, f32))>>,
    mut saved_fog: Local<Option<bevy::pbr::DistanceFog>>,
    mut sky: Query<&mut Transform, (With<crate::fx::sky::SkyDome>, Without<MainCamera>)>,
) {
    // Scaled-up worlds push the course's distance fog out with the far plane. `fog_base` is
    // (the course's own range, the range we last wrote) so a fog re-inserted by another system
    // is picked up as the new base.
    match cam.fog {
        Some((color, start, end)) => {
            for mut f in &mut fog {
                if saved_fog.is_none() {
                    *saved_fog = Some(f.clone());
                }
                f.color = color;
                f.falloff = bevy::pbr::FogFalloff::Linear { start: start as f32 * SCALE, end: end as f32 * SCALE };
            }
        }
        None => {
            if let Some(original) = saved_fog.take() {
                for mut f in &mut fog {
                    *f = original.clone();
                }
                *fog_base = None;
            }
        }
    }
    for mut f in &mut fog {
        if cam.fog.is_some() {
            break;
        }
        if let bevy::pbr::FogFalloff::Linear { start, end } = f.falloff {
            let current = (start, end);
            let course = match *fog_base {
                Some((course, written)) if written == current => course,
                _ => current,
            };
            let k = cam.far_scale as f32;
            let written = (course.0 * k, course.1 * k);
            f.falloff = bevy::pbr::FogFalloff::Linear { start: written.0, end: written.1 };
            *fog_base = Some((course, written));
        }
    }
    match cam.dof {
        Some((focus, strength)) => {
            commands.entity(*camera_entity).insert(bevy::core_pipeline::dof::DepthOfField {
                mode: bevy::core_pipeline::dof::DepthOfFieldMode::Gaussian,
                focal_distance: focus as f32 * SCALE,
                aperture_f_stops: 0.012 / (strength as f32).max(0.05),
                max_depth: 2000.0,
                ..default()
            });
        }
        None => {
            commands.entity(*camera_entity).remove::<bevy::core_pipeline::dof::DepthOfField>();
        }
    }
    let (tf, projection) = &mut *camera;
    let Projection::Perspective(p) = &mut **projection else { return };
    let base = base.get_or_insert_with(|| p.clone());
    match cam.pose {
        Some(pose) => {
            **tf = Transform::from_translation(to_bevy(pose.eye.into())).looking_at(to_bevy(pose.look.into()), Vec3::new(pose.up.x as f32, pose.up.y as f32, pose.up.z as f32));
            p.fov = (pose.fov as f32).to_radians();
            p.near = (pose.near as f32 * SCALE).max(0.01);
            p.far = pose.far as f32 * SCALE;
        }
        None => {
            p.fov = base.fov;
            p.near = base.near;
            p.far = base.far * cam.far_scale as f32;
        }
    }
    // The sky dome sits at 0.75 of the default far plane; keep it behind scaled-up views.
    let dome = (p.far / base.far).max(1.0);
    for mut t in &mut sky {
        t.scale = Vec3::splat(dome);
    }
}

/// Root entities of the loaded course. The sky dome is camera-following, not part of the course
/// (hiding/moving/scaling it with the course left the experiments without a sky).
type CourseRoot = (Without<crate::fx::sky::SkyDome>, Without<MarioRoot>, Without<XpEntity>, Without<ChildOf>, Without<Node>, Without<MainCamera>, Without<DirectionalLight>, Without<crate::characters::CharacterRoot>);

/// Runs in PostUpdate (after every Update system wrote its transforms/visibility, before
/// propagation) so the experiment's course changes win over the course/object views.
#[allow(clippy::type_complexity)]
fn apply_course_state(
    mut shell: ResMut<Shell>,
    character: Res<crate::characters::Character>,
    rig: Res<crate::characters::gltf_view::GltfRig>,
    mut mario: Query<&mut Visibility, With<MarioRoot>>,
    mut roots: Query<(Entity, &mut Visibility, &mut Transform, Has<ObjectView>, Has<PickupView>), CourseRoot>,
) {
    for mut v in &mut mario {
        // Mario's model stays hidden while Link/Banjo/the glTF rig is the player.
        let want = if shell.player_visible && !rig.replaces_mario(*character) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if shell.course_hidden {
        if shell.hidden_roots.is_empty() {
            shell.hidden_roots = roots.iter_mut().map(|(e, mut v, ..)| (e, std::mem::replace(&mut *v, Visibility::Hidden))).collect();
        } else {
            for (_, mut v, ..) in &mut roots {
                *v = Visibility::Hidden;
            }
        }
    } else if !shell.hidden_roots.is_empty() {
        for (e, old) in std::mem::take(&mut shell.hidden_roots) {
            if let Ok((_, mut v, ..)) = roots.get_mut(e) {
                *v = old;
            }
        }
    }
    match shell.course_xform {
        Some((k, offset)) => {
            let Shell { originals, .. } = &mut *shell;
            for (e, mut v, mut tf, object, pickup) in &mut roots {
                if object || pickup {
                    *v = Visibility::Hidden;
                    continue;
                }
                let orig = *originals.entry(e).or_insert(*tf);
                tf.translation = orig.translation * k + offset;
                tf.scale = orig.scale * k;
                tf.rotation = orig.rotation;
            }
        }
        None if !shell.originals.is_empty() => {
            for (e, orig) in std::mem::take(&mut shell.originals) {
                if let Ok((_, _, mut tf, ..)) = roots.get_mut(e) {
                    *tf = orig;
                }
            }
        }
        None => {}
    }
}

fn update_ui(ui: Res<XpUi>, mut line: Single<&mut Text, With<LineText>>) {
    if line.0 != ui.line {
        line.0 = ui.line.clone();
    }
}

fn flush_cues(mut shell: ResMut<Shell>, mut out: EventWriter<crate::audio::SfxCue>) {
    if !shell.cues.is_empty() {
        for c in shell.cues.drain(..) {
            out.write(c);
        }
    }
}
