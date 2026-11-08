/**
 * What the MK64 experiment should sound like, engine-free: turns kart state and race events into sound cues and
 * continuous engine/skid parameters. Browser twin of native/src/experiments/kart_sound.rs; the synthesis itself is
 * kartAudio.ts (web) and kart_audio.rs (native), both reading assets/audio/kart-voices.json.
 */
import type { RaceEvent } from './kartRace';
import { LAPS } from './kartRace';

export type KartCue =
  | 'hop' | 'driftStart' | 'charge1' | 'charge2' | 'turbo' | 'bump' | 'box' | 'roulette' | 'itemGet' | 'mushroom'
  | 'bananaDrop' | 'bananaHit' | 'shellFire' | 'shellHit' | 'count' | 'go' | 'lap' | 'finalLap' | 'finish' | 'lakitu';
export const KART_CUES: readonly KartCue[] = [
  'hop', 'driftStart', 'charge1', 'charge2', 'turbo', 'bump', 'box', 'roulette', 'itemGet', 'mushroom',
  'bananaDrop', 'bananaHit', 'shellFire', 'shellHit', 'count', 'go', 'lap', 'finalLap', 'finish', 'lakitu',
];

/** The slice of kart and racer state the sounds follow (sampled once per 30 Hz tick). */
export interface SoundFrame {
  hop: boolean;
  drifting: boolean;
  driftState: number;
  miniTurbo: boolean;
  bumped: boolean;
  midair: boolean;
  /** currentSpeed / top speed, may exceed 1 while boosting. */
  speedRatio: number;
  grass: boolean;
  accel: boolean;
  reversing: boolean;
  /** Roulette ticks left (0 = not spinning) and the sim clock for the tick rhythm. */
  roulette: number;
  clock: number;
  spinning: boolean;
  boosting: boolean;
  driving: boolean;
}

export interface Engine {
  /** Fundamental in Hz. */
  freq: number;
  gain: number;
  /** Tyre skid noise level. */
  skid: number;
}

/**
 * Engine note: idles low and climbs with speed, with a slight lift while the throttle is down, like the MK64 engine
 * sample being resampled by speed. Silent when parked on foot.
 */
export function engineSound(f: SoundFrame): Engine {
  if (!f.driving) return { freq: 50, gain: 0, skid: 0 };
  const r = Math.max(0, Math.min(1.3, f.speedRatio));
  const freq = 46 + 128 * Math.pow(r, 0.85) + (f.accel ? 7 : 0) + (f.boosting ? 22 : 0) + (f.reversing ? 10 : 0);
  const load = f.accel ? 0.2 : 0;
  const gain = (0.2 + 0.34 * Math.min(1, r) + load) * (f.midair ? 0.55 : 1) * (f.spinning ? 0.5 : 1);
  const slide = f.drifting && !f.hop ? 0.55 : 0;
  const rough = f.grass && !f.midair ? 0.35 * Math.min(1, r) : 0;
  return { freq, gain, skid: Math.max(slide, rough) * (f.midair ? 0 : 1) };
}

/** Edge detector: previous frame in, cues out. */
export class KartSounds {
  private prev: SoundFrame | null = null;

  reset(): void { this.prev = null; }

  update(f: SoundFrame, events: readonly RaceEvent[] = []): KartCue[] {
    const cues: KartCue[] = [];
    const p = this.prev;
    this.prev = f;
    if (p && f.driving && p.driving) {
      if (f.hop && !p.hop) cues.push('hop');
      if (f.drifting && !p.drifting && !f.hop) cues.push('driftStart');
      if (f.drifting && f.driftState > p.driftState) cues.push(f.driftState >= 2 ? 'charge2' : 'charge1');
      if (f.miniTurbo && !p.miniTurbo && !f.boosting) cues.push('turbo');
      if (f.bumped && !p.bumped && f.speedRatio > 0.1) cues.push('bump');
      if (f.roulette > 0 && f.clock % 3 === 0) cues.push('roulette');
      if (p.roulette > 0 && f.roulette === 0) cues.push('itemGet');
    }
    for (const e of events) {
      switch (e.kind) {
        case 'count': cues.push('count'); break;
        case 'go': cues.push('go'); break;
        case 'pickup': if (e.i === 0) cues.push('box'); break;
        case 'use':
          if (e.i === 0) cues.push(e.item === 'mushroom' ? 'mushroom' : e.item === 'banana' ? 'bananaDrop' : 'shellFire');
          break;
        case 'smash': if (e.i === 0) cues.push(e.item === 'banana' ? 'bananaHit' : 'shellHit'); break;
        case 'lap': if (e.i === 0 && e.n < LAPS) cues.push(e.n === LAPS - 1 ? 'finalLap' : 'lap'); break;
        case 'finish': if (e.i === 0) cues.push('finish'); break;
        case 'rescue': if (e.i === 0) cues.push('lakitu'); break;
        default: break;
      }
    }
    return cues;
  }
}
