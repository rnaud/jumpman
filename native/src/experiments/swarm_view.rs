//! Star Swarm presentation: the render sync (pooled enemy models, instanced-by-sharing simple
//! meshes for stars/gems/orbs), the HUD, the level-up cards and the end-of-run card, all on
//! `bevy_ui`. The experiment (`swarm.rs`) owns the rules; it talks to this module through the
//! [`SwarmBridge`] resource (render lists and HUD numbers one way, card picks and button presses
//! the other), so none of this needs the experiment host.
//!
//! Rendering notes: enemies are `WorldAssetRoot`s of the real SM64 actor models. Entities are pooled
//! per model and only ever hidden/moved, never despawned mid-run, so a kill costs no scene
//! instantiation. Identical mesh + material pairs are batched by Bevy automatically, so hundreds
//! of foes cost a handful of draw calls. Gems far from the player are culled by the experiment.
use crate::button_marker::{Button, BorderRadius};
use super::swarm_core::*;
use super::XpEntity;
use crate::render::{AssetRoot, SCALE};
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::{AppState, ReturnToMenu};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Actor model per enemy kind (index = `EnemyKind as u8`, 0 unused).
pub const MODELS: [&str; 16] = [
    "",
    "actors/goomba/goomba_geo.glb",
    "actors/thwomp/thwomp_geo.glb",
    "actors/spiny/spiny_geo.glb",
    "actors/boo/boo_geo.glb",
    "actors/king_bobomb/king_bobomb_geo.glb",
    // 6..=11 are the ported species (drawn by `foes_view`).
    "",
    "",
    "",
    "",
    "",
    "",
    "actors/bobomb/black_bobomb_geo.glb",
    "actors/bully/bully_geo.glb",
    "actors/chain_chomp/chain_chomp_geo.glb",
    "actors/bowser/bowser_geo.glb",
];

/// One thing to draw: `kind == 0` hides the slot.
#[derive(Clone, Copy, Default, Debug)]
pub struct RItem {
    pub kind: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub scale: f32,
    pub aux: f32,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Hud {
    pub hp: f32,
    pub max_hp: f32,
    pub xp: u32,
    pub xp_need: u32,
    pub level: u32,
    pub secs: f32,
    pub kills: u32,
    pub hint: bool,
    /// Outside the storm ring (the screen reddens).
    pub storm: bool,
    /// The boss on the field: its kind and the share of its health left.
    pub boss: Option<(u8, f32)>,
    /// Chests waiting to be opened (their cards come after the current ones).
    pub chests: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardView {
    pub name: &'static str,
    pub desc: &'static str,
    pub from: u8,
    pub to: u8,
    pub rare: bool,
    /// An evolution: the best card in the game.
    pub evo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Summary {
    pub secs: f32,
    pub level: u32,
    pub kills: u32,
    pub best: f32,
    pub new_best: bool,
    /// Killed the Star Tyrant rather than died.
    pub won: bool,
    pub bits: u32,
    pub rank_before: u32,
    pub rank_after: u32,
    pub level_name: &'static str,
    pub tier_name: &'static str,
    pub chests: u32,
    pub elites: u32,
    pub bosses: u32,
    /// This win opened the next difficulty.
    pub unlocked_tier: bool,
}

/// A damage number in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumItem {
    pub pos: Vec3,
    pub amount: f32,
    pub crit: bool,
    /// 0 a foe was hit, 1 the player was hurt, 2 the player healed.
    pub kind: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BriefAction {
    Level(i32),
    Tier(i32),
    Start,
}

/// The screen before a run: the battlefield, the difficulty and what the player brings.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Briefing {
    pub level: usize,
    pub level_count: usize,
    pub level_name: &'static str,
    pub blurb: &'static str,
    pub tier: usize,
    pub tier_name: &'static str,
    pub tier_locked: [bool; 3],
    pub best_secs: f32,
    pub wins: u32,
    pub rank: u32,
    pub bits: u32,
    pub next_bits: u32,
    pub hero: &'static str,
}

#[derive(Resource, Default)]
pub struct SwarmBridge {
    pub active: bool,
    pub hud: Hud,
    /// Cards on offer (`card_gen` changes whenever the set does).
    pub cards: Vec<CardView>,
    pub card_gen: u32,
    /// The cards come from a chest.
    pub cards_chest: bool,
    pub cards_ready: bool,
    pub focus: usize,
    /// Set by the UI; the experiment applies it.
    pub pick: Option<usize>,
    pub summary: Option<Summary>,
    pub summary_ready: bool,
    pub retry: bool,
    /// The summary's CHANGE LEVEL button: back to the briefing.
    pub change_setup: bool,
    pub briefing: Option<Briefing>,
    pub brief_action: Option<BriefAction>,
    /// The character in play (kept up to date by [`track_character`] even when the mode is not running).
    pub character: crate::characters::Character,
    pub booms: Vec<RItem>,
    pub beacons: Vec<RItem>,
    pub posts: Vec<RItem>,
    pub nums: Vec<NumItem>,
    /// The upgrades owned: name, level, evolution.
    pub build: Vec<(&'static str, u8, bool)>,
    pub build_gen: u32,
    pub enemies: Vec<RItem>,
    /// Ported OoT / BK species for `foes_view`.
    pub foes: Vec<super::foes_view::FoeItem>,
    pub bolts: Vec<RItem>,
    pub orbs: Vec<RItem>,
    pub gems: Vec<RItem>,
    pub rings: Vec<RItem>,
    pub orbit: Vec<RItem>,
    pub time: f32,
}

impl SwarmBridge {
    /// Size the render lists for a fresh run.
    pub fn reset(&mut self) {
        let fill = |v: &mut Vec<RItem>, n: usize| {
            v.clear();
            v.resize(n, RItem::default());
        };
        self.foes.clear();
        fill(&mut self.enemies, ENEMY_SLOTS);
        fill(&mut self.bolts, BOLT_SLOTS);
        fill(&mut self.orbs, ORB_SLOTS);
        fill(&mut self.gems, GEM_SLOTS);
        fill(&mut self.rings, RING_SLOTS);
        fill(&mut self.orbit, ORBIT_MAX);
        fill(&mut self.booms, BOOM_SLOTS);
        fill(&mut self.beacons, super::swarm::BEACON_SLOTS);
        fill(&mut self.posts, super::swarm::RING_POSTS);
        self.nums.clear();
        self.build.clear();
        self.build_gen = self.build_gen.wrapping_add(1);
        self.cards_chest = false;
        self.cards.clear();
        self.card_gen = self.card_gen.wrapping_add(1);
        self.summary = None;
        self.pick = None;
        self.retry = false;
        self.change_setup = false;
        self.focus = 0;
        self.hud = Hud::default();
    }
}

pub struct SwarmViewPlugin;

impl Plugin for SwarmViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwarmBridge>().init_resource::<Pools>().init_resource::<UiState>().add_systems(Update, track_character).add_systems(
            Update,
            (sync_render, manage_ui, update_hud, update_hud_extra, update_nums, card_input, brief_input, summary_input, style_cards, overlay_display).chain().after(super::run_frame).run_if(|b: Res<SwarmBridge>| b.active),
        );
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<SwarmBridge>| !b.active));
        app.add_systems(Update, push_foes.after(super::run_frame).before(super::foes_view::sync_foes).run_if(|b: Res<SwarmBridge>| b.active));
    }
}

