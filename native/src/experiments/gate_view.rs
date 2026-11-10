//! Hold the Gate presentation: the render sync (pooled foe and tower models, shared simple meshes
//! for shells, coins, beams and rings), the HUD, the build bar, the tower card, the wave preview,
//! the map picker and the end-of-run card, all on `bevy_ui`. The experiment (`gate.rs`) owns the
//! rules; it talks to this module through the [`GateBridge`] resource (render lists and HUD numbers
//! one way, [`Cmd`]s the other), so none of this needs the experiment host.
//!
//! Foes and towers are `WorldAssetRoot`s of the real SM64 actor models spawned with
//! `object_view::spawn_model`, which plays their animation clips (a bare scene leaves limbs in the
//! raw rest pose). Entities are pooled per slot and only hidden/moved, never despawned mid-run.
//! Taps on the playfield (touch or mouse) are ray-cast onto the ground and arrive as [`Cmd::Tap`].
use crate::button_marker::BorderRadius;
use super::gate_core::*;
use super::swarm_view::RItem;
use super::XpEntity;
use crate::camera::MainCamera;
use crate::render::{AssetRoot, SCALE};
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::{AppState, ReturnToMenu};
use crate::touch::{ModeButtons, TouchControls};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Far from every course object, so nothing in the hidden course reaches Mario.
pub const ORIGIN: [f64; 3] = [150000.0, 1000.0, 150000.0];

/// Actor model per foe kind (index = `EKind as u8`, 0 unused).
pub const FOE_MODELS: [&str; 11] = [
    "",
    "actors/goomba/goomba_geo.glb",
    "actors/bobomb/black_bobomb_geo.glb",
    "actors/bully/bully_geo.glb",
    "actors/spiny/spiny_geo.glb",
    "actors/boo/boo_geo.glb",
    "actors/lakitu_enemy/enemy_lakitu_geo.glb",
    "actors/koopa/koopa_with_shell_geo.glb",
    "actors/king_bobomb/king_bobomb_geo.glb",
    "actors/chain_chomp/chain_chomp_geo.glb",
    "actors/whomp/whomp_geo.glb",
];

/// Model, scale and lift of each tower kind (index = `TKind as u8`).
pub const TOWER_MODELS: [(&str, f32, f32); 6] = [
    ("actors/cannon_barrel/cannon_barrel_geo.glb", 0.9, 52.0),
    ("actors/thwomp/thwomp_geo.glb", 0.55, 50.0),
    ("actors/piranha_plant/piranha_plant_geo.glb", 0.9, 36.0),
    ("actors/amp/dAmpGeo.glb", 1.6, 90.0),
    ("actors/star/star_geo.glb", 1.1, 85.0),
    ("actors/coin/yellow_coin_geo.glb", 3.2, 100.0),
];

pub const TOWER_TINTS: [[f32; 3]; 6] = [[0.85, 0.45, 0.2], [0.55, 0.55, 0.65], [0.95, 0.3, 0.25], [0.55, 0.3, 0.85], [1.0, 0.85, 0.25], [0.3, 0.85, 0.4]];
pub const SHORT_NAMES: [&str; 6] = ["Bombard", "Slab", "Bloom", "Eye", "Prism", "Spout"];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Stage {
    #[default]
    MapSelect,
    Playing,
    Over,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Hud {
    pub coins: u32,
    pub lives: i32,
    pub max_lives: i32,
    pub wave: u32,
    pub waves: u32,
    /// Whole seconds until the next wave (0 while one is spawning).
    pub timer: u32,
    pub spawning: bool,
    pub alive: u32,
    pub speed: u8,
    pub paused: bool,
    pub can_call: bool,
    /// Preview shows the running wave (true) or the one to come (false), and which.
    pub preview_current: bool,
    pub preview_wave: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildView {
    pub cell: (i32, i32),
    /// Selected tower kind awaiting its second tap, if any.
    pub sel: Option<u8>,
    pub afford: [bool; 6],
    /// Why the cell cannot be built on (coins aside).
    pub blocked: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardView {
    pub kind: u8,
    pub level: u8,
    pub mode: u8,
    pub up_cost: Option<u32>,
    pub afford: bool,
    pub sell: u32,
    pub jam: bool,
    pub rally: bool,
    pub overdrive: bool,
    pub kills: u32,
    pub dealt: u32,
    pub range: f32,
    pub damage: f32,
    pub per_sec: f32,
    pub effect: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum MenuView {
    #[default]
    None,
    Build(BuildView),
    Card(CardView),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CursorView {
    pub show: bool,
    pub pos: Vec3,
    pub ok: bool,
    /// Range ring in SM64 units (0 = none) and its tint.
    pub range: f32,
    pub tint: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TowerItem {
    pub kind: u8,
    pub level: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub kick: f32,
    pub jam: bool,
    pub rally: bool,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BeamItem {
    pub on: bool,
    pub a: Vec3,
    pub b: Vec3,
    pub fade: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub won: bool,
    pub map: usize,
    pub waves_cleared: u32,
    pub kills: u32,
    pub coins: u32,
    pub best: u32,
    pub new_best: bool,
}

/// UI -> experiment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    PickMap(usize),
    PickType(u8),
    Confirm,
    Upgrade,
    Sell,
    CycleMode,
    Close,
    Speed,
    Pause,
    NextWave,
    Interact,
    Retry,
    OtherMap,
    /// A tap on the playfield, as an arena-plane point (SM64 units, relative to the centre).
    Tap([f32; 2]),
}

#[derive(Resource, Default)]
pub struct GateBridge {
    pub active: bool,
    pub stage: Stage,
    pub hud: Hud,
    pub menu: MenuView,
    pub ui_gen: u32,
    pub cursor: CursorView,
    pub cmds: Vec<Cmd>,
    pub summary: Option<Summary>,
    pub summary_ready: bool,
    pub best: [u32; 2],
    pub preview: Preview,
    pub enemies: Vec<RItem>,
    /// Ported OoT / BK units for `foes_view`.
    pub foes: Vec<super::foes_view::FoeItem>,
    pub towers: Vec<TowerItem>,
    pub projs: Vec<RItem>,
    pub pickups: Vec<RItem>,
    pub beams: Vec<BeamItem>,
    pub fx: Vec<RItem>,
    /// Gamepad focus into the open menu.
    pub focus: usize,
    /// Highlight the focused entry (set once a gamepad walks the menu, cleared by a tap or click).
    pub pad_focus: bool,
    /// The touch overlay is showing (the camera nudges the meadow left, clear of the buttons).
    pub touch: bool,
    /// Logical window size and safe-area insets `[w, h, left, right, top, bottom]` (0 until the first frame), for the camera framing.
    pub view: [f32; 6],
    pub time: f32,
    pub hint: bool,
}

impl GateBridge {
    /// Size the render lists for a fresh run.
    pub fn reset(&mut self) {
        fn fill<T: Clone + Default>(v: &mut Vec<T>, n: usize) {
            v.clear();
            v.resize(n, T::default());
        }
        self.foes.clear();
        fill(&mut self.enemies, ENEMY_SLOTS);
        fill(&mut self.towers, TOWER_SLOTS);
        fill(&mut self.projs, PROJ_SLOTS);
        fill(&mut self.pickups, PICKUP_SLOTS);
        fill(&mut self.beams, BEAM_SLOTS);
        fill(&mut self.fx, FX_SLOTS);
        self.menu = MenuView::None;
        self.ui_gen = self.ui_gen.wrapping_add(1);
        self.cursor = CursorView::default();
        self.cmds.clear();
        self.summary = None;
        self.summary_ready = false;
        self.focus = 0;
        self.hud = Hud::default();
    }
}

pub struct GateViewPlugin;

impl Plugin for GateViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GateBridge>().init_resource::<Pools>().init_resource::<UiState>().add_systems(
            Update,
            (sync_render, manage_ui, update_ui, gather_input, world_taps, overlay_display).chain().after(super::run_frame).run_if(|b: Res<GateBridge>| b.active),
        );
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<GateBridge>| !b.active));
        app.add_systems(Update, push_foes.after(super::run_frame).before(super::foes_view::sync_foes).run_if(|b: Res<GateBridge>| b.active));
    }
}

fn push_foes(bridge: Res<GateBridge>, mut board: ResMut<super::foes_view::FoeBoard>) {
    board.items.extend(bridge.foes.iter().copied());
}

// ---------------------------------------------------------------------------------------------
// World sync
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct Pooled;

struct Shared {
    shell: (Handle<Mesh>, Handle<StandardMaterial>),
    fire: (Handle<Mesh>, Handle<StandardMaterial>),
    coin: (Handle<Mesh>, Handle<StandardMaterial>),
    beam: (Handle<Mesh>, Handle<StandardMaterial>),
    ring: Handle<Mesh>,
    cell: (Handle<Mesh>, Handle<StandardMaterial>),
    range_mat: Handle<StandardMaterial>,
    bar_mesh: Handle<Mesh>,
    bar_bg: Handle<StandardMaterial>,
    bar_fill: Handle<StandardMaterial>,
    bar_boss: Handle<StandardMaterial>,
    pedestal: Handle<Mesh>,
    pedestal_mats: [Handle<StandardMaterial>; 6],
    pip: (Handle<Mesh>, Handle<StandardMaterial>),
    jam: (Handle<Mesh>, Handle<StandardMaterial>),
    rally: (Handle<Mesh>, Handle<StandardMaterial>),
    select: (Handle<Mesh>, Handle<StandardMaterial>),
}

struct TowerSlot {
    root: Entity,
    kind: u8,
    model: Entity,
    pips: [Entity; 3],
    jam: Entity,
    rally: Entity,
    select: Entity,
}

#[derive(Resource, Default)]
struct Pools {
    shared: Option<Shared>,
    enemy_slot: Vec<Option<(u8, Entity)>>,
    enemy_free: [Vec<Entity>; 11],
    bars: Vec<Option<(Entity, Entity)>>,
    towers: Vec<Option<TowerSlot>>,
    proj: Vec<Option<(Entity, u8)>>,
    pickup: Vec<Option<Entity>>,
    beam: Vec<Option<Entity>>,
    fx: Vec<Option<(Entity, Handle<StandardMaterial>)>>,
    cursor: Option<(Entity, Entity, Handle<StandardMaterial>)>,
}

fn unlit(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, ..default() })
}

