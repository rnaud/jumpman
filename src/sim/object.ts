/**
 * SM64 object runtime core: hitboxes, held states, object_step and the
 * interaction checks used by Bob-ombs and breakable crates.
 *
 * Constants mirror reference/sm64/{include/sm64.h,src/game/interaction.h,
 * src/game/obj_behaviors.c}. Positions and velocities stay in SM64 units and
 * advance one tick at a time, like the original 30 Hz object processor.
 */

import { FLOOR_LOWER_LIMIT, type CollisionWorld, type Surface } from './collision';
import { atan2s, coss, sins, type Vec3 } from './math';
import {
  ACT_FLAG_AIR,
  ACT_FLAG_ATTACKING,
  ACT_FLAG_SHORT_HITBOX,
  ACT_GROUND_POUND,
  ACT_GROUND_POUND_LAND,
  ACT_JUMP_KICK,
  ACT_MOVE_PUNCHING,
  ACT_PUNCHING,
  ACT_SLIDE_KICK,
  ACT_SLIDE_KICK_SLIDE,
  MARIO_KICKING,
  MARIO_PUNCHING,
  MARIO_TRIPPING,
} from './state';
import type { MarioRuntime } from './MarioRuntime';

// ---------------------------------------------------------------------------
// Interaction constants (interaction.h / object_constants.h / obj_behaviors.c)
// ---------------------------------------------------------------------------

export const INTERACT_POLE = 1 << 6;
export const INTERACT_GRABBABLE = 1 << 1;
export const INTERACT_DAMAGE = 1 << 3;
export const INTERACT_BREAKABLE = 1 << 9;
export const INTERACT_MR_BLIZZARD = 1 << 21;

export const INT_SUBTYPE_GRABS_MARIO = 0x00000004;
export const INT_SUBTYPE_HOLDABLE_NPC = 0x00000010;
export const INT_SUBTYPE_DROP_IMMEDIATELY = 0x00000040;
export const INT_SUBTYPE_KICKABLE = 0x00000100;
export const INT_SUBTYPE_NOT_GRABBABLE = 0x00000200;

export const INT_STATUS_MARIO_STUNNED = 1 << 0;
export const INT_STATUS_MARIO_KNOCKBACK_DMG = 1 << 1;
export const INT_STATUS_MARIO_DROP_OBJECT = 1 << 3;
export const INT_STATUS_ATTACKED_MARIO = 1 << 13;
export const INT_STATUS_WAS_ATTACKED = 1 << 14;
export const INT_STATUS_INTERACTED = 1 << 15;
export const INT_STATUS_TOUCHED_BOB_OMB = 1 << 23;

export const ATTACK_PUNCH = 1;
export const ATTACK_KICK_OR_TRIP = 2;
export const ATTACK_FROM_ABOVE = 3;
export const ATTACK_GROUND_POUND_OR_TWIRL = 4;
export const ATTACK_FAST_ATTACK = 5;
export const ATTACK_FROM_BELOW = 6;

export const HELD_FREE = 0;
export const HELD_HELD = 1;
export const HELD_THROWN = 2;
export const HELD_DROPPED = 3;

export const OBJ_COL_FLAG_GROUNDED = 1 << 0;
export const OBJ_COL_FLAG_HIT_WALL = 1 << 1;
export const OBJ_COL_FLAG_UNDERWATER = 1 << 2;
export const OBJ_COL_FLAG_NO_Y_VEL = 1 << 3;

export type ObjectBehaviorKind = 'bobomb' | 'breakableBox' | 'breakableBoxSmall' | 'chainChomp' | 'woodenPost' | 'chainChompGate' | 'chainPart' | 'purpleSwitch' | 'openableGrill' | 'grillDoor' | 'pitBowlingBall' | 'kingBobomb' | 'koopaTheQuick' | 'koopaFlag' | 'tree' | 'waterBombCannon' | 'cannonBarrel' | 'waterBomb';

export interface ObjectHitbox {
  interactType: number;
  downOffset: number;
  damageOrCoinValue: number;
  health: number;
  numLootCoins: number;
  radius: number;
  height: number;
  hurtboxRadius: number;
  hurtboxHeight: number;
}

