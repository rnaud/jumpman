export const COURSES = {
  bob: { name: 'Bob-omb Battlefield', note: 'Collect 13 coins, then reach the summit star.' },
  castle_grounds: { name: 'Castle Grounds', note: 'Explore the grounds and reach the star by the castle. Swim in the moat or enter the castle through the front doors.' },
  castle_inside: { name: 'Castle Lobby', note: 'Explore the lobby. Walk through the front doors to return to the grounds.' },
  wf: { name: 'Whomp’s Fortress', note: 'Explore the fortress and reach the upper courtyard star. Ride the moving platforms to the upper courtyard.' },
  bbh: { name: 'Big Boo’s Haunt', note: 'Explore the haunted mansion terrain. The star is on the upper floor; most mansion objects are not yet ported.' },
  ccm: { name: 'Cool, Cool Mountain', note: 'Explore the snowy mountain terrain. The star is on the upper slope; penguins and most snow objects are not yet ported.' },
  jrb: { name: 'Jolly Roger Bay', note: 'Explore the bay terrain and water. The star is on the upper rock; most underwater objects and treasure chests are not yet ported.' },
  lll: { name: 'Lethal Lava Land', note: 'Explore the volcanic terrain and lava. The star is on the upper platform; most fire objects and the bully are not yet ported.' },
  ssl: { name: 'Shifting Sand Land', note: 'Explore the desert terrain. The star is on a high ledge; most quicksand features and the pyramid objects are not yet ported.' },
  wdw: { name: 'Wet-Dry World', note: 'Explore the flooded town terrain. The star is on the high ledge; the water-level puzzle and most town objects are not yet ported.' },
  sl: { name: 'Snowman’s Land', note: 'Explore the snowy mountain terrain with Mr. Blizzard. The star is on a high ledge; most snow objects are not yet ported.' },
  ttm: { name: 'Tall, Tall Mountain', note: 'Explore the mountain terrain. The star is on the summit; most mountain objects and the slide are not yet ported.' },
  pss: { name: 'Princess’s Secret Slide', note: 'Slide to the finish star. Coins are optional — press R to try again.' },
  playground: { name: 'Movement Playground', note: 'Collect 15 coins across the meadow, then climb the star staircase.' },
} as const;
export type CourseId = keyof typeof COURSES;
export interface CourseMission { act: number; name: string }
export const COURSE_MISSIONS: Partial<Record<CourseId, CourseMission[]>> = {
  bob: [
    { act: 1, name: 'Big Bob-omb on the Summit' },
    { act: 2, name: 'Footrace with Koopa the Quick' },
    { act: 3, name: 'Behind Chain Chomp’s Gate' },
    { act: 4, name: 'Find the 8 Red Coins' },
  ],
};
export function missionsFor(id: CourseId): CourseMission[] {
  return COURSE_MISSIONS[id] ?? [{ act: 1, name: COURSES[id].name }];
}
export function courseId(value: string | null): CourseId {
  return value && Object.hasOwn(COURSES, value) ? value as CourseId : 'bob';
}
