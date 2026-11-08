/**
 * Race mode for the MK64 experiment, engine-free: ordered checkpoints, laps, a 3-2-1-GO start, three CPU
 * drivers on the shared kartPhysics core, ranking and the item rules (mushroom, banana, green shell).
 * Browser twin of native/src/experiments/kart_race.rs; the rules are checked by kartRace.test.ts there and here.
 * Everything runs on the 30 Hz sim tick (tick() steps the 60 Hz kart physics twice) in MK64 course units.
 *
 * Simplifications against MK64: no early-start spinout, no kart-to-kart collision, a simple CPU item decision
 * (timer plus a cone check), no Lakitu animation (a fall puts you back on the last gate) and a spinout is a
 * speed loss plus a visual wobble rather than the decomp's tumble.
 */
import { KartPhysics, type KartGround, type KartInput } from './kartPhysics';

export const LAPS = 3;
export const CPU_COUNT = 3;
export const RACERS = CPU_COUNT + 1;
export const GATES = 24;
/** Half the width of a checkpoint gate, course units. */
export const GATE_HALF = 420;
export const COUNTDOWN_TICKS = 90;
export const GO_TICKS = 30;
export const SPIN_TICKS = 36;
export const IMMUNE_TICKS = 54;
export const BOOST_TICKS = 42;
export const ROULETTE_TICKS = 45;
export const BOX_RESPAWN = 150;
export const BOX_RADIUS = 30;
export const HIT_RADIUS = 24;
/** Raw units per 60 Hz frame. */
export const SHELL_SPEED = 15;
export const SHELL_LIFE = 240;
export const MAX_BANANAS = 12;
const SPIN_DAMP = 0.94;
const PLAYER_SLOT = 3;

type V2 = [number, number];

export const toS16 = (v: number) => (Math.trunc(v) << 16) >> 16;
/** MK64 rotation[1] for a direction in the xz plane (forward = (sin yaw, cos yaw)). */
export const rotationFor = (dir: V2) => toS16(-Math.atan2(dir[0], dir[1]) * 32768 / Math.PI);
const norm = (v: V2): V2 => { const l = Math.hypot(v[0], v[1]); return l < 1e-9 ? [0, 1] : [v[0] / l, v[1] / l]; };
const dist = (a: V2, b: V2) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const forward = (k: KartPhysics): V2 => { const y = k.yaw(); return [Math.sin(y), Math.cos(y)]; };
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** xorshift32, identical to the native twin so seeds replay the same. */
export class Rng {
  constructor(public state: number) { this.state >>>= 0; }
  nextU32(): number {
    let x = this.state;
    x = (x ^ (x << 13)) >>> 0;
    x = (x ^ (x >>> 17)) >>> 0;
    x = (x ^ (x << 5)) >>> 0;
    this.state = x;
    return x;
  }
  unit(): number { return this.nextU32() / 4294967296; }
}

export type Item = 'mushroom' | 'banana' | 'shell';
export const ITEMS: readonly Item[] = ['mushroom', 'banana', 'shell'];
export const ITEM_NAMES: Record<Item, string> = { mushroom: 'Mushroom', banana: 'Banana', shell: 'Green shell' };

export interface Gate { c: V2; t: V2; arc: number }
/** Did the segment prev -> now cross this gate forwards inside its width? */
export function gateCrossed(g: Gate, prev: V2, now: V2): boolean {
  const d0 = (prev[0] - g.c[0]) * g.t[0] + (prev[1] - g.c[1]) * g.t[1];
  const d1 = (now[0] - g.c[0]) * g.t[0] + (now[1] - g.c[1]) * g.t[1];
  if (!(d0 < 0 && d1 >= 0)) return false;
  const s = d0 / (d0 - d1);
  const p: V2 = [prev[0] + (now[0] - prev[0]) * s, prev[1] + (now[1] - prev[1]) * s];
  const lateral = (p[0] - g.c[0]) * g.t[1] - (p[1] - g.c[1]) * g.t[0];
  return Math.abs(lateral) <= GATE_HALF;
}

/** The closed centreline plus the checkpoint gates cut across it. */
export class Track {
  readonly nodes: Array<[number, number, number]>;
  readonly cum: number[];
  readonly length: number;
  readonly gates: Gate[] = [];
  readonly spacing: number;
  readonly minY: number;

