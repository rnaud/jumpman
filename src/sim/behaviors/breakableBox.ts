/**
 * Breakable boxes, ported from reference/sm64/src/game/behaviors/
 * breakable_box.inc.c and breakable_box_small.inc.c.
 *
 * The large box is a static breakable object. The small box is grabbable,
 * can be thrown, and breaks when it hits a wall.
 */

import {
  HELD_DROPPED,
  HELD_FREE,
  HELD_HELD,
  HELD_THROWN,
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_GRABBABLE,
  OBJ_COL_FLAG_HIT_WALL,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

export const BREAKABLE_BOX_BP_NO_COINS = 0;
export const BREAKABLE_BOX_BP_3_COINS = 1;
export const BREAKABLE_BOX_BP_5_COINS = 2;
export const BREAKABLE_BOX_BP_GIANT = 3;

const BREAKABLE_BOX_HITBOX: ObjectHitbox = {
  interactType: INTERACT_BREAKABLE,
  downOffset: 20,
  damageOrCoinValue: 0,
  health: 1,
  numLootCoins: 0,
  radius: 150,
  height: 200,
  hurtboxRadius: 150,
  hurtboxHeight: 200,
};

const BREAKABLE_BOX_SMALL_HITBOX: ObjectHitbox = {
  interactType: INTERACT_GRABBABLE,
  downOffset: 20,
  damageOrCoinValue: 0,
  health: 1,
  numLootCoins: 0,
  radius: 150,
  height: 250,
  hurtboxRadius: 150,
  hurtboxHeight: 250,
};

function lootCoins(param: number): number {
  if (param === BREAKABLE_BOX_BP_3_COINS) return 3;
  if (param === BREAKABLE_BOX_BP_5_COINS) return 5;
  return 0;
}

function spawnLoot(object: SimObject, ctx: ObjectContext, count: number): void {
  if (count <= 0) return;
  const step = (Math.PI * 2) / count;
  for (let i = 0; i < count; i++) {
    const angle = i * step;
    ctx.spawnCoin([
      object.pos[0] + Math.sin(angle) * 40,
      object.pos[1] + 80,
      object.pos[2] + Math.cos(angle) * 40,
    ]);
  }
}

export function initBreakableBox(object: SimObject): void {
  object.setHitbox(BREAKABLE_BOX_HITBOX);
  object.hitbox.numLootCoins = lootCoins(object.param & 0xff);
  object.scale = (object.param & 0xff) === BREAKABLE_BOX_BP_GIANT ? 1.5 : 1;
  object.gravity = 0;
  object.friction = 1;
  object.visible = true;
}

export function breakableBoxBehavior(object: SimObject, ctx: ObjectContext): void {
  object.timer++;
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    ctx.breakObject(object);
    spawnLoot(object, ctx, object.hitbox.numLootCoins);
    object.deactivate(-1);
  }
}

export function initBreakableBoxSmall(object: SimObject): void {
  object.gravity = 2.5;
  object.friction = 0.99;
  object.buoyancy = 1.4;
  object.scale = 0.4;
  object.setHitbox(BREAKABLE_BOX_SMALL_HITBOX);
  object.setAction(0);
  object.vars.released = 0;
  object.visible = true;
}

function breakSmallBox(object: SimObject, ctx: ObjectContext): void {
  ctx.breakObject(object);
  spawnLoot(object, ctx, 3);
  object.deactivate(90);
}

export function breakableBoxSmallBehavior(object: SimObject, ctx: ObjectContext): void {
  object.timer++;

  switch (object.heldState) {
    case HELD_HELD:
      object.vars.released = 1;
      return;
    case HELD_THROWN:
      object.heldState = HELD_FREE;
      object.forwardVel = 40;
      object.vel[1] = 20;
      object.setAction(0);
      object.vars.released = 1;
      break;
    case HELD_DROPPED:
      object.heldState = HELD_FREE;
      object.forwardVel = 0;
      object.setAction(0);
      object.vars.released = 1;
      break;
    default:
      break;
  }

  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.interactStatus = 0;
    breakSmallBox(object, ctx);
    return;
  }

  const flags = object.stepPhysics(ctx.world);
  object.faceYaw = object.moveYaw;

  if (flags & OBJ_COL_FLAG_HIT_WALL) {
    breakSmallBox(object, ctx);
  }
}
