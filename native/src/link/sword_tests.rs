//! The sword on a phone: a finger on the B button, through the real touch systems (`touch::drive_touch` and Link's C-button
//! layer), into the sim; and the sword from every state a player naturally swings it in (sim level, real course data).
use crate::characters::Character;
use crate::experiments::ExperimentSlot;
use crate::game::runner::GameSim;
use crate::game::world::CourseRequest;
use crate::input::PlayerInput;
use crate::link::ui::LinkMenu;
use crate::link::LinkInput;
use crate::safe_area::SafeArea;
use crate::shell::AppState;
use crate::touch::{self, TouchControls};
use bevy::input::touch::{touch_screen_input_system, TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use jumpman_sim::link::items::Age;
use jumpman_sim::link::moves::{Mv, C_LEFT};
use jumpman_sim::state::*;

fn assets() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn load(course: &str, age: Age) -> Option<GameSim> {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(course)).ok()?;
    sim.as_link = true;
    sim.mario.link.enable(age);
    for _ in 0..40 {
        sim.tick(&ControllerSnapshot::default());
    }
    Some(sim)
}

fn ctl(x: f64, y: f64, down: u32, pressed: u32) -> ControllerSnapshot {
    let mag = (x * x + y * y).sqrt().min(64.0);
    ControllerSnapshot { stick_x: x, stick_y: y, stick_mag: mag, button_down: down | pressed, button_pressed: pressed, ..Default::default() }
}

/// Phone sizes (logical px) with their safe areas (left, right, bottom): iPhone landscape, small and large, and portrait.
const PHONES: [(f32, f32, [f32; 3]); 5] = [(874.0, 402.0, [59.0, 59.0, 21.0]), (667.0, 375.0, [0.0, 0.0, 0.0]), (844.0, 390.0, [47.0, 47.0, 21.0]), (932.0, 430.0, [59.0, 59.0, 21.0]), (390.0, 844.0, [0.0, 0.0, 34.0])];

/// The app's touch chain for Link, with the window of a phone.
fn phone(w: f32, h: f32, inset: [f32; 3]) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource({
            let mut tc = TouchControls::default();
            tc.visible = true;
            tc
        })
        .insert_resource(SafeArea { left: inset[0], right: inset[1], bottom: inset[2], top: 0.0 })
        .init_resource::<touch::ModeButtons>()
        .init_resource::<PlayerInput>()
        .init_resource::<LinkInput>()
        .init_resource::<LinkMenu>()
        .init_resource::<ExperimentSlot>()
        .insert_resource(Character::Link)
        .init_resource::<Touches>()
        .insert_resource(AppState::Playing)
        .add_message::<TouchInput>()
        .add_message::<crate::shell::StateCmd>()
        .add_systems(Update, (touch_screen_input_system, touch::drive_touch, crate::link::ui::touch_buttons).chain());
    let window = app.world_mut().spawn((Window { resolution: (w as u32, h as u32).into(), ..default() }, PrimaryWindow)).id();
    (app, window)
}

fn finger(app: &mut App, window: Entity, phase: TouchPhase, at: Vec2, id: u64) {
    app.world_mut().write_message(TouchInput { phase, position: at, window, force: None, id });
    app.update();
}

fn rect_of(size: Vec2, inset: [f32; 3], c: touch::Control) -> Rect {
    touch::layout(size, Vec3::from(inset)).into_iter().find(|(k, _)| *k == c).unwrap().1
}

