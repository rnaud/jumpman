// water_bomb_cannon.inc.c and water_bomb_act_shot_from_cannon in water_bomb.inc.c.
import { approachS32, coss, sins, toS16 } from '../math';
import type { ObjectContext, SimObject } from '../object';

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

export function waterBombBehavior(o: SimObject): void {
  if (o.actionTimer++ > 100) { o.deactivate(-1); return; }
  if (o.vars.height > 1.2) o.vars.height -= 0.1;
  o.vars.width = 2-o.vars.height;
  moveAlongBarrel(o,o.vars.pitch);
}
