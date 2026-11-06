import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

function wavInfo(path: string) {
  const bytes = readFileSync(path);
  expect(bytes.subarray(0, 4).toString()).toBe('RIFF');
  expect(bytes.subarray(8, 12).toString()).toBe('WAVE');
  const channels = bytes.readUInt16LE(22);
  const rate = bytes.readUInt32LE(24);
  const dataSize = bytes.length - 44;
  return { channels, rate, seconds: dataSize / (channels * 2 * rate) };
}

describe('rendered source music', () => {
  it('contains audible looping course stems', () => {
    for (const track of ['grass', 'inside', 'slide']) {
      const info = wavInfo(`assets/audio/music/${track}.wav`);
      expect(info.channels).toBe(1);
      expect(info.rate).toBeGreaterThanOrEqual(22050);
      expect(info.seconds).toBeGreaterThan(25);
    }
  });
});
