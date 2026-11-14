//! Gem Clash: drawing the course hazards of a real-world region (`brawl_hazard.rs`). Each hazard is its own course's model (a
//! Chain Chomp on its post with its chain, the Whomp King, Thwomps, iron balls, OoT horses, a Tweester, Boos, Redeads ...) and a
//! telegraph laid on the ground: a red ring or lane that fills and pulses faster as the strike comes. A child module of `brawl.rs`.
use super::*;
use crate::render::SCALE;

/// Model, base scale and lift (local units the model's origin sits above the hazard's position).
fn look_model(look: Look) -> (Option<&'static str>, f32, f32) {
    match look {
        Look::ChainChomp => (Some("actors/chain_chomp/chain_chomp_geo.glb"), 1.5, 0.0),
        Look::DekuBaba => (Some("levels/oot_enemies/deku_baba.glb"), 1.6, 95.0),
        Look::Whomp => (Some("actors/whomp/whomp_geo.glb"), 1.0, 0.0),
        Look::WhompKing => (Some("actors/whomp/whomp_geo.glb"), 1.45, 0.0),
        Look::Thwomp => (Some("actors/thwomp/thwomp_geo.glb"), 1.3, 0.0),
        Look::IronBall => (Some("actors/bowling_ball/bowling_ball_geo.glb"), 0.75, 85.0),
        Look::Boulder => (None, 100.0, 100.0),
        Look::Snowball => (None, 110.0, 110.0),
        Look::Horse => (Some("levels/oot_residents/horse_normal.glb"), 1.0, 0.0),
        Look::Goomba => (Some("actors/goomba/goomba_geo.glb"), 4.2, 0.0),
        Look::Wave => (None, 1.0, 0.0),
        Look::Tornado => (Some("actors/tornado/tweester_geo.glb"), 0.13, 0.0),
        Look::Boo => (Some("actors/boo/boo_geo.glb"), 1.9, 0.0),
        Look::Redead => (Some("levels/oot_enemies/redead.glb"), 1.0, 0.0),
    }
}

struct Ents {
    look: Look,
    body: Entity,
    tele: Entity,
    tele_mat: Handle<StandardMaterial>,
    /// Chain Chomp: the post and the chain links.
    post: Option<Entity>,
    links: Vec<Entity>,
}

#[derive(Default)]
pub struct HazardViews {
    sig: Vec<(Look, [i32; 2])>,
    ents: Vec<Ents>,
    lane: Option<Handle<Mesh>>,
    ring: Option<Handle<Mesh>>,
}

impl HazardViews {
    /// Forget the entities (the experiment's own cleanup despawns them on exit).
    pub fn forget(&mut self) {
        self.sig.clear();
        self.ents.clear();
    }

    fn despawn(&mut self, host: &mut Host) {
        for e in self.ents.drain(..) {
            for x in [Some(e.body), Some(e.tele), e.post].into_iter().flatten().chain(e.links) {
                host.commands.entity(x).despawn();
            }
        }
        self.sig.clear();
    }

