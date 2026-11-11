//! Link's sword (and the rest of Link) in the Banjo-Kazooie worlds: real BK course data, real BK baddies, finger-cadence taps through the
//! shared interaction pass. `probe_*` tests are `#[ignore]`d diagnostics (`cargo test --release -p jumpman bk_enemies::tests::link::probe_
//! -- --ignored --nocapture`).
use super::*;
use jumpman_sim::link::combat::{Dmg, Swing};
use jumpman_sim::link::moves::LinkEvent;
use jumpman_sim::link::targeting::{target_class, TargetClass};

/// A world that places `kind` (the baddies are spread over the BK maps).
pub fn world_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Grublin | Kind::Termite | Kind::Bigbutt | Kind::Beehive => "bk_mumbos_mountain",
        Kind::Snippet | Kind::Shrapnel | Kind::Nipper => "bk_treasure_trove_cove",
        Kind::Flibbit | Kind::Buzzbomb | Kind::Tanktup => "bk_bubblegloop_swamp",
        Kind::Clucker | Kind::Snarebear | Kind::GrublinHood => "bk_click_clock_wood_summer",
        Kind::Bat | Kind::TeeHee | Kind::Limbo => "bk_mad_monster_mansion",
        Kind::SirSlush => "bk_freezeezy_peak",
        Kind::GrilleChompa => "bk_clankers_cavern",
        Kind::Slappa | Kind::Grabba => "bk_gobis_valley",
        _ => "bk_mumbos_mountain",
    }
}

#[derive(Debug, Default, Clone)]
pub struct Run {
    pub swings: Vec<(usize, Swing)>,
    pub hits: Vec<(usize, Dmg)>,
    /// (tick, enemy action) at every change
    pub acts: Vec<(usize, i32)>,
    /// (tick, Link's action) at every change
    pub link_acts: Vec<(usize, u32)>,
    pub struck: f64,
    pub hp: f64,
    pub gone_at: Option<usize>,
    pub link_hurt: f64,
    /// the Z-target was on the enemy at some tick
    pub locked: bool,
}

impl Run {
    pub fn finisher(&self) -> bool {
        self.swings.iter().any(|(_, s)| matches!(s, Swing::Combo(_)))
    }
}

/// What the player's hands do on one tick.
#[derive(Clone, Copy, Default)]
pub struct In {
    pub down: u32,
    pub pressed: u32,
    pub x: f64,
    pub y: f64,
}

pub struct Setup {
    pub kind: Kind,
    pub nth: usize,
    pub age: Age,
    pub dist: f64,
    pub z: bool,
    /// the baddie keeps its place (its state machine still runs)
    pub freeze: bool,
    /// runs once Link stands in place (put him in the air, turn him around...)
    pub pre: Option<fn(&mut GameSim)>,
    pub ticks: usize,
}

impl Setup {
    pub fn new(kind: Kind, age: Age, dist: f64) -> Setup {
        Setup { kind, nth: 0, age, dist, z: false, freeze: true, pre: None, ticks: 120 }
    }
}

/// Place Link `dist` units in front of the `nth` baddie of `kind` (everything else asleep) and play `input` for `ticks` ticks.
pub fn play(s: &Setup, mut input: impl FnMut(usize, &GameSim) -> In) -> Option<(GameSim, Run)> {
    let mut sim = GameSim::load(&assets(), CourseRequest::new(world_of(s.kind))).ok()?;
    sim.as_link = true;
    sim.mario.link.enable(s.age);
    let i = sim.mario.objects.objects.iter().enumerate().filter(|(_, o)| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == s.kind).map(|(i, _)| i).nth(s.nth)?;
    face_enemy(&mut sim, i, s.dist, 0.0);
    if let Some(pre) = s.pre {
        pre(&mut sim);
    }
    sim.focus_held = s.z;
    let at = sim.mario.objects.objects[i].pos;
    let mut run = Run::default();
    let health0 = sim.mario.link.inv.health as f64;
    let (mut last_act, mut last_link) = (-1, 0);
    for k in 0..s.ticks {
        let h = input(k, &sim);
        let mag = (h.x * h.x + h.y * h.y).sqrt().min(64.0);
        let c = ControllerSnapshot { stick_x: h.x, stick_y: h.y, stick_mag: mag, button_down: h.down | h.pressed, button_pressed: h.pressed, ..Default::default() };
        if s.freeze {
            let o = &mut sim.mario.objects.objects[i];
            if o.active {
                o.pos[0] = at[0];
                o.pos[2] = at[2];
                o.forward_vel = 0.0;
            }
        }
        sim.tick(&c);
        if let Some(t) = sim.link_focus.target {
            run.locked |= t == crate::camera::link_focus::FocusTarget::Object(ObjId(i as u32));
        }
        for e in sim.mario.link.take_events() {
            match e {
                LinkEvent::Swing(sw) => run.swings.push((k, sw)),
                LinkEvent::HitObject { id, dmg, .. } if id as usize == i => run.hits.push((k, dmg)),
                _ => {}
            }
        }
        if sim.mario.action != last_link {
            last_link = sim.mario.action;
            run.link_acts.push((k, last_link));
        }
        let o = &sim.mario.objects.objects[i];
        if o.active && o.action != last_act {
            last_act = o.action;
            run.acts.push((k, o.action));
        }
        if !o.active && run.gone_at.is_none() {
            run.gone_at = Some(k);
        }
    }
    let o = &sim.mario.objects.objects[i];
    run.struck = o.var("struck");
    run.hp = o.var("hp");
    run.link_hurt = health0 - sim.mario.link.inv.health as f64;
    Some((sim, run))
}

