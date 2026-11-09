//! Banjo-Kazooie sound effects (`enum sfx_e`), decoded from the ROM's sound bank by `scripts/export-bk-sfx.py` into
//! `assets/audio/voices/bk_sfx/<hex id>_<NAME>_0.wav` (402 clips). They play through the voice mixer next to the SM64 engine;
//! a build without the ROM assets is silent. [`SFX`] lists the clips the game code asks for (the test checks each exists).

pub const FOLDER: &str = "voices/bk_sfx/";

pub const EGG_SHATTER: &str = "D_EGGSHELL_BREAKING";
pub const BEAK_BUSTER: &str = "13_BEAKBUSTER_GROUND";
pub const SHOCK_SPRING: &str = "E_SHOCKSPRING_BOING";
pub const TAKE_OFF: &str = "C_TAKING_FLIGHT_LIFTOFF";
pub const DROWNING: &str = "CA_BANJO_DROWNING_1";
pub const CATCH_BREATH: &str = "AF_BANJO_CATCHING_BREATH";
pub const SPLASH: &str = "F_SMALL_WATER_SPLASH";
pub const DOOR_OPEN: &str = "7E_CREAKY_DOOR_OPENING";
pub const DOOR_SLAM: &str = "7F_HEAVYDOOR_SLAM";
/// `bsclimb_move_update`: Jinxy sniffles on the pole, a quiet metallic ring in Clanker's Cavern, Mad Monster Mansion and Rusty Bucket Bay.
pub const CLIMB_STEP: &str = "D3_JINXIE_SNIFFLING_1";
pub const CLIMB_STEP_METAL: &str = "D2_QUIET_METALLIC";
pub const EGG_BOUNCE: &str = "21_EGG_BOUNCE_1";

pub const SFX: &[&str] = &[EGG_SHATTER, BEAK_BUSTER, SHOCK_SPRING, TAKE_OFF, DROWNING, CATCH_BREATH, SPLASH, DOOR_OPEN, DOOR_SLAM, CLIMB_STEP, CLIMB_STEP_METAL, EGG_BOUNCE];

/// The metallic climb sound is used in these maps (`gsworld_getMap` of `bsclimb_move_update`).
pub fn metallic_climb(course: &str) -> bool {
    matches!(course, "bk_clankers_cavern" | "bk_mad_monster_mansion" | "bk_rusty_bucket_bay")
}

/// Play `<name>` of the bank at its default level. Returns false when the clip file is missing.
pub fn play(name: &str) -> bool {
    play_gain(name, 1.0)
}

pub fn play_gain(name: &str, gain: f32) -> bool {
    super::voices::play_gain(&format!("{FOLDER}{name}"), 1.0, gain).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn every_referenced_clip_exists_and_is_audible() {
        for name in SFX {
            let rel = format!("{FOLDER}{name}");
            let path = super::super::voices::clip_path(&assets(), &rel, 0);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} (run scripts/export-bk-sfx.py)", path.display()));
            let clip = super::super::voices::parse_wav(&bytes).unwrap_or_else(|| panic!("{} is not mono 16-bit PCM", path.display()));
            let peak = clip.pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.1, "{name} is nearly silent");
        }
    }

    #[test]
    fn the_manifest_lists_the_whole_bank() {
        let text = std::fs::read_to_string(assets().join("audio/voices/manifest-bk-sfx.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["clips"].as_object().unwrap().len() + json["skipped"].as_object().unwrap().len(), 402);
    }

    #[test]
    fn metallic_maps() {
        assert!(metallic_climb("bk_clankers_cavern") && !metallic_climb("bk_spiral_mountain"));
    }
}
