//! Ocarina of Time item / combat / pickup / enemy sound effects (`NA_SE_IT_*`, `NA_SE_SY_*`, `NA_SE_PL_*`, `NA_SE_EV_*`,
//! `NA_SE_EN_*`), decoded from the ROM by `scripts/export-oot-sfx.py` into `assets/audio/voices/oot_sfx/<NAME>_0.wav`
//! (`NAME` = the decomp name without `NA_SE_`). They play through the voice [`Mixer`](super::voices::Mixer) next to the
//! SM64 engine; a build without the ROM assets is silent.
//!
//! [`SFX`] lists every clip the game code asks for (the tests check each exists and is audible); the per-enemy tables
//! follow the `Actor_PlaySfx` calls of the `ovl_En_*` actors (`z_en_dekubaba.c`, `z_en_tite.c`, ...).
use jumpman_sim::oot_enemy::Kind;

pub const FOLDER: &str = "voices/oot_sfx/";

/// Play `NA_SE_<name>` (`name` without the prefix) at its default level. Returns false when the clip file is missing.
pub fn play(name: &str) -> bool {
    play_gain(name, 1.0)
}

pub fn play_gain(name: &str, gain: f32) -> bool {
    super::voices::play_gain(&format!("{FOLDER}{name}"), 1.0, gain).1
}

/// The two sounds an enemy makes when a blow lands without killing it: its own damage cry.
pub fn enemy_hurt(kind: Kind) -> &'static str {
    match kind {
        Kind::DekuBaba | Kind::BigDekuBaba | Kind::WitheredBaba => "EN_DEKU_DAMAGE",
        Kind::GomaLarva => "EN_GOMA_JR_DAM1",
        Kind::Skulltula | Kind::GoldSkulltula => "EN_STALTU_DAMAGE",
        Kind::Peahat | Kind::PeahatLarva => "EN_PIHAT_DAMAGE",
        Kind::Tektite | Kind::RedTektite => "EN_TEKU_DAMAGE",
        Kind::Octorok => "EN_OCTAROCK_BUBLE",
        Kind::MadScrub => "EN_NUTS_DAMAGE",
        Kind::Wolfos => "EN_WOLFOS_DAMAGE",
        Kind::Guay => "EN_KAICHO_CRY",
        Kind::Keese => "EN_FFLY_ATTACK",
        Kind::BabyDodongo => "EN_DODO_J_DAMAGE",
        Kind::Biri => "EN_BIRI_SPARK",
        Kind::KingDodongo => "EN_DODO_J_DAMAGE",
        Kind::Armos => "EN_AMOS_DAMAGE",
        Kind::Shabom => "EN_BUBLE_DAMAGE",
        Kind::Stinger => "EN_BUBLE_BITE",
        Kind::ShellBlade => "EN_SHELL_DEAD",
        Kind::Beamos => "EN_FFLY_ATTACK",
        Kind::Redead => "EN_REDEAD_DAMAGE",
        Kind::Lizalfos => "EN_RIZA_DAMAGE",
        Kind::FlameSkull => "EN_BUBLE_BITE",
        Kind::TorchSlug => "EN_BUBLE_DAMAGE",
        Kind::Freezard => "EN_FREEZAD_DAMAGE",
        Kind::LikeLike => "EN_LIKE_DAMAGE",
        Kind::Wallmaster => "EN_BUBLE_DAMAGE",
        Kind::Barinade => "EN_VALVAISA_DAMAGE1",
        Kind::PhantomGanon => "EN_VALVAISA_DAMAGE1",
        Kind::Leever => "EN_RIVA_DAMAGE",
    }
}

/// The death sound, whoever delivered the blow (sword, arrow, bomb, fire...).
pub fn enemy_dead(kind: Kind) -> &'static str {
    match kind {
        Kind::DekuBaba | Kind::BigDekuBaba | Kind::WitheredBaba => "EN_DEKU_DEAD",
        Kind::GomaLarva => "EN_GOMA_JR_DEAD",
        Kind::Skulltula | Kind::GoldSkulltula => "EN_STALWALL_DEAD",
        Kind::Peahat | Kind::PeahatLarva => "EN_PIHAT_SM_DEAD",
        Kind::Tektite | Kind::RedTektite => "EN_TEKU_DEAD",
        Kind::Octorok => "EN_OCTAROCK_DEAD1",
        Kind::MadScrub => "EN_NUTS_DEAD",
        Kind::Wolfos => "EN_WOLFOS_DEAD",
        Kind::Guay => "EN_KAICHO_DEAD",
        Kind::Keese => "EN_FFLY_DEAD",
        Kind::BabyDodongo => "EN_DODO_J_DEAD",
        Kind::Biri => "EN_BIRI_DEAD",
        Kind::KingDodongo => "EN_DODO_J_DEAD",
        Kind::Armos => "EN_AMOS_DEAD",
        Kind::Shabom => "EN_BUBLE_DEAD",
        Kind::Stinger => "EN_BUBLE_DEAD",
        Kind::ShellBlade => "EN_SHELL_DEAD",
        Kind::Beamos => "EN_FFLY_DEAD",
        Kind::Redead => "EN_REDEAD_DEAD",
        Kind::Lizalfos => "EN_RIZA_DEAD",
        Kind::FlameSkull => "EN_BUBLE_DEAD",
        Kind::TorchSlug => "EN_BUBLE_DEAD",
        Kind::Freezard => "EN_FREEZAD_DEAD",
        Kind::LikeLike => "EN_LIKE_DEAD",
        Kind::Wallmaster => "EN_BUBLE_DEAD",
        Kind::Barinade => "EN_VALVAISA_DEAD",
        Kind::PhantomGanon => "EN_VALVAISA_DEAD",
        Kind::Leever => "EN_RIVA_DEAD",
    }
}

