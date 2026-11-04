import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import {
  A_BUTTON,
  B_BUTTON,
  Z_TRIG,
  type ControllerSnapshot,
} from '../src/sim/state';
import type { Vec3 } from '../src/sim/math';

export function addGround(world: CollisionWorld, size = 4000, y = 0): void {
  const h = size / 2;
  world.addQuad([-h, y, -h], [-h, y, h], [h, y, h], [h, y, -h]);
  world.finalize();
}

export interface BoxOptions {
  type?: number;
  addBottom?: boolean;
}

/** Add an axis-aligned box with outward-facing collision triangles. */
export function addBox(
  world: CollisionWorld,
  cx: number,
  cy: number,
  cz: number,
  w: number,
  h: number,
  d: number,
  options: BoxOptions = {},
): void {
  const x0 = cx - w / 2;
  const x1 = cx + w / 2;
  const y0 = cy;
  const y1 = cy + h;
  const z0 = cz - d / 2;
  const z1 = cz + d / 2;
  const opts = { type: options.type ?? 0 };

  const top = [
    [-x1, y1, -z1],
    [-x1, y1, z1],
    [x1, y1, z1],
    [x1, y1, -z1],
  ] as Vec3[];

  world.addQuad(top[0], top[1], top[2], top[3], opts);

  if (options.addBottom !== false) {
    world.addQuad([-x1, y0, -z1], [x1, y0, -z1], [x1, y0, z1], [-x1, y0, z1], opts);
  }

  // +Z, -Z, +X, -X
  world.addQuad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], opts);
  world.addQuad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], opts);
  world.addQuad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], opts);
  world.addQuad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], opts);

  world.finalize();
}

/**
 * A 45-degree ramp rising toward +Z.
 * Low edge is at z=0,y=0; high edge at z=length,y=height.
 */
export function addRamp(
  world: CollisionWorld,
  cx: number,
  cz: number,
  width: number,
  length: number,
  height: number,
  type = 0,
): void {
  const x0 = cx - width / 2;
  const x1 = cx + width / 2;
  const z0 = cz - length / 2;
  const z1 = cz + length / 2;

  // Sloped top, rising toward +Z. Winding chosen so the normal points up.
  world.addQuad([x0, 0, z0], [x0, height, z1], [x1, height, z1], [x1, 0, z0], { type });
  // Back face and sides.
  world.addQuad([x0, 0, z1], [x1, 0, z1], [x1, height, z1], [x0, height, z1], { type });
  world.addQuad([x0, 0, z0], [x0, 0, z1], [x0, height, z1], [x0, height, z0], { type });
  world.addQuad([x1, 0, z0], [x1, height, z0], [x1, height, z1], [x1, 0, z1], { type });
  world.finalize();
}

export interface TestInput {
  /** -1..1, right positive */
  x?: number;
  /** -1..1, up positive */
  y?: number;
  /** stick magnitude, default 64 */
  mag?: number;
  a?: boolean;
  aPressed?: boolean;
  b?: boolean;
  bPressed?: boolean;
  z?: boolean;
  zPressed?: boolean;
}

export function makeController(input: TestInput): ControllerSnapshot {
  const x = input.x ?? 0;
  const y = input.y ?? 0;
  const mag = input.mag ?? 64;
  const length = Math.hypot(x, y) || 1;
  let buttonDown = 0;
  let buttonPressed = 0;
  if (input.a) buttonDown |= A_BUTTON;
  if (input.aPressed) buttonPressed |= A_BUTTON;
  if (input.b) buttonDown |= B_BUTTON;
  if (input.bPressed) buttonPressed |= B_BUTTON;
  if (input.z) buttonDown |= Z_TRIG;
  if (input.zPressed) buttonPressed |= Z_TRIG;

  return {
    stickX: (x / length) * mag,
    stickY: (y / length) * mag,
    stickMag: Math.hypot(x, y) === 0 ? 0 : mag,
    buttonDown,
    buttonPressed,
  };
}

export class Sim {
  readonly world: CollisionWorld;
  readonly mario: MarioRuntime;

  /** Camera sits behind Mario by default, so "up" moves toward +Z. */
  readonly fixedCameraYaw: number;

  constructor(world: CollisionWorld, spawn: Vec3 = [0, 0, 0], faceYaw = 0) {
    this.world = world;
    this.mario = new MarioRuntime(world, { pos: spawn, faceYaw });
    this.fixedCameraYaw = faceYaw + 0x8000;
  }

  get cameraYaw(): number {
    return this.fixedCameraYaw;
  }

  tick(input: TestInput = {}, cameraYaw = this.cameraYaw): MarioRuntime {
    this.mario.step(makeController(input), cameraYaw);
    return this.mario;
  }

  run(input: TestInput, frames: number, cameraYaw = this.cameraYaw): MarioRuntime {
    for (let i = 0; i < frames; i++) {
      this.tick(i === 0 ? input : { ...input, aPressed: false, bPressed: false, zPressed: false }, cameraYaw);
    }
    return this.mario;
  }
}

/** Wait until Mario lands (or the frame budget runs out). */
export function waitForLanding(sim: Sim, maxFrames = 300): MarioRuntime {
  for (let i = 0; i < maxFrames; i++) {
    if (sim.mario.action === 0x04000470 || sim.mario.action === 0x0c000230) return sim.mario;
    sim.tick({});
  }
  return sim.mario;
}
