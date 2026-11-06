import { ACT_READING_NPC_DIALOG, ACT_TELEPORT_FADE_IN, ACT_TELEPORT_FADE_OUT, ACT_RIDING_SHELL_GROUND } from '../sim/state';
import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import type { CharacterView } from './CharacterView';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { angleToRadians } from '../sim/math';
import { sm64VisualRotation } from './sm64Animation';
import { Sm64AnimationPlayer, type Sm64AnimationMetadata } from './Sm64AnimationPlayer';
import { ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_WATER, ACT_END_PEACH_CUTSCENE, ACT_FLAG_SWIMMING_OR_FLYING, ACT_JUMBO_STAR_CUTSCENE, MARIO_CAP_IN_HAND, ACT_IDLE, MARIO_METAL_CAP, MARIO_VANISH_CAP, MARIO_WING_CAP } from '../sim/state';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';

/** Rigid-part SM64 clips already contain pose/flip/root offsets in source units. */
export class Sm64MarioView implements CharacterView {
  readonly root = new THREE.Group();
  readonly clipNames: string[];
  private readonly player: Sm64AnimationPlayer;
  private readonly headRotations: THREE.Object3D[] = [];
  private readonly eyeSwitches: THREE.Object3D[] = [];
  private readonly capSwitches: THREE.Object3D[] = [];
  private readonly handSwitches: THREE.Object3D[] = [];
  private readonly wingNodes: THREE.Object3D[] = [];
  private readonly capMaterials = new Map<THREE.MeshStandardMaterial, { metalness: number; roughness: number; color: number; opacity: number; transparent: boolean }>();
  private faceTime = 0;
  private readonly billboards: Sm64Billboards;

  static async load(): Promise<Sm64MarioView> {
    const variants = 'actors/mario/variants/mario_geo_';
    const [mario, ...parts] = await Promise.all([
      SM64_MODELS.mario,
      variants + 'face_and_wings/geo_switch_mario_cap_on_off_1.glb',
      variants + 'left_hand/geo_switch_mario_hand_1.glb',
      ...[1,2,3,4].map(i => variants + 'right_hand/geo_switch_mario_hand_' + i + '.glb'),
    ].map(loadSm64Asset));
    // Layout fragments can have several top-level commands. The export's default
    // scene contains only the first, so resolve each switch node explicitly.
    const switches = await Promise.all(parts.map((part, i) => {
      const name = i === 0 ? 'geo_switch_mario_cap_on_off' : 'geo_switch_mario_hand';
      const index = part.parser.json.nodes.findIndex((node: {extras?: {sm64Args?: string[]}}) => node.extras?.sm64Args?.[1] === name);
      return part.parser.getDependency('node', index) as Promise<THREE.Object3D>;
    }));
    return new Sm64MarioView(mario, switches);
  }

  private constructor(gltf: GLTF, switches: THREE.Object3D[]) {
    this.root.name = 'sm64-mario';
    const model = instantiateSm64(gltf);
    this.root.add(model);
    // The main GLB carries the animated nodes, while alternative display lists live in separate GLBs.
    switches.forEach((variant, i) => {
      const callback = i === 0 ? 'geo_switch_mario_cap_on_off' : 'geo_switch_mario_hand';
      const side = i === 1 ? '1' : '0';
      const index = i <= 2 ? 1 : i - 1;
      const selected = variant.children[index];
      model.traverse(node => {
        if (String(node.userData.sm64Args?.[0]) !== side || node.userData.sm64Args?.[1] !== callback) return;
        const slot = node.children[index];
        slot.scale.setScalar(1);
        slot.clear();
        slot.add(selected.clone(true));
      });
    });
    model.traverse(object => {
      if (object instanceof THREE.Mesh) {
        object.receiveShadow = false;
        const materials = Array.isArray(object.material) ? object.material : [object.material];
        for (const material of materials) {
          if (material instanceof THREE.MeshStandardMaterial && !this.capMaterials.has(material)) {
            this.capMaterials.set(material, {
              metalness: material.metalness,
              roughness: material.roughness,
              color: material.color.getHex(),
              opacity: material.opacity,
              transparent: material.transparent,
            });
          }
        }
      }
      if (object.userData.sm64Args?.includes('geo_mario_head_rotation') && object.parent) {
        const next = object.parent.children[object.parent.children.indexOf(object) + 1];
        if (next?.userData.sm64Command === 'GEO_ROTATION_NODE') this.headRotations.push(next);
      }
      // The native eye callback selects a single head display list (case zero = open).
      if (object.userData.sm64Command === 'GEO_SWITCH_CASE'
          && object.userData.sm64Args?.includes('geo_switch_mario_eyes')) {
        this.eyeSwitches.push(object);
        object.children.forEach((child, index) => { child.visible = index === 0; });
      }
      if (object.userData.sm64Command === 'GEO_SWITCH_CASE') {
        const callback = object.userData.sm64Args?.[1];
        if (callback === 'geo_switch_mario_cap_on_off') this.capSwitches.push(object);
        if (callback === 'geo_switch_mario_hand') this.handSwitches.push(object);
      }
      if (object.userData.sm64Command === 'GEO_DISPLAY_LIST' && object.userData.sm64Args?.[1] === 'mario_cap_wings') {
        this.wingNodes.push(object);
        object.visible = false;
      }
    });
    this.billboards = new Sm64Billboards(model);
    this.clipNames = gltf.animations.map(clip => clip.name);
    const metadata = new Map<string, Sm64AnimationMetadata>();
    for (const animation of gltf.parser.json.animations ?? []) metadata.set(animation.name, animation.extras ?? {});
    this.player = new Sm64AnimationPlayer(model.children[0], gltf.animations, metadata);
    this.player.update({ action: ACT_IDLE, actionArg: 0, actionState: 0, forwardVel: 0, vel: [0, 0, 0] }, 0);
  }

