//! Arena Brawl presentation: the [`BrawlBridge`] resource between the experiment (`brawl.rs`,
//! rules glue + arena entities) and bevy systems, the pooled fighter / item / projectile / gem /
//! effect entities, the HUD (health cards, gem slots, round wins, timer, banner), the menu and
//! summary screens, and the touch / gamepad input gathering.
//!
//! Fighters, bombs, crates and hearts are SM64 actors spawned through
//! `render::object_view::spawn_model` (it plays their animation clips; the bare scene would show
//! raw rest poses), pooled per slot and only moved or hidden afterwards.
use crate::button_marker::{Button, BorderRadius};
use super::brawl_core::{Hero, GEMS, MAXF, MAXI, MAXP};
use super::XpEntity;
use crate::characters::Character;
use crate::render::{AssetRoot, SCALE};
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::{AppState, ReturnToMenu};
use crate::touch::{ButtonLabels, ModeButtons, TouchControls};
use bevy::prelude::*;

/// Fighter models by `Kind as usize` (Mario is the real player model, drawn by the game).
pub const FIGHTER_MODELS: [&str; 7] = ["", "actors/goomba/goomba_geo.glb", "actors/bobomb/black_bobomb_geo.glb", "actors/koopa/koopa_with_shell_geo.glb", "actors/bully/bully_geo.glb", "actors/chuckya/chuckya_geo.glb", "actors/king_bobomb/king_bobomb_geo.glb"];
pub const CRATE_MODEL: &str = "actors/breakable_box/breakable_box_geo.glb";
pub const BOMB_MODEL: &str = "actors/bobomb/black_bobomb_geo.glb";
pub const HEAL_MODEL: &str = "actors/mushroom_1up/mushroom_1up_geo.glb";

