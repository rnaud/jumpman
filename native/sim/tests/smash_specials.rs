//! Engine-free tests for the Super Smash Bros. specials (`smash.rs`): every fighter's neutral, up and down special on
//! Mario's sim (flat floor, 30 Hz), against a mock target set that stands in for a mode's enemies.
use jumpman_sim::collision::*;
use jumpman_sim::mario_runtime::*;
use jumpman_sim::smash::*;
use jumpman_sim::state::*;
use std::sync::Arc;

fn flat_world() -> CollisionWorld {
    let mut w = CollisionWorld::default();
    let o = SurfaceOptions::default();
    w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
    w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
    w
}

fn fighter(f: SmashFighter) -> MarioRuntime {
    let mut m = MarioRuntime::new(Arc::new(flat_world()), SpawnPoint { action: None, pos: [0.0, 0.0, 0.0], face_yaw: 0.0 }, -1600.0);
    m.face_angle[1] = 0.0;
    m.smash.fighter = Some(f);
    for _ in 0..3 {
        frame(&mut m, 0.0, 0.0, 0, 0);
    }
    m
}

/// One step: stick (raw, -64..64) and button masks (`pressed` is also held).
fn frame(m: &mut MarioRuntime, sx: f64, sy: f64, down: u32, pressed: u32) {
    let c = ControllerSnapshot { focus_yaw: None, stick_x: sx, stick_y: sy, stick_mag: sx.hypot(sy).min(64.0), button_down: down | pressed, button_pressed: pressed };
    m.step(&c, 0x8000 as f64, false, false);
}

/// A dummy enemy.
#[derive(Clone, Debug)]
struct Dummy {
    pos: [f64; 3],
    hp: f32,
    hits: u32,
    asleep: bool,
    gone: bool,
    fighter: Option<SmashFighter>,
    last: Option<SmashHit>,
}

#[derive(Default)]
struct Mock {
    d: Vec<Dummy>,
    /// Enemy projectiles: (id, pos, vel), reflected ones flip `reflected`.
    shots: Vec<(u64, [f64; 3], [f64; 3], bool)>,
    reflected: Vec<(u64, [f64; 3])>,
    absorbed: Vec<u64>,
}

impl Mock {
    fn at(pos: [f64; 3]) -> Self {
        Mock { d: vec![Dummy { pos, hp: 100.0, hits: 0, asleep: false, gone: false, fighter: None, last: None }], ..Default::default() }
    }
    fn damage(&self) -> f32 {
        100.0 - self.d[0].hp
    }
}

impl SmashTargets for Mock {
    fn targets(&self) -> Vec<Target> {
        self.d.iter().enumerate().filter(|(_, d)| !d.gone).map(|(i, d)| Target { key: i as u64, pos: d.pos, radius: 40.0, height: 120.0, fighter: d.fighter }).collect()
    }
    fn hit(&mut self, i: usize, hit: &SmashHit) -> HitOutcome {
        let idx = self.d.iter().enumerate().filter(|(_, d)| !d.gone).nth(i).map(|(k, _)| k).unwrap();
        let d = &mut self.d[idx];
        d.last = Some(*hit);
        d.hits += 1;
        match hit.kind {
            HitKind::Sleep => {
                d.asleep = true;
                HitOutcome::Hit
            }
            HitKind::Capture => {
                d.gone = true;
                HitOutcome::Captured(d.fighter)
            }
            _ => {
                d.hp -= hit.damage;
                HitOutcome::Hit
            }
        }
    }
    fn incoming(&self) -> Vec<Incoming> {
        self.shots.iter().filter(|s| !s.3).map(|s| Incoming { id: s.0, pos: s.1, vel: s.2, radius: 20.0, energy: true }).collect()
    }
    fn reflect(&mut self, id: u64, vel: [f64; 3]) {
        self.reflected.push((id, vel));
        if let Some(s) = self.shots.iter_mut().find(|s| s.0 == id) {
            s.3 = true;
        }
    }
    fn absorb(&mut self, id: u64) {
        self.absorbed.push(id);
        self.shots.retain(|s| s.0 != id);
    }
}

/// A step plus target resolution.
fn step(m: &mut MarioRuntime, t: &mut Mock, sx: f64, sy: f64, down: u32, pressed: u32) {
    frame(m, sx, sy, down, pressed);
    resolve(&mut m.smash, t);
}

fn jump_to_air(m: &mut MarioRuntime) {
    frame(m, 0.0, 0.0, A_BUTTON, A_BUTTON);
    for _ in 0..7 {
        frame(m, 0.0, 0.0, A_BUTTON, 0);
    }
    assert!(m.action & ACT_FLAG_AIR != 0);
}

