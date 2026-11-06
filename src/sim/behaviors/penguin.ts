/** Penguin behaviors for Cool, Cool Mountain. */

import { atan2s, dist3D } from '../math';
import { ACT_DIVE_SLIDE, ACT_IDLE } from '../state';
import { HELD_FREE, HELD_HELD, INTERACT_GRABBABLE, INT_SUBTYPE_HOLDABLE_NPC, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const PENGUIN_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 45,
  height: 90,
  hurtboxRadius: 40,
  hurtboxHeight: 80,
};

export function initPenguin(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(PENGUIN_HITBOX);
  object.vars.dir = 1;
  object.vars.timer = 0;
  object.visible = true;
}

export function penguinBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (Number(object.vars.timer) % 150 === 0) object.vars.dir = -Number(object.vars.dir ?? 1);
  object.forwardVel = 1.4 * Number(object.vars.dir ?? 1);
  object.faceYaw = object.moveYaw;
  const step = object.stepPhysics(ctx.world);
  if (step === 1) {
    object.vars.dir = -Number(object.vars.dir ?? 1);
    object.forwardVel = -object.forwardVel;
  }
}

// Small penguins and Tuxie's mother in tuxie.inc.c use the second behavior byte as a family ID.
export function initBabyPenguin(o: SimObject): void {
  o.gravity = 4;
  o.setHitbox({...PENGUIN_HITBOX,interactType:INTERACT_GRABBABLE,radius:40,height:40});
  o.interactSubtype = INT_SUBTYPE_HOLDABLE_NPC;
  o.vars.animation = 0;
}

export function initMother(o: SimObject): void {
  o.scale = 4;
  o.gravity = 4;
  o.vars.animation = 3;
  o.setHitbox({...PENGUIN_HITBOX,radius:200,height:300});
}

function placeOnFloor(o: SimObject, ctx: ObjectContext): void {
  if (o.vars.placed) return;
  const hit = ctx.world.findFloor(o.pos[0],o.pos[1]+200,o.pos[2]);
  if (hit.surface) o.home[1] = o.pos[1] = hit.height;
  o.vars.placed = 1;
}

export function babyPenguinBehavior(o: SimObject, ctx: ObjectContext): void {
  placeOnFloor(o,ctx);
  if (o.heldState === HELD_HELD) {
    o.vars.animation = 0;
    if (o.timer % 30 === 0) ctx.penguinSound(o.pos,false);
    if (o.vars.rejected) o.vars.rejected = 0;
    return;
  }
  if (o.heldState !== HELD_FREE) {
    o.setHeldState(HELD_FREE);
    o.forwardVel = o.vel[1] = 0;
    o.setAction(o.vars.stayNearMother ? 5 : 0);
  }
  const mother = ctx.findByBehavior('tuxiesMother');
  const distance = mother ? dist3D(o.pos,mother.pos) : Infinity;
  const frame = o.actionTimer++;
  switch (o.action) {
    case 0:
      o.vars.animation = 3;
      if (frame === 0) {
        o.vars.turn = Math.floor(Math.random()*0x400);
        o.vars.range = Math.random()*100;
        o.vars.speed = Math.random();
        o.forwardVel = 0;
      }
      if (frame === 0 && distance<1000) o.setAction(5);
      else if (o.distanceToMario<1000 && o.distanceToMario>600+o.vars.range) o.setAction(1);
      else if (o.distanceToMario<300+o.vars.range) o.setAction(2);
      if (o.distanceToMario>2000 && dist3D(o.home,ctx.mario.pos)>2000) o.pos=[...o.home];
      break;
    case 1:
    case 2: {
      const flee = o.action === 2;
      o.vars.animation = 0;
      o.forwardVel = 3+o.vars.speed;
      o.turnTowardAngle(o.angleToMario+(flee?0x8000:0),0x600+o.vars.turn);
      if (flee && frame === 0 && distance<1000) o.setAction(5);
      else if (flee ? o.distanceToMario>500+o.vars.range : o.distanceToMario<300+o.vars.range || o.distanceToMario>1100) o.setAction(0);
      break;
    }
    case 3:
      if (frame===6) ctx.penguinSound(o.pos,true);
      if (frame>=6) o.vars.animation = 1;
      if (frame>25 && ctx.mario.action!==ACT_DIVE_SLIDE) o.setAction(4);
      break;
    case 4:
      if (frame>20) { o.forwardVel=0; o.vars.animation=2; }
      if (frame>40) o.setAction(o.vars.beforeDive);
      break;
    case 5:
      if (mother) {
        o.vars.animation = 0;
        o.forwardVel = o.distanceToMario<1000 ? 2 : 0;
        const yaw = atan2s(mother.pos[2]-o.pos[2],mother.pos[0]-o.pos[0]);
        o.turnTowardAngle(yaw+(distance>200 ? 0 : 0x8000),0x400);
      }
      break;
  }
  if ([1,2,5].includes(o.action) && ctx.mario.action===ACT_DIVE_SLIDE) {
    o.vars.beforeDive=o.action;
    o.setAction(3);
  }
  o.faceYaw=o.moveYaw;
  o.stepPhysics(ctx.world,30);
}

export function motherBehavior(o: SimObject, ctx: ObjectContext): void {
  placeOnFloor(o,ctx);
  const m=ctx.mario;
  o.forwardVel = 0;
  o.vars.animation = 3;
  if (o.action === 1 && o.link) {
    if (o.link.heldState !== HELD_FREE) return;
    if (o.link.param === o.param) {
      o.link.vars.returned = 1;
      ctx.penguinReunited([o.pos[0],o.pos[1]+200,o.pos[2]]);
    } else o.link.vars.rejected = 1;
    o.setAction(2);
  }
  let nearest: SimObject | undefined;
  let distance = Infinity;
  for (const baby of ctx.objects) {
    if (!baby.active || baby.behavior !== 'babyPenguin' || baby.vars.returned || baby.vars.rejected) continue;
    const d=dist3D(baby.pos,o.pos);
    if (d<distance) {nearest=baby; distance=d;}
  }
  if (nearest && distance<300 && nearest.heldState!==HELD_FREE) {
    o.link=nearest;
    nearest.vars.stayNearMother=1;
    ctx.dialogue(nearest.param===o.param ? 'You found my precious, precious baby!' : "That's not my baby! She looks nothing like me!");
    if (m.heldObj===nearest) { m.dropHeldObject(); m.setAction(ACT_IDLE); }
    o.setAction(1);
  } else if (o.action===0 && distance>=500 && o.distanceToMario<300 && !o.vars.told) {
    ctx.dialogue('Egad! My baby!! Have you seen my baby???');
    o.vars.told=1;
  } else if (o.distanceToMario>450) o.vars.told=0;
  if (o.action===2 && m.heldObj?.vars.returned && dist3D(o.pos,m.heldObj.pos)<1000) {
    const range=Math.hypot(m.pos[0]-o.home[0],m.pos[2]-o.home[2]);
    if(range>800)o.vars.stopped=1;
    if(range<700)o.vars.stopped=0;
    if(!o.vars.stopped) {
      o.forwardVel=10;
      o.vars.animation=0;
      o.turnTowardAngle(o.angleToMario,0x400);
    }
  }
  o.faceYaw=o.moveYaw;
  o.stepPhysics(ctx.world,30);
}
