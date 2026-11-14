//! Gem Clash heroes: the playable fighters behind `Kind::Mario` (Mario, Link, Young Link, Banjo).
//!
//! Engine-free. A [`Hero`] is a per-fighter style layered over the core: it sets the hit points,
//! speed, power, weight and body size of the player's fighter and the numbers of its moves (the
//! three-hit ground combo, the air dive, the ground pound and, for the two Links, the sword spin).
//! `brawl_core` stays authoritative for position, health and hits. The model, clips and the bridge
//! into the game's character rigs live in `brawl.rs` (`hero_pose`).
//!
//! Mario's style is the baseline the core always had (these numbers reproduce it exactly), so
//! old tests and the determinism digest keep their meaning.

/// One melee blow: its length in ticks, the tick the blade / fist connects, the damage, knockback,
/// pop-up and the forward reach (world units, added to the bodies' radii).
#[derive(Clone, Copy, Debug)]
pub struct Blow {
    pub total: u16,
    pub hit_at: u16,
    pub dmg: f32,
    pub kb: f32,
    pub up: f32,
    pub reach: f32,
}

const fn blow(total: u16, hit_at: u16, dmg: f32, kb: f32, up: f32, reach: f32) -> Blow {
    Blow { total, hit_at, dmg, kb, up, reach }
}

/// The air dive (B in the air): speed, then (damage, knockback, pop-up, reach) of the body blow.
#[derive(Clone, Copy, Debug)]
pub struct Dive {
    pub speed: f32,
    pub dmg: f32,
    pub kb: f32,
    pub up: f32,
    pub reach: f32,
}

