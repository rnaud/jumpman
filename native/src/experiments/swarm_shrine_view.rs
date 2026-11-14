//! Star Swarm: drawing the level's shrines and pots (`swarm_shrines.rs`).
//!
//! The experiment fills `ChestBoard::shrines` / `pots` (in the [`SwarmBridge`]) every frame with the ones near the
//! player. This module owns small pools of low-poly pieces: a six-sided stone plinth and column tinted by the shrine's
//! kind, its pixel icon (`swarm_icons::shrine_pixels`, nearest-sampled) turning on top like a console pickup, and the
//! N64-look effects (`fx::n64`): a light pillar over a shrine that still has something to give, and for a charge shrine
//! its circle on the ground plus a disc that fills as it charges. Pots are faceted clay jars.
use super::swarm_view::SwarmBridge;
use super::XpEntity;
use crate::fx::n64::{pat, N64Fx, N64Meshes};
use crate::render::SCALE;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub const SHRINES_DRAWN: usize = 12;
pub const POTS_DRAWN: usize = 40;
const KINDS: usize = 5;

#[derive(Clone, Copy, Default, Debug)]
pub struct ShrineItem {
    pub pos: Vec3,
    /// `ShrineKind as u8`.
    pub kind: u8,
    /// Still has something to give (lit, beam).
    pub live: bool,
    /// Charging or fighting.
    pub active: bool,
    pub charge: f32,
    /// Within the reveal range (the beam shows).
    pub near: bool,
}

pub use super::swarm_icons::shrine_rgb;

pub struct ShrineViewPlugin;

impl Plugin for ShrineViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShrinePools>().add_systems(Update, sync_shrines.after(super::run_frame).run_if(|b: Res<SwarmBridge>| b.active));
        app.add_systems(Update, forget.after(super::run_frame).run_if(|b: Res<SwarmBridge>| !b.active));
    }
}

struct Slot {
    root: Entity,
    stone: [Entity; 3],
    icon: Entity,
    beam: Entity,
    circle: Entity,
    fill: Entity,
    last: (u8, bool),
}

#[derive(Resource, Default)]
struct ShrinePools {
    built: bool,
    slots: Vec<Slot>,
    pots: Vec<Entity>,
    plinth: Handle<Mesh>,
    column: Handle<Mesh>,
    cap: Handle<Mesh>,
    stone: Vec<Handle<StandardMaterial>>,
    spent: Handle<StandardMaterial>,
    icon: Vec<Handle<StandardMaterial>>,
    beam: Vec<Handle<N64Fx>>,
    circle: Vec<Handle<N64Fx>>,
    fill: Vec<Handle<N64Fx>>,
}

