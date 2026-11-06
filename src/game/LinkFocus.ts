import * as THREE from 'three';
import { atan2s, coss, sins, type Vec3 } from '../sim/math';
import { INTERACT_DAMAGE, INTERACT_KOOPA, INTERACT_MR_BLIZZARD, type SimObject } from '../sim/object';
import type { GoombaEnemy } from './goomba';
import type { PlaygroundResult } from './world';

type Enemy = GoombaEnemy | SimObject;

export class LinkFocus {
  readonly marker = new THREE.Mesh(
    new THREE.ConeGeometry(14, 24, 4),
    new THREE.MeshBasicMaterial({ color: 0xffdc49 }),
  );
  private target?: Enemy;
  private readonly point = new THREE.Vector3();

  constructor() {
    this.marker.rotation.z = Math.PI;
    this.clear();
  }

  clear(): void {
    this.target = undefined;
    this.marker.visible = false;
  }

  /** Prefer opponents ahead of Link. Keep the chosen opponent until it leaves range. */
  acquire(world: PlaygroundResult, pos: Vec3, yaw: number): number {
    let best: Enemy | undefined;
    let bestScore = Infinity;
    const consider = (enemy: Enemy, x: number, y: number, z: number) => {
      const dx = x - pos[0], dz = z - pos[2];
      const distance = Math.hypot(dx, y - pos[1], dz);
      const retained = this.target === enemy;
      if (distance > (retained ? 1800 : 1500)) return;
      const ahead = (dx * sins(yaw) + dz * coss(yaw)) / Math.max(1, Math.hypot(dx,dz));
      if (!retained && ahead < -0.35 && distance > 250) return;
      const score = retained ? -1 : distance * (1.5 - 0.5 * ahead);
      if (score < bestScore) {
        bestScore = score;
        best = enemy;
      }
    };
    for (const enemy of world.goombas ?? []) {
      if (enemy.tangible) {
        const {x,y,z} = enemy.model.position;
        consider(enemy,x,y,z);
      }
    }
    for (const obj of world.objects?.manager.objects ?? []) {
      if (obj.active && !obj.heldState && (obj.behavior === 'bobomb' ||
          obj.hitbox.interactType & (INTERACT_DAMAGE | INTERACT_KOOPA | INTERACT_MR_BLIZZARD))) {
        consider(obj,...obj.pos);
      }
    }
    this.target = best;
    const point = this.updateMarker();
    return point ? atan2s(point.z-pos[2],point.x-pos[0]) : yaw;
  }

  updateMarker(): THREE.Vector3 | undefined {
    const target = this.target;
    if (!target || ('tangible' in target ? !target.tangible : !target.active)) {
      this.clear();
      return;
    }
    if ('tangible' in target) {
      this.point.copy(target.model.position);
      this.point.y = target.top;
    } else {
      this.point.fromArray(target.pos);
      this.point.y += target.hitbox.height - target.hitbox.downOffset;
    }
    this.marker.position.copy(this.point);
    this.marker.position.y += 32;
    this.marker.visible = true;
    return this.point;
  }
}