  /** `path` as exported (raw course units). Sentinel nodes (the export ends with x = -32768) are dropped. */
  constructor(path: ReadonlyArray<readonly number[]>) {
    this.nodes = path.filter(p => p.every(Number.isFinite) && p[0] > -30000).map(p => [p[0], p[1], p[2]] as [number, number, number]);
    const n = this.nodes.length;
    if (n < 8) throw new Error('track needs a path');
    this.cum = new Array<number>(n).fill(0);
    for (let i = 1; i < n; i++) this.cum[i] = this.cum[i - 1] + this.seg(i - 1);
    this.length = this.cum[n - 1] + this.seg(n - 1);
    this.spacing = this.length / GATES;
    this.minY = Math.min(...this.nodes.map(p => p[1]));
    for (let k = 0; k < GATES; k++) {
      const i = Math.floor(k * n / GATES);
      this.gates.push({ c: this.xz(i), t: this.dir(i), arc: this.cum[i] });
    }
  }
  get size(): number { return this.nodes.length; }
  xz(i: number): V2 { const p = this.nodes[i % this.nodes.length]; return [p[0], p[2]]; }
  private seg(i: number): number { return dist(this.xz(i), this.xz(i + 1)); }
  /** Smoothed unit direction of travel at node i. */
  dir(i: number): V2 {
    const n = this.nodes.length;
    const a = this.xz((i + n - 2) % n), b = this.xz(i + 2);
    return norm([b[0] - a[0], b[1] - a[1]]);
  }
  /** Right-hand perpendicular of the direction of travel (the lateral axis). */
  side(i: number): V2 { const t = this.dir(i); return [t[1], -t[0]]; }
  /** Node `distance` of arc length ahead of i. */
  ahead(i: number, distance: number): number {
    let j = i % this.size, acc = 0;
    for (let k = 0; k < this.size; k++) {
      if (acc >= distance) break;
      acc += this.seg(j);
      j = (j + 1) % this.size;
    }
    return j;
  }
  behind(i: number, distance: number): number {
    const n = this.size;
    let j = i % n, acc = 0;
    for (let k = 0; k < n; k++) {
      if (acc >= distance) break;
      j = (j + n - 1) % n;
      acc += this.seg(j);
    }
    return j;
  }
  nearestGlobal(p: V2): number {
    let best = 0, bd = Infinity;
    for (let i = 0; i < this.size; i++) { const d = dist(this.xz(i), p); if (d < bd) { bd = d; best = i; } }
    return best;
  }
  /** Nearest node within `window` nodes of `hint`. */
  nearest(p: V2, hint: number, window: number): number {
    const n = this.size;
    let best = hint % n, bd = Infinity;
    for (let k = 0; k <= 2 * window; k++) {
      const i = (hint + n + k - window % n) % n;
      const d = dist(this.xz(i), p);
      if (d < bd) { bd = d; best = i; }
    }
    return best;
  }
  /** Starting grid: two abreast, rows behind the line. */
  grid(slot: number): { pos: V2; rotation: number } {
    const j = this.behind(0, 70 + 75 * Math.floor(slot / 2));
    const side = this.side(j);
    const lateral = slot % 2 === 0 ? -38 : 38;
    const c = this.xz(j);
    return { pos: [c[0] + side[0] * lateral, c[1] + side[1] * lateral], rotation: rotationFor(this.dir(j)) };
  }
  /** Item box rows: node index for each of the four stations along the lap. */
  boxNodes(): number[] {
    return [0.08, 0.31, 0.54, 0.78].map(f => Math.max(0, this.cum.findIndex(c => c >= f * this.length)));
  }
}

export type RaceEvent =
  | { kind: 'count'; n: number } | { kind: 'go' } | { kind: 'pickup'; i: number }
  | { kind: 'use'; i: number; item: Item } | { kind: 'hit'; i: number } | { kind: 'lap'; i: number; n: number }
  | { kind: 'finish'; i: number } | { kind: 'rescue'; i: number };
export type Phase = 'countdown' | 'racing' | 'finished';

export interface Racer {
  passed: number; nextGate: number; last: V2; hint: number;
  spin: number; immune: number; boost: number;
  item: Item | null; roulette: number; pending: Item;
  lapStart: number; laps: number[]; finish: number | null; distNext: number;
  stuck: number; hold: number; rescues: number;
}
const newRacer = (pos: V2): Racer => ({
  passed: 0, nextGate: 0, last: pos, hint: 0, spin: 0, immune: 0, boost: 0, item: null, roulette: 0, pending: 'mushroom',
  lapStart: 0, laps: [], finish: null, distNext: 0, stuck: 0, hold: 0, rescues: 0,
});
export const racerLap = (r: Racer) => Math.min(LAPS, r.laps.length + 1);

