//! Star Swarm attack visuals in the shared N64 look (`fx::n64`). `swarm_view::sync_render` keeps owning the pooled
//! entities and their transforms; this module runs right after it and swaps each pooled effect entity onto an
//! [`N64Fx`] material (once per entity), re-meshes the ones that want a different shape (rings become low-poly rings,
//! discs, pie slices or camera-facing sprites), and drives the per-frame state from the same bridge lists. It also owns
//! a few extra pooled layers the old look never had: lightning bolts over strike rings, kill pops, and the level-up
//! and chest bursts. With `JUMPMAN_FX_QUALITY=off` none of this runs and the original flat materials stay.
//!
//! Everything is pooled (fixed arrays sized once), materials are shared per weapon where they carry no per-effect
//! state, and an asset is only written when its state actually changed.
use super::swarm_core::*;
use super::swarm_view::{Pools, SwarmBridge};
use super::XpEntity;
use crate::fx::n64::{self, pat, Mount, N64Fx, N64Meshes};
use crate::render::SCALE;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

/// A pickup's sprite: (ICON variant, sRGB colour, sprite half size in SM64 units) by its look (gem tiers 0..=2, heart,
/// chest, magnet, bomb).
fn gem_look(look: u8) -> (u32, [f32; 3], f32) {
    match look {
        // The colours of the old flat pickups, so a tier still reads at a glance.
        0 => (0, [1.0, 0.85, 0.1], 40.0),
        1 => (0, [0.3, 0.65, 1.0], 44.0),
        2 => (0, [1.0, 0.3, 0.3], 48.0),
        3 => (1, [1.0, 0.45, 0.65], 56.0),
        4 => (2, [1.0, 0.72, 0.15], 70.0),
        5 => (3, [0.35, 0.95, 1.0], 52.0),
        _ => (4, [0.95, 0.25, 0.1], 52.0),
    }
}

const POPS: usize = 28;

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Ring,
    Disc,
    Sector,
    Sprite,
}

/// How a ring-list kind is drawn.
#[derive(Clone, Copy)]
struct Look {
    pattern: f32,
    variant: u32,
    color: [f32; 3],
    additive: bool,
    shape: Shape,
}

fn ring_look(kind: u8) -> Look {
    let l = |pattern, variant, color, additive, shape| Look { pattern, variant, color, additive, shape };
    match kind {
        1 => l(pat::RING, 0, [0.5, 0.95, 1.0], true, Shape::Ring),
        2 => l(pat::RING, 1, [1.0, 0.65, 0.2], true, Shape::Ring),
        4 => l(pat::RING, 2, [1.0, 0.95, 0.45], true, Shape::Ring),
        5 => l(pat::RING, 4, [0.95, 0.97, 1.0], true, Shape::Ring),
        6 => l(pat::BURST, 0, [1.0, 0.5, 0.15], true, Shape::Sprite),
        7 => l(pat::SLASH, 0, [0.42, 0.78, 1.0], true, Shape::Sector),
        8 => l(pat::RING, 3, [0.55, 0.85, 1.0], true, Shape::Ring),
        9 => l(pat::RING, 2, [1.0, 0.2, 0.12], true, Shape::Ring),
        10 => l(pat::ZONE, 0, [1.0, 0.42, 0.1], true, Shape::Disc),
        11 => l(pat::ZONE, 1, [0.45, 0.9, 0.25], false, Shape::Disc),
        12 => l(pat::ZONE, 2, [0.5, 0.2, 0.85], false, Shape::Disc),
        _ => l(pat::RING, 0, [1.0, 0.5, 0.8], true, Shape::Ring),
    }
}

fn ring_material(look: &Look, slot: usize) -> N64Fx {
    let c = look.color;
    let mount = if look.shape == Shape::Sprite { Mount::Sprite } else { Mount::Mesh };
    N64Fx::new(look.pattern, Color::srgb(c[0], c[1], c[2]), look.additive).mount(mount).snap(3.0).variant(look.variant).seed(slot as f32 * 0.173 % 1.0)
}

#[derive(Clone, Copy, Default)]
struct Pop {
    on: bool,
    born: f32,
    life: f32,
    pos: Vec3,
    size: f32,
    color: [f32; 3],
}