fn track_character(character: Res<crate::characters::Character>, mut bridge: ResMut<SwarmBridge>) {
    if bridge.character != *character {
        bridge.character = *character;
    }
}

fn push_foes(bridge: Res<SwarmBridge>, mut board: ResMut<super::foes_view::FoeBoard>) {
    board.items.extend(bridge.foes.iter().copied());
}

// ---------------------------------------------------------------------------------------------
// World sync
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct Pooled;

struct Shared {
    bolt: (Handle<Mesh>, Handle<StandardMaterial>),
    orb: (Handle<Mesh>, Handle<StandardMaterial>),
    orbit: (Handle<Mesh>, Handle<StandardMaterial>),
    coin: Handle<Mesh>,
    gem_mats: [Handle<StandardMaterial>; 7],
    heart: Handle<Mesh>,
    chest: Handle<Mesh>,
    ring: Handle<Mesh>,
    boom: (Handle<Mesh>, Handle<StandardMaterial>),
    post: (Handle<Mesh>, Handle<StandardMaterial>),
    beacon: Handle<Mesh>,
    beacon_mats: [Handle<StandardMaterial>; 2],
}

#[derive(Resource, Default)]
struct Pools {
    shared: Option<Shared>,
    enemy_slot: Vec<Option<(u8, Entity)>>,
    enemy_free: [Vec<Entity>; 16],
    bolt: Vec<Option<Entity>>,
    orb: Vec<Option<Entity>>,
    gem: Vec<Option<(Entity, u8)>>,
    ring: Vec<Option<(Entity, Handle<StandardMaterial>)>>,
    orbit: Vec<Option<Entity>>,
    boom: Vec<Option<Entity>>,
    post: Vec<Option<Entity>>,
    beacon: Vec<Option<(Entity, u8)>>,
}

fn unlit(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, ..default() })
}

fn build_shared(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Shared {
    let sphere = |meshes: &mut Assets<Mesh>, r: f32| meshes.add(Sphere::new(r * SCALE).mesh().ico(1).unwrap());
    Shared {
        bolt: (sphere(meshes, 42.0), unlit(materials, Color::srgb(1.0, 0.95, 0.45))),
        orb: (sphere(meshes, 38.0), unlit(materials, Color::srgb(0.85, 0.35, 1.0))),
        orbit: (sphere(meshes, 52.0), unlit(materials, Color::srgb(0.45, 0.95, 1.0))),
        coin: meshes.add(Cylinder::new(34.0 * SCALE, 9.0 * SCALE)),
        gem_mats: [unlit(materials, Color::srgb(1.0, 0.85, 0.1)), unlit(materials, Color::srgb(0.3, 0.65, 1.0)), unlit(materials, Color::srgb(1.0, 0.3, 0.3)), unlit(materials, Color::srgb(1.0, 0.45, 0.65)), unlit(materials, Color::srgb(1.0, 0.72, 0.15)), unlit(materials, Color::srgb(0.35, 0.95, 1.0)), unlit(materials, Color::srgb(0.95, 0.25, 0.1))],
        heart: sphere(meshes, 46.0),
        chest: meshes.add(Cuboid::new(110.0 * SCALE, 80.0 * SCALE, 80.0 * SCALE)),
        ring: meshes.add(Annulus::new(0.9, 1.0)),
        boom: (meshes.add(Cuboid::new(190.0 * SCALE, 18.0 * SCALE, 56.0 * SCALE)), unlit(materials, Color::srgb(0.7, 1.0, 0.85))),
        post: (meshes.add(Cuboid::new(34.0 * SCALE, 760.0 * SCALE, 34.0 * SCALE)), unlit(materials, Color::srgb(1.0, 0.45, 0.12))),
        beacon: meshes.add(Cylinder::new(30.0 * SCALE, 1500.0 * SCALE)),
        beacon_mats: [
            materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.3, 0.2, 0.32), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() }),
            materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.85, 0.2, 0.4), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() }),
        ],
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

