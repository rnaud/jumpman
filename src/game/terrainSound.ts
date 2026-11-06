import * as S from '../sim/surfaces';

// sTerrainSounds and mario_get_terrain_sound_addend (mario.c).
const terrainSounds = [
  [0,3,1,1,1,0], [3,3,3,3,1,1], [5,6,5,6,3,3], [7,3,7,7,3,3],
  [4,4,4,4,3,3], [0,3,1,6,3,6], [3,3,3,3,6,6],
];
export function terrainSound(terrain: number, surface: number | undefined, wet: boolean): number {
  if (surface === undefined) return 0;
  if (wet) return 2;
  if (S.isQuicksand(surface)) return 7;
  let column = 0;
  switch (surface) {
    case S.SURFACE_NOT_SLIPPERY: case S.SURFACE_HARD: case S.SURFACE_HARD_NOT_SLIPPERY: case S.SURFACE_SWITCH:
      column = 1; break;
    case S.SURFACE_SLIPPERY: case S.SURFACE_HARD_SLIPPERY: case S.SURFACE_NO_CAM_COL_SLIPPERY:
      column = 2; break;
    case S.SURFACE_ICE: case S.SURFACE_VERY_SLIPPERY: case S.SURFACE_HARD_VERY_SLIPPERY:
    case S.SURFACE_NOISE_VERY_SLIPPERY_73: case S.SURFACE_NOISE_VERY_SLIPPERY_74:
    case S.SURFACE_NOISE_VERY_SLIPPERY: case S.SURFACE_NO_CAM_COL_VERY_SLIPPERY:
      column = 3; break;
    case S.SURFACE_NOISE_DEFAULT: column = 4; break;
    case S.SURFACE_NOISE_SLIPPERY: column = 5; break;
  }
  return terrainSounds[terrain][column];
}