/// A tap on the B button at every phone size reaches the sim as Link's B press (no C button, pause, KART or camera drag claims it),
/// and the sim starts the sword swing from it.
#[test]
fn a_tap_on_b_swings_the_sword_at_every_phone_size() {
    use crate::bindings::Action;
    for (w, h, inset) in PHONES {
        let size = Vec2::new(w, h);
        let b = rect_of(size, inset, touch::Control::Button(Action::Punch)).center();
        for age in [Age::Adult, Age::Child] {
            let (mut app, window) = phone(w, h, inset);
            finger(&mut app, window, TouchPhase::Started, b, 3);
            let c = app.world_mut().resource_mut::<PlayerInput>().consume_step();
            assert!(c.button_pressed & B_BUTTON != 0, "{w}x{h}: the tap on B {b:?} was swallowed (down {:#x})", c.button_down);
            assert_eq!(c.button_pressed & (A_BUTTON | Z_TRIG), 0, "{w}x{h}: only B is pressed");
            assert_eq!(app.world().resource::<LinkInput>().c_pressed, 0, "{w}x{h}: a C button took the B tap");
            finger(&mut app, window, TouchPhase::Ended, b, 3);
            let tc = app.world().resource::<TouchControls>();
            assert!(tc.owners_empty(), "{w}x{h}: the finger is released");
            // ... and the sim swings from exactly that snapshot (the pre_tick path hands it over unchanged).
            let Some(mut sim) = load("kokiri_forest", age) else { continue };
            sim.tick(&c);
            assert_eq!(sim.mario.link.mv, Mv::Swing, "{w}x{h} {age:?}: B did not start a swing");
        }
    }
}

/// Taps and stick together, like a thumb on each side: the stick finger never steals the B tap, and a second B tap right after
/// the first (the combo) still registers (the first finger is released, so B is not stuck "held").
#[test]
fn b_works_while_the_stick_is_held_and_on_every_tap() {
    use crate::bindings::Action;
    for (w, h, inset) in PHONES {
        let size = Vec2::new(w, h);
        let (stick, b) = (rect_of(size, inset, touch::Control::Stick).center(), rect_of(size, inset, touch::Control::Button(Action::Punch)).center());
        let (mut app, window) = phone(w, h, inset);
        finger(&mut app, window, TouchPhase::Started, stick, 1);
        finger(&mut app, window, TouchPhase::Moved, stick + Vec2::new(0.0, -40.0), 1);
        for tap in 0..4 {
            let id = 10 + tap;
            finger(&mut app, window, TouchPhase::Started, b, id);
            let c = app.world_mut().resource_mut::<PlayerInput>().consume_step();
            assert!(c.button_pressed & B_BUTTON != 0, "{w}x{h}: tap {tap} with the stick held was lost");
            assert!(c.stick_mag > 20.0, "{w}x{h}: the stick finger is still steering");
            finger(&mut app, window, TouchPhase::Ended, b, id);
        }
        // A tap so quick that down and up land in one frame still counts.
        app.world_mut().write_message(TouchInput { phase: TouchPhase::Started, position: b, window, force: None, id: 99 });
        app.world_mut().write_message(TouchInput { phase: TouchPhase::Ended, position: b, window, force: None, id: 99 });
        app.update();
        let c = app.world_mut().resource_mut::<PlayerInput>().consume_step();
        assert!(c.button_pressed & B_BUTTON != 0, "{w}x{h}: a same-frame tap was lost");
        assert_eq!(c.button_down & B_BUTTON, c.button_pressed & B_BUTTON);
    }
}

/// The C buttons sit next to B but never take its tap, and B never takes theirs.
#[test]
fn the_c_buttons_and_b_do_not_steal_each_others_taps() {
    for (w, h, inset) in PHONES {
        let size = Vec2::new(w, h);
        let rects = crate::link::ui::button_rects(size, Vec3::from(inset));
        let c_left = rects.iter().find(|(b, _)| *b == C_LEFT).unwrap().1.center();
        let (mut app, window) = phone(w, h, inset);
        finger(&mut app, window, TouchPhase::Started, c_left, 5);
        let c = app.world_mut().resource_mut::<PlayerInput>().consume_step();
        assert_eq!(c.button_pressed & B_BUTTON, 0, "{w}x{h}: C-left pressed B");
        assert_eq!(app.world().resource::<LinkInput>().c_pressed & C_LEFT, C_LEFT, "{w}x{h}: C-left tap");
    }
}

