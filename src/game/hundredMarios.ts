import * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';

export type MarioAgentPersonality = 'explorer' | 'racer' | 'follower' | 'chaos';

export interface MarioAgentDebug {
  id: number;
  personality: MarioAgentPersonality;
  pos: [number, number, number];
}

interface Agent {
  id: number;
  personality: MarioAgentPersonality;
  sprite: THREE.Sprite;
  pos: Vec3;
  velY: number;
  grounded: boolean;
  yaw: number;
  speed: number;
  timer: number;
  target: Vec3;
  bob: number;
}

function makeMarioTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 32;
  canvas.height = 48;
  const ctx = canvas.getContext('2d')!;
  ctx.clearRect(0, 0, 32, 48);
  // Cap
  ctx.fillStyle = '#d92b2b';
  ctx.fillRect(7, 3, 18, 8);
  ctx.fillRect(4, 8, 24, 3);
  // Face
  ctx.fillStyle = '#f2c08a';
  ctx.fillRect(8, 11, 16, 11);
  ctx.fillStyle = '#151515';
  ctx.fillRect(11, 15, 3, 4);
  ctx.fillRect(19, 15, 3, 4);
  // Mustache and shirt
  ctx.fillStyle = '#4a2412';
  ctx.fillRect(10, 20, 12, 2);
  ctx.fillStyle = '#2b5fd1';
  ctx.fillRect(9, 23, 14, 13);
  ctx.fillStyle = '#e7e7e7';
  ctx.fillRect(6, 24, 4, 8);
  ctx.fillRect(22, 24, 4, 8);
  // Feet
  ctx.fillStyle = '#5b3218';
  ctx.fillRect(7, 36, 8, 5);
  ctx.fillRect(17, 36, 8, 5);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

/** Lightweight autonomous Mario agents for the A Hundred Marios experiment. */
export class HundredMarios {
  enabled = false;
  private agents: Agent[] = [];
  private nextId = 1;
  selectedId: number | null = null;
  private material: THREE.SpriteMaterial | null = null;
  private texture: THREE.CanvasTexture | null = null;

  constructor(private readonly scene: THREE.Scene, private world: CollisionWorld) {}

  setWorld(world: CollisionWorld): void {
    this.disable();
    this.world = world;
  }

  toggle(origin: Vec3, count = 48): boolean {
    this.enabled = !this.enabled;
    if (this.enabled) this.spawn(origin, count);
    else this.disable();
    return this.enabled;
  }

  spawn(origin: Vec3, count = 48): void {
    this.disable();
    this.enabled = true;
    for (let i = 0; i < count; i++) {
      const angle = Math.random() * Math.PI * 2;
      const radius = 160 + Math.random() * 900;
      const pos: Vec3 = [
        origin[0] + Math.sin(angle) * radius,
        origin[1] + 100,
        origin[2] + Math.cos(angle) * radius,
      ];
      const floor = this.world.findFloor(pos[0], pos[1], pos[2]);
      if (floor.surface) pos[1] = floor.height;
      const personality: MarioAgentPersonality = (['explorer', 'racer', 'follower', 'chaos'] as const)[i % 4];
      const sprite = new THREE.Sprite(this.spriteMaterial());
      sprite.scale.set(64, 96, 1);
      sprite.center.set(0.5, 0);
      sprite.position.set(pos[0], pos[1], pos[2]);
      this.scene.add(sprite);
      const agent: Agent = {
        id: this.nextId++,
        personality,
        sprite,
        pos,
        velY: 0,
        grounded: true,
        yaw: angle,
        speed: 8 + Math.random() * 10,
        timer: Math.random() * 30,
        target: [pos[0], pos[1], pos[2]],
        bob: Math.random() * Math.PI * 2,
      };
      this.agents.push(agent);
    }
  }

  select(id: number | null): void {
    this.selectedId = id;
  }

  pickAt(clientX: number, clientY: number, camera: THREE.Camera, canvas: HTMLCanvasElement, radius = 80): number | null {
    const rect = canvas.getBoundingClientRect();
    const point = new THREE.Vector3();
    let best: { id: number; distance: number } | null = null;
    for (const agent of this.agents) {
      point.set(agent.pos[0], agent.pos[1] + 48, agent.pos[2]).project(camera);
      if (point.z < -1 || point.z > 1) continue;
      const x = rect.left + (point.x * 0.5 + 0.5) * rect.width;
      const y = rect.top + (-point.y * 0.5 + 0.5) * rect.height;
      const distance = Math.hypot(x - clientX, y - clientY);
      if (distance <= radius && (!best || distance < best.distance)) best = { id: agent.id, distance };
    }
    this.selectedId = best?.id ?? null;
    return this.selectedId;
  }

  selectedDebug(): MarioAgentDebug | null {
    const agent = this.agents.find(candidate => candidate.id === this.selectedId);
    if (!agent) return null;
    return { id: agent.id, personality: agent.personality, pos: [...agent.pos] as [number, number, number] };
  }

