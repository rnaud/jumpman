//! Human-finger B taps against Link's sword state machine: a tap lasts 30-150 ms and consecutive taps come every 100-800 ms, at
//! any phase against the 30 Hz tick. The 3-hit finisher must come out of ordinary tapping, not only out of idealised 1-tick presses.
use jumpman_sim::collision::*;
use jumpman_sim::link::combat::*;
use jumpman_sim::link::items::*;
use jumpman_sim::link::moves::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::object::*;
use jumpman_sim::object_manager::ObjectManager;
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
    w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
    w
}

fn link(age: Age) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -9000.0);
    m.set_objects(ObjectManager::new(Vec::<ObjectSpec>::new(), None, None));
    m.link.enable(age);
    m
}

pub struct Run {
    pub swings: Vec<Swing>,
}

impl Run {
    pub fn hits(&self) -> usize {
        self.swings.len()
    }
    pub fn finisher(&self) -> bool {
        self.swings.iter().any(|s| matches!(s, Swing::Combo(_)))
    }
}

/// `taps` presses `interval_ms` apart, each held `dur_ms`, the first at `phase_ms` after 300 ms. The edge latches like
/// `PlayerInput::consume_step`: a press made between two ticks is seen by the next tick, even if it was already released.
pub fn run(age: Age, interval_ms: f64, dur_ms: f64, taps: usize, phase_ms: f64, z: bool, stick: bool) -> Run {
    let mut m = link(age);
    let dt = 1000.0 / 30.0;
    let starts: Vec<f64> = (0..taps).map(|i| 300.0 + phase_ms + i as f64 * interval_ms).collect();
    let mut swings = vec![];
    let mut next_edge = 0;
    for k in 0..300 {
        let (t0, t1) = (k as f64 * dt, (k + 1) as f64 * dt);
        let mut pressed = 0;
        while next_edge < starts.len() && starts[next_edge] < t1 {
            pressed = B_BUTTON;
            next_edge += 1;
        }
        let held = starts.iter().any(|s| *s < t1 && s + dur_ms > t0);
        let down = if held || pressed != 0 { B_BUTTON } else { 0 };
        let (y, mag) = if stick { (64.0, 64.0) } else { (0.0, 0.0) };
        let c = ControllerSnapshot { focus_yaw: z.then_some(0.0), stick_x: 0.0, stick_y: y, stick_mag: mag, button_down: down | pressed, button_pressed: pressed };
        m.step(&c, 32768.0, true, false);
        for e in m.link.take_events() {
            if let LinkEvent::Swing(s) = e {
                swings.push(s);
            }
        }
    }
    Run { swings }
}

/// `JUMPMAN_COMBO_MATRIX=1 cargo test ... print_the_tap_matrix -- --nocapture` prints hits/finisher per cadence.
#[test]
fn print_the_tap_matrix() {
    if std::env::var("JUMPMAN_COMBO_MATRIX").is_err() {
        return;
    }
    for age in [Age::Adult, Age::Child] {
        for (z, stick) in [(false, false), (true, false), (false, true), (true, true)] {
            println!("== {age:?} z={z} stick={stick}: worst case over 4 phases x 4 durations, 3 taps");
            for iv in [100.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0, 600.0, 800.0] {
                let mut worst = (9, true);
                for dur in [30.0, 60.0, 100.0, 150.0] {
                    for ph in [0.0, 9.0, 18.0, 27.0] {
                        let r = run(age, iv, dur, 3, ph, z, stick);
                        worst = (worst.0.min(r.hits()), worst.1 && r.finisher());
                    }
                }
                println!("  {iv:>4} ms: {} swings, finisher {}", worst.0, worst.1);
            }
        }
    }
}

/// Taps a finger makes at any ordinary cadence (150-600 ms apart, held 30-100 ms, at any phase against the 30 Hz tick, standing,
/// walking, Z-targeting) chain into the finisher. Before the combo count was kept for a finger's rhythm, only 150/250/300 ms worked.
#[test]
fn finger_taps_at_human_cadence_give_the_three_hit_finisher() {
    for age in [Age::Adult, Age::Child] {
        for (z, stick) in [(false, false), (true, false), (false, true), (true, true)] {
            for iv in [150.0, 200.0, 250.0, 300.0, 400.0, 500.0, 600.0] {
                for dur in [30.0, 60.0, 100.0] {
                    for ph in [0.0, 7.0, 14.0, 21.0, 28.0] {
                        let r = run(age, iv, dur, 3, ph, z, stick);
                        assert_eq!(r.hits(), 3, "{age:?} z={z} stick={stick} every {iv} ms held {dur} ms phase {ph}: {:?}", r.swings);
                        assert!(r.finisher(), "{age:?} z={z} stick={stick} every {iv} ms held {dur} ms phase {ph}: {:?}", r.swings);
                    }
                }
            }
        }
    }
}

/// A press on the very first ticks of a swing, or on the tick it ends, is not lost; a double-tap still gives at least two hits.
#[test]
fn fast_taps_are_not_dropped_at_the_swing_boundaries() {
    for age in [Age::Adult, Age::Child] {
        for ph in [0.0, 10.0, 20.0, 30.0] {
            let r = run(age, 100.0, 30.0, 3, ph, false, false);
            assert!(r.hits() >= 2, "{age:?} phase {ph}: {:?}", r.swings);
        }
    }
}

/// A pause longer than the combo count's life starts the chain over (taps 1.2 s apart are three single slashes).
#[test]
fn slow_taps_do_not_chain() {
    let r = run(Age::Adult, 1200.0, 60.0, 3, 0.0, false, false);
    assert_eq!(r.hits(), 3);
    assert!(!r.finisher(), "{:?}", r.swings);
}
