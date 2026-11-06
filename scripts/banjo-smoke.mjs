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
      const S = await import('/src/sim/state.ts');
      const { CollisionWorld } = await import('/src/sim/collision.ts');
      const { MarioRuntime } = await import('/src/sim/MarioRuntime.ts');
      const world = new CollisionWorld();
      world.addQuad([-8000,0,-8000],[-8000,0,8000],[8000,0,8000],[8000,0,-8000]);
      world.finalize();
      const m = new MarioRuntime(world, {pos:[0,0,0],yaw:0});
      const A = S.A_BUTTON, B = S.B_BUTTON, Z = S.Z_TRIG;
      const tick = (stickY = 0, down = 0, pressed = 0) => {
        m.step({stickX: 0, stickY, stickMag: Math.abs(stickY), buttonDown: down, buttonPressed: pressed}, 0, false, true);
        view.update(m, 1/30, g.renderPos.fromArray(m.pos), m.faceAngle[1]);
        return view.activeClip;
      };
      const reset = () => { m.setAction(S.ACT_IDLE); m.pos = [0,0,0]; m.vel = [0,0,0]; m.forwardVel = 0; m.banjoFlap = 0; tick(); };
      const settle = () => { for (let i = 0; i < 120 && !(m.action === S.ACT_IDLE && m.pos[1] === 0); i++) tick(); return m.action === S.ACT_IDLE; };
      const out = {};
      reset();
      out.idle = view.activeClip;

      // Jump: holding A rides bsjump's lighter gravity higher than a tap.
      const jumpHeight = held => {
        reset();
        tick(0, A, A);
        let peak = 0;
        for (let i = 0; i < 60 && m.action & S.ACT_FLAG_AIR; i++) { tick(0, held ? A : 0); peak = Math.max(peak, m.pos[1]); }
        return Math.round(peak);
      };
      const tap = jumpHeight(false), hold = jumpHeight(true);
      out.variableJump = hold > tap * 1.6 && hold > 180 && hold < 320;

      // Feathery Flap from the jump's peak.
      reset();
      tick(0, A, A);
      while (m.vel[1] > 2) tick(0, A);
      out.flap = tick(0, A, A);
      const flapY = m.pos[1];
      for (let i = 0; i < 30; i++) tick(0, A);
      out.flapHovers = m.pos[1] > flapY - 60;
      tick(0, Z, Z);
      out.bust = m.action === S.ACT_GROUND_POUND ? view.activeClip : 'none';
      out.bustLands = settle();

      // Flap Flip: crouch (Z) then A, much higher than a jump.
      reset();
      tick(0, Z, Z); tick(0, Z); tick(0, Z);
      tick(0, Z | A, A);
      let flipPeak = 0;
      for (let i = 0; i < 90 && (m.action & S.ACT_FLAG_AIR || i < 6); i++) { const clip = tick(0, A); if (i === 10) out.flip = clip; flipPeak = Math.max(flipPeak, m.pos[1]); }
      out.flipHigher = flipPeak > hold * 1.5;
      settle();

      // Claw Swipe standing, Forward Roll running.
      reset();
      tick(0, B, B);
      let punched = false;
      for (let i = 0; i < 20; i++) { tick(); punched ||= (m.flags & S.MARIO_PUNCHING) !== 0; }
      out.claw = view.activeClip;
      out.clawHits = punched;
      out.clawEnds = settle();
      reset();
      for (let i = 0; i < 30; i++) tick(64);
      tick(64, B, B);
      let rollSpeed = 0;
      for (let i = 0; i < 10; i++) { tick(64); rollSpeed = Math.max(rollSpeed, m.forwardVel); }
      out.roll = view.activeClip;
      out.rollFast = rollSpeed >= 24 && (m.flags & S.MARIO_TRIPPING) !== 0;
      settle();

      // Beak Barge: crouch then B, holding B for the full 850 charge.
      reset();
      tick(0, Z, Z); tick(0, Z);
      tick(0, Z | B, B);
      let bargeSpeed = 0;
      for (let i = 0; i < 10; i++) { tick(0, B); bargeSpeed = Math.max(bargeSpeed, m.forwardVel); }
      out.barge = view.activeClip;
      out.bargeFast = bargeSpeed > 34;
      out.bargeEnds = settle();

      // Rat-a-tat Rap: B in the air hovers with Kazooie's pecks.
      reset();
      tick(0, A, A);
      for (let i = 0; i < 8; i++) tick(0, A);
      tick(0, 0, B);
      const peckY = m.pos[1];
      let kicking = false;
      for (let i = 0; i < 12; i++) { tick(); kicking ||= (m.flags & S.MARIO_KICKING) !== 0; }
      out.peck = view.activeClip;
      out.peckHovers = kicking && m.pos[1] > peckY - 40;
      out.peckLands = settle();

      // Claw a real Battlefield Goomba.
      const goomba = g.playground.goombas.find(enemy => enemy.tangible);
      const p = goomba.model.position;
      g.teleport3(p.x, p.y + 5, p.z - 90, 0);
      for (let i = 0; i < 3; i++) g.stepSimulation();
      const yaw = Math.atan2(p.x - g.mario.pos[0], p.z - g.mario.pos[2]) * 32768 / Math.PI;
      g.mario.faceAngle[1] = Math.round(yaw);
      window.dispatchEvent(new KeyboardEvent('keydown', {code: 'KeyJ'}));
      g.stepSimulation();
      window.dispatchEvent(new KeyboardEvent('keyup', {code: 'KeyJ'}));
      for (let i = 0; i < 12 && goomba.tangible; i++) g.stepSimulation();
      out.goombaClawed = !goomba.tangible;

      const [x,y,z] = g.mario.pos;
      const angle = g.mario.faceAngle[1] * Math.PI/32768 + 0.35;
      g.camera.position.set(x+Math.sin(angle)*520,y+130,z+Math.cos(angle)*520);
      g.camera.lookAt(x,y+90,z);
      g.renderer.render(g.scene, g.camera);
      out.image = g.renderer.domElement.toDataURL();
      return out;
    });
    if (process.env.SCREENSHOT) await writeFile(process.env.SCREENSHOT, Buffer.from(result.image.split(',')[1], 'base64'));
    delete result.image;
    console.log(result);
    assert.deepEqual(result, {
      idle: 'idle', variableJump: true, flap: 'flapEnter', flapHovers: true, bust: 'bust', bustLands: true,
      flip: 'flipEnter', flipHigher: true, claw: 'claw', clawHits: true, clawEnds: true, roll: 'roll', rollFast: true,
      barge: 'barge', bargeFast: true, bargeEnds: true, peck: 'peckEnter', peckHovers: true, peckLands: true, goombaClawed: true,
    });

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
