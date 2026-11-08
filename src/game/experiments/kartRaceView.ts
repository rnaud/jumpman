/**
 * Presentation for the MK64 items and race mode: CPU kart sprites (the same directional atlas as the player,
 * tinted), pooled item boxes (translucent rainbow cube tumbling around a spinning '?'), bananas and shells, the item
 * window, and in a race the lap-position-time HUD, countdown banner and results card. Item boxes and items also
 * exist in free roam ("practice" phase of kartRace.ts). Rules live in kartRace.ts; kart.ts only calls the small
 * hooks below.
 */
import * as THREE from 'three';
import type { KartGround, KartInput, KartPhysics } from './kartPhysics';
import {
  BOX_RESPAWN, CPU_COUNT, ITEM_NAMES, ITEMS, KartRace, LAPS, formatTicks, loadBests, ordinal, racerLap, resultsPlace, saveBests,
  type Item, type RaceEvent,
} from './kartRace';
import { actionButton, el, isTouch } from './ui';
import type { ExperimentHost } from './types';

export const CPU_STYLE = [
  { name: 'Luigi', tint: 0x86ff86, css: '#58d058' },
  { name: 'Peach', tint: 0xffa6d4, css: '#ff8ec4' },
  { name: 'Toad', tint: 0x9fc4ff, css: '#7fb0ff' },
] as const;
const ITEM_CSS: Record<Item, string> = { mushroom: '#e5483c', banana: '#f3cd2b', shell: '#46b44b' };
const SEED = 20240607;
/** Item icons and the box's question mark (scripts/make-kart-items.py). */
export type ItemTextures = Record<Item | 'question', THREE.Texture>;
export const ITEM_ICON: Record<Item | 'question', string> = {
  mushroom: '/mk64/item-mushroom.png', banana: '/mk64/item-banana.png', shell: '/mk64/item-shell.png', question: '/mk64/item-question.png',
};
/** Edge of the item box cube, world units (10 course units). */
const BOX_SIZE = 80;
/** MK64 draws the kart as an 18 x 18 course-unit quad: 144 world units. */
export const SPRITE_SIZE = 144;

/** MK64 picks sprite columns from the view angle: denser in the rear 45 degrees, then nine-degree steps. */
function spriteColumn(relative: number): { frame: number; flip: boolean } {
  const degrees = Math.abs(relative) * 180 / Math.PI;
  return { frame: Math.min(34, Math.round(degrees < 45 ? degrees / 2.86 : degrees / 9 + 15)), flip: relative < 0 };
}
const wrap = (a: number) => Math.atan2(Math.sin(a), Math.cos(a));

export class KartRaceView {
  race: KartRace | null = null;
  private readonly bests = loadBests();
  private readonly group = new THREE.Group();
  private readonly sprites: THREE.Sprite[] = [];
  /** A coloured gem floating over each CPU so the three are easy to tell apart. */
  private readonly markers: THREE.Mesh[] = [];
  private readonly prev: Array<[number, number, number]> = [];
  private readonly textures: THREE.Texture[] = [];
  private readonly boxes: THREE.Group[] = [];
  private readonly bananas: THREE.Sprite[] = [];
  private readonly shells: THREE.Sprite[] = [];
  /** Events drained by the last tick (for sound cues). */
  lastEvents: RaceEvent[] = [];
  /** World positions of the boxes smashed by the last tick. */
  smashed: Array<[number, number, number]> = [];
  private readonly disposables: Array<{ dispose(): void }> = [];
  private readonly hud = el('div', 'kart-hud');
  private readonly parts = {
    place: el('b', 'kart-place'), lap: el('span', 'kart-lap'), time: el('span', 'kart-time'), best: el('small', 'kart-best'),
    item: el('div', 'kart-item'), banner: el('div', 'kart-banner'), result: el('section', 'kart-result'),
  };
  private readonly row = el('div', 'kart-row');
  private readonly itemIcon = el('img', 'kart-item-icon');
  private readonly buttonIcon = el('img', 'kart-item-button-icon');
  private readonly buttonLabel = el('span', 'kart-item-button-label');
  private readonly itemButton = el('button', 'kart-item-button');
  private readonly cache = new Map<string, string>();
  private raceButtons: HTMLElement[] = [];
  private freeButtons: HTMLElement[] = [];
  private bothButtons: HTMLElement[] = [];
  private startButton!: HTMLElement;
  private itemRequested = false;
  private rescueRequested = false;
  private onChange: () => void = () => {};
  private onExit: () => void = () => {};
  private onStart: () => void = () => {};

