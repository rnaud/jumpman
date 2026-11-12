//! Where a currency skin (a rupee, a note or a procedural coin) sits relative to the collectible it stands for, in every world.
//!
//! The skins are generated shapes whose mesh is centred on its own origin (`pickup_fx::shape_tris`: the bounding box of a
//! gem, a note and a disc is centred on the origin and its half-height is exactly one unit), so the placement is a pure
//! function of the look and of what the world's `pos` means ([`Slot`]):
//!
//! * [`Slot::Hover`]: `pos` is the ORIGIN of the original item, which hovers `centre` above it ([`SM64_COIN`]: the coin model
//!   `actors/coin` is 64 tall with its origin at the bottom, so the original centre is 32 above `pos`; [`VILLAGE`]: the real OoT
//!   rupee model of the Kokiri village runs from 7 to 92 above its origin). The skin's centre is the original's centre; its half
//!   height is `half`, so its lowest point is `centre - half >= CLEARANCE` above the origin. These items do not bob.
//! * [`Slot::Centre`]: OoT drops and BK notes: `pos` is the centre of the item, `rest` units above the floor it lies on (a drop
//!   is 20 above, a placed collectible 30, a BK note is measured against the floor under it). The skin keeps its world size and
//!   is lifted until its lowest point is [`CLEARANCE`] above the floor, then bobs UP from there by at most [`BOB`].
//!
//! The skin always turns about the world's vertical axis through its centre (rupee and note like in OoT / BK; the billboard
//! of an SM64 coin view is counter-rotated, see [`skin_local`]). The maths is engine-free and unit tested.
use super::pickup_fx::{Look, Shape};
use bevy::prelude::*;
use std::f32::consts::TAU;

/// How far above the floor the lowest point of a skin stays (sim units).
pub const CLEARANCE: f32 = 6.0;
/// Peak of the upward bob of a free skin (sim units).
pub const BOB: f32 = 4.0;
/// Radians per second of the turn of a skin.
pub const SPIN: f32 = 2.6;

/// What the world's position of a collectible means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    /// `pos` is the origin of the original item, centred `centre` above it; the skin is `half` tall from its own centre.
    Hover { centre: f32, half: f32, bob: f32 },
    /// `pos` is the centre of the item, `rest` above the floor it lies on.
    Centre { rest: f32 },
}

/// SM64 coin objects: placed, loot, moving, bag and blue coins, rings. The coin model is y 0..64 above the object's position.
pub const SM64_COIN: Slot = Slot::Hover { centre: 32.0, half: 26.0, bob: 0.0 };
/// The rupees of the Kokiri village: the real model (`export-oot-courses.py`, `rupee_green.glb`) is y 7..92 above the actor.
pub const VILLAGE: Slot = Slot::Hover { centre: 49.5, half: 36.0, bob: 0.0 };
/// OoT drops (`zelda_enemies::step_drops` rests them 20 above the floor).
pub const DROP: Slot = Slot::Centre { rest: 20.0 };
/// OoT placed collectibles (`place_collectibles` puts them 30 above the floor).
pub const PLACED: Slot = Slot::Centre { rest: 30.0 };

/// The families of currency a theme can draw: rupees, notes and the procedural coin.
pub fn is_currency(look: Look) -> bool {
    matches!(look.shape(), Shape::Gem | Shape::Note) || matches!(look, Look::CoinYellow | Look::CoinRed | Look::CoinBlue)
}

/// Where and how big one skin is, in sim units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Sim units per mesh unit: the skin's half-height.
    pub half: f32,
    /// The skin's centre above the collectible's `pos` (at the bottom of its bob).
    pub lift: f32,
    /// Peak of the upward bob.
    pub bob: f32,
}

impl Placement {
    pub fn of(look: Look, slot: Slot) -> Placement {
        match slot {
            Slot::Hover { centre, half, bob } => Placement { half, lift: centre, bob },
            Slot::Centre { rest } => {
                let half = look.size();
                Placement { half, lift: (CLEARANCE + half - rest).max(0.0), bob: BOB }
            }
        }
    }

    /// Height of the skin's centre above `pos` when the bob is at `phase` (radians); never below `lift`.
    pub fn centre_above(&self, phase: f32) -> f32 {
        self.lift + self.bob * 0.5 * (1.0 + phase.sin())
    }
}

/// The turn of a skin at time `t` (seconds) about the vertical axis; `phase` staggers neighbours.
pub fn spin(t: f32, phase: f32) -> f32 {
    t * SPIN + phase
}

