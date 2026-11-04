/**
 * Tiny synthesized sound effects.  No sampled Nintendo audio is used.
 */

export class Sfx {
  private context: AudioContext | null = null;
  private master: GainNode | null = null;
  private noiseBuffer: AudioBuffer | null = null;

  ensure(): void {
    if (this.context) {
      if (this.context.state === 'suspended') void this.context.resume();
      return;
    }

    const Ctor = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    if (!Ctor) return;
    this.context = new Ctor();
    this.master = this.context.createGain();
    this.master.gain.value = 0.24;
    this.master.connect(this.context.destination);

    const length = this.context.sampleRate * 0.6;
    const buffer = this.context.createBuffer(1, length, this.context.sampleRate);
    const data = buffer.getChannelData(0);
    for (let i = 0; i < length; i++) data[i] = Math.random() * 2 - 1;
    this.noiseBuffer = buffer;
  }

  private now(): number {
    return this.context ? this.context.currentTime : 0;
  }

  private tone(
    type: OscillatorType,
    startFreq: number,
    endFreq: number,
    duration: number,
    gain = 0.5,
    delay = 0,
  ): void {
    if (!this.context || !this.master) return;
    const t = this.now() + delay;
    const oscillator = this.context.createOscillator();
    const envelope = this.context.createGain();
    oscillator.type = type;
    oscillator.frequency.setValueAtTime(startFreq, t);
    oscillator.frequency.exponentialRampToValueAtTime(Math.max(20, endFreq), t + duration);
    envelope.gain.setValueAtTime(0.0001, t);
    envelope.gain.exponentialRampToValueAtTime(gain, t + 0.008);
    envelope.gain.exponentialRampToValueAtTime(0.0001, t + duration);
    oscillator.connect(envelope).connect(this.master);
    oscillator.start(t);
    oscillator.stop(t + duration + 0.02);
  }

  private noise(duration: number, gain: number, filterFreq: number, delay = 0): void {
    if (!this.context || !this.master || !this.noiseBuffer) return;
    const t = this.now() + delay;
    const source = this.context.createBufferSource();
    source.buffer = this.noiseBuffer;
    const filter = this.context.createBiquadFilter();
    filter.type = 'lowpass';
    filter.frequency.value = filterFreq;
    const envelope = this.context.createGain();
    envelope.gain.setValueAtTime(gain, t);
    envelope.gain.exponentialRampToValueAtTime(0.0001, t + duration);
    source.connect(filter).connect(envelope).connect(this.master);
    source.start(t);
    source.stop(t + duration + 0.02);
  }

  jump(power = 1): void {
    this.ensure();
    this.tone('square', 260 * power, 520 * power, 0.12, 0.16);
    this.tone('sine', 130 * power, 260 * power, 0.16, 0.18);
  }

  doubleJump(): void {
    this.ensure();
    this.tone('square', 340, 680, 0.13, 0.16);
    this.tone('triangle', 170, 340, 0.18, 0.15, 0.02);
  }

  tripleJump(): void {
    this.ensure();
    this.tone('square', 420, 900, 0.18, 0.16);
    this.tone('triangle', 210, 460, 0.24, 0.16, 0.03);
    this.noise(0.12, 0.06, 2200, 0.0);
  }

  land(strength = 1): void {
    this.ensure();
    this.noise(0.09 + strength * 0.05, 0.22, 380 + strength * 220);
    this.tone('sine', 120, 48, 0.1 + strength * 0.04, 0.2);
  }

  bonk(): void {
    this.ensure();
    this.tone('triangle', 190, 70, 0.16, 0.3);
    this.noise(0.08, 0.16, 1200);
  }

  wallKick(): void {
    this.ensure();
    this.tone('square', 300, 760, 0.1, 0.18);
    this.noise(0.14, 0.12, 2400);
  }

  groundPound(): void {
    this.ensure();
    this.tone('sine', 90, 32, 0.3, 0.38);
    this.noise(0.32, 0.28, 300);
  }

  dive(): void {
    this.ensure();
    this.noise(0.18, 0.1, 1800);
    this.tone('triangle', 500, 180, 0.16, 0.1);
  }

  coin(): void {
    this.ensure();
    this.tone('sine', 988, 988, 0.07, 0.22);
    this.tone('sine', 1319, 1319, 0.16, 0.2, 0.07);
  }

  star(): void {
    this.ensure();
    const notes = [523, 659, 784, 1047, 1319];
    notes.forEach((note, index) => {
      this.tone('triangle', note, note, 0.24, 0.22, index * 0.11);
    });
    this.tone('sine', 261, 523, 0.7, 0.12);
  }

  step(): void {
    this.ensure();
    this.noise(0.035, 0.05, 1400);
  }
}
