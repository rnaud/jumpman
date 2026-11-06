import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import type { CollisionWorld } from '../../sim/collision';
import type { MarioRuntime } from '../../sim/MarioRuntime';
import { atan2s, coss, sins, type Vec3 } from '../../sim/math';
import { SURFACE_DEATH_PLANE } from '../../sim/surfaces';
import {
  ACT_FLAG_AIR, ACT_FLAG_ATTACKING, ACT_FLAG_INVULNERABLE, ACT_FLAG_INTANGIBLE, ACT_FREEFALL,
  MARIO_KICKING, MARIO_PUNCHING, actionName,
} from '../../sim/state';
import type { Particles } from '../Particles';
import type { Sfx } from '../Sfx';
import { instantiateSm64 } from '../sm64Assets';
import { CrowdBatcher, CrowdMario, CrowdShadows, type DriveInput } from './crowd';
import { marioRoute } from './crowdRoute';

export type CrowdTrick = 'backflip' | 'longjump' | 'bigjump' | 'kicks';
export const CROWD_TRICKS: readonly CrowdTrick[] = ['bigjump', 'backflip', 'longjump', 'kicks'];

/** Frame-by-frame controller scripts for showing off on demand. */
function trickScript(trick: CrowdTrick, faceYaw: number): DriveInput[] {
  const hold = (frames: number, input: DriveInput) => Array.from({ length: frames }, () => ({ ...input }));
  switch (trick) {
    case 'backflip':
      return [{ yaw: null, crouch: true }, ...hold(3, { yaw: null, holdZ: true }), { yaw: null, holdZ: true, jump: true, holdA: true }, ...hold(14, { yaw: null, holdA: true })];
    case 'longjump':
      return [...hold(14, { yaw: faceYaw }), { yaw: faceYaw, crouch: true }, { yaw: faceYaw, jump: true, holdA: true }, ...hold(16, { yaw: faceYaw, holdA: true })];
    case 'kicks':
      return [{ yaw: null, punch: true }, ...hold(5, { yaw: null }), { yaw: null, punch: true }, ...hold(5, { yaw: null }), { yaw: null, punch: true }, ...hold(12, { yaw: null })];
    default:
      return [{ yaw: null, jump: true, holdA: true }, ...hold(16, { yaw: null, holdA: true })];
  }
}

export type Personality = 'racer' | 'hunter' | 'follower' | 'brawler' | 'explorer';
export const PERSONALITY_ORDER: readonly Personality[] = ['racer', 'hunter', 'follower', 'brawler', 'explorer'];
export const PERSONALITIES: Record<Personality, { label: string; color: number; overalls?: number; blurb: string }> = {
  racer: { label: 'Racer', color: 0xe23b2e, blurb: 'Runs the Koopa race route to the summit, lap after lap.' },
  hunter: { label: 'Coin hunter', color: 0xf5c518, blurb: 'Sprints to the nearest coin before anyone else.' },
  follower: { label: 'Follower', color: 0xff6fb5, blurb: 'Flocks around you and copies your jumps.' },
  brawler: { label: 'Brawler', color: 0x8c3fd9, overalls: 0x22222e, blurb: 'Guards a patch of turf and punches intruders.' },
  explorer: { label: 'Explorer', color: 0x17b3a6, blurb: 'Wanders anywhere, long-jumping and triple-jumping.' },
};

export type CrowdMix = 'mixed' | 'race' | 'gold' | 'parade' | 'brawl';
const MIX_WEIGHTS: Record<CrowdMix, Partial<Record<Personality, number>>> = {
  mixed: { racer: 0.3, hunter: 0.25, follower: 0.2, brawler: 0.1, explorer: 0.15 },
  race: { racer: 1 },
  gold: { hunter: 1 },
  parade: { follower: 1 },
  brawl: { brawler: 1 },
};

/** Deterministic personality assignment for agent index i under a mix. */
export function personalityFor(mix: CrowdMix, index: number, count: number, available: readonly Personality[] = PERSONALITY_ORDER): Personality {
  const weights = Object.entries(MIX_WEIGHTS[mix]).filter(([p]) => available.includes(p as Personality)) as Array<[Personality, number]>;
  if (!weights.length) return available[0] ?? 'explorer';
  const total = weights.reduce((sum, [, w]) => sum + w, 0);
  // Interleave so every slice of the crowd has the same blend.
  const slot = ((index * 0.61803398875) % 1) * total;
  let acc = 0;
  for (const [personality, weight] of weights) {
    acc += weight;
    if (slot < acc) return personality;
  }
  void count;
  return weights[weights.length - 1][0];
}

export function makeRng(seed: number): () => number {
  let state = (seed >>> 0) || 1;
  return () => {
    state ^= state << 13; state >>>= 0;
    state ^= state >>> 17;
    state ^= state << 5; state >>>= 0;
    return state / 0x100000000;
  };
}

