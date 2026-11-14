//! Solid bodies in Relic Descent (`Relic::body_block`): every foe that walks, charges or bites at the hero ends at his
//! surface, and the hero walking into a foe stops at it (or shoves a light one aside), while the bites still land. The
//! test hero follows the core's corrected position each tick, as the glue does by shifting Mario by `ev.push`.
use super::*;

fn frame_at(r: &Relic) -> PlayerFrame {
    PlayerFrame { pos: r.player.pos, grounded: true, face: r.player.face, ..Default::default() }
}

fn kinds() -> Vec<EnemyKind> {
    let mut k = vec![EnemyKind::Scamp, EnemyKind::Bruiser, EnemyKind::Lancer, EnemyKind::Sapper, EnemyKind::Seer, EnemyKind::King];
    k.extend_from_slice(relic_foe_kinds());
    k
}

fn tough(r: &mut Relic) {
    r.stats.max_hp *= 1000.0;
    r.player.hp = r.stats.max_hp;
}

#[test]
fn every_foe_stops_at_the_heros_body_and_still_hits() {
    let mut melee_hits = 0;
    for kind in kinds() {
        let mut r = Relic::new(7, Hero::Mario, 2, 0);
        tough(&mut r);
        let c = r.start_pos();
        r.player.pos = c;
        let i = r.spawn_enemy(kind, [c[0], c[1] + 260.0], 0, 0).unwrap();
        r.enemies[i].hp = 1.0e5;
        r.enemies[i].max_hp = 1.0e5;
        let hp0 = r.player.hp;
        let mut worst = 0.0f32;
        for t in 0..450 {
            let f = frame_at(&r);
            r.tick(&f, &Intent::default());
            r.player.iframes = 0;
            let o = r.body_overlap();
            worst = worst.max(o);
            assert!(o < 0.5, "{kind:?} tick {t}: overlaps the hero by {o:.1}");
        }
        if r.player.hp < hp0 {
            melee_hits += 1;
        }
        eprintln!("body relic {kind:?}: worst overlap {worst:.2}, hero lost {:.0}", hp0 - r.player.hp);
    }
    assert!(melee_hits >= kinds().len() * 2 / 3, "most foes still hurt a hero who stands at body contact ({melee_hits})");
}

#[test]
fn the_hero_cannot_walk_into_a_foe() {
    for kind in kinds() {
        if kind == EnemyKind::Wraith {
            continue;
        }
        let mut r = Relic::new(7, Hero::Mario, 2, 0);
        tough(&mut r);
        r.hold_foes = true;
        let c = r.start_pos();
        r.player.pos = c;
        let i = r.spawn_enemy(kind, [c[0], c[1] + 400.0], 0, 0).unwrap();
        r.enemies[i].hp = 1.0e5;
        for t in 0..60 {
            let e = r.enemies[i].pos;
            let to = norm([e[0] - r.player.pos[0], e[1] - r.player.pos[1]]);
            let f = PlayerFrame { pos: [r.player.pos[0] + to[0] * 25.0, r.player.pos[1] + to[1] * 25.0], grounded: true, ..Default::default() };
            r.tick(&f, &Intent::default());
            let o = r.body_overlap();
            assert!(o < 0.5, "{kind:?} tick {t}: the hero walked {o:.1} into it");
        }
        let d = dist(r.player.pos, r.enemies[i].pos);
        assert!(d < r.enemies[i].radius + PLAYER_RADIUS + 40.0 || r.enemies[i].pos != [c[0], c[1] + 400.0], "{kind:?}: the hero reached it ({d:.0})");
    }
}

#[test]
fn the_wraith_is_a_ghost_and_the_dive_passes_through() {
    let mut r = Relic::new(7, Hero::Mario, 2, 0);
    let c = r.start_pos();
    r.player.pos = c;
    let i = r.spawn_enemy(EnemyKind::Wraith, c, 0, 0).unwrap();
    assert!(!r.solid_foe(i));
    let j = r.spawn_enemy(EnemyKind::Scamp, c, 0, 0).unwrap();
    assert!(r.solid_foe(j));
    r.player.dodge_t = 10;
    assert_eq!(r.body_overlap(), 0.0);
}

/// A crowded room at the cap (the busiest scene): bodies stay apart from the hero and the pass costs little.
#[test]
fn a_full_room_presses_on_the_hero_without_overlap_and_the_pass_is_cheap() {
    let mut r = Relic::new(3, Hero::Mario, 1, 0);
    tough(&mut r);
    let c = r.start_pos();
    r.player.pos = c;
    let mut n = 0;
    for k in 0..enemy_cap() {
        let a = k as f32 * 0.61;
        let d = 250.0 + (k % 5) as f32 * 90.0;
        if r.spawn_enemy(EnemyKind::Scamp, [c[0] + a.cos() * d, c[1] + a.sin() * d], 0, 0).is_some() {
            n += 1;
        }
    }
    let mut worst = 0.0f32;
    let t0 = std::time::Instant::now();
    for _ in 0..300 {
        let f = frame_at(&r);
        r.tick(&f, &Intent::default());
        r.player.iframes = 0;
        worst = worst.max(r.body_overlap());
    }
    let per = t0.elapsed().as_secs_f64() * 1e6 / 300.0;
    let t1 = std::time::Instant::now();
    for _ in 0..300 {
        r.body_block();
    }
    let pass = t1.elapsed().as_secs_f64() * 1e6 / 300.0;
    eprintln!("body cost Relic Descent: {n} foes, {per:.1} us/tick, body pass {pass:.2} us, worst overlap {worst:.2}");
    assert!(worst < 0.5, "worst overlap {worst:.1}");
    assert!(pass < 100.0);
}
