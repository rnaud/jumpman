import type { MarioRuntime } from './MarioRuntime';
import { approachF32, clamp, coss, sins, toS16 } from './math';
import { marioSetForwardVel } from './marioHelpers';
import {
  ACT_BACKFLIP, ACT_CROUCHING, ACT_DOUBLE_JUMP, ACT_FLAG_AIR, ACT_FLAG_BUTT_OR_STOMACH_SLIDE, ACT_FLAG_DIVING,
  ACT_FLAG_INTANGIBLE, ACT_FLAG_INVULNERABLE, ACT_FLAG_METAL_WATER, ACT_FLAG_RIDING_SHELL, ACT_FLAG_SWIMMING,
  ACT_FREEFALL, ACT_FREEFALL_LAND, ACT_GROUND_POUND, ACT_GROUP_MASK, ACT_GROUP_MOVING, ACT_GROUP_STATIONARY,
  ACT_IDLE, ACT_JUMP, ACT_JUMP_KICK, ACT_JUMP_LAND, ACT_LAVA_BOOST, ACT_LEDGE_GRAB, ACT_LONG_JUMP,
  ACT_MOVE_PUNCHING, ACT_PUNCHING, ACT_SIDE_FLIP, ACT_START_CROUCHING, ACT_STEEP_JUMP, ACT_STOP_CROUCHING,
  ACT_TRIPLE_JUMP, ACT_WALKING, ACT_WALL_KICK_AIR, A_BUTTON, AIR_STEP_CHECK_LEDGE_GRAB, AIR_STEP_GRABBED_LEDGE,
  AIR_STEP_HIT_LAVA_WALL, AIR_STEP_LANDED, B_BUTTON, GROUND_STEP_HIT_WALL, GROUND_STEP_LEFT_GROUND,
  INPUT_OFF_FLOOR, MARIO_KICKING, MARIO_PUNCHING, MARIO_TRIPPING, MARIO_WING_CAP, Z_TRIG,
} from './state';

/*
 * Banjo-Kazooie's moveset from src/core2/bs/*.c in n64decomp/banjo-kazooie.
 * Each move borrows the SM64 action ID with the same role (ACT_PUNCHING for the Claw Swipe,
 * ACT_GROUND_POUND for the Beak Buster, …) so course objects react to Banjo through
 * their native interactions; the action argument marks the BK version.
 * Rare's units per second are converted to Jumpman's 30 Hz frames at Banjo's 1.25 model scale.
 */
const SCALE = 1.25;
const speed = (unitsPerSecond: number) => unitsPerSecond * SCALE / 30;
const accel = (unitsPerSecond2: number) => unitsPerSecond2 * SCALE / 900;
const seconds = (s: number) => Math.round(s * 30);

export const BANJO_FLAP = 0x60;
export const BANJO_JUMP = 0x61;
export const BANJO_FLIP = 0x62;
export const BANJO_CLAW = 0x63;
export const BANJO_ROLL = 0x64;
export const BANJO_BARGE = 0x65;
export const BANJO_PECK = 0x66;

export const banjoFlapping = (m: MarioRuntime) => m.action === ACT_FREEFALL && m.actionArg === BANJO_FLAP;
export const banjoMove = (m: MarioRuntime) => (m.actionArg >= BANJO_FLAP && m.actionArg <= BANJO_PECK ? m.actionArg : 0);

