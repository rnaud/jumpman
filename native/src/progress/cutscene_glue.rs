//! Presentation side of the SM64 cutscene actions (`jumpman_sim::cutscene_ext`).
//!
//! * the **cue pump** drains `MarioRuntime::cut.cues` every sim tick and re-publishes them as
//!   [`CutCueMsg`] (the camera agent, audio and the credits driver read those);
//! * **automatic dialogs** (`ACT_READING_AUTOMATIC_DIALOG`): the SM64 dialog table pages
//!   (`sm64_dialogs.rs`) shown in the shared dialog box, A / B turns the page;
//! * the **classic intro** (`ACT_INTRO_CUTSCENE`): Peach's letter (DIALOG_020), the camera push toward
//!   the castle-grounds warp pipe, the pipe rising / lowering and Mario's jump out of it. Started from the
//!   "Classic intro and credits" setting on a fresh castle-grounds load, or `JUMPMAN_INTRO=1`;
//! * the intro **warp pipe** model, driven from `cut.intro_pipe_y`.
//!
//! The camera side belongs to `camera/` (`CameraScript` is the hook, set here with a plain
//! dolly; the real `CUTSCENE_INTRO_PEACH` spline and Lakitu are not ported).
use super::CourseScoped;
use crate::camera::CameraScript;
use crate::game::runner::GameSim;
use crate::smfont::{sm_text, SmText};
use crate::progress::flow::{Flow, Progress};
use crate::render::{scene, to_bevy, AssetRoot};
use crate::shell::settings::Settings;
use bevy::math::DVec3;
use bevy::prelude::*;
use jumpman_sim::cutscene_ext::{CameraEvent, CutCue, INTRO_PIPE_POS};
use jumpman_sim::sm64_dialogs::{dialog, DIALOG_PEACH_LETTER};
use jumpman_sim::state::*;

pub const PIPE_MODEL: &str = "actors/warp_pipe/warp_pipe_geo.glb";

/// Mario's start in the intro (`MARIO_POS(1, 180, -1328, 260, 4664)` of castle grounds).
pub const INTRO_MARIO_POS: [f64; 3] = [-1328.0, 260.0, 4664.0];

/// A cue forwarded from the sim (one message per cue).
#[derive(Message, Clone, Debug)]
pub struct CutCueMsg(pub CutCue);

/// The dialog the automatic-dialog action is showing.
#[derive(Resource, Default, Debug)]
pub struct AutoDialog {
    pub id: u32,
    pub pages: Vec<String>,
    pub page: usize,
}

impl AutoDialog {
    pub fn open(&self) -> bool {
        !self.pages.is_empty()
    }
}

/// Dialog text split into boxes of `lines_per_box` lines, with the button tokens spelled out.
pub fn dialog_pages(id: u32) -> Vec<String> {
    let Some(d) = dialog(id) else { return vec![] };
    let text = d.text.replace("[A]", "A").replace("[B]", "B").replace("[C]", "C").replace("[Z]", "Z").replace("[R]", "R").replace("[%]", "%").replace("//", "  ");
    let lines: Vec<&str> = text.split('\n').collect();
    lines.chunks(d.lines_per_box.max(1) as usize).map(|c| c.join("\n").trim_end().to_string()).collect()
}

#[derive(Resource, Default, Debug)]
pub struct IntroRun {
    pub active: bool,
    /// Sim ticks since the intro began.
    pub frame: u32,
    /// Tick the letter was dismissed (`camera_event` cleared).
    pub letter_done: Option<u32>,
    /// Course the intro already ran in this session.
    pub played: bool,
}

#[derive(Component)]
struct IntroPipe;

pub struct CutsceneGluePlugin;

impl Plugin for CutsceneGluePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AutoDialog>()
            .init_resource::<IntroRun>()
            .add_message::<CutCueMsg>()
            .add_systems(Startup, spawn_dialog_box.in_set(crate::progress::PersistentSpawn))
            .add_systems(Update, sync_dialog_box)
            .add_systems(
                FixedUpdate,
                (cue_pump, auto_dialog, intro_driver, sync_pipe).chain().after(crate::SimSet).run_if(crate::shell::is_playing),
            )
            .add_systems(Update, start_intro_on_switch.after(super::apply_switch));
    }
}