/// The ground pound (Z in the air): shock wave radius multiplier, damage, knockback and pop-up.
#[derive(Clone, Copy, Debug)]
pub struct Pound {
    pub radius: f32,
    pub dmg: f32,
    pub kb: f32,
    pub up: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HeroStyle {
    pub name: &'static str,
    pub hp: f32,
    pub speed: f32,
    pub power: f32,
    pub weight: f32,
    /// Body size (radius and height multiplier on the 38 x 150 reference fighter).
    pub scale: f32,
    /// Fit of the game's model to that body (the rigs differ in height).
    pub fit: f32,
    /// The three-hit chain (jab, jab, finisher). Every tap while a blow ends chains the next one.
    pub combo: [Blow; 3],
    /// Distance the fighter steps into each blow.
    pub lunge: [f32; 3],
    pub dive: Dive,
    pub pound: Pound,
    /// Hold B after the first blow: the sword spin (all around). `None` for the brawlers.
    pub spin: Option<Blow>,
    /// Gem power form tint (linear rgb) for the glow and the effects.
    pub tint: [f32; 3],
}

impl HeroStyle {
    /// The 14 ticks (0.47 s) a blow of `step` keeps the fighter busy.
    pub fn blow(&self, step: u8) -> Blow {
        self.combo[(step as usize).min(2)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Hero {
    #[default]
    Mario,
    Link,
    YoungLink,
    Banjo,
}

/// Every hero in picker order (`Hero as usize` indexes it).
pub const HEROES: [Hero; 4] = [Hero::Mario, Hero::Link, Hero::YoungLink, Hero::Banjo];

static MARIO: HeroStyle = HeroStyle {
    name: "Mario",
    hp: 100.0,
    speed: 1.0,
    power: 1.0,
    weight: 1.0,
    scale: 1.0,
    fit: 1.0,
    combo: [blow(9, 3, 6.0, 260.0, 0.0, 105.0), blow(9, 3, 6.0, 300.0, 60.0, 105.0), blow(14, 5, 10.0, 640.0, 240.0, 130.0)],
    lunge: [190.0; 3],
    dive: Dive { speed: 1030.0, dmg: 9.0, kb: 540.0, up: 220.0, reach: 90.0 },
    pound: Pound { radius: 270.0, dmg: 13.0, kb: 430.0, up: 700.0 },
    spin: None,
    tint: [1.0, 0.35, 0.25],
};

// Link: tall, a longer sword reach and heavier blows, but slower on his feet and in his swings; the spin
// is the big move (hold B).
static LINK: HeroStyle = HeroStyle {
    name: "Link",
    hp: 108.0,
    speed: 0.92,
    power: 1.0,
    weight: 1.12,
    scale: 1.1,
    fit: 0.86,
    combo: [blow(11, 3, 7.0, 270.0, 0.0, 140.0), blow(11, 3, 7.0, 310.0, 60.0, 140.0), blow(17, 6, 12.0, 700.0, 260.0, 165.0)],
    lunge: [170.0, 170.0, 230.0],
    dive: Dive { speed: 980.0, dmg: 10.0, kb: 560.0, up: 230.0, reach: 120.0 },
    pound: Pound { radius: 250.0, dmg: 14.0, kb: 450.0, up: 700.0 },
    spin: Some(blow(24, 8, 14.0, 760.0, 300.0, 170.0)),
    tint: [0.35, 1.0, 0.45],
};

// Young Link: small, fast and light, the Kokiri blade is short and weak.
static YOUNG_LINK: HeroStyle = HeroStyle {
    name: "Young Link",
    hp: 92.0,
    speed: 1.14,
    power: 0.94,
    weight: 0.82,
    scale: 0.88,
    fit: 1.06,
    combo: [blow(8, 2, 5.0, 240.0, 0.0, 112.0), blow(8, 2, 5.0, 280.0, 60.0, 112.0), blow(12, 4, 8.0, 560.0, 220.0, 128.0)],
    lunge: [200.0, 200.0, 230.0],
    dive: Dive { speed: 1060.0, dmg: 8.0, kb: 500.0, up: 220.0, reach: 98.0 },
    pound: Pound { radius: 240.0, dmg: 11.0, kb: 400.0, up: 650.0 },
    spin: Some(blow(20, 6, 10.0, 640.0, 260.0, 140.0)),
    tint: [0.45, 0.9, 1.0],
};

// Banjo: the Beak Barge is the strongest dive and Rat-a-tat Rap (the pound) lands hard.
static BANJO: HeroStyle = HeroStyle {
    name: "Banjo",
    hp: 104.0,
    speed: 0.97,
    power: 1.0,
    weight: 1.1,
    scale: 1.0,
    fit: 1.0,
    combo: [blow(9, 3, 6.0, 260.0, 0.0, 108.0), blow(9, 3, 6.0, 300.0, 60.0, 108.0), blow(14, 5, 10.0, 640.0, 240.0, 125.0)],
    lunge: [190.0; 3],
    dive: Dive { speed: 1220.0, dmg: 13.0, kb: 680.0, up: 240.0, reach: 105.0 },
    pound: Pound { radius: 310.0, dmg: 15.0, kb: 470.0, up: 720.0 },
    spin: None,
    tint: [1.0, 0.75, 0.2],
};

impl Hero {
    pub fn style(self) -> &'static HeroStyle {
        match self {
            Hero::Mario => &MARIO,
            Hero::Link => &LINK,
            Hero::YoungLink => &YOUNG_LINK,
            Hero::Banjo => &BANJO,
        }
    }
    pub fn from_character(c: crate::characters::Character) -> Hero {
        use crate::characters::Character as C;
        match c {
            C::Mario => Hero::Mario,
            C::Link => Hero::Link,
            C::YoungLink => Hero::YoungLink,
            C::Banjo => Hero::Banjo,
        }
    }
    pub fn name(self) -> &'static str {
        self.style().name
    }
    pub fn is_link(self) -> bool {
        matches!(self, Hero::Link | Hero::YoungLink)
    }
    /// Per-hero move names shown on the how-to-play card: (B, B in the air, Z in the air).
    pub fn move_names(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Hero::Mario => ("PUNCH, PUNCH, KICK", "DIVE", "GROUND POUND"),
            Hero::Link => ("SWORD COMBO, HOLD TO SPIN", "JUMP SLASH", "SWORD PLUNGE"),
            Hero::YoungLink => ("KOKIRI BLADE COMBO, HOLD TO SPIN", "FLIP SLASH", "SWORD PLUNGE"),
            Hero::Banjo => ("CLAW SWIPE COMBO", "BEAK BARGE", "RAT-A-TAT RAP"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pose bridge for Link (clips are chosen from the sim's `link.pose` hint; see `characters::link_pose`)
// ---------------------------------------------------------------------------------------------

use super::brawl_core::Act;
use jumpman_sim::link::combat::{Direction, Swing};
use jumpman_sim::link::moves::PoseHint;

/// The hint for one swing `t` ticks in: the swing clip (frames count in 30 Hz ticks, the renderer converts), then
/// the `_end` clip at one frame per tick, as the faithful controller does.
pub fn swing_hint(sw: Swing, t: u16) -> PoseHint {
    let i = sw.info();
    let (t, total) = (t as f64, sw.total_ticks());
    match i.end {
        Some((end, _)) if t >= total => PoseHint { clip: end, frame: (t - total) * 1.5, looping: false },
        _ => PoseHint { clip: i.clip, frame: t, looping: false },
    }
}

/// Which of Link's clips plays for a fighter in `act` since `t` ticks. `None`: let the plain stance / walk / jump logic choose.
pub fn link_hint(act: Act, t: u16) -> Option<PoseHint> {
    Some(match act {
        Act::Punch(0) | Act::Shoot => swing_hint(Swing::Forward, t),
        Act::Punch(1) | Act::Toss => swing_hint(Swing::Right, t),
        Act::Punch(_) => swing_hint(Swing::Combo(Direction::Forward), t),
        Act::Spin | Act::Nova => swing_hint(Swing::Spin, t),
        Act::Swing => swing_hint(Swing::Left, t),
        Act::Dive => swing_hint(Swing::FlipSlash, t),
        Act::DiveSlide => swing_hint(Swing::FlipLand, t),
        Act::PoundWind | Act::Pound => swing_hint(Swing::JumpSlash, t),
        Act::PoundLand => swing_hint(Swing::JumpLand, t),
        Act::Beam => PoseHint { clip: "bowReady", frame: (t as f64).min(9.0), looping: false },
        _ => return None,
    })
}
