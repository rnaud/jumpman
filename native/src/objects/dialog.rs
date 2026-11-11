//! Dialogs and "modal" ticks: signposts (`signpost.ts`), the King Bob-omb dialog
//! (`kingDialog.ts` + `Game.stepWorld`), the Bob-omb buddy / cannon operator, and the Koopa the
//! Quick / racing penguin question freeze. While a modal runs, the regular 30 Hz tick is replaced
//! (`ModalActive`) by a Mario-only step and the camera is overridden here.
use crate::button_marker::BorderRadius;
use super::cannons::CannonSet;
use crate::camera::npc_dialog::NpcDialogCamera;
use crate::camera::{CameraRig, MainCamera};
use crate::game::cannons::{CannonOperator, OperatorOutput};
use crate::game::runner::{GameSim, SimEvent};
use crate::game::signpost::{king_dialog_pages, readable_sign, DialogBox, SignReading, SignSound};
use crate::progress::flow::Progress;
use crate::progress::CourseSwitched;
use crate::input::PlayerInput;
use crate::render::to_bevy;
use crate::sim_plugin::{ModalActive, PostTickSet, PreTickSet, SimEventMsg};
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::behaviors::koopa_the_quick::{KOOPA_RACE_COUNTDOWN, KOOPA_RACE_INVITATION, KOOPA_RACE_RESULT_DIALOG};
use jumpman_sim::object::{ObjId, ObjectBehaviorKind as K};
use jumpman_sim::object_manager::ObjectEvent;
use jumpman_sim::state::*;

pub struct DialogPlugin;

/// Triggers that may start a modal; they run first in [`PreTickSet`].
#[derive(SystemSet, Hash, Eq, PartialEq, Clone, Debug)]
pub struct TriggerSet;

impl Plugin for DialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DialogUi>()
            .init_resource::<ModalState>()
            .init_resource::<CameraOverride>()
            .add_message::<SignSoundMsg>()
            .configure_sets(FixedUpdate, TriggerSet.in_set(PreTickSet))
            .add_systems(Startup, spawn_dialog_ui.in_set(crate::progress::PersistentSpawn))
            .add_systems(FixedUpdate, (race_freeze, sign_trigger).chain().in_set(TriggerSet))
            .add_systems(FixedUpdate, king_trigger.in_set(PostTickSet))
            .add_systems(FixedUpdate, (modal_step, tail_step).chain().after(PreTickSet).after(crate::SimSet))
            .add_systems(Update, (update_dialog_ui, apply_camera.after(crate::CameraSet), reset_on_switch));
    }
}

#[derive(Resource, Default)]
pub struct DialogUi(pub DialogBox);

#[derive(Message, Clone, Copy, Debug)]
pub struct SignSoundMsg(pub SignSound);

/// A camera taken over from the rig: (eye, focus) at the previous and current tick, plus a
/// field-of-view offset in degrees (cannon raise shake).
#[derive(Resource, Default)]
pub struct CameraOverride {
    pub prev: Option<(DVec3, DVec3)>,
    pub cur: Option<(DVec3, DVec3)>,
    pub fov_offset: f64,
}

impl CameraOverride {
    pub fn set(&mut self, cam: Option<(DVec3, DVec3)>) {
        self.prev = if self.cur.is_some() { self.cur } else { cam };
        self.cur = cam;
    }
}

pub struct KingDialog {
    king: ObjId,
    id: i32,
    pub(crate) page: i32,
    pub(crate) camera: Option<NpcDialogCamera>,
    eye: DVec3,
    returning: bool,
    /// Fight/defeat jingle to start when the dialog ends (`Game.ts`: `kingFight`/`kingDefeat`).
    pub(crate) music: Option<crate::audio::MusicEvent>,
}

impl KingDialog {
    pub(crate) fn new(king: ObjId, id: i32, eye: DVec3) -> Self {
        Self { king, id, page: -1, camera: None, eye, returning: false, music: None }
    }
}

