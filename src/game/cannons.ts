import * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { angleToRadians, degToAngle, toS16, type Vec3 } from '../sim/math';
import { ACT_FLAG_INTANGIBLE, ACT_IN_CANNON } from '../sim/state';
import type { CannonSeat } from '../sim/actions/cannon';
import type { CollisionData } from './nativeCollision';
import { DoorCollider } from './roomDoors';
import lidTextureUrl from '../../assets/extracted/actors/cannon_lid/cannon_lid.rgba16.png?url';

export interface CannonPlacement {pos: Vec3; yaw: number; param: number; acts?: number[]}
export const CANNON_BASE = 'actors/cannon_base/cannon_base_geo.glb';
export const CANNON_BARREL = 'actors/cannon_barrel/cannon_barrel_geo.glb';

interface Cannon extends CannonSeat {
  lid: THREE.Mesh; base: THREE.Group; barrel: THREE.Group; cover: DoorCollider;
}

export class Cannons {
  private entries: Cannon[] = [];
  private open = false;
  private opening = -1;
  private material: THREE.MeshBasicMaterial;
  private geometry = new THREE.PlaneGeometry(203,203);

  constructor(root: THREE.Group, world: CollisionWorld, collision: CollisionData, placements: CannonPlacement[],
    make: (path: string) => THREE.Group) {
    const texture = new THREE.TextureLoader().load(lidTextureUrl);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.magFilter = THREE.NearestFilter;
    this.material = new THREE.MeshBasicMaterial({map:texture});
    this.geometry.rotateX(-Math.PI/2);
    for (const spec of placements) {
      const lid = new THREE.Mesh(this.geometry,this.material);
      lid.rotation.y = angleToRadians(degToAngle(spec.yaw));
      const base = make(CANNON_BASE), barrel = make(CANNON_BARREL);
      root.add(lid,base,barrel);
      const baseYaw = toS16(spec.param << 8);
      this.entries.push({home:spec.pos, baseYaw, yaw:baseYaw, pitch:0, rise:0, shake:0, inputYaw:0, cooldown:0,
        lid, base, barrel, cover:new DoorCollider(world,collision,spec.yaw)});
    }
  }

  reset(unlocked: boolean): void {
    this.open = unlocked;
    this.opening = -1;
    for (const c of this.entries) {
      c.lid.position.fromArray(c.home);
      c.cover.move(unlocked ? null : c.home);
      c.lid.visible = !unlocked;
      c.base.visible = c.barrel.visible = false;
      c.cooldown = c.pitch = c.shake = c.rise = 0;
      c.yaw = c.baseYaw;
    }
  }

  unlock(): void {
    if (!this.open && this.opening < 0) this.opening = 0;
  }

  step(m: MarioRuntime): void {
    if (this.opening >= 0) {
      const t = this.opening++ - 61;
      if (t === 80) this.reset(true);
      else if (t >= 0) for (const c of this.entries) {
        if (t < 30) c.lid.position.y -= 0.5;
        else c.lid.position.x += 4;
        c.cover.move(c.lid.position.toArray());
      }
    }
    if (!this.open) return;
    for (const c of this.entries) {
      const [x,y,z] = c.home, baseY = y-340;
      const dist = Math.hypot(m.pos[0]-x,m.pos[2]-z);
      if (c.cooldown > 0) c.cooldown--;
      if (m.cannon !== c && !c.cooldown) {
        c.rise = c.shake = c.pitch = 0;
        c.yaw = c.baseYaw;
        if (!(m.action & ACT_FLAG_INTANGIBLE) && dist < 187 && m.pos[1] < baseY+150 && m.pos[1]+160 > baseY) {
          m.dropHeldObject();
          m.ridingShell = null;
          m.cannon = c;
          m.setAction(ACT_IN_CANNON);
        }
      }
      c.base.position.set(x+c.shake,baseY+c.rise,z+c.shake);
      c.barrel.position.copy(c.base.position);
      c.base.rotation.y = angleToRadians(c.yaw);
      c.barrel.rotation.set(-angleToRadians(c.pitch),c.base.rotation.y,0,'YXZ');
      c.base.visible = c.barrel.visible = !c.cooldown && Math.hypot(dist,m.pos[1]-baseY) < 500
        && !(m.cannon === c && m.actionState === 2);
    }
  }

  dispose(): void {
    for (const c of this.entries) c.cover.move(null);
    this.material.map?.dispose();
    this.material.dispose();
    this.geometry.dispose();
  }
}
