import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { HundredMarios } from '../src/game/hundredMarios';
import { addGround } from './helpers';

describe('A Hundred Marios agents', () => {
  it('spawns, moves and disposes autonomous agents', () => {
    const world = new CollisionWorld();
    addGround(world);
    const marios = new HundredMarios(new THREE.Scene(), world);
    expect(marios.toggle([0, 0, 0], 12)).toBe(true);
    expect(marios.debug().count).toBe(12);
    expect(marios.debug().fullPhysics).toBe(8);
    const before = marios.debug().agents.map(agent => agent.pos);
    for (let i = 0; i < 30; i++) marios.step([0, 0, 0]);
    const after = marios.debug().agents.map(agent => agent.pos);
    expect(after.some((pos, index) => Math.hypot(pos[0] - before[index][0], pos[2] - before[index][2]) > 1)).toBe(true);
    const first = marios.debug().agents[0];
    marios.select(first.id);
    expect(marios.selectedDebug()?.id).toBe(first.id);
    expect(marios.cyclePersonality()).toBe('racer');
    marios.disable();
    expect(marios.debug().count).toBe(0);
    marios.dispose();
  });
});
