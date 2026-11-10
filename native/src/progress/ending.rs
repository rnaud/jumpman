//! The castle ending (`castleEnding.ts` + the ending branch of `Game.stepWorld`): the Grand Star
//! flight camera, the Peach / Toad cutscene with its US-timed dialogue, voices and music, and the
//! fade in / out. The sim already runs the actions (`jumpman_sim::actions::ending`); this module is
//! the presentation: `EndingScene` is the engine-free script, the Bevy systems below own the
//! Peach, Toad and star models.
use super::flow::Progress;
use super::CourseScoped;
use crate::audio::{course_music, MusicTrack, PeachVoice, SfxCue};
use crate::camera::ending::{EndingCamera, GrandStarCamera};
use crate::camera::{CameraScript, MainCamera};
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::render::{glb_animation_count, scene, to_bevy, AssetRoot};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::state::*;

const PEACH: &str = "actors/peach/peach_geo.glb";
const TOAD: &str = "actors/toad/toad_geo.glb";
const STAR: &str = "actors/star/star_geo.glb";

pub const PEACH_START_Y: f64 = 2428.0;

pub struct Line {
    pub phase: i32,
    pub frame: i32,
    pub text: &'static str,
    pub duration: u32,
    pub voice: Option<PeachVoice>,
}

const fn line(phase: i32, frame: i32, text: &'static str, duration: u32, voice: Option<PeachVoice>) -> Line {
    Line { phase, frame, text, duration, voice }
}

pub const DIALOGUE: [Line; 9] = [
    line(6, 230, "Mario!", 30, Some(PeachVoice::Mario)),
    line(6, 290, "The power of the Stars is restored to the castle...", 60, Some(PeachVoice::Stars)),
    line(7, 29, "...and it's all thanks to you!", 30, Some(PeachVoice::Thanks)),
    line(7, 75, "Thank you, Mario!", 30, Some(PeachVoice::ThankYou)),
    line(7, 130, "We have to do something special for you...", 40, Some(PeachVoice::Special)),
    line(10, 1, "Listen, everybody,", 30, Some(PeachVoice::Cake)),
    line(10, 55, "let's bake a delicious cake...", 40, None),
    line(10, 130, "...for Mario...", 50, Some(PeachVoice::ForMario)),
    line(11, 95, "Mario!", 40, Some(PeachVoice::Mario2)),
];

/// Which model a clip change applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Peach = 0,
    LeftToad = 1,
    RightToad = 2,
}

/// Clip that follows another when it finishes (`successors` in the TS `actor()`).
fn successor(who: Who, clip: usize) -> Option<usize> {
    let table: &[(usize, usize)] = match who {
        Who::Peach => &[(0, 1), (1, 2), (2, 3), (6, 7), (7, 8)],
        Who::LeftToad => &[(4, 5)],
        Who::RightToad => &[(6, 7)],
    };
    table.iter().find(|(a, _)| *a == clip).map(|(_, b)| *b)
}

/// What the models should look like this tick.
#[derive(Default, Debug, Clone)]
pub struct EndingFrame {
    pub star_visible: bool,
    pub star_spin: f64,
    pub actors_visible: bool,
    pub peach_y: f64,
    pub plays: Vec<(Who, usize)>,
    pub line: Option<usize>,
    pub camera: Option<(DVec3, DVec3)>,
    /// Canvas opacity (None = untouched).
    pub opacity: Option<f32>,
    pub cues: Vec<SfxCue>,
}

pub struct EndingScene {
    descent: i32,
    peach_y: f64,
    last_phase: i32,
    star_spin: f64,
    peach_cam: EndingCamera,
    grand_cam: Option<GrandStarCamera>,
    base_track: MusicTrack,
    victory_done: bool,
}

impl EndingScene {
    pub fn new(base_track: MusicTrack) -> Self {
        Self { descent: 0, peach_y: PEACH_START_Y, last_phase: -1, star_spin: 0.0, peach_cam: EndingCamera::default(), grand_cam: None, base_track, victory_done: false }
    }

    fn track(&self, sequence: u32, preset: u32, name: &str) -> MusicTrack {
        MusicTrack { name: name.into(), sequence, preset, ..self.base_track.clone() }
    }

