import * as THREE from 'three';
import { GLTFLoader, type GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { clone } from 'three/addons/utils/SkeletonUtils.js';

// Vite emits the copied library with hashed URLs for production. Only models
// actually requested below are downloaded by the browser.
const urls = import.meta.glob<string>('../../assets/{actors,levels}/**/*.glb', {
  eager: true, query: '?url', import: 'default',
});
const prefix = '../../assets/';
export const sm64AssetPaths = Object.keys(urls).map(key => key.slice(prefix.length)).sort();
export const SM64_MODELS = {
  mario: 'actors/mario/mario_geo.glb',
  battlefield: 'levels/bob/bob_geo_000488.glb',
  redCoin: 'actors/coin/red_coin_no_shadow_geo.glb',
  coin: 'actors/coin/yellow_coin_no_shadow_geo.glb',
  transparentStar: 'actors/transparent_star/transparent_star_geo.glb',
  star: 'actors/star/star_geo.glb',
  tree: 'actors/tree/bubbly_tree_geo.glb',
  goomba: 'actors/goomba/goomba_geo.glb',
} as const;

export function sm64AssetUrl(path: string): string {
  const url = urls[prefix + path];
  if (!url) throw new Error(`SM64 asset missing: assets/${path}. Copy the exporter output into assets/.`);
  return url;
}

export function sourceUnitScale(asset: GLTF['asset']): number {
  const metersPerUnit = Number(asset.extras?.sm64UnitsToMeters ?? 0.01);
  if (!Number.isFinite(metersPerUnit) || metersPerUnit <= 0) {
    throw new Error('Invalid SM64 unit scale');
  }
  return 1 / metersPerUnit;
}

const pending = new Map<string, Promise<GLTF>>();
export function loadSm64Asset(path: string): Promise<GLTF> {
  if (!pending.has(path)) {
    const promise = new GLTFLoader().loadAsync(sm64AssetUrl(path)).catch(error => {
      pending.delete(path);
      throw error;
    });
    pending.set(path, promise);
  }
  return pending.get(path)!;
}

export function instantiateSm64(gltf: GLTF): THREE.Group {
  const group = new THREE.Group();
  group.name = String(gltf.asset.extras?.symbol ?? 'sm64-asset');
  group.scale.setScalar(sourceUnitScale(gltf.asset));
  group.add(clone(gltf.scene));
  group.traverse(object => {
    if (!(object instanceof THREE.Mesh)) return;
    object.userData.sharedSm64Asset = true;
    object.castShadow = true;
    object.receiveShadow = true;
  });
  group.traverse(node => {
    // N64 decal render modes bias coplanar overlays (tree shadows, floor emblems).
    // glTF has no polygon-offset field, so restore it from the GeoLayout layer.
    if (!String(node.userData.sm64Args?.[0]).includes('DECAL')) return;
    node.traverse(child => {
      if (!(child instanceof THREE.Mesh)) return;
      child.castShadow = false;
      child.receiveShadow = false;
      child.renderOrder = 1;
      for (const material of Array.isArray(child.material) ? child.material : [child.material]) {
        material.polygonOffset = true;
        material.polygonOffsetFactor = -2;
        material.polygonOffsetUnits = -2;
        material.depthWrite = false;
      }
    });
  });
  return group;
}

/** Restore camera-facing orientation while preserving the exporter's scale fix. */
export class Sm64Billboards {
  private readonly nodes: THREE.Object3D[] = [];
  private readonly seen = new Set<THREE.Object3D>();
  private readonly parentRotation = new THREE.Quaternion();
  private readonly cameraRotation = new THREE.Quaternion();

  constructor(root: THREE.Object3D) {
    this.register(root);
  }

  /** Register billboards added after the initial scene scan (course objects). */
  register(root: THREE.Object3D): void {
    root.traverse(node => {
      if (this.seen.has(node)) return;
      if (node.userData.objectBillboard || String(node.userData.sm64Command).startsWith('GEO_BILLBOARD')) {
        this.seen.add(node);
        this.nodes.push(node);
        // mtxf_billboard replaces the inherited matrix basis. Object scale remains,
        // but GEO_SCALE (e.g. Goomba's 1/4 skeletal scale) must not shrink the sprite.
        if (!node.userData.objectBillboard) {
          let geoScale = 1;
          for (let parent = node.parent; parent; parent = parent.parent) {
            if (parent.userData.sm64Command === 'GEO_SCALE') geoScale *= parent.scale.x;
          }
          if (geoScale !== 0) node.scale.multiplyScalar(1 / geoScale);
        }
        // Native billboard sprites have baked shading, not rotating shadow-map silhouettes.
        node.traverse(child => {
          if (child instanceof THREE.Mesh) { child.castShadow = false; child.receiveShadow = false; }
        });
      }
    });
  }

  get size(): number {
    return this.nodes.length;
  }

  update(camera: THREE.Camera): void {
    camera.getWorldQuaternion(this.cameraRotation);
    for (const node of this.nodes) {
      if (!node.parent) continue;
      node.parent.getWorldQuaternion(this.parentRotation);
      node.quaternion.copy(this.parentRotation.invert()).multiply(this.cameraRotation);
      node.updateMatrixWorld(true);
    }
  }
}
