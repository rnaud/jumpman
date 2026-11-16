//! Gem Clash with Smash fighters: specials on the seat, scripted normal attacks, SSB64 knockback, blast zones, bots.
use super::*;
use jumpman_sim::smash_hitbox::MotionEvents;
use std::sync::Arc;

/// Hand-written scripts in the import's format (no ROM data): Falcon Punch (24 / 25%, 361, growth 120, base 30 on
/// frames 42-46) and Mario's first jab (2% on frames 2-3, growth 50, base 8), as in `sim/tests/smash_specials.rs`.
pub(crate) const SCRIPTS: &str = r#"{"fps":60,"game":"ssb64","fighters":{
  "captain":{"size":1.05,"motions":{"203":{"motion":"SpecialN","end":55,"loops":[],"root_motion":true,"script":0,"events":[],
    "hitboxes":[
     {"id":0,"set":0,"from":42,"to":47,"group":0,"joint":16,"offset":[100,0,0],"size":250,"scaled":false,"damage":24,"angle":361,
      "knockback_scale":120,"knockback_weight":0,"knockback_base":30,"element":0,"shield_damage":0,"hit_ground":true,"hit_air":true,
      "can_rebound":true,"fgm_kind":1,"fgm_level":2,"positions":[[0,250,700],[0,250,720],[0,250,730],[0,250,730],[0,250,730]],"on_joint":true}]}}},
  "mario":{"size":1.12,"motions":{"165":{"motion":"Attack11","end":10,"loops":[],"root_motion":false,"script":0,"events":[],
    "hitboxes":[{"id":0,"set":0,"from":2,"to":4,"group":0,"joint":10,"offset":[0,0,0],"size":160,"scaled":false,"damage":2,"angle":361,
      "knockback_scale":50,"knockback_weight":0,"knockback_base":8,"element":0,"shield_damage":0,"hit_ground":true,"hit_air":true,
      "can_rebound":true,"fgm_kind":0,"fgm_level":0,"positions":[[0,300,250],[0,300,250]],"on_joint":true}]}}}}}"#;

fn idle() -> [Input; MAXF] {
    [Input::default(); MAXF]
}

fn place(b: &mut Brawl, i: usize, xz: V2) {
    let top = b.arena.support(xz[0], xz[1], 1000.0).map_or(0.0, |t| t.0);
    let f = &mut b.f[i];
    f.pos = [xz[0], top, xz[1]];
    f.vel = [0.0; 3];
    f.grounded = true;
    f.act = Act::Idle;
    f.inv = 0;
}

/// Two humans past the countdown, the first a Smash fighter, the second Mario (or another fighter), `gap` apart on +x.
pub(crate) fn duel(a: SmashFighter, b_smash: Option<SmashFighter>, gap: f32, scripts: bool) -> Brawl {
    let mut smash = [None; MAXF];
    smash[0] = Some(a);
    smash[1] = b_smash;
    let mut b = Brawl::new(Config { seed: 3, mode: Mode::Versus, arena: ArenaKind::Bumpers, humans: 2, bots: 0, same_kind: Some(Kind::Mario), smash, ..Default::default() });
    b.set_scripts(if scripts { Some(Arc::new(MotionEvents::from_json(SCRIPTS).unwrap())) } else { None });
    for _ in 0..INTRO_TICKS + 2 {
        b.tick(&idle());
    }
    b.arena.bumpers.clear();
    b.items = [Item::NONE; MAXI];
    b.item_timer = 100_000;
    for g in b.gems.iter_mut() {
        g.state = GemState::Spent;
    }
    place(&mut b, 0, [0.0, 0.0]);
    place(&mut b, 1, [gap, 0.0]);
    b.f[0].yaw = yaw_of([1.0, 0.0]);
    b.f[1].yaw = yaw_of([-1.0, 0.0]);
    b
}

fn special(slot: u8) -> [Input; MAXF] {
    let mut inp = idle();
    inp[0].special = true;
    inp[0].special_held = true;
    inp[0].slot = slot;
    inp[0].mv = [1.0, 0.0];
    inp
}

#[test]
fn smash_fighters_get_a_seat_their_name_and_ssb64_weight() {
    let b = duel(SmashFighter::Purin, None, 300.0, false);
    assert!(b.seats[0].is_some() && b.seats[1].is_none());
    assert_eq!(b.f[0].stats().name, "Jigglypuff");
    assert!((b.f[0].stats().weight - 1.0 / 1.3).abs() < 1e-5, "light in Gem Clash terms");
    assert_eq!(b.f[1].stats().name, "Mario");
}

/// Mario's fireball comes out of the seat, flies after the special ends and burns the foe 400 units away.
#[test]
fn a_fireball_special_hits_the_gem_clash_foe() {
    let mut b = duel(SmashFighter::Mario, None, 400.0, false);
    b.tick(&special(0));
    assert_eq!(b.f[0].act, Act::Special);
    for _ in 0..40 {
        b.tick(&idle());
        place(&mut b, 1, [400.0, 0.0]);
        if b.f[1].hp < b.f[1].max_hp {
            break;
        }
    }
    assert!(b.f[1].hp < b.f[1].max_hp, "the fireball reached the foe");
}

