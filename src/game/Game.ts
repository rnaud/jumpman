/**
 * Game shell: Three.js rendering, a fixed 30 Hz Mario simulation, camera,
 * coins, the star, particles and synthesized audio.
 */

import * as THREE from 'three';
import { MarioRuntime } from '../sim/MarioRuntime';
import { atan2s, toS16 } from '../sim/math';
import {
  ACT_PULLING_DOOR, ACT_PUSHING_DOOR, ACT_WALKING, ACT_DECELERATING,
  ACT_FLAG_INVULNERABLE, ACT_FLAG_ATTACKING, MARIO_PUNCHING, MARIO_KICKING, MARIO_TRIPPING,
  ACT_BACKFLIP,
  ACT_DOUBLE_JUMP,
  ACT_FREEFALL,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_LONG_JUMP,
  ACT_SIDE_FLIP,
  ACT_TRIPLE_JUMP,
  ACT_WALL_KICK_AIR,
  actionName,
  type MarioEvent,
} from '../sim/state';
import { buildPlayground, addSky, type CastleDoor, type PlaygroundResult } from './world';
import { MarioModel } from './MarioModel';
import { GltfCharacterView } from './GltfCharacterView';
import { Sm64MarioView } from './Sm64MarioView';
import { playgroundZone as zoneFor } from './playgroundLayout';
import { preparePlayground } from './sm64Playground';
import { prepareCourse, type WorldFactory } from './sm64World';
import type { CharacterView } from './CharacterView';
import { crossesGoomba } from './goomba';
import type { ObjectEvent } from '../sim/objectManager';
import { CameraRig } from './CameraRig';
import { InputManager } from './Input';
import { Particles } from './Particles';
import { Sfx } from './Sfx';
import { HUD, formatTime } from './HUD';
import { CLASSIC, MODERN, readDisplaySettings, displaySize, type DisplaySettings } from './displaySettings';
import { COURSES, missionsFor, type CourseId } from './courses';
import { activeSaveSlot, SaveFile, TOTAL_STARS } from './save';
import { paintingDestination } from './painting';
import { RuntimeEditor } from './editor';

const STEP = 1 / 30;

function byId<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing #${id}`);
  return element as T;
}


export class Game {
  private display = readDisplaySettings();
  private readonly settings = byId<HTMLDialogElement>('settings');
  private readonly canvas: HTMLCanvasElement;
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly sun: THREE.DirectionalLight;
  private readonly blobShadow = new THREE.Mesh(
    new THREE.CircleGeometry(55, 24),
    new THREE.MeshBasicMaterial({ color: 0x000000, transparent: true, opacity: 0.28, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2 }),
  );

  private playground: PlaygroundResult;
  private mario: MarioRuntime;
  private readonly procedural: MarioModel;
  private character: CharacterView;
  private loadedCharacter: GltfCharacterView | Sm64MarioView | null = null;
  private characterLoading = false;
  private readonly characterUrl: string;
  private readonly clipMapUrl: string | null;
  private readonly requestedCharacter: string;
  private readonly characterLabel: string;
  private readonly characterAutoSelect: boolean;
  private rig: CameraRig;
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

  private courseScene = new THREE.Scene();
  private skyScene = new THREE.Scene();
  private loadingCourse = false;
  private stepDistance = 0;
  private started = false;
  private enteringDoor?: CastleDoor;
  private enteringPainting = false;
  private viewMode: '3d' | 'side' | 'top' = '3d';
  private readonly editor: RuntimeEditor;
  private editorEnabled = false;
  private sideLaneZ = 0;
  private underwater = false;
  private baseFog: THREE.Fog | null = null;
  private baseBackground: THREE.Color | THREE.Texture | null = null;
  private lastSwimSound = 0;
  private won = false;
  private saveSlot = activeSaveSlot();
  private save = new SaveFile(this.saveSlot);
  private starVisibleByCourse = true;
  private starReady = false;
  private coinsCollected = 0;
  private lastStarHint = -10;

  private readonly startOverlay = byId('start-overlay');
  private readonly winOverlay = byId('win-overlay');
  private readonly winText = byId('win-text');

