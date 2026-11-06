import * as THREE from 'three';
import { KOOPA_RACE_ROUTE } from '../../sim/behaviors/koopaTheQuick';
import { atan2s, type Vec3 } from '../../sim/math';
import { emptyController, type ControllerSnapshot } from '../../sim/state';
import { makeGuide } from './dimensions';
import { MarioCrowd, PERSONALITIES, type CrowdAgent } from './marioCrowd';
import { RailPath, type RailHit } from './railPath';
import type { Experiment, ExperimentHost, StepOverride } from './types';
import { actionButton, dock, el, readout, titleCard } from './ui';

const RIVALS = 7;
const COUNTDOWN = 3 * 30 + 15;
const BEST_KEY = 'jumpman.race.best';
const ORDINALS = ['1st', '2nd', '3rd', '4th', '5th', '6th', '7th', '8th'];
const RIVAL_COLORS = [0x17b3a6, 0xf5c518, 0x1fa3e0, 0x8c3fd9, 0xff6fb5, 0xff8a1f, 0xeeeeee];

type RaceState = 'grid' | 'racing' | 'finished';

interface Runner {
  name: string;
  color: number;
  player: boolean;
  agent?: CrowdAgent;
  hit: RailHit | null;
  finishTicks: number | null;
}

/**
 * Summit Race: you against seven full-physics rivals up Bob-omb Battlefield's
 * race route. The route is the one Mario can actually run (re-planned from
 * Koopa the Quick's path); rivals that fall far behind get a gentle catch-up.
 */
