import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { A_BUTTON, ACT_FLAG_AIR } from '../src/sim/state';
import { LIMITS, RUNNING_JUMP, RunnerTrack, THEMES, floorHops, maxGap, maxRise } from '../src/game/experiments/runnerGen';
import { addGround } from './helpers';

/** A running single jump measured through the real MarioRuntime. */
function measureRunningJump(): { distance: number; height: number } {
  const world = new CollisionWorld();
  addGround(world, 16000);
  const m = new MarioRuntime(world, { pos: [0, 0, -7000], faceYaw: 0 });
  const forward = { stickX: 0, stickY: 64, stickMag: 64, buttonDown: 0, buttonPressed: 0 };
  for (let i = 0; i < 60; i++) m.step(forward, 0x8000);
  const start = m.pos[2];
  let peak = 0;
  for (let i = 0; i < 120; i++) {
    const air = Boolean(m.action & ACT_FLAG_AIR);
    m.step({ ...forward, buttonDown: A_BUTTON, buttonPressed: i === 0 ? A_BUTTON : 0 }, 0x8000);
    peak = Math.max(peak, m.pos[1]);
    if (air && !(m.action & ACT_FLAG_AIR)) break;
  }
  return { distance: m.pos[2] - start, height: peak };
}

const segments = (seed: number, count: number) => {
  const track = new RunnerTrack(seed, 0, 1000);
  return Array.from({ length: count }, (_, i) => track.segment(i));
};

describe('endless runner track', () => {
  it('keeps its limits well inside Mario’s measured running jump', () => {
    const jump = measureRunningJump();
    expect(jump.distance).toBeGreaterThan(RUNNING_JUMP.distance * 0.95);
    expect(jump.height).toBeGreaterThan(RUNNING_JUMP.height * 0.95);
    expect(LIMITS.gap[1]).toBeLessThan(jump.distance * 0.6);
    expect(LIMITS.rise[1]).toBeLessThan(jump.height * 0.55);
    expect(LIMITS.hurdle).toBeLessThan(jump.height * 0.4);
  });

  it('is continuous and deterministic per seed', () => {
    const a = segments(11, 30), b = segments(11, 30), c = segments(12, 30);
    expect(a.map(s => [s.kind, s.start, s.end])).toEqual(b.map(s => [s.kind, s.start, s.end]));
    expect(a.map(s => s.kind)).not.toEqual(c.map(s => s.kind));
    for (let i = 1; i < a.length; i++) expect(a[i].start).toBe(a[i - 1].end);
  });

  it('never asks for a gap or step Mario cannot make', () => {
    for (let seed = 1; seed <= 40; seed++) {
      for (const hop of floorHops(segments(seed, 40))) {
        expect(hop.gap, `seed ${seed}`).toBeLessThanOrEqual(maxGap(hop.difficulty) + 1e-6);
        expect(hop.rise, `seed ${seed}`).toBeLessThanOrEqual(maxRise(hop.difficulty) + 1e-6);
      }
    }
  });

  it('gets harder and changes theme the further you run', () => {
    const run = segments(5, 40);
    expect(run[35].difficulty).toBeGreaterThan(run[3].difficulty);
    expect(new Set(run.map(s => s.theme)).size).toBe(THEMES.length);
    expect(new Set(run.map(s => s.kind)).size).toBeGreaterThanOrEqual(6);
  });
});
