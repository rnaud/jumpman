/** Mr. Blizzard snowman enemy, simplified from the CCM behavior. */

import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

const BLIZZARD_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE | INTERACT_BREAKABLE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 1,
  numLootCoins: 0,
  radius: 100,
  height: 170,
  hurtboxRadius: 90,
  hurtboxHeight: 150,
};

export function initMrBlizzard(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.scale = 2;
  object.setHitbox(BLIZZARD_HITBOX);
  object.vars.timer = 0;
  object.visible = true;
}

export function mrBlizzardBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    ctx.spawnCoin(object.pos);
    object.deactivate(-1);
    return;
  }
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.vars.timer % 90 === 0 && object.distanceToMario < 900) {
    object.vel[1] = 24;
    object.forwardVel = 4;
    object.moveYaw = object.angleToMario;
    object.faceYaw = object.moveYaw;
  }
  object.stepPhysics(ctx.world);
}
