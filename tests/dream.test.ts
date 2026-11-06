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

  it('does not generate new cells while disabled', () => {
    const dream = new DreamWorld(new THREE.Scene(), new CollisionWorld(), 7);
    dream.update([10000, 0, 10000]);
    expect(dream.debug().islands).toBe(0);
  });
});
