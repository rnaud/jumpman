/**
 * Web Audio synthesis for the MK64 experiment: a looped engine note whose pitch follows the speed, a tyre skid
 * layer and the one-shot cues of kartSound.ts. Browser twin of native/src/experiments/kart_audio.rs; both play the
 * layer tables in assets/audio/kart-voices.json (the MK64 samples themselves are not in the repository).
 */
import voiceTable from '../../../assets/audio/kart-voices.json';
import type { Sfx } from '../Sfx';
import type { Engine, KartCue } from './kartSound';

export interface VoiceLayer {
  wave: 'sine' | 'square' | 'saw' | 'tri' | 'noise';
  f0: number; f1: number; dur: number; gain: number;
  t0?: number; attack?: number; hold?: boolean; vib?: [number, number];
}
export interface Voice { gain: number; layers: VoiceLayer[] }
export const VOICES = Object.fromEntries(Object.entries(voiceTable).filter(([k]) => !k.startsWith('_'))) as unknown as Record<KartCue, Voice>;

const OSC_TYPE = { sine: 'sine', square: 'square', saw: 'sawtooth', tri: 'triangle' } as const;

export class KartAudio {
  private ctx: AudioContext | null = null;
  private bus: GainNode | null = null;
  private noise: AudioBuffer | null = null;
  private engine: { oscs: OscillatorNode[]; filter: BiquadFilterNode; gain: GainNode; mix: GainNode[] } | null = null;
  private skid: { src: AudioBufferSourceNode; filter: BiquadFilterNode; gain: GainNode } | null = null;
  private last = '';

  constructor(private readonly sfx: Sfx) {}

  /** Create the graph once the browser lets audio start. Safe to call every frame until it succeeds. */
  private ensure(): boolean {
    if (this.ctx) return true;
    const bus = this.sfx.synthBus();
    if (!bus) return false;
    const ctx = bus.context;
    this.ctx = ctx;
    const compressor = ctx.createDynamicsCompressor();
    this.bus = ctx.createGain();
    this.bus.gain.value = 1.6;
    this.bus.connect(compressor);
    compressor.connect(bus.out);
    const length = ctx.sampleRate * 2;
    this.noise = ctx.createBuffer(1, length, ctx.sampleRate);
    const data = this.noise.getChannelData(0);
    let seed = 0x1234567;
    for (let i = 0; i < length; i++) { seed = (seed * 1664525 + 1013904223) >>> 0; data[i] = seed / 2147483648 - 1; }
    // Engine: two detuned saws and a square an octave down through a low-pass that opens with the revs.
    const filter = ctx.createBiquadFilter();
    filter.type = 'lowpass';
    filter.frequency.value = 500;
    filter.Q.value = 2;
    const gain = ctx.createGain();
    gain.gain.value = 0;
    filter.connect(gain);
    gain.connect(this.bus);
    const oscs: OscillatorNode[] = [];
    const mix: GainNode[] = [];
    for (const [type, ratio, level] of [['sawtooth', 1, 0.5], ['sawtooth', 1.006, 0.4], ['square', 0.5, 0.45]] as const) {
      const osc = ctx.createOscillator();
      osc.type = type;
      osc.frequency.value = 50 * ratio;
      osc.detune.value = 0;
      const g = ctx.createGain();
      g.gain.value = level;
      osc.connect(g);
      g.connect(filter);
      osc.start();
      oscs.push(osc);
      mix.push(g);
      (osc as OscillatorNode & { ratio?: number }).ratio = ratio;
    }
    this.engine = { oscs, filter, gain, mix };
    const src = ctx.createBufferSource();
    src.buffer = this.noise;
    src.loop = true;
    const skidFilter = ctx.createBiquadFilter();
    skidFilter.type = 'bandpass';
    skidFilter.frequency.value = 2600;
    skidFilter.Q.value = 1.4;
    const skidGain = ctx.createGain();
    skidGain.gain.value = 0;
    src.connect(skidFilter);
    skidFilter.connect(skidGain);
    skidGain.connect(this.bus);
    src.start();
    this.skid = { src, filter: skidFilter, gain: skidGain };
    return true;
  }

