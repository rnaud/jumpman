//! Star Swarm: drawing the level's hidden chests and the compass that points at the ones already found.
//!
//! The experiment (`swarm.rs`) fills [`ChestBoard`] (part of the [`super::swarm_view::SwarmBridge`]) every frame: the chests
//! near the player (with their tier, whether they are open, and whether their light beam is lit) and the compass points of
//! the chests the player has already seen. This module owns the entities: a small pool of procedurally built chests
//! (wood, silver-blue, gold by tier) with an optional light beam, and a round compass in a screen corner whose dots are
//! the revealed chests relative to where the camera looks.
use super::swarm_view::SwarmBridge;
use crate::button_marker::BorderRadius;
use super::XpEntity;
use crate::render::SCALE;
use crate::safe_area::SafeArea;
use crate::shell::AppState;
use bevy::prelude::*;

/// Chests drawn at once (those within the draw range of the player).
pub const DRAWN: usize = 24;
/// Compass dots.
pub const DOTS: usize = 14;
/// World distance at the edge of the compass.
pub const COMPASS_RANGE: f32 = 4200.0;

#[derive(Clone, Copy, Default, Debug)]
pub struct ChestItem {
    pub pos: Vec3,
    /// 0 common, 1 rare, 2 golden.
    pub tier: u8,
    pub opened: bool,
    /// The light beam is lit (the player is close and it is not open).
    pub beam: bool,
    /// Golden chest that has been locked on a fight.
    pub busy: bool,
    /// Model scale (bigger for golden ones, smaller when the player stands on top of it so it does not hide him).
    pub scale: f32,
}

/// A revealed, unopened chest as the compass shows it.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct CompassPt {
    /// World distance to the right of the camera's view, and ahead of it.
    pub right: f32,
    pub ahead: f32,
    pub tier: u8,
}

#[derive(Default, Debug, Clone)]
pub struct ChestBoard {
    pub items: Vec<ChestItem>,
    pub compass: Vec<CompassPt>,
    pub keys: u32,
    /// Seconds left of a golden chest's fight (0 = none).
    pub fight: f32,
    pub opened: u32,
    pub total: u32,
    /// Shrines near the player and unbroken pots (drawn by `swarm_shrine_view.rs`).
    pub shrines: Vec<super::swarm_shrine_view::ShrineItem>,
    pub pots: Vec<Vec3>,
    /// A charge shrine's charge while the player holds it (0 = none).
    pub charge: f32,
}

pub struct ChestViewPlugin;

impl Plugin for ChestViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChestPools>().add_systems(Update, (sync_chests, sync_compass).chain().after(super::run_frame).run_if(|b: Res<SwarmBridge>| b.active));
        app.add_systems(Update, forget.after(super::run_frame).run_if(|b: Res<SwarmBridge>| !b.active));
    }
}

#[derive(Component)]
struct ChestRoot;
#[derive(Component)]
struct Lid;
#[derive(Component)]
struct Beam;
#[derive(Component)]
struct Body;
#[derive(Component)]
struct Band;

#[derive(Component)]
struct CompassRoot;
#[derive(Component)]
struct CompassDot(usize);
#[derive(Component)]
struct CompassText;

struct Slot {
    root: Entity,
    lid: Entity,
    beam: Entity,
    body: Entity,
    band: Entity,
    last: (u8, bool, bool),
}

#[derive(Resource, Default)]
struct ChestPools {
    meshes: Option<Meshes>,
    mats: Option<Mats>,
    slots: Vec<Slot>,
    compass: Option<Entity>,
}

struct Meshes {
    body: Handle<Mesh>,
    lid: Handle<Mesh>,
    band: Handle<Mesh>,
    beam: Handle<Mesh>,
}

struct Mats {
    /// Body material per tier.
    body: [Handle<StandardMaterial>; 3],
    band: [Handle<StandardMaterial>; 3],
    beam: [Handle<StandardMaterial>; 3],
}

/// A compass dot's colour: chest tiers 0..=2, then the shrine kinds (3 + `ShrineKind as u8`).
fn tier_color(tier: u8) -> Color {
    match tier {
        0 => Color::srgb(0.78, 0.5, 0.2),
        1 => Color::srgb(0.55, 0.78, 1.0),
        2 => Color::srgb(1.0, 0.82, 0.2),
        t => {
            let c = super::swarm_shrine_view::shrine_rgb(t - 3);
            Color::srgb(c[0], c[1], c[2])
        }
    }
}

