import * as THREE from 'three';
import { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';
import { SURFACE_CAMERA_BOUNDARY, SURFACE_DEATH_PLANE, SURFACE_FLAG_NO_CAM_COLLISION } from '../sim/surfaces';
import { type Coin, type PlaygroundResult } from './world';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';
import { addNativeCollision, type CollisionData } from './nativeCollision';
import type { CourseId } from './courses';

export type WorldFactory = (scene: THREE.Scene) => PlaygroundResult;
interface CourseData extends CollisionData {
  model: string;
  spawn: { pos: Vec3; faceYaw: number };
  objects: { model: string; pos: Vec3; yaw: number; collision?: string }[];
  coins: Vec3[];
  extraCollision: Record<string, CollisionData>;
}
const courseUrls = import.meta.glob<string>('../../assets/courses/*.json', { eager: true, query: '?url', import: 'default' });

/** Native terrain collision is kept separate from display geometry. */
export async function prepareCourse(id: Exclude<CourseId, 'playground'>): Promise<WorldFactory> {
  const url = courseUrls[`../../assets/courses/${id}.json`];
  if (!url) throw new Error('Course data missing. Run pnpm export:courses after copying local assets.');
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Course data request failed: ${response.status}`);
  const data: CourseData = await response.json();
  if (id === 'bob') data.objects.push({ model: 'levels/bob/bob_geo_000458.glb', pos: [-2303, 717, 1024], yaw: 45 });
  const paths = [...new Set([data.model, SM64_MODELS.coin, SM64_MODELS.star, ...data.objects.map(o => o.model), ...(id === 'bob' ? [SM64_MODELS.goomba] : [])])];
  const models = new Map(await Promise.all(paths.map(async path => [path, await loadSm64Asset(path)] as const)));
  const make = (path: string) => instantiateSm64(models.get(path)!);
  const root = new THREE.Group();
  root.name = id;
  const terrain = make(data.model);
  root.add(terrain);
  const collision = new CollisionWorld();
  addNativeCollision(collision, data);
  const extras: Record<string, string> = {
    'levels/bob/bob_geo_000458.glb': 'seesaw_platform/collision.inc.c',
    'levels/wf/wf_geo_0008A8.glb': 'areas/1/10/collision.inc.c',
    'levels/wf/wf_geo_0008E8.glb': 'areas/1/11/collision.inc.c',
  };
  for (const object of data.objects) {
    const model = make(object.model);
    model.position.fromArray(object.pos);
    model.rotation.y = object.yaw * Math.PI / 180;
    root.add(model);
    const extra = data.extraCollision[object.collision ?? extras[object.model]];
    if (extra) addNativeCollision(collision, extra, object.pos, object.yaw);
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
  const floorAt = (x: number, y: number, z: number) => {
    const hit = collision.findFloor(x, y, z);
    if (!hit.surface || hit.surface.type === SURFACE_DEATH_PLANE || hit.surface.type === SURFACE_CAMERA_BOUNDARY) throw new Error(`No safe floor in ${id} at ${x}, ${y}, ${z}`);
    return hit.height;
  };
  const [sx, sy, sz] = data.spawn.pos;
  data.spawn.pos[1] = floorAt(sx, sy + 200, sz);
  const bobCoins = [[-6360, 6280], [-6150, 6100], [-5880, 5800], [-5500, 5500], [-5000, 5200], [-4500, 5000], [-4000, 4700], [-3500, 4200], [-3000, 3500], [-2600, 2600], [-2200, 1600], [-1700, 700], [-1000, 400]];
  const coinSpots: Vec3[] = id === 'bob' ? bobCoins.map(([x, z]) => [x, floorAt(x, 20000, z) + 65, z]) : data.coins;
  const coins: Coin[] = coinSpots.map(pos => {
    const group = make(SM64_MODELS.coin);
    group.scale.multiplyScalar(0.7);
    const position = new THREE.Vector3(...pos);
    group.position.copy(position);
    root.add(group);
    return { group, position, collected: false };
  });
  // Exploration goals; these do not claim to reproduce the original star missions.
  const starLocations: Record<string, Vec3> = { bob: [1636, 20000, -5567], castle_grounds: [0, 1000, -1300], wf: [1707, 2700, -170], pss: [-6385, -4000, 5770] };
  const [x, y, z] = starLocations[id];
  const starY = floorAt(x, y, z) + 100;
  const star = make(SM64_MODELS.star);
  star.scale.multiplyScalar(0.25);
  star.position.set(x, starY, z);
  root.add(star);
  const starLight = new THREE.PointLight(0xffd969, 200, 450, 2);
  starLight.position.copy(star.position);
  root.add(starLight);
  const mixers: THREE.AnimationMixer[] = [];
  if (id === 'bob') {
    for (const [x, z] of [[-4800, 5700], [-3700, 5300]]) {
      const goomba = make(SM64_MODELS.goomba);
      goomba.position.set(x, floorAt(x, 20000, z), z); root.add(goomba);
      const mixer = new THREE.AnimationMixer(goomba.children[0]);
      const clip = models.get(SM64_MODELS.goomba)!.animations[0];
      if (clip) mixer.clipAction(clip).play();
      mixers.push(mixer);
    }
  }
  const billboards = new Sm64Billboards(root);
  let lastTime = 0;
  const world: PlaygroundResult = {
    collision, collisionMeshes: [helper], coins, star, starLight, spawn: data.spawn,
    deathHeight: Math.min(...data.vertices.map(v => v[1])) - 500,
    requireCoins: id === 'bob',
    update(time) {
      for (const mixer of mixers) mixer.update(Math.min(0.1, time - lastTime));
      lastTime = time;
      for (const coin of coins) coin.group.position.y = coin.position.y + Math.sin(time * 2.2) * 5;
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