#[allow(clippy::too_many_arguments)]
fn sync_render(
    mut minimal: ResMut<crate::hud::HudMinimal>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    root: Res<AssetRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<SwarmBridge>,
    mut pools: ResMut<Pools>,
    mut q: Query<(&mut Transform, &mut Visibility), With<Pooled>>,
) {
    if !minimal.0 {
        minimal.0 = true;
    }
    if pools.shared.is_none() {
        pools.shared = Some(build_shared(&mut meshes, &mut materials));
        pools.enemy_slot = vec![None; ENEMY_SLOTS];
        pools.bolt = vec![None; BOLT_SLOTS];
        pools.orb = vec![None; ORB_SLOTS];
        pools.gem = vec![None; GEM_SLOTS];
        pools.ring = (0..RING_SLOTS).map(|_| None).collect();
        pools.orbit = vec![None; ORBIT_MAX];
        pools.boom = vec![None; BOOM_SLOTS];
        pools.post = vec![None; super::swarm::RING_POSTS];
        pools.beacon = vec![None; super::swarm::BEACON_SLOTS];
    }
    let pools = &mut *pools;
    let shared = pools.shared.as_ref().unwrap();
    let hidden = |commands: &mut Commands, bundle: (Mesh3d, MeshMaterial3d<StandardMaterial>)| commands.spawn((XpEntity, Pooled, bundle, Transform::default(), Visibility::Hidden)).id();
    // Enemies: a slot's model can change when it is reused; entities return to a per-model pool.
    for (i, item) in bridge.enemies.iter().enumerate() {
        let current = pools.enemy_slot[i];
        if let Some((k, e)) = current {
            if k != item.kind {
                if let Ok((_, mut v)) = q.get_mut(e) {
                    *v = Visibility::Hidden;
                }
                pools.enemy_free[k as usize].push(e);
                pools.enemy_slot[i] = None;
            }
        }
        if item.kind == 0 || item.kind as usize >= MODELS.len() {
            continue;
        }
        let entity = match pools.enemy_slot[i] {
            Some((_, e)) => e,
            None => {
                let e = pools.enemy_free[item.kind as usize].pop().unwrap_or_else(|| {
                    let path = MODELS[item.kind as usize];
                    if root.exists(path) {
                        // Foe models carry SM64 animation clips; without a playing clip their limbs sit in the raw rest
                        // pose (spiny on its side, bob-omb halves, lying goombas). Same driver the course objects use.
                        let mut foe = crate::render::object_view::spawn_model(&mut commands, &assets, &root, path, true);
                        foe.insert((XpEntity, Pooled, Visibility::Hidden));
                        foe.id()
                    } else {
                        // Missing model: a coloured blob keeps the game playable.
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
    // Stars, orbs, orbit stars: one shared mesh + material each.
    let simple = |list: &[RItem], ents: &mut Vec<Option<Entity>>, pair: &(Handle<Mesh>, Handle<StandardMaterial>), commands: &mut Commands, q: &mut Query<(&mut Transform, &mut Visibility), With<Pooled>>| {
        for (i, item) in list.iter().enumerate() {
            if item.kind == 0 && ents[i].is_none() {
                continue;
            }
            let e = *ents[i].get_or_insert_with(|| hidden(commands, (Mesh3d(pair.0.clone()), MeshMaterial3d(pair.1.clone()))));
            place(q, e, item, Quat::IDENTITY);
        }
    };
    simple(&bridge.bolts, &mut pools.bolt, &shared.bolt, &mut commands, &mut q);
    simple(&bridge.orbs, &mut pools.orb, &shared.orb, &mut commands, &mut q);
    simple(&bridge.orbit, &mut pools.orbit, &shared.orbit, &mut commands, &mut q);
    // Boomerangs spin flat.
    for (i, item) in bridge.booms.iter().enumerate() {
        if item.kind == 0 && pools.boom[i].is_none() {
            continue;
        }
        let e = *pools.boom[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.boom.0.clone()), MeshMaterial3d(shared.boom.1.clone()))));
        place(&mut q, e, item, Quat::from_rotation_y(item.yaw));
    }
    // The storm ring's pillars, and the light pillars over elites and chests (lifted so they stand on the ground).
    for (i, item) in bridge.posts.iter().enumerate() {
        if item.kind == 0 && pools.post[i].is_none() {
            continue;
        }
        let e = *pools.post[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.post.0.clone()), MeshMaterial3d(shared.post.1.clone()))));
        let mut lifted = *item;
        lifted.pos.y += 380.0 * SCALE;
        place(&mut q, e, &lifted, Quat::IDENTITY);
    }
    for (i, item) in bridge.beacons.iter().enumerate() {
        if item.kind == 0 && pools.beacon[i].is_none() {
            continue;
        }
        let which = if item.aux > 0.5 { 1u8 } else { 0 };
        let (e, w) = match pools.beacon[i] {
            Some(x) => x,
            None => (hidden(&mut commands, (Mesh3d(shared.beacon.clone()), MeshMaterial3d(shared.beacon_mats[which as usize].clone()))), which),
        };
        if item.kind != 0 && w != which {
            commands.entity(e).insert(MeshMaterial3d(shared.beacon_mats[which as usize].clone()));
        }
        pools.beacon[i] = Some((e, which));
        let mut lifted = *item;
        lifted.pos.y += 750.0 * SCALE;
        place(&mut q, e, &lifted, Quat::IDENTITY);
    }
    // Gems spin; the colour (and hearts) follow `aux`.
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for (i, item) in bridge.gems.iter().enumerate() {
        if item.kind == 0 && pools.gem[i].is_none() {
            continue;
        }
        let tier = (item.aux as u8).min(6);
        let mesh_for = |tier: u8| match tier {
            3 | 5 | 6 => shared.heart.clone(),
            4 => shared.chest.clone(),
            _ => shared.coin.clone(),
        };
        let (e, t) = match pools.gem[i] {
            Some(x) => x,
            None => (hidden(&mut commands, (Mesh3d(mesh_for(tier)), MeshMaterial3d(shared.gem_mats[tier as usize].clone()))), tier),
        };
        if item.kind != 0 && t != tier {
            commands.entity(e).insert((Mesh3d(mesh_for(tier)), MeshMaterial3d(shared.gem_mats[tier as usize].clone())));
            pools.gem[i] = Some((e, tier));
        } else {
            pools.gem[i] = Some((e, t));
        }
        // Coins spin flat; a chest turns upright and bobs; the rest tumble.
        let rot = if tier == 4 { Quat::from_rotation_y(bridge.time * 1.5 + i as f32) } else if tier >= 3 { Quat::from_rotation_y(bridge.time * 3.0 + i as f32) } else { Quat::from_rotation_y(bridge.time * 3.0 + i as f32) * flat };
        place(&mut q, e, item, rot);
    }
    // Rings: each slot owns a material so it can fade on its own.
    for (i, item) in bridge.rings.iter().enumerate() {
        if item.kind == 0 && pools.ring[i].is_none() {
            continue;
        }
        let (e, mat) = pools.ring[i].get_or_insert_with(|| {
            let mat = unlit_add(&mut materials);
            (hidden(&mut commands, (Mesh3d(shared.ring.clone()), MeshMaterial3d(mat.clone()))), mat)
        });
        if item.kind != 0 {
            if let Some(mut m) = materials.get_mut(&*mat) {
                let c = match item.kind {
                    1 => [0.5, 0.95, 1.0],
                    2 => [1.0, 0.65, 0.2],
                    4 => [1.0, 0.95, 0.45],
                    5 => [0.95, 0.97, 1.0],
                    6 => [1.0, 0.5, 0.15],
                    _ => [1.0, 0.5, 0.8],
                };
                m.base_color = Color::srgba(c[0], c[1], c[2], 0.8 * item.aux);
            }
        }
        place(&mut q, *e, item, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
    }
}

fn unlit_add(materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: Color::srgba(1.0, 1.0, 1.0, 0.8), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })
}

/// Entities are despawned by the framework on exit; forget the handles.
fn cleanup(mut pools: ResMut<Pools>, mut ui: ResMut<UiState>, mut minimal: ResMut<crate::hud::HudMinimal>) {
    // Only undo what this mode did (another experiment may own the flag now).
    if pools.shared.is_some() {
        if minimal.0 {
            minimal.0 = false;
        }
        *pools = Pools::default();
    }
    *ui = UiState::default();
}

