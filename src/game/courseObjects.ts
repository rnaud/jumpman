/**
 * Three.js presentation and lifecycle for interactive course objects.
 * Simulation lives in src/sim/objectManager.ts; this file owns models,
 * animations, dynamic coins and the restart/reset path.
 */

import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { angleToRadians, degToAngle, type Vec3 } from '../sim/math';
import type { CollisionWorld } from '../sim/collision';
import type { MarioRuntime } from '../sim/MarioRuntime';
import {
  ObjectManager,
  type ObjectEvent,
} from '../sim/objectManager';
import { INTERACT_BREAKABLE, type ObjectBehaviorKind, type SimObject } from '../sim/object';
import { instantiateSm64, type Sm64Billboards } from './sm64Assets';
import type { EditorActorTarget } from './editor';
import { DoorCollider } from './roomDoors';
import type { CollisionData } from './nativeCollision';
import type { ActorShadows } from './actorShadows';
import type { Coin } from './world';

export interface CourseObjectData {
  model: string;
  pos: Vec3;
  yaw: number;
  param?: number;
  behavior?: ObjectBehaviorKind;
  acts?: number[];
  warpTo?: string;
  warpArea?: number;
  warpEntry?: string;
}

interface View {
  collider?: DoorCollider;
  solid?: boolean;
  colliderY?: number;
  group: THREE.Group;
  baseScale: number;
  mixer: THREE.AnimationMixer | null;
  animation?: number;
  lid?: THREE.Group;
  outline?: THREE.Group;
}

export interface CourseObjectsOptions {
  root: THREE.Group;
  models: Map<string, GLTF>;
  shadows?: ActorShadows;
  woodenPostCollision?: CollisionData;
  chainGateCollision?: CollisionData;
  starBoxCollision?: CollisionData;
  blastWalls?: CollisionData[];
  collision: CollisionWorld;
  objects: CourseObjectData[];
  coins: Coin[];
  makeCoin: () => THREE.Group;
  billboards?: Sm64Billboards;
}

export class CourseObjects {
  readonly manager: ObjectManager;
  private readonly root: THREE.Group;
  private readonly models: Map<string, GLTF>;
  private readonly coins: Coin[];
  private readonly makeCoin: () => THREE.Group;
  private readonly billboards?: Sm64Billboards;
  private readonly views = new Map<number, View>();
  private readonly dynamicCoins: Coin[] = [];
  private lastTime = 0;

  constructor(private options: CourseObjectsOptions) {
    this.root = options.root;
    this.models = options.models;
    this.coins = options.coins;
    this.makeCoin = options.makeCoin;
    this.billboards = options.billboards;
    this.manager = new ObjectManager(
      options.collision,
      options.objects
        .filter((object): object is CourseObjectData & { behavior: ObjectBehaviorKind } => Boolean(object.behavior))
        .map(object => ({
          behavior: object.behavior,
          pos: object.pos,
          yaw: degToAngle(object.yaw),
          param: object.param ?? 0,
          model: object.model,
          warpTo: object.warpTo,
          warpArea: object.warpArea,
          warpEntry: object.warpEntry,
        })),
    );
    this.buildViews();
  }

  step(mario: MarioRuntime): ObjectEvent[] {
    this.manager.step(mario);
    const events = this.manager.drainEvents();
    const remaining: ObjectEvent[] = [];
    for (const event of events) {
      if (event.type === 'coin') this.spawnCoin(event.pos);
      else remaining.push(event);
    }
    this.syncViews();
    return remaining;
  }

  afterMarioStep(mario: MarioRuntime): void {
    this.manager.afterMarioStep(mario);
    this.syncViews();
  }

  update(time: number): void {
    const dt = Math.max(0, Math.min(0.1, time - this.lastTime));
    this.lastTime = time;
    for (const view of this.views.values()) view.mixer?.update(dt);
  }

  private wingUnlocked = false;

