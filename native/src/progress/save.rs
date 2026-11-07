//! Port of `src/game/save.ts`: four save slots with stars, best coin counts, key/door flags,
//! open cannons and the wing-switch flag. The browser's localStorage becomes one JSON file per
//! slot in the platform data directory (`JUMPMAN_SAVE_DIR` overrides it). The file layout is the
//! TS `SaveData` (version 3) so saves are interchangeable. The Zelda village/dungeon state is kept
//! verbatim as opaque JSON.
use crate::game::courses::SCORE_COURSES;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const TOTAL_STARS: u32 = 120;
pub const SLOTS: u32 = 4;

/// Save-file flag bits (`get_door_save_file_flag` and friends).
pub const FLAG_WING_SWITCH: u32 = 2;
pub const FLAG_DDD_MOVED: u32 = 0x100;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CastleKey {
    Basement = 1,
    Upstairs = 2,
}

impl CastleKey {
    fn held(self) -> u32 {
        if self == CastleKey::Basement { 0x10 } else { 0x20 }
    }
    fn unlocked(self) -> u32 {
        if self == CastleKey::Basement { 0x40 } else { 0x80 }
    }
    pub fn from_number(n: i32) -> Option<Self> {
        match n {
            1 => Some(Self::Basement),
            2 => Some(Self::Upstairs),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SaveData {
    pub version: u32,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub stars: Vec<String>,
    #[serde(default)]
    pub cannons: Vec<String>,
    #[serde(default)]
    pub best_coins: BTreeMap<String, u32>,
    /// Zelda village state, preserved untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kokiri: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deku: Option<Value>,
}

impl Default for SaveData {
    fn default() -> Self {
        Self { version: 3, flags: 0, stars: vec![], cannons: vec![], best_coins: BTreeMap::new(), kokiri: None, deku: None }
    }
}

fn star_key(course: &str, act: i32) -> String {
    if act > 1 { format!("{course}:{act}") } else { course.to_string() }
}

/// `parse`: tolerate old versions and migrate their star ids.
pub fn parse(raw: &str) -> SaveData {
    let Ok(mut data) = serde_json::from_str::<SaveData>(raw) else { return SaveData::default() };
    if !(1..=3).contains(&data.version) {
        return SaveData::default();
    }
    let version = data.version;
    let mut flags = data.flags;
    // Older builds recorded the first two arena rewards as stars.
    data.stars.retain(|star| {
        if star != "bowser_1" && star != "bowser_2" {
            return true;
        }
        let key = if star == "bowser_1" { CastleKey::Basement } else { CastleKey::Upstairs };
        if flags & key.unlocked() == 0 {
            flags |= key.held();
        }
        false
    });
    data.flags = flags;
    let mut seen = std::collections::HashSet::new();
    data.stars = data
        .stars
        .iter()
        .map(|id| match id.as_str() {
            "jrb" if version == 1 => "jrb:3".to_string(),
            "bob:3" if version != 3 => "bob:6".to_string(),
            other => other.to_string(),
        })
        .filter(|id| seen.insert(id.clone()))
        .collect();
    data.version = 3;
    data
}

/// Directory holding the slot files.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("JUMPMAN_SAVE_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        home().join("Library/Application Support/Jumpman")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Jumpman")).unwrap_or_else(|| home().join("Jumpman"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("jumpman")
    }
}

/// `activeSaveSlot`/`SaveFile.select`: the slot most recently chosen (1..=4).
pub fn active_slot(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join("slot.txt")).ok().and_then(|s| s.trim().parse().ok()).filter(|s| (1..=SLOTS).contains(s)).unwrap_or(1)
}

pub fn select_slot(dir: &Path, slot: u32) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join("slot.txt"), slot.to_string());
}

/// Persistent star/coin records for one of the four slots.
#[derive(Clone, Debug)]
pub struct SaveFile {
    pub slot: u32,
    data: SaveData,
    /// Snapshot taken when a course star is collected; progress stays in memory until the player
    /// chooses to save (`deferCourseSave`/`confirmCourseSave`).
    before_save_choice: Option<SaveData>,
    dir: Option<PathBuf>,
}

