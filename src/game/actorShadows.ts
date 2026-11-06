import * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import circle from '../../assets/extracted/textures/segment2/shadow_quarter_circle.ia8.png?url';
import square from '../../assets/extracted/textures/segment2/shadow_quarter_square.ia8.png?url';

interface Shadow {
  model: THREE.Group;
  mesh: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;
  size: number;
  solidity: number;
}

// GEO_SHADOW is a projected texture on the floor under the actor (shadow.c).
export class ActorShadows {
  private readonly entries = new Map<THREE.Group,Shadow>();
  private readonly geometry = new THREE.PlaneGeometry(1,1);
  private readonly maps = new Map<string,THREE.Texture>();
  private readonly up = new THREE.Vector3(0,1,0);
  private readonly normal = new THREE.Vector3();

  constructor(private root: THREE.Group, private world: CollisionWorld) {
    this.geometry.rotateX(-Math.PI/2);
  }

  register(model: THREE.Group): void {
    let args: string[] | undefined;
    model.traverse(node => {
      if (!args && node.userData.sm64Command === 'GEO_SHADOW') args = node.userData.sm64Args;
    });
    if (!args) return;
    const url = String(args[0]).includes('SQUARE') ? square : circle;
    let map = this.maps.get(url);
    if (!map) {
      map = new THREE.TextureLoader().load(url);
      map.wrapS = map.wrapT = THREE.MirroredRepeatWrapping;
      map.flipY = false;
      map.repeat.set(2,2);
      map.offset.set(-1,-1);
      this.maps.set(url,map);
    }
    const size = Number(args[2]), solidity = Number(args[1]);
    if (!size || !solidity) return;
    const material = new THREE.MeshBasicMaterial({map,color:0,transparent:true,depthWrite:false,
      polygonOffset:true,polygonOffsetFactor:-2,polygonOffsetUnits:-2});
    const mesh = new THREE.Mesh(this.geometry,material);
    mesh.name = 'shadow-'+model.name;
    mesh.renderOrder = 1;
    this.root.add(mesh);
    model.traverse(n => { if (n instanceof THREE.Mesh) n.castShadow = false; });
    this.entries.set(model,{model,mesh,size,solidity});
  }

  remove(model: THREE.Group): void {
    const shadow = this.entries.get(model);
    if (!shadow) return;
    shadow.mesh.removeFromParent();
    shadow.mesh.material.dispose();
    this.entries.delete(model);
  }

  update(): void {
    for (const {model,mesh,size,solidity} of this.entries.values()) {
      mesh.visible = model.visible;
      if (!model.visible) continue;
      const {x,y,z} = model.position;
      const floor = this.world.findFloor(x,y,z);
      const water = this.world.waterLevel(x,z);
      const onWater = water > floor.height && water <= y;
      if (!onWater && !floor.surface) { mesh.visible = false; continue; }
      const height = onWater ? water : floor.height;
      const dist = THREE.MathUtils.clamp((y-height)/600,0,1);
      mesh.position.set(x,height+1,z);
      this.normal.copy(this.up);
      if (!onWater && floor.surface) this.normal.fromArray(floor.surface.normal);
      mesh.quaternion.setFromUnitVectors(this.up,this.normal);
      mesh.scale.setScalar(size * model.scale.x/100 * (1-0.5*dist));
      mesh.material.opacity = (solidity-Math.max(0,solidity-120)*dist)/255;
    }
  }

  dispose(): void {
    for (const model of this.entries.keys()) this.remove(model);
    for (const texture of this.maps.values()) texture.dispose();
    this.geometry.dispose();
  }
}
