import { describe, expect, it } from 'vitest';
import voiceTable from '../../../assets/audio/kart-voices.json';
import { VOICES } from './kartAudio';
import { KART_CUES, KartSounds, engineSound, type SoundFrame } from './kartSound';

const base: SoundFrame = {
  hop: false, drifting: false, driftState: 0, miniTurbo: false, bumped: false, midair: false, speedRatio: 0.5, grass: false, accel: true,
  reversing: false, roulette: 0, clock: 1, spinning: false, boosting: false, driving: true,
};

describe('kart sound cues', () => {
  it('every cue has a synthesised voice with sane layers', () => {
    expect(Object.keys(VOICES).sort()).toEqual([...KART_CUES].sort());
    for (const cue of KART_CUES) {
      const v = VOICES[cue];
      expect(v.gain).toBeGreaterThan(0);
      expect(v.layers.length).toBeGreaterThan(0);
      for (const l of v.layers) {
        expect(['sine', 'square', 'saw', 'tri', 'noise']).toContain(l.wave);
        expect(l.dur).toBeGreaterThan(0.02);
        expect(l.dur + (l.t0 ?? 0)).toBeLessThan(2.5);
        expect(l.f0).toBeGreaterThan(0);
        expect(l.f1).toBeGreaterThan(0);
      }
    }
    expect(Object.keys(voiceTable).filter(k => !k.startsWith('_'))).toHaveLength(KART_CUES.length);
  });
  it('the engine climbs with speed, brightens under throttle and is silent on foot', () => {
    const idle = engineSound({ ...base, speedRatio: 0, accel: false });
    const half = engineSound({ ...base, speedRatio: 0.5 });
    const full = engineSound({ ...base, speedRatio: 1 });
    const boost = engineSound({ ...base, speedRatio: 1.15, boosting: true });
    expect(idle.freq).toBeLessThan(half.freq);
    expect(half.freq).toBeLessThan(full.freq);
    expect(full.freq).toBeLessThan(boost.freq);
    expect(idle.gain).toBeGreaterThan(0.1);
    expect(full.gain).toBeGreaterThan(idle.gain);
    expect(engineSound({ ...base, driving: false }).gain).toBe(0);
    expect(engineSound({ ...base, midair: true }).gain).toBeLessThan(half.gain);
  });
  it('skids while drifting or ploughing through grass, not in the air', () => {
    expect(engineSound({ ...base, drifting: true }).skid).toBeGreaterThan(0.3);
    expect(engineSound({ ...base, grass: true }).skid).toBeGreaterThan(0);
    expect(engineSound(base).skid).toBe(0);
    expect(engineSound({ ...base, drifting: true, midair: true }).skid).toBe(0);
  });
  it('fires a cue on the edge of hop, drift, charge, turbo, bump, roulette and item arrival', () => {
    const s = new KartSounds();
    expect(s.update(base)).toEqual([]);
    expect(s.update({ ...base, hop: true })).toEqual(['hop']);
    expect(s.update({ ...base, hop: true })).toEqual([]);
    expect(s.update({ ...base, drifting: true })).toEqual(['driftStart']);
    expect(s.update({ ...base, drifting: true, driftState: 1 })).toEqual(['charge1']);
    expect(s.update({ ...base, drifting: true, driftState: 2 })).toEqual(['charge2']);
    expect(s.update({ ...base, miniTurbo: true })).toEqual(['turbo']);
    expect(s.update({ ...base, miniTurbo: true, bumped: true })).toEqual(['bump']);
    expect(s.update({ ...base, roulette: 30, clock: 3 })).toEqual(['roulette']);
    expect(s.update({ ...base, roulette: 29, clock: 4 })).toEqual([]);
    expect(s.update({ ...base, roulette: 0, clock: 5 })).toEqual(['itemGet']);
  });
  it('maps race events to cues for the player only', () => {
    const s = new KartSounds();
    s.update(base);
    expect(s.update(base, [{ kind: 'count', n: 3 }, { kind: 'go' }])).toEqual(['count', 'go']);
    expect(s.update(base, [
      { kind: 'pickup', i: 0 }, { kind: 'pickup', i: 2 }, { kind: 'use', i: 0, item: 'mushroom' }, { kind: 'use', i: 0, item: 'banana' },
      { kind: 'use', i: 1, item: 'shell' }, { kind: 'use', i: 0, item: 'shell' }, { kind: 'smash', i: 0, item: 'banana' }, { kind: 'smash', i: 0, item: 'shell' },
    ])).toEqual(['box', 'mushroom', 'bananaDrop', 'shellFire', 'bananaHit', 'shellHit']);
    expect(s.update(base, [
      { kind: 'lap', i: 0, n: 1 }, { kind: 'lap', i: 0, n: 2 }, { kind: 'lap', i: 0, n: 3 }, { kind: 'lap', i: 1, n: 1 },
      { kind: 'finish', i: 0 }, { kind: 'rescue', i: 0 },
    ])).toEqual(['lap', 'finalLap', 'finish', 'lakitu']);
  });
});