pub enum Modal {
    Sign(SignReading),
    King(KingDialog),
    Operator(CannonOperator),
}

#[derive(Resource, Default)]
pub struct ModalState {
    pub modal: Option<Modal>,
    /// The race question tick: the sim already ran an objects-only step this tick.
    one_shot: bool,
    /// King camera that finishes its return after the dialog ended.
    pub(crate) tail: Option<NpcDialogCamera>,
    /// The tail was created this tick: its first step is the next tick's (`Game.stepWorld` steps the return at the start of a tick).
    pub(crate) tail_fresh: bool,
    /// The modal started inside this tick's `tick` (cannon operator): its first step is the next tick's.
    fresh: bool,
}

/// The Bob-omb buddy check of `Game.stepWorld` (inside `GameSim::tick_with`, see `objects::stage`)
/// started a cannon operator.
pub fn start_operator(state: &mut ModalState, active: &mut ModalActive, op: CannonOperator) {
    start_modal(state, active, Modal::Operator(op));
    state.fresh = true;
}

// ---------------------------------------------------------------------------
// Dialog box UI
// ---------------------------------------------------------------------------

#[derive(Component)]
struct DialogRoot;
#[derive(Component)]
struct DialogBoxNode;
#[derive(Component)]
struct DialogLines;

const FONT: f32 = 26.0;
const LINE_H: f32 = FONT * 1.4;

fn spawn_dialog_ui(mut commands: Commands) {
    commands
        .spawn((
            DialogRoot,
            Node { position_type: PositionType::Absolute, bottom: Val::Percent(7.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                DialogBoxNode,
                Node {
                    min_width: Val::Px(520.0),
                    max_width: Val::Percent(80.0),
                    padding: UiRect::axes(Val::Px(28.0), Val::Px(16.0)),
                    border: UiRect::all(Val::Px(3.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                // The v3 dialog frame: navy gradient, light top-left and dark bottom-right bevel, dark outline ring.
                BackgroundColor(crate::shell::theme::hex(0x1c2a70)),
                BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::percent(crate::shell::theme::hex(0x2a3a8a), 0.0), ColorStop::percent(crate::shell::theme::hex(0x141c58), 100.0)])),
                BorderColor { top: crate::shell::theme::hex(0x6f86e0), left: crate::shell::theme::hex(0x6f86e0), right: crate::shell::theme::hex(0x0a0f35), bottom: crate::shell::theme::hex(0x0a0f35) },
                BoxShadow::new(crate::shell::theme::hex(0x040720), Val::Px(0.0), Val::Px(0.0), Val::Px(2.0), Val::Px(0.0)),
                BorderRadius::all(Val::Px(0.0)),
            ))
            .with_children(|b| {
                b.spawn((DialogLines, Node { flex_direction: FlexDirection::Column, ..default() }));
            });
        });
}

