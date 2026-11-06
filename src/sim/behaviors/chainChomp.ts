/**
 * Chain Chomp, wooden post and gate behavior, ported from
 * reference/sm64/src/game/behaviors/chain_chomp.inc.c.
 *
 * The Source chains five rigid segments to a pivot post. Jumpman keeps the
 * same state machine and chain length, but links the five metallic balls along
 * a simple constrained line rather than simulating every segment's oscillator.
 * The Chomp's lunge speed is reduced from the source's 140 u/f to 40 u/f so the
 * encounter reads at a playable speed in the WebGL build.
 */

import { atan2s, toS16, type Vec3 } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_MR_BLIZZARD,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';
import { ACT_GROUND_POUND_LAND, MARIO_VANISH_CAP } from '../state';

const CHAIN_CHOMP_ACT_UNINITIALIZED = 0;
const CHAIN_CHOMP_ACT_MOVE = 1;

const SUB_ACT_TURN = 0;
const SUB_ACT_LUNGE = 1;
const SUB_ACT_RETURN = 2;

const CHAIN_MAX_DIST = 750;
const CHAIN_MAX_DIST_LUNGE = 900;

const CHAIN_CHOMP_HITBOX: ObjectHitbox = {
  interactType: INTERACT_MR_BLIZZARD,
  downOffset: 0,
  damageOrCoinValue: 3,
  health: 1,
  numLootCoins: 0,
  radius: 80,
  height: 160,
  hurtboxRadius: 80,
  hurtboxHeight: 160,
};

export function initChainChomp(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.buoyancy = 2;
  object.scale = 2;
  object.graphYOffset = 240;
  object.visible = true;
  object.setHitbox(CHAIN_CHOMP_HITBOX);
  object.setAction(CHAIN_CHOMP_ACT_UNINITIALIZED);
  object.vars.timer = 0;
  object.vars.subAction = SUB_ACT_TURN;
  object.vars.release = 0;
  object.vars.releaseTimer = 0;
  object.vars.hitGate = 0;
}

export function initWoodenPost(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.buoyancy = 2;
  object.scale = 0.5;
  object.visible = true;
  object.vars.offsetY = 0;
  object.vars.speedY = 0;
  object.vars.pounded = 0;
}

export function initChainChompGate(object: SimObject): void {
  object.gravity = 0;
  object.friction = 1;
  object.visible = true;
}

export function initChainPart(object: SimObject): void {
  object.gravity = 0;
  object.friction = 1;
  object.visible = true;
  object.scale = 2;
  object.graphYOffset = 40;
}

function spawnChain(object: SimObject, ctx: ObjectContext): void {
  const post = ctx.spawn({
    behavior: 'woodenPost',
    pos: [...object.home] as Vec3,
    yaw: 0,
    model: 'actors/poundable_pole/wooden_post_geo.glb',
  });
  const postFloor = ctx.world.findFloor(object.home[0], object.home[1] + 200, object.home[2]);
  if (postFloor.surface) post.pos[1] = postFloor.height;
  post.home[1] = post.pos[1];
  post.link = object;

  const parts: SimObject[] = [];
  for (let i = 0; i < 5; i++) {
    const part = ctx.spawn({
      behavior: 'chainPart',
      pos: [...object.home] as Vec3,
      yaw: 0,
      model: 'actors/chain_ball/metallic_ball_geo.glb',
    });
    part.link = object;
    part.linkIndex = i;
    parts.push(part);
  }

  object.links = [post, ...parts];
  const gate = ctx.findByBehavior('chainChompGate');
  if (gate) gate.link = object;
  updateChain(object);
}

function updateChain(object: SimObject): void {
  const post = object.links?.[0];
  if (!post) return;
  const parts = object.links!.slice(1);
  const dx = object.pos[0] - post.pos[0];
  const dy = object.pos[1] - post.pos[1];
  const dz = object.pos[2] - post.pos[2];

  parts.forEach((part, index) => {
    const fraction = (index + 1) / 5;
    const sag = Math.sin(fraction * Math.PI) * 38;
    part.pos[0] = post.pos[0] + dx * fraction;
    part.pos[1] = post.pos[1] + dy * fraction - sag;
    part.pos[2] = post.pos[2] + dz * fraction;
    part.visible = object.visible && object.active;
  });
}

function distToPost(object: SimObject): number {
  const post = object.links?.[0];
  if (!post) return 0;
  const dx = object.pos[0] - post.pos[0];
  const dy = object.pos[1] - post.pos[1];
  const dz = object.pos[2] - post.pos[2];
  return Math.hypot(dx, dy, dz);
}

function constrainToChain(object: SimObject): void {
  const post = object.links?.[0];
  if (!post) return;
  const dx = object.pos[0] - post.pos[0];
  const dy = object.pos[1] - post.pos[1];
  const dz = object.pos[2] - post.pos[2];
  const length = Math.hypot(dx, dy, dz);
  const max = object.vars.subAction === SUB_ACT_LUNGE ? CHAIN_MAX_DIST_LUNGE : CHAIN_MAX_DIST;
  if (length <= max || length === 0) return;
  const ratio = max / length;
  object.pos[0] = post.pos[0] + dx * ratio;
  object.pos[1] = post.pos[1] + dy * ratio;
  object.pos[2] = post.pos[2] + dz * ratio;
}