  constructor(buildWorld: WorldFactory = buildPlayground, public worldName: CourseId = 'playground', public act = 1) {
    this.canvas = byId<HTMLCanvasElement>('game');

    this.renderer = new THREE.WebGLRenderer({
      canvas: this.canvas,
      antialias: false,
      powerPreference: 'high-performance',
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFShadowMap;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.04;

    this.scene.fog = worldName !== 'playground'
      ? new THREE.Fog(0xc7e8ff, 10000, 26000)
      : new THREE.Fog(0xc7e8ff, 6500, 16000);

    this.camera = new THREE.PerspectiveCamera(
      55,
      window.innerWidth / window.innerHeight,
      1,
      worldName !== 'playground' ? 40000 : 26000,
    );
    this.camera.position.set(0, 300, 600);

    if (worldName === 'pss') { this.scene.background = new THREE.Color(0x000000); this.scene.fog = null; }
    else addSky(this.skyScene, worldName !== 'playground' ? 30000 : 20000);
    this.scene.add(this.skyScene, this.courseScene);
    this.scene.add(this.blobShadow);

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

    this.playground = buildWorld(this.courseScene);
    this.mario = new MarioRuntime(this.playground.collision, this.playground.spawn, this.playground.deathHeight);
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

    this.rig = new CameraRig(this.camera, this.playground.collisionMeshes, worldName !== 'playground');
    this.rig.snapBehind(this.mario.faceAngle[1]);
    this.rig.setFloorProbe((x, y, z) => {
      const hit = this.playground.collision.findFloor(x, y, z);
      return hit.surface ? hit.height : null;
    });

    this.particles = new Particles(this.scene);
    this.input = new InputManager(this.canvas);
    this.editor = new RuntimeEditor(
      this.scene, this.camera, this.canvas, this.playground.collisionMeshes, this.playground.collision,
      `jumpman.editor.v1.${worldName}`,
    );

    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.mario.faceAngle[1];
    this.renderYaw = this.prevYaw;

    this.starVisibleByCourse = this.playground.star.visible;
    if (this.starVisibleByCourse) this.playground.star.visible = !this.save.hasStar(worldName);
    this.starReady = this.playground.requireCoins === false && this.playground.star.visible && !this.save.hasStar(worldName);
    this.bindUI();
    this.bindSettings();
    this.populateMissions();
    this.populateSaveSlots();
    const initialView = new URLSearchParams(window.location.search).get('view');
    if (initialView === 'side' || initialView === 'top') this.setViewMode(initialView, false);
    window.addEventListener('resize', this.onResize);

    this.seedMotes();

    this.lastFrame = performance.now();
    requestAnimationFrame(this.animate);
  }

  /** Replace only course state; keep the renderer, input, character and audio alive. */
  async loadCourse(id: CourseId, entry?: string | null, push = false, historyMode: 'push' | 'none' = 'push', act = 1): Promise<void> {
    if (this.loadingCourse) return;
    this.loadingCourse = true;
    this.canvas.style.clipPath = '';
    this.settings.close();
    this.input.clear();
    this.sfx.ensure();
    this.hud.showToast(`LOADING ${COURSES[id].name.toUpperCase()}…`, 60000);
    const next = new THREE.Scene();
    try {
      const factory = id === 'playground' ? await preparePlayground() : await prepareCourse(id, entry, push, act);
      const world = factory(next);
      const mario = new MarioRuntime(world.collision, world.spawn, world.deathHeight);
      const rig = new CameraRig(this.camera, world.collisionMeshes, id !== 'playground');
      rig.setFloorProbe((x, y, z) => {
        const hit = world.collision.findFloor(x, y, z);
        return hit.surface ? hit.height : null;
      });
      this.playground.dispose?.();
      if (this.worldName === 'playground') this.disposeGeneratedScene(this.courseScene);
      this.scene.remove(this.courseScene);
      this.courseScene = next;
      this.scene.add(next);
      this.playground = world;
      this.editor.setWorld(world.collision, world.collisionMeshes, `jumpman.editor.v1.${id}`);
      this.mario = mario;
      this.mario.movementMode = this.viewMode === 'side' ? 'side' : this.viewMode === 'top' ? 'topdown' : 'default';
      this.rig = rig;
      this.worldName = id;
      this.act = act;
      this.time = 0;
      this.shake = 0;
      this.particles.clear();
      this.disposeGeneratedScene(this.skyScene);
      this.skyScene.clear();
      this.scene.background = id === 'pss' ? new THREE.Color(0) : null;
      this.scene.fog = id === 'pss' ? null : new THREE.Fog(0xc7e8ff, id === 'playground' ? 6500 : 10000, id === 'playground' ? 16000 : 26000);
      this.baseFog = this.scene.fog ? this.scene.fog.clone() : null;
      this.baseBackground = this.scene.background instanceof THREE.Color
        ? this.scene.background.clone()
        : this.scene.background;
      if (id !== 'pss') addSky(this.skyScene, id === 'playground' ? 20000 : 30000);
      this.camera.far = id === 'playground' ? 26000 : 40000;
      this.camera.updateProjectionMatrix();
      this.starVisibleByCourse = world.star.visible;
      this.restart();
      this.start();
      if (this.viewMode !== '3d') this.setViewMode(this.viewMode, false);
      const url = new URL(location.href);
      url.searchParams.set('level', id);
      url.searchParams.delete('entry'); url.searchParams.delete('push');
      if (act > 1) url.searchParams.set('act', String(act)); else url.searchParams.delete('act');
      if (entry) { url.searchParams.set('entry', entry); url.searchParams.set('push', push ? '1' : '0'); }
      if (historyMode === 'push') history.pushState(null, '', url);
      byId('course-name').textContent = COURSES[id].name;
      byId('load-note').textContent = COURSES[id].note;
      document.querySelectorAll<HTMLAnchorElement>('.course-links a').forEach(link => {
        if (new URL(link.href).searchParams.get('level') === id) link.setAttribute('aria-current', 'page');
        else link.removeAttribute('aria-current');
      });
      this.populateMissions();
      const mission = missionsFor(id).find(candidate => candidate.act === act);
      this.hud.showToast(mission ? mission.name.toUpperCase() : COURSES[id].name.toUpperCase(), 2600);
    } catch (error) {
      console.error('[jumpman] course load failed', error);
      this.enteringDoor = undefined;
      this.mario.resetToSpawn();
      this.hud.showToast('COURSE COULD NOT LOAD — TRY AGAIN', 5000);
    } finally {
      this.loadingCourse = false;
      this.canvas.focus();
      this.accumulator = 0;
      this.lastFrame = performance.now();
    }
  }

  private disposeGeneratedScene(root: THREE.Object3D): void {
    root.traverse(object => {
      if (object instanceof THREE.Mesh && !object.userData.sharedSm64Asset) {
        object.geometry.dispose();
        for (const material of Array.isArray(object.material) ? object.material : [object.material]) material.dispose();
      }
    });
  }

  private bindUI(): void {
    const mute = byId<HTMLInputElement>('mute-audio');
    mute.checked = this.sfx.muted;
    mute.addEventListener('change', () => this.sfx.setMuted(mute.checked));
    byId('start-button').addEventListener('click', (event) => {
      event.stopPropagation();
      this.start();
    });
    byId('again-button').addEventListener('click', (event) => {
      event.stopPropagation();
      this.restart();
    });
    byId('reset-save').addEventListener('click', () => {
      this.save.reset();
      this.restart();
      this.populateSaveSlots();
    const initialView = new URLSearchParams(window.location.search).get('view');
    if (initialView === 'side' || initialView === 'top') this.setViewMode(initialView, false);
      this.hud.showToast('STARS & SAVED COINS RESET', 2400);
    });

    window.addEventListener('keydown', (event) => {
      if (event.code === 'Escape' && this.started && !this.settings.open && !event.repeat) {
        event.preventDefault();
        this.openPauseMenu();
        return;
      }
      if (this.settings.open || (event.target instanceof HTMLElement && event.target.closest('button, select, input, a'))) return;
      if (!this.started && (event.code === 'Space' || event.code === 'Enter')) {
        event.preventDefault();
        this.start();
      }
      if (event.code === 'KeyR') this.restart();
      if (event.code === 'F3' || event.code === 'Backquote') this.hud.toggleDebug();
      if (event.code === 'KeyC') {
        const mode = this.rig.cycleMode();
        this.hud.showToast(`CAMERA · ${mode.toUpperCase()}`, 1400);
      }
      if (event.code === 'KeyX') this.rig.snapBehind(this.mario.faceAngle[1]);
      if (event.code === 'KeyM') this.cycleViewMode();
      if (event.code === 'KeyE') this.toggleEditor();
      if (this.editorEnabled && (event.ctrlKey || event.metaKey) && event.code === 'KeyZ') {
        event.preventDefault();
        if (event.shiftKey) this.editor.redo(); else this.editor.undo();
        return;
      }
      if (event.code === 'Delete' || event.code === 'Backspace') this.editor.deleteSelected();
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
      if (mesh.userData.nativeCollision) { mesh.visible = !mesh.visible; continue; }
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

  start(): void {
    if (this.started) return;
    this.started = true;
    this.startOverlay.hidden = true;
    this.canvas.focus();
    this.sfx.ensure();
    this.accumulator = 0;
    this.lastFrame = performance.now();
    this.elapsed = 0;
    this.hud.showToast('GO!  A = JUMP · HOLD FOR HEIGHT', 2600);
  }

  private restart(): void {
    this.enteringDoor = undefined;
    this.canvas.style.clipPath = '';
    this.playground.reset?.();
    for (const platform of this.playground.platforms ?? []) platform.reset(this.playground.collision);
    this.mario.resetToSpawn();
    if (this.viewMode === 'side') this.sideLaneZ = this.mario.pos[2];
    for (const enemy of this.playground.goombas ?? []) enemy.reset();
    for (const coin of this.playground.coins) {
      coin.collected = false;
      coin.group.visible = true;
    }
    this.stepDistance = 0;
    this.coinsCollected = 0;
    if (this.starVisibleByCourse) this.playground.star.visible = !this.save.hasStar(this.worldName);
    this.starReady = this.playground.requireCoins === false && this.playground.star.visible && !this.save.hasStar(this.worldName);
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

  private openPauseMenu(): void {
    this.input.clear();
    this.settings.showModal();
  }

  private setSaveSlot(slot: number): void {
    if (slot === this.saveSlot) return;
    this.saveSlot = slot;
    SaveFile.select(slot);
    this.save = new SaveFile(slot);
    this.restart();
    this.populateSaveSlots();
    const initialView = new URLSearchParams(window.location.search).get('view');
    if (initialView === 'side' || initialView === 'top') this.setViewMode(initialView, false);
    this.hud.showToast(`SAVE FILE ${'ABCD'[slot - 1]} SELECTED`, 2000);
  }

  private populateSaveSlots(): void {
    const container = byId('save-slots');
    container.replaceChildren();
    for (let slot = 1; slot <= 4; slot++) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = `${'ABCD'[slot - 1]} · ${new SaveFile(slot).stars}/${TOTAL_STARS}★`;
      if (slot === this.saveSlot) button.setAttribute('aria-current', 'true');
      button.addEventListener('click', () => this.setSaveSlot(slot));
      container.append(button);
    }
  }

  private populateMissions(): void {
    const nav = byId('mission-links');
    nav.replaceChildren();
    const missions = missionsFor(this.worldName);
    if (missions.length < 2) {
      nav.hidden = true;
      return;
    }
    nav.hidden = false;
    for (const mission of missions) {
      const link = document.createElement('a');
      link.href = '#';
      link.textContent = `${mission.act}. ${mission.name}`;
      if (mission.act === this.act) link.setAttribute('aria-current', 'page');
      link.addEventListener('click', event => {
        event.preventDefault();
        if (mission.act === this.act) return;
        void this.loadCourse(this.worldName, undefined, true, 'push', mission.act);
      });
      nav.append(link);
    }
  }

  private bindSettings(): void {
    byId('settings-button').addEventListener('click', () => this.openPauseMenu());
    this.settings.addEventListener('close', () => {
      this.input.clear();
      this.lastFrame = performance.now();
      this.canvas.focus();
    });
    for (const key of Object.keys(this.display) as (keyof DisplaySettings)[]) {
      byId<HTMLSelectElement>(`display-${key}`).addEventListener('change', event => {
        Object.assign(this.display, { [key]: (event.target as HTMLSelectElement).value });
        this.applyDisplay();
      });
    }
    byId('classic-preset').addEventListener('click', () => { this.display = { ...CLASSIC }; this.applyDisplay(); });
    byId('modern-preset').addEventListener('click', () => { this.display = { ...MODERN }; this.applyDisplay(); });
    this.applyDisplay();
  }

  private applyDisplay(): void {
    for (const key of Object.keys(this.display) as (keyof DisplaySettings)[]) byId<HTMLSelectElement>(`display-${key}`).value = this.display[key];
    try { localStorage.setItem('jumpman-display', JSON.stringify({ ...this.display, version: 2 })); } catch { /* Storage can be unavailable. */ }
    const classic = this.display.look === 'classic';
    document.body.classList.toggle('classic-look', classic);
    this.renderer.toneMapping = classic ? THREE.NoToneMapping : THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = classic ? 1 : 1.04;
    this.renderer.shadowMap.enabled = !classic;
    this.sun.intensity = classic ? 1.2 : 2.35;
    this.camera.fov = classic ? 45 : 55;
    this.canvas.style.imageRendering = this.display.scaling === 'sharp' ? 'pixelated' : 'auto';
    this.onResize();
  }

  private onResize = (): void => {
    const size = displaySize(this.display, window.innerWidth, window.innerHeight, window.devicePixelRatio);
    this.camera.aspect = size.aspect;
    this.camera.updateProjectionMatrix();
    this.renderer.setPixelRatio(1);
    this.renderer.setSize(size.width, size.height, false);
    this.canvas.style.width = `${size.cssWidth}px`;
    this.canvas.style.height = `${size.cssHeight}px`;
    byId('display-size').textContent = `${size.width} × ${size.height} internal resolution · physics stay at 30 Hz`;
  };

  private animate = (now: number): void => {
    const rawDt = (now - this.lastFrame) / 1000;
    this.lastFrame = now;
    const dt = Math.min(0.1, Math.max(0.0001, rawDt));

    this.fps = this.fps * 0.92 + (1 / dt) * 0.08;
    this.input.pollGamepad();
    if (this.input.consumeStart()) {
      if (this.settings.open) this.settings.close();
      else if (!this.started) this.start();
      else if (!this.won) this.openPauseMenu();
    }
    if (this.settings.open || this.loadingCourse) {
      this.renderer.render(this.scene, this.camera);
      requestAnimationFrame(this.animate);
      return;
    }

    this.time += dt;
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

    const alpha = this.display.motion === 'stepped' || this.won || !this.started ? 1 : Math.min(1, this.accumulator / STEP);
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

    this.rig.setWaterMode(this.underwater);
    const king = this.playground.objects?.manager.objects.find(object => object.behavior === 'kingBobomb' && object.active);
    const kingNear = king && Math.hypot(king.pos[0] - this.mario.pos[0], king.pos[2] - this.mario.pos[2]) < 2500;
    this.rig.setBossFocus(kingNear ? new THREE.Vector3(king.pos[0], king.pos[1] + 120, king.pos[2]) : null);
    this.updateViewCamera(dt);

    if (this.shake > 0.01) {
      this.camera.position.x += (Math.random() - 0.5) * this.shake;
      this.camera.position.y += (Math.random() - 0.5) * this.shake;
      this.camera.position.z += (Math.random() - 0.5) * this.shake;
      this.shake *= Math.exp(-7 * dt);
    } else {
      this.shake = 0;
    }

    this.blobShadow.visible = this.display.look === 'classic' && this.mario.floor !== null;
    if (this.mario.floor) {
      this.blobShadow.position.set(this.renderPos.x, this.mario.floorHeight + 2, this.renderPos.z);
      this.blobShadow.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), new THREE.Vector3(...this.mario.floor.normal));
      this.blobShadow.scale.setScalar(Math.max(0.45, 1 - Math.max(0, this.renderPos.y - this.mario.floorHeight) / 1200));
    }
    this.character.update(this.mario, dt, this.renderPos, this.renderYaw, this.display.motion === 'smooth');
    this.playground.update(this.time);
    this.character.faceCamera?.(this.camera);
    this.playground.faceCamera?.(this.camera);
    this.particles.update(dt);

    this.sun.position.set(this.renderPos.x + 700, this.renderPos.y + 1200, this.renderPos.z + 500);
    this.sun.target.position.set(this.renderPos.x, this.renderPos.y, this.renderPos.z);
    this.sun.target.updateMatrixWorld();

    this.updateEnvironment(dt);
    const zone = this.worldName !== 'playground' ? COURSES[this.worldName].name.toUpperCase() : zoneFor(this.renderPos.x, this.renderPos.z);
    const requiredCoins = this.playground.requiredCoins ?? this.playground.coins.length;
    this.hud.update(this.mario, {
      coins: this.coinsCollected,
      totalCoins: requiredCoins,
      interaction: this.mario.heldObj
        ? 'B THROW · A JUMP'
        : this.mario.nearGrabbable
          ? 'B GRAB'
          : this.mario.nearBreakable
            ? 'B PUNCH · Z POUND'
            : null,
      time: this.elapsed,
      fps: this.fps,
      zone,
      starReady: this.starReady,
      stars: this.save.stars,
      totalStars: TOTAL_STARS,
    });

    this.renderer.render(this.scene, this.camera);
    requestAnimationFrame(this.animate);
  };

  private stepSimulation(): void {
    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.prevYaw = this.mario.faceAngle[1];

    if (this.enteringDoor) {
      const tick = ++this.mario.actionTimer;
      if (tick >= 16) this.canvas.style.clipPath = `circle(${Math.max(0, (36 - tick) * 5)}% at center)`;
      if (tick === 36) {
        void this.loadCourse(this.enteringDoor.destination, this.enteringDoor.destNode, this.mario.action === ACT_PUSHING_DOOR);
      }
      return;
    }
    const controller = this.input.consumeStep();
    for (const platform of this.playground.platforms ?? []) platform.step(this.mario);
    for (const enemy of this.playground.goombas ?? []) enemy.step(this.mario.pos, this.playground.collision);
    if (this.mario.invincTimer > 0) this.mario.invincTimer--;
    const objectEvents = this.playground.objects?.step(this.mario) ?? [];
    this.mario.step(controller, this.rig.yawS16);
    if (this.viewMode === 'side') {
      this.mario.pos[2] = this.sideLaneZ;
      this.mario.vel[2] = 0;
    }
    this.checkPaintingWarp();
    this.playground.objects?.afterMarioStep(this.mario);
    this.processObjectEvents(objectEvents);
    if ([ACT_WALKING, ACT_DECELERATING].includes(this.mario.action) && this.mario.floor) {
      this.stepDistance += Math.hypot(this.mario.pos[0] - this.prevPos.x, this.mario.pos[2] - this.prevPos.z);
      if (this.stepDistance >= 120) { this.sfx.footstep(this.mario.floor?.type ?? 0); this.stepDistance %= 120; }
    } else this.stepDistance = 0;

    if (!this.mario.events.some(event => event.type === 'respawn')) {
      for (const enemy of this.playground.goombas ?? []) {
        if (!enemy.model.visible) continue;
        if (!crossesGoomba(this.prevPos, this.mario.pos, enemy)) {
          const [x, y, z] = this.mario.pos;
          const foot = enemy.model.position;
          if (!(this.mario.action & ACT_FLAG_INVULNERABLE) && !this.mario.invincTimer && y < enemy.top && y + 160 > foot.y
              && Math.hypot(x - foot.x, z - foot.z) < enemy.radius + 37) {
            const angle = atan2s(foot.z - z, foot.x - x);
            const difference = Math.abs(toS16(angle - this.mario.faceAngle[1]));
            const strike = this.mario.flags & (MARIO_PUNCHING | MARIO_KICKING | MARIO_TRIPPING);
            if ((strike && difference <= 0x2aaa) || ((this.mario.action & ACT_FLAG_ATTACKING) && Math.abs(this.mario.forwardVel) >= 26)) {
              enemy.defeat();
              this.particles.sparks(foot, 8);
              continue;
            }
            const hurt = this.mario.takeDamage(1, [foot.x, foot.y, foot.z]);
            enemy.alert(x, z);
            if (hurt) {
              this.sfx.bonk();
              break;
            }
          }
          continue;
        }
        enemy.defeat();
        this.mario.pos[1] = enemy.top;
        this.mario.setAction(ACT_FREEFALL);
        this.mario.vel[1] = 30;
        this.sfx.jump();
        this.particles.dust(enemy.model.position, 8);
        break;
      }
    }
    this.processEvents(this.mario.events);
    this.collectCoins();
    this.checkStar();
    this.elapsed += STEP;
    if (!this.won && (this.mario.action === ACT_WALKING || this.mario.action === ACT_DECELERATING)) {
      for (const door of this.playground.doors ?? []) {
        const dx = door.pos[0] - this.mario.pos[0], dz = door.pos[2] - this.mario.pos[2];
        if (Math.hypot(dx, dz) >= 117 || this.mario.pos[1] >= door.pos[1] + 100
            || this.mario.pos[1] + 160 <= door.pos[1]) continue;
        const yaw = Math.round(door.yaw * 65536 / 360);
        const pull = Math.abs(toS16(yaw - atan2s(dz, dx))) <= 0x4000;
        this.enteringDoor = door;
        this.mario.setAction(pull ? ACT_PULLING_DOOR : ACT_PUSHING_DOOR, pull ? 5 : 6);
        this.mario.faceAngle[1] = yaw;
        this.mario.pos = [...door.pos];
        this.mario.forwardVel = 0;
        this.mario.vel = [0, 0, 0];
        door.play?.(pull ? 1 : 2);
        break;
      }
    }
  }

  private checkPaintingWarp(): void {
    if (this.enteringPainting || this.loadingCourse || this.won) return;
    const type = this.mario.floor?.type ?? -1;
    const destination = paintingDestination(type);
    if (!destination) return;
    this.enteringPainting = true;
    this.hud.showToast(`ENTERING PAINTING — ${destination.toUpperCase()}`, 1800);
    void this.loadCourse(destination, null, true).finally(() => {
      this.enteringPainting = false;
    });
  }

  private toggleEditor(): void {
    this.editorEnabled = !this.editorEnabled;
    this.editor.enabled = this.editorEnabled;
    byId('editor-hint').hidden = !this.editorEnabled;
    this.input.setPointerHandler(this.editorEnabled ? {
      down: event => this.editor.pointerDown(event),
      move: event => this.editor.pointerMove(event),
      up: () => this.editor.pointerUp(),
      wheel: delta => this.editor.wheel(delta),
    } : null);
    this.canvas.style.cursor = this.editorEnabled ? 'crosshair' : 'grab';
    this.hud.showToast(this.editorEnabled ? 'WORLD EDITOR ON' : 'WORLD EDITOR OFF', 1600);
  }

  private cycleViewMode(): void {
    const order: Array<'3d' | 'side' | 'top'> = ['3d', 'side', 'top'];
    this.setViewMode(order[(order.indexOf(this.viewMode) + 1) % order.length]);
  }

  private setViewMode(mode: '3d' | 'side' | 'top', announce = true): void {
    this.viewMode = mode;
    this.mario.movementMode = mode === 'side' ? 'side' : mode === 'top' ? 'topdown' : 'default';
    if (mode === 'side') this.sideLaneZ = this.mario.pos[2];
    this.camera.up.set(0, 1, 0);
    this.camera.fov = mode === 'side' ? 34 : mode === 'top' ? 50 : 55;
    this.camera.updateProjectionMatrix();
    byId('view-mode').textContent = mode === '3d' ? '3D' : mode === 'side' ? '2D SIDE' : 'TOP-DOWN';
    const url = new URL(location.href);
    if (mode === '3d') url.searchParams.delete('view'); else url.searchParams.set('view', mode);
    history.replaceState(null, '', url);
    if (announce) this.hud.showToast(`VIEW · ${mode === '3d' ? '3D' : mode === 'side' ? '2D SIDE' : 'TOP-DOWN'}`, 1400);
  }

  private updateViewCamera(dt: number): void {
    if (this.viewMode === '3d') {
      this.rig.update(dt, this.renderPos, this.renderYaw, Math.abs(this.mario.forwardVel) > 2);
      return;
    }
    if (this.viewMode === 'side') {
      this.camera.position.set(this.renderPos.x, this.renderPos.y + 220, this.sideLaneZ + 1100);
      this.camera.up.set(0, 1, 0);
      this.camera.lookAt(this.renderPos.x, this.renderPos.y + 90, this.sideLaneZ);
    } else {
      this.camera.position.set(this.renderPos.x, this.renderPos.y + 1500, this.renderPos.z + 1);
      this.camera.up.set(0, 0, -1);
      this.camera.lookAt(this.renderPos.x, this.renderPos.y, this.renderPos.z);
    }
  }

  private updateEnvironment(dt: number): void {
    const water = this.playground.collision.waterLevel(this.mario.pos[0], this.mario.pos[2]);
    const underwater = water !== null && this.mario.pos[1] < water - 40;
    if (underwater !== this.underwater) {
      this.underwater = underwater;
      this.particles.burst(this.mario.pos, {
        color: 0x7fd5ff,
        count: 18,
        speed: 90,
        life: 0.7,
        gravity: 120,
        size: 7,
        upward: 80,
      });
      this.sfx.splash();
    }
    if (underwater) {
      this.scene.fog = new THREE.Fog(0x1d5878, 120, 9000);
      if (!(this.scene.background instanceof THREE.Texture)) this.scene.background = new THREE.Color(0x1d5878);
      this.lastSwimSound += dt;
      if (this.mario.forwardVel > 3 && this.lastSwimSound > 0.7) {
        this.lastSwimSound = 0;
        this.sfx.swim();
      }
    } else {
      this.scene.fog = this.baseFog;
      this.scene.background = this.baseBackground;
    }
  }

  private processObjectEvents(events: ObjectEvent[]): void {
    for (const event of events) {
      switch (event.type) {
        case 'explosion':
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 18);
          this.sfx.explosion();
          this.shake = Math.max(this.shake, 14);
          break;
        case 'break':
          this.particles.dust(event.pos, 16);
          this.sfx.breakBox();
          this.shake = Math.max(this.shake, 5);
          break;
        case 'hurt':
          this.sfx.bonk();
          this.shake = Math.max(this.shake, 10);
          break;
        case 'boss':
          this.starReady = true;
          this.hud.showToast('★ KING BOB-OMB DEFEATED', 3600);
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 30);
          this.sfx.star();
          this.shake = Math.max(this.shake, 12);
          break;
        case 'dialogue':
          this.hud.showToast(event.text, 3600);
          break;
        case 'warp':
          if (!this.loadingCourse && !this.enteringPainting && Object.hasOwn(COURSES, event.destination)) {
            this.enteringPainting = true;
            this.hud.showToast('WARPING…', 1200);
            void this.loadCourse(event.destination as CourseId, null, true).finally(() => {
              this.enteringPainting = false;
            });
          }
          break;
        case 'race':
          if (event.state === 'start') {
            this.hud.showToast('RACE KOOPA THE QUICK!', 1800);
          } else if (event.state === 'won') {
            this.starReady = true;
            this.hud.showToast('★ RACE WON!', 3200);
            this.sfx.star();
          } else {
            this.hud.showToast('KOOPA WON — TRY AGAIN', 2200);
            this.sfx.bonk();
          }
          break;
        case 'coin':
          break;
      }
    }
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
          this.sfx.groundPoundStart();
          this.particles.dust(feet, 3);
          break;

        case 'groundpoundland':
          this.sfx.groundPound();
          this.particles.pound(feet);
          this.shake = Math.max(this.shake, 16);
          break;

        case 'longjump':
          this.sfx.longJump();
          this.particles.dust(feet, 4);
          break;

        case 'dive':
          this.sfx.dive();
          break;

        case 'hurt':
          this.sfx.bonk();
          this.shake = Math.max(this.shake, 6);
          break;
        case 'death':
          this.sfx.bonk();
          this.hud.showToast('MARIO DIED', 1800);
          this.shake = Math.max(this.shake, 10);
          break;
        case 'respawn':
          this.particles.dust(feet, 12);
          this.hud.showToast(this.worldName !== 'playground' ? COURSES[this.worldName].name.toUpperCase() : 'BACK TO THE PLAZA', 1400);
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
      this.save.recordCoins(this.worldName, this.coinsCollected);
      this.mario.healCounter += 4;
      this.sfx.coin();
      this.particles.sparks(coin.position, 10);

      const required = this.playground.requiredCoins ?? this.playground.coins.length;
      if (!this.starReady && this.coinsCollected >= required) {
        this.starReady = true;
        this.hud.showToast('★ REACH THE STAR ★', 4200);
        this.sfx.star();
        this.shake = Math.max(this.shake, 6);
      } else {
        this.hud.showToast(`COIN ${Math.min(this.coinsCollected, required)} / ${required}`, 700);
      }
    }
  }