/** A standable floor point near (cx, cz), or null. */
export function sampleFloor(world: CollisionWorld, cx: number, cz: number, radius: number, rng: () => number, ceiling = 9000): Vec3 | null {
  for (let attempt = 0; attempt < 40; attempt++) {
    const angle = rng() * Math.PI * 2;
    const r = Math.sqrt(rng()) * radius;
    const x = cx + Math.cos(angle) * r;
    const z = cz + Math.sin(angle) * r;
    if (Math.abs(x) > world.horizontalBoundary - 300 || Math.abs(z) > world.horizontalBoundary - 300) continue;
    const floor = world.findFloor(x, ceiling, z);
    if (!floor.surface || floor.surface.normal[1] < 0.86 || floor.surface.type === SURFACE_DEATH_PLANE) continue;
    if (world.waterLevel(x, z) > floor.height - 20) continue;
    return [x, floor.height, z];
  }
  return null;
}

interface Coin {
  group: THREE.Object3D;
  pos: Vec3;
  active: boolean;
  respawn: number;
  /** Dropped coins fall from the sky and attract everyone nearby. */
  fallVel: number;
  hot: number;
}

export interface CrowdAgent {
  id: number;
  name: string;
  body: CrowdMario;
  personality: Personality;
  goal: Vec3 | null;
  label: string;
  waypoint: number;
  laps: number;
  coins: number;
  hits: number;
  home: Vec3;
  goalTimer: number;
  celebrate: number;
  holdA: number;
  jumpAt: number;
  cooldown: number;
  stuck: number;
  stuckCheck: Vec3;
  stuckTicks: number;
  /** Recoveries at the current waypoint; repeated failures warp a racer onward. */
  recoveries: number;
  /** Closest approach to the current goal, and ticks since it improved. */
  bestDistance: number;
  noProgress: number;
  stalls: number;
  progressGoal: Vec3 | null;
  unstick: number;
  unstickYaw: number;
  flair: 'none' | 'longjump' | 'combo';
  combo: number;
  startTick: number;
  bestLap: number;
  trick: DriveInput[];
  /** Goals this Mario gave up on (unreachable coins), until the given tick. */
  avoid: Map<Coin, number>;
}

export interface CrowdAgentDebug {
  id: number;
  name: string;
  personality: Personality;
  label: string;
  action: string;
  pos: [number, number, number];
  speed: number;
  coins: number;
  laps: number;
  hits: number;
  bestLap: number | null;
}

export interface MarioCrowdOptions {
  parent: THREE.Object3D;
  world: CollisionWorld;
  /** Waypoints for racers; racers become explorers without one. */
  route?: readonly Vec3[];
  /** The player (or another protagonist) that followers flock to. */
  leader?: () => MarioRuntime | null;
  coinTemplate?: GLTF | null;
  coinCount?: number;
  /** Centre and radius used to sample spawn, coin and wander points. */
  center: Vec3;
  radius: number;
  particles?: Particles;
  sfx?: Sfx;
  /** Listener position for deciding whether a sound is worth playing. */
  listener?: () => THREE.Vector3;
  seed?: number;
  /** Racers stop at the summit instead of starting another lap. */
  singleLap?: boolean;
}

const NAMES = ['Mario', 'Mario', 'Mario'];
const SEPARATION = 120;
const GRID = 256;

/**
 * A crowd of full-physics Marios with simple, readable goals. The brain only
 * ever touches the stick and buttons; all movement is SM64's own.
 */
export class MarioCrowd {
  readonly agents: CrowdAgent[] = [];
  readonly coins: Coin[] = [];
  readonly shadows: CrowdShadows;
  /** Few draw calls for the whole crowd (one per Mario material). */
  private batcher = new CrowdBatcher(160);
  readonly root = new THREE.Group();
  speed = 1;
  mix: CrowdMix = 'mixed';
  private tick = 0;
  private accumulator = 0;
  private nextId = 1;
  private readonly rng: () => number;
  private generation = 0;
  private leaderWasAir = false;
  private readonly grid = new Map<number, CrowdAgent[]>();
  readonly stats = { summits: 0, coins: 0, hits: 0, bestLap: Infinity };
  selected: CrowdAgent | null = null;
  readonly ring: THREE.Mesh;
  loading = false;

  /** The racers' route, re-planned so Mario can actually run it. */
  readonly route: readonly Vec3[] | undefined;

