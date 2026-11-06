import { toS16 } from '../math';
import type { ObjectContext, SimObject } from '../object';

export function initChest(o: SimObject): void {
  o.gravity = 0;
  o.vars.opened = 0;
  o.vars.lidPitch = 0;
  o.visible = true;
}

/** bhv_treasure_chest_bottom_loop and the hinged lid in treasure_chest.inc.c. */
export function chestBehavior(o: SimObject, ctx: ObjectContext): void {
  const m = ctx.mario;
  const puzzle = ctx.chestPuzzle;
  const facing = Math.abs(toS16(o.moveYaw - m.faceAngle[1] - 0x8000)) < 0x3000;
  if (o.action === 0 && !puzzle.locked && facing && o.distanceToMario < 150) {
    if (puzzle.next === o.param) {
      puzzle.next++;
      o.setAction(1);
      ctx.chestOpened(o.param);
    } else {
      puzzle.next = 1;
      puzzle.locked = true;
      o.setAction(2);
      m.takeDamage(1, o.pos);
      ctx.chestOpened(0);
    }
  } else if (o.action === 1 && puzzle.locked) {
    o.setAction(0);
  } else if (o.action === 2 && o.distanceToMario >= 500) {
    puzzle.locked = false;
    o.setAction(0);
  }
  o.vars.opened = Number(o.action === 1);
  o.vars.lidPitch = o.action === 1 ? Math.max(-0x4000, o.vars.lidPitch - 0x200)
    : Math.min(0, o.vars.lidPitch + 0x800);

  const dx = m.pos[0] - o.pos[0], dz = m.pos[2] - o.pos[2], distance = Math.hypot(dx, dz);
  if (Math.abs(m.pos[1] - o.pos[1]) < 150 && distance < 150) {
    m.pos[0] += (150 - distance) / 150 * dx;
    m.pos[2] += (150 - distance) / 150 * dz;
  }
}
