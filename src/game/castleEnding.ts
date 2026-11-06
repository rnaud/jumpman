import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import type { Sfx } from './Sfx';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { instantiateSm64, SM64_MODELS } from './sm64Assets';

export type EndingLine = (typeof dialogue)[number];

export const PEACH = 'actors/peach/peach_geo.glb';
export const TOAD = 'actors/toad/toad_geo.glb';

function actor(asset: GLTF, parent: THREE.Group, x: number, y: number, z: number, successors: Record<number, number>) {
  const model = instantiateSm64(asset);
  model.position.set(x, y, z);
  model.visible = false;
  parent.add(model);
  const mixer = new THREE.AnimationMixer(model);
  let index = -1;
  function play(i: number) {
    if (index === i) return;
    index = i;
    mixer.stopAllAction();
    mixer.clipAction(asset.animations[i]).reset().play();
  }
  mixer.addEventListener('loop', () => {
    const next = successors[index];
    if (next !== undefined) play(next);
  });
  return {model, mixer, play};
}

const dialogue: { phase: number; frame: number; text: string; duration: number; voice?: Parameters<Sfx['peachVoice']>[0] }[] = [
  {phase: 6, frame: 230, text: 'Mario!', duration: 30, voice: 'peachMario'},
  {phase: 6, frame: 290, text: 'The power of the Stars is restored to the castle...', duration: 60, voice: 'peachStars'},
  {phase: 7, frame: 29, text: "...and it's all thanks to you!", duration: 30, voice: 'peachThanks'},
  {phase: 7, frame: 75, text: 'Thank you, Mario!', duration: 30, voice: 'peachThankYou'},
  {phase: 7, frame: 130, text: 'We have to do something special for you...', duration: 40, voice: 'peachSpecial'},
  {phase: 10, frame: 1, text: 'Listen, everybody,', duration: 30, voice: 'peachCake'},
  {phase: 10, frame: 55, text: "let's bake a delicious cake...", duration: 40},
  {phase: 10, frame: 130, text: '...for Mario...', duration: 50, voice: 'peachForMario'},
  {phase: 11, frame: 95, text: 'Mario!', duration: 40, voice: 'peachMario2'},
];

// US timing of end_peach_cutscene_* and the original Princess/Toad animation tables.
export function castleEnding(models: Map<string, GLTF>, parent: THREE.Group) {
  const star = instantiateSm64(models.get(SM64_MODELS.star)!);
  star.scale.multiplyScalar(3);
  star.position.set(0, 2528, -1800);
  star.visible = false;
  parent.add(star);
  const peach = actor(models.get(PEACH)!, parent, 0, 2428, -1300, {0:1,1:2,2:3,6:7,7:8});
  const left = actor(models.get(TOAD)!, parent, -200, 906, -1290, {4:5});
  const right = actor(models.get(TOAD)!, parent, 200, 906, -1290, {6:7});
  let descent = 0;
  let peachY = 2428;
  let lastPhase = -1;
  const cameraTrack = new EndingCamera();
  return (m: MarioRuntime, camera: THREE.PerspectiveCamera) => {
    const phase = m.actionArg, t = m.actionTimer;
    if (phase === 0 && lastPhase !== 0) { descent = 0; peachY = 2428; cameraTrack.reset(); }
    star.visible = phase === 2 || (phase === 3 && t < 40);
    if (star.visible) star.rotation.y += Math.PI / 32;
    peach.model.visible = left.model.visible = right.model.visible = phase > 3 || (phase === 3 && t >= 40);
    if (phase === 4) {
      if (peachY >= 1300 && descent < 60) descent += 5;
      if (peachY < 1300 && descent >= 27) descent -= 2;
      peachY -= Math.trunc(descent / 10);
      if (peachY <= 907) peachY = 906;
    }
    peach.model.position.y = peachY;
    if (peach.model.visible) {
      if (phase === 3 && t === 40) { peach.play(4); left.play(2); right.play(3); }
      if (phase === 5 && t === 22) peach.play(5);
      if (phase === 6 && t === 80) peach.play(6);
      if (phase === 7 && lastPhase !== phase) peach.play(9);
      if (phase === 8 && lastPhase !== phase) peach.play(10);
      if (phase === 10 && t === 1) { peach.play(0); left.play(4); right.play(6); }
      for (const a of [peach, left, right]) a.mixer.update(1/30);
    }
    cameraTrack.step(camera, m, peachY);
    lastPhase = phase;
    return dialogue.find(line => line.phase === phase && line.frame === t);
  };
}

