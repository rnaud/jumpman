import * as THREE from 'three';
import type { CollisionWorld, Surface, SurfaceOptions } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import grassTop from '../../../assets/extracted/textures/generic/bob_textures.05800.rgba16.png?url';
import grassEdge from '../../../assets/extracted/textures/grass/wf_textures.05000.rgba16.png?url';
import dirt from '../../../assets/extracted/textures/grass/wf_textures.04800.rgba16.png?url';
import stone from '../../../assets/extracted/textures/generic/bob_textures.05000.rgba16.png?url';
import flagstone from '../../../assets/extracted/textures/grass/wf_textures.08000.rgba16.png?url';
import sandstone from '../../../assets/extracted/textures/grass/wf_textures.07000.rgba16.png?url';
import brick from '../../../assets/extracted/textures/grass/wf_textures.09000.rgba16.png?url';
import planks from '../../../assets/extracted/textures/generic/bob_textures.06000.rgba16.png?url';
import gold from '../../../assets/extracted/textures/generic/bob_textures.0A800.rgba16.png?url';
import checker from '../../../assets/extracted/textures/sky/rr_textures.08000.rgba16.png?url';
import tiles from '../../../assets/extracted/textures/sky/rr_textures.03000.rgba16.png?url';
import whiteBrick from '../../../assets/extracted/textures/sky/rr_textures.01000.rgba16.png?url';
import rainbow from '../../../assets/extracted/textures/sky/rr_textures.05800.rgba16.png?url';

/** Real SM64 course textures used by generated geometry. */
export const TEXTURES = { grassTop, grassEdge, dirt, stone, flagstone, sandstone, brick, planks, gold, checker, tiles, whiteBrick, rainbow } as const;
export type TextureName = keyof typeof TEXTURES;

const textureCache = new Map<TextureName, THREE.Texture>();
const loader = typeof document !== 'undefined' ? new THREE.TextureLoader() : null;

/** A nearest-filtered, world-tiling SM64 texture (shared; never dispose). */
export function sm64Texture(name: TextureName): THREE.Texture | null {
  if (!loader) return null;
  let texture = textureCache.get(name);
  if (!texture) {
    texture = loader.load(TEXTURES[name]);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.magFilter = THREE.NearestFilter;
    texture.minFilter = THREE.LinearMipmapLinearFilter;
    texture.anisotropy = 4;
    textureCache.set(name, texture);
  }
  return texture;
}

/**
 * Add a planar convex polygon as collision triangles whose normal faces
 * `outward`, regardless of the vertex order given.
 */
export function addFace(world: CollisionWorld, points: readonly Vec3[], outward: readonly [number, number, number], into: Set<Surface>, options: SurfaceOptions = {}): void {
  if (points.length < 3) return;
  const [a, b, c] = points;
  // Same convention as CollisionWorld.addTriangle: n = (v2 - v1) x (v3 - v2).
  const nx = (b[1] - a[1]) * (c[2] - b[2]) - (b[2] - a[2]) * (c[1] - b[1]);
  const ny = (b[2] - a[2]) * (c[0] - b[0]) - (b[0] - a[0]) * (c[2] - b[2]);
  const nz = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
  const flip = nx * outward[0] + ny * outward[1] + nz * outward[2] < 0;
  const ordered = flip ? [...points].reverse() : points;
  for (let i = 1; i < ordered.length - 1; i++) {
    const surface = world.addTriangle(ordered[0], ordered[i], ordered[i + 1], options);
    if (surface) into.add(surface);
  }
}

/** Collision for a box rotated by `yaw` (radians) about Y. */
export function addBoxCollision(world: CollisionWorld, center: THREE.Vector3, size: THREE.Vector3, yaw: number, into: Set<Surface>, options: SurfaceOptions = {}): void {
  const c = Math.cos(yaw), s = Math.sin(yaw);
  const hx = size.x / 2, hy = size.y / 2, hz = size.z / 2;
  const corner = (x: number, y: number, z: number): Vec3 => [center.x + x * c + z * s, center.y + y, center.z - x * s + z * c];
  const top = [corner(-hx, hy, -hz), corner(hx, hy, -hz), corner(hx, hy, hz), corner(-hx, hy, hz)];
  const bottom = [corner(-hx, -hy, -hz), corner(hx, -hy, -hz), corner(hx, -hy, hz), corner(-hx, -hy, hz)];
  addFace(world, top, [0, 1, 0], into, options);
  addFace(world, bottom, [0, -1, 0], into, options);
  const sides: Array<[number, number, [number, number, number]]> = [
    [0, 1, [-s, 0, -c]], [1, 2, [c, 0, -s]], [2, 3, [s, 0, c]], [3, 0, [-c, 0, s]],
  ];
  for (const [i, j, normal] of sides) addFace(world, [top[i], top[j], bottom[j], bottom[i]], normal, into, options);
}

