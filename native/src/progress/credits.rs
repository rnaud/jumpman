//! The classic ending roll (`level_update.c` `sCreditsSequence`, `warp_credits`,
//! `act_credits_cutscene`, `act_end_waving_cutscene`): after Peach's cake scene the game visits 20 course
//! spots one after another (Mario standing in `ACT_CREDITS_CUTSCENE`, the 3D view shrinking into a corner
//! while the staff names show), ends on the castle roof with Mario waving next to Peach and the Toads,
//! then shows the cake "Thank You" screen (`levels/ending/cake`).
//!
//! The sim side is `jumpman_sim::cutscene_ext`; this module is the driver (course loads, fades, the
//! viewport mask and the text). Enabled by the "Classic intro and credits" setting; with it off the old
//! one-card ending (`Flow::Credits`) is unchanged. Courses the native game cannot load yet are skipped.
//! The credits camera of the original (`CAM_EVENT_START_CREDITS` orbits) belongs to the camera layer: the
//! normal follow camera is used.
use super::cutscene_glue::CutCueMsg;
use super::flow::{Flow, Progress, Switch};
use super::CourseSwitched;
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use crate::hud::HudMinimal;
use crate::shell::settings::Settings;
use bevy::prelude::*;
use jumpman_sim::cutscene_ext::{start_credits_scene, CreditsEntry, CutCue, WarpOp, CREDITS_SEQUENCE};
use jumpman_sim::state::*;

/// `WARP_OP_CREDITS_NEXT` fades 20 frames (60 for the first scene); `CREDITS_END` 60.
const FADE_NEXT: u32 = 20;
const FADE_END: u32 = 60;
const FADE_IN: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// A course switch was queued for `CREDITS_SEQUENCE[index]`; Mario is placed when it lands.
    Loading,
    Playing,
    FadeOut { left: u32, then_cake: bool },
}

#[derive(Resource, Default, Debug)]
pub struct CreditsRun {
    pub phase: Phase,
    /// Index into `CREDITS_SEQUENCE` of the scene being shown / loaded.
    pub index: usize,
    fade_in: u32,
    /// The cake "Thank You" card is up.
    pub cake: bool,
}

impl CreditsRun {
    pub fn active(&self) -> bool {
        self.phase != Phase::Idle
    }
}

#[derive(Component)]
struct CreditsRoot;
#[derive(Component)]
struct CreditsBar(usize);
#[derive(Component)]
struct CreditsText(usize);
const TEXT_SLOTS: usize = 6;
#[derive(Component)]
struct CakeRoot;

pub struct CreditsPlugin;

impl Plugin for CreditsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreditsRun>()
            .add_systems(Startup, spawn_overlay.in_set(crate::progress::PersistentSpawn))
            .add_systems(FixedUpdate, (debug_scene, driver).chain().after(crate::SimSet).run_if(crate::shell::is_playing))
            .add_systems(Update, (place_mario_on_switch.after(super::apply_switch), sync_overlay));
    }
}

/// The course request of a credits scene (`gCurrActNum = unk02 & 7`).
pub fn scene_request(entry: &CreditsEntry) -> CourseRequest {
    let mut r = CourseRequest::new(entry.level);
    r.area = entry.area;
    r.act = (entry.flags & 7).max(1);
    r
}

fn queue_scene(progress: &mut Progress, run: &mut CreditsRun, mut index: usize) -> bool {
    // Skip scenes whose course the native game cannot load.
    while index < CREDITS_SEQUENCE.len() && crate::game::courses::info(CREDITS_SEQUENCE[index].level).is_none() {
        index += 1;
    }
    if index >= CREDITS_SEQUENCE.len() {
        return false;
    }
    run.index = index;
    run.phase = Phase::Loading;
    progress.pending = Some(Switch { request: scene_request(&CREDITS_SEQUENCE[index]), carry: None, reveal: false, completed: None, fresh_run: false });
    true
}

fn finish(progress: &mut Progress, run: &mut CreditsRun, hud: &mut HudMinimal) {
    run.phase = Phase::Idle;
    run.cake = true;
    hud.0 = false;
    progress.fade = 0.0;
    progress.flow = Flow::Credits { timer: 0 };
}

