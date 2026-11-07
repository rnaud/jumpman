//! Zelda content: Kokiri Village / Deku Tree / the other `kokiri`-block courses. Engine-free
//! ports with tests (`save`, `scene`, `visit`, `dungeon`, `gohma`, `slingshot`, `sword_quest`,
//! `textbox`, `world`) and their Bevy glue (`view`, `ui`, this file).
//!
//! The glue (`ZeldaPlugin`) is inert on courses without a `kokiri` block. On those courses it
//! runs [`world::ZeldaWorld::step`] before the 30 Hz player step (conversations, shops, climbs,
//! the dungeon puzzles and the slingshot can consume the tick), draws the actors, the OoT
//! textbox and the prompt, takes over the camera for dialogue/fixed rooms/aiming, and relaunches
//! the executable for course changes (the save travels in `JUMPMAN_ZELDA_SAVE`).
//!
//! Keys: `H` / RB hold = aim the slingshot (A/Space fires), `E` = equip/unequip it, `Z`/`K` =
//! deflect the Deku Scrub's nut. Test hooks: `JUMPMAN_ZELDA_FLAGS=sword,shield,slingshot,
//! rupees=50,gohma,...`, `JUMPMAN_ZELDA_POS=x,y,z`, `JUMPMAN_ZELDA_PRESS=2.0:A,4.0:A` (button
//! presses at the given seconds), `JUMPMAN_ZELDA_AIM=1` (hold the slingshot).
#![allow(dead_code)]
pub mod dungeon;
pub mod glb;
pub mod gohma;
pub mod save;
pub mod scene;
pub mod slingshot;
pub mod sword_quest;
pub mod textbox;
pub mod ui;
pub mod view;
pub mod visit;
pub mod world;

use crate::audio::SfxCue;
use crate::characters::Character;
use crate::experiments::StepControl;
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::render::AssetRoot;
use bevy::prelude::*;
use bevy::transform::TransformSystem;
use jumpman_sim::state::*;
use save::ZeldaSave;
use world::{Travel, ZeldaWorld};

/// A course change requested by the Zelda scene (exits, warp pad, boss door).
#[derive(Event, Clone, Debug)]
pub struct ZeldaTravel(pub Travel);

pub struct ZeldaPlugin;

fn load_save() -> ZeldaSave {
    let mut save = std::env::var("JUMPMAN_ZELDA_SAVE")
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<ZeldaSave>(&b).ok())
        .unwrap_or_default();
    apply_flags(&mut save, &std::env::var("JUMPMAN_ZELDA_FLAGS").unwrap_or_default());
    save
}

/// `JUMPMAN_ZELDA_FLAGS` test hook: `sword`, `shield`, `slingshot`, `equipped`, `path`,
/// `rupees=N`, anything else is a Deku challenge flag (`gohma`, `block`, ...).
pub fn apply_flags(save: &mut ZeldaSave, flags: &str) {
    for flag in flags.split(',').map(str::trim).filter(|f| !f.is_empty()) {
        match flag {
            "sword" => save.sword = true,
            "shield" => save.shield = true,
            "slingshot" => {
                save.open_treasure_chest("deku_tree:1", 5);
            }
            "equipped" => save.set_slingshot_equipped(true),
            "path" => save.deku_path_open = true,
            f if f.starts_with("rupees=") => save.rupees = f[7..].parse().unwrap_or(0),
            f => save.clear_deku_challenge(f),
        }
    }
}

impl Plugin for ZeldaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_save())
            .add_event::<ZeldaTravel>()
            .init_resource::<ScriptedPresses>()
            .add_systems(Startup, (setup_world, view::spawn_scene, ui::spawn_ui).chain())
            .add_systems(PostStartup, apply_light)
            .add_systems(FixedUpdate, zelda_step.before(crate::SimSet))
            .add_systems(Update, (equip_key, view::drive_animation, view::sync_actors, view::sync_effects, ui::update_ui, handle_travel))
            .add_systems(PostUpdate, view::zelda_camera.before(TransformSystem::TransformPropagate));
    }
}

