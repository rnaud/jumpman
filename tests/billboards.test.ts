import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { Sm64Billboards } from '../src/game/sm64Assets';

describe('SM64 billboards', () => {
  it('registers billboards discovered after the initial scene scan exactly once', () => {
    const root = new THREE.Group();
    const original = new THREE.Group();
    original.userData.sm64Command = 'GEO_BILLBOARD_5';
    root.add(original);

    const billboards = new Sm64Billboards(root);
    expect(billboards.size).toBe(1);

    const added = new THREE.Group();
    const late = new THREE.Group();
    late.userData.sm64Command = 'GEO_BILLBOARD_5';
    added.add(late);
    billboards.register(added);
    billboards.register(added);
    expect(billboards.size).toBe(2);

    const camera = new THREE.PerspectiveCamera();
    camera.position.set(0, 0, 100);
    camera.lookAt(0, 0, 0);
    camera.updateMatrixWorld();
    billboards.update(camera);
    expect(original.quaternion.lengthSq()).toBeCloseTo(1);
    expect(late.quaternion.lengthSq()).toBeCloseTo(1);
  });
});
