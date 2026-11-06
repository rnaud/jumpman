import { PerspectiveCamera, Vector3 } from 'three';
import { describe, expect, it } from 'vitest';
import { NpcDialogCamera } from '../src/game/NpcDialogCamera';
import { CollisionWorld } from '../src/sim/collision';

describe('King dialogue camera', () => {
  it.each([-1,1])('keeps the entering shoulder on side %i and returns to the boss framing', side => {
    const camera = new PerspectiveCamera();
    const before = new Vector3(side*600,350,0);
    const focus = new Vector3(0,150,100);
    camera.position.copy(before);
    const shot = new NpcDialogCamera(camera,focus,[0,0,0],[0,0,400],300);
    for (let i=0;i<180;i++) shot.step(camera,[0,0,0]);
    expect(camera.position.x*side).toBeCloseTo(180*Math.SQRT1_2,-1);
    expect(camera.position.z).toBeCloseTo(-180*Math.SQRT1_2,-1);
    const target = new Vector3(0,185,120);
    const direction = camera.getWorldDirection(new Vector3());
    expect(direction.dot(target.sub(camera.position).normalize())).toBeGreaterThan(0.99999);
    const world = new CollisionWorld();
    shot.beginReturn(camera);
    for (let i=0;i<15;i++) shot.stepReturn(camera,world);
    expect(shot.returnFrames).toBe(0);
    expect(camera.position.distanceTo(before)).toBeLessThan(2);
    expect(camera.getWorldDirection(direction).dot(focus.clone().sub(before).normalize())).toBeGreaterThan(0.99999);
  });
});