export interface ObjectSpec {
  behavior: ObjectBehaviorKind;
  pos: Vec3;
  /** s16 angle. */
  yaw: number;
  param?: number;
  model?: string;
}

export interface ObjectContext {
  world: CollisionWorld;
  mario: MarioRuntime;
  explode(source: SimObject, radius: number, damage: number): void;
  spawnCoin(position: Vec3): void;
  breakObject(source: SimObject): void;
  bossDefeated(position: Vec3): void;
  raceEvent(state: 'start' | 'won' | 'lost'): void;
  spawn(spec: ObjectSpec): SimObject;
  findByBehavior(behavior: ObjectBehaviorKind): SimObject | undefined;
}

export interface ObjectBody {
  pos: Vec3;
  hitbox: ObjectHitbox;
}

export function hitboxesOverlap(a: ObjectBody, b: ObjectBody): boolean {
  const aBottom = a.pos[1] - a.hitbox.downOffset;
  const bBottom = b.pos[1] - b.hitbox.downOffset;
  if (aBottom > bBottom + b.hitbox.height) return false;
  if (aBottom + a.hitbox.height < bBottom) return false;

  const dx = a.pos[0] - b.pos[0];
  const dz = a.pos[2] - b.pos[2];
  const radius = a.hitbox.radius + b.hitbox.radius;
  return dx * dx + dz * dz < radius * radius;
}

export function marioHitbox(m: MarioRuntime): ObjectHitbox {
  const short = Boolean(m.action & ACT_FLAG_SHORT_HITBOX);
  return {
    interactType: 0,
    downOffset: 0,
    damageOrCoinValue: 0,
    health: 0,
    numLootCoins: 0,
    radius: 37,
    height: short ? 100 : 160,
    hurtboxRadius: 0,
    hurtboxHeight: 0,
  };
}

export function marioOverlapsObject(m: MarioRuntime, object: SimObject): boolean {
  return hitboxesOverlap({ pos: m.pos, hitbox: marioHitbox(m) }, object);
}

/** determine_interaction (interaction.c). */
export function detectInteraction(m: MarioRuntime, object: SimObject): number {
  const action = m.action;
  let interaction = 0;

  if (action & ACT_FLAG_ATTACKING) {
    if (action === ACT_PUNCHING || action === ACT_MOVE_PUNCHING || action === ACT_JUMP_KICK) {
      const dYaw = atan2s(object.pos[2] - m.pos[2], object.pos[0] - m.pos[0]) - m.faceAngle[1];
      if (m.flags & MARIO_PUNCHING) {
        if (dYaw >= -0x2aaa && dYaw <= 0x2aaa) interaction = ATTACK_PUNCH;
      }
      if (m.flags & MARIO_KICKING) {
        if (dYaw >= -0x2aaa && dYaw <= 0x2aaa) interaction = ATTACK_KICK_OR_TRIP;
      }
      if (m.flags & MARIO_TRIPPING) {
        if (dYaw >= -0x4000 && dYaw <= 0x4000) interaction = ATTACK_KICK_OR_TRIP;
      }
    } else if (action === ACT_GROUND_POUND) {
      if (m.vel[1] < 0) interaction = ATTACK_GROUND_POUND_OR_TWIRL;
    } else if (action === ACT_GROUND_POUND_LAND) {
      interaction = ATTACK_GROUND_POUND_OR_TWIRL;
    } else if (action === ACT_SLIDE_KICK || action === ACT_SLIDE_KICK_SLIDE) {
      interaction = ATTACK_FAST_ATTACK;
    } else if (m.forwardVel <= -26 || m.forwardVel >= 26) {
      interaction = ATTACK_FAST_ATTACK;
    }
  }

  if (interaction === 0 && action & ACT_FLAG_AIR) {
    if (m.vel[1] < 0) {
      if (m.pos[1] > object.pos[1]) interaction = ATTACK_FROM_ABOVE;
    } else if (m.pos[1] < object.pos[1]) {
      interaction = ATTACK_FROM_BELOW;
    }
  }

  return interaction;
}

