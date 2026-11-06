import { ACT_FLAG_INTANGIBLE } from '../state';
import { marioOverlapsObject, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const WARP_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 50,
  height: 50,
  hurtboxRadius: 0,
  hurtboxHeight: 0,
};

export function initWarp(object: SimObject): void {
  const radius = object.param === 0xff ? 10000 : object.param * 10 || 50;
  object.setHitbox({ ...WARP_HITBOX, radius });
  object.gravity = 0;
  object.visible = false;
}

export function warpBehavior(object: SimObject, ctx: ObjectContext): void {
  if (!object.warpTo || (ctx.mario.action & ACT_FLAG_INTANGIBLE)) return;
  if (!marioOverlapsObject(ctx.mario, object)) return;
  ctx.warp(object.warpTo, object.warpEntry, object.warpArea);
  if (!object.warpArea) object.deactivate(-1);
}