    fn build(&mut self, host: &mut Host, hz: &[Hazard]) {
        self.despawn(host);
        let lane = self.lane.get_or_insert_with(|| host.meshes.add(Plane3d::default().mesh().size(1.0, 1.0))).clone();
        let ring = self.ring.get_or_insert_with(|| host.meshes.add(Annulus::new(0.8, 1.0))).clone();
        for h in hz {
            let (model, base, _) = look_model(h.look);
            let body = match (model, h.look) {
                (Some(path), _) if host.asset_root.exists(path) => {
                    let k = if path.starts_with("levels/") { crate::zelda::glb::bevy_scale(&host.asset_root.0.join(path)) } else { 1.0 };
                    let inner = crate::render::object_view::spawn_model(host.commands, host.assets, host.asset_root, path, true).insert(Transform::from_scale(Vec3::splat(base * k))).id();
                    let parent = host.spawn((Transform::default(), Visibility::Hidden));
                    host.commands.entity(parent).add_child(inner);
                    parent
                }
                (_, Look::Wave) => {
                    // A long rolling crest: a flattened ball, white-blue and see-through.
                    let mesh = host.meshes.add(Sphere::new(0.5).mesh().ico(3).unwrap());
                    let mat = host.materials.add(StandardMaterial { base_color: Color::srgba(0.82, 0.93, 1.0, 0.7), emissive: LinearRgba::rgb(0.25, 0.35, 0.45), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.15, ..default() });
                    host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden))
                }
                (_, look) => {
                    // Boulders and snowballs (and any model missing from this machine): a textured ball.
                    let mesh = host.meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap());
                    let mat = match look {
                        Look::Snowball => host.materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.97, 1.0), perceptual_roughness: 0.8, ..default() }),
                        _ => textured(host, "stone", Color::srgb(0.62, 0.56, 0.5)),
                    };
                    host.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden))
                }
            };
            let tele_mat = host.materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.2, 0.1, 0.0), emissive: LinearRgba::rgb(0.6, 0.05, 0.0), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, depth_bias: 40.0, ..default() });
            let circle = matches!(h.spec().kind, HzKind::Lunger) || matches!(h.look, Look::Redead | Look::Tornado | Look::Boo);
            let tele = host.spawn((Mesh3d(if circle { ring.clone() } else { lane.clone() }), MeshMaterial3d(tele_mat.clone()), Transform::default(), Visibility::Hidden));
            let (mut post, mut links) = (None, vec![]);
            if h.look == Look::DekuBaba {
                let mesh = host.meshes.add(Cylinder::new(1.0, 1.0));
                let green = host.materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.5, 0.15), perceptual_roughness: 0.8, ..default() });
                post = Some(host.spawn((Mesh3d(mesh), MeshMaterial3d(green), Transform::default(), Visibility::Hidden)));
            }
            if h.look == Look::ChainChomp {
                let wood = textured(host, "planks", Color::srgb(0.7, 0.5, 0.3));
                let mesh = host.meshes.add(Cylinder::new(28.0 * SCALE, 140.0 * SCALE));
                let p = bv(Vec3::from(h.home)) + Vec3::Y * 70.0 * SCALE;
                post = Some(host.spawn((Mesh3d(mesh), MeshMaterial3d(wood), Transform::from_translation(p), Visibility::Inherited)));
                let link_mesh = host.meshes.add(Torus::new(9.0 * SCALE, 20.0 * SCALE));
                let iron = host.materials.add(StandardMaterial { base_color: Color::srgb(0.18, 0.18, 0.2), metallic: 0.6, perceptual_roughness: 0.4, ..default() });
                for _ in 0..6 {
                    links.push(host.spawn((Mesh3d(link_mesh.clone()), MeshMaterial3d(iron.clone()), Transform::default(), Visibility::Hidden)));
                }
            }
            self.ents.push(Ents { look: h.look, body, tele, tele_mat, post, links });
        }
        self.sig = hz.iter().map(|h| (h.look, [h.home[0] as i32, h.home[2] as i32])).collect();
    }

    /// Place every hazard and its telegraph. `prev` holds the hazards' positions one core tick ago and `a` the blend toward now.
    pub fn sync(&mut self, host: &mut Host, core: &Brawl, prev: &[[f32; 3]; MAXH], a: f32, t: f32) {
        let hz = &core.arena.hazards;
        let sig: Vec<(Look, [i32; 2])> = hz.iter().map(|h| (h.look, [h.home[0] as i32, h.home[2] as i32])).collect();
        if sig != self.sig {
            self.build(host, hz);
        }
        let live = core.phase != Phase::MatchEnd;
        for (k, h) in hz.iter().enumerate() {
            let Some(e) = self.ents.get(k) else { continue };
            let sp = h.spec();
            let pos = mix3(prev[k], h.pos, a);
            let (_, base, lift) = look_model(h.look);
            // ---- body ----
            let mut rot = Quat::from_rotation_y(h.yaw);
            let mut scale = Vec3::ONE;
            let mut at = pos + Vec3::Y * lift;
            match h.look {
                Look::Whomp | Look::WhompKing => {
                    // Rear back during the warning, fall flat on its face, lie there, then push itself up.
                    let tilt = match h.st {
                        HzState::Warn => -0.28 * (h.t as f32 / sp.warn as f32).min(1.0),
                        HzState::Act => -0.28 + (std::f32::consts::FRAC_PI_2 + 0.28) * (h.t as f32 / sp.act as f32).min(1.0),
                        HzState::Rest => {
                            let up = sp.rest.saturating_sub(16);
                            if h.t > up { std::f32::consts::FRAC_PI_2 * (1.0 - (h.t - up) as f32 / 16.0) } else { std::f32::consts::FRAC_PI_2 }
                        }
                        HzState::Idle => 0.04 * (t * 5.0).sin(),
                    };
                    rot *= Quat::from_rotation_x(tilt);
                }
                Look::IronBall | Look::Boulder | Look::Snowball => {
                    // Roll along the lane.
                    let r = sp.radius;
                    rot = Quat::from_axis_angle(Vec3::new(h.dir[1], 0.0, -h.dir[0]).normalize_or(Vec3::X), -h.run / r);
                    if h.look != Look::IronBall {
                        scale = Vec3::splat(r * SCALE);
                        at = pos + Vec3::Y * r;
                    }
                }
                Look::Wave => {
                    let w = core.arena.half[0].max(core.arena.half[1]) * 2.2;
                    let swell = 1.0 + 0.08 * (t * 9.0).sin();
                    scale = Vec3::new(w * SCALE, sp.height * 2.0 * swell * SCALE, sp.radius * 2.6 * SCALE);
                    at = pos;
                }
                Look::Tornado => rot = Quat::from_rotation_y(t * 9.0),
                Look::Thwomp => {
                    if h.st == HzState::Warn && h.t + 12 >= sp.warn {
                        // Locked: a shiver before the drop.
                        at.x += (t * 70.0).sin() * 6.0;
                    }
                }
                Look::Boo => {
                    // Shy: shrinks a little while its target is looking.
                    let tgt = h.target;
                    if tgt >= 0 {
                        let f = &core.f[tgt as usize];
                        let to = norm2([h.pos[0] - f.pos[0], h.pos[2] - f.pos[2]], [0.0, 1.0]);
                        let fc = dir_of(f.yaw);
                        if fc[0] * to[0] + fc[1] * to[1] > 0.2 {
                            scale = Vec3::splat(0.8);
                        }
                    }
                }
                Look::Goomba => {
                    if h.st == HzState::Act {
                        rot *= Quat::from_rotation_z(0.08 * (t * 10.0).sin());
                    }
                }
                _ => {}
            }
            let _ = base;
            let show = live && h.shown();
            host.commands.entity(e.body).insert((Transform::from_translation(bv(at)).with_rotation(rot).with_scale(scale), if show { Visibility::Inherited } else { Visibility::Hidden }));
            // ---- Deku Baba stem: from its root to the head ----
            if let (Some(stem), Look::DekuBaba) = (e.post, h.look) {
                let root = bv(Vec3::from(h.home));
                let head = bv(at);
                let d = head - root;
                let len = d.length().max(0.01);
                let tf = Transform::from_translation(root + d * 0.5).with_rotation(Quat::from_rotation_arc(Vec3::Y, d / len)).with_scale(Vec3::new(9.0 * SCALE, len, 9.0 * SCALE));
                host.commands.entity(stem).insert((tf, if show { Visibility::Inherited } else { Visibility::Hidden }));
            }
            // ---- chain ----
            if !e.links.is_empty() {
                let anchor = Vec3::from(h.home) + Vec3::Y * 110.0;
                let head = pos + Vec3::Y * 90.0;
                let n = e.links.len();
                for (i, l) in e.links.iter().enumerate() {
                    let k = (i as f32 + 1.0) / (n as f32 + 1.0);
                    let sag = (k * std::f32::consts::PI).sin() * 40.0 * (1.0 - (dist2([anchor.x, anchor.z], [head.x, head.z]) / sp.reach).min(1.0));
                    let p = anchor.lerp(head, k) - Vec3::Y * sag;
                    let r = Quat::from_rotation_y(h.yaw + i as f32 * 1.57) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    host.commands.entity(*l).insert((Transform::from_translation(bv(p)).with_rotation(r), if live { Visibility::Inherited } else { Visibility::Hidden }));
                }
            }
            // ---- telegraph ----
            let zone = h.zone();
            let warn = h.st == HzState::Warn || (sp.kind == HzKind::Roller && h.st == HzState::Act);
            let drifter = sp.kind == HzKind::Drifter && !(h.look == Look::Redead && h.st == HzState::Warn);
            let (alpha, tint) = if !live {
                (0.0, [1.0, 0.2, 0.1])
            } else if drifter {
                // A faint ring under the drifters: where their touch reaches.
                (0.16, [0.75, 0.55, 1.0])
            } else if warn {
                let k = h.warn_k();
                let pulse = 0.5 + 0.5 * (t * (8.0 + 18.0 * k)).sin();
                ((0.45 + 0.3 * k + 0.2 * pulse).min(0.9), [1.0, 0.12, 0.05])
            } else if h.st == HzState::Act {
                (0.55, [1.0, 0.85, 0.3])
            } else {
                (0.0, [1.0, 0.2, 0.1])
            };
            if let Some(mut m) = host.materials.get_mut(&e.tele_mat) {
                let want = Color::srgba(tint[0], tint[1], tint[2], alpha);
                if m.base_color != want {
                    m.base_color = want;
                }
            }
            let ground_y = |x: f32, z: f32, y: f32| core.arena.support(x, z, y + 120.0).map_or(y, |s| s.0) + 6.0;
            let tf = match zone {
                Some(Zone::Circle { c, r }) => {
                    let y = ground_y(c[0], c[1], if sp.kind == HzKind::Drifter { h.pos[1] - if h.look == Look::Boo { 70.0 } else { 0.0 } } else { h.aim[1] });
                    Some(Transform::from_translation(bv(Vec3::new(c[0], y, c[1]))).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(r * SCALE)))
                }
                Some(Zone::Lane { a: s, dir, len, half }) => {
                    let wide = if h.look == Look::Wave { core.arena.half[0].max(core.arena.half[1]) * 2.2 } else { half * 2.0 };
                    let (len, wide) = if h.look == Look::Wave { (len.min(sp.radius * 2.0 + 120.0), wide) } else { (len, wide) };
                    let mid = [s[0] + dir[0] * len * 0.5, s[1] + dir[1] * len * 0.5];
                    let y = ground_y(mid[0], mid[1], if sp.kind == HzKind::Roller { h.pos[1] } else { h.pos[1] });
                    Some(Transform::from_translation(bv(Vec3::new(mid[0], y, mid[1]))).with_rotation(Quat::from_rotation_y(yaw_of(dir))).with_scale(Vec3::new(wide * SCALE, 1.0, len * SCALE)))
                }
                Some(Zone::Square { c, half }) => {
                    let y = ground_y(c[0], c[1], h.aim[1]);
                    Some(Transform::from_translation(bv(Vec3::new(c[0], y, c[1]))).with_scale(Vec3::new(half * 2.0 * SCALE, 1.0, half * 2.0 * SCALE)))
                }
                None => None,
            };
            match tf {
                Some(tf) if alpha > 0.01 => drop(host.commands.entity(e.tele).insert((tf, Visibility::Inherited))),
                _ => drop(host.commands.entity(e.tele).insert(Visibility::Hidden)),
            }
            let _ = e.look;
        }
    }
}