// ---------------------------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct Overlay;
#[derive(Component)]
struct HudRoot;
#[derive(Component)]
struct CardsRoot;
#[derive(Component)]
struct SummaryRoot;
#[derive(Component)]
struct TimerText;
#[derive(Component)]
struct KillText;
#[derive(Component)]
struct HpFill;
#[derive(Component)]
struct HpText;
#[derive(Component)]
struct XpFill;
#[derive(Component)]
struct LevelText;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct CardIdx(usize);
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum SummaryButton {
    Retry,
    Setup,
    Menu,
}
/// Hidden while the briefing is up.
#[derive(Component)]
struct HideInBrief;
#[derive(Component)]
struct BossRoot;
#[derive(Component)]
struct BossName;
#[derive(Component)]
struct BossFill;
#[derive(Component)]
struct StormRoot;
#[derive(Component)]
struct BuildRoot;
#[derive(Component)]
struct NumText;
#[derive(Component, Clone, Copy, PartialEq)]
struct BriefButton(BriefAction);

struct ActiveNum {
    e: Entity,
    pos: Vec3,
    born: f32,
}

#[derive(Default)]
struct NumPool {
    free: Vec<Entity>,
    active: Vec<ActiveNum>,
    spawned: usize,
}

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    cards: Option<(Entity, u32)>,
    summary: Option<Entity>,
    brief: Option<(Entity, Briefing)>,
    build: Option<(Entity, u32)>,
    storm: Option<Entity>,
    nums: NumPool,
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

fn bar(p: &mut ChildSpawnerCommands, width: f32, height: f32, fill: Color, fill_marker: impl Component) -> Entity {
    let mut inner = Entity::PLACEHOLDER;
    p.spawn((Node { width: Val::Px(width), height: Val::Px(height), border: UiRect::all(Val::Px(3.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.8)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(height / 2.0))))
        .with_children(|b| {
            inner = b.spawn((fill_marker, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(fill))).id();
        });
    inner
}

fn spawn_hud(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let a = a.hud(); // in-game heads-up element: keeps the pre-v3 look
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 6.0);
    let root = commands.spawn((XpEntity, Overlay, HideInBrief, HudRoot, GlobalZIndex(12), node)).id();
    commands.entity(root).with_children(|p| {
        p.spawn(Node { align_items: AlignItems::Baseline, column_gap: Val::Px(14.0), ..default() }).with_children(|r| {
            r.spawn((TimerText, Text::new("0:00"), font(&a.display, 30.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
            r.spawn((KillText, text(a, "0 KOs", 14.0, GOLD_HI)));
        });
        // Health bar with its number on top.
        p.spawn(Node { width: Val::Px(250.0), height: Val::Px(20.0), margin: UiRect::top(Val::Px(2.0)), ..default() }).with_children(|h| {
            bar(h, 250.0, 20.0, hex(0x39c24a), HpFill);
            h.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|t| {
                t.spawn((HpText, text(a, "100 / 100", 12.0, Color::WHITE)));
            });
        });
        // Level pill + experience bar.
        p.spawn(Node { align_items: AlignItems::Center, column_gap: Val::Px(6.0), margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|r| {
            r.spawn((Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(1.0)), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(GOLD), BorderColor::all(NAVY), BorderRadius::all(Val::Px(99.0))))
                .with_children(|l| {
                    l.spawn((LevelText, Text::new("LV 1"), font(&a.display, 13.0), TextColor(NAVY)));
                });
            bar(r, 190.0, 12.0, hex(0x4fc3ff), XpFill);
        });
        // The boss on the field, with its health.
        p.spawn((BossRoot, Node { display: Display::None, flex_direction: FlexDirection::Column, align_items: AlignItems::Center, margin: UiRect::top(Val::Px(6.0)), row_gap: Val::Px(2.0), ..default() })).with_children(|b| {
            b.spawn((BossName, text(a, "BOSS", 13.0, hex(0xff9a8a))));
            bar(b, 320.0, 14.0, hex(0xe5302a), BossFill);
        });
        crate::hints::note(HINT);
        p.spawn((HintText, text(a, HINT, 12.0, CREAM), Node { margin: UiRect::top(Val::Px(8.0)), max_width: Val::Px(360.0), ..default() }));
    });
    root
}

const HINT: &str = "Weapons fire by themselves - keep moving! Stay inside the glowing ring, grab chests, and slay the Star Tyrant at 7:00. Jump, then ground-pound for a quake; dives hurt foes too.";

fn rarity(rare: bool) -> Color {
    if rare { hex(0xd36cff) } else { hex(0x5aa0ff) }
}

fn evo_color() -> Color {
    hex(0xffb13a)
}

fn spawn_cards(commands: &mut Commands, a: &UiAssets, area: &SafeArea, cards: &[CardView], level: u32, window_w: f32, chest: bool) -> Entity {
    // Keep clear of the touch stick (left) and buttons (right) on phones.
    let card_w = ((window_w - area.left.max(52.0) - area.right.max(52.0) - 2.0 * 112.0) / 3.0 - 12.0).clamp(120.0, 214.0);
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    node.row_gap = Val::Px(6.0);
    node.padding.top = Val::Px(area.top + 84.0);
    let root = commands.spawn((XpEntity, Overlay, CardsRoot, GlobalZIndex(22), node, BackgroundColor(Color::srgba(0.02, 0.04, 0.14, 0.66)))).id();
    commands.entity(root).with_children(|p| {
        if chest {
            outlined(p, a, "CHEST!", 30.0, GOLD);
            p.spawn(text(a, "A rare reward - pick one", 14.0, CREAM));
        } else {
            outlined(p, a, "LEVEL UP!", 30.0, GOLD);
            p.spawn(text(a, &format!("Level {level} - pick an upgrade"), 14.0, CREAM));
        }
        p.spawn(Node { column_gap: Val::Px(10.0), margin: UiRect::vertical(Val::Px(4.0)), width: Val::Vw(62.0), max_width: Val::Px(690.0), ..default() }).with_children(|row| {
            for (i, c) in cards.iter().enumerate() {
                row.spawn((
                    Button,
                    CardIdx(i),
                    Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), max_width: Val::Px(card_w), height: Val::Px(136.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::all(Val::Px(7.0)), row_gap: Val::Px(2.0), border: UiRect::all(Val::Px(4.0)), ..default() },
                    BackgroundColor(NAVY_2),
                    BorderColor::from(if c.evo { evo_color() } else { rarity(c.rare) }),
                    BorderRadius::all(Val::Px(16.0)),
                    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), Val::Px(0.0), Val::Px(5.0), Val::Px(0.0), Val::Px(2.0)),
                ))
                .with_children(|k| {
                    k.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|t| {
                        t.spawn(text(a, &format!("[{}]", i + 1), 11.0, GOLD_HI));
                        t.spawn(text(a, if c.evo { "EVOLUTION" } else if c.rare { "RARE" } else { "" }, 11.0, if c.evo { evo_color() } else { hex(0xe9b6ff) }));
                    });
                    k.spawn((Text::new(clean(c.name.to_string())), font(&a.display, 17.0), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, TextLayout::justify(Justify::Center)));
                    k.spawn(text(a, &if c.evo { "NEW".to_string() } else if c.from == 0 { "NEW".to_string() } else { format!("Lv {} > {}", c.from, c.to) }, 12.0, hex(0x9fe0ff)));
                    k.spawn((Text::new(clean(c.desc.to_string())), font(&a.body, 12.0), TextColor(Color::WHITE), TextLayout::justify(Justify::Center), Node { width: Val::Percent(100.0), ..default() }));
                });
            }
        });
        crate::hints::note("Tap a card, or press 1 / 2 / 3");
        p.spawn((crate::hints::ControlHint, text(a, "Tap a card, or press 1 / 2 / 3", 11.0, CREAM.with_alpha(0.8))));
    });
    root
}

