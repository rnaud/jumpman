import * as THREE from 'three';
import { describe, it, expect } from 'vitest';
import { buildNativePlayground, PLAYGROUND_SIZE } from '../src/game/playgroundLayout';
import type { PlaygroundSkin } from '../src/game/world';

function build() {
  const material = new THREE.MeshBasicMaterial();
  const skin: PlaygroundSkin = {
    materials: { grass: material, stone: material, wood: material, ice: material },
    make: () => new THREE.Group(),
  };
  const scene = new THREE.Scene();
  return { scene, world: buildNativePlayground(scene, skin) };
}
describe('spacious playground', () => {
  it('has walkable ground across the expanded footprint and matching ramp collision', () => {
    const {world} = build();
    expect(PLAYGROUND_SIZE).toBe(12000);
    for (const [x,z] of [[5500,5500],[-5500,-5500],[0,500]]) {
      expect(world.collision.findFloor(x,2000,z).height).toBeCloseTo(0);
    }
    expect(world.collision.findFloor(3100,2000,300).height).toBeCloseTo(240);
    for (let i=0;i<12;i++) {
      expect(world.collision.findFloor(2900,2000,-2200-i*150).height).toBeCloseTo((i+1)*60);
    }
    expect(world.collision.findFloor(2900,2000,-4200).height).toBeCloseTo(720);
  });
  it('has no floating architectural meshes or intersecting obstacle interiors', () => {
    const {world,scene} = build();
    const bounds = world.collisionMeshes.slice(1).map(mesh => new THREE.Box3().setFromObject(mesh));
    for (const box of bounds) expect(box.min.y).toBeCloseTo(0);
    for (let i=0;i<bounds.length;i++) for (let j=i+1;j<bounds.length;j++) {
      const overlap = bounds[i].clone().intersect(bounds[j]);
      if (overlap.isEmpty()) continue;
      const size = overlap.getSize(new THREE.Vector3());
      expect(Math.min(size.x,size.y,size.z), `obstacles ${i} / ${j}: ${JSON.stringify(bounds[i])} / ${JSON.stringify(bounds[j])}`).toBeLessThan(0.01);
    }
    expect(scene.children.some(child => child instanceof THREE.Sprite)).toBe(false);
    expect(world.coins).toHaveLength(15);
  });
});
