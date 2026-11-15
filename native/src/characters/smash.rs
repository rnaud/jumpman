//! The twelve Super Smash Bros. fighters as playable characters: skins on Mario's controller and physics, exactly like
//! Bowser (`bowser.rs`). The sim keeps playing Mario (`GameSim::{as_link, as_banjo}` stay off), so every course, enemy,
//! star and mode rule sees Mario's moves and Mario's hitbox; only the model changes. One implementation serves every
//! fighter: the models are the Smash import's skinned glbs (`public/models/ssb64/<fighter>.glb`, from the user's own ROM,
//! never bundled, see `native/SSB64.md`) and the clip of each move comes from the import's `fighters.json` motion table
//! (decomp motion name -> the glb clip that plays it), so a fighter is data, not code.
//!
//! Mario action -> Smash motion ([`Move`] -> [`Move::motions`], the first motion the fighter has wins; the fighters that
//! lack all of them fall back to `Wait`, the tests list every such gap):
//!
//! | Mario actions                                                   | Smash motion                       | playback              |
//! |-----------------------------------------------------------------|------------------------------------|-----------------------|
//! | idle, `*_LAND_STOP`, panting, reading, waiting, holding idle    | `Wait`                             | loop                  |
//! | walking / decelerating / holding walk                           | `WalkSlow` / `WalkMiddle` / `WalkFast` / `Run` by speed | loop, rate from speed |
//! | turning around, braking                                         | `RunBrake`                         | once, holds           |
//! | crouch start / crouching, crawling / crouch end                 | `Squat` / `SquatWait` / `SquatRv`  | once / loop / once    |
//! | jump, steep jump, wall kick, long jump (rising; falling: `Fall`)| `JumpF`                            | once, holds           |
//! | double jump, side flip                                          | `JumpAerialF` (Kirby, Jigglypuff `JumpAerialF1`) | once, holds |
//! | triple jump                                                     | `JumpAerialF` (`JumpAerialF2`)     | once, holds           |
//! | backflip                                                        | `JumpAerialB` (`JumpAerialF3`)     | once, holds           |
//! | freefall and every other airborne action                        | `Fall`                             | loop                  |
//! | landings                                                        | `LandingLight` (the `KneeBend` clip) | once                |
//! | punch, second punch, kick (Mario's punch sequence)              | `Attack11`, `Attack12`, `Attack13` (else `AttackS3`) | once per hit |
//! | jump kick                                                       | `AttackAirN`                       | once                  |
//! | dive                                                            | `AttackAirF`                       | once, holds           |
//! | slide kick                                                      | `AttackDash`                       | once, holds           |
//! | dive slide, stomach slide                                       | `DownBounceD` (lying face down)    | once, holds           |
//! | butt slide                                                      | `SquatWait`                        | loop                  |
//! | ground pound / its landing                                      | `AttackAirLw` / `LandingAirLw`     | once                  |
//! | soft knockback, squish                                          | `DamageN1`                         | once                  |
//! | hard / ground knockback (backward, forward)                     | `DownBounceU`, `DownBounceD`       | once, holds           |
//! | air knockback, blown away / hard air knockback, burning         | `DamageFlyN` / `DamageFlyRoll`     | once / loop           |
//! | shocked, electrocuted                                           | `DamageE`                          | loop                  |
//! | bonks                                                           | `WallDamage`                       | once                  |
//! | deaths                                                          | `DownBounceU`                      | once, holds           |
//! | sleeping / waking up                                            | `SquatWait` / `SquatRv`            | loop / once           |
//! | star dances, grand star                                         | `Appeal` (the taunt)               | loop                  |
//! | ledge grab, hanging, poles                                      | `CliffWait`                        | loop                  |
//! | ledge climbs (slow / fast)                                      | `CliffClimbSlow1` / `CliffClimbQuick1` | once              |
//! | picking up / throwing                                           | `LightGet` / `LightThrowF`         | once                  |
//! | swimming (SSB64 cannot swim)                                    | `Fall`, tilted forward on strokes  | loop, slow            |
//! | flying, shot from a cannon                                      | `JumpF`, tilted forward            | once, holds           |
//!
//! Gem Clash maps its hero states straight to motions ([`gem_clash_clip`]): punches `Attack11` / `Attack12` / `AttackS3`,
//! swing `AttackS4`, spin `AttackHi4`, shoot / beam `SpecialN`, pound `AttackAirLw`, hurt / down / get up `DamageN1` /
//! `DownBounceD` / `DownStandD`.
//!
//! Size: every fighter is drawn at one common scale that makes Smash Mario as tall as SM64 Mario ([`BASE_SCALE`]), so
//! their relative sizes are Smash's own (Kirby, Jigglypuff, Pikachu small, Donkey Kong big) with a [`Fighter::trim`] for
//! the outliers; the hitbox stays Mario's, so every course still fits. Big fighters get the follow camera further back
//! ([`Fighter::camera_scale`]). Speeds and jumps stay Mario's ([`ATTRIBUTE_SPEEDS`] is off, see its note).
//!
//! What the export has and lacks: Link's sword and shield and Samus's arm cannon are part of their models (joints of the
//! high-poly skeleton), so they swing and aim with the clips. Missing: the hand swaps (open / closed fists stay as modelled),
//! face swaps (blinks), items, and voices: the fighters use the `ssb64` id, mapped to the `silent` voice set (SM64's hit
//! sound and the star jingle).
//!
//! Specials: the special button (N64 R: `Action::RButton`, H / R2; [`smash_input`]) plays the fighter's neutral / up / down
//! special on the sim (`jumpman_sim::smash`, see `native/SSB64.md`); while one runs this view plays its motions
//! (`SmashExt::motion`) instead of the table above and holds the root-motion joint still ([`root_motion_clips`]).
//! Projectiles, effects, sound messages and goomba hits are `smash_fx.rs`.
//! In Gem Clash the fighters are drawn between 0.8x and 1.25x Mario's height ([`Fighter::gem_clash_scale`]): the arena
//! camera and the power form (which doubles the hero) are built around Mario.
//! Missing glb or `fighters.json`: the fighter is left out of the roster and the V cycle; selecting it anyway (env) toasts
//! and draws nothing.
use super::Character;
use crate::game::runner::GameSim;
use crate::hud::Toast;
use crate::render::object_view::{spawn_model, AnimRig};
use crate::render::{to_bevy, AssetRoot, SCALE};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use jumpman_sim::math::to_s16;
use jumpman_sim::state::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Folder of the Smash import, relative to the asset root.
pub const DIR: &str = "../public/models/ssb64";
/// The import's index (motion tables, clip lengths, attributes).
pub const INDEX: &str = "../public/models/ssb64/fighters.json";
/// Smash Mario's bind-pose height in glb units (`native/SSB64.md`).
pub const SMASH_MARIO_HEIGHT: f32 = 1.84;
/// SM64 Mario's height in SM64 units.
pub const MARIO_HEIGHT: f32 = 160.0;
/// The common model scale: Smash Mario stands as tall as SM64 Mario.
pub const BASE_SCALE: f32 = MARIO_HEIGHT * SCALE / SMASH_MARIO_HEIGHT;
/// Scale running speed and jump height per fighter from `fighters.json` attributes. Off: the courses are built around
/// Mario's numbers (gaps, ledges, star heights), and a fast fighter (Captain Falcon runs 1.7x Mario in SSB64) or a floaty one
/// (Jigglypuff) would break traversal. The ratios are in [`Attributes::speed_ratio`] / [`Attributes::jump_ratio`].
#[allow(dead_code)]
pub const ATTRIBUTE_SPEEDS: bool = false;

/// One of the twelve playable Smash fighters (Master Hand is a boss, not playable).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Fighter {
    Mario,
    Luigi,
    Donkey,
    Link,
    Samus,
    Yoshi,
    Kirby,
    Fox,
    Pikachu,
    Purin,
    Captain,
    Ness,
}