/** Collision for a wedge rising toward local -Z, rotated by `yaw`. */
export function addRampCollision(world: CollisionWorld, center: THREE.Vector3, size: THREE.Vector3, yaw: number, into: Set<Surface>): void {
  const c = Math.cos(yaw), s = Math.sin(yaw);
  const hx = size.x / 2, hy = size.y / 2, hz = size.z / 2;
  const corner = (x: number, y: number, z: number): Vec3 => [center.x + x * c + z * s, center.y + y, center.z - x * s + z * c];
  const frontL = corner(-hx, -hy, hz), frontR = corner(hx, -hy, hz);
  const backL = corner(-hx, -hy, -hz), backR = corner(hx, -hy, -hz);
  const topL = corner(-hx, hy, -hz), topR = corner(hx, hy, -hz);
  const slopeNormal = new THREE.Vector3(0, size.z, size.y).normalize();
  const n: [number, number, number] = [slopeNormal.z * s, slopeNormal.y, slopeNormal.z * c];
  addFace(world, [frontL, frontR, topR, topL], n, into);
  addFace(world, [frontL, frontR, backR, backL], [0, -1, 0], into);
  addFace(world, [backL, backR, topR, topL], [-s, 0, -c], into);
  addFace(world, [frontL, backL, topL], [-c, 0, s], into);
  addFace(world, [frontR, backR, topR], [c, 0, -s], into);
}

/** Box geometry whose UVs tile the texture every `tile` world units. */
export function tiledBox(width: number, height: number, depth: number, tile = 256): THREE.BoxGeometry {
  const geometry = new THREE.BoxGeometry(width, height, depth);
  const uv = geometry.getAttribute('uv') as THREE.BufferAttribute;
  const normal = geometry.getAttribute('normal') as THREE.BufferAttribute;
  for (let i = 0; i < uv.count; i++) {
    const nx = Math.abs(normal.getX(i)), ny = Math.abs(normal.getY(i));
    const [u, v] = nx > 0.5 ? [depth, height] : ny > 0.5 ? [width, depth] : [width, height];
    uv.setXY(i, uv.getX(i) * u / tile, uv.getY(i) * v / tile);
  }
  return geometry;
}

/** Wedge geometry matching addRampCollision (rises toward -Z). */
export function rampGeometry(width: number, height: number, depth: number, tile = 256): THREE.BufferGeometry {
  const hx = width / 2, hy = height / 2, hz = depth / 2;
  const fl = [-hx, -hy, hz], fr = [hx, -hy, hz], bl = [-hx, -hy, -hz], br = [hx, -hy, -hz], tl = [-hx, hy, -hz], tr = [hx, hy, -hz];
  const slope = Math.hypot(height, depth) / tile;
  const positions: number[] = [];
  const uvs: number[] = [];
  const quad = (a: number[], b: number[], c: number[], d: number[], uw: number, vh: number) => {
    positions.push(...a, ...b, ...c, ...a, ...c, ...d);
    uvs.push(0, 0, uw, 0, uw, vh, 0, 0, uw, vh, 0, vh);
  };
  quad(fl, fr, tr, tl, width / tile, slope);
  quad(bl, br, fr, fl, width / tile, depth / tile);
  quad(br, bl, tl, tr, width / tile, height / tile);
  positions.push(...fl, ...tl, ...bl, ...fr, ...br, ...tr);
  uvs.push(0, 0, depth / tile, height / tile, depth / tile, 0, 0, 0, depth / tile, 0, depth / tile, height / tile);
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute('uv', new THREE.Float32BufferAttribute(uvs, 2));
  geometry.computeVertexNormals();
  return geometry;
}
