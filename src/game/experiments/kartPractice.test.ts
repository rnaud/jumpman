import { describe, expect, it } from 'vitest';
import { KartPhysics, type KartGround, type KartInput } from './kartPhysics';
import { KartRace, ROULETTE_TICKS } from './kartRace';

const flat = (): KartGround => ({ floor: () => ({ height: 0, normal: [0, 1, 0], grass: false }), wall: () => null });
/** A 1200 x 800 rectangle that starts mid-edge heading -z like Luigi Raceway, plus the export's sentinel node. */
function oval(): number[][] {
  const pts: number[][] = [];
  const corners = [[0, 0], [0, -400], [1200, -400], [1200, 400], [0, 400]];
  for (let k = 0; k < corners.length; k++) {
    const a = corners[k], b = corners[(k + 1) % corners.length];
    const n = Math.trunc((Math.abs(b[0] - a[0]) + Math.abs(b[1] - a[1])) / 20);
    for (let s = 0; s < n; s++) pts.push([a[0] + (b[0] - a[0]) * s / n, 0, a[1] + (b[1] - a[1]) * s / n]);
  }
  pts.push([-32768, -32768, -32768]);
  return pts;
}
const PAD: KartInput = { rawX: 0, rawY: 0, accel: false, brake: false, hopDown: false, hopPressed: false };

describe('kart free roam with items (practice phase)', () => {
  it('has item boxes, no CPUs or laps, and never moves a parked kart', () => {
    const r = new KartRace(oval(), 4);
    const g = flat();
    const k = new KartPhysics([0, 0, 0], -32768);
    r.startPractice(k, g);
    expect(r.phase).toBe('practice');
    expect(r.cpus).toHaveLength(0);
    expect(r.racers).toHaveLength(1);
    expect(r.boxes.length).toBeGreaterThanOrEqual(15);
    expect(r.banner()).toBeNull();
    r.tick(PAD, false, k, g);
    expect(Math.hypot(k.pos[0], k.pos[2])).toBeLessThan(1);
    expect(r.phase).toBe('practice');
  });
  it('a box rolls an item for the player, which can then be used; the box respawns later', () => {
    const r = new KartRace(oval(), 4);
    const g = flat();
    r.startPractice(new KartPhysics([0, 0, 0], 0), g);
    const box = r.boxes[1];
    const k = new KartPhysics([box.pos[0], 0, box.pos[1]], 0);
    r.tick(PAD, false, k, g);
    expect(r.drainEvents()).toContainEqual({ kind: 'pickup', i: 0 });
    expect(r.racers[0].roulette).toBeGreaterThan(0);
    expect(box.respawn).toBeGreaterThan(0);
    for (let i = 0; i < ROULETTE_TICKS + 1; i++) r.tick(PAD, false, k, g);
    expect(r.racers[0].item).not.toBeNull();
    const item = r.racers[0].item!;
    r.tick(PAD, true, k, g);
    expect(r.racers[0].item).toBeNull();
    expect(r.drainEvents()).toContainEqual({ kind: 'use', i: 0, item });
    k.pos[0] += 4000;
    for (let i = 0; i < 200; i++) r.tick(PAD, false, k, g);
    expect(box.respawn).toBe(0);
  });
  it('rolls the same items for the same seed, from the middle of the pack', () => {
    const rolls = () => { const r = new KartRace(oval(), 9); r.startPractice(new KartPhysics([0, 0, 0], 0), flat()); return Array.from({ length: 30 }, () => r.roll(0)); };
    expect(rolls()).toEqual(rolls());
    expect(new Set(rolls()).size).toBeGreaterThan(1);
  });
  it('a banana you drive back over spins you out and says what hit', () => {
    const r = new KartRace(oval(), 4);
    const g = flat();
    const k = new KartPhysics([0, 0, 0], 0);
    r.startPractice(k, g);
    r.bananas.push({ pos: [k.pos[0], k.pos[2]], y: 0, owner: 0, age: 40 });
    r.tick(PAD, false, k, g);
    expect(r.racers[0].spin).toBeGreaterThan(0);
    expect(r.drainEvents()).toContainEqual({ kind: 'smash', i: 0, item: 'banana' });
  });
});
