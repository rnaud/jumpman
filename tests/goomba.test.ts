import { Group, Vector3 } from 'three';
import { describe, expect, it } from 'vitest';
import { crossesGoomba, touchesGoombaHurtbox } from '../src/game/goomba';

describe('Goomba stomp', () => {
  const enemy = { model: new Group(), radius: 40, top: 100 };
  it('uses the regular Goomba hurt cylinder only for damaging contact', () => {
    expect(touchesGoombaHurtbox([62,0,0],160,enemy)).toBe(true);
    expect(touchesGoombaHurtbox([63,0,0],160,enemy)).toBe(false);
    expect(touchesGoombaHurtbox([0,61,0],160,enemy)).toBe(false);
    expect(touchesGoombaHurtbox([0,-101,0],100,enemy)).toBe(false);
  });
  it('hits at the crossing point even if Mario ends the tick beyond the edge', () => {
    expect(crossesGoomba(new Vector3(0, 120, 0), [80, 80, 0], enemy)).toBe(true);
  });
  it('ignores jumps from below, side contact and clear misses', () => {
    expect(crossesGoomba(new Vector3(0, 80, 0), [0, 125, 0], enemy)).toBe(false);
    expect(crossesGoomba(new Vector3(0, 90, 0), [0, 75, 0], enemy)).toBe(false);
    expect(crossesGoomba(new Vector3(100, 120, 0), [100, 80, 0], enemy)).toBe(false);
  });
});
