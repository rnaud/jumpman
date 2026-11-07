// Audio differential for the Rust port: runs the REAL `Sfx.ts` against a fake music node that records every
// worklet message, replays the messages through the same wasm engine the worklet uses (music-worklet.js logic, at the
// 32 kHz engine rate, commands applied at render-block boundaries) and writes
//   native/sim/tests/fixtures/audio.json   (per scene: engine calls, PCM FNV hash, RMS)
//   native/target/audio_ts/<scene>.s16     (the rendered PCM, s16le stereo 32 kHz)
// Compared by native/src/audio/audio_tests.rs, which builds the same scenes (tests/audioScenes.json) with the Rust
// `Sfx` + `Processor`. `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.audio.gen.test.ts`
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it, vi } from 'vitest';

type Message = Record<string, any>;
const sent: Message[] = [];

vi.mock('../src/game/music', async () => {
  const actual = await vi.importActual<typeof import('../src/game/music')>('../src/game/music');
  return {
    ...actual,
    createMusicNode: async () => ({ port: { postMessage: (m: Message) => sent.push(JSON.parse(JSON.stringify(m))) }, connect() {} }),
  };
});

const store: Record<string, string> = {};
(globalThis as any).window = {
  addEventListener() {},
  AudioContext: class {
    state = 'running';
    destination = {};
    createGain() { return { gain: { value: 0 }, connect() {} }; }
    resume() { return Promise.resolve(); }
  },
};
(globalThis as any).localStorage = { getItem: (k: string) => store[k] ?? null, setItem: (k: string, v: string) => { store[k] = v; } };

const scenes = JSON.parse(readFileSync('tests/audioScenes.json', 'utf8')) as { seconds: number; scenes: { name: string; ops: Message[] }[] };
const RATE = 32000;

/** The worklet's onmessage handler as a list of engine calls. */
function calls(data: Message): [string, number[]][] {
  const out: [string, number[]][] = [];
  if (data.location) out.push(['music_location', [...data.location]]);
  if (data.sound) out.push(['sound_effect', [...data.sound]]);
  if (data.birds) data.birds.forEach((pos: number[], i: number) => out.push(['music_bird', [i, ...pos]]));
  if (data.track) out.push(['music_start', [data.track.sequence, data.track.preset]]);
  if (data.power) out.push(['music_power', [...data.power]]);
  switch (data.event) {
    case 'exitKey': out.push(['music_exit_reward', [1]]); break;
    case 'exitStar': out.push(['music_exit_reward', [0]]); break;
    case 'clear': out.push(['play_course_clear', []]); break;
    case 'spawn': out.push(['play_power_star_jingle', [1]]); break;
    case 'puzzle': out.push(['play_puzzle_jingle', []]); break;
    case 'kingIntro': out.push(['music_king_dialog', [0]]); break;
    case 'kingFight': out.push(['music_king_dialog', [1]]); break;
    case 'kingDefeat': out.push(['music_king_dialog', [2]]); break;
    case 'kingLeave': out.push(['music_king_dialog', [3]]); break;
    case 'raceStart': out.push(['music_race', [1]]); break;
    case 'raceEnd': out.push(['music_race', [0]]); break;
  }
  if (data.stop) out.push(['seq_player_fade_out', [0, 126]]);
  return out;
}

function fnv(bytes: Uint8Array): number {
  let h = 0x811c9dc5;
  for (const b of bytes) h = Math.imul(h ^ b, 0x01000193) >>> 0;
  return h;
}

