import * as A from '../sim/state';
import { SURFACE_CLASS_VERY_SLIPPERY } from '../sim/surfaces';

// IDs and rates from reference/sm64/{include/mario_animation_ids.h,src/game/mario_actions_*.c}.
export const SM64_ANIM = {
  idle: 0xc5, idleLeft: 0xc3, tiptoeStart: 0xca, tiptoe: 0x92, walk: 0x48, run: 0x72,
  jump: 0x4d, jumpLand: 0x4e, doubleRise: 0x50, doubleFall: 0x4c, doubleLand: 0x4b,
  triple: 0xc1, tripleLand: 0xc0, backflip: 0x04, sideflip: 0xbf,
  sideLand: 0xbe, longJump: 0x13, longJumpSlow: 0x14, longLand: 0x11, longLandSlow: 0x12,
  fall: 0x56, fallFromSlide: 0x90, fallFromSlideKick: 0x53, land: 0x57,
  crouch: 0x98, crouchStart: 0x97, crouchStop: 0x96, crawl: 0x99,
  crawlStart: 0x9b, crawlStop: 0x9a, dive: 0x88, diveSlide: 0x89,
  slide: 0x91, slideStop: 0x8f, slideKick: 0x8c, slideKickStop: 0x8d,
  poundStart: 0x3c, triplePound: 0x3b, pound: 0x3d, poundLand: 0x3a,
  wallKick: 0xcb, hitWall: 0xcc,
  airKick: 0x4f, punch: 0x67, punchFast: 0x69, punchSecond: 0x68, punchSecondFast: 0x6a, kick: 0x66, breakdance: 0x71,
  brake: 0x0f, brakeStop: 0x10, turn: 0xbc, turnEnd: 0xbd,
  deathOnBack: 0x03, deathOnStomach: 0x2e, deathFallOver: 0x32,
  drowning1: 0xa5, drowning2: 0xa6, waterDying: 0xa7,
  hardBackwardKb: 0x01, hardForwardKb: 0x2c,
  ledge: 0x33, ledgeClimb: 0x34, slowLedge: 0x00,
  hangOnCeiling: 0x35, handstandLeft: 0xc6, handstandRight: 0xc7,
  poleClimb: 0x05, poleGrab: 0x06, poleTop: 0x09, poleTransition: 0x0b, poleIdle: 0x0d,
  lavaLand: 0x28, lavaBurn: 0x29, quicksandDeath: 0x76,
  wireLeft: 0x5d, wireRight: 0x5c,
  startSleepIdle: 0x81, startSleepScratch: 0x82, startSleepYawn: 0x83, startSleepSitting: 0x84,
  sleepIdle: 0x85, sleepStartLying: 0x86, sleepLying: 0x87, wakeFromSleep: 0xc8,
  pickUp: 0x6b, placeDown: 0x6e, groundThrow: 0x65, airThrow: 0x52,
  holdIdle: 0x3f, holdJump: 0x41, holdFall: 0x43, holdLand: 0x40, holdFallLand: 0x42,
  holdSlowWalk: 0x18, holdWalk: 0x16, holdRun: 0x17,
} as const;

export function sm64ClipName(id: number): string {
  return `anim_${id.toString(16).toUpperCase().padStart(2, '0')}`;
}

export interface Sm64AnimationState {
  action: number;
  actionArg: number;
  actionState: number;
  actionTimer?: number;
  forwardVel: number;
  intendedMag?: number;
  floorClass?: () => number;
  longJumpIsSlow?: boolean;
  angleVel?: readonly number[];
  faceAngle?: readonly number[];
  vel: readonly number[];
}
export interface Sm64ClipChoice { name: string; speed: number; loop?: boolean }
export interface Sm64PlaybackContext { name: string; frame: number; cycles: number; finished?: boolean }

