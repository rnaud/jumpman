import type { Group, Vector3 } from 'three';
import { angleToRadians, approachF32, atan2s, clamp, coss, sins, toS16, type Vec3 } from '../sim/math';
import type { CollisionWorld } from '../sim/collision';
import { SURFACE_DEATH_PLANE } from '../sim/surfaces';

export interface Goomba {
  model: Group;
  top: number;
  radius: number;
}

/** Regular Goombas have separate attack and damage cylinders. Mario's hurt radius is zero. */
export function touchesGoombaHurtbox(feet: Vec3, marioHeight: number, enemy: Goomba): boolean {
  const p = enemy.model.position;
  return Math.hypot(feet[0]-p.x,feet[2]-p.z) < 42*1.5
    && feet[1] <= p.y+40*1.5 && feet[1]+marioHeight >= p.y;
}

export abstract class GoombaEnemy implements Goomba {
  readonly radius = 72*1.5;
  protected dead = false;
  private squashFrame = -1;
  protected vy = 0;
  private readonly baseScale: Vector3;

  constructor(readonly model: Group) { this.baseScale = model.scale.clone(); }
  get top(): number { return this.model.position.y+50*1.5; }
  get tangible(): boolean { return this.model.visible && !this.dead; }

  reset(): void {
    this.dead = false;
    this.vy = 0;
    this.squashFrame = -1;
    this.model.scale.copy(this.baseScale);
    this.model.visible = true;
    this.resetMovement();
  }

  defeat(stomp = false): void {
    this.dead = true;
    if (stomp) this.squashFrame = 0;
    else this.model.visible = false;
  }

  /** Returns true on the single tick that finishes a stomp and releases its coin. */
  step(mario: Vec3, world: CollisionWorld): boolean {
    if (!this.dead) this.move(mario,world);
    if (this.squashFrame < 0) return false;
    const frame = this.squashFrame++;
    const p = this.model.position;
    const ground = world.findFloor(p.x,p.y,p.z).height;
    this.vy = Math.max(-78,this.vy-4);
    p.y = Math.max(ground,p.y+this.vy);
    if (p.y===ground) this.vy = 0;
    const height = Math.max(0.3,1-(frame+1)*0.14);
    const width = height===0.3 ? 1.7 : 1;
    this.model.scale.copy(this.baseScale);
    this.model.scale.x *= width;
    this.model.scale.z *= width;
    this.model.scale.y *= height;
    if (frame <= 15) return false;
    this.model.visible = false;
    this.squashFrame = -1;
    return true;
  }

  protected abstract resetMovement(): void;
  protected abstract move(mario: Vec3,world: CollisionWorld): void;
  abstract alert(x: number,z: number): void;
}

/** Test the feet at the instant they cross the enemy's head during a physics tick. */
export function crossesGoomba(previous: Vector3, current: Vec3, enemy: Goomba): boolean {
  const drop = previous.y - current[1];
  if (drop <= 0 || previous.y < enemy.top || current[1] > enemy.top) return false;
  const t = (previous.y - enemy.top) / drop;
  const x = previous.x + t * (current[0] - previous.x);
  const z = previous.z + t * (current[2] - previous.z);
  return Math.hypot(x - enemy.model.position.x, z - enemy.model.position.z) < enemy.radius + 37;
}

/** Regular-size goomba_act_walk / goomba_act_jump, advanced once per object tick. */
export class WalkingGoomba extends GoombaEnemy {
  readonly home: Vector3;
  private yaw = 0;
  private target = 0;
  private speed = 0;
  private relativeSpeed = 0;
  private walkTimer = 0;
  private airborne = false;

  private awake = false;

  constructor(model: Group, private readonly spawner?: Vec3) { super(model); this.home = model.position.clone(); }

  protected resetMovement(): void {
    this.model.position.copy(this.home);
    this.model.visible = true;
    this.awake = !this.spawner;
    this.model.rotation.y = 0;
    this.yaw = this.target = this.speed = this.relativeSpeed = this.walkTimer = this.vy = 0;
    this.airborne = false;
  }

  alert(x: number, z: number): void {
    this.target = atan2s(z - this.model.position.z, x - this.model.position.x);
    this.airborne = true;
    this.speed = 0;
    this.vy = 25;
  }

  protected move(mario: Vec3, world: CollisionWorld): void {
    if (this.spawner) {
      const range = Math.hypot(mario[0] - this.spawner[0], mario[1] - this.spawner[1], mario[2] - this.spawner[2]);
      if (range > 4000 && this.awake) this.reset();
      if (range < 3000) this.awake = true;
      if (!this.awake) return;
    }
    const p = this.model.position;
    const homing = Math.hypot(p.x - this.home.x, p.z - this.home.z) > 1000;
    const nearby = !homing && Math.hypot(p.x - mario[0], p.y - mario[1], p.z - mario[2]) < 500;
    if (!this.airborne) {
      this.speed = approachF32(this.speed, this.relativeSpeed * 1.5, 0.4, 0.4);
      if (nearby) {
        if (this.relativeSpeed <= 2) this.alert(mario[0], mario[2]);
        this.target = atan2s(mario[2] - p.z, mario[0] - p.x);
        this.relativeSpeed = 20;
      } else {
        this.relativeSpeed = 4 / 3;
        if (homing) this.target = atan2s(this.home.z - p.z, this.home.x - p.x);
        else if (--this.walkTimer <= 0) {
          this.walkTimer = 100 + Math.floor(Math.random() * 100);
          const hop = Math.random() < 0.25;
          if (hop) this.alert(p.x, p.z);
          this.target = toS16(this.yaw + (Math.random() < 0.5 ? 1 : -1) * (hop ? 0x6000 : 0x2000));
        }
      }
    }
    const turn = this.airborne ? 0x800 : 0x200;
    this.yaw = toS16(this.yaw + clamp(toS16(this.target - this.yaw), -turn, turn));
    const next: Vec3 = [p.x + sins(this.yaw) * this.speed, p.y, p.z + coss(this.yaw) * this.speed];
    const wall = world.resolveAndReturnWallCollisions(next, 30, 40);
    const floor = world.findFloor(next[0], p.y + 30, next[2]);
    if (wall || !floor.surface || floor.surface.type === SURFACE_DEATH_PLANE || floor.height < p.y - 120 || floor.surface.normal[1] < Math.cos(78 * Math.PI / 180)) {
      this.target = toS16(this.yaw + 0x8000);
      this.speed = 0;
    } else {
      p.x = next[0]; p.z = next[2];
    }
    const ground = world.findFloor(p.x, p.y + 30, p.z).height;
    this.vy = Math.max(-78, this.vy - 4);
    p.y = Math.max(ground, p.y + this.vy);
    if (p.y <= ground) { this.vy = 0; this.airborne = false; }
    this.model.rotation.y = angleToRadians(this.yaw);
  }
}
