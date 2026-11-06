import * as THREE from 'three';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js';
import { HorizontalTiltShiftShader } from 'three/addons/shaders/HorizontalTiltShiftShader.js';
import { VerticalTiltShiftShader } from 'three/addons/shaders/VerticalTiltShiftShader.js';
import { KOOPA_RACE_ROUTE } from '../../sim/behaviors/koopaTheQuick';
import { angleToRadians, atan2s, clamp, coss, sins, type Vec3 } from '../../sim/math';
import { ACT_FLAG_AIR, ACT_FREEFALL } from '../../sim/state';
import { crossesGoomba, touchesGoombaHurtbox, type GoombaEnemy } from '../goomba';
import type { PointerHandler } from '../Input';
import { loadSm64Asset, SM64_MODELS } from '../sm64Assets';
import { CROWD_TRICKS, MarioCrowd, PERSONALITIES, type CrowdAgent } from './marioCrowd';
import type { Experiment, ExperimentHost } from './types';
import { actionButton, dock, titleCard } from './ui';

const CASE_PINK = 0xff86b4;
const CASE_INNER = 0xffd3e6;
const CASE_TRIM = 0x8ad8e8;

interface GoombaState {
  enemy: GoombaEnemy;
  chase: number;
  target: CrowdAgent | null;
  respawn: number;
}

function gradientTexture(top: string, bottom: string): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 2;
  canvas.height = 256;
  const context = canvas.getContext('2d')!;
  const gradient = context.createLinearGradient(0, 0, 0, 256);
  gradient.addColorStop(0, top);
  gradient.addColorStop(1, bottom);
  context.fillStyle = gradient;
  context.fillRect(0, 0, 2, 256);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

function starShape(outer: number, inner: number): THREE.Shape {
  const shape = new THREE.Shape();
  for (let i = 0; i < 10; i++) {
    const r = i % 2 ? inner : outer;
    const a = Math.PI / 2 + i * Math.PI / 5;
    if (i === 0) shape.moveTo(Math.cos(a) * r, Math.sin(a) * r);
    else shape.lineTo(Math.cos(a) * r, Math.sin(a) * r);
  }
  shape.closePath();
  return shape;
}

/**
 * Diorama: the real course shrunk (by distance and a tilt-shift lens, not by
 * scaling physics) into a plastic compact case. Real Marios and Goombas live
 * on it; you fly around and poke them.
 */
export class DioramaExperiment implements Experiment {
  readonly id = 'diorama';
  readonly title = 'Diorama';
  readonly course = 'bob' as const;
  readonly freezesPlayer = true;
  readonly hidesSky = true;
  private host!: ExperimentHost;
  private crowd: MarioCrowd | null = null;
  private readonly caseGroup = new THREE.Group();
  private composer: EffectComposer | null = null;
  private tiltH: ShaderPass | null = null;
  private tiltV: ShaderPass | null = null;
  private goombas: GoombaState[] = [];
  private saved: {
    background: THREE.Scene['background']; fog: THREE.Fog | THREE.FogExp2 | null; far: number; near: number; fov: number;
    shadow?: { light: THREE.DirectionalLight; left: number; right: number; top: number; bottom: number; far: number };
  } | null = null;
  private background: THREE.Texture | null = null;
  private readonly eye = new THREE.Vector3();
  private yaw = 0;
  private pitch = -0.5;
  private flyVelocity = new THREE.Vector3();
  private drag: { x: number; y: number; moved: number; button: number } | null = null;
  private readonly raycaster = new THREE.Raycaster();
  private trickIndex = 0;
  private tiltShift = true;
  private pokes = 0;
  private bounds = new THREE.Box3();

