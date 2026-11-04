/**
 * Game shell: Three.js rendering, a fixed 30 Hz Mario simulation, camera,
 * coins, the star, particles and synthesized audio.
 */

import * as THREE from 'three';
import { MarioRuntime } from '../sim/MarioRuntime';
import { toS16 } from '../sim/math';
import {
  ACT_BACKFLIP,
  ACT_DOUBLE_JUMP,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_LONG_JUMP,
  ACT_SIDE_FLIP,
  ACT_TRIPLE_JUMP,
  ACT_WALL_KICK_AIR,
  actionName,
  type MarioEvent,
} from '../sim/state';
import { buildPlayground, addSky, type PlaygroundResult } from './world';
import { MarioModel } from './MarioModel';
import { GltfCharacterView } from './GltfCharacterView';
import { Sm64MarioView } from './Sm64MarioView';
import type { WorldFactory } from './sm64World';
import type { CharacterView } from './CharacterView';
import { CameraRig } from './CameraRig';
import { InputManager } from './Input';
import { Particles } from './Particles';
import { Sfx } from './Sfx';
import { HUD, formatTime } from './HUD';

const STEP = 1 / 30;

function byId<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing #${id}`);
  return element as T;
}

function zoneFor(x: number, z: number): string {
  if (x > 900 && x < 1450 && z < -1400) return 'STAR TOWER';
  if (z < -2000) return 'LONG JUMP GAP';
  if (x < -1000 && z < 300) return 'WALL KICK CANYON';
  if (x > 1150 && x < 1800 && z > -1500 && z < 800) return 'BUTT-SLIDE RINK';
  if (z < -260) return 'JUMP GARDEN';
  if (x < -600 && z > 200) return 'STAIR & PYRAMID';
  if (z > 2000) return 'SLOPE LAB';
  return 'SPAWN PLAZA';
}

export class Game {
  private readonly canvas: HTMLCanvasElement;
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly sun: THREE.DirectionalLight;

  private readonly playground: PlaygroundResult;
  private readonly mario: MarioRuntime;
  private readonly procedural: MarioModel;
  private character: CharacterView;
  private loadedCharacter: GltfCharacterView | Sm64MarioView | null = null;
  private characterLoading = false;
  private readonly characterUrl: string;
  private readonly clipMapUrl: string | null;
  private readonly requestedCharacter: string;
  private readonly characterLabel: string;
  private readonly characterAutoSelect: boolean;
  private readonly rig: CameraRig;
  private readonly input: InputManager;
  private readonly particles: Particles;
  private readonly sfx = new Sfx();
  private readonly hud = new HUD();

  private readonly prevPos = new THREE.Vector3();
  private readonly renderPos = new THREE.Vector3();
  private prevYaw = 0;
  private renderYaw = 0;

  private accumulator = 0;
  private lastFrame = 0;
  private time = 0;
  private elapsed = 0;
  private fps = 60;
  private shake = 0;

  private started = false;
  private won = false;
  private starReady = false;
  private coinsCollected = 0;
  private lastStarHint = -10;

  private readonly startOverlay = byId('start-overlay');
  private readonly winOverlay = byId('win-overlay');
  private readonly winText = byId('win-text');

