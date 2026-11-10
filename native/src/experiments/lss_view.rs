//! Last Star Standing presentation: the render sync (pickups, shots, blasts and the Blight wall
//! as shared simple meshes), the HUD (health, inventory, alive count, zone timer, minimap,
//! elimination feed, name tags), the spectate bar and the end-of-match card, all on `bevy_ui`.
//!
//! The experiment (`lss.rs`) owns the rules and the Mario bodies; it talks to this module only
//! through the [`LssBridge`] resource (render lists and HUD numbers one way, button presses the
//! other). Pickups, shots and rings are pooled entities that are only ever moved and hidden;
//! far-away pickups are culled by the experiment.
use crate::button_marker::{Button, BorderRadius};
use super::lss_core::*;
use super::swarm_core::clock;
use super::XpEntity;
use crate::camera::MainCamera;
use crate::render::SCALE;
use crate::safe_area::SafeArea;
use crate::shell::theme::*;
use crate::shell::{AppState, ReturnToMenu};
use crate::touch::{ModeButtons, TouchControls, MARGIN_X, MARGIN_Y, PAUSE_SIZE, PAUSE_TOP};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::asset::RenderAssetUsages;

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
pub struct SlotView {
    /// 0 = empty, else `Kind` index + 1.
    pub kind: u8,
    pub rarity: u8,
    pub ammo: u8,
    pub cooldown: f32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Hud {
    pub hp: f32,
    pub shield: f32,
    pub alive: u32,
    pub total: u32,
    pub kills: u32,
    pub secs: f32,
    pub phase: u8,
    /// 0 landing, 1 hold, 2 shrink, 3 final.
    pub stage: u8,
    pub zone_secs: f32,
    /// Distance in SM64 units from the player to the centre of the next safe circle.
    pub zone_dist: f32,
    pub outside: bool,
    pub dropping: bool,
    pub quake: u8,
    pub boost: bool,
    pub hint: bool,
    pub god: bool,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct MapView {
    pub arena: [f32; 3],
    pub cur: [f32; 3],
    pub next: [f32; 3],
    pub me: [f32; 2],
    pub show_next: bool,
}

#[derive(Clone, Debug)]
pub struct FeedLine {
    pub text: String,
    pub age: f32,
    pub mine: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Tag {
    pub id: usize,
    pub pos: Vec3,
    pub hp: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    /// You are out but the match goes on.
    Out,
    /// You won.
    Win,
    /// The match ended without you.
    End,
}

#[derive(Clone, Debug)]
pub struct Card {
    pub kind: CardKind,
    pub place: u32,
    pub total: u32,
    pub kills: u32,
    pub secs: f32,
    pub best_place: u32,
    pub wins: u32,
    pub new_best: bool,
    pub winner: String,
    pub can_spectate: bool,
    /// Difficulty for the next match (0 easy, 1 normal, 2 hard).
    pub difficulty: u8,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct ZoneVis {
    pub cur: [f32; 3],
    pub next: [f32; 3],
    pub show_next: bool,
    pub base_y: f32,
    pub active: bool,
}

#[derive(Resource, Default)]
pub struct LssBridge {
    pub active: bool,
    pub hud: Hud,
    pub slots: [SlotView; 3],
    pub map: MapView,
    pub feed: Vec<FeedLine>,
    pub tags: Vec<Tag>,
    pub names: Vec<String>,
    pub pickups: Vec<RItem>,
    pub shots: Vec<RItem>,
    pub rings: Vec<RItem>,
    pub zone: ZoneVis,
    pub card: Option<Card>,
    pub card_gen: u32,
    pub card_ready: bool,
    pub spectate: Option<String>,
    pub time: f32,
    // From the UI, applied by the experiment.
    pub slot_press: [bool; 3],
    pub retry: bool,
    pub spectate_start: bool,
    pub spec_step: i8,
    pub diff_press: bool,
}

impl LssBridge {
    pub fn reset(&mut self, names: Vec<String>) {
        let fill = |v: &mut Vec<RItem>, n: usize| {
            v.clear();
            v.resize(n, RItem::default());
        };
        fill(&mut self.pickups, PICKUP_SLOTS);
        fill(&mut self.shots, SHOT_SLOTS);
        fill(&mut self.rings, RING_SLOTS);
        self.names = names;
        self.feed.clear();
        self.tags.clear();
        self.card = None;
        self.card_gen = self.card_gen.wrapping_add(1);
        self.card_ready = false;
        self.spectate = None;
        self.slot_press = [false; 3];
        self.retry = false;
        self.spectate_start = false;
        self.spec_step = 0;
        self.diff_press = false;
        self.hud = Hud::default();
        self.slots = [SlotView::default(); 3];
    }
}

pub struct LssViewPlugin;

impl Plugin for LssViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LssBridge>().init_resource::<Pools>().init_resource::<UiState>().add_systems(
            Update,
            (sync_render, manage_ui, read_input, update_hud, update_map, update_spec, update_tags, card_input, overlay_display).chain().after(super::run_frame).run_if(|b: Res<LssBridge>| b.active),
        );
        app.add_systems(Update, cleanup.after(super::run_frame).run_if(|b: Res<LssBridge>| !b.active));
    }
}

// ---------------------------------------------------------------------------------------------
// Colours and kinds
// ---------------------------------------------------------------------------------------------

pub fn kind_color(k: Kind) -> Color {
    match k {
        Kind::Shell => hex(0x4cd964),
        Kind::Bomb => hex(0x2a2a33),
        Kind::Flower => hex(0xff8a1f),
        Kind::StarBurst => hex(0xffe14a),
        Kind::Quake => hex(0xb5651d),
        Kind::Cap => hex(0xe5302a),
        Kind::Boots => hex(0x4aa8ff),
        Kind::Heart => hex(0xff5c8a),
        Kind::OneUp => hex(0x57e36a),
    }
}

pub fn rarity_color(r: u8) -> Color {
    match r {
        0 => hex(0xdfe6f5),
        1 => hex(0x4aa8ff),
        _ => hex(0xd36cff),
    }
}

/// Distinct cap colours for the field (hue steps by the golden angle).
pub fn cap_color(i: usize) -> u32 {
    let h = (i as f32 * 137.508) % 360.0;
    let (s, v) = if i % 3 == 0 { (0.85, 0.95) } else if i % 3 == 1 { (0.65, 0.9) } else { (0.95, 0.75) };
    let c = Color::hsv(h, s, v).to_srgba();
    ((c.red * 255.0) as u32) << 16 | ((c.green * 255.0) as u32) << 8 | (c.blue * 255.0) as u32
}

fn kind_of(k: u8) -> Option<Kind> {
    k.checked_sub(1).and_then(|i| ALL_KINDS.get(i as usize).copied())
}

// ---------------------------------------------------------------------------------------------
// World sync
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct Pooled;

struct Shared {
    item_mesh: [Handle<Mesh>; 9],
    item_mat: [Handle<StandardMaterial>; 9],
    ring: Handle<Mesh>,
    ring_mat: [Handle<StandardMaterial>; 3],
    chest: (Handle<Mesh>, Handle<StandardMaterial>),
    lid: (Handle<Mesh>, Handle<StandardMaterial>),
    crate_body: (Handle<Mesh>, Handle<StandardMaterial>),
    chute: (Handle<Mesh>, Handle<StandardMaterial>),
    beam: (Handle<Mesh>, Handle<StandardMaterial>),
    shot_mesh: [Handle<Mesh>; 3],
    shot_mat: [Handle<StandardMaterial>; 3],
    fuse: (Handle<Mesh>, Handle<StandardMaterial>),
    blast_mat: [Handle<StandardMaterial>; 3],
    wall: Handle<Mesh>,
    wall_mat: Handle<StandardMaterial>,
    next_wall: Handle<Mesh>,
    next_mat: Handle<StandardMaterial>,
    beam_tall: (Handle<Mesh>, Handle<StandardMaterial>),
}

#[derive(Clone, Copy, Default)]
struct PickEnts {
    body: Option<Entity>,
    ring: Option<Entity>,
    extra: Option<Entity>,
    kind: u8,
}

#[derive(Resource, Default)]
struct Pools {
    shared: Option<Shared>,
    pick: Vec<PickEnts>,
    shot: Vec<Option<(u8, Entity)>>,
    shot_free: [Vec<Entity>; 4],
    ring: Vec<Option<(Entity, Handle<StandardMaterial>)>>,
    wall: Option<[Entity; 3]>,
}

fn unlit(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, ..default() })
}

