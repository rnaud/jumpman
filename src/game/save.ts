import { treasureRewards } from './zeldaTreasure';
import { SCORE_COURSES, type CourseId } from './courses';

const LEGACY_KEY = 'jumpman.save.v1';
const SLOT_KEY = 'jumpman.save.slot';
export const TOTAL_STARS = 120;

function starKey(course: CourseId, act = 1): string {
  return act > 1 ? `${course}:${act}` : course;
}

export type CastleKey = 1 | 2;
const heldFlag = (key: CastleKey) => key === 1 ? 0x10 : 0x20;
const unlockedFlag = (key: CastleKey) => key === 1 ? 0x40 : 0x80;

const dungeonFlags = ['ladder','web','scrub','eye','crossed','block','burned','brothers','gohma','heart','emerald'] as const;
export type DekuFlag = typeof dungeonFlags[number];
interface SaveData {
  deku?: { equipped:boolean; seeds:number; flags:DekuFlag[] };
  kokiri?: { rupees: number; shield: boolean; sword?: boolean; treasure?: Record<string,number>; dekuPathOpen?: boolean; collected: string[]; spoken?: string[] };
  flags: number;
  version: 1 | 2 | 3;
  stars: string[];
  cannons: string[];
  bestCoins: Partial<Record<CourseId, number>>;
}

function emptySave(): SaveData {
  return { version: 3, flags: 0, stars: [], cannons: [], bestCoins: {} };
}

function storageKey(slot: number): string {
  return `${LEGACY_KEY}.slot${slot}`;
}

