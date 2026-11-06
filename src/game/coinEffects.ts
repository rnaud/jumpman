import * as THREE from 'three';
import type { Vec3 } from '../sim/math';

const digitPath = (n: number) => 'actors/number/variants/number_geo/geo_switch_anim_state_'+n+'.glb';
const sparkPaths = ['actors/sparkle/sparkles_geo.glb', ...[2,4,6].map(n =>
  'actors/sparkle/variants/sparkles_geo/geo_switch_anim_state_'+n+'.glb')];

export function coinEffectPaths(numberCount: number): string[] {
  return [...Array.from({length:numberCount},(_,i) => digitPath(i+1)), ...sparkPaths];
}

export class CoinEffects {
  private readonly digits: {model:THREE.Group; age:number; velocity:number}[] = [];
  private readonly bursts: {pos:THREE.Vector3; left:number}[] = [];
  private readonly sparks: {group:THREE.Group; age:number}[] = [];
  private readonly spareSparks: THREE.Group[] = [];

  constructor(private root: THREE.Group, private make: (path: string) => THREE.Group) {}

  number(n: number, pos: THREE.Vector3): void {
    const model = this.make(digitPath(n));
    model.position.copy(pos);
    model.position.y += 25;
    this.root.add(model);
    this.digits.push({model, age:0, velocity:26});
  }

  burst(pos: THREE.Vector3 | Vec3): void {
    this.bursts.push({pos:pos instanceof THREE.Vector3 ? pos.clone() : new THREE.Vector3(...pos), left:3});
  }

  step(): void {
    for (let i=this.digits.length-1;i>=0;i--) {
      const digit = this.digits[i];
      digit.model.position.y += digit.velocity;
      digit.velocity -= 2;
      if (digit.velocity < -21) digit.velocity = 14;
      if (++digit.age === 36) {
        digit.model.position.y -= 30;
        this.burst(digit.model.position);
        digit.model.removeFromParent();
        this.digits.splice(i,1);
      }
    }
    for (let i=this.bursts.length-1;i>=0;i--) {
      const burst = this.bursts[i];
      const group = this.spareSparks.pop() ?? new THREE.Group().add(...sparkPaths.map(path => this.make(path)));
      group.position.copy(burst.pos);
      group.position.x += Math.random()*30-15;
      group.position.y += 25;
      group.position.z += Math.random()*30-15;
      this.root.add(group);
      this.sparks.push({group,age:0});
      if (--burst.left === 0) this.bursts.splice(i,1);
    }
    for (let i=this.sparks.length-1;i>=0;i--) {
      const spark = this.sparks[i], age = spark.age++;
      if (age === 10) {
        spark.group.removeFromParent();
        this.spareSparks.push(spark.group);
        this.sparks.splice(i,1);
      } else {
        spark.group.scale.setScalar(age >= 8 ? 0.6 : 1);
        const frame = Math.min(3,age >> 1);
        spark.group.children.forEach((node,index) => { node.visible = index === frame; });
      }
    }
  }

  faceCamera(camera: THREE.Camera): void {
    for (const digit of this.digits) digit.model.quaternion.copy(camera.quaternion);
    for (const spark of this.sparks) spark.group.quaternion.copy(camera.quaternion);
  }

  clear(): void {
    for (const digit of this.digits) digit.model.removeFromParent();
    for (const spark of this.sparks) spark.group.removeFromParent();
    this.digits.length = this.sparks.length = this.bursts.length = this.spareSparks.length = 0;
  }
}
