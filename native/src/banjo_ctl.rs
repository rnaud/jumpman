//! Native side of Banjo-Kazooie's extended controller (`jumpman_sim::banjo_ext`): input mapping
//! (keyboard, D-pad, touch buttons), the item HUD, egg projectiles, collectible pickups and their
//! particle effects. Everything here is inert unless the active character is Banjo.
//!
//! Default mapping (see `native/BANJO_CONTROLLER.md`):
//!
//! | N64 | keyboard | gamepad | touch |
//! |-----|----------|---------|-------|
//! | A / B / Z | Space / J,Shift / K,Ctrl | south / east / west,north,L2,R2 | A / B / Z |
//! | C-up (egg) | F | D-pad up | EGG |
//! | C-down (egg backwards) | G | D-pad down | BACK |
//! | C-left (Talon Trot) | T | D-pad left | TROT |
//! | C-right (Wonderwing) | Y | D-pad right | WING |
//! | R (equip next pad, tight flight turn) | U | R1 | POWER |
use crate::characters::Character;
use crate::fx::particles::{BurstOptions, Particles};
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::render::{to_bevy, SCALE};
use crate::safe_area::SafeArea;
use crate::shell::AppState;
use crate::touch::{TouchControls, BUTTON_GAP, BUTTON_SIZE, MARGIN_X, MARGIN_Y};
use bevy::prelude::*;
use crate::button_marker::BorderRadius;
use bevy::window::PrimaryWindow;
use jumpman_sim::banjo_ext::{BanjoEvent, EggKind, PickupKind, Power};
use jumpman_sim::state::{C_DOWN, C_LEFT, C_RIGHT, C_UP, R_TRIG};

pub struct BanjoCtlPlugin;

impl Plugin for BanjoCtlPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hud, spawn_touch_buttons))
            .add_systems(PreUpdate, banjo_input.after(crate::touch::drive_touch))
            .add_systems(Update, (scatter_pickups, sync_water_kind, sync_note_doors, sync_markers, egg_vs_goombas, drain_events, sync_eggs, sync_pickups, update_hud, update_touch_buttons).after(crate::CameraSet));
    }
}

/// Size of the BK buttons (a little smaller than A/B).
pub const BANJO_BUTTON: f32 = 44.0;
/// Touch button ids: bit `i` of `TouchControls::banjo_down`.
pub const TOUCH_BUTTONS: [(&str, u32); 5] = [("EGG", C_UP), ("BACK", C_DOWN), ("TROT", C_LEFT), ("WING", C_RIGHT), ("POWER", R_TRIG)];

/// Where the BK buttons sit: two rows to the left of the A/B grid.
pub fn layout(size: Vec2, inset: Vec3) -> Vec<(u8, Rect)> {
    let right = MARGIN_X.max(inset.y);
    let bottom = MARGIN_Y.max(inset.z);
    let grid = 2.0 * BUTTON_SIZE + BUTTON_GAP;
    let grid_left = size.x - right - grid;
    let rect = |slot: usize, row: usize| {
        let x1 = grid_left - 10.0 - slot as f32 * (BANJO_BUTTON + BUTTON_GAP);
        let y1 = size.y - bottom - row as f32 * (BANJO_BUTTON + BUTTON_GAP);
        Rect::from_corners(Vec2::new(x1 - BANJO_BUTTON, y1 - BANJO_BUTTON), Vec2::new(x1, y1))
    };
    vec![(0, rect(0, 0)), (2, rect(1, 0)), (3, rect(2, 0)), (1, rect(0, 1)), (4, rect(1, 1))]
}

fn is_banjo(current: &Character) -> bool {
    *current == Character::Banjo
}

