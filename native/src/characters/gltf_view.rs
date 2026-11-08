//! `GltfCharacterView.ts` + `clipMap.ts`: a glTF character driven by SM64 action ids. The bundled
//! CC0 rig is Quaternius' Universal Animation Library (mannequin + 46 clips,
//! `public/models/quaternius/AnimationLibrary.glb`); a "bring your own rig" lives in
//! `public/models/local/character.glb` (`JUMPMAN_RIG=local`). Unlike the Nintendo rigs this is
//! ordinary skinned glTF, so Bevy's own skinning/animation player does the work.
//!
//! Toggle with `G` (or start with `JUMPMAN_RIG=quaternius|local`); it replaces Mario's model while
//! the character is Mario. The pure parts (`ClipAliases`, `resolve_clip_name`, `choose_clip`,
//! `visual_adjust`) are unit tested.
use crate::game::runner::GameSim;
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::math::to_s16;
use jumpman_sim::state::*;
use std::collections::HashMap;
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// clipMap.ts
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ClipKey {
    Idle,
    Walk,
    Jog,
    Sprint,
    JumpStart,
    JumpLoop,
    JumpLand,
    Roll,
    RollRm,
    CrouchIdle,
    CrouchWalk,
    PunchJab,
    PunchCross,
    Interact,
}

pub fn default_alias(key: ClipKey) -> &'static str {
    match key {
        ClipKey::Idle => "Idle_Loop",
        ClipKey::Walk => "Walk_Loop",
        ClipKey::Jog => "Jog_Fwd_Loop",
        ClipKey::Sprint => "Sprint_Loop",
        ClipKey::JumpStart => "Jump_Start",
        ClipKey::JumpLoop => "Jump_Loop",
        ClipKey::JumpLand => "Jump_Land",
        ClipKey::Roll => "Roll",
        ClipKey::RollRm => "Roll_RM",
        ClipKey::CrouchIdle => "Crouch_Idle_Loop",
        ClipKey::CrouchWalk => "Crouch_Fwd_Loop",
        ClipKey::PunchJab => "Punch_Jab",
        ClipKey::PunchCross => "Punch_Cross",
        ClipKey::Interact => "Interact",
    }
}

fn fallbacks(key: ClipKey) -> &'static [ClipKey] {
    use ClipKey::*;
    match key {
        Walk => &[Jog, Idle],
        Jog => &[Walk, Sprint, Idle],
        Sprint => &[Jog, Walk, Idle],
        JumpStart => &[JumpLoop, Idle],
        JumpLoop => &[JumpStart, Idle],
        JumpLand => &[Idle, JumpLoop],
        Roll => &[RollRm, JumpLoop, Idle],
        RollRm => &[Roll, JumpLoop, Idle],
        CrouchIdle => &[CrouchWalk, Idle],
        CrouchWalk => &[CrouchIdle, Walk, Idle],
        PunchJab => &[PunchCross, Idle],
        PunchCross => &[PunchJab, Idle],
        Interact => &[Idle, JumpLoop],
        Idle => &[Idle],
    }
}

/// Custom clip names for a bring-your-own rig (missing keys use the defaults).
#[derive(Clone, Default, Debug)]
pub struct ClipAliases(pub HashMap<ClipKey, String>);

/// `resolveClipName`: custom alias, default alias, semantic fallbacks, then the first clip.
pub fn resolve_clip_name<'a>(clips: &'a [String], aliases: &ClipAliases, key: ClipKey) -> Option<&'a str> {
    let mut candidates: Vec<String> = vec![aliases.0.get(&key).cloned().unwrap_or_default(), default_alias(key).to_string()];
    for f in fallbacks(key) {
        candidates.push(aliases.0.get(f).cloned().unwrap_or_default());
        candidates.push(default_alias(*f).to_string());
    }
    candidates.extend(clips.iter().cloned());
    candidates.into_iter().filter(|c| !c.is_empty()).find_map(|c| clips.iter().find(|n| **n == c).map(String::as_str))
}

// ---------------------------------------------------------------------------------------------
// GltfCharacterView.ts: action -> clip and pose adjustments
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ClipChoice {
    pub key: ClipKey,
    pub looped: bool,
    pub time_scale: f64,
    pub fade: f64,
}

fn lp(key: ClipKey, time_scale: f64) -> ClipChoice {
    ClipChoice { key, looped: true, time_scale, fade: 0.14 }
}
fn once(key: ClipKey, time_scale: f64) -> ClipChoice {
    ClipChoice { key, looped: false, time_scale, fade: 0.1 }
}

