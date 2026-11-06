import { OBJ_COL_FLAG_NO_Y_VEL, type ObjectContext, type SimObject } from '../object';
import { SURFACE_BURNING, SURFACE_DEATH_PLANE } from '../surfaces';

export function spawnMovingYellowCoins(source: SimObject, ctx: ObjectContext, count: number): void {
  for (let i=0;i<count;i++) {
    ctx.spawn({behavior:'movingYellowCoin',model:'actors/coin/yellow_coin_geo.glb',pos:source.pos,yaw:0});
  }
}

// obj_spawn_yellow_coins and bhv_moving_yellow_coin_loop.
export function initMovingYellowCoin(o: SimObject): void {
  o.forwardVel = Math.random()*20;
  o.vel[1] = Math.random()*40+20;
  o.moveYaw = Math.floor(Math.random()*65536);
  o.gravity = 3;
  o.friction = 1;
  o.buoyancy = 1.5;
  o.billboard = true;
  o.hitbox.radius = 100;
  o.hitbox.height = 64;
}

export function movingYellowCoin(o: SimObject, ctx: ObjectContext): void {
  const flags = o.stepPhysics(ctx.world);
  if (o.grounded) {
    if (!(flags & OBJ_COL_FLAG_NO_Y_VEL)) ctx.coinBounce(o.pos);
    const type = ctx.world.findFloor(...o.pos).surface?.type;
    if (type === SURFACE_BURNING || type === SURFACE_DEATH_PLANE) {
      o.deactivate();
      return;
    }
  }
  if (o.timer >= 342) { o.deactivate(); return; }
  o.visible = o.timer < 302 || o.timer%2 === 0;
}
