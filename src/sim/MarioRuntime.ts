import { linkGuarding, stepLink } from './linkController';
import { submergedStep } from './actions/submerged';
import { ACT_FALL_AFTER_STAR_GRAB, ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_NO_EXIT, ACT_STAR_DANCE_WATER, ACT_FLAG_SWIMMING, ACT_WATER_PLUNGE } from './state';
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
import {
  SURFACE_BURNING,
  SURFACE_DEATH_PLANE,
  SURFACE_DEEP_MOVING_QUICKSAND,
  SURFACE_DEEP_QUICKSAND,
  SURFACE_INSTANT_MOVING_QUICKSAND,
  SURFACE_INSTANT_QUICKSAND,
  SURFACE_SHALLOW_MOVING_QUICKSAND,
  SURFACE_SHALLOW_QUICKSAND,
  isQuicksand,
} from './surfaces';
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
  ACT_FLYING,
  ACT_GRABBED,
  ACT_RIDING_SHELL_GROUND,
  ACT_FLAG_RIDING_SHELL,
  ACT_RIDING_SHELL_JUMP,
  ACT_METAL_WATER_FALLING,
  ACT_METAL_WATER_JUMP,
  ACT_METAL_WATER_STANDING,
  ACT_METAL_WATER_WALKING,
  ACT_FREEFALL,
  ACT_GROUND_POUND, ACT_GROUND_POUND_LAND,
  ACT_IDLE,
  ACT_SPECIAL_EXIT_AIRBORNE, ACT_SPAWN_SPIN_AIRBORNE,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_BACKFLIP,
  ACT_LONG_JUMP,
  ACT_SIDE_FLIP,
  ACT_SLIDE_KICK,
  ACT_STEEP_JUMP,
  ACT_FLYING_TRIPLE_JUMP, ACT_TRIPLE_JUMP,
  ACT_WALL_KICK_AIR,
  ACT_TOP_OF_POLE_JUMP,
  MARIO_WING_CAP,
  MARIO_METAL_CAP,
  MARIO_VANISH_CAP,
  ACT_BEGIN_SLIDING, ACT_HOLD_BEGIN_SLIDING, ACT_HOLD_BUTT_SLIDE, ACT_HOLD_STOMACH_SLIDE, ACT_HOLD_BUTT_SLIDE_AIR, ACT_HOLD_BUTT_SLIDE_STOP, ACT_DIVE_PICKING_UP,
  ACT_BUTT_SLIDE,
  ACT_STOMACH_SLIDE,
  ACT_WALKING,
  ACT_HOLD_JUMP,
  ACT_HOLD_WALKING,
  ACT_FLAG_AIR,
  ACT_ID_MASK,
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
  ACT_JUMBO_STAR_CUTSCENE, ACT_STANDING_DEATH,
  ACT_DEATH_ON_BACK,
  ACT_DROWNING,
  ACT_WATER_DEATH,
  ACT_GRAB_POLE_SLOW,
  ACT_LAVA_BOOST,
  ACT_QUICKSAND_DEATH,
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
  ACT_PICKING_UP_BOWSER, ACT_HOLDING_BOWSER, ACT_RELEASING_BOWSER,
  ACT_PICKING_UP,
  ACT_PLACING_DOWN,
  ACT_THROWING,
  ACT_AIR_THROW,
} from './state';
import { HELD_DROPPED, HELD_FREE, HELD_HELD, HELD_THROWN, type SimObject } from './object';
import { performAirStep, performGroundStep } from './marioStep';

export interface SpawnPoint {
  action?: number;
  pos: Vec3;
  faceYaw: number;
}

const MAX_ACTION_ITERATIONS = 12;

