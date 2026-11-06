import * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import { MarioRuntime } from '../sim/MarioRuntime';
import { atan2s } from '../sim/math';
import type { ControllerSnapshot } from '../sim/state';
import type { Vec3 } from '../sim/math';
import { createCrowdMario } from './crowdMarioModel';

export type MarioAgentPersonality = 'explorer' | 'racer' | 'follower' | 'chaos';

export interface MarioAgentDebug {
  id: number;
  personality: MarioAgentPersonality;
  pos: [number, number, number];
  physics: 'sprite' | 'runtime';
}

interface Agent {
  id: number;
  personality: MarioAgentPersonality;
  mesh: THREE.Mesh;
  pos: Vec3;
  velY: number;
  grounded: boolean;
  yaw: number;
  speed: number;
  timer: number;
  target: Vec3;
  bob: number;
}

/** Lightweight autonomous Mario agents for the A Hundred Marios experiment. */
export class HundredMarios {
  enabled = false;
  speedScale = 1;
  private agents: Agent[] = [];
  private fullAgents: Array<{ id: number; runtime: MarioRuntime; mesh: THREE.Mesh; timer: number; personality: MarioAgentPersonality }> = [];
  private nextId = 1;
  private accumulator = 0;
  selectedId: number | null = null;

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
    this.accumulator = 0;
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
      const mesh = createCrowdMario();
      mesh.position.set(pos[0], pos[1], pos[2]);
      this.scene.add(mesh);
      const agent: Agent = {
        id: this.nextId++,
        personality,
        mesh,
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

    for (let i = 0; i < 8; i++) {
      const angle = (i / 8) * Math.PI * 2;
      const x = origin[0] + Math.sin(angle) * 260;
      const z = origin[2] + Math.cos(angle) * 260;
      const floor = this.world.findFloor(x, origin[1] + 200, z);
      const runtime = new MarioRuntime(this.world, {
        pos: [x, floor.surface ? floor.height + 20 : origin[1] + 100, z],
        faceYaw: Math.round(angle * 32768 / Math.PI),
      });
      const mesh = createCrowdMario();
      mesh.position.set(runtime.pos[0], runtime.pos[1], runtime.pos[2]);
      this.scene.add(mesh);
      this.fullAgents.push({
        id: this.nextId++,
        runtime,
        mesh,
        timer: Math.random() * 30,
        personality: (['explorer', 'racer', 'follower', 'chaos'] as const)[i % 4],
      });
    }
  }

  select(id: number | null): void {
    this.selectedId = id;
  }

  pickAt(clientX: number, clientY: number, camera: THREE.Camera, canvas: HTMLCanvasElement, radius = 80): number | null {
    const rect = canvas.getBoundingClientRect();
    const point = new THREE.Vector3();
    let best: { id: number; distance: number } | null = null;
    const consider = (id: number, pos: Vec3) => {
      point.set(pos[0], pos[1] + 48, pos[2]).project(camera);
      if (point.z < -1 || point.z > 1) return;
      const x = rect.left + (point.x * 0.5 + 0.5) * rect.width;
      const y = rect.top + (-point.y * 0.5 + 0.5) * rect.height;
      const distance = Math.hypot(x - clientX, y - clientY);
      if (distance <= radius && (!best || distance < best.distance)) best = { id, distance };
    };
    for (const agent of this.agents) consider(agent.id, agent.pos);
    for (const agent of this.fullAgents) consider(agent.id, agent.runtime.pos);
    const picked = best as { id: number; distance: number } | null;
    this.selectedId = picked?.id ?? null;
    return this.selectedId;
  }

  selectedDebug(): MarioAgentDebug | null {
    const agent = this.agents.find(candidate => candidate.id === this.selectedId);
    if (agent) return { id: agent.id, personality: agent.personality, pos: [...agent.pos] as [number, number, number], physics: 'sprite' };
    const full = this.fullAgents.find(candidate => candidate.id === this.selectedId);
    if (full) return { id: full.id, personality: full.personality, pos: [...full.runtime.pos] as [number, number, number], physics: 'runtime' };
    return null;
  }

