//! Port of `terrainSound.ts` (sTerrainSounds / mario_get_terrain_sound_addend).
use jumpman_sim::surfaces as S;

const TERRAIN_SOUNDS: [[i32; 6]; 7] = [
    [0, 3, 1, 1, 1, 0],
    [3, 3, 3, 3, 1, 1],
    [5, 6, 5, 6, 3, 3],
    [7, 3, 7, 7, 3, 3],
    [4, 4, 4, 4, 3, 3],
    [0, 3, 1, 6, 3, 6],
    [3, 3, 3, 3, 6, 6],
];

pub fn terrain_sound(terrain: usize, surface: Option<i32>, wet: bool) -> i32 {
    let Some(surface) = surface else { return 0 };
    if wet {
        return 2;
    }
    if S::is_quicksand(surface) {
        return 7;
    }
    let column = match surface {
        S::SURFACE_NOT_SLIPPERY | S::SURFACE_HARD | S::SURFACE_HARD_NOT_SLIPPERY | S::SURFACE_SWITCH => 1,
        S::SURFACE_SLIPPERY | S::SURFACE_HARD_SLIPPERY | S::SURFACE_NO_CAM_COL_SLIPPERY => 2,
        S::SURFACE_ICE
        | S::SURFACE_VERY_SLIPPERY
        | S::SURFACE_HARD_VERY_SLIPPERY
        | S::SURFACE_NOISE_VERY_SLIPPERY_73
        | S::SURFACE_NOISE_VERY_SLIPPERY_74
        | S::SURFACE_NOISE_VERY_SLIPPERY
        | S::SURFACE_NO_CAM_COL_VERY_SLIPPERY => 3,
        S::SURFACE_NOISE_DEFAULT => 4,
        S::SURFACE_NOISE_SLIPPERY => 5,
        _ => 0,
    };
    TERRAIN_SOUNDS[terrain.min(6)][column]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_lookup() {
        assert_eq!(terrain_sound(0, None, false), 0);
        assert_eq!(terrain_sound(2, Some(0), false), 5); // snow, default surface
        assert_eq!(terrain_sound(0, Some(S::SURFACE_HARD), false), 3);
        assert_eq!(terrain_sound(0, Some(S::SURFACE_HARD), true), 2);
        assert_eq!(terrain_sound(0, Some(S::SURFACE_DEEP_QUICKSAND), false), 7);
        assert_eq!(terrain_sound(3, Some(S::SURFACE_ICE), false), 7);
    }
}
