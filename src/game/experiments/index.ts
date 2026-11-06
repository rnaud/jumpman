import type { CourseId } from '../courses';
import { DimensionsExperiment } from './dimensions';
import { DioramaExperiment } from './diorama';
import { EndlessRunnerExperiment } from './runner';
import { GodModeExperiment } from './god';
import { HundredMariosExperiment } from './hundred';
import type { Experiment, ExperimentId } from './types';

const TITLES: Record<ExperimentId, string> = {
  dimensions: 'Across Dimensions',
  god: 'Mario vs. God',
  runner: 'Endless Run',
  hundred: 'A Hundred Marios',
  diorama: 'Diorama',
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
  }
}