  readonly pointer: PointerHandler = {
    down: event => {
      this.drag = { x: event.clientX, y: event.clientY, moved: 0, button: event.button };
      this.host.canvas.setPointerCapture?.(event.pointerId);
      return true;
    },
    move: event => {
      if (!this.drag) return false;
      const dx = event.clientX - this.drag.x;
      const dy = event.clientY - this.drag.y;
      this.drag.x = event.clientX;
      this.drag.y = event.clientY;
      this.drag.moved += Math.abs(dx) + Math.abs(dy);
      if (this.drag.moved > 4) {
        this.yaw -= dx * 0.0042;
        this.pitch = clamp(this.pitch - dy * 0.0036, -1.45, 1.2);
        this.host.canvas.style.cursor = 'grabbing';
      }
      return true;
    },
    up: () => {
      const drag = this.drag;
      this.drag = null;
      this.host.canvas.style.cursor = 'crosshair';
      if (drag && drag.moved <= 4 && drag.button === 0) this.poke(drag.x, drag.y);
    },
    wheel: delta => {
      const forward = new THREE.Vector3();
      this.host.camera.getWorldDirection(forward);
      this.eye.addScaledVector(forward, -delta * 14);
      return true;
    },
  };

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    const { scene, camera } = host;
    const sun = scene.children.find((child): child is THREE.DirectionalLight => child instanceof THREE.DirectionalLight && child.castShadow);
    this.saved = {
      background: scene.background, fog: scene.fog, far: camera.far, near: camera.near, fov: camera.fov,
      shadow: sun ? { light: sun, left: sun.shadow.camera.left, right: sun.shadow.camera.right, top: sun.shadow.camera.top, bottom: sun.shadow.camera.bottom, far: sun.shadow.camera.far } : undefined,
    };
    if (sun) {
      const c = sun.shadow.camera;
      c.left = c.bottom = -10500;
      c.right = c.top = 10500;
      c.far = 40000;
      c.updateProjectionMatrix();
    }
    this.background = gradientTexture('#fff3df', '#f4b9a6');
    scene.background = this.background;
    scene.fog = null;
    camera.far = 260000;
    // Viewing a 20k-unit board from far away: keep depth precision for the case's layers.
    camera.near = 40;
    camera.fov = 30;
    camera.updateProjectionMatrix();
    host.setPlayerVisible(false);
    // Centre the frozen player under the board so the sun's shadow box covers it.
    host.placeMario([0, 3000, 0]);
    this.bounds.setFromObject(host.courseScene);
    this.buildCase();
    this.buildComposer();
    this.resetCamera();
    host.canvas.style.cursor = 'crosshair';
    this.buildUi();

    for (const enemy of host.playground.goombas ?? []) {
      enemy.reset();
      this.goombas.push({ enemy, chase: 0, target: null, respawn: 0 });
    }

