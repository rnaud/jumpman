import grass from '../assets/audio/music/grass.json';
import inside from '../assets/audio/music/inside.json';
import slide from '../assets/audio/music/slide.json';
import { describe, expect, it } from 'vitest';

describe('source-derived music tracks', () => {
  it('contains bounded note loops', () => {
    for (const track of [grass, inside, slide]) {
      expect(track.events.length).toBeGreaterThan(0);
      expect(track.events.length).toBeLessThanOrEqual(64);
      expect(track.events.every(event => event.note >= 0 && event.note <= 127 && event.duration > 0)).toBe(true);
      expect(track.loopTicks).toBeLessThanOrEqual(2000);
    }
  });
});
