// Regular Koopa and the rideable shell. Koopa the Quick shares the terrain movement below.
import type { ObjectContext, ObjectHitbox, SimObject } from '../object';
import { HELD_DROPPED, HELD_FREE, HELD_HELD, HELD_THROWN } from '../object';
import { approachF32, atan2s, coss, dist3D, sins, toS16 } from '../math';
import { ATTACK_FROM_ABOVE, ATTACK_GROUND_POUND_OR_TWIRL, ATTACK_FAST_ATTACK, ATTACK_KICK_OR_TRIP } from '../object';

const KOOPA_HITBOX: ObjectHitbox = {
  interactType: 0x80,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: -1,
  radius: 60,
  height: 40,
  hurtboxRadius: 40,
  hurtboxHeight: 30,
};

const SHELL_HITBOX: ObjectHitbox = {
  interactType: 0x80000,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
  radius: 50,
  height: 50,
  hurtboxRadius: 50,
  hurtboxHeight: 50,
};

export function initKoopa(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(KOOPA_HITBOX);
  object.vars.rest = 0;
  object.vars.animation = 7;
  object.vars.hit = 0;
  object.visible = true;
}

export function initKoopaShell(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(SHELL_HITBOX);
  object.vars.sliding = 0;
  object.vars.timer = 0;
  object.visible = true;
}

// cur_obj_move_standard(-78) for Koopa's zero drag and bounciness.
export function moveKoopa(o: SimObject, ctx: ObjectContext): void {
  const wall = ctx.world.resolveAndReturnWallCollisions(o.pos,10,50);
  o.hitWall = !!wall && wall.normal[0]*sins(o.moveYaw)+wall.normal[2]*coss(o.moveYaw)<0;
  const floor = ctx.world.findFloor(...o.pos);
  const x = o.pos[0]+o.forwardVel*sins(o.moveYaw), z = o.pos[2]+o.forwardVel*coss(o.moveYaw);
  const next = ctx.world.findFloor(x,o.pos[1],z);
  const delta = next.height-floor.height;
  o.vars.edge = 0;
  if (!next.surface || (delta < -50 && o.grounded) || (delta < 5 && next.surface.normal[1] <= 0.208)) {
    o.vars.edge = 1;
  } else if (next.surface.normal[1]>0.208 || o.pos[1]>next.height) {
    o.pos[0] = x;
    o.pos[2] = z;
  } else o.hitWall = true;
  o.vel[1] = Math.max(-78,o.vel[1]-o.gravity);
  o.pos[1] += o.vel[1];
  o.grounded = o.pos[1] <= floor.height;
  if (o.grounded) {
    o.pos[1] = floor.height;
    o.vel[1] = 0;
  }
}

export const KoopaAction = {Idle:0, Walk:1, Flee:2, Lying:3, Run:4, Dive:5, Squished:6, Knockback:7} as const;

function shedShell(o: SimObject, ctx: ObjectContext, stomp: boolean): void {
  ctx.koopaSound(o.pos,'shell');
  o.vars.unshelled = 1;
  o.vars.animation = 2;
  o.setAction(KoopaAction.Lying);
  o.vars.rest = 0;
  o.forwardVel = 20;
  if (!stomp) o.moveYaw = toS16(o.angleToMario+0x8000);
  if (o.link && !o.link.active) {
    o.link.reactivate();
    initKoopaShell(o.link);
    o.link.pos = [...o.pos];
  } else {
    o.link = ctx.spawn({behavior:'koopaShell',model:'actors/koopa_shell/koopa_shell_geo.glb',pos:o.pos,yaw:o.moveYaw});
  }
}