fn unlit_blend(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })
}

fn build_shared(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Shared {
    let sphere = |meshes: &mut Assets<Mesh>, r: f32| meshes.add(Sphere::new(r * SCALE).mesh().ico(1).unwrap());
    let half = TILE * 0.46 * SCALE;
    Shared {
        shell: (sphere(meshes, 34.0), materials.add(StandardMaterial { base_color: Color::srgb(0.12, 0.12, 0.16), perceptual_roughness: 0.5, ..default() })),
        fire: (sphere(meshes, 26.0), unlit(materials, Color::srgb(1.0, 0.55, 0.12))),
        coin: (meshes.add(Cylinder::new(30.0 * SCALE, 8.0 * SCALE)), unlit(materials, Color::srgb(1.0, 0.85, 0.1))),
        beam: (meshes.add(Cuboid::new(1.0, 1.0, 1.0)), unlit_blend(materials, Color::srgba(1.0, 0.95, 0.5, 0.9))),
        ring: meshes.add(Annulus::new(0.94, 1.0)),
        cell: (meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(half))), unlit_blend(materials, Color::srgba(0.2, 1.0, 0.3, 0.45))),
        range_mat: unlit_blend(materials, Color::srgba(1.0, 1.0, 1.0, 0.8)),
        bar_mesh: meshes.add(Rectangle::new(1.0, 1.0)),
        bar_bg: unlit(materials, Color::srgb(0.05, 0.05, 0.1)),
        bar_fill: unlit(materials, Color::srgb(0.25, 0.9, 0.3)),
        bar_boss: unlit(materials, Color::srgb(0.95, 0.25, 0.25)),
        pedestal: meshes.add(Cylinder::new(86.0 * SCALE, 34.0 * SCALE)),
        pedestal_mats: std::array::from_fn(|i| materials.add(StandardMaterial { base_color: Color::srgb(TOWER_TINTS[i][0], TOWER_TINTS[i][1], TOWER_TINTS[i][2]), perceptual_roughness: 0.8, ..default() })),
        pip: (sphere(meshes, 12.0), unlit(materials, Color::srgb(1.0, 0.9, 0.2))),
        jam: (sphere(meshes, 30.0), unlit(materials, Color::srgb(0.9, 0.1, 0.1))),
        rally: (meshes.add(Annulus::new(0.9, 1.0)), unlit_blend(materials, Color::srgba(1.0, 0.9, 0.3, 0.85))),
        select: (meshes.add(Annulus::new(0.93, 1.0)), unlit_blend(materials, Color::srgba(1.0, 1.0, 1.0, 0.95))),
    }
}

/// Pooled entities are hidden/moved only; this applies one item to one entity.
fn place(q: &mut Query<(&mut Transform, &mut Visibility), With<Pooled>>, e: Entity, item: &RItem, rot: Quat) {
    let Ok((mut tf, mut vis)) = q.get_mut(e) else { return };
    let want = if item.kind == 0 { Visibility::Hidden } else { Visibility::Inherited };
    if *vis != want {
        *vis = want;
    }
    if item.kind != 0 {
        tf.translation = item.pos;
        tf.rotation = rot;
        tf.scale = Vec3::splat(item.scale);
    }
}

