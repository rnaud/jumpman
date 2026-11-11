//! Link's three-hit sword combo on real SM64 courses: B taps at human cadences against the course's own enemies, through the
//! same sim step the app uses (the touch layer itself is covered in `sword_tests`).
use super::mario_worlds::{ctl, face, load_age};
use crate::game::runner::GameSim;
use jumpman_sim::link::combat::{Direction, Swing};
use jumpman_sim::link::items::Age;
use jumpman_sim::link::moves::LinkEvent;
use jumpman_sim::object::{ObjId, ObjectBehaviorKind};
use jumpman_sim::state::*;

/// How Link stands while tapping: Z-locked on the target, and the stick (relative to the facing).
#[derive(Clone, Copy, Default)]
pub(super) struct Mode {
    pub z: bool,
    /// Stick held at this angle from Link's facing at the start of the run, in s16 units (0 ahead, 0x4000 left, 0x8000 back, 0xc000 right).
    pub rel: Option<i32>,
    /// Link starts with his back to the target (spin attacks cut all round).
    pub turn_away: bool,
    /// Press A on this tick (a jump, for the jump slash).
    pub a_at: Option<usize>,
}

/// What one run of taps did.
#[derive(Debug, Default, Clone)]
pub(super) struct Report {
    /// (tick, swing) for each swing that began.
    pub swings: Vec<(usize, Swing)>,
    /// (tick, swing index, object id) for each blade hit event.
    pub hits: Vec<(usize, usize, u32)>,
    /// Link's own damage events (the enemy hurt him).
    pub link_health_lost: f64,
    /// Blade hits on objects other than the target.
    pub other_hits: usize,
    pub dist0: f64,
    pub struck: u32,
    pub health_var: f64,
    pub alive: bool,
    pub moved: f64,
    pub enemy_moved: f64,
    pub trace: Vec<String>,
}

impl Report {
    pub fn finisher(&self) -> bool {
        self.swings.last().is_some_and(|(_, s)| matches!(s, Swing::Combo(_)))
    }
    /// Hits on `id` per swing index.
    pub fn hits_on(&self, id: u32) -> usize {
        self.hits.iter().filter(|h| h.2 == id).count()
    }
}

/// `taps` B presses `interval_ms` apart, each held `hold_ms`, the first at 200 ms + `phase_ms`, then `tail` more ticks. The press edge latches
/// like `PlayerInput::consume_step`: a press made between two ticks is seen by the next tick even if released already.
pub(super) fn tap_run(sim: &mut GameSim, id: ObjId, interval_ms: f64, hold_ms: f64, taps: usize, phase_ms: f64, tail: usize, mode: Mode) -> Report {
    let dt = 1000.0 / 30.0;
    let starts: Vec<f64> = (0..taps).map(|i| 200.0 + phase_ms + i as f64 * interval_ms).collect();
    let end_ticks = ((starts.last().copied().unwrap_or(0.0) + hold_ms) / dt) as usize + tail;
    let mut r = Report::default();
    let start = sim.mario.pos;
    let estart = sim.mario.objects[id].pos;
    let hp0 = sim.mario.health;
    r.dist0 = ((start[0] - estart[0]).powi(2) + (start[2] - estart[2]).powi(2)).sqrt();
    if mode.turn_away {
        sim.mario.face_angle[1] += 32768.0;
    }
    let face0 = sim.mario.face_angle[1];
    let trace = std::env::var_os("LM_TRACE").is_some();
    sim.mario.link.take_events();
    let mut next_edge = 0;
    for k in 0..end_ticks {
        let (t0, t1) = (k as f64 * dt, (k + 1) as f64 * dt);
        let mut pressed = 0;
        while next_edge < starts.len() && starts[next_edge] < t1 {
            pressed = B_BUTTON;
            next_edge += 1;
        }
        let held = starts.iter().any(|s| *s < t1 && s + hold_ms > t0);
        let down = if held || pressed != 0 { B_BUTTON } else { 0 };
        let (sx, sy) = match mode.rel {
            // `intended_yaw = atan2s(-stick_y, stick_x) + camera_yaw`: aim the stick at (facing at the start + rel).
            Some(rel) => {
                let th = (face0 + rel as f64 - sim.mario.camera_yaw) * std::f64::consts::PI / 32768.0;
                (64.0 * th.sin(), -64.0 * th.cos())
            }
            None => (0.0, 0.0),
        };
        let a = if mode.a_at == Some(k) { A_BUTTON } else { 0 };
        let a_down = if mode.a_at.is_some_and(|t| k >= t && k < t + 12) { A_BUTTON } else { 0 };
        let mut c = ctl(sx, sy, down | a_down, pressed | a);
        if mode.z {
            // Z held: Link faces the target, the stick is read relative to that.
            let (m, o) = (&sim.mario, &sim.mario.objects[id]);
            c.focus_yaw = Some(jumpman_sim::math::atan2s(o.pos[2] - m.pos[2], o.pos[0] - m.pos[0]) as f64);
            sim.focus_held = true;
        }
        sim.tick(&c);
        for e in sim.mario.link.take_events() {
            match e {
                LinkEvent::Swing(s) => r.swings.push((k, s)),
                LinkEvent::HitObject { id: o, .. } => {
                    if o == id.0 {
                        r.hits.push((k, r.swings.len().saturating_sub(1), o))
                    } else {
                        r.other_hits += 1;
                    }
                }
                _ => {}
            }
        }
        if trace {
            let m = &sim.mario;
            let o = &m.objects[id];
            eprintln!("t{k:3} {} flags {:#x} swing {:?} act {:#x}/{} mv {:?} pos {:?} hp {} | {} {:?} act {} struck {} hv {} act? {}", if down != 0 { "B" } else { "." }, m.flags & (MARIO_PUNCHING | MARIO_KICKING), m.link.swing, m.action, m.action_arg, m.link.mv, m.pos.map(|v| v.round()), m.health, o.behavior.as_str(), o.pos.map(|v| v.round()), o.action, o.struck, o.var("health"), o.active);
        }
    }
    let m = &sim.mario;
    let o = &m.objects[id];
    r.struck = o.struck;
    r.health_var = o.var("health");
    r.alive = o.active;
    r.link_health_lost = (hp0 - m.health).max(0.0);
    r.moved = ((m.pos[0] - start[0]).powi(2) + (m.pos[2] - start[2]).powi(2)).sqrt();
    r.enemy_moved = ((o.pos[0] - estart[0]).powi(2) + (o.pos[2] - estart[2]).powi(2)).sqrt();
    r
}

