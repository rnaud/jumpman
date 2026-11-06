import * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { atan2s, clamp, coss, sins, angleToRadians, degToAngle, toS16, type Vec3 } from '../sim/math';
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

// cutscene_enter_cannon_raise and update_in_cannon, evaluated on the 30 Hz clock.
export class CannonCamera {
  fovOffset = 0;
  private readonly distance: number;
  private readonly yaw: number;
  private readonly pitch: number;

  constructor(camera: THREE.Camera, seat: CannonSeat) {
    const [x,y,z] = seat.home;
    const dx = camera.position.x-x, dy = camera.position.y-(y-340), dz = camera.position.z-z;
    this.distance = Math.hypot(dx,dy,dz);
    this.yaw = atan2s(dz,dx);
    this.pitch = atan2s(Math.hypot(dx,dz),dy);
  }

  update(camera: THREE.Camera, mario: MarioRuntime): void {
    const seat = mario.cannon!;
    const t = mario.actionTimer-1;
    const shakeTick = t-71; // frame 70 of the raise shot, after the initial camera frame
    this.fovOffset = shakeTick < 0 ? 0 : coss(shakeTick*0x8000)*Math.max(0,0x100-0x30*shakeTick)/0x100;
    if (mario.actionState === 2) {
      const [pitch,yaw] = mario.faceAngle;
      const eye = camera.position.fromArray(mario.pos);
      eye.y += 125;
      const offset = Math.max(0,800-(t-122)*100);
      camera.lookAt(eye.x+800*coss(pitch)*sins(yaw), eye.y+offset+800*sins(pitch), eye.z+800*coss(pitch)*coss(yaw));
      return;
    }
    if (t <= 0) return;
    const [x,y,z] = seat.home;
    const distance = this.distance + clamp(400-this.distance,-5*(t-1),5*(t-1));
    const yaw = this.yaw + (t-1)*0x40;
    const pitch = this.pitch + clamp(-this.pitch,-t*0x80,t*0x80);
    const baseY = y-340;
    const eye = camera.position.set(x+distance*coss(pitch)*sins(yaw),
      baseY+Math.min(t,69)*5+distance*sins(pitch)+2*t, z+distance*coss(pitch)*coss(yaw));
    const floor = mario.world.findFloor(eye.x,eye.y+500,eye.z);
    if (floor.surface) eye.y = Math.max(eye.y,floor.height+100);
    camera.lookAt(x+seat.shake+100*sins(seat.pitch)*sins(seat.yaw), baseY+seat.rise+100*coss(seat.pitch),
      z+seat.shake+100*sins(seat.pitch)*coss(seat.yaw));
  }
}
