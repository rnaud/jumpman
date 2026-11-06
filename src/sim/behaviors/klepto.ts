/** Simplified Klepto: circles above the desert and dives at Mario when close. */

import { atan2s } from '../math';
import { INTERACT_DAMAGE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const KLETPO_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 90,
  height: 120,
  hurtboxRadius: 80,
  hurtboxHeight: 110,
};

export function initKlepto(object: SimObject): void {
  object.gravity = 0;
  object.friction = 0;
  object.scale = 2;
  object.setHitbox(KLETPO_HITBOX);
  object.vars.timer = Math.random() * 100;
  object.visible = true;
}

export function kleptoBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  const t = Number(object.vars.timer);
  if (object.distanceToMario < 900) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.pos[0] += Math.sin(object.moveYaw * Math.PI * 2 / 65536) * 4;
    object.pos[2] += Math.cos(object.moveYaw * Math.PI * 2 / 65536) * 4;
  } else {
    const angle = t * 0.012;
    object.pos[0] = object.home[0] + Math.sin(angle) * 500;
    object.pos[2] = object.home[2] + Math.cos(angle) * 500;
  }
  object.pos[1] = object.home[1] + Math.sin(t * 0.04) * 120;
  object.faceYaw = object.moveYaw;
}