impl SaveFile {
    /// In-memory only (tests, headless runs without persistence).
    pub fn in_memory(slot: u32) -> Self {
        Self { slot, data: SaveData::default(), before_save_choice: None, dir: None }
    }

    pub fn open(dir: &Path, slot: u32) -> Self {
        let raw = std::fs::read_to_string(Self::path(dir, slot)).unwrap_or_default();
        Self { slot, data: parse(&raw), before_save_choice: None, dir: Some(dir.to_path_buf()) }
    }

    pub fn path(dir: &Path, slot: u32) -> PathBuf {
        dir.join(format!("save_slot{slot}.json"))
    }

    pub fn data(&self) -> &SaveData {
        &self.data
    }

    pub fn defer_course_save(&mut self) {
        if self.before_save_choice.is_none() {
            self.before_save_choice = Some(self.data.clone());
        }
    }

    pub fn confirm_course_save(&mut self) {
        self.before_save_choice = None;
        self.persist();
    }

    /// Discard unsaved course progress (quit without saving).
    pub fn revert_course_save(&mut self) {
        if let Some(before) = self.before_save_choice.take() {
            self.data = before;
        }
    }

    pub fn has_unsaved(&self) -> bool {
        self.before_save_choice.is_some()
    }

    /// Stars counted at castle doors; bonus playgrounds and arena rewards do not count.
    pub fn stars(&self) -> u32 {
        const EXCLUDED: [&str; 5] = ["playground", "mushroom2d", "castle_grounds", "castle_inside", "bowser_3"];
        self.data.stars.iter().filter(|id| !EXCLUDED.contains(&id.split(':').next().unwrap_or(""))).count() as u32
    }

    pub fn has_star(&self, course: &str, act: i32) -> bool {
        let key = star_key(course, act);
        self.data.stars.contains(&key)
    }

    pub fn best_coins(&self, course: &str) -> u32 {
        self.data.best_coins.get(course).copied().unwrap_or(0)
    }

    pub fn collect_star(&mut self, course: &str, act: i32, coins: u32) {
        let key = star_key(course, act);
        let new_star = !self.data.stars.contains(&key);
        let high_score = SCORE_COURSES.contains(&course) && coins > self.best_coins(course);
        if new_star {
            self.data.stars.push(key);
        }
        // SM64 compares the full run total, then stores the low byte of the record.
        if high_score {
            self.data.best_coins.insert(course.to_string(), coins & 0xff);
        }
        if new_star || high_score {
            self.persist();
        }
    }

    pub fn wing_switch_on(&self) -> bool {
        self.data.flags & FLAG_WING_SWITCH != 0
    }
    pub fn press_wing_switch(&mut self) {
        self.data.flags |= FLAG_WING_SWITCH;
        self.persist();
    }

    pub fn cannon_open(&self, course: &str) -> bool {
        self.data.cannons.iter().any(|c| c == course)
    }
    pub fn open_cannon(&mut self, course: &str) {
        if !self.cannon_open(course) {
            self.data.cannons.push(course.to_string());
            self.persist();
        }
    }

    pub fn has_flag(&self, flag: u32) -> bool {
        self.data.flags & flag != 0
    }
    pub fn set_flag(&mut self, flag: u32) {
        if flag == 0 || self.has_flag(flag) {
            return;
        }
        self.data.flags |= flag;
        self.persist();
    }

    pub fn collect_key(&mut self, key: CastleKey) {
        if self.data.flags & (key.held() | key.unlocked()) != 0 {
            return;
        }
        self.data.flags |= key.held();
        self.persist();
    }
    pub fn has_key(&self, key: CastleKey) -> bool {
        self.data.flags & key.held() != 0
    }
    pub fn door_unlocked(&self, key: CastleKey) -> bool {
        self.data.flags & key.unlocked() != 0
    }
    pub fn unlock_door(&mut self, key: CastleKey) -> bool {
        if self.door_unlocked(key) {
            return true;
        }
        if !self.has_key(key) {
            return false;
        }
        self.data.flags = (self.data.flags & !key.held()) | key.unlocked();
        self.persist();
        true
    }

