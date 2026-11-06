import type { CourseId } from './courses';

// get_painting_warp_node indexes the current area's table by floor type.
export function paintingDestination(type: number, height: number, nodes?: Partial<Record<number, CourseId>>): CourseId | null {
  const index = type - 0xd3;
  if (index < 0 || index >= 0x2d || (index >= 0x2a && height >= 80)) return null;
  return nodes?.[index] ?? null;
}