    let coin = null;
    try { coin = await loadSm64Asset(SM64_MODELS.coin); } catch { /* optional */ }
    this.crowd = new MarioCrowd({
      parent: scene,
      world: host.playground.collision,
      route: host.worldName === 'bob' ? KOOPA_RACE_ROUTE : undefined,
      coinTemplate: coin,
      coinCount: 18,
      center: [0, 0, 0],
      radius: 6800,
      particles: host.particles,
      sfx: host.sfx,
      listener: () => host.camera.position,
      seed: 0xd10a,
    });
    host.toast('DIORAMA · CLICK A MARIO, A GOOMBA, OR THE GROUND', 3600);
    await this.crowd.populate(26, [0, 0, 0], 6000);
  }

  private resetCamera(): void {
    const b = this.bounds;
    const center = b.getCenter(new THREE.Vector3());
    const size = b.getSize(new THREE.Vector3());
    const span = Math.max(size.x, size.z);
    this.eye.set(center.x + span * 0.3, center.y + span * 0.78, center.z + span * 1.2);
    const look = new THREE.Vector3(center.x, center.y - size.y * 0.2, center.z).sub(this.eye).normalize();
    this.yaw = Math.atan2(look.x, look.z);
    this.pitch = Math.asin(look.y);
    this.flyVelocity.set(0, 0, 0);
  }

  private buildCase(): void {
    const b = this.bounds;
    const size = b.getSize(new THREE.Vector3());
    const center = b.getCenter(new THREE.Vector3());
    const width = size.x + 1400;
    const depth = size.z + 1400;
    const thickness = 1400;
    // The course's lowest geometry (the void floor) sits inside the plastic tray.
    const top = Math.max(b.min.y, -900) + 60;
    const plastic = new THREE.MeshPhysicalMaterial({ color: CASE_PINK, roughness: 0.32, clearcoat: 0.8, clearcoatRoughness: 0.25, sheen: 0.4 });
    const inner = new THREE.MeshPhysicalMaterial({ color: CASE_INNER, roughness: 0.4, clearcoat: 0.6 });
    const trim = new THREE.MeshPhysicalMaterial({ color: CASE_TRIM, roughness: 0.3, clearcoat: 0.9 });

    const tray = new THREE.Mesh(new RoundedBoxGeometry(width, thickness, depth, 6, 520), plastic);
    tray.position.set(center.x, top - thickness / 2, center.z);
    tray.receiveShadow = true;
    tray.castShadow = true;
    this.caseGroup.add(tray);
    const rim = new THREE.Mesh(new RoundedBoxGeometry(width + 260, 180, depth + 260, 4, 560), trim);
    rim.position.set(center.x, top - thickness + 90, center.z);
    this.caseGroup.add(rim);

    // The lid stands open behind the course, hinged on the back edge.
    const hinge = new THREE.Group();
    hinge.position.set(center.x, top - thickness * 0.45, center.z - depth / 2);
    hinge.rotation.x = -Math.PI * 0.53;
    const lid = new THREE.Mesh(new RoundedBoxGeometry(width, 700, depth, 6, 520), plastic);
    lid.position.set(0, 0, depth / 2);
    lid.castShadow = true;
    hinge.add(lid);
    const panel = new THREE.Mesh(new RoundedBoxGeometry(width - 1300, 80, depth - 1300, 4, 380), inner);
    panel.position.set(0, -360, depth / 2);
    hinge.add(panel);
    const star = new THREE.Mesh(
      new THREE.ExtrudeGeometry(starShape(2600, 1150), { depth: 160, bevelEnabled: true, bevelSize: 90, bevelThickness: 90, bevelSegments: 3 }),
      new THREE.MeshPhysicalMaterial({ color: 0xffd24a, roughness: 0.25, metalness: 0.15, clearcoat: 1 }),
    );
    star.rotation.x = Math.PI / 2;
    star.position.set(0, -420, depth / 2);
    hinge.add(star);
    for (const side of [-1, 1]) {
      const knuckle = new THREE.Mesh(new THREE.CylinderGeometry(260, 260, 1600, 24), trim);
      knuckle.rotation.z = Math.PI / 2;
      knuckle.position.set(center.x + side * width * 0.3, top - thickness * 0.45, center.z - depth / 2);
      this.caseGroup.add(knuckle);
    }
    this.caseGroup.add(hinge);

    const table = new THREE.Mesh(
      new THREE.CircleGeometry(Math.max(width, depth) * 2.4, 64),
      new THREE.MeshStandardMaterial({ color: 0xc98e5c, roughness: 0.7 }),
    );
    table.rotation.x = -Math.PI / 2;
    table.position.set(center.x, top - thickness - 2, center.z);
    table.receiveShadow = true;
    this.caseGroup.add(table);
    this.caseGroup.name = 'diorama-case';
    this.host.scene.add(this.caseGroup);
  }

  private buildComposer(): void {
    const { renderer, scene, camera } = this.host;
    const composer = new EffectComposer(renderer);
    composer.addPass(new RenderPass(scene, camera));
    this.tiltH = new ShaderPass(HorizontalTiltShiftShader);
    this.tiltV = new ShaderPass(VerticalTiltShiftShader);
    composer.addPass(this.tiltH);
    composer.addPass(this.tiltV);
    composer.addPass(new OutputPass());
    this.composer = composer;
  }

  private buildUi(): void {
    titleCard(this.host.ui, '05', 'Diorama', 'Bob-omb Battlefield in a plastic compact. Everyone in it is real.', [
      ['Drag', 'look around'], ['WASD · Q/E', 'fly · down/up'], ['Shift', 'fly faster'], ['Click', 'poke a Mario, a Goomba, or drop a coin'],
    ]);
    const bar = dock(this.host.ui);
    bar.append(
      actionButton('Reset view', () => this.resetCamera(), 'R'),
      actionButton('Miniature lens', () => {
        this.tiltShift = !this.tiltShift;
        this.host.toast(this.tiltShift ? 'MINIATURE LENS ON' : 'MINIATURE LENS OFF', 1200);
      }, 'T'),
      actionButton('Shake the case', () => this.shake(), 'Space'),
    );
  }

  keydown(event: KeyboardEvent): boolean {
    if (event.code === 'KeyR') { this.resetCamera(); return true; }
    if (event.code === 'KeyT') { this.tiltShift = !this.tiltShift; return true; }
    if (event.code === 'Space') { event.preventDefault(); this.shake(); return true; }
    return false;
  }

  /** Every Mario hops at once, like the toy was bumped. */
  private shake(): void {
    for (const agent of this.crowd?.agents ?? []) {
      if (!(agent.body.runtime.action & ACT_FLAG_AIR)) this.crowd!.perform(agent, 'bigjump');
    }
    for (const goomba of this.goombas) if (goomba.enemy.tangible) goomba.enemy.alert(goomba.enemy.model.position.x, goomba.enemy.model.position.z);
    this.host.sfx.groundPound();
  }

  private poke(clientX: number, clientY: number): void {
    const { camera, canvas } = this.host;
    const crowd = this.crowd;
    if (!crowd) return;
    const agent = crowd.pick(clientX, clientY, camera, canvas, 40);
    if (agent) {
      const trick = CROWD_TRICKS[this.trickIndex++ % CROWD_TRICKS.length];
      crowd.perform(agent, trick);
      crowd.selected = agent;
      this.pokes++;
      this.host.particles.sparks([agent.body.pos[0], agent.body.pos[1] + 140, agent.body.pos[2]], 8);
      this.host.toast(`${PERSONALITIES[agent.personality].label.toUpperCase()} · ${agent.label.toUpperCase()}`, 1200);
      return;
    }
    const rect = canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
    this.raycaster.setFromCamera(ndc, camera);
    const goombaHit = this.raycaster.intersectObjects(this.goombas.filter(g => g.enemy.tangible).map(g => g.enemy.model), true)[0];
    if (goombaHit) {
      const goomba = this.goombas.find(g => {
        let found = false;
        g.enemy.model.traverse(node => { if (node === goombaHit.object) found = true; });
        return found;
      });
      if (goomba) {
        goomba.chase = 30 * 9;
        goomba.target = this.nearestMario(goomba.enemy.model.position);
        goomba.enemy.alert(goomba.enemy.model.position.x, goomba.enemy.model.position.z);
        this.pokes++;
        this.host.toast('GOOMBA · CHASE!', 1200);
        return;
      }
    }
    const ground = this.raycaster.intersectObjects([this.host.courseScene], true)[0];
    if (ground && crowd.dropCoin([ground.point.x, ground.point.y, ground.point.z])) {
      this.pokes++;
      this.host.toast('A COIN FALLS FROM THE SKY', 1200);
    }
  }

  private nearestMario(position: THREE.Vector3): CrowdAgent | null {
    let best: CrowdAgent | null = null;
    let distance = Infinity;
    for (const agent of this.crowd?.agents ?? []) {
      const d = Math.hypot(agent.body.pos[0] - position.x, agent.body.pos[2] - position.z);
      if (d < distance) { distance = d; best = agent; }
    }
    return best;
  }

  afterStep(): void {
    this.crowd?.update();
    this.stepGoombas();
  }

  private stepGoombas(): void {
    const crowd = this.crowd;
    if (!crowd) return;
    const world = this.host.playground.collision;
    for (const goomba of this.goombas) {
      const { enemy } = goomba;
      const p = enemy.model.position;
      if (!enemy.tangible) {
        enemy.step([p.x, p.y, p.z], world);
        if (++goomba.respawn > 30 * 12) { goomba.respawn = 0; enemy.reset(); }
        continue;
      }
      const nearest = this.nearestMario(p);
      if (goomba.chase > 0 && goomba.target) {
        goomba.chase--;
        // Charging: a straight run at the chosen Mario across the whole board.
        const t = goomba.target.body.pos;
        const yaw = atan2s(t[2] - p.z, t[0] - p.x);
        const next: Vec3 = [p.x + sins(yaw) * 13, p.y, p.z + coss(yaw) * 13];
        world.resolveAndReturnWallCollisions(next, 30, 40);
        const floor = world.findFloor(next[0], p.y + 120, next[2]);
        if (floor.surface && floor.height > p.y - 400) p.set(next[0], floor.height, next[2]);
        else goomba.chase = 0;
        enemy.model.rotation.y = angleToRadians(yaw);
        if (goomba.chase === 0) (enemy as unknown as { home?: THREE.Vector3 }).home?.copy(p);
      } else {
        enemy.step(nearest ? [...nearest.body.pos] : [p.x, p.y + 5000, p.z], world);
      }
      for (const agent of crowd.agents) {
        const m = agent.body.runtime;
        if (m.vel[1] < 0 && crossesGoomba(agent.body.previous, m.pos, enemy)) {
          enemy.defeat(true);
          m.vel[1] = 42;
          m.setAction(ACT_FREEFALL, 0);
          m.vel[1] = 42;
          this.host.particles.burst([p.x, p.y + 60, p.z], { count: 10, color: 0xffffff });
          if (goomba.target === agent) goomba.chase = 0;
          break;
        }
        if (touchesGoombaHurtbox(m.pos, 160, enemy) && m.takeDamage(1, [p.x, p.y, p.z])) {
          this.host.particles.sparks([m.pos[0], m.pos[1] + 80, m.pos[2]], 8);
          if (goomba.target === agent) goomba.chase = Math.min(goomba.chase, 20);
        }
      }
    }
  }

  frame(dt: number, alpha: number): void {
    const { input, camera } = this.host;
    const forward = new THREE.Vector3(Math.sin(this.yaw) * Math.cos(this.pitch), Math.sin(this.pitch), Math.cos(this.yaw) * Math.cos(this.pitch));
    const right = new THREE.Vector3(-Math.cos(this.yaw), 0, Math.sin(this.yaw));
    const speed = input.isDown('ShiftLeft') || input.isDown('ShiftRight') ? 16000 : 6500;
    const wish = new THREE.Vector3();
    if (input.isDown('KeyW') || input.isDown('ArrowUp')) wish.add(forward);
    if (input.isDown('KeyS') || input.isDown('ArrowDown')) wish.sub(forward);
    if (input.isDown('KeyD') || input.isDown('ArrowRight')) wish.add(right);
    if (input.isDown('KeyA') || input.isDown('ArrowLeft')) wish.sub(right);
    if (input.isDown('KeyE')) wish.y += 1;
    if (input.isDown('KeyQ')) wish.y -= 1;
    if (wish.lengthSq() > 0) wish.normalize().multiplyScalar(speed);
    this.flyVelocity.lerp(wish, 1 - Math.exp(-dt * 6));
    this.eye.addScaledVector(this.flyVelocity, dt);
    // Stay above the table and within a sensible orbit of the case.
    this.eye.y = Math.max(this.bounds.min.y + 200, this.eye.y);
    this.eye.clampLength(0, 90000);
    // Never fly through the ground: hover a little above whatever is below.
    const floor = this.host.playground.collision.findFloor(this.eye.x, this.eye.y + 50, this.eye.z);
    if (floor.surface && this.eye.y < floor.height + 120) this.eye.y = floor.height + 120;
    camera.position.copy(this.eye);
    camera.up.set(0, 1, 0);
    camera.lookAt(this.eye.clone().add(forward));
    this.host.input.consumeCameraInput();

    this.crowd?.render(alpha, dt, camera);
    this.updateTiltShift();
  }

  private updateTiltShift(): void {
    if (!this.tiltH || !this.tiltV) return;
    const size = this.host.renderer.getDrawingBufferSize(new THREE.Vector2());
    // Stronger blur when far away (the miniature look); none when skimming the grass.
    const height = this.eye.y - this.bounds.min.y;
    const strength = clamp((height - 1500) / 14000, 0, 1) * (this.tiltShift ? 1 : 0);
    this.tiltH.uniforms.h.value = (strength * 2.2) / Math.max(1, size.x);
    this.tiltV.uniforms.v.value = (strength * 2.2) / Math.max(1, size.y);
    this.tiltH.uniforms.r.value = 0.5;
    this.tiltV.uniforms.r.value = 0.5;
  }

  render(): boolean {
    const composer = this.composer;
    if (!composer) return false;
    const renderer = this.host.renderer;
    const size = renderer.getSize(new THREE.Vector2());
    composer.setPixelRatio(renderer.getPixelRatio());
    composer.setSize(size.x, size.y);
    composer.render();
    return true;
  }

  exit(): void {
    const host = this.host;
    this.crowd?.dispose();
    this.crowd = null;
    for (const goomba of this.goombas) goomba.enemy.reset();
    this.goombas = [];
    this.caseGroup.traverse(node => {
      if (node instanceof THREE.Mesh) {
        node.geometry.dispose();
        (node.material as THREE.Material).dispose();
      }
    });
    this.caseGroup.removeFromParent();
    this.caseGroup.clear();
    this.composer?.dispose();
    this.composer = null;
    const saved = this.saved;
    if (saved) {
      host.scene.background = saved.background;
      host.scene.fog = saved.fog;
      host.camera.far = saved.far;
      host.camera.near = saved.near;
      host.camera.fov = saved.fov;
      host.camera.updateProjectionMatrix();
      if (saved.shadow) {
        const c = saved.shadow.light.shadow.camera;
        Object.assign(c, { left: saved.shadow.left, right: saved.shadow.right, top: saved.shadow.top, bottom: saved.shadow.bottom, far: saved.shadow.far });
        c.updateProjectionMatrix();
      }
    }
    this.background?.dispose();
    host.setPlayerVisible(true);
    host.mario.resetToSpawn();
    host.placeMario([...host.mario.pos], host.mario.faceAngle[1]);
    host.canvas.style.cursor = 'grab';
  }

  debug(): Record<string, unknown> {
    return {
      ready: Boolean(this.crowd && !this.crowd.loading),
      marios: this.crowd?.agents.length ?? 0,
      goombas: this.goombas.filter(g => g.enemy.tangible).length,
      chasing: this.goombas.filter(g => g.chase > 0).length,
      coins: this.crowd?.coins.filter(c => c.active).length ?? 0,
      pokes: this.pokes,
      eye: this.eye.toArray(),
      tiltShift: this.tiltShift,
      selected: this.crowd?.selected ? this.crowd.describe(this.crowd.selected) : null,
    };
  }

  /** Test hook: poke the i-th Mario as if clicked. */
  pokeMario(index: number): boolean {
    const agent = this.crowd?.agents[index];
    if (!agent || !this.crowd) return false;
    this.crowd.perform(agent, CROWD_TRICKS[this.trickIndex++ % CROWD_TRICKS.length]);
    this.pokes++;
    return true;
  }
}

