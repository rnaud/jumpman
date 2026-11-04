/**
 * Math helpers matching Super Mario 64's conventions.
 *
 * SM64 angles are signed 16-bit values ("s16") where a full turn is 65536.
 * Yaw 0 points down +Z, yaw 0x4000 points down +X, and positive yaw rotates
 * clockwise when looking down from +Y.  `sins` is the X component and `coss`
 * is the Z component.
 *
 * The decompilation's `atan2s(y, x)` is a little surprising: it computes
 * `atan2(x, y)` in the usual maths sense.  That is why `atan2s(normal.z,
 * normal.x)` yields a yaw, and `atan2s(-stickY, stickX)` yields a camera-relative
 * stick direction.
 */

export type Vec3 = [number, number, number];

export const TAU = Math.PI * 2;
export const ANGLE_TO_RAD = Math.PI / 32768;
export const RAD_TO_ANGLE = 32768 / Math.PI;

/** Wrap an integer into the signed 16-bit range, like a C `s16` cast. */
export function toS16(value: number): number {
  const v = Math.trunc(value) & 0xffff;
  return v >= 0x8000 ? v - 0x10000 : v;
}

export function toU16(value: number): number {
  return Math.trunc(value) & 0xffff;
}

export function sins(angle: number): number {
  return Math.sin(toU16(angle) * ANGLE_TO_RAD);
}

export function coss(angle: number): number {
  return Math.cos(toU16(angle) * ANGLE_TO_RAD);
}

/**
 * SM64's `atan2s(a, b)` returns the angle whose sine is `b` and cosine is `a`,
 * i.e. standard `atan2(b, a)`.
 */
export function atan2s(a: number, b: number): number {
  return toS16(Math.round(Math.atan2(b, a) * RAD_TO_ANGLE));
}

/** Convert an s16 angle to radians for Three.js. */
export function angleToRadians(angle: number): number {
  return toS16(angle) * ANGLE_TO_RAD;
}

/** Convert radians to an s16 angle. */
export function radiansToAngle(radians: number): number {
  return toS16(Math.round(radians * RAD_TO_ANGLE));
}

export function approachS32(current: number, target: number, inc: number, dec: number): number {
  if (current < target) {
    current += inc;
    if (current > target) current = target;
  } else {
    current -= dec;
    if (current < target) current = target;
  }
  return current;
}

export function approachF32(current: number, target: number, inc: number, dec: number): number {
  if (current < target) {
    current += inc;
    if (current > target) current = target;
  } else {
    current -= dec;
    if (current < target) current = target;
  }
  return current;
}

export function clamp(value: number, min: number, max: number): number {
  return value < min ? min : value > max ? max : value;
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

export function vec3(x = 0, y = 0, z = 0): Vec3 {
  return [x, y, z];
}

export function copyVec3(dest: Vec3, src: readonly number[]): Vec3 {
  dest[0] = src[0];
  dest[1] = src[1];
  dest[2] = src[2];
  return dest;
}

export function setVec3(dest: Vec3, x: number, y: number, z: number): Vec3 {
  dest[0] = x;
  dest[1] = y;
  dest[2] = z;
  return dest;
}

export function dist2D(x1: number, z1: number, x2: number, z2: number): number {
  const dx = x2 - x1;
  const dz = z2 - z1;
  return Math.sqrt(dx * dx + dz * dz);
}

export function dist3D(a: readonly number[], b: readonly number[]): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const dz = b[2] - a[2];
  return Math.sqrt(dx * dx + dy * dy + dz * dz);
}

export function degToAngle(deg: number): number {
  return toS16(Math.round((deg / 360) * 65536));
}

/** Shortest signed distance from angle `b` to angle `a`. */
export function angleDelta(a: number, b: number): number {
  return toS16(a - b);
}

/** Linear interpolation between two s16 angles along the shortest path. */
export function angleLerp(a: number, b: number, t: number): number {
  return toS16(a + angleDelta(b, a) * t);
}

export function wrap01(t: number): number {
  return t - Math.floor(t);
}
