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
pub const MODELS: [&str; 6] = ["", "actors/goomba/goomba_geo.glb", "actors/thwomp/thwomp_geo.glb", "actors/spiny/spiny_geo.glb", "actors/boo/boo_geo.glb", "actors/king_bobomb/king_bobomb_geo.glb"];

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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardView {
    pub name: &'static str,
    pub desc: &'static str,
    pub from: u8,
    pub to: u8,
    pub rare: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub secs: f32,
    pub level: u32,
    pub kills: u32,
    pub best: f32,
    pub new_best: bool,
    /// Survived to the dawn rather than died.
    pub won: bool,
}

#[derive(Resource, Default)]
pub struct SwarmBridge {
    pub active: bool,
    pub hud: Hud,
    /// Cards on offer (`card_gen` changes whenever the set does).
    pub cards: Vec<CardView>,
    pub card_gen: u32,
    pub cards_ready: bool,
    pub focus: usize,
    /// Set by the UI; the experiment applies it.
    pub pick: Option<usize>,
    pub summary: Option<Summary>,
    pub summary_ready: bool,
    pub retry: bool,
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
        self.cards.clear();
        self.card_gen = self.card_gen.wrapping_add(1);
        self.summary = None;
        self.pick = None;
        self.retry = false;
        self.focus = 0;
        self.hud = Hud::default();
    }
}

pub struct SwarmViewPlugin;

impl Plugin for SwarmViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwarmBridge>().init_resource::<Pools>().init_resource::<UiState>().add_systems(
            Update,
            (sync_render, manage_ui, update_hud, card_input, summary_input, style_cards, overlay_display).chain().after(super::run_frame).run_if(|b: Res<SwarmBridge>| b.active),
        );
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<SwarmBridge>| !b.active));
        app.add_systems(Update, push_foes.after(super::run_frame).before(super::foes_view::sync_foes).run_if(|b: Res<SwarmBridge>| b.active));
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
    gem_mats: [Handle<StandardMaterial>; 4],
    heart: Handle<Mesh>,
    ring: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct Pools {
    shared: Option<Shared>,
    enemy_slot: Vec<Option<(u8, Entity)>>,
    enemy_free: [Vec<Entity>; 6],
    bolt: Vec<Option<Entity>>,
    orb: Vec<Option<Entity>>,
    gem: Vec<Option<(Entity, u8)>>,
    ring: Vec<Option<(Entity, Handle<StandardMaterial>)>>,
    orbit: Vec<Option<Entity>>,
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
        gem_mats: [unlit(materials, Color::srgb(1.0, 0.85, 0.1)), unlit(materials, Color::srgb(0.3, 0.65, 1.0)), unlit(materials, Color::srgb(1.0, 0.3, 0.3)), unlit(materials, Color::srgb(1.0, 0.45, 0.65))],
        heart: sphere(meshes, 46.0),
        ring: meshes.add(Annulus::new(0.9, 1.0)),
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
    // Gems spin; the colour (and hearts) follow `aux`.
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for (i, item) in bridge.gems.iter().enumerate() {
        if item.kind == 0 && pools.gem[i].is_none() {
            continue;
        }
        let tier = item.aux as u8;
        let (e, t) = match pools.gem[i] {
            Some(x) => x,
            None => {
                let mesh = if tier == 3 { shared.heart.clone() } else { shared.coin.clone() };
                (hidden(&mut commands, (Mesh3d(mesh), MeshMaterial3d(shared.gem_mats[tier as usize].clone()))), tier)
            }
        };
        if item.kind != 0 && t != tier {
            let mesh = if tier == 3 { shared.heart.clone() } else { shared.coin.clone() };
            commands.entity(e).insert((Mesh3d(mesh), MeshMaterial3d(shared.gem_mats[tier as usize].clone())));
            pools.gem[i] = Some((e, tier));
        } else {
            pools.gem[i] = Some((e, t));
        }
        place(&mut q, e, item, Quat::from_rotation_y(bridge.time * 3.0 + i as f32) * flat);
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
    Menu,
}

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    cards: Option<(Entity, u32)>,
    summary: Option<Entity>,
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
    let mut node = root_node(area);
    node.padding.top = Val::Px(area.top.max(0.0) + 6.0);
    let root = commands.spawn((XpEntity, Overlay, HudRoot, GlobalZIndex(12), node)).id();
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
        crate::hints::note("Stars fire by themselves - keep moving! Survive until 8:00. Jump, then ground-pound for a quake.");
        p.spawn((HintText, text(a, "Stars fire by themselves - keep moving! Survive until 8:00. Jump, then ground-pound for a quake.", 12.0, CREAM), Node { margin: UiRect::top(Val::Px(8.0)), max_width: Val::Px(360.0), ..default() }));
    });
    root
}

fn rarity(rare: bool) -> Color {
    if rare { hex(0xd36cff) } else { hex(0x5aa0ff) }
}