/// Colour per fighter slot: red, blue, yellow, green.
pub const SLOT_COLORS: [[f32; 3]; 4] = [[1.0, 0.3, 0.25], [0.3, 0.55, 1.0], [1.0, 0.85, 0.25], [0.35, 0.9, 0.4]];
pub fn slot_color(i: usize) -> Color {
    let c = SLOT_COLORS[i % 4];
    Color::srgb(c[0], c[1], c[2])
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    #[default]
    None,
    Menu,
    Summary,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UiCmd {
    /// Change menu row `.0` by `.1` steps.
    Adjust(u8, i8),
    Cursor(i8),
    Start,
    Rematch,
    Menu,
    Next,
    /// Open / close the HOW TO PLAY card.
    Help,
    CloseHelp,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FView {
    pub on: bool,
    pub kind: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub scale: f32,
    pub tilt: f32,
    pub roll: f32,
    pub ground: f32,
    pub blink: bool,
    pub human: bool,
    pub power: bool,
    pub burn: bool,
    /// Power-form glow colour (the hero's).
    pub tint: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IView {
    pub on: bool,
    pub kind: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub tilt: f32,
    pub lit: bool,
    pub held: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PView {
    pub on: bool,
    /// 0 bullet, 1 boomerang, 2 shell.
    pub kind: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub aim: Vec3,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GView {
    pub on: bool,
    pub pos: Vec3,
    pub held: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FxKind {
    #[default]
    Ring,
    Flash,
    Beam,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FxView {
    pub on: bool,
    pub kind: FxKind,
    pub pos: Vec3,
    pub to: Vec3,
    pub color: [f32; 3],
    pub age: f32,
    pub life: f32,
    pub radius: f32,
}

pub const FX_MAX: usize = 16;

#[derive(Clone, Debug, Default)]
pub struct CardView {
    pub on: bool,
    pub name: String,
    pub hp: f32,
    pub hp_num: i32,
    pub gems: u8,
    pub wins: u8,
    pub to_win: u8,
    pub power: f32,
    pub human: bool,
    pub ko: bool,
    pub tag: &'static str,
    /// 1 right after this fighter was hit, easing to 0 (the card flashes and jolts).
    pub hit: f32,
}

#[derive(Clone, Debug, Default)]
pub struct HudView {
    pub cards: [CardView; MAXF],
    pub n: usize,
    pub round: String,
    pub clock: String,
    pub banner: String,
    pub sub: String,
    pub banner_alpha: f32,
    pub warn: String,
    pub hint: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuView {
    pub mode: u8,
    pub arena: u8,
    pub bots: u8,
    pub diff: u8,
    pub monsters: u8,
    pub cursor: u8,
    /// The chosen character (`Hero as u8`).
    pub hero: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SummaryView {
    pub title: String,
    pub sub: String,
    pub lines: Vec<String>,
    pub win: bool,
    pub can_next: bool,
}

/// One extra gamepad's controls (slots 2..4; the first pad is the main player's).
#[derive(Clone, Copy, Debug, Default)]
pub struct PadState {
    pub mv: Vec2,
    pub jump: bool,
    pub jump_held: bool,
    pub attack: bool,
    pub z: bool,
    pub grab: bool,
    pub s1: bool,
    pub s2: bool,
    pub present: bool,
}

#[derive(Resource)]
pub struct BrawlBridge {
    pub active: bool,
    /// The character the player picked (Mario, Link, Young Link, Banjo).
    pub hero: Hero,
    pub screen: Screen,
    pub menu: MenuView,
    pub summary: SummaryView,
    pub cmds: Vec<UiCmd>,
    /// Touch / pad edges since the glue last drained them: bit 0 Nova, bit 1 Beam, bit 2 grab.
    pub edges: u8,
    pub pads: [PadState; 3],
    pub fighters: [FView; MAXF],
    pub items: [IView; MAXI],
    pub projs: [PView; MAXP],
    /// Monsters for `foes_view` (copied to the shared board each frame).
    pub foes: Vec<super::foes_view::FoeItem>,
    pub gems: [GView; GEMS],
    pub fx: [FxView; FX_MAX],
    pub hud: HudView,
    pub time: f32,
    /// Power buttons: available this frame.
    pub nova_ready: bool,
    pub beam_ready: bool,
    /// Cooldown left on each power button, 0 (ready) to 1 (just used); 1 also while the form is off.
    pub nova_cd: f32,
    pub beam_cd: f32,
    /// The HOW TO PLAY card is open.
    pub help: bool,
    pub phone: bool,
    /// Window aspect ratio (width / height) and the safe-area insets in logical px, for the camera.
    pub aspect: f32,
    pub insets: [f32; 4],
    /// Tide glow and wind streaks for the arena set pieces (0 none).
    pub wind: Vec3,
    pub wind_strength: f32,
}

impl Default for BrawlBridge {
    fn default() -> Self {
        Self {
            active: false,
            hero: Hero::Mario,
            screen: Screen::None,
            menu: MenuView::default(),
            summary: SummaryView::default(),
            cmds: vec![],
            edges: 0,
            pads: [PadState::default(); 3],
            fighters: [FView::default(); MAXF],
            items: [IView::default(); MAXI],
            projs: [PView::default(); MAXP],
            foes: vec![],
            gems: [GView::default(); GEMS],
            fx: [FxView::default(); FX_MAX],
            hud: HudView::default(),
            time: 0.0,
            nova_ready: false,
            beam_ready: false,
            nova_cd: 1.0,
            beam_cd: 1.0,
            help: false,
            phone: false,
            aspect: 1.78,
            insets: [0.0; 4],
            wind: Vec3::ZERO,
            wind_strength: 0.0,
        }
    }
}

impl BrawlBridge {
    pub fn reset(&mut self) {
        *self = BrawlBridge { active: self.active, help: self.help, hero: self.hero, ..Default::default() };
    }
}

pub struct BrawlViewPlugin;

impl Plugin for BrawlViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BrawlBridge>().init_resource::<Pools>().init_resource::<UiState>();
        app.add_systems(PostUpdate, mario_scale.before(bevy::transform::TransformSystems::Propagate));
        app.add_systems(Update, pick_hero.before(super::run_frame));
        app.add_systems(Update, gather_input.before(super::run_frame).run_if(|b: Res<BrawlBridge>| b.active));
        app.add_systems(Update, (sync_render, drive_touch, manage_ui, update_hud, ui_buttons, menu_keys, overlay_display).chain().after(super::run_frame).run_if(|b: Res<BrawlBridge>| b.active));
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<BrawlBridge>| !b.active));
        app.add_systems(Update, push_foes.after(super::run_frame).before(super::foes_view::sync_foes).run_if(|b: Res<BrawlBridge>| b.active));
    }
}

// ---------------------------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------------------------

fn push_foes(bridge: Res<BrawlBridge>, mut board: ResMut<super::foes_view::FoeBoard>) {
    board.items.extend(bridge.foes.iter().copied());
}

fn gather_input(mut bridge: ResMut<BrawlBridge>, mut tc: ResMut<TouchControls>, pads: Query<&Gamepad>, mut l_held: Local<bool>) {
    let bits = std::mem::take(&mut tc.mode_pressed);
    bridge.edges |= bits & 0b111;
    // The touch L button is an explicit grab / throw (it also works in the air).
    if tc.focus && !*l_held {
        bridge.edges |= 4;
    }
    *l_held = tc.focus;
    // Gamepad 1 drives the main player through the shared controller; the other pads join as
    // extra local fighters (Versus).
    for (k, pad) in pads.iter().skip(1).take(3).enumerate() {
        let s = pad.left_stick();
        let p = &mut bridge.pads[k];
        p.present = true;
        p.mv = Vec2::new(s.x, s.y);
        p.jump_held = pad.pressed(GamepadButton::South);
        p.jump = pad.just_pressed(GamepadButton::South);
        p.attack = pad.just_pressed(GamepadButton::West);
        p.grab = pad.just_pressed(GamepadButton::East);
        p.z = pad.just_pressed(GamepadButton::RightTrigger2) || pad.just_pressed(GamepadButton::LeftTrigger2);
        p.s1 = pad.just_pressed(GamepadButton::North);
        p.s2 = pad.just_pressed(GamepadButton::RightTrigger);
    }
    for k in pads.iter().count().saturating_sub(1).min(3)..3 {
        bridge.pads[k].present = false;
    }
    // The first pad's special buttons (the shared controller has no room for them).
    if let Some(pad) = pads.iter().next() {
        if pad.just_pressed(GamepadButton::North) {
            bridge.edges |= 1;
        }
        if pad.just_pressed(GamepadButton::RightTrigger) {
            bridge.edges |= 2;
        }
        if pad.just_pressed(GamepadButton::LeftTrigger) {
            bridge.edges |= 4;
        }
    }
}

fn drive_touch(mut bridge: ResMut<BrawlBridge>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>, area: Res<SafeArea>, mut tc: ResMut<TouchControls>, mut mode: ResMut<ModeButtons>, mut labels: ResMut<ButtonLabels>, mut minimal: ResMut<crate::hud::HudMinimal>) {
    if !minimal.0 {
        minimal.0 = true;
    }
    if let Ok(w) = windows.single() {
        let a = w.width() / w.height().max(1.0);
        if (bridge.aspect - a).abs() > 1e-3 {
            bridge.aspect = a;
        }
    }
    let ins = [area.left, area.right, area.bottom, area.top];
    if bridge.insets != ins {
        bridge.insets = ins;
    }
    if bridge.phone != tc.visible {
        bridge.phone = tc.visible;
    }
    let menu = bridge.screen != Screen::None || bridge.help;
    if tc.suppress != menu {
        tc.suppress = menu;
    }
    // The stick and buttons would sit on top of the menu's own buttons.
    if tc.hide_pad != menu {
        tc.hide_pad = menu;
    }
    if !mode.active {
        mode.active = true;
    }
    let want = ButtonLabels { jump: Some("JUMP".into()), punch: Some("HIT".into()), crouch: Some("GRAB".into()), jump_tint: None };
    if *labels != want {
        *labels = want;
    }
    let lit = |ready: bool, c: Color| if ready { Some(c) } else { Some(Color::srgba(0.22, 0.22, 0.3, 0.6)) };
    let t0 = lit(bridge.nova_ready, Color::srgb(0.85, 0.45, 0.1));
    let t1 = lit(bridge.beam_ready, Color::srgb(0.25, 0.5, 0.9));
    // SHOCKWAVE blasts everything around you; SEEKER BEAM zaps the nearest foe from afar. Both are power-form moves.
    let l = ["SHOCK\nWAVE".to_string(), "SEEK\nBEAM".to_string(), String::new()];
    if mode.labels != l {
        mode.labels = l;
    }
    let fill = [bridge.nova_cd, bridge.beam_cd, 0.0];
    if mode.fill != fill {
        mode.fill = fill;
    }
    if mode.tint[0] != t0 {
        mode.tint[0] = t0;
    }
    if mode.tint[1] != t1 {
        mode.tint[1] = t1;
    }
    if mode.tint[2].is_some() {
        mode.tint[2] = None;
    }
}

fn menu_keys(mut bridge: ResMut<BrawlBridge>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, state: Res<AppState>, mut leave: MessageWriter<ReturnToMenu>) {
    if *state != AppState::Playing {
        return;
    }
    let pad = |b: GamepadButton| pads.iter().any(|p| p.just_pressed(b));
    let up = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) || pad(GamepadButton::DPadUp);
    let down = keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) || pad(GamepadButton::DPadDown);
    let left = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) || pad(GamepadButton::DPadLeft);
    let right = keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) || pad(GamepadButton::DPadRight);
    let confirm = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) || pad(GamepadButton::South);
    // HOW TO PLAY: F1 or `/` opens it any time, a confirm / F1 / `/` / B closes it.
    let help_key = keys.just_pressed(KeyCode::F1) || keys.just_pressed(KeyCode::Slash);
    if bridge.help {
        if help_key || confirm || keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) {
            bridge.cmds.push(UiCmd::CloseHelp);
        }
        return;
    }
    if help_key || pad(GamepadButton::Select) {
        bridge.cmds.push(UiCmd::Help);
        return;
    }
    match bridge.screen {
        Screen::Menu => {
            let row = bridge.menu.cursor;
            if up {
                bridge.cmds.push(UiCmd::Cursor(-1));
            }
            if down {
                bridge.cmds.push(UiCmd::Cursor(1));
            }
            if left {
                bridge.cmds.push(UiCmd::Adjust(row, -1));
            }
            if right {
                bridge.cmds.push(UiCmd::Adjust(row, 1));
            }
            if confirm {
                if row == 5 {
                    bridge.cmds.push(UiCmd::Start);
                } else {
                    bridge.cmds.push(UiCmd::Adjust(row, 1));
                }
            }
            if keys.just_pressed(KeyCode::KeyN) {
                bridge.cmds.push(UiCmd::Start);
            }
            // B backs out of the setup screen: the hub.
            if pad(GamepadButton::East) {
                leave.write(ReturnToMenu);
            }
        }
        Screen::Summary => {
            if confirm {
                let c = if bridge.summary.can_next { UiCmd::Next } else { UiCmd::Rematch };
                bridge.cmds.push(c);
            }
            if pad(GamepadButton::East) || keys.just_pressed(KeyCode::KeyN) {
                bridge.cmds.push(UiCmd::Menu);
            }
        }
        Screen::None => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Pooled world entities
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct Pooled;

#[derive(Resource, Default)]
struct Pools {
    ready: bool,
    fighter: [Option<(u8, Entity)>; MAXF],
    ring: Vec<Entity>,
    /// The power form's translucent glow around each fighter.
    aura: Vec<Entity>,
    shadow: Vec<Entity>,
    arrow: Vec<Entity>,
    /// Pooled item entities by item kind; `slot` maps item slot -> (kind, entity).
    free: [Vec<Entity>; 10],
    slot: Vec<Option<(u8, Entity)>>,
    proj: Vec<Option<(u8, Entity)>>,
    proj_free: [Vec<Entity>; 5],
    telegraph: Vec<Entity>,
    gem: Vec<Entity>,
    pillar: Vec<Entity>,
    orb: Vec<Entity>,
    fx_ring: Vec<Entity>,
    fx_flash: Vec<Entity>,
    fx_beam: Vec<Entity>,
    /// One material per pooled effect (ring, flash, beam) so each can be tinted and faded on its own.
    fx_mats: [Vec<Handle<StandardMaterial>>; 3],
    streaks: Vec<Entity>,
    meshes: Option<Meshes>,
}

#[derive(Clone)]
struct Meshes {
    disc: Handle<Mesh>,
    annulus: Handle<Mesh>,
    sphere: Handle<Mesh>,
    gem: Handle<Mesh>,
    cube: Handle<Mesh>,
    cyl: Handle<Mesh>,
    cone: Handle<Mesh>,
    shadow_mat: Handle<StandardMaterial>,
    ring_mats: [Handle<StandardMaterial>; 4],
    aura_mats: [Handle<StandardMaterial>; 4],
    gem_mats: [Handle<StandardMaterial>; GEMS],
    pillar_mats: [Handle<StandardMaterial>; GEMS],
    white_glow: Handle<StandardMaterial>,
    orange_glow: Handle<StandardMaterial>,
    red_glow: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    rock: Handle<StandardMaterial>,
    steel: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    teal: Handle<StandardMaterial>,
    black: Handle<StandardMaterial>,
}

/// Colour of an effect `k` (0..1) of the way through its life: rings and beams fade out evenly, a flash
/// holds for its first third and then drops away.
pub fn fx_color(kind: FxKind, color: [f32; 3], k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    let c = if color == [0.0; 3] {
        match kind {
            FxKind::Flash => [1.0, 0.6, 0.15],
            _ => [1.0, 1.0, 0.95],
        }
    } else {
        color
    };
    let a = match kind {
        FxKind::Ring => 0.9 * (1.0 - k) * (1.0 - k).sqrt(),
        FxKind::Flash => 0.85 * (1.0 - ((k - 0.33) / 0.67).max(0.0).powi(2)),
        FxKind::Beam => 0.9 * (1.0 - k * k),
    };
    Color::srgba(c[0], c[1], c[2], a)
}

fn glow(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })
}

