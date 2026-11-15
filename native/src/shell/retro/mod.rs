//! The "v3" front end: an N64-era file-select list on the title screen (with the real Mario model from
//! the player's imported data beside it) and a grid of mode tiles in two tabs (GAMES, LAB) for choosing
//! a mode, then the character select.
//!
//! Rendering: a private camera draws the 3D props (Mario, the gem, the select stage) into a ~320x240
//! image, a CPU framebuffer ([`fb`]) draws the 2D layer (bitmap font, slabs, tiles, cursor) at the same
//! resolution, and a faint scanline strip goes on top; the three images are stretched over the
//! window with bilinear filtering. Navigation lives in [`model`] (pure, unit tested); this module
//! turns its output into the shell's normal [`UiAction`]s so everything downstream is unchanged.
//!
//! `JUMPMAN_MENU=classic` falls back to the old bevy_ui title screen. Screenshot hooks:
//! `JUMPMAN_MENU_PAGE=games|lab` starts on a tab of the grid (`hub` is kept as an alias of `games`),
//! `JUMPMAN_MENU_TILE=n` (or the old `JUMPMAN_MENU_DOOR=n`) highlights a tile and `JUMPMAN_MENU_ROW=n`
//! a title row.
pub mod fb;
pub mod font;
pub mod grid;
pub mod hero;
pub mod icons;
pub mod model;
pub mod pages;
pub mod scene;
pub mod select;
pub mod setup;
pub mod skin;

use super::nav::{NavCmd, UiActionEvent};
use super::screens::ShellRoot;
use super::theme::UiAssets;
use super::{AppState, Character, MenuSfx, Screen, ShellUi, SwitchCharacter};
use crate::experiments::ExperimentSlot;
use crate::safe_area::SafeArea;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fb::{mix, rgb, Fb, IRect, Rgba};
use model::{sign_name, Ctx, Hit, In, Menu, Out, Page, TitleRow, Wing, TITLE_ROWS};

/// Break `s` into lines of at most `max` characters, on words.
pub(crate) fn wrap_text(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

pub const INTERNAL_H: i32 = 240;

#[derive(Resource)]
pub struct MenuRes(pub Menu);
#[derive(Resource)]
pub struct CtxRes(pub Ctx);
/// Whether the v3 front end is on screen this frame.
#[derive(Resource, Default)]
pub struct RetroActive(pub bool);
/// Set false by `JUMPMAN_MENU=classic`.
#[derive(Resource)]
pub struct RetroEnabled(pub bool);

/// Highlight a character on the select screen (`pick:<id>` in the headless shell script).
#[derive(Message, Clone, Copy, Debug)]
pub struct PickCmd(pub Character);

/// Whether the v3 front end draws the screen: the title (unless `JUMPMAN_MENU=classic`), a mode's setup page and the
/// character select.
pub fn wants(enabled: bool, state: AppState, screen: Screen) -> bool {
    state == AppState::Menu && (matches!(screen, Screen::CharSelect | Screen::ModeSetup) || (enabled && screen == Screen::Title))
}

/// Internal resolution, following the window's aspect between 4:3 and about 2.2:1.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct LowRes {
    pub w: i32,
    pub h: i32,
}

impl LowRes {
    pub fn for_window(w: f32, h: f32) -> Self {
        let aspect = if h > 0.0 { (w / h).clamp(4.0 / 3.0, 2.2) } else { 4.0 / 3.0 };
        Self { w: ((INTERNAL_H as f32 * aspect) / 4.0).round() as i32 * 4, h: INTERNAL_H }
    }
    pub fn aspect(&self) -> f32 {
        self.w as f32 / self.h as f32
    }

    /// The resolution for the window while the character select or the grid is up: a portrait window gets a
    /// portrait canvas (240 wide) so the lineup / the tiles are not stretched; everything else follows [`Self::for_window`].
    pub fn for_page(w: f32, h: f32, portrait_ok: bool) -> Self {
        if portrait_ok && w > 0.0 && h > w {
            let ih = (240.0 * h / w).round().clamp(240.0, 640.0) as i32;
            return Self { w: 240, h: ih };
        }
        Self::for_window(w, h)
    }
}