/// Every touch control the game can claim a finger with stays clear of the B button, with the iOS margins too.
#[test]
fn nothing_overlaps_the_b_button() {
    use crate::bindings::Action;
    for (w, h, inset) in PHONES {
        let size = Vec2::new(w, h);
        let inset3 = Vec3::from(inset);
        let b = rect_of(size, inset, touch::Control::Button(Action::Punch));
        let mut others: Vec<(String, Rect)> = vec![("pause".into(), touch::pause_rect(size, inset[1], 0.0))];
        others.push(("kart".into(), crate::kart_anywhere::touch_rect(size, inset[1], 0.0)));
        for (bit, r) in crate::link::ui::button_rects(size, inset3) {
            others.push((format!("link {bit:#x}"), r));
        }
        for (c, r) in touch::layout(size, inset3).into_iter().chain(touch::mode_layout(size, inset3)) {
            if c != touch::Control::Button(Action::Punch) {
                others.push((format!("{c:?}"), r));
            }
        }
        for (name, r) in others {
            assert!(b.intersect(r).is_empty(), "{w}x{h}: {name} {r:?} overlaps B {b:?}");
        }
    }
}

/// The sword from every state a player naturally swings it in (walk, release, land): B always starts a swing.
#[test]
fn b_swings_from_standing_running_decelerating_and_landing() {
    for course in ["bob", "ccm", "wf", "kokiri_forest"] {
        for age in [Age::Adult, Age::Child] {
            for k in [0, 2, 5, 10, 20, 45] {
                let Some(mut sim) = load(course, age) else { return };
                for _ in 0..60 {
                    sim.tick(&ctl(0.0, 64.0, 0, 0));
                }
                for _ in 0..k {
                    sim.tick(&ctl(0.0, 0.0, 0, 0));
                }
                let before = sim.mario.action;
                if before & (ACT_FLAG_INVULNERABLE | ACT_FLAG_AIR) != 0 {
                    continue; // a goomba on the way knocked him back: that is not the state under test
                }
                sim.tick(&ctl(0.0, 0.0, 0, B_BUTTON));
                assert_eq!(sim.mario.link.mv, Mv::Swing, "{course} {age:?}: B {k} ticks after the stick was released, from {before:#x}");
            }
            // B while the stick is still held.
            let Some(mut sim) = load(course, age) else { return };
            for _ in 0..60 {
                sim.tick(&ctl(0.0, 64.0, 0, 0));
            }
            if sim.mario.action & (ACT_FLAG_INVULNERABLE | ACT_FLAG_AIR) == 0 {
                sim.tick(&ctl(0.0, 64.0, 0, B_BUTTON));
                assert_eq!(sim.mario.link.mv, Mv::Swing, "{course} {age:?}: B while running");
            }
            // Right after a jump lands.
            for k in [0, 1, 3, 6, 10, 20] {
                let Some(mut sim) = load(course, age) else { return };
                sim.tick(&ctl(0.0, 0.0, 0, A_BUTTON));
                for _ in 0..200 {
                    sim.tick(&ctl(0.0, 0.0, 0, 0));
                    if sim.mario.action & ACT_FLAG_AIR == 0 {
                        break;
                    }
                }
                for _ in 0..k {
                    sim.tick(&ctl(0.0, 0.0, 0, 0));
                }
                sim.tick(&ctl(0.0, 0.0, 0, B_BUTTON));
                assert_eq!(sim.mario.link.mv, Mv::Swing, "{course} {age:?}: B {k} ticks after landing");
            }
        }
    }
}

#[test]
#[ignore]
fn print_enemy_spots() {
    for course in ["bob", "ccm", "wf"] {
        let Some(sim) = load(course, Age::Adult) else { continue };
        println!("{course} spawn {:?}", sim.mario.pos.map(|v| v.round()));
        for o in &sim.mario.objects.objects {
            if o.active && o.hitbox.interact_type != 0 {
                let d = ((o.pos[0] - sim.mario.pos[0]).powi(2) + (o.pos[2] - sim.mario.pos[2]).powi(2)).sqrt();
                if d < 3500.0 {
                    println!("  {} at {:?} dist {:.0} hp r{} h{}", o.behavior.as_str(), o.pos.map(|v| v.round()), d, o.hitbox.radius, o.hitbox.height);
                }
            }
        }
    }
}

