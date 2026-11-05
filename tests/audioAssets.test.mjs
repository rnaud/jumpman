import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const hasExportedAudio = existsSync(new URL('../assets/audio/manifest.json', import.meta.url));
const hasReferenceSamples = existsSync(new URL('../reference/sm64/sound/samples', import.meta.url));

describe.skipIf(!hasExportedAudio || !hasReferenceSamples)('exported ROM audio', () => {
  it('preserves every extracted sample frame and rate in browser-playable PCM', () => {
    const manifest = JSON.parse(readFileSync(new URL('../assets/audio/manifest.json', import.meta.url)));
    expect(Object.keys(manifest)).toHaveLength(219);
    for (const [path, info] of Object.entries(manifest)) {
      const wav = readFileSync(new URL(`../assets/audio/${path}`, import.meta.url));
      expect(wav.toString('ascii', 0, 4)).toBe('RIFF');
      expect(wav.toString('ascii', 8, 12)).toBe('WAVE');
      expect(wav.readUInt16LE(20)).toBe(1);
      expect(wav.readUInt16LE(22)).toBe(1);
      expect(wav.readUInt32LE(24)).toBe(info.sampleRate);
      expect(wav.readUInt32LE(40)).toBe(info.frames * 2);
      expect(wav.length).toBe(info.frames * 2 + 44);
      const original = path.replaceAll('sharp', '%23').replace(/\.wav$/, '.aiff');
      const aiff = readFileSync(new URL(`../reference/sm64/sound/samples/${original}`, import.meta.url));
      let sound;
      for (let offset = 12; offset + 8 <= aiff.length;) {
        const size = aiff.readUInt32BE(offset + 4);
        if (aiff.toString('ascii', offset, offset + 4) === 'SSND') {
          const start = offset + 16 + aiff.readUInt32BE(offset + 8);
          sound = Buffer.from(aiff.subarray(start, start + info.frames * 2)).swap16();
          break;
        }
        offset += 8 + size + (size & 1);
      }
      expect(sound?.equals(wav.subarray(44)), path).toBe(true);
      if(info.loop) {
        expect(info.loop[0]).toBeGreaterThanOrEqual(0);
        expect(info.loop[1]).toBeGreaterThan(info.loop[0]);
        expect(info.loop[1]).toBeLessThanOrEqual(info.duration);
      }
    }
  });
});
