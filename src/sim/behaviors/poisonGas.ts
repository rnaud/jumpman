/** Poison gas area hazard, simplified from the source HMC/BBH gas behavior. */

import { type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const GAS_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 120,
  height: 220,
  hurtboxRadius: 0,
  hurtboxHeight: 0,
};

export function initPoisonGas(object: SimObject): void {
  object.gravity = 0;
  object.setHitbox(GAS_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function poisonGasBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  const dx = ctx.mario.pos[0] - object.pos[0];
  const dz = ctx.mario.pos[2] - object.pos[2];
  const dy = ctx.mario.pos[1] - object.pos[1];
  if (dx * dx + dz * dz > 140 * 140 || dy < -40 || dy > 220) return;
  if (Number(object.vars.timer) % 45 !== 0) return;
  ctx.mario.takeDamage(1, object.pos);
}
