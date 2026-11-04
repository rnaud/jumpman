/**
 * Action → animation-clip aliases for glTF characters.
 *
 * The bundled CC0 rig uses these names directly.  A custom rig can supply a
 * JSON file with the same keys and its own clip names; missing clips fall back
 * to sensible alternatives so a partial rig still plays.
 */

export const DEFAULT_CLIP_ALIASES = {
  idle: 'Idle_Loop',
  walk: 'Walk_Loop',
  jog: 'Jog_Fwd_Loop',
  sprint: 'Sprint_Loop',
  jumpStart: 'Jump_Start',
  jumpLoop: 'Jump_Loop',
  jumpLand: 'Jump_Land',
  roll: 'Roll',
  rollRM: 'Roll_RM',
  crouchIdle: 'Crouch_Idle_Loop',
  crouchWalk: 'Crouch_Fwd_Loop',
  punchJab: 'Punch_Jab',
  punchCross: 'Punch_Cross',
  interact: 'Interact',
} as const;

export type ClipAliasKey = keyof typeof DEFAULT_CLIP_ALIASES;
export type ClipAliases = Record<ClipAliasKey, string>;

const FALLBACK_KEYS: Partial<Record<ClipAliasKey, ClipAliasKey[]>> = {
  walk: ['jog', 'idle'],
  jog: ['walk', 'sprint', 'idle'],
  sprint: ['jog', 'walk', 'idle'],
  jumpStart: ['jumpLoop', 'idle'],
  jumpLoop: ['jumpStart', 'idle'],
  jumpLand: ['idle', 'jumpLoop'],
  roll: ['rollRM', 'jumpLoop', 'idle'],
  rollRM: ['roll', 'jumpLoop', 'idle'],
  crouchIdle: ['crouchWalk', 'idle'],
  crouchWalk: ['crouchIdle', 'walk', 'idle'],
  punchJab: ['punchCross', 'idle'],
  punchCross: ['punchJab', 'idle'],
  interact: ['idle', 'jumpLoop'],
};

/** Merge a user-supplied JSON object with the built-in aliases. */
export function sanitizeAliases(input: unknown): Partial<ClipAliases> {
  if (!input || typeof input !== 'object') return {};
  const result: Partial<ClipAliases> = {};
  for (const key of Object.keys(DEFAULT_CLIP_ALIASES) as ClipAliasKey[]) {
    const value = (input as Record<string, unknown>)[key];
    if (typeof value === 'string' && value.length > 0) {
      result[key] = value;
    }
  }
  return result;
}

export function mergeAliases(input: unknown): ClipAliases {
  return { ...DEFAULT_CLIP_ALIASES, ...sanitizeAliases(input) };
}

/**
 * Resolve the best available clip for an alias.  Order:
 * custom alias → default alias → semantic fallbacks → first available clip.
 */
export function resolveClipName(
  clipNames: readonly string[],
  aliases: ClipAliases,
  key: ClipAliasKey,
): string | null {
  const available = new Set(clipNames);
  const candidates: string[] = [aliases[key], DEFAULT_CLIP_ALIASES[key]];

  for (const fallbackKey of FALLBACK_KEYS[key] ?? ['idle' as ClipAliasKey]) {
    candidates.push(aliases[fallbackKey], DEFAULT_CLIP_ALIASES[fallbackKey]);
  }

  candidates.push(...clipNames);

  for (const candidate of candidates) {
    if (candidate && available.has(candidate)) return candidate;
  }
  return null;
}