fn set_vis(q: &mut Query<(&mut Transform, &mut Visibility), With<Pooled>>, e: Entity, show: bool) {
    if let Ok((_, mut v)) = q.get_mut(e) {
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

fn spawn_tower(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, shared: &Shared, kind: u8) -> TowerSlot {
    let k = (kind - 1) as usize;
    let rootent = commands.spawn((XpEntity, Pooled, Transform::default(), Visibility::Hidden)).id();
    let ped = commands.spawn((Mesh3d(shared.pedestal.clone()), MeshMaterial3d(shared.pedestal_mats[k].clone()), Transform::from_xyz(0.0, 17.0 * SCALE, 0.0))).id();
    let (path, scale, lift) = TOWER_MODELS[k];
    let model = if root.exists(path) {
        let mut m = crate::render::object_view::spawn_model(commands, assets, root, path, true);
        m.insert(Transform::from_xyz(0.0, lift * SCALE, 0.0).with_scale(Vec3::splat(scale)));
        m.id()
    } else {
        commands.spawn((Mesh3d(shared.pip.0.clone()), MeshMaterial3d(shared.pip.1.clone()), Transform::from_xyz(0.0, lift * SCALE, 0.0).with_scale(Vec3::splat(4.0)))).id()
    };
    let pips: [Entity; 3] = std::array::from_fn(|i| {
        let a = (i as f32 - 1.0) * 0.55;
        commands.spawn((Mesh3d(shared.pip.0.clone()), MeshMaterial3d(shared.pip.1.clone()), Transform::from_xyz(a.sin() * 80.0 * SCALE, 40.0 * SCALE, a.cos() * 80.0 * SCALE), Visibility::Hidden)).id()
    });
    let jam = commands.spawn((Mesh3d(shared.jam.0.clone()), MeshMaterial3d(shared.jam.1.clone()), Transform::from_xyz(0.0, 230.0 * SCALE, 0.0), Visibility::Hidden)).id();
    let rally = commands.spawn((Mesh3d(shared.rally.0.clone()), MeshMaterial3d(shared.rally.1.clone()), Transform::from_xyz(0.0, 6.0 * SCALE, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(110.0 * SCALE)), Visibility::Hidden)).id();
    let select = commands.spawn((Mesh3d(shared.select.0.clone()), MeshMaterial3d(shared.select.1.clone()), Transform::from_xyz(0.0, 8.0 * SCALE, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(100.0 * SCALE)), Visibility::Hidden)).id();
    let mut children = vec![ped, model, jam, rally, select];
    children.extend(pips);
    commands.entity(rootent).add_children(&children);
    TowerSlot { root: rootent, kind, model, pips, jam, rally, select }
}

#[allow(clippy::too_many_arguments)]
fn sync_render(
    mut minimal: ResMut<crate::hud::HudMinimal>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<GateBridge>,
    mut pools: ResMut<Pools>,
    mut q: Query<(&mut Transform, &mut Visibility), With<Pooled>>,
    cam: Query<&Transform, (With<MainCamera>, Without<Pooled>)>,
) {
    if !minimal.0 {
        minimal.0 = true;
    }
    if pools.shared.is_none() {
        pools.shared = Some(build_shared(&mut meshes, &mut materials));
        pools.enemy_slot = vec![None; ENEMY_SLOTS];
        pools.bars = vec![None; ENEMY_SLOTS];
        pools.towers = (0..TOWER_SLOTS).map(|_| None).collect();
        pools.proj = vec![None; PROJ_SLOTS];
        pools.pickup = vec![None; PICKUP_SLOTS];
        pools.beam = vec![None; BEAM_SLOTS];
        pools.fx = (0..FX_SLOTS).map(|_| None).collect();
    }
    if bridge.enemies.len() != ENEMY_SLOTS {
        return;
    }
    let pools = &mut *pools;
    let shared = pools.shared.as_ref().unwrap();
    let hidden = |commands: &mut Commands, bundle: (Mesh3d, MeshMaterial3d<StandardMaterial>)| commands.spawn((XpEntity, Pooled, bundle, Transform::default(), Visibility::Hidden)).id();
    let cam_rot = cam.single().map_or(Quat::IDENTITY, |t| t.rotation);
    // Foes: a slot's model can change when it is reused; entities return to a per-model pool.
    for (i, item) in bridge.enemies.iter().enumerate() {
        if let Some((k, e)) = pools.enemy_slot[i] {
            if k != item.kind {
                set_vis(&mut q, e, false);
                pools.enemy_free[k as usize].push(e);
                pools.enemy_slot[i] = None;
            }
        }
        if item.kind == 0 {
            if let Some((bg, fill)) = pools.bars[i] {
                set_vis(&mut q, bg, false);
                set_vis(&mut q, fill, false);
            }
            continue;
        }
        // The ported OoT / BK units (kinds past the SM64 models) are drawn by `foes_view`; only their bar is ours.
        if (item.kind as usize) < FOE_MODELS.len() {
        let entity = match pools.enemy_slot[i] {
            Some((_, e)) => e,
            None => {
                let e = pools.enemy_free[item.kind as usize].pop().unwrap_or_else(|| {
                    let path = FOE_MODELS[item.kind as usize];
                    if root.exists(path) {
                        // Foe models carry SM64 animation clips; without a playing clip their limbs sit in the raw
                        // rest pose. `spawn_model` is the same driver the course objects use.
                        let mut foe = crate::render::object_view::spawn_model(&mut commands, &assets, &root, path, true);
                        foe.insert((XpEntity, Pooled, Visibility::Hidden));
                        foe.id()
                    } else {
                        let mesh = meshes.add(Sphere::new(60.0 * SCALE).mesh().ico(1).unwrap());
                        let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.8, 0.3, 0.3), ..default() });
                        hidden(&mut commands, (Mesh3d(mesh), MeshMaterial3d(mat)))
                    }
                });
                pools.enemy_slot[i] = Some((item.kind, e));
                e
            }
        };
        place(&mut q, entity, item, Quat::from_rotation_y(item.yaw));
        }
        // Health bar: shown once hurt (and always for bosses), turned to face the camera.
        let frac = item.aux.clamp(0.0, 1.0);
        let boss = EKind::from_u8(item.kind).is_some_and(|k| k.is_boss());
        let show = frac < 0.999 || boss;
        if show || pools.bars[i].is_some() {
            let (bg, fill) = *pools.bars[i].get_or_insert_with(|| {
                let bg = hidden(&mut commands, (Mesh3d(shared.bar_mesh.clone()), MeshMaterial3d(shared.bar_bg.clone())));
                let fill = hidden(&mut commands, (Mesh3d(shared.bar_mesh.clone()), MeshMaterial3d(shared.bar_fill.clone())));
                (bg, fill)
            });
            set_vis(&mut q, bg, show);
            set_vis(&mut q, fill, show);
            if show {
                let w = if boss { 2.2 } else { 1.0 };
                let h = if boss { 0.2 } else { 0.14 };
                let top = item.pos + Vec3::Y * (item.scale * if boss { 3.4 } else { 0.8 } + 0.4);
                if let Ok((mut t, _)) = q.get_mut(bg) {
                    *t = Transform { translation: top, rotation: cam_rot, scale: Vec3::new(w + 0.05, h + 0.05, 1.0) };
                }
                if let Ok((mut t, _)) = q.get_mut(fill) {
                    let right = cam_rot * Vec3::X;
                    *t = Transform { translation: top - right * (w * (1.0 - frac) * 0.5) + cam_rot * Vec3::Z * 0.01, rotation: cam_rot, scale: Vec3::new((w * frac).max(0.001), h, 1.0) };
                }
                let want = if boss { shared.bar_boss.clone() } else { shared.bar_fill.clone() };
                commands.entity(fill).insert(MeshMaterial3d(want));
            }
        }
    }
    // Towers.
    for (i, item) in bridge.towers.iter().enumerate() {
        if let Some(slot) = &pools.towers[i] {
            if slot.kind != item.kind {
                commands.entity(slot.root).despawn();
                pools.towers[i] = None;
            }
        }
        if item.kind == 0 {
            // Sold, or the run restarted: hide the pooled tower.
            if let Some(slot) = &pools.towers[i] {
                set_vis(&mut q, slot.root, false);
            }
            continue;
        }
        if pools.towers[i].is_none() {
            pools.towers[i] = Some(spawn_tower(&mut commands, &assets, &root, shared, item.kind));
        }
        let slot = pools.towers[i].as_ref().unwrap();
        if let Ok((mut tf, mut vis)) = q.get_mut(slot.root) {
            *vis = Visibility::Inherited;
            tf.translation = item.pos;
            tf.rotation = Quat::IDENTITY;
        }
        // Level scales the model; the shot kick squashes it for a moment; ranged towers turn to aim.
        let (_, base, lift) = TOWER_MODELS[(item.kind - 1) as usize];
        let k = base * (1.0 + 0.16 * item.level as f32) * (1.0 + 0.12 * item.kick);
        // Cannon and flower turn to aim (the barrel leans toward its target); the star and the coin spin.
        let yaw = match item.kind - 1 {
            0 | 2 => item.yaw,
            4 | 5 => bridge.time * 2.2,
            _ => 0.0,
        };
        let lean = if item.kind == 1 { Quat::from_rotation_x(0.9) } else { Quat::IDENTITY };
        // The Prism Star is a flat model: face it to the camera and pinwheel it in its own plane so it never shows edge-on.
        let rot = if item.kind == 5 { cam_rot * Quat::from_rotation_z(bridge.time * 1.8) } else { Quat::from_rotation_y(yaw) * lean };
        commands.entity(slot.model).insert(Transform::from_xyz(0.0, lift * SCALE, 0.0).with_scale(Vec3::splat(k)).with_rotation(rot));
        for (p, e) in slot.pips.iter().enumerate() {
            set_vis(&mut q, *e, item.level as usize >= p);
        }
        set_vis(&mut q, slot.jam, item.jam);
        set_vis(&mut q, slot.rally, item.rally && !item.jam);
        set_vis(&mut q, slot.select, item.selected);
    }
    // Shells and fireballs, coins.
    for (i, item) in bridge.projs.iter().enumerate() {
        if item.kind == 0 && pools.proj[i].is_none() {
            continue;
        }
        let pair = if item.kind == 2 { &shared.fire } else { &shared.shell };
        let want = item.kind.max(1);
        let e = match pools.proj[i] {
            Some((e, k)) if k == want || item.kind == 0 => e,
            Some((e, _)) => {
                commands.entity(e).insert((Mesh3d(pair.0.clone()), MeshMaterial3d(pair.1.clone())));
                pools.proj[i] = Some((e, want));
                e
            }
            None => {
                let e = hidden(&mut commands, (Mesh3d(pair.0.clone()), MeshMaterial3d(pair.1.clone())));
                pools.proj[i] = Some((e, want));
                e
            }
        };
        place(&mut q, e, item, Quat::IDENTITY);
    }
    for (i, item) in bridge.pickups.iter().enumerate() {
        if item.kind == 0 && pools.pickup[i].is_none() {
            continue;
        }
        let e = *pools.pickup[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.coin.0.clone()), MeshMaterial3d(shared.coin.1.clone()))));
        let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        place(&mut q, e, item, Quat::from_rotation_y(bridge.time * 4.0 + i as f32) * flat);
    }
    // Beams: a stretched unit cube from a to b.
    for (i, b) in bridge.beams.iter().enumerate() {
        if !b.on && pools.beam[i].is_none() {
            continue;
        }
        let e = *pools.beam[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.beam.0.clone()), MeshMaterial3d(shared.beam.1.clone()))));
        set_vis(&mut q, e, b.on);
        if b.on {
            if let Ok((mut t, _)) = q.get_mut(e) {
                let d = b.b - b.a;
                let len = d.length().max(0.01);
                let w = 0.2 * b.fade.clamp(0.2, 1.0);
                *t = Transform { translation: (b.a + b.b) * 0.5, rotation: Quat::from_rotation_arc(Vec3::Z, d / len), scale: Vec3::new(w, w, len) };
            }
        }
    }
    // Rings: each slot owns a material so it can fade on its own.
    for (i, item) in bridge.fx.iter().enumerate() {
        if item.kind == 0 && pools.fx[i].is_none() {
            continue;
        }
        let (e, mat) = pools.fx[i].get_or_insert_with(|| {
            let mat = unlit_blend(&mut materials, Color::srgba(1.0, 1.0, 1.0, 0.8));
            (hidden(&mut commands, (Mesh3d(shared.ring.clone()), MeshMaterial3d(mat.clone()))), mat)
        });
        if item.kind != 0 {
            if let Some(mut m) = materials.get_mut(&*mat) {
                let c = match item.kind {
                    1 => [1.0, 0.6, 0.2],
                    2 => [0.8, 0.8, 1.0],
                    _ => [1.0, 0.95, 0.4],
                };
                m.base_color = Color::srgba(c[0], c[1], c[2], 0.85 * item.aux);
            }
        }
        place(&mut q, *e, item, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
    }
    // Build cursor and range ring.
    let (cell, ring, ring_mat) = pools.cursor.get_or_insert_with(|| {
        let cell = hidden(&mut commands, (Mesh3d(shared.cell.0.clone()), MeshMaterial3d(shared.cell.1.clone())));
        let mat = unlit_blend(&mut materials, Color::srgba(1.0, 1.0, 1.0, 0.8));
        let ring = hidden(&mut commands, (Mesh3d(shared.ring.clone()), MeshMaterial3d(mat.clone())));
        (cell, ring, mat)
    });
    let c = bridge.cursor;
    set_vis(&mut q, *cell, c.show);
    set_vis(&mut q, *ring, c.show && c.range > 0.0);
    if c.show {
        if let Ok((mut t, _)) = q.get_mut(*cell) {
            t.translation = c.pos + Vec3::Y * 0.1;
        }
        if let Some(mut m) = materials.get_mut(&shared.cell.1) {
            let col = if c.ok { Color::srgba(0.2, 1.0, 0.3, 0.5) } else { Color::srgba(1.0, 0.2, 0.2, 0.5) };
            if m.base_color != col {
                m.base_color = col;
            }
        }
        if c.range > 0.0 {
            if let Ok((mut t, _)) = q.get_mut(*ring) {
                *t = Transform { translation: c.pos + Vec3::Y * 0.12, rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2), scale: Vec3::splat(c.range * SCALE) };
            }
            if let Some(mut m) = materials.get_mut(&*ring_mat) {
                let col = Color::srgba(c.tint[0], c.tint[1], c.tint[2], 0.9);
                if m.base_color != col {
                    m.base_color = col;
                }
            }
        }
    }
    let _ = &shared.range_mat;
}

