//! `JUMPMAN_TOUCH_SCRIPT`: scripted *real* touch events for headless checks. Every entry becomes a `TouchInput` message at the
//! centre of the named on-screen control, so the whole phone path (`touch::drive_touch`, Link's C-button layer, the item screen,
//! the camera drag) runs exactly as it does under a finger.
//!
//! `JUMPMAN_TOUCH_SCRIPT="1.5:stick=up,2.0:tap=b,2.6:tap=b:0.5,3:stick=none,4:tap=c1,5:tap=300x200"`
//! Entries are `seconds:what` (sim seconds: 30 sim ticks each, counted from the start of the course; the same clock as `JUMPMAN_LINK_DEMO` ticks / 30). `tap=NAME[:hold seconds]` (default 0.12 s) presses and releases a control: `a b z l` (the round
//! buttons), `c1 c2 c3 c4` (C-left / down / right / up), `items`, `pause`, or a raw `XxY` point in logical px.
//! `stick=up|down|left|right|upleft|upright|none` puts a finger on the pad and keeps it there (`none` lifts it).
use crate::bindings::Action;
use crate::safe_area::SafeArea;
use crate::touch::{layout, pause_rect, Control};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[derive(Clone, Debug, PartialEq)]
pub enum What {
    Tap(String, f32),
    Stick(Option<Vec2>),
    /// A keyboard press (`key=v` cycles the character, `key=tab` opens Link's item screen, `key=esc`).
    Key(KeyCode),
}

pub fn parse(script: &str) -> Vec<(f32, What)> {
    let mut out = vec![];
    for entry in script.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((t, rest)) = entry.split_once(':') else { continue };
        let Ok(t) = t.parse::<f32>() else { continue };
        let what = if let Some(spec) = rest.strip_prefix("tap=") {
            let (name, hold) = spec.split_once(':').map_or((spec, 0.12), |(n, h)| (n, h.parse().unwrap_or(0.12)));
            What::Tap(name.to_string(), hold)
        } else if let Some(k) = rest.strip_prefix("key=") {
            What::Key(match k {
                "v" => KeyCode::KeyV,
                "tab" => KeyCode::Tab,
                "esc" => KeyCode::Escape,
                _ => continue,
            })
        } else if let Some(dir) = rest.strip_prefix("stick=") {
            What::Stick(match dir {
                "up" => Some(Vec2::new(0.0, 1.0)),
                "down" => Some(Vec2::new(0.0, -1.0)),
                "left" => Some(Vec2::new(-1.0, 0.0)),
                "right" => Some(Vec2::new(1.0, 0.0)),
                "upleft" => Some(Vec2::new(-0.7, 0.7)),
                "upright" => Some(Vec2::new(0.7, 0.7)),
                _ => None,
            })
        } else {
            continue;
        };
        out.push((t, what));
    }
    out
}

/// Where a named control sits for a window of `size`.
pub fn target(name: &str, size: Vec2, inset: Vec3) -> Option<Vec2> {
    let base = layout(size, inset);
    let round = |a: Action| base.iter().find(|(c, _)| *c == Control::Button(a)).map(|(_, r)| r.center());
    let c = |bit: u32| crate::link::ui::button_rects(size, inset).into_iter().find(|(b, _)| *b == bit).map(|(_, r)| r.center());
    use jumpman_sim::link::moves::{C_DOWN, C_LEFT, C_RIGHT, C_UP};
    match name {
        "a" => round(Action::Jump),
        "b" => round(Action::Punch),
        "z" => round(Action::Crouch),
        "l" => round(Action::Focus),
        "c1" => c(C_LEFT),
        "c2" => c(C_DOWN),
        "c3" => c(C_RIGHT),
        "c4" => c(C_UP),
        "items" => c(0x1_0000),
        "pause" => Some(pause_rect(size, inset.y, 0.0).center()),
        _ => name.split_once('x').and_then(|(x, y)| Some(Vec2::new(x.parse().ok()?, y.parse().ok()?))),
    }
}

#[derive(Resource)]
struct Script {
    steps: Vec<(f32, What)>,
    next: usize,
    /// (release time, finger id, position) of taps in flight.
    held: Vec<(f32, u64, Vec2)>,
    stick: Option<u64>,
    ids: u64,
    keys: Vec<KeyCode>,
}

pub struct TouchScriptPlugin;

impl Plugin for TouchScriptPlugin {
    fn build(&self, app: &mut App) {
        let Ok(spec) = std::env::var("JUMPMAN_TOUCH_SCRIPT") else { return };
        app.insert_resource(Script { steps: parse(&spec), next: 0, held: vec![], stick: None, ids: 500, keys: vec![] }).add_systems(PreUpdate, (run.before(InputSystems), press_keys.after(InputSystems)));
    }
}

fn run(mut s: ResMut<Script>, sim: Res<crate::game::runner::GameSim>, windows: Query<(Entity, &Window), With<PrimaryWindow>>, area: Res<SafeArea>, mut out: MessageWriter<TouchInput>) {
    let Ok((window, w)) = windows.single() else { return };
    let (size, inset) = (w.size(), Vec3::new(area.left, area.right, area.bottom));
    // Script time is sim time (30 ticks a second since the course started), so it does not depend on how long loading took.
    let now = sim.ticks as f32 / 30.0;
    let ev = |phase, position, id| TouchInput { phase, position, window, force: None, id };
    let held = std::mem::take(&mut s.held);
    for (at, id, pos) in held {
        if now >= at {
            out.write(ev(TouchPhase::Ended, pos, id));
        } else {
            s.held.push((at, id, pos));
        }
    }
    while s.next < s.steps.len() && s.steps[s.next].0 <= now {
        let (_, what) = s.steps[s.next].clone();
        s.next += 1;
        s.ids += 1;
        let id = s.ids;
        match what {
            What::Tap(name, hold) => {
                let Some(pos) = target(&name, size, inset) else { continue };
                out.write(ev(TouchPhase::Started, pos, id));
                s.held.push((now + hold, id, pos));
            }
            What::Key(k) => s.keys.push(k),
            What::Stick(dir) => {
                let pad = layout(size, inset).into_iter().find(|(c, _)| *c == Control::Stick).map(|(_, r)| r.center()).unwrap_or_default();
                if let Some(old) = s.stick.take() {
                    out.write(ev(TouchPhase::Ended, pad, old));
                }
                if let Some(d) = dir {
                    out.write(ev(TouchPhase::Started, pad, id));
                    out.write(ev(TouchPhase::Moved, pad + Vec2::new(d.x, -d.y) * 40.0, id));
                    s.stick = Some(id);
                }
            }
        }
    }
    // A held stick finger: keep reporting it so `Touches` sees it as pressed.
}

/// Keys go in after Bevy's own keyboard system has cleared last frame's edges, so `just_pressed` is true for this frame's readers.
fn press_keys(mut s: ResMut<Script>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    for k in std::mem::take(&mut s.keys) {
        keys.press(k);
        keys.release(k);
    }
}