/// Keyboard, D-pad and touch buttons into the C-button bits of `PlayerInput`.
fn banjo_input(
    current: Res<Character>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut tc: ResMut<TouchControls>,
    mut input: ResMut<PlayerInput>,
    state: Option<Res<AppState>>,
    time: Res<Time>,
    mut script: Local<Option<Vec<ScriptStep>>>,
) {
    let playing = state.is_none_or(|s| *s == AppState::Playing);
    tc.banjo_active = is_banjo(&current) && playing;
    if !tc.banjo_active {
        input.extra_down = 0;
        tc.banjo_pressed = 0;
        return;
    }
    let mut down = 0u32;
    let mut edge = 0u32;
    let table = [
        (KeyCode::KeyF, GamepadButton::DPadUp, C_UP),
        (KeyCode::KeyG, GamepadButton::DPadDown, C_DOWN),
        (KeyCode::KeyT, GamepadButton::DPadLeft, C_LEFT),
        (KeyCode::KeyY, GamepadButton::DPadRight, C_RIGHT),
        (KeyCode::KeyU, GamepadButton::RightTrigger, R_TRIG),
    ];
    let pad = pads.iter().next();
    for (key, button, mask) in table {
        if keys.pressed(key) || pad.is_some_and(|p| p.pressed(button)) {
            down |= mask;
        }
        if keys.just_pressed(key) || pad.is_some_and(|p| p.just_pressed(button)) {
            edge |= mask;
        }
    }
    for (i, (_, mask)) in TOUCH_BUTTONS.iter().enumerate() {
        if tc.banjo_down & (1 << i) != 0 {
            down |= mask;
        }
    }
    let touched = std::mem::take(&mut tc.banjo_pressed);
    for (i, (_, mask)) in TOUCH_BUTTONS.iter().enumerate() {
        if touched & (1 << i) != 0 {
            edge |= mask;
        }
    }
    // Test hook: JUMPMAN_BANJO_SCRIPT (see `parse_script`).
    let steps = script.get_or_insert_with(|| std::env::var("JUMPMAN_BANJO_SCRIPT").map(|s| parse_script(&s)).unwrap_or_default());
    let now = time.elapsed_secs();
    for step in steps.iter_mut() {
        if now < step.at || now > step.at + step.hold.max(0.05) {
            continue;
        }
        let first = !step.fired;
        step.fired = true;
        for b in &step.buttons {
            match b.as_str() {
                "a" => { input.down |= jumpman_sim::state::A_BUTTON; if first { input.latch(jumpman_sim::state::A_BUTTON); } }
                "b" => { input.down |= jumpman_sim::state::B_BUTTON; if first { input.latch(jumpman_sim::state::B_BUTTON); } }
                "z" => { input.down |= jumpman_sim::state::Z_TRIG; if first { input.latch(jumpman_sim::state::Z_TRIG); } }
                "stick" => { input.stick = Vec2::new(0.0, 64.0); input.stick_mag = 64.0; }
                other => {
                    let mask = match other { "up" => C_UP, "down" => C_DOWN, "left" => C_LEFT, "right" => C_RIGHT, "r" => R_TRIG, _ => 0 };
                    down |= mask;
                    if first { edge |= mask; }
                }
            }
        }
    }
    input.extra_down = down;
    input.latch_extra(edge);
}

/// One scripted press: `at` seconds after start, `buttons` held for `hold` seconds.
pub struct ScriptStep {
    at: f32,
    hold: f32,
    buttons: Vec<String>,
    fired: bool,
}

