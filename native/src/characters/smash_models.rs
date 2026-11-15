//! The SSB64 weapon, effect and item models (`public/models/ssb64/objects/*.glb` + `objects.json`, written by the Smash
//! import's `scripts/export-ssb64-weapons.py`, see native/SSB64.md "Weapons, effects and items") drawn for the Smash
//! fighters: the specials' projectiles from `sim.mario.smash.projectiles`, the running special's own effects (Falcon
//! Punch / Kick fire, the Reflector, PSI Magnet, Sing, Spin Attack, Thunder), hit flashes, and the item layer
//! (`jumpman_sim::smash::items`): items on the field, the one in the fighter's hand (on its `joint_itemlight_id` /
//! `joint_itemheavy_id` joint), shots. A model that is missing (no import, an older import) leaves the particles of
//! [`super::smash_fx`] in its place ([`SmashModels::covers`]).
//!
//! Item sounds: the import's item FGM clips (`voices/ssb64/FGM*_0.wav`, `manifest-ssb64-items.json`) on the item events.
//! The item layer is switched on for the courses (`JUMPMAN_SMASH_ITEMS=0` keeps it off); `JUMPMAN_SMASH_ITEM=<item>`
//! drops one in front of the fighter at start (headless checks).
use super::smash::DIR;
use super::Character;
use crate::camera::MainCamera;
use crate::fx::particles::Particles;
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::gltf::{Gltf, GltfMesh};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use jumpman_sim::smash::items::{ItemEvent, ItemKind, ItemState, UseAct, ITEM_LIFE};
use jumpman_sim::smash::{HitKind, Phase, ProjKind, Projectile, SmashFighter, SpecialMove};
use std::collections::{HashMap, HashSet};

/// The import's index of the object models.
pub const INDEX: &str = "../public/models/ssb64/objects.json";
/// glb units (SSB64 units x 0.004) to bevy units: SSB64 -> SM64 is `smash::K`, SM64 -> bevy is `SCALE`.
pub const OBJ_SCALE: f32 = (jumpman_sim::smash::K / 0.004) as f32 * SCALE;

pub struct SmashModelsPlugin;

impl Plugin for SmashModelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SmashModels>()
            .add_systems(Update, (load_models, item_setup, item_events, draw).chain().after(super::smash_fx::drain_events).before(crate::CameraSet));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjInfo {
    pub kind: String,
    /// The axis a flat (sprite) model has no extent along.
    pub plane: Option<char>,
    pub billboard: bool,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Parse `objects.json`: model name -> info, and the fighters' item joints (light, heavy).
pub fn parse_index(text: &str) -> Result<(HashMap<String, ObjInfo>, HashMap<String, (i64, i64)>), String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let objs = v["objects"].as_object().ok_or("objects.json: no objects")?;
    let f3 = |a: &serde_json::Value| -> [f32; 3] {
        let a = a.as_array().cloned().unwrap_or_default();
        [0, 1, 2].map(|k| a.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32)
    };
    let mut out = HashMap::new();
    for (name, o) in objs {
        out.insert(
            name.clone(),
            ObjInfo {
                kind: o["kind"].as_str().unwrap_or("").to_string(),
                plane: o["plane"].as_str().and_then(|s| s.chars().next()),
                billboard: o["billboard"].as_bool().unwrap_or(false),
                min: f3(&o["min"]),
                max: f3(&o["max"]),
            },
        );
    }
    let mut hands = HashMap::new();
    if let Some(h) = v["hands"].as_object() {
        for (f, j) in h {
            hands.insert(f.clone(), (j["light"].as_i64().unwrap_or(-1), j["heavy"].as_i64().unwrap_or(-1)));
        }
    }
    Ok((out, hands))
}

type Parts = Vec<(Handle<Mesh>, Handle<StandardMaterial>)>;

