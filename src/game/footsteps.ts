import { ACT_DECELERATING, ACT_HOLD_DECELERATING, ACT_HOLD_WALKING, ACT_WALKING } from '../sim/state';
import { chooseSm64Clip, type Sm64AnimationState } from './sm64Animation';

// Start frame, loop end and the two play_step_sound frames from anim_and_audio_for_walk/hold_walk.
const clips: Record<string, [number, number, number, number]> = {
  anim_CA: [25,30,7,22], anim_92: [27,116,14,72], anim_48: [0,77,10,49], anim_72: [0,72,9,45],
  anim_18: [42,100,12,62], anim_16: [0,100,12,62], anim_17: [0,76,10,49],
};

export class Footsteps {
  private clip = '';
  private position = 0;
  reset(): void { this.clip = ''; }

  step(m: Sm64AnimationState): 'step' | 'tiptoe' | undefined {
    if (![ACT_WALKING, ACT_DECELERATING, ACT_HOLD_WALKING, ACT_HOLD_DECELERATING].includes(m.action)) {
      this.reset();
      return;
    }
    const previous = clips[this.clip];
    const context = previous ? {name: this.clip, frame: Math.floor(this.position % previous[1]),
      cycles: Math.floor(this.position / previous[1])} : undefined;
    const choice = chooseSm64Clip(m, context);
    const clip = clips[choice.name];
    if (!clip) { this.reset(); return; }
    if (this.clip !== choice.name) {
      this.clip = choice.name;
      this.position = clip[0];
      return;
    }
    const start = this.position;
    this.position += choice.speed;
    const crossed = (frame: number) => Math.floor((start-frame)/clip[1]) < Math.floor((this.position-frame)/clip[1]);
    if (crossed(clip[2]) || crossed(clip[3])) return this.clip === 'anim_92' ? 'tiptoe' : 'step';
  }
}
