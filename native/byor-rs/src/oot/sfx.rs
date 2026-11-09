//! Port of `scripts/export-oot-sfx.py`: item / combat / system / enemy / environment sound effects rendered from the
//! sequence 0 channel scripts and Soundfont 0 / 1 of the ROM. Same floating point operations in the same order as the
//! Python (libm `pow` / `sin`), so the WAV files are identical.
use super::voices::{audio_seg, vadpcm, wav_bytes};
use super::{f32_, i16_, i32_, libm, round_to, u32_, u8_, Ctx};
use crate::pyc::{dumps, int0, re, round, R};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

const OUT_RATE: f64 = 16000.0;
const ENV_TICK: f64 = 1.0 / 180.0;
const TICKS_PER_SECOND: f64 = 96.0;
const MAX_SECONDS: f64 = 1.8;
const CLIP_SECONDS: f64 = 2.0;
const PEAK: f64 = 26000.0;

const WANTED: &str = "
IT_SWORD_IMPACT IT_SWORD_SWING IT_SWORD_PUTAWAY IT_SWORD_PICKOUT IT_ARROW_SHOT IT_BOOMERANG_THROW IT_SHIELD_BOUND
IT_BOW_DRAW IT_SHIELD_REFLECT_SW IT_ARROW_STICK_HRAD IT_HAMMER_HIT IT_HOOKSHOT_CHAIN IT_SHIELD_REFLECT_MG IT_BOMB_IGNIT
IT_BOMB_EXPLOSION IT_BOMB_UNEXPLOSION IT_BOOMERANG_FLY IT_SWORD_STRIKE IT_HAMMER_SWING IT_HOOKSHOT_REFLECT
IT_ARROW_STICK_CRE IT_ARROW_STICK_OBJ IT_SWORD_SWING_HARD IT_WALL_HIT_HARD IT_WALL_HIT_SOFT IT_STONE_HIT IT_WOODSTICK_BROKEN
IT_SHIELD_POSTURE IT_SLING_SHOT IT_SLING_DRAW IT_SWORD_CHARGE IT_ROLLING_CUT IT_SWORD_STRIKE_HARD IT_SLING_REFLECT
IT_HOOKSHOT_READY IT_HOOKSHOT_RECEIVE IT_HOOKSHOT_STICK_OBJ IT_SWORD_REFLECT_MG IT_DEKU IT_ROLLING_CUT_LV1
IT_ROLLING_CUT_LV2 IT_BOW_FLICK IT_BOMBCHU_MOVE IT_SLING_FLICK IT_SWORD_STICK_STN IT_REFLECTION_WOOD IT_MAGIC_ARROW_SHOT
IT_EXPLOSION_FRAME IT_EXPLOSION_ICE IT_EXPLOSION_LIGHT IT_FLAME IT_MASTER_SWORD_SWING
SY_GET_RUPY SY_GET_ITEM SY_HP_RECOVER SY_ERROR SY_DECIDE SY_CURSOR SY_CANCEL SY_LOCK_ON SY_LOCK_ON_HUMAN SY_FOUND
SY_HIT_SOUND SY_CORRECT_CHIME SY_TRE_BOX_APPEAR SY_PIECE_OF_HEART SY_GET_BOXITEM SY_SET_FIRE_ARROW SY_SET_ICE_ARROW
SY_SET_LIGHT_ARROW SY_RUPY_COUNT SY_HITPOINT_ALARM SY_GLASSMODE_ON SY_GLASSMODE_OFF SY_ATTENTION_ON
PL_DAMAGE PL_BODY_HIT PL_CATCH_BOOMERANG PL_CHANGE_ARMS PL_TAKE_OUT_SHIELD PL_PULL_UP_POT PL_PULL_UP_WOODBOX
PL_PULL_UP_PLANT PL_PULL_UP_ROCK PL_THROW PL_MAGIC_FIRE PL_MAGIC_WIND_NORMAL PL_MAGIC_WIND_WARP PL_MAGIC_SOUL_NORMAL
PL_ARROW_CHARGE_FIRE PL_ARROW_CHARGE_ICE PL_ARROW_CHARGE_LIGHT PL_ICE_BROKEN PL_FREEZE PL_SPARK PL_KNOCK PL_ROLL
PL_PUT_OUT_ITEM PL_SWORD_CHARGE PL_BODY_BOUND PL_CLIMB_CLIFF
EV_EXPLOSION EV_DOOR_OPEN EV_DOOR_CLOSE EV_SLIDE_DOOR_OPEN EV_SLIDE_DOOR_CLOSE EV_FOOT_SWITCH EV_TBOX_UNLOCK EV_TBOX_OPEN
EV_FLAME_IGNITION EV_TORCH EV_BOMB_BOUND EV_BOX_BREAK EV_HAMMER_SWITCH EV_BLOCK_BOUND EV_PLANT_BROKEN EV_ROCK_BROKEN
EV_WEB_VIBRATION EV_WEB_BROKEN EV_WOODBOX_BREAK EV_POT_BROKEN EV_WALL_BROKEN EV_DIAMOND_SWITCH EV_BURNING EV_ICE_FREEZE
EV_ICE_BROKEN EV_STONE_BOUND EV_TREE_CUT EV_WOOD_HIT EV_BLOCK_RISING EV_BURN_OUT EV_FLAME_OF_FIRE EV_RED_EYE
EN_DEKU_DAMAGE EN_DEKU_DEAD EN_DEKU_ATTACK EN_DEKU_MOUTH EN_DEKU_JR_DEAD EN_NUTS_DAMAGE EN_NUTS_DEAD EN_NUTS_THROW EN_NUTS_UP
EN_NUTS_DOWN EN_NUTS_FAINT EN_EXTINCT EN_TEKU_DAMAGE EN_TEKU_DEAD EN_STAL_DAMAGE EN_STAL_DEAD EN_STAL_WALK EN_STALKID_DAMAGE
EN_STALKID_DEAD EN_STALKID_ATTACK EN_RIZA_DAMAGE EN_RIZA_DEAD EN_RIZA_ATTACK EN_WOLFOS_DAMAGE EN_WOLFOS_DEAD EN_WOLFOS_ATTACK
EN_REDEAD_DAMAGE EN_REDEAD_DEAD EN_REDEAD_ATTACK EN_OCTAROCK_ROCK EN_OCTAROCK_DEAD1 EN_OCTAROCK_DEAD2 EN_OCTAROCK_JUMP
EN_OCTAROCK_LAND EN_OCTAROCK_BUBLE EN_BUBLE_WING EN_BUBLE_BITE EN_BUBLE_DEAD EN_BUBLE_DAMAGE EN_MORIBLIN_DEAD
EN_MORIBLIN_ATTACK EN_PO_DAMAGE EN_PO_DEAD EN_BARI_DAMAGE EN_BARI_DEAD EN_FFLY_DEAD EN_FFLY_ATTACK EN_BIRI_DEAD
EN_BIRI_SPARK EN_LIKE_DAMAGE EN_LIKE_DEAD EN_GOMA_JR_DAM1 EN_GOMA_JR_DEAD EN_GOMA_DAM1 EN_GOMA_DEAD EN_GOMA_BJR_DAM1
EN_GOMA_BJR_DEAD EN_FLAME_DAMAGE EN_ANUBIS_DEAD EN_FREEZAD_DAMAGE EN_FREEZAD_DEAD EN_DODO_J_DAMAGE EN_DODO_J_DEAD
EN_VALVAISA_DAMAGE1 EN_VALVAISA_DEAD EN_AMOS_DAMAGE EN_AMOS_DEAD EN_SHELL_DEAD EN_BUBLEWALK_DAMAGE EN_BUBLEWALK_DEAD
EN_GERUDOFT_DAMAGE EN_GERUDOFT_DEAD EN_DEADHAND_DAMAGE EN_DEADHAND_DEAD EN_KDOOR_BREAK EN_KDOOR_HIT
EN_PIHAT_DAMAGE EN_PIHAT_SM_DEAD EN_PIHAT_FLY EN_PIHAT_LAND EN_TEKU_WALK EN_STALTU_DAMAGE EN_STALWALL_DEAD EN_KAICHO_DEAD
EN_KAICHO_CRY EN_KAICHO_ATTACK EN_RIVA_DAMAGE EN_RIVA_DEAD EN_RIVA_APPEAR EN_GOMA_JR_DAM2 EN_NUTS_CUTBODY EN_WOLFOS_CRY
EN_GOMA_JR_FREEZE EN_DEKU_SCRAPE EN_STALTU_DOWN EN_OCTAROCK_SINK EN_NUTS_WALK
";

