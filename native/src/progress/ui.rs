//! `bevy_ui` presentation of the progression flow: screen wipes, the act selector, the course
//! result, the bonus-star save prompt and game over, in the navy / gold / cream palette of
//! `src/ui/theme.css`.
use crate::button_marker::{Button, BorderRadius};
use super::flow::{Flow, MenuClick, Progress};
use super::menus::{ActSelector, BonusSave, CourseResult};
use super::wipe::{self, Mask, Masks, WipeKind};
use crate::render::AssetRoot;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

const NAVY: Color = Color::srgb(0.039, 0.086, 0.251);
const NAVY2: Color = Color::srgb(0.078, 0.157, 0.431);
const GOLD: Color = Color::srgb(1.0, 0.8, 0.2);
const GOLD_HI: Color = Color::srgb(1.0, 0.945, 0.627);
const CREAM: Color = Color::srgb(1.0, 0.965, 0.863);
const RED: Color = Color::srgb(0.9, 0.19, 0.16);
const SKY: Color = Color::srgb(0.56, 0.83, 1.0);

pub struct ProgressUiPlugin;

impl Plugin for ProgressUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WipeState>().add_systems(Startup, spawn_overlays.in_set(crate::progress::PersistentSpawn)).add_systems(Update, (sync_wipe, sync_panels, menu_mouse));
    }
}

#[derive(Component)]
struct WipeImage;
#[derive(Component)]
struct FadeNode;
#[derive(Component)]
struct PanelRoot;

/// A clickable menu element.
#[derive(Component, Clone, Copy)]
struct Clickable(MenuClick);

fn menu_mouse(mut progress: ResMut<Progress>, buttons: Query<(&Interaction, &Clickable), Changed<Interaction>>, mouse: Res<ButtonInput<MouseButton>>) {
    for (interaction, target) in &buttons {
        if *interaction == Interaction::Pressed {
            progress.menu_click = Some(target.0);
            return;
        }
    }
    // Credits / game over / act-less prompts: any click acts as A.
    if mouse.just_pressed(MouseButton::Left) && matches!(progress.flow, Flow::GameOver { .. } | Flow::Credits { .. }) {
        progress.menu_click = Some(MenuClick::Confirm);
    }
}

#[derive(Resource, Default)]
struct WipeState {
    masks: Masks,
    handle: Option<Handle<Image>>,
    key: Option<(bool, u32, WipeKind, usize)>,
    panel_key: String,
    buffer: Vec<u8>,
}

fn load_mask(root: &AssetRoot, file: &str) -> Option<Mask> {
    let bytes = std::fs::read(root.0.join("extracted/textures/segment2").join(file)).ok()?;
    let img = Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, false, ImageSampler::nearest(), RenderAssetUsages::default()).ok()?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    // The textures are white intensity + alpha; the transition draws them as black with that alpha.
    let mut data = img.data?;
    for px in data.chunks_exact_mut(4) {
        px[0] = 0;
        px[1] = 0;
        px[2] = 0;
    }
    (data.len() == w * h * 4).then_some(Mask { width: w, height: h, rgba: data })
}

fn spawn_overlays(mut commands: Commands, root: Res<AssetRoot>, mut state: ResMut<WipeState>) {
    state.masks = Masks {
        mario: load_mask(&root, "segment2.10458.ia8.png"),
        bowser: load_mask(&root, "segment2.11458.ia8.png"),
        circle: load_mask(&root, "segment2.0FC58.ia8.png"),
        star: load_mask(&root, "segment2.0F458.ia8.png"),
    };
    let full = Node { position_type: PositionType::Absolute, left: Val::Px(0.), top: Val::Px(0.), width: Val::Percent(100.), height: Val::Percent(100.), ..default() };
    commands.spawn((full.clone(), BackgroundColor(Color::NONE), GlobalZIndex(40), FadeNode));
    commands.spawn((full.clone(), ImageNode::default(), Visibility::Hidden, GlobalZIndex(60), WipeImage));
    commands.spawn((
        Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::Center, row_gap: Val::Px(14.), ..full },
        GlobalZIndex(50),
        PanelRoot,
    ));
}

