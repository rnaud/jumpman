import type { CourseId } from './courses';

/** Painting warp surface ranges from include/surface_terrains.h. */
export function paintingDestination(type: number): CourseId | null {
  if (type >= 0x00d3 && type <= 0x00d5) return 'bob';
  if (type >= 0x00d9 && type <= 0x00db) return 'wf';
  return null;
}

export function isPaintingSurface(type: number): boolean {
  return type >= 0x00d3 && type <= 0x00fe;
}