  constructor(
    private readonly host: ExperimentHost,
    root: THREE.Group,
    atlas: THREE.Texture,
    private readonly path: number[][],
    private readonly unit: number,
    private readonly icons: ItemTextures,
  ) {
    root.add(this.group);
    for (let i = 0; i < CPU_COUNT; i++) {
      const map = atlas.clone();
      this.textures.push(map);
      const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map, alphaTest: 0.5, color: CPU_STYLE[i].tint }));
      sprite.center.set(0.5, 0);
      sprite.scale.set(SPRITE_SIZE, SPRITE_SIZE, 1);
      sprite.visible = false;
      this.group.add(sprite);
      this.sprites.push(sprite);
      const gem = new THREE.Mesh(new THREE.OctahedronGeometry(15), new THREE.MeshBasicMaterial({ color: CPU_STYLE[i].tint }));
      gem.visible = false;
      this.disposables.push(gem.geometry, gem.material as THREE.Material);
      this.group.add(gem);
      this.markers.push(gem);
      this.prev.push([0, 0, 0]);
    }
    // Item box: a translucent cube with a rainbow gradient across its corners, a '?' spinning inside.
    const cube = new THREE.BoxGeometry(BOX_SIZE, BOX_SIZE, BOX_SIZE);
    const hues: number[] = [];
    const pos = cube.getAttribute('position');
    for (let i = 0; i < pos.count; i++) {
      const c = new THREE.Color().setHSL((((pos.getX(i) + pos.getY(i) * 1.3 - pos.getZ(i) * 0.7) / BOX_SIZE + 1.5) / 3) % 1, 0.95, 0.58);
      hues.push(c.r, c.g, c.b);
    }
    cube.setAttribute('color', new THREE.Float32BufferAttribute(hues, 3));
    const cubeMaterial = new THREE.MeshBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.62, depthWrite: false, side: THREE.DoubleSide });
    const quad = new THREE.PlaneGeometry(BOX_SIZE * 0.66, BOX_SIZE * 0.66);
    const questionMaterial = new THREE.MeshBasicMaterial({ map: this.icons.question, transparent: true, alphaTest: 0.4, side: THREE.DoubleSide });
    this.disposables.push(cube, cubeMaterial, quad, questionMaterial);
    for (let i = 0; i < 24; i++) {
      const box = new THREE.Group();
      const body = new THREE.Mesh(cube, cubeMaterial);
      const mark = new THREE.Mesh(quad, questionMaterial);
      mark.name = 'mark';
      body.name = 'body';
      body.renderOrder = 2;
      box.add(body, mark);
      box.visible = false;
      this.group.add(box);
      this.boxes.push(box);
    }
    const sprite = (list: THREE.Sprite[], count: number, map: THREE.Texture) => {
      const material = new THREE.SpriteMaterial({ map, alphaTest: 0.4 });
      this.disposables.push(material);
      for (let i = 0; i < count; i++) {
        const s = new THREE.Sprite(material);
        s.center.set(0.5, 0);
        s.visible = false;
        this.group.add(s);
        list.push(s);
      }
    };
    sprite(this.bananas, 12, this.icons.banana);
    sprite(this.shells, 8, this.icons.shell);
  }

  /** In a race (as opposed to free roam with items). */
  get active(): boolean { return this.race !== null && this.race.phase !== 'practice'; }
  /** Items are live (free roam or race). */
  get live(): boolean { return this.race !== null; }

  /** Dock buttons and the HUD; `free` buttons are hidden while a race runs, the others only then. */
  mount(bar: HTMLElement, free: HTMLElement[], hooks: { start(): void; exit(): void; change(): void }): void {
    this.onStart = hooks.start;
    this.onExit = hooks.exit;
    this.onChange = hooks.change;
    this.freeButtons = free;
    const touch = isTouch();
    const race = actionButton(touch ? 'RACE · 3 laps' : 'Race · T', () => hooks.start(), touch ? undefined : 'T');
    race.dataset.kartRace = '';
    race.classList.add('kart-race-button');
    const item = actionButton(touch ? 'Item' : 'Item · E', () => { this.itemRequested = true; }, touch ? undefined : 'E');
    const retry = actionButton(touch ? 'Retry' : 'Retry · T', () => hooks.start());
    const leave = actionButton(touch ? 'Free roam' : 'Free roam · N', () => hooks.exit());
    leave.dataset.kartFree = '';
    this.raceButtons = [retry, leave];
    this.bothButtons = [item];
    bar.append(race, ...this.bothButtons, ...this.raceButtons);
    this.startButton = race;
    const p = this.parts;
    this.row.append(p.place, p.lap, p.time, p.best);
    this.hud.append(this.row);
    this.itemIcon.alt = '';
    this.itemIcon.draggable = false;
    p.item.append(this.itemIcon, el('span', 'kart-item-name'));
    this.itemButton.type = 'button';
    this.buttonIcon.alt = '';
    this.buttonIcon.draggable = false;
    this.itemButton.append(this.buttonIcon, this.buttonLabel);
    this.itemButton.dataset.kartItem = '';
    this.itemButton.addEventListener('pointerdown', event => { event.preventDefault(); event.stopPropagation(); this.itemRequested = true; });
    p.result.append(el('strong', 'kart-result-title'), el('div', 'kart-result-body'));
    const buttons = el('div', 'kart-result-actions');
    const again = actionButton('Retry', () => hooks.start());
    again.dataset.kartRetry = '';
    const back = actionButton('Back to free roam', () => hooks.exit());
    back.dataset.kartBack = '';
    buttons.append(again, back);
    p.result.append(buttons);
    this.host.ui.append(this.hud, p.item, p.banner, p.result, this.itemButton);
    this.syncVisibility();
  }

  private syncVisibility(): void {
    const racing = this.active;
    for (const b of [...this.freeButtons, this.startButton]) b.hidden = racing;
    for (const b of this.raceButtons) b.hidden = !racing;
    for (const b of this.bothButtons) b.hidden = !this.live;
    this.hud.hidden = !this.live;
    this.row.hidden = !racing;
    this.parts.item.hidden = !this.live;
    this.itemButton.hidden = !this.live;
    if (!racing) { this.parts.banner.hidden = true; this.parts.result.hidden = true; this.host.ui.classList.remove('kart-finished'); }
  }

  private makeRace(): KartRace {
    const race = new KartRace(this.path, SEED);
    race.bestLap = this.bests.bestLap;
    race.bestTotal = this.bests.bestTotal;
    return race;
  }

  /** Free roam with item boxes: no countdown, laps or CPUs. The kart stays where it is. */
  beginPractice(kart: KartPhysics, ground: KartGround): void {
    this.race = this.makeRace();
    this.race.startPractice(kart, ground);
    this.itemRequested = this.rescueRequested = false;
    this.lastEvents = [];
    this.smashed = [];
    for (const s of [...this.sprites, ...this.markers]) s.visible = false;
    this.syncVisibility();
    this.onChange();
  }

  /** Start or restart a race; returns the player's fresh kart on the grid. */
  begin(ground: KartGround): KartPhysics {
    if (!this.race || this.race.phase === 'practice') this.race = this.makeRace();
    this.itemRequested = this.rescueRequested = false;
    const kart = this.race.start(ground);
    this.race.cpus.forEach((c, i) => { this.prev[i] = [...c.kart.pos]; });
    this.syncVisibility();
    this.onChange();
    return kart;
  }

  /** Leave race mode (the sprites and items disappear). */
  end(): void {
    this.race = null;
    this.lastEvents = [];
    for (const s of [...this.sprites, ...this.markers]) s.visible = false;
    for (const m of [...this.boxes, ...this.bananas, ...this.shells]) m.visible = false;
    this.syncVisibility();
    this.onChange();
  }

  requestItem(): void { this.itemRequested = true; }
  requestRescue(): void { this.rescueRequested = true; }

  /** One 30 Hz tick of the race. Returns the player's kart (replaced after a rescue). */
  step(input: KartInput, itemPressed: boolean, kart: KartPhysics, ground: KartGround): KartPhysics {
    const race = this.race!;
    if (this.rescueRequested) {
      this.rescueRequested = false;
      const rescued = race.rescuePlayer(ground);
      if (rescued) kart = rescued;
    }
    const use = this.itemRequested || itemPressed;
    this.itemRequested = false;
    race.cpus.forEach((c, i) => { this.prev[i] = [...c.kart.pos]; });
    kart = race.tick(input, use, kart, ground);
    this.lastEvents = race.drainEvents();
    this.smashed = this.lastEvents.some(e => e.kind === 'pickup' && e.i === 0)
      ? race.boxes.filter(b => b.respawn === BOX_RESPAWN).map(b => [b.pos[0] * this.unit, b.y * this.unit + 70, b.pos[1] * this.unit] as [number, number, number])
      : [];
    this.handleEvents(this.lastEvents);
    if (race.bestDirty) {
      race.bestDirty = false;
      this.bests.bestLap = race.bestLap;
      this.bests.bestTotal = race.bestTotal;
      saveBests(this.bests);
    }
    return kart;
  }

  private handleEvents(events: RaceEvent[]): void {
    for (const e of events) {
      if (e.kind === 'lap' && e.i === 0 && e.n < LAPS) this.host.toast(e.n === LAPS - 1 ? 'Final lap!' : `Lap ${e.n + 1} of ${LAPS}`, 1600);
      else if (e.kind === 'hit' && e.i === 0) this.host.toast('Spun out!', 1000);
      else if (e.kind === 'rescue' && e.i === 0) this.host.toast('Back on track', 1000);
    }
  }

  private set(key: string, node: HTMLElement, text: string): void {
    if (this.cache.get(key) === text) return;
    this.cache.set(key, text);
    node.textContent = text;
  }

  /** Per rendered frame: camera-facing sprites, item meshes and the HUD. */
  frame(dt: number, kart: KartPhysics, alpha = 1): void {
    const race = this.race;
    if (!race || race.racers.length === 0) return;
    const cam = this.host.camera.position;
    const u = this.unit;
    for (let c = 0; c < CPU_COUNT; c++) {
      if (c >= race.cpus.length) { this.sprites[c].visible = false; this.markers[c].visible = false; continue; }
      const k = race.cpus[c].kart;
      const sprite = this.sprites[c], a = this.prev[c], a1 = Math.min(1, Math.max(0, alpha));
      const px = (a[0] + (k.pos[0] - a[0]) * a1) * u, py = (a[1] + (k.pos[1] - a[1]) * a1) * u, pz = (a[2] + (k.pos[2] - a[2]) * a1) * u;
      sprite.visible = true;
      sprite.position.set(px, py, pz);
      const gem = this.markers[c];
      gem.visible = true;
      gem.position.set(px, py + SPRITE_SIZE + 24, pz);
      gem.rotation.set(0.6, this.host.time * 2, 0.6);
      const camAngle = Math.atan2(cam.x - sprite.position.x, cam.z - sprite.position.z);
      const relative = wrap(camAngle - k.yaw() - Math.PI + race.spinAngle(c + 1));
      const { frame, flip } = spriteColumn(relative);
      const map = (sprite.material as THREE.SpriteMaterial).map!;
      map.repeat.x = (flip ? -1 : 1) / 35;
      map.offset.x = (frame + (flip ? 1 : 0)) / 35;
    }
    const t = this.host.time;
    race.boxes.forEach((b, i) => {
      const box = this.boxes[i];
      if (!box) return;
      box.visible = b.respawn === 0;
      if (!box.visible) return;
      // MK64: the cube tumbles (x +1, y -2, z +1 degrees per 60 Hz frame) and the '?' turns twice as fast about y.
      box.position.set(b.pos[0] * u, b.y * u + 72 + Math.sin(t * 2.4 + i) * 6, b.pos[1] * u);
      const body = box.getObjectByName('body')!, mark = box.getObjectByName('mark')!;
      body.rotation.set(t * 1.05, -t * 2.1, t * 1.05);
      mark.rotation.set(0, -t * 4.2, 0);
    });
    this.bananas.forEach((m, i) => {
      const b = race.bananas[i];
      m.visible = !!b;
      if (b) { m.position.set(b.pos[0] * u, b.y * u + 4, b.pos[1] * u); m.scale.set(84, 84, 1); }
    });
    this.shells.forEach((m, i) => {
      const s = race.shells[i];
      m.visible = !!s;
      if (s) { m.position.set(s.pos[0] * u, s.y * u + 6 + Math.abs(Math.sin(t * 14 + i)) * 10, s.pos[1] * u); m.scale.set(96, 96, 1); }
    });
    this.updateHud(race, kart);
    void dt;
  }

  /** The player's racer (null before the first start). */
  me() { return this.race?.racers[0] ?? null; }
  clock(): number { return this.race?.clock ?? 0; }

  /** Spin offset for the player's own sprite. */
  playerSpin(): number { return this.race ? this.race.spinAngle(0) : 0; }

  private updateHud(race: KartRace, kart: KartPhysics): void {
    const p = this.parts;
    const me = race.racers[0];
    const rank = race.rank(0);
    this.set('place', p.place, ordinal(rank));
    this.set('lap', p.lap, `LAP ${racerLap(me)}/${LAPS}`);
    const shown = race.phase === 'countdown' ? 0 : me.finish ?? race.raceTick;
    this.set('time', p.time, formatTicks(shown));
    this.set('best', p.best, race.bestLap ? `BEST ${formatTicks(race.bestLap)}` : '');
    let text = 'ITEM';
    let color = '';
    let icon: Item | 'question' = 'question';
    if (me.roulette > 0) {
      const pick = ITEMS[Math.floor(race.clock / 3) % ITEMS.length];
      text = ITEM_NAMES[pick];
      color = ITEM_CSS[pick];
      icon = pick;
    } else if (me.item) { text = ITEM_NAMES[me.item]; color = ITEM_CSS[me.item]; icon = me.item; }
    this.set('item', p.item.querySelector('.kart-item-name') as HTMLElement, text);
    if (this.cache.get('icon') !== icon) {
      this.cache.set('icon', icon);
      this.itemIcon.src = ITEM_ICON[icon];
      this.buttonIcon.src = ITEM_ICON[icon];
    }
    this.set('itemButton', this.buttonLabel, text === 'ITEM' ? 'ITEM' : text.toUpperCase());
    p.item.style.setProperty('--item', color || 'transparent');
    p.item.dataset.state = me.roulette > 0 ? 'roulette' : me.item ? 'ready' : 'empty';
    this.itemButton.style.setProperty('--item', color || '#24324f');
    this.itemButton.disabled = false;
    const banner = race.phase === 'practice' ? null : race.banner();
    p.banner.hidden = banner === null || race.phase === 'finished';
    if (banner !== null) this.set('banner', p.banner, banner === 0 ? 'GO!' : String(banner));
    const res = race.results;
    p.result.hidden = !res;
    this.host.ui.classList.toggle('kart-finished', !!res);
    if (res) this.renderResults(res, race);
    void kart;
  }

  private renderResults(res: NonNullable<KartRace['results']>, race: KartRace): void {
    const title = this.parts.result.querySelector('.kart-result-title') as HTMLElement;
    const body = this.parts.result.querySelector('.kart-result-body') as HTMLElement;
    const place = resultsPlace(res);
    const key = `${res.total}|${res.order.join('')}|${res.laps.join(',')}`;
    if (this.cache.get('results') === key) return;
    this.cache.set('results', key);
    title.textContent = place === 1 ? '1st place!' : `${ordinal(place - 1)} place`;
    body.replaceChildren();
    const row = (label: string, value: string, className = '') => {
      const r = el('div', `kart-result-row ${className}`.trim());
      r.append(el('span', '', label), el('b', '', value));
      body.append(r);
    };
    row('Total', formatTicks(res.total) + (res.newBestTotal ? '  NEW BEST' : ''));
    res.laps.forEach((l, i) => row(`Lap ${i + 1}`, formatTicks(l) + (l === res.bestLap ? '  best' : '')));
    if (res.newBestLap) row('Best lap', formatTicks(res.bestLap) + '  NEW BEST', 'kart-new');
    const standings = el('ol', 'kart-standings');
    for (const idx of res.order) {
      const li = el('li', idx === 0 ? 'kart-you' : '', idx === 0 ? 'YOU' : CPU_STYLE[idx - 1].name);
      if (idx > 0) li.style.setProperty('--tint', CPU_STYLE[idx - 1].css);
      standings.append(li);
    }
    body.append(standings);
    void race;
  }

  debug(): Record<string, unknown> {
    const r = this.race;
    if (!r || r.racers.length === 0) return { race: false };
    if (r.phase === 'practice') {
      return {
        race: false, items: true, item: r.racers[0].item, roulette: r.racers[0].roulette, boost: r.racers[0].boost, spin: r.racers[0].spin,
        bananas: r.bananas.length, shells: r.shells.length, boxes: r.boxes.filter(b => b.respawn === 0).length,
      };
    }
    return {
      race: true, phase: r.phase, tick: r.raceTick, rank: r.rank(0), lap: racerLap(r.racers[0]), gates: r.racers[0].passed,
      item: r.racers[0].item, roulette: r.racers[0].roulette, spin: r.racers[0].spin, boost: r.racers[0].boost,
      cpus: r.cpus.map(c => c.kart.pos.map(v => Math.round(v))), standings: r.order(), results: r.results ? { ...r.results } : null,
      bananas: r.bananas.length, shells: r.shells.length, boxes: r.boxes.filter(b => b.respawn === 0).length,
    };
  }

  /** Key handling; returns true when the key belonged to the race UI. */
  keydown(event: KeyboardEvent): boolean {
    if (event.code === 'KeyT') { this.onStart(); return true; }
    if (!this.live) return false;
    if (event.code === 'KeyN' && this.active) { this.onExit(); return true; }
    if (event.code === 'KeyE' || event.code === 'ShiftLeft' || event.code === 'ShiftRight') { this.itemRequested = true; return true; }
    return false;
  }

  dispose(): void {
    this.group.removeFromParent();
    this.hud.remove();
    this.parts.item.remove();
    this.parts.banner.remove();
    this.parts.result.remove();
    this.itemButton.remove();
    this.host.ui.classList.remove('kart-finished');
    for (const s of this.sprites) (s.material as THREE.Material).dispose();
    this.textures.forEach(t => t.dispose());
    this.disposables.forEach(d => d.dispose());
  }
}
