import type * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import type { MarioRuntime } from '../sim/MarioRuntime';
import type { Vec3 } from '../sim/math';
import { DoorCollider } from './roomDoors';
import type { CollisionData } from './nativeCollision';

export const SWITCH_TOP = 'actors/capswitch/cap_switch_geo.glb';
export const SWITCH_BASE = 'actors/capswitch/cap_switch_base.glb';

export class WingSwitch {
  private readonly collider: DoorCollider;
  private readonly top: THREE.Group;
  private readonly pos: Vec3;
  private height = -1;
  private readonly collision: CollisionData;
  private timer = -1;
  private readonly originalY: number[];

  constructor(root: THREE.Group, world: CollisionWorld, pos: Vec3, shapes: Record<string,CollisionData>, make: (path:string)=>THREE.Group) {
    this.pos = [pos[0],pos[1]+71,pos[2]];
    const base = make(SWITCH_BASE);
    base.scale.multiplyScalar(0.5);
    base.position.fromArray(pos);
    this.top = make(SWITCH_TOP);
    this.top.scale.multiplyScalar(0.5);
    this.top.position.fromArray(this.pos);
    root.add(base,this.top);
    const scaled = (shape: CollisionData): CollisionData => ({...shape,vertices:shape.vertices.map(([x,y,z])=>[x/2,y/2,z/2])});
    new DoorCollider(world,scaled(shapes.switchBase),0).move(pos);
    this.collision = scaled(shapes.switchTop);
    this.collider = new DoorCollider(world,this.collision,0);
    this.originalY = this.collision.vertices.map(v=>v[1]);
    this.reset(false);
  }

  private resize(value: number): void {
    if (this.height === value) return;
    this.height = value;
    this.top.scale.y = this.top.scale.x*value;
    for (let i=0;i<this.collision.vertices.length;i++) this.collision.vertices[i][1] = this.originalY[i]*value;
    this.collider.move(this.pos);
  }

  reset(pressed: boolean): void {
    this.timer = pressed ? 5 : -1;
    this.resize(pressed ? 0.2 : 1);
  }

  step(m: MarioRuntime): boolean {
    const pressed = this.timer < 0 && this.collider.owns(m.floor) && m.pos[1] <= m.floorHeight+1;
    if (pressed) this.timer = 0;
    if (this.timer >= 0 && this.timer < 5) this.resize(1-0.2*this.timer++);
    return pressed;
  }
}
