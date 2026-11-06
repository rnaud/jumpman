/**
 * Whomp enemies, simplified from Whomp's Fortress. Small Whomps are one-hit
 * enemies; the Whomp King is a three-hit boss that unlocks the WF star.
 */

import { atan2s } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

const SMALL_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE | INTERACT_BREAKABLE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 1,
  numLootCoins: 0,
  radius: 90,
  height: 150,
  hurtboxRadius: 80,
  hurtboxHeight: 140,
};

const KING_HITBOX: ObjectHitbox = {
  ...SMALL_HITBOX,
  health: 3,
  radius: 180,
  height: 300,
  hurtboxRadius: 160,
  hurtboxHeight: 280,
};

export function initWhomp(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.scale = 2;
  object.setHitbox(SMALL_HITBOX);
  object.vars.health = 1;
  object.vars.timer = 0;
  object.visible = true;
}

export function initWhompKing(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.scale = 3;
  object.setHitbox(KING_HITBOX);
  object.vars.health = 3;
  object.vars.timer = 0;
  object.visible = true;
}

export function whompBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    const health = Number(object.vars.health ?? 1) - 1;
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
  if (object.distanceToMario < 1400) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.forwardVel = 3;
  } else {
    object.forwardVel = 0;
  }
  object.stepPhysics(ctx.world);
}

export function whompKingBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    const health = Number(object.vars.health ?? 3) - 1;
    if (health <= 0) {
      ctx.whompKingDefeated(object.pos);
      object.deactivate(-1);
      return;
    }
    object.vars.health = health;
    object.vars.flash = 12;
    object.pos[0] += Math.sin(object.faceYaw * Math.PI * 2 / 65536) * 40;
    object.pos[2] += Math.cos(object.faceYaw * Math.PI * 2 / 65536) * 40;
    return;
  }
  if (Number(object.vars.flash ?? 0) > 0) object.vars.flash--;
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.distanceToMario < 1800) {
    object.moveYaw = atan2s(ctx.mario.pos[2] - object.pos[2], ctx.mario.pos[0] - object.pos[0]);
    object.faceYaw = object.moveYaw;
    object.forwardVel = 2.4;
  } else {
    object.forwardVel = 0;
  }
  object.stepPhysics(ctx.world);
}