/// Entities are despawned by the framework on exit; forget the handles.
fn cleanup(mut pools: ResMut<Pools>, mut ui: ResMut<UiState>, mut minimal: ResMut<crate::hud::HudMinimal>, mut mode: ResMut<ModeButtons>) {
    if pools.shared.is_some() {
        if minimal.0 {
            minimal.0 = false;
        }
        *pools = Pools::default();
        mode.active = false;
    }
    *ui = UiState::default();
}

// ---------------------------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct Overlay;
/// Panels and buttons that swallow playfield taps.
#[derive(Component)]
struct UiBlock;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum GateBtn {
    Type(u8),
    Close,
    Upgrade,
    Sell,
    Mode,
    Speed,
    Pause,
    Next,
    Map(usize),
    Retry,
    OtherMap,
    Menu,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Label {
    Coins,
    Lives,
    Wave,
    Timer,
    Speed,
    Pause,
    Next,
    PreviewHead,
    PreviewTag,
    Preview(u8),
    BuildInfo,
    CardTitle,
    CardLine(u8),
    CardMode,
    CardUp,
    CardSell,
    Banner,
    Hint,
    Cost(u8),
}

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    preview: Option<Entity>,
    menu: Option<(Entity, u32, u8)>,
    maps: Option<Entity>,
    summary: Option<Entity>,
    banner: Option<Entity>,
    last_hud: Option<Hud>,
    last_preview: Option<(Preview, u32, bool)>,
    last_menu: Option<MenuView>,
    last_best: [u32; 2],
    selected: [Option<GateBtn>; 2],
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

fn text(a: &UiAssets, s: &str, size: f32, color: Color) -> impl Bundle {
    (Text::new(clean(s.to_string())), font(&a.body, size), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY })
}

fn pill<'a>(p: &'a mut ChildSpawnerCommands<'_>, bg: Color) -> EntityCommands<'a> {
    p.spawn((Node { padding: UiRect::axes(Val::Px(9.0), Val::Px(2.0)), border: UiRect::all(Val::Px(2.0)), align_items: AlignItems::Center, column_gap: Val::Px(5.0), height: Val::Px(28.0), ..default() }, BackgroundColor(bg), BorderColor::all(NAVY), BorderRadius::all(Val::Px(99.0)), UiBlock))
}

fn small_button<'a>(p: &'a mut ChildSpawnerCommands, a: &UiAssets, kind: ButtonKind, w: f32, h: f32, size: f32, label: &str, id: GateBtn, lab: Option<Label>) -> EntityCommands<'a> {
    let mut b = button(p, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(h), size), label);
    b.insert((id, UiBlock));
    if let Some(l) = lab {
        // The label text is the button's second child (after the bevel highlight).
        b.queue(move |mut e: EntityWorldMut| {
            let kids: Vec<Entity> = e.get::<Children>().map(|c| c.to_vec()).unwrap_or_default();
            if let Some(t) = kids.get(1) {
                let t = *t;
                e.world_scope(|w| {
                    w.entity_mut(t).insert(l);
                });
            }
        });
    }
    b
}

fn spawn_hud(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 4.0);
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(12), node)).id();
    commands.entity(root).with_children(|p| {
        p.spawn((Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), ..default() },)).with_children(|r| {
            pill(r, NAVY_2).with_children(|c| {
                icon(c, &a.coin, 16.0);
                c.spawn((Label::Coins, text(a, "0", 15.0, GOLD_HI)));
            });
            pill(r, NAVY_2).with_children(|c| {
                c.spawn(text(a, "GATE", 11.0, CREAM));
                c.spawn((Label::Lives, text(a, "20", 15.0, Color::WHITE)));
            });
            pill(r, NAVY_2).with_children(|c| {
                c.spawn((Label::Wave, text(a, "WAVE 0/15", 13.0, Color::WHITE)));
            });
            pill(r, NAVY_2).with_children(|c| {
                c.spawn((Label::Timer, text(a, "NEXT 0:40", 13.0, CREAM)));
            });
            small_button(r, a, ButtonKind::Gold, 64.0, 30.0, 12.0, "CALL", GateBtn::Next, Some(Label::Next));
            small_button(r, a, ButtonKind::Blue, 40.0, 30.0, 13.0, "x1", GateBtn::Speed, Some(Label::Speed));
            small_button(r, a, ButtonKind::Blue, 40.0, 30.0, 12.0, "||", GateBtn::Pause, Some(Label::Pause));
        });
        p.spawn((Label::Hint, text(a, "Build towers on grass. Punch, dive and ground-pound to help, and stand by a tower to rally it.", 11.0, CREAM), Node { margin: UiRect::top(Val::Px(4.0)), max_width: Val::Px(380.0), ..default() }));
    });
    root
}

