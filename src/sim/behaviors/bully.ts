/** Small Bullies from Lethal Lava Land: charge Mario and knock him back. */

import { atan2s } from '../math';
import { INTERACT_DAMAGE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const BULLY_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 80,
  height: 120,
  hurtboxRadius: 70,
  hurtboxHeight: 110,
};

export function initBully(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(BULLY_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function bullyBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.distanceToMario < 1600) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.forwardVel = 7;
  } else {
    object.forwardVel = 0;
  }
  object.stepPhysics(ctx.world);
}