fn icon_image(px: Vec<u8>) -> Image {
    let size = super::swarm_icons::SIZE as u32;
    let mut img = Image::new(Extent3d { width: size, height: size, depth_or_array_layers: 1 }, TextureDimension::D2, px, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    img.sampler = ImageSampler::nearest();
    img
}

#[allow(clippy::too_many_arguments)]
fn build(pools: &mut ShrinePools, commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, fx: &mut Assets<N64Fx>, images: &mut Assets<Image>, n64: &N64Meshes) {
    pools.plinth = meshes.add(Cylinder::new(110.0 * SCALE, 40.0 * SCALE).mesh().resolution(6));
    pools.column = meshes.add(Cylinder::new(40.0 * SCALE, 150.0 * SCALE).mesh().resolution(6));
    pools.cap = meshes.add(Cylinder::new(78.0 * SCALE, 22.0 * SCALE).mesh().resolution(6));
    let pot_body = meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap());
    let pot_neck = meshes.add(Cylinder::new(0.55, 0.5).mesh().resolution(6));
    for k in 0..KINDS as u8 {
        let c = shrine_rgb(k);
        let stone = Color::srgb(0.42 + c[0] * 0.25, 0.42 + c[1] * 0.25, 0.44 + c[2] * 0.25);
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        pools.stone.push(materials.add(StandardMaterial { base_color: stone, emissive: LinearRgba::rgb(l.red * 0.25, l.green * 0.25, l.blue * 0.25), perceptual_roughness: 0.9, ..default() }));
        let tex = images.add(icon_image(super::swarm_icons::shrine_pixels(k)));
        pools.icon.push(materials.add(StandardMaterial { base_color_texture: Some(tex), unlit: true, alpha_mode: AlphaMode::Mask(0.5), cull_mode: None, double_sided: true, ..default() }));
        let col = Color::srgb(c[0], c[1], c[2]);
        pools.beam.push(fx.add(N64Fx::new(pat::PILLAR, col, true).extent(2.0).seed(k as f32 * 0.21)));
        pools.circle.push(fx.add(N64Fx::new(pat::RING, col, true).variant(6).snap(2.0)));
        pools.fill.push(fx.add(N64Fx::new(pat::ZONE, col, true).snap(2.0).seed(k as f32 * 0.37)));
    }
    pools.spent = materials.add(StandardMaterial { base_color: Color::srgb(0.36, 0.36, 0.38), perceptual_roughness: 1.0, ..default() });
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for _ in 0..SHRINES_DRAWN {
        let root = commands.spawn((XpEntity, Transform::default(), Visibility::Hidden)).id();
        let stone = [
            commands.spawn((Mesh3d(pools.plinth.clone()), MeshMaterial3d(pools.stone[0].clone()), Transform::from_xyz(0.0, 20.0 * SCALE, 0.0), ChildOf(root))).id(),
            commands.spawn((Mesh3d(pools.column.clone()), MeshMaterial3d(pools.stone[0].clone()), Transform::from_xyz(0.0, 115.0 * SCALE, 0.0), ChildOf(root))).id(),
            commands.spawn((Mesh3d(pools.cap.clone()), MeshMaterial3d(pools.stone[0].clone()), Transform::from_xyz(0.0, 200.0 * SCALE, 0.0), ChildOf(root))).id(),
        ];
        let icon = commands.spawn((Mesh3d(n64.quad.clone()), MeshMaterial3d(pools.icon[0].clone()), Transform::from_xyz(0.0, 290.0 * SCALE, 0.0).with_scale(Vec3::splat(62.0 * SCALE)), bevy::light::NotShadowCaster, ChildOf(root))).id();
        let beam = commands.spawn((Mesh3d(n64.pillar.clone()), MeshMaterial3d(pools.beam[0].clone()), Transform::from_xyz(0.0, 800.0 * SCALE, 0.0).with_scale(Vec3::new(34.0 * SCALE, 1300.0 * SCALE, 34.0 * SCALE)), Visibility::Hidden, bevy::light::NotShadowCaster, ChildOf(root))).id();
        let r = super::swarm_shrines::CHARGE_R * SCALE;
        let circle = commands.spawn((Mesh3d(n64.ring.clone()), MeshMaterial3d(pools.circle[0].clone()), Transform::from_xyz(0.0, 6.0 * SCALE, 0.0).with_rotation(flat).with_scale(Vec3::splat(r)), Visibility::Hidden, bevy::light::NotShadowCaster, ChildOf(root))).id();
        let fill = commands.spawn((Mesh3d(n64.disc.clone()), MeshMaterial3d(pools.fill[0].clone()), Transform::from_xyz(0.0, 4.0 * SCALE, 0.0).with_rotation(flat).with_scale(Vec3::splat(r)), Visibility::Hidden, bevy::light::NotShadowCaster, ChildOf(root))).id();
        pools.slots.push(Slot { root, stone, icon, beam, circle, fill, last: (255, false) });
    }
    let clay = materials.add(StandardMaterial { base_color: Color::srgb(0.72, 0.4, 0.22), perceptual_roughness: 0.85, ..default() });
    let rim = materials.add(StandardMaterial { base_color: Color::srgb(0.5, 0.26, 0.14), perceptual_roughness: 0.85, ..default() });
    for _ in 0..POTS_DRAWN {
        let root = commands.spawn((XpEntity, Transform::default(), Visibility::Hidden)).id();
        commands.spawn((Mesh3d(pot_body.clone()), MeshMaterial3d(clay.clone()), Transform::from_xyz(0.0, 42.0 * SCALE, 0.0).with_scale(Vec3::new(44.0 * SCALE, 42.0 * SCALE, 44.0 * SCALE)), ChildOf(root)));
        commands.spawn((Mesh3d(pot_neck.clone()), MeshMaterial3d(rim.clone()), Transform::from_xyz(0.0, 88.0 * SCALE, 0.0).with_scale(Vec3::new(40.0 * SCALE, 30.0 * SCALE, 40.0 * SCALE)), ChildOf(root)));
        pools.pots.push(root);
    }
    pools.built = true;
}

