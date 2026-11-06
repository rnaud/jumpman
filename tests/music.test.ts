import { readFileSync, readdirSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { courseMusic } from '../src/game/music';

const bytes = readFileSync('assets/audio/music/sm64-us.wasm');
const compiled = WebAssembly.compile(bytes);
interface Engine {
  memory: WebAssembly.Memory;
  _initialize(): void;
  music_init(): void;
  music_location(level: number, area: number, room: number, x: number, y: number, z: number, stars: number): void;
  music_start(id: number, preset: number): void;
  music_samples(): number;
  music_render(): number;
}

describe('US soundtrack', () => {
  it('bundles every extracted US sequence byte-for-byte', () => {
    const dir = 'assets/extracted/sound/sequences/us';
    const tracks = readdirSync(dir).filter(file => file.endsWith('.m64'));
    expect(tracks).toHaveLength(34);
    for (const name of tracks) expect(bytes.includes(readFileSync(dir+'/'+name)), name).toBe(true);
  });

  // Independent 8-second PCM captures from the native 64-bit sm64-port mixer.
  // Scalar WebAssembly and native vector operations differ slightly in rounding.
  it.each([
    [3,9,0,[2421,2439,2406,2047,2365,1589,2333,1765]],
    [4,6,1,[1791,2445,2221,1426,1989,2526,1494,1813]],
    [5,12,3,[2067,1820,3050,2083,2604,2814,2473,2659]],
    [9,27,1,[1985,2556,2829,2848,3263,3559,3780,3419]],
    [25,34,2,[2983,2829,3729,3978,4909,3836,1400,1233]],
    [32,16,7,[3906,4203,4035,4558,4811,5209,5250,5989]],
  ] as const)('matches native playback levels for sequence %i', async (id, level, preset, reference) => {
    const e = (await WebAssembly.instantiate(await compiled)).exports as unknown as Engine;
    e._initialize(); e.music_init(); e.music_location(level,1,0,0,0,0,0); e.music_start(id,preset);
    for (const expected of reference) {
      let power = 0, frames = 0;
      for (let i=0; i<60; i++) {
        const n = e.music_render();
        for (const value of new Int16Array(e.memory.buffer,e.music_samples(),n*2)) power += value*value;
        frames += n;
      }
      expect(frames).toBe(32000);
      expect(Math.abs(Math.sqrt(power/(2*frames)) - expected)).toBeLessThan(5);
    }
  });
  it('selects outdoor ambience, course-specific variations and second-area overrides', () => {
    expect(courseMusic('castle_grounds').sequence).toBe(0);
    expect(courseMusic('sa').sequence).toBe(0x85);
    expect(courseMusic('ccm',2).sequence).toBe(9);
    expect(courseMusic('bowser_3').sequence).toBe(25);
  });
});

describe('US sound effects', () => {
  it('plays all eight red-coin notes with the music player silent', async () => {
    const ids = JSON.parse(readFileSync('assets/audio/music/sounds.json', 'utf8'));
    const signatures = new Set<string>();
    for (let offset=0; offset<8; offset++) {
      const e = (await WebAssembly.instantiate(await compiled)).exports as unknown as Engine & {
        sound_effect(id: number, variations: number, source: number, x: number, y: number, z: number): void;
      };
      e._initialize(); e.music_init(); e.music_location(9,1,0,0,0,0,0); e.music_start(0,0);
      for (let i=0; i<8; i++) e.music_render();
      e.sound_effect(ids.MENU_COLLECT_RED_COIN + (offset << 16), 1, 1, 0, 0, 0);
      let peak = 0;
      const energy: number[] = [];
      for (let i=0; i<36; i++) {
        const frames = e.music_render();
        let power = 0;
        for (const v of new Int16Array(e.memory.buffer,e.music_samples(),frames*2)) {
          peak = Math.max(peak,Math.abs(v));
          power += v*v;
        }
        energy.push(power);
      }
      expect(peak).toBeGreaterThan(1000);
      signatures.add(energy.join(','));
    }
    expect(signatures.size).toBe(8);
  });
});
