//! Movement and combat feel for Link and Banjo that follows the simulation's state instead of hooking the
//! controllers: footstep puffs, water plunges, hard-landing rings; for Banjo also the blows he lands on enemies
//! (sparks, a ring, a few frames of hit-stop and a small camera shake), attack and flap puffs, the hurt flash
//! and the Jiggy flourish. Presentation only: nothing here writes to the simulation.
//!
//! Hit-stop, camera shake and the screen flashes are the ones of `link::combat_fx` (they run for every
//! character); particles are the shared pools, so every effect is bounded.

use crate::fx::particles::{BurstOptions, Particles};
use crate::fx::pickup_fx::PickupFx;
use crate::game::runner::GameSim;
use crate::link::combat_fx::{CombatFx, Ring};
use crate::render::to_bevy;
use bevy::prelude::*;
use jumpman_sim::bk_enemy::{kind_of, Kind};
use jumpman_sim::characters::{banjo_move, BANJO_BARGE, BANJO_CLAW, BANJO_FLAP, BANJO_ROLL};
use jumpman_sim::object::ObjectBehaviorKind;
use jumpman_sim::state::ACT_FLAG_AIR;

pub struct HeroFeelPlugin;

impl Plugin for HeroFeelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ground>().init_resource::<BanjoFeel>().add_systems(Update, (ground_fx, banjo_fx));
    }
}

// ---------------------------------------------------------------------------------------------
// Pure model

/// Sim ticks between two footstep puffs.
pub const STEP_TICKS: u64 = 8;

/// Whether a footstep puff is due this tick: on the ground, at a run.
pub fn step_due(grounded: bool, speed: f64, tick: u64) -> bool {
    grounded && speed.abs() > 14.0 && tick % STEP_TICKS == 0
}

/// Whether the feet went through the water surface `level` on the way down between two ticks (no water: never).
pub fn plunged(prev_y: f64, y: f64, level: f64) -> bool {
    level.is_finite() && prev_y > level && y <= level
}

/// The floor ring of a hard landing: its radius (sim units) for the fall speed `vy` (negative down), none for a hop.
pub fn landing_ring(vy: f64) -> Option<f32> {
    (vy < -45.0).then(|| (110.0 + (-vy - 45.0) * 3.0).min(260.0) as f32)
}

/// A blow Banjo landed, read off an enemy's health between two ticks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blow {
    Light,
    Heavy,
    Kill,
}

/// The blow that took an enemy from (`old_hp`, `was_dead`) to (`hp`, `dead`), if any.
pub fn blow(old_hp: f64, hp: f64, was_dead: bool, dead: bool) -> Option<Blow> {
    if dead && !was_dead {
        Some(Blow::Kill)
    } else if dead || was_dead {
        None
    } else if old_hp - hp >= 2.0 {
        Some(Blow::Heavy)
    } else if hp < old_hp {
        Some(Blow::Light)
    } else {
        None
    }
}

/// How a blow reads: spark count, ring radius (sim units), hit-stop (real seconds) and camera shake (bevy units).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Impact {
    pub sparks: usize,
    pub ring: f32,
    pub stop: f32,
    pub shake: f32,
}

pub fn impact(b: Blow) -> Impact {
    match b {
        Blow::Light => Impact { sparks: 7, ring: 80.0, stop: 0.04, shake: 0.012 },
        Blow::Heavy => Impact { sparks: 11, ring: 110.0, stop: 0.06, shake: 0.024 },
        // the defeat puff itself is `bk_enemies::enemy_drops`
        Blow::Kill => Impact { sparks: 9, ring: 130.0, stop: 0.07, shake: 0.028 },
    }
}

