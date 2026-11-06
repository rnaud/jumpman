import { describe, expect, it, vi } from 'vitest';
import * as THREE from 'three';
import { CameraRig } from '../src/game/CameraRig';
import { WalkingGoomba } from '../src/game/goomba';
import { MovingPlatform } from '../src/game/movingPlatform';
import { InputManager } from '../src/game/Input';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { ACT_BREASTSTROKE, ACT_FLUTTER_KICK, ACT_SWIMMING_END, ACT_WATER_JUMP, A_BUTTON, emptyController } from '../src/sim/state';
import { addGround } from './helpers';

describe('native course behaviors', () => {
  it('pulls inside a new camera blocker on the very first obstructed frame', () => {
    const camera = new THREE.PerspectiveCamera();
    const wall = new THREE.Mesh(new THREE.BoxGeometry(600, 900, 20));
    wall.position.z = 2000; wall.updateMatrixWorld();
    const rig = new CameraRig(camera, [wall]);
    rig.update(1 / 60, new THREE.Vector3(), 0);
    expect(camera.position.z).toBeGreaterThan(800);
    wall.position.z = 180; wall.updateMatrixWorld();
    rig.update(1 / 60, new THREE.Vector3(), 0);
    expect(camera.position.z).toBeLessThan(170);
  });

  it('a held Start produces exactly one pause edge and a second press resumes', () => {
    const target = new EventTarget();
    const buttons = Array.from({ length: 10 }, () => ({ pressed: false, value: 0 }));
    vi.stubGlobal('window', target);
    vi.stubGlobal('navigator', { getGamepads: () => [{ index: 0, connected: true, axes: [], buttons }] });
    try {
      const input = new InputManager(target as HTMLCanvasElement);
      input.pollGamepad(); buttons[9].pressed = true; input.pollGamepad();
      expect(input.consumeStart()).toBe(true);
      input.clear(); input.pollGamepad(); expect(input.consumeStart()).toBe(false);
      buttons[9].pressed = false; input.pollGamepad();
      buttons[9].pressed = true; input.pollGamepad(); expect(input.consumeStart()).toBe(true);
    } finally { vi.unstubAllGlobals(); }
  });

  it('Goomba hops on alert, chases on landing, and a defeated triplet stays dead until reset', () => {
    const world = new CollisionWorld(); addGround(world);
    const enemy = new WalkingGoomba(new THREE.Group(), [0, 0, 0]);
    enemy.reset(); enemy.step([5000,0,0],world);
    expect(enemy.model.visible).toBe(true);
    expect(enemy.model.position.toArray()).toEqual([0,0,0]);
    enemy.step([0, 0, 350], world);
    expect(enemy.model.position.y).toBe(21);
    for (let i = 0; i < 28; i++) enemy.step([0, 0, 350], world);
    expect(enemy.model.position.z).toBeGreaterThan(0);
    enemy.defeat(); enemy.step([5000, 0, 0], world); enemy.step([0, 0, 0], world);
    expect(enemy.model.visible).toBe(false);
    enemy.reset(); enemy.step([0, 0, 350], world); expect(enemy.model.visible).toBe(true);
  });

  it('cycles the paired checkerboard lifts in opposite directions and returns both to their starts', () => {
    const world = new CollisionWorld();
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    for (const direction of [1, -1]) {
      const model = new THREE.Group();
      const lift = new MovingPlatform(model, { vertices: [], triangles: [] }, 'checkerboard_lift', 40 * direction);
      expect(lift.step(mario)).toBe(direction > 0 ? 'lower' : 'upper');
      for (let i = 0; i < 42; i++) lift.step(mario);
      expect(model.position.y).toBe(420 * direction);
      for (let i = 0; i < 32; i++) lift.step(mario);
      expect(model.position.z * direction).toBeGreaterThan(140);
      expect(model.rotation.x).toBeCloseTo(direction > 0 ? Math.PI / 2 : 0);
      for (let i = 0; i < 32 + 42 + 64; i++) lift.step(mario);
      expect(model.position.length()).toBeCloseTo(0);
    }
  });

  it('a rider tilts the seesaw and stays on its moving floor', () => {
    const world = new CollisionWorld(); addGround(world, 4000, -200);
    const model = new THREE.Group();
    const platform = new MovingPlatform(model, {
      vertices: [[-100,0,-300], [-100,0,300], [100,0,300], [100,0,-300]],
      triangles: [[0,1,2,0,0], [0,2,3,0,0]],
    }, 'seesaw_platform', 0);
    platform.refresh(world);
    const m = new MarioRuntime(world, { pos: [0, 0, 200], faceYaw: 0 });
    for (let i = 0; i < 30; i++) { platform.step(m); m.step(emptyController(), 0); }
    expect(model.rotation.x).toBeGreaterThan(0);
    expect(m.pos[1]).toBeLessThan(-5);
    expect(m.pos[1]).toBeCloseTo(m.floorHeight);
    expect(world.surfaces).toHaveLength(4);
    platform.reset(world); expect(world.findFloor(0, 10, 200).height).toBeCloseTo(0);
  });

  it('uses the wooden platform wait/turn cycle and signed first-byte rotation speed', () => {
    const world = new CollisionWorld(); addGround(world);
    const m = new MarioRuntime(world, { pos: [1000, 0, 1000], faceYaw: 0 });
    const data = { vertices: [], triangles: [] };
    const wood = new THREE.Group();
    const timed = new MovingPlatform(wood, data, 'rotating_wooden_platform', 0);
    for (let i = 0; i < 62; i++) timed.step(m);
    expect(wood.rotation.y).toBe(0);
    for (let i = 0; i < 128; i++) timed.step(m);
    expect(wood.rotation.y).toBeCloseTo(Math.PI);
    timed.step(m); expect(wood.rotation.y).toBeCloseTo(Math.PI);
    const rotating = new THREE.Group();
    const continuous = new MovingPlatform(rotating, data, 'rotating_platform', 8);
    continuous.step(m);
    expect(rotating.rotation.y).toBeCloseTo(0x80 * Math.PI / 32768);
  });

  it('uses the sliding block speed parameter and its source endpoints', () => {
    const random = vi.spyOn(Math, 'random').mockReturnValue(0);
    try {
      const world = new CollisionWorld(); addGround(world);
      const m = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
      const model = new THREE.Group(); model.position.x = 3328;
      const platform = new MovingPlatform(model, { vertices: [], triangles: [] }, 'sliding_platform', 2);
      for (let i = 0; i < 102; i++) platform.step(m);
      expect(model.position.x).toBe(3345);
      for (let i = 0; i < 40; i++) platform.step(m);
      expect(model.position.x).toBe(3840);
      for (let i = 0; i < 65; i++) platform.step(m);
      expect(model.position.x).toBe(3330);
    } finally { random.mockRestore(); }
  });

  it('holds a stroke into flutter kick, releases to glide, and jumps out at the surface', () => {
    const world = new CollisionWorld(); addGround(world, 4000, -700);
    world.waterBoxes = [[0, -500, -500, 500, 500, 0]];
    const m = new MarioRuntime(world, { pos: [0, -700, 0], faceYaw: 0 });
    m.step(emptyController(), 0);
    expect(m.pos[1]).toBeGreaterThan(-110);
    const held = { ...emptyController(), buttonDown: A_BUTTON };
    m.step({ ...held, buttonPressed: A_BUTTON }, 0);
    expect(m.action).toBe(ACT_BREASTSTROKE);
    for (let i = 0; i < 14; i++) m.step(held, 0);
    expect(m.action).toBe(ACT_FLUTTER_KICK);
    m.step(emptyController(), 0); expect(m.action).toBe(ACT_SWIMMING_END);
    m.pos[1] = -80;
    m.step({ ...held, buttonPressed: A_BUTTON, stickY: -64, stickMag: 64 }, 0);
    expect(m.action).toBe(ACT_WATER_JUMP);
    m.step(emptyController(), 0); expect(m.pos[1]).toBeGreaterThan(-80);
  });
});
