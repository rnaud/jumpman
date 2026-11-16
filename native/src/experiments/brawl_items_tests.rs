//! Gem Clash's SSB64 items: spawning, pick-up, swings / shots / blasts with SSB64 knockback, food and the Star.
use super::super::smash::tests::duel;
use super::*;
use jumpman_sim::smash::items::ItemKind as SItem;

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

/// With `smash_items` a share of the drops are SSB64 items riding Gem Clash's mechanics; off by default.
#[test]
fn smash_items_spawn_in_gem_clash() {
    let mut b = Brawl::new(Config { seed: 4, humans: 0, bots: 2, smash_items: true, ..Default::default() });
    let mut seen = std::collections::HashSet::new();
    for _ in 0..30 * 120 {
        b.tick(&idle());
        for it in b.items.iter().filter(|i| i.alive) {
            if let Some(s) = it.ssb {
                assert_eq!(it.kind, base_kind(s));
                seen.insert(s);
            }
        }
    }
    assert!(seen.len() >= 3, "SSB64 items dropped: {seen:?}");
    let mut b = Brawl::new(Config { seed: 4, humans: 0, bots: 2, ..Default::default() });
    for _ in 0..30 * 60 {
        b.tick(&idle());
        assert!(b.items.iter().all(|i| i.ssb.is_none()));
    }
}

fn give(b: &mut Brawl, i: usize, s: SItem) -> usize {
    let p = b.f[i].pos;
    let k = b.spawn_item(ItemKind::Crate, p, false).unwrap();
    b.make_smash_item(k, s);
    k
}

/// Any character picks an SSB64 weapon up (here plain Mario, not a Smash fighter) and its swing deals the item's damage
/// with SSB64 knockback; a Ray Gun shot hits from range.
#[test]
fn a_beam_sword_swing_and_a_ray_gun_shot_hit_the_foe() {
    let mut b = duel(SmashFighter::Fox, None, 150.0, false);
    place(&mut b, 0, [-500.0, 0.0]);
    let k = give(&mut b, 1, SItem::BeamSword);
    let mut inp = idle();
    inp[1].grab = true;
    b.tick(&inp);
    assert_eq!(b.f[1].held, k as i8, "picked up");
    place(&mut b, 0, [0.0, 0.0]);
    place(&mut b, 1, [150.0, 0.0]);
    b.f[1].yaw = yaw_of([-1.0, 0.0]);
    let mut inp = idle();
    inp[1].attack = true;
    b.tick(&inp);
    assert_eq!(b.f[1].act, Act::Swing);
    for _ in 0..12 {
        b.tick(&idle());
    }
    assert!((b.f[0].hp - (b.f[0].max_hp - 14.0)).abs() < 0.6, "the sword's 14%: {}", b.f[0].hp);
    assert!(b.f[0].kvel != [0.0; 3] || b.f[0].pos[0] < -1.0, "launched");
    // Ray Gun
    let mut b = duel(SmashFighter::Mario, None, 600.0, false);
    let k = give(&mut b, 0, SItem::RayGun);
    let mut inp = idle();
    inp[0].grab = true;
    b.tick(&inp);
    assert_eq!(b.f[0].held, k as i8);
    b.f[0].yaw = yaw_of([1.0, 0.0]);
    let mut inp = idle();
    inp[0].attack = true;
    b.tick(&inp);
    let mut hit = false;
    for _ in 0..20 {
        b.tick(&idle());
        place(&mut b, 1, [600.0, 0.0]);
        hit |= b.f[1].hp < b.f[1].max_hp;
    }
    assert!(hit, "the shot hit");
}

/// Maxim Tomato heals half, the Heart Container all, the Star makes the toucher invincible; a Bob-omb's blast launches.
#[test]
fn food_star_and_bobomb() {
    let mut b = duel(SmashFighter::Mario, None, 500.0, false);
    b.f[0].hp = 20.0;
    give(&mut b, 0, SItem::MaximTomato);
    b.tick(&idle());
    b.tick(&idle());
    assert!((b.f[0].hp - 70.0).abs() < 0.1, "half healed: {}", b.f[0].hp);
    give(&mut b, 0, SItem::Heart);
    b.tick(&idle());
    b.tick(&idle());
    assert_eq!(b.f[0].hp, b.f[0].max_hp);
    give(&mut b, 0, SItem::Star);
    b.tick(&idle());
    b.tick(&idle());
    assert!(b.f[0].inv > 200, "invincible");
    let k = give(&mut b, 1, SItem::BobOmb);
    b.items[k].fuse = 1;
    place(&mut b, 0, [420.0, 0.0]);
    b.f[0].inv = 0;
    b.tick(&idle());
    b.tick(&idle());
    assert!(b.f[0].hp < b.f[0].max_hp, "the blast hurt");
}

/// Bots pick SSB64 items up and use them (they ride the mechanics the bots already know).
#[test]
fn bots_use_smash_items() {
    let mut b = Brawl::new(Config { seed: 12, humans: 0, bots: 3, smash_items: true, ..Default::default() });
    let mut held = 0;
    for _ in 0..30 * 150 {
        b.tick(&idle());
        held += (0..b.n).filter(|&i| b.f[i].held >= 0 && b.items[b.f[i].held as usize].ssb.is_some()).count();
        if b.phase == Phase::MatchEnd {
            break;
        }
    }
    assert!(held > 30, "bots held SSB64 items for {held} fighter-ticks");
}
