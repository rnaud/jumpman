/**
 * ROM-derived samples, with synthesized fallback while audio buffers load.
 */

type AudioManifest = Record<string, { loop: [number, number] | null }>;
const manifests = import.meta.glob<AudioManifest>('../../assets/audio/manifest.json', { eager: true, import: 'default' });
const manifest = manifests['../../assets/audio/manifest.json'] ?? {};
const sampleUrls = import.meta.glob<string>('../../assets/audio/**/*.wav', { eager: true, query: '?url', import: 'default' });
const clips = {
  jump: 'sfx_mario/00_mario_jump_hoo.wav', double: 'sfx_mario/01_mario_jump_wah.wav',
  triple: 'sfx_mario/04_mario_yahoo.wav', kick: 'sfx_mario/02_mario_yah.wav',
  dive: 'sfx_mario/18_mario_waha.wav', star: 'sfx_mario/0C_mario_here_we_go.wav',
  land: 'sfx_terrain/00_step_default.wav', step: 'sfx_terrain/01_step_grass.wav',
  bonk: 'sfx_7/03.wav', pound: 'sfx_1/05_heavy_landing.wav', coin: 'sfx_9/03.wav',
} as const;
type Sound = keyof typeof clips;

export class Sfx {
  muted = false;
  constructor() {
    try { this.muted = localStorage.getItem('jumpman-muted') === 'true'; } catch { /* optional storage */ }
    window.addEventListener('storage', event => {
      if (event.key === 'jumpman-muted') this.setMuted(event.newValue === 'true');
    });
  }
  setMuted(muted: boolean): void {
    this.muted = muted;
    if (this.master) this.master.gain.value = muted ? 0 : 0.24;
    try { localStorage.setItem('jumpman-muted', String(muted)); } catch { /* optional storage */ }
  }

  private readonly samples = new Map<Sound, AudioBuffer>();

  private preload(): void {
    const context = this.context!;
    for (const [key, path] of Object.entries(clips)) {
      const url = sampleUrls[`../../assets/audio/${path}`];
      if (!url) continue;
      void fetch(url).then(response => {
        if (!response.ok) throw new Error(`Audio HTTP ${response.status}: ${path}`);
        return response.arrayBuffer();
      }).then(bytes => context.decodeAudioData(bytes))
        .then(buffer => this.samples.set(key as Sound, buffer))
        .catch(error => console.warn('[jumpman] sample unavailable; using synth fallback', error));
    }
  }

  private sample(key: Sound, note = 39, gain = 0.8, delay = 0, duration?: number): boolean {
    const buffer = this.samples.get(key);
    if (!buffer || !this.context || !this.master) return false;
    const source = this.context.createBufferSource();
    source.buffer = buffer;
    source.playbackRate.value = 2 ** ((note - 39) / 12);
    const envelope = this.context.createGain();
    const start = this.context.currentTime + delay;
    const length = duration ?? buffer.duration / source.playbackRate.value;
    const loop = manifest[clips[key]]?.loop;
    if (duration && loop) { source.loop = true; [source.loopStart, source.loopEnd] = loop; }
    envelope.gain.setValueAtTime(gain, start);
    envelope.gain.setValueAtTime(gain, start + Math.max(0, length - 0.025));
    envelope.gain.linearRampToValueAtTime(0, start + length);
    source.connect(envelope).connect(this.master);
    source.onended = () => { source.disconnect(); envelope.disconnect(); };
    source.start(start);
    source.stop(start + length);
    return true;
  }
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
    this.master.gain.value = this.muted ? 0 : 0.24;
    this.master.connect(this.context.destination);
    this.preload();

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
    if (this.sample('jump', 37)) return;
    this.tone('square', 260 * power, 520 * power, 0.12, 0.16);
    this.tone('sine', 130 * power, 260 * power, 0.16, 0.18);
  }

  doubleJump(): void {
    this.ensure();
    if (this.sample('double', 36)) return;
    this.tone('square', 340, 680, 0.13, 0.16);
    this.tone('triangle', 170, 340, 0.18, 0.15, 0.02);
  }

  tripleJump(): void {
    this.ensure();
    if (this.sample('triple', 40)) return;
    this.tone('square', 420, 900, 0.18, 0.16);
    this.tone('triangle', 210, 460, 0.24, 0.16, 0.03);
    this.noise(0.12, 0.06, 2200, 0.0);
  }

  land(strength = 1): void {
    this.ensure();
    if (this.sample('land', 34, Math.min(1, 0.5 + strength * 0.2))) return;
    this.noise(0.09 + strength * 0.05, 0.22, 380 + strength * 220);
    this.tone('sine', 120, 48, 0.1 + strength * 0.04, 0.2);
  }

  bonk(): void {
    this.ensure();
    if (this.sample('bonk', 34, 0.8, 0, 0.15)) return;
    this.tone('triangle', 190, 70, 0.16, 0.3);
    this.noise(0.08, 0.16, 1200);
  }

  wallKick(): void {
    this.ensure();
    if (this.sample('kick', 36)) return;
    this.tone('square', 300, 760, 0.1, 0.18);
    this.noise(0.14, 0.12, 2400);
  }

  groundPound(): void {
    this.ensure();
    if (this.sample('pound', 39)) return;
    this.tone('sine', 90, 32, 0.3, 0.38);
    this.noise(0.32, 0.28, 300);
  }

  dive(): void {
    this.ensure();
    if (this.sample('dive', 39)) return;
    this.noise(0.18, 0.1, 1800);
    this.tone('triangle', 500, 180, 0.16, 0.1);
  }

  coin(): void {
    this.ensure();
    // Native .layer_11E4: four notes, transpose +24, at sequence tempo 120.
    if (this.samples.has('coin')) {
      let delay = 0;
      for (const [note, ticks, velocity] of [[49, 2, 40], [61, 7, 85], [54, 5, 40], [66, 55, 85]]) {
        const duration = ticks / 96;
        this.sample('coin', note, velocity / 127, delay, duration);
        delay += duration;
      }
      return;
    }
    this.tone('sine', 988, 988, 0.07, 0.22);
    this.tone('sine', 1319, 1319, 0.16, 0.2, 0.07);
  }

  star(): void {
    this.ensure();
    if (this.sample('star', 39)) return;
    const notes = [523, 659, 784, 1047, 1319];
    notes.forEach((note, index) => {
      this.tone('triangle', note, note, 0.24, 0.22, index * 0.11);
    });
    this.tone('sine', 261, 523, 0.7, 0.12);
  }

  step(grass = true): void {
    this.ensure();
    if (this.sample(grass ? 'step' : 'land', 39, 0.3)) return;
    this.noise(0.035, 0.05, 1400);
  }
}