export function chooseSm64Clip(m: Sm64AnimationState, previous?: Sm64PlaybackContext): Sm64ClipChoice {
  // Native assets are sampled at 30 fps: one source frame per simulation tick is 1x.
  // SM64's 16.16 animation acceleration is a multiplier, not a normalized velocity.
  const clip = (id: number, speed = 1, loop?: boolean): Sm64ClipChoice => ({ name: sm64ClipName(id), speed, ...(loop === undefined ? {} : { loop }) });
  switch (m.action) {
    case A.ACT_JUMBO_STAR_CUTSCENE:
      if (m.actionArg === 0) return clip(m.actionState ? SM64_ANIM.land : SM64_ANIM.fall);
      if (m.actionArg === 1) return clip(m.actionState ? 0x26 : 0x25);
      return clip(0x2a);
    case A.ACT_END_PEACH_CUTSCENE: {
      const t = m.actionTimer ?? 0;
      switch (m.actionArg) {
        case 0: return clip(SM64_ANIM.fall);
        case 1: return clip(SM64_ANIM.land);
        case 2: return clip(t < 96 ? 0x20 : 0x21);
        case 3: return clip(0x21);
        case 4: return clip(t < 200 ? 0x21 : 0x1f);
        case 5: return clip(SM64_ANIM.run, 8);
        case 6: return clip(t < 27 ? 0x22 : 0x1e);
        case 7: case 8: return clip(0x1e);
        case 9: return clip(0x27);
        case 10: return clip(0xc2);
        default: return clip(t < 100 ? 0x23 : 0x24);
      }
    }
    case A.ACT_WATER_IDLE: return clip(178, 1, true);
    case A.ACT_IN_CANNON: return clip(SM64_ANIM.dive);
    case A.ACT_SHOT_FROM_CANNON: return clip(0x15);
    case A.ACT_FLYING: return clip(SM64_ANIM.fall, 1, true);
    case A.ACT_GRABBED: return clip(SM64_ANIM.hardBackwardKb, 1, true);
    case A.ACT_RIDING_SHELL_GROUND: return clip(SM64_ANIM.walk, 1, true);
    case A.ACT_RIDING_SHELL_JUMP: return clip(SM64_ANIM.jump);
    case A.ACT_METAL_WATER_STANDING: return clip(SM64_ANIM.idle, 1, true);
    case A.ACT_METAL_WATER_WALKING: return clip(SM64_ANIM.walk, 1, true);
    case A.ACT_METAL_WATER_FALLING: return clip(SM64_ANIM.fall, 1, true);
    case A.ACT_METAL_WATER_JUMP: return clip(SM64_ANIM.jump);
    case A.ACT_SOFT_BACKWARD_GROUND_KB: return clip(0x74);
    case A.ACT_SOFT_FORWARD_GROUND_KB: return clip(0x75);
    case A.ACT_BACKWARD_GROUND_KB: return clip(0x7b);
    case A.ACT_FORWARD_GROUND_KB: return clip(0x7c);
    case A.ACT_BACKWARD_AIR_KB: return clip(0x02);
    case A.ACT_FORWARD_AIR_KB: return clip(0x2d);
    case A.ACT_HARD_BACKWARD_AIR_KB: return clip(SM64_ANIM.hardBackwardKb);
    case A.ACT_HARD_FORWARD_AIR_KB: return clip(SM64_ANIM.hardForwardKb);
    case A.ACT_HARD_BACKWARD_GROUND_KB: return clip(SM64_ANIM.hardBackwardKb);
    case A.ACT_HARD_FORWARD_GROUND_KB: return clip(SM64_ANIM.hardForwardKb);
    case A.ACT_SPAWN_SPIN_AIRBORNE: return clip(m.actionState === 0 ? 0x6f : SM64_ANIM.fall);
    case A.ACT_SPAWN_NO_SPIN_AIRBORNE:
    case A.ACT_SPECIAL_EXIT_AIRBORNE: return clip(SM64_ANIM.jump);
    case A.ACT_EXIT_AIRBORNE:
    case A.ACT_FALLING_EXIT_AIRBORNE: return clip(SM64_ANIM.fall);
    case A.ACT_SPAWN_NO_SPIN_LANDING:
    case A.ACT_SPAWN_SPIN_LANDING:
      return clip(SM64_ANIM.land);
    case A.ACT_EXIT_LAND_SAVE_DIALOG: return clip(m.actionArg ? SM64_ANIM.jumpLand : SM64_ANIM.land);
    case A.ACT_ENTERING_STAR_DOOR: return (m.actionTimer ?? 0) < 15 ? clip(0xc2) : clip(SM64_ANIM.walk, 2.5);
    case A.ACT_UNLOCKING_STAR_DOOR: return clip(m.actionState === 3 ? 0x9d : 0x9c);
    case A.ACT_UNLOCKING_KEY_DOOR: return clip(0x61);
    case A.ACT_PULLING_DOOR: return clip(0x5f);
    case A.ACT_PUSHING_DOOR: return clip(0x60);
    case A.ACT_WATER_JUMP: return clip(SM64_ANIM.jump);
    case A.ACT_WATER_ACTION_END:
    case A.ACT_WATER_PLUNGE: return clip(173);
    case A.ACT_BREASTSTROKE: return clip(170);
    case A.ACT_SWIMMING_END: return clip(171);
    case A.ACT_FLUTTER_KICK: return clip(172, 1, true);
    case A.ACT_IDLE: {
      const sequence = [0xc3, 0xc4, 0xc5];
      const index = sequence.findIndex(id => sm64ClipName(id) === previous?.name);
      return clip(sequence[index < 0 ? 0 : (index + (previous!.finished ? 1 : 0)) % sequence.length], 1, false);
    }
    case A.ACT_WALKING: {
      const speed = Math.max(m.intendedMag ?? 0, m.forwardVel, 4);
      const is = (id: number) => previous?.name === sm64ClipName(id);
      if (speed <= 8 && (!previous || ![SM64_ANIM.walk, SM64_ANIM.run, SM64_ANIM.tiptoe].some(is))) {
        if (!is(SM64_ANIM.tiptoeStart) || !previous || previous.cycles === 0 || previous.frame < 23) {
          return clip(SM64_ANIM.tiptoeStart, speed / 4);
        }
      }
      if (speed < 5 || (is(SM64_ANIM.tiptoe) && speed <= 8)) return clip(SM64_ANIM.tiptoe, speed);
      if (speed > 22 || (is(SM64_ANIM.run) && speed >= 18)) return clip(SM64_ANIM.run, speed / 4);
      return clip(SM64_ANIM.walk, speed / 4);
    }
    case A.ACT_DECELERATING:
      return m.floorClass?.() === SURFACE_CLASS_VERY_SLIPPERY ? clip(SM64_ANIM.idleLeft) : clip(SM64_ANIM.walk, Math.max(m.forwardVel / 4, 1 / 16));
    case A.ACT_TURNING_AROUND: return clip(m.forwardVel >= 18 ? SM64_ANIM.turn : SM64_ANIM.turnEnd);
    case A.ACT_FINISH_TURNING_AROUND: return clip(SM64_ANIM.turnEnd);
    case A.ACT_BRAKING: return clip(SM64_ANIM.brake);
    case A.ACT_BRAKING_STOP: return clip(SM64_ANIM.brakeStop);
    case A.ACT_JUMP:
    case A.ACT_STEEP_JUMP: return clip(SM64_ANIM.jump);
    case A.ACT_DOUBLE_JUMP: return clip(m.vel[1] >= 0 ? SM64_ANIM.doubleRise : SM64_ANIM.doubleFall);
    case A.ACT_TRIPLE_JUMP: return clip(SM64_ANIM.triple);
    case A.ACT_BACKFLIP: return clip(SM64_ANIM.backflip);
    case A.ACT_SIDE_FLIP: return clip(SM64_ANIM.sideflip);
    case A.ACT_LONG_JUMP: return clip(m.longJumpIsSlow ? SM64_ANIM.longJumpSlow : SM64_ANIM.longJump);
    case A.ACT_WALL_KICK_AIR: return clip(SM64_ANIM.wallKick);
    case A.ACT_AIR_HIT_WALL: return clip(SM64_ANIM.hitWall);
    case A.ACT_JUMP_KICK: return clip(SM64_ANIM.airKick);
    case A.ACT_JUMP_LAND:
    case A.ACT_JUMP_LAND_STOP: return clip(SM64_ANIM.jumpLand);
    case A.ACT_DOUBLE_JUMP_LAND:
    case A.ACT_DOUBLE_JUMP_LAND_STOP: return clip(SM64_ANIM.doubleLand);
    case A.ACT_BACKFLIP_LAND:
    case A.ACT_BACKFLIP_LAND_STOP:
    case A.ACT_TRIPLE_JUMP_LAND:
    case A.ACT_TRIPLE_JUMP_LAND_STOP: return clip(SM64_ANIM.tripleLand);
    case A.ACT_SIDE_FLIP_LAND:
    case A.ACT_SIDE_FLIP_LAND_STOP: return clip(SM64_ANIM.sideLand);
    case A.ACT_LONG_JUMP_LAND:
    case A.ACT_LONG_JUMP_LAND_STOP: return clip(m.longJumpIsSlow ? SM64_ANIM.longLandSlow : SM64_ANIM.longLand);
    case A.ACT_FREEFALL_LAND:
    case A.ACT_FREEFALL_LAND_STOP: return clip(SM64_ANIM.land);
    case A.ACT_FREEFALL: return clip(m.actionArg === 1 ? SM64_ANIM.fallFromSlide : m.actionArg === 2 ? SM64_ANIM.fallFromSlideKick : SM64_ANIM.fall);
    case A.ACT_SOFT_BONK: return clip(SM64_ANIM.fall);
    case A.ACT_CROUCHING: return clip(SM64_ANIM.crouch);
    case A.ACT_CROUCH_SLIDE:
    case A.ACT_START_CROUCHING: return clip(SM64_ANIM.crouchStart);
    case A.ACT_STOP_CROUCHING: return clip(SM64_ANIM.crouchStop);
    case A.ACT_CRAWLING: return clip(SM64_ANIM.crawl, Math.max(0, (m.intendedMag ?? 0) * 2));
    case A.ACT_START_CRAWLING: return clip(SM64_ANIM.crawlStart);
    case A.ACT_STOP_CRAWLING: return clip(SM64_ANIM.crawlStop);
    case A.ACT_DIVE:
    case A.ACT_DIVE_SLIDE: return clip(SM64_ANIM.dive);
    case A.ACT_STOMACH_SLIDE: return clip(SM64_ANIM.diveSlide);
    case A.ACT_BUTT_SLIDE:
    case A.ACT_BUTT_SLIDE_AIR: return clip(SM64_ANIM.slide);
    case A.ACT_BUTT_SLIDE_STOP: return clip(SM64_ANIM.slideStop);
    case A.ACT_SLIDE_KICK:
    case A.ACT_SLIDE_KICK_SLIDE: return clip(SM64_ANIM.slideKick);
    case A.ACT_SLIDE_KICK_SLIDE_STOP: return clip(SM64_ANIM.slideKickStop);
    case A.ACT_GROUND_POUND: return clip(m.actionState === 0 ? (m.actionArg === 0 ? SM64_ANIM.poundStart : SM64_ANIM.triplePound) : SM64_ANIM.pound);
    case A.ACT_GROUND_POUND_LAND: return clip(SM64_ANIM.poundLand);
    case A.ACT_PUNCHING:
    case A.ACT_MOVE_PUNCHING:
      if (m.actionArg === 9) return clip(SM64_ANIM.breakdance);
      if (m.actionArg === 6) return clip(SM64_ANIM.kick);
      if (m.actionArg === 5) return clip(SM64_ANIM.punchSecondFast);
      if (m.actionArg === 3 || m.actionArg === 4) return clip(SM64_ANIM.punchSecond);
      if (m.actionArg === 2) return clip(SM64_ANIM.punchFast);
      return clip(SM64_ANIM.punch);
    case A.ACT_LEDGE_GRAB: return clip(SM64_ANIM.ledge);
    case A.ACT_LEDGE_CLIMB_SLOW_1:
    case A.ACT_LEDGE_CLIMB_SLOW_2: return clip(SM64_ANIM.slowLedge);
    case A.ACT_LEDGE_CLIMB_FAST: return clip(SM64_ANIM.ledgeClimb);
    case A.ACT_START_HANGING: return clip(SM64_ANIM.hangOnCeiling);
    case A.ACT_GRAB_POLE_SLOW:
    case A.ACT_GRAB_POLE_FAST: return clip(SM64_ANIM.poleGrab);
    case A.ACT_HOLDING_POLE: return clip(SM64_ANIM.poleIdle);
    case A.ACT_CLIMBING_POLE: return clip(SM64_ANIM.poleClimb);
    case A.ACT_TOP_OF_POLE_TRANSITION:
      return clip(m.actionArg === 0 ? SM64_ANIM.poleTransition : SM64_ANIM.poleIdle);
    case A.ACT_TOP_OF_POLE: return clip(SM64_ANIM.poleTop);
    case A.ACT_LAVA_BOOST: return clip(SM64_ANIM.lavaBurn);
    case A.ACT_LAVA_BOOST_LAND: return clip(SM64_ANIM.lavaLand);
    case A.ACT_QUICKSAND_DEATH: return clip(SM64_ANIM.quicksandDeath);
    case A.ACT_HANGING:
      return clip(m.actionArg & 1 ? SM64_ANIM.handstandLeft : SM64_ANIM.handstandRight);
    case A.ACT_HANG_MOVING:
      return clip(m.actionArg & 1 ? SM64_ANIM.wireLeft : SM64_ANIM.wireRight);
    case A.ACT_START_SLEEPING:
      return clip([SM64_ANIM.startSleepIdle, SM64_ANIM.startSleepScratch, SM64_ANIM.startSleepYawn, SM64_ANIM.startSleepSitting][Math.min(3, m.actionState)]);
    case A.ACT_SLEEPING:
      return clip([SM64_ANIM.sleepIdle, SM64_ANIM.sleepStartLying, SM64_ANIM.sleepLying][Math.min(2, m.actionState)]);
    case A.ACT_WAKING_UP: return clip(SM64_ANIM.wakeFromSleep);
    case A.ACT_STANDING_DEATH: return clip(SM64_ANIM.deathFallOver);
    case A.ACT_DEATH_ON_BACK: return clip(SM64_ANIM.deathOnBack);
    case A.ACT_DEATH_ON_STOMACH: return clip(SM64_ANIM.deathOnStomach);
    case A.ACT_DROWNING:
      return clip(m.actionState === 0 ? SM64_ANIM.drowning1 : SM64_ANIM.drowning2);
    case A.ACT_WATER_DEATH: return clip(SM64_ANIM.waterDying);
    case A.ACT_PICKING_UP_BOWSER: return clip(0xb5);
    case A.ACT_HOLDING_BOWSER: return clip(m.angleVel?.[1] ? 0xb6 : 0xb8, 1, true);
    case A.ACT_RELEASING_BOWSER: return clip(0xb7);
    case A.ACT_PICKING_UP: return clip(SM64_ANIM.pickUp);
    case A.ACT_PLACING_DOWN: return clip(SM64_ANIM.placeDown);
    case A.ACT_THROWING: return clip(SM64_ANIM.groundThrow);
    case A.ACT_AIR_THROW:
    case A.ACT_AIR_THROW_LAND: return clip(SM64_ANIM.airThrow);
    case A.ACT_HOLD_IDLE: return clip(SM64_ANIM.holdIdle, 1, true);
    case A.ACT_HOLD_WALKING: {
      const speed = Math.max(m.intendedMag ?? 0, m.forwardVel, 4);
      if (speed > 11) return clip(SM64_ANIM.holdRun, speed / 2);
      if (speed > 3) return clip(SM64_ANIM.holdWalk, speed);
      return clip(SM64_ANIM.holdSlowWalk, speed);
    }
    case A.ACT_HOLD_DECELERATING:
      return clip(SM64_ANIM.holdSlowWalk, Math.max(m.forwardVel / 4, 1 / 16));
    case A.ACT_HOLD_JUMP: return clip(SM64_ANIM.holdJump);
    case A.ACT_HOLD_FREEFALL: return clip(SM64_ANIM.holdFall);
    case A.ACT_HOLD_JUMP_LAND:
    case A.ACT_HOLD_JUMP_LAND_STOP: return clip(SM64_ANIM.holdLand);
    case A.ACT_HOLD_FREEFALL_LAND:
    case A.ACT_HOLD_FREEFALL_LAND_STOP: return clip(SM64_ANIM.holdFallLand);
    default: throw new Error(`No SM64 animation mapping for action 0x${m.action.toString(16)}`);
  }
}

