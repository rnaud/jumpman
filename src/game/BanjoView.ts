import * as THREE from 'three';
import type { CharacterView } from './CharacterView';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { ANGLE_TO_RAD } from '../sim/math';
import { BANJO_BARGE, BANJO_CLAW, BANJO_FLAP, BANJO_FLIP, BANJO_JUMP, BANJO_PECK, BANJO_ROLL, banjoMove } from '../sim/banjoController';
import {
  ACT_AIR_THROW, ACT_BACKFLIP, ACT_BEGIN_SLIDING, ACT_BRAKING, ACT_BUTT_SLIDE, ACT_BUTT_SLIDE_AIR,
  ACT_CLIMBING_POLE, ACT_CRAWLING, ACT_CROUCH_SLIDE, ACT_CROUCHING, ACT_DEATH_ON_BACK, ACT_DEATH_ON_STOMACH,
  ACT_DIVE, ACT_DIVE_SLIDE, ACT_DOUBLE_JUMP, ACT_DROWNING, ACT_FLAG_AIR, ACT_FLAG_HANGING, ACT_FLAG_ON_POLE,
  ACT_FLAG_SWIMMING, ACT_FLAG_INVULNERABLE, ACT_FLAG_MOVING, ACT_FLYING, ACT_GROUND_POUND, ACT_GROUND_POUND_LAND,
  ACT_HOLD_IDLE, ACT_HOLD_WALKING, ACT_HOLD_DECELERATING, ACT_HOLD_JUMP, ACT_JUMP, ACT_JUMP_KICK,
  ACT_LEDGE_GRAB, ACT_LONG_JUMP, ACT_MOVE_PUNCHING, ACT_PICKING_UP, ACT_PUNCHING, ACT_QUICKSAND_DEATH,
  ACT_SHOT_FROM_CANNON, ACT_SIDE_FLIP, ACT_SLIDE_KICK, ACT_SLIDE_KICK_SLIDE, ACT_STANDING_DEATH, ACT_STAR_DANCE_EXIT,
  ACT_STAR_DANCE_NO_EXIT, ACT_STAR_DANCE_WATER, ACT_START_CROUCHING, ACT_STEEP_JUMP, ACT_STOMACH_SLIDE,
  ACT_STOP_CROUCHING, ACT_THROWING, ACT_TOP_OF_POLE_JUMP, ACT_TRIPLE_JUMP, ACT_TURNING_AROUND, ACT_WALL_KICK_AIR,
  ACT_WATER_DEATH, ACT_WATER_IDLE, ACT_JUMP_LAND, ACT_FREEFALL_LAND, A_BUTTON, ACT_HOLD_JUMP_LAND, ACT_FALL_AFTER_STAR_GRAB,
} from '../sim/state';

interface BanjoClip {
  start: number;
  end: number;
  duration: number;
  /** [boneId, channel (0-2 rot° / 3-5 scale / 6-8 translation), [flags, frame, value×64]…] */
  elements: [number, number, number[]][];
}

interface BanjoAsset {
  transScale: number;
  bones: {pivot: [number, number, number]; id: number; parent: number}[];
  textures: Record<string, {width: number; height: number; rgba: string}>;
  batches: {part: string; texture: string | null; wrap: [number, number]; vertices: number[][]}[];
  clips: Record<string, BanjoClip>;
}

/** Banjo's walking stride and Mario's forward speed, in course units per frame. */
const WALK_SPEED = 12;
const RUN_SPEED = 26;
const BLEND = 0.12;

type Part = 'kazooieUpper' | 'kazooieFeet' | 'kazooieAss';

interface Choice {
  clip: string;
  loop: boolean;
  /** Seconds per playthrough when different from the bs state duration. */
  duration?: number;
  /** Start offset (0..1) the bs state seeds into its animation controller. */
  start?: number;
  /** Drive the animation directly from the move's own timer (0..1). */
  at?: number;
  parts?: Part[];
}