// baphysics.c and bsjump.c
const GRAVITY = accel(2700);
const TERMINAL = -75;
const JUMP_VELOCITY = speed(710);
const JUMP_HELD_GRAVITY = accel(1350);
const MAX_WALK = speed(500);
// bsbflap.c
const FLAP_IMPULSE = speed(280);
const FLAP_GRAVITY = accel(1100);
const FLAP_TERMINAL = speed(-399.9);
const FLAP_FRAMES = seconds(2.5);
const FLAP_PERIOD = 9;
// bsbflip.c
const FLIP_VELOCITY = speed(920);
const FLIP_GRAVITY = accel(1200);
const FLIP_TERMINAL = speed(-533.3);
const FLIP_FORWARD = speed(200);
const FLIP_WINDUP = 5;
// bsclaw.c: 1.3 s swipe, three hitbox windows, lunge at 160 until 0.5788.
const CLAW_FRAMES = seconds(1.3);
const CLAW_LUNGE = speed(160);
// bstwirl.c (BS_31_ROLL): 0.9 s, 600 units/s, hitbox until 0.8011.
const ROLL_FRAMES = seconds(0.9);
const ROLL_SPEED = speed(600);
// bsbbarge.c: wind up, charge at 850 (B held) or 500, shed 80 per frame below the hold, stop at 200.
const BARGE_WINDUP = 4;
const BARGE_FULL = speed(850);
const BARGE_SHORT = speed(500);
const BARGE_DECAY = speed(80);
const BARGE_END = speed(200);
const BARGE_FRAMES = seconds(1.0);
// bsbpeck.c: gravity -1400, 120 units/s hop per peck, 0.5 s of pecking, then a 0.2 s recoil.
const PECK_GRAVITY = accel(1400);
const PECK_HOP = speed(120);
const PECK_FRAMES = seconds(0.5);
const PECK_END = seconds(0.7);

const FLAP_FROM = new Set([ACT_JUMP, ACT_DOUBLE_JUMP, ACT_TRIPLE_JUMP, ACT_FREEFALL, ACT_SIDE_FLIP,
  ACT_BACKFLIP, ACT_STEEP_JUMP, ACT_WALL_KICK_AIR, ACT_LONG_JUMP, ACT_JUMP_KICK]);
const CROUCHED = new Set([ACT_CROUCHING, ACT_START_CROUCHING, ACT_STOP_CROUCHING]);

/** A plain standing, walking or landing action where BK's button moves may start. */
function onBasicGround(m: MarioRuntime): boolean {
  const group = m.action & ACT_GROUP_MASK;
  return (group === ACT_GROUP_STATIONARY || group === ACT_GROUP_MOVING)
    && !(m.action & (ACT_FLAG_AIR | ACT_FLAG_SWIMMING | ACT_FLAG_INVULNERABLE | ACT_FLAG_INTANGIBLE | ACT_FLAG_METAL_WATER
      | ACT_FLAG_BUTT_OR_STOMACH_SLIDE | ACT_FLAG_DIVING | ACT_FLAG_RIDING_SHELL))
    && !banjoMove(m);
}

function start(m: MarioRuntime, action: number, arg: number) {
  const forward = m.forwardVel;
  const vy = m.vel[1];
  m.setAction(action, arg);
  // setAction seeds SM64's own launch velocities; BK moves set theirs explicitly.
  m.forwardVel = forward;
  m.vel[1] = vy;
}

/** Steer toward the stick the way baphysics' airborne mode chases a target speed. */
function airControl(m: MarioRuntime, max: number, rate = 1.5) {
  const target = m.intendedMag > 0 ? Math.min(max, m.intendedMag / 32 * max) : 0;
  if (m.intendedMag > 0) {
    const turn = toS16(m.intendedYaw - m.faceAngle[1]);
    m.faceAngle[1] = toS16(m.faceAngle[1] + clamp(turn, -0x800, 0x800));
  }
  m.forwardVel = approachF32(m.forwardVel, target, rate, rate);
  m.vel[0] = sins(m.faceAngle[1]) * m.forwardVel;
  m.vel[2] = coss(m.faceAngle[1]) * m.forwardVel;
}

/** One airborne frame with BK gravity; SM64's applyGravity after the step is undone. */
function airStep(m: MarioRuntime, gravity: number, terminal = TERMINAL): boolean {
  m.vel[1] = Math.max(terminal, m.vel[1] - gravity);
  const vy = m.vel[1];
  const hit = m.stepAir(AIR_STEP_CHECK_LEDGE_GRAB);
  if (hit !== AIR_STEP_LANDED) m.vel[1] = vy;
  if (hit === AIR_STEP_LANDED) {
    m.landEvent(vy, m.forwardVel);
    m.setAction(m.actionArg === BANJO_JUMP ? ACT_JUMP_LAND : ACT_FREEFALL_LAND);
    return true;
  }
  if (hit === AIR_STEP_GRABBED_LEDGE) m.setAction(ACT_LEDGE_GRAB);
  else if (hit === AIR_STEP_HIT_LAVA_WALL) m.setAction(ACT_LAVA_BOOST);
  return hit === AIR_STEP_GRABBED_LEDGE || hit === AIR_STEP_HIT_LAVA_WALL;
}

