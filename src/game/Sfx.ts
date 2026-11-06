/**
 * ROM-derived samples, with synthesized fallback while audio buffers load.
 */

type AudioManifest = Record<string, { loop: [number, number] | null }>;
const manifests = import.meta.glob<AudioManifest>('../../assets/audio/manifest.json', { eager: true, import: 'default' });
const manifest = manifests['../../assets/audio/manifest.json'] ?? {};
const sampleUrls = import.meta.glob<string>('../../assets/audio/**/*.wav', { eager: true, query: '?url', import: 'default' });
const musicFiles = import.meta.glob<{ events: Array<{ note: number; duration: number; velocity: number }> }>('../../assets/audio/music/*.json', { eager: true, import: 'default' });
const clips = {
  jump: 'sfx_mario/00_mario_jump_hoo.wav', double: 'sfx_mario/01_mario_jump_wah.wav',
  triple: 'sfx_mario/04_mario_yahoo.wav', kick: 'sfx_mario/02_mario_yah.wav',
  dive: 'sfx_mario/18_mario_waha.wav', star: 'sfx_mario/0C_mario_here_we_go.wav',
  land: 'sfx_terrain/00_step_default.wav', step: 'sfx_terrain/01_step_grass.wav',
  bonk: 'sfx_7/03.wav', pound: 'sfx_1/05_heavy_landing.wav', coin: 'sfx_9/03.wav',
  splash: 'sfx_water/01_splash.wav', swim: 'sfx_water/02_swim.wav', plunge: 'sfx_water/00_plunge.wav',
  stoneStep: 'sfx_terrain/02_step_stone.wav', sandStep: 'sfx_terrain/07_step_sand.wav',
  snowStep: 'sfx_terrain/04_step_snow.wav', iceStep: 'sfx_terrain/05_step_ice.wav',
  metalStep: 'sfx_terrain/06_step_metal.wav',
  start: 'course_start/00_la_E3.wav',
} as const;
type Sound = keyof typeof clips;
export type MusicTrack = 'grass' | 'inside' | 'slide';

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
    if (muted) this.stopMusic();
    else if (this.currentMusic) this.startMusic(this.currentMusic);
    try { localStorage.setItem('jumpman-muted', String(muted)); } catch { /* optional storage */ }
  }

  currentMusic: MusicTrack | null = null;
  private musicTimer: ReturnType<typeof setTimeout> | null = null;
  private musicGeneration = 0;

  startMusic(track: MusicTrack): void {
    if (this.currentMusic === track) return;
    this.stopMusic();
    this.currentMusic = track;
    if (this.muted) return;
    this.ensure();
    if (!this.context) return;
    const data = musicFiles[`../../assets/audio/music/${track}.json`];
    if (!data) return;
    const generation = ++this.musicGeneration;
    const play = () => {
      if (generation !== this.musicGeneration || !this.context || !this.master) return;
      let delay = 0;
      for (const event of data.events) {
        const seconds = Math.max(0.04, event.duration / 48);
        const frequency = 440 * 2 ** ((event.note - 69) / 12);
        this.tone('triangle', frequency, frequency, Math.max(0.08, seconds * 0.9), 0.028 * (event.velocity / 127), delay);
        delay += seconds;
      }
      this.musicTimer = setTimeout(play, Math.max(0.1, delay) * 1000);
    };
    play();
  }

  stopMusic(): void {
    this.musicGeneration++;
    if (this.musicTimer !== null) clearTimeout(this.musicTimer);
    this.musicTimer = null;
  }

  musicTrack(): MusicTrack | null {
    return this.currentMusic;
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

  splash(): void {
    this.ensure();
    if (this.sample('splash', 39, 0.8)) return;
    this.noise(0.18, 0.18, 1200);
    this.tone('sine', 360, 90, 0.2, 0.12);
  }

  swim(): void {
    this.ensure();
    if (this.sample('swim', 39, 0.45)) return;
    this.noise(0.12, 0.06, 900);
  }

  footstep(surfaceType = 0): void {
    this.ensure();
    const key: Sound = [0x2e, 0x13, 0x14, 0x35, 0x36].includes(surfaceType)
      ? 'iceStep'
      : surfaceType === 0x5
        ? 'metalStep'
        : [0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d].includes(surfaceType)
          ? 'sandStep'
          : surfaceType === 0x30 || surfaceType === 0x37
            ? 'stoneStep'
            : 'step';
    if (this.sample(key, 39, 0.5)) return;
    this.noise(0.06, 0.08, 900);
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

  longJump(): void {
    this.ensure();
    this.noise(0.07, 0.09, 1200);
    if (this.sample('triple', 37, 0.85)) return;
    this.tone('triangle', 420, 900, 0.22, 0.24);
    this.tone('sine', 210, 460, 0.28, 0.14, 0.02);
  }

  groundPoundStart(): void {
    this.ensure();
    this.noise(0.18, 0.12, 1800);
    this.tone('triangle', 520, 140, 0.18, 0.16);
  }

  explosion(): void {
    this.ensure();
    this.tone('sine', 110, 28, 0.45, 0.42);
    this.tone('square', 74, 22, 0.38, 0.22, 0.01);
    this.noise(0.5, 0.4, 420);
    this.noise(0.22, 0.2, 1400, 0.02);
  }

  breakBox(): void {
    this.ensure();
    this.noise(0.18, 0.24, 2600);
    this.tone('triangle', 240, 70, 0.16, 0.2);
  }

  courseStart(): void {
    this.ensure();
    const notes: Array<[number, number, number]> = [[52, 0, 0.12], [56, 0.12, 0.12], [59, 0.24, 0.12], [64, 0.36, 0.42]];
    if (this.samples.has('start')) {
      for (const [note, delay, duration] of notes) this.sample('start', note, 0.42, delay, duration);
      return;
    }
    for (const [note, delay, duration] of notes) {
      const frequency = 440 * 2 ** ((note - 69) / 12);
      this.tone('triangle', frequency, frequency, duration, 0.14, delay);
    }
  }

  raceStart(): void {
    this.ensure();
    [660, 660, 660, 880].forEach((frequency, index) => {
      this.tone('square', frequency, frequency, index === 3 ? 0.22 : 0.08, 0.13, index * 0.16);
    });
  }

  bossIntro(): void {
    this.ensure();
    this.tone('sawtooth', 72, 42, 0.6, 0.2);
    this.noise(0.45, 0.16, 320);
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
