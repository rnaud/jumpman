/**
 * Object list processor and Mario interaction dispatcher for the course
 * objects. Runs once per 30 Hz tick before Mario's action step, mirroring the
 * order used by level_update.c.
 */

import { ACT_DIVE, ACT_FREEFALL, ACT_MOVE_PUNCHING, ACT_PUNCHING } from './state';
import { atan2s, dist3D, type Vec3 } from './math';
import type { CollisionWorld } from './collision';
import type { MarioRuntime } from './MarioRuntime';
import {
  ATTACK_FAST_ATTACK,
  ATTACK_FROM_ABOVE,
  ATTACK_KICK_OR_TRIP,
  attackObject,
  detectInteraction,
  HELD_DROPPED,
  HELD_HELD,
  INT_STATUS_INTERACTED,
  INT_STATUS_MARIO_KNOCKBACK_DMG,
  INT_STATUS_TOUCHED_BOB_OMB,
  INT_STATUS_WAS_ATTACKED,
  INTERACT_BREAKABLE,
  INTERACT_DAMAGE,
  INTERACT_GRABBABLE,
  INTERACT_MR_BLIZZARD,
  INTERACT_POLE,
  INTERACT_KOOPA,
  INTERACT_KOOPA_SHELL,
  INT_SUBTYPE_NOT_GRABBABLE,
  marioOverlapsObject,
  pushMarioOutOfObject,
  SimObject,
  type ObjectBehaviorKind,
  type ObjectContext,
  type ObjectSpec,
} from './object';
import { bobombBehavior, initBobomb } from './behaviors/bobomb';
import {
  chainChompBehavior,
  chainChompGateBehavior,
  chainPartBehavior,
  initChainChomp,
  initChainChompGate,
  initChainPart,
  initWoodenPost,
  woodenPostBehavior,
} from './behaviors/chainChomp';
import {
  grillDoorBehavior,
  initOpenableGrill,
  initPitBowlingBall,
  initPurpleSwitch,
  openableGrillBehavior,
  pitBowlingBallBehavior,
  purpleSwitchBehavior,
} from './behaviors/bobObjects';
import { initKingBobomb, kingBobombBehavior } from './behaviors/kingBobomb';
import { initTree, treeBehavior } from './behaviors/tree';
import { initKoopa, initKoopaShell, koopaBehavior, koopaShellBehavior } from './behaviors/koopa';
import { bobombBuddyBehavior, initBobombBuddy } from './behaviors/bobombBuddy';
import { initWarp, warpBehavior } from './behaviors/warp';
import { initPoisonGas, poisonGasBehavior } from './behaviors/poisonGas';
import { booBehavior, initBoo } from './behaviors/boo';
import { flameBehavior, initFlame } from './behaviors/flame';
import { bigBooBehavior, initBigBoo } from './behaviors/bigBoo';
import {
  cannonBarrelBehavior,
  initCannonBarrel,
  initWaterBomb,
  initWaterBombCannon,
  waterBombBehavior,
  waterBombCannonBehavior,
} from './behaviors/waterBombCannon';
import { initKoopaTheQuick, koopaTheQuickBehavior, initKoopaFlag, koopaFlagBehavior } from './behaviors/koopaTheQuick';
import {
  breakableBoxBehavior,
  breakableBoxSmallBehavior,
  initBreakableBox,
  initBreakableBoxSmall,
} from './behaviors/breakableBox';

export type ObjectEvent =
  | { type: 'explosion'; pos: Vec3; radius: number }
  | { type: 'break'; pos: Vec3 }
  | { type: 'coin'; pos: Vec3 }
  | { type: 'hurt'; pos: Vec3 }
  | { type: 'boss'; pos: Vec3 }
  | { type: 'bigBoo'; pos: Vec3 }
  | { type: 'race'; state: 'start' | 'won' | 'lost' }
  | { type: 'dialogue'; text: string }
  | { type: 'warp'; destination: string };

const INITIALIZERS: Record<ObjectBehaviorKind, (object: SimObject) => void> = {
  bobomb: initBobomb,
  breakableBox: initBreakableBox,
  breakableBoxSmall: initBreakableBoxSmall,
  chainChomp: initChainChomp,
  woodenPost: initWoodenPost,
  chainChompGate: initChainChompGate,
  chainPart: initChainPart,
  purpleSwitch: initPurpleSwitch,
  openableGrill: initOpenableGrill,
  grillDoor: (object) => { object.visible = true; object.vars.timer = 0; },
  pitBowlingBall: initPitBowlingBall,
  kingBobomb: initKingBobomb,
  koopaTheQuick: initKoopaTheQuick,
  koopaFlag: initKoopaFlag,
  tree: initTree,
  waterBombCannon: initWaterBombCannon,
  cannonBarrel: initCannonBarrel,
  waterBomb: initWaterBomb,
  koopa: initKoopa,
  koopaShell: initKoopaShell,
  bobombBuddy: initBobombBuddy,
  warp: initWarp,
  poisonGas: initPoisonGas,
  boo: initBoo,
  bigBoo: initBigBoo,
  flame: initFlame,
};

