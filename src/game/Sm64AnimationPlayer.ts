import * as THREE from 'three';
import { ACT_IDLE } from '../sim/state';
import { chooseSm64Clip, continuesSm64Clip, type Sm64AnimationState, type Sm64PlaybackContext } from './sm64Animation';

export interface Sm64AnimationMetadata {
  fps?: number;
  startFrame?: number;
  loopStart?: number;
  loop?: boolean;
  backward?: boolean;
}
interface Entry { clip: THREE.AnimationClip; action?: THREE.AnimationAction; metadata: Sm64AnimationMetadata; frames: number }

/** Interpolating copies are immutable, so every player (e.g. a crowd of Marios) shares them. */
const sampledClips = new WeakMap<THREE.AnimationClip, THREE.AnimationClip>();
function sampledClip(clip: THREE.AnimationClip): THREE.AnimationClip {
  let pose = sampledClips.get(clip);
  if (!pose) {
    // Exported transforms use STEP keys. Sample a private linear/slerp copy
    // between source frames, or exactly on the keys for stepped rendering.
    pose = clip.clone();
    for (const track of pose.tracks) {
      if (track.getInterpolation() === THREE.InterpolateDiscrete) track.setInterpolation(THREE.InterpolateLinear);
    }
    sampledClips.set(clip, pose);
  }
  return pose;
}

/** Samples the source frame clock explicitly. glTF ends at (N-1)/fps, while an
 * SM64 loop lasts N/fps. Using AnimationAction's default loop loses the last frame.
 * Explicit sampling also preserves nonzero start frames and loop ranges. */
export class Sm64AnimationPlayer {
  private readonly mixer: THREE.AnimationMixer;
  private readonly entries = new Map<string, Entry>();
  private current?: Entry;
  private lastAction = -1;
  private lastArg = -1;
  private lastTimer = -1;
  private position = 0;
  private speed = 1;
  private frame = 0;
  private cycles = 0;
  private loop = false;

  constructor(root: THREE.Object3D, clips: readonly THREE.AnimationClip[], metadata: ReadonlyMap<string, Sm64AnimationMetadata>) {
    this.mixer = new THREE.AnimationMixer(root);
    for (const clip of clips) {
      const info = metadata.get(clip.name) ?? {};
      const fps = info.fps ?? 30;
      // Actions bind lazily: most of the 209 clips are never used by a given Mario.
      this.entries.set(clip.name, { clip: sampledClip(clip), metadata: info, frames: Math.round(clip.duration * fps) + 1 });
    }
  }

  get debug(): Sm64PlaybackContext & { speed: number; loop: boolean } {
    return { name: this.current?.clip.name ?? '', frame: this.frame, cycles: this.cycles, speed: this.speed, loop: this.loop, finished: Boolean(this.current && this.position >= this.current.frames) };
  }

  update(m: Sm64AnimationState, dt: number, smooth = false): void {
    const choice = chooseSm64Clip(m, m.action === ACT_IDLE && this.lastAction !== ACT_IDLE ? undefined : this.debug);
    const entry = this.entries.get(choice.name);
    if (!entry) throw new Error(`Exported Mario is missing ${choice.name}`);
    const action = entry.action ??= this.mixer.clipAction(entry.clip);
    const { metadata: info, frames } = entry;
    const fps = info.fps ?? 30;
    this.loop = choice.loop ?? info.loop ?? false;
    const restart = entry !== this.current || !continuesSm64Clip(this.lastAction, m.action)
      || m.actionArg !== this.lastArg
      || (!this.loop && m.action === this.lastAction && (m.actionTimer ?? 0) < this.lastTimer);
    if (restart) {
      const idleTransition = m.action === ACT_IDLE && this.lastAction === ACT_IDLE;
      if (this.current && entry !== this.current) {
        if (idleTransition) this.current.action?.stop();
        else this.current.action?.fadeOut(0.06);
      }
      action.reset().setEffectiveWeight(1).play();
      // Blend between different poses; resetting the same clip must not fade it to zero.
      if (this.current && entry !== this.current && !idleTransition) action.fadeIn(0.06);
      action.paused = true;
      // A paused, one-shot action lets us assign its time without Three wrapping it.
      action.setLoop(THREE.LoopOnce, 1);
      action.clampWhenFinished = true;
      this.current = entry;
      this.position = Math.max(0, Math.min(frames - 1, info.startFrame ?? 0));
    } else {
      this.position += Math.max(0, dt) * fps * choice.speed * (info.backward ? -1 : 1);
    }
    const low = Math.max(0, Math.min(frames - 1, info.loopStart ?? 0));
    const span = frames - low;
    this.cycles = 0;
    let sample = this.position;
    if (this.loop && (sample >= frames || (info.backward && sample < low))) {
      this.cycles = Math.abs(Math.floor((sample - low) / span));
      sample = low + ((sample - low) % span + span) % span;
    }
    this.frame = Math.min(frames - 1, Math.max(0, Math.floor(sample + 1e-6)));
    const times = action.getClip().tracks[0]?.times;
    const start = times?.[this.frame] ?? this.frame / fps;
    const end = times?.[this.frame + 1] ?? start;
    action.time = smooth ? start + (end - start) * Math.max(0, sample - this.frame) : start;
    this.speed = choice.speed;
    this.lastAction = m.action;
    this.lastArg = m.actionArg;
    this.lastTimer = m.actionTimer ?? 0;
    this.mixer.update(Math.max(0, dt));
  }
}
