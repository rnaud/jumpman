//! Time Trial ghosts, engine-free: a recorder that samples the player's kart (position, heading, slip angle, state
//! bits) every second tick of the 30 Hz race, a compact deterministic codec (delta + zig-zag varints, base64) for the
//! save, a bounded store (`kart_ghosts.json`, one ghost per track and character, at most [`MAX_GHOSTS`]) and the
//! playback pose (linear position, shortest-arc heading) the view draws as a translucent kart sprite.
//!
//! The ROM's staff ghosts (`courses/*/staff_ghost.inc.c`, Luigi / Mario / Royal Raceway) are controller recordings
//! (`StaffGhost { buttons, frameDuration, stickY, stickX }`), not poses: playing them needs the original frame-exact
//! physics, which this port only approximates, so they are not used.
use std::collections::BTreeMap;

/// A recorded run stores a sample every this many ticks.
pub const STRIDE: u32 = 2;
/// Longest run kept (8 minutes at 30 Hz); longer ones are not saved.
pub const MAX_TICKS: u32 = 30 * 60 * 8;
/// Ghosts kept in the save (the oldest go first).
pub const MAX_GHOSTS: usize = 16;
/// Positions are stored in 1/16 course units.
const Q: f64 = 16.;
const VERSION: u8 = 1;

pub const FLAG_DRIFT: u8 = 1;
pub const FLAG_SPIN: u8 = 2;
pub const FLAG_MIDAIR: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Sample {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// `Kart::yaw()` in 1/32768 of a half turn (wraps like an s16).
    pub yaw: i16,
    /// `Kart::slip_raw()` (s16 units).
    pub slip: i16,
    pub flags: u8,
}
impl Sample {
    pub fn new(pos: [f64; 3], yaw: f64, slip: i32, flags: u8) -> Self {
        Self {
            x: (pos[0] * Q).round() as i32,
            y: (pos[1] * Q).round() as i32,
            z: (pos[2] * Q).round() as i32,
            yaw: ((yaw * 32768. / std::f64::consts::PI).round() as i64) as i16,
            slip: slip.clamp(-32768, 32767) as i16,
            flags,
        }
    }
    pub fn pos(&self) -> [f64; 3] {
        [self.x as f64 / Q, self.y as f64 / Q, self.z as f64 / Q]
    }
    pub fn yaw_rad(&self) -> f64 {
        self.yaw as f64 * std::f64::consts::PI / 32768.
    }
}

/// Where the ghost is at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub pos: [f64; 3],
    pub yaw: f64,
    pub slip: i32,
    pub flags: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ghost {
    pub character: u8,
    /// Race time of the run in ticks.
    pub total: u32,
    /// Sample `i` is the kart at tick `i * STRIDE` of the race.
    pub samples: Vec<Sample>,
}

impl Ghost {
    /// The ghost at race tick `tick` (fractional: the view interpolates between ticks). `None` before the start;
    /// after the end the ghost stays on the line.
    pub fn pose_at(&self, tick: f64) -> Option<Pose> {
        if self.samples.is_empty() || tick < 0. {
            return None;
        }
        let f = tick / STRIDE as f64;
        let i = f.floor() as usize;
        if i + 1 >= self.samples.len() {
            let s = self.samples.last()?;
            return Some(Pose { pos: s.pos(), yaw: s.yaw_rad(), slip: s.slip as i32, flags: s.flags });
        }
        let t = f - i as f64;
        let (a, b) = (&self.samples[i], &self.samples[i + 1]);
        let (pa, pb) = (a.pos(), b.pos());
        let lerp = |u: f64, v: f64| u + (v - u) * t;
        // Shortest arc between the two s16 headings.
        let dy = (b.yaw.wrapping_sub(a.yaw)) as f64 * std::f64::consts::PI / 32768.;
        let ds = b.slip.wrapping_sub(a.slip) as f64;
        Some(Pose {
            pos: [lerp(pa[0], pb[0]), lerp(pa[1], pb[1]), lerp(pa[2], pb[2])],
            yaw: a.yaw_rad() + dy * t,
            slip: (a.slip as f64 + ds * t).round() as i32,
            flags: if t < 0.5 { a.flags } else { b.flags },
        })
    }
    /// Whether the run is short enough to keep.
    pub fn storable(&self) -> bool {
        self.total > 0 && self.total <= MAX_TICKS && !self.samples.is_empty()
    }