/** glspline_catmull_rom_interpolate with four knots. */
function catmull(x: number, k: number[]): number {
  x = Math.min(1, Math.max(0, x));
  const c2 = -0.5 * k[0] + 1.5 * k[1] - 1.5 * k[2] + 0.5 * k[3];
  const c1 = k[0] - 2.5 * k[1] + 2 * k[2] - 0.5 * k[3];
  const c0 = -0.5 * k[0] + 0.5 * k[2];
  return ((c2 * x + c1) * x + c0) * x + k[1];
}

const DEFAULTS = [0, 0, 0, 1, 1, 1, 0, 0, 0];

/** animationfilebin_func_8033AC38: one channel of a Rare animation file at a frame time. */
function sample(clip: BanjoClip, channel: number, keys: number[], time: number): number {
  const n = keys.length / 3;
  const flag15 = (i: number) => keys[3 * i] >> 1;
  const flag14 = (i: number) => keys[3 * i] & 1;
  const frame = (i: number) => keys[3 * i + 1];
  const value = (i: number) => keys[3 * i + 2] / 64;
  if (Math.trunc(time) < frame(0)) {
    const k2 = value(0);
    const k3 = flag15(0) === 1 && n >= 2 ? value(1) : k2;
    const def = DEFAULTS[channel];
    return catmull((time - clip.start) / (frame(0) - clip.start), [def, def, k2, k3]);
  }
  let last = n - 1;
  if (Math.trunc(time) >= frame(last)) {
    const k1 = value(last);
    const k0 = flag14(last) === 1 && n >= 2 ? value(last - 1) : k1;
    return catmull(time - frame(last), [k0, k1, k1, k1]);
  }
  let first = 0;
  while (first + 1 < last) {
    const mid = first + ((last - first) >> 1);
    if (frame(mid) <= Math.trunc(time)) first = mid; else last = mid;
  }
  const k1 = value(first), k2 = value(last);
  const t = (time - frame(first)) / (frame(last) - frame(first));
  if (flag14(first) === 0 && flag15(last) === 0) return k1 + (k2 - k1) * t;
  const k0 = flag14(first) === 1 && first > 0 ? value(first - 1) : k1;
  const k3 = flag15(last) === 1 && last + 1 < n ? value(last + 1) : k2;
  return catmull(t, [k0, k1, k2, k3]);
}

interface Pose {
  rot: THREE.Quaternion[];
  scale: THREE.Vector3[];
  trans: THREE.Vector3[];
}

function emptyPose(count: number): Pose {
  return {
    rot: Array.from({length: count}, () => new THREE.Quaternion()),
    scale: Array.from({length: count}, () => new THREE.Vector3(1, 1, 1)),
    trans: Array.from({length: count}, () => new THREE.Vector3()),
  };
}

const DEATHS = new Set([ACT_STANDING_DEATH, ACT_DEATH_ON_BACK, ACT_DEATH_ON_STOMACH, ACT_QUICKSAND_DEATH, ACT_DROWNING, ACT_WATER_DEATH]);
const DANCES = new Set([ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_NO_EXIT, ACT_STAR_DANCE_WATER]);
const JUMPS = new Set([ACT_JUMP, ACT_DOUBLE_JUMP, ACT_STEEP_JUMP, ACT_WALL_KICK_AIR, ACT_TOP_OF_POLE_JUMP, ACT_HOLD_JUMP, ACT_LONG_JUMP]);
const SLIDES = new Set([ACT_BUTT_SLIDE, ACT_BEGIN_SLIDING, ACT_BUTT_SLIDE_AIR, ACT_CROUCH_SLIDE]);
const BARGES = new Set([ACT_DIVE, ACT_DIVE_SLIDE, ACT_SLIDE_KICK, ACT_SLIDE_KICK_SLIDE, ACT_STOMACH_SLIDE, ACT_SHOT_FROM_CANNON, ACT_FLYING]);
const CROUCHES = new Set([ACT_CROUCHING, ACT_START_CROUCHING, ACT_STOP_CROUCHING]);


