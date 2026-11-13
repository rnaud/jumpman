//! Gem Clash attack visuals in the shared N64 look (`fx::n64`). `brawl_view::sync_render` keeps owning the pooled
//! entities (hit rings, flashes, beams, power auras, gem pillars, shell markers); this module runs right after it,
//! moves each onto an [`N64Fx`] material once, re-meshes it (low-poly rings, camera-facing sprites, lightning
//! ribbons) and drives the per-effect state from the same bridge lists. On the high tier every flash also gets a
//! spinning star behind it. With `JUMPMAN_FX_QUALITY=off` none of this runs and the flat materials stay.
use super::brawl_core::{GEMS, GEM_COLORS, MAXF};
use super::brawl_view::{slot_color, BrawlBridge, FxKind, Pools, FX_MAX};
use super::XpEntity;
use crate::fx::n64::{self, pat, Mount, N64Fx, N64Meshes, Quality};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct BrawlFx {
    built: bool,
    ring: Vec<Handle<N64Fx>>,
    flash: Vec<Handle<N64Fx>>,
    beam: Vec<Handle<N64Fx>>,
    /// The star layer behind each flash (high tier).
    star: Vec<(Entity, Handle<N64Fx>)>,
    aura: Vec<Handle<N64Fx>>,
    floor_ring: Vec<Handle<N64Fx>>,
    pillar: Vec<Handle<N64Fx>>,
    telegraph: Handle<N64Fx>,
    bullet: Handle<N64Fx>,
    fireball: Handle<N64Fx>,
}

/// Forget the handles once the mode is left.
pub fn reset(mut fx: ResMut<BrawlFx>) {
    if fx.built {
        *fx = BrawlFx::default();
    }
}

fn build(fx: &mut BrawlFx, commands: &mut Commands, mats: &mut Assets<N64Fx>, meshes: &N64Meshes) {
    let c = |r, g, b| Color::srgb(r, g, b);
    fx.ring = (0..FX_MAX).map(|i| mats.add(N64Fx::new(pat::RING, Color::WHITE, true).snap(3.0).seed(i as f32 * 0.173 % 1.0))).collect();
    fx.flash = (0..FX_MAX).map(|i| mats.add(N64Fx::new(pat::BURST, c(1.0, 0.6, 0.15), true).mount(Mount::Sprite).snap(2.0).seed(i as f32 * 0.211 % 1.0))).collect();
    fx.beam = (0..FX_MAX).map(|i| mats.add(N64Fx::new(pat::BEAM, Color::WHITE, true).mount(Mount::Ribbon).seed(i as f32 * 0.37 % 1.0))).collect();
    fx.star = (0..FX_MAX)
        .map(|i| {
            let m = mats.add(N64Fx::new(pat::STAR, Color::WHITE, true).mount(Mount::Sprite).snap(2.0).spin(2.5).seed(i as f32 * 0.29 % 1.0));
            // With its mesh from the start: `adopt` only restyles entities that have one.
            (commands.spawn((XpEntity, Mesh3d(meshes.quad.clone()), MeshMaterial3d(m.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster)).id(), m)
        })
        .collect();
    fx.aura = (0..MAXF).map(|i| mats.add(N64Fx::new(pat::AURA, c(1.0, 0.82, 0.25), true).seed(i as f32 * 0.3))).collect();
    fx.floor_ring = (0..MAXF).map(|i| mats.add(N64Fx::new(pat::RING, slot_color(i), true).variant(6).snap(2.0))).collect();
    fx.pillar = (0..GEMS).map(|i| mats.add(N64Fx::new(pat::PILLAR, Color::srgb(GEM_COLORS[i][0], GEM_COLORS[i][1], GEM_COLORS[i][2]), true).extent(2.0).seed(i as f32 * 0.33))).collect();
    fx.telegraph = mats.add(N64Fx::new(pat::RING, c(1.0, 0.2, 0.15), true).variant(5).snap(2.0));
    fx.bullet = mats.add(N64Fx::new(pat::STAR, c(1.0, 1.0, 0.85), true).mount(Mount::Sprite).snap(2.0).spin(5.0));
    fx.fireball = mats.add(N64Fx::new(pat::BURST, c(1.0, 0.5, 0.12), true).mount(Mount::Sprite).snap(2.0).variant(1));
    fx.built = true;
}

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

/// Write an effect's state when it changed (pattern and variant included: a slot is reused by every kind of effect).
fn touch(mats: &mut Assets<N64Fx>, h: &Handle<N64Fx>, pattern: f32, variant: u32, color: [f32; 3], k: f32, intensity: f32) {
    let Some(cur) = mats.get(h) else { return };
    let mut probe = cur.clone();
    let changed = probe.set(color, k, intensity) || probe.p0.x != pattern || probe.p2.x != variant as f32;
    if changed {
        if let Some(mut m) = mats.get_mut(h) {
            m.p0.x = pattern;
            m.p2.x = variant as f32;
            m.set(color, k, intensity);
        }
    }
}

fn srgb(c: Color) -> [f32; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue]
}

