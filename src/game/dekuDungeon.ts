import * as THREE from 'three';
import type {MarioRuntime} from '../sim/MarioRuntime';
import type {CollisionWorld} from '../sim/collision';
import type {Vec3} from '../sim/math';
import {ACT_CLIMBING_POLE, ACT_FREEFALL, ACT_JUMP, ACT_WATER_IDLE, A_BUTTON, Z_TRIG, type ControllerSnapshot} from '../sim/state';
import type {DoorCollider} from './roomDoors';
import type {SaveFile} from './save';
import type {SeedTarget} from './fairySlingshot';

export interface DungeonPart { model:THREE.Group; home:THREE.Vector3; collision?:DoorCollider }
type Traverse = {from:THREE.Vector3; to:THREE.Vector3; ticks:number; elapsed:number; arc:number; done?:()=>void};
type Route = {from:THREE.Vector3; to:THREE.Vector3; prompt:string; arc:number; ticks:number; oneWay?:boolean};
const scale=4.4;

/** All anchors use the ydan scene's N64 coordinates, relative to the lobby web at (0,0,0). */
export class DekuDungeon {
  private routes:Route[];
  private climb?:Traverse;
  private peak=-Infinity;
  private ladderDrop=0;
  private clock=0;
  private waterTimer=0;
  private waterDrop=0;
  private readonly normalWater:number;
  private ridden=false;
  private nutAge=0;
  private reflected=false;
  private readonly nutVel=new THREE.Vector3();
  private readonly nut=new THREE.Mesh(new THREE.SphereGeometry(18,8,8),new THREE.MeshStandardMaterial({color:0x9c5326}));
  private readonly actorPos=new THREE.Vector3();
  prompt:string|null=null;

  constructor(private parts:Map<string,DungeonPart>,private world:CollisionWorld,root:THREE.Group) {
    root.add(this.nut);
    this.nut.visible=false;
    this.normalWater=world.waterBoxes[1][5];
    this.routes=[
      this.route([-228,0,197],[-263,195,236],'CLIMB THE LOBBY LADDER',55),
      this.route([-284,195,259],[-340,400,311],'CLIMB TO THE UPPER WALKWAY',65),
      this.route([-1020,400,1020],[-1188,390,1190],'JUMP TO THE PLATFORM',27,225,true),
      this.route([-1188,390,1190],[-1322,480,1322],'JUMP TO THE CHEST LEDGE',28,260,true),
      this.route([-1280,280,1280],[-1322,480,1322],'CLIMB THE VINES',54),
      this.route([-1068,280,1068],[-1020,400,1020],'CLIMB THE FALLEN LADDER',43),
      this.route([-340,400,311],[-20,400,20],'LEAP TOWARD THE WEB',35,160,true),
      this.route([-432,400,432],[-480,400,480],'PASS THROUGH THE DOOR',16),
      this.route([-910,400,910],[-962,400,962],'PASS THROUGH THE DOOR',16),
      this.route([-20,-913,20],[-55,-880,240],'SWIM TO THE BANK',65,12,true),
      this.route([-565,-880,990],[-835,-915,1245],'SWIM TO THE SWITCH',76,0,true),
      this.route([-75,-880,545],[-75,-880,615],'PASS THROUGH THE DOOR',16),
      this.route([-300,-880,960],[-372,-880,960],'ENTER THE WATER ROOM',16),
    ].reverse();
  }
  private part(id:string) {return this.parts.get(id)!;}
  private at(x:number,y:number,z:number) {
    return new THREE.Vector3(x*scale,y*scale,z*scale).add(this.part('web').home);
  }
  private route(a:Vec3,b:Vec3,prompt:string,ticks:number,arc=0,oneWay=false):Route {
    return {from:this.at(...a),to:this.at(...b),prompt,ticks,arc,oneWay};
  }
  private move(id:string,pos:THREE.Vector3) {
    const p=this.part(id);
    p.model.position.copy(pos);
    p.collision?.move(pos.toArray());
  }
  private visible(id:string,visible:boolean) {
    const p=this.part(id);
    p.model.visible=visible;
    p.collision?.move(visible?p.model.position.toArray():null);
  }
  private nearby(p:THREE.Vector3,r:number,height=150) {
    return Math.hypot(p.x-this.actorPos.x,p.z-this.actorPos.z)<r && Math.abs(p.y-this.actorPos.y)<height;
  }
  restore(save:SaveFile) {
    this.climb=undefined;
    this.peak=-Infinity;
    this.clock=this.waterTimer=this.waterDrop=this.nutAge=0;
    this.nut.visible=this.ridden=false;
    this.prompt=null;
    this.ladderDrop=save.dekuCleared('ladder')?280:0;
    this.move('ladder',this.part('ladder').home.clone().add(new THREE.Vector3(0,-this.ladderDrop*scale,0)));
    this.visible('web',!save.dekuCleared('web'));
    this.visible('steppingStone',!save.hasSlingshot);
    this.visible('scrub',!save.dekuCleared('scrub'));
    this.visible('gate',!save.dekuCleared('eye'));
    this.part('eye').model.visible=!save.dekuCleared('eye');
    this.world.waterBoxes[1][5]=this.normalWater;
    this.move('ferry',this.part('ferry').home);
  }
  dispose() { this.nut.geometry.dispose();this.nut.material.dispose(); }
  targets(save:SaveFile,toast:(s:string)=>void):SeedTarget[] {
    const targets:SeedTarget[]=[];
    if (!save.dekuCleared('ladder')) {
      const part=this.part('ladder');
      targets.push({center:part.home.clone().add(new THREE.Vector3(0,60*scale,0)),radius:185,hit:()=>{
        save.clearDekuChallenge('ladder');
        toast('The ladder is coming down!');
      }});
    }
    if (!save.dekuCleared('eye')) targets.push({center:this.part('eye').home,radius:80,hit:()=>{
      if (!save.dekuCleared('scrub')) {toast('The Deku Scrub is guarding this door.');return;}
      save.clearDekuChallenge('eye');
      this.part('eye').model.visible=false;
      this.visible('gate',false);
      toast('The eye switch opened the water room.');
    }});
    return targets;
  }

