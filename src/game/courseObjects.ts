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
import type { ObjectBehaviorKind } from '../sim/object';
import { instantiateSm64 } from './sm64Assets';
import type { Coin } from './world';

export interface CourseObjectData {
  model: string;
  pos: Vec3;
  yaw: number;
  param?: number;
  behavior?: ObjectBehaviorKind;
}

interface View {
  group: THREE.Group;
  baseScale: number;
  mixer: THREE.AnimationMixer | null;
}

export interface CourseObjectsOptions {
  root: THREE.Group;
  models: Map<string, GLTF>;
  collision: CollisionWorld;
  objects: CourseObjectData[];
  coins: Coin[];
  makeCoin: () => THREE.Group;
}

export class CourseObjects {
  readonly manager: ObjectManager;
  private readonly root: THREE.Group;
  private readonly models: Map<string, GLTF>;
  private readonly coins: Coin[];
  private readonly makeCoin: () => THREE.Group;
  private readonly views = new Map<number, View>();
  private readonly dynamicCoins: Coin[] = [];
  private lastTime = 0;

  constructor(options: CourseObjectsOptions) {
    this.root = options.root;
    this.models = options.models;
    this.coins = options.coins;
    this.makeCoin = options.makeCoin;
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

  reset(): void {
    this.removeDynamicCoins();
    this.clearViews();
    this.manager.reset();
    this.buildViews();
    this.lastTime = 0;
  }

  dispose(): void {
    this.removeDynamicCoins();
    this.clearViews();
  }

  private buildViews(): void {
    for (const object of this.manager.objects) {
      if (!object.model) continue;
      const gltf = this.models.get(object.model);
      if (!gltf) throw new Error(`Object model missing: ${object.model}`);
      const group = instantiateSm64(gltf);
      group.position.fromArray(object.pos);
      group.rotation.y = angleToRadians(object.moveYaw);
      this.root.add(group);

      let mixer: THREE.AnimationMixer | null = null;
      if (gltf.animations.length > 0) {
        mixer = new THREE.AnimationMixer(group);
        const action = mixer.clipAction(gltf.animations[0]);
        action.setLoop(THREE.LoopRepeat, Infinity);
        action.play();
      }
      this.views.set(object.id, { group, baseScale: group.scale.x, mixer });
    }
  }

  private clearViews(): void {
    for (const view of this.views.values()) {
      view.mixer?.stopAllAction();
      this.root.remove(view.group);
    }
    this.views.clear();
  }

  private syncViews(): void {
    for (const object of this.manager.objects) {
      const view = this.views.get(object.id);
      if (!view) continue;
      view.group.position.fromArray(object.pos);
      view.group.rotation.y = angleToRadians(object.moveYaw);
      view.group.scale.setScalar(view.baseScale * object.scale);
      view.group.visible = object.active && object.visible;
    }
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
