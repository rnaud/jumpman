/** Per-character voice sets (data in assets/audio/character-sounds.json, shared with the native build). */
export type VoiceSet = 'mario' | 'link' | 'banjo' | 'silent';
export type VoiceKey =
  | 'jump' | 'backflip' | 'doubleJump' | 'tripleJump' | 'longJump' | 'kick' | 'wallKick' | 'wingPull' | 'flightBurn'
  | 'cannonFire' | 'bowserGrab' | 'bowserThrowFast' | 'bowserThrow' | 'hurt' | 'death' | 'groundPoundDrop' | 'dive'
  | 'deathLanding' | 'mamaMia' | 'star';

export const VOICE_KEYS: readonly VoiceKey[] = [
  'jump', 'backflip', 'doubleJump', 'tripleJump', 'longJump', 'kick', 'wallKick', 'wingPull', 'flightBurn',
  'cannonFire', 'bowserGrab', 'bowserThrowFast', 'bowserThrow', 'hurt', 'death', 'groundPoundDrop', 'dive',
  'deathLanding', 'mamaMia', 'star',
];

/** Unknown characters (custom glTF rigs, ...) get the default (silent) set, never Mario's voice. */
export function resolveVoiceSet(data: { characters: Record<string, string>; default: string }, name: string): VoiceSet {
  return (data.characters[name] ?? data.default) as VoiceSet;
}
