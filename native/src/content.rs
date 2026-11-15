//! Which content can actually be played with the assets that are installed.
//!
//! The full build has everything. A bring-your-own-ROM install (see `byor.rs`) only has the games the
//! user imported: Super Mario 64 is required, Mario Kart 64, Ocarina of Time and Banjo-Kazooie are
//! optional. The menu consults [`AvailableContent`] to disable (with a reason) what is missing instead
//! of letting it fail to load. The probes look for the exported files themselves, so they give the same
//! answer for the repo checkout, a bundle and a data dir, and need no stamps.
use bevy::prelude::*;
use std::collections::HashSet;
use std::path::Path;

/// The optional games a BYOR install may be missing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Game {
    Mk64,
    Oot,
    Bk,
}

impl Game {
    pub const ALL: [Game; 3] = [Game::Mk64, Game::Oot, Game::Bk];

    pub fn name(self) -> &'static str {
        match self {
            Game::Mk64 => "Mario Kart 64",
            Game::Oot => "Ocarina of Time",
            Game::Bk => "Banjo-Kazooie",
        }
    }

    /// Shown in place of a disabled entry's description.
    pub fn reason(self) -> &'static str {
        if crate::byor::game_stale(self.id()) {
            return "Update needed in Setup";
        }
        match self {
            Game::Mk64 => "Import Mario Kart 64 ROM in Setup",
            Game::Oot => "Import Ocarina of Time ROM in Setup",
            Game::Bk => "Import Banjo-Kazooie ROM in Setup",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Game::Mk64 => "mk64",
            Game::Oot => "oot",
            Game::Bk => "bk",
        }
    }

    /// The exported files without which this game's content cannot load, relative to `assets/`.
    fn probes(self) -> &'static [&'static str] {
        match self {
            Game::Mk64 => &["../public/mk64/luigi-raceway.json"],
            Game::Oot => &["../public/models/oot/link.json", "../public/models/oot/young-link.json", "courses/kokiri_forest.json"],
            Game::Bk => &["../public/models/bk/banjo.json"],
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct AvailableContent {
    /// `None` = every course is assumed present (the default, used when nothing was probed).
    courses: Option<HashSet<String>>,
    have: [bool; 3],
    /// Bowser's model (`characters::bowser::MODEL`) is in the SM64 export.
    bowser: bool,
    /// Each Smash fighter's glb (and the import's `fighters.json`) is installed (`characters::smash::installed`).
    smash: [bool; crate::characters::smash::Fighter::COUNT],
}

impl Default for AvailableContent {
    fn default() -> Self {
        Self::all()
    }
}

impl AvailableContent {
    /// Everything available (the full build, tests).
    pub fn all() -> Self {
        Self { courses: None, have: [true; 3], bowser: true, smash: [true; crate::characters::smash::Fighter::COUNT] }
    }

    /// Probe an asset root (`<root>/courses/*.json`, `<root>/../public/...`).
    pub fn probe(root: &Path) -> Self {
        // A game imported with older exporters is switched off (its data may miss files newer code expects) until re-imported.
        let in_data = root.starts_with(crate::byor::data_dir());
        let mut have = Game::ALL.map(|g| g.probes().iter().all(|p| root.join(p).is_file()) && !(in_data && crate::byor::game_stale(g.id())));
        // Test hook for the menus (locked characters, greyed modes): `JUMPMAN_MISSING_GAMES=oot,bk,mk64` pretends those are not installed.
        if let Ok(list) = std::env::var("JUMPMAN_MISSING_GAMES") {
            for (g, h) in Game::ALL.iter().zip(have.iter_mut()) {
                if list.split(',').any(|x| x.trim() == g.id()) {
                    *h = false;
                }
            }
        }
        let courses = std::fs::read_dir(root.join("courses"))
            .map(|d| d.flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_string)).collect())
            .ok();
        // `JUMPMAN_MISSING_GAMES=bowser` hides him too (menu checks).
        let hidden = |id: &str| std::env::var("JUMPMAN_MISSING_GAMES").is_ok_and(|l| l.split(',').any(|x| x.trim() == id));
        let bowser = crate::characters::bowser::installed(root) && !hidden("bowser");
        // The Smash fighters follow their glbs (`JUMPMAN_MISSING_GAMES=ssb64` hides them all, `ssb-pikachu` one of them).
        let smash = crate::characters::smash::Fighter::ALL.map(|f| crate::characters::smash::installed(root, f) && !hidden("ssb64") && !hidden(f.id()));
        Self { courses, have, bowser, smash }
    }

    pub fn has(&self, game: Game) -> bool {
        self.have[Game::ALL.iter().position(|g| *g == game).unwrap_or(0)]
    }

    /// Which optional game is missing for this course (`None` = playable).
    pub fn course_missing(&self, id: &str) -> Option<Game> {
        let zelda = crate::game::courses::info(id).is_some_and(|c| c.series == crate::game::courses::Series::Zelda);
        if zelda && !self.has(Game::Oot) {
            return Some(Game::Oot);
        }
        match &self.courses {
            Some(set) if zelda && !set.contains(id) => Some(Game::Oot),
            _ => None,
        }
    }

    /// Whether the course's json is installed (always true when nothing was probed). The optional Super Smash Bros.
    /// stages are listed only when present.
    pub fn course_present(&self, id: &str) -> bool {
        self.courses.as_ref().is_none_or(|set| set.contains(id))
    }

    pub fn character_missing(&self, c: crate::shell::Character) -> Option<Game> {
        use crate::shell::Character::*;
        match c {
            Mario => None,
            AdultLink | YoungLink => (!self.has(Game::Oot)).then_some(Game::Oot),
            Banjo => (!self.has(Game::Bk)).then_some(Game::Bk),
            // Part of the required SM64 import: never "import a game", only present or not (see `bowser_installed`).
            Bowser => None,
            // An optional import that adds characters: present or left out of the roster (see `smash_installed`).
            Smash(_) => None,
        }
    }

    /// Whether Bowser's model is installed (the lineup leaves him out otherwise).
    pub fn bowser_installed(&self) -> bool {
        self.bowser
    }

    /// Whether this Smash fighter's model is installed (the roster leaves it out otherwise).
    pub fn smash_installed(&self, f: crate::characters::smash::Fighter) -> bool {
        self.smash[f.index()]
    }

    /// Whether the installed data can draw this character.
    pub fn character_ok(&self, c: crate::shell::Character) -> bool {
        use crate::shell::Character::*;
        self.character_missing(c).is_none()
            && match c {
                Bowser => self.bowser,
                Smash(f) => self.smash_installed(f),
                _ => true,
            }
    }

    pub fn experiment_missing(&self, e: crate::shell::ExperimentId) -> Option<Game> {
        match e {
            crate::shell::ExperimentId::Kart => (!self.has(Game::Mk64)).then_some(Game::Mk64),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::{Character, ExperimentId};

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "{}").unwrap();
    }

    fn fake_root(name: &str, files: &[&str]) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("content-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let root = d.join("assets");
        std::fs::create_dir_all(root.join("courses")).unwrap();
        for f in files {
            touch(&root, f);
        }
        root
    }

    #[test]
    fn sm64_only_install_hides_the_optional_games() {
        let root = fake_root("sm64", &["courses/bob.json"]);
        let c = AvailableContent::probe(&root);
        assert_eq!(c.experiment_missing(ExperimentId::Kart), Some(Game::Mk64));
        assert_eq!(c.experiment_missing(ExperimentId::Swarm), None);
        assert_eq!(c.character_missing(Character::AdultLink), Some(Game::Oot));
        assert_eq!(c.character_missing(Character::YoungLink), Some(Game::Oot));
        assert_eq!(c.character_missing(Character::Banjo), Some(Game::Bk));
        assert_eq!(c.character_missing(Character::Mario), None);
        assert_eq!(c.course_missing("kokiri_forest"), Some(Game::Oot));
        assert_eq!(c.course_missing("bob"), None);
        assert_eq!(Game::Mk64.reason(), "Import Mario Kart 64 ROM in Setup");
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn each_imported_game_enables_only_its_content() {
        let root = fake_root(
            "all",
            &["courses/bob.json", "courses/kokiri_forest.json", "../public/mk64/luigi-raceway.json", "../public/models/oot/link.json", "../public/models/oot/young-link.json", "../public/models/bk/banjo.json"],
        );
        let c = AvailableContent::probe(&root);
        assert!(c.has(Game::Mk64) && c.has(Game::Oot) && c.has(Game::Bk));
        assert_eq!(c.character_missing(Character::Banjo), None);
        assert_eq!(c.course_missing("kokiri_forest"), None);
        // Only Banjo-Kazooie removed.
        std::fs::remove_file(root.join("../public/models/bk/banjo.json")).unwrap();
        let c = AvailableContent::probe(&root);
        assert!(c.has(Game::Mk64) && c.has(Game::Oot) && !c.has(Game::Bk));
        assert_eq!(c.character_missing(Character::Banjo), Some(Game::Bk));
        assert_eq!(c.character_missing(Character::AdultLink), None);
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    /// Bowser follows his glb: present in the SM64 export, or left out (never "import a game").
    #[test]
    fn bowser_follows_his_model() {
        let root = fake_root("bowser", &["courses/bob.json"]);
        let c = AvailableContent::probe(&root);
        assert!(!c.bowser_installed() && !c.character_ok(Character::Bowser));
        assert_eq!(c.character_missing(Character::Bowser), None);
        assert!(c.character_ok(Character::Mario));
        touch(&root, crate::characters::bowser::MODEL);
        let c = AvailableContent::probe(&root);
        assert!(c.bowser_installed() && c.character_ok(Character::Bowser));
        assert!(AvailableContent::all().character_ok(Character::Bowser));
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    /// Each Smash fighter follows its own glb plus the import's index: missing, it is not ok (and never "import a game").
    #[test]
    fn smash_fighters_follow_their_models() {
        use crate::characters::smash::{self, Fighter};
        let root = fake_root("smash", &["courses/bob.json"]);
        let c = AvailableContent::probe(&root);
        for f in Fighter::ALL {
            assert!(!c.smash_installed(f) && !c.character_ok(Character::Smash(f)));
            assert_eq!(c.character_missing(Character::Smash(f)), None);
        }
        touch(&root, &Fighter::Pikachu.model());
        assert!(!AvailableContent::probe(&root).smash_installed(Fighter::Pikachu), "a glb without fighters.json is not enough");
        touch(&root, smash::INDEX);
        let c = AvailableContent::probe(&root);
        assert!(c.smash_installed(Fighter::Pikachu) && c.character_ok(Character::Smash(Fighter::Pikachu)));
        assert!(!c.character_ok(Character::Smash(Fighter::Donkey)), "only the glbs that exist");
        assert!(Fighter::ALL.iter().all(|f| AvailableContent::all().character_ok(Character::Smash(*f))));
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn full_build_has_everything() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        if !root.join("courses").is_dir() {
            return;
        }
        let c = AvailableContent::probe(&root);
        for g in Game::ALL {
            assert!(c.has(g), "{g:?} missing from the repo checkout");
        }
        for course in crate::game::courses::COURSES {
            assert_eq!(c.course_missing(course.id), None, "{} should be available", course.id);
        }
        assert!(Character::ALL.into_iter().all(|ch| c.character_missing(ch).is_none()));
        assert!(ExperimentId::ALL.into_iter().all(|e| c.experiment_missing(e).is_none()));
    }
}
