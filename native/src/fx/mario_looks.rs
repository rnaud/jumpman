//! Mario's look variants (`Sm64MarioView.update`): `GEO_SWITCH_CASE` selection for the cap on/off,
//! eye blink and hand poses, metal / vanish / teleport-fade materials.
//!
//! The alternate display lists (open / peace-sign / cap-holding hands, cap-off head) live in
//! separate glbs (`actors/mario/variants/*`); like `Sm64MarioView.load` their switch-node child is
//! rebuilt from the glb's `GltfNode`s and parented into the empty slot of the main model.
use crate::game::runner::GameSim;
use crate::render::mario_view::MarioRoot;
use bevy::gltf::{Gltf, GltfExtras, GltfMesh, GltfNode};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::state::*;

pub struct MarioLooksPlugin;

impl Plugin for MarioLooksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MarioLooks>().add_observer(on_mario_ready).add_systems(Update, (install_variants, apply_switches, apply_materials).after(crate::CameraSet));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SwitchKind {
    Eyes,
    CapOnOff,
    /// `true` = right hand (arg 0).
    Hand(bool),
    /// Switches the TS leaves at their first case (stand/run, cap effect).
    Default,
}

struct BaseMaterial {
    entity: Entity,
    handle: Handle<StandardMaterial>,
    color: Color,
    metallic: f32,
    roughness: f32,
    alpha_mode: AlphaMode,
}

#[derive(Resource, Default)]
struct MarioLooks {
    switches: Vec<(Entity, SwitchKind)>,
    materials: Vec<BaseMaterial>,
    face_time: f32,
    applied: Option<(bool, bool, u32)>,
    /// Variant glbs still to splice into their slots.
    pending: Vec<Pending>,
    requested: bool,
}

struct Pending {
    gltf: Handle<Gltf>,
    callback: &'static str,
    /// Switch side argument ("0" right / cap, "1" left).
    side: &'static str,
    /// Child index in the switch node (and in the variant file's switch node).
    index: usize,
}

fn classify(extras: &str) -> Option<SwitchKind> {
    let json: serde_json::Value = serde_json::from_str(extras).ok()?;
    if json["sm64Command"] != "GEO_SWITCH_CASE" {
        return None;
    }
    let args = json["sm64Args"].as_array()?;
    Some(match args.get(1).and_then(|v| v.as_str())? {
        "geo_switch_mario_eyes" => SwitchKind::Eyes,
        "geo_switch_mario_cap_on_off" => SwitchKind::CapOnOff,
        "geo_switch_mario_hand" => SwitchKind::Hand(args[0].as_str() == Some("0")),
        _ => SwitchKind::Default,
    })
}

fn on_mario_ready(
    trigger: On<WorldInstanceReady>,
    mut commands: Commands,
    roots: Query<(), With<MarioRoot>>,
    children: Query<&Children>,
    extras: Query<&GltfExtras>,
    mats: Query<&MeshMaterial3d<StandardMaterial>>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    mut looks: ResMut<MarioLooks>,
) {
    if !roots.contains(trigger.event_target()) {
        return;
    }
    looks.switches.clear();
    looks.materials.clear();
    looks.applied = None;
    looks.requested = false;
    for e in children.iter_descendants(trigger.event_target()) {
        if let Ok(x) = extras.get(e) {
            if let Some(kind) = classify(&x.value) {
                looks.switches.push((e, kind));
            }
        }
        if let Ok(m) = mats.get(e) {
            // A private copy per mesh so cap effects never leak into shared assets.
            if let Some(base) = assets.get(&m.0).cloned() {
                let handle = assets.add(base.clone());
                commands.entity(e).insert(MeshMaterial3d(handle.clone()));
                looks.materials.push(BaseMaterial { entity: e, handle, color: base.base_color, metallic: base.metallic, roughness: base.perceptual_roughness, alpha_mode: base.alpha_mode });
            }
        }
    }
}

