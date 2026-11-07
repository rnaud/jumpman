//! The Painted Halls presentation, world half: the [`HallsBridge`] resource connecting the
//! experiment (`halls.rs`, which owns the rules glue and the room entities) to bevy systems,
//! plus the pooled foe / boss / hazard / drop models and the input gathering (touch buttons,
//! gamepad). The HUD, map screen and prompts live in `halls_ui.rs`.
//!
//! Foe and boss models are SM64 actors with animation clips and are spawned through
//! `render::object_view::spawn_model` (never the bare scene), pooled per slot and only moved or
//! hidden while a room lasts, so a kill costs no scene instantiation.
use super::halls_core::{FOE_SLOTS, HAZARD_SLOTS, DROP_SLOTS, MapRoom};
use super::halls_world::Ability;
use super::XpEntity;
use crate::characters::Character;
use crate::render::{AssetRoot, SCALE};
use crate::touch::{ModeButtons, TouchControls};
use bevy::prelude::*;

/// Model per foe kind (index = kind + 1; 0 unused) and the two bosses after them.
pub const MODELS: [&str; 9] = ["", "actors/goomba/goomba_geo.glb", "actors/bobomb/black_bobomb_geo.glb", "actors/spiny/spiny_geo.glb", "actors/boo/boo_geo.glb", "actors/bully/bully_geo.glb", "actors/piranha_plant/piranha_plant_geo.glb", "actors/whomp/whomp_geo.glb", "actors/king_bobomb/king_bobomb_geo.glb"];
pub const HEART_MODEL: &str = "actors/heart/heart_geo.glb";