#[allow(clippy::too_many_arguments)]
pub fn style(
    mut commands: Commands,
    mut fx: ResMut<BrawlFx>,
    meshes: Res<N64Meshes>,
    mut mats: ResMut<Assets<N64Fx>>,
    bridge: Res<BrawlBridge>,
    pools: Res<Pools>,
    look: Query<(&Mesh3d, Has<MeshMaterial3d<StandardMaterial>>, Option<&MeshMaterial3d<N64Fx>>)>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
) {
    if !pools.is_ready() {
        return;
    }
    if fx.built && fx.star.first().is_some_and(|(e, _)| !tfs.contains(*e)) {
        *fx = BrawlFx::default();
    }
    if !fx.built {
        build(&mut fx, &mut commands, &mut mats, &meshes);
        return;
    }
    let fx = &mut *fx;
    let high = n64::quality() == Quality::High;
    let t = bridge.time;

    // Under each fighter: its ring; round it: the power-form glow.
    for i in 0..MAXF.min(pools.ring.len()) {
        let f = bridge.fighters[i];
        let col = if f.power { f.tint } else { srgb(slot_color(i)) };
        adopt(&mut commands, pools.ring[i], Some(&meshes.ring), &fx.floor_ring[i], &look);
        touch(&mut mats, &fx.floor_ring[i], pat::RING, 6, col, 0.0, 1.0);
        let tint = if f.power { f.tint } else { [1.0, 0.82, 0.25] };
        adopt(&mut commands, pools.aura[i], Some(&meshes.sphere), &fx.aura[i], &look);
        touch(&mut mats, &fx.aura[i], pat::AURA, 0, tint, 0.0, if f.power && f.on { 1.0 } else { 0.0 });
    }
    for (g, e) in pools.pillar.iter().enumerate() {
        adopt(&mut commands, *e, Some(&meshes.pillar), &fx.pillar[g], &look);
    }
    for e in &pools.telegraph {
        adopt(&mut commands, *e, Some(&meshes.ring), &fx.telegraph, &look);
    }
    // Projectiles: bullets are glowing stars, mortar shots fireballs.
    for (k, slot) in pools.proj.iter().enumerate() {
        let Some((kind, e)) = *slot else { continue };
        let p = bridge.projs[k];
        match kind {
            0 | 4 => {
                adopt(&mut commands, e, Some(&meshes.quad), if kind == 0 { &fx.bullet } else { &fx.fireball }, &look);
                if p.on {
                    if let Ok((mut tf, _)) = tfs.get_mut(e) {
                        tf.scale *= if kind == 0 { 2.2 } else { 1.9 };
                    }
                }
            }
            _ => {}
        }
    }

    // Hit rings, flashes and beams.
    let (mut r_i, mut f_i, mut b_i) = (0usize, 0usize, 0usize);
    for fv in bridge.fx.iter().filter(|f| f.on) {
        let k = (fv.age / fv.life.max(0.01)).clamp(0.0, 1.0);
        let col = if fv.color == [0.0; 3] {
            match fv.kind {
                FxKind::Flash => [1.0, 0.6, 0.15],
                _ => [1.0, 1.0, 0.95],
            }
        } else {
            fv.color
        };
        match fv.kind {
            FxKind::Ring if r_i < pools.fx_ring.len() => {
                let e = pools.fx_ring[r_i];
                adopt(&mut commands, e, Some(&meshes.ring), &fx.ring[r_i], &look);
                // A big shock wave is jagged, a combo ring crisp.
                let variant = if fv.radius >= 500.0 { 1 } else { 0 };
                touch(&mut mats, &fx.ring[r_i], pat::RING, variant, col, k, (1.0 - k) * 1.5);
                r_i += 1;
            }
            FxKind::Flash if f_i < pools.fx_flash.len() => {
                let e = pools.fx_flash[f_i];
                adopt(&mut commands, e, Some(&meshes.quad), &fx.flash[f_i], &look);
                let spark = fv.radius < 90.0;
                let intensity = 1.0 - ((k - 0.33) / 0.67).max(0.0).powi(2);
                touch(&mut mats, &fx.flash[f_i], if spark { pat::STAR } else { pat::BURST }, 0, col, k, intensity);
                let mut at = Vec3::ZERO;
                let mut size = 0.0;
                if let Ok((mut tf, _)) = tfs.get_mut(e) {
                    tf.scale *= if spark { 1.9 } else { 1.35 };
                    // Lift the sprite so its lower half is not cut off by the ground.
                    tf.translation.y += tf.scale.y * 0.5;
                    at = tf.translation;
                    size = tf.scale.x;
                }
                // A spinning star behind the big flashes: the extra glow layer of the high tier.
                let (se, sm) = &fx.star[f_i];
                if let Ok((mut tf, mut v)) = tfs.get_mut(*se) {
                    let show = high && !spark;
                    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
                    if *v != want {
                        *v = want;
                    }
                    if show {
                        tf.translation = at;
                        tf.scale = Vec3::splat(size * 1.5);
                    }
                }
                if high && !spark {
                    adopt(&mut commands, *se, Some(&meshes.quad), sm, &look);
                    touch(&mut mats, sm, pat::STAR, 0, [1.0, 0.95, 0.7], k, intensity * 0.8);
                }
                f_i += 1;
            }
            FxKind::Beam if b_i < pools.fx_beam.len() => {
                let e = pools.fx_beam[b_i];
                adopt(&mut commands, e, Some(&meshes.ribbon), &fx.beam[b_i], &look);
                touch(&mut mats, &fx.beam[b_i], pat::BEAM, 0, col, k, 1.0 - k * k * 0.7);
                if let Ok((mut tf, _)) = tfs.get_mut(e) {
                    // The legacy box is a thin bar; the ribbon needs room for the jagged bolt.
                    tf.scale.x = 0.75 * (1.0 - 0.3 * k);
                    tf.scale.y = 1.0;
                }
                b_i += 1;
            }
            _ => {}
        }
    }
    // Flashes that ended: their star layers go too.
    for (se, _) in fx.star.iter().skip(f_i) {
        if let Ok((_, mut v)) = tfs.get_mut(*se) {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
        }
    }
    let _ = t;
}

