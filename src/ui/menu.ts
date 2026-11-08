import { MODE_CARDS, thumbnailFor } from './modes';
import { initTabs } from './tabs';

function cardFor(mode: (typeof MODE_CARDS)[number], index: number): HTMLButtonElement {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'mx-card';
  button.dataset.experiment = mode.id;
  button.style.setProperty('--hue', String(mode.hue));
  const thumb = document.createElement('span');
  thumb.className = 'mx-thumb';
  const img = document.createElement('img');
  img.src = thumbnailFor(mode.id);
  img.alt = '';
  img.width = 512;
  img.height = 320;
  img.loading = 'lazy';
  img.decoding = 'async';
  img.addEventListener('error', () => img.remove());
  const num = document.createElement('b');
  num.textContent = String(index + 1).padStart(2, '0');
  thumb.append(img, num);
  const body = document.createElement('span');
  body.className = 'mx-card-body';
  const title = document.createElement('strong');
  title.textContent = mode.title;
  const blurb = document.createElement('span');
  blurb.className = 'mx-blurb';
  blurb.textContent = mode.blurb;
  body.append(title, blurb);
  button.append(thumb, body);
  return button;
}

/** Fill the mode grids and the level-tab header art, then wire the tab bars. Run before the Game binds its buttons. */
export function buildMenus(): void {
  for (const grid of document.querySelectorAll<HTMLElement>('[data-mode-grid]')) {
    grid.replaceChildren(...MODE_CARDS.map(cardFor));
  }
  for (const art of document.querySelectorAll<HTMLElement>('[data-levels-art]')) {
    const img = document.createElement('img');
    img.src = thumbnailFor('levels');
    img.alt = '';
    img.loading = 'lazy';
    img.decoding = 'async';
    img.addEventListener('error', () => img.remove());
    art.append(img);
  }
  for (const root of document.querySelectorAll<HTMLElement>('[data-menu-tabs]')) initTabs(root);
}