#[derive(Resource)]
pub struct Images {
    scene: Handle<Image>,
    overlay: Handle<Image>,
    scan: Handle<Image>,
}

#[derive(Component)]
pub struct RetroRoot;
#[derive(Component)]
struct OverlayNode;
#[derive(Component)]
struct SceneNode;

/// Hit rectangles of the frame just drawn (topmost last).
#[derive(Resource, Default)]
pub struct Hits(Vec<(IRect, Hit)>);

/// Static facts about the registry that do not change while running.
#[derive(Resource)]
pub struct Ported(Vec<(crate::shell::ExperimentId, bool)>);

#[derive(Resource, Default)]
pub struct Anim {
    sel: Option<(Page, Wing, usize, usize)>,
    since: f32,
    cursor_y: f32,
    touch_start: Option<Vec2>,
    last_mouse: Vec2,
}

fn new_image(w: i32, h: i32, filter: ImageSampler, usage: TextureUsages) -> Image {
    let mut img = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; (w * h * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = filter;
    img.texture_descriptor.usage = usage;
    img
}

fn start_page() -> (Page, Wing, Option<usize>, Option<usize>) {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (page, wing) = match env("JUMPMAN_MENU_PAGE").as_deref() {
        Some("grid") | Some("hub") | Some("games") => (Page::Grid, Wing::Games),
        Some("lab") => (Page::Grid, Wing::Lab),
        _ => (Page::Title, Wing::Games),
    };
    let tile = env("JUMPMAN_MENU_TILE").or_else(|| env("JUMPMAN_MENU_DOOR")).and_then(|v| v.parse().ok());
    (page, wing, tile, env("JUMPMAN_MENU_ROW").and_then(|v| v.parse().ok()))
}

#[allow(clippy::too_many_arguments)]
pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    ui: Option<ResMut<UiAssets>>,
    character: Option<Res<crate::characters::Character>>,
) {
    let low = LowRes { w: 320, h: INTERNAL_H };
    let scene_img = images.add(new_image(low.w, low.h, ImageSampler::linear(), TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT));
    let overlay = images.add(new_image(low.w, low.h, ImageSampler::linear(), TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST));
    // Scanlines: every other half-line darkened, sampled without filtering so they stay crisp.
    let mut scan = new_image(1, low.h * 2, ImageSampler::nearest(), TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
    if let Some(d) = scan.data.as_mut() {
        for (i, p) in d.chunks_exact_mut(4).enumerate() {
            p.copy_from_slice(&[0, 0, 8, if i % 2 == 1 { 46 } else { 0 }]);
        }
    }
    let scan = images.add(scan);
    commands.insert_resource(Images { scene: scene_img.clone(), overlay: overlay.clone(), scan: scan.clone() });

    let hero = character.map_or(Character::Mario, |c| Character::from_theirs(*c));
    let ported: Vec<_> = crate::shell::ExperimentId::ALL.iter().map(|id| (*id, id.available())).collect();
    let ctx = Ctx::from_registry(hero, [true; Character::COUNT], |id| ported.iter().any(|(p, ok)| *p == id && *ok));
    scene::spawn_scene(&mut commands, &mut meshes, &mut mats, scene_img.clone());

    // Per-mode vignettes also fill the classic cards' empty thumbnail slots.
    if let Some(mut ui) = ui {
        for id in crate::experiments::EXPERIMENT_IDS {
            let Some(mine) = crate::shell::ExperimentId::from_id(id.key()) else { continue };
            if ui.modes.contains_key(id.key()) {
                continue;
            }
            let ic = icons::icon(mine);
            let mut img = new_image(ic.w, ic.h, ImageSampler::nearest(), TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
            img.data = Some(ic.data);
            ui.modes.insert(id.key(), images.add(img));
        }
    }

    let mut menu = Menu::default();
    let (page, wing, door, row) = start_page();
    menu.page = page;
    menu.wing = wing;
    if let Some(d) = door {
        menu.tile[wing.idx()] = d.min(ctx.tiles[wing.idx()].len().saturating_sub(1));
    }
    if let Some(r) = row {
        menu.row = r.min(TITLE_ROWS.len() - 1);
    }
    commands.insert_resource(MenuRes(menu));
    commands.insert_resource(CtxRes(ctx));
    commands.insert_resource(Ported(ported));
    commands.insert_resource(low);
    commands.insert_resource(scene::CamState::default());
    commands.insert_resource(hero::TitleHero::default());
    commands.insert_resource(Hits::default());
    commands.insert_resource(Anim::default());
    commands.insert_resource(RetroActive(false));
    commands.insert_resource(RetroEnabled(std::env::var("JUMPMAN_MENU").map_or(true, |v| v != "classic")));

    let full = Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands
        .spawn((RetroRoot, ShellRoot, GlobalZIndex(50), full.clone(), BackgroundColor(Color::BLACK), Visibility::Hidden))
        .with_children(|p| {
            p.spawn((SceneNode, ImageNode::new(scene_img), full.clone()));
            p.spawn((OverlayNode, ImageNode::new(overlay), full.clone()));
            p.spawn((ImageNode::new(scan), full.clone()));
        });
}

/// Which wing of the registry an experiment id belongs to is decided by the registry itself.
#[allow(clippy::too_many_arguments)]
pub fn sync_state(
    state: Res<AppState>,
    ui: Res<ShellUi>,
    enabled: Res<RetroEnabled>,
    mut active: ResMut<RetroActive>,
    mut menu: ResMut<MenuRes>,
    mut ctx: ResMut<CtxRes>,
    ported: Res<Ported>,
    character: Option<Res<crate::characters::Character>>,
    content: Option<Res<crate::content::AvailableContent>>,
    mut root: Query<&mut Visibility, With<RetroRoot>>,
    mut was: Local<bool>,
    _slot: Option<Res<ExperimentSlot>>,
) {
    let now = wants(enabled.0, *state, ui.screen);
    active.0 = now;
    if let Ok(mut v) = root.single_mut() {
        *v = if now { Visibility::Inherited } else { Visibility::Hidden };
    }
    if now && !*was && *state == AppState::Menu && ui.stack.is_empty() {
        // Coming back from a game: start on the file-select list.
        if menu.0.pending.is_some() {
            menu.0.pending = None;
        }
    }
    *was = now;
    let content = content.as_deref().cloned().unwrap_or_default();
    if let Some(c) = character {
        ctx.0.hero = Character::from_theirs(*c);
    }
    for (i, ch) in Character::ALL.iter().enumerate() {
        ctx.0.hero_ok[i] = content.character_ok(*ch);
    }
    for wing in 0..2 {
        for d in &mut ctx.0.tiles[wing] {
            let ported = ported.0.iter().any(|(p, ok)| *p == d.id && *ok);
            let missing = content.experiment_missing(d.id);
            d.enabled = ported && missing.is_none();
            d.lock = if !ported { Some(model::NOT_PORTED) } else { missing.map(model::tile_lock) };
        }
    }
    // The character select: what it is for and who the mode takes; the mode's start options for its setup page.
    ctx.0.allowed = select_allowed(ui.pick);
    ctx.0.target = select_target(ui.pick, &ui.course);
    ctx.0.setup = match ui.pick {
        Some(super::Pick::Mode(e)) if ui.screen == Screen::ModeSetup => e.theirs().map_or(Vec::new(), |x| setup_rows(x, &content)),
        _ => Vec::new(),
    };
    match ui.screen {
        Screen::CharSelect if menu.0.page != Page::Select => menu.0.open_select(&ctx.0),
        Screen::ModeSetup if menu.0.page != Page::Setup => menu.0.open_setup(&ctx.0),
        Screen::CharSelect | Screen::ModeSetup => {}
        _ if matches!(menu.0.page, Page::Select | Page::Setup) => menu.0.close_select(),
        _ => {}
    }
}

/// The setup page's rows for a mode (its `start_setup` options; arenas on courses of games not imported are locked).
pub fn setup_rows(id: crate::experiments::ExperimentId, content: &crate::content::AvailableContent) -> Vec<model::SetupRow> {
    crate::experiments::start_setup::rows(id, &|course| content.course_missing(course).is_none())
        .into_iter()
        .map(|r| model::SetupRow { label: r.label.to_string(), values: r.values, value: r.value, locked: r.locked, enabled: r.enabled })
        .collect()
}

/// Which characters the picked level / mode takes (a level takes everyone; Mario-only modes only Mario).
pub fn select_allowed(pick: Option<super::Pick>) -> [bool; Character::COUNT] {
    use crate::experiments::characters_support as support;
    match pick {
        Some(super::Pick::Mode(e)) => match e.theirs() {
            Some(x) => Character::ALL.map(|c| support::is_supported(x, c.theirs())),
            None => [true; Character::COUNT],
        },
        _ => [true; Character::COUNT],
    }
}

/// The plate of the select screen: the course or the mode being started.
pub fn select_target(pick: Option<super::Pick>, course: &str) -> model::Target {
    match pick {
        Some(super::Pick::Mode(e)) => model::Target { kind: "MODE", name: sign_name(e), mario_only: select_allowed(pick).iter().filter(|b| **b).count() == 1 },
        _ => model::Target { kind: "COURSE", name: crate::game::courses::info(course).map_or(String::new(), |c| c.name.to_ascii_uppercase()), mario_only: false },
    }
}

pub fn resize(
    windows: Query<&Window, With<PrimaryWindow>>,
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    mut low: ResMut<LowRes>,
    images_res: Res<Images>,
    mut images: ResMut<Assets<Image>>,
) {
    if !active.0 {
        return;
    }
    let Ok(w) = windows.single() else { return };
    let next = LowRes::for_page(w.width(), w.height(), matches!(menu.0.page, Page::Select | Page::Grid | Page::Setup));
    if next == *low {
        return;
    }
    *low = next;
    let size = Extent3d { width: next.w as u32, height: next.h as u32, depth_or_array_layers: 1 };
    for h in [&images_res.scene, &images_res.overlay] {
        if let Some(mut img) = images.get_mut(h) {
            img.resize(size);
            if let Some(d) = img.data.as_mut() {
                d.fill(0);
            }
        }
    }
    // The scanline strip follows the canvas height so the lines stay one framebuffer row each.
    if let Some(mut img) = images.get_mut(&images_res.scan) {
        img.resize(Extent3d { width: 1, height: next.h as u32 * 2, depth_or_array_layers: 1 });
        if let Some(d) = img.data.as_mut() {
            for (i, p) in d.chunks_exact_mut(4).enumerate() {
                p.copy_from_slice(&[0, 0, 8, if i % 2 == 1 { 46 } else { 0 }]);
            }
        }
    }
}

fn to_fb(window: &Window, low: &LowRes, p: Vec2) -> Vec2 {
    Vec2::new(p.x / window.width().max(1.0) * low.w as f32, p.y / window.height().max(1.0) * low.h as f32)
}

fn hit_at(hits: &Hits, p: Vec2) -> Option<Hit> {
    hits.0.iter().rev().find(|(r, _)| r.contains(p.x, p.y)).map(|(_, h)| *h)
}

#[allow(clippy::too_many_arguments)]
pub fn input(
    active: Res<RetroActive>,
    mut menu: ResMut<MenuRes>,
    mut ctx: ResMut<CtxRes>,
    hits: Res<Hits>,
    low: Res<LowRes>,
    area: Res<SafeArea>,
    mut anim: ResMut<Anim>,
    mut nav: MessageReader<NavCmd>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut actions: MessageWriter<UiActionEvent>,
    mut chars: MessageWriter<SwitchCharacter>,
    mut sfx: Option<ResMut<MenuSfx>>,
    time: Res<Time<Real>>,
    mut picks: MessageReader<PickCmd>,
) {
    if !active.0 {
        nav.clear();
        picks.clear();
        return;
    }
    let Ok(window) = windows.single() else { return };
    // Up / down on the grid step by the columns the layout has right now.
    let sa = insets(window, &low, &area);
    for wing in 0..2 {
        ctx.0.cols[wing] = grid::layout(low.w, low.h, sa, ctx.0.tiles[wing].len()).cols;
    }
    // The character roster's rows follow its layout too.
    ctx.0.roster_cols = select::layout(low.w, low.h, sa, &ctx.0.shown_slots()).cols;
    let mut events: Vec<In> = Vec::new();
    // The select and the setup page are screens of the shell's stack: Back pops it (`nav::apply_nav`), Start confirms.
    let selecting = matches!(menu.0.page, Page::Select | Page::Setup);
    for cmd in nav.read() {
        match *cmd {
            NavCmd::Move(d) => events.push(In::Move(d)),
            // Pad Start confirms on the select screen, like a fighting game.
            NavCmd::Activate | NavCmd::Start if selecting || matches!(cmd, NavCmd::Activate) => events.push(In::Activate),
            // Esc / pad B already pop the screen stack through `nav::apply_nav`; sending Back here too would pop twice.
            NavCmd::Back if selecting => {}
            NavCmd::Back => events.push(In::Back),
            NavCmd::Tab(d) => events.push(In::Wing(d)),
            _ => {}
        }
    }
    for PickCmd(c) in picks.read() {
        if let Some(i) = Character::ALL.iter().position(|x| x == c) {
            events.push(In::Hover(Hit::Slot(i)));
        }
    }
    if let Some(cursor) = window.cursor_position() {
        let p = to_fb(window, &low, cursor);
        if p.distance(anim.last_mouse) > 0.5 {
            anim.last_mouse = p;
            if let Some(h) = hit_at(&hits, p) {
                events.push(In::Hover(h));
            }
        }
        if mouse.just_pressed(MouseButton::Left) {
            if let Some(h) = hit_at(&hits, p) {
                events.push(In::Click(h));
            }
        }
    }
    for t in touches.iter_just_pressed() {
        anim.touch_start = Some(t.position());
    }
    for t in touches.iter_just_released() {
        let start = anim.touch_start.take().unwrap_or(t.position());
        let moved = t.position() - start;
        let swipe = moved.x.abs() > 44.0 && moved.x.abs() > moved.y.abs() * 1.5;
        if swipe && menu.0.page == Page::Select {
            events.push(In::Move(IVec2::new(if moved.x < 0.0 { 1 } else { -1 }, 0)));
        } else if swipe && menu.0.page == Page::Grid {
            // A sideways swipe on the grid flips between GAMES and LAB.
            events.push(In::Wing(if moved.x < 0.0 { 1 } else { -1 }));
        } else if moved.length() < 18.0 {
            if let Some(h) = hit_at(&hits, to_fb(window, &low, t.position())) {
                events.push(In::Click(h));
            }
        }
    }
    let _ = time;
    for ev in events {
        for out in menu.0.input(ev, &ctx.0) {
            match out {
                Out::Action(a) => {
                    if let UiAction::Character(c) = a {
                        let _ = &mut chars;
                        actions.write(UiActionEvent(UiAction::Character(c)));
                    } else {
                        actions.write(UiActionEvent(a));
                    }
                }
                Out::Sfx(s) => MenuSfx::push(&mut sfx, s),
            }
        }
    }
}

use super::UiAction;

pub fn tick(active: Res<RetroActive>, mut menu: ResMut<MenuRes>, time: Res<Time<Real>>, mut actions: MessageWriter<UiActionEvent>) {
    if !active.0 {
        return;
    }
    if let Some(a) = menu.0.tick(time.delta_secs().min(0.1)) {
        actions.write(UiActionEvent(a));
    }
}

// ------------------------------------------------------------------------------------------
// Drawing
// ------------------------------------------------------------------------------------------

const NAVY: Rgba = rgb(0x0b1240);
const GOLD: Rgba = rgb(0xffd23a);
const WHITE: Rgba = rgb(0xf6f6ee);

#[derive(Clone, Copy, Debug, Default)]
pub struct Insets {
    pub l: i32,
    pub r: i32,
    pub t: i32,
    pub b: i32,
}

/// Rectangles of the seven title rows.
pub fn title_rows(w: i32, h: i32, sa: Insets) -> [IRect; 6] {
    let rw = 132.min(w / 2 - 12);
    let x = w - rw - 14 - sa.r;
    let step = 22;
    let rows_h = step * 6 - 3;
    let y0 = (h - sa.b - 22 - rows_h).max(sa.t + 60);
    std::array::from_fn(|i| IRect::new(x, y0 + i as i32 * step, rw, 19))
}

fn row_label(row: TitleRow, ctx: &Ctx) -> String {
    match row {
        TitleRow::Games => "GAMES".into(),
        TitleRow::Levels => "LEVELS".into(),
        TitleRow::Lab => "LAB".into(),
        TitleRow::File => "SAVE FILE".into(),
        TitleRow::Options => "OPTIONS".into(),
        TitleRow::Controls => "CONTROLS".into(),
    }
}

fn row_hint(row: TitleRow, ctx: &Ctx) -> String {
    match row {
        TitleRow::Games => format!("{} GAMES TO CHOOSE FROM", ctx.tiles[0].len()),
        TitleRow::Levels => "PICK A COURSE AND A MISSION, THEN YOUR HERO".into(),
        TitleRow::Lab => format!("{} EXPERIMENTS AND TOYS IN THE LAB", ctx.tiles[1].len()),
        TitleRow::File => "CHOOSE A SAVE FILE".into(),
        TitleRow::Options => if crate::byor::importer_available() { "DISPLAY, SOUND, CAMERA AND ROM IMPORT".into() } else { "DISPLAY, SOUND AND CAMERA".into() },
        TitleRow::Controls => "SEE AND REBIND THE CONTROLS".into(),
    }
}

fn input_hint(pad: bool) -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        "TAP TO CHOOSE"
    } else if pad {
        "D-PAD MOVE   A SELECT   B BACK   L/R TAB"
    } else {
        "ARROWS MOVE   ENTER SELECT   ESC BACK   TAB SWITCH"
    }
}