export class SummitRaceExperiment implements Experiment {
  readonly id = 'race';
  readonly title = 'Summit Race';
  readonly course = 'bob' as const;
  private host!: ExperimentHost;
  private crowd: MarioCrowd | null = null;
  private rail: RailPath | null = null;
  private runners: Runner[] = [];
  private state: RaceState = 'grid';
  private ticks = 0;
  private raceTicks = 0;
  private best = Infinity;
  private readonly guide = makeGuide(24);
  private board!: HTMLElement;
  private results!: HTMLElement;
  private readouts: Array<(value: string) => void> = [];

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    try { this.best = Number(localStorage.getItem(BEST_KEY)) || Infinity; } catch { /* optional */ }
    host.scene.add(this.guide);
    titleCard(host.ui, '06', 'Summit Race', 'Beat seven rivals to the top of the mountain. Follow the gold dots — or find a shortcut.', [
      ['WASD · Space', 'run · jump'], ['Gold dots', 'the race route'], ['R', 'race again'],
    ]);
    const bar = dock(host.ui);
    bar.classList.add('xp-dock-stats');
    this.readouts = [readout(bar, 'Position'), readout(bar, 'Time'), readout(bar, 'To summit'), readout(bar, 'Best')];
    bar.append(actionButton('Race again', () => this.restart(), 'R'));
    this.board = el('div', 'xp-side xp-race-board');
    host.ui.append(this.board);
    this.results = el('section', 'xp-gameover');
    this.results.hidden = true;
    host.ui.append(this.results);
    this.crowd = new MarioCrowd({
      parent: host.scene,
      world: host.playground.collision,
      route: KOOPA_RACE_ROUTE,
      center: [0, 0, 0],
      radius: 6000,
      particles: host.particles,
      sfx: host.sfx,
      listener: () => host.camera.position,
      singleLap: true,
    });
    this.rail = new RailPath(this.crowd.route!);
    await this.restart();
  }

  /** Line everyone up on a starting grid at the foot of the route. */
  private async restart(): Promise<void> {
    const crowd = this.crowd!, rail = this.rail!;
    const start = rail.at(250);
    const [tx, tz] = start.tangent;
    const yaw = atan2s(tz, tx);
    const spots: Vec3[] = [];
    const world = this.host.playground.collision;
    for (let i = 0; i < RIVALS + 1; i++) {
      const row = Math.floor(i / 4), col = (i % 4) - 1.5;
      const x = start.point[0] - tx * row * 220 - tz * col * 170;
      const z = start.point[2] - tz * row * 220 + tx * col * 170;
      const floor = world.findFloor(x, start.point[1] + 400, z);
      spots.push([x, floor.surface ? floor.height : start.point[1], z]);
    }
    const playerSpot = spots.splice(1, 1)[0];
    this.host.placeMario(playerSpot, yaw);
    this.state = 'grid';
    this.ticks = 0;
    this.raceTicks = 0;
    this.results.hidden = true;
    await crowd.populateAt(spots, 'racer', yaw);
    crowd.agents.forEach((agent, i) => {
      agent.body.setLook({ cap: RIVAL_COLORS[i % RIVAL_COLORS.length] });
      agent.name = ['Acqua', 'Oro', 'Azul', 'Viola', 'Rosa', 'Arancia', 'Bianco'][i] ?? agent.name;
    });
    this.runners = [
      { name: 'You', color: PERSONALITIES.racer.color, player: true, hit: null, finishTicks: null },
      ...crowd.agents.map((agent, i) => ({ name: agent.name, color: RIVAL_COLORS[i % RIVAL_COLORS.length], player: false, agent, hit: null, finishTicks: null })),
    ];
    this.host.toast('ON YOUR MARKS…', 1200);
  }

  exit(): void {
    this.crowd?.dispose();
    this.crowd = null;
    this.guide.removeFromParent();
    this.guide.geometry.dispose();
    (this.guide.material as THREE.Material).dispose();
  }

  keydown(event: KeyboardEvent): boolean {
    if (event.code === 'KeyR') { void this.restart(); return true; }
    return false;
  }

  beforeStep(controller: ControllerSnapshot): StepOverride | void {
    void controller;
    if (this.state === 'grid') return { controller: emptyController() };
  }

  afterStep(): void {
    const crowd = this.crowd, rail = this.rail;
    if (!crowd || !rail || crowd.loading) return;
    this.ticks++;
    if (this.state === 'grid') {
      const left = COUNTDOWN - this.ticks;
      if (left % 30 === 0 && left > 0) { this.host.toast(String(left / 30), 700); this.host.sfx.coin(); }
      if (left <= 0) {
        this.state = 'racing';
        this.host.toast('GO!', 900);
        this.host.sfx.starSound();
      }
      return;
    }
    this.raceTicks++;
    crowd.update();
    for (const runner of this.runners) {
      const pos = runner.player ? this.host.mario.pos : runner.agent!.body.pos;
      runner.hit = rail.nearest(pos, runner.hit?.index ?? -1, 60);
      if (runner.finishTicks === null && runner.hit.s > rail.total - 260 && Math.abs(pos[1] - runner.hit.point[1]) < 500) {
        runner.finishTicks = this.raceTicks;
        if (runner.player) this.finish();
      }
    }
    this.catchUp();
  }

  /** Rivals far behind the player get nudged up the route so the race stays close. */
  private catchUp(): void {
    const player = this.runners[0];
    if (!player.hit || this.state !== 'racing') return;
    for (const runner of this.runners) {
      if (runner.player || !runner.hit || runner.finishTicks !== null || !runner.agent) continue;
      if (player.hit.s - runner.hit.s > 7000 && this.raceTicks % 90 === 0) {
        const ahead = this.rail!.at(runner.hit.s + 2500);
        runner.agent.body.teleport([ahead.point[0], ahead.point[1] + 200, ahead.point[2]]);
        runner.agent.waypoint = this.waypointNear(ahead.point);
        runner.hit = null;
        this.host.particles.sparks(ahead.point, 10);
      }
    }
  }

  private waypointNear(point: Vec3): number {
    const route = this.crowd!.route!;
    let best = 0, distance = Infinity;
    route.forEach((p, i) => {
      const d = Math.hypot(p[0] - point[0], p[2] - point[2]) + Math.abs(p[1] - point[1]);
      if (d < distance) { distance = d; best = i; }
    });
    return Math.min(route.length - 1, best + 1);
  }

  private standings(): Runner[] {
    return [...this.runners].sort((a, b) => {
      if (a.finishTicks !== null || b.finishTicks !== null) return (a.finishTicks ?? Infinity) - (b.finishTicks ?? Infinity);
      return (b.hit?.s ?? 0) - (a.hit?.s ?? 0);
    });
  }

  private finish(): void {
    this.state = 'finished';
    const place = this.standings().findIndex(runner => runner.player);
    const seconds = this.raceTicks / 30;
    const record = seconds < this.best;
    if (record) {
      this.best = seconds;
      try { localStorage.setItem(BEST_KEY, String(seconds)); } catch { /* optional */ }
    }
    this.host.sfx.starSound();
    this.host.particles.sparks([this.host.mario.pos[0], this.host.mario.pos[1] + 150, this.host.mario.pos[2]], 30);
    this.results.replaceChildren(
      el('strong', '', place === 0 ? 'YOU WIN!' : `${ORDINALS[place].toUpperCase()} PLACE`),
      el('p', 'xp-gameover-score', formatTime(seconds)),
      el('p', '', record ? 'NEW BEST TIME!' : `Best ${Number.isFinite(this.best) ? formatTime(this.best) : '—'}`),
      el('small', '', 'R · RACE AGAIN'),
    );
    this.results.hidden = false;
  }

  frame(dt: number, alpha: number): void {
    const crowd = this.crowd, rail = this.rail;
    if (!crowd || !rail) return;
    crowd.render(alpha, dt, this.host.camera);
    const player = this.runners[0];
    const hit = player?.hit ?? rail.nearest(this.host.mario.pos);
    this.updateGuide(hit);
    const standings = this.standings();
    const place = standings.findIndex(runner => runner.player);
    this.readouts[0]?.(`${ORDINALS[place] ?? '—'} / ${this.runners.length}`);
    this.readouts[1]?.(formatTime((player?.finishTicks ?? this.raceTicks) / 30));
    this.readouts[2]?.(`${Math.max(0, Math.round((rail.total - hit.s) / 100))} m`);
    this.readouts[3]?.(Number.isFinite(this.best) ? formatTime(this.best) : '—');
    this.board.replaceChildren(...standings.map((runner, i) => {
      const chip = el('span', 'xp-chip');
      chip.style.setProperty('--chip', `#${runner.color.toString(16).padStart(6, '0')}`);
      chip.append(el('b', '', ORDINALS[i]), el('i'), el('span', '', runner.player ? 'YOU' : runner.name), el('b', '', runner.finishTicks !== null ? '★' : `${Math.round(100 * (runner.hit?.s ?? 0) / rail.total)}%`));
      if (runner.player) chip.classList.add('xp-chip-you');
      return chip;
    }));
  }

  private updateGuide(hit: RailHit): void {
    const rail = this.rail!;
    const world = this.host.playground.collision;
    const matrix = new THREE.Matrix4();
    let n = 0;
    for (let i = 1; i <= this.guide.instanceMatrix.count && n < this.guide.instanceMatrix.count; i++) {
      const s = hit.s + i * 160;
      if (s > rail.total) break;
      const p = rail.at(s).point;
      const floor = world.findFloor(p[0], Math.max(p[1], this.host.renderPos.y) + 600, p[2]);
      if (!floor.surface) continue;
      const pulse = 1 + 0.3 * Math.sin(this.host.time * 6 - i * 0.7);
      matrix.makeScale(pulse, pulse, pulse).setPosition(p[0], floor.height + 16, p[2]);
      this.guide.setMatrixAt(n++, matrix);
    }
    this.guide.count = n;
    this.guide.visible = true;
    this.guide.instanceMatrix.needsUpdate = true;
  }

  debug(): Record<string, unknown> {
    return {
      state: this.state,
      raceSeconds: this.raceTicks / 30,
      routeLength: this.rail?.total ?? 0,
      runners: this.runners.map(r => ({ name: r.name, player: r.player, s: Math.round(r.hit?.s ?? 0), finished: r.finishTicks })),
      place: this.standings().findIndex(runner => runner.player) + 1,
    };
  }
}

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  return `${m}:${(seconds - m * 60).toFixed(1).padStart(4, '0')}`;
}