fn translucent(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial { base_color: c, unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, double_sided: true, ..default() })
}

/// A unit-radius, unit-high open tube whose vertex alpha fades from `bottom` to `top`.
fn tube(color: [f32; 3], bottom: f32, top: f32) -> Mesh {
    const N: usize = 72;
    let mut pos = Vec::with_capacity(2 * (N + 1));
    let mut nor = Vec::with_capacity(2 * (N + 1));
    let mut col = Vec::with_capacity(2 * (N + 1));
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let (s, c) = a.sin_cos();
        pos.push([s, 0.0, c]);
        pos.push([s, 1.0, c]);
        nor.push([s, 0.0, c]);
        nor.push([s, 0.0, c]);
        col.push([color[0], color[1], color[2], bottom]);
        col.push([color[0], color[1], color[2], top]);
    }
    let mut idx = Vec::with_capacity(N * 6);
    for i in 0..N as u32 {
        let b = i * 2;
        idx.extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

fn build_shared(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Shared {
    let sphere = |meshes: &mut Assets<Mesh>, r: f32| meshes.add(Sphere::new(r * SCALE).mesh().ico(1).unwrap());
    let item_mesh = [
        meshes.add(Sphere::new(34.0 * SCALE).mesh().ico(1).unwrap()),
        sphere(meshes, 38.0),
        meshes.add(Cone { radius: 34.0 * SCALE, height: 70.0 * SCALE }),
        meshes.add(Sphere::new(46.0 * SCALE).mesh().ico(0).unwrap()),
        meshes.add(Cuboid::new(60.0 * SCALE, 60.0 * SCALE, 60.0 * SCALE)),
        meshes.add(Cylinder::new(38.0 * SCALE, 26.0 * SCALE)),
        meshes.add(Cuboid::new(70.0 * SCALE, 40.0 * SCALE, 40.0 * SCALE)),
        sphere(meshes, 36.0),
        sphere(meshes, 36.0),
    ];
    let item_mat = ALL_KINDS.map(|k| unlit(materials, kind_color(k)));
    let ring_mat = [0u8, 1, 2].map(|r| materials.add(StandardMaterial { base_color: rarity_color(r).with_alpha(0.7), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() }));
    Shared {
        item_mesh,
        item_mat,
        ring: meshes.add(Annulus::new(0.78, 1.0)),
        ring_mat,
        chest: (meshes.add(Cuboid::new(110.0 * SCALE, 62.0 * SCALE, 72.0 * SCALE)), materials.add(StandardMaterial { base_color: hex(0x8a4b17), perceptual_roughness: 0.9, ..default() })),
        lid: (meshes.add(Cuboid::new(116.0 * SCALE, 24.0 * SCALE, 78.0 * SCALE)), materials.add(StandardMaterial { base_color: hex(0xffc83a), perceptual_roughness: 0.5, ..default() })),
        crate_body: (meshes.add(Cuboid::new(120.0 * SCALE, 120.0 * SCALE, 120.0 * SCALE)), materials.add(StandardMaterial { base_color: hex(0xff6a2a), perceptual_roughness: 0.6, ..default() })),
        chute: (meshes.add(Cone { radius: 220.0 * SCALE, height: 90.0 * SCALE }), translucent(materials, Color::srgba(1.0, 0.95, 0.5, 0.8))),
        beam: (meshes.add(Cylinder::new(16.0 * SCALE, 600.0 * SCALE)), translucent(materials, Color::srgba(0.85, 0.5, 1.0, 0.35))),
        shot_mesh: [sphere(meshes, 44.0), sphere(meshes, 50.0), sphere(meshes, 26.0)],
        shot_mat: [unlit(materials, hex(0x32d04f)), unlit(materials, hex(0x15151c)), unlit(materials, hex(0xff9a2a))],
        fuse: (sphere(meshes, 18.0), unlit(materials, hex(0xffd23a))),
        blast_mat: [translucent(materials, Color::srgba(1.0, 0.55, 0.15, 0.7)), translucent(materials, Color::srgba(1.0, 0.85, 0.4, 0.7)), translucent(materials, Color::srgba(1.0, 0.95, 0.3, 0.7))],
        wall: meshes.add(tube([0.55, 0.12, 0.85], 0.7, 0.05)),
        wall_mat: translucent(materials, Color::WHITE),
        next_wall: meshes.add(tube([1.0, 1.0, 1.0], 0.55, 0.0)),
        next_mat: translucent(materials, Color::WHITE),
        beam_tall: (meshes.add(Cylinder::new(30.0 * SCALE, 1.0)), translucent(materials, Color::srgba(1.0, 0.92, 0.45, 0.28))),
    }
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<LssBridge>,
    mut pools: ResMut<Pools>,
    mut q: Query<(&mut Transform, &mut Visibility), With<Pooled>>,
) {
    if !minimal.0 {
        minimal.0 = true;
    }
    if pools.shared.is_none() {
        pools.shared = Some(build_shared(&mut meshes, &mut materials));
        pools.pick = vec![PickEnts::default(); PICKUP_SLOTS];
        pools.shot = vec![None; SHOT_SLOTS];
        pools.ring = (0..RING_SLOTS).map(|_| None).collect();
    }
    let pools = &mut *pools;
    let shared = pools.shared.as_ref().unwrap();
    let t = bridge.time;
    let hidden = |commands: &mut Commands, mesh: &Handle<Mesh>, mat: &Handle<StandardMaterial>| commands.spawn((XpEntity, Pooled, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden)).id();

    // The Blight wall, the next-circle marker and the beam over its centre.
    if bridge.zone.active {
        let walls = *pools.wall.get_or_insert_with(|| [hidden(&mut commands, &shared.wall, &shared.wall_mat), hidden(&mut commands, &shared.next_wall, &shared.next_mat), hidden(&mut commands, &shared.beam_tall.0, &shared.beam_tall.1)]);
        let z = &bridge.zone;
        let base = z.base_y - 400.0;
        let (cr, nr) = (z.cur[2] * SCALE, z.next[2] * SCALE);
        place(&mut q, walls[0], true, Vec3::new(z.cur[0], base, z.cur[1]) * SCALE, Quat::IDENTITY, Vec3::new(cr, 9000.0 * SCALE, cr));
        place(&mut q, walls[1], z.show_next, Vec3::new(z.next[0], base, z.next[1]) * SCALE, Quat::IDENTITY, Vec3::new(nr, 1400.0 * SCALE, nr));
        // The beam is a unit-height cylinder centred on its origin.
        place(&mut q, walls[2], z.show_next, Vec3::new(z.next[0], z.base_y + 3000.0, z.next[1]) * SCALE, Quat::IDENTITY, Vec3::new(1.0, 6000.0 * SCALE, 1.0));
    }
    // Pickups.
    let ring_rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for (i, item) in bridge.pickups.iter().enumerate() {
        let slot = &mut pools.pick[i];
        if item.kind == 0 {
            for e in [slot.body, slot.ring, slot.extra].into_iter().flatten() {
                place(&mut q, e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
            }
            continue;
        }
        let rarity = item.aux as usize % 3;
        let spin = Quat::from_rotation_y(t * 2.0 + i as f32);
        match item.kind {
            1..=9 => {
                let k = (item.kind - 1) as usize;
                if slot.body.is_none() || slot.kind != item.kind {
                    if let Some(e) = slot.body.take() {
                        commands.entity(e).despawn();
                    }
                    slot.body = Some(hidden(&mut commands, &shared.item_mesh[k], &shared.item_mat[k]));
                    slot.kind = item.kind;
                }
                let bob = (t * 2.5 + i as f32).sin() * 8.0;
                place(&mut q, slot.body.unwrap(), true, item.pos + Vec3::Y * (60.0 + bob) * SCALE, spin, Vec3::splat(item.scale));
                let ring = *slot.ring.get_or_insert_with(|| hidden(&mut commands, &shared.ring, &shared.ring_mat[0]));
                commands.entity(ring).insert(MeshMaterial3d(shared.ring_mat[rarity].clone()));
                place(&mut q, ring, true, item.pos + Vec3::Y * 4.0 * SCALE, ring_rot, Vec3::splat(70.0 * SCALE));
                if rarity == 2 {
                    let beam = *slot.extra.get_or_insert_with(|| hidden(&mut commands, &shared.beam.0, &shared.beam.1));
                    place(&mut q, beam, true, item.pos + Vec3::Y * 300.0 * SCALE, Quat::IDENTITY, Vec3::ONE);
                } else if let Some(e) = slot.extra {
                    place(&mut q, e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
                }
            }
            10 | 11 => {
                // Chest: closed or open (lid tipped back).
                if slot.body.is_none() || slot.kind != 10 {
                    if let Some(e) = slot.body.take() {
                        commands.entity(e).despawn();
                    }
                    if let Some(e) = slot.extra.take() {
                        commands.entity(e).despawn();
                    }
                    slot.body = Some(hidden(&mut commands, &shared.chest.0, &shared.chest.1));
                    slot.extra = Some(hidden(&mut commands, &shared.lid.0, &shared.lid.1));
                    slot.kind = 10;
                }
                let yaw = Quat::from_rotation_y(item.yaw);
                place(&mut q, slot.body.unwrap(), true, item.pos + Vec3::Y * 31.0 * SCALE, yaw, Vec3::ONE);
                let open = item.kind == 11;
                let lid_pos = if open { Vec3::new(0.0, 58.0, -44.0) } else { Vec3::new(0.0, 74.0, 0.0) };
                let lid_rot = if open { Quat::from_rotation_x(-1.9) } else { Quat::IDENTITY };
                place(&mut q, slot.extra.unwrap(), true, item.pos + yaw * (lid_pos * SCALE), yaw * lid_rot, Vec3::ONE);
                let ring = *slot.ring.get_or_insert_with(|| hidden(&mut commands, &shared.ring, &shared.ring_mat[2]));
                place(&mut q, ring, !open, item.pos + Vec3::Y * 4.0 * SCALE, ring_rot, Vec3::splat(110.0 * SCALE));
            }
            _ => {
                // Airdrop crate (+ parachute while falling, aux = height above its landing spot).
                if slot.body.is_none() || slot.kind != 12 {
                    if let Some(e) = slot.body.take() {
                        commands.entity(e).despawn();
                    }
                    if let Some(e) = slot.extra.take() {
                        commands.entity(e).despawn();
                    }
                    slot.body = Some(hidden(&mut commands, &shared.crate_body.0, &shared.crate_body.1));
                    slot.extra = Some(hidden(&mut commands, &shared.chute.0, &shared.chute.1));
                    slot.kind = 12;
                }
                let falling = item.aux > 40.0;
                place(&mut q, slot.body.unwrap(), true, item.pos + Vec3::Y * 62.0 * SCALE, Quat::from_rotation_y(item.yaw), Vec3::splat(if item.kind == 13 { 0.8 } else { 1.0 }));
                place(&mut q, slot.extra.unwrap(), falling, item.pos + Vec3::Y * 230.0 * SCALE, Quat::from_rotation_x(std::f32::consts::PI), Vec3::ONE);
                let ring = *slot.ring.get_or_insert_with(|| hidden(&mut commands, &shared.ring, &shared.ring_mat[2]));
                place(&mut q, ring, !falling && item.kind == 12, item.pos + Vec3::Y * 4.0 * SCALE, ring_rot, Vec3::splat(120.0 * SCALE));
            }
        }
    }
    // Shots: green shell, black bomb with a spark, fireball.
    for (i, item) in bridge.shots.iter().enumerate() {
        if let Some((k, e)) = pools.shot[i] {
            if k != item.kind {
                place(&mut q, e, false, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
                pools.shot_free[k as usize].push(e);
                pools.shot[i] = None;
            }
        }
        if item.kind == 0 {
            continue;
        }
        let e = match pools.shot[i] {
            Some((_, e)) => e,
            None => {
                let k = (item.kind - 1) as usize;
                let e = pools.shot_free[item.kind as usize].pop().unwrap_or_else(|| hidden(&mut commands, &shared.shot_mesh[k], &shared.shot_mat[k]));
                pools.shot[i] = Some((item.kind, e));
                e
            }
        };
        let squash = if item.kind == 1 { Vec3::new(1.0, 0.65, 1.0) } else { Vec3::ONE };
        place(&mut q, e, true, item.pos, Quat::from_rotation_y(item.yaw), squash * item.scale);
    }
    // Blasts and shockwaves.
    for (i, item) in bridge.rings.iter().enumerate() {
        if item.kind == 0 && pools.ring[i].is_none() {
            continue;
        }
        let (e, mat) = pools.ring[i].get_or_insert_with(|| {
            let mat = translucent(&mut materials, Color::WHITE);
            (hidden(&mut commands, &shared.ring, &mat), mat)
        });
        if item.kind != 0 {
            if let Some(mut m) = materials.get_mut(&*mat) {
                let c = match item.kind {
                    1 => [1.0, 0.55, 0.15],
                    2 => [1.0, 0.9, 0.5],
                    _ => [1.0, 0.95, 0.3],
                };
                m.base_color = Color::srgba(c[0], c[1], c[2], 0.85 * item.aux);
            }
        }
        place(&mut q, *e, item.kind != 0, item.pos + Vec3::Y * 12.0 * SCALE, ring_rot, Vec3::splat(item.scale.max(0.01)));
    }
}

/// Entities are despawned by the framework on exit; forget the handles.
fn cleanup(mut pools: ResMut<Pools>, mut ui: ResMut<UiState>, mut minimal: ResMut<crate::hud::HudMinimal>, mut mode: ResMut<ModeButtons>) {
    // Only undo what this mode did (another experiment may own the flags now).
    if pools.shared.is_some() {
        if mode.active {
            mode.active = false;
        }
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
struct CardRoot;
#[derive(Component)]
struct AliveText;
#[derive(Component)]
struct KillText;
#[derive(Component)]
struct ZoneText;
#[derive(Component)]
struct DistText;
#[derive(Component)]
struct HpFill;
#[derive(Component)]
struct HpText;
#[derive(Component)]
struct ShieldFill;
#[derive(Component)]
struct HintText;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct Vignette;
#[derive(Component)]
struct FeedText(usize);
#[derive(Component)]
struct Hotbar;
#[derive(Component)]
struct SlotCell(usize);
#[derive(Component)]
struct SlotLabel(usize);
#[derive(Component)]
struct SpecBar;
#[derive(Component)]
struct SpecName;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum SpecButton {
    Prev,
    Next,
    Menu,
}
#[derive(Component)]
struct MapCur;
#[derive(Component)]
struct MapNext;
#[derive(Component)]
struct MapMe;
#[derive(Component)]
struct MapPtr;
#[derive(Component)]
struct TagNode(usize);
#[derive(Component)]
struct TagName(usize);
#[derive(Component)]
struct TagBar(usize);
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum CardButton {
    Retry,
    Spectate,
    Menu,
    Difficulty,
}

const MAP: f32 = 92.0;

/// Top edge of the minimap column: just under the touch pause button.
pub fn minimap_top(area: &SafeArea) -> f32 {
    MARGIN_Y.max(area.top) + PAUSE_TOP + PAUSE_SIZE + 6.0
}

/// Screen rectangle of the minimap and its distance label for a window of `size`.
pub fn minimap_rect(size: Vec2, area: &SafeArea) -> Rect {
    let right = MARGIN_X.max(area.right);
    let top = minimap_top(area);
    Rect::from_corners(Vec2::new(size.x - right - MAP, top), Vec2::new(size.x - right, top + MAP + 14.0))
}
const TAGS: usize = 6;
const FEED_LINES: usize = 5;

#[derive(Resource, Default)]
struct UiState {
    hud: Option<Entity>,
    card: Option<(Entity, u32)>,
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

fn abs(left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    let v = |x: Option<f32>| x.map_or(Val::Auto, Val::Px);
    Node { position_type: PositionType::Absolute, left: v(left), right: v(right), top: v(top), bottom: v(bottom), ..default() }
}

fn dot(size: f32, color: Color) -> (Node, BackgroundColor, BorderRadius) {
    (Node { position_type: PositionType::Absolute, width: Val::Px(size), height: Val::Px(size), ..default() }, BackgroundColor(color), BorderRadius::MAX)
}

fn spawn_hud(commands: &mut Commands, a: &UiAssets, area: &SafeArea) -> Entity {
    let left = MARGIN_X.max(area.left);
    let right = MARGIN_X.max(area.right);
    let top = MARGIN_Y.max(area.top).min(MARGIN_Y.max(area.top));
    let root = commands.spawn((XpEntity, Overlay, HudRoot, GlobalZIndex(12), Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() })).id();
    commands.entity(root).with_children(|p| {
        // Danger tint while in the Blight.
        p.spawn((Vignette, abs(Some(0.0), Some(0.0), Some(0.0), Some(0.0)), BackgroundColor(Color::srgba(0.45, 0.05, 0.6, 0.0))));
        // Left column: health, shield, feed.
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Px(left), top: Val::Px(top), flex_direction: FlexDirection::Column, row_gap: Val::Px(3.0), ..default() }).with_children(|c| {
            c.spawn(Node { width: Val::Px(188.0), height: Val::Px(20.0), ..default() }).with_children(|h| {
                h.spawn((Node { width: Val::Px(188.0), height: Val::Px(20.0), border: UiRect::all(Val::Px(3.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.8)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(10.0))))
                    .with_children(|b| {
                        b.spawn((HpFill, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0x39c24a))));
                    });
                h.spawn(abs(Some(0.0), Some(0.0), Some(0.0), Some(0.0))).insert(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }).with_children(|t| {
                    t.spawn((HpText, text(a, "100", 12.0, Color::WHITE)));
                });
            });
            c.spawn((Node { width: Val::Px(188.0), height: Val::Px(6.0), ..default() }, BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.6)), BorderRadius::all(Val::Px(3.0)))).with_children(|b| {
                b.spawn((ShieldFill, Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0x4aa8ff)), BorderRadius::all(Val::Px(3.0))));
            });
            for i in 0..FEED_LINES {
                c.spawn((FeedText(i), text(a, "", 12.0, CREAM), Node { max_width: Val::Px(250.0), ..default() }));
            }
        });
        // Top centre: alive and kills, then the zone line.
        p.spawn(Node { position_type: PositionType::Absolute, left: Val::Percent(30.0), right: Val::Percent(30.0), top: Val::Px(top - 4.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }).with_children(|c| {
            c.spawn(Node { align_items: AlignItems::Baseline, column_gap: Val::Px(12.0), ..default() }).with_children(|r| {
                r.spawn((AliveText, Text::new("40"), font(&a.display, 26.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
                r.spawn(text(a, "ALIVE", 11.0, CREAM));
                r.spawn((KillText, text(a, "0 KOs", 13.0, GOLD_HI)));
            });
            c.spawn((ZoneText, text(a, "", 13.0, CREAM), TextLayout::justify(Justify::Center)));
            c.spawn((StatusText, Text::new(""), font(&a.display, 22.0), TextColor(RED_HI), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
            crate::hints::note("Hold A to dive fast. Land near loot. Tap B in the air to dive, Z to ground-pound.");
            c.spawn((HintText, text(a, "Hold A to dive fast. Land near loot. Tap B in the air to dive, Z to ground-pound.", 12.0, CREAM), Node { margin: UiRect::top(Val::Px(6.0)), max_width: Val::Px(360.0), ..default() }, TextLayout::justify(Justify::Center)));
        });
        // Minimap under the pause button.
        let map_top = minimap_top(area);
        p.spawn(Node { position_type: PositionType::Absolute, right: Val::Px(right), top: Val::Px(map_top), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(1.0), ..default() }).with_children(|c| {
            c.spawn((Node { width: Val::Px(MAP), height: Val::Px(MAP), border: UiRect::all(Val::Px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.30, 0.08, 0.42, 0.78)), BorderColor::from(Color::srgba(1.0, 1.0, 1.0, 0.6)), BorderRadius::MAX)).with_children(|m| {
                m.spawn((MapCur, Node { position_type: PositionType::Absolute, border: UiRect::all(Val::Px(1.5)), ..default() }, BackgroundColor(Color::srgba(0.45, 0.85, 0.55, 0.55)), BorderColor::from(Color::WHITE), BorderRadius::MAX));
                m.spawn((MapNext, Node { position_type: PositionType::Absolute, border: UiRect::all(Val::Px(2.0)), ..default() }, BorderColor::all(GOLD), BorderRadius::MAX));
                m.spawn((MapMe, dot(8.0, Color::srgb(1.0, 0.25, 0.2))));
                m.spawn((MapPtr, dot(7.0, GOLD)));
            });
            c.spawn((DistText, text(a, "", 11.0, GOLD_HI)));
        });
        // Name tags over the nearest rivals.
        for i in 0..TAGS {
            p.spawn((TagNode(i), Node { position_type: PositionType::Absolute, display: Display::None, width: Val::Px(90.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(1.0), ..default() })).with_children(|t| {
                t.spawn((TagName(i), text(a, "", 11.0, Color::WHITE)));
                t.spawn((Node { width: Val::Px(46.0), height: Val::Px(5.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)), BorderRadius::all(Val::Px(2.0)))).with_children(|b| {
                    b.spawn((TagBar(i), Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(hex(0xff5c4a)), BorderRadius::all(Val::Px(2.0))));
                });
            });
        }
        // Hotbar (desktop; phones use the touch buttons).
        p.spawn((Hotbar, Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-96.0)), bottom: Val::Px(MARGIN_Y.max(area.bottom)), column_gap: Val::Px(6.0), ..default() })).with_children(|h| {
            for i in 0..3 {
                h.spawn((Button, SlotCell(i), Node { width: Val::Px(60.0), height: Val::Px(60.0), border: UiRect::all(Val::Px(3.0)), flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgba(0.04, 0.08, 0.25, 0.8)), BorderColor::all(NAVY), BorderRadius::all(Val::Px(12.0)))).with_children(|c| {
                    c.spawn((SlotLabel(i), Text::new(""), font(&a.body, 11.0), TextColor(Color::WHITE), TextLayout::justify(Justify::Center)));
                });
            }
        });
        // Spectate bar.
        p.spawn((SpecBar, Node { position_type: PositionType::Absolute, display: Display::None, left: Val::Percent(50.0), margin: UiRect::left(Val::Px(-170.0)), bottom: Val::Px(MARGIN_Y.max(area.bottom)), width: Val::Px(340.0), column_gap: Val::Px(8.0), align_items: AlignItems::Center, justify_content: JustifyContent::Center, ..default() })).with_children(|b| {
            let mut btn = button(b, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(42.0), Val::Px(38.0), 18.0), "<");
            btn.insert(SpecButton::Prev);
            b.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, min_width: Val::Px(120.0), ..default() }).with_children(|n| {
                n.spawn(text(a, "SPECTATING", 10.0, GOLD));
                n.spawn((SpecName, Text::new(""), font(&a.display, 17.0), TextColor(Color::WHITE), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
            });
            let mut btn = button(b, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(42.0), Val::Px(38.0), 18.0), ">");
            btn.insert(SpecButton::Next);
            let mut btn = button(b, a, ButtonSpec::new(ButtonKind::Red, Val::Px(78.0), Val::Px(38.0), 14.0), "LEAVE");
            btn.insert(SpecButton::Menu);
        });
    });
    root
}

fn spawn_card(commands: &mut Commands, a: &UiAssets, area: &SafeArea, c: &Card, window_h: f32) -> Entity {
    let mut node = root_node(area);
    node.justify_content = JustifyContent::Center;
    let root = commands.spawn((XpEntity, Overlay, CardRoot, GlobalZIndex(22), node, BackgroundColor(Color::srgba(0.06, 0.0, 0.1, 0.62)))).id();
    let compact = window_h < 460.0;
    commands.entity(root).with_children(|p| {
        panel(p, a, Node { padding: UiRect::axes(Val::Px(28.0), Val::Px(if compact { 8.0 } else { 14.0 })), row_gap: Val::Px(if compact { 3.0 } else { 6.0 }), width: Val::Px(400.0), align_items: AlignItems::Center, ..default() }).with_children(|k| {
            if !compact {
                kicker(k, a, "LAST STAR STANDING");
            }
            let (title, color) = match c.kind {
                CardKind::Win => ("LAST STAR STANDING!", GOLD),
                CardKind::Out => ("ELIMINATED", RED_HI),
                CardKind::End => ("MATCH OVER", CREAM),
            };
            outlined(k, a, title, if c.kind == CardKind::Win { 26.0 } else if compact { 30.0 } else { 36.0 }, color);
            let place = format!("#{} of {}", c.place, c.total);
            k.spawn((Text::new(clean(place)), font(&a.display, if compact { 34.0 } else { 46.0 }), TextColor(if c.place == 1 { GOLD } else { Color::WHITE }), TextShadow { offset: Vec2::new(0.0, 3.0), color: NAVY }));
            if c.kind == CardKind::End {
                k.spawn(text(a, &format!("{} wins", c.winner), 14.0, GOLD_HI));
            }
            let mut row = |label: &str, value: String, color: Color| {
                k.spawn(Node { width: Val::Percent(100.0), justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
                    r.spawn(text(a, label, 15.0, CREAM));
                    r.spawn((Text::new(clean(value)), font(&a.display, 19.0), TextColor(color), TextShadow { offset: Vec2::new(0.0, 2.0), color: NAVY }));
                });
            };
            row("Knockouts", c.kills.to_string(), Color::WHITE);
            row("Survived", clock(c.secs), Color::WHITE);
            row(if c.new_best { "Best placement (new!)" } else { "Best placement" }, if c.best_place == 0 { "-".to_string() } else { format!("#{}", c.best_place) }, if c.new_best { GOLD } else { GOLD_HI });
            if c.wins > 0 {
                row("Wins", c.wins.to_string(), GOLD_HI);
            }
            k.spawn(Node { column_gap: Val::Px(8.0), margin: UiRect::top(Val::Px(6.0)), ..default() }).with_children(|r| {
                if c.can_spectate {
                    let mut b = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(112.0), Val::Px(44.0), 15.0), "SPECTATE");
                    b.insert(CardButton::Spectate);
                }
                let mut b = button(r, a, ButtonSpec::new(ButtonKind::Gold, Val::Px(100.0), Val::Px(44.0), 19.0), "RETRY");
                b.insert(CardButton::Retry);
                let mut b = button(r, a, ButtonSpec::new(ButtonKind::Blue, Val::Px(112.0), Val::Px(44.0), 15.0), "MAIN MENU");
                b.insert(CardButton::Menu);
            });
            let label = format!("BOTS: {}  (tap to change)", ["EASY", "NORMAL", "HARD"][c.difficulty.min(2) as usize]);
            let mut b = button(k, a, ButtonSpec::new(ButtonKind::Chip, Val::Px(250.0), Val::Px(28.0), 12.0), &label);
            b.insert(CardButton::Difficulty);
        });
    });
    root
}

