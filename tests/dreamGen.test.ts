import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { A_BUTTON, ACT_FLAG_AIR } from '../src/sim/state';
import {
  BIOMES, DreamGenerator, MARIO_SINGLE_JUMP, ROUTE_LIMITS, biomeFor, hop, maxGap, maxRise, type Platform,
} from '../src/game/experiments/dreamGen';
import { addGround } from './helpers';

const ORIGIN = { x: 60000, y: 1000, z: 0, heading: Math.PI / 2 };

function routeOf(generator: DreamGenerator, pieces: number): Array<{ platform: Platform; difficulty: number }> {
  const route: Array<{ platform: Platform; difficulty: number }> = [];
  for (let i = 0; i < pieces; i++) {
    const piece = generator.piece(i);
    for (const platform of piece.platforms) if (platform.route) route.push({ platform, difficulty: piece.difficulty });
  }
  return route;
}

/** A running single jump measured through the real MarioRuntime. */
function measureSingleJump(): { distance: number; height: number } {
  const world = new CollisionWorld();
  addGround(world, 16000);
  const m = new MarioRuntime(world, { pos: [0, 0, -7000], faceYaw: 0 });
  const forward = { stickX: 0, stickY: 64, stickMag: 64, buttonDown: 0, buttonPressed: 0 };
  for (let i = 0; i < 60; i++) m.step(forward, 0x8000);
  const start = m.pos[2];
  let peak = 0;
  let jumped = false;
  for (let i = 0; i < 120; i++) {
    const air = Boolean(m.action & ACT_FLAG_AIR);
    m.step({ ...forward, buttonDown: A_BUTTON, buttonPressed: jumped ? 0 : A_BUTTON }, 0x8000);
    jumped = true;
    peak = Math.max(peak, m.pos[1]);
    if (air && !(m.action & ACT_FLAG_AIR)) break;
  }
  return { distance: m.pos[2] - start, height: peak };
}

describe('infinite dream generator', () => {
  it('uses limits within Mario’s measured single jump', () => {
    const measured = measureSingleJump();
    expect(measured.distance).toBeGreaterThan(MARIO_SINGLE_JUMP.distance * 0.95);
    expect(measured.height).toBeGreaterThan(MARIO_SINGLE_JUMP.height * 0.95);
    // Even the hardest hop leaves a wide safety margin.
    expect(ROUTE_LIMITS.gap[1]).toBeLessThan(measured.distance * 0.6);
    expect(ROUTE_LIMITS.rise[1]).toBeLessThan(measured.height * 0.6);
  });

  it('is deterministic per seed and differs across seeds', () => {
    const a = routeOf(new DreamGenerator(7, ORIGIN), 20).map(entry => [entry.platform.x, entry.platform.y, entry.platform.z]);
    const b = routeOf(new DreamGenerator(7, ORIGIN), 20).map(entry => [entry.platform.x, entry.platform.y, entry.platform.z]);
    const c = routeOf(new DreamGenerator(8, ORIGIN), 20).map(entry => [entry.platform.x, entry.platform.y, entry.platform.z]);
    expect(a).toEqual(b);
    expect(a).not.toEqual(c);
  });

  it('keeps every hop on the route jumpable across many seeds', () => {
    for (let seed = 1; seed <= 40; seed++) {
      const route = routeOf(new DreamGenerator(seed, ORIGIN), 36);
      expect(route.length).toBeGreaterThan(60);
      for (let i = 1; i < route.length; i++) {
        const { gap, rise } = hop(route[i - 1].platform, route[i].platform);
        const difficulty = Math.max(route[i - 1].difficulty, route[i].difficulty);
        expect(rise, `seed ${seed} hop ${i} rise`).toBeLessThanOrEqual(maxRise(difficulty) + 1e-6);
        expect(gap, `seed ${seed} hop ${i} gap`).toBeLessThanOrEqual(maxGap(difficulty, rise) + 1e-6);
      }
    }
  });

  it('moves forward, grows harder, and passes through every biome', () => {
    const generator = new DreamGenerator(3, ORIGIN);
    const kinds = new Set<string>();
    for (let i = 0; i < 34; i++) kinds.add(generator.piece(i).kind);
    expect(generator.piece(33).exit.x - ORIGIN.x).toBeGreaterThan(20000);
    expect(generator.piece(30).difficulty).toBeGreaterThan(generator.piece(2).difficulty);
    expect(new Set(Array.from({ length: 34 }, (_, i) => biomeFor(i))).size).toBe(BIOMES.length);
    expect(kinds.size).toBeGreaterThanOrEqual(6);
  });
});