const flyToWindowPos = [
  [0, -86, 876, 640], [0, -86, 876, 610], [0, -66, 945, 393],
  [0, -80, 976, 272], [0, -66, 1306, -36], [0, -70, 1869, -149],
  [0, -10, 2093, -146], [0, -10, 2530, -248], [0, -10, 2530, -263],
  [0, -10, 2530, -273],
];

const flyToWindowFocus = [
  [50, -33, 889, -7], [35, -33, 889, -7], [31, -17, 1070, -193],
  [25, -65, 1182, -272], [20, -64, 1559, -542], [25, -68, 2029, -677],
  [25, -9, 2204, -673], [25, -8, 2529, -772], [0, -8, 2529, -772],
  [0, -8, 2529, -772], [0, -8, 2529, -772],
];

const peachDescentCamPos = [
  [50, 1, 120, -1150], [50, 1, 120, -1150], [40, 118, 121, -1199],
  [40, 147, 74, -1306], [40, 162, 95, -1416], [40, 25, 111, -1555],
  [40, -188, 154, -1439], [40, -203, 181, -1242], [40, 7, 191, -1057],
  [40, 262, 273, -1326], [40, -4, 272, -1627], [35, -331, 206, -1287],
  [30, -65, 219, -877], [25, 6, 216, -569], [25, -8, 157, 40],
  [25, -4, 106, 200], [25, -6, 72, 574], [0, -6, 72, 574],
  [0, -6, 72, 574], [0, -6, 72, 574],
];

const marioToPeachPos = [
  [0, -130, 1111, -1815], [0, -131, 1052, -1820], [0, -271, 1008, -1651],
  [0, -439, 1043, -1398], [0, -433, 1040, -1120], [0, -417, 1040, -1076],
  [0, -417, 1040, -1076], [0, -417, 1040, -1076], [0, -417, 1040, -1076],
];

const marioToPeachFocus = [
  [50, -37, 1020, -1332], [20, -36, 1012, -1330], [20, -24, 1006, -1215],
  [20, 28, 1002, -1224], [24, 45, 1013, -1262], [35, 34, 1000, -1287],
  [0, 34, 1000, -1287], [0, 34, 1000, -1287], [0, 34, 1000, -1287],
];

const lookUpAtCastle = [
  [50, 200, 1066, -1414], [50, 200, 1066, -1414], [30, 198, 1078, -1412],
  [33, 15, 1231, -1474], [39, -94, 1381, -1368], [0, -92, 1374, -1379],
  [0, -92, 1374, -1379], [0, -92, 1374, -1379],
];

const lookAtSkyFocus = [
  [50, 484, 1368, -888], [72, 479, 1372, -892], [50, 351, 1817, -918],
  [50, 351, 1922, -598], [0, 636, 2027, -415], [0, 636, 2027, -415],
  [0, 636, 2027, -415],
];

class EndingCamera {
  private frame = 0;
  private segment = 0;
  private fraction = 0;
  private focus = new THREE.Vector3();

  reset(): void { this.frame = this.segment = this.fraction = 0; }

  // move_point_along_spline: position and focus paths share a segment and clock.
  private point(out: THREE.Vector3, path: number[][]) {
    const s = Math.min(this.segment, path.length - 4), t = this.fraction, u = 1-t;
    const weights = [u*u*u/6, t*t*t/2-t*t+2/3, -t*t*t/2+t*t/2+t/2+1/6, t*t*t/6];
    out.set(0,0,0);
    for (let i = 0; i < 4; i++) {
      const [,x,y,z] = path[s+i], w = weights[i];
      out.x += x*w; out.y += y*w; out.z += z*w;
    }
    const a = path[s+1][0], b = path[s+2][0];
    this.fraction += (a ? u/a : 0) + (b ? t/b : 0);
    if (this.fraction >= 1) { this.segment++; this.fraction--; }
  }