impl Fighter {
    pub const ALL: [Fighter; 12] = [
        Fighter::Mario,
        Fighter::Luigi,
        Fighter::Donkey,
        Fighter::Link,
        Fighter::Samus,
        Fighter::Yoshi,
        Fighter::Kirby,
        Fighter::Fox,
        Fighter::Pikachu,
        Fighter::Purin,
        Fighter::Captain,
        Fighter::Ness,
    ];
    pub const COUNT: usize = Self::ALL.len();

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    /// File stem and `fighters.json` key of the import.
    pub fn stem(self) -> &'static str {
        match self {
            Fighter::Mario => "mario",
            Fighter::Luigi => "luigi",
            Fighter::Donkey => "donkey",
            Fighter::Link => "link",
            Fighter::Samus => "samus",
            Fighter::Yoshi => "yoshi",
            Fighter::Kirby => "kirby",
            Fighter::Fox => "fox",
            Fighter::Pikachu => "pikachu",
            Fighter::Purin => "purin",
            Fighter::Captain => "captain",
            Fighter::Ness => "ness",
        }
    }

    /// The character id (`JUMPMAN_CHARACTER`, the shell's `pick:` / `char:`, settings).
    pub fn id(self) -> &'static str {
        match self {
            Fighter::Mario => "ssb-mario",
            Fighter::Luigi => "ssb-luigi",
            Fighter::Donkey => "ssb-dk",
            Fighter::Link => "ssb-link",
            Fighter::Samus => "ssb-samus",
            Fighter::Yoshi => "ssb-yoshi",
            Fighter::Kirby => "ssb-kirby",
            Fighter::Fox => "ssb-fox",
            Fighter::Pikachu => "ssb-pikachu",
            Fighter::Purin => "ssb-jigglypuff",
            Fighter::Captain => "ssb-falcon",
            Fighter::Ness => "ssb-ness",
        }
    }

    /// `ssb-<id>`, also `ssb-<file stem>` (`ssb-donkey`, `ssb-purin`, `ssb-captain`) and a few common names.
    pub fn from_id(name: &str) -> Option<Self> {
        let rest = name.strip_prefix("ssb-").or_else(|| name.strip_prefix("smash-"))?;
        Self::ALL.into_iter().find(|f| f.id() == name || f.stem() == rest).or(match rest {
            "donkey-kong" => Some(Fighter::Donkey),
            "puff" => Some(Fighter::Purin),
            "captain-falcon" => Some(Fighter::Captain),
            _ => None,
        })
    }

    /// Label in menus and toasts (the Smash Mario and Link are told apart from SM64's Mario and OoT's Link).
    pub fn label(self) -> &'static str {
        match self {
            Fighter::Mario => "SMASH MARIO",
            Fighter::Luigi => "LUIGI",
            Fighter::Donkey => "DONKEY KONG",
            Fighter::Link => "SMASH LINK",
            Fighter::Samus => "SAMUS",
            Fighter::Yoshi => "YOSHI",
            Fighter::Kirby => "KIRBY",
            Fighter::Fox => "FOX",
            Fighter::Pikachu => "PIKACHU",
            Fighter::Purin => "JIGGLYPUFF",
            Fighter::Captain => "CAPTAIN FALCON",
            Fighter::Ness => "NESS",
        }
    }

    /// The name on a roster tile (at most 6 characters: the tiles are small; the SMASH group header says the rest).
    pub fn short(self) -> &'static str {
        match self {
            Fighter::Mario => "MARIO",
            Fighter::Luigi => "LUIGI",
            Fighter::Donkey => "DK",
            Fighter::Link => "LINK",
            Fighter::Samus => "SAMUS",
            Fighter::Yoshi => "YOSHI",
            Fighter::Kirby => "KIRBY",
            Fighter::Fox => "FOX",
            Fighter::Pikachu => "PIKA",
            Fighter::Purin => "PUFF",
            Fighter::Captain => "FALCON",
            Fighter::Ness => "NESS",
        }
    }

    /// Mixed-case name (settings, logs).
    pub fn name(self) -> &'static str {
        match self {
            Fighter::Mario => "Mario (Smash)",
            Fighter::Luigi => "Luigi",
            Fighter::Donkey => "Donkey Kong",
            Fighter::Link => "Link (Smash)",
            Fighter::Samus => "Samus",
            Fighter::Yoshi => "Yoshi",
            Fighter::Kirby => "Kirby",
            Fighter::Fox => "Fox",
            Fighter::Pikachu => "Pikachu",
            Fighter::Purin => "Jigglypuff",
            Fighter::Captain => "Captain Falcon",
            Fighter::Ness => "Ness",
        }
    }

    /// Two lines for the character select (moves, blurb). The moves are Mario's (a skin), named as the fighter plays them.
    pub fn blurb(self) -> (&'static str, &'static str) {
        match self {
            Fighter::Mario => ("PUNCH, KICK, DRILL POUND", "SMASH BROS. MARIO ON SM64 MOVES"),
            Fighter::Luigi => ("PUNCH, KICK, DRILL POUND", "MARIO'S TALLER BROTHER"),
            Fighter::Donkey => ("PUNCHES, SMASH, STOMP", "THE BIGGEST FIGHTER: MARIO'S MOVES, TWICE THE SIZE"),
            Fighter::Link => ("SWORD, SPIN, DOWN THRUST", "SMASH BROS. LINK: SWORD AND SHIELD ON MARIO'S MOVES"),
            Fighter::Samus => ("CANNON BLOWS, KICKS, STOMP", "BOUNTY HUNTER IN THE POWER SUIT"),
            Fighter::Yoshi => ("JABS, KICKS, YOSHI BOMB", "THE DINOSAUR: FLUTTER-FREE MARIO MOVES"),
            Fighter::Kirby => ("JABS, KICK, STONE DROP", "FIVE MIDAIR JUMP POSES, MARIO'S PHYSICS"),
            Fighter::Fox => ("JABS, KICKS, DRILL", "STAR FOX LEADER ON FOOT"),
            Fighter::Pikachu => ("HEADBUTT, KICK, THUNDER DROP", "SMALL AND QUICK-LOOKING"),
            Fighter::Purin => ("SLAPS, KICKS, DRILL", "FIVE MIDAIR JUMP POSES, MARIO'S PHYSICS"),
            Fighter::Captain => ("JABS, KNEE, STOMP", "F-ZERO RACER ON FOOT"),
            Fighter::Ness => ("JABS, BAT SWING, STOMP", "PSI KID FROM ONETT"),
        }
    }

    /// Extra visual scale on top of [`BASE_SCALE`] (Smash's own sizes otherwise). Donkey Kong stands 2.5x Mario upright in
    /// SSB64: trimmed so he towers like Bowser does without filling the screen; the small ones are left small.
    pub fn trim(self) -> f32 {
        match self {
            Fighter::Donkey => 0.72,
            _ => 1.0,
        }
    }

    /// Bind-pose height in glb units (the import's `POSITION` bounds; `native/SSB64.md`). Only used for the camera and the
    /// clip rates: the model scale itself is [`BASE_SCALE`] for everyone.
    pub fn bind_height(self) -> f32 {
        match self {
            Fighter::Mario => 1.84,
            Fighter::Luigi => 2.12,
            Fighter::Donkey => 4.67,
            Fighter::Link => 2.38,
            Fighter::Samus => 2.48,
            Fighter::Yoshi => 2.25,
            Fighter::Kirby => 1.17,
            Fighter::Fox => 2.15,
            Fighter::Pikachu => 1.41,
            Fighter::Purin => 1.28,
            Fighter::Captain => 2.69,
            Fighter::Ness => 1.80,
        }
    }

    /// Drawn height relative to SM64 Mario.
    pub fn height_ratio(self) -> f32 {
        self.bind_height() * self.trim() / SMASH_MARIO_HEIGHT
    }

    /// The model's scale in the world.
    pub fn scale(self) -> f32 {
        BASE_SCALE * self.trim()
    }

    /// Follow-camera distance factor (1 up to Mario's size, further back for the big ones).
    pub fn camera_scale(self) -> f64 {
        (1.0 + (self.height_ratio() - 1.15).max(0.0) * 0.4).min(1.4) as f64
    }

    /// The model's scale in Gem Clash: the arena is framed for Mario-sized heroes and the power form doubles them, so the
    /// fighters are kept within 0.8x-1.25x Mario's height there (Donkey Kong would otherwise fill the screen).
    pub fn gem_clash_scale(self) -> f32 {
        let r = self.height_ratio();
        self.scale() * r.clamp(0.8, 1.25) / r
    }

    pub fn model(self) -> String {
        format!("{DIR}/{}.glb", self.stem())
    }

    /// The sim's fighter (its specials, `jumpman_sim::smash`).
    pub fn sim(self) -> jumpman_sim::smash::SmashFighter {
        jumpman_sim::smash::SmashFighter::ALL[self.index()]
    }

    pub fn from_sim(f: jumpman_sim::smash::SmashFighter) -> Self {
        Self::ALL[f.index()]
    }
}

/// The clips whose `joint_00` (SSB64's `TransN`, the root-motion joint) starts at the origin: the TRANSN-flagged motions
/// (Super Jump Punch, Falcon Punch / Dive / Kick, Final Cutter, Pound...) whose travel the sim already does, so the view
/// holds that joint still while they play. Read from the glb's own keys (the first `joint_00` translation of each clip).
pub fn root_motion_clips(path: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let Ok(glb) = std::fs::read(path) else { return out };
    if glb.len() < 28 || &glb[0..4] != b"glTF" {
        return out;
    }
    let len = u32::from_le_bytes([glb[12], glb[13], glb[14], glb[15]]) as usize;
    let Some(json) = glb.get(20..20 + len).and_then(|j| serde_json::from_slice::<serde_json::Value>(j).ok()) else { return out };
    let bin_at = 20 + len + 8;
    let Some(joint) = json["nodes"].as_array().and_then(|n| n.iter().position(|n| n["name"] == "joint_00")) else { return out };
    let first_y = |acc: usize| -> Option<f32> {
        let a = &json["accessors"][acc];
        let bv = &json["bufferViews"][a["bufferView"].as_u64()? as usize];
        let off = bin_at + bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        glb.get(off + 4..off + 8).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    for anim in json["animations"].as_array().into_iter().flatten() {
        for ch in anim["channels"].as_array().into_iter().flatten() {
            if ch["target"]["node"].as_u64() != Some(joint as u64) || ch["target"]["path"] != "translation" {
                continue;
            }
            let Some(sampler) = ch["sampler"].as_u64() else { continue };
            let Some(acc) = anim["samplers"][sampler as usize]["output"].as_u64() else { continue };
            if first_y(acc as usize).is_some_and(|y| y.abs() < 20.0) {
                out.insert(anim["name"].as_str().unwrap_or("").to_string());
            }
        }
    }
    out
}

/// Whether the installed data has this fighter (its glb and the import's index).
pub fn installed(root: &Path, f: Fighter) -> bool {
    root.join(f.model()).is_file() && root.join(INDEX).is_file()
}

// ---------------------------------------------------------------------------------------------
// fighters.json
// ---------------------------------------------------------------------------------------------

/// The `FTAttributes` numbers of `fighters.json` this module reads (SSB64 units per frame).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Attributes {
    pub run_speed: f32,
    pub jump_height_base: f32,
    pub gravity: f32,
    pub weight: f32,
    pub jumps_max: u32,
}