  /** Continuous engine and skid parameters, once per rendered frame. */
  setEngine(e: Engine): void {
    if (!this.ensure() || !this.ctx || !this.engine || !this.skid) return;
    const now = this.ctx.currentTime;
    for (const osc of this.engine.oscs) {
      const ratio = (osc as OscillatorNode & { ratio?: number }).ratio ?? 1;
      osc.frequency.setTargetAtTime(e.freq * ratio, now, 0.05);
    }
    this.engine.filter.frequency.setTargetAtTime(380 + e.freq * 9, now, 0.06);
    this.engine.gain.gain.setTargetAtTime(e.gain * 0.5, now, 0.06);
    this.skid.gain.gain.setTargetAtTime(e.skid * 0.35, now, 0.05);
    this.skid.filter.frequency.setTargetAtTime(1900 + e.freq * 6, now, 0.1);
    this.last = `${e.freq.toFixed(1)}|${e.gain.toFixed(2)}|${e.skid.toFixed(2)}`;
  }

  /** Last engine parameters, for tests and the debug hook. */
  state(): string { return this.last; }

  play(cue: KartCue): void {
    if (!this.ensure() || !this.ctx || !this.bus || !this.noise) return;
    const voice = VOICES[cue];
    if (!voice) return;
    const ctx = this.ctx;
    const base = ctx.currentTime + 0.005;
    for (const layer of voice.layers) {
      const t0 = base + (layer.t0 ?? 0);
      const end = t0 + layer.dur;
      const peak = Math.max(0.0001, layer.gain * voice.gain);
      const env = ctx.createGain();
      env.gain.setValueAtTime(0.0001, t0);
      env.gain.linearRampToValueAtTime(peak, t0 + (layer.attack ?? 0.003));
      if (layer.hold) env.gain.setValueAtTime(peak, t0 + layer.dur * 0.7);
      env.gain.exponentialRampToValueAtTime(0.0001, end);
      env.connect(this.bus);
      let source: AudioScheduledSourceNode;
      if (layer.wave === 'noise') {
        const src = ctx.createBufferSource();
        src.buffer = this.noise;
        src.loop = true;
        const lp = ctx.createBiquadFilter();
        lp.type = 'lowpass';
        lp.frequency.setValueAtTime(layer.f0, t0);
        lp.frequency.exponentialRampToValueAtTime(Math.max(20, layer.f1), end);
        src.connect(lp);
        lp.connect(env);
        src.start(t0, Math.random() * 1.5);
        source = src;
      } else {
        const osc = ctx.createOscillator();
        osc.type = OSC_TYPE[layer.wave];
        osc.frequency.setValueAtTime(layer.f0, t0);
        if (layer.f1 !== layer.f0) osc.frequency.exponentialRampToValueAtTime(layer.f1, end);
        osc.connect(env);
        if (layer.vib) {
          const lfo = ctx.createOscillator();
          const depth = ctx.createGain();
          lfo.frequency.value = layer.vib[0];
          depth.gain.value = Math.max(layer.f0, layer.f1) * layer.vib[1];
          lfo.connect(depth);
          depth.connect(osc.frequency);
          lfo.start(t0);
          lfo.stop(end + 0.05);
        }
        osc.start(t0);
        source = osc;
      }
      source.stop(end + 0.05);
      source.onended = () => env.disconnect();
    }
  }

  dispose(): void {
    for (const osc of this.engine?.oscs ?? []) { try { osc.stop(); } catch { /* already stopped */ } osc.disconnect(); }
    try { this.skid?.src.stop(); } catch { /* already stopped */ }
    this.engine?.gain.disconnect();
    this.skid?.gain.disconnect();
    this.bus?.disconnect();
    this.engine = null;
    this.skid = null;
    this.bus = null;
    this.ctx = null;
  }
}