fn driver(
    mut sim: ResMut<GameSim>,
    mut progress: ResMut<Progress>,
    mut run: ResMut<CreditsRun>,
    mut cues: MessageReader<CutCueMsg>,
    settings: Option<Res<Settings>>,
    mut hud: ResMut<HudMinimal>,
) {
    let classic = settings.as_ref().is_some_and(|s| s.classic_intro) || std::env::var("JUMPMAN_CREDITS").is_ok();
    progress.ending.classic_credits = classic;
    if progress.ending.credits_pending {
        progress.ending.credits_pending = false;
        if classic && !run.active() {
            run.cake = false;
            hud.0 = true;
            // Entry 0 is the castle grounds of the Peach scene that just ended; the roll starts at entry 1.
            if !queue_scene(&mut progress, &mut run, 1) {
                finish(&mut progress, &mut run, &mut hud);
            }
        } else {
            progress.flow = Flow::Credits { timer: 0 };
        }
        return;
    }
    if !run.active() {
        // Leaving the cake card returns to normal play.
        if run.cake && !matches!(progress.flow, Flow::Credits { .. }) {
            run.cake = false;
        }
        return;
    }
    // Scene fade-in after the load.
    if run.fade_in > 0 {
        run.fade_in -= 1;
        progress.fade = run.fade_in as f32 / FADE_IN as f32;
    }
    for CutCueMsg(c) in cues.read() {
        match c {
            CutCue::Warp(WarpOp::CreditsNext) if run.phase == Phase::Playing => run.phase = Phase::FadeOut { left: FADE_NEXT, then_cake: false },
            CutCue::Warp(WarpOp::CreditsEnd) if run.phase == Phase::Playing => run.phase = Phase::FadeOut { left: FADE_END, then_cake: true },
            _ => {}
        }
    }
    if let Phase::FadeOut { left, then_cake } = run.phase {
        let total = if then_cake { FADE_END } else { FADE_NEXT };
        progress.fade = 1.0 - left as f32 / total as f32;
        if left == 0 {
            let next = run.index + 1;
            if then_cake || !queue_scene(&mut progress, &mut run, next) {
                finish(&mut progress, &mut run, &mut hud);
            }
        } else {
            run.phase = Phase::FadeOut { left: left - 1, then_cake };
        }
    }
    let _ = &mut sim;
}

/// `JUMPMAN_CREDITS_SCENE=<index>` (with `JUMPMAN_COURSE` / `_ACT` set to that entry's course): jump straight into
/// one credits scene on the first tick, for screenshots.
fn debug_scene(mut sim: ResMut<GameSim>, mut run: ResMut<CreditsRun>, mut progress: ResMut<Progress>, mut hud: ResMut<HudMinimal>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    if std::env::var("JUMPMAN_CREDITS_SCENE").is_ok_and(|v| v == "cake") {
        finish(&mut progress, &mut run, &mut hud);
        return;
    }
    let Some(i) = std::env::var("JUMPMAN_CREDITS_SCENE").ok().and_then(|v| v.parse::<usize>().ok()).filter(|i| *i < CREDITS_SEQUENCE.len()) else { return };
    let entry = &CREDITS_SEQUENCE[i];
    if i == CREDITS_SEQUENCE.len() - 1 {
        let m = &mut sim.mario;
        m.pos = [entry.mario_pos[0] as f64, entry.mario_pos[1] as f64, entry.mario_pos[2] as f64];
        m.face_angle = [0.0, ((entry.mario_angle as i32) << 8) as f64, 0.0];
        m.set_action(ACT_END_WAVING_CUTSCENE, 0.0);
    } else {
        start_credits_scene(&mut sim.mario, entry);
    }
    run.index = i;
    run.phase = Phase::Playing;
    hud.0 = true;
    progress.fade = 0.0;
}

