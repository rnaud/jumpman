//! Super Smash Bros. sounds for the twelve playable fighters: their own voices and special-move effects, rendered from
//! the user's ROM by the Smash import (`scripts/export-ssb64-sounds.py` / `byor-rs` `ssb64_sounds.rs`, never bundled,
//! see `native/SSB64.md` "Sounds") into `<data>/assets/audio/voices/ssb64/<Name>_0.wav`, where `Name` is the decomp's
//! `gmFGMVoiceID` name without `nSYAudio` (`VoiceMarioJump`, `FGMCaptainSpecialNPunch`...).
//!
//! * [`smash_sfx`]`(fighter, SmashSfx::NeutralSpecial)` names the clips (layers played together) of one sound;
//!   [`play`] plays them through the voice mixer (mute and the voice log honoured), and any system can send
//!   `SfxCue::Smash { fighter, sfx }` instead of holding the `GameAudio` resource.
//! * The everyday actions reach the fighters through their voice sets in `assets/audio/character-sounds.json`
//!   (`ssb_<stem>`, [`set_name`]): jump, double jump, punches / kicks, hurt, death, the star dance. Those sets are
//!   [`voice_key_sfx`] applied to this table (a test keeps them equal).
//! * Missing files (no Smash import) are silent.
//!
//! Two-part specials: `NeutralSpecialStart` is the wind-up (Captain Falcon's "FALCON..."), `NeutralSpecial` the
//! release ("...PUNCH!", the fireball, the blaster shot); `NeutralSpecialCharge` is a charge loop (Samus, Donkey Kong;
//! clips stop after at most 3.5 s); `NeutralSpecialEnd`, `UpSpecialStart`, `DownSpecialEnd` are follow-ups (the
//! boomerang coming back, Fox's Fire Fox take-off, Jigglypuff waking from Rest). An empty slice means SSB64 has no
//! sound for it.
#![allow(dead_code)] // the lookup API the special moves call into; the voice-set helpers document character-sounds.json
use super::smash::Fighter;
use crate::audio::Sfx;

/// Folder of the exported clips under `<asset root>/audio/`.
pub const CLIP_DIR: &str = "voices/ssb64";

/// A fighter sound, by what it is for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SmashSfx {
    Jump,
    DoubleJump,
    /// Jab / light attack grunt (`Smash1`).
    AttackLight,
    /// Tilt / strong attack grunt (`Smash2`).
    AttackStrong,
    /// Smash attack yell (`Smash3`).
    AttackSmash,
    /// Picking up something heavy (`HeavyGet`).
    HeavyLift,
    Hurt,
    /// Star KO (`DeadUp`, the scream off the top of the screen) with the KO star twinkle.
    Ko,
    /// Falling out of the blast zone (`Dead`).
    Death,
    /// Shield break / dizzy (`FuraFura`).
    Dizzy,
    /// Taunt (`Appeal`).
    Taunt,
    NeutralSpecialStart,
    NeutralSpecialCharge,
    NeutralSpecial,
    NeutralSpecialEnd,
    UpSpecialStart,
    UpSpecial,
    DownSpecial,
    DownSpecialEnd,
}

impl SmashSfx {
    pub const ALL: [SmashSfx; 19] = [
        SmashSfx::Jump,
        SmashSfx::DoubleJump,
        SmashSfx::AttackLight,
        SmashSfx::AttackStrong,
        SmashSfx::AttackSmash,
        SmashSfx::HeavyLift,
        SmashSfx::Hurt,
        SmashSfx::Ko,
        SmashSfx::Death,
        SmashSfx::Dizzy,
        SmashSfx::Taunt,
        SmashSfx::NeutralSpecialStart,
        SmashSfx::NeutralSpecialCharge,
        SmashSfx::NeutralSpecial,
        SmashSfx::NeutralSpecialEnd,
        SmashSfx::UpSpecialStart,
        SmashSfx::UpSpecial,
        SmashSfx::DownSpecial,
        SmashSfx::DownSpecialEnd,
    ];
    pub const SPECIALS: [SmashSfx; 8] = [
        SmashSfx::NeutralSpecialStart,
        SmashSfx::NeutralSpecialCharge,
        SmashSfx::NeutralSpecial,
        SmashSfx::NeutralSpecialEnd,
        SmashSfx::UpSpecialStart,
        SmashSfx::UpSpecial,
        SmashSfx::DownSpecial,
        SmashSfx::DownSpecialEnd,
    ];
}

