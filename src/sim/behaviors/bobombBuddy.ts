import type { ObjectContext, ObjectHitbox, SimObject } from '../object';

const BUDDY_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 100,
  height: 60,
  hurtboxRadius: 100,
  hurtboxHeight: 60,
};

export function initBobombBuddy(object: SimObject): void {
  object.setHitbox(BUDDY_HITBOX);
  object.gravity = 2.5;
  object.friction = 0.8;
  object.buoyancy = 1.3;
  object.visible = true;
}

export function bobombBuddyBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.timer === 1) {
    const floor = ctx.world.findFloor(object.home[0],object.home[1]+200,object.home[2]);
    if (floor.surface) object.pos[1] = object.home[1] = floor.height;
  }
  object.stepPhysics(ctx.world);
  if (object.distanceToMario < 1000) object.turnTowardAngle(object.angleToMario,0x140);
  object.faceYaw = object.moveYaw;
  object.visible = object.distanceToMario < 3000;
}