type Op = (String, Vec<String>);

/// Blocks of seq_0.prg.seq: name -> (kind, ops) and label -> (block, index).
struct Seq {
    blocks: HashMap<String, (String, Vec<Op>)>,
    labels: HashMap<String, (String, usize)>,
}

fn split_args(s: &str) -> Vec<String> {
    s.split(',').map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).collect()
}

impl Seq {
    fn new(text: &str) -> Seq {
        let mut s = Seq { blocks: HashMap::new(), labels: HashMap::new() };
        let mut cur: Option<String> = None;
        let (head, label, op) = (re(r"^\.(channel|layer|envelope)\s+(\w+)"), re(r"^(\w+):\s*$"), re(r"^/\*.*?\*/\s+([.\w]+)\s*(.*?)\s*(?://.*)?$"));
        let (comment, framerate) = (re(r"/\*.*?\*/"), re(r"FRAMERATE_CONST\((\w+),\s*\w+\)"));
        for line in text.lines() {
            if let Some(m) = head.captures(line) {
                s.blocks.insert(m[2].to_string(), (m[1].to_string(), Vec::new()));
                s.labels.insert(m[2].to_string(), (m[2].to_string(), 0));
                cur = Some(m[2].to_string());
                continue;
            }
            let Some(name) = cur.clone() else { continue };
            if let Some(m) = label.captures(line) {
                let n = s.blocks[&name].1.len();
                s.labels.insert(m[1].to_string(), (name, n));
                continue;
            }
            if let Some(m) = op.captures(line) {
                let args = comment.replace_all(&m[2], "").into_owned();
                let args = framerate.replace_all(&args, "$1").into_owned();
                s.blocks.get_mut(&name).unwrap().1.push((m[1].to_string(), split_args(&args)));
            } else if line.starts_with('\t') || line.starts_with("    ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let b = s.blocks.get_mut(&name).unwrap();
                if !parts.is_empty() && b.0 == "envelope" {
                    b.1.push((parts[0].to_string(), split_args(&parts[1..].join(" "))));
                }
            }
        }
        s
    }
}

