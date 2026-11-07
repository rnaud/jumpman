import { Vector3, PerspectiveCamera } from 'three';
import { expect, it } from 'vitest';
import { CameraRig } from '../src/game/CameraRig';
import { radialWallYaw } from '../src/game/cameraWall';
import { CollisionWorld } from '../src/sim/collision';
import { SURFACE_FLAG_NO_CAM_COLLISION } from '../src/sim/surfaces';

function wall(height: number, type = 0, flags = 0) {
  const world = new CollisionWorld();
  world.addQuad([2500,0,-3900],[2500,height,-3900],[3700,height,-3900],[3700,0,-3900],{type,flags});
  world.finalize();
  return world;
}

const mario = new Vector3(3072,0,-4200);
const eye = new Vector3(3072,266,-3210);

it('deflects to the wall tangent on the camera-facing side of tall blocking triangles', () => {
  expect(radialWallYaw(mario,eye,wall(900))).toBe(-0x4400);
  expect(radialWallYaw(mario,eye,wall(149))).toBeUndefined();
  expect(radialWallYaw(mario,eye,wall(900,0x28))).toBeUndefined();
  expect(radialWallYaw(mario,eye,wall(900,0,SURFACE_FLAG_NO_CAM_COLLISION))).toBeUndefined();
  expect(radialWallYaw(eye,mario,wall(900))).toBeUndefined();
});

it('swings the radial camera clear while keeping the native eye distance', () => {
  const run = (fps: number) => {
    const world = wall(900);
    const camera = new PerspectiveCamera();
    const rig = new CameraRig(camera,[]);
    rig.setBattlefieldFloor(0);
    rig.setCollisionWorld(world);
    for (let frame=0;frame<fps*4;frame++) rig.update(1/fps,mario,0);
    expect(camera.position.x).toBeGreaterThan(mario.x+870);
    expect(camera.position.z).toBeLessThan(-3900);
    expect(Math.hypot(camera.position.x-mario.x,camera.position.z-mario.z)).toBeCloseTo(990,-1);
    return camera.position;
  };
  expect(run(30).distanceTo(run(120))).toBeLessThan(0.01);
});
