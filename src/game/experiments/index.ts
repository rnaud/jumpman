import { KartExperiment } from './kart';
import type { CourseId } from '../courses';
import { DimensionsExperiment } from './dimensions';
import { DioramaExperiment } from './diorama';
import { EndlessRunnerExperiment } from './runner';
import { GodModeExperiment } from './god';
import { HundredMariosExperiment } from './hundred';
import { TinyPlanetExperiment } from './planet';
import { SummitRaceExperiment } from './race';
import { ShrinkRayExperiment } from './shrink';
import type { Experiment, ExperimentId } from './types';

const TITLES: Record<ExperimentId, string> = {
  dimensions: 'Across Dimensions',
  god: 'Mario vs. God',
  runner: 'Endless Run',
  hundred: 'A Hundred Marios',
  diorama: 'Diorama',
  race: 'Summit Race',
  shrink: 'Shrink Ray',
  planet: 'Tiny Planet',
  kart: 'Mario Kart 64 · Luigi Raceway',
};

export function experimentTitle(id: ExperimentId): string {
  return TITLES[id];
}

/** Build a fresh experiment; each run starts from a clean slate. */
export function createExperiment(id: ExperimentId, worldName: CourseId): Experiment {
  switch (id) {
    case 'dimensions':
      // The authored side-scroller opens directly in 2D on its own lane.
      return worldName === 'mushroom2d'
        ? new DimensionsExperiment({ course: null, initial: 'side', laneYaw: 0x4000 })
        : new DimensionsExperiment();
    case 'god': return new GodModeExperiment();
    case 'runner': return new EndlessRunnerExperiment();
    case 'hundred': return new HundredMariosExperiment();
    case 'diorama': return new DioramaExperiment();
    case 'race': return new SummitRaceExperiment();
    case 'shrink': return new ShrinkRayExperiment();
    case 'kart': return new KartExperiment();
    case 'planet': return new TinyPlanetExperiment();
  }
}