fn update_dialog_ui(
    mut commands: Commands,
    ui: Res<DialogUi>,
    mut root: Single<&mut Visibility, With<DialogRoot>>,
    mut boxn: Single<(&mut Node, &mut Transform), (With<DialogBoxNode>, Without<DialogLines>)>,
    lines: Single<(Entity, &mut Node), (With<DialogLines>, Without<DialogBoxNode>)>,
    mut shown: Local<Option<String>>,
) {
    let (lines_entity, mut lines_node) = lines.into_inner();
    let b = &ui.0;
    **root = if b.text.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    let (node, tf) = &mut *boxn;
    tf.scale = Vec3::splat(b.scale.max(0.01) as f32);
    node.height = if b.height_lines > 0.0 { Val::Px(b.height_lines as f32 * LINE_H + 8.0) } else { Val::Auto };
    lines_node.top = Val::Px(-(b.scroll_lines as f32) * LINE_H);
    if *shown != b.text {
        *shown = b.text.clone();
        commands.entity(lines_entity).despawn_related::<Children>();
        if let Some(text) = &b.text {
            commands.entity(lines_entity).with_children(|p| {
                for line in text.split('\n') {
                    p.spawn((Text::new(line), TextFont { font_size: FontSize::Px(FONT), ..default() }, TextColor(Color::WHITE), TextShadow { offset: Vec2::splat(2.0), color: crate::shell::theme::SHADOW_INK }, Node { min_height: Val::Px(LINE_H), ..default() }));
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

fn start_modal(state: &mut ModalState, active: &mut ModalActive, modal: Modal) {
    state.modal = Some(modal);
    active.0 = true;
}

/// Koopa the Quick: while he asks a question only the objects move (`GameSim::koopa_question`).
/// The racing penguin's question is handled inside `GameSim::tick_with`.
fn race_freeze(mut sim: ResMut<GameSim>, mut input: ResMut<PlayerInput>, mut state: ResMut<ModalState>, mut active: ResMut<ModalActive>, mut out: MessageWriter<SimEventMsg>) {
    let sim = &mut *sim;
    let asking = sim.mario.objects.find_by_behavior(K::KoopaTheQuick).is_some_and(|k| {
        let a = sim.mario.objects[k].action;
        a == KOOPA_RACE_INVITATION || a == KOOPA_RACE_COUNTDOWN || a == KOOPA_RACE_RESULT_DIALOG
    });
    if !asking {
        return;
    }
    let controls = input.consume_step();
    sim.koopa_question(&controls);
    for e in sim.events.drain(..) {
        out.write(SimEventMsg(e));
    }
    state.one_shot = true;
    active.0 = true;
}

fn sign_trigger(
    mut sim: ResMut<GameSim>,
    input: Res<PlayerInput>,
    rig: Res<CameraRig>,
    mut state: ResMut<ModalState>,
    mut active: ResMut<ModalActive>,
    mut sounds: MessageWriter<SignSoundMsg>,
) {
    if active.0 || input.peek_pressed() == 0 {
        return;
    }
    let sim = &mut *sim;
    let clip = sim.anim.current_clip();
    let Some(sign) = readable_sign(&sim.course.extras.signs, &sim.mario, clip).cloned() else { return };
    let (reading, cues) = SignReading::new(&sign, &mut sim.mario, rig.eye, rig.look_at);
    for s in cues {
        sounds.write(SignSoundMsg(s));
    }
    start_modal(&mut state, &mut active, Modal::Sign(reading));
}

/// `processObjectEvents` 'kingDialog': Mario starts reading, the dialog camera takes over.
fn king_trigger(mut events: MessageReader<SimEventMsg>, mut sim: ResMut<GameSim>, rig: Res<CameraRig>, mut state: ResMut<ModalState>, mut active: ResMut<ModalActive>) {
    for e in events.read() {
        if let SimEvent::Object(ObjectEvent::KingDialog { king, id }) = &e.0 {
            sim.mario.used_obj = Some(*king);
            sim.mario.set_action(ACT_READING_NPC_DIALOG, 2.0);
            start_modal(&mut state, &mut active, Modal::King(KingDialog::new(*king, *id as i32, rig.eye)));
        }
        if let SimEvent::Object(ObjectEvent::Dialogue { .. }) = &e.0 {}
    }
}

// ---------------------------------------------------------------------------
// Modal ticks
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn modal_step(
    mut sim: ResMut<GameSim>,
    mut input: ResMut<PlayerInput>,
    rig: Res<CameraRig>,
    mut state: ResMut<ModalState>,
    mut active: ResMut<ModalActive>,
    mut ui: ResMut<DialogUi>,
    mut camera: ResMut<CameraOverride>,
    mut cannons: Option<ResMut<CannonSet>>,
    mut colliders: Option<ResMut<super::colliders::ObjectColliders>>,
    mut progress: ResMut<Progress>,
    mut sounds: MessageWriter<SignSoundMsg>,
    mut out: MessageWriter<SimEventMsg>,
    mut cues: MessageWriter<crate::audio::SfxCue>,
) {
    if !active.0 {
        return;
    }
    if std::mem::take(&mut state.fresh) {
        return;
    }
    if state.one_shot {
        state.one_shot = false;
        active.0 = false;
        return;
    }
    let Some(modal) = state.modal.take() else {
        active.0 = false;
        camera.set(None);
        return;
    };
    let controls = input.consume_step();
    sim.camera_yaw = rig.yaw_s16();
    let sim = &mut *sim;
    match modal {
        Modal::Sign(mut reading) => {
            sim.tick_mario_only(&controls);
            let (done, cues) = reading.step(&mut sim.mario, &controls, &mut ui.0);
            for s in cues {
                sounds.write(SignSoundMsg(s));
            }
            camera.set(reading.camera_active().then(|| reading.camera()));
            if done {
                active.0 = false;
                camera.set(None);
            } else {
                state.modal = Some(Modal::Sign(reading));
            }
        }
        Modal::King(mut k) => {
            let done = step_king(&mut k, sim, &controls, (rig.eye, rig.look_at), &mut ui.0, &mut camera, &mut state);
            if let Some(e) = k.music.take() {
                cues.write(crate::audio::SfxCue::Music(e));
            }
            if done {
                active.0 = false;
            } else {
                state.modal = Some(Modal::King(k));
            }
        }
        Modal::Operator(mut op) => {
            let Some(cannons) = cannons.as_deref_mut() else {
                active.0 = false;
                return;
            };
            let before = op.text();
            let mut res = OperatorOutput::default();
            let cam = op.step(&mut sim.mario, &controls, (rig.eye, rig.look_at), &mut cannons.0, &mut res);
            if sim.mario.action == ACT_READING_NPC_DIALOG {
                sim.tick_mario_only(&controls);
            }
            // The objects (the buddy turning) still need their snapshots refreshed.
            sim_after_mario(sim, &mut out);
            if let Some(colliders) = colliders.as_deref_mut() {
                super::colliders::sync(sim, colliders, progress.save.wing_switch_on());
            }
            if before != op.text() || res.finished {
                match (res.finished, op.text()) {
                    (false, Some(t)) => ui.0.show(t),
                    _ => ui.0.hide(),
                }
            }
            if let Some(p) = res.play_door {
                cues.write(crate::audio::SfxCue::CannonDoor { pos: p.to_array() });
            }
            if res.save_unlock {
                progress.save.open_cannon(&sim.course.request.id.clone());
            }
            camera.set(cam);
            if res.finished {
                active.0 = false;
                camera.set(None);
            } else {
                state.modal = Some(Modal::Operator(op));
            }
        }
    }
}

fn sim_after_mario(sim: &mut GameSim, out: &mut MessageWriter<SimEventMsg>) {
    jumpman_sim::object_manager::after_mario_step(&mut sim.mario);
    for e in sim.events.drain(..) {
        out.write(SimEventMsg(e));
    }
}

/// One tick of the King Bob-omb dialog. Returns true when it ends.
pub(crate) fn step_king(k: &mut KingDialog, sim: &mut GameSim, controls: &ControllerSnapshot, rig: (DVec3, DVec3), ui: &mut DialogBox, camera: &mut CameraOverride, state: &mut ModalState) -> bool {
    let m = &mut sim.mario;
    if k.returning {
        let cam = k.camera.as_mut().unwrap();
        let (e, f) = cam.step_return(&m.world);
        camera.set(Some((e, f)));
        if cam.return_frames() <= 3.0 {
            m.action_state = 9.0;
            m.objects[k.king].set_var("dialogDone", 1.0);
            state.tail = k.camera.take();
            state.tail_fresh = true;
            return true;
        }
        return false;
    }
    if k.id != 116 {
        let angle = m.objects[k.king].angle_to_mario;
        m.objects[k.king].turn_toward_angle(angle, 0x800 as f64);
        m.objects[k.king].face_yaw = m.objects[k.king].move_yaw;
    }
    sim.tick_mario_only(controls);
    let m = &mut sim.mario;
    let king = &m.objects[k.king];
    if m.action_state == 8.0 && (k.id == 116 || king.move_yaw == king.angle_to_mario) {
        let pages = king_dialog_pages(k.id);
        let (kpos, kheight) = (king.pos, king.hitbox.height);
        let cam = k.camera.get_or_insert_with(|| NpcDialogCamera::new(rig.0, rig.1, m.pos, kpos, kheight));
        let (e, f) = cam.step(k.eye, m.pos);
        k.eye = e;
        camera.set(Some((e, f)));
        if k.page < 0 {
            if cam.frame == 11 {
                k.page += 1;
                ui.show(pages[0]);
            }
        } else if controls.button_pressed & (A_BUTTON | B_BUTTON) != 0 {
            k.page += 1;
            if (k.page as usize) < pages.len() {
                ui.show(pages[k.page as usize]);
            } else {
                ui.hide();
                cam.begin_return(k.eye);
                k.music = Some(if k.id == 116 { crate::audio::MusicEvent::KingDefeat } else { crate::audio::MusicEvent::KingFight });
                k.page = -1;
                k.returning = true;
            }
        }
    }
    false
}

/// Finish the king camera's return after the dialog itself ended (`Game.stepWorld` steps it at the
/// start of every tick and drops the camera when the transition is over).
pub(crate) fn step_tail(state: &mut ModalState, world: &jumpman_sim::collision::CollisionWorld, camera: &mut CameraOverride) {
    if std::mem::take(&mut state.tail_fresh) {
        return;
    }
    let Some(cam) = state.tail.as_mut() else { return };
    let (e, f) = cam.step_return(world);
    camera.set(Some((e, f)));
    if cam.return_frames() <= 0.0 {
        state.tail = None;
        camera.set(None);
    }
}

fn tail_step(sim: Res<GameSim>, mut state: ResMut<ModalState>, mut camera: ResMut<CameraOverride>) {
    step_tail(&mut state, &sim.mario.world, &mut camera);
}

fn apply_camera(
    camera: Res<CameraOverride>,
    fixed: Res<Time<Fixed>>,
    mut main: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    mut touched_fov: Local<bool>,
) {
    let (tf, projection) = &mut *main;
    if let (Some(prev), Some(cur)) = (camera.prev, camera.cur) {
        let t = fixed.overstep_fraction() as f64;
        let eye = prev.0.lerp(cur.0, t);
        let focus = prev.1.lerp(cur.1, t);
        **tf = Transform::from_translation(to_bevy(eye.to_array())).looking_at(to_bevy(focus.to_array()), Vec3::Y);
    }
    // Cannon raise shake (`fovOffset`): only touch the projection while it is in play.
    if let Projection::Perspective(p) = &mut **projection {
        if camera.fov_offset != 0.0 {
            p.fov = (50.0 + camera.fov_offset as f32).to_radians();
            *touched_fov = true;
        } else if *touched_fov {
            p.fov = 50f32.to_radians();
            *touched_fov = false;
        }
    }
}


/// A course switch replaces the sim: drop any dialog or modal that referred to the old one.
fn reset_on_switch(
    mut switched: MessageReader<CourseSwitched>,
    mut state: ResMut<ModalState>,
    mut active: ResMut<ModalActive>,
    mut ui: ResMut<DialogUi>,
    mut camera: ResMut<CameraOverride>,
) {
    if switched.read().count() > 0 {
        *state = ModalState::default();
        active.0 = false;
        ui.0.hide();
        camera.set(None);
        camera.fov_offset = 0.0;
    }
}
