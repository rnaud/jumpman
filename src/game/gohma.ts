import * as THREE from 'three';
import type { DungeonPart } from './dekuDungeon';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { A_BUTTON, B_BUTTON, type ControllerSnapshot } from '../sim/state';
import type { SaveFile } from './save';
import type { SeedTarget } from './fairySlingshot';

type Phase = 'crawl' | 'drop' | 'stunned' | 'attack' | 'rise' | 'dying' | 'cleared';

export class Gohma {
  private phase: Phase='crawl';
  private ticks=0;
  private lastHit=0;
  private health=3;
  private vulnerable=false;
  private readonly eye:THREE.Object3D;
  private readonly position=new THREE.Vector3();
  private readonly mixer:THREE.AnimationMixer;
  private readonly clips=new Map<string,THREE.AnimationAction>();
  private readonly portal=new THREE.Mesh(new THREE.CylinderGeometry(150,195,130,48,1,true),
    new THREE.MeshBasicMaterial({color:0x59bcff,transparent:true,opacity:.55,side:THREE.DoubleSide,depthWrite:false}));
  private readonly halo=new THREE.Mesh(new THREE.RingGeometry(45,180,48),
    new THREE.MeshBasicMaterial({color:0x93eaff,transparent:true,opacity:.75,side:THREE.DoubleSide}));
  private readonly glint=new THREE.Mesh(new THREE.SphereGeometry(26,12,8),new THREE.MeshBasicMaterial({color:0xff4433}));
  prompt:string|null=null;
  constructor(private boss:DungeonPart,private heart:DungeonPart,clips:THREE.AnimationClip[],root:THREE.Group,private floor:number) {
    this.mixer=new THREE.AnimationMixer(boss.model);
    for(const clip of clips)this.clips.set(clip.name,this.mixer.clipAction(clip));
    this.eye=boss.model.getObjectByName('joint_4')!;
    this.eye.add(this.glint);
    this.portal.position.copy(heart.home);this.portal.position.x+=480;this.portal.position.y=floor+65;
    this.halo.rotation.x=-Math.PI/2;this.halo.position.y=-59;
    this.portal.add(this.halo);root.add(this.portal);
    this.heart.model.traverse(node=>{
      if(node instanceof THREE.Mesh) {
        node.material=node.material.clone();
        node.material.color.setHex(0xff555e);
      }
    });
  }
  private transition(phase:Phase,clip:string) {
    this.phase=phase;this.ticks=0;
    this.mixer.stopAllAction();this.clips.get(clip)?.reset().play();
  }
  restore(save:SaveFile) {
    this.health=3;this.lastHit=0;this.vulnerable=false;
    const won=save.dekuCleared('gohma');
    this.transition(won?'cleared':'crawl','Idle');
    this.boss.model.visible=!won;this.glint.visible=false;
    this.heart.model.visible=won && !save.dekuCleared('heart');this.portal.visible=won;
    this.boss.model.position.copy(this.boss.home);
  }
  dispose(){
    this.mixer.stopAllAction();
    for(const mesh of [this.glint,this.halo,this.portal]){mesh.geometry.dispose();mesh.material.dispose();}
  }
  targets():SeedTarget[]{
    if(!this.vulnerable)return [];
    return [{center:this.eye.getWorldPosition(new THREE.Vector3()),radius:100,hit:()=>{
      if(!this.vulnerable)return;
      this.vulnerable=false;this.glint.visible=false;
      this.transition('drop','Stunned');
    }}];
  }
  step(m:MarioRuntime,c:ControllerSnapshot,save:SaveFile,aiming:boolean,link:boolean,notify:(s:string)=>void,returnToForest:()=>void) {
    this.ticks++;this.prompt=null;
    const model=this.boss.model;
    this.position.fromArray(m.pos);
    this.mixer.update(1/30);
    if(this.phase==='cleared'){
      this.portal.scale.x=this.portal.scale.z=1+.07*Math.sin(this.ticks*.08);
      this.heart.model.rotation.y+=.045;
      this.heart.model.position.y=this.floor+92+16*Math.sin(this.ticks*.08);
      if(this.heart.model.visible && this.position.distanceTo(this.heart.model.position)<240){
        save.clearDekuChallenge('heart');this.heart.model.visible=false;
        m.maxHealth=0x980;m.health=m.maxHealth;notify('Heart Container! Your maximum health increased.');
      }
      if(this.position.distanceTo(this.portal.position)<290){
        this.prompt='SPACE / A · ENTER THE BLUE WARP';
        if(c.buttonPressed&A_BUTTON)returnToForest();
      }
      return;
    }
    const t=this.ticks;
    if(this.phase==='crawl'){
      model.rotation.z=Math.PI;
      model.position.x=this.boss.home.x+360*Math.sin(t*.012);
      model.position.z=this.boss.home.z+180*Math.cos(t*.012);
      model.rotation.y=Math.atan2(m.pos[0]-model.position.x,m.pos[2]-model.position.z);
      this.vulnerable=t>=45;
      this.glint.visible=this.vulnerable;
      this.prompt=this.vulnerable?'QUEEN GOHMA · SHOOT THE RED EYE':'QUEEN GOHMA · WATCH THE CEILING';
      if(save.dekuSeeds===0 && !aiming){this.prompt='SPACE / A · GATHER DEKU SEEDS';if(c.buttonPressed&A_BUTTON)save.refillSeeds();}
      if(t>=240){this.glint.visible=this.vulnerable=false;this.transition('attack','Attack');}
    } else if(this.phase==='drop' || this.phase==='attack'){
      model.position.y=Math.max(this.floor,model.position.y-66);
      model.rotation.z=Math.max(0,model.rotation.z-.14);
      if(this.phase==='attack'){
        const towards=this.position.clone().sub(model.position);towards.y=0;
        model.position.addScaledVector(towards.normalize(),12);
        if(t===45 && this.position.distanceTo(model.position)<550)m.takeDamage(2,model.position.toArray());
        if(t>=65)this.transition('rise','Idle');
      } else if(model.position.y===this.floor){this.lastHit=0;this.transition('stunned','Stunned');}
    } else if(this.phase==='stunned'){
      this.prompt=link?'GOHMA IS STUNNED · J / B TO SLASH':'GOHMA IS STUNNED · J / B TO PUNCH';
      const heading=Math.atan2(model.position.x-m.pos[0],model.position.z-m.pos[2]);
      const facing=Math.cos(heading-m.faceAngle[1]*Math.PI/32768)>.35;
      if(!aiming && facing && c.buttonPressed&B_BUTTON && this.position.distanceTo(model.position)<460 && t-this.lastHit>=12){
        this.lastHit=t;
        if(--this.health===0){this.transition('dying','Death');notify('Queen Gohma defeated!');}
      }
      if(this.phase==='stunned' && t>180)this.transition('rise','Idle');
    } else if(this.phase==='rise'){
      model.position.lerp(this.boss.home,.08);model.rotation.z=Math.min(Math.PI,model.rotation.z+.09);
      if(t>=45)this.transition('crawl','Idle');
    } else if(this.phase==='dying' && t>=70){
      save.clearDekuChallenge('gohma');this.restore(save);
    }
  }
}
