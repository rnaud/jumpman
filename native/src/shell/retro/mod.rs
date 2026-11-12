//! The "v3" front end: an N64-era file-select list on the title screen and a hub room (a ring of
//! doors: games in one wing, experiments in another) for choosing a mode.
//!
//! Rendering: a private camera draws the 3D props (hero, gem, rooms) into a ~320x240 image, a CPU
//! framebuffer ([`fb`]) draws the 2D layer (bitmap font, slabs, signs, cursor) at the same
//! resolution, and a faint scanline strip goes on top; the three images are stretched over the
//! window with bilinear filtering. Navigation lives in [`model`] (pure, unit tested); this module
//! turns its output into the shell's normal [`UiAction`]s so everything downstream is unchanged.
//!
//! `JUMPMAN_MENU=classic` falls back to the old bevy_ui title screen. Screenshot hooks:
//! `JUMPMAN_MENU_PAGE=hub|lab` starts in a hub wing, `JUMPMAN_MENU_DOOR=n` picks the door and
//! `JUMPMAN_MENU_ROW=n` the title row.
pub mod fb;
pub mod font;
pub mod icons;
pub mod model;
pub mod pages;
pub mod scene;
pub mod select;
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
use fb::{clean, mix, rgb, Fb, IRect, Rgba};
use model::{sign_lines, sign_name, Ctx, Hit, In, Menu, Out, Page, TitleRow, Wing, TITLE_ROWS};

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

/// Whether the v3 front end draws the screen: the title (unless `JUMPMAN_MENU=classic`) and the character select.
pub fn wants(enabled: bool, state: AppState, screen: Screen) -> bool {
    state == AppState::Menu && (screen == Screen::CharSelect || (enabled && screen == Screen::Title))
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

    /// The resolution for the window while the character select is up: a portrait window gets a portrait
    /// canvas (240 wide) so the lineup is not stretched; everything else follows [`Self::for_window`].
    pub fn for_page(w: f32, h: f32, select: bool) -> Self {
        if select && w > 0.0 && h > w {
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
    last_wing: Option<Wing>,
    wing_flash: f32,
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
        Some("hub") | Some("games") => (Page::Hub, Wing::Games),
        Some("lab") => (Page::Hub, Wing::Lab),
        _ => (Page::Title, Wing::Games),
    };
    (page, wing, env("JUMPMAN_MENU_DOOR").and_then(|v| v.parse().ok()), env("JUMPMAN_MENU_ROW").and_then(|v| v.parse().ok()))
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
    let ctx = Ctx::from_registry(hero, [true; 4], |id| ported.iter().any(|(p, ok)| *p == id && *ok));
    let ids: [Vec<crate::shell::ExperimentId>; 2] = [ctx.doors[0].iter().map(|d| d.id).collect(), ctx.doors[1].iter().map(|d| d.id).collect()];
    scene::spawn_scene(&mut commands, &mut meshes, &mut mats, scene_img.clone(), &ids);

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
        menu.door[wing.idx()] = d.min(ctx.doors[wing.idx()].len().saturating_sub(1));
    }
    if let Some(r) = row {
        menu.row = r.min(TITLE_ROWS.len() - 1);
    }
    commands.insert_resource(MenuRes(menu));
    commands.insert_resource(CtxRes(ctx));
    commands.insert_resource(Ported(ported));
    commands.insert_resource(low);
    commands.insert_resource(scene::CamState::default());
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
        ctx.0.hero_ok[i] = content.character_missing(*ch).is_none();
    }
    for wing in 0..2 {
        for d in &mut ctx.0.doors[wing] {
            let ok = ported.0.iter().any(|(p, ok)| *p == d.id && *ok);
            d.enabled = ok && content.experiment_missing(d.id).is_none();
        }
    }
    // The character select: what it is for and who the mode takes.
    ctx.0.allowed = select_allowed(ui.pick);
    ctx.0.target = select_target(ui.pick, &ui.course);
    if ui.screen == Screen::CharSelect && menu.0.page != Page::Select {
        menu.0.open_select(&ctx.0);
    } else if ui.screen != Screen::CharSelect && menu.0.page == Page::Select {
        menu.0.close_select();
    }
}

