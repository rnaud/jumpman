//! Loaders for the pre-extracted Link (`public/models/oot/*.json`) and Banjo
//! (`public/models/bk/banjo.json`) character assets, shared with the web build.
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize, Clone, Debug)]
pub struct TextureSpec {
    pub width: u32,
    pub height: u32,
    /// Base64 RGBA8.
    pub rgba: String,
}

impl TextureSpec {
    pub fn bytes(&self) -> Vec<u8> {
        base64_decode(&self.rgba)
    }
}

/// Minimal standard-alphabet base64 decoder (padding and whitespace tolerated).
pub fn base64_decode(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

#[derive(Deserialize, Clone, Debug)]
pub struct Limb {
    pub pos: [f64; 3],
    pub child: usize,
    pub sibling: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LinkBatch {
    pub mode: String,
    pub texture: Option<String>,
    pub color: u32,
    pub wrap: [usize; 2],
    /// `[x, y, z, s, t, nx, ny, nz, bone]`
    pub vertices: Vec<[f64; 9]>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LinkAsset {
    pub limbs: Vec<Limb>,
    pub textures: HashMap<String, TextureSpec>,
    pub batches: Vec<LinkBatch>,
    /// Per clip, per frame: `[x, y, z, rot(3 * limbs)...]` (s16 angles).
    pub clips: HashMap<String, Vec<Vec<f64>>>,
}

/// Epona (`scripts/export-oot-epona.py`): skinned batches, nine animations, Link's riding clips.
#[derive(Deserialize, Clone, Debug)]
pub struct EponaBatch {
    pub texture: Option<String>,
    pub color: u32,
    pub wrap: [usize; 2],
    /// `[s, t, nx, ny, nz, normal limb, n, (limb, x, y, z, weight) * n]`
    pub vertices: Vec<Vec<f64>>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EponaAsset {
    pub limbs: Vec<Limb>,
    pub textures: HashMap<String, TextureSpec>,
    pub batches: Vec<EponaBatch>,
    pub clips: HashMap<String, Vec<Vec<f64>>>,
    pub rider_clips: HashMap<String, Vec<Vec<f64>>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BanjoBone {
    pub pivot: [f64; 3],
    pub id: i64,
    pub parent: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BanjoBatch {
    pub part: String,
    pub texture: Option<String>,
    pub wrap: [usize; 2],
    /// `[x, y, z, u, v, r, g, b, a, bone]`
    pub vertices: Vec<[f64; 10]>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct BanjoClip {
    pub start: f64,
    pub end: f64,
    pub duration: f64,
    /// `[bone id, channel, [flags, frame, value*64]...]` (flat triples).
    pub elements: Vec<(i64, usize, Vec<f64>)>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BanjoAsset {
    pub trans_scale: f64,
    pub bones: Vec<BanjoBone>,
    pub textures: HashMap<String, TextureSpec>,
    pub batches: Vec<BanjoBatch>,
    pub clips: HashMap<String, BanjoClip>,
}

fn read<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The web build keeps these under `public/models`; the native bundle may copy them next to the
/// other assets (`<assets>/models`). Search both.
fn find(root: &Path, rel: &str) -> std::path::PathBuf {
    let under_assets = root.join("models").join(rel);
    if under_assets.is_file() {
        return under_assets;
    }
    root.join("..").join("public/models").join(rel)
}

/// One of the static OoT item models (`public/models/oot/items/<name>.json`, `scripts/export-oot-items.py`): a one-bone Link-format asset.
pub fn load_item(root: &Path, name: &str) -> Result<LinkAsset, String> {
    read(&find(root, &format!("oot/items/{name}.json")))
}

/// The clips `scripts/export-oot-link.py` gained with the z_player.c sword animation pass.
pub const NEW_CLIPS: &[&str] = &[
    "slashEndZ", "slashREnd", "slashREndZ", "slashLEnd", "slashLEndZ", "stabEnd", "stabEndZ", "finishEndZ", "slashRFinishEnd", "slashRFinishEndZ", "slashLFinishEnd",
    "slashLFinishEndZ", "stabFinishEnd", "stabFinishEndZ", "spinEnd", "spinEndZ", "bigSpinEnd", "hammerEnd", "hammerSideEnd", "jumpSlashHit", "jumpSlashEnd",
    "flipSlashHit", "flipSlashEnd", "swordIdle", "swordWalk", "swordRun",
];

pub fn load_link(root: &Path, age: &str) -> Result<LinkAsset, String> {
    let mut asset: LinkAsset = read(&find(root, &format!("oot/{age}.json")))?;
    fix_root_heights(&mut asset, age == "young-link");
    // Test hook `JUMPMAN_LINK_OLD_CLIPS=1`: the data of a device that has not re-imported yet, without the 26 clips added with the
    // `_end` recoveries, landing blows and drawn-sword stance (the swing logic must not depend on them).
    if std::env::var_os("JUMPMAN_LINK_OLD_CLIPS").is_some() {
        for name in NEW_CLIPS {
            asset.clips.remove(*name);
        }
    }
    Ok(asset)
}

/// The exported `fall` (`normal_fall_wait`) and `hang` clips carry a root height of about -3300 / -3050 model units
/// where every standing, jumping and landing clip has +2200..+3700: the hip is the same distance *below* the model origin
/// instead of above it, so the body sank some 300 course units under the floor the moment the jump arc switched to the
/// long-fall clip (vy below -14), which is the last part of every descent. A hip below the origin is never right for
/// Link (the lowest legitimate root height among the clips is about 2100), so mirror it.
///
/// The mirrored `fall` pose is then lowered or raised so its lowest vertex sits where the idle pose's does, i.e. the soles are at
/// the sim position (the exported hip height is a different animation's baseline and left the feet ~25 units low).
fn fix_root_heights(asset: &mut LinkAsset, young: bool) {
    for frames in asset.clips.values_mut() {
        for f in frames.iter_mut() {
            if f.len() > 3 && f[1] < 0.0 {
                f[1] = -f[1];
            }
        }
    }
    let lowest = |asset: &LinkAsset, clip: &str| -> Option<f32> {
        let frames = asset.clips.get(clip)?;
        let bones = super::link_pose::pose_bones(asset, frames, 0.0, young, false, false);
        asset
            .batches
            .iter()
            .filter(|b| b.mode == "base")
            .flat_map(|b| b.vertices.iter())
            .map(|v| bones[(v[8] as usize).min(bones.len() - 1)].transform_point3(bevy::math::Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32)).y)
            .reduce(f32::min)
    };
    if let (Some(idle), Some(fall)) = (lowest(asset, "idle"), lowest(asset, "fall")) {
        let lift = (idle - fall) as f64 / if young { 0.64 } else { 1.0 };
        for f in asset.clips.get_mut("fall").into_iter().flatten() {
            f[1] += lift;
        }
    }
}

pub fn load_epona(root: &Path) -> Result<EponaAsset, String> {
    read(&find(root, "oot/epona.json"))
}

pub fn load_banjo(root: &Path) -> Result<BanjoAsset, String> {
    let mut asset: BanjoAsset = read(&find(root, "bk/banjo.json"))?;
    // Clips for the extended moveset (`scripts/export-bk-banjo.py --extras`); optional.
    let extra = find(root, "bk/banjo-extra.json");
    if extra.is_file() {
        #[derive(Deserialize)]
        struct Extra {
            clips: HashMap<String, BanjoClip>,
        }
        let e: Extra = read(&extra)?;
        asset.clips.extend(e.clips);
    }
    Ok(asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        assert_eq!(base64_decode("aGVsbG8="), b"hello");
        assert_eq!(base64_decode("AAEC/w=="), vec![0, 1, 2, 255]);
    }

    fn assets() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    #[test]
    fn real_assets_parse() {
        for age in ["link", "young-link"] {
            let a = load_link(&assets(), age).unwrap();
            assert_eq!(a.limbs.len(), 21);
            assert!(a.clips.contains_key("idle"));
            for b in &a.batches {
                assert_eq!(b.vertices.len() % 3, 0);
                if let Some(t) = &b.texture {
                    let spec = &a.textures[t];
                    assert_eq!(spec.bytes().len() as u32, spec.width * spec.height * 4);
                }
            }
        }
        let e = load_epona(&assets()).unwrap();
        assert_eq!(e.limbs.len(), 47);
        for b in &e.batches {
            assert_eq!(b.vertices.len() % 3, 0);
            for v in &b.vertices {
                assert_eq!(v.len(), 7 + 5 * v[6] as usize);
            }
            if let Some(t) = &b.texture {
                assert!(e.textures.contains_key(t));
            }
        }
        for (i, name) in ["idle", "whinny", "refuse", "rearing", "walk", "trot", "gallop", "jump", "jumpHigh"].iter().enumerate() {
            assert_eq!(e.clips[*name].len() as f64, jumpman_sim::link::epona::ANIM_LEN[i], "{name}");
        }
        assert!(e.rider_clips.contains_key("fastrun") && e.rider_clips.contains_key("mountLeft"));
        let b = load_banjo(&assets()).unwrap();
        assert_eq!(b.bones.len(), 63);
        assert!(b.clips.contains_key("idle"));
    }
}