/// Safe-area insets in framebuffer px.
fn insets(window: &Window, low: &LowRes, area: &SafeArea) -> Insets {
    let k = low.w as f32 / window.width().max(1.0);
    Insets { l: (area.left * k).round() as i32, r: (area.right * k).round() as i32, t: (area.top * k).round() as i32, b: (area.bottom * k).round() as i32 }
}

fn draw_dialog(f: &mut Fb, r: IRect) {
    f.slab(r, rgb(0x2a3a8a), rgb(0x141c58), rgb(0x6f86e0), rgb(0x0a0f35), rgb(0x040720));
}

fn draw_logo(f: &mut Fb, cx: i32, y: i32, scale: i32, t: f32, wobble: bool) {
    let text = "JUMPMAN";
    let step = 6 * scale + scale;
    let total = text.len() as i32 * step - scale;
    let x0 = cx - total / 2;
    for (i, ch) in text.chars().enumerate() {
        let bob = if wobble { ((t * 2.4 - i as f32 * 0.55).sin() * 1.6).round() as i32 } else { 0 };
        let x = x0 + i as i32 * step;
        let s = ch.to_string();
        // Extruded side, then outlined gradient face.
        for d in (1..=scale + 1).rev() {
            f.glyphs(x + d, y + bob + d, &s, scale, &|_| rgb(0x4a1d05));
        }
        f.text_outlined(x, y + bob, &s, scale, rgb(0x5a2608), &|row| mix(rgb(0xffe56a), rgb(0xf08a18), row as f32 / 6.0));
    }
}