  private scrub(m:MarioRuntime,c:ControllerSnapshot,save:SaveFile,toast:(s:string)=>void) {
    if(save.dekuCleared('scrub')) return;
    const origin=this.part('scrub').home.clone();origin.y+=85;
    const player=this.actorPos.clone();player.y+=80;
    if(!this.nearby(origin,1500,330)) {this.nut.visible=false;this.nutAge=0;return;}
    this.prompt='HOLD K / Z TO DEFLECT THE DEKU NUT';
    if(!this.nut.visible) {
      if(++this.nutAge<42) return;
      this.nut.position.copy(origin);
      this.nutVel.copy(player).sub(origin).normalize().multiplyScalar(28);
      this.nut.visible=true;this.reflected=false;this.nutAge=0;
    }
    this.nut.position.add(this.nutVel);
    if(!this.reflected && this.nut.position.distanceTo(player)<105) {
      if(c.buttonDown & Z_TRIG) {
        this.reflected=true;
        this.nutVel.copy(origin).sub(this.nut.position).normalize().multiplyScalar(42);
      } else {
        m.takeDamage(1,origin.toArray());
        this.nut.visible=false;
      }
    }
    if(this.reflected && this.nut.position.distanceTo(origin)<100) {
      save.clearDekuChallenge('scrub');
      this.visible('scrub',false);this.nut.visible=false;
      toast('The Scrub yields. Shoot the golden eye above the door.');
    }
    if(++this.nutAge>110) {this.nut.visible=false;this.nutAge=0;}
  }

  private water(m:MarioRuntime,c:ControllerSnapshot,save:SaveFile,toast:(s:string)=>void) {
    const ferry=this.part('ferry'), log=this.part('log');
    const aboard=ferry.collision!.owns(m.floor) && Math.abs(m.pos[1]-m.floorHeight)<35;
    if(this.waterTimer>0)this.waterTimer--;
    this.waterDrop=THREE.MathUtils.clamp(this.waterDrop+(this.waterTimer>0?1.5:-1.5),0,47);
    const water=this.normalWater-this.waterDrop*scale;
    this.world.waterBoxes[1][5]=water;
    const next=ferry.home.clone();
    next.x+=Math.sin(this.clock*Math.PI/192)*165*scale;
    next.y=water+20*scale;
    if(aboard){
      m.pos[0]+=next.x-ferry.model.position.x;
      m.pos[1]+=next.y-ferry.model.position.y;
      if(this.waterDrop>30 && next.x<log.home.x-150) this.ridden=true;
    }
    this.move('ferry',next);
    log.model.rotation.x=this.clock*.056;
    const inPath=Math.abs(this.actorPos.z-log.home.z)<340 && Math.abs(this.actorPos.x-log.home.x)<145;
    if(inPath && this.actorPos.y>log.home.y-170 && this.actorPos.y<log.home.y+200) {
      m.takeDamage(1,log.home.toArray());
      m.pos=this.at(-575,-880,1020).toArray();
      m.vel=[0,0,0];m.forwardVel=0;m.setAction(ACT_FREEFALL);
      toast('Lower the water to clear the spiked log.');
    }
    const floorSwitch=this.part('switch').home;
    if (this.waterTimer && this.nearby(floorSwitch,270,770)) {
      this.prompt='SPACE / A · SWIM TO THE PLATFORM';
      if(c.buttonPressed & A_BUTTON && !this.climb) {
        const landing=ferry.home.clone();
        landing.x+=Math.sin((this.clock+44)*Math.PI/192)*165*scale;
        landing.y=this.normalWater-27*scale+12;
        this.climb={from:this.actorPos.clone(),to:landing,ticks:45,elapsed:0,arc:0};
      }
    } else if(aboard && this.ridden && next.x<ferry.home.x-120*scale){
      this.prompt='SPACE / A · JUMP TO THE FAR BANK';
      if(c.buttonPressed & A_BUTTON && !this.climb) {
        this.climb={from:this.actorPos.clone(),to:this.at(-1150,-880,1050),ticks:32,elapsed:0,arc:230};
        m.setAction(ACT_JUMP);
      }
    } else if(this.nearby(floorSwitch,270,770)){
      this.prompt='SPACE / A · DIVE TO THE WATER SWITCH';
      if(c.buttonPressed & A_BUTTON) {
        this.climb={from:this.actorPos.clone(),to:floorSwitch.clone(),ticks:27,elapsed:0,arc:0,done:()=>{
          this.waterTimer=480;
          toast('Water draining. Board the floating platform.');
          m.setAction(ACT_WATER_IDLE);
        }};
      }
    } else if(aboard) this.prompt=this.waterTimer?'RIDE UNDER THE LOG · STEP OFF AT THE FAR BANK':'DIVE TO THE BLUE SWITCH TO LOWER THE WATER';
    if(this.ridden && this.actorPos.x<this.at(-1130,0,0).x && Math.abs(this.actorPos.z-log.home.z)<750 &&
       this.actorPos.y>this.at(0,-910,0).y && !save.dekuCleared('crossed')) {
      save.clearDekuChallenge('crossed');toast('Water room crossed!');
    }
  }

