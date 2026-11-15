//! SSB64 motion event scripts on the sim: the hitboxes (and sound / hurtbox events) of every fighter motion, from the
//! import's `public/models/ssb64/motion-events.json` (`scripts/export-ssb64-motions.py` = `byor-rs` `ssb64_motions`, read
//! from the user's own ROM, never bundled; see `native/SSB64.md` "Motion event scripts"). Engine-free.
//!
//! The export already ran the game's script interpreter (`ftMainParseMotionEvent`) and replayed the attack collisions,
//! so a motion here is a list of [`HitWindow`]s: one SSB64 hitbox (`fp->attack_colls[id]`) with constant values over
//! frames `[from, to)` of the motion (60 Hz, frame 0 = the frame the motion starts), its centre on every one of those
//! frames already placed on the animated skeleton (the figatree pose of the joint at that frame; root-motion clips with
//! `joint_00` held at the origin, as the view draws them), in fighter space: SSB64 units from the feet, +Z forward, +Y up.
//!
//! Questions this answers: [`Motion::active`] (hitboxes on at a 60 Hz frame), [`Motion::active_in_tick`] (the sim's 30 Hz
//! tick `t` covers frames `2t` and `2t + 1`), [`ActiveHit::world`] (centre in the world for a fighter at `feet` facing
//! `yaw`), [`Motion::sounds_between`], [`Motion::hit_status_at`]. Each hit carries a [`Knockback`] descriptor with the
//! SSB64 inputs of its knockback (angle, base, growth, fixed, element, damage) for the knockback / percent layer.
//!
//! Data: the app loads the file once and [`register`]s it; [`crate::smash::SmashExt::scripts`] can hold another table
//! (tests). Without data nothing here is used and the specials keep their approximate numbers.
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// What a hitbox does on contact besides damage (`GMHitElement`, src/gm/gmdef.h).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Element {
    Normal,
    Fire,
    Electric,
    Slash,
    Coin,
    /// The decomp's guess; flashes the screen blue on a strong hit.
    Freezing,
    Sleep,
    Other(u8),
}

impl Element {
    pub fn from_id(id: u8) -> Self {
        match id {
            0 => Element::Normal,
            1 => Element::Fire,
            2 => Element::Electric,
            3 => Element::Slash,
            4 => Element::Coin,
            5 => Element::Freezing,
            6 => Element::Sleep,
            n => Element::Other(n),
        }
    }
}

/// The SSB64 knockback inputs of one hit (`FTAttackColl`, src/ft/fttypes.h; the field names there are in brackets).
/// SSB64's knockback (`ftParamGetCommonKnockback`) is
/// `((((percent / 10 + percent * damage / 20) * 200 / (weight + 100) * 1.4) + 18) * growth / 100) + base`,
/// with `percent` the target's damage after the hit and `fixed` (when not 0) replacing the percent term: see the
/// knockback layer for the exact formula; this struct only carries the hitbox's numbers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Knockback {
    /// Launch angle in degrees (`angle`); 361 is the "Sakurai angle" (depends on the knockback / ground state).
    pub angle: i32,
    /// Knockback growth, percent (`knockback_scale`).
    pub growth: u32,
    /// Fixed ("set") knockback, 0 = none (`knockback_weight`).
    pub fixed: u32,
    /// Base knockback (`knockback_base`).
    pub base: u32,
    pub element: Element,
    /// Damage in percent (`damage`, before the game's move staling).
    pub damage: u32,
    /// Extra shield damage (`shield_damage`).
    pub shield_damage: i32,
    /// Whether it hits grounded / airborne targets (`is_hit_ground`, `is_hit_air`).
    pub hit_ground: bool,
    pub hit_air: bool,
    /// Clanks with other hitboxes (`can_rebound`).
    pub can_rebound: bool,
    /// Hit sound (`fgm_kind`: punch, kick, coin, burn, zap, slash, fan, bat) and loudness (`fgm_level`).
    pub sfx_kind: u8,
    pub sfx_level: u8,
}

#[derive(Deserialize, Debug)]
struct RawWindow {
    id: u8,
    set: u32,
    from: u32,
    to: Option<u32>,
    group: u8,
    joint: i32,
    offset: [i32; 3],
    size: u32,
    scaled: bool,
    damage: u32,
    angle: i32,
    knockback_scale: u32,
    knockback_weight: u32,
    knockback_base: u32,
    element: u8,
    shield_damage: i32,
    hit_ground: bool,
    hit_air: bool,
    can_rebound: bool,
    fgm_kind: u8,
    fgm_level: u8,
    positions: Vec<[f64; 3]>,
    on_joint: bool,
}