/// `JUMPMAN_BANJO_SCRIPT="2:z+left@3,2:stick@3,6:a"`: at 2 s press Z + C-left and hold both for 3 s while the
/// stick is held forward, at 6 s tap A. Buttons: a b z up down left right r stick (up..right are the C buttons).
pub fn parse_script(text: &str) -> Vec<ScriptStep> {
    text.split(',')
        .filter_map(|item| {
            let (at, rest) = item.split_once(':')?;
            let (buttons, hold) = rest.split_once('@').unwrap_or((rest, "0.1"));
            Some(ScriptStep { at: at.trim().parse().ok()?, hold: hold.trim().parse().ok()?, buttons: buttons.split('+').map(|b| b.trim().to_string()).collect(), fired: false })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Pickups
// ---------------------------------------------------------------------------------------------

/// Scatter a sandbox set of BK collectibles around the spawn of every course Banjo enters (the SM64
/// courses have no notes, eggs or feathers of their own).
fn scatter_pickups(current: Res<Character>, mut sim: ResMut<GameSim>, mut done_for: Local<String>) {
    if !is_banjo(&current) {
        return;
    }
    if *done_for == sim.course.name {
        return;
    }
    *done_for = sim.course.name.clone();
    if !sim.mario.banjo.pickups.is_empty() {
        return;
    }
    let origin = sim.mario.pos;
    let world = sim.mario.world.clone();
    let kinds = [
        PickupKind::Note,
        PickupKind::Egg,
        PickupKind::Note,
        PickupKind::RedFeather,
        PickupKind::Note,
        PickupKind::GoldFeather,
        PickupKind::Note,
        PickupKind::Honeycomb,
        PickupKind::Note,
        PickupKind::Egg,
        PickupKind::Jiggy,
        PickupKind::RedFeather,
    ];
    for (i, kind) in kinds.iter().enumerate() {
        let ang = i as f64 / kinds.len() as f64 * std::f64::consts::TAU;
        let r = 260.0 + 70.0 * (i % 3) as f64;
        let (x, z) = (origin[0] + ang.sin() * r, origin[2] + ang.cos() * r);
        let floor = world.find_floor(x, origin[1] + 200.0, z);
        if floor.surface.is_none() || (floor.height - origin[1]).abs() > 300.0 {
            continue;
        }
        sim.mario.banjo.drop_pickup(*kind, [x, floor.height + 40.0, z]);
    }
}

/// The walking goombas live outside the sim's object arena (`objects::goombas`): an egg that reaches
/// one defeats it like a punch would (`WalkingGoomba::defeat`) and shatters.
fn egg_vs_goombas(current: Res<Character>, mut sim: ResMut<GameSim>, mut goombas: ResMut<crate::objects::goombas::Goombas>) {
    if !is_banjo(&current) || sim.mario.banjo.eggs_in_flight.is_empty() {
        return;
    }
    let mut shattered = vec![];
    sim.mario.banjo.eggs_in_flight.retain(|egg| {
        if egg.kind != EggKind::Head {
            return true;
        }
        for g in goombas.0.iter_mut() {
            if !g.tangible() {
                continue;
            }
            let (dx, dz) = (egg.pos[0] - g.pos.x, egg.pos[2] - g.pos.z);
            if dx * dx + dz * dz < (crate::game::goomba::RADIUS * crate::game::goomba::RADIUS) && egg.pos[1] > g.pos.y - 20.0 && egg.pos[1] < g.top() + 20.0 {
                g.defeat(false);
                shattered.push(egg.pos);
                return false;
            }
        }
        true
    });
    for pos in shattered {
        sim.mario.banjo.events.push(BanjoEvent::EggShattered { pos, hit: true });
    }
}

#[derive(Component)]
struct PickupView(PickupKind);
#[derive(Component)]
struct EggView;

/// Tell the air meter whether the course's water is polluted (RBB, CCW winter).
fn sync_water_kind(mut sim: ResMut<GameSim>) {
    let polluted = jumpman_sim::banjo_ext::polluted_course(&sim.course.request.id);
    if sim.mario.banjo.polluted_water != polluted {
        sim.mario.banjo.polluted_water = polluted;
    }
}

#[derive(Component)]
struct NoteDoorView(u32);

#[derive(Component)]
struct MapMarker;

/// Markers for the setup actors that have no model here: Bottles' molehills (a brown mound, grey once the lesson is
/// learned) and the climbable poles (a thin wooden post from the base to the top node).
fn sync_markers(
    mut commands: Commands,
    current: Res<Character>,
    sim: Res<GameSim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    views: Query<(Entity, &MapMarker)>,
    mut shown: Local<(usize, usize, u32)>,
) {
    use jumpman_sim::banjo_ext::mole_abilities;
    let b = &sim.mario.banjo;
    let on = is_banjo(&current);
    let learned: u32 = b.moles.iter().filter(|m| mole_abilities(m.sm, m.id).iter().all(|a| b.has(*a))).count() as u32;
    let key = (if on { b.moles.len() } else { 0 }, if on { b.climbs.len() } else { 0 }, learned);
    if key == *shown && views.iter().count() == key.0 + key.1 {
        return;
    }
    *shown = key;
    for (e, _) in &views {
        commands.entity(e).despawn();
    }
    if !on {
        return;
    }
    let mound = materials.add(StandardMaterial { base_color: Color::srgb(0.45, 0.3, 0.15), ..default() });
    let spent = materials.add(StandardMaterial { base_color: Color::srgb(0.5, 0.5, 0.5), ..default() });
    let post = materials.add(StandardMaterial { base_color: Color::srgb(0.55, 0.38, 0.2), emissive: LinearRgba::rgb(0.08, 0.05, 0.02), ..default() });
    for m in &b.moles {
        let done = mole_abilities(m.sm, m.id).iter().all(|a| b.has(*a));
        let mesh = meshes.add(Cone::new(60.0 * SCALE, 45.0 * SCALE));
        commands.spawn((MapMarker, Mesh3d(mesh), MeshMaterial3d(if done { spent.clone() } else { mound.clone() }), Transform::from_translation(to_bevy([m.pos[0], m.pos[1] + 22.0, m.pos[2]]))));
    }
    for c in &b.climbs {
        let h = (c.top[1] - c.bottom[1]) as f32;
        let mesh = meshes.add(Cylinder::new((c.radius as f32 * 0.25).max(6.0) * SCALE, h * SCALE));
        commands.spawn((MapMarker, Mesh3d(mesh), MeshMaterial3d(post.clone()), Transform::from_translation(to_bevy([c.bottom[0], c.bottom[1] + h as f64 * 0.5, c.bottom[2]]))));
    }
}

/// The lair's closed note doors: a glowing purple slab where the setup places them (the door model is not extracted).
fn sync_note_doors(
    mut commands: Commands,
    current: Res<Character>,
    sim: Res<GameSim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    views: Query<(Entity, &NoteDoorView)>,
) {
    use jumpman_sim::banjo_ext::{DOOR_HALF_WIDTH, DOOR_HEIGHT, DOOR_THICKNESS};
    let b = &sim.mario.banjo;
    let want: Vec<&jumpman_sim::banjo_ext::NoteDoor> =
        if is_banjo(&current) && b.in_progression() { b.door_actors.iter().filter(|d| !b.note_door_open(d.index)).collect() } else { vec![] };
    let have: Vec<u32> = views.iter().map(|(_, v)| v.0).collect();
    if want.len() == have.len() && want.iter().all(|d| have.contains(&d.index)) {
        return;
    }
    for (e, _) in &views {
        commands.entity(e).despawn();
    }
    for d in want {
        let mesh = meshes.add(Cuboid::new(DOOR_HALF_WIDTH as f32 * 2.0 * SCALE, DOOR_HEIGHT as f32 * SCALE, DOOR_THICKNESS as f32 * SCALE));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.2, 0.9, 0.55),
            emissive: LinearRgba::rgb(0.3, 0.1, 0.55),
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let at = to_bevy([d.pos[0], d.pos[1] + DOOR_HEIGHT as f64 * 0.5, d.pos[2]]);
        commands.spawn((NoteDoorView(d.index), Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(at).with_rotation(Quat::from_rotation_y((d.yaw as f32).to_radians()))));
    }
}

fn pickup_color(kind: PickupKind) -> Color {
    match kind {
        PickupKind::Note => Color::srgb(1.0, 0.85, 0.2),
        PickupKind::Egg => Color::srgb(0.7, 0.85, 1.0),
        PickupKind::RedFeather => Color::srgb(0.95, 0.15, 0.15),
        PickupKind::GoldFeather => Color::srgb(1.0, 0.78, 0.1),
        PickupKind::Honeycomb => Color::srgb(0.95, 0.55, 0.1),
        PickupKind::Jiggy => Color::srgb(0.6, 0.25, 0.9),
        PickupKind::MumboToken => Color::srgb(0.9, 0.9, 0.8),
    }
}

fn sync_pickups(
    mut commands: Commands,
    current: Res<Character>,
    sim: Res<GameSim>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut views: Query<(Entity, &PickupView, &mut Transform)>,
) {
    let want: &[jumpman_sim::banjo_ext::BkPickup] = if is_banjo(&current) { &sim.mario.banjo.pickups } else { &[] };
    let have = views.iter().count();
    if have != want.len() {
        for (e, _, _) in &views {
            commands.entity(e).despawn();
        }
        for p in want {
            let mesh = match p.kind {
                PickupKind::Note => meshes.add(Cuboid::new(7.0 * SCALE * 2.0, 14.0 * SCALE * 2.0, 3.0 * SCALE * 2.0)),
                PickupKind::Egg => meshes.add(Sphere::new(9.0 * SCALE * 2.0)),
                PickupKind::RedFeather | PickupKind::GoldFeather => meshes.add(Cone::new(7.0 * SCALE * 2.5, 22.0 * SCALE * 2.5)),
                PickupKind::Honeycomb => meshes.add(Cylinder::new(14.0 * SCALE * 2.0, 8.0 * SCALE * 2.0).mesh().resolution(6)),
                PickupKind::Jiggy => meshes.add(Cuboid::new(24.0 * SCALE * 2.0, 24.0 * SCALE * 2.0, 6.0 * SCALE * 2.0)),
                PickupKind::MumboToken => meshes.add(Cylinder::new(12.0 * SCALE * 2.0, 4.0 * SCALE * 2.0).mesh().resolution(8)),
            };
            let material = materials.add(StandardMaterial { base_color: pickup_color(p.kind), emissive: pickup_color(p.kind).to_linear() * 0.6, unlit: false, ..default() });
            commands.spawn((PickupView(p.kind), Mesh3d(mesh), MeshMaterial3d(material), Transform::from_translation(to_bevy(p.pos))));
        }
        return;
    }
    for ((_, _, mut tf), p) in views.iter_mut().zip(want) {
        tf.translation = to_bevy(p.pos) + Vec3::Y * ((time.elapsed_secs() * 2.0).sin() * 0.04);
        tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 2.2);
    }
}

fn sync_eggs(
    mut commands: Commands,
    current: Res<Character>,
    sim: Res<GameSim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut views: Query<(Entity, &mut Transform), With<EggView>>,
    mut assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    let eggs: &[jumpman_sim::banjo_ext::Egg] = if is_banjo(&current) { &sim.mario.banjo.eggs_in_flight } else { &[] };
    let (mesh, material) = assets
        .get_or_insert_with(|| (meshes.add(Sphere::new(11.0 * SCALE)), materials.add(StandardMaterial { base_color: Color::srgb(0.8, 0.9, 1.0), emissive: LinearRgba::rgb(0.3, 0.4, 0.5), ..default() })))
        .clone();
    let mut existing: Vec<_> = views.iter_mut().collect();
    for (i, egg) in eggs.iter().enumerate() {
        let at = to_bevy(egg.pos);
        let scale = if egg.kind == EggKind::Head { Vec3::new(1.0, 1.0, 1.3) } else { Vec3::ONE };
        if let Some((_, tf)) = existing.get_mut(i) {
            tf.translation = at;
            tf.scale = scale;
        } else {
            commands.spawn((EggView, Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::from_translation(at).with_scale(scale)));
        }
    }
    for (e, _) in existing.iter().skip(eggs.len()) {
        commands.entity(*e).despawn();
    }
}

// ---------------------------------------------------------------------------------------------
// Events, HUD
// ---------------------------------------------------------------------------------------------

fn drain_events(current: Res<Character>, mut sim: ResMut<GameSim>, mut particles: ResMut<Particles>, mut toast: ResMut<Toast>) {
    if !is_banjo(&current) {
        sim.mario.banjo.events.clear();
        return;
    }
    for e in std::mem::take(&mut sim.mario.banjo.events) {
        match e {
            BanjoEvent::EggShattered { pos, hit } => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::EGG_SHATTER);
                particles.burst(pos, BurstOptions { color: if hit { 0xffe08a } else { 0xe8f0ff }, count: if hit { 14 } else { 8 }, speed: 220.0, life: 0.4, gravity: 500.0, size: 14.0, upward: 100.0, spread: 1.0 });
            }
            BanjoEvent::BeakBuster { pos } => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::BEAK_BUSTER);
                particles.pound(pos)
            }
            BanjoEvent::Collected(kind) => {
                if crate::bk_music::owns_course(sim.course.request.id.as_str()) {
                    if let Some(t) = crate::bk_music::jingle_for(kind) {
                        crate::bk_music::cue(crate::bk_music::Cue::Jingle(t));
                    }
                }
                particles.sparks(sim.mario.pos, 6);
                toast.show(kind.label(), 0.8);
            }
            BanjoEvent::OutOf(kind) => toast.show(format!("NO {}", kind.label()), 1.0),
            BanjoEvent::ShockSpring => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::SHOCK_SPRING);
                particles.pound(sim.mario.pos)
            }
            BanjoEvent::TakeOff => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::TAKE_OFF);
                particles.dust(sim.mario.pos, 14)
            }
            BanjoEvent::WonderwingOn => toast.show("WONDERWING", 1.0),
            BanjoEvent::CatchBreath { air } => {
                if air < jumpman_sim::banjo_ext::AIR_BUBBLE * 3 {
                    toast.show("PHEW!", 0.8);
                }
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::CATCH_BREATH);
                particles.sparks(sim.mario.pos, 4);
            }
            BanjoEvent::Drowning => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::DROWNING);
                toast.show("OUT OF AIR", 1.2)
            }
            BanjoEvent::Learned(a) => toast.show(format!("LEARNED {}", a.label()), 2.2),
            BanjoEvent::NoteDoorOpened(i) => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::DOOR_OPEN);
                toast.show(format!("NOTE DOOR {i} OPENED"), 2.0)
            }
            BanjoEvent::FormChanged(f) => {
                particles.sparks(sim.mario.pos, 14);
                toast.show(if f == jumpman_sim::banjo_ext::Form::Banjo { "BANJO AGAIN".to_string() } else { format!("MUMBO: {}", f.name()) }, 2.0)
            }
            BanjoEvent::PicturePieces(pic, n, cost) => toast.show(format!("PICTURE {pic}: {n}/{cost} JIGGIES"), 1.5),
            BanjoEvent::DoorBroken(_) => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::EGG_SHATTER);
                particles.dust(sim.mario.pos, 10)
            }
            BanjoEvent::FormAttack(_) => {
                let _ = crate::audio::bk_sfx::play(crate::audio::bk_sfx::CATCH_BREATH);
            }
            BanjoEvent::NeedTokens(n) => toast.show(format!("MUMBO NEEDS {n} TOKENS"), 2.0),
            BanjoEvent::SwitchPressed(_) => {
                crate::audio::bk_sfx::play(crate::audio::bk_sfx::DOOR_OPEN);
                toast.show("SWITCH", 1.0)
            }
            BanjoEvent::PuzzleComplete(w) => toast.show(format!("PUZZLE {w} COMPLETE"), 2.0),
            BanjoEvent::ClimbGrab => particles.dust(sim.mario.pos, 4),
            BanjoEvent::ClimbRelease => {}
            BanjoEvent::ClimbStep => {
                crate::audio::bk_sfx::play(if crate::audio::bk_sfx::metallic_climb(&sim.course.request.id) { crate::audio::bk_sfx::CLIMB_STEP_METAL } else { crate::audio::bk_sfx::CLIMB_STEP });
            }
            BanjoEvent::EggFired { .. } | BanjoEvent::BeakBomb | BanjoEvent::WonderwingOff => {}
        }
    }
}