  setWingBoxes(unlocked: boolean): void {
    this.wingUnlocked = unlocked;
    for (const box of this.manager.objects) {
      if (box.behavior !== 'wingBox') continue;
      box.vars.unlocked = Number(unlocked);
      if (!box.action) box.hitbox.interactType = unlocked ? INTERACT_BREAKABLE : 0;
    }
    this.syncViews();
  }

  reset(): void {
    this.removeDynamicCoins();
    this.clearViews();
    this.manager.reset();
    this.buildViews();
    this.setWingBoxes(this.wingUnlocked);
    this.lastTime = 0;
  }

  dispose(): void {
    this.removeDynamicCoins();
    this.clearViews();
  }

  private buildViews(): void {
    for (const object of this.manager.objects) this.ensureView(object);
  }

  private ensureView(object: SimObject): void {
    if (this.views.has(object.id) || !object.model) return;
    const gltf = this.models.get(object.model);
    if (!gltf) throw new Error(`Object model missing: ${object.model}`);
    const group = instantiateSm64(gltf);
    group.position.set(object.pos[0], object.pos[1] + object.graphYOffset, object.pos[2]);
    group.rotation.y = angleToRadians(object.moveYaw);
    if (object.billboard) group.userData.objectBillboard = true;
    if (object.behavior === 'openableGrill' || object.behavior === 'grillDoor' || object.behavior === 'chainChompGate') {
      group.traverse(node => {
        if (!(node instanceof THREE.Mesh)) return;
        for (const material of Array.isArray(node.material) ? node.material : [node.material]) {
          material.side = THREE.DoubleSide;
          material.needsUpdate = true;
        }
      });
    }
    this.billboards?.register(group);
    this.root.add(group);
    this.options.shadows?.register(group);

    let mixer: THREE.AnimationMixer | null = null;
    if (gltf.animations.length > 0) {
      mixer = new THREE.AnimationMixer(group);
      const action = mixer.clipAction(gltf.animations[0]);
      action.setLoop(THREE.LoopRepeat, Infinity);
      action.play();
    }
    let outline: THREE.Group | undefined;
    if (object.behavior === 'wingBox') {
      outline = instantiateSm64(this.models.get('actors/exclamation_box_outline/exclamation_box_outline_geo.glb')!);
      outline.scale.setScalar(1);
      group.add(outline);
    }
    let lid: THREE.Group | undefined;
    if (object.behavior === 'chest') {
      lid = instantiateSm64(this.models.get('actors/treasure_chest/treasure_chest_lid_geo.glb')!);
      lid.scale.divideScalar(group.scale.x);
      lid.position.set(0, 102 / group.scale.x, -77 / group.scale.x);
      group.add(lid);
    }
    const collision = object.behavior === 'blastWall' ? this.options.blastWalls?.[object.param]
      : object.behavior === 'chainChompGate' ? this.options.chainGateCollision
      : object.behavior === 'woodenPost' ? this.options.woodenPostCollision
      : (object.behavior === 'starBox' || object.behavior === 'wingBox') ? this.options.starBoxCollision : undefined;
    const collider = collision ? new DoorCollider(this.options.collision, collision, object.moveYaw * 180 / 32768) : undefined;
    collider?.move(object.home);
    this.views.set(object.id, { group, baseScale: group.scale.x, mixer, lid, outline, collider, solid: !!collider, colliderY: object.home[1] });
  }

  private clearViews(): void {
    for (const view of this.views.values()) {
      view.collider?.move(null);
      view.mixer?.stopAllAction();
      this.options.shadows?.remove(view.group);
      this.root.remove(view.group);
    }
    this.views.clear();
  }

