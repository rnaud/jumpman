/** Simplified Boo: drifts toward Mario and damages on contact. */

import { atan2s, type Vec3 } from '../math';
import { INTERACT_DAMAGE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';
import { MARIO_VANISH_CAP } from '../state';

const BOO_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 80,
  height: 130,
  hurtboxRadius: 70,
  hurtboxHeight: 120,
};

export function initBoo(object: SimObject): void {
  object.gravity = 0;
  object.friction = 0;
  object.setHitbox(BOO_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function booBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  const dx = ctx.mario.pos[0] - object.pos[0];
  const dz = ctx.mario.pos[2] - object.pos[2];
  const distance = Math.hypot(dx, dz);
  const invisible = (ctx.mario.flags & MARIO_VANISH_CAP) !== 0;
  if (distance < 2600 && !invisible) {
    const yaw = atan2s(dz, dx);
    object.moveYaw = yaw;
    object.faceYaw = yaw;
    object.forwardVel = distance > 110 ? 6 : 0;
    if (distance > 110) {
      const speed = Math.min(6, distance - 100);
      object.pos[0] += Math.sin(yaw * Math.PI * 2 / 65536) * speed;
      object.pos[2] += Math.cos(yaw * Math.PI * 2 / 65536) * speed;
    }
  } else {
    object.forwardVel = 0;
  }
  object.pos[1] = object.home[1] + Math.sin(Number(object.vars.timer) * 0.045) * 22;
  object.visible = true;
  void (object.pos as Vec3);
}
