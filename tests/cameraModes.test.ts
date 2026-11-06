import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { ACT_LONG_JUMP, ACT_SLEEPING } from '../src/sim/state';
import { CameraRig } from '../src/game/CameraRig';

describe('camera modes', () => {
  it('frames both sides of the mountain trail using the native radial focus and pan', () => {
    const world = new CollisionWorld();
    addNativeCollision(world,JSON.parse(readFileSync(new URL('../assets/courses/bob.json',import.meta.url),'utf8')));
    world.finalize();
    for (const [x,y,z] of [[3930,3600,-6800],[160,1050,-2780]]) {
      const floor = world.findFloor(x,y,z).height;
      const mario = new THREE.Vector3(x,floor,z);
      const camera = new THREE.PerspectiveCamera();
      const rig = new CameraRig(camera,[]);
      rig.setCollisionWorld(world);
      rig.setBattlefieldFloor(0);
      const yaw = Math.atan2(x-3072,z+4608);
      const face = Math.round(yaw*32768/Math.PI) + 0x4000;
      const run = (action: number) => {
        for (let i=0;i<360;i++) rig.update(1/30,mario,face,false,0,undefined,action);
        const view = camera.getWorldDirection(new THREE.Vector3());
        return camera.position.clone().addScaledVector(view,(floor+125-camera.position.y)/view.y);
      };
      const forward = run(0);
      const eye = camera.position.clone();
      const original = mario.clone().add(new THREE.Vector3(0,125,0));
      const pan = Math.sin(0xc00*Math.PI/32768)*eye.distanceTo(mario);
      expect(forward.x-original.x).toBeCloseTo(Math.cos(yaw)*pan,0);
      expect(forward.z-original.z).toBeCloseTo(-Math.sin(yaw)*pan,0);
      const reverse = run(ACT_LONG_JUMP);
      expect(reverse.clone().add(forward).distanceTo(original.clone().multiplyScalar(2))).toBeLessThan(0.1);
      expect(run(ACT_SLEEPING).distanceTo(original)).toBeLessThan(0.1);
      expect(camera.position.distanceTo(eye)).toBeLessThan(0.1);
    }
  });

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
