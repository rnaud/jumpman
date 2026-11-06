/**
 * King Bob-omb, simplified from
 * reference/sm64/src/game/behaviors/king_bobomb.inc.c.
 *
 * The source fight is built around dialogue, a cutscene camera, grabbing Mario
 * and a three-throw health bar. Jumpman implements the three-throw core:
 * Mario can grab and throw the King; each landing costs one health, and the
 * third landing emits the boss-defeated event that unlocks the summit star.
 */

import { INTERACT_GRABBABLE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';
import { HELD_DROPPED, HELD_FREE, HELD_HELD, HELD_THROWN } from '../object';
import { MARIO_VANISH_CAP } from '../state';

const KING_ACT_IDLE = 0;
const KING_ACT_THROWN = 1;

const KING_HITBOX: ObjectHitbox = {
  interactType: INTERACT_GRABBABLE,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 3,
  numLootCoins: 0,
  radius: 100,
  height: 100,
  hurtboxRadius: 100,
  hurtboxHeight: 100,
};

export function initKingBobomb(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.buoyancy = 2;
  object.setHitbox(KING_HITBOX);
  object.setAction(KING_ACT_IDLE);
  object.vars.health = 3;
  object.vars.thrown = 0;
  object.vars.grabbing = 0;
  object.vars.grabCooldown = 0;
  object.vars.intro = 0;
  object.vars.introTimer = 0;
  object.visible = true;
}

function throwLanding(object: SimObject, ctx: ObjectContext): void {
  if (!object.grounded) return;
  if (object.vars.thrown === 0) return;
  object.vars.thrown = 0;
  object.vars.grabbing = 0;
  object.vars.grabCooldown = 0;
  object.vars.intro = 0;
  object.vars.introTimer = 0;
  object.vars.health--;
  object.forwardVel = 0;
  object.vel[1] = 0;
  object.faceYaw = object.moveYaw;

  if (object.vars.health <= 0) {
    ctx.bossDefeated(object.pos);
    object.deactivate(-1);
    return;
  }

  object.setAction(KING_ACT_IDLE);
}

export function kingBobombBehavior(object: SimObject, ctx: ObjectContext): void {
  switch (object.heldState) {
    case HELD_HELD:
      object.visible = true;
      return;

    case HELD_THROWN:
      object.heldState = HELD_FREE;
      object.setAction(KING_ACT_THROWN);
      object.vars.thrown = 1;
      object.forwardVel = 20;
      object.vel[1] = 30;
      object.gravity = 4;
      break;

    case HELD_DROPPED:
      object.heldState = HELD_FREE;
      object.forwardVel = 0;
      object.vel[1] = 0;
      object.setAction(KING_ACT_IDLE);
      break;

    default:
      break;
  }

  if (object.vars.thrown) {
    object.stepPhysics(ctx.world);
    throwLanding(object, ctx);
    return;
  }

  object.visible = object.distanceToMario < 5000;
  if ((ctx.mario.flags & MARIO_VANISH_CAP) && !object.vars.grabbing) {
    object.forwardVel = 0;
    object.stepPhysics(ctx.world);
    return;
  }
  if (object.vars.grabbing) {
    object.vars.grabTimer--;
    object.forwardVel = 0;
    if (object.vars.grabTimer <= 0) {
      object.vars.grabbing = 0;
      object.vars.grabCooldown = 150;
      object.vars.thrown = 0;
      ctx.mario.throwFromGrab(object);
    }
    object.stepPhysics(ctx.world);
    object.faceYaw = object.moveYaw;
    return;
  }

  if (Number(object.vars.intro ?? 0) === 0 && object.distanceToMario < 2500) {
    object.vars.intro = 1;
    object.vars.introTimer = 60;
    object.forwardVel = 0;
    ctx.dialogue('KING BOB-OMB: SO YOU WANT TO CHALLENGE ME? THREE THROWS, LITTLE PLUMBER!');
    return;
  }
  if (Number(object.vars.intro ?? 0) === 1) {
    object.vars.introTimer--;
    object.forwardVel = 0;
    object.turnTowardAngle(object.angleToMario, 0x100);
    object.stepPhysics(ctx.world);
    object.faceYaw = object.moveYaw;
    if (object.vars.introTimer <= 0) object.vars.intro = 2;
    return;
  }

  object.vars.grabCooldown = Math.max(0, Number(object.vars.grabCooldown ?? 0) - 1);
  if (
    object.vars.grabCooldown === 0
    && object.distanceToMario < 240
    && Math.abs(ctx.mario.pos[1] - object.pos[1]) < 160
    && ctx.mario.startKingGrab(object)
  ) {
    object.vars.grabbing = 1;
    object.vars.grabTimer = 40;
    object.forwardVel = 0;
    object.faceYaw = object.angleToMario;
    return;
  }

  if (object.distanceToMario < 2500 && object.action === KING_ACT_IDLE) {
    object.forwardVel = 3;
    object.turnTowardAngle(object.angleToMario, 0x100);
    object.stepPhysics(ctx.world);
  } else {
    object.forwardVel = 0;
    object.stepPhysics(ctx.world);
  }
  object.faceYaw = object.moveYaw;
}
