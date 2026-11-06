// water_bomb_cannon.inc.c and water_bomb_act_shot_from_cannon in water_bomb.inc.c.
import { approachS32, coss, sins, toS16 } from '../math';
import { INTERACT_MR_BLIZZARD, INT_STATUS_INTERACTED, type ObjectContext, type SimObject } from '../object';

export function initWaterBombCannon(o: SimObject): void {
  o.visible = false;
  Object.assign(o.vars, {fireTimer:0, aimDelay:0, targetYaw:0});
}

export function initCannonBarrel(o: SimObject): void {
  o.vars.recoil = 0;
}

export function initWaterBomb(o: SimObject): void {
  o.forwardVel = -100;
  o.vars.height = 1.7;
}

function moveAlongBarrel(o: SimObject, pitch: number): void {
  const horizontal = o.forwardVel * coss(pitch);
  o.pos[0] += horizontal * sins(o.moveYaw);
  o.pos[1] -= o.forwardVel * sins(pitch);
  o.pos[2] += horizontal * coss(o.moveYaw);
}

export function waterBombCannonBehavior(o: SimObject, ctx: ObjectContext): void {
  const dx = ctx.mario.pos[0]-o.pos[0], dz = ctx.mario.pos[2]-o.pos[2];
  const dist = Math.hypot(dx,dz);
  if (Math.abs(ctx.mario.pos[1]-o.pos[1]) < 300 && dist < 220) {
    ctx.mario.pos[0] += (220-dist)/220*dx;
    ctx.mario.pos[2] += (220-dist)/220*dz;
  }

  if (!o.action) {
    if (o.distanceToMario >= 2000) return;
    const barrel = o.link ?? ctx.spawn({behavior:'cannonBarrel',model:'actors/cannon_barrel/cannon_barrel_geo.glb',pos:o.pos,yaw:o.moveYaw});
    o.link = barrel;
    barrel.link = o;
    barrel.active = barrel.visible = o.visible = true;
    barrel.forwardVel = barrel.vars.recoil = 0;
    o.vars.pitch = o.vars.targetPitch = 0x1c00;
    o.setAction(1);
    return;
  }
  if (o.distanceToMario > 2500) {
    o.visible = false;
    o.link?.deactivate(-1);
    o.setAction(0);
    return;
  }
  // The act-1 cannon at the start has BPARAM2(1) and never fires.
  if (o.param !== 0) return;
  if (o.vars.fireTimer > 0) { o.vars.fireTimer--; return; }
  o.vars.pitch = approachS32(o.vars.pitch,o.vars.targetPitch,0x80,0x80);
  const delta = toS16(o.vars.targetYaw-o.faceYaw);
  o.faceYaw = toS16(o.vars.targetYaw-approachS32(delta,0,0x100,0x100));
  if (o.faceYaw !== o.vars.targetYaw) return;
  if (o.vars.aimDelay > 0) { o.vars.aimDelay--; return; }
  o.vars.fireTimer = 70;
  o.vars.aimDelay = 60;
  o.vars.targetPitch = 0x1000 + 0x400 * Math.floor(Math.random()*4);
  o.vars.targetYaw = toS16(o.moveYaw-0x2000 + 0x1000 * Math.floor(Math.random()*5));
  ctx.waterCannonFired(o.pos);
}

export function cannonBarrelBehavior(o: SimObject, ctx: ObjectContext): void {
  const base = o.link;
  if (!base) return;
  o.moveYaw = o.faceYaw = base.faceYaw;
  o.vars.pitch = base.vars.pitch;
  o.vars.recoil += o.forwardVel;
  if (o.vars.recoil > 0) {
    moveAlongBarrel(o,o.vars.pitch+0x4000);
    o.forwardVel = approachS32(o.forwardVel,-5,18,18);
    return;
  }
  o.vars.recoil = 0;
  o.pos = [...base.pos];
  if (!base.vars.fireTimer) { o.forwardVel = 0; return; }
  if (o.forwardVel !== 0) return;
  o.forwardVel = 35;
  // Its 101-frame lifetime is shorter than the minimum interval between shots.
  const bomb = base.links?.[0] ?? ctx.spawn({behavior:'waterBomb',model:'actors/water_bubble/water_bomb_geo.glb',pos:o.pos,yaw:o.faceYaw});
  base.links = [bomb];
  bomb.pos = [...o.pos];
  bomb.moveYaw = bomb.faceYaw = o.faceYaw;
  bomb.vars.pitch = o.vars.pitch+0x4000;
  bomb.vars.height = 1.7;
  bomb.active = bomb.visible = true;
  bomb.actionTimer = 0;
}

export function waterBombBehavior(o: SimObject, ctx: ObjectContext): void {
  if (o.action !== 0) { fallingBomb(o,ctx); return; }
  if (o.actionTimer++ > 100) { o.deactivate(-1); return; }
  if (o.vars.height > 1.2) o.vars.height -= 0.1;
  o.vars.width = 2-o.vars.height;
  moveAlongBarrel(o,o.vars.pitch);
}

