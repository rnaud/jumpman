import { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';
import { SURFACE_FLAG_NO_CAM_COLLISION } from '../sim/surfaces';
export interface CollisionData {
  vertices: Vec3[];
  triangles: [number, number, number, number, number][];
}
export function addNativeCollision(world: CollisionWorld, data: CollisionData, pos: Vec3 = [0, 0, 0], yaw = 0): void {
  const angle = yaw * Math.PI / 180;
  const c = Math.cos(angle), s = Math.sin(angle);
  const vertices: Vec3[] = data.vertices.map(([x, y, z]) => [c * x + s * z + pos[0], y + pos[1], -s * x + c * z + pos[2]]);
  for (const [a, b, c, type, force] of data.triangles) {
    if (!vertices[a] || !vertices[b] || !vertices[c]) throw new Error('Invalid native collision index');
    world.addTriangle(vertices[a], vertices[b], vertices[c], {
      type, force, flags: type >= 0x76 && type <= 0x79 ? SURFACE_FLAG_NO_CAM_COLLISION : 0,
    });
  }
}
