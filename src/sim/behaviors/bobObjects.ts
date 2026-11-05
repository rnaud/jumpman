/**
 * Bob-omb Battlefield object set: purple switch, openable gate grill, and the
 * pit bowling balls.
 *
 * Sources: src/game/behaviors/purple_switch.inc.c, grill_door.inc.c and
 * bowling_ball.inc.c. The source switch collision model and cage-door geometry
 * are approximated with a single removable barrier, and the bowling balls roll
 * straight down the slope instead of following the source's waypoint table.
 */

import type { CollisionWorld, Surface } from '../collision';
import { coss, sins, toS16, type Vec3 } from '../math';
import {
  INTERACT_DAMAGE,
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';

const PURPLE_SWITCH_ACT_IDLE = 0;
const PURPLE_SWITCH_ACT_PRESSED = 1;
const PURPLE_SWITCH_ACT_TICKING = 2;
const PURPLE_SWITCH_ACT_UNPRESSED = 3;
const PURPLE_SWITCH_ACT_WAIT = 4;

export const PURPLE_SWITCH_TICKING = PURPLE_SWITCH_ACT_TICKING;

const BOWLING_BALL_HITBOX: ObjectHitbox = {
  interactType: INTERACT_DAMAGE,
  downOffset: 0,
  damageOrCoinValue: 2,
  health: 0,
  numLootCoins: 0,
  radius: 100,
  height: 150,
  hurtboxRadius: 0,
  hurtboxHeight: 0,
};

function yawVector(yaw: number): { rx: number; rz: number; fx: number; fz: number } {
  return { rx: coss(yaw), rz: -sins(yaw), fx: sins(yaw), fz: coss(yaw) };
}

/** Add a removable wall at an object's position. */
export function addBarrier(
  world: CollisionWorld,
  object: SimObject,
  halfWidth: number,
  height: number,
  halfDepth: number,
): void {
  if (object.collisionSurfaces?.length) return;
  const { rx, rz, fx, fz } = yawVector(object.moveYaw);
  const baseY = object.pos[1];
  const topY = baseY + height;
  const surfaces: Surface[] = [];

  for (const side of [-1, 1]) {
    const cx = object.pos[0] + fx * halfDepth * side;
    const cz = object.pos[2] + fz * halfDepth * side;
    const ax = cx - rx * halfWidth;
    const az = cz - rz * halfWidth;
    const bx = cx + rx * halfWidth;
    const bz = cz + rz * halfWidth;
    const quad = side < 0
      ? [[ax, baseY, az], [bx, baseY, bz], [bx, topY, bz], [ax, topY, az]]
      : [[bx, baseY, bz], [ax, baseY, az], [ax, topY, az], [bx, topY, bz]];
    surfaces.push(...world.addQuad(
      quad[0] as Vec3,
      quad[1] as Vec3,
      quad[2] as Vec3,
      quad[3] as Vec3,
    ));
  }
  object.collisionSurfaces = surfaces;
}

export function removeBarrier(world: CollisionWorld, object: SimObject): void {
  if (!object.collisionSurfaces?.length) return;
  world.removeSurfaces(new Set(object.collisionSurfaces));
  object.collisionSurfaces = [];
}

export function initPurpleSwitch(object: SimObject): void {
  object.scale = 1.5;
  object.visible = true;
  object.vars.action = PURPLE_SWITCH_ACT_IDLE;
  object.vars.timer = 0;
}

function marioOnSwitch(object: SimObject, ctx: ObjectContext): boolean {
  const mario = ctx.mario;
  const dx = mario.pos[0] - object.pos[0];
  const dz = mario.pos[2] - object.pos[2];
  return (
    dx * dx + dz * dz < 127.5 * 127.5
    && Math.abs(mario.pos[1] - object.pos[1]) < 80
    && Math.abs(mario.pos[1] - mario.floorHeight) < 20
  );
}

export function purpleSwitchBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer++;
  switch (object.vars.action) {
    case PURPLE_SWITCH_ACT_IDLE:
      object.scale = 1.5;
      if (marioOnSwitch(object, ctx)) {
        object.vars.action = PURPLE_SWITCH_ACT_PRESSED;
        object.vars.timer = 0;
      }
      break;

    case PURPLE_SWITCH_ACT_PRESSED:
      object.scale = 1.5 - 1.3 * Math.min(1, object.vars.timer / 3);
      if (object.vars.timer >= 3) {
        object.vars.action = PURPLE_SWITCH_ACT_TICKING;
        object.vars.timer = 0;
      }
      break;

    case PURPLE_SWITCH_ACT_TICKING:
      if (object.vars.timer > 400) {
        object.vars.action = PURPLE_SWITCH_ACT_WAIT;
        object.vars.timer = 0;
      }
      break;

    case PURPLE_SWITCH_ACT_WAIT:
      if (!marioOnSwitch(object, ctx)) {
        object.vars.action = PURPLE_SWITCH_ACT_UNPRESSED;
        object.vars.timer = 0;
      }
      break;

    case PURPLE_SWITCH_ACT_UNPRESSED:
      object.scale = 0.2 + 1.3 * Math.min(1, object.vars.timer / 3);
      if (object.vars.timer >= 3) {
        object.vars.action = PURPLE_SWITCH_ACT_IDLE;
        object.vars.timer = 0;
      }
      break;
  }
}