#[allow(dead_code)]
impl Attributes {
    /// Running speed relative to Smash Mario's (`run_speed` 44 in SSB64 units).
    pub fn speed_ratio(&self) -> f32 {
        if self.run_speed > 0.0 { self.run_speed / 44.0 } else { 1.0 }
    }
    /// Jump height relative to Smash Mario's (height grows with base^2 / gravity; Mario 26 and 2.4... see `native/SSB64.md`).
    pub fn jump_ratio(&self) -> f32 {
        if self.gravity > 0.0 && self.jump_height_base > 0.0 { (self.jump_height_base * self.jump_height_base / self.gravity) / (26.0 * 26.0 / 2.4) } else { 1.0 }
    }
}

/// One fighter of `fighters.json`.
#[derive(Clone, Debug, Default)]
pub struct FighterData {
    #[allow(dead_code)]
    pub label: String,
    /// Decomp motion name -> glb clip name (`motions`, e.g. `LandingLight` -> `KneeBend`, Mario `Attack13` -> `Jab3`).
    pub motions: HashMap<String, String>,
    /// Clip name -> length in seconds.
    pub clips: HashMap<String, f32>,
    pub attributes: Attributes,
}

impl FighterData {
    /// The clip that plays `motion` (a decomp motion name), when the fighter has it.
    pub fn motion_clip(&self, motion: &str) -> Option<&str> {
        self.motions.get(motion).map(String::as_str).filter(|c| self.clips.contains_key(*c))
    }

    /// The clip for a move: the first of its motions the fighter has, else `Wait` (every fighter has it).
    pub fn resolve(&self, mv: Move) -> &str {
        mv.motions().iter().find_map(|m| self.motion_clip(m)).or_else(|| self.motion_clip("Wait")).unwrap_or("Wait")
    }

    /// Whether the move resolves to one of its own motions (not the `Wait` fallback).
    #[cfg(test)]
    pub fn has(&self, mv: Move) -> bool {
        mv.motions().iter().any(|m| self.motion_clip(m).is_some())
    }

    pub fn seconds(&self, clip: &str) -> f32 {
        self.clips.get(clip).copied().unwrap_or(1.0).max(1.0 / 60.0)
    }
}

/// Parse the import's index: fighter file stem -> its data.
pub fn parse_index(text: &str) -> Result<HashMap<String, FighterData>, String> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("fighters.json: {e}"))?;
    let fighters = json.get("fighters").and_then(|f| f.as_object()).ok_or("fighters.json: no fighters")?;
    let mut out = HashMap::new();
    for (stem, f) in fighters {
        let mut d = FighterData { label: f.get("label").and_then(|l| l.as_str()).unwrap_or(stem).to_string(), ..Default::default() };
        if let Some(clips) = f.get("clips").and_then(|c| c.as_object()) {
            for (name, c) in clips {
                let secs = c.get("seconds").and_then(|s| s.as_f64()).or_else(|| c.get("frames").and_then(|n| n.as_f64()).map(|n| n / 60.0)).unwrap_or(1.0);
                d.clips.insert(name.clone(), secs as f32);
            }
        }
        if let Some(motions) = f.get("motions").and_then(|m| m.as_object()) {
            // Ids in ascending order: the first (common) motion of a name wins over a later duplicate.
            let mut ids: Vec<(&String, &serde_json::Value)> = motions.iter().collect();
            ids.sort_by_key(|(k, _)| k.parse::<u32>().unwrap_or(u32::MAX));
            for (_, m) in ids {
                if let (Some(motion), Some(clip)) = (m.get("motion").and_then(|x| x.as_str()), m.get("clip").and_then(|x| x.as_str())) {
                    d.motions.entry(motion.to_string()).or_insert_with(|| clip.to_string());
                }
            }
        }
        if let Some(a) = f.get("attributes") {
            let n = |k: &str| a.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            d.attributes = Attributes { run_speed: n("run_speed"), jump_height_base: n("jump_height_base"), gravity: n("gravity"), weight: n("weight"), jumps_max: n("jumps_max") as u32 };
        }
        out.insert(stem.clone(), d);
    }
    Ok(out)
}

pub fn load_index(root: &Path) -> Result<HashMap<String, FighterData>, String> {
    let text = std::fs::read_to_string(root.join(INDEX)).map_err(|e| format!("{INDEX}: {e}"))?;
    parse_index(&text)
}

// ---------------------------------------------------------------------------------------------
// Mario action -> move
// ---------------------------------------------------------------------------------------------

/// What the fighter is doing, as a list of Smash motions to try in order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Move {
    Wait,
    WalkSlow,
    WalkMiddle,
    WalkFast,
    Run,
    Brake,
    Squat,
    SquatWait,
    SquatRv,
    Jump,
    DoubleJump,
    TripleJump,
    BackFlip,
    Fall,
    Land,
    Jab1,
    Jab2,
    Jab3,
    JumpKick,
    AirForward,
    DashAttack,
    AirDown,
    AirDownLand,
    SmashSide,
    SmashUp,
    Special,
    Hurt,
    Fly,
    Tumble,
    Shock,
    Bonk,
    DownFace,
    DownBack,
    GetUp,
    Taunt,
    LedgeWait,
    LedgeClimbSlow,
    LedgeClimbQuick,
    PickUp,
    Throw,
}

impl Move {
    /// Every move (the tests resolve each one for every fighter).
    #[cfg(test)]
    pub const ALL: [Move; 40] = [
        Move::Wait,
        Move::WalkSlow,
        Move::WalkMiddle,
        Move::WalkFast,
        Move::Run,
        Move::Brake,
        Move::Squat,
        Move::SquatWait,
        Move::SquatRv,
        Move::Jump,
        Move::DoubleJump,
        Move::TripleJump,
        Move::BackFlip,
        Move::Fall,
        Move::Land,
        Move::Jab1,
        Move::Jab2,
        Move::Jab3,
        Move::JumpKick,
        Move::AirForward,
        Move::DashAttack,
        Move::AirDown,
        Move::AirDownLand,
        Move::SmashSide,
        Move::SmashUp,
        Move::Special,
        Move::Hurt,
        Move::Fly,
        Move::Tumble,
        Move::Shock,
        Move::Bonk,
        Move::DownFace,
        Move::DownBack,
        Move::GetUp,
        Move::Taunt,
        Move::LedgeWait,
        Move::LedgeClimbSlow,
        Move::LedgeClimbQuick,
        Move::PickUp,
        Move::Throw,
    ];

