import * as THREE from 'three';
import type { CharacterView } from './CharacterView';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { ANGLE_TO_RAD } from '../sim/math';
import { LINK_BACKFLIP, LINK_HOP_LEFT, LINK_HOP_RIGHT, linkFinishing, linkGuarding, linkRolling } from '../sim/linkController';
import { ACT_CLIMBING_POLE, ACT_CRAWLING, ACT_FREEFALL, ACT_FLAG_AIR, ACT_FLAG_SWIMMING, ACT_LEDGE_GRAB, ACT_PUNCHING } from '../sim/state';

interface LinkAsset {
  limbs: {pos: [number, number, number]; child: number; sibling: number}[];
  textures: Record<string, {width: number; height: number; rgba: string}>;
  batches: {mode: string; texture: string; color: number; wrap: [number, number]; vertices: number[][]}[];
  clips: Record<string, number[][]>;
}

export class OotLinkView implements CharacterView {
  readonly root = new THREE.Group();
  readonly clipNames: string[];
  activeClip = '';
  private phase = 0;
  private readonly skeleton: THREE.Skeleton;
  private readonly equipment = new Map<string, THREE.Mesh[]>();

  static async load() {
    const res = await fetch(import.meta.env.BASE_URL + 'models/oot/link.json');
    if (!res.ok) throw new Error('Could not load the OoT Link asset');
    return new OotLinkView(await res.json());
  }

