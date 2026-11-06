/**
 * The playground: a Three.js course built to exercise every part of the SM64
 * movement kit.  Every piece of collision is extracted from the rendered mesh,
 * so what you see is exactly what you land on.
 */

import * as THREE from 'three';
import type { MovingPlatform } from './movingPlatform';
import type { GoombaEnemy } from './goomba';
import type { CourseObjects } from './courseObjects';
import { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';
import {
  SURFACE_ICE,
  SURFACE_NOT_SLIPPERY,
  SURFACE_SLIPPERY,
  SURFACE_VERY_SLIPPERY,
} from '../sim/surfaces';

export interface CastleDoor {
  pos: Vec3;
  yaw: number;
  node: string;
  destNode: string;
  play?: (index: number) => void;
  destination: 'castle_grounds' | 'castle_inside';
}

export interface Coin {
  group: THREE.Group;
  position: THREE.Vector3;
  collected: boolean;
}

export interface PlaygroundResult {
  dispose?: () => void;
  reset?: () => void;
  objects?: CourseObjects;
  requiredCoins?: number;
  doors?: CastleDoor[];
  platforms?: MovingPlatform[];
  goombas?: GoombaEnemy[];
  deathHeight?: number;
  requireCoins?: boolean;
  starRequiresEvent?: boolean;
  collision: CollisionWorld;
  collisionMeshes: THREE.Mesh[];
  spawn: { pos: Vec3; faceYaw: number };
  coins: Coin[];
  redCoins?: Coin[];
  star: THREE.Group;
  starLight: THREE.PointLight;
  update: (time: number) => void;
  faceCamera?: (camera: THREE.Camera) => void;
}

const COLORS = {
  grassA: 0x84c95c,
  grassB: 0x74b94e,
  stone: 0xd7c39a,
  stoneDark: 0xb59d72,
  platform: 0xe8d7b0,
  brick: 0xc98f63,
  wall: 0x8f6f52,
  ice: 0xa9e6ff,
  gold: 0xffc61a,
  red: 0xd94b3d,
  blue: 0x2f6fd0,
};

function standard(color: number, options: Partial<THREE.MeshStandardMaterialParameters> = {}) {
  return new THREE.MeshStandardMaterial({
    color,
    roughness: 0.85,
    metalness: 0.02,
    flatShading: true,
    ...options,
  });
}

/** Extract world-space triangles from a mesh into the SM64 collision world. */
export function addMeshCollision(
  collision: CollisionWorld,
  mesh: THREE.Mesh,
  type = 0,
): void {
  mesh.updateWorldMatrix(true, false);
  const source = mesh.geometry;
  const geometry = source.index ? source.toNonIndexed() : source;
  const position = geometry.getAttribute('position');
  const a = new THREE.Vector3();
  const b = new THREE.Vector3();
  const c = new THREE.Vector3();

  for (let i = 0; i < position.count; i += 3) {
    a.fromBufferAttribute(position, i).applyMatrix4(mesh.matrixWorld);
    b.fromBufferAttribute(position, i + 1).applyMatrix4(mesh.matrixWorld);
    c.fromBufferAttribute(position, i + 2).applyMatrix4(mesh.matrixWorld);
    collision.addTriangle([a.x, a.y, a.z], [b.x, b.y, b.z], [c.x, c.y, c.z], { type });
  }

  if (geometry !== source) geometry.dispose();
}

function makeCheckerTexture(): THREE.CanvasTexture {
  const size = 128;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d')!;

  ctx.fillStyle = '#84c95c';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#74b94e';
  ctx.fillRect(0, 0, size / 2, size / 2);
  ctx.fillRect(size / 2, size / 2, size / 2, size / 2);

  // A few subtle speckles so the ground is not perfectly flat.
  for (let i = 0; i < 40; i++) {
    ctx.fillStyle = i % 2 === 0 ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.04)';
    ctx.fillRect(Math.random() * size, Math.random() * size, 2, 2);
  }

  const texture = new THREE.CanvasTexture(canvas);
  texture.wrapS = THREE.RepeatWrapping;
  texture.wrapT = THREE.RepeatWrapping;
  texture.repeat.set(30, 30);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 8;
  return texture;
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function makeLabel(
  text: string,
  options: { color?: string; bg?: string; width?: number; height?: number; font?: number } = {},
): THREE.Sprite {
  const width = options.width ?? 512;
  const height = options.height ?? 128;
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d')!;

  ctx.clearRect(0, 0, width, height);
  ctx.fillStyle = options.bg ?? 'rgba(20, 33, 43, 0.86)';
  roundRect(ctx, 4, 4, width - 8, height - 8, 26);
  ctx.fill();

  ctx.strokeStyle = 'rgba(255, 224, 150, 0.55)';
  ctx.lineWidth = 4;
  roundRect(ctx, 7, 7, width - 14, height - 14, 23);
  ctx.stroke();

  ctx.fillStyle = options.color ?? '#fff4d6';
  ctx.font = `700 ${options.font ?? 56}px ui-monospace, SFMono-Regular, Menlo, monospace`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(text, width / 2, height / 2 + 2);

  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 8;

  const material = new THREE.SpriteMaterial({
    map: texture,
    transparent: true,
    depthWrite: false,
    depthTest: true,
  });
  const sprite = new THREE.Sprite(material);
  const scale = 122;
  sprite.scale.set(scale, (scale * height) / width, 1);
  return sprite;
}

function makeRampGeometry(width: number, length: number, height: number): THREE.BufferGeometry {
  const x0 = -width / 2;
  const x1 = width / 2;
  const z0 = -length / 2;
  const z1 = length / 2;
  const positions: number[] = [];

  const quad = (
    a: [number, number, number],
    b: [number, number, number],
    c: [number, number, number],
    d: [number, number, number],
  ) => {
    positions.push(...a, ...b, ...c, ...a, ...c, ...d);
  };

  // Sloped top (rises toward +Z).
  quad([x0, 0, z0], [x0, height, z1], [x1, height, z1], [x1, 0, z0]);
  // Back face.
  quad([x0, 0, z1], [x1, 0, z1], [x1, height, z1], [x0, height, z1]);
  // Sides.
  quad([x0, 0, z0], [x0, 0, z1], [x0, height, z1], [x0, height, z0]);
  quad([x1, 0, z0], [x1, height, z0], [x1, height, z1], [x1, 0, z1]);
  // Bottom.
  quad([x0, 0, z0], [x1, 0, z0], [x1, 0, z1], [x0, 0, z1]);

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
  geometry.computeVertexNormals();
  geometry.computeBoundingSphere();
  return geometry;
}

function makeCoin(): THREE.Group {
  const group = new THREE.Group();

  const disc = new THREE.Mesh(
    new THREE.CylinderGeometry(15, 15, 4, 20),
    new THREE.MeshStandardMaterial({
      color: COLORS.gold,
      metalness: 0.85,
      roughness: 0.2,
      emissive: 0x6b4300,
      emissiveIntensity: 0.55,
    }),
  );
  disc.rotation.x = Math.PI / 2;
  disc.castShadow = true;

  const rim = new THREE.Mesh(
    new THREE.TorusGeometry(12, 2.4, 6, 18),
    new THREE.MeshStandardMaterial({
      color: 0xfff0a8,
      metalness: 0.7,
      roughness: 0.25,
      emissive: 0x4a2c00,
      emissiveIntensity: 0.4,
    }),
  );
  rim.position.z = 2.2;

  group.add(disc, rim);
  return group;
}

function makeStar(): { group: THREE.Group; light: THREE.PointLight } {
  const shape = new THREE.Shape();
  const points = 5;
  for (let i = 0; i < points * 2; i++) {
    const radius = i % 2 === 0 ? 42 : 18;
    const angle = (i / (points * 2)) * Math.PI * 2 - Math.PI / 2;
    const x = Math.cos(angle) * radius;
    const y = Math.sin(angle) * radius;
    if (i === 0) shape.moveTo(x, y);
    else shape.lineTo(x, y);
  }
  shape.closePath();

  const geometry = new THREE.ExtrudeGeometry(shape, {
    depth: 12,
    bevelEnabled: true,
    bevelThickness: 3,
    bevelSize: 3,
    bevelSegments: 1,
    curveSegments: 1,
  });
  geometry.center();

  const material = new THREE.MeshStandardMaterial({
    color: 0xffdf4d,
    emissive: 0xff9d00,
    emissiveIntensity: 1.1,
    metalness: 0.65,
    roughness: 0.2,
  });

  const star = new THREE.Mesh(geometry, material);
  star.castShadow = true;

  const glowCanvas = document.createElement('canvas');
  glowCanvas.width = 256;
  glowCanvas.height = 256;
  const ctx = glowCanvas.getContext('2d')!;
  const gradient = ctx.createRadialGradient(128, 128, 5, 128, 128, 128);
  gradient.addColorStop(0, 'rgba(255, 236, 150, 0.95)');
  gradient.addColorStop(0.35, 'rgba(255, 190, 40, 0.42)');
  gradient.addColorStop(1, 'rgba(255, 160, 0, 0)');
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, 256, 256);

  const glowTexture = new THREE.CanvasTexture(glowCanvas);
  glowTexture.colorSpace = THREE.SRGBColorSpace;
  const glow = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: glowTexture,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      transparent: true,
    }),
  );
  glow.scale.set(340, 340, 1);

  const beamCanvas = document.createElement('canvas');
  beamCanvas.width = 128;
  beamCanvas.height = 512;
  const beamCtx = beamCanvas.getContext('2d')!;
  const beamGradient = beamCtx.createLinearGradient(0, 0, 0, 512);
  beamGradient.addColorStop(0, 'rgba(255, 226, 120, 0)');
  beamGradient.addColorStop(0.45, 'rgba(255, 214, 80, 0.28)');
  beamGradient.addColorStop(0.8, 'rgba(255, 190, 40, 0.16)');
  beamGradient.addColorStop(1, 'rgba(255, 190, 40, 0)');
  beamCtx.fillStyle = beamGradient;
  beamCtx.fillRect(0, 0, 128, 512);
  const beamTexture = new THREE.CanvasTexture(beamCanvas);
  beamTexture.colorSpace = THREE.SRGBColorSpace;
  const beam = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: beamTexture,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      depthTest: false,
      transparent: true,
      opacity: 0.4,
    }),
  );
  beam.scale.set(300, 2100, 1);
  beam.position.y = 760;
  beam.renderOrder = 20;

  const group = new THREE.Group();
  group.add(beam, glow, star);
  group.rotation.z = 0.08;

  const light = new THREE.PointLight(0xffbf33, 260, 900, 2);
  light.position.set(0, 40, 0);
  group.add(light);

  return { group, light };
}

