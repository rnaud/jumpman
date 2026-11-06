import { sins } from '../math';
import { ACT_GROUND_POUND_LAND } from '../state';
import { INTERACT_BREAKABLE, INT_STATUS_WAS_ATTACKED, marioOverlapsObject, type ObjectContext, type SimObject } from '../object';

/** The star contents of bhvExclamationBox, including its eight-frame recoil. */
export function initStarBox(o: SimObject): void {
  o.scale = 2;
  o.setHitbox({ interactType: INTERACT_BREAKABLE, downOffset: 10, radius: 80, height: 60,
    health: 1, damageOrCoinValue: 0, numLootCoins: 0, hurtboxRadius: 80, hurtboxHeight: 60 });
}

export function starBoxBehavior(o: SimObject, ctx: ObjectContext): void {
  if (o.behavior === 'wingBox' && !o.vars.unlocked) return;
  if (o.action === 2) {
    if (o.actionTimer++ > 300) {
      o.visible = true;
      o.pos = [...o.home];
      o.graphYOffset = 0;
      o.vars.width = o.vars.height = 1;
      o.setAction(0);
      initStarBox(o);
    }
    return;
  }
  if (!o.action) {
    const m = ctx.mario;
    const pound = m.action === ACT_GROUND_POUND_LAND && Math.abs(m.pos[1] - o.home[1] - 104) < 5
      && Math.abs(m.pos[0] - o.home[0]) < 52 && Math.abs(m.pos[2] - o.home[2]) < 52;
    if (!(o.interactStatus & INT_STATUS_WAS_ATTACKED) && !pound) return;
    o.interactStatus = 0;
    o.hitbox.interactType = 0;
    o.vel[1] = 30;
    o.setAction(1);
    return;
  }
  const t = o.actionTimer++;
  o.vel[1] = Math.max(0, o.vel[1] - 8);
  o.pos[1] += o.vel[1];
  const wave = sins(0x4000 + t * 0x1000);
  o.vars.width = (1 - wave) * 0.5 + 1;
  o.vars.height = (1 + wave) * 0.3;
  o.graphYOffset = (1 - wave) * 26;
  if (t === 8) {
    ctx.breakObject(o);
    if (o.behavior === 'wingBox') {
      const cap = o.link ?? ctx.spawn({behavior:'wingCap',model:'actors/mario_cap/marios_wing_cap_geo.glb',pos:o.pos,yaw:ctx.mario.faceAngle[1]});
      o.link = cap;
      cap.reactivate();
      cap.pos = [...o.pos];
      cap.forwardVel = 3;
      cap.moveYaw = ctx.mario.faceAngle[1];
      cap.vel[1] = 20;
      o.visible = false;
      o.setAction(2);
    } else {
      ctx.starFromBox(o);
      o.deactivate(-1);
    }
  }
}

export function initWingCap(o: SimObject): void {
  o.gravity = 1.2;
  o.friction = 0.999;
  o.buoyancy = 0.9;
  o.setHitbox({interactType:0,downOffset:0,damageOrCoinValue:0,health:0,numLootCoins:0,
    radius:80,height:80,hurtboxRadius:90,hurtboxHeight:90});
}

export function wingCapBehavior(o: SimObject, ctx: ObjectContext): void {
  o.faceYaw += o.forwardVel*128;
  o.stepPhysics(ctx.world);
  if (o.grounded) o.vel[1] = 0;
  if (o.timer > 20 && marioOverlapsObject(ctx.mario,o)) {
    ctx.mario.grantWingCap(1800);
    o.deactivate(-1);
  } else if (o.timer > 320) o.deactivate(-1);
  else if (o.timer > 300) o.visible = o.timer%2 === 0;
}
