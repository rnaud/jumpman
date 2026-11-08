//! Scripted input for headless checks (`JUMPMAN_LINK_DEMO`), like `JUMPMAN_WALK`:
//! `JUMPMAN_LINK_DEMO="30:lock,40:stick=up,60:a,90:c1:20,120:shot=bow"`, entries `tick:what[:hold]`
//! (sim ticks since the course started). `what` is `a` / `b` / `z` (button, held `hold` ticks), `c1` `c2` `c3` `c4`
//! (C-left, down, right, up), `stick=up|down|left|right|upleft|upright|none`, `lock` (toggle Z-target) or
//! `shot=name` (a PNG into `JUMPMAN_LINK_SHOTS`, default the current directory).
use super::{LinkInput, LockOn};
use crate::game::runner::GameSim;
use crate::input::PlayerInput;
use crate::sim_plugin::PreTickSet;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use jumpman_sim::link::items::{CItem, Item};
use jumpman_sim::link::moves::{C_DOWN, C_LEFT, C_RIGHT, C_UP};
use jumpman_sim::state::{A_BUTTON, B_BUTTON, Z_TRIG};

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Button(u32, u32),
    C(u32, u32),
    Stick(f32, f32),
    Lock,
    Shot(String),
}

pub fn parse(script: &str) -> Vec<(u64, Step)> {
    let mut out = vec![];
    for entry in script.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let mut parts = entry.splitn(3, ':');
        let (Some(t), Some(what)) = (parts.next().and_then(|t| t.parse::<u64>().ok()), parts.next()) else { continue };
        let hold = parts.next().and_then(|h| h.parse::<u32>().ok()).unwrap_or(1);
        let step = match what {
            "a" => Step::Button(A_BUTTON, hold),
            "b" => Step::Button(B_BUTTON, hold),
            "z" => Step::Button(Z_TRIG, hold),
            "c1" => Step::C(C_LEFT, hold),
            "c2" => Step::C(C_DOWN, hold),
            "c3" => Step::C(C_RIGHT, hold),
            "c4" => Step::C(C_UP, hold),
            "lock" => Step::Lock,
            "stick=up" => Step::Stick(0.0, 64.0),
            "stick=down" => Step::Stick(0.0, -64.0),
            "stick=left" => Step::Stick(-64.0, 0.0),
            "stick=right" => Step::Stick(64.0, 0.0),
            "stick=upleft" => Step::Stick(-45.0, 45.0),
            "stick=upright" => Step::Stick(45.0, 45.0),
            "stick=none" => Step::Stick(0.0, 0.0),
            s if s.starts_with("shot=") => Step::Shot(s[5..].to_string()),
            _ => continue,
        };
        out.push((t, step));
    }
    out
}

#[derive(Resource, Default)]
struct Demo {
    steps: Vec<(u64, Step)>,
    stick: (f32, f32),
    held: Vec<(u32, u32, bool)>,
    done: usize,
}

/// `JUMPMAN_LINK_C="firearrow,dins,stick"`: the items on C-left, C-down, C-right (names: bow firearrow icearrow lightarrow slingshot
/// stick nut bomb bombchu hookshot boomerang hammer dins farore nayru lens bean ocarina), set once Link is the active character.
pub fn parse_loadout(spec: &str) -> Vec<Item> {
    use Item::*;
    spec.split(',')
        .filter_map(|n| {
            Some(match n.trim() {
                "bow" => Bow,
                "firearrow" => FireArrow,
                "icearrow" => IceArrow,
                "lightarrow" => LightArrow,
                "slingshot" => Slingshot,
                "stick" => DekuStick,
                "nut" => DekuNut,
                "bomb" => Bomb,
                "bombchu" => Bombchu,
                "hookshot" => Hookshot,
                "boomerang" => Boomerang,
                "hammer" => MegatonHammer,
                "dins" => DinsFire,
                "farore" => FaroresWind,
                "nayru" => NayrusLove,
                "lens" => LensOfTruth,
                "bean" => MagicBean,
                "ocarina" => Ocarina,
                _ => return None,
            })
        })
        .take(3)
        .collect()
}

fn set_loadout(mut sim: ResMut<GameSim>, mut done: Local<bool>) {
    if *done || !sim.mario.link.enabled {
        return;
    }
    *done = true;
    let Ok(spec) = std::env::var("JUMPMAN_LINK_C") else { return };
    let age = sim.mario.link.age;
    for (slot, item) in parse_loadout(&spec).into_iter().enumerate() {
        sim.mario.link.inv.set_c(slot, Some(CItem::Item(item)), age);
    }
}

pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("JUMPMAN_LINK_C").is_some() {
            app.add_systems(FixedUpdate, set_loadout.in_set(PreTickSet).before(super::pre_tick));
        }
        let Ok(script) = std::env::var("JUMPMAN_LINK_DEMO") else { return };
        app.insert_resource(Demo { steps: parse(&script), ..default() }).add_systems(FixedUpdate, run_demo.in_set(PreTickSet).before(super::pre_tick));
    }
}

fn run_demo(mut demo: ResMut<Demo>, sim: Res<GameSim>, mut input: ResMut<PlayerInput>, mut li: ResMut<LinkInput>, mut lock: ResMut<LockOn>, mut commands: Commands) {
    let now = sim.ticks;
    while demo.done < demo.steps.len() && demo.steps[demo.done].0 <= now {
        let (_, step) = demo.steps[demo.done].clone();
        demo.done += 1;
        match step {
            Step::Button(mask, hold) => {
                input.latch(mask);
                demo.held.push((mask, hold, false));
            }
            Step::C(bit, hold) => {
                li.c_pressed |= bit;
                demo.held.push((bit, hold, true));
            }
            Step::Stick(x, y) => demo.stick = (x, y),
            Step::Lock => {
                lock.locked = !lock.locked;
                input.focus_held = lock.locked;
            }
            Step::Shot(name) => {
                let dir = std::env::var("JUMPMAN_LINK_SHOTS").unwrap_or_else(|_| ".".into());
                let path = format!("{dir}/{name}.png");
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            }
        }
    }
    let (x, y) = demo.stick;
    if x != 0.0 || y != 0.0 {
        input.stick = Vec2::new(x, y);
        input.stick_mag = (x * x + y * y).sqrt().min(64.0);
    }
    let mut down_c = 0;
    let mut down_b = 0;
    for h in demo.held.iter_mut() {
        if h.1 > 0 {
            h.1 -= 1;
            if h.2 {
                down_c |= h.0;
            } else {
                down_b |= h.0;
            }
        }
    }
    demo.held.retain(|h| h.1 > 0);
    li.c_down |= down_c;
    input.down |= down_b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_script_grammar() {
        let s = parse("30:lock, 40:stick=up,60:a:5,90:c1:20,120:shot=bow,x:y,70:nonsense");
        assert_eq!(s.len(), 5);
        assert_eq!(s[0], (30, Step::Lock));
        assert_eq!(s[1], (40, Step::Stick(0.0, 64.0)));
        assert_eq!(s[2], (60, Step::Button(A_BUTTON, 5)));
        assert_eq!(s[3], (90, Step::C(C_LEFT, 20)));
        assert_eq!(s[4], (120, Step::Shot("bow".into())));
    }

    #[test]
    fn parses_the_loadout() {
        assert_eq!(parse_loadout("firearrow, dins ,stick,bow,nonsense"), vec![Item::FireArrow, Item::DinsFire, Item::DekuStick]);
        assert!(parse_loadout("x").is_empty());
    }
}
