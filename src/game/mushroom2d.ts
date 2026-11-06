import * as THREE from 'three';
import { CollisionWorld } from '../sim/collision';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';
import { GoombaEnemy } from './goomba';
import type { Group } from 'three';
import type { WorldFactory } from './sm64World';
import type { Coin, PlaygroundResult } from './world';
import grassUrl from '../../assets/extracted/textures/generic/bob_textures.05800.rgba16.png?url';
import stoneUrl from '../../assets/extracted/textures/generic/bob_textures.05000.rgba16.png?url';
import woodUrl from '../../assets/extracted/textures/generic/bob_textures.06000.rgba16.png?url';

const GROUND = 0;

class SideGoomba extends GoombaEnemy {
  private dir = 1;
  private homeX: number;
  constructor(model: Group) {
    super(model);
    this.homeX = model.position.x;
  }
  protected resetMovement(): void {
    this.model.position.x = this.homeX;
    this.model.position.y = GROUND;
    this.model.position.z = 0;
    this.model.visible = true;
    this.dir = 1;
  }
  alert(x: number, _z: number): void { this.dir = x < this.model.position.x ? -1 : 1; }
  protected move(mario: [number, number, number], world: CollisionWorld): void {
    const x = this.model.position.x;
    if (Math.abs(mario[2] - 0) > 140) return;
    if (Math.abs(mario[0] - x) < 420) this.dir = mario[0] < x ? -1 : 1;
    const ahead = world.findFloor(x + this.dir * 50, 2000, 0);
    const here = world.findFloor(x, 2000, 0);
    if (!ahead.surface || (here.surface && ahead.height < here.height - 20) || Math.abs(x) > 2500) {
      this.dir *= -1;
    } else {
      this.model.position.x += this.dir * 2.4;
    }
    const floor = world.findFloor(this.model.position.x, 2000, 0);
    if (floor.surface) this.model.position.y = floor.height;
    this.model.position.z = 0;
    this.model.rotation.y = this.dir > 0 ? Math.PI / 2 : -Math.PI / 2;
  }
}


const LANE_DEPTH = 360;

/**
 * A hand-authored side-scrolling level using real SM64 assets: segmented
 * ground, brick platforms, pipes, coins, Goombas, a staircase and a finish star.
 */
