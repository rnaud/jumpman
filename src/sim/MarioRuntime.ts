import { submergedStep } from './actions/submerged';
import { ACT_FLAG_SWIMMING, ACT_WATER_PLUNGE } from './state';
/**
 * The Mario controller runtime.
 *
 * This class owns Mario's state and the per-frame pipeline from mario.c:
 *   update_mario_inputs -> update_mario_geometry_inputs -> action loop
 *
 * Physics runs at a fixed 30 Hz, just like the Nintendo 64 original.  Render
 * code should interpolate between the previous and current state.
 */

import type { CollisionWorld } from './collision';
import { SURFACE_DEATH_PLANE } from './surfaces';
import { atan2s, coss, sins, toS16, type Vec3 } from './math';
import {
  findFloorHeightRelativePolar,
  marioFloorIsSlippery,
  marioFloorIsSteep,
  marioGetFloorClass,
  marioSetForwardVel,
} from './marioHelpers';
import { executeMarioAction } from './actions';
import {
  ACT_DIVE,
  ACT_DOUBLE_JUMP,
  ACT_FREEFALL,
  ACT_GROUND_POUND,
  ACT_GROUND_POUND_LAND,
  ACT_IDLE,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_BACKFLIP,
  ACT_LONG_JUMP,
  ACT_SIDE_FLIP,
  ACT_SLIDE_KICK,
  ACT_STEEP_JUMP,
  ACT_TRIPLE_JUMP,
  ACT_WALL_KICK_AIR,
  ACT_BEGIN_SLIDING,
  ACT_BUTT_SLIDE,
  ACT_STOMACH_SLIDE,
  ACT_WALKING,
  ACT_HOLD_JUMP,
  ACT_HOLD_WALKING,
  ACT_FLAG_AIR,
  ACT_FLAG_INTANGIBLE,
  AIR_STEP_CHECK_LEDGE_GRAB,
  A_BUTTON,
  B_BUTTON,
  INPUT_A_DOWN,
  INPUT_A_PRESSED,
  INPUT_ABOVE_SLIDE,
  INPUT_B_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_INTERACT_OBJ_GRABBABLE,
  INPUT_OFF_FLOOR,
  INPUT_UNKNOWN_5,
  INPUT_Z_DOWN,
  INPUT_Z_PRESSED,
  MARIO_KICKING,
  MARIO_PUNCHING,
  MARIO_TRIPPING,
  MARIO_UNKNOWN_08,
  Z_TRIG,
  emptyController,
  type ControllerSnapshot,
  type MarioEvent,
  type MarioState,
  ACT_AIR_THROW_LAND,
  ACT_HARD_BACKWARD_AIR_KB,
  ACT_HARD_BACKWARD_GROUND_KB,
  ACT_STANDING_DEATH,
  ACT_DEATH_ON_BACK,
  ACT_DROWNING,
  ACT_WATER_DEATH,
  ACT_GRAB_POLE_SLOW,
  ACT_GRAB_POLE_FAST,
  ACT_FLAG_ON_POLE,
  ACT_DEATH_ON_STOMACH,
  ACT_HARD_FORWARD_AIR_KB,
  ACT_HARD_FORWARD_GROUND_KB,
  ACT_BACKWARD_AIR_KB,
  ACT_FORWARD_AIR_KB,
  ACT_BACKWARD_GROUND_KB,
  ACT_FORWARD_GROUND_KB,
  ACT_SOFT_BACKWARD_GROUND_KB,
  ACT_SOFT_FORWARD_GROUND_KB,
  ACT_FLAG_INVULNERABLE,
  ACT_HOLD_DECELERATING,
  ACT_HOLD_FREEFALL,
  ACT_HOLD_FREEFALL_LAND,
  ACT_HOLD_FREEFALL_LAND_STOP,
  ACT_HOLD_JUMP_LAND,
  ACT_HOLD_JUMP_LAND_STOP,
  ACT_HOLD_IDLE,
  ACT_PICKING_UP,
  ACT_PLACING_DOWN,
  ACT_THROWING,
  ACT_AIR_THROW,
} from './state';
import { HELD_DROPPED, HELD_FREE, HELD_HELD, HELD_THROWN, type SimObject } from './object';
import { performAirStep, performGroundStep } from './marioStep';

