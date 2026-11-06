/** 1-Up mushroom pickup. */

import { marioOverlapsObject, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const ONE_UP_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 60,
  height: 80,
  hurtboxRadius: 50,
  hurtboxHeight: 70,
};

export function initOneUp(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(ONE_UP_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function oneUpBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  object.pos[1] = object.home[1] + Math.sin(Number(object.vars.timer) * 0.06) * 8;
  if (!marioOverlapsObject(ctx.mario, object)) return;
  ctx.mario.lives = Math.min(100,ctx.mario.lives + 1);
  ctx.oneUpCollected(object.pos);
  object.deactivate(-1);
}
