import { atan2s, clamp, coss, dist3D, sins, toS16 } from '../math';
import type { MarioRuntime } from '../MarioRuntime';
import { HELD_FREE, HELD_HELD, HELD_THROWN, INTERACT_GRABBABLE, marioOverlapsObject,
  type ObjectContext, type SimObject } from '../object';
import { SURFACE_BURNING } from '../surfaces';

const WALK = 0, THROWN = 1, RETURNING = 2, DEFEATED = 3;

export function initBowser(object: SimObject): void {
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox({ interactType: INTERACT_GRABBABLE, downOffset: 0, damageOrCoinValue: 2,
    health: object.param === 3 ? 3 : 1, numLootCoins: 0, radius: 400, height: 400,
    hurtboxRadius: 0, hurtboxHeight: 0 });
  object.vars.health = object.hitbox.health;
}

export function atBowserTail(o: SimObject, mario: MarioRuntime): boolean {
  // Tail anchor projected behind the root; radius and vertical offset from bhvBowserTailAnchor.
  const dx = mario.pos[0] - (o.pos[0] - 300 * sins(o.moveYaw));
  const dz = mario.pos[2] - (o.pos[2] - 300 * coss(o.moveYaw));
  return dx * dx + dz * dz < 137 * 137 && mario.pos[1] < o.pos[1] + 100 && mario.pos[1] + 160 > o.pos[1] + 50;
}

export function initBowserMine(o: SimObject): void {
  o.setHitbox({ interactType: 0, downOffset: 40, damageOrCoinValue: 0, health: 1, numLootCoins: 0,
    radius: 40, height: 40, hurtboxRadius: 0, hurtboxHeight: 0 });
}

export function bowserMineBehavior(o: SimObject, ctx: ObjectContext): void {
  if (marioOverlapsObject(ctx.mario, o)) {
    ctx.explode(o, 150, 2);
    o.deactivate(-1);
  }
}

function reboundToArena(o: SimObject): void {
  o.moveYaw = atan2s(-o.pos[2], -o.pos[0]);
  o.vel[1] = 100;
  o.forwardVel = o.param === 3 ? 400 : 200;
  o.setAction(RETURNING);
}

export function bowserBehavior(o: SimObject, ctx: ObjectContext): void {
  if (o.heldState === HELD_HELD) {
    o.vars.heldTicks = (o.vars.heldTicks || 0) + 1;
    o.vars.animation = o.vars.heldTicks < 36 ? 3 : 2;
    return;
  }
  o.vars.heldTicks = 0;
  if (o.heldState !== HELD_FREE) {
    const spin = o.heldState === HELD_THROWN ? Math.abs(o.vars.spin || 0) : 0;
    let speed = spin / 3000 * 70;
    if (speed > 90) speed *= 2.5;
    o.forwardVel = coss(-spin) * speed;
    o.vel[1] = -sins(-spin) * speed;
    o.setHeldState(HELD_FREE);
    o.setAction(THROWN);
  }

  if (o.action === DEFEATED) {
    o.vars.animation = 17;
    if (o.distanceToMario < 700 && Math.abs(toS16(ctx.mario.faceAngle[1] - o.angleToMario)) > 0x6000) {
      ctx.bowserDefeated(o.pos);
      o.deactivate(-1);
    }
    return;
  }
  if (o.action === WALK) {
    if (o.timer % 120 === 0) ctx.spawnObject({ behavior: 'bowserFlame',
      model: 'actors/bowser_flame/bowser_flames_geo.glb',
      pos: [o.pos[0], o.pos[1] + 220, o.pos[2]], yaw: o.faceYaw });
    const turn = o.param === 2 || o.vars.health >= 3 ? 0x400 : o.vars.health === 2 ? 0x300 : 0x200;
    o.moveYaw = toS16(o.moveYaw + clamp(toS16(o.angleToMario - o.moveYaw), -turn, turn));
    o.faceYaw = o.moveYaw;
    o.forwardVel = o.distanceToMario < 3000 ? 3.5 : 0;
    o.vars.animation = o.forwardVel ? 13 : 12;
  }
  if (o.action === RETURNING) {
    o.vars.animation = o.vars.health > 0 ? 25 : 16;
    o.forwardVel = Math.min(o.forwardVel, Math.hypot(o.pos[0], o.pos[2]));
  }
  if (o.action === THROWN) o.vars.animation = 2;
  o.stepPhysics(ctx.world);
  if (o.action === THROWN) {
    let distance = 800;
    let nearest: SimObject | undefined;
    for (const mine of ctx.objects) {
      if (!mine.active || mine.behavior !== 'bowserMine') continue;
      const d = dist3D(o.pos, mine.pos);
      if (d < distance) { nearest = mine; distance = d; }
    }
    if (nearest) {
      ctx.explode(nearest, 150, 2);
      nearest.deactivate(-1);
      o.vars.health--;
      reboundToArena(o);
      return;
    }
    if (o.grounded) o.setAction(WALK);
  } else if (o.action === RETURNING && o.grounded) {
    o.forwardVel = 0;
    o.setAction(o.vars.health > 0 ? WALK : DEFEATED);
  }
  const floor = o.grounded ? ctx.world.findFloor(...o.pos).surface : null;
  if (o.pos[1] < o.home[1] - 1000 || Math.max(Math.abs(o.pos[0]), Math.abs(o.pos[2])) > 8000 || floor?.type === SURFACE_BURNING) {
    o.pos = [0, o.home[1] + 2000, 0];
    o.forwardVel = o.vel[1] = 0;
    o.setAction(RETURNING);
  }
}
