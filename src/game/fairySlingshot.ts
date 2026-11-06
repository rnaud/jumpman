import * as THREE from 'three';
import { ACT_IDLE, A_BUTTON, B_BUTTON, type ControllerSnapshot } from '../sim/state';
import type { MarioRuntime } from '../sim/MarioRuntime';
import type { SaveFile } from './save';

export interface SeedTarget { center:THREE.Vector3; radius:number; hit():void }

/** Aiming uses the same ray for the sight, the seed's flight and collision with scenery. */
export class FairySlingshot {
  aiming=false;
  private yaw=0;
  private pitch=0;
  private readonly origin=new THREE.Vector3();
  private readonly direction=new THREE.Vector3();
  private readonly ray=new THREE.Raycaster();
  private readonly seed=new THREE.Mesh(new THREE.SphereGeometry(9,8,6),new THREE.MeshStandardMaterial({color:0xca9949}));
  private remaining=0;
  private readonly velocity=new THREE.Vector3();
  private impact?:()=>void;

  constructor(private model:THREE.Group, root:THREE.Object3D, private terrain:THREE.Object3D) {
    root.add(model,this.seed);
    this.reset();
  }
  reset() {
    this.aiming=false;
    this.model.visible=this.seed.visible=false;
    this.remaining=0;
    this.impact=undefined;
  }
  dispose() {this.seed.geometry.dispose();this.seed.material.dispose();}

  step(m:MarioRuntime,c:ControllerSnapshot,aim:boolean,save:SaveFile,targets:SeedTarget[],toast:(s:string)=>void) {
    if (this.remaining>0) {
      const travel=Math.min(180,this.remaining);
      this.seed.position.addScaledVector(this.velocity,travel);
      this.remaining-=travel;
      if (!this.remaining) { this.seed.visible=false; this.impact?.(); this.impact=undefined; }
    }
    if (!aim || !save.slingshotEquipped || !m.floor || Math.abs(m.pos[1]-m.floorHeight)>35) {
      this.aiming=this.model.visible=false;
      return false;
    }
    if (!this.aiming) { this.yaw=m.faceAngle[1]*Math.PI/32768; this.pitch=0; }
    this.aiming=this.model.visible=true;
    this.yaw-=c.stickX*.0007;
    this.pitch=THREE.MathUtils.clamp(this.pitch+c.stickY*.00055,-1.3,1.35);
    this.direction.set(Math.sin(this.yaw)*Math.cos(this.pitch),Math.sin(this.pitch),Math.cos(this.yaw)*Math.cos(this.pitch));
    this.origin.fromArray(m.pos).y+=130;
    this.model.position.copy(this.origin).addScaledVector(this.direction,85);
    this.model.position.x-=Math.cos(this.yaw)*28;
    this.model.position.z+=Math.sin(this.yaw)*28;
    this.model.position.y-=25;
    this.model.rotation.set(-this.pitch,this.yaw,0,'YXZ');
    m.faceAngle[1]=Math.round(this.yaw*32768/Math.PI);
    m.forwardVel=0; m.vel=[0,0,0];
    if(m.action!==ACT_IDLE) m.setAction(ACT_IDLE);
    if (!this.remaining && c.buttonPressed & (A_BUTTON|B_BUTTON)) {
      if (!save.useSeed()) { toast('Out of seeds. Refill at the slingshot chest.'); return true; }
      this.ray.set(this.origin,this.direction);
      this.ray.far=7200;
      let distance=this.ray.intersectObject(this.terrain,false)[0]?.distance ?? this.ray.far;
      this.impact=undefined;
      for (const target of targets) {
        const point=this.ray.ray.intersectSphere(new THREE.Sphere(target.center,target.radius),new THREE.Vector3());
        if (!point) continue;
        const reach=point.distanceTo(this.origin);
        if (reach<=distance) {distance=reach;this.impact=target.hit;}
      }
      this.remaining=Math.max(1,distance);
      this.velocity.copy(this.direction);
      this.seed.position.copy(this.origin);
      this.seed.visible=true;
    }
    return true;
  }
  frame(camera:THREE.PerspectiveCamera) {
    camera.position.copy(this.origin);
    camera.lookAt(this.origin.clone().add(this.direction));
  }
}
