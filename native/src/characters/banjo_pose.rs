//! Port of the animation logic of `BanjoView.ts`: Rare's keyframe sampling
//! (`animationfilebin_func_8033AC38`), clip choice from the sim state, pose blending and the
//! bone matrices (`animMtxList_setBoned`: parent * T(pivot + s*t) * R * S * T(-pivot)).
use super::assets::{BanjoAsset, BanjoClip};
use bevy::math::{EulerRot, Mat4, Quat, Vec3};
use jumpman_sim::characters::*;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;
use std::collections::HashMap;

/// Banjo's walking stride and Mario's forward speed, in course units per frame.
const WALK_SPEED: f64 = 12.0;
const RUN_SPEED: f64 = 26.0;
const BLEND: f64 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    KazooieUpper,
    KazooieFeet,
    KazooieAss,
}

impl Part {
    pub fn from_name(s: &str) -> Option<Part> {
        match s {
            "kazooieUpper" => Some(Part::KazooieUpper),
            "kazooieFeet" => Some(Part::KazooieFeet),
            "kazooieAss" => Some(Part::KazooieAss),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub clip: &'static str,
    pub looping: bool,
    /// Seconds per playthrough when different from the clip's own.
    pub duration: Option<f64>,
    /// Start offset (0..1) the bs state seeds into its animation controller.
    pub start: Option<f64>,
    /// Drive the animation directly from the move's own timer (0..1).
    pub at: Option<f64>,
    pub parts: &'static [Part],
}

impl Choice {
    pub fn new(clip: &'static str, looping: bool) -> Self {
        Self { clip, looping, duration: None, start: None, at: None, parts: &[] }
    }
    pub fn parts(mut self, p: &'static [Part]) -> Self {
        self.parts = p;
        self
    }
    pub fn duration(mut self, d: f64) -> Self {
        self.duration = Some(d);
        self
    }
    pub fn start(mut self, s: f64) -> Self {
        self.start = Some(s);
        self
    }
    pub fn at(mut self, a: f64) -> Self {
        self.at = Some(a);
        self
    }
}

const UPPER: &[Part] = &[Part::KazooieUpper];

/// glspline_catmull_rom_interpolate with four knots.
fn catmull(x: f64, k: [f64; 4]) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let c2 = -0.5 * k[0] + 1.5 * k[1] - 1.5 * k[2] + 0.5 * k[3];
    let c1 = k[0] - 2.5 * k[1] + 2.0 * k[2] - 0.5 * k[3];
    let c0 = -0.5 * k[0] + 0.5 * k[2];
    ((c2 * x + c1) * x + c0) * x + k[1]
}

pub const DEFAULTS: [f64; 9] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0];

/// One channel of a Rare animation file at a frame time. `keys` is flat `[flags, frame, value*64]`.
pub fn sample(clip: &BanjoClip, channel: usize, keys: &[f64], time: f64) -> f64 {
    let n = keys.len() / 3;
    let flag15 = |i: usize| (keys[3 * i] as i64) >> 1;
    let flag14 = |i: usize| (keys[3 * i] as i64) & 1;
    let frame = |i: usize| keys[3 * i + 1];
    let value = |i: usize| keys[3 * i + 2] / 64.0;
    if time.trunc() < frame(0) {
        let k2 = value(0);
        let k3 = if flag15(0) == 1 && n >= 2 { value(1) } else { k2 };
        let def = DEFAULTS[channel];
        return catmull((time - clip.start) / (frame(0) - clip.start), [def, def, k2, k3]);
    }
    let mut last = n - 1;
    if time.trunc() >= frame(last) {
        let k1 = value(last);
        let k0 = if flag14(last) == 1 && n >= 2 { value(last - 1) } else { k1 };
        return catmull(time - frame(last), [k0, k1, k1, k1]);
    }
    let mut first = 0;
    while first + 1 < last {
        let mid = first + ((last - first) >> 1);
        if frame(mid) <= time.trunc() {
            first = mid;
        } else {
            last = mid;
        }
    }
    let (k1, k2) = (value(first), value(last));
    let t = (time - frame(first)) / (frame(last) - frame(first));
    if flag14(first) == 0 && flag15(last) == 0 {
        return k1 + (k2 - k1) * t;
    }
    let k0 = if flag14(first) == 1 && first > 0 { value(first - 1) } else { k1 };
    let k3 = if flag15(last) == 1 && last + 1 < n { value(last + 1) } else { k2 };
    catmull(t, [k0, k1, k2, k3])
}