// These are graphics-only offsets in the decomp; never change physics/camera yaw.
export function sm64VisualRotation(m: Sm64AnimationState): { yaw: number; pitch: number; roll?: number } {
  if (m.action === A.ACT_JUMBO_STAR_CUTSCENE && m.actionArg === 2) return { yaw: 0, pitch: -(m.faceAngle?.[0] ?? 0)*Math.PI/32768, roll: (m.faceAngle?.[2] ?? 0)*Math.PI/32768 };
  const reverse = [A.ACT_SIDE_FLIP, A.ACT_SIDE_FLIP_LAND, A.ACT_SIDE_FLIP_LAND_STOP, A.ACT_FINISH_TURNING_AROUND].includes(m.action);
  if (m.action === A.ACT_HOLDING_BOWSER) return { yaw: 0, pitch: Math.abs(m.angleVel?.[1] ?? 0) * Math.PI / 32768 };
  return { yaw: reverse ? Math.PI : 0, pitch: m.action === A.ACT_DIVE || (m.action & A.ACT_FLAG_SWIMMING) ? -(m.faceAngle?.[0] ?? 0) * Math.PI / 32768 : 0 };
}

const continuations = new Map<number, readonly number[]>([
  [A.ACT_JUMP_LAND_STOP, [A.ACT_JUMP_LAND]],
  [A.ACT_DOUBLE_JUMP_LAND_STOP, [A.ACT_DOUBLE_JUMP_LAND]],
  [A.ACT_TRIPLE_JUMP_LAND_STOP, [A.ACT_TRIPLE_JUMP_LAND]],
  [A.ACT_BACKFLIP_LAND_STOP, [A.ACT_BACKFLIP_LAND]],
  [A.ACT_SIDE_FLIP_LAND_STOP, [A.ACT_SIDE_FLIP_LAND]],
  [A.ACT_LONG_JUMP_LAND_STOP, [A.ACT_LONG_JUMP_LAND]],
  [A.ACT_FREEFALL_LAND_STOP, [A.ACT_FREEFALL_LAND]],
  [A.ACT_DIVE_SLIDE, [A.ACT_DIVE]],
  [A.ACT_SLIDE_KICK_SLIDE, [A.ACT_SLIDE_KICK]],
  [A.ACT_BUTT_SLIDE, [A.ACT_BUTT_SLIDE_AIR]],
  [A.ACT_BUTT_SLIDE_AIR, [A.ACT_BUTT_SLIDE]],
  [A.ACT_FINISH_TURNING_AROUND, [A.ACT_TURNING_AROUND]],
  [A.ACT_START_CROUCHING, [A.ACT_CROUCH_SLIDE]],
  [A.ACT_DECELERATING, [A.ACT_WALKING]],
  [A.ACT_WALKING, [A.ACT_DECELERATING]],
]);

export function continuesSm64Clip(previousAction: number, action: number): boolean {
  return previousAction === action || (continuations.get(action)?.includes(previousAction) ?? false);
}