export interface PlaygroundSkin {
  materials: Record<'grass' | 'stone' | 'wood' | 'ice', THREE.Material>;
  make: (kind: 'tree' | 'coin' | 'star' | 'crate', size?: THREE.Vector3) => THREE.Group;
}

export function buildPlayground(scene: THREE.Scene, skin?: PlaygroundSkin): PlaygroundResult {
  const collision = new CollisionWorld();
  const collisionMeshes: THREE.Mesh[] = [];
  const coins: Coin[] = [];

  const grassMat = skin?.materials.grass ?? new THREE.MeshStandardMaterial({
    map: makeCheckerTexture(),
    roughness: 1,
    metalness: 0,
  });
  const stoneMat = skin?.materials.stone ?? standard(COLORS.stone);
  const stoneDarkMat = skin?.materials.stone ?? standard(COLORS.stoneDark);
  const platformMat = skin?.materials.wood ?? standard(COLORS.platform);
  const brickMat = skin?.materials.wood ?? standard(COLORS.brick);
  const wallMat = skin?.materials.stone ?? standard(COLORS.wall, { roughness: 0.95 });
  const iceMat = skin?.materials.ice ?? new THREE.MeshStandardMaterial({
    color: 0x8fdaf2,
    emissive: 0x0b3f56,
    emissiveIntensity: 0.22,
    roughness: 0.08,
    metalness: 0.06,
    transparent: true,
    opacity: 0.94,
    flatShading: true,
  });

  const register = (mesh: THREE.Mesh, surfaceType = 0): THREE.Mesh => {
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    scene.add(mesh);
    addMeshCollision(collision, mesh, surfaceType);
    collisionMeshes.push(mesh);
    return mesh;
  };

  // -------------------------------------------------------------------------
  // Ground
  // -------------------------------------------------------------------------

  const groundGeo = new THREE.PlaneGeometry(6000, 6000, 60, 60);
  const ground = new THREE.Mesh(groundGeo, grassMat);
  ground.rotation.x = -Math.PI / 2;
  ground.receiveShadow = true;
  ground.position.y = 0;
  scene.add(ground);
  addMeshCollision(collision, ground);
  collisionMeshes.push(ground);

  // -------------------------------------------------------------------------
  // Small helpers
  // -------------------------------------------------------------------------

  const addBox = (
    x: number,
    y: number,
    z: number,
    w: number,
    h: number,
    d: number,
    material: THREE.Material = platformMat,
    type = 0,
    rotationY = 0,
  ): THREE.Mesh => {
    const geometry = new THREE.BoxGeometry(w, h, d);
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(x, y + h / 2, z);
    mesh.rotation.y = rotationY;
    register(mesh, type);
    if (skin && material === brickMat && h >= 100 && w === d) {
      const crate = skin.make('crate', new THREE.Vector3(w, h, d));
      crate.position.set(x, y, z);
      crate.rotation.y = rotationY;
      scene.add(crate);
      mesh.visible = false;
      mesh.userData.nativeCollision = true;
    }
    return mesh;
  };

  const addRamp = (
    x: number,
    y: number,
    z: number,
    width: number,
    length: number,
    height: number,
    material: THREE.Material = platformMat,
    type = 0,
    rotationY = 0,
  ): THREE.Mesh => {
    const mesh = new THREE.Mesh(makeRampGeometry(width, length, height), material);
    mesh.position.set(x, y, z);
    mesh.rotation.y = rotationY;
    return register(mesh, type);
  };

  const addLabel = (
    text: string,
    x: number,
    y: number,
    z: number,
    scale = 1,
    color?: string,
  ): THREE.Sprite => {
    const sprite = makeLabel(text, { color, width: 640, height: 140, font: 52 });
    sprite.position.set(x, y, z);
    sprite.scale.multiplyScalar(scale);
    scene.add(sprite);
    return sprite;
  };

  const addCoin = (x: number, y: number, z: number): void => {
    const group = skin?.make('coin') ?? makeCoin();
    group.position.set(x, y, z);
    scene.add(group);
    coins.push({ group, position: new THREE.Vector3(x, y, z), collected: false });
  };

  // -------------------------------------------------------------------------
  // Spawn plaza
  // -------------------------------------------------------------------------

  const plaza = new THREE.Mesh(new THREE.CylinderGeometry(340, 340, 26, 48), stoneMat);
  plaza.position.set(0, 13, 0);
  register(plaza);

  const plazaRing = new THREE.Mesh(
    new THREE.TorusGeometry(280, 7, 8, 60),
    standard(0xb8a374, { roughness: 1 }),
  );
  plazaRing.rotation.x = -Math.PI / 2;
  plazaRing.position.y = 27;
  plazaRing.receiveShadow = true;
  scene.add(plazaRing);


  // -------------------------------------------------------------------------
  // Jump garden (north)
  // -------------------------------------------------------------------------

  addBox(0, 0, -500, 320, 40, 320, brickMat);
  addBox(0, 0, -800, 280, 100, 280, brickMat);
  addBox(0, 0, -1100, 260, 180, 260, brickMat);
  addBox(0, 0, -1420, 240, 260, 240, brickMat);

  addLabel('JUMP GARDEN', 420, 190, -760, 1.1, '#ffe9a8');
  addCoin(0, 110, -500);
  addCoin(0, 170, -800);
  addCoin(0, 250, -1100);
  addCoin(0, 330, -1360);

  // Little archway for scale.
  addBox(-460, 0, -1900, 60, 420, 60, stoneDarkMat);
  addBox(460, 0, -1900, 60, 420, 60, stoneDarkMat);
  addBox(0, 420, -1900, 980, 60, 60, stoneDarkMat);

  // -------------------------------------------------------------------------
  // Wall kick canyon (west)
  // -------------------------------------------------------------------------

  addBox(-1660, 0, -650, 60, 520, 800, wallMat);
  addBox(-1300, 0, -650, 60, 520, 800, wallMat);
  addBox(-1480, 0, -1080, 420, 520, 60, wallMat);
  addBox(-1660, 520, -650, 60, 60, 800, stoneDarkMat);
  addBox(-1300, 520, -650, 60, 60, 800, stoneDarkMat);

  // Buttresses give the long wall face some depth and shadow.
  for (const z of [-420, -620, -820, -980]) {
    addBox(-1615, 0, z, 30, 520, 46, stoneDarkMat);
    addBox(-1345, 0, z, 30, 520, 46, stoneDarkMat);
  }

  // A darker stone floor strip makes the canyon read as a place.
  addBox(-1480, 0, -650, 280, 2, 800, stoneDarkMat);

  addLabel('WALL KICK CANYON', -1480, 700, -650, 1.2, '#ffe9a8');
  addCoin(-1480, 120, -380);
  addCoin(-1480, 300, -700);
  addCoin(-1300, 520, -1120);

  // Safe ledges halfway up the walls.
  addBox(-1660, 260, -1040, 180, 26, 150, stoneDarkMat);
  addBox(-1300, 380, -1040, 180, 26, 150, stoneDarkMat);

  // -------------------------------------------------------------------------
  // Slide rink (east)
  // -------------------------------------------------------------------------

  addBox(1500, 0, -700, 520, 280, 400, stoneMat);
  addRamp(1500, 0, -300, 480, 400, 280, iceMat, SURFACE_ICE, Math.PI);
  addBox(1500, 0, 300, 500, 10, 800, iceMat, SURFACE_ICE);
  addRamp(1500, 0, -1150, 420, 500, 280, stoneMat, 0);

  addLabel('BUTT-SLIDE RINK', 1850, 360, -700, 0.9, '#c8f4ff');
  const rinkLight = new THREE.PointLight(0x66d9ff, 420, 1000, 2);
  rinkLight.position.set(1500, 260, -300);
  scene.add(rinkLight);
  addCoin(1500, 340, -700);
  addCoin(1500, 80, 300);

  // -------------------------------------------------------------------------
  // Long jump gap (far north)
  // -------------------------------------------------------------------------

  addBox(0, 0, -1700, 520, 200, 520, brickMat);
  addBox(0, 0, -2520, 520, 150, 420, brickMat);
  addRamp(0, 0, -1290, 360, 420, 200, stoneMat, Math.PI);

  addLabel('LONG JUMP GAP', 0, 380, -1700, 1.15, '#ffe9a8');
  addCoin(0, 230, -2520);

  for (let i = 0; i < 4; i++) {
    const z = -1950 - i * 100;
    addBox(-330, 0, z, 14, 90, 14, stoneDarkMat);
    addLabel(`${(i + 1) * 100}`, -380, 120, z, 0.35);
  }

  // -------------------------------------------------------------------------
  // Star tower (north-east)
  // -------------------------------------------------------------------------

  addBox(1180, 0, -1960, 220, 760, 220, stoneDarkMat);
  for (let i = 0; i < 6; i++) {
    const angle = (i / 6) * Math.PI * 2;
    const radius = 235;
    const height = 110 + i * 118;
    addBox(
      1180 + Math.sin(angle) * radius,
      height - 20,
      -1960 + Math.cos(angle) * radius,
      210,
      20,
      150,
      platformMat,
      0,
      angle,
    );
  }
  addBox(1180, 760, -1960, 260, 20, 260, platformMat);

  addLabel('STAR TOWER', 1180, 900, -1960, 1.15, '#ffe9a8');
  addCoin(1180 + Math.sin(Math.PI * 0.66) * 235, 400, -1960 + Math.cos(Math.PI * 0.66) * 235);

  const starData = skin ? { group: skin.make('star'), light: new THREE.PointLight(0xffbf33, 260, 900, 2) } : makeStar();
  if (skin) starData.group.add(starData.light);
  starData.group.position.set(1180, 820, -1960);
  scene.add(starData.group);

  // -------------------------------------------------------------------------
  // Stairs and pyramid (south-west)
  // -------------------------------------------------------------------------

  for (let i = 0; i < 8; i++) {
    addBox(-1000, i * 40, 500 + i * 70, 400, 40, 70, stoneMat);
  }
  addBox(-2400, 0, 620, 700, 80, 700, brickMat);
  addBox(-2400, 80, 620, 560, 80, 560, brickMat);
  addBox(-2400, 160, 620, 420, 80, 420, brickMat);
  addBox(-2400, 240, 620, 280, 80, 280, brickMat);

  addLabel('STAIR & PYRAMID', -1700, 560, 620, 1.05, '#ffe9a8');
  addCoin(-2400, 390, 620);

  // -------------------------------------------------------------------------
  // Slope lab (south-east)
  // -------------------------------------------------------------------------

  const slopeLength = 350;
  const rampSpecs: Array<[number, number, number, number]> = [
    [-600, 20, slopeLength * Math.tan((20 * Math.PI) / 180), SURFACE_NOT_SLIPPERY],
    [-200, 30, slopeLength * Math.tan((30 * Math.PI) / 180), SURFACE_SLIPPERY],
    [200, 45, slopeLength * Math.tan((45 * Math.PI) / 180), SURFACE_VERY_SLIPPERY],
    [600, 60, slopeLength * Math.tan((60 * Math.PI) / 180), SURFACE_ICE],
  ];

  for (const [x, degrees, height, type] of rampSpecs) {
    addRamp(x, 0, 2400, 180, slopeLength, height, platformMat, type, 0);
    addLabel(`${degrees}°`, x, height + 90, 2225, 0.4, '#fff4d6');
  }

  addLabel('SLOPE LAB', 0, 760, 2400, 1.1, '#ffe9a8');
  addCoin(-200, 260, 2560);

  // -------------------------------------------------------------------------
  // Trees, rocks and bushes
  // -------------------------------------------------------------------------

  const trunkMat = standard(0x7a4f2b, { roughness: 1 });
  const leafMats = [
    standard(0x3f8f3a, { roughness: 1 }),
    standard(0x4da844, { roughness: 1 }),
    standard(0x2f7a36, { roughness: 1 }),
  ];
  const rockMat = standard(0x8b8f88, { roughness: 1 });

  let seed = 1337;
  const rand = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 0xffffffff;
  };

  const blocked = (x: number, z: number): boolean => {
    if (Math.hypot(x, z) < 520) return true;
    if (x > -420 && x < 420 && z > -2800 && z < 250) return true;
    if (x > -2050 && x < -1050 && z > -1300 && z < 850) return true;
    if (x > 1200 && x < 1800 && z > -1500 && z < 800) return true;
    if (Math.hypot(x - 1180, z + 1960) < 620) return true;
    if (x > 880 && x < 1500 && z > -1600 && z < -700) return true;
    if (x > -2850 && x < -600 && z > 240 && z < 1150) return true;
    if (x > -850 && x < 850 && z > 2100 && z < 2700) return true;
    return false;
  };

  const addTree = (x: number, z: number, scale: number): void => {
    const trunkHeight = 90 * scale;
    const trunk = new THREE.Mesh(new THREE.CylinderGeometry(10 * scale, 14 * scale, trunkHeight, 7), trunkMat);
    trunk.position.set(x, trunkHeight / 2, z);
    register(trunk);

    if (skin) {
      trunk.visible = false;
      trunk.userData.nativeCollision = true;
      const tree = skin.make('tree');
      tree.scale.multiplyScalar(scale);
      tree.position.set(x, 0, z);
      scene.add(tree);
      return;
    }
    const leaves = leafMats[Math.floor(rand() * leafMats.length)];
    for (let i = 0; i < 3; i++) {
      const radius = (58 - i * 10) * scale;
      const foliage = new THREE.Mesh(new THREE.IcosahedronGeometry(radius, 1), leaves);
      foliage.position.set(
        x + (rand() - 0.5) * 34 * scale,
        trunkHeight + radius * 0.5 + i * 34 * scale,
        z + (rand() - 0.5) * 34 * scale,
      );
      foliage.castShadow = true;
      foliage.receiveShadow = true;
      scene.add(foliage);
    }
  };

  let planted = 0;
  for (let attempt = 0; attempt < 220 && planted < 30; attempt++) {
    const angle = rand() * Math.PI * 2;
    const radius = 780 + rand() * 1500;
    const x = Math.sin(angle) * radius;
    const z = Math.cos(angle) * radius;
    if (blocked(x, z)) continue;
    addTree(x, z, 0.8 + rand() * 0.8);
    planted++;
  }

  for (let i = 0; i < 16; i++) {
    const angle = rand() * Math.PI * 2;
    const radius = 600 + rand() * 1700;
    const x = Math.sin(angle) * radius;
    const z = Math.cos(angle) * radius;
    if (blocked(x, z)) continue;
    const radiusRock = 26 + rand() * 60;
    if (skin) {
      addBox(x, 0, z, radiusRock * 2, radiusRock * 2, radiusRock * 2, brickMat);
      continue;
    }
    const rock = new THREE.Mesh(new THREE.IcosahedronGeometry(radiusRock, 0), rockMat);
    rock.position.set(x, radiusRock * 0.55, z);
    rock.rotation.set(rand() * 2, rand() * 2, rand() * 2);
    rock.scale.y = 0.72;
    register(rock);
  }

  const bushMat = standard(0x4c9a40, { roughness: 1 });
  for (let i = 0; i < 24; i++) {
    const angle = rand() * Math.PI * 2;
    const radius = 560 + rand() * 1600;
    const x = Math.sin(angle) * radius;
    const z = Math.cos(angle) * radius;
    if (blocked(x, z)) continue;
    if (skin) { addTree(x, z, 0.35 + rand() * 0.2); continue; }
    const bush = new THREE.Mesh(new THREE.IcosahedronGeometry(24 + rand() * 22, 1), bushMat);
    bush.position.set(x, 14, z);
    bush.scale.y = 0.7;
    bush.castShadow = true;
    bush.receiveShadow = true;
    scene.add(bush);
  }

  // -------------------------------------------------------------------------
  // Boundary scenery
  // -------------------------------------------------------------------------

  // Low hills around the edge make the 6000-unit plane feel intentional.
  const hillMat = skin?.materials.grass ?? standard(0x6da848, { roughness: 1 });
  for (let i = 0; i < 18; i++) {
    const angle = (i / 18) * Math.PI * 2;
    const radius = 2600 + (i % 3) * 90;
    const size = 210 + (i % 4) * 60;
    const hill = new THREE.Mesh(new THREE.IcosahedronGeometry(size, 1), hillMat);
    hill.position.set(Math.sin(angle) * radius, size * 0.25, Math.cos(angle) * radius);
    hill.scale.y = 0.55;
    hill.castShadow = false;
    hill.receiveShadow = true;
    scene.add(hill);
  }

  // Clouds.
  const cloudMat = new THREE.MeshBasicMaterial({
    color: 0xffffff,
    transparent: true,
    opacity: 0.82,
    depthWrite: false,
  });
  for (let i = 0; i < 12; i++) {
    const cloud = new THREE.Group();
    const puffs = 3 + (i % 3);
    for (let j = 0; j < puffs; j++) {
      const puff = new THREE.Mesh(new THREE.SphereGeometry(70 + Math.random() * 45, 8, 6), cloudMat);
      puff.position.set(j * 90 - puffs * 40, Math.random() * 30, Math.random() * 40);
      cloud.add(puff);
    }
    const angle = (i / 12) * Math.PI * 2 + 0.2;
    cloud.position.set(Math.sin(angle) * 2100, 1100 + (i % 4) * 90, Math.cos(angle) * 2100);
    scene.add(cloud);
  }

  // Sun disc.
  const sunCanvas = document.createElement('canvas');
  sunCanvas.width = 256;
  sunCanvas.height = 256;
  const sunCtx = sunCanvas.getContext('2d')!;
  const sunGradient = sunCtx.createRadialGradient(128, 128, 10, 128, 128, 128);
  sunGradient.addColorStop(0, 'rgba(255, 250, 220, 1)');
  sunGradient.addColorStop(0.32, 'rgba(255, 236, 150, 0.9)');
  sunGradient.addColorStop(0.65, 'rgba(255, 200, 80, 0.25)');
  sunGradient.addColorStop(1, 'rgba(255, 180, 40, 0)');
  sunCtx.fillStyle = sunGradient;
  sunCtx.fillRect(0, 0, 256, 256);
  const sunTexture = new THREE.CanvasTexture(sunCanvas);
  sunTexture.colorSpace = THREE.SRGBColorSpace;
  const sunSprite = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: sunTexture,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      depthTest: false,
      transparent: true,
    }),
  );
  sunSprite.scale.set(520, 520, 1);
  sunSprite.position.set(-2300, 2100, -2800);
  sunSprite.renderOrder = -1;
  scene.add(sunSprite);

  collision.finalize();

  const starBaseY = 820;

  return {
    collision,
    collisionMeshes,
    spawn: { pos: [0, 40, 80], faceYaw: 0x8000 },
    coins,
    star: starData.group,
    starLight: starData.light,
    update(time: number) {
      for (const coin of coins) {
        if (coin.collected) continue;
        if (!skin) coin.group.rotation.y = time * 2.6;
        coin.group.position.y = coin.position.y + Math.sin(time * 2.2 + coin.position.x) * 5;
      }
      const bob = Math.sin(time * 1.6) * 14;
      starData.group.position.y = starBaseY + bob;
      starData.group.rotation.y = time * 1.25;
      starData.group.rotation.z = 0.08 + Math.sin(time * 0.9) * 0.06;
      starData.light.intensity = 230 + Math.sin(time * 3.1) * 55;
    },
  };
}

