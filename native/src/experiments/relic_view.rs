//! Relic Descent presentation, world half: the [`RelicBridge`] resource that connects the
//! experiment (`relic.rs`, which owns the rules) to bevy systems, plus the render sync (pooled
//! enemy models, shared simple meshes for shots, drops and effects), the world-projected overlays
//! (damage numbers, loot labels, enemy health bars) and the input gathering (touch buttons,
//! gamepad). The HUD, the pack / skill-tree screens and the run summary are in `relic_ui.rs`.
//!
//! Entities are pooled per slot and only ever hidden or moved while a floor lasts, so a kill
//! or a pickup costs no scene instantiation. Identical mesh + material pairs batch in Bevy.
use super::relic_core::*;
use super::relic_loot::*;
use super::swarm_view::RItem;
use super::XpEntity;
use crate::characters::Character;
use crate::render::{AssetRoot, SCALE};
use crate::touch::TouchControls;
use bevy::prelude::*;

/// Actor model per enemy kind (index = `EnemyKind as u8`, 0 unused).
pub const MODELS: [&str; 8] = ["", "actors/goomba/goomba_geo.glb", "actors/bully/bully_geo.glb", "actors/spiny/spiny_geo.glb", "actors/boo/boo_geo.glb", "actors/bobomb/black_bobomb_geo.glb", "actors/mr_i_eyeball/mr_i_geo.glb", "actors/king_bobomb/king_bobomb_geo.glb"];
pub const CHEST_BASE: &str = "actors/treasure_chest/treasure_chest_base_geo.glb";
pub const CHEST_LID: &str = "actors/treasure_chest/treasure_chest_lid_geo.glb";

/// Which full-screen page is open over the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    #[default]
    None,
    Bag,
    Tree,
}

/// UI -> experiment commands.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UiCmd {
    Equip(u32),
    Sell(u32),
    Unequip(usize),
    SellUpTo(Rarity),
    Rank(Perk),
    ToggleFilter,
    Open(Screen),
    Close,
    Retry,
}