/// CODEC_SMALL_ADPCM: 16 samples in 5 bytes (header scale / predictor + sixteen 2-bit residuals, MSB first).
pub(super) fn small_adpcm(book: &[u8], data: &[u8]) -> R<Vec<i32>> {
    let (order, npred) = (i32_(book, 0) as usize, i32_(book, 4) as usize);
    let table: Vec<i64> = (0..8 * order * npred).map(|i| i16_(book, 8 + 2 * i) as i64).collect();
    let mut out = Vec::new();
    let (mut h2, mut h1) = (0i64, 0i64);
    let mut off = 0;
    while off + 4 < data.len() {
        let scale = (data[off] >> 4) as u32;
        let pred = (data[off] & 15) as usize % npred.max(1);
        let (t0, t1) = (&table[pred * 16..pred * 16 + 8], &table[pred * 16 + 8..pred * 16 + 16]);
        let mut res = [0i64; 16];
        for (k, b) in data[off + 1..off + 5].iter().enumerate() {
            for j in 0..4 {
                let n = ((b >> (6 - 2 * j)) & 3) as i64;
                res[4 * k + j] = (if n >= 2 { n - 4 } else { n }) << scale;
            }
        }
        for half in [0usize, 8] {
            let e = &res[half..half + 8];
            let mut y = [0i64; 8];
            for i in 0..8 {
                let mut acc = (e[i] << 11) + h2 * t0[i] + h1 * t1[i];
                for j in 0..i {
                    acc += e[j] * t1[i - 1 - j];
                }
                y[i] = (acc >> 11).clamp(-32768, 32767);
            }
            out.extend(y.iter().map(|&v| v as i32));
            h2 = y[6];
            h1 = y[7];
        }
        off += 5;
    }
    Ok(out)
}

