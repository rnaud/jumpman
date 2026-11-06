import * as THREE from 'three';

export interface WaterSurface {
  mesh: THREE.Mesh;
  update(time: number): void;
  dispose(): void;
}

interface WaterGeometryState {
  base: Float32Array;
  phase: number;
}

/** Animated translucent planes over the collision world's water boxes. */
export function addWaterSurfaces(
  scene: THREE.Scene,
  boxes: [number, number, number, number, number, number][],
): WaterSurface[] {
  const surfaces: WaterSurface[] = [];
  for (const [id, x1, z1, x2, z2, y] of boxes) {
    if (id >= 50) continue;
    const width = Math.abs(x2 - x1);
    const depth = Math.abs(z2 - z1);
    if (width < 2 || depth < 2) continue;
    const geometry = new THREE.PlaneGeometry(width, depth, 18, 18);
    geometry.rotateX(-Math.PI / 2);
    const base = Float32Array.from(geometry.attributes.position.array as Float32Array);
    const phase = (id * 1.7) % (Math.PI * 2);
    const material = new THREE.MeshStandardMaterial({
      color: 0x4fb9df,
      transparent: true,
      opacity: 0.34,
      roughness: 0.16,
      metalness: 0.05,
      depthWrite: false,
      side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set((x1 + x2) / 2, y - 4, (z1 + z2) / 2);
    mesh.receiveShadow = false;
    mesh.renderOrder = 2;
    scene.add(mesh);
    const state: WaterGeometryState = { base, phase };
    surfaces.push({
      mesh,
      update(time: number): void {
        const position = geometry.attributes.position;
        const array = position.array as Float32Array;
        for (let i = 0; i < array.length; i += 3) {
          const x = state.base[i];
          const z = state.base[i + 2];
          array[i + 1] = Math.sin(x * 0.012 + time * 1.4 + state.phase) * 3.5
            + Math.cos(z * 0.009 + time * 0.9 + state.phase) * 2.5;
        }
        position.needsUpdate = true;
        material.opacity = 0.3 + Math.sin(time * 1.2 + state.phase) * 0.04;
      },
      dispose(): void {
        scene.remove(mesh);
        geometry.dispose();
        material.dispose();
      },
    });
  }
  return surfaces;
}
