import { describe, expect, it } from 'vitest';
import {
  DEFAULT_CLIP_ALIASES,
  mergeAliases,
  resolveClipName,
  sanitizeAliases,
} from '../src/game/clipMap';

describe('glTF clip mapping', () => {
  it('uses the default alias when the rig has the clip', () => {
    const clips = ['Idle_Loop', 'Walk_Loop', 'Roll'];
    expect(resolveClipName(clips, DEFAULT_CLIP_ALIASES, 'walk')).toBe('Walk_Loop');
  });

  it('honours a custom clip map', () => {
    const aliases = mergeAliases({ idle: 'MyIdle', roll: 'MyFlip' });
    const clips = ['MyIdle', 'MyFlip'];
    expect(resolveClipName(clips, aliases, 'idle')).toBe('MyIdle');
    expect(resolveClipName(clips, aliases, 'roll')).toBe('MyFlip');
  });

  it('falls back semantically when a clip is missing', () => {
    const clips = ['Idle_Loop', 'Jog_Fwd_Loop'];
    expect(resolveClipName(clips, DEFAULT_CLIP_ALIASES, 'sprint')).toBe('Jog_Fwd_Loop');
    expect(resolveClipName(clips, DEFAULT_CLIP_ALIASES, 'roll')).toBe('Idle_Loop');
  });

  it('ignores malformed custom entries and returns null for an empty rig', () => {
    expect(sanitizeAliases({ idle: 42, walk: '' })).toEqual({});
    expect(resolveClipName([], DEFAULT_CLIP_ALIASES, 'idle')).toBeNull();
  });
});
