import * as THREE from 'three';
import { KOOPA_RACE_ROUTE } from '../../sim/behaviors/koopaTheQuick';
import { angleToRadians } from '../../sim/math';
import { emptyController } from '../../sim/state';
import type { PointerHandler } from '../Input';
import { loadSm64Asset, SM64_MODELS } from '../sm64Assets';
import { MarioCrowd, PERSONALITIES, PERSONALITY_ORDER, type CrowdAgent, type CrowdMix, type Personality } from './marioCrowd';
import type { Experiment, ExperimentHost, StepOverride } from './types';
import { actionButton, dock, el, readout, segmented, titleCard, type Segmented } from './ui';

const COUNTS = [25, 50, 100] as const;

/**
 * A Hundred Marios: a crowd of full-physics, fully animated Marios with five
 * colour-coded personalities flooding the course around the player.
 */
export class HundredMariosExperiment implements Experiment {
  readonly id = 'hundred';
  readonly title = 'A Hundred Marios';
  readonly course = 'bob' as const;
  private host!: ExperimentHost;
  private crowd: MarioCrowd | null = null;
  private watching: CrowdAgent | null = null;
  private readonly watchPos = new THREE.Vector3();
  private readonly watchLook = new THREE.Vector3();
  private readonly ray = new THREE.Raycaster();
  private inspector!: HTMLElement;
  private legend!: HTMLElement;
  private countSeg!: Segmented<string>;
  private refresh = 0;
  private count = 100;
  private readouts: Array<(value: string) => void> = [];

