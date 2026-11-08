import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { EXPERIMENT_IDS } from '../src/game/experiments/types';
import { MODE_CARDS, modesAreComplete, thumbnailFor } from '../src/ui/modes';
import { nextTabIndex } from '../src/ui/tabs';

const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');

describe('menu tab keyboard model', () => {
  it('wraps arrows and jumps with Home/End', () => {
    expect(nextTabIndex(3, 0, 'ArrowRight')).toBe(1);
    expect(nextTabIndex(3, 2, 'ArrowRight')).toBe(0);
    expect(nextTabIndex(3, 0, 'ArrowLeft')).toBe(2);
    expect(nextTabIndex(3, 1, 'Home')).toBe(0);
    expect(nextTabIndex(3, 1, 'End')).toBe(2);
    expect(nextTabIndex(3, 1, 'a')).toBeNull();
    expect(nextTabIndex(0, 0, 'ArrowRight')).toBeNull();
  });
});

describe('mode cards', () => {
  it('cover every experiment exactly once with a thumbnail path per mode', () => {
    expect(modesAreComplete()).toBe(true);
    expect(new Set(MODE_CARDS.map(card => card.id)).size).toBe(EXPERIMENT_IDS.length);
    for (const id of EXPERIMENT_IDS) expect(thumbnailFor(id)).toBe(`/assets/ui/modes/${id}.jpg`);
    expect(thumbnailFor('levels')).toBe('/assets/ui/modes/levels.jpg');
  });
});

describe('menu markup', () => {
  it('wires each tab to an existing tabpanel', () => {
    const tabs = [...html.matchAll(/role="tab"[^>]*aria-controls="([^"]+)"/g)].map(match => match[1]);
    expect(tabs.length).toBeGreaterThanOrEqual(5);
    for (const id of tabs) expect(html).toMatch(new RegExp(`role="tabpanel" id="${id}"`));
  });

  it('keeps the ids the smoke scripts and the game rely on', () => {
    for (const id of ['start-button', 'start-overlay', 'settings', 'leave-experiment', 'main-menu-button', 'settings-button', 'win-overlay', 'save-slots', 'mission-links', 'experiment-ui', 'experiment-badge']) {
      expect(html).toContain(`id="${id}"`);
    }
    expect(html).toContain('data-mode-grid');
  });
});
