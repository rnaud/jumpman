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
import { atan2s, coss, sins } from '../math';
import { ACT_FLAG_AIR, MARIO_VANISH_CAP } from '../state';

const KING_ACT_IDLE = 0;
const KING_ACT_THROWN = 1;
const KING_ACT_RETURN_HOME = 5;

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
  object.setHitbox({...KING_HITBOX, interactType:0});
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
  if (object.pos[1] <= object.home[1]-100) {
    beginReturn(object);
    return;
  }
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

function beginReturn(o: SimObject): void {
  o.setAction(KING_ACT_RETURN_HOME);
  o.hitbox.interactType = 0;
  o.forwardVel = 0;
  o.moveYaw = atan2s(o.home[2]-o.pos[2],o.home[0]-o.pos[0]);
  o.faceYaw = o.moveYaw;
}

function returnHome(o: SimObject): void {
  if (o.actionState === 0) {
    if (o.pos[1] < o.home[1]) { o.pos[1] += 100; return; }
    // arc_to_goal_pos with vertical speed 100 and gravity -4.
    o.vel[1] = 100;
    o.forwardVel = Math.hypot(o.home[0]-o.pos[0],o.home[2]-o.pos[2])/49;
    o.actionState = 1;
  }
  o.vel[1] -= 4;
  o.pos[1] += o.vel[1];
  o.pos[0] += o.forwardVel*sins(o.moveYaw);
  o.pos[2] += o.forwardVel*coss(o.moveYaw);
  if (o.vel[1]<0 && o.pos[1]<=o.home[1]) {
    o.pos = [...o.home];
    o.vel[1] = o.forwardVel = 0;
    o.setAction(KING_ACT_IDLE);
    o.vars.intro = 0;
  }
}

export function kingBobombBehavior(object: SimObject, ctx: ObjectContext): void {
  if (!object.vars.homeSet) {
    const floor = ctx.world.findFloor(...object.home);
    if (floor.surface) object.home[1] = floor.height;
    object.vars.homeSet = 1;
  }
  if (object.action === KING_ACT_RETURN_HOME) { returnHome(object); return; }
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

  if (object.pos[1] - ctx.mario.pos[1] > 1200) {
    object.vars.health = 3;
    object.vars.intro = 0;
  }
  if (!object.vars.intro) {
    object.pos = [...object.home];
    object.forwardVel = object.vel[1] = 0;
    object.hitbox.interactType = 0;
    const dy = ctx.mario.pos[1]-object.pos[1];
    if (dy > -160 && dy < 100 && !(ctx.mario.action & ACT_FLAG_AIR)
        && Math.hypot(ctx.mario.pos[0]-object.pos[0],ctx.mario.pos[2]-object.pos[2]) < 500) {
      object.vars.intro = 1;
      object.vars.introTimer = 60;
      ctx.dialogue('KING BOB-OMB: Can you pick me up from the back and hurl me to this royal turf?');
    }
    return;
  }
  if (object.pos[1] < object.home[1]-100) { beginReturn(object); return; }
  if (Number(object.vars.intro ?? 0) === 1) {
    object.vars.introTimer--;
    object.forwardVel = 0;
    object.turnTowardAngle(object.angleToMario, 0x100);
    object.stepPhysics(ctx.world);
    object.faceYaw = object.moveYaw;
    if (object.vars.introTimer <= 0) {
      object.vars.intro = 2;
      object.hitbox.interactType = INTERACT_GRABBABLE;
    }
    return;
  }

  if ((ctx.mario.flags & MARIO_VANISH_CAP) && !object.vars.grabbing) {
    object.forwardVel = 0;
    object.stepPhysics(ctx.world);
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
