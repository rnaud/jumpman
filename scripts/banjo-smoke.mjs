import assert from 'node:assert/strict';
import puppeteer from 'puppeteer-core';
import { writeFile } from 'node:fs/promises';

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  args: ['--no-sandbox', '--enable-unsafe-swiftshader'],
});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(new URL('?level=bob', process.env.URL || 'http://localhost:5173').href, {waitUntil: 'domcontentloaded', timeout: 90000});
  await page.waitForSelector('.character-picker:not(:disabled)', {timeout: 90000});
  await page.click('.character-picker input[value=banjo]');
  await page.waitForFunction(() => window.__JUMPMAN__?.character?.clipNames?.includes('flap'), {timeout: 90000});
  await page.click('#start-button');
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.debugState.character), 'banjo');
  if (process.argv.includes('--menu-only')) {
    assert.deepEqual(errors, []);
    console.log('Started Bob-omb Battlefield as Banjo-Kazooie from the main menu.');
  } else {
    const result = await page.evaluate(async () => {
      const g = window.__JUMPMAN__, view = g.character;
      const { ACT_IDLE, ACT_FREEFALL, ACT_GROUND_POUND, A_BUTTON, Z_TRIG } = await import('/src/sim/state.ts');
      const { CollisionWorld } = await import('/src/sim/collision.ts');
      const { MarioRuntime } = await import('/src/sim/MarioRuntime.ts');
      const world = new CollisionWorld();
      world.addQuad([-4000,0,-4000],[-4000,0,4000],[4000,0,4000],[4000,0,-4000]);
      world.finalize();
      const m = new MarioRuntime(world, {pos:[0,0,0],yaw:0});
      const tick = (stickY = 0, down = 0, pressed = 0) => {
        m.step({stickX: 0, stickY, stickMag: Math.abs(stickY), buttonDown: down, buttonPressed: pressed}, 0, false, true);
        view.update(m, 1/30, g.renderPos.fromArray(m.pos), m.faceAngle[1]);
        return view.activeClip;
      };
      m.setAction(ACT_IDLE);
      const idle = tick();
      let walk = '';
      for (let i = 0; i < 40; i++) walk = tick(64);
      let jump = tick(64, A_BUTTON, A_BUTTON);
      for (let i = 0; i < 3; i++) jump = tick(64, A_BUTTON);
      // Release, let the jump peak, then press and hold A for Kazooie's Feathery Flap.
      let peak = 0;
      while (m.vel[1] > 2 && peak++ < 60) tick(64);
      const flap = tick(64, A_BUTTON, A_BUTTON);
      const flapStartY = m.pos[1];
      for (let i = 0; i < 30; i++) tick(64, A_BUTTON);
      const hovered = m.action === ACT_FREEFALL && m.pos[1] > flapStartY - 60;
      const bust = (tick(0, Z_TRIG, Z_TRIG), m.action === ACT_GROUND_POUND ? view.activeClip : 'none');
      for (let i = 0; i < 90 && m.action !== ACT_IDLE; i++) tick();
      const landed = m.action === ACT_IDLE;
      const [x,y,z] = g.mario.pos;
      const angle = g.mario.faceAngle[1] * Math.PI/32768 + 0.35;
      g.camera.position.set(x+Math.sin(angle)*520,y+130,z+Math.cos(angle)*520);
      g.camera.lookAt(x,y+90,z);
      view.update(g.mario, 1/30, g.renderPos.fromArray(g.mario.pos), g.mario.faceAngle[1]);
      g.renderer.render(g.scene, g.camera);
      const image = g.renderer.domElement.toDataURL();
      return {idle, walk, jump, flap, hovered, bust, landed, image};
    });
    if (process.env.SCREENSHOT) await writeFile(process.env.SCREENSHOT, Buffer.from(result.image.split(',')[1], 'base64'));
    delete result.image;
    console.log(result);
    assert.deepEqual(result, {idle: 'idle', walk: 'run', jump: 'jump', flap: 'flap', hovered: true, bust: 'bust', landed: true});

    // Banjo stays the playable character through every course.
    const courses = await page.evaluate(async () => Object.keys((await import('/src/game/courses.ts')).COURSES));
    const visited = [];
    for (const id of courses) {
      const ok = await page.evaluate(async id => {
        const g = window.__JUMPMAN__;
        await g.loadCourse(id);
        for (let i = 0; i < 10; i++) g.stepSimulation();
        const s = g.debugState;
        return s.character === 'banjo' && s.world === id && g.character.root.visible;
      }, id);
      assert.ok(ok, `Banjo is not playable in ${id}`);
      visited.push(id);
    }
    assert.deepEqual(errors, []);
    console.log(`Banjo-Kazooie played ${visited.length} courses: ${visited.join(', ')}`);
  }
} finally { await browser.close(); }
