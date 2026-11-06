import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ACT_BACKWARD_GROUND_KB, ACT_CROUCH_SLIDE, ACT_RIDING_SHELL_GROUND, ACT_RIDING_SHELL_JUMP } from '../src/sim/state';
import { SimObject } from '../src/sim/object';
import { addGround, Sim } from './helpers';

function mounted(water=false) {
  const world = new CollisionWorld();
  addGround(world,16000,water ? -300 : 0);
  if (water) world.waterBoxes=[[0,-7000,-7000,7000,7000,0]];
  const sim = new Sim(world);
  const shell = new SimObject({behavior:'koopaShell',pos:[0,0,0],yaw:0});
  sim.mario.pos=[0,0,0];
  sim.mario.rideShell(shell);
  return {sim,shell,world};
}

describe('Koopa shell riding', () => {
  it('accelerates, jumps, lands still riding and dismounts into a crouch slide', () => {
    const {sim,shell}=mounted();
    for(let i=0;i<25;i++)sim.tick({x:1});
    expect(sim.mario.forwardVel).toBeGreaterThan(20);
    expect(sim.mario.pos[0]).toBeLessThan(-150);
    sim.tick({x:1,a:true,aPressed:true});
    expect(sim.mario.action).toBe(ACT_RIDING_SHELL_JUMP);
    expect(sim.mario.ridingShell).toBe(shell);
    for(let i=0;i<90 && sim.mario.action!==ACT_RIDING_SHELL_GROUND;i++)sim.tick({x:1,a:true});
    expect(sim.mario.actionArg).toBe(1);
    expect(shell.pos).toEqual(sim.mario.pos);
    sim.tick({z:true,zPressed:true});
    expect(sim.mario.action).toBe(ACT_CROUCH_SLIDE);
    expect(sim.mario.ridingShell).toBeNull();
    expect(shell.active).toBe(false);
  });
  it('rides across a water surface without sinking to the underwater floor',()=>{
    const {sim}=mounted(true);
    for(let i=0;i<60;i++)sim.tick({});
    expect(sim.mario.action).toBe(ACT_RIDING_SHELL_GROUND);
    expect(sim.mario.pos[1]).toBe(0);
    expect(sim.mario.pos[2]).toBeGreaterThan(600);
  });
  it('loses the shell on a head-on wall impact',()=>{
    const {sim,shell,world}=mounted();
    world.addQuad([500,0,200],[-500,0,200],[-500,500,200],[500,500,200]);
    world.finalize();
    for(let i=0;i<60 && shell.active;i++)sim.tick({});
    expect(sim.mario.action).toBe(ACT_BACKWARD_GROUND_KB);
    expect(shell.active).toBe(false);
  });
});
