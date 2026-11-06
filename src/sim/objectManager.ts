/**
 * Object list processor and Mario interaction dispatcher for the course
 * objects. Runs once per 30 Hz tick before Mario's action step, mirroring the
 * order used by level_update.c.
 */

import { ACT_DIVE, ACT_FREEFALL, ACT_MOVE_PUNCHING, ACT_PUNCHING } from './state';
import { atan2s, dist3D, type Vec3 } from './math';
import { FLOOR_LOWER_LIMIT, type CollisionWorld } from './collision';
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
import { initPenguin, penguinBehavior } from './behaviors/penguin';
import { initMrBlizzard, mrBlizzardBehavior } from './behaviors/mrBlizzard';
import { initOneUp, oneUpBehavior } from './behaviors/oneUp';
import { initWhomp, initWhompKing, whompBehavior, whompKingBehavior } from './behaviors/whomp';
import { bullyBehavior, initBully } from './behaviors/bully';
import { bigBullyBehavior, initBigBully } from './behaviors/bigBully';
import { bowserBehavior, initBowser, initBowserMine, bowserMineBehavior, atBowserTail } from './behaviors/bowser';
import { bowserFlameBehavior, initBowserFlame } from './behaviors/bowserFlame';
import { initKlepto, kleptoBehavior } from './behaviors/klepto';
import { initPokey, pokeyBehavior } from './behaviors/pokey';
import { initStarBox, starBoxBehavior } from './behaviors/starBox';
import { chestBehavior, initChest } from './behaviors/chest';
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
  | { type: 'oneUp'; pos: Vec3 }
  | { type: 'whompKing'; pos: Vec3 }
  | { type: 'bigBully'; pos: Vec3 }
  | { type: 'bowser'; pos: Vec3 }
  | { type: 'chest'; number: number }
  | { type: 'boxStar'; pos: Vec3; act: number }
  | { type: 'chestReward' }
  | { type: 'race'; state: 'start' | 'won' | 'lost' }
  | { type: 'dialogue'; text: string }
  | { type: 'warp'; destination: string; entry?: string };

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
  penguin: initPenguin,
  mrBlizzard: initMrBlizzard,
  oneUp: initOneUp,
  whomp: initWhomp,
  whompKing: initWhompKing,
  bully: initBully,
  bigBully: initBigBully,
  bowserMine: initBowserMine,
  bowser: initBowser,
  bowserFlame: initBowserFlame,
  klepto: initKlepto,
  pokey: initPokey,
  starBox: initStarBox,
  chest: initChest,
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
  penguin: penguinBehavior,
  mrBlizzard: mrBlizzardBehavior,
  oneUp: oneUpBehavior,
  whomp: whompBehavior,
  whompKing: whompKingBehavior,
  bully: bullyBehavior,
  bigBully: bigBullyBehavior,
  bowserMine: bowserMineBehavior,
  bowser: bowserBehavior,
  bowserFlame: bowserFlameBehavior,
  klepto: kleptoBehavior,
  pokey: pokeyBehavior,
  starBox: starBoxBehavior,
  chest: chestBehavior,
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
  chestPuzzle = { next: 1, locked: false };
  private chestRewardTimer = 0;
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

  spawnObject(spec: ObjectSpec): SimObject {
    return this.spawn(spec);
  }

  findByBehavior(behavior: ObjectBehaviorKind): SimObject | undefined {
    return this.objects.find(object => object.behavior === behavior && object.active);
  }

  reset(): void {
    this.chestPuzzle = { next: 1, locked: false };
    this.chestRewardTimer = 0;
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
    if (this.chestPuzzle.next === 5 && ++this.chestRewardTimer === 62) this.events.push({ type: 'chestReward' });

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
        && (object.pos[1] < FLOOR_LOWER_LIMIT || Math.abs(object.pos[0]) > 8000 || Math.abs(object.pos[2]) > 8000)
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
    const forward = object.behavior === 'bowser' ? 315 : object.behavior === 'bobomb' ? 150 : 120;
    const height = object.behavior === 'bowser' ? 0 : object.behavior === 'bobomb' ? 75 : 55;
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

      if (object.behavior === 'bowser') {
        if (object.action !== 0) continue;
        if (atBowserTail(object, mario)) {
          mario.nearGrabbable = true;
          if (canGrabAction(mario)) {
            mario.interactObj = object;
            mario.interactGrabbable = true;
          }
        } else if (Math.hypot(mario.pos[0] - object.pos[0], mario.pos[2] - object.pos[2]) < 137) {
          mario.takeDamage(2, object.pos);
        }
        continue;
      }

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

  oneUpCollected(position: Vec3): void {
    this.events.push({ type: 'oneUp', pos: [...position] as Vec3 });
  }

  whompKingDefeated(position: Vec3): void {
    this.events.push({ type: 'whompKing', pos: [...position] as Vec3 });
  }

  bigBullyDefeated(position: Vec3): void {
    this.events.push({ type: 'bigBully', pos: [...position] as Vec3 });
  }

  bowserDefeated(position: Vec3): void {
    this.events.push({ type: 'bowser', pos: [...position] as Vec3 });
  }

  starFromBox(box: SimObject): void {
    this.events.push({ type: 'boxStar', pos: [...box.pos], act: box.param });
  }

  chestOpened(number: number): void {
    this.events.push({ type: 'chest', number });
  }

  raceEvent(state: 'start' | 'won' | 'lost'): void {
    this.events.push({ type: 'race', state });
  }

  dialogue(text: string): void {
    this.events.push({ type: 'dialogue', text });
  }

  warp(destination: string, entry?: string): void {
    this.events.push({ type: 'warp', destination, entry });
  }
}