export function koopaBehavior(o: SimObject, ctx: ObjectContext): void {
  const attack = o.vars.hit;
  const t = o.actionTimer++;
  if (attack) {
    o.vars.hit = 0;
    const stomp = attack === ATTACK_FROM_ABOVE || attack === ATTACK_GROUND_POUND_OR_TWIRL;
    if (!o.vars.unshelled) shedShell(o,ctx,stomp);
    else {
      if (stomp) ctx.koopaSound(o.pos,'stomp');
      o.setAction(stomp ? KoopaAction.Squished : KoopaAction.Knockback);
      o.vars.vertical = Number(attack === ATTACK_FAST_ATTACK || attack === ATTACK_KICK_OR_TRIP);
      o.forwardVel = stomp ? 0 : o.vars.vertical ? 20 : 50;
      o.vel[1] = stomp ? 0 : o.vars.vertical ? 50 : 30;
      o.moveYaw = toS16(o.angleToMario+0x8000);
    }
  }
  if (o.action === KoopaAction.Squished || o.action === KoopaAction.Knockback) {
    const finished = o.action === KoopaAction.Squished ? o.actionTimer > 15
      : o.actionTimer > 1 && (o.grounded || o.hitWall || (o.vars.vertical && o.actionTimer >= 9));
    if (finished) {
      ctx.koopaSound(o.pos,'defeated');
      ctx.spawnCoin(o.pos,true,5);
      o.deactivate();
      return;
    }
  } else if (o.action === KoopaAction.Lying || o.action === KoopaAction.Dive) {
    if (o.grounded) o.forwardVel = Math.max(0,o.forwardVel-(o.vars.unshelled ? 0.5 : 0.3));
    if (o.action === KoopaAction.Dive && t > 10 && o.link?.active && dist3D(o.pos,o.link.pos)<50
        && dist3D(ctx.mario.pos,o.link.pos)>200) {
      o.link.deactivate();
      o.vars.unshelled = 0;
      o.vars.animation = 5;
      o.forwardVel *= 0.5;
      o.setAction(KoopaAction.Lying);
    }
    if (!o.forwardVel && o.vars.rest >= 20) o.vars.animation = 6;
    if (!o.forwardVel && ++o.vars.rest >= 34) {
      o.vars.rest = 0;
      o.setAction(o.vars.unshelled ? KoopaAction.Run : KoopaAction.Idle);
    }
  } else if (o.action === KoopaAction.Run) {
    o.vars.animation = 3;
    const shell = o.link?.active ? o.link : undefined;
    const yaw = shell ? atan2s(shell.pos[2]-o.pos[2],shell.pos[0]-o.pos[0]) : o.moveYaw;
    const away = toS16(o.angleToMario+0x8000);
    o.turnTowardAngle(o.distanceToMario>800 || Math.abs(toS16(yaw-away))<0x2000 ? yaw : away,0x600);
    o.forwardVel = approachF32(o.forwardVel,20,1,1);
    if (shell && o.forwardVel===20 && dist3D(o.pos,shell.pos)<600 && Math.abs(toS16(yaw-o.moveYaw))<0xc00) {
      o.setAction(KoopaAction.Dive);
      o.vars.animation = 5;
      o.moveYaw = yaw;
      o.forwardVel *= 1.2;
      o.vel[1] = dist3D(o.pos,shell.pos)/20;
    }
  } else if (o.action === KoopaAction.Flee) {
    o.vars.animation = 1;
    const homeward = dist3D(o.home,o.pos)>1000;
    if (t>30 && !homeward && o.distanceToMario>800) {
      o.forwardVel = Math.max(0,o.forwardVel-1);
      if (!o.forwardVel) o.setAction(KoopaAction.Idle);
    } else {
      o.turnTowardAngle(homeward ? atan2s(o.home[2]-o.pos[2],o.home[0]-o.pos[0]) : o.angleToMario+0x8000,0x400);
      o.forwardVel = approachF32(o.forwardVel,17,1,1);
    }
  } else {
    if (o.action === KoopaAction.Idle) {
      o.forwardVel = 0;
      o.vars.animation = 7;
      if (t >= 59) {
        o.vars.target = o.moveYaw+(Math.random()<0.5 ? -0x2000 : 0x2000);
        o.vars.walkFrames = 34+30+Math.floor(Math.random()*100);
        o.setAction(KoopaAction.Walk);
      }
    } else {
      o.vars.animation = t<34 ? 11 : t<o.vars.walkFrames ? 9 : 10;
      if (o.hitWall || o.vars.edge) o.vars.target = o.moveYaw+0x8000;
      o.turnTowardAngle(o.vars.target,0x200);
      o.forwardVel = approachF32(o.forwardVel,t<o.vars.walkFrames ? 3 : 0,0.3,1);
      if (t>=o.vars.walkFrames+14) o.setAction(KoopaAction.Idle);
    }
    if (o.distanceToMario<300 && Math.abs(toS16(o.angleToMario-o.moveYaw))<0x3000) o.setAction(KoopaAction.Flee);
  }
  moveKoopa(o,ctx);
  o.faceYaw = o.moveYaw;
}

export function koopaShellBehavior(object: SimObject, ctx: ObjectContext): void {
  object.vars.rideCooldown = Math.max(0, Number(object.vars.rideCooldown ?? 0) - 1);
  if (object.heldState === HELD_HELD) {
    object.visible = true;
    return;
  }
  if (object.heldState === HELD_THROWN || object.heldState === HELD_DROPPED) {
    object.heldState = HELD_FREE;
    object.vars.sliding = 1;
    object.forwardVel = 20;
  }

  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  object.stepPhysics(ctx.world);

  const dx = ctx.mario.pos[0] - object.pos[0];
  const dz = ctx.mario.pos[2] - object.pos[2];
  if (
    !ctx.mario.ridingShell
    && Number(object.vars.sliding)
    && Number(object.vars.rideCooldown) === 0
    && dx * dx + dz * dz < 70 * 70
    && ctx.mario.pos[1] > object.pos[1] + 20
    && ctx.mario.pos[1] < object.pos[1] + 180
    && ctx.mario.vel[1] <= 0
  ) {
    object.vars.rideCooldown = 10;
    ctx.mario.rideShell(object);
    return;
  }

  if (Number(object.vars.sliding)) {
    object.faceYaw = object.moveYaw;
    const step = object.stepPhysics(ctx.world);
    if (step === 1) {
      object.forwardVel = -object.forwardVel;
      object.moveYaw = (object.moveYaw + 0x8000) & 0xffff;
    }
    if (ctx.mario.ridingShell === object) return;
    const dy = ctx.mario.pos[1] - object.pos[1];
    if (
      Number(object.vars.rideCooldown) === 0
      && dx * dx + dz * dz < 90 * 90
      && dy > -40 && dy < 140
      && ctx.mario.takeDamage(1, object.pos)
    ) {
      ctx.mario.events.push({ type: 'hurt', damage: 1 });
      object.forwardVel *= -0.5;
    }
  } else {
    object.faceYaw = (object.faceYaw + 0x1000) & 0xffff;
  }
}