    /// The decomp motion names (`src/ft/ftdef.h` and the fighters' own enums) that play this move, best first.
    pub fn motions(self) -> &'static [&'static str] {
        match self {
            Move::Wait => &["Wait"],
            Move::WalkSlow => &["WalkSlow", "WalkMiddle"],
            Move::WalkMiddle => &["WalkMiddle"],
            Move::WalkFast => &["WalkFast", "WalkMiddle"],
            Move::Run => &["Run", "Dash"],
            Move::Brake => &["RunBrake", "WalkEnd"],
            Move::Squat => &["Squat", "SquatWait"],
            Move::SquatWait => &["SquatWait", "Squat"],
            Move::SquatRv => &["SquatRv"],
            Move::Jump => &["JumpF"],
            // Kirby and Jigglypuff have no `JumpAerialF`: five midair jumps of their own.
            Move::DoubleJump => &["JumpAerialF", "JumpAerialF1"],
            Move::TripleJump => &["JumpAerialF", "JumpAerialF2"],
            Move::BackFlip => &["JumpAerialB", "JumpAerialF3", "JumpAerialF"],
            Move::Fall => &["Fall"],
            Move::Land => &["LandingLight", "LandingAirNull"],
            Move::Jab1 => &["Attack11"],
            // Pikachu's jab is a single headbutt.
            Move::Jab2 => &["Attack12", "Attack11"],
            Move::Jab3 => &["Attack13", "AttackS3"],
            Move::JumpKick => &["AttackAirN"],
            Move::AirForward => &["AttackAirF"],
            Move::DashAttack => &["AttackDash"],
            Move::AirDown => &["AttackAirLw"],
            Move::AirDownLand => &["LandingAirLw", "LandingLight"],
            Move::SmashSide => &["AttackS4"],
            Move::SmashUp => &["AttackHi4"],
            Move::Special => &["SpecialN", "SpecialNStart", "AttackS4"],
            Move::Hurt => &["DamageN1"],
            Move::Fly => &["DamageFlyN"],
            Move::Tumble => &["DamageFlyRoll", "DamageFlyN"],
            Move::Shock => &["DamageE", "DamageN2"],
            Move::Bonk => &["WallDamage", "DamageHi1"],
            Move::DownFace => &["DownBounceD"],
            Move::DownBack => &["DownBounceU", "DownBounceD"],
            Move::GetUp => &["DownStandD"],
            Move::Taunt => &["Appeal"],
            Move::LedgeWait => &["CliffWait", "CliffCatch"],
            Move::LedgeClimbSlow => &["CliffClimbSlow1", "CliffSlow"],
            Move::LedgeClimbQuick => &["CliffClimbQuick1", "CliffQuick"],
            Move::PickUp => &["LightGet"],
            Move::Throw => &["LightThrowF", "ThrowF"],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Play {
    Loop,
    /// Plays once and holds the last frame.
    Once,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SmashClip {
    pub mv: Move,
    pub play: Play,
    /// Playback rate (1 = the game's 60 frames a second).
    pub speed: f32,
    /// Restart the clip when this changes within the same action (Mario's punch sequence stages).
    pub stage: u8,
    /// Forward pitch of the model (radians): swimming strokes and flight lie the upright Smash poses down.
    pub tilt: f32,
}

const fn lp(mv: Move, speed: f32) -> SmashClip {
    SmashClip { mv, play: Play::Loop, speed, stage: 0, tilt: 0.0 }
}
const fn once(mv: Move, speed: f32) -> SmashClip {
    SmashClip { mv, play: Play::Once, speed, stage: 0, tilt: 0.0 }
}

/// Mario's punch sequence stage from `actionArg` (`mario_update_punch_sequence`: 0-2 first punch, 3-5 second punch, 6+ kick).
fn punch_stage(arg: f64) -> u8 {
    if arg < 3.0 {
        0
    } else if arg < 6.0 {
        1
    } else {
        2
    }
}

/// Forward speed (SM64 units a frame) from which each gait takes over.
pub const GAITS: [f64; 3] = [5.0, 12.0, 20.0];

/// The clip for one frame of Mario's state (the module table). `size` is the fighter's [`Fighter::height_ratio`]: a big
/// fighter's stride covers more ground, so its gait cycles play slower at the same speed.
pub fn clip_for(action: u32, action_arg: f64, forward_vel: f64, vel_y: f64, size: f32) -> SmashClip {
    use Move::*;
    let speed = forward_vel.abs();
    let stride = |rate: f64| (rate / (size as f64).max(0.4).sqrt()) as f32;
    match action {
        ACT_IDLE | ACT_BRAKING_STOP | ACT_BUTT_SLIDE_STOP | ACT_SLIDE_KICK_SLIDE_STOP | ACT_JUMP_LAND_STOP | ACT_DOUBLE_JUMP_LAND_STOP | ACT_TRIPLE_JUMP_LAND_STOP
        | ACT_FREEFALL_LAND_STOP | ACT_SIDE_FLIP_LAND_STOP | ACT_LONG_JUMP_LAND_STOP | ACT_BACKFLIP_LAND_STOP | ACT_PANTING | ACT_STANDING_AGAINST_WALL | ACT_COUGHING
        | ACT_SHIVERING | ACT_READING_SIGN | ACT_READING_NPC_DIALOG | ACT_READING_AUTOMATIC_DIALOG | ACT_WAITING_FOR_DIALOG | ACT_HOLD_IDLE | ACT_HOLD_JUMP_LAND_STOP
        | ACT_HOLD_FREEFALL_LAND_STOP | ACT_HOLD_BUTT_SLIDE_STOP | ACT_METAL_WATER_STANDING | ACT_PUTTING_ON_CAP | ACT_IN_QUICKSAND | ACT_TOP_OF_POLE | ACT_RIDING_SHELL_GROUND => {
            lp(Wait, 1.0)
        }
        ACT_WALKING | ACT_HOLD_WALKING | ACT_DECELERATING | ACT_HOLD_DECELERATING | ACT_METAL_WATER_WALKING => {
            if speed >= GAITS[2] {
                lp(Run, stride((speed / 26.0).clamp(0.7, 1.6)))
            } else if speed >= GAITS[1] {
                lp(WalkFast, stride((speed / 15.0).clamp(0.7, 1.5)))
            } else if speed >= GAITS[0] {
                lp(WalkMiddle, stride((speed / 8.0).clamp(0.7, 1.5)))
            } else {
                lp(WalkSlow, stride((speed / 3.0).clamp(0.5, 1.5)))
            }
        }
        ACT_TURNING_AROUND | ACT_FINISH_TURNING_AROUND | ACT_BRAKING => once(Brake, 1.0),
        ACT_BEGIN_SLIDING | ACT_HOLD_BEGIN_SLIDING | ACT_CROUCH_SLIDE | ACT_START_CROUCHING => once(Squat, 1.0),
        ACT_BUTT_SLIDE | ACT_HOLD_BUTT_SLIDE | ACT_BUTT_SLIDE_AIR | ACT_HOLD_BUTT_SLIDE_AIR | ACT_CROUCHING | ACT_CRAWLING | ACT_START_CRAWLING | ACT_STOP_CRAWLING | ACT_START_SLEEPING
        | ACT_SLEEPING => lp(SquatWait, 1.0),
        ACT_STOP_CROUCHING | ACT_WAKING_UP => once(SquatRv, 1.0),
        ACT_JUMP | ACT_STEEP_JUMP | ACT_WALL_KICK_AIR | ACT_HOLD_JUMP | ACT_RIDING_SHELL_JUMP | ACT_METAL_WATER_JUMP | ACT_WATER_JUMP | ACT_TOP_OF_POLE_JUMP => {
            if vel_y > 0.0 {
                once(Jump, 1.0)
            } else {
                lp(Fall, 1.0)
            }
        }
        ACT_LONG_JUMP => {
            if vel_y > 0.0 {
                SmashClip { tilt: 0.35, ..once(Jump, 0.8) }
            } else {
                SmashClip { tilt: 0.35, ..lp(Fall, 1.0) }
            }
        }
        ACT_DOUBLE_JUMP | ACT_SIDE_FLIP | ACT_FLYING_TRIPLE_JUMP => once(DoubleJump, 1.0),
        ACT_TRIPLE_JUMP => once(TripleJump, 1.0),
        ACT_BACKFLIP => once(BackFlip, 1.0),
        ACT_JUMP_LAND | ACT_FREEFALL_LAND | ACT_DOUBLE_JUMP_LAND | ACT_SIDE_FLIP_LAND | ACT_TRIPLE_JUMP_LAND | ACT_LONG_JUMP_LAND | ACT_BACKFLIP_LAND | ACT_HOLD_JUMP_LAND
        | ACT_HOLD_FREEFALL_LAND | ACT_AIR_THROW_LAND | ACT_LAVA_BOOST_LAND | ACT_DEATH_EXIT_LAND | ACT_SPAWN_NO_SPIN_LANDING | ACT_SPAWN_SPIN_LANDING => once(Land, 1.0),
        ACT_PUNCHING | ACT_MOVE_PUNCHING => {
            let stage = punch_stage(action_arg);
            SmashClip { stage, ..once([Jab1, Jab2, Jab3][stage as usize], 1.15) }
        }
        ACT_JUMP_KICK => once(JumpKick, 1.0),
        ACT_DIVE => once(AirForward, 1.0),
        ACT_SLIDE_KICK | ACT_SLIDE_KICK_SLIDE => once(DashAttack, 1.0),
        ACT_DIVE_SLIDE | ACT_STOMACH_SLIDE | ACT_HOLD_STOMACH_SLIDE => once(DownFace, 1.0),
        ACT_GROUND_POUND => once(AirDown, 1.2),
        ACT_GROUND_POUND_LAND => once(AirDownLand, 1.0),
        ACT_PICKING_UP | ACT_DIVE_PICKING_UP | ACT_PLACING_DOWN | ACT_PICKING_UP_BOWSER => once(PickUp, 1.0),
        ACT_THROWING | ACT_AIR_THROW | ACT_RELEASING_BOWSER => once(Throw, 1.0),
        ACT_HOLDING_BOWSER => lp(Wait, 1.0),
        ACT_SOFT_BACKWARD_GROUND_KB | ACT_SOFT_FORWARD_GROUND_KB | ACT_SQUISHED | ACT_GRABBED => once(Hurt, 1.0),
        ACT_HARD_BACKWARD_GROUND_KB | ACT_BACKWARD_GROUND_KB => once(DownBack, 1.0),
        ACT_HARD_FORWARD_GROUND_KB | ACT_FORWARD_GROUND_KB | ACT_HEAD_STUCK_IN_GROUND | ACT_BUTT_STUCK_IN_GROUND | ACT_FEET_STUCK_IN_GROUND => once(DownFace, 1.0),
        ACT_BACKWARD_AIR_KB | ACT_FORWARD_AIR_KB | ACT_GETTING_BLOWN => once(Fly, 1.0),
        ACT_HARD_FORWARD_AIR_KB | ACT_HARD_BACKWARD_AIR_KB | ACT_BURNING_JUMP | ACT_BURNING_FALL | ACT_LAVA_BOOST | ACT_SHOT_FROM_CANNON => lp(Tumble, 1.0),
        ACT_BURNING_GROUND => lp(Run, 1.4),
        ACT_SHOCKED | ACT_ELECTROCUTION => lp(Shock, 1.0),
        ACT_SOFT_BONK | ACT_AIR_HIT_WALL => once(Bonk, 1.0),
        ACT_STANDING_DEATH | ACT_DEATH_ON_STOMACH | ACT_DEATH_ON_BACK | ACT_QUICKSAND_DEATH | ACT_SUFFOCATION | ACT_EATEN_BY_BUBBA => once(DownBack, 1.0),
        ACT_DROWNING | ACT_WATER_DEATH => once(Fly, 0.6),
        ACT_STAR_DANCE_EXIT | ACT_STAR_DANCE_NO_EXIT | ACT_STAR_DANCE_WATER | ACT_JUMBO_STAR_CUTSCENE | ACT_FALL_AFTER_STAR_GRAB => lp(Taunt, 1.0),
        ACT_WATER_IDLE | ACT_SWIMMING_END | ACT_WATER_ACTION_END | ACT_WATER_PLUNGE | ACT_METAL_WATER_FALLING => lp(Fall, 0.6),
        ACT_BREASTSTROKE | ACT_FLUTTER_KICK => SmashClip { tilt: 1.1, ..lp(Fall, 0.8) },
        ACT_LEDGE_GRAB | ACT_START_HANGING | ACT_HANGING | ACT_HANG_MOVING | ACT_HOLDING_POLE | ACT_GRAB_POLE_SLOW | ACT_GRAB_POLE_FAST | ACT_CLIMBING_POLE
        | ACT_TOP_OF_POLE_TRANSITION | ACT_LEDGE_CLIMB_DOWN => lp(LedgeWait, 1.0),
        ACT_LEDGE_CLIMB_SLOW_1 | ACT_LEDGE_CLIMB_SLOW_2 => once(LedgeClimbSlow, 1.0),
        ACT_LEDGE_CLIMB_FAST => once(LedgeClimbQuick, 1.0),
        ACT_FLYING => SmashClip { tilt: 1.2, ..once(Jump, 1.0) },
        a if a & ACT_FLAG_AIR != 0 => lp(Fall, 1.0),
        _ => lp(Wait, 1.0),
    }
}

/// Gem Clash's hero states (`brawl_core::Act`) as Smash moves, so a fighter looks right in the arena (the move feel stays
/// Mario's hero for now). `air` / `rising` describe the fighter, `moving` its ground speed.
pub fn gem_clash_clip(act: crate::experiments::brawl_core::Act, air: bool, rising: bool, moving: bool, jumps: u8) -> SmashClip {
    use crate::experiments::brawl_core::Act as A;
    use Move::*;
    match act {
        A::Idle | A::Air => {
            if air {
                if rising {
                    if jumps >= 2 { once(DoubleJump, 1.0) } else { once(Jump, 1.0) }
                } else {
                    lp(Fall, 1.0)
                }
            } else if moving {
                lp(Run, 1.0)
            } else {
                lp(Wait, 1.0)
            }
        }
        A::Punch(k) => SmashClip { stage: k, ..once([Jab1, Jab2, Jab3][(k as usize).min(2)], 1.15) },
        A::Swing => once(SmashSide, 1.0),
        A::Spin => once(SmashUp, 1.0),
        A::Shoot | A::Beam => once(Special, 1.0),
        A::Dive => once(if air { AirForward } else { DashAttack }, 1.0),
        A::DiveSlide => once(DashAttack, 1.0),
        A::PoundWind | A::Pound => once(AirDown, 1.0),
        A::PoundLand => once(AirDownLand, 1.0),
        A::Toss => once(Throw, 1.0),
        A::Nova => once(SmashUp, 1.0),
        A::Hurt => once(Hurt, 1.0),
        A::Down => once(DownFace, 1.0),
        A::GetUp => once(GetUp, 1.0),
        A::Dead => lp(Tumble, 1.0),
    }
}

/// The sample time of a clip `len` seconds long, `t` seconds after it started.
pub fn sample_time(clip: &SmashClip, len: f32, t: f32) -> f32 {
    let t = t.max(0.0) * clip.speed;
    match clip.play {
        Play::Loop => t.rem_euclid(len.max(1.0 / 60.0)),
        Play::Once => t.min(len),
    }
}

/// The playing clock of the clip on screen.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ClipClock {
    key: Option<(Move, u32, u8)>,
    pub t: f32,
}

impl ClipClock {
    /// Advance by `dt`; a new move, or a one-shot move whose action or stage changed, starts over. Returns the seconds played.
    pub fn step(&mut self, clip: &SmashClip, action: u32, dt: f32) -> f32 {
        let key = (clip.mv, if clip.play == Play::Once { action } else { 0 }, clip.stage);
        if self.key != Some(key) {
            self.key = Some(key);
            self.t = 0.0;
        } else {
            self.t += dt;
        }
        self.t
    }
    #[cfg(test)]
    pub fn started(&self) -> bool {
        self.t == 0.0
    }
}

/// `JUMPMAN_SMASH_CLIP=<motion or clip name>`: hold one clip (looped) whatever Mario does, to look at the clips headless.
fn forced_clip() -> Option<String> {
    std::env::var("JUMPMAN_SMASH_CLIP").ok().filter(|v| !v.is_empty())
}

/// The glb clip a forced name means: a motion name of the fighter's table, or a clip name.
pub fn forced_clip_name<'a>(data: &'a FighterData, name: &'a str) -> &'a str {
    data.motion_clip(name).unwrap_or(name)
}

// ---------------------------------------------------------------------------------------------
// Bevy glue
// ---------------------------------------------------------------------------------------------

pub struct SmashPlugin;

impl Plugin for SmashPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SmashView>()
            .add_systems(PreUpdate, smash_input.after(crate::banjo_ctl::banjo_input))
            .add_systems(Update, (spawn_fighter, drive_fighter).chain().after(super::update_rigs).before(crate::CameraSet))
            .add_systems(PostUpdate, hold_root_motion.after(bevy::app::AnimationSystems).before(bevy::transform::TransformSystems::Propagate));
    }
}

