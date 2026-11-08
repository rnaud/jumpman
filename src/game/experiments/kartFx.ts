/**
 * Particle effects around the player's kart: drift sparks (white, then orange, then blue as the mini-turbo charges),
 * tyre smoke while drifting and on grass, the mini-turbo / mushroom exhaust flame, and the shards of a smashed item
 * box. Pooled camera-facing sprites in world units. Emission rules are in kartFxRules (engine-free, also ported to
 * native/src/experiments/kart_fx.rs).
 */
import * as THREE from 'three';

export type FxKind = 'spark' | 'smoke' | 'flame' | 'shard';

export interface FxFrame {
  /** World position of the kart's feet, yaw (forward = (sin, cos)) and whether it is being driven. */
  pos: [number, number, number];
  yaw: number;
  driving: boolean;
  midair: boolean;
  drifting: boolean;
  hop: boolean;
  driftState: number;
  boosting: boolean;
  grass: boolean;
  /** currentSpeed / top speed. */
  speedRatio: number;
}

/** Particles per second for each kind this frame. */
export interface FxRates { spark: number; smoke: number; flame: number }

/** What a frame should emit. Mirrors kart_fx::rates in the native build. */
export function fxRates(f: FxFrame): FxRates {
  const none = { spark: 0, smoke: 0, flame: 0 };
  if (!f.driving) return none;
  const ground = !f.midair;
  const sliding = ground && f.drifting && !f.hop;
  return {
    spark: sliding ? 38 + 14 * Math.min(2, f.driftState) : 0,
    smoke: ground && f.speedRatio > 0.15 ? (sliding ? 30 : f.grass ? 22 * f.speedRatio : 0) : 0,
    flame: f.boosting ? 90 : 0,
  };
}

/** Spark colour for the drift charge: 0xRRGGBB. */
export function sparkColour(driftState: number): number {
  return driftState >= 2 ? 0x4fb4ff : driftState === 1 ? 0xffa21f : 0xfff2a8;
}

interface Particle {
  sprite: THREE.Sprite;
  vel: THREE.Vector3;
  age: number;
  life: number;
  size: number;
  grow: number;
  kind: FxKind;
  alive: boolean;
}

const MAX = 120;

export class KartFx {
  private readonly group = new THREE.Group();
  private readonly pool: Particle[] = [];
  private readonly texture: THREE.CanvasTexture;
  private carry: FxRates = { spark: 0, smoke: 0, flame: 0 };
  private seed = 1;