/// A finger on B: `taps` presses `interval_ms` apart, each held `hold_ms`, the first 300 ms in; an edge latches like
/// `PlayerInput::consume_step` (a press between two ticks is seen by the next one, even when already released).
pub fn finger(interval_ms: f64, hold_ms: f64, taps: usize) -> impl FnMut(usize, &GameSim) -> In {
    let dt = 1000.0 / 30.0;
    let starts: Vec<f64> = (0..taps).map(|n| 300.0 + n as f64 * interval_ms).collect();
    let mut next_edge = 0;
    move |k, _| {
        let t1 = (k + 1) as f64 * dt;
        let t0 = k as f64 * dt;
        let mut pressed = 0;
        while next_edge < starts.len() && starts[next_edge] < t1 {
            pressed = B_BUTTON;
            next_edge += 1;
        }
        let held = starts.iter().any(|s| *s < t1 && s + hold_ms > t0);
        In { down: if held || pressed != 0 { B_BUTTON } else { 0 }, pressed, ..Default::default() }
    }
}

/// Taps that give the finisher: `(interval_ms, hold_ms)` at human cadences.
const CADENCES: [(f64, f64); 5] = [(150.0, 100.0), (200.0, 60.0), (300.0, 100.0), (400.0, 100.0), (450.0, 100.0)];

/// Three finger taps at human cadences, with the baddie in blade reach, chain into the three swings and the finisher for adult and Young Link,
/// and every swing's blade window lands a blow on a baddie that is still standing: the Ticker and the Snippet take two (they have two hit points),
/// the Bigbutt takes three (a sword never fells it, it only reels), the Flibbit three (each blow bounces it).
#[test]
fn finger_taps_chain_into_the_finisher_and_every_swing_lands_on_banjo_baddies() {
    for age in [Age::Adult, Age::Child] {
        for (interval, hold) in CADENCES {
            for (kind, blows, dies) in [(Kind::Termite, 2, true), (Kind::Snippet, 2, true), (Kind::Bigbutt, 3, false), (Kind::Flibbit, 3, false)] {
                let (_, r) = play(&Setup::new(kind, age, 150.0), finger(interval, hold, 3)).unwrap();
                assert_eq!(r.swings.len(), 3, "{kind:?} {age:?} every {interval} ms: {:?}", r.swings);
                assert!(r.finisher(), "{kind:?} {age:?} every {interval} ms: {:?}", r.swings);
                assert!(r.hits.len() >= blows, "{kind:?} {age:?} every {interval} ms: blows {:?} swings {:?} acts {:?}", r.hits, r.swings, r.acts);
                assert_eq!(r.gone_at.is_some(), dies, "{kind:?} {age:?} every {interval} ms: gone {:?} hp {} acts {:?}", r.gone_at, r.hp, r.acts);
                // one blow per swing at most, each inside its swing's clip
                for (tick, _) in &r.hits {
                    assert!(r.swings.iter().any(|(s, _)| tick >= s && *tick <= s + 16), "{kind:?}: a blow at tick {tick} outside every swing {:?}", r.swings);
                }
                assert!(r.hits.len() <= 3);
            }
        }
    }
}