const IDLE_ACTIONS: [u32; 11] = [
    ACT_IDLE,
    ACT_BRAKING_STOP,
    ACT_BUTT_SLIDE_STOP,
    ACT_SLIDE_KICK_SLIDE_STOP,
    ACT_JUMP_LAND_STOP,
    ACT_DOUBLE_JUMP_LAND_STOP,
    ACT_TRIPLE_JUMP_LAND_STOP,
    ACT_FREEFALL_LAND_STOP,
    ACT_SIDE_FLIP_LAND_STOP,
    ACT_LONG_JUMP_LAND_STOP,
    ACT_BACKFLIP_LAND_STOP,
];
const LANDING_ACTIONS: [u32; 7] = [ACT_JUMP_LAND, ACT_DOUBLE_JUMP_LAND, ACT_TRIPLE_JUMP_LAND, ACT_FREEFALL_LAND, ACT_SIDE_FLIP_LAND, ACT_LONG_JUMP_LAND, ACT_BACKFLIP_LAND];
const FLIP_ACTIONS: [u32; 6] = [ACT_DOUBLE_JUMP, ACT_TRIPLE_JUMP, ACT_BACKFLIP, ACT_SIDE_FLIP, ACT_JUMP_KICK, ACT_SLIDE_KICK];

/// `chooseClip`. `action_time` is seconds in the current action.
pub fn choose_clip(action: u32, action_arg: f64, forward_vel: f64, action_time: f64) -> ClipChoice {
    use ClipKey::*;
    let speed = forward_vel.abs();
    let a = action;
    if IDLE_ACTIONS.contains(&a) {
        return lp(Idle, 1.0);
    }
    if LANDING_ACTIONS.contains(&a) {
        return once(JumpLand, 1.0);
    }
    if a == ACT_WALKING {
        if speed > 27.0 {
            return lp(Sprint, (speed / 32.0).clamp(0.8, 1.5));
        }
        if speed > 10.0 {
            return lp(Jog, (speed / 22.0).clamp(0.7, 1.35));
        }
        return lp(Walk, (speed / 12.0).clamp(0.65, 1.1));
    }
    if a == ACT_DECELERATING {
        return lp(if speed > 10.0 { Jog } else { Walk }, 0.7);
    }
    if a == ACT_TURNING_AROUND {
        return lp(Walk, 0.65);
    }
    if a == ACT_BRAKING {
        return lp(Walk, 0.35);
    }
    if [ACT_CROUCHING, ACT_START_CROUCHING, ACT_STOP_CROUCHING].contains(&a) {
        return lp(CrouchIdle, 1.0);
    }
    if [ACT_CRAWLING, ACT_START_CRAWLING, ACT_STOP_CRAWLING].contains(&a) {
        return lp(CrouchWalk, 0.8);
    }
    if a == ACT_CROUCH_SLIDE {
        return lp(CrouchWalk, 1.25);
    }
    if [ACT_JUMP, ACT_STEEP_JUMP, ACT_WALL_KICK_AIR].contains(&a) {
        return if action_time < 0.28 { once(JumpStart, 1.25) } else { lp(JumpLoop, 1.0) };
    }
    if [ACT_FREEFALL, ACT_SOFT_BONK, ACT_AIR_HIT_WALL].contains(&a) {
        return lp(JumpLoop, 0.95);
    }
    if a == ACT_DOUBLE_JUMP {
        return once(Roll, 1.25);
    }
    if a == ACT_TRIPLE_JUMP || a == ACT_BACKFLIP {
        return once(Roll, 1.45);
    }
    if a == ACT_SIDE_FLIP {
        return once(RollRm, 1.4);
    }
    if a == ACT_LONG_JUMP {
        return lp(JumpLoop, 1.05);
    }
    if a == ACT_DIVE {
        return lp(JumpLoop, 0.85);
    }
    if FLIP_ACTIONS.contains(&a) {
        return once(Roll, 1.15);
    }
    if [ACT_BUTT_SLIDE, ACT_BUTT_SLIDE_AIR, ACT_BUTT_SLIDE_STOP, ACT_STOMACH_SLIDE, ACT_DIVE_SLIDE, ACT_SLIDE_KICK_SLIDE].contains(&a) {
        return lp(Roll, 1.2);
    }
    if a == ACT_SLIDE_KICK_SLIDE_STOP {
        return once(CrouchIdle, 1.0);
    }
    if a == ACT_GROUND_POUND {
        return once(Roll, 1.35);
    }
    if a == ACT_GROUND_POUND_LAND {
        return once(JumpLand, 1.0);
    }
    if a == ACT_PUNCHING || a == ACT_MOVE_PUNCHING {
        return if action_arg == 1.0 || action_arg == 4.0 { once(PunchCross, 1.4) } else { once(PunchJab, 1.4) };
    }
    if a == ACT_LEDGE_GRAB || a == ACT_LEDGE_CLIMB_FAST {
        return once(Interact, 1.0);
    }
    lp(Idle, 1.0)
}