export interface SpawnPoint {
  pos: Vec3;
  faceYaw: number;
}

const MAX_ACTION_ITERATIONS = 12;

/** Actions that may keep an object in Mario's hands. */
const HOLDING_ACTIONS = new Set([
  ACT_PICKING_UP,
  ACT_HOLD_IDLE,
  ACT_HOLD_WALKING,
  ACT_HOLD_DECELERATING,
  ACT_HOLD_JUMP,
  ACT_HOLD_FREEFALL,
  ACT_HOLD_JUMP_LAND,
  ACT_HOLD_FREEFALL_LAND,
  ACT_HOLD_JUMP_LAND_STOP,
  ACT_HOLD_FREEFALL_LAND_STOP,
  ACT_THROWING,
  ACT_PLACING_DOWN,
  ACT_AIR_THROW,
  ACT_AIR_THROW_LAND,
]);

export class MarioRuntime implements MarioState {
  readonly world: CollisionWorld;
  readonly spawn: SpawnPoint;

  action = ACT_IDLE;
  prevAction = 0;
  actionState = 0;
  actionTimer = 0;
  actionArg = 0;

  input = 0;
  flags = 0;
  particleFlags = 0;

  pos: Vec3 = [0, 0, 0];
  vel: Vec3 = [0, 0, 0];
  faceAngle: [number, number, number] = [0, 0, 0];

  longJumpIsSlow = false;
  forwardVel = 0;
  slideVelX = 0;
  slideVelZ = 0;
  slideYaw = 0;
  intendedMag = 0;
  intendedYaw = 0;

  floor = null as MarioState['floor'];
  ceil = null as MarioState['ceil'];
  wall = null as MarioState['wall'];
  floorHeight = 0;
  ceilHeight = 20000;
  floorAngle = 0;

  peakHeight = 0;
  swimStrength = 160;
  swimYawVel = 0;
  quicksandDepth = 0;
  squishTimer = 0;

  doubleJumpTimer = 0;
  wallKickTimer = 0;
  framesSinceA = 0;
  framesSinceB = 0;

  invincTimer = 0;
  health = 0x880;
  hurtCounter = 0;
  healCounter = 0;

  animFrame = 0;
  animLength = 1;
  jumpCombo = 0;
  lands = 0;
  wallKicks = 0;
  respawns = 0;

  /** Camera yaw used to rotate the analog stick into world space. */
  cameraYaw = 0;
  controller: ControllerSnapshot = emptyController();
  events: MarioEvent[] = [];
  transitions: Array<Record<string, unknown>> = [];

  /** Object interaction state, mirroring MarioState.heldObj/usedObj/interactObj. */
  heldObj: SimObject | null = null;
  usedObj: SimObject | null = null;
  polePos = 0;
  stickY = 0;
  interactObj: SimObject | null = null;
  interactGrabbable = false;
  nearGrabbable = false;
  nearBreakable = false;

  private actionLoops = 0;

  constructor(world: CollisionWorld, spawn: SpawnPoint, private readonly deathHeight = -1600) {
    this.world = world;
    this.spawn = { pos: [...spawn.pos] as Vec3, faceYaw: spawn.faceYaw };
    this.resetToSpawn();
  }

  resetToSpawn(): void {
    if (this.heldObj) this.heldObj.setHeldState(HELD_FREE);
    this.heldObj = null;
    this.usedObj = null;
    this.interactObj = null;
    this.interactGrabbable = false;
    this.nearGrabbable = false;
    this.nearBreakable = false;
    const [x, , z] = this.spawn.pos;
    const hit = this.world.findFloor(x, this.spawn.pos[1] + 200, z);
    this.pos = [x, hit.surface ? hit.height : this.spawn.pos[1], z];
    this.vel = [0, 0, 0];
    this.forwardVel = 0;
    this.slideVelX = 0;
    this.slideVelZ = 0;
    this.slideYaw = 0;
    this.faceAngle = [0, this.spawn.faceYaw, 0];
    this.intendedMag = 0;
    this.intendedYaw = this.spawn.faceYaw;
    this.action = ACT_IDLE;
    this.prevAction = 0;
    this.actionState = 0;
    this.actionTimer = 0;
    this.actionArg = 0;
    this.input = 0;
    this.flags = 0;
    this.particleFlags = 0;
    this.floor = hit.surface;
    this.ceil = null;
    this.wall = null;
    this.floorHeight = hit.surface ? hit.height : this.spawn.pos[1];
    this.ceilHeight = 20000;
    this.floorAngle = hit.surface ? atan2s(hit.surface.normal[2], hit.surface.normal[0]) : 0;
    this.peakHeight = this.pos[1];
    this.swimStrength = 160;
    this.swimYawVel = 0;
    this.quicksandDepth = 0;
    this.squishTimer = 0;
    this.doubleJumpTimer = 0;
    this.wallKickTimer = 0;
    this.invincTimer = 0;
    this.health = 0x880;
    this.hurtCounter = 0;
    this.healCounter = 0;
    this.jumpCombo = 0;
    this.animFrame = 0;
    this.animLength = 1;
    this.controller = emptyController();
    this.cameraYaw = toS16(this.spawn.faceYaw + 0x8000);
  }