#[derive(Resource, Default)]
pub struct SmashModels {
    pub index: HashMap<String, ObjInfo>,
    pub hands: HashMap<String, (i64, i64)>,
    gltf: HashMap<String, Handle<Gltf>>,
    parts: HashMap<String, Parts>,
    tried: bool,
    /// Drawn entities by key (projectile / item / effect identity).
    live: HashMap<u64, (Entity, String)>,
    /// Short-lived effects (hit flashes): (key, model, position, seconds left, total).
    flashes: Vec<(u64, &'static str, Vec3, f32, f32)>,
    flash_seq: u64,
    spawned_test_item: bool,
}

impl SmashModels {
    /// The model is loaded and drawable.
    pub fn has(&self, name: &str) -> bool {
        self.parts.get(name).is_some_and(|p| !p.is_empty())
    }

    /// A model draws this projectile (its particles are not needed).
    pub fn covers(&self, p: &Projectile) -> bool {
        p.mv == SpecialMove::Item || projectile_model(p).is_some_and(|m| self.has(m))
    }

    /// A model draws the running special's effect in this phase.
    pub fn covers_special(&self, mv: SpecialMove, phase: Phase, total: u32) -> bool {
        special_effect(mv, phase, total).is_some_and(|(m, ..)| self.has(m))
    }
}

/// The model of a special's projectile (`None`: particles only).
pub fn projectile_model(p: &Projectile) -> Option<&'static str> {
    if p.mv == SpecialMove::Item {
        return match p.kind {
            ProjKind::Laser => Some("ray_gun_shot"),
            ProjKind::Star => Some("yoshi_star"),
            _ => None,
        };
    }
    Some(match p.kind {
        ProjKind::Fireball if p.fighter == SmashFighter::Luigi => "fireball_luigi",
        ProjKind::Fireball => "fireball",
        ProjKind::GreenFireball => "fireball_luigi",
        ProjKind::ChargeShot => "charge_shot",
        ProjKind::Boomerang => "boomerang",
        ProjKind::LinkBomb => "link_bomb",
        ProjKind::SamusBomb => "samus_bomb",
        ProjKind::Egg => "yoshi_egg",
        ProjKind::Star => "yoshi_star",
        ProjKind::Laser => "blaster",
        ProjKind::Cutter => "final_cutter",
        ProjKind::ThunderJolt if p.vel[1] < -1.0 => "thunder_jolt_air",
        ProjKind::ThunderJolt => "thunder_jolt",
        ProjKind::Thunderbolt => "thunder_head",
        ProjKind::PkFireSpark => "pk_fire",
        ProjKind::PkFirePillar => "pk_fire_pillar",
        ProjKind::PkThunder => "pk_thunder",
        ProjKind::Blast | ProjKind::Item => return None,
    })
}

/// How the running special's effect is drawn: model, distance ahead, height, scale.
pub fn special_effect(mv: SpecialMove, phase: Phase, total: u32) -> Option<(&'static str, f32, f32, f32)> {
    Some(match (mv, phase) {
        (SpecialMove::FalconPunch, _) if (20..34).contains(&total) => ("falcon_punch", 70.0, 100.0, 1.0),
        (SpecialMove::FalconKick, Phase::Start | Phase::Attack) if total < 30 => ("falcon_kick", 20.0, 40.0, 0.8),
        (SpecialMove::Reflector, Phase::Start | Phase::Hold) => ("reflector", 0.0, 75.0, 0.8),
        (SpecialMove::PsiMagnet, Phase::Start | Phase::Hold) => ("psi_magnet", 30.0, 80.0, 0.9),
        (SpecialMove::Sing, _) if total > 8 => ("sing", 0.0, 190.0, 1.4),
        (SpecialMove::SpinAttack, Phase::Start | Phase::Attack) => ("spin_attack", 0.0, 60.0, 0.6),
        (SpecialMove::Thunder, Phase::Attack) => ("thunder_shock", 0.0, 80.0, 1.6),
        _ => return None,
    })
}

