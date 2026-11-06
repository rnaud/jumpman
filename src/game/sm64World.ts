import * as THREE from 'three';
import { MovingPlatform } from './movingPlatform';
import { WalkingGoomba } from './goomba';
import { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';
import { SURFACE_CAMERA_BOUNDARY, SURFACE_DEATH_PLANE, SURFACE_FLAG_NO_CAM_COLLISION } from '../sim/surfaces';
import { type Coin, type CastleDoor, type PlaygroundResult } from './world';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';
import { addNativeCollision, type CollisionData } from './nativeCollision';
import type { CourseId } from './courses';
import { CourseObjects } from './courseObjects';
import type { ObjectBehaviorKind } from '../sim/object';

export type WorldFactory = (scene: THREE.Scene) => PlaygroundResult;
interface CourseData extends CollisionData {
  doors?: CastleDoor[];
  goombas?: { pos: Vec3; triplet: boolean }[];
  waterBoxes?: CollisionWorld['waterBoxes'];
  model: string;
  spawn: { pos: Vec3; faceYaw: number };
  objects: { model: string; pos: Vec3; yaw: number; collision?: string; param?: number; behavior?: ObjectBehaviorKind; acts?: number[]; warpTo?: CourseId }[];
  coins: Vec3[];
  extraCollision: Record<string, CollisionData>;
}
const courseUrls = import.meta.glob<string>('../../assets/courses/*.json', { eager: true, query: '?url', import: 'default' });

/** Native terrain collision is kept separate from display geometry. */
export async function prepareCourse(id: Exclude<CourseId, 'playground'>, entry?: string | null, push = false, act = 1): Promise<WorldFactory> {
  const url = courseUrls[`../../assets/courses/${id}.json`];
  if (!url) throw new Error('Course data missing. Run pnpm export:courses after copying local assets.');
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Course data request failed: ${response.status}`);
  const data: CourseData = await response.json();
  if (id === 'bob') data.objects.push({ model: 'levels/bob/bob_geo_000458.glb', pos: [-2303, 717, 1024], yaw: 45 });
  if (id === 'castle_inside') data.objects.push({ model: '', pos: [2013, 768, -2014], yaw: 0, behavior: 'warp', warpTo: 'pss' });
  if (id === 'bob' && data.objects.some(object => object.behavior === 'waterBombCannon')) {
    data.objects.push({
      model: 'actors/cannon_barrel/cannon_barrel_geo.glb',
      pos: [-5694, 128, 5600],
      yaw: 135,
      behavior: 'cannonBarrel',
      acts: [1],
    });
  }
  const activeObjects = data.objects.filter(object => !object.acts || object.acts.length === 0 || object.acts.includes(act));
  const chainModels = activeObjects.some(o => o.behavior === 'chainChomp')
    ? ['actors/poundable_pole/wooden_post_geo.glb', 'actors/chain_ball/metallic_ball_geo.glb']
    : [];
  const paths = [...new Set([data.model, SM64_MODELS.coin, SM64_MODELS.star, ...activeObjects.map(o => o.model).filter(Boolean),
    ...(activeObjects.some(o => o.behavior === 'waterBombCannon') ? ['actors/water_bubble/water_bomb_geo.glb'] : []),
    ...(activeObjects.some(o => o.behavior === 'koopa') ? ['actors/koopa_shell/koopa_shell_geo.glb'] : []),
    ...chainModels, ...(id === 'bob' || data.goombas?.length ? [SM64_MODELS.goomba] : [])])];
  const models = new Map(await Promise.all(paths.map(async path => [path, await loadSm64Asset(path)] as const)));
  const make = (path: string) => instantiateSm64(models.get(path)!);
  const root = new THREE.Group();
  root.name = id;
  const terrain = make(data.model);
  root.add(terrain);
  const collision = new CollisionWorld();
  collision.waterBoxes = data.waterBoxes ?? [];
  addNativeCollision(collision, data);
  const extras: Record<string, string> = {
    'levels/bob/bob_geo_000458.glb': 'seesaw_platform/collision.inc.c',
    'levels/wf/wf_geo_0008A8.glb': 'areas/1/10/collision.inc.c',
    'levels/wf/wf_geo_0008E8.glb': 'areas/1/11/collision.inc.c',
  };
  const mixers: THREE.AnimationMixer[] = [];
  const platforms: MovingPlatform[] = [];
  for (const object of activeObjects) {
    // Interactive objects are instantiated by CourseObjects below.
    if (object.behavior) continue;
    const model = make(object.model);
    model.position.fromArray(object.pos);
    model.rotation.y = object.yaw * Math.PI / 180;
    // Trees use an object-level billboard flag in the game, outside the geo layout.
    if (object.model === SM64_MODELS.tree) {
      model.userData.objectBillboard = true;
      model.traverse(node => {
        if (!(node instanceof THREE.Mesh)) return;
        for (const material of Array.isArray(node.material) ? node.material : [node.material]) {
          material.side = THREE.DoubleSide;
        }
      });
    }
    root.add(model);
    if (object.model === 'actors/door/castle_door_geo.glb') {
      const entrance = data.doors?.find(d => d.pos.every((v, i) => v === object.pos[i]));
      if (entrance) {
        const mixer = new THREE.AnimationMixer(model.children[0]);
        mixers.push(mixer);
        entrance.play = index => {
          const clip = models.get(object.model)?.animations[index];
          if (!clip) return;
          mixer.stopAllAction();
          const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1);
          action.clampWhenFinished = true; action.play();
        };
        entrance.play(0);
      }
    }
    const extra = data.extraCollision[object.collision ?? extras[object.model]];
    const kind = (object.collision ?? extras[object.model] ?? '').split('/')[0];
    if (extra && ['seesaw_platform', 'rotating_platform', 'rotating_wooden_platform', 'sliding_platform', 'small_bomp', 'large_bomp', 'mesh_elevator', 'merry_go_round', 'moving_bookshelf', 'tilting_trap_platform'].includes(kind)) {
      const platform = new MovingPlatform(model, extra, kind, object.param ?? (kind === 'rotating_platform' ? 8 : kind === 'sliding_platform' ? 2 : 0));
      platforms.push(platform);
      root.add(platform.blocker);
    } else if (extra) addNativeCollision(collision, extra, object.pos, object.yaw);
  }
  collision.finalize();
  // Camera collision uses native surfaces too. Invisible helper supports G wireframe.
  const points: number[] = [];
  for (const surface of collision.surfaces) {
    if (surface.flags & SURFACE_FLAG_NO_CAM_COLLISION || surface.type === SURFACE_DEATH_PLANE) continue;
    points.push(...surface.v1, ...surface.v2, ...surface.v3);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
  const helper = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ color: 0x44ff88, wireframe: true, side: THREE.DoubleSide }));
  helper.visible = false;
  helper.userData.nativeCollision = true;
  root.add(helper);
  for (const platform of platforms) platform.refresh(collision);
  const floorAt = (x: number, y: number, z: number) => {
    const hit = collision.findFloor(x, y, z);
    if (!hit.surface || hit.surface.type === SURFACE_DEATH_PLANE || hit.surface.type === SURFACE_CAMERA_BOUNDARY) throw new Error(`No safe floor in ${id} at ${x}, ${y}, ${z}`);
    return hit.height;
  };
  const door = data.doors?.find(d => d.node === entry);
  if (door) {
    const angle = door.yaw * Math.PI / 180 + (push ? Math.PI : 0);
    data.spawn = { pos: [door.pos[0] + 300 * Math.sin(angle), door.pos[1],
      door.pos[2] + 300 * Math.cos(angle)], faceYaw: Math.round(angle * 32768 / Math.PI) };
    door.play?.(push ? 4 : 3);
  }
  const [sx, sy, sz] = data.spawn.pos;
  data.spawn.pos[1] = floorAt(sx, sy + 200, sz);
  const bobCoins = [[-6360, 6280], [-6150, 6100], [-5880, 5800], [-5500, 5500], [-5000, 5200], [-4500, 5000], [-4000, 4700], [-3500, 4200], [-3000, 3500], [-2600, 2600], [-2200, 1600], [-1700, 700], [-1000, 400]];
  const coinSpots: Vec3[] = id === 'bob' ? bobCoins.map(([x, z]) => [x, floorAt(x, 20000, z) + 65, z]) : data.coins;
  const redCoinSpots: Vec3[] = id === 'bob' && act === 4
    ? [[-6360, 6280], [-5880, 5800], [-5000, 5200], [-4000, 4700], [-3000, 3500], [-2200, 1600], [-1000, 400], [-600, 500]]
      .map(([x, z]) => [x, floorAt(x, 20000, z) + 70, z])
    : [];
  const coins: Coin[] = coinSpots.map(pos => {
    const group = make(SM64_MODELS.coin);
    group.scale.multiplyScalar(0.7);
    group.userData.objectBillboard = true;
    const position = new THREE.Vector3(...pos);
    group.position.copy(position);
    root.add(group);
    return { group, position, collected: false };
  });
  // Exploration goals; these do not claim to reproduce the original star missions.
  const starLocations: Record<string, Vec3> = { bob: [1636, 20000, -5567], castle_grounds: [0, 1000, -1300], wf: [0, 20000, 0], bbh: [102, 20000, 218], ccm: [512, 20000, -511], jrb: [-4161, 20000, 6315], lll: [0, 20000, -4385], ssl: [-8191, 20000, 1536], wdw: [-255, 20000, -4351], sl: [-5688, 20000, -8191], ttm: [0, 20000, 0], thi: [-1023, 20000, -2555], ttc: [1457, 20000, -602], rr: [-2880, 20000, -6570], pss: [-6385, -4000, 5770] };
  const missionStar = id === 'bob'
    ? act === 2 ? [3304, 4242, -4603] : act === 3 ? [1456, 20000, 446] : starLocations[id]
    : starLocations[id];
  const [x, y, z] = missionStar ?? data.spawn.pos;
  const starY = floorAt(x, y, z) + 100;
  const redCoins: Coin[] = redCoinSpots.map(pos => {
    const group = make(SM64_MODELS.coin);
    group.scale.multiplyScalar(0.7);
    group.userData.objectBillboard = true;
    group.traverse(node => {
      if (!(node instanceof THREE.Mesh)) return;
      const material = new THREE.MeshStandardMaterial({ color: 0xff4538, emissive: 0x3d0805, roughness: 0.35, metalness: 0.2 });
      node.material = material;
    });
    const position = new THREE.Vector3(...pos);
    group.position.copy(position);
    root.add(group);
    return { group, position, collected: false };
  });
  const star = make(SM64_MODELS.star);
  star.scale.multiplyScalar(0.25);
  star.position.set(x, starY, z);
  star.visible = id !== 'castle_inside';
  root.add(star);
  const starLight = new THREE.PointLight(0xffd969, 200, 450, 2);
  starLight.position.copy(star.position);
  root.add(starLight);
  const goombas: WalkingGoomba[] = [];
  const legacyGoombas: Vec3[] = id === 'bob' ? [[-4800, 20000, 5700], [-3700, 20000, 5300]] : [];
  for (const spawn of data.goombas ?? legacyGoombas.map(pos => ({ pos, triplet: false }))) {
    for (let i = 0; i < (spawn.triplet ? 3 : 1); i++) {
      const angle = i * 0x5555 * Math.PI / 32768;
      const x = spawn.pos[0] + (spawn.triplet ? Math.trunc(500 * Math.cos(angle)) : 0);
      const z = spawn.pos[2] + (spawn.triplet ? Math.trunc(500 * Math.sin(angle)) : 0);
      const floor = collision.findFloor(x, spawn.pos[1] + 200, z);
      if (!floor.surface) continue;
      const goomba = make(SM64_MODELS.goomba);
      goomba.scale.multiplyScalar(1.5);
      goomba.position.set(x, floor.height, z);
      root.add(goomba);
      const actor = new WalkingGoomba(goomba, spawn.triplet ? spawn.pos : undefined);
      actor.reset();
      goombas.push(actor);
      const mixer = new THREE.AnimationMixer(goomba.children[0]);
      const clip = models.get(SM64_MODELS.goomba)!.animations[0];
      if (clip) mixer.clipAction(clip).play();
      mixers.push(mixer);
    }
  }
  const billboards = new Sm64Billboards(root);
  const courseObjects = new CourseObjects({
    root,
    models,
    collision,
    objects: activeObjects,
    coins,
    makeCoin: () => make(SM64_MODELS.coin),
    billboards,
  });
  let lastTime = 0;
  const world: PlaygroundResult = {
    dispose() {
      courseObjects.dispose();
      for (const mixer of mixers) mixer.stopAllAction();
      geometry.dispose();
      helper.material.dispose();
    },
    reset() { courseObjects.reset(); },
    collision, platforms, collisionMeshes: [helper, ...platforms.map(p => p.blocker)], coins, redCoins, star, starLight, spawn: data.spawn,
    deathHeight: Math.min(...data.vertices.map(v => v[1])) - 500,
    goombas, doors: data.doors, objects: courseObjects,
    requireCoins: id === 'bob',
    starRequiresEvent: id === 'bob' || id === 'bbh' || id === 'wf' || id === 'lll',
    requiredCoins: coinSpots.length,
    update(time) {
      for (const mixer of mixers) mixer.update(Math.min(0.1, time - lastTime));
      courseObjects.update(time);
      lastTime = time;
      for (const coin of coins) coin.group.position.y = coin.position.y + Math.sin(time * 2.2) * 5;
      for (const coin of redCoins) coin.group.position.y = coin.position.y + Math.sin(time * 2.8) * 6;
      star.rotation.y = time * 1.25;
      star.position.y = starY + Math.sin(time * 1.6) * 14;
      starLight.position.copy(star.position);
    },
    faceCamera(camera) {
      billboards.update(camera);
      for (const coin of coins) coin.group.quaternion.copy(camera.quaternion);
    },
  };
  return scene => { scene.add(root); return world; };
}
export const prepareBattlefield = () => prepareCourse('bob');
