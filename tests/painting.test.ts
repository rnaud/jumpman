import { describe, expect, it } from 'vitest';
import { isPaintingSurface, paintingDestination } from '../src/game/painting';

describe('painting warps', () => {
  it('maps the BoB and WF painting surface ranges', () => {
    expect(paintingDestination(0x00d3)).toBe('bob');
    expect(paintingDestination(0x00d5)).toBe('bob');
    expect(paintingDestination(0x00d9)).toBe('wf');
    expect(paintingDestination(0x00db)).toBe('wf');
  });

  it('identifies painting surfaces without a ported destination', () => {
    expect(isPaintingSurface(0x00d6)).toBe(true);
    expect(paintingDestination(0x00d6)).toBeNull();
    expect(isPaintingSurface(0x0000)).toBe(false);
  });
});