  step(camera: THREE.PerspectiveCamera, m: MarioRuntime, peachY: number): void {
    const t = this.frame++, p = camera.position, f = this.focus;
    // Shot durations from sCutsceneEnding, US version.
    if ([315, 840, 1430, 1525, 1950, 2186, 2431].includes(t)) this.segment = this.fraction = 0;
    if (t < 170) {
      p.set(165,4725,324); f.set(m.pos[0],m.pos[1]+80,m.pos[2]+Math.max(100,Math.abs(m.pos[1]-p.y)*0.1));
    } else if (t < 240) {
      p.set(-86,876,790); f.set(-33,969,-7);
    } else if (t < 315) {
      p.set(-51,988,-202); f.set(85,826,250);
    } else if (t < 701) {
      this.point(p,flyToWindowPos); this.point(f,flyToWindowFocus);
    } else if (t < 840) {
      p.set(179,2463,-1216); f.set(0,peachY+125,-1300);
    } else if (t < 1430) {
      this.point(p,peachDescentCamPos); p.y += peachY; f.set(0,peachY+125,-1300);
    } else if (t < 1525) {
      this.point(p,marioToPeachPos); this.point(f,marioToPeachFocus);
    } else if (t < 1950) {
      p.set(-163,978,-1082);
      if (t === 1525 || t >= 1775) this.point(f,lookUpAtCastle);
    } else if (t < 2186) {
      p.set(-473,970,-1152); f.set(11,983,-1273);
    } else if (t < 2431) {
      if (t < 2341) { p.set(-149,1021,-1216); f.set(350,1034,-1216); }
      else { p.lerp(new THREE.Vector3(-250,966,-1111),0.05); f.lerp(new THREE.Vector3(233,1068,-1298),0.05); }
    } else {
      p.set(699,1680,-703);
      if (t === 2431 || t >= 2931) this.point(f,lookAtSkyFocus);
    }
    camera.lookAt(f);
  }
}

// cutscene_grand_star and cutscene_grand_star_fly in camera.c.
export class GrandStarCamera {
  private frame = 0;
  private focus = new THREE.Vector3();
  private move = new THREE.Vector3();
  private offset = new THREE.Vector3();
  private focusOffset = new THREE.Vector3();
  private speed = 0;

  step(camera: THREE.PerspectiveCamera, m: MarioRuntime): void {
    const t = this.frame++;
    const pos = new THREE.Vector3(...m.pos);
    const p = camera.position;
    if (t < 360) {
      if (!t) this.focus.copy(pos);
      if (t < 110) {
        const diff = p.clone().sub(pos);
        const dist = diff.length();
        const yaw = Math.atan2(diff.x,diff.z);
        const targetYaw = -Math.PI + 0x1200*Math.PI/32768;
        const dyaw = Math.atan2(Math.sin(targetYaw-yaw),Math.cos(targetYaw-yaw));
        const goalY = 457-pos.y;
        const angle = Math.atan2(diff.y,Math.hypot(diff.x,diff.z));
        const pitch = angle + (Math.atan2(goalY,600)-angle)/32;
        const d = dist + (Math.hypot(goalY,600)-dist)*0.1;
        p.set(pos.x + d*Math.cos(pitch)*Math.sin(yaw+dyaw/20), pos.y + d*Math.sin(pitch),
          pos.z + d*Math.cos(pitch)*Math.cos(yaw+dyaw/20));
      } else {
        if (t < 160) { this.move.x = 5; this.move.z = Math.max(-40,this.move.z-2); }
        else { this.move.x = 0; this.move.z = Math.min(0,this.move.z+2); p.x += (pos.x-p.x)*0.01; p.z += (pos.z-p.z)*0.01; }
        p.x -= this.move.x; p.z += this.move.z;
      }
      this.focus.x += (pos.x-this.focus.x)*0.5;
      this.focus.y += ((pos.y-307)*0.5+407-this.focus.y)*0.8;
      this.focus.z += (pos.z-this.focus.z)*0.5;
    } else {
      if (t === 360) this.focusOffset.y = this.focus.y-pos.y;
      if (t <= 500) {
        this.offset.lerp(new THREE.Vector3(-600,0,-400),0.05);
        this.focusOffset.y += THREE.MathUtils.clamp(-this.focusOffset.y,-2,2);
        this.focusOffset.z = Math.max(-200,this.focusOffset.z-6);
      } else {
        const v = Math.min(15,(t-500)*0.1);
        this.offset.x = Math.max(-2000,this.offset.x-v);
        this.offset.y = Math.min(1200,this.offset.y+v/10);
        this.offset.z = Math.min(1000,this.offset.z+v/10);
        this.focusOffset.y = Math.min(1200,this.focusOffset.y+v/2);
        this.focusOffset.z = Math.min(1000,this.focusOffset.z+v/1.5);
      }
      const rotation = new THREE.Euler(-m.faceAngle[0]*Math.PI/32768,m.faceAngle[1]*Math.PI/32768,0,'YXZ');
      const rotate = (v: THREE.Vector3) => new THREE.Vector3(v.x,v.y,-v.z).applyEuler(rotation).add(pos);
      const goal = rotate(this.offset), focus = rotate(this.focusOffset);
      this.speed = Math.min(90,this.speed+2.5);
      p.add(goal.clone().sub(p).clampLength(0,this.speed)).lerp(goal,0.01);
      this.focus.x += (focus.x-this.focus.x)*0.5; this.focus.y += (focus.y-this.focus.y)*0.8; this.focus.z += (focus.z-this.focus.z)*0.5;
    }
    camera.lookAt(this.focus);
  }
}