  respawn(): void {
    this.respawns++;
    this.events.push({ type: 'respawn' });
    this.resetToSpawn();
  }

  /**
   * Advance one 30 Hz simulation frame.
   */
  step(controller: ControllerSnapshot, cameraYaw: number): void {
    this.events.length = 0;
    this.controller = controller;
    this.cameraYaw = cameraYaw;

    if (!this.updateMarioInputs()) {
      return;
    }

    const surface = this.world.waterLevel(this.pos[0], this.pos[2]);
    if (!(this.action & ACT_FLAG_SWIMMING) && this.pos[1] < surface - 100) {
      this.setAction(ACT_WATER_PLUNGE);
      this.pos[1] = surface - 100;
      this.faceAngle[0] = this.faceAngle[2] = this.swimYawVel = 0;
      this.forwardVel *= 0.25;
      this.vel[1] *= 0.5;
    }
    if (
      (this.action & ACT_FLAG_SWIMMING)
      && this.action !== ACT_DROWNING
      && this.action !== ACT_WATER_DEATH
    ) {
      submergedStep(this, surface);
      this.checkVoid();
      this.updateMarioHealth();
      return;
    }
    let inLoop = true;
    this.actionLoops = 0;
    while (inLoop && this.actionLoops++ < MAX_ACTION_ITERATIONS) {
      inLoop = executeMarioAction(this);
    }

    this.checkVoid();
    this.updateMarioHealth();
    if (this.squishTimer > 0) this.squishTimer--;
    this.animFrame++;
    if (this.animFrame > this.animLength) this.animFrame = 0;
  }

  /** update_mario_health (mario.c). */
  private updateMarioHealth(): void {
    if (this.health < 0x100) {
      if (!(this.action & ACT_FLAG_INTANGIBLE)) {
        if (this.action & ACT_FLAG_SWIMMING) this.startDrowning();
        else this.startDeath();
      }
      return;
    }

    if (this.health >= 0x100) {
      if (this.healCounter === 0 && this.hurtCounter === 0) {
        if (this.action & ACT_FLAG_SWIMMING) {
          const surface = this.world.waterLevel(this.pos[0], this.pos[2]);
          if (this.pos[1] >= surface - 140) this.health += 0x1a;
          else this.health -= 1;
        }
      }

      if (this.healCounter > 0) {
        this.health += 0x40;
        this.healCounter--;
      }
      if (this.hurtCounter > 0) {
        this.health -= 0x40;
        this.hurtCounter--;
      }

      if (this.health > 0x880) this.health = 0x880;
      if (this.health < 0x100) {
        if (this.action & ACT_FLAG_SWIMMING) this.startDrowning();
        else this.startDeath();
      }
    }
  }

  /** Start the source drowning sequence while submerged. */
  private startDrowning(): void {
    if (this.action === ACT_DROWNING || this.action === ACT_WATER_DEATH) return;
    this.health = 0xff;
    this.events.push({ type: 'death' });
    this.setAction(ACT_DROWNING, 0);
  }