fn load_models(root: Option<Res<AssetRoot>>, assets: Option<Res<AssetServer>>, mut models: ResMut<SmashModels>, gltfs: Res<Assets<Gltf>>, gmeshes: Res<Assets<GltfMesh>>, gmats: Res<Assets<bevy::gltf::GltfMaterial>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let (Some(root), Some(assets)) = (root, assets) else { return };
    if !models.tried {
        models.tried = true;
        let Ok(text) = std::fs::read_to_string(root.0.join(INDEX)) else { return };
        match parse_index(&text) {
            Ok((index, hands)) => {
                for name in index.keys() {
                    // `public/` sits beside the asset root (as for the fighters, `render::scene`).
                    let h: Handle<Gltf> = assets.load_builder().override_unapproved().load(format!("{DIR}/objects/{name}.glb"));
                    models.gltf.insert(name.clone(), h);
                }
                models.index = index;
                models.hands = hands;
            }
            Err(e) => warn!("Smash objects: {e}"),
        }
        return;
    }
    if models.gltf.is_empty() {
        return;
    }
    let pending: Vec<String> = models.gltf.keys().cloned().collect();
    for name in pending {
        let Some(g) = models.gltf.get(&name).and_then(|h| gltfs.get(h)) else { continue };
        let info = models.index.get(&name).cloned();
        let mut parts = vec![];
        for mh in &g.meshes {
            let Some(mesh) = gmeshes.get(mh) else { continue };
            for prim in &mesh.primitives {
                let mut m = prim.material.as_ref().and_then(|h| gmats.get(h)).map(bevy::pbr::gltf::standard_material_from_gltf_material).unwrap_or_default();
                // Weapons, effects and the sprite items are pre-lit in SSB64: unlit here. The solid items keep the scene's light.
                let flat = info.as_ref().is_some_and(|i| i.kind != "item" || i.plane.is_some());
                if flat {
                    m.unlit = true;
                }
                if info.as_ref().is_some_and(|i| i.kind != "item") && matches!(m.alpha_mode, AlphaMode::Blend) {
                    m.alpha_mode = AlphaMode::Add;
                }
                parts.push((prim.mesh.clone(), mats.add(m)));
            }
        }
        models.gltf.remove(&name);
        models.parts.insert(name, parts);
    }
}

/// Turn the item layer on where a Smash fighter plays; drop the test item.
fn item_setup(character: Res<Character>, mut sim: ResMut<GameSim>, mut models: ResMut<SmashModels>) {
    let smash = matches!(*character, Character::Smash(_));
    let want = smash && std::env::var("JUMPMAN_SMASH_ITEMS").map_or(true, |v| v != "0");
    if sim.mario.smash.items.enabled != want {
        sim.mario.smash.items.enabled = want;
    }
    if smash && !models.spawned_test_item && sim.mario.smash.fighter.is_some() && sim.ticks > 10 {
        models.spawned_test_item = true;
        if let Some(kind) = std::env::var("JUMPMAN_SMASH_ITEM").ok().and_then(|s| ItemKind::parse(&s)) {
            let m = &sim.mario;
            let yaw = jumpman_sim::math::angle_to_radians(m.face_angle[1]);
            let pos = [m.pos[0] + yaw.sin() * 60.0, m.pos[1] + 20.0, m.pos[2] + yaw.cos() * 60.0];
            sim.mario.smash.items.spawn(kind, pos);
        }
    }
}