/// Falcon Punch (scripted) launches with SSB64's knockback: further at a higher percent, and it carries a heavier
/// fighter (Donkey Kong) less far than a light one (Jigglypuff).
#[test]
fn falcon_punch_launches_by_percent_and_weight() {
    let launch = |victim: Option<SmashFighter>, hp: f32| {
        let mut b = duel(SmashFighter::Captain, victim, 200.0, true);
        b.f[1].hp = hp;
        b.tick(&special(0));
        for _ in 0..60 {
            b.tick(&idle());
            if b.f[1].kvel != [0.0; 3] {
                return (b.f[1].kvel[0].hypot(b.f[1].kvel[1]), b.f[1].percent());
            }
        }
        panic!("no launch");
    };
    let (low, _) = launch(None, 100.0);
    let (high, _) = launch(None, 40.0);
    assert!(high > low * 1.5, "at 60% the punch sends much further: {low} vs {high}");
    let (dk, _) = launch(Some(SmashFighter::Donkey), 50.0);
    let (puff, _) = launch(Some(SmashFighter::Purin), 50.0);
    assert!(puff > dk, "Jigglypuff flies further than Donkey Kong: {puff} vs {dk}");
}

#[test]
fn a_launched_fighter_beyond_the_blast_zone_is_knocked_out() {
    let mut b = duel(SmashFighter::Mario, None, 200.0, false);
    let z = b.blast_zone();
    b.f[1].launched = 20;
    b.f[1].pos = [z.max[0] + 50.0, 100.0, 0.0];
    b.f[1].grounded = false;
    b.tick(&idle());
    assert!(!b.f[1].alive, "out of the blast zone");
    assert_eq!(b.f[0].kos, 0, "no hit, no credit");
    // not launched: the rim holds the fighter
    let mut b = duel(SmashFighter::Mario, None, 200.0, false);
    if let Some(r) = b.arena.bounds {
        b.f[1].pos = [r[0] + 10.0, 0.0, 0.0];
        b.tick(&idle());
        assert!(b.f[1].alive && b.f[1].pos[0] <= r[0]);
    }
}

/// The punch chain plays the scripted jab: its damage (2%) and knockback, not Mario's 6-point blow.
#[test]
fn the_jab_is_the_scripted_attack11() {
    let mut b = duel(SmashFighter::Mario, None, 90.0, true);
    let mut inp = idle();
    inp[0].attack = true;
    b.tick(&inp);
    for _ in 0..8 {
        b.tick(&idle());
    }
    assert!((b.f[1].hp - (100.0 - 2.0 * super::tuning(SmashFighter::Mario).power)).abs() < 0.01, "a 2% jab: {}", b.f[1].hp);
    // without scripts: Mario's blow
    let mut b = duel(SmashFighter::Mario, None, 90.0, false);
    b.tick(&inp);
    for _ in 0..8 {
        b.tick(&idle());
    }
    assert!((b.f[1].hp - (100.0 - 6.0 * super::tuning(SmashFighter::Mario).power)).abs() < 0.01, "Mario's jab: {}", b.f[1].hp);
}

#[test]
fn stone_is_armour_and_a_hit_cancels_a_special() {
    let mut b = duel(SmashFighter::Kirby, None, 120.0, false);
    b.tick(&special(2));
    for _ in 0..6 {
        b.tick(&idle());
    }
    assert_eq!(b.f[0].act, Act::Special);
    assert!(!b.hit(0, 1, 10.0, [1.0, 0.0], 300.0, 100.0), "Stone shrugs off the blow");
    // Falcon Punch's wind-up is interrupted by a blow
    let mut b = duel(SmashFighter::Captain, None, 120.0, false);
    b.tick(&special(0));
    b.tick(&idle());
    assert!(b.hit(0, 1, 10.0, [1.0, 0.0], 300.0, 100.0));
    b.tick(&idle());
    b.tick(&idle());
    assert!(b.seats[0].as_ref().unwrap().special().is_none(), "cancelled");
}

/// Every fighter's three specials start in Gem Clash and end again (none hangs the fighter).
#[test]
fn every_special_starts_and_ends_in_gem_clash() {
    for f in SmashFighter::ALL {
        for slot in 0..3u8 {
            let mut b = duel(f, None, 600.0, false);
            b.tick(&special(slot));
            let started = b.seats[0].as_ref().unwrap().special().is_some() || b.f[0].act == Act::Special;
            assert!(started || (f == SmashFighter::Donkey && slot == 2), "{f:?} slot {slot} did not start");
            let mut done = false;
            for t in 0..400 {
                let mut inp = idle();
                // charges and holds: let go / press again
                if t == 30 {
                    inp[0].special = true;
                }
                b.tick(&inp);
                if b.f[0].act != Act::Special {
                    done = true;
                    break;
                }
            }
            assert!(done || !b.f[0].alive, "{f:?} slot {slot} still in its special after 400 ticks");
        }
    }
}

/// Smash bots use their specials in a match (and the match finishes).
#[test]
fn smash_bots_use_specials() {
    let mut smash = [None; MAXF];
    smash[0] = Some(SmashFighter::Fox);
    smash[1] = Some(SmashFighter::Ness);
    let mut b = Brawl::new(Config { seed: 9, humans: 0, bots: 2, same_kind: Some(Kind::Mario), smash, rounds_to_win: 1, ..Default::default() });
    let mut specials = 0;
    let mut ticks = 0;
    while b.phase != Phase::MatchEnd && ticks < 30 * 60 * 6 {
        b.tick(&idle());
        ticks += 1;
        specials += (0..2).filter(|&i| b.f[i].act == Act::Special && b.f[i].act_t == 1).count();
    }
    assert!(specials >= 3, "bots used {specials} specials");
}