  constructor(private readonly options: MarioCrowdOptions) {
    this.rng = makeRng(options.seed ?? 0x5eed);
    this.route = options.route?.length ? marioRoute(options.world, options.route) : undefined;
    this.root.name = 'mario-crowd';
    options.parent.add(this.root);
    this.shadows = new CrowdShadows(160);
    this.root.add(this.shadows.mesh, this.batcher.root);
    this.ring = new THREE.Mesh(
      new THREE.RingGeometry(70, 88, 40),
      new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.95, depthWrite: false, side: THREE.DoubleSide }),
    );
    this.ring.rotation.x = -Math.PI / 2;
    this.ring.visible = false;
    this.ring.renderOrder = 2;
    this.root.add(this.ring);
    this.scatterCoins(options.coinCount ?? 0);
  }

  get world(): CollisionWorld { return this.options.world; }

  private available(): Personality[] {
    return PERSONALITY_ORDER.filter(p => (p !== 'racer' || this.route?.length) && (p !== 'follower' || this.options.leader));
  }

  /** Replace the crowd with `count` Marios that rain in around `origin`. */
  async populate(count: number, origin: Vec3 = this.options.center, spread = 1600): Promise<void> {
    const generation = ++this.generation;
    this.clearAgents();
    this.loading = true;
    try {
      await CrowdMario.preload();
      const available = this.available();
      for (let i = 0; i < count; i++) {
        if (generation !== this.generation) return;
        const personality = personalityFor(this.mix, i, count, available);
        const spot = sampleFloor(this.world, origin[0], origin[2], spread + (i / count) * spread, this.rng)
          ?? sampleFloor(this.world, this.options.center[0], this.options.center[2], this.options.radius, this.rng);
        if (!spot) continue;
        const drop: Vec3 = [spot[0], spot[1] + 500 + this.rng() * 1100, spot[2]];
        const look = PERSONALITIES[personality];
        const body = await CrowdMario.create(this.world, { pos: drop, faceYaw: Math.floor(this.rng() * 65536) - 32768, action: ACT_FREEFALL }, { cap: look.color, overalls: look.overalls });
        if (generation !== this.generation) return;
        this.root.add(body.root);
        this.batcher.add(body);
        const id = this.nextId++;
        this.agents.push({
          id, name: `${NAMES[id % NAMES.length]} #${id}`, body, personality,
          goal: null, label: 'Dropping in', waypoint: 0, laps: 0, coins: 0, hits: 0,
          home: spot, goalTimer: 0, celebrate: 0, holdA: 0, jumpAt: -1, cooldown: 0,
          stuck: 0, stuckCheck: [...spot], stuckTicks: 0, recoveries: 0, bestDistance: Infinity, noProgress: 0, stalls: 0, progressGoal: null, unstick: 0, unstickYaw: 0,
          flair: 'none', combo: 0, startTick: this.tick, bestLap: Infinity, trick: [], avoid: new Map(),
        });
        if (personality === 'racer') this.startRace(this.agents[this.agents.length - 1], true);
        // Yield every few spawns so the drop-in animates instead of stalling a frame.
        if (i % 6 === 5) await new Promise(resolve => requestAnimationFrame(resolve));
      }
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }

  /** Place one Mario per spot, standing (a starting grid rather than a drop-in). */
  async populateAt(spots: readonly Vec3[], personality: Personality, faceYaw: number): Promise<void> {
    const generation = ++this.generation;
    this.clearAgents();
    this.loading = true;
    try {
      await CrowdMario.preload();
      for (const spot of spots) {
        if (generation !== this.generation) return;
        const look = PERSONALITIES[personality];
        const body = await CrowdMario.create(this.world, { pos: [...spot], faceYaw }, { cap: look.color, overalls: look.overalls });
        if (generation !== this.generation) return;
        this.root.add(body.root);
        this.batcher.add(body);
        const id = this.nextId++;
        this.agents.push({
          id, name: `Rival #${this.agents.length + 1}`, body, personality,
          goal: null, label: 'On the grid', waypoint: 0, laps: 0, coins: 0, hits: 0,
          home: [...spot], goalTimer: 0, celebrate: 0, holdA: 0, jumpAt: -1, cooldown: 0,
          stuck: 0, stuckCheck: [...spot], stuckTicks: 0, recoveries: 0, bestDistance: Infinity, noProgress: 0, stalls: 0, progressGoal: null,
          unstick: 0, unstickYaw: 0,
          flair: 'none', combo: 0, startTick: this.tick, bestLap: Infinity, trick: [], avoid: new Map(),
        });
        if (personality === 'racer') this.startRace(this.agents[this.agents.length - 1], true);
      }
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }

  setMix(mix: CrowdMix): void {
    this.mix = mix;
    const available = this.available();
    this.agents.forEach((agent, index) => this.setPersonality(agent, personalityFor(mix, index, this.agents.length, available)));
  }

  setPersonality(agent: CrowdAgent, personality: Personality): void {
    if (!this.available().includes(personality)) return;
    agent.personality = personality;
    agent.goal = null;
    agent.goalTimer = 0;
    agent.home = [...agent.body.pos];
    const look = PERSONALITIES[personality];
    agent.body.setLook({ cap: look.color, overalls: look.overalls });
    if (personality === 'racer') this.startRace(agent, true);
  }

  cyclePersonality(agent: CrowdAgent): Personality {
    const available = this.available();
    const next = available[(available.indexOf(agent.personality) + 1) % available.length];
    this.setPersonality(agent, next);
    return next;
  }

  /** Make a Mario show off with a scripted move. */
  perform(agent: CrowdAgent, trick: CrowdTrick): void {
    agent.trick = trickScript(trick, agent.body.runtime.faceAngle[1]);
    agent.label = ({ backflip: 'Backflip!', longjump: 'Long jump!', bigjump: 'Jump!', kicks: 'Punch combo!' } as const)[trick];
  }

  /** Drop a coin from the sky; every Mario nearby races for it. */
  dropCoin(at: Vec3): boolean {
    const floor = this.world.findFloor(at[0], at[1] + 400, at[2]);
    if (!floor.surface) return false;
    const coin = this.makeCoin([at[0], floor.height + 1400, at[2]]);
    if (!coin) return false;
    coin.fallVel = 0;
    coin.hot = 30 * 20;
    coin.respawn = -1;
    this.coins.push(coin);
    return true;
  }

  private makeCoin(pos: Vec3): Coin | null {
    const template = this.options.coinTemplate;
    if (!template) return null;
    const group = instantiateSm64(template);
    group.scale.multiplyScalar(0.9);
    group.userData.objectBillboard = true;
    group.traverse(node => { if (node instanceof THREE.Mesh) node.castShadow = false; });
    group.position.fromArray(pos);
    this.root.add(group);
    return { group, pos: [...pos], active: true, respawn: 0, fallVel: 0, hot: 0 };
  }

  private scatterCoins(count: number): void {
    const { center, radius } = this.options;
    for (let i = 0; i < count; i++) {
      const spot = sampleFloor(this.world, center[0], center[2], radius, this.rng);
      if (!spot) continue;
      const coin = this.makeCoin([spot[0], spot[1] + 70, spot[2]]);
      if (coin) this.coins.push(coin);
    }
  }

  private startRace(agent: CrowdAgent, nearest: boolean): void {
    const route = this.route;
    if (!route?.length) return;
    let best = 0;
    if (nearest) {
      let distance = Infinity;
      route.forEach((point, index) => {
        const d = Math.hypot(point[0] - agent.body.pos[0], (point[1] - agent.body.pos[1]) * 2, point[2] - agent.body.pos[2]);
        if (d < distance && index < route.length - 2) { distance = d; best = index; }
      });
    }
    agent.waypoint = best;
    agent.startTick = this.tick;
  }

  /** Advance the crowd by one game tick, scaled by `speed`. */
  update(): void {
    this.accumulator += this.speed;
    let steps = 0;
    while (this.accumulator >= 1 && steps < 4) {
      this.accumulator -= 1;
      this.stepOnce();
      steps++;
    }
  }

  private stepOnce(): void {
    this.tick++;
    this.buildGrid();
    const leader = this.options.leader?.() ?? null;
    if (leader) this.echoLeaderJump(leader);
    for (const agent of this.agents) this.think(agent, leader);
    this.resolveHits(leader);
    this.stepCoins();
  }

  private echoLeaderJump(leader: MarioRuntime): void {
    const air = Boolean(leader.action & ACT_FLAG_AIR);
    if (air && !this.leaderWasAir && leader.vel[1] > 20) {
      for (const agent of this.agents) {
        if (agent.personality !== 'follower') continue;
        const distance = Math.hypot(agent.body.pos[0] - leader.pos[0], agent.body.pos[2] - leader.pos[2]);
        if (distance > 2500) continue;
        // A jump wave ripples outward from the player.
        agent.jumpAt = this.tick + 2 + Math.floor(distance / 70);
      }
    }
    this.leaderWasAir = air;
  }

  private buildGrid(): void {
    this.grid.clear();
    for (const agent of this.agents) {
      const key = this.cellKey(agent.body.pos[0], agent.body.pos[2]);
      let list = this.grid.get(key);
      if (!list) this.grid.set(key, list = []);
      list.push(agent);
    }
  }

  private cellKey(x: number, z: number): number {
    return (Math.floor(x / GRID) + 4096) * 8192 + (Math.floor(z / GRID) + 4096);
  }

  private neighbours(pos: Vec3, radius: number, visit: (other: CrowdAgent, distance: number) => void): void {
    const x0 = Math.floor((pos[0] - radius) / GRID), x1 = Math.floor((pos[0] + radius) / GRID);
    const z0 = Math.floor((pos[2] - radius) / GRID), z1 = Math.floor((pos[2] + radius) / GRID);
    for (let x = x0; x <= x1; x++) for (let z = z0; z <= z1; z++) {
      const list = this.grid.get((x + 4096) * 8192 + (z + 4096));
      if (!list) continue;
      for (const other of list) {
        const d = Math.hypot(other.body.pos[0] - pos[0], other.body.pos[2] - pos[2]);
        if (d <= radius && Math.abs(other.body.pos[1] - pos[1]) < 200) visit(other, d);
      }
    }
  }

  private think(agent: CrowdAgent, leader: MarioRuntime | null): void {
    const m = agent.body.runtime;
    const pos = m.pos;
    const grounded = !(m.action & ACT_FLAG_AIR);
    agent.goalTimer++;
    if (agent.cooldown > 0) agent.cooldown--;

    // Knockbacks and other intangible states play out untouched.
    if (m.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE)) {
      agent.label = 'Knocked back';
      agent.body.drive({ yaw: null });
      return;
    }
    if (agent.label === 'Dropping in' && grounded) agent.label = '';
    const scripted = agent.trick.shift();
    if (scripted) {
      agent.body.drive(scripted);
      agent.stuckTicks = 0;
      return;
    }

    let mag = 64;
    let jump = false;
    let punch = false;
    let crouch = false;
    const hot = this.hottestCoin(pos);
    if (hot) {
      agent.goal = hot.pos;
      agent.label = 'Chasing a dropped coin';
    } else switch (agent.personality) {
      case 'racer': {
        const route = this.route!;
        if (agent.celebrate > 0) {
          agent.celebrate--;
          agent.label = 'Summit! Celebrating';
          agent.goal = null;
          if (agent.celebrate % 24 === 0 && grounded) jump = true;
          if (this.options.singleLap) { agent.celebrate = Math.max(agent.celebrate, 2); break; }
          if (agent.celebrate === 0) {
            agent.laps++;
            const start = sampleFloor(this.world, route[0][0], route[0][2], 260, this.rng) ?? route[0];
            agent.body.teleport([start[0], start[1] + 300, start[2]]);
            this.options.particles?.sparks(agent.body.pos, 16);
            agent.waypoint = 1;
            agent.startTick = this.tick;
          }
          break;
        }
        const target = route[Math.min(agent.waypoint, route.length - 1)];
        agent.goal = target;
        const dx = target[0] - pos[0], dz = target[2] - pos[2];
        if (Math.hypot(dx, dz) < 240 && Math.abs(target[1] - pos[1]) < 450) {
          agent.waypoint++;
          agent.recoveries = 0;
          agent.stuck = 0;
          // Respawn on the last checkpoint when the race knocks a Mario off the mountain.
          m.spawn = { pos: [...target], faceYaw: m.faceAngle[1] };
          if (agent.waypoint >= route.length) {
            agent.celebrate = 90;
            const lap = (this.tick - agent.startTick) / 30;
            agent.bestLap = Math.min(agent.bestLap, lap);
            this.stats.summits++;
            this.stats.bestLap = Math.min(this.stats.bestLap, lap);
            this.options.particles?.sparks([pos[0], pos[1] + 100, pos[2]], 24);
            this.playNear(pos, sfx => sfx.starSound());
          }
        }
        agent.label = `Racing · checkpoint ${Math.min(agent.waypoint + 1, route.length)}/${route.length}`;
        break;
      }
      case 'hunter': {
        const coin = this.nearestCoin(pos, 7000, agent);
        if (coin) {
          agent.goal = coin.pos;
          const d = Math.hypot(coin.pos[0] - pos[0], coin.pos[2] - pos[2]);
          agent.label = `Hunting a coin · ${Math.round(d / 100)} m`;
          // Leap for coins that float above head height.
          if (d < 160 && coin.pos[1] - pos[1] > 110 && grounded) jump = true;
        } else this.explore(agent, 'Looking for coins');
        break;
      }
      case 'follower': {
        if (!leader) { this.explore(agent, 'Wandering'); break; }
        const angle = agent.id * 2.39996;
        const radius = 280 + (agent.id % 7) * 110;
        const goal: Vec3 = [leader.pos[0] + Math.cos(angle) * radius, leader.pos[1], leader.pos[2] + Math.sin(angle) * radius];
        agent.goal = goal;
        const d = Math.hypot(goal[0] - pos[0], goal[2] - pos[2]);
        mag = d < 60 ? 0 : d < 400 ? 24 + d / 10 : 64;
        agent.label = d > 3000 ? 'Catching up with you' : 'Following you';
        if (agent.jumpAt === this.tick && grounded) { jump = true; agent.holdA = 10; }
        break;
      }
      case 'brawler': {
        let intruder: { pos: Vec3; distance: number } | null = null;
        this.neighbours(pos, 900, (other, distance) => {
          if (other === agent || other.personality === 'brawler') return;
          if (Math.hypot(other.body.pos[0] - agent.home[0], other.body.pos[2] - agent.home[2]) > 900) return;
          if (!intruder || distance < intruder.distance) intruder = { pos: other.body.pos, distance };
        });
        if (leader && Math.hypot(leader.pos[0] - agent.home[0], leader.pos[2] - agent.home[2]) < 700) {
          const distance = Math.hypot(leader.pos[0] - pos[0], leader.pos[2] - pos[2]);
          const current = intruder as { distance: number } | null;
          if (!current || distance < current.distance) intruder = { pos: leader.pos, distance };
        }
        const found = intruder as { pos: Vec3; distance: number } | null;
        if (found) {
          agent.goal = found.pos;
          agent.label = 'Chasing an intruder';
          if (found.distance < 150 && grounded && agent.cooldown === 0) {
            punch = true;
            agent.cooldown = 20;
          }
        } else {
          if (!agent.goal || agent.goalTimer > 150 || Math.hypot(agent.goal[0] - pos[0], agent.goal[2] - pos[2]) < 120) {
            agent.goal = sampleFloor(this.world, agent.home[0], agent.home[2], 600, this.rng) ?? agent.home;
            agent.goalTimer = 0;
          }
          mag = 34;
          agent.label = 'Guarding turf';
        }
        break;
      }
      case 'explorer':
        this.explore(agent, 'Exploring');
        // Show off: long jumps on long straights, chained triple jumps.
        if (grounded && agent.flair === 'none' && m.forwardVel > 26 && agent.goal
            && Math.hypot(agent.goal[0] - pos[0], agent.goal[2] - pos[2]) > 1400 && this.rng() < 0.02) {
          agent.flair = 'longjump';
          crouch = true;
        } else if (agent.flair === 'longjump') {
          jump = true;
          agent.holdA = 6;
          agent.flair = 'none';
        } else if (grounded && agent.body.landedTicks === 1 && agent.combo > 0 && agent.combo < 3) {
          jump = true;
          agent.holdA = 12;
          agent.combo++;
        } else if (grounded && m.forwardVel > 22 && agent.combo === 0 && this.rng() < 0.012) {
          jump = true;
          agent.holdA = 12;
          agent.combo = 1;
        } else if (grounded && agent.body.landedTicks > 3) agent.combo = 0;
        break;
    }

    let yaw: number | null = null;
    if (agent.goal) {
      let vx = agent.goal[0] - pos[0];
      let vz = agent.goal[2] - pos[2];
      const distance = Math.hypot(vx, vz);
      if (distance < 40 && agent.personality !== 'racer') mag = 0;
      if (distance > 0.01) { vx /= distance; vz /= distance; }
      // Crowd avoidance: steer away from Marios that are too close.
      let sx = 0, sz = 0;
      this.neighbours(pos, SEPARATION, (other, d) => {
        if (other === agent || d < 0.01) return;
        const push = (SEPARATION - d) / SEPARATION;
        sx += (pos[0] - other.body.pos[0]) / d * push;
        sz += (pos[2] - other.body.pos[2]) / d * push;
      });
      vx += sx * 1.4;
      vz += sz * 1.4;
      if (mag > 0) yaw = atan2s(vz, vx);
    }

    // Unsticking: jump at walls, then try another heading, then give up and respawn.
    if (agent.unstick > 0) {
      agent.unstick--;
      yaw = agent.unstickYaw;
      mag = 64;
      agent.label = 'Finding another way';
      if (agent.unstick === 12 && grounded) { jump = true; agent.holdA = 12; }
    } else if (yaw !== null && grounded && m.wall && m.forwardVel < 12 && agent.cooldown === 0) {
      jump = true;
      agent.holdA = 14;
      agent.cooldown = 12;
    }
    // Goal progress: oscillating in place (jumping at a wall) still counts as stuck.
    if (agent.goal && (agent.personality === 'racer' || agent.personality === 'hunter' || agent.personality === 'explorer') && agent.celebrate === 0) {
      if (agent.progressGoal !== agent.goal) {
        agent.progressGoal = agent.goal;
        agent.bestDistance = Infinity;
        agent.noProgress = 0;
        agent.stalls = 0;
      }
      const d = Math.hypot(agent.goal[0] - pos[0], (agent.goal[1] - pos[1]) * 0.5, agent.goal[2] - pos[2]);
      if (d < agent.bestDistance - 80) { agent.bestDistance = d; agent.noProgress = 0; }
      else if (++agent.noProgress > 150 && agent.unstick === 0) {
        agent.noProgress = 0;
        agent.bestDistance = d;
        if (++agent.stalls >= 3) { agent.stalls = 0; this.recover(agent); return; }
        const coin = this.coins.find(c => c.pos === agent.goal);
        if (coin) agent.avoid.set(coin, this.tick + 30 * 25);
        if (agent.personality === 'explorer') agent.goalTimer = 1e9;
        agent.unstick = 28;
        agent.unstickYaw = (atan2s(agent.goal[2] - pos[2], agent.goal[0] - pos[0]) + 0x4000 * (this.rng() < 0.5 ? 1 : -1)) | 0;
      }
    }
    if (++agent.stuckTicks >= 24) {
      agent.stuckTicks = 0;
      const moved = Math.hypot(pos[0] - agent.stuckCheck[0], pos[2] - agent.stuckCheck[2]);
      agent.stuckCheck = [...pos];
      if (yaw !== null && mag > 30 && moved < 50 && agent.celebrate === 0) {
        agent.stuck++;
        if (agent.stuck === 1 && grounded) { jump = true; agent.holdA = 14; }
        else if (agent.stuck >= 2 && agent.stuck < 7) {
          // Give up on a goal that keeps blocking us; pick something else for a while.
          if (agent.stuck >= 3 && agent.goal) {
            const coin = this.coins.find(c => c.pos === agent.goal);
            if (coin) agent.avoid.set(coin, this.tick + 30 * 25);
            if (agent.personality === 'explorer' || agent.personality === 'brawler') agent.goalTimer = 1e9;
          }
          agent.unstick = 28;
          agent.unstickYaw = (yaw + 0x4000 * (this.rng() < 0.5 ? 1 : -1) + Math.floor((this.rng() - 0.5) * 0x3000)) | 0;
        } else if (agent.stuck >= 7) {
          this.recover(agent);
          return;
        }
      } else agent.stuck = Math.max(0, agent.stuck - 1);
    }

    if (agent.holdA > 0) agent.holdA--;
    agent.body.drive({ yaw, mag, jump, holdA: agent.holdA > 0, punch, crouch });
    if (agent.body.runtime.respawns > 0) {
      agent.body.runtime.respawns = 0;
      agent.body.syncPrevious();
    }
  }

  private explore(agent: CrowdAgent, label: string): void {
    const pos = agent.body.pos;
    const reached = agent.goal && Math.hypot(agent.goal[0] - pos[0], agent.goal[2] - pos[2]) < 180;
    if (!agent.goal || reached || agent.goalTimer > 30 * 12) {
      const roll = this.rng();
      const route = this.route;
      if (roll < 0.3 && route?.length) agent.goal = route[Math.floor(this.rng() * route.length)];
      else agent.goal = sampleFloor(this.world, pos[0], pos[2], 3200, this.rng)
        ?? sampleFloor(this.world, this.options.center[0], this.options.center[2], this.options.radius, this.rng);
      agent.goalTimer = 0;
    }
    agent.label = label;
  }

  private recover(agent: CrowdAgent): void {
    const route = this.route;
    let anchor = agent.home;
    if (agent.personality === 'racer' && route?.length) {
      // Twice stuck on the same checkpoint: warp onto it instead of retrying forever.
      if (++agent.recoveries >= 2) {
        agent.recoveries = 0;
        anchor = route[Math.min(agent.waypoint, route.length - 1)];
      } else anchor = route[Math.max(0, agent.waypoint - 1)];
    }
    const spot = sampleFloor(this.world, anchor[0], anchor[2], 300, this.rng) ?? anchor;
    agent.body.teleport([spot[0], spot[1] + 200, spot[2]]);
    agent.stuck = 0;
    agent.goal = null;
    agent.label = 'Warped back';
    this.options.particles?.sparks(agent.body.pos, 10);
  }

  private hottestCoin(pos: Vec3): Coin | null {
    let best: Coin | null = null;
    let distance = 4200;
    for (const coin of this.coins) {
      if (!coin.active || coin.hot <= 0) continue;
      const d = Math.hypot(coin.pos[0] - pos[0], coin.pos[2] - pos[2]);
      if (d < distance) { distance = d; best = coin; }
    }
    return best;
  }

  private nearestCoin(pos: Vec3, range: number, agent?: CrowdAgent): Coin | null {
    let best: Coin | null = null;
    let distance = range;
    for (const coin of this.coins) {
      if (!coin.active) continue;
      const until = agent?.avoid.get(coin);
      if (until !== undefined) {
        if (until > this.tick) continue;
        agent!.avoid.delete(coin);
      }
      const d = Math.hypot(coin.pos[0] - pos[0], (coin.pos[1] - pos[1]) * 1.5, coin.pos[2] - pos[2]);
      if (d < distance) { distance = d; best = coin; }
    }
    return best;
  }

  private stepCoins(): void {
    for (const coin of this.coins) {
      if (!coin.active) {
        if (coin.respawn > 0 && --coin.respawn === 0) {
          const { center, radius } = this.options;
          const spot = sampleFloor(this.world, center[0], center[2], radius, this.rng);
          if (spot) {
            coin.pos = [spot[0], spot[1] + 70, spot[2]];
            coin.group.position.fromArray(coin.pos);
            coin.active = true;
            coin.group.visible = true;
          } else coin.respawn = 30;
        }
        continue;
      }
      if (coin.hot > 0) coin.hot--;
      if (coin.respawn < 0) {
        const floor = this.world.findFloor(coin.pos[0], coin.pos[1] + 50, coin.pos[2]);
        const rest = floor.height + 70;
        if (coin.pos[1] > rest) {
          coin.fallVel = Math.max(-60, coin.fallVel - 3);
          coin.pos[1] = Math.max(rest, coin.pos[1] + coin.fallVel);
          if (coin.pos[1] === rest) this.playNear(coin.pos, sfx => sfx.coinBounce(coin.pos));
        }
      }
      coin.group.position.set(coin.pos[0], coin.pos[1] + Math.sin(this.tick * 0.15 + coin.pos[0]) * 4, coin.pos[2]);
      // Any Mario that touches a coin takes it.
      this.neighbours(coin.pos, 90, other => {
        if (!coin.active || Math.abs(other.body.pos[1] + 60 - coin.pos[1]) > 140) return;
        coin.active = false;
        coin.group.visible = false;
        other.coins++;
        this.stats.coins++;
        this.options.particles?.sparks(coin.pos, 8);
        this.playNear(coin.pos, sfx => sfx.coin());
        if (coin.respawn < 0) {
          coin.group.removeFromParent();
          this.coins.splice(this.coins.indexOf(coin), 1);
        } else coin.respawn = 30 * 18;
      });
    }
  }

  /** Brawler punches, player punches and stomps all turn into real knockbacks. */
  private resolveHits(leader: MarioRuntime | null): void {
    const attacking = (m: MarioRuntime) => (m.flags & (MARIO_PUNCHING | MARIO_KICKING)) !== 0 || (m.action & ACT_FLAG_ATTACKING) !== 0;
    const hitFrom = (attacker: MarioRuntime, victim: MarioRuntime, reach: number) => {
      const dx = victim.pos[0] - attacker.pos[0], dz = victim.pos[2] - attacker.pos[2];
      const d = Math.hypot(dx, dz);
      if (d > reach || Math.abs(victim.pos[1] - attacker.pos[1]) > 140) return false;
      const facing = (dx * sins(attacker.faceAngle[1]) + dz * coss(attacker.faceAngle[1])) / Math.max(1, d);
      return d < 60 || facing > 0.25;
    };
    for (const agent of this.agents) {
      const m = agent.body.runtime;
      if (!attacking(m)) continue;
      this.neighbours(m.pos, 170, other => {
        if (other === agent || !hitFrom(m, other.body.runtime, 170)) return;
        if (other.body.runtime.takeDamage(2, m.pos)) {
          agent.hits++;
          this.stats.hits++;
          this.options.particles?.sparks([other.body.pos[0], other.body.pos[1] + 80, other.body.pos[2]], 10);
          this.playNear(other.body.pos, sfx => sfx.bonk());
        }
      });
      if (leader && hitFrom(m, leader, 160) && leader.takeDamage(1, m.pos)) {
        agent.hits++;
        leader.hurtCounter = 0;
        this.options.particles?.sparks([leader.pos[0], leader.pos[1] + 80, leader.pos[2]], 10);
      }
    }
    if (!leader) return;
    if (attacking(leader)) {
      this.neighbours(leader.pos, 180, other => {
        if (!hitFrom(leader, other.body.runtime, 180)) return;
        if (other.body.runtime.takeDamage(2, leader.pos)) {
          this.options.particles?.sparks([other.body.pos[0], other.body.pos[1] + 80, other.body.pos[2]], 12);
          this.options.sfx?.bonk();
        }
      });
    }
    // Stomp a Mario's head to bounce off it.
    if (leader.vel[1] < 0 && leader.action & ACT_FLAG_AIR) {
      this.neighbours(leader.pos, 80, other => {
        const head = other.body.pos[1] + 150;
        if (leader.pos[1] < head - 40 || leader.pos[1] > head + 30) return;
        leader.vel[1] = 52;
        leader.pos[1] = head;
        leader.setAction(ACT_FREEFALL, 0);
        leader.vel[1] = 52;
        other.body.runtime.squishTimer = 24;
        this.options.particles?.burst([leader.pos[0], head, leader.pos[2]], { count: 8, color: 0xffffff });
        this.options.sfx?.stompGoomba();
      });
    }
  }

  private playNear(pos: Vec3, play: (sfx: Sfx) => void): void {
    const sfx = this.options.sfx;
    const listener = this.options.listener?.();
    if (!sfx || !listener) return;
    if (Math.hypot(listener.x - pos[0], listener.y - pos[1], listener.z - pos[2]) < 2600) play(sfx);
  }

  /** Per-frame: interpolate, animate, billboards, shadows, selection ring. */
  render(alpha: number, dt: number, camera: THREE.Camera): void {
    const eye = camera.position;
    for (const agent of this.agents) {
      const p = agent.body.pos;
      agent.body.render(alpha, dt, Math.hypot(eye.x - p[0], eye.y - p[1], eye.z - p[2]));
      agent.body.faceCamera(camera);
    }
    this.batcher.sync();
    for (const coin of this.coins) if (coin.active) coin.group.rotation.y += dt * 4;
    this.shadows.update(this.agents.map(agent => agent.body));
    const selected = this.selected;
    this.ring.visible = Boolean(selected?.body.runtime.floor);
    if (selected?.body.runtime.floor) {
      (this.ring.material as THREE.MeshBasicMaterial).color.setHex(PERSONALITIES[selected.personality].color);
      this.ring.position.set(selected.body.interpolated.x, selected.body.runtime.floorHeight + 4, selected.body.interpolated.z);
      const pulse = 1 + Math.sin(performance.now() * 0.008) * 0.08;
      this.ring.scale.setScalar(pulse);
    }
  }

  /** Screen-space pick by head position: cheap and forgiving for small targets. */
  pick(clientX: number, clientY: number, camera: THREE.Camera, canvas: HTMLCanvasElement, radius = 46): CrowdAgent | null {
    const rect = canvas.getBoundingClientRect();
    const point = new THREE.Vector3();
    let best: CrowdAgent | null = null;
    let bestDistance = radius;
    for (const agent of this.agents) {
      for (const height of [40, 110]) {
        point.set(agent.body.interpolated.x, agent.body.interpolated.y + height, agent.body.interpolated.z).project(camera);
        if (point.z < -1 || point.z > 1) continue;
        const x = rect.left + (point.x * 0.5 + 0.5) * rect.width;
        const y = rect.top + (-point.y * 0.5 + 0.5) * rect.height;
        const d = Math.hypot(x - clientX, y - clientY);
        if (d < bestDistance) { bestDistance = d; best = agent; }
      }
    }
    return best;
  }

  describe(agent: CrowdAgent): CrowdAgentDebug {
    const m = agent.body.runtime;
    return {
      id: agent.id,
      name: agent.name,
      personality: agent.personality,
      label: agent.label || (agent.body.runtime.action & ACT_FLAG_AIR ? 'Airborne' : 'Idle'),
      action: actionName(m.action),
      pos: [m.pos[0], m.pos[1], m.pos[2]],
      speed: m.forwardVel,
      coins: agent.coins,
      laps: agent.laps,
      hits: agent.hits,
      bestLap: Number.isFinite(agent.bestLap) ? agent.bestLap : null,
    };
  }

  get drawCalls(): number { return this.batcher.drawCalls; }

  counts(): Record<Personality, number> {
    const counts = { racer: 0, hunter: 0, follower: 0, brawler: 0, explorer: 0 };
    for (const agent of this.agents) counts[agent.personality]++;
    return counts;
  }

  private clearAgents(): void {
    for (const agent of this.agents) {
      this.batcher.remove(agent.body);
      agent.body.dispose();
    }
    this.agents.length = 0;
    this.selected = null;
    this.stats.summits = this.stats.coins = this.stats.hits = 0;
    this.stats.bestLap = Infinity;
  }

  dispose(): void {
    this.generation++;
    this.clearAgents();
    for (const coin of this.coins) coin.group.removeFromParent();
    this.coins.length = 0;
    this.shadows.dispose();
    this.batcher.dispose();
    this.ring.geometry.dispose();
    (this.ring.material as THREE.Material).dispose();
    this.root.removeFromParent();
  }
}
