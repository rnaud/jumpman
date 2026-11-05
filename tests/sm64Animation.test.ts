import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import * as A from '../src/sim/state';
import { chooseSm64Clip, sm64VisualRotation, type Sm64AnimationState } from '../src/game/sm64Animation';
import { Sm64AnimationPlayer, type Sm64AnimationMetadata } from '../src/game/Sm64AnimationPlayer';
import { STATIONARY_ACTIONS } from '../src/sim/actions/stationary';
import { MOVING_ACTIONS } from '../src/sim/actions/moving';
import { AIRBORNE_ACTIONS } from '../src/sim/actions/airborne';
import { AUTOMATIC_ACTIONS } from '../src/sim/actions/automatic';
import { OBJECT_ACTIONS } from '../src/sim/actions/object';
import { CollisionWorld } from '../src/sim/collision';
import { Sim, addGround } from './helpers';

const idle: Sm64AnimationState = { action: A.ACT_IDLE, actionArg: 0, actionState: 0, actionTimer: 0, forwardVel: 0, intendedMag: 0, vel: [0, 0, 0] };
const state = (action: number, overrides: Partial<Sm64AnimationState> = {}): Sm64AnimationState => ({ ...idle, action, ...overrides });
const previous = (name: string) => ({ name, frame: 0, cycles: 0 });
function flat() { const world = new CollisionWorld(); addGround(world, 8000); return new Sim(world); }

function player(clips: { name: string; frames: number; metadata?: Sm64AnimationMetadata }[]) {
  const root = new THREE.Group(); root.name = 'pose';
  const animations = clips.map(({name, frames}) => new THREE.AnimationClip(name, -1, [new THREE.NumberKeyframeTrack('pose.position[x]', Array.from({length: frames}, (_, i) => i / 30), Array.from({length: frames}, (_, i) => i), THREE.InterpolateDiscrete)]));
  return { root, player: new Sm64AnimationPlayer(root, animations, new Map(clips.map(c => [c.name, { fps: 30, ...c.metadata }]))) };
}

