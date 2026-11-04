export const COURSES = {
  bob: { name: 'Bob-omb Battlefield', note: 'Collect 13 coins, then reach the summit star.' },
  castle_grounds: { name: 'Castle Grounds', note: 'Explore the grounds and reach the star by the castle. Water and doors are scenery.' },
  wf: { name: 'Whomp’s Fortress', note: 'Explore the fortress and reach the upper courtyard star. Platforms are static; bosses are not active.' },
  pss: { name: 'Princess’s Secret Slide', note: 'Slide to the finish star. Coins are optional — press R to try again.' },
  playground: { name: 'Movement Playground', note: 'Collect 13 coins, then climb the star tower.' },
} as const;
export type CourseId = keyof typeof COURSES;
export function courseId(value: string | null): CourseId {
  return value && Object.hasOwn(COURSES, value) ? value as CourseId : 'bob';
}
