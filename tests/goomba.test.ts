import { Group, Vector3 } from 'three';
import { describe, expect, it } from 'vitest';
import { crossesGoomba } from '../src/game/goomba';

describe('Goomba stomp', () => {
  const enemy = { model: new Group(), radius: 40, top: 100 };
  it('hits at the crossing point even if Mario ends the tick beyond the edge', () => {
    expect(crossesGoomba(new Vector3(0, 120, 0), [80, 80, 0], enemy)).toBe(true);
  });
  it('ignores jumps from below, side contact and clear misses', () => {
    expect(crossesGoomba(new Vector3(0, 80, 0), [0, 125, 0], enemy)).toBe(false);
    expect(crossesGoomba(new Vector3(0, 90, 0), [0, 75, 0], enemy)).toBe(false);
    expect(crossesGoomba(new Vector3(100, 120, 0), [100, 80, 0], enemy)).toBe(false);
  });
});
