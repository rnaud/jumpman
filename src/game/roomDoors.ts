import type * as THREE from 'three';
import type { SaveFile } from './save';
import type { CollisionWorld, Surface } from '../sim/collision';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { atan2s, sins, coss, toS16, degToAngle, type Vec3 } from '../sim/math';
import { ACT_WALKING, ACT_DECELERATING, ACT_PULLING_DOOR, ACT_PUSHING_DOOR, ACT_ENTERING_STAR_DOOR, ACT_UNLOCKING_STAR_DOOR, ACT_IDLE } from '../sim/state';
import { addNativeCollision, type CollisionData } from './nativeCollision';

export interface RoomDoor {
  pos: Vec3;
  yaw: number;
  stars: number;
  sliding: boolean;
  model: string;
}
// get_door_save_file_flag in interaction.c.
function unlockFlag(door: RoomDoor): number {
  switch (door.stars) {
    case 1: return door.pos[1] > 500 ? 1 << 10 : 1 << 11;
    case 3: return door.pos[0] < 0 ? 1 << 12 : 1 << 13;
    case 8: return 1 << 14;
    case 30: return 1 << 15;
    case 50: return 1 << 20;
    default: return 0;
  }
}

export type DoorMotion = Record<'push' | 'pull', [number, number][]>;

// Closed door faces block every movement action, including jumps and dives.
export class DoorCollider {
  private faces = new Set<Surface>();
  constructor(private world: CollisionWorld, private data: CollisionData, private yaw: number) {}
  owns(surface: Surface | null): boolean { return !!surface && this.faces.has(surface); }
  move(pos: Vec3 | null): void {
    this.world.removeSurfaces(this.faces);
    this.faces.clear();
    if (!pos) return;
    const start = this.world.surfaces.length;
    addNativeCollision(this.world, this.data, pos, this.yaw);
    for (const face of this.world.surfaces.slice(start)) this.faces.add(face);
  }
}

interface Leaf {
  door: RoomDoor;
  model: THREE.Group;
  collision: DoorCollider;
  play?: (clip: number) => void;
  timer: number;
}

export class RoomDoors {
  private leaves: Leaf[] = [];
  private unlocking?: RoomDoor;
  private passage?: { leaf: Leaf; pull: boolean; dx: number; dz: number };
  constructor(private motion: DoorMotion) {}

  add(leaf: Omit<Leaf, 'timer'>): void {
    this.leaves.push({ ...leaf, timer: -1 });
    leaf.collision.move(leaf.door.pos);
  }

  reset(): void {
    this.passage = undefined;
    this.unlocking = undefined;
    for (const leaf of this.leaves) {
      leaf.timer = -1;
      leaf.model.position.fromArray(leaf.door.pos);
      leaf.collision.move(leaf.door.pos);
      leaf.play?.(0);
    }
  }

