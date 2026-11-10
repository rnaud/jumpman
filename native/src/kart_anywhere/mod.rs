//! A kart the player can call in any level they are walking in (SM64 courses, Ocarina scenes and dungeons,
//! Banjo-Kazooie levels): the active character sits in it and drives over the level's own collision with the MK64
//! kart physics, then steps out where the kart stands. Free driving only: no items, laps, opponents or race HUD.
//!
//! Call / leave: the `Kart` action (keyboard `B` by default, rebindable; a gamepad button if one is bound),
//! LB + RB together on a gamepad, the KART touch button beside pause, or the pause menu's CALL KART / LEAVE KART.
//! Driving uses the kart mode's controls: stick steers, stick up is the throttle, stick down or Z brakes and
//! reverses, A hops and drifts.
//!
//! Hooks outside this module, each marked `kart_anywhere hook`: the pause list entry (`shell/retro/pages.rs`), its
//! action (`shell/screens.rs`), `Action::Kart` (`bindings.rs`) and the plugin (`lib.rs`). Everything else goes
//! through existing seams: `StepControl` (the simulation gets an empty controller while the kart has the real one),
//! `GameSim::skip_mario_step` (the on-foot controller does not run), `CameraScript` (the chase camera).
mod ride;
mod view;

use crate::bindings::{Action, Bindings};
use crate::button_marker::BorderRadius;
use crate::experiments::{ExperimentSlot, StepControl};
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::input::PlayerInput;
use crate::shell::AppState;
use crate::sim_plugin::{ModalActive, PreTickSet, SimResetMsg};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jumpman_sim::state::ControllerSnapshot;
use ride::{Ride, Tick};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// What the pause menu offers: 0 nothing, 1 CALL KART, 2 LEAVE KART.
static OFFER: AtomicU8 = AtomicU8::new(0);
/// A call / leave request from the pause menu (or the test hook).
static REQUEST: AtomicBool = AtomicBool::new(false);

/// The pause menu picked CALL KART / LEAVE KART.
pub fn request_toggle() {
    REQUEST.store(true, Ordering::Relaxed);
}

/// The pause menu's entry, if a kart can be called or left right now.
pub fn pause_label() -> Option<&'static str> {
    match OFFER.load(Ordering::Relaxed) {
        1 => Some("CALL KART"),
        2 => Some("LEAVE KART"),
        _ => None,
    }
}

#[derive(Resource, Default)]
pub struct KartAnywhere {
    ride: Option<Ride>,
    /// The chase camera starts over (new ride, rescue).
    snap: bool,
}

impl KartAnywhere {
    pub fn driving(&self) -> bool {
        self.ride.is_some()
    }
}

/// Touch button geometry: left of the pause button.
const TOUCH_SIZE: f32 = 40.;
const TOUCH_GAP: f32 = 10.;

fn touch_rect(size: Vec2, right_inset: f32, top_inset: f32) -> Rect {
    let pause = crate::touch::pause_rect(size, right_inset, top_inset);
    let min = Vec2::new(pause.min.x - TOUCH_GAP - TOUCH_SIZE, pause.min.y);
    Rect::from_corners(min, min + Vec2::splat(TOUCH_SIZE))
}

#[derive(Component)]
struct TouchButton;

pub struct KartAnywherePlugin;

impl Plugin for KartAnywherePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KartAnywhere>()
            .init_resource::<view::ChaseCam>()
            .add_systems(Startup, spawn_touch_button)
            .add_systems(
                FixedUpdate,
                drive.before(PreTickSet).before(crate::SimSet).run_if(crate::shell::is_playing).run_if(crate::progress::sim_running).run_if(|modal: Res<ModalActive>| !modal.0),
            )
            .add_systems(Update, (watch, read_inputs, toggle, publish, view::chase_camera, view::sync_kart, update_touch_button).chain().before(crate::CameraSet).after(crate::sim_plugin::handle_reset));
        app.add_systems(Update, view::seat_link.after(crate::characters::update_rigs).before(crate::CameraSet));
        if let Some(at) = std::env::var("JUMPMAN_KART").ok().and_then(|v| v.parse::<f32>().ok()) {
            // Test hook: call the kart this many seconds into play (and again `JUMPMAN_KART_LEAVE` seconds in, to leave).
            let leave = std::env::var("JUMPMAN_KART_LEAVE").ok().and_then(|v| v.parse::<f32>().ok());
            app.add_systems(Update, (move |time: Res<Time>, state: Res<AppState>, mut played: Local<f32>| {
                if *state != AppState::Playing {
                    return;
                }
                let before = *played;
                *played += time.delta_secs();
                if (before < at && *played >= at) || leave.is_some_and(|l| before < l && *played >= l) {
                    request_toggle();
                }
            }).before(toggle));
        }
    }
}