/// `centre` moved `out` sim units toward the camera (`eye`, sim units), so an impact drawn there is in front of the
/// body that was struck; `centre` itself without a camera or with the camera inside the body.
pub fn toward_eye(centre: [f64; 3], eye: Option<[f64; 3]>, out: f64) -> [f64; 3] {
    let Some(e) = eye else { return centre };
    let d = [e[0] - centre[0], e[1] - centre[1], e[2] - centre[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len <= out {
        return centre;
    }
    [centre[0] + d[0] / len * out, centre[1] + d[1] / len * out, centre[2] + d[2] / len * out]
}

/// Honeycombs shown for a health value (256 a comb).
pub fn combs(health: f64) -> i32 {
    (health / 256.0).ceil().max(0.0) as i32
}

/// Whether `tick` follows `last` closely enough to compare the two states (not the first frame, a restart or a new course).
pub fn continuous(last: u64, tick: u64) -> bool {
    last != 0 && tick > last && tick - last <= 30
}

// ---------------------------------------------------------------------------------------------
// Systems

#[derive(Resource, Default)]
struct Ground {
    tick: u64,
    y: f64,
    vy: f64,
    air: bool,
}

/// Footsteps, plunges and hard landings of Link and Banjo (Mario's are `fx::juice`).
fn ground_fx(sim: Res<GameSim>, mut g: ResMut<Ground>, mut parts: ResMut<Particles>, mut combat: ResMut<CombatFx>) {
    if !(sim.as_link || sim.as_banjo) {
        g.tick = 0;
        return;
    }
    if sim.ticks == g.tick {
        return;
    }
    let m = &sim.mario;
    let known = continuous(g.tick, sim.ticks);
    g.tick = sim.ticks;
    let air = m.action & ACT_FLAG_AIR != 0;
    let level = m.world.water_level(m.pos[0], m.pos[2]);
    let grounded = !air && m.pos[1] <= m.floor_height + 4.0 && m.pos[1] > level - 10.0;
    if known {
        if step_due(grounded, m.forward_vel, sim.ticks) {
            parts.dust(m.pos, 1);
        }
        if plunged(g.y, m.pos[1], level) && g.vy < -6.0 {
            let at = [m.pos[0], level, m.pos[2]];
            parts.splash(at);
            parts.burst(at, BurstOptions { color: 0xe8f8ff, count: 6, speed: 150.0, life: 0.45, gravity: 700.0, size: 14.0, upward: 260.0, spread: 1.0 });
            combat.rings.push(Ring { pos: [at[0] as f32, level as f32 + 3.0, at[2] as f32], age: 0.0, life: 0.5, r1: 170.0, rgb: [0.8, 0.95, 1.0] });
        }
        if g.air && grounded {
            if let Some(r1) = landing_ring(g.vy) {
                combat.rings.push(Ring { pos: [m.pos[0] as f32, m.pos[1] as f32 + 4.0, m.pos[2] as f32], age: 0.0, life: 0.3, r1, rgb: [1.0, 0.97, 0.88] });
            }
        }
    }
    g.y = m.pos[1];
    g.vy = m.vel[1];
    g.air = air;
}

#[derive(Resource, Default)]
struct BanjoFeel {
    tick: u64,
    /// (object index, hp, dead), in index order.
    enemies: Vec<(usize, f64, bool)>,
    mv: f64,
    combs: i32,
    jiggies: i32,
    form: Option<jumpman_sim::banjo_ext::Form>,
}

/// Banjo's blows, attack puffs, hurt flash, Jiggy flourish and transformation flash.
fn banjo_fx(
    sim: Res<GameSim>,
    mut feel: ResMut<BanjoFeel>,
    mut parts: ResMut<Particles>,
    mut fx: ResMut<PickupFx>,
    mut combat: ResMut<CombatFx>,
    camera: Query<&Transform, With<crate::camera::MainCamera>>,
) {
    let eye = camera.iter().next().map(|t| (t.translation / crate::render::SCALE).as_dvec3().to_array());
    if !sim.as_banjo {
        feel.tick = 0;
        return;
    }
    if sim.ticks == feel.tick {
        return;
    }
    let known = continuous(feel.tick, sim.ticks);
    feel.tick = sim.ticks;
    let m = &sim.mario;
    let body_sim = [m.pos[0], m.pos[1] + 55.0, m.pos[2]];
    let body = to_bevy(body_sim);

    // Blows landed on enemies.
    let mut seen = Vec::with_capacity(feel.enemies.len());
    for (i, o) in m.objects.objects.iter().enumerate() {
        if o.behavior != ObjectBehaviorKind::BkEnemy || kind_of(o) == Kind::Snowball {
            continue;
        }
        let (hp, dead) = (o.var("hp"), o.var("died") != 0.0);
        seen.push((i, hp, dead));
        if !known {
            continue;
        }
        let Ok(k) = feel.enemies.binary_search_by_key(&i, |e| e.0) else { continue };
        let (_, old_hp, was_dead) = feel.enemies[k];
        let Some(b) = blow(old_hp, hp, was_dead, dead) else { continue };
        let look = impact(b);
        // on the camera's side of the body, not inside it where the model hides everything
        let at = toward_eye([o.pos[0], o.pos[1] + 60.0, o.pos[2]], eye, 75.0);
        parts.burst(at, BurstOptions { color: 0xfff4c8, count: look.sparks, speed: 420.0, life: 0.32, gravity: 620.0, size: 24.0, upward: 140.0, spread: 1.0 });
        parts.burst(at, BurstOptions { color: 0xffffff, count: 2, speed: 20.0, life: 0.16, gravity: 0.0, size: 70.0, upward: 0.0, spread: 1.0 });
        if look.ring > 0.0 {
            fx.halo(to_bevy(at), [1.0, 0.97, 0.8], look.ring, 0.22);
            fx.sparkle(to_bevy(at), [1.0, 0.95, 0.6], 3, 220.0, 24.0);
        }
        combat.stop.request(look.stop);
        combat.shake.request(look.shake, 0.2);
    }
    feel.enemies = seen;

    // The start of a claw swipe, a roll, a barge and a flap.
    let mv = banjo_move(m);
    if known && mv != feel.mv {
        let (s, c) = (jumpman_sim::math::sins(m.face_angle[1]), jumpman_sim::math::coss(m.face_angle[1]));
        let ahead = |d: f64, up: f64| [m.pos[0] + s * d, m.pos[1] + up, m.pos[2] + c * d];
        if mv == BANJO_CLAW {
            fx.sparkle(to_bevy(ahead(70.0, 60.0)), [1.0, 1.0, 0.95], 3, 170.0, 22.0);
            parts.dust(m.pos, 1);
        } else if mv == BANJO_ROLL {
            parts.dust(ahead(-20.0, 0.0), 4);
        } else if mv == BANJO_BARGE {
            parts.dust(ahead(-30.0, 0.0), 5);
            fx.halo(to_bevy(ahead(40.0, 55.0)), [1.0, 1.0, 0.95], 90.0, 0.22);
        } else if mv == BANJO_FLAP {
            // feathers' worth of air pushed down
            parts.burst([m.pos[0], m.pos[1] + 20.0, m.pos[2]], BurstOptions { color: 0xf4f0e6, count: 4, speed: 110.0, life: 0.35, gravity: 260.0, size: 24.0, upward: -30.0, spread: 1.0 });
        }
    }
    feel.mv = mv;

    // Honeycombs lost and gained.
    let now = combs(m.health);
    if known && now < feel.combs {
        combat.hurt = combat.hurt.max(0.26);
        combat.shake.request(0.03, 0.25);
        combat.stop.request(0.05);
        parts.burst(body_sim, BurstOptions { color: 0xffd0a0, count: 8, speed: 240.0, life: 0.3, gravity: 500.0, size: 20.0, upward: 110.0, spread: 1.0 });
        fx.halo(body, [1.0, 0.55, 0.4], 120.0, 0.28);
    } else if known && now > feel.combs {
        fx.sparkle(body, [1.0, 0.8, 0.3], 6, 200.0, 26.0);
        fx.halo(body, [1.0, 0.8, 0.3], 100.0, 0.3);
    }
    feel.combs = now;

    // A Jiggy: bigger than anything else he picks up.
    let jiggies = m.banjo.jiggies;
    if known && jiggies > feel.jiggies {
        let gold = [1.0, 0.86, 0.3];
        let above = body + Vec3::Y * 0.5;
        fx.sparkle(above, gold, 16, 420.0, 40.0);
        fx.sparkle(body, [1.0, 1.0, 0.9], 8, 240.0, 28.0);
        fx.halo(above, [1.0, 1.0, 0.9], 110.0, 0.25);
        fx.halo(above, gold, 230.0, 0.45);
        fx.halo(above, gold, 380.0, 0.7);
        parts.burst(body_sim, BurstOptions { color: 0xffdc50, count: 16, speed: 320.0, life: 0.7, gravity: 420.0, size: 22.0, upward: 320.0, spread: 1.0 });
        combat.rings.push(Ring { pos: [m.pos[0] as f32, m.pos[1] as f32 + 4.0, m.pos[2] as f32], age: 0.0, life: 0.55, r1: 300.0, rgb: gold });
        combat.white = combat.white.max(0.22);
        combat.stop.request(0.09);
        combat.shake.request(0.018, 0.3);
    }
    feel.jiggies = jiggies;

    // Mumbo's transformation: the screen blinks white with the puff.
    let form = Some(m.banjo.form);
    if known && feel.form.is_some() && form != feel.form {
        combat.white = combat.white.max(0.4);
        combat.shake.request(0.02, 0.3);
    }
    feel.form = form;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impacts_are_drawn_on_the_camera_side_of_the_body() {
        assert_eq!(toward_eye([0.0, 0.0, 0.0], Some([0.0, 0.0, 500.0]), 75.0), [0.0, 0.0, 75.0]);
        assert_eq!(toward_eye([10.0, 20.0, 30.0], None, 75.0), [10.0, 20.0, 30.0]);
        assert_eq!(toward_eye([0.0, 0.0, 0.0], Some([0.0, 40.0, 0.0]), 75.0), [0.0, 0.0, 0.0], "a camera inside the body leaves it");
    }

    #[test]
    fn footsteps_only_at_a_run_on_the_ground_and_spaced_out() {
        let due = (0..64).filter(|t| step_due(true, 30.0, *t)).count();
        assert_eq!(due, 8, "four a second at 30 Hz, give or take");
        assert!(!step_due(false, 30.0, 0) && !step_due(true, 5.0, 0));
        assert!(step_due(true, -30.0, 16), "backwards counts");
    }

    #[test]
    fn a_plunge_needs_water_and_a_crossing_on_the_way_down() {
        assert!(plunged(10.0, -5.0, 0.0));
        assert!(!plunged(-5.0, 10.0, 0.0), "surfacing is not a plunge");
        assert!(!plunged(-5.0, -8.0, 0.0) && !plunged(20.0, 10.0, 0.0));
        assert!(!plunged(10.0, -5.0, f64::NEG_INFINITY), "no water here");
    }

    #[test]
    fn only_hard_landings_ring_and_the_ring_is_bounded() {
        assert_eq!(landing_ring(-20.0), None);
        let (a, b) = (landing_ring(-50.0).unwrap(), landing_ring(-70.0).unwrap());
        assert!(a < b && b <= 260.0);
        assert_eq!(landing_ring(-1000.0), Some(260.0));
    }

    #[test]
    fn blows_are_read_off_the_health() {
        assert_eq!(blow(3.0, 2.0, false, false), Some(Blow::Light));
        assert_eq!(blow(3.0, 1.0, false, false), Some(Blow::Heavy));
        assert_eq!(blow(1.0, 0.0, false, true), Some(Blow::Kill));
        assert_eq!(blow(2.0, 2.0, false, false), None);
        assert_eq!(blow(0.0, 0.0, true, true), None, "a corpse is not hit again");
        assert_eq!(blow(1.0, 3.0, false, false), None, "a slot reused by a fresh enemy");
        // heavier blows hold and shake more, all within the hit-stop cap
        let (l, h, k) = (impact(Blow::Light), impact(Blow::Heavy), impact(Blow::Kill));
        assert!(l.stop < h.stop && h.stop < k.stop && k.stop <= 0.12);
        assert!(l.shake < h.shake && h.shake < k.shake && k.shake < 0.05);
        assert!(l.sparks <= 16 && h.sparks <= 16 && k.sparks <= 16);
    }

    #[test]
    fn combs_and_continuity() {
        assert_eq!((combs(0.0), combs(1.0), combs(256.0), combs(257.0), combs(-5.0)), (0, 1, 1, 2, 0));
        assert!(continuous(10, 11) && continuous(10, 40));
        assert!(!continuous(0, 1) && !continuous(10, 10) && !continuous(10, 3) && !continuous(10, 400));
    }
}