fn mv(m: &MarioRuntime) -> Option<SpecialMove> {
    m.smash.special.map(|s| s.mv)
}

/// Runs until the special is over (or `max` ticks), tracking the apex; returns (apex, ticks).
fn until_done(m: &mut MarioRuntime, t: &mut Mock, max: u32) -> (f64, u32) {
    let mut apex = m.pos[1];
    for n in 0..max {
        step(m, t, 0.0, 0.0, 0, 0);
        apex = apex.max(m.pos[1]);
        if m.smash.special.is_none() {
            return (apex, n + 1);
        }
    }
    (apex, max)
}

fn count(m: &MarioRuntime, k: ProjKind) -> usize {
    m.smash.projectiles.iter().filter(|p| p.kind == k && !p.dead).count()
}

// ---------------------------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------------------------

#[test]
fn slots_come_from_the_stick_or_a_and_z() {
    let c = |sx: f64, sy: f64, down: u32| ControllerSnapshot { focus_yaw: None, stick_x: sx, stick_y: sy, stick_mag: 0.0, button_down: down, button_pressed: R_TRIG };
    assert_eq!(slot_from_input(&c(0.0, 0.0, 0)), SpecialSlot::Neutral);
    assert_eq!(slot_from_input(&c(0.0, 60.0, 0)), SpecialSlot::Up);
    assert_eq!(slot_from_input(&c(0.0, -60.0, 0)), SpecialSlot::Down);
    assert_eq!(slot_from_input(&c(60.0, 30.0, 0)), SpecialSlot::Neutral, "a sideways tilt is neutral");
    assert_eq!(slot_from_input(&c(0.0, 0.0, A_BUTTON)), SpecialSlot::Up, "A held");
    assert_eq!(slot_from_input(&c(0.0, 0.0, Z_TRIG)), SpecialSlot::Down, "Z held");
    // Every fighter has a move in every slot, and Mario / Luigi share theirs.
    for f in SmashFighter::ALL {
        let moves: Vec<SpecialMove> = [SpecialSlot::Neutral, SpecialSlot::Up, SpecialSlot::Down].iter().map(|s| f.special(*s)).collect();
        assert_eq!(moves.len(), 3);
    }
    assert_eq!(SmashFighter::Luigi.special(SpecialSlot::Up), SmashFighter::Mario.special(SpecialSlot::Up));
}

#[test]
fn nothing_runs_without_a_fighter_and_r_starts_one() {
    let mut m = fighter(SmashFighter::Mario);
    m.smash.fighter = None;
    frame(&mut m, 0.0, 0.0, 0, R_TRIG);
    assert!(m.smash.special.is_none());
    m.smash.fighter = Some(SmashFighter::Mario);
    frame(&mut m, 0.0, 0.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Fireball));
    // The cue for the sound layer.
    assert!(m.smash.events.iter().any(|e| matches!(e, SmashEvent::Sfx(SmashSfx { fighter: SmashFighter::Mario, mv: SpecialMove::Fireball, phase: SfxPhase::Start, .. }))));
}

#[test]
fn a_hit_on_the_fighter_interrupts_the_special() {
    let mut m = fighter(SmashFighter::Captain);
    frame(&mut m, 0.0, 0.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::FalconPunch));
    frame(&mut m, 0.0, 0.0, 0, 0);
    assert!(m.take_damage(2.0, Some([0.0, 0.0, 200.0])));
    frame(&mut m, 0.0, 0.0, 0, 0);
    assert!(m.smash.special.is_none(), "knockback cancels the punch");
}

// ---------------------------------------------------------------------------------------------
// Mario / Luigi
// ---------------------------------------------------------------------------------------------

#[test]
fn mario_fireball_bounces_forward_and_hits() {
    let mut m = fighter(SmashFighter::Mario);
    let mut t = Mock::at([0.0, 0.0, 700.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    let mut seen = false;
    let mut low = f64::MAX;
    let mut high: f64 = 0.0;
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        if let Some(p) = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::Fireball) {
            seen = true;
            low = low.min(p.pos[1]);
            high = high.max(p.pos[1]);
        }
    }
    assert!(seen);
    assert!(low > 0.0 && high < 160.0, "bounces along the floor: {low}..{high}");
    assert!((t.damage() - 7.0).abs() < 0.01, "the ball reached the dummy 700 ahead: {}", t.damage());
    assert_eq!(t.d[0].last.unwrap().kind, HitKind::Fire);
}

