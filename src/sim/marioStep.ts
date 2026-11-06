/**
 * Ground and air movement integration, ported from src/game/mario_step.c.
 *
 * Mario moves in four quarter-steps per 30 Hz frame.  Walls are resolved on the
 * *destination* position before floor height is sampled, and floors within 100
 * units above Mario are snapped to rather than treated as a fall.
 */

import type { CollisionWorld, Surface } from './collision';
import { atan2s, toS16, type Vec3 } from './math';
import { marioSetForwardVel } from './marioHelpers';
import {
  ACT_FLAG_CONTROL_JUMP_HEIGHT,
  ACT_FLAG_INTANGIBLE,
  ACT_FLAG_INVULNERABLE,
  ACT_SHOT_FROM_CANNON, ACT_FLYING,
  ACT_GETTING_BLOWN,
  ACT_LAVA_BOOST,
  ACT_LONG_JUMP,
  ACT_SLIDE_KICK,
  AIR_STEP_CHECK_HANG,
  AIR_STEP_CHECK_LEDGE_GRAB,
  AIR_STEP_GRABBED_CEILING,
  AIR_STEP_GRABBED_LEDGE,
  AIR_STEP_HIT_LAVA_WALL,
  AIR_STEP_HIT_WALL,
  AIR_STEP_LANDED,
  AIR_STEP_NONE,
  GROUND_STEP_HIT_WALL,
  GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS,
  GROUND_STEP_HIT_WALL_STOP_QSTEPS,
  GROUND_STEP_LEFT_GROUND,
  GROUND_STEP_NONE,
  INPUT_A_DOWN,
  MARIO_UNKNOWN_08,
  MARIO_WING_CAP, MARIO_METAL_CAP,
  type MarioState,
} from './state';
import { SURFACE_HANGABLE } from './surfaces';

/** mario_set_forward_vel */
export { marioSetForwardVel };

/** stop_and_set_height_to_floor */
export function stopAndSetHeightToFloor(m: MarioState): void {
  marioSetForwardVel(m, 0);
  m.vel[1] = 0;
  m.pos[1] = m.floorHeight;
}

/** stationary_ground_step */
export function stationaryGroundStep(m: MarioState, world: CollisionWorld): number {
  marioSetForwardVel(m, 0);
  m.pos[1] = m.floorHeight;
  return GROUND_STEP_NONE;
}

/**
 * perform_ground_quarter_step
 */
function performGroundQuarterStep(
  m: MarioState,
  world: CollisionWorld,
  nextPos: Vec3,
): number {
  world.resolveAndReturnWallCollisions(nextPos, 30, 24);
  const upperWall = world.resolveAndReturnWallCollisions(nextPos, 60, 50);

  const floorHit = world.findFloor(nextPos[0], nextPos[1], nextPos[2]);
  const floorHeight = floorHit.height;
  const floor = floorHit.surface;
  const ceilHit = world.vec3fFindCeil(nextPos, floorHeight);
  const ceilHeight = ceilHit.height;

  m.wall = upperWall;

  if (floor === null) {
    return GROUND_STEP_HIT_WALL_STOP_QSTEPS;
  }

  if (nextPos[1] > floorHeight + 100) {
    if (nextPos[1] + 160 >= ceilHeight) {
      return GROUND_STEP_HIT_WALL_STOP_QSTEPS;
    }

    m.pos[0] = nextPos[0];
    m.pos[1] = nextPos[1];
    m.pos[2] = nextPos[2];
    m.floor = floor;
    m.floorHeight = floorHeight;
    return GROUND_STEP_LEFT_GROUND;
  }

  if (floorHeight + 160 >= ceilHeight) {
    return GROUND_STEP_HIT_WALL_STOP_QSTEPS;
  }

  m.pos[0] = nextPos[0];
  m.pos[1] = floorHeight;
  m.pos[2] = nextPos[2];
  m.floor = floor;
  m.floorHeight = floorHeight;

  if (upperWall !== null) {
    const wallDYaw = toS16(atan2s(upperWall.normal[2], upperWall.normal[0]) - m.faceAngle[1]);

    if (wallDYaw >= 0x2aaa && wallDYaw <= 0x5555) {
      return GROUND_STEP_NONE;
    }
    if (wallDYaw <= -0x2aaa && wallDYaw >= -0x5555) {
      return GROUND_STEP_NONE;
    }

    return GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS;
  }

  return GROUND_STEP_NONE;
}