  step(m: MarioRuntime, save: SaveFile, notify: (message: string) => void): boolean {
    for (const leaf of this.leaves) {
      if (leaf.timer < 0) continue;
      const frame = ++leaf.timer;
      const { door } = leaf;
      if (door.sliding) {
        const offset = -8 * Math.min(17, frame, Math.max(0, 66 - frame));
        const angle = degToAngle(door.yaw);
        leaf.model.position.set(door.pos[0] + offset * coss(angle), door.pos[1], door.pos[2] - offset * sins(angle));
        if (frame <= 17 || frame >= 49) leaf.collision.move(leaf.model.position.toArray());
      }
      if (frame >= (door.sliding ? 66 : 80)) {
        leaf.timer = -1;
        leaf.collision.move(door.pos);
        leaf.play?.(0);
      }
    }
    if (this.unlocking) {
      const frame = ++m.actionTimer;
      if (frame === 64) m.actionState = 2;
      if (frame === 135) m.actionState = 3;
      if (frame >= 161) {
        save.setFlag(unlockFlag(this.unlocking));
        notify('THE DOOR HAS OPENED!');
        m.setAction(ACT_WALKING);
        this.unlocking = undefined;
      }
      return true;
    }
    if (this.passage) {
      const { leaf, pull, dx, dz } = this.passage;
      const yaw = degToAngle(leaf.door.yaw);
      const frame = ++m.actionTimer;
      const motion = this.motion[pull ? 'pull' : 'push'];
      if (leaf.door.sliding) {
        if (frame >= 15 && frame < 35) { m.pos[0] += dx; m.pos[2] += dz; }
        if (frame >= 35) {
          m.faceAngle[1] = toS16(yaw + (pull ? 0 : 0x8000));
          m.pos[0] += 12 * sins(m.faceAngle[1]);
          m.pos[2] += 12 * coss(m.faceAngle[1]);
        }
      } else {
        const [x, z] = motion[Math.min(frame - 1, motion.length - 1)];
        m.pos[0] = leaf.door.pos[0] + Math.trunc(x * coss(yaw) + z * sins(yaw));
        m.pos[2] = leaf.door.pos[2] + Math.trunc(z * coss(yaw) - x * sins(yaw));
      }
      const floor = m.world.findFloor(m.pos[0], m.pos[1] + 78, m.pos[2]);
      if (floor.surface) {
        m.floor = floor.surface;
        m.pos[1] = m.floorHeight = floor.height;
      }
      if (frame >= (leaf.door.sliding ? 48 : motion.length)) {
        m.faceAngle[1] = toS16(yaw + (pull ? 0 : 0x8000));
        m.setAction(ACT_IDLE);
        this.passage = undefined;
      }
      return true;
    }
    if (m.action !== ACT_WALKING && m.action !== ACT_DECELERATING) return false;
    for (const leaf of this.leaves) {
      const { door } = leaf;
      const [x, y, z] = door.pos;
      if (leaf.timer >= 0 || Math.hypot(x - m.pos[0], z - m.pos[2]) >= 117 || m.pos[1] > y + 100 || m.pos[1] + 160 < y) continue;
      // The 70-star door admits Mario to the endless stairs after its warning.
      if (save.stars < door.stars) {
        notify(`YOU NEED ${door.stars - save.stars} MORE STARS`);
        if (door.stars !== 70) return false;
      }
      const yaw = degToAngle(door.yaw);
      const pull = Math.abs(toS16(yaw - atan2s(z - m.pos[2], x - m.pos[0]))) <= 0x4000;
      const flag = unlockFlag(door);
      if (flag && !save.hasFlag(flag)) {
        this.unlocking = door;
        m.setAction(ACT_UNLOCKING_STAR_DOOR, pull ? 1 : 2);
        m.actionState = 1;
        m.faceAngle[1] = toS16(yaw + (pull ? 0 : 0x8000));
        m.vel = [0, 0, 0];
        m.forwardVel = 0;
        return true;
      }
      const target = yaw + 0x1555 + (pull ? 0 : 0x5556);
      const dx = x + 150 * sins(target) - m.pos[0], dz = z + 150 * coss(target) - m.pos[2];
      this.passage = { leaf, pull, dx: dx / 20, dz: dz / 20 };
      m.setAction(door.sliding ? ACT_ENTERING_STAR_DOOR : pull ? ACT_PULLING_DOOR : ACT_PUSHING_DOOR, pull ? 1 : 2);
      m.vel = [0, 0, 0];
      m.forwardVel = 0;
      m.faceAngle[1] = door.sliding ? atan2s(dz, dx) : yaw;
      leaf.timer = 0;
      if (door.sliding) {
        const mate = this.leaves.find(other => other !== leaf && other.door.sliding
          && other.door.stars === door.stars && Math.hypot(other.door.pos[0] - x, other.door.pos[2] - z) < 200);
        if (mate) mate.timer = 0;
      } else {
        leaf.collision.move(null);
        leaf.play?.(pull ? 1 : 2);
      }
      return true;
    }
    return false;
  }
}