/// The item FGM clip names an item event plays.
pub fn item_sounds(e: &ItemEvent) -> Vec<&'static str> {
    use ItemEvent as E;
    use ItemKind as K;
    let one = |s: &'static str| vec![s];
    match *e {
        E::Spawned { .. } => one("FGMItemSpawn1"),
        E::PickedUp { kind } if kind.heavy() => vec![],
        E::PickedUp { .. } | E::HammerOn => one("FGMItemGet"),
        E::Eaten { .. } => one("FGMPlayerHeal"),
        E::StarGot => one("FGMStarGet"),
        E::Thrown { heavy: true, .. } => one("FGMHeavySwing1"),
        E::Thrown { .. } => one("FGMItemThrow"),
        E::Swing { kind, level } => one(match (kind, level) {
            (K::BeamSword, 4) => "FGMSwordSwing4",
            (K::BeamSword, 3) => "FGMSwordSwing3",
            (K::BeamSword, _) => "FGMSwordSwing1",
            (K::StarRod, 4) => "FGMStarRodSwing4",
            (K::StarRod, 3) => "FGMStarRodSwing3",
            (K::StarRod, _) => "FGMStarRodSwing1",
            (K::Hammer, _) => "FGMHammerSwing",
            (_, 4) => "FGMLightSwingL",
            (_, 3) => "FGMLightSwingM",
            _ => "FGMLightSwingS",
        }),
        E::Shot { kind: K::RayGun } => one("FGMLGunShoot"),
        E::Shot { kind: K::FireFlower } => one("FGMFireFlowerShoot"),
        E::Shot { .. } => vec![],
        E::Empty { kind: K::RayGun } => one("FGMLGunEmpty"),
        E::Empty { kind: K::FireFlower } => one("FGMBurnE"),
        E::Empty { .. } => one("FGMStarRodEmpty"),
        E::Hit { kind: K::HomeRunBat, .. } => one("FGMBatHit"),
        E::Hit { kind: K::Fan, .. } => one("FGMHarisenHit"),
        E::Hit { kind: K::GreenShell | K::RedShell, .. } => one("FGMShellHit"),
        E::Hit { kind: K::Bumper, .. } => one("FGMBumperHit"),
        E::Hit { .. } => one("FGMPunchL"),
        E::Landed { kind: K::Star, .. } => one("FGMStarMapCollide"),
        E::Landed { .. } => one("FGMItemMapCollide"),
        E::Exploded { .. } => one("FGMExplodeL"),
        E::Broke { kind: K::PokeBall, .. } => one("FGMMBallOpen"),
        E::Broke { .. } => one("FGMContainerSmash"),
        E::Armed { .. } => one("FGMMSBombAttach"),
        E::Vanished { .. } => vec![],
    }
}

/// Item events into sounds and puffs.
fn item_events(mut sim: ResMut<GameSim>, audio: Option<ResMut<crate::audio::GameAudio>>, particles: Option<ResMut<Particles>>) {
    if sim.mario.smash.items.events.is_empty() {
        return;
    }
    let events = std::mem::take(&mut sim.mario.smash.items.events);
    let mut audio = audio;
    let mut particles = particles;
    for e in &events {
        if let Some(a) = audio.as_mut() {
            for name in item_sounds(e) {
                let clip = super::smash_audio::clip(name);
                a.sfx.voice_log.push(("smash:item".into(), clip.clone(), 1.0));
                if a.sfx.voice_log.len() > 256 {
                    a.sfx.voice_log.remove(0);
                }
                if !a.sfx.muted {
                    crate::audio::voices::play(&clip, 1.0);
                }
            }
        }
        if let Some(p) = particles.as_mut() {
            use crate::fx::particles::BurstOptions;
            match *e {
                ItemEvent::Spawned { pos, .. } | ItemEvent::Broke { pos, .. } => {
                    p.burst(pos, BurstOptions { color: 0xfff8d0, count: 10, speed: 200.0, life: 0.4, gravity: 0.0, size: 26.0, upward: 40.0, spread: 1.0 })
                }
                ItemEvent::Eaten { .. } | ItemEvent::StarGot => {
                    let m = sim.mario.pos;
                    p.burst([m[0], m[1] + 80.0, m[2]], BurstOptions { color: 0xfff060, count: 14, speed: 240.0, life: 0.5, gravity: 0.0, size: 24.0, upward: 80.0, spread: 1.0 });
                }
                _ => {}
            }
        }
    }
}

#[derive(Component)]
struct SmashObj;