/// Which full-screen layer is up over the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    #[default]
    None,
    Map,
    /// "New game / Continue" at the start.
    Prompt,
    Dead,
    Win,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UiCmd {
    NewGame,
    Continue,
    OpenMap,
    CloseMap,
    CycleCap,
    KeepPlaying,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FoeView {
    /// 0 hidden, else `MODELS` index.
    pub kind: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub scale: f32,
    pub tilt: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HazardView {
    pub on: bool,
    pub pos: Vec3,
    pub radius: f32,
    pub frac: f32,
    pub bomb: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BossHud {
    pub frac: f32,
    pub phase: u8,
    pub name: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hud {
    pub hp: i32,
    pub max_hp: i32,
    pub fragments: u32,
    pub total_fragments: u32,
    pub abilities: u8,
    pub cap: Option<Ability>,
    pub flaps: u8,
    pub has_wing: bool,
    pub boss: Option<BossHud>,
    pub region: &'static str,
    pub room: &'static str,
    pub keys: u8,
    pub low_hp: bool,
    pub hurt_flash: f32,
    pub god: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WinCard {
    pub fragments: u32,
    pub total: u32,
    pub secs: u64,
    pub deaths: u32,
    pub abilities: u32,
    pub rooms: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptCard {
    pub fragments: u32,
    pub rooms: u32,
    pub abilities: u32,
    pub secs: u64,
    pub has_save: bool,
}

#[derive(Resource, Default)]
pub struct HallsBridge {
    pub active: bool,
    pub hud: Hud,
    pub screen: Screen,
    pub cmds: Vec<UiCmd>,
    /// Button edges from touch / gamepad (bit 0 map, bit 1 cap).
    pub edges: u8,
    pub foes: Vec<FoeView>,
    pub boss: FoeView,
    pub hazards: Vec<HazardView>,
    pub drops: Vec<FoeView>,
    pub map: Vec<MapRoom>,
    pub map_gen: u32,
    pub here: usize,
    /// Room banner text and its age in seconds (None = hidden).
    pub banner: Option<(String, f32)>,
    /// Black fade, 0 clear .. 1 black.
    pub fade: f32,
    pub win: WinCard,
    pub prompt: PromptCard,
    pub time: f32,
    /// A one-line hint at the bottom (what a gate wants, a locked door...).
    pub hint: Option<(String, f32)>,
}

impl HallsBridge {
    pub fn reset(&mut self) {
        self.foes = vec![FoeView::default(); FOE_SLOTS];
        self.hazards = vec![HazardView::default(); HAZARD_SLOTS];
        self.drops = vec![FoeView::default(); DROP_SLOTS];
        self.boss = FoeView::default();
        self.cmds.clear();
        self.edges = 0;
        self.screen = Screen::None;
        self.banner = None;
        self.fade = 0.0;
        self.hint = None;
    }
}

pub struct HallsViewPlugin;

impl Plugin for HallsViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HallsBridge>().init_resource::<Pools>().init_resource::<StashedCharacter>();
        app.add_systems(Update, gather_input.before(super::run_frame).run_if(|b: Res<HallsBridge>| b.active));
        app.add_systems(Update, (sync_render, drive_touch, force_mario).chain().after(super::run_frame).run_if(|b: Res<HallsBridge>| b.active));
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<HallsBridge>| !b.active));
        app.add_plugins(super::halls_ui::HallsUiPlugin);
    }
}

#[derive(Resource, Default)]
struct StashedCharacter(Option<Character>);

/// The Halls are built around Mario's moves (long jump, wall kick, ground pound, caps).
fn force_mario(mut character: ResMut<Character>, mut stash: ResMut<StashedCharacter>) {
    if *character != Character::Mario {
        stash.0.get_or_insert(*character);
        *character = Character::Mario;
    }
}

fn gather_input(mut bridge: ResMut<HallsBridge>, mut tc: ResMut<TouchControls>, pads: Query<&Gamepad>) {
    let bits = std::mem::take(&mut tc.mode_pressed);
    bridge.edges |= bits & 0b11;
    for pad in &pads {
        if pad.just_pressed(GamepadButton::Select) {
            bridge.edges |= 1;
        }
        if pad.just_pressed(GamepadButton::RightTrigger) || pad.just_pressed(GamepadButton::North) {
            bridge.edges |= 2;
        }
    }
}

/// Mode buttons: MAP and CAP; touches are swallowed under a full-screen layer.
fn drive_touch(bridge: Res<HallsBridge>, mut tc: ResMut<TouchControls>, mut mode: ResMut<ModeButtons>) {
    let menu = bridge.screen != Screen::None;
    if tc.suppress != menu {
        tc.suppress = menu;
    }
    if !mode.active {
        mode.active = true;
    }
    let cap = bridge.hud.cap;
    let have_caps = bridge.hud.abilities & (Ability::WingCap.bit() | Ability::MetalCap.bit() | Ability::VanishCap.bit()) != 0;
    let map = Some(Color::srgb(0.2, 0.3, 0.7));
    let cap_tint = match cap {
        Some(a) => {
            let c = a.color();
            Some(Color::srgb(c[0] as f32 / 255.0 * 0.8, c[1] as f32 / 255.0 * 0.8, c[2] as f32 / 255.0 * 0.8))
        }
        None if have_caps => Some(Color::srgb(0.35, 0.35, 0.42)),
        None => None,
    };
    let labels = ["MAP".to_string(), match cap {
        Some(Ability::WingCap) => "WING".to_string(),
        Some(Ability::MetalCap) => "METAL".to_string(),
        Some(Ability::VanishCap) => "VANISH".to_string(),
        _ => "CAP".to_string(),
    }, String::new()];
    if mode.labels != labels {
        mode.labels = labels;
    }
    if mode.tint[0] != map {
        mode.tint[0] = map;
    }
    if mode.tint[1] != cap_tint {
        mode.tint[1] = cap_tint;
    }
    if mode.tint[2].is_some() {
        mode.tint[2] = None;
    }
}

// ---------------------------------------------------------------------------------------------
// World sync
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct Pooled;

#[derive(Resource, Default)]
struct Pools {
    ready: bool,
    foe: Vec<Option<(u8, Entity)>>,
    free: [Vec<Entity>; 9],
    boss: Option<(u8, Entity)>,
    fill: Vec<Entity>,
    edge: Vec<Entity>,
    bomb: Vec<Entity>,
    drop: Vec<Option<Entity>>,
    disc: Handle<Mesh>,
    ring: Handle<Mesh>,
    sphere: Handle<Mesh>,
    fill_mat: Handle<StandardMaterial>,
    edge_mat: Handle<StandardMaterial>,
    bomb_mat: Handle<StandardMaterial>,
}

fn glow(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })
}

