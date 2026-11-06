/** Static red flame hazard used in the haunted mansion. */

import { INTERACT_DAMAGE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const FLAME_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 55,
  height: 110,
  hurtboxRadius: 45,
  hurtboxHeight: 100,
};

export function initFlame(object: SimObject): void {
  object.gravity = 0;
  object.friction = 0;
  object.setHitbox(FLAME_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function flameBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  object.pos[1] = object.home[1] + Math.sin(Number(object.vars.timer) * 0.12) * 6;
  object.scale = 1 + Math.sin(Number(object.vars.timer) * 0.2) * 0.08;
  void ctx;
}