#[derive(Component)]
struct BanjoHud;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        BanjoHud,
        Text::new(""),
        TextFont { font_size: FontSize::Px(15.0), ..default() },
        TextColor(Color::WHITE),
        TextShadow::default(),
        Visibility::Hidden,
        GlobalZIndex(30),
        Node { position_type: PositionType::Absolute, left: Val::Px(14.0), top: Val::Px(150.0), ..default() },
    ));
}

/// The HUD lines for the current counters (also used by tests).
pub fn hud_text(sim: &GameSim) -> String {
    let b = &sim.mario.banjo;
    let m = &sim.mario;
    let combs = (m.health / 256.0).ceil().max(0.0) as usize;
    let honey: String = (0..8).map(|i| if i < combs { '#' } else { '.' }).collect();
    let power = match b.power {
        Power::Turbo => format!("TURBO TRAINERS {}S", (b.turbo_frames / 30.0).ceil() as i32),
        p => p.label().to_string(),
    };
    // The air meter (`fxairscore`): six bubbles, shown while swimming or not yet refilled.
    let air = if b.air < jumpman_sim::banjo_ext::AIR_MAX || jumpman_sim::banjo_ext::underwater(m) {
        let n = b.air_bubbles() as usize;
        let bubbles: String = (0..6).map(|i| if i < n { 'O' } else { '.' }).collect();
        format!("\nAIR [{bubbles}]")
    } else {
        String::new()
    };
    let tokens = if b.mumbo_tokens > 0 { format!("   TOKENS {}", b.mumbo_tokens) } else { String::new() };
    // A closed note door nearby shows what it costs (`code_73640_printItemCount(ITEM_C_NOTE)` within 500 units).
    let door = if b.in_progression() {
        b.door_actors
            .iter()
            .filter(|d| !b.note_door_open(d.index) && (d.pos[0] - m.pos[0]).hypot(d.pos[2] - m.pos[2]) < 500.0)
            .map(|d| format!("\nNOTE DOOR {}: {}/{}", d.index, b.notes, jumpman_sim::banjo_ext::NOTE_DOOR_COST[(d.index - 1) as usize]))
            .next()
            .unwrap_or_default()
    } else {
        String::new()
    };
    format!(
        "EGGS {:>3}   RED {:>3}   GOLD {:>2}\nNOTES {}   JIGGIES {}{tokens}\nHONEYCOMB [{honey}]{air}\nPAD: {power}{door}",
        b.eggs,
        b.red_feathers,
        b.gold_feathers,
        b.notes + sim.coins as i32,
        b.jiggies
    )
}