fn build(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> (Meshes, Mats) {
    let m = Meshes {
        body: meshes.add(Cuboid::new(104.0 * SCALE, 56.0 * SCALE, 72.0 * SCALE)),
        lid: meshes.add(Cuboid::new(108.0 * SCALE, 24.0 * SCALE, 76.0 * SCALE)),
        band: meshes.add(Cuboid::new(16.0 * SCALE, 90.0 * SCALE, 80.0 * SCALE)),
        beam: meshes.add(Cylinder::new(26.0 * SCALE, 1300.0 * SCALE)),
    };
    let lit = |c: Color, glow: f32| {
        let l = c.to_linear();
        StandardMaterial { base_color: c, emissive: LinearRgba::rgb(l.red * glow, l.green * glow, l.blue * glow), perceptual_roughness: 0.55, ..default() }
    };
    let beam = |c: Color| StandardMaterial { base_color: c.with_alpha(0.34), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, ..default() };
    let mats = Mats {
        body: [materials.add(lit(Color::srgb(0.45, 0.27, 0.12), 0.5)), materials.add(lit(Color::srgb(0.32, 0.45, 0.72), 0.6)), materials.add(lit(Color::srgb(0.95, 0.72, 0.1), 1.1))],
        band: [materials.add(lit(Color::srgb(0.9, 0.78, 0.35), 0.7)), materials.add(lit(Color::srgb(0.92, 0.96, 1.0), 0.8)), materials.add(lit(Color::srgb(1.0, 0.95, 0.5), 1.4))],
        beam: [materials.add(beam(tier_color(0))), materials.add(beam(tier_color(1))), materials.add(beam(tier_color(2)))],
    };
    (m, mats)
}

fn sync_chests(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bridge: Res<SwarmBridge>,
    mut pools: ResMut<ChestPools>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
) {
    if pools.meshes.is_none() {
        let (m, mats) = build(&mut meshes, &mut materials);
        pools.meshes = Some(m);
        pools.mats = Some(mats);
    }
    let pools = &mut *pools;
    let (m, mats) = (pools.meshes.as_ref().unwrap(), pools.mats.as_ref().unwrap());
    let items = &bridge.world.items;
    while pools.slots.len() < items.len().min(DRAWN) {
        let root = commands.spawn((XpEntity, ChestRoot, Transform::default(), Visibility::Hidden)).id();
        let body = commands.spawn((Body, Mesh3d(m.body.clone()), MeshMaterial3d(mats.body[0].clone()), Transform::from_xyz(0.0, 28.0 * SCALE, 0.0), ChildOf(root))).id();
        let band = commands.spawn((Band, Mesh3d(m.band.clone()), MeshMaterial3d(mats.band[0].clone()), Transform::from_xyz(0.0, 38.0 * SCALE, 0.0), ChildOf(root))).id();
        let lid = commands.spawn((Lid, Mesh3d(m.lid.clone()), MeshMaterial3d(mats.body[0].clone()), Transform::from_xyz(0.0, 68.0 * SCALE, 0.0), ChildOf(root))).id();
        let beam = commands.spawn((Beam, Mesh3d(m.beam.clone()), MeshMaterial3d(mats.beam[0].clone()), Transform::from_xyz(0.0, 650.0 * SCALE, 0.0), Visibility::Hidden, ChildOf(root))).id();
        pools.slots.push(Slot { root, lid, beam, body, band, last: (255, false, false) });
    }
    for (i, slot) in pools.slots.iter_mut().enumerate() {
        let Some(item) = items.get(i).filter(|_| i < DRAWN) else {
            if let Ok((_, mut v)) = tfs.get_mut(slot.root) {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
            }
            continue;
        };
        if let Ok((mut tf, mut v)) = tfs.get_mut(slot.root) {
            tf.translation = item.pos;
            tf.scale = Vec3::splat(item.scale);
            if *v != Visibility::Visible {
                *v = Visibility::Visible;
            }
        }
        let state = (item.tier, item.opened, item.beam);
        if slot.last != state {
            let t = item.tier.min(2) as usize;
            commands.entity(slot.body).insert(MeshMaterial3d(mats.body[t].clone()));
            commands.entity(slot.lid).insert(MeshMaterial3d(mats.body[t].clone()));
            commands.entity(slot.band).insert(MeshMaterial3d(mats.band[t].clone()));
            commands.entity(slot.beam).insert((MeshMaterial3d(mats.beam[t].clone()), if item.beam && !item.opened { Visibility::Inherited } else { Visibility::Hidden }));
            // An open chest has its lid thrown back.
            let lid = if item.opened { Transform::from_xyz(0.0, 62.0 * SCALE, -46.0 * SCALE).with_rotation(Quat::from_rotation_x(-1.9)) } else { Transform::from_xyz(0.0, 68.0 * SCALE, 0.0) };
            commands.entity(slot.lid).insert(lid);
            slot.last = state;
        }
    }
}

fn sync_compass(
    mut commands: Commands,
    bridge: Res<SwarmBridge>,
    assets: Option<Res<crate::shell::theme::UiAssets>>,
    area: Res<SafeArea>,
    state: Res<AppState>,
    mut pools: ResMut<ChestPools>,
    mut dots: Query<(&CompassDot, &mut Node, &mut BackgroundColor, &mut Visibility), Without<CompassRoot>>,
    mut root: Query<&mut Node, With<CompassRoot>>,
    mut label: Query<&mut Text, With<CompassText>>,
) {
    let Some(assets) = assets else { return };
    let shown = *state == AppState::Playing && bridge.briefing.is_none() && bridge.world.total > 0;
    if pools.compass.is_none() {
        let a = assets.hud();
        let size = 112.0;
        let e = commands
            .spawn((XpEntity, CompassRoot, GlobalZIndex(10), Node { position_type: PositionType::Absolute, right: Val::Px(area.right.max(10.0)), top: Val::Px(area.top.max(0.0) + 10.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: Val::Px(3.0), ..default() }))
            .with_children(|p| {
                p.spawn((Node { width: Val::Px(size), height: Val::Px(size), border: UiRect::all(Val::Px(3.0)), ..default() }, BackgroundColor(Color::srgba(0.03, 0.06, 0.2, 0.55)), BorderColor::all(Color::srgba(1.0, 0.85, 0.3, 0.8)), BorderRadius::all(Val::Percent(50.0)))).with_children(|c| {
                    // The player at the middle (looking up the screen).
                    c.spawn((Node { position_type: PositionType::Absolute, left: Val::Px(size / 2.0 - 8.0), top: Val::Px(size / 2.0 - 8.0), width: Val::Px(10.0), height: Val::Px(10.0), ..default() }, BackgroundColor(Color::srgba(0.7, 1.0, 1.0, 0.95)), BorderRadius::all(Val::Percent(50.0))));
                    for i in 0..DOTS {
                        c.spawn((CompassDot(i), Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Px(11.0), height: Val::Px(11.0), ..default() }, BackgroundColor(Color::WHITE), BorderRadius::all(Val::Percent(50.0)), Visibility::Hidden));
                    }
                });
                p.spawn((CompassText, Text::new(""), crate::shell::theme::font(&a.body, 12.0), TextColor(Color::srgb(1.0, 0.92, 0.6)), TextShadow { offset: Vec2::new(0.0, 2.0), color: crate::shell::theme::NAVY }));
            })
            .id();
        pools.compass = Some(e);
        return;
    }
    for mut n in &mut root {
        let d = if shown { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    }
    let size = 112.0f32;
    let board = &bridge.world;
    for (d, mut node, mut bg, mut vis) in &mut dots {
        let Some(pt) = board.compass.get(d.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };
        // Distance maps onto the compass radius (the nearest ring is stretched so close chests are told apart).
        let dist = (pt.right * pt.right + pt.ahead * pt.ahead).sqrt().max(1.0);
        let r = (dist / COMPASS_RANGE).min(1.0).sqrt() * (size / 2.0 - 12.0);
        let (x, y) = (pt.right / dist * r, -pt.ahead / dist * r);
        node.left = Val::Px(size / 2.0 - 8.0 + x);
        node.top = Val::Px(size / 2.0 - 8.0 + y);
        let c = tier_color(pt.tier);
        if bg.0 != c {
            bg.0 = c;
        }
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
    }
    let text = if board.fight > 0.0 {
        format!("HOLD OUT {:.0}s", board.fight.ceil())
    } else if board.charge > 0.0 {
        format!("CHARGING {:.0}%", (board.charge * 100.0).floor())
    } else if board.keys > 0 {
        format!("CHESTS {}/{}  KEYS {}", board.opened, board.total, board.keys)
    } else {
        format!("CHESTS {}/{}", board.opened, board.total)
    };
    for mut t in &mut label {
        if t.0 != text {
            t.0 = text.clone();
        }
    }
}

fn forget(mut pools: ResMut<ChestPools>) {
    if pools.meshes.is_some() || pools.compass.is_some() {
        *pools = ChestPools::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_have_distinct_colours() {
        let c: Vec<_> = (0..3).map(|t| tier_color(t).to_srgba().to_f32_array()).collect();
        assert!(c[0] != c[1] && c[1] != c[2] && c[0] != c[2]);
    }

    #[test]
    fn the_board_defaults_empty() {
        let b = ChestBoard::default();
        assert!(b.items.is_empty() && b.compass.is_empty() && b.fight == 0.0);
    }
}
