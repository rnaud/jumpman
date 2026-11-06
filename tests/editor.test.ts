import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { RuntimeEditor } from '../src/game/editor';

function canvas(): HTMLCanvasElement {
  return { getBoundingClientRect: () => ({ left: 0, top: 0, width: 1280, height: 720 }) } as HTMLCanvasElement;
}

describe('runtime world editor', () => {
  it('spawns and removes a block with synchronized collision', () => {
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera();
    const world = new CollisionWorld();
    const editor = new RuntimeEditor(scene, camera, canvas(), [], world);
    editor.enabled = true;

    const id = editor.spawn(new THREE.Vector3(0, 0, 0));
    expect(editor.debug()).toHaveLength(1);
    const floor = world.findFloor(0, 100, 0);
    expect(floor.surface).not.toBeNull();
    expect(floor.height).toBeCloseTo(36);
    expect(world.f32FindWallCollision([125, 18, 0], 18, 10)).toBeGreaterThan(0);

    editor.remove(id);
    expect(editor.debug()).toHaveLength(0);
    expect(world.findFloor(0, 100, 0).surface).toBeNull();
    editor.dispose();
  });

  it('persists blocks per storage key', () => {
    const store = new Map<string, string>();
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => store.get(key) ?? null,
        setItem: (key: string, value: string) => { store.set(key, value); },
        removeItem: (key: string) => { store.delete(key); },
        clear: () => { store.clear(); },
        key: () => null,
        length: 0,
      } satisfies Storage,
    });

    const scene = new THREE.Scene();
    const editor = new RuntimeEditor(scene, new THREE.PerspectiveCamera(), canvas(), [], new CollisionWorld(), 'test-editor');
    editor.spawn(new THREE.Vector3(0, 0, 0));
    expect(store.size).toBe(1);
    editor.dispose();

    const restored = new RuntimeEditor(scene, new THREE.PerspectiveCamera(), canvas(), [], new CollisionWorld(), 'test-editor');
    expect(restored.debug()).toHaveLength(1);
    restored.dispose();
  });

  it('spawns ramps with a sloped collision surface', () => {
    const scene = new THREE.Scene();
    const world = new CollisionWorld();
    const editor = new RuntimeEditor(scene, new THREE.PerspectiveCamera(), canvas(), [], world);
    editor.enabled = true;
    editor.selectedType = 'ramp';
    editor.spawn(new THREE.Vector3(0, 0, 0));
    expect(editor.debug()[0].type).toBe('ramp');
    const back = world.findFloor(0, 100, -130);
    const front = world.findFloor(0, 100, 130);
    expect(back.surface).not.toBeNull();
    expect(front.surface).not.toBeNull();
    expect(back.height).toBeGreaterThan(front.height + 10);
    editor.dispose();
  });

  it('marks bounce-platform surfaces for the game bounce check', () => {
    const scene = new THREE.Scene();
    const world = new CollisionWorld();
    const editor = new RuntimeEditor(scene, new THREE.PerspectiveCamera(), canvas(), [], world);
    editor.enabled = true;
    editor.selectedType = 'bounce';
    editor.spawn(new THREE.Vector3(0, 0, 0));
    expect(editor.debug()[0].type).toBe('bounce');
    const floor = world.findFloor(0, 100, 0);
    expect(editor.isBounceSurface(floor.surface)).toBe(true);
    editor.dispose();
  });

  it('picks and drags an actor target', () => {
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(55, 16 / 9, 0.1, 1000);
    camera.position.set(0, 0, 100);
    camera.lookAt(0, 0, 0);
    camera.updateMatrixWorld();
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(20, 20, 20));
    scene.add(mesh);
    const editor = new RuntimeEditor(scene, camera, canvas(), [], new CollisionWorld());
    editor.enabled = true;
    let moved = 0;
    editor.actorProvider = () => [{
      id: 'actor-1',
      mesh,
      position: new THREE.Vector3(0, 0, 0),
      move: () => { moved++; },
    }];

    const down = { clientX: 640, clientY: 360, button: 0 } as PointerEvent;
    expect(editor.pointerDown(down)).toBe(true);
    const move = { clientX: 740, clientY: 360, button: 0 } as PointerEvent;
    expect(editor.pointerMove(move)).toBe(true);
    expect(moved).toBeGreaterThan(0);
    editor.pointerUp();
    editor.dispose();
  });

  it('undoes and redoes block operations', () => {
    const scene = new THREE.Scene();
    const editor = new RuntimeEditor(scene, new THREE.PerspectiveCamera(), canvas(), [], new CollisionWorld());
    editor.enabled = true;
    editor.spawn(new THREE.Vector3(0, 0, 0));
    const size = editor.debug()[0].size[0];
    editor.wheel(120);
    expect(editor.debug()[0].size[0]).toBeGreaterThan(size);
    expect(editor.undo()).toBe(true);
    expect(editor.debug()[0].size[0]).toBe(size);
    expect(editor.redo()).toBe(true);
    expect(editor.debug()[0].size[0]).toBeGreaterThan(size);
    editor.dispose();
  });

  it('resizes the selected block', () => {
    const scene = new THREE.Scene();
    const editor = new RuntimeEditor(scene, new THREE.PerspectiveCamera(), canvas(), [], new CollisionWorld());
    editor.enabled = true;
    editor.spawn(new THREE.Vector3(0, 0, 0));
    expect(editor.wheel(120)).toBe(true);
    expect(editor.debug()[0].size[0]).toBeGreaterThan(260);
    editor.dispose();
  });
});
