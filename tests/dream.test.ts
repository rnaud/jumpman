import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { DreamWorld } from '../src/game/dream';

describe('infinite dream generator', () => {
  it('streams deterministic islands and removes them when disabled', () => {
    const world = new CollisionWorld();
    const dream = new DreamWorld(new THREE.Scene(), world, 42);
    expect(dream.toggle([0, 0, 0])).toBe(true);
    const first = dream.debug();
    expect(first.enabled).toBe(true);
    expect(first.islands).toBeGreaterThan(0);
    expect(world.surfaces.length).toBeGreaterThan(0);

    const other = new DreamWorld(new THREE.Scene(), new CollisionWorld(), 42);
    other.toggle([0, 0, 0]);
    expect(other.debug().islands).toBe(first.islands);

    dream.disable();
    expect(dream.debug().islands).toBe(0);
  });

  it('keeps the main island spine reachable between cells', () => {
    const world = new CollisionWorld();
    const dream = new DreamWorld(new THREE.Scene(), world, 42);
    dream.toggle([0, 0, 0]);
    const path = dream.debugPath();
    expect(path.length).toBeGreaterThan(1);
    const a = path.find(entry => Math.abs(entry.x) < 1 && Math.abs(entry.z) < 1);
    const b = path.find(entry => Math.abs(entry.x - 620) < 1 && Math.abs(entry.z) < 1);
    expect(a).toBeDefined();
    expect(b).toBeDefined();
    if (!a || !b) return;
    const gap = 620 - (a.width + b.width) / 2;
    expect(gap).toBeLessThan(300);
    expect(Math.abs(a.y - b.y)).toBeLessThan(260);
    expect(a.difficulty).toBe(0);
    expect(b.difficulty).toBeGreaterThan(0);
    const floor = world.findFloor(a.x, a.top + 100, a.z);
    expect(floor.surface).not.toBeNull();
    expect(floor.height).toBeCloseTo(a.top, 3);
  });

  it('changes biome bands with distance and recovers ownership of floors', () => {
    const world = new CollisionWorld();
    const dream = new DreamWorld(new THREE.Scene(), world, 42);
    dream.toggle([0, 0, 0]);
    const near = dream.debugPath().find(entry => Math.abs(entry.x) < 1 && Math.abs(entry.z) < 1)!;
    const floor = world.findFloor(near.x, near.top + 100, near.z);
    expect(dream.ownsSurface(floor.surface)).toBe(true);
    expect(dream.ownsSurface(null)).toBe(false);
    dream.update([5000, 0, 5000]);
    const far = dream.debugPath().find(entry => Math.abs(entry.x - 4960) < 1 && Math.abs(entry.z - 4960) < 1);
    expect(far).toBeDefined();
    if (far) expect(far.biome).not.toBe(near.biome);
  });

  it('does not generate new cells while disabled', () => {
    const dream = new DreamWorld(new THREE.Scene(), new CollisionWorld(), 7);
    dream.update([10000, 0, 10000]);
    expect(dream.debug().islands).toBe(0);
  });
});
