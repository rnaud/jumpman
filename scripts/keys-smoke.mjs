import puppeteer from 'puppeteer-core';
import assert from 'node:assert/strict';

const base = process.env.URL || 'http://127.0.0.1:5173/';
const browser = await puppeteer.launch({
  executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  headless: true,
  args: ['--no-sandbox', '--enable-unsafe-swiftshader', '--use-gl=angle', '--use-angle=swiftshader'],
});
try {
  const page = await browser.newPage();
  await page.setViewport({ width: 440, height: 320 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const enterDoor = async node => {
    await page.evaluate(id => {
      const game = window.__JUMPMAN__;
      const door = game.playground.doors.find(d => d.node === id);
      game.teleport3(door.pos[0], door.pos[1], door.pos[2] + 75, 0x8000);
    }, node);
    await page.keyboard.down('w');
  };
  for (const key of [1, 2, 3]) {
    await page.goto(new URL('?level=bowser_' + key, base).href, { waitUntil: 'networkidle0' });
    await page.click('#start-button');
    // Drive each throw with controller samples. Position Mario at the tail to isolate the fight.
    await page.evaluate(stage => {
      const g = window.__JUMPMAN__;
      const boss = g.playground.objects.manager.findByBehavior('bowser');
      const original = g.input.consumeStep;
      const tick = (x = 0, y = 0, b = false) => {
        g.input.consumeStep = () => ({stickX: x * 64, stickY: y * 64, stickMag: x || y ? 64 : 0,
          buttonDown: b ? 0x4000 : 0, buttonPressed: b ? 0x4000 : 0});
        g.stepSimulation();
      };
      try {
        for (let hit = 0; hit < (stage === 3 ? 3 : 1); hit++) {
          for (let t = 0; t < 180 && (!boss.grounded || boss.action !== 0); t++) tick();
          const yaw = boss.moveYaw * Math.PI / 32768;
          g.teleport3(boss.pos[0] - 300 * Math.sin(yaw), boss.pos[1], boss.pos[2] - 300 * Math.cos(yaw), boss.moveYaw);
          tick(0, 0, true);
          for (let t = 0; t < 24; t++) tick();
          if (g.mario.heldObj !== boss) throw Error('Tail grab failed in arena ' + stage);
          const mines = g.playground.objects.manager.objects.filter(o => o.active && o.behavior === 'bowserMine');
          const nearest = mines.sort((a,b) => Math.hypot(a.pos[0]-g.mario.pos[0],a.pos[2]-g.mario.pos[2])
            - Math.hypot(b.pos[0]-g.mario.pos[0],b.pos[2]-g.mario.pos[2]))[0];
          const target = Math.atan2(nearest.pos[0]-g.mario.pos[0],nearest.pos[2]-g.mario.pos[2]);
          for (let t = 0; t < 240; t++) {
            tick(Math.sin(t*Math.PI/2), Math.cos(t*Math.PI/2));
            const diff = target - g.mario.faceAngle[1] * Math.PI / 32768;
            if (Math.abs(g.mario.angleVel[1]) === 4096 && Math.cos(diff) > 0.981) break;
          }
          tick(0, 0, true);
          for (let t = 0; t < 180 && nearest.active; t++) tick();
          if (nearest.active) throw Error('Throw missed in arena ' + stage + ' hit ' + (hit+1));
        }
        for (let t = 0; t < 180 && boss.active && boss.action !== 3; t++) tick();
        if (boss.active) {
          g.teleport3(boss.pos[0], boss.pos[1], boss.pos[2]-350, 0);
          tick();
        }
        if (!g.missionComplete) throw Error('Missing arena reward ' + stage);
      } finally { g.input.consumeStep = original; }
    }, key);
    await page.waitForFunction(() => window.__JUMPMAN__.keyFlight === null, { timeout: 15000 });
    if (key === 3) await page.waitForFunction(() => window.__JUMPMAN__.grandStar.tangible);
    await page.evaluate(() => {
      const g = window.__JUMPMAN__;
      g.teleport3(...g.debugState.starPos);
    });
    if (key === 3) {
      await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x1909);
      const flight = await page.evaluate(() => {
        const g = window.__JUMPMAN__, phases = new Set();
        const resets = g.mario.respawns;
        for (let i=0; i<900 && !g.loadingCourse; i++) {
          phases.add(g.mario.actionArg);
          g.stepSimulation();
        }
        return { phases: [...phases], resets: g.mario.respawns - resets, warping: g.loadingCourse };
      });
      assert.deepEqual(flight, { phases: [0,1,2], resets: 0, warping: true });
      await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_grounds'
        && window.__JUMPMAN__.mario.action === 0x1918);
      const grounds = await page.evaluate(() => {
        const g = window.__JUMPMAN__, phases = new Set();
        for (let i=0; i<5000 && !g.won; i++) {
          phases.add(g.mario.actionArg);
          g.stepSimulation();
        }
        return { phases: [...phases], won: g.won, pos: g.mario.pos, saved: g.save.hasStar('bowser_3') };
      });
      assert.deepEqual(grounds.phases, Array.from({ length: 13 }, (_,i) => i));
      assert(grounds.won && grounds.saved);
      assert.equal(grounds.pos[2], -1180);
      continue;
    }
    await page.waitForFunction(() => window.__JUMPMAN__.worldName === 'castle_inside');
    assert.equal(await page.evaluate(k => window.__JUMPMAN__.save.hasKey(k), key), true);
    assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.stars), key === 1 ? 0 : 1);
    const returnArea = key === 1 ? 1 : 3;
    await page.waitForFunction(area => window.__JUMPMAN__.playground.area === area
      && window.__JUMPMAN__.worldName === 'castle_inside', {}, returnArea);
    await page.waitForFunction(() => window.__JUMPMAN__.mario.action === 0x0c400201);
    if (returnArea === 3) {
      await enterDoor('WARP_NODE_00');
      await page.waitForFunction(() => window.__JUMPMAN__.playground.area === 1);
      await page.keyboard.up('w');
    }
    await enterDoor(key === 1 ? 'WARP_NODE_05' : 'WARP_NODE_03');
    const destinationArea = key === 1 ? 3 : 2;
    await page.waitForFunction(area => window.__JUMPMAN__.playground.area === area, { timeout: 20000 }, destinationArea);
    await page.keyboard.up('w');
    await page.reload({ waitUntil: 'networkidle0' });
    assert.equal(await page.evaluate(k => window.__JUMPMAN__.save.doorUnlocked(k), key), true);
    assert.equal(await page.evaluate(k => window.__JUMPMAN__.save.hasKey(k), key), false);
    const destination = key === 1 ? 'hmc' : 'wdw';
    await page.evaluate(id => {
      const g = window.__JUMPMAN__;
      const index = Number(Object.keys(g.playground.paintings).find(i => g.playground.paintings[i] === id));
      const floor = g.playground.collision.surfaces.find(s => s.type === 0xd3 + index && s.normal[1] > 0.01);
      const center = floor.v1.map((v, k) => (v + floor.v2[k] + floor.v3[k]) / 3);
      g.teleport3(center[0], center[1] + 15, center[2]);
    }, destination);
    await page.waitForFunction(id => window.__JUMPMAN__.worldName === id, {}, destination);
    if (key === 1) {
      // Collect the sixth cave star while the first act is selected.
      await page.evaluate(() => {
        const g = window.__JUMPMAN__;
        const reward = g.playground.sideStars.find(star => star.act === 6);
        const { x, y, z } = reward.group.position;
        g.teleport3(x, y - 30, z);
      });
      await page.waitForFunction(() => window.__JUMPMAN__.save.hasStar('hmc', 6));
      assert.equal(await page.evaluate(() => window.__JUMPMAN__.save.hasStar('hmc', 1)), false);
    }
  }
  assert.deepEqual(errors, []);
  console.log('PASS: all three Bowser fights, both key rewards, castle return nodes and permanent key doors');
} finally {
  await browser.close();
}
