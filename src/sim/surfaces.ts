/**
 * Surface types and flags from Super Mario 64.
 * See include/surface_terrains.h in the decompilation.
 */

export const SURFACE_DEFAULT = 0x0000;
export const SURFACE_BURNING = 0x0001;
export const SURFACE_HANGABLE = 0x0005;
export const SURFACE_SLOW = 0x0009;
export const SURFACE_DEATH_PLANE = 0x000a;
export const SURFACE_VERY_SLIPPERY = 0x0013;
export const SURFACE_SLIPPERY = 0x0014;
export const SURFACE_NOT_SLIPPERY = 0x0015;
export const SURFACE_SHALLOW_QUICKSAND = 0x0021;
export const SURFACE_DEEP_QUICKSAND = 0x0022;
export const SURFACE_INSTANT_QUICKSAND = 0x0023;
export const SURFACE_DEEP_MOVING_QUICKSAND = 0x0024;
export const SURFACE_SHALLOW_MOVING_QUICKSAND = 0x0025;
export const SURFACE_QUICKSAND = 0x0026;
export const SURFACE_MOVING_QUICKSAND = 0x0027;
export const SURFACE_HORIZONTAL_WIND = 0x002c;
export const SURFACE_INSTANT_MOVING_QUICKSAND = 0x002d;
export const SURFACE_ICE = 0x002e;
export const SURFACE_HARD = 0x0030;
export const SURFACE_HARD_SLIPPERY = 0x0035;
export const SURFACE_HARD_VERY_SLIPPERY = 0x0036;
export const SURFACE_HARD_NOT_SLIPPERY = 0x0037;
export const SURFACE_VERTICAL_WIND = 0x0038;
export const SURFACE_NOISE_DEFAULT = 0x0029;
export const SURFACE_NOISE_SLIPPERY = 0x002a;
export const SURFACE_CAMERA_BOUNDARY = 0x0072;
export const SURFACE_NOISE_VERY_SLIPPERY_73 = 0x0073;
export const SURFACE_NOISE_VERY_SLIPPERY_74 = 0x0074;
export const SURFACE_NOISE_VERY_SLIPPERY = 0x0075;
export const SURFACE_NO_CAM_COL_VERY_SLIPPERY = 0x0078;
export const SURFACE_NO_CAM_COL_SLIPPERY = 0x0079;
export const SURFACE_SWITCH = 0x007a;
export const SURFACE_VANISH_CAP_WALLS = 0x007b;

export const SURFACE_CLASS_DEFAULT = 0x0000;
export const SURFACE_CLASS_VERY_SLIPPERY = 0x0013;
export const SURFACE_CLASS_SLIPPERY = 0x0014;
export const SURFACE_CLASS_NOT_SLIPPERY = 0x0015;

export const SURFACE_FLAG_DYNAMIC = 1 << 0;
export const SURFACE_FLAG_NO_CAM_COLLISION = 1 << 1;
export const SURFACE_FLAG_X_PROJECTION = 1 << 3;

export function isQuicksand(type: number): boolean {
  return type >= SURFACE_SHALLOW_QUICKSAND && type <= SURFACE_MOVING_QUICKSAND;
}

export function isSlopeSurface(type: number): boolean {
  return type === SURFACE_HORIZONTAL_WIND || type === SURFACE_VERTICAL_WIND;
}