/// A goomba knocks Link back and the player taps B a moment before he can move again: the swing is not lost.
#[test]
fn a_b_tap_just_before_the_knock_back_ends_still_swings() {
    for age in [Age::Adult, Age::Child] {
        let knock = |sim: &mut GameSim| {
            sim.mario.set_action(ACT_BACKWARD_GROUND_KB, 0.0);
            sim.mario.forward_vel = -10.0;
        };
        // How long the knock-back lasts.
        let Some(mut sim) = load("bob", age) else { return };
        knock(&mut sim);
        let mut len = 0;
        while sim.mario.action == ACT_BACKWARD_GROUND_KB && len < 200 {
            sim.tick(&ctl(0.0, 0.0, 0, 0));
            len += 1;
        }
        assert!(len > 6 && len < 200, "{age:?}: knock-back lasts {len} ticks");
        // Press B 4 ticks before it ends.
        let Some(mut sim) = load("bob", age) else { return };
        knock(&mut sim);
        let mut swung = false;
        for t in 0..len + 12 {
            sim.tick(&ctl(0.0, 0.0, 0, if t == len - 4 { B_BUTTON } else { 0 }));
            swung |= sim.mario.link.mv == Mv::Swing;
        }
        assert!(swung, "{age:?}: a B tap 4 ticks before the knock-back ended was thrown away");
        // A press long before it is not kept for ever.
        let Some(mut sim) = load("bob", age) else { return };
        knock(&mut sim);
        let mut swung = false;
        for t in 0..len + 12 {
            sim.tick(&ctl(0.0, 0.0, 0, if t == 1 { B_BUTTON } else { 0 }));
            swung |= sim.mario.link.mv == Mv::Swing;
        }
        assert!(!swung, "{age:?}: a stale press from the start of the knock-back swung later");
    }
}

/// A real finger tapping B at human cadences, at 60 and 120 Hz frames against the 30 Hz sim, through the touch layer and the
/// latch (`PlayerInput::consume_step`): every cadence from 150 to 600 ms gives three swings with the third one the finisher, for
/// both Links, standing and with the stick held. (Before the combo count outlived 400 ms and a press at the first ticks of a swing was
/// kept, 400 ms and slower taps never reached the finisher.)
#[test]
fn a_finger_tapping_b_at_human_cadence_reaches_the_finisher_through_the_touch_layer() {
    use crate::bindings::Action;
    use jumpman_sim::link::combat::Swing;
    use jumpman_sim::link::moves::LinkEvent;
    let (w, h, inset) = PHONES[0];
    let size = Vec2::new(w, h);
    let (stick, b) = (rect_of(size, inset, touch::Control::Stick).center(), rect_of(size, inset, touch::Control::Button(Action::Punch)).center());
    for age in [Age::Adult, Age::Child] {
        for held_stick in [false, true] {
            for frame_hz in [60.0, 120.0] {
                for (interval, hold) in [(150.0, 60.0), (250.0, 100.0), (300.0, 60.0), (450.0, 100.0), (600.0, 60.0)] {
                    let Some(mut sim) = load("kokiri_forest", age) else { return };
                    let (mut app, window) = phone(w, h, inset);
                    if held_stick {
                        finger(&mut app, window, TouchPhase::Started, stick, 1);
                        finger(&mut app, window, TouchPhase::Moved, stick + Vec2::new(0.0, -40.0), 1);
                    }
                    let starts: Vec<f64> = (0..3).map(|i| 200.0 + i as f64 * interval).collect();
                    let dt = 1000.0 / frame_hz;
                    let mut swings = vec![];
                    let (mut ms, mut next_tick, mut down_id) = (0.0f64, 1000.0 / 30.0, None);
                    while ms < 4000.0 {
                        ms += dt;
                        if let Some((id, until)) = down_id {
                            if ms >= until {
                                app.world_mut().write_message(TouchInput { phase: TouchPhase::Ended, position: b, window, force: None, id });
                                down_id = None;
                            }
                        }
                        if down_id.is_none() {
                            if let Some(i) = starts.iter().position(|s| *s <= ms && ms < s + dt) {
                                app.world_mut().write_message(TouchInput { phase: TouchPhase::Started, position: b, window, force: None, id: 40 + i as u64 });
                                down_id = Some((40 + i as u64, starts[i] + hold));
                            }
                        }
                        // `input::read_input` rebuilds the held state every frame before the touch layer adds its own.
                        app.world_mut().resource_mut::<PlayerInput>().down = 0;
                        app.update();
                        while next_tick <= ms {
                            next_tick += 1000.0 / 30.0;
                            let c = app.world_mut().resource_mut::<PlayerInput>().consume_step();
                            sim.tick(&c);
                            for e in sim.mario.link.take_events() {
                                if let LinkEvent::Swing(s) = e {
                                    swings.push(s);
                                }
                            }
                        }
                    }
                    assert_eq!(swings.len(), 3, "{age:?} stick {held_stick} {frame_hz} Hz: B every {interval} ms held {hold} ms swung {swings:?}");
                    assert!(matches!(swings[2], Swing::Combo(_)), "{age:?} stick {held_stick} {frame_hz} Hz: B every {interval} ms held {hold} ms: {swings:?}");
                }
            }
        }
    }
}