/// One SSB64 hitbox over a run of frames with constant values.
#[derive(Clone, Debug, PartialEq)]
pub struct HitWindow {
    /// Hitbox slot (`attack_colls[id]`, 0..3).
    pub id: u8,
    /// Hit records: a target is hit at most once per set (hitboxes of one group share them; a refresh starts a new set).
    pub set: u32,
    pub from: u32,
    /// Exclusive; `None`: on until the motion ends.
    pub to: Option<u32>,
    pub group: u8,
    /// SSB64 joint id (0 = the fighter's root at the feet, 4 + n = the glb's `joint_NN`) and offset from it.
    pub joint: i32,
    pub offset: [i32; 3],
    /// Diameter in SSB64 units (the game stores `size / 2` as the radius).
    pub size: u32,
    /// `MakeAttackCollScaled`: the offset ignores the fighter's model scale.
    pub scaled: bool,
    pub knockback: Knockback,
    /// Centre per frame from `from` (fighter space, SSB64 units); the last one holds past the end.
    pub positions: Vec<[f64; 3]>,
    /// False when the joint is not in the model (the offset from the feet was used).
    pub on_joint: bool,
}

impl HitWindow {
    pub fn active(&self, frame: u32) -> bool {
        frame >= self.from && self.to.is_none_or(|t| frame < t)
    }
    /// Radius in SSB64 units.
    pub fn radius(&self) -> f64 {
        self.size as f64 * 0.5
    }
    /// Centre at `frame` (fighter space, SSB64 units).
    pub fn position(&self, frame: u32) -> [f64; 3] {
        let k = frame.saturating_sub(self.from) as usize;
        self.positions.get(k.min(self.positions.len().saturating_sub(1))).copied().unwrap_or([0.0, 0.0, 0.0])
    }
}

/// A script event other than the hitbox ones, for sounds and hurtbox state (`op` = the decomp's `nFTMotionEvent*` name).
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptEvent {
    pub frame: u32,
    pub op: String,
    /// Sound id (`gmFGMVoiceID`) and its name (`VoiceCaptainSpecialNPunch`: the exported clip is `<name>_0.wav`).
    pub fgm: Option<u32>,
    pub name: Option<String>,
    /// The one-word argument (`SetHitStatusAll` status, flags...).
    pub value: Option<i64>,
}

/// Fighter hurtbox state (`GMHitStatus`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitStatus {
    None,
    Normal,
    /// Hit but takes no damage.
    Invincible,
    /// Cannot be hit.
    Intangible,
}

#[derive(Clone, Debug, Default)]
pub struct Motion {
    pub id: u32,
    /// The decomp's motion name (`Attack11`, `SpecialN`, `CopyCaptainSpecialN`...).
    pub name: String,
    /// Frame of the script's `End` (None: it loops or pauses).
    pub end: Option<u32>,
    /// The script repeats frames `from..from + period` (a goto back), if it loops.
    pub repeat: Option<(u32, u32)>,
    /// A root-motion clip (the sim moves the fighter, `joint_00` is held at the origin).
    pub root_motion: bool,
    pub hitboxes: Vec<HitWindow>,
    pub events: Vec<ScriptEvent>,
}

/// One hitbox on at a frame.
#[derive(Clone, Copy, Debug)]
pub struct ActiveHit<'a> {
    pub window: &'a HitWindow,
    pub frame: u32,
    /// Fighter space, SSB64 units.
    pub pos: [f64; 3],
}

impl ActiveHit<'_> {
    /// Centre in the world for a fighter standing at `feet` facing `yaw` (s16 units, SM64), with `k` world units per
    /// SSB64 unit (the sim's `smash::K`).
    pub fn world(&self, feet: [f64; 3], yaw: f64, k: f64) -> [f64; 3] {
        to_world(self.pos, feet, yaw, k)
    }
}

/// Fighter space (SSB64 units, +Z forward) -> world for a fighter at `feet` facing `yaw` (SM64 s16 angle).
pub fn to_world(p: [f64; 3], feet: [f64; 3], yaw: f64, k: f64) -> [f64; 3] {
    let (s, c) = (crate::math::sins(yaw), crate::math::coss(yaw));
    [feet[0] + (p[0] * c + p[2] * s) * k, feet[1] + p[1] * k, feet[2] + (-p[0] * s + p[2] * c) * k]
}