fn spawn_banner(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(11), node)).id();
    commands.entity(root).with_children(|p| {
        p.spawn((Label::Banner, Text::new("PAUSED"), font(&a.display, 34.0), TextColor(GOLD), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
    });
    root
}

fn spawn_preview(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let right = area.right.max(0.0) + crate::touch::MARGIN_X.min(60.0);
    let top = area.top.max(0.0) + crate::touch::PAUSE_TOP + 46.0 + 6.0;
    let root = commands
        .spawn((
            XpEntity,
            Overlay,
            UiBlock,
            GlobalZIndex(10),
            Node { position_type: PositionType::Absolute, right: Val::Px(right.max(8.0) - 6.0), top: Val::Px(top), width: Val::Px(142.0), padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)), border: UiRect::all(Val::Px(2.0)), flex_direction: FlexDirection::Column, row_gap: Val::Px(1.0), ..default() },
            BackgroundColor(Color::srgba(0.04, 0.09, 0.28, 0.82)),
            BorderColor::all(NAVY),
            BorderRadius::all(Val::Px(10.0)),
        ))
        .id();
    commands.entity(root).with_children(|p| {
        p.spawn((Label::PreviewHead, Text::new("NEXT - WAVE 1"), font(&a.display, 12.0), TextColor(GOLD)));
        p.spawn((Label::PreviewTag, text(a, "", 10.0, RED_HI)));
        for i in 0..6u8 {
            p.spawn((Label::Preview(i), text(a, "", 11.0, CREAM)));
        }
    });
    root
}

fn short_effect(c: &CardView) -> String {
    match c.kind - 1 {
        0 => format!("Splash {:.0}", c.effect),
        1 => format!("Stun {:.1}s", c.effect / TICKS_PER_SEC as f32),
        2 => format!("Burn {:.0}/s", c.effect),
        3 => format!("Slow {:.0}%", c.effect * 100.0),
        4 => "Pierces a line".to_string(),
        _ => format!("Pays {:.0} coins", c.effect),
    }
}

fn card_lines(c: &CardView) -> [String; 3] {
    let tk = TKind::from_u8(c.kind - 1).unwrap_or(TKind::Bombard);
    let l1 = if tk == TKind::Spout { format!("{}   every {:.1}s", short_effect(c), 90.0 / TICKS_PER_SEC as f32) } else if tk == TKind::Eye { format!("{}   Range {:.0}", short_effect(c), c.range) } else { format!("Dmg {:.0}   {:.1}/s   Range {:.0}", c.damage, c.per_sec, c.range) };
    let l2 = if tk == TKind::Spout || tk == TKind::Eye || tk == TKind::Prism { String::new() } else { short_effect(c) };
    let l3 = if c.jam {
        "JAMMED - stand close to repair it".to_string()
    } else if c.overdrive {
        "OVERDRIVE!".to_string()
    } else if c.rally {
        "Rallied by the hero".to_string()
    } else if tk == TKind::Spout {
        format!("Earned {} coins", c.dealt)
    } else {
        format!("{} kills, {} damage", c.kills, c.dealt)
    };
    [l1, l2, l3]
}

fn spawn_menu(commands: &mut Commands, a: &UiAssets, area: &SafeArea, menu: &MenuView) -> Option<Entity> {
    match menu {
        MenuView::None => None,
        MenuView::Build(b) => {
            // Between the touch stick (left) and the buttons (right): bottom centre.
            let bottom = area.bottom.max(0.0) + 8.0;
            let root = commands
                .spawn((
                    XpEntity,
                    Overlay,
                    GlobalZIndex(14),
                    Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), bottom: Val::Px(bottom), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), ..default() },
                ))
                .id();
            commands.entity(root).with_children(|p| {
                p.spawn((
                    Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)), border: UiRect::all(Val::Px(2.0)), max_width: Val::Px(420.0), ..default() },
                    BackgroundColor(Color::srgba(0.04, 0.09, 0.28, 0.85)),
                    BorderColor::all(NAVY),
                    BorderRadius::all(Val::Px(10.0)),
                    UiBlock,
                ))
                .with_children(|i| {
                    i.spawn((Label::BuildInfo, text(a, "Pick a tower", 12.0, CREAM)));
                });
                p.spawn((Node { column_gap: Val::Px(5.0), padding: UiRect::all(Val::Px(5.0)), border: UiRect::all(Val::Px(3.0)), ..default() }, BackgroundColor(Color::srgba(0.04, 0.09, 0.28, 0.88)), BorderColor::all(GOLD), BorderRadius::all(Val::Px(14.0)), UiBlock)).with_children(|row| {
                    for k in 0..6u8 {
                        let mut btn = button(row, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(58.0), Val::Px(52.0), 11.0), "");
                        btn.insert((GateBtn::Type(k), UiBlock, Node { width: Val::Px(58.0), height: Val::Px(52.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::all(Val::Px(3.0)), flex_shrink: 0.0, ..default() }));
                        btn.with_children(|c| {
                            c.spawn(text(a, SHORT_NAMES[k as usize], 11.0, Color::srgb(TOWER_TINTS[k as usize][0] * 0.4 + 0.6, TOWER_TINTS[k as usize][1] * 0.4 + 0.6, TOWER_TINTS[k as usize][2] * 0.4 + 0.6)));
                            c.spawn((Label::Cost(k), text(a, &format!("{}", TKind::from_u8(k).unwrap().spec(0).cost), 13.0, if b.afford[k as usize] { GOLD_HI } else { RED_HI })));
                        });
                    }
                    small_button(row, a, ButtonKind::Red, 34.0, 52.0, 14.0, "X", GateBtn::Close, None);
                });
            });
            Some(root)
        }
        MenuView::Card(_) => {
            let left = area.left.max(0.0) + 10.0;
            let top = area.top.max(0.0) + 40.0;
            let root = commands
                .spawn((
                    XpEntity,
                    Overlay,
                    UiBlock,
                    GlobalZIndex(14),
                    Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), width: Val::Px(214.0), padding: UiRect::axes(Val::Px(9.0), Val::Px(6.0)), border: UiRect::all(Val::Px(3.0)), flex_direction: FlexDirection::Column, row_gap: Val::Px(2.0), ..default() },
                    BackgroundColor(Color::srgba(0.04, 0.09, 0.28, 0.9)),
                    BorderColor::all(GOLD),
                    BorderRadius::all(Val::Px(14.0)),
                ))
                .id();
            commands.entity(root).with_children(|p| {
                p.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|r| {
                    r.spawn((Label::CardTitle, Text::new("TOWER"), font(&a.display, 15.0), TextColor(GOLD), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                    small_button(r, a, ButtonKind::Red, 28.0, 24.0, 12.0, "X", GateBtn::Close, None);
                });
                for i in 0..3u8 {
                    p.spawn((Label::CardLine(i), text(a, "", 11.0, if i == 2 { CREAM.with_alpha(0.85) } else { Color::WHITE })));
                }
                p.spawn(Node { margin: UiRect::top(Val::Px(3.0)), ..default() }).with_children(|r| {
                    small_button(r, a, ButtonKind::Blue, 196.0, 28.0, 12.0, "Target: First", GateBtn::Mode, Some(Label::CardMode));
                });
                p.spawn(Node { column_gap: Val::Px(6.0), margin: UiRect::top(Val::Px(2.0)), ..default() }).with_children(|r| {
                    small_button(r, a, ButtonKind::Green, 110.0, 34.0, 13.0, "UPGRADE", GateBtn::Upgrade, Some(Label::CardUp));
                    small_button(r, a, ButtonKind::Gold, 80.0, 34.0, 13.0, "SELL", GateBtn::Sell, Some(Label::CardSell));
                });
            });
            Some(root)
        }
    }
}

