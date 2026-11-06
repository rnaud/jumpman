import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ACT_FLYING, ACT_FLYING_TRIPLE_JUMP, MARIO_METAL_CAP, MARIO_VANISH_CAP } from '../src/sim/state';
import { addGround, Sim } from './helpers';

function sim(): Sim {
  const world = new CollisionWorld();
  addGround(world, 8000, 0);
  return new Sim(world);
}

describe('wing cap', () => {
  it('grants metal and vanish cap effects', () => {
    const s = sim();
    s.mario.grantMetalCap(60);
    s.mario.grantVanishCap(60);
    s.tick({});
    expect(s.mario.flags & MARIO_METAL_CAP).toBeTruthy();
    expect(s.mario.flags & MARIO_VANISH_CAP).toBeTruthy();
    expect(s.mario.vanished).toBe(true);
    expect(s.mario.takeDamage(2, [100, 0, 0])).toBe(false);
    for (let i = 0; i < 62; i++) s.tick({});
    expect(s.mario.flags & MARIO_METAL_CAP).toBeFalsy();
    expect(s.mario.flags & MARIO_VANISH_CAP).toBeFalsy();
  });

  it('keeps ordinary jumps ballistic and takes flight from a winged triple jump', () => {
    const s = sim();
    s.mario.grantWingCap(30);
    s.tick({ a: true, aPressed: true });
    let flew = false;
    for (let i = 0; i < 20; i++) {
      s.tick({ a: true });
      if (s.mario.action === ACT_FLYING) flew = true;
    }
    expect(flew).toBe(false);
    s.mario.grantWingCap(120);
    s.mario.setAction(ACT_FLYING_TRIPLE_JUMP);
    for(let i=0;i<32 && !flew;i++) { s.tick({a:true}); flew=s.mario.action===ACT_FLYING; }
    expect(flew).toBe(true);
    expect(s.mario.flags & 1 << 3).toBeTruthy();
  });
});