impl Motion {
    /// The script frame that plays at motion frame `frame` (a repeating script wraps).
    pub fn script_frame(&self, frame: u32) -> u32 {
        match self.repeat {
            Some((from, period)) if period > 0 && frame >= from + period => from + (frame - from) % period,
            _ => frame,
        }
    }

    pub fn has_hitboxes(&self) -> bool {
        !self.hitboxes.is_empty()
    }

    /// The hitboxes on at a 60 Hz motion frame.
    pub fn active(&self, frame: u32) -> Vec<ActiveHit<'_>> {
        let f = self.script_frame(frame);
        self.hitboxes.iter().filter(|w| w.active(f)).map(|w| ActiveHit { window: w, frame: f, pos: w.position(f) }).collect()
    }

    /// The hitboxes on during sim tick `tick` of the motion (frames `2 * tick` and `2 * tick + 1`), one entry per window
    /// and frame.
    pub fn active_in_tick(&self, tick: u32) -> Vec<ActiveHit<'_>> {
        self.active_in_frames(tick * 2, tick * 2 + 2)
    }

    /// The hitboxes on at any frame of `[from, to)`.
    pub fn active_in_frames(&self, from: u32, to: u32) -> Vec<ActiveHit<'_>> {
        (from..to.max(from + 1)).flat_map(|f| self.active(f)).collect()
    }

    /// First frame a hitbox comes on, and the frame after the last one ends (None: no hitbox or never ends).
    pub fn active_span(&self) -> Option<(u32, Option<u32>)> {
        let first = self.hitboxes.iter().map(|w| w.from).min()?;
        let last = self.hitboxes.iter().map(|w| w.to).try_fold(0u32, |a, t| t.map(|t| a.max(t)));
        Some((first, last))
    }

    /// Sound events (`PlayFGM`, `PlayVoiceStoreInfo`, ...) in frames `[from, to)`.
    pub fn sounds_between(&self, from: u32, to: u32) -> Vec<&ScriptEvent> {
        self.events.iter().filter(|e| e.fgm.is_some() && e.frame >= from && e.frame < to).collect()
    }

    /// The whole-body hurtbox state the script set by `frame` (`SetHitStatusAll` / `SetHitStatusPartAll`; normal before).
    pub fn hit_status_at(&self, frame: u32) -> HitStatus {
        let f = self.script_frame(frame);
        let mut s = HitStatus::Normal;
        for e in &self.events {
            if e.frame > f {
                break;
            }
            if e.op == "SetHitStatusAll" || e.op == "SetHitStatusPartAll" {
                s = match e.value.unwrap_or(1) {
                    0 => HitStatus::None,
                    2 => HitStatus::Invincible,
                    3 => HitStatus::Intangible,
                    _ => HitStatus::Normal,
                };
            }
        }
        s
    }
}

#[derive(Clone, Debug, Default)]
pub struct FighterScripts {
    /// `FTAttributes.size` (model scale).
    pub size: f64,
    pub motions: HashMap<u32, Motion>,
    by_name: HashMap<String, u32>,
}

impl FighterScripts {
    pub fn by_id(&self, id: u32) -> Option<&Motion> {
        self.motions.get(&id)
    }
    /// The motion of that decomp name (the lowest id when two share it).
    pub fn by_name(&self, name: &str) -> Option<&Motion> {
        self.by_name.get(name).and_then(|id| self.motions.get(id))
    }
    /// The first of these names the fighter has (the order a move lists its motions in).
    pub fn first<S: AsRef<str>>(&self, names: &[S]) -> Option<&Motion> {
        names.iter().find_map(|n| self.by_name(n.as_ref()))
    }
}

/// Every fighter's scripts (keys: the decomp's lowercase names, `mario`, `captain`, `purin`...).
#[derive(Clone, Debug, Default)]
pub struct MotionEvents {
    pub fighters: HashMap<String, FighterScripts>,
}