/// The finisher's lunge stops at a baddie's body: Link does not end up inside, or on the far side of, a Bigbutt.
#[test]
fn the_lunge_of_the_finisher_stops_at_a_baddies_body() {
    for age in [Age::Adult, Age::Child] {
        let (sim, r) = play(&Setup::new(Kind::Bigbutt, age, 150.0), finger(250.0, 100.0, 3)).unwrap();
        assert!(r.finisher());
        let e = sim.mario.objects.objects.iter().find(|o| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == Kind::Bigbutt && o.distance_to_mario < 400.0).unwrap();
        let d = ((sim.mario.pos[0] - e.pos[0]).powi(2) + (sim.mario.pos[2] - e.pos[2]).powi(2)).sqrt();
        assert!(d > 90.0, "{age:?}: Link stands {d:.0} units from the Bigbutt's centre, inside its body");
        assert!(sim.mario.pos[2] < e.pos[2], "{age:?}: Link was carried through the Bigbutt (z {} vs {})", sim.mario.pos[2], e.pos[2]);
    }
}

/// A Grublin falls to the first blow (it has no hit reaction in the decomp), the Hood only flinches from the sword (only strong blows kill it).
#[test]
fn the_sword_kills_a_grublin_at_once_and_only_makes_a_hood_flinch() {
    for age in [Age::Adult, Age::Child] {
        let (_, g) = play(&Setup::new(Kind::Grublin, age, 150.0), finger(300.0, 100.0, 3)).unwrap();
        assert!(g.gone_at.is_some(), "{age:?}: grublin acts {:?}", g.acts);
        assert!(g.acts.iter().any(|(_, a)| *a == hum::DIE));
        let (_, h) = play(&Setup::new(Kind::GrublinHood, age, 150.0), finger(300.0, 100.0, 3)).unwrap();
        assert_eq!(h.hits.len(), 3, "{age:?}: hood blows {:?}", h.hits);
        assert!(h.gone_at.is_none() && !h.acts.iter().any(|(_, a)| *a == hum::DIE), "{age:?}: a hood is not felled by a sword slash: {:?}", h.acts);
    }
}

/// The adult's spin attack and jump slash are strong blows (the Master Sword's `MasterSpin` / `MasterJump` rows): they fell the Hood and knock the
/// Bigbutt over, which the slashes cannot.
#[test]
fn the_adults_charged_spin_fells_a_hood_and_knocks_a_bigbutt_over() {
    let hold_b = |k: usize, _: &GameSim| match k {
        0..=2 => In { down: B_BUTTON, pressed: if k == 0 { B_BUTTON } else { 0 }, ..Default::default() },
        3..=70 => In { down: B_BUTTON, ..Default::default() },
        _ => In::default(),
    };
    let (_, hood) = play(&Setup { ticks: 200, ..Setup::new(Kind::GrublinHood, Age::Adult, 150.0) }, hold_b).unwrap();
    assert!(hood.swings.iter().any(|(_, s)| s.is_spin()), "the release spins: {:?}", hood.swings);
    assert!(hood.acts.iter().any(|(_, a)| *a == hum::DIE), "hood {:?} hits {:?}", hood.acts, hood.hits);
    let (_, bull) = play(&Setup { ticks: 200, ..Setup::new(Kind::Bigbutt, Age::Adult, 150.0) }, hold_b).unwrap();
    assert!(bull.hits.iter().any(|(_, d)| matches!(d, Dmg::MasterSpin | Dmg::KokiriSpin)), "spin blow {:?}", bull.hits);
    assert!(bull.acts.iter().any(|(_, a)| *a == jumpman_sim::bk_enemy::bull::FALL), "bigbutt {:?}", bull.acts);
}

