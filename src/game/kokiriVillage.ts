import { treasureRewards, warpDestinations, type TreasureChest, type WarpPad } from './zeldaTreasure';
import * as THREE from 'three';
import { OotTextbox, type MessagePage } from './ootTextbox';
import type { SaveFile } from './save';
import type { CourseId } from './courses';
import { A_BUTTON, B_BUTTON, ACT_CLIMBING_POLE, ACT_CRAWLING, ACT_IDLE } from '../sim/state';
import type { MarioRuntime, SpawnPoint } from '../sim/MarioRuntime';
import type { Vec3 } from '../sim/math';

interface FixedRoom { eye: Vec3; pitch: number; yaw: number; fov: number }
interface TalkSpot { kind: 'saria' | 'sign' | 'resident'; pos: Vec3; pages: MessagePage[]; id?: string; label?: string; range?: number; height?: number; portrait?: {center: Vec3; distance: number; yaw: number}; repeat?: MessagePage[] }
type ShopItem = 'shield' | 'heart';
interface RupeePickup { key: string; pos: Vec3; value: number; permanent: boolean; hidden?: boolean }
export interface KokiriScene {
  rupees?: RupeePickup[];
  treasureChests?: TreasureChest[];
  warpPad?: WarpPad;
  counter?: Record<ShopItem, Vec3>;
  light: {ambient:Vec3; key:Vec3; keyDir:Vec3; fill:Vec3; fillDir:Vec3};
  quest?: {mouth:{closed:Vec3; opened:Vec3; meeting:Vec3; eye:Vec3; lookFrom:Vec3; lookTo:Vec3; first:MessagePage[]; retry:MessagePage[]}; chest:Vec3; crawl:[Vec3,Vec3]; path:Vec3[]; reward:MessagePage[]; approved:MessagePage[]; passed:MessagePage[]};
  fixed: FixedRoom | null;
  enclosed?: boolean;
  exits: Record<number, { course: CourseId; entry: string }>;
  arrivals: Record<string, SpawnPoint>;
  talks: TalkSpot[];
  ladder?: [Vec3, Vec3];
  afterMido?: [MessagePage[],MessagePage[]];
  greeting?: MessagePage[];
  repeat?: MessagePage[];
}

const backgrounds = import.meta.glob<string>('../../assets/levels/*/background.jpg', {eager:true, query:'?url', import:'default'});

export function frameKokiriRoom(camera: THREE.PerspectiveCamera, room: FixedRoom) {
  const yaw = room.yaw * Math.PI/32768, pitch = room.pitch * Math.PI/32768;
  camera.position.fromArray(room.eye);
  camera.lookAt(camera.position.x + Math.cos(pitch)*Math.sin(yaw), camera.position.y - Math.sin(pitch),
    camera.position.z + Math.cos(pitch)*Math.cos(yaw));
  camera.fov = room.fov;
  camera.updateProjectionMatrix();
  return yaw + Math.PI;
}

/** The ROM draws the room into the depth buffer and lays the original photograph over its color. */
export async function prepareKokiriBackground(id: CourseId, room: FixedRoom, terrain: THREE.Group) {
  const texture = await new THREE.TextureLoader().loadAsync(backgrounds[`../../assets/levels/${id}/background.jpg`]);
  texture.colorSpace = THREE.SRGBColorSpace;
  terrain.traverse(node => {
    if (!(node instanceof THREE.Mesh)) return;
    node.renderOrder = -1;
    node.castShadow = false;
    for (const mat of Array.isArray(node.material) ? node.material : [node.material]) mat.colorWrite = false;
  });
  const view = new THREE.PerspectiveCamera();
  frameKokiriRoom(view,room);
  const depth = 5000;
  const height = 2 * depth * Math.tan(room.fov * Math.PI/360);
  const plate = new THREE.Mesh(new THREE.PlaneGeometry(height*4/3,height),
    new THREE.MeshBasicMaterial({map:texture, fog:false, toneMapped:false, depthWrite:false}));
  plate.position.copy(view.position).addScaledVector(view.getWorldDirection(new THREE.Vector3()),depth);
  plate.quaternion.copy(view.quaternion);
  plate.renderOrder = -2;
  terrain.add(plate);
  return () => { texture.dispose(); plate.geometry.dispose(); plate.material.dispose(); };
}

