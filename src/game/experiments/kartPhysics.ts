/**
 * Port of Mario Kart 64's human-player driving model (n64decomp/mk64 src/player_controller.c and
 * src/data/kart_attributes.c). Browser twin of native/src/experiments/kart_physics.rs; both are checked against
 * the same trace (kartPhysics.test.ts / kart_physics_trace.txt).
 *
 * Units are MK64's own: course units, s16 angles (65536 = 360 deg), one step() = one 60 Hz kart update. Callers
 * convert to world units (x8) and run two steps per 30 Hz tick. Decomp function names are kept in comments.
 * Ground contact follows the decomp: the floor reaction is func_8003F46C -> func_8003E9EC (Luigi Raceway), the wall
 * response is func_8003F734 / func_8003FBAC + func_8002C954 through func_8002A5F4, and the slope terms come from the
 * four tyre heights of func_80029B4C (slopeAccel, unk_206).
 * Not ported (unreachable in free roam): items, spinouts, boost ramps, the Lakitu retrieval (the host does a graceful
 * rescue instead), stars, AB-spin, triple-A/B combos, CPU logic, the terrain tumble trigger, tyre surface types other
 * than grass/asphalt. The kart origin sits BOUNDING_BOX below the decomp's, so the decomp's surfaceDistance is
 * normal.y * (y + BOUNDING_BOX - floor) - BOUNDING_BOX here.
 */
export const FRAME_HZ = 60;
const BOUNDING_BOX = 5.5; // gKartBoundingBoxSizeTable (Mario)
const KART_LENGTH = 0.75 * 18; // gCharacterSize (MARIO_SIZE) * 18
const WHEEL_BASE = KART_LENGTH + 1;
const TYRE_RADIUS = 18 * (0.75 / 2);
const FRICTION = 5800; // gKartFrictionTable
const GRAVITY = 2600; // gKartGravityTable
export const TOP_SPEED = 320; // gTopSpeed150cc (Mario)
const SPEED_CAP = 9; // gKartTopSpeedTable
const UNK_084 = -20; // D_800E2400[150cc]
const UNK_088 = 35; // D_800E24B4[150cc]
const HANDLING = 1.25; // gKartHandlingTable (Mario)
const ACCEL = [2.0, 2.0, 2.0, 1.6, 1.4, 1.2, 1.0, 0.8, 0.6, 0.4]; // gKartAccelerationMario
const HOP_JERK = 0.03;
const HOP_VELOCITY = 0.93;
const DEG = 182; // DEGREES(1) as u16
const GRASS_BACK = 0.09;
const GRASS_FRONT = 0.38;
const GRASS_BRAKE = 0.2;
const GRASS_SIDE_BACK = 0.58;
const GRASS_SIDE_FRONT = 0.38;
const NON_ASPHALT_GRAVITY = [0.825, 0.8, 0.725, 0.625, 0.425, 0.3, 0.3, 0.3, 0.3, 0.3, 0.3];
// Generated from the decomp: sp44 (func_80033AE0) and the update_steering_large/small call rows.
// LARGE = [threshold, numerator<<12, unk_090 magnitude]; SMALL = [threshold, numerator<<12, unk_090 step].
const SP44 = [0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.6, 0.6, 0.6, 0.6, 0.6, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.5, 0.5, 0.5, 0.5, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.6, 0.6, 0.6, 0.6, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8];
const LARGE: number[][] = [[90,120,450],[89,118,440],[88,116,430],[87,114,420],[86,112,410],[85,88,400],[84,86,395],[83,80,390],[82,79,390],[81,78,380],[80,77,370],[79,76,360],[78,76,360],[77,75,350],[76,74,340],[75,73,330],[74,73,330],[73,73,330],[72,72,320],[71,71,315],[70,71,315],[69,70,305],[68,70,305],[67,69,280],[66,70,270],[65,69,270],[64,68,260],[63,67,250],[62,67,250],[61,67,250],[60,61,245],[59,60,245],[58,59,245],[57,58,245],[56,56,245],[55,56,230],[54,56,230],[53,56,230],[52,56,230],[51,56,230],[50,50,220],[49,50,220],[48,50,220],[47,50,220],[46,50,220],[45,48,110],[44,46,110],[43,46,110],[42,46,110],[41,46,110],[40,46,110],[39,44,110],[38,40,110],[37,40,110],[36,36,110],[35,36,110],[34,34,110],[32,32,100],[31,32,100]];
const SMALL: number[][] = [[30,31,0.9],[29,30,0.9],[28,29,0.9],[27,28,0.9],[26,27,0.9],[25,26,1.0],[24,25,1.0],[23,24,1.0],[22,23,1.0],[21,22,1.0],[20,21,1.05],[19,20,1.05],[18,19,1.05],[17,18,1.05],[16,17,1.05],[15,16,1.2],[14,15,1.2],[13,14,1.2],[12,13,1.2],[11,12,1.2],[10,14,1.6],[9,13,1.6],[8,12,1.6],[7,11,1.6],[6,10,1.6],[5,9,1.9],[4,8,1.9],[3,7,1.9],[2,6,1.9],[1,5,1.9],[0,0,1.9]];