/// A thumb that rests on a C button and slides (the bow's "hold, drag to aim, release to fire"): the C bit stays down, the slide
/// is handed to the aim as `claim_drag`, the camera never turns from it, and the other thumb on the stick is not disturbed.
#[test]
fn a_thumb_sliding_on_a_c_button_aims_and_does_not_turn_the_camera() {
    for (w, h, inset) in PHONES.into_iter().take(4) {
        let size = Vec2::new(w, h);
        let rects = crate::link::ui::button_rects(size, Vec3::from(inset));
        let c_left = rects.iter().find(|(b, _)| *b == C_LEFT).unwrap().1.center();
        let stick = rect_of(size, inset, touch::Control::Stick).center();
        let (mut app, window) = phone(w, h, inset);
        finger(&mut app, window, TouchPhase::Started, stick, 1);
        finger(&mut app, window, TouchPhase::Moved, stick + Vec2::new(0.0, -40.0), 1);
        finger(&mut app, window, TouchPhase::Started, c_left, 2);
        assert_eq!(app.world().resource::<LinkInput>().c_pressed & C_LEFT, C_LEFT, "{w}x{h}: the press edge");
        assert_eq!(app.world().resource::<LinkInput>().c_down & C_LEFT, C_LEFT, "{w}x{h}: held");
        // Slide 30 px left and 12 px up while holding.
        finger(&mut app, window, TouchPhase::Moved, c_left + Vec2::new(-30.0, -12.0), 2);
        let li = app.world().resource::<LinkInput>();
        assert!((li.claim_drag - Vec2::new(-30.0, -12.0)).length() < 1e-3, "{w}x{h}: the slide is the aim drag, got {:?}", li.claim_drag);
        assert_eq!(li.c_down & C_LEFT, C_LEFT, "{w}x{h}: still held after sliding");
        let look = app.world().resource::<PlayerInput>().look_peek();
        assert_eq!(look.0, Vec2::ZERO, "{w}x{h}: the C finger is not a camera drag");
        assert!(app.world().resource::<PlayerInput>().stick_mag > 20.0, "{w}x{h}: the stick finger keeps steering");
        finger(&mut app, window, TouchPhase::Ended, c_left + Vec2::new(-30.0, -12.0), 2);
        // (`c_down` itself is rebuilt each frame by `read_link_keys`, which this harness does not run: look at the touch bits.)
        assert_eq!(app.world().resource::<LinkInput>().touch_bits & C_LEFT, 0, "{w}x{h}: released");
    }
}
