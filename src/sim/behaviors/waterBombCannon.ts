/**
 * Water bomb cannon, simplified from
 * reference/sm64/src/game/behaviors/water_bomb_cannon.inc.c and
 * water_bomb.inc.c. The cannon acquires Mario within the source 2000-unit
 * range and fires arcing water bombs; each bomb explodes on ground contact.
 */

import { atan2s } from '../math';
import type { ObjectContext, ObjectHitbox, SimObject } from '../object';

const CANNON_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 150,
  height: 150,
  hurtboxRadius: 150,
  hurtboxHeight: 150,
};

const BOMB_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 1,
  numLootCoins: 0,
  radius: 80,
  height: 50,
  hurtboxRadius: 60,
  hurtboxHeight: 50,
};

export function initWaterBombCannon(object: SimObject): void {
  object.setHitbox(CANNON_HITBOX);
  object.graphYOffset = 0;
  object.gravity = 0;
  object.vars.timer = 0;
  object.visible = true;
}

export function initCannonBarrel(object: SimObject): void {
  object.graphYOffset = 100;
  object.gravity = 0;
  object.vars.timer = 0;
  object.visible = true;
}

export function initWaterBomb(object: SimObject): void {
  object.setHitbox(BOMB_HITBOX);
  object.graphYOffset = 40;
  object.gravity = 4;
  object.vars.timer = 0;
  object.visible = true;
}

export function waterBombCannonBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.distanceToMario > 2000) {
    object.vars.timer = 0;
    return;
  }
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (Number(object.vars.timer) < 90) return;
  object.vars.timer = 0;

  const angle = atan2s(ctx.mario.pos[0] - object.pos[0], ctx.mario.pos[2] - object.pos[2]);
  const bomb = ctx.spawn({
    behavior: 'waterBomb',
    model: 'actors/water_bubble/water_bomb_geo.glb',
    pos: [object.pos[0], object.pos[1] + 180, object.pos[2]],
    yaw: angle,
  });
  bomb.faceYaw = angle;
  bomb.moveYaw = angle;
  bomb.forwardVel = 28;
  bomb.vel[1] = 70;
}

export function cannonBarrelBehavior(object: SimObject, ctx: ObjectContext): void {
  const angle = atan2s(ctx.mario.pos[0] - object.pos[0], ctx.mario.pos[2] - object.pos[2]);
  object.faceYaw = angle;
  object.moveYaw = angle;
}

export function waterBombBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  object.stepPhysics(ctx.world);
  if (object.grounded || Number(object.vars.timer) > 150) {
    ctx.explode(object, 220, 1);
    object.deactivate(-1);
  }
}
