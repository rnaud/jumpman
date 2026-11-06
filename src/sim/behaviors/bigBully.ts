/** Big Bully boss from Lethal Lava Land: three hits, strong contact damage. */

import { atan2s } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

const BIG_BULLY_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE | INTERACT_BREAKABLE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 3,
  numLootCoins: 0,
  radius: 170,
  height: 280,
  hurtboxRadius: 150,
  hurtboxHeight: 260,
};

export function initBigBully(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.scale = 3;
  object.setHitbox(BIG_BULLY_HITBOX);
  object.vars.health = 3;
  object.vars.timer = 0;
  object.visible = true;
}

export function bigBullyBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    const health = Number(object.vars.health ?? 3) - 1;
    if (health <= 0) {
      ctx.bigBullyDefeated(object.pos);
      object.deactivate(-1);
      return;
    }
    object.vars.health = health;
    object.vars.flash = 12;
    object.pos[0] += Math.sin(object.faceYaw * Math.PI * 2 / 65536) * 50;
    object.pos[2] += Math.cos(object.faceYaw * Math.PI * 2 / 65536) * 50;
    return;
  }
  if (Number(object.vars.flash ?? 0) > 0) object.vars.flash--;
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.distanceToMario < 2200) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.forwardVel = 5;
  } else {
    object.forwardVel = 0;
  }
  object.stepPhysics(ctx.world);
}