  step(m:MarioRuntime,c:ControllerSnapshot,save:SaveFile,toast:(s:string)=>void,canInteract:boolean) {
    this.clock++;this.prompt=null;
    this.actorPos.fromArray(m.pos);
    if(save.dekuCleared('ladder') && this.ladderDrop<280) {
      this.ladderDrop=Math.min(280,this.ladderDrop+8);
      const p=this.part('ladder');
      this.move('ladder',p.home.clone().add(new THREE.Vector3(0,-this.ladderDrop*scale,0)));
    }
    if(save.hasSlingshot && this.part('steppingStone').model.visible) this.visible('steppingStone',false);
    const onGround=m.floor && Math.abs(m.floorHeight-m.pos[1])<25;
    const web=this.part('web');
    if(web.model.visible && web.collision!.owns(m.floor) && onGround && this.peak-m.pos[1]>260*scale) {
      save.clearDekuChallenge('web');this.visible('web',false);
      m.setAction(ACT_FREEFALL);m.vel[1]=-42;toast('The web broke!');
    }
    this.peak=onGround?m.pos[1]:Math.max(this.peak,m.pos[1]);
    this.scrub(m,c,save,toast);
    this.water(m,canInteract?c:{...c,buttonPressed:0},save,toast);
    if(this.climb){
      const move=this.climb,t=++move.elapsed/move.ticks;
      const p=move.from.clone().lerp(move.to,t);
      p.y+=4*t*(1-t)*move.arc;
      m.pos=p.toArray();m.vel=[0,0,0];m.forwardVel=0;m.animFrame++;
      if(t>=1){this.climb=undefined;m.setAction(ACT_FREEFALL);move.done?.();}
      return true;
    }
    if(!canInteract)return false;
    const supply=this.at(-1391,480,1391);
    if(save.hasSlingshot && save.dekuSeeds<30 && this.nearby(supply,250)){
      this.prompt='SPACE / A · REFILL DEKU SEEDS';
      if(c.buttonPressed&A_BUTTON){save.refillSeeds();toast('Deku Seeds · 30/30');return true;}
    }
    for(const route of this.routes) {
      if(route.prompt.includes('WATER ROOM') && !save.dekuCleared('eye'))continue;
      if(route.prompt.includes('FALLEN') && this.ladderDrop<280) continue;
      if(route.prompt.includes('PLATFORM') && save.hasSlingshot)continue;
      if(route.prompt.includes('WEB') && save.dekuCleared('web'))continue;
      const forward=this.nearby(route.from,route.prompt.includes('SWIM TO THE BANK')?425:195,100);
      if(!forward && (route.oneWay || !this.nearby(route.to,165,100)))continue;
      // The upper landing offers the web leap; descending the vines is available nearer their foot.
      if(!forward && route.prompt.includes('UPPER WALKWAY') && !save.dekuCleared('web'))continue;
      const dest=forward?route.to:route.from;
      this.prompt='SPACE / A · '+(forward?route.prompt:route.prompt.includes('DOOR') || route.prompt.includes('WATER ROOM')?'RETURN THROUGH THE DOOR':'CLIMB DOWN');
      if(c.buttonPressed&A_BUTTON) {
        this.climb={from:this.actorPos.clone(),to:dest,ticks:route.ticks,elapsed:0,arc:route.arc};
        m.setAction(route.arc?ACT_JUMP:ACT_CLIMBING_POLE);
        m.faceAngle[1]=Math.round(Math.atan2(dest.x-m.pos[0],dest.z-m.pos[2])*32768/Math.PI);
        return true;
      }
      break;
    }
    return false;
  }
}