export interface KartInput {
  /** N64 stick units (about -80..80). */
  rawX: number;
  rawY: number;
  accel: boolean;
  brake: boolean;
  hopDown: boolean;
  hopPressed: boolean;
}
export interface KartFloor { height: number; normal: [number, number, number]; grass: boolean }
export interface KartWallHit {
  /** Unit normal pointing out of the wall, towards the kart. */
  normal: [number, number, number];
  /** Mostly faces the X axis (decomp unk54 / func_8003FBAC); otherwise the Z axis (unk48 / func_8003F734). */
  xFacing: boolean;
}
export interface KartWall {
  /** The kart position pushed out of every wall. */
  pos: [number, number, number];
  hits: KartWallHit[];
}
export interface KartGround {
  floor(x: number, y: number, z: number): KartFloor | null;
  /** If the kart at pos overlaps a wall, the corrected position and the walls hit. */
  wall(pos: [number, number, number]): KartWall | null;
}

const trunc = Math.trunc;
const s16 = (v: number) => (trunc(v) << 16) >> 16;
const sinA = (a: number) => Math.sin(a * Math.PI / 32768);
const cosA = (a: number) => Math.cos(a * Math.PI / 32768);
const atan2s = (y: number, x: number) => trunc(Math.atan2(y, x) * 32768 / Math.PI);
// move_f32_towards (snaps tiny values to zero)
const towards = (value: number, target: number, rate: number) => {
  const v = (1 - rate) * value + rate * target;
  return Math.abs(v) < 0.001 ? 0 : v;
};
// move_s16_towards
const towardsS16 = (value: number, target: number, rate: number) => s16(value - (value - target) * rate);
/** func_8002A5F4: reflect the part of v along n with restitution when distance < -threshold, otherwise slide. */
const reflect = (n: [number, number, number], distance: number, v: [number, number, number], restitution: number, threshold: number) => {
  const dot = n[0] * v[0] + n[1] * v[1] + n[2] * v[2];
  const k = distance < -threshold ? 1 + restitution : 1;
  for (let i = 0; i < 3; i++) v[i] -= dot * n[i] * k;
};
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export class KartPhysics {
  pos: [number, number, number];
  oldPos: [number, number, number];
  velocity: [number, number, number] = [0, 0, 0];
  rotation: number;
  currentSpeed = 0;
  speed = 0;
  previousSpeed = 0;
  unk08c = 0; unk0e8 = 0; unk0a0 = 0; unk104 = 0;
  unk078 = 0; unk090 = 0; unk0c0 = 0; unk20c = 0; unk208 = UNK_088;
  steerPosition = 0;
  steerChangeIncrement = 0;
  steerPositionDelta = 0;
  hop = false; drifting = false; midair = false; braking = false; miniTurbo = false; driftOutside = false;
  driftState = 0; driftStateCounter = 0; driftDuration = 0; unk23a = 0;
  hopVelocity = 0; hopAccel = 0; hopJerk = 0;
  kartGravity = GRAVITY; unkDac = 1;
  backUp = false; moveBackwards = false; leftTurn = false; rightTurn = false; turnFrames = 0;
  normal: [number, number, number] = [0, 1, 0];
  surfaceDistance = 0;
  grass = false;
  bumped = false;
  /** slopeAccel (s16): pitch of the floor under the tyres, positive downhill. */
  slopeAccel = 0;
  /** unk_206 (s16): roll of the floor across the tyres. */
  rollSlope = 0;
  /** Smoothed tyre heights: left, right, front, back (unk_230, unk_23C, unk_1FC, unk_1F8). */
  tyreHeights: [number, number, number, number] = [0, 0, 0, 0];
  tyresReady = false;
  /** Grass under each tyre: front left, front right, back left, back right. */
  tyreGrass: [boolean, boolean, boolean, boolean] = [false, false, false, false];
  /** unk_064: last frame's ground reaction / brake-hold force. */
  groundForce: [number, number, number] = [0, 0, 0];

  constructor(pos: [number, number, number], rotation: number) {
    this.pos = [...pos];
    this.oldPos = [...pos];
    this.rotation = rotation;
  }

  /** speed / 18 * 216 */
  kmh(): number { return this.speed / 18 * 216; }
  /** Heading in radians; forward is (sin y, cos y). */
  yaw(): number { return -this.rotation * Math.PI / 32768; }
  slipDegrees(): number { return trunc(this.unk0c0 / DEG); }

  private slopeDegrees(): number {
    // slopeAccel / DEGREES(1): positive downhill along the heading.
    return trunc(this.slopeAccel / DEG);
  }

  private kartHop(): void {
    this.hopJerk = HOP_JERK;
    this.hopAccel = 0;
    this.hopVelocity = HOP_VELOCITY;
    this.hop = true;
    this.unkDac = 3;
    this.kartGravity = 500;
    if (this.kmh() > 20) { this.driftDuration = 0; this.drifting = true; }
  }

  private cancelDrift(): void {
    if (!this.drifting) return;
    if (trunc(this.unk0c0 / DEG) > 0) this.steerPosition = (trunc(((this.steerPosition >> 16) * 13 + 13 * 53) / (2 * 53)) + 40) << 16;
    if (trunc(this.unk0c0 / DEG) < 0) this.steerPosition = (trunc(((this.steerPosition >> 16) * 13 + 13 * 53) / (2 * 53)) - 53) << 16;
    this.drifting = false;
  }

  static clampedStickX(rawX: number, rawY: number): number {
    let x = rawX, y = rawY, x2 = x, y2 = y;
    if (x > 12) { y2 = trunc(y * 12 / x); x2 = 12; }
    if (x2 < -12) { y2 = trunc(y2 * 12 / -x2); x2 = -12; }
    if (y2 > 12) { x2 = trunc(x2 * 12 / y2); y2 = 12; }
    if (y2 < -12) { x2 = trunc(x2 * 12 / -y2); y2 = -12; }
    if (rawX > -13 && rawX < 13 && rawY > -13 && rawY < 13) { x = 0; y = 0; } else { x -= x2; y -= y2; }
    if (x > 0x35) { y = trunc(y * 0x35 / x); x = 0x35; }
    if (x < -0x35) { y = trunc(y * 0x35 / -x); x = -0x35; }
    return x;
  }

  private steerInc(minimum: number): void {
    const before = this.steerChangeIncrement;
    this.steerChangeIncrement = (this.steerChangeIncrement - (1 << 11)) >>> 0;
    if (this.steerChangeIncrement >= 0xf0000000) this.steerChangeIncrement = before;
    if (minimum >= (this.steerChangeIncrement | 0)) this.steerChangeIncrement = minimum >>> 0;
  }

  private steering(input: KartInput): void {
    const kmh = this.kmh();
    if ((!this.hop && ((trunc(this.unk0c0 / DEG) <= 6 && trunc(this.unk0c0 / DEG) >= -6) || !input.hopDown)) || kmh <= 20) this.cancelDrift();
    if (trunc(this.unk0c0 / DEG) < -5) { this.leftTurn = true; this.rightTurn = false; this.turnFrames++; }
    else if (trunc(this.unk0c0 / DEG) > 5) { this.rightTurn = true; this.leftTurn = false; this.turnFrames++; }
    else { this.leftTurn = false; this.rightTurn = false; this.turnFrames = 0; }
    if (this.hop || this.drifting) { this.leftTurn = false; this.rightTurn = false; }
    let steerPosition = this.steerPosition;
    let clampedX = KartPhysics.clampedStickX(input.rawX, input.rawY);
    if (this.backUp || this.moveBackwards) clampedX = -clampedX;
    this.steerPosition = clampedX << 16;
    const delta = (steerPosition - this.steerPosition) >> 16;
    this.steerPositionDelta = s16(delta);
    const a0 = this.drifting && !this.hop ? 3 : 0;
    let large: number, small: number;
    if (kmh >= 15) {
      if (this.rightTurn || this.leftTurn) {
        const near = this.rightTurn ? delta <= 35 && delta >= 0 : delta >= -35 && delta <= 0;
        if (near) { large = a0 + 15; small = a0 + 15; } else { large = a0 + 5; small = a0 + 9; }
      } else { large = a0 + 3; small = a0 + 6; }
    } else { large = 8; small = 8; }
    for (const [thr, num, arg6] of LARGE) {
      if (delta >= thr || -thr >= delta) {
        this.steerInc(trunc((num << 12) / large));
        steerPosition = this.steerPosition < steerPosition ? (steerPosition - this.steerChangeIncrement) | 0 : (steerPosition + this.steerChangeIncrement) | 0;
        if (this.unk090 < arg6) this.unk090 = -arg6;
      }
    }
    for (const [thr, num, step] of SMALL) {
      if (delta >= thr || -thr >= delta) {
        this.steerInc(trunc((num << 12) / small));
        steerPosition = this.steerPosition < steerPosition ? (steerPosition - this.steerChangeIncrement) | 0 : (steerPosition + this.steerChangeIncrement) | 0;
        this.unk090 += step;
        if (this.unk090 >= 0) this.unk090 = 0;
      }
    }
    let sp = steerPosition >> 16;
    let f2: number;
    if (this.drifting) f2 = trunc(sp / 8);
    else if (kmh <= 25) f2 = trunc(sp / 12);
    else f2 = sp / (8 + this.currentSpeed / 50);
    f2 = Math.abs(f2);
    const table = SP44[clamp(trunc(kmh), 0, 155)];
    f2 *= this.drifting ? table : table * 1.5;
    this.steerPosition = steerPosition;
    this.driftOutside = false;
    const varF12 = 0;
    sp = steerPosition >> 16;
    if (!this.hop && !this.drifting) {
      const handling = sp >= 45 || sp <= -45 ? 0.15 + HANDLING : HANDLING;
      if (!this.braking) this.unk078 = s16(sp * (f2 + f2 * varF12) * handling);
      else if (kmh < 8) this.unk078 = s16(sp * (f2 + f2 * varF12));
      else if (kmh < 65) this.unk078 = s16(sp * ((f2 + 1.5) + f2 * varF12));
      else this.unk078 = s16(sp * ((f2 + 1.6) + f2 * varF12));
      this.driftStateCounter = 0;
      if (this.driftState < 2) this.driftState = 0;
    } else if (!this.midair && !this.hop) {
      let varS1: number;
      if (trunc(this.unk0c0 / DEG) > 0) {
        varS1 = trunc((sp * 13 + 13 * 53) / (2 * 53)) + 40;
        if (sp <= -40) this.driftOutside = true;
      } else {
        varS1 = trunc((sp * 13 + 13 * 53) / (2 * 53)) - 53;
        if (sp >= 40) this.driftOutside = true;
      }
      this.updateDriftStateCounter(sp);
      this.unk078 = kmh < 8 ? s16(varS1 * ((f2 + 2) + f2 * varF12)) : kmh < 65 ? s16(varS1 * ((f2 + 3) + f2 * varF12)) : s16(varS1 * ((f2 + 3.5) + f2 * varF12));
      this.unk078 = s16(this.unk078 * (this.driftOutside ? 0.9 : 0.65));
    } else {
      let v = sp;
      if (clampedX === 0) v = 0;
      this.unk078 = kmh <= 5 ? s16(v * (f2 + 6)) : s16(v * (f2 + 1.5));
    }
  }

  private updateDriftStateCounter(sp: number): void {
    const against = trunc(this.unk0c0 / DEG) > 0 ? sp <= -10 : sp >= 10;
    if (against) {
      if (this.driftStateCounter <= 100) this.driftStateCounter++;
    } else {
      if (this.driftStateCounter >= 18 && this.driftStateCounter < 100 && this.driftState < 3) this.driftState++;
      if (this.driftStateCounter >= 10 && this.driftStateCounter < 100) this.driftStateCounter = 10;
      else { this.driftStateCounter = 0; this.driftState = 0; }
    }
  }

  private decelerate(amount: number): void {
    this.currentSpeed -= amount;
    if (this.currentSpeed <= 0) this.currentSpeed = 0;
    if (this.speed < 0.2) this.unk08c = 0;
    if (TOP_SPEED <= this.currentSpeed) this.currentSpeed = TOP_SPEED;
    if (!this.midair) this.unk08c = this.currentSpeed * this.currentSpeed / 25;
  }

  private accelerate(): void {
    const slope = this.slopeDegrees();
    const tier = clamp(Math.floor(this.currentSpeed / (TOP_SPEED * 0.1)), 0, 9);
    if (this.currentSpeed >= 0 && this.currentSpeed <= TOP_SPEED) this.currentSpeed += ACCEL[tier] + 0.05 * slope;
    if (this.currentSpeed < 0) this.currentSpeed = 0;
    if (TOP_SPEED <= this.currentSpeed) this.currentSpeed = TOP_SPEED;
    if (!this.midair) this.unk08c = this.currentSpeed * this.currentSpeed / 25;
  }

  private brake(): void {
    this.braking = true;
    const v = this.grass ? GRASS_BRAKE * 2 : 0;
    if (this.previousSpeed - this.speed <= 0) this.unk20c = 0;
    else this.unk20c = Math.min(this.unk20c + 0.02, 2);
    if (this.kmh() <= 20) this.decelerate((1 - v) * 4);
    if (this.unk20c >= 2) this.decelerate((1 - v) * 2.5);
    else this.decelerate((1 - v) * 1.2);
  }

  private handleInput(input: KartInput): void {
    if (!this.midair && !this.hop && !this.drifting && input.hopPressed) this.kartHop();
    if (!this.midair || (this.hop && this.surfaceDistance <= 5)) this.steering(input);
    this.braking = false;
    if (!this.backUp) {
      if (input.accel) this.accelerate(); else this.decelerate(1);
      if (input.brake) this.brake(); else this.unk20c = 0;
    }
    if (input.rawY < -0x31 && this.kmh() <= 5 && input.brake) {
      this.currentSpeed = 140;
      this.backUp = true;
      this.unk08c = this.currentSpeed * this.currentSpeed / 25;
      this.unk20c = 0;
    }
    if ((input.rawY >= -0x1d || !input.brake) && this.backUp) {
      this.backUp = false;
      this.currentSpeed = 0;
    }
  }

  private miniTurboTick(): void {
    if (!this.miniTurbo && !this.drifting && this.driftState >= 2) {
      this.miniTurbo = true; this.unk23a = 0; this.driftState = 0; this.driftStateCounter = 0;
    } else if (this.miniTurbo) {
      this.unk23a++;
      if (this.unk23a >= 0x1f) { this.unk23a = 0; this.miniTurbo = false; this.driftState = 0; this.driftStateCounter = 0; }
    }
  }

  private gravityTick(): void {
    const slope = this.slopeDegrees();
    if (!this.midair && this.unk08c > 0) {
      if (slope < -1 && slope >= -0x14 && this.kmh() >= 20) {
        this.kartGravity = towards(this.kartGravity, 500, 1);
        this.unkDac = towards(this.unkDac, 3, 0.05);
      } else {
        this.kartGravity = towards(this.kartGravity, GRAVITY, 0.1);
        this.unkDac = towards(this.unkDac, 1, 0.07);
      }
    } else {
      if (this.surfaceDistance >= 50) this.unkDac = 2;
      this.kartGravity = towards(this.kartGravity, GRAVITY, 0.02);
      this.unkDac = towards(this.unkDac, 1, 0.07);
    }
  }

  private toWorld(v: [number, number, number], a: number): [number, number, number] {
    const s = sinA(a), c = cosA(a);
    return [v[0] * c - v[2] * s, v[1], v[0] * s + v[2] * c];
  }

  private lateral(): [number, number, number] {
    if (this.unk078 === 0) return [0, 0, 0];
    const m = this.unk078 < 0 ? 1 : -1;
    const kmh = this.kmh();
    let varF18: number, sp20: number;
    if (this.drifting && !this.hop) {
      varF18 = this.unk208 + (-kmh * 3) + (-this.unk20c * 50);
      sp20 = UNK_084 * 3;
    } else {
      let d = this.steerPositionDelta;
      if (d > 0) d = -d;
      const s = this.steerPosition >> 16;
      const base = this.unk208 + (-kmh / 3);
      if (s >= -20 && s <= 20) {
        varF18 = d < 20
          ? base + (-this.currentSpeed * 0.02) + (-this.unk20c * 50)
          : base + (m * s * 0.01 + (-this.currentSpeed * 0.05)) + (-this.unk20c * 50);
      } else varF18 = base + (m * s * 0.1 + (-this.currentSpeed * 0.15)) + (-this.unk20c * 50);
      sp20 = UNK_084;
    }
    return this.toWorld([m * (this.unk090 + varF18) * this.speed, 0, this.speed * sp20], this.rotation);
  }

  private thrust(): number {
    let v = 0;
    const kmh = this.kmh();
    if (kmh >= 8) {
      if (this.grass) v += GRASS_BACK * 2 + GRASS_FRONT + GRASS_BACK;
      if (!this.midair) {
        const t = this.slopeDegrees();
        if (kmh >= 20) v -= (t > 17 || t < -17) ? t * 0.0126 / 3 : t * 0.026 / 3;
        else v += -0.2;
        const slip = Math.abs(trunc(this.unk0c0 / DEG));
        v += this.drifting || this.driftDuration > 0 ? slip * 0.004 : slip * 0.01;
        if (this.driftOutside && this.driftStateCounter < 10) v += slip * 0.008;
      }
      this.unk0a0 = this.midair ? towards(this.unk0a0, this.unk08c * 0.04, 0.15) : towards(this.unk0a0, 0, 0.1);
    } else {
      this.unk0a0 = 0;
      this.unk0e8 = 0;
      if (this.slopeDegrees() < 0) v += -0.85;
    }
    this.unk0e8 = this.miniTurbo && !this.drifting ? towards(this.unk0e8, 580, 0.2) : towards(this.unk0e8, 0, 0.01);
    this.unk104 = towards(this.unk104, v, 0.05);
    const total = Math.max(0, this.unk08c + this.unk0e8 - this.unk0a0);
    return (1 - this.unk104) * total;
  }

  private sideGripTick(): void {
    let v = 0;
    if (this.grass) v += GRASS_SIDE_BACK * 2 + GRASS_SIDE_FRONT * 2;
    if (this.currentSpeed >= 200 && (this.turnFrames & 4) !== 0) {
      const t = trunc(this.unk0c0 / DEG);
      if (t > 0xf || t < -0xf) v += 1;
    }
    this.unk208 = UNK_088 - (UNK_088 >= 0 ? UNK_088 * v : -UNK_088 * v);
  }

  private slipTick(old: [number, number, number], next: [number, number, number]): void {
    const sp28 = sinA(-this.rotation) * this.speed + old[0];
    const t16 = cosA(-this.rotation) * this.speed + old[2];
    const prev = this.unk0c0;
    const sp = this.steerPosition >> 16;
    if (!this.drifting && (this.kmh() <= 8 || (sp < 5 && sp > -5))) {
      this.unk0c0 = this.unk078 * 9;
      const t = s16(this.unk0c0 - prev);
      this.unk0c0 = prev + trunc(t / 15);
    } else {
      const var1 = (this.turnFrames & 8) !== 0 ? 2 : 0;
      const diff = s16(atan2s(old[0] - next[0], old[2] - next[2]) - atan2s(old[0] - sp28, old[2] - t16));
      const slipDeg = trunc(this.unk0c0 / DEG);
      this.unk0c0 = this.currentSpeed >= 200 && var1 === 2 && (slipDeg >= 0x10 || slipDeg < -0xf) ? diff : diff * 2;
      if (!this.drifting && ((sp > 0 && this.unk0c0 < 0) || (sp < 0 && this.unk0c0 > 0))) {
        this.unk0c0 = this.unk078 * 0x14;
      } else {
        this.unk0c0 = clamp(this.unk0c0, -0x1c70, 0x1c70);
      }
      const t = s16(this.unk0c0 - prev);
      this.unk0c0 = prev + trunc(t / 12);
    }
  }

  /** func_80029B4C: floor height under each tyre -> slopeAccel and unk_206. Uses the position the kart had at the
   * start of this update, and nextY for the airborne case. */
  private sampleTyres(ground: KartGround, nextY: number): void {
    const r = TYRE_RADIUS;
    // front left, front right, back left, back right (x, z) in kart space.
    const offsets = [[r - 3.6, r - 2], [-r + 3.6, r - 2], [r - 2.6, -r + 4], [-r + 2.6, -r + 4]];
    const angle = this.rotation + this.unk0c0;
    const base = [0, 0, 0, 0];
    offsets.forEach(([x, z], i) => {
      const w = this.toWorld([x, 0, z], angle);
      const f = ground.floor(this.pos[0] + w[0], this.pos[1] + BOUNDING_BOX + 1, this.pos[2] + w[2]);
      // A tyre touches when the floor is within one bounding box of the kart origin.
      if (f && f.height <= this.pos[1] + BOUNDING_BOX && f.height >= this.pos[1] - BOUNDING_BOX) {
        this.tyreGrass[i] = f.grass;
        base[i] = f.height;
      } else base[i] = this.pos[1];
    });
    if (!this.midair) {
      const target: [number, number, number, number] = [(base[2] + base[0]) / 2, (base[3] + base[1]) / 2, (base[1] + base[0]) / 2, (base[3] + base[2]) / 2];
      if (this.tyresReady) this.tyreHeights = this.tyreHeights.map((h, i) => towards(h, target[i], 0.5)) as typeof this.tyreHeights;
      else { this.tyreHeights = target; this.tyresReady = true; }
    }
    const [left, right, front, back] = this.tyreHeights;
    this.rollSlope = this.midair ? 0 : -atan2s((right - left) / WHEEL_BASE, 1);
    if (!this.midair) this.slopeAccel = towardsS16(this.slopeAccel, atan2s((back - front) / WHEEL_BASE, 1), 0.5);
    else {
      const rise = (this.pos[1] - nextY) / WHEEL_BASE;
      const a = atan2s(rise, 1);
      this.slopeAccel = towardsS16(this.slopeAccel, rise >= 0 ? trunc(a / 4) : s16(a * 10), 0.5);
    }
  }

  /** One 60 Hz update: input handling then func_8002D268. */
  step(input: KartInput, ground: KartGround): void {
    this.bumped = false;
    this.previousSpeed = this.speed;
    this.handleInput(input);
    if (this.drifting) this.driftDuration = Math.min(this.driftDuration + 1, 100);
    else this.driftDuration = Math.max(this.driftDuration - 1, 0);
    this.miniTurboTick();
    if (this.driftOutside && this.driftStateCounter >= 100) this.decelerate(4);
    const sp7c = this.driftOutside && this.driftStateCounter >= 100 ? 2 : 0;
    let lateral: [number, number, number] = [0, 0, 0];
    if (this.unk078 !== 0) {
      if (!this.driftOutside || this.driftStateCounter >= 100) this.rotation = s16(this.rotation + this.unk078);
      lateral = this.lateral();
    }
    this.gravityTick();
    this.sideGripTick();
    // unk_064 (last frame's ground reaction) is flipped while reversing.
    if (this.backUp) { this.groundForce[0] = -this.groundForce[0]; this.groundForce[2] = -this.groundForce[2]; }
    const kmh = this.kmh();
    let gx: number, gy: number, gz: number;
    if (this.midair) { gx = 0; gy = -this.kartGravity; gz = 0; }
    else {
      const n = this.normal;
      // Both back tyres on asphalt -> 0.925, otherwise the speed-indexed table (sp104).
      const slopeFactor = !this.tyreGrass[2] && !this.tyreGrass[3] ? 0.925 : NON_ASPHALT_GRAVITY[Math.min(trunc(kmh / 10), 10)];
      gx = -this.groundForce[0] + (-n[0] * this.kartGravity * slopeFactor);
      gy = -n[1] * this.kartGravity;
      gz = -this.groundForce[2] + (-n[2] * this.kartGravity * slopeFactor);
    }
    // MOVE_BACKWARDS from last frame's displacement; also the brake-hold force (sp178[2]).
    const b = this.rotation + this.unk0c0;
    const movedBack = (this.oldPos[2] - this.pos[2]) * cosA(b) + (-(this.oldPos[0] - this.pos[0])) * sinA(b);
    this.moveBackwards = movedBack > 0.1;
    const heading = this.backUp ? s16(this.rotation + 32768) : this.rotation;
    let sp178 = this.toWorld([0, 0, this.unk08c <= 0 && this.braking ? movedBack * 4500 : 0], heading);
    gx += sp178[0];
    gz += sp178[2];
    const thrust = this.thrust();
    const t = this.toWorld([0, 0, thrust], heading);
    const friction = 0.12 * FRICTION;
    const v: [number, number, number] = [...this.velocity];
    const divisor = Math.abs(this.steerPosition >> 16) >= 40 ? this.unk20c * 0.6 + 1 + sp7c : sp7c + 1;
    v[0] += ((t[0] + gx + lateral[0]) - v[0] * friction) / 6000 / divisor;
    v[2] += ((t[2] + gz + lateral[2]) - v[2] * friction) / 6000 / divisor;
    v[1] += ((t[1] + gy + lateral[1]) - v[1] * friction) / 6000 / this.unkDac;
    const old: [number, number, number] = [...this.pos];
    this.oldPos = [...old];
    const next: [number, number, number] = [old[0] + this.velocity[0], old[1] + this.velocity[1], old[2] + this.velocity[2]];
    this.hopAccel = clamp(this.hopAccel - this.hopJerk, -9, 9);
    this.hopVelocity = Math.min(this.hopVelocity + this.hopAccel, 15);
    if (this.hopVelocity <= 0) { this.hopJerk = 0; this.hopAccel = 0; this.hopVelocity = 0; }
    next[1] += this.hopVelocity - 0.02;
    // actor_terrain_collision: surfaceDistance[2] is the signed distance to the floor plane minus the bounding box;
    // the floor only registers within 16 units below the plane.
    const pre: [number, number, number] = [...next];
    this.midair = true;
    this.surfaceDistance = 1000;
    const floor = ground.floor(next[0], next[1] + 20, next[2]);
    if (floor) {
      const d = floor.normal[1] * (next[1] + BOUNDING_BOX - floor.height) - BOUNDING_BOX;
      if (d > -16) { this.surfaceDistance = d; this.normal = floor.normal; this.grass = floor.grass; }
    }
    const walls = ground.wall(pre);
    const distance = this.surfaceDistance;
    if (distance <= 0) {
      this.hop = false;
      this.midair = false;
      this.hopVelocity = 0;
      // func_8003F46C -> func_8003E9EC: push out of the floor, bounce or slide, and the slope reaction.
      const n = this.normal;
      next[0] += n[0] * -distance;
      next[1] += n[1] * -distance * 0.1;
      next[2] += n[2] * -distance;
      reflect(n, distance, v, 1.2, 2);
      const slope = this.slopeDegrees();
      const roll = trunc(this.rollSlope / DEG);
      let local: [number, number, number];
      if (n[1] <= 0.8357) {
        this.decelerate(4);
        this.unkDac = 1;
        local = [roll * 120, 0, -slope * 180];
      } else if (this.kmh() > 20) {
        const asphalt = this.tyreGrass.some(g => !g);
        local = [roll * (asphalt ? 5 : 40), 0, -slope * 40];
      } else local = [0, 0, -slope * 50];
      sp178 = this.toWorld(local, heading);
    }
    // Walls: Z-facing first (surfaceDistance[0], func_8003F734), then X-facing (func_8003FBAC), each followed by
    // func_8002C954's scrub.
    if (walls) {
      const dx = walls.pos[0] - pre[0], dz = walls.pos[2] - pre[2];
      next[0] += dx;
      next[2] += dz;
      for (const xFacing of [false, true]) {
        for (const hit of walls.hits) {
          if (hit.xFacing !== xFacing) continue;
          const n = hit.normal;
          const depth = Math.max(0, n[0] * dx + n[2] * dz);
          const vertical = Math.abs(n[1]) < 0.01;
          const restitution = vertical
            ? (this.slopeAccel < 0 && this.kmh() < 10 ? (xFacing ? 1.5 : 2.5) : 0.5)
            : n[1] <= 0.5 ? 1 : 1.2;
          if (!vertical && n[1] <= 0.5) next[1] += n[1] * depth * 0.1;
          reflect(n, -Math.max(depth, 1e-9), v, restitution, 0);
          if (!vertical && !this.midair) v[1] *= -1e-5;
          this.drifting = false;
          this.bumped = true;
          if (this.slopeAccel >= 0 && this.kmh() > 5) this.decelerate(18);
        }
      }
    }
    this.sampleTyres(ground, next[1]);
    this.slipTick(old, next);
    this.speed = Math.sqrt(v[0] * v[0] + v[2] * v[2]);
    if (this.unk08c <= 0 && this.speed <= 0.08) { v[0] = 0; v[2] = 0; }
    else { this.pos[0] = next[0]; this.pos[2] = next[2]; }
    this.pos[1] = next[1];
    this.groundForce = sp178;
    this.velocity = v;
    if (SPEED_CAP < this.speed) {
      const k = SPEED_CAP / this.speed;
      this.velocity = [v[0] * k, v[1] * k, v[2] * k];
      this.speed = SPEED_CAP;
    }
    if (this.backUp && this.speed > 1) {
      const k = 1 / this.speed;
      this.velocity = [this.velocity[0] * k, this.velocity[1] * k, this.velocity[2] * k];
      this.speed = 1;
    }
  }
}
