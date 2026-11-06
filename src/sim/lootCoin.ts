import { atan2s, clamp, coss, sins, toS16, type Vec3 } from './math';
import type { CollisionWorld } from './collision';
import { SURFACE_BURNING, SURFACE_DEATH_PLANE } from './surfaces';

function applyDrag(speed: number): number {
  const slowed = Math.abs(speed)-speed*speed*0.001;
  return slowed < 0.001 ? 0 : Math.sign(speed)*slowed;
}

/** bhvSingleCoinGetsSpawned, used by wooden posts, large boxes and ordinary Goombas. */
export class LootCoin {
  private yaw = Math.floor(Math.random()*65536);
  private speed = Math.random()*10;
  private verticalSpeed = 50+Math.random()*10;
  private bounce = 0.7;
  private grounded = false;
  private age = 0;
  tangible = false;
  visible = true;

  step(pos: Vec3, world: CollisionWorld): boolean {
    const wall = world.resolveAndReturnWallCollisions(pos,10,30);
    if (wall) this.yaw = atan2s(wall.normal[2],wall.normal[0]);
    const floor = world.findFloor(...pos);
    const vx = applyDrag(this.speed*sins(this.yaw)), vz = applyDrag(this.speed*coss(this.yaw));
    this.speed = Math.hypot(vx,vz);
    const next = world.findFloor(pos[0]+vx,pos[1],pos[2]+vz);
    if (next.surface && !(this.grounded && next.height < floor.height-50)
        && (next.surface.normal[1] > 0.46947 || (next.height >= floor.height+5 && pos[1] > next.height))) {
      pos[0] += vx;
      pos[2] += vz;
    }
    this.verticalSpeed = Math.max(-78,this.verticalSpeed-4);
    pos[1] += this.verticalSpeed;
    if (this.verticalSpeed < 0) this.tangible = true;
    const surface = floor.surface;
    const onFloor = surface !== null && pos[1] <= floor.height;
    if (onFloor) {
      if (surface.type === SURFACE_BURNING || surface.type === SURFACE_DEATH_PLANE) return false;
      pos[1] = floor.height;
      if (this.verticalSpeed < 0) this.verticalSpeed *= -this.bounce;
      if (this.grounded) this.bounce = 0;
      const normal = surface.normal;
      if (normal[1]<0.9) this.yaw = toS16(this.yaw+clamp(toS16(atan2s(normal[2],normal[0])-this.yaw),-0x400,0x400));
    }
    this.grounded = onFloor;
    this.visible = this.age < 400 || this.age%2 === 0;
    return this.age++ < 443 && pos[1] > -11000;
  }
}