/// A ring / slash effect; `half` is a slash's half-angle (0 for full rings).
#[derive(Clone, Copy, Debug, Default)]
pub struct RingView {
    pub item: RItem,
    pub half: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BarView {
    pub pos: Vec3,
    pub frac: f32,
    /// 0 hidden, 1 foe, 2 elite, 3 boss.
    pub kind: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FloaterView {
    pub on: bool,
    pub pos: Vec3,
    pub value: i32,
    pub kind: u8,
    /// 0..1 through its life.
    pub age: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LabelView {
    pub on: bool,
    pub pos: Vec3,
    /// Item id for items; amount for gold.
    pub id: u32,
    /// 0 gold, 1 potion, 2 item.
    pub kind: u8,
    pub rarity: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MiniRoom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 0 combat, 1 start, 2 treasure, 3 stairs, 4 boss.
    pub kind: u8,
    pub seen: bool,
    pub cleared: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BossHud {
    pub frac: f32,
    pub phase: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hud {
    pub hp: f32,
    pub max_hp: f32,
    pub shield: f32,
    pub shield_max: f32,
    pub spark: f32,
    pub max_spark: f32,
    pub xp: u32,
    pub xp_need: u32,
    pub level: u32,
    pub floor: u32,
    pub gold: u32,
    pub potions: u32,
    pub kills: u32,
    pub points: u32,
    pub boss: Option<BossHud>,
    /// Cooldown fraction left per skill (0 ready) and whether it is unlocked / affordable.
    pub skill_cd: [f32; 4],
    pub skill_unlocked: [bool; 4],
    pub skill_ready: [bool; 4],
    pub dodge_cd: f32,
    pub stairs: f32,
    pub filter: bool,
    pub hint: bool,
    pub secs: f32,
    pub low_hp: bool,
}

/// A copy of the numbers the pack and skill screens show; refreshed while one is open.
#[derive(Clone, Default)]
pub struct Snap {
    pub gold: u32,
    pub bag: Vec<Item>,
    pub equip: [Option<Item>; EQUIP_SLOTS],
    pub stats: Stats,
    pub tree: Tree,
    pub level: u32,
    pub hero: &'static str,
    pub attack: &'static str,
    pub potions: u32,
    pub filter: bool,
    pub hp: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SummaryView {
    pub victory: bool,
    pub floor: u32,
    pub level: u32,
    pub kills: u32,
    pub gold: u32,
    pub items: u32,
    pub best_rarity: u8,
    pub secs: f32,
    pub banked: u32,
    pub stash: u32,
    pub best_floor: u32,
    pub new_best_floor: bool,
    pub legacy_pct: u32,
}

#[derive(Resource, Default)]
pub struct RelicBridge {
    pub active: bool,
    /// Kept up to date by [`track_character`] even when the mode is not running.
    pub character: Character,
    pub hud: Hud,
    pub screen: Screen,
    pub snap: Snap,
    pub snap_gen: u32,
    pub cmds: Vec<UiCmd>,
    /// Button edges gathered from touch / gamepad (bit per extra-button id, see `touch.rs`).
    pub edges: u16,
    pub atk_held: bool,
    pub summary: Option<SummaryView>,
    pub summary_ready: bool,
    pub enemies: Vec<RItem>,
    pub shots: Vec<RItem>,
    pub hazards: Vec<RItem>,
    pub rings: Vec<RingView>,
    pub drops: Vec<RItem>,
    pub chests: Vec<RItem>,
    pub bars: [BarView; 20],
    pub floaters: [FloaterView; 24],
    pub labels: [LabelView; 12],
    pub names: std::collections::HashMap<u32, String>,
    pub mini: Vec<MiniRoom>,
    pub mini_player: [f32; 2],
    pub time: f32,
    /// Where the last floor's entities were built (changes when a new floor starts).
    pub floor_gen: u32,
}

impl RelicBridge {
    pub fn reset(&mut self) {
        let fill = |v: &mut Vec<RItem>, n: usize| {
            v.clear();
            v.resize(n, RItem::default());
        };
        self.rings.clear();
        self.rings.resize(RING_SLOTS, RingView::default());
        fill(&mut self.enemies, ENEMY_SLOTS);
        fill(&mut self.shots, SHOT_SLOTS);
        fill(&mut self.hazards, HAZARD_SLOTS);
        fill(&mut self.drops, DROP_SLOTS);
        fill(&mut self.chests, CHEST_SLOTS);
        self.bars = [BarView::default(); 20];
        self.floaters = [FloaterView::default(); 24];
        self.labels = [LabelView::default(); 12];
        self.cmds.clear();
        self.edges = 0;
        self.atk_held = false;
        self.screen = Screen::None;
        self.summary = None;
        self.summary_ready = false;
        self.names.clear();
        self.hud = Hud::default();
    }
}

pub struct RelicViewPlugin;

impl Plugin for RelicViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RelicBridge>().init_resource::<Pools>().add_systems(Update, track_character).add_systems(Update, gather_input.before(super::run_frame).run_if(|b: Res<RelicBridge>| b.active)).add_systems(
            Update,
            (sync_render, overlays, drive_touch_flags).chain().after(super::run_frame).run_if(|b: Res<RelicBridge>| b.active),
        );
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<RelicBridge>| !b.active));
        app.add_plugins(super::relic_ui::RelicUiPlugin);
    }
}

fn track_character(character: Res<Character>, mut bridge: ResMut<RelicBridge>) {
    if bridge.character != *character {
        bridge.character = *character;
    }
}

/// Map the app's character to the core's hero.
pub fn hero_of(c: Character) -> Hero {
    match c {
        Character::Mario => Hero::Mario,
        Character::Link => Hero::Link,
        Character::YoungLink => Hero::YoungLink,
        Character::Banjo => Hero::Banjo,
    }
}

// ---------------------------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------------------------

/// Touch extras, gamepad and keyboard-hold -> bridge edges (consumed by the experiment).
fn gather_input(mut bridge: ResMut<RelicBridge>, mut tc: ResMut<TouchControls>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>) {
    let bits = std::mem::take(&mut tc.extra_pressed);
    bridge.edges |= bits;
    let mut held = tc.extra_down & (1 << 5) != 0 || keys.pressed(KeyCode::Enter);
    for pad in &pads {
        held |= pad.pressed(GamepadButton::RightTrigger);
        for (button, id) in [(GamepadButton::DPadLeft, 1), (GamepadButton::DPadRight, 2), (GamepadButton::DPadDown, 3), (GamepadButton::DPadUp, 6), (GamepadButton::Select, 7), (GamepadButton::LeftThumb, 8)] {
            if pad.just_pressed(button) {
                bridge.edges |= 1 << id;
            }
        }
    }
    if bridge.atk_held != held {
        bridge.atk_held = held;
    }
}

/// Tell the touch layer what to show: the extra buttons while playing, nothing under a menu.
fn drive_touch_flags(bridge: Res<RelicBridge>, mut tc: ResMut<TouchControls>) {
    let menu = bridge.screen != Screen::None || bridge.summary.is_some();
    if tc.extra_active == menu {
        tc.extra_active = !menu;
    }
    if tc.suppress != menu {
        tc.suppress = menu;
    }
    let h = &bridge.hud;
    for i in 0..4 {
        let f = if !h.skill_unlocked[i] { 1.0 } else if h.skill_cd[i] > 0.0 { h.skill_cd[i] } else if !h.skill_ready[i] { 0.5 } else { 0.0 };
        tc.extra_fill[i] = f;
    }
    tc.extra_fill[4] = h.dodge_cd;
    tc.extra_fill[6] = if h.potions == 0 { 1.0 } else { 0.0 };
}

// ---------------------------------------------------------------------------------------------
// World sync
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct Pooled;

struct Shared {
    shot_player: (Handle<Mesh>, Handle<StandardMaterial>),
    shot_enemy: (Handle<Mesh>, Handle<StandardMaterial>),
    shot_wave: (Handle<Mesh>, Handle<StandardMaterial>),
    coin: (Handle<Mesh>, Handle<StandardMaterial>),
    potion: (Handle<Mesh>, Handle<StandardMaterial>),
    gem: Handle<Mesh>,
    gem_mats: [Handle<StandardMaterial>; 5],
    beam: Handle<Mesh>,
    beam_mats: [Handle<StandardMaterial>; 5],
    ring: Handle<Mesh>,
    disc: Handle<Mesh>,
    fans: Vec<(i32, Handle<Mesh>)>,
    hazard_fill: Handle<StandardMaterial>,
    hazard_edge: Handle<StandardMaterial>,
    elite_rings: [Handle<StandardMaterial>; 5],
}

#[derive(Resource, Default)]
struct Pools {
    shared: Option<Shared>,
    enemy_slot: Vec<Option<(u8, Entity)>>,
    enemy_free: [Vec<Entity>; 8],
    elite: Vec<Option<Entity>>,
    shot: Vec<Option<(Entity, u8)>>,
    hazard_fill: Vec<Option<Entity>>,
    hazard_edge: Vec<Option<Entity>>,
    ring: Vec<Option<(Entity, Handle<StandardMaterial>, i32)>>,
    drop: Vec<Option<(Entity, u8)>>,
    beam: Vec<Option<Entity>>,
    chest: Vec<Option<(Entity, Entity)>>,
}

fn unlit(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, ..default() })
}