fn spawn_maps(commands: &mut Commands, a: &UiAssets, area: &SafeArea, best: [u32; 2]) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    node.row_gap = Val::Px(8.0);
    let root = commands.spawn((XpEntity, Overlay, UiBlock, GlobalZIndex(22), node, BackgroundColor(Color::srgba(0.02, 0.05, 0.16, 0.8)))).id();
    commands.entity(root).with_children(|p| {
        outlined(p, a, "HOLD THE GATE", 34.0, GOLD);
        p.spawn(text(a, "Choose a map. Build towers, fight beside them, and keep the foes from the gate for 15 waves.", 12.0, CREAM));
        p.spawn(Node { column_gap: Val::Px(14.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|row| {
            for (i, m) in MAPS.iter().enumerate() {
                let mut b = button(row, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(230.0), Val::Px(118.0), 16.0), "");
                b.insert((GateBtn::Map(i), UiBlock, Node { width: Val::Px(230.0), height: Val::Px(118.0), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, padding: UiRect::all(Val::Px(8.0)), border: UiRect::all(Val::Px(3.0)), row_gap: Val::Px(3.0), ..default() }));
                b.with_children(|c| {
                    c.spawn((Text::new(clean(format!("[{}] {}", i + 1, m.name))), font(&a.display, 20.0), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                    c.spawn((text(a, m.blurb, 11.0, Color::WHITE), TextLayout::justify(Justify::Center), Node { width: Val::Percent(100.0), ..default() }));
                    c.spawn(text(a, &format!("Best: wave {}/{}", best[i], WAVES), 12.0, CREAM));
                });
            }
        });
        crate::hints::note("Tap a map, or press 1 / 2");
        p.spawn((crate::hints::ControlHint, text(a, "Tap a map, or press 1 / 2", 11.0, CREAM.with_alpha(0.8))));
    });
    root
}

fn spawn_summary(commands: &mut Commands, a: &UiAssets, area: &SafeArea, s: &Summary) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, UiBlock, GlobalZIndex(22), node, BackgroundColor(Color::srgba(0.06, 0.0, 0.08, 0.6)))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(12.0)), row_gap: Val::Px(5.0), width: Val::Px(400.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
            kicker(c, a, &format!("HOLD THE GATE - {}", MAPS[s.map % MAPS.len()].name.to_uppercase()));
            let (title, color) = if s.won { ("GATE HELD!", GOLD) } else if s.new_best { ("NEW BEST!", GOLD) } else { ("GATE FALLEN", RED_HI) };
            outlined(c, a, title, 38.0, color);
            let mut row = |label: &str, value: String, color: Color| {
                c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                    r.spawn(text(a, label, 15.0, CREAM));
                    r.spawn((Text::new(clean(value)), font(&a.display, 19.0), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                });
            };
            row("Waves cleared", format!("{} / {}", s.waves_cleared, WAVES), Color::WHITE);
            row("Foes defeated", s.kills.to_string(), Color::WHITE);
            row("Coins earned", s.coins.to_string(), GOLD_HI);
            row("Best on this map", format!("wave {}", s.best), GOLD_HI);
            c.spawn(Node { column_gap: Val::Px(8.0), margin: UiRect::top(Val::Px(8.0)), ..default() }).with_children(|r| {
                for (label, kind, which, w) in [("RETRY", ButtonKind::Gold, GateBtn::Retry, 110.0), ("OTHER MAP", ButtonKind::Green, GateBtn::OtherMap, 120.0), ("MAIN MENU", ButtonKind::Blue, GateBtn::Menu, 120.0)] {
                    let mut b = button(r, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(44.0), 15.0), label);
                    b.insert((which, UiBlock));
                }
            });
        });
    });
    root
}

fn set_text(t: &mut Text, s: &str) {
    if t.0 != s {
        t.0.clear();
        t.0.push_str(s);
    }
}

#[allow(clippy::too_many_arguments)]
fn manage_ui(mut commands: Commands, bridge: Res<GateBridge>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, mut ui: ResMut<UiState>) {
    let Some(a) = assets else { return };
    let playing = bridge.stage != Stage::MapSelect;
    if ui.hud.is_none() && playing {
        ui.hud = Some(spawn_hud(&mut commands, &a, &area));
        ui.banner = Some(spawn_banner(&mut commands, &a, &area));
    }
    // The wave preview gives way to the summary card.
    match (ui.preview, bridge.stage == Stage::Playing) {
        (None, true) => ui.preview = Some(spawn_preview(&mut commands, &a, &area)),
        (Some(e), false) => {
            commands.entity(e).despawn();
            ui.preview = None;
            ui.last_preview = None;
        }
        _ => {}
    }
    if !playing {
        for e in [ui.hud.take(), ui.preview.take(), ui.banner.take()].into_iter().flatten() {
            commands.entity(e).despawn();
        }
        ui.last_hud = None;
        ui.last_preview = None;
    }
    // Map picker.
    match (ui.maps, bridge.stage == Stage::MapSelect) {
        (None, true) => {
            ui.maps = Some(spawn_maps(&mut commands, &a, &area, bridge.best));
            ui.last_best = bridge.best;
        }
        (Some(e), false) => {
            commands.entity(e).despawn();
            ui.maps = None;
        }
        (Some(e), true) if ui.last_best != bridge.best => {
            commands.entity(e).despawn();
            ui.maps = Some(spawn_maps(&mut commands, &a, &area, bridge.best));
            ui.last_best = bridge.best;
        }
        _ => {}
    }
    // Build bar / tower card: rebuilt when the structure changes (not on every number).
    let kind_tag = match bridge.menu {
        MenuView::None => 0u8,
        MenuView::Build(_) => 1,
        MenuView::Card(_) => 2,
    };
    let rebuild = match ui.menu {
        None => kind_tag != 0,
        Some((_, gen, tag)) => gen != bridge.ui_gen || tag != kind_tag,
    };
    if rebuild {
        if let Some((e, _, _)) = ui.menu.take() {
            commands.entity(e).despawn();
        }
        ui.last_menu = None;
        if let Some(e) = spawn_menu(&mut commands, &a, &area, &bridge.menu) {
            ui.menu = Some((e, bridge.ui_gen, kind_tag));
        }
    }
    // Summary.
    match (ui.summary, bridge.summary.filter(|_| bridge.summary_ready)) {
        (None, Some(s)) => ui.summary = Some(spawn_summary(&mut commands, &a, &area, &s)),
        (Some(e), None) => {
            commands.entity(e).despawn();
            ui.summary = None;
        }
        _ => {}
    }
}

fn kind_color(k: u8) -> Color {
    let boss = EKind::from_u8(k).is_some_and(|k| k.is_boss());
    if boss {
        GOLD_HI
    } else if EKind::from_u8(k).is_some_and(|k| k.flying()) {
        hex(0xb9d8ff)
    } else {
        CREAM
    }
}

