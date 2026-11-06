/** Small wandering penguin used by Cool, Cool Mountain. */

import type { ObjectContext, ObjectHitbox, SimObject } from '../object';

const PENGUIN_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 45,
  height: 90,
  hurtboxRadius: 40,
  hurtboxHeight: 80,
};

export function initPenguin(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(PENGUIN_HITBOX);
  object.vars.dir = 1;
  object.vars.timer = 0;
  object.visible = true;
}

export function penguinBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (Number(object.vars.timer) % 150 === 0) object.vars.dir = -Number(object.vars.dir ?? 1);
  object.forwardVel = 1.4 * Number(object.vars.dir ?? 1);
  object.faceYaw = object.moveYaw;
  const step = object.stepPhysics(ctx.world);
  if (step === 1) {
    object.vars.dir = -Number(object.vars.dir ?? 1);
    object.forwardVel = -object.forwardVel;
  }
}