/** One grounded frame of a BK move at `forward` speed. Returns false once the move left the ground or hit a wall. */
function groundStep(m: MarioRuntime, forward: number): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    m.setAction(ACT_FREEFALL);
    return false;
  }
  marioSetForwardVel(m, forward);
  const hit = m.stepGround();
  if (hit === GROUND_STEP_LEFT_GROUND) {
    m.setAction(ACT_FREEFALL);
    return false;
  }
  if (hit === GROUND_STEP_HIT_WALL && forward > 8) {
    marioSetForwardVel(m, 0);
    m.setAction(ACT_IDLE);
    return false;
  }
  return true;
}

/** Banjo's moveset. Returns true when it consumed this simulation frame. */
export function stepBanjo(m: MarioRuntime): boolean {
  if (!(m.action & ACT_FLAG_AIR)) m.banjoFlap = 0;
  if (m.heldObj || m.ridingShell || (m.flags & MARIO_WING_CAP)) return false;
  const c = m.controller;
  const pressed = c.buttonPressed;

  switch (banjoMove(m)) {
    case BANJO_FLAP: return stepFlap(m);
    case BANJO_JUMP: if (m.action === ACT_JUMP) return stepJump(m); break;
    case BANJO_FLIP: if (m.action === ACT_BACKFLIP) return stepFlip(m); break;
    case BANJO_CLAW: if (m.action === ACT_PUNCHING) return stepClaw(m); break;
    case BANJO_ROLL: if (m.action === ACT_MOVE_PUNCHING) return stepRoll(m); break;
    case BANJO_BARGE: if (m.action === ACT_MOVE_PUNCHING) return stepBarge(m); break;
    case BANJO_PECK: if (m.action === ACT_JUMP_KICK) return stepPeck(m); break;
  }

  if (m.action & ACT_FLAG_AIR) {
    if (!FLAP_FROM.has(m.action)) return false;
    // Rat-a-tat Rap (B), Feathery Flap (A once per airtime); Z falls through to SM64's ground pound.
    if (pressed & B_BUTTON) {
      start(m, ACT_JUMP_KICK, BANJO_PECK);
      m.vel[1] = PECK_HOP;
      return stepPeck(m);
    }
    if (m.banjoFlap === 0 && (pressed & A_BUTTON) && m.vel[1] < 12) {
      start(m, ACT_FREEFALL, BANJO_FLAP);
      m.banjoFlap = FLAP_FRAMES;
      m.vel[1] = 0;
      return stepFlap(m);
    }
    return false;
  }

  if (!onBasicGround(m)) return false;
  const crouched = CROUCHED.has(m.action);
  if (crouched && (pressed & A_BUTTON)) {
    start(m, ACT_BACKFLIP, BANJO_FLIP);
    marioSetForwardVel(m, 0);
    m.vel[1] = 0;
    return stepFlip(m);
  }
  if (crouched && (pressed & B_BUTTON)) {
    start(m, ACT_MOVE_PUNCHING, BANJO_BARGE);
    return stepBarge(m);
  }
  if (crouched) return false;
  if (pressed & A_BUTTON) {
    if (m.intendedMag > 0) m.faceAngle[1] = m.intendedYaw;
    start(m, ACT_JUMP, BANJO_JUMP);
    m.vel[1] = JUMP_VELOCITY;
    m.jumpTakeoffSpeed = Math.max(MAX_WALK, Math.abs(m.forwardVel));
    airControl(m, m.jumpTakeoffSpeed);
    return stepJump(m, true);
  }
  if (pressed & B_BUTTON) {
    // func_802B6F20: fast walking rolls, otherwise Banjo swipes.
    if (Math.abs(m.forwardVel) > speed(225)) {
      start(m, ACT_MOVE_PUNCHING, BANJO_ROLL);
      return stepRoll(m);
    }
    start(m, ACT_PUNCHING, BANJO_CLAW);
    return stepClaw(m);
  }
  if (pressed & Z_TRIG) {
    marioSetForwardVel(m, 0);
    m.setAction(ACT_START_CROUCHING);
    return false;
  }
  return false;
}

