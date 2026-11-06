import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { paintingDestination } from '../src/game/painting';

const lobby = JSON.parse(readFileSync('assets/courses/castle_inside.json', 'utf8')).paintings;
describe('castle lobby painting warps', () => {
  it.each([[0xd3, 'bob'], [0xd6, 'ccm'], [0xd9, 'wf'], [0xdc, 'jrb']] as const)(
    'resolves all three floor strips starting at %i to %s', (start, course) => {
      for (let type = start; type < start + 3; type++) expect(paintingDestination(type, 0, lobby)).toBe(course);
    },
  );

  it('ignores ordinary floors and warp nodes belonging to other areas', () => {
    for (const type of [0, 0xd2, 0xdf, 0xfd]) expect(paintingDestination(type, 0, lobby)).toBeNull();
  });
});