fn spawn_summary(commands: &mut Commands, a: &UiAssets, area: &SafeArea, s: &Summary) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, SummaryRoot, GlobalZIndex(22), node, BackgroundColor(Color::srgba(0.06, 0.0, 0.08, 0.6)))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(30.0), Val::Px(14.0)), row_gap: Val::Px(6.0), width: Val::Px(400.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
            kicker(c, a, &format!("{} - {}", s.level_name, s.tier_name));
            let (title, color) = if s.won { ("TYRANT SLAIN!", GOLD) } else if s.new_best { ("NEW BEST!", GOLD) } else { ("RUN OVER", RED_HI) };
            outlined(c, a, title, 40.0, color);
            if s.won {
                c.spawn(text(a, "The Star Tyrant falls and the swarm scatters.", 14.0, CREAM));
            }
            if s.unlocked_tier {
                c.spawn(text(a, "A HARDER DIFFICULTY IS UNLOCKED!", 14.0, GOLD_HI));
            }
            let mut row = |label: &str, value: String, color: Color| {
                c.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                    r.spawn(text(a, label, 16.0, CREAM));
                    r.spawn((Text::new(clean(value)), font(&a.display, 20.0), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                });
            };
            row("Time survived", clock(s.secs), Color::WHITE);
            row("Level reached", s.level.to_string(), Color::WHITE);
            row("Foes defeated", s.kills.to_string(), Color::WHITE);
            row("Chests opened", s.chests.to_string(), Color::WHITE);
            row("Best time here", clock(s.best), GOLD_HI);
            row("Star bits earned", format!("+{}", s.bits), hex(0x9fe0ff));
            if s.rank_after > s.rank_before {
                c.spawn(text(a, &format!("RANK UP! You are now rank {}: stronger from the start.", s.rank_after), 14.0, GOLD_HI));
            } else {
                c.spawn(text(a, &format!("Rank {}", s.rank_after), 13.0, CREAM));
            }
            c.spawn(Node { column_gap: Val::Px(8.0), margin: UiRect::top(Val::Px(8.0)), flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center, row_gap: Val::Px(6.0), ..default() }).with_children(|r| {
                for (label, kind, which, w) in [("RETRY", ButtonKind::Gold, SummaryButton::Retry, 130.0), ("LEVEL / TIER", ButtonKind::Green, SummaryButton::Setup, 150.0), ("MAIN MENU", ButtonKind::Blue, SummaryButton::Menu, 130.0)] {
                    let mut b = button(r, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(44.0), 16.0), label);
                    b.insert(which);
                }
            });
        });
    });
    root
}

fn spawn_briefing(commands: &mut Commands, a: &UiAssets, area: &SafeArea, b: &Briefing) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, GlobalZIndex(23), node, BackgroundColor(Color::srgba(0.02, 0.04, 0.14, 0.45)))).id();
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(26.0), Val::Px(14.0)), row_gap: Val::Px(6.0), width: Val::Px(430.0), max_width: Val::Percent(96.0), align_items: AlignItems::Center, ..default() }).with_children(|c| {
            kicker(c, a, "STAR SWARM");
            outlined(c, a, "CHOOSE YOUR BATTLEFIELD", 22.0, GOLD);
            // Battlefield picker.
            c.spawn(Node { width: Val::Percent(100.0), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, column_gap: Val::Px(8.0), ..default() }).with_children(|r| {
                let mut l = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(46.0), Val::Px(42.0), 22.0), "<");
                l.insert(BriefButton(BriefAction::Level(-1)));
                r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_grow: 1.0, ..default() }).with_children(|m| {
                    m.spawn((Text::new(clean(b.level_name.to_string())), font(&a.display, 21.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, TextLayout::justify(Justify::Center)));
                    m.spawn(text(a, &format!("{} / {}", b.level + 1, b.level_count), 11.0, CREAM.with_alpha(0.8)));
                });
                let mut rb = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(46.0), Val::Px(42.0), 22.0), ">");
                rb.insert(BriefButton(BriefAction::Level(1)));
            });
            c.spawn((Text::new(clean(b.blurb.to_string())), font(&a.body, 12.0), TextColor(CREAM), TextLayout::justify(Justify::Center)));
            // Difficulty.
            c.spawn(Node { width: Val::Percent(100.0), align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, column_gap: Val::Px(8.0), margin: UiRect::top(Val::Px(4.0)), ..default() }).with_children(|r| {
                let mut l = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(46.0), Val::Px(36.0), 20.0), "-");
                l.insert(BriefButton(BriefAction::Tier(-1)));
                let next_locked = b.tier + 1 < b.tier_locked.len() && b.tier_locked[b.tier + 1];
                r.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, flex_grow: 1.0, ..default() }).with_children(|m| {
                    m.spawn((Text::new(clean(format!("DIFFICULTY: {}", b.tier_name))), font(&a.display, 18.0), TextColor(if b.tier == 0 { Color::WHITE } else if b.tier == 1 { hex(0xffb13a) } else { RED_HI }), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                    if next_locked {
                        m.spawn(text(a, "Win on this difficulty to unlock the next", 11.0, CREAM.with_alpha(0.8)));
                    }
                });
                let mut rb = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(46.0), Val::Px(36.0), 20.0), "+");
                rb.insert(BriefButton(BriefAction::Tier(1)));
            });
            c.spawn(text(a, &format!("Best here {}   Wins {}", clock(b.best_secs), b.wins), 14.0, GOLD_HI));
            let rank_line = if b.next_bits == 0 { format!("RANK {} (MAX)   {} star bits", b.rank, b.bits) } else { format!("RANK {}   {} / {} star bits to the next rank", b.rank, b.bits, b.next_bits) };
            c.spawn(text(a, &rank_line, 12.0, hex(0x9fe0ff)));
            c.spawn(text(a, &format!("You start with: {}", b.hero), 12.0, CREAM));
            let mut go = button(c, a, ButtonSpec::new(ButtonKind::Gold, Val::Px(220.0), Val::Px(48.0), 22.0), "START");
            go.insert(BriefButton(BriefAction::Start));
            crate::hints::note("Left / Right: battlefield   Up / Down: difficulty   Enter: start");
            c.spawn((crate::hints::ControlHint, text(a, "Left / Right: battlefield   Up / Down: difficulty   Enter: start", 11.0, CREAM.with_alpha(0.8))));
        });
    });
    root
}

