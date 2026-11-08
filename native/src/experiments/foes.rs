//! The ported Ocarina of Time and Banjo-Kazooie enemies as a cast for the special game modes
//! (Relic Descent, Gem Clash, Star Swarm, Hold the Gate). Engine-free: this file is the one place
//! that ties the modes to the decomp tables of the course ports (`jumpman_sim::oot_enemy` and
//! `jumpman_sim::bk_enemy`): hit points, touch damage, hurt-box size, model and animation clip names.
//! Each mode scales those numbers to its own units (see `relic_foes.rs`, `brawl_foes.rs`...), so a
//! Peahat is always "6 hits, one heart of touch damage, flying" and a Bigbutt always the heavy one.
//!
//! The view half is `foes_view.rs` (glb scenes for the OoT actors, CPU-skinned rigs for the BK ones).
use jumpman_sim::bk_enemy::{Kind as BkKind, S as BK_S};
use jumpman_sim::oot_enemy::{Drops, Kind as OotKind, U as OOT_U};

/// One ported enemy species. `repr(u8)` ids start at 1 so 0 can mean "none" in view buffers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Species {
    DekuBaba = 1,
    Skulltula,
    GoldSkulltula,
    Octorok,
    MadScrub,
    Tektite,
    Peahat,
    Wolfos,
    Guay,
    Grublin,
    Bigbutt,
    Snippet,
    MutieSnippet,
    SirSlush,
    Ticker,
    Limbo,
    Shrapnel,
    Bat,
}

pub const SPECIES: [Species; 18] = [
    Species::DekuBaba,
    Species::Skulltula,
    Species::GoldSkulltula,
    Species::Octorok,
    Species::MadScrub,
    Species::Tektite,
    Species::Peahat,
    Species::Wolfos,
    Species::Guay,
    Species::Grublin,
    Species::Bigbutt,
    Species::Snippet,
    Species::MutieSnippet,
    Species::SirSlush,
    Species::Ticker,
    Species::Limbo,
    Species::Shrapnel,
    Species::Bat,
];

/// Where a species' model comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    /// `assets/levels/oot_enemies/<stem>.glb` (a scene with animation clips).
    Glb(&'static str),
    /// `public/models/bk/enemies/<key>.json` (CPU-skinned rig).
    Bk(&'static str),
}

/// What an enemy is doing, as far as its animation is concerned.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pose {
    #[default]
    Idle,
    Move,
    Attack,
    Hurt,
    Die,
}