/// The spin attack hits all around Link, also a baddie behind him; the adult's jump slash (B in the air) is a strong blow too.
#[test]
fn the_spin_reaches_a_baddie_behind_link_and_the_jump_slash_fells_a_hood() {
    let hold_b = |k: usize, _: &GameSim| match k {
        0..=2 => In { down: B_BUTTON, pressed: if k == 0 { B_BUTTON } else { 0 }, ..Default::default() },
        3..=70 => In { down: B_BUTTON, ..Default::default() },
        _ => In::default(),
    };
    // turn Link's back to the Hood: only a swing that goes all around reaches it (the first slash, aimed the other way, cannot)
    let behind = Setup { ticks: 200, pre: Some(|sim| sim.mario.face_angle[1] = 32768.0), ..Setup::new(Kind::GrublinHood, Age::Adult, 120.0) };
    let (_, r) = play(&behind, hold_b).unwrap();
    assert!(r.hits.iter().any(|(_, d)| *d == Dmg::MasterSpin), "the spin hit the Hood behind him: swings {:?} hits {:?}", r.swings, r.hits);
    assert!(r.acts.iter().any(|(_, a)| *a == hum::DIE));
    // a jump slash from above the Hood's reach
    let up = Setup {
        ticks: 90,
        pre: Some(|sim| {
            sim.mario.pos[1] += 120.0;
            sim.mario.set_action(ACT_FREEFALL, 0.0);
        }),
        ..Setup::new(Kind::GrublinHood, Age::Adult, 110.0)
    };
    let (_, r) = play(&up, |k, _| if k == 1 { In { down: B_BUTTON, pressed: B_BUTTON, ..Default::default() } } else { In::default() }).unwrap();
    assert!(r.swings.iter().any(|(_, s)| matches!(s, Swing::JumpSlash)), "{:?}", r.swings);
    assert!(r.hits.iter().any(|(_, d)| *d == Dmg::MasterJump), "jump slash blows {:?}", r.hits);
    assert!(r.acts.iter().any(|(_, a)| *a == hum::DIE), "hood {:?}", r.acts);
}

/// Z-targeting a baddie: the lock picks it, stick forward + B is the stab, stick sideways the side slash, and the blade lands.
#[test]
fn z_target_stab_and_side_slashes_hit_banjo_baddies() {
    for age in [Age::Adult, Age::Child] {
        let mut stab = Setup::new(Kind::Termite, age, 150.0);
        stab.z = true;
        let (_, r) = play(&stab, |k, _| if k == 12 { In { down: B_BUTTON, pressed: B_BUTTON, y: 64.0, ..Default::default() } } else { In::default() }).unwrap();
        assert!(r.locked, "{age:?}: the lock never took the Ticker");
        assert_eq!(r.swings.first().map(|s| s.1), Some(Swing::Stab), "{age:?}: {:?}", r.swings);
        assert_eq!(r.hits.len(), 1, "{age:?}: the stab lands once: {:?}", r.hits);
        let mut side = Setup::new(Kind::Snippet, age, 150.0);
        side.z = true;
        let (_, r) = play(&side, |k, _| if k == 12 { In { down: B_BUTTON, pressed: B_BUTTON, x: 64.0, ..Default::default() } } else { In::default() }).unwrap();
        assert!(matches!(r.swings.first().map(|s| s.1), Some(Swing::Left | Swing::Right)), "{age:?}: {:?}", r.swings);
        assert_eq!(r.hits.len(), 1, "{age:?}: the side slash lands once: {:?}", r.hits);
        // stick back + B while locked: the backward slash (the right slash of `func_80837818`)
        let mut back = Setup::new(Kind::Termite, age, 150.0);
        back.z = true;
        let (_, r) = play(&back, |k, _| if k == 12 { In { down: B_BUTTON, pressed: B_BUTTON, y: -64.0, ..Default::default() } } else { In::default() }).unwrap();
        assert!(matches!(r.swings.first().map(|s| s.1), Some(Swing::Right | Swing::Left)), "{age:?}: {:?}", r.swings);
        assert!(r.hits.len() <= 1, "{age:?}: {:?}", r.hits);
    }
}

