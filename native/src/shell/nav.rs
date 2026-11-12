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
#[derive(Message, Clone, Copy, PartialEq, Debug)]
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
    /// Shoulder buttons / Tab: previous (-1) or next (+1) tab of the hub.
    Tab(i32),
}

/// The activated action (mouse click or Activate); consumed by `screens::apply_actions`.
#[derive(Message, Clone, Debug)]
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
    mut keys: MessageReader<KeyboardInput>,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    state: Res<AppState>,
    mut repeat: Local<StickRepeat>,
    mut out: MessageWriter<NavCmd>,
    bindings: Option<Res<crate::bindings::Bindings>>,
) {
    // `swallow_nav`: the rebind ended earlier this frame (Esc cancel / assigned key); that press is not a menu command.
    if bindings.is_some_and(|b| b.listening.is_some() || b.swallow_nav) {
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
            KeyCode::Tab | KeyCode::BracketRight | KeyCode::PageDown if state.in_menu() => {
                out.write(NavCmd::Tab(1));
            }
            KeyCode::BracketLeft | KeyCode::PageUp if state.in_menu() => {
                out.write(NavCmd::Tab(-1));
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
    if pad.just_pressed(GamepadButton::LeftTrigger) {
        out.write(NavCmd::Tab(-1));
    }
    if pad.just_pressed(GamepadButton::RightTrigger) {
        out.write(NavCmd::Tab(1));
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

type Focusable<'w, 's> = Query<'w, 's, (Entity, &'static UiAction, &'static bevy::ui::UiGlobalTransform, Has<Disabled>)>;

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
            let (pa, pb) = (a.2.translation, b.2.translation);
            (pa.y.round(), pa.x).partial_cmp(&(pb.y.round(), pb.x)).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    if let Some((e, _, _, _)) = chosen {
        focus.0 = Some(*e);
    }
}

pub fn apply_nav(
    mut cmds: MessageReader<NavCmd>,
    mut focus: ResMut<Focus>,
    buttons: Focusable,
    mut ui: ResMut<super::ShellUi>,
    mut actions: MessageWriter<UiActionEvent>,
    mut state_cmds: MessageWriter<super::StateCmd>,
    mut reveal: ResMut<super::scroll::RevealFocus>,
    mut sfx: Option<ResMut<super::MenuSfx>>,
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
                let candidates: Vec<_> = buttons.iter().filter(|b| !b.3).map(|b| (b.0, b.2.translation)).collect();
                if let Some(next) = pick(from.translation, &candidates, dir) {
                    focus.0 = Some(next);
                    ui.last_focus = buttons.get(next).ok().map(|b| b.1.clone());
                    reveal.0 = true;
                    super::MenuSfx::push(&mut sfx, super::MenuSfx::MOVE);
                }
            }
            NavCmd::Activate => {
                if let Some((_, action, _, disabled)) = focused {
                    if !disabled {
                        actions.write(UiActionEvent(action.clone()));
                    } else {
                        super::MenuSfx::push(&mut sfx, super::MenuSfx::DENIED);
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
            NavCmd::Tab(d) => {
                if matches!(ui.screen, super::Screen::Title | super::Screen::Courses) {
                    actions.write(UiActionEvent(UiAction::Tab(ui.tab.step(d))));
                }
            }
        }
    }
}

/// Mouse / touch: hover moves focus; a press that is released (without turning into a scroll
/// drag) activates. Activating on release lets a finger scroll a list of buttons.
#[allow(clippy::type_complexity)]
pub fn pointer(
    changed: Query<(Entity, &Interaction, &UiAction, Has<Disabled>), Changed<Interaction>>,
    mut focus: ResMut<Focus>,
    mut ui: ResMut<super::ShellUi>,
    mut actions: MessageWriter<UiActionEvent>,
    drag: Res<super::scroll::PointerDrag>,
    mut held: Local<Option<Entity>>,
    mut sfx: Option<ResMut<super::MenuSfx>>,
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
                    // A mouse sweeping over buttons ticks like a console cursor (a tap only clicks).
                    if !drag.touch && *interaction == Interaction::Hovered {
                        super::MenuSfx::push(&mut sfx, super::MenuSfx::MOVE);
                    }
                }
                if *interaction == Interaction::Pressed {
                    *held = Some(entity);
                } else if *held == Some(entity) {
                    // Released over the button.
                    *held = None;
                    if !(drag.touch && drag.dragged) {
                        actions.write(UiActionEvent(action.clone()));
                    }
                }
            }
            Interaction::None => {
                // A finger lifts without hovering; a mouse that left the button before release cancels.
                if *held == Some(entity) {
                    *held = None;
                    if drag.touch && !drag.dragged {
                        actions.write(UiActionEvent(action.clone()));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_pick_prefers_aligned_neighbours() {
        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        let c = Entity::from_raw_u32(3).unwrap();
        let cands = [(a, Vec2::new(0.0, 50.0)), (b, Vec2::new(10.0, 120.0)), (c, Vec2::new(200.0, 10.0))];
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(0, 1)), Some(a));
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(1, 0)), Some(c));
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(0, -1)), None);
        assert_eq!(pick(Vec2::ZERO, &cands, IVec2::new(-1, 0)), None);
    }

    #[test]
    fn the_compact_pause_screen_has_no_tabs_so_shoulder_buttons_do_nothing_there() {
        use super::super::{Screen, ShellUi, Tab};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Focus>()
            .init_resource::<super::super::scroll::RevealFocus>()
            .insert_resource(ShellUi { screen: Screen::Pause, stack: vec![], course: "bob".into(), act: 1, page: 0, tab: Tab::Games, confirm_reset: false, last_focus: None, default_focus: None, dirty: false, pick: None })
            .add_message::<NavCmd>()
            .add_message::<UiActionEvent>()
            .add_message::<super::super::StateCmd>()
            .add_systems(Update, apply_nav);
        app.world_mut().write_message(NavCmd::Tab(1));
        app.update();
        assert_eq!(app.world().resource::<Messages<UiActionEvent>>().iter_current_update_messages().count(), 0, "no tab to switch in the pause menu");
        // the title still switches tabs
        app.world_mut().resource_mut::<ShellUi>().screen = Screen::Title;
        app.world_mut().write_message(NavCmd::Tab(1));
        app.update();
        assert_eq!(app.world().resource::<Messages<UiActionEvent>>().iter_current_update_messages().count(), 1);
        // Back from the pause menu is an ordinary Back (resumes play)
        app.world_mut().write_message(NavCmd::Back);
        app.update();
        assert!(app.world().resource::<Messages<UiActionEvent>>().iter_current_update_messages().any(|e| matches!(e.0, UiAction::Back)));
    }
}