const UPDATERS: Record<ObjectBehaviorKind, (object: SimObject, ctx: ObjectContext) => void> = {
  bobomb: bobombBehavior,
  breakableBox: breakableBoxBehavior,
  breakableBoxSmall: breakableBoxSmallBehavior,
  chainChomp: chainChompBehavior,
  woodenPost: woodenPostBehavior,
  chainChompGate: chainChompGateBehavior,
  chainPart: chainPartBehavior,
  purpleSwitch: purpleSwitchBehavior,
  openableGrill: openableGrillBehavior,
  grillDoor: grillDoorBehavior,
  pitBowlingBall: pitBowlingBallBehavior,
  kingBobomb: kingBobombBehavior,
  koopaTheQuick: koopaTheQuickBehavior,
  koopaFlag: koopaFlagBehavior,
  tree: treeBehavior,
  waterBombCannon: waterBombCannonBehavior,
  cannonBarrel: cannonBarrelBehavior,
  waterBomb: waterBombBehavior,
  koopa: koopaBehavior,
  koopaShell: koopaShellBehavior,
  bobombBuddy: bobombBuddyBehavior,
  warp: warpBehavior,
  poisonGas: poisonGasBehavior,
  boo: booBehavior,
  bigBoo: bigBooBehavior,
  flame: flameBehavior,
};

function canGrabAction(mario: MarioRuntime): boolean {
  return (
    ((mario.action === ACT_PUNCHING || mario.action === ACT_MOVE_PUNCHING) && mario.actionArg < 2)
    || mario.action === ACT_DIVE
  );
}

export class ObjectManager implements ObjectContext {
  readonly world: CollisionWorld;
  readonly objects: SimObject[] = [];
  readonly events: ObjectEvent[] = [];
  mario!: MarioRuntime;

  private readonly specs: ObjectSpec[];

  constructor(world: CollisionWorld, specs: ObjectSpec[]) {
    this.world = world;
    this.specs = specs.map(spec => ({ ...spec, pos: [...spec.pos] as Vec3 }));
    this.reset();
  }

  spawn(spec: ObjectSpec): SimObject {
    const object = new SimObject(spec);
    INITIALIZERS[object.behavior](object);
    this.objects.push(object);
    return object;
  }

  findByBehavior(behavior: ObjectBehaviorKind): SimObject | undefined {
    return this.objects.find(object => object.behavior === behavior && object.active);
  }

  reset(): void {
    this.objects.length = 0;
    this.events.length = 0;
    for (const spec of this.specs) {
      const object = new SimObject(spec);
      INITIALIZERS[object.behavior](object);
      this.objects.push(object);
    }
  }

  /** Run one object tick and refresh Mario's interaction targets. */
  step(mario: MarioRuntime): void {
    this.mario = mario;
    this.events.length = 0;

    for (const object of this.objects) {
      if (!object.active) {
        if (object.respawnTimer >= 0) {
          object.respawnTimer--;
          if (object.respawnTimer === 0) {
            object.reactivate();
            INITIALIZERS[object.behavior](object);
          }
        }
        continue;
      }

      object.timer++;
      object.distanceToMario = dist3D(object.pos, mario.pos);
      object.angleToMario = atan2s(mario.pos[2] - object.pos[2], mario.pos[0] - object.pos[0]);
      if (object.heldState === HELD_HELD && mario.heldObj === object) this.syncHeld(mario, object);
    }

    this.detectMarioInteractions(mario);
    for (const object of this.objects) {
      if (!object.active) continue;
      UPDATERS[object.behavior](object, this);
      if (
        object.active
        && (object.pos[1] < -5000 || Math.abs(object.pos[0]) > 8000 || Math.abs(object.pos[2]) > 8000)
      ) {
        object.deactivate(object.behavior === 'bobomb' ? 90 : -1);
      }
    }

    if (mario.heldObj) this.syncHeld(mario, mario.heldObj);
  }

  /** Keep a held object attached after Mario's action step has run. */
  afterMarioStep(mario: MarioRuntime): void {
    for (const object of this.objects) {
      if (object.heldState === HELD_HELD && mario.heldObj !== object) {
        object.heldState = HELD_DROPPED;
      }
    }
    if (mario.heldObj) this.syncHeld(mario, mario.heldObj);
  }

  drainEvents(): ObjectEvent[] {
    if (this.events.length === 0) return [];
    return this.events.splice(0, this.events.length);
  }

  private syncHeld(mario: MarioRuntime, object: SimObject): void {
    const yaw = mario.faceAngle[1];
    // Bob-ombs hide their model while held in the original. Jumpman has no hand
    // attachment, so keep the object visible just in front of Mario instead.
    const forward = object.behavior === 'bobomb' ? 150 : 120;
    const height = object.behavior === 'bobomb' ? 75 : 55;
    object.pos[0] = mario.pos[0] + Math.sin((yaw / 32768) * Math.PI) * forward;
    object.pos[1] = mario.pos[1] + height;
    object.pos[2] = mario.pos[2] + Math.cos((yaw / 32768) * Math.PI) * forward;
    object.moveYaw = yaw;
    object.faceYaw = yaw;
    object.forwardVel = 0;
    object.vel[1] = 0;
  }