export interface Banana { pos: V2; y: number; owner: number; age: number }
export interface Shell { pos: V2; y: number; dir: V2; bounces: number; life: number; owner: number }
export interface ItemBox { pos: V2; y: number; respawn: number }
export interface Results { order: number[]; total: number; laps: number[]; bestLap: number; newBestLap: boolean; newBestTotal: boolean }
export const resultsPlace = (r: Results) => r.order.indexOf(0) + 1;

const NO_INPUT: KartInput = { rawX: 0, rawY: 0, accel: false, brake: false, hopDown: false, hopPressed: false };

/** A path-following CPU on the same physics. Deterministic: the only randomness is the seeded lane and skill. */
export class Driver {
  readonly lane: number;
  readonly skill: number;
  boostCool = 90;
  ticks = 0;
  constructor(rng: Rng, public hint: number) {
    this.lane = (rng.unit() - 0.5) * 100;
    this.skill = 0.9 + rng.unit() * 0.08;
  }
  /** `rubber`: how far this CPU is ahead of the player along the lap (negative when behind). */
  input(track: Track, kart: KartPhysics, rubber: number, racing: boolean): { input: KartInput; boost: boolean } {
    this.ticks++;
    const pos: V2 = [kart.pos[0], kart.pos[2]];
    this.hint = track.nearest(pos, this.hint, 30);
    if (!racing) return { input: { ...NO_INPUT }, boost: false };
    const look = 130 + kart.speed * 22;
    const targetNode = track.ahead(this.hint, look);
    const side = track.side(targetNode);
    const tp = track.xz(targetNode);
    const target: V2 = [tp[0] + side[0] * this.lane, tp[1] + side[1] * this.lane];
    const want = Math.atan2(target[0] - pos[0], target[1] - pos[1]);
    let diff = want - kart.yaw();
    while (diff > Math.PI) diff -= 2 * Math.PI;
    while (diff < -Math.PI) diff += 2 * Math.PI;
    const stick = clamp(-diff * 120, -64, 64);
    const a = track.dir(track.ahead(this.hint, 120));
    const b = track.dir(track.ahead(this.hint, 420));
    const turn = Math.acos(clamp(a[0] * b[0] + a[1] * b[1], -1, 1));
    const safe = clamp(6.4 - turn * 3.2, 2.6, 6.4) * this.skill;
    let accel = true, brake = false;
    if (kart.speed > safe + 0.3 || (Math.abs(diff) > 0.7 && kart.speed > 3.2)) {
      accel = false;
      brake = kart.speed > safe + 1.3 && kart.speed > 2;
    }
    // Rubber band: leaders lift now and then, stragglers get an occasional boost.
    if (rubber > 1400 && this.ticks % 5 === 0) accel = false;
    let boost = false;
    if (this.boostCool > 0) this.boostCool--;
    else if (rubber < -900) { boost = true; this.boostCool = 300; }
    const rawY = brake ? -80 : accel ? 80 : 0;
    return { input: { rawX: Math.round(stick * 80 / 64), rawY, accel, brake, hopDown: false, hopPressed: false }, boost };
  }
}

export interface Cpu { kart: KartPhysics; driver: Driver }

/** Roulette weights per rank (0 = leading): [mushroom, banana, shell]. */
const WEIGHTS: number[][] = [[2, 5, 3], [4, 4, 3], [5, 2, 3], [6, 1, 3]];

/** One 30 Hz tick of a kart: spin/boost on top of the physics core, two 60 Hz updates. */
function stepKart(kart: KartPhysics, st: Racer, input: KartInput, racing: boolean, ground: KartGround): void {
  const pad = !racing || st.spin > 0 ? NO_INPUT : input;
  for (let f = 0; f < 2; f++) {
    // The mushroom reuses the mini-turbo thrust: keep the flag raised for its duration.
    if (st.boost > 0) kart.miniTurbo = true;
    kart.step({ ...pad, hopPressed: pad.hopPressed && f === 0 }, ground);
    if (st.spin > 0) {
      for (let k = 0; k < 3; k++) kart.velocity[k] *= SPIN_DAMP;
      kart.currentSpeed *= SPIN_DAMP;
    }
  }
  st.spin = Math.max(0, st.spin - 1);
  st.immune = Math.max(0, st.immune - 1);
  st.boost = Math.max(0, st.boost - 1);
}