fn draw_chevron(f: &mut Fb, x: i32, y: i32, t: f32) {
    let bob = ((t * 7.0).sin().abs() * 3.0) as i32;
    let x = x - bob;
    let pts = [(x as f32, (y - 6) as f32), ((x + 9) as f32, y as f32), (x as f32, (y + 6) as f32), ((x + 3) as f32, y as f32)];
    let shadow: Vec<_> = pts.iter().map(|p| (p.0 + 1.0, p.1 + 1.0)).collect();
    f.poly(&shadow, rgb(0x050a22));
    f.poly(&pts, GOLD);
}

#[allow(clippy::too_many_arguments)]
fn draw_title(f: &mut Fb, menu: &Menu, ctx: &Ctx, hits: &mut Hits, anim: &mut Anim, t: f32, dt: f32, sa: Insets, pad: bool, quit: bool) {
    let (w, h) = (f.w, f.h);
    draw_logo(f, w / 2, 12 + sa.t, 4, t, true);
    f.text_centered(w / 2, 12 + sa.t + 34, "A 3D COLLECTION", 1, rgb(0xaec6ff));
    let rows = title_rows(w, h, sa);
    for (i, r) in rows.iter().enumerate() {
        let sel = i == menu.row;
        let (top, bot, light, dark) = if sel {
            (rgb(0x4a7dff), rgb(0x1f48d0), rgb(0xb8ccff), rgb(0x0f2a8a))
        } else {
            (rgb(0x33449a), rgb(0x1a2468), rgb(0x5b70c8), rgb(0x0a1040))
        };
        f.slab(*r, top, bot, light, dark, rgb(0x040720));
        let label = row_label(TITLE_ROWS[i], ctx);
        let scale = if label.len() * 12 > (r.w - 8) as usize { 1 } else { 2 };
        let ty = r.y + (r.h - 7 * scale) / 2;
        let col = if sel { WHITE } else { rgb(0xdfe6ff) };
        f.text_centered(r.x + r.w / 2, ty, &label, scale, col);
        hits.0.push((r.inflate(1), Hit::Row(i)));
    }
    // Smooth cursor.
    let target = rows[menu.row].y as f32 + 9.5;
    if anim.cursor_y == 0.0 {
        anim.cursor_y = target;
    }
    anim.cursor_y += (target - anim.cursor_y) * (1.0 - (-18.0 * dt).exp());
    draw_chevron(f, rows[0].x - 13, anim.cursor_y.round() as i32, t);
    // Hint bar.
    let bar = IRect::new(8 + sa.l, h - 17 - sa.b, w - 16 - sa.l - sa.r, 13);
    draw_dialog(f, bar);
    let hint = format!("{}   -   {}", row_hint(TITLE_ROWS[menu.row], ctx), input_hint(pad));
    let hint = if Fb::text_width(&hint, 1) > bar.w - 8 { row_hint(TITLE_ROWS[menu.row], ctx) } else { hint };
    f.text(bar.x + 5, bar.y + 3, &hint, 1, WHITE);
    if quit {
        let q = IRect::new(8 + sa.l, 8 + sa.t, 34, 13);
        f.slab(q, rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603));
        f.text_centered(q.x + q.w / 2, q.y + 3, "QUIT", 1, WHITE);
        hits.0.push((q, Hit::Quit));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    ctx: Res<CtxRes>,
    low: Res<LowRes>,
    mut hits: ResMut<Hits>,
    mut anim: ResMut<Anim>,
    time: Res<Time<Real>>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pads: Query<&Gamepad>,
    images_res: Res<Images>,
    mut images: ResMut<Assets<Image>>,
    _stage: Res<select::Stage>,
) {
    if !active.0 {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let sa = insets(window, &low, &area);
    let m = &menu.0;
    let key = (m.page, m.wing, m.row, m.cur_tile());
    if anim.sel != Some(key) {
        anim.sel = Some(key);
        anim.since = t;
    }
    let mut f = Fb::new(low.w, low.h);
    hits.0.clear();
    let pad = !pads.is_empty();
    match m.page {
        Page::Title => draw_title(&mut f, m, &ctx.0, &mut hits, &mut anim, t, dt, sa, pad, !cfg!(target_os = "ios")),
        Page::Setup => setup::draw_setup(&mut f, m, &ctx.0, &mut hits, t, sa, setup::hint(pad)),
        Page::Grid => grid::draw_grid(&mut f, m, &ctx.0, &mut hits, &anim, t, sa, input_hint(pad), |id| select_allowed(Some(super::Pick::Mode(id))).iter().filter(|b| **b).count() == 1),
        Page::Select => {
            select::draw_select(&mut f, m, &ctx.0, &mut hits, &mut anim, t, sa, pad);
        }
    }
    if let Some(mut img) = images.get_mut(&images_res.overlay) {
        if img.width() as i32 == low.w && img.height() as i32 == low.h {
            img.data = Some(f.data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_res_follows_window_aspect_within_limits() {
        assert_eq!(LowRes::for_window(1040.0, 680.0).h, 240);
        assert_eq!(LowRes::for_window(800.0, 600.0).w, 320);
        let wide = LowRes::for_window(1920.0, 1080.0);
        assert!(wide.w > 400 && wide.w % 4 == 0);
        assert!(LowRes::for_window(3000.0, 600.0).aspect() <= 2.21);
        assert_eq!(LowRes::for_window(0.0, 0.0).w, 320);
        // The grid and the select get a portrait canvas on a portrait window; the title keeps the landscape one.
        assert_eq!(LowRes::for_page(820.0, 1180.0, true).w, 240);
        assert!(LowRes::for_page(820.0, 1180.0, true).h > 240);
        assert_eq!(LowRes::for_page(820.0, 1180.0, false), LowRes::for_window(820.0, 1180.0));
    }

    #[test]
    fn retro_screen_only_replaces_the_title() {
        assert!(wants(true, AppState::Menu, Screen::Title));
        assert!(!wants(true, AppState::Menu, Screen::Courses));
        assert!(!wants(true, AppState::Paused, Screen::Title));
        assert!(!wants(false, AppState::Menu, Screen::Title));
        // The character select is always drawn here, also with the classic title; never while playing or paused.
        assert!(wants(false, AppState::Menu, Screen::CharSelect) && wants(true, AppState::Menu, Screen::CharSelect));
        assert!(wants(false, AppState::Menu, Screen::ModeSetup) && !wants(true, AppState::Playing, Screen::ModeSetup));
        assert!(!wants(true, AppState::Playing, Screen::CharSelect) && !wants(true, AppState::Transition, Screen::CharSelect));
    }

    #[test]
    fn title_rows_fit_and_do_not_overlap() {
        for (w, h) in [(320, 240), (427, 240), (528, 240)] {
            let rows = title_rows(w, h, Insets::default());
            for (i, r) in rows.iter().enumerate() {
                assert!(r.x >= w / 2 && r.x + r.w <= w && r.y >= 50 && r.y + r.h <= h - 10, "{w}x{h} row {i}: {r:?}");
                if i > 0 {
                    assert!(r.y >= rows[i - 1].y + rows[i - 1].h);
                }
            }
        }
        // A notch inset pushes the list inward.
        let a = title_rows(427, 240, Insets::default());
        let b = title_rows(427, 240, Insets { r: 30, ..Default::default() });
        assert!(b[0].x < a[0].x);
    }

    #[test]
    fn title_rows_are_comfortable_touch_targets() {
        // At 390 pt tall (a phone in landscape) one internal px is 1.6 pt.
        let rows = title_rows(520, 240, Insets::default());
        assert!(rows[0].h as f32 * 390.0 / 240.0 >= 30.0);
    }

    #[test]
    fn title_draw_registers_rows_and_paints_pixels() {
        let ctx = Ctx::from_registry(Character::Mario, [true; Character::COUNT], |_| true);
        let menu = Menu::default();
        let mut f = Fb::new(320, 240);
        let mut hits = Hits::default();
        let mut anim = Anim::default();
        draw_title(&mut f, &menu, &ctx, &mut hits, &mut anim, 0.5, 0.016, Insets::default(), false, true);
        assert_eq!(hits.0.iter().filter(|(_, h)| matches!(h, Hit::Row(_))).count(), 6);
        assert!(f.data.chunks(4).filter(|p| p[3] > 0).count() > 3000);
        assert_eq!(hit_at(&hits, Vec2::new(250.0, hits.0[0].0.y as f32 + 5.0)), Some(Hit::Row(0)));
    }

    #[test]
    fn title_hints_count_the_tiles() {
        let ctx = Ctx::from_registry(Character::Mario, [true; Character::COUNT], |_| true);
        assert!(row_hint(TitleRow::Games, &ctx).starts_with(&ctx.tiles[0].len().to_string()));
        assert!(row_hint(TitleRow::Lab, &ctx).starts_with(&ctx.tiles[1].len().to_string()));
    }
}