  private detectMarioInteractions(mario: MarioRuntime): void {
    mario.interactObj = null;
    mario.interactGrabbable = false;
    mario.nearGrabbable = false;
    mario.nearBreakable = false;

    for (const object of this.objects) {
      if (!object.active || object.heldState === HELD_HELD) continue;
      if (!marioOverlapsObject(mario, object)) continue;

      const interactType = object.hitbox.interactType;
      const attack = detectInteraction(mario, object);

      if (interactType & INTERACT_POLE) {
        if (mario.tryGrabPole(object)) continue;
        continue;
      }

      if (interactType & (INTERACT_KOOPA | INTERACT_KOOPA_SHELL)) {
        if (attack !== 0) {
          if (object.behavior === 'koopa') {
            object.vars.hit = 1;
            if (attack === ATTACK_FROM_ABOVE) {
              mario.pos[1] = object.pos[1] + object.hitbox.height + 60;
              mario.vel[1] = 30;
              mario.setAction(ACT_FREEFALL, 0);
            }
          } else {
            object.vars.sliding = 1;
          }
        }
        continue;
      }

      if (interactType & (INTERACT_DAMAGE | INTERACT_MR_BLIZZARD)) {
        if (attack !== 0) {
          attackObject(object, attack);
          continue;
        }
        if (mario.takeDamage(object.hitbox.damageOrCoinValue || 1, object.pos)) {
          object.interactStatus |= INT_STATUS_INTERACTED;
        }
        continue;
      }

      if (interactType & INTERACT_GRABBABLE) {
        if (object.interactSubtype & INT_SUBTYPE_NOT_GRABBABLE) continue;
        mario.nearGrabbable = true;

        if (attack === ATTACK_KICK_OR_TRIP || attack === ATTACK_FAST_ATTACK) {
          attackObject(object, attack);
          object.interactStatus |= INT_STATUS_MARIO_KNOCKBACK_DMG;
          continue;
        }

        if (canGrabAction(mario)) {
          mario.interactObj = object;
          mario.interactGrabbable = true;
        } else {
          // The source uses -5, but our interaction pass runs before Mario's action
          // step; a tighter push keeps the object inside the grab cone on the
          // punch frame that actually checks for a grab.
          pushMarioOutOfObject(mario, object, -20);
        }
        continue;
      }

      if (interactType & INTERACT_BREAKABLE) {
        mario.nearBreakable = true;
        if (attack === 0) {
          pushMarioOutOfObject(mario, object, 0);
          continue;
        }
        attackObject(object, attack);
        if (attack === ATTACK_FROM_ABOVE) {
          mario.pos[1] = object.pos[1] + object.hitbox.height - object.hitbox.downOffset;
          mario.vel[1] = 30;
          mario.setAction(ACT_FREEFALL, 0);
        }
      }
    }
  }

  explode(source: SimObject, radius: number, damage: number): void {
    this.events.push({ type: 'explosion', pos: [...source.pos] as Vec3, radius });

    for (const object of this.objects) {
      if (object === source || !object.active) continue;
      const distance = dist3D(object.pos, source.pos);
      if (distance > radius) continue;
      if (object.behavior === 'bobomb') {
        object.interactStatus |= INT_STATUS_TOUCHED_BOB_OMB;
      } else if (object.behavior === 'breakableBox') {
        object.interactStatus |= INT_STATUS_INTERACTED | INT_STATUS_WAS_ATTACKED;
      }
    }

    const mario = this.mario;
    if (!mario) return;
    const dx = mario.pos[0] - source.pos[0];
    const dz = mario.pos[2] - source.pos[2];
    const vertical = mario.pos[1] > source.pos[1] - 160 && mario.pos[1] < source.pos[1] + 260;
    if (dx * dx + dz * dz > (radius + 37) * (radius + 37) || !vertical) return;

    if (mario.takeDamage(damage, source.pos)) {
      this.events.push({ type: 'hurt', pos: [...mario.pos] as Vec3 });
    }
  }

  spawnCoin(position: Vec3): void {
    this.events.push({ type: 'coin', pos: [...position] as Vec3 });
  }

  breakObject(source: SimObject): void {
    this.events.push({ type: 'break', pos: [...source.pos] as Vec3 });
  }

  bossDefeated(position: Vec3): void {
    this.events.push({ type: 'boss', pos: [...position] as Vec3 });
  }

  bigBooDefeated(position: Vec3): void {
    this.events.push({ type: 'bigBoo', pos: [...position] as Vec3 });
  }

  raceEvent(state: 'start' | 'won' | 'lost'): void {
    this.events.push({ type: 'race', state });
  }

  dialogue(text: string): void {
    this.events.push({ type: 'dialogue', text });
  }

  warp(destination: string): void {
    this.events.push({ type: 'warp', destination });
  }
}
