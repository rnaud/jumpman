import { describe, expect, it } from 'vitest';
import { applyGravity } from '../src/sim/marioStep';
import type { MarioState } from '../src/sim/state';

describe('editor gravity control', () => {
  it('scales gravitational acceleration without touching horizontal velocity', () => {
    const mario = { action: 0, vel: [12, 0, -4], gravityScale: 0.5 } as MarioState;
    applyGravity(mario);
    expect(mario.vel[1]).toBe(-2);
    expect(mario.vel[0]).toBe(12);
    expect(mario.vel[2]).toBe(-4);
  });
});