/// A level the player is in on foot, with nothing else owning the character.
fn available(state: &AppState, slot: &ExperimentSlot) -> bool {
    matches!(state, AppState::Playing | AppState::Paused) && slot.active.is_none() && slot.pending.is_none()
}

/// The ride does not survive a new course, a reset, an experiment or leaving for the menus.
fn watch(
    mut ka: ResMut<KartAnywhere>,
    mut sim: ResMut<GameSim>,
    state: Res<AppState>,
    slot: Res<ExperimentSlot>,
    mut switched: MessageReader<crate::progress::CourseSwitched>,
    mut resets: MessageReader<SimResetMsg>,
) {
    let fresh = switched.read().count() + resets.read().count() > 0;
    let Some(ride) = ka.ride.take() else { return };
    if fresh {
        // The character was just rebuilt from scratch; only the Link pose hint could linger.
        ride.dismiss(&mut sim);
    } else if !available(&state, &slot) {
        ride.dismiss(&mut sim);
    } else {
        ka.ride = Some(ride);
    }
}

fn read_inputs(
    state: Res<AppState>,
    bindings: Res<Bindings>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    touches: Res<Touches>,
    mouse: Res<ButtonInput<MouseButton>>,
    tc: Res<crate::touch::TouchControls>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<crate::safe_area::SafeArea>,
) {
    if *state != AppState::Playing || bindings.listening.is_some() || OFFER.load(Ordering::Relaxed) == 0 {
        return;
    }
    let mut asked = bindings.just_pressed(&keys, Action::Kart);
    for pad in &pads {
        asked |= bindings.pad_buttons(Action::Kart).iter().any(|b| pad.just_pressed(*b));
        // The fixed combo: both shoulder buttons, either one going down last.
        let (lb, rb) = (GamepadButton::LeftTrigger, GamepadButton::RightTrigger);
        asked |= pad.pressed(lb) && pad.pressed(rb) && (pad.just_pressed(lb) || pad.just_pressed(rb));
    }
    if tc.visible {
        if let Ok(window) = windows.single() {
            let rect = touch_rect(window.size(), area.right, area.top);
            asked |= touches.iter_just_pressed().any(|t| rect.contains(t.position()));
            asked |= tc.simulate && mouse.just_pressed(MouseButton::Left) && window.cursor_position().is_some_and(|p| rect.contains(p));
        }
    }
    if asked {
        request_toggle();
    }
}

fn toggle(mut ka: ResMut<KartAnywhere>, mut sim: ResMut<GameSim>, state: Res<AppState>, slot: Res<ExperimentSlot>, mut toast: ResMut<Toast>, mut rig: ResMut<crate::camera::rig::CameraRig>) {
    if !REQUEST.swap(false, Ordering::Relaxed) || !available(&state, &slot) {
        return;
    }
    match ka.ride.take() {
        Some(ride) => {
            let yaw = ride.yaw();
            ride.dismiss(&mut sim);
            rig.snap_behind(yaw);
        }
        None => match Ride::summon(&mut sim) {
            Ok(ride) => {
                ka.ride = Some(ride);
                ka.snap = true;
            }
            Err(why) => toast.show(why, 2.0),
        },
    }
}

fn publish(ka: Res<KartAnywhere>, state: Res<AppState>, slot: Res<ExperimentSlot>) {
    let offer = if !available(&state, &slot) {
        0
    } else if ka.driving() {
        2
    } else {
        1
    };
    OFFER.store(offer, Ordering::Relaxed);
}

/// The 30 Hz step while driving: the kart takes the player's controller and the simulation gets an empty one, so
/// nothing on foot (Mario's, Link's or Banjo's controller, their items and moves) sees a single input.
fn drive(mut ka: ResMut<KartAnywhere>, mut sim: ResMut<GameSim>, mut input: ResMut<PlayerInput>, mut control: ResMut<StepControl>, slot: Res<ExperimentSlot>, mut toast: ResMut<Toast>, mut rig: ResMut<crate::camera::rig::CameraRig>) {
    let Some(ride) = ka.ride.as_mut() else { return };
    if slot.active.is_some() || control.controller.is_some() || control.skip {
        return;
    }
    let c = input.consume_step();
    match ride.pre_tick(&mut sim, &c) {
        Tick::Driving => control.controller = Some(ControllerSnapshot::default()),
        Tick::Rescued => {
            control.controller = Some(ControllerSnapshot::default());
            toast.show("Back on solid ground", 1.5);
            ka.snap = true;
        }
        Tick::Ended(_) => {
            // The driver is on foot in whatever took him; this tick is his, with the controller just read.
            control.controller = Some(c);
            rig.snap_behind(sim.mario.face_angle[1]);
            ka.ride = None;
        }
    }
}

