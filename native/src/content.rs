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

    fn id(self) -> &'static str {
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
}

impl Default for AvailableContent {
    fn default() -> Self {
        Self::all()
    }
}

impl AvailableContent {
    /// Everything available (the full build, tests).
    pub fn all() -> Self {
        Self { courses: None, have: [true; 3] }
    }

    /// Probe an asset root (`<root>/courses/*.json`, `<root>/../public/...`).
    pub fn probe(root: &Path) -> Self {
        // A game imported with older exporters is switched off (its data may miss files newer code expects) until re-imported.
        let in_data = root.starts_with(crate::byor::data_dir());
        let have = Game::ALL.map(|g| g.probes().iter().all(|p| root.join(p).is_file()) && !(in_data && crate::byor::game_stale(g.id())));
        let courses = std::fs::read_dir(root.join("courses"))
            .map(|d| d.flatten().filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(str::to_string)).collect())
            .ok();
        Self { courses, have }
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

    pub fn character_missing(&self, c: crate::shell::Character) -> Option<Game> {
        use crate::shell::Character::*;
        match c {
            Mario => None,
            AdultLink | YoungLink => (!self.has(Game::Oot)).then_some(Game::Oot),
            Banjo => (!self.has(Game::Bk)).then_some(Game::Bk),
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
