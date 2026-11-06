/** Big Boo boss: a larger, punchable Boo used by the BBH ghost-hunt mission. */

import { atan2s } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';
import { MARIO_VANISH_CAP } from '../state';

const BIG_BOO_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE | INTERACT_BREAKABLE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 3,
  numLootCoins: 0,
  radius: 150,
  height: 260,
  hurtboxRadius: 120,
  hurtboxHeight: 240,
};

export function initBigBoo(object: SimObject): void {
  object.gravity = 0;
  object.friction = 0;
  object.scale = 3;
  object.setHitbox(BIG_BOO_HITBOX);
  object.vars.health = 3;
  object.vars.timer = 0;
  object.visible = true;
}

export function bigBooBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    object.vars.health = Number(object.vars.health ?? 3) - 1;
    object.vars.flash = 12;
    if (Number(object.vars.health) <= 0) {
      ctx.bigBooDefeated(object.pos);
      object.deactivate(-1);
      return;
    }
    object.pos[0] += Math.sin(object.angleToMario) * 30;
    object.pos[2] += Math.cos(object.angleToMario) * 30;
    return;
  }

  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (Number(object.vars.flash ?? 0) > 0) object.vars.flash--;
  const dx = ctx.mario.pos[0] - object.pos[0];
  const dz = ctx.mario.pos[2] - object.pos[2];
  const distance = Math.hypot(dx, dz);
  if (distance < 2600 && !(ctx.mario.flags & MARIO_VANISH_CAP)) {
    object.moveYaw = atan2s(dz, dx);
    object.faceYaw = object.moveYaw;
    if (distance > 240) {
      const speed = Math.min(4, distance - 220);
      object.pos[0] += Math.sin(object.moveYaw * Math.PI * 2 / 65536) * speed;
      object.pos[2] += Math.cos(object.moveYaw * Math.PI * 2 / 65536) * speed;
    }
  }
  object.pos[1] = object.home[1] + Math.sin(Number(object.vars.timer) * 0.035) * 28;
  object.scale = 3 + Math.sin(Number(object.vars.timer) * 0.08) * 0.08;
  object.visible = true;
}
