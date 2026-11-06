import { atan2s, coss, sins, type Vec3 } from './math';

/** Vertical arcs and horizontal travel of bhv_spawned_star_loop at 30 Hz. */
export class SpawnedStar {
  readonly pos: Vec3;
  readonly homeY: number;
  private velocity = 50;
  private gravity = -4;
  private dx: number;
  private dz: number;
  private phase = 0;
  private spin = 0x800;
  yaw = 0;

  constructor(origin: Vec3, mario: Vec3) {
    this.homeY = mario[1] + 250;
    this.pos = [origin[0], this.homeY, origin[2]];
    const x = mario[0] - origin[0], z = mario[2] - origin[2];
    const speed = Math.hypot(x, z) / 23;
    const yaw = atan2s(z, x);
    this.dx = sins(yaw) * speed;
    this.dz = coss(yaw) * speed;
  }

  get tangible(): boolean { return this.phase === 3; }

  step(): void {
    if (this.phase === 0 && this.velocity < 0 && this.pos[1] < this.homeY) {
      this.dx = this.dz = 0;
      this.velocity = 20;
      this.gravity = -1;
      this.phase++;
    } else if (this.phase === 1) {
      this.velocity = Math.max(-4, this.velocity);
      if (this.velocity < 0 && this.pos[1] < this.homeY) {
        this.velocity = this.gravity = 0;
        this.phase++;
      }
    } else if (this.phase === 2) {
      this.phase++;
    } else if (this.tangible && this.spin > 0x400) {
      this.spin -= 0x40;
    }
    this.velocity += this.gravity;
    this.pos[0] += this.dx;
    this.pos[1] += this.velocity;
    this.pos[2] += this.dz;
    this.yaw = (this.yaw + this.spin) & 0xffff;
  }
}