fn brief_input(mut bridge: ResMut<SwarmBridge>, buttons: Query<(&Interaction, &BriefButton), Changed<Interaction>>, pads: Query<&Gamepad>, state: Res<AppState>) {
    if bridge.briefing.is_none() || *state != AppState::Playing {
        return;
    }
    for (i, b) in &buttons {
        if *i == Interaction::Pressed {
            bridge.brief_action = Some(b.0);
        }
    }
    for pad in &pads {
        if pad.just_pressed(GamepadButton::DPadLeft) {
            bridge.brief_action = Some(BriefAction::Level(-1));
        }
        if pad.just_pressed(GamepadButton::DPadRight) {
            bridge.brief_action = Some(BriefAction::Level(1));
        }
        if pad.just_pressed(GamepadButton::DPadUp) {
            bridge.brief_action = Some(BriefAction::Tier(1));
        }
        if pad.just_pressed(GamepadButton::DPadDown) {
            bridge.brief_action = Some(BriefAction::Tier(-1));
        }
        if pad.just_pressed(GamepadButton::South) {
            bridge.brief_action = Some(BriefAction::Start);
        }
    }
}

/// Boss bar, storm tint and the owned-upgrade list: small pieces of the heads-up display that change often.
#[allow(clippy::type_complexity)]
fn update_hud_extra(
    mut commands: Commands,
    bridge: Res<SwarmBridge>,
    assets: Option<Res<UiAssets>>,
    area: Res<SafeArea>,
    mut ui: ResMut<UiState>,
    mut boss_root: Query<&mut Node, (With<BossRoot>, Without<BossFill>)>,
    mut boss_fill: Query<&mut Node, (With<BossFill>, Without<BossRoot>)>,
    mut boss_name: Query<&mut Text, With<BossName>>,
    mut storm: Query<(&mut BackgroundColor, &mut BorderColor), With<StormRoot>>,
) {
    let Some(a) = assets else { return };
    let h = bridge.hud;
    for mut n in &mut boss_root {
        let d = if h.boss.is_some() { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    }
    if let Some((kind, frac)) = h.boss {
        for mut n in &mut boss_fill {
            let w = Val::Percent(frac.clamp(0.0, 1.0) * 100.0);
            if n.width != w {
                n.width = w;
            }
        }
        let name = match EnemyKind::from_u8(kind) {
            Some(EnemyKind::King) => "KING BOB-OMB",
            Some(EnemyKind::Chomp) => "CHOMP",
            Some(EnemyKind::Tyrant) => "THE STAR TYRANT",
            _ => "BOSS",
        };
        for mut t in &mut boss_name {
            if t.0 != name {
                t.0 = name.to_string();
            }
        }
    }
    // The storm: a red frame that pulses while you stand outside the ring.
    if ui.storm.is_none() {
        let a = a.hud();
        let _ = a;
        ui.storm = Some(commands.spawn((XpEntity, Overlay, StormRoot, GlobalZIndex(11), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), border: UiRect::all(Val::Px(16.0)), ..default() }, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE))).id());
    }
    let pulse = if h.storm { 0.55 + 0.45 * (bridge.time * 8.0).sin().abs() } else { 0.0 };
    for (mut bg, mut border) in &mut storm {
        let (b, f) = (Color::srgba(0.95, 0.1, 0.05, 0.12 * pulse), Color::srgba(1.0, 0.15, 0.05, 0.55 * pulse));
        if bg.0 != b {
            bg.0 = b;
        }
        if border.top != f {
            *border = BorderColor::all(f);
        }
    }
    // The build list follows the upgrades the player owns.
    if ui.build.is_none_or(|(_, g)| g != bridge.build_gen) {
        if let Some((e, _)) = ui.build {
            commands.entity(e).despawn();
        }
        let a = a.hud();
        let root = commands.spawn((XpEntity, Overlay, HideInBrief, BuildRoot, GlobalZIndex(10), Node { position_type: PositionType::Absolute, left: Val::Px(area.left.max(8.0)), top: Val::Px(area.top.max(0.0) + 112.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(1.0), ..default() })).id();
        commands.entity(root).with_children(|p| {
            for (name, lv, evo) in bridge.build.iter() {
                let shown = name.strip_prefix("EVOLVE: ").unwrap_or(name);
                p.spawn(text(a, &format!("{shown} {lv}"), 11.0, if *evo { evo_color() } else { Color::WHITE.with_alpha(0.85) }));
            }
        });
        ui.build = Some((root, bridge.build_gen));
    }
}