fn solid(materials: &mut Assets<StandardMaterial>, c: Color, rough: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, perceptual_roughness: rough, ..default() })
}

fn build_meshes(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Meshes {
    let gem_color = |i: usize, a: f32| {
        let c = super::brawl_core::GEM_COLORS[i];
        Color::srgba(c[0], c[1], c[2], a)
    };
    Meshes {
        disc: meshes.add(Circle::new(1.0)),
        annulus: meshes.add(Annulus::new(0.86, 1.0)),
        sphere: meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap()),
        gem: meshes.add(Sphere::new(1.0).mesh().ico(0).unwrap()),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cyl: meshes.add(Cylinder::new(1.0, 1.0)),
        cone: meshes.add(Cone { radius: 1.0, height: 1.0 }),
        shadow_mat: glow(materials, Color::srgba(0.0, 0.0, 0.05, 0.4)),
        ring_mats: std::array::from_fn(|i| glow(materials, slot_color(i).with_alpha(0.85))),
        aura_mats: std::array::from_fn(|_| glow(materials, Color::srgba(1.0, 0.82, 0.25, 0.0))),
        gem_mats: std::array::from_fn(|i| materials.add(StandardMaterial { base_color: gem_color(i, 1.0), emissive: LinearRgba::rgb(super::brawl_core::GEM_COLORS[i][0] * 2.5, super::brawl_core::GEM_COLORS[i][1] * 2.5, super::brawl_core::GEM_COLORS[i][2] * 2.5), perceptual_roughness: 0.15, ..default() })),
        pillar_mats: std::array::from_fn(|i| glow(materials, gem_color(i, 0.28))),
        white_glow: glow(materials, Color::srgba(1.0, 1.0, 0.95, 0.9)),
        orange_glow: glow(materials, Color::srgba(1.0, 0.6, 0.15, 0.85)),
        red_glow: glow(materials, Color::srgba(1.0, 0.15, 0.1, 0.55)),
        wood: solid(materials, Color::srgb(0.55, 0.34, 0.16), 0.9),
        rock: solid(materials, Color::srgb(0.5, 0.5, 0.54), 1.0),
        steel: solid(materials, Color::srgb(0.7, 0.72, 0.78), 0.35),
        gold: solid(materials, Color::srgb(0.95, 0.75, 0.2), 0.4),
        teal: solid(materials, Color::srgb(0.2, 0.75, 0.75), 0.4),
        black: solid(materials, Color::srgb(0.06, 0.06, 0.08), 0.5),
    }
}

pub fn place(q: &mut Query<(&mut Transform, &mut Visibility), With<Pooled>>, e: Entity, show: bool, pos: Vec3, rot: Quat, scale: Vec3) {
    let Ok((mut tf, mut vis)) = q.get_mut(e) else { return };
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if show {
        tf.translation = pos;
        tf.rotation = rot;
        tf.scale = scale;
    }
}

fn prim(commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>) -> Entity {
    commands.spawn((XpEntity, Pooled, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden)).id()
}

/// A hammer / boomerang / blaster built from a few boxes, as one pooled parent.
fn weapon_entity(commands: &mut Commands, m: &Meshes, kind: u8) -> Entity {
    let parent = commands.spawn((XpEntity, Pooled, Transform::default(), Visibility::Hidden)).id();
    let part = |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>, t: Transform| -> Entity { commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), t)).id() };
    let kids: Vec<Entity> = match kind {
        // Hammer: wooden shaft with a steel head (units: SM64/100).
        4 => vec![part(commands, &m.cyl, &m.wood, Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)).with_scale(Vec3::new(0.09, 1.0, 0.09))), part(commands, &m.cube, &m.steel, Transform::from_translation(Vec3::new(0.0, 0.55, 0.0)).with_scale(Vec3::new(0.7, 0.4, 0.4)))],
        // Boomerang: two gold wings in a V.
        5 => vec![
            part(commands, &m.cube, &m.gold, Transform::from_translation(Vec3::new(0.18, 0.0, 0.0)).with_rotation(Quat::from_rotation_y(0.55)).with_scale(Vec3::new(0.62, 0.08, 0.16))),
            part(commands, &m.cube, &m.gold, Transform::from_translation(Vec3::new(-0.18, 0.0, 0.0)).with_rotation(Quat::from_rotation_y(-0.55)).with_scale(Vec3::new(0.62, 0.08, 0.16))),
        ],
        // Blaster: teal body with a dark barrel.
        _ => vec![part(commands, &m.cube, &m.teal, Transform::from_translation(Vec3::ZERO).with_scale(Vec3::new(0.22, 0.3, 0.55))), part(commands, &m.cyl, &m.black, Transform::from_translation(Vec3::new(0.0, 0.05, 0.42)).with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(0.07, 0.4, 0.07)))],
    };
    commands.entity(parent).add_children(&kids);
    parent
}