/// Eye case from the blink clock: frames 92/94 half closed, 93 closed (30 fps, 3.2 s loop).
pub fn eye_case(face_time: f32) -> usize {
    match (face_time * 30.0).floor() as i32 {
        92 | 94 => 1,
        93 => 2,
        _ => 0,
    }
}

/// Right/left hand case for the current action (cases: 0 fist, 1 swim/fly hand, 2 star-dance
/// hand, 3 cap in hand, 4 wing cap in hand).
pub fn hand_case(right: bool, action: u32, arg: f64, state: f64, timer: f64, flags: u32) -> usize {
    let mut hand = (action & ACT_FLAG_SWIMMING_OR_FLYING != 0) as usize;
    let cap_in_hand = flags & MARIO_CAP_IN_HAND != 0;
    if right && action == ACT_JUMBO_STAR_CUTSCENE && arg == 2.0 {
        hand = 1;
    }
    if right && action == ACT_END_PEACH_CUTSCENE && arg == 9.0 && timer >= 98.0 {
        hand = 2;
    }
    if right && state != 2.0 && (((action == ACT_STAR_DANCE_EXIT || action == ACT_STAR_DANCE_NO_EXIT) && timer >= 40.0) || (action == ACT_STAR_DANCE_WATER && timer >= 62.0)) {
        hand = 2;
    }
    if right && cap_in_hand {
        hand = if flags & MARIO_WING_CAP != 0 { 4 } else { 3 };
    }
    hand
}

fn show_only(children: &[Entity], selected: usize, vis: &mut Query<&mut Visibility>) {
    for (i, c) in children.iter().enumerate() {
        if let Ok(mut v) = vis.get_mut(*c) {
            let want = if i == selected { Visibility::Inherited } else { Visibility::Hidden };
            if *v != want {
                *v = want;
            }
        }
    }
}

fn apply_switches(sim: Res<GameSim>, time: Res<Time>, mut looks: ResMut<MarioLooks>, kids: Query<&Children>, mut vis: Query<&mut Visibility>) {
    looks.face_time = (looks.face_time + time.delta_secs()) % 3.2;
    let m = &sim.mario;
    let eye = eye_case(looks.face_time);
    let cap_in_hand = m.flags & MARIO_CAP_IN_HAND != 0;
    for &(switch, kind) in &looks.switches {
        let Ok(children) = kids.get(switch) else { continue };
        let children: Vec<Entity> = children.iter().collect();
        let selected = match kind {
            SwitchKind::Eyes => {
                if eye < children.len() {
                    eye
                } else {
                    0
                }
            }
            SwitchKind::CapOnOff => cap_in_hand as usize,
            SwitchKind::Hand(right) => {
                let want = std::env::var("JUMPMAN_HAND").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| hand_case(right, m.action, m.action_arg, m.action_state, m.action_timer, m.flags));
                // Variant slots are empty placeholders in the base glb: keep the default hand.
                if want < children.len() && kids.get(children[want]).is_ok() {
                    want
                } else {
                    0
                }
            }
            SwitchKind::Default => 0,
        };
        show_only(&children, selected, &mut vis);
    }
}

/// Teleport fade opacity (`ACT_TELEPORT_FADE_*`).
pub fn teleport_fade(action: u32, timer: f64) -> f32 {
    if action == ACT_TELEPORT_FADE_OUT {
        ((248.0 - 8.0 * timer).max(0.0) / 255.0) as f32
    } else if action == ACT_TELEPORT_FADE_IN {
        (timer * 8.0 / 255.0).min(1.0) as f32
    } else {
        1.0
    }
}