/// The clips (exported names, played together) of `sfx` for `fighter`. Names are `gmFGMVoiceID` names without the
/// `nSYAudio` prefix; every one is in the import's export list.
pub fn smash_sfx(fighter: Fighter, sfx: SmashSfx) -> &'static [&'static str] {
    use Fighter as F;
    use SmashSfx as S;
    match (fighter, sfx) {
        // ---- Mario: fireball, Super Jump Punch (coins), Mario Tornado
        (F::Mario, S::Jump) => &["VoiceMarioJump"],
        (F::Mario, S::DoubleJump) => &["VoiceMarioJumpAerial"],
        (F::Mario, S::AttackLight) => &["VoiceMarioSmash1"],
        (F::Mario, S::AttackStrong) => &["VoiceMarioSmash2"],
        (F::Mario, S::AttackSmash) => &["VoiceMarioSmash3"],
        (F::Mario, S::HeavyLift) => &["VoiceMarioHeavyGet"],
        (F::Mario, S::Hurt) => &["VoiceMarioDamage"],
        (F::Mario, S::Ko) => &["VoiceMarioDeadUp", "FGMDeadUpStar"],
        (F::Mario, S::Death) => &["VoiceMarioDead"],
        (F::Mario, S::Dizzy) => &["VoiceMarioFuraFura"],
        (F::Mario, S::Taunt) => &["VoiceMarioHereWe"],
        (F::Mario, S::NeutralSpecial) => &["FGMMarioSpecialN"],
        (F::Mario, S::UpSpecial) => &["FGMMarioSpecialHiJump", "FGMMarioSpecialHiCoin"],
        (F::Mario, S::DownSpecial) => &["VoiceMarioSpecialLw"],
        // ---- Luigi: green fireball (Mario's sound), Super Jump Punch, Luigi Cyclone
        (F::Luigi, S::Jump) => &["VoiceLuigiJump"],
        (F::Luigi, S::DoubleJump) => &["VoiceLuigiJumpAerial"],
        (F::Luigi, S::AttackLight) => &["VoiceLuigiSmash1"],
        (F::Luigi, S::AttackStrong) => &["VoiceLuigiSmash2"],
        (F::Luigi, S::AttackSmash) => &["VoiceLuigiSmash3"],
        (F::Luigi, S::HeavyLift) => &["VoiceLuigiHeavyGet"],
        (F::Luigi, S::Hurt) => &["VoiceLuigiDamage"],
        (F::Luigi, S::Ko) => &["VoiceLuigiDeadUp", "FGMDeadUpStar"],
        (F::Luigi, S::Death) => &["VoiceLuigiDead"],
        (F::Luigi, S::Dizzy) => &["VoiceLuigiFuraFura"],
        (F::Luigi, S::Taunt) => &["VoiceLuigiHereWe"],
        (F::Luigi, S::NeutralSpecial) => &["FGMMarioSpecialN"],
        (F::Luigi, S::UpSpecial) => &["FGMMarioSpecialHiJump"],
        (F::Luigi, S::DownSpecial) => &["VoiceLuigiSpecialLw"],
        // ---- Donkey Kong: Giant Punch (charge, swing), Spinning Kong, Hand Slap
        (F::Donkey, S::AttackLight) => &["VoiceDonkeySmash1"],
        (F::Donkey, S::AttackStrong) => &["VoiceDonkeySmash2"],
        (F::Donkey, S::AttackSmash) => &["VoiceDonkeySmash3"],
        (F::Donkey, S::HeavyLift) => &["VoiceDonkeyHeavyGet"],
        (F::Donkey, S::Hurt) => &["VoiceDonkeyDamage"],
        (F::Donkey, S::Ko) => &["VoiceDonkeyDeadUp", "FGMDeadUpStar"],
        (F::Donkey, S::Death) => &["VoiceDonkeyDead1"],
        (F::Donkey, S::Dizzy) => &["VoiceDonkeyFuraFura"],
        (F::Donkey, S::Taunt) => &["VoiceDonkeyAppeal"],
        (F::Donkey, S::NeutralSpecialCharge) => &["FGMDonkeyCharge"],
        (F::Donkey, S::NeutralSpecial) => &["VoiceDonkeySpecialN"],
        (F::Donkey, S::UpSpecial) => &["FGMDonkeySpin"],
        (F::Donkey, S::DownSpecial) => &["FGMDonkeySlap1"],
        // ---- Link: boomerang (throw, return), Spin Attack, bomb
        (F::Link, S::Jump) => &["VoiceLinkJump"],
        (F::Link, S::DoubleJump) => &["VoiceLinkJumpAerial"],
        (F::Link, S::AttackLight) => &["VoiceLinkSmash1"],
        (F::Link, S::AttackStrong) => &["VoiceLinkSmash2"],
        (F::Link, S::AttackSmash) => &["VoiceLinkSmash3"],
        (F::Link, S::HeavyLift) => &["VoiceLinkHeavyGet"],
        (F::Link, S::Hurt) => &["VoiceLinkDamage"],
        (F::Link, S::Ko) => &["VoiceLinkDeadUp", "FGMDeadUpStar"],
        (F::Link, S::Death) => &["VoiceLinkDead"],
        (F::Link, S::Dizzy) => &["VoiceLinkFuraFura"],
        (F::Link, S::Taunt) => &["VoiceLinkGrunt2"],
        (F::Link, S::NeutralSpecial) => &["FGMLinkSpecialNShoot"],
        (F::Link, S::NeutralSpecialEnd) => &["FGMLinkSpecialNReturn"],
        (F::Link, S::UpSpecial) => &["VoiceLinkSpecialHi", "FGMLinkSpecialHi"],
        (F::Link, S::DownSpecial) => &["FGMLinkSpecialLwGet"],
        // ---- Samus: Charge Shot (charge loop, release), Screw Attack, bomb (lay, explode)
        (F::Samus, S::Jump) => &["FGMSamusJump1"],
        (F::Samus, S::DoubleJump) => &["FGMSamusJumpAerial"],
        (F::Samus, S::AttackLight) => &["VoiceSamusSmash1"],
        (F::Samus, S::AttackStrong) => &["VoiceSamusSmash2"],
        (F::Samus, S::AttackSmash) => &["VoiceSamusSmash3"],
        (F::Samus, S::Hurt) => &["VoiceSamusDamage"],
        (F::Samus, S::Ko) => &["VoiceSamusDeadUp", "FGMDeadUpStar"],
        (F::Samus, S::Death) => &["VoiceSamusDead"],
        (F::Samus, S::Dizzy) => &["VoiceSamusFura"],
        (F::Samus, S::Taunt) => &["VoiceSamusAppeal"],
        (F::Samus, S::NeutralSpecialCharge) => &["FGMSamusSpecialNCharge0"],
        (F::Samus, S::NeutralSpecial) => &["FGMSamusSpecialNShootL"],
        (F::Samus, S::UpSpecial) => &["FGMSamusSpecialHi"],
        (F::Samus, S::DownSpecial) => &["FGMSamusSpecialLw"],
        (F::Samus, S::DownSpecialEnd) => &["FGMExplodeS"],
        // ---- Yoshi: Egg Lay (tongue, egg), Egg Throw, Yoshi Bomb (jump, fall)
        (F::Yoshi, S::Jump) => &["VoiceYoshiJump"],
        (F::Yoshi, S::DoubleJump) => &["VoiceYoshiJumpAerial"],
        (F::Yoshi, S::AttackLight) => &["VoiceYoshiSmash1"],
        (F::Yoshi, S::AttackStrong) => &["VoiceYoshiSmash2"],
        (F::Yoshi, S::AttackSmash) => &["VoiceYoshiSmash3"],
        (F::Yoshi, S::HeavyLift) => &["VoiceYoshiHeavyGet"],
        (F::Yoshi, S::Hurt) => &["VoiceYoshiDamage"],
        (F::Yoshi, S::Ko) => &["VoiceYoshiDeadUp", "FGMDeadUpStar"],
        (F::Yoshi, S::Death) => &["VoiceYoshiDead"],
        (F::Yoshi, S::Dizzy) => &["VoiceYoshiFuraFura"],
        (F::Yoshi, S::Taunt) => &["VoiceYoshiAppeal"],
        (F::Yoshi, S::NeutralSpecial) => &["FGMYoshiSpecialNTongue"],
        (F::Yoshi, S::NeutralSpecialEnd) => &["FGMYoshiEggLayShatter"],
        (F::Yoshi, S::UpSpecial) => &["VoiceYoshiThrow", "FGMYoshiSpecialHiThrow"],
        (F::Yoshi, S::DownSpecial) => &["VoiceYoshiSpecialLwJump"],
        (F::Yoshi, S::DownSpecialEnd) => &["VoiceYoshiSpecialLwFall"],
        // ---- Kirby: Inhale (loop, spit), Final Cutter, Stone
        (F::Kirby, S::Jump) => &["FGMKirbyPurinJump"],
        (F::Kirby, S::DoubleJump) => &["FGMInflateJump1"],
        (F::Kirby, S::AttackLight) => &["VoiceKirbySmash1"],
        (F::Kirby, S::AttackStrong) => &["VoiceKirbySmash2"],
        (F::Kirby, S::AttackSmash) => &["VoiceKirbySmash3"],
        (F::Kirby, S::HeavyLift) => &["VoiceKirbyHeavyGet"],
        (F::Kirby, S::Hurt) => &["VoiceKirbyDamage"],
        (F::Kirby, S::Ko) => &["VoiceKirbyDeadUp", "FGMDeadUpStar"],
        (F::Kirby, S::Death) => &["VoiceKirbyDead"],
        (F::Kirby, S::Dizzy) => &["VoiceKirbyFuraFura"],
        (F::Kirby, S::Taunt) => &["VoiceKirbyAppeal"],
        (F::Kirby, S::NeutralSpecial) => &["FGMKirbySpecialNStart"],
        (F::Kirby, S::NeutralSpecialEnd) => &["FGMKirbySpecialNThrow"],
        (F::Kirby, S::UpSpecial) => &["VoiceKirbySpecialHi"],
        (F::Kirby, S::DownSpecial) => &["VoiceKirbySpecialLw", "FGMKirbySpecialLwStart"],
        // ---- Fox: Blaster, Fire Fox (take-off, flight), Reflector (on, reflect)
        (F::Fox, S::DoubleJump) => &["VoiceFoxJumpAerial"],
        (F::Fox, S::AttackLight) => &["VoiceFoxSmash1"],
        (F::Fox, S::AttackStrong) => &["VoiceFoxSmash2"],
        (F::Fox, S::AttackSmash) => &["VoiceFoxSmash3"],
        (F::Fox, S::HeavyLift) => &["VoiceFoxHeavyGet"],
        (F::Fox, S::Hurt) => &["VoiceFoxDamage"],
        (F::Fox, S::Ko) => &["VoiceFoxDeadUp", "FGMDeadUpStar"],
        (F::Fox, S::Death) => &["VoiceFoxDead"],
        (F::Fox, S::Dizzy) => &["VoiceFoxFuraFura"],
        (F::Fox, S::Taunt) => &["VoiceFoxAppeal"],
        (F::Fox, S::NeutralSpecial) => &["FGMFoxBlaster"],
        (F::Fox, S::UpSpecialStart) => &["VoiceFoxSpecialHi", "FGMFoxSpecialHiStart"],
        (F::Fox, S::UpSpecial) => &["FGMFoxSpecialHiFly"],
        (F::Fox, S::DownSpecial) => &["FGMFoxSpecialLwStart"],
        (F::Fox, S::DownSpecialEnd) => &["FGMFoxSpecialLwHit"],
        // ---- Pikachu: Thunder Jolt, Quick Attack, Thunder (call, bolt)
        (F::Pikachu, S::AttackLight) => &["VoicePikachuSmash1"],
        (F::Pikachu, S::AttackStrong) => &["VoicePikachuSmash2"],
        (F::Pikachu, S::AttackSmash) => &["VoicePikachuSmash3"],
        (F::Pikachu, S::HeavyLift) => &["VoicePikachuHeavyGet"],
        (F::Pikachu, S::Hurt) => &["VoicePikachuDamage"],
        (F::Pikachu, S::Ko) => &["VoicePikachuDeadUp", "FGMDeadUpStar"],
        (F::Pikachu, S::Death) => &["VoicePikachuDead"],
        (F::Pikachu, S::Dizzy) => &["VoicePikachuFuraFura"],
        (F::Pikachu, S::Taunt) => &["VoicePikachuAppeal"],
        (F::Pikachu, S::NeutralSpecial) => &["VoicePikachuSpecialN", "FGMPikachuElectric1"],
        (F::Pikachu, S::UpSpecial) => &["VoicePikachuSpecialHi", "FGMPikachuSpecialHiStart"],
        (F::Pikachu, S::DownSpecial) => &["VoicePikachuSpecialLw"],
        (F::Pikachu, S::DownSpecialEnd) => &["FGMPikachuSpecialLwThunder"],
        // ---- Jigglypuff: Pound, Sing, Rest (sleep, wake)
        (F::Purin, S::Jump) => &["FGMKirbyPurinJump"],
        (F::Purin, S::DoubleJump) => &["FGMInflateJump1"],
        (F::Purin, S::AttackLight) => &["VoicePurinSmash1"],
        (F::Purin, S::AttackStrong) => &["VoicePurinSmash2"],
        (F::Purin, S::AttackSmash) => &["VoicePurinSmash3"],
        (F::Purin, S::HeavyLift) => &["VoicePurinUnkGrunt3"],
        (F::Purin, S::Hurt) => &["VoicePurinDamage"],
        (F::Purin, S::Ko) => &["VoicePurinDeadUp", "FGMDeadUpStar"],
        (F::Purin, S::Death) => &["VoicePurinUnkGrunt4"],
        (F::Purin, S::Dizzy) => &["VoicePurinFuraFura"],
        (F::Purin, S::Taunt) => &["VoicePurinAppeal"],
        (F::Purin, S::NeutralSpecial) => &["VoicePurinSpecialN"],
        (F::Purin, S::UpSpecial) => &["VoicePurinSpecialHi"],
        (F::Purin, S::DownSpecial) => &["VoicePurinSpecialLwSleep"],
        (F::Purin, S::DownSpecialEnd) => &["VoicePurinSpecialLwWake"],
        // ---- Captain Falcon: Falcon Punch ("FALCON..." then "...PUNCH!"), Falcon Dive, Falcon Kick
        (F::Captain, S::DoubleJump) => &["VoiceCaptainJumpAerial"],
        (F::Captain, S::AttackLight) => &["VoiceCaptainSmash1"],
        (F::Captain, S::AttackStrong) => &["VoiceCaptainSmash2"],
        (F::Captain, S::AttackSmash) => &["VoiceCaptainSmash3"],
        (F::Captain, S::HeavyLift) => &["VoiceCaptainHeavyGet"],
        (F::Captain, S::Hurt) => &["VoiceCaptainDamage"],
        (F::Captain, S::Ko) => &["VoiceCaptainDeadUp", "FGMDeadUpStar"],
        (F::Captain, S::Death) => &["VoiceCaptainDead"],
        (F::Captain, S::Dizzy) => &["VoiceCaptainFuraFura"],
        (F::Captain, S::Taunt) => &["VoiceCaptainAppeal"],
        (F::Captain, S::NeutralSpecialStart) => &["VoiceCaptainSpecialNFalcon", "FGMCaptainSpecialNStart"],
        (F::Captain, S::NeutralSpecial) => &["VoiceCaptainSpecialNPunch", "FGMCaptainSpecialNPunch"],
        (F::Captain, S::UpSpecial) => &["VoiceCaptainSpecialHi", "FGMCaptainSpecialHi"],
        (F::Captain, S::DownSpecial) => &["VoiceCaptainSpecialLw"],
        // ---- Ness: PK Fire, PK Thunder (call, loop), PSI Magnet
        (F::Ness, S::Jump) => &["FGMNessJump"],
        (F::Ness, S::AttackLight) => &["VoiceNessSmash1"],
        (F::Ness, S::AttackStrong) => &["VoiceNessSmash2"],
        (F::Ness, S::AttackSmash) => &["VoiceNessSmash3"],
        (F::Ness, S::HeavyLift) => &["VoiceNessHeavyGet"],
        (F::Ness, S::Hurt) => &["VoiceNessDamage"],
        (F::Ness, S::Ko) => &["VoiceNessDeadUp", "FGMDeadUpStar"],
        (F::Ness, S::Death) => &["VoiceNessDead"],
        (F::Ness, S::Dizzy) => &["VoiceNessFuraFura"],
        (F::Ness, S::Taunt) => &["VoiceNessAppeal"],
        (F::Ness, S::NeutralSpecial) => &["VoiceNessSpecialN", "FGMNessPSI"],
        (F::Ness, S::UpSpecialStart) => &["VoiceNessSpecialHi"],
        (F::Ness, S::UpSpecial) => &["FGMNessPKThunderLoop"],
        (F::Ness, S::DownSpecial) => &["FGMNessSpecialLwStart"],
        _ => &[],
    }
}