fn spawn_actor(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, m: &Meshes, path: &str, fallback: &Handle<Mesh>, fallback_mat: &Handle<StandardMaterial>) -> Entity {
    if root.exists(path) {
        let mut e = crate::render::object_view::spawn_model(commands, assets, root, path, true);
        e.insert((XpEntity, Pooled, Visibility::Hidden));
        e.id()
    } else {
        let _ = m;
        prim(commands, fallback, fallback_mat)
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_render(
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<BrawlBridge>,
    mut pools: ResMut<Pools>,
    mut q: Query<(&mut Transform, &mut Visibility), With<Pooled>>,
) {
    if !pools.ready {
        pools.ready = true;
        let m = build_meshes(&mut meshes, &mut materials);
        for i in 0..MAXF {
            let s = prim(&mut commands, &m.disc, &m.shadow_mat);
            pools.shadow.push(s);
            let r = prim(&mut commands, &m.annulus, &m.ring_mats[i]);
            pools.ring.push(r);
            let a = prim(&mut commands, &m.cone, &m.ring_mats[i]);
            pools.arrow.push(a);
            let au = prim(&mut commands, &m.sphere, &m.aura_mats[i]);
            pools.aura.push(au);
        }
        for g in 0..GEMS {
            let e = prim(&mut commands, &m.gem, &m.gem_mats[g]);
            pools.gem.push(e);
            let p = prim(&mut commands, &m.cyl, &m.pillar_mats[g]);
            pools.pillar.push(p);
            let o = prim(&mut commands, &m.gem, &m.gem_mats[g]);
            pools.orb.push(o);
        }
        for _ in 0..3 {
            let t = prim(&mut commands, &m.annulus, &m.red_glow);
            pools.telegraph.push(t);
        }
        for _ in 0..FX_MAX {
            let (rm, fm, bm) = (glow(&mut materials, Color::srgba(1.0, 1.0, 0.95, 0.9)), glow(&mut materials, Color::srgba(1.0, 0.6, 0.15, 0.85)), glow(&mut materials, Color::srgba(1.0, 1.0, 0.95, 0.9)));
            let r = prim(&mut commands, &m.annulus, &rm);
            pools.fx_ring.push(r);
            let f = prim(&mut commands, &m.sphere, &fm);
            pools.fx_flash.push(f);
            let b = prim(&mut commands, &m.cube, &bm);
            pools.fx_beam.push(b);
            pools.fx_mats[0].push(rm);
            pools.fx_mats[1].push(fm);
            pools.fx_mats[2].push(bm);
        }
        for _ in 0..14 {
            let s = prim(&mut commands, &m.cube, &m.white_glow);
            pools.streaks.push(s);
        }
        pools.slot = vec![None; MAXI];
        pools.proj = vec![None; MAXP];
        pools.meshes = Some(m);
    }
    let pools = &mut *pools;
    let Some(m) = pools.meshes.clone() else { return };
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let t = bridge.time;

    // ---- fighters ----
    for i in 0..MAXF {
        let f = bridge.fighters[i];
        // Model swap when the kind changes (new match / stage).
        if let Some((k, e)) = pools.fighter[i] {
            if !f.on && k == 0 || (f.on && k != f.kind) {
                if k != 0 {
                    commands.entity(e).despawn();
                }
                pools.fighter[i] = None;
            }
        }
        if f.on && pools.fighter[i].is_none() {
            let path = FIGHTER_MODELS.get(f.kind as usize).copied().unwrap_or("");
            if f.kind == 0 || path.is_empty() {
                // Mario: the game's own player model; remember a marker so we do not retry.
                pools.fighter[i] = Some((0, Entity::PLACEHOLDER));
            } else {
                let e = spawn_actor(&mut commands, &assets, &root, &m, path, &m.sphere, &m.rock);
                pools.fighter[i] = Some((f.kind, e));
            }
        }
        if let Some((k, e)) = pools.fighter[i] {
            if k != 0 {
                let blink_hide = f.blink && ((t * 24.0) as i32 % 2 == 0);
                let rot = Quat::from_rotation_y(f.yaw) * Quat::from_rotation_x(f.tilt) * Quat::from_rotation_z(f.roll);
                place(&mut q, e, f.on && !blink_hide, f.pos, rot, Vec3::splat(f.scale));
            }
        }
        let g = f.pos.with_y(f.ground);
        place(&mut q, pools.shadow[i], f.on, g + Vec3::Y * 0.05, flat, Vec3::splat(0.45 * f.scale.max(0.5) * (1.0 - ((f.pos.y - f.ground) * 0.0015).min(0.5))));
        let pulse = 1.0 + 0.08 * (t * 6.0).sin();
        // Power form: a pulsing glow in the hero's colour round the body (gold for the bots), the ring takes the colour too.
        {
            let tint = if f.power { f.tint } else { [1.0, 0.82, 0.25] };
            if let Some(mut mat) = materials.get_mut(&m.aura_mats[i]) {
                let a = if f.power && f.on { 0.20 + 0.07 * (t * 9.0).sin() } else { 0.0 };
                mat.base_color = Color::srgba(tint[0], tint[1], tint[2], a);
            }
            if f.power {
                if let Some(mut mat) = materials.get_mut(&m.ring_mats[i]) {
                    mat.base_color = Color::srgba(tint[0], tint[1], tint[2], 0.9);
                }
            } else if let Some(mut mat) = materials.get_mut(&m.ring_mats[i]) {
                mat.base_color = slot_color(i).with_alpha(0.85);
            }
            let k = f.scale.max(0.5);
            place(&mut q, pools.aura[i], f.on && f.power, f.pos + Vec3::Y * (80.0 * SCALE * k), Quat::IDENTITY, Vec3::splat(78.0 * SCALE * k * (1.0 + 0.05 * (t * 7.0).sin())));
        }
        place(&mut q, pools.ring[i], f.on, g + Vec3::Y * 0.07, flat, Vec3::splat(0.78 * f.scale.max(0.5) * pulse * if f.power { 1.15 } else { 1.0 }));
        place(&mut q, pools.arrow[i], f.on && f.human, f.pos + Vec3::Y * (2.1 * f.scale.max(1.0) + 0.1 * (t * 5.0).sin()), Quat::from_rotation_x(std::f32::consts::PI), Vec3::new(0.22, 0.34, 0.22));
    }

    // ---- items ----
    for k in 0..MAXI {
        let it = bridge.items[k];
        if let Some((kind, e)) = pools.slot[k] {
            if !it.on || kind != it.kind {
                if let Ok((_, mut v)) = q.get_mut(e) {
                    *v = Visibility::Hidden;
                }
                pools.free[kind as usize].push(e);
                pools.slot[k] = None;
            }
        }
        if !it.on {
            continue;
        }
        let e = match pools.slot[k] {
            Some((_, e)) => e,
            None => {
                let e = pools.free[it.kind as usize].pop().unwrap_or_else(|| match it.kind {
                    0 => spawn_actor(&mut commands, &assets, &root, &m, CRATE_MODEL, &m.cube, &m.wood),
                    1 => {
                        let b = prim(&mut commands, &m.cyl, &m.wood);
                        b
                    }
                    2 => prim(&mut commands, &m.sphere, &m.rock),
                    3 => spawn_actor(&mut commands, &assets, &root, &m, BOMB_MODEL, &m.sphere, &m.black),
                    7 => spawn_actor(&mut commands, &assets, &root, &m, HEAL_MODEL, &m.sphere, &m.white_glow),
                    8 => {
                        // Winged boots: two orange wedges on a glowing disc.
                        let parent = commands.spawn((XpEntity, Pooled, Transform::default(), Visibility::Hidden)).id();
                        let kids: Vec<Entity> = [-0.22f32, 0.22].iter().map(|x| commands.spawn((Mesh3d(m.cube.clone()), MeshMaterial3d(m.orange_glow.clone()), Transform::from_translation(Vec3::new(*x, 0.0, 0.0)).with_scale(Vec3::new(0.3, 0.26, 0.55)))).id()).collect();
                        commands.entity(parent).add_children(&kids);
                        parent
                    }
                    9 => prim(&mut commands, &m.sphere, &m.pillar_mats[1]),
                    other => weapon_entity(&mut commands, &m, other),
                });
                pools.slot[k] = Some((it.kind, e));
                e
            }
        };
        let (scale, lift) = match it.kind {
            0 => (Vec3::splat(0.85), 0.0),
            1 => (Vec3::new(0.46, 0.9, 0.46), 0.45),
            2 => (Vec3::new(0.42, 0.34, 0.42), 0.3),
            3 => (Vec3::splat(if it.lit { 0.8 + 0.12 * (t * 20.0).sin().abs() } else { 0.8 }), 0.0),
            7 => (Vec3::splat(1.2 + 0.1 * (t * 3.0).sin()), 0.0),
            8 => (Vec3::splat(1.1 + 0.08 * (t * 4.0).sin()), 0.45),
            9 => (Vec3::splat(0.55 + 0.06 * (t * 3.0).sin()), 0.55),
            _ => (Vec3::splat(1.0), 0.35),
        };
        let rot = Quat::from_rotation_y(it.yaw) * Quat::from_rotation_x(it.tilt);
        place(&mut q, e, true, it.pos + Vec3::Y * lift * if it.held { 0.0 } else { 1.0 }, rot, scale);
    }

    // ---- projectiles ----
    for k in 0..MAXP {
        let p = bridge.projs[k];
        if let Some((kind, e)) = pools.proj[k] {
            if !p.on || kind != p.kind {
                if let Ok((_, mut v)) = q.get_mut(e) {
                    *v = Visibility::Hidden;
                }
                pools.proj_free[kind as usize].push(e);
                pools.proj[k] = None;
            }
        }
        if !p.on {
            continue;
        }
        let e = match pools.proj[k] {
            Some((_, e)) => e,
            None => {
                let e = pools.proj_free[p.kind as usize].pop().unwrap_or_else(|| match p.kind {
                    0 => prim(&mut commands, &m.sphere, &m.white_glow),
                    1 => weapon_entity(&mut commands, &m, 5),
                    3 => prim(&mut commands, &m.sphere, &m.wood),
                    4 => prim(&mut commands, &m.sphere, &m.white_glow),
                    _ => prim(&mut commands, &m.sphere, &m.black),
                });
                pools.proj[k] = Some((p.kind, e));
                e
            }
        };
        match p.kind {
            0 => place(&mut q, e, true, p.pos, Quat::IDENTITY, Vec3::splat(0.16)),
            1 => place(&mut q, e, true, p.pos, Quat::from_rotation_y(t * 22.0), Vec3::splat(1.1)),
            3 => place(&mut q, e, true, p.pos, Quat::IDENTITY, Vec3::splat(0.26)),
            4 => place(&mut q, e, true, p.pos, Quat::IDENTITY, Vec3::splat(0.34)),
            _ => place(&mut q, e, true, p.pos, Quat::IDENTITY, Vec3::splat(0.42)),
        }
    }
    // Shell telegraphs.
    let mut ti = 0;
    for p in bridge.projs.iter().filter(|p| p.on && (p.kind == 2 || p.kind == 4)) {
        if ti < pools.telegraph.len() {
            let pulse = 1.0 + 0.12 * (t * 14.0).sin();
            place(&mut q, pools.telegraph[ti], true, p.aim + Vec3::Y * 0.1, flat, Vec3::splat(2.35 * pulse));
            ti += 1;
        }
    }
    for e in pools.telegraph.iter().skip(ti) {
        place(&mut q, *e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    }

    // ---- gems ----
    for g in 0..GEMS {
        let gv = bridge.gems[g];
        let spin = Quat::from_rotation_y(t * 2.2 + g as f32) * Quat::from_rotation_z(0.3);
        let free = gv.on && !gv.held;
        place(&mut q, pools.gem[g], free, gv.pos + Vec3::Y * (0.55 + 0.1 * (t * 3.0 + g as f32).sin()), spin, Vec3::splat(0.34));
        place(&mut q, pools.pillar[g], free, Vec3::new(gv.pos.x, gv.pos.y + 1.6, gv.pos.z), Quat::IDENTITY, Vec3::new(0.2, 3.2, 0.2));
        // Held gems orbit their carrier.
        let a = t * 4.0 + g as f32 * 2.1;
        place(&mut q, pools.orb[g], gv.on && gv.held, gv.pos + Vec3::new(a.cos() * 0.8, 1.6 + 0.2 * a.sin(), a.sin() * 0.8), spin, Vec3::splat(0.2));
    }

    // ---- effects ----
    let (mut r_i, mut f_i, mut b_i) = (0, 0, 0);
    for fx in bridge.fx.iter().filter(|f| f.on) {
        let k = (fx.age / fx.life.max(0.01)).clamp(0.0, 1.0);
        // Tint and fade (an all-zero colour means the kind's default).
        let (slot, idx) = match fx.kind {
            FxKind::Ring => (0, r_i),
            FxKind::Flash => (1, f_i),
            FxKind::Beam => (2, b_i),
        };
        if let Some(h) = pools.fx_mats[slot].get(idx) {
            let want = fx_color(fx.kind, fx.color, k);
            if materials.get(h).is_some_and(|m| m.base_color != want) {
                if let Some(mut m) = materials.get_mut(h) {
                    m.base_color = want;
                }
            }
        }
        match fx.kind {
            FxKind::Ring if r_i < pools.fx_ring.len() => {
                place(&mut q, pools.fx_ring[r_i], true, fx.pos + Vec3::Y * 0.12, flat, Vec3::splat((fx.radius * SCALE * (0.15 + 0.85 * k)).max(0.01)));
                r_i += 1;
            }
            FxKind::Flash if f_i < pools.fx_flash.len() => {
                place(&mut q, pools.fx_flash[f_i], true, fx.pos + Vec3::Y * 0.5, Quat::IDENTITY, Vec3::splat((fx.radius * SCALE * (0.3 + 0.7 * k.sqrt()) * (1.0 - 0.6 * k)).max(0.01)));
                f_i += 1;
            }
            FxKind::Beam if b_i < pools.fx_beam.len() => {
                let d = fx.to - fx.pos;
                let len = d.length().max(0.01);
                let rot = Quat::from_rotation_arc(Vec3::Z, d / len);
                place(&mut q, pools.fx_beam[b_i], true, fx.pos + d * 0.5, rot, Vec3::new(0.14 * (1.0 - k), 0.14 * (1.0 - k), len));
                b_i += 1;
            }
            _ => {}
        }
    }
    for e in pools.fx_ring.iter().skip(r_i) {
        place(&mut q, *e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    }
    for e in pools.fx_flash.iter().skip(f_i) {
        place(&mut q, *e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    }
    for e in pools.fx_beam.iter().skip(b_i) {
        place(&mut q, *e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    }
    // Wind streaks drift downwind across the arena.
    let w = bridge.wind_strength;
    for (i, e) in pools.streaks.iter().enumerate() {
        if w < 0.05 {
            place(&mut q, *e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
            continue;
        }
        let ph = (t * 0.9 + i as f32 * 0.37).fract();
        let across = (i as f32 * 7.31).sin() * 8.0;
        let h = 0.6 + ((i * 5) % 7) as f32 * 0.45;
        let dir = bridge.wind;
        let side = Vec3::new(-dir.z, 0.0, dir.x);
        let pos = dir * (ph * 20.0 - 10.0) + side * across + Vec3::Y * h;
        let rot = Quat::from_rotation_arc(Vec3::Z, dir.normalize_or(Vec3::Z));
        place(&mut q, *e, true, pos, rot, Vec3::new(0.03, 0.03, 1.4 * w));
    }
}

/// Entities are despawned by the framework on exit; forget the handles.
fn cleanup(mut pools: ResMut<Pools>, mut minimal: ResMut<crate::hud::HudMinimal>, mut tc: ResMut<TouchControls>, mut mode: ResMut<ModeButtons>, mut labels: ResMut<ButtonLabels>, mut ui: ResMut<UiState>, mut sim: ResMut<crate::game::runner::GameSim>) {
    // Leave the character rig clean: Gem Clash drives Link's pose hint and stance through the sim's runtime.
    if pools.ready {
        sim.mario.link.pose = None;
        sim.mario.controller.focus_yaw = None;
    }
    if pools.ready {
        if minimal.0 {
            minimal.0 = false;
        }
        if tc.suppress {
            tc.suppress = false;
        }
        tc.hide_pad = false;
        mode.active = false;
        *labels = ButtonLabels::default();
        *pools = Pools::default();
    }
    if ui.hud.is_some() || ui.screen.is_some() {
        *ui = UiState::default();
    }
}

// ---------------------------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct Overlay;

#[derive(Component, Clone, Copy, PartialEq)]
enum Act {
    Row(u8),
    Arrow(u8, i8),
    Start,
    Rematch,
    Next,
    Menu,
    Help,
    CloseHelp,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum HudText {
    Name(u8),
    Wins(u8),
    Round,
    Clock,
    Banner,
    Sub,
    Warn,
    Hint,
}

#[derive(Component, Clone, Copy)]
enum HudFill {
    Hp(u8),
    /// The pale bar behind the health bar that drains a moment later, showing the damage just taken.
    Ghost(u8),
    Power(u8),
}

/// HUD animation state: each card's lagging health bar and the banner's pop-in.
#[derive(Default)]
struct HudAnim {
    ghost: [f32; MAXF],
    banner: String,
    banner_t: f32,
}

/// The lagging health bar: follows `hp` up at once, waits while the card still reels from the hit, then drains.
fn ghost_step(ghost: f32, hp: f32, hit: f32, dt: f32) -> f32 {
    if ghost <= hp {
        hp
    } else if hit > 0.25 {
        ghost
    } else {
        (ghost - dt * 0.55).max(hp)
    }
}

#[derive(Component)]
struct GemDot(u8, u8);

#[derive(Component)]
struct CardRoot(u8);

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    help_btn: Option<Entity>,
    help: Option<Entity>,
    hud_key: (usize, bool, i32),
    screen: Option<(Entity, Screen, u64)>,
    texts: Vec<(HudText, String)>,
}

fn txt(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY })
}

fn root_node(area: &SafeArea) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        padding: UiRect { left: Val::Px(area.left), right: Val::Px(area.right), top: Val::Px(area.top), bottom: Val::Px(area.bottom) },
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn spawn_hud(commands: &mut Commands, a: &UiAssets, area: &SafeArea, n: usize, touch: bool) -> Entity {
    let a = a.hud(); // in-game heads-up element: keeps the pre-v3 look
    let compact = touch;
    let fs = if compact { 11.0 } else { 13.0 };
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + if compact { 4.0 } else { 8.0 });
    node.padding.left = Val::Px(area.left.max(if compact { 10.0 } else { 14.0 }));
    node.padding.right = Val::Px(area.right.max(if compact { 10.0 } else { 14.0 }) + if touch { 56.0 } else { 0.0 });
    node.align_items = AlignItems::Stretch;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), node)).id();
    commands.entity(root).with_children(|p| {
        p.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::FlexStart, justify_content: JustifyContent::SpaceBetween, width: Val::Percent(100.0), ..default() }).with_children(|row| {
            for idx in 0..n {
                if idx == n.div_ceil(2) {
                    // Round / clock pill between the cards.
                    row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)), min_width: Val::Px(if compact { 64.0 } else { 88.0 }), ..default() }, BackgroundColor(Color::srgba(0.02, 0.04, 0.16, 0.78)), BorderRadius::all(Val::Px(10.0)))).with_children(|c| {
                        c.spawn((HudText::Clock, Text::new(""), font(&a.display, if compact { 20.0 } else { 26.0 }), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                        c.spawn((HudText::Round, Text::new(""), font(&a.body, fs - 1.0), TextColor(CREAM)));
                    });
                }
                let col = slot_color(idx);
                row.spawn((CardRoot(idx as u8), Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: Val::Px(0.0), max_width: Val::Px(210.0), padding: UiRect::all(Val::Px(if compact { 3.0 } else { 5.0 })), row_gap: Val::Px(2.0), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.04, 0.16, 0.78)), BorderColor::all(col), BorderRadius::all(Val::Px(9.0)))).with_children(|card| {
                    card.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                        r.spawn((HudText::Name(idx as u8), Text::new(""), font(&a.body, fs), TextColor(col), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY }));
                        r.spawn((HudText::Wins(idx as u8), Text::new(""), font(&a.body, fs), TextColor(GOLD_HI)));
                    });
                    card.spawn((Node { height: Val::Px(if compact { 8.0 } else { 11.0 }), width: Val::Percent(100.0), border: UiRect::all(Val::Px(1.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(4.0)))).with_children(|bar| {
                        bar.spawn((HudFill::Ghost(idx as u8), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), height: Val::Percent(100.0), width: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgba(1.0, 0.96, 0.86, 0.85))));
                        bar.spawn((HudFill::Hp(idx as u8), Node { height: Val::Percent(100.0), width: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0x58d26a))));
                    });
                    card.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(3.0), height: Val::Px(if compact { 9.0 } else { 12.0 }), ..default() }).with_children(|r| {
                        for g in 0..GEMS {
                            let c = super::brawl_core::GEM_COLORS[g];
                            r.spawn((GemDot(idx as u8, g as u8), Node { width: Val::Px(if compact { 8.0 } else { 10.0 }), height: Val::Px(if compact { 8.0 } else { 10.0 }), border: UiRect::all(Val::Px(1.0)), ..default() }, BackgroundColor(Color::srgba(c[0], c[1], c[2], 0.2)), BorderColor::from(Color::srgb(c[0], c[1], c[2])), BorderRadius::all(Val::Px(3.0))));
                        }
                        r.spawn((Node { flex_grow: 1.0, height: Val::Px(5.0), overflow: Overflow::clip(), margin: UiRect::left(Val::Px(4.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)), BorderRadius::all(Val::Px(3.0)))).with_children(|b| {
                            b.spawn((HudFill::Power(idx as u8), Node { height: Val::Percent(100.0), width: Val::Percent(0.0), ..default() }, BackgroundColor(GOLD)));
                        });
                    });
                });
            }
        });
        // Banner and sub-banner in the upper third; warning line and hint below the cards.
        p.spawn((Node { position_type: PositionType::Absolute, top: Val::Percent(24.0), left: Val::Px(0.0), width: Val::Percent(100.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() },)).with_children(|c| {
            c.spawn((HudText::Banner, Text::new(""), font(&a.display, if compact { 38.0 } else { 56.0 }), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
            c.spawn((HudText::Sub, Text::new(""), font(&a.body, if compact { 14.0 } else { 18.0 }), TextColor(CREAM), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
        });
        p.spawn((Node { position_type: PositionType::Absolute, top: Val::Px(if compact { 58.0 } else { 74.0 }), left: Val::Px(0.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },)).with_children(|c| {
            c.spawn((HudText::Warn, Text::new(""), font(&a.display, if compact { 17.0 } else { 22.0 }), TextColor(RED_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
        });
        p.spawn((Node { position_type: PositionType::Absolute, bottom: Val::Px(if compact { 6.0 } else { 14.0 }), left: Val::Px(0.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },)).with_children(|c| {
            c.spawn((HudText::Hint, Text::new(""), font(&a.body, if compact { 11.0 } else { 13.0 }), TextColor(Color::srgba(1.0, 0.96, 0.86, 0.75)), TextShadow { offset: Vec2::new(0.0, 1.5), color: NAVY }));
        });
    });
    root
}

#[allow(clippy::too_many_arguments)]
fn manage_ui(mut commands: Commands, bridge: Res<BrawlBridge>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, tc: Res<TouchControls>, mut ui: ResMut<UiState>) {
    let Some(a) = assets else { return };
    let key = (bridge.hud.n, tc.visible, area.left as i32 + area.right as i32 * 7 + area.top as i32 * 13);
    if bridge.hud.n == 0 && ui.hud.is_some() {
        if let Some(e) = ui.hud.take() {
            commands.entity(e).despawn();
        }
    }
    if bridge.hud.n > 0 && (ui.hud.is_none() || ui.hud_key != key) {
        if let Some(e) = ui.hud.take() {
            commands.entity(e).despawn();
        }
        ui.hud = Some(spawn_hud(&mut commands, &a, &area, bridge.hud.n, tc.visible));
        ui.hud_key = key;
        ui.texts.clear();
    }
    // The always-available HELP button (top right, under the pause button) and the card itself.
    let want_btn = bridge.hud.n > 0 && bridge.screen == Screen::None && !bridge.help;
    if want_btn && ui.help_btn.is_none() {
        ui.help_btn = Some(spawn_help_button(&mut commands, &a, &area));
    } else if !want_btn {
        if let Some(e) = ui.help_btn.take() {
            commands.entity(e).despawn();
        }
    }
    if bridge.help && ui.help.is_none() {
        ui.help = Some(spawn_help(&mut commands, &a, &area, tc.visible, bridge.menu.hero));
    } else if !bridge.help {
        if let Some(e) = ui.help.take() {
            commands.entity(e).despawn();
        }
    }
    // Full-screen layers (menu / summary) are rebuilt when their content changes.
    let sig: u64 = match bridge.screen {
        Screen::None => 0,
        Screen::Menu => 1 + bridge.menu.mode as u64 * 10 + bridge.menu.arena as u64 * 100 + bridge.menu.bots as u64 * 1000 + bridge.menu.diff as u64 * 10000 + bridge.menu.monsters as u64 * 50000 + bridge.menu.cursor as u64 * 100000 + bridge.menu.hero as u64 * 1_000_000_000 + key.2 as u64 * 10_000_000,
        Screen::Summary => 2 + bridge.summary.lines.iter().map(|l| l.len() as u64).sum::<u64>() * 7 + bridge.summary.title.len() as u64 * 131 + bridge.summary.win as u64 * 17 + key.2 as u64 * 10_000_000,
    };
    let have = ui.screen.map(|(_, s, g)| (s, g));
    if have != Some((bridge.screen, sig)) && !(bridge.screen == Screen::None && ui.screen.is_none()) {
        if let Some((e, ..)) = ui.screen.take() {
            commands.entity(e).despawn();
        }
        let e = match bridge.screen {
            Screen::None => None,
            Screen::Menu => Some(spawn_menu(&mut commands, &a, &area, &bridge.menu, tc.visible)),
            Screen::Summary => Some(spawn_summary(&mut commands, &a, &area, &bridge.summary)),
        };
        ui.screen = e.map(|e| (e, bridge.screen, sig));
    }
}

fn set_text(t: &mut Text, s: &str) {
    if t.0 != s {
        t.0 = s.to_string();
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    bridge: Res<BrawlBridge>,
    time: Res<Time<Real>>,
    mut anim: Local<HudAnim>,
    mut texts: Query<(&HudText, &mut Text, Option<&mut TextColor>, &mut UiTransform), Without<CardRoot>>,
    mut fills: Query<(&HudFill, &mut Node, &mut BackgroundColor), Without<GemDot>>,
    mut dots: Query<(&GemDot, &mut BackgroundColor), Without<HudFill>>,
    mut cards: Query<(&CardRoot, &mut Node, &mut BorderColor, &mut UiTransform), (Without<HudFill>, Without<GemDot>)>,
) {
    let h = &bridge.hud;
    let dt = time.delta_secs().min(0.1);
    for i in 0..MAXF {
        let c = &h.cards[i];
        anim.ghost[i] = if c.on { ghost_step(anim.ghost[i], c.hp, c.hit, dt) } else { 0.0 };
    }
    if anim.banner != h.banner {
        anim.banner.clone_from(&h.banner);
        anim.banner_t = 0.0;
    }
    anim.banner_t += dt;
    for (kind, mut t, color, mut xf) in &mut texts {
        match *kind {
            HudText::Name(i) => {
                let c = &h.cards[i as usize % MAXF];
                let s = format!("{}{}", c.name, c.tag);
                set_text(&mut t, &s);
                if let Some(mut col) = color {
                    let want = if c.ko { Color::srgba(0.6, 0.6, 0.65, 0.8) } else { slot_color(i as usize) };
                    if col.0 != want {
                        col.0 = want;
                    }
                }
            }
            HudText::Wins(i) => {
                let c = &h.cards[i as usize % MAXF];
                let s: String = (0..c.to_win).map(|k| if k < c.wins { 'O' } else { 'o' }).collect::<Vec<_>>().iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
                set_text(&mut t, &s);
            }
            HudText::Round => set_text(&mut t, &h.round),
            HudText::Clock => set_text(&mut t, &h.clock),
            HudText::Banner => {
                set_text(&mut t, &h.banner);
                // A new banner punches in: big, then settles with a small overshoot.
                let s = 1.0 + 0.9 * (1.0 - super::feel::ease_out_back(anim.banner_t / 0.22));
                let want = Vec2::splat(s);
                if xf.scale != want {
                    xf.scale = want;
                }
                if let Some(mut col) = color {
                    let want = Color::srgba(1.0, 0.95, 0.65, h.banner_alpha);
                    if col.0 != want {
                        col.0 = want;
                    }
                }
            }
            HudText::Sub => {
                set_text(&mut t, &h.sub);
                if let Some(mut col) = color {
                    let want = Color::srgba(1.0, 0.96, 0.86, h.banner_alpha);
                    if col.0 != want {
                        col.0 = want;
                    }
                }
            }
            HudText::Warn => set_text(&mut t, &h.warn),
            HudText::Hint => set_text(&mut t, &h.hint),
        }
    }
    for (fill, mut node, mut bg) in &mut fills {
        match *fill {
            HudFill::Hp(i) => {
                let c = &h.cards[i as usize % MAXF];
                let w = Val::Percent((c.hp * 100.0).clamp(0.0, 100.0));
                if node.width != w {
                    node.width = w;
                }
                let col = if c.hp > 0.55 { hex(0x58d26a) } else if c.hp > 0.25 { hex(0xf0c040) } else { hex(0xe5302a) };
                if bg.0 != col {
                    bg.0 = col;
                }
            }
            HudFill::Ghost(i) => {
                let w = Val::Percent((anim.ghost[i as usize % MAXF] * 100.0).clamp(0.0, 100.0));
                if node.width != w {
                    node.width = w;
                }
            }
            HudFill::Power(i) => {
                let c = &h.cards[i as usize % MAXF];
                let w = Val::Percent((c.power * 100.0).clamp(0.0, 100.0));
                if node.width != w {
                    node.width = w;
                }
            }
        }
    }
    for (GemDot(i, g), mut bg) in &mut dots {
        let c = &h.cards[*i as usize % MAXF];
        let col = super::brawl_core::GEM_COLORS[*g as usize];
        let have = c.gems & (1 << g) != 0;
        let want = Color::srgba(col[0], col[1], col[2], if have { 1.0 } else { 0.18 });
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (CardRoot(i), mut node, mut border, mut xf) in &mut cards {
        let c = &h.cards[*i as usize % MAXF];
        let d = if c.on { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
        let base = if c.power > 0.0 { GOLD } else { slot_color(*i as usize) };
        // A hit flashes the card's border white and knocks the card about for a moment.
        let want = if c.hit > 0.02 { base.mix(&Color::WHITE, c.hit.min(1.0)) } else { base };
        if border.top != want {
            *border = BorderColor::all(want);
        }
        let (off, sc) = if c.hit > 0.02 { (Val2::px((bridge.time * 55.0 + *i as f32).sin() * 5.0 * c.hit, c.hit * 3.0), Vec2::splat(1.0 + 0.05 * c.hit)) } else { (Val2::px(0.0, 0.0), Vec2::ONE) };
        if xf.translation != off || xf.scale != sc {
            xf.translation = off;
            xf.scale = sc;
        }
    }
}

fn spawn_menu(commands: &mut Commands, a: &UiAssets, area: &SafeArea, m: &MenuView, touch: bool) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(26), node, BackgroundColor(Color::srgba(0.02, 0.0, 0.1, 0.5)))).id();
    let compact = touch;
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(26.0), Val::Px(if compact { 8.0 } else { 14.0 })), row_gap: Val::Px(if compact { 4.0 } else { 7.0 }), width: Val::Px(if compact { 560.0 } else { 600.0 }), align_items: AlignItems::Center, ..default() }).with_children(|b| {
            kicker(b, a, "ARENA BRAWL");
            outlined(b, a, "GEM CLASH", if compact { 30.0 } else { 38.0 }, GOLD);
            b.spawn(txt(a, "Grab anything, snatch all three gems, go giant, and be the last one standing.", if compact { 10.0 } else { 11.0 }, CREAM));
            let rows: [(&str, String, bool); 5] = [
                ("MODE", ["VERSUS", "GAUNTLET"][m.mode as usize % 2].to_string(), true),
                (if m.mode == 1 { "WORLD" } else { "ARENA" }, super::brawl_region::arena_name(m.arena).to_string(), true),
                ("OPPONENTS", if m.mode == 1 { "escalating + boss".into() } else { format!("{}", m.bots) }, m.mode == 0),
                ("BOT LEVEL", ["EASY", "NORMAL", "HARD"][m.diff as usize % 3].to_string(), m.mode == 0),
                ("MONSTERS", super::brawl_core::Monsters::from_index(m.monsters).label().to_string(), true),
            ];
            for (r, (name, value, enabled)) in rows.iter().enumerate() {
                let sel = m.cursor as usize == r;
                b.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
                    let mut l = button(row, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(34.0), Val::Px(if compact { 28.0 } else { 34.0 }), 16.0), "<");
                    l.insert(Act::Arrow(r as u8, -1));
                    let mut mid = button(row, a, ButtonSpec::new(if sel { ButtonKind::Gold } else { ButtonKind::Blue }, Val::Px(430.0), Val::Px(if compact { 28.0 } else { 34.0 }), if compact { 13.0 } else { 15.0 }), &format!("{name}:  {value}"));
                    mid.insert(Act::Row(r as u8));
                    if !enabled {
                        mid.insert(Disabled);
                    }
                    let mut rr = button(row, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(34.0), Val::Px(if compact { 28.0 } else { 34.0 }), 16.0), ">");
                    rr.insert(Act::Arrow(r as u8, 1));
                });
            }
            b.spawn(txt(a, &format!("PLAYING AS {}  -  change in the main menu (PLAY AS) or press V", super::brawl_core::HEROES[m.hero as usize % 4].name().to_uppercase()), if compact { 10.0 } else { 12.0 }, GOLD));
            let mode = m.mode as usize % 2;
            let blurb = if mode == 0 { super::brawl_region::arena_blurb(m.arena) } else { "Six arenas, six harder rounds, then King Bob-omb. Three lives." };
            b.spawn(txt(a, blurb, if compact { 10.0 } else { 11.0 }, GOLD_HI));
            b.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|r| {
                let mut s = button(r, a, ButtonSpec::new(ButtonKind::Green, Val::Px(190.0), Val::Px(if compact { 36.0 } else { 44.0 }), 19.0), "FIGHT!");
                s.insert(Act::Start);
                if m.cursor == 5 {
                    s.insert(Selected);
                }
                let mut hh = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(150.0), Val::Px(if compact { 36.0 } else { 44.0 }), 13.0), "HOW TO PLAY");
                hh.insert(Act::Help);
                let mut mm = button(r, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(90.0), Val::Px(if compact { 36.0 } else { 44.0 }), 14.0), "BACK");
                mm.insert(Act::Menu);
            });
            let hint = if touch { "A jump - B hit - Z grab / throw - SHOCK WAVE and SEEK BEAM unlock in power form (3 gems)" } else { "Space jump - Shift hit - Ctrl grab/throw - G shock wave, H seek beam (power form) - F1 how to play" };
            crate::hints::note(hint);
            b.spawn((crate::hints::ControlHint, txt(a, hint, if compact { 10.0 } else { 11.0 }, hex(0x8f9be0))));
        });
    });
    root
}