  private checkStar(): void {
    if (this.won || !this.playground.star.visible) return;
    const star = this.playground.star.position;
    const dx = this.mario.pos[0] - star.x;
    const dy = this.mario.pos[1] + 60 - star.y;
    const dz = this.mario.pos[2] - star.z;
    if (dx * dx + dy * dy + dz * dz > 130 * 130) return;

    if (!this.starReady) {
      if (this.elapsed - this.lastStarHint > 3) {
        this.lastStarHint = this.elapsed;
        const required = this.playground.requiredCoins ?? this.playground.coins.length;
        this.hud.showToast(
          `THE STAR IS ASLEEP — ${Math.min(this.coinsCollected, required)}/${required} COINS`,
          2200,
        );
      }
      return;
    }

    this.won = true;
    this.starReady = false;
    this.save.collectStar(this.worldName);
    this.sfx.star();
    this.particles.pound(star);
    this.particles.sparks(star, 40);
    this.shake = Math.max(this.shake, 10);
    this.winText.textContent = `You collected ${this.coinsCollected} coins and reached the star in ${formatTime(
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
      display: { ...this.display, width: this.canvas.width, height: this.canvas.height },
      paused: this.settings.open,
      respawns: this.mario.respawns,
      started: this.started,
      won: this.won,
      coins: this.coinsCollected,
      action: this.mario.action,
      actionName: actionName(this.mario.action),
      pos: [...this.mario.pos],
      vel: [...this.mario.vel],
      forwardVel: this.mario.forwardVel,
      health: this.mario.health,
      floorHeight: this.mario.floorHeight,
      starReady: this.starReady,
      cameraYaw: this.rig.yawS16,
      cameraMode: this.rig.mode,
      cameraContext: this.rig.context,
      viewMode: this.viewMode,
      editorEnabled: this.editorEnabled,
      editorBlocks: this.editor.debug(),
      underwater: this.underwater,
      cameraPos: [this.camera.position.x, this.camera.position.y, this.camera.position.z],
      faceYaw: this.mario.faceAngle[1],
      floorNormal: this.mario.floor ? [...this.mario.floor.normal] : null,
      floorType: this.mario.floor?.type ?? -1,
      input: this.mario.input,
      prevAction: this.mario.prevAction,
      doubleJumpTimer: this.mario.doubleJumpTimer,
      transitions: this.mario.transitions,
      world: this.worldName,
      act: this.act,
      collisionTriangles: this.playground.collision.surfaces.length,
      character: this.character === this.procedural ? 'procedural' : this.character instanceof Sm64MarioView ? 'sm64' : 'gltf',
      characterLoaded: this.loadedCharacter !== null,
      characterClips: this.loadedCharacter?.clipNames.length ?? 0,
      characterClip: this.character === this.loadedCharacter ? (this.loadedCharacter?.activeClip ?? null) : null,
      characterAnimation: this.character instanceof Sm64MarioView ? this.character.animationDebug : null,
      characterRotation: { yaw: this.character.root.rotation.y, pitch: this.character.root.rotation.x },
      pose: this.procedural.poseDebug,
      goombas: (this.playground.goombas ?? []).map(({ model, top }) => ({
        x: model.position.x, top, z: model.position.z, alive: model.visible,
      })),
      coinPositions: this.playground.coins.map((c) => ({ x: c.position.x, y: c.position.y, z: c.position.z, collected: c.collected })),
      starPos: [this.playground.star.position.x, this.playground.star.position.y, this.playground.star.position.z],
      held: this.mario.heldObj ? {
        id: this.mario.heldObj.id,
        behavior: this.mario.heldObj.behavior,
        heldState: this.mario.heldObj.heldState,
      } : null,
      objects: (this.playground.objects?.manager.objects ?? []).map(object => ({
        id: object.id,
        behavior: object.behavior,
        pos: [...object.pos],
        active: object.active,
        heldState: object.heldState,
        action: object.action,
        grounded: object.grounded,
        links: object.links?.length ?? 0,
        vars: { ...object.vars },
        render: this.playground.objects?.debugView(object.id) ?? null,
      })),
    };
  }
}