#[allow(clippy::too_many_arguments)]
fn manage_ui(mut commands: Commands, bridge: Res<LssBridge>, assets: Option<Res<UiAssets>>, area: Res<SafeArea>, windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut ui: ResMut<UiState>) {
    let Some(a) = assets else { return };
    if ui.hud.is_none() {
        ui.hud = Some(spawn_hud(&mut commands, &a, &area));
    }
    let wh = windows.single().map_or(402.0, |w| w.height());
    match (ui.card.as_ref().map(|c| (c.0, c.1)), bridge.card.as_ref()) {
        (None, Some(c)) => ui.card = Some((spawn_card(&mut commands, &a, &area, c, wh), bridge.card_gen)),
        (Some((e, g)), Some(c)) if g != bridge.card_gen => {
            commands.entity(e).despawn();
            ui.card = Some((spawn_card(&mut commands, &a, &area, c, wh), bridge.card_gen));
        }
        (Some((e, _)), None) => {
            commands.entity(e).despawn();
            ui.card = None;
        }
        _ => {}
    }
}

fn set_text(t: &mut Text, s: String) {
    if t.0 != s {
        t.0 = s;
    }
}

fn set_display(n: &mut Node, show: bool) {
    let d = if show { Display::Flex } else { Display::None };
    if n.display != d {
        n.display = d;
    }
}