function stepJump(m: MarioRuntime, launched = false): boolean {
  const c = m.controller;
  if (!launched) {
    if (c.buttonPressed & Z_TRIG) { m.setAction(ACT_GROUND_POUND); return false; }
    if (c.buttonPressed & B_BUTTON) {
      start(m, ACT_JUMP_KICK, BANJO_PECK);
      m.vel[1] = PECK_HOP;
      return stepPeck(m);
    }
    if ((c.buttonPressed & A_BUTTON) && m.banjoFlap === 0 && m.vel[1] < 12) {
      start(m, ACT_FREEFALL, BANJO_FLAP);
      m.banjoFlap = FLAP_FRAMES;
      m.vel[1] = 0;
      return stepFlap(m);
    }
    airControl(m, m.jumpTakeoffSpeed);
  }
  m.actionTimer++;
  // bsjump_update: releasing A while rising restores full gravity.
  const held = (c.buttonDown & A_BUTTON) && m.vel[1] > 0;
  airStep(m, held ? JUMP_HELD_GRAVITY : GRAVITY);
  return true;
}

function stepFlap(m: MarioRuntime): boolean {
  const c = m.controller;
  if (c.buttonPressed & Z_TRIG) {
    m.banjoFlap = -1;
    m.setAction(ACT_GROUND_POUND);
    return false;
  }
  if (!(c.buttonDown & A_BUTTON) || --m.banjoFlap <= 0) {
    // Kazooie is spent until Banjo lands again.
    m.banjoFlap = -1;
    m.setAction(ACT_FREEFALL);
    return false;
  }
  airControl(m, Math.max(MAX_WALK, m.forwardVel), 1);
  if (m.actionTimer++ % FLAP_PERIOD === 0) m.vel[1] = FLAP_IMPULSE + FLAP_GRAVITY;
  airStep(m, FLAP_GRAVITY, FLAP_TERMINAL);
  return true;
}

function stepFlip(m: MarioRuntime): boolean {
  const c = m.controller;
  m.actionTimer++;
  if (m.actionTimer <= FLIP_WINDUP) {
    // Banjo crouches while Kazooie braces; the launch happens at anim 0.1837.
    if (m.actionTimer === FLIP_WINDUP) {
      if (m.intendedMag > 0) m.faceAngle[1] = m.intendedYaw;
      m.forwardVel = m.intendedMag > 0 ? FLIP_FORWARD : 0;
      m.vel[1] = FLIP_VELOCITY + FLIP_GRAVITY;
      m.vel[0] = sins(m.faceAngle[1]) * m.forwardVel;
      m.vel[2] = coss(m.faceAngle[1]) * m.forwardVel;
    } else {
      m.stepGround();
      return true;
    }
  } else {
    if (c.buttonPressed & Z_TRIG) { m.setAction(ACT_GROUND_POUND); return false; }
    airControl(m, Math.max(FLIP_FORWARD, m.forwardVel), 0.5);
  }
  // Holding A keeps the slow fall until release (bsbflip case 2).
  const terminal = c.buttonDown & A_BUTTON ? FLIP_TERMINAL : TERMINAL;
  airStep(m, FLIP_GRAVITY, terminal);
  return true;
}