#[derive(Component)]
#[allow(dead_code)]
pub struct SmashRoot(pub Fighter);
#[derive(Component)]
pub struct SmashBody;

/// The import's index, loaded once (shared with the character select).
#[derive(Default)]
pub struct IndexCache {
    index: Option<Arc<HashMap<String, FighterData>>>,
    failed: bool,
}

impl IndexCache {
    pub fn get(&mut self, root: &Path) -> Option<Arc<HashMap<String, FighterData>>> {
        if self.index.is_none() && !self.failed {
            match load_index(root) {
                Ok(i) => self.index = Some(Arc::new(i)),
                Err(e) => {
                    warn!("Smash fighters: {e}");
                    self.failed = true;
                }
            }
        }
        self.index.clone()
    }
}

#[derive(Resource, Default)]
pub struct SmashView {
    /// The fighter on screen: its root and body entities and its data.
    on: Option<(Fighter, Entity, Entity, Arc<FighterData>)>,
    failed: Vec<Fighter>,
    clock: ClipClock,
    pub cache: IndexCache,
    /// The on-screen fighter's root-motion clips ([`root_motion_clips`]), its `joint_00` and the clip playing now.
    root_motion: std::collections::HashSet<String>,
    joint00: Option<Entity>,
    playing: String,
}

/// Spawn a fighter's glb (the object views' clip driver, seek mode) under `parent`-less body; mesh culling off (the clips
/// move the skin far from its bind-pose bounds). Returns the body entity.
pub fn spawn_body(commands: &mut Commands, assets: &AssetServer, root: &AssetRoot, f: Fighter, layer: Option<bevy::camera::visibility::RenderLayers>) -> Entity {
    let model = f.model();
    let mut body = spawn_model(commands, assets, root, &model, false);
    if let Some(l) = layer.clone() {
        body.insert(l);
    }
    body.observe(move |trigger: On<WorldInstanceReady>, mut commands: Commands, children: Query<&Children>, meshes: Query<(), With<Mesh3d>>| {
        for child in children.iter_descendants(trigger.event_target()) {
            if let Some(l) = &layer {
                commands.entity(child).insert(l.clone());
            }
            if meshes.contains(child) {
                commands.entity(child).insert(NoFrustumCulling);
            }
        }
    })
    .id()
}

#[allow(clippy::too_many_arguments)]
fn spawn_fighter(mut commands: Commands, character: Res<Character>, root: Option<Res<AssetRoot>>, assets: Option<Res<AssetServer>>, mut view: ResMut<SmashView>, mut toast: ResMut<Toast>) {
    let want = match *character {
        Character::Smash(f) => Some(f),
        _ => None,
    };
    // Another fighter (or none): take the old one down.
    if let Some((f, root_e, _, _)) = view.on.as_ref() {
        if want != Some(*f) {
            commands.entity(*root_e).despawn();
            view.on = None;
        }
    }
    let Some(f) = want else { return };
    if view.on.is_some() || view.failed.contains(&f) {
        return;
    }
    let (Some(root), Some(assets)) = (root, assets) else { return };
    let data = if installed(&root.0, f) { view.cache.get(&root.0).and_then(|i| i.get(f.stem()).cloned()) } else { None };
    let Some(data) = data else {
        warn!("Smash fighter {f:?} is missing: {}", root.0.join(f.model()).display());
        toast.show(format!("{} IS NOT INSTALLED - IMPORT SUPER SMASH BROS.", f.label()), 3.2);
        view.failed.push(f);
        return;
    };
    let root_e = commands.spawn((Transform::default(), Visibility::Hidden, super::CharacterRoot(Character::Smash(f)), SmashRoot(f))).id();
    let body = spawn_body(&mut commands, &assets, &root, f, None);
    commands.entity(body).insert(SmashBody);
    commands.entity(root_e).add_child(body);
    view.on = Some((f, root_e, body, Arc::new(data)));
    view.clock = ClipClock::default();
    view.root_motion = root_motion_clips(&root.0.join(f.model()));
    view.joint00 = None;
}