export function attackObject(object: SimObject, interaction: number): number {
  let attackType = 0;
  switch (interaction) {
    case ATTACK_GROUND_POUND_OR_TWIRL:
      attackType = ATTACK_GROUND_POUND_OR_TWIRL;
      break;
    case ATTACK_PUNCH:
      attackType = ATTACK_PUNCH;
      break;
    case ATTACK_KICK_OR_TRIP:
      attackType = ATTACK_KICK_OR_TRIP;
      break;
    case ATTACK_FAST_ATTACK:
      attackType = ATTACK_FAST_ATTACK;
      break;
    case ATTACK_FROM_ABOVE:
      attackType = ATTACK_FROM_ABOVE;
      break;
    case ATTACK_FROM_BELOW:
      attackType = ATTACK_FROM_BELOW;
      break;
    default:
      break;
  }
  object.interactStatus = attackType + (INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED);
  return attackType;
}

/** push_mario_out_of_object (interaction.c). */
export function pushMarioOutOfObject(m: MarioRuntime, object: SimObject, padding: number): void {
  const radius = marioHitbox(m).radius;
  const minDistance = object.hitbox.radius + radius + padding;
  const offsetX = m.pos[0] - object.pos[0];
  const offsetZ = m.pos[2] - object.pos[2];
  const distance = Math.hypot(offsetX, offsetZ);
  if (distance >= minDistance) return;

  const pushAngle = distance === 0 ? m.faceAngle[1] : atan2s(offsetZ, offsetX);
  const next: Vec3 = [
    object.pos[0] + minDistance * sins(pushAngle),
    m.pos[1],
    object.pos[2] + minDistance * coss(pushAngle),
  ];
  m.world.f32FindWallCollision(next, 60, 50);
  const floor = m.world.findFloor(next[0], next[1], next[2]);
  if (floor.surface) {
    m.pos[0] = next[0];
    m.pos[2] = next[2];
  }
}

// ---------------------------------------------------------------------------
// SimObject
// ---------------------------------------------------------------------------

let nextObjectId = 1;

export class SimObject implements ObjectBody {
  readonly id = nextObjectId++;
  readonly behavior: ObjectBehaviorKind;
  readonly model?: string;
  readonly param: number;

  pos: Vec3;
  readonly home: Vec3;
  vel: Vec3 = [0, 0, 0];
  moveYaw: number;
  faceYaw: number;
  forwardVel = 0;
  gravity = 0;
  friction = 1;
  buoyancy = 1;

  hitbox: ObjectHitbox = {
    interactType: 0,
    downOffset: 0,
    damageOrCoinValue: 0,
    health: 1,
    numLootCoins: 0,
    radius: 50,
    height: 100,
    hurtboxRadius: 0,
    hurtboxHeight: 0,
  };

  interactStatus = 0;
  interactSubtype = 0;
  heldState = HELD_FREE;
  action = 0;
  prevAction = 0;
  actionState = 0;
  actionTimer = 0;
  timer = 0;
  scale = 1;
  graphYOffset = 0;
  billboard = false;
  visible = true;
  active = true;
  respawnTimer = -1;
  distanceToMario = Infinity;
  angleToMario = 0;
  grounded = false;
  hitWall = false;
  /** Behavior-local scalar state, mirroring the per-behavior o* fields. */
  readonly vars: Record<string, number> = {};
  /** Runtime links used by compound objects such as the Chain Chomp. */
  link?: SimObject;
  links?: SimObject[];
  linkIndex?: number;
  chainSegments?: Vec3[];
  collisionSurfaces?: Surface[];

  constructor(spec: ObjectSpec) {
    this.behavior = spec.behavior;
    this.model = spec.model;
    this.param = spec.param ?? 0;
    this.pos = [...spec.pos] as Vec3;
    this.home = [...spec.pos] as Vec3;
    this.moveYaw = spec.yaw;
    this.faceYaw = spec.yaw;
  }

