import * as THREE from 'three';
import type { CollisionWorld, Surface } from '../sim/collision';
import type { Vec3 } from '../sim/math';
import type { KokiriScene } from './kokiriVillage';
import type { SaveFile } from './save';

type Quest = NonNullable<KokiriScene['quest']>;

/** The sword and Mido's permission are separate save flags. Re-entering a room
 * restores the final poses directly, without replaying the reward or dialogue. */
export class KokiriSwordQuest {
  private readonly opening: THREE.AnimationAction;
  private walls = new Set<Surface>();
  private opened = false;
  private permitted = false;
  private rewardTime = 0;
  private walking = false;
  private readonly destination: THREE.Vector3;
  private readonly home: THREE.Vector3;

  constructor(private data: Quest, private collision: CollisionWorld, chest: THREE.Group,
    private sword: THREE.Group, private mido: THREE.Group, private talk: Vec3,
    private mixer: THREE.AnimationMixer, clip: THREE.AnimationClip) {
    this.opening=mixer.clipAction(clip).setLoop(THREE.LoopOnce,1);
    this.opening.clampWhenFinished=true;
    sword.position.copy(chest.position);
    this.home=new THREE.Vector3(...data.path[0]);
    this.destination=new THREE.Vector3(...data.path[data.path.length-1]);
  }

  private blockPassage(block:boolean) {
    if (block === (this.walls.size>0)) return;
    if (!block) {
      this.collision.removeSurfaces(this.walls);
      this.walls.clear();
      return;
    }
    // The neck between the two rock walls at Mido's home position. Both
    // windings are needed by the shared one-sided wall solver.
    const {x,y,z}=this.home;
    const halfWidth=440;
    const a:Vec3=[x,y-190,z-halfWidth], b:Vec3=[x,y-190,z+halfWidth];
    const c:Vec3=[x,y+1350,z+halfWidth], d:Vec3=[x,y+1350,z-halfWidth];
    this.walls=new Set([...this.collision.addQuad(a,b,c,d),...this.collision.addQuad(b,a,d,c)]);
  }

  restore(save:SaveFile) {
    this.opened=save.kokiriSword;
    this.permitted=save.dekuPathOpen;
    this.walking=false;
    this.rewardTime=0;
    this.sword.visible=false;
    this.opening.reset().play();
    this.opening.paused=true;
    this.opening.time=this.opened ? this.opening.getClip().duration : 0;
    this.mixer.update(0);
    this.mido.position.copy(this.permitted ? this.destination : this.home);
    this.mido.position.toArray(this.talk);
    this.blockPassage(!this.permitted);
  }

  step(save:SaveFile, player:Vec3, talking:boolean) {
    if (save.kokiriSword && !this.opened) {
      this.opened=true;
      this.opening.reset().setEffectiveTimeScale(1.5).play();
      this.rewardTime=90;
    }
    if (this.rewardTime>0) {
      const elapsed=90-this.rewardTime--;
      this.sword.visible=elapsed>=24 && this.rewardTime>0;
      this.sword.position.y=this.data.chest[1]+Math.min(245,95+elapsed*2.6);
      this.sword.rotation.y=elapsed*0.065;
    }
    if (save.dekuPathOpen && !this.permitted) {
      this.permitted=true;
      this.walking=true;
      this.blockPassage(false);
    }
    const pos=this.mido.position;
    if (this.walking) {
      const delta=this.destination.clone().sub(pos);
      const distance=delta.length();
      this.mido.rotation.y=Math.atan2(delta.x,delta.z);
      if (distance<=6.6) {pos.copy(this.destination);this.walking=false;}
      else pos.addScaledVector(delta,6.6/distance);
    } else if (!this.permitted && !talking) {
      // EnMd_BlockPath intercepts Link on a 60 OoT-unit radius.
      const angle=Math.atan2(player[0]-this.home.x,player[2]-this.home.z);
      pos.set(this.home.x+264*Math.sin(angle),this.home.y,this.home.z+264*Math.cos(angle));
    }
    if (!this.walking) this.mido.rotation.y=Math.atan2(player[0]-pos.x,player[2]-pos.z);
    pos.toArray(this.talk);
  }

  get isWalking() {return this.walking;}
}
