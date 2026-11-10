//! A kart the player can call in any level they are walking in (SM64 courses, Ocarina scenes and dungeons,
//! Banjo-Kazooie levels): the active character sits in it and drives over the level's own collision with the MK64
//! kart physics, then steps out where the kart stands. Free driving only: no items, laps, opponents or race HUD.
//!
//! Call / leave: the `Kart` action (keyboard `B` by default, rebindable; a gamepad button if one is bound),
//! LB + RB together on a gamepad, the KART touch button beside pause, or the pause menu's CALL KART / LEAVE KART.
//! Driving uses the kart mode's controls: stick steers, stick up is the throttle, stick down or Z brakes and
//! reverses, A hops and drifts. The camera stick, mouse or a touch drag swings the chase camera, which eases back
//! behind the kart. What the character would pick up by touching it on foot is picked up from the seat.
//!
//! Hooks outside this module, each marked `kart_anywhere hook`: the pause list entry (`shell/retro/pages.rs`), its
//! action (`shell/screens.rs`), `Action::Kart` (`bindings.rs`), the plugin (`lib.rs`), the touch button's claim on
//! its taps (`touch.rs`) and Banjo's pickup overlap made public (`sim/src/banjo_ext.rs`). Everything else goes
//! through existing seams: `StepControl` (the simulation gets an empty controller while the kart has the real one),
//! `GameSim::skip_mario_step` (the on-foot controller does not run), `CameraScript` (the chase camera),
//! `fx::particles` (dust, sparks, splashes) and the kart mode's synthesised engine (`experiments::kart_audio`).
mod ride;
mod view;

use crate::bindings::{Action, Bindings};
use crate::button_marker::BorderRadius;
use crate::experiments::kart_audio;
use crate::experiments::kart_sound::{engine_sound, KartSounds, SoundFrame};
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
            .init_resource::<view::Look>()
            .add_systems(Startup, spawn_touch_button)
            .add_systems(
                FixedUpdate,
                drive.before(PreTickSet).before(crate::SimSet).run_if(crate::shell::is_playing).run_if(crate::progress::sim_running).run_if(|modal: Res<ModalActive>| !modal.0),
            )
            .add_systems(Update, (watch, read_inputs, toggle, publish, view::chase_camera, view::sync_kart, sound, update_touch_button).chain().before(crate::CameraSet).after(crate::sim_plugin::handle_reset));
        app.add_systems(PostUpdate, view::seat_driver.before(bevy::transform::TransformSystems::Propagate));
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
            // And say once a second where the character is, so a scripted run can be read back from its log.
            app.add_systems(Update, |time: Res<Time>, state: Res<AppState>, ka: Res<KartAnywhere>, sim: Res<GameSim>, mut last: Local<i32>| {
                let now = time.elapsed_secs() as i32;
                if now != *last {
                    *last = now;
                    let p = sim.mario.pos;
                    eprintln!("KARTLOG t={now} state={:?} course={} ride={} pos={:.0},{:.0},{:.0} action={:#x}", *state, sim.course.request.id, ka.ride.is_some(), p[0], p[1], p[2], sim.mario.action);
                }
            });
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
    mouse: Res<ButtonInput<MouseButton>>,
    mut tc: ResMut<crate::touch::TouchControls>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // Taps on the touch button are claimed by `touch::drive_touch`, so they are nothing else's.
    let tapped = std::mem::take(&mut tc.top_tapped);
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
    asked |= tapped;
    if let (Some(rect), Ok(window)) = (tc.top_extra, windows.single()) {
        asked |= tc.simulate && mouse.just_pressed(MouseButton::Left) && window.cursor_position().is_some_and(|p| rect.contains(p));
    }
    if asked {
        request_toggle();
    }
}

fn toggle(
    mut ka: ResMut<KartAnywhere>,
    mut sim: ResMut<GameSim>,
    state: Res<AppState>,
    slot: Res<ExperimentSlot>,
    who: Res<crate::characters::Character>,
    mut toast: ResMut<Toast>,
    mut rig: ResMut<crate::camera::rig::CameraRig>,
    mut particles: Option<ResMut<crate::fx::particles::Particles>>,
) {
    if !REQUEST.swap(false, Ordering::Relaxed) || !available(&state, &slot) {
        return;
    }
    let mut poof = |at| {
        if let Some(p) = particles.as_mut() {
            view::poof(p, at, view::kart_scale(*who));
        }
    };
    match ka.ride.take() {
        Some(ride) => {
            let yaw = ride.yaw();
            poof(ride.pos());
            ride.dismiss(&mut sim);
            rig.snap_behind(yaw);
        }
        None => match Ride::summon(&mut sim) {
            Ok(ride) => {
                poof(ride.pos());
                ka.ride = Some(ride);
                ka.snap = true;
            }
            Err(why) => toast.show(why, 2.0),
        },
    }
}