/// The small "?" under the pause button: opens the HOW TO PLAY card any time during a match.
fn spawn_help_button(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let a = a.hud(); // in-game heads-up element: keeps the pre-v3 look
    let _ = area;
    let top = crate::touch::MARGIN_Y + crate::touch::PAUSE_TOP + crate::touch::PAUSE_SIZE + 8.0;
    commands
        .spawn((
            XpEntity,
            Overlay,
            Act::Help,
            Button,
            crate::hints::ControlHint,
            GlobalZIndex(12),
            Node { position_type: PositionType::Absolute, right: Val::Px(crate::touch::MARGIN_X + 3.0), top: Val::Px(top), width: Val::Px(34.0), height: Val::Px(34.0), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.09, 0.14, 0.23, 0.74)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.6)),
            BorderRadius::MAX,
        ))
        .with_child((Text::new("?"), font(&a.display, 20.0), TextColor(GOLD_HI)))
        .id()
}

/// HOW TO PLAY: every control and the two power-form moves, in plain words.
fn spawn_help(commands: &mut Commands, a: &UiAssets, area: &SafeArea, touch: bool, hero: u8) -> Entity {
    let hero = super::brawl_core::HEROES[hero as usize % 4];
    let (hit, air, pound) = hero.move_names();
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(30), node, BackgroundColor(Color::srgba(0.01, 0.0, 0.08, 0.72)))).id();
    let compact = touch;
    // (chip, chip colour, heading, text)
    let key = |t: &'static str, k: &'static str| if touch { t } else { k };
    let rows: [(&str, Color, &str, String); 8] = [
        (key("STICK", "WASD"), hex(0x3a4a8a), "MOVE", "Run around the arena. Edges and lava are real: watch your feet.".into()),
        (key("A", "SPACE"), hex(0xd23a2a), "JUMP", "Tap again in the air for a double jump.".into()),
        (key("B", "SHIFT"), hex(0x2f6fd0), "HIT", format!("{}. In the air: {}. Holding an item, it throws or uses it.", hit, air)),
        (key("Z / L", "CTRL / F"), hex(0x2f8a4a), "GRAB AND THROW", format!("Pick up crates, bombs, hammers and more; press again to throw. In the air, Z is {}.", pound)),
        ("3 GEMS", hex(0xb8860b), "GEMS = POWER FORM", "Three gems fall each round. Hold all three to go GIANT for 14 seconds. A hard hit makes you drop them!".into()),
        (key("SHOCK WAVE", "G"), hex(0xd9731a), "SHOCK WAVE", "Power form only. A blast all around you that damages and flings every fighter nearby.".into()),
        (key("SEEK BEAM", "H"), hex(0x3c78d8), "SEEK BEAM", "Power form only. A homing beam that zaps the closest foe from across the arena.".into()),
        ("WIN", hex(0x2f8a4a), "HOW YOU WIN", "Empty your rivals' health bars to KO them. Win two rounds to take the match.".into()),
    ];
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(22.0), Val::Px(if compact { 8.0 } else { 14.0 })), row_gap: Val::Px(if compact { 3.0 } else { 6.0 }), width: Val::Px(if compact { 700.0 } else { 820.0 }), align_items: AlignItems::Center, ..default() }).with_children(|b| {
            outlined(b, a, "HOW TO PLAY", if compact { 26.0 } else { 34.0 }, GOLD);
            b.spawn(Node { column_gap: Val::Px(18.0), width: Val::Percent(100.0), align_items: AlignItems::FlexStart, ..default() }).with_children(|cols| {
                for half in rows.chunks(4) {
                    cols.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(if compact { 4.0 } else { 8.0 }), flex_grow: 1.0, flex_basis: Val::Px(0.0), ..default() }).with_children(|col| {
                        for (chip, color, head, text) in half {
                            col.spawn(Node { column_gap: Val::Px(8.0), align_items: AlignItems::FlexStart, ..default() }).with_children(|row| {
                                row.spawn((Node { width: Val::Px(if compact { 58.0 } else { 70.0 }), min_height: Val::Px(if compact { 24.0 } else { 28.0 }), padding: UiRect::axes(Val::Px(3.0), Val::Px(2.0)), flex_shrink: 0.0, justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(*color), BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.7)), BorderRadius::all(Val::Px(9.0))))
                                    .with_child((Text::new(clean((*chip).to_string())), font(&a.body, if compact { 10.0 } else { 12.0 }), TextColor(Color::WHITE), TextLayout::justify(Justify::Center)));
                                row.spawn(Node { flex_direction: FlexDirection::Column, flex_shrink: 1.0, ..default() }).with_children(|t| {
                                    t.spawn(txt(a, head, if compact { 11.0 } else { 13.0 }, GOLD_HI));
                                    t.spawn(txt(a, text, if compact { 10.0 } else { 12.0 }, CREAM));
                                });
                            });
                        }
                    });
                }
            });
            let mut x = button(b, a, ButtonSpec::new(ButtonKind::Green, Val::Px(190.0), Val::Px(if compact { 32.0 } else { 40.0 }), 17.0), "GOT IT!");
            x.insert(Act::CloseHelp);
        });
    });
    root
}