/// `JUMPMAN_ZELDA_PRESS` parsed once: (seconds, button mask).
#[derive(Resource, Default)]
struct ScriptedPresses {
    list: Vec<(f64, u32)>,
    next: usize,
}

fn setup_world(
    mut commands: Commands,
    root: Res<AssetRoot>,
    mut sim: ResMut<GameSim>,
    save: Res<ZeldaSave>,
    mut character: ResMut<Character>,
    mut presses: ResMut<ScriptedPresses>,
) {
    let req = sim.course.request.clone();
    let Some(scene) = scene::ZeldaScene::load(&root.0, &req.id, req.area) else { return };
    if let Ok(pos) = std::env::var("JUMPMAN_ZELDA_POS") {
        let v: Vec<f64> = pos.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if v.len() == 3 {
            sim.mario.pos = [v[0], v[1], v[2]];
            sim.curr = sim.snapshot();
            sim.prev = sim.curr.clone();
        }
    }
    if std::env::var("JUMPMAN_CHARACTER").is_err() {
        *character = Character::Link;
    }
    if let Ok(list) = std::env::var("JUMPMAN_ZELDA_PRESS") {
        presses.list = list
            .split(',')
            .filter_map(|item| {
                let (t, b) = item.split_once(':')?;
                let mask = match b.trim() {
                    "A" => A_BUTTON,
                    "B" => B_BUTTON,
                    _ => return None,
                };
                Some((t.trim().parse().ok()?, mask))
            })
            .collect();
    }
    let world = ZeldaWorld::new(&root.0, scene, &mut sim, &save);
    info!("zelda scene: {} ({} managed objects)", req.id, world.scene.objects.len());
    commands.insert_resource(world);
}

/// The kokiri block's lighting for the actors/terrain (`setActorLights`).
fn apply_light(world: Option<Res<ZeldaWorld>>, mut ambient: ResMut<AmbientLight>, mut sun: Query<(&mut DirectionalLight, &mut Transform)>) {
    let Some(world) = world else { return };
    let l = &world.scene.kokiri.light;
    let norm = |c: [f64; 3]| {
        let m = c[0].max(c[1]).max(c[2]).max(1.0);
        (Color::srgb((c[0] / m) as f32, (c[1] / m) as f32, (c[2] / m) as f32), (m / 255.0) as f32)
    };
    let (ac, am) = norm(l.ambient);
    ambient.color = ac;
    ambient.brightness = 160.0 + 900.0 * am;
    let (kc, km) = norm(l.key);
    for (mut light, mut tf) in &mut sun {
        light.color = kc;
        light.illuminance = 700.0 + 1600.0 * km;
        let dir = Vec3::new(l.key_dir[0] as f32, l.key_dir[1] as f32, l.key_dir[2] as f32).normalize_or_zero();
        *tf = Transform::from_translation(dir * 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
}

#[allow(clippy::too_many_arguments)]
fn zelda_step(
    world: Option<ResMut<ZeldaWorld>>,
    mut sim: ResMut<GameSim>,
    mut input: ResMut<PlayerInput>,
    mut ctl: ResMut<StepControl>,
    mut save: ResMut<ZeldaSave>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    mut presses: ResMut<ScriptedPresses>,
    mut toast: ResMut<Toast>,
    mut sfx: EventWriter<SfxCue>,
    mut travel: EventWriter<ZeldaTravel>,
    fixed: Res<Time<Fixed>>,
) {
    let Some(mut world) = world else { return };
    let mut controller = ctl.controller.take().unwrap_or_else(|| input.consume_step());
    // Scripted button presses (test hook).
    let now = time.elapsed_secs_f64();
    while let Some(&(at, mask)) = presses.list.get(presses.next) {
        if at > now {
            break;
        }
        controller.button_pressed |= mask;
        controller.button_down |= mask;
        presses.next += 1;
    }
    let held = keys.pressed(KeyCode::KeyH) || pads.iter().any(|g| g.pressed(GamepadButton::RightTrigger)) || std::env::var("JUMPMAN_ZELDA_AIM").is_ok();
    let name = if sim.as_link { "Link" } else if sim.as_banjo { "Banjo" } else { "Mario" };
    // Up/Down arrows steer menu choices like the stick.
    if controller.stick_y == 0.0 {
        if keys.pressed(KeyCode::ArrowUp) {
            controller.stick_y = 64.0;
        } else if keys.pressed(KeyCode::ArrowDown) {
            controller.stick_y = -64.0;
        }
    }
    let _ = &fixed;
    let link = sim.as_link;
    let r = world.step(&mut sim, &controller, &mut save, name, held, link, now * 1000.0);
    for t in world.toasts.drain(..) {
        toast.show(t, 2.3);
    }
    if world.coin_sfx {
        sfx.write(SfxCue::Coin { underwater: false });
    }
    if let Some(t) = world.travel.take() {
        travel.write(ZeldaTravel(t));
    }
    ctl.controller = Some(controller);
    if r.skip {
        ctl.skip = true;
        sim.refresh_view();
    }
}

fn equip_key(keys: Res<ButtonInput<KeyCode>>, mut save: ResMut<ZeldaSave>, mut toast: ResMut<Toast>, world: Option<Res<ZeldaWorld>>) {
    if world.is_none() || !keys.just_pressed(KeyCode::KeyE) || !save.has_slingshot() {
        return;
    }
    let on = !save.slingshot_equipped();
    save.set_slingshot_equipped(on);
    toast.show(if on { "FAIRY SLINGSHOT EQUIPPED - HOLD H TO AIM" } else { "FAIRY SLINGSHOT PUT AWAY" }, 2.0);
}

/// Where the relaunched process finds the save.
pub fn save_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("jumpman-zelda-save-{}.json", std::process::id()))
}