fn cue_pump(mut sim: ResMut<GameSim>, mut out: MessageWriter<CutCueMsg>, settings: Option<Res<Settings>>) {
    // The classic setting also turns on the cap pickup cutscene (`interact_cap`).
    sim.mario.cut.cap_cutscene = settings.is_some_and(|s| s.classic_intro);
    for c in sim.mario.cut.take_cues() {
        out.write(CutCueMsg(c));
    }
}

fn auto_dialog(mut sim: ResMut<GameSim>, mut cues: MessageReader<CutCueMsg>, mut dlg: ResMut<AutoDialog>) {
    for CutCueMsg(c) in cues.read() {
        if let CutCue::Dialog(id) = c {
            dlg.id = *id;
            dlg.pages = dialog_pages(*id);
            dlg.page = 0;
            if dlg.pages.is_empty() {
                sim.mario.cut.dialog_open = false;
            }
        }
    }
    if !dlg.open() {
        return;
    }
    // A (or B) turns the page; the press that opened the box is not the one that closes it.
    let pressed = sim.mario.controller.button_pressed;
    if pressed & (A_BUTTON | B_BUTTON) != 0 {
        dlg.page += 1;
        if dlg.page >= dlg.pages.len() {
            dlg.pages.clear();
            sim.mario.cut.dialog_open = false;
        }
    }
}

// ------------------------------------------------------------------ the SM64-style dialog box

#[derive(Component)]
struct DialogBoxRoot;
#[derive(Component)]
struct DialogBoxLines;

const DIALOG_LINES: usize = 6;

fn spawn_dialog_box(mut commands: Commands) {
    commands
        .spawn((
            DialogBoxRoot,
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), bottom: Val::Percent(9.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
            GlobalZIndex(35),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                DialogBoxLines,
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), padding: UiRect::axes(Val::Px(30.0), Val::Px(18.0)), min_width: Val::Px(420.0), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
            ))
            .with_children(|b| {
                // A fixed pool of lines: the text is swapped in place (removing `SmText` entities races the font system).
                for i in 0..DIALOG_LINES {
                    b.spawn((DialogLine(i), sm_text(" ", Color::WHITE, 2.0)));
                }
            });
        });
}

#[derive(Component)]
struct DialogLine(usize);

/// Draw the current page of the automatic dialog (or Peach's letter) in the SM64 menu font.
fn sync_dialog_box(dlg: Res<AutoDialog>, mut root: Single<&mut Visibility, With<DialogBoxRoot>>, mut lines: Query<(&DialogLine, &mut SmText, &mut Node)>) {
    **root = if dlg.open() { Visibility::Inherited } else { Visibility::Hidden };
    let page = dlg.pages.get(dlg.page).cloned().unwrap_or_default();
    let rows: Vec<&str> = page.lines().collect();
    for (line, mut text, mut node) in &mut lines {
        match rows.get(line.0) {
            Some(r) => {
                SmText::set(&mut text, if r.is_empty() { " " } else { r });
                node.display = Display::Flex;
            }
            None => node.display = Display::None,
        }
    }
}

/// Put Mario at the castle-grounds pipe and start `ACT_INTRO_CUTSCENE`.
pub fn begin_intro(sim: &mut GameSim, run: &mut IntroRun) {
    let m = &mut sim.mario;
    m.pos = INTRO_MARIO_POS;
    m.face_angle = [0.0, 0x8000 as f64, 0.0];
    m.cut.camera_cutscene_done = false;
    m.set_action(ACT_INTRO_CUTSCENE, 0.0);
    run.active = true;
    run.frame = 0;
    run.letter_done = None;
    run.played = true;
}

fn start_intro_on_switch(
    mut switched: MessageReader<crate::progress::CourseSwitched>,
    mut sim: ResMut<GameSim>,
    mut run: ResMut<IntroRun>,
    settings: Option<Res<Settings>>,
    progress: Res<Progress>,
    mut first: Local<bool>,
) {
    let env = std::env::var("JUMPMAN_INTRO").is_ok();
    let enabled = env || settings.as_ref().is_some_and(|s| s.classic_intro);
    let fresh = switched.read().count() > 0 || !*first;
    *first = true;
    if !fresh || run.active || !enabled || sim.course.request.id != "castle_grounds" {
        return;
    }
    // Once per session, and only on a new save (no stars yet), unless forced by the env hook.
    if !env && (run.played || progress.save.stars() > 0) {
        return;
    }
    begin_intro(&mut sim, &mut run);
}