/// When a credits scene's course has loaded, put Mario into the credits action at the entry's spot.
fn place_mario_on_switch(mut switched: MessageReader<CourseSwitched>, mut sim: ResMut<GameSim>, mut run: ResMut<CreditsRun>, mut progress: ResMut<Progress>) {
    if switched.read().count() == 0 || run.phase != Phase::Loading {
        return;
    }
    let entry = &CREDITS_SEQUENCE[run.index];
    let last = run.index == CREDITS_SEQUENCE.len() - 1;
    if last {
        // `WARP_NODE_CREDITS_END`: the waving scene on the castle roof.
        let m = &mut sim.mario;
        m.pos = [entry.mario_pos[0] as f64, entry.mario_pos[1] as f64, entry.mario_pos[2] as f64];
        m.face_angle = [0.0, ((entry.mario_angle as i32) << 8) as f64, 0.0];
        m.set_action(ACT_END_WAVING_CUTSCENE, 0.0);
    } else {
        start_credits_scene(&mut sim.mario, entry);
    }
    run.phase = Phase::Playing;
    run.fade_in = FADE_IN;
    progress.fade = 1.0;
}

// ------------------------------------------------------------------ overlay

fn spawn_overlay(mut commands: Commands, assets: Res<AssetServer>) {
    let full = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands
        .spawn((CreditsRoot, full.clone(), GlobalZIndex(30), Visibility::Hidden))
        .with_children(|p| {
            for i in 0..4 {
                p.spawn((CreditsBar(i), Node { position_type: PositionType::Absolute, ..default() }, BackgroundColor(Color::BLACK)));
            }
            for i in 0..TEXT_SLOTS {
                p.spawn((CreditsText(i), Text::new(" "), crate::shell::theme::pixel_font(22.0), TextColor(Color::srgb(1.0, 0.96, 0.86)), TextShadow { offset: Vec2::splat(2.0), color: Color::srgb(0.02, 0.04, 0.13) }));
            }
        });
    commands
        .spawn((CakeRoot, Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..full }, BackgroundColor(Color::BLACK), GlobalZIndex(60), Visibility::Hidden))
        .with_children(|p| {
            p.spawn((
                ImageNode::new(assets.load("extracted/levels/ending/cake.png")).with_mode(NodeImageMode::Auto),
                Node { height: Val::Percent(92.0), aspect_ratio: Some(316.0 / 228.0), ..default() },
            ));
        });
}