fn apply_materials(sim: Res<GameSim>, mut looks: ResMut<MarioLooks>, mut assets: ResMut<Assets<StandardMaterial>>) {
    let m = &sim.mario;
    let metal = m.flags & MARIO_METAL_CAP != 0;
    let vanish = m.flags & MARIO_VANISH_CAP != 0;
    let fade = teleport_fade(m.action, m.action_timer);
    let key = (metal, vanish, fade.to_bits());
    if looks.applied == Some(key) {
        return;
    }
    looks.applied = Some(key);
    for base in &looks.materials {
        let Some(mut mat) = assets.get_mut(&base.handle) else { continue };
        let _ = base.entity;
        mat.metallic = if metal { 0.95 } else { base.metallic };
        mat.perceptual_roughness = if metal { 0.2 } else { base.roughness };
        let tint = if metal { Color::srgb(0xc7 as f32 / 255.0, 0xd2 as f32 / 255.0, 0xd9 as f32 / 255.0) } else { base.color };
        let base_alpha = base.color.alpha();
        let alpha = fade * if vanish { 0.32 } else { base_alpha };
        mat.base_color = tint.with_alpha(alpha);
        mat.alpha_mode = if fade < 1.0 || vanish { AlphaMode::Blend } else { base.alpha_mode };
    }
}

/// Variant list of `Sm64MarioView.load`: (file, callback, side, slot index).
const VARIANTS: [(&str, &str, &str, usize); 6] = [
    ("face_and_wings/geo_switch_mario_cap_on_off_1", "geo_switch_mario_cap_on_off", "0", 1),
    ("left_hand/geo_switch_mario_hand_1", "geo_switch_mario_hand", "1", 1),
    ("right_hand/geo_switch_mario_hand_1", "geo_switch_mario_hand", "0", 1),
    ("right_hand/geo_switch_mario_hand_2", "geo_switch_mario_hand", "0", 2),
    ("right_hand/geo_switch_mario_hand_3", "geo_switch_mario_hand", "0", 3),
    ("right_hand/geo_switch_mario_hand_4", "geo_switch_mario_hand", "0", 4),
];

fn spawn_node(commands: &mut Commands, parent: Entity, node: &GltfNode, nodes: &Assets<GltfNode>, meshes: &Assets<GltfMesh>, looks: &mut MarioLooks, assets: &mut Assets<StandardMaterial>, gltf_mats: &Assets<bevy::gltf::GltfMaterial>) {
    let me = commands.spawn((node.transform, Visibility::Inherited)).id();
    commands.entity(parent).add_child(me);
    if let Some(mesh) = node.mesh.as_ref().and_then(|h| meshes.get(h)) {
        for prim in &mesh.primitives {
            let mut e = commands.spawn((Mesh3d(prim.mesh.clone()), Transform::default(), Visibility::Inherited));
            if let Some(base) = prim.material.as_ref().and_then(|h| gltf_mats.get(h)).map(bevy::pbr::gltf::standard_material_from_gltf_material) {
                let handle = assets.add(base.clone());
                e.insert(MeshMaterial3d(handle.clone()));
                looks.materials.push(BaseMaterial { entity: e.id(), handle, color: base.base_color, metallic: base.metallic, roughness: base.perceptual_roughness, alpha_mode: base.alpha_mode });
            }
            let id = e.id();
            commands.entity(me).add_child(id);
        }
    }
    for c in &node.children {
        if let Some(n) = nodes.get(c) {
            spawn_node(commands, me, n, nodes, meshes, looks, assets, gltf_mats);
        }
    }
}