#[test]
fn luigi_fireball_flies_straight() {
    let mut m = fighter(SmashFighter::Luigi);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..10 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    let p = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::GreenFireball).expect("green fireball");
    let (y0, z0) = (p.pos[1], p.pos[2]);
    for _ in 0..10 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    let p = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::GreenFireball).expect("still flying");
    assert_eq!(p.pos[1], y0, "no gravity");
    assert!(p.pos[2] - z0 > 200.0);
}

#[test]
fn super_jump_punch_rises_then_falls_helpless() {
    for f in [SmashFighter::Mario, SmashFighter::Luigi] {
        let mut m = fighter(f);
        let mut t = Mock::at([0.0, 0.0, 60.0]);
        step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
        assert_eq!(mv(&m), Some(SpecialMove::SuperJumpPunch));
        let mut apex: f64 = 0.0;
        let mut helpless = false;
        for _ in 0..90 {
            step(&mut m, &mut t, 0.0, 0.0, 0, 0);
            apex = apex.max(m.pos[1]);
            helpless |= m.smash.helpless();
            // A special press while helpless does nothing.
            if m.smash.helpless() {
                frame(&mut m, 0.0, 0.0, 0, R_TRIG);
                assert_eq!(mv(&m), Some(SpecialMove::SuperJumpPunch));
            }
        }
        assert!(apex > 450.0, "{f:?} apex {apex}");
        assert!(m.pos[2] > 200.0, "moves forward");
        assert!(helpless, "{f:?} helpless fall");
        assert!(m.smash.special.is_none() && m.action & ACT_FLAG_AIR == 0, "landed and done");
        assert!(t.damage() > 0.0);
        if f == SmashFighter::Luigi {
            assert!(t.damage() >= 25.0, "Luigi's fire sweet spot: {}", t.damage());
        }
    }
}

#[test]
fn tornado_multi_hits_and_rises_when_mashed() {
    let mut m = fighter(SmashFighter::Mario);
    let mut t = Mock::at([0.0, 0.0, 80.0]);
    step(&mut m, &mut t, 0.0, 0.0, Z_TRIG, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Tornado));
    until_done(&mut m, &mut t, 60);
    assert!(t.d[0].hits >= 5, "multi-hit: {}", t.d[0].hits);
    assert!(t.damage() >= 14.0, "{}", t.damage());
    let mut m = fighter(SmashFighter::Luigi);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    let mut top: f64 = 0.0;
    for k in 0..44 {
        step(&mut m, &mut t, 0.0, 0.0, 0, if k % 2 == 0 { R_TRIG } else { 0 });
        top = top.max(m.pos[1]);
    }
    assert!(top > 100.0, "mashing lifts the cyclone: {top}");
}

// ---------------------------------------------------------------------------------------------
// Donkey Kong
// ---------------------------------------------------------------------------------------------