fn handle_travel(mut events: EventReader<ZeldaTravel>, save: Res<ZeldaSave>, character: Res<Character>, mut exit: EventWriter<AppExit>, mut done: Local<bool>) {
    for ZeldaTravel(t) in events.read() {
        if *done {
            return;
        }
        *done = true;
        eprintln!("zelda travel: {} (entry {}, area {}{})", t.course, t.entry, t.area, if t.push { ", push" } else { "" });
        if std::env::var("JUMPMAN_NO_TRAVEL").is_ok() {
            continue;
        }
        let path = save_path();
        if std::fs::write(&path, serde_json::to_vec(&*save).unwrap_or_default()).is_err() {
            continue;
        }
        let Ok(exe) = std::env::current_exe() else { continue };
        let mut cmd = std::process::Command::new(exe);
        cmd.arg(&t.course).arg("--entry").arg(&t.entry).arg("--area").arg(t.area.to_string());
        if t.push {
            cmd.arg("--push");
        }
        let name = match *character {
            Character::Mario => "sm64",
            Character::Link => "link",
            Character::YoungLink => "young-link",
            Character::Banjo => "banjo",
        };
        cmd.env("JUMPMAN_ZELDA_SAVE", &path).env("JUMPMAN_CHARACTER", name);
        // The relaunched process inherits the test hooks only through explicit env (not flags).
        cmd.env_remove("JUMPMAN_ZELDA_FLAGS").env_remove("JUMPMAN_ZELDA_POS").env_remove("JUMPMAN_ZELDA_PRESS").env_remove("JUMPMAN_COURSE");
        if cmd.spawn().is_ok() {
            exit.write(AppExit::Success);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_preset_the_save() {
        let mut s = ZeldaSave::default();
        apply_flags(&mut s, "sword, shield, slingshot, equipped, rupees=42, gohma");
        assert!(s.sword && s.shield && s.slingshot_equipped() && s.rupees == 42 && s.deku_cleared("gohma"));
    }

    #[test]
    fn save_roundtrips_through_json() {
        let mut s = ZeldaSave::default();
        apply_flags(&mut s, "sword,slingshot,rupees=9");
        s.finish_conversation("mido");
        let back: ZeldaSave = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(back.sword && back.has_slingshot() && back.has_spoken_to("mido") && back.rupees == 9);
        assert_eq!(back.deku_seeds(), 30);
    }
}
