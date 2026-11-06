import { beforeEach, describe, expect, it } from 'vitest';
import { activeSaveSlot, SaveFile } from '../src/game/save';

describe('persistent save file', () => {
  const store = new Map<string, string>();

  beforeEach(() => {
    store.clear();
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => store.get(key) ?? null,
        setItem: (key: string, value: string) => { store.set(key, value); },
        removeItem: (key: string) => { store.delete(key); },
        clear: () => { store.clear(); },
        key: () => null,
        length: 0,
      } satisfies Storage,
    });
  });

  it('persists collected stars and best coin counts across instances', () => {
    const save = new SaveFile();
    expect(save.stars).toBe(0);
    expect(save.hasStar('bob')).toBe(false);

    save.collectStar('bob');
    save.collectStar('bob');
    save.recordCoins('bob', 7);
    save.recordCoins('bob', 3);

    const reloaded = new SaveFile();
    expect(reloaded.stars).toBe(1);
    expect(reloaded.hasStar('bob')).toBe(true);
    expect(reloaded.bestCoins('bob')).toBe(7);

    reloaded.reset();
    const clean = new SaveFile();
    expect(clean.stars).toBe(0);
    expect(clean.bestCoins('bob')).toBe(0);
  });

  it('tracks Bob-omb Battlefield stars per act while keeping legacy act-1 saves', () => {
    const save = new SaveFile();
    save.collectStar('bob');
    save.collectStar('bob', 1);
    save.collectStar('bob', 2);
    expect(save.stars).toBe(2);
    expect(save.hasStar('bob', 1)).toBe(true);
    expect(save.hasStar('bob', 2)).toBe(true);
    expect(save.hasStar('bob', 3)).toBe(false);
    const reloaded = new SaveFile();
    expect(reloaded.hasStar('bob', 2)).toBe(true);
  });

  it('tracks each Bowser key independently of stars, and persists a door unlock', () => {
    const file = new SaveFile();
    file.collectKey(1);
    expect(new SaveFile().hasKey(1)).toBe(true);
    expect(file.unlockDoor(2)).toBe(false);
    expect(file.unlockDoor(1)).toBe(true);
    const reloaded = new SaveFile();
    expect(reloaded.doorUnlocked(1)).toBe(true);
    reloaded.collectKey(1);
    expect(reloaded.hasKey(1)).toBe(false);
    expect(reloaded.stars).toBe(0);
    expect(new SaveFile(2).doorUnlocked(1)).toBe(false);
  });

  it('restores keys earned as arena stars in older saves', () => {
    localStorage.setItem('jumpman.save.v1', JSON.stringify({ version: 1, stars: ['bob', 'bowser_2'] }));
    const save = new SaveFile();
    expect(save.stars).toBe(1);
    expect(save.hasKey(2)).toBe(true);
    expect(save.unlockDoor(2)).toBe(true);
    expect(new SaveFile().hasKey(2)).toBe(false);
  });

  it('moves legacy cave rewards to act three and preserves newly earned ship stars', () => {
    localStorage.setItem('jumpman.save.v1', JSON.stringify({ version: 1, stars: ['jrb'] }));
    const file = new SaveFile();
    expect(file.hasStar('jrb', 3)).toBe(true);
    expect(file.hasStar('jrb', 1)).toBe(false);
    file.collectStar('jrb', 1);
    expect(new SaveFile().stars).toBe(2);
    expect(new SaveFile().hasStar('jrb', 1)).toBe(true);
  });

  it.each([1,2])('migrates the Chain Chomp star from save version %i exactly once', version => {
    localStorage.setItem('jumpman.save.v1.slot2', JSON.stringify({version, stars: ['bob:3'], bestCoins: {bob: 34}}));
    const file = new SaveFile(2);
    expect(file.hasStar('bob', 6)).toBe(true);
    expect(file.hasStar('bob', 3)).toBe(false);
    file.collectStar('bob', 3);
    expect(new SaveFile(2).stars).toBe(2);
    expect(new SaveFile(2).bestCoins('bob')).toBe(34);
  });

  it('isolates the four save slots and remembers the active one', () => {
    const slotA = new SaveFile(1);
    slotA.collectStar('bob');
    expect(new SaveFile(2).stars).toBe(0);
    expect(new SaveFile(1).stars).toBe(1);

    SaveFile.select(3);
    expect(activeSaveSlot()).toBe(3);
  });
});
