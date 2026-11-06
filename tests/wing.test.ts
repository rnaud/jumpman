import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ACT_FLYING, ACT_FLYING_TRIPLE_JUMP, ACT_BACKWARD_AIR_KB, ACT_LAVA_BOOST, MARIO_METAL_CAP, MARIO_VANISH_CAP } from '../src/sim/state';
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

  it('lowers the nose at the edge of the collision mesh and keeps flying', () => {
    const world = new CollisionWorld();
    addGround(world,1000);
    const s = new Sim(world);
    const m = s.mario;
    m.grantWingCap(); m.pos = [0,300,499]; m.forwardVel = 60; m.setAction(ACT_FLYING);
    s.tick({});
    expect(m.action).toBe(ACT_FLYING);
    expect(m.faceAngle[0]).toBeLessThan(0);
    expect(m.events.some(event => event.type === 'flightBoundary')).toBe(true);
    s.tick({});
    expect(m.events.some(event => event.type === 'flightBoundary')).toBe(false);
  });

  it.each([0,1])('hits a real wall with surface type %i in flight', type => {
    const world = new CollisionWorld();
    addGround(world,4000);
    world.addQuad([500,0,100],[-500,0,100],[-500,1000,100],[500,1000,100],{type});
    world.finalize();
    const s = new Sim(world), m = s.mario;
    m.grantWingCap(); m.pos=[0,300,0]; m.forwardVel=80; m.setAction(ACT_FLYING);
    for(let i=0;i<4 && m.action===ACT_FLYING;i++) s.tick({});
    expect(m.action).toBe(type ? ACT_LAVA_BOOST : ACT_BACKWARD_AIR_KB);
    if(type){
      expect(m.vel[1]).toBe(84);
      expect(m.hurtCounter).toBeGreaterThan(0);
    } else {
      expect(m.forwardVel).toBe(-16);
      expect(m.vel[1]).toBeLessThanOrEqual(0);
    }
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