    /// One 30 Hz step after Mario's. `eye` is the current camera eye (starting point of the cameras).
    pub fn step(&mut self, m: &MarioRuntimeView, eye: DVec3) -> EndingFrame {
        let mut out = EndingFrame { peach_y: self.peach_y, ..Default::default() };
        let (phase, t) = (m.arg, m.timer);
        if m.action == ACT_JUMBO_STAR_CUTSCENE {
            if phase > 0 {
                let cam = self.grand_cam.get_or_insert_with(|| GrandStarCamera::new(eye));
                out.camera = Some(cam.step(m.pos, m.face_pitch, m.face_yaw));
            }
            if phase == 0 && m.state == 1 && t == 0 && !self.victory_done {
                self.victory_done = true;
                out.cues.push(SfxCue::StartMusic(self.track(0x1f, self.base_track.preset, "victory")));
            }
            if phase == 2 && t >= 500 {
                out.opacity = Some(((530 - t) as f32 / 30.0).clamp(0.0, 1.0));
            }
            return out;
        }
        // ACT_END_PEACH_CUTSCENE
        if phase == 0 && self.last_phase != 0 {
            self.descent = 0;
            self.peach_y = PEACH_START_Y;
            self.peach_cam.reset();
            self.peach_cam.eye = eye;
        }
        let star_visible = phase == 2 || (phase == 3 && t < 40);
        if star_visible {
            self.star_spin += std::f64::consts::PI / 32.0;
        }
        out.star_visible = star_visible;
        out.star_spin = self.star_spin;
        out.actors_visible = phase > 3 || (phase == 3 && t >= 40);
        if phase == 4 {
            if self.peach_y >= 1300.0 && self.descent < 60 {
                self.descent += 5;
            }
            if self.peach_y < 1300.0 && self.descent >= 27 {
                self.descent -= 2;
            }
            self.peach_y -= f64::from(self.descent / 10);
            if self.peach_y <= 907.0 {
                self.peach_y = 906.0;
            }
        }
        out.peach_y = self.peach_y;
        if out.actors_visible {
            if phase == 3 && t == 40 {
                out.plays.extend([(Who::Peach, 4), (Who::LeftToad, 2), (Who::RightToad, 3)]);
            }
            if phase == 5 && t == 22 {
                out.plays.push((Who::Peach, 5));
            }
            if phase == 6 && t == 80 {
                out.plays.push((Who::Peach, 6));
            }
            if phase == 7 && self.last_phase != phase {
                out.plays.push((Who::Peach, 9));
            }
            if phase == 8 && self.last_phase != phase {
                out.plays.push((Who::Peach, 10));
            }
            if phase == 10 && t == 1 {
                out.plays.extend([(Who::Peach, 0), (Who::LeftToad, 4), (Who::RightToad, 6)]);
            }
        }
        out.camera = Some(self.peach_cam.step(m.pos, self.peach_y));
        self.last_phase = phase;
        out.line = DIALOGUE.iter().position(|l| l.phase == phase && l.frame == t);
        if phase == 2 && t == 90 {
            out.cues.push(SfxCue::StartMusic(self.track(0x20, 7, "ending")));
        }
        if phase == 9 && t == 140 {
            out.cues.push(SfxCue::StartMusic(self.track(0x1a, 7, "credits")));
        }
        if phase == 0 {
            out.opacity = Some((t as f32 / 20.0).min(1.0));
        }
        out
    }
}

/// The few Mario fields the script reads.
pub struct MarioRuntimeView {
    pub action: u32,
    pub arg: i32,
    pub timer: i32,
    pub state: i32,
    pub pos: DVec3,
    pub face_pitch: f64,
    pub face_yaw: f64,
}

impl MarioRuntimeView {
    pub fn of(m: &jumpman_sim::mario_runtime::MarioRuntime) -> Self {
        Self {
            action: m.action,
            arg: m.action_arg as i32,
            timer: m.action_timer as i32,
            state: m.action_state as i32,
            pos: DVec3::from(m.pos),
            face_pitch: m.face_angle[0],
            face_yaw: m.face_angle[1],
        }
    }
}

// ------------------------------------------------------------------ Bevy glue

#[derive(Resource, Default)]
struct EndingRuntime {
    scene: Option<EndingScene>,
    spawned: bool,
}

#[derive(Component)]
struct EndingModel(Option<Who>);