export class KartRace {
  readonly track: Track;
  phase: Phase = 'countdown';
  clock = 0;
  raceTick = 0;
  private countdown = COUNTDOWN_TICKS;
  racers: Racer[] = [];
  cpus: Cpu[] = [];
  bananas: Banana[] = [];
  shells: Shell[] = [];
  boxes: ItemBox[] = [];
  rng: Rng;
  private readonly seed: number;
  events: RaceEvent[] = [];
  results: Results | null = null;
  bestLap: number | null = null;
  bestTotal: number | null = null;
  /** Set when best* changed and should be saved. */
  bestDirty = false;
  readonly fallY: number;

  constructor(path: ReadonlyArray<readonly number[]>, seed: number) {
    this.track = new Track(path);
    this.fallY = this.track.minY - 300;
    this.seed = Math.max(1, seed >>> 0);
    this.rng = new Rng(this.seed);
  }

  private static floorY(ground: KartGround, x: number, y: number, z: number): number {
    return ground.floor(x, y + 60, z)?.height ?? y;
  }

  /** (Re)start: grid, countdown, fresh items. The player's kart is replaced and returned. */
  start(ground: KartGround): KartPhysics {
    this.rng = new Rng(this.seed);
    this.phase = 'countdown';
    this.clock = 0;
    this.raceTick = 0;
    this.countdown = COUNTDOWN_TICKS;
    this.results = null;
    this.events = [];
    this.bananas = [];
    this.shells = [];
    this.racers = [];
    this.cpus = [];
    const y0 = this.track.nodes[0][1];
    let player!: KartPhysics;
    for (let slot = 0; slot < RACERS; slot++) {
      const g = this.track.grid(slot);
      const p = g.pos;
      const y = KartRace.floorY(ground, p[0], y0, p[1]);
      const kart = new KartPhysics([p[0], y, p[1]], g.rotation);
      const racer = newRacer(p);
      racer.hint = this.track.nearestGlobal(p);
      racer.distNext = dist(p, this.track.gates[0].c);
      if (slot === PLAYER_SLOT) { player = kart; this.racers.unshift(racer); }
      else {
        const driver = new Driver(this.rng, racer.hint);
        this.racers.push(racer);
        this.cpus.push({ kart, driver });
      }
    }
    this.boxes = [];
    for (const node of this.track.boxNodes()) {
      const side = this.track.side(node), c = this.track.xz(node);
      for (const lateral of [-52, 0, 52]) {
        const p: V2 = [c[0] + side[0] * lateral, c[1] + side[1] * lateral];
        this.boxes.push({ pos: p, y: KartRace.floorY(ground, p[0], this.track.nodes[node][1], p[1]), respawn: 0 });
      }
    }
    this.events.push({ kind: 'count', n: 3 });
    return player;
  }

  kartPos(i: number, player: KartPhysics): [number, number, number] {
    return i === 0 ? player.pos : this.cpus[i - 1].kart.pos;
  }

  /** Race progress in course units: whole gates passed minus the distance left to the next one. */
  progress(i: number): number {
    const r = this.racers[i];
    return r.passed * this.track.spacing - r.distNext;
  }

  /** Racer indices, first place first. Finished racers rank by finishing time. */
  order(): number[] {
    const o = this.racers.map((_, i) => i);
    o.sort((a, b) => {
      const ra = this.racers[a], rb = this.racers[b];
      if (ra.finish !== null && rb.finish !== null) return ra.finish - rb.finish || a - b;
      if (ra.finish !== null) return -1;
      if (rb.finish !== null) return 1;
      const d = this.progress(b) - this.progress(a);
      return d < 0 ? -1 : d > 0 ? 1 : a - b;
    });
    return o;
  }
  /** 0-based rank of racer i. */
  rank(i: number): number { return this.order().indexOf(i); }

  /** Visual spin of racer i in radians (0 when not spinning). */
  spinAngle(i: number): number {
    const s = this.racers[i].spin;
    return s === 0 ? 0 : (1 - s / SPIN_TICKS) * 2 * Math.PI;
  }

