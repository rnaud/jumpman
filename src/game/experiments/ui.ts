/** Tiny DOM helpers so every experiment's controls share one visual language. */

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K, className = '', text = '',
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

/** The touch overlay is available (phones and tablets): hints should talk about fingers, not keys. */
export const isTouch = (): boolean => typeof matchMedia === 'function' && matchMedia('(any-pointer: coarse)').matches;

/** The experiment title card in the top-left: name, one-line pitch, key hints. With `touchHints` on a touch
 * screen it becomes a compact card (no pitch) with finger wording that steps aside after a few seconds. */
export function titleCard(
  root: HTMLElement, number: string, title: string, pitch: string, hints: Array<[string, string]>, touchHints?: Array<[string, string]>,
): HTMLElement {
  const touch = !!touchHints && isTouch();
  const card = el('section', touch ? 'xp-card xp-card-touch' : 'xp-card');
  card.append(el('b', 'xp-num', number), el('strong', 'xp-title', title));
  if (!touch) card.append(el('p', 'xp-pitch', pitch));
  const list = el('dl', 'xp-hints');
  for (const [key, action] of touch ? touchHints! : hints) list.append(el('dt', '', key), el('dd', '', action));
  card.append(list);
  root.append(card);
  return card;
}

/** Rename the round touch buttons (A, B, Z) while an experiment runs; `undefined` restores the letter. */
export function relabelTouch(labels: Partial<Record<'A' | 'B' | 'Z', string | undefined>>): void {
  for (const [name, text] of Object.entries(labels)) {
    const button = document.querySelector<HTMLElement>(`[data-touch=${name}]`);
    if (!button) continue;
    button.dataset.label ??= button.textContent ?? name;
    const want = text ?? button.dataset.label;
    if (button.textContent !== want) button.textContent = want;
    button.classList.toggle('relabelled', text !== undefined);
  }
}

/** Set or clear a state class (e.g. a drift tint) on one touch button. */
export function tintTouch(name: 'A' | 'B' | 'Z', className: string, on: boolean): void {
  document.querySelector<HTMLElement>(`[data-touch=${name}]`)?.classList.toggle(className, on);
}

export interface Segmented<T extends string> {
  readonly element: HTMLElement;
  set(value: T): void;
}

/** A row of mutually exclusive buttons. */
export function segmented<T extends string>(
  options: ReadonlyArray<{ value: T; label: string; key?: string; title?: string }>,
  initial: T,
  onChange: (value: T) => void,
  className = '',
): Segmented<T> {
  const element = el('div', `xp-seg ${className}`.trim());
  const buttons = new Map<T, HTMLButtonElement>();
  for (const option of options) {
    const button = el('button', '', '');
    button.type = 'button';
    button.title = option.title ?? option.label;
    if (option.key) button.append(el('kbd', '', option.key));
    button.append(el('span', '', option.label));
    button.addEventListener('pointerdown', event => event.stopPropagation());
    button.addEventListener('click', event => {
      event.stopPropagation();
      (event.currentTarget as HTMLButtonElement).blur();
      onChange(option.value);
    });
    buttons.set(option.value, button);
    element.append(button);
  }
  const set = (value: T) => {
    for (const [key, button] of buttons) button.toggleAttribute('aria-pressed', key === value);
  };
  set(initial);
  return { element, set };
}

export function actionButton(label: string, onClick: () => void, key?: string): HTMLButtonElement {
  const button = el('button', 'xp-action');
  button.type = 'button';
  if (key) button.append(el('kbd', '', key));
  button.append(el('span', '', label));
  button.addEventListener('pointerdown', event => event.stopPropagation());
  button.addEventListener('click', event => {
    event.stopPropagation();
    button.blur();
    onClick();
  });
  return button;
}

/** A bottom-centre dock that holds the experiment's main controls. */
export function dock(root: HTMLElement): HTMLElement {
  const element = el('div', 'xp-dock');
  element.addEventListener('pointerdown', event => event.stopPropagation());
  element.addEventListener('wheel', event => event.stopPropagation());
  root.append(element);
  return element;
}

export function readout(parent: HTMLElement, label: string): (value: string) => void {
  const wrap = el('div', 'xp-readout');
  const value = el('span', 'xp-readout-value', '—');
  wrap.append(el('span', 'xp-readout-label', label), value);
  parent.append(wrap);
  let last = '';
  return text => {
    if (text === last) return;
    last = text;
    value.textContent = text;
  };
}
