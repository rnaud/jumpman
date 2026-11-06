import { KOOPA_RACE_STAR } from '../sim/behaviors/koopaTheQuick';
import * as THREE from 'three';
import { KokiriSwordQuest } from './kokiriSwordQuest';
import { VillageRupees, prepareKokiriBackground, type KokiriScene } from './kokiriVillage';
import { castleEnding, PEACH, TOAD } from './castleEnding';
import { WingSwitch, SWITCH_BASE, SWITCH_TOP } from './wingSwitch';
import { Cannons, CANNON_BASE, CANNON_BARREL, type CannonPlacement } from './cannons';
import { MovingPlatform } from './movingPlatform';
import { ActorShadows } from './actorShadows';
import { WalkingGoomba } from './goomba';
import { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';
import { SURFACE_CAMERA_BOUNDARY, SURFACE_DEATH_PLANE, SURFACE_FLAG_NO_CAM_COLLISION } from '../sim/surfaces';
import { type Coin, type CastleDoor, type PlaygroundResult } from './world';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';
import { addNativeCollision, type CollisionData } from './nativeCollision';
import { SaveFile, activeSaveSlot } from './save';
import { buildCastlePaintings, type PaintingArt } from './castlePaintings';
import { RoomDoors, DoorCollider, type RoomDoor, type DoorMotion } from './roomDoors';
import { formationCoins, type CoinFormation } from './coinFormation';
import { SCORE_COURSES, type CourseId } from './courses';
import { CoinEffects, coinEffectPaths } from './coinEffects';
import { CourseObjects } from './courseObjects';
import type { ObjectBehaviorKind } from '../sim/object';

import { ACT_DEATH_EXIT, ACT_FALLING_DEATH_EXIT, ACT_UNUSED_DEATH_EXIT, ACT_SPECIAL_DEATH_EXIT, ACT_FLYING, ACT_END_PEACH_CUTSCENE, ACT_WATER_IDLE, ACT_SPECIAL_EXIT_AIRBORNE, ACT_SPAWN_NO_SPIN_AIRBORNE, ACT_EXIT_AIRBORNE, ACT_FALLING_EXIT_AIRBORNE, ACT_SPAWN_SPIN_AIRBORNE } from '../sim/state';

const warpActions = {
  bhvDeathWarp: ACT_FALLING_DEATH_EXIT,
  bhvPaintingDeathWarp: ACT_DEATH_EXIT,
  bhvAirborneDeathWarp: ACT_UNUSED_DEATH_EXIT,
  bhvLaunchDeathWarp: ACT_SPECIAL_DEATH_EXIT,
  bhvFlyingWarp: ACT_FLYING,
  bhvSwimmingWarp: ACT_WATER_IDLE,
  bhvLaunchStarCollectWarp: ACT_SPECIAL_EXIT_AIRBORNE,
  bhvAirborneWarp: ACT_SPAWN_NO_SPIN_AIRBORNE,
  bhvSpinAirborneCircleWarp: ACT_SPAWN_SPIN_AIRBORNE,
  bhvSpinAirborneWarp: ACT_SPAWN_SPIN_AIRBORNE,
  bhvPaintingStarCollectWarp: ACT_EXIT_AIRBORNE,
  bhvAirborneStarCollectWarp: ACT_FALLING_EXIT_AIRBORNE,
};

export type WorldFactory = (scene: THREE.Scene) => PlaygroundResult;
interface CourseData extends CollisionData {
  explorationOnly?: boolean;
  kokiri?: KokiriScene;
  horizontalBoundary?: number;
  doors?: CastleDoor[];
  roomDoors?: RoomDoor[];
  doorMotion?: DoorMotion;
  fadingWarps?: PlaygroundResult['fadingWarps'];
  instantWarps?: PlaygroundResult['instantWarps'];
  goombas?: { pos: Vec3; triplet: boolean }[];
  floorWarp?: PlaygroundResult['floorWarp'];
  drainWater?: boolean;
  shipCollision?: {name: string; acts: number[]; pos: Vec3; rotation: Vec3}[];
  slideTerrain?: boolean;
  snowTerrain?: boolean;
  waterBoxes?: CollisionWorld['waterBoxes'];
  model: string;
  spawn: PlaygroundResult['spawn'];
  successEntry?: string;
  deathWarp?: PlaygroundResult['deathWarp'];
  fallWarp?: PlaygroundResult['fallWarp'];
  successArea?: number;
  roomModels?: string[];
  roomPaintings?: number[][];
  paintingArt?: PaintingArt;
  paintings?: PlaygroundResult['paintings'];
  warps?: { pos: Vec3; param: number; warpTo: CourseId; warpEntry: string; warpArea?: number }[];
  warpSpawns?: Record<string, { pos: Vec3; faceYaw: number; behavior: keyof typeof warpActions }>;
  objects: { speaker?: string; actorScale?: number; spinZ?: number; treasureKey?: string; groundedProp?: boolean; recoveryHeart?: boolean; scrollT?: number; billboard?: boolean; pages?: string[]; rupee?: string; fairy?: boolean; owner?: string; idle?: boolean; model: string; pos: Vec3; yaw: number; collision?: string; track?: Vec3[]; submarine?: boolean; rotation?: Vec3; param?: number; behavior?: ObjectBehaviorKind; acts?: number[]; warpTo?: CourseId; warpEntry?: string }[];
  redCoins?: Vec3[];
  secretStar?: { act: number; pos: Vec3; origin?: Vec3; triggers: Vec3[] };
  mountainBallSpawners?: {pos: Vec3; path: number; period: number; acts: number[]}[];
  mountainBallPaths?: Vec3[][];
  waterBombSpawners?: {pos: Vec3; param:number}[];
  wingSwitch?: Vec3;
  cannons?: CannonPlacement[];
  snowmanPath?: Vec3[];
  snowmanStar?: PlaygroundResult['snowmanStar'];
  penguinRace?: import('../sim/object').PenguinRace;
  penguinStar?: PlaygroundResult['penguinStar'];
  chestStar?: { act: number; pos: Vec3 };
  redCoinStar?: Vec3;
  redCoinAct?: number;
  placedStars?: { pos: Vec3; act: number; acts: number[] }[];
  coins: Vec3[];
  coinFormations?: CoinFormation[];
  extraCollision: Record<string, CollisionData>;
}
const courseUrls = import.meta.glob<string>('../../assets/courses/*.json', { eager: true, query: '?url', import: 'default' });

/** Native terrain collision is kept separate from display geometry. */
export async function prepareCourse(id: Exclude<CourseId, 'playground'>, entry?: string | null, push = false, act = 1, requestedArea = 1): Promise<WorldFactory> {
  const area = id === 'castle_inside' && (requestedArea === 2 || requestedArea === 3) ? requestedArea
    : (id === 'ddd' || id === 'jrb' || id === 'ccm') && requestedArea === 2 ? 2 : 1;
  if (id !== 'ddd' && id !== 'ccm') return prepareArea(id, entry, push, act, area);
  const [current, other] = await Promise.all([
    prepareArea(id, entry, push, act, area), prepareArea(id, null, false, act, 3 - area),
  ]);
  return scene => {
    const world = current(scene);
    const otherScene = new THREE.Scene();
    const otherWorld = other(otherScene);
    world.linkedArea = { scene: otherScene, world: otherWorld };
    otherWorld.linkedArea = { scene, world };
    return world;
  };
}

async function prepareArea(id: Exclude<CourseId, 'playground'>, entry: string | null | undefined,
    push: boolean, act: number, area: number): Promise<WorldFactory> {
  const url = courseUrls[`../../assets/courses/${id}${area > 1 ? '.' + area : ''}.json`];
  if (!url) throw new Error('Course data missing. Run pnpm export:courses after copying local assets.');
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Course data request failed: ${response.status}`);
  const data: CourseData = await response.json();
  if (id === 'bob') data.objects.push({ model: 'levels/bob/bob_geo_000458.glb', pos: [-2303, 717, 1024], yaw: 45 });
  for (const warp of data.warps ?? []) data.objects.push({ ...warp, model: '', yaw: 0, behavior: 'warp' });
  const redAct = data.redCoinAct ?? 1;
  const redCoinStar = act === redAct ? data.redCoinStar : undefined;
  const placements = (data.placedStars ?? []).filter(star => !star.acts.length || star.acts.includes(act));
  const selectedStar = placements.find(star => star.act === act);
  const cannonSpots = (data.cannons ?? []).filter(c => !c.acts?.length || c.acts.includes(act));
  const activeObjects = data.objects.filter(object => !object.acts || object.acts.length === 0 || object.acts.includes(act));
  if (data.penguinRace && !activeObjects.some(o=>o.behavior==='racingPenguin')) delete data.penguinStar;
  if (data.snowmanStar && act!==data.snowmanStar.act) {
    const body = data.objects.find(o=>o.behavior==='snowmanBody');
    if (body) activeObjects.push({...body,param:-1,pos:[-4230,-1344,1813]});
  }
  const chainModels = activeObjects.some(o => o.behavior === 'chainChomp')
    ? ['actors/poundable_pole/wooden_post_geo.glb', 'actors/chain_ball/metallic_ball_geo.glb']
    : [];
  const rewardKey = id === 'bowser_1' ? 1 : id === 'bowser_2' ? 2 : undefined;
  const rewardModel = rewardKey ? 'actors/bowser_key/bowser_key_geo.glb' : SM64_MODELS.star;
  const ending = id === 'castle_grounds' && entry === 'WARP_NODE_CREDITS_START';
  const paths = [...new Set([data.model, ...data.roomModels ?? [], SM64_MODELS.coin, rewardModel, SM64_MODELS.transparentStar, ...activeObjects.map(o => o.model).filter(Boolean),
    ...(id === 'castle_inside' && area === 1 ? ['levels/castle_inside/castle_lobby_light.glb'] : []),
    ...(data.wingSwitch ? [SWITCH_BASE,SWITCH_TOP] : []),
    ...(activeObjects.some(o=>o.behavior==='wingBox') ? ['actors/exclamation_box_outline/exclamation_box_outline_geo.glb','actors/mario_cap/marios_wing_cap_geo.glb'] : []),
    ...(data.waterBombSpawners?.length ? ['actors/water_bubble/water_bomb_geo.glb','actors/water_bubble/water_bomb_shadow_geo.glb'] : []),
    ...(activeObjects.some(o => o.behavior === 'waterBombCannon') ? [CANNON_BARREL,'actors/water_bubble/water_bomb_geo.glb'] : []),
    ...(activeObjects.some(o => o.behavior === 'bobomb' || o.behavior === 'breakableBoxSmall') ? ['actors/coin/yellow_coin_geo.glb'] : []),
    ...(activeObjects.some(o => o.behavior === 'koopa') ? ['actors/koopa_shell/koopa_shell_geo.glb','actors/koopa/koopa_without_shell_geo.glb','actors/coin/blue_coin_geo.glb'] : []),
    ...(activeObjects.some(o => o.behavior === 'bowser') ? ['actors/bowser_flame/bowser_flames_geo.glb'] : []),
    ...coinEffectPaths(data.redCoins?.length ? 7 : data.secretStar ? 4 : 0),
    ...(data.redCoins?.length ? [SM64_MODELS.redCoin] : []),
    ...(activeObjects.some(o => o.behavior === 'chest') ? ['actors/treasure_chest/treasure_chest_lid_geo.glb'] : []),
    ...(cannonSpots.length ? [CANNON_BASE,CANNON_BARREL] : []), ...(ending ? [PEACH, TOAD] : []), ...chainModels, ...(id === 'bob' || data.goombas?.length ? [SM64_MODELS.goomba] : [])])];
  const models = new Map(await Promise.all(paths.map(async path => [path, await loadSm64Asset(path)] as const)));
  const make = (path: string) => instantiateSm64(models.get(path)!);
  const root = new THREE.Group();
  const coinEffects = new CoinEffects(root,make);
  root.name = id;
  const rooms = (data.roomModels ?? [data.model]).map(path => make(path));
  for (const [index, model] of rooms.entries()) {
    model.visible = index === 0;
    root.add(model);
  }
  if (data.explorationOnly) {
    for (const room of rooms) room.traverse(node => {
      if (!(node instanceof THREE.Mesh)) return;
      for (const material of Array.isArray(node.material) ? node.material : [node.material]) {
        if (material.map) material.map.anisotropy = 8;
        if (material.userData.ootDecal) {
          material.polygonOffset = true;
          material.polygonOffsetFactor = -2;
          material.polygonOffsetUnits = -2;
          material.depthWrite = false;
          node.renderOrder = 1;
        }
      }
    });
  }
  const disposeBackground = data.kokiri?.fixed ? await prepareKokiriBackground(id,data.kokiri.fixed,rooms[0]) : undefined;
  const paintings = data.paintingArt ? await buildCastlePaintings(data.paintingArt, root) : undefined;
  const collision = new CollisionWorld(undefined, data.horizontalBoundary);
  collision.slideTerrain = !!data.slideTerrain;
  collision.snowTerrain = data.snowTerrain ?? false;
  collision.waterBoxes = data.waterBoxes ?? [];
  const shipAngles = ([pitch, yaw, roll]: Vec3) => new THREE.Euler(pitch * Math.PI / 32768, yaw * Math.PI / 32768, roll * Math.PI / 32768, 'YXZ');
  addNativeCollision(collision, data);
  for (const part of data.shipCollision ?? []) {
    if (!part.acts.includes(act)) continue;
    const source = data.extraCollision[part.name];
    const rotation = shipAngles(part.rotation);
    const vertices = source.vertices.map(v => new THREE.Vector3(...v).applyEuler(rotation).toArray());
    addNativeCollision(collision, { ...source, vertices }, part.pos);
  }
  let currentRoom = -1;
  const selectRoom = (pos: Vec3) => {
    if (!data.roomModels) return;
    const floorRoom = collision.findFloor(...pos, true).surface?.room;
    const selected = floorRoom ? floorRoom - 1 : Math.max(0, currentRoom);
    if (selected === currentRoom || !rooms[selected]) return;
    if (currentRoom >= 0) rooms[currentRoom].visible = false;
    else rooms[0].visible = false;
    rooms[selected].visible = true;
    currentRoom = selected;
    if (data.roomPaintings) paintings?.showRoom(data.roomPaintings[selected]);
  };
  const extras: Record<string, string> = {
    'levels/bob/bob_geo_000458.glb': 'seesaw_platform/collision.inc.c',
    'levels/wf/wf_geo_0008A8.glb': 'areas/1/10/collision.inc.c',
    'levels/wf/wf_geo_0008E8.glb': 'areas/1/11/collision.inc.c',
  };
  const roomDoors = data.doorMotion ? new RoomDoors(data.doorMotion) : undefined;
  const saveGeometry: ((save: SaveFile) => void)[] = [];
  if (id === 'castle_inside' && area === 1) {
    const light = make('levels/castle_inside/castle_lobby_light.glb');
    root.add(light);
    saveGeometry.push(save => { light.visible = save.stars >= 10 && !save.wingSwitchOn; });
  }
  if (paintings) saveGeometry.push(paintings.reset);
  if (data.extraCollision.ddd_warp) {
    const front = new DoorCollider(collision, data.extraCollision.ddd_warp, 0);
    const back = new DoorCollider(collision, data.extraCollision.ddd_warp_2, 0);
    saveGeometry.push(save => {
      const moved = save.hasStar('ddd', 1);
      front.move(moved ? null : [0, 0, 0]);
      back.move(moved ? [0, 0, 0] : null);
    });
  }
  const questParts = new Map<string,THREE.Group>();
  const speakers = new Map<string, (clip: string) => void>();
  const rupeeModels = new Map<string,THREE.Group>();
  const scrollingTextures = new Map<THREE.Texture,number>();
  const chestActions = new Map<string,THREE.AnimationAction>();
  const recoveryHearts: THREE.Group[] = [];
  const spinners: {model: THREE.Group; rate:number}[] = [];
  const mixers: THREE.AnimationMixer[] = [];
  const fairies: {model: THREE.Group; home: Vec3; owner?: string}[] = [];
  const signs: import('./signpost').Signpost[] = [];
  const platforms: MovingPlatform[] = [];
  for (const object of activeObjects) {
    // Interactive objects are instantiated by CourseObjects below.
    if (object.behavior) continue;
    if (object.pages) {
      const [x,y,z] = object.pos;
      object.pos[1] = collision.findFloor(x,y+200,z).height;
      signs.push({pos:object.pos, yaw:Math.round(object.yaw*32768/180), pages:object.pages});
    }
    if (object.treasureKey) {
      // Some chests originally dropped into place after a cutscene. Use their landed pose.
      const chest = data.kokiri?.treasureChests?.find(c=>c.key===object.treasureKey);
      const [x,y,z]=object.pos;
      const ground=collision.findFloor(x,y+100,z);
      if (chest && ground.surface) chest.pos[1]=object.pos[1]=ground.height;
    }
    if (object.groundedProp) {
      const [x,y,z] = object.pos;
      const hit = collision.findFloor(x,y+150,z);
      if (hit.surface) object.pos[1] = hit.height;
    }
    const model = make(object.model);
    model.position.fromArray(object.pos);
    if (data.kokiri?.quest && ['sword_chest','kokiri_sword','mido','deku_tree_mouth'].some(id=>object.model===`levels/kokiri_forest/${id}.glb`)) {
      questParts.set(object.model.split('/').pop()!.replace('.glb',''),model);
    }
    model.rotation.y = object.yaw * Math.PI / 180;
    if (object.billboard) model.userData.objectBillboard = true;
    if (object.scrollT) {
      const speed = object.scrollT;
      model.traverse(node => {
        if (!(node instanceof THREE.Mesh)) return;
        for (const material of Array.isArray(node.material) ? node.material : [node.material]) {
          if (material.map) scrollingTextures.set(material.map,speed);
        }
      });
    }
    if (object.treasureKey) {
      const mixer=new THREE.AnimationMixer(model);
      const clip=models.get(object.model)!.animations.find(a=>a.name==='Open')!;
      const lid=mixer.clipAction(clip).setLoop(THREE.LoopOnce,1);
      lid.clampWhenFinished=true;
      mixers.push(mixer);
      chestActions.set(object.treasureKey,lid);
      const key=object.treasureKey;
      saveGeometry.push(save=>{
        lid.reset().play();
        lid.paused=true;
        lid.time=save.chestOpened(key) ? clip.duration : 0;
        mixer.update(0);
      });
    }
    if (object.recoveryHeart) recoveryHearts.push(model);
    if (object.spinZ) spinners.push({model,rate:object.spinZ});
    if (object.actorScale) model.scale.multiplyScalar(object.actorScale);
    if (object.speaker) {
      const spot = data.kokiri?.talks.find(t=>t.id===object.speaker);
      if (spot) {
        // Use the posed mesh, including any offset from the actor origin (e.g. King Zora).
        model.updateMatrixWorld(true);
        const bounds = new THREE.Box3().setFromObject(model,true);
        const size = bounds.getSize(new THREE.Vector3());
        const center = bounds.getCenter(new THREE.Vector3());
        center.y += size.y*.12;
        spot.portrait = {center:center.toArray(),distance:Math.max(280,size.y*1.4,size.x*1.25,size.z*1.25),yaw:model.rotation.y+.33};
        spot.range = Math.max(spot.range ?? 240, Math.max(size.x,size.z)*.55+120);
      }
    }
    if (object.rupee) rupeeModels.set(object.rupee,model);
    if (object.fairy) fairies.push({model,home:object.pos,owner:object.owner});
    if (object.idle) {
      const mixer = new THREE.AnimationMixer(model);
      const clips = models.get(object.model)!.animations;
      let active = mixer.clipAction(clips[0]).play();
      if (clips.length > 1) {
        const id = object.model.split('/').pop()!.replace('.glb','');
        speakers.set(id, name => {
          const next = mixer.clipAction(clips.find(c=>c.name===name) ?? clips[0]);
          if (next === active) return;
          next.reset().play().crossFadeFrom(active,0.12,false);
          active = next;
        });
      }
      mixers.push(mixer);
    }
    if (object.rotation) model.rotation.copy(shipAngles(object.rotation));
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
    const matchesDoor = (d: { pos: Vec3 }) => d.pos.every((v, i) => v === object.pos[i]);
    const entrance = data.doors?.find(matchesDoor);
    const roomDoor = data.roomDoors?.find(matchesDoor);
    if (entrance || roomDoor) {
      const collider = new DoorCollider(collision, data.extraCollision[roomDoor?.sliding ? 'star_door' : 'door'], object.yaw);
      const mixer = new THREE.AnimationMixer(model.children[0]);
      mixers.push(mixer);
      const play = (index: number) => {
        const clip = models.get(object.model)?.animations[index];
        if (!clip) return;
        mixer.stopAllAction();
        const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopOnce, 1);
        action.clampWhenFinished = true;
        action.play();
      };
      if (entrance) {
        collider.move(entrance.pos);
        entrance.play = play;
        play(0);
      }
      if (roomDoor) roomDoors?.add({ door: roomDoor, model, collision: collider, play });
    }
    const extra = data.extraCollision[object.collision ?? extras[object.model]];
    const kind = (object.collision ?? extras[object.model] ?? '').split('/')[0];
    if (object.submarine && extra) {
      const collider = new DoorCollider(collision, extra, object.yaw);
      saveGeometry.push(save => {
        model.visible = !(save.hasKey(2) || save.doorUnlocked(2));
        collider.move(model.visible ? object.pos : null);
      });
    } else if (extra && ['checkerboard_lift', 'ropeway_lift', 'tower_solid', 'tower_slide', 'tower_elevator','seesaw_platform', 'rotating_platform', 'rotating_wooden_platform', 'sliding_platform', 'small_bomp', 'large_bomp', 'mesh_elevator', 'merry_go_round', 'moving_bookshelf', 'tilting_trap_platform'].includes(kind)) {
      const platform = new MovingPlatform(model, extra, kind, object.param ?? (kind === 'rotating_platform' ? 8 : kind === 'sliding_platform' ? 2 : 0), object.track);
      platforms.push(platform);
      root.add(platform.blocker);
    } else if (extra) addNativeCollision(collision, extra, object.pos, object.yaw);
  }
  let forestQuest: KokiriSwordQuest | undefined;
  if (data.kokiri?.quest) {
    const chest=questParts.get('sword_chest')!;
    const mixer=new THREE.AnimationMixer(chest);
    mixers.push(mixer);
    forestQuest=new KokiriSwordQuest(data.kokiri.quest,collision,chest,questParts.get('kokiri_sword')!,questParts.get('mido')!,
      questParts.get('deku_tree_mouth')!, data.extraCollision.tree_mouth,
      data.kokiri.talks.find(t=>t.id==='mido')!.pos,mixer,models.get('levels/kokiri_forest/sword_chest.glb')!.animations.find(a=>a.name==='Open')!);
    const quest=forestQuest;
    saveGeometry.push(save=>quest.restore(save));
  }
  const villageRupees = data.kokiri?.rupees?.length ? new VillageRupees(data.kokiri.rupees,rupeeModels) : undefined;
  if (villageRupees) saveGeometry.push(save => villageRupees.restore(save));
  const updateSaveGeometry = (save: SaveFile) => { for (const update of saveGeometry) update(save); };
  if (saveGeometry.length) updateSaveGeometry(new SaveFile(activeSaveSlot()));
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
  if (id === 'jrb' && area === 2 && !entry) data.spawn = { pos: [928, 1050, -1248], faceYaw: -32768, action: ACT_WATER_IDLE };
  if (id === 'ddd' && area === 2) {
    // Standalone area entry uses the dock. Instant warps carry the current Mario state.
    data.spawn = { pos: [3900, 1000, -600], faceYaw: 0 };
  }
  if (entry && data.kokiri?.arrivals[entry]) data.spawn = data.kokiri.arrivals[entry];
  const [sx, sy, sz] = data.spawn.pos;
  if (!entry && id === 'ccm' && area === 2) entry = 'WARP_NODE_0A';
  if (!entry && id === 'totwc') entry = 'WARP_NODE_0A';
  const warp = entry && data.warpSpawns && Object.hasOwn(data.warpSpawns, entry) ? data.warpSpawns[entry] : undefined;
  if (ending) {
    data.spawn = { pos: [0, 8000, 0], faceYaw: -32768, action: ACT_END_PEACH_CUTSCENE };
  } else if (warp) data.spawn = { pos: warp.pos, faceYaw: warp.faceYaw, action: warpActions[warp.behavior] };
  else if (!data.spawn.action) data.spawn.pos[1] = floorAt(sx, sy + 200, sz);
  const coinSpots = [...data.coins];
  for (const formation of data.coinFormations ?? []) coinSpots.push(...formationCoins(formation, collision));
  const redCoinSpots = data.redCoins ?? [];
  const makeCoin = (pos: Vec3, model: string): Coin => {
    const group = make(model);
    group.userData.objectBillboard = true;
    const position = new THREE.Vector3(...pos);
    group.position.copy(position);
    root.add(group);
    return { group, position, collected: false, nativeHitbox: true };
  };
  const coins = coinSpots.map(pos => makeCoin(pos, SM64_MODELS.coin));
  // Exploration goals; these do not claim to reproduce the original star missions.
  const starLocations: Record<string, Vec3> = { bob: [1636, 20000, -5567], castle_grounds: [0, 1000, -1300], wf: [180, 3880, 340], bbh: [102, 20000, 218], ccm: [512, 20000, -511], lll: [0, 20000, -4385], ssl: [-8191, 20000, 1536], wdw: [-255, 20000, -4351], sl: [-5688, 20000, -8191], ttm: [0, 20000, 0], thi: [-1023, 20000, -2555], ttc: [1457, 20000, -602], rr: [-2880, 20000, -6570], bitdw: [6611, 20000, -518], bitfs: [1536, 20000, -262], bits: [214, 20000, -5698], bowser_1: [0, 20000, 0], bowser_2: [0, 20000, 0], bowser_3: [0, 20000, 0], pss: [-6385, -4000, 5770] };
  const missionStar = id === 'bob'
    ? act === 2 ? KOOPA_RACE_STAR : starLocations[id]
    : starLocations[id];
  const [x, y, z] = redCoinStar ?? selectedStar?.pos ?? missionStar ?? data.spawn.pos;
  const starY = (id === 'bob' && act === 2) || id === 'ccm' || redCoinStar || selectedStar || id === 'wf' || id === 'castle_inside' || id === 'ddd' || id === 'jrb' ? y : floorAt(x, y, z) + 100;
  const redCoins = redCoinSpots.map(pos => makeCoin(pos, SM64_MODELS.redCoin));
  const makeReward = (model: string) => {
    const group = new THREE.Group();
    const gold = make(model);
    const collected = make(SM64_MODELS.transparentStar);
    collected.visible = false;
    group.add(gold, collected);
    return { group, gold, setCollectedModel(saved: boolean) {
      gold.visible = !saved;
      collected.visible = saved;
    } };
  };
  const hundredCoinStar = SCORE_COURSES.has(id) ? makeReward(SM64_MODELS.star) : undefined;
  if (hundredCoinStar) {
    hundredCoinStar.group.visible = false;
    root.add(hundredCoinStar.group);
  }
  const primary = makeReward(rewardModel);
  const star = primary.group;
  if (rewardKey) {
    primary.gold.scale.multiplyScalar(0.5);
    primary.gold.rotation.z = -Math.PI / 2;
    primary.gold.position.y = 165;
  } else if (id === 'bowser_3') {
    primary.gold.scale.multiplyScalar(2);
    primary.gold.position.y = 110;
  }
  const sidePlacements = placements.filter(star => star !== selectedStar).map(star => ({ ...star, redCoins: false }));
  if (data.redCoinStar && !redCoinStar) {
    sidePlacements.push({ pos: data.redCoinStar, acts: [], act: redAct, redCoins: true });
  }
  if (data.snowmanStar && act===5) sidePlacements.push({...data.snowmanStar,acts:[],redCoins:false});
  if (data.penguinStar) sidePlacements.push({...data.penguinStar,acts:[],redCoins:false});
  if (data.chestStar) sidePlacements.push({ ...data.chestStar, acts: [], redCoins: false });
  if (data.secretStar) sidePlacements.push({ act: data.secretStar.act, pos: data.secretStar.pos, acts: [], redCoins: false });
  const boxes = activeObjects.filter(object => object.behavior === 'starBox');
  for (const box of boxes) sidePlacements.push({ act: box.param!, pos: box.pos, acts: [], redCoins: false });
  const sideStars = sidePlacements.map(placement => {
    const reward = makeReward(SM64_MODELS.star);
    reward.group.position.fromArray(placement.pos);
    root.add(reward.group);
    return { act: placement.act, penguin: placement.act === data.penguinStar?.act, snowman:placement.act === data.snowmanStar?.act, secret: placement.act === data.secretStar?.act, box: boxes.some(box => box.param === placement.act && box.pos === placement.pos), chest: data.chestStar?.act === placement.act, redCoins: placement.redCoins, group: reward.group, setCollectedModel: reward.setCollectedModel };
  });
  const redReward = data.redCoinStar ? {
    pos: data.redCoinStar, group: sideStars.find(reward => reward.redCoins)?.group ?? star,
    marker: undefined as THREE.Group | undefined, yaw: 0,
  } : undefined;
  if (redReward && !['jrb','bitdw','bitfs','bits'].includes(id)) {
    const [mx,my,mz] = redReward.pos;
    const marker = make(SM64_MODELS.transparentStar);
    marker.position.set(mx,collision.findFloor(mx,my+200,mz).height+60,mz);
    marker.scale.multiply(new THREE.Vector3(1.5,1.5,0.75));
    marker.rotation.set(Math.PI/2,0,0,'YXZ');
    root.add(marker);
    redReward.marker = marker;
  }
  star.position.set(x, starY, z);
  star.visible = !data.explorationOnly && id !== 'castle_inside' && id !== 'castle_grounds' && (!['ddd', 'jrb', 'bob','ccm'].includes(id) || id === 'bob' && act <= 2 || !!selectedStar || !!redCoinStar);
  root.add(star);
  const starLight = new THREE.PointLight(0xffd969, 200, 450, 2);
  starLight.position.copy(star.position);
  starLight.visible = star.visible;
  root.add(starLight);
  const shadows = new ActorShadows(root,collision);
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
      shadows.register(goomba);
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
    snowmanPath:data.snowmanPath, penguinRace:data.penguinRace,
    blastWalls: [data.extraCollision['breakable_wall_left/collision.inc.c'],data.extraCollision['breakable_wall_right/collision.inc.c']],
    objects: [...activeObjects,...(data.mountainBallSpawners ?? []).filter(s => s.acts.includes(act)).map(s => ({
      pos: s.pos, model: '', yaw: 0, behavior: 'mountainBallSpawner' as const,
      ballRoute: {waypoints: data.mountainBallPaths![s.path], period: s.period, maxDistance: s.path === 0 ? 7000 : 6000},
    })),...(data.waterBombSpawners ?? []).map(spec => ({
      ...spec,model:'',yaw:0,behavior:'waterBombSpawner' as const,
    }))], shadows,
    coins,
    starBoxCollision: data.extraCollision.starBox, chainGateCollision: data.extraCollision.chainGate,
    woodenPostCollision: data.extraCollision.woodenPost,
    makeCoin: value => make(value === 5 ? 'actors/coin/blue_coin_geo.glb' : SM64_MODELS.coin),
    billboards,
  });
  if (data.snowmanStar && act!==data.snowmanStar.act) {
    const star = data.snowmanStar.act;
    const applySnowman = (save: SaveFile) => courseObjects.setSnowmanCompleted(save.hasStar(id,star));
    saveGeometry.push(applySnowman);
    applySnowman(new SaveFile(activeSaveSlot()));
  }
  const wingSwitch = data.wingSwitch ? new WingSwitch(root,collision,data.wingSwitch,data.extraCollision,make) : undefined;
  if (wingSwitch || activeObjects.some(o=>o.behavior==='wingBox')) {
    const applySwitch = (save: SaveFile) => {
      courseObjects.setWingBoxes(save.wingSwitchOn);
      wingSwitch?.reset(save.wingSwitchOn);
    };
    saveGeometry.push(applySwitch);
    applySwitch(new SaveFile(activeSaveSlot()));
  }
  const cannons = cannonSpots.length ? new Cannons(root,collision,data.extraCollision.cannonLid,cannonSpots,make) : undefined;
  if (cannons) {
    saveGeometry.push(save => cannons.reset(save.cannonOpen(id)));
    cannons.reset(new SaveFile(activeSaveSlot()).cannonOpen(id));
  }
  const waterHeight = collision.waterBoxes[0]?.[5];
  let lastTime = 0;
  const world: PlaygroundResult = {
    dispose() {
      disposeBackground?.();
      cannons?.dispose();
      paintings?.dispose();
      courseObjects.dispose();
      shadows.dispose();
      for (const mixer of mixers) mixer.stopAllAction();
      geometry.dispose();
      helper.material.dispose();
    },
    reset() {
      coinEffects.clear();
      for (const heart of recoveryHearts) heart.visible = true;
      courseObjects.reset();
      roomDoors?.reset();
      if (data.drainWater && waterHeight !== undefined) collision.waterBoxes[0][5] = waterHeight;
    },
    openTreasureLid: key=>{ chestActions.get(key)?.reset().setEffectiveTimeScale(1.5).play(); },
    collectRecoveryHeart: recoveryHearts.length ? (pos) => {
      const heart = recoveryHearts.find(h=>h.visible && Math.abs(h.position.y-pos[1])<135 &&
        Math.hypot(h.position.x-pos[0],h.position.z-pos[2])<95);
      if (heart) heart.visible = false;
      return !!heart;
    } : undefined,
    nativeWaterBoxes: id==='lake_hylia' ? [2,3] : undefined,
    signs, kokiri:data.kokiri, villageRupees, forestQuest,
    poseSpeaker: speakers.size ? (id,clip) => {
      for (const [actor,pose] of speakers) pose(actor==='mido' && forestQuest?.isWalking ? 'Walk' : actor===id ? clip : 'Idle');
    } : undefined, wingSwitch, cannons, collision, platforms, collisionMeshes: [helper, ...platforms.map(p => p.blocker)], coins, redCoins, star, starLight, spawn: data.spawn, deathWarp:data.deathWarp, fallWarp:data.fallWarp, successEntry: data.successEntry,
    updateSaveGeometry: saveGeometry.length ? updateSaveGeometry : undefined,
    selectRoom: data.roomModels ? selectRoom : undefined,
    stepEnding: ending ? castleEnding(models, root) : undefined,
    stepPaintings: paintings?.step,
    entrySpawns: Object.fromEntries(Object.entries(data.warpSpawns ?? {}).map(([node,s]) =>
      [node,{pos:s.pos,faceYaw:s.faceYaw,action:warpActions[s.behavior]}])),
    area, hasCourseStar: star.visible, roomDoors, fadingWarps:data.fadingWarps, instantWarps: data.instantWarps, paintings: data.paintings, rewardKey, successArea: data.successArea,
    setCollectedStarModel: saved => primary.setCollectedModel(saved && !rewardKey && id !== 'bowser_3'),
    coinEffects, redReward, snowmanStar:data.snowmanStar, penguinStar:data.penguinStar, secretStar: data.secretStar, floorWarp: data.floorWarp, drainWater: data.drainWater, sideStars, hundredCoinStar, primaryStarAct: redCoinStar ? data.redCoinAct : undefined,
    deathHeight: Math.min(...data.vertices.map(v => v[1])) - 500,
    goombas, doors: data.doors, objects: courseObjects,
    noExitStar: id === 'bitdw' || id === 'bitfs' || id === 'bits',
    requireCoins: false,
    starRequiresEvent: !!redCoinStar || !selectedStar && (id === 'bob' || id === 'bbh' || id === 'wf' || id === 'lll' || id === 'jrb' || ['bitdw', 'bitfs', 'bits', 'bowser_1', 'bowser_2', 'bowser_3'].includes(id)),
    requiredCoins: coinSpots.length,
    update(time) {
      villageRupees?.update(time);
      for (const [texture,speed] of scrollingTextures) texture.offset.y = (time*speed)%1;
      for (const heart of recoveryHearts) heart.rotation.y = time*2;
      for (const {model,rate} of spinners) model.rotation.z = time*rate;
      for (const {model,home,owner} of fairies) {
        if (owner) questParts.get(owner)?.position.toArray(home);
        model.position.set(home[0]+Math.sin(time*1.9)*67,home[1]+204+Math.sin(time*3.4)*19,home[2]+Math.cos(time*1.9)*58);
        model.rotation.y = time*1.9;
      }
      for (const mixer of mixers) mixer.update(Math.min(0.1, time - lastTime));
      courseObjects.update(time);
      shadows.update();
      lastTime = time;
      // bhv_collect_star_loop advances yaw by 0x800 every simulation tick.
      star.rotation.y = Math.floor(time * 30) * Math.PI / (rewardKey || id === 'bowser_3' ? 32 : 16);
      for (const reward of sideStars) reward.group.rotation.y = star.rotation.y;
      if (redReward) redReward.group.rotation.y = redReward.yaw;
      starLight.position.copy(star.position);
    },
    faceCamera(camera) {
      coinEffects.faceCamera(camera);
      billboards.update(camera);
      for (const coin of coins) coin.group.quaternion.copy(camera.quaternion);
    },
  };
  return scene => { scene.add(root); return world; };
}
export const prepareBattlefield = () => prepareCourse('bob');
