import { describe, expect, it } from 'vitest';
import { actChoices } from '../src/game/ActSelector';
import { missionsFor, SCORE_COURSES } from '../src/game/courses';

describe('course missions', () => {
  it('exposes all Bob-omb Battlefield mission acts', () => {
    expect(missionsFor('bob').map(mission => mission.act)).toEqual([1, 2, 3, 4, 5, 6]);
    expect(missionsFor('bob')[0].name).toContain('Big Bob-omb');
  });

  it('falls back to a single act for courses without authored missions', () => {
    expect(missionsFor('pss')).toEqual([{ act: 1, name: 'Princess’s Secret Slide' }]);
  });

  it('uses source mission names for other native courses', () => {
    expect(missionsFor('wf').map(m=>m.act)).toEqual([1,2,3,4,5,6]);
    expect(missionsFor('wf')[5].name).toBe('Blast Away the Wall');
    expect(missionsFor('jrb')[0].name).toContain('Sunken Ship');
    expect(missionsFor('rr')[0].name).toContain('Rainbow');
  });
});

describe('act selector progress', () => {
  it('shows the next mission and permits replaying completed stars across gaps', () => {
    expect(actChoices([true,false,false,false,true,false])).toEqual({firstOpen:2,visible:5,selectable:[1,2,5]});
    expect(actChoices(Array(6).fill(false))).toEqual({firstOpen:1,visible:1,selectable:[1]});
    expect(actChoices(Array(6).fill(true))).toEqual({firstOpen:6,visible:6,selectable:[1,2,3,4,5,6]});
    for (const course of SCORE_COURSES) expect(missionsFor(course).map(m=>m.act)).toEqual([1,2,3,4,5,6]);
  });
});