  readonly pointer: PointerHandler = {
    down: event => {
      if (event.button !== 0 || !this.crowd) return false;
      const agent = this.crowd.pick(event.clientX, event.clientY, this.host.camera, this.host.canvas);
      if (!agent) return false;
      this.select(agent);
      return true;
    },
    move: () => false,
    up: () => {},
    wheel: () => false,
  };

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    this.buildUi();
    let coin = null;
    try { coin = await loadSm64Asset(SM64_MODELS.coin); } catch { /* coins are optional */ }
    const onBattlefield = host.worldName === 'bob';
    this.crowd = new MarioCrowd({
      parent: host.scene,
      world: host.playground.collision,
      route: onBattlefield ? KOOPA_RACE_ROUTE : undefined,
      leader: () => host.mario,
      coinTemplate: coin,
      coinCount: 40,
      center: onBattlefield ? [0, 0, 0] : [...host.mario.pos],
      radius: onBattlefield ? 6800 : 4000,
      particles: host.particles,
      sfx: host.sfx,
      listener: () => host.camera.position,
    });
    host.toast('A HUNDRED MARIOS · CLICK ONE TO INSPECT IT', 3200);
    await this.crowd.populate(this.count, [...host.mario.pos]);
  }

  exit(): void {
    this.crowd?.dispose();
    this.crowd = null;
    this.watching = null;
  }

  private buildUi(): void {
    const root = this.host.ui;
    titleCard(root, '04', 'A Hundred Marios', 'Every Mario runs the real SM64 physics. Colour shows its personality.', [
      ['Click', 'inspect a Mario'], ['Tab', 'watch the selected Mario'], ['P', 'change its personality'], ['J / Shift', 'punch the crowd'],
    ]);
    const side = el('div', 'xp-side');
    this.legend = el('div', 'xp-legend');
    side.append(this.legend);
    root.append(side);
    const bar = dock(root);
    this.countSeg = segmented(COUNTS.map(count => ({ value: String(count), label: `${count}` })), String(this.count), value => {
      this.count = Number(value);
      this.countSeg.set(value);
      void this.crowd?.populate(this.count, [...this.host.mario.pos]);
      this.host.toast(`${this.count} MARIOS INCOMING`, 1600);
    });
    const mixes: Array<{ value: CrowdMix; label: string; title: string }> = [
      { value: 'mixed', label: 'Mixed', title: 'All five personalities' },
      { value: 'race', label: 'Race day', title: 'Everyone races to the summit' },
      { value: 'gold', label: 'Gold rush', title: 'Everyone hunts coins' },
      { value: 'parade', label: 'Parade', title: 'Everyone follows you' },
      { value: 'brawl', label: 'Brawl', title: 'Everyone guards turf' },
    ];
    const mixSeg: Segmented<CrowdMix> = segmented(mixes, 'mixed', value => {
      mixSeg.set(value);
      this.crowd?.setMix(value);
    });
    const speeds = [{ value: '0.5', label: '½×' }, { value: '1', label: '1×' }, { value: '2', label: '2×' }, { value: '3', label: '3×' }];
    const speedSeg: Segmented<string> = segmented(speeds, '1', value => {
      speedSeg.set(value);
      if (this.crowd) this.crowd.speed = Number(value);
    });
    bar.append(labelled('Marios', this.countSeg.element), labelled('Mix', mixSeg.element), labelled('Sim speed', speedSeg.element));
    const stats = el('div', 'xp-stats');
    side.append(stats);
    this.readouts = [readout(stats, 'Summits'), readout(stats, 'Best lap'), readout(stats, 'Coins'), readout(stats, 'Knockbacks')];
    this.inspector = el('section', 'xp-inspector');
    this.inspector.hidden = true;
    this.inspector.addEventListener('pointerdown', event => event.stopPropagation());
    root.append(this.inspector);
  }

  private select(agent: CrowdAgent | null): void {
    if (!this.crowd) return;
    this.crowd.selected = agent;
    if (!agent && this.watching) this.watching = null;
    this.refresh = 0;
  }

  keydown(event: KeyboardEvent): boolean {
    const crowd = this.crowd;
    if (!crowd) return false;
    if (event.code === 'Tab') {
      event.preventDefault();
      if (this.watching) this.watching = null;
      else this.watching = crowd.selected ?? crowd.agents[0] ?? null;
      if (this.watching) crowd.selected = this.watching;
      this.host.toast(this.watching ? `WATCHING ${this.watching.name.toUpperCase()} · TAB TO RETURN` : 'BACK TO MARIO', 1600);
      return true;
    }
    if (event.code === 'KeyP' && crowd.selected) {
      const next = crowd.cyclePersonality(crowd.selected);
      this.host.toast(`${crowd.selected.name.toUpperCase()} IS NOW A ${PERSONALITIES[next].label.toUpperCase()}`, 1600);
      this.refresh = 0;
      return true;
    }
    return false;
  }

  beforeStep(): StepOverride | void {
    // While spectating, the player stands still instead of running off-screen.
    if (this.watching) return { controller: emptyController() };
  }

  afterStep(): void {
    this.crowd?.update();
  }

  frame(dt: number, alpha: number): void {
    const crowd = this.crowd;
    if (!crowd) return;
    if (this.watching && !crowd.agents.includes(this.watching)) this.watching = null;
    if (this.watching) this.watchCamera(this.watching, dt);
    crowd.render(alpha, dt, this.host.camera);
    if ((this.refresh -= dt) <= 0) {
      this.refresh = 0.15;
      this.updatePanels();
    }
  }

  private watchCamera(agent: CrowdAgent, dt: number): void {
    const camera = this.host.camera;
    const target = agent.body.interpolated;
    const yaw = angleToRadians(agent.body.runtime.faceAngle[1]);
    const desired = new THREE.Vector3(target.x - Math.sin(yaw) * 900, target.y + 420, target.z - Math.cos(yaw) * 900);
    // Keep the chase camera out of walls and hills.
    const head = new THREE.Vector3(target.x, target.y + 120, target.z);
    const toward = desired.clone().sub(head);
    const length = toward.length();
    this.ray.set(head, toward.normalize());
    this.ray.far = length;
    const hit = this.ray.intersectObjects(this.host.playground.collisionMeshes, false)[0];
    if (hit) desired.copy(head).addScaledVector(toward, Math.max(120, hit.distance - 60));
    const k = 1 - Math.exp(-dt * 3.5);
    if (this.watchPos.lengthSq() === 0 || this.watchPos.distanceTo(desired) > 4000) this.watchPos.copy(desired);
    this.watchPos.lerp(desired, k);
    this.watchLook.lerp(new THREE.Vector3(target.x, target.y + 110, target.z), Math.min(1, k * 2.5));
    camera.position.copy(this.watchPos);
    camera.lookAt(this.watchLook);
  }

  private updatePanels(): void {
    const crowd = this.crowd!;
    const counts = crowd.counts();
    this.legend.replaceChildren(...PERSONALITY_ORDER.filter(p => counts[p] > 0 || crowd.mix === 'mixed').map(p => {
      const chip = el('span', 'xp-chip');
      chip.style.setProperty('--chip', `#${PERSONALITIES[p].color.toString(16).padStart(6, '0')}`);
      chip.title = PERSONALITIES[p].blurb;
      chip.append(el('i'), el('span', '', PERSONALITIES[p].label), el('b', '', String(counts[p])));
      return chip;
    }));
    const { stats } = crowd;
    this.readouts[0](String(stats.summits));
    this.readouts[1](Number.isFinite(stats.bestLap) ? formatLap(stats.bestLap) : '—');
    this.readouts[2](String(stats.coins));
    this.readouts[3](String(stats.hits));
    const agent = crowd.selected;
    this.inspector.hidden = !agent;
    if (!agent) return;
    const info = crowd.describe(agent);
    const color = `#${PERSONALITIES[info.personality].color.toString(16).padStart(6, '0')}`;
    this.inspector.style.setProperty('--chip', color);
    this.inspector.replaceChildren(
      el('strong', '', info.name),
      chipFor(info.personality),
      el('p', 'xp-goal', info.label),
      row('Action', info.action.replace(/^ACT_/, '').replace(/_/g, ' ').toLowerCase()),
      row('Speed', `${Math.round(Math.abs(info.speed))} u/f`),
      row('Coins', String(info.coins)),
      ...(info.personality === 'racer' ? [row('Laps', String(info.laps)), row('Best lap', info.bestLap ? formatLap(info.bestLap) : '—')] : []),
      ...(info.personality === 'brawler' ? [row('Knockbacks', String(info.hits))] : []),
      el('div', 'xp-inspector-actions'),
    );
    const actions = this.inspector.lastElementChild as HTMLElement;
    actions.append(
      actionButton(this.watching === agent ? 'Stop watching' : 'Watch', () => {
        this.watching = this.watching === agent ? null : agent;
        this.refresh = 0;
      }, 'Tab'),
      actionButton('Personality', () => {
        crowd.cyclePersonality(agent);
        this.refresh = 0;
      }, 'P'),
      actionButton('✕', () => this.select(null)),
    );
  }

  debug(): Record<string, unknown> {
    const crowd = this.crowd;
    return {
      count: crowd?.agents.length ?? 0,
      loading: crowd?.loading ?? false,
      mix: crowd?.mix,
      speed: crowd?.speed,
      counts: crowd?.counts(),
      stats: crowd ? { ...crowd.stats } : null,
      selected: crowd?.selected ? crowd.describe(crowd.selected) : null,
      watching: this.watching?.id ?? null,
      agents: crowd?.agents.map(agent => crowd.describe(agent)) ?? [],
    };
  }

  /** Test hook: select an agent by id. */
  inspect(id: number): void {
    this.select(this.crowd?.agents.find(agent => agent.id === id) ?? null);
  }
}

function labelled(label: string, control: HTMLElement): HTMLElement {
  const group = el('div', 'xp-group');
  group.append(el('span', 'xp-group-label', label), control);
  return group;
}

function row(label: string, value: string): HTMLElement {
  const line = el('div', 'xp-row');
  line.append(el('span', '', label), el('b', '', value));
  return line;
}

function chipFor(personality: Personality): HTMLElement {
  const chip = el('span', 'xp-chip');
  chip.style.setProperty('--chip', `#${PERSONALITIES[personality].color.toString(16).padStart(6, '0')}`);
  chip.append(el('i'), el('span', '', PERSONALITIES[personality].label));
  chip.title = PERSONALITIES[personality].blurb;
  return chip;
}

function formatLap(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds - m * 60;
  return `${m}:${s.toFixed(1).padStart(4, '0')}`;
}