#[allow(clippy::type_complexity)]
fn update_ui(
    bridge: Res<GateBridge>,
    mut ui: ResMut<UiState>,
    mut labels: Query<(&Label, &mut Text, &mut Node, Option<&mut TextColor>)>,
    mut mode: ResMut<ModeButtons>,
    touch: Res<TouchControls>,
    mut commands: Commands,
    btns: Query<(Entity, &GateBtn)>,
) {
    // HUD numbers: reformat only when something changed.
    let h = bridge.hud;
    let pv = (bridge.preview, h.preview_wave, h.preview_current);
    let hud_changed = ui.last_hud != Some(h);
    let preview_changed = ui.last_preview != Some(pv);
    let menu_changed = ui.last_menu != Some(bridge.menu);
    if hud_changed || preview_changed || menu_changed {
        for (l, mut t, mut n, color) in &mut labels {
            match *l {
                Label::Coins if hud_changed => set_text(&mut t, &h.coins.to_string()),
                Label::Lives if hud_changed => {
                    set_text(&mut t, &format!("{}", h.lives.max(0)));
                    if let Some(mut c) = color {
                        c.0 = if h.lives * 4 <= h.max_lives { RED_HI } else { Color::WHITE };
                    }
                }
                Label::Wave if hud_changed => set_text(&mut t, &format!("WAVE {}/{}", h.wave, h.waves)),
                Label::Timer if hud_changed => {
                    let s = if h.wave >= h.waves && !h.spawning {
                        "FINAL WAVE".to_string()
                    } else if h.spawning {
                        format!("{} foes", h.alive)
                    } else {
                        format!("NEXT {}:{:02}", h.timer / 60, h.timer % 60)
                    };
                    set_text(&mut t, &s);
                }
                Label::Speed if hud_changed => set_text(&mut t, if h.speed >= 2 { "x2" } else { "x1" }),
                Label::Pause if hud_changed => set_text(&mut t, if h.paused { ">" } else { "||" }),
                Label::Next if hud_changed => set_text(&mut t, if h.can_call { "CALL" } else { "..." }),
                Label::Banner if hud_changed => {
                    let d = if h.paused { Display::Flex } else { Display::None };
                    if n.display != d {
                        n.display = d;
                    }
                }
                Label::Hint if hud_changed || menu_changed => {
                    let d = if bridge.hint && crate::hints::show() && h.wave == 0 && bridge.menu == MenuView::None { Display::Flex } else { Display::None };
                    if n.display != d {
                        n.display = d;
                    }
                }
                Label::PreviewHead if preview_changed => {
                    let s = if bridge.preview.n == 0 { String::new() } else if h.preview_current { format!("WAVE {} NOW", h.preview_wave) } else { format!("NEXT - WAVE {}", h.preview_wave) };
                    set_text(&mut t, &s);
                }
                Label::PreviewTag if preview_changed => {
                    let p = bridge.preview;
                    let s = if p.boss { "BOSS WAVE" } else if p.air { "Flyers incoming" } else { "" };
                    set_text(&mut t, s);
                    let d = if s.is_empty() { Display::None } else { Display::Flex };
                    if n.display != d {
                        n.display = d;
                    }
                    if let Some(mut c) = color {
                        c.0 = if p.boss { GOLD } else { hex(0xb9d8ff) };
                    }
                }
                Label::Preview(i) if preview_changed => {
                    let p = bridge.preview;
                    let i = i as usize;
                    if i < p.n {
                        let kind = EKind::from_u8(p.kinds[i]);
                        set_text(&mut t, &format!("{} x{}", kind.map_or("?", |k| k.name()), p.counts[i]));
                        if let Some(mut c) = color {
                            c.0 = kind_color(p.kinds[i]);
                        }
                        if n.display != Display::Flex {
                            n.display = Display::Flex;
                        }
                    } else if n.display != Display::None {
                        n.display = Display::None;
                    }
                }
                Label::Cost(k) if menu_changed => {
                    if let (MenuView::Build(b), Some(mut c)) = (bridge.menu, color) {
                        c.0 = if b.afford[k as usize] { GOLD_HI } else { RED_HI };
                    }
                }
                Label::BuildInfo if menu_changed => {
                    if let MenuView::Build(b) = bridge.menu {
                        let s = match (b.blocked, b.sel) {
                            (Some(why), _) => why.to_string(),
                            (None, Some(k)) => {
                                let tk = TKind::from_u8(k).unwrap();
                                format!("{} {}c: {}  Tap again to build", tk.name(), tk.spec(0).cost, tk.blurb())
                            }
                            (None, None) => "Pick a tower to build here".to_string(),
                        };
                        set_text(&mut t, &s);
                    }
                }
                Label::CardTitle if menu_changed => {
                    if let MenuView::Card(c) = bridge.menu {
                        let name = TKind::from_u8(c.kind - 1).map_or("", |k| k.name());
                        set_text(&mut t, &format!("{} LV {}", name.to_uppercase(), c.level + 1));
                    }
                }
                Label::CardLine(i) if menu_changed => {
                    if let MenuView::Card(c) = bridge.menu {
                        let lines = card_lines(&c);
                        set_text(&mut t, &lines[i as usize]);
                        let d = if lines[i as usize].is_empty() { Display::None } else { Display::Flex };
                        if n.display != d {
                            n.display = d;
                        }
                        if i == 2 {
                            if let Some(mut col) = color {
                                col.0 = if c.jam { RED_HI } else if c.overdrive || c.rally { GOLD_HI } else { CREAM.with_alpha(0.85) };
                            }
                        }
                    }
                }
                Label::CardMode if menu_changed => {
                    if let MenuView::Card(c) = bridge.menu {
                        let tk = TKind::from_u8(c.kind - 1).unwrap_or(TKind::Bombard);
                        if matches!(tk, TKind::Spout | TKind::Eye | TKind::Slab) {
                            set_text(&mut t, "No targeting");
                        } else {
                            let m = match c.mode {
                                0 => Target::First,
                                1 => Target::Last,
                                2 => Target::Strongest,
                                _ => Target::Closest,
                            };
                            set_text(&mut t, &format!("Target: {}", m.name()));
                        }
                    }
                }
                Label::CardUp if menu_changed => {
                    if let MenuView::Card(c) = bridge.menu {
                        let s = match c.up_cost {
                            Some(cost) => format!("UPGRADE {cost}"),
                            None => "MAX LEVEL".to_string(),
                        };
                        set_text(&mut t, &s);
                        if let Some(mut col) = color {
                            col.0 = if c.up_cost.is_some() && !c.afford { RED_HI } else { Color::WHITE };
                        }
                    }
                }
                Label::CardSell if menu_changed => {
                    if let MenuView::Card(c) = bridge.menu {
                        set_text(&mut t, &format!("SELL +{}", c.sell));
                    }
                }
                _ => {}
            }
        }
        ui.last_hud = Some(h);
        ui.last_preview = Some(pv);
        ui.last_menu = Some(bridge.menu);
    }
    // Touch buttons for the hero (build here, next wave, speed).
    let want_touch = bridge.active && touch.visible && bridge.stage == Stage::Playing;
    if mode.active != want_touch {
        mode.active = want_touch;
    }
    if want_touch {
        let labels = ["BUILD", if h.can_call { "WAVE" } else { "..." }, if h.speed >= 2 { "x2" } else { "x1" }];
        for i in 0..3 {
            if mode.labels[i] != labels[i] {
                mode.labels[i] = labels[i].to_string();
            }
            let c = [Some(hex(0x2a8f37)), Some(hex(0xc47d10)), Some(hex(0x2f6fe6))][i].map(|c| c.with_alpha(0.9));
            if mode.tint[i] != c {
                mode.tint[i] = c;
            }
        }
    }
    // Gamepad focus + selected tower type use `Selected` so the shared button styling shows them.
    let focused = if bridge.pad_focus {
        match bridge.menu {
            MenuView::Build(_) => Some(if bridge.focus < 6 { GateBtn::Type(bridge.focus as u8) } else { GateBtn::Close }),
            MenuView::Card(_) => Some([GateBtn::Upgrade, GateBtn::Sell, GateBtn::Mode, GateBtn::Close][bridge.focus.min(3)]),
            MenuView::None => None,
        }
    } else {
        None
    };
    let sel = [
        match bridge.menu {
            MenuView::Build(b) => b.sel.map(GateBtn::Type),
            _ => None,
        },
        focused,
    ];
    if ui.selected != sel {
        for (e, id) in &btns {
            let (was, now) = (ui.selected.contains(&Some(*id)), sel.contains(&Some(*id)));
            if was && !now {
                commands.entity(e).remove::<Selected>();
            }
            if now && !was {
                commands.entity(e).insert(Selected);
            }
        }
        ui.selected = sel;
    }
}

fn gather_input(
    mut bridge: ResMut<GateBridge>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut touch: ResMut<TouchControls>,
    buttons: Query<(&Interaction, &GateBtn), Changed<Interaction>>,
    state: Res<AppState>,
    mut menu: MessageWriter<ReturnToMenu>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
) {
    if let Ok(w) = windows.single() {
        bridge.view = [w.width(), w.height(), area.left.max(0.0), area.right.max(0.0), area.top.max(0.0), area.bottom.max(0.0)];
    }
    if *state != AppState::Playing {
        return;
    }
    let bits = std::mem::take(&mut touch.mode_pressed);
    if bridge.touch != touch.visible {
        bridge.touch = touch.visible;
    }
    let push = |b: &mut GateBridge, c: Cmd| {
        if b.cmds.len() < 16 {
            b.cmds.push(c);
        }
    };
    if bridge.stage == Stage::MapSelect {
        for (k, i) in [(KeyCode::Digit1, 0), (KeyCode::Digit2, 1)] {
            if keys.just_pressed(k) {
                push(&mut bridge, Cmd::PickMap(i));
            }
        }
    } else {
        if bits & 1 != 0 || keys.just_pressed(KeyCode::KeyB) || pads.iter().any(|p| p.just_pressed(GamepadButton::West)) {
            push(&mut bridge, Cmd::Interact);
        }
        if bits & 2 != 0 || keys.just_pressed(KeyCode::KeyN) || pads.iter().any(|p| p.just_pressed(GamepadButton::North)) {
            push(&mut bridge, Cmd::NextWave);
        }
        if bits & 4 != 0 || keys.just_pressed(KeyCode::KeyF) || pads.iter().any(|p| p.just_pressed(GamepadButton::DPadUp)) {
            push(&mut bridge, Cmd::Speed);
        }
        if keys.just_pressed(KeyCode::KeyP) || pads.iter().any(|p| p.just_pressed(GamepadButton::DPadDown)) {
            push(&mut bridge, Cmd::Pause);
        }
        for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6].into_iter().enumerate() {
            if keys.just_pressed(k) {
                push(&mut bridge, Cmd::PickType(i as u8));
            }
        }
        if keys.just_pressed(KeyCode::KeyU) {
            push(&mut bridge, Cmd::Upgrade);
        }
        if keys.just_pressed(KeyCode::KeyV) {
            push(&mut bridge, Cmd::Sell);
        }
        if keys.just_pressed(KeyCode::KeyT) {
            push(&mut bridge, Cmd::CycleMode);
        }
        if keys.just_pressed(KeyCode::Enter) {
            push(&mut bridge, Cmd::Confirm);
        }
        if keys.just_pressed(KeyCode::Backspace) || pads.iter().any(|p| p.just_pressed(GamepadButton::East)) {
            push(&mut bridge, Cmd::Close);
        }
        // Gamepad: d-pad left/right walks the open menu, South activates the focused entry.
        let n = match bridge.menu {
            MenuView::Build(_) => 7,
            MenuView::Card(_) => 4,
            MenuView::None => 0,
        };
        if n > 0 {
            if pads.iter().any(|p| p.just_pressed(GamepadButton::DPadLeft)) {
                bridge.focus = (bridge.focus + n - 1) % n;
                bridge.pad_focus = true;
            }
            if pads.iter().any(|p| p.just_pressed(GamepadButton::DPadRight)) {
                bridge.focus = (bridge.focus + 1) % n;
                bridge.pad_focus = true;
            }
            if pads.iter().any(|p| p.just_pressed(GamepadButton::South)) {
                let f = bridge.focus;
                let c = match bridge.menu {
                    MenuView::Build(_) if f < 6 => Cmd::PickType(f as u8),
                    MenuView::Build(_) => Cmd::Close,
                    _ => [Cmd::Upgrade, Cmd::Sell, Cmd::CycleMode, Cmd::Close][f.min(3)],
                };
                push(&mut bridge, c);
            }
        }
    }
    if bridge.summary.is_some() && bridge.summary_ready && pads.iter().any(|p| p.just_pressed(GamepadButton::South)) {
        push(&mut bridge, Cmd::Retry);
    }
    for (i, b) in &buttons {
        if *i != Interaction::Pressed {
            continue;
        }
        bridge.pad_focus = false;
        let c = match *b {
            GateBtn::Type(k) => Cmd::PickType(k),
            GateBtn::Close => Cmd::Close,
            GateBtn::Upgrade => Cmd::Upgrade,
            GateBtn::Sell => Cmd::Sell,
            GateBtn::Mode => Cmd::CycleMode,
            GateBtn::Speed => Cmd::Speed,
            GateBtn::Pause => Cmd::Pause,
            GateBtn::Next => Cmd::NextWave,
            GateBtn::Map(m) => Cmd::PickMap(m),
            GateBtn::Retry => Cmd::Retry,
            GateBtn::OtherMap => Cmd::OtherMap,
            GateBtn::Menu => {
                menu.write(ReturnToMenu);
                continue;
            }
        };
        push(&mut bridge, c);
    }
}

