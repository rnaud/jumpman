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
  await page.click('.character-picker input[value=link]');
  await page.waitForFunction(() => window.__JUMPMAN__?.character?.clipNames?.includes('slash'), {timeout: 90000});
  await page.click('#start-button');
  assert.equal(await page.evaluate(() => window.__JUMPMAN__.debugState.character), 'link');
  if (process.argv.includes('--menu-only')) {
    assert.deepEqual(errors, []);
    console.log('Started Bob-omb Battlefield as Adult Link from the main menu.');
  } else {
    const result = await page.evaluate(async () => {
      const g = window.__JUMPMAN__, m = g.mario, view = g.character;
      const { ACT_IDLE, ACT_FREEFALL, A_BUTTON, B_BUTTON, Z_TRIG } = await import('/src/sim/state.ts');
      m.setAction(ACT_IDLE);
      m.pos[1] = m.floorHeight;
      const tick = (stickY = 0, down = 0, pressed = 0) => {
        m.step({stickX: 0, stickY, stickMag: Math.abs(stickY), buttonDown: down, buttonPressed: pressed}, 0, true);
        view.update(m, 1/30, g.renderPos.fromArray(m.pos), m.faceAngle[1]);
        return view.activeClip;
      };
      const idle = tick();
      const [x,y,z] = m.pos;
      const angle = m.faceAngle[1] * Math.PI/32768 + 0.35;
      g.camera.position.set(x+Math.sin(angle)*620,y+145,z+Math.cos(angle)*620);
      g.camera.lookAt(x,y+100,z);
      g.renderer.render(g.scene, g.camera);
      const image = g.renderer.domElement.toDataURL();
      const roll = tick(64, A_BUTTON, A_BUTTON);
      for (let i = 0; i < 18; i++) tick();
      const slash = tick(0,B_BUTTON,B_BUTTON);
      for (let i = 0; i < 4; i++) tick();
      tick(0,B_BUTTON,B_BUTTON);
      const combo = [];
      for (let i = 0; i < 32; i++) combo.push(tick());
      const finisher = combo.includes('finish') && combo.includes('finishRecover') && combo.at(-1) === 'idle';
      const guard = tick(0,Z_TRIG);
      m.invincTimer = 0;
      const frontHit = m.takeDamage(1,[m.pos[0]+Math.sin(m.faceAngle[1]*Math.PI/32768)*100,m.pos[1],m.pos[2]+Math.cos(m.faceAngle[1]*Math.PI/32768)*100]);
      tick();
      const { CollisionWorld } = await import('/src/sim/collision.ts');
      const { MarioRuntime } = await import('/src/sim/MarioRuntime.ts');
      const world = new CollisionWorld();
      for (const [radius,height] of [[1800,-400],[180,0]]) {
        world.addQuad([-radius,height,-radius],[-radius,height,radius],[radius,height,radius],[radius,height,-radius]);
      }
      world.finalize();
      const walker = new MarioRuntime(world, {pos:[0,0,0],yaw:0});
      for (let i = 0; i < 20 && walker.action !== ACT_FREEFALL; i++) {
        walker.step({stickX:0, stickY:-64, stickMag:64, buttonDown:0, buttonPressed:0}, 0, true);
      }
      const jumped = walker.action === ACT_FREEFALL && walker.vel[1] > 0;
      walker.setAction(ACT_IDLE);
      walker.pos = [0,0,0];
      walker.step({stickX:0,stickY:64,stickMag:64,buttonPressed:A_BUTTON,buttonDown:A_BUTTON,focusYaw:0},0,true);
      view.update(walker,1/30,g.renderPos,0);
      const dodge = view.activeClip;
      const goomba = g.playground.goombas.find(enemy => enemy.tangible);
      const p = goomba.model.position;
      g.teleport3(p.x, p.y, p.z - 320, 0);
      window.dispatchEvent(new KeyboardEvent('keydown',{code:'KeyL'}));
      g.stepSimulation();
      const marker = g.linkFocus.marker;
      const locked = marker.visible && Math.abs(marker.position.x-p.x) < 1 && Math.abs(marker.position.z-p.z) < 1;
      g.renderPos.fromArray(m.pos);
      for (let i = 0; i < 40; i++) g.updateViewCamera(1/30);
      g.camera.updateMatrixWorld(true);
      const framed = [p, g.renderPos].every(pos => {
        const ndc = pos.clone().project(g.camera);
        return Math.abs(ndc.x) < 1 && Math.abs(ndc.y) < 1 && Math.abs(ndc.z) < 1;
      });
      window.dispatchEvent(new KeyboardEvent('keyup',{code:'KeyL'}));
      g.stepSimulation();
      const released = !marker.visible;
      return {idle,roll,slash,guard,frontHit,jumped,dodge,finisher,locked,framed,released,image};
    });
    if (process.env.SCREENSHOT) await writeFile(process.env.SCREENSHOT, Buffer.from(result.image.split(',')[1], 'base64'));
    delete result.image;
    console.log(result);
    assert.deepEqual(result,{idle:'idle',roll:'roll',slash:'slash',guard:'guard',frontHit:false,jumped:true,dodge:'backflip',finisher:true,locked:true,framed:true,released:true});
    assert.deepEqual(errors, []);
  }
} finally { await browser.close(); }