export function waterBombSpawner(o: SimObject, ctx: ObjectContext): void {
  if (!o.vars.onFloor) {
    const floor = ctx.world.findFloor(...o.home);
    if (floor.surface) o.pos[1] = floor.height;
    o.vars.onFloor = 1;
  }
  const m = ctx.mario;
  if (o.link?.active || Math.hypot(m.pos[0]-o.pos[0],m.pos[2]-o.pos[2]) >= 200+50*o.param ||
      m.pos[1]-o.pos[1] >= 1000) return;
  if (o.vars.delay > 0) { o.vars.delay--; return; }

  const ahead = 100 + 28*m.forwardVel;
  const bomb = o.link ?? ctx.spawn({behavior:'waterBomb',model:'actors/water_bubble/water_bomb_geo.glb',pos:o.pos,yaw:0});
  o.link = bomb;
  bomb.reactivate();
  bomb.pos = [m.pos[0]+ahead*sins(m.faceAngle[1]),o.pos[1]+2000,m.pos[2]+ahead*coss(m.faceAngle[1])];
  bomb.setAction(1);
  bomb.hitbox.interactType = 0;
  bomb.graphYOffset = 0;
  Object.assign(bomb.vars,{height:1,width:1,stretch:0,bounces:0});
  const shadow = bomb.link ?? ctx.spawn({behavior:'waterBombShadow',model:'actors/water_bubble/water_bomb_shadow_geo.glb',pos:bomb.pos,yaw:0});
  shadow.link = bomb;
  bomb.link = shadow;
  shadow.active = shadow.visible = true;
  o.vars.delay = Math.floor(Math.random()*50);
}

export function waterBombShadow(o: SimObject, ctx: ObjectContext): void {
  const bomb = o.link;
  if (!bomb?.active || bomb.action === 3) { o.deactivate(-1); return; }
  const [x,y,z] = bomb.pos;
  const floor = ctx.world.findFloor(x,y,z);
  o.pos = [x,Math.min(y,floor.height+500),z];
  o.scale = bomb.vars.width;
}

function fallingBomb(o: SimObject, ctx: ObjectContext): void {
  if (o.action === 1) {
    ctx.waterBombEvent(o.pos,'drop');
    o.vel[1] = -40;
    o.setAction(2);
    return;
  }
  if (o.action === 3) {
    ctx.waterBombEvent(o.pos,'burst');
    o.deactivate(-1);
    return;
  }
  o.setHitbox({interactType:INTERACT_MR_BLIZZARD,downOffset:25,damageOrCoinValue:1,
    health:99,numLootCoins:0,radius:80,height:50,hurtboxRadius:60,hurtboxHeight:50});
  if ((o.interactStatus & INT_STATUS_INTERACTED) || o.vars.enteredWater) {
    ctx.waterBombEvent(o.pos,'splash');
    o.setAction(3);
    return;
  }
  if (o.grounded) {
    if (!o.vars.squashing) {
      o.vars.squashing = 1;
      o.vars.bounces++;
      ctx.waterBombEvent(o.pos,o.vars.bounces < 3 ? 'bounce' : 'splash');
      o.moveYaw = o.angleToMario;
      o.forwardVel = 10;
      o.vars.stretchSpeed = -40;
    }
    o.vars.stretchSpeed += 15-o.vars.bounces*2.8;
    o.vars.stretch += o.vars.stretchSpeed*0.01;
    if (o.vars.stretch < -0.8) o.setAction(3);
    else if (o.vars.stretch > 0.1) o.vel[1] = 1.8*o.vars.stretchSpeed;
  } else {
    o.vars.stretch = approachS32(o.vars.stretch,0,0.008,0.008);
    o.vars.squashing = 0;
  }
  o.vars.height = 1+o.vars.stretch;
  o.vars.width = 1-o.vars.stretch*(o.vars.bounces === 3 ? 4 : 1);
  o.graphYOffset = 40*o.vars.height;

  // cur_obj_move_standard with drag strength 10, gravity -4 and zero restitution.
  const drag = (v: number) => v*(1-Math.abs(v)*0.001);
  const vx = drag(o.forwardVel*sins(o.moveYaw)), vz = drag(o.forwardVel*coss(o.moveYaw));
  const next = [o.pos[0]+vx,o.pos[1],o.pos[2]+vz] as [number,number,number];
  ctx.world.resolveAndReturnWallCollisions(next,25,120);
  const floor = ctx.world.findFloor(...o.pos);
  const intended = ctx.world.findFloor(...next);
  if (intended.surface && (intended.height-floor.height < 5 || intended.surface.normal[1] > Math.cos(78*Math.PI/180)
      || o.pos[1] > intended.height)) {
    o.pos[0] = next[0];
    o.pos[2] = next[2];
  }
  o.forwardVel = Math.hypot(vx,vz);
  o.vel[1] = Math.max(-78,o.vel[1]-4);
  o.pos[1] += o.vel[1];
  o.vars.enteredWater = Number(o.pos[1] <= ctx.world.waterLevel(o.pos[0],o.pos[2]));
  o.grounded = !!floor.surface && o.pos[1] < floor.height;
  if (o.grounded) { o.pos[1]=floor.height; o.vel[1]=0; }
}