#[derive(Resource, Default)]
pub struct SwarmFx {
    built: bool,
    bolt: Vec<Handle<N64Fx>>,
    orb: Handle<N64Fx>,
    orbit: Handle<N64Fx>,
    boom: Handle<N64Fx>,
    marker: Handle<N64Fx>,
    /// XP gems, hearts, chests, magnets and bombs: one tagged sprite material for all of them.
    icon: Handle<N64Fx>,
    post: Handle<N64Fx>,
    beacon: Vec<Handle<N64Fx>>,
    /// One material per ring slot (a slot changes kind when it is reused).
    ring: Vec<Handle<N64Fx>>,
    ring_kind: Vec<u8>,
    /// Lightning over strike rings, one per ring slot.
    bolts: Vec<(Entity, Handle<N64Fx>)>,
    pops: Vec<(Entity, Handle<N64Fx>)>,
    pop_state: [Pop; POPS],
    prev_kind: Vec<u8>,
    prev_pos: Vec<Vec3>,
    prev_level: u32,
    prev_chests: u32,
}

/// Forget the handles once the mode is left (the entities go with the experiment).
pub fn reset(mut fx: ResMut<SwarmFx>) {
    if fx.built {
        *fx = SwarmFx::default();
    }
}

impl SwarmFx {
    fn spawn_pop(&mut self, now: f32, pos: Vec3, size: f32, life: f32, color: [f32; 3]) {
        if let Some(i) = self.pop_state.iter().position(|p| !p.on) {
            self.pop_state[i] = Pop { on: true, born: now, life, pos, size, color };
        }
    }
}

fn build(fx: &mut SwarmFx, commands: &mut Commands, mats: &mut Assets<N64Fx>, meshes: &N64Meshes, bridge: &SwarmBridge) {
    let c = |r, g, b| Color::srgb(r, g, b);
    fx.bolt = vec![
        mats.add(N64Fx::new(pat::STAR, c(1.0, 0.92, 0.35), true).mount(Mount::Sprite).snap(2.0).spin(3.0)),
        mats.add(N64Fx::new(pat::BURST, c(1.0, 0.5, 0.1), true).mount(Mount::Sprite).snap(2.0).variant(1)),
        mats.add(N64Fx::new(pat::BURST, c(1.0, 0.3, 0.3), true).mount(Mount::Sprite).snap(2.0).variant(2)),
        mats.add(N64Fx::new(pat::STAR, c(0.6, 0.95, 1.0), true).mount(Mount::Sprite).snap(2.0).spin(0.0)),
    ];
    fx.orb = mats.add(N64Fx::new(pat::ORB, c(0.85, 0.35, 1.0), false).mount(Mount::Sprite).snap(2.0));
    fx.orbit = mats.add(N64Fx::new(pat::BLADE, c(0.45, 0.95, 1.0), true).mount(Mount::Sprite).snap(2.0).spin(7.0));
    fx.boom = mats.add(N64Fx::new(pat::BOOM, c(0.55, 1.0, 0.8), true).mount(Mount::Sprite).snap(2.0));
    fx.icon = mats.add(N64Fx::new(pat::ICON, Color::WHITE, false).mount(Mount::Sprite).snap(2.0).tagged());
    fx.marker = mats.add(N64Fx::new(pat::RING, c(0.7, 1.0, 1.0), true).variant(6).snap(2.0));
    fx.post = mats.add(N64Fx::new(pat::PILLAR, c(1.0, 0.45, 0.12), true).extent(1.0 / (380.0 * SCALE)));
    fx.beacon = vec![mats.add(N64Fx::new(pat::PILLAR, c(1.0, 0.3, 0.2), true).extent(1.0 / (750.0 * SCALE)).seed(0.3)), mats.add(N64Fx::new(pat::PILLAR, c(1.0, 0.85, 0.2), true).extent(1.0 / (750.0 * SCALE)).seed(0.6))];
    fx.ring = (0..bridge.rings.len()).map(|i| mats.add(ring_material(&ring_look(1), i))).collect();
    fx.ring_kind = vec![0; bridge.rings.len()];
    // The extra layers carry their mesh and material from the start (`adopt` only restyles entities that have a mesh).
    let hidden = |commands: &mut Commands, mesh: &Handle<Mesh>, m: &Handle<N64Fx>| commands.spawn((XpEntity, Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster)).id();
    fx.bolts = (0..RING_SLOTS)
        .map(|i| {
            let m = mats.add(N64Fx::new(pat::BEAM, c(1.0, 0.95, 0.5), true).mount(Mount::Ribbon).seed(i as f32 * 0.31 % 1.0));
            (hidden(commands, &meshes.ribbon, &m), m)
        })
        .collect();
    fx.pops = (0..POPS)
        .map(|i| {
            let m = mats.add(N64Fx::new(pat::POP, c(1.0, 0.95, 0.6), true).mount(Mount::Sprite).snap(2.0).seed(i as f32 * 0.137 % 1.0));
            (hidden(commands, &meshes.quad, &m), m)
        })
        .collect();
    fx.prev_kind = vec![0; bridge.enemies.len()];
    fx.prev_pos = vec![Vec3::ZERO; bridge.enemies.len()];
    fx.built = true;
}

