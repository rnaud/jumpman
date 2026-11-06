import type { MarioRuntime } from '../MarioRuntime';
import { approachF32, clamp, coss, sins, toS16, type Vec3 } from '../math';
import { ACT_BREASTSTROKE, ACT_FLUTTER_KICK, ACT_FREEFALL, ACT_SWIMMING_END, ACT_WATER_ACTION_END, ACT_WATER_JUMP, ACT_FLAG_STATIONARY, ACT_WATER_IDLE, ACT_WATER_PLUNGE, ACT_WALKING, INPUT_A_DOWN, INPUT_A_PRESSED } from '../state';

/** Normal-cap swimming. The surface is measured at Mario's origin minus his
 * 80-unit head clearance, as in perform_water_step. */
export function submergedStep(m: MarioRuntime, surface: number): void {
  if (surface < m.floorHeight + 80) {
    m.faceAngle[0] = 0;
    m.setAction(m.pos[1] > m.floorHeight + 100 ? ACT_FREEFALL : ACT_WALKING);
    return;
  }
  const pressed = Boolean(m.input & INPUT_A_PRESSED);
  const held = Boolean(m.input & INPUT_A_DOWN);
  const nearSurface = surface - 80 - m.pos[1] < 400;
  const buoyancy = nearSurface ? 1.25 : (m.action & ACT_FLAG_STATIONARY) ? -2 : 0;
  // Transitions execute the new action during the same tick, as in mario_execute_submerged_action.
  for (let transition = 0; transition < 4; transition++) {
    const before = m.action;
    if ((before === ACT_BREASTSTROKE || before === ACT_SWIMMING_END) && pressed
        && Math.trunc(m.pos[1] + 1.5) >= surface - 80 && m.faceAngle[0] >= 0 && m.controller.stickY < -60) {
      m.swimYawVel = 0;
      m.setAction(ACT_WATER_JUMP);
      m.vel[1] = 42;
      return;
    }
    switch (before) {
      case ACT_WATER_IDLE:
      case ACT_WATER_ACTION_END:
        if (pressed) m.setAction(ACT_BREASTSTROKE);
        else if (before === ACT_WATER_ACTION_END && ++m.actionTimer >= 25) m.setAction(ACT_WATER_IDLE);
        break;
      case ACT_BREASTSTROKE:
        if (!m.actionArg) m.swimStrength = 160;
        if (++m.actionTimer === 14) { m.setAction(ACT_FLUTTER_KICK); break; }
        if (m.actionTimer < 6) m.forwardVel += 0.5;
        if (m.actionTimer >= 9) m.forwardVel += 1.5;
        if (m.actionTimer >= 2 && m.actionTimer < 6 && pressed) m.actionState = 1;
        if (m.actionTimer === 9 && m.actionState) {
          m.actionTimer = 1; m.actionState = 0; m.swimStrength = 160;
        }
        if (m.actionTimer === 1) m.events.push({type: 'swimStroke', fast: m.swimStrength !== 160});
        break;
      case ACT_FLUTTER_KICK:
        if (!held) {
          if (!m.actionTimer) m.swimStrength = Math.min(280, m.swimStrength + 10);
          m.setAction(ACT_SWIMMING_END);
        } else {
          m.forwardVel = approachF32(m.forwardVel, 12, 0.1, 0.15);
          m.actionTimer = 1; m.swimStrength = 160;
          if (m.forwardVel < 14) {
            if (m.animFrame === 0 || m.animFrame === 12) m.events.push({ type: 'flutterKick' });
            m.animFrame = (m.animFrame + 1) % 24;
          }
        }
        break;
      case ACT_SWIMMING_END:
        if (m.actionTimer >= 15) m.setAction(ACT_WATER_ACTION_END);
        else if (held && m.actionTimer >= 7) {
          if (m.actionTimer === 7) m.swimStrength = Math.min(280, m.swimStrength + 10);
          m.setAction(ACT_BREASTSTROKE, 1);
        } else {
          if (m.actionTimer >= 7) m.swimStrength = 160;
          m.actionTimer++; m.forwardVel -= 0.25;
        }
        break;
    }
    if (m.action === before) break;
  }
  if (m.action === ACT_WATER_PLUNGE) {
    if (m.actionTimer++ === 0) m.events.push({type: 'waterPlunge'});
    m.swimYawVel = 0;
    m.faceAngle[0] = approachF32(m.faceAngle[0], 0, 0x200, 0x200);
    m.forwardVel = approachF32(m.forwardVel, 0, 1, 1);
    m.vel[1] = approachF32(m.vel[1], buoyancy, 2, 1);
  } else {
    const targetYaw = -Math.trunc(10 * m.controller.stickX);
    if (targetYaw > 0 && m.swimYawVel < 0) m.swimYawVel = Math.min(0x10, m.swimYawVel + 0x40);
    else if (targetYaw < 0 && m.swimYawVel > 0) m.swimYawVel = Math.max(-0x10, m.swimYawVel - 0x40);
    else m.swimYawVel = approachF32(m.swimYawVel, targetYaw,
      targetYaw === 0 ? 64 : targetYaw > 0 ? 16 : 32, targetYaw === 0 ? 64 : targetYaw > 0 ? 32 : 16);
    m.faceAngle[1] = toS16(m.faceAngle[1] + m.swimYawVel);
    m.faceAngle[2] = -m.swimYawVel * 8;
    const pitchVel = m.faceAngle[0] < 0 ? 0x100 : 0x200;
    m.faceAngle[0] = approachF32(m.faceAngle[0], -Math.trunc(m.controller.stickY * 252), pitchVel, pitchVel);
    const idle = Boolean(m.action & ACT_FLAG_STATIONARY);
    m.forwardVel = clamp(m.forwardVel - (idle ? 2 : 0), 0, 28);
    if (m.forwardVel > (idle ? 16 : m.swimStrength / 10)) m.forwardVel -= 0.5;
    m.vel[1] = m.forwardVel * sins(m.faceAngle[0]) + buoyancy;
  }
  const horizontal = m.forwardVel * coss(m.faceAngle[0]);
  m.vel[0] = horizontal * sins(m.faceAngle[1]);
  m.vel[2] = horizontal * coss(m.faceAngle[1]);
  const next: Vec3 = [m.pos[0] + m.vel[0], Math.min(surface - 80, m.pos[1] + m.vel[1]), m.pos[2] + m.vel[2]];
  m.world.resolveAndReturnWallCollisions(next, 10, 110);
  const floor = m.world.findFloor(...next);
  if (floor.surface) {
    const ceil = m.world.vec3fFindCeil(next, floor.height);
    if (ceil.height - floor.height >= 160) {
      next[1] = clamp(next[1], floor.height, ceil.height - 160);
      m.pos = next; m.floor = floor.surface; m.floorHeight = floor.height;
    }
  }
  if (m.action === ACT_WATER_PLUNGE && (m.pos[1] === m.floorHeight || m.vel[1] >= (nearSurface ? 0 : -5) || m.actionTimer > 20)) {
    m.setAction(held ? ACT_FLUTTER_KICK : ACT_WATER_ACTION_END);
  }
}
