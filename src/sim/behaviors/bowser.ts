/** Simplified Bowser arena boss: chase, contact damage and three hits. */

import { atan2s } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

const BOWSER_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE | INTERACT_BREAKABLE,
  downOffset: 0,
  damageOrCoinValue: 2,
  health: 3,
  numLootCoins: 0,
  radius: 230,
  height: 420,
  hurtboxRadius: 210,
  hurtboxHeight: 400,
};

export function initBowser(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.scale = 4;
  object.setHitbox(BOWSER_HITBOX);
  object.vars.health = 3;
  object.vars.timer = 0;
  object.visible = true;
}

export function bowserBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    const health = Number(object.vars.health ?? 3) - 1;
    if (health <= 0) {
      ctx.bowserDefeated(object.pos);
      object.deactivate(-1);
      return;
    }
    object.vars.health = health;
    object.vars.flash = 15;
    object.pos[0] += Math.sin(object.faceYaw * Math.PI * 2 / 65536) * 80;
    object.pos[2] += Math.cos(object.faceYaw * Math.PI * 2 / 65536) * 80;
    return;
  }
  if (Number(object.vars.flash ?? 0) > 0) object.vars.flash--;
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.distanceToMario < 3000) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.forwardVel = 3.5;
  } else {
    object.forwardVel = 0;
  }
  object.stepPhysics(ctx.world);
}