fn args_of(node: &GltfNode) -> Option<Vec<String>> {
    let json: serde_json::Value = serde_json::from_str(&node.extras.as_ref()?.value).ok()?;
    Some(json["sm64Args"].as_array()?.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
}

#[allow(clippy::too_many_arguments)]
fn install_variants(
    mut commands: Commands,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes: Res<Assets<GltfMesh>>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    gltf_mats: Res<Assets<bevy::gltf::GltfMaterial>>,
    mut looks: ResMut<MarioLooks>,
    extras: Query<&GltfExtras>,
    kids: Query<&Children>,
) {
    if looks.switches.is_empty() {
        return;
    }
    if !looks.requested {
        looks.requested = true;
        looks.pending = VARIANTS
            .iter()
            .map(|&(file, callback, side, index)| Pending { gltf: server.load(format!("actors/mario/variants/mario_geo_{file}.glb")), callback, side, index })
            .collect();
    }
    let mut pending = std::mem::take(&mut looks.pending);
    pending.retain(|p| {
        let Some(gltf) = gltfs.get(&p.gltf) else { return !matches!(server.load_state(&p.gltf), bevy::asset::LoadState::Failed(_)) };
        if !gltf.nodes.iter().all(|h| nodes.get(h).is_some()) {
            return true;
        }
        // The fragment's switch node (matched by its callback) holds the display list at `index`.
        let Some(src) = gltf.nodes.iter().filter_map(|h| nodes.get(h)).find(|n| args_of(n).is_some_and(|a| a.get(1).map(String::as_str) == Some(p.callback))) else { return false };
        let Some(child) = src.children.get(p.index).and_then(|h| nodes.get(h)) else { return false };
        // Target: the model's switch with the same callback/side.
        let target = looks.switches.iter().map(|s| s.0).find(|&e| {
            extras
                .get(e)
                .ok()
                .and_then(|x| serde_json::from_str::<serde_json::Value>(&x.value).ok())
                .is_some_and(|j| j["sm64Args"][0].as_str() == Some(p.side) && j["sm64Args"][1].as_str() == Some(p.callback))
        });
        let Some(slot) = target.and_then(|t| kids.get(t).ok()).and_then(|c| c.get(p.index).copied()) else { return false };
        // `slot.clear(); slot.add(selected)`.
        if let Ok(old) = kids.get(slot) {
            for c in old.iter() {
                commands.entity(c).despawn();
            }
        }
        commands.entity(slot).insert(Transform::IDENTITY);
        spawn_node(&mut commands, slot, child, &nodes, &meshes, &mut looks, &mut assets, &gltf_mats);
        looks.applied = None;
        false
    });
    looks.pending = pending;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_frames() {
        assert_eq!(eye_case(0.0), 0);
        assert_eq!(eye_case(92.0 / 30.0 + 0.001), 1);
        assert_eq!(eye_case(93.0 / 30.0 + 0.001), 2);
        assert_eq!(eye_case(94.0 / 30.0 + 0.001), 1);
        assert_eq!(eye_case(95.0 / 30.0 + 0.001), 0);
    }

    #[test]
    fn hand_cases() {
        assert_eq!(hand_case(true, ACT_IDLE, 0.0, 0.0, 0.0, 0), 0);
        assert_eq!(hand_case(true, ACT_STAR_DANCE_EXIT, 0.0, 0.0, 40.0, 0), 2);
        assert_eq!(hand_case(false, ACT_STAR_DANCE_EXIT, 0.0, 0.0, 40.0, 0), 0);
        assert_eq!(hand_case(true, ACT_IDLE, 0.0, 0.0, 0.0, MARIO_CAP_IN_HAND), 3);
        assert_eq!(hand_case(true, ACT_IDLE, 0.0, 0.0, 0.0, MARIO_CAP_IN_HAND | MARIO_WING_CAP), 4);
        assert_eq!(hand_case(true, ACT_END_PEACH_CUTSCENE, 9.0, 0.0, 98.0, 0), 2);
    }

    #[test]
    fn fade_curve() {
        assert_eq!(teleport_fade(ACT_IDLE, 5.0), 1.0);
        assert!((teleport_fade(ACT_TELEPORT_FADE_OUT, 0.0) - 248.0 / 255.0).abs() < 1e-6);
        assert_eq!(teleport_fade(ACT_TELEPORT_FADE_OUT, 40.0), 0.0);
        assert_eq!(teleport_fade(ACT_TELEPORT_FADE_IN, 100.0), 1.0);
    }

    #[test]
    fn classify_switches() {
        let eyes = r#"{"sm64Command":"GEO_SWITCH_CASE","sm64Args":["0","geo_switch_mario_eyes"]}"#;
        assert_eq!(classify(eyes), Some(SwitchKind::Eyes));
        let hand = r#"{"sm64Command":"GEO_SWITCH_CASE","sm64Args":["1","geo_switch_mario_hand"]}"#;
        assert_eq!(classify(hand), Some(SwitchKind::Hand(false)));
        assert_eq!(classify(r#"{"sm64Command":"GEO_BRANCH"}"#), None);
    }
}
