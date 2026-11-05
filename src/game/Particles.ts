/**
 * A tiny sprite particle pool for dust, landing puffs and wall-kick sparks.
 */

import * as THREE from 'three';
import type { Vec3 } from '../sim/math';

interface Particle {
  sprite: THREE.Sprite;
  material: THREE.SpriteMaterial;
  velocity: THREE.Vector3;
  life: number;
  maxLife: number;
  gravity: number;
  size: number;
  spin: number;
}

function makeSoftTexture(): THREE.CanvasTexture {
  const size = 128;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d')!;
  const gradient = ctx.createRadialGradient(size / 2, size / 2, 2, size / 2, size / 2, size / 2);
  gradient.addColorStop(0, 'rgba(255,255,255,1)');
  gradient.addColorStop(0.45, 'rgba(255,255,255,0.75)');
  gradient.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, size, size);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

export interface BurstOptions {
  color?: number;
  count?: number;
  speed?: number;
  life?: number;
  gravity?: number;
  size?: number;
  upward?: number;
  spread?: number;
}

export class Particles {
  private readonly pool: Particle[] = [];
  private cursor = 0;
  private readonly texture = makeSoftTexture();
  private readonly origin = new THREE.Vector3();

  clear(): void {
    for (const p of this.pool) { p.life = 0; p.sprite.visible = false; }
  }

  constructor(scene: THREE.Scene, capacity = 140) {
    for (let i = 0; i < capacity; i++) {
      const material = new THREE.SpriteMaterial({
        map: this.texture,
        color: 0xffffff,
        transparent: true,
        opacity: 0,
        depthWrite: false,
      });
      const sprite = new THREE.Sprite(material);
      sprite.visible = false;
      sprite.frustumCulled = false;
      scene.add(sprite);
      this.pool.push({
        sprite,
        material,
        velocity: new THREE.Vector3(),
        life: 0,
        maxLife: 1,
        gravity: 0,
        size: 20,
        spin: 0,
      });
    }
  }

  burst(position: Vec3 | THREE.Vector3, options: BurstOptions = {}): void {
    if (position instanceof THREE.Vector3) this.origin.copy(position);
    else this.origin.set(position[0], position[1], position[2]);

    const count = options.count ?? 8;
    const speed = options.speed ?? 110;
    const life = options.life ?? 0.5;
    const gravity = options.gravity ?? 180;
    const size = options.size ?? 26;
    const upward = options.upward ?? 40;
    const spread = options.spread ?? 1;

    for (let i = 0; i < count; i++) {
      const particle = this.pool[this.cursor];
      this.cursor = (this.cursor + 1) % this.pool.length;

      const angle = Math.random() * Math.PI * 2;
      const radius = Math.random() * speed;
      particle.velocity.set(
        Math.cos(angle) * radius * spread,
        upward + Math.random() * speed * 0.65,
        Math.sin(angle) * radius * spread,
      );
      particle.life = life * (0.7 + Math.random() * 0.6);
      particle.maxLife = particle.life;
      particle.gravity = gravity;
      particle.size = size * (0.6 + Math.random() * 0.8);
      particle.spin = (Math.random() - 0.5) * 2;

      particle.sprite.position.copy(this.origin);
      particle.sprite.position.x += (Math.random() - 0.5) * 18;
      particle.sprite.position.z += (Math.random() - 0.5) * 18;
      particle.sprite.visible = true;
      particle.material.color.setHex(options.color ?? 0xf3e6c8);
      particle.material.opacity = 0.9;
      particle.material.rotation = Math.random() * Math.PI;
      particle.sprite.scale.setScalar(particle.size);
    }
  }

  dust(position: Vec3 | THREE.Vector3, count = 6): void {
    this.burst(position, {
      color: 0xf0e2c0,
      count,
      speed: 90,
      life: 0.55,
      gravity: 150,
      size: 30,
      upward: 30,
      spread: 1,
    });
  }

  sparks(position: Vec3 | THREE.Vector3, count = 10): void {
    this.burst(position, {
      color: 0xffdd70,
      count,
      speed: 330,
      life: 0.32,
      gravity: 720,
      size: 18,
      upward: 120,
      spread: 1,
    });
  }

  pound(position: Vec3 | THREE.Vector3): void {
    this.burst(position, {
      color: 0xf6e5b8,
      count: 26,
      speed: 380,
      life: 0.65,
      gravity: 260,
      size: 34,
      upward: 160,
      spread: 1,
    });
    this.burst(position, {
      color: 0xffd35e,
      count: 10,
      speed: 260,
      life: 0.4,
      gravity: 600,
      size: 16,
      upward: 220,
      spread: 1,
    });
  }

  update(dt: number): void {
    for (const particle of this.pool) {
      if (!particle.sprite.visible) continue;

      particle.life -= dt;
      if (particle.life <= 0) {
        particle.sprite.visible = false;
        particle.material.opacity = 0;
        continue;
      }

      particle.velocity.y -= particle.gravity * dt;
      particle.sprite.position.addScaledVector(particle.velocity, dt);
      particle.material.rotation += particle.spin * dt;

      const lifeRatio = particle.life / particle.maxLife;
      particle.material.opacity = Math.min(1, lifeRatio * 1.6) * 0.85;
      const scale = particle.size * (0.7 + (1 - lifeRatio) * 0.9);
      particle.sprite.scale.setScalar(scale);
    }
  }
}
