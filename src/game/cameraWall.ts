import type { Vector3 } from 'three';
import type { CollisionWorld, Surface, WallCollisionData } from '../sim/collision';
import { atan2s, clamp, coss, sins, toS16 } from '../sim/math';

function inFront(x: number, y: number, z: number, wall: Surface) {
  return (x-wall.v1[0])*wall.normal[0]+(y-wall.v1[1])*wall.normal[1]+(z-wall.v1[2])*wall.normal[2] > 0;
}

/** Obstructing-wall branch of rotate_camera_around_walls. Radial mode ignores near-only hits. */
export function radialWallYaw(mario: Vector3, eye: Vector3, world: CollisionWorld): number | undefined {
  const dx = eye.x-mario.x, dy = eye.y-mario.y, dz = eye.z-mario.z;
  const yaw = atan2s(dz,dx);
  const flat = Math.hypot(dx,dz), distance = Math.hypot(flat,dy);
  const pitch = atan2s(flat,dy);
  const reach = distance*coss(pitch);
  const y = mario.y+distance*sins(pitch);
  const range = 0x400;
  const data: WallCollisionData = {x:0,y:0,z:0,offsetY:100,radius:150,walls:[],numWalls:0};
  let fineRadius = 100;
  for (let step=0; step<8; step++) {
    const t = step/8;
    const x = mario.x+dx*t, z = mario.z+dz*t;
    data.x = x;
    data.y = mario.y+dy*t;
    data.z = z;
    data.radius = Math.min(250,150+30*step);
    if (!world.findWallCollisions(data,true)) continue;
    data.x = x;
    data.z = z;
    data.radius = fineRadius;
    fineRadius = Math.min(fineRadius+20,200);
    if (!world.findWallCollisions(data,true)) continue;
    const wall = data.walls[data.numWalls-1];
    if (wall.type === 0x28 || !inFront(mario.x,mario.y,mario.z,wall)) continue;
    const heights = [wall.v1[1],wall.v2[1],wall.v3[1]];
    if (Math.max(...heights)-Math.min(...heights)<150) continue;
    if (inFront(mario.x+reach*sins(yaw+range),y,mario.z+reach*coss(yaw+range),wall) &&
        inFront(mario.x+reach*sins(yaw-range),y,mario.z+reach*coss(yaw-range),wall)) continue;
    const normal = atan2s(wall.normal[2],wall.normal[0]);
    const tangent = normal+0x4000;
    const avoid = toS16(tangent+(toS16(tangent-yaw+0x4000)<0 ? 0x8000 : 0));
    return toS16(avoid+clamp(toS16(normal-avoid),-range,range));
  }
}
