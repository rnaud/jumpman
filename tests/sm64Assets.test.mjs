import { existsSync, readFileSync } from 'node:fs';
import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { chooseSm64Clip, SM64_ANIM, sm64ClipName } from '../src/game/sm64Animation';
import { instantiateSm64, sourceUnitScale, sm64AssetPaths, sm64AssetUrl, SM64_MODELS, Sm64Billboards } from '../src/game/sm64Assets';
import * as A from '../src/sim/state';
import { CollisionWorld } from '../src/sim/collision';
import { addMeshCollision } from '../src/game/world';

function readGlb(path) {
  const bytes = readFileSync(new URL(`../assets/${path}`, import.meta.url));
  const size = bytes.readUInt32LE(12);
  return { doc: JSON.parse(bytes.subarray(20,20+size).toString()), bin: bytes.subarray(28+size) };
}
const hasAssets = existsSync(new URL('../assets/actors/mario/mario_geo.glb', import.meta.url));
const mario = hasAssets ? readGlb(SM64_MODELS.mario).doc : null;

describe.skipIf(!hasAssets)('native SM64 assets', () => {
  it('indexes the copied library and resolves the models used by both worlds', () => {
    // Validate runtime dependencies, rather than an unrelated exporter's total count.
    for (const id of ['bob', 'castle_grounds', 'castle_inside', 'wf', 'pss']) {
      const course = JSON.parse(readFileSync(new URL(`../assets/courses/${id}.json`, import.meta.url)));
      for (const path of [course.model, ...course.objects.map(object => object.model)]) {
        expect(sm64AssetPaths).toContain(path);
        const { doc } = readGlb(path);
        expect(doc.meshes.length).toBeGreaterThan(0);
        expect(doc.images.length).toBeGreaterThan(0);
        expect(doc.images.every(image => image.bufferView !== undefined && image.mimeType === 'image/png')).toBe(true);
      }
    }
    for (const path of Object.values(SM64_MODELS)) expect(sm64AssetUrl(path)).toContain('.glb');
    expect(() => sm64AssetUrl('../secret')).toThrow('SM64 asset missing');
  });

  it('exports the interactive Bob-ombs and boxes placed by Bob-omb Battlefield macros', () => {
    const course = JSON.parse(readFileSync(new URL('../assets/courses/bob.json', import.meta.url)));
    const interactive = course.objects.filter(object => object.behavior);
    expect(interactive.filter(object => object.behavior === 'bobomb')).toHaveLength(12);
    expect(interactive.filter(object => object.behavior === 'breakableBox')).toHaveLength(3);
    expect(interactive.filter(object => object.behavior === 'breakableBoxSmall')).toHaveLength(2);
    expect(interactive.filter(object => object.behavior === 'chainChomp')).toHaveLength(1);
    expect(interactive.filter(object => object.behavior === 'woodenPost')).toHaveLength(4);
    expect(interactive.filter(object => object.behavior === 'chainChompGate')).toHaveLength(1);
    expect(interactive.filter(object => object.behavior === 'purpleSwitch')).toHaveLength(1);
    expect(interactive.filter(object => object.behavior === 'openableGrill')).toHaveLength(1);
    expect(interactive.filter(object => object.behavior === 'pitBowlingBall')).toHaveLength(2);
    expect(interactive.filter(object => object.behavior === 'kingBobomb')).toHaveLength(1);
    for (const path of ['actors/chain_ball/metallic_ball_geo.glb', 'actors/poundable_pole/wooden_post_geo.glb']) {
      expect(sm64AssetPaths).toContain(path);
    }
    for (const object of interactive) {
      expect(sm64AssetPaths).toContain(object.model);
      const { doc } = readGlb(object.model);
      expect(doc.meshes.length).toBeGreaterThan(0);
    }
  });

  it('keeps the cap logo material identical across eye-switch alternatives', () => {
    const logoMaterials = mario.materials.flatMap((material, index) => material.name === 'mario_texture_m_logo' ? [index] : []);
    // Blinking used to inherit the previous head's brown hair light, creating
    // a second incorrectly tinted logo image/material for alternate eye cases.
    expect(logoMaterials).toHaveLength(1);
    const uses = mario.meshes.flatMap(mesh => mesh.primitives).filter(p => p.material === logoMaterials[0]);
    expect(uses.length).toBeGreaterThanOrEqual(8);
    expect(mario.images.filter(image => image.name === 'mario_logo.rgba16.png')).toHaveLength(1);
  });

  it('restores source units without fitting Mario to his exported ledge-climb pose', () => {
    expect(sourceUnitScale(mario.asset)).toBe(100);
    expect(sourceUnitScale({version:'2.0', extras:{sm64UnitsToMeters:0.1}})).toBe(10);
    expect(() => sourceUnitScale({version:'2.0', extras:{sm64UnitsToMeters:0}})).toThrow();
  });

  it('maps every supported animation to a real native clip', () => {
    const names = new Set(mario.animations.map((clip) => clip.name));
    expect(names.size).toBe(209);
    for (const id of Object.values(SM64_ANIM)) expect(names.has(sm64ClipName(id))).toBe(true);
    const state = {action:A.ACT_IDLE,actionArg:0,actionState:0,forwardVel:0,vel:[0,0,0]};
    expect(chooseSm64Clip(state).name).toBe('anim_C3');
    expect(chooseSm64Clip({...state,action:A.ACT_WALKING,forwardVel:32})).toEqual({name:'anim_72',speed:8});
    expect(chooseSm64Clip({...state,action:A.ACT_DOUBLE_JUMP,vel:[0,20,0]}).name).toBe('anim_50');
    expect(chooseSm64Clip({...state,action:A.ACT_DOUBLE_JUMP,vel:[0,-20,0]}).name).toBe('anim_4C');
    expect(chooseSm64Clip({...state,action:A.ACT_GROUND_POUND,actionState:1}).name).toBe('anim_3D');
    expect(mario.animations.find(a=>a.name===chooseSm64Clip({...state,action:A.ACT_LONG_JUMP}).name).extras.loop).toBe(false);
  });

  it('turns real battlefield geometry into floors at the spawn and summit', () => {
    const {doc,bin}=readGlb(SM64_MODELS.battlefield);
    const root=new THREE.Group();root.scale.setScalar(sourceUnitScale(doc.asset));
    const array=(index) => {
      const a=doc.accessors[index],v=doc.bufferViews[a.bufferView];
      const components=a.type==='VEC3'?3:1;
      const offset=(v.byteOffset??0)+(a.byteOffset??0);
      return Array.from({length:a.count*components},(_,i)=>a.componentType===5126?bin.readFloatLE(offset+i*4):bin.readUInt32LE(offset+i*4));
    };
    const nodes=doc.nodes.map((node)=>{
      const object=new THREE.Group();
      if(node.translation)object.position.fromArray(node.translation);
      if(node.rotation)object.quaternion.fromArray(node.rotation);
      if(node.scale)object.scale.fromArray(node.scale);
      if(node.mesh!==undefined)for(const p of doc.meshes[node.mesh].primitives){
        const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute(array(p.attributes.POSITION),3));geometry.setIndex(array(p.indices));object.add(new THREE.Mesh(geometry));
      }
      return object;
    });
    doc.nodes.forEach((node,i)=>node.children?.forEach(c=>nodes[i].add(nodes[c])));
    for(const i of doc.scenes[0].nodes)root.add(nodes[i]);
    const world=new CollisionWorld();root.traverse(node=>{if(node instanceof THREE.Mesh)addMeshCollision(world,node);});world.finalize();
    expect(world.surfaces.length).toBeGreaterThan(1000);
    expect(world.findFloor(-6558,200,6464).height).toBeCloseTo(0);
    expect(world.findFloor(1636,20000,-5567).height).toBeGreaterThan(4200);
  });
});