/** Scene-local flags reset on entry. Permanent pickups and the wallet belong to the save slot. */
export class VillageRupees {
  private readonly taken = new Set<string>();
  constructor(private pickups: RupeePickup[], private models: Map<string,THREE.Group>) {}

  restore(save: SaveFile) {
    this.taken.clear();
    for (const pickup of this.pickups) {
      const visible = !save.hasVillagePickup(pickup.key);
      const model = this.models.get(pickup.key);
      if (model) model.visible = visible;
      if (!visible) this.taken.add(pickup.key);
    }
  }

  step(pos: Vec3, save: SaveFile): number {
    let gained = 0;
    for (const p of this.pickups) {
      if (this.taken.has(p.key)) continue;
      const radius = p.hidden ? 220 : 132;
      if (Math.hypot(pos[0]-p.pos[0],pos[2]-p.pos[2]) >= radius || Math.abs(pos[1]-p.pos[1]) >= 132) continue;
      const before = save.rupees;
      if (!save.collectVillageRupees(p.key,p.value,p.permanent)) continue;
      gained += save.rupees-before;
      this.taken.add(p.key);
      const model = this.models.get(p.key);
      if (model) model.visible = false;
    }
    return gained;
  }

  update(time: number) {
    for (const model of this.models.values()) model.rotation.y = time * 3;
  }
}

export class KokiriVisit {
  readonly textbox = new OotTextbox();
  private pages: MessagePage[] = [];
  private index = 0;
  private conversationFlag?: string;
  private visitor = 'Link';
  private speaking?: TalkSpot;
  private offer?: ShopItem;
  private approvingMido = false;
  private treeMeeting?: Vec3;
  private acceptingTree = false;

  private warpMenu?: { destinations: typeof warpDestinations; page:number };

  private treasureAt(scene: KokiriScene, pos: Vec3, save: SaveFile) {
    let nearest: TreasureChest | undefined;
    let distance=260;
    for (const chest of scene.treasureChests ?? []) {
      const d=Math.hypot(chest.pos[0]-pos[0],chest.pos[2]-pos[2]);
      if (d<distance && Math.abs(chest.pos[1]-pos[1])<180 && !save.chestOpened(chest.key)) {
        nearest=chest;
        distance=d;
      }
    }
    return nearest;
  }

  private onWarpPad(scene: KokiriScene,pos: Vec3) {
    const p=scene.warpPad?.pos;
    return p && Math.abs(pos[1]-p[1])<160 && Math.hypot(pos[0]-p[0],pos[2]-p[2])<235;
  }

  private showWarpMenu() {
    const {destinations,page}=this.warpMenu!;
    const labels=destinations.slice(page*2,page*2+2).map(d=>d.label);
    labels.push(page===0 ? 'More destinations' : 'Previous destinations');
    this.textbox.show('Warp pad',{text:'Travel to (B to cancel)\n\x1c'+labels.join('\n'),style:3,branches:[[],[],[]]},false,true);
  }

  private nearTree(scene: KokiriScene, pos: Vec3, save: SaveFile) {
    const meeting = scene.quest?.mouth.meeting;
    return meeting && save.dekuPathOpen && !save.hasSpokenTo('dekuTreeAccepted') &&
      Math.abs(pos[1]-meeting[1])<650 && Math.hypot(pos[0]-meeting[0],pos[2]-meeting[2]) < 2600;
  }

  private showCurrent() {
    const page = this.pages[this.index];
    const target = this.speaking;
    const speaker = this.treeMeeting ? 'Great Deku Tree' : target?.label ?? (target?.kind === 'saria' ? 'Saria' : target?.kind === 'sign' ? 'Sign' : target ? 'Kokiri Shop' : 'Kokiri Sword');
    this.textbox.show(speaker,{...page,text:page.text.replaceAll('{name}',target?.kind==='sign'?'Link':this.visitor)},
      this.index+1 < this.pages.length);
  }