  cyclePersonality(): MarioAgentPersonality | null {
    const agent = this.agents.find(candidate => candidate.id === this.selectedId) ?? this.fullAgents.find(candidate => candidate.id === this.selectedId);
    if (!agent) return null;
    const personalities: MarioAgentPersonality[] = ['explorer', 'racer', 'follower', 'chaos'];
    agent.personality = personalities[(personalities.indexOf(agent.personality) + 1) % personalities.length];
    agent.timer = 99;
    return agent.personality;
  }

  step(marioPos: Vec3): void {
    if (!this.enabled) return;
    this.accumulator += this.speedScale;
    while (this.accumulator >= 1) {
      this.accumulator -= 1;
      this.stepOnce(marioPos);
    }
  }

  private stepOnce(marioPos: Vec3): void {
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
      agent.mesh.position.set(agent.pos[0], agent.pos[1], agent.pos[2]);
      agent.mesh.rotation.y = agent.yaw;
      agent.mesh.scale.setScalar(1);
    }

    for (const agent of this.fullAgents) this.stepFullAgent(agent, marioPos, dt);
  }

  private stepFullAgent(
    agent: { runtime: MarioRuntime; mesh: THREE.Mesh; timer: number; personality: MarioAgentPersonality },
    marioPos: Vec3,
    dt: number,
  ): void {
    agent.timer += dt;
    const runtime = agent.runtime;
    let target: Vec3;
    if (agent.personality === 'follower') {
      target = marioPos;
    } else if (agent.personality === 'racer') {
      target = [marioPos[0] + Math.sin(agent.timer) * 1400, marioPos[1], marioPos[2] + Math.cos(agent.timer) * 1400];
    } else {
      target = [runtime.pos[0] + Math.sin(agent.timer * 0.7) * 900, runtime.pos[1], runtime.pos[2] + Math.cos(agent.timer * 0.7) * 900];
    }
    const dx = target[0] - runtime.pos[0];
    const dz = target[2] - runtime.pos[2];
    const yaw = atan2s(dx, dz);
    const radians = yaw * Math.PI * 2 / 65536;
    const jump = agent.timer % 3.5 < 1 / 30;
    const controller: ControllerSnapshot = {
      stickX: Math.round(Math.cos(radians) * 64),
      stickY: Math.round(-Math.sin(radians) * 64),
      stickMag: 64,
      buttonDown: jump ? 0x8000 : 0,
      buttonPressed: jump ? 0x8000 : 0,
    };
    runtime.step(controller, 0);
    agent.mesh.position.set(runtime.pos[0], runtime.pos[1], runtime.pos[2]);
    agent.mesh.rotation.y = runtime.faceAngle[1] * Math.PI / 32768;
  }


  disable(): void {
    this.enabled = false;
    this.selectedId = null;
    for (const agent of this.agents) this.scene.remove(agent.mesh);
    for (const agent of this.fullAgents) this.scene.remove(agent.mesh);
    this.agents = [];
    this.fullAgents = [];
  }

  debug(): { enabled: boolean; count: number; fullPhysics: number; speed: number; selected: MarioAgentDebug | null; agents: MarioAgentDebug[] } {
    return {
      enabled: this.enabled,
      count: this.agents.length,
      fullPhysics: this.fullAgents.length,
      speed: this.speedScale,
      selected: this.selectedDebug(),
      agents: [
        ...this.agents.map(agent => ({
          id: agent.id,
          personality: agent.personality,
          pos: [...agent.pos] as [number, number, number],
          physics: 'sprite' as const,
        })),
        ...this.fullAgents.map(agent => ({
          id: agent.id,
          personality: agent.personality,
          pos: [...agent.runtime.pos] as [number, number, number],
          physics: 'runtime' as const,
        })),
      ],
    };
  }

  dispose(): void {
    this.disable();
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
