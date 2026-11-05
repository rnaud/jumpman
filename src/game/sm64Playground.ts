import * as THREE from 'three';
import { buildNativePlayground } from './playgroundLayout';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';
import type { WorldFactory } from './sm64World';
import grassUrl from '../../assets/extracted/textures/generic/bob_textures.05800.rgba16.png?url';
import stoneUrl from '../../assets/extracted/textures/generic/bob_textures.05000.rgba16.png?url';
import woodUrl from '../../assets/extracted/textures/generic/bob_textures.06000.rgba16.png?url';
import iceUrl from '../../assets/extracted/textures/snow/ccm_textures.00800.rgba16.png?url';

const paths = {
  tree: SM64_MODELS.tree,
  coin: SM64_MODELS.coin,
  star: SM64_MODELS.star,
  crate: 'actors/breakable_box/breakable_box_geo.glb',
} as const;

/** Keep the movement course, dress it with the extracted game's resources. */
export async function preparePlayground(): Promise<WorldFactory> {
  const models = new Map(await Promise.all(Object.entries(paths).map(async ([key, path]) =>
    [key, await loadSm64Asset(path)] as const)));
  const loader = new THREE.TextureLoader();
  const maps = await Promise.all([grassUrl, stoneUrl, woodUrl, iceUrl].map(url => loader.loadAsync(url)));
  return scene => {
    const textures = maps.map((source, i) => {
      const map = source.clone();
      map.colorSpace = THREE.SRGBColorSpace;
      map.wrapS = map.wrapT = THREE.RepeatWrapping;
      map.magFilter = THREE.NearestFilter;
      map.minFilter = THREE.NearestMipmapLinearFilter;
      map.repeat.setScalar(i === 0 ? 60 : 2);
      map.needsUpdate = true;
      return map;
    });
    const materials = textures.map(map => new THREE.MeshStandardMaterial({ map, roughness: 1 }));
    const world = buildNativePlayground(scene, {
      materials: { grass: materials[0], stone: materials[1], wood: materials[2], ice: materials[3] },
      make(kind, size) {
        const model = instantiateSm64(models.get(kind)!);
        if (kind === 'tree' || kind === 'coin') {
          model.userData.objectBillboard = true;
          model.traverse(node => {
            if (node instanceof THREE.Mesh) {
              for (const material of Array.isArray(node.material) ? node.material : [node.material]) material.side = THREE.DoubleSide;
            }
          });
        }
        // Native trees are ~320 units tall; keep clear sight lines in the course.
        if (kind === 'tree') model.scale.multiplyScalar(0.65);
        if (kind === 'star') model.scale.multiplyScalar(0.5);
        if (size) {
          const bounds = new THREE.Box3().setFromObject(model);
          const extent = bounds.getSize(new THREE.Vector3());
          model.scale.multiply(size.clone().divide(extent));
          const fitted = new THREE.Box3().setFromObject(model);
          const center = fitted.getCenter(new THREE.Vector3());
          const wrapper = new THREE.Group();
          model.position.set(-center.x, -fitted.min.y, -center.z);
          wrapper.add(model);
          return wrapper;
        }
        return model;
      },
    });
    const billboards = new Sm64Billboards(scene);
    world.faceCamera = camera => billboards.update(camera);
    world.dispose = () => {
      textures.forEach(texture => texture.dispose());
      maps.forEach(texture => texture.dispose());
      materials.forEach(material => material.dispose());
    };
    return world;
  };
}