impl MotionEvents {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("motion-events.json: {e}"))?;
        Self::from_value(&v)
    }

    pub fn from_value(v: &serde_json::Value) -> Result<Self, String> {
        let fighters = v.get("fighters").and_then(|f| f.as_object()).ok_or("motion-events.json: no fighters")?;
        let mut out = MotionEvents::default();
        for (name, f) in fighters {
            let mut fs = FighterScripts { size: f.get("size").and_then(|s| s.as_f64()).unwrap_or(1.0), ..Default::default() };
            for (id, m) in f.get("motions").and_then(|m| m.as_object()).into_iter().flatten() {
                let id: u32 = id.parse().map_err(|_| format!("{name}: motion id {id}"))?;
                let hitboxes = match m.get("hitboxes") {
                    Some(h) => Vec::<RawWindow>::deserialize(h).map_err(|e| format!("{name} motion {id}: {e}"))?,
                    None => vec![],
                };
                let u = |v: &serde_json::Value| v.as_u64().map(|x| x as u32);
                let repeat = m.get("loops").and_then(|l| l.as_array()).and_then(|l| {
                    l.iter().find(|x| x["thread"].as_u64() == Some(0)).and_then(|x| Some((u(&x["from"])?, u(&x["period"])?)))
                });
                let events = m
                    .get("events")
                    .and_then(|e| e.as_array())
                    .into_iter()
                    .flatten()
                    .filter(|e| e.get("thread").is_none())
                    .map(|e| ScriptEvent {
                        frame: u(&e["frame"]).unwrap_or(0),
                        op: e["op"].as_str().unwrap_or("").to_string(),
                        fgm: u(&e["fgm"]),
                        name: e["name"].as_str().map(str::to_string),
                        value: e["value"].as_i64(),
                    })
                    .collect();
                let motion = Motion {
                    id,
                    name: m.get("motion").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                    end: m.get("end").and_then(u),
                    repeat,
                    root_motion: m.get("root_motion").and_then(|r| r.as_bool()).unwrap_or(false),
                    hitboxes: hitboxes.into_iter().map(HitWindow::from).collect(),
                    events,
                };
                let e = fs.by_name.entry(motion.name.clone()).or_insert(id);
                *e = (*e).min(id);
                fs.motions.insert(id, motion);
            }
            out.fighters.insert(name.clone(), fs);
        }
        Ok(out)
    }

    /// A fighter by the decomp's name (`Mario`, `Captain`, `Purin`...; any case).
    pub fn fighter(&self, decomp_name: &str) -> Option<&FighterScripts> {
        self.fighters.get(&decomp_name.to_lowercase())
    }
}

impl From<RawWindow> for HitWindow {
    fn from(r: RawWindow) -> Self {
        HitWindow {
            id: r.id,
            set: r.set,
            from: r.from,
            to: r.to,
            group: r.group,
            joint: r.joint,
            offset: r.offset,
            size: r.size,
            scaled: r.scaled,
            knockback: Knockback {
                angle: r.angle,
                growth: r.knockback_scale,
                fixed: r.knockback_weight,
                base: r.knockback_base,
                element: Element::from_id(r.element),
                damage: r.damage,
                shield_damage: r.shield_damage,
                hit_ground: r.hit_ground,
                hit_air: r.hit_air,
                can_rebound: r.can_rebound,
                sfx_kind: r.fgm_kind,
                sfx_level: r.fgm_level,
            },
            positions: r.positions,
            on_joint: r.on_joint,
        }
    }
}

fn registry() -> &'static RwLock<Option<Arc<MotionEvents>>> {
    static R: OnceLock<RwLock<Option<Arc<MotionEvents>>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(None))
}

/// Install the scripts every [`crate::smash::SmashExt`] uses when it has none of its own (the app, once at start-up).
pub fn register(m: Arc<MotionEvents>) {
    if let Ok(mut g) = registry().write() {
        *g = Some(m);
    }
}