    pub fn reset(&mut self) {
        self.data = SaveData::default();
        self.confirm_course_save();
    }

    fn persist(&self) {
        let Some(dir) = &self.dir else { return };
        // Course progress stays in memory until the player saves; Zelda village state autosaves.
        let durable = match &self.before_save_choice {
            Some(before) => SaveData { kokiri: self.data.kokiri.clone(), ..before.clone() },
            None => self.data.clone(),
        };
        let _ = std::fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_string_pretty(&durable) {
            let tmp = dir.join(format!("save_slot{}.json.tmp", self.slot));
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path(dir, self.slot));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jumpman-save-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn stars_and_high_scores() {
        let mut s = SaveFile::in_memory(1);
        s.collect_star("bob", 1, 40);
        s.collect_star("bob", 2, 30);
        s.collect_star("castle_inside", 1, 0);
        assert_eq!(s.stars(), 2);
        assert!(s.has_star("bob", 2) && !s.has_star("bob", 3));
        assert_eq!(s.best_coins("bob"), 40);
        s.collect_star("bob", 2, 300);
        assert_eq!(s.best_coins("bob"), 300 & 0xff);
        // Non-score courses never record a best.
        s.collect_star("castle_grounds", 1, 99);
        assert_eq!(s.best_coins("castle_grounds"), 0);
    }

    #[test]
    fn keys_and_doors() {
        let mut s = SaveFile::in_memory(1);
        assert!(!s.unlock_door(CastleKey::Basement));
        s.collect_key(CastleKey::Basement);
        assert!(s.has_key(CastleKey::Basement));
        assert!(s.unlock_door(CastleKey::Basement));
        assert!(!s.has_key(CastleKey::Basement) && s.door_unlocked(CastleKey::Basement));
        s.collect_key(CastleKey::Basement); // already used: no new key
        assert!(!s.has_key(CastleKey::Basement));
    }

    #[test]
    fn persists_per_slot_and_defers_course_saves() {
        let dir = temp();
        let mut a = SaveFile::open(&dir, 2);
        a.defer_course_save();
        a.collect_star("wf", 1, 12);
        // Not durable until confirmed.
        assert_eq!(SaveFile::open(&dir, 2).stars(), 0);
        a.confirm_course_save();
        assert_eq!(SaveFile::open(&dir, 2).stars(), 1);
        assert_eq!(SaveFile::open(&dir, 1).stars(), 0);
        let mut b = SaveFile::open(&dir, 2);
        b.defer_course_save();
        b.collect_star("ccm", 1, 0);
        b.revert_course_save();
        assert_eq!(b.stars(), 1);
        select_slot(&dir, 3);
        assert_eq!(active_slot(&dir), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_ts_saves_and_migrates() {
        let raw = r#"{"version":1,"flags":0,"stars":["jrb","bob","bowser_1","bob"],"cannons":["bob"],"bestCoins":{"bob":12}}"#;
        let d = parse(raw);
        assert_eq!(d.stars, vec!["jrb:3", "bob"]);
        assert_eq!(d.flags & 0x10, 0x10, "bowser_1 star became a held key");
        assert_eq!(d.best_coins["bob"], 12);
        assert_eq!(parse("garbage"), SaveData::default());
        assert_eq!(parse(r#"{"version":9,"stars":[]}"#), SaveData::default());
        // Round trip keeps unknown village state.
        let with_village = parse(r#"{"version":3,"flags":2,"stars":[],"kokiri":{"rupees":5}}"#);
        assert!(with_village.kokiri.is_some());
        assert!(SaveFile { slot: 1, data: with_village, before_save_choice: None, dir: None }.wing_switch_on());
    }
}