fn black_image(w: u32) -> Image {
    let mut img = Image::new(
        Extent3d { width: w, height: 240, depth_or_array_layers: 1 },
        TextureDimension::D2,
        [0u8, 0, 0, 255].repeat(w as usize * 240),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    img.sampler = ImageSampler::nearest();
    img
}

fn sync_wipe(
    progress: Res<Progress>,
    mut state: ResMut<WipeState>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window>,
    mut wipe: Query<(&mut ImageNode, &mut Visibility), With<WipeImage>>,
    mut fade: Query<&mut BackgroundColor, With<FadeNode>>,
) {
    if let Ok(mut f) = fade.single_mut() {
        f.0 = Color::srgba(0.0, 0.0, 0.0, progress.fade);
    }
    let Ok((mut node, mut vis)) = wipe.single_mut() else { return };
    // Which wipe frame is on screen?
    let spec = match &progress.flow {
        Flow::Closing { kind, frame, .. } => Some((false, (*frame).min(kind.duration() - 1), *kind)),
        Flow::Entering { frame, .. } if *frame >= 16 => Some((false, (*frame - 16).min(19), WipeKind::Iris)),
        // The game-over card has its own black backdrop; a last wipe frame would paint over its text (z 60 > 50).
        Flow::GameOver { .. } => None,
        Flow::Playing if progress.floor_warp_wipe().is_some() => progress.floor_warp_wipe().map(|f| (false, f, WipeKind::Iris)),
        _ => progress.reveal.map(|f| (true, f.min(15), WipeKind::Mario)),
    };
    let Some((opening, frame, kind)) = spec else {
        *vis = Visibility::Hidden;
        return;
    };
    let Ok(window) = windows.single() else { return };
    let w = wipe::overlay_width(window.width(), window.height()) as u32;
    let key = (opening, frame, kind, w as usize);
    if state.key != Some(key) || state.handle.is_none() {
        let mut buffer = std::mem::take(&mut state.buffer);
        wipe::render(&mut buffer, w as usize, opening, frame, kind, &state.masks);
        match state.handle.clone().and_then(|h| images.get_mut(&h).map(|i| (i.width() == w).then_some(h))).flatten() {
            Some(h) => {
                if let Some(mut img) = images.get_mut(&h) {
                    img.data = Some(buffer.clone());
                }
            }
            None => {
                let mut img = black_image(w);
                img.data = Some(buffer.clone());
                state.handle = Some(images.add(img));
            }
        }
        state.buffer = buffer;
        state.key = Some(key);
    }
    if let Some(h) = &state.handle {
        if node.image != *h {
            node.image = h.clone();
        }
    }
    *vis = Visibility::Inherited;
}

/// A label in the SM64 menu font; `size` is the old bevy_ui pixel size, mapped to an integer glyph scale.
fn text(parent: &mut ChildSpawnerCommands, s: impl Into<String>, size: f32, color: Color) {
    parent.spawn(crate::smfont::sm_text(s, color, ((size / 12.0) * 2.0).round().max(3.0) / 2.0));
}

fn panel_key(flow: &Flow, progress: &Progress) -> String {
    match flow {
        Flow::ActSelect(s) => format!("act:{}:{}:{}", s.course, s.selection, s.hundred_coin),
        Flow::Result(r) => format!("res:{}:{}:{}:{}", r.shown, r.selection, r.options_visible, r.show_record()),
        Flow::Dance(d) => match &d.prompt {
            Some(p) => format!("bonus:{}:{}", p.page, p.choice),
            None => String::new(),
        },
        Flow::GameOver { .. } => format!("over:{}", progress.save.slot),
        Flow::Credits { .. } => "credits".into(),
        _ => String::new(),
    }
}

fn sync_panels(mut commands: Commands, progress: Res<Progress>, mut state: ResMut<WipeState>, root: Query<Entity, With<PanelRoot>>, children: Query<&Children>) {
    let key = panel_key(&progress.flow, &progress);
    if key == state.panel_key {
        return;
    }
    state.panel_key = key;
    let Ok(root) = root.single() else { return };
    if let Ok(kids) = children.get(root) {
        for k in kids.iter() {
            commands.entity(k).despawn();
        }
    }
    commands.entity(root).insert(BackgroundColor(Color::NONE));
    match &progress.flow {
        Flow::ActSelect(sel) => {
            commands.entity(root).insert(BackgroundColor(Color::srgba(0.039, 0.086, 0.251, 0.78))).with_children(|p| act_selector(p, sel, &progress));
        }
        Flow::Result(r) => {
            commands.entity(root).insert(BackgroundColor(Color::srgba(0.039, 0.086, 0.251, 0.55))).with_children(|p| result(p, r));
        }
        Flow::Dance(d) if d.prompt.is_some() => {
            let prompt = d.prompt.clone().unwrap();
            commands.entity(root).with_children(|p| bonus(p, &prompt));
        }
        Flow::GameOver { .. } => {
            commands.entity(root).insert(BackgroundColor(Color::BLACK)).with_children(|p| {
                text(p, "GAME OVER", 72., GOLD);
                text(p, format!("SAVE FILE {}   PRESS A TO CONTINUE", (b'A' + progress.save.slot.saturating_sub(1) as u8) as char), 20., CREAM);
            });
        }
        Flow::Credits { .. } => {
            commands.entity(root).with_children(|p| {
                text(p, "You collected the Grand Star!", 54., GOLD);
                text(p, "PRESS A TO RETURN TO THE CASTLE", 20., CREAM);
            });
        }
        _ => {}
    }
}

fn card() -> (Node, BackgroundColor, BorderColor, BorderRadius) {
    (
        Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(Val::Px(36.), Val::Px(24.)), row_gap: Val::Px(12.), border: UiRect::all(Val::Px(4.)), ..default() },
        BackgroundColor(NAVY),
        BorderColor::all(GOLD),
        BorderRadius::all(Val::Px(22.)),
    )
}