  constructor(buildWorld: WorldFactory = buildPlayground, readonly worldName = 'playground') {
    this.canvas = byId<HTMLCanvasElement>('game');

    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      antialias: true,
      powerPreference: 'high-performance',
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.04;

    this.scene.fog = worldName === 'bob'
      ? new THREE.Fog(0xc7e8ff, 10000, 26000)
      : new THREE.Fog(0xc7e8ff, 2600, 7200);

    this.camera = new THREE.PerspectiveCamera(
      55,
      window.innerWidth / window.innerHeight,
      1,
      worldName === 'bob' ? 40000 : 9000,
    );
    this.camera.position.set(0, 300, 600);

    addSky(this.scene, worldName === 'bob' ? 30000 : 5200);

    const hemi = new THREE.HemisphereLight(0xcfeaff, 0x5e8a44, 1.15);
    this.scene.add(hemi);

    this.sun = new THREE.DirectionalLight(0xfff1cf, 2.35);
    this.sun.position.set(700, 1200, 500);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048);
    this.sun.shadow.camera.near = 1;
    this.sun.shadow.camera.far = 3000;
    this.sun.shadow.camera.left = -720;
    this.sun.shadow.camera.right = 720;
    this.sun.shadow.camera.top = 720;
    this.sun.shadow.camera.bottom = -720;
    this.sun.shadow.bias = -0.0006;
    this.sun.shadow.normalBias = 2.5;
    this.scene.add(this.sun);
    this.scene.add(this.sun.target);

    const fill = new THREE.DirectionalLight(0x9ec8ff, 0.55);
    fill.position.set(-900, 500, -600);
    this.scene.add(fill);

    this.playground = buildWorld(this.scene);
    this.mario = new MarioRuntime(this.playground.collision, this.playground.spawn);
    this.procedural = new MarioModel();
    this.character = this.procedural;
    this.scene.add(this.procedural.root);

    const params = new URLSearchParams(window.location.search);
    const requestedCharacter = params.get('character') ?? 'sm64';
    this.requestedCharacter = requestedCharacter;
    this.characterAutoSelect = requestedCharacter !== 'procedural';
    this.clipMapUrl = params.get('clips');
    this.characterLabel = ['sm64', 'procedural'].includes(requestedCharacter)
      ? 'SM64 MARIO' : requestedCharacter === 'cc0' ? 'CC0 ANIMATED RIG' : 'CUSTOM GLTF RIG';
    this.characterUrl = requestedCharacter === 'local'
      ? `${import.meta.env.BASE_URL}models/local/character.glb`
      : requestedCharacter === 'cc0'
        ? `${import.meta.env.BASE_URL}models/quaternius/AnimationLibrary.glb`
        : requestedCharacter;
    void this.loadCharacter();

    this.rig = new CameraRig(this.camera, this.playground.collisionMeshes, worldName === 'bob');
    this.rig.snapBehind(this.mario.faceAngle[1]);
    this.rig.setFloorProbe((x, z) => {
      const hit = this.playground.collision.findFloor(x, 20000, z);
      return hit.surface ? hit.height : null;
    });

    this.particles = new Particles(this.scene);
    this.input = new InputManager(this.canvas);

    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.mario.faceAngle[1];
    this.renderYaw = this.prevYaw;

    this.bindUI();
    window.addEventListener('resize', this.onResize);

    this.seedMotes();

