/**
 * Assorted Mario helpers ported from src/game/mario.c and
 * src/game/mario_actions_moving.c.
 */

import type { CollisionWorld } from './collision';
import { atan2s, sins, coss, toS16, type Vec3 } from './math';
import type { MarioState } from './state';
import { ACT_CRAWLING } from './state';
import {
  SURFACE_CLASS_DEFAULT,
  SURFACE_ICE,
  SURFACE_NOT_SLIPPERY,
  SURFACE_HARD_NOT_SLIPPERY,
  SURFACE_SWITCH,
  SURFACE_SLIPPERY,
  SURFACE_NOISE_SLIPPERY,
  SURFACE_HARD_SLIPPERY,
  SURFACE_NO_CAM_COL_SLIPPERY,
  SURFACE_VERY_SLIPPERY,
  SURFACE_HARD_VERY_SLIPPERY,
  SURFACE_NOISE_VERY_SLIPPERY_73,
  SURFACE_NOISE_VERY_SLIPPERY_74,
  SURFACE_NOISE_VERY_SLIPPERY,
  SURFACE_NO_CAM_COL_VERY_SLIPPERY,
  SURFACE_CLASS_VERY_SLIPPERY,
  SURFACE_CLASS_SLIPPERY,
  SURFACE_CLASS_NOT_SLIPPERY,
} from './surfaces';

/** The default terrain type for the playground. */
export const TERRAIN_DEFAULT = 0;
export const TERRAIN_SLIDE = 6;
export const TERRAIN_MASK = 7;

/**
 * Sets Mario's other velocities from his forward speed.
 * (mario_set_forward_vel)
 */
export function marioSetForwardVel(m: MarioState, forwardVel: number): void {
  m.forwardVel = forwardVel;
  m.slideVelX = sins(m.faceAngle[1]) * m.forwardVel;
  m.slideVelZ = coss(m.faceAngle[1]) * m.forwardVel;
  m.vel[0] = m.slideVelX;
  m.vel[2] = m.slideVelZ;
}

/** mario_get_floor_class */
export function marioGetFloorClass(m: MarioState): number {
  let floorClass = m.slideTerrain ? SURFACE_CLASS_VERY_SLIPPERY : SURFACE_CLASS_DEFAULT;

  if (m.floor) {
    switch (m.floor.type) {
      case SURFACE_NOT_SLIPPERY:
      case SURFACE_HARD_NOT_SLIPPERY:
      case SURFACE_SWITCH:
        floorClass = SURFACE_CLASS_NOT_SLIPPERY;
        break;

      case SURFACE_SLIPPERY:
      case SURFACE_NOISE_SLIPPERY:
      case SURFACE_HARD_SLIPPERY:
      case SURFACE_NO_CAM_COL_SLIPPERY:
        floorClass = SURFACE_CLASS_SLIPPERY;
        break;

      case SURFACE_VERY_SLIPPERY:
      case SURFACE_ICE:
      case SURFACE_HARD_VERY_SLIPPERY:
      case SURFACE_NOISE_VERY_SLIPPERY_73:
      case SURFACE_NOISE_VERY_SLIPPERY_74:
      case SURFACE_NOISE_VERY_SLIPPERY:
      case SURFACE_NO_CAM_COL_VERY_SLIPPERY:
        floorClass = SURFACE_CLASS_VERY_SLIPPERY;
        break;
    }
  }

  if (
    m.action === ACT_CRAWLING &&
    m.floor &&
    m.floor.normal[1] > 0.5 &&
    floorClass === SURFACE_CLASS_DEFAULT
  ) {
    floorClass = SURFACE_CLASS_NOT_SLIPPERY;
  }

  return floorClass;
}

/** mario_facing_downhill */
export function marioFacingDownhill(m: MarioState, turnYaw: boolean): boolean {
  let faceAngleYaw = m.faceAngle[1];

  if (turnYaw && m.forwardVel < 0) {
    faceAngleYaw = toS16(faceAngleYaw + 0x8000);
  }

  faceAngleYaw = toS16(m.floorAngle - faceAngleYaw);
  return -0x4000 < faceAngleYaw && faceAngleYaw < 0x4000;
}

/** mario_floor_is_slippery */
export function marioFloorIsSlippery(m: MarioState): boolean {
  if (!m.floor) return false;
  if (m.slideTerrain && m.floor.normal[1] < 0.9998477) return true;

  let normY: number;
  switch (marioGetFloorClass(m)) {
    case SURFACE_CLASS_VERY_SLIPPERY:
      normY = 0.9848077;
      break;
    case SURFACE_CLASS_SLIPPERY:
      normY = 0.9396926;
      break;
    case SURFACE_CLASS_NOT_SLIPPERY:
      normY = 0;
      break;
    default:
      normY = 0.7880108;
      break;
  }

  return m.floor.normal[1] <= normY;
}

/** mario_floor_is_slope */
export function marioFloorIsSlope(m: MarioState): boolean {
  if (!m.floor) return false;
  if (m.slideTerrain && m.floor.normal[1] < 0.9998477) return true;

  let normY: number;
  switch (marioGetFloorClass(m)) {
    case SURFACE_CLASS_VERY_SLIPPERY:
      normY = 0.9961947;
      break;
    case SURFACE_CLASS_SLIPPERY:
      normY = 0.9848077;
      break;
    case SURFACE_CLASS_NOT_SLIPPERY:
      normY = 0.9396926;
      break;
    default:
      normY = 0.9659258;
      break;
  }

  return m.floor.normal[1] <= normY;
}

/** mario_floor_is_steep */
export function marioFloorIsSteep(m: MarioState): boolean {
  if (!m.floor) return false;
  if (marioFacingDownhill(m, false)) return false;

  let normY: number;
  switch (marioGetFloorClass(m)) {
    case SURFACE_CLASS_VERY_SLIPPERY:
      normY = 0.9659258;
      break;
    case SURFACE_CLASS_SLIPPERY:
      normY = 0.9396926;
      break;
    default:
      normY = 0.8660254;
      break;
    case SURFACE_CLASS_NOT_SLIPPERY:
      normY = 0.8660254;
      break;
  }

  return m.floor.normal[1] <= normY;
}

/** find_floor_height_relative_polar */
export function findFloorHeightRelativePolar(
  m: MarioState,
  world: CollisionWorld,
  angleFromMario: number,
  distFromMario: number,
): number {
  const a = toS16(m.faceAngle[1] + angleFromMario);
  const y = sins(a) * distFromMario;
  const x = coss(a) * distFromMario;
  return world.findFloor(m.pos[0] + y, m.pos[1] + 100, m.pos[2] + x).height;
}

/** find_floor_slope */
export function findFloorSlope(m: MarioState, world: CollisionWorld, yawOffset: number): number {
  const a = toS16(m.faceAngle[1] + yawOffset);
  const x = sins(a) * 5;
  const z = coss(a) * 5;

  const forwardFloorY = world.findFloor(m.pos[0] + x, m.pos[1] + 100, m.pos[2] + z).height;
  const backwardFloorY = world.findFloor(m.pos[0] - x, m.pos[1] + 100, m.pos[2] - z).height;

  const forwardYDelta = forwardFloorY - m.pos[1];
  const backwardYDelta = m.pos[1] - backwardFloorY;

  if (forwardYDelta * forwardYDelta < backwardYDelta * backwardYDelta) {
    return atan2s(5, forwardYDelta);
  }
  return atan2s(5, backwardYDelta);
}

export function vec3fCopy(dest: Vec3, src: readonly number[]): void {
  dest[0] = src[0];
  dest[1] = src[1];
  dest[2] = src[2];
}