  /** 3, 2, 1 while counting down, then 0 for "GO!" during its first second; null otherwise. */
  banner(): number | null {
    if (this.phase === 'countdown') return Math.ceil(this.countdown / 30);
    return this.raceTick < GO_TICKS ? 0 : null;
  }

  private rescueKart(i: number, ground: KartGround): KartPhysics {
    const r = this.racers[i];
    let p: V2, rot: number, y: number;
    if (r.passed === 0) {
      const g = this.track.grid(i === 0 ? PLAYER_SLOT : i - 1);
      p = g.pos; rot = g.rotation; y = this.track.nodes[0][1];
    } else {
      const g = this.track.gates[(r.nextGate + GATES - 1) % GATES];
      const idx = Math.max(0, this.track.cum.findIndex(c => c >= g.arc));
      p = g.c; rot = rotationFor(g.t); y = this.track.nodes[idx][1];
    }
    return new KartPhysics([p[0], KartRace.floorY(ground, p[0], y, p[1]), p[1]], rot);
  }

  /** Returns the replacement kart when the player was moved (the caller owns the player's kart). */
  private rescue(i: number, ground: KartGround): KartPhysics {
    const k = this.rescueKart(i, ground);
    const p: V2 = [k.pos[0], k.pos[2]];
    if (i > 0) this.cpus[i - 1].kart = k;
    const r = this.racers[i];
    r.last = p; r.spin = 0; r.boost = 0; r.stuck = 0; r.immune = 60; r.rescues++;
    r.hint = this.track.nearestGlobal(p);
    if (i > 0) this.cpus[i - 1].driver.hint = r.hint;
    this.events.push({ kind: 'rescue', i });
    return k;
  }

  /** Player asked for a Lakitu rescue (R in race mode). Returns the new player kart, or null. */
  rescuePlayer(ground: KartGround): KartPhysics | null {
    return this.phase === 'racing' ? this.rescue(0, ground) : null;
  }

  roll(rank: number): Item {
    const w = WEIGHTS[Math.min(rank, RACERS - 1)];
    const total = w.reduce((a, b) => a + b, 0);
    let pick = Math.floor(this.rng.unit() * total);
    for (let k = 0; k < w.length; k++) {
      if (pick < w[k]) return ITEMS[k];
      pick -= w[k];
    }
    return 'mushroom';
  }

  private hit(i: number, player: KartPhysics): void {
    const r = this.racers[i];
    if (r.spin > 0 || r.immune > 0 || (r.finish !== null && i !== 0)) return;
    r.spin = SPIN_TICKS;
    r.immune = SPIN_TICKS + IMMUNE_TICKS;
    r.boost = 0;
    const kart = i === 0 ? player : this.cpus[i - 1].kart;
    for (let k = 0; k < 3; k++) kart.velocity[k] *= 0.35;
    kart.currentSpeed *= 0.35;
    this.events.push({ kind: 'hit', i });
  }

  private useItem(i: number, player: KartPhysics, ground: KartGround): void {
    const r = this.racers[i];
    const item = r.item;
    if (!item) return;
    r.item = null;
    const kart = i === 0 ? player : this.cpus[i - 1].kart;
    const f = forward(kart), p: V2 = [kart.pos[0], kart.pos[2]], y = kart.pos[1];
    if (item === 'mushroom') r.boost = BOOST_TICKS;
    else if (item === 'banana') {
      if (this.bananas.length >= MAX_BANANAS) this.bananas.shift();
      const q: V2 = [p[0] - f[0] * 30, p[1] - f[1] * 30];
      this.bananas.push({ pos: q, y: KartRace.floorY(ground, q[0], y, q[1]), owner: i, age: 0 });
    } else {
      this.shells.push({ pos: [p[0] + f[0] * 28, p[1] + f[1] * 28], y, dir: f, bounces: 0, life: 0, owner: i });
    }
    this.events.push({ kind: 'use', i, item });
  }