  cyclePersonality(): MarioAgentPersonality | null {
    const agent = this.agents.find(candidate => candidate.id === this.selectedId);
    if (!agent) return null;
    const personalities: MarioAgentPersonality[] = ['explorer', 'racer', 'follower', 'chaos'];
    agent.personality = personalities[(personalities.indexOf(agent.personality) + 1) % personalities.length];
    agent.timer = 99;
    return agent.personality;
  }

  step(marioPos: Vec3): void {
    if (!this.enabled) return;
    const dt = 1 / 30;
    for (const agent of this.agents) {
      agent.timer += dt;
      if (agent.timer > 2.5 || this.distance2D(agent.pos, agent.target) < 70) {
        agent.timer = 0;
        agent.target = this.pickTarget(agent, marioPos);
      }

      const dx = agent.target[0] - agent.pos[0];
      const dz = agent.target[2] - agent.pos[2];
      const dist = Math.hypot(dx, dz);
      if (dist > 1) {
        agent.yaw = Math.atan2(dx, dz);
        const separation = this.separation(agent);
        const speed = Math.max(0, agent.speed - separation * 2.4);
        agent.pos[0] += Math.sin(agent.yaw) * speed * dt;
        agent.pos[2] += Math.cos(agent.yaw) * speed * dt;
      }

      agent.velY -= 4;
      agent.pos[1] += agent.velY;
      const floor = this.world.findFloor(agent.pos[0], agent.pos[1] + 100, agent.pos[2]);
      if (floor.surface && agent.pos[1] <= floor.height) {
        agent.pos[1] = floor.height;
        agent.velY = 0;
        agent.grounded = true;
        if (agent.personality === 'chaos' && Math.random() < 0.012) agent.velY = 22;
        else if (Math.random() < 0.004) agent.velY = 18;
      } else {
        agent.grounded = false;
      }

      if (this.distance2D(agent.pos, marioPos) > 3500 || agent.pos[1] < -1600) {
        agent.pos = [marioPos[0] + (Math.random() - 0.5) * 500, marioPos[1] + 200, marioPos[2] + (Math.random() - 0.5) * 500];
        agent.velY = 0;
      }

      agent.bob += dt * 8;
      agent.sprite.position.set(agent.pos[0], agent.pos[1], agent.pos[2]);
      agent.sprite.scale.y = 96 + Math.sin(agent.bob) * (agent.grounded ? 2 : 0);
    }
  }

  private spriteMaterial(): THREE.SpriteMaterial {
    if (this.material) return this.material;
    if (typeof document !== 'undefined') this.texture = makeMarioTexture();
    this.material = this.texture
      ? new THREE.SpriteMaterial({ map: this.texture, depthWrite: false })
      : new THREE.SpriteMaterial({ depthWrite: false });
    return this.material;
  }

  disable(): void {
    this.enabled = false;
    this.selectedId = null;
    for (const agent of this.agents) this.scene.remove(agent.sprite);
    this.agents = [];
  }

  debug(): { enabled: boolean; count: number; selected: MarioAgentDebug | null; agents: MarioAgentDebug[] } {
    return {
      enabled: this.enabled,
      count: this.agents.length,
      selected: this.selectedDebug(),
      agents: this.agents.map(agent => ({
        id: agent.id,
        personality: agent.personality,
        pos: [...agent.pos] as [number, number, number],
      })),
    };
  }

  dispose(): void {
    this.disable();
    this.material?.dispose();
    this.texture?.dispose();
  }

  private pickTarget(agent: Agent, marioPos: Vec3): Vec3 {
    switch (agent.personality) {
      case 'follower':
        return [marioPos[0] + (Math.random() - 0.5) * 500, marioPos[1], marioPos[2] + (Math.random() - 0.5) * 500];
      case 'racer':
        return [marioPos[0] + Math.sin(agent.id) * 1800, marioPos[1], marioPos[2] + Math.cos(agent.id) * 1800];
      case 'chaos':
        return [agent.pos[0] + (Math.random() - 0.5) * 1200, agent.pos[1], agent.pos[2] + (Math.random() - 0.5) * 1200];
      default:
        return [agent.pos[0] + (Math.random() - 0.5) * 2200, agent.pos[1], agent.pos[2] + (Math.random() - 0.5) * 2200];
    }
  }

  private separation(agent: Agent): number {
    let push = 0;
    for (const other of this.agents) {
      if (other === agent) continue;
      const distance = this.distance2D(agent.pos, other.pos);
      if (distance < 60 && distance > 0.01) push += (60 - distance) / 60;
    }
    return Math.min(2, push);
  }

  private distance2D(a: Vec3, b: Vec3): number {
    return Math.hypot(a[0] - b[0], a[2] - b[2]);
  }
}
