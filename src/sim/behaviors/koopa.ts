/**
 * Normal Koopa and its shell, simplified from
 * reference/sm64/src/game/behaviors/koopa.inc.c and koopa_shell.inc.c.
 * Stomping or punching a Koopa removes the shell; a shell can be punched or
 * stomped again to send it sliding, and a sliding shell damages Mario.
 */

import type { ObjectContext, ObjectHitbox, SimObject } from '../object';
import { HELD_DROPPED, HELD_FREE, HELD_HELD, HELD_THROWN } from '../object';
import type { Vec3 } from '../math';

const KOOPA_HITBOX: ObjectHitbox = {
  interactType: 0x80,
  downOffset: 0,
  damageOrCoinValue: 1,
  health: 0,
  numLootCoins: 0,
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
  object.vars.timer = 0;
  object.vars.dir = 1;
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

export function koopaBehavior(object: SimObject, ctx: ObjectContext): void {
  if (object.heldState === HELD_THROWN || object.heldState === HELD_DROPPED) {
    object.heldState = HELD_FREE;
  }
  object.vars.timer = Number(object.vars.timer ?? 0) + 1;
  if (object.vars.timer % 90 === 0) object.vars.dir = -Number(object.vars.dir ?? 1);

  if (object.vars.hit) {
    object.vars.hit = 0;
    ctx.spawn({
      behavior: 'koopaShell',
      model: 'actors/koopa_shell/koopa_shell_geo.glb',
      pos: [...object.pos] as Vec3,
      yaw: object.moveYaw,
    });
    object.deactivate(-1);
    return;
  }

  object.forwardVel = 1.2 * Number(object.vars.dir ?? 1);
  object.faceYaw = object.moveYaw;
  const step = object.stepPhysics(ctx.world);
  if (step === 1) {
    object.vars.dir = -Number(object.vars.dir ?? 1);
    object.forwardVel = -object.forwardVel;
  }
  void ctx;
}

export function koopaShellBehavior(object: SimObject, ctx: ObjectContext): void {
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

  if (Number(object.vars.sliding)) {
    object.faceYaw = object.moveYaw;
    const step = object.stepPhysics(ctx.world);
    if (step === 1) {
      object.forwardVel = -object.forwardVel;
      object.moveYaw = (object.moveYaw + 0x8000) & 0xffff;
    }
    const dx = ctx.mario.pos[0] - object.pos[0];
    const dy = ctx.mario.pos[1] - object.pos[1];
    const dz = ctx.mario.pos[2] - object.pos[2];
    if (
      dx * dx + dz * dz < 90 * 90
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