/** Banjo & Kazooie from the US Banjo-Kazooie ROM, animated by Rare's own keyframe files. */
export class BanjoView implements CharacterView {
  readonly root = new THREE.Group();
  readonly clipNames: string[];
  activeClip = '';
  private readonly bones: THREE.Bone[];
  private readonly skeleton: THREE.Skeleton;
  private readonly parts = new Map<string, THREE.Mesh[]>();
  private readonly channels = new Map<string, Map<number, (number[] | null)[]>>();
  private progress = 0;
  private blend = 1;
  private readonly pose: Pose;
  private readonly from: Pose;
  private readonly target: Pose;
  private readonly euler = new THREE.Euler();
  private readonly local = new THREE.Matrix4();
  private readonly tmp = new THREE.Matrix4();
  private readonly world: THREE.Matrix4[];
  private lastAction = -1;

  static async load() {
    const res = await fetch(import.meta.env.BASE_URL + 'models/bk/banjo.json');
    if (!res.ok) throw new Error('Could not load the Banjo-Kazooie asset');
    return new BanjoView(await res.json());
  }

  private constructor(private asset: BanjoAsset) {
    this.clipNames = Object.keys(asset.clips);
    for (const [name, clip] of Object.entries(asset.clips)) {
      const byBone = new Map<number, (number[] | null)[]>();
      for (const [bone, channel, keys] of clip.elements) {
        if (!byBone.has(bone)) byBone.set(bone, Array(9).fill(null));
        byBone.get(bone)![channel] = keys;
      }
      this.channels.set(name, byBone);
    }
    const count = asset.bones.length;
    this.pose = emptyPose(count);
    this.from = emptyPose(count);
    this.target = emptyPose(count);
    this.world = asset.bones.map(() => new THREE.Matrix4());
    this.bones = asset.bones.map(() => {
      const bone = new THREE.Bone();
      bone.matrixAutoUpdate = false;
      bone.matrixWorldAutoUpdate = false;
      return bone;
    });
    // Vertices are stored in model space; each anim matrix pivots about its bone.
    this.skeleton = new THREE.Skeleton(this.bones, this.bones.map(() => new THREE.Matrix4()));

    const textures = new Map<string, THREE.DataTexture>();
    const wrapping = [THREE.RepeatWrapping, THREE.MirroredRepeatWrapping, THREE.ClampToEdgeWrapping, THREE.MirroredRepeatWrapping] as const;
    const toLinear = (c: number) => Math.pow(c / 255, 2.2);
    for (const batch of asset.batches) {
      let map: THREE.DataTexture | null = null;
      if (batch.texture) {
        const key = [batch.texture, ...batch.wrap].join('/');
        map = textures.get(key) ?? null;
        if (!map) {
          const spec = asset.textures[batch.texture];
          const bytes = Uint8Array.from(atob(spec.rgba), c => c.charCodeAt(0));
          map = new THREE.DataTexture(bytes, spec.width, spec.height);
          map.wrapS = wrapping[batch.wrap[0]];
          map.wrapT = wrapping[batch.wrap[1]];
          map.colorSpace = THREE.SRGBColorSpace;
          map.magFilter = THREE.LinearFilter;
          map.needsUpdate = true;
          textures.set(key, map);
        }
      }
      const pos: number[] = [], col: number[] = [], uv: number[] = [], indices: number[] = [], weights: number[] = [];
      for (const [x, y, z, u, v, r, g, b, , bone] of batch.vertices) {
        pos.push(x, y, z);
        uv.push(u, v);
        col.push(toLinear(r), toLinear(g), toLinear(b));
        indices.push(Math.max(0, bone), 0, 0, 0);
        weights.push(1, 0, 0, 0);
      }
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3));
      geo.setAttribute('color', new THREE.Float32BufferAttribute(col, 3));
      geo.setAttribute('uv', new THREE.Float32BufferAttribute(uv, 2));
      geo.setAttribute('skinIndex', new THREE.Uint16BufferAttribute(indices, 4));
      geo.setAttribute('skinWeight', new THREE.Float32BufferAttribute(weights, 4));
      geo.computeVertexNormals();
      // Rare prelit every vertex, so keep its colours and only lift them with the scene lights.
      const mesh = new THREE.SkinnedMesh(geo, new THREE.MeshLambertMaterial({
        map, vertexColors: true, side: THREE.DoubleSide, alphaTest: 0.3,
        emissive: 0xffffff, emissiveIntensity: 0.0,
      }));
      mesh.bindMode = THREE.DetachedBindMode;
      mesh.bind(this.skeleton, new THREE.Matrix4());
      mesh.frustumCulled = false;
      mesh.castShadow = true;
      this.root.add(mesh);
      if (!this.parts.has(batch.part)) this.parts.set(batch.part, []);
      this.parts.get(batch.part)!.push(mesh);
    }
    // Banjo stands about 130 units tall in his own game; bring him just above Mario's height.
    this.root.scale.setScalar(1.25);
    this.evaluate('idle', 0, this.pose);
    this.apply();
  }

  setVisible(visible: boolean) { this.root.visible = visible; }

  /** Evaluate one clip at progress 0..1 into per-bone TRS. */
  private evaluate(name: string, progress: number, out: Pose) {
    const clip = this.asset.clips[name];
    const byBone = this.channels.get(name)!;
    const time = clip.start + Math.min(0.999999, Math.max(0, progress)) * (clip.end - clip.start);
    const values = [0, 0, 0, 1, 1, 1, 0, 0, 0];
    for (const [i, bone] of this.asset.bones.entries()) {
      const channels = byBone.get(bone.id);
      for (let c = 0; c < 9; c++) {
        const keys = channels?.[c];
        values[c] = keys && keys.length ? sample(clip, c, keys, time) : DEFAULTS[c];
      }
      this.euler.set(values[0] * Math.PI / 180, values[1] * Math.PI / 180, values[2] * Math.PI / 180, 'ZYX');
      out.rot[i].setFromEuler(this.euler);
      out.scale[i].set(values[3], values[4], values[5]);
      out.trans[i].set(values[6], values[7], values[8]);
    }
  }

  /** animMtxList_setBoned: parent · T(pivot + s·t) · R · S · T(-pivot). */
  private apply() {
    const s = this.asset.transScale;
    for (const [i, bone] of this.asset.bones.entries()) {
      const [px, py, pz] = bone.pivot;
      const t = this.pose.trans[i];
      this.local.makeTranslation(px + s * t.x, py + s * t.y, pz + s * t.z);
      this.tmp.makeRotationFromQuaternion(this.pose.rot[i]);
      this.local.multiply(this.tmp);
      this.tmp.makeScale(this.pose.scale[i].x, this.pose.scale[i].y, this.pose.scale[i].z);
      this.local.multiply(this.tmp);
      this.tmp.makeTranslation(-px, -py, -pz);
      this.local.multiply(this.tmp);
      if (bone.parent >= 0) this.world[i].multiplyMatrices(this.world[bone.parent], this.local);
      else this.world[i].copy(this.local);
      this.bones[i].matrixWorld.copy(this.world[i]);
    }
  }

  private choose(m: MarioRuntime): Choice {
    const a = m.action;
    const bk = this.chooseBanjoMove(m);
    if (bk) return bk;
    if (DANCES.has(a) || a === ACT_FALL_AFTER_STAR_GRAB) return {clip: 'jiggy', loop: false, parts: ['kazooieUpper']};
    if (DEATHS.has(a)) return {clip: 'die', loop: false};
    if (BARGES.has(a)) return {clip: 'barge', loop: false, duration: 0.7, parts: ['kazooieUpper']};
    if ((a & ACT_FLAG_INVULNERABLE) && !(a & ACT_FLAG_SWIMMING)) return {clip: 'ow', loop: false, duration: 1.0};
    if (a & ACT_FLAG_SWIMMING) {
      if (a === ACT_WATER_IDLE) return {clip: 'swimIdle', loop: true};
      return {clip: m.pos[1] < m.world.waterLevel(m.pos[0], m.pos[2]) - 160 ? 'dive' : 'swim', loop: true, parts: ['kazooieUpper']};
    }
    if (a === ACT_CLIMBING_POLE) return {clip: 'climb', loop: true};
    if ((a & ACT_FLAG_ON_POLE) || (a & ACT_FLAG_HANGING) || a === ACT_LEDGE_GRAB) return {clip: 'climbIdle', loop: true};
    if (a === ACT_GROUND_POUND || a === ACT_GROUND_POUND_LAND) return {clip: 'bust', loop: false, start: 0.15, parts: ['kazooieUpper']};
    if (a === ACT_BACKFLIP) return {clip: 'flipEnter', loop: false, start: 0.45, duration: 1.0, parts: ['kazooieUpper']};
    if (a === ACT_SIDE_FLIP || a === ACT_TRIPLE_JUMP) return {clip: 'tumble', loop: true};
    if (a === ACT_JUMP_KICK) return {clip: 'peck', loop: true, parts: ['kazooieUpper']};
    if (a === ACT_PUNCHING || a === ACT_MOVE_PUNCHING) return {clip: 'claw', loop: false, duration: 0.65};
    if (a === ACT_THROWING || a === ACT_AIR_THROW) return {clip: 'throw', loop: false};
    if (a & ACT_FLAG_AIR) {
      if (JUMPS.has(a) && m.vel[1] > 0) return {clip: 'jump', loop: false, start: 0.3};
      return {clip: m.vel[1] > 0 ? 'jump' : 'fall', loop: m.vel[1] <= 0, start: m.vel[1] > 0 ? 0.5 : 0};
    }
    if (a === ACT_JUMP_LAND || a === ACT_FREEFALL_LAND) return {clip: 'jump', loop: false, start: 0.6667, duration: 1.4};
    if (a === ACT_START_CROUCHING) return {clip: 'crouchEnter', loop: false};
    if (SLIDES.has(a)) return {clip: 'slideFront', loop: true};
    if (CROUCHES.has(a)) return {clip: 'crouch', loop: true};
    if (a === ACT_CRAWLING) return {clip: 'creep', loop: true, duration: 1.2};
    if (a === ACT_HOLD_IDLE || a === ACT_PICKING_UP || a === ACT_HOLD_JUMP_LAND) return {clip: 'carryIdle', loop: true};
    if (a === ACT_HOLD_WALKING || a === ACT_HOLD_DECELERATING) return {clip: 'carry', loop: true};
    if (a === ACT_TURNING_AROUND || a === ACT_BRAKING) return {clip: 'turn', loop: false};
    if ((a & ACT_FLAG_MOVING) && Math.abs(m.forwardVel) > 1) {
      const speed = Math.abs(m.forwardVel);
      if (speed < 5) return {clip: 'creep', loop: true, duration: 0.8 * 5 / Math.max(2, speed)};
      if (speed < 18) return {clip: 'walk', loop: true, duration: Math.min(1.5, Math.max(0.3, 0.43 * WALK_SPEED / speed))};
      return {clip: 'run', loop: true, duration: Math.max(0.4, 0.66 * RUN_SPEED / speed)};
    }
    return {clip: 'idle', loop: true};
  }

  /** Banjo's own moves, with the animation phases of their bs states. */
  private chooseBanjoMove(m: MarioRuntime): Choice | null {
    const t = m.actionTimer;
    const aHeld = (m.controller.buttonDown & A_BUTTON) !== 0;
    switch (banjoMove(m)) {
      case BANJO_JUMP:
        // bsjump: 0.3→0.5042 rising, on to 0.6667 falling, then the fall loop.
        if (m.vel[1] > 0) return {clip: 'jump', loop: false, at: Math.min(0.5042, 0.3 + t / 30 / 1.9)};
        if (t > 40) return {clip: 'fall', loop: true};
        return {clip: 'jump', loop: false, at: Math.min(0.6667, 0.5042 + (t - 15) / 30 / 4 * 0.6667)};
      case BANJO_FLAP:
        return t < 9 ? {clip: 'flapEnter', loop: false, parts: ['kazooieUpper']} : {clip: 'flap', loop: true, parts: ['kazooieUpper']};
      case BANJO_FLIP:
        if (t < 26) return {clip: 'flipEnter', loop: false, at: Math.min(0.999, t / 26), parts: ['kazooieUpper']};
        return aHeld ? {clip: 'flipHold', loop: true, parts: ['kazooieUpper']} : {clip: 'flipExit', loop: false, parts: ['kazooieUpper']};
      case BANJO_CLAW: return {clip: 'claw', loop: false, at: t / 39};
      case BANJO_ROLL: return {clip: 'roll', loop: false, at: t / 27};
      case BANJO_BARGE: return {clip: 'barge', loop: false, at: t / 30, parts: ['kazooieUpper']};
      case BANJO_PECK:
        return t < 15 ? {clip: 'peckEnter', loop: true, parts: ['kazooieUpper']} : {clip: 'peck', loop: false, parts: ['kazooieUpper']};
    }
    return null;
  }

  update(m: MarioRuntime, dt: number, pos: THREE.Vector3, yaw: number, smooth = true) {
    this.root.position.copy(pos);
    this.root.rotation.y = yaw * ANGLE_TO_RAD;
    const choice = this.choose(m);
    const clip = this.asset.clips[choice.clip];
    const restart = choice.clip !== this.activeClip || (!choice.loop && m.action !== this.lastAction);
    if (restart) {
      for (let i = 0; i < this.pose.rot.length; i++) {
        this.from.rot[i].copy(this.pose.rot[i]);
        this.from.scale[i].copy(this.pose.scale[i]);
        this.from.trans[i].copy(this.pose.trans[i]);
      }
      this.blend = this.activeClip && smooth ? 0 : 1;
      this.progress = choice.start ?? 0;
      this.activeClip = choice.clip;
    } else if (choice.at === undefined) {
      this.progress += dt / (choice.duration ?? clip.duration);
      this.progress = choice.loop ? this.progress % 1 : Math.min(this.progress, 0.999999);
    }
    if (choice.at !== undefined) this.progress = Math.min(0.999999, Math.max(0, choice.at));
    this.lastAction = m.action;
    this.blend = Math.min(1, this.blend + dt / BLEND);

    this.evaluate(choice.clip, this.progress, this.target);
    for (let i = 0; i < this.pose.rot.length; i++) {
      this.pose.rot[i].slerpQuaternions(this.from.rot[i], this.target.rot[i], this.blend);
      this.pose.scale[i].lerpVectors(this.from.scale[i], this.target.scale[i], this.blend);
      this.pose.trans[i].lerpVectors(this.from.trans[i], this.target.trans[i], this.blend);
    }
    this.apply();

    // Kazooie pokes out of the backpack only during the moves that use her.
    const shown = new Set(choice.parts ?? []);
    if (choice.clip === 'idle' && ((this.progress > 0.0909 && this.progress < 0.6818) || this.progress > 0.7727)) shown.add('kazooieUpper');
    for (const part of ['kazooieUpper', 'kazooieFeet', 'kazooieAss'] as Part[]) {
      for (const mesh of this.parts.get(part) ?? []) mesh.visible = shown.has(part);
    }
    this.root.scale.set(1.25, m.squishTimer > 0 ? 0.8 : 1.25, 1.25);
  }
}
