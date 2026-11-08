/**
 * Presentation for the MK64 race mode: CPU kart sprites (the same directional atlas as the player, tinted),
 * pooled item boxes / bananas / shells, the lap-position-time HUD, countdown banner and results card.
 * Rules live in kartRace.ts; kart.ts only calls the small hooks below.
 */
import * as THREE from 'three';
import type { KartGround, KartInput, KartPhysics } from './kartPhysics';
import {
  CPU_COUNT, ITEM_NAMES, KartRace, LAPS, formatTicks, loadBests, ordinal, racerLap, resultsPlace, saveBests,
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
  private readonly boxes: THREE.Mesh[] = [];
  private readonly bananas: THREE.Mesh[] = [];
  private readonly shells: THREE.Mesh[] = [];
  private readonly disposables: Array<{ dispose(): void }> = [];
  private readonly hud = el('div', 'kart-hud');
  private readonly parts = {
    place: el('b', 'kart-place'), lap: el('span', 'kart-lap'), time: el('span', 'kart-time'), best: el('small', 'kart-best'),
    item: el('div', 'kart-item'), banner: el('div', 'kart-banner'), result: el('section', 'kart-result'),
  };
  private readonly itemButton = el('button', 'kart-item-button');
  private readonly cache = new Map<string, string>();
  private raceButtons: HTMLElement[] = [];
  private freeButtons: HTMLElement[] = [];
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
  ) {
    root.add(this.group);
    for (let i = 0; i < CPU_COUNT; i++) {
      const map = atlas.clone();
      this.textures.push(map);
      const sprite = new THREE.Sprite(new THREE.SpriteMaterial({ map, alphaTest: 0.5, color: CPU_STYLE[i].tint }));
      sprite.center.set(0.5, 0.08);
      sprite.scale.set(240, 240, 1);
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
    const make = <T extends THREE.BufferGeometry>(count: number, geometry: T, color: number, list: THREE.Mesh[], opacity = 1) => {
      const material = new THREE.MeshBasicMaterial({ color, transparent: opacity < 1, opacity });
      this.disposables.push(geometry, material);
      for (let i = 0; i < count; i++) {
        const mesh = new THREE.Mesh(geometry, material);
        mesh.visible = false;
        this.group.add(mesh);
        list.push(mesh);
      }
    };
    make(12, new THREE.BoxGeometry(64, 64, 64), 0xffd24a, this.boxes, 0.85);
    make(12, new THREE.TorusGeometry(30, 11, 6, 12, Math.PI), 0xf3cd2b, this.bananas);
    make(8, new THREE.SphereGeometry(30, 10, 8), 0x3fbf45, this.shells);
  }

  get active(): boolean { return this.race !== null; }

  /** Dock buttons and the HUD; `free` buttons are hidden while a race runs, the others only then. */
  mount(bar: HTMLElement, free: HTMLElement[], hooks: { start(): void; exit(): void; change(): void }): void {
    this.onStart = hooks.start;
    this.onExit = hooks.exit;
    this.onChange = hooks.change;
    this.freeButtons = free;
    const touch = isTouch();
    const race = actionButton(touch ? 'Race' : 'Race · T', () => hooks.start(), touch ? undefined : 'T');
    race.dataset.kartRace = '';
    const item = actionButton(touch ? 'Item' : 'Item · E', () => { this.itemRequested = true; }, touch ? undefined : 'E');
    const retry = actionButton(touch ? 'Retry' : 'Retry · T', () => hooks.start());
    const leave = actionButton(touch ? 'Free roam' : 'Free roam · N', () => hooks.exit());
    leave.dataset.kartFree = '';
    this.raceButtons = [item, retry, leave];
    bar.append(race, ...this.raceButtons);
    this.startButton = race;
    const p = this.parts;
    const row = el('div', 'kart-row');
    row.append(p.place, p.lap, p.time, p.best);
    this.hud.append(row, p.item);
    p.item.append(el('span', 'kart-item-name'));
    this.itemButton.type = 'button';
    this.itemButton.textContent = 'ITEM';
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
    this.host.ui.append(this.hud, p.banner, p.result, this.itemButton);
    this.syncVisibility();
  }

  private syncVisibility(): void {
    const racing = this.active;
    for (const b of [...this.freeButtons, this.startButton]) b.hidden = racing;
    for (const b of this.raceButtons) b.hidden = !racing;
    this.hud.hidden = !racing;
    this.itemButton.hidden = !racing;
    if (!racing) { this.parts.banner.hidden = true; this.parts.result.hidden = true; this.host.ui.classList.remove('kart-finished'); }
  }

  /** Start or restart a race; returns the player's fresh kart on the grid. */
  begin(ground: KartGround): KartPhysics {
    if (!this.race) {
      this.race = new KartRace(this.path, SEED);
      this.race.bestLap = this.bests.bestLap;
      this.race.bestTotal = this.bests.bestTotal;
    }
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
    this.handleEvents(race.drainEvents());
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
      const k = race.cpus[c].kart;
      const sprite = this.sprites[c], a = this.prev[c], a1 = Math.min(1, Math.max(0, alpha));
      const px = (a[0] + (k.pos[0] - a[0]) * a1) * u, py = (a[1] + (k.pos[1] - a[1]) * a1) * u, pz = (a[2] + (k.pos[2] - a[2]) * a1) * u;
      sprite.visible = true;
      sprite.position.set(px, py + 100, pz);
      const gem = this.markers[c];
      gem.visible = true;
      gem.position.set(px, py + 252, pz);
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
      const mesh = this.boxes[i];
      mesh.visible = b.respawn === 0;
      mesh.position.set(b.pos[0] * u, b.y * u + 70 + Math.sin(t * 3 + i) * 8, b.pos[1] * u);
      mesh.rotation.set(t * 1.3, t * 1.9, 0);
      (mesh.material as THREE.MeshBasicMaterial).color.setHSL((t * 0.4) % 1, 0.9, 0.6);
    });
    this.bananas.forEach((m, i) => {
      const b = race.bananas[i];
      m.visible = !!b;
      if (b) { m.position.set(b.pos[0] * u, b.y * u + 22, b.pos[1] * u); m.rotation.set(-Math.PI / 2 + 0.5, t + i, 0); }
    });
    this.shells.forEach((m, i) => {
      const s = race.shells[i];
      m.visible = !!s;
      if (s) { m.position.set(s.pos[0] * u, s.y * u + 36, s.pos[1] * u); m.rotation.y = t * 12; }
    });
    this.updateHud(race, kart);
    void dt;
  }

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
    if (me.roulette > 0) {
      const names = Object.keys(ITEM_NAMES) as Item[];
      const pick = names[Math.floor(race.clock / 3) % names.length];
      text = ITEM_NAMES[pick];
      color = ITEM_CSS[pick];
    } else if (me.item) { text = ITEM_NAMES[me.item]; color = ITEM_CSS[me.item]; }
    this.set('item', p.item.firstElementChild as HTMLElement, text);
    this.set('itemButton', this.itemButton, text === 'ITEM' ? 'ITEM' : text.toUpperCase());
    p.item.style.setProperty('--item', color || 'transparent');
    p.item.dataset.state = me.roulette > 0 ? 'roulette' : me.item ? 'ready' : 'empty';
    this.itemButton.style.setProperty('--item', color || '#24324f');
    this.itemButton.disabled = false;
    const banner = race.banner();
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
    if (!this.active) return false;
    if (event.code === 'KeyN') { this.onExit(); return true; }
    if (event.code === 'KeyE' || event.code === 'ShiftLeft' || event.code === 'ShiftRight') { this.itemRequested = true; return true; }
    return false;
  }

  dispose(): void {
    this.group.removeFromParent();
    this.hud.remove();
    this.parts.banner.remove();
    this.parts.result.remove();
    this.itemButton.remove();
    this.host.ui.classList.remove('kart-finished');
    for (const s of this.sprites) (s.material as THREE.Material).dispose();
    this.textures.forEach(t => t.dispose());
    this.disposables.forEach(d => d.dispose());
  }
}
