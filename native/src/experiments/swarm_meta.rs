//! Star Swarm, run-to-run progression: difficulty tiers, per-level records, "star bits" earned
//! every run and the ranks they buy. Pure data and arithmetic (no engine), so it is unit-tested
//! here; `swarm.rs` loads and saves it as `swarm.json` next to the save files.
use std::collections::BTreeMap;

pub const TIERS: usize = 3;
pub const MAX_RANK: u32 = 10;

/// One difficulty: multipliers on the swarm, and on what a run pays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tier {
    pub name: &'static str,
    pub hp: f32,
    pub speed: f32,
    pub spawn: f32,
    pub damage: f32,
    pub xp: f32,
    pub bits: f32,
}

pub fn tier(i: usize) -> Tier {
    match i.min(TIERS - 1) {
        0 => Tier { name: "NORMAL", hp: 1.0, speed: 1.0, spawn: 1.0, damage: 1.0, xp: 1.0, bits: 1.0 },
        1 => Tier { name: "HARD", hp: 1.5, speed: 1.08, spawn: 1.25, damage: 1.3, xp: 1.15, bits: 1.6 },
        _ => Tier { name: "NIGHTMARE", hp: 2.3, speed: 1.16, spawn: 1.5, damage: 1.7, xp: 1.3, bits: 2.5 },
    }
}

/// What the player's rank adds at the start of every run.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Perks {
    /// Multiplier on star damage.
    pub damage: f32,
    pub max_hp: f32,
    pub xp: f32,
}

pub fn perks(rank: u32) -> Perks {
    let r = rank.min(MAX_RANK) as f32;
    Perks { damage: 1.0 + 0.02 * r, max_hp: 4.0 * r, xp: 1.0 + 0.015 * r }
}

/// Star bits needed for each rank (cumulative): 60, 180, 360, ...
pub fn bits_for_rank(rank: u32) -> u32 {
    30 * rank * (rank + 1)
}

pub fn rank_of(bits: u32) -> u32 {
    let mut r = 0;
    while r < MAX_RANK && bits >= bits_for_rank(r + 1) {
        r += 1;
    }
    r
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelRecord {
    pub best_secs: f32,
    pub wins: u32,
    pub runs: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub bits: u32,
    pub runs: u32,
    pub wins: u32,
    pub kills: u32,
    /// Wins per difficulty tier (any level): tier `t + 1` opens with a win on tier `t`.
    pub wins_by_tier: [u32; TIERS],
    pub levels: BTreeMap<String, LevelRecord>,
    /// The battlefield and difficulty picked last, so the mode opens on them again.
    pub last_level: String,
    pub last_tier: usize,
}

/// What one finished run earned.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RunResult {
    pub bits: u32,
    pub new_best: bool,
    pub rank_before: u32,
    pub rank_after: u32,
}

/// Bits a run pays out before the tier bonus: time, kills and a bonus for the win.
pub fn bits_for_run(secs: f32, kills: u32, won: bool, t: Tier) -> u32 {
    let base = secs / 6.0 + kills as f32 / 12.0 + if won { 150.0 } else { 0.0 };
    (base * t.bits).round() as u32
}

impl Meta {
    pub fn rank(&self) -> u32 {
        rank_of(self.bits)
    }

    pub fn tier_unlocked(&self, i: usize) -> bool {
        i == 0 || (i < TIERS && self.wins_by_tier[i - 1] > 0)
    }

    pub fn record(&self, level: &str) -> LevelRecord {
        self.levels.get(level).copied().unwrap_or_default()
    }

    /// Book a finished run. `cheated` runs (test hooks) change nothing.
    pub fn finish_run(&mut self, level: &str, tier_i: usize, secs: f32, kills: u32, won: bool, cheated: bool) -> RunResult {
        let rank_before = self.rank();
        if cheated {
            return RunResult { bits: 0, new_best: false, rank_before, rank_after: rank_before };
        }
        let bits = bits_for_run(secs, kills, won, tier(tier_i));
        self.bits += bits;
        self.runs += 1;
        self.kills += kills;
        let rec = self.levels.entry(level.to_string()).or_default();
        rec.runs += 1;
        let new_best = secs > rec.best_secs + 0.05;
        if new_best {
            rec.best_secs = secs;
        }
        if won {
            rec.wins += 1;
            self.wins += 1;
            self.wins_by_tier[tier_i.min(TIERS - 1)] += 1;
        }
        RunResult { bits, new_best, rank_before, rank_after: self.rank() }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let levels: serde_json::Map<String, serde_json::Value> = self.levels.iter().map(|(k, r)| (k.clone(), serde_json::json!({ "best_secs": r.best_secs, "wins": r.wins, "runs": r.runs }))).collect();
        serde_json::json!({ "bits": self.bits, "runs": self.runs, "wins": self.wins, "kills": self.kills, "wins_by_tier": self.wins_by_tier, "levels": levels, "last_level": self.last_level, "last_tier": self.last_tier })
    }