fn spawn_summary(commands: &mut Commands, a: &UiAssets, area: &SafeArea, s: &SummaryView) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let tint = if s.win { Color::srgba(0.08, 0.06, 0.0, 0.8) } else { Color::srgba(0.06, 0.0, 0.04, 0.8) };
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(26), node, BackgroundColor(tint))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(12.0)), row_gap: Val::Px(5.0), width: Val::Px(540.0), align_items: AlignItems::Center, ..default() }).with_children(|b| {
            kicker(b, a, if s.win { "VICTORY" } else { "DEFEAT" });
            outlined(b, a, &s.title, 32.0, if s.win { GOLD } else { RED_HI });
            b.spawn(txt(a, &s.sub, 14.0, CREAM));
            for l in &s.lines {
                b.spawn(txt(a, l, 13.0, GOLD_HI));
            }
            b.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(8.0)), ..default() }).with_children(|r| {
                if s.can_next {
                    let mut x = button(r, a, ButtonSpec::new(ButtonKind::Green, Val::Px(180.0), Val::Px(42.0), 18.0), "NEXT STAGE");
                    x.insert(Act::Next);
                } else {
                    let mut x = button(r, a, ButtonSpec::new(ButtonKind::Green, Val::Px(180.0), Val::Px(42.0), 18.0), "REMATCH");
                    x.insert(Act::Rematch);
                }
                let mut m = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(130.0), Val::Px(42.0), 16.0), "SETUP");
                m.insert(Act::Menu);
            });
        });
    });
    root
}

