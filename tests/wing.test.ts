import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ACT_FLYING } from '../src/sim/state';
import { addGround, Sim } from './helpers';

function sim(): Sim {
  const world = new CollisionWorld();
  addGround(world, 8000, 0);
  return new Sim(world);
}

describe('wing cap', () => {
  it('turns a falling jump into flight and expires back to freefall', () => {
    const s = sim();
    s.mario.grantWingCap(30);
    s.tick({ a: true, aPressed: true });
    let flew = false;
    for (let i = 0; i < 20; i++) {
      s.tick({ a: true });
      if (s.mario.action === ACT_FLYING) flew = true;
    }
    expect(flew).toBe(true);
    expect(s.mario.flags & 1 << 3).toBeTruthy();
  });
});
