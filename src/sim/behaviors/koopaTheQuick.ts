/**
 * Koopa the Quick, simplified from
 * reference/sm64/src/game/behaviors/koopa.inc.c and the BoB trajectory.
 *
 * Dialogue and the source path-follower are approximated with a scripted race:
 * approaching Koopa starts a 2-second countdown, then he runs the source BoB
 * waypoints. Reaching the flag before him marks the race won; otherwise he
 * resets after a short delay so the race can be retried.
 */

import {
  type ObjectContext,
  type ObjectHitbox,
  type SimObject,
} from '../object';
import type { Vec3 } from '../math';

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

const KOOPA_SPEED = 24;
const START_DISTANCE = 400;
const END_DISTANCE = 260;

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
  object.vars.raceState = 0;
  object.vars.timer = 0;
  object.vars.waypoint = 0;
  object.vars.intro = 0;
  object.visible = true;
}

export function initKoopaFlag(object: SimObject): void {
  object.gravity = 0;
  object.visible = true;
}

export function koopaFlagBehavior(_object: SimObject, _ctx: ObjectContext): void {
  // The flag is the visual race endpoint; the race logic lives on the Koopa.
}

export function koopaTheQuickBehavior(object: SimObject, ctx: ObjectContext): void {
  const state = Number(object.vars.raceState ?? 0);
  const flag = ctx.findByBehavior('koopaFlag');
  const endpoint = flag?.pos ?? [3304, 4242, -4603];

  switch (state) {
    case 0: {
      object.vars.timer = 0;
      if (object.distanceToMario < START_DISTANCE) {
        if (Number(object.vars.intro ?? 0) === 0) {
          object.vars.intro = 1;
          ctx.dialogue('KOOPA THE QUICK: I AM THE FASTEST THING ON FOUR LEGS. TRY TO KEEP UP!');
        }
        object.vars.raceState = 1;
        ctx.raceEvent('start');
      }
      break;
    }

    case 1:
      object.vars.timer = Number(object.vars.timer ?? 0) + 1;
      if (Number(object.vars.timer) >= 60) {
        object.vars.raceState = 2;
        object.vars.waypoint = 0;
  object.vars.intro = 0;
      }
      break;

    case 2: {
      object.vars.timer = Number(object.vars.timer ?? 0) + 1;
      if (horizontalDistance(ctx.mario.pos, endpoint) < END_DISTANCE) {
        object.vars.raceState = 3;
        object.vars.timer = 0;
        ctx.raceEvent('won');
        break;
      }

      const waypointIndex = Number(object.vars.waypoint ?? 0);
      const target = waypointIndex < WAYPOINTS.length
        ? WAYPOINTS[waypointIndex]
        : endpoint;
      const dx = target[0] - object.pos[0];
      const dz = target[2] - object.pos[2];
      const distance = Math.hypot(dx, dz);
      if (distance < 80) {
        if (waypointIndex >= WAYPOINTS.length) {
          object.vars.raceState = 3;
          object.vars.timer = 0;
          ctx.raceEvent('lost');
          break;
        }
        object.vars.waypoint = waypointIndex + 1;
        break;
      }

      const angle = Math.atan2(dx, dz);
      object.faceYaw = angle;
      object.moveYaw = angle;
      const step = Math.min(KOOPA_SPEED, distance);
      object.pos[0] += (dx / distance) * step;
      object.pos[2] += (dz / distance) * step;
      object.pos[1] = target[1];
      break;
    }

    case 3:
      object.vars.timer = Number(object.vars.timer ?? 0) + 1;
      if (Number(object.vars.timer) >= 300) {
        object.vars.raceState = 0;
        object.vars.timer = 0;
        object.vars.waypoint = 0;
  object.vars.intro = 0;
        object.pos = [...object.home] as Vec3;
      }
      break;
  }
}

function horizontalDistance(a: Vec3, b: Vec3): number {
  return Math.hypot(a[0] - b[0], a[2] - b[2]);
}