/// Animation setup of an actor once its scene exists.
#[derive(Component)]
struct EndingRig {
    who: Who,
    player: Option<Entity>,
    nodes: Vec<AnimationNodeIndex>,
    current: Option<usize>,
    pending: Option<usize>,
}

#[derive(Component)]
struct Clips(Vec<Handle<AnimationClip>>);

pub struct EndingPlugin;

impl Plugin for EndingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EndingRuntime>()
            .add_systems(FixedUpdate, ending_tick.after(crate::SimSet).after(super::flow_post))
            .add_systems(Update, drive_actors);
    }
}

fn is_ending(action: u32) -> bool {
    action == ACT_JUMBO_STAR_CUTSCENE || action == ACT_END_PEACH_CUTSCENE
}

fn setup_rig(
    trigger: On<WorldInstanceReady>,
    mut rigs: Query<(&mut EndingRig, &Clips)>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut commands: Commands,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
) {
    let Ok((mut rig, clips)) = rigs.get_mut(trigger.event_target()) else { return };
    if clips.0.is_empty() {
        return;
    }
    let (graph, nodes) = AnimationGraph::from_clips(clips.0.iter().cloned());
    let graph = graphs.add(graph);
    for child in children.iter_descendants(trigger.event_target()) {
        if players.contains(child) {
            commands.entity(child).insert((AnimationPlayer::default(), AnimationGraphHandle(graph.clone())));
            rig.player = Some(child);
            rig.nodes = nodes;
            return;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ending_tick(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    sim: Res<GameSim>,
    mut progress: ResMut<Progress>,
    mut rt: ResMut<EndingRuntime>,
    mut script: ResMut<CameraScript>,
    camera: Query<&Transform, With<MainCamera>>,
    mut toast: ResMut<Toast>,
    mut cues: MessageWriter<SfxCue>,
    mut models: Query<(&EndingModel, &mut Transform, &mut Visibility), Without<MainCamera>>,
    mut rigs: Query<&mut EndingRig>,
) {
    let m = &sim.mario;
    if !is_ending(m.action) {
        rt.spawned = false;
        if rt.scene.take().is_some() {
            script.0 = None;
            progress.ending.opacity = None;
            progress.fade = 0.0;
        }
        return;
    }
    // Models are spawned once per course (they are `CourseScoped`).
    if !rt.spawned && models.is_empty() && m.action == ACT_END_PEACH_CUTSCENE {
        rt.spawned = true;
        let mut spawn = |path: &str, who: Option<Who>, pos: [f64; 3], scale: f32| {
            if !root.exists(path) {
                return;
            }
            let mut e = commands.spawn((scene(&assets, path), Transform::from_translation(to_bevy(pos)).with_scale(Vec3::splat(scale)), Visibility::Hidden, EndingModel(who), CourseScoped));
            if let Some(who) = who {
                let clips = crate::render::clips::clip_handles(&assets, &root, path, glb_animation_count(&root.0.join(path)));
                e.insert((EndingRig { who, player: None, nodes: vec![], current: None, pending: None }, Clips(clips)));
                e.observe(setup_rig);
            }
        };
        spawn(STAR, None, [0.0, 2528.0, -1800.0], 3.0);
        spawn(PEACH, Some(Who::Peach), [0.0, PEACH_START_Y, -1300.0], 1.0);
        spawn(TOAD, Some(Who::LeftToad), [-200.0, 906.0, -1290.0], 1.0);
        spawn(TOAD, Some(Who::RightToad), [200.0, 906.0, -1290.0], 1.0);
    }
    let scene_state = rt.scene.get_or_insert_with(|| EndingScene::new(course_music(&sim.course.request.id, sim.course.request.area)));
    let eye = camera.single().map_or(DVec3::ZERO, |t| {
        let p = t.translation / crate::render::SCALE;
        DVec3::new(p.x as f64, p.y as f64, p.z as f64)
    });
    let frame = scene_state.step(&MarioRuntimeView::of(m), eye);
    script.0 = frame.camera;
    for cue in frame.cues {
        cues.write(cue);
    }
    if let Some(i) = frame.line {
        let l = &DIALOGUE[i];
        toast.show(l.text.to_string(), l.duration as f32 / 30.0);
        if let Some(v) = l.voice {
            cues.write(SfxCue::Peach(v));
        }
    }
    progress.ending.opacity = frame.opacity;
    progress.fade = frame.opacity.map_or(0.0, |o| 1.0 - o);
    for (model, mut tf, mut vis) in &mut models {
        match model.0 {
            None => {
                *vis = if frame.star_visible { Visibility::Inherited } else { Visibility::Hidden };
                tf.rotation = Quat::from_rotation_y(frame.star_spin as f32);
            }
            Some(who) => {
                *vis = if frame.actors_visible { Visibility::Inherited } else { Visibility::Hidden };
                if who == Who::Peach {
                    tf.translation.y = frame.peach_y as f32 * crate::render::SCALE;
                }
            }
        }
    }
    for (who, clip) in frame.plays {
        for mut rig in &mut rigs {
            if rig.who == who {
                rig.pending = Some(clip);
            }
        }
    }
}

/// Start requested clips and chain successors when a clip finishes (`mixer` `loop` events).
fn drive_actors(mut rigs: Query<&mut EndingRig>, mut players: Query<&mut AnimationPlayer>) {
    for mut rig in &mut rigs {
        let Some(player_entity) = rig.player else { continue };
        let Ok(mut player) = players.get_mut(player_entity) else { continue };
        let mut start = rig.pending.take();
        if start.is_none() {
            if let Some(cur) = rig.current {
                let finished = rig.nodes.get(cur).and_then(|n| player.animation(*n)).is_some_and(|a| a.is_finished());
                if finished {
                    start = successor(rig.who, cur);
                }
            }
        }
        if let Some(clip) = start {
            let Some(node) = rig.nodes.get(clip).copied() else { continue };
            player.stop_all();
            let anim = player.play(node);
            if successor(rig.who, clip).is_none() {
                anim.repeat();
            }
            rig.current = Some(clip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(action: u32, arg: i32, timer: i32) -> MarioRuntimeView {
        MarioRuntimeView { action, arg, timer, state: 0, pos: DVec3::new(0., 3000., 0.), face_pitch: 0.0, face_yaw: 0.0 }
    }

    fn scene() -> EndingScene {
        EndingScene::new(course_music("castle_grounds", 1))
    }

    #[test]
    fn peach_descends_to_the_floor_and_dialogue_fires_on_its_frames() {
        let mut s = scene();
        s.step(&view(ACT_END_PEACH_CUTSCENE, 0, 0), DVec3::ZERO);
        let mut y = PEACH_START_Y;
        for t in 0..600 {
            y = s.step(&view(ACT_END_PEACH_CUTSCENE, 4, t), DVec3::ZERO).peach_y;
        }
        assert_eq!(y, 906.0);
        let f = s.step(&view(ACT_END_PEACH_CUTSCENE, 6, 230), DVec3::ZERO);
        assert_eq!(DIALOGUE[f.line.unwrap()].text, "Mario!");
        let f = s.step(&view(ACT_END_PEACH_CUTSCENE, 11, 95), DVec3::ZERO);
        assert!(f.line.is_some());
    }

    #[test]
    fn star_is_visible_in_phases_two_and_early_three_and_music_starts() {
        let mut s = scene();
        let f = s.step(&view(ACT_END_PEACH_CUTSCENE, 2, 90), DVec3::ZERO);
        assert!(f.star_visible && !f.actors_visible);
        assert!(matches!(f.cues.as_slice(), [SfxCue::StartMusic(t)] if t.sequence == 0x20 && t.preset == 7));
        let f = s.step(&view(ACT_END_PEACH_CUTSCENE, 3, 40), DVec3::ZERO);
        assert!(!f.star_visible && f.actors_visible);
        assert_eq!(f.plays.len(), 3);
        let f = s.step(&view(ACT_END_PEACH_CUTSCENE, 9, 140), DVec3::ZERO);
        assert!(matches!(f.cues.as_slice(), [SfxCue::StartMusic(t)] if t.sequence == 0x1a));
    }

    #[test]
    fn grand_star_fades_out_at_the_end() {
        let mut s = scene();
        let f = s.step(&view(ACT_JUMBO_STAR_CUTSCENE, 2, 515), DVec3::new(0., 500., 400.));
        assert!(f.camera.is_some());
        assert_eq!(f.opacity, Some(0.5));
    }
}