#[test]
fn giant_punch_charges_stores_and_hits_harder() {
    // Quick punch.
    let mut m = fighter(SmashFighter::Donkey);
    let mut t = Mock::at([0.0, 0.0, 140.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..6 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    until_done(&mut m, &mut t, 80);
    let quick = t.damage();
    assert!((10.0..14.0).contains(&quick), "quick {quick}");
    // Full charge: stored after 10 loops, then released at once.
    let mut m = fighter(SmashFighter::Donkey);
    let mut t = Mock::at([0.0, 0.0, 140.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    until_done(&mut m, &mut t, 120);
    assert_eq!(m.smash.dk_charge, 10, "full charge stored");
    assert_eq!(t.damage(), 0.0);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    until_done(&mut m, &mut t, 80);
    assert!(t.damage() >= 27.0, "full Giant Punch {}", t.damage());
    assert_eq!(m.smash.dk_charge, 0);
    // Jump cancels and keeps a partial charge.
    let mut m = fighter(SmashFighter::Donkey);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..30 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    step(&mut m, &mut t, 0.0, 0.0, Z_TRIG, Z_TRIG);
    assert!(m.smash.special.is_none() && (3..10).contains(&m.smash.dk_charge), "stored {}", m.smash.dk_charge);
}

#[test]
fn spinning_kong_travels_and_lifts_in_the_air() {
    let mut m = fighter(SmashFighter::Donkey);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::SpinningKong));
    for _ in 0..30 {
        step(&mut m, &mut t, 0.0, 64.0, 0, 0);
    }
    assert!(m.pos[2] > 250.0, "travels on the ground: {}", m.pos[2]);
    let mut m = fighter(SmashFighter::Donkey);
    jump_to_air(&mut m);
    let y0 = m.pos[1];
    step(&mut m, &mut t, 0.0, 0.0, A_BUTTON, R_TRIG);
    let (apex, _) = until_done(&mut m, &mut t, 200);
    assert!(apex - y0 > 150.0, "air lift {}", apex - y0);
}

#[test]
fn hand_slap_hits_the_ground_around_and_not_in_the_air() {
    let mut m = fighter(SmashFighter::Donkey);
    let mut t = Mock::at([0.0, 0.0, 200.0]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::HandSlap));
    until_done(&mut m, &mut t, 60);
    assert!(t.damage() >= 12.0 && t.d[0].hits >= 2, "two slaps: {} / {}", t.damage(), t.d[0].hits);
    let mut m = fighter(SmashFighter::Donkey);
    jump_to_air(&mut m);
    frame(&mut m, 0.0, 0.0, Z_TRIG, R_TRIG);
    assert!(m.smash.special.is_none(), "ground only");
}

// ---------------------------------------------------------------------------------------------
// Link
// ---------------------------------------------------------------------------------------------

#[test]
fn boomerang_goes_out_and_comes_back() {
    let mut m = fighter(SmashFighter::Link);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    let mut far: f64 = 0.0;
    let mut out = false;
    let mut back = false;
    for _ in 0..100 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        if let Some(p) = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::Boomerang) {
            out = true;
            far = far.max(p.pos[2]);
        } else if out {
            back = true;
            break;
        }
    }
    assert!(out && back, "caught again");
    assert!(far > 350.0, "range {far}");
    // A hit turns it back.
    let mut m = fighter(SmashFighter::Link);
    let mut t = Mock::at([0.0, 0.0, 300.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..100 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(t.d[0].hits, 1);
    assert_eq!(t.damage(), 9.0);
    // Only one at a time: a second throw while it flies plays the empty clip.
    let mut m = fighter(SmashFighter::Link);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    until_done(&mut m, &mut t, 30);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..12 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(count(&m, ProjKind::Boomerang), 1);
    assert!(m.smash.motion().unwrap().0[0].contains("Empty"));
}

#[test]
fn spin_attack_spins_on_the_ground_and_rises_in_the_air() {
    let mut m = fighter(SmashFighter::Link);
    let mut t = Mock::at([120.0, 0.0, 0.0]);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::SpinAttack));
    until_done(&mut m, &mut t, 60);
    assert!(t.d[0].hits >= 2, "a spin hits all around: {}", t.d[0].hits);
    let mut m = fighter(SmashFighter::Link);
    jump_to_air(&mut m);
    let y0 = m.pos[1];
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    let (apex, _) = until_done(&mut m, &mut t, 200);
    assert!(apex - y0 > 300.0, "air spin lift {}", apex - y0);
}

#[test]
fn link_bomb_is_lobbed_and_explodes() {
    let mut m = fighter(SmashFighter::Link);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::LinkBomb));
    let mut landing = None;
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        if let Some(p) = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::Blast) {
            landing = Some(p.pos);
            break;
        }
    }
    let at = landing.expect("blast on landing");
    assert!(at[2] > 150.0, "lobbed forward: {at:?}");
    // A dummy where it lands takes the blast.
    let mut m = fighter(SmashFighter::Link);
    let mut t = Mock::at([0.0, 0.0, at[2]]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert!(t.damage() >= 6.0, "bomb damage {}", t.damage());
}

// ---------------------------------------------------------------------------------------------
// Samus
// ---------------------------------------------------------------------------------------------

#[test]
fn charge_shot_charges_to_seven_and_scales() {
    let mut m = fighter(SmashFighter::Samus);
    let mut t = Mock::at([0.0, 0.0, 600.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    until_done(&mut m, &mut t, 200);
    assert_eq!(m.smash.samus_charge, 7, "charged up and stored");
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(t.damage(), 26.0, "full Charge Shot");
    // Uncharged: 3%.
    let mut m = fighter(SmashFighter::Samus);
    let mut t = Mock::at([0.0, 0.0, 600.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..9 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(t.damage(), 3.0);
}

#[test]
fn screw_attack_lifts_and_multi_hits() {
    let mut m = fighter(SmashFighter::Samus);
    let mut t = Mock::at([0.0, 150.0, 30.0]);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::ScrewAttack));
    let (apex, _) = until_done(&mut m, &mut t, 200);
    assert!(apex > 250.0, "apex {apex}");
    assert!(t.d[0].hits >= 2);
}

#[test]
fn samus_bomb_drops_and_blows_after_its_fuse() {
    let mut m = fighter(SmashFighter::Samus);
    let mut t = Mock::at([95.0, 0.0, 0.0]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::SamusBomb));
    for _ in 0..10 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(count(&m, ProjKind::SamusBomb), 1);
    let mut blown = false;
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        blown |= m.smash.events.iter().any(|e| matches!(e, SmashEvent::Explode { .. }));
    }
    assert!(blown && count(&m, ProjKind::SamusBomb) == 0);
    assert!(t.damage() > 0.0);
}

