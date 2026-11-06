import * as THREE from 'three';
import type { MarioRuntime } from '../sim/MarioRuntime';
import type { Vec3 } from '../sim/math';
import type { SaveFile } from './save';

type Pair = [number, number];
interface Painting {
  name: string; index: number; id: number; pos: Vec3; rotation: Pair; size: number; dimensions: Pair;
  opacity: number; environment: boolean; magnitude: Pair; decay: Pair; rate: Pair; dispersion: Pair;
  parts: { shape: string; texture: string }[];
}
export interface PaintingArt {
  shapes: Record<string, { vertices: [number, number, number, number, number][]; triangles: number[] }>;
  layouts: Painting[];
}
const urls = import.meta.glob<string>('../../assets/extracted/levels/castle_inside/*.png', {
  eager: true, query: '?url', import: 'default',
});

// geo_painting_draw supplies geometry at runtime, outside the static level display lists.
export async function buildCastlePaintings(art: PaintingArt, parent: THREE.Group) {
  const loader = new THREE.TextureLoader();
  const names = new Set(art.layouts.flatMap(p => p.parts.map(part => part.texture)));
  const textures = new Map(await Promise.all([...names].map(async name => {
    const texture = await loader.loadAsync(urls['../../assets/extracted/levels/castle_inside/' + name]);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.magFilter = THREE.NearestFilter;
    return [name, texture] as const;
  })));
  const paintings = art.layouts.map(spec => {
    const root = new THREE.Group();
    root.name = spec.name + '_painting';
    root.position.fromArray(spec.pos);
    root.rotation.set(spec.rotation[0] * Math.PI / 180, spec.rotation[1] * Math.PI / 180, 0);
    root.scale.setScalar(spec.size / 614);
    const parts = spec.parts.map(part => {
      const shape = art.shapes[part.shape];
      const geometry = new THREE.BufferGeometry();
      geometry.setIndex(shape.triangles);
      geometry.setAttribute('position', new THREE.Float32BufferAttribute(shape.vertices.flatMap(([x,y]) => [x,y,0]), 3));
      geometry.setAttribute('uv', new THREE.Float32BufferAttribute(shape.vertices.flatMap(([, , , u, v]) =>
        [(u / 32 + 0.5) / spec.dimensions[0], 1 - (v / 32 + 0.5) / spec.dimensions[1]]), 2));
      geometry.computeVertexNormals();
      const texture = textures.get(part.texture);
      const material = spec.environment ? new THREE.MeshMatcapMaterial({matcap: texture}) : new THREE.MeshBasicMaterial({map: texture});
      material.opacity = spec.opacity;
      material.transparent = spec.opacity < 1;
      const mesh = new THREE.Mesh(geometry, material);
      root.add(mesh);
      return { geometry, material, shape };
    });
    parent.add(root);
    return { spec, root, parts };
  });
  const docks = paintings.find(p => p.spec.name === 'ddd');
  let floorType = -1;
  let active: typeof paintings[number] | undefined;
  let frame = 0, magnitude = 0, phase = 0, originX = 0, originY = 0;
  function deform(painting: typeof paintings[number], flat: boolean) {
    const {spec} = painting;
    for (const {geometry, shape} of painting.parts) {
      const position = geometry.getAttribute('position');
      for (let i = 0; i < shape.vertices.length; i++) {
        const [x, y, movable] = shape.vertices[i];
        if (!movable) continue;
        const distance = Math.hypot(x * spec.size / 614 - originX, y * spec.size / 614 - originY);
        const elapsed = frame - distance / spec.dispersion[phase];
        const z = flat || elapsed < 0 ? 0 : Math.round(magnitude * Math.cos(spec.rate[phase] * 2 * Math.PI * elapsed));
        position.setZ(i, z);
      }
      position.needsUpdate = true;
      geometry.computeVertexNormals();
    }
  }
  return {
    showRoom(ids: number[]) {
      for (const p of paintings) {
        // The GeoLayout index and ripple-floor ID differ for HMC.
        p.root.visible = ids.includes(p.spec.index);
      }
    },
    reset(save: SaveFile) {
      if (active) deform(active, true);
      active = undefined;
      floorType = -1;
      if (docks) docks.root.position.x = save.hasFlag(0x100) ? 5529.6 : 3456;
    },
    step(m: MarioRuntime, save: SaveFile) {
      if (docks && save.hasStar('ddd', 1) && docks.root.position.x < 5529.6) {
        docks.root.position.x = Math.min(5529.6, docks.root.position.x + 20);
        if (docks.root.position.x === 5529.6) save.setFlag(0x100);
      }
      const type = m.floor?.type ?? -1;
      if (type !== floorType) {
        floorType = type;
        for (const p of paintings) {
          const {spec} = p;
          const wobble = type - (0xa6 + 3 * spec.id), enter = type - (0xd3 + 3 * spec.id);
          const entering = enter >= 0 && enter < 3;
          if (!entering && (wobble < 0 || wobble >= 3)) continue;
          if (active && active !== p) deform(active, true);
          if (p !== active || !spec.environment) frame = 0;
          active = p;
          phase = entering ? 1 : 0;
          magnitude = spec.magnitude[phase];
          originX = (spec.environment && !entering ? 0.5 : ((entering ? enter : wobble) + 1) / 4) * spec.size;
          originY = spec.environment && !entering ? spec.size / 2 : THREE.MathUtils.clamp(m.pos[1] - spec.pos[1] + 50, 0, spec.size);
          if (spec.rotation[0] && entering) {
            originX = THREE.MathUtils.clamp(m.pos[0] - spec.pos[0], 0, spec.size);
            originY = THREE.MathUtils.clamp(spec.pos[2] - m.pos[2], 0, spec.size);
          }
          break;
        }
      }
      if (!active) return;
      deform(active, false);
      frame++;
      magnitude *= active.spec.decay[phase];
      if (active.spec.environment && phase === 1 && magnitude <= active.spec.magnitude[0]) {
        magnitude = active.spec.magnitude[0];
        phase = 0;
      } else if (magnitude <= 1) {
        deform(active, true);
        active = undefined;
      }
    },
    dispose() {
      for (const p of paintings) for (const part of p.parts) {
        part.geometry.dispose();
        part.material.dispose();
      }
      for (const texture of textures.values()) texture.dispose();
    },
  };
}