export function initOpenableGrill(object: SimObject): void {
  object.visible = true;
  object.scale = 1;
  object.vars.spawned = 0;
  object.vars.open = 0;
}

function spawnGrillDoors(object: SimObject, ctx: ObjectContext): void {
  const { rx, rz } = yawVector(object.moveYaw);
  for (const side of [-1, 1]) {
    const door = ctx.spawn({
      behavior: 'grillDoor',
      pos: [
        object.pos[0] + rx * 320 * side,
        object.pos[1],
        object.pos[2] + rz * 320 * side,
      ],
      yaw: side < 0 ? toS16(object.moveYaw + 0x8000) : object.moveYaw,
      model: 'levels/bob/bob_geo_000470.glb',
    });
    door.link = object;
    door.vars.side = side;
    door.vars.timer = 0;
  }
  object.vars.spawned = 1;
  addBarrier(ctx.world, object, 320, 400, 40);
}

export function openableGrillBehavior(object: SimObject, ctx: ObjectContext): void {
  if (!object.vars.spawned) spawnGrillDoors(object, ctx);
  const switchObject = ctx.findByBehavior('purpleSwitch');
  if (
    switchObject
    && switchObject.vars.action === PURPLE_SWITCH_ACT_TICKING
    && !object.vars.open
  ) {
    object.vars.open = 1;
    removeBarrier(ctx.world, object);
    ctx.breakObject(object);
  }
}

export function grillDoorBehavior(object: SimObject, ctx: ObjectContext): void {
  if (!object.link) return;
  if (!object.link.vars.open) return;
  if (object.vars.timer >= 64) return;
  object.vars.timer++;
  object.moveYaw = toS16(object.moveYaw - object.vars.side * 0x100);
  object.faceYaw = object.moveYaw;
}

export function initPitBowlingBall(object: SimObject): void {
  object.gravity = 12;
  object.friction = 1;
  object.buoyancy = 2;
  object.setHitbox(BOWLING_BALL_HITBOX);
  object.visible = true;
}

export function pitBowlingBallBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.distanceToMario > 4000) {
    object.visible = false;
    return;
  }
  object.visible = true;

  const floor = ctx.world.findFloor(object.pos[0], object.pos[1], object.pos[2]);
  if (
    floor.surface
    && Math.abs(floor.surface.normal[0]) < 0.01
    && Math.abs(floor.surface.normal[2]) < 0.01
  ) {
    object.forwardVel = 28;
  }

  object.stepPhysics(ctx.world);
  object.setHitbox(BOWLING_BALL_HITBOX);
  object.faceYaw = object.moveYaw;

  if (object.pos[1] < -3000) object.deactivate(-1);
}