function releasedStep(object: SimObject, ctx: ObjectContext): void {
  const phase = object.vars.release;
  object.gravity = 4;
  if (phase === 1) {
    object.forwardVel = 0;
    object.faceYaw = object.moveYaw = toS16(object.moveYaw + Math.max(-800, Math.min(800, toS16(object.angleToMario-object.moveYaw))));
    if (object.grounded && ++object.vars.releaseTimer > 60) {
      object.vars.release = 2;
      object.vars.lunges = 0;
    }
  } else if (phase === 2 && object.grounded) {
    if (++object.vars.lunges <= 4) {
      object.moveYaw = toS16(object.angleToMario + (Math.random() < 0.5 ? -0x2000 : 0x2000));
      object.forwardVel = 30;
    } else {
      const dx = 1450 - object.pos[0], dz = 562 - object.pos[2];
      object.moveYaw = atan2s(dz, dx);
      object.forwardVel = Math.hypot(dx,dz)/8;
      object.vars.release = 3;
    }
    object.vel[1] = 50;
  } else if (phase === 3 && object.hitWall && !object.vars.hitGate) {
    object.vars.hitGate = 1;
    object.forwardVel *= 0.4;
  } else if (phase === 3 && object.vars.hitGate && object.grounded) {
    const dx = 3288 - object.pos[0], dz = -1770 - object.pos[2];
    object.moveYaw = atan2s(dz,dx);
    object.forwardVel = Math.hypot(dx,dz)/50;
    object.vel[1] = 120;
    object.vars.release = 4;
  } else if (phase === 4 && object.grounded) {
    for (const part of object.links?.slice(1) ?? []) part.deactivate(-1);
    object.deactivate(-1);
    return;
  }
  object.faceYaw = object.moveYaw;
  object.stepPhysics(ctx.world, object.vars.release >= 2 ? 200 : 0);
  updateChain(object);
}

export function chainChompBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.action === CHAIN_CHOMP_ACT_UNINITIALIZED) {
    if (!object.links) spawnChain(object, ctx);
    if (object.distanceToMario > 3000) return;
    object.setAction(CHAIN_CHOMP_ACT_MOVE);
    object.visible = true;
    object.pos = [...object.home] as Vec3;
    object.vars.subAction = SUB_ACT_TURN;
    object.vars.timer = 0;
    return;
  }

  if (object.vars.release) {
    releasedStep(object, ctx);
    return;
  }

  if ((ctx.mario.flags & MARIO_VANISH_CAP) && object.action !== 0) {
    object.forwardVel = 0;
    object.stepPhysics(ctx.world);
    return;
  }

  object.vars.timer++;

  if (object.vars.subAction === SUB_ACT_TURN) {
    object.forwardVel = 0;
    object.gravity = 4;
    object.turnTowardAngle(object.angleToMario, 0x400);
    const delta = Math.abs(toS16(object.angleToMario - object.moveYaw));
    if (delta < 0x800 && object.vars.timer > 30) {
      object.vars.subAction = SUB_ACT_LUNGE;
      object.vars.timer = 0;
      object.vel[1] = 20;
      object.gravity = 0;
    }
  } else if (object.vars.subAction === SUB_ACT_LUNGE) {
    object.forwardVel = 40;
    object.gravity = 0;
    if (distToPost(object) > CHAIN_MAX_DIST_LUNGE || object.vars.timer > 60) {
      object.vars.subAction = SUB_ACT_RETURN;
      object.vars.timer = 0;
      object.vel[1] = 10;
      object.gravity = 4;
    }
  } else {
    object.forwardVel = -20;
    object.gravity = 4;
    if (distToPost(object) < 250) {
      object.vars.subAction = SUB_ACT_TURN;
      object.vars.timer = 0;
      object.forwardVel = 0;
    }
  }

  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    object.vars.subAction = SUB_ACT_LUNGE;
    object.vars.timer = 0;
    object.vel[1] = 30;
    object.gravity = 4;
  }
  object.interactStatus = 0;

  object.stepPhysics(ctx.world);
  constrainToChain(object);
  object.faceYaw = object.moveYaw;
  updateChain(object);
}

export function woodenPostBehavior(object: SimObject, ctx: ObjectContext): void {
  const mario = ctx.mario;
  const onPost = mario.action === ACT_GROUND_POUND_LAND
    && Math.hypot(mario.pos[0]-object.pos[0], mario.pos[2]-object.pos[2]) < 60
    && Math.abs(mario.pos[1] - object.pos[1] - 205) < 5;
  if (!object.vars.pounded) {
    if (onPost) {
      object.vars.pounded = 1;
      object.vars.speedY = -70;
    }
    return;
  }
  object.vars.speedY = Math.min(0, object.vars.speedY + 25);
  if (object.vars.speedY === 0) {
    object.vars.pounded = Number(onPost);
    return;
  }
  object.vars.offsetY = Math.max(-190, object.vars.offsetY + object.vars.speedY);
  object.pos[1] = object.home[1] + object.vars.offsetY;
  if (object.vars.offsetY === -190 && object.link?.behavior === 'chainChomp') {
    object.link.vars.release = 1;
    object.link = undefined;
  }
}

export function chainChompGateBehavior(object: SimObject, ctx: ObjectContext): void {
  if (!object.link) object.link = ctx.findByBehavior('chainChomp');
  if (object.link && object.link.vars.hitGate) {
    ctx.breakObject(object);
    object.deactivate(-1);
  }
}

export function chainPartBehavior(object: SimObject): void {
  if (!object.link || !object.link.active) object.deactivate(-1);
}
