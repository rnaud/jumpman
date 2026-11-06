import { atan2s, toS16, type Vec3 } from '../math';
import { marioSetForwardVel, stopAndSetHeightToFloor } from '../marioStep';
import type { MarioRuntime } from '../MarioRuntime';
import { AIR_STEP_LANDED, INPUT_A_DOWN, MARIO_WING_CAP, MARIO_CAP_ON_HEAD, MARIO_CAP_IN_HAND } from '../state';

// sJumboStarKeyframes in mario_actions_cutscene.c. Speeds are in thousandths per tick.
const flightKeys = [
  [20, 0, 678, -2916], [30, 0, 680, -3500], [40, 1000, 700, -4000],
  [50, 2500, 750, -3500], [50, 3500, 800, -2000], [50, 4000, 850, 0],
  [50, 3500, 900, 2000], [50, 2000, 950, 3500], [50, 0, 1000, 4000],
  [50, -2000, 1050, 3500], [50, -3500, 1100, 2000], [50, -4000, 1150, 0],
  [50, -3500, 1200, -2000], [50, -2000, 1250, -3500], [50, 0, 1300, -4000],
  [50, 2000, 1350, -3500], [50, 3500, 1400, -2000], [50, 4000, 1450, 0],
  [50, 3500, 1500, 2000], [50, 2000, 1600, 3500], [50, 0, 1700, 4000],
  [50, -2000, 1800, 3500], [50, -3500, 1900, 2000], [30, -4000, 2000, 0],
  [0, -3500, 2100, -2000], [0, -2000, 2200, -3500], [0, 0, 2300, -4000],
];
// Root translation of anim_25, divided by four as in update_mario_pos_for_anim.
const x25 = [0];
const y25 = [42, 42, 42, 42, 42, 42, 42, 43, 43, 43, 43, 43, 43, 43, 43, 44, 44, 44, 44, 44, 44, 44, 44, 45, 45, 45, 45, 45, 45, 45, 45, 46, 46, 46, 46, 46, 46, 46, 46, 46, 47, 47, 47, 47, 47, 47, 47, 47, 46, 46, 46, 45, 44, 43, 42, 40, 38, 36, 34, 33, 31, 30, 30, 27, 26, 40, 65, 88, 108, 127, 143, 158, 171, 182, 192, 200, 207, 212, 217, 220, 223, 224, 225, 224, 222, 218, 212, 205, 196, 186, 175, 162, 148, 132, 116, 98, 79, 60, 39, 29];
const z25 = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 7, 9, 11, 9, 6, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4];
// Root translation of anim_26, divided by four as in update_mario_pos_for_anim.
const x26 = [0];
const y26 = [29, 29, 29, 77, 119, 154, 184, 209, 229, 245, 258, 267, 273, 278, 280, 281, 281, 281, 278, 273, 266, 256, 242, 225, 205, 180, 150, 115, 75, 30, 29, 29, 30, 76, 118, 155, 188, 218, 243, 265, 284, 299, 312, 322, 330, 336, 341, 343, 344, 344, 338, 325, 306, 281, 253, 221, 188, 153, 119, 86, 55, 29, 29, 29, 29, 29, 75, 117, 156, 191, 223, 252, 278, 300, 320, 338, 353, 365, 376, 384, 391, 396, 399, 401, 401, 400, 399, 396, 393, 389, 385, 380, 376, 371];
const z26 = [4];

// Uniform clamped cubic, with the decomp's special first and last two segments.
function splineWeights(segment: number, t: number): number[] {
  const n = flightKeys.length - 4;
  if (segment >= n - 1) return splineWeights(n - segment, 1 - t).reverse();
  const a = t*t, b = a*t, c = (1-t)**3;
  if (segment === 0) return [c, b*1.75-a*4.5+t*3, -b*11/12+a*1.5, b/6];
  if (segment === 1) return [c/4, b*7/12-a*1.25+t/4+7/12, -b/2+a/2+t/2+1/6, b/6];
  return [c/6, b/2-a+2/3, -b/2+a/2+t/2+1/6, b/6];
}