/// `print_displaying_credits_entry`: lines of one entry with their position in the 320x240 screen.
pub fn credits_text_layout(entry: &CreditsEntry) -> Vec<(String, f32, f32, bool)> {
    let mut out = vec![];
    let Some(first) = entry.text.first() else { return out };
    let mut lines = entry.text.iter();
    let title = lines.next().unwrap();
    let num = first.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(1) as i32;
    let title = &title[1..];
    let mut y = (if entry.flags & 0x20 != 0 { 28 } else { 172 } + if num == 1 { 16 } else { 0 }) as f32;
    out.push((title.to_string(), 28.0, y, false));
    let mut names = num;
    let mut line_h = 16.0;
    match num {
        4 => {
            out.push((lines.next().unwrap().to_string(), 28.0, y + 24.0, false));
            names = 2;
            line_h = 24.0;
        }
        5 => {
            out.push((lines.next().unwrap().to_string(), 28.0, y + 16.0, false));
            names = 3;
        }
        _ => {}
    }
    for _ in 0..names {
        if let Some(n) = lines.next() {
            out.push((n.to_string(), 292.0, y, true));
            y += line_h;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn sync_overlay(
    sim: Res<GameSim>,
    run: Res<CreditsRun>,
    progress: Res<Progress>,
    mut root: Single<&mut Visibility, (With<CreditsRoot>, Without<CakeRoot>)>,
    mut cake: Single<&mut Visibility, (With<CakeRoot>, Without<CreditsRoot>)>,
    mut bars: Query<(&CreditsBar, &mut Node)>,
    mut texts: Query<(&CreditsText, &mut Text, &mut Node), Without<CreditsBar>>,
) {
    **cake = if run.cake && matches!(progress.flow, Flow::Credits { .. }) { Visibility::Inherited } else { Visibility::Hidden };
    let active = run.active() && run.phase == Phase::Playing || matches!(run.phase, Phase::FadeOut { .. });
    **root = if active { Visibility::Inherited } else { Visibility::Hidden };
    if !active {
        return;
    }
    let cut = &sim.mario.cut;
    // The 3D view shrinks into a corner (`sEndCutsceneVp`): black bars outside the viewport rectangle.
    // vscale / vtrans are in N64 viewport units (4 per pixel of the 320x240 screen).
    let rect = cut.credits_viewport.map(|(s, t)| {
        let (cx, cy, hw, hh) = (t[0] as f32 / 4.0, t[1] as f32 / 4.0, s[0] as f32 / 4.0, s[1] as f32 / 4.0);
        ((cx - hw).max(0.0), (cx + hw).min(320.0), (cy - hh).max(0.0), (cy + hh).min(240.0))
    });
    for (bar, mut node) in &mut bars {
        let (l, r, t, b) = rect.unwrap_or((0.0, 320.0, 0.0, 240.0));
        let (x0, x1, y0, y1) = match bar.0 {
            0 => (0.0, 320.0, 0.0, t),
            1 => (0.0, 320.0, b, 240.0),
            2 => (0.0, l, t, b),
            _ => (r, 320.0, t, b),
        };
        let pct = |v: f32, d: f32| Val::Percent(v / d * 100.0);
        node.left = pct(x0, 320.0);
        node.top = pct(y0, 240.0);
        node.width = pct(x1 - x0, 320.0);
        node.height = pct(y1 - y0, 240.0);
        if rect.is_none() {
            node.width = Val::Px(0.0);
        }
    }
    // The staff text: a pool of `TEXT_SLOTS` lines, positioned and set in place.
    let layout = if cut.credits_text_visible { credits_text_layout(&CREDITS_SEQUENCE[run.index]) } else { vec![] };
    for (slot, mut text, mut node) in &mut texts {
        match layout.get(slot.0) {
            Some((line, x, y, right)) => {
                text.0 = line.clone();
                node.position_type = PositionType::Absolute;
                node.display = Display::Flex;
                node.top = Val::Percent(y / 240.0 * 100.0);
                if *right {
                    node.left = Val::Auto;
                    node.right = Val::Percent((320.0 - x) / 320.0 * 100.0);
                } else {
                    node.right = Val::Auto;
                    node.left = Val::Percent(x / 320.0 * 100.0);
                }
            }
            None => node.display = Display::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_scene_is_bob_with_the_game_director() {
        let r = scene_request(&CREDITS_SEQUENCE[1]);
        assert_eq!((r.id.as_str(), r.area, r.act), ("bob", 1, 1));
        let lay = credits_text_layout(&CREDITS_SEQUENCE[1]);
        // numLines == 1: title at 172 + 16 on the left, name at the same line on the right.
        assert_eq!(lay[0], ("GAME DIRECTOR".into(), 28.0, 188.0, false));
        assert_eq!(lay[1], ("SHIGERU MIYAMOTO".into(), 292.0, 188.0, true));
    }

    #[test]
    fn four_line_entries_use_two_titles_and_24_unit_rows() {
        // BBH: "4CAMERA PROGRAMMER", "MARIO FACE PROGRAMMER", names.
        let lay = credits_text_layout(&CREDITS_SEQUENCE[5]);
        assert_eq!(lay.len(), 4);
        assert_eq!((lay[0].2, lay[1].2), (172.0, 196.0));
        assert_eq!((lay[2].2, lay[3].2), (172.0, 196.0));
    }

    #[test]
    fn act_comes_from_the_low_flag_bits() {
        // WF flags 50 = 0b110010: act 2; DDD area 2 flags 145: act 1.
        assert_eq!(scene_request(&CREDITS_SEQUENCE[2]).act, 2);
        assert_eq!(scene_request(&CREDITS_SEQUENCE[19]).act, 1);
        assert_eq!(scene_request(&CREDITS_SEQUENCE[19]).area, 2);
    }

    #[test]
    fn top_row_flag_moves_text_to_the_top() {
        // WF flags 50 has 0x20 set: strY = 28.
        assert_eq!(credits_text_layout(&CREDITS_SEQUENCE[2])[0].2, 28.0);
    }
}