/** perform_ground_step */
export function performGroundStep(m: MarioState, world: CollisionWorld): number {
  let stepResult = GROUND_STEP_NONE;
  const intendedPos: Vec3 = [0, 0, 0];

  for (let i = 0; i < 4; i++) {
    const normalY = m.floor ? m.floor.normal[1] : 1;
    intendedPos[0] = m.pos[0] + normalY * (m.vel[0] / 4);
    intendedPos[2] = m.pos[2] + normalY * (m.vel[2] / 4);
    intendedPos[1] = m.pos[1];

    stepResult = performGroundQuarterStep(m, world, intendedPos);
    if (
      stepResult === GROUND_STEP_LEFT_GROUND ||
      stepResult === GROUND_STEP_HIT_WALL_STOP_QSTEPS
    ) {
      break;
    }
  }

  if (stepResult === GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS) {
    stepResult = GROUND_STEP_HIT_WALL;
  }
  return stepResult;
}

/**
 * check_ledge_grab
 */
export function checkLedgeGrab(
  m: MarioState,
  world: CollisionWorld,
  wall: Surface,
  intendedPos: Vec3,
  nextPos: Vec3,
): boolean {
  if (m.vel[1] > 0) return false;

  const displacementX = nextPos[0] - intendedPos[0];
  const displacementZ = nextPos[2] - intendedPos[2];

  if (displacementX * m.vel[0] + displacementZ * m.vel[2] > 0) return false;

  const ledgePos: Vec3 = [
    nextPos[0] - wall.normal[0] * 60,
    0,
    nextPos[2] - wall.normal[2] * 60,
  ];
  const ledgeHit = world.findFloor(ledgePos[0], nextPos[1] + 160, ledgePos[2]);
  ledgePos[1] = ledgeHit.height;

  if (ledgePos[1] - nextPos[1] <= 100) return false;
  if (!ledgeHit.surface) return false;

  m.pos[0] = ledgePos[0];
  m.pos[1] = ledgePos[1];
  m.pos[2] = ledgePos[2];
  m.floor = ledgeHit.surface;
  m.floorHeight = ledgePos[1];
  m.floorAngle = atan2s(ledgeHit.surface.normal[2], ledgeHit.surface.normal[0]);

  m.faceAngle[0] = 0;
  m.faceAngle[1] = toS16(atan2s(wall.normal[2], wall.normal[0]) + 0x8000);
  return true;
}

/** perform_air_quarter_step */
function performAirQuarterStep(
  m: MarioState,
  world: CollisionWorld,
  intendedPos: Vec3,
  stepArg: number,
): number {
  const nextPos: Vec3 = [intendedPos[0], intendedPos[1], intendedPos[2]];

  const upperWall = world.resolveAndReturnWallCollisions(nextPos, 150, 50);
  const lowerWall = world.resolveAndReturnWallCollisions(nextPos, 30, 50);

  const floorHit = world.findFloor(nextPos[0], nextPos[1], nextPos[2]);
  let floorHeight = floorHit.height;
  const floor = floorHit.surface;
  const ceilHit = world.vec3fFindCeil(nextPos, floorHeight);
  const ceilHeight = ceilHit.height;

  m.wall = null;

  if (floor === null) {
    if (nextPos[1] <= m.floorHeight) {
      m.pos[1] = m.floorHeight;
      return AIR_STEP_LANDED;
    }
    m.pos[1] = nextPos[1];
    return AIR_STEP_HIT_WALL;
  }

  if (nextPos[1] <= floorHeight) {
    if (ceilHeight - floorHeight > 160) {
      m.pos[0] = nextPos[0];
      m.pos[2] = nextPos[2];
      m.floor = floor;
      m.floorHeight = floorHeight;
    }
    m.pos[1] = floorHeight;
    return AIR_STEP_LANDED;
  }

  if (nextPos[1] + 160 > ceilHeight) {
    if (m.vel[1] >= 0) {
      m.vel[1] = 0;
      if (stepArg & AIR_STEP_CHECK_HANG && m.ceil && m.ceil.type === SURFACE_HANGABLE) {
        return AIR_STEP_GRABBED_CEILING;
      }
      return AIR_STEP_NONE;
    }

    if (nextPos[1] <= m.floorHeight) {
      m.pos[1] = m.floorHeight;
      return AIR_STEP_LANDED;
    }

    m.pos[1] = nextPos[1];
    return AIR_STEP_HIT_WALL;
  }

  if (stepArg & AIR_STEP_CHECK_LEDGE_GRAB && upperWall === null && lowerWall !== null) {
    if (checkLedgeGrab(m, world, lowerWall, intendedPos, nextPos)) {
      return AIR_STEP_GRABBED_LEDGE;
    }

    m.pos[0] = nextPos[0];
    m.pos[1] = nextPos[1];
    m.pos[2] = nextPos[2];
    m.floor = floor;
    m.floorHeight = floorHeight;
    return AIR_STEP_NONE;
  }

  m.pos[0] = nextPos[0];
  m.pos[1] = nextPos[1];
  m.pos[2] = nextPos[2];
  m.floor = floor;
  m.floorHeight = floorHeight;

  if (upperWall !== null || lowerWall !== null) {
    const wall = upperWall !== null ? upperWall : lowerWall;
    m.wall = wall;
    const wallDYaw = toS16(atan2s(wall!.normal[2], wall!.normal[0]) - m.faceAngle[1]);

    if (wall!.type === 0x0001) {
      return AIR_STEP_HIT_LAVA_WALL;
    }
    if (wallDYaw < -0x6000 || wallDYaw > 0x6000) {
      return AIR_STEP_HIT_WALL;
    }
  }

  return AIR_STEP_NONE;
}