/// Everything that is up and about is a target (also the harmless Ticker, which is only a breakable body to the interaction pass); hives, Mumbo,
/// podiums, snowballs, corpses and buried biters are not. The lock takes the baddie in front of Link.
#[test]
fn link_z_targets_banjo_baddies_that_are_up_and_not_the_rest() {
    for kind in [Kind::Termite, Kind::Grublin, Kind::Bigbutt, Kind::Snippet, Kind::Flibbit, Kind::Buzzbomb, Kind::TeeHee, Kind::SirSlush, Kind::Snarebear] {
        let mut s = Setup::new(kind, Age::Adult, 400.0);
        s.z = true;
        s.freeze = false;
        s.ticks = 8;
        let (sim, r) = play(&s, |_, _| In::default()).unwrap();
        assert!(r.locked, "{kind:?}: not locked");
        let i = sim.mario.objects.objects.iter().position(|o| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == kind && o.active && target_class(o).is_some()).unwrap();
        assert_eq!(target_class(&sim.mario.objects.objects[i]), Some(TargetClass::Enemy), "{kind:?}");
    }
    let sim = mumbos();
    for o in &sim.mario.objects.objects {
        if o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == Kind::Beehive {
            assert_eq!(target_class(o), None, "a beehive is not an opponent");
        }
    }
    // a Grublin that is dying and a plant that is buried are not
    let mut sim = mumbos();
    let i = grublins(&sim)[0];
    face_enemy(&mut sim, i, 400.0, 0.0);
    for _ in 0..3 {
        sim.tick(&idle());
    }
    assert_eq!(target_class(&sim.mario.objects.objects[i]), Some(TargetClass::Enemy));
    sim.mario.objects.objects[i].interact_status |= jumpman_sim::object::INT_STATUS_WAS_ATTACKED | jumpman_sim::object::ATTACK_PUNCH as u32;
    sim.tick(&idle());
    assert_eq!(sim.mario.objects.objects[i].action, hum::DIE);
    assert_eq!(target_class(&sim.mario.objects.objects[i]), None, "a corpse is not a target");
    let sim = GameSim::load(&assets(), CourseRequest::new("bk_clankers_cavern")).unwrap();
    for o in &sim.mario.objects.objects {
        if o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == Kind::GrilleChompa && o.action == jumpman_sim::bk_enemy::plant::IDLE {
            assert_eq!(target_class(o), None, "a buried Grille Chompa is not a target");
        }
    }
}

/// A chasing Grublin that touches Link takes a heart and throws him back; a B tapped during the knock-back is not lost, it swings as soon as Link can
/// act, and that swing kills the Grublin.
#[test]
fn damage_from_a_banjo_baddie_knocks_link_back_and_a_b_press_made_then_is_kept() {
    for age in [Age::Adult, Age::Child] {
        let mut s = Setup::new(Kind::Grublin, age, 700.0);
        s.freeze = false;
        s.ticks = 300;
        // the buffer keeps a B press made in the last 9 ticks of the knock-back (`B_BUFFER_TICKS`): tap 5 ticks before it ends
        let (mut kb_from, mut tapped) = (None, None);
        let (sim, r) = play(&s, |k, sim| {
            let kb = matches!(sim.mario.action, ACT_SOFT_BACKWARD_GROUND_KB | ACT_BACKWARD_GROUND_KB | ACT_HARD_BACKWARD_GROUND_KB | ACT_BACKWARD_AIR_KB | ACT_HARD_BACKWARD_AIR_KB);
            if kb && kb_from.is_none() {
                kb_from = Some(k);
            }
            if kb && tapped.is_none() && kb_from.is_some_and(|f| k >= f + 17) {
                tapped = Some(k);
                return In { down: B_BUTTON, pressed: B_BUTTON, ..Default::default() };
            }
            In::default()
        })
        .unwrap();
        let t = tapped.unwrap_or_else(|| panic!("{age:?}: the Grublin never touched Link: {:?} acts {:?}", r.acts, r.link_acts));
        assert!(r.link_hurt >= jumpman_sim::link::items::HEART as f64, "{age:?}: lost {} health units of Link's hearts", r.link_hurt);
        let swing = r.swings.first().unwrap_or_else(|| panic!("{age:?}: the B pressed in the knock-back at tick {t} was lost: {:?}", r.link_acts));
        assert!(swing.0 > t && swing.0 < t + 60, "{age:?}: tapped at {t}, swung at {}", swing.0);
        let _ = sim;
    }
}