fn glow(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() })
}

pub fn rarity_color(r: u8) -> Color {
    let c = RARITIES[(r as usize).min(4)].color();
    Color::srgb(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0)
}

/// A flat fan (sector) in the XZ plane, unit radius, centred on +Z, `half` radians either side.
pub fn fan_mesh(half: f32) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    let n = 14;
    let mut pos = vec![[0.0, 0.0, 0.0]];
    for i in 0..=n {
        let a = -half + 2.0 * half * i as f32 / n as f32;
        pos.push([a.sin(), 0.0, a.cos()]);
    }
    let mut idx = vec![];
    for i in 0..n as u32 {
        idx.extend([0, i + 2, i + 1]);
    }
    let count = pos.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count]);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

fn build_shared(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Shared {
    let sphere = |meshes: &mut Assets<Mesh>, r: f32| meshes.add(Sphere::new(r * SCALE).mesh().ico(1).unwrap());
    let rar: Vec<Color> = (0..5).map(rarity_color).collect();
    Shared {
        shot_player: (sphere(meshes, 36.0), unlit(materials, Color::srgb(1.0, 0.95, 0.45))),
        shot_enemy: (sphere(meshes, 30.0), unlit(materials, Color::srgb(0.85, 0.35, 1.0))),
        shot_wave: (sphere(meshes, 70.0), glow(materials, Color::srgba(1.0, 0.8, 0.35, 0.7))),
        coin: (meshes.add(Cylinder::new(30.0 * SCALE, 8.0 * SCALE)), unlit(materials, Color::srgb(1.0, 0.85, 0.1))),
        potion: (sphere(meshes, 36.0), unlit(materials, Color::srgb(1.0, 0.25, 0.3))),
        gem: meshes.add(Cuboid::new(46.0 * SCALE, 46.0 * SCALE, 46.0 * SCALE)),
        gem_mats: std::array::from_fn(|i| unlit(materials, rar[i])),
        beam: meshes.add(Cylinder::new(9.0 * SCALE, 420.0 * SCALE)),
        beam_mats: std::array::from_fn(|i| glow(materials, rar[i].with_alpha(0.28))),
        ring: meshes.add(Annulus::new(0.9, 1.0)),
        disc: meshes.add(Circle::new(1.0)),
        fans: vec![],
        hazard_fill: glow(materials, Color::srgba(1.0, 0.18, 0.12, 0.38)),
        hazard_edge: glow(materials, Color::srgba(1.0, 0.45, 0.25, 0.85)),
        elite_rings: [glow(materials, Color::srgba(0.4, 1.0, 1.0, 0.8)), glow(materials, Color::srgba(1.0, 0.4, 0.4, 0.8)), glow(materials, Color::srgba(1.0, 0.3, 0.8, 0.8)), glow(materials, Color::srgba(1.0, 0.6, 0.1, 0.8)), glow(materials, Color::srgba(0.5, 0.6, 1.0, 0.8))],
    }
}

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
    bridge: Res<RelicBridge>,
    mut pools: ResMut<Pools>,
    mut q: Query<(&mut Transform, &mut Visibility), With<Pooled>>,
) {
    if !minimal.0 {
        minimal.0 = true;
    }
    if pools.shared.is_none() {
        pools.shared = Some(build_shared(&mut meshes, &mut materials));
        pools.enemy_slot = vec![None; ENEMY_SLOTS];
        pools.elite = vec![None; ENEMY_SLOTS];
        pools.shot = vec![None; SHOT_SLOTS];
        pools.hazard_fill = vec![None; HAZARD_SLOTS];
        pools.hazard_edge = vec![None; HAZARD_SLOTS];
        pools.ring = (0..RING_SLOTS).map(|_| None).collect();
        pools.drop = vec![None; DROP_SLOTS];
        pools.beam = vec![None; DROP_SLOTS];
        pools.chest = vec![None; CHEST_SLOTS];
    }
    let pools = &mut *pools;
    let shared = pools.shared.as_mut().unwrap();
    let hidden = |commands: &mut Commands, bundle: (Mesh3d, MeshMaterial3d<StandardMaterial>)| commands.spawn((XpEntity, Pooled, bundle, Transform::default(), Visibility::Hidden)).id();

    // Enemies: a slot's model can change when it is reused; entities return to a per-model pool.
    for (i, item) in bridge.enemies.iter().enumerate() {
        if let Some((k, e)) = pools.enemy_slot[i] {
            if k != item.kind {
                if let Ok((_, mut v)) = q.get_mut(e) {
                    *v = Visibility::Hidden;
                }
                pools.enemy_free[k as usize].push(e);
                pools.enemy_slot[i] = None;
            }
        }
        // Elite marker ring under the foe (colour = modifier).
        let mods = item.aux as u8;
        if item.kind != 0 && mods != 0 {
            let e = *pools.elite[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.ring.clone()), MeshMaterial3d(shared.elite_rings[0].clone()))));
            let which = match mods {
                MOD_SWIFT => 0,
                MOD_STOUT => 1,
                MOD_LEECH => 2,
                MOD_VOLATILE => 3,
                _ => 4,
            };
            commands.entity(e).insert(MeshMaterial3d(shared.elite_rings[which].clone()));
            let ring = RItem { kind: 1, pos: item.pos + Vec3::Y * 0.12, yaw: 0.0, scale: 1.7 * item.scale.max(1.0) * 0.55 + 0.5, aux: 0.0 };
            place(&mut q, e, &ring, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        } else if let Some(e) = pools.elite[i] {
            if let Ok((_, mut v)) = q.get_mut(e) {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
            }
        }
        if item.kind == 0 {
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

    // Shots: player stars, enemy orbs, waves.
    for (i, item) in bridge.shots.iter().enumerate() {
        if item.kind == 0 && pools.shot[i].is_none() {
            continue;
        }
        let kind = item.kind.max(1);
        let (e, k) = match pools.shot[i] {
            Some(x) => x,
            None => {
                let pair = &shared.shot_player;
                (hidden(&mut commands, (Mesh3d(pair.0.clone()), MeshMaterial3d(pair.1.clone()))), 1)
            }
        };
        if item.kind != 0 && k != kind {
            let pair = match kind {
                1 => &shared.shot_player,
                2 => &shared.shot_enemy,
                _ => &shared.shot_wave,
            };
            commands.entity(e).insert((Mesh3d(pair.0.clone()), MeshMaterial3d(pair.1.clone())));
            pools.shot[i] = Some((e, kind));
        } else {
            pools.shot[i] = Some((e, k));
        }
        place(&mut q, e, item, Quat::IDENTITY);
    }

    // Hazards: a growing red disc inside a bright edge.
    for (i, item) in bridge.hazards.iter().enumerate() {
        if item.kind == 0 && pools.hazard_fill[i].is_none() {
            continue;
        }
        let fill = *pools.hazard_fill[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.disc.clone()), MeshMaterial3d(shared.hazard_fill.clone()))));
        let edge = *pools.hazard_edge[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.ring.clone()), MeshMaterial3d(shared.hazard_edge.clone()))));
        let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let mut inner = *item;
        inner.scale = (item.scale * item.aux).max(0.01);
        inner.pos += Vec3::Y * 0.05;
        place(&mut q, fill, &inner, flat);
        let mut outer = *item;
        outer.pos += Vec3::Y * 0.07;
        place(&mut q, edge, &outer, flat);
    }

    // Rings: each slot owns a material so it can fade on its own; slashes use a fan mesh.
    for (i, rv) in bridge.rings.iter().enumerate() {
        let item = &rv.item;
        if item.kind == 0 && pools.ring[i].is_none() {
            continue;
        }
        let kind = item.kind;
        if pools.ring[i].is_none() {
            let mat = glow(&mut materials, Color::WHITE);
            let e = hidden(&mut commands, (Mesh3d(shared.ring.clone()), MeshMaterial3d(mat.clone())));
            pools.ring[i] = Some((e, mat, -1));
        }
        let (e, mat, shape) = pools.ring[i].clone().unwrap();
        if kind != 0 {
            // Slash: fan keyed by half-angle (hundredths of a radian); everything else the ring.
            let want = if kind == 1 { (rv.half * 100.0) as i32 } else { 0 };
            if shape != want {
                let mesh = if kind == 1 {
                    if let Some((_, h)) = shared.fans.iter().find(|(k, _)| *k == want) {
                        h.clone()
                    } else {
                        let h = meshes.add(fan_mesh(want as f32 / 100.0));
                        shared.fans.push((want, h.clone()));
                        h
                    }
                } else {
                    shared.ring.clone()
                };
                commands.entity(e).insert(Mesh3d(mesh));
                pools.ring[i] = Some((e, mat.clone(), want));
            }
            if let Some(mut m) = materials.get_mut(&mat) {
                let c = match kind {
                    1 => [1.0, 0.97, 0.8],
                    2 => [1.0, 0.65, 0.2],
                    3 => [0.5, 0.95, 1.0],
                    4 => [0.5, 0.8, 1.0],
                    _ => [1.0, 0.5, 0.3],
                };
                m.base_color = Color::srgba(c[0], c[1], c[2], (if kind == 1 { 0.6 } else { 0.8 }) * item.aux);
            }
        }
        let rot = if kind == 1 { Quat::from_rotation_y(item.yaw) } else { Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) };
        place(&mut q, e, &RItem { aux: 0.0, ..*item }, rot);
    }

    // Drops: coins spin, potions bob, items are rarity-coloured cubes with a beam from epic up.
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for (i, item) in bridge.drops.iter().enumerate() {
        if item.kind == 0 && pools.drop[i].is_none() {
            continue;
        }
        let want = item.kind;
        let (e, k) = match pools.drop[i] {
            Some(x) => x,
            None => (hidden(&mut commands, (Mesh3d(shared.coin.0.clone()), MeshMaterial3d(shared.coin.1.clone()))), 1),
        };
        if item.kind != 0 && k != want + (item.aux as u8) * 8 {
            let (mesh, mat) = match item.kind {
                1 => (shared.coin.0.clone(), shared.coin.1.clone()),
                2 => (shared.potion.0.clone(), shared.potion.1.clone()),
                _ => (shared.gem.clone(), shared.gem_mats[(item.aux as usize).min(4)].clone()),
            };
            commands.entity(e).insert((Mesh3d(mesh), MeshMaterial3d(mat)));
        }
        pools.drop[i] = Some((e, if item.kind != 0 { want + (item.aux as u8) * 8 } else { k }));
        let rot = match item.kind {
            1 => Quat::from_rotation_y(bridge.time * 4.0 + i as f32) * flat,
            3 => Quat::from_rotation_y(bridge.time * 2.0 + i as f32) * Quat::from_rotation_x(0.5),
            _ => Quat::IDENTITY,
        };
        place(&mut q, e, item, rot);
        // Beam.
        let epic = item.kind == 3 && item.aux >= 3.0;
        if epic || pools.beam[i].is_some() {
            let b = *pools.beam[i].get_or_insert_with(|| hidden(&mut commands, (Mesh3d(shared.beam.clone()), MeshMaterial3d(shared.beam_mats[(item.aux as usize).min(4)].clone()))));
            if epic {
                commands.entity(b).insert(MeshMaterial3d(shared.beam_mats[(item.aux as usize).min(4)].clone()));
            }
            let mut beam = *item;
            beam.kind = if epic { 1 } else { 0 };
            beam.pos += Vec3::Y * 2.0;
            beam.scale = 1.0;
            place(&mut q, b, &beam, Quat::IDENTITY);
        }
    }

    // Chests: base + lid models; the lid is hidden once opened.
    for (i, item) in bridge.chests.iter().enumerate() {
        if item.kind == 0 && pools.chest[i].is_none() {
            continue;
        }
        let (base, lid) = *pools.chest[i].get_or_insert_with(|| {
            let mut spawn_model = |commands: &mut Commands, path: &str| {
                if root.exists(path) {
                    commands.spawn((XpEntity, Pooled, crate::render::scene(&assets, path), Transform::default(), Visibility::Hidden)).id()
                } else {
                    let mesh = meshes.add(Cuboid::new(120.0 * SCALE, 80.0 * SCALE, 90.0 * SCALE));
                    let mat = materials.add(StandardMaterial { base_color: Color::srgb(0.6, 0.38, 0.12), ..default() });
                    commands.spawn((XpEntity, Pooled, Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden)).id()
                }
            };
            (spawn_model(&mut commands, CHEST_BASE), spawn_model(&mut commands, CHEST_LID))
        });
        place(&mut q, base, item, Quat::from_rotation_y(item.yaw));
        let mut l = *item;
        if item.aux > 0.5 {
            l.kind = 0;
        }
        place(&mut q, lid, &l, Quat::from_rotation_y(item.yaw));
    }
}