/// The intro's script around the sim action: the letter, the camera dolly and when the camera
/// hands control back.
fn intro_driver(mut sim: ResMut<GameSim>, mut run: ResMut<IntroRun>, mut dlg: ResMut<AutoDialog>, mut script: ResMut<CameraScript>, mut progress: ResMut<Progress>) {
    if !run.active {
        return;
    }
    if sim.mario.action != ACT_INTRO_CUTSCENE {
        run.active = false;
        script.0 = None;
        sim.mario.cut.camera_event = None;
        sim.mario.cut.camera_cutscene_done = true;
        progress.fade = 0.0;
        return;
    }
    run.frame += 1;
    let pipe = DVec3::from_array(INTRO_PIPE_POS);
    let m = &mut sim.mario;
    let arg = m.action_arg as i32;
    // Fade in from white-ish black over the first 90 ticks (`WARP_TRANSITION_FADE_FROM_COLOR`, 0x5A frames).
    progress.fade = (1.0 - run.frame as f32 / 90.0).clamp(0.0, 1.0);
    if arg == 1 && m.cut.camera_event == Some(CameraEvent::StartIntro) {
        // Peach's letter: shown after a beat, dismissed with A / B or after 12 seconds.
        if run.frame == 45 {
            dlg.id = DIALOG_PEACH_LETTER;
            dlg.pages = dialog_pages(DIALOG_PEACH_LETTER);
            dlg.page = 0;
        }
        let pressed = m.controller.button_pressed & (A_BUTTON | B_BUTTON) != 0;
        if run.frame > 60 && (pressed || !dlg.open() && run.frame > 50 || run.frame > 45 + 360) {
            dlg.pages.clear();
            m.cut.camera_event = None;
            run.letter_done = Some(run.frame);
        }
    }
    // Camera: a slow push from the castle side toward the pipe, then hold on it.
    let t = (run.frame as f64 / 240.0).min(1.0);
    let ease = t * t * (3.0 - 2.0 * t);
    let start = pipe + DVec3::new(900.0, 1600.0, -3600.0);
    let end = pipe + DVec3::new(0.0, 560.0, -1250.0);
    let eye = start.lerp(end, ease);
    let focus = pipe + DVec3::new(0.0, 200.0 + 120.0 * ease, 0.0);
    script.0 = Some((eye, focus));
    if arg >= 6 {
        // `gCamera->cutscene == 0`: hand the camera back a few frames after Mario lands.
        m.cut.camera_cutscene_done = m.action_timer > 20.0 || run.frame > 2000;
        if m.cut.camera_cutscene_done {
            script.0 = None;
        }
    }
}

fn sync_pipe(
    mut commands: Commands,
    sim: Res<GameSim>,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut pipes: Query<(Entity, &mut Transform), With<IntroPipe>>,
) {
    let y = sim.mario.cut.intro_pipe_y;
    match (y, pipes.iter_mut().next()) {
        (Some(y), Some((_, mut t))) => {
            t.translation = to_bevy([INTRO_PIPE_POS[0], y, INTRO_PIPE_POS[2]]);
        }
        (Some(y), None) if root.exists(PIPE_MODEL) => {
            commands.spawn((
                IntroPipe,
                CourseScoped,
                scene(&assets, PIPE_MODEL),
                Transform::from_translation(to_bevy([INTRO_PIPE_POS[0], y, INTRO_PIPE_POS[2]])).with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            ));
        }
        (None, _) => {
            for (e, _) in &pipes {
                commands.entity(e).despawn();
            }
        }
        _ => {}
    }
}

/// Keep the unused `Flow` import honest for the credits module.
pub fn is_cutscene_flow(flow: &Flow) -> bool {
    matches!(flow, Flow::Playing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peach_letter_is_one_six_line_box() {
        let pages = dialog_pages(DIALOG_PEACH_LETTER);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].starts_with("Dear Mario:"));
        assert_eq!(pages[0].lines().count(), 6);
    }

    #[test]
    fn long_dialogs_page_by_lines_per_box() {
        // DIALOG_000: 6 lines per box.
        let pages = dialog_pages(0);
        assert!(pages.len() > 1);
        assert!(pages.iter().all(|p| p.lines().count() <= 6));
        assert!(pages.join("\n").contains("(Press B"), "button tokens spelled out");
    }
}
