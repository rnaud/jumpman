/** Simplified Pokey cactus: a multi-hit walking hazard. */

import { atan2s } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

const POKEY_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE | INTERACT_BREAKABLE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 3,
  numLootCoins: 0,
  radius: 100,
  height: 260,
  hurtboxRadius: 90,
  hurtboxHeight: 240,
};

export function initPokey(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.scale = 1.6;
  object.setHitbox(POKEY_HITBOX);
  object.vars.health = 3;
  object.vars.timer = 0;
  object.visible = true;
}

export function pokeyBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    const health = Number(object.vars.health ?? 3) - 1;
    if (health <= 0) {
      ctx.spawnCoin(object.pos);
      object.deactivate(-1);
      return;
    }
    object.vars.health = health;
    object.vars.flash = 12;
    return;
  }
  if (Number(object.vars.flash ?? 0) > 0) object.vars.flash--;
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.distanceToMario < 1300) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.forwardVel = 2;
  } else {
    object.forwardVel = 0;
  }
  object.stepPhysics(ctx.world);
}