function parse(raw: string | null): SaveData {
  try {
    if (!raw) return emptySave();
    const parsed = JSON.parse(raw) as Partial<SaveData>;
    if ((parsed.version !== 1 && parsed.version !== 2 && parsed.version !== 3) || !Array.isArray(parsed.stars)) return emptySave();
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
    const migrated = stars.map(id => {
      if (parsed.version === 1 && id === 'jrb') return 'jrb:3';
      if (parsed.version !== 3 && id === 'bob:3') return 'bob:6';
      return id;
    });
    const cannons = Array.isArray(parsed.cannons) ? parsed.cannons.filter(id => typeof id === 'string') : [];
    const k = parsed.kokiri;
    const kokiri = k ? {rupees: Math.max(0,Math.min(99,Number.isInteger(k.rupees) ? k.rupees : 0)),
      treasure: Object.fromEntries(Object.entries(k.treasure ?? {}).filter(([key,value]) =>
        /^[a-z_]+:\d+$/.test(key) && Number.isInteger(value) && Object.hasOwn(treasureRewards,value))),
      spoken: Array.isArray(k.spoken) ? k.spoken.filter(id=>typeof id === 'string') : [],
      sword:k.sword === true, dekuPathOpen:k.dekuPathOpen === true && k.sword === true && k.shield === true,
      shield: k.shield === true, collected: Array.isArray(k.collected) ? k.collected.filter(id=>typeof id === 'string') : []} : undefined;
    const dungeon = parsed.deku;
    const deku = dungeon ? {equipped:dungeon.equipped===true,
      seeds:Number.isInteger(dungeon.seeds) ? Math.max(0,Math.min(30,dungeon.seeds)) : 30,
      flags:Array.isArray(dungeon.flags) ? dungeon.flags.filter(f=>dungeonFlags.includes(f)) : []} : undefined;
    return { version: 3, kokiri, deku, flags, cannons, stars: [...new Set(migrated)], bestCoins };
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
  private beforeSaveChoice?: SaveData;

  deferCourseSave(): void {
    this.beforeSaveChoice ??= structuredClone(this.data);
  }

  confirmCourseSave(): void {
    this.beforeSaveChoice = undefined;
    this.persist();
  }

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

  collectStar(course: CourseId, act = 1, coins = 0): void {
    const key = starKey(course, act);
    const newStar = !this.data.stars.includes(key);
    const highScore = SCORE_COURSES.has(course) && coins > this.bestCoins(course);
    if (newStar) this.data.stars.push(key);
    // SM64 compares the full run total, then stores the low byte of the record.
    if (highScore) this.data.bestCoins[course] = coins & 0xff;
    if (newStar || highScore) this.persist();
  }

  hasSpokenTo(id: string): boolean { return this.data.kokiri?.spoken?.includes(id) ?? false; }
  finishConversation(id: string): void {
    if (this.hasSpokenTo(id)) return;
    const village = this.data.kokiri ??= {rupees:0,shield:false,collected:[]};
    (village.spoken ??= []).push(id);
    this.persist();
  }

  chestOpened(key: string): boolean {
    return Object.hasOwn(this.data.kokiri?.treasure ?? {},key);
  }

  openTreasureChest(key: string, item: number): boolean {
    const reward = treasureRewards[item];
    if (!reward || this.chestOpened(key)) return false;
    const village = this.data.kokiri ??= {rupees:0,shield:false,collected:[]};
    (village.treasure ??= {})[key] = item;
    village.rupees = Math.min(99,village.rupees+(reward.rupees ?? 0));
    this.persist();
    return true;
  }

  private dungeonState() { return this.data.deku ??= {equipped:false,seeds:30,flags:[]}; }
  get hasSlingshot() { return this.chestOpened('deku_tree:1'); }
  get slingshotEquipped() { return this.hasSlingshot && this.data.deku?.equipped===true; }
  get dekuSeeds() { return this.hasSlingshot ? this.data.deku?.seeds ?? 30 : 0; }
  setSlingshotEquipped(equipped:boolean) {
    if (!this.hasSlingshot) return;
    this.dungeonState().equipped=equipped;
    this.persist();
  }
  useSeed() {
    if (!this.slingshotEquipped || this.dekuSeeds===0) return false;
    this.dungeonState().seeds--;
    this.persist();
    return true;
  }
  refillSeeds() {
    if (!this.hasSlingshot || this.dekuSeeds===30) return;
    this.dungeonState().seeds=30;
    this.persist();
  }
  dekuCleared(flag:DekuFlag) { return !!this.data.deku?.flags.includes(flag); }
  clearDekuChallenge(flag:DekuFlag) {
    if (this.dekuCleared(flag)) return;
    this.dungeonState().flags.push(flag);
    this.persist();
  }

  get treasureInventory(): string[] {
    return Object.values(this.data.kokiri?.treasure ?? {}).map(item=>treasureRewards[item])
      .filter(reward=>!reward.rupees && !reward.heal).map(reward=>reward.name);
  }

  get rupees(): number { return this.data.kokiri?.rupees ?? 0; }
  get kokiriSword(): boolean { return this.data.kokiri?.sword === true; }
  get dekuPathOpen(): boolean { return this.data.kokiri?.dekuPathOpen === true; }

  collectKokiriSword(): boolean {
    if (this.kokiriSword) return false;
    const village = this.data.kokiri ??= {rupees:0,shield:false,collected:[]};
    village.sword = true;
    this.persist();
    return true;
  }

  openDekuPath(): boolean {
    if (!this.kokiriSword || !this.dekuShield || this.dekuPathOpen) return false;
    this.data.kokiri!.dekuPathOpen = true;
    this.persist();
    return true;
  }

  get dekuShield(): boolean { return this.data.kokiri?.shield === true; }
  hasVillagePickup(key: string): boolean { return this.data.kokiri?.collected.includes(key) ?? false; }

  collectVillageRupees(key: string, value: number, permanent: boolean): boolean {
    if (this.rupees >= 99 || this.hasVillagePickup(key)) return false;
    const wallet = this.data.kokiri ??= {rupees:0, shield:false, collected:[]};
    wallet.rupees = Math.min(99,wallet.rupees+value);
    if (permanent) wallet.collected.push(key);
    this.persist();
    return true;
  }

  buyVillageItem(item: 'shield' | 'heart'): boolean {
    const cost = item === 'shield' ? 40 : 10;
    const wallet = this.data.kokiri;
    if (!wallet || wallet.rupees < cost || (item === 'shield' && wallet.shield)) return false;
    wallet.rupees -= cost;
    if (item === 'shield') wallet.shield = true;
    this.persist();
    return true;
  }

  get wingSwitchOn(): boolean { return (this.data.flags & 2) !== 0; }
  pressWingSwitch(): void { this.data.flags |= 2; this.persist(); }

  cannonOpen(course: CourseId): boolean { return this.data.cannons.includes(course); }

  openCannon(course: CourseId): void {
    if (this.cannonOpen(course)) return;
    this.data.cannons.push(course);
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

  reset(): void {
    this.data = emptySave();
    this.confirmCourseSave();
  }

  private persist(): void {
    try {
      // Native course progress stays in memory until the player saves. Village
      // inventory uses its own autosave even while a course reward is pending.
      const durable = this.beforeSaveChoice ? {...this.beforeSaveChoice, kokiri:this.data.kokiri} : this.data;
      localStorage.setItem(storageKey(this.slot), JSON.stringify(durable));
    } catch {
      // Private browsing or a full storage quota: keep the in-memory record.
    }
  }
}
