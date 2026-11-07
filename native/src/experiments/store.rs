//! Tiny persistent key/value store (`~/.jumpman/experiments.json`, or `JUMPMAN_STORE`) standing
//! in for the TS `localStorage` (best scores, ghosts). Failures are ignored: storage is optional.
use serde_json::{Map, Value};
use std::path::PathBuf;

fn path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JUMPMAN_STORE") {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".jumpman").join("experiments.json"))
}

fn load() -> Map<String, Value> {
    path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn get(key: &str) -> Option<Value> {
    load().get(key).cloned()
}

pub fn get_f64(key: &str) -> Option<f64> {
    get(key).and_then(|v| v.as_f64())
}

pub fn set(key: &str, value: Value) {
    let Some(p) = path() else { return };
    let mut map = load();
    map.insert(key.to_string(), value);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, serde_json::to_string(&map).unwrap_or_default());
}