fn zone_line(h: &Hud) -> (String, Color) {
    let t = clock(h.zone_secs);
    match h.stage {
        0 => (format!("DROP IN! Glide to your landing spot  {}", t), CREAM),
        1 => (format!("Phase {} - the Blight moves in {}", h.phase, t), if h.zone_secs < 12.0 { GOLD_HI } else { CREAM }),
        2 => (format!("Phase {} - the Blight is closing  {}", h.phase, t), hex(0xff9be8)),
        _ => ("The last safe patch - fight!".to_string(), RED_HI),
    }
}

#[allow(clippy::type_complexity)]
fn update_hud(
    bridge: Res<LssBridge>,
    touch: Res<TouchControls>,
    time: Res<Time>,
    mut texts: ParamSet<(
        Query<&mut Text, With<AliveText>>,
        Query<&mut Text, With<KillText>>,
        Query<(&mut Text, &mut TextColor), With<ZoneText>>,
        Query<&mut Text, With<DistText>>,
        Query<&mut Text, With<HpText>>,
        Query<&mut Text, With<StatusText>>,
        Query<(&FeedText, &mut Text, &mut TextColor)>,
        Query<(&SlotLabel, &mut Text)>,
    )>,
    mut nodes: ParamSet<(
        Query<(&mut Node, &mut BackgroundColor), With<HpFill>>,
        Query<&mut Node, With<ShieldFill>>,
        Query<&mut Node, With<HintText>>,
        Query<&mut Node, With<Hotbar>>,
        Query<&mut Node, With<SpecBar>>,
        Query<&mut BackgroundColor, With<Vignette>>,
        Query<(&SlotCell, &mut BorderColor, &mut BackgroundColor)>,
    )>,
    mut mode: ResMut<ModeButtons>,
) {
    let h = bridge.hud;
    let spectating = bridge.spectate.is_some();
    let dead = spectating || bridge.card.as_ref().is_some_and(|c| c.kind != CardKind::Win && h.hp <= 0.0);
    for mut t in texts.p0().iter_mut() {
        set_text(&mut t, h.alive.to_string());
    }
    for mut t in texts.p1().iter_mut() {
        set_text(&mut t, format!("{} KOs", h.kills));
    }
    for (mut t, mut c) in texts.p2().iter_mut() {
        let (s, col) = zone_line(&h);
        set_text(&mut t, s);
        if c.0 != col {
            c.0 = col;
        }
    }
    for mut t in texts.p3().iter_mut() {
        set_text(&mut t, if h.stage == 3 || spectating { String::new() } else { format!("ZONE {}m", (h.zone_dist / 100.0) as i32) });
    }
    for mut t in texts.p4().iter_mut() {
        set_text(&mut t, format!("{}", h.hp.max(0.0).ceil() as i32));
    }
    for mut t in texts.p5().iter_mut() {
        set_text(&mut t, if h.outside && !dead && !h.dropping { "IN THE BLIGHT!".to_string() } else { String::new() });
    }
    for (f, mut t, mut c) in texts.p6().iter_mut() {
        // Newest at the bottom.
        let n = bridge.feed.len();
        let first = n.saturating_sub(FEED_LINES);
        match bridge.feed.get(first + f.0) {
            Some(line) => {
                set_text(&mut t, line.text.clone());
                let a = (1.0 - ((line.age - 5.0) / 2.0).clamp(0.0, 1.0)).clamp(0.0, 1.0);
                let col = if line.mine { GOLD_HI } else { CREAM }.with_alpha(a);
                if c.0 != col {
                    c.0 = col;
                }
            }
            None => set_text(&mut t, String::new()),
        }
    }
    let frac = (h.hp / MAX_HP).clamp(0.0, 1.0);
    for (mut n, mut c) in nodes.p0().iter_mut() {
        let w = Val::Percent(frac * 100.0);
        if n.width != w {
            n.width = w;
        }
        let col = if frac > 0.5 { hex(0x39c24a) } else if frac > 0.25 { hex(0xf0b429) } else { hex(0xe5302a) };
        if c.0 != col {
            c.0 = col;
        }
    }
    for mut n in nodes.p1().iter_mut() {
        let w = Val::Percent((h.shield / 60.0).clamp(0.0, 1.0) * 100.0);
        if n.width != w {
            n.width = w;
        }
    }
    for mut n in nodes.p2().iter_mut() {
        set_display(&mut n, h.hint && !dead && crate::hints::show());
    }
    let touch_ui = touch.visible;
    for mut n in nodes.p3().iter_mut() {
        set_display(&mut n, !touch_ui && !dead);
    }
    for mut n in nodes.p4().iter_mut() {
        set_display(&mut n, spectating);
    }
    for mut bg in nodes.p5().iter_mut() {
        let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 5.0).sin();
        let a = if h.outside && !dead && !h.dropping { 0.14 + 0.1 * pulse } else { 0.0 };
        let col = Color::srgba(0.45, 0.05, 0.6, a);
        if bg.0 != col {
            bg.0 = col;
        }
    }
    // Slots: hotbar cells and touch buttons share one description.
    let labels: [String; 3] = std::array::from_fn(|i| {
        let s = bridge.slots[i];
        match kind_of(s.kind) {
            None => if i == 2 { "ITEM".to_string() } else { "-".to_string() },
            Some(k) => {
                if i == 2 || s.ammo > 1 {
                    format!("{}\nx{}", k.short(), s.ammo)
                } else {
                    k.short().to_string()
                }
            }
        }
    });
    let tint: [Color; 3] = std::array::from_fn(|i| {
        let s = bridge.slots[i];
        match kind_of(s.kind) {
            None => Color::srgba(0.09, 0.14, 0.23, 0.6),
            Some(k) => kind_color(k).mix(&Color::srgb(0.05, 0.1, 0.25), 0.45).with_alpha(0.92),
        }
    });
    for (cell, mut border, mut bg) in nodes.p6().iter_mut() {
        let s = bridge.slots[cell.0];
        let b = if s.kind == 0 { NAVY } else { rarity_color(s.rarity) };
        if border.top != b {
            *border = BorderColor::all(b);
        }
        let ready = if s.cooldown > 0.0 { 0.55 } else { 1.0 };
        let col = tint[cell.0].with_alpha(0.85 * ready);
        if bg.0 != col {
            bg.0 = col;
        }
    }
    for (l, mut t) in texts.p7().iter_mut() {
        let key = ["[1]", "[2]", "[3]"][l.0];
        set_text(&mut t, format!("{}\n{}", labels[l.0], key));
    }
    let want_touch = bridge.active && touch_ui && !dead;
    if mode.active != want_touch {
        mode.active = want_touch;
    }
    if want_touch {
        for i in 0..3 {
            if mode.labels[i] != labels[i] {
                mode.labels[i] = labels[i].clone();
            }
            let s = bridge.slots[i];
            let c = if s.kind == 0 { Color::srgba(0.09, 0.14, 0.23, 0.6) } else { tint[i] };
            if mode.tint[i] != Some(c) {
                mode.tint[i] = Some(c);
            }
        }
    }
}