// ---------------------------------------------------------------------------------------------
// Yoshi
// ---------------------------------------------------------------------------------------------

#[test]
fn egg_lay_swallows_what_the_tongue_catches() {
    let mut m = fighter(SmashFighter::Yoshi);
    let mut t = Mock::at([0.0, 0.0, 150.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::EggLay));
    let mut caught = false;
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        caught |= m.smash.special.is_some_and(|s| s.phase == Phase::Caught);
    }
    assert!(t.d[0].gone && caught, "caught into an egg");
}

#[test]
fn egg_throw_arcs_and_bursts() {
    let mut m = fighter(SmashFighter::Yoshi);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::EggThrow));
    let mut top: f64 = 0.0;
    let mut burst = None;
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        if let Some(p) = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::Egg) {
            top = top.max(p.pos[1]);
        }
        if let Some(p) = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::Blast) {
            burst = Some(p.pos);
        }
    }
    assert!(top > 200.0, "an arc: {top}");
    let at = burst.expect("burst on landing");
    let mut m = fighter(SmashFighter::Yoshi);
    let mut t = Mock::at([at[0], 0.0, at[2]]);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert!(t.damage() > 0.0);
}

#[test]
fn yoshi_bomb_hops_slams_and_throws_stars() {
    let mut m = fighter(SmashFighter::Yoshi);
    let mut t = Mock::at([0.0, 0.0, 110.0]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::YoshiBomb));
    let mut top: f64 = 0.0;
    let mut pounded = false;
    let mut stars = 0;
    for _ in 0..50 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        top = top.max(m.pos[1]);
        pounded |= m.action == ACT_GROUND_POUND_LAND;
        stars = stars.max(count(&m, ProjKind::Star));
    }
    assert!(top > 100.0, "hop {top}");
    assert!(pounded, "lands as a ground pound (modes see the pound)");
    assert_eq!(stars, 2);
    assert!(t.damage() >= 6.0, "{}", t.damage());
}

// ---------------------------------------------------------------------------------------------
// Kirby
// ---------------------------------------------------------------------------------------------

#[test]
fn inhale_swallows_removes_and_copies() {
    let mut m = fighter(SmashFighter::Kirby);
    let mut t = Mock::at([0.0, 0.0, 200.0]);
    t.d[0].fighter = Some(SmashFighter::Mario);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Inhale));
    for _ in 0..30 {
        step(&mut m, &mut t, 0.0, 0.0, R_TRIG, 0);
    }
    assert!(t.d[0].gone, "inhaled");
    assert_eq!(m.smash.mouth, Some(Some(SmashFighter::Mario)));
    // Z swallows: Mario's fireball is copied.
    step(&mut m, &mut t, 0.0, 0.0, Z_TRIG, Z_TRIG);
    until_done(&mut m, &mut t, 40);
    assert_eq!(m.smash.copy, Some(SmashFighter::Mario));
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Fireball));
    assert!(m.smash.motion().unwrap().0[0].starts_with("CopyMario"), "{:?}", m.smash.motion());
    for _ in 0..10 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(count(&m, ProjKind::Fireball), 1, "Kirby throws Mario's fireball");
}

#[test]
fn inhale_spits_a_star_at_another_enemy() {
    let mut m = fighter(SmashFighter::Kirby);
    let mut t = Mock::at([0.0, 0.0, 180.0]);
    t.d.push(Dummy { pos: [0.0, 0.0, 700.0], hp: 100.0, hits: 0, asleep: false, gone: false, fighter: None, last: None });
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..20 {
        step(&mut m, &mut t, 0.0, 0.0, R_TRIG, 0);
    }
    assert!(t.d[0].gone && !t.d[1].gone);
    for _ in 0..8 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(t.d[1].hp, 90.0, "the spat star (10%)");
    assert!(m.smash.copy.is_none(), "spitting copies nothing");
}

#[test]
fn final_cutter_rises_drops_and_sends_a_wave() {
    let mut m = fighter(SmashFighter::Kirby);
    let mut t = Mock::at([0.0, 0.0, 500.0]);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::FinalCutter));
    let mut apex: f64 = 0.0;
    let mut wave = false;
    for _ in 0..80 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        apex = apex.max(m.pos[1]);
        wave |= count(&m, ProjKind::Cutter) > 0;
    }
    assert!(apex > 500.0, "apex {apex}");
    assert!(wave, "landing shock wave");
    assert!(t.damage() >= 3.0, "the wave runs into the dummy: {}", t.damage());
}