// ---------------------------------------------------------------------------------------------
// Overlays: floating numbers, loot labels, enemy health bars (UI nodes placed from world points)
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct OverlayRoot;
#[derive(Component)]
pub struct FloaterNode(pub usize);
#[derive(Component)]
pub struct LabelNode(pub usize);
#[derive(Component)]
pub struct BarNode(pub usize);
#[derive(Component)]
pub struct BarFill(pub usize);

fn cleanup(mut pools: ResMut<Pools>, mut minimal: ResMut<crate::hud::HudMinimal>, mut tc: ResMut<TouchControls>) {
    // Only undo what this mode did: it used to clear the flag every frame while inactive, which fought
    // with whichever experiment owned it (the HUD of Star Swarm / Hold the Gate flickered back on).
    if pools.shared.is_some() {
        if minimal.0 {
            minimal.0 = false;
        }
        *pools = Pools::default();
    }
    if tc.extra_active || tc.suppress {
        tc.extra_active = false;
        tc.suppress = false;
        tc.extra_fill = [0.0; 9];
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn overlays(
    bridge: Res<RelicBridge>,
    camera: Query<(&Camera, &GlobalTransform), With<crate::camera::MainCamera>>,
    mut floaters: Query<(&FloaterNode, &mut Node, &mut Text, &mut TextColor, &mut Visibility), (Without<LabelNode>, Without<BarNode>, Without<BarFill>)>,
    mut labels: Query<(&LabelNode, &mut Node, &mut Text, &mut TextColor, &mut Visibility), (Without<FloaterNode>, Without<BarNode>, Without<BarFill>)>,
    mut bars: Query<(&BarNode, &mut Node, &mut Visibility), (Without<FloaterNode>, Without<LabelNode>, Without<BarFill>)>,
    mut fills: Query<(&BarFill, &mut Node, &mut BackgroundColor), (Without<FloaterNode>, Without<LabelNode>, Without<BarNode>)>,
) {
    let Ok((cam, gt)) = camera.single() else { return };
    let project = |p: Vec3| cam.world_to_viewport(gt, p).ok();
    for (f, mut node, mut text, mut color, mut vis) in &mut floaters {
        let v = bridge.floaters[f.0];
        let screen = if v.on { project(v.pos + Vec3::Y * (0.6 + v.age * 0.9)) } else { None };
        let want = if screen.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some(s) = screen {
            node.left = Val::Px(s.x - 20.0);
            node.top = Val::Px(s.y - 10.0);
            let (s_text, c) = match v.kind {
                0 => (v.value.to_string(), Color::WHITE),
                1 => (format!("{}!", v.value), Color::srgb(1.0, 0.85, 0.2)),
                2 => (if v.value == 0 { "BLOCK".into() } else { format!("-{}", v.value) }, Color::srgb(1.0, 0.35, 0.3)),
                3 => (format!("+{}", v.value), Color::srgb(0.45, 1.0, 0.5)),
                _ => (format!("+{}g", v.value), Color::srgb(1.0, 0.88, 0.3)),
            };
            if text.0 != s_text {
                text.0 = s_text;
            }
            let c = c.with_alpha((1.0 - v.age * v.age).clamp(0.0, 1.0));
            if color.0 != c {
                color.0 = c;
            }
        }
    }
    for (l, mut node, mut text, mut color, mut vis) in &mut labels {
        let v = bridge.labels[l.0];
        let screen = if v.on { project(v.pos + Vec3::Y * 0.9) } else { None };
        let want = if screen.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some(s) = screen {
            let s_text = match v.kind {
                0 => format!("{} gold", v.id),
                1 => "Potion".to_string(),
                _ => bridge.names.get(&v.id).cloned().unwrap_or_default(),
            };
            node.left = Val::Px(s.x - s_text.len() as f32 * 3.4);
            node.top = Val::Px(s.y - 8.0);
            if text.0 != s_text {
                text.0 = s_text;
            }
            let c = match v.kind {
                0 => Color::srgb(1.0, 0.88, 0.3),
                1 => Color::srgb(1.0, 0.5, 0.55),
                _ => rarity_color(v.rarity),
            };
            if color.0 != c {
                color.0 = c;
            }
        }
    }
    for (b, mut node, mut vis) in &mut bars {
        let v = bridge.bars[b.0];
        let screen = if v.kind != 0 { project(v.pos) } else { None };
        let want = if screen.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some(s) = screen {
            let w = if v.kind == 3 { 90.0 } else { 46.0 };
            node.left = Val::Px(s.x - w / 2.0);
            node.top = Val::Px(s.y);
            node.width = Val::Px(w);
        }
    }
    for (b, mut node, mut bg) in &mut fills {
        let v = bridge.bars[b.0];
        let w = Val::Percent(v.frac.clamp(0.0, 1.0) * 100.0);
        if node.width != w {
            node.width = w;
        }
        let c = match v.kind {
            2 => Color::srgb(1.0, 0.7, 0.2),
            3 => Color::srgb(0.95, 0.2, 0.2),
            _ => Color::srgb(0.35, 0.9, 0.4),
        };
        if bg.0 != c {
            bg.0 = c;
        }
    }
}
