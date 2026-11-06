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

  private returnTicks = 0;
  constructor(origin: Vec3, mario?: Vec3) {
    // bhvSpawnedStar sets parameter 2 to 1; only the 100-coin star flies to Mario.
    this.homeY = mario ? mario[1] + 250 : origin[1];
    this.pos = [origin[0], this.homeY, origin[2]];
    const x = mario ? mario[0] - origin[0] : 0, z = mario ? mario[2] - origin[2] : 0;
    const speed = Math.hypot(x, z) / 23;
    const yaw = atan2s(z, x);
    this.dx = sins(yaw) * speed;
    this.dz = coss(yaw) * speed;
  }

  get tangible(): boolean { return this.phase === 3; }
  get returning(): boolean { return this.phase === 2; }
  get bouncing(): boolean { return this.phase === 1; }

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
      if (++this.returnTicks >= 15) this.phase++;
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

/** bhv_grand_star_loop waits, arcs to the center, then bounces before becoming tangible. */
export class GrandStar {
  readonly pos: Vec3;
  private ticks = 0;
  private velocity = 80;
  private bounced = false;
  private dx: number;
  private dz: number;
  private homeY: number;
  tangible = false;

  constructor(origin: Vec3) {
    this.pos = [...origin];
    this.homeY = origin[1];
    const speed = Math.hypot(origin[0], origin[2]) / 79;
    const yaw = atan2s(-origin[2], -origin[0]);
    this.dx = sins(yaw) * speed;
    this.dz = coss(yaw) * speed;
  }

  step(): void {
    if (this.tangible || this.ticks++ < 72) return;
    this.velocity -= 2;
    this.pos[1] += this.velocity;
    this.pos[0] += this.dx;
    this.pos[2] += this.dz;
    if (!this.bounced && this.pos[1] < this.homeY) {
      this.bounced = true;
      this.pos[1] = this.homeY;
      this.velocity = 60;
      this.dx = this.dz = 0;
    } else if (this.bounced && this.velocity < 0 && this.pos[1] < this.homeY + 200) {
      this.pos[1] = this.homeY + 200;
      this.tangible = true;
    }
  }
}
