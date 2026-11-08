// Sound-cue trigger parity for the Rust port: every Mario event, object event and progression cue that Game.ts turns
// into a Sfx call is replayed through the REAL Sfx.ts (against a fake music node that records the worklet messages) and
// the resulting engine-call stream is written to native/sim/tests/fixtures/cues.json. native/src/audio/cue_tests.rs feeds
// the same events through the Rust dispatch (`audio::dispatch_sim_event` / `apply_cue`, the functions the Bevy systems call)
// and a Rust `Sfx`, and compares the engine calls case by case. The dispatch below is a literal copy of the sfx lines of
// `Game.processEvents`, `Game.processObjectEvents` and the progression glue (camera = identity, so camera-space = world).
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.cues.gen.test.ts`
import { writeFileSync, mkdirSync } from 'node:fs';
import { describe, it, vi } from 'vitest';
import {
  ACT_DOUBLE_JUMP, ACT_TRIPLE_JUMP, ACT_BACKFLIP, ACT_JUMP_KICK, ACT_JUMP, ACT_LONG_JUMP,
} from '../src/sim/state';

type Message = Record<string, any>;
const sent: Message[] = [];

vi.mock('../src/game/music', async () => {
  const actual = await vi.importActual<typeof import('../src/game/music')>('../src/game/music');
  return {
    ...actual,
    createMusicNode: async () => ({ port: { postMessage: (m: Message) => sent.push(JSON.parse(JSON.stringify(m))) }, connect() {} }),
  };
});

(globalThis as any).window = {
  addEventListener() {},
  AudioContext: class {
    state = 'running';
    destination = {};
    createGain() { return { gain: { value: 0 }, connect() {} }; }
    resume() { return Promise.resolve(); }
  },
};
(globalThis as any).localStorage = { getItem: () => null, setItem() {} };

/** The worklet's onmessage handler as a list of engine calls (same as rustFixtures.audio.gen.test.ts). */
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

// ---- the copied dispatch --------------------------------------------------------------------------------------------
const P = [123, 45, -678];

function marioEvents(sfx: any, events: any[]): void {
  for (const event of events) {
    switch (event.type) {
      case 'jump':
        if (event.action === ACT_DOUBLE_JUMP) sfx.doubleJump();
        else if (event.action === ACT_TRIPLE_JUMP) sfx.tripleJump();
        else if (event.action === ACT_BACKFLIP) sfx.backflip();
        else if (event.action === ACT_JUMP_KICK) sfx.kick();
        else sfx.jump();
        break;
      case 'triplejump': sfx.tripleJump(); break;
      case 'land': if (!events.some(e => e.type === 'groundpoundland')) sfx.land(); break;
      case 'wingFlight': sfx.wingFlight(event.pullUp); break;
      case 'flightBoundary': sfx.flightBoundary(); break;
      case 'flightBurn': sfx.flightBurn(); break;
      case 'bonk': sfx.bonk(); break;
      case 'wallkick': sfx.wallKick(); break;
      case 'groundpound': sfx.groundPoundStart(); break;
      case 'cannonMove': sfx.cannonMove(event.phase); break;
      case 'cannonFire': sfx.cannonFire(); break;
      case 'bowserGrab': sfx.bowserGrab(); break;
      case 'bowserSpin': sfx.bowserSpin(); break;
      case 'bowserThrow': sfx.bowserThrow(event.fast); break;
      case 'waterPlunge': sfx.splash(); break;
      case 'swimStroke': sfx.swim(event.fast); break;
      case 'flutterKick': sfx.flutter(); break;
      case 'groundpounddrop': sfx.groundPoundDrop(); break;
      case 'groundpoundland': sfx.groundPound(); break;
      case 'longjump': sfx.longJump(); break;
      case 'dive': sfx.dive(); break;
      case 'hurt': sfx.hurt(); break;
      case 'death': sfx.death(); break;
      case 'deathLanding': sfx.deathLanding(); break;
      case 'mamaMia': sfx.mamaMia(); break;
    }
  }
}

function objectEvent(sfx: any, event: any): void {
  switch (event.type) {
    case 'waterBomb': if (event.phase !== 'burst') sfx.waterBomb(event.pos, event.phase); break;
    case 'waterCannon': sfx.waterCannon(event.pos); break;
    case 'explosion': sfx.explosion(event.pos, event.mine); break;
    case 'break':
      if (event.wall) {
        sfx.blastWall(event.pos);
        if (event.wall === 'right') sfx.musicEvent('puzzle');
      } else sfx.breakBox(event.pos);
      break;
    case 'hurt': sfx.hurt(); break;
    case 'chest': sfx.chestAnswer(Boolean(event.number)); break;
    case 'racerSound': sfx.racerSound(event.pos, event.cue); break;
    case 'penguinSound': sfx.penguin(event.pos, event.diving); break;
    case 'snowman':
      if (event.cue !== 'reward') {
        sfx.snowman(event.pos, event.cue);
        if (event.cue === 'assembled') sfx.musicEvent('puzzle');
      }
      break;
    case 'chestReward': sfx.musicEvent('puzzle'); break;
    case 'bowser': case 'bigBully': case 'whompKing': case 'bigBoo': sfx.musicEvent('spawn'); break;
    case 'oneUp': sfx.oneUp(); break;
    case 'kingLeave': sfx.musicEvent('kingLeave'); break;
    case 'kingDialog': if (event.id === 17) sfx.musicEvent('kingIntro'); break;
    case 'race':
      if (event.state === 'start') sfx.musicEvent('raceStart');
      else if (event.state === 'won') { sfx.musicEvent('raceEnd'); sfx.star(); }
      else { sfx.musicEvent('raceEnd'); sfx.bonk(); }
      break;
    case 'bowlingBall': if (event.cue !== 'end') sfx.ballBounce(event.pos); break;
    case 'koopaSound': sfx.koopaSound(event.pos, event.cue); break;
    case 'coinBounce': sfx.coinBounce(event.pos); break;
  }
}

/** Progression cues: the `this.sfx.*` calls of Game.ts that do not come from sim events. */
function glue(sfx: any, cue: any): void {
  switch (cue.name) {
    case 'coin': sfx.coin(cue.underwater); break;
    case 'star': sfx.star(); break;
    case 'starSound': sfx.starSound(); break;
    case 'oneUp': sfx.oneUp(); break;
    case 'teleport': sfx.teleport(); break;
    case 'capSwitch': sfx.capSwitch(); break;
    case 'deathWarp': sfx.deathWarp(); break;
    case 'bonk': sfx.bonk(); break;
    case 'stompGoomba': sfx.stompGoomba(); break;
    case 'signMessage': sfx.signMessage(cue.cue); break;
    case 'courseCoin': sfx.courseCoin(cue.count, cue.record); break;
    case 'secret': sfx.secret(cue.count); break;
    case 'redCoin': sfx.redCoin(cue.count); break;
    case 'spawnedStar': sfx.spawnedStar(cue.pos, cue.appearing); break;
    case 'coordinateStar': sfx.coordinateStar(cue.pos, cue.landed); break;
    case 'cannonDoor': sfx.cannonDoor(cue.pos); break;
    case 'lift': sfx.lift(cue.pos, cue.cue); break;
    case 'moveMenu': sfx.moveMenu(); break;
    case 'selectMission': sfx.selectMission(); break;
    case 'peachVoice': sfx.peachVoice(cue.voice); break;
    case 'musicEvent': sfx.musicEvent(cue.event); break;
    case 'stopMusic': sfx.stopMusic(); break;
  }
}

// ---- the cases ----------------------------------------------------------------------------------------------------------
interface Case { name: string; kind: 'mario' | 'object' | 'glue'; events?: any[]; cue?: any }
const cases: Case[] = [];
const mario = (name: string, ...events: any[]) => cases.push({ name, kind: 'mario', events });
const obj = (name: string, event: any) => cases.push({ name, kind: 'object', events: [event] });
const glueCase = (name: string, cue: any) => cases.push({ name, kind: 'glue', cue });

for (const [n, action] of [['single', ACT_JUMP], ['double', ACT_DOUBLE_JUMP], ['triple', ACT_TRIPLE_JUMP], ['backflip', ACT_BACKFLIP], ['kick', ACT_JUMP_KICK], ['long', ACT_LONG_JUMP]] as const) mario('jump ' + n, { type: 'jump', action, vy: 42 });
mario('triplejump', { type: 'triplejump' });
mario('land', { type: 'land', speed: 10, vy: -40 });
mario('land with ground pound land', { type: 'land', speed: 0, vy: -90 }, { type: 'groundpoundland' });
mario('groundpoundland alone', { type: 'groundpoundland' });
mario('groundpound start', { type: 'groundpound' });
mario('groundpound drop', { type: 'groundpounddrop' });
for (const pullUp of [true, false]) mario('wingFlight ' + pullUp, { type: 'wingFlight', pullUp });
mario('flightBoundary', { type: 'flightBoundary' });
mario('flightBurn', { type: 'flightBurn' });
mario('bonk', { type: 'bonk', speed: 30 });
mario('wallkick', { type: 'wallkick' });
for (const phase of [0, 1, 2, 3]) mario('cannonMove ' + phase, { type: 'cannonMove', phase });
mario('cannonFire', { type: 'cannonFire' });
mario('bowserGrab', { type: 'bowserGrab' });
mario('bowserSpin', { type: 'bowserSpin' });
for (const fast of [true, false]) mario('bowserThrow ' + fast, { type: 'bowserThrow', fast });
mario('waterPlunge', { type: 'waterPlunge' });
for (const fast of [true, false]) mario('swimStroke ' + fast, { type: 'swimStroke', fast });
mario('flutterKick', { type: 'flutterKick' });
mario('longjump', { type: 'longjump' });
mario('dive', { type: 'dive' });
mario('hurt', { type: 'hurt', damage: 2 });
mario('death', { type: 'death' });
mario('deathLanding', { type: 'deathLanding' });
mario('mamaMia', { type: 'mamaMia' });
mario('silent events', { type: 'step', speed: 5 }, { type: 'castleEnding' }, { type: 'endingCredits' }, { type: 'respawn' });
mario('mixed batch', { type: 'jump', action: ACT_DOUBLE_JUMP, vy: 50 }, { type: 'wallkick' }, { type: 'hurt', damage: 1 }, { type: 'land', speed: 1, vy: -20 });

for (const phase of ['drop', 'bounce', 'splash', 'burst']) obj('waterBomb ' + phase, { type: 'waterBomb', pos: P, phase });
obj('waterCannon', { type: 'waterCannon', pos: P });
for (const mine of [true, false]) obj('explosion mine=' + mine, { type: 'explosion', pos: P, radius: 400, mine });
for (const wall of [undefined, 'left', 'right']) obj('break ' + wall, { type: 'break', pos: P, ...(wall ? { wall } : {}) });
obj('object hurt', { type: 'hurt', pos: P });
for (const number of [0, 1, 4]) obj('chest ' + number, { type: 'chest', number });
for (const cue of ['gun', 'slide', 'impact']) obj('racerSound ' + cue, { type: 'racerSound', pos: P, cue });
for (const diving of [true, false]) obj('penguinSound ' + diving, { type: 'penguinSound', pos: P, diving });
for (const cue of ['roll', 'bounce', 'assembled', 'reward']) obj('snowman ' + cue, { type: 'snowman', pos: P, cue });
obj('penguinReward', { type: 'penguinReward', pos: P });
obj('chestReward', { type: 'chestReward' });
for (const type of ['bowser', 'bigBully', 'whompKing', 'bigBoo', 'boss']) obj(type + ' defeated', { type, pos: P });
obj('oneUp', { type: 'oneUp', pos: P });
obj('kingLeave', { type: 'kingLeave' });
for (const id of [17, 116, 128]) obj('kingDialog ' + id, { type: 'kingDialog', id });
for (const state of ['start', 'won', 'lost', 'disqualified']) obj('race ' + state, { type: 'race', state });
for (const cue of ['bounce', 'end']) obj('bowlingBall ' + cue, { type: 'bowlingBall', pos: P, cue });
for (const cue of ['shell', 'stomp', 'defeated']) obj('koopaSound ' + cue, { type: 'koopaSound', pos: P, cue });
obj('coinBounce', { type: 'coinBounce', pos: P });
obj('coin', { type: 'coin', pos: P, loot: false, value: 1 });
obj('movingCoinCollected', { type: 'movingCoinCollected', pos: P });
obj('boxStar', { type: 'boxStar', pos: P, act: 1 });
obj('dialogue', { type: 'dialogue', text: 'x' });

for (const underwater of [false, true]) glueCase('coin ' + underwater, { name: 'coin', underwater });
for (const name of ['star', 'starSound', 'oneUp', 'teleport', 'capSwitch', 'deathWarp', 'bonk', 'stompGoomba', 'moveMenu', 'selectMission', 'stopMusic']) glueCase(name, { name });
for (const cue of ['read', 'open', 'page', 'close']) glueCase('signMessage ' + cue, { name: 'signMessage', cue });
for (const [count, record] of [[50, false], [100, true], [150, false]] as const) glueCase(`courseCoin ${count} ${record}`, { name: 'courseCoin', count, record });
for (const count of [1, 3, 8]) { glueCase('secret ' + count, { name: 'secret', count }); glueCase('redCoin ' + count, { name: 'redCoin', count }); }
for (const appearing of [true, false]) glueCase('spawnedStar ' + appearing, { name: 'spawnedStar', pos: P, appearing });
for (const landed of [true, false]) glueCase('coordinateStar ' + landed, { name: 'coordinateStar', pos: P, landed });
glueCase('cannonDoor', { name: 'cannonDoor', pos: P });
for (const cue of ['moving', 'stop', 'lower', 'upper']) glueCase('lift ' + cue, { name: 'lift', pos: P, cue });
for (const voice of ['Mario', 'Stars', 'Thanks', 'ThankYou', 'Special', 'Cake', 'ForMario', 'Mario2']) glueCase('peach ' + voice, { name: 'peachVoice', voice: 'peach' + voice });
for (const event of ['clear', 'exitKey', 'exitStar', 'spawn', 'puzzle', 'kingIntro', 'kingFight', 'kingDefeat', 'kingLeave', 'raceStart', 'raceEnd']) glueCase('musicEvent ' + event, { name: 'musicEvent', event });

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust cue fixtures', () => {
  it('writes cue goldens', async () => {
    const { Sfx } = await import('../src/game/Sfx');
    const { courseMusic } = await vi.importActual<typeof import('../src/game/music')>('../src/game/music');
    const sfx: any = new Sfx();
    sfx.startMusic(courseMusic('bob', 1));
    await new Promise(r => setTimeout(r, 5));
    sfx.setPlayer({
      pos: [0, 0, 0], metalCapTimer: 0, floor: { type: 1 }, floorHeight: 100, world: { waterLevel: () => 0 },
    }, [0, 0, 0], false);
    const out: any[] = [];
    for (const c of cases) {
      sent.length = 0;
      if (c.kind === 'mario') marioEvents(sfx, c.events!);
      else if (c.kind === 'object') objectEvent(sfx, c.events![0]);
      else glue(sfx, c.cue);
      await new Promise(r => setTimeout(r, 0));
      out.push({ ...c, calls: sent.flatMap(m => calls(m)) });
    }
    mkdirSync('native/sim/tests/fixtures', { recursive: true });
    writeFileSync('native/sim/tests/fixtures/cues.json', JSON.stringify({ cases: out }));
    console.log('cue cases', out.length, 'with calls', out.filter(c => c.calls.length).length);
  }, 120000);
});
