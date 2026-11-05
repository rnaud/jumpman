import type { CourseId } from './courses';

const STORAGE_KEY = 'jumpman.save.v1';
export const TOTAL_STARS = 5;

interface SaveData {
  version: 1;
  stars: CourseId[];
  bestCoins: Partial<Record<CourseId, number>>;
}

function emptySave(): SaveData {
  return { version: 1, stars: [], bestCoins: {} };
}

function load(): SaveData {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return emptySave();
    const parsed = JSON.parse(raw) as Partial<SaveData>;
    if (parsed.version !== 1 || !Array.isArray(parsed.stars)) return emptySave();
    const stars = parsed.stars.filter((id): id is CourseId => typeof id === 'string');
    const bestCoins = parsed.bestCoins && typeof parsed.bestCoins === 'object' ? parsed.bestCoins : {};
    return { version: 1, stars, bestCoins };
  } catch {
    return emptySave();
  }
}

/** Persistent star/coin records for the local browser profile. */
export class SaveFile {
  private data = load();

  get stars(): number {
    return this.data.stars.length;
  }

  hasStar(course: CourseId): boolean {
    return this.data.stars.includes(course);
  }

  collectStar(course: CourseId): void {
    if (this.hasStar(course)) return;
    this.data.stars.push(course);
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
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.data));
    } catch {
      // Private browsing or a full storage quota: keep the in-memory record.
    }
  }
}