  setVisible(visible: boolean): void { this.root.visible = visible; }
  get activeClip(): string { return this.player.debug.name; }
  get animationDebug() { return this.player.debug; }

  update(m: MarioRuntime, dt: number, pos: THREE.Vector3, yaw: number, smooth = true): void {
    this.root.position.copy(pos);
    if (m.ridingShell) this.root.position.y += m.action === ACT_RIDING_SHELL_GROUND ? 45 : 42;
    const rotation = sm64VisualRotation(m);
    // Apply local pitch after world yaw, including SM64's graphics-only half-turn.
    this.root.rotation.set(rotation.pitch, angleToRadians(yaw) + rotation.yaw, rotation.roll ?? 0, 'YXZ');
    if (m.action === ACT_RIDING_SHELL_GROUND) this.root.rotation.z = angleToRadians(m.shellRoll);
    this.player.update(m, dt, smooth);
    for (const head of this.headRotations) head.rotation.z = m.action === ACT_READING_NPC_DIALOG ? angleToRadians(m.actionTimer) : 0;
    this.faceTime = (this.faceTime + dt) % 3.2;
    const blinkFrame = Math.floor(this.faceTime * 30);
    const eyeCase = blinkFrame === 92 || blinkFrame === 94 ? 1 : blinkFrame === 93 ? 2 : 0;
    for (const node of this.eyeSwitches) {
      const selected = node.children[eyeCase] ? eyeCase : 0;
      node.children.forEach((child, index) => { child.visible = index === selected; });
    }
    this.root.scale.set(m.squishTimer > 0 ? 1.2 : 1, m.squishTimer > 0 ? 0.65 : 1, m.squishTimer > 0 ? 1.2 : 1);
    const capInHand = Boolean(m.flags & MARIO_CAP_IN_HAND);
    for (const node of this.capSwitches) node.children.forEach((child,i) => { child.visible = i === Number(capInHand); });
    for (const node of this.handSwitches) {
      const right = node.userData.sm64Args?.[0] === '0';
      let hand = Number(Boolean(m.action & ACT_FLAG_SWIMMING_OR_FLYING));
      if (right && m.action === ACT_JUMBO_STAR_CUTSCENE && m.actionArg === 2) hand = 1;
      if (right && m.action === ACT_END_PEACH_CUTSCENE && m.actionArg === 9 && m.actionTimer >= 98) hand = 2;
      if (right && (m.action === ACT_STAR_DANCE_EXIT && m.actionTimer >= 40
        || m.action === ACT_STAR_DANCE_WATER && m.actionTimer >= 62)) hand = 2;
      if (right && capInHand) hand = m.flags & MARIO_WING_CAP ? 4 : 3;
      node.children.forEach((child,i) => { child.visible = i === hand; });
    }
    for (const wing of this.wingNodes) wing.visible = (m.flags & MARIO_WING_CAP) !== 0;
    const metal = (m.flags & MARIO_METAL_CAP) !== 0;
    const vanish = (m.flags & MARIO_VANISH_CAP) !== 0;
    const fade = m.action === ACT_TELEPORT_FADE_OUT ? Math.max(0,248-8*m.actionTimer)/255
      : m.action === ACT_TELEPORT_FADE_IN ? Math.min(1,m.actionTimer*8/255) : 1;
    for (const [material, base] of this.capMaterials) {
      material.metalness = metal ? 0.95 : base.metalness;
      material.roughness = metal ? 0.2 : base.roughness;
      material.color.setHex(metal ? 0xc7d2d9 : base.color);
      material.opacity = fade*(vanish ? 0.32 : base.opacity);
      material.transparent = fade<1 || vanish || base.transparent;
      material.depthWrite = !vanish && fade===1;
    }
  }

  faceCamera(camera: THREE.Camera): void { this.billboards.update(camera); }
}