fn act_selector(p: &mut ChildSpawnerCommands, sel: &ActSelector, progress: &Progress) {
    let course_number = crate::game::courses::SCORE_COURSES.iter().position(|c| *c == sel.course).map_or(0, |i| i + 1);
    let info = crate::game::courses::info(&sel.course);
    let missions = crate::game::courses::missions_for(&sel.course);
    p.spawn(card()).with_children(|c| {
        text(c, format!("COURSE {course_number}"), 22., GOLD);
        text(c, info.map_or("", |i| i.name).to_uppercase(), 44., CREAM);
        c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(16.), align_items: AlignItems::Center, margin: UiRect::vertical(Val::Px(10.)), ..default() }).with_children(|row| {
            for act in 1..=sel.choices.visible {
                let collected = sel.collected[act as usize - 1];
                let selected = act == sel.selection;
                let open = sel.choices.selectable.contains(&act);
                let size = if selected { 86. } else { 66. };
                let (bg, fg) = if collected { (GOLD, NAVY) } else if open { (CREAM, NAVY) } else { (NAVY2, SKY) };
                row.spawn((
                    Button,
                    Clickable(MenuClick::Act(act)),
                    Node { width: Val::Px(size), height: Val::Px(size), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(if selected { 6. } else { 3. })), ..default() },
                    BackgroundColor(bg),
                    BorderColor::from(if selected { RED } else { GOLD_HI }),
                    BorderRadius::all(Val::Px(14.)),
                ))
                .with_children(|b| text(b, act.to_string(), if selected { 40. } else { 30. }, fg));
            }
            if sel.hundred_coin {
                row.spawn((
                    Node { width: Val::Px(58.), height: Val::Px(58.), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(3.)), margin: UiRect::left(Val::Px(18.)), ..default() },
                    BackgroundColor(GOLD),
                    BorderColor::all(GOLD_HI),
                    BorderRadius::all(Val::Px(29.)),
                ))
                .with_children(|b| text(b, "100", 22., NAVY));
            }
        });
        let name = missions.iter().find(|m| m.act == sel.selection).map_or("", |m| m.name);
        text(c, name.to_uppercase(), 30., GOLD_HI);
        if sel.best_coins > 0 {
            text(c, format!("MY SCORE   $\u{d7}{}", sel.best_coins), 20., CREAM);
        }
        let _ = progress;
        text(c, "LEFT / RIGHT  CHOOSE     A  START", 16., SKY);
    });
}

fn result(p: &mut ChildSpawnerCommands, r: &CourseResult) {
    p.spawn(card()).with_children(|c| {
        text(c, r.course.heading.clone(), 24., GOLD);
        text(c, r.course.title.to_uppercase(), 36., CREAM);
        text(c, format!("$\u{d7}{}", r.shown), 42., GOLD_HI);
        if r.show_record() {
            text(c, "HIGH SCORE!", 26., RED);
        }
        if r.options_visible {
            c.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(8.), margin: UiRect::top(Val::Px(14.)), ..default() }).with_children(|list| {
                for (i, label) in ["SAVE", "SAVE & QUIT", "CONTINUE, DON'T SAVE"].into_iter().enumerate() {
                    let on = i == r.selection;
                    list.spawn((
                        Button,
                        Clickable(MenuClick::Result(i)),
                        Node { padding: UiRect::axes(Val::Px(22.), Val::Px(6.)), border: UiRect::all(Val::Px(3.)), ..default() },
                        BackgroundColor(if on { GOLD } else { NAVY2 }),
                        BorderColor::from(if on { RED } else { NAVY2 }),
                        BorderRadius::all(Val::Px(12.)),
                    ))
                    .with_children(|b| text(b, label, 24., if on { NAVY } else { CREAM }));
                }
            });
        }
    });
}

fn bonus(p: &mut ChildSpawnerCommands, b: &BonusSave) {
    p.spawn((
        Node { position_type: PositionType::Absolute, bottom: Val::Px(40.), width: Val::Px(620.), ..card().0 },
        BackgroundColor(NAVY),
        BorderColor::all(GOLD),
        BorderRadius::all(Val::Px(18.)),
    ))
    .with_children(|c| {
        text(c, b.question(), 22., CREAM);
        if b.has_choices() {
            c.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(24.), ..default() }).with_children(|row| {
                for (i, label) in b.labels().into_iter().enumerate() {
                    let on = i == b.choice;
                    row.spawn((
                        Button,
                        Clickable(MenuClick::Bonus(i)),
                        Node { padding: UiRect::axes(Val::Px(24.), Val::Px(6.)), border: UiRect::all(Val::Px(3.)), ..default() },
                        BackgroundColor(if on { GOLD } else { NAVY2 }),
                        BorderColor::from(if on { RED } else { NAVY2 }),
                        BorderRadius::all(Val::Px(12.)),
                    ))
                    .with_children(|bt| text(bt, label, 24., if on { NAVY } else { CREAM }));
                }
            });
        } else {
            c.spawn((Button, Clickable(MenuClick::Confirm), Node { padding: UiRect::axes(Val::Px(12.), Val::Px(4.)), ..default() })).with_children(|b| text(b, "A  NEXT", 16., SKY));
        }
    });
}