/// `JUMPMAN_FX_GALLERY=1`: fill the hit-effect list with one of every kind round the player, looping, so the look can be
/// checked without a fight (test hook; runs between the host and `sync_render`).
pub fn gallery_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("JUMPMAN_FX_GALLERY").is_some())
}

pub fn gallery(mut bridge: ResMut<BrawlBridge>) {
    use super::brawl_view::FxView;
    let t = bridge.time;
    let c = bridge.fighters[0].pos;
    let defs: [(FxKind, f32, f32, [f32; 3]); 9] = [
        (FxKind::Ring, 150.0, 0.35, [1.0, 1.0, 1.0]),
        (FxKind::Ring, 260.0, 0.4, [1.0, 0.85, 0.3]),
        (FxKind::Ring, 700.0, 0.9, [0.3, 0.55, 1.0]),
        (FxKind::Flash, 60.0, 0.2, [1.0, 1.0, 0.92]),
        (FxKind::Flash, 140.0, 0.4, [0.0; 3]),
        (FxKind::Flash, 340.0, 0.6, [0.0; 3]),
        (FxKind::Beam, 0.0, 0.5, [0.75, 0.5, 1.0]),
        (FxKind::Flash, 220.0, 0.4, [0.75, 0.5, 1.0]),
        (FxKind::Ring, 520.0, 0.7, [1.0, 0.3, 0.25]),
    ];
    for (i, f) in bridge.fx.iter_mut().enumerate() {
        *f = FxView::default();
        if let Some(d) = defs.get(i) {
            let a = i as f32 / defs.len() as f32 * std::f32::consts::TAU;
            let pos = c + Vec3::new(a.cos() * 3.2, 0.3, a.sin() * 3.2);
            let age = (t * 0.8 + i as f32 * 0.31) % (d.2 + 0.5);
            if age < d.2 {
                *f = FxView { on: true, kind: d.0, pos, to: pos + Vec3::new(1.5, 1.4, -1.0), color: d.3, age, life: d.2, radius: d.1 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_writes_only_on_change() {
        let mut mats = Assets::<N64Fx>::default();
        let h = mats.add(N64Fx::new(pat::RING, Color::WHITE, true));
        touch(&mut mats, &h, pat::BURST, 1, [1.0, 0.5, 0.1], 0.5, 1.0);
        let m = mats.get(&h).unwrap();
        assert_eq!((m.p0.x, m.p2.x), (pat::BURST, 1.0));
        assert!((m.p0.y - 0.5).abs() < 0.02);
    }
}