export async function prepareMushroom2D(): Promise<WorldFactory> {
  const paths = {
    coin: SM64_MODELS.coin,
    star: SM64_MODELS.star,
    goomba: SM64_MODELS.goomba,
    tree: SM64_MODELS.tree,
    pipe: 'actors/warp_pipe/warp_pipe_geo.glb',
    post: 'actors/poundable_pole/wooden_post_geo.glb',
  } as const;
  const models = new Map(await Promise.all(Object.entries(paths).map(async ([key, path]) =>
    [key, await loadSm64Asset(path)] as const)));
  const loader = new THREE.TextureLoader();
  const textures = await Promise.all([grassUrl, stoneUrl, woodUrl].map(url => loader.loadAsync(url)));

  return scene => {
    const root = new THREE.Group();
    root.name = 'mushroom-2d';
    scene.add(root);
    const collision = new CollisionWorld();
    const collisionMeshes: THREE.Mesh[] = [];
    const maps = textures.map((source, i) => {
      const map = source.clone();
      map.colorSpace = THREE.SRGBColorSpace;
      map.wrapS = map.wrapT = THREE.RepeatWrapping;
      map.magFilter = THREE.NearestFilter;
      map.minFilter = THREE.NearestMipmapLinearFilter;
      map.repeat.setScalar(i === 0 ? 4 : 2);
      map.needsUpdate = true;
      return map;
    });
    const mats = [
      new THREE.MeshStandardMaterial({ map: maps[0], roughness: 1 }),
      new THREE.MeshStandardMaterial({ map: maps[1], roughness: 0.9 }),
      new THREE.MeshStandardMaterial({ map: maps[2], roughness: 0.92 }),
    ];

    const addBox = (x: number, top: number, z: number, w: number, h: number, d: number, material: THREE.Material) => {
      const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), material);
      mesh.position.set(x, top - h / 2, z);
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      mesh.userData.nativeCollision = true;
      root.add(mesh);
      collisionMeshes.push(mesh);
      const hw = w / 2;
      const hd = d / 2;
      const bottom = top - h;
      collision.addQuad(
        [x - hw, top, z - hd], [x - hw, top, z + hd], [x + hw, top, z + hd], [x + hw, top, z - hd],
        { type: 0 },
      );
      collision.addQuad(
        [x - hw, bottom, z - hd], [x - hw, bottom, z + hd], [x - hw, top, z + hd], [x - hw, top, z - hd],
        { type: 0 },
      );
      collision.addQuad(
        [x + hw, bottom, z + hd], [x + hw, bottom, z - hd], [x + hw, top, z - hd], [x + hw, top, z + hd],
        { type: 0 },
      );
    };

    // Segmented ground with two jumpable gaps.
    const segments: Array<[number, number]> = [[-2300, -520], [-360, 430], [610, 1180], [1320, 2640]];
    for (const [from, to] of segments) {
      addBox((from + to) / 2, GROUND, 0, to - from, 280, LANE_DEPTH, mats[0]);
    }

    // Brick/question rows and floating platforms.
    for (const x of [-1700, -1600, -1500, -1400]) addBox(x, 300, 0, 90, 90, 90, mats[1]);
    for (const x of [-100, 0, 100]) addBox(x, 300, 0, 90, 90, 90, mats[1]);
    addBox(780, 390, 0, 420, 80, 160, mats[2]);
    addBox(1100, 520, 0, 300, 80, 160, mats[2]);
    addBox(1450, 300, 0, 120, 120, 120, mats[1]);

    // Classic staircase to the finish.
    for (let i = 0; i < 6; i++) addBox(1750 + i * 100, 100 + i * 90, 0, 100, 100 + i * 90, 170, mats[2]);

    // Real warp-pipe visuals and collision columns.
    const pipeTemplate = models.get('pipe')!;
    for (const x of [-700, 430, 1550]) {
      const pipe = instantiateSm64(pipeTemplate);
      const box = new THREE.Box3().setFromObject(pipe);
      const height = box.getSize(new THREE.Vector3()).y || 1;
      pipe.scale.multiplyScalar(300 / height);
      const fitted = new THREE.Box3().setFromObject(pipe);
      pipe.position.set(x - (fitted.min.x + fitted.max.x) / 2, GROUND - fitted.min.y, -(fitted.min.z + fitted.max.z) / 2);
      root.add(pipe);
      addBox(x, 300, 0, 160, 300, 160, mats[2]);
    }

    // Real trees in the background.
    const treeTemplate = models.get('tree')!;
    for (const [x, z] of [[-1600, -320], [-1250, -320], [-850, -320], [-350, -320], [300, -320], [900, -320], [1500, -320], [2050, -320], [2300, 260]]) {
      const tree = instantiateSm64(treeTemplate);
      const size = new THREE.Box3().setFromObject(tree).getSize(new THREE.Vector3());
      tree.scale.multiplyScalar(260 / Math.max(1, size.y));
      tree.position.set(x, GROUND, z);
      root.add(tree);
    }

    // Flag pole finish marker.
    const post = instantiateSm64(models.get('post')!);
    const postSize = new THREE.Box3().setFromObject(post).getSize(new THREE.Vector3());
    post.scale.multiplyScalar(620 / Math.max(1, postSize.y));
    post.position.set(2380, GROUND, 0);
    root.add(post);

    const coinSpots: Array<[number, number]> = [
      [-1700, 380], [-1600, 380], [-1500, 380], [-1400, 380], [-1800, 120], [-650, 120], [-250, 360], [80, 360], [430, 140],
      [780, 450], [1120, 580], [1550, 360], [1900, 700],
    ];
    const coins: Coin[] = coinSpots.map(([x, y]) => {
      const group = instantiateSm64(models.get('coin')!);
      group.scale.multiplyScalar(0.72);
      group.userData.objectBillboard = true;
      group.position.set(x, y, 0);
      root.add(group);
      return { group, position: new THREE.Vector3(x, y, 0), collected: false };
    });

    const goombas: GoombaEnemy[] = [];
    for (const x of [-1150, -700, 180, 980, 1500]) {
      const model = instantiateSm64(models.get('goomba')!);
      model.position.set(x, GROUND, 0);
      root.add(model);
      goombas.push(new SideGoomba(model));
    }

    const star = instantiateSm64(models.get('star')!);
    star.scale.multiplyScalar(0.55);
    star.position.set(2380, 760, 0);
    star.userData.objectBillboard = true;
    root.add(star);
    const starLight = new THREE.PointLight(0xffe58a, 2, 900, 2);
    starLight.position.set(2380, 840, 0);
    root.add(starLight);
    const billboards = new Sm64Billboards(scene);

    collision.finalize();
    return {
      collision,
      collisionMeshes,
      spawn: { pos: [-1900, 90, 0], faceYaw: 0x4000 },
      coins,
      goombas,
      star,
      starLight,
      requireCoins: true,
      requiredCoins: 8,
      deathHeight: -900,
      update(time: number) {
        for (const coin of coins) {
          if (coin.collected) continue;
          coin.group.rotation.y = time * 2.4 + coin.position.x * 0.01;
          coin.group.position.y = coin.position.y + Math.sin(time * 2.2 + coin.position.x * 0.01) * 3;
        }
        star.rotation.y = time * 1.4;
        star.position.y = 760 + Math.sin(time * 1.8) * 6;
      },
      faceCamera(camera: THREE.Camera) {
        billboards.update(camera);
      },
      dispose() {
        root.traverse(object => {
          if (object instanceof THREE.Mesh) {
            object.geometry.dispose();
            for (const material of Array.isArray(object.material) ? object.material : [object.material]) material.dispose();
          }
        });
        maps.forEach(map => map.dispose());
        textures.forEach(texture => texture.dispose());
        mats.forEach(material => material.dispose());
      },
    } satisfies PlaygroundResult;
  };
}
