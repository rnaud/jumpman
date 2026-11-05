import { beforeEach, describe, expect, it } from 'vitest';
import { SaveFile } from '../src/game/save';

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
});