/// The local transform of the skin child of a coin view whose own rotation is `parent_rot` (the camera's, for a billboarded
/// coin) and whose own scale is uniform: the offset and the turn are counter-rotated so that, in the world, the centre is
/// straight above the view's origin and the shape turns about the vertical axis whatever the camera does. The view's scale
/// (a loot coin is smaller) multiplies the offset like it does the original coin's.
pub fn skin_local(look: Look, slot: Slot, t: f32, parent_rot: Quat) -> Transform {
    let p = Placement::of(look, slot);
    let inv = parent_rot.inverse();
    Transform {
        translation: inv * Vec3::Y * (p.centre_above(0.0) * crate::render::SCALE),
        rotation: inv * Quat::from_rotation_y(spin(t, 0.0)),
        scale: Vec3::splat(p.half * crate::render::SCALE),
    }
}

/// The world transform of a free-standing skin whose collectible is at `at` (bevy units), `age` seconds after it appeared.
pub fn skin_world(look: Look, slot: Slot, at: Vec3, t: f32, age: f32) -> Transform {
    let phase = stagger(at);
    let p = Placement::of(look, slot);
    Transform {
        translation: at + Vec3::Y * (p.centre_above(t * 2.4 + phase) * crate::render::SCALE),
        rotation: Quat::from_rotation_y(spin(t, phase)),
        scale: Vec3::splat((p.half * crate::render::SCALE * super::pickup_fx::pop_in(age / 0.32)).max(1e-4)),
    }
}

/// The idle transform of any collectible: a currency skin is anchored by its [`Slot`], everything else keeps the old bob.
pub fn idle(look: Look, slot: Slot, at: Vec3, t: f32, age: f32) -> Transform {
    if is_currency(look) {
        skin_world(look, slot, at, t, age)
    } else {
        super::pickup_fx::idle_transform(look, at, t, age)
    }
}

/// A per-position phase so that neighbours are out of step.
pub fn stagger(at: Vec3) -> f32 {
    let phase = (at.x * 12.9898 + at.z * 78.233).sin() * 43758.547;
    (phase - phase.floor()) * TAU
}