#[derive(Clone)]
struct Sample {
    pcm: std::rc::Rc<Vec<i32>>,
    lp: Option<(usize, usize)>,
    tuning: f64,
}

struct Font<'a> {
    font: &'a [u8],
    table: &'a [u8],
    cache: HashMap<(usize, u32), Option<Sample>>,
}

impl<'a> Font<'a> {
    fn sample(&mut self, ptr: usize, tuning: f32) -> R<Option<Sample>> {
        let key = (ptr, tuning.to_bits());
        if let Some(c) = self.cache.get(&key) {
            return Ok(c.clone());
        }
        let font = self.font;
        let (bits, addr, lp, book) = (u32_(font, ptr), u32_(font, ptr + 4) as usize, u32_(font, ptr + 8) as usize, u32_(font, ptr + 12) as usize);
        let (codec, size) = (bits >> 28, (bits & 0xFF_FFFF) as usize);
        let r = if codec != 0 && codec != 3 {
            None
        } else {
            let (order, npred) = (i32_(font, book) as usize, i32_(font, book + 4) as usize);
            let b = &font[book..book + 8 + 16 * order * npred];
            let d = self.table.get(addr..addr + size).ok_or("IndexError: sample outside the Audiotable")?;
            let pcm = if codec == 0 { vadpcm(b, d)? } else { small_adpcm(b, d)? };
            let (lstart, lend, lcount) = (u32_(font, lp) as usize, u32_(font, lp + 4) as usize, u32_(font, lp + 8));
            let lp = if lcount != 0 { Some((lstart, lend.min(pcm.len()))) } else { None };
            Some(Sample { pcm: std::rc::Rc::new(pcm), lp, tuning: tuning as f64 })
        };
        self.cache.insert(key, r.clone());
        Ok(r)
    }

    /// `(sample, Some((envelope pointer, release index)))`, or `(None, None)` when the instrument or sample is missing.
    #[allow(clippy::type_complexity)]
    fn instrument(&mut self, i: usize, key: i32) -> R<(Option<Sample>, Option<(usize, u8)>)> {
        let font = self.font;
        let inst = u32_(font, 8 + 4 * i) as usize;
        if inst == 0 {
            return Ok((None, None));
        }
        let (lo, hi) = (u8_(font, inst + 1) as i32, u8_(font, inst + 2) as i32);
        let slot = if key < lo {
            8
        } else if key > hi {
            24
        } else {
            16
        };
        let (ptr, tuning) = (u32_(font, inst + slot) as usize, f32_(font, inst + slot + 4));
        if ptr == 0 {
            return Ok((None, None));
        }
        let env = u32_(font, inst + 4) as usize;
        Ok((self.sample(ptr, tuning)?, Some((env, u8_(font, inst + 3)))))
    }

    fn default_envelope(&self, env: usize) -> Vec<(i64, i64)> {
        let mut pts = Vec::new();
        for k in 0..64 {
            let (delay, arg) = (i16_(self.font, env + 4 * k) as i64, i16_(self.font, env + 4 * k + 2) as i64);
            pts.push((delay, arg));
            if delay == 0 || delay == -1 || delay == -3 {
                break;
            }
        }
        pts
    }
}