    pub fn encode(&self) -> String {
        let mut out = vec![VERSION, self.character];
        out.extend(self.total.to_le_bytes());
        out.extend((self.samples.len() as u32).to_le_bytes());
        let mut prev = Sample::default();
        for s in &self.samples {
            varint(&mut out, s.x - prev.x);
            varint(&mut out, s.y - prev.y);
            varint(&mut out, s.z - prev.z);
            varint(&mut out, s.yaw.wrapping_sub(prev.yaw) as i32);
            varint(&mut out, s.slip.wrapping_sub(prev.slip) as i32);
            out.push(s.flags);
            prev = *s;
        }
        b64_encode(&out)
    }

    pub fn decode(text: &str) -> Option<Ghost> {
        let bytes = b64_decode(text)?;
        if bytes.len() < 10 || bytes[0] != VERSION {
            return None;
        }
        let character = bytes[1];
        let total = u32::from_le_bytes(bytes[2..6].try_into().ok()?);
        let count = u32::from_le_bytes(bytes[6..10].try_into().ok()?) as usize;
        if count > (MAX_TICKS / STRIDE) as usize + 2 {
            return None;
        }
        let mut at = 10;
        let mut prev = Sample::default();
        let mut samples = Vec::with_capacity(count);
        for _ in 0..count {
            let dx = read_varint(&bytes, &mut at)?;
            let dy = read_varint(&bytes, &mut at)?;
            let dz = read_varint(&bytes, &mut at)?;
            let dyaw = read_varint(&bytes, &mut at)?;
            let dslip = read_varint(&bytes, &mut at)?;
            let flags = *bytes.get(at)?;
            at += 1;
            prev = Sample {
                x: prev.x.checked_add(dx)?,
                y: prev.y.checked_add(dy)?,
                z: prev.z.checked_add(dz)?,
                yaw: prev.yaw.wrapping_add(dyaw as i16),
                slip: prev.slip.wrapping_add(dslip as i16),
                flags,
            };
            samples.push(prev);
        }
        (at == bytes.len()).then_some(Ghost { character, total, samples })
    }
}

