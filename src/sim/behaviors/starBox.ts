import { sins } from '../math';
import { ACT_GROUND_POUND_LAND } from '../state';
import { INTERACT_BREAKABLE, INT_STATUS_WAS_ATTACKED, type ObjectContext, type SimObject } from '../object';

/** The star contents of bhvExclamationBox, including its eight-frame recoil. */
export function initStarBox(o: SimObject): void {
  o.scale = 2;
  o.setHitbox({ interactType: INTERACT_BREAKABLE, downOffset: 10, radius: 80, height: 60,
    health: 1, damageOrCoinValue: 0, numLootCoins: 0, hurtboxRadius: 80, hurtboxHeight: 60 });
}

export function starBoxBehavior(o: SimObject, ctx: ObjectContext): void {
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
    ctx.starFromBox(o);
    o.deactivate(-1);
  }
}