  private shelf(scene: KokiriScene, pos: Vec3): ShopItem | undefined {
    if (!scene.counter) return;
    const npc = this.nearest(scene,pos);
    let best = npc ? Math.hypot(npc.pos[0]-pos[0],npc.pos[2]-pos[2]) : 245;
    let selected: ShopItem | undefined;
    for (const item of ['shield','heart'] as const) {
      const p = scene.counter[item];
      const distance = Math.hypot(p[0]-pos[0],p[2]-pos[2]);
      if (Math.abs(p[1]-pos[1]) < 140 && distance < best) {
        best = distance;
        selected = item;
      }
    }
    return selected;
  }
  private climb?: { from: Vec3; to: Vec3; tick: number; crawling?: boolean };
  private crawlEnd(scene: KokiriScene, pos: Vec3) {
    return scene.quest?.crawl.findIndex(p=>Math.abs(p[1]-pos[1])<85 && Math.hypot(p[0]-pos[0],p[2]-pos[2])<160) ?? -1;
  }

  private nearChest(scene: KokiriScene,pos: Vec3,save: SaveFile) {
    const chest = scene.quest?.chest;
    return chest && !save.kokiriSword && Math.abs(chest[1]-pos[1])<180 && Math.hypot(chest[0]-pos[0],chest[2]-pos[2])<260;
  }

  private ladderEnd(scene: KokiriScene, pos: Vec3) {
    return scene.ladder?.findIndex(p => Math.abs(p[1]-pos[1]) < 80 && Math.hypot(p[0]-pos[0],p[2]-pos[2]) < 160) ?? -1;
  }

  clear() {
    this.climb = undefined;
    this.warpMenu = undefined;
    this.pages = [];
    this.offer = undefined;
    this.approvingMido = false;
    this.acceptingTree = false;
    this.treeMeeting = undefined;
    this.textbox.hide();
    this.speaking = undefined;
    this.conversationFlag = undefined;
  }

  private nearest(scene: KokiriScene, pos: Vec3) {
    let nearest: TalkSpot | undefined;
    let distance = Infinity;
    for (const spot of scene.talks) {
      const d = Math.hypot(spot.pos[0]-pos[0],spot.pos[2]-pos[2]);
      if (d < distance && d < (spot.range ?? 240) && Math.abs(spot.pos[1]-pos[1]) < (spot.height ?? 155)) {
        nearest = spot;
        distance = d;
      }
    }
    return nearest;
  }

  get speakerId() { return this.speaking?.id ?? this.speaking?.kind; }
  get gesture() {
    if (this.speakerId === 'mido' && this.index >= 2) return this.index >= 5 ? 'Annoyed' : 'Curious';
    if (this.speakerId === 'saria' && this.index >= 3 && this.index <= 5) return 'Surprise';
    return 'Talk';
  }

  frameConversation(camera: THREE.PerspectiveCamera, player: Vec3): boolean {
    if (this.treeMeeting) {
      const [x,y,z] = this.treeMeeting;
      camera.position.set(x-2600,y+480,z+1495);
      camera.lookAt(x+500,y+1180,z-280);
      return true;
    }
    if (!this.speaking || this.speaking.kind === 'sign') return false;
    if (this.speaking.portrait) {
      const {center,distance,yaw} = this.speaking.portrait;
      camera.position.set(center[0]+distance*Math.sin(yaw),center[1]+distance*.12,center[2]+distance*Math.cos(yaw));
      camera.lookAt(...center);
      return true;
    }
    const [x,y,z] = this.speaking.pos;
    const angle = Math.atan2(player[0]-x,player[2]-z) + 1.08;
    camera.position.set(x+Math.sin(angle)*380,y+175,z+Math.cos(angle)*380);
    camera.lookAt(x,y+134,z);
    return true;
  }

  prompt(scene: KokiriScene | undefined, pos: Vec3, save: SaveFile) {
    if (!scene || this.offer || this.pages.length || this.climb || this.warpMenu) return null;
    if (this.treasureAt(scene,pos,save)) return 'SPACE / A · OPEN CHEST';
    if (this.onWarpPad(scene,pos)) return 'SPACE / A · WARP';
    if (this.nearTree(scene,pos,save)) return 'SPACE / A · SPEAK TO THE GREAT DEKU TREE';
    if (this.nearChest(scene,pos,save)) return 'SPACE / A · OPEN CHEST';
    if (this.crawlEnd(scene,pos)>=0) return 'SPACE / A · ENTER CRAWLSPACE';
    const end = this.ladderEnd(scene,pos);
    if (end >= 0) return end === 1 ? 'SPACE / A · CLIMB DOWN' : 'SPACE / A · CLIMB UP';
    const shelf = this.shelf(scene,pos);
    if (shelf) return shelf === 'shield' ? 'SPACE / A · DEKU SHIELD · 40 RUPEES' : 'SPACE / A · RECOVERY HEART · 10 RUPEES';
    const target = this.nearest(scene,pos);
    return target ? (target.kind === 'sign' ? 'SPACE / A · READ SIGN' : 'SPACE / A · TALK') : null;
  }