describe('SM64 action wiring', () => {
  it('explicitly maps every action implemented by the controller', () => {
    const actions = Object.keys({...STATIONARY_ACTIONS,...MOVING_ACTIONS,...AIRBORNE_ACTIONS,...AUTOMATIC_ACTIONS,...OBJECT_ACTIONS}).map(Number);
    expect(actions.length).toBe(85);
    for (const action of actions) {
      expect(() => chooseSm64Clip(state(action)), A.actionName(action)).not.toThrow();
      if (action !== A.ACT_IDLE) expect(chooseSm64Clip(state(action)).name).not.toBe('anim_C5');
    }
    expect(() => chooseSm64Clip(state(-1))).toThrow('No SM64 animation mapping');
  });

  it('maps the held-object action set to its source clips', () => {
    expect(chooseSm64Clip(state(A.ACT_PICKING_UP)).name).toBe('anim_6B');
    expect(chooseSm64Clip(state(A.ACT_PLACING_DOWN)).name).toBe('anim_6E');
    expect(chooseSm64Clip(state(A.ACT_THROWING)).name).toBe('anim_65');
    expect(chooseSm64Clip(state(A.ACT_AIR_THROW)).name).toBe('anim_52');
    expect(chooseSm64Clip(state(A.ACT_AIR_THROW_LAND)).name).toBe('anim_52');
    expect(chooseSm64Clip(state(A.ACT_HOLD_IDLE))).toEqual({name:'anim_3F',speed:1,loop:true});
    expect(chooseSm64Clip(state(A.ACT_HOLD_WALKING,{forwardVel:6})).name).toBe('anim_16');
    expect(chooseSm64Clip(state(A.ACT_HOLD_WALKING,{forwardVel:20})).name).toBe('anim_17');
    expect(chooseSm64Clip(state(A.ACT_HOLD_JUMP)).name).toBe('anim_41');
    expect(chooseSm64Clip(state(A.ACT_HOLD_FREEFALL)).name).toBe('anim_43');
    expect(chooseSm64Clip(state(A.ACT_HOLD_JUMP_LAND)).name).toBe('anim_40');
    expect(chooseSm64Clip(state(A.ACT_HOLD_FREEFALL_LAND)).name).toBe('anim_42');
    expect(chooseSm64Clip(state(A.ACT_HARD_BACKWARD_GROUND_KB)).name).toBe('anim_01');
    expect(chooseSm64Clip(state(A.ACT_HARD_FORWARD_GROUND_KB)).name).toBe('anim_2C');
    expect(chooseSm64Clip(state(A.ACT_HARD_BACKWARD_AIR_KB)).name).toBe('anim_01');
    expect(chooseSm64Clip(state(A.ACT_HARD_FORWARD_AIR_KB)).name).toBe('anim_2C');
    expect(chooseSm64Clip(state(A.ACT_LEDGE_CLIMB_SLOW_1)).name).toBe('anim_00');
    expect(chooseSm64Clip(state(A.ACT_START_SLEEPING,{actionState:0})).name).toBe('anim_81');
    expect(chooseSm64Clip(state(A.ACT_START_SLEEPING,{actionState:3})).name).toBe('anim_84');
    expect(chooseSm64Clip(state(A.ACT_SLEEPING,{actionState:0})).name).toBe('anim_85');
    expect(chooseSm64Clip(state(A.ACT_SLEEPING,{actionState:2})).name).toBe('anim_87');
    expect(chooseSm64Clip(state(A.ACT_WAKING_UP)).name).toBe('anim_C8');
  });

  it('uses source acceleration and hysteresis for walking/running/tiptoeing', () => {
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:32}))).toEqual({name:'anim_72',speed:8});
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:4,intendedMag:32})).speed).toBe(8);
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:20}),previous('anim_72')).name).toBe('anim_72');
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:20}),previous('anim_48')).name).toBe('anim_48');
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:17}),previous('anim_72')).name).toBe('anim_48');
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:4}),previous('anim_48'))).toEqual({name:'anim_92',speed:4});
    expect(chooseSm64Clip(state(A.ACT_WALKING,{forwardVel:6}),previous('anim_C5'))).toEqual({name:'anim_CA',speed:1.5});
    expect(chooseSm64Clip(state(A.ACT_DECELERATING,{forwardVel:32}))).toEqual({name:'anim_48',speed:8});
    expect(chooseSm64Clip(state(A.ACT_DECELERATING)).speed).toBe(1/16);
    expect(chooseSm64Clip(state(A.ACT_DECELERATING,{floorClass:()=>0x13})).name).toBe('anim_C3');
    expect(chooseSm64Clip(state(A.ACT_CRAWLING,{intendedMag:3.2})).speed).toBe(6.4);
  });

  it('reaches the side flip from controller input and reverses only the visual yaw', () => {
    const sim = flat(); sim.run({y:1},70); sim.tick({y:-1}); sim.tick({a:true,aPressed:true,y:-1});
    const m = sim.mario, yaw = m.faceAngle[1];
    expect(m.action).toBe(A.ACT_SIDE_FLIP);
    expect(chooseSm64Clip(m).name).toBe('anim_BF');
    expect(sm64VisualRotation(m)).toEqual({yaw:Math.PI,pitch:0});
    expect(m.faceAngle[1]).toBe(yaw);
    for(const action of [A.ACT_SIDE_FLIP_LAND,A.ACT_SIDE_FLIP_LAND_STOP,A.ACT_FINISH_TURNING_AROUND]) expect(sm64VisualRotation(state(action)).yaw).toBe(Math.PI);
    expect(sm64VisualRotation(state(A.ACT_IDLE)).yaw).toBe(0);
  });

  it('captures slow long jumps before velocity is multiplied and retains it for landing', () => {
    for (const [velocity, slow] of [[16,true],[17,false]] as const) {
      const sim = flat(); sim.mario.forwardVel=velocity; sim.mario.setAction(A.ACT_LONG_JUMP);
      expect(sim.mario.longJumpIsSlow).toBe(slow);
      expect(chooseSm64Clip(sim.mario).name).toBe(slow?'anim_14':'anim_13');
      sim.mario.forwardVel=0;sim.mario.setAction(A.ACT_LONG_JUMP_LAND);
      expect(chooseSm64Clip(sim.mario).name).toBe(slow?'anim_12':'anim_11');
    }
  });

  it('distinguishes bonks, wall kicks, slide falls and punch combo stages', () => {
    for (const [action, arg, name] of [
      [A.ACT_WALL_KICK_AIR,0,'anim_CB'], [A.ACT_AIR_HIT_WALL,0,'anim_CC'],
      [A.ACT_FREEFALL,1,'anim_90'], [A.ACT_FREEFALL,2,'anim_53'],
      [A.ACT_DIVE_SLIDE,0,'anim_88'], [A.ACT_STOMACH_SLIDE,0,'anim_89'],
      [A.ACT_BACKFLIP_LAND,0,'anim_C0'], [A.ACT_CROUCH_SLIDE,0,'anim_97'],
      [A.ACT_PUNCHING,0,'anim_67'], [A.ACT_PUNCHING,1,'anim_68'],
      [A.ACT_PUNCHING,2,'anim_66'], [A.ACT_MOVE_PUNCHING,9,'anim_71'],
      [A.ACT_GROUND_POUND,1,'anim_3B'],
    ] as const) expect(chooseSm64Clip(state(action,{actionArg:arg})).name).toBe(name);
    expect(sm64VisualRotation(state(A.ACT_DIVE,{faceAngle:[-0x2000,0,0]})).pitch).toBe(Math.PI/4);
    expect(sm64VisualRotation(state(A.ACT_DIVE_SLIDE,{faceAngle:[-0x2000,0,0]})).pitch).toBe(0);
  });
});

