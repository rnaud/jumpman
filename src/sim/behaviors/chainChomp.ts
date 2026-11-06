/**
 * Chain Chomp, wooden post and gate behavior, ported from
 * reference/sm64/src/game/behaviors/chain_chomp.inc.c.
 */

import { approachF32, atan2s, coss, sins, toS16, type Vec3 } from '../math';
import {
  INT_STATUS_WAS_ATTACKED,
  INTERACT_MR_BLIZZARD,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';
import { readyForNpcDialog } from '../actions/cutscene';
import { ACT_READING_NPC_DIALOG, ACT_GROUND_POUND_LAND } from '../state';

const CHAIN_CHOMP_ACT_UNINITIALIZED = 0;
const CHAIN_CHOMP_ACT_MOVE = 1;

const SUB_ACT_TURN = 0;
const SUB_ACT_LUNGE = 1;

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
  object.vars.jawFrame = 0;
  object.vars.segmentRadius = 0;
  object.vars.linkSpacing = 0;
  object.vars.restricted = 0;
  object.vars.recoil = 0;
  object.vars.facePitch = 0;
  object.vars.targetPitch = 0;
  object.visible = false;
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
  object.vars.totalAngle = object.vars.prevAngle = object.vars.rewarded = 0;
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
  object.billboard = true;
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
  for (let i = 0; i < 4; i++) {
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

function updateChain(object: SimObject, pivot = object.links![0].home): void {
  let previous = object.pos;
  for (const [index, part] of object.links!.slice(1).entries()) {
    part.pos[1] = Math.max(pivot[1], part.pos[1] + (object.vel[1] < 0 ? object.vel[1] : -20));
    limitDistance(part.pos, previous, object.vars.linkSpacing);
    limitDistance(part.pos, pivot, object.vars.segmentRadius * (4 - index));
    part.visible = object.visible && object.active;
    previous = part.pos;
  }
}

// Negative spacing is intentional: the source alternates its sign during recoil.
function limitDistance(pos: Vec3, origin: Vec3, limit: number): boolean {
  const dx = pos[0] - origin[0], dy = pos[1] - origin[1], dz = pos[2] - origin[2];
  const distance = Math.hypot(dx, dy, dz);
  if (distance === 0 || distance <= limit) return false;
  const ratio = limit / distance;
  pos[0] = origin[0] + dx * ratio;
  pos[1] = origin[1] + dy * ratio;
  pos[2] = origin[2] + dz * ratio;
  return true;
}

function moveChomp(object: SimObject, ctx: ObjectContext, wallRadius: number): void {
  if (wallRadius) {
    const wall = ctx.world.resolveAndReturnWallCollisions(object.pos, 10, wallRadius);
    object.hitWall = !!wall && wall.normal[0]*sins(object.moveYaw) + wall.normal[2]*coss(object.moveYaw) < 0;
    if (object.hitWall && wall) object.vars.wallYaw = atan2s(wall.normal[2], wall.normal[0]);
  }
  // cur_obj_move_standard(78), zero drag, -50% bounce.
  const floor = ctx.world.findFloor(...object.pos);
  const x = object.pos[0] + object.forwardVel * sins(object.moveYaw);
  const z = object.pos[2] + object.forwardVel * coss(object.moveYaw);
  const next = ctx.world.findFloor(x, object.pos[1], z);
  if (next.surface && (next.height - floor.height < 5 || next.surface.normal[1] > coss(78 * 182) || object.pos[1] > next.height)) {
    object.pos[0] = x;
    object.pos[2] = z;
  }
  object.vel[1] = Math.max(-78, object.vel[1] - object.gravity);
  object.pos[1] += object.vel[1];
  object.grounded = !!floor.surface && object.pos[1] < floor.height;
  if (object.grounded) {
    object.pos[1] = floor.height;
    if (object.vel[1] < 0) object.vel[1] *= -0.5;
  }
}

function releasedStep(object: SimObject, ctx: ObjectContext): void {
  const phase = object.vars.release;
  object.gravity = 4;
  if (phase === 1) {
    object.forwardVel = 0;
    const mario = ctx.mario;
    if (mario.action === ACT_READING_NPC_DIALOG && mario.usedObj === object) {
      if (mario.actionState === 8 && object.grounded) {
        object.vars.release = 2;
        object.vars.releaseTimer = 0;
        object.vars.lunges = 0;
      }
    } else if (readyForNpcDialog(mario)) {
      mario.usedObj = object;
      mario.setAction(ACT_READING_NPC_DIALOG, 2);
    }
  } else if (phase === 2) {
    object.vars.segmentRadius = approachF32(object.vars.segmentRadius, CHAIN_MAX_DIST / 5, 4, 4);
    object.vars.linkSpacing = object.vars.segmentRadius;
    object.vars.releaseTimer++;
    if (object.grounded && object.vars.lunges === 0) {
      object.turnTowardAngle(object.angleToMario, 800);
      if (toS16(object.moveYaw-object.angleToMario) !== 0) object.vars.releaseTimer = 0;
      else if (object.vars.releaseTimer > 60) object.vars.lunges = 1;
    } else if (object.grounded && ++object.vars.lunges <= 5) {
      object.moveYaw = toS16(object.angleToMario + (Math.random() < 0.5 ? -0x2000 : 0x2000));
      object.forwardVel = 30;
      object.vel[1] = 50;
    } else if (object.grounded) {
      const dx = 1450 - object.pos[0], dz = 562 - object.pos[2];
      object.moveYaw = atan2s(dz, dx);
      object.forwardVel = Math.hypot(dx,dz)/8;
      object.vars.release = 3;
      object.vel[1] = 50;
    }
  } else if (phase === 3 && object.hitWall && !object.vars.hitGate) {
    object.vars.hitGate = 1;
    object.moveYaw = toS16(2*object.vars.wallYaw-object.moveYaw+0x8000);
    object.forwardVel *= 0.4;
  } else if (phase === 3 && object.vars.hitGate && object.grounded) {
    const dx = 3288 - object.pos[0], dz = -1770 - object.pos[2];
    object.moveYaw = atan2s(dz,dx);
    object.forwardVel = Math.hypot(dx,dz)/50;
    object.vel[1] = 120;
    object.vars.release = 4;
  } else if (phase === 4 && object.grounded) {
    object.vars.release = 5;
    object.vars.releaseTimer = 0;
  } else if (phase === 5 && ++object.vars.releaseTimer >= 15) {
    if (ctx.mario.action === ACT_READING_NPC_DIALOG && ctx.mario.usedObj === object) ctx.mario.actionState = 9;
    for (const part of object.links?.slice(1) ?? []) part.deactivate(-1);
    object.deactivate(-1);
    return;
  }
  object.faceYaw = object.moveYaw;
  moveChomp(object, ctx, object.vars.lunges > 0 ? 200 : 0);
  // The free pivot follows the Chomp once the tether reaches its full length.
  const pivot = object.chainPivot ??= [...object.links![0].home];
  const previous: Vec3 = [...pivot];
  limitDistance(pivot, object.pos, object.vars.segmentRadius * 5);
  for (const link of object.links!.slice(1)) {
    for (let axis = 0; axis < 3; axis++) link.pos[axis] += pivot[axis] - previous[axis];
  }
  updateChain(object, pivot);
}

export function chainChompBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.action === CHAIN_CHOMP_ACT_UNINITIALIZED) {
    if (!object.links) spawnChain(object, ctx);
    if (object.distanceToMario > 3000) return;
    object.setAction(CHAIN_CHOMP_ACT_MOVE);
    object.visible = true;
    object.pos = [...object.home] as Vec3;
    updateChain(object);
    object.vars.subAction = SUB_ACT_TURN;
    object.vars.timer = 0;
    return;
  }

  if (object.vars.release) {
    object.vars.jawFrame = (object.vars.jawFrame + 1) % 20;
    releasedStep(object, ctx);
    return;
  }

  const v = object.vars;
  const pivot = object.links![0].home;
  v.timer++;
  let holdJaw = false;
  if (v.subAction === SUB_ACT_TURN) {
    object.gravity = 4;
    v.segmentRadius = approachF32(v.segmentRadius, CHAIN_MAX_DIST / 5, 4, 4);
    v.linkSpacing = v.segmentRadius;
    object.turnTowardAngle(object.angleToMario, object.grounded ? 0x400 : 400);
    if (!object.grounded) v.timer = 0;
    else if (Math.abs(toS16(object.angleToMario - object.moveYaw)) >= 0x800) {
      object.forwardVel = 10;
      object.vel[1] = 20;
    } else if (v.timer <= 30) {
      object.forwardVel = 0;
    } else if (v.jawFrame !== 0) {
      v.timer--;
    } else {
      holdJaw = true;
      if (v.timer > 40) {
        v.subAction = SUB_ACT_LUNGE;
        v.segmentRadius = CHAIN_MAX_DIST_LUNGE / 5;
        object.forwardVel = 140;
        object.vel[1] = 20;
        object.gravity = 0;
        v.targetPitch = -atan2s(object.forwardVel, object.vel[1]);
      }
    }
  } else {
    v.facePitch = approachF32(v.facePitch, v.targetPitch, 0x400, 0x400);
    if (object.forwardVel !== 0) {
      if (v.restricted) {
        object.forwardVel = object.vel[1] = 0;
        v.recoil = 30;
      }
      const distance = Math.hypot(object.pos[0]-pivot[0], object.pos[1]-pivot[1], object.pos[2]-pivot[2]);
      v.linkSpacing = Math.min(220, CHAIN_MAX_DIST_LUNGE-distance) / 220 * v.segmentRadius;
      v.timer = 0;
    } else {
      object.turnTowardAngle(atan2s(object.pos[2]-pivot[2], object.pos[0]-pivot[0]), 0x1000);
      if (v.recoil > 0) v.recoil = Math.max(0, v.recoil - 0.8);
      else v.subAction = SUB_ACT_TURN;
      v.linkSpacing = v.timer % 2 ? -v.recoil : v.recoil;
    }
    holdJaw = v.timer < 30;
  }
  if (!holdJaw) v.jawFrame = (v.jawFrame + 1) % 20;
  moveChomp(object, ctx, 0);
  v.restricted = Number(limitDistance(object.pos, pivot, v.segmentRadius * 5));
  if (object.interactStatus & INT_STATUS_WAS_ATTACKED) {
    v.subAction = SUB_ACT_LUNGE;
    v.segmentRadius = CHAIN_MAX_DIST_LUNGE / 5;
    object.forwardVel = 0;
    object.vel[1] = 300;
    object.gravity = 4;
    v.targetPitch = -0x3000;
  }
  object.interactStatus = 0;
  object.faceYaw = object.moveYaw;
  updateChain(object);
}

export function woodenPostBehavior(object: SimObject, ctx: ObjectContext): void {
  const mario = ctx.mario;
  if (!object.vars.offsetY && !object.vars.rewarded) {
    if (object.distanceToMario > 400) {
      object.timer = object.vars.totalAngle = 0;
    } else {
      object.vars.totalAngle += toS16(object.angleToMario-object.vars.prevAngle);
      if (Math.abs(object.vars.totalAngle)>0x30000 && object.timer<200) {
        for (let i=0;i<5;i++) ctx.spawnCoin(object.pos,true);
        object.vars.rewarded = 1;
      }
    }
    object.vars.prevAngle = object.angleToMario;
  }
  const onPost = mario.action === ACT_GROUND_POUND_LAND
    && mario.floor?.object === object
    && Math.abs(mario.pos[1] - mario.floorHeight) < 4;
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