/// Per-action visual adjustment on top of the clip: (tilt_x, tilt_z, spin_y, scale xyz).
pub fn visual_adjust(action: u32, squish: bool, action_time: f64) -> (f64, f64, f64, [f64; 3]) {
    let (mut tx, tz, mut spin, mut s) = (0.0, 0.0, 0.0, [1.0, 1.0, 1.0]);
    match action {
        ACT_DIVE => tx = 1.1,
        ACT_LONG_JUMP => tx = 0.38,
        ACT_STOMACH_SLIDE | ACT_DIVE_SLIDE => tx = 1.18,
        ACT_BUTT_SLIDE | ACT_BUTT_SLIDE_AIR | ACT_BUTT_SLIDE_STOP => tx = -0.38,
        ACT_SLIDE_KICK_SLIDE => tx = -0.28,
        ACT_GROUND_POUND => {
            spin = action_time * 12.0;
            s[1] = 0.96;
        }
        ACT_GROUND_POUND_LAND => s = [1.12, 0.78, 1.12],
        ACT_BRAKING => tx = -0.2,
        _ => {}
    }
    if squish {
        s = [1.22, 0.62, 1.22];
    }
    (tx, tz, spin, s)
}

// ---------------------------------------------------------------------------------------------
// Bevy glue
// ---------------------------------------------------------------------------------------------

const BUNDLED: &str = "../public/models/quaternius/AnimationLibrary.glb";
const LOCAL: &str = "../public/models/local/character.glb";
/// Mario's height in SM64 units; the rig is scaled to it.
const HEIGHT: f64 = 160.0;

#[derive(Resource, Default)]
pub struct GltfRig {
    pub on: bool,
    /// Which file to use (`quaternius` or `local`).
    pub local: bool,
    started: bool,
}

#[derive(Component)]
struct RigRoot;
#[derive(Component)]
struct RigVisual;

#[derive(Resource)]
struct RigClips {
    names: Vec<String>,
    nodes: Vec<AnimationNodeIndex>,
    graph: Handle<AnimationGraph>,
    aliases: ClipAliases,
    /// glb path relative to the asset root.
    path: String,
}

#[derive(Component, Default)]
struct RigState {
    current: Option<String>,
    action: u32,
    action_time: f64,
    tilt: [f64; 4],
    spin: f64,
    scale: [f64; 3],
}

pub struct GltfRigPlugin;

impl Plugin for GltfRigPlugin {
    fn build(&self, app: &mut App) {
        let env = std::env::var("JUMPMAN_RIG").ok();
        app.insert_resource(GltfRig { on: env.is_some(), local: env.as_deref() == Some("local"), started: false })
            .add_systems(Update, (toggle_rig, spawn_rig, drive_rig).chain().before(crate::CameraSet));
    }
}

fn toggle_rig(slot: Res<crate::experiments::ExperimentSlot>, keys: Res<ButtonInput<KeyCode>>, mut rig: ResMut<GltfRig>, mut toast: ResMut<crate::hud::Toast>) {
    if slot.id == Some(crate::experiments::ExperimentId::Kart) { rig.on = false; return; }
    if keys.just_pressed(KeyCode::KeyG) {
        rig.on = !rig.on;
        toast.show(if rig.on { "GLTF MANNEQUIN RIG" } else { "SM64 MARIO MODEL" }, 1.6);
    }
}