fn spawn_touch_button(mut commands: Commands) {
    commands
        .spawn((
            TouchButton,
            Visibility::Hidden,
            GlobalZIndex(41),
            Node { position_type: PositionType::Absolute, width: Val::Px(TOUCH_SIZE), height: Val::Px(TOUCH_SIZE), border: UiRect::all(Val::Px(2.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            BackgroundColor(Color::srgba(0.09, 0.14, 0.23, 0.74)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.6)),
            BorderRadius::MAX,
        ))
        .with_child((Text::new("KART"), TextFont { font_size: FontSize::Px(10.0), ..default() }, TextColor(Color::WHITE)));
}

fn update_touch_button(
    ka: Res<KartAnywhere>,
    state: Res<AppState>,
    tc: Res<crate::touch::TouchControls>,
    windows: Query<&Window, With<PrimaryWindow>>,
    area: Res<crate::safe_area::SafeArea>,
    mut button: Query<(&mut Visibility, &mut Node, &mut BorderColor), With<TouchButton>>,
) {
    let Ok((mut vis, mut node, mut border)) = button.single_mut() else { return };
    let show = tc.visible && *state == AppState::Playing && OFFER.load(Ordering::Relaxed) != 0;
    let want = if show { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if !show {
        return;
    }
    if let Ok(window) = windows.single() {
        let rect = touch_rect(window.size(), area.right, area.top);
        let (left, top) = (Val::Px(rect.min.x), Val::Px(rect.min.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
    }
    let rim = if ka.driving() { Color::srgba(1.0, 0.85, 0.3, 0.95) } else { Color::srgba(1.0, 1.0, 1.0, 0.55) };
    *border = BorderColor::all(rim);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kart_action_has_its_own_key_and_no_pad_button_by_default() {
        let b = Bindings::default();
        let mine = b.keys(Action::Kart).to_vec();
        assert_eq!(mine, vec![KeyCode::KeyB]);
        for a in Action::KEYBOARD.iter().filter(|a| **a != Action::Kart) {
            for k in b.keys(*a) {
                assert!(!mine.contains(k), "{:?} is also bound to {:?}", k, a.label());
            }
        }
        // Every pad button is taken by a character (Link's and Banjo's items use the D-pad and the shoulders), so
        // the action ships unbound on the pad; LB + RB together is the fixed combo.
        assert!(b.pad_buttons(Action::Kart).is_empty());
        assert!(Action::KEYBOARD.contains(&Action::Kart) && Action::PAD.contains(&Action::Kart));
        // Rebinding it to a pad button works like any other action.
        let mut b = Bindings::default();
        b.rebind_pad(Action::Kart, GamepadButton::DPadDown);
        assert_eq!(b.pad_buttons(Action::Kart), &[GamepadButton::DPadDown]);
        assert_eq!(Bindings::from_json(&b.to_json()).pad_buttons(Action::Kart), &[GamepadButton::DPadDown]);
    }

    #[test]
    fn the_kart_key_is_none_of_the_fixed_keys() {
        // Keys the game reads directly, outside the bindings table (`src/input.rs`, `zelda`, `audio`, the shell).
        let fixed = [KeyCode::Escape, KeyCode::Enter, KeyCode::Space, KeyCode::Tab, KeyCode::KeyH, KeyCode::KeyM, KeyCode::KeyX, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight];
        for k in Bindings::default().keys(Action::Kart) {
            assert!(!fixed.contains(k));
        }
    }

    #[test]
    fn the_touch_button_sits_beside_pause_without_overlapping_it() {
        let size = Vec2::new(844., 390.);
        let (kart, pause) = (touch_rect(size, 0., 0.), crate::touch::pause_rect(size, 0., 0.));
        assert!(kart.intersect(pause).is_empty());
        assert!(kart.min.x > size.x * 0.6 && kart.max.y < size.y * 0.4);
        // And it follows the safe area with it.
        let inset = touch_rect(size, 60., 0.);
        assert!(inset.max.x < kart.max.x);
    }
}
