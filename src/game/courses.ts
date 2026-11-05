export const COURSES = {
  bob: { name: 'Bob-omb Battlefield', note: 'Collect 13 coins, then reach the summit star.' },
  castle_grounds: { name: 'Castle Grounds', note: 'Explore the grounds and reach the star by the castle. Swim in the moat or enter the castle through the front doors.' },
  castle_inside: { name: 'Castle Lobby', note: 'Explore the lobby. Walk through the front doors to return to the grounds.' },
  wf: { name: 'Whomp’s Fortress', note: 'Explore the fortress and reach the upper courtyard star. Ride the moving platforms to the upper courtyard.' },
  pss: { name: 'Princess’s Secret Slide', note: 'Slide to the finish star. Coins are optional — press R to try again.' },
  playground: { name: 'Movement Playground', note: 'Collect 15 coins across the meadow, then climb the star staircase.' },
} as const;
export type CourseId = keyof typeof COURSES;
export function courseId(value: string | null): CourseId {
  return value && Object.hasOwn(COURSES, value) ? value as CourseId : 'bob';
}