/// Link's blow lands: the sword (or other weapon) connecting.
pub const HIT_BLADE: &str = "IT_SWORD_STRIKE";
pub const HIT_BLUNT: &str = "IT_HAMMER_HIT";
/// The generic puff when an enemy disappears (`Actor_Kill` + `NA_SE_EN_EXTINCT`).
pub const EXTINCT: &str = "EN_EXTINCT";
/// A hit that bounces (shield, hidden Baba): `NA_SE_IT_SHIELD_REFLECT_SW`.
pub const CLANK: &str = "IT_SHIELD_REFLECT_SW";

/// Every clip the game plays, by `NA_SE_` name without the prefix.
pub const SFX: &[&str] = &[
    // sword, shield, spin
    "IT_SWORD_SWING", "IT_SWORD_SWING_HARD", "IT_SWORD_STRIKE", "IT_SWORD_STRIKE_HARD", "IT_SWORD_IMPACT", "IT_SWORD_PICKOUT",
    "IT_SWORD_PUTAWAY", "IT_SWORD_CHARGE", "IT_ROLLING_CUT", "IT_ROLLING_CUT_LV1", "IT_SHIELD_BOUND", "IT_SHIELD_REFLECT_SW",
    "IT_SHIELD_REFLECT_MG", "IT_SHIELD_POSTURE", "IT_WOODSTICK_BROKEN", "IT_HAMMER_SWING", "IT_HAMMER_HIT", "IT_WALL_HIT_HARD", "IT_WALL_HIT_SOFT", "IT_STONE_HIT",
    // projectiles
    "IT_ARROW_SHOT", "IT_BOW_DRAW", "IT_MAGIC_ARROW_SHOT", "IT_ARROW_STICK_OBJ", "IT_ARROW_STICK_CRE", "IT_ARROW_STICK_HRAD",
    "IT_SLING_SHOT", "IT_SLING_DRAW", "IT_SLING_REFLECT", "IT_BOOMERANG_THROW", "IT_BOOMERANG_FLY", "PL_CATCH_BOOMERANG",
    "IT_HOOKSHOT_READY", "IT_HOOKSHOT_CHAIN", "IT_HOOKSHOT_STICK_OBJ", "IT_HOOKSHOT_REFLECT", "IT_HOOKSHOT_RECEIVE",
    // bombs, nuts, spells
    "IT_BOMB_IGNIT", "IT_BOMB_EXPLOSION", "IT_BOMBCHU_MOVE", "EV_BOMB_BOUND", "IT_DEKU", "PL_MAGIC_FIRE", "PL_MAGIC_WIND_NORMAL",
    "PL_MAGIC_WIND_WARP", "PL_MAGIC_SOUL_NORMAL", "IT_EXPLOSION_FRAME", "IT_FLAME", "PL_ARROW_CHARGE_FIRE", "PL_ARROW_CHARGE_ICE",
    "PL_ARROW_CHARGE_LIGHT", "PL_ICE_BROKEN", "PL_FREEZE", "PL_THROW", "IT_BOMB_UNEXPLOSION", "PL_PULL_UP_POT", "PL_PULL_UP_PLANT", "PL_PULL_UP_ROCK",
    "PL_PULL_UP_WOODBOX", "PL_PUT_OUT_ITEM",
    // Link, pickups, UI
    "PL_DAMAGE", "PL_CHANGE_ARMS", "PL_TAKE_OUT_SHIELD", "PL_ROLL", "SY_GET_RUPY", "SY_GET_ITEM", "SY_HP_RECOVER", "SY_ERROR",
    "SY_LOCK_ON", "SY_FOUND", "SY_CORRECT_CHIME", "SY_TRE_BOX_APPEAR", "SY_DECIDE", "SY_CURSOR", "SY_CANCEL", "SY_GLASSMODE_ON",
    "SY_GLASSMODE_OFF", "SY_SET_FIRE_ARROW",
    // world props
    "EV_EXPLOSION", "EV_DOOR_OPEN", "EV_DOOR_CLOSE", "EV_SLIDE_DOOR_OPEN", "EV_FOOT_SWITCH", "EV_TBOX_UNLOCK", "EV_TBOX_OPEN",
    "EV_FLAME_IGNITION", "EV_TORCH", "EV_BOX_BREAK", "EV_HAMMER_SWITCH", "EV_PLANT_BROKEN", "EV_ROCK_BROKEN", "EV_WEB_VIBRATION",
    "EV_WEB_BROKEN", "EV_WOODBOX_BREAK", "EV_POT_BROKEN", "EV_WALL_BROKEN", "EV_DIAMOND_SWITCH", "EV_BURNING", "EV_RED_EYE",
    "EV_BLOCK_RISING", "EV_BURN_OUT", "EV_FLAME_OF_FIRE", "EV_TREE_CUT",
    // enemies
    "EN_EXTINCT", "EN_DEKU_DAMAGE", "EN_DEKU_DEAD", "EN_DEKU_ATTACK", "EN_GOMA_JR_DAM1", "EN_GOMA_JR_DEAD", "EN_STALTU_DAMAGE",
    "EN_STALWALL_DEAD", "EN_PIHAT_DAMAGE", "EN_PIHAT_SM_DEAD", "EN_TEKU_DAMAGE", "EN_TEKU_DEAD", "EN_OCTAROCK_BUBLE",
    "EN_OCTAROCK_DEAD1", "EN_OCTAROCK_ROCK", "EN_NUTS_DAMAGE", "EN_NUTS_DEAD", "EN_NUTS_THROW", "EN_WOLFOS_DAMAGE",
    "EN_WOLFOS_DEAD", "EN_WOLFOS_ATTACK", "EN_KAICHO_CRY", "EN_KAICHO_DEAD", "EN_FFLY_ATTACK", "EN_FFLY_DEAD", "EN_DODO_J_DAMAGE", "EN_DODO_J_DEAD", "EN_AMOS_DAMAGE", "EN_AMOS_DEAD", "EN_REDEAD_DAMAGE", "EN_REDEAD_DEAD", "EN_RIZA_DAMAGE", "EN_RIZA_DEAD", "EN_FREEZAD_DAMAGE", "EN_FREEZAD_DEAD", "EN_LIKE_DAMAGE", "EN_LIKE_DEAD", "EN_SHELL_DEAD", "EN_BUBLE_DAMAGE", "EN_BUBLE_DEAD", "EN_BUBLE_BITE", "EN_VALVAISA_DAMAGE1", "EN_VALVAISA_DEAD", "EN_BIRI_SPARK", "EN_BIRI_DEAD", "EN_RIVA_DAMAGE", "EN_RIVA_DEAD",
];

