import * as THREE from 'three';
import { CollisionWorld } from '../sim/collision';
import { addMeshCollision, type Coin, type PlaygroundResult } from './world';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';

export type WorldFactory = (scene: THREE.Scene) => PlaygroundResult;

/** Load everything before replacing the world, so failed loads leave no partial scene. */
export async function prepareBattlefield(): Promise<WorldFactory> {
  const [terrainAsset, coinAsset, starAsset, treeAsset, goombaAsset, seesawAsset] = await Promise.all([
    loadSm64Asset(SM64_MODELS.battlefield), loadSm64Asset(SM64_MODELS.coin),
    loadSm64Asset(SM64_MODELS.star), loadSm64Asset(SM64_MODELS.tree),
    loadSm64Asset(SM64_MODELS.goomba), loadSm64Asset('levels/bob/bob_geo_000458.glb'),
  ]);
  const build: WorldFactory = scene => {
    const collision = new CollisionWorld();
    const collisionMeshes: THREE.Mesh[] = [];
    const root = new THREE.Group();
    root.name = 'bob-omb-battlefield';

    const terrain = instantiateSm64(terrainAsset);
    root.add(terrain);
    const seesaw = instantiateSm64(seesawAsset);
    // Static placement from levels/bob/script.c. Its tilting behavior is not ported.
    seesaw.position.set(-2303, 717, 1024);
    seesaw.rotation.y = Math.PI / 4;
    root.add(seesaw);
    for (const object of [terrain, seesaw]) {
      object.traverse(node => {
        if (!(node instanceof THREE.Mesh)) return;
        node.castShadow = false;
        const materials = Array.isArray(node.material) ? node.material : [node.material];
        if (materials.some(material => /TRANSPARENT|DECAL/.test(String(material.userData.sm64Layer)))) return;
        addMeshCollision(collision, node);
        collisionMeshes.push(node);
      });
    }
    collision.finalize();
    const floorAt = (x: number, z: number) => {
      const hit = collision.findFloor(x, 20000, z);
      if (!hit.surface) throw new Error(`Battlefield has no floor at ${x}, ${z}`);
      return hit.height;
    };
    const spawnX = -6558, spawnZ = 6464;
    const spawnY = floorAt(spawnX, spawnZ);

    // An authored Jumpman challenge; these are not the game's behavior spawns.
    const coinSpots = [
      [-6360, 6280], [-6150, 6100], [-5880, 5800], [-5500, 5500],
      [-5000, 5200], [-4500, 5000], [-4000, 4700], [-3500, 4200],
      [-3000, 3500], [-2600, 2600], [-2200, 1600], [-1700, 700], [-1000, 400],
    ];
    const coins: Coin[] = coinSpots.map(([x, z]) => {
      const group = instantiateSm64(coinAsset);
      group.scale.multiplyScalar(0.7);
      const position = new THREE.Vector3(x, floorAt(x, z) + 65, z);
      group.position.copy(position);
      root.add(group);
      return { group, position, collected: false };
    });

    const star = instantiateSm64(starAsset);
    star.scale.multiplyScalar(0.25);
    const starY = floorAt(1636, -5567) + 100;
    star.position.set(1636, starY, -5567);
    root.add(star);
    const starLight = new THREE.PointLight(0xffd969, 200, 450, 2);
    starLight.position.copy(star.position);
    root.add(starLight);

    for (const [x, z] of [[-6200, 6850], [-5700, 6650], [-5000, 6100]]) {
      const tree = instantiateSm64(treeAsset);
      tree.position.set(x, floorAt(x, z), z);
      root.add(tree);
    }
    const mixers: THREE.AnimationMixer[] = [];
    for (const [x, z] of [[-4800, 5700], [-3700, 5300]]) {
      const goomba = instantiateSm64(goombaAsset);
      goomba.position.set(x, floorAt(x, z), z);
      root.add(goomba);
      const mixer = new THREE.AnimationMixer(goomba.children[0]);
      if (goombaAsset.animations[0]) mixer.clipAction(goombaAsset.animations[0]).play();
      mixers.push(mixer);
    }
    scene.add(root);
    const billboards = new Sm64Billboards(root);
    let lastTime = 0;
    return {
      collision, collisionMeshes, coins, star, starLight,
      spawn: { pos: [spawnX, spawnY, spawnZ], faceYaw: 0x6000 },
      update(time) {
        const dt = Math.max(0, Math.min(0.1, time - lastTime));
        lastTime = time;
        for (const mixer of mixers) mixer.update(dt);
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
  };
  // Build/probe collision before Game construction so an invalid library can
  // fall back cleanly, just like a missing download.
  const staging = new THREE.Scene();
  const world = build(staging);
  return scene => {
    scene.add(...[...staging.children]);
    return world;
  };
}
