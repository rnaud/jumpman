/**
 * Action IDs, flags and state for the Mario 64 controller port.
 * Action values are copied verbatim from include/sm64.h so that traces line up
 * with the decompilation.
 */

import type { Surface } from './collision';
import type { Vec3 } from './math';

// ---------------------------------------------------------------------------
// Controller
// ---------------------------------------------------------------------------

export const A_BUTTON = 0x8000;
export const B_BUTTON = 0x4000;
export const Z_TRIG = 0x2000;

export interface ControllerSnapshot {
  /** [-64, 64], positive right */
  stickX: number;
  /** [-64, 64], positive up */
  stickY: number;
  /** [0, 64] */
  stickMag: number;
  buttonDown: number;
  buttonPressed: number;
}

export function emptyController(): ControllerSnapshot {
  return { stickX: 0, stickY: 0, stickMag: 0, buttonDown: 0, buttonPressed: 0 };
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

export const INPUT_NONZERO_ANALOG = 0x0001;
export const INPUT_A_PRESSED = 0x0002;
export const INPUT_OFF_FLOOR = 0x0004;
export const INPUT_ABOVE_SLIDE = 0x0008;
export const INPUT_FIRST_PERSON = 0x0010;
export const INPUT_UNKNOWN_5 = 0x0020;
export const INPUT_SQUISHED = 0x0040;
export const INPUT_A_DOWN = 0x0080;
export const INPUT_IN_POISON_GAS = 0x0100;
export const INPUT_IN_WATER = 0x0200;
export const INPUT_STOMPED = 0x0400;
export const INPUT_INTERACT_OBJ_GRABBABLE = 0x0800;
export const INPUT_B_PRESSED = 0x2000;
export const INPUT_Z_DOWN = 0x4000;
export const INPUT_Z_PRESSED = 0x8000;

// ---------------------------------------------------------------------------
// Action flags / groups
// ---------------------------------------------------------------------------

export const ACT_ID_MASK = 0x000001ff;
export const ACT_GROUP_MASK = 0x000001c0;
export const ACT_GROUP_STATIONARY = 0 << 6;
export const ACT_GROUP_MOVING = 1 << 6;
export const ACT_GROUP_AIRBORNE = 2 << 6;
export const ACT_GROUP_SUBMERGED = 3 << 6;
export const ACT_GROUP_CUTSCENE = 4 << 6;
export const ACT_GROUP_AUTOMATIC = 5 << 6;
export const ACT_GROUP_OBJECT = 6 << 6;

export const ACT_FLAG_STATIONARY = 1 << 9;
export const ACT_FLAG_MOVING = 1 << 10;
export const ACT_FLAG_AIR = 1 << 11;
export const ACT_FLAG_INTANGIBLE = 1 << 12;
export const ACT_FLAG_SWIMMING = 1 << 13;
export const ACT_FLAG_METAL_WATER = 1 << 14;
export const ACT_FLAG_SHORT_HITBOX = 1 << 15;
export const ACT_FLAG_RIDING_SHELL = 1 << 16;
export const ACT_FLAG_INVULNERABLE = 1 << 17;
export const ACT_FLAG_BUTT_OR_STOMACH_SLIDE = 1 << 18;
export const ACT_FLAG_DIVING = 1 << 19;
export const ACT_FLAG_ON_POLE = 1 << 20;
export const ACT_FLAG_HANGING = 1 << 21;
export const ACT_FLAG_IDLE = 1 << 22;
export const ACT_FLAG_ATTACKING = 1 << 23;
export const ACT_FLAG_ALLOW_VERTICAL_WIND_ACTION = 1 << 24;
export const ACT_FLAG_CONTROL_JUMP_HEIGHT = 1 << 25;
export const ACT_FLAG_ALLOW_FIRST_PERSON = 1 << 26;
export const ACT_FLAG_PAUSE_EXIT = 1 << 27;
export const ACT_FLAG_SWIMMING_OR_FLYING = 1 << 28;
export const ACT_FLAG_WATER_OR_TEXT = 1 << 29;
export const ACT_FLAG_THROWING = 1 << 31;

// ---------------------------------------------------------------------------
// Mario flags (only the ones relevant to movement are used)
// ---------------------------------------------------------------------------

export const MARIO_NORMAL_CAP = 1 << 0;
export const MARIO_VANISH_CAP = 1 << 1;
export const MARIO_METAL_CAP = 1 << 2;
export const MARIO_WING_CAP = 1 << 3;
export const MARIO_CAP_ON_HEAD = 1 << 4;
export const MARIO_CAP_IN_HAND = 1 << 5;
export const MARIO_METAL_SHOCK = 1 << 6;
export const MARIO_TELEPORTING = 1 << 7;
export const MARIO_UNKNOWN_08 = 1 << 8;
export const MARIO_ACTION_SOUND_PLAYED = 1 << 16;
export const MARIO_MARIO_SOUND_PLAYED = 1 << 17;
export const MARIO_UNKNOWN_18 = 1 << 18;
export const MARIO_PUNCHING = 1 << 20;
export const MARIO_KICKING = 1 << 21;
export const MARIO_TRIPPING = 1 << 22;

// ---------------------------------------------------------------------------
// Actions (verbatim values from include/sm64.h)
// ---------------------------------------------------------------------------

export const ACT_HARD_BACKWARD_GROUND_KB = 0x00020460;
export const ACT_HARD_FORWARD_GROUND_KB = 0x00020461;
export const ACT_SOFT_BACKWARD_GROUND_KB = 0x00020464;
export const ACT_SOFT_FORWARD_GROUND_KB = 0x00020465;
export const ACT_BACKWARD_GROUND_KB = 0x00020462;
export const ACT_FORWARD_GROUND_KB = 0x00020463;
export const ACT_BACKWARD_AIR_KB = 0x010208b0;
export const ACT_FORWARD_AIR_KB = 0x010208b1;
export const ACT_HARD_FORWARD_AIR_KB = 0x010208b2;
export const ACT_HARD_BACKWARD_AIR_KB = 0x010208b3;
export const ACT_SHOCKED = 0x00020338;
export const ACT_SPAWN_NO_SPIN_AIRBORNE = 0x00001932;
export const ACT_SPAWN_NO_SPIN_LANDING = 0x00001333;
export const ACT_SPAWN_SPIN_AIRBORNE = 0x00001924;
export const ACT_SPAWN_SPIN_LANDING = 0x00001325;
export const ACT_SPECIAL_EXIT_AIRBORNE = 0x0000192b;
export const ACT_EXIT_AIRBORNE = 0x00001926;
export const ACT_EXIT_LAND_SAVE_DIALOG = 0x00001327;
export const ACT_FALLING_EXIT_AIRBORNE = 0x0000192d;
export const ACT_ENTERING_STAR_DOOR = 0x00001331;
export const ACT_UNLOCKING_STAR_DOOR = 0x0000132f;
export const ACT_UNLOCKING_KEY_DOOR = 0x0000132e;
export const ACT_PULLING_DOOR = 0x00001320;
export const ACT_PUSHING_DOOR = 0x00001321;
export const ACT_WATER_JUMP = 0x01000889;
export const ACT_WATER_ACTION_END = 0x300022c2;
export const ACT_WATER_IDLE = 0x380022c0;
export const ACT_WATER_PLUNGE = 0x300022e2;
export const ACT_DROWNING = 0x300032c4;
export const ACT_WATER_DEATH = 0x300032c7;
export const ACT_BREASTSTROKE = 0x300024d0;
export const ACT_SWIMMING_END = 0x300024d1;
export const ACT_FLUTTER_KICK = 0x300024d2;
export const ACT_IDLE = 0x0c400201;
export const ACT_CROUCHING = 0x0c008220;
export const ACT_START_CROUCHING = 0x0c008221;
export const ACT_STOP_CROUCHING = 0x0c008222;
export const ACT_START_CRAWLING = 0x0c008223;
export const ACT_STOP_CRAWLING = 0x0c008224;
export const ACT_SLIDE_KICK_SLIDE_STOP = 0x08000225;
export const ACT_BACKFLIP_LAND_STOP = 0x0800022f;
export const ACT_JUMP_LAND_STOP = 0x0c000230;
export const ACT_DOUBLE_JUMP_LAND_STOP = 0x0c000231;
export const ACT_FREEFALL_LAND_STOP = 0x0c000232;
export const ACT_SIDE_FLIP_LAND_STOP = 0x0c000233;
export const ACT_TRIPLE_JUMP_LAND_STOP = 0x0800023a;
export const ACT_LONG_JUMP_LAND_STOP = 0x0800023b;
export const ACT_GROUND_POUND_LAND = 0x0080023c;
export const ACT_BRAKING_STOP = 0x0c00023d;
export const ACT_BUTT_SLIDE_STOP = 0x0c00023e;

export const ACT_WALKING = 0x04000440;
export const ACT_TURNING_AROUND = 0x00000443;
export const ACT_FINISH_TURNING_AROUND = 0x00000444;
export const ACT_BRAKING = 0x04000445;
export const ACT_CRAWLING = 0x04008448;
export const ACT_DECELERATING = 0x0400044a;
export const ACT_BEGIN_SLIDING = 0x00000050;
export const ACT_BUTT_SLIDE = 0x00840452;
export const ACT_STOMACH_SLIDE = 0x008c0453;
export const ACT_DIVE_SLIDE = 0x00880456;
export const ACT_MOVE_PUNCHING = 0x00800457;
export const ACT_CROUCH_SLIDE = 0x04808459;
export const ACT_SLIDE_KICK_SLIDE = 0x0080045a;
export const ACT_JUMP_LAND = 0x04000470;
export const ACT_FREEFALL_LAND = 0x04000471;
export const ACT_DOUBLE_JUMP_LAND = 0x04000472;
export const ACT_SIDE_FLIP_LAND = 0x04000473;
export const ACT_TRIPLE_JUMP_LAND = 0x04000478;
export const ACT_LONG_JUMP_LAND = 0x00000479;
export const ACT_BACKFLIP_LAND = 0x0400047a;

export const ACT_JUMP = 0x03000880;
export const ACT_DOUBLE_JUMP = 0x03000881;
export const ACT_TRIPLE_JUMP = 0x01000882;
export const ACT_BACKFLIP = 0x01000883;
export const ACT_STEEP_JUMP = 0x03000885;
export const ACT_WALL_KICK_AIR = 0x03000886;
export const ACT_SIDE_FLIP = 0x01000887;
export const ACT_LONG_JUMP = 0x03000888;
export const ACT_DIVE = 0x0188088a;
export const ACT_FREEFALL = 0x0100088c;
export const ACT_BUTT_SLIDE_AIR = 0x0300088e;
export const ACT_AIR_HIT_WALL = 0x000008a7;
export const ACT_GROUND_POUND = 0x008008a9;
export const ACT_SLIDE_KICK = 0x018008aa;
export const ACT_JUMP_KICK = 0x018008ac;
export const ACT_SOFT_BONK = 0x010208b6;
export const ACT_LAVA_BOOST = 0x010208b7;
export const ACT_GETTING_BLOWN = 0x010208b8;
export const ACT_IN_CANNON = 0x00001371;
export const ACT_SHOT_FROM_CANNON = 0x00880898;
export const ACT_FLYING = 0x10880899;
export const ACT_GRABBED = 0x00020370;
export const ACT_RIDING_SHELL_GROUND = 0x20810446;
export const ACT_RIDING_SHELL_JUMP = 0x0281089a;
export const ACT_METAL_WATER_STANDING = 0x080042f0;
export const ACT_METAL_WATER_WALKING = 0x000044f2;
export const ACT_METAL_WATER_FALLING = 0x000042f4;
export const ACT_METAL_WATER_JUMP = 0x000044f8;

export const ACT_LEDGE_GRAB = 0x0800034b;
export const ACT_LAVA_BOOST_LAND = 0x08000239;
export const ACT_QUICKSAND_DEATH = 0x00021312;
export const ACT_HOLDING_POLE = 0x08100340;
export const ACT_GRAB_POLE_SLOW = 0x00100341;
export const ACT_GRAB_POLE_FAST = 0x00100342;
export const ACT_CLIMBING_POLE = 0x00100343;
export const ACT_TOP_OF_POLE_TRANSITION = 0x00100344;
export const ACT_TOP_OF_POLE = 0x00100345;
export const ACT_START_HANGING = 0x08200348;
export const ACT_HANGING = 0x00200349;
export const ACT_HANG_MOVING = 0x0020054a;
export const ACT_LEDGE_CLIMB_SLOW_1 = 0x0000054c;
export const ACT_LEDGE_CLIMB_SLOW_2 = 0x0000054d;
export const ACT_LEDGE_CLIMB_FAST = 0x0000054f;
export const ACT_START_SLEEPING = 0x0c400202;
export const ACT_SLEEPING = 0x0c000203;
export const ACT_WAKING_UP = 0x0c000204;
export const ACT_JUMBO_STAR_CUTSCENE = 0x00001909;
export const ACT_END_PEACH_CUTSCENE = 0x00001918;
export const ACT_STANDING_DEATH = 0x00021311;
export const ACT_DEATH_ON_STOMACH = 0x00021315;
export const ACT_DEATH_ON_BACK = 0x00021316;

export const ACT_PUNCHING = 0x00800380;

export const ACT_PICKING_UP_BOWSER = 0x00000390;
export const ACT_HOLDING_BOWSER = 0x00000391;
export const ACT_RELEASING_BOWSER = 0x00000392;
export const ACT_PICKING_UP = 0x00000383;
export const ACT_PLACING_DOWN = 0x00000387;
export const ACT_THROWING = 0x80000588;
export const ACT_HOLD_IDLE = 0x08000207;
export const ACT_HOLD_WALKING = 0x00000442;
export const ACT_HOLD_DECELERATING = 0x0000044b;
export const ACT_HOLD_JUMP = 0x030008a0;
export const ACT_HOLD_FREEFALL = 0x010008a1;
export const ACT_AIR_THROW = 0x830008ab;
export const ACT_HOLD_JUMP_LAND = 0x00000474;
export const ACT_HOLD_FREEFALL_LAND = 0x00000475;
export const ACT_HOLD_JUMP_LAND_STOP = 0x08000234;
export const ACT_HOLD_FREEFALL_LAND_STOP = 0x08000235;
export const ACT_AIR_THROW_LAND = 0x80000a36;

// ---------------------------------------------------------------------------
// Step results
// ---------------------------------------------------------------------------

export const GROUND_STEP_LEFT_GROUND = 0;
export const GROUND_STEP_NONE = 1;
export const GROUND_STEP_HIT_WALL = 2;
export const GROUND_STEP_HIT_WALL_STOP_QSTEPS = 2;
export const GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS = 3;

export const AIR_STEP_CHECK_LEDGE_GRAB = 0x00000001;
export const AIR_STEP_CHECK_HANG = 0x00000002;

export const AIR_STEP_NONE = 0;
export const AIR_STEP_LANDED = 1;
export const AIR_STEP_HIT_WALL = 2;
export const AIR_STEP_GRABBED_LEDGE = 3;
export const AIR_STEP_GRABBED_CEILING = 4;
export const AIR_STEP_HIT_LAVA_WALL = 6;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

export interface MarioState {
  action: number;
  prevAction: number;
  actionState: number;
  actionTimer: number;
  actionArg: number;

  input: number;
  flags: number;
  particleFlags: number;

  pos: Vec3;
  vel: Vec3;
  faceAngle: [number, number, number];

  forwardVel: number;
  gravityScale?: number;
  angleVel?: [number, number, number];
  slideVelX: number;
  slideVelZ: number;
  slideYaw: number;
  intendedMag: number;
  intendedYaw: number;

  floor: Surface | null;
  ceil: Surface | null;
  wall: Surface | null;
  floorHeight: number;
  ceilHeight: number;
  floorAngle: number;

  peakHeight: number;
  quicksandDepth: number;
  squishTimer: number;

  doubleJumpTimer: number;
  wallKickTimer: number;
  framesSinceA: number;
  framesSinceB: number;

  health: number;
  hurtCounter: number;
  healCounter: number;

  // Purely visual / bookkeeping state. Not read by the physics.
  animFrame: number;
  animLength: number;
  jumpCombo: number;
  lands: number;
  wallKicks: number;
  respawns: number;
}

export type MarioEvent =
  | { type: 'jump'; action: number; vy: number }
  | { type: 'land'; speed: number; vy: number }
  | { type: 'bonk'; speed: number }
  | { type: 'wallkick' }
  | { type: 'groundpound' }
  | { type: 'cannonMove'; phase: 0 | 1 | 2 | 3 }
  | { type: 'cannonFire' | 'waterPlunge' | 'flutterKick' | 'bowserGrab' | 'bowserSpin' }
  | { type: 'bowserThrow'; fast: boolean }
  | { type: 'swimStroke'; fast: boolean }
  | { type: 'groundpounddrop' }
  | { type: 'groundpoundland' }
  | { type: 'death' }
  | { type: 'longjump' }
  | { type: 'triplejump' }
  | { type: 'dive' }
  | { type: 'step'; speed: number }
  | { type: 'castleEnding' | 'endingCredits' }
  | { type: 'respawn' }
  | { type: 'hurt'; damage: number };

export const ACTION_NAMES: Record<number, string> = {
  [ACT_SOFT_BACKWARD_GROUND_KB]: 'HIT',
  [ACT_SOFT_FORWARD_GROUND_KB]: 'HIT',
  [ACT_BACKWARD_GROUND_KB]: 'HIT',
  [ACT_FORWARD_GROUND_KB]: 'HIT',
  [ACT_BACKWARD_AIR_KB]: 'HIT',
  [ACT_FORWARD_AIR_KB]: 'HIT',
  [ACT_HARD_BACKWARD_AIR_KB]: 'HIT',
  [ACT_HARD_FORWARD_AIR_KB]: 'HIT',
  [ACT_HARD_BACKWARD_GROUND_KB]: 'HIT',
  [ACT_HARD_FORWARD_GROUND_KB]: 'HIT',
  [ACT_SHOCKED]: 'SHOCKED',
  [ACT_SPAWN_NO_SPIN_AIRBORNE]: 'WARP ARRIVAL',
  [ACT_SPAWN_NO_SPIN_LANDING]: 'WARP LANDING',
  [ACT_SPAWN_SPIN_AIRBORNE]: 'SPIN AIRBORNE',
  [ACT_SPAWN_SPIN_LANDING]: 'SPIN LANDING',
  [ACT_SPECIAL_EXIT_AIRBORNE]: 'LAUNCH EXIT',
  [ACT_EXIT_AIRBORNE]: 'PAINTING EXIT',
  [ACT_FALLING_EXIT_AIRBORNE]: 'FALLING EXIT',
  [ACT_EXIT_LAND_SAVE_DIALOG]: 'STAR EXIT LANDING',
  [ACT_PULLING_DOOR]: 'OPEN DOOR',
  [ACT_PUSHING_DOOR]: 'OPEN DOOR',
  [ACT_WATER_JUMP]: 'WATER JUMP',
  [ACT_WATER_ACTION_END]: 'TREAD WATER',
  [ACT_WATER_IDLE]: 'TREAD WATER',
  [ACT_WATER_PLUNGE]: 'WATER PLUNGE',
  [ACT_DROWNING]: 'DROWNING',
  [ACT_WATER_DEATH]: 'DROWNING',
  [ACT_BREASTSTROKE]: 'BREASTSTROKE',
  [ACT_SWIMMING_END]: 'SWIM GLIDE',
  [ACT_FLUTTER_KICK]: 'FLUTTER KICK',
  [ACT_IDLE]: 'IDLE',
  [ACT_CROUCHING]: 'CROUCHING',
  [ACT_START_CROUCHING]: 'START CROUCHING',
  [ACT_STOP_CROUCHING]: 'STOP CROUCHING',
  [ACT_START_CRAWLING]: 'START CRAWLING',
  [ACT_STOP_CRAWLING]: 'STOP CRAWLING',
  [ACT_SLIDE_KICK_SLIDE_STOP]: 'SLIDE KICK STOP',
  [ACT_BACKFLIP_LAND_STOP]: 'BACKFLIP LAND STOP',
  [ACT_JUMP_LAND_STOP]: 'JUMP LAND STOP',
  [ACT_DOUBLE_JUMP_LAND_STOP]: 'DOUBLE JUMP LAND STOP',
  [ACT_FREEFALL_LAND_STOP]: 'FREEFALL LAND STOP',
  [ACT_SIDE_FLIP_LAND_STOP]: 'SIDE FLIP LAND STOP',
  [ACT_TRIPLE_JUMP_LAND_STOP]: 'TRIPLE JUMP LAND STOP',
  [ACT_LONG_JUMP_LAND_STOP]: 'LONG JUMP LAND STOP',
  [ACT_GROUND_POUND_LAND]: 'GROUND POUND LAND',
  [ACT_BRAKING_STOP]: 'BRAKING STOP',
  [ACT_BUTT_SLIDE_STOP]: 'BUTT SLIDE STOP',
  [ACT_WALKING]: 'WALKING',
  [ACT_TURNING_AROUND]: 'TURNING AROUND',
  [ACT_FINISH_TURNING_AROUND]: 'FINISH TURNING',
  [ACT_BRAKING]: 'BRAKING',
  [ACT_CRAWLING]: 'CRAWLING',
  [ACT_DECELERATING]: 'DECELERATING',
  [ACT_BEGIN_SLIDING]: 'BEGIN SLIDING',
  [ACT_BUTT_SLIDE]: 'BUTT SLIDE',
  [ACT_STOMACH_SLIDE]: 'STOMACH SLIDE',
  [ACT_IN_CANNON]: 'IN CANNON',
  [ACT_SHOT_FROM_CANNON]: 'SHOT FROM CANNON',
  [ACT_DIVE_SLIDE]: 'DIVE SLIDE',
  [ACT_MOVE_PUNCHING]: 'MOVE PUNCHING',
  [ACT_CROUCH_SLIDE]: 'CROUCH SLIDE',
  [ACT_SLIDE_KICK_SLIDE]: 'SLIDE KICK',
  [ACT_JUMP_LAND]: 'JUMP LAND',
  [ACT_FREEFALL_LAND]: 'FREEFALL LAND',
  [ACT_DOUBLE_JUMP_LAND]: 'DOUBLE JUMP LAND',
  [ACT_SIDE_FLIP_LAND]: 'SIDE FLIP LAND',
  [ACT_TRIPLE_JUMP_LAND]: 'TRIPLE JUMP LAND',
  [ACT_LONG_JUMP_LAND]: 'LONG JUMP LAND',
  [ACT_BACKFLIP_LAND]: 'BACKFLIP LAND',
  [ACT_JUMP]: 'JUMP',
  [ACT_DOUBLE_JUMP]: 'DOUBLE JUMP',
  [ACT_TRIPLE_JUMP]: 'TRIPLE JUMP',
  [ACT_BACKFLIP]: 'BACKFLIP',
  [ACT_STEEP_JUMP]: 'STEEP JUMP',
  [ACT_WALL_KICK_AIR]: 'WALL KICK',
  [ACT_SIDE_FLIP]: 'SIDE FLIP',
  [ACT_LONG_JUMP]: 'LONG JUMP',
  [ACT_DIVE]: 'DIVE',
  [ACT_FREEFALL]: 'FREEFALL',
  [ACT_BUTT_SLIDE_AIR]: 'BUTT SLIDE AIR',
  [ACT_AIR_HIT_WALL]: 'AIR HIT WALL',
  [ACT_GROUND_POUND]: 'GROUND POUND',
  [ACT_SLIDE_KICK]: 'SLIDE KICK AIR',
  [ACT_JUMP_KICK]: 'JUMP KICK',
  [ACT_SOFT_BONK]: 'SOFT BONK',
  [ACT_LEDGE_GRAB]: 'LEDGE GRAB',
  [ACT_LAVA_BOOST]: 'LAVA BOOST',
  [ACT_LAVA_BOOST_LAND]: 'LAVA LAND',
  [ACT_QUICKSAND_DEATH]: 'QUICKSAND',
  [ACT_HOLDING_POLE]: 'ON POLE',
  [ACT_GRAB_POLE_SLOW]: 'GRABBING POLE',
  [ACT_GRAB_POLE_FAST]: 'GRABBING POLE',
  [ACT_CLIMBING_POLE]: 'CLIMBING POLE',
  [ACT_TOP_OF_POLE_TRANSITION]: 'POLE TOP',
  [ACT_TOP_OF_POLE]: 'POLE TOP',
  [ACT_START_HANGING]: 'HANGING',
  [ACT_HANGING]: 'HANGING',
  [ACT_HANG_MOVING]: 'HANGING',
  [ACT_LEDGE_CLIMB_SLOW_1]: 'LEDGE CLIMB',
  [ACT_LEDGE_CLIMB_SLOW_2]: 'LEDGE CLIMB',
  [ACT_LEDGE_CLIMB_FAST]: 'LEDGE CLIMB',
  [ACT_START_SLEEPING]: 'GETTING SLEEPY',
  [ACT_SLEEPING]: 'SLEEPING',
  [ACT_WAKING_UP]: 'WAKING UP',
  [ACT_JUMBO_STAR_CUTSCENE]: 'GRAND STAR',
  [ACT_END_PEACH_CUTSCENE]: 'CASTLE ENDING',
  [ACT_STANDING_DEATH]: 'DYING',
  [ACT_DEATH_ON_STOMACH]: 'DYING',
  [ACT_DEATH_ON_BACK]: 'DYING',
  [ACT_PUNCHING]: 'PUNCHING',
  [ACT_PICKING_UP_BOWSER]: 'GRABBING BOWSER',
  [ACT_HOLDING_BOWSER]: 'HOLDING BOWSER',
  [ACT_RELEASING_BOWSER]: 'RELEASING BOWSER',
  [ACT_PICKING_UP]: 'PICKING UP',
  [ACT_PLACING_DOWN]: 'PLACING DOWN',
  [ACT_THROWING]: 'THROWING',
  [ACT_HOLD_IDLE]: 'HOLDING',
  [ACT_HOLD_WALKING]: 'CARRYING',
  [ACT_HOLD_DECELERATING]: 'CARRYING',
  [ACT_HOLD_JUMP]: 'CARRY JUMP',
  [ACT_HOLD_FREEFALL]: 'CARRY FALL',
  [ACT_AIR_THROW]: 'AIR THROW',
  [ACT_HOLD_JUMP_LAND]: 'CARRY LAND',
  [ACT_HOLD_FREEFALL_LAND]: 'CARRY LAND',
  [ACT_HOLD_JUMP_LAND_STOP]: 'CARRY LAND',
  [ACT_HOLD_FREEFALL_LAND_STOP]: 'CARRY LAND',
  [ACT_AIR_THROW_LAND]: 'AIR THROW LAND',
};

export function actionName(action: number): string {
  return ACTION_NAMES[action] ?? `0x${(action & ACT_ID_MASK).toString(16)}`;
}