fn spawn_rig(mut commands: Commands, assets: Res<AssetServer>, root: Res<AssetRoot>, mut graphs: ResMut<Assets<AnimationGraph>>, mut rig: ResMut<GltfRig>, mut toast: ResMut<crate::hud::Toast>) {
    if !rig.on || rig.started {
        return;
    }
    rig.started = true;
    let path = if rig.local { LOCAL } else { BUNDLED };
    let abs = root.0.join(path);
    if !abs.is_file() {
        warn!("glTF rig missing: {}", abs.display());
        toast.show("NO GLTF RIG LOADED", 3.0);
        rig.on = false;
        return;
    }
    let names = crate::zelda::glb::animation_names(&abs);
    let handles: Vec<Handle<AnimationClip>> = (0..names.len()).map(|i| assets.load_builder().override_unapproved().load(GltfAssetLabel::Animation(i).from_asset(path.to_owned()))).collect();
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    commands.insert_resource(RigClips { names, nodes, graph: graphs.add(graph), aliases: ClipAliases::default(), path: path.to_owned() });
    commands
        .spawn((RigRoot, Transform::default(), Visibility::Hidden, RigState { scale: [1.0; 3], ..default() }))
        .with_children(|p| {
            p.spawn((RigVisual, WorldAssetRoot(assets.load_builder().override_unapproved().load(GltfAssetLabel::Scene(0).from_asset(path.to_owned()))), Transform::default()));
        })
        .observe(prepare_rig);
}