/// The notes, Jiggies and honeycombs lie on the ground for Link too: he takes them by walking over them (a note is a rupee on his counter,
/// a honeycomb heals), the egg and feather ammo stays Banjo's.
#[test]
fn link_picks_up_notes_jiggies_and_honeycombs_but_not_banjos_ammo() {
    use jumpman_sim::banjo_ext::PickupKind;
    let mut sim = mumbos();
    sim.as_link = true;
    sim.mario.link.enable(Age::Adult);
    for _ in 0..4 {
        sim.tick(&idle());
    }
    let p = sim.mario.pos;
    for (n, kind) in [PickupKind::Note, PickupKind::Egg, PickupKind::Jiggy, PickupKind::RedFeather, PickupKind::Honeycomb].into_iter().enumerate() {
        sim.mario.banjo.drop_pickup(kind, [p[0] + 10.0 * n as f64, p[1] + 40.0, p[2]]);
    }
    sim.mario.link.inv.rupees = 10;
    let (eggs, feathers, rupees) = (sim.mario.banjo.eggs, sim.mario.banjo.red_feathers, sim.mario.link.inv.rupees);
    sim.mario.link.inv.health = 3 * jumpman_sim::link::items::HEART;
    for _ in 0..4 {
        sim.tick(&idle());
    }
    let b = &sim.mario.banjo;
    assert_eq!((b.notes, b.jiggies), (1, 1));
    assert_eq!((b.eggs, b.red_feathers), (eggs, feathers), "the ammo is Banjo's");
    assert!(b.pickups.iter().all(|k| matches!(k.kind, PickupKind::Egg | PickupKind::RedFeather)), "the egg and the feather are still there: {:?}", b.pickups.len());
    assert_eq!(sim.mario.link.inv.health, 4 * jumpman_sim::link::items::HEART, "the honeycomb heals one heart");
    assert_eq!(sim.mario.link.inv.rupees, rupees + 1, "the note is a rupee on Link's counter");
}

fn floor_of(sim: &GameSim) -> Option<f64> {
    let p = sim.mario.pos;
    let f = sim.mario.world.find_floor(p[0], p[1] + 80.0, p[2]);
    f.surface.as_ref().map(|_| f.height)
}

/// Where Link ends up walking eight ways for fifteen seconds from the spawn of a BK world: how far he got in the best direction, and whether
/// he fell to his death on any bearing. `who` is `link`, `mario` or `banjo`.
fn walked(id: &str, who: &str) -> (f64, usize) {
    let mut best: f64 = 0.0;
    let mut died = 0;
    for dir in 0..8 {
        let mut sim = GameSim::load(&assets(), CourseRequest::new(id)).unwrap();
        sim.as_banjo = who == "banjo";
        sim.as_link = who == "link";
        if sim.as_link {
            sim.mario.link.enable(Age::Adult);
        }
        // some maps start high above their floor (Clanker's belly): let him land
        for _ in 0..200 {
            sim.tick(&idle());
        }
        let start = sim.mario.pos;
        let spawn_floor = floor_of(&sim);
        let swimming = sim.mario.action & ACT_GROUP_MASK == ACT_GROUP_SUBMERGED;
        assert!(swimming || spawn_floor.is_some_and(|f| (f - start[1]).abs() < 60.0), "{id} {who}: starts at {:?} with the floor at {spawn_floor:?}", start);
        let a = dir as f64 * std::f64::consts::FRAC_PI_4;
        let (sx, sy) = (a.sin() * 64.0, a.cos() * 64.0);
        let mut dead = false;
        for t in 0..450 {
            let press = if t % 97 == 96 { A_BUTTON } else { 0 };
            sim.tick(&ControllerSnapshot { stick_x: sx, stick_y: sy, stick_mag: 64.0, button_down: press, button_pressed: press, ..Default::default() });
            let m = &sim.mario;
            assert!(m.pos.iter().all(|v| v.is_finite()), "{id} {who}");
            dead |= m.health < 256.0 || m.pos[1] < m.death_height + 50.0;
            best = best.max(((m.pos[0] - start[0]).powi(2) + (m.pos[2] - start[2]).powi(2)).sqrt());
        }
        died += dead as usize;
    }
    (best, died)
}

/// Link starts on the floor of every BK world and can walk away from the spawn wherever Banjo can (two maps are closed rooms for everyone: they
/// are entered from a warp, not the default spawn); a few are cliff-edge starts where walking blindly off the edge is a fall for Link and Banjo alike.
#[test]
fn link_starts_on_the_floor_and_walks_out_of_every_banjo_world_that_banjo_can() {
    for id in worlds() {
        let (link, link_died) = walked(id, "link");
        let (banjo, banjo_died) = walked(id, "banjo");
        assert!((link < 150.0) == (banjo < 150.0), "{id}: Link walked {link:.0} units, Banjo {banjo:.0}");
        assert!(link_died <= banjo_died + 1, "{id}: Link died on {link_died} of 8 bearings, Banjo on {banjo_died}");
    }
}

// ---- diagnostics ----

