import { EXPERIMENT_IDS, type ExperimentId } from '../game/experiments/types';

export interface ModeCard {
  readonly id: ExperimentId;
  readonly title: string;
  readonly blurb: string;
  /** Fallback tint (hue, degrees) behind the thumbnail while the image loads or is missing. */
  readonly hue: number;
}

/** One card per web-available experiment, in menu order. */
export const MODE_CARDS: readonly ModeCard[] = [
  { id: 'dimensions', title: 'Across Dimensions', blurb: 'Flatten Bob-omb Battlefield into a 2D cross-section, or a top-down map.', hue: 205 },
  { id: 'god', title: 'Mario vs. God', blurb: 'Drag his platforms away mid-jump. Paint a bridge under him.', hue: 275 },
  { id: 'runner', title: 'Endless Run', blurb: 'Mario never stops running. Jump, steer, survive as far as you can.', hue: 20 },
  { id: 'hundred', title: 'A Hundred Marios', blurb: 'Racers, coin hunters, followers, brawlers and explorers.', hue: 350 },
  { id: 'diorama', title: 'Diorama', blurb: 'The whole course in a plastic compact. Fly around, poke the cast.', hue: 150 },
  { id: 'race', title: 'Summit Race', blurb: 'You against seven rivals to the top of Bob-omb Battlefield.', hue: 40 },
  { id: 'shrink', title: 'Shrink Ray', blurb: 'Same Mario, any size: grass becomes a jungle, the mountain a step.', hue: 105 },
  { id: 'planet', title: 'Tiny Planet', blurb: 'Run all the way around a world fifteen seconds wide.', hue: 230 },
  { id: 'kart', title: 'Mario Kart 64', blurb: 'Drive Luigi Raceway, park your kart, and explore on foot as Mario.', hue: 0 },
];

export const MODES_ART_DIR = '/assets/ui/modes';
export const thumbnailFor = (id: ExperimentId | 'levels'): string => `${MODES_ART_DIR}/${id}.jpg`;

export function modesAreComplete(): boolean {
  return EXPERIMENT_IDS.every(id => MODE_CARDS.some(card => card.id === id)) && MODE_CARDS.length === EXPERIMENT_IDS.length;
}