    this.lastFrame = performance.now();
    requestAnimationFrame(this.animate);
  }

  private bindUI(): void {
    byId('start-button').addEventListener('click', (event) => {
      event.stopPropagation();
      this.start();
    });
    byId('again-button').addEventListener('click', (event) => {
      event.stopPropagation();
      this.restart();
    });

    window.addEventListener('keydown', (event) => {
      if (!this.started && (event.code === 'Space' || event.code === 'Enter')) {
        event.preventDefault();
        this.start();
      }
      if (event.code === 'KeyR') this.restart();
      if (event.code === 'F3' || event.code === 'Backquote') this.hud.toggleDebug();
      if (event.code === 'KeyC') this.rig.snapBehind(this.mario.faceAngle[1]);
      if (event.code === 'KeyG') this.toggleCollisionWireframe();
      if (event.code === 'KeyV') this.cycleCharacter();
      if (event.code === 'KeyF') this.toggleFullscreen();
    });
  }

  private seedMotes(): void {
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(180 * 3);
    for (let i = 0; i < 180; i++) {
      positions[i * 3] = (Math.random() - 0.5) * 2600;
      positions[i * 3 + 1] = 80 + Math.random() * 700;
      positions[i * 3 + 2] = (Math.random() - 0.5) * 2600;
    }
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    const material = new THREE.PointsMaterial({
      color: 0xfff4c8,
      size: 4,
      transparent: true,
      opacity: 0.35,
      depthWrite: false,
      sizeAttenuation: true,
    });
    const motes = new THREE.Points(geometry, material);
    motes.frustumCulled = false;
    this.scene.add(motes);
  }

  private async loadCharacter(): Promise<void> {
    this.characterLoading = true;
    try {
      let aliases: unknown;
      if (this.clipMapUrl) {
        const response = await fetch(this.clipMapUrl);
        if (!response.ok) {
          throw new Error(`clip map ${this.clipMapUrl}: HTTP ${response.status}`);
        }
        aliases = await response.json();
      }

      const view = ['sm64', 'procedural'].includes(this.requestedCharacter)
        ? await Sm64MarioView.load()
        : await GltfCharacterView.load(this.characterUrl, { aliases });
      view.setVisible(false);
      this.scene.add(view.root);
      this.loadedCharacter = view;
      this.characterLoading = false;
      if (this.characterAutoSelect) {
        this.procedural.setVisible(false);
        view.setVisible(true);
        this.character = view;
      }
      this.hud.showToast(`${this.characterLabel} READY — V TO SWAP`, 2600);
    } catch (error) {
      this.characterLoading = false;
      console.warn('[jumpman] could not load character', error);
      this.hud.showToast('GLTF RIG FAILED TO LOAD — USING MARIO', 3200);
    }
  }

  private cycleCharacter(): void {
    if (!this.loadedCharacter) {
      this.hud.showToast(
        this.characterLoading ? 'CHARACTER STILL LOADING…' : 'NO GLTF RIG LOADED',
        1800,
      );
      return;
    }

    const useCc0 = this.character !== this.loadedCharacter;
    this.procedural.setVisible(!useCc0);
    this.loadedCharacter.setVisible(useCc0);
    this.character = useCc0 ? this.loadedCharacter : this.procedural;
    this.hud.showToast(useCc0 ? this.characterLabel : 'PROCEDURAL MARIO RIG', 1400);
  }

  swapCharacter(): void {
    this.cycleCharacter();
  }

  private toggleCollisionWireframe(): void {
    const seen = new Set<THREE.Material>();
    for (const mesh of this.playground.collisionMeshes) {
      const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
      for (const material of materials) {
        if (seen.has(material)) continue;
        seen.add(material);
        if (material instanceof THREE.MeshStandardMaterial) {
          material.wireframe = !material.wireframe;
        }
      }
    }
    this.hud.showToast('COLLISION WIREFRAME');
  }

  private toggleFullscreen(): void {
    if (!document.fullscreenElement) {
      void document.documentElement.requestFullscreen?.();
    } else {
      void document.exitFullscreen?.();
    }
  }

  private start(): void {
    if (this.started) return;
    this.started = true;
    this.startOverlay.hidden = true;
    this.sfx.ensure();
    this.accumulator = 0;
    this.lastFrame = performance.now();
    this.elapsed = 0;
    this.hud.showToast('GO!  A = JUMP · HOLD FOR HEIGHT', 2600);
  }

  private restart(): void {
    this.mario.resetToSpawn();
    for (const coin of this.playground.coins) {
      coin.collected = false;
      coin.group.visible = true;
    }
    this.coinsCollected = 0;
    this.starReady = false;
    this.won = false;
    this.elapsed = 0;
    this.accumulator = 0;
    this.lastStarHint = -10;
    this.winOverlay.hidden = true;
    this.rig.snapBehind(this.mario.faceAngle[1]);
    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.mario.faceAngle[1];
    this.renderYaw = this.prevYaw;
    this.hud.showToast('RUN RESET');
  }

  private onResize = (): void => {
    this.camera.aspect = window.innerWidth / window.innerHeight;
    this.camera.updateProjectionMatrix();
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(window.innerWidth, window.innerHeight);
  };

  private animate = (now: number): void => {
    const rawDt = (now - this.lastFrame) / 1000;
    this.lastFrame = now;
    const dt = Math.min(0.1, Math.max(0.0001, rawDt));
    this.time += dt;

    this.fps = this.fps * 0.92 + (1 / dt) * 0.08;
    this.input.pollGamepad();

    if (this.started && !this.won) {
      this.accumulator += dt;
      let steps = 0;
      while (this.accumulator >= STEP && steps < 5) {
        this.stepSimulation();
        this.accumulator -= STEP;
        steps++;
      }
      if (steps === 5) this.accumulator = 0;
    } else if (!this.started) {
      this.accumulator = 0;
    }

    const alpha = this.won || !this.started ? 1 : Math.min(1, this.accumulator / STEP);
    this.renderPos.set(
      THREE.MathUtils.lerp(this.prevPos.x, this.mario.pos[0], alpha),
      THREE.MathUtils.lerp(this.prevPos.y, this.mario.pos[1], alpha),
      THREE.MathUtils.lerp(this.prevPos.z, this.mario.pos[2], alpha),
    );
    this.renderYaw = toS16(
      this.prevYaw + toS16(this.mario.faceAngle[1] - this.prevYaw) * alpha,
    );

    const cam = this.input.consumeCameraInput();
    if (cam.dx !== 0 || cam.dy !== 0) this.rig.rotate(cam.dx, cam.dy);
    if (cam.orbitX !== 0 || cam.orbitY !== 0) {
      this.rig.orbit(cam.orbitX * dt * 1.8, cam.orbitY * dt * 1.1);
    }
    if (cam.wheel !== 0) this.rig.zoom(cam.wheel);

    this.rig.update(dt, this.renderPos, this.renderYaw);

    if (this.shake > 0.01) {
      this.camera.position.x += (Math.random() - 0.5) * this.shake;
      this.camera.position.y += (Math.random() - 0.5) * this.shake;
      this.camera.position.z += (Math.random() - 0.5) * this.shake;
      this.shake *= Math.exp(-7 * dt);
    } else {
      this.shake = 0;
    }

    this.character.update(this.mario, dt, this.renderPos, this.renderYaw);
    this.playground.update(this.time);
    this.character.faceCamera?.(this.camera);
    this.playground.faceCamera?.(this.camera);
    this.particles.update(dt);

    this.sun.position.set(this.renderPos.x + 700, this.renderPos.y + 1200, this.renderPos.z + 500);
    this.sun.target.position.set(this.renderPos.x, this.renderPos.y, this.renderPos.z);
    this.sun.target.updateMatrixWorld();

    const zone = this.worldName === 'bob' ? 'BOB-OMB BATTLEFIELD' : zoneFor(this.renderPos.x, this.renderPos.z);
    this.hud.update(this.mario, {
      coins: this.coinsCollected,
      totalCoins: this.playground.coins.length,
      time: this.elapsed,
      fps: this.fps,
      zone,
      starReady: this.starReady,
    });

    this.renderer.render(this.scene, this.camera);
    requestAnimationFrame(this.animate);
  };

  private stepSimulation(): void {
    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.prevYaw = this.mario.faceAngle[1];

    const controller = this.input.consumeStep();
    this.mario.step(controller, this.rig.yawS16);

    this.processEvents(this.mario.events);
    this.collectCoins();
    this.checkStar();
    this.elapsed += STEP;
  }

  private processEvents(events: MarioEvent[]): void {
    if (events.length === 0) return;

    const feet = new THREE.Vector3(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    const head = feet.clone().add(new THREE.Vector3(0, 95, 0));

    for (const event of events) {
      switch (event.type) {
        case 'jump':
          if (event.action === ACT_DOUBLE_JUMP) this.sfx.doubleJump();
          else if (event.action === ACT_TRIPLE_JUMP) this.sfx.tripleJump();
          else if (
            event.action === ACT_BACKFLIP ||
            event.action === ACT_SIDE_FLIP ||
            event.action === ACT_WALL_KICK_AIR ||
            event.action === ACT_JUMP_KICK
          ) {
            this.sfx.jump(1.18);
          } else if (event.action === ACT_LONG_JUMP) {
            this.sfx.jump(0.85);
          } else if (event.action === ACT_JUMP) {
            this.sfx.jump(1);
          }
          this.particles.dust(feet, 4);
          break;

        case 'triplejump':
          this.sfx.tripleJump();
          this.particles.sparks(feet, 12);
          break;

        case 'land': {
          const strength = Math.min(1.7, Math.abs(event.vy) / 45);
          this.sfx.land(strength);
          this.particles.dust(feet, Math.round(5 + strength * 6));
          if (strength > 1.1) this.shake = Math.max(this.shake, 7);
          break;
        }

        case 'bonk':
          this.sfx.bonk();
          this.particles.sparks(head, 8);
          this.shake = Math.max(this.shake, 5);
          break;

        case 'wallkick':
          this.sfx.wallKick();
          this.particles.sparks(head, 12);
          this.hud.showToast('WALL KICK', 900);
          break;

        case 'groundpound':
          this.sfx.groundPound();
          this.particles.pound(feet);
          this.shake = Math.max(this.shake, 16);
          break;

        case 'dive':
          this.sfx.dive();
          break;

        case 'respawn':
          this.particles.dust(feet, 12);
          this.hud.showToast(this.worldName === 'bob' ? 'BACK TO THE BATTLEFIELD' : 'BACK TO THE PLAZA', 1400);
          this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
          this.prevYaw = this.mario.faceAngle[1];
          this.rig.snapBehind(this.mario.faceAngle[1]);
          break;
      }
    }
  }

  private collectCoins(): void {
    const px = this.mario.pos[0];
    const py = this.mario.pos[1] + 55;
    const pz = this.mario.pos[2];

    for (const coin of this.playground.coins) {
      if (coin.collected) continue;
      const dx = px - coin.position.x;
      const dy = py - coin.position.y;
      const dz = pz - coin.position.z;
      if (dx * dx + dy * dy + dz * dz > 70 * 70) continue;

      coin.collected = true;
      coin.group.visible = false;
      this.coinsCollected++;
      this.sfx.coin();
      this.particles.sparks(coin.position, 10);

      if (this.coinsCollected === this.playground.coins.length) {
        this.starReady = true;
        this.hud.showToast(this.worldName === 'bob' ? '★ STAR READY — REACH THE SUMMIT ★' : '★ STAR READY — CLIMB THE TOWER ★', 4200);
        this.sfx.star();
        this.shake = Math.max(this.shake, 6);
      } else {
        this.hud.showToast(`COIN ${this.coinsCollected} / ${this.playground.coins.length}`, 700);
      }
    }
  }

  private checkStar(): void {
    if (this.won) return;
    const star = this.playground.star.position;
    const dx = this.mario.pos[0] - star.x;
    const dy = this.mario.pos[1] + 60 - star.y;
    const dz = this.mario.pos[2] - star.z;
    if (dx * dx + dy * dy + dz * dz > 130 * 130) return;

    if (!this.starReady) {
      if (this.elapsed - this.lastStarHint > 3) {
        this.lastStarHint = this.elapsed;
        this.hud.showToast(
          `THE STAR IS ASLEEP — ${this.coinsCollected}/${this.playground.coins.length} COINS`,
          2200,
        );
      }
      return;
    }

    this.won = true;
    this.starReady = false;
    this.sfx.star();
    this.particles.pound(star);
    this.particles.sparks(star, 40);
    this.shake = Math.max(this.shake, 10);
    this.winText.textContent = `You collected all ${this.playground.coins.length} coins and reached the star in ${formatTime(
      this.elapsed,
    )}.`;
    this.winOverlay.hidden = false;
  }

  /** Debug helper used by the screenshot harness. */
  teleport3(x: number, y: number, z: number, faceYaw = 0x8000): void {
    const hit = this.playground.collision.findFloor(x, y + 100, z);
    this.mario.pos = [x, y, z];
    this.mario.vel = [0, 0, 0];
    this.mario.forwardVel = 0;
    this.mario.faceAngle = [0, toS16(faceYaw), 0];
    this.mario.floor = hit.surface;
    this.mario.floorHeight = hit.height;
    this.mario.action = 0x0c400201; // ACT_IDLE
    this.mario.actionState = 0;
    this.mario.actionTimer = 0;
    this.mario.actionArg = 0;
    this.prevPos.set(x, y, z);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.mario.faceAngle[1];
    this.renderYaw = this.prevYaw;
    this.rig.snapBehind(this.mario.faceAngle[1]);
    this.accumulator = 0;
  }

  teleport(x: number, z: number, faceYaw = 0x8000): void {
    const hit = this.playground.collision.findFloor(x, 2000, z);
    this.mario.pos = [x, hit.surface ? hit.height : 0, z];
    this.mario.vel = [0, 0, 0];
    this.mario.forwardVel = 0;
    this.mario.faceAngle = [0, toS16(faceYaw), 0];
    this.mario.floor = hit.surface;
    this.mario.floorHeight = hit.height;
    this.mario.action = 0x0c400201; // ACT_IDLE
    this.mario.actionState = 0;
    this.mario.actionTimer = 0;
    this.mario.actionArg = 0;
    this.prevPos.set(x, this.mario.pos[1], z);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.mario.faceAngle[1];
    this.renderYaw = this.prevYaw;
    this.rig.snapBehind(this.mario.faceAngle[1]);
    this.accumulator = 0;
  }

  /** Exposed for automated smoke tests. */
  get debugState() {
    return {
      started: this.started,
      won: this.won,
      coins: this.coinsCollected,
      action: this.mario.action,
      actionName: actionName(this.mario.action),
      pos: [...this.mario.pos],
      vel: [...this.mario.vel],
      forwardVel: this.mario.forwardVel,
      floorHeight: this.mario.floorHeight,
      starReady: this.starReady,
      cameraYaw: this.rig.yawS16,
      cameraPos: [this.camera.position.x, this.camera.position.y, this.camera.position.z],
      faceYaw: this.mario.faceAngle[1],
      floorNormal: this.mario.floor ? [...this.mario.floor.normal] : null,
      floorType: this.mario.floor?.type ?? -1,
      input: this.mario.input,
      prevAction: this.mario.prevAction,
      doubleJumpTimer: this.mario.doubleJumpTimer,
      transitions: this.mario.transitions,
      world: this.worldName,
      collisionTriangles: this.playground.collision.surfaces.length,
      character: this.character === this.procedural ? 'procedural' : this.character instanceof Sm64MarioView ? 'sm64' : 'gltf',
      characterLoaded: this.loadedCharacter !== null,
      characterClips: this.loadedCharacter?.clipNames.length ?? 0,
      characterClip: this.character === this.loadedCharacter ? (this.loadedCharacter?.activeClip ?? null) : null,
      characterAnimation: this.character instanceof Sm64MarioView ? this.character.animationDebug : null,
      characterRotation: { yaw: this.character.root.rotation.y, pitch: this.character.root.rotation.x },
      pose: this.procedural.poseDebug,
      coinPositions: this.playground.coins.map((c) => ({ x: c.position.x, y: c.position.y, z: c.position.z, collected: c.collected })),
      starPos: [this.playground.star.position.x, this.playground.star.position.y, this.playground.star.position.z],
    };
  }
}