/// Every baddie kind, both ages, three distances, with and without the Z-lock.
#[test]
#[ignore]
fn probe_sword() {
    for kind in [Kind::Grublin, Kind::Termite, Kind::Bigbutt, Kind::Snippet, Kind::Shrapnel, Kind::Nipper, Kind::Flibbit, Kind::Buzzbomb, Kind::Clucker, Kind::GrublinHood, Kind::Snarebear, Kind::Bat, Kind::TeeHee, Kind::Limbo, Kind::SirSlush, Kind::GrilleChompa, Kind::Slappa] {
        for age in [Age::Adult, Age::Child] {
            for dist in [150.0, 220.0, 300.0] {
                for z in [false, true] {
                    let s = Setup { z, ..Setup::new(kind, age, dist) };
                    match play(&s, finger(300.0, 100.0, 3)) {
                        None => println!("{kind:?} {age:?}: not in {}", world_of(kind)),
                        Some((_, r)) => println!("{kind:12?} {age:5?} d{dist:3} z{} swings {} fin {} hits {} struck {} hp {} gone {:?} hurt {} locked {} acts {:?}", z as u8, r.swings.len(), r.finisher() as u8, r.hits.len(), r.struck, r.hp, r.gone_at, r.link_hurt, r.locked as u8, r.acts),
                    }
                }
            }
        }
    }
}

/// Link / Mario / Banjo walking every world (`LM_WORLD=bk_x` for one).
#[test]
#[ignore]
fn probe_walk() {
    for id in worlds() {
        if std::env::var("LM_WORLD").is_ok_and(|o| o != id) {
            continue;
        }
        let row: Vec<String> = ["link", "mario", "banjo"].iter().map(|w| format!("{w} {:.0}/{}", walked(id, w).0, walked(id, w).1)).collect();
        println!("{id:34} {}", row.join("  "));
    }
}

/// Where to stand for a capture: a spot `dist` units from each baddie of `kind` on its floor, facing it (teleport coordinates and yaw).
#[test]
#[ignore]
fn probe_place() {
    let kind = std::env::var("LM_KIND").ok().and_then(|k| ALL_KINDS_NAMED.iter().find(|(n, _)| *n == k).map(|(_, k)| *k)).unwrap_or(Kind::Grublin);
    let dist: f64 = std::env::var("LM_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(500.0);
    let sim = GameSim::load(&assets(), CourseRequest::new(world_of(kind))).unwrap();
    for (n, o) in sim.mario.objects.objects.iter().enumerate().filter(|(_, o)| o.behavior == ObjectBehaviorKind::BkEnemy && kind_of(o) == kind) {
        let ef = sim.mario.world.find_floor(o.pos[0], o.pos[1] + 300.0, o.pos[2]);
        for bearing in (0..360).step_by(45) {
            let a = (bearing as f64).to_radians();
            let (x, z) = (o.pos[0] + a.sin() * dist, o.pos[2] + a.cos() * dist);
            let f = sim.mario.world.find_floor(x, ef.height + 200.0, z);
            if f.surface.is_none() || (f.height - ef.height).abs() > 80.0 {
                continue;
            }
            // clear line of sight: floor stays near on the way
            let ok = (1..8).all(|i| {
                let t = i as f64 / 8.0;
                let g = sim.mario.world.find_floor(o.pos[0] + (x - o.pos[0]) * t, ef.height + 200.0, o.pos[2] + (z - o.pos[2]) * t);
                g.surface.is_some() && (g.height - ef.height).abs() < 80.0
            });
            if ok {
                // yaw from +z towards +x that faces the enemy
                let yaw = (o.pos[0] - x).atan2(o.pos[2] - z).to_degrees();
                println!("{kind:?} #{n} at [{:.1},{:.1},{:.1}] -> tp={:.1}/{:.1}/{:.1}/{:.0}", o.pos[0], ef.height, o.pos[2], x, f.height + 5.0, z, yaw);
            }
        }
    }
}

const ALL_KINDS_NAMED: [(&str, Kind); 10] = [("grublin", Kind::Grublin), ("termite", Kind::Termite), ("bigbutt", Kind::Bigbutt), ("snippet", Kind::Snippet), ("flibbit", Kind::Flibbit), ("hood", Kind::GrublinHood), ("limbo", Kind::Limbo), ("buzzbomb", Kind::Buzzbomb), ("clucker", Kind::Clucker), ("slush", Kind::SirSlush)];