/// What a Gem Clash match tells the skin about the hero (set by `brawl.rs` while a match runs).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HeroAct {
    pub act: crate::experiments::brawl_core::Act,
    pub air: bool,
    pub rising: bool,
    pub moving: bool,
    pub jumps: u8,
}

#[allow(clippy::too_many_arguments)]
fn drive_fighter(
    character: Res<Character>,
    sim: Res<GameSim>,
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut view: ResMut<SmashView>,
    mut roots: Query<(&mut Transform, &mut Visibility), With<SmashRoot>>,
    mut rigs: Query<&mut AnimRig, With<SmashBody>>,
    brawl: Option<Res<crate::experiments::brawl_view::BrawlBridge>>,
) {
    let Some((f, root_e, body, data)) = view.on.clone() else { return };
    let Ok((mut tf, mut vis)) = roots.get_mut(root_e) else { return };
    let show = *character == Character::Smash(f);
    vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
    if !show {
        return;
    }
    let m = &sim.mario;
    let (pos, yaw) = sim.mario_at(fixed.overstep_fraction() as f64);
    let mut render_yaw = yaw;
    if matches!(m.action, ACT_SPECIAL_EXIT_AIRBORNE | ACT_EXIT_AIRBORNE | ACT_FALLING_EXIT_AIRBORNE | ACT_EXIT_LAND_SAVE_DIALOG) {
        render_yaw = to_s16(render_yaw + 32768.0) as f64;
    }
    let dt = time.delta_secs().min(0.1);
    let hint = brawl.as_ref().filter(|b| b.active).and_then(|b| b.hero_act);
    let clip = match hint {
        Some(h) => gem_clash_clip(h.act, h.air, h.rising, h.moving, h.jumps),
        None => clip_for(m.action, m.action_arg, m.forward_vel, m.vel[1], f.height_ratio()),
    };
    tf.translation = to_bevy(pos);
    tf.rotation = Quat::from_rotation_y(jumpman_sim::math::angle_to_radians(render_yaw) as f32) * Quat::from_rotation_x(clip.tilt);
    // Hidden like Mario's model: cutscenes that clear GRAPH_RENDER_ACTIVE, and the blink after a hit.
    let hidden = !m.cut.mario_visible || crate::render::mario_view::blinks_out(m.invinc_timer, sim.ticks as u64);
    let size = if hint.is_some() { f.gem_clash_scale() } else { f.scale() };
    tf.scale = if hidden {
        Vec3::splat(0.0001)
    } else if m.squish_timer > 0.0 {
        Vec3::new(1.2, 0.65, 1.2) * size
    } else {
        Vec3::splat(size)
    };
    let forced = forced_clip();
    // A special (`jumpman_sim::smash`) plays its own motions: the first one the fighter's table has.
    let special = if hint.is_none() && forced.is_none() { m.smash.motion() } else { None };
    let (name, sample) = if let Some((names, secs, looped)) = special {
        view.clock = ClipClock::default();
        let name = names.iter().find_map(|n| data.motion_clip(n)).unwrap_or_else(|| data.resolve(if m.action & ACT_FLAG_AIR != 0 { Move::Fall } else { Move::Wait }));
        let len = data.seconds(name);
        (name, if looped { secs.rem_euclid(len) } else { secs.min(len) })
    } else {
        let played = view.clock.step(&clip, m.action, dt);
        let name = match forced.as_deref() {
            Some(n) => forced_clip_name(&data, n),
            None => data.resolve(clip.mv),
        };
        let len = data.seconds(name);
        (name, if forced.is_some() { (time.elapsed_secs()).rem_euclid(len) } else { sample_time(&clip, len, played) })
    };
    if view.playing != name {
        view.playing = name.to_string();
    }
    if let Ok(mut rig) = rigs.get_mut(body) {
        if let Some(i) = rig.names.iter().position(|n| n == name) {
            rig.want = i;
            rig.seek = Some(sample);
        }
    }
}

/// Hold `joint_00` at the origin while a root-motion clip plays: the sim moves the fighter, the clip must not move it
/// again (it would fly twice as high and snap back).
fn hold_root_motion(mut view: ResMut<SmashView>, children: Query<&Children>, names: Query<&Name>, mut tfs: Query<&mut Transform>) {
    let Some((_, _, body, _)) = view.on.clone() else { return };
    if view.joint00.is_none_or(|j| tfs.get(j).is_err()) {
        view.joint00 = children.iter_descendants(body).find(|e| names.get(*e).is_ok_and(|n| n.as_str() == "joint_00"));
    }
    let Some(j) = view.joint00 else { return };
    if view.root_motion.contains(&view.playing) {
        if let Ok(mut tf) = tfs.get_mut(j) {
            tf.translation = Vec3::ZERO;
        }
    }
}

