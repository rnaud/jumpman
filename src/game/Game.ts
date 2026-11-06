import { KoopaRace, KOOPA_RACE_STAR } from '../sim/behaviors/koopaTheQuick';
/**
 * Game shell: Three.js rendering, a fixed 30 Hz Mario simulation, camera,
 * coins, the star, particles and synthesized audio.
 */

import * as THREE from 'three';
import { GrandStarCamera } from './castleEnding';
import { GrandStar, SpawnedStar } from '../sim/spawnedStar';
import { MarioRuntime } from '../sim/MarioRuntime';
import { atan2s, toS16, type Vec3 } from '../sim/math';
import {
  ACT_UNLOCKING_KEY_DOOR, ACT_SPECIAL_EXIT_AIRBORNE, ACT_PULLING_DOOR, ACT_PUSHING_DOOR, ACT_WALKING, ACT_DECELERATING,
  ACT_IN_CANNON, ACT_FLAG_AIR, ACT_FLAG_SWIMMING, ACT_FLAG_METAL_WATER, ACT_FLAG_INTANGIBLE, ACT_FLAG_INVULNERABLE, ACT_FLAG_ATTACKING, MARIO_PUNCHING, MARIO_KICKING, MARIO_TRIPPING,
  ACT_EXIT_AIRBORNE, ACT_FALLING_EXIT_AIRBORNE, ACT_EXIT_LAND_SAVE_DIALOG,
  ACT_JUMBO_STAR_CUTSCENE, ACT_END_PEACH_CUTSCENE, ACT_BACKFLIP,
  ACT_DOUBLE_JUMP,
  ACT_FREEFALL,
  ACT_FLYING,
  A_BUTTON, B_BUTTON,
  ACT_JUMP_KICK,
  ACT_TRIPLE_JUMP,
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
import { prepareMushroom2D } from './mushroom2d';
import type { CharacterView } from './CharacterView';
import { crossesGoomba } from './goomba';
import { marioHitbox } from '../sim/object';
import type { ObjectEvent } from '../sim/objectManager';
import { CameraRig } from './CameraRig';
import { InputManager } from './Input';
import { Particles } from './Particles';
import { Footsteps } from './footsteps';
import { Sfx } from './Sfx';
import { courseMusic, type MusicTrack } from './music';
import { HUD, formatTime } from './HUD';
import { CLASSIC, MODERN, readDisplaySettings, displaySize, type DisplaySettings } from './displaySettings';
import { COURSES, missionsFor, type CourseId } from './courses';
import { activeSaveSlot, SaveFile, TOTAL_STARS } from './save';
import { paintingDestination } from './painting';
import { RuntimeEditor } from './editor';
import { findSidePlacement } from './viewTransitions';
import { Diorama } from './diorama';
import { DreamWorld } from './dream';
import { HundredMarios } from './hundredMarios';
import { addWaterSurfaces, type WaterSurface } from './waterSurfaces';

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
  private readonly birds = [
    new THREE.Vector3(-7430,1500,873), new THREE.Vector3(-80,1500,5004), new THREE.Vector3(7131,1500,-2989),
  ];
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

  private initialArea = 1;
  private readonly areaRewards = new Map<PlaygroundResult, {
    missionComplete: boolean; starReady: boolean; coinStarFlight: SpawnedStar | null;
  }>();
  private courseScene = new THREE.Scene();
  private skyScene = new THREE.Scene();
  private loadingCourse = false;
  private readonly footsteps = new Footsteps();
  private started = false;
  private enteringDoor?: CastleDoor;
  private enteringPainting = false;
  private viewMode: '3d' | 'side' | 'top' = '3d';
  private readonly editor: RuntimeEditor;
  private readonly dream: DreamWorld;
  private readonly hundred: HundredMarios;
  private readonly diorama: Diorama;
  private waterSurfaces: WaterSurface[] = [];
  private editorEnabled = false;
  private sideLaneZ = 0;
  private readonly sideCameraRaycaster = new THREE.Raycaster();
  private underwater = false;
  private baseFog: THREE.Fog | null = null;
  private baseBackground: THREE.Color | THREE.Texture | null = null;
  private hudCoins = 0;
  private hudTick = 0;
  private won = false;
  private saveSlot = activeSaveSlot();
  private save = new SaveFile(this.saveSlot);
  private starVisibleByCourse = true;
  private secrets = new Set<number>();
  private secretTimer = -1;
  private objectStar?: {act:number;pos:Vec3;origin:Vec3};
  private spawnCamera: {focus: THREE.Vector3; original: THREE.Vector3; ticks: number} | null = null;
  private secretCamera: { position: THREE.Vector3; focus: THREE.Vector3; fov: number } | null = null;
  private boxFlights = new Map<number, SpawnedStar>();
  private drainingShip = false;
  private floorWarpTimer = 0;
  private lookPitch = 0;
  private lookYaw = 0;
  private towerFade = 0;
  private endingCamera: GrandStarCamera | null = null;
  private grandStar: GrandStar | null = null;
  private keyFlight: { velocity: number; landed: boolean } | null = null;
  private coinStarFlight: SpawnedStar | null = null;
  private collectedNoExitStar = false;
  private starReady = false;
  private missionComplete = false;
  private coinsCollected = 0;
  private redCoinsCollected = 0;
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
    this.sun.position.set(4900, 8400, 3500);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048);
    this.sun.shadow.camera.near = 1;
    this.sun.shadow.camera.far = 20000;
    this.sun.shadow.camera.left = -4400;
    this.sun.shadow.camera.right = 4400;
    this.sun.shadow.camera.top = 4400;
    this.sun.shadow.camera.bottom = -4400;
    this.sun.shadow.bias = -0.0006;
    this.sun.shadow.normalBias = 2.5;
    this.scene.add(this.sun);
    this.scene.add(this.sun.target);

    const fill = new THREE.DirectionalLight(0x9ec8ff, 0.55);
    fill.position.set(-900, 500, -600);
    this.scene.add(fill);

    this.playground = buildWorld(this.courseScene);
    this.waterSurfaces = addWaterSurfaces(this.courseScene, this.playground.collision.waterBoxes);
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
    this.editor.onSpawnActor = (type, position) => this.spawnEditorActor(type, position);
    this.editor.actorProvider = () => this.editorActorTargets();
    const seed = Number(new URLSearchParams(window.location.search).get('dreamSeed') ?? 1) || 1;
    this.dream = new DreamWorld(this.scene, this.playground.collision, seed);
    this.hundred = new HundredMarios(this.scene, this.playground.collision);
    this.diorama = new Diorama(this.scene);
    this.diorama.onNotify = message => this.hud.showToast(message, 1600);

    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.mario.faceAngle[1];
    this.renderYaw = this.prevYaw;

    this.initialArea = this.playground.area ?? 1;
    this.starVisibleByCourse = this.playground.star.visible;
    this.resetMissionStar();
    this.bindUI();
    this.canvas.addEventListener('pointerdown', this.onInspectorPointerDown);
    this.bindSettings();
    this.populateMissions();
    this.populateSaveSlots();
    const experimentParams = new URLSearchParams(window.location.search);
    const initialView = experimentParams.get('view');
    if (initialView === 'side' || initialView === 'top') this.setViewMode(initialView, false);
    else if (worldName === 'mushroom2d') this.setViewMode('side', false);
    const experiment = experimentParams.get('experiment');
    if (experiment === 'dimensions' || experiment === 'god' || experiment === 'dream' || experiment === 'hundred' || experiment === 'diorama') {
      this.startExperiment(experiment);
    }
    window.addEventListener('resize', this.onResize);

    this.seedMotes();

    this.lastFrame = performance.now();
    requestAnimationFrame(this.animate);
  }

  /** Replace only course state; keep the renderer, input, character and audio alive. */
  async loadCourse(id: CourseId, entry?: string | null, push = false, historyMode: 'push' | 'none' = 'push', act = 1, area = 1): Promise<void> {
    if (this.loadingCourse) return;
    this.loadingCourse = true;
    this.canvas.style.opacity = '';
    this.canvas.style.clipPath = '';
    this.settings.close();
    this.input.clear();
    this.sfx.ensure();
    this.hud.showToast(`LOADING ${COURSES[id].name.toUpperCase()}…`, 60000);
    const next = new THREE.Scene();
    try {
      const factory = id === 'playground'
        ? await preparePlayground()
        : id === 'mushroom2d'
          ? await prepareMushroom2D()
          : await prepareCourse(id, entry, push, act, area);
      const world = factory(next);
      const mario = new MarioRuntime(world.collision, world.spawn, world.deathHeight);
      const rig = new CameraRig(this.camera, world.collisionMeshes, id !== 'playground');
      rig.setFloorProbe((x, y, z) => {
        const hit = world.collision.findFloor(x, y, z);
        return hit.surface ? hit.height : null;
      });
      this.playground.linkedArea?.world.dispose?.();
      this.playground.dispose?.();
      this.areaRewards.clear();
      if (this.worldName === 'playground') this.disposeGeneratedScene(this.courseScene);
      this.disposeWaterSurfaces();
      this.scene.remove(this.courseScene);
      this.courseScene = next;
      this.courseScene.visible = !this.dream.enabled;
      this.scene.add(next);
      this.playground = world;
      this.waterSurfaces = addWaterSurfaces(next, world.collision.waterBoxes);
      this.editor.setWorld(world.collision, world.collisionMeshes, `jumpman.editor.v1.${id}`);
      this.dream.setWorld(world.collision);
      this.hundred.setWorld(world.collision);
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
      this.initialArea = world.area ?? 1;
      this.starVisibleByCourse = world.star.visible;
      this.restart();
      this.start();
      if (id === 'mushroom2d') this.setViewMode('side', false);
      else if (this.viewMode !== '3d') this.setViewMode(this.viewMode, false);
      this.updateExperimentBadge();
      const url = new URL(location.href);
      url.searchParams.set('level', id);
      if ((world.area ?? 1) > 1) url.searchParams.set('area', String(world.area)); else url.searchParams.delete('area');
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
      if (this.started) this.sfx.startMusic(this.musicForCourse());
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

  private changeLinkedArea(area: number, delta: Vec3): void {
    const old = this.playground;
    const linked = old.linkedArea;
    if (!linked || linked.world.area !== area) return;
    const world = linked.world;
    if (world.objects?.manager.penguinRace) world.objects.manager.resetRacer();
    this.areaRewards.set(old, { missionComplete: this.missionComplete, starReady: this.starReady, coinStarFlight: this.coinStarFlight });
    this.disposeWaterSurfaces();
    this.scene.remove(this.courseScene);
    this.courseScene = linked.scene;
    this.scene.add(this.courseScene);
    this.playground = world;
    this.waterSurfaces = addWaterSurfaces(this.courseScene, world.collision.waterBoxes);
    this.editor.setWorld(world.collision, world.collisionMeshes, `jumpman.editor.v1.${this.worldName}.area${area}`);
    this.dream.setWorld(world.collision);
    this.hundred.setWorld(world.collision);
    this.mario.changeArea(world.collision, world.spawn, world.deathHeight ?? -1600, delta);
    this.prevPos.fromArray(this.mario.pos);
    this.rig.displace(...delta);
    this.rig.setBlockers(world.collisionMeshes, true);
    this.rig.setFloorProbe((x, y, z) => {
      const floor = world.collision.findFloor(x, y, z);
      return floor.surface ? floor.height : null;
    });
    this.starVisibleByCourse = world.hasCourseStar ?? world.star.visible;
    if (this.started) this.sfx.startMusic(this.musicForCourse());
    const prior = this.areaRewards.get(world);
    if (prior) {
      this.missionComplete = prior.missionComplete;
      this.starReady = prior.starReady;
      this.coinStarFlight = prior.coinStarFlight;
    } else this.resetMissionStar();
  }

  private disposeWaterSurfaces(): void {
    for (const surface of this.waterSurfaces) surface.dispose();
    this.waterSurfaces = [];
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
    document.querySelectorAll<HTMLButtonElement>('[data-experiment]').forEach(button => {
      button.addEventListener('click', event => {
        event.stopPropagation();
        this.startExperiment(button.dataset.experiment ?? '');
      });
    });
    byId('again-button').addEventListener('click', (event) => {
      event.stopPropagation();
      this.restart();
    });
    byId('castle-button').addEventListener('click', (event) => {
      event.stopPropagation();
      this.winOverlay.hidden = true;
      const entry = this.playground.successEntry;
      void this.loadCourse(entry ? 'castle_inside' : 'castle_grounds', entry, false, 'push', 1, this.playground.successArea);
    });
    byId('reset-save').addEventListener('click', () => {
      this.save.reset();
      this.restart();
      this.populateSaveSlots();
    const experimentParams = new URLSearchParams(window.location.search);
    const initialView = experimentParams.get('view');
    if (initialView === 'side' || initialView === 'top') this.setViewMode(initialView, false);
    else if (this.worldName === 'mushroom2d') this.setViewMode('side', false);
    const experiment = experimentParams.get('experiment');
    if (experiment === 'dimensions' || experiment === 'god' || experiment === 'dream' || experiment === 'hundred' || experiment === 'diorama') {
      this.startExperiment(experiment);
    }
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
      if (event.code === 'KeyR') {
        if (this.diorama.enabled) {
          this.diorama.resetCamera();
          this.hud.showToast('DIORAMA CAMERA RESET', 1200);
          return;
        }
        this.restart();
      }
      if (event.code === 'F3' || event.code === 'Backquote') this.hud.toggleDebug();
      if (event.code === 'KeyC') {
        const mode = this.rig.cycleMode();
        this.hud.showToast(`CAMERA · ${mode.toUpperCase()}`, 1400);
      }
      if (event.code === 'KeyX') this.rig.snapBehind(this.mario.faceAngle[1]);
      if (event.code === 'KeyM') this.cycleViewMode();
      if (event.code === 'KeyE') this.toggleEditor();
      if (event.code === 'KeyH') {
        const enabled = this.hundred.toggle(this.mario.pos);
        this.updateExperimentBadge();
        this.updateAgentInspector();
        this.hud.showToast(enabled ? 'A HUNDRED MARIOS' : 'MARIOS DISPERSED', 1800);
      }
      if (event.code === 'KeyO' && this.hundred.enabled && !this.editorEnabled) {
        this.hundred.spawn(this.mario.pos, 100);
        this.updateExperimentBadge();
        this.updateAgentInspector();
        this.hud.showToast('100 MARIOS · FLOOD', 1800);
      }
      if (event.code === 'BracketLeft' || event.code === 'BracketRight') {
        if (this.hundred.enabled && !this.editorEnabled) {
          event.preventDefault();
          const delta = event.code === 'BracketRight' ? 0.25 : -0.25;
          this.hundred.speedScale = Math.max(0.25, Math.min(3, this.hundred.speedScale + delta));
          this.updateExperimentBadge();
          this.hud.showToast(`MARIOS SPEED ×${this.hundred.speedScale.toFixed(2)}`, 1200);
          return;
        }
      }
      if (event.code === 'KeyY') {
        this.mario.grantWingCap();
        this.updateCapBadge();
        this.hud.showToast('WING CAP · 30 SECONDS', 1800);
      }
      if (event.code === 'KeyU') {
        this.mario.grantMetalCap();
        this.updateCapBadge();
        this.hud.showToast('METAL CAP · 30 SECONDS', 1800);
      }
      if (event.code === 'KeyI') {
        this.mario.grantVanishCap();
        this.updateCapBadge();
        this.hud.showToast('VANISH CAP · 30 SECONDS', 1800);
      }
      if (event.code === 'KeyP' && this.hundred.enabled) {
        const personality = this.hundred.cyclePersonality();
        if (personality) {
          this.updateAgentInspector();
          this.hud.showToast(`MARIO PERSONALITY · ${personality.toUpperCase()}`, 1400);
        }
      }
      if (event.code === 'KeyN') this.toggleDream();
      if (event.code === 'KeyV') {
        if (this.diorama.enabled) this.disableDiorama();
        else this.startExperiment('diorama');
      }
      if (this.editorEnabled && (event.code === 'BracketLeft' || event.code === 'BracketRight')) {
        event.preventDefault();
        const delta = event.code === 'BracketRight' ? 0.25 : -0.25;
        this.mario.gravityScale = Math.max(0.25, Math.min(2.5, this.mario.gravityScale + delta));
        this.hud.showToast(`GRAVITY ×${this.mario.gravityScale.toFixed(2)}`, 1400);
        return;
      }
      if (this.editorEnabled && (event.code === 'Digit1' || event.code === 'Digit2' || event.code === 'Digit3' || event.code === 'Digit4' || event.code === 'Digit5')) {
        event.preventDefault();
        this.editor.selectedType = event.code === 'Digit2' ? 'ramp' : event.code === 'Digit3' ? 'bounce' : event.code === 'Digit4' ? 'bobomb' : event.code === 'Digit5' ? 'coin' : 'platform';
        this.hud.showToast(`SPAWN · ${this.editor.selectedType.toUpperCase()}`, 1200);
        return;
      }
      if (this.editorEnabled && event.code === 'KeyR') {
        event.preventDefault();
        this.editor.reset();
        this.hud.showToast('EDITOR WORLD RESET', 1600);
        return;
      }
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
    this.sfx.startMusic(this.musicForCourse());
    this.accumulator = 0;
    this.lastFrame = performance.now();
    this.elapsed = 0;
    this.hud.showToast('GO!  A = JUMP · HOLD FOR HEIGHT', 2600);
  }

  private restart(): void {
    this.enteringDoor = undefined;
    this.canvas.style.opacity = '';
    this.canvas.style.clipPath = '';
    if (this.playground.linkedArea) {
      if (this.playground.area !== this.initialArea) this.changeLinkedArea(this.initialArea, [0, 0, 0]);
      this.areaRewards.clear();
      const other = this.playground.linkedArea.world;
      other.updateSaveGeometry?.(this.save);
      other.reset?.();
      for (const coin of [...other.coins, ...(other.redCoins ?? [])]) {
        coin.collected = false;
        coin.group.visible = true;
      }
    }
    this.playground.updateSaveGeometry?.(this.save);
    this.playground.reset?.();
    for (const platform of this.playground.platforms ?? []) platform.reset(this.playground.collision);
    this.mario.resetToSpawn();
    if (this.viewMode === 'side') this.sideLaneZ = this.mario.pos[2];
    for (const enemy of this.playground.goombas ?? []) enemy.reset();
    for (const coin of this.playground.coins) {
      coin.collected = false;
      coin.group.visible = true;
    }
    for (const coin of this.playground.redCoins ?? []) {
      coin.collected = false;
      coin.group.visible = true;
    }
    this.redCoinsCollected = 0;
    this.footsteps.reset();
    this.coinsCollected = this.hudCoins = 0;
    this.resetMissionStar();
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

  private resetMissionStar(): void {
    const world = this.playground;
    this.keyFlight = null;
    this.grandStar = null;
    this.endingCamera = null;
    this.drainingShip = false;
    this.lookPitch = this.lookYaw = this.towerFade = 0;
    this.floorWarpTimer = 0;
    this.secrets.clear();
    this.secretTimer = -1;
    this.objectStar = undefined;
    this.restoreSecretCamera();
    this.spawnCamera = null;
    this.boxFlights.clear();
    this.coinStarFlight = null;
    if (world.hundredCoinStar) {
      world.hundredCoinStar.group.visible = false;
      world.hundredCoinStar.setCollectedModel(this.save.hasStar(this.worldName, 7));
    }
    this.collectedNoExitStar = false;
    const collected = this.save.hasStar(this.worldName, world.primaryStarAct ?? this.act);
    for (const reward of world.sideStars ?? []) {
      reward.group.visible = !reward.snowman && !reward.penguin && !reward.secret && !reward.redCoins && !reward.chest && !reward.box;
      reward.setCollectedModel(this.save.hasStar(this.worldName, reward.act));
    }
    this.missionComplete = !world.starRequiresEvent;
    world.setCollectedStarModel?.(collected);
    world.star.visible = this.starVisibleByCourse && this.missionComplete
      && (world.setCollectedStarModel !== undefined || !collected);
    world.starLight.visible = world.star.visible;
    this.starReady = world.requireCoins === false && world.star.visible;
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
    const experimentParams = new URLSearchParams(window.location.search);
    const initialView = experimentParams.get('view');
    if (initialView === 'side' || initialView === 'top') this.setViewMode(initialView, false);
    else if (this.worldName === 'mushroom2d') this.setViewMode('side', false);
    const experiment = experimentParams.get('experiment');
    if (experiment === 'dimensions' || experiment === 'god' || experiment === 'dream' || experiment === 'hundred' || experiment === 'diorama') {
      this.startExperiment(experiment);
    }
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
    if (this.diorama.enabled) {
      this.diorama.update(dt, this.input, this.camera);
      this.renderer.render(this.scene, this.camera);
      requestAnimationFrame(this.animate);
      return;
    }

    document.body.classList.toggle('cannon-aim',this.mario.action === ACT_IN_CANNON && this.mario.actionState === 2);
    document.body.classList.toggle('castle-ending', this.mario.action === ACT_END_PEACH_CUTSCENE || this.mario.action === ACT_JUMBO_STAR_CUTSCENE);
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

    if (this.mario.action === ACT_SPECIAL_EXIT_AIRBORNE || this.mario.action === ACT_EXIT_AIRBORNE || this.mario.action === ACT_FALLING_EXIT_AIRBORNE
        || this.mario.action === ACT_EXIT_LAND_SAVE_DIALOG) this.renderYaw = toS16(this.renderYaw + 0x8000);

    const cam = this.input.consumeCameraInput();
    if (cam.dx !== 0 || cam.dy !== 0) this.rig.rotate(cam.dx, cam.dy);
    if (cam.orbitX !== 0 || cam.orbitY !== 0) {
      this.rig.orbit(cam.orbitX * dt * 1.8, cam.orbitY * dt * 1.1);
    }
    if (cam.wheel !== 0) this.rig.zoom(cam.wheel);

    if (!this.spawnCamera && !this.secretCamera && !this.endingCamera && this.mario.action !== ACT_END_PEACH_CUTSCENE) this.updateViewCamera(dt);

    if (this.shake > 0.01) {
      this.camera.position.x += (Math.random() - 0.5) * this.shake;
      this.camera.position.y += (Math.random() - 0.5) * this.shake;
      this.camera.position.z += (Math.random() - 0.5) * this.shake;
      this.shake *= Math.exp(-7 * dt);
    } else {
      this.shake = 0;
    }

    this.blobShadow.visible = this.display.look === 'classic' && this.mario.floor !== null && this.mario.action !== ACT_IN_CANNON;
    if (this.mario.floor) {
      this.blobShadow.position.set(this.renderPos.x, this.mario.floorHeight + 2, this.renderPos.z);
      this.blobShadow.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), new THREE.Vector3(...this.mario.floor.normal));
      this.blobShadow.scale.setScalar(Math.max(0.45, 1 - Math.max(0, this.renderPos.y - this.mario.floorHeight) / 1200));
    }
    this.character.root.visible = !this.lookPitch && this.mario.action !== ACT_IN_CANNON;
    this.character.update(this.mario, dt, this.renderPos, this.renderYaw, this.display.motion === 'smooth');
    this.playground.selectRoom?.(this.mario.pos);
    this.playground.update(this.time);
    for (const surface of this.waterSurfaces) surface.update(this.time);
    this.character.faceCamera?.(this.camera);
    this.playground.faceCamera?.(this.camera);
    this.particles.update(dt);

    this.sun.position.set(this.renderPos.x + 4900, this.renderPos.y + 8400, this.renderPos.z + 3500);
    this.sun.target.position.set(this.renderPos.x, this.renderPos.y, this.renderPos.z);
    this.sun.target.updateMatrixWorld();

    this.updateEnvironment();
    const zone = this.worldName !== 'playground' ? COURSES[this.worldName].name.toUpperCase() : zoneFor(this.renderPos.x, this.renderPos.z);
    const requiredCoins = this.playground.requiredCoins ?? this.playground.coins.length;
    this.hud.update(this.mario, {
      coins: this.hudCoins,
      totalCoins: this.playground.requireCoins === false ? undefined : requiredCoins,
      interaction: this.worldName === 'castle_inside' && this.mario.floor?.type === 0x2f && this.save.stars >= 10
        ? 'HOLD ↑ TO LOOK UP · ← → TURN'
        : this.mario.action === ACT_IN_CANNON && this.mario.actionState === 2
        ? 'WASD AIM · SPACE FIRE'
        : this.playground.objects?.manager.findByBehavior('koopaTheQuick')?.action === KoopaRace.ResultDialog ? 'A CONTINUE'
        : (this.playground.objects?.manager.findByBehavior('racingPenguin')?.action===1 || this.playground.objects?.manager.findByBehavior('koopaTheQuick')?.action === KoopaRace.Invitation) ? 'A ACCEPT · B DECLINE'
        : this.cannonBuddy() ? 'B TALK' : this.mario.heldObj
        ? this.mario.heldObj.behavior === 'bowser' ? 'ROTATE STICK OR WASD · B THROW' : 'B THROW · A JUMP'
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

  private cannonBuddy() {
    return this.playground.cannons && this.playground.objects?.manager.objects.find(o => o.behavior === 'bobombBuddy' && o.param === 1
          && o.active && o.distanceToMario < 400 && Math.abs(toS16(o.angleToMario+0x8000-this.mario.faceAngle[1])) < 0x4000);
  }

  private stepSimulation(): void {
    this.prevPos.set(this.mario.pos[0], this.mario.pos[1], this.mario.pos[2]);
    this.prevYaw = this.mario.faceAngle[1];
    this.sfx.setPlayer(this.mario, new THREE.Vector3(...this.mario.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(), this.worldName === 'lll');
    if (this.stepSpawnedStar() || this.stepSecretStar()) return;
    this.sfx.setMusicLocation(this.playground.area ?? 1, this.mario.floor?.room ?? 0, this.mario.pos, this.save.stars);
    if (this.worldName === 'castle_grounds' && this.mario.action !== ACT_END_PEACH_CUTSCENE) {
      this.sfx.setOutdoorSources(this.birds.map(pos => pos.clone().applyMatrix4(this.camera.matrixWorldInverse).toArray()));
    }

    if (this.mario.action === ACT_JUMBO_STAR_CUTSCENE || this.mario.action === ACT_END_PEACH_CUTSCENE) {
      this.mario.step(this.input.consumeStep(), this.rig.yawS16);
      if (this.mario.action === ACT_JUMBO_STAR_CUTSCENE && this.mario.actionArg > 0) {
        this.endingCamera ??= new GrandStarCamera();
        this.endingCamera.step(this.camera, this.mario);
      }
      const {action, actionArg, actionTimer, actionState} = this.mario;
      if (action === ACT_JUMBO_STAR_CUTSCENE && actionArg === 0 && actionState === 1 && actionTimer === 0) {
        this.sfx.startMusic({ ...this.musicForCourse(), sequence: 0x1f, name: 'victory' });
      }
      if (action === ACT_END_PEACH_CUTSCENE && (actionArg === 2 && actionTimer === 90 || actionArg === 9 && actionTimer === 140)) {
        const credits = actionArg === 9;
        this.sfx.startMusic({ ...this.musicForCourse(), sequence: credits ? 0x1a : 0x20,
          preset: 7, name: credits ? 'credits' : 'ending' });
      }
      const line = this.playground.stepEnding?.(this.mario, this.camera);
      if (line) {
        this.hud.showToast(line.text, line.duration * 1000 / 30);
        if (line.voice) this.sfx.peachVoice(line.voice);
      }
      if (this.mario.action === ACT_JUMBO_STAR_CUTSCENE && this.mario.actionArg === 2 && this.mario.actionTimer >= 500) {
        this.canvas.style.opacity = String(Math.max(0, (530 - this.mario.actionTimer) / 30));
      }
      if (this.mario.action === ACT_END_PEACH_CUTSCENE && this.mario.actionArg === 0) {
        this.canvas.style.opacity = String(Math.min(1, this.mario.actionTimer / 20));
      }
      this.processEvents(this.mario.events);
      return;
    }
    if (this.enteringDoor) {
      const tick = ++this.mario.actionTimer;
      if (this.mario.action === ACT_UNLOCKING_KEY_DOOR) {
        if (tick >= 147) { // MARIO_ANIM_UNLOCK_DOOR
          const door = this.enteringDoor;
          this.save.unlockDoor(door.requiredKey!);
          const push = Boolean(this.mario.actionArg & 2);
          this.mario.setAction(push ? ACT_PUSHING_DOOR : ACT_PULLING_DOOR, push ? 6 : 5);
          this.mario.pos = [...door.pos];
          door.play?.(push ? 2 : 1);
        }
        return;
      }
      if (tick >= 16) this.canvas.style.clipPath = `circle(${Math.max(0, (36 - tick) * 5)}% at center)`;
      if (tick === 36) {
        const door = this.enteringDoor;
        if (door.destination === this.worldName && door.destArea === this.playground.linkedArea?.world.area) {
          const push = this.mario.action === ACT_PUSHING_DOOR;
          this.enteringDoor = undefined;
          this.canvas.style.clipPath = '';
          this.changeLinkedArea(door.destArea,[0,0,0]);
          const arrival = this.playground.doors!.find(d=>d.node===door.destNode)!;
          const angle = arrival.yaw*Math.PI/180 + (push ? Math.PI : 0);
          this.teleport3(arrival.pos[0]+300*Math.sin(angle),arrival.pos[1],arrival.pos[2]+300*Math.cos(angle),angle*32768/Math.PI);
          arrival.play?.(push ? 4 : 3);
          const reward = this.playground.sideStars?.find(r=>r.secret);
          if (this.playground.secretStar?.origin && !reward?.group.visible) {
            this.objectStar = undefined;
            this.secretTimer = 4;
          }
          return;
        }
        void this.loadCourse(this.enteringDoor.destination, this.enteringDoor.destNode, this.mario.action === ACT_PUSHING_DOOR, 'push', 1, this.enteringDoor.destArea);
      }
      return;
    }
    if (this.towerFade || (this.worldName === 'castle_inside' && this.mario.floor?.type === 0x2f
        && !(this.mario.action & ACT_FLAG_AIR) && this.input.lookingUp)) {
      this.lookPitch = Math.min(0x1800,this.lookPitch+0x100);
      this.lookYaw = toS16(this.lookYaw+Math.round(this.input.headTurn*0x240));
      const yaw = toS16(this.mario.faceAngle[1]+this.lookYaw);
      if (this.towerFade || (this.lookPitch === 0x1800 && (yaw < -0x6fff || yaw >= 0x7000) && this.save.stars >= 10)) {
        if (!this.towerFade) this.sfx.starSound();
        this.towerFade++;
        this.canvas.style.opacity = String(Math.max(0,1-this.towerFade/30));
        if (this.towerFade === 30) void this.loadCourse('totwc','WARP_NODE_0A');
      }
      return;
    }
    this.lookPitch = this.lookYaw = 0;
    if (this.worldName === 'totwc' && (this.floorWarpTimer || this.mario.floor?.type === 0x0a && this.mario.pos[1] < this.mario.floorHeight+2048)) {
      if (++this.floorWarpTimer === 20) void this.loadCourse('castle_inside','WARP_NODE_20');
      return;
    }
    const controller = this.input.consumeStep();
    const actors = this.playground.objects;
    const koopa = actors?.manager.findByBehavior('koopaTheQuick');
    if (actors && koopa && (koopa.action === KoopaRace.Invitation || koopa.action === KoopaRace.Countdown || koopa.action === KoopaRace.ResultDialog)) {
      koopa.vars.answer = controller.buttonPressed & A_BUTTON ? 1 : controller.buttonPressed & B_BUTTON ? -1 : 0;
      this.processObjectEvents(actors.step(this.mario,koopa));
      koopa.vars.answer = 0;
      this.updateRaceHud();
      return;
    }
    const { metalCapTimer, wingCapTimer, vanishCapTimer, ridingShell } = this.mario;
    const cap = Math.max(metalCapTimer, wingCapTimer, vanishCapTimer);
    this.sfx.setPowerMusic(ridingShell ? 0x8e : metalCapTimer ? 0x0f : cap ? 0x0e : 0, !ridingShell && cap === 60);
    if (this.playground.roomDoors?.step(this.mario, this.save, message => this.hud.showToast(message, 1800))) return;
    const shift = this.playground.instantWarps?.[(this.mario.floor?.type ?? 0) - 0x1b];
    if (shift && (this.worldName !== 'castle_inside' || this.save.stars < 70)) {
      if (shift.area !== this.playground.area) this.changeLinkedArea(shift.area, shift.delta);
      else {
        for (let axis = 0; axis < 3; axis++) this.mario.pos[axis] += shift.delta[axis];
        this.prevPos.fromArray(this.mario.pos);
        this.rig.displace(...shift.delta);
      }
    }
    for (const platform of this.playground.platforms ?? []) {
      const liftSound = platform.step(this.mario);
      if (liftSound) this.sfx.ropeway(platform.blocker.position.clone().applyMatrix4(this.camera.matrixWorldInverse).toArray(), liftSound);
    }
    for (const enemy of this.playground.goombas ?? []) enemy.step(this.mario.pos, this.playground.collision);
    if (this.mario.invincTimer > 0) this.mario.invincTimer--;
    const racer=this.playground.objects?.manager.findByBehavior('racingPenguin');
    if (racer) {
      racer.scale = this.save.stars===120 ? 8 : 4;
      if (racer.action===1) racer.vars.answer = controller.buttonPressed & A_BUTTON ? 1 : controller.buttonPressed & B_BUTTON ? -1 : 0;
    }
    const objectEvents = this.playground.objects?.step(this.mario) ?? [];
    if (koopa && (koopa.action === KoopaRace.Invitation || koopa.action === KoopaRace.ResultDialog)) {
      this.processObjectEvents(objectEvents);
      return;
    }
    if (racer && (racer.action===1 || racer.action===2 || racer.vars.answer)) {
      this.processObjectEvents(objectEvents);
      racer.vars.answer=0;
      return;
    }
    if (this.playground.cannons) {
      if ((controller.buttonPressed & B_BUTTON) && !(this.mario.action & (ACT_FLAG_AIR | ACT_FLAG_INTANGIBLE))) {
        if (this.cannonBuddy()) {
          controller.buttonPressed &= ~B_BUTTON;
          if (!this.save.cannonOpen(this.worldName)) {
            this.save.openCannon(this.worldName);
            this.playground.cannons.unlock();
          }
          this.hud.showToast('Ready for blastoff! Hop into the cannon!', 3000);
        }
      }
      this.playground.cannons.step(this.mario);
    }
    this.mario.step(controller, this.rig.yawS16);

    if (this.playground.wingSwitch?.step(this.mario)) {
      this.save.pressWingSwitch();
      this.playground.objects?.setWingBoxes(true);
      this.sfx.capSwitch();
      this.hud.showToast('Wing Cap boxes are now solid!',3000);
    }
    this.dream.update(this.mario.pos);
    this.hundred.step(this.mario.pos);
    this.updateAgentInspector();
    this.updateRaceHud();
    this.updateCapBadge();
    if (this.dream.enabled && this.mario.floor && !this.dream.ownsSurface(this.mario.floor)
        && this.mario.pos[1] <= this.mario.floorHeight + 4) {
      const start = this.dream.startPosition();
      if (start) {
        this.mario.pos = [...start];
        this.mario.vel = [0, 0, 0];
        this.mario.forwardVel = 0;
        this.mario.setAction(ACT_FREEFALL, 0);
        this.rig.snapBehind(this.mario.faceAngle[1]);
        this.hud.showToast('DREAM RESET', 1200);
      }
    }
    if (this.mario.vel[1] <= 0 && this.editor.isBounceSurface(this.mario.floor) && this.mario.pos[1] <= this.mario.floorHeight + 4) {
      this.mario.vel[1] = 80;
      this.mario.setAction(ACT_FREEFALL, 0);
      this.particles.sparks(this.mario.pos, 10);
      this.sfx.jump();
    }
    if (this.viewMode === 'side') {
      this.mario.pos[2] = this.sideLaneZ;
      this.mario.vel[2] = 0;
    }
    if (this.drainingShip) {
      const water = this.playground.collision.waterBoxes[0];
      water[5] = Math.max(-335, water[5] - 5);
      this.drainingShip = water[5] > -335;
    }
    const floorWarp = this.playground.floorWarp;
    if (floorWarp && (this.floorWarpTimer || this.mario.floor?.type === 0x32)) {
      const tick = ++this.floorWarpTimer;
      this.canvas.style.clipPath = `circle(${(20 - tick) * 5}% at center)`;
      if (tick === 20) {
        const { health, hurtCounter, healCounter } = this.mario;
        const coins = this.coinsCollected, redCoins = this.redCoinsCollected, elapsed = this.elapsed;
        void this.loadCourse(this.worldName, floorWarp.entry, false, 'push', this.act, floorWarp.area).then(() => {
          if (this.playground.area === floorWarp.area) {
            Object.assign(this.mario, { health, hurtCounter, healCounter });
            this.hudCoins = this.coinsCollected = coins;
            this.redCoinsCollected = redCoins;
            this.elapsed = elapsed;
          }
          this.floorWarpTimer = 0;
        });
        return;
      }
    }
    this.playground.stepPaintings?.(this.mario, this.save);
    this.checkPaintingWarp();
    this.playground.objects?.afterMarioStep(this.mario);
    this.processObjectEvents(objectEvents);
    const step = this.footsteps.step(this.mario);
    if (step && this.mario.floor) this.sfx.footstep(step === 'tiptoe');

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
              this.playground.objects?.spawnCoin(foot.toArray(),true);
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
        this.playground.objects?.spawnCoin(enemy.model.position.toArray(),true);
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
    this.collectRedCoins();
    this.checkSecrets();
    if ((++this.hudTick & 1) && this.hudCoins < this.coinsCollected) {
      this.hudCoins++;
      this.sfx.coin(Boolean(this.mario.action & (ACT_FLAG_SWIMMING | ACT_FLAG_METAL_WATER)));
    }
    this.checkStar();
    this.elapsed += STEP;
    if (!this.won && (this.mario.action === ACT_WALKING || this.mario.action === ACT_DECELERATING)) {
      for (const door of this.playground.doors ?? []) {
        const dx = door.pos[0] - this.mario.pos[0], dz = door.pos[2] - this.mario.pos[2];
        if (Math.hypot(dx, dz) >= 117 || this.mario.pos[1] >= door.pos[1] + 100
            || this.mario.pos[1] + 160 <= door.pos[1]) continue;
        const locked = door.requiredKey && !this.save.doorUnlocked(door.requiredKey);
        if (locked && !this.save.hasKey(door.requiredKey!)) {
          this.hud.showToast('YOU NEED A KEY TO OPEN THIS DOOR', 1800);
          this.mario.pos[0] = this.prevPos.x;
          this.mario.pos[2] = this.prevPos.z;
          this.mario.forwardVel = 0;
          break;
        }
        const yaw = Math.round(door.yaw * 65536 / 360);
        const pull = Math.abs(toS16(yaw - atan2s(dz, dx))) <= 0x4000;
        this.enteringDoor = door;
        this.mario.setAction(locked ? ACT_UNLOCKING_KEY_DOOR : pull ? ACT_PULLING_DOOR : ACT_PUSHING_DOOR, pull ? 5 : 6);
        this.mario.faceAngle[1] = yaw;
        this.mario.pos = [...door.pos];
        this.mario.forwardVel = 0;
        this.mario.vel = [0, 0, 0];
        if (locked) {
          const angle = door.yaw * Math.PI / 180;
          this.mario.pos[0] += Math.cos(angle) * 75;
          this.mario.pos[2] += Math.sin(angle) * 75;
          if (!pull) this.mario.faceAngle[1] = toS16(yaw + 0x8000);
        } else door.play?.(pull ? 1 : 2);
        break;
      }
    }
  }

  private checkPaintingWarp(): void {
    if (this.worldName !== 'castle_inside' || (this.mario.action & ACT_FLAG_INTANGIBLE)) return;
    if (this.enteringPainting || this.loadingCourse || this.won) return;
    const type = this.mario.floor?.type ?? -1;
    const destination = paintingDestination(type, this.mario.pos[1] - this.mario.floorHeight, this.playground.paintings);
    if (!destination) return;
    this.enteringPainting = true;
    this.hud.showToast(`ENTERING PAINTING — ${destination.toUpperCase()}`, 1800);
    const missions = missionsFor(destination);
    const selected = missions.find(mission => !this.save.hasStar(destination, mission.act)) ?? missions[missions.length - 1];
    void this.loadCourse(destination, 'WARP_NODE_0A', false, 'push', selected.act).finally(() => {
      this.enteringPainting = false;
    });
  }

  private editorActorTargets() {
    const actors = [...(this.playground.objects?.editorActors() ?? [])];
    this.playground.coins.forEach((coin, index) => {
      if (coin.collected) return;
      const baseScale = coin.group.scale.x / (Number(coin.group.userData.editorScale) || 1);
      actors.push({
        id: `coin-${index}`,
        mesh: coin.group,
        position: coin.position.clone(),
        scale: Number(coin.group.userData.editorScale) || 1,
        move: position => {
          coin.position.set(position.x, position.y, position.z);
          coin.group.position.copy(coin.position);
        },
        setScale: scale => {
          coin.group.userData.editorScale = scale;
          coin.group.scale.setScalar(baseScale * scale);
        },
        remove: () => {
          coin.collected = true;
          coin.group.visible = false;
        },
      });
    });
    return actors;
  }

  private spawnEditorActor(type: 'bobomb' | 'coin', position: THREE.Vector3): boolean {
    if (type === 'coin') {
      const template = this.playground.coins[0]?.group;
      const group = template
        ? template.clone(true)
        : new THREE.Mesh(
          new THREE.CylinderGeometry(28, 28, 8, 16),
          new THREE.MeshStandardMaterial({ color: 0xffd84a, metalness: 0.35, roughness: 0.35 }),
        );
      group.visible = true;
      group.position.set(position.x, position.y + 80, position.z);
      const parent = template?.parent ?? this.courseScene;
      parent.add(group);
      const coin = {
        group: group as THREE.Group,
        position: new THREE.Vector3(position.x, position.y + 80, position.z),
        collected: false,
      };
      this.playground.coins.push(coin);
      this.hud.showToast('SPAWNED COIN', 1200);
      return true;
    }
    if (this.worldName !== 'bob' || !this.playground.objects) {
      this.hud.showToast('ACTOR SPAWN UNAVAILABLE IN THIS COURSE', 1600);
      return false;
    }
    const object = this.playground.objects.manager.spawn({
      behavior: 'bobomb',
      model: 'actors/bobomb/black_bobomb_geo.glb',
      pos: [position.x, position.y + 80, position.z],
      yaw: 0,
    });
    this.hud.showToast(`SPAWNED BOB-OMB #${object.id}`, 1400);
    return true;
  }

  private toggleDream(): void {
    const enabled = this.dream.toggle(this.mario.pos);
    this.courseScene.visible = !enabled;
    const hadMarios = this.hundred.enabled;
    if (enabled) {
      const start = this.dream.startPosition();
      if (start) {
        this.mario.pos = [...start];
        this.mario.vel = [0, 0, 0];
        this.mario.forwardVel = 0;
        this.mario.setAction(ACT_FREEFALL, 0);
        this.rig.snapBehind(this.mario.faceAngle[1]);
      }
      if (hadMarios) this.hundred.spawn(this.mario.pos, this.hundred.debug().count);
    } else if (hadMarios) {
      this.hundred.spawn(this.mario.pos, this.hundred.debug().count);
    }
    this.updateExperimentBadge();
    this.hud.showToast(enabled ? 'INFINITE DREAM · STREAMING ISLANDS' : 'DREAM OFF', 1800);
  }

  private startExperiment(name: string): void {
    this.settings.close();
    this.start();
    const url = new URL(location.href);
    if (name) url.searchParams.set('experiment', name); else url.searchParams.delete('experiment');
    history.replaceState(null, '', url);
    switch (name) {
      case 'dimensions':
        if (this.worldName !== 'mushroom2d') {
          void this.loadCourse('mushroom2d', null, false, 'push').then(() => {
            this.setViewMode('side', true);
            this.hud.showToast('ACROSS DIMENSIONS · MUSHROOM KINGDOM 1-1', 2200);
          });
          break;
        }
        if (this.viewMode !== 'side') this.setViewMode('side', true);
        this.hud.showToast('ACROSS DIMENSIONS · SIDE VIEW', 1800);
        break;
      case 'god':
        if (!this.editorEnabled) this.toggleEditor();
        break;
      case 'dream':
        if (!this.dream.enabled) this.toggleDream();
        break;
      case 'hundred':
        if (!this.hundred.enabled) {
          this.hundred.toggle(this.mario.pos);
          this.updateExperimentBadge();
          this.hud.showToast('A HUNDRED MARIOS', 1800);
        }
        break;
      case 'diorama':
        this.enableDiorama();
        break;
    }
  }

  private enableDiorama(): void {
    if (this.diorama.enabled) return;
    this.start();
    void this.diorama.enable();
    this.courseScene.visible = false;
    this.character.root.visible = false;
    this.blobShadow.visible = false;
    this.canvas.style.cursor = 'crosshair';
    this.input.setPointerHandler({
      down: event => this.diorama.click(event.clientX, event.clientY, this.camera, this.canvas),
      move: () => false,
      up: () => {},
      wheel: () => false,
    });
    this.updateExperimentBadge();
    this.hud.showToast('POLY-POCKET DIORAMA · FREE-FLY CAMERA', 2600);
  }

  private disableDiorama(): void {
    if (!this.diorama.enabled) return;
    this.diorama.disable();
    this.courseScene.visible = true;
    this.character.root.visible = true;
    this.blobShadow.visible = true;
    this.canvas.style.cursor = 'grab';
    this.input.setPointerHandler(this.editorEnabled ? {
      down: event => this.editor.pointerDown(event),
      move: event => this.editor.pointerMove(event),
      up: () => this.editor.pointerUp(),
      wheel: delta => this.editor.wheel(delta),
    } : null);
    const url = new URL(location.href);
    if (url.searchParams.get('experiment') === 'diorama') url.searchParams.delete('experiment');
    history.replaceState(null, '', url);
    this.updateExperimentBadge();
    this.hud.showToast('DIORAMA CLOSED', 1600);
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
    this.updateExperimentBadge();
    this.hud.showToast(this.editorEnabled ? 'WORLD EDITOR ON' : 'WORLD EDITOR OFF', 1600);
  }

  private readonly onInspectorPointerDown = (event: PointerEvent): void => {
    if (this.diorama.enabled || this.editorEnabled || !this.hundred.enabled || event.button !== 0) return;
    this.hundred.pickAt(event.clientX, event.clientY, this.camera, this.canvas);
    this.updateAgentInspector();
  };

  private updateAgentInspector(): void {
    const panel = byId('agent-inspector');
    const selected = this.hundred.enabled ? this.hundred.selectedDebug() : null;
    panel.hidden = selected === null;
    if (!selected) return;
    panel.innerHTML = `<strong>MARIO #${selected.id}</strong>`
      + `PHYSICS · ${selected.physics === 'runtime' ? 'FULL RUNTIME' : 'SPRITE CROWD'}<br>`
      + `PERSONALITY · ${selected.personality}<br>`
      + `POSITION · ${selected.pos.map(value => Math.round(value)).join(', ')}<br>`
      + `P · CYCLE PERSONALITY`;
  }

  private updateCapBadge(): void {
    const badge = byId('cap-badge');
    const parts: string[] = [];
    if (this.mario.wingCapTimer > 0) parts.push(`WING ${Math.ceil(this.mario.wingCapTimer / 30)}s`);
    if (this.mario.metalCapTimer > 0) parts.push(`METAL ${Math.ceil(this.mario.metalCapTimer / 30)}s`);
    if (this.mario.vanishCapTimer > 0) parts.push(`VANISH ${Math.ceil(this.mario.vanishCapTimer / 30)}s`);
    badge.textContent = parts.join(' · ');
    badge.hidden = parts.length === 0;
  }

  private updateRaceHud(): void {
    const koopa = this.playground.objects?.manager.objects.find(object => object.behavior === 'koopaTheQuick');
    const badge = byId('race-timer');
    if (koopa && koopa.action >= KoopaRace.Running && koopa.action <= KoopaRace.ResultDialog) {
      badge.hidden = false;
      badge.textContent = `KOOPA RACE · ${formatTime(koopa.vars.elapsed / 30)}`;
    } else {
      badge.hidden = true;
    }
  }

  private musicForCourse(): MusicTrack {
    const track = courseMusic(this.worldName, this.playground.area ?? 1);
    if (this.mario.action === ACT_END_PEACH_CUTSCENE) track.preset = 7;
    return track;
  }

  private updateExperimentBadge(): void {
    const parts: string[] = [];
    if (this.editorEnabled) parts.push('GOD MODE');
    if (this.dream.enabled) parts.push('DREAM');
    if (this.hundred.enabled) parts.push(`MARIOS · ${this.hundred.debug().count} · ${this.hundred.speedScale.toFixed(2)}×`);
    if (this.diorama.enabled) parts.push('DIORAMA');
    const badge = byId('experiment-badge');
    badge.textContent = parts.join(' · ');
    badge.hidden = parts.length === 0;
  }

  private cycleViewMode(): void {
    const order: Array<'3d' | 'side' | 'top'> = ['3d', 'side', 'top'];
    this.setViewMode(order[(order.indexOf(this.viewMode) + 1) % order.length]);
  }

  private chooseSideLane(): boolean {
    if (this.worldName === 'mushroom2d') {
      this.mario.pos[2] = 0;
      this.sideLaneZ = 0;
      return false;
    }
    const placement = findSidePlacement(this.playground.collision, this.mario.pos);
    this.mario.pos = [...placement.pos] as [number, number, number];
    this.sideLaneZ = placement.laneZ;
    return placement.recovered;
  }

  private setViewMode(mode: '3d' | 'side' | 'top', announce = true): void {
    this.viewMode = mode;
    this.mario.movementMode = mode === 'side' ? 'side' : mode === 'top' ? 'topdown' : 'default';
    const recovered = mode === 'side' ? this.chooseSideLane() : false;
    this.camera.up.set(0, 1, 0);
    this.camera.fov = mode === 'side' ? 34 : mode === 'top' ? 50 : 55;
    this.camera.updateProjectionMatrix();
    byId('view-mode').textContent = mode === '3d' ? '3D' : mode === 'side' ? '2D SIDE' : 'TOP-DOWN';
    const url = new URL(location.href);
    if (mode === '3d') url.searchParams.delete('view'); else url.searchParams.set('view', mode);
    history.replaceState(null, '', url);
    if (announce) this.hud.showToast(`VIEW · ${mode === '3d' ? '3D' : mode === 'side' ? '2D SIDE' : 'TOP-DOWN'}${recovered ? ' · SAFE LANE' : ''}`, 1400);
  }

  private updateViewCamera(dt: number): void {
    this.rig.setWaterMode(this.underwater);
    this.rig.setSlideMode(this.worldName === 'pss' || this.worldName === 'ccm' && this.playground.area === 2);
    this.rig.setInteriorMode(this.worldName === 'castle_inside');
    this.rig.setFlightMode(this.mario.action === ACT_FLYING);
    const king = this.playground.objects?.manager.objects.find(object => object.behavior === 'kingBobomb' && object.active);
    const battlefield = this.worldName === 'bob';
    const surface = this.mario.floor?.type ?? 0;
    this.rig.setBattlefieldFloor(battlefield ? surface : null);
    const summit = battlefield && surface === 0x65;
    const bowser = this.playground.objects?.manager.objects.find(object => (object.behavior === 'bowser' || object.behavior === 'bigBully') && object.active);
    const bowserNear = bowser && Math.hypot(bowser.pos[0] - this.mario.pos[0], bowser.pos[2] - this.mario.pos[2]) < 3000;
    this.rig.setBossFocus(summit ? (king ? new THREE.Vector3(...king.pos) : new THREE.Vector3(3072,this.mario.pos[1],-4608)) : bowserNear ? new THREE.Vector3(bowser.pos[0], bowser.pos[1] + 180, bowser.pos[2]) : null);
    if (this.lookPitch) {
      const yaw = (this.mario.faceAngle[1]+this.lookYaw)*Math.PI/32768;
      const pitch = this.lookPitch*Math.PI/32768;
      const eye = this.camera.position.fromArray(this.mario.pos);
      eye.y += 145;
      this.camera.lookAt(eye.x+Math.cos(pitch)*Math.sin(yaw),eye.y+Math.sin(pitch),eye.z+Math.cos(pitch)*Math.cos(yaw));
      return;
    }
    if (this.mario.action === ACT_IN_CANNON && this.mario.actionState === 2) {
      const [pitch,yaw] = this.mario.faceAngle.map(a => a*Math.PI/32768);
      const eye = this.camera.position.fromArray(this.mario.pos);
      this.camera.lookAt(eye.x+Math.cos(pitch)*Math.sin(yaw),eye.y+Math.sin(pitch),eye.z+Math.cos(pitch)*Math.cos(yaw));
      return;
    }
    if (this.viewMode === '3d') {
      this.rig.update(dt, this.renderPos, this.renderYaw, Math.abs(this.mario.forwardVel) > 2, this.mario.forwardVel);
      return;
    }
    if (this.viewMode === 'side') {
      const focus = new THREE.Vector3(this.renderPos.x, this.renderPos.y + 90, this.sideLaneZ);
      const desired = new THREE.Vector3(this.renderPos.x, this.renderPos.y + 220, this.sideLaneZ + 1100);
      const direction = desired.clone().sub(focus);
      const length = direction.length();
      if (length > 1) {
        direction.multiplyScalar(1 / length);
        this.sideCameraRaycaster.set(focus, direction);
        this.sideCameraRaycaster.near = 0;
        this.sideCameraRaycaster.far = length;
        const hit = this.sideCameraRaycaster.intersectObjects(this.playground.collisionMeshes, false)[0];
        if (hit) desired.copy(focus).addScaledVector(direction, Math.max(80, hit.distance - 30));
      }
      const floor = this.playground.collision.findFloor(desired.x, desired.y + 50, desired.z);
      if (floor.surface && desired.y < floor.height + 24) desired.y = floor.height + 24;
      this.camera.position.copy(desired);
      this.camera.up.set(0, 1, 0);
      this.camera.lookAt(focus);
    } else {
      this.camera.position.set(this.renderPos.x, this.renderPos.y + 1500, this.renderPos.z + 1);
      this.camera.up.set(0, 0, -1);
      this.camera.lookAt(this.renderPos.x, this.renderPos.y, this.renderPos.z);
    }
  }

  private updateEnvironment(): void {
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
    }
    if (underwater) {
      this.scene.fog = new THREE.Fog(0x1d5878, 120, 9000);
      if (!(this.scene.background instanceof THREE.Texture)) this.scene.background = new THREE.Color(0x1d5878);
    } else {
      this.scene.fog = this.baseFog;
      this.scene.background = this.baseBackground;
    }
  }

  private processObjectEvents(events: ObjectEvent[]): void {
    for (const event of events) {
      switch (event.type) {
        case 'waterBomb':
          if (event.phase === 'burst') {
            this.particles.burst(event.pos,{count:5,color:0xbbe5ff,upward:300,speed:700,gravity:1800,size:35});
          } else {
            this.sfx.waterBomb(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(),event.phase);
          }
          break;
        case 'waterCannon':
          this.sfx.waterCannon(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray());
          break;
        case 'explosion':
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 18);
          this.sfx.explosion(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(), event.mine);
          this.shake = Math.max(this.shake, 14);
          break;
        case 'break':
          this.particles.dust(event.pos, 16);
          if (event.wall) {
            this.sfx.blastWall(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray());
            if (event.wall === 'right') this.sfx.musicEvent('puzzle');
          } else this.sfx.breakBox(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray());
          this.shake = Math.max(this.shake, 5);
          break;
        case 'hurt':
          this.sfx.hurt();
          this.shake = Math.max(this.shake, 10);
          break;
        case 'chest':
          this.sfx.chestAnswer(Boolean(event.number));
          if (this.playground.drainWater && event.number === 4) this.drainingShip = true;
          this.hud.showToast(event.number ? `TREASURE CHEST ${event.number} / 4` : 'TRY A DIFFERENT ORDER', 1200);
          break;
        case 'boxStar':
          this.boxFlights.set(event.act, new SpawnedStar(event.pos));
          break;
        case 'racerSound':
          this.sfx.racerSound(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(),event.cue);
          if(event.cue==='impact') this.shake=Math.max(this.shake,5);
          break;
        case 'penguinSound':
          this.sfx.penguin(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(),event.diving);
          break;
        case 'snowman':
        case 'penguinReward': {
          if (event.type==='snowman' && event.cue !== 'reward') {
            this.sfx.snowman(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(),event.cue);
            if(event.cue==='assembled') this.sfx.musicEvent('puzzle');
          } else {
            const target=event.type==='snowman' ? this.playground.snowmanStar : this.playground.penguinStar;
            if(target){this.objectStar={...target,origin:event.pos};this.secretTimer=4;}
          }
          break;
        }
        case 'chestReward': {
          const reward = this.playground.sideStars?.find(star => star.chest);
          if (reward) {
            reward.group.visible = true;
            this.hud.showToast('★ ALL CHESTS OPEN', 3000);
            this.sfx.musicEvent('puzzle');
          }
          break;
        }
        case 'bowser':
          this.missionComplete = true;
          this.starReady = true;
          if (this.playground.rewardKey) {
            this.playground.star.position.fromArray(event.pos);
            this.keyFlight = { velocity: 70, landed: false };
          } else if (this.worldName === 'bowser_3') {
            this.grandStar = new GrandStar(event.pos);
          }
          this.hud.showToast('BOWSER DEFEATED', 3600);
          this.sfx.musicEvent('spawn');
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 40);
          break;
        case 'bigBully':
          this.missionComplete = true;
          this.starReady = true;
          this.hud.showToast('★ BIG BULLY DEFEATED', 3200);
          this.sfx.musicEvent('spawn');
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 30);
          break;
        case 'whompKing':
          this.missionComplete = true;
          this.starReady = true;
          this.hud.showToast('★ WHOMP KING DEFEATED', 3200);
          this.sfx.musicEvent('spawn');
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 30);
          break;
        case 'oneUp':
          this.hud.showToast('★ 1-UP · HEALTH RESTORED', 2400);
          this.sfx.musicEvent('spawn');
          this.particles.sparks(event.pos, 20);
          break;
        case 'bigBoo':
          this.missionComplete = true;
          this.starReady = true;
          this.hud.showToast('★ BIG BOO DEFEATED', 3200);
          this.sfx.musicEvent('spawn');
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 30);
          break;
        case 'boss':
          this.missionComplete = true;
          this.starReady = true;
          this.hud.showToast('★ KING BOB-OMB DEFEATED', 3600);
          this.particles.pound(event.pos);
          this.particles.sparks(event.pos, 30);
          this.sfx.musicEvent('spawn');
          this.shake = Math.max(this.shake, 12);
          break;
        case 'dialogue':
          this.hud.showToast(event.text, 3600);
          break;
        case 'warp':
          if (event.destination === this.worldName && event.area && event.area === this.playground.linkedArea?.world.area) {
            this.mario.dropHeldObject();
            this.changeLinkedArea(event.area,[0,0,0]);
            const spawn = this.playground.entrySpawns?.[event.entry ?? ''] ?? this.playground.spawn;
            this.teleport3(...spawn.pos,spawn.faceYaw);
            if (spawn.action) this.mario.setAction(spawn.action);
            break;
          }
          if (!this.loadingCourse && !this.enteringPainting && Object.hasOwn(COURSES, event.destination)) {
            this.enteringPainting = true;
            this.hud.showToast('WARPING…', 1200);
            void this.loadCourse(event.destination as CourseId, event.entry).finally(() => {
              this.enteringPainting = false;
            });
          }
          break;
        case 'race':
          if (event.state === 'start') {
            this.sfx.musicEvent('raceStart');
            this.hud.showToast('RACE KOOPA THE QUICK!', 1800);
          } else if (event.state === 'won') {
            this.sfx.musicEvent('raceEnd');
            this.missionComplete = true;
            this.starReady = true;
            const koopa = this.playground.objects?.manager.findByBehavior('koopaTheQuick');
            if (koopa) {
              this.objectStar = {act:2, origin:[...koopa.pos], pos:KOOPA_RACE_STAR};
              this.secretTimer = 4;
            }
            this.hud.showToast('★ RACE WON!', 3200);
            this.sfx.star();
          } else {
            this.sfx.musicEvent('raceEnd');
            this.hud.showToast(event.state === 'disqualified' ? 'Cannon used — race disqualified' : 'Koopa won the race', 2200);
            this.sfx.bonk();
          }
          break;
        case 'koopaSound':
          this.sfx.koopaSound(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray(),event.cue);
          break;
        case 'coinBounce':
          this.sfx.coinBounce(new THREE.Vector3(...event.pos).applyMatrix4(this.camera.matrixWorldInverse).toArray());
          break;
        case 'movingCoinCollected':
          this.addCoins(1);
          this.particles.sparks(new THREE.Vector3(...event.pos),10);
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
          else if (event.action === ACT_BACKFLIP) this.sfx.jump('MARIO_YAH_WAH_HOO',1);
          else if (event.action === ACT_JUMP_KICK) this.sfx.kick();
          else this.sfx.jump();
          this.particles.dust(feet, 4);
          break;

        case 'triplejump':
          this.sfx.tripleJump();
          this.particles.sparks(feet, 12);
          break;

        case 'land': {
          const strength = Math.min(1.7, Math.abs(event.vy) / 45);
          if (!events.some(e => e.type === 'groundpoundland')) this.sfx.land();
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

        case 'cannonMove': this.sfx.cannonMove(event.phase); break;
        case 'cannonFire': this.sfx.cannonFire(); break;
        case 'bowserGrab': this.sfx.bowserGrab(); break;
        case 'bowserSpin': this.sfx.bowserSpin(); break;
        case 'bowserThrow': this.sfx.bowserThrow(event.fast); break;
        case 'waterPlunge': this.sfx.splash(); break;
        case 'swimStroke': this.sfx.swim(event.fast); break;
        case 'flutterKick': this.sfx.flutter(); break;
        case 'groundpounddrop':
          this.sfx.groundPoundDrop();
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
          this.sfx.hurt();
          this.shake = Math.max(this.shake, 6);
          break;
        case 'death':
          this.sfx.death();
          this.hud.showToast('MARIO DIED', 1800);
          this.shake = Math.max(this.shake, 10);
          break;
        case 'castleEnding':
          void this.loadCourse('castle_grounds', 'WARP_NODE_CREDITS_START');
          break;
        case 'endingCredits':
          this.won = true;
          this.winText.textContent = 'You collected the Grand Star!';
          byId<HTMLButtonElement>('castle-button').hidden = false;
          this.winOverlay.hidden = false;
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

  private touchesNativePickup(pos: THREE.Vector3, radius: number, height: number): boolean {
    const hitbox = marioHitbox(this.mario);
    const [x, y, z] = this.mario.pos;
    return !(this.mario.action & ACT_FLAG_INTANGIBLE)
      && (x - pos.x) ** 2 + (z - pos.z) ** 2 < (radius + hitbox.radius) ** 2
      && y <= pos.y + height && y + hitbox.height >= pos.y;
  }

  private addCoins(value: number): void {
    const previous = this.coinsCollected;
    this.coinsCollected += value;
    this.save.recordCoins(this.worldName, this.coinsCollected);
    this.mario.healCounter += 4 * value;
    const reward = this.playground.hundredCoinStar;
    if (reward && previous < 100 && this.coinsCollected >= 100) {
      this.coinStarFlight = new SpawnedStar(this.mario.pos, this.mario.pos);
      reward.group.position.fromArray(this.coinStarFlight.pos);
      reward.group.visible = true;
      this.sfx.star();
    }
  }

  private collectCoins(): void {
    const px = this.mario.pos[0];
    const py = this.mario.pos[1] + 55;
    const pz = this.mario.pos[2];

    for (const coin of this.playground.coins) {
      if (coin.collected || (coin.loot && !coin.loot.tangible)) continue;
      const dx = px - coin.position.x;
      const dy = py - coin.position.y;
      const dz = pz - coin.position.z;
      if (coin.nativeHitbox ? !this.touchesNativePickup(coin.position, coin.value === 5 ? 120 : 100, 64)
          : dx * dx + dy * dy + dz * dz > 70 * 70) continue;

      coin.collected = true;
      coin.group.visible = false;
      this.addCoins(coin.value ?? 1);
      this.particles.sparks(coin.position, 10);

      const required = this.playground.requiredCoins ?? this.playground.coins.length;
      if (!this.playground.starRequiresEvent && !this.starReady && this.coinsCollected >= required) {
        this.starReady = true;
        this.hud.showToast('★ REACH THE STAR ★', 4200);
        this.sfx.star();
        this.shake = Math.max(this.shake, 6);
      } else {
        this.hud.showToast(this.playground.requireCoins === false ? `COIN ${this.coinsCollected}`
          : `COIN ${this.coinsCollected} / ${required}`, 700);
      }
    }
  }

  private collectRedCoins(): void {
    for (const coin of this.playground.redCoins ?? []) {
      if (coin.collected || !this.touchesNativePickup(coin.position, 100, 64)) continue;
      coin.collected = true;
      coin.group.visible = false;
      this.redCoinsCollected++;
      this.addCoins(2);
      this.sfx.redCoin(this.redCoinsCollected);
      this.particles.sparks(coin.position, 12);
      if (this.redCoinsCollected === 8) {
        const reward = this.playground.sideStars?.find(star => star.redCoins);
        if (reward) reward.group.visible = true;
        else {
          this.missionComplete = true;
          this.starReady = true;
        }
        this.hud.showToast('★ ALL 8 RED COINS — REACH THE STAR', 3600);
        this.sfx.star();
      } else {
        this.hud.showToast(`RED COIN ${this.redCoinsCollected} / 8`, 900);
      }
    }
  }

  private checkSecrets(): void {
    const puzzle = this.playground.secretStar;
    if (!puzzle || this.secretTimer >= 0) return;
    const point = new THREE.Vector3();
    for (const [i,pos] of puzzle.triggers.entries()) {
      if (this.secrets.has(i) || !this.touchesNativePickup(point.fromArray(pos),100,100)) continue;
      this.secrets.add(i);
      const count = 5 - puzzle.triggers.length + this.secrets.size;
      this.sfx.secret(count);
      if (count === 5) this.secretTimer = 0;
      else this.hud.showToast(String(count), 1000);
    }
  }

  // bhv_hidden_star_loop and bhv_star_spawn_loop. For a hidden star, origin and home coincide.
  private stepSecretStar(): boolean {
    const puzzle = this.objectStar ?? this.playground.secretStar;
    if (!puzzle || this.secretTimer < 0 || this.secretTimer >= 119) return false;
    const t = this.secretTimer++;
    const reward = this.playground.sideStars?.find(star => this.objectStar ? star.act===this.objectStar.act : star.secret);
    const group = reward?.group ?? (this.objectStar && this.worldName === 'bob' && this.act === 2 ? this.playground.star : undefined);
    if (!group || t < 4) return false;
    group.visible = true;
    const arc = t - 26;
    let y = 0;
    if (arc >= 0 && arc <= 30) y = 400 * Math.sin(Math.PI * arc / 30);
    if (arc === 30) this.sfx.musicEvent('spawn');
    if (arc > 30) {
      const bounce = arc - 30;
      const rising = Math.min(20,bounce);
      y = Math.max(0, rising * (41-rising) / 2 - Math.max(0,bounce-20)*10);
    }
    if (puzzle.origin) {
      const progress = Math.max(0,Math.min(1,(arc+1)/30));
      group.position.fromArray(puzzle.origin).lerp(new THREE.Vector3(...puzzle.pos),progress);
      group.position.y += y;
    } else group.position.y = puzzle.pos[1] + y;
    if (!this.secretCamera) {
      const focus = this.camera.getWorldDirection(new THREE.Vector3()).multiplyScalar(
        this.camera.position.distanceTo(this.renderPos)).add(this.camera.position);
      this.secretCamera = { position: this.camera.position.clone(), focus, fov: this.camera.fov };
    }
    const {focus} = this.secretCamera;
    if (t === 34) {
      const [x,y,z] = puzzle.pos;
      const towardMario = Math.atan2(this.mario.pos[0]-x, this.mario.pos[2]-z);
      const towardCamera = Math.atan2(this.camera.position.x-x,this.camera.position.z-z);
      const yaw = towardMario + (towardMario < towardCamera ? Math.PI/2 : -Math.PI/2);
      const horizontal = 400*Math.cos(Math.PI/8);
      this.camera.position.set(x+horizontal*Math.sin(yaw),y+400*Math.sin(Math.PI/8),z+horizontal*Math.cos(yaw));
      this.camera.fov = 60;
      this.camera.updateProjectionMatrix();
    }
    focus.x += (puzzle.pos[0]-focus.x)*0.15;
    focus.z += (puzzle.pos[2]-focus.z)*0.15;
    if (t < 34) focus.y += (group.position.y-focus.y)*0.1;
    else focus.y = puzzle.pos[1] + y*0.8;
    this.camera.lookAt(focus);
    if (t === 118) this.restoreSecretCamera();
    if (t % 3 === 0 && t < 99) this.particles.sparks(group.position,2);
    return true;
  }

  private restoreSecretCamera(): void {
    if (!this.secretCamera) return;
    this.camera.position.copy(this.secretCamera.position);
    this.camera.fov = this.secretCamera.fov;
    this.camera.updateProjectionMatrix();
    this.secretCamera = null;
  }

  // CUTSCENE_STAR_SPAWN freezes Mario and the other objects throughout both arcs.
  private stepSpawnedStar(): boolean {
    let flight = this.coinStarFlight;
    let group = this.playground.hundredCoinStar?.group;
    if (!flight || flight.tangible) {
      if (!this.boxFlights.size) return false;
      const reward = this.playground.sideStars?.find(r => r.box && this.boxFlights.get(r.act)?.tangible === false);
      if (!reward) return false;
      flight = this.boxFlights.get(reward.act)!;
      group = reward.group;
    }
    if (!group) return false;
    if (!this.spawnCamera) {
      const focus = this.camera.getWorldDirection(new THREE.Vector3())
        .multiplyScalar(this.camera.position.distanceTo(this.renderPos)).add(this.camera.position);
      this.spawnCamera = {focus, original:focus.clone(), ticks:0};
    }
    const wasBouncing = flight.bouncing;
    flight.step();
    group.visible = true;
    group.position.fromArray(flight.pos);
    group.rotation.y = flight.yaw * Math.PI / 32768;
    const shot = this.spawnCamera;
    if (!wasBouncing && flight.bouncing) this.sfx.musicEvent('spawn');
    if (flight.returning || flight.tangible) {
      shot.focus.lerp(shot.original, 1 / Math.max(1, 15 - shot.ticks++));
    } else {
      shot.focus.lerp(group.position, 0.1);
      this.particles.sparks(group.position, 1);
    }
    this.camera.lookAt(shot.focus);
    if (flight.tangible) this.spawnCamera = null;
    return true;
  }

  private checkStar(): void {
    if (this.mario.action === ACT_JUMBO_STAR_CUTSCENE || this.mario.action === ACT_END_PEACH_CUTSCENE) return;
    if (this.won || this.collectedNoExitStar) return;
    const hundred = this.playground.hundredCoinStar;
    if (hundred && this.coinStarFlight) {
      const flight = this.coinStarFlight;
      if (flight.tangible) flight.step();
      hundred.group.position.fromArray(flight.pos);
      hundred.group.rotation.y = flight.yaw * Math.PI / 32768;
      if (flight.tangible && this.touchesNativePickup(hundred.group.position, 80, 50)) {
        this.save.collectStar(this.worldName, 7);
        this.particles.sparks(hundred.group.position, 40);
        this.sfx.star();
        this.hud.showToast('★ 100 COINS', 3600);
        hundred.group.visible = false;
        this.coinStarFlight = null;
        return;
      }
    }
    for (const reward of this.playground.sideStars ?? []) {
      const flight = reward.box ? this.boxFlights.get(reward.act) : undefined;
      if (flight) {
        if (flight.tangible) flight.step();
        reward.group.rotation.y = flight.yaw * Math.PI / 32768;
        reward.group.position.fromArray(flight.pos);
        reward.group.visible = true;
        if (!flight.tangible) continue;
      }
      if (reward.group.visible && this.touchesNativePickup(reward.group.position, 80, 50)) {
        this.finishReward(reward.group.position, reward.act);
        return;
      }
    }
    if (this.playground.starRequiresEvent && this.missionComplete && this.starVisibleByCourse) {
      this.playground.star.visible = true;
      this.playground.starLight.visible = true;
    }
    if (this.won || !this.playground.star.visible) return;
    const star = this.playground.star.position;
    if (this.grandStar) {
      this.grandStar.step();
      star.fromArray(this.grandStar.pos);
      if (!this.grandStar.tangible) return;
    }
    if (this.keyFlight) {
      const flight = this.keyFlight;
      const floor = this.playground.collision.findFloor(star.x, star.y + 78, star.z);
      flight.velocity = Math.max(-78, flight.velocity - 4);
      star.y += flight.velocity;
      if (star.y < floor.height) {
        star.y = floor.height;
        flight.velocity *= -0.7;
        if (flight.landed) this.keyFlight = null;
        flight.landed = true;
      } else flight.landed = false;
      return;
    }
    const dx = this.mario.pos[0] - star.x;
    const dy = this.mario.pos[1] + 60 - star.y;
    const dz = this.mario.pos[2] - star.z;
    if (this.playground.setCollectedStarModel) {
      if (!this.touchesNativePickup(star, this.playground.rewardKey || this.grandStar ? 160 : 80, this.playground.rewardKey || this.grandStar ? 100 : 50)) return;
    } else if (dx * dx + dy * dy + dz * dz > 130 * 130) return;

    if (!this.starReady) {
      if (this.elapsed - this.lastStarHint > 3) {
        this.lastStarHint = this.elapsed;
        const required = this.playground.requiredCoins ?? this.playground.coins.length;
        this.hud.showToast(`THE STAR IS ASLEEP — ${Math.min(this.coinsCollected, required)}/${required} COINS`, 2200);
      }
      return;
    }

    this.finishReward(star, this.playground.primaryStarAct ?? this.act);
  }

  private finishReward(star: THREE.Vector3, act: number): void {
    this.starReady = false;
    if (this.playground.rewardKey) this.save.collectKey(this.playground.rewardKey);
    else this.save.collectStar(this.worldName, act);
    this.sfx.star();
    this.particles.pound(star);
    this.particles.sparks(star, 40);
    this.shake = Math.max(this.shake, 10);
    if (!this.grandStar) this.sfx.musicEvent('clear');
    if (this.grandStar) {
      this.playground.star.visible = this.playground.starLight.visible = false;
      this.sfx.stopMusic();
      this.mario.hurtCounter = this.mario.healCounter = 0;
      this.mario.setAction(ACT_JUMBO_STAR_CUTSCENE);
      return;
    }
    if (this.playground.noExitStar) {
      this.collectedNoExitStar = true;
      this.playground.star.visible = false;
      this.playground.starLight.visible = false;
      this.hud.showToast('★ POWER STAR', 3600);
      return;
    }
    this.won = true;
    this.winText.textContent = this.playground.rewardKey ? "You collected Bowser’s key!" : `You collected ${this.coinsCollected} coins and reached the star in ${formatTime(
      this.elapsed,
    )}.`;
    byId<HTMLButtonElement>('castle-button').hidden = this.worldName === 'playground';
    this.winOverlay.hidden = false;
  }

  /** Debug helper used by the screenshot harness. */
  inspectAgent(id: number): void {
    this.hundred.select(id);
    this.updateAgentInspector();
  }

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
      fps: Math.round(this.fps),
      diorama: this.diorama.debug(),
      musicTrack: this.sfx.musicTrack(),
      display: { ...this.display, width: this.canvas.width, height: this.canvas.height },
      paused: this.settings.open,
      respawns: this.mario.respawns,
      started: this.started,
      won: this.won,
      coins: this.coinsCollected,
      redCoins: this.redCoinsCollected,
      action: this.mario.action,
      actionName: actionName(this.mario.action),
      pos: [...this.mario.pos],
      vel: [...this.mario.vel],
      forwardVel: this.mario.forwardVel,
      health: this.mario.health,
      floorHeight: this.mario.floorHeight,
      starReady: this.starReady,
      missionComplete: this.missionComplete,
      cameraYaw: this.rig.yawS16,
      cameraMode: this.rig.mode,
      cameraContext: this.rig.context,
      viewMode: this.viewMode,
      sideLaneZ: this.sideLaneZ,
      gravityScale: this.mario.gravityScale,
      editorEnabled: this.editorEnabled,
      editorBlocks: this.editor.debug(),
      editorActors: this.editor.actorDebug(),
      wingCap: this.mario.wingCapTimer,
      caps: {
        wing: this.mario.wingCapTimer / 30,
        metal: this.mario.metalCapTimer / 30,
        vanish: this.mario.vanishCapTimer / 30,
      },
      dream: this.dream.debug(),
      waterSurfaces: this.waterSurfaces.length,
      platforms: this.playground.platforms?.length ?? 0,
      hundred: this.hundred.debug(),
      coinCount: this.playground.coins.filter(coin => !coin.collected).length,
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
