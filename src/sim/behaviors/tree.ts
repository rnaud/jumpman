import { INTERACT_POLE, type ObjectContext, type ObjectHitbox, type SimObject } from '../object';

const TREE_HITBOX: ObjectHitbox = {
  interactType: INTERACT_POLE,
  downOffset: 0,
  damageOrCoinValue: 0,
  health: 0,
  numLootCoins: 0,
  radius: 80,
  height: 500,
  hurtboxRadius: 80,
  hurtboxHeight: 500,
};

export function initTree(object: SimObject): void {
  object.setHitbox(TREE_HITBOX);
  object.billboard = true;
  object.visible = true;
}

export function treeBehavior(_object: SimObject, _ctx: ObjectContext): void {
  // bhvTree is a static pole-like actor; climbing is handled by Mario's actions.
}
