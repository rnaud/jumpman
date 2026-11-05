import { marioOverlapsObject, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const WARP_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 80,
  height: 200,
  hurtboxRadius: 80,
  hurtboxHeight: 200,
};

export function initWarp(object: SimObject): void {
  object.setHitbox(WARP_HITBOX);
  object.gravity = 0;
  object.visible = false;
}

export function warpBehavior(object: SimObject, ctx: ObjectContext): void {
  if (!object.warpTo) return;
  if (!marioOverlapsObject(ctx.mario, object)) return;
  ctx.warp(object.warpTo);
  object.deactivate(-1);
}