/// One model to draw this frame.
struct Draw {
    key: u64,
    model: &'static str,
    pos: Vec3,
    rot: Quat,
    scale: f32,
    /// Face the camera: around every axis (`Full`), or only around the facing (`Axis`, the sideways fire of Falcon Punch).
    face: Face,
    visible: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Face {
    None,
    Full,
    Axis,
}

/// Rotation that turns the model's flat side (its `plane` axis) toward `to_cam`.
fn face_camera(plane: Option<char>, to_cam: Vec3) -> Quat {
    let d = to_cam.normalize_or(Vec3::Z);
    let look = Transform::IDENTITY.looking_to(-d, Vec3::Y).rotation;
    match plane {
        Some('x') => look * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        Some('y') => look * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        _ => look,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    character: Res<Character>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut models: ResMut<SmashModels>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    bodies: Query<Entity, With<super::smash::SmashBody>>,
    children: Query<&Children>,
    names: Query<(&Name, &GlobalTransform)>,
    mut tfs: Query<(&mut Transform, &mut Visibility), With<SmashObj>>,
    particles: Option<ResMut<Particles>>,
) {
    let fighter = match *character {
        Character::Smash(f) => Some(f),
        _ => None,
    };
    let ext = &sim.mario.smash;
    let mut draws: Vec<Draw> = vec![];
    let over = fixed.overstep_fraction() as f64;
    let t = time.elapsed_secs();
    let mut particles = particles;
    if fighter.is_some() && ext.fighter.is_some() && !models.parts.is_empty() {
        // Projectiles.
        for p in ext.projectiles.iter().filter(|p| !p.dead) {
            let Some(model) = projectile_model(p) else {
                if p.mv == SpecialMove::Item && p.kind == ProjKind::GreenFireball {
                    if let Some(pp) = particles.as_mut() {
                        pp.puff(p.pos, [0.0; 3], if p.age % 2 == 0 { 0xff7020 } else { 0xffc040 }, 34.0, 0.12);
                    }
                }
                continue;
            };
            if !models.has(model) {
                continue;
            }
            let pos = to_bevy([p.pos[0] + p.vel[0] * over, p.pos[1] + p.vel[1] * over, p.pos[2] + p.vel[2] * over]);
            let yaw = p.vel[0].atan2(p.vel[2]) as f32;
            let (rot, face, scale) = match p.kind {
                ProjKind::Boomerang => (Quat::from_rotation_y(t * 20.0), Face::None, 1.0),
                ProjKind::Cutter => (Quat::from_rotation_y(yaw), Face::None, 1.0),
                ProjKind::ThunderJolt if model == "thunder_jolt" => (Quat::from_rotation_y(yaw), Face::None, 0.5),
                ProjKind::ChargeShot => (Quat::IDENTITY, Face::Full, (p.radius as f32 * 2.2 * SCALE) / (0.12 * OBJ_SCALE)),
                ProjKind::LinkBomb => (Quat::IDENTITY, Face::Full, 1.0),
                ProjKind::PkFirePillar => (Quat::IDENTITY, Face::None, 0.6),
                _ => (Quat::IDENTITY, Face::Full, 1.0),
            };
            draws.push(Draw { key: (1 << 40) | p.id as u64, model, pos, rot, scale, face, visible: true });
            // Thunder's bolt and PK Thunder's tail: the head repeated up / behind.
            if p.kind == ProjKind::Thunderbolt {
                for k in 1..5u64 {
                    draws.push(Draw { key: (1 << 40) | (k << 32) | p.id as u64, model, pos: pos + Vec3::Y * (k as f32 * 0.9), rot: Quat::IDENTITY, scale: 1.0, face: Face::Full, visible: true });
                }
            }
            if p.kind == ProjKind::PkThunder && models.has("pk_thunder_trail") {
                let v = Vec3::new(p.vel[0] as f32, p.vel[1] as f32, p.vel[2] as f32).normalize_or_zero();
                for k in 1..4u64 {
                    draws.push(Draw { key: (1 << 40) | (k << 32) | p.id as u64, model: "pk_thunder_trail", pos: pos - v * (k as f32 * 0.35), rot: Quat::IDENTITY, scale: 0.8, face: Face::Full, visible: true });
                }
            }
        }
        // The running special's effect.
        if let Some(sp) = ext.special {
            if let Some((model, ahead, up, scale)) = special_effect(sp.mv, sp.phase, sp.total) {
                if models.has(model) {
                    let m = &sim.mario;
                    let yaw = jumpman_sim::math::angle_to_radians(m.face_angle[1]);
                    let pos = to_bevy([m.pos[0] + yaw.sin() * ahead as f64, m.pos[1] + up as f64, m.pos[2] + yaw.cos() * ahead as f64]);
                    let (rot, face) = match model {
                        "falcon_punch" | "falcon_kick" => (Quat::from_rotation_y(yaw as f32), Face::Axis),
                        "spin_attack" => (Quat::from_rotation_y(t * 18.0), Face::None),
                        _ => (Quat::IDENTITY, Face::Full),
                    };
                    let pulse = if model == "reflector" || model == "psi_magnet" { 1.0 + 0.08 * (t * 20.0).sin() } else { 1.0 };
                    draws.push(Draw { key: 2 << 40, model, pos, rot, scale: scale * pulse, face, visible: true });
                }
            }
        }
        // Items on the field.
        for it in &ext.items.list {
            let model = it.kind.model();
            if !models.has(model) {
                continue;
            }
            let lift = models.index.get(model).map_or(0.0, |i| -i.min[1] * OBJ_SCALE);
            let pos = to_bevy(it.pos) + Vec3::Y * lift;
            let flat = models.index.get(model).and_then(|i| i.plane).is_some();
            let blink = it.state == ItemState::Ground && it.age + 60 > ITEM_LIFE && (it.age / 3) % 2 == 0;
            let rot = Quat::from_rotation_y(it.yaw as f32 + it.spin as f32);
            draws.push(Draw { key: (3 << 40) | it.id as u64, model, pos, rot, scale: 1.0, face: if flat { Face::Full } else { Face::None }, visible: !blink });
        }
        // Hit flashes (`flash`, from the hit events `smash_fx` drains).
        let dt = time.delta_secs();
        models.flashes.retain_mut(|f| {
            f.3 -= dt;
            f.3 > 0.0
        });
        let flashes = models.flashes.clone();
        for (key, model, pos, left, total) in flashes {
            let s = 0.6 + 0.8 * (1.0 - left / total);
            draws.push(Draw { key, model, pos, rot: Quat::IDENTITY, scale: s, face: Face::Full, visible: true });
        }
    }
    // The item in hand (or the hammer): on the fighter's item joint.
    if let (Some(f), Some(kind)) = (fighter, ext.items.in_hand()) {
        let model = kind.model();
        if models.has(model) && ext.items.using.is_none_or(|u| u.act != UseAct::Throw || u.t < 3) {
            let (light, heavy) = models.hands.get(f.stem()).copied().unwrap_or((-1, -1));
            let joint = if kind.heavy() { heavy } else { light };
            let want = format!("joint_{joint:02}");
            let mut on_joint = false;
            if let Some(body) = bodies.iter().next() {
                if let Some((_, gt)) = children.iter_descendants(body).filter_map(|e| names.get(e).ok()).find(|(n, _)| n.as_str() == want) {
                    let (_, r, p) = gt.to_scale_rotation_translation();
                    on_joint = true;
                    draws.push(Draw { key: 4 << 40, model, pos: p, rot: r, scale: 1.0, face: Face::None, visible: true });
                }
            }
            if !on_joint {
                // No joint (older import): over the fighter's head / in front.
                let m = &sim.mario;
                let yaw = jumpman_sim::math::angle_to_radians(m.face_angle[1]);
                let up = if kind.heavy() { 200.0 } else { 70.0 };
                let pos = to_bevy([m.pos[0] + yaw.sin() * 40.0, m.pos[1] + up, m.pos[2] + yaw.cos() * 40.0]);
                draws.push(Draw { key: 4 << 40, model, pos, rot: Quat::from_rotation_y(yaw as f32), scale: 1.0, face: Face::None, visible: true });
            }
        }
    }
    // Star invincibility sparkles.
    if ext.items.star > 0 {
        if let Some(p) = particles.as_mut() {
            let m = sim.mario.pos;
            let a = t * 11.0;
            p.puff([m[0] + (a.sin() * 70.0) as f64, m[1] + 40.0 + ((a * 1.7).sin() * 60.0 + 60.0) as f64, m[2] + (a.cos() * 70.0) as f64], [0.0, 60.0, 0.0], if (ext.items.star / 2) % 2 == 0 { 0xfff060 } else { 0xffffff }, 22.0, 0.3);
        }
    }
    // Apply: update or spawn, then drop what was not drawn.
    let cam = camera.iter().next().map(|g| g.translation());
    let mut seen: HashSet<u64> = HashSet::new();
    for d in draws {
        seen.insert(d.key);
        let info = models.index.get(d.model).cloned();
        let plane = info.as_ref().and_then(|i| i.plane);
        let mut scale = Vec3::splat(OBJ_SCALE * d.scale);
        let rot = match (d.face, cam) {
            (Face::Full, Some(c)) => face_camera(plane, c - d.pos),
            (Face::Axis, Some(c)) => {
                // Facing the camera like a sprite, mirrored so the model's front (+Z) leans along the fighter's facing.
                let q = face_camera(plane, c - d.pos);
                if (q * Vec3::Z).dot(d.rot * Vec3::Z) < 0.0 {
                    scale.z = -scale.z;
                }
                q
            }
            _ => d.rot,
        };
        let tf = Transform { translation: d.pos, rotation: rot, scale };
        let vis = if d.visible { Visibility::Inherited } else { Visibility::Hidden };
        let existing = models.live.get(&d.key).filter(|(_, m)| m == d.model).map(|(e, _)| *e);
        if let Some(e) = existing.filter(|e| tfs.get(*e).is_ok()) {
            if let Ok((mut t0, mut v0)) = tfs.get_mut(e) {
                *t0 = tf;
                v0.set_if_neq(vis);
            }
            continue;
        }
        if let Some((old, _)) = models.live.remove(&d.key) {
            if let Ok(mut ec) = commands.get_entity(old) {
                ec.despawn();
            }
        }
        let Some(parts) = models.parts.get(d.model).cloned() else { continue };
        let e = commands
            .spawn((SmashObj, tf, vis, Name::new(format!("ssb64:{}", d.model))))
            .with_children(|c| {
                for (mesh, mat) in parts {
                    c.spawn((Mesh3d(mesh), MeshMaterial3d(mat), NotShadowCaster));
                }
            })
            .id();
        models.live.insert(d.key, (e, d.model.to_string()));
    }
    let stale: Vec<u64> = models.live.keys().filter(|k| !seen.contains(k)).copied().collect();
    for k in stale {
        if let Some((e, _)) = models.live.remove(&k) {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
    }
}

/// A hit flash at `pos` (from `smash_fx`'s hit events): the SSB64 slash spark when the import has it.
pub fn flash(models: &mut SmashModels, pos: [f64; 3], kind: HitKind) {
    if !models.has("hit_slash") || matches!(kind, HitKind::Sleep | HitKind::Capture | HitKind::Grab) {
        return;
    }
    models.flash_seq = models.flash_seq.wrapping_add(1);
    let key = (5 << 40) | (models.flash_seq & 0xFFFF_FFFF);
    if models.flashes.len() < 16 {
        models.flashes.push((key, "hit_slash", to_bevy(pos), 0.16, 0.16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_parses_objects_and_hands() {
        let text = r#"{"game":"ssb64","unit":0.004,"objects":{"fireball":{"kind":"weapon","plane":"x","billboard":true,"min":[0,-0.6,-0.6],"max":[0,0.6,0.6]},
            "crate":{"kind":"item","plane":null,"billboard":false,"min":[-0.9,-0.9,-0.9],"max":[0.9,0.9,0.9]}},"hands":{"mario":{"light":17,"heavy":28}}}"#;
        let (o, h) = parse_index(text).unwrap();
        assert_eq!(o["fireball"].plane, Some('x'));
        assert!(o["fireball"].billboard);
        assert_eq!(o["crate"].plane, None);
        assert_eq!(h["mario"], (17, 28));
    }

    #[test]
    fn every_projectile_kind_and_item_has_a_model_name() {
        let p = |kind, mv| Projectile {
            id: 1,
            kind,
            fighter: SmashFighter::Mario,
            mv,
            pos: [0.0; 3],
            vel: [0.0; 3],
            age: 0,
            life: 10,
            radius: 10.0,
            damage: 1.0,
            hit: HitKind::Normal,
            gravity: 0.0,
            terminal: 0.0,
            rehit: 0,
            pierce: false,
            level: 0,
            dead: false,
        };
        assert_eq!(projectile_model(&p(ProjKind::Fireball, SpecialMove::Fireball)), Some("fireball"));
        assert_eq!(projectile_model(&p(ProjKind::Laser, SpecialMove::Item)), Some("ray_gun_shot"));
        assert_eq!(projectile_model(&p(ProjKind::Item, SpecialMove::Item)), None);
        // The model names are the exporter's (scripts/export-ssb64-weapons.py OBJECTS).
        let py = include_str!("../../../scripts/export-ssb64-weapons.py");
        for k in ItemKind::ALL {
            assert!(py.contains(&format!("('{}', 'item'", k.model())), "{k:?}");
        }
        for kind in [
            ProjKind::Fireball,
            ProjKind::GreenFireball,
            ProjKind::ChargeShot,
            ProjKind::Boomerang,
            ProjKind::LinkBomb,
            ProjKind::SamusBomb,
            ProjKind::Egg,
            ProjKind::Star,
            ProjKind::Laser,
            ProjKind::Cutter,
            ProjKind::ThunderJolt,
            ProjKind::Thunderbolt,
            ProjKind::PkFireSpark,
            ProjKind::PkFirePillar,
            ProjKind::PkThunder,
        ] {
            let m = projectile_model(&p(kind, SpecialMove::Fireball)).unwrap();
            assert!(py.contains(&format!("('{m}', ")), "{m}");
        }
        for (mv, phase, total) in [(SpecialMove::FalconPunch, Phase::Attack, 25), (SpecialMove::Reflector, Phase::Hold, 3), (SpecialMove::Sing, Phase::Attack, 20)] {
            let (m, ..) = special_effect(mv, phase, total).unwrap();
            assert!(py.contains(&format!("('{m}', ")), "{m}");
        }
    }

    #[test]
    fn item_events_have_their_sounds() {
        let snd = include_str!("../../../scripts/export-ssb64-weapons.py");
        let e = [
            ItemEvent::PickedUp { kind: ItemKind::BeamSword },
            ItemEvent::Swing { kind: ItemKind::BeamSword, level: 4 },
            ItemEvent::Shot { kind: ItemKind::RayGun },
            ItemEvent::Exploded { kind: ItemKind::BobOmb, pos: [0.0; 3], radius: 1.0 },
            ItemEvent::Eaten { kind: ItemKind::MaximTomato },
            ItemEvent::Hit { kind: ItemKind::HomeRunBat, pos: [0.0; 3] },
            ItemEvent::Broke { kind: ItemKind::Crate, pos: [0.0; 3] },
            ItemEvent::Swing { kind: ItemKind::Hammer, level: 1 },
        ];
        for ev in e {
            let s = item_sounds(&ev);
            assert!(!s.is_empty(), "{ev:?}");
            for name in s {
                assert!(snd.contains(&format!("'{name}'")), "{name} is not exported");
            }
        }
    }

    #[test]
    fn camera_facing_turns_the_flat_side() {
        let to_cam = Vec3::new(0.0, 0.0, -5.0);
        let q = face_camera(Some('z'), to_cam);
        assert!((q * Vec3::Z).dot(to_cam.normalize()) > 0.99);
        let q = face_camera(Some('x'), Vec3::new(3.0, 0.0, 4.0));
        assert!((q * Vec3::X).dot(Vec3::new(0.6, 0.0, 0.8)) > 0.99);
    }
}
