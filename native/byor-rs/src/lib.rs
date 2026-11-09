//! Pure-Rust BYOR import pipeline (the Android twin of `native/byor/pipeline.py`; see `native/ANDROID_IMPORT.md`).
pub mod bk;
pub mod cli;
pub mod courses;
mod courses_g2;
mod courses_obj;
pub mod courses_post;
pub mod pyc;
pub mod sm64_world;
pub mod hash;
pub mod mk64;
pub mod music;
pub mod models;
pub mod n64;
pub mod oot;
pub mod pipeline;
pub mod png;
pub mod rom;
pub mod sm64;
pub mod sm64_import;
pub mod sound;
pub mod sound_banks;
pub mod zpng;

/// Worker threads for the parallel exporters: the machine's parallelism, at most 4 on iOS (heat, memory), overridable with
/// `BYOR_THREADS` (1 gives the single-threaded reference run the tests compare against).
pub fn worker_threads() -> usize {
    if let Some(n) = std::env::var("BYOR_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0) {
        return n;
    }
    let n = std::thread::available_parallelism().map_or(1, usize::from);
    if cfg!(target_os = "ios") { n.min(4) } else { n.min(8) }
}