describe('SM64 native animation clock', () => {
  it('cycles left, right and center idle clips, and resets after movement', () => {
    const {player:p} = player([
      ...['anim_C3','anim_C4','anim_C5'].map(name => ({name,frames:30})),
      {name:'anim_72',frames:30},
    ]);
    p.update(idle,0);
    for (const name of ['anim_C3','anim_C4','anim_C5','anim_C3']) {
      expect(p.debug.name).toBe(name);
      p.update(idle,1);
      p.update(idle,1/60);
    }
    p.update(state(A.ACT_WALKING,{forwardVel:32}),0);
    p.update(idle,0);
    expect(p.debug.name).toBe('anim_C3');
    expect(p.debug.frame).toBe(0);
  });
  it('holds the last idle frame instead of wrapping before changing clips', () => {
    const { player: p, root } = player(['anim_C3','anim_C4','anim_C5'].map(name => ({name,frames:30})));
    p.update(idle, 0);
    p.update(idle, 29/30, true);
    expect(root.position.x).toBeCloseTo(29);
    p.update(idle, 1/30, true);
    expect(p.debug.name).toBe('anim_C3');
    expect(p.debug.finished).toBe(true);
    expect(root.position.x).toBeCloseTo(29);
    p.update(idle, 1/60, true);
    expect(p.debug.name).toBe('anim_C4');
    expect(root.position.x).toBeCloseTo(0);
  });
  it('starts landing at frame 22 and continues through LAND_STOP without restarting', () => {
    const {player:p,root} = player([{name:'anim_4E',frames:38,metadata:{startFrame:22,loop:false}}]);
    p.update(state(A.ACT_JUMP_LAND),0);
    expect(p.debug.frame).toBe(22);expect(root.position.x).toBe(22);
    p.update(state(A.ACT_JUMP_LAND,{actionTimer:3}),3/30);
    expect(p.debug.frame).toBe(25);expect(root.position.x).toBe(25);
    p.update(state(A.ACT_JUMP_LAND_STOP),1/30);
    expect(p.debug.frame).toBe(26);expect(root.position.x).toBe(26);
    p.update(state(A.ACT_JUMP_LAND_STOP,{actionTimer:7}),10);
    expect(p.debug.frame).toBe(37);
    // A new jump landing really does replay the animation.
    p.update(state(A.ACT_JUMP_LAND),0);
    expect(p.debug.frame).toBe(22);
  });

  it('samples an intermediate pose while the simulation is between ticks', () => {
    const { player: p, root } = player([{ name: 'anim_4E', frames: 38, metadata: { startFrame: 22 } }]);
    const m = state(A.ACT_JUMP_LAND);
    p.update(m, 0, true);
    p.update(m, 1 / 60, true);
    expect(root.position.x).toBeCloseTo(22.5);
  });

  it('holds the last loop frame for a complete source tick before wrapping', () => {
    const {player:p,root} = player([{name:'anim_72',frames:72,metadata:{loop:true}}]);
    const m = state(A.ACT_WALKING,{forwardVel:32});p.update(m,0);
    p.update(m,71/(30*8));expect(p.debug.frame).toBe(71);expect(root.position.x).toBe(71);
    p.update(m,.5/(30*8));expect(p.debug.frame).toBe(71);
    p.update(m,.5/(30*8));expect(p.debug.frame).toBe(0);expect(root.position.x).toBe(0);
  });

  it('advances the same source frames at 30, 60 and 144 Hz', () => {
    for(const hz of [30,60,144]) {
      const {player:p} = player([{name:'anim_72',frames:72,metadata:{loop:true}}]);
      const m = state(A.ACT_WALKING,{forwardVel:32});p.update(m,0);
      for(let i=0;i<hz;i++)p.update(m,1/hz);
      expect(p.debug.frame).toBe(24);expect(p.debug.cycles).toBe(3);
    }
  });

  it('honors loopStart, backward playback, startFrame and single-frame holds', () => {
    const {player:p} = player([{name:'anim_33',frames:10,metadata:{startFrame:4,loopStart:3,loop:true,backward:true}}]);
    const m = state(A.ACT_LEDGE_GRAB);p.update(m,0);p.update(m,2/30);expect(p.debug.frame).toBe(9);
    const {player:q,root} = player([{name:'anim_56',frames:1,metadata:{loop:false}}]);
    q.update(state(A.ACT_FREEFALL),0);q.update(state(A.ACT_FREEFALL),100);expect(root.position.x).toBe(0);expect(q.debug.frame).toBe(0);
  });

  it('starts the falling ground-pound at its held last frame', () => {
    const {player:p,root} = player([{name:'anim_3D',frames:11,metadata:{startFrame:10,loop:false}}]);
    p.update(state(A.ACT_GROUND_POUND,{actionState:1}),0);expect(root.position.x).toBe(10);
  });
});