  /** Straight-line items, one tick: shells advance (bouncing off one wall), bananas age. */
  stepProjectiles(ground: KartGround): void {
    for (const b of this.bananas) b.age++;
    this.shells = this.shells.filter(s => {
      s.life++;
      if (s.life > SHELL_LIFE) return false;
      for (let k = 0; k < 4; k++) {
        const step = SHELL_SPEED * 2 / 4;
        const np: V2 = [s.pos[0] + s.dir[0] * step, s.pos[1] + s.dir[1] * step];
        const floor = ground.floor(np[0], s.y + 30, np[1]);
        if (!floor) return false;
        s.y = floor.height;
        const w = ground.wall([np[0], s.y, np[1]]);
        if (w) {
          if (s.bounces >= 1) return false;
          s.bounces++;
          // The wall's own normal (pointing out of it) when the physics ground reports one, else the push-out direction.
          const hit = w.hits[0];
          const flat = hit ? Math.hypot(hit.normal[0], hit.normal[2]) : 0;
          const n = flat > 0.1 ? norm([hit.normal[0], hit.normal[2]]) : norm([w.pos[0] - np[0], w.pos[2] - np[1]]);
          const dot = s.dir[0] * n[0] + s.dir[1] * n[1];
          s.dir = norm([s.dir[0] - 2 * dot * n[0], s.dir[1] - 2 * dot * n[1]]);
          s.pos = [w.pos[0], w.pos[2]];
          continue;
        }
        s.pos = np;
      }
      return true;
    });
  }

  /**
   * Advance the whole race by one 30 Hz tick. `input` is the player's pad; `useItem` is the item button edge.
   * Returns the player's kart (replaced on a rescue, otherwise the one passed in).
   */
  tick(input: KartInput, useItem: boolean, player: KartPhysics, ground: KartGround): KartPhysics {
    if (this.racers.length === 0) player = this.start(ground);
    this.clock++;
    if (this.phase === 'countdown') {
      this.countdown = Math.max(0, this.countdown - 1);
      if (this.countdown === 0) { this.phase = 'racing'; this.raceTick = 0; this.events.push({ kind: 'go' }); }
      else if (this.countdown % 30 === 0) this.events.push({ kind: 'count', n: this.countdown / 30 });
    } else this.raceTick++;
    const racing = this.phase !== 'countdown';
    const pad = this.phase === 'finished' ? NO_INPUT : input;
    stepKart(player, this.racers[0], pad, racing, ground);
    const playerProgress = this.progress(0);
    for (let c = 0; c < CPU_COUNT; c++) {
      const rubber = this.progress(c + 1) - playerProgress;
      const cpu = this.cpus[c];
      const out = cpu.driver.input(this.track, cpu.kart, rubber, racing);
      if (out.boost && this.racers[c + 1].boost === 0) this.racers[c + 1].boost = BOOST_TICKS >> 1;
      stepKart(cpu.kart, this.racers[c + 1], out.input, racing, ground);
    }
    if (racing) {
      for (let i = 0; i < RACERS; i++) {
        const pos = this.kartPos(i, player);
        const p: V2 = [pos[0], pos[2]];
        const slow = (i === 0 ? player.speed : this.cpus[i - 1].kart.speed) < 0.25;
        const r = this.racers[i];
        r.stuck = slow && i > 0 ? r.stuck + 1 : 0;
        if (pos[1] < this.fallY || r.stuck > 150) {
          const k = this.rescue(i, ground);
          if (i === 0) player = k;
          continue;
        }
        this.progressStep(i, p);
      }
      this.itemsStep(useItem, player, ground);
    }
    return player;
  }

  progressStep(i: number, p: V2): void {
    const tick = this.raceTick;
    const r = this.racers[i];
    const gate = this.track.gates[r.nextGate];
    if (gateCrossed(gate, r.last, p)) {
      const crossed = r.nextGate;
      r.passed++;
      r.nextGate = (r.nextGate + 1) % GATES;
      if (crossed === 0) {
        if (r.passed === 1) r.lapStart = tick;
        else {
          const time = tick - r.lapStart;
          r.lapStart = tick;
          r.laps.push(time);
          const n = r.laps.length;
          this.events.push({ kind: 'lap', i, n });
          if (i === 0 && (this.bestLap === null || time < this.bestLap)) { this.bestLap = time; this.bestDirty = true; }
          if (n >= LAPS && r.finish === null) {
            r.finish = tick;
            this.events.push({ kind: 'finish', i });
            if (i === 0) this.finishPlayer();
          }
        }
      }
    }
    r.last = p;
    r.distNext = dist(p, this.track.gates[r.nextGate].c);
  }

  private finishPlayer(): void {
    this.phase = 'finished';
    const total = this.racers[0].finish ?? this.raceTick;
    const laps = [...this.racers[0].laps];
    const bestLap = Math.min(...laps);
    const newBestTotal = this.bestTotal === null || total < this.bestTotal;
    if (newBestTotal) { this.bestTotal = total; this.bestDirty = true; }
    const newBestLap = this.bestLap === bestLap && this.bestDirty;
    this.results = { order: this.order(), total, laps, bestLap, newBestLap, newBestTotal };
  }