/// Damage numbers: pooled text that rises and fades over where the hit landed.
#[allow(clippy::too_many_arguments)]
fn update_nums(
    mut commands: Commands,
    assets: Option<Res<UiAssets>>,
    mut bridge: ResMut<SwarmBridge>,
    mut ui: ResMut<UiState>,
    cams: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    mut texts: Query<(&mut Node, &mut Text, &mut TextColor, &mut TextFont, &mut Visibility), With<NumText>>,
) {
    use std::fmt::Write;
    let (Some(a), Ok((cam, gt))) = (assets, cams.single()) else {
        bridge.nums.clear();
        return;
    };
    let a = a.hud();
    let now = bridge.time;
    let pool = &mut ui.nums;
    for k in 0..bridge.nums.len() {
        let n = bridge.nums[k];
        let e = match pool.free.pop() {
            Some(e) => e,
            None if pool.spawned < 64 => {
                pool.spawned += 1;
                commands.spawn((XpEntity, Overlay, NumText, GlobalZIndex(13), Text::new(String::with_capacity(8)), font(&a.display, 16.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden)).id()
            }
            None => continue,
        };
        if let Ok((_, mut t, mut c, mut f, mut v)) = texts.get_mut(e) {
            t.0.clear();
            let v_int = n.amount.round() as i32;
            let _ = match (n.kind, n.crit) {
                (1, _) => write!(t.0, "-{v_int}"),
                (2, _) => write!(t.0, "+{v_int}"),
                (_, true) => write!(t.0, "{v_int}!"),
                _ => write!(t.0, "{v_int}"),
            };
            c.0 = match (n.kind, n.crit) {
                (1, _) => Color::srgb(1.0, 0.35, 0.3),
                (2, _) => Color::srgb(0.4, 1.0, 0.5),
                (_, true) => Color::srgb(1.0, 0.85, 0.2),
                _ => Color::WHITE,
            };
            f.font_size = FontSize::Px(match (n.kind, n.crit) {
                (1, _) => 24.0,
                (_, true) => 24.0,
                _ => 15.0,
            });
            *v = Visibility::Inherited;
        }
        pool.active.push(ActiveNum { e, pos: n.pos, born: now });
    }
    bridge.nums.clear();
    let mut i = 0;
    while i < pool.active.len() {
        let an = &pool.active[i];
        let age = now - an.born;
        let done = !(0.0..0.8).contains(&age);
        let Ok((mut node, _, mut color, _, mut vis)) = texts.get_mut(an.e) else {
            pool.active.swap_remove(i);
            continue;
        };
        if done {
            *vis = Visibility::Hidden;
            pool.free.push(an.e);
            pool.active.swap_remove(i);
            continue;
        }
        let world = an.pos + Vec3::Y * (age * 2.2);
        match cam.world_to_viewport(gt, world) {
            Ok(px) => {
                node.left = Val::Px(px.x - 12.0);
                node.top = Val::Px(px.y - 10.0 - age * 40.0);
                let alpha = if age > 0.5 { 1.0 - (age - 0.5) / 0.3 } else { 1.0 };
                color.0 = color.0.with_alpha(alpha);
                *vis = Visibility::Inherited;
            }
            Err(_) => *vis = Visibility::Hidden,
        }
        i += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn manage_ui(
    mut commands: Commands,
    bridge: Res<SwarmBridge>,
    assets: Option<Res<UiAssets>>,
    area: Res<SafeArea>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ui: ResMut<UiState>,
) {
    let Some(a) = assets else { return };
    if ui.hud.is_none() {
        ui.hud = Some(spawn_hud(&mut commands, &a, &area));
    }
    // Cards: rebuilt whenever the offer changes.
    let want_cards = !bridge.cards.is_empty();
    match (ui.cards, want_cards) {
        (Some((e, gen)), true) if gen != bridge.card_gen => {
            commands.entity(e).despawn();
            ui.cards = Some((spawn_cards(&mut commands, &a, &area, &bridge.cards, bridge.hud.level, windows.single().map_or(874.0, |w| w.width()), bridge.cards_chest), bridge.card_gen));
        }
        (None, true) => {
            ui.cards = Some((spawn_cards(&mut commands, &a, &area, &bridge.cards, bridge.hud.level, windows.single().map_or(874.0, |w| w.width()), bridge.cards_chest), bridge.card_gen));
        }
        (Some((e, _)), false) => {
            commands.entity(e).despawn();
            ui.cards = None;
        }
        _ => {}
    }
    match (ui.brief, bridge.briefing) {
        (None, Some(b)) => ui.brief = Some((spawn_briefing(&mut commands, &a, &area, &b), b)),
        (Some((e, old)), Some(b)) if old != b => {
            commands.entity(e).despawn();
            ui.brief = Some((spawn_briefing(&mut commands, &a, &area, &b), b));
        }
        (Some((e, _)), None) => {
            commands.entity(e).despawn();
            ui.brief = None;
        }
        _ => {}
    }
    match (ui.summary, bridge.summary) {
        (None, Some(s)) => ui.summary = Some(spawn_summary(&mut commands, &a, &area, &s)),
        (Some(e), None) => {
            commands.entity(e).despawn();
            ui.summary = None;
        }
        _ => {}
    }
}

#[allow(clippy::type_complexity)]
fn update_hud(
    bridge: Res<SwarmBridge>,
    mut timer: Query<&mut Text, (With<TimerText>, Without<KillText>, Without<HpText>, Without<LevelText>)>,
    mut kills: Query<&mut Text, (With<KillText>, Without<TimerText>, Without<HpText>, Without<LevelText>)>,
    mut hp_text: Query<&mut Text, (With<HpText>, Without<TimerText>, Without<KillText>, Without<LevelText>)>,
    mut level: Query<&mut Text, (With<LevelText>, Without<TimerText>, Without<KillText>, Without<HpText>)>,
    mut hp_fill: Query<(&mut Node, &mut BackgroundColor), (With<HpFill>, Without<XpFill>, Without<HintText>)>,
    mut xp_fill: Query<&mut Node, (With<XpFill>, Without<HpFill>, Without<HintText>)>,
    mut hint: Query<&mut Node, (With<HintText>, Without<HpFill>, Without<XpFill>)>,
) {
    let h = bridge.hud;
    let set = |t: &mut Text, s: String| {
        if t.0 != s {
            t.0 = s;
        }
    };
    for mut t in &mut timer {
        set(&mut t, format!("{} / {}", clock(h.secs), clock(WIN_SECS)));
    }
    for mut t in &mut kills {
        set(&mut t, format!("{} KOs", h.kills));
    }
    for mut t in &mut hp_text {
        set(&mut t, format!("{} / {}", h.hp.max(0.0).ceil() as i32, h.max_hp.ceil() as i32));
    }
    for mut t in &mut level {
        set(&mut t, format!("LV {}", h.level));
    }
    let frac = if h.max_hp > 0.0 { (h.hp / h.max_hp).clamp(0.0, 1.0) } else { 0.0 };
    for (mut n, mut c) in &mut hp_fill {
        let w = Val::Percent(frac * 100.0);
        if n.width != w {
            n.width = w;
        }
        let col = if frac > 0.5 { hex(0x39c24a) } else if frac > 0.25 { hex(0xf0b429) } else { hex(0xe5302a) };
        if c.0 != col {
            c.0 = col;
        }
    }
    let xf = if h.xp_need > 0 { (h.xp as f32 / h.xp_need as f32).clamp(0.0, 1.0) } else { 0.0 };
    for mut n in &mut xp_fill {
        let w = Val::Percent(xf * 100.0);
        if n.width != w {
            n.width = w;
        }
    }
    for mut n in &mut hint {
        let d = if h.hint && crate::hints::show() { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    }
}

fn card_input(
    mut bridge: ResMut<SwarmBridge>,
    cards: Query<(&Interaction, &CardIdx), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    state: Res<AppState>,
) {
    if bridge.cards.is_empty() || *state != AppState::Playing {
        return;
    }
    let n = bridge.cards.len();
    for (i, idx) in &cards {
        if *i == Interaction::Pressed {
            bridge.pick = Some(idx.0);
        }
    }
    let (mut left, mut right, mut confirm) = (keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA), keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD), keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space));
    for pad in &pads {
        left |= pad.just_pressed(GamepadButton::DPadLeft);
        right |= pad.just_pressed(GamepadButton::DPadRight);
        confirm |= pad.just_pressed(GamepadButton::South);
    }
    if left {
        bridge.focus = (bridge.focus + n - 1) % n;
    }
    if right {
        bridge.focus = (bridge.focus + 1) % n;
    }
    if confirm {
        bridge.pick = Some(bridge.focus.min(n - 1));
    }
}

fn summary_input(bridge: ResMut<SwarmBridge>, buttons: Query<(&Interaction, &SummaryButton), Changed<Interaction>>, pads: Query<&Gamepad>, state: Res<AppState>, mut menu: MessageWriter<ReturnToMenu>) {
    let mut bridge = bridge;
    if bridge.summary.is_none() || !bridge.summary_ready || *state != AppState::Playing {
        return;
    }
    for (i, b) in &buttons {
        if *i == Interaction::Pressed {
            match b {
                SummaryButton::Retry => bridge.retry = true,
                SummaryButton::Setup => bridge.change_setup = true,
                SummaryButton::Menu => {
                    menu.write(ReturnToMenu);
                }
            }
        }
    }
    if pads.iter().any(|p| p.just_pressed(GamepadButton::South)) {
        bridge.retry = true;
    }
    // B leaves for the hub (the MAIN MENU button).
    if pads.iter().any(|p| p.just_pressed(GamepadButton::East)) {
        menu.write(ReturnToMenu);
    }
}

fn style_cards(bridge: Res<SwarmBridge>, mut cards: Query<(&CardIdx, &mut BorderColor, &mut BackgroundColor, &Interaction)>) {
    for (idx, mut border, mut bg, interaction) in &mut cards {
        let rare = bridge.cards.get(idx.0).is_some_and(|c| c.rare);
        let evo = bridge.cards.get(idx.0).is_some_and(|c| c.evo);
        let focused = idx.0 == bridge.focus || *interaction == Interaction::Hovered;
        let b = if focused && bridge.cards_ready { GOLD_HI } else if evo { evo_color() } else { rarity(rare) };
        let f = if !bridge.cards_ready { hex(0x101a44) } else if focused { hex(0x24409a) } else { NAVY_2 };
        if border.top != b {
            *border = BorderColor::all(b);
        }
        if bg.0 != f {
            bg.0 = f;
        }
    }
}

/// Menus (pause, settings) draw over the game; hide ours meanwhile so it cannot take their taps.
fn overlay_display(state: Res<AppState>, bridge: Res<SwarmBridge>, mut roots: Query<(&mut Node, Has<HideInBrief>), With<Overlay>>) {
    for (mut n, hide) in &mut roots {
        let d = if *state == AppState::Playing && !(hide && bridge.briefing.is_some()) { Display::Flex } else { Display::None };
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
        let mut b = SwarmBridge::default();
        b.pick = Some(1);
        b.retry = true;
        b.cards.push(CardView { name: "x", desc: "y", from: 0, to: 1, rare: false, evo: false });
        b.reset();
        assert_eq!((b.enemies.len(), b.bolts.len(), b.orbs.len(), b.gems.len(), b.rings.len(), b.orbit.len()), (ENEMY_SLOTS, BOLT_SLOTS, ORB_SLOTS, GEM_SLOTS, RING_SLOTS, ORBIT_MAX));
        assert_eq!((b.booms.len(), b.beacons.len(), b.posts.len()), (BOOM_SLOTS, super::super::swarm::BEACON_SLOTS, super::super::swarm::RING_POSTS));
        assert!(b.cards.is_empty() && b.pick.is_none() && !b.retry && b.summary.is_none());
        assert!(b.enemies.iter().all(|e| e.kind == 0));
    }

    #[test]
    fn every_enemy_kind_has_a_model_and_every_card_is_ascii() {
        for k in (1..=5u8).chain(12..=15) {
            assert!(EnemyKind::from_u8(k).is_some());
            assert!(MODELS[k as usize].ends_with(".glb"));
        }
        // Every kind has a slot in the model table (the ported species are drawn elsewhere and leave it empty).
        for k in 1..MODELS.len() as u8 {
            assert!(EnemyKind::from_u8(k).is_some(), "kind {k}");
        }
        for u in ALL_UPGRADES {
            let i = u.info();
            assert!(clean(i.name.to_string()).is_ascii() && clean(i.desc.to_string()).is_ascii(), "{}", i.name);
        }
    }
}

#[cfg(test)]
mod pad_tests {
    use super::*;
    use crate::last_input::testing::*;

    fn card(name: &'static str) -> CardView {
        CardView { name, desc: "", from: 0, to: 1, rare: false, evo: false }
    }

    #[test]
    fn level_up_cards_are_picked_with_the_dpad_and_a() {
        let (mut app, pad) = base_app();
        app.init_resource::<SwarmBridge>().add_systems(Update, card_input);
        app.world_mut().resource_mut::<SwarmBridge>().cards = vec![card("a"), card("b"), card("c")];
        tap_update(&mut app, pad, GamepadButton::DPadRight);
        tap_update(&mut app, pad, GamepadButton::DPadRight);
        tap_update(&mut app, pad, GamepadButton::South);
        assert_eq!(app.world().resource::<SwarmBridge>().pick, Some(2));
    }

    #[test]
    fn the_summary_retries_with_a_and_leaves_with_b() {
        let (mut app, pad) = base_app();
        app.init_resource::<SwarmBridge>().add_systems(Update, summary_input);
        {
            let mut b = app.world_mut().resource_mut::<SwarmBridge>();
            b.summary = Some(Summary { secs: 1.0, level: 1, ..Default::default() });
            b.summary_ready = true;
        }
        tap_update(&mut app, pad, GamepadButton::South);
        assert!(app.world().resource::<SwarmBridge>().retry);
        tap_update(&mut app, pad, GamepadButton::East);
        assert!(left_to_hub(&mut app));
    }
}
