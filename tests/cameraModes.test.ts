import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { CameraRig } from '../src/game/CameraRig';

describe('camera modes', () => {
  it('eases behind Mario after manual orbit input expires', () => {
    const rig = new CameraRig(new THREE.PerspectiveCamera(), []);
    const position = new THREE.Vector3();
    rig.yaw = 0;
    rig.update(1 / 30, position, 0, true);
    expect(rig.yaw).toBeGreaterThan(0);

    rig.orbit(1, 0);
    const manualYaw = rig.yaw;
    rig.update(0.2, position, 0, true);
    expect(rig.yaw).toBeCloseTo(manualYaw, 5);
    rig.update(2.2, position, 0, true);
    expect(rig.yaw).not.toBeCloseTo(manualYaw, 3);
  });

  it('prioritizes boss focus over water context', () => {
    const rig = new CameraRig(new THREE.PerspectiveCamera(), []);
    expect(rig.context).toBe('normal');
    rig.setWaterMode(true);
    expect(rig.context).toBe('water');
    rig.setBossFocus(new THREE.Vector3(1000, 0, 0));
    expect(rig.context).toBe('boss');
    rig.setBossFocus(null);
    expect(rig.context).toBe('water');
    rig.setWaterMode(false);
    expect(rig.context).toBe('normal');
  });

  it('uses a low slide context and gives boss focus priority', () => {
    const rig = new CameraRig(new THREE.PerspectiveCamera(), []);
    rig.setSlideMode(true);
    expect(rig.context).toBe('slide');
    rig.setBossFocus(new THREE.Vector3(1, 0, 0));
    expect(rig.context).toBe('boss');
    rig.setBossFocus(null);
    expect(rig.context).toBe('slide');
    rig.setSlideMode(false);
    expect(rig.context).toBe('normal');
  });

  it('prioritizes flight and interior camera contexts', () => {
    const rig = new CameraRig(new THREE.PerspectiveCamera(), []);
    rig.setInteriorMode(true);
    expect(rig.context).toBe('interior');
    rig.setFlightMode(true);
    expect(rig.context).toBe('flight');
    rig.setWaterMode(true);
    expect(rig.context).toBe('water');
    rig.setBossFocus(new THREE.Vector3(1, 0, 0));
    expect(rig.context).toBe('boss');
  });

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