  private itemsStep(useItem: boolean, player: KartPhysics, ground: KartGround): void {
    // Boxes and roulette.
    for (const box of this.boxes) {
      if (box.respawn > 0) { box.respawn--; continue; }
      for (let i = 0; i < RACERS; i++) {
        const r = this.racers[i];
        if (r.item || r.roulette > 0 || r.finish !== null) continue;
        const k = this.kartPos(i, player);
        if (dist([k[0], k[2]], box.pos) < BOX_RADIUS) {
          box.respawn = BOX_RESPAWN;
          const item = this.roll(this.rank(i));
          r.roulette = ROULETTE_TICKS;
          r.pending = item;
          r.hold = 0;
          this.events.push({ kind: 'pickup', i });
          break;
        }
      }
    }
    for (const r of this.racers) {
      if (r.roulette > 0) {
        r.roulette--;
        if (r.roulette === 0) r.item = r.pending;
      } else if (r.item) r.hold++;
    }
    // Use.
    if (useItem && this.phase === 'racing' && this.racers[0].spin === 0) this.useItem(0, player, ground);
    for (let c = 0; c < CPU_COUNT; c++) {
      const i = c + 1, r = this.racers[i];
      if (!r.item || r.spin > 0) continue;
      let go: boolean;
      if (r.item === 'mushroom') go = r.hold > 20 + 7 * c;
      else if (r.item === 'banana') go = r.hold > 45 + 20 * c;
      else {
        const me = this.cpus[c].kart;
        const f = forward(me), p: V2 = [me.pos[0], me.pos[2]];
        let target = false;
        for (let j = 0; j < RACERS && !target; j++) {
          if (j === i) continue;
          const k = this.kartPos(j, player);
          const d: V2 = [k[0] - p[0], k[2] - p[1]];
          const l = Math.hypot(d[0], d[1]);
          if (l > 40 && l < 900 && (d[0] * f[0] + d[1] * f[1]) / l > 0.96) target = true;
        }
        go = target || r.hold > 300;
      }
      if (go) this.useItem(i, player, ground);
    }
    this.stepProjectiles(ground);
    // Hits.
    const hits: number[] = [];
    for (let i = 0; i < RACERS; i++) {
      const k = this.kartPos(i, player);
      const p: V2 = [k[0], k[2]];
      const r = this.racers[i];
      if (r.spin > 0 || r.immune > 0) continue;
      const b = this.bananas.findIndex(x => x.age >= (x.owner === i ? 24 : 4) && dist(x.pos, p) < HIT_RADIUS);
      if (b >= 0) { this.bananas.splice(b, 1); hits.push(i); continue; }
      const s = this.shells.findIndex(x => (x.owner !== i || x.life > 10) && dist(x.pos, p) < HIT_RADIUS + 6);
      if (s >= 0) { this.shells.splice(s, 1); hits.push(i); }
    }
    for (const i of hits) this.hit(i, player);
  }

  drainEvents(): RaceEvent[] {
    const out = this.events;
    this.events = [];
    return out;
  }
}

/** m:ss.cc for a tick count at 30 Hz. */
export function formatTicks(ticks: number): string {
  const cs = Math.round(ticks * 100 / 30);
  return `${Math.floor(cs / 6000)}:${String(Math.floor(cs / 100) % 60).padStart(2, '0')}.${String(cs % 100).padStart(2, '0')}`;
}
export const ordinal = (place: number) => ['1st', '2nd', '3rd', '4th'][Math.min(place, 3)];

export interface Bests { bestLap: number | null; bestTotal: number | null }
const BESTS_KEY = 'jumpman.kart.bests';
export function loadBests(): Bests {
  try {
    const raw = JSON.parse(localStorage.getItem(BESTS_KEY) ?? 'null') as Partial<Bests> | null;
    const num = (v: unknown) => typeof v === 'number' && Number.isFinite(v) && v > 0 ? v : null;
    return { bestLap: num(raw?.bestLap), bestTotal: num(raw?.bestTotal) };
  } catch { return { bestLap: null, bestTotal: null }; }
}
export function saveBests(b: Bests): void {
  try { localStorage.setItem(BESTS_KEY, JSON.stringify(b)); } catch { /* storage is optional */ }
}