/** A gradient sky dome, kept out of the collision world. */
export function addSky(scene: THREE.Scene, radius = 5200): void {
  const geometry = new THREE.SphereGeometry(radius, 32, 16);
  const material = new THREE.ShaderMaterial({
    side: THREE.BackSide,
    depthWrite: false,
    uniforms: {
      topColor: { value: new THREE.Color(0x3f8fd6) },
      midColor: { value: new THREE.Color(0x9ed4f5) },
      bottomColor: { value: new THREE.Color(0xe8f6ff) },
    },
    vertexShader: /* glsl */ `
      varying vec3 vWorldPosition;
      void main() {
        vec4 worldPosition = modelMatrix * vec4(position, 1.0);
        vWorldPosition = worldPosition.xyz;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform vec3 topColor;
      uniform vec3 midColor;
      uniform vec3 bottomColor;
      varying vec3 vWorldPosition;
      void main() {
        float h = normalize(vWorldPosition).y;
        vec3 color = mix(bottomColor, midColor, smoothstep(-0.05, 0.22, h));
        color = mix(color, topColor, smoothstep(0.2, 0.75, h));
        gl_FragColor = vec4(color, 1.0);
      }
    `,
  });
  const sky = new THREE.Mesh(geometry, material);
  sky.frustumCulled = false;
  scene.add(sky);
}