function stepClaw(m: MarioRuntime): boolean {
  const c = m.controller;
  const t = m.actionTimer++ / CLAW_FRAMES;
  if (c.buttonPressed & A_BUTTON) {
    start(m, ACT_JUMP, BANJO_JUMP);
    m.vel[1] = JUMP_VELOCITY;
    m.jumpTakeoffSpeed = MAX_WALK;
    return stepJump(m, true);
  }
  // Three swipes: 0.05–0.25, 0.23–0.43 and 0.48–0.68 of the animation.
  if ((t > 0.0488 && t < 0.2488) || (t > 0.2288 && t < 0.4288) || (t > 0.4788 && t < 0.6788)) {
    if (m.marioCheckObjectGrab()) return true;
    m.flags |= MARIO_PUNCHING;
  }
  if (m.intendedMag > 0 && t < 0.5788) {
    const turn = toS16(m.intendedYaw - m.faceAngle[1]);
    m.faceAngle[1] = toS16(m.faceAngle[1] + clamp(turn, -0x400, 0x400));
  }
  const target = t < 0.5788 ? CLAW_LUNGE : 0;
  if (!groundStep(m, approachF32(m.forwardVel, target, 1.5, 1.5))) return true;
  if (m.actionTimer >= CLAW_FRAMES) m.setAction(ACT_IDLE);
  return true;
}

function stepRoll(m: MarioRuntime): boolean {
  const c = m.controller;
  const t = m.actionTimer++ / ROLL_FRAMES;
  if (c.buttonPressed & A_BUTTON) {
    start(m, ACT_JUMP, BANJO_JUMP);
    m.vel[1] = JUMP_VELOCITY;
    m.jumpTakeoffSpeed = Math.max(MAX_WALK, m.forwardVel);
    return stepJump(m, true);
  }
  const rolling = t < 0.8011;
  if (rolling) m.flags |= MARIO_TRIPPING;
  if (m.intendedMag > 0) {
    const turn = toS16(m.intendedYaw - m.faceAngle[1]);
    m.faceAngle[1] = toS16(m.faceAngle[1] + clamp(turn, -0x300, 0x300));
  }
  if (!groundStep(m, approachF32(m.forwardVel, rolling ? ROLL_SPEED : 0, 2, rolling ? 2 : 3))) return true;
  if (m.actionTimer >= ROLL_FRAMES) m.setAction(m.forwardVel > 1 ? ACT_WALKING : ACT_IDLE);
  return true;
}

function stepBarge(m: MarioRuntime): boolean {
  const c = m.controller;
  const timer = m.actionTimer++;
  // actionState: 0 wind-up, 1 charging (B held for the full 850), 2 sliding to a stop.
  if (!(c.buttonDown & B_BUTTON) && m.actionState === 0) m.actionState = -1;
  let forward = m.forwardVel;
  if (timer < BARGE_WINDUP) forward = approachF32(forward, 0, 3, 3);
  else if (timer === BARGE_WINDUP) {
    if (m.intendedMag > 0) m.faceAngle[1] = m.intendedYaw;
    forward = m.actionState === -1 ? BARGE_SHORT : BARGE_FULL;
    m.actionState = 1;
  } else if (forward > BARGE_END) forward = Math.max(BARGE_END, forward - (timer > BARGE_WINDUP + 3 ? BARGE_DECAY : 0));
  else forward = approachF32(forward, 0, 1, 1);
  if (timer >= BARGE_WINDUP && forward > BARGE_END) m.flags |= MARIO_PUNCHING;
  if (!groundStep(m, forward)) return true;
  if (m.actionTimer >= BARGE_FRAMES) m.setAction(ACT_IDLE);
  return true;
}

function stepPeck(m: MarioRuntime): boolean {
  const timer = m.actionTimer++;
  // Two pecks per 0.35 s loop of BSBPECK_ENTER, each with a little hop.
  if (timer < PECK_FRAMES) {
    m.flags |= MARIO_KICKING;
    const loop = (timer % seconds(0.35)) / seconds(0.35);
    if (timer > 0 && (Math.abs(loop - 0.1621) < 0.05 || Math.abs(loop - 0.7) < 0.05)) m.vel[1] = PECK_HOP + PECK_GRAVITY;
  }
  airControl(m, Math.max(1, m.forwardVel * 0.1), 2);
  if (timer >= PECK_END) {
    m.setAction(ACT_FREEFALL);
    return false;
  }
  airStep(m, PECK_GRAVITY);
  return true;
}
