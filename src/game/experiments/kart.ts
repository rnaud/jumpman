import * as THREE from 'three';
import { CollisionWorld, type WallCollisionData } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import { SURFACE_FLAG_X_PROJECTION, SURFACE_NOT_SLIPPERY } from '../../sim/surfaces';
import { A_BUTTON, B_BUTTON, Z_TRIG, type ControllerSnapshot } from '../../sim/state';
import { KartPhysics, TOP_SPEED, type KartGround, type KartInput } from './kartPhysics';
import { addMeshCollision } from '../world';
import { SkyDome } from './sky';
import { Lakitu } from './kartRescue';
import { ITEM_ICON, KartRaceView, SPRITE_SIZE, type ItemTextures } from './kartRaceView';
import { KartCamera, cameraKart, lerpPose, type CameraPose } from './kartCamera';
import { KartFx } from './kartFx';
import { KartAudio } from './kartAudio';
import { KartSounds, engineSound, type SoundFrame } from './kartSound';
import { actionButton, dock, isTouch, readout, relabelTouch, tintTouch, titleCard } from './ui';
import type { Experiment, ExperimentHost } from './types';

interface RacewayData {
  vertices: number[][];
  groups: Record<string, number[]>;
  textures: Record<string, { file: string; width: number; height: number }>;
  path: Vec3[];
  scale: number;
}

/** Heading (browser convention, forward = (sin y, cos y)) to the physics' s16 rotation. */
const s16Rotation = (yaw: number): number => (Math.trunc(-yaw * 32768 / Math.PI) << 16) >> 16;

/** Free-roam driving in the original course, with SM64 movement when dismounted.
 * Geometry/textures/sprite views come from MK64; driving is a browser controller,
 * not a binary-compatible port of player_controller.c.
 */