#[cfg(test)]
mod pad_tests {
    use super::*;
    use crate::last_input::testing::*;
    use bevy::input::keyboard::KeyboardInput;

    fn app(state: AppState) -> (App, Entity) {
        let (mut app, pad) = base_app();
        *app.world_mut().resource_mut::<AppState>() = state;
        app.add_message::<KeyboardInput>().add_message::<NavCmd>().add_systems(Update, gather_nav);
        (app, pad)
    }

    #[test]
    fn menus_are_navigated_with_the_dpad_a_b_shoulders_and_start() {
        let (mut app, pad) = app(AppState::Menu);
        for (button, want) in [
            (GamepadButton::DPadDown, NavCmd::Move(IVec2::new(0, 1))),
            (GamepadButton::DPadUp, NavCmd::Move(IVec2::new(0, -1))),
            (GamepadButton::DPadLeft, NavCmd::Move(IVec2::new(-1, 0))),
            (GamepadButton::DPadRight, NavCmd::Move(IVec2::new(1, 0))),
            (GamepadButton::South, NavCmd::Activate),
            (GamepadButton::East, NavCmd::Back),
            (GamepadButton::LeftTrigger, NavCmd::Tab(-1)),
            (GamepadButton::RightTrigger, NavCmd::Tab(1)),
            (GamepadButton::Start, NavCmd::Start),
        ] {
            tap_update(&mut app, pad, button);
            assert_eq!(sent::<NavCmd>(&mut app), vec![want], "{button:?}");
        }
    }

    #[test]
    fn the_left_stick_moves_the_focus_with_a_repeat() {
        let (mut app, pad) = app(AppState::Paused);
        stick(&mut app, pad, 0.0, -1.0);
        app.update();
        assert_eq!(sent::<NavCmd>(&mut app), vec![NavCmd::Move(IVec2::new(0, 1))], "stick down moves down (Bevy y is up)");
    }

    #[test]
    fn while_playing_only_start_reaches_the_menu_layer_and_it_pauses() {
        let (mut app, pad) = app(AppState::Playing);
        tap_update(&mut app, pad, GamepadButton::South);
        assert!(sent::<NavCmd>(&mut app).is_empty(), "A belongs to the game");
        tap_update(&mut app, pad, GamepadButton::Start);
        assert_eq!(sent::<NavCmd>(&mut app), vec![NavCmd::Pause]);
    }
}