fn place(q: &mut Query<(&mut Transform, &mut Visibility), With<Pooled>>, e: Entity, show: bool, pos: Vec3, rot: Quat, scale: Vec3) {
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

#[allow(clippy::too_many_arguments)]
fn sync_render(
    mut minimal: ResMut<crate::hud::HudMinimal>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<HallsBridge>,
    mut pools: ResMut<Pools>,
    mut q: Query<(&mut Transform, &mut Visibility), With<Pooled>>,
) {
    if !minimal.0 {
        minimal.0 = true;
    }
    let hidden = |commands: &mut Commands, bundle: (Mesh3d, MeshMaterial3d<StandardMaterial>)| commands.spawn((XpEntity, Pooled, bundle, Transform::default(), Visibility::Hidden)).id();
    if !pools.ready {
        pools.ready = true;
        pools.foe = vec![None; FOE_SLOTS];
        pools.drop = vec![None; DROP_SLOTS];
        pools.disc = meshes.add(Circle::new(1.0));
        pools.ring = meshes.add(Annulus::new(0.9, 1.0));
        pools.sphere = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
        pools.fill_mat = glow(&mut materials, Color::srgba(1.0, 0.18, 0.12, 0.38));
        pools.edge_mat = glow(&mut materials, Color::srgba(1.0, 0.45, 0.25, 0.85));
        pools.bomb_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.08, 0.1), perceptual_roughness: 0.4, ..default() });
        for _ in 0..HAZARD_SLOTS {
            let (d, r, s) = (pools.disc.clone(), pools.ring.clone(), pools.sphere.clone());
            let (fm, em, bm) = (pools.fill_mat.clone(), pools.edge_mat.clone(), pools.bomb_mat.clone());
            let f = hidden(&mut commands, (Mesh3d(d), MeshMaterial3d(fm)));
            let e = hidden(&mut commands, (Mesh3d(r), MeshMaterial3d(em)));
            let b = hidden(&mut commands, (Mesh3d(s), MeshMaterial3d(bm)));
            pools.fill.push(f);
            pools.edge.push(e);
            pools.bomb.push(b);
        }
    }
    let pools = &mut *pools;
    let mut spawn_foe = |commands: &mut Commands, kind: u8, pools_free: &mut Vec<Entity>, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>| -> Entity {
        if let Some(e) = pools_free.pop() {
            return e;
        }
        let path = MODELS[kind as usize];
        if root.exists(path) {
            let mut foe = crate::render::object_view::spawn_model(commands, &assets, &root, path, true);
            foe.insert((XpEntity, Pooled, Visibility::Hidden));
            foe.id()
        } else {
            let mesh = meshes.add(Sphere::new(60.0 * SCALE).mesh().ico(1).unwrap());
            let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.8, 0.3, 0.3), ..default() });
            commands.spawn((XpEntity, Pooled, Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden)).id()
        }
    };
    for (i, item) in bridge.foes.iter().enumerate() {
        if let Some((k, e)) = pools.foe[i] {
            if k != item.kind {
                if let Ok((_, mut v)) = q.get_mut(e) {
                    *v = Visibility::Hidden;
                }
                pools.free[k as usize].push(e);
                pools.foe[i] = None;
            }
        }
        if item.kind == 0 {
            continue;
        }
        let e = match pools.foe[i] {
            Some((_, e)) => e,
            None => {
                let e = spawn_foe(&mut commands, item.kind, &mut pools.free[item.kind as usize], &mut meshes, &mut materials);
                pools.foe[i] = Some((item.kind, e));
                e
            }
        };
        place(&mut q, e, true, item.pos, Quat::from_rotation_y(item.yaw) * Quat::from_rotation_x(item.tilt), Vec3::splat(item.scale));
    }
    // Boss (one slot, model by kind).
    let b = bridge.boss;
    if let Some((k, e)) = pools.boss {
        if k != b.kind {
            if let Ok((_, mut v)) = q.get_mut(e) {
                *v = Visibility::Hidden;
            }
            pools.free[k as usize].push(e);
            pools.boss = None;
        }
    }
    if b.kind != 0 {
        let e = match pools.boss {
            Some((_, e)) => e,
            None => {
                let e = spawn_foe(&mut commands, b.kind, &mut pools.free[b.kind as usize], &mut meshes, &mut materials);
                pools.boss = Some((b.kind, e));
                e
            }
        };
        place(&mut q, e, true, b.pos, Quat::from_rotation_y(b.yaw) * Quat::from_rotation_x(b.tilt), Vec3::splat(b.scale));
    }
    // Hazards.
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for (i, h) in bridge.hazards.iter().enumerate() {
        let r = h.radius * SCALE;
        place(&mut q, pools.fill[i], h.on, h.pos + Vec3::Y * 0.06, flat, Vec3::splat((r * h.frac).max(0.01)));
        place(&mut q, pools.edge[i], h.on, h.pos + Vec3::Y * 0.08, flat, Vec3::splat(r.max(0.01)));
        place(&mut q, pools.bomb[i], h.on && h.bomb, h.pos + Vec3::Y * (0.5 + (1.0 - h.frac) * 3.0), Quat::IDENTITY, Vec3::splat(0.38 * (1.0 + 0.25 * (h.frac * 40.0).sin().abs())));
    }
    // Heart drops.
    for (i, d) in bridge.drops.iter().enumerate() {
        if d.kind == 0 && pools.drop[i].is_none() {
            continue;
        }
        let e = *pools.drop[i].get_or_insert_with(|| {
            let mut e = crate::render::object_view::spawn_model(&mut commands, &assets, &root, HEART_MODEL, true);
            e.insert((XpEntity, Pooled, Visibility::Hidden));
            e.id()
        });
        place(&mut q, e, d.kind != 0, d.pos, Quat::from_rotation_y(bridge.time * 2.5), Vec3::splat(d.scale));
    }
}

/// Entities are despawned by the framework on exit; forget the handles.
fn cleanup(mut pools: ResMut<Pools>, mut minimal: ResMut<crate::hud::HudMinimal>, mut tc: ResMut<TouchControls>, mut mode: ResMut<ModeButtons>, mut stash: ResMut<StashedCharacter>, mut character: ResMut<Character>) {
    if let Some(c) = stash.0.take() {
        *character = c;
    }
    if pools.ready {
        if minimal.0 {
            minimal.0 = false;
        }
        if tc.suppress {
            tc.suppress = false;
        }
        mode.active = false;
        *pools = Pools::default();
    }
}