fn spawn_cards(commands: &mut Commands, a: &UiAssets, area: &SafeArea, cards: &[CardView], level: u32, window_w: f32) -> Entity {
    // Keep clear of the touch stick (left) and buttons (right) on phones.
    let card_w = ((window_w - area.left.max(52.0) - area.right.max(52.0) - 2.0 * 112.0) / 3.0 - 12.0).clamp(120.0, 214.0);
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    node.row_gap = Val::Px(6.0);
    node.padding.top = Val::Px(area.top + 84.0);
    let root = commands.spawn((XpEntity, Overlay, CardsRoot, GlobalZIndex(22), node, BackgroundColor(Color::srgba(0.02, 0.04, 0.14, 0.66)))).id();
    commands.entity(root).with_children(|p| {
        outlined(p, a, "LEVEL UP!", 30.0, GOLD);
        p.spawn(text(a, &format!("Level {level} - pick an upgrade"), 14.0, CREAM));
        p.spawn(Node { column_gap: Val::Px(10.0), margin: UiRect::vertical(Val::Px(4.0)), width: Val::Vw(62.0), max_width: Val::Px(690.0), ..default() }).with_children(|row| {
            for (i, c) in cards.iter().enumerate() {
                row.spawn((
                    Button,
                    CardIdx(i),
                    Node { flex_grow: 1.0, flex_basis: Val::Px(0.0), min_width: Val::Px(0.0), max_width: Val::Px(card_w), height: Val::Px(136.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::all(Val::Px(7.0)), row_gap: Val::Px(2.0), border: UiRect::all(Val::Px(4.0)), ..default() },
                    BackgroundColor(NAVY_2),
                    BorderColor::from(rarity(c.rare)),
                    BorderRadius::all(Val::Px(16.0)),
                    BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), Val::Px(0.0), Val::Px(5.0), Val::Px(0.0), Val::Px(2.0)),
                ))
                .with_children(|k| {
                    k.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|t| {
                        t.spawn(text(a, &format!("[{}]", i + 1), 11.0, GOLD_HI));
                        t.spawn(text(a, if c.rare { "RARE" } else { "" }, 11.0, hex(0xe9b6ff)));
                    });
                    k.spawn((Text::new(clean(c.name.to_string())), font(&a.display, 17.0), TextColor(GOLD_HI), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }, TextLayout::justify(Justify::Center)));
                    k.spawn(text(a, &format!("Lv {} > {}", c.from, c.to), 12.0, hex(0x9fe0ff)));
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
            kicker(c, a, "STAR SWARM");
            let (title, color) = if s.won { ("YOU SURVIVED!", GOLD) } else if s.new_best { ("NEW BEST!", GOLD) } else { ("RUN OVER", RED_HI) };
            outlined(c, a, title, 40.0, color);
            if s.won {
                c.spawn(text(a, "The dawn breaks and the swarm flees.", 14.0, CREAM));
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
            row("Best time", clock(s.best), GOLD_HI);
            c.spawn(Node { column_gap: Val::Px(12.0), margin: UiRect::top(Val::Px(8.0)), ..default() }).with_children(|r| {
                for (label, kind, which, w) in [("RETRY", ButtonKind::Gold, SummaryButton::Retry, 170.0), ("MAIN MENU", ButtonKind::Blue, SummaryButton::Menu, 150.0)] {
                    let mut b = button(r, a, ButtonSpec::new(kind, Val::Px(w), Val::Px(46.0), 20.0), label);
                    b.insert(which);
                }
            });
        });
    });
    root
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
            ui.cards = Some((spawn_cards(&mut commands, &a, &area, &bridge.cards, bridge.hud.level, windows.single().map_or(874.0, |w| w.width())), bridge.card_gen));
        }
        (None, true) => {
            ui.cards = Some((spawn_cards(&mut commands, &a, &area, &bridge.cards, bridge.hud.level, windows.single().map_or(874.0, |w| w.width())), bridge.card_gen));
        }
        (Some((e, _)), false) => {
            commands.entity(e).despawn();
            ui.cards = None;
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
                SummaryButton::Menu => {
                    menu.write(ReturnToMenu);
                }
            }
        }
    }
    if pads.iter().any(|p| p.just_pressed(GamepadButton::South)) {
        bridge.retry = true;
    }
}

fn style_cards(bridge: Res<SwarmBridge>, mut cards: Query<(&CardIdx, &mut BorderColor, &mut BackgroundColor, &Interaction)>) {
    for (idx, mut border, mut bg, interaction) in &mut cards {
        let rare = bridge.cards.get(idx.0).is_some_and(|c| c.rare);
        let focused = idx.0 == bridge.focus || *interaction == Interaction::Hovered;
        let b = if focused && bridge.cards_ready { GOLD_HI } else { rarity(rare) };
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
        let mut b = SwarmBridge::default();
        b.pick = Some(1);
        b.retry = true;
        b.cards.push(CardView { name: "x", desc: "y", from: 0, to: 1, rare: false });
        b.reset();
        assert_eq!((b.enemies.len(), b.bolts.len(), b.orbs.len(), b.gems.len(), b.rings.len(), b.orbit.len()), (ENEMY_SLOTS, BOLT_SLOTS, ORB_SLOTS, GEM_SLOTS, RING_SLOTS, ORBIT_MAX));
        assert!(b.cards.is_empty() && b.pick.is_none() && !b.retry && b.summary.is_none());
        assert!(b.enemies.iter().all(|e| e.kind == 0));
    }

    #[test]
    fn every_enemy_kind_has_a_model_and_every_card_is_ascii() {
        for k in 1..=5u8 {
            assert!(EnemyKind::from_u8(k).is_some());
            assert!(MODELS[k as usize].ends_with(".glb"));
        }
        for u in ALL_UPGRADES {
            let i = u.info();
            assert!(clean(i.name.to_string()).is_ascii() && clean(i.desc.to_string()).is_ascii(), "{}", i.name);
        }
    }
}
