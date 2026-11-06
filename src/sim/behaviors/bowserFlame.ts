/** Bowser's fire breath projectile. */

import { INTERACT_DAMAGE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const FLAME_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 90,
  height: 110,
  hurtboxRadius: 80,
  hurtboxHeight: 100,
};

export function initBowserFlame(object: SimObject): void {
  object.gravity = 0;
  object.friction = 0;
  object.scale = 1.5;
  object.setHitbox(FLAME_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function bowserFlameBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  const speed = 7;
  object.pos[0] += Math.sin(object.moveYaw * Math.PI * 2 / 65536) * speed;
  object.pos[2] += Math.cos(object.moveYaw * Math.PI * 2 / 65536) * speed;
  object.pos[1] = object.home[1] + Math.sin(Number(object.vars.timer) * 0.1) * 8;
  if (Number(object.vars.timer) > 100) object.deactivate(-1);
  void ctx;
}