fn update_map(
    bridge: Res<LssBridge>,
    mut map: ParamSet<(Query<&mut Node, With<MapCur>>, Query<&mut Node, With<MapNext>>, Query<&mut Node, With<MapMe>>, Query<&mut Node, With<MapPtr>>)>,
) {
    let dead = bridge.spectate.is_some() || bridge.hud.hp <= 0.0;
    let m = bridge.map;
    let to =|p: [f32; 2]| -> (f32, f32) {
        let r = m.arena[2].max(1.0);
        let k = (MAP - 4.0) / 2.0 / r * 0.97;
        (MAP / 2.0 - 2.0 + (p[0] - m.arena[0]) * k, MAP / 2.0 - 2.0 + (p[1] - m.arena[1]) * k)
    };
    let circle = |n: &mut Node, c: [f32; 3], show: bool| {
        let r = m.arena[2].max(1.0);
        let k = (MAP - 4.0) / 2.0 / r * 0.97;
        let (x, y) = to([c[0], c[1]]);
        let rad = c[2] * k;
        n.left = Val::Px(x - rad);
        n.top = Val::Px(y - rad);
        n.width = Val::Px(rad * 2.0);
        n.height = Val::Px(rad * 2.0);
        set_display(n, show);
    };
    // `cur`/`next` are (x, z, r); the map's arena is (x, z, r) too.
    for mut n in map.p0().iter_mut() {
        circle(&mut n, [m.cur[0], m.cur[1], m.cur[2]], true);
    }
    for mut n in map.p1().iter_mut() {
        circle(&mut n, [m.next[0], m.next[1], m.next[2]], m.show_next);
    }
    for mut n in map.p2().iter_mut() {
        let (x, y) = to(m.me);
        n.left = Val::Px(x - 4.0);
        n.top = Val::Px(y - 4.0);
    }
    for mut n in map.p3().iter_mut() {
        // A gold pip on the rim pointing at the next safe centre.
        let (cx, cy) = (m.next[0] - m.me[0], m.next[1] - m.me[1]);
        let l = cx.hypot(cy).max(1.0);
        let rim = MAP / 2.0 - 2.0;
        n.left = Val::Px(rim + cx / l * (rim - 3.0) - 3.5);
        n.top = Val::Px(rim + cy / l * (rim - 3.0) - 3.5);
        set_display(&mut n, m.show_next && !dead);
    }
}