/// `voices/ssb64/<name>`: the layer name `character-sounds.json` and [`crate::audio`]'s voice mixer use.
pub fn clip(name: &str) -> String {
    format!("{CLIP_DIR}/{name}")
}

/// The fighter's voice set in `character-sounds.json`.
pub fn set_name(fighter: Fighter) -> String {
    format!("ssb_{}", fighter.stem())
}

/// Which fighter sound each `character-sounds.json` action plays (the SM64 action triggers: `Sfx::jump`, `kick`,
/// `hurt`, `death`, ... and the `MARIO_*` voices `Sfx::named` hands to non-Mario sets: the punch sequence, pick-ups).
/// `star` also plays the SM64 star jingle first; `deathLanding` stays silent.
pub const VOICE_KEYS: [(&str, Option<SmashSfx>); 24] = [
    ("jump", Some(SmashSfx::Jump)),
    ("backflip", Some(SmashSfx::DoubleJump)),
    ("doubleJump", Some(SmashSfx::DoubleJump)),
    ("tripleJump", Some(SmashSfx::AttackSmash)),
    ("longJump", Some(SmashSfx::AttackStrong)),
    ("kick", Some(SmashSfx::AttackLight)),
    ("wallKick", Some(SmashSfx::DoubleJump)),
    ("wingPull", Some(SmashSfx::DoubleJump)),
    ("flightBurn", Some(SmashSfx::Hurt)),
    ("cannonFire", Some(SmashSfx::Ko)),
    ("bowserGrab", Some(SmashSfx::HeavyLift)),
    ("bowserThrowFast", Some(SmashSfx::AttackSmash)),
    ("bowserThrow", Some(SmashSfx::AttackStrong)),
    ("hurt", Some(SmashSfx::Hurt)),
    ("death", Some(SmashSfx::Death)),
    ("groundPoundDrop", Some(SmashSfx::AttackStrong)),
    ("dive", Some(SmashSfx::AttackLight)),
    ("deathLanding", None),
    ("mamaMia", Some(SmashSfx::Dizzy)),
    ("star", Some(SmashSfx::Taunt)),
    ("MARIO_PUNCH_YAH", Some(SmashSfx::AttackLight)),
    ("MARIO_PUNCH_WAH", Some(SmashSfx::AttackStrong)),
    ("MARIO_PUNCH_HOO", Some(SmashSfx::AttackSmash)),
    ("MARIO_HRMM", Some(SmashSfx::HeavyLift)),
];

