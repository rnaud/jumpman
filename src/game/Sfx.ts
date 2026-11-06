import { createMusicNode, type MusicTrack } from './music';
import sounds from '../../assets/audio/music/sounds.json';
import { terrainSound } from './terrainSound';
import type { MarioRuntime } from '../sim/MarioRuntime';

type Sound = keyof typeof sounds;
const peachVoices = {
  peachMario: 'PEACH_MARIO', peachStars: 'PEACH_POWER_OF_THE_STARS', peachThanks: 'PEACH_THANKS_TO_YOU',
  peachThankYou: 'PEACH_THANK_YOU_MARIO', peachSpecial: 'PEACH_SOMETHING_SPECIAL', peachCake: 'PEACH_BAKE_A_CAKE',
  peachForMario: 'PEACH_FOR_MARIO', peachMario2: 'PEACH_MARIO2',
} as const;

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
    if (!muted && this.currentMusic) this.startMusic(this.currentMusic);
    try { localStorage.setItem('jumpman-muted', String(muted)); } catch { /* optional storage */ }
  }

  private currentMusic: MusicTrack | null = null;
  private node: AudioWorkletNode | null = null;
  private loadingMusic = false;
  private power = 0;
  private location = [0, 1, 0, 0, 0, 0, 0];

  startMusic(track: MusicTrack): void {
    const previous = this.currentMusic;
    this.currentMusic = track;
    this.location[0] = track.level;
    if (this.node) {
      if (previous?.sequence !== track.sequence || previous.preset !== track.preset || previous.level !== track.level)
        this.node.port.postMessage({ track, location: this.location, power: [this.power, 0] });
      return;
    }
    if (this.loadingMusic || this.muted) return;
    this.ensure();
    if (!this.context || !this.master) return;
    this.loadingMusic = true;
    void createMusicNode(this.context).then(node => {
      this.node = node;
      node.connect(this.master!);
      if (this.currentMusic) node.port.postMessage({ track: this.currentMusic, location: this.location, power: [this.power, 0] });
      for (const pending of this.pendingSounds.splice(0)) {
        if (performance.now() - pending.time < 250) node.port.postMessage({sound: pending.sound});
      }
    }).catch(error => console.error('[jumpman] native music unavailable', error))
      .finally(() => { this.loadingMusic = false; });
  }

  setMusicLocation(area: number, room: number, pos: readonly number[], stars: number): void {
    if (!this.currentMusic) return;
    const loc = this.location;
    if (loc[1] === area && loc[2] === room && loc[3] === pos[0] && loc[4] === pos[1] && loc[5] === pos[2] && loc[6] === stars) return;
    loc[1] = area; loc[2] = room;
    [loc[3],loc[4],loc[5]] = pos;
    loc[6] = stars;
    this.node?.port.postMessage({ location: loc });
  }

  setPowerMusic(seq: number, fade: boolean): void {
    if (seq === this.power && !fade) return;
    this.power = seq;
    this.node?.port.postMessage({power: [seq, Number(fade)]});
  }

  stopMusic(): void {
    this.power = 0;
    this.currentMusic = null;
    this.node?.port.postMessage({ stop: true });
  }

  musicTrack(): string | null { return this.currentMusic?.name ?? null; }

  private context: AudioContext | null = null;
  private master: GainNode | null = null;
  private surface = 0;
  private metal = false;
  private marioPos: readonly number[] = [0,0,0];
  private nextSource = 2;
  private pendingSounds: { time: number; sound: number[] }[] = [];

  setPlayer(mario: MarioRuntime, cameraPos: readonly number[], lava: boolean): void {
    const [x,,z] = mario.pos;
    this.marioPos = cameraPos;
    this.metal = mario.metalCapTimer > 0;
    this.surface = terrainSound(this.currentMusic?.terrain ?? 0, mario.floor?.type,
      !lava && mario.floorHeight < mario.world.waterLevel(x,z) - 10);
  }

  private effect(name: Sound, offset = 0, variants = 1, pos = this.marioPos, source = 0): void {
    const sound = [sounds[name] + (offset << 16), variants, source, ...pos];
    if (this.node) this.node.port.postMessage({sound});
    else if (this.loadingMusic && !this.muted) {
      this.pendingSounds.push({time: performance.now(), sound});
      if (this.pendingSounds.length > 16) this.pendingSounds.shift();
    }
  }

  private terrain(normal: Sound, metal: Sound): void {
    this.effect(this.metal ? metal : normal, this.metal ? 0 : this.surface);
  }

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
  }

  peachVoice(key: keyof typeof peachVoices): void { this.effect(peachVoices[key]); }
  jump(voice: Sound = 'MARIO_YAH_WAH_HOO', variants = 3): void {
    this.terrain('ACTION_TERRAIN_JUMP','ACTION_METAL_JUMP');
    this.effect(voice,0,variants);
  }
  doubleJump(): void { this.jump('MARIO_HOOHOO',1); }
  tripleJump(): void { this.jump('MARIO_YAHOO_WAHA_YIPPEE',5); }
  longJump(): void { this.jump('MARIO_YAHOO',1); }
  kick(): void { this.effect('MARIO_PUNCH_HOO'); }
  wallKick(): void { this.effect('MARIO_YAH_WAH_HOO',0,3); }
  splash(): void { this.effect('ACTION_UNKNOWN430'); }
  swim(fast: boolean): void { this.effect(fast ? 'ACTION_SWIM_FAST' : 'ACTION_SWIM'); }
  flutter(): void { this.effect('ACTION_UNKNOWN434'); }
  footstep(tiptoe: boolean): void {
    if (tiptoe) this.terrain('ACTION_TERRAIN_STEP_TIPTOE','ACTION_METAL_STEP_TIPTOE');
    else this.terrain('ACTION_TERRAIN_STEP','ACTION_METAL_STEP');
  }
  rideShell(lava: boolean): void {
    if (lava) this.effect('MOVING_RIDING_SHELL_LAVA');
    else this.effect('MOVING_TERRAIN_RIDING_SHELL',this.surface);
  }
  stompGoomba(): void { this.effect('ACTION_BOUNCE_OFF_OBJECT'); this.effect('OBJ_STOMPED'); }
  teleport(): void { this.effect('ACTION_TELEPORT'); }
  shellJump(): void { this.effect('ACTION_TERRAIN_JUMP',this.surface); }
  land(): void { this.terrain('ACTION_TERRAIN_LANDING','ACTION_METAL_LANDING'); }
  groundPound(): void { this.terrain('ACTION_TERRAIN_HEAVY_LANDING','ACTION_METAL_HEAVY_LANDING'); }
  signMessage(cue: 'read' | 'open' | 'page' | 'close'): void {
    this.effect(({read:'ACTION_READ_SIGN',open:'MENU_MESSAGE_APPEAR',page:'MENU_MESSAGE_NEXT_PAGE',close:'MENU_MESSAGE_DISAPPEAR'} as const)[cue]);
  }
  cannonDoor(pos: readonly number[]): void { this.objectSound('GENERAL_CANNON_UP',pos); }
  cannonMove(phase: 0 | 1 | 2 | 3): void {
    const sounds = ['OBJ_CANNON1','OBJ_CANNON2','OBJ_CANNON3','MOVING_AIM_CANNON'] as const;
    this.effect(sounds[phase]);
  }
  waterBomb(pos: number[], phase: 'drop' | 'bounce' | 'splash'): void {
    this.objectSound(({drop:'OBJ_SOMETHING_LANDING',bounce:'OBJ_WATER_BOMB_BOUNCING',splash:'OBJ_DIVING_IN_WATER'} as const)[phase],pos);
  }
  lift(pos: readonly number[], cue: 'moving' | 'stop' | 'lower' | 'upper'): void {
    if (cue === 'lower' || cue === 'upper') {
      this.effect('ENV_ELEVATOR4',0,1,pos,cue === 'lower' ? 14 : 15);
      return;
    }
    this.effect(cue === 'moving' ? 'ENV_ELEVATOR3' : 'GENERAL_UNKNOWN4_LOWPRIO', 0, 1, pos, 16);
  }
  snowman(pos:readonly number[],cue:'roll'|'bounce'|'assembled'):void {
    const sounds={roll:'ENV_UNKNOWN2',bounce:'OBJ_SNOWMAN_BOUNCE',assembled:'OBJ_SNOWMAN_EXPLODE'} as const;
    this.effect(sounds[cue],0,1,pos,17);
  }
  racerSound(pos:readonly number[],cue:'gun'|'slide'|'impact'): void {
    const sound = {gun:'GENERAL_RACE_GUN_SHOT',slide:'AIR_ROUGH_SLIDE',impact:'OBJ_POUNDING_LOUD'} as const;
    this.effect(sound[cue],0,1,pos,17);
  }
  penguin(pos:number[], diving:boolean): void { this.objectSound(diving ? 'OBJ_BABY_PENGUIN_DIVE' : 'OBJ2_BABY_PENGUIN_YELL',pos); }
  blastWall(pos: number[]): void { this.objectSound('GENERAL_WALL_EXPLOSION',pos); }
  waterCannon(pos: number[]): void { this.objectSound('OBJ_CANNON4',pos); }
  capSwitch(): void { this.effect('GENERAL_ACTIVATE_CAP_SWITCH'); }
  coordinateStar(pos: readonly number[], landed: boolean): void {
    this.effect('ENV_STAR',0,1,pos,17);
    if (landed) this.objectSound('GENERAL_STAR_APPEARS',pos);
  }
  starSound(): void { this.effect('MENU_STAR_SOUND'); }
  wingFlight(pullUp: boolean): void {
    if (pullUp) {
      this.effect('ACTION_FLYING_FAST');
      this.effect('MARIO_YAHOO_WAHA_YIPPEE',0,5);
    }
    this.effect('MOVING_FLYING');
  }
  flightBoundary(): void { this.effect('ACTION_HIT'); }
  flightBurn(): void { this.effect('MARIO_ON_FIRE'); }
  cannonFire(): void {
    this.effect('ACTION_FLYING_FAST');
    this.effect('OBJ_POUNDING_CANNON');
    this.effect('MARIO_YAHOO');
  }
  bowserGrab(): void { this.effect('MARIO_HRMM'); this.effect('OBJ_BOWSER_TAIL_PICKUP'); }
  bowserSpin(): void { this.effect('OBJ_BOWSER_SPINNING'); }
  bowserThrow(fast: boolean): void { this.effect(fast ? 'MARIO_SO_LONGA_BOWSER' : 'MARIO_HERE_WE_GO'); }
  chestAnswer(correct: boolean): void {
    this.effect(correct ? 'GENERAL2_RIGHT_ANSWER' : 'MENU_CAMERA_BUZZ',0,1,[0,0,0],1);
  }
  bonk(): void { this.effect('ACTION_BONK'); }
  hurt(): void { this.effect('MARIO_ATTACKED'); }
  death(): void { this.effect('MARIO_DYING'); }
  groundPoundStart(): void { this.effect('ACTION_THROW'); this.effect('ACTION_SPIN'); }
  groundPoundDrop(): void { this.effect('MARIO_GROUND_POUND_WAH'); }
  dive(): void { this.effect('ACTION_THROW'); this.effect('MARIO_HOOHOO'); }
  koopaSound(pos: readonly number[], cue: 'shell' | 'stomp' | 'defeated'): void {
    this.objectSound(({shell:'OBJ_KOOPA_DAMAGE',stomp:'OBJ_STOMPED',defeated:'OBJ_KOOPA_FLYGUY_DEATH'} as const)[cue],pos);
  }
  ballBounce(pos: readonly number[]): void { this.objectSound('GENERAL_QUIET_POUND1_LOWPRIO',pos); }
  coinBounce(pos: readonly number[]): void { this.objectSound('GENERAL_COIN_DROP',pos); }
  courseCoin(count: number, record: boolean): void {
    this.effect('MENU_YOSHI_GAIN_LIVES');
    if ([50,100,150].includes(count)) this.effect('GENERAL_COLLECT_1UP');
    if (record) this.effect('MENU_MARIO_CASTLE_WARP2');
  }

  coin(underwater = false): void { this.effect(underwater ? 'GENERAL_COIN_WATER' : 'GENERAL_COIN'); }
  secret(count: number): void { this.effect('MENU_COLLECT_SECRET',count-1,1,[0,0,0],1); }
  redCoin(count: number): void { this.effect('MENU_COLLECT_RED_COIN',count-1,1,[0,0,0],1); }
  star(): void { this.effect('MARIO_HERE_WE_GO'); }

  private objectSound(sound: Sound, pos: readonly number[]): void {
    this.effect(sound,0,1,pos,this.nextSource);
    this.nextSource = 2 + ((this.nextSource - 1) % 12);
  }
  explosion(pos = this.marioPos, mine = false): void {
    this.objectSound(mine ? 'GENERAL_BOWSER_BOMB_EXPLOSION' : 'GENERAL2_BOBOMB_EXPLOSION',pos);
  }
  breakBox(pos: readonly number[]): void { this.objectSound('GENERAL_BREAK_BOX',pos); }

  musicEvent(event: 'clear' | 'exitKey' | 'exitStar' | 'spawn' | 'puzzle' | 'kingIntro' | 'kingFight' | 'kingDefeat' | 'kingLeave' | 'raceStart' | 'raceEnd'): void {
    this.node?.port.postMessage({event});
  }

  setOutdoorSources(birds: number[][]): void {
    this.node?.port.postMessage({birds});
  }

}