export class KartExperiment implements Experiment {
  readonly id = 'kart';
  readonly title = 'Mario Kart 64 · Luigi Raceway';
  readonly course = 'playground';
  readonly replacesCourse = true;
  private host!: ExperimentHost;
  private readonly root = new THREE.Group();
  private readonly world = new CollisionWorld(1024, 100000);
  private readonly meshes: THREE.Mesh[] = [];
  private readonly materials = new Set<THREE.Material>();
  private readonly textures: THREE.Texture[] = [];
  private readonly parked = new THREE.Group();
  private sprite!: THREE.Sprite;
  private race!: KartRaceView;
  private sky?: SkyDome;
  private saved?: { fog: THREE.Scene['fog']; background: THREE.Scene['background']; far: number };
  private spawn: Vec3 = [0, 0, 0];
  private position: Vec3 = [0, 0, 0];
  private yaw = Math.PI;
  private speed = 0;
  private kart!: KartPhysics;
  /** Course units -> world units (the MK64 export scale). */
  private readonly unit = 8;
  private readonly ground: KartGround = {
    floor: (x, y, z) => {
      const hit = this.world.findFloor(x * this.unit, y * this.unit, z * this.unit);
      if (!hit.surface) return null;
      return { height: hit.height / this.unit, normal: [hit.surface.normal[0], hit.surface.normal[1], hit.surface.normal[2]], grass: hit.surface.type === SURFACE_NOT_SLIPPERY };
    },
    wall: pos => {
      const data: WallCollisionData = { x: pos[0] * this.unit, y: pos[1] * this.unit, z: pos[2] * this.unit, offsetY: 50, radius: 65, numWalls: 0, walls: [] };
      if (!this.world.findWallCollisions(data)) return null;
      return {
        pos: [data.x / this.unit, data.y / this.unit, data.z / this.unit],
        hits: data.walls.map(s => ({ normal: [s.normal[0], s.normal[1], s.normal[2]], xFacing: (s.flags & SURFACE_FLAG_X_PROJECTION) !== 0 })),
      };
    },
  };
  private readonly camera = new KartCamera();
  private poses: [CameraPose, CameraPose] | null = null;
  private fx?: KartFx;
  private audio?: KartAudio;
  private readonly sounds = new KartSounds();
  /** Last tick's pad, bump counter and landing squash for the sprite. */
  private accel = false;
  private bumpsSeen = 0;
  private bumpedTick = false;
  private watched: KartPhysics | null = null;
  private squash = 0;
  private wasMidair = false;
  private lastFov = 0;
  private baseFov = 50;
  private driving = true;
  private ready = false;
  private disposed = false;
  private toggleRequested = false;
  private button!: HTMLButtonElement;
  private speedText: (text: string) => void = () => {};
  private modeText: (text: string) => void = () => {};
  private driftText: (text: string) => void = () => {};
  /** Graceful out-of-bounds rescue (set once the course path is known) and its black overlay. */
  private lakitu?: Lakitu;
  private fade?: HTMLElement;

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    const response = await fetch('/mk64/luigi-raceway.json');
    if (!response.ok || !response.headers.get('content-type')?.includes('json')) {
      throw new Error('MK64 assets missing. Run pnpm export:mk64 /path/to/Mario-Kart-64.z64');
    }
    const data = await response.json() as RacewayData;
    const loader = new THREE.TextureLoader();
    // Load sequentially so a failed load cannot leave other promises allocating resources after exit.
    const materials = new Map<string, THREE.MeshBasicMaterial>();
    for (const [key, info] of Object.entries(data.textures)) {
      const texture = await loader.loadAsync('/mk64/' + info.file);
      if (this.disposed) { texture.dispose(); return; }
      texture.colorSpace = THREE.SRGBColorSpace;
      texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
      texture.magFilter = THREE.NearestFilter;
      this.textures.push(texture);
      const material = new THREE.MeshBasicMaterial({ map: texture, vertexColors: true, side: THREE.DoubleSide, alphaTest: 0.5 });
      materials.set(key, material);
      this.materials.add(material);
    }
    for (const [key, indices] of Object.entries(data.groups)) {
      const positions: number[] = [], colors: number[] = [], uvs: number[] = [];
      const info = data.textures[key];
      for (const index of indices) {
        const v = data.vertices[index];
        positions.push(v[0] * data.scale, v[1] * data.scale, v[2] * data.scale);
        // Vertex colours are baked lighting, supplied in sRGB by the original game.
        const c = new THREE.Color().setRGB(v[5] / 255, v[6] / 255, v[7] / 255, THREE.SRGBColorSpace);
        colors.push(c.r, c.g, c.b);
        uvs.push(v[3] / (32 * info.width), 1 - v[4] / (32 * info.height));
      }
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
      geometry.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
      geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
      geometry.computeVertexNormals();
      const mesh = new THREE.Mesh(geometry, materials.get(key));
      mesh.name = key;
      this.root.add(mesh);
      this.meshes.push(mesh);
      addMeshCollision(this.world, mesh, key.includes('Grass') ? SURFACE_NOT_SLIPPERY : 0);
    }
    this.world.finalize();
    const atlas = await loader.loadAsync('/mk64/mario-kart.png');
    if (this.disposed) { atlas.dispose(); return; }
    this.textures.push(atlas);
    atlas.colorSpace = THREE.SRGBColorSpace;
    atlas.magFilter = atlas.minFilter = THREE.NearestFilter;
    atlas.generateMipmaps = false;
    atlas.repeat.set(1 / 35, 1);
    this.sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: atlas, alphaTest: 0.5 }));
    this.sprite.center.set(0.5, 0);
    this.sprite.scale.set(SPRITE_SIZE, SPRITE_SIZE, 1);
    this.root.add(this.sprite);
    const icons = {} as ItemTextures;
    for (const [name, url] of Object.entries(ITEM_ICON)) {
      const texture = await loader.loadAsync(url);
      if (this.disposed) { texture.dispose(); return; }
      texture.colorSpace = THREE.SRGBColorSpace;
      this.textures.push(texture);
      icons[name as keyof ItemTextures] = texture;
    }
    this.race = new KartRaceView(host, this.root, atlas, data.path, this.unit, icons);
    this.fx = new KartFx(this.root);
    this.audio = new KartAudio(host.sfx);
    this.buildParkedKart();
    this.root.add(this.parked);
    this.spawn = data.path[0].map(v => v * data.scale) as Vec3;
    this.position = [...this.spawn];
    const floor = this.world.findFloor(this.spawn[0], this.spawn[1] + 200, this.spawn[2]);
    if (!floor.surface) throw new Error('Luigi Raceway spawn has no collision floor');
    this.spawn[1] = this.position[1] = floor.height;
    this.resetKart();
    this.saved = { fog: host.scene.fog, background: host.scene.background, far: host.camera.far };
    this.baseFov = host.camera.fov;
    host.scene.fog = new THREE.Fog(0xa7d8f3, 18000, 45000);
    host.camera.far = 65000;
    host.camera.updateProjectionMatrix();
    this.sky = new SkyDome({ top: 0x398bdb, mid: 0xa7d8f3, bottom: 0xe4f3ff });
    host.scene.add(this.root, this.sky.mesh);
    host.setMarioWorld(this.world, { pos: this.spawn, faceYaw: -32768 }, -5000, this.meshes);
    host.placeMario([...this.spawn], -32768);
    host.setPlayerVisible(false);
    titleCard(host.ui, '09', 'Luigi Raceway', 'Drive through the item boxes, or press T for a 3-lap race against 3 CPU karts. Park anywhere and explore the track on foot.', [
      ['T · RACE', 'start a 3-lap Grand Prix with 3 CPU karts'],
      ['W / S · stick', 'accelerate / brake (hold S at a stop to reverse)'], ['A / D · stick', 'steer'],
      ['Space · A', 'hop; hold while steering to drift, then counter-steer to charge a mini-turbo'],
      ['E / Shift · gamepad B', 'use the item you picked up (mushroom, banana, green shell)'],
      ['F', 'get out / get in nearby'], ['Space on foot', 'jump'], ['R', 'reset to starting line'],
    ], [
      ['RACE', 'tap RACE for 3 laps against 3 karts'], ['Stick', 'steer; up drives, down brakes'], ['A', 'hop; hold while turning to drift'],
      ['A + stick', 'steer against drift: turbo'], ['B / ITEM', 'use the item from the item boxes'], ['Get out', 'park and walk around'],
    ]);
    const bar = dock(host.ui);
    bar.classList.add('xp-dock-between');
    this.button = actionButton('Get out · F', () => { this.toggleRequested = true; });
    this.button.dataset.kartToggle = '';
    const resetButton = actionButton(isTouch() ? 'Reset' : 'Reset · R', () => this.reset());
    bar.append(this.button, resetButton);
    this.race.mount(bar, [this.button, resetButton], { start: () => this.startRace(), exit: () => this.endRace(), change: () => this.updateUI() });
    this.race.beginPractice(this.kart, this.ground);
    this.snapCamera();
    this.modeText = readout(bar, 'Mode');
    this.speedText = readout(bar, 'Speed');
    this.driftText = readout(bar, 'Drift');
    this.lakitu = new Lakitu(data.path.map(p => p.map(v => v * data.scale) as Vec3));
    this.fade = document.createElement('div');
    this.fade.style.cssText = 'position:absolute;inset:0;background:#000;opacity:0;pointer-events:none;z-index:6';
    host.ui.append(this.fade);
    this.ready = true;
    this.updateUI();
    host.toast(isTouch() ? 'Grab the item boxes, or tap RACE for 3 laps' : 'Grab the item boxes, or press T for a 3-lap race', 5000);
  }

  /** MK64 stores Mario and his kart together as sprites. An empty 3D kart is
   * used only while parked, so Mario does not remain sitting in the sprite. */
  private buildParkedKart(): void {
    const box = (w: number, h: number, d: number, color: number, x: number, y: number, z: number) => {
      const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), new THREE.MeshStandardMaterial({ color, roughness: 0.7 }));
      mesh.position.set(x, y, z);
      this.parked.add(mesh);
    };
    box(100, 24, 140, 0xd72c22, 0, 30, 0);
    box(65, 14, 60, 0xffffff, 0, 44, 45);
    box(62, 55, 16, 0x24252c, 0, 61, -38);
    box(140, 10, 12, 0xc4c7cd, 0, 25, 78);
    for (const x of [-62, 62]) for (const z of [-45, 45]) {
      const wheel = new THREE.Mesh(new THREE.CylinderGeometry(25, 25, 22, 12), new THREE.MeshStandardMaterial({ color: 0x18191d }));
      wheel.rotation.z = Math.PI / 2;
      wheel.position.set(x, 25, z);
      this.parked.add(wheel);
    }
    const steering = new THREE.Mesh(new THREE.TorusGeometry(19, 3, 6, 16), new THREE.MeshStandardMaterial({ color: 0x24252c }));
    steering.position.set(0, 70, 20);
    steering.rotation.x = -0.5;
    this.parked.add(steering);
  }

  keydown(event: KeyboardEvent): boolean {
    if (event.code === 'KeyV') return true; // This map's pedestrian is Mario.
    if (!this.ready) return false;
    if (this.race.keydown(event)) return true;
    if (event.code === 'KeyF') { if (!this.race.active) this.toggleRequested = true; return true; }
    if (event.code === 'KeyR') { if (this.race.active) this.race.requestRescue(); else this.reset(); return true; }
    return false;
  }

  /** Race mode (kartRace.ts): T starts or restarts a three-lap race from the grid. */
  private startRace(): void {
    if (!this.ready) return;
    this.driving = true;
    this.toggleRequested = false;
    this.lakitu?.forget();
    if (this.fade) this.fade.style.opacity = '0';
    this.host.setPlayerVisible(false);
    this.kart = this.race.begin(this.ground);
    this.snapCamera();
    this.syncFromKart();
    this.host.placeMario([...this.position], this.yaw * 32768 / Math.PI);
    this.updateUI();
  }

  private endRace(): void {
    this.race.end();
    this.reset();
  }

  private syncFromKart(): void {
    this.position = this.kart.pos.map(v => v * this.unit) as Vec3;
    this.yaw = this.kart.yaw();
    this.speed = this.kart.speed;
    this.host.mario.pos = [...this.position];
    this.host.mario.faceAngle[1] = this.yaw * 32768 / Math.PI;
    this.host.mario.forwardVel = 0;
  }

  private reset(): void {
    if (!this.ready) return;
    this.lakitu?.forget();
    this.position = [...this.spawn];
    this.yaw = Math.PI;
    this.speed = 0;
    this.resetKart();
    this.race.beginPractice(this.kart, this.ground);
    this.snapCamera();
    this.driving = true;
    this.toggleRequested = false;
    this.host.setPlayerVisible(false);
    this.host.placeMario([...this.spawn], -32768);
    this.updateUI();
  }

  /** The kart was put somewhere new (start, reset, rescue, getting back in): drop the camera behind it. */
  private snapCamera(): void {
    this.camera.snap(this.cameraKart());
    this.poses = null;
    this.sounds.reset();
    this.bumpsSeen = this.kart.bumps;
    this.watched = this.kart;
    this.squash = 0;
  }

  private cameraKart() {
    const me = this.race?.me();
    return cameraKart(this.kart, (me?.boost ?? 0) > 0, (me?.spin ?? 0) > 0);
  }

  /** Two 60 Hz camera updates per 30 Hz tick, then the sounds for what just happened. */
  private tickEffects(input: KartInput): void {
    if (this.watched !== this.kart) { this.watched = this.kart; this.bumpsSeen = this.kart.bumps; }
    this.bumpedTick = this.kart.bumps !== this.bumpsSeen;
    this.bumpsSeen = this.kart.bumps;
    this.accel = input.accel;
    const ck = { ...this.cameraKart(), bumped: this.bumpedTick };
    const before = this.camera.initialised ? this.camera.pose() : null;
    this.camera.step(ck);
    this.camera.step({ ...ck, bumped: false });
    const after = this.camera.pose();
    this.poses = [before ?? after, after];
    const cues = this.sounds.update(this.soundFrame(), this.race.lastEvents);
    for (const cue of cues) this.audio?.play(cue);
    for (const at of this.race.smashed) this.fx?.burst(at);
  }

  private soundFrame(): SoundFrame {
    const k = this.kart, me = this.race.me();
    return {
      hop: k.hop, drifting: k.drifting, driftState: k.driftState, miniTurbo: k.miniTurbo, bumped: this.bumpedTick, midair: k.midair,
      speedRatio: k.currentSpeed / TOP_SPEED, grass: k.grass, accel: this.accel, reversing: k.backUp,
      roulette: me?.roulette ?? 0, clock: this.race.clock(), spinning: (me?.spin ?? 0) > 0, boosting: (me?.boost ?? 0) > 0, driving: this.driving,
    };
  }

  private resetKart(): void {
    this.kart = new KartPhysics(this.spawn.map(v => v / this.unit) as Vec3, -32768);
  }

  private distance(): number { return Math.hypot(...this.host.mario.pos.map((v, i) => v - this.position[i])); }

  private toggle(): void {
    if (this.driving) {
      // Try both sides and behind, rejecting walls, steep ground and drops.
      for (const angle of [Math.PI / 2, -Math.PI / 2, Math.PI]) {
        const pos: Vec3 = [this.position[0] + Math.sin(this.yaw + angle) * 170, this.position[1], this.position[2] + Math.cos(this.yaw + angle) * 170];
        if (this.world.f32FindWallCollision(pos, 60, 50)) continue;
        const floor = this.world.findFloor(pos[0], pos[1] + 100, pos[2]);
        if (!floor.surface || floor.surface.normal[1] < 0.7 || Math.abs(floor.height - pos[1]) > 90) continue;
        pos[1] = floor.height;
        this.speed = 0;
        this.kart.velocity = [0, 0, 0];
        this.kart.currentSpeed = 0;
        this.kart.speed = 0;
        this.driving = false;
        this.host.placeMario(pos, this.yaw * 32768 / Math.PI);
        this.host.setPlayerVisible(true);
        return;
      }
      this.host.toast('Move the kart to open ground before getting out.');
    } else if (this.distance() < 300 && Math.abs(this.host.mario.pos[1] - this.position[1]) < 100) {
      this.driving = true;
      this.snapCamera();
      this.host.placeMario([...this.position], this.yaw * 32768 / Math.PI);
      this.host.setPlayerVisible(false);
    } else this.host.toast('Walk back to the kart to get in.');
  }

  simulate(controller: ControllerSnapshot): void {
    if (!this.ready) return;
    // B uses an item while driving; on foot it still gets back in. F and the dock button always toggle.
    if (!this.race.active && (this.toggleRequested || (!this.driving && (controller.buttonPressed & B_BUTTON)))) {
      this.toggleRequested = false;
      this.toggle();
      this.updateUI();
      return;
    }
    const m = this.host.mario;
    if (!this.driving) {
      m.step(controller, this.host.rig.yawS16);
      if (m.pos[1] < -4500) this.reset();
    } else if (this.lakitu?.active && this.rescueTick()) {
      // Lakitu is holding the kart while the screen fades out and back in.
    } else {
      // MK64 runs its kart update at 60 Hz; the sim ticks at 30 Hz, so two updates per tick.
      const input: KartInput = {
        rawX: Math.round(controller.stickX * 80 / 64), rawY: Math.round(controller.stickY * 80 / 64),
        accel: controller.stickY > 16, brake: !!(controller.buttonDown & Z_TRIG) || controller.stickY < -16,
        hopDown: !!(controller.buttonDown & A_BUTTON), hopPressed: !!(controller.buttonPressed & A_BUTTON),
      };
      this.kart = this.race.step(input, !!(controller.buttonPressed & B_BUTTON), this.kart, this.ground);
      this.syncFromKart();
      this.tickEffects(input);
      if (!this.race.active) {
        if (!this.kart.midair) this.lakitu?.noteGrounded(this.position);
        if (this.lakitu?.shouldRescue(this.position)) { this.lakitu.start(); this.audio?.play('lakitu'); }
        else if (!this.lakitu && this.position[1] < -5000) { this.reset(); return; }
      }
    }
    this.updateUI();
  }

  /** One tick of Lakitu's rescue; true while the kart is held (physics paused). */
  private rescueTick(): boolean {
    const lakitu = this.lakitu!;
    if (lakitu.tick()) {
      const dest = lakitu.destination(this.position);
      if (!dest) { this.reset(); return true; }
      const floor = this.world.findFloor(dest.pos[0], dest.pos[1] + 200, dest.pos[2]);
      if (floor.surface) dest.pos[1] = floor.height;
      this.position = dest.pos;
      this.yaw = dest.yaw;
      this.speed = 0;
      this.kart = new KartPhysics(dest.pos.map(v => v / this.unit) as Vec3, 0);
      this.kart.rotation = s16Rotation(dest.yaw);
      this.snapCamera();
      this.host.placeMario([...dest.pos], dest.yaw * 32768 / Math.PI);
      this.host.setPlayerVisible(false);
      this.host.toast('Lakitu fished you out.');
    }
    return lakitu.holding;
  }

  private updateUI(): void {
    const touch = isTouch();
    const racing = this.race?.active ?? false;
    const label = this.driving ? 'Get out' : this.distance() < 300 ? 'Get in' : 'Return to kart';
    const text = this.driving || this.distance() < 300 ? (touch ? label : `${label} · F`) : label;
    if (this.button.textContent !== text) this.button.textContent = text;
    this.modeText(racing ? 'RACING' : this.driving ? 'DRIVING' : 'ON FOOT');
    this.speedText(this.driving ? `${Math.round(this.kart.currentSpeed / TOP_SPEED * 100)}%` : '—');
    // Drift charge: two pips fill while counter-steering, then the boost flashes on release.
    const pips = Math.max(0, Math.min(2, this.kart.driftState));
    this.driftText(!this.driving ? '—' : this.kart.miniTurbo ? 'MINI-TURBO!' : this.kart.drifting ? `[${'#'.repeat(pips)}${'-'.repeat(2 - pips)}]${pips >= 2 ? ' RELEASE' : ''}` : '—');
    // Touch buttons say what they do here; A glows while a drift charges. In a race B uses the held item.
    relabelTouch({ A: this.driving ? 'HOP\nDRIFT' : 'JUMP', B: this.driving ? 'ITEM' : 'GET\nIN', Z: this.driving ? 'BRAKE' : undefined });
    tintTouch('A', 'kart-turbo', this.driving && this.kart.miniTurbo);
    tintTouch('A', 'kart-drift', this.driving && this.kart.drifting && !this.kart.miniTurbo);
  }

  frame(dt: number, alpha = 1): void {
    if (!this.ready) return;
    if (this.fade) this.fade.style.opacity = String(this.lakitu?.alpha ?? 0);
    this.sprite.visible = this.driving;
    this.parked.visible = !this.driving;
    this.parked.position.fromArray(this.position);
    this.parked.rotation.y = this.yaw;
    const k = this.kart;
    this.fx?.update(dt, {
      pos: [this.host.renderPos.x, this.host.renderPos.y, this.host.renderPos.z], yaw: this.yaw, driving: this.driving, midair: k.midair,
      drifting: k.drifting, hop: k.hop, driftState: k.driftState, boosting: this.soundFrame().boosting || k.miniTurbo,
      grass: k.grass, speedRatio: k.currentSpeed / TOP_SPEED,
    });
    this.audio?.setEngine(engineSound(this.soundFrame()));
    if (this.driving) {
      const p = this.host.renderPos;
      this.sprite.position.copy(p);
      // The real MK64 camera (kartCamera.ts), stepped at 60 Hz in course units and blended between 30 Hz ticks.
      const pose = this.poses ? lerpPose(this.poses[0], this.poses[1], Math.min(1, Math.max(0, alpha))) : this.camera.pose();
      const u = this.unit;
      this.host.camera.position.set(pose.eye[0] * u, pose.eye[1] * u, pose.eye[2] * u);
      this.host.camera.lookAt(pose.look[0] * u, pose.look[1] * u, pose.look[2] * u);
      if (Math.abs(pose.fov - this.lastFov) > 0.01) {
        this.lastFov = pose.fov;
        this.host.camera.fov = pose.fov;
        this.host.camera.updateProjectionMatrix();
      }
      const cameraAngle = Math.atan2(this.host.camera.position.x - p.x, this.host.camera.position.z - p.z);
      const turn = cameraAngle - this.yaw - Math.PI + this.race.playerSpin();
      const relative = Math.atan2(Math.sin(turn), Math.cos(turn));
      // MK64 uses denser views in the rear 45 degrees, then nine-degree steps.
      const degrees = Math.abs(relative) * 180 / Math.PI;
      const frame = Math.min(34, Math.round(degrees < 45 ? degrees / 2.86 : degrees / 9 + 15));
      const map = this.sprite.material.map!;
      map.repeat.x = (relative < 0 ? -1 : 1) / 35;
      map.offset.x = (frame + (relative < 0 ? 1 : 0)) / 35;
      // Steering lean, and a squash when the kart lands from a hop or a jump.
      const lean = this.kart.drifting ? 0.16 * Math.sign(this.kart.steerPosition) : this.kart.steerPosition / (1 << 16) / 53 * 0.08;
      this.sprite.material.rotation = -lean;
      if (this.wasMidair && !this.kart.midair) this.squash = 1;
      this.wasMidair = this.kart.midair;
      this.squash = Math.max(0, this.squash - dt * 6);
      this.sprite.scale.set(SPRITE_SIZE * (1 + 0.1 * this.squash), SPRITE_SIZE * (1 - 0.16 * this.squash), 1);
    }
    this.race.frame(dt, this.kart, alpha);
    this.sky?.update(this.host.camera, this.host.time);
  }

  debug(): Record<string, unknown> {
    return { ready: this.ready, mode: this.driving ? 'driving' : 'walking', speed: this.speed, reversing: this.kart?.backUp ?? false, drifting: this.kart?.drifting ?? false, miniTurbo: this.kart?.miniTurbo ?? false, kart: [...this.position], yaw: this.yaw, fov: this.camera.fov, camera: this.camera.initialised ? this.camera.pose() : null, fx: this.fx?.count ?? 0, audio: this.audio?.state() ?? '', triangles: this.meshes.reduce((n, m) => n + m.geometry.getAttribute('position').count / 3, 0), ...this.race?.debug() };
  }

  exit(): void {
    this.disposed = true;
    this.ready = false;
    this.fade?.remove();
    this.race?.dispose();
    this.fx?.dispose();
    this.audio?.dispose();
    this.host.camera.fov = this.baseFov;
    this.host.camera.updateProjectionMatrix();
    relabelTouch({ A: undefined, B: undefined, Z: undefined });
    tintTouch('A', 'kart-turbo', false);
    tintTouch('A', 'kart-drift', false);
    this.root.removeFromParent();
    this.root.traverse(node => {
      if (node instanceof THREE.Mesh || node instanceof THREE.Sprite) {
        if (node instanceof THREE.Mesh) node.geometry.dispose();
        for (const material of Array.isArray(node.material) ? node.material : [node.material]) this.materials.add(material);
      }
    });
    this.materials.forEach(m => m.dispose());
    this.textures.forEach(t => t.dispose());
    this.sky?.dispose();
    if (this.saved) {
      this.host.scene.fog = this.saved.fog;
      this.host.scene.background = this.saved.background;
      this.host.camera.far = this.saved.far;
      this.host.camera.updateProjectionMatrix();
      this.host.restoreMarioWorld();
      this.host.setPlayerVisible(true);
    }
  }
}