fn varint(out: &mut Vec<u8>, v: i32) {
    let mut z = ((v << 1) ^ (v >> 31)) as u32;
    loop {
        let b = (z & 0x7f) as u8;
        z >>= 7;
        if z == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}
fn read_varint(bytes: &[u8], at: &mut usize) -> Option<i32> {
    let (mut z, mut shift) = (0u32, 0);
    loop {
        let b = *bytes.get(*at)?;
        *at += 1;
        z |= ((b & 0x7f) as u32).checked_shl(shift)?;
        if b & 0x80 == 0 {
            return Some(((z >> 1) as i32) ^ -((z & 1) as i32));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn b64_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}
fn b64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for ch in text.bytes() {
        if ch == b'=' {
            break;
        }
        let v = B64.iter().position(|b| *b == ch)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Some(out)
}

/// Samples the player's kart during a race.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    samples: Vec<Sample>,
    next: u32,
    overflow: bool,
}
impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Call once per race tick (`race_tick` counts from 0 at GO) with the kart's state after the tick.
    pub fn record(&mut self, race_tick: u32, sample: Sample) {
        if race_tick > MAX_TICKS {
            self.overflow = true;
            return;
        }
        if race_tick % STRIDE == 0 && race_tick / STRIDE == self.samples.len() as u32 {
            self.samples.push(sample);
            self.next = race_tick + STRIDE;
        }
    }
    pub fn ticks(&self) -> u32 {
        self.samples.len() as u32 * STRIDE
    }
    /// Finish the run: the ghost of a race that took `total` ticks (None if too long or empty).
    pub fn finish(&self, character: u8, total: u32) -> Option<Ghost> {
        let ghost = Ghost { character, total, samples: self.samples.clone() };
        (!self.overflow && ghost.storable()).then_some(ghost)
    }
}

/// The saved ghosts (`kart_ghosts.json`): `track:character` -> encoded run.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GhostStore {
    pub ghosts: BTreeMap<String, StoredGhost>,
    pub counter: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredGhost {
    pub total: u32,
    pub seq: u64,
    pub data: String,
}
pub fn key(track: &str, character: usize) -> String {
    format!("{track}:{character}")
}
impl GhostStore {
    /// Keep `ghost` if it beats the saved one for the same track and character; trims to [`MAX_GHOSTS`].
    pub fn offer(&mut self, track: &str, ghost: &Ghost) -> bool {
        if !ghost.storable() {
            return false;
        }
        let k = key(track, ghost.character as usize);
        if self.ghosts.get(&k).is_some_and(|g| g.total <= ghost.total) {
            return false;
        }
        self.counter += 1;
        self.ghosts.insert(k, StoredGhost { total: ghost.total, seq: self.counter, data: ghost.encode() });
        while self.ghosts.len() > MAX_GHOSTS {
            let Some(oldest) = self.ghosts.iter().min_by_key(|(_, g)| g.seq).map(|(k, _)| k.clone()) else { break };
            self.ghosts.remove(&oldest);
        }
        true
    }
    pub fn get(&self, track: &str, character: usize) -> Option<Ghost> {
        self.ghosts.get(&key(track, character)).and_then(|g| Ghost::decode(&g.data))
    }
    /// Fastest ghost on a track whatever the character (what the Time Trial races against by default).
    pub fn best_on(&self, track: &str) -> Option<Ghost> {
        let prefix = format!("{track}:");
        self.ghosts.iter().filter(|(k, _)| k.starts_with(&prefix)).min_by_key(|(_, g)| g.total).and_then(|(_, g)| Ghost::decode(&g.data))
    }
    pub fn bytes(&self) -> usize {
        self.ghosts.values().map(|g| g.data.len()).sum()
    }
}
pub fn store_path() -> std::path::PathBuf {
    crate::progress::save::data_dir().join("kart_ghosts.json")
}
pub fn load_store() -> GhostStore {
    std::fs::read_to_string(store_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
pub fn save_store(s: &GhostStore) {
    let path = store_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string(s).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::kart_physics::{Floor, Ground, Input, Kart, Wall};

    struct Flat;
    impl Ground for Flat {
        fn floor(&self, _x: f64, _y: f64, _z: f64) -> Option<Floor> {
            Some(Floor::flat(0., false))
        }
        fn wall(&self, _pos: [f64; 3]) -> Option<Wall> {
            None
        }
    }

    /// Drive the real physics core through a fixed script: accelerate, weave, hop-drift, brake.
    fn drive(ticks: u32) -> (Recorder, Kart) {
        let mut kart = Kart::new([0., 0., 0.], 0);
        let mut rec = Recorder::new();
        for t in 0..ticks {
            let steer = ((t / 40) % 4) as i32 * 40 - 60;
            let input = Input { raw_x: steer, raw_y: 80, accel: t < ticks - 60, brake: t >= ticks - 60, hop_down: t % 97 > 60, hop_pressed: t % 97 == 61 };
            for f in 0..2 {
                kart.step(&Input { hop_pressed: input.hop_pressed && f == 0, ..input }, &Flat);
            }
            let flags = if kart.drifting { FLAG_DRIFT } else { 0 } | if kart.midair { FLAG_MIDAIR } else { 0 };
            rec.record(t, Sample::new(kart.pos, kart.yaw(), kart.slip_raw(), flags));
        }
        (rec, kart)
    }

    #[test]
    fn recording_is_deterministic_and_the_codec_round_trips_exactly() {
        let (a, ka) = drive(2400);
        let (b, kb) = drive(2400);
        assert_eq!(ka.pos, kb.pos);
        let (ga, gb) = (a.finish(2, 2400).unwrap(), b.finish(2, 2400).unwrap());
        assert_eq!(ga, gb);
        assert_eq!(ga.samples.len(), 1200);
        let text = ga.encode();
        assert_eq!(text, gb.encode(), "same run, same bytes");
        let back = Ghost::decode(&text).unwrap();
        assert_eq!(back, ga, "decode(encode(g)) == g");
        assert_eq!(back.character, 2);
        assert_eq!(back.total, 2400);
        // The ghost really moved and turned.
        assert!(ga.samples.iter().any(|s| s.flags & FLAG_DRIFT != 0) || ga.samples.last().unwrap().x != 0);
        let last = ga.samples.last().unwrap();
        assert!((last.x, last.z) != (0, 0));
    }

    #[test]
    fn playback_matches_the_recording_and_interpolates_between_samples() {
        let (rec, _) = drive(900);
        let g = rec.finish(0, 900).unwrap();
        // On a sample the pose is exactly the stored sample.
        for i in [0usize, 1, 50, 200, 449] {
            let p = g.pose_at((i as u32 * STRIDE) as f64).unwrap();
            assert_eq!(p.pos, g.samples[i].pos());
            assert!((p.yaw - g.samples[i].yaw_rad()).abs() < 1e-12);
        }
        // Between two samples the position is their midpoint and the heading goes the short way round.
        let (a, b) = (g.samples[100], g.samples[101]);
        let mid = g.pose_at(100. * STRIDE as f64 + 1.).unwrap();
        for k in 0..3 {
            assert!((mid.pos[k] - (a.pos()[k] + b.pos()[k]) / 2.).abs() < 1e-9);
        }
        assert!(g.pose_at(-1.).is_none());
        let end = g.pose_at(1e9).unwrap();
        assert_eq!(end.pos, g.samples.last().unwrap().pos(), "the ghost parks on the line after its run");
        // Wrapping heading: 32767 -> -32768 is a tiny turn, not a full one.
        let wrap = Ghost {
            character: 0,
            total: 10,
            samples: vec![Sample { yaw: 32767, ..Default::default() }, Sample { yaw: -32768, ..Default::default() }],
        };
        let p = wrap.pose_at(1.).unwrap();
        assert!((p.yaw - 32767.5 * std::f64::consts::PI / 32768.).abs() < 1e-6, "{}", p.yaw);
    }

    #[test]
    fn playback_replays_the_same_poses_every_time() {
        let (rec, _) = drive(1200);
        let g = Ghost::decode(&rec.finish(1, 1200).unwrap().encode()).unwrap();
        let run = |g: &Ghost| (0..1200).map(|t| g.pose_at(t as f64 + 0.25).unwrap()).collect::<Vec<_>>();
        assert_eq!(run(&g), run(&g.clone()));
    }

    #[test]
    fn size_is_bounded_and_garbage_is_rejected() {
        // A full 3-minute run stays small, and the store never exceeds its cap.
        let (rec, _) = drive(5400);
        let g = rec.finish(0, 5400).unwrap();
        let text = g.encode();
        assert!(text.len() < 60_000, "{} bytes for 3 minutes", text.len());
        let mut store = GhostStore::default();
        for i in 0..40 {
            let mut g = g.clone();
            g.total = 5000 + i;
            g.character = (i % 8) as u8;
            assert!(store.offer(&format!("track{i}"), &g));
        }
        assert_eq!(store.ghosts.len(), MAX_GHOSTS);
        assert!(store.bytes() < MAX_GHOSTS * 60_000);
        assert!(store.get("track39", 39 % 8).is_some(), "the newest are kept");
        assert!(store.get("track0", 0).is_none(), "the oldest are dropped");
        for bad in ["", "AAAA", "not base64!!", &text[..text.len() / 2]] {
            assert!(Ghost::decode(bad).is_none(), "{bad:?}");
        }
        let long = Ghost { total: MAX_TICKS + 1, ..g.clone() };
        assert!(!long.storable());
        let mut r = Recorder::new();
        r.record(MAX_TICKS + 10, Sample::default());
        assert!(r.finish(0, MAX_TICKS + 10).is_none());
    }

    #[test]
    fn the_store_keeps_only_a_faster_ghost_per_track_and_character() {
        let (rec, _) = drive(600);
        let slow = rec.finish(3, 3000).unwrap();
        let fast = Ghost { total: 2800, ..slow.clone() };
        let mut store = GhostStore::default();
        assert!(store.offer("luigi_raceway", &slow));
        assert!(!store.offer("luigi_raceway", &slow), "equal is not better");
        assert!(store.offer("luigi_raceway", &fast));
        assert!(!store.offer("luigi_raceway", &slow));
        assert_eq!(store.get("luigi_raceway", 3).unwrap().total, 2800);
        assert!(store.get("luigi_raceway", 4).is_none());
        assert_eq!(store.best_on("luigi_raceway").unwrap().total, 2800);
        let mut other = fast.clone();
        other.character = 4;
        other.total = 2700;
        assert!(store.offer("luigi_raceway", &other));
        assert_eq!(store.best_on("luigi_raceway").unwrap().character, 4);
        let back: GhostStore = serde_json::from_str(&serde_json::to_string(&store).unwrap()).unwrap();
        assert_eq!(back, store);
    }
}