async function runScene(Sfx: any, courseMusic: any, scene: { name: string; ops: Message[] }, module: WebAssembly.Module) {
  sent.length = 0;
  const sfx = new Sfx();
  const timeline: { t: number; messages: Message[] }[] = [];
  let player: Message = { cam: [0, 0, 0], floorType: 1, underwater: false };
  for (const op of scene.ops) {
    const before = sent.length;
    switch (op.op) {
      case 'player': {
        player = op;
        break;
      }
      case 'music': sfx.startMusic(courseMusic(op.course, op.area)); await new Promise(r => setTimeout(r, 5)); break;
      case 'location': sfx.setMusicLocation(op.area, op.room, op.pos, op.stars); break;
      case 'power': sfx.setPowerMusic(op.seq, op.fade); break;
      case 'stop': sfx.stopMusic(); break;
      case 'event': sfx.musicEvent(op.name); break;
      default: {
        const mario = {
          pos: [0, 0, 0], metalCapTimer: player.metal ? 1 : 0,
          floor: player.floorType === null ? undefined : { type: player.floorType },
          floorHeight: player.underwater ? -100 : 100, world: { waterLevel: () => 0 },
        };
        sfx.setPlayer(mario, player.cam, false);
        const f = (sfx as any)[op.op];
        if (typeof f !== 'function') throw new Error('Sfx has no ' + op.op);
        switch (op.op) {
          case 'coin': sfx.coin(op.underwater); break;
          case 'swim': sfx.swim(op.fast); break;
          case 'footstep': sfx.footstep(op.tiptoe); break;
          case 'secret': case 'redCoin': (sfx as any)[op.op](op.count); break;
          case 'explosion': sfx.explosion(op.pos, op.mine); break;
          default: f.call(sfx);
        }
      }
    }
    for (const m of sent.slice(before)) timeline.push({ t: op.t, messages: [m] });
  }
  // Render like the worklet: messages are applied between render blocks.
  const engine = new WebAssembly.Instance(module, {}).exports as any;
  engine._initialize();
  engine.music_init();
  const total = scenes.seconds * RATE;
  const pcmOut = new Int16Array(total * 2 + 544 * 2);
  let rendered = 0, playing = false, pending = [...timeline];
  const applied: [number, string, number[]][] = [];
  const guard = 100000;
  for (let i = 0; i < guard && rendered < total; i++) {
    while (pending.length && pending[0].t * RATE <= rendered) {
      for (const m of pending.shift()!.messages) {
        for (const [name, args] of calls(m)) {
          engine[name](...args);
          applied.push([rendered, name, args]);
        }
        if (m.track) playing = true;
      }
    }
    if (!playing) { rendered += 544; continue; }
    const frames = engine.music_render();
    const view = new Int16Array(engine.memory.buffer, engine.music_samples(), 544 * 2);
    pcmOut.set(view.subarray(0, frames * 2), rendered * 2);
    rendered += frames;
  }
  const bytes = new Uint8Array(pcmOut.buffer, 0, total * 4);
  let sum = 0;
  for (let i = 0; i < total * 2; i++) sum += pcmOut[i] * pcmOut[i];
  mkdirSync('native/target/audio_ts', { recursive: true });
  writeFileSync(`native/target/audio_ts/${scene.name}.s16`, bytes);
  return { name: scene.name, calls: applied, frames: rendered, hash: fnv(bytes), rms: Math.sqrt(sum / (total * 2)) / 32768 };
}

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust audio fixtures', () => {
  it('writes audio goldens', async () => {
    const { Sfx } = await import('../src/game/Sfx');
    const { courseMusic } = await vi.importActual<typeof import('../src/game/music')>('../src/game/music');
    const module = await WebAssembly.compile(readFileSync('assets/audio/music/sm64-us.wasm'));
    const out = [];
    for (const scene of scenes.scenes) {
      const r = await runScene(Sfx, courseMusic, scene, module);
      console.log(r.name, 'calls', r.calls.length, 'rms', r.rms.toFixed(4), 'hash', r.hash.toString(16));
      out.push(r);
    }
    mkdirSync('native/sim/tests/fixtures', { recursive: true });
    writeFileSync('native/sim/tests/fixtures/audio.json', JSON.stringify({ seconds: scenes.seconds, scenes: out }));
  }, 600000);
});
