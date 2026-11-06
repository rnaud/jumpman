import { describe, expect, it } from 'vitest';
import { readableSign, type Signpost } from '../src/game/signpost';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { ACT_IDLE, ACT_WALKING } from '../src/sim/state';
import { addGround } from './helpers';

describe('wooden sign interaction', () => {
  it('requires a readable action, overlap and both facing angles', () => {
    const world = new CollisionWorld();
    addGround(world);
    const m = new MarioRuntime(world,{pos:[0,0,180],faceYaw:-0x8000});
    const sign: Signpost = {pos:[0,0,0], yaw:0, pages:['Test']};
    const read = (animation?: string) => readableSign([sign],m,animation);
    m.setAction(ACT_IDLE);
    expect(read()).toBe(sign);
    m.pos[2] = 188;
    expect(read()).toBeUndefined();
    m.pos[2] = -100;
    expect(read()).toBeUndefined();
    m.pos = [0,0,100];
    m.faceAngle[1] = 0;
    expect(read()).toBeUndefined();
    m.faceAngle[1] = -0x8000;
    m.setAction(ACT_WALKING);
    expect(read('anim_48')).toBeUndefined();
    expect(read('anim_6C')).toBe(sign);
  });
});
