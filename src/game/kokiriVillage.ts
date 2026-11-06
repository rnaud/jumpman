import * as THREE from 'three';
import { OotTextbox, type MessagePage } from './ootTextbox';
import type { SaveFile } from './save';
import type { CourseId } from './courses';
import { A_BUTTON, B_BUTTON, ACT_CLIMBING_POLE, ACT_IDLE } from '../sim/state';
import type { MarioRuntime, SpawnPoint } from '../sim/MarioRuntime';
import type { Vec3 } from '../sim/math';

interface FixedRoom { eye: Vec3; pitch: number; yaw: number; fov: number }
interface TalkSpot { kind: 'saria' | 'sign' | 'resident'; pos: Vec3; pages: MessagePage[]; id?: string; label?: string; repeat?: MessagePage[] }
type ShopItem = 'shield' | 'heart';
interface RupeePickup { key: string; pos: Vec3; value: number; permanent: boolean; hidden?: boolean }
export interface KokiriScene {
  rupees?: RupeePickup[];
  counter?: Record<ShopItem, Vec3>;
  fixed: FixedRoom | null;
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

  private showCurrent() {
    const page = this.pages[this.index];
    const target = this.speaking;
    const speaker = target?.label ?? (target?.kind === 'saria' ? 'Saria' : target?.kind === 'sign' ? 'Sign' : 'Kokiri Shop');
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
  private climb?: { from: Vec3; to: Vec3; tick: number };

  private ladderEnd(scene: KokiriScene, pos: Vec3) {
    return scene.ladder?.findIndex(p => Math.abs(p[1]-pos[1]) < 80 && Math.hypot(p[0]-pos[0],p[2]-pos[2]) < 160) ?? -1;
  }

  clear() {
    this.climb = undefined;
    this.pages = [];
    this.offer = undefined;
    this.textbox.hide();
    this.speaking = undefined;
    this.conversationFlag = undefined;
  }

  private nearest(scene: KokiriScene, pos: Vec3) {
    let nearest: TalkSpot | undefined;
    let distance = 240;
    for (const spot of scene.talks) {
      const d = Math.hypot(spot.pos[0]-pos[0],spot.pos[2]-pos[2]);
      if (d < distance && Math.abs(spot.pos[1]-pos[1]) < 155) {
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
    if (!this.speaking || this.speaking.kind === 'sign') return false;
    const [x,y,z] = this.speaking.pos;
    const angle = Math.atan2(player[0]-x,player[2]-z) + 1.08;
    camera.position.set(x+Math.sin(angle)*380,y+175,z+Math.cos(angle)*380);
    camera.lookAt(x,y+134,z);
    return true;
  }

  prompt(scene: KokiriScene | undefined, pos: Vec3) {
    if (!scene || this.offer || this.pages.length || this.climb) return null;
    const end = this.ladderEnd(scene,pos);
    if (end >= 0) return end === 1 ? 'SPACE / A · CLIMB DOWN' : 'SPACE / A · CLIMB UP';
    const shelf = this.shelf(scene,pos);
    if (shelf) return shelf === 'shield' ? 'SPACE / A · DEKU SHIELD · 40 RUPEES' : 'SPACE / A · RECOVERY HEART · 10 RUPEES';
    const target = this.nearest(scene,pos);
    return target ? (target.kind === 'sign' ? 'SPACE / A · READ SIGN' : 'SPACE / A · TALK') : null;
  }

  step(scene: KokiriScene, player: MarioRuntime, buttons: number, name: string, save: SaveFile, stickY = 0) {
    const pressed = !!(buttons & A_BUTTON);
    const pos = player.pos;
    // The village NPC has a solid cylinder. Keep the two rigs apart even
    // when the player releases the stick immediately beside Saria.
    for (const npc of scene.talks) {
      if (npc.kind !== 'saria' || Math.abs(pos[1]-npc.pos[1]) > 150) continue;
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
    if (this.climb) {
      const {from,to} = this.climb;
      const t = ++this.climb.tick / 48;
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
      if (branch) this.pages.splice(this.index+1,0,...branch);
      if (++this.index >= this.pages.length) {
        if (this.conversationFlag) save.finishConversation(this.conversationFlag);
        this.clear();
      } else this.showCurrent();
      return true;
    }
    if (!pressed) return false;
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
