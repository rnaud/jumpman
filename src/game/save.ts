import type { CourseId } from './courses';

const LEGACY_KEY = 'jumpman.save.v1';
const SLOT_KEY = 'jumpman.save.slot';
export const TOTAL_STARS = 120;

function starKey(course: CourseId, act = 1): string {
  return act > 1 ? `${course}:${act}` : course;
}

export type CastleKey = 1 | 2;
const heldFlag = (key: CastleKey) => key === 1 ? 0x10 : 0x20;
const unlockedFlag = (key: CastleKey) => key === 1 ? 0x40 : 0x80;

interface SaveData {
  flags: number;
  version: 1 | 2;
  stars: string[];
  bestCoins: Partial<Record<CourseId, number>>;
}

function emptySave(): SaveData {
  return { version: 2, flags: 0, stars: [], bestCoins: {} };
}

function storageKey(slot: number): string {
  return `${LEGACY_KEY}.slot${slot}`;
}

function parse(raw: string | null): SaveData {
  try {
    if (!raw) return emptySave();
    const parsed = JSON.parse(raw) as Partial<SaveData>;
    if ((parsed.version !== 1 && parsed.version !== 2) || !Array.isArray(parsed.stars)) return emptySave();
    let flags = Number.isInteger(parsed.flags) ? Number(parsed.flags) : 0;
    const legacyStars = parsed.stars.filter((id): id is string => typeof id === 'string');
    // Older builds recorded the first two arena rewards as stars.
    const stars = legacyStars.filter(star => {
      if (star !== 'bowser_1' && star !== 'bowser_2') return true;
      const key = star === 'bowser_1' ? 1 : 2;
      if (!(flags & unlockedFlag(key))) flags |= heldFlag(key);
      return false;
    });
    const bestCoins = parsed.bestCoins && typeof parsed.bestCoins === 'object' ? parsed.bestCoins : {};
    const migrated = stars.map(id => parsed.version === 1 && id === 'jrb' ? 'jrb:3' : id);
    return { version: 2, flags, stars: [...new Set(migrated)], bestCoins };
  } catch {
    return emptySave();
  }
}

export function activeSaveSlot(): number {
  try {
    const slot = Number(localStorage.getItem(SLOT_KEY) ?? 1);
    return Number.isInteger(slot) && slot >= 1 && slot <= 4 ? slot : 1;
  } catch {
    return 1;
  }
}

/** Persistent star/coin records for one of the four local save slots. */
export class SaveFile {
  readonly slot: number;
  private data: SaveData;

  constructor(slot = 1) {
    this.slot = slot;
    let raw: string | null = null;
    try {
      raw = localStorage.getItem(storageKey(slot));
      if (!raw && slot === 1) raw = localStorage.getItem(LEGACY_KEY);
    } catch {
      // Storage unavailable: use the in-memory default.
    }
    this.data = parse(raw);
  }

  static select(slot: number): void {
    try {
      localStorage.setItem(SLOT_KEY, String(slot));
    } catch {
      // Ignore storage failures; the current session still uses the slot.
    }
  }

  get stars(): number {
    // Bonus playgrounds and old prototype hub/arena rewards do not count at castle doors.
    const excluded = ['playground', 'mushroom2d', 'castle_grounds', 'castle_inside', 'bowser_3'];
    return this.data.stars.filter(id => !excluded.includes(id.split(':')[0])).length;
  }

  hasStar(course: CourseId, act = 1): boolean {
    return this.data.stars.includes(starKey(course, act));
  }

  collectStar(course: CourseId, act = 1): void {
    const key = starKey(course, act);
    if (this.data.stars.includes(key)) return;
    this.data.stars.push(key);
    this.persist();
  }

  hasFlag(flag: number): boolean {
    return Boolean(this.data.flags & flag);
  }

  setFlag(flag: number): void {
    if (!flag || this.hasFlag(flag)) return;
    this.data.flags |= flag;
    this.persist();
  }

  collectKey(key: CastleKey): void {
    if (this.data.flags & (heldFlag(key) | unlockedFlag(key))) return;
    this.data.flags |= heldFlag(key);
    this.persist();
  }

  hasKey(key: CastleKey): boolean {
    return Boolean(this.data.flags & heldFlag(key));
  }

  doorUnlocked(key: CastleKey): boolean {
    return Boolean(this.data.flags & unlockedFlag(key));
  }

  unlockDoor(key: CastleKey): boolean {
    if (this.data.flags & unlockedFlag(key)) return true;
    if (!(this.data.flags & heldFlag(key))) return false;
    this.data.flags = (this.data.flags & ~heldFlag(key)) | unlockedFlag(key);
    this.persist();
    return true;
  }

  bestCoins(course: CourseId): number {
    return this.data.bestCoins[course] ?? 0;
  }

  recordCoins(course: CourseId, coins: number): void {
    if (coins <= this.bestCoins(course)) return;
    this.data.bestCoins[course] = coins;
    this.persist();
  }

  reset(): void {
    this.data = emptySave();
    this.persist();
  }

  private persist(): void {
    try {
      localStorage.setItem(storageKey(this.slot), JSON.stringify(this.data));
    } catch {
      // Private browsing or a full storage quota: keep the in-memory record.
    }
  }
}