/// Which characters the picked level / mode takes (a level takes everyone; Mario-only modes only Mario).
pub fn select_allowed(pick: Option<super::Pick>) -> [bool; 4] {
    use crate::experiments::characters_support as support;
    match pick {
        Some(super::Pick::Mode(e)) => match e.theirs() {
            Some(x) => Character::ALL.map(|c| support::is_supported(x, c.theirs())),
            None => [true; 4],
        },
        _ => [true; 4],
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
    let next = LowRes::for_page(w.width(), w.height(), menu.0.page == Page::Select);
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
    ctx: Res<CtxRes>,
    hits: Res<Hits>,
    low: Res<LowRes>,
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
    let mut events: Vec<In> = Vec::new();
    let selecting = menu.0.page == Page::Select;
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
        if moved.x.abs() > 44.0 && moved.x.abs() > moved.y.abs() * 1.5 && matches!(menu.0.page, Page::Hub | Page::Select) {
            events.push(In::Move(IVec2::new(if moved.x < 0.0 { 1 } else { -1 }, 0)));
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
        TitleRow::Games => format!("ENTER THE HUB ROOM: {} GAMES BEHIND THE DOORS", ctx.doors[0].len()),
        TitleRow::Levels => "PICK A COURSE AND A MISSION, THEN YOUR HERO".into(),
        TitleRow::Lab => format!("{} EXPERIMENTS AND TOYS IN THE LAB WING", ctx.doors[1].len()),
        TitleRow::File => "CHOOSE A SAVE FILE".into(),
        TitleRow::Options => if crate::byor::importer_available() { "DISPLAY, SOUND, CAMERA AND ROM IMPORT".into() } else { "DISPLAY, SOUND AND CAMERA".into() },
        TitleRow::Controls => "SEE AND REBIND THE CONTROLS".into(),
    }
}

fn input_hint(pad: bool) -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        "TAP TO CHOOSE"
    } else if pad {
        "D-PAD MOVE   A SELECT   B BACK   L/R WING"
    } else {
        "ARROWS MOVE   ENTER SELECT   ESC BACK   TAB WING"
    }
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

