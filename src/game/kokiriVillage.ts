import * as THREE from 'three';
import type { CourseId } from './courses';
import { ACT_CLIMBING_POLE, ACT_IDLE } from '../sim/state';
import type { MarioRuntime, SpawnPoint } from '../sim/MarioRuntime';
import type { Vec3 } from '../sim/math';

interface FixedRoom { eye: Vec3; pitch: number; yaw: number; fov: number }
interface TalkSpot { kind: 'saria' | 'sign'; pos: Vec3; pages: string[] }
export interface KokiriScene {
  fixed: FixedRoom | null;
  exits: Record<number, { course: CourseId; entry: string }>;
  arrivals: Record<string, SpawnPoint>;
  talks: TalkSpot[];
  ladder?: [Vec3, Vec3];
  greeting?: string[];
  repeat?: string[];
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

export class KokiriVisit {
  private readonly box = document.getElementById('kokiri-dialogue')!;
  private readonly speaker = this.box.querySelector('strong')!;
  private readonly line = this.box.querySelector('p')!;
  private pages: string[] = [];
  private index = 0;
  private metSaria = false;
  private speaking?: TalkSpot;
  private climb?: { from: Vec3; to: Vec3; tick: number };

  private ladderEnd(scene: KokiriScene, pos: Vec3) {
    return scene.ladder?.findIndex(p => Math.abs(p[1]-pos[1]) < 80 && Math.hypot(p[0]-pos[0],p[2]-pos[2]) < 160) ?? -1;
  }

  clear() {
    this.climb = undefined;
    this.pages = [];
    this.box.hidden = true;
    this.speaking = undefined;
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

  frameConversation(camera: THREE.PerspectiveCamera, player: Vec3): boolean {
    if (this.speaking?.kind !== 'saria') return false;
    const [x,y,z] = this.speaking.pos;
    const angle = Math.atan2(player[0]-x,player[2]-z) + 1.08;
    camera.position.set(x+Math.sin(angle)*380,y+175,z+Math.cos(angle)*380);
    camera.lookAt(x,y+134,z);
    return true;
  }

  prompt(scene: KokiriScene | undefined, pos: Vec3) {
    if (!scene || this.pages.length || this.climb) return null;
    const end = this.ladderEnd(scene,pos);
    if (end >= 0) return end === 1 ? 'SPACE / A · CLIMB DOWN' : 'SPACE / A · CLIMB UP';
    const target = this.nearest(scene,pos);
    return target ? (target.kind === 'saria' ? 'SPACE / A · TALK TO SARIA' : 'SPACE / A · READ SIGN') : null;
  }

  step(scene: KokiriScene, player: MarioRuntime, pressed: boolean, name: string) {
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
    if (this.pages.length) {
      if (pressed && ++this.index >= this.pages.length) {
        if (this.speaking?.kind === 'saria') this.metSaria = true;
        this.clear();
      }
      else this.line.textContent = this.pages[this.index];
      return true;
    }
    if (!pressed) return false;
    const target = this.nearest(scene,pos);
    if (!target) return false;
    const saria = target.kind === 'saria';
    const pages = saria ? (this.metSaria ? scene.repeat! : [...scene.greeting!, ...target.pages]) : target.pages;
    this.pages = pages.map(page => page.replaceAll('{name}',saria ? name : 'Link').replace(/\s+/g,' ').trim());
    this.index = 0;
    this.speaking = target;
    player.forwardVel = 0;
    player.vel[0] = player.vel[2] = 0;
    player.faceAngle[1] = Math.round(Math.atan2(target.pos[0]-pos[0],target.pos[2]-pos[2])*32768/Math.PI);
    player.setAction(ACT_IDLE);
    this.speaker.textContent = saria ? 'Saria' : 'Sign';
    this.line.textContent = this.pages[0];
    this.box.hidden = false;
    return true;
  }
}