  setHitbox(hitbox: ObjectHitbox): void {
    this.hitbox = hitbox;
  }

  setAction(action: number): void {
    this.prevAction = this.action;
    this.action = action;
    this.actionState = 0;
    this.actionTimer = 0;
  }

  setHeldState(state: number): void {
    this.heldState = state;
  }

  deactivate(respawnFrames = -1): void {
    this.active = false;
    this.visible = false;
    this.respawnTimer = respawnFrames;
  }

  reactivate(): void {
    this.active = true;
    this.visible = true;
    this.respawnTimer = -1;
    this.pos = [...this.home] as Vec3;
    this.vel = [0, 0, 0];
    this.forwardVel = 0;
    this.moveYaw = this.faceYaw;
    this.heldState = HELD_FREE;
    this.interactStatus = 0;
    this.actionState = 0;
    this.actionTimer = 0;
    this.timer = 0;
    this.scale = 1;
    this.grounded = false;
    this.hitWall = false;
    for (const key of Object.keys(this.vars)) delete this.vars[key];
  }

  /**
   * object_step (obj_behaviors.c): walls, floor, gravity, friction and water.
   * Returns the source collision flags.
   */
  stepPhysics(world: CollisionWorld): number {
    let result = 0;
    const velX = this.forwardVel * sins(this.moveYaw);
    const velZ = this.forwardVel * coss(this.moveYaw);
    const next: Vec3 = [this.pos[0] + velX, this.pos[1], this.pos[2] + velZ];

    const wall = world.resolveAndReturnWallCollisions(
      next,
      this.hitbox.height / 2,
      this.hitbox.radius,
    );
    if (wall) {
      this.pos[0] = next[0];
      this.pos[2] = next[2];
      this.turnAwayFromSurface(velX, velZ, wall);
      result |= OBJ_COL_FLAG_HIT_WALL;
    } else {
      this.pos[0] += velX;
      this.pos[2] += velZ;
    }

    const floor = world.findFloor(this.pos[0], this.pos[1], this.pos[2]);
    const floorY = floor.surface ? floor.height : FLOOR_LOWER_LIMIT;
    const waterY = world.waterLevel(this.pos[0], this.pos[2]);

    if (floor.surface && floor.surface.normal[1] < 0.5 && floorY > this.pos[1]) {
      this.turnAwayFromSurface(velX, velZ, floor.surface);
      result |= OBJ_COL_FLAG_HIT_WALL;
    } else if (waterY > this.pos[1]) {
      this.stepUnderwater(floor.surface, floorY, velX, velZ);
      result |= OBJ_COL_FLAG_UNDERWATER;
    } else {
      this.stepOnFloor(floor.surface, floorY, velX, velZ);
    }


    if (Math.trunc(this.pos[1]) === Math.trunc(floorY)) result |= OBJ_COL_FLAG_GROUNDED;
    if (Math.trunc(this.vel[1]) === 0) result |= OBJ_COL_FLAG_NO_Y_VEL;
    this.grounded = Boolean(result & OBJ_COL_FLAG_GROUNDED);
    this.hitWall = Boolean(result & OBJ_COL_FLAG_HIT_WALL);
    return result;
  }

  private stepOnFloor(surface: Surface | null, floorY: number, velX: number, velZ: number): void {
    this.vel[1] -= this.gravity;
    this.vel[1] = Math.max(-75, Math.min(75, this.vel[1]));
    this.pos[1] += this.vel[1];

    if (this.pos[1] < floorY) {
      this.pos[1] = floorY;
      this.vel[1] = this.vel[1] < -17.5 ? -(this.vel[1] / 2) : 0;
    }

    if (!surface) return;
    if (Math.trunc(this.pos[1]) >= Math.trunc(floorY) && Math.trunc(this.pos[1]) < Math.trunc(floorY) + 37) {
      const nx = surface.normal[0];
      const ny = surface.normal[1];
      const nz = surface.normal[2];
      const denom = nx * nx + ny * ny + nz * nz;
      if (denom > 0) {
        velX += (nx * (nx * nx + nz * nz) / denom) * this.gravity * 2;
        velZ += (nz * (nx * nx + nz * nz) / denom) * this.gravity * 2;
      }
      if (Math.abs(velX) < 0.000001) velX = 0;
      if (Math.abs(velZ) < 0.000001) velZ = 0;
      if (velX !== 0 || velZ !== 0) this.moveYaw = atan2s(velZ, velX);
      const friction = ny < 0.2 && this.friction < 0.9999 ? 0 : this.friction;
      this.forwardVel = Math.hypot(velX, velZ) * friction;
    }
  }

