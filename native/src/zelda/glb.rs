//! Tiny glb JSON-chunk reader: animation names (index = Bevy `GltfAssetLabel::Animation`) and
//! the mesh bounds (`Box3.setFromObject` for NPC portrait framing), returned in SM64 units
//! (glb units divided by the exporter's `sm64UnitsToMeters`).
use serde_json::Value;
use std::io::Read;
use std::path::Path;

pub fn read_json(path: &Path) -> Option<Value> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 20];
    file.read_exact(&mut head).ok()?;
    if &head[0..4] != b"glTF" {
        return None;
    }
    let len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let mut json = vec![0u8; len];
    file.read_exact(&mut json).ok()?;
    serde_json::from_slice(&json).ok()
}

/// Bevy scale for a glb: the exporter's `sm64UnitsToMeters` (default 0.01) says how many metres
/// one glb unit is worth; the Bevy world is 0.01 per SM64 unit. SM64 actors/levels come out as 1,
/// the Zelda assets (`sm64UnitsToMeters: 1`, authored in SM64 units) as 0.01.
pub fn bevy_scale(path: &Path) -> f32 {
    let meters = read_json(path).and_then(|j| j["asset"]["extras"]["sm64UnitsToMeters"].as_f64()).unwrap_or(0.01);
    if meters > 0.0 {
        (0.01 / meters) as f32
    } else {
        1.0
    }
}

pub fn animation_names(path: &Path) -> Vec<String> {
    read_json(path)
        .and_then(|v| v["animations"].as_array().map(|a| a.iter().enumerate().map(|(i, x)| x["name"].as_str().map_or(format!("anim{i}"), str::to_string)).collect()))
        .unwrap_or_default()
}

fn vec3(v: &Value, default: [f64; 3]) -> [f64; 3] {
    match v.as_array() {
        Some(a) if a.len() >= 3 => [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0), a[2].as_f64().unwrap_or(0.0)],
        _ => default,
    }
}

/// (min, max) of every mesh in the default scene in SM64 units; node translation and scale are
/// applied, rotation is ignored (the NPC rigs are posed upright).
pub fn bounds(path: &Path) -> Option<([f64; 3], [f64; 3])> {
    let json = read_json(path)?;
    let nodes = json["nodes"].as_array()?;
    let scene = json["scene"].as_u64().unwrap_or(0) as usize;
    let roots: Vec<usize> = json["scenes"][scene]["nodes"].as_array()?.iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut stack: Vec<(usize, [f64; 3], [f64; 3])> = roots.into_iter().map(|r| (r, [1.0; 3], [0.0; 3])).collect();
    while let Some((i, ps, pt)) = stack.pop() {
        let node = nodes.get(i)?;
        let s = vec3(&node["scale"], [1.0; 3]);
        let t = vec3(&node["translation"], [0.0; 3]);
        let scale = [ps[0] * s[0], ps[1] * s[1], ps[2] * s[2]];
        let trans = [pt[0] + ps[0] * t[0], pt[1] + ps[1] * t[1], pt[2] + ps[2] * t[2]];
        if let Some(mesh) = node["mesh"].as_u64() {
            for prim in json["meshes"][mesh as usize]["primitives"].as_array()? {
                let acc = &json["accessors"][prim["attributes"]["POSITION"].as_u64()? as usize];
                let (lo, hi) = (vec3(&acc["min"], [0.0; 3]), vec3(&acc["max"], [0.0; 3]));
                for k in 0..3 {
                    let (a, b) = (lo[k] * scale[k] + trans[k], hi[k] * scale[k] + trans[k]);
                    min[k] = min[k].min(a.min(b));
                    max[k] = max[k].max(a.max(b));
                }
            }
        }
        for c in node["children"].as_array().into_iter().flatten() {
            stack.push((c.as_u64()? as usize, scale, trans));
        }
    }
    if !min[0].is_finite() {
        return None;
    }
    // Skinned rigs: the accessors only cover the bind-pose parts; frame the whole body from the
    // root joint's height instead.
    if let Some(joint) = json["skins"][0]["joints"][0].as_u64().and_then(|j| nodes.get(j as usize)) {
        let h = vec3(&joint["translation"], [0.0; 3])[1] * 1.5;
        min[1] = 0.0;
        max[1] = max[1].max(h);
    }
    let meters = json["asset"]["extras"]["sm64UnitsToMeters"].as_f64().filter(|m| *m > 0.0).unwrap_or(0.01);
    Some((min.map(|v| v / meters), max.map(|v| v / meters)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn mido_has_clips_and_a_child_sized_box() {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/levels/kokiri_forest/mido.glb");
        let names = animation_names(&p);
        assert!(names.iter().any(|n| n == "Idle"), "{names:?}");
        let (lo, hi) = bounds(&p).unwrap();
        let h = hi[1] - lo[1];
        assert!(h > 60.0 && h < 400.0, "height {h}");
    }
}