fn update_spec(bridge: Res<LssBridge>, mut names: Query<&mut Text, With<SpecName>>) {
    for mut t in &mut names {
        set_text(&mut t, bridge.spectate.clone().unwrap_or_default());
    }
}

#[allow(clippy::type_complexity)]
fn update_tags(
    bridge: Res<LssBridge>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut nodes: Query<(&TagNode, &mut Node)>,
    mut names: Query<(&TagName, &mut Text)>,
    mut bars: Query<(&TagBar, &mut Node), Without<TagNode>>,
) {
    let Ok((camera, gt)) = cam.single() else { return };
    for (n, mut node) in &mut nodes {
        let tag = bridge.tags.get(n.0);
        let screen = tag.and_then(|t| camera.world_to_viewport(gt, t.pos).ok());
        match screen {
            Some(s) => {
                set_display(&mut node, true);
                node.left = Val::Px(s.x - 45.0);
                node.top = Val::Px(s.y - 22.0);
            }
            None => set_display(&mut node, false),
        }
    }
    for (n, mut t) in &mut names {
        if let Some(tag) = bridge.tags.get(n.0) {
            let name = bridge.names.get(tag.id).map_or("", |s| s.as_str());
            set_text(&mut t, name.to_string());
        }
    }
    for (n, mut node) in &mut bars {
        if let Some(tag) = bridge.tags.get(n.0) {
            node.width = Val::Percent(tag.hp.clamp(0.0, 1.0) * 100.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------------------------

fn read_input(
    mut bridge: ResMut<LssBridge>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut touch: ResMut<TouchControls>,
    cells: Query<(&Interaction, &SlotCell), Changed<Interaction>>,
    spec: Query<(&Interaction, &SpecButton), Changed<Interaction>>,
    state: Res<AppState>,
    mut menu: MessageWriter<ReturnToMenu>,
) {
    if *state != AppState::Playing {
        return;
    }
    let bits = std::mem::take(&mut touch.mode_pressed);
    for i in 0..3 {
        if bits & (1 << i) != 0 {
            bridge.slot_press[i] = true;
        }
    }
    for (k, i) in [(KeyCode::Digit1, 0), (KeyCode::Digit2, 1), (KeyCode::Digit3, 2)] {
        if keys.just_pressed(k) {
            bridge.slot_press[i] = true;
        }
    }
    for pad in &pads {
        if pad.just_pressed(GamepadButton::DPadLeft) {
            bridge.slot_press[0] = true;
        }
        if pad.just_pressed(GamepadButton::DPadUp) {
            bridge.slot_press[1] = true;
        }
        if pad.just_pressed(GamepadButton::DPadRight) {
            bridge.slot_press[2] = true;
        }
    }
    for (i, c) in &cells {
        if *i == Interaction::Pressed {
            bridge.slot_press[c.0] = true;
        }
    }
    if bridge.spectate.is_some() {
        for (i, b) in &spec {
            if *i == Interaction::Pressed {
                match b {
                    SpecButton::Prev => bridge.spec_step = -1,
                    SpecButton::Next => bridge.spec_step = 1,
                    SpecButton::Menu => {
                        menu.write(ReturnToMenu);
                    }
                }
            }
        }
        if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyE) || pads.iter().any(|p| p.just_pressed(GamepadButton::RightTrigger)) {
            bridge.spec_step = 1;
        }
        if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyQ) || pads.iter().any(|p| p.just_pressed(GamepadButton::LeftTrigger)) {
            bridge.spec_step = -1;
        }
    }
}

fn card_input(mut bridge: ResMut<LssBridge>, buttons: Query<(&Interaction, &CardButton), Changed<Interaction>>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, state: Res<AppState>, mut menu: MessageWriter<ReturnToMenu>) {
    if bridge.card.is_none() || !bridge.card_ready || *state != AppState::Playing {
        return;
    }
    for (i, b) in &buttons {
        if *i == Interaction::Pressed {
            match b {
                CardButton::Retry => bridge.retry = true,
                CardButton::Spectate => bridge.spectate_start = true,
                CardButton::Difficulty => bridge.diff_press = true,
                CardButton::Menu => {
                    menu.write(ReturnToMenu);
                }
            }
        }
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::KeyR) || pads.iter().any(|p| p.just_pressed(GamepadButton::South)) {
        bridge.retry = true;
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
        let mut b = LssBridge::default();
        b.slot_press = [true; 3];
        b.retry = true;
        b.feed.push(FeedLine { text: "x".into(), age: 0.0, mine: false });
        b.reset(vec!["YOU".into()]);
        assert_eq!((b.pickups.len(), b.shots.len(), b.rings.len()), (PICKUP_SLOTS, SHOT_SLOTS, RING_SLOTS));
        assert!(b.feed.is_empty() && !b.retry && b.slot_press == [false; 3] && b.card.is_none());
        assert!(b.pickups.iter().all(|p| p.kind == 0));
    }

    #[test]
    fn caps_are_distinct_and_text_is_ascii() {
        let caps: std::collections::HashSet<u32> = (0..MAX_FIGHTERS).map(cap_color).collect();
        assert_eq!(caps.len(), MAX_FIGHTERS, "every Mario needs his own cap colour");
        for n in NAMES {
            assert!(clean(n.to_string()).is_ascii(), "{n}");
        }
        for k in ALL_KINDS {
            assert!(clean(k.name().to_string()).is_ascii() && clean(k.hint().to_string()).is_ascii());
        }
        assert!(zone_line(&Hud { stage: 1, phase: 2, zone_secs: 20.0, ..default() }).0.is_ascii());
    }

    /// On phones (874x402 and friends, with notch insets) the slot buttons, minimap, pause and
    /// stick/action buttons must all fit the screen and never overlap each other.
    #[test]
    fn phone_hud_layout_has_no_overlaps() {
        use crate::touch::{layout, mode_layout, pause_rect};
        let phones = [
            (Vec2::new(874.0, 402.0), SafeArea { left: 62.0, right: 62.0, bottom: 21.0, top: 0.0 }),
            (Vec2::new(844.0, 390.0), SafeArea { left: 47.0, right: 47.0, bottom: 21.0, top: 0.0 }),
            (Vec2::new(932.0, 430.0), SafeArea { left: 62.0, right: 62.0, bottom: 21.0, top: 0.0 }),
            (Vec2::new(667.0, 375.0), SafeArea::default()),
        ];
        let overlaps = |a: Rect, b: Rect| a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y;
        for (size, area) in phones {
            let inset = Vec3::new(area.left, area.right, area.bottom);
            let mut rects: Vec<(String, Rect)> = layout(size, inset).into_iter().map(|(c, r)| (format!("{c:?}"), r)).collect();
            rects.extend(mode_layout(size, inset).into_iter().map(|(c, r)| (format!("{c:?}"), r)));
            rects.push(("pause".into(), pause_rect(size, area.right, area.top)));
            rects.push(("minimap".into(), minimap_rect(size, &area)));
            for (name, r) in &rects {
                assert!(r.min.x >= 0.0 && r.max.x <= size.x && r.min.y >= 0.0 && r.max.y <= size.y, "{name} off screen at {size}: {r:?}");
            }
            for i in 0..rects.len() {
                for j in i + 1..rects.len() {
                    assert!(!overlaps(rects[i].1, rects[j].1), "{} overlaps {} at {size}", rects[i].0, rects[j].0);
                }
            }
        }
    }

    #[test]
    fn zone_line_follows_the_stage() {
        for stage in 0..4 {
            let (s, _) = zone_line(&Hud { stage, phase: 3, zone_secs: 65.0, ..default() });
            assert!(!s.is_empty());
        }
        assert!(zone_line(&Hud { stage: 1, phase: 3, zone_secs: 65.0, ..default() }).0.contains("1:05"));
        assert!(zone_line(&Hud { stage: 2, phase: 3, zone_secs: 9.0, ..default() }).0.contains("closing"));
    }
}
