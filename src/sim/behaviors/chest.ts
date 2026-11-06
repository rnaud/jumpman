/** JRB treasure chest: opens with B while Mario is standing over it. */

import { B_BUTTON } from '../state';
import { marioOverlapsObject, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const CHEST_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 150,
  height: 180,
  hurtboxRadius: 140,
  hurtboxHeight: 170,
};

export function initChest(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(CHEST_HITBOX);
  object.vars.opened = 0;
  object.visible = true;
}

export function chestBehavior(object: SimObject, ctx: ObjectContext): void {
  if (Number(object.vars.opened ?? 0) !== 0) return;
  if (!marioOverlapsObject(ctx.mario, object)) return;
  if (!(ctx.mario.controller.buttonPressed & B_BUTTON)) return;
  object.vars.opened = 1;
  ctx.chestOpened(object.pos);
}
