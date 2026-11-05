import { describe, expect, it } from 'vitest';
import { missionsFor } from '../src/game/courses';

describe('course missions', () => {
  it('exposes both Bob-omb Battlefield mission acts', () => {
    expect(missionsFor('bob').map(mission => mission.act)).toEqual([1, 2]);
    expect(missionsFor('bob')[0].name).toContain('Big Bob-omb');
  });

  it('falls back to a single act for courses without authored missions', () => {
    expect(missionsFor('pss')).toEqual([{ act: 1, name: 'Princess’s Secret Slide' }]);
  });
});