  constructor(root: THREE.Group) {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 32;
    const c = canvas.getContext('2d');
    if (c) {
      const g = c.createRadialGradient(16, 16, 1, 16, 16, 15);
      g.addColorStop(0, 'rgba(255,255,255,1)');
      g.addColorStop(0.5, 'rgba(255,255,255,.65)');
      g.addColorStop(1, 'rgba(255,255,255,0)');
      c.fillStyle = g;
      c.fillRect(0, 0, 32, 32);
    }
    this.texture = new THREE.CanvasTexture(canvas);
    root.add(this.group);
    for (let i = 0; i < MAX; i++) {
      const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map: this.texture, transparent: true, depthWrite: false }));
      sprite.visible = false;
      this.group.add(sprite);
      this.pool.push({ sprite, vel: new THREE.Vector3(), age: 0, life: 1, size: 20, grow: 0, kind: 'spark', alive: false });
    }
  }

  private rand(): number {
    this.seed = (this.seed * 1664525 + 1013904223) >>> 0;
    return this.seed / 4294967296;
  }

  private spawn(kind: FxKind, pos: THREE.Vector3, vel: THREE.Vector3, life: number, size: number, grow: number, color: number, opacity = 1): void {
    const p = this.pool.find(q => !q.alive);
    if (!p) return;
    p.alive = true; p.kind = kind; p.age = 0; p.life = life; p.size = size; p.grow = grow;
    p.vel.copy(vel);
    p.sprite.position.copy(pos);
    p.sprite.material.color.setHex(color);
    p.sprite.material.opacity = opacity;
    p.sprite.scale.set(size, size, 1);
    p.sprite.visible = true;
  }

  /** Smashed item box: a ring of rainbow shards. */
  burst(pos: [number, number, number]): void {
    for (let i = 0; i < 16; i++) {
      const a = this.rand() * Math.PI * 2, up = 80 + this.rand() * 220, out = 120 + this.rand() * 260;
      const color = new THREE.Color().setHSL(this.rand(), 0.9, 0.6).getHex();
      this.spawn('shard', new THREE.Vector3(...pos), new THREE.Vector3(Math.cos(a) * out, up, Math.sin(a) * out), 0.55, 24 + this.rand() * 18, -20, color);
    }
  }

  update(dt: number, f: FxFrame): void {
    const rates = fxRates(f);
    const sin = Math.sin(f.yaw), cos = Math.cos(f.yaw);
    for (const kind of ['spark', 'smoke', 'flame'] as const) {
      this.carry[kind] += rates[kind] * dt;
      while (this.carry[kind] >= 1) {
        this.carry[kind] -= 1;
        const side = this.rand() < 0.5 ? -1 : 1;
        // Rear wheels: 40 across, 56 behind the kart's centre (world units).
        const lx = side * 40, lz = -56;
        const base = new THREE.Vector3(f.pos[0] + sin * lz + cos * lx, f.pos[1] + 10, f.pos[2] + cos * lz - sin * lx);
        if (kind === 'spark') {
          const out = side * (60 + this.rand() * 120);
          const vel = new THREE.Vector3(cos * out - sin * 120, 90 + this.rand() * 160, -sin * out - cos * 120);
          this.spawn('spark', base, vel, 0.28 + this.rand() * 0.18, 22, -30, sparkColour(f.driftState));
        } else if (kind === 'smoke') {
          const vel = new THREE.Vector3((this.rand() - 0.5) * 80 - sin * 140, 30 + this.rand() * 60, (this.rand() - 0.5) * 80 - cos * 140);
          this.spawn('smoke', base, vel, 0.6 + this.rand() * 0.3, 40, 90, f.grass ? 0x9a8660 : 0xdcdcdc, 0.55);
        } else {
          // Twin exhausts: 18 either side, 80 behind the centre, low.
          const mid = new THREE.Vector3(f.pos[0] + sin * -80 + cos * side * 18, f.pos[1] + 24 + this.rand() * 8, f.pos[2] + cos * -80 - sin * side * 18);
          const vel = new THREE.Vector3(-sin * (300 + this.rand() * 200) + (this.rand() - 0.5) * 60, (this.rand() - 0.5) * 40, -cos * (300 + this.rand() * 200) + (this.rand() - 0.5) * 60);
          this.spawn('flame', mid, vel, 0.2, 30, -70, this.rand() < 0.5 ? 0xffd23a : 0xff7a1c, 0.85);
        }
      }
    }
    for (const p of this.pool) {
      if (!p.alive) continue;
      p.age += dt;
      if (p.age >= p.life) { p.alive = false; p.sprite.visible = false; continue; }
      if (p.kind === 'shard' || p.kind === 'spark') p.vel.y -= 520 * dt;
      p.sprite.position.addScaledVector(p.vel, dt);
      const size = Math.max(4, p.size + p.grow * p.age);
      p.sprite.scale.set(size, size, 1);
      const left = 1 - p.age / p.life;
      p.sprite.material.opacity = (p.kind === 'smoke' ? 0.55 : 1) * left;
    }
  }

  /** Live particle count (tests and the debug hook). */
  get count(): number { return this.pool.filter(p => p.alive).length; }

  dispose(): void {
    this.group.removeFromParent();
    for (const p of this.pool) p.sprite.material.dispose();
    this.texture.dispose();
  }
}
