import { existsSync, readFileSync } from 'node:fs';
import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { chooseSm64Clip, SM64_ANIM, sm64ClipName } from '../src/game/sm64Animation';
import { sourceUnitScale, sm64AssetPaths, sm64AssetUrl, SM64_MODELS, Sm64Billboards } from '../src/game/sm64Assets';
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
    expect(sm64AssetPaths.length).toBe(957);
    for (const path of Object.values(SM64_MODELS)) expect(sm64AssetUrl(path)).toContain('.glb');
    expect(() => sm64AssetUrl('../secret')).toThrow('SM64 asset missing');
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
    expect(chooseSm64Clip(state).name).toBe('anim_C5');
    expect(chooseSm64Clip({...state,action:A.ACT_WALKING,forwardVel:32})).toEqual({name:'anim_72',speed:8});
    expect(chooseSm64Clip({...state,action:A.ACT_DOUBLE_JUMP,vel:[0,20,0]}).name).toBe('anim_50');
    expect(chooseSm64Clip({...state,action:A.ACT_DOUBLE_JUMP,vel:[0,-20,0]}).name).toBe('anim_4C');
    expect(chooseSm64Clip({...state,action:A.ACT_GROUND_POUND,actionState:1}).name).toBe('anim_3D');
    expect(mario.animations.find(a=>a.name===chooseSm64Clip({...state,action:A.ACT_LONG_JUMP}).name).extras.loop).toBe(false);
  });

  it('faces billboard nodes toward the camera without changing the scale correction', () => {
    const root = new THREE.Group(); root.rotation.y=1.2;root.scale.setScalar(100);
    const billboard = new THREE.Group();billboard.userData.sm64Command='GEO_BILLBOARD';billboard.scale.setScalar(4);root.add(billboard);
    const camera=new THREE.PerspectiveCamera();camera.rotation.set(.2,.6,0);
    new Sm64Billboards(root).update(camera);
    const actual=billboard.getWorldQuaternion(new THREE.Quaternion());
    expect(actual.angleTo(camera.getWorldQuaternion(new THREE.Quaternion()))).toBeLessThan(1e-6);
    expect(billboard.scale.x).toBe(4);
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