  /** Start the source death sequence appropriate to the current knockback. */
  private startDeath(): void {
    if (this.action === ACT_STANDING_DEATH || this.action === ACT_DEATH_ON_BACK || this.action === ACT_DEATH_ON_STOMACH) {
      return;
    }

    this.health = 0xff;
    let action = ACT_STANDING_DEATH;
    if (
      this.action === ACT_DEATH_ON_BACK
      || this.action === ACT_HARD_BACKWARD_GROUND_KB
      || this.action === ACT_HARD_BACKWARD_AIR_KB
      || this.action === ACT_BACKWARD_GROUND_KB
      || this.action === ACT_BACKWARD_AIR_KB
    ) {
      action = ACT_DEATH_ON_BACK;
    } else if (
      this.action === ACT_HARD_FORWARD_GROUND_KB
      || this.action === ACT_HARD_FORWARD_AIR_KB
      || this.action === ACT_FORWARD_GROUND_KB
      || this.action === ACT_FORWARD_AIR_KB
    ) {
      action = ACT_DEATH_ON_STOMACH;
    }

    this.events.push({ type: 'death' });
    this.setAction(action, 0);
  }

  /**
   * take_damage_and_knock_back / determine_knockback_action (interaction.c).
   * Damage is in source units: 1 = one health wedge (0x100).
   */
  takeDamage(damage: number, sourcePosition?: Vec3): boolean {
    if (damage <= 0) return false;
    if (this.action & ACT_FLAG_INVULNERABLE || this.invincTimer > 0) return false;

    this.hurtCounter += 4 * damage;

    const angleToObject = sourcePosition
      ? atan2s(sourcePosition[2] - this.pos[2], sourcePosition[0] - this.pos[0])
      : toS16(this.faceAngle[1] + 0x8000);
    const facingDYaw = toS16(angleToObject - this.faceAngle[1]);
    const remainingHealth = this.health - 0x40 * this.hurtCounter;
    const strength = remainingHealth < 0x100 || damage >= 4 ? 2 : damage >= 2 ? 1 : 0;
    const inFront = facingDYaw >= -0x4000 && facingDYaw <= 0x4000;
    const air = Boolean(this.action & ACT_FLAG_AIR);

    let action: number;
    if (air) {
      if (inFront) {
        this.faceAngle[1] = angleToObject;
        this.forwardVel = -Math.max(16, Math.abs(this.forwardVel));
        action = strength === 2 ? ACT_HARD_BACKWARD_AIR_KB : ACT_BACKWARD_AIR_KB;
      } else {
        this.faceAngle[1] = toS16(angleToObject + 0x8000);
        this.forwardVel = Math.max(16, Math.abs(this.forwardVel));
        action = strength === 2 ? ACT_HARD_FORWARD_AIR_KB : ACT_FORWARD_AIR_KB;
      }
    } else if (inFront) {
      this.faceAngle[1] = angleToObject;
      this.forwardVel = -Math.max(16, Math.abs(this.forwardVel));
      action = strength === 2
        ? ACT_HARD_BACKWARD_GROUND_KB
        : strength === 1 ? ACT_BACKWARD_GROUND_KB : ACT_SOFT_BACKWARD_GROUND_KB;
    } else {
      this.faceAngle[1] = toS16(angleToObject + 0x8000);
      this.forwardVel = Math.max(16, Math.abs(this.forwardVel));
      action = strength === 2
        ? ACT_HARD_FORWARD_GROUND_KB
        : strength === 1 ? ACT_FORWARD_GROUND_KB : ACT_SOFT_FORWARD_GROUND_KB;
    }

    this.events.push({ type: 'hurt', damage });
    this.dropAndSetAction(action, damage);
    return true;
  }

  private checkVoid(): void {
    if ((this.floor?.type === SURFACE_DEATH_PLANE && this.pos[1] <= this.floorHeight + 10) || this.pos[1] < this.deathHeight || Math.abs(this.pos[0]) >= 8192 || Math.abs(this.pos[2]) >= 8192) {
      this.respawn();
    }
  }

  // -------------------------------------------------------------------------
  // Input
  // -------------------------------------------------------------------------

  private updateMarioInputs(): boolean {
    this.particleFlags = 0;
    this.input = 0;
    this.flags &= 0xffffff;
    this.flags &= ~(MARIO_PUNCHING | MARIO_KICKING | MARIO_TRIPPING);

    this.updateMarioButtonInputs();
    this.updateMarioJoystickInputs();
    if (!this.updateMarioGeometryInputs()) {
      return false;
    }

    if (!(this.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED))) {
      this.input |= INPUT_UNKNOWN_5;
    }