  private stepUnderwater(surface: Surface | null, floorY: number, velX: number, velZ: number): void {
    const netYAccel = (1 - this.buoyancy) * -this.gravity;
    this.vel[1] -= netYAccel;
    this.vel[1] = Math.max(-75, Math.min(75, this.vel[1]));
    this.pos[1] += this.vel[1];

    if (this.pos[1] < floorY) {
      this.pos[1] = floorY;
      this.vel[1] = this.vel[1] < -17.5 ? -(this.vel[1] / 2) : 0;
    }

    if (!surface) return;
    if (Math.trunc(this.pos[1]) >= Math.trunc(floorY) && Math.trunc(this.pos[1]) < Math.trunc(floorY) + 37) {
      const nx = surface.normal[0];
      const ny = surface.normal[1];
      const nz = surface.normal[2];
      const denom = nx * nx + ny * ny + nz * nz;
      if (denom > 0) {
        velX += (nx * (nx * nx + nz * nz) / denom) * netYAccel * 2;
        velZ += (nz * (nx * nx + nz * nz) / denom) * netYAccel * 2;
      }
      if (Math.abs(velX) < 0.000001) velX = 0;
      if (Math.abs(velZ) < 0.000001) velZ = 0;
      if (velX !== 0 || velZ !== 0) this.moveYaw = atan2s(velZ, velX);
      const friction = ny < 0.2 && this.friction < 0.9999 ? 0 : this.friction;
      this.forwardVel = Math.hypot(velX, velZ) * friction;
    }
  }

  private turnAwayFromSurface(velX: number, velZ: number, surface: Surface): void {
    const nx = surface.normal[0];
    const nz = surface.normal[2];
    const denom = nx * nx + nz * nz;
    if (denom < 0.000001) {
      this.moveYaw += 0x7fff;
      return;
    }
    const yawX = ((nz * nz - nx * nx) * velX) / denom - (2 * velZ * (nx * nz)) / denom;
    const yawZ = ((nx * nx - nz * nz) * velZ) / denom - (2 * velX * (nx * nz)) / denom;
    this.moveYaw = atan2s(yawZ, yawX);
  }

  /** obj_check_if_facing_toward_angle */
  facingTowardAngle(base: number, goal: number, range: number): boolean {
    const dAngle = (goal & 0xffff) - (base & 0xffff);
    return sins(-range) < sins(dAngle) && sins(dAngle) < sins(range) && coss(dAngle) > 0;
  }

  /** obj_turn_toward_object */
  turnTowardAngle(goal: number, turnAmount: number): void {
    const delta = (goal & 0xffff) - (this.moveYaw & 0xffff);
    const shortest = delta > 0x8000 ? delta - 0x10000 : delta;
    if (Math.abs(shortest) <= turnAmount) this.moveYaw = goal;
    else this.moveYaw += shortest > 0 ? turnAmount : -turnAmount;
  }
}

/** obj_return_home_if_safe (obj_behaviors.c). */
export function returnHomeIfSafe(object: SimObject, mario: MarioRuntime, distance: number): boolean {
  const dx = object.home[0] - object.pos[0];
  const dz = object.home[2] - object.pos[2];
  const mdx = object.home[0] - mario.pos[0];
  const mdy = object.home[1] - mario.pos[1];
  const mdz = object.home[2] - mario.pos[2];
  if (mdx * mdx + mdy * mdy + mdz * mdz < distance * distance) return true;
  object.turnTowardAngle(atan2s(dz, dx), 320);
  return false;
}
