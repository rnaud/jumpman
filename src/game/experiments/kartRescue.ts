/**
 * Lakitu-style rescue for the MK64 free roam (browser twin of native/src/experiments/kart_rescue.rs).
 *
 * When the kart falls off the course the screen fades to black, the kart is put back on the nearest path node to
 * where it last had ground under it, facing down the track, and the screen fades back in. Positions are world units
 * (course units x 8); one tick is one 30 Hz sim step.
 */
type Vec3 = [number, number, number];

/** Ticks of fade to black, of holding black (the teleport happens at its start), and of fade back in. */
export const FADE_OUT = 12;
export const HOLD = 4;
export const FADE_IN = 14;
/** How far below the lowest path node (world units) the kart may fall before it is fished out. */
export const FALL_MARGIN = 1500;
/** Nodes ahead of the nearest one that the kart is turned towards (the path has a node every ~160 units). */
const LOOKAHEAD = 3;

export class Lakitu {
  private readonly nodes: Vec3[];
  private readonly floorLimit: number;
  private safe: Vec3 | null = null;
  /** Ticks since the rescue started; null while driving normally. */
  private timer: number | null = null;

  /** path in world units; the export's sentinel node (x = -32768 * scale) is skipped. */
  constructor(path: Vec3[]) {
    this.nodes = path.filter(p => p[0] > -30000);
    this.floorLimit = Math.min(...this.nodes.map(p => p[1])) - FALL_MARGIN;
  }

  /** Remember the last place the kart had ground under it. */
  noteGrounded(pos: Vec3): void { this.safe = [...pos]; }
  forget(): void { this.safe = null; this.timer = null; }
  shouldRescue(pos: Vec3): boolean { return this.timer === null && this.nodes.length > 0 && pos[1] < this.floorLimit; }
  start(): void { this.timer = 0; }
  get active(): boolean { return this.timer !== null; }

  /** Where to put the kart: the path node nearest to the last grounded position (or `from`), and the yaw
   * (forward = (sin y, cos y)) towards the node a few steps ahead. */
  destination(from: Vec3): { pos: Vec3; yaw: number } | null {
    const origin = this.safe ?? from;
    let best = -1, bestDistance = Infinity;
    this.nodes.forEach((p, i) => {
      const d = Math.hypot(p[0] - origin[0], p[2] - origin[2]) + 0.5 * Math.abs(p[1] - origin[1]);
      if (d < bestDistance) { bestDistance = d; best = i; }
    });
    if (best < 0) return null;
    const node = this.nodes[best], ahead = this.nodes[(best + LOOKAHEAD) % this.nodes.length];
    return { pos: [...node], yaw: Math.atan2(ahead[0] - node[0], ahead[2] - node[2]) };
  }

  /** Advance one tick. Returns true on the tick the kart must be teleported (the screen is fully black). */
  tick(): boolean {
    if (this.timer === null) return false;
    const next = this.timer + 1;
    this.timer = next >= FADE_OUT + HOLD + FADE_IN ? null : next;
    return next === FADE_OUT;
  }

  /** 0 = clear, 1 = black. */
  get alpha(): number {
    const t = this.timer;
    if (t === null) return 0;
    if (t < FADE_OUT) return (t + 1) / FADE_OUT;
    if (t < FADE_OUT + HOLD) return 1;
    return 1 - Math.min((t - FADE_OUT - HOLD + 1) / FADE_IN, 1);
  }

  /** The kart is held (no driving) while the screen is fading out or black. */
  get holding(): boolean { return this.timer !== null && this.timer < FADE_OUT + HOLD; }
}