  private syncViews(): void {
    for (const object of this.manager.objects) {
      this.ensureView(object);
      const view = this.views.get(object.id);
      if (!view) continue;
      view.group.position.set(object.pos[0], object.pos[1] + object.graphYOffset, object.pos[2]);
      view.group.rotation.y = angleToRadians(object.moveYaw);
      view.group.scale.setScalar(view.baseScale * object.scale);
      if ((object.behavior === 'bowser' || object.behavior === 'babyPenguin' || object.behavior === 'tuxiesMother') && object.model && view.mixer && view.animation !== object.vars.animation) {
        // penguin_seg5_anims_05008B74 uses a different order from the GLB clip list.
        const index = object.behavior === 'bowser' ? object.vars.animation : [3,0,1,2][object.vars.animation];
        const clip = this.models.get(object.model)?.animations[index];
        if (clip) {
          view.mixer.stopAllAction();
          view.mixer.clipAction(clip).reset().play();
          view.animation = object.vars.animation;
        }
      }
      if (view.outline) {
        view.outline.visible = !this.wingUnlocked;
        view.group.children[0].visible = this.wingUnlocked;
      }
      if (object.behavior === 'wingCap') view.group.rotation.y = angleToRadians(object.faceYaw);
      if (view.collider) {
        const solid = object.active && object.action === 0 && (!view.outline || this.wingUnlocked);
        if (solid !== view.solid || solid && view.colliderY !== object.pos[1]) {
          view.collider.move(solid ? object.pos : null);
          view.colliderY = object.pos[1];
        }
        view.solid = solid;
      }
      if (object.behavior === 'waterBombCannon') view.group.rotation.y = angleToRadians(object.faceYaw);
      if (object.behavior === 'cannonBarrel') view.group.rotation.set(angleToRadians(object.vars.pitch), angleToRadians(object.faceYaw), 0, 'YXZ');
      if (object.behavior === 'starBox' || object.behavior === 'wingBox' || object.behavior === 'waterBomb') {
        view.group.scale.x *= object.vars.width ?? 1;
        view.group.scale.z = view.group.scale.x;
        view.group.scale.y *= object.vars.height ?? 1;
      }
      if (view.lid) view.lid.rotation.x = angleToRadians(object.vars.lidPitch);
      view.group.visible = object.active && object.visible;
    }
  }

  editorActors(): EditorActorTarget[] {
    const actors: EditorActorTarget[] = [];
    for (const object of this.manager.objects) {
      const view = this.views.get(object.id);
      if (!view || !object.active) continue;
      if (!['bobomb', 'kingBobomb', 'koopa', 'koopaShell', 'pitBowlingBall', 'bobombBuddy'].includes(object.behavior)) continue;
      actors.push({
        id: `object-${object.id}`,
        mesh: view.group,
        position: new THREE.Vector3(...object.pos),
        scale: object.scale,
        move: position => {
          object.pos[0] = position.x;
          object.pos[1] = Math.max(object.pos[1], position.y);
          object.pos[2] = position.z;
        },
        setScale: scale => { object.scale = scale; },
        remove: () => { object.deactivate(-1); object.visible = false; },
      });
    }
    return actors;
  }

  debugView(id: number): { visible: boolean; pos: Vec3; scale: number } | null {
    const view = this.views.get(id);
    if (!view) return null;
    return {
      visible: view.group.visible,
      pos: [view.group.position.x, view.group.position.y, view.group.position.z],
      scale: view.group.scale.x,
    };
  }

  private spawnCoin(position: Vec3): void {
    const group = this.makeCoin();
    group.scale.multiplyScalar(0.7);
    group.userData.objectBillboard = true;
    group.position.set(position[0], position[1], position[2]);
    this.root.add(group);
    const coin: Coin = {
      nativeHitbox: true,
      group,
      position: new THREE.Vector3(position[0], position[1], position[2]),
      collected: false,
    };
    this.coins.push(coin);
    this.dynamicCoins.push(coin);
  }

  private removeDynamicCoins(): void {
    for (const coin of this.dynamicCoins) {
      this.root.remove(coin.group);
      const index = this.coins.indexOf(coin);
      if (index >= 0) this.coins.splice(index, 1);
    }
    this.dynamicCoins.length = 0;
  }
}
