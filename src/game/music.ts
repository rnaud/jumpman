import type { CourseId } from './courses';
import sequenceEngine from '../../assets/audio/music/sm64-us.wasm?url';
import workletUrl from './music-worklet.js?url';
import courseAudio from '../../assets/audio/music/courses.json';

export interface MusicTrack { name: string; sequence: number; preset: number; level: number }
const levels: Record<string, {level: number; areas: {name: string; sequence: number; preset: number}[]}> = courseAudio;

export function courseMusic(course: CourseId, area = 1): MusicTrack {
  const source = levels[course] ?? levels.bob;
  return {level: source.level, ...(source.areas[area-1] ?? source.areas[0])};
}

export async function createMusicNode(context: AudioContext): Promise<AudioWorkletNode> {
  const response = await fetch(sequenceEngine);
  if (!response.ok) throw new Error(`Music engine request failed (${response.status})`);
  const [module] = await Promise.all([
    WebAssembly.compile(await response.arrayBuffer()), context.audioWorklet.addModule(workletUrl),
  ]);
  return new AudioWorkletNode(context, 'sm64-music', {
    numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2], processorOptions: {module},
  });
}