    /// Read a saved file; the old format (a single `best_secs`) becomes the Bob-omb Battlefield record.
    pub fn from_json(v: &serde_json::Value) -> Self {
        let mut m = Meta::default();
        let n = |x: &serde_json::Value| x.as_u64().unwrap_or(0) as u32;
        m.bits = n(&v["bits"]);
        m.runs = n(&v["runs"]);
        m.wins = n(&v["wins"]);
        m.kills = n(&v["kills"]);
        for (i, w) in v["wins_by_tier"].as_array().into_iter().flatten().take(TIERS).enumerate() {
            m.wins_by_tier[i] = n(w);
        }
        if let Some(map) = v["levels"].as_object() {
            for (k, r) in map {
                m.levels.insert(k.clone(), LevelRecord { best_secs: r["best_secs"].as_f64().unwrap_or(0.0) as f32, wins: n(&r["wins"]), runs: n(&r["runs"]) });
            }
        }
        m.last_level = v["last_level"].as_str().unwrap_or("").to_string();
        m.last_tier = n(&v["last_tier"]) as usize;
        if let Some(old) = v["best_secs"].as_f64() {
            m.levels.entry("bob".into()).or_insert(LevelRecord { best_secs: old as f32, wins: 0, runs: 0 });
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_climb_with_bits_and_cap() {
        assert_eq!(rank_of(0), 0);
        assert_eq!(rank_of(59), 0);
        assert_eq!(rank_of(60), 1);
        assert_eq!(rank_of(179), 1);
        assert_eq!(rank_of(180), 2);
        assert_eq!(rank_of(u32::MAX), MAX_RANK);
        assert!(perks(5).damage > perks(1).damage && perks(0) == Perks { damage: 1.0, max_hp: 0.0, xp: 1.0 });
    }

    #[test]
    fn harder_tiers_pay_more_and_unlock_by_winning() {
        let mut m = Meta::default();
        assert!(m.tier_unlocked(0) && !m.tier_unlocked(1) && !m.tier_unlocked(2));
        assert!(bits_for_run(300.0, 400, false, tier(2)) > bits_for_run(300.0, 400, false, tier(0)));
        m.finish_run("bob", 0, 480.0, 900, true, false);
        assert!(m.tier_unlocked(1) && !m.tier_unlocked(2));
        m.finish_run("bob", 1, 480.0, 900, true, false);
        assert!(m.tier_unlocked(2));
    }

    #[test]
    fn records_are_per_level_and_cheated_runs_change_nothing() {
        let mut m = Meta::default();
        let r = m.finish_run("bob", 0, 100.0, 50, false, false);
        assert!(r.new_best && r.bits > 0);
        assert!(!m.finish_run("bob", 0, 90.0, 50, false, false).new_best);
        assert!(m.finish_run("wf", 0, 90.0, 50, false, false).new_best, "a new level has its own record");
        let before = m.clone();
        let c = m.finish_run("bob", 0, 999.0, 9999, true, true);
        assert_eq!((c.bits, c.new_best), (0, false));
        assert_eq!(m, before);
    }

    #[test]
    fn json_round_trips_and_reads_the_old_single_record() {
        let mut m = Meta::default();
        m.finish_run("bob", 0, 123.0, 80, true, false);
        m.finish_run("wf", 1, 77.0, 20, false, false);
        assert_eq!(Meta::from_json(&m.to_json()), m);
        let old = Meta::from_json(&serde_json::json!({ "best_secs": 211.0 }));
        assert_eq!(old.record("bob").best_secs, 211.0);
        assert_eq!(Meta::from_json(&serde_json::json!(null)), Meta::default());
    }
}