/// The engine note, the skid and the hop / drift / turbo / bump cues, on the kart mode's synthesiser. (Its real
/// MK64 sounds need the sound library the kart mode loads from the imported game, and are not used here.)
fn sound(ka: Res<KartAnywhere>, state: Res<AppState>, slot: Res<ExperimentSlot>, mut on: Local<bool>, mut cues: Local<KartSounds>, mut heard: Local<u32>) {
    let Some(ride) = &ka.ride else {
        // An experiment that starts owns the synthesiser from here; otherwise it goes quiet with the kart.
        if std::mem::take(&mut *on) && slot.active.is_none() && slot.pending.is_none() {
            kart_audio::set_active(false);
        }
        return;
    };
    if !*on {
        *on = true;
        cues.reset();
        kart_audio::set_active(true);
    }
    let frame = sound_frame(ride, *state == AppState::Playing);
    kart_audio::set_engine(engine_sound(&frame));
    if ride.ticks() != *heard {
        *heard = ride.ticks();
        for cue in cues.update(&frame, &[]) {
            kart_audio::play(cue);
        }
    }
}

fn sound_frame(ride: &Ride, playing: bool) -> SoundFrame {
    let k = &ride.kart;
    SoundFrame {
        hop: k.hop,
        drifting: k.drifting,
        drift_state: k.drift_state,
        mini_turbo: k.mini_turbo,
        bumped: k.bumped,
        midair: k.midair,
        speed_ratio: ride.speed_ratio(),
        accel: ride.accel,
        reversing: k.back_up,
        clock: ride.ticks(),
        // Paused: the engine is cut, not left droning under the menu.
        driving: playing,
        ..default()
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
    mut tc: ResMut<crate::touch::TouchControls>,
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
    let rect = windows.single().ok().filter(|_| show).map(|w| touch_rect(w.size(), area.right, area.top));
    if tc.top_extra != rect {
        tc.top_extra = rect;
    }
    if let Some(rect) = rect {
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

    /// A finger through the real touch system (`touch::drive_touch`): down at `at`, then dragged 60 px left.
    /// Returns (the tap was latched for the kart, buttons held, camera drag, pause toggles).
    fn tap_and_drag(at: Vec2, button: Option<Rect>) -> (bool, u32, Vec2, usize) {
        use bevy::input::touch::{touch_screen_input_system, TouchInput, TouchPhase};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource({
                let mut tc = crate::touch::TouchControls::default();
                tc.visible = true;
                tc.top_extra = button;
                tc
            })
            .init_resource::<crate::safe_area::SafeArea>()
            .init_resource::<crate::touch::ModeButtons>()
            .init_resource::<PlayerInput>()
            .init_resource::<Touches>()
            .insert_resource(AppState::Playing)
            .add_message::<TouchInput>()
            .add_message::<crate::shell::StateCmd>()
            .add_systems(Update, (touch_screen_input_system, crate::touch::drive_touch).chain());
        let window = app.world_mut().spawn((Window { resolution: (844, 390).into(), ..default() }, PrimaryWindow)).id();
        let mut send = |app: &mut App, phase, position| {
            app.world_mut().write_message(TouchInput { phase, position, window, force: None, id: 7 });
            app.update();
        };
        send(&mut app, TouchPhase::Started, at);
        send(&mut app, TouchPhase::Moved, at + Vec2::new(-60., 0.));
        let tc = app.world().resource::<crate::touch::TouchControls>();
        let drag = app.world().resource::<PlayerInput>().look_peek().0;
        let pauses = app.world().resource::<Messages<crate::shell::StateCmd>>().len();
        (tc.top_tapped, tc.buttons, drag, pauses)
    }

    #[test]
    fn a_tap_on_the_kart_button_is_only_a_tap_on_the_kart_button() {
        let size = Vec2::new(844., 390.);
        let rect = touch_rect(size, 0., 0.);
        // With the button shown, a finger on it is latched for the kart and is no camera drag, button or pause.
        let (tapped, buttons, drag, pauses) = tap_and_drag(rect.center(), Some(rect));
        assert!(tapped);
        assert_eq!((buttons, drag, pauses), (0, Vec2::ZERO, 0));
        // The same finger with the button hidden is an ordinary camera drag (so the claim is what stops it).
        let (tapped, buttons, drag, _) = tap_and_drag(rect.center(), None);
        assert!(!tapped && buttons == 0);
        assert_eq!(drag, Vec2::new(-60., 0.));
        // A finger just beside the shown button still drags the camera, and one on pause still pauses.
        let (tapped, _, drag, _) = tap_and_drag(rect.min - Vec2::new(30., 0.), Some(rect));
        assert!(!tapped);
        assert_eq!(drag, Vec2::new(-60., 0.));
        let (tapped, _, drag, pauses) = tap_and_drag(crate::touch::pause_rect(size, 0., 0.).center(), Some(rect));
        assert!(!tapped && drag == Vec2::ZERO && pauses == 1);
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