#[test]
fn stone_falls_hard_and_shrugs_off_hits() {
    let mut m = fighter(SmashFighter::Kirby);
    let mut t = Mock::default();
    jump_to_air(&mut m);
    step(&mut m, &mut t, 0.0, 0.0, Z_TRIG, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Stone));
    for _ in 0..30 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert!(m.smash.armor());
    assert!(m.pos[1] < 1.0, "dropped to the floor");
    assert!(!m.take_damage(3.0, Some([0.0, 0.0, 100.0])), "a rock cannot be hurt");
    let (_, n) = until_done(&mut m, &mut t, 120);
    assert!(n < 120 && !m.smash.armor(), "turns back by itself");
}

// ---------------------------------------------------------------------------------------------
// Fox
// ---------------------------------------------------------------------------------------------

#[test]
fn blaster_fires_a_fast_laser() {
    let mut m = fighter(SmashFighter::Fox);
    let mut t = Mock::at([0.0, 0.0, 1000.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..20 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(t.damage(), 3.0, "a laser reached 1000 units away within 20 ticks");
}

#[test]
fn fire_fox_charges_then_flies() {
    let mut m = fighter(SmashFighter::Fox);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::FireFox));
    for _ in 0..15 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert!(m.pos[1] < 10.0, "still charging in place");
    let (apex, _) = until_done(&mut m, &mut t, 200);
    assert!(apex > 400.0, "flight {apex}");
    // With a stick it flies that way.
    let mut m = fighter(SmashFighter::Fox);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    for _ in 0..40 {
        step(&mut m, &mut t, 64.0, 0.0, 0, 0);
    }
    assert!(m.pos[0].abs() > 200.0, "angled flight: {:?}", m.pos);
}

#[test]
fn reflector_sends_projectiles_back() {
    let mut m = fighter(SmashFighter::Fox);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Reflector));
    t.shots.push((7, [0.0, 80.0, 90.0], [0.0, 0.0, -20.0], false));
    for _ in 0..10 {
        step(&mut m, &mut t, 0.0, 0.0, R_TRIG, 0);
    }
    assert_eq!(t.reflected.len(), 1);
    assert!(t.reflected[0].1[2] > 20.0, "sent back faster: {:?}", t.reflected[0].1);
    // Released: ends.
    let (_, n) = until_done(&mut m, &mut t, 40);
    assert!(n < 40);
}

// ---------------------------------------------------------------------------------------------
// Pikachu
// ---------------------------------------------------------------------------------------------

#[test]
fn thunder_jolt_crawls_along_the_floor() {
    let mut m = fighter(SmashFighter::Pikachu);
    let mut t = Mock::at([0.0, 0.0, 600.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    let mut ys = vec![];
    for _ in 0..50 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        if let Some(p) = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::ThunderJolt) {
            ys.push(p.pos[1]);
        }
    }
    assert!(!ys.is_empty() && ys.iter().all(|y| *y < 60.0), "hugs the floor: {ys:?}");
    assert_eq!(t.damage(), 8.0);
    assert_eq!(t.d[0].last.unwrap().kind, HitKind::Electric);
    // From the air it drops to the floor first, then crawls.
    let mut m = fighter(SmashFighter::Pikachu);
    let mut t = Mock::default();
    jump_to_air(&mut m);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    let p = m.smash.projectiles.iter().find(|p| p.kind == ProjKind::ThunderJolt).expect("air jolt");
    assert!(p.pos[1] < 40.0 && p.vel[0].hypot(p.vel[2]) > 10.0, "{p:?}");
}

#[test]
fn quick_attack_zips_once_or_twice() {
    let mut m = fighter(SmashFighter::Pikachu);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::QuickAttack));
    let (one, _) = until_done(&mut m, &mut t, 120);
    assert!(one > 150.0, "one zip {one}");
    let mut m = fighter(SmashFighter::Pikachu);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    let mut two: f64 = 0.0;
    for k in 0..120 {
        step(&mut m, &mut t, 0.0, 0.0, 0, if k == 5 { R_TRIG } else { 0 });
        two = two.max(m.pos[1]);
    }
    assert!(two > one * 1.5, "two zips {two} vs {one}");
}

