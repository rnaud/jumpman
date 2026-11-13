//! Personal bests and medals shared by the short goal modes (summit climbs, Endless Run milestones, God's star
//! ladder, Tiny Planet's coin rush). Stored through [`super::store`] under `jumpman.records.<key>`, so a run that
//! sets `JUMPMAN_STORE` (tests, headless checks) never touches the player's own records.
use super::store;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Medal {
    Bronze,
    Silver,
    Gold,
}

impl Medal {
    pub fn label(self) -> &'static str {
        match self {
            Medal::Gold => "GOLD",
            Medal::Silver => "SILVER",
            Medal::Bronze => "BRONZE",
        }
    }
}

/// Par times (seconds) for gold, silver and bronze: lower is better.
pub fn medal_for_time(secs: f64, pars: [f64; 3]) -> Option<Medal> {
    if secs <= pars[0] {
        Some(Medal::Gold)
    } else if secs <= pars[1] {
        Some(Medal::Silver)
    } else if secs <= pars[2] {
        Some(Medal::Bronze)
    } else {
        None
    }
}

/// Medal for a "lower is better" count against thresholds (pieces used, finishing place).
pub fn medal_for_count(n: u32, pars: [u32; 3]) -> Option<Medal> {
    medal_for_time(n as f64, pars.map(|p| p as f64))
}

fn full(key: &str) -> String {
    format!("jumpman.records.{key}")
}

pub fn best(key: &str) -> Option<f64> {
    store::get_f64(&full(key))
}

/// Records `value` when it beats the stored best (`lower_wins`: times, places, pieces). Returns the previous best and
/// whether this is a new record.
pub fn submit(key: &str, value: f64, lower_wins: bool) -> (Option<f64>, bool) {
    let before = best(key);
    let better = beats(before, value, lower_wins);
    if better {
        store::set(&full(key), serde_json::json!(value));
    }
    (before, better)
}

/// Does `value` beat the stored best?
pub fn beats(before: Option<f64>, value: f64, lower_wins: bool) -> bool {
    match before {
        None => true,
        Some(b) if lower_wins => value < b,
        Some(b) => value > b,
    }
}

/// "NEW RECORD" or "best 1:02.3" for a finished attempt (`fmt` formats the stored value).
pub fn verdict(before: Option<f64>, new: bool, fmt: impl Fn(f64) -> String) -> String {
    match (new, before) {
        (true, Some(b)) => format!("NEW RECORD (was {})", fmt(b)),
        (true, None) => "FIRST RECORD SET".to_string(),
        (false, Some(b)) => format!("best {}", fmt(b)),
        (false, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medals_follow_the_pars() {
        let pars = [60.0, 90.0, 150.0];
        assert_eq!(medal_for_time(45.0, pars), Some(Medal::Gold));
        assert_eq!(medal_for_time(60.0, pars), Some(Medal::Gold));
        assert_eq!(medal_for_time(61.0, pars), Some(Medal::Silver));
        assert_eq!(medal_for_time(149.0, pars), Some(Medal::Bronze));
        assert_eq!(medal_for_time(151.0, pars), None);
        assert_eq!(medal_for_count(1, [1, 10, 50]), Some(Medal::Gold));
        assert_eq!(medal_for_count(11, [1, 10, 50]), Some(Medal::Bronze));
    }

    #[test]
    fn records_keep_the_better_value() {
        assert!(beats(None, 80.0, true));
        assert!(!beats(Some(80.0), 90.0, true) && beats(Some(80.0), 70.0, true));
        assert!(beats(Some(5.0), 9.0, false) && !beats(Some(5.0), 4.0, false));
        assert_eq!(verdict(Some(80.0), true, |v| format!("{v}")), "NEW RECORD (was 80)");
    }
}
