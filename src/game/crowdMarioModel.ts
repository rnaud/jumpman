import * as THREE from 'three';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';

let geometry: THREE.BufferGeometry | null = null;
let material: THREE.MeshStandardMaterial | null = null;

function part(w: number, h: number, d: number, x: number, y: number, z: number, color: number): THREE.BufferGeometry {
  const box = new THREE.BoxGeometry(w, h, d).toNonIndexed();
  box.translate(x, y, z);
  const c = new THREE.Color(color);
  const colors = new Float32Array(box.getAttribute('position').count * 3);
  for (let i = 0; i < colors.length; i += 3) {
    colors[i] = c.r; colors[i + 1] = c.g; colors[i + 2] = c.b;
  }
  box.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  return box;
}

/** One merged, vertex-colored low-poly Mario shared by the whole crowd. */
export function createCrowdMario(): THREE.Mesh {
  if (!geometry) {
    const parts = [
      part(18, 8, 20, 0, 80, 0, 0xd92b2b),      // cap crown
      part(26, 4, 15, 0, 76, 7, 0xd92b2b),      // brim
      part(20, 16, 18, 0, 68, 0, 0xf2c08a),     // head
      part(4, 4, 2, -4, 70, 9, 0x1b1b1b),       // eyes
      part(4, 4, 2, 4, 70, 9, 0x1b1b1b),
      part(10, 3, 3, 0, 64, 10, 0x6b3d20),      // mustache
      part(25, 19, 17, 0, 51, 0, 0xd92b2b),     // shirt
      part(22, 18, 18, 0, 35, 0, 0x2b58c8),     // overalls
      part(5, 15, 5, -17, 46, 0, 0xf2c08a),     // arms
      part(5, 15, 5, 17, 46, 0, 0xf2c08a),
      part(8, 17, 8, -7, 18, 0, 0x2b58c8),      // legs
      part(8, 17, 8, 7, 18, 0, 0x2b58c8),
      part(11, 7, 17, -8, 5, 3, 0x5b3218),      // shoes
      part(11, 7, 17, 8, 5, 3, 0x5b3218),
    ];
    geometry = mergeGeometries(parts);
    geometry.computeVertexNormals();
    material = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.72, metalness: 0.02 });
  }
  const mesh = new THREE.Mesh(geometry, material!);
  mesh.castShadow = false;
  mesh.receiveShadow = false;
  return mesh;
}