fn update_hud(current: Res<Character>, sim: Res<GameSim>, mut hud: Query<(&mut Text, &mut Visibility), With<BanjoHud>>) {
    for (mut text, mut vis) in &mut hud {
        let show = is_banjo(&current);
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if show {
            let s = hud_text(&sim);
            if text.0 != s {
                text.0 = s;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Touch buttons
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
struct TouchButton(u8);

fn spawn_touch_buttons(mut commands: Commands) {
    let fill = Color::srgba(0.35, 0.22, 0.08, 0.8);
    let rim = Color::srgba(1.0, 1.0, 1.0, 0.6);
    for (i, (label, _)) in TOUCH_BUTTONS.iter().enumerate() {
        commands
            .spawn((
                TouchButton(i as u8),
                Visibility::Hidden,
                GlobalZIndex(41),
                Node { position_type: PositionType::Absolute, width: Val::Px(BANJO_BUTTON), height: Val::Px(BANJO_BUTTON), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(fill),
                BorderColor::all(rim),
                BorderRadius::MAX,
            ))
            .with_child((Text::new(*label), TextFont { font_size: FontSize::Px(11.0), ..default() }, TextColor(Color::WHITE)));
    }
}

fn update_touch_buttons(
    tc: Res<TouchControls>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<SafeArea>,
    mut parts: Query<(&TouchButton, &mut Visibility, &mut Node, &mut BorderColor)>,
) {
    let Ok(window) = windows.single() else { return };
    let rects = layout(window.size(), Vec3::new(area.left, area.right, area.bottom));
    for (part, mut vis, mut node, mut border) in &mut parts {
        let want = if tc.banjo_active && tc.visible && !tc.suppress && !tc.hide_pad { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some((_, r)) = rects.iter().find(|(i, _)| *i == part.0) {
            node.left = Val::Px(r.min.x);
            node.top = Val::Px(r.min.y);
        }
        *border = BorderColor::all(if tc.banjo_down & (1 << part.0) != 0 { Color::srgb(1.0, 0.82, 0.29) } else { Color::srgba(1.0, 1.0, 1.0, 0.6) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banjo_buttons_fit_and_clear_the_base_controls() {
        let size = Vec2::new(844.0, 390.0);
        let inset = Vec3::ZERO;
        let base = crate::touch::layout(size, inset);
        let mut all = layout(size, inset);
        assert_eq!(all.len(), TOUCH_BUTTONS.len());
        for (_, r) in &all {
            assert!(r.min.x >= 0.0 && r.min.y >= 0.0 && r.max.x <= size.x && r.max.y <= size.y);
            for (_, b) in &base {
                assert!(r.intersect(*b).is_empty(), "{r:?} overlaps {b:?}");
            }
        }
        let n = all.len();
        for i in 0..n {
            for j in i + 1..n {
                assert!(all[i].1.intersect(all[j].1).is_empty(), "buttons {} and {} overlap", all[i].0, all[j].0);
            }
        }
        all.sort_by_key(|(i, _)| *i);
        assert_eq!(all.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
    }

    /// Scripted scenario on the real Bob-omb Battlefield: egg, trot, jump, wonderwing, buster, flight.
    #[test]
    fn banjo_plays_a_scripted_scenario_in_bob() {
        use crate::game::world::CourseRequest;
        use jumpman_sim::banjo_ext::*;
        use jumpman_sim::state::*;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let mut sim = GameSim::load(&root, CourseRequest::new("bob")).unwrap();
        sim.as_banjo = true;
        let step = |sim: &mut GameSim, stick: f64, down: u32, pressed: u32| {
            let c = ControllerSnapshot { stick_y: stick, stick_mag: stick.abs(), button_down: down | pressed, button_pressed: pressed, ..Default::default() };
            sim.tick(&c);
        };
        for _ in 0..30 {
            step(&mut sim, 0.0, 0, 0);
        }
        // Egg.
        let eggs = sim.mario.banjo.eggs;
        step(&mut sim, 0.0, 0, C_UP);
        for _ in 0..22 {
            step(&mut sim, 0.0, 0, 0);
        }
        assert_eq!(sim.mario.banjo.eggs, eggs - 1);
        // Talon Trot with Z held, then a trot jump.
        step(&mut sim, 64.0, Z_TRIG, Z_TRIG | C_LEFT);
        for _ in 0..10 {
            step(&mut sim, 64.0, Z_TRIG, 0);
        }
        assert_eq!(sim.mario.action_arg, BANJO_TROT);
        step(&mut sim, 64.0, Z_TRIG | A_BUTTON, A_BUTTON);
        assert_eq!(sim.mario.action_arg, BANJO_TROT_JUMP);
        for _ in 0..60 {
            step(&mut sim, 64.0, Z_TRIG, 0);
        }
        // Back to a stop, Wonderwing, Beak Buster from a jump.
        for _ in 0..40 {
            step(&mut sim, 0.0, 0, 0);
        }
        step(&mut sim, 0.0, Z_TRIG, Z_TRIG | C_RIGHT);
        assert_eq!(sim.mario.action_arg, BANJO_WONDER);
        for _ in 0..5 {
            step(&mut sim, 0.0, Z_TRIG, 0);
        }
        step(&mut sim, 0.0, 0, 0);
        step(&mut sim, 0.0, A_BUTTON, A_BUTTON);
        for _ in 0..8 {
            step(&mut sim, 0.0, A_BUTTON, 0);
        }
        step(&mut sim, 0.0, Z_TRIG, Z_TRIG);
        assert_eq!(sim.mario.action_arg, BANJO_BUSTER);
        for _ in 0..80 {
            step(&mut sim, 0.0, 0, 0);
        }
        assert!(sim.mario.pos.iter().all(|v| v.is_finite()));
        assert!(sim.mario.banjo.events.iter().any(|e| matches!(e, BanjoEvent::BeakBuster { .. })));
        assert!(!hud_text(&sim).is_empty());
    }

    #[test]
    fn hud_shows_the_air_meter_only_when_it_is_not_full() {
        use crate::game::world::CourseRequest;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let Ok(mut sim) = GameSim::load(&root, CourseRequest::new("bob")) else { return };
        sim.as_banjo = true;
        assert!(!hud_text(&sim).contains("AIR"));
        sim.mario.banjo.air = 1500;
        assert!(hud_text(&sim).contains("AIR [OOO..."), "{}", hud_text(&sim));
    }

    #[test]
    fn polluted_water_is_rbb_and_ccw_winter() {
        use jumpman_sim::banjo_ext::polluted_course;
        assert!(polluted_course("bk_rusty_bucket_bay") && polluted_course("bk_rbb_boathouse"));
        assert!(polluted_course("bk_click_clock_wood_winter") && polluted_course("bk_ccw_winter_nabnuts_house"));
        assert!(!polluted_course("bk_click_clock_wood_spring") && !polluted_course("bk_treasure_trove_cove"));
    }

    #[test]
    fn script_parses() {
        let steps = parse_script("2:z+left@3,6:a");
        assert_eq!(steps.len(), 2);
        assert_eq!((steps[0].at, steps[0].hold, steps[0].buttons.len()), (2.0, 3.0, 2));
        assert_eq!(steps[1].buttons, vec!["a".to_string()]);
    }

    #[test]
    fn touch_button_masks_are_the_n64_c_buttons() {
        assert_eq!(TOUCH_BUTTONS[0].1, C_UP);
        assert_eq!(TOUCH_BUTTONS[4].1, R_TRIG);
    }
}
