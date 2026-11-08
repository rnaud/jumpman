//! Level ambience: the sound emitters the decomp levels place as invisible objects.
//!
//! `bhvWaterfallSoundLoop`, `bhvSandSoundLoop`, `bhvVolcanoSoundLoop` and `bhvBirdsSoundLoop` request one sound every
//! frame at their position (`cur_obj_play_sound_1`), so the sound exists only while the player is within earshot and
//! the engine pans and attenuates it from the camera. The positions are the `OBJECT(...)` lines of
//! `levels/<course>/script.c` (the exported courses use the same world units). Castle grounds' three birds and its
//! ambient bed run inside the wasm engine (`music_start`), not here.

/// (course id, sound name, world position).
pub const EMITTERS: &[(&str, &str, [f64; 3])] = &[
    ("cotmc", "ENV_WATERFALL2", [0.0, 500.0, -7373.0]),
    ("cotmc", "ENV_WATERFALL2", [0.0, 500.0, 3584.0]),
    ("ssl", "ENV_MOVINGSAND", [1198.0, -133.0, 2396.0]),
    ("ssl", "ENV_MOVINGSAND", [7.0, 1229.0, -708.0]),
    ("ssl", "ENV_MOVINGSAND", [7.0, 4317.0, -708.0]),
    ("lll", "ENV_DRONING1", [-26.0, 103.0, -2649.0]),
    ("ttm", "ENV_WATERFALL2", [3295.0, -3692.0, 2928.0]),
    ("ttm", "ENV_WATERFALL2", [2004.0, -1580.0, 1283.0]),
    ("castle_grounds", "ENV_WATERFALL2", [-5812.0, 100.0, -5937.0]),
    ("castle_courtyard", "OBJ2_BIRD_CHIRP1", [-2700.0, 0.0, -1652.0]),
    ("castle_courtyard", "GENERAL2_BIRD_CHIRP2", [2700.0, 0.0, -1652.0]),
];

/// Emitters farther than this from Mario are inaudible in the game (volume reaches zero well before).
pub const HEARING_RANGE: f64 = 9000.0;

/// The emitters of `course` within earshot of `mario`, nearest first, at most four (the loop sound slots).
pub fn audible(course: &str, mario: [f64; 3]) -> Vec<(usize, &'static str, [f64; 3])> {
    let mut v: Vec<(f64, usize, &'static str, [f64; 3])> = EMITTERS
        .iter()
        .enumerate()
        .filter(|(_, e)| e.0 == course)
        .map(|(i, e)| (((e.2[0] - mario[0]).powi(2) + (e.2[1] - mario[1]).powi(2) + (e.2[2] - mario[2]).powi(2)).sqrt(), i, e.1, e.2))
        .filter(|e| e.0 < HEARING_RANGE)
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.into_iter().take(4).map(|(_, i, n, p)| (i, n, p)).collect()
}