/** Actions that may keep an object in Mario's hands. */
const HOLDING_ACTIONS = new Set([
  ACT_HOLD_BEGIN_SLIDING, ACT_HOLD_BUTT_SLIDE, ACT_HOLD_STOMACH_SLIDE, ACT_HOLD_BUTT_SLIDE_AIR, ACT_HOLD_BUTT_SLIDE_STOP, ACT_DIVE_PICKING_UP,
  ACT_PICKING_UP_BOWSER, ACT_HOLDING_BOWSER, ACT_RELEASING_BOWSER,
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
  get slideTerrain(): boolean { return this.world.slideTerrain; }
  world: CollisionWorld;
  spawn: SpawnPoint;

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
  twirlYaw = 0;
  angleVel: [number, number, number] = [0, 0, 0];
  stickX = 0;
  cannon: import('./actions/cannon').CannonSeat | null = null;
  wingCapTimer = 0;
  ridingShell: SimObject | null = null;
  shellRoll = 0;
  grabbedBy: SimObject | null = null;
  grabTimer = 0;
  metalCapTimer = 0;
  vanishCapTimer = 0;

  longJumpIsSlow = false;
  forwardVel = 0;
  slideVelX = 0;
  slideVelZ = 0;
  slideYaw = 0;
  movementMode: 'default' | 'side' | 'topdown' = 'default';
  /** Side-scrolling: world yaw that stick-right moves toward (0x4000 = +X). */
  sideYaw = 0x4000;
  gravityScale = 1;
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
  signMotion: Vec3 = [0,0,0];
  usedObj: SimObject | null = null;
  polePos = 0;
  poleYawVelocity = 0;
  stickY = 0;
  interactObj: SimObject | null = null;
  interactGrabbable = false;
  nearGrabbable = false;
  nearBreakable = false;

  private actionLoops = 0;

  constructor(world: CollisionWorld, spawn: SpawnPoint, private deathHeight = -1600) {
    this.world = world;
    this.spawn = { ...spawn, pos: [...spawn.pos] };
    this.resetToSpawn();
  }

  changeArea(world: CollisionWorld, spawn: SpawnPoint, deathHeight: number, offset: Vec3): void {
    this.world = world;
    this.spawn = { ...spawn, pos: [...spawn.pos] };
    this.deathHeight = deathHeight;
    for (let axis = 0; axis < 3; axis++) this.pos[axis] += offset[axis];
    this.peakHeight += offset[1];
    const floor = world.findFloor(...this.pos);
    this.floor = floor.surface;
    this.floorHeight = floor.height;
    this.floorAngle = this.floor ? atan2s(this.floor.normal[2], this.floor.normal[0]) : 0;
    const ceil = world.vec3fFindCeil(this.pos, floor.height);
    this.ceil = ceil.surface;
    this.ceilHeight = ceil.height;
    this.wall = null;
  }

  resetToSpawn(): void {
    this.ridingShell = null;
    if (this.heldObj) this.heldObj.setHeldState(HELD_FREE);
    this.cannon = null;
    this.heldObj = null;
    this.usedObj = null;
    this.interactObj = null;
    this.interactGrabbable = false;
    this.nearGrabbable = false;
    this.nearBreakable = false;
    const [x, , z] = this.spawn.pos;
    const hit = this.world.findFloor(x, this.spawn.pos[1] + 200, z);
    this.pos = [x, this.spawn.action || !hit.surface ? this.spawn.pos[1] : hit.height, z];
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
    if (this.spawn.action === ACT_FLYING) {
      this.grantWingCap(1200);
      this.setAction(ACT_FLYING,2);
    } else if (this.spawn.action) this.setAction(this.spawn.action);
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
  step(controller: ControllerSnapshot, cameraYaw: number, asLink = false): void {
    this.events.length = 0;
    this.controller = controller;
    this.cameraYaw = cameraYaw;

    // Scripted flight crosses empty space outside the final arena.
    if (this.action === ACT_JUMBO_STAR_CUTSCENE && this.actionArg > 0) {
      executeMarioAction(this);
      return;
    }
    if (!this.updateMarioInputs()) {
      return;
    }

    if (this.wingCapTimer > 0) this.wingCapTimer--;
    if (this.metalCapTimer > 0) this.metalCapTimer--;
    if (this.vanishCapTimer > 0) this.vanishCapTimer--;
    this.tickCap(this.wingCapTimer, MARIO_WING_CAP, () => { this.wingCapTimer = 0; });
    this.tickCap(this.metalCapTimer, MARIO_METAL_CAP, () => { this.metalCapTimer = 0; });
    this.tickCap(this.vanishCapTimer, MARIO_VANISH_CAP, () => { this.vanishCapTimer = 0; });

    if (this.grabbedBy) {
      this.stepGrabbed();
      this.checkVoid();
      this.updateMarioHealth();
      this.animFrame++;
      if (this.animFrame > this.animLength) this.animFrame = 0;
      return;
    }

    const surface = this.world.waterLevel(this.pos[0], this.pos[2]);
    if ([ACT_FALL_AFTER_STAR_GRAB, ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_NO_EXIT, ACT_STAR_DANCE_WATER].includes(this.action)) {
      executeMarioAction(this);
      return;
    }
    if (this.metalCapTimer > 0 && this.pos[1] < surface - 20) {
      this.stepMetalWater();
      this.checkVoid();
      this.updateMarioHealth();
      this.animFrame++;
      if (this.animFrame > this.animLength) this.animFrame = 0;
      return;
    }
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


    let inLoop = !(asLink && stepLink(this));
    this.actionLoops = 0;
    while (inLoop && this.actionLoops++ < MAX_ACTION_ITERATIONS) {
      inLoop = executeMarioAction(this);
    }

    if (this.ridingShell) {
      this.ridingShell.pos = [...this.pos];
      this.ridingShell.faceYaw = this.faceAngle[1];
    }
    this.updateSurfaceHazards();
    this.checkVoid();
    this.updateMarioHealth();
    if (this.squishTimer > 0) this.squishTimer--;
    this.animFrame++;
    if (this.animFrame > this.animLength) this.animFrame = 0;
  }

  /** check_lava_boost / mario_update_quicksand from interaction.c and mario_step.c. */
  private updateSurfaceHazards(): void {
    if (!this.floor || (this.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE | ACT_FLAG_RIDING_SHELL))) return;

    if (this.floor.type === SURFACE_BURNING && this.pos[1] < this.floorHeight + 10) {
      this.hurtCounter += 18;
      this.vel[1] = 84;
      this.events.push({ type: 'hurt', damage: 0 });
      this.setAction(ACT_LAVA_BOOST, 0);
      return;
    }

    if (this.pos[1] > this.floorHeight + 1 || !isQuicksand(this.floor.type)) return;
    const type = this.floor.type;
    if (type === SURFACE_INSTANT_QUICKSAND || type === SURFACE_INSTANT_MOVING_QUICKSAND) {
      this.quicksandDepth = 180;
      this.events.push({ type: 'death' });
      this.setAction(ACT_QUICKSAND_DEATH, 0);
      return;
    }

    this.quicksandDepth = Math.max(1.1, this.quicksandDepth + 0.5);
    const cap = type === SURFACE_SHALLOW_QUICKSAND
      ? 10
      : type === SURFACE_SHALLOW_MOVING_QUICKSAND
        ? 25
        : (type === SURFACE_DEEP_MOVING_QUICKSAND || type === SURFACE_DEEP_QUICKSAND)
          ? 160
          : 60;
    this.quicksandDepth = Math.min(cap, this.quicksandDepth);
    if (this.quicksandDepth >= 160) {
      this.events.push({ type: 'death' });
      this.setAction(ACT_QUICKSAND_DEATH, 0);
    }
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
        if ((this.action & ACT_FLAG_SWIMMING) && !(this.action & ACT_FLAG_INTANGIBLE)) {
          const surface = this.world.waterLevel(this.pos[0], this.pos[2]);
          if (this.pos[1] >= surface - 140 && !this.world.snowTerrain) this.health += 0x1a;
          else this.health -= this.world.snowTerrain ? 3 : 1;
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
    if (this.action & (ACT_FLAG_INVULNERABLE | ACT_FLAG_INTANGIBLE) || this.invincTimer > 0 || this.vanishCapTimer > 0) return false;


    const angleToObject = sourcePosition
      ? atan2s(sourcePosition[2] - this.pos[2], sourcePosition[0] - this.pos[0])
      : toS16(this.faceAngle[1] + 0x8000);
    const facingDYaw = toS16(angleToObject - this.faceAngle[1]);
    if (sourcePosition && linkGuarding(this) && Math.abs(facingDYaw) < 0x4000) return false;
    this.hurtCounter += 4 * damage;
    const remainingHealth = this.health - 0x40 * this.hurtCounter;
    let strength = remainingHealth < 0x100 || damage >= 4 ? 2 : damage >= 2 ? 1 : 0;
    if (this.metalCapTimer > 0) strength = Math.max(0, strength - 1);
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
    if ((this.floor?.type === SURFACE_DEATH_PLANE && this.pos[1] <= this.floorHeight + 10) || this.pos[1] < this.deathHeight || Math.abs(this.pos[0]) >= this.world.horizontalBoundary || Math.abs(this.pos[2]) >= this.world.horizontalBoundary) {
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
    this.stickX = c.stickX;
    this.stickY = c.stickY;

    if (this.movementMode === 'side') {
      const mag = (Math.abs(c.stickX) / 64) * (Math.abs(c.stickX) / 64) * 64;
      this.intendedMag = this.squishTimer === 0 ? mag / 2 : mag / 8;
      if (this.intendedMag > 0) {
        this.intendedYaw = toS16(c.stickX > 0 ? this.sideYaw : this.sideYaw + 0x8000);
        this.input |= INPUT_NONZERO_ANALOG;
      } else {
        this.intendedYaw = this.faceAngle[1];
      }
      return;
    }

    const mag = (c.stickMag / 64) * (c.stickMag / 64) * 64;
    this.intendedMag = this.squishTimer === 0 ? mag / 2 : mag / 8;

    if (this.intendedMag > 0) {
      const yaw = this.movementMode === 'topdown'
        ? atan2s(-c.stickY, c.stickX)
        : atan2s(-c.stickY, c.stickX) + this.cameraYaw;
      this.intendedYaw = toS16(yaw);
      this.input |= INPUT_NONZERO_ANALOG;
    } else {
      this.intendedYaw = this.faceAngle[1];
    }
  }

  private updateMarioGeometryInputs(): boolean {
    this.world.f32FindWallCollision(this.pos, 60, 50);
    this.world.f32FindWallCollision(this.pos, 30, 24);

    const floorHit = this.world.findMovementFloor(this.pos,this.action);
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
    if (this.interactObj.behavior === 'bowser') {
      const yaw = this.interactObj.moveYaw;
      if (Math.abs(toS16(this.faceAngle[1] - yaw)) > 0x5555) return false;
      this.faceAngle[1] = yaw;
      this.usedObj = this.interactObj;
      return this.setAction(ACT_PICKING_UP_BOWSER, 0);
    }
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
    const id = this.action & ACT_ID_MASK;
    if (id < 0x80 || id >= 0xa0 || ((this.prevAction & ACT_FLAG_ON_POLE) && this.usedObj === object)) return false;
    const speed = this.forwardVel;
    this.dropHeldObject();
    this.poleYawVelocity = speed <= 10 ? 0 : 0x1000;
    this.usedObj = object;
    this.polePos = this.pos[1] - object.pos[1];
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

  /** Simplified metal-water mode: sink to the bottom and walk there. */
  private stepMetalWater(): void {
    const floor = this.world.findFloor(this.pos[0], this.pos[1] + 100, this.pos[2]);
    const bottom = floor.surface ? floor.height : null;
    if (bottom === null) {
      this.vel[1] -= 5;
      this.pos[1] += this.vel[1];
      this.setAction(ACT_METAL_WATER_FALLING, 0);
      return;
    }

    if (this.pos[1] > bottom + 4) {
      this.vel[1] -= 5;
      this.pos[1] += this.vel[1];
      this.setAction(ACT_METAL_WATER_FALLING, 0);
      if (this.pos[1] <= bottom) {
        this.pos[1] = bottom;
        this.vel[1] = 0;
      }
      return;
    }

    this.pos[1] = bottom;
    this.vel[1] = 0;
    this.forwardVel = 0;
    if (this.controller.buttonPressed & A_BUTTON) {
      this.vel[1] = 34;
      this.pos[1] += 2;
      this.setAction(ACT_METAL_WATER_JUMP, 0);
      return;
    }

    if (this.intendedMag > 5) {
      const speed = Math.min(7, this.intendedMag / 4);
      const yaw = this.intendedYaw;
      this.faceAngle[1] = yaw;
      this.pos[0] += Math.sin(yaw * Math.PI * 2 / 65536) * speed;
      this.pos[2] += Math.cos(yaw * Math.PI * 2 / 65536) * speed;
      this.setAction(ACT_METAL_WATER_WALKING, 0);
    } else {
      this.setAction(ACT_METAL_WATER_STANDING, 0);
    }
  }

  startKingGrab(king: SimObject): boolean {
    if (this.grabbedBy || this.ridingShell || (this.action & ACT_FLAG_AIR)) return false;
    this.grabbedBy = king;
    this.grabTimer = 45;
    this.pos = [king.pos[0], king.pos[1] + 80, king.pos[2]];
    this.vel = [0, 0, 0];
    this.forwardVel = 0;
    this.setAction(ACT_GRABBED, 0);
    return true;
  }

  private stepGrabbed(): void {
    const king = this.grabbedBy;
    if (!king || !king.active) {
      this.grabbedBy = null;
      this.setAction(ACT_FREEFALL, 0);
      return;
    }
    this.pos = [
      king.pos[0] + Math.sin(king.moveYaw * Math.PI * 2 / 65536) * 120,
      king.pos[1] + 100,
      king.pos[2] + Math.cos(king.moveYaw * Math.PI * 2 / 65536) * 120,
    ];
    this.faceAngle[1] = king.moveYaw;
    this.vel = [0, 0, 0];
    this.setAction(ACT_GRABBED, 0);
  }

  throwFromGrab(king: SimObject): void {
    if (this.grabbedBy !== king) return;
    this.grabbedBy = null;
    this.pos[1] += 100;
    this.vel = [0, 30, 0];
    this.setAction(ACT_FREEFALL, 0);
    this.takeDamage(1, king.pos);
  }

  rideShell(shell: SimObject): void {
    this.shellRoll = 0;
    this.quicksandDepth = 0;
    this.ridingShell = shell;
    this.dropHeldObject();
    this.setAction(ACT_RIDING_SHELL_GROUND);
  }

  get vanished(): boolean {
    return (this.flags & MARIO_VANISH_CAP) !== 0;
  }

  grantWingCap(frames = 900): void {
    this.wingCapTimer = Math.max(this.wingCapTimer, frames);
    this.flags |= MARIO_WING_CAP;
  }

  grantMetalCap(frames = 900): void {
    this.metalCapTimer = Math.max(this.metalCapTimer, frames);
    this.flags |= MARIO_METAL_CAP;
  }

  grantVanishCap(frames = 900): void {
    this.vanishCapTimer = Math.max(this.vanishCapTimer, frames);
    this.flags |= MARIO_VANISH_CAP;
  }

  private tickCap(timer: number, flag: number, expire: () => void): void {
    if (timer > 0) {
      this.flags |= flag;
      if (timer === 1) expire();
    } else {
      this.flags &= ~flag;
    }
  }

  setAction(action: number, actionArg = 0): true {
    if (this.ridingShell && !(action & ACT_FLAG_RIDING_SHELL)) {
      this.ridingShell.deactivate();
      this.ridingShell = null;
    }
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
      case ACT_FALL_AFTER_STAR_GRAB:
        marioSetForwardVel(this, 0);
        break;
      case ACT_SPECIAL_EXIT_AIRBORNE:
        this.vel[1] = 64;
        break;
      case ACT_SPAWN_SPIN_AIRBORNE:
        marioSetForwardVel(this, 2);
        break;
      case ACT_JUMP:
        this.jumpCombo = 1;
        this.events.push({ type: 'jump', action, vy: this.vel[1] });
        break;
      case ACT_DOUBLE_JUMP:
        this.jumpCombo = 2;
        this.events.push({ type: 'jump', action, vy: this.vel[1] });
        break;
      case ACT_FLYING_TRIPLE_JUMP:
      case ACT_TRIPLE_JUMP:
        this.jumpCombo = 3;
        this.events.push({ type: 'triplejump' });
        break;
      case ACT_BACKFLIP:
      case ACT_SIDE_FLIP:
      case ACT_TOP_OF_POLE_JUMP:
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

      case ACT_HOLD_BEGIN_SLIDING:
        action = marioFacingDownhillForSlide(this) ? ACT_HOLD_BUTT_SLIDE : ACT_HOLD_STOMACH_SLIDE;
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

      case ACT_FLYING_TRIPLE_JUMP:
        this.setYVelBasedOnFspeed(82,0);
        break;
      case ACT_TRIPLE_JUMP:
        this.setYVelBasedOnFspeed(69, 0);
        this.forwardVel *= 0.8;
        break;

      case ACT_RIDING_SHELL_JUMP:
        this.setYVelBasedOnFspeed(42,0.25);
        break;

      case ACT_JUMP:
      case ACT_HOLD_JUMP:
        this.setYVelBasedOnFspeed(42, 0.25);
        this.forwardVel *= 0.8;
        break;

      case ACT_TOP_OF_POLE_JUMP:
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
          if (this.flags & MARIO_WING_CAP) this.setAction(ACT_FLYING_TRIPLE_JUMP);
          else if (this.forwardVel > 20) {
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
