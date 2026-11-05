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

    editor.remove(id);
    expect(editor.debug()).toHaveLength(0);
    expect(world.findFloor(0, 100, 0).surface).toBeNull();
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