/// The registered scripts, if any.
pub fn registered() -> Option<Arc<MotionEvents>> {
    registry().read().ok().and_then(|g| g.clone())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A hand-written table in the export's format (no ROM data): Captain Falcon's Falcon Punch with the decomp's numbers
    /// (dCaptainMainMotion_FalconPunchGround: 24 / 25 / 26% on frames 42-46), and a looping Mario Tornado-like motion.
    pub(crate) const SAMPLE: &str = r#"{"fps":60,"game":"ssb64","fighters":{
      "captain":{"size":1.05,"motions":{"203":{"motion":"SpecialN","end":55,"loops":[],"root_motion":true,"script":0,
        "events":[{"frame":0,"op":"PlayVoiceStoreInfo","fgm":348,"name":"VoiceCaptainSpecialNFalcon"},
                  {"frame":42,"op":"PlayVoiceStoreInfo","fgm":347,"name":"VoiceCaptainSpecialNPunch"},{"frame":55,"op":"End"}],
        "hitboxes":[
         {"id":0,"set":0,"from":42,"to":47,"group":0,"joint":16,"offset":[100,0,0],"size":250,"scaled":false,"damage":24,"angle":361,
          "knockback_scale":120,"knockback_weight":0,"knockback_base":30,"element":0,"shield_damage":0,"hit_ground":true,"hit_air":true,
          "can_rebound":true,"fgm_kind":1,"fgm_level":2,"positions":[[0,250,300],[0,250,500],[0,250,520],[0,250,530],[0,250,530]],"on_joint":true},
         {"id":1,"set":0,"from":42,"to":47,"group":0,"joint":14,"offset":[60,0,0],"size":200,"scaled":false,"damage":25,"angle":361,
          "knockback_scale":120,"knockback_weight":0,"knockback_base":30,"element":0,"shield_damage":0,"hit_ground":true,"hit_air":true,
          "can_rebound":true,"fgm_kind":1,"fgm_level":2,"positions":[[0,260,250],[0,260,340],[0,260,350],[0,260,350],[0,260,350]],"on_joint":true}]}}},
      "mario":{"size":1.12,"motions":{"202":{"motion":"SpecialLw","end":null,"loops":[{"thread":0,"from":4,"period":6}],"root_motion":false,"script":0,
        "events":[{"frame":0,"op":"SetHitStatusAll","value":3},{"frame":8,"op":"SetHitStatusAll","value":1}],
        "hitboxes":[{"id":0,"set":0,"from":4,"to":6,"group":0,"joint":0,"offset":[0,300,150],"size":160,"scaled":false,"damage":1,"angle":180,
          "knockback_scale":0,"knockback_weight":1,"knockback_base":70,"element":0,"shield_damage":0,"hit_ground":true,"hit_air":true,
          "can_rebound":true,"fgm_kind":1,"fgm_level":1,"positions":[[0,300,150],[0,300,150]],"on_joint":false}]}}}}}"#;

    #[test]
    fn falcon_punch_hits_only_on_its_scripted_frames() {
        let ev = MotionEvents::from_json(SAMPLE).unwrap();
        let fp = ev.fighter("Captain").unwrap().by_name("SpecialN").unwrap();
        assert!(fp.active(41).is_empty());
        assert_eq!(fp.active(42).len(), 2);
        assert_eq!(fp.active(46).len(), 2);
        assert!(fp.active(47).is_empty());
        let kb = fp.active(42)[0].window.knockback;
        assert_eq!((kb.damage, kb.angle, kb.growth, kb.base, kb.fixed, kb.element), (24, 361, 120, 30, 0, Element::Normal));
        // 30 Hz: tick 21 covers frames 42-43, tick 20 frames 40-41, tick 23 frames 46-47
        assert!(fp.active_in_tick(20).is_empty());
        assert_eq!(fp.active_in_tick(21).len(), 4);
        assert_eq!(fp.active_in_tick(23).len(), 2);
        assert!(fp.active_in_tick(24).is_empty());
        assert_eq!(fp.active_span(), Some((42, Some(47))));
        assert_eq!(fp.active(43)[0].pos, [0.0, 250.0, 500.0]);
        assert_eq!(fp.sounds_between(40, 44)[0].name.as_deref(), Some("VoiceCaptainSpecialNPunch"));
        // facing +Z (yaw 0): forward stays +Z; facing +X (yaw 0x4000): forward becomes +X
        let w = fp.active(43)[0].world([10.0, 0.0, 0.0], 0.0, 0.5);
        assert_eq!(w, [10.0, 125.0, 250.0]);
        let w = fp.active(43)[0].world([0.0; 3], 16384.0, 1.0);
        assert!((w[0] - 500.0).abs() < 1e-9 && w[2].abs() < 1e-9, "{w:?}");
    }

    #[test]
    fn a_repeating_script_wraps_and_tracks_intangibility() {
        let ev = MotionEvents::from_json(SAMPLE).unwrap();
        let t = ev.fighter("mario").unwrap().by_id(202).unwrap();
        assert_eq!(t.script_frame(9), 9);
        assert_eq!(t.script_frame(10), 4);
        assert_eq!(t.script_frame(17), 5);
        assert_eq!(t.active(5).len(), 1);
        assert!(t.active(7).is_empty());
        assert_eq!(t.active(11).len(), 1, "frame 11 plays script frame 5");
        assert_eq!(t.hit_status_at(0), HitStatus::Intangible);
        assert_eq!(t.hit_status_at(8), HitStatus::Normal);
        assert_eq!(t.active_span(), Some((4, Some(6))));
    }
}
