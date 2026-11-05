/**
 * Bob-omb Buddy NPCs, simplified from bhvBobombBuddy. The source opens a
 * textbox with advice and can open the cannon; Jumpman emits a HUD dialogue
 * event when Mario approaches.
 */

import type { ObjectContext, ObjectHitbox, SimObject } from '../object';

const BUDDY_HITBOX: ObjectHitbox = {
  interactType: 0,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 100,
  height: 60,
  hurtboxRadius: 100,
  hurtboxHeight: 60,
};

export function initBobombBuddy(object: SimObject): void {
  object.setHitbox(BUDDY_HITBOX);
  object.gravity = 4;
  object.vars.cooldown = 0;
  object.vars.cannon = object.param;
  object.visible = true;
}

export function bobombBuddyBehavior(object: SimObject, ctx: ObjectContext): void {
  const cooldown = Number(object.vars.cooldown ?? 0);
  if (cooldown > 0) {
    object.vars.cooldown = cooldown - 1;
    return;
  }
  if (object.distanceToMario < 400) {
    object.vars.cooldown = 300;
    const cannon = object.vars.cannon === 1;
    ctx.dialogue(cannon
      ? 'BOB-OMB BUDDY: THE CANNON IS READY — AIM FOR THE SUMMIT!'
      : 'BOB-OMB BUDDY: THE STAR IS AT THE TOP OF THE MOUNTAIN!');
  }
}