/// Put `e` on `mat` (and `mesh` when given); legacy materials are dropped once.
fn adopt(commands: &mut Commands, e: Entity, mesh: Option<&Handle<Mesh>>, mat: &Handle<N64Fx>, look: &Query<(&Mesh3d, Has<MeshMaterial3d<StandardMaterial>>, Option<&MeshMaterial3d<N64Fx>>)>) {
    let Ok((m, legacy, cur)) = look.get(e) else { return };
    let new_mat = cur.is_none_or(|c| c.0 != *mat);
    let new_mesh = mesh.filter(|h| m.0 != **h);
    if !legacy && !new_mat && new_mesh.is_none() {
        return;
    }
    let mut ec = commands.entity(e);
    if legacy {
        ec.remove::<MeshMaterial3d<StandardMaterial>>();
    }
    if new_mat {
        ec.insert(MeshMaterial3d(mat.clone()));
    }
    if let Some(h) = new_mesh {
        ec.insert(Mesh3d(h.clone()));
    }
}

fn touch(mats: &mut Assets<N64Fx>, h: &Handle<N64Fx>, color: [f32; 3], k: f32, intensity: f32) {
    if mats.get(h).is_some_and(|m| {
        let mut probe = m.clone();
        probe.set(color, k, intensity)
    }) {
        if let Some(mut m) = mats.get_mut(h) {
            m.set(color, k, intensity);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn style(
    mut commands: Commands,
    mut fx: ResMut<SwarmFx>,
    meshes: Res<N64Meshes>,
    mut mats: ResMut<Assets<N64Fx>>,
    bridge: Res<SwarmBridge>,
    pools: Res<Pools>,
    time: Res<Time>,
    look: Query<(&Mesh3d, Has<MeshMaterial3d<StandardMaterial>>, Option<&MeshMaterial3d<N64Fx>>)>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
    mut tags: Query<&mut MeshTag>,
) {
    if !pools.ready() {
        return;
    }
    // The pooled extras vanish with the experiment's entities (a retry): rebuild then.
    if fx.built && fx.pops.first().is_some_and(|(e, _)| !tfs.contains(*e)) {
        *fx = SwarmFx::default();
    }
    if !fx.built {
        // The extra layers spawned here show up next frame; the pooled entities are restyled right away.
        build(&mut fx, &mut commands, &mut mats, &meshes, &bridge);
    }
    let fx = &mut *fx;
    let now = time.elapsed_secs();

    // Bolts: star shot, fireball, missile, sword beam as sprites.
    for (i, slot) in pools.bolt.iter().enumerate() {
        let Some((e, which)) = *slot else { continue };
        adopt(&mut commands, e, Some(&meshes.quad), &fx.bolt[which as usize], &look);
        if bridge.bolts[i].kind != 0 {
            if let Ok((mut tf, _)) = tfs.get_mut(e) {
                tf.scale *= 42.0 * SCALE * if which == 1 { 2.4 } else { 2.1 };
            }
        }
    }
    for (i, e) in pools.orb.iter().enumerate() {
        let Some(e) = *e else { continue };
        adopt(&mut commands, e, Some(&meshes.quad), &fx.orb, &look);
        if bridge.orbs[i].kind != 0 {
            if let Ok((mut tf, _)) = tfs.get_mut(e) {
                tf.scale *= 38.0 * SCALE * 1.9;
            }
        }
    }
    for (i, e) in pools.orbit.iter().enumerate() {
        let Some(e) = *e else { continue };
        adopt(&mut commands, e, Some(&meshes.quad), &fx.orbit, &look);
        if bridge.orbit[i].kind != 0 {
            if let Ok((mut tf, _)) = tfs.get_mut(e) {
                tf.scale *= 52.0 * SCALE * 2.1;
            }
        }
    }
    for (i, e) in pools.boom.iter().enumerate() {
        let Some(e) = *e else { continue };
        adopt(&mut commands, e, Some(&meshes.quad), &fx.boom, &look);
        if bridge.booms[i].kind != 0 {
            if let Ok((mut tf, _)) = tfs.get_mut(e) {
                tf.scale *= 60.0 * SCALE * 1.5;
            }
        }
    }
    // Pickups: 16-texel sprites that turn like the console's (gem tiers, heart, chest, magnet coin, bomb ball).
    for (i, slot) in pools.gem.iter().enumerate() {
        let Some((e, _)) = *slot else { continue };
        let item = bridge.gems[i];
        adopt(&mut commands, e, Some(&meshes.quad), &fx.icon, &look);
        if item.kind == 0 {
            continue;
        }
        let (variant, color, radius) = gem_look(item.aux as u8);
        if let Ok((mut tf, _)) = tfs.get_mut(e) {
            tf.scale *= radius * SCALE;
        }
        let tag = n64::pack_tag(color, 1.0, 0.0, variant, i as f32 * 0.137);
        match tags.get_mut(e) {
            Ok(mut t) => {
                if t.value != tag {
                    t.value = tag;
                }
            }
            Err(_) => {
                commands.entity(e).insert(MeshTag::new(tag));
            }
        }
    }
    if let Some(e) = pools.marker {
        adopt(&mut commands, e, Some(&meshes.ring), &fx.marker, &look);
    }
    for e in pools.post.iter().flatten() {
        adopt(&mut commands, *e, None, &fx.post, &look);
    }
    for (e, which) in pools.beacon.iter().flatten() {
        adopt(&mut commands, *e, None, &fx.beacon[*which as usize & 1], &look);
    }

    // Rings and ground zones.
    for (i, slot) in pools.ring.iter().enumerate() {
        let Some((e, _, _)) = slot else { continue };
        let item = bridge.rings[i];
        let strike = &fx.bolts[i.min(RING_SLOTS - 1)];
        if item.kind == 0 {
            if i < RING_SLOTS {
                if let Ok((_, mut v)) = tfs.get_mut(strike.0) {
                    if *v != Visibility::Hidden {
                        *v = Visibility::Hidden;
                    }
                }
            }
            continue;
        }
        let kind = item.kind;
        let lk = ring_look(kind);
        if fx.ring_kind[i] != kind {
            fx.ring_kind[i] = kind;
            if let Some(mut m) = mats.get_mut(&fx.ring[i]) {
                *m = ring_material(&lk, i);
            }
        }
        let mesh = match lk.shape {
            Shape::Ring => &meshes.ring,
            Shape::Disc => &meshes.disc,
            Shape::Sector => &meshes.sector,
            Shape::Sprite => &meshes.quad,
        };
        adopt(&mut commands, *e, Some(mesh), &fx.ring[i], &look);
        let k = 1.0 - item.aux;
        let intensity = match kind {
            10 => 0.9,
            11 => 0.8,
            12 => 0.95,
            // Swings overlap in heavy runs: kept below full so stacked additive arcs stay blue, not white.
            7 => (item.aux * 1.2).min(0.8),
            _ => (item.aux * 1.4).min(1.0),
        };
        touch(&mut mats, &fx.ring[i], lk.color, k, intensity);
        if lk.shape == Shape::Sprite {
            if let Ok((mut tf, _)) = tfs.get_mut(*e) {
                tf.scale *= 1.2;
                tf.translation.y += tf.scale.y * 0.4;
            }
        }
        // A strike's lightning: a bolt from the sky onto the footprint.
        if kind == 4 && i < RING_SLOTS {
            let (se, sm) = (fx.bolts[i].0, fx.bolts[i].1.clone());
            if let Ok((mut tf, mut v)) = tfs.get_mut(se) {
                let height = 9.0;
                tf.translation = item.pos + Vec3::Y * height * 0.5;
                tf.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                tf.scale = Vec3::new((item.scale * 0.55).max(0.3), 1.0, height);
                if *v != Visibility::Inherited {
                    *v = Visibility::Inherited;
                }
            }
            adopt(&mut commands, se, Some(&meshes.ribbon), &sm, &look);
            touch(&mut mats, &sm, [1.0, 0.95, 0.5], k, (item.aux * 1.6).min(1.0));
        } else if i < RING_SLOTS {
            if let Ok((_, mut v)) = tfs.get_mut(strike.0) {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
            }
        }
    }

    // Kill pops: a foe slot that goes empty near the player. Level-up and chest bursts.
    let me = bridge.marker.pos;
    for (i, item) in bridge.enemies.iter().enumerate() {
        if i >= fx.prev_kind.len() {
            break;
        }
        if fx.prev_kind[i] != 0 && item.kind == 0 && fx.prev_pos[i].distance_squared(me) < 14.0 * 14.0 {
            let p = fx.prev_pos[i];
            fx.spawn_pop(now, p + Vec3::Y * 0.5, 0.9, 0.38, [1.0, 0.9, 0.55]);
        }
        fx.prev_kind[i] = item.kind;
        if item.kind != 0 {
            fx.prev_pos[i] = item.pos;
        }
    }
    let (level, chests) = (bridge.hud.level, bridge.hud.chests);
    if bridge.marker.kind != 0 {
        if level > fx.prev_level && fx.prev_level > 0 {
            fx.spawn_pop(now, me + Vec3::Y * 1.0, 3.2, 0.8, [1.0, 0.9, 0.3]);
            fx.spawn_pop(now, me + Vec3::Y * 1.0, 2.0, 0.6, [0.5, 1.0, 1.0]);
        }
        if chests > fx.prev_chests {
            fx.spawn_pop(now, me + Vec3::Y * 1.0, 2.6, 0.7, [1.0, 0.78, 0.2]);
        }
        fx.prev_level = level;
        fx.prev_chests = chests;
    }
    let (pops, pop_state) = (&fx.pops, &mut fx.pop_state);
    for (i, p) in pop_state.iter_mut().enumerate() {
        let (e, h) = &pops[i];
        let Ok((mut tf, mut v)) = tfs.get_mut(*e) else { continue };
        let k = (now - p.born) / p.life.max(0.01);
        if !p.on || k >= 1.0 {
            p.on = false;
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
            continue;
        }
        if *v != Visibility::Inherited {
            *v = Visibility::Inherited;
        }
        tf.translation = p.pos;
        tf.scale = Vec3::splat(p.size);
        adopt(&mut commands, *e, Some(&meshes.quad), h, &look);
        touch(&mut mats, h, p.color, k, 1.0 - k * k);
    }
}

/// `JUMPMAN_FX_GALLERY=1`: every ring and zone kind laid out round the player on a loop, plus a level-up every few
/// seconds, so the rarely seen effects (strike lightning, frost ring, fire zone, black hole, level-up and chest bursts)
/// can be screenshotted without a long run (test hook; runs between the experiment and `sync_render`).
pub fn gallery(mut bridge: ResMut<SwarmBridge>) {
    let t = bridge.time;
    let me = bridge.marker.pos;
    let kinds: [u8; 8] = [1, 2, 4, 5, 6, 7, 8, 9];
    for (i, item) in bridge.rings.iter_mut().enumerate() {
        item.kind = 0;
        let kind = if i < RING_SLOTS { kinds.get(i).copied() } else { [10u8, 11, 12].get(i - RING_SLOTS).copied() };
        let Some(kind) = kind else { continue };
        let n = if i < RING_SLOTS { kinds.len() } else { 3 };
        let j = if i < RING_SLOTS { i } else { i - RING_SLOTS };
        let a = j as f32 / n as f32 * std::f32::consts::TAU + if i < RING_SLOTS { 0.0 } else { 0.5 };
        let r = if i < RING_SLOTS { 5.5 } else { 9.0 };
        let life = if kind >= 10 { 1.0 } else { 0.4 + (t * 0.7 + j as f32 * 0.37) % 0.6 };
        *item = super::swarm_view::RItem { kind, pos: me + Vec3::new(a.cos() * r, 0.1, a.sin() * r), yaw: a, scale: if kind >= 10 { 2.6 } else { 2.0 }, aux: 1.0 - (t * 0.9 + j as f32 * 0.29) % 1.0 * life };
    }
    // A level-up and a chest every 3 s.
    let beat = (t / 3.0) as u32;
    bridge.hud.level += beat;
    bridge.hud.chests += beat / 2;
}

/// Cheap sanity checks of the look tables.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pickup_has_a_sprite() {
        for look in 0..=6u8 {
            let (v, c, r) = gem_look(look);
            assert!(v <= 4 && r > 0.0 && c.iter().all(|x| (0.0..=1.0).contains(x)), "look {look}");
        }
    }

    #[test]
    fn every_ring_kind_has_a_look() {
        for kind in [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
            let l = ring_look(kind);
            assert!(l.pattern >= 0.0 && l.color.iter().all(|c| (0.0..=1.0).contains(c)), "kind {kind}");
        }
    }

    #[test]
    fn a_reused_slot_gets_a_fresh_material_for_its_kind() {
        let fire = ring_material(&ring_look(10), 3);
        let frost = ring_material(&ring_look(8), 3);
        assert_ne!(fire, frost);
        assert!(fire.is_additive());
        assert!(!ring_material(&ring_look(11), 0).is_additive(), "poison blends");
        let _ = n64::quality();
    }
}