fn seq_envelope(seq: &Seq, name: &str) -> R<Option<Vec<(i64, i64)>>> {
    let Some(blk) = seq.blocks.get(name) else { return Ok(None) };
    let arg = |a: &Vec<String>, i: usize| -> R<i64> { int0(a.get(i).ok_or("IndexError: list index out of range")?) };
    let mut pts = Vec::new();
    for (op, a) in &blk.1 {
        match op.as_str() {
            "point" => pts.push((arg(a, 0)?, arg(a, 1)?)),
            "hang" => pts.push((-1, 0)),
            "disable" => pts.push((0, 0)),
            "goto" => pts.push((-2, arg(a, 0)?)),
            "restart" => pts.push((-3, 0)),
            _ => {}
        }
    }
    Ok(Some(pts))
}

fn release_seconds(index: Option<i64>) -> f64 {
    match index {
        None => 0.1,
        Some(i) => 0.02 + libm::pow_((255 - i) as f64 / 255.0, 1.5) * 1.5,
    }
}

/// `envelope_gain`: function(t) -> 0..1.
fn envelope_gain(points: &[(i64, i64)], t_on: f64, release: f64) -> R<impl Fn(f64) -> f64> {
    let n = points.len() as i64;
    let mut knots: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    let (mut t, mut level, mut i, mut hops) = (0.0f64, 0.0f64, 0i64, 0);
    let mut hang: Option<f64> = None;
    let at = |i: i64| -> R<(i64, i64)> {
        let j = if i < 0 { i + n } else { i };
        points.get(j as usize).copied().filter(|_| j >= 0).ok_or_else(|| "IndexError: list index out of range".to_string())
    };
    while i < n && hops < 4 && t < 30.0 {
        let (delay, arg) = at(i)?;
        if delay == 0 {
            break;
        }
        if delay == -1 {
            hang = Some(t);
            break;
        }
        if delay == -2 {
            i = arg;
            hops += 1;
            continue;
        }
        if delay == -3 {
            break;
        }
        t += delay as f64 * ENV_TICK;
        level = arg as f64 / 32767.0;
        knots.push((t, level));
        i += 1;
    }
    let mut sustain = level;
    if (hang.is_none() && i >= n) || (hang.is_none() && i < n && matches!(at(i)?.0, 0 | -3)) {
        sustain = 0.0;
    }
    let level_at = move |x: f64| -> f64 {
        let last = knots[knots.len() - 1];
        if x >= last.0 {
            return if hang.is_some() { sustain } else { last.1 };
        }
        for w in knots.windows(2) {
            let ((t0, l0), (t1, l1)) = (w[0], w[1]);
            if x <= t1 {
                return l0 + (l1 - l0) * (if t1 > t0 { (x - t0) / (t1 - t0) } else { 1.0 });
            }
        }
        0.0
    };
    Ok(move |x: f64| -> f64 {
        let mut g = level_at(x.min(t_on));
        if x > t_on {
            g = level_at(t_on) * (0.0f64).max(1.0 - (x - t_on) / release.max(1e-3));
        }
        g
    })
}