/// Clip names per pose: `[idle, move, attack, hurt, die]`.
type Clips = [&'static str; 5];

impl Species {
    pub fn from_u8(v: u8) -> Option<Species> {
        SPECIES.iter().copied().find(|s| *s as u8 == v)
    }

    fn oot(self) -> Option<OotKind> {
        Some(match self {
            Species::DekuBaba => OotKind::DekuBaba,
            Species::Skulltula => OotKind::Skulltula,
            Species::GoldSkulltula => OotKind::GoldSkulltula,
            Species::Octorok => OotKind::Octorok,
            Species::MadScrub => OotKind::MadScrub,
            Species::Tektite => OotKind::Tektite,
            Species::Peahat => OotKind::Peahat,
            Species::Wolfos => OotKind::Wolfos,
            Species::Guay => OotKind::Guay,
            _ => return None,
        })
    }

    fn bk(self) -> Option<BkKind> {
        Some(match self {
            Species::Grublin => BkKind::Grublin,
            Species::Bigbutt => BkKind::Bigbutt,
            Species::Snippet => BkKind::Snippet,
            Species::MutieSnippet => BkKind::MutieSnippet,
            Species::SirSlush => BkKind::SirSlush,
            Species::Ticker => BkKind::Termite,
            Species::Limbo => BkKind::Limbo,
            Species::Shrapnel => BkKind::Shrapnel,
            Species::Bat => BkKind::Bat,
            _ => return None,
        })
    }

    pub fn is_oot(self) -> bool {
        self.oot().is_some()
    }

    pub fn name(self) -> &'static str {
        match self {
            Species::DekuBaba => "Deku Baba",
            Species::Skulltula => "Skulltula",
            Species::GoldSkulltula => "Gold Skulltula",
            Species::Octorok => "Octorok",
            Species::MadScrub => "Mad Scrub",
            Species::Tektite => "Tektite",
            Species::Peahat => "Peahat",
            Species::Wolfos => "Wolfos",
            Species::Guay => "Guay",
            Species::Grublin => "Grublin",
            Species::Bigbutt => "Bigbutt",
            Species::Snippet => "Snippet",
            Species::MutieSnippet => "Mutie Snippet",
            Species::SirSlush => "Sir Slush",
            Species::Ticker => "Ticker",
            Species::Limbo => "Limbo",
            Species::Shrapnel => "Shrapnel",
            Species::Bat => "Bat",
        }
    }

    pub fn src(self) -> Src {
        match (self.oot(), self.bk()) {
            (Some(k), _) => Src::Glb(k.spec().model),
            (_, Some(k)) => Src::Bk(k.model()),
            _ => unreachable!(),
        }
    }

    /// Hit points in the decomp (hits of a basic weapon: `colChkInfo.health` for the OoT actors, the
    /// `hp` of the BK port). Bigbutt's 99 is "cannot be hurt by an ordinary blow".
    pub fn decomp_hp(self) -> i32 {
        match (self.oot(), self.bk()) {
            (Some(k), _) => k.spec().hp,
            (_, Some(k)) => k.spec().hp,
            _ => 1,
        }
    }

    /// Touch / attack damage in hearts (OoT: the AT collider's sixteenths; BK: wedges, a wedge being half a heart).
    pub fn touch_hearts(self) -> f32 {
        match (self.oot(), self.bk()) {
            (Some(k), _) => k.spec().touch as f32 / 16.0,
            (_, Some(k)) => k.spec().damage as f32 * 0.5,
            _ => 0.5,
        }
    }

    /// Hurt-box radius in Jumpman units, as the course port draws it. Used to size the model.
    pub fn course_radius(self) -> f32 {
        match (self.oot(), self.bk()) {
            (Some(k), _) => (k.spec().radius * OOT_U) as f32,
            (_, Some(k)) => (k.spec().radius * BK_S) as f32,
            _ => 50.0,
        }
    }

    /// Model scale relative to the model file's own scale: the course port's `Spec.scale` for OoT, the
    /// BK world scale for BK rigs (a rig is in BK units, drawn at 1.25 in the course).
    pub fn model_base(self) -> f32 {
        match (self.oot(), self.bk()) {
            (Some(k), _) => k.spec().scale as f32,
            _ => BK_S as f32,
        }
    }

    /// What a kill leaves in the course: coins (BK) / a drop table (OoT). Modes map this onto their own loot.
    pub fn loot(self) -> Loot {
        match (self.oot(), self.bk()) {
            (Some(k), _) => match k.spec().drops {
                Drops::Token => Loot { token: true, coins: 6.0, hearts: 0.0 },
                Drops::Nuts(n) => Loot { token: false, coins: n as f32 * 1.5, hearts: 0.0 },
                Drops::Random(t) => Loot { token: false, coins: 2.0 + (t % 3) as f32, hearts: 0.12 },
                Drops::None => Loot { token: false, coins: 0.0, hearts: 0.0 },
            },
            (_, Some(k)) => Loot { token: false, coins: k.spec().coins as f32, hearts: if self == Species::Bigbutt { 0.5 } else { 0.1 } },
            _ => Loot { token: false, coins: 1.0, hearts: 0.0 },
        }
    }

    /// Clip names `[idle, move, attack, hurt, die]`; every name exists in the species' model file
    /// (`clip_names_exist` in the tests of `foes_view.rs` checks the exported files).
    fn clips(self) -> Clips {
        match self {
            Species::DekuBaba => ["Idle", "Idle", "Wait", "Idle", "Idle"],
            Species::Skulltula | Species::GoldSkulltula => ["Idle", "Idle", "Idle", "Idle", "Idle"],
            Species::Octorok => ["Idle", "Idle", "Shoot", "Hit", "Die"],
            Species::MadScrub => ["Idle", "Run", "Spit", "Damage", "Die"],
            Species::Tektite => ["Idle", "Walk", "Attack", "Land", "Idle"],
            Species::Peahat => ["Idle", "Rising", "Rising", "Recoil", "Landing"],
            Species::Wolfos => ["Idle", "Run", "Attack", "Damaged", "Damaged"],
            Species::Guay => ["Idle", "Idle", "Idle", "Idle", "Idle"],
            Species::Grublin => ["grublin_idle_walk", "grublin_chase", "grublin_alert", "grublin_idle_walk", "grublin_die"],
            Species::Bigbutt => ["bigbutt_walk", "bigbutt_run", "bigbutt_attack", "bigbutt_ow", "bigbutt_die"],
            Species::Snippet => ["crab_idle", "crab_walk", "crab_walk", "crab_upsidedown", "crab_upsidedown"],
            Species::MutieSnippet => ["mutant_crab_idle", "mutant_crab_walk", "mutant_crab_walk", "mutant_crab_upsidedown", "mutant_crab_upsidedown"],
            Species::SirSlush => ["sir_slush_idle", "sir_slush_idle", "sir_slush_attack", "sir_slush_idle", "sir_slush_die"],
            Species::Ticker => ["bsant_idle", "bsant_walk", "bsant_walk", "termite_ow", "termite_ow"],
            Species::Limbo => ["skeleton_idle", "skeleton_walk", "skeleton_alert", "skeleton_alert", "skeleton_break_apart"],
            Species::Shrapnel => ["shrapnel_idle", "shrapnel_move", "shrapnel_move", "shrapnel_idle", "shrapnel_idle"],
            Species::Bat => ["bat_fly", "bat_fly", "bat_fly", "bat_fall", "bat_die"],
        }
    }

    pub fn clip(self, p: Pose) -> &'static str {
        self.clips()[p as usize]
    }

    /// Seconds one loop of a pose's clip takes (BK rigs are sampled by progress; glb clips play at their own rate).
    pub fn clip_secs(self, p: Pose) -> f64 {
        match p {
            Pose::Idle => 1.2,
            Pose::Move => 0.7,
            Pose::Attack => 0.8,
            Pose::Hurt => 0.5,
            Pose::Die => 0.9,
        }
    }

    /// The pose plays once and holds (death, hurt) instead of looping.
    pub fn once(p: Pose) -> bool {
        matches!(p, Pose::Die)
    }

    /// Lifts the model off the floor for flyers (fraction of the radius).
    pub fn flies(self) -> bool {
        matches!(self, Species::Peahat | Species::Guay | Species::Bat)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Loot {
    /// A Gold Skulltula token: modes turn it into their best drop.
    pub token: bool,
    /// Coins in the course; modes scale to gold / gems.
    pub coins: f32,
    /// Chance of a heart-like pickup.
    pub hearts: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_species_has_decomp_numbers_and_unique_ids() {
        let mut seen = std::collections::HashSet::new();
        for s in SPECIES {
            assert!(seen.insert(s as u8), "{s:?}");
            assert_eq!(Species::from_u8(s as u8), Some(s));
            assert!(s.decomp_hp() >= 1, "{s:?}");
            assert!(s.touch_hearts() > 0.0 || s == Species::Ticker || s == Species::SirSlush, "{s:?}");
            assert!(s.course_radius() > 5.0, "{s:?}");
            assert!(!s.name().is_empty());
            for p in [Pose::Idle, Pose::Move, Pose::Attack, Pose::Hurt, Pose::Die] {
                assert!(!s.clip(p).is_empty());
            }
        }
        assert_eq!(Species::Bigbutt.decomp_hp(), 99);
        assert_eq!(Species::Peahat.decomp_hp(), 6);
        assert_eq!(Species::Wolfos.decomp_hp(), 8);
        assert!(Species::GoldSkulltula.loot().token);
    }
}