#[test]
fn thunder_strikes_pikachu_and_bursts() {
    let mut m = fighter(SmashFighter::Pikachu);
    let mut t = Mock::at([90.0, 0.0, 0.0]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Thunder));
    let mut bolt = false;
    let mut burst = false;
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        bolt |= count(&m, ProjKind::Thunderbolt) > 0;
        burst |= m.smash.special.is_some_and(|s| s.phase == Phase::Attack);
    }
    assert!(bolt && burst, "bolt {bolt} burst {burst}");
    assert!(t.damage() >= 17.0, "burst {}", t.damage());
}

// ---------------------------------------------------------------------------------------------
// Jigglypuff
// ---------------------------------------------------------------------------------------------

#[test]
fn pound_lunges_and_slaps() {
    let mut m = fighter(SmashFighter::Purin);
    let mut t = Mock::at([0.0, 0.0, 140.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Pound));
    until_done(&mut m, &mut t, 60);
    assert!(m.pos[2] > 100.0, "lunge {}", m.pos[2]);
    assert_eq!(t.damage(), 12.0);
}

#[test]
fn sing_puts_bodies_to_sleep() {
    let mut m = fighter(SmashFighter::Purin);
    let mut t = Mock::at([150.0, 0.0, 100.0]);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Sing));
    until_done(&mut m, &mut t, 120);
    assert!(t.d[0].asleep);
    assert_eq!(t.damage(), 0.0);
}

#[test]
fn rest_hits_hard_then_sleeps() {
    let mut m = fighter(SmashFighter::Purin);
    let mut t = Mock::at([0.0, 0.0, 50.0]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::Rest));
    for _ in 0..5 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    assert_eq!(t.damage(), 20.0);
    assert!(m.smash.asleep());
    let z = m.pos[2];
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 64.0, A_BUTTON, A_BUTTON);
        assert!(m.smash.asleep());
    }
    assert!((m.pos[2] - z).abs() < 1.0 && m.pos[1] < 1.0, "cannot move or jump while asleep");
    let (_, n) = until_done(&mut m, &mut t, 120);
    assert!(n < 120 && !m.smash.asleep(), "wakes up");
}

// ---------------------------------------------------------------------------------------------
// Captain Falcon
// ---------------------------------------------------------------------------------------------

#[test]
fn falcon_punch_charges_then_lunges() {
    let mut m = fighter(SmashFighter::Captain);
    let mut t = Mock::at([0.0, 0.0, 240.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    let mut first_hit = None;
    let mut z_at_hit = 0.0;
    for n in 1..46 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        if first_hit.is_none() && t.damage() > 0.0 {
            first_hit = Some(n);
            z_at_hit = m.pos[2];
        }
        if n < 20 {
            assert!(m.pos[2].abs() < 1.0, "winds up in place");
        }
    }
    let n = first_hit.expect("the punch lands");
    assert!(n >= 25, "no hit during the wind-up: tick {n}");
    assert!(z_at_hit > 40.0 && m.pos[2] > 100.0, "lunges forward: {z_at_hit} / {}", m.pos[2]);
    assert_eq!(t.damage(), 25.0);
    assert_eq!(t.d[0].last.unwrap().kind, HitKind::Fire);
}

#[test]
fn falcon_dive_rises_and_grabs() {
    let mut m = fighter(SmashFighter::Captain);
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::FalconDive));
    let (apex, _) = until_done(&mut m, &mut t, 200);
    assert!(apex > 400.0, "apex {apex}");
    // A dummy above in its path is grabbed and blown off.
    let mut m = fighter(SmashFighter::Captain);
    let mut t = Mock::at([0.0, 150.0, 70.0]);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    let mut caught = false;
    for _ in 0..80 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        caught |= m.smash.special.is_some_and(|s| s.phase == Phase::Caught);
    }
    assert!(caught);
    assert!(t.damage() >= 10.0, "{}", t.damage());
}

#[test]
fn falcon_kick_dashes_on_the_ground_and_dives_from_the_air() {
    let mut m = fighter(SmashFighter::Captain);
    let mut t = Mock::at([0.0, 0.0, 300.0]);
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::FalconKick));
    until_done(&mut m, &mut t, 60);
    assert!(m.pos[2] > 700.0, "dash {}", m.pos[2]);
    assert!(t.damage() >= 9.0, "{}", t.damage());
    let mut m = fighter(SmashFighter::Captain);
    jump_to_air(&mut m);
    let z0 = m.pos[2];
    step(&mut m, &mut t, 0.0, 0.0, Z_TRIG, R_TRIG);
    let mut landed = false;
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        landed |= m.smash.special.is_some_and(|s| s.phase == Phase::Landing);
    }
    assert!(landed && m.pos[2] > z0 + 50.0, "dives forward to the floor");
}

// ---------------------------------------------------------------------------------------------
// Ness
// ---------------------------------------------------------------------------------------------

