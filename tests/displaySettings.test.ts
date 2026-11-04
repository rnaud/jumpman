import { describe, expect, it } from 'vitest';
import { CLASSIC, MODERN, displaySize } from '../src/game/displaySettings';
describe('display sizing', () => {
  it('renders 4:3 at a fixed internal resolution on retina screens', () => {
    expect(displaySize(CLASSIC, 1920, 1080, 2)).toEqual({ cssWidth: 1440, cssHeight: 1080, width: 640, height: 480, aspect: 4 / 3 });
    expect(displaySize({ ...CLASSIC, resolution: '240' }, 390, 844, 3).width).toBe(320);
    expect(displaySize(CLASSIC, 390, 844, 3).cssHeight).toBe(292.5);
  });
  it('keeps widescreen unstretched and caps native retina rendering at 2x', () => {
    expect(displaySize({ ...CLASSIC, aspect: 'wide', resolution: '240' }, 1600, 900, 1).width).toBe(427);
    expect(displaySize(MODERN, 1600, 900, 3).width).toBe(3200);
  });
});