  step(scene: KokiriScene, player: MarioRuntime, buttons: number, name: string, save: SaveFile, stickY = 0,
       openLid?: (key:string)=>void, travel?: (destination:CourseId)=>void) {
    const pressed = !!(buttons & A_BUTTON);
    const pos = player.pos;
    if (this.warpMenu) {
      this.textbox.choose(stickY);
      if (buttons & B_BUTTON) this.clear();
      else if (pressed) {
        if (this.textbox.choice===2) {
          this.warpMenu.page=1-this.warpMenu.page;
          this.showWarpMenu();
        } else {
          const dest=this.warpMenu.destinations[this.warpMenu.page*2+this.textbox.choice];
          this.clear();
          travel?.(dest.id);
        }
      }
      return true;
    }
    if (pressed && !this.pages.length && !this.offer && !this.climb) {
      const chest=this.treasureAt(scene,pos,save);
      if (chest || this.onWarpPad(scene,pos)) {
        player.setAction(ACT_IDLE);
        player.forwardVel=0;
        player.vel=[0,0,0];
        if (chest && save.openTreasureChest(chest.key,chest.item)) {
          const reward=treasureRewards[chest.item];
          if (reward.heal) player.health=Math.min(0x880,player.health+0x100);
          openLid?.(chest.key);
          const text=reward.rupees ? `You found a ${reward.name}!\nWallet: ${save.rupees}/99 rupees.` :
            reward.heal ? 'You found a Recovery Heart!' : `You found the ${reward.name}!`;
          this.pages=[{text,style:3}];
          this.index=0;
          this.textbox.show('Treasure chest',this.pages[0],false);
        } else if (!chest && scene.warpPad) {
          this.warpMenu={destinations:warpDestinations.filter(d=>d.id!==scene.warpPad!.course),page:0};
          this.showWarpMenu();
        }
        return true;
      }
    }
    // The village NPC has a solid cylinder. Keep the two rigs apart even
    // when the player releases the stick immediately beside Saria.
    for (const npc of scene.talks) {
      if ((npc.kind !== 'saria' && npc.id !== 'mido') || Math.abs(pos[1]-npc.pos[1]) > 150) continue;
      const dx = pos[0]-npc.pos[0], dz = pos[2]-npc.pos[2];
      if (Math.hypot(dx,dz) < 100) {
        const angle = Math.atan2(dx,dz);
        pos[0] = npc.pos[0] + 100*Math.sin(angle);
        pos[2] = npc.pos[2] + 100*Math.cos(angle);
      }
    }
    const end = this.ladderEnd(scene,pos);
    if (pressed && end >= 0 && !this.pages.length && !this.climb) {
      this.climb = {from:[...pos], to:scene.ladder![1-end], tick:0};
      player.setAction(ACT_CLIMBING_POLE);
      player.vel = [0,0,0];
      player.forwardVel = 0;
      player.faceAngle[1] = 0;
    }
    const crawlEnd = this.crawlEnd(scene,pos);
    if (pressed && crawlEnd>=0 && !this.pages.length && !this.climb) {
      this.climb={from:[...pos],to:scene.quest!.crawl[1-crawlEnd],tick:0,crawling:true};
      player.setAction(ACT_CRAWLING);
      player.forwardVel=0;
      player.vel=[0,0,0];
      player.faceAngle[1]=crawlEnd===0 ? 0 : -32768;
    }
    if (this.climb) {
      const {from,to} = this.climb;
      const t = ++this.climb.tick / (this.climb.crawling ? 100 : 48);
      if (this.climb.crawling) player.intendedMag=16;
      player.animFrame++;
      for (let i=0; i<3; i++) pos[i] = from[i]+(to[i]-from[i])*t;
      if (t >= 1) {
        this.climb = undefined;
        player.setAction(ACT_IDLE);
      }
      return true;
    }
    if (this.offer) {
      if (buttons & B_BUTTON) this.clear();
      else if (pressed) {
        const item = this.offer;
        this.offer = undefined;
        const cost = item === 'shield' ? 40 : 10;
        let message: string;
        if (item === 'shield' && save.dekuShield) message = 'You already own a Deku Shield.';
        else if (item === 'heart' && player.health >= 0x880) message = 'Your health is full.';
        else if (!save.buyVillageItem(item)) message = `You need ${cost} rupees. You have ${save.rupees}.`;
        else if (item === 'shield') message = 'You got a Deku Shield!';
        else { player.health = Math.min(0x880,player.health+0x100); message = 'Recovered one health wedge.'; }
        this.pages = [{text:message,style:3}];
        this.index = 0;
        this.textbox.show('Kokiri Shop',this.pages[0],false,true);
      }
      return true;
    }
    if (this.pages.length) {
      this.textbox.choose(stickY);
      if (!pressed || this.textbox.finishReveal()) return true;
      const branch=this.pages[this.index].branches?.[this.textbox.choice];
      if (branch) {
        this.pages.splice(this.index+1,0,...branch);
        if (this.treeMeeting) this.acceptingTree = this.textbox.choice === 0;
      }
      if (++this.index >= this.pages.length) {
        if (this.acceptingTree) save.finishConversation('dekuTreeAccepted');
        if (this.approvingMido) save.openDekuPath();
        if (this.conversationFlag) save.finishConversation(this.conversationFlag);
        this.clear();
      } else this.showCurrent();
      return true;
    }
    const metTree = save.hasSpokenTo('dekuTreeMeeting');
    if (scene.quest && this.nearTree(scene,pos,save) && (pressed || !metTree)) {
      const mouth = scene.quest.mouth;
      this.treeMeeting = mouth.meeting;
      this.conversationFlag = 'dekuTreeMeeting';
      this.pages = [...(metTree ? mouth.retry : mouth.first)];
      this.index = 0;
      this.visitor = name;
      player.setAction(ACT_IDLE);
      player.forwardVel = 0;
      player.vel = [0,0,0];
      player.faceAngle[1] = Math.round(Math.atan2(mouth.closed[0]-pos[0],mouth.closed[2]-pos[2])*32768/Math.PI);
      this.showCurrent();
      return true;
    }
    if (!pressed) return false;
    if (scene.quest && this.nearChest(scene,pos,save) && save.collectKokiriSword()) {
      this.pages = scene.quest.reward;
      this.index = 0;
      player.forwardVel=0;
      player.vel=[0,0,0];
      player.setAction(ACT_IDLE);
      this.textbox.show('Kokiri Sword',this.pages[0],this.pages.length>1);
      return true;
    }
    const shelf = this.shelf(scene,pos);
    if (shelf) {
      this.offer = shelf;
      player.setAction(ACT_IDLE);
      player.forwardVel = player.vel[0] = player.vel[2] = 0;
      this.textbox.show('Kokiri Shop',{text:`${shelf==='shield'?'Deku Shield · 40':'Recovery Heart · 10'} rupees.
Your wallet holds ${save.rupees}.
A: Buy      B: Cancel`.replaceAll('·','-'),style:3},false,true);
      return true;
    }
    const target = this.nearest(scene,pos);
    if (!target) return false;
    const saria = target.kind === 'saria';
    this.visitor = name;
    const afterMido = saria && save.hasSpokenTo('mido') && scene.afterMido;
    this.conversationFlag = target.id ?? (saria ? (afterMido ? 'sariaAfterMido' : 'saria') : undefined);
    const met=!!this.conversationFlag && save.hasSpokenTo(this.conversationFlag);
    this.pages = [...(afterMido ? afterMido[met?1:0] : met && target.repeat ? target.repeat : saria ?
      (met ? scene.repeat! : [...scene.greeting!, ...target.pages]) : target.pages)];
    if (target.id==='mido' && scene.quest) {
      this.approvingMido = !save.dekuPathOpen && save.kokiriSword && save.dekuShield;
      if (this.approvingMido) this.pages = [...scene.quest.approved];
      else if (save.dekuPathOpen) this.pages = [...scene.quest.passed];
    }
    this.index = 0;
    this.speaking = target;
    player.forwardVel = 0;
    player.vel[0] = player.vel[2] = 0;
    player.faceAngle[1] = Math.round(Math.atan2(target.pos[0]-pos[0],target.pos[2]-pos[2])*32768/Math.PI);
    player.setAction(ACT_IDLE);
    this.showCurrent();
    return true;
  }
}