/// The special button for the Smash fighters: N64 R (`Action::RButton`: H on the keyboard, R2 on a pad) into `R_TRIG`.
/// Runs after Banjo's C-button reader, which owns the extra bits while Banjo plays. Touch has no special button.
fn smash_input(
    current: Res<Character>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    bindings: Option<Res<crate::bindings::Bindings>>,
    mut input: ResMut<crate::input::PlayerInput>,
) {
    if !matches!(*current, Character::Smash(_)) {
        return;
    }
    use crate::bindings::Action;
    let r_keys = bindings.as_ref().map(|b| b.keys(Action::RButton)).unwrap_or(&[]);
    let pad = pads.iter().next();
    let pad_held = pad.is_some_and(|p| bindings.as_ref().map_or_else(|| p.pressed(GamepadButton::RightTrigger2), |b| b.pad_pressed(p, Action::RButton)));
    let pad_tap = pad.is_some_and(|p| bindings.as_ref().map_or_else(|| p.just_pressed(GamepadButton::RightTrigger2), |b| b.pad_just_pressed(p, Action::RButton)));
    let held = r_keys.iter().any(|k| keys.pressed(*k)) || pad_held;
    let tapped = r_keys.iter().any(|k| keys.just_pressed(*k)) || pad_tap;
    if held {
        input.extra_down |= R_TRIG;
    } else {
        input.extra_down &= !R_TRIG;
    }
    if tapped {
        input.latch_extra(R_TRIG);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The asset root: `JUMPMAN_SMASH_TEST_ASSETS` (a data dir's `assets/` with a Smash import next to it) or the repo's.
    fn assets() -> std::path::PathBuf {
        std::env::var_os("JUMPMAN_SMASH_TEST_ASSETS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets"))
    }

    /// Every special of every fighter, on the ground and in the air, run through its phases on the sim: each motion list
    /// it names resolves to a clip of the fighter's import, and the root-motion clips are the TRANSN ones. Skipped without
    /// a Smash import.
    #[test]
    fn special_motions_resolve_in_the_import() {
        use jumpman_sim::collision::{CollisionWorld, SurfaceOptions};
        use jumpman_sim::mario_runtime::{MarioRuntime, SpawnPoint};
        use jumpman_sim::smash::SpecialSlot;
        let root = assets();
        if !root.join(INDEX).is_file() {
            eprintln!("skipped: no {}", root.join(INDEX).display());
            return;
        }
        let index = load_index(&root).unwrap();
        let mut w = CollisionWorld::default();
        let o = SurfaceOptions::default();
        w.add_triangle([-8000.0, 0.0, -8000.0], [-8000.0, 0.0, 8000.0], [8000.0, 0.0, 8000.0], &o);
        w.add_triangle([-8000.0, 0.0, -8000.0], [8000.0, 0.0, 8000.0], [8000.0, 0.0, -8000.0], &o);
        let world = std::sync::Arc::new(w);
        let step = |m: &mut MarioRuntime, sy: f64, down: u32, pressed: u32| {
            let c = ControllerSnapshot { focus_yaw: None, stick_x: 0.0, stick_y: sy, stick_mag: sy.abs(), button_down: down | pressed, button_pressed: pressed };
            m.step(&c, 0x8000 as f64, false, false);
        };
        let mut checked = 0;
        for f in Fighter::ALL {
            let data = &index[f.stem()];
            for (slot, sy) in [(SpecialSlot::Neutral, 0.0), (SpecialSlot::Up, 64.0), (SpecialSlot::Down, -64.0)] {
                for air in [false, true] {
                    let mut m = MarioRuntime::new(world.clone(), SpawnPoint { action: None, pos: [0.0; 3], face_yaw: 0.0 }, -1600.0);
                    m.smash.fighter = Some(f.sim());
                    for _ in 0..3 {
                        step(&mut m, 0.0, 0, 0);
                    }
                    if air {
                        step(&mut m, 0.0, A_BUTTON, A_BUTTON);
                        for _ in 0..7 {
                            step(&mut m, 0.0, A_BUTTON, 0);
                        }
                    }
                    step(&mut m, sy, 0, R_TRIG);
                    for k in 0..400 {
                        let Some((names, _, _)) = m.smash.motion() else { break };
                        assert!(names.iter().any(|n| data.motion_clip(n).is_some()), "{f:?} {slot:?} air={air}: none of {names:?} ({:?})", m.smash.special.map(|s| s.phase));
                        checked += 1;
                        step(&mut m, 0.0, 0, if k == 20 || k == 200 { R_TRIG } else { 0 });
                    }
                }
            }
            // Kirby's copies: every copied neutral special has its Copy<Name> motions.
            if f == Fighter::Kirby {
                for c in Fighter::ALL.iter().filter(|c| **c != Fighter::Kirby) {
                    let name = format!("Copy{}SpecialN", c.sim().decomp_name());
                    let start = format!("{name}Start");
                    assert!(data.motion_clip(&name).is_some() || data.motion_clip(&start).is_some(), "Kirby lacks {name}");
                }
            }
            let rm = root_motion_clips(&root.join(f.model()));
            assert!(!rm.contains(data.resolve(Move::Wait)), "{f:?}: Wait is not a root-motion clip");
        }
        assert!(checked > 1000, "{checked}");
        let mario = root_motion_clips(&root.join(Fighter::Mario.model()));
        assert!(mario.contains(index["mario"].motion_clip("SpecialHi").unwrap()), "Super Jump Punch carries root motion");
        let falcon = root_motion_clips(&root.join(Fighter::Captain.model()));
        assert!(falcon.contains(index["captain"].motion_clip("SpecialN").unwrap()) && falcon.contains(index["captain"].motion_clip("SpecialLw").unwrap()));
    }

    /// A tiny index in the import's format: two fighters, one with Kirby's multi-jumps and no `Attack12`.
    const SAMPLE: &str = r#"{"fps": 60, "game": "ssb64", "fighters": {
        "mario": {"label": "Mario", "attributes": {"run_speed": 44.0, "jump_height_base": 26.0, "gravity": 2.4, "weight": 1.0, "jumps_max": 2},
            "clips": {"Wait": {"frames": 61, "seconds": 1.0167}, "KneeBend": {"frames": 4, "seconds": 0.0667}, "JumpAerialF": {"frames": 40, "seconds": 0.6667},
                      "Attack11": {"frames": 16, "seconds": 0.2667}, "Attack12": {"frames": 16}, "Jab3": {"frames": 30, "seconds": 0.5}},
            "motions": {"4": {"clip": "Wait", "motion": "Wait"}, "14": {"clip": "KneeBend", "motion": "KneeBend"}, "18": {"clip": "JumpAerialF", "motion": "JumpAerialF"},
                        "25": {"clip": "KneeBend", "motion": "LandingLight"}, "165": {"clip": "Attack11", "motion": "Attack11"},
                        "166": {"clip": "Attack12", "motion": "Attack12"}, "195": {"clip": "Jab3", "motion": "Attack13"}}},
        "kirby": {"label": "Kirby", "attributes": {"run_speed": 39.0, "jumps_max": 6},
            "clips": {"Wait": {"seconds": 1.0}, "Jump2": {"seconds": 0.5}, "Attack11": {"seconds": 0.2}},
            "motions": {"4": {"clip": "Wait", "motion": "Wait"}, "165": {"clip": "Attack11", "motion": "Attack11"}, "198": {"clip": "Jump2", "motion": "JumpAerialF1"},
                        "300": {"clip": "Missing", "motion": "Appeal"}}}}}"#;

    #[test]
    fn index_parses_and_resolves_motions_with_fallbacks() {
        let i = parse_index(SAMPLE).unwrap();
        let (mario, kirby) = (&i["mario"], &i["kirby"]);
        assert_eq!(mario.label, "Mario");
        assert_eq!(mario.resolve(Move::Land), "KneeBend", "landing is the KneeBend clip");
        assert_eq!(mario.resolve(Move::Jab3), "Jab3", "Mario's own Attack13");
        assert_eq!(mario.resolve(Move::DoubleJump), "JumpAerialF");
        assert!((mario.seconds("Attack12") - 16.0 / 60.0).abs() < 1e-4, "frames when seconds is missing");
        assert_eq!(kirby.resolve(Move::DoubleJump), "Jump2", "Kirby's first midair jump");
        assert_eq!(kirby.resolve(Move::Jab2), "Attack11", "no second jab: the first again");
        assert_eq!(kirby.resolve(Move::Taunt), "Wait", "a motion whose clip is not in the glb falls back to Wait");
        assert!(!kirby.has(Move::Taunt) && kirby.has(Move::Jab1));
        assert_eq!(mario.attributes.jumps_max, 2);
        assert!((mario.attributes.speed_ratio() - 1.0).abs() < 1e-6 && (mario.attributes.jump_ratio() - 1.0).abs() < 1e-6);
        assert!(kirby.attributes.speed_ratio() < 1.0);
        assert!(parse_index("{}").is_err() && parse_index("nope").is_err());
    }

    /// One representative action of every Mario action group, with the move it must play.
    const GROUPS: [(u32, Move); 33] = [
        (ACT_IDLE, Move::Wait),
        (ACT_JUMP_LAND_STOP, Move::Wait),
        (ACT_TURNING_AROUND, Move::Brake),
        (ACT_START_CROUCHING, Move::Squat),
        (ACT_CROUCHING, Move::SquatWait),
        (ACT_STOP_CROUCHING, Move::SquatRv),
        (ACT_DOUBLE_JUMP, Move::DoubleJump),
        (ACT_TRIPLE_JUMP, Move::TripleJump),
        (ACT_BACKFLIP, Move::BackFlip),
        (ACT_FREEFALL, Move::Fall),
        (ACT_GROUND_POUND, Move::AirDown),
        (ACT_GROUND_POUND_LAND, Move::AirDownLand),
        (ACT_JUMP_LAND, Move::Land),
        (ACT_JUMP_KICK, Move::JumpKick),
        (ACT_DIVE, Move::AirForward),
        (ACT_SLIDE_KICK, Move::DashAttack),
        (ACT_DIVE_SLIDE, Move::DownFace),
        (ACT_SOFT_BACKWARD_GROUND_KB, Move::Hurt),
        (ACT_HARD_BACKWARD_GROUND_KB, Move::DownBack),
        (ACT_BACKWARD_AIR_KB, Move::Fly),
        (ACT_HARD_BACKWARD_AIR_KB, Move::Tumble),
        (ACT_SHOCKED, Move::Shock),
        (ACT_SOFT_BONK, Move::Bonk),
        (ACT_DEATH_ON_BACK, Move::DownBack),
        (ACT_STAR_DANCE_EXIT, Move::Taunt),
        (ACT_BREASTSTROKE, Move::Fall),
        (ACT_LEDGE_GRAB, Move::LedgeWait),
        (ACT_LEDGE_CLIMB_FAST, Move::LedgeClimbQuick),
        (ACT_LEDGE_CLIMB_SLOW_1, Move::LedgeClimbSlow),
        (ACT_PICKING_UP, Move::PickUp),
        (ACT_THROWING, Move::Throw),
        (ACT_FLYING, Move::Jump),
        (ACT_SLEEPING, Move::SquatWait),
    ];

    #[test]
    fn every_action_group_maps_to_its_move() {
        for (action, want) in GROUPS {
            assert_eq!(clip_for(action, 0.0, 0.0, 0.0, 1.0).mv, want, "action {action:#x}");
        }
        // Gaits by speed, faster with speed, slower for a big fighter.
        let walk = |v| clip_for(ACT_WALKING, 0.0, v, 0.0, 1.0);
        assert_eq!([walk(2.0).mv, walk(8.0).mv, walk(15.0).mv, walk(30.0).mv], [Move::WalkSlow, Move::WalkMiddle, Move::WalkFast, Move::Run]);
        assert!(walk(32.0).speed > walk(22.0).speed);
        assert!(clip_for(ACT_WALKING, 0.0, 30.0, 0.0, 2.0).speed < walk(30.0).speed, "a big stride cycles slower");
        // A jump rises with JumpF and falls with Fall; swimming strokes and flight are tilted.
        assert_eq!(clip_for(ACT_JUMP, 0.0, 0.0, 20.0, 1.0).mv, Move::Jump);
        assert_eq!(clip_for(ACT_JUMP, 0.0, 0.0, -5.0, 1.0).mv, Move::Fall);
        assert!(clip_for(ACT_BREASTSTROKE, 0.0, 0.0, 0.0, 1.0).tilt > 0.5 && clip_for(ACT_FLYING, 0.0, 0.0, 0.0, 1.0).tilt > 0.5);
        assert_eq!(clip_for(ACT_IDLE, 0.0, 0.0, 0.0, 1.0).tilt, 0.0);
        // Every action, mapped or not, lands on a move; airborne ones never stand in Wait.
        assert_eq!(clip_for(ACT_WALL_KICK_AIR, 0.0, 0.0, -1.0, 1.0).mv, Move::Fall);
        assert_eq!(clip_for(0x0dea_d000 | ACT_FLAG_AIR, 0.0, 0.0, 0.0, 1.0).mv, Move::Fall);
        assert_eq!(clip_for(0x0dea_d000 & !ACT_FLAG_AIR, 0.0, 0.0, 0.0, 1.0).mv, Move::Wait);
        for a in 0..0x400u32 {
            for action in [a, a | ACT_FLAG_AIR, a | ACT_FLAG_MOVING] {
                assert!(Move::ALL.contains(&clip_for(action, 0.0, 10.0, 0.0, 1.0).mv));
            }
        }
    }

    #[test]
    fn punches_restart_per_stage_and_loops_wrap() {
        let p = |arg| clip_for(ACT_PUNCHING, arg, 0.0, 0.0, 1.0);
        assert_eq!((p(0.0).mv, p(4.0).mv, p(7.0).mv), (Move::Jab1, Move::Jab2, Move::Jab3));
        assert_eq!((p(0.0).stage, p(2.0).stage, p(4.0).stage, p(7.0).stage), (0, 0, 1, 2));
        let mut clock = ClipClock::default();
        clock.step(&p(1.0), ACT_PUNCHING, 0.0);
        clock.step(&p(1.0), ACT_PUNCHING, 0.2);
        assert!(clock.t > 0.1);
        clock.step(&p(4.0), ACT_PUNCHING, 0.016);
        assert!(clock.started(), "the second punch starts its own clip");
        let idle = clip_for(ACT_IDLE, 0.0, 0.0, 0.0, 1.0);
        assert!(sample_time(&idle, 1.0, 1.25) < 0.3, "a loop wraps");
        assert_eq!(sample_time(&clip_for(ACT_JUMP_LAND, 0.0, 0.0, 0.0, 1.0), 0.07, 5.0), 0.07, "a one-shot holds its last frame");
        // A walk keeps its clock across speed changes (no restart every frame).
        let mut clock = ClipClock::default();
        clock.step(&clip_for(ACT_WALKING, 0.0, 6.0, 0.0, 1.0), ACT_WALKING, 0.0);
        clock.step(&clip_for(ACT_WALKING, 0.0, 7.0, 0.0, 1.0), ACT_WALKING, 0.1);
        assert!(clock.t > 0.05);
    }

    #[test]
    fn gem_clash_states_map_to_smash_moves() {
        use crate::experiments::brawl_core::Act as A;
        let g = |a| gem_clash_clip(a, false, false, false, 0).mv;
        assert_eq!([g(A::Punch(0)), g(A::Punch(1)), g(A::Punch(2))], [Move::Jab1, Move::Jab2, Move::Jab3]);
        assert_eq!([g(A::Swing), g(A::Spin), g(A::Shoot), g(A::Beam), g(A::Pound)], [Move::SmashSide, Move::SmashUp, Move::Special, Move::Special, Move::AirDown]);
        assert_eq!([g(A::Hurt), g(A::Down), g(A::GetUp)], [Move::Hurt, Move::DownFace, Move::GetUp]);
        assert_eq!(gem_clash_clip(A::Idle, false, false, true, 0).mv, Move::Run);
        assert_eq!(gem_clash_clip(A::Air, true, true, false, 2).mv, Move::DoubleJump);
        assert_eq!(gem_clash_clip(A::Air, true, false, false, 1).mv, Move::Fall);
        assert_eq!(gem_clash_clip(A::Dive, true, false, false, 0).mv, Move::AirForward);
        assert_ne!(gem_clash_clip(A::Punch(0), false, false, false, 0).stage, gem_clash_clip(A::Punch(1), false, false, false, 0).stage);
    }

    #[test]
    fn fighter_ids_labels_and_sizes() {
        for f in Fighter::ALL {
            assert_eq!(Fighter::from_id(f.id()), Some(f));
            assert_eq!(Fighter::from_id(&format!("ssb-{}", f.stem())), Some(f));
            assert!(f.short().len() <= 6, "{f:?}");
            assert_eq!(f.label(), crate::shell::retro::fb::clean(f.label()));
            assert!(f.model().ends_with(&format!("ssb64/{}.glb", f.stem())));
            assert_eq!(Fighter::ALL[f.index()], f);
        }
        assert_eq!(Fighter::from_id("ssb-pikachu"), Some(Fighter::Pikachu));
        assert_eq!(Fighter::from_id("ssb-jigglypuff"), Some(Fighter::Purin));
        assert_eq!(Fighter::from_id("ssb-boss"), None, "Master Hand is not playable");
        assert_eq!(Fighter::from_id("pikachu"), None);
        // Smash Mario stands as tall as SM64 Mario; the small ones are smaller, Donkey Kong bigger (and the camera backs off).
        assert!((Fighter::Mario.height_ratio() - 1.0).abs() < 1e-3);
        for small in [Fighter::Kirby, Fighter::Purin, Fighter::Pikachu] {
            assert!(small.height_ratio() < 0.8, "{small:?}");
            assert_eq!(small.camera_scale(), 1.0);
        }
        assert!(Fighter::Donkey.height_ratio() > 1.5 && Fighter::Donkey.camera_scale() > 1.15);
        assert!(Fighter::ALL.iter().all(|f| f.camera_scale() <= 1.4));
        // Gem Clash keeps everyone near Mario's size.
        for f in Fighter::ALL {
            let r = f.gem_clash_scale() / f.scale() * f.height_ratio();
            assert!((0.8 - 1e-4..=1.25 + 1e-4).contains(&r), "{f:?} {r}");
        }
        assert!((Fighter::Mario.gem_clash_scale() - Fighter::Mario.scale()).abs() < 1e-6);
        assert_ne!(Fighter::Mario.label(), "MARIO");
        assert_ne!(Fighter::Link.label(), "LINK");
    }

    /// The clip map against the real import: every move resolves, for every fighter, to a clip of its glb; the core moves to
    /// one of their own motions (not the Wait fallback); the bind heights match the glbs. Skipped without a Smash import.
    #[test]
    fn clip_map_matches_the_imported_fighters() {
        let root = assets();
        if !root.join(INDEX).is_file() {
            eprintln!("skipped: no {}", root.join(INDEX).display());
            return;
        }
        let index = load_index(&root).unwrap();
        // The moves a fighter may lack (each falls back as documented in `Move::motions`, ending at Wait).
        let optional = [Move::Special, Move::Shock, Move::Throw, Move::SquatRv, Move::Brake];
        for f in Fighter::ALL {
            assert!(installed(&root, f), "{f:?}");
            let data = &index[f.stem()];
            let path = root.join(f.model());
            let names = crate::render::g1_view::glb_animation_names(&path);
            for mv in Move::ALL {
                let clip = data.resolve(mv);
                assert!(names.iter().any(|n| n == clip), "{f:?} {mv:?} -> {clip} is not a clip of {}", path.display());
                if !optional.contains(&mv) {
                    assert!(data.has(mv), "{f:?} has none of {:?}", mv.motions());
                }
            }
            // Bind-pose height from the glb's position bounds.
            let glb = std::fs::read(&path).unwrap();
            let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
            let json: serde_json::Value = serde_json::from_slice(&glb[20..20 + len]).unwrap();
            let top = json["meshes"][0]["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| json["accessors"][p["attributes"]["POSITION"].as_u64().unwrap() as usize]["max"][1].as_f64().unwrap() as f32)
                .fold(0.0, f32::max);
            assert!((top - f.bind_height()).abs() < 0.03, "{f:?}: glb is {top} tall, table says {}", f.bind_height());
        }
        // Kirby and Jigglypuff take their own midair jumps.
        assert_eq!(index["kirby"].resolve(Move::DoubleJump), index["kirby"].motion_clip("JumpAerialF1").unwrap());
        assert_eq!(index["purin"].resolve(Move::TripleJump), index["purin"].motion_clip("JumpAerialF2").unwrap());
        assert!(!index.contains_key("boss") || Fighter::ALL.iter().all(|f| f.stem() != "boss"));
    }

    #[test]
    fn missing_import_is_not_installed() {
        let empty = std::env::temp_dir().join("jumpman-no-smash");
        for f in Fighter::ALL {
            assert!(!installed(&empty, f));
        }
        assert!(IndexCache::default().get(&empty).is_none());
    }

    /// Selecting a fighter without the import: a toast, no entity, and it is not retried every frame.
    #[test]
    fn missing_model_spawns_nothing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .insert_resource(Character::Smash(Fighter::Pikachu))
            .insert_resource(AssetRoot::new(std::env::temp_dir().join("jumpman-no-smash")))
            .init_resource::<Toast>()
            .init_resource::<SmashView>()
            .add_systems(Update, spawn_fighter);
        app.update();
        app.update();
        let view = app.world().resource::<SmashView>();
        assert!(view.failed == vec![Fighter::Pikachu] && view.on.is_none());
        assert_eq!(app.world_mut().query::<&SmashRoot>().iter(app.world()).count(), 0);
    }

    /// With the import: one root per selected fighter, swapped when the selection changes. Skipped without data.
    #[test]
    fn spawns_and_swaps_imported_fighters() {
        if !installed(&assets(), Fighter::Pikachu) || !installed(&assets(), Fighter::Donkey) {
            eprintln!("skipped: no Smash import");
            return;
        }
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<AnimationClip>()
            .init_asset::<bevy::gltf::Gltf>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .insert_resource(Character::Smash(Fighter::Pikachu))
            .insert_resource(AssetRoot::new(assets()))
            .init_resource::<Toast>()
            .init_resource::<SmashView>()
            .add_systems(Update, spawn_fighter);
        app.update();
        let roots: Vec<Fighter> = app.world_mut().query::<&SmashRoot>().iter(app.world()).map(|r| r.0).collect();
        assert_eq!(roots, vec![Fighter::Pikachu]);
        let clips = app.world_mut().query::<&AnimRig>().single(app.world()).map(|r| r.names.len()).unwrap();
        assert!(clips > 100, "{clips} clips");
        *app.world_mut().resource_mut::<Character>() = Character::Smash(Fighter::Donkey);
        app.update();
        app.update();
        let roots: Vec<Fighter> = app.world_mut().query::<&SmashRoot>().iter(app.world()).map(|r| r.0).collect();
        assert_eq!(roots, vec![Fighter::Donkey]);
        *app.world_mut().resource_mut::<Character>() = Character::Mario;
        app.update();
        app.update();
        assert_eq!(app.world_mut().query::<&SmashRoot>().iter(app.world()).count(), 0);
    }
}