/// How far above the floor a collectible's centre is when its `pos` is `pos_y` over a floor at `floor_y` (sim units): the `rest`
/// of a [`Slot::Centre`]. A floor that is not there (a fall into the void) counts as resting on the item's own half-height.
pub fn rest_over(pos_y: f32, floor_y: f32, has_floor: bool, look: Look) -> f32 {
    if has_floor {
        (pos_y - floor_y).max(0.0)
    } else {
        look.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::pickup_fx::{shape_tris, LOOKS};

    fn skins() -> Vec<Look> {
        LOOKS.iter().copied().filter(|l| is_currency(*l)).collect()
    }

    /// Lowest and highest y of the look's mesh scaled by `half` and turned by `angle` about Y.
    fn y_range(look: Look, half: f32, angle: f32) -> (f32, f32) {
        let q = Quat::from_rotation_y(angle);
        let t = shape_tris(look.shape());
        let ys: Vec<f32> = t.pos.iter().map(|p| (q * Vec3::from(*p) * half).y).collect();
        (ys.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max))
    }

    #[test]
    fn there_are_skins_of_all_three_families() {
        let s = skins();
        assert_eq!(s.len(), 10, "{s:?}");
        assert!(s.contains(&Look::Note) && s.contains(&Look::RupeeSilver) && s.contains(&Look::CoinBlue));
    }

    #[test]
    fn every_skin_mesh_is_centred_on_its_anchor_with_unit_half_height() {
        for l in skins() {
            let t = shape_tris(l.shape());
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for p in &t.pos {
                lo = lo.min(Vec3::from(*p));
                hi = hi.max(Vec3::from(*p));
            }
            let (c, h) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
            assert!(c.abs().max_element() < 0.01, "{l:?} is centred on {c:?}");
            assert!((h.y - 1.0).abs() < 0.01, "{l:?} half-height {}", h.y);
            // never wider than a coin (the note: its flag), never thicker than 0.4 of its height
            assert!(h.x <= 1.0 + 0.01 && h.z <= 0.4, "{l:?} extents {h:?}");
        }
    }

    #[test]
    fn the_hover_slots_keep_the_originals_centre_and_clear_the_floor_at_every_angle() {
        for (slot, centre, top) in [(SM64_COIN, 32.0, 64.0), (VILLAGE, 49.5, 92.0)] {
            for l in skins() {
                let p = Placement::of(l, slot);
                // the same centre as the original model
                assert_eq!(p.lift, centre, "{slot:?}");
                assert_eq!(p.bob, 0.0);
                for k in 0..64 {
                    let (lo, hi) = y_range(l, p.half, k as f32 / 64.0 * TAU);
                    assert!(p.lift + lo >= CLEARANCE - 1e-3, "{l:?} in {slot:?} dips to {} at {k}", p.lift + lo);
                    assert!(p.lift + hi <= top + 1e-3, "{l:?} pokes out of the original's {top}: {}", p.lift + hi);
                }
            }
        }
        // a skin of the SM64 slot is about the height of the coin (64) it replaces
        assert!(2.0 * Placement::of(Look::RupeeBlue, SM64_COIN).half >= 48.0);
    }

    #[test]
    fn centred_skins_stay_above_the_floor_through_their_whole_bob() {
        for l in skins() {
            for rest in [10.0, 20.0, 24.0, 30.0, 60.0] {
                let slot = Slot::Centre { rest };
                let p = Placement::of(l, slot);
                for k in 0..64 {
                    let (lo, _) = y_range(l, p.half, k as f32 / 64.0 * TAU);
                    for b in 0..32 {
                        let low = rest + p.centre_above(b as f32 / 32.0 * TAU) + lo;
                        assert!(low >= CLEARANCE - 1e-3, "{l:?} rest {rest}: lowest {low}");
                    }
                }
                // the item keeps its world size, only goes up, and bobs up only
                assert_eq!(p.half, l.size());
                assert!(p.lift >= 0.0 && p.bob > 0.0);
                assert!((0..64).all(|b| p.centre_above(b as f32 / 64.0 * TAU) >= p.lift - 1e-6));
            }
            // the centre of a drop is the world's own position plus the clearance lift only
            assert!(Placement::of(l, DROP).lift < 12.0);
        }
    }

    #[test]
    fn an_item_that_floats_already_is_not_moved() {
        assert_eq!(Placement::of(Look::RupeeGreen, Slot::Centre { rest: 100.0 }).lift, 0.0);
        let p = Placement::of(Look::RupeeGreen, DROP);
        assert!(p.lift > 0.0 && p.lift < 10.0);
    }

    #[test]
    fn skins_clear_a_floor_that_rises_twenty_degrees_around_them() {
        // the floor under the origin rises `slope` per unit sideways in the worst direction: every vertex must stay above it
        let slope = 0.36;
        for l in skins() {
            let t = shape_tris(l.shape());
            // (slot, centre height above the floor under `pos`'s origin at the bottom of the bob)
            let cases = [(SM64_COIN, 32.0), (VILLAGE, 49.5), (DROP, 20.0 + Placement::of(l, DROP).lift), (PLACED, 30.0 + Placement::of(l, PLACED).lift)];
            for (slot, centre) in cases {
                let p = Placement::of(l, slot);
                for k in 0..32 {
                    let q = Quat::from_rotation_y(k as f32 / 32.0 * TAU);
                    for v in &t.pos {
                        let w = q * Vec3::from(*v) * p.half;
                        let floor_y = slope * Vec2::new(w.x, w.z).length();
                        assert!(centre + w.y >= floor_y - 1e-3, "{l:?} {slot:?}: vertex {w:?} under a 20 degree floor");
                    }
                }
            }
        }
    }

    #[test]
    fn a_billboarded_coin_view_keeps_the_skin_upright_over_the_origin() {
        let cam = Quat::from_euler(EulerRot::YXZ, 0.7, -0.5, 0.0);
        let tf = skin_local(Look::RupeeBlue, SM64_COIN, 1.3, cam);
        let world = cam * tf.translation;
        assert!(world.x.abs() < 1e-6 && world.z.abs() < 1e-6, "{world:?}");
        assert!((world.y - 32.0 * crate::render::SCALE).abs() < 1e-6);
        // turned about the vertical axis in the world: Y maps to Y
        let up = (cam * tf.rotation) * Vec3::Y;
        assert!((up - Vec3::Y).length() < 1e-5);
        // and the scale is the skin's half-height
        assert!((tf.scale.x - 26.0 * crate::render::SCALE).abs() < 1e-6);
    }

    #[test]
    fn a_free_skin_sits_above_its_position_and_bobs_up_only() {
        let at = Vec3::new(1.0, 2.0, 3.0);
        let p = Placement::of(Look::RupeeRed, DROP);
        for i in 0..80 {
            let tf = skin_world(Look::RupeeRed, DROP, at, i as f32 * 0.1, 10.0);
            assert!(tf.translation.y - at.y >= p.lift * crate::render::SCALE - 1e-6);
            assert!(tf.translation.y - at.y <= (p.lift + p.bob) * crate::render::SCALE + 1e-6);
            assert!((tf.scale.x - 19.0 * crate::render::SCALE).abs() < 1e-6);
            assert!((tf.rotation * Vec3::Y - Vec3::Y).length() < 1e-5, "stays upright");
        }
    }

    #[test]
    fn rest_is_measured_against_the_floor_under_the_item() {
        assert_eq!(rest_over(150.0, 100.0, true, Look::Note), 50.0);
        assert_eq!(rest_over(90.0, 100.0, true, Look::Note), 0.0);
        assert_eq!(rest_over(90.0, -9000.0, false, Look::Note), 24.0);
    }

    #[test]
    fn the_currency_families_are_the_skins_and_nothing_else() {
        assert!(is_currency(Look::RupeeGold) && is_currency(Look::Note) && is_currency(Look::CoinYellow));
        assert!(!is_currency(Look::Heart) && !is_currency(Look::Egg) && !is_currency(Look::Jiggy) && !is_currency(Look::Token));
    }
}