/// The clips the Link / enemy / prop code plays by event; the tests resolve every name here.
pub fn all() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = SFX.to_vec();
    for k in jumpman_sim::oot_enemy::KINDS {
        v.push(enemy_hurt(k));
        v.push(enemy_dead(k));
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    /// Every clip the game plays exists, is mono 16-bit, audible and of a sane length; none is a Mario / SM64 sound.
    #[test]
    fn every_referenced_clip_exists_and_is_audible() {
        let names = all();
        assert!(names.len() > 100, "{} clips referenced", names.len());
        for name in names {
            assert!(!name.to_ascii_uppercase().contains("MARIO") && !name.contains("MARIO"), "{name}");
            let rel = format!("{FOLDER}{name}");
            let path = super::super::voices::clip_path(&assets(), &rel, 0);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let clip = super::super::voices::parse_wav(&bytes).unwrap_or_else(|| panic!("{} is not mono 16-bit PCM", path.display()));
            let peak = clip.pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let rms = (clip.pcm.iter().map(|v| v * v).sum::<f32>() / clip.pcm.len() as f32).sqrt();
            let secs = clip.pcm.len() as f32 / clip.rate as f32;
            assert!(peak > 0.3 && rms > 0.02, "{name} is nearly silent (peak {peak}, rms {rms})");
            assert!((0.03..=2.1).contains(&secs), "{name} lasts {secs} s");
        }
    }

    #[test]
    fn every_enemy_kind_has_its_own_hurt_and_death_sound() {
        use std::collections::HashSet;
        let hurt: HashSet<_> = jumpman_sim::oot_enemy::KINDS.iter().map(|k| enemy_hurt(*k)).collect();
        let dead: HashSet<_> = jumpman_sim::oot_enemy::KINDS.iter().map(|k| enemy_dead(*k)).collect();
        assert!(hurt.len() >= 9 && dead.len() >= 9, "{} / {}", hurt.len(), dead.len());
        assert!(hurt.iter().chain(dead.iter()).all(|n| n.starts_with("EN_")));
    }

    #[test]
    fn manifest_lists_what_was_decoded_and_what_was_not() {
        let text = std::fs::read_to_string(assets().join("audio/voices/manifest-oot-sfx.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(v["clips"].as_object().unwrap().len() >= 200);
        for (k, e) in v["clips"].as_object().unwrap() {
            assert!(k.starts_with("voices/oot_sfx/"), "{k}");
            assert!(e["peak"].as_i64().unwrap() > 1000, "{k}");
        }
        assert!(v["skipped"].is_object());
    }
}
