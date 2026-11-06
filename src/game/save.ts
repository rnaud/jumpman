import type { CourseId } from './courses';

const LEGACY_KEY = 'jumpman.save.v1';
const SLOT_KEY = 'jumpman.save.slot';
export const TOTAL_STARS = 15;

function starKey(course: CourseId, act = 1): string {
  return course === 'bob' && act > 1 ? `${course}:${act}` : course;
}

interface SaveData {
  version: 1;
  stars: string[];
  bestCoins: Partial<Record<CourseId, number>>;
}

function emptySave(): SaveData {
  return { version: 1, stars: [], bestCoins: {} };
}

function storageKey(slot: number): string {
  return `${LEGACY_KEY}.slot${slot}`;
}

function parse(raw: string | null): SaveData {
  try {
    if (!raw) return emptySave();
    const parsed = JSON.parse(raw) as Partial<SaveData>;
    if (parsed.version !== 1 || !Array.isArray(parsed.stars)) return emptySave();
    const stars = parsed.stars.filter((id): id is string => typeof id === 'string');
    const bestCoins = parsed.bestCoins && typeof parsed.bestCoins === 'object' ? parsed.bestCoins : {};
    return { version: 1, stars, bestCoins };
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
    return this.data.stars.length;
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