describe('billboard orientation', () => {
  it.each([{ objectBillboard: true }, { sm64Command: 'GEO_BILLBOARD' }])(
    'orients %o toward the camera while preserving its scale', flags => {
      const root = new THREE.Group();
      root.rotation.y = 1.2;
      root.scale.setScalar(100);
      const billboard = new THREE.Group();
      Object.assign(billboard.userData, flags);
      billboard.scale.setScalar(4);
      root.add(billboard);
      const camera = new THREE.PerspectiveCamera();
      const billboards = new Sm64Billboards(root);
      for (const yaw of [0, Math.PI / 2, Math.PI, -Math.PI / 2]) {
        camera.rotation.set(0.2, yaw, 0);
        billboards.update(camera);
        const actual = billboard.getWorldQuaternion(new THREE.Quaternion());
        expect(actual.angleTo(camera.getWorldQuaternion(new THREE.Quaternion()))).toBeLessThan(1e-6);
        expect(billboard.scale.x).toBe(4);
      }
    },
  );
});


describe('native rendering rules', () => {
  it('biases native decal layers and excludes them from dynamic shadow maps', () => {
    const scene = new THREE.Group();
    const decal = new THREE.Mesh(new THREE.PlaneGeometry(), new THREE.MeshBasicMaterial());
    decal.userData = {sm64Command:'GEO_DISPLAY_LIST',sm64Args:['LAYER_TRANSPARENT_DECAL','shadow']};
    scene.add(decal);
    const instance = instantiateSm64({asset:{version:'2.0'},scene});
    const mesh = instance.children[0].children[0];
    expect(mesh.material.polygonOffset).toBe(true);
    expect(mesh.material.polygonOffsetFactor).toBeLessThan(0);
    expect(mesh.material.depthWrite).toBe(false);
    expect(mesh.castShadow).toBe(false);
  });

  it('keeps Goomba billboards at object scale after replacing the skeletal basis', () => {
    const root = new THREE.Group(); root.scale.setScalar(150);
    const scale = new THREE.Group(); scale.scale.setScalar(.25); scale.userData.sm64Command='GEO_SCALE';root.add(scale);
    const joint = new THREE.Group(); joint.rotation.set(.3,.7,1.4); scale.add(joint);
    const sprite = new THREE.Mesh(new THREE.PlaneGeometry(),new THREE.MeshBasicMaterial());
    sprite.userData.sm64Command='GEO_BILLBOARD';sprite.castShadow=sprite.receiveShadow=true;joint.add(sprite);
    const camera = new THREE.PerspectiveCamera();camera.rotation.set(.2,-.5,0);
    new Sm64Billboards(root).update(camera);
    expect(sprite.getWorldScale(new THREE.Vector3()).x).toBeCloseTo(150);
    expect(sprite.getWorldQuaternion(new THREE.Quaternion()).angleTo(camera.quaternion)).toBeLessThan(1e-6);
    expect(sprite.castShadow).toBe(false);expect(sprite.receiveShadow).toBe(false);
  });

  it.skipIf(!hasAssets)('exports coin UVs inside the image after applying half-scale texture coordinates', () => {
    const {doc} = readGlb(SM64_MODELS.coin);
    for(const mesh of doc.meshes) for(const primitive of mesh.primitives) {
      const uv=doc.accessors[primitive.attributes.TEXCOORD_0];
      expect(Math.max(...uv.max)).toBeLessThanOrEqual(1);
      expect(Math.max(...uv.max)).toBeGreaterThan(.9);
    }
  });

  it.skipIf(!hasAssets)('reattaches Goomba head and feet to the reopened animated parent', () => {
    const {doc} = readGlb(SM64_MODELS.goomba);
    const parent = doc.nodes.find(node => node.children?.some(i => doc.nodes[i].extras?.sm64Command==='GEO_BILLBOARD'));
    const children=parent.children.map(i=>doc.nodes[i]);
    expect(children.some(node=>node.extras.sm64Command==='GEO_SWITCH_CASE')).toBe(true);
    expect(children.filter(node=>node.extras.sm64Command==='GEO_ANIMATED_PART')).toHaveLength(2);
  });
});