/// `LM_COURSE=ssl LM_KIND=pokey LM_IV=150 LM_BACK=170 LM_TRACE=1 ... trace_one`: per-tick trace of one run.
#[test]
#[ignore]
fn trace_one() {
    let course = std::env::var("LM_COURSE").unwrap_or("ssl".into());
    let kind = std::env::var("LM_KIND").unwrap_or("pokey".into());
    let iv: f64 = std::env::var("LM_IV").ok().and_then(|v| v.parse().ok()).unwrap_or(150.0);
    let back: f64 = std::env::var("LM_BACK").ok().and_then(|v| v.parse().ok()).unwrap_or(170.0);
    let taps: usize = std::env::var("LM_TAPS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let kind = ObjectBehaviorKind::ALL.iter().copied().find(|k| k.as_str() == kind).expect("kind");
    let Some(mut sim) = load_age(&course, Age::Adult) else { return };
    let id = face(&mut sim, kind, back).expect("enemy");
    let rel = std::env::var("LM_REL").ok().and_then(|v| v.parse::<i32>().ok());
    let a_at = std::env::var("LM_A").ok().and_then(|v| v.parse::<usize>().ok());
    let mode = Mode { z: std::env::var_os("LM_Z").is_some(), rel, a_at, ..Default::default() };
    let r = tap_run(&mut sim, id, iv, 100.0, taps, 0.0, 40, mode);
    eprintln!("{r:?}");
}

/// Z modes, spin, jump slash, goombas, boxes, friends and every object kind: prints what each does.
#[test]
#[ignore]
fn explore_modes() {
    use ObjectBehaviorKind as K;
    let cases: &[(&str, K, f64)] = &[("bbh", K::BigBoo, 250.0), ("ssl", K::Pokey, 200.0), ("bob", K::Bobomb, 130.0), ("wf", K::Whomp, 200.0), ("lll", K::BigBully, 300.0)];
    let modes: &[(&str, Mode, f64, usize)] = &[
        ("free     ", Mode::default(), 300.0, 3),
        ("z        ", Mode { z: true, ..Default::default() }, 300.0, 3),
        ("z stab   ", Mode { z: true, rel: Some(0), ..Default::default() }, 300.0, 3),
        ("z left   ", Mode { z: true, rel: Some(0x4000), ..Default::default() }, 300.0, 3),
        ("z right  ", Mode { z: true, rel: Some(0xc000), ..Default::default() }, 300.0, 3),
        ("z back   ", Mode { z: true, rel: Some(0x8000), ..Default::default() }, 300.0, 3),
        ("spin     ", Mode::default(), 1200.0, 1),
        ("jump     ", Mode { z: true, a_at: Some(1), rel: Some(0), ..Default::default() }, 300.0, 1),
        ("spin behind", Mode { turn_away: true, ..Default::default() }, 1200.0, 1),
    ];
    for (course, kind, back) in cases {
        for age in [Age::Adult, Age::Child] {
            for (name, mode, iv, taps) in modes {
                let Some(mut sim) = load_age(course, age) else { continue };
                let Some(id) = face(&mut sim, *kind, *back) else { continue };
                let hv0 = sim.mario.objects[id].var("health");
                // spin: one long hold; jump: B 14 ticks after A (in the air)
                let (hold, start_phase) = if name.starts_with("spin") { (1000.0, 0.0) } else if name.starts_with("jump") { (100.0, 100.0) } else { (100.0, 0.0) };
                let r = tap_run(&mut sim, id, *iv, hold, *taps, start_phase, 50, *mode);
                let sw: Vec<String> = r.swings.iter().map(|(t, s)| format!("{t}:{s:?}")).collect();
                println!(
                    "{course:4} {:9} {age:?} {name} | swings {} | hits {:?} | struck {} hp {}->{} alive {} | link hurt {} moved {:.0} enemy moved {:.0} others {}",
                    kind.as_str(),
                    sw.join(" "),
                    r.hits.iter().map(|h| format!("{}:{}", h.0, h.1)).collect::<Vec<_>>(),
                    r.struck,
                    hv0,
                    r.health_var,
                    r.alive,
                    r.link_health_lost,
                    r.moved,
                    r.enemy_moved,
                    r.other_hits
                );
            }
        }
    }
}

/// `JUMPMAN_COMBO_EXPLORE=1 cargo test ... mario_combo::explore -- --ignored --nocapture`: three taps at a few cadences against each
/// SM64 enemy kind on its course, both Links. Prints what happened.
#[test]
#[ignore]
fn explore() {
    use ObjectBehaviorKind as K;
    let only = std::env::var("LM_KIND").ok();
    let cases: &[(&str, K, f64)] = &[
        ("bob", K::Bobomb, 130.0),
        ("bob", K::ChainChomp, 220.0),
        ("bob", K::BreakableBox, 230.0),
        ("bob", K::BreakableBoxSmall, 130.0),
        ("wf", K::Whomp, 200.0),
        ("wf", K::WhompKing, 300.0),
        ("wf", K::BreakableBoxSmall, 130.0),
        ("ccm", K::Spindrift, 160.0),
        ("ccm", K::MrBlizzard, 200.0),
        ("bbh", K::Boo, 150.0),
        ("bbh", K::BigBoo, 250.0),
        ("bbh", K::Scuttlebug, 190.0),
        ("lll", K::Bully, 150.0),
        ("lll", K::BigBully, 280.0),
        ("ssl", K::Pokey, 170.0),
        ("ssl", K::Bobomb, 130.0),
        ("ssl", K::Klepto, 200.0),
        ("hmc", K::Snufit, 200.0),
    ];
    for (course, kind, back) in cases {
        if only.as_deref().is_some_and(|o| o != kind.as_str()) {
            continue;
        }
        for age in [Age::Adult, Age::Child] {
            for (iv, hold) in [(150.0, 100.0), (300.0, 100.0), (450.0, 100.0)] {
                let Some(mut sim) = load_age(course, age) else { continue };
                let Some(id) = face(&mut sim, *kind, *back) else {
                    println!("{course} {kind:?}: none reachable");
                    break;
                };
                let hv0 = sim.mario.objects[id].var("health");
                let r = tap_run(&mut sim, id, iv, hold, 3, 0.0, 40, Mode::default());
                let sw: Vec<String> = r.swings.iter().map(|(t, s)| format!("{t}:{s:?}")).collect();
                println!(
                    "{course:4} {:12} {age:?} iv {iv:3} | swings {} fin {} | hits {:?} | struck {} hp {}->{} alive {} | link hurt {} moved {:.0} enemy moved {:.0} dist0 {:.0} other hits {}",
                    kind.as_str(),
                    sw.join(" "),
                    r.finisher(),
                    r.hits.iter().map(|h| format!("{}:{}", h.0, h.1)).collect::<Vec<_>>(),
                    r.struck,
                    hv0,
                    r.health_var,
                    r.alive,
                    r.link_health_lost,
                    r.moved,
                    r.enemy_moved,
                    r.dist0,
                    r.other_hits
                );
            }
        }
    }
}


// ---------------------------------------------------------------------------------------------------------------------
// Regression tests: real SM64 courses, human tap cadences.
// ---------------------------------------------------------------------------------------------------------------------

use jumpman_sim::link::items::Sword;

/// The multi-hit enemies of the SM64 courses: (course, kind, distance to stand from it). Each is kept out of Link's way for the
/// length of a combo, so the three blows are his to land (the heavy ones are pushed back by every blow).
const TOUGH: [(&str, ObjectBehaviorKind, f64); 4] = [
    ("bbh", ObjectBehaviorKind::BigBoo, 250.0),
    ("ssl", ObjectBehaviorKind::Pokey, 200.0),
    ("lll", ObjectBehaviorKind::BigBully, 300.0),
    ("wf", ObjectBehaviorKind::WhompKing, 320.0),
];

/// (interval, hold, phase) in ms: a finger's taps against the 30 Hz tick.
fn cadences() -> Vec<(f64, f64, f64)> {
    let mut v = vec![];
    for iv in [200.0, 250.0, 300.0, 400.0, 450.0] {
        for hold in [60.0, 100.0] {
            for ph in [0.0, 12.0, 24.0] {
                v.push((iv, hold, ph));
            }
        }
    }
    v
}

/// Three taps give three swings, the third the finisher, three blows on a Pokey / Big Boo / Big Bully / King Whomp (health 3, one point
/// per Kokiri or Master blow), one per swing, and the enemy is then defeated: both Links, every cadence. The King walks at Link at 3
/// units a tick, so for him the test looks for a standing distance (260-340) from which the blade reaches him all three times: the
/// cadences differ in when it must arrive, the swords in how far it reaches.
#[test]
fn three_taps_deal_three_blows_to_a_multi_health_sm64_enemy() {
    let mut ran = 0;
    for (course, kind, back) in TOUGH {
        let backs: Vec<f64> = if kind == ObjectBehaviorKind::WhompKing { (26..=34).map(|b| b as f64 * 10.0).collect() } else { vec![back] };
        for age in [Age::Adult, Age::Child] {
            for (iv, hold, ph) in cadences() {
                let mut last = String::new();
                let mut ok = false;
                for back in &backs {
                    let Some(mut sim) = load_age(course, age) else { return };
                    let Some(id) = face(&mut sim, kind, *back) else { panic!("no {kind:?} on {course}") };
                    assert_eq!(sim.mario.objects[id].var("health"), 3.0, "{kind:?} starts with 3 hit points");
                    let r = tap_run(&mut sim, id, iv, hold, 3, ph, 40, Mode::default());
                    last = format!("{course} {kind:?} {age:?} from {back} every {iv} ms held {hold} ms phase {ph}: {r:?}");
                    // Whatever the distance: three swings, the third the finisher, never two blows from one swing.
                    assert_eq!(r.swings.len(), 3, "{last}");
                    assert!(matches!(r.swings[0].1, Swing::Right) && matches!(r.swings[1].1, Swing::Right) && matches!(r.swings[2].1, Swing::Combo(Direction::Right)), "{last}");
                    for sw in 0..3 {
                        assert!(r.hits.iter().filter(|h| h.1 == sw).count() <= 1, "{last}");
                    }
                    if r.hits.len() == 3 {
                        assert_eq!(r.hits.iter().map(|h| h.1).collect::<Vec<_>>(), vec![0, 1, 2], "one blow per swing: {last}");
                        assert_eq!(r.struck, 3, "{last}");
                        assert!(!r.alive, "three blows defeat it: {last}");
                        assert_eq!(r.link_health_lost, 0.0, "{last}");
                        ok = true;
                        break;
                    }
                }
                assert!(ok, "never three blows: {last}");
                ran += 1;
            }
        }
    }
    assert!(ran > 0);
}

/// Biggoron's blade takes two points off per blow (OoT: 4 half-hearts against the Kokiri sword's 1), so two of three taps are enough.
#[test]
fn a_stronger_blade_needs_fewer_blows() {
    for (sword, blows) in [(Sword::Kokiri, 3), (Sword::Master, 3), (Sword::Biggoron, 2)] {
        let Some(mut sim) = load_age("ssl", Age::Adult) else { return };
        sim.mario.link.inv.sword = Some(sword);
        let id = face(&mut sim, ObjectBehaviorKind::Pokey, 200.0).expect("pokey");
        let r = tap_run(&mut sim, id, 300.0, 100.0, 3, 0.0, 40, Mode::default());
        assert_eq!(r.hits.len(), blows, "{sword:?}: {r:?}");
        assert!(!r.alive, "{sword:?}: {r:?}");
    }
}

/// A normal one-hit enemy (Bob-omb on Bob-omb Battlefield, Whomp, a Spindrift, a Scuttlebug) falls to the first blow; the combo goes on
/// swinging and the corpse is not struck again.
#[test]
fn three_taps_on_a_one_hit_enemy_swing_three_times_and_strike_once() {
    for (course, kind, back) in [("bob", ObjectBehaviorKind::Bobomb, 130.0), ("wf", ObjectBehaviorKind::Whomp, 200.0), ("ccm", ObjectBehaviorKind::Spindrift, 160.0), ("bbh", ObjectBehaviorKind::Scuttlebug, 190.0)] {
        for age in [Age::Adult, Age::Child] {
            for (iv, hold, ph) in cadences() {
                let Some(mut sim) = load_age(course, age) else { return };
                let id = face(&mut sim, kind, back).expect("enemy");
                let r = tap_run(&mut sim, id, iv, hold, 3, ph, 40, Mode::default());
                let what = format!("{course} {kind:?} {age:?} every {iv} ms held {hold} ms phase {ph}: {r:?}");
                assert_eq!(r.swings.len(), 3, "{what}");
                assert!(r.finisher(), "{what}");
                assert_eq!(r.hits.len(), 1, "{what}");
                assert_eq!(r.hits[0].1, 0, "the first swing does it: {what}");
                assert!(!r.alive, "{what}");
            }
        }
    }
}

/// Z-locked on a Big Boo: the forward slashes (Z, no stick) and the side slashes (stick left: Right, stick right: Left) each give the
/// three-blow combo with the matching finisher; the stab row (stick forward) swings Stab, Stab, Combo(Stab) on a Bob-omb.
#[test]
fn z_locked_combos_use_the_forward_side_and_stab_rows() {
    for age in [Age::Adult, Age::Child] {
        for (rel, first, fin) in [(None, Swing::Forward, Direction::Forward), (Some(0x4000), Swing::Right, Direction::Right), (Some(0xc000), Swing::Left, Direction::Left)] {
            let Some(mut sim) = load_age("bbh", age) else { return };
            let id = face(&mut sim, ObjectBehaviorKind::BigBoo, 250.0).expect("big boo");
            let r = tap_run(&mut sim, id, 300.0, 100.0, 3, 0.0, 40, Mode { z: true, rel, ..Default::default() });
            let kinds: Vec<Swing> = r.swings.iter().map(|s| s.1).collect();
            assert_eq!(kinds, vec![first, first, Swing::Combo(fin)], "{age:?} rel {rel:?}: {r:?}");
            assert_eq!(r.hits.len(), 3, "{age:?} rel {rel:?}: {r:?}");
        }
        let Some(mut sim) = load_age("bob", age) else { return };
        let id = face(&mut sim, ObjectBehaviorKind::Bobomb, 130.0).expect("bob-omb");
        let r = tap_run(&mut sim, id, 300.0, 100.0, 3, 0.0, 40, Mode { z: true, rel: Some(0), ..Default::default() });
        let kinds: Vec<Swing> = r.swings.iter().map(|s| s.1).collect();
        assert_eq!(kinds, vec![Swing::Stab, Swing::Stab, Swing::Combo(Direction::Stab)], "{age:?}: {r:?}");
        assert!(!r.hits.is_empty(), "the stab lands: {r:?}");
    }
}

/// The spin attack cuts all round: a Pokey behind Link is struck, once.
#[test]
fn the_spin_attack_strikes_an_enemy_behind_link() {
    for age in [Age::Adult, Age::Child] {
        let Some(mut sim) = load_age("ssl", age) else { return };
        let id = face(&mut sim, ObjectBehaviorKind::Pokey, 200.0).expect("pokey");
        let r = tap_run(&mut sim, id, 300.0, 1000.0, 1, 0.0, 50, Mode { turn_away: true, ..Default::default() });
        let Some(spin) = r.swings.iter().position(|s| s.1 == Swing::Spin) else { panic!("{age:?}: no spin: {r:?}") };
        assert_eq!(r.hits.iter().filter(|h| h.1 == spin).count(), 1, "{age:?}: one blow from the spin: {r:?}");
    }
}

/// Two enemies in the blade's reach at once are each struck once per swing: two Pokeys on one spot lose the same hit points.
#[test]
fn overlapping_enemies_are_each_struck_once_per_swing() {
    let Some(mut sim) = load_age("ssl", Age::Child) else { return };
    let a = face(&mut sim, ObjectBehaviorKind::Pokey, 200.0).expect("pokey");
    let Some(other) = sim.mario.objects.ids().find(|i| *i != a && sim.mario.objects[*i].behavior == ObjectBehaviorKind::Pokey && sim.mario.objects[*i].active) else { panic!("a second pokey") };
    let pos = sim.mario.objects[a].pos;
    sim.mario.objects[other].pos = [pos[0] + 30.0, pos[1], pos[2] + 10.0];
    let r = tap_run(&mut sim, a, 300.0, 100.0, 2, 0.0, 40, Mode::default());
    assert_eq!(r.hits.len(), 2, "{r:?}");
    assert_eq!(r.other_hits, 2, "the neighbour takes its own blow each swing: {r:?}");
    assert_eq!(sim.mario.objects[a].var("health"), sim.mario.objects[other].var("health"));
    assert_eq!(sim.mario.objects[a].var("health"), 1.0);
}

/// A blow knocks the heavy enemies away from Link, not into him.
#[test]
fn a_blow_pushes_a_heavy_enemy_back() {
    for (course, kind, back) in [("bbh", ObjectBehaviorKind::BigBoo, 250.0), ("lll", ObjectBehaviorKind::BigBully, 300.0), ("wf", ObjectBehaviorKind::WhompKing, 320.0)] {
        let Some(mut sim) = load_age(course, Age::Adult) else { return };
        let id = face(&mut sim, kind, back).expect("enemy");
        let mut last = None;
        let mut checked = false;
        for t in 0..40 {
            let b = if t == 6 { B_BUTTON } else { 0 };
            sim.tick(&ctl(0.0, 0.0, b, b));
            let (m, o) = (&sim.mario, &sim.mario.objects[id]);
            let d = ((m.pos[0] - o.pos[0]).powi(2) + (m.pos[2] - o.pos[2]).powi(2)).sqrt();
            if let Some((prev_d, prev_struck)) = last {
                if o.struck > prev_struck {
                    assert!(d > prev_d, "{kind:?}: the blow took it from {prev_d:.0} to {d:.0} units");
                    checked = true;
                }
            }
            last = Some((d, o.struck));
        }
        assert!(checked, "{kind:?} was never struck");
    }
}

/// OoT's crates break to the sword: the SM64 corkbox (a small breakable box) splits on the blade, a big breakable box too; a Bob-omb
/// Buddy at the end of the swing is left alone and Link takes no harm.
#[test]
fn the_blade_splits_crates_and_spares_friends() {
    let Some(mut sim) = load_age("bob", Age::Adult) else { return };
    let id = face(&mut sim, ObjectBehaviorKind::BreakableBoxSmall, 130.0).expect("a small box");
    let r = tap_run(&mut sim, id, 300.0, 100.0, 1, 0.0, 30, Mode::default());
    assert_eq!(r.hits.len(), 1, "{r:?}");
    assert!(!r.alive, "the small box breaks: {r:?}");
    let Some(mut sim) = load_age("bob", Age::Adult) else { return };
    let id = face(&mut sim, ObjectBehaviorKind::BreakableBox, 150.0).expect("a big box");
    let r = tap_run(&mut sim, id, 300.0, 100.0, 1, 0.0, 30, Mode::default());
    assert_eq!(r.hits.len(), 1, "{r:?}");
    assert!(!r.alive, "the big box breaks: {r:?}");
    let Some(mut sim) = load_age("bob", Age::Adult) else { return };
    if let Some(id) = face(&mut sim, ObjectBehaviorKind::BobombBuddy, 90.0) {
        let r = tap_run(&mut sim, id, 300.0, 100.0, 3, 0.0, 30, Mode::default());
        assert!(r.alive && r.hits.is_empty() && r.link_health_lost == 0.0, "{r:?}");
    }
}

/// SM64's walking goombas (not sim objects) take the same sword: the blow reports an impact (mark, sound, hit-stop) like any object's,
/// the spin reaches one behind Link, and a slash does not.
#[test]
fn goombas_fall_to_the_sword_with_an_impact_and_the_spin_reaches_behind() {
    use crate::game::goomba::{interact, spawn_goombas};
    use jumpman_sim::link::moves::EXTRA_TARGET_BASE;
    for (spin, behind, expect) in [(false, false, true), (false, true, false), (true, true, true)] {
        let Some(mut sim) = load_age("bob", Age::Adult) else { return };
        let mut goombas = spawn_goombas(&sim.course.extras.goombas, &sim.mario.world);
        let Some(i) = goombas.iter().position(|g| g.tangible()) else { panic!("no goomba") };
        let gp = goombas[i].pos;
        // Stand 110 units from it on a side that has floor, facing it (or away).
        let mut placed = false;
        for step in 0..16 {
            let a = step as f64 * std::f64::consts::FRAC_PI_8;
            let (x, z) = (gp.x - a.sin() * 110.0, gp.z - a.cos() * 110.0);
            let floor = sim.mario.world.find_floor_height(x, gp.y + 150.0, z);
            if (floor - gp.y).abs() < 60.0 {
                sim.mario.pos = [x, floor, z];
                sim.mario.face_angle[1] = jumpman_sim::math::atan2s(gp.z - z, gp.x - x) as f64 + if behind { 32768.0 } else { 0.0 };
                sim.mario.set_action(ACT_IDLE, 0.0);
                placed = true;
                break;
            }
        }
        assert!(placed);
        let mut hit = false;
        let mut down = false;
        sim.mario.link.take_events();
        for t in 0..60 {
            // A tap at tick 1, or (spin) B held from tick 1 for 40 ticks and released.
            let pressed = if t == 1 { B_BUTTON } else { 0 };
            let held = if spin { t >= 1 && t < 40 } else { t == 1 };
            let c = ctl(0.0, 0.0, if held { B_BUTTON } else { 0 }, pressed);
            let prev = sim.mario.pos;
            sim.tick(&c);
            // Only Link's blade is under test: the goomba stands still.
            goombas[i].pos = gp;
            interact(&mut goombas, &mut sim.mario, &prev);
            for e in sim.mario.link.take_events() {
                if let LinkEvent::HitObject { id, .. } = e {
                    hit |= id == EXTRA_TARGET_BASE + i as u32;
                }
            }
            down |= !goombas[i].tangible();
        }
        assert_eq!(down, expect, "spin {spin} behind {behind}");
        assert_eq!(hit, expect, "an impact is reported with the blow: spin {spin} behind {behind}");
    }
}

/// Never crash: swing at every kind of object each course has (slashes, a Z-stab, a spin, a jump slash), both Links; Link ends on the
/// floor of the course with finite coordinates and no swing strikes one object twice.
#[test]
fn swinging_at_any_sm64_object_never_panics_or_leaves_the_course() {
    let mut kinds_seen = std::collections::BTreeSet::new();
    for course in ["bob", "wf", "ccm", "bbh", "jrb", "lll", "ssl", "hmc", "thi", "castle_grounds", "castle_inside"] {
        let Some(sim0) = load_age(course, Age::Adult) else { continue };
        let mut picked: Vec<(ObjectBehaviorKind, f64)> = vec![];
        for o in &sim0.mario.objects.objects {
            if o.active && o.hitbox.interact_type != 0 && !picked.iter().any(|(k, _)| *k == o.behavior) {
                picked.push((o.behavior, (o.hitbox.radius + 90.0).clamp(110.0, 320.0)));
            }
        }
        for (kind, back) in picked {
            for age in [Age::Adult, Age::Child] {
                for (name, mode, hold) in [
                    ("slashes", Mode::default(), 100.0),
                    ("z stab", Mode { z: true, rel: Some(0), ..Default::default() }, 100.0),
                    ("spin", Mode::default(), 1000.0),
                    ("jump", Mode { z: true, a_at: Some(1), rel: Some(0), ..Default::default() }, 100.0),
                ] {
                    let Some(mut sim) = load_age(course, age) else { continue };
                    let Some(id) = face(&mut sim, kind, back) else { continue };
                    kinds_seen.insert(kind.as_str());
                    let r = tap_run(&mut sim, id, 250.0, hold, if name == "spin" { 1 } else { 3 }, if name == "jump" { 100.0 } else { 0.0 }, 60, mode);
                    let m = &sim.mario;
                    let floor = m.world.find_floor_height(m.pos[0], m.pos[1] + 100.0, m.pos[2]);
                    assert!(m.pos.iter().all(|v| v.is_finite()), "{course} {kind:?} {name}: {:?}", m.pos);
                    assert!(m.pos[1] >= floor - 1.0 || m.action & ACT_FLAG_AIR != 0 || m.pos[1] < -5000.0, "{course} {kind:?} {name} {age:?}: Link at {:?} under the floor {floor}", m.pos);
                    for sw in 0..r.swings.len() {
                        assert!(r.hits.iter().filter(|h| h.1 == sw).count() <= 1, "{course} {kind:?} {name}: a swing struck it twice: {r:?}");
                    }
                }
            }
        }
    }
    println!("object kinds swung at: {kinds_seen:?}");
}

/// `capture_spots -- --ignored --nocapture`: where to stand for the app captures (`JUMPMAN_TELEPORT` x,y,z,yaw degrees) so the
/// target is `back` units ahead.
#[test]
#[ignore]
fn capture_spots() {
    use ObjectBehaviorKind as K;
    for (name, course, kind, back) in [("bigboo", "bbh", K::BigBoo, 250.0), ("bobomb", "bob", K::Bobomb, 140.0), ("pokey", "ssl", K::Pokey, 200.0), ("whomp", "wf", K::Whomp, 200.0), ("bigboo300", "bbh", K::BigBoo, 300.0)] {
        let Ok(mut sim) = GameSim::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"), crate::game::world::CourseRequest::new(course)) else { continue };
        sim.as_link = true;
        sim.mario.link.enable(Age::Adult);
        let ids: Vec<ObjId> = sim.mario.objects.ids().filter(|i| sim.mario.objects[*i].behavior == kind && sim.mario.objects[*i].active).collect();
        for id in ids.into_iter().take(6) {
            let p = sim.mario.objects[id].pos;
            for step in 0..16 {
                let a = step as f64 * std::f64::consts::FRAC_PI_8;
                let (x, z) = (p[0] - a.sin() * back, p[2] - a.cos() * back);
                let floor = sim.mario.world.find_floor_height(x, p[1] + 150.0, z);
                if (floor - p[1]).abs() < 60.0 {
                    println!("SPOT {name} {course} obj {} at {:?}: TELEPORT={:.0},{:.0},{:.0},{:.1}", id.0, p.map(|v| v.round()), x, floor, z, a.to_degrees());
                    break;
                }
            }
        }
    }
}

/// The stab's lunge and the swings' steps use the course's own wall and floor collision: Link tapping B with the stick held in any
/// direction, Z-locked or not, never ends a tick inside a wall, below the floor or beyond the course.
#[test]
fn lunges_and_slashes_never_clip_into_walls_or_leave_the_floor() {
    let mut runs = 0;
    for course in ["bob", "wf", "ccm", "bbh", "castle_grounds"] {
        for dir in 0..16 {
            for (z, age) in [(true, Age::Adult), (false, Age::Child)] {
                let Some(mut sim) = load_age(course, age) else { continue };
                let face0 = sim.mario.face_angle[1];
                let rel = dir as f64 * 4096.0;
                for t in 0..120 {
                    let b = if t % 8 == 2 { B_BUTTON } else { 0 };
                    let th = (face0 + rel - sim.mario.camera_yaw) * std::f64::consts::PI / 32768.0;
                    let mut c = ctl(64.0 * th.sin(), -64.0 * th.cos(), if t % 8 < 4 { B_BUTTON } else { 0 }, b);
                    if z {
                        c.focus_yaw = Some(face0);
                        sim.focus_held = true;
                    }
                    sim.tick(&c);
                    let m = &sim.mario;
                    assert!(m.pos.iter().all(|v| v.is_finite()), "{course} dir {dir}");
                    let floor = m.world.find_floor_height(m.pos[0], m.pos[1] + 100.0, m.pos[2]);
                    assert!(m.pos[1] >= floor - 1.0 || m.action & ACT_FLAG_AIR != 0, "{course} dir {dir} t{t}: Link at {:?} under the floor {floor}", m.pos);
                    let mut p = m.pos;
                    m.world.f32_find_wall_collision(&mut p, 60.0, 30.0);
                    let inside = ((p[0] - m.pos[0]).powi(2) + (p[2] - m.pos[2]).powi(2)).sqrt();
                    assert!(inside < 25.0, "{course} dir {dir} t{t} {z}: Link is {inside:.0} units inside a wall at {:?}, act {:#x}", m.pos, m.action);
                }
                runs += 1;
            }
        }
    }
    assert!(runs > 0 || load_age("bob", Age::Adult).is_none());
}

/// A Z-targeted forward jump (A with the stick forward) and B in the air is the flip slash, and it lands on a Bob-omb ahead.
#[test]
fn a_flip_slash_after_a_z_forward_jump_defeats_a_bobomb() {
    for age in [Age::Adult, Age::Child] {
        let Some(mut sim) = load_age("bob", age) else { return };
        let id = face(&mut sim, ObjectBehaviorKind::Bobomb, 130.0).expect("bob-omb");
        let r = tap_run(&mut sim, id, 300.0, 100.0, 1, 100.0, 50, Mode { z: true, a_at: Some(1), rel: Some(0), ..Default::default() });
        assert_eq!(r.swings.first().map(|s| s.1), Some(Swing::FlipSlash), "{age:?}: {r:?}");
        assert_eq!(r.hits.len(), 1, "{age:?}: {r:?}");
        assert!(!r.alive, "{age:?}: {r:?}");
    }
}
