import { coss, sins, type Vec3 } from '../sim/math';
import type { CollisionWorld } from '../sim/collision';

export interface CoinFormation {
  shape: 'line_horizontal' | 'line_vertical' | 'ring_horizontal' | 'ring_vertical' | 'arrow';
  pos: Vec3;
  yaw: number;
  flying: boolean;
}

const arrow = [[0, -150], [0, -50], [0, 50], [0, 150], [-50, 100], [-100, 50], [50, 100], [100, 50]];

// spawn_coin_in_formation and bhv_coin_formation_spawn_loop in coin.inc.c.
export function formationCoins(formation: CoinFormation, world: CollisionWorld): Vec3[] {
  const { shape, pos, yaw, flying } = formation;
  const positions: Vec3[] = [];
  const count = shape.startsWith('line_') ? 5 : 8;
  for (let i = 0; i < count; i++) {
    let x = 0, y = 0, z = 0;
    switch (shape) {
      case 'line_horizontal': z = 160 * (i - 2); break;
      case 'line_vertical': y = 128 * i; break;
      case 'ring_horizontal':
        x = Math.trunc(sins(i << 13) * 300);
        z = Math.trunc(coss(i << 13) * 300);
        break;
      case 'ring_vertical':
        x = Math.trunc(coss(i << 13) * 200);
        y = Math.trunc(sins(i << 13) * 200 + 200);
        break;
      case 'arrow': [x, z] = arrow[i]; break;
    }
    const px = pos[0] + coss(yaw) * x + sins(yaw) * z;
    const pz = pos[2] - sins(yaw) * x + coss(yaw) * z;
    let py = pos[1] + y;
    if (!flying && !shape.endsWith('_vertical')) {
      const floor = world.findFloor(px, py + 300, pz);
      if (!floor.surface || floor.height > py + 300 || floor.height < -10000) continue;
      py = floor.height;
    }
    positions.push([px, py, pz]);
  }
  return positions;
}
