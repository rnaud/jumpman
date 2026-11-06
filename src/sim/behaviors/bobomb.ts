import { spawnMovingYellowCoins } from './movingYellowCoin';
/**
 * Bob-omb behavior, ported from reference/sm64/src/game/behaviors/bobomb.inc.c.
 *
 * The generic Bob-omb patrols, chases Mario, is picked up and thrown, lights
 * its fuse while held, and explodes after landing. The stationary variant (the
 * caged Bob-ombs in Bob-omb Battlefield) never patrols.
 */

import {
  HELD_DROPPED,
  HELD_FREE,
  HELD_HELD,
  HELD_THROWN,
  INT_STATUS_MARIO_KNOCKBACK_DMG,
  INT_STATUS_TOUCHED_BOB_OMB,
  INT_STATUS_WAS_ATTACKED,
  INT_SUBTYPE_KICKABLE,
  INTERACT_GRABBABLE,
  returnHomeIfSafe,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

export const BOBOMB_ACT_PATROL = 0;
export const BOBOMB_ACT_LAUNCHED = 1;
export const BOBOMB_ACT_CHASE_MARIO = 2;
export const BOBOMB_ACT_EXPLODE = 3;

export const BOBOMB_BP_STYPE_GENERIC = 0;
export const BOBOMB_BP_STYPE_STATIONARY = 1;

const BOBOMB_HITBOX: ObjectHitbox = {
  interactType: INTERACT_GRABBABLE,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 65,
  height: 113,
  hurtboxRadius: 0,
  hurtboxHeight: 0,
};

export function initBobomb(object: SimObject): void {
  object.gravity = 2.5;
  object.friction = 0.8;
  object.buoyancy = 1.3;
  object.interactSubtype = INT_SUBTYPE_KICKABLE;
  object.setHitbox(BOBOMB_HITBOX);
  object.setAction(BOBOMB_ACT_PATROL);
  object.vars.fuseLit = 0;
  object.vars.fuseTimer = 0;
  object.vars.blinkTimer = 0;
}

function checkInteractions(object: SimObject, ctx: ObjectContext): void {
  object.setHitbox(BOBOMB_HITBOX);

  if (object.interactStatus & INT_STATUS_MARIO_KNOCKBACK_DMG) {
    object.moveYaw = ctx.mario.faceAngle[1];
    object.forwardVel = 25;
    object.vel[1] = 30;
    object.setAction(BOBOMB_ACT_LAUNCHED);
  }

  if (object.interactStatus & INT_STATUS_TOUCHED_BOB_OMB) {
    object.setAction(BOBOMB_ACT_EXPLODE);
  }

  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.forwardVel = 20;
    object.vel[1] = 20;
    object.setAction(BOBOMB_ACT_LAUNCHED);
  }

  object.interactStatus = 0;
}

function explode(object: SimObject, ctx: ObjectContext): void {
  object.actionTimer++;
  if (object.actionTimer < 5) {
    object.scale = 1 + object.actionTimer / 5;
    return;
  }

  object.scale = 1;
  ctx.explode(object, 240, 1);
  if (!object.coinDropped) {
    spawnMovingYellowCoins(object,ctx,1);
    object.coinDropped = true;
  }
  object.waitToRespawn(3000);
}

function freeLoop(object: SimObject, ctx: ObjectContext): void {
  switch (object.action) {
    case BOBOMB_ACT_PATROL:
      object.forwardVel = 5;
      object.stepPhysics(ctx.world);
      if (
        returnHomeIfSafe(object, ctx.mario, 400)
        && object.facingTowardAngle(object.moveYaw, object.angleToMario, 0x2000)
      ) {
        object.vars.fuseLit = 1;
        object.setAction(BOBOMB_ACT_CHASE_MARIO);
      }
      break;

    case BOBOMB_ACT_LAUNCHED:
      object.stepPhysics(ctx.world);
      if (object.grounded) object.setAction(BOBOMB_ACT_EXPLODE);
      break;

    case BOBOMB_ACT_CHASE_MARIO:
      object.forwardVel = 20;
      object.stepPhysics(ctx.world);
      object.turnTowardAngle(object.angleToMario, 0x800);
      break;

    case BOBOMB_ACT_EXPLODE:
      explode(object, ctx);
      break;

    default:
      object.setAction(BOBOMB_ACT_PATROL);
      break;
  }

  checkInteractions(object, ctx);

}

function heldLoop(object: SimObject, ctx: ObjectContext): void {
  object.vars.fuseLit = 1;
  object.visible = true;
  if (object.vars.fuseTimer > 150) {
    object.setAction(BOBOMB_ACT_EXPLODE);
    ctx.mario.dropHeldObject();
  }
}

function thrownLoop(object: SimObject): void {
  object.heldState = HELD_FREE;
  object.forwardVel = 25;
  object.vel[1] = 20;
  object.setAction(BOBOMB_ACT_LAUNCHED);
}

function droppedLoop(object: SimObject): void {
  object.heldState = HELD_FREE;
  object.forwardVel = 0;
  object.setAction(BOBOMB_ACT_PATROL);
}

export function bobombBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.distanceToMario < 4000) {
    switch (object.heldState) {
      case HELD_HELD:
        heldLoop(object, ctx);
        break;
      case HELD_THROWN:
        thrownLoop(object);
        break;
      case HELD_DROPPED:
        if (object.action === BOBOMB_ACT_EXPLODE) explode(object, ctx);
        else droppedLoop(object);
        break;
      default:
        if (!object.coinDropped && (object.param & 0xff) === BOBOMB_BP_STYPE_STATIONARY) {
          if (object.action === BOBOMB_ACT_EXPLODE) {
            explode(object, ctx);
          } else if (object.action === BOBOMB_ACT_LAUNCHED) {
            object.stepPhysics(ctx.world);
            if (object.grounded) object.setAction(BOBOMB_ACT_EXPLODE);
            checkInteractions(object, ctx);
          } else {
            checkInteractions(object, ctx);
          }
        } else {
          freeLoop(object, ctx);
        }
        if (object.vars.fuseTimer > 150 && object.action !== BOBOMB_ACT_EXPLODE) object.setAction(BOBOMB_ACT_EXPLODE);
        break;
    }
    if (object.vars.fuseLit) object.vars.fuseTimer++;
  }

  object.faceYaw = object.moveYaw;
}