#[derive(Clone)]
pub struct Pose {
    pub rot: Vec<Quat>,
    pub scale: Vec<Vec3>,
    pub trans: Vec<Vec3>,
}

impl Pose {
    fn new(n: usize) -> Self {
        Self { rot: vec![Quat::IDENTITY; n], scale: vec![Vec3::ONE; n], trans: vec![Vec3::ZERO; n] }
    }
}

pub struct BanjoAnimator {
    /// clip -> bone id -> 9 channels.
    channels: HashMap<String, HashMap<i64, Vec<Option<Vec<f64>>>>>,
    pub progress: f64,
    pub blend: f64,
    pub pose: Pose,
    from: Pose,
    target: Pose,
    pub active_clip: String,
    last_action: i64,
    pub shown_parts: Vec<Part>,
}

impl BanjoAnimator {
    pub fn new(asset: &BanjoAsset) -> Self {
        let mut channels = HashMap::new();
        for (name, clip) in &asset.clips {
            let mut by_bone: HashMap<i64, Vec<Option<Vec<f64>>>> = HashMap::new();
            for (bone, channel, keys) in &clip.elements {
                by_bone.entry(*bone).or_insert_with(|| vec![None; 9])[*channel] = Some(keys.clone());
            }
            channels.insert(name.clone(), by_bone);
        }
        let n = asset.bones.len();
        let mut a = Self {
            channels,
            progress: 0.0,
            blend: 1.0,
            pose: Pose::new(n),
            from: Pose::new(n),
            target: Pose::new(n),
            active_clip: String::new(),
            last_action: -1,
            shown_parts: vec![],
        };
        let mut pose = Pose::new(n);
        a.evaluate(asset, "idle", 0.0, &mut pose);
        a.pose = pose;
        a
    }

    /// Evaluate one clip at progress 0..1 into per-bone TRS.
    fn evaluate(&self, asset: &BanjoAsset, name: &str, progress: f64, out: &mut Pose) {
        let clip = &asset.clips[name];
        let by_bone = &self.channels[name];
        let time = clip.start + progress.clamp(0.0, 0.999999) * (clip.end - clip.start);
        let mut values = [0.0; 9];
        for (i, bone) in asset.bones.iter().enumerate() {
            let chans = by_bone.get(&bone.id);
            for c in 0..9 {
                values[c] = match chans.and_then(|ch| ch[c].as_ref()) {
                    Some(keys) if !keys.is_empty() => sample(clip, c, keys, time),
                    _ => DEFAULTS[c],
                };
            }
            let r = |v: f64| (v * std::f64::consts::PI / 180.0) as f32;
            out.rot[i] = Quat::from_euler(EulerRot::ZYX, r(values[2]), r(values[1]), r(values[0]));
            out.scale[i] = Vec3::new(values[3] as f32, values[4] as f32, values[5] as f32);
            out.trans[i] = Vec3::new(values[6] as f32, values[7] as f32, values[8] as f32);
        }
    }

    /// Bone world matrices of the current pose.
    pub fn matrices(&self, asset: &BanjoAsset) -> Vec<Mat4> {
        let s = asset.trans_scale as f32;
        let mut world: Vec<Mat4> = Vec::with_capacity(asset.bones.len());
        for (i, bone) in asset.bones.iter().enumerate() {
            let p = Vec3::new(bone.pivot[0] as f32, bone.pivot[1] as f32, bone.pivot[2] as f32);
            let local = Mat4::from_translation(p + self.pose.trans[i] * s)
                * Mat4::from_quat(self.pose.rot[i])
                * Mat4::from_scale(self.pose.scale[i])
                * Mat4::from_translation(-p);
            world.push(if bone.parent >= 0 { world[bone.parent as usize] * local } else { local });
        }
        world
    }