fn ui_buttons(mut bridge: ResMut<BrawlBridge>, q: Query<(&Interaction, &Act, Has<Disabled>), Changed<Interaction>>, state: Res<AppState>, mut leave: MessageWriter<ReturnToMenu>) {
    if *state != AppState::Playing {
        return;
    }
    for (i, act, disabled) in &q {
        if *i != Interaction::Pressed {
            continue;
        }
        match *act {
            Act::Row(r) if !disabled && !bridge.help => bridge.cmds.push(UiCmd::Adjust(r, 1)),
            Act::Arrow(r, d) => bridge.cmds.push(UiCmd::Adjust(r, d)),
            Act::Start => bridge.cmds.push(UiCmd::Start),
            Act::Rematch => bridge.cmds.push(UiCmd::Rematch),
            Act::Next => bridge.cmds.push(UiCmd::Next),
            Act::Menu if bridge.screen == Screen::Menu => {
                leave.write(ReturnToMenu);
            }
            Act::Menu => bridge.cmds.push(UiCmd::Menu),
            Act::Help => bridge.cmds.push(UiCmd::Help),
            Act::CloseHelp => bridge.cmds.push(UiCmd::CloseHelp),
            _ => {}
        }
    }
}

/// Menus (pause, settings) draw over the game; hide ours meanwhile so it cannot take their taps.
fn overlay_display(state: Res<AppState>, mut roots: Query<&mut Visibility, With<Overlay>>) {
    let v = if *state == AppState::Playing { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in &mut roots {
        if *vis != v {
            *vis = v;
        }
    }
}

/// Gem Clash plays whoever the player picked (the shell's PLAY AS row, `V`): the bridge carries that character into
/// the next match. The choice is read at the start of a match, so a switch mid-round shows from the next one.
fn pick_hero(character: Res<Character>, mut bridge: ResMut<BrawlBridge>) {
    let h = Hero::from_character(*character);
    if bridge.hero != h {
        bridge.hero = h;
    }
}

/// The power form makes the hero big (and fits each rig to the fighter's body): scale the root after the game placed it.
fn mario_scale(bridge: Res<BrawlBridge>, mut mario: Query<&mut Transform, Or<(With<crate::render::mario_view::MarioRoot>, With<crate::characters::CharacterRoot>)>>) {
    if !bridge.active {
        return;
    }
    let k = bridge.fighters[0].scale;
    if k > 0.0 {
        for mut t in &mut mario {
            t.scale *= k;
        }
    }
}

#[cfg(test)]
mod pad_tests {
    use super::*;
    use crate::last_input::testing::*;

    #[test]
    fn the_setup_screen_is_navigated_and_left_with_the_pad() {
        let (mut app, pad) = base_app();
        app.init_resource::<BrawlBridge>().add_systems(Update, menu_keys);
        app.world_mut().resource_mut::<BrawlBridge>().screen = Screen::Menu;
        tap_update(&mut app, pad, GamepadButton::DPadDown);
        assert!(matches!(app.world().resource::<BrawlBridge>().cmds.last(), Some(UiCmd::Cursor(1))));
        tap_update(&mut app, pad, GamepadButton::DPadRight);
        assert!(matches!(app.world().resource::<BrawlBridge>().cmds.last(), Some(UiCmd::Adjust(_, 1))));
        tap_update(&mut app, pad, GamepadButton::Select);
        assert!(matches!(app.world().resource::<BrawlBridge>().cmds.last(), Some(UiCmd::Help)), "Select opens HOW TO PLAY");
        app.world_mut().resource_mut::<BrawlBridge>().cmds.clear();
        tap_update(&mut app, pad, GamepadButton::East);
        assert!(left_to_hub(&mut app), "B backs out to the hub");
    }
}

#[cfg(test)]
mod feel_tests {
    use super::*;

    #[test]
    fn effects_fade_out_and_keep_their_tint() {
        for kind in [FxKind::Ring, FxKind::Flash, FxKind::Beam] {
            let (a0, a1) = (fx_color(kind, [0.2, 0.4, 0.6], 0.0).to_srgba(), fx_color(kind, [0.2, 0.4, 0.6], 1.0).to_srgba());
            assert!(a0.alpha > 0.8 && a1.alpha < 0.01, "{kind:?}");
            assert!((a0.red - 0.2).abs() < 1e-4 && (a0.blue - 0.6).abs() < 1e-4);
            // No tint given: the kind's own colour, never black.
            assert!(fx_color(kind, [0.0; 3], 0.0).to_srgba().red > 0.9);
        }
        // A flash holds before it drops.
        assert!(fx_color(FxKind::Flash, [1.0; 3], 0.3).to_srgba().alpha > 0.84);
    }

    #[test]
    fn ghost_bar_waits_out_the_hit_then_drains_to_the_health() {
        assert_eq!(ghost_step(0.5, 0.8, 0.0, 0.016), 0.8);
        assert_eq!(ghost_step(0.8, 0.5, 1.0, 0.016), 0.8);
        let mut g = 0.8;
        for _ in 0..200 {
            let n = ghost_step(g, 0.5, 0.0, 0.016);
            assert!(n <= g && n >= 0.5);
            g = n;
        }
        assert_eq!(g, 0.5);
    }
}
