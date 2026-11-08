import { describe, expect, it } from 'vitest';
import data from '../assets/audio/character-sounds.json';
import sounds from '../assets/audio/music/sounds.json';
import { VOICE_KEYS, resolveVoiceSet } from '../src/game/characterSounds';

const ids = ['sm64', 'procedural', 'link', 'young-link', 'banjo', 'cc0', 'local', 'unknown-rig'];
type Actions = Record<string, [string, number][]>;
const sets = data.sets as unknown as Record<string, Actions>;

describe('per-character sound sets', () => {
  it('resolves every character x action to real sound names or an explicit none', () => {
    for (const id of ids) {
      const set = resolveVoiceSet(data, id);
      const actions = sets[set];
      expect(actions, `${id} -> ${set}`).toBeTruthy();
      for (const key of VOICE_KEYS) {
        const layers = actions[key];
        expect(Array.isArray(layers), `${id}/${key}`).toBe(true);
        for (const [sound, variants] of layers) {
          expect(sounds, `${id}/${key}: ${sound}`).toHaveProperty(sound);
          expect(variants).toBeGreaterThan(0);
        }
      }
    }
  });

  it('never plays a Mario voice clip for a non-Mario character', () => {
    for (const id of ids.filter(i => resolveVoiceSet(data, i) !== 'mario')) {
      const actions = sets[resolveVoiceSet(data, id)];
      for (const key of VOICE_KEYS) for (const [sound] of actions[key]) expect(sound, `${id}/${key}`).not.toMatch(/^MARIO_/);
    }
    expect(resolveVoiceSet(data, 'sm64')).toBe('mario');
    expect(resolveVoiceSet(data, 'unknown-rig')).toBe('silent');
  });

  it('keeps Mario on his original SM64 clips', () => {
    expect(sets.mario.jump).toEqual([['MARIO_YAH_WAH_HOO', 3]]);
    expect(sets.mario.tripleJump).toEqual([['MARIO_YAHOO_WAHA_YIPPEE', 5]]);
    expect(sets.mario.death).toEqual([['MARIO_DYING', 1]]);
  });
});
