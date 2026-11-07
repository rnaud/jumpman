//! Menu navigation: keyboard (arrows/WASD, Enter/Space, Esc), gamepad (d-pad/stick, A, B, Start)
//! and mouse hover/click all drive one [`Focus`] over the focusable [`UiAction`] buttons, with
//! spatial (nearest-in-direction) movement like a console menu.
use super::theme::Disabled;
use super::{AppState, UiAction};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct Focus(pub Option<Entity>);

/// A device-independent menu command.
#[derive(Event, Clone, Copy, PartialEq, Debug)]
pub enum NavCmd {
    /// Direction in screen space (up is -y).
    Move(IVec2),
    Activate,
    /// Esc / pad B.
    Back,
    /// Esc or pad Start while playing/paused (pause toggle).
    Pause,
    /// Pad Start in menus.
    Start,
}

/// The activated action (mouse click or Activate); consumed by `screens::apply_actions`.
#[derive(Event, Clone, Debug)]
pub struct UiActionEvent(pub UiAction);

/// Best candidate when moving `dir` from `from`: nearest along the axis, penalising sideways drift.
pub fn pick(from: Vec2, candidates: &[(Entity, Vec2)], dir: IVec2) -> Option<Entity> {
    let d = dir.as_vec2();
    candidates
        .iter()
        .filter_map(|&(e, p)| {
            let delta = p - from;
            let along = delta.dot(d);
            let across = (delta - d * along).length();
            // Must be genuinely in that direction (within a 60 degree cone).
            (along > 1.0 && across <= along * 1.7).then_some((e, along + across * 2.5))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
}

#[derive(Default)]
pub struct StickRepeat {
    dir: IVec2,
    timer: f32,
}

pub fn gather_nav(
    mut keys: EventReader<KeyboardInput>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    state: Res<AppState>,
    mut repeat: Local<StickRepeat>,
    mut out: EventWriter<NavCmd>,
    bindings: Option<Res<crate::bindings::Bindings>>,
) {
    if bindings.is_some_and(|b| b.listening.is_some()) {
        // Rebinding: the next key/button belongs to the controls screen.
        keys.clear();
        return;
    }
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        let dir = match key.key_code {
            KeyCode::ArrowUp | KeyCode::KeyW => Some(IVec2::new(0, -1)),
            KeyCode::ArrowDown | KeyCode::KeyS => Some(IVec2::new(0, 1)),
            KeyCode::ArrowLeft | KeyCode::KeyA => Some(IVec2::new(-1, 0)),
            KeyCode::ArrowRight | KeyCode::KeyD => Some(IVec2::new(1, 0)),
            _ => None,
        };
        if let Some(d) = dir {
            // Movement keys are game input while playing; OS key-repeat only matters in menus.
            if state.in_menu() {
                out.write(NavCmd::Move(d));
            }
            continue;
        }
        if key.repeat {
            continue;
        }
        match key.key_code {
            KeyCode::Enter | KeyCode::NumpadEnter if state.in_menu() => {
                out.write(NavCmd::Activate);
            }
            KeyCode::Space if state.in_menu() => {
                out.write(NavCmd::Activate);
            }
            KeyCode::Escape => {
                out.write(if state.in_menu() { NavCmd::Back } else { NavCmd::Pause });
            }
            _ => {}
        }
    }
    let Some(pad) = pads.iter().next() else { return };
    if pad.just_pressed(GamepadButton::Start) {
        out.write(if state.in_menu() { NavCmd::Start } else { NavCmd::Pause });
    }
    if !state.in_menu() {
        return;
    }
    if pad.just_pressed(GamepadButton::South) {
        out.write(NavCmd::Activate);
    }
    if pad.just_pressed(GamepadButton::East) {
        out.write(NavCmd::Back);
    }
    for (button, d) in [(GamepadButton::DPadUp, IVec2::new(0, -1)), (GamepadButton::DPadDown, IVec2::new(0, 1)), (GamepadButton::DPadLeft, IVec2::new(-1, 0)), (GamepadButton::DPadRight, IVec2::new(1, 0))] {
        if pad.just_pressed(button) {
            out.write(NavCmd::Move(d));
        }
    }
    let stick = pad.left_stick();
    let dir = if stick.length() < 0.55 {
        IVec2::ZERO
    } else if stick.x.abs() > stick.y.abs() {
        IVec2::new(stick.x.signum() as i32, 0)
    } else {
        IVec2::new(0, -(stick.y.signum() as i32))
    };
    if dir == IVec2::ZERO {
        repeat.dir = dir;
    } else if dir != repeat.dir {
        repeat.dir = dir;
        repeat.timer = 0.4;
        out.write(NavCmd::Move(dir));
    } else {
        repeat.timer -= time.delta_secs();
        if repeat.timer <= 0.0 {
            repeat.timer = 0.14;
            out.write(NavCmd::Move(dir));
        }
    }
}

type Focusable<'w, 's> = Query<'w, 's, (Entity, &'static UiAction, &'static GlobalTransform, Has<Disabled>)>;

/// Keep focus valid: if nothing (or a despawned button) is focused, pick the remembered action,
/// else the first default/top-left button.
pub fn ensure_focus(mut focus: ResMut<Focus>, buttons: Focusable, ui: Res<super::ShellUi>) {
    if focus.0.is_some_and(|e| buttons.contains(e)) {
        return;
    }
    let live: Vec<_> = buttons.iter().filter(|b| !b.3).collect();
    let remembered = ui.last_focus.as_ref().and_then(|a| live.iter().find(|b| b.1 == a));
    let chosen = remembered.or_else(|| live.iter().find(|b| Some(b.1) == ui.default_focus.as_ref())).or_else(|| {
        live.iter().min_by(|a, b| {
            let (pa, pb) = (a.2.translation(), b.2.translation());
            (pa.y.round(), pa.x).partial_cmp(&(pb.y.round(), pb.x)).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    if let Some((e, _, _, _)) = chosen {
        focus.0 = Some(*e);
    }
}

pub fn apply_nav(
    mut cmds: EventReader<NavCmd>,
    mut focus: ResMut<Focus>,
    buttons: Focusable,
    mut ui: ResMut<super::ShellUi>,
    mut actions: EventWriter<UiActionEvent>,
    mut state_cmds: EventWriter<super::StateCmd>,
) {
    for cmd in cmds.read() {
        let focused = focus.0.and_then(|e| buttons.get(e).ok());
        match *cmd {
            NavCmd::Move(dir) => {
                // Left/Right on a cycle row changes its value instead of moving.
                if let Some((_, UiAction::Cycle(key, _), _, _)) = focused {
                    if dir.y == 0 {
                        actions.write(UiActionEvent(UiAction::Cycle(*key, dir.x)));
                        continue;
                    }
                }
                let Some((_, _, from, _)) = focused else { continue };
                let candidates: Vec<_> = buttons.iter().filter(|b| !b.3).map(|b| (b.0, b.2.translation().truncate())).collect();
                if let Some(next) = pick(from.translation().truncate(), &candidates, dir) {
                    focus.0 = Some(next);
                    ui.last_focus = buttons.get(next).ok().map(|b| b.1.clone());
                }
            }
            NavCmd::Activate => {
                if let Some((_, action, _, disabled)) = focused {
                    if !disabled {
                        actions.write(UiActionEvent(action.clone()));
                    }
                }
            }
            NavCmd::Back => {
                actions.write(UiActionEvent(UiAction::Back));
            }
            NavCmd::Pause => {
                state_cmds.write(super::StateCmd::TogglePause);
            }
            NavCmd::Start => {
                state_cmds.write(super::StateCmd::PadStart);
            }
        }
    }
}

/// Mouse: hover moves focus, press activates.
#[allow(clippy::type_complexity)]
pub fn pointer(
    changed: Query<(Entity, &Interaction, &UiAction, Has<Disabled>), Changed<Interaction>>,
    mut focus: ResMut<Focus>,
    mut ui: ResMut<super::ShellUi>,
    mut actions: EventWriter<UiActionEvent>,
) {
    for (entity, interaction, action, disabled) in &changed {
        if disabled {
            continue;
        }
        match interaction {
            Interaction::Hovered | Interaction::Pressed => {
                if focus.0 != Some(entity) {
                    focus.0 = Some(entity);
                    ui.last_focus = Some(action.clone());
                }
                if *interaction == Interaction::Pressed {
                    actions.write(UiActionEvent(action.clone()));
                }
            }
            Interaction::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_pick_prefers_aligned_neighbours() {
        let a = Entity::from_raw(1);
        let b = Entity::from_raw(2);
        let c = Entity::from_raw(3);
        let cands = [(a, Vec2::new(0.0, 50.0)), (b, Vec2::new(10.0, 120.0)), (c, Vec2::new(200.0, 10.0))];
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(0, 1)), Some(a));
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(1, 0)), Some(c));
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(0, -1)), None);
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(-1, 0)), None);
    }
}