#[test]
fn pk_fire_becomes_a_pillar() {
    let mut m = fighter(SmashFighter::Ness);
    let mut t = Mock::at([0.0, 0.0, 250.0]);
    step(&mut m, &mut t, 0.0, 0.0, 0, R_TRIG);
    let mut pillar = false;
    for _ in 0..60 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        pillar |= count(&m, ProjKind::PkFirePillar) > 0;
    }
    assert!(pillar);
    assert!(t.d[0].hits >= 3, "the pillar burns: {} hits", t.d[0].hits);
}

#[test]
fn pk_thunder_is_steered_and_launches_ness_into_pk_thunder_2() {
    let mut m = fighter(SmashFighter::Ness);
    let mut t = Mock::default();
    jump_to_air(&mut m);
    step(&mut m, &mut t, 0.0, 64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::PkThunder));
    for _ in 0..12 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
    }
    let i = m.smash.projectiles.iter().position(|p| p.kind == ProjKind::PkThunder).expect("the ball");
    // Steering turns it.
    let before = m.smash.projectiles[i].vel;
    for _ in 0..5 {
        step(&mut m, &mut t, 64.0, 0.0, 0, 0);
    }
    let i = m.smash.projectiles.iter().position(|p| p.kind == ProjKind::PkThunder).expect("still flying");
    assert_ne!(m.smash.projectiles[i].vel, before);
    // Aim it at Ness from behind: PK Thunder 2 launches him forward.
    let (p0, z0) = (m.pos, m.pos[2]);
    m.smash.projectiles[i].pos = [p0[0], p0[1] + 80.0, p0[2] - 120.0];
    m.smash.projectiles[i].vel = [0.0, 0.0, 21.0];
    let mut launched = false;
    for _ in 0..40 {
        step(&mut m, &mut t, 0.0, 0.0, 0, 0);
        launched |= m.smash.special.is_some_and(|s| s.phase == Phase::Travel);
    }
    assert!(launched, "PK Thunder 2");
    assert!(m.pos[2] - z0 > 300.0, "flung forward {}", m.pos[2] - z0);
}

#[test]
fn psi_magnet_absorbs_and_heals() {
    let mut m = fighter(SmashFighter::Ness);
    m.health = 0x500 as f64;
    let mut t = Mock::default();
    step(&mut m, &mut t, 0.0, -64.0, 0, R_TRIG);
    assert_eq!(mv(&m), Some(SpecialMove::PsiMagnet));
    for _ in 0..8 {
        step(&mut m, &mut t, 0.0, 0.0, R_TRIG, 0);
    }
    t.shots.push((3, [0.0, 70.0, 90.0], [0.0, 0.0, -20.0], false));
    for _ in 0..5 {
        step(&mut m, &mut t, 0.0, 0.0, R_TRIG, 0);
    }
    assert_eq!(t.absorbed, vec![3]);
    assert!(m.health > 0x500 as f64, "healed: {}", m.health);
}

// ---------------------------------------------------------------------------------------------
// Every special: starts, ends, and names motions
// ---------------------------------------------------------------------------------------------

#[test]
fn every_special_starts_ends_and_names_its_motions() {
    for f in SmashFighter::ALL {
        for (slot, sy, down) in [(SpecialSlot::Neutral, 0.0, 0), (SpecialSlot::Up, 64.0, 0), (SpecialSlot::Down, -64.0, 0)] {
            for in_air in [false, true] {
                let mut m = fighter(f);
                if in_air {
                    jump_to_air(&mut m);
                }
                let mut t = Mock::default();
                step(&mut m, &mut t, 0.0, sy, down, R_TRIG);
                let want = f.special(slot);
                if want == SpecialMove::HandSlap && in_air {
                    assert!(m.smash.special.is_none());
                    continue;
                }
                assert_eq!(mv(&m), Some(want), "{f:?} {slot:?} air={in_air}");
                let (names, _, _) = m.smash.motion().unwrap();
                assert!(!names.is_empty() && names.iter().all(|n| !n.is_empty()));
                // Release the charge / hold moves with a press, then let it finish.
                let mut done = false;
                for k in 0..400 {
                    let p = if k == 20 || k == 200 { R_TRIG } else { 0 };
                    step(&mut m, &mut t, 0.0, 0.0, 0, p);
                    if m.smash.special.is_none() {
                        done = true;
                        break;
                    }
                }
                assert!(done, "{f:?} {slot:?} air={in_air} never ended: {:?}", m.smash.special);
                assert!(m.pos[1] > -1.0, "{f:?} {slot:?} stayed above the floor");
            }
        }
    }
}