fn wing_tab(f: &mut Fb, r: IRect, label: &str, sel: bool) {
    if sel {
        f.slab(r, rgb(0xffd23a), rgb(0xe08a18), rgb(0xfff1a8), rgb(0x8a4a08), rgb(0x3a1a04));
        f.text_centered(r.x + r.w / 2, r.y + 4, label, 1, rgb(0x3a1a04));
    } else {
        f.slab(r, rgb(0x33449a), rgb(0x1a2468), rgb(0x5b70c8), rgb(0x0a1040), rgb(0x040720));
        f.text_centered(r.x + r.w / 2, r.y + 4, label, 1, rgb(0xdfe6ff));
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hub(f: &mut Fb, menu: &Menu, ctx: &Ctx, cam: &scene::CamState, hits: &mut Hits, anim: &mut Anim, t: f32, sa: Insets, pad: bool) {
    let (w, h) = (f.w, f.h);
    let doors = menu.doors(ctx);
    let n = doors.len();
    let sel = menu.cur_door();
    let intro = scene::CamState { ..*cam }.intro;
    let fade_in = intro >= 1.0;

    // Door signs (back to front by depth), only when the camera is not diving into a door.
    if menu.pending.is_none() || menu.enter_progress() < 0.5 {
        let mut order: Vec<(f32, usize)> = (0..n).filter_map(|i| cam.project(scene::sign_anchor(menu.wing, i, n), w as f32, h as f32).map(|p| (p.2, i))).collect();
        order.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, i) in order {
            let Some((sx, sy, _)) = cam.project(scene::sign_anchor(menu.wing, i, n), w as f32, h as f32) else { continue };
            let base = scene::dir_of(scene::door_angle(i, n)) * (scene::ring_radius(menu.wing) - 0.3);
            let (left, right) = {
                let a = scene::door_angle(i, n);
                let rv = Vec3::new(a.cos(), 0.0, -a.sin()) * scene::DOOR_HALF_W;
                (cam.project(base - rv, w as f32, h as f32), cam.project(base + rv, w as f32, h as f32))
            };
            let floor = cam.project(base, w as f32, h as f32).map_or(sy + 40.0, |p| p.1);
            let name = clean(&sign_name(doors[i].id));
            let lines = sign_lines(&name, 8);
            let tw = lines.iter().map(|l| Fb::text_width(l, 1)).max().unwrap_or(30);
            let (bw, bh) = (tw + 8, lines.len() as i32 * 9 + 5);
            let r = IRect::new(sx.round() as i32 - bw / 2, sy.round() as i32 - bh / 2, bw, bh);
            // Keep signs clear of the top bar.
            if r.x > w || r.x + r.w < 0 || r.y < 24 + sa.t {
                continue;
            }
            let selected = i == sel;
            let accent = icons::accent(doors[i].id);
            let (face_a, face_b) = if selected { (mix(accent, WHITE, 0.25), mix(accent, rgb(0), 0.1)) } else { (mix(accent, rgb(0), 0.1), mix(accent, rgb(0), 0.45)) };
            f.rect(r.inflate(2), if selected { mix(GOLD, WHITE, ((t * 6.0).sin() * 0.5 + 0.5) * 0.5) } else { rgb(0x2a2a30) });
            f.vgrad(r, face_a, face_b);
            f.frame(r, rgb(0x0a0a14));
            for (k, l) in lines.iter().enumerate() {
                f.text_centered(r.x + r.w / 2, r.y + 3 + k as i32 * 9, l, 1, if doors[i].enabled { WHITE } else { rgb(0xb0b0b8) });
            }
            hits.0.push((r.inflate(2), Hit::Door(i)));
            let (dl, dr) = (left.map_or(r.x, |p| p.0 as i32), right.map_or(r.x + r.w, |p| p.0 as i32));
            hits.0.push((IRect::new(dl.min(dr), r.y + r.h + 2, (dr - dl).abs().max(8), (floor as i32 - r.y - r.h - 2).max(8)), Hit::Door(i)));
        }
    }

    // Top bar: BACK, wing tabs, position.
    let top = 6 + sa.t;
    let back = IRect::new(8 + sa.l, top, 38, 15);
    f.slab(back, rgb(0xd04a3a), rgb(0x8a1a12), rgb(0xff9a8a), rgb(0x500a06), rgb(0x2a0603));
    f.text_centered(back.x + back.w / 2, back.y + 4, "BACK", 1, WHITE);
    hits.0.push((back.inflate(2), Hit::Back));
    let (tg, tl) = (IRect::new(w / 2 - 50, top, 48, 15), IRect::new(w / 2 + 2, top, 48, 15));
    wing_tab(f, tg, "GAMES", menu.wing == Wing::Games);
    wing_tab(f, tl, "LAB", menu.wing == Wing::Lab);
    hits.0.push((tg.inflate(2), Hit::Wing(Wing::Games)));
    hits.0.push((tl.inflate(2), Hit::Wing(Wing::Lab)));
    let count = format!("{}/{}", sel + 1, n);
    f.text(w - 10 - sa.r - Fb::text_width(&count, 1), top + 4, &count, 1, WHITE);

    // Side arrows (also the touch controls for stepping around the ring).
    for (left_side, hit) in [(true, Hit::Prev), (false, Hit::Next)] {
        let x = if left_side { 4 + sa.l } else { w - 24 - sa.r };
        let r = IRect::new(x, h / 2 - 34, 20, 40);
        let (cx, cy) = ((x + 10) as f32, (h / 2 - 14) as f32);
        let dir = if left_side { -1.0 } else { 1.0 };
        let pulse = ((t * 5.0).sin() * 1.5) as f32 * dir;
        f.poly(&[(cx - 6.0 * dir + pulse + 1.0, cy + 1.0), (cx + 6.0 * dir + pulse + 1.0, cy - 9.0 + 1.0), (cx + 6.0 * dir + pulse + 1.0, cy + 9.0 + 1.0)], rgb(0x050a22));
        f.poly(&[(cx - 6.0 * dir + pulse, cy), (cx + 6.0 * dir + pulse, cy - 9.0), (cx + 6.0 * dir + pulse, cy + 9.0)], GOLD);
        hits.0.push((r, hit));
    }

    // Dialog box with the selected mode.
    let box_r = IRect::new(8 + sa.l, h - 52 - sa.b, w - 16 - sa.l - sa.r, 46);
    draw_dialog(f, box_r);
    if let Some(d) = doors.get(sel) {
        let ic = icons::icon(d.id);
        f.rect(IRect::new(box_r.x + 5, box_r.y + 7, 52, 34), rgb(0x040720));
        f.blit(&ic, box_r.x + 7, box_r.y + 9, 1);
        let title = crate::shell::ExperimentId::title(d.id).to_ascii_uppercase();
        let name = clean(&sign_name(d.id));
        let _ = title;
        f.text(box_r.x + 64, box_r.y + 6, &name, 2, GOLD);
        let tag = clean(d.id.tagline());
        let typed: usize = ((t - anim.since).max(0.0) * 60.0) as usize;
        let max_chars = ((box_r.w - 64 - 60) / 6).max(10) as usize;
        let shown: String = tag.chars().take(typed).collect();
        let wrapped = wrap_text(&shown, max_chars);
        for (k, l) in wrapped.iter().take(3).enumerate() {
            f.text(box_r.x + 64, box_r.y + 23 + k as i32 * 8, l, 1, WHITE);
        }
        let enter = IRect::new(box_r.x + box_r.w - 52, box_r.y + 8, 46, 16);
        if d.enabled {
            let pulse = (t * 5.0).sin() * 0.5 + 0.5;
            f.slab(enter, mix(rgb(0x4ad060), rgb(0x8aff8a), pulse * 0.5), rgb(0x1a8a30), rgb(0xc8ffc8), rgb(0x0a4a18), rgb(0x03220a));
            f.text_centered(enter.x + enter.w / 2, enter.y + 5, "ENTER", 1, rgb(0x04300c));
        } else {
            f.slab(enter, rgb(0x6a6a78), rgb(0x3a3a46), rgb(0x9a9aa8), rgb(0x20202a), rgb(0x0a0a10));
            f.text_centered(enter.x + enter.w / 2, enter.y + 5, "LOCKED", 1, rgb(0xc0c0cc));
        }
        hits.0.push((enter.inflate(2), Hit::Enter));
        // Modes that only take Mario say so (the character select greys the others out).
        if select_allowed(Some(super::Pick::Mode(d.id))).iter().filter(|b| **b).count() == 1 {
            let tag = "MARIO ONLY";
            f.text(box_r.x + box_r.w - 6 - Fb::text_width(tag, 1), enter.y + enter.h + 7, tag, 1, rgb(0x9fe0ff));
        }
    }
    let hint = input_hint(pad);
    f.text(w - 8 - sa.r - Fb::text_width(hint, 1), box_r.y - 9, hint, 1, rgb(0xe8e8f0));
    let _ = (fade_in, intro);
}

fn wrap_text(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        if !cur.is_empty() && cur.len() + 1 + word.len() > max {
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

#[allow(clippy::too_many_arguments)]
pub fn draw(
    active: Res<RetroActive>,
    menu: Res<MenuRes>,
    ctx: Res<CtxRes>,
    cam: Res<scene::CamState>,
    low: Res<LowRes>,
    mut hits: ResMut<Hits>,
    mut anim: ResMut<Anim>,
    time: Res<Time<Real>>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    pads: Query<&Gamepad>,
    images_res: Res<Images>,
    mut images: ResMut<Assets<Image>>,
    stage: Res<select::Stage>,
) {
    if !active.0 {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let k = low.w as f32 / window.width().max(1.0);
    let sa = Insets { l: (area.left * k).round() as i32, r: (area.right * k).round() as i32, t: (area.top * k).round() as i32, b: (area.bottom * k).round() as i32 };
    let m = &menu.0;
    let key = (m.page, m.wing, m.row, m.cur_door());
    if anim.sel != Some(key) {
        anim.sel = Some(key);
        anim.since = t;
    }
    if anim.last_wing != Some(m.wing) {
        if anim.last_wing.is_some() {
            anim.wing_flash = 1.0;
        }
        anim.last_wing = Some(m.wing);
    }
    anim.wing_flash = (anim.wing_flash - dt * 4.0).max(0.0);
    let mut f = Fb::new(low.w, low.h);
    hits.0.clear();
    let pad = !pads.is_empty();
    match m.page {
        Page::Title => draw_title(&mut f, m, &ctx.0, &mut hits, &mut anim, t, dt, sa, pad, !cfg!(target_os = "ios")),
        Page::Hub => draw_hub(&mut f, m, &ctx.0, &cam, &mut hits, &mut anim, t, sa, pad),
        Page::Select => {
            let lay = select::layout(low.w, low.h, sa);
            let cam = select::cam_for(low.w, low.h);
            let spots = select::slot_positions(&lay, &cam, low.w, low.h);
            select::draw_select(&mut f, m, &ctx.0, &cam, &spots, select::pops(&stage), &mut hits, &mut anim, t, sa, pad);
        }
    }
    // Dive into the door: fade to white-hot, and a short flash when switching wings.
    let dive = ((m.enter_progress() - 0.55) / 0.45).clamp(0.0, 1.0);
    let flash = anim.wing_flash.max(dive);
    if flash > 0.0 {
        f.rect(IRect::new(0, 0, low.w, low.h), [10, 8, 20, (flash * 255.0) as u8]);
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
    }

    #[test]
    fn retro_screen_only_replaces_the_title() {
        assert!(wants(true, AppState::Menu, Screen::Title));
        assert!(!wants(true, AppState::Menu, Screen::Courses));
        assert!(!wants(true, AppState::Paused, Screen::Title));
        assert!(!wants(false, AppState::Menu, Screen::Title));
        // The character select is always drawn here, also with the classic title; never while playing or paused.
        assert!(wants(false, AppState::Menu, Screen::CharSelect) && wants(true, AppState::Menu, Screen::CharSelect));
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
    fn hub_hit_zones_exist_for_every_control() {
        let ctx = Ctx::from_registry(Character::Mario, [true; 4], |_| true);
        let mut menu = Menu::default();
        menu.page = Page::Hub;
        let (eye, target) = scene::hub_camera(Wing::Games, 0.0, 1.0, 0.0);
        let cam = scene::CamState { eye, target, aspect: 4.0 / 3.0, ..Default::default() };
        let mut f = Fb::new(320, 240);
        let mut hits = Hits::default();
        let mut anim = Anim::default();
        draw_hub(&mut f, &menu, &ctx, &cam, &mut hits, &mut anim, 1.0, Insets::default(), false);
        for want in [Hit::Back, Hit::Prev, Hit::Next, Hit::Enter, Hit::Wing(Wing::Games), Hit::Wing(Wing::Lab), Hit::Door(0)] {
            assert!(hits.0.iter().any(|(_, h)| *h == want), "missing hit zone {want:?}");
        }
        // The selected door's sign sits in the middle of the screen.
        let (r, _) = hits.0.iter().find(|(_, h)| *h == Hit::Door(0)).unwrap();
        assert!((r.center().0 - 160).abs() < 30);
    }

    #[test]
    fn title_draw_registers_rows_and_paints_pixels() {
        let ctx = Ctx::from_registry(Character::Mario, [true; 4], |_| true);
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
    fn wrap_breaks_on_words() {
        assert_eq!(wrap_text("ONE TWO THREE", 7), vec!["ONE TWO", "THREE"]);
    }
}
