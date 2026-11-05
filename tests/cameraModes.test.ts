import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { CameraRig } from '../src/game/CameraRig';

describe('camera modes', () => {
  it('cycles through the experiment camera presets', () => {
    const rig = new CameraRig(new THREE.PerspectiveCamera(), []);
    expect(rig.mode).toBe('lakitu');
    expect(rig.cycleMode()).toBe('close');
    expect(rig.distance).toBe(480);
    expect(rig.cycleMode()).toBe('topdown');
    expect(rig.pitch).toBeCloseTo(1.18);
    expect(rig.cycleMode()).toBe('side');
    expect(rig.cycleMode()).toBe('lakitu');
  });
});