    pub fn update(&mut self, asset: &BanjoAsset, m: &MarioRuntime, dt: f64, smooth: bool) {
        let choice = choose(m);
        let clip = &asset.clips[choice.clip];
        let restart = choice.clip != self.active_clip || (!choice.looping && m.action as i64 != self.last_action);
        if restart {
            self.from = self.pose.clone();
            self.blend = if !self.active_clip.is_empty() && smooth { 0.0 } else { 1.0 };
            self.progress = choice.start.unwrap_or(0.0);
            self.active_clip = choice.clip.to_string();
        } else if choice.at.is_none() {
            self.progress += dt / choice.duration.unwrap_or(clip.duration);
            self.progress = if choice.looping { self.progress % 1.0 } else { self.progress.min(0.999999) };
        }
        if let Some(at) = choice.at {
            self.progress = at.clamp(0.0, 0.999999);
        }
        self.last_action = m.action as i64;
        self.blend = (self.blend + dt / BLEND).min(1.0);

        let mut target = self.target.clone();
        self.evaluate(asset, choice.clip, self.progress, &mut target);
        let b = self.blend as f32;
        for i in 0..self.pose.rot.len() {
            self.pose.rot[i] = self.from.rot[i].slerp(target.rot[i], b);
            self.pose.scale[i] = self.from.scale[i].lerp(target.scale[i], b);
            self.pose.trans[i] = self.from.trans[i].lerp(target.trans[i], b);
        }
        self.target = target;

        // Kazooie pokes out of the backpack only during the moves that use her.
        self.shown_parts = choice.parts.to_vec();
        if choice.clip == "idle" && ((self.progress > 0.0909 && self.progress < 0.6818) || self.progress > 0.7727) {
            self.shown_parts.push(Part::KazooieUpper);
        }
    }
}

fn in_set(a: u32, set: &[u32]) -> bool {
    set.contains(&a)
}

