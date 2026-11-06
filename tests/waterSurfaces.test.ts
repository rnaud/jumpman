import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { addWaterSurfaces } from '../src/game/waterSurfaces';

describe('animated water surfaces', () => {
  it('creates, updates and disposes a water surface', () => {
    const scene = new THREE.Scene();
    const surfaces = addWaterSurfaces(scene, [[1, -500, -500, 500, 500, 100]]);
    expect(surfaces).toHaveLength(1);
    const geometry = surfaces[0].mesh.geometry;
    const before = Float32Array.from(geometry.attributes.position.array as Float32Array);
    surfaces[0].update(1.5);
    const after = Float32Array.from(geometry.attributes.position.array as Float32Array);
    expect(after.some((value, index) => Math.abs(value - before[index]) > 0.01)).toBe(true);
    surfaces[0].dispose();
    expect(scene.children).toHaveLength(0);
  });
});