/// Scale the mannequin to Mario's height, recolour the two materials, bind the clip graph.
fn prepare_rig(
    trigger: On<WorldInstanceReady>,
    mut commands: Commands,
    clips: Res<RigClips>,
    children: Query<&Children>,
    players: Query<(), With<AnimationPlayer>>,
    names: Query<(&bevy::gltf::GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    visual: Query<Entity, With<RigVisual>>,
) {
    for child in children.iter_descendants(trigger.event_target()) {
        if players.contains(child) {
            commands.entity(child).insert((AnimationGraphHandle(clips.graph.clone()), AnimationTransitions::new()));
        }
        if let Ok((name, handle)) = names.get(child) {
            if let Some(src) = materials.get(&handle.0).cloned() {
                let mut m = src;
                match name.0.as_str() {
                    "M_Main" => m.base_color = Color::srgb_u8(0xd9, 0x40, 0x32),
                    "M_Joints" => m.base_color = Color::srgb_u8(0x31, 0x5b, 0xd6),
                    _ => {}
                }
                m.perceptual_roughness = 0.72;
                m.metallic = 0.02;
                commands.entity(child).insert(MeshMaterial3d(materials.add(m)));
                commands.entity(child).insert(bevy::camera::visibility::NoFrustumCulling);
            }
        }
    }
    // Mannequin: ~1.83 m tall, feet at y = 0 -> Mario's 160 SM64 units.
    let scale = (HEIGHT * SCALE as f64 / 1.829) as f32;
    for v in &visual {
        commands.entity(v).insert(Transform::from_scale(Vec3::splat(scale)));
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_rig(
    rig: Res<GltfRig>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    clips: Option<Res<RigClips>>,
    mut root: Query<(&mut Transform, &mut Visibility, &mut RigState), With<RigRoot>>,
    mut visual: Query<&mut Transform, (With<RigVisual>, Without<RigRoot>)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    character: Res<crate::characters::Character>,
) {
    let Ok((mut tf, mut vis, mut st)) = root.single_mut() else { return };
    let show = rig.on && *character == crate::characters::Character::Mario;
    *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
    if !show {
        return;
    }
    let m = &sim.mario;
    let t = fixed.overstep_fraction() as f64;
    let (pos, yaw) = sim.mario_at(t);
    let dt = time.delta_secs_f64().min(0.1);
    tf.translation = to_bevy(pos);
    tf.rotation = Quat::from_rotation_y((to_s16(yaw) as f64 * std::f64::consts::PI / 32768.0) as f32);
    if m.action != st.action {
        st.action = m.action;
        st.action_time = 0.0;
    } else {
        st.action_time += dt;
    }
    let (tx, tz, spin, s) = visual_adjust(m.action, m.squish_timer > 0.0, st.action_time);
    let damp = 1.0 - (-16.0 * dt).exp();
    let lerp = |a: f64, b: f64| a + (b - a) * damp;
    st.tilt = [lerp(st.tilt[0], tx), lerp(st.tilt[1], tz), 0.0, 0.0];
    st.spin = lerp(st.spin, spin);
    for i in 0..3 {
        st.scale[i] = lerp(st.scale[i], s[i]);
    }
    // The visual wrapper: tilt/spin/squash on top of the base height scale.
    if let Ok(mut v) = visual.single_mut() {
        let base = (HEIGHT * SCALE as f64 / 1.829) as f32;
        v.rotation = Quat::from_euler(EulerRot::YXZ, st.spin as f32, st.tilt[0] as f32, st.tilt[1] as f32);
        v.scale = Vec3::new(base * st.scale[0] as f32, base * st.scale[1] as f32, base * st.scale[2] as f32);
    }
    let Some(clips) = clips else { return };
    let choice = choose_clip(m.action, m.action_arg, m.forward_vel, st.action_time);
    let Some(name) = resolve_clip_name(&clips.names, &clips.aliases, choice.key) else { return };
    let Some(index) = clips.names.iter().position(|n| n == name) else { return };
    for (mut player, mut transitions) in &mut players {
        if st.current.as_deref() != Some(name) {
            let active = transitions.play(&mut player, clips.nodes[index], Duration::from_secs_f64(choice.fade));
            active.set_speed(choice.time_scale as f32);
            if choice.looped {
                active.repeat();
            } else {
                active.set_repeat(bevy::animation::RepeatAnimation::Never);
            }
        } else if let Some(a) = player.animation_mut(clips.nodes[index]) {
            a.set_speed(choice.time_scale as f32);
        }
    }
    st.current = Some(name.to_string());
}

impl GltfRig {
    /// Mario's own model is hidden while another character (or this rig) is drawn.
    pub fn replaces_mario(&self, character: crate::characters::Character) -> bool {
        character != crate::characters::Character::Mario || self.on
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clips() -> Vec<String> {
        ["Idle_Loop", "Walk_Loop", "Jog_Fwd_Loop", "Jump_Loop", "Roll", "Punch_Jab"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clip_names_resolve_with_fallbacks() {
        let c = clips();
        let a = ClipAliases::default();
        assert_eq!(resolve_clip_name(&c, &a, ClipKey::Idle), Some("Idle_Loop"));
        assert_eq!(resolve_clip_name(&c, &a, ClipKey::Sprint), Some("Jog_Fwd_Loop"), "sprint falls back to jog");
        assert_eq!(resolve_clip_name(&c, &a, ClipKey::CrouchWalk), Some("Walk_Loop"));
        assert_eq!(resolve_clip_name(&c, &a, ClipKey::PunchCross), Some("Punch_Jab"));
        assert_eq!(resolve_clip_name(&c, &a, ClipKey::JumpLand), Some("Idle_Loop"));
        let mut custom = ClipAliases::default();
        custom.0.insert(ClipKey::Idle, "Roll".into());
        assert_eq!(resolve_clip_name(&c, &custom, ClipKey::Idle), Some("Roll"));
        assert_eq!(resolve_clip_name(&["Weird".to_string()], &a, ClipKey::Walk), Some("Weird"), "first clip as a last resort");
        assert_eq!(resolve_clip_name(&[], &a, ClipKey::Walk), None);
    }

    #[test]
    fn walking_speed_picks_the_gait() {
        assert_eq!(choose_clip(ACT_WALKING, 0.0, 5.0, 0.0).key, ClipKey::Walk);
        assert_eq!(choose_clip(ACT_WALKING, 0.0, 16.0, 0.0).key, ClipKey::Jog);
        let sprint = choose_clip(ACT_WALKING, 0.0, 32.0, 0.0);
        assert_eq!((sprint.key, sprint.time_scale), (ClipKey::Sprint, 1.0));
        assert_eq!(choose_clip(ACT_IDLE, 0.0, 0.0, 0.0), lp(ClipKey::Idle, 1.0));
    }

    #[test]
    fn jumps_start_then_loop_and_punches_alternate() {
        assert_eq!(choose_clip(ACT_JUMP, 0.0, 0.0, 0.1).key, ClipKey::JumpStart);
        assert_eq!(choose_clip(ACT_JUMP, 0.0, 0.0, 0.5).key, ClipKey::JumpLoop);
        assert_eq!(choose_clip(ACT_PUNCHING, 1.0, 0.0, 0.0).key, ClipKey::PunchCross);
        assert_eq!(choose_clip(ACT_PUNCHING, 2.0, 0.0, 0.0).key, ClipKey::PunchJab);
        assert!(!choose_clip(ACT_DOUBLE_JUMP, 0.0, 0.0, 0.0).looped);
    }

    #[test]
    fn visual_adjustments() {
        assert_eq!(visual_adjust(ACT_DIVE, false, 0.0).0, 1.1);
        let (_, _, spin, s) = visual_adjust(ACT_GROUND_POUND, false, 0.5);
        assert_eq!((spin, s[1]), (6.0, 0.96));
        assert_eq!(visual_adjust(ACT_IDLE, true, 0.0).3, [1.22, 0.62, 1.22]);
    }
}
