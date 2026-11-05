import * as THREE from 'three';
import { CollisionWorld } from '../sim/collision';
import { SURFACE_ICE, SURFACE_NOT_SLIPPERY, SURFACE_SLIPPERY, SURFACE_VERY_SLIPPERY } from '../sim/surfaces';
import { addMeshCollision, type Coin, type PlaygroundResult, type PlaygroundSkin } from './world';

export const PLAYGROUND_SIZE = 12000;
export function playgroundZone(x: number, z: number): string {
  if (x > 1800 && z < -1800) return 'STAR STAIRCASE';
  if (x < -1800 && z < 0) return 'WALL KICK CANYON';
  if (x > 1800 && z >= -1800 && z < 1800) return 'SLIDE RINK';
  if (z < -2800 && Math.abs(x) < 1300) return 'LONG JUMP GAP';
  if (z < -800 && Math.abs(x) < 1300) return 'JUMP GARDEN';
  if (z > 2000 && x < -1600) return 'STAIR & PYRAMID';
  if (z > 2000 && x > -1000) return 'SLOPE LAB';
  return 'OPEN MEADOW';
}

/** Spacious, deliberately placed challenges. All architecture has matching collision. */
export function buildNativePlayground(scene: THREE.Scene, skin: PlaygroundSkin): PlaygroundResult {
  const collision = new CollisionWorld();
  const collisionMeshes: THREE.Mesh[] = [];
  const coins: Coin[] = [];
  const obstacles: THREE.Box3[] = [];
  const { grass, stone, wood, ice } = skin.materials;
  function register(mesh: THREE.Mesh, surface = 0) {
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    scene.add(mesh);
    addMeshCollision(collision, mesh, surface);
    collisionMeshes.push(mesh);
    obstacles.push(new THREE.Box3().setFromObject(mesh));
    return mesh;
  }
  function box(x: number, z: number, w: number, h: number, d: number, material = stone, surface = 0) {
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), material);
    mesh.position.set(x, h / 2, z);
    return register(mesh, surface);
  }
  function crate(x: number, z: number, size: number) {
    const blocker = box(x, z, size, size, size, wood);
    blocker.visible = false;
    blocker.userData.nativeCollision = true;
    const model = skin.make('crate', new THREE.Vector3(size, size, size));
    model.position.set(x, 0, z);
    scene.add(model);
  }
  function coin(x: number, y: number, z: number) {
    const group = skin.make('coin');
    group.position.set(x, y, z);
    scene.add(group);
    coins.push({ group, position: group.position.clone(), collected: false });
  }
  // A closed wedge with a continuous inclined floor, rising toward +Z.
  function ramp(x: number, z: number, width: number, length: number, height: number, material: THREE.Material, surface = 0) {
    const vertices = [-width/2,0,-length/2, width/2,0,-length/2, -width/2,0,length/2, width/2,0,length/2, -width/2,height,length/2, width/2,height,length/2];
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.Float32BufferAttribute(vertices, 3));
    geometry.setAttribute('uv', new THREE.Float32BufferAttribute([0,0,1,0,0,1,1,1,0,1,1,1], 2));
    geometry.setIndex([0,4,5,0,5,1,2,3,5,2,5,4,0,2,4,1,5,3,0,1,3,0,3,2]);
    geometry.computeVertexNormals();
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(x, 0, z);
    register(mesh, surface);
  }

  const ground = new THREE.Mesh(new THREE.PlaneGeometry(PLAYGROUND_SIZE, PLAYGROUND_SIZE), grass);
  ground.rotation.x = -Math.PI / 2;
  register(ground);
  obstacles.pop(); // The ground must not exclude every scenery placement.
  ground.castShadow = false;

  // North: four distinct crates, then a separate long-jump runway and landing.
  [100, 160, 220, 280].forEach((size, i) => {
    const z = -1000 - i * 450;
    crate(0, z, size);
    coin(0, size + 70, z);
  });
  box(0, -3400, 700, 180, 900, wood);
  box(0, -4500, 700, 180, 600, wood);
  coin(0, 250, -4500);
  // Access stairs meet the runway's south edge without intersecting it.
  [60,120,180].forEach((h,i) => box(-280 + i*140, -2880, 140, h, 140));

  // West: two solid walls with a 300-unit clear channel and an open exit.
  box(-3350, -1100, 180, 600, 1400);
  box(-2870, -1100, 180, 600, 1400);
  coin(-3110, 100, -650);
  coin(-3110, 420, -1450);

  // East: a broad, continuous ice ramp and runout. No coplanar floor overlays.
  ramp(3100, 300, 900, 1600, 480, ice, SURFACE_ICE);
  box(3100, 1400, 900, 480, 600);
  box(3100, -1300, 900, 8, 1600, ice, SURFACE_ICE);
  coin(3100, 550, 1400);
  coin(3100, 78, -1700);

  // North-east: climbable stone staircase, every tread supported to ground.
  for (let i = 0; i < 12; i++) box(2900, -2200 - i * 150, 500, (i + 1) * 60, 150);
  box(2900, -4200, 800, 720, 550);
  coin(2900, 430, -2950);
  const star = skin.make('star');
  star.position.set(2900, 820, -4200);
  const starLight = new THREE.PointLight(0xffbf33, 260, 900, 2);
  star.add(starLight);
  scene.add(star);

  // South-west: adjacent solid steps, ending at a broad viewing platform.
  for (let i = 0; i < 8; i++) box(-3100, 2200 + i * 180, 700, (i+1)*50, 180);
  box(-3100, 3980, 1100, 400, 860);
  coin(-3100, 470, 3980);

  // South: ramps with generous side clearance and the original slope physics.
  const surfaces = [SURFACE_NOT_SLIPPERY, SURFACE_SLIPPERY, SURFACE_VERY_SLIPPERY, SURFACE_ICE];
  [20,30,45,60].forEach((degrees,i) => {
    const x = -300 + i * 850;
    const height = 600 * Math.tan(degrees * Math.PI / 180);
    ramp(x, 3200, 480, 600, height, i === 3 ? ice : stone, surfaces[i]);
    coin(x, height + 70, 3480);
  });

  // Deterministic peripheral groves; no tiny bushes, random crates or overlapping hills.
  const trees: THREE.Vector2[] = [];
  for (let row = -5; row <= 5; row++) for (let col = -5; col <= 5; col++) {
    if ((row * row + col * col) < 14 || (row + col) % 3 !== 0) continue;
    const x = col * 1000 + (row % 2) * 160;
    const z = row * 1000;
    if (obstacles.some(bounds => x >= bounds.min.x - 450 && x <= bounds.max.x + 450 && z >= bounds.min.z - 450 && z <= bounds.max.z + 450)) continue;
    if (trees.some(p => p.distanceTo(new THREE.Vector2(x,z)) < 650)) continue;
    trees.push(new THREE.Vector2(x,z));
    const tree = skin.make('tree');
    tree.position.set(x,0,z);
    scene.add(tree);
    const trunk = new THREE.Mesh(new THREE.CylinderGeometry(14,14,160,8), wood);
    trunk.position.set(x,80,z);
    register(trunk);
    trunk.visible = false;
    trunk.userData.nativeCollision = true;
  }
  collision.finalize();
  return {
    collision, collisionMeshes, coins, star, starLight,
    spawn: { pos: [0, 10, 500], faceYaw: 0x8000 },
    update(time) {
      for (const coin of coins) {
        coin.group.position.y = coin.position.y + Math.sin(time*2.2)*4;

      }
      star.position.y = 820 + Math.sin(time*1.6)*10;
      star.rotation.y = time*1.25;
    },
  };
}