#[derive(Clone, PartialEq)]
enum Inst {
    Font(usize, usize),
    Wave(&'static str),
}

fn wave_table(kind: &str) -> Vec<i32> {
    let n = 64.0f64;
    (0..=64)
        .map(|k| {
            let k = k as f64;
            match kind {
                "saw" => (-12000.0 + 24000.0 * k / n) as i32,
                "tri" => (12000.0 * (4.0 * (k / n - 0.5).abs() - 1.0)) as i32,
                "square" => {
                    if k < 32.0 {
                        12000
                    } else {
                        -12000
                    }
                }
                _ => (12000.0 * libm::sin_(2.0 * std::f64::consts::PI * k / n)) as i32,
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn render_note(fonts: &mut [Font], inst: &Inst, key: i32, vel: i64, length_s: f64, env_points: &Option<Vec<(i64, i64)>>, release: f64, mix: &mut Vec<f64>, start_s: f64) -> R<bool> {
    if start_s > MAX_SECONDS {
        return Ok(true);
    }
    let (pcm, lp, src_rate, idata): (std::rc::Rc<Vec<i32>>, Option<(usize, usize)>, f64, Option<(usize, u8)>) = match inst {
        Inst::Wave(kind) => (std::rc::Rc::new(wave_table(kind)), Some((0, 64)), 27.5 * libm::pow_(2.0, key as f64 / 12.0) * 64.0, None),
        Inst::Font(f, i) => {
            let (sample, idata) = fonts.get_mut(*f).ok_or("KeyError: soundfont")?.instrument(*i, key)?;
            let Some(s) = sample else { return Ok(false) };
            (s.pcm, s.lp, s.tuning * 32000.0 * super::pitch_ratio(key), idata)
        }
    };
    let pts: Vec<(i64, i64)> = match env_points {
        Some(p) => p.clone(),
        None => match (inst, idata) {
            (Inst::Font(f, _), Some((env, _))) if env != 0 => fonts[*f].default_envelope(env),
            _ => vec![(1, 32000), (-1, 0)],
        },
    };
    let gain = if !pts.is_empty() && pts[0].0 != 0 { Some(envelope_gain(&pts, length_s, release)?) } else { None };
    let total = if lp.is_some() || gain.is_some() { (length_s + release).min(MAX_SECONDS) } else { MAX_SECONDS };
    let step = src_rate / OUT_RATE;
    let n = (total * OUT_RATE) as usize;
    let (mut pos, base) = (0.0f64, (start_s * OUT_RATE) as usize);
    if mix.len() < base + n {
        mix.resize(base + n, 0.0);
    }
    let v = vel as f64 / 127.0;
    for k in 0..n {
        let mut i = pos as usize;
        if let Some((l0, l1)) = lp {
            if i >= l1 {
                i = l0 + ((i as i64 - l0 as i64).rem_euclid(1.max(l1 as i64 - l0 as i64))) as usize;
                pos = i as f64;
            }
        }
        if i + 1 >= pcm.len() {
            break;
        }
        let frac = pos - i as f64;
        let x = pcm[i] as f64 + (pcm[i + 1] - pcm[i]) as f64 * frac;
        let t = k as f64 / OUT_RATE;
        let g = match &gain {
            Some(g) => g(t),
            None => {
                if t < length_s || lp.is_some() {
                    1.0
                } else {
                    (0.0f64).max(1.0 - (t - length_s) / release.max(1e-3))
                }
            }
        };
        mix[base + k] += x * g * v;
        pos += step;
    }
    Ok(true)
}

struct Chan<'a> {
    seq: &'a Seq,
    pitches: &'a HashMap<String, i64>,
    mix: Vec<f64>,
    notes: usize,
    report: BTreeSet<String>,
}

fn a_at(a: &[String], i: usize) -> R<&String> {
    a.get(i).ok_or_else(|| "IndexError: list index out of range".to_string())
}

#[allow(clippy::too_many_arguments)]
fn run(c: &mut Chan, fonts: &mut [Font], block: &str, idx: usize, inst: Option<Inst>, transpose: i64, env: Option<Vec<(i64, i64)>>, rel: f64, start: i64, depth: usize) -> R<()> {
    let (mut block, mut idx, mut inst, mut transpose, mut env, mut rel) = (block.to_string(), idx, inst, transpose, env, rel);
    let (mut t, mut last_len, mut steps) = (start, 48i64, 0);
    if depth > 900 {
        return Err("RecursionError".into());
    }
    let seq = c.seq;
    while steps < 200 {
        steps += 1;
        let ops = &seq.blocks.get(&block).ok_or_else(|| format!("KeyError: '{block}'"))?.1;
        if idx >= ops.len() {
            return Ok(());
        }
        let (op, a) = &ops[idx];
        idx += 1;
        match op.as_str() {
            "end" => return Ok(()),
            "instr" => {
                let a0 = a_at(a, 0)?;
                inst = match re(r"^SF(\d)_INST_(\d+)").captures(a0) {
                    Some(m) => Some(Inst::Font(m[1].parse().unwrap(), m[2].parse().map_err(|_| "bad instrument")?)),
                    None => match a0.as_str() {
                        "FONTANY_INSTR_SAWTOOTH" => Some(Inst::Wave("saw")),
                        "FONTANY_INSTR_TRIANGLE" => Some(Inst::Wave("tri")),
                        "FONTANY_INSTR_SINE" => Some(Inst::Wave("sine")),
                        "FONTANY_INSTR_SQUARE" => Some(Inst::Wave("square")),
                        _ => None,
                    },
                };
            }
            "transpose" => transpose = int0(a_at(a, 0)?)?,
            "env" => {
                env = if a.is_empty() { None } else { seq_envelope(seq, &a[0])? };
                if a.len() > 1 {
                    rel = release_seconds(Some(int0(&a[1])?));
                }
            }
            "releaserate" => rel = release_seconds(Some(int0(a_at(a, 0)?)?)),
            "ldlayer" => run(c, fonts, a_at(a, 1)?, 0, inst.clone(), transpose, env.clone(), rel, t, depth + 1)?,
            "notedv" | "notedvg" | "noteldv" | "notevg" => {
                let p = a_at(a, 0)?;
                let key = match c.pitches.get(p) {
                    Some(k) => *k,
                    None => match int0(p) {
                        Ok(k) => k,
                        Err(_) => continue,
                    },
                };
                let vel;
                if op == "notedv" || op == "notedvg" {
                    last_len = int0(a_at(a, 1)?)?;
                    vel = int0(a_at(a, 2)?)?;
                } else {
                    vel = if a.len() > 1 { int0(&a[1])? } else { 100 };
                }
                if let Some(i) = &inst {
                    let len = (if last_len != 0 { last_len } else { 150 }) as f64 / TICKS_PER_SECOND;
                    if render_note(fonts, i, (key + transpose) as i32, vel, len, &env, rel, &mut c.mix, t as f64 / TICKS_PER_SECOND)? {
                        c.notes += 1;
                    } else if let Inst::Font(f, n) = i {
                        c.report.insert(format!("instrument ({f}, {n}) unavailable"));
                    }
                }
                if op != "noteldv" {
                    t += last_len;
                }
            }
            "ldelay" | "delay1" | "delay" | "lldelay" => {
                if let Some(Ok(d)) = a.first().map(|s| int0(s)) {
                    t += d;
                }
            }
            "jump" | "rjump" | "call" => {
                let last = a.last().ok_or("IndexError: list index out of range")?;
                if let Some((b, i)) = seq.labels.get(last).filter(|_| depth < 6) {
                    if op == "call" {
                        run(c, fonts, b, *i, inst.clone(), transpose, env.clone(), rel, t, depth + 1)?;
                    } else {
                        block = b.clone();
                        idx = *i;
                        steps += 20;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `write_clip`: normalised 16 kHz mono WAV; returns (frames, peak, rms).
fn write_clip(path: &Path, mut mix: Vec<f64>) -> R<(usize, i64, f64)> {
    let peak = mix.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let peak = if peak == 0.0 { 1.0 } else { peak };
    let scale = PEAK / peak;
    mix.truncate((CLIP_SECONDS * OUT_RATE) as usize);
    let fade = (0.05 * OUT_RATE) as usize;
    let len = mix.len();
    for k in 0..fade.min(len) {
        mix[len - 1 - k] *= k as f64 / fade as f64;
    }
    let pcm: Vec<i32> = mix.iter().map(|&x| round(x * scale).clamp(-32768, 32767) as i32).collect();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(path, wav_bytes(&pcm, OUT_RATE as u32)).map_err(|e| e.to_string())?;
    let sum: i64 = pcm.iter().map(|&x| x as i64 * x as i64).sum();
    let rms = (sum as f64 / pcm.len() as f64).powf(0.5);
    Ok((pcm.len(), pcm.iter().map(|&x| (x as i64).abs()).max().unwrap_or(0), round_to(rms, 1)))
}

pub fn export(ctx: &Ctx, out: &Path) -> R<()> {
    let seq_path = ctx.decomp.join("assets/audio/sequences/seq_0.prg.seq");
    if !seq_path.is_file() {
        return Err(format!("OoT sfx: need {}", seq_path.display()));
    }
    let bank = audio_seg(ctx, 3, 179648)?;
    let table = audio_seg(ctx, 5, 4526992)?;
    let mut fonts = [Font { font: &bank[0..15008], table: &table, cache: HashMap::new() }, Font { font: &bank[15008..15008 + 6064], table: &table, cache: HashMap::new() }];
    let aseq = ctx.read_decomp("include/audio/aseq.h")?;
    let pitches: HashMap<String, i64> = re(r"#define (PITCH_\w+)\s+(\d+)").captures_iter(&aseq).map(|m| (m[1].to_string(), m[2].parse().unwrap())).collect();
    let mut chans: HashMap<String, String> = HashMap::new();
    let mut tables: Vec<_> = std::fs::read_dir(ctx.decomp.join("include/tables/sfx")).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with("_table.h"))).collect();
    tables.sort();
    let define = re(r"DEFINE_SFX\((\w+), (NA_SE_\w+),");
    for t in tables {
        for m in define.captures_iter(&crate::pyc::read(&t)?) {
            chans.entry(m[2].to_string()).or_insert_with(|| m[1].to_string());
        }
    }
    let seq = Seq::new(&std::fs::read_to_string(&seq_path).map_err(|e| e.to_string())?);
    let mut manifest: BTreeMap<String, Value> = BTreeMap::new();
    let mut skipped: BTreeMap<String, String> = BTreeMap::new();
    for name in WANTED.split_whitespace() {
        let full = format!("NA_SE_{name}");
        let chan = chans.get(&full);
        let Some(chan) = chan.filter(|c| seq.blocks.contains_key(*c)) else {
            skipped.insert(name.to_string(), if chan.is_none() { "no channel script in the sfx tables".into() } else { format!("{} not in seq_0", chan.unwrap()) });
            continue;
        };
        let mut c = Chan { seq: &seq, pitches: &pitches, mix: Vec::new(), notes: 0, report: BTreeSet::new() };
        if let Err(e) = run(&mut c, &mut fonts, chan, 0, None, 0, None, 0.1, 0, 0) {
            skipped.insert(name.to_string(), format!("{chan}: {e}"));
            continue;
        }
        if c.notes == 0 || c.mix.is_empty() || c.mix.iter().fold(0.0f64, |m, x| m.max(x.abs())) < 1.0 {
            let why = if c.report.is_empty() { "none rendered".to_string() } else { c.report.iter().cloned().collect::<Vec<_>>().join(", ") };
            skipped.insert(name.to_string(), format!("{chan}: no decodable note ({why})"));
            continue;
        }
        let rel = format!("voices/oot_sfx/{name}_0.wav");
        let (frames, peak, rms) = write_clip(&out.parent().unwrap().join(&rel), c.mix)?;
        manifest.insert(rel, json!({"sampleRate": 16000, "frames": frames, "duration": round_to(frames as f64 / OUT_RATE, 4), "peak": peak, "rms": rms, "source": format!("{full} {chan}, {} note(s)", c.notes)}));
    }
    let clips: Map<String, Value> = manifest.into_iter().collect();
    let skipped: Map<String, Value> = skipped.into_iter().map(|(k, v)| (k, Value::from(v))).collect();
    let doc = json!({"clips": clips, "skipped": skipped});
    std::fs::write(out.join("manifest-oot-sfx.json"), dumps(&doc, Some(1)) + "\n").map_err(|e| e.to_string())
}