// The path and its speed are fixed. Sample it once at the source's 30 Hz clock.
function flightPath(): Vec3[] {
  const path: Vec3[] = [];
  let t = 0;
  for (let segment = 0; segment < flightKeys.length - 3; segment++) {
    while (t < 1) {
      const weights = splineWeights(segment, t);
      const pos: Vec3 = [0, 0, 0];
      for (let i = 0; i < 4; i++) for (let axis = 0; axis < 3; axis++) pos[axis] += weights[i] * flightKeys[segment+i][axis+1];
      path.push(pos);
      t = Math.fround(t + Math.fround(flightKeys[segment][0] / 1000));
    }
    t = Math.fround(t - 1);
  }
  return path;
}

const flight = flightPath();

function advance(m: MarioRuntime): void {
  m.actionArg++;
  m.actionTimer = m.actionState = 0;
}

function wingLanding(m: MarioRuntime, stepArg: number): boolean {
  m.input |= INPUT_A_DOWN;
  m.flags |= MARIO_WING_CAP | MARIO_CAP_ON_HEAD;
  marioSetForwardVel(m, 0);
  return m.stepAir(stepArg) === AIR_STEP_LANDED;
}

export function jumboStarCutscene(m: MarioRuntime): boolean {
  m.flags |= MARIO_WING_CAP | MARIO_CAP_ON_HEAD;
  if (m.actionArg === 0) {
    m.faceAngle[1] = -0x8000;
    m.pos[0] = m.pos[2] = 0;
    if (!m.actionState) {
      if (wingLanding(m, 1)) m.actionState = 1;
    } else if (++m.actionTimer >= 15) advance(m);
  } else if (m.actionArg === 1) {
    const t = m.actionTimer++;
    const takeoff = m.actionState === 1;
    const root = takeoff ? [x26,y26,z26] : [x25,y25,z25];
    const translation = root.map(frames => frames[Math.min(t, frames.length-1)]);
    m.pos = [-translation[0], 307 + translation[1], -translation[2] - (takeoff ? Math.max(0,t-2)*32 : 0)];
    if (m.actionTimer === (takeoff ? 94 : 100)) {
      if (takeoff) advance(m);
      else { m.actionState = 1; m.actionTimer = 0; }
    }
  } else {
    const t = m.actionTimer++;
    const next = flight[t];
    if (next) {
      const [dx, dy, dz] = next.map((v,i) => v - m.pos[i]);
      const yaw = atan2s(dz, dx);
      m.faceAngle = [-atan2s(Math.hypot(dx,dz), dy), yaw, toS16(toS16(m.faceAngle[1] - yaw) * 20)];
      m.pos = [...next];
    }
    if (t === 530) m.events.push({type: 'castleEnding'});
  }
  return false;
}

export function endPeachCutscene(m: MarioRuntime): boolean {
  const t = m.actionTimer++;
  switch (m.actionArg) {
    case 0:
      if (wingLanding(m, 0)) advance(m);
      break;
    case 1:
      m.flags |= MARIO_WING_CAP | MARIO_CAP_ON_HEAD;
      stopAndSetHeightToFloor(m);
      if (t === 14) { m.wingCapTimer = 60; advance(m); }
      break;
    case 2: if (t === 255) advance(m); break;
    case 3: if (t === 355) advance(m); break;
    case 4: if (t === 584) advance(m); break;
    case 5:
      m.pos[2] -= 20;
      if (m.pos[2] <= -1181) { m.pos[2] = -1180; advance(m); }
      m.pos[1] = m.world.findFloor(...m.pos).height;
      break;
    case 6:
      if (t === 8) m.flags = (m.flags & ~MARIO_CAP_ON_HEAD) | MARIO_CAP_IN_HAND;
      if (t === 480) advance(m);
      break;
    case 7: if (t === 200) advance(m); break;
    case 8: if (t === 140) advance(m); break;
    case 9:
      if (t === 77) m.flags = (m.flags & ~MARIO_CAP_IN_HAND) | MARIO_CAP_ON_HEAD;
      if (t === 142) advance(m);
      break;
    case 10: if (t === 350) advance(m); break;
    case 11: if (t === 389) advance(m); break;
    case 12: if (t === 0) m.events.push({type: 'endingCredits'}); break;
  }
  return false;
}
