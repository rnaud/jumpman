import { describe, expect, it } from 'vitest';
import { createCrowdMario } from '../src/game/crowdMarioModel';

describe('crowd Mario model', () => {
  it('shares one real 3D Mario mesh across the crowd', () => {
    const first = createCrowdMario();
    const second = createCrowdMario();
    expect(first.geometry).toBe(second.geometry);
    expect(first.material).toBe(second.material);
    expect(first.geometry.getAttribute('color')).toBeDefined();
    expect(first.geometry.getAttribute('position').count).toBeGreaterThan(100);
  });
});