/// The fighter sound a `character-sounds.json` action plays (see [`VOICE_KEYS`]).
pub fn voice_key_sfx(key: &str) -> Option<SmashSfx> {
    VOICE_KEYS.iter().find(|(k, _)| *k == key).and_then(|(_, s)| *s)
}

/// Play `sfx` for `fighter` now (all layers at once). Returns the layer names (`voices/ssb64/...`) requested; nothing
/// plays while muted, and files missing from the data dir are silent.
pub fn play(audio: &mut Sfx, fighter: Fighter, sfx: SmashSfx) -> Vec<String> {
    let layers: Vec<String> = smash_sfx(fighter, sfx).iter().map(|n| clip(n)).collect();
    for name in &layers {
        audio.voice_log.push((format!("smash:{sfx:?}"), name.clone(), 1.0));
        if audio.voice_log.len() > 256 {
            audio.voice_log.remove(0);
        }
        if !audio.muted {
            let (file, loaded) = crate::audio::voices::play(name, 1.0);
            if std::env::var_os("JUMPMAN_VOICE_LOG").is_some() {
                eprintln!("smash[{}] {sfx:?} clip {file}{}", fighter.stem(), if loaded { "" } else { " (file missing: silent)" });
            }
        }
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name the game asks for, deduplicated (the import's export list must cover these).
    fn used() -> std::collections::BTreeSet<&'static str> {
        Fighter::ALL.iter().flat_map(|f| SmashSfx::ALL.iter().flat_map(move |s| smash_sfx(*f, *s).iter().copied())).collect()
    }

    #[test]
    fn every_fighter_has_its_core_voices_and_specials() {
        for f in Fighter::ALL {
            for s in [SmashSfx::AttackLight, SmashSfx::AttackStrong, SmashSfx::AttackSmash, SmashSfx::Hurt, SmashSfx::Ko, SmashSfx::Death, SmashSfx::Taunt] {
                assert!(!smash_sfx(f, s).is_empty(), "{f:?} has no {s:?}");
            }
            assert!(!smash_sfx(f, SmashSfx::NeutralSpecial).is_empty(), "{f:?} has no neutral special sound");
            assert!(!smash_sfx(f, SmashSfx::UpSpecial).is_empty(), "{f:?} has no up special sound");
            // a fighter's voices are its own: Luigi borrows only Mario's effects (the fireball), never his voice
            for s in SmashSfx::ALL {
                for name in smash_sfx(f, s) {
                    if let Some(rest) = name.strip_prefix("Voice") {
                        let own = match f {
                            Fighter::Donkey => "Donkey",
                            Fighter::Purin => "Purin",
                            Fighter::Captain => "Captain",
                            other => &other.label()[other.label().rfind(' ').map_or(0, |i| i + 1)..],
                        };
                        assert!(rest.to_ascii_lowercase().starts_with(&own.to_ascii_lowercase()), "{f:?}/{s:?} plays {name}");
                    } else {
                        assert!(name.starts_with("FGM"), "{name}");
                    }
                }
            }
        }
        assert_eq!(smash_sfx(Fighter::Captain, SmashSfx::NeutralSpecialStart)[0], "VoiceCaptainSpecialNFalcon");
        assert_eq!(smash_sfx(Fighter::Captain, SmashSfx::NeutralSpecial)[0], "VoiceCaptainSpecialNPunch");
        assert_eq!(smash_sfx(Fighter::Ness, SmashSfx::NeutralSpecial)[0], "VoiceNessSpecialN");
        assert_eq!(smash_sfx(Fighter::Mario, SmashSfx::NeutralSpecial), smash_sfx(Fighter::Luigi, SmashSfx::NeutralSpecial));
        assert_eq!(clip("VoiceMarioJump"), "voices/ssb64/VoiceMarioJump");
        assert_eq!(set_name(Fighter::Purin), "ssb_purin");
    }

    /// The names are exactly what the importer writes: its export list is every `Voice<Fighter>*` plus EXPORT_FGM.
    #[test]
    fn names_are_in_the_import_export_list() {
        let py = include_str!("../../../scripts/export-ssb64-sounds.py");
        let fgm = &py[py.find("EXPORT_FGM = [").unwrap()..];
        let fgm = &fgm[..fgm.find(']').unwrap()];
        let rs = include_str!("../../byor-rs/src/ssb64_sounds.rs");
        for name in used() {
            if name.starts_with("FGM") {
                assert!(fgm.contains(&format!("'{name}'")), "{name} is not in export-ssb64-sounds.py EXPORT_FGM");
                assert!(rs.contains(&format!("\"{name}\"")), "{name} is not in ssb64_sounds.rs EXPORT_FGM");
            } else {
                assert!(name.starts_with("Voice"), "{name}");
            }
        }
        assert!(used().len() > 150, "{}", used().len());
    }

    /// `character-sounds.json` holds one set per fighter that is this table applied to [`VOICE_KEYS`].
    #[test]
    fn voice_sets_match_the_table() {
        let data: serde_json::Value = serde_json::from_str(include_str!("../../../assets/audio/character-sounds.json")).unwrap();
        for f in Fighter::ALL {
            assert_eq!(data["characters"][f.id()].as_str(), Some(set_name(f).as_str()), "{f:?}");
            let set = data["sets"][set_name(f)].as_object().unwrap_or_else(|| panic!("no set {}", set_name(f)));
            for (key, sfx) in VOICE_KEYS {
                let mut want: Vec<(String, f64)> = sfx.map_or(vec![], |s| smash_sfx(f, s).iter().map(|n| (clip(n), 1.0)).collect());
                if key == "star" {
                    want.insert(0, ("MENU_STAR_SOUND".to_string(), 1.0));
                }
                let got: Vec<(String, f64)> = set.get(key).map(|v| serde_json::from_value(v.clone()).unwrap()).unwrap_or_default();
                assert_eq!(got, want, "{}/{key}", set_name(f));
            }
            assert_eq!(set.len(), VOICE_KEYS.len(), "{}: keys outside VOICE_KEYS", set_name(f));
        }
    }

    /// With the Smash import in the asset tree (`assets/audio/voices/ssb64`, written by the importer; never committed),
    /// every clip the table names exists, decodes and is audible. Skipped without the import.
    #[test]
    fn exported_clips_exist_for_all_fighters() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets");
        let dir = root.join("audio").join(CLIP_DIR);
        if !dir.join("manifest-ssb64.json").is_file() {
            eprintln!("skipped: no Smash sounds at {} (import the ROM or run scripts/export-ssb64-sounds.py)", dir.display());
            return;
        }
        for name in used() {
            let path = crate::audio::voices::clip_path(&root, &clip(name), 0);
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let c = crate::audio::voices::parse_wav(&bytes).unwrap_or_else(|| panic!("{} is not mono 16-bit PCM", path.display()));
            let peak = c.pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let seconds = c.pcm.len() as f32 / c.rate as f32;
            assert!(peak > 0.1, "{name} is nearly silent (peak {peak})");
            assert!((0.05..=3.6).contains(&seconds), "{name} lasts {seconds} s");
        }
    }

    #[test]
    fn play_logs_layers_and_respects_mute() {
        let mut s = crate::audio::test_sfx(true);
        let played = play(&mut s, Fighter::Captain, SmashSfx::NeutralSpecial);
        assert_eq!(played, ["voices/ssb64/VoiceCaptainSpecialNPunch", "voices/ssb64/FGMCaptainSpecialNPunch"]);
        assert_eq!(s.voice_log.last().unwrap().1, "voices/ssb64/FGMCaptainSpecialNPunch");
        assert!(play(&mut s, Fighter::Donkey, SmashSfx::Jump).is_empty());
    }
}
