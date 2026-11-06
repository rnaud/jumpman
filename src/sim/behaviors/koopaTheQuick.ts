import {
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';
import { approachF32, atan2s, clamp, coss, dist3D, sins, toS16, type Vec3 } from '../math';
import { ACT_FLAG_AIR, ACT_SHOT_FROM_CANNON } from '../state';

export const KoopaRace = {
  Waiting:0, Invitation:1, Countdown:2, Running:3, Decelerating:4, AtSummit:5, ResultDialog:6, Finished:7,
} as const;
export const KOOPA_RACE_STAR: Vec3 = [3030,4500,-4600];

const WAYPOINTS: Vec3[] = [
  [-2220, 204, 5520], [-2020, 204, 5820], [760, 765, 5680], [1920, 768, 5040],
  [2000, 768, 4360], [1240, 768, 3600], [-280, 768, 2720], [-1680, 768, 1840],
  [-2280, 0, 1800], [-2720, 0, 1120], [-2520, 0, 480], [-1720, 0, 120],
  [560, 630, -840], [2700, 1571, -1500], [4520, 1830, -2600], [6240, 1943, -3500],
  [6380, 2066, -6180], [4000, 2390, -7720], [-120, 2608, -5515], [-180, 2641, -4860],
  [580, 2769, -3380], [1670, 2867, -2580], [2216, 2900, -2126], [3167, 2965, -1870],
  [4069, 3050, -2487], [4880, 3037, -3416], [5020, 3181, -5760], [4660, 3354, -6300],
  [3940, 3514, -6800], [3200, 3619, -6850], [1290, 3768, -5793], [1150, 3900, -3670],
  [2980, 4046, -2930], [4334, 4186, -3680], [4180, 4242, -4220], [3660, 4242, -4380],
];

const KOOPA_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 180,
  height: 300,
  hurtboxRadius: 180,
  hurtboxHeight: 300,
};

export function initKoopaTheQuick(object: SimObject): void {
  object.scale = 3;
  object.gravity = 4;
  object.friction = 1;
  object.setHitbox(KOOPA_HITBOX);
  object.vars.animation = 7;
}

export function initKoopaFlag(object: SimObject): void {
  object.gravity = 0;
  object.visible = true;
}

export function koopaFlagBehavior(flag: SimObject, ctx: ObjectContext): void {
  const koopa = ctx.findByBehavior('koopaTheQuick');
  if (!koopa || koopa.action < KoopaRace.Running || flag.vars.result) return;
  koopa.vars.elapsed++;
  if (dist3D(flag.pos, koopa.pos) < 400) flag.vars.result = -2;
  else if (flag.distanceToMario < 400) flag.vars.result = koopa.vars.cannon ? -1 : 1;
}

// cur_obj_move_standard(-78) for Koopa's zero drag and bounciness.
function moveKoopa(o: SimObject, ctx: ObjectContext): void {
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

function followPath(o: SimObject, ctx: ObjectContext, result: number): void {
  const index = o.vars.waypoint;
  const prev = WAYPOINTS[index], next = WAYPOINTS[index+1];
  const dx = next[0]-o.pos[0], dy = next[1]-o.pos[1], dz = next[2]-o.pos[2];
  if ((next[0]-prev[0])*dx + (next[1]-prev[1])*dy + (next[2]-prev[2])*dz <= 0) {
    if (++o.vars.waypoint === WAYPOINTS.length-1) {
      o.setAction(KoopaRace.Decelerating);
      return;
    }
  }
  const agility = result && o.distanceToMario > 1500 && index < 28 ? 8 : 4;
  const delta = toS16(atan2s(dz,dx)-o.moveYaw);
  o.moveYaw = toS16(o.moveYaw + clamp(delta,-150*agility,150*agility));
  o.faceYaw = o.moveYaw;
  const starting = o.actionTimer++ < 34;
  const pitch = atan2s(Math.hypot(dx,dz),-dy);
  const speed = starting ? 3*agility : 6*agility*(1+sins(pitch));
  const accel = agility*(starting ? 0.3 : 0.1);
  o.forwardVel = approachF32(o.forwardVel,speed,accel,accel);
  o.vars.animation = starting ? 11 : 9;
  if (o.hitWall) o.vel[1] = 20;
  if (o.vars.edge && o.grounded) {
    o.vel[1] = index < 8 ? 40 : 80;
    o.gravity = 6;
  }
  moveKoopa(o,ctx);
  if (!o.grounded && o.vel[1] > 0) o.vars.animation = 13;
}

export function koopaTheQuickBehavior(o: SimObject, ctx: ObjectContext): void {
  const flag = ctx.findByBehavior('koopaFlag');
  if (!flag) return;
  const dy = ctx.mario.pos[1] - o.pos[1];
  const lateral = Math.hypot(ctx.mario.pos[0]-o.pos[0],ctx.mario.pos[2]-o.pos[2]);
  const canTalk = lateral < 400 && dy > -160 && dy < 400 && !(ctx.mario.action & ACT_FLAG_AIR);
  const radius = o.action === KoopaRace.Running ? 180 : 140;
  if (Math.abs(dy) < 300 && lateral < radius) {
    ctx.mario.pos[0] = o.pos[0] + radius*sins(o.angleToMario);
    ctx.mario.pos[2] = o.pos[2] + radius*coss(o.angleToMario);
  }
  switch (o.action) {
    case KoopaRace.Waiting:
      if (o.vars.cooldown > 0) o.vars.cooldown--;
      else if (canTalk) {
        ctx.dialogue('KOOPA THE QUICK: How about a race to the mountaintop?');
        o.setAction(KoopaRace.Invitation);
      }
      break;
    case KoopaRace.Invitation:
      if (o.vars.answer < 0) {
        o.vars.cooldown = 60;
        o.setAction(KoopaRace.Waiting);
      } else if (o.vars.answer > 0) {
        o.vars.waypoint = o.vars.elapsed = o.vars.cannon = 0;
        o.setAction(KoopaRace.Countdown);
      }
      break;
    case KoopaRace.Countdown:
      if (o.actionTimer++ === 50) {
        ctx.raceEvent('start');
        o.setAction(KoopaRace.Running);
      }
      break;
    case KoopaRace.Running:
      if (ctx.mario.action === ACT_SHOT_FROM_CANNON) o.vars.cannon = 1;
      followPath(o,ctx,flag.vars.result);
      break;
    case KoopaRace.Decelerating:
      o.forwardVel = approachF32(o.forwardVel,3,1,1);
      moveKoopa(o,ctx);
      if (o.actionTimer++ >= 20) {
        o.vars.animation = 10;
        o.setAction(KoopaRace.AtSummit);
      }
      break;
    case KoopaRace.AtSummit:
      o.forwardVel = 0;
      moveKoopa(o,ctx);
      if (o.actionTimer++ >= 14 && canTalk && flag.vars.result) {
        const result = flag.vars.result;
        o.vars.animation = 7;
        ctx.dialogue(result === 1 ? "Whoa! You...really...are...fast! Here you go--you've won it, fair and square!"
          : result === -1 ? "You've gotta run the whole course!" : "I win! You lose! Better luck next time!");
        o.setAction(KoopaRace.ResultDialog);
      }
      break;
    case KoopaRace.ResultDialog:
      if (o.vars.answer) {
        ctx.raceEvent(flag.vars.result === 1 ? 'won' : flag.vars.result === -1 ? 'disqualified' : 'lost');
        o.setAction(KoopaRace.Finished);
      }
      break;
  }
}