/// `BanjoView.choose`.
pub fn choose(m: &MarioRuntime) -> Choice {
    let a = m.action;
    if let Some(c) = choose_banjo_move(m) {
        return c;
    }
    if in_set(a, &[ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_NO_EXIT, ACT_STAR_DANCE_WATER, ACT_FALL_AFTER_STAR_GRAB]) {
        return Choice::new("jiggy", false).parts(UPPER);
    }
    if in_set(a, &[ACT_STANDING_DEATH, ACT_DEATH_ON_BACK, ACT_DEATH_ON_STOMACH, ACT_QUICKSAND_DEATH, ACT_DROWNING, ACT_WATER_DEATH]) {
        return Choice::new("die", false);
    }
    if in_set(a, &[ACT_DIVE, ACT_DIVE_SLIDE, ACT_SLIDE_KICK, ACT_SLIDE_KICK_SLIDE, ACT_STOMACH_SLIDE, ACT_SHOT_FROM_CANNON, ACT_FLYING]) {
        return Choice::new("barge", false).duration(0.7).parts(UPPER);
    }
    if a & ACT_FLAG_INVULNERABLE != 0 && a & ACT_FLAG_SWIMMING == 0 {
        return Choice::new("ow", false).duration(1.0);
    }
    if a & ACT_FLAG_SWIMMING != 0 {
        if a == ACT_WATER_IDLE {
            return Choice::new("swimIdle", true);
        }
        let deep = m.pos[1] < m.world.water_level(m.pos[0], m.pos[2]) - 160.0;
        return Choice::new(if deep { "dive" } else { "swim" }, true).parts(UPPER);
    }
    if a == ACT_CLIMBING_POLE {
        return Choice::new("climb", true);
    }
    if a & ACT_FLAG_ON_POLE != 0 || a & ACT_FLAG_HANGING != 0 || a == ACT_LEDGE_GRAB {
        return Choice::new("climbIdle", true);
    }
    if a == ACT_GROUND_POUND || a == ACT_GROUND_POUND_LAND {
        return Choice::new("bust", false).start(0.15).parts(UPPER);
    }
    if a == ACT_BACKFLIP {
        return Choice::new("flipEnter", false).start(0.45).duration(1.0).parts(UPPER);
    }
    if a == ACT_SIDE_FLIP || a == ACT_TRIPLE_JUMP {
        return Choice::new("tumble", true).parts(UPPER);
    }
    if a == ACT_JUMP_KICK {
        return Choice::new("peck", true).parts(UPPER);
    }
    if a == ACT_PUNCHING || a == ACT_MOVE_PUNCHING {
        return Choice::new("claw", false).duration(0.65);
    }
    if a == ACT_THROWING || a == ACT_AIR_THROW {
        return Choice::new("throw", false);
    }
    if a & ACT_FLAG_AIR != 0 {
        let jumps = [ACT_JUMP, ACT_DOUBLE_JUMP, ACT_STEEP_JUMP, ACT_WALL_KICK_AIR, ACT_TOP_OF_POLE_JUMP, ACT_HOLD_JUMP, ACT_LONG_JUMP];
        if in_set(a, &jumps) && m.vel[1] > 0.0 {
            return Choice::new("jump", false).start(0.3).parts(UPPER);
        }
        return Choice::new(if m.vel[1] > 0.0 { "jump" } else { "fall" }, m.vel[1] <= 0.0).start(if m.vel[1] > 0.0 { 0.5 } else { 0.0 }).parts(UPPER);
    }
    if a == ACT_JUMP_LAND || a == ACT_FREEFALL_LAND {
        return Choice::new("jump", false).start(0.6667).duration(1.4).parts(UPPER);
    }
    if a == ACT_START_CROUCHING {
        return Choice::new("crouchEnter", false);
    }
    if a == ACT_BUTT_SLIDE && m.action_arg == crate::kart_anywhere::ride::ARG_KART {
        // The kart from anywhere: the sitting slide (legs out in front, paws down by the bodywork), not the belly one.
        return Choice::new("slideBack", true);
    }
    if in_set(a, &[ACT_BUTT_SLIDE, ACT_BEGIN_SLIDING, ACT_BUTT_SLIDE_AIR, ACT_CROUCH_SLIDE]) {
        return Choice::new("slideFront", true);
    }
    if in_set(a, &[ACT_CROUCHING, ACT_START_CROUCHING, ACT_STOP_CROUCHING]) {
        return Choice::new("crouch", true);
    }
    if a == ACT_CRAWLING {
        return Choice::new("creep", true).duration(1.2);
    }
    if a == ACT_HOLD_IDLE || a == ACT_PICKING_UP || a == ACT_HOLD_JUMP_LAND {
        return Choice::new("carryIdle", true);
    }
    if a == ACT_HOLD_WALKING || a == ACT_HOLD_DECELERATING {
        return Choice::new("carry", true);
    }
    if a == ACT_TURNING_AROUND || a == ACT_BRAKING {
        return Choice::new("turn", false);
    }
    if a & ACT_FLAG_MOVING != 0 && m.forward_vel.abs() > 1.0 {
        let speed = m.forward_vel.abs();
        if speed < 5.0 {
            return Choice::new("creep", true).duration(0.8 * 5.0 / speed.max(2.0));
        }
        if speed < 18.0 {
            return Choice::new("walk", true).duration((0.43 * WALK_SPEED / speed).clamp(0.3, 1.5));
        }
        return Choice::new("run", true).duration((0.66 * RUN_SPEED / speed).max(0.4));
    }
    Choice::new("idle", true)
}