/** should_strengthen_gravity_for_jump_ascent */
export function shouldStrengthenGravityForJumpAscent(m: MarioState): boolean {
  if (!(m.flags & MARIO_UNKNOWN_08)) return false;
  if (m.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE)) return false;
  if (!(m.input & INPUT_A_DOWN) && m.vel[1] > 20) {
    return (m.action & ACT_FLAG_CONTROL_JUMP_HEIGHT) !== 0;
  }
  return false;
}

/** apply_gravity */
export function applyGravity(m: MarioState): void {
  const scale = (m.gravityScale ?? 1) * (m.flags & MARIO_METAL_CAP ? 1.25 : 1);
  if (m.action === ACT_LONG_JUMP || m.action === ACT_SLIDE_KICK) {
    m.vel[1] -= 2 * scale;
    if (m.vel[1] < -75) m.vel[1] = -75;
  } else if (m.action === ACT_SHOT_FROM_CANNON) {
    m.vel[1] = Math.max(-75,m.vel[1] - 1);
  } else if (m.action === ACT_LAVA_BOOST) {
    m.vel[1] -= 3.2 * scale;
    if (m.vel[1] < -65) m.vel[1] = -65;
  } else if (m.action === ACT_GETTING_BLOWN) {
    m.vel[1] -= 3.2 * scale;
    if (m.vel[1] < -75) m.vel[1] = -75;
  } else if (shouldStrengthenGravityForJumpAscent(m)) {
    m.vel[1] /= 4;
  } else if ((m.flags & MARIO_WING_CAP) && m.vel[1] < 0 && (m.input & INPUT_A_DOWN)) {
    m.vel[1] -= 2;
    if (m.vel[1] < -37.5) m.vel[1] = Math.min(-37.5, m.vel[1] + 4);
  } else {
    m.vel[1] -= 4 * scale;
    if (m.vel[1] < -75) m.vel[1] = -75;
  }
}

/** perform_air_step */
export function performAirStep(m: MarioState, world: CollisionWorld, stepArg: number): number {
  const intendedPos: Vec3 = [0, 0, 0];
  let stepResult = AIR_STEP_NONE;

  m.wall = null;

  for (let i = 0; i < 4; i++) {
    intendedPos[0] = m.pos[0] + m.vel[0] / 4;
    intendedPos[1] = m.pos[1] + m.vel[1] / 4;
    intendedPos[2] = m.pos[2] + m.vel[2] / 4;

    const quarterStepResult = performAirQuarterStep(m, world, intendedPos, stepArg);

    if (quarterStepResult !== AIR_STEP_NONE) {
      stepResult = quarterStepResult;
    }

    if (
      quarterStepResult === AIR_STEP_LANDED ||
      quarterStepResult === AIR_STEP_GRABBED_LEDGE ||
      quarterStepResult === AIR_STEP_GRABBED_CEILING ||
      quarterStepResult === AIR_STEP_HIT_LAVA_WALL
    ) {
      break;
    }
  }

  if (m.vel[1] >= 0) {
    m.peakHeight = m.pos[1];
  }

  if (m.action !== ACT_FLYING) {
    applyGravity(m);
  }

  return stepResult;
}

/** mario_bonk_reflection, minus the sound. */
export function bonkReflection(m: MarioState, negateSpeed: boolean): void {
  if (m.wall !== null) {
    const wallAngle = atan2s(m.wall.normal[2], m.wall.normal[0]);
    m.faceAngle[1] = toS16(wallAngle - toS16(m.faceAngle[1] - wallAngle));
  }

  if (negateSpeed) {
    marioSetForwardVel(m, -m.forwardVel);
  } else {
    m.faceAngle[1] = toS16(m.faceAngle[1] + 0x8000);
  }
}

// Re-export frequently used values so action modules can import from one place.
export {
  AIR_STEP_NONE,
  AIR_STEP_LANDED,
  AIR_STEP_HIT_WALL,
  AIR_STEP_GRABBED_LEDGE,
  AIR_STEP_GRABBED_CEILING,
  AIR_STEP_HIT_LAVA_WALL,
  GROUND_STEP_HIT_WALL,
  GROUND_STEP_NONE,
  GROUND_STEP_LEFT_GROUND,
  GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS,
  GROUND_STEP_HIT_WALL_STOP_QSTEPS,
};