    if (this.wallKickTimer > 0) this.wallKickTimer--;
    if (this.doubleJumpTimer > 0) this.doubleJumpTimer--;
    return true;
  }

  private updateMarioButtonInputs(): void {
    if (this.controller.buttonPressed & A_BUTTON) this.input |= INPUT_A_PRESSED;
    if (this.controller.buttonDown & A_BUTTON) this.input |= INPUT_A_DOWN;

    if (this.squishTimer === 0) {
      if (this.controller.buttonPressed & B_BUTTON) this.input |= INPUT_B_PRESSED;
      if (this.controller.buttonDown & Z_TRIG) this.input |= INPUT_Z_DOWN;
      if (this.controller.buttonPressed & Z_TRIG) this.input |= INPUT_Z_PRESSED;
    }

    if (this.interactGrabbable) this.input |= INPUT_INTERACT_OBJ_GRABBABLE;

    if (this.input & INPUT_A_PRESSED) {
      this.framesSinceA = 0;
    } else if (this.framesSinceA < 0xff) {
      this.framesSinceA++;
    }

    if (this.input & INPUT_B_PRESSED) {
      this.framesSinceB = 0;
    } else if (this.framesSinceB < 0xff) {
      this.framesSinceB++;
    }
  }

  private updateMarioJoystickInputs(): void {
    const c = this.controller;
    this.stickY = c.stickY;
    const mag = (c.stickMag / 64) * (c.stickMag / 64) * 64;

    this.intendedMag = this.squishTimer === 0 ? mag / 2 : mag / 8;

    if (this.intendedMag > 0) {
      this.intendedYaw = toS16(atan2s(-c.stickY, c.stickX) + this.cameraYaw);
      this.input |= INPUT_NONZERO_ANALOG;
    } else {
      this.intendedYaw = this.faceAngle[1];
    }
  }

  private updateMarioGeometryInputs(): boolean {
    this.world.f32FindWallCollision(this.pos, 60, 50);
    this.world.f32FindWallCollision(this.pos, 30, 24);

    const floorHit = this.world.findFloor(this.pos[0], this.pos[1], this.pos[2]);
    this.floorHeight = floorHit.height;
    this.floor = floorHit.surface;

    if (this.floor === null) {
      this.respawn();
      return false;
    }

    const ceilHit = this.world.vec3fFindCeil(this.pos, this.floorHeight);
    this.ceilHeight = ceilHit.height;
    this.ceil = ceilHit.surface;

    this.floorAngle = atan2s(this.floor.normal[2], this.floor.normal[0]);

    if (marioFloorIsSlippery(this)) {
      this.input |= INPUT_ABOVE_SLIDE;
    }

    if (this.pos[1] > this.floorHeight + 100) {
      this.input |= INPUT_OFF_FLOOR;
    }

    return true;
  }

  // -------------------------------------------------------------------------
  // Object interactions
  // -------------------------------------------------------------------------

  /** mario_check_object_grab (interaction.c). */
  marioCheckObjectGrab(): boolean {
    if (!(this.input & INPUT_INTERACT_OBJ_GRABBABLE) || !this.interactObj) return false;
    const facing = toS16(
      atan2s(
        this.interactObj.pos[2] - this.pos[2],
        this.interactObj.pos[0] - this.pos[0],
      ) - this.faceAngle[1],
    );
    if (facing < -0x2aaa || facing > 0x2aaa) return false;

    this.usedObj = this.interactObj;
    if (!(this.action & ACT_FLAG_AIR)) {
      this.setAction(ACT_PICKING_UP, 0);
    }
    return true;
  }

  /** mario_grab_used_object */
  tryGrabPole(object: SimObject): boolean {
    if (!(this.action & ACT_FLAG_AIR) || (this.prevAction & ACT_FLAG_ON_POLE)) return false;
    const speed = this.forwardVel;
    this.usedObj = object;
    this.polePos = Math.max(0, Math.min(object.hitbox.height - 100, this.pos[1] - object.pos[1]));
    this.forwardVel = 0;
    this.vel = [0, 0, 0];
    this.setAction(speed <= 10 ? ACT_GRAB_POLE_SLOW : ACT_GRAB_POLE_FAST, 0);
    return true;
  }

  grabUsedObject(): void {
    if (this.heldObj || !this.usedObj) return;
    this.heldObj = this.usedObj;
    this.heldObj.setHeldState(HELD_HELD);
  }

  /** mario_drop_held_object */
  dropHeldObject(): void {
    const object = this.heldObj;
    if (!object) return;
    object.setHeldState(HELD_DROPPED);
    const yaw = this.faceAngle[1];
    object.pos = [
      this.pos[0] + 80 * sins(yaw),
      this.pos[1],
      this.pos[2] + 80 * coss(yaw),
    ];
    object.moveYaw = yaw;
    object.faceYaw = yaw;
    object.forwardVel = 0;
    object.vel[1] = 0;
    this.heldObj = null;
  }

  /** mario_throw_held_object */
  throwHeldObject(): void {
    const object = this.heldObj;
    if (!object) return;
    object.setHeldState(HELD_THROWN);
    const yaw = this.faceAngle[1];
    object.pos = [
      this.pos[0] + 180 * sins(yaw),
      this.pos[1] + 75,
      this.pos[2] + 180 * coss(yaw),
    ];
    object.moveYaw = yaw;
    object.faceYaw = yaw;
    object.forwardVel = 25;
    object.vel[1] = 20;
    this.heldObj = null;
  }

  // -------------------------------------------------------------------------
  // Actions / transitions
  // -------------------------------------------------------------------------

  setAction(action: number, actionArg = 0): true {
    if (this.heldObj && !HOLDING_ACTIONS.has(action)) this.dropHeldObject();

    switch (action & 0x1c0) {
      case 0x40:
        action = this.setMarioActionMoving(action);
        break;
      case 0x80:
        action = this.setMarioActionAirborne(action, actionArg);
        break;
    }

    this.flags &= ~(0x10000 | 0x20000);

    if (!(this.action & ACT_FLAG_AIR)) {
      this.flags &= ~0x40000;
    }

    if (this.transitions.length < 60) {
      this.transitions.push({
        from: this.prevAction,
        to: action,
        input: this.input,
        floorN: this.floor ? [...this.floor.normal] : null,
        floorType: this.floor?.type ?? -1,
        pos: [...this.pos],
        fv: this.forwardVel,
        intended: this.intendedYaw,
        face: this.faceAngle[1],
      });
    }

    this.prevAction = this.action;
    this.action = action;
    this.actionArg = actionArg;
    this.actionState = 0;
    this.actionTimer = 0;
    this.animFrame = 0;
    this.animLength = 1;

    if (!(action & ACT_FLAG_AIR)) {
      this.jumpCombo = 0;
    }

    switch (action) {
      case ACT_JUMP:
        this.jumpCombo = 1;
        this.events.push({ type: 'jump', action, vy: this.vel[1] });
        break;
      case ACT_DOUBLE_JUMP:
        this.jumpCombo = 2;
        this.events.push({ type: 'jump', action, vy: this.vel[1] });
        break;
      case ACT_TRIPLE_JUMP:
        this.jumpCombo = 3;
        this.events.push({ type: 'triplejump' });
        break;
      case ACT_BACKFLIP:
      case ACT_SIDE_FLIP:
      case ACT_WALL_KICK_AIR:
      case ACT_JUMP_KICK:
      case ACT_LONG_JUMP:
      case ACT_DIVE:
        if (action === ACT_WALL_KICK_AIR) {
          this.wallKicks++;
          this.events.push({ type: 'wallkick' });
        } else if (action === ACT_LONG_JUMP) {
          this.events.push({ type: 'longjump' });
        } else if (action === ACT_DIVE) {
          this.events.push({ type: 'dive' });
        } else {
          this.events.push({ type: 'jump', action, vy: this.vel[1] });
        }
        break;
      case ACT_GROUND_POUND:
        this.events.push({ type: 'groundpound' });
        break;
      case ACT_GROUND_POUND_LAND:
        this.events.push({ type: 'groundpoundland' });
        break;
    }

    return true;
  }

  private setMarioActionMoving(action: number): number {
    const floorClass = marioGetFloorClass(this);
    const forwardVel = this.forwardVel;
    const mag = Math.min(this.intendedMag, 8);

    switch (action) {
      case ACT_WALKING:
        if (floorClass !== 0x13) {
          if (0 <= forwardVel && forwardVel < mag) {
            this.forwardVel = mag;
          }
        }
        break;

      case ACT_HOLD_WALKING:
        if (0 <= forwardVel && forwardVel < mag / 2) {
          this.forwardVel = mag / 2;
        }
        break;

      case ACT_BEGIN_SLIDING:
        action = marioFacingDownhillForSlide(this) ? ACT_BUTT_SLIDE : ACT_STOMACH_SLIDE;
        break;
    }

    return action;
  }

  private setMarioActionAirborne(action: number, _actionArg: number): number {
    if (
      (this.squishTimer !== 0 || this.quicksandDepth >= 1) &&
      (action === ACT_DOUBLE_JUMP || action === 0x108008a4)
    ) {
      action = ACT_JUMP;
    }

    switch (action) {
      case ACT_DOUBLE_JUMP:
        this.setYVelBasedOnFspeed(52, 0.25);
        this.forwardVel *= 0.8;
        break;

      case ACT_BACKFLIP:
        this.forwardVel = -16;
        this.setYVelBasedOnFspeed(62, 0);
        break;

      case ACT_TRIPLE_JUMP:
        this.setYVelBasedOnFspeed(69, 0);
        this.forwardVel *= 0.8;
        break;

      case ACT_JUMP:
      case ACT_HOLD_JUMP:
        this.setYVelBasedOnFspeed(42, 0.25);
        this.forwardVel *= 0.8;
        break;

      case ACT_WALL_KICK_AIR:
        this.setYVelBasedOnFspeed(62, 0);
        if (this.forwardVel < 24) this.forwardVel = 24;
        this.wallKickTimer = 0;
        break;

      case ACT_SIDE_FLIP:
        this.setYVelBasedOnFspeed(62, 0);
        this.forwardVel = 8;
        this.faceAngle[1] = this.intendedYaw;
        break;

      case ACT_STEEP_JUMP:
        this.setYVelBasedOnFspeed(42, 0.25);
        this.faceAngle[0] = -0x2000;
        break;

      case ACT_DIVE: {
        let forwardVel = this.forwardVel + 15;
        if (forwardVel > 48) forwardVel = 48;
        marioSetForwardVel(this, forwardVel);
        break;
      }

      case ACT_LONG_JUMP:
        this.longJumpIsSlow = this.forwardVel <= 16;
        this.setYVelBasedOnFspeed(30, 0);
        this.forwardVel *= 1.5;
        if (this.forwardVel > 48) this.forwardVel = 48;
        break;

      case ACT_SLIDE_KICK:
        this.vel[1] = 12;
        if (this.forwardVel < 32) this.forwardVel = 32;
        break;

      case ACT_JUMP_KICK:
        this.vel[1] = 20;
        break;
    }

    this.peakHeight = this.pos[1];
    this.flags |= MARIO_UNKNOWN_08;
    return action;
  }

  private setYVelBasedOnFspeed(initialVelY: number, multiplier: number): void {
    this.vel[1] = initialVelY + this.forwardVel * multiplier;
    if (this.squishTimer !== 0 || this.quicksandDepth > 1) {
      this.vel[1] *= 0.5;
    }
  }

  /** set_jump_from_landing */
  setJumpFromLanding(): true {
    if (marioFloorIsSteep(this)) {
      this.setSteepJumpAction();
    } else if (this.doubleJumpTimer === 0 || this.squishTimer !== 0) {
      this.setAction(ACT_JUMP, 0);
    } else {
      switch (this.prevAction) {
        case 0x04000470: // ACT_JUMP_LAND
        case 0x04000471: // ACT_FREEFALL_LAND
        case 0x0c000233: // ACT_SIDE_FLIP_LAND_STOP
          this.setAction(ACT_DOUBLE_JUMP, 0);
          break;

        case 0x04000472: // ACT_DOUBLE_JUMP_LAND
          if (this.forwardVel > 20) {
            this.setAction(ACT_TRIPLE_JUMP, 0);
          } else {
            this.setAction(ACT_JUMP, 0);
          }
          break;

        default:
          this.setAction(ACT_JUMP, 0);
          break;
      }
    }

    this.doubleJumpTimer = 0;
    return true;
  }

  /** set_jumping_action */
  setJumpingAction(action: number, actionArg: number): true {
    if (marioFloorIsSteep(this)) {
      this.setSteepJumpAction();
    } else {
      this.setAction(action, actionArg);
    }
    return true;
  }

  /** set_steep_jump_action */
  setSteepJumpAction(): void {
    if (this.forwardVel > 0) {
      const angleTemp = toS16(this.floorAngle + 0x8000);
      const faceAngleTemp = toS16(this.faceAngle[1] - angleTemp);
      const y = Math.sin(faceAngleTemp / 32768 * Math.PI) * this.forwardVel;
      const x = Math.cos(faceAngleTemp / 32768 * Math.PI) * this.forwardVel * 0.75;
      this.forwardVel = Math.sqrt(y * y + x * x);
      this.faceAngle[1] = toS16(atan2s(x, y) + angleTemp);
    }
    this.setAction(ACT_STEEP_JUMP, 0);
  }

  /** drop_and_set_mario_action (no objects to drop in the playground) */
  dropAndSetAction(action: number, actionArg = 0): true {
    return this.setAction(action, actionArg);
  }

  /** check_common_action_exits */
  checkCommonActionExits(): boolean {
    if (this.input & INPUT_A_PRESSED) {
      return this.setAction(ACT_JUMP, 0);
    }
    if (this.input & INPUT_OFF_FLOOR) {
      return this.setAction(ACT_FREEFALL, 0);
    }
    if (this.input & INPUT_NONZERO_ANALOG) {
      return this.setAction(ACT_WALKING, 0);
    }
    if (this.input & INPUT_ABOVE_SLIDE) {
      return this.setAction(ACT_BEGIN_SLIDING, 0);
    }
    return false;
  }

  /** mario_push_off_steep_floor */
  pushOffSteepFloor(action: number, actionArg = 0): true {
    const floorDYaw = toS16(this.floorAngle - this.faceAngle[1]);
    if (floorDYaw > -0x4000 && floorDYaw < 0x4000) {
      this.forwardVel = 16;
      this.faceAngle[1] = this.floorAngle;
    } else {
      this.forwardVel = -16;
      this.faceAngle[1] = toS16(this.floorAngle + 0x8000);
    }
    return this.setAction(action, actionArg);
  }

  /** mario_get_floor_class convenience used by actions. */
  floorClass(): number {
    return marioGetFloorClass(this);
  }

  floorIsSteep(): boolean {
    return marioFloorIsSteep(this);
  }

  floorIsSlippery(): boolean {
    return marioFloorIsSlippery(this);
  }

  floorHeightRelativePolar(angleFromMario: number, dist: number): number {
    return findFloorHeightRelativePolar(this, this.world, angleFromMario, dist);
  }

  /** mario_bonk_reflection */
  bonkReflection(negateSpeed: boolean): void {
    const speed = this.forwardVel;
    if (this.wall !== null) {
      const wallAngle = atan2s(this.wall.normal[2], this.wall.normal[0]);
      this.faceAngle[1] = toS16(wallAngle - toS16(this.faceAngle[1] - wallAngle));
    }
    if (negateSpeed) {
      marioSetForwardVel(this, -this.forwardVel);
    } else {
      this.faceAngle[1] = toS16(this.faceAngle[1] + 0x8000);
    }
    this.events.push({ type: 'bonk', speed });
  }

  stepGround(): number {
    return performGroundStep(this, this.world);
  }

  stepAir(stepArg = 0): number {
    return performAirStep(this, this.world, stepArg);
  }

  landEvent(vy: number, speed: number): void {
    this.lands++;
    this.events.push({ type: 'land', speed, vy });
  }

  airStepArg(): number {
    return AIR_STEP_CHECK_LEDGE_GRAB;
  }
}

/**
 * Small local copy of mario_facing_downhill for the BEGIN_SLIDING transition.
 * (Kept here to avoid a circular import with the action modules.)
 */
function marioFacingDownhillForSlide(m: MarioRuntime): boolean {
  const faceAngleYaw = toS16(m.floorAngle - m.faceAngle[1]);
  return -0x4000 < faceAngleYaw && faceAngleYaw < 0x4000;
}