/// Banjo's own moves, with the animation phases of their bs states.
fn choose_banjo_move(m: &MarioRuntime) -> Option<Choice> {
    if let Some(c) = super::banjo_pose_ext::choose_ext(m) {
        return Some(c);
    }
    let t = m.action_timer;
    let a_held = m.controller.button_down & A_BUTTON != 0;
    let mv = banjo_move(m);
    if mv == BANJO_JUMP {
        // bsjump: 0.3 -> 0.5042 rising, on to 0.6667 falling, then the fall loop.
        if m.vel[1] > 0.0 {
            return Some(Choice::new("jump", false).at((0.3 + t / 30.0 / 1.9).min(0.5042)).parts(UPPER));
        }
        if t > 40.0 {
            return Some(Choice::new("fall", true).parts(UPPER));
        }
        return Some(Choice::new("jump", false).at((0.5042 + (t - 15.0) / 30.0 / 4.0 * 0.6667).min(0.6667)).parts(UPPER));
    }
    if mv == BANJO_FLAP {
        return Some(if t < 9.0 { Choice::new("flapEnter", false).parts(UPPER) } else { Choice::new("flap", true).parts(UPPER) });
    }
    if mv == BANJO_FLIP {
        if t < 26.0 {
            return Some(Choice::new("flipEnter", false).at((t / 26.0).min(0.999)).parts(UPPER));
        }
        return Some(if a_held { Choice::new("flipHold", true).parts(UPPER) } else { Choice::new("flipExit", false).parts(UPPER) });
    }
    if mv == BANJO_CLAW {
        return Some(Choice::new("claw", false).at(t / 39.0));
    }
    if mv == BANJO_ROLL {
        return Some(Choice::new("roll", false).at(t / 27.0));
    }
    if mv == BANJO_BARGE {
        return Some(Choice::new("barge", false).at(t / 30.0).parts(UPPER));
    }
    if mv == BANJO_PECK {
        return Some(if t < 15.0 { Choice::new("peckEnter", true).parts(UPPER) } else { Choice::new("peck", false).parts(UPPER) });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::assets::load_banjo;

    fn assets() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn mario() -> MarioRuntime {
        crate::characters::test_support::mario()
    }

    #[test]
    fn catmull_hits_knots() {
        let k = [0.0, 1.0, 3.0, 4.0];
        assert!((catmull(0.0, k) - 1.0).abs() < 1e-12);
        assert!((catmull(1.0, k) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn linear_keys_interpolate() {
        let clip = BanjoClip { start: 0.0, end: 10.0, duration: 1.0, elements: vec![] };
        // two keys, no spline flags: frame 0 -> 0, frame 10 -> 10 (value*64)
        let keys = [0.0, 0.0, 0.0, 0.0, 10.0, 640.0];
        assert!((sample(&clip, 0, &keys, 5.0) - 5.0).abs() < 1e-9);
        assert!((sample(&clip, 0, &keys, 20.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn movement_clip_choice() {
        let mut m = mario();
        assert_eq!(choose(&m).clip, "idle");
        m.set_action(ACT_WALKING, 0.0);
        m.forward_vel = 10.0;
        let walk = choose(&m);
        assert_eq!(walk.clip, "walk");
        assert!((walk.duration.unwrap() - 0.43 * 12.0 / 10.0).abs() < 1e-9);
        m.forward_vel = 30.0;
        assert_eq!(choose(&m).clip, "run");
        m.set_action(ACT_FREEFALL, BANJO_FLAP);
        m.action_timer = 20.0;
        let flap = choose(&m);
        assert_eq!((flap.clip, flap.looping, flap.parts), ("flap", true, UPPER));
    }

    /// Every exported clip binds only to bones that exist, the rig and all four meshes (body and
    /// Kazooie's upper half, feet and rear) are complete, and every clip poses all of them to finite
    /// positions that stay attached to Banjo (no exploded or collapsed Kazooie).
    #[test]
    fn every_clip_binds_the_full_rig_including_kazooie() {
        let asset = load_banjo(&assets()).unwrap();
        assert_eq!(asset.bones.len(), 63, "joint count");
        let ids: std::collections::HashSet<i64> = asset.bones.iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), asset.bones.len(), "bone ids are unique");
        for (i, b) in asset.bones.iter().enumerate() {
            assert!(b.parent < i as i64, "bone {i} parent {} must precede it", b.parent);
        }
        let mut tris = HashMap::new();
        for b in &asset.batches {
            *tris.entry(b.part.clone()).or_insert(0usize) += b.vertices.len() / 3;
            for v in &b.vertices {
                assert!((v[9] as usize) < asset.bones.len(), "vertex bound to missing bone {}", v[9]);
            }
        }
        for part in ["body", "kazooieUpper", "kazooieFeet", "kazooieAss"] {
            assert!(tris.get(part).copied().unwrap_or(0) >= 40, "mesh {part} missing or truncated: {tris:?}");
        }
        // Kazooie's meshes hang off bones of their own, none of which is Banjo's.
        let kazooie_bones: Vec<usize> = asset.batches.iter().filter(|b| b.part != "body").flat_map(|b| b.vertices.iter().map(|v| v[9] as usize)).collect();
        assert!(kazooie_bones.len() > 400);

        let mut anim = BanjoAnimator::new(&asset);
        for (name, clip) in &asset.clips {
            let mut seen = std::collections::HashSet::new();
            for (bone, channel, keys) in &clip.elements {
                assert!(ids.contains(bone), "clip {name} drives missing bone id {bone}");
                assert!(*channel < 9 && !keys.is_empty() && seen.insert((*bone, *channel)), "clip {name} bone {bone} channel {channel}");
            }
            for p in [0.0, 0.25, 0.5, 0.75, 0.999] {
                let mut pose = anim.pose.clone();
                anim.evaluate(&asset, name, p, &mut pose);
                anim.pose = pose;
                let mats = anim.matrices(&asset);
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for b in asset.batches.iter().filter(|b| b.part == "body") {
                    for v in &b.vertices {
                        let q = mats[v[9] as usize].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
                        assert!(q.is_finite(), "{name}@{p}: body vertex not finite");
                        lo = lo.min(q);
                        hi = hi.max(q);
                    }
                }
                for b in asset.batches.iter().filter(|b| b.part != "body") {
                    for v in &b.vertices {
                        let q = mats[v[9] as usize].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
                        assert!(q.is_finite(), "{name}@{p}: {} vertex not finite", b.part);
                        assert!(q.cmpge(lo - Vec3::splat(120.0)).all() && q.cmple(hi + Vec3::splat(120.0)).all(), "{name}@{p}: {} detached from Banjo: {q:?} vs {lo:?}..{hi:?}", b.part);
                    }
                }
            }
        }
    }

    /// Every clip `choose` can pick exists, and Kazooie is out for the whole jump (rise, fall,
    /// landing, tumble) instead of leaving an empty backpack.
    #[test]
    fn chosen_clips_exist_and_jumps_show_kazooie() {
        let asset = load_banjo(&assets()).unwrap();
        let mut m = mario();
        for action in [ACT_IDLE, ACT_WALKING, ACT_JUMP, ACT_DOUBLE_JUMP, ACT_FREEFALL, ACT_JUMP_LAND, ACT_FREEFALL_LAND, ACT_LONG_JUMP, ACT_SIDE_FLIP, ACT_TRIPLE_JUMP, ACT_GROUND_POUND, ACT_BACKFLIP, ACT_CROUCHING] {
            for arg in [0.0, BANJO_FLAP, BANJO_JUMP, BANJO_FLIP, BANJO_CLAW, BANJO_ROLL, BANJO_BARGE, BANJO_PECK] {
                for vy in [-20.0, 0.0, 20.0] {
                    m.set_action(action, arg);
                    m.vel[1] = vy;
                    m.forward_vel = 15.0;
                    for t in [0.0, 10.0, 30.0, 60.0] {
                        m.action_timer = t;
                        let c = choose(&m);
                        assert!(asset.clips.contains_key(c.clip), "{action:#x}/{arg} -> missing clip {}", c.clip);
                    }
                }
            }
        }
        for (action, vy) in [(ACT_JUMP, 20.0), (ACT_JUMP, -20.0), (ACT_DOUBLE_JUMP, 10.0), (ACT_FREEFALL, -10.0), (ACT_JUMP_LAND, 0.0), (ACT_SIDE_FLIP, 5.0)] {
            m.set_action(action, 0.0);
            m.vel[1] = vy;
            assert!(choose(&m).parts.contains(&Part::KazooieUpper), "action {action:#x} vy {vy} hides Kazooie");
        }
        for (arg, t) in [(BANJO_JUMP, 3.0), (BANJO_JUMP, 25.0), (BANJO_JUMP, 60.0)] {
            m.set_action(ACT_FREEFALL, arg);
            m.vel[1] = if t < 20.0 { 12.0 } else { -12.0 };
            m.action_timer = t;
            assert!(choose(&m).parts.contains(&Part::KazooieUpper), "banjo jump t={t} hides Kazooie");
        }
    }

    #[test]
    fn idle_pose_is_finite_and_body_sized() {
        let asset = load_banjo(&assets()).unwrap();
        let mut anim = BanjoAnimator::new(&asset);
        anim.update(&asset, &mario(), 1.0 / 30.0, true);
        let mats = anim.matrices(&asset);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for b in &asset.batches {
            for v in &b.vertices {
                let p = mats[v[9].max(0.0) as usize].transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
                assert!(p.is_finite());
                lo = lo.min(p.y);
                hi = hi.max(p.y);
            }
        }
        assert!(hi - lo > 60.0 && hi - lo < 200.0, "height {}", hi - lo);
    }
}