/// Taps on the playfield (a short touch that did not start on a control, or a left click) become
/// ground points for the experiment.
#[allow(clippy::too_many_arguments)]
fn world_taps(
    mut bridge: ResMut<GateBridge>,
    touches: Res<Touches>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    blocks: Query<(&ComputedNode, &bevy::ui::UiGlobalTransform), With<UiBlock>>,
    area: Res<SafeArea>,
    time: Res<Time>,
    state: Res<AppState>,
    mut starts: Local<Vec<(u64, Vec2, f32, bool)>>,
    mut mouse_start: Local<Option<(Vec2, bool)>>,
) {
    if *state != AppState::Playing || bridge.stage == Stage::MapSelect {
        starts.clear();
        return;
    }
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cam.single()) else { return };
    let scale = window.scale_factor();
    let size = window.size();
    let inset = Vec3::new(area.left, area.right, area.bottom);
    let controls: Vec<Rect> = {
        let mut v: Vec<Rect> = crate::touch::layout(size, inset).into_iter().map(|c| c.1).collect();
        v.extend(crate::touch::mode_layout(size, inset).into_iter().map(|c| c.1));
        v.push(crate::touch::pause_rect(size, area.right, area.top));
        v
    };
    let blocked = |p: Vec2| -> bool {
        let phys = p * scale;
        controls.iter().any(|r| r.contains(p)) || blocks.iter().any(|(n, g)| {
            let c = g.translation;
            let half = n.size() * 0.5;
            n.size().x > 1.0 && (phys.x - c.x).abs() <= half.x && (phys.y - c.y).abs() <= half.y
        })
    };
    let now = time.elapsed_secs();
    for t in touches.iter_just_pressed() {
        starts.push((t.id(), t.position(), now, blocked(t.position())));
    }
    let mut taps: [Option<Vec2>; 2] = [None; 2];
    for t in touches.iter_just_released() {
        if let Some(i) = starts.iter().position(|s| s.0 == t.id()) {
            let (_, p0, t0, was_blocked) = starts.swap_remove(i);
            if !was_blocked && t.position().distance(p0) < 14.0 && now - t0 < 0.45 {
                taps[0] = Some(t.position());
            }
        }
    }
    for t in touches.iter_just_canceled() {
        starts.retain(|s| s.0 != t.id());
    }
    if mouse.just_pressed(MouseButton::Left) {
        if let Some(p) = window.cursor_position() {
            *mouse_start = Some((p, blocked(p)));
        }
    }
    if mouse.just_released(MouseButton::Left) {
        if let (Some((p0, was_blocked)), Some(p)) = (mouse_start.take(), window.cursor_position()) {
            if !was_blocked && p.distance(p0) < 6.0 {
                taps[1] = Some(p);
            }
        }
    }
    for p in taps.into_iter().flatten() {
        if let Ok(ray) = camera.viewport_to_world(cam_tf, p) {
            if let Some(d) = ray.intersect_plane(Vec3::new(0.0, ORIGIN[1] as f32 * SCALE, 0.0), InfinitePlane3d::new(Vec3::Y)) {
                let hit = ray.get_point(d);
                let tap = [(hit.x / SCALE - ORIGIN[0] as f32), (hit.z / SCALE - ORIGIN[2] as f32)];
                if bridge.cmds.len() < 16 {
                    bridge.cmds.push(Cmd::Tap(tap));
                }
            }
        }
    }
}

/// Menus (pause, settings) draw over the game; hide ours meanwhile so it cannot take their taps.
fn overlay_display(state: Res<AppState>, mut roots: Query<&mut Node, With<Overlay>>) {
    let d = if *state == AppState::Playing { Display::Flex } else { Display::None };
    for mut n in &mut roots {
        if n.display != d {
            n.display = d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_reset_sizes_every_list_and_clears_state() {
        let mut b = GateBridge::default();
        b.cmds.push(Cmd::Retry);
        b.summary = Some(Summary::default());
        b.reset();
        assert_eq!((b.enemies.len(), b.towers.len(), b.projs.len(), b.pickups.len(), b.beams.len(), b.fx.len()), (ENEMY_SLOTS, TOWER_SLOTS, PROJ_SLOTS, PICKUP_SLOTS, BEAM_SLOTS, FX_SLOTS));
        assert!(b.cmds.is_empty() && b.summary.is_none() && b.menu == MenuView::None);
        assert!(b.enemies.iter().all(|e| e.kind == 0) && b.towers.iter().all(|t| t.kind == 0));
    }

    #[test]
    fn every_foe_has_a_model_and_every_tower_a_name() {
        for k in ALL_EKINDS {
            assert!(FOE_MODELS[k as usize].ends_with(".glb"), "{}", k.name());
            assert!(clean(k.name().to_string()).is_ascii());
        }
        for (i, t) in ALL_TKINDS.iter().enumerate() {
            assert!(TOWER_MODELS[i].0.ends_with(".glb"));
            assert!(clean(t.name().to_string()).is_ascii() && clean(t.blurb().to_string()).is_ascii());
            assert!(!SHORT_NAMES[i].is_empty());
        }
        for m in &MAPS {
            assert!(clean(m.name.to_string()).is_ascii() && clean(m.blurb.to_string()).is_ascii());
        }
    }

    #[test]
    fn model_files_exist_in_the_asset_tree() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !root.exists() {
            return;
        }
        for p in FOE_MODELS.iter().skip(1).chain(TOWER_MODELS.iter().map(|t| &t.0)) {
            assert!(root.join(p).exists(), "{p}");
        }
    }

    #[test]
    fn card_text_covers_every_tower_kind() {
        for k in 0..6u8 {
            let tk = TKind::from_u8(k).unwrap();
            let s = tk.spec(1);
            let c = CardView { kind: k + 1, level: 1, mode: 0, up_cost: Some(150), afford: true, sell: 100, jam: false, rally: false, overdrive: false, kills: 3, dealt: 99, range: s.range, damage: s.damage, per_sec: 30.0 / s.interval, effect: s.effect };
            let lines = card_lines(&c);
            assert!(lines[0].len() > 3 && lines.iter().all(|l| clean(l.clone()).is_ascii()), "{:?}", lines);
        }
    }
}