  private constructor(private asset: LinkAsset) {
    this.clipNames = Object.keys(asset.clips);
    const bones = asset.limbs.map(({pos}) => {
      const bone = new THREE.Bone();
      bone.position.set(...pos);
      return bone;
    });
    for (const [i, limb] of asset.limbs.entries()) {
      for (let j = limb.child; j !== 255; j = asset.limbs[j].sibling) bones[i].add(bones[j]);
    }
    // Each N64 vertex is already in the local space of its matrix slot.
    this.skeleton = new THREE.Skeleton(bones, bones.map(() => new THREE.Matrix4()));
    const textures = new Map<string, THREE.DataTexture>();
    const wrapping = [THREE.RepeatWrapping, THREE.MirroredRepeatWrapping, THREE.ClampToEdgeWrapping, THREE.MirroredRepeatWrapping] as const;
    for (const batch of asset.batches) {
      const spec = asset.textures[batch.texture];
      const key = [batch.texture, ...batch.wrap].join('/');
      let map = textures.get(key);
      if (!map) {
        const bytes = Uint8Array.from(atob(spec.rgba), c => c.charCodeAt(0));
        map = new THREE.DataTexture(bytes, spec.width, spec.height);
        map.wrapS = wrapping[batch.wrap[0]];
        map.wrapT = wrapping[batch.wrap[1]];
        map.colorSpace = THREE.SRGBColorSpace;
        map.needsUpdate = true;
        textures.set(key, map);
      }
      const pos = [], norm = [], uv = [], indices = [], weights = [];
      for (const [x,y,z,s,t,nx,ny,nz,bone] of batch.vertices) {
        pos.push(x,y,z);
        norm.push(nx/127,ny/127,nz/127);
        uv.push((s/32 + 0.5)/spec.width,(t/32 + 0.5)/spec.height);
        indices.push(bone,0,0,0);
        weights.push(1,0,0,0);
      }
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.Float32BufferAttribute(pos,3));
      geo.setAttribute('normal', new THREE.Float32BufferAttribute(norm,3));
      geo.setAttribute('uv', new THREE.Float32BufferAttribute(uv,2));
      geo.setAttribute('skinIndex', new THREE.Uint16BufferAttribute(indices,4));
      geo.setAttribute('skinWeight', new THREE.Float32BufferAttribute(weights,4));
      const mesh = new THREE.SkinnedMesh(geo, new THREE.MeshLambertMaterial({
        map, color: batch.color, side: THREE.DoubleSide, alphaTest: 0.3,
      }));
      mesh.bindMode = THREE.DetachedBindMode;
      mesh.bind(this.skeleton, new THREE.Matrix4());
      mesh.frustumCulled = false;
      this.root.add(mesh);
      if (!this.equipment.has(batch.mode)) this.equipment.set(batch.mode, []);
      this.equipment.get(batch.mode)!.push(mesh);
    }
    this.root.scale.setScalar(0.044);
  }

  setVisible(visible: boolean) { this.root.visible = visible; }

  update(m: MarioRuntime, dt: number, pos: THREE.Vector3, yaw: number, smooth = true) {
    this.root.position.copy(pos);
    this.root.rotation.y = yaw * ANGLE_TO_RAD;
    let clip = 'idle';
    let speed = 30;
    let frame: number | undefined;
    if (linkRolling(m)) { clip = 'roll'; frame = m.actionTimer * 1.25; }
    else if (m.action === ACT_PUNCHING) {
      const finishing = linkFinishing(m);
      const strike = finishing ? 'finish' : 'slash';
      const windDown = this.asset.clips[strike].length;
      clip = m.actionTimer < windDown ? strike : finishing ? 'finishRecover' : 'recover';
      frame = m.actionTimer < windDown ? m.actionTimer : m.actionTimer - windDown;
    } else if (linkGuarding(m)) clip = 'guard';
    else if (m.action === ACT_CRAWLING) {
      clip = 'crawl';
      frame = Math.min(m.animFrame, this.asset.clips.crawl.length - 1);
    }
    else if (m.action === ACT_CLIMBING_POLE) clip = 'climb';
    else if (m.action === ACT_LEDGE_GRAB) clip = 'hang';
    else if (m.action & ACT_FLAG_SWIMMING) clip = 'swim';
    else if (m.action === ACT_FREEFALL && [LINK_BACKFLIP, LINK_HOP_LEFT, LINK_HOP_RIGHT].includes(m.actionArg)) {
      clip = m.actionArg === LINK_BACKFLIP ? 'backflip' : m.actionArg === LINK_HOP_LEFT ? 'leftHop' : 'rightHop';
      frame = Math.min(this.asset.clips[clip].length - 1, m.actionTimer);
    } else if (m.action & ACT_FLAG_AIR) clip = m.vel[1] > 0 ? 'jump' : 'fall';
    else if (Math.abs(m.forwardVel) > 1) {
      clip = Math.abs(m.forwardVel) > 16 ? 'run' : 'walk';
      if (m.controller.focusYaw !== undefined) clip = 'strafe';
      speed *= Math.max(0.35, Math.abs(m.forwardVel) / (clip === 'run' ? 32 : 16));
    }
    if (clip === 'idle' && m.controller.focusYaw !== undefined) clip = 'focus';
    const frames = this.asset.clips[clip];
    this.phase = frame ?? (clip === this.activeClip ? (this.phase + dt * speed) % frames.length : 0);
    this.activeClip = clip;
    const index = Math.min(this.phase, frames.length - 1);
    const a = frames[Math.floor(index)], b = frames[(Math.floor(index)+1) % frames.length];
    const t = smooth ? index % 1 : 0;
    const bones = this.skeleton.bones;
    bones[0].position.set(a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t, a[2]+(b[2]-a[2])*t);
    if (clip === 'crawl') bones[0].position.x = bones[0].position.z = 0;
    for (let j = 0; j < bones.length; j++) {
      const angle = (axis: number) => {
        const i = 3*j + 3 + axis;
        const delta = ((b[i]-a[i]+0x18000) % 0x10000)-0x8000;
        return (a[i]+delta*t)*ANGLE_TO_RAD;
      };
      bones[j].rotation.set(angle(0), angle(1), angle(2), 'ZYX');
    }
    bones[0].updateMatrixWorld(true);
    for (const mode of ['sheathed','drawn']) {
      for (const mesh of this.equipment.get(mode) ?? []) mesh.visible = (mode === 'drawn') === (m.action === ACT_PUNCHING || linkGuarding(m) || m.controller.focusYaw !== undefined);
    }
  }
}