#[allow(clippy::too_many_arguments)]
fn sync_shrines(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fx: ResMut<Assets<N64Fx>>,
    mut images: ResMut<Assets<Image>>,
    n64: Option<Res<N64Meshes>>,
    bridge: Res<SwarmBridge>,
    time: Res<Time>,
    mut pools: ResMut<ShrinePools>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
) {
    let Some(n64) = n64 else { return };
    // The pooled entities go with the experiment's (a retry): build again then.
    if pools.built && pools.slots.first().is_some_and(|s| !tfs.contains(s.root)) {
        *pools = ShrinePools::default();
    }
    if !pools.built {
        build(&mut pools, &mut commands, &mut meshes, &mut materials, &mut fx, &mut images, &n64);
        return;
    }
    let t = time.elapsed_secs();
    let pools = &mut *pools;
    let board = &bridge.world;
    let show = |tfs: &mut Query<(&mut Transform, &mut Visibility)>, e: Entity, on: bool| {
        if let Ok((_, mut v)) = tfs.get_mut(e) {
            let want = if on { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    };
    for (i, slot) in pools.slots.iter_mut().enumerate() {
        let Some(item) = board.shrines.get(i) else {
            if let Ok((_, mut v)) = tfs.get_mut(slot.root) {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
            }
            continue;
        };
        let k = (item.kind as usize).min(KINDS - 1);
        if let Ok((mut tf, mut v)) = tfs.get_mut(slot.root) {
            tf.translation = item.pos;
            if *v != Visibility::Visible {
                *v = Visibility::Visible;
            }
        }
        if slot.last != (item.kind, item.live) {
            for e in slot.stone {
                commands.entity(e).insert(MeshMaterial3d(if item.live { pools.stone[k].clone() } else { pools.spent.clone() }));
            }
            commands.entity(slot.icon).insert(MeshMaterial3d(pools.icon[k].clone()));
            commands.entity(slot.beam).insert(MeshMaterial3d(pools.beam[k].clone()));
            commands.entity(slot.circle).insert(MeshMaterial3d(pools.circle[k].clone()));
            commands.entity(slot.fill).insert(MeshMaterial3d(pools.fill[k].clone()));
            slot.last = (item.kind, item.live);
        }
        // The icon turns and bobs over a live shrine (a spent one keeps it, still and lower).
        if let Ok((mut tf, _)) = tfs.get_mut(slot.icon) {
            let spin = if item.live { t * 1.6 + i as f32 } else { 0.0 };
            let bob = if item.live { (t * 2.2 + i as f32).sin() * 10.0 } else { -40.0 };
            tf.rotation = Quat::from_rotation_y(spin);
            tf.translation.y = (290.0 + bob) * SCALE;
        }
        show(&mut tfs, slot.beam, item.live && item.near);
        let charge = k == super::swarm_shrines::ShrineKind::Charge as usize;
        show(&mut tfs, slot.circle, charge && item.live);
        show(&mut tfs, slot.fill, (charge && item.live && item.charge > 0.0) || item.active);
        if charge {
            let c = shrine_rgb(k as u8);
            if let Some(m) = fx.get(&pools.circle[k]).cloned() {
                let mut probe = m;
                let pulse = if item.active { 0.75 + 0.25 * (t * 6.0).sin() } else { 0.55 };
                if probe.set(c, 0.0, pulse) {
                    if let Some(mut m) = fx.get_mut(&pools.circle[k]) {
                        m.set(c, 0.0, pulse);
                    }
                }
            }
        }
        if let Ok((mut tf, _)) = tfs.get_mut(slot.fill) {
            // The fill grows from the middle as the shrine charges (a challenge's fight shows its progress the same way).
            let r = super::swarm_shrines::CHARGE_R * SCALE * item.charge.clamp(0.05, 1.0);
            tf.scale = Vec3::splat(r);
        }
    }
    for (i, e) in pools.pots.iter().enumerate() {
        let Ok((mut tf, mut v)) = tfs.get_mut(*e) else { continue };
        match board.pots.get(i) {
            Some(p) => {
                tf.translation = *p;
                tf.rotation = Quat::from_rotation_y(i as f32 * 1.3);
                if *v != Visibility::Visible {
                    *v = Visibility::Visible;
                }
            }
            None => {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
            }
        }
    }
}

fn forget(mut pools: ResMut<ShrinePools>) {
    if pools.built {
        *pools = ShrinePools::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shrine_kind_has_its_own_colour() {
        let c: Vec<[f32; 3]> = (0..KINDS as u8).map(shrine_rgb).collect();
        for i in 0..c.len() {
            for j in i + 1..c.len() {
                assert_ne!(c[i], c[j]);
            }
        }
        assert_eq!(super::super::swarm_world::SHRINE_KINDS.len(), KINDS);
    }
}
